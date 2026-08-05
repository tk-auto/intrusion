//! **What a raid's noise does to the ground ahead** (§7.3/§14 v3/#210) — the campaign
//! alert, and the whole of the mapping from it onto level modifiers.
//!
//! §14 v3's complaint is one sentence: *"the whole point of the alert system is that
//! being loud in facility 2 makes facility 3 harder — until that loop closes, alert is
//! decoration."* The §7.3 ladder inside a facility has teeth (#311/#374), but it dies
//! with the facility, so nothing a raid did had ever survived the walk out of it. This
//! closes that loop, and it closes it through the **shared** §12.6 seam: what a loud
//! raid produces is a [`LevelModifiers`] contribution landing in
//! [`ModifierSources::alert`](crate::ModifierSources), drawn from the same directed pool
//! the difficulty axis draws from ([`draw_from_pool`]). There is no second difficulty
//! path, and no knob only the campaign can turn.
//!
//! # The rule
//!
//! The campaign alert is **the condition the last completed raid ended at** — the §7.3
//! rung, in the player's own word (§11.8) — and it reaches the facilities the map is
//! *about to offer*, and no further:
//!
//! | The raid ended at | What the offers ahead get |
//! |---|---|
//! | condition 0, never noticed | **one** of them, drawn, gets one **easier** rule |
//! | condition 1 | nothing |
//! | [`ALERTS_ONE`] (condition 2) | **one** of them, drawn, gets one **harder** rule |
//! | [`ALERTS_ALL`] (condition 3) | **every** one of them gets one **harder** rule |
//!
//! # Three things it deliberately is not
//!
//! **It does not accumulate**, so there is nothing to decay. The alert is *replaced* at
//! every hop by what the last raid did, and a quiet raid puts it back to zero on its
//! own. §2.2 demands escalation stay recoverable and warns off the loud → harder →
//! louder spiral; a level that cannot add to itself cannot spiral, which is a stronger
//! guarantee than a decay rate tuned to hope so. It is also why there is no floor to
//! state: the floor is zero and every raid can reach it.
//!
//! **It does not reach past the next hop.** Being loud in facility 2 makes facility 3
//! harder — that is the sentence, and the sentence is the whole promise. A raid whose
//! noise still bent facility 6 would be a difficulty curve nobody designed, and the
//! player would have no way to tell which raid they were still paying for.
//!
//! **The step from condition 2 to condition 3 is breadth, not depth.** Both switch on
//! one rule; what the top of the ladder takes away is the *route around it*. At
//! condition 2 the map still holds an unalerted way on, and finding it is the play; at
//! condition 3 every open road is watched and there is nothing to steer toward. That is
//! an escalation the player can read off the map screen and act on, which a second
//! modifier stacked on one facility would not be.
//!
//! # The alternative route is alerted at the top of the ladder, and only there
//!
//! At condition 2 the mark lands only on the **open** successors. The play at that rung is
//! finding the road nobody is watching, and a run should not have to *buy* one to have the
//! option — a mark on the intel-locked edge (§14 v3's alternative-route sink, #212) would
//! also be a consequence landing on ground the run has not paid to walk onto. The fiction
//! agrees with the arithmetic: that edge reaches a lane two across that the open ones
//! cannot, so it is the road the noise did not travel.
//!
//! At **condition 3** it is reached like everything else, bought or not. What the top of
//! the ladder takes away is *the route around it*, and a road you paid for is still a road
//! ahead; intel that bought immunity from the escalation would be a second, unwritten rule
//! about what the alert is, and it would quietly undo the one [SETTLED] thing condition 3
//! says. What #212's sink sells at that rung is **ground the map was not offering** — a
//! lane the run could not otherwise reach — which is worth intel without being a way out
//! of the alert.

use crate::alert::TOP_RUNG;
use crate::modifiers::{draw_from_pool, LevelModifiers, ModifierDirection};
use crate::rng::Rng;

use super::map::FacilityMap;
use super::NodeId;

/// The condition that alerts **one** facility ahead (§7.3 rung 2, **[START]**).
///
/// Rung 2 is where the facility stops reacting to you and starts *adding* to itself
/// (§7.3: a guard walks in), so it is the first rung whose meaning is "this raid went
/// wrong" rather than "you were seen once". Rung 1 is a fact of almost every honest run
/// — first contact and it never comes down — and a campaign that taxed it would be
/// taxing playing the game at all.
pub const ALERTS_ONE: u32 = 2;

