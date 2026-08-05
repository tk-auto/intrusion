//! The **facility map** (§14 v3): the campaign's geography, as a graph with real
//! edges, grown lazily from the run seed.
//!
//! The old game's map was "a flat list with no adjacency and no geography, where every
//! unlocked facility was always selectable" (§14 v3). The complaint against it is one
//! sentence — **geography should mean something** — and everything here follows from
//! taking that literally: a facility you can reach from where you stand is a different
//! thing from a facility that merely exists, and which ground the run is *on* has to
//! change what is in front of it.
//!
//! # Lazy, and deterministic anyway
//!
//! Nothing is pre-generated. A node's successors, its [`Flavour`] and its position are
//! all **derived on demand** from `(run seed, node identity)` and from nothing else
//! (§12.4) — so there is no world-build at the start of a run, no fog to lift over one,
//! and no state to keep in step with the walking. The two properties people usually
//! think are in tension are both here at once: the graph does not exist until it is
//! looked at, and looking at it twice gives the same graph.
//!
//! **Never a fresh source.** Every draw below hangs off the run seed through
//! [`MAP_STREAM_SALT`], the same discipline [`facility_seed`](super::facility_seed)
//! follows: a whole run reproduces from `(run seed, [inputs])` because the path taken is
//! a function of the inputs and the graph is a function of the seed.
//!
//! # The lattice, and why identity is `(depth, lane)`
//!
//! A node is a **depth** — how far along the run is, with the archive at
//! [`DEPTH_TO_ARCHIVE`] — and a **lane**, one of [`LANES`] parallel tracks across the
//! facility's country. [`NodeId`](super::NodeId) packs the pair, which is what makes
//! lazy growth safe: successors are *named*, not invented, so two nodes can never
//! collide on an id and a facility is the same facility however the run reached it.
//!
//! **Lanes are topology, not geography.** What a node's lane buys is who its neighbours
//! are; where it is *drawn* is [`position`](FacilityMap::position), a seeded wander
//! around the lane rather than a column ruled down the page. The two are deliberately
//! different: adjacency has to be crisp for the rules to be legible, and the picture has
//! to be crooked for the country to read as country.
//!
//! # What a choice is made of
//!
//! From any node short of the archive the run is offered **[`MIN_OPEN`]–[`MAX_OPEN`]**
//! successors, drawn from the lanes *adjacent* to the one it stands in — so a run
//! against the edge of the map is offered fewer, and drifting across the country is
//! something you do a lane at a time rather than by teleporting to whatever looked best.
//! Each successor's flavour is **visible when offered** (§14 v3: no fog), because the
//! choice is the mechanic and a choice made blind is a coin flip.
//!
//! One further successor is **[`Offer::locked`]** — the §14 v3 "unlock an alternative
//! route" sink (#212), shipped inert here. It reaches a lane two across, which the open
//! edges cannot reach at all, so what intel buys is *ground*: not a better facility
//! handed over, a part of the map that was not on offer. What stands on that ground is
//! whatever the seed put there, exactly as everywhere else.

use crate::modifiers::{GuardCount, IntelCount, LevelModifiers};
use crate::rng::Rng;

use super::NodeId;

#[cfg(test)]
mod tests;

/// How many parallel lanes the country is divided into — the map's width in adjacency
/// terms.
///
/// **Five [START].** It has to be odd, so the run starts and the archive sits in a
/// middle lane with the same room on either side; it has to be at least five for a
/// **far** lane ([`Offer::locked`]) to exist at all, since the open edges already reach
/// one lane either way. Five is therefore the smallest width at which every rule here
/// has something to bite on, and a wider country would only mean more of the map a
/// single run never sees.
pub const LANES: u32 = 5;

/// The lane the run starts in, and the lane the archive stands in: the middle one, so
/// the first choice and the last convergence are both symmetric.
pub const CENTRE_LANE: u32 = LANES / 2;

