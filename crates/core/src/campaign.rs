//! The **campaign** (§14 v3 / §2.2): the run, as a forward sequence of facilities.
//!
//! Everything below this module plays *one* facility. [`State`](crate::State) is the
//! turn loop of a single raid (§4.2) and [`LevelSeed`] is that raid's reproducible
//! config (§12.4) — neither knows there is anything after it. §2.2 says the run is
//! the campaign, so something has to own the layer above: the order the facilities
//! come in, what the player carries between them, and the four transitions that move
//! a run through it.
//!
//! # What carries, and what does not
//!
//! §2.2's table is the whole specification, and it cuts both ways:
//!
//! | | Within a run | Across runs |
//! |---|---|---|
//! | Salvaged tech | **accumulates** ([`loadout`](Campaign::loadout)) | nothing carries |
//! | Intel | **accumulates and is spent** ([`intel`](Campaign::intel)) | nothing carries |
//! | Alert | **carries and scales** ([`alert`](Campaign::alert)) — #210, see below | nothing carries |
//!
//! "Across runs nothing carries" needs no code at all: a [`Campaign`] is a plain
//! value with no persistence behind it, so a lost run is a dropped value and the
//! next one is built from scratch. What *does* need code is the within-a-run half,
//! and it is these three fields, carried from the day the layer exists rather than
//! retrofitted. Later tickets give them their behaviour: caches fill the loadout
//! (#209) and the intel epic spends the wallet (#211).
//!
//! **The alert is the exception, deliberately.** Its field is here — later tickets
//! need a place to put a run-level alert, and retrofitting one would touch every
//! transition — but **nothing writes it yet, and every facility starts at base
//! alert**. What a raid's loudness is worth, how it decays, and what a raised alert
//! does to the next facility are one decision, and it is #210's: half of it landed
//! here would be a number carried for no reason, and a campaign that got harder by an
//! accident of this ticket's arithmetic. The §7.3 ladder stays what it is — per
//! facility, and it dies with the facility.
//!
//! # A geography this ticket does not have yet
//!
//! The sequence here is **linear**: a route of [`NodeId`]s walked front to back, one
//! facility at a time, no backtracking. The real campaign map is a graph with edges
//! and a choice at every node (#207), and it grows lazily. What that ticket replaces
//! is how the route is *built* — the transitions below are the same either way, which
//! is why the seam is a node id rather than an index into a list.
//!
//! # Two meanings of "replay" (§2.2 vs §12.4)
//!
//! The player's run is one-shot: capture ends it, there is no retry and no snapshot
//! to restore ([`RunMode::exits`] refuses the campaign a way to play it again). The
//! *engine* stays deterministic all the same, and the two are not in tension. Every
//! facility's seed is derived from `(run seed, node id)` by [`facility_seed`], so a
//! whole run reproduces from its run seed and its inputs exactly as one level does
//! (§12.4) — for tests, for bug repro, and for nothing the player is ever handed.

use crate::ability::{AbilityId, Loadout};
use crate::difficulty::Difficulty;
use crate::level_seed::LevelSeed;
use crate::modifiers::{IntelGate, LevelModifiers, ModifierSources};
use crate::rng::Rng;
use crate::state::Outcome;
use crate::verdict::{Ending, RunMode, RunOptions, RunStats, Verdict};

#[cfg(test)]
mod tests;

/// A facility's **identity** within a run — the key its seed is derived from
/// ([`facility_seed`]) and the thing a route is a sequence of.
///
/// A newtype rather than a bare index because it is not one: the campaign map (#207)
/// grows a graph whose nodes are chosen from, so two runs that have visited the same
/// number of facilities need not be standing on the same one. Deriving from the
/// *identity* rather than from the position is what keeps a facility the same
/// facility however a run reached it.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct NodeId(u32);