/// The condition that alerts **every** facility ahead — the top of the §7.3 ladder,
/// where control has nothing louder to say than *send everyone*.
pub const ALERTS_ALL: u32 = TOP_RUNG;

/// Separates the two questions the alert asks about one node — *which* facility the
/// noise settled on, and *what* rule that facility is drawn — from each other and from
/// every other use of the run seed (§12.4). Two salts, exactly as the map keeps its
/// successors and its positions on separate streams: adding a question here must not
/// shift the answer to the ones already asked.
const MARK_STREAM_SALT: u64 = 0x_A1E7_0000_A1E7_0000;
/// See [`MARK_STREAM_SALT`].
const DRAW_STREAM_SALT: u64 = 0x_A1E7_FFFF_A1E7_FFFF;

/// The odd stride a node id is multiplied by before it is mixed — the golden-ratio
/// constant, so consecutive ids land far apart in the seed space rather than in a
/// neighbourhood.
const NODE_STRIDE: u64 = 0x_9E37_79B9_7F4A_7C15;

/// **How loudly a finished raid rang**, as the campaign reads it (§7.3/#210) — the
/// campaign alert, in the four states it can be in.
///
/// A named state rather than the bare rung, because what the layer above cares about is
/// not the number but what the number *does*, and the two are not the same shape: three
/// of the ladder's four positions map to three different behaviours and one maps to
/// none. Naming them is what lets the map screen say which one the run is in without
/// re-deriving the mapping (§11.3), and what makes the mapping total — every rung the
/// ladder can reach, including any it grows, has an answer here.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Loudness {
    /// **Condition 0** — nobody ever knew you were there. The cherry on a ghost run: one
    /// facility ahead is caught off guard, and gets a rule bent the player's way.
    ///
    /// It is the one *reward* in the mapping, and it is deliberately the same size as
    /// the punishment above it. A ghost run is the hardest way to play (§2.2 encourages
    /// stealth and rewards measured risk), and a campaign that only ever punished noise
    /// would make the alert a tax rather than an axis.
    #[default]
    Unnoticed,
    /// **Condition 1** — you were seen, or a post missed a ping, and nothing came of it
    /// beyond the facility you left behind. Nothing carries.
    Ordinary,
    /// **Condition 2** ([`ALERTS_ONE`]) — the facility called for help. One of the
    /// facilities ahead is expecting you, and which one is on the map.
    Noticed,
    /// **Condition 3** ([`ALERTS_ALL`]) — a body was found, or the net went quiet. Every
    /// open road ahead is expecting you; there is no unwatched way on.
    Hunted,
}

impl Loudness {
    /// What a raid that ended at `condition` leaves behind (§7.3) — the ladder's rung
    /// read as a campaign state.
    ///
    /// Saturating rather than exhaustive on the number: the ladder is capped at
    /// [`TOP_RUNG`] today, and a rung above it — were §7.3 ever to grow one — is
    /// unambiguously the loudest thing that can happen rather than an unhandled case.
    #[must_use]
    pub fn of(condition: u32) -> Self {
        match condition {
            0 => Loudness::Unnoticed,
            c if c >= ALERTS_ALL => Loudness::Hunted,
            c if c >= ALERTS_ONE => Loudness::Noticed,
            _ => Loudness::Ordinary,
        }
    }

    /// Which way this bends the facilities it reaches, or `None` when it reaches none.
    ///
    /// The direction the §12.6 pool is filtered on, so it is also the §2.3 guarantee:
    /// a raid that ended loud can only ever draw a rule documented *harder*, and one
    /// that ended unnoticed only ever one documented *easier*.
    #[must_use]
    pub fn direction(self) -> Option<ModifierDirection> {
        match self {
            Loudness::Unnoticed => Some(ModifierDirection::Easier),
            Loudness::Ordinary => None,
            Loudness::Noticed | Loudness::Hunted => Some(ModifierDirection::Harder),
        }
    }