/// **Depth to the archive** (§14 v3) — how many facilities deep the run reaches before
/// the graph ends, and the coarsest knob on the 2–3 hour target (§2.2).
///
/// **Six [START]**, so a run raids seven facilities: the start, five choices, and the
/// archive. It takes over the campaign-length constant #206 carried as a placeholder,
/// which is what that constant said it was for. Every number around it is a starting
/// value too — the ticket proposed 6–8 and the honest answer is that nobody has played
/// a run end to end yet, so this moves the day the sim or a human says it should.
pub const DEPTH_TO_ARCHIVE: u32 = 6;

/// The fewest open successors a choice point offers, and the most (§14 v3 **[START]**,
/// 2–3).
///
/// Two is the floor because one is not a choice; three is the ceiling because the open
/// edges reach the adjacent lanes and there are only three of those. A node against the
/// edge of the country has only two neighbouring lanes and is offered two — that is the
/// geography biting, not a shortfall.
pub const MIN_OPEN: u32 = 2;
/// See [`MIN_OPEN`].
pub const MAX_OPEN: u32 = 3;

/// Separates the map's draws from every other use of the run seed (§12.4), exactly as
/// [`FACILITY_STREAM_SALT`](super::FACILITY_STREAM_SALT) separates the per-facility
/// seed: two streams that never share a position, so no draw here can shift a facility
/// and no facility can shift the shape of the country.
const MAP_STREAM_SALT: u64 = 0x_0BAD_C0DE_0BAD_C0DE;

/// The salt on the **per-depth flavour rotation** — a third stream, for the reason the
/// other two exist. See [`FacilityMap::flavour`], which is the one draw here that is
/// deliberately *not* per node.
const FLAVOUR_STREAM_SALT: u64 = 0x_F1A9_0000_F1A9_0000;

/// The odd stride a node id is multiplied by before it is mixed — the golden-ratio
/// constant, so consecutive ids land far apart in the seed space rather than in a
/// neighbourhood.
const NODE_STRIDE: u64 = 0x_9E37_79B9_7F4A_7C15;

/// Cells between one lane and the next in drawn space, and between one depth and the
/// next. Presentation-adjacent numbers that live here rather than in the renderer
/// because they are what [`position`](FacilityMap::position) means: the map screen
/// (#208) fits the country it is handed, it does not lay it out.
pub const LANE_SPACING: i32 = 8;
/// See [`LANE_SPACING`].
pub const DEPTH_SPACING: i32 = 6;

/// How far a node may wander from its lane's centre line, and from its depth's row —
/// the difference between a map and a spreadsheet (§14 v3's "arbitrary geography, not
/// strict columns").
///
/// Bounded well inside [`LANE_SPACING`] so the wander never lets two lanes cross: which
/// lane a node is in must be readable off the picture, or the adjacency rule stops being
/// something the player can see and starts being something they have to be told.
const JITTER_X: i32 = 2;
/// See [`JITTER_X`].
const JITTER_Y: i32 = 1;