impl NodeId {
    /// The node with this identity.
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    /// The raw identity — what [`facility_seed`] mixes.
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Separates the per-facility seed draw from every other use of the run seed, exactly
/// as the loadout draw is separated from generation (§12.4): two streams that never
/// share a position. The run seed still fixes the whole campaign (§12.4 rule 1), and
/// no facility's seed can shift because something else drew first.
const FACILITY_STREAM_SALT: u64 = 0x_FAC1_11D0_FAC1_11D0;

/// The odd stride each node id is multiplied by before it is mixed in — the golden-
/// ratio constant, so consecutive node ids land far apart in the seed space rather
/// than in a neighbourhood.
const NODE_STRIDE: u64 = 0x_9E37_79B9_7F4A_7C15;

/// The **campaign's starting facility count** — how many raids a run is, end to end.
///
/// **[START]** at six. The campaign is 2–3 hours (§14 v3) and this is the coarsest
/// knob on that length; #207 owns the real number as its depth-to-archive, and will
/// take this over along with the graph it counts nodes in.
pub const CAMPAIGN_LENGTH: usize = 6;

/// Derive a facility's seed from the run seed and the facility's identity (§12.4).
///
/// **Never a fresh source.** The whole campaign hangs off one run seed, so a run
/// reproduces from `(seed, [inputs])` even though the player never gets to replay it.
/// The result is narrowed to the level-seed token's field width
/// ([`LevelSeed::narrow_seed`]), which buys a property worth having on its own: every
/// facility of a campaign is itself a **sayable level** — a token you can hand to
/// someone, or to the sim, and play by hand (§13.1).
///
/// Public because the map (#207) derives its successors' facilities the same way, from
/// node identities it invents; there must not be a second derivation to drift from
/// this one.
pub fn facility_seed(run_seed: u64, node: NodeId) -> u64 {
    let mixed = run_seed ^ FACILITY_STREAM_SALT ^ u64::from(node.get()).wrapping_mul(NODE_STRIDE);
    LevelSeed::narrow_seed(Rng::new(mixed).next_u64())
}

/// Where a run stands — the campaign's own four-way answer, one rung coarser than the
/// turn loop's [`Outcome`] (§4.5).
///
/// The two live states are the ones the layer exists to tell apart: a player *between*
/// facilities has a campaign to act on (spend intel, choose a route) and a player
/// *inside* one has a raid to finish. The two dead states are §2.2's, and they are
/// dead for the whole run, not for the facility.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CampaignStage {
    /// Between facilities, with the next one ahead — where a run starts and where every
    /// completed raid returns it. The stage the map/hub acts in (#207/#211).
    Approach,
    /// Inside a facility: a raid is under way and the campaign is waiting for its
    /// verdict.
    Inside,
    /// The run reached the end of its sequence. The archive and its ending are #217's;
    /// what is settled here is only *that* the run ends won.
    Won,
    /// Captured (§2.2) — terminal for the run, wherever it happened. There is no retry
    /// and no snapshot to restore; the value is dropped and the next run starts from
    /// nothing.
    Lost,
}

impl CampaignStage {
    /// Whether the run is over, either way — the one question every caller asks before
    /// offering the player anything.
    pub fn is_over(self) -> bool {
        matches!(self, CampaignStage::Won | CampaignStage::Lost)
    }
}

/// A **run**: the facilities it will raid, where it stands in them, and what it is
/// carrying (§14 v3 / §2.2).
///
/// The type is deliberately small and plain — the whole layer is a state machine over
/// four transitions, and every one of them is a method here:
///
/// | | |
/// |---|---|
/// | [`enter`](Self::enter) | hand out the current facility's [`LevelSeed`] and start the raid |
/// | [`complete`](Self::complete) | fold a finished raid's verdict back in |
/// | [`salvage`](Self::salvage) | add found tech to what the run carries (#209's seam) |
/// | *drop* | the run is over and nothing survives it (§2.2) |
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Campaign {
    seed: u64,
    route: Vec<NodeId>,
    position: usize,
    stage: CampaignStage,
    loadout: Loadout,
    intel: u32,
    alert: u32,
}

impl Campaign {
    /// A fresh run from its seed, at the standard [`CAMPAIGN_LENGTH`].
    pub fn new(seed: u64) -> Self {
        Self::of_length(seed, CAMPAIGN_LENGTH)
    }