    /// Whether the noise reaches **every** open road ahead rather than one of them —
    /// the whole of the step from condition 2 to condition 3.
    #[must_use]
    pub fn reaches_every_offer(self) -> bool {
        matches!(self, Loudness::Hunted)
    }

    /// Whether the noise made in `from` reached `to`.
    ///
    /// False for a node the run could not have walked to from there at all — the noise
    /// settles on the ground ahead, not on the country at large.
    ///
    /// **The intel-locked edge is reached at the top of the ladder and nowhere else**
    /// (#212). The two rungs are different statements and they treat a bought road
    /// differently on purpose:
    ///
    /// - At **condition 2** the noise settles on *one* open road ([`marked`]), drawn from
    ///   the map's own open successors. The alternative route is never that one, because
    ///   the play at condition 2 is finding the unwatched road and a run should not have to
    ///   buy one to have the option.
    /// - At **condition 3** it reaches **every** road ahead, the locked one included —
    ///   which is §14 v3's *"what the top of the ladder takes away is the route around it"*
    ///   [SETTLED] meant literally. A road you paid for is still a road ahead, and intel
    ///   that bought immunity from the escalation would be a second, unwritten rule about
    ///   what the alert is.
    ///
    /// It answers about the *edge*, not about the purchase: an alternative route is
    /// reported alerted at condition 3 whether or not this run has bought it, so the map's
    /// line says the same thing before and after the money changes hands.
    #[must_use]
    pub fn reaches(self, map: FacilityMap, from: NodeId, to: NodeId) -> bool {
        if self.direction().is_none() {
            return false;
        }
        if self.reaches_every_offer() {
            return map.successors(from).iter().any(|offer| offer.node == to);
        }
        let open = open_successors(map, from);
        open.contains(&to) && marked(map, &open) == Some(to)
    }

    /// **The contribution** the campaign alert makes to the facility at `to`, having
    /// been made in `from` — or `None` where the noise did not reach.
    ///
    /// One pick from the §12.6 directed pool, drawn per **node**: the two facilities a
    /// condition-3 run is offered are each drawn their own rule, so *every road is
    /// watched* is not *every road is watched the same way*.
    ///
    /// Built over [`LevelModifiers::neutral`] and not the default. A contributing source
    /// is a set of **departures** (§12.6), and a source built from the game's baseline
    /// would silently ask for the §4.5 intel gate — locking the exit of a campaign
    /// facility whose whole point is that intel is currency (§2.2) and extraction
    /// voluntary.
    #[must_use]
    pub fn contribution(
        self,
        map: FacilityMap,
        from: NodeId,
        to: NodeId,
    ) -> Option<LevelModifiers> {
        let direction = self.direction()?;
        self.reaches(map, from, to).then(|| {
            draw_from_pool(
                LevelModifiers::neutral(),
                direction,
                1,
                map.seed() ^ DRAW_STREAM_SALT ^ mix(to),
            )
        })
    }
}

/// The open successors of `node`, in the order the map offers them — the facilities the
/// noise may settle on.
fn open_successors(map: FacilityMap, node: NodeId) -> Vec<NodeId> {
    map.successors(node)
        .into_iter()
        .filter(|offer| !offer.locked)
        .map(|offer| offer.node)
        .collect()
}

/// Which of `open` the noise settled on, drawn from the run seed and the ground it was
/// made on — `None` only for a node with nothing open ahead of it (the archive).
///
/// **Deliberately not a function of the loudness.** Where the noise carries is a fact
/// about the country, so the facility a ghost run catches off guard is the same one a
/// loud run would have alerted. That keeps the draw one question with one answer, and it
/// makes the §13.4-style comparison honest: the loud and the quiet arms of a run differ
/// in *which way* one facility is bent, not in which facility.
fn marked(map: FacilityMap, open: &[NodeId]) -> Option<NodeId> {
    let first = *open.first()?;
    let mut rng = Rng::new(map.seed() ^ MARK_STREAM_SALT ^ mix(first));
    open.get(rng.below(open.len() as u32) as usize).copied()
}

