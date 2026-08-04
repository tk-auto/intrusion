//! What the country owes a run (§14 v3): that it is a **graph** rather than a list, that
//! it grows lazily without ever growing differently, that a choice on it is a choice
//! between different things, and that it ends.

use super::*;

/// Every node a run of this seed could stand on, depth by depth — the whole country,
/// walked exhaustively rather than along one path, so a rule that only holds on the
/// route a test happened to take is not mistaken for a rule.
fn every_node(map: FacilityMap) -> Vec<NodeId> {
    (0..=map.depth())
        .flat_map(|depth| (0..LANES).map(move |lane| NodeId::at(depth, lane)))
        .collect()
}

/// **A node id is a coordinate**, and it round-trips as one — the property lazy growth
/// rests on, since a successor has to be *named* before anything has built it.
#[test]
fn a_node_id_is_its_depth_and_its_lane() {
    for depth in 0..=DEPTH_TO_ARCHIVE {
        for lane in 0..LANES {
            let node = NodeId::at(depth, lane);
            assert_eq!((node.depth(), node.lane()), (depth, lane));
        }
    }
    // Distinct coordinates are distinct ids: two facilities can never collide on one,
    // which is what stops a lazily-grown graph from quietly merging two places.
    let ids: Vec<u32> = every_node(FacilityMap::new(0))
        .iter()
        .map(|n| n.get())
        .collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), ids.len());
}

/// **The graph is a function of the seed** (§12.4) — the whole of "lazy ≠
/// non-deterministic". Growing the same country twice gives the same country, and a
/// different run seed gives a different one.
#[test]
fn the_same_seed_grows_the_same_country() {
    let a = FacilityMap::new(8371);
    let b = FacilityMap::new(8371);
    for node in every_node(a) {
        assert_eq!(a.successors(node), b.successors(node), "{node:?}");
        assert_eq!(a.flavour(node), b.flavour(node), "{node:?}");
        assert_eq!(a.position(node), b.position(node), "{node:?}");
    }

    let other = FacilityMap::new(8372);
    assert!(
        every_node(a)
            .into_iter()
            .any(|node| a.successors(node) != other.successors(node)
                || a.flavour(node) != other.flavour(node)),
        "a different run seed must be a different country",
    );
}

/// **A golden two-hop path** (§12.4): the exact successors, flavours and derived
/// facility seeds a named seed grows, pinned so that a change to the derivation is a
/// red test rather than a silently different game for everyone who ever shared a run.
#[test]
fn a_two_hop_path_grows_the_same_graph_every_time() {
    let map = FacilityMap::new(8371);
    let start = map.start();
    assert_eq!(start, NodeId::at(0, 2));

    let first: Vec<(u32, Flavour, bool)> = map
        .successors(start)
        .iter()
        .map(|o| (o.node.lane(), o.flavour, o.locked))
        .collect();
    assert_eq!(
        first,
        vec![
            (2, Flavour::Depot, false),
            (3, Flavour::Vault, false),
            (4, Flavour::Outpost, true),
        ],
        "the first choice moved",
    );

    let taken = NodeId::at(1, 3);
    let second: Vec<(u32, Flavour, bool)> = map
        .successors(taken)
        .iter()
        .map(|o| (o.node.lane(), o.flavour, o.locked))
        .collect();
    assert_eq!(
        second,
        vec![
            (3, Flavour::Outpost, false),
            (4, Flavour::Depot, false),
            // Two across, and the flavour is whatever stands there — here the same one
            // the open edge to lane 4 offers. Intel buys *ground*, not a better
            // facility handed over; what that ground is worth is the seed's business
            // and, once #212 prices the unlock, the player's judgement.
            (1, Flavour::Depot, true),
        ],
        "the second choice moved",
    );

    // And the facilities those two hops name (§12.7) — the seeds a run would actually
    // have played, not just the shape of the graph over them.
    assert_eq!(
        [
            crate::campaign::facility_seed(8371, start),
            crate::campaign::facility_seed(8371, taken),
        ],
        [124_555, 47_017],
    );
}

/// **Geography means something** (§14 v3): an open edge only ever reaches a lane
/// adjacent to the one the run stands in, so where you are decides what is in front of
/// you and crossing the country is a sequence of choices.
#[test]
fn an_open_edge_only_reaches_a_neighbouring_lane() {
    for seed in 0..30 {
        let map = FacilityMap::new(seed);
        for node in every_node(map) {
            for offer in map.successors(node) {
                assert_eq!(
                    offer.node.depth(),
                    node.depth() + 1,
                    "an edge must go forward exactly one facility (§2.2)",
                );
                if offer.node.depth() == map.depth() {
                    continue; // the last hop converges on the archive from anywhere
                }
                let reach = offer.node.lane().abs_diff(node.lane());
                if offer.locked {
                    assert_eq!(
                        reach, 2,
                        "the locked edge reaches ground the open ones cannot"
                    );
                } else {
                    assert!(reach <= 1, "an open edge reaches a neighbouring lane");
                }
            }
        }
    }
}