    /// A fresh run over an explicit number of facilities: a straight route of
    /// `facilities` nodes, innate abilities only, an empty wallet and a quiet world.
    ///
    /// **Innate only** is §2.2's across-runs rule stated as a starting value: tech is
    /// salvaged *within* a run (#209), so a run that began holding some would be
    /// meta-progression by another name. Panics on an empty route — a campaign of no
    /// facilities is not a degenerate run, it is a bug in whoever built it.
    ///
    /// The length is a knob rather than a constant here because the two ends of it are
    /// both real: #207 sets it from the graph's depth to the archive, and a route of
    /// **one** is the degenerate campaign — a single facility, entered and left, which
    /// is the game v1 already ships.
    pub fn of_length(seed: u64, facilities: usize) -> Self {
        assert!(facilities > 0, "a campaign runs at least one facility");
        Self {
            seed,
            route: (0..facilities as u32).map(NodeId::new).collect(),
            position: 0,
            stage: CampaignStage::Approach,
            loadout: Loadout::innate(),
            intel: 0,
            alert: 0,
        }
    }

    /// The run's seed (§12.4) — every facility's seed derives from it.
    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// The facilities this run will raid, in order.
    pub fn route(&self) -> &[NodeId] {
        &self.route
    }

    /// How far along the route the run is: `0` before the first raid, the route's
    /// length once the last one is done.
    pub fn position(&self) -> usize {
        self.position
    }

    /// The facility the run is at — about to enter, or inside — or `None` once the
    /// route is walked out.
    pub fn node(&self) -> Option<NodeId> {
        self.route.get(self.position).copied()
    }

    /// Where the run stands.
    pub fn stage(&self) -> CampaignStage {
        self.stage
    }

    /// The run's outcome in the turn loop's own vocabulary (§4.5), so a caller that
    /// already branches on [`Outcome`] does not need a second shape for the same
    /// three answers.
    pub fn outcome(&self) -> Outcome {
        match self.stage {
            CampaignStage::Approach | CampaignStage::Inside => Outcome::Playing,
            CampaignStage::Won => Outcome::Won,
            CampaignStage::Lost => Outcome::Lost,
        }
    }

    /// The abilities the run carries — its accumulating loadout (§2.2/§8.3). Every
    /// facility boots with exactly this set.
    pub fn loadout(&self) -> Loadout {
        self.loadout
    }

    /// The intel banked so far — the run's **currency** (§2.2), not an exit key.
    /// Harvested at every completed raid; the sinks that spend it are #211's.
    pub fn intel(&self) -> u32 {
        self.intel
    }

    /// The campaign alert (§7.3/§14 v3) — the run-level layer above the *in-facility*
    /// ladder, which climbs within a raid and dies with it.
    ///
    /// **Zero, for now.** #210 owns the whole of it: what a loud raid contributes, the
    /// decay and floor that keep §2.2's fairness promise, and the mapping onto level
    /// modifiers that makes being loud in facility 2 a fact about facility 3. Until
    /// then every facility starts at base alert, which is v1's behaviour and is the
    /// honest state of the game rather than a placeholder rule the balance would
    /// quietly inherit.
    pub fn alert(&self) -> u32 {
        self.alert
    }

    /// The run's framing (§2.2/appendix 31): a campaign, so the end screen offers no
    /// way to play it again.
    ///
    /// The difficulty names [`Difficulty::Standard`] and means it: the campaign scales
    /// through the **alert** (#210), not through the §12.6 difficulty axis quick play
    /// chose at the menu, and [`RunOptions::difficulty`] exists to reroll a *new run*
    /// at the same setting — which is the one exit a campaign does not have.
    pub fn run_options(&self) -> RunOptions {
        RunOptions {
            mode: RunMode::Campaign,
            difficulty: Difficulty::Standard,
        }
    }