/// A node's identity, spread across the seed space before it is mixed into a stream.
fn mix(node: NodeId) -> u64 {
    u64::from(node.get()).wrapping_mul(NODE_STRIDE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::campaign::map::DEPTH_TO_ARCHIVE;
    use crate::modifiers::IntelGate;

    /// A spread of run seeds wide enough that a rule which held on one country by luck
    /// shows up as a failure on another.
    const SEEDS: [u64; 6] = [1, 42, 66, 8371, 123_456, u64::MAX];

    /// The start node of every country — where the first raid's noise is made.
    fn start(map: FacilityMap) -> NodeId {
        map.start()
    }

    /// **The ladder maps onto the campaign, totally** (§7.3/#210): every rung the
    /// facility alert can reach has an answer, and the answers are the four this
    /// ticket settles.
    #[test]
    fn every_condition_the_ladder_can_reach_says_what_it_leaves_behind() {
        assert_eq!(Loudness::of(0), Loudness::Unnoticed);
        assert_eq!(Loudness::of(1), Loudness::Ordinary);
        assert_eq!(Loudness::of(ALERTS_ONE), Loudness::Noticed);
        assert_eq!(Loudness::of(ALERTS_ALL), Loudness::Hunted);
        // A rung above the ladder's top is the loudest thing that can happen, not an
        // unhandled case.
        assert_eq!(Loudness::of(TOP_RUNG + 7), Loudness::Hunted);

        // Only the two ends bend anything, and they bend opposite ways — the §2.3
        // guarantee, stated on the mapping before the pool is ever asked.
        assert_eq!(
            Loudness::Unnoticed.direction(),
            Some(ModifierDirection::Easier),
        );
        assert_eq!(Loudness::Ordinary.direction(), None);
        assert_eq!(
            Loudness::Noticed.direction(),
            Some(ModifierDirection::Harder),
        );
        assert_eq!(
            Loudness::Hunted.direction(),
            Some(ModifierDirection::Harder)
        );

        // The step from condition 2 to condition 3 is breadth and nothing else.
        assert!(!Loudness::Noticed.reaches_every_offer());
        assert!(Loudness::Hunted.reaches_every_offer());
    }

    /// **How far the noise carries**, over every country in the spread: one open road at
    /// condition 2, all of them at condition 3, one at condition 0, none at condition 1 —
    /// and the intel-locked edge only at the top of the ladder (#212).
    #[test]
    fn the_noise_reaches_one_road_ahead_or_all_of_them() {
        for seed in SEEDS {
            let map = FacilityMap::to_depth(seed, DEPTH_TO_ARCHIVE);
            let from = start(map);
            let offers = map.successors(from);
            let open: Vec<NodeId> = open_successors(map, from);
            assert!(open.len() >= 2, "a choice point offers a choice");

            let reached = |loudness: Loudness| -> Vec<NodeId> {
                open.iter()
                    .copied()
                    .filter(|&node| loudness.reaches(map, from, node))
                    .collect()
            };
            assert_eq!(reached(Loudness::Ordinary).len(), 0, "seed {seed}");
            assert_eq!(reached(Loudness::Unnoticed).len(), 1, "seed {seed}");
            assert_eq!(reached(Loudness::Noticed).len(), 1, "seed {seed}");
            assert_eq!(reached(Loudness::Hunted), open, "seed {seed}");

            // The ground the noise settles on is a fact about the country, not about how
            // the raid went: the facility caught off guard is the one that would have
            // been alerted.
            assert_eq!(reached(Loudness::Unnoticed), reached(Loudness::Noticed));

            // **The alternative route is reached at the top of the ladder and only there**
            // (#212). At condition 2 the play is finding the unwatched road, and a run
            // should not have to buy one to have that option; at condition 3 what the
            // escalation takes away is the route around it, and a road you paid for is
            // still a road ahead — intel that bought immunity from the alert would be a
            // second, unwritten rule about what the alert is.
            for offer in offers.iter().filter(|offer| offer.locked) {
                for loudness in [Loudness::Unnoticed, Loudness::Noticed] {
                    assert!(
                        !loudness.reaches(map, from, offer.node),
                        "seed {seed}: the alternative route was alerted below the top rung",
                    );
                }
                assert!(
                    Loudness::Hunted.reaches(map, from, offer.node),
                    "seed {seed}: the alternative route escaped a condition-3 sweep",
                );
            }

            // And never a node that is not ahead of `from` at all.
            assert!(!Loudness::Hunted.reaches(map, from, from));
        }
    }

    /// **The contribution is one rule, bending the way the raid asked for** — the §2.3
    /// directional assertion, at the mapping's own seam. It can never be empty where it
    /// reaches, and never the wrong way round.
    #[test]
    fn a_reached_facility_is_drawn_exactly_one_rule_of_the_asked_for_direction() {
        for seed in SEEDS {
            let map = FacilityMap::to_depth(seed, DEPTH_TO_ARCHIVE);
            let from = start(map);
            for loudness in [Loudness::Unnoticed, Loudness::Noticed, Loudness::Hunted] {
                let direction = loudness.direction().expect("it bends something");
                for node in open_successors(map, from) {
                    let Some(drawn) = loudness.contribution(map, from, node) else {
                        assert!(!loudness.reaches(map, from, node));
                        continue;
                    };
                    // A contribution is a set of **departures** from `neutral`, and
                    // `neutral` is not the empty active set: it names the campaign's own
                    // §4.5 intel gate. What the draw added is what is left once that row
                    // is taken off, and it must be exactly one rule of the asked-for
                    // direction.
                    let quiet = LevelModifiers::neutral().active();
                    let added: Vec<_> = drawn
                        .active()
                        .into_iter()
                        .filter(|rule| !quiet.contains(rule))
                        .collect();
                    assert_eq!(added.len(), 1, "seed {seed}: one rule, not a package");
                    assert_eq!(added[0].direction, direction, "seed {seed}");
                    // And it must not ask for a gate the campaign settles as `None`
                    // (§2.2): intel is currency, so the exit never refuses.
                    assert_eq!(drawn.intel_to_exit, IntelGate::None, "seed {seed}");
                }
            }
            // Condition 1 contributes nothing anywhere.
            for node in open_successors(map, from) {
                assert_eq!(Loudness::Ordinary.contribution(map, from, node), None);
            }
        }
    }

    /// **Same country, same answers** (§12.4) — asked twice, and asked about every node
    /// of a whole country rather than one choice point.
    #[test]
    fn the_mapping_is_a_function_of_the_country_and_the_node() {
        let map = FacilityMap::to_depth(8371, 4);
        for depth in 0..map.depth() {
            for lane in 0..crate::campaign::map::LANES {
                let from = NodeId::at(depth, lane);
                for to in open_successors(map, from) {
                    for loudness in [Loudness::Unnoticed, Loudness::Noticed, Loudness::Hunted] {
                        assert_eq!(
                            loudness.contribution(map, from, to),
                            loudness.contribution(map, from, to),
                        );
                    }
                }
            }
        }
        // Two countries do not agree by accident: over the spread, the rule an alerted
        // facility is drawn is not always the same one.
        let drawn: std::collections::BTreeSet<String> = SEEDS
            .iter()
            .map(|&seed| {
                let map = FacilityMap::to_depth(seed, DEPTH_TO_ARCHIVE);
                let from = start(map);
                let reached = open_successors(map, from)
                    .into_iter()
                    .find_map(|node| Loudness::Noticed.contribution(map, from, node))
                    .expect("condition 2 reaches one road");
                format!("{:?}", reached.active())
            })
            .collect();
        assert!(drawn.len() > 1, "the draw ignores its seed: {drawn:?}");
    }

    /// The last hop is the one place a choice point has a single successor — the archive
    /// (§14 v3). The noise still lands on it: there is nowhere else for it to go, which
    /// is the geography biting rather than a special case.
    #[test]
    fn the_last_hop_carries_the_noise_onto_the_archive() {
        let map = FacilityMap::to_depth(4242, 2);
        let from = NodeId::at(1, crate::campaign::map::CENTRE_LANE);
        let archive = map.archive();
        assert_eq!(open_successors(map, from), vec![archive]);
        for loudness in [Loudness::Unnoticed, Loudness::Noticed, Loudness::Hunted] {
            assert!(loudness.reaches(map, from, archive));
        }
        // And the archive itself has nothing ahead of it to alert.
        assert!(open_successors(map, archive).is_empty());
        assert!(!Loudness::Hunted.reaches(map, archive, archive));
    }
}