/// What kind of facility a node is (§14 v3 **[START]**) — the thing that makes a branch
/// a decision rather than a coin flip.
///
/// **Two axes, and both of them are real** (§2.3). Guards are the risk and consoles are
/// the reward — under the campaign's [`IntelGate::None`](crate::IntelGate) intel is
/// currency (§2.2), so a console is loot rather than a lock — and each flavour is a
/// stated position on both, resolved through the [`ModifierSources::flavour`] seam
/// (§12.6). A flavour that carried no modifier set would be a differently-worded label
/// on the same facility, which is the old flat list wearing a costume.
///
/// **A property of the node, not of the edge.** [`FacilityMap::flavour`] derives it from
/// the node's own identity, so a facility is the same facility however a run reached it
/// — the invariant [`NodeId`](super::NodeId) exists to hold.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Flavour {
    /// **Thin, and thinly guarded.** One console fewer and one guard fewer: the quiet
    /// route, taken when the run needs a facility it can walk out of rather than a
    /// facility worth robbing.
    Outpost,
    /// **The plain facility** — the §10.2 recipe untouched, and the flavour that proves
    /// the others are doing something: it is the game as v1 ships it, standing in a
    /// campaign.
    Depot,
    /// **Rich, and watched.** One console more and one guard more — the trade the whole
    /// map exists to offer, and the reason an [`Outpost`](Self::Outpost) is not simply
    /// the correct answer every time.
    Vault,
    /// **Somebody else's kit, badly locked up** (§14 v3/#209): the facility that hides
    /// an **equipment cache**, and the only way salvaged tech enters a run.
    ///
    /// It is the third position on the reward axis rather than a fourth rung on the
    /// same ladder, and that is the whole reason it exists. A [`Vault`](Self::Vault)
    /// pays in **intel**, which is currency the run spends (§2.2); this pays in a §8.3
    /// **ability**, which the run keeps for every facility after it — §14 v3's "power
    /// curve, and the reason the campaign exists". Two rewards you cannot convert
    /// between is what makes a choice point a decision rather than a ranking.
    ///
    /// **And it costs a console.** One fewer than the recipe asks for, so the trade is
    /// tech *instead of* currency rather than tech *as well as* — the §2.3 rule that a
    /// reward with no cost is not a choice, applied on the map rather than inside the
    /// building. Guards stay at the recipe's count: a facility that were both poorer
    /// and better watched would be one nobody picks, which is the same failure as one
    /// everybody does.
    Workshop,
    /// **The archive** (§14 v3): the run's terminus at [`DEPTH_TO_ARCHIVE`], and the one
    /// node nobody chooses between. More guards, and a search that flushes hideouts —
    /// the last raid should be the hard one.
    ///
    /// What the archive actually *holds*, and what reaching it concludes, is #217's; this
    /// is the guarantee that the graph ends somewhere distinguished, and that walking
    /// into it feels like arriving.
    Archive,
}

impl Flavour {
    /// The flavours a **choice point** draws from, in the fixed cycle
    /// [`FacilityMap::flavour`] rotates. [`Archive`](Self::Archive) is not among them:
    /// it is where the map ends, not something offered against alternatives.
    ///
    /// The order is the risk/reward ladder, quiet end first, and the cycle's *step* is
    /// what guarantees a differentiated offer — see [`FacilityMap::flavour`].
    pub const OFFERED: [Flavour; 4] = [
        Flavour::Outpost,
        Flavour::Depot,
        Flavour::Vault,
        Flavour::Workshop,
    ];

    /// **Every** flavour, [`OFFERED`](Self::OFFERED) plus the terminus — what an
    /// exhaustive check walks. The map screen (#208) measures its rows against this at
    /// **compile time**, so a blurb too long for the board fails the build rather than
    /// being discovered as a clipped line in a screenshot.
    pub const ALL: [Flavour; 5] = [
        Flavour::Outpost,
        Flavour::Depot,
        Flavour::Vault,
        Flavour::Workshop,
        Flavour::Archive,
    ];