/// **Every choice is a choice between different things** (§2.3, and this ticket's own
/// bite check): no choice point ever offers two open successors of the same flavour, on
/// any seed, anywhere in the country. A branch whose options generate the same facility
/// is the old flat list wearing a costume.
#[test]
fn no_choice_point_offers_the_same_flavour_twice() {
    for seed in 0..60 {
        let map = FacilityMap::new(seed);
        for node in every_node(map) {
            let open: Vec<Flavour> = map
                .successors(node)
                .iter()
                .filter(|o| !o.locked)
                .map(|o| o.flavour)
                .collect();
            let mut distinct = open.clone();
            distinct.sort_by_key(|f| f.label());
            distinct.dedup();
            assert_eq!(
                distinct.len(),
                open.len(),
                "seed {seed}, {node:?} offered {open:?}",
            );
        }
    }
}

/// **A facility is the same facility however the run reached it.** Flavour is derived
/// from the node, not from the edge that offered it — so two routes that meet on one
/// node meet on one *place*.
#[test]
fn a_node_is_the_same_facility_from_every_direction() {
    let map = FacilityMap::new(1234);
    for node in every_node(map) {
        for offer in map.successors(node) {
            assert_eq!(
                offer.flavour,
                map.flavour(offer.node),
                "an offer must name the node's own flavour",
            );
        }
    }
}

/// **The offer is 2–3 open successors plus one locked edge** (§14 v3 **[START]**), and a
/// node against the edge of the country is offered fewer because its neighbourhood is
/// smaller — not because anything failed.
#[test]
fn a_choice_point_offers_two_or_three_open_successors_and_one_lock() {
    for seed in 0..30 {
        let map = FacilityMap::new(seed);
        for node in every_node(map) {
            if node.depth() + 1 >= map.depth() {
                continue; // the archive and the hop onto it are not choice points
            }
            let offers = map.successors(node);
            let open = offers.iter().filter(|o| !o.locked).count();
            let reachable = (node.lane().saturating_sub(1)..=(node.lane() + 1).min(LANES - 1))
                .count()
                .min(MAX_OPEN as usize);
            assert!(
                (MIN_OPEN as usize..=MAX_OPEN as usize).contains(&open) && open <= reachable,
                "seed {seed}, {node:?} offered {open} open successors",
            );
            assert_eq!(
                offers.iter().filter(|o| o.locked).count(),
                1,
                "exactly one intel-locked edge, always (#212's seam)",
            );
        }
    }
}

/// **The country ends, and every route ends at the same place** (§14 v3). The archive is
/// the one node with no successors, and it is what the last hop converges on wherever
/// the run has drifted to.
#[test]
fn every_route_converges_on_the_archive() {
    for depth in [0, 1, 2, DEPTH_TO_ARCHIVE] {
        let map = FacilityMap::to_depth(77, depth);
        assert_eq!(map.archive().depth(), depth);
        assert_eq!(map.flavour(map.archive()), Flavour::Archive);
        assert!(map.successors(map.archive()).is_empty());

        for lane in 0..LANES {
            let node = NodeId::at(depth.saturating_sub(1), lane);
            if depth == 0 {
                continue;
            }
            let offers = map.successors(node);
            assert_eq!(offers.len(), 1, "the last hop is not a choice");
            assert_eq!(offers[0].node, map.archive());
            assert!(!offers[0].locked, "nothing is locked out of the archive");
        }
    }
}

/// **Depth to the archive is one knob** (§14 v3), pinned so a change to the length of a
/// run is a deliberate one rather than a number that drifted.
#[test]
fn the_depth_to_the_archive_is_pinned() {
    assert_eq!(DEPTH_TO_ARCHIVE, 6);
    assert_eq!(
        FacilityMap::new(0).depth(),
        DEPTH_TO_ARCHIVE,
        "a standard run is the standard country",
    );
}

/// **Positions are geography, not a table** (§14 v3): nodes wander off their lane's
/// centre line, but never far enough for two lanes to cross — the picture has to be
/// crooked and the adjacency has to stay readable off it.
#[test]
fn positions_wander_without_letting_the_lanes_cross() {
    let mut wandered = false;
    for seed in 0..20 {
        let map = FacilityMap::new(seed);
        for node in every_node(map) {
            let pos = map.position(node);
            let centre = node.lane() as i32 * LANE_SPACING + LANE_SPACING / 2;
            assert!((pos.x - centre).abs() <= JITTER_X, "{node:?} left its lane");
            assert!((pos.y - node.depth() as i32 * DEPTH_SPACING).abs() <= JITTER_Y);
            wandered |= pos.x != centre;
        }
    }
    assert!(wandered, "a country ruled into columns is not a country");
    // The wander is bounded well inside the spacing, so the lane a node is in can still
    // be read straight off the picture — a build-time fact rather than a per-seed one.
    const _: () = assert!(JITTER_X * 2 < LANE_SPACING);
}

/// **Every flavour says what it is, distinctly.** The map screen (#208) draws these and
/// nothing else identifies a facility to the player, so two flavours sharing a word
/// would be a choice between two rows that read the same.
#[test]
fn every_flavour_names_itself() {
    let mut labels: Vec<&str> = Flavour::OFFERED
        .into_iter()
        .chain([Flavour::Archive])
        .map(|f| {
            assert!(!f.blurb().is_empty(), "{f:?} says nothing about itself");
            f.label()
        })
        .collect();
    let count = labels.len();
    labels.sort_unstable();
    labels.dedup();
    assert_eq!(labels.len(), count, "two flavours share a name");
    assert!(
        !Flavour::OFFERED.contains(&Flavour::Archive),
        "the archive is where the map ends, not something offered against alternatives",
    );
}