    /// The config the current facility would boot with — the run's carried state
    /// resolved into one [`LevelSeed`], without starting the raid.
    ///
    /// `None` once the route is walked out. Three pieces, all of them the campaign's:
    /// the seed derived from `(run seed, node id)`, the modifiers, and the carried
    /// loadout. The gate is [`IntelGate::None`] because §4.5 settles it that way for
    /// the campaign — intel is currency, so the exit never refuses and extraction is
    /// voluntary — and the modifiers resolve through [`ModifierSources`] so the alert
    /// contribution (#210) has a place to arrive that is not a private knob.
    pub fn next_level(&self) -> Option<LevelSeed> {
        let node = self.node()?;
        Some(LevelSeed {
            seed: facility_seed(self.seed, node),
            modifiers: ModifierSources {
                chosen: LevelModifiers {
                    intel_to_exit: IntelGate::None,
                    ..LevelModifiers::default()
                },
                // The campaign alert's modifier contribution is #210's mapping; the
                // hook is here so it lands in the shared seam (§12.6) rather than in
                // a difficulty path of the campaign's own.
                alert: None,
            }
            .resolve(),
            abilities: self.loadout,
        })
    }

    /// **Enter the current facility**: the raid begins, and the caller boots
    /// [`start_level`](crate::start_level) on the config handed back.
    ///
    /// `None` when there is no facility to enter — the run is over, or a raid is
    /// already under way, in which case handing the config out again would invite a
    /// second [`State`](crate::State) for one facility and two answers to how it went.
    pub fn enter(&mut self) -> Option<LevelSeed> {
        if self.stage != CampaignStage::Approach {
            return None;
        }
        let level = self.next_level()?;
        self.stage = CampaignStage::Inside;
        Some(level)
    }

    /// **Complete the current raid**: fold its verdict into the run and hand control
    /// back to the campaign layer. Returns the stage the run is now in.
    ///
    /// The facility itself does not survive: geometry, guards and bodies belonged to
    /// the [`State`](crate::State) the caller is about to drop, and nothing here keeps
    /// a copy. What crosses is the three carried axes and nothing else.
    ///
    /// - **Escaped** — the raid's intel is banked and the run moves to the next
    ///   facility; past the last one it is [`Won`](CampaignStage::Won).
    /// - **Captured** or **entombed** — the run is over (§2.2). Nothing is banked,
    ///   because there is no later facility to spend it in.
    ///
    /// Does nothing outside a raid ([`Inside`](CampaignStage::Inside)): a verdict with
    /// no raid behind it is a caller bug, not a run event.
    pub fn complete(&mut self, verdict: &Verdict) -> CampaignStage {
        debug_assert_eq!(
            self.stage,
            CampaignStage::Inside,
            "a verdict arrived with no raid under way",
        );
        if self.stage != CampaignStage::Inside {
            return self.stage;
        }
        match verdict.ending {
            Ending::Captured { .. } | Ending::Entombed { .. } => self.stage = CampaignStage::Lost,
            Ending::Escaped => {
                self.bank(verdict.stats);
                self.position += 1;
                self.stage = if self.position >= self.route.len() {
                    CampaignStage::Won
                } else {
                    CampaignStage::Approach
                };
            }
        }
        self.stage
    }

    /// Add a raid's haul to what the run carries: the consoles it took, and — once
    /// #210 says what a loud raid is worth — its loudness.
    ///
    /// The raid's [`alert_peak`](RunStats::alert_peak) is deliberately *not* read here.
    /// It is the obvious thing to fold in and the wrong thing to fold in blind: a rule
    /// invented in this ticket would be a difficulty curve nobody designed, arriving
    /// before the relief valves §2.2 requires of one.
    fn bank(&mut self, stats: RunStats) {
        let taken = u32::try_from(stats.intel).unwrap_or(u32::MAX);
        self.intel = self.intel.saturating_add(taken);
    }

    /// **Salvage tech** (§2.2/§8.3): add `id` to the loadout the rest of the run
    /// carries — the seam an equipment cache writes (#209).
    ///
    /// Idempotent, and capacity is not its business: the held cap and the discard
    /// prompt it needs are #266's, and enforcing a silent one here would drop a
    /// pickup the player was never told about.
    pub fn salvage(&mut self, id: AbilityId) {
        self.loadout = self.loadout.with(id);
    }
}