    /// The flavour's name, as the map screen (#208) prints it. `const` so that screen's
    /// width bound can measure it before the build finishes.
    pub const fn label(self) -> &'static str {
        match self {
            Flavour::Outpost => "Outpost",
            Flavour::Depot => "Depot",
            Flavour::Vault => "Vault",
            Flavour::Workshop => "Workshop",
            Flavour::Archive => "Archive",
        }
    }

    /// One line saying what taking this node buys and what it costs — the offer as the
    /// player reads it, in §11.8's meta vocabulary. It says the **trade**, not the
    /// numbers: "one more console" is the help panel's job once there is a run to
    /// describe (§12.6), and a menu row that printed modifier arithmetic would be asking
    /// the player to do the sum the flavour exists to have already done.
    pub const fn blurb(self) -> &'static str {
        match self {
            Flavour::Outpost => "thin, and thinly guarded",
            Flavour::Depot => "an ordinary facility",
            Flavour::Vault => "worth robbing, and watched",
            Flavour::Workshop => "salvage, and little else",
            Flavour::Archive => "what you came for",
        }
    }

    /// The modifier contribution this flavour makes to the facility it names (§12.6) —
    /// what it *is*, mechanically, and the whole of it.
    ///
    /// It lands in [`ModifierSources::flavour`](crate::ModifierSources) rather than in a
    /// knob set of the campaign's own, so it composes with the player's choice and with
    /// the campaign alert (#210) under one rule instead of three.
    pub fn modifiers(self) -> LevelModifiers {
        match self {
            Flavour::Outpost => LevelModifiers {
                guard_count: GuardCount::Fewer,
                intel_count: IntelCount::Fewer,
                ..LevelModifiers::neutral()
            },
            // The recipe untouched, said as the empty contribution rather than as a
            // special case skipped at the seam — a Depot composes like every other
            // flavour, it simply composes to nothing.
            //
            // **[`neutral`] and not [`default`]**: the default is the *game's* baseline,
            // which names an intel gate one rung tighter than the campaign's, and union
            // composes gates harder-ward. A flavour built from the default would lock
            // the exit of a facility whose whole point is that intel is currency (§2.2).
            //
            // [`neutral`]: LevelModifiers::neutral
            // [`default`]: LevelModifiers::default
            Flavour::Depot => LevelModifiers::neutral(),
            Flavour::Vault => LevelModifiers {
                guard_count: GuardCount::More,
                intel_count: IntelCount::More,
                ..LevelModifiers::neutral()
            },
            // The cache, and the console it costs (#209). Both halves are here rather
            // than the reward alone, because a flavour is a *position* on the axes and
            // not a bonus: what the run buys with the console it gives up is an ability
            // it keeps for the rest of the campaign (§2.2).
            Flavour::Workshop => LevelModifiers {
                equipment_cache: true,
                intel_count: IntelCount::Fewer,
                ..LevelModifiers::neutral()
            },
            // The terminus is the hard raid, and it is hard through **pressure** rather
            // than through scarcity: the consoles stay at the recipe's count because what
            // the archive holds is #217's to say, and a number invented here would be a
            // reward curve nobody designed sitting on the run's last facility.
            Flavour::Archive => LevelModifiers {
                guard_count: GuardCount::More,
                guards_always_search_hideouts: true,
                ..LevelModifiers::neutral()
            },
        }
    }
}

/// Where a node is **drawn**, in cells (§14 v3) — the geography half of the model, kept
/// apart from the lane that decides adjacency.
///
/// Produced by [`FacilityMap::position`] and consumed by the map screen (#208). It is
/// part of the *model* rather than of the renderer because it has to be stable: a
/// facility that moved on the map between two looks at it would be a different place
/// each time you glanced away.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct MapPos {
    /// Across the country — lane, wandered.
    pub x: i32,
    /// Along the run — depth, wandered. Zero at the start node and rising toward the
    /// archive; which end of a screen that is, is the renderer's business.
    pub y: i32,
}

/// One node offered at a choice point: which facility, what it is, and whether the run
/// may take it (§14 v3).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Offer {
    /// The facility on the other end of the edge.
    pub node: NodeId,
    /// What it is — **visible when offered**, always (§14 v3: no fog). The map screen
    /// draws it and the facility plays it, from this one value.
    pub flavour: Flavour,
    /// Whether this edge is the **intel-locked** one (§14 v3's alternative-route sink).
    ///
    /// Inert here (#207): the edge is drawn, named and refused by
    /// [`Campaign::choose`](super::Campaign::choose). #212 spends intel against it
    /// through #211's wallet, and this is the seam it flips — nothing else about the
    /// graph changes when it does.
    pub locked: bool,
}

/// The campaign's **graph** (§14 v3): every facility a run could reach, as a function of
/// the run seed.
///
/// It holds a seed and nothing else, deliberately. There is no node table, no adjacency
/// list and no cursor — those are what a pre-generated world needs, and every one of them
/// is a copy of something the seed already says. Growing the graph is therefore free, and
/// asking the same question twice cannot give two answers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FacilityMap {
    seed: u64,
    depth: u32,
}

impl FacilityMap {
    /// The country a run of this seed raids, at the standard [`DEPTH_TO_ARCHIVE`].
    pub fn new(seed: u64) -> Self {
        Self::to_depth(seed, DEPTH_TO_ARCHIVE)
    }

