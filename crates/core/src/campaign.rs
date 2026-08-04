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
//! # The geography
//!
//! The sequence is **forward-only** and it is a walk through a graph, not down a list:
//! at every completed facility the run is offered the successors of the node it stands
//! on, picks one, and moves. The graph itself — the lanes, the flavours, the lazy
//! derivation and the intel-locked edge — is [`map`], and this layer's whole
//! relationship with it is three calls: [`offers`](Campaign::offers) to ask what is
//! ahead, [`choose`](Campaign::choose) to move, and [`Flavour::modifiers`] to make the
//! facility what the offer said it was.
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

pub mod map;
#[cfg(test)]
mod tests;

pub use map::{FacilityMap, Flavour, MapPos, Offer, DEPTH_TO_ARCHIVE};

use map::LANES;

/// A facility's **identity** within a run — the key its seed is derived from
/// ([`facility_seed`]) and the node the campaign map (§14 v3) is a graph of.
///
/// A newtype rather than a bare index because it is not one: the map grows a graph
/// whose nodes are *chosen* from, so two runs that have visited the same number of
/// facilities need not be standing on the same one. Deriving from the **identity**
/// rather than from the position is what keeps a facility the same facility however a
/// run reached it.
///
/// # It packs `(depth, lane)`, and that is load-bearing
///
/// The graph is grown lazily (see [`map`]), which means a node has to be able to *name*
/// its successors before anything has built them. Packing the pair does exactly that:
/// an id is a coordinate rather than a serial number, so no two nodes can collide on
/// one, nothing has to allocate ids as it goes, and the identity of a facility is a
/// fact about where it stands rather than about the order some walk discovered it in.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct NodeId(u32);

impl NodeId {
    /// The node at `depth` in `lane` — the only way to name one.
    pub const fn at(depth: u32, lane: u32) -> Self {
        Self(depth * LANES + lane)
    }

    /// How far into the run this node stands: `0` at the start, [`DEPTH_TO_ARCHIVE`] at
    /// the archive.
    pub const fn depth(self) -> u32 {
        self.0 / LANES
    }

    /// Which lane across the country this node stands in — its adjacency, not its
    /// drawn position ([`FacilityMap::position`]).
    pub const fn lane(self) -> u32 {
        self.0 % LANES
    }

    /// The packed identity — what [`facility_seed`] mixes.
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

/// Where a run stands — the campaign's own answer, one rung coarser than the turn
/// loop's [`Outcome`] (§4.5).
///
/// The three live states are the ones the layer exists to tell apart, and the map (§14
/// v3) is why there are three rather than two: a player who has just walked out of a
/// facility has a **choice** to make and no facility to make it about, a player standing
/// at the mouth of the next one has a raid to start, and a player inside has a raid to
/// finish. Two dead states are §2.2's, and they are dead for the whole run, not for the
/// facility.
///
/// A stage rather than a pair of flags because the illegal combinations should not be
/// representable: "choosing where to go while inside a building" is the one this
/// spelling makes impossible to write.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CampaignStage {
    /// Standing on a facility not yet raided, with nothing between the run and it —
    /// where a run starts, and where [`choose`](Campaign::choose) leaves it.
    Approach,
    /// **At a choice point** (§14 v3): the last raid is banked, the map is the surface,
    /// and the run picks which of [`offers`](Campaign::offers) to walk to. The stage the
    /// map screen (#208) and the intel hub (#211) act in — and the only one in which the
    /// run may move.
    Choosing,
    /// Inside a facility: a raid is under way and the campaign is waiting for its
    /// verdict.
    Inside,
    /// The run reached the archive and left it. The ending itself is #217's; what is
    /// settled here is only *that* the run ends won.
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
/// | [`choose`](Self::choose) | take one of the offered edges and move (§14 v3) |
/// | [`salvage`](Self::salvage) | add found tech to what the run carries (#209's seam) |
/// | *drop* | the run is over and nothing survives it (§2.2) |
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Campaign {
    map: FacilityMap,
    /// Every facility the run has stood on, in the order it stood on them — the
    /// **route it actually took**, which on a graph is a thing that happens rather than
    /// a thing that is planned. The last entry is where the run is now.
    ///
    /// Kept whole rather than reduced to a cursor because the map screen (#208) draws
    /// the way you came, and because a run's route is the most interesting thing about
    /// it: two runs of one seed differ in exactly this.
    path: Vec<NodeId>,
    stage: CampaignStage,
    loadout: Loadout,
    intel: u32,
    alert: u32,
}

impl Campaign {
    /// A fresh run from its seed, over the standard country ([`DEPTH_TO_ARCHIVE`]).
    pub fn new(seed: u64) -> Self {
        Self::to_depth(seed, DEPTH_TO_ARCHIVE)
    }

