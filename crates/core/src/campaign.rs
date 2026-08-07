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
//! | Intel | **accumulates and is spent** ([`wallet`]) | nothing carries |
//! | Alert | **carries and scales** ([`alert`](Campaign::alert)) — #210, see below | nothing carries |
//!
//! "Across runs nothing carries" needs no code at all: a [`Campaign`] is a plain
//! value with no persistence behind it, so a lost run is a dropped value and the
//! next one is built from scratch. What *does* need code is the within-a-run half,
//! and it is these three fields, carried from the day the layer exists rather than
//! retrofitted. The loadout is now filled by the thing it was declared for — an
//! equipment cache opened in a facility rides out on the verdict and is added here
//! (#209) — intel is a [`Wallet`] with a debit path rather than a counter (#211), and the
//! alert now closes §14 v3's loop (#210).
//!
//! # Intel is spent here, and nowhere else
//!
//! The wallet's one exit is [`spend`](Campaign::spend), and the sinks that call it live at
//! the **map between facilities** (§14 v3) — there is no in-level spending, and the stage
//! check that makes that true is in the campaign rather than in each sink. What a
//! campaign's intel is *for* is therefore the hub, not the exit: **the exit takes
//! nothing** (§4.5), so everything a facility holds past the first thing is **surplus**
//! and how much of it you stay for is yours to choose. Appendix 47 records why.
//!
//! The one thing the exit does ask is that the raid happened at all — the **minimum
//! haul** ([`IntelGate::AtLeastOne`], #574): one objective taken, a console or a crate,
//! kept in full. That is a check, not a fee, and the distinction is the whole of why it
//! leaves the currency intact (appendix 59). A run can still walk out of a facility
//! poorer than it hoped; it can no longer walk out of one it never entered.
//!
//! **The alert carries exactly one hop**, which is the whole of what §14 v3 asks for:
//! *being loud in facility 2 makes facility 3 harder*. It is the §7.3 condition the last
//! completed raid ended at, it is **replaced** at every hop rather than accumulated, and
//! it reaches the facilities the map is about to offer through the §12.6 modifier seam —
//! one of them at condition 2, all of them at condition 3, and one of them the *other*
//! way after a raid nobody ever noticed. The mapping and the reasoning behind it are
//! [`loudness`]; the §7.3 ladder inside a facility stays exactly what it was, per
//! facility and dying with it.
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

use serde::{Deserialize, Serialize};

use crate::ability::{AbilityId, Loadout};
use crate::difficulty::Difficulty;
use crate::level_seed::LevelSeed;
use crate::modifiers::{IntelGate, LevelModifiers, ModifierDirection, ModifierSources};
use crate::rng::Rng;
use crate::salvage::cache_contents;
use crate::state::Outcome;
use crate::verdict::{Ending, RunMode, RunOptions, RunStats, Verdict};

pub mod loudness;
pub mod map;
#[cfg(test)]
mod tests;
pub mod wallet;

pub use loudness::{Loudness, ALERTS_ALL, ALERTS_ONE};
pub use map::{FacilityMap, Flavour, MapPos, Offer, DEPTH_TO_ARCHIVE};
pub use wallet::{Outlay, Wallet};

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
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
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

/// **What an alternative route costs** (§14 v3's first intel sink, #212) — the price of
/// flipping the map's intel-locked successor to takeable.
///
/// **One [START], and the number follows from what is being sold.** The map draws unbought
/// ground as `?`: what the run is paying for is a lane two across with **no idea what
/// stands on it**. A price has to be proportionate to what the buyer knows, and against a
/// facility's whole haul — three consoles at the §10.2 recipe — a road bought blind is not
/// worth a raid. One intel is what an unseen road is worth, and the sink still asks for
/// something real: the **first** choice point of every run is unaffordable, because
/// nothing has been raided yet.
///
/// **It does not rest on scarcity.** The bite is opportunity cost — the sinks behind it
/// (#213–#216) spend the same wallet, so a route bought here is an alert not lowered or a
/// facility not scouted there. If a played run buys one reflexively at every junction, the
/// first lever is **not** the price: it is that the player cannot see what they are buying,
/// and the scouting sinks are what fix that. Appendix 48 records this and the reasoning it
/// replaced.
pub const ROUTE_UNLOCK_COST: u32 = 1;