    /// The same country cut to an explicit depth — the knob [`DEPTH_TO_ARCHIVE`] is the
    /// starting value of.
    ///
    /// A depth of **zero** is the degenerate campaign §12.7 names: the start node *is*
    /// the archive, one facility entered and left, which is the game v1 already ships.
    /// The tests use short depths to walk a whole run in milliseconds; nothing else
    /// should.
    pub fn to_depth(seed: u64, depth: u32) -> Self {
        Self { seed, depth }
    }

    /// The run seed the whole country hangs off (§12.4).
    pub fn seed(self) -> u64 {
        self.seed
    }

    /// How deep the archive sits — [`DEPTH_TO_ARCHIVE`] for a standard run.
    pub fn depth(self) -> u32 {
        self.depth
    }

    /// Where a run begins: depth zero, the middle lane.
    pub fn start(self) -> NodeId {
        NodeId::at(0, CENTRE_LANE)
    }

    /// The archive — the distinguished terminus every route converges on.
    pub fn archive(self) -> NodeId {
        NodeId::at(self.depth, CENTRE_LANE)
    }

    /// Whether `node` is the archive, and so the end of the run's traversal.
    pub fn is_archive(self, node: NodeId) -> bool {
        node.depth() >= self.depth
    }

    /// What kind of facility a node is (§14 v3).
    ///
    /// **The one draw here that is not per node, and it earns the exception.** A flavour
    /// derived independently for each node could hand a choice point three identical
    /// options — the branch would be cosmetic, which is the precise failure the map is
    /// replacing (§14 v3, and this ticket's own bite check). So the flavours are a fixed
    /// **cycle** ([`Flavour::OFFERED`]) laid across the lanes, rotated by an amount drawn
    /// per depth from the run seed.
    ///
    /// That buys the guarantee outright: the open successors of a node are distinct lanes
    /// inside a window three wide ([`open_lanes`]), and any two distinct values inside a
    /// window of three consecutive integers differ modulo any cycle length of three or
    /// more — so **every open offer is a choice between different things**, on every
    /// seed, at every node, with no rejection loop and no draw that could fail. The
    /// rotation is what stops that being a fixed stripe down the country.
    ///
    /// It therefore survived the cycle growing from three flavours to four (#209), and
    /// would survive a fifth. What a longer cycle costs is *coverage*: with four
    /// flavours over a three-wide window, one of them is missing from any given choice
    /// point — which is the map having somewhere to send you rather than a shortfall.
    ///
    /// And it is still a property of the *node*: two runs standing on the same facility
    /// see the same flavour, whichever way they came.
    pub fn flavour(self, node: NodeId) -> Flavour {
        if self.is_archive(node) {
            return Flavour::Archive;
        }
        let cycle = Flavour::OFFERED.len() as u32;
        let mut rng = Rng::new(
            self.seed ^ FLAVOUR_STREAM_SALT ^ u64::from(node.depth()).wrapping_mul(NODE_STRIDE),
        );
        let rotation = rng.below(cycle);
        Flavour::OFFERED[((node.lane() + rotation) % cycle) as usize]
    }

    /// Where a node is drawn (§14 v3) — its lane and depth, wandered off the ruled line
    /// by a seeded jitter so the map reads as country rather than as a table.
    ///
    /// Derived from `(run seed, node id)` like everything else, so a facility holds still
    /// on the map however often it is looked at.
    pub fn position(self, node: NodeId) -> MapPos {
        let mut rng = self.node_rng(node, 1);
        let lane_centre = node.lane() as i32 * LANE_SPACING + LANE_SPACING / 2;
        MapPos {
            x: lane_centre + rng.range_inclusive(-JITTER_X, JITTER_X),
            y: node.depth() as i32 * DEPTH_SPACING + rng.range_inclusive(-JITTER_Y, JITTER_Y),
        }
    }