    /// A fresh run over a country cut to an explicit depth: standing on its start node,
    /// innate abilities only, an empty wallet and a quiet world.
    ///
    /// **Innate only** is §2.2's across-runs rule stated as a starting value: tech is
    /// salvaged *within* a run (#209), so a run that began holding some would be
    /// meta-progression by another name.
    ///
    /// The depth is a knob rather than a constant here because both ends of it are
    /// real: [`DEPTH_TO_ARCHIVE`] is the shipped **[START]**, and a depth of **zero** is
    /// the degenerate campaign — the start node *is* the archive, one facility entered
    /// and left, which is the game v1 already ships.
    pub fn to_depth(seed: u64, depth: u32) -> Self {
        let map = FacilityMap::to_depth(seed, depth);
        Self {
            path: vec![map.start()],
            map,
            stage: CampaignStage::Approach,
            loadout: Loadout::innate(),
            intel: 0,
            alert: 0,
        }
    }

    /// The run's seed (§12.4) — every facility's seed, and the whole country, derive
    /// from it.
    pub fn seed(&self) -> u64 {
        self.map.seed()
    }

    /// The country this run is raiding (§14 v3) — the graph, for a caller that wants to
    /// look further ahead than the current offer (the map screen, #208).
    pub fn map(&self) -> FacilityMap {
        self.map
    }

    /// The facilities this run has stood on, in order, ending with the current one.
    pub fn path(&self) -> &[NodeId] {
        &self.path
    }

    /// The facility the run is at — about to enter, or inside. Never `None`: a run
    /// always stands somewhere, even when it stands on its last node.
    pub fn node(&self) -> NodeId {
        *self.path.last().expect("a run always stands somewhere")
    }

    /// What the map offers from where the run stands (§14 v3) — the open edges and the
    /// intel-locked one, each with its flavour visible.
    ///
    /// Empty at the archive and empty **inside** a facility: an offer is something to
    /// act on between raids, and handing one out mid-raid would invite a caller to move
    /// the run while it was still in a building.
    pub fn offers(&self) -> Vec<Offer> {
        if self.stage != CampaignStage::Choosing {
            return Vec::new();
        }
        self.map.successors(self.node())
    }

    /// **The facilities the run may walk into next** — what the map screen (#208) puts
    /// in front of the player, in one call, whichever of the two live stages the run is
    /// in.
    ///
    /// At a [`Choosing`](CampaignStage::Choosing) point that is
    /// [`offers`](Self::offers). On the [`Approach`](CampaignStage::Approach) — a fresh
    /// run, or a choice just made — there is exactly one, and it is **the facility the
    /// run is standing on**: the map is the surface a campaign is played from, so
    /// *"raid this one"* has to be a row on it rather than a second screen with one
    /// button. Empty once the run is over.
    ///
    /// The offer that names the current node is not a move; taking it enters. The two
    /// read identically to the player and differ only in whether
    /// [`choose`](Self::choose) runs first, which is the shell's one line of
    /// bookkeeping rather than a second screen's worth.
    pub fn ahead(&self) -> Vec<Offer> {
        match self.stage {
            CampaignStage::Approach => vec![Offer {
                node: self.node(),
                flavour: self.flavour(),
                locked: false,
            }],
            _ => self.offers(),
        }
    }