/// **What scouting a facility costs** (§14 v3's pre-level intel sink, #215) — the price of
/// a plan of the building ahead: its consoles, its crates and its cupboards, drawn in the
/// §11.5a remembered state from turn one.
///
/// **[START], and it is a facility's whole haul.** Three consoles is what the §10.2 recipe
/// puts in a building, so scouting one facility costs what raiding one earns: the sink is
/// deliberately the expensive end of the hub, because what it sells is the largest thing
/// intel can buy — the §10 exploration of an entire building, answered before turn one. A
/// price that let a run scout every facility it walked into would make the fog a formality
/// rather than a thing the player pays to lift.
///
/// **What that price actually asks for.** No run can scout its *first* facility, and few
/// can scout two in a row: three intel is two or three raids' surplus once the other sinks
/// have taken their share, so scouting is the thing a run saves up for and spends on the
/// facility it is most afraid of. That is the decision the sink exists to sell, and the
/// reason it is priced three rather than one — see [`ROUTE_UNLOCK_COST`], which is one
/// because a blind road is worth a third of what a known building is.
///
/// **The §2.3 answer to *when would a good player choose not to?*** is on the map beside
/// it. The same wallet buys the alternative route and the sinks after it, so a facility
/// scouted is three roads not bought; and the scout is bought **before the run commits**,
/// so intel spent on a facility the run then declines is spent all the same. Scouting the
/// road you are not sure of is exactly the purchase that can be wrong.
pub const SCOUT_COST: u32 = 3;