    /// The successors of `node`: the open edges first, then the intel-locked one (§14 v3).
    ///
    /// Empty at the archive — traversal ends there (§2.2: forward-only, no backtracking,
    /// and nothing past the terminus).
    ///
    /// **The last hop converges.** Every node one short of the archive has exactly one
    /// successor and it is the archive, wherever across the country the run has drifted
    /// to. That is the graph *ending* rather than fanning out forever, and it is why the
    /// run's last choice is made a facility earlier than its last raid.
    pub fn successors(self, node: NodeId) -> Vec<Offer> {
        if self.is_archive(node) {
            return Vec::new();
        }
        let next = node.depth() + 1;
        if next >= self.depth {
            return vec![Offer {
                node: self.archive(),
                flavour: Flavour::Archive,
                locked: false,
            }];
        }
        let mut rng = self.node_rng(node, 0);
        let mut lanes = open_lanes(node.lane());
        let wanted = (MIN_OPEN + rng.below(MAX_OPEN - MIN_OPEN + 1)) as usize;
        // A partial Fisher–Yates over the reachable lanes, then the first `wanted` of
        // them: a node against the edge of the country has fewer neighbours than a node
        // in the middle and is offered fewer, which is the geography doing its job.
        let taken = wanted.min(lanes.len());
        for i in 0..taken {
            let j = i + rng.below((lanes.len() - i) as u32) as usize;
            lanes.swap(i, j);
        }
        let mut offers: Vec<Offer> = lanes[..taken]
            .iter()
            .map(|&lane| self.offer(NodeId::at(next, lane), false))
            .collect();
        // The offers are drawn in a shuffled order; the run reads them as a list, and a
        // list whose rows change places between two looks at the same choice is a list
        // you have to re-read. Sorting by lane makes the rows sit in the order the map
        // draws them, left to right.
        offers.sort_by_key(|offer| offer.node.lane());
        if let Some(lane) = locked_lane(node.lane(), &mut rng) {
            offers.push(self.offer(NodeId::at(next, lane), true));
        }
        offers
    }

    /// One offer over a node, with its flavour resolved.
    fn offer(self, node: NodeId, locked: bool) -> Offer {
        Offer {
            node,
            flavour: self.flavour(node),
            locked,
        }
    }

    /// A draw stream for one node, in the map's own salt (§12.4). `use_` separates the
    /// independent questions asked about a single node — its successors, its position —
    /// so adding a third cannot shift the answers to the first two.
    fn node_rng(self, node: NodeId, use_: u64) -> Rng {
        Rng::new(
            self.seed
                ^ MAP_STREAM_SALT
                ^ u64::from(node.get())
                    .wrapping_mul(NODE_STRIDE)
                    .rotate_left(use_ as u32 * 17),
        )
    }
}

/// The lanes an open edge may reach from `lane`: its own and the two beside it, clipped
/// to the country's width.
///
/// **This is the whole of "geography means something".** A run drifts across the map one
/// lane at a time, so where it stands decides what is in front of it, and reaching the
/// far side is a sequence of choices rather than a single selection from a list. It is
/// also what makes the locked edge worth anything: there is ground the open edges cannot
/// reach from here.
fn open_lanes(lane: u32) -> Vec<u32> {
    (lane.saturating_sub(1)..=(lane + 1).min(LANES - 1)).collect()
}

/// The lane the **intel-locked** edge reaches from `lane`: two across, which no open edge
/// from here can reach (§14 v3's "unlock an alternative route").
///
/// Both sides exist only from the middle of the country, and the seed picks between them;
/// nearer the edge there is one far lane and it is taken. `None` cannot happen at
/// [`LANES`] = 5 and is the honest answer at a width where it could — a lock over an edge
/// that reaches nothing would be an offer with nothing behind it.
fn locked_lane(lane: u32, rng: &mut Rng) -> Option<u32> {
    let left = lane.checked_sub(2);
    let right = (lane + 2 < LANES).then_some(lane + 2);
    match (left, right) {
        (Some(l), Some(r)) => Some(if rng.bool() { l } else { r }),
        (some, None) | (None, some) => some,
    }
}