    /// **Take an offered edge** (§14 v3): move the run to `node` and stand it there,
    /// ready to enter. `true` if the run moved.
    ///
    /// Refused — and the run left exactly where it was — for anything that is not an
    /// open edge from here: a node the map does not offer, the **intel-locked** one
    /// (#212 opens that, and until it does this is what "shipped inert" means), and any
    /// call made while a raid is under way or the run is over. Forward-only is not
    /// enforced by a check: the offers only ever point forward, so there is nothing to
    /// refuse.
    pub fn choose(&mut self, node: NodeId) -> bool {
        let offered = self
            .offers()
            .into_iter()
            .find(|offer| offer.node == node && !offer.locked);
        if offered.is_none() {
            return false;
        }
        self.path.push(node);
        self.stage = CampaignStage::Approach;
        true
    }

    /// What the facility the run stands on **is** (§14 v3) — the flavour the offer named
    /// when it was taken, and the same one it will name to anyone who reaches this node
    /// by any route.
    pub fn flavour(&self) -> Flavour {
        self.map.flavour(self.node())
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
            CampaignStage::Approach | CampaignStage::Choosing | CampaignStage::Inside => {
                Outcome::Playing
            }
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
    /// Three pieces, all of them the campaign's: the seed derived from `(run seed, node
    /// id)`, the modifiers, and the carried loadout. The gate is [`IntelGate::None`]
    /// because §4.5 settles it that way for the campaign — intel is currency, so the
    /// exit never refuses and extraction is voluntary — and the modifiers resolve
    /// through [`ModifierSources`], where the node's **flavour** (§14 v3) and the
    /// campaign alert (#210) each land as their own source rather than as a private
    /// knob set of the campaign's.
    ///
    /// **This is what makes the offer honest** (§2.3). The map screen says *Vault*, and
    /// the run walks into the facility that flavour's [`Flavour::modifiers`] describe —
    /// one console more, one guard more — because the same value produces both. It also
    /// travels: the flavour rides in the [`LevelSeed`]'s modifiers, so a campaign
    /// facility's level-seed token is the facility as it was actually played (§12.7).
    pub fn next_level(&self) -> LevelSeed {
        LevelSeed {
            seed: facility_seed(self.seed(), self.node()),
            modifiers: ModifierSources {
                chosen: LevelModifiers {
                    intel_to_exit: IntelGate::None,
                    ..LevelModifiers::default()
                },
                // The campaign alert's modifier contribution is #210's mapping; the
                // hook is here so it lands in the shared seam (§12.6) rather than in
                // a difficulty path of the campaign's own.
                alert: None,
                flavour: Some(self.flavour().modifiers()),
            }
            .resolve(),
            abilities: self.loadout,
        }
    }

    /// **Enter the current facility**: the raid begins, and the caller boots
    /// [`start_level`](crate::start_level) on the config handed back.
    ///
    /// `None` when there is no facility to enter — the run is over, a raid is already
    /// under way (handing the config out again would invite a second
    /// [`State`](crate::State) for one facility and two answers to how it went), or the
    /// run is standing at a choice point with the last facility behind it and no
    /// [`choose`](Self::choose) made yet.
    pub fn enter(&mut self) -> Option<LevelSeed> {
        if self.stage != CampaignStage::Approach {
            return None;
        }
        self.stage = CampaignStage::Inside;
        Some(self.next_level())
    }

    /// **Complete the current raid**: fold its verdict into the run and hand control
    /// back to the campaign layer. Returns the stage the run is now in.
    ///
    /// The facility itself does not survive: geometry, guards and bodies belonged to
    /// the [`State`](crate::State) the caller is about to drop, and nothing here keeps
    /// a copy. What crosses is the three carried axes and nothing else.
    ///
    /// - **Escaped** — the raid's intel is banked and the run arrives at its next
    ///   **choice point** ([`Choosing`](CampaignStage::Choosing)), still standing on the
    ///   facility it has just emptied, with the map's offers ahead of it. Escaping the
    ///   **archive** is instead [`Won`](CampaignStage::Won): there is nothing past the
    ///   terminus to be offered.
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
                self.stage = if self.map.is_archive(self.node()) {
                    CampaignStage::Won
                } else {
                    CampaignStage::Choosing
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