/// **What a facility's cache manifest costs** (§14 v3's second pre-level intel sink, #550)
/// — the price of learning *which* tech the crates ahead hold.
///
/// **[START], and it is priced against what it does not tell you.** The full scout
/// ([`SCOUT_COST`]) hands over the building: where every console, crate and cupboard
/// stands. This hands over one fact about at most three boxes, and **never a position** —
/// so a run that buys it still has to find the crates, and what it has bought is only the
/// answer to *is the detour worth walking*.
///
/// Two thirds of a scout rather than a third of one, because the fact is worth more than
/// its size suggests: a crate is at least sixteen cells from the spawn (§10.6), and the
/// §8.3 rules make the detour a real gamble — the tech may be something the run already
/// carries, and at a full bar it costs an **exchange** (#266). Knowing beforehand is what
/// turns walking past a `¤` from a shrug into a decision, which is exactly the §2.3 bite
/// the sink is asked for.
///
/// **The §2.3 answer to *when would a good player choose not to?***: when the run needs
/// anything at all and will walk to every crate regardless — an early run with two slots
/// free has nothing to decide — and when the two intel are wanted for the road or the
/// building itself. It is the cheap sink beside an expensive one, so buying it is rarely
/// wrong and often unnecessary, which is the shape a small price should have.
pub const MANIFEST_COST: u32 = 2;

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
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
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
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
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
    /// The run's **currency** (§2.2/§14 v3) — see [`wallet`]. Filled by every completed
    /// raid and emptied by the hub's sinks, and by nothing else: the balance is not a
    /// field anything outside [`Wallet`] can set.
    wallet: Wallet,
    alert: u32,
    /// The intel-locked edges this run has **bought** (§14 v3/#212), in the order it
    /// bought them.
    ///
    /// A list of node identities rather than a flag on the map, for the reason the map
    /// holds no node table at all: the country is a function of the seed and buying a road
    /// does not change the country — it changes what *this run* may walk down. Two runs of
    /// one seed differ in exactly this and in the path they took, which is the whole of
    /// what a run is.
    unlocked: Vec<NodeId>,
    /// **The facilities this run has scouted** (§11.5a/§14 v3/#215), in the order it
    /// bought them.
    ///
    /// A list of node identities rather than a flag on the map, for the reason
    /// [`unlocked`](Self::unlocked) is one: the country is a function of the seed and
    /// buying a plan of a building does not change the building — it changes what *this
    /// run* walks in knowing. It is keyed by node so a scout bought at a choice point
    /// survives the [`choose`](Self::choose) that walks the run onto it, and so a scout
    /// bought for a facility the run then declines is spent rather than silently
    /// refunded onto the one it took instead.
    scouted: Vec<NodeId>,
    /// **The facilities whose cache manifest this run has bought** (§8.3/§14 v3/#550), in
    /// the order it bought them.
    ///
    /// A list of nodes like [`unlocked`](Self::unlocked) and [`scouted`](Self::scouted), and
    /// for the same reason — and this one carries even less: the manifest is a *fact about
    /// the country* that the seed already fixes, so what a purchase records is only that
    /// **this run has been told**. Nothing about the facility changes, which is why —
    /// unlike the scout — it reaches no [`LevelModifiers`] and takes no level-seed token
    /// slot (§12.7): there is nothing to carry into the raid, only something to say at the
    /// hub.
    manifests: Vec<NodeId>,
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
            wallet: Wallet::empty(),
            alert: 0,
            unlocked: Vec::new(),
            scouted: Vec::new(),
            manifests: Vec::new(),
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
    ///
    /// **A route this run has bought is not locked** (#212). The map still calls the edge
    /// locked — the country is a function of the seed and a purchase does not change it —
    /// so the run's own purchases are folded in here, at the one place every caller reads
    /// its options from. That is what makes [`choose`](Self::choose) accept a bought road
    /// without knowing the sink exists.
    pub fn offers(&self) -> Vec<Offer> {
        if self.stage != CampaignStage::Choosing {
            return Vec::new();
        }
        self.map
            .successors(self.node())
            .into_iter()
            .map(|offer| Offer {
                locked: offer.locked && !self.unlocked.contains(&offer.node),
                ..offer
            })
            .collect()
    }

    /// Whether this run has bought its way onto `node` (#212) — the purchase itself,
    /// asked about directly rather than read off an [`Offer`], for a caller standing
    /// somewhere the offers are not.
    pub fn is_unlocked(&self, node: NodeId) -> bool {
        self.unlocked.contains(&node)
    }

    /// **Buy the alternative route** (§14 v3's first intel sink, #212): spend
    /// [`ROUTE_UNLOCK_COST`] to flip the map's intel-locked successor to takeable, and
    /// hand back what the wallet said.
    ///
    /// What the intel buys is **ground**, not a better facility: the locked edge reaches a
    /// lane *two* across, which no open edge from here can reach, and what stands on it is
    /// whatever the seed put there. It is bought unseen — the map draws unbought ground as
    /// `?` — so the purchase is a bet on a part of the country that was not on offer.
    ///
    /// **It does not commit the run.** A bought edge becomes an ordinary offer, flavour
    /// visible like every other (§14 v3 **[SETTLED]**: no fog on what is offered), and the
    /// run may still take one of the open roads instead. So what is bought is ground *and*
    /// the knowledge of what is on it, and the §2.3 answer to *when would a good player
    /// choose not to?* is: when the open offers already hold what the run needs, or when
    /// the intel is wanted for something else.
    ///
    /// Refused, with nothing spent, for anything that is not a locked edge on offer right
    /// now — including every call made outside a choice point, since
    /// [`offers`](Self::offers) is empty there. Buying the same road twice is refused the
    /// same way: after the first purchase it is no longer locked.
    #[must_use]
    pub fn unlock(&mut self, node: NodeId) -> Outlay {
        let locked = self
            .offers()
            .into_iter()
            .any(|offer| offer.node == node && offer.locked);
        if !locked {
            return Outlay::Closed;
        }
        let outlay = self.spend(ROUTE_UNLOCK_COST);
        if outlay.paid() {
            self.unlocked.push(node);
        }
        outlay
    }

    /// **Scout the facility at `node`** (§11.5a/§14 v3's pre-level intel sink, #215):
    /// spend [`SCOUT_COST`] so that raiding it opens with its points of interest already
    /// **remembered** — position known, live state as fogged as ever.
    ///
    /// What the intel buys is **a plan of the building's contents**: where the consoles,
    /// the crates and the cupboards are ([`scout`](crate::scout)). It is the one thing
    /// allowed to buy its way past §11.5a's *hidden until seen*, and what it may never buy
    /// is the live layer — no guard, no door pose, no cone — because that is earned inside
    /// the facility or not at all.
    ///
    /// **Bought before the run commits** (#215/#207). It is refused for anything that is
    /// not a facility the run may walk into right now ([`ahead`](Self::ahead)) — so at a
    /// choice point it is the *offered* successors that may be scouted, before
    /// [`choose`](Self::choose) has moved anything, and the run may still take one of the
    /// others with the intel spent. That is the sink's teeth: scouting a road you then
    /// decline costs exactly what scouting the road you take costs.
    ///
    /// Refused, with nothing spent, for everything [`scoutable`](Self::scoutable) refuses
    /// — a locked edge, a facility already scouted, one whose config has no room left for
    /// the rule — and outside the hub by [`spend`](Self::spend), like every sink.
    #[must_use]
    pub fn scout(&mut self, node: NodeId) -> Outlay {
        if !self.scoutable(node) {
            return Outlay::Closed;
        }
        let outlay = self.spend(SCOUT_COST);
        if outlay.paid() {
            self.scouted.push(node);
        }
        outlay
    }

    /// **Whether `node` may be scouted at all** (#215) — what the hub asks before it puts
    /// a price in front of the player, so an offer that cannot be taken is never made.
    ///
    /// Three things have to hold. The facility must be one the run may walk into
    /// ([`ahead`](Self::ahead)) and not behind a road it has not bought; it must not be
    /// scouted already ([`is_scouted`](Self::is_scouted)), since a second plan of one
    /// building is not a purchase; and the facility's config must still **fit its
    /// level-seed token** ([`LevelSeed::is_sayable`]).
    ///
    /// **That last one is a real refusal and not a formality.** The token carries at most
    /// a handful of rules (§12.7), and a rich facility under an alerted campaign can
    /// already be spending them all — a Vault is three on its own. Selling a rule that
    /// would push the config off the wire would buy the player a facility that can no
    /// longer be written down, shared or replayed, and the sink is not worth that: the
    /// row is simply not offered, exactly as an unaffordable one is drawn unaffordable
    /// rather than discovered by pressing it.
    pub fn scoutable(&self, node: NodeId) -> bool {
        let offered = self
            .ahead()
            .into_iter()
            .any(|offer| offer.node == node && !offer.locked);
        offered && !self.is_scouted(node) && self.level_at(node, true).is_sayable()
    }

    /// **Whether this run has scouted `node`** (#215) — the purchase itself, which is what
    /// the facility's [`LevelModifiers::scouted`] becomes when it is raided and what the
    /// hub reads to draw a facility as already bought.
    pub fn is_scouted(&self, node: NodeId) -> bool {
        self.scouted.contains(&node)
    }

    /// **Buy the cache manifest** for the facility at `node` (§8.3/§14 v3's second
    /// pre-level intel sink, #550): spend [`MANIFEST_COST`] to be told **which** tech its
    /// crates hold, and hand back what the wallet said.
    ///
    /// **What, never where.** The manifest is a *set*, and it says nothing about the cells
    /// the crates stand in — those stay fogged until seen (§11.5a), and buying them is
    /// [`scout`](Self::scout)'s. The two sinks compose and neither implies the other: a run
    /// may know a Vault holds the Drone and still have to find the box, or know where all
    /// three boxes are and not what is in any of them.
    ///
    /// Refused, with nothing spent, for everything [`manifest_on_sale`](Self::manifest_on_sale)
    /// refuses — a facility that is not on offer, one that hides no crates, one already
    /// bought — and outside the hub by [`spend`](Self::spend), like every sink.
    #[must_use]
    pub fn buy_manifest(&mut self, node: NodeId) -> Outlay {
        if !self.manifest_on_sale(node) {
            return Outlay::Closed;
        }
        let outlay = self.spend(MANIFEST_COST);
        if outlay.paid() {
            self.manifests.push(node);
        }
        outlay
    }

    /// **Whether `node`'s manifest may be bought right now** (#550) — what the hub asks
    /// before it puts a price in front of the player.
    ///
    /// Three things have to hold: the facility must be one the run may walk into
    /// ([`ahead`](Self::ahead)) and not behind a road it has not bought; it must actually
    /// **hide crates**; and it must not be bought already.
    ///
    /// **A facility with no crates is not offered the sale**, rather than offered and
    /// refused (#550). The flavour is visible when offered (§14 v3 **[SETTLED]**) and an
    /// [`Outpost`](Flavour::Outpost) hides none, so the row's absence tells the player
    /// nothing the map screen had not already told them — and a price for an empty list
    /// would be the hub selling a blank page.
    pub fn manifest_on_sale(&self, node: NodeId) -> bool {
        let offered = self
            .ahead()
            .into_iter()
            .any(|offer| offer.node == node && !offer.locked);
        offered && !self.has_manifest(node) && !self.crates_at(node).is_empty()
    }

    /// Whether this run has bought `node`'s manifest (#550).
    pub fn has_manifest(&self, node: NodeId) -> bool {
        self.manifests.contains(&node)
    }

    /// **What the crates at `node` hold**, or `None` while the run has not paid to know
    /// (#550) — the one read the hub draws its list from.
    ///
    /// An `Option` rather than a list-or-empty because the two are different facts and a
    /// screen must not confuse them: *nobody has told you* is not *there is nothing there*.
    /// A facility that hides no crates is never on sale in the first place, so a `Some`
    /// here is never empty.
    ///
    /// The order is the **draw's own** ([`cache_contents`]), which is the order the crates
    /// were stocked in and carries no spatial information — the placement that decides
    /// *where* they stand happens later and from a different stream. Handing back a list
    /// that could be read as a map of the building would give away #215's sink for free.
    pub fn manifest(&self, node: NodeId) -> Option<Vec<AbilityId>> {
        self.has_manifest(node).then(|| self.crates_at(node))
    }

    /// **The tech the facility at `node` is stocked with** — the truth behind
    /// [`manifest`](Self::manifest), read whether or not the run has paid for it.
    ///
    /// Private, because it is the one thing a hub must not be able to say by accident. It
    /// is also what makes the reveal **incapable of lying**: it is not a copy of the
    /// stocking rule, it is [`cache_contents`] itself, called on the very
    /// [`LevelSeed`] the raid will boot from ([`level_at`](Self::level_at)). §8.3 draws a
    /// facility's crates from its seed alone, so the answer exists before the building
    /// does and cannot drift from what walking in would find.
    fn crates_at(&self, node: NodeId) -> Vec<AbilityId> {
        let level = self.level_at(node, self.is_scouted(node));
        cache_contents(level.seed, level.modifiers.caches.crates())
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
    /// open edge from here: a node the map does not offer, an **intel-locked** one the run
    /// has not bought ([`unlock`](Self::unlock)), and any call made while a raid is under
    /// way or the run is over. Forward-only is not enforced by a check: the offers only
    /// ever point forward, so there is nothing to refuse.
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
    /// Harvested at every completed raid, spent at the hub ([`spend`](Self::spend)).
    pub fn intel(&self) -> u32 {
        self.wallet.balance()
    }

    /// Whether the run could pay `cost` right now — what a sink asks before it *offers*,
    /// so an unaffordable price is drawn as unaffordable rather than only discovered by
    /// pressing the key.
    ///
    /// It answers about the balance alone. Whether the run is anywhere it may spend is
    /// [`spend`](Self::spend)'s to say, and only that call settles it.
    pub fn affords(&self, cost: u32) -> bool {
        self.wallet.affords(cost)
    }

    /// **Spend intel at the hub** (§14 v3) — the one debit path, and the call every sink
    /// makes before it applies its effect.
    ///
    /// Three answers, all of them [`Outlay`]'s: paid, refused for want of intel, or
    /// refused because the run is not at the hub. A refusal changes **nothing** — no
    /// partial payment, no half-applied sink — so a caller that branches on
    /// [`Outlay::paid`] cannot leave the run in a state where the money went somewhere the
    /// effect did not.
    ///
    /// **Where "at the hub" is** (§14 v3): the map between facilities, which is both live
    /// stages the map screen is the surface of — standing on a facility not yet raided
    /// ([`Approach`](CampaignStage::Approach)) and at a choice point
    /// ([`Choosing`](CampaignStage::Choosing)). Refused [`Inside`](CampaignStage::Inside),
    /// because there is no in-level spending and a wallet you could dip into mid-raid
    /// would let the player buy out of a §4.4 mistake, and refused once the run is over,
    /// because there is nothing left to spend on.
    ///
    /// The check lives here rather than in each sink for the reason the wallet is a
    /// newtype: a sink that forgot it would be a shop open inside a facility, and nothing
    /// about the sink's own code would look wrong.
    #[must_use]
    pub fn spend(&mut self, cost: u32) -> Outlay {
        if self.stage == CampaignStage::Inside || self.stage.is_over() {
            return Outlay::Closed;
        }
        self.wallet.spend(cost)
    }

    /// The campaign alert (§7.3/§14 v3/#210) — the run-level layer above the
    /// *in-facility* ladder, which climbs within a raid and dies with it.
    ///
    /// **The condition the last completed raid ended at**, on the same 0…[`TOP_RUNG`]
    /// scale the facility ladder uses, and zero before a run has finished one. It is
    /// replaced at each hop rather than added to: what carries is the last raid's noise
    /// and nothing older, which is what makes the loud → harder → louder spiral §2.2
    /// warns against unrepresentable rather than merely unlikely.
    ///
    /// What it *does* is [`Loudness`] — see [`loudness`] for the mapping and
    /// [`alert_reaches`](Self::alert_reaches) for which facility ahead it landed on.
    ///
    /// [`TOP_RUNG`]: crate::alert::TOP_RUNG
    pub fn alert(&self) -> u32 {
        self.alert
    }

    /// **How loud the last completed raid was**, as the campaign reads it — or `None`
    /// before the run has finished one.
    ///
    /// The distinction is load-bearing and is why this is an `Option` rather than
    /// `Loudness::of(self.alert())`: a run that has raided nothing is not a run that
    /// slipped through unnoticed, and the cherry for a ghost raid (§7.3 condition 0)
    /// must not be handed to the first facility of every campaign for free.
    pub fn loudness(&self) -> Option<Loudness> {
        self.noise_made_at().map(|_| Loudness::of(self.alert))
    }

    /// **Whether the last raid's noise reached `node`**, and which way it bends it
    /// (§12.6) — what the map screen (#208) reads to say *which* facility ahead is
    /// expecting you.
    ///
    /// `None` for every facility the noise did not reach and before the run's first raid.
    /// It answers about any node, but only the ones the map has just offered can ever be
    /// `Some`: the alert reaches one hop and no further. The **alternative route** (#212)
    /// is among them at the top of the ladder and only there — see [`Loudness::reaches`],
    /// which is where that asymmetry is argued.
    pub fn alert_reaches(&self, node: NodeId) -> Option<ModifierDirection> {
        let loudness = self.loudness()?;
        let from = self.noise_made_at()?;
        loudness
            .reaches(self.map, from, node)
            .then(|| loudness.direction())
            .flatten()
    }

    /// **The facility the noise on the ground ahead was made in** — the raid the
    /// campaign alert is currently reporting, or `None` before the run has finished one.
    ///
    /// It is a different node in the two live stages, and that is the geography rather
    /// than a special case. At a [`Choosing`](CampaignStage::Choosing) point the run is
    /// still standing on the facility it has just emptied, so the noise was made *here*
    /// and the offers it reaches are this node's successors. Once the run has
    /// [`choose`](Self::choose)n and walked on, the facility it stands on is one of those
    /// successors, and the noise was made one node back.
    fn noise_made_at(&self) -> Option<NodeId> {
        match self.stage {
            // Only a completed raid puts a run here, so there is always one behind it.
            CampaignStage::Choosing => Some(self.node()),
            _ => self.previous(),
        }
    }

    /// The facility the run stood on **before** the one it stands on now. `None` on the
    /// first facility of a run, which is the whole of the "no raid behind us yet" case.
    fn previous(&self) -> Option<NodeId> {
        let before = self.path.len().checked_sub(2)?;
        self.path.get(before).copied()
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
    /// id)`, the modifiers, and the carried loadout. See [`level_at`](Self::level_at),
    /// which is this asked about any facility the run can see.
    pub fn next_level(&self) -> LevelSeed {
        self.level_at(self.node(), self.is_scouted(self.node()))
    }

    /// **The config raiding `node` would boot** — [`next_level`](Self::next_level) asked
    /// about a facility the run has not walked into yet, with `scouted` standing in for
    /// what it would know when it did (#215).
    ///
    /// The gate is [`IntelGate::AtLeastOne`] — the **minimum haul** (#574): a facility
    /// must be left with at least one objective taken, an intel console or an equipment
    /// cache. Intel stays a currency and not an exit key, because *nothing is spent* —
    /// the haul is kept, the wallet never sees the exit, and everything past the first
    /// thing is still surplus. What closes is only the case of walking out with nothing,
    /// which made a building something you could stand in the doorway of (appendix 59).
    /// **Except at the archive**, which is [`IntelGate::All`] and is the run's one
    /// mandatory *complete* objective (§14 v3/#217): the terminus asks for all of it,
    /// and taking its data out is the run won.
    ///
    /// **The gate is set here rather than in the archive's composite**, and the line is
    /// worth keeping straight (#565). A composite says what a *facility* is — how many
    /// guards stand in it, what is locked, what its patrols notice — and every one of those
    /// clauses of the archive is in [`Composite::Archive`](crate::Composite::Archive). A
    /// gate says what the **run** is asked for, which is a property of the **node**: it is
    /// the end of this map, and there is nothing past it to spend a surplus in. Two
    /// different facts about one word, kept in the two places that own them — and both
    /// travel in the token, the composite in its slot and the gate in its own field.
    ///
    /// The modifiers resolve through [`ModifierSources`], where the node's **flavour** (§14
    /// v3) and the campaign alert (#210) each land as their own source rather than as a
    /// private knob set of the campaign's.
    ///
    /// **This is what makes the offer honest** (§2.3). The map screen says *Vault*, and
    /// the run walks into the facility that flavour's [`Flavour::modifiers`] describe —
    /// one console more, one guard more — because the same value produces both. It also
    /// travels: the flavour rides in the [`LevelSeed`]'s modifiers, so a campaign
    /// facility's level-seed token is the facility as it was actually played (§12.7).
    ///
    /// The `scouted` parameter is what lets the hub ask a question about a purchase it has
    /// not made: *would this facility still fit its token if I sold the scout?*
    /// ([`scoutable`](Self::scoutable)). Every other caller passes what the run has
    /// actually bought.
    pub fn level_at(&self, node: NodeId, scouted: bool) -> LevelSeed {
        LevelSeed {
            seed: facility_seed(self.seed(), node),
            modifiers: ModifierSources {
                chosen: LevelModifiers {
                    // A minimum haul everywhere, the whole set at the terminus
                    // (§4.5/#211/#217/#574).
                    intel_to_exit: if self.map.is_archive(node) {
                        IntelGate::All
                    } else {
                        IntelGate::AtLeastOne
                    },
                    // What the run **paid to know** about this facility before walking
                    // in (§11.5a/#215). It rides in the chosen set rather than in a
                    // source of its own because it is the run's own decision, exactly as
                    // the gate is — the alert and the flavour are what the *world* says
                    // about a facility, and neither has anything to add about what the
                    // player scouted.
                    scouted,
                    ..LevelModifiers::default()
                },
                // The campaign alert (#210), through the shared seam (§12.6) rather
                // than a difficulty path of the campaign's own: if the last raid's
                // noise reached this facility, it is drawn a rule — harder for a loud
                // raid, easier for one nobody noticed — and composed like any other
                // source.
                alert: self.alert_contribution(node),
                flavour: Some(self.map.flavour(node).modifiers()),
            }
            .resolve(),
            abilities: self.loadout,
        }
    }

    /// The modifier contribution the campaign alert makes to `node` (§12.6/#210) — `None`
    /// on the first facility of a run, and for a facility the last raid's noise did not
    /// reach.
    ///
    /// Kept beside [`level_at`](Self::level_at) rather than inlined into it so the
    /// source is a named thing a test can hold: the §2.3 assertion this ticket owes is
    /// *the rule the alert drew is active in the facility the run walks into*, and that
    /// is stated against the resolved seed, with this as its witness.
    ///
    /// The noise is read from [`noise_made_at`](Self::noise_made_at), the same place
    /// [`alert_reaches`](Self::alert_reaches) reads it — so the contribution and the map
    /// screen's *"the Vault is alerted"* line are two views of one fact, in both live
    /// stages.
    fn alert_contribution(&self, node: NodeId) -> Option<LevelModifiers> {
        let loudness = self.loudness()?;
        loudness.contribution(self.map, self.noise_made_at()?, node)
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

    /// Add a raid's haul to what the run carries: the consoles it took, the **tech it
    /// salvaged** (#209), and **how loud it was** (#210).
    ///
    /// The salvage is folded here rather than at the moment the crate was opened, and
    /// the two are not in tension: within the facility the ability is already on the
    /// player's deck (that is what "usable immediately" means), and this is the run
    /// *keeping* it — which only a raid the player walked out of has earned. A capture
    /// banks nothing, tech included, because there is no later facility to carry it to.
    ///
    /// The raid's [`alert_peak`](RunStats::alert_peak) is **assigned, not added**
    /// (§14 v3/#210). What carries is the noise of the raid just finished and nothing
    /// older: a quiet raid puts the campaign alert back to zero however loud the one
    /// before it was, which is what makes §2.2's "escalation must stay recoverable"
    /// a property of the type rather than a decay rate tuned to hope so. The ladder
    /// never falls *within* a facility (§7.3) and is wiped by the walk out of it, which
    /// is the same statement one layer up: an alert is a fact about a raid, and the raid
    /// is over.
    fn bank(&mut self, stats: RunStats) {
        self.wallet
            .bank(u32::try_from(stats.intel).unwrap_or(u32::MAX));
        self.alert = stats.alert_peak;
        // **The loadout is assigned, not added to** (§8.3/#266). It used to be a fold of
        // the raid's finds, which was right while a raid could only ever *gain* tech;
        // the exchange lets one be traded away, and once a set can shrink the order of
        // the moves matters — a run that swaps A for B and later finds A again ends
        // holding something no union of "found" and "given up" can reconstruct. So the
        // raid reports what it walked out with ([`RunStats::held`]) and this takes it,
        // the same way the alert takes the rung the raid ended at.
        self.loadout = stats.held;
    }

    /// **Salvage tech** (§2.2/§8.3): add `id` to the loadout the rest of the run
    /// carries — the seam an equipment cache writes (#209).
    ///
    /// Public as well as used by [`bank`](Self::bank): a raid folds its own finds in
    /// through the verdict, and a caller that has tech to grant from somewhere else (a
    /// test, a future sink) says so here rather than reaching into the loadout.
    ///
    /// Idempotent, and capacity is not enforced *here*: the §8.3 cap is kept at the
    /// **pickup**, where a bump on a crate the run has no room for is refused and says so
    /// ([`State::step`](crate::State::step)). That is the one place the player can be
    /// told; a silent cap in this method would drop a find nobody was warned about, so
    /// nothing that arrives here can be over the cap in the first place. Trading one
    /// piece of tech for another is #266's exchange screen.
    pub fn salvage(&mut self, id: AbilityId) {
        self.loadout = self.loadout.with(id);
    }
}
