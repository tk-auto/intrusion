use super::*;
use crate::region::RegionKind;
use crate::test_support::{open_room, seed_sweep};
use crate::vision::{field_of_view_with_peek, FULL_SIGHT_ARC, PLAYER_SIGHT_RANGE};
use std::collections::HashSet;

/// The bounding box `(width, height)` of a set of cells.
fn bbox(cells: &[Cell]) -> (u32, u32) {
    let x0 = cells.iter().map(|c| c.x).min().unwrap();
    let x1 = cells.iter().map(|c| c.x).max().unwrap();
    let y0 = cells.iter().map(|c| c.y).min().unwrap();
    let y1 = cells.iter().map(|c| c.y).max().unwrap();
    (x1 - x0 + 1, y1 - y0 + 1)
}

/// The bounding box of a region's **floor lane** — its cells minus the recessed
/// cupboards on the wall line. Room-size and corridor-width are guarantees about
/// the walkable lane (§10.1); a cupboard recessed into a wall joins the region it
/// opens onto (§10.1.6) but sits *outside* that lane, so it must not count toward
/// the lane's extent.
fn floor_bbox(facility: &Facility, cells: &[Cell]) -> (u32, u32) {
    let floor: Vec<Cell> = cells
        .iter()
        .copied()
        .filter(|&c| facility.terrain(c) == Some(Terrain::Floor))
        .collect();
    bbox(&floor)
}

fn regions_of_kind(layout: &Layout, kind: RegionKind) -> usize {
    layout
        .regions()
        .regions()
        .filter(|(_, r)| r.kind() == kind)
        .count()
}

/// The placement densities over a seed sweep under `tuning`, as
/// `(corridor_hideout, room_hideout, corridor_table, room_table)` — cupboards
/// or tables per walkable cell of that region kind. The metric behind the #91
/// bias, measured against [`Tuning::UNIFORM`] rather than a brittle absolute.
fn placement_shares(seeds: &[u64], tuning: &Tuning) -> (f64, f64, f64, f64) {
    let (mut cc, mut rc) = (0u32, 0u32); // corridor / room walkable cells
    let (mut ch, mut rh) = (0u32, 0u32); // corridor / room hideouts
    let (mut ct, mut rt) = (0u32, 0u32); // corridor / room tables
    for &seed in seeds {
        let layout =
            generate_where(40, 40, &mut Rng::new(seed), passes_guarantees, tuning).unwrap();
        let (f, g) = (layout.facility(), layout.regions());
        for (_, region) in g.regions() {
            let hideouts = region
                .cells()
                .iter()
                .filter(|&&c| f.terrain(c) == Some(Terrain::Hideout))
                .count() as u32;
            if region.kind() == RegionKind::Room {
                rc += region.cells().len() as u32;
                rh += hideouts;
            } else {
                cc += region.cells().len() as u32;
                ch += hideouts;
            }
        }
        for y in 0..f.height() {
            for x in 0..f.width() {
                let c = Cell::new(x, y);
                if f.terrain(c) != Some(Terrain::PartialCover) {
                    continue;
                }
                // A table is region-less (solid cover); name it by an adjacent
                // floor cell's region.
                match f
                    .neighbours(c)
                    .find_map(|n| g.region_at(n).map(|id| g.kind(id)))
                {
                    Some(RegionKind::Room) => rt += 1,
                    Some(RegionKind::Corridor) => ct += 1,
                    None => {}
                }
            }
        }
    }
    (
        ch as f64 / cc.max(1) as f64,
        rh as f64 / rc.max(1) as f64,
        ct as f64 / cc.max(1) as f64,
        rt as f64 / rc.max(1) as f64,
    )
}

/// #91 sharpened into a rule: hideouts lean into corridors (denser than rooms,
/// denser than the uniform tuning), and a table is **room furniture only** —
/// corridors carry none at all, under either tuning, because
/// [`can_take_table`] refuses corridor floor structurally rather than by
/// preference. Directions are asserted against [`Tuning::UNIFORM`] over a
/// seed sweep, not as brittle absolute counts.
#[test]
fn placement_is_biased_by_region() {
    let seeds = seed_sweep(48);
    let (u_ch, _u_rh, u_ct, u_rt) = placement_shares(&seeds, &Tuning::UNIFORM);
    let (b_ch, b_rh, b_ct, b_rt) = placement_shares(&seeds, &Tuning::BIASED);

    // Hideouts lean harder into corridors than the uniform tuning, and than
    // rooms do.
    assert!(
        b_ch > u_ch,
        "corridor hideout share should rise vs uniform: {b_ch:.4} vs {u_ch:.4}"
    );
    assert!(
        b_ch > b_rh,
        "hideouts should favour corridors over rooms: {b_ch:.4} vs {b_rh:.4}"
    );
    // Tables are room furniture, full stop — no tuning puts one in a corridor.
    assert!(
        b_ct == 0.0 && u_ct == 0.0,
        "corridors must carry no tables: biased {b_ct:.4}, uniform {u_ct:.4}"
    );
    // The #91 room preference still bites: the tighter room run-limit stamps
    // more room cover than the uniform limit does.
    assert!(
        b_rt > u_rt,
        "room table share should rise vs uniform: {b_rt:.4} vs {u_rt:.4}"
    );
    assert!(b_rt > 0.0, "rooms must still carry crouch cover");
}

#[test]
fn partitions_the_v1_config() {
    let mut rng = Rng::new(7);
    let layout = generate(40, 40, &mut rng).expect("40x40 partitions");
    assert!(
        regions_of_kind(&layout, RegionKind::Corridor) >= 1,
        "expected at least one corridor"
    );
    assert!(
        regions_of_kind(&layout, RegionKind::Room) >= 2,
        "objectives and guards need at least two rooms"
    );
}

#[test]
fn room_count_stays_within_the_budget() {
    for seed in seed_sweep(64) {
        let layout = generate(40, 40, &mut Rng::new(seed)).unwrap();
        let rooms = regions_of_kind(&layout, RegionKind::Room);
        assert!(
            rooms <= MAX_ROOMS,
            "seed {seed}: {rooms} rooms exceeds budget"
        );
    }
}

/// **[SETTLED]** — corridor width is always 2–4, never single-file. A corridor's
/// narrow bounding-box dimension is its width (throats only extend its length).
#[test]
fn corridor_width_is_always_2_to_4() {
    for seed in seed_sweep(200) {
        let layout = generate(40, 40, &mut Rng::new(seed)).unwrap();
        for (_, region) in layout.regions().regions() {
            if region.kind() == RegionKind::Corridor {
                let (w, h) = floor_bbox(layout.facility(), region.cells());
                let narrow = w.min(h);
                assert!(
                    (CORRIDOR_MIN_WIDTH..=CORRIDOR_MAX_WIDTH).contains(&narrow),
                    "seed {seed}: corridor narrow dim {narrow} outside 2..=4"
                );
            }
        }
    }
}

/// Every room's floor lane is always ≥ 6×6 (§10.1) — the thicken pass (§10.1.5)
/// may erode a wall inward, but never past the minimum ([`thinning_underruns_room`]).
#[test]
fn rooms_are_at_least_6x6() {
    for seed in seed_sweep(200) {
        let layout = generate(40, 40, &mut Rng::new(seed)).unwrap();
        for (_, region) in layout.regions().regions() {
            if region.kind() == RegionKind::Room {
                let (w, h) = floor_bbox(layout.facility(), region.cells());
                assert!(w >= 6 && h >= 6, "seed {seed}: room {w}x{h} below 6x6");
            }
        }
    }
}

/// The §10.6 guarantee, and the reason the graph exists (§10.5): every walkable
/// interior cell belongs to exactly one region, every wall to none. A recessed
/// hideout is a former *wall* cell that the cupboard pass claims for the region it
/// opens onto (§10.1.6) — it is a spot *in* that room or corridor, so cell → region
/// still answers for someone ducked inside — so the walkable interior is
/// floor-or-hideout. Nothing is "painted and forgotten".
#[test]
fn every_walkable_cell_belongs_to_exactly_one_region() {
    // Many seeds, because generation both *removes* cells from regions (a table
    // turns claimed floor into solid cover; a thickened wall eats room floor) and
    // *adds* them (a recessed cupboard claims a wall cell) — lockstep must survive
    // both.
    for seed in seed_sweep(64) {
        let layout = generate(40, 40, &mut Rng::new(seed)).unwrap();
        let facility = layout.facility();
        for y in 0..facility.height() {
            for x in 0..facility.width() {
                let terrain = facility.terrain_at(x, y);
                let walkable = terrain == Some(Terrain::Floor) || terrain == Some(Terrain::Hideout);
                let has_region = layout.regions().region_at(Cell::new(x, y)).is_some();
                assert_eq!(
                    walkable, has_region,
                    "seed {seed} ({x},{y}): walkable={walkable} but region={has_region}"
                );
            }
        }
    }
}

/// The border is enclosed unconditionally (§10.6): every border cell is wall.
#[test]
fn the_border_stays_sealed() {
    for seed in seed_sweep(200) {
        let layout = generate(40, 40, &mut Rng::new(seed)).unwrap();
        let f = layout.facility();
        for x in 0..f.width() {
            assert_eq!(f.terrain_at(x, 0), Some(Terrain::Wall));
            assert_eq!(f.terrain_at(x, f.height() - 1), Some(Terrain::Wall));
        }
        for y in 0..f.height() {
            assert_eq!(f.terrain_at(0, y), Some(Terrain::Wall));
            assert_eq!(f.terrain_at(f.width() - 1, y), Some(Terrain::Wall));
        }
    }
}

/// The headline §10.6 property: the corridor network is connected. Every
/// corridor punches into its parent, so the union of all corridor cells is a
/// single 4-connected component. Asserted over many seeds.
#[test]
fn the_corridor_network_is_always_connected() {
    // Deliberately `generate_once`: this asserts the *construction* is sound,
    // so the §10.6 gate in `generate` must not get the chance to mask a break
    // by silently rejecting and redrawing.
    for seed in seed_sweep(200) {
        let layout = generate_once(40, 40, &mut Rng::new(seed), &Tuning::BIASED).unwrap();
        assert_corridors_connected(&layout, seed);
    }
}

/// Connectivity holds across a range of footprints, not just the v1 square.
#[test]
fn connectivity_holds_across_sizes() {
    for &(w, h) in &[(18, 18), (24, 40), (40, 24), (33, 51), (60, 60)] {
        for seed in seed_sweep(40) {
            let layout = generate_once(w, h, &mut Rng::new(seed), &Tuning::BIASED).unwrap();
            assert_corridors_connected(&layout, seed);
        }
    }
}

fn assert_corridors_connected(layout: &Layout, seed: u64) {
    let corridor: HashSet<Cell> = layout
        .regions()
        .regions()
        .filter(|(_, r)| r.kind() == RegionKind::Corridor)
        .flat_map(|(_, r)| r.cells().iter().copied())
        .collect();
    assert!(!corridor.is_empty(), "seed {seed}: no corridors");

    let (w, h) = (layout.facility().width(), layout.facility().height());
    let start = *corridor.iter().next().unwrap();
    let reached = path::flood_from(start, w, h, |c| corridor.contains(&c)).len();
    assert_eq!(
        reached,
        corridor.len(),
        "seed {seed}: corridor network split into disconnected pieces"
    );
}

/// §12.4: all randomness comes from the run `Rng`, so a seed reproduces a
/// facility exactly — same grid, same regions.
#[test]
fn generation_is_deterministic() {
    let a = generate(40, 40, &mut Rng::new(2026)).unwrap();
    let b = generate(40, 40, &mut Rng::new(2026)).unwrap();
    assert_eq!(
        crate::ascii_grid(a.facility()),
        crate::ascii_grid(b.facility())
    );
    assert_eq!(a.regions().region_count(), b.regions().region_count());
}

#[test]
fn different_seeds_give_different_facilities() {
    let a = generate(40, 40, &mut Rng::new(1)).unwrap();
    let b = generate(40, 40, &mut Rng::new(2)).unwrap();
    assert_ne!(
        crate::ascii_grid(a.facility()),
        crate::ascii_grid(b.facility())
    );
}

/// #110 / §12.4: the whole **placed** run — the facility a player actually boots
/// into, not just the carve — is reproduced exactly by its seed. This is the
/// round-trip seed sharing (§13.1) leans on: two boots of the same seed through
/// the shell's and the sim's identical path (`Rng::new` → [`generate_level`] with
/// [`LevelConfig::V1`] → [`State::new`] facing north → render) render a
/// byte-identical screen — same walls, player, exit, intel and guard placement —
/// while a neighbouring seed does not. So typing the seed the sim printed, or
/// opening a `…#seed=N` link, yields the same level the bot played (§13.2).
#[test]
fn a_seed_reproduces_the_whole_placed_level() {
    let boot = |seed: u64| {
        let mut rng = Rng::new(seed);
        let (layout, placement) =
            generate_level(&LevelConfig::V1, &mut rng).expect("the v1 footprint always carves");
        let guards = placement.guards(&layout);
        let state = crate::State::new(
            layout,
            placement.player(),
            crate::Direction::North,
            guards,
            placement.intel().iter().copied(),
            placement.exit(),
        )
        .with_rng(rng);
        crate::render::render(&state).to_text().join("\n")
    };
    // Seeds are consecutive because that is exactly how `sim --bot` numbers a
    // batch (S, S+1, …): the neighbour a player is most likely to try next.
    assert_eq!(boot(8371), boot(8371), "same seed → byte-identical level");
    assert_ne!(
        boot(8371),
        boot(8372),
        "a different seed → a different level"
    );
}

/// A footprint too small to partition is rejected, not silently shipped as an
/// unplaceable single-room level (§10.2).
#[test]
fn footprints_too_small_are_rejected() {
    // Interior 8×8: no axis reaches 16.
    assert_eq!(
        generate(10, 10, &mut Rng::new(0)).unwrap_err(),
        GenError::TooSmall {
            width: 10,
            height: 10
        }
    );
    // Interior 36×5: one axis fits, but a room could never be 6 deep.
    assert_eq!(
        generate(38, 7, &mut Rng::new(0)).unwrap_err(),
        GenError::TooSmall {
            width: 38,
            height: 7
        }
    );
}

/// The smallest footprint that *does* partition: interior 16×16, exactly one
/// corridor, two rooms.
#[test]
fn the_minimum_footprint_partitions() {
    let layout = generate(18, 18, &mut Rng::new(5)).expect("18x18 partitions");
    assert!(regions_of_kind(&layout, RegionKind::Corridor) >= 1);
    assert!(regions_of_kind(&layout, RegionKind::Room) >= 2);
}

/// §10.6: every room reaches a corridor. The doorway pass (§10.1.4) must cut at
/// least one door from every room into the corridor network — a room with no
/// door is sealed, taking its future objectives and guards with it.
#[test]
fn every_room_reaches_a_corridor_through_a_door() {
    for &(w, h) in &[(18, 18), (40, 40), (24, 40), (60, 60)] {
        for seed in seed_sweep(40) {
            let layout = generate(w, h, &mut Rng::new(seed)).unwrap();
            let regions = layout.regions();
            for (id, region) in regions.regions() {
                if region.kind() != RegionKind::Room {
                    continue;
                }
                let reaches_corridor = regions
                    .neighbours(id)
                    .any(|(_, other)| regions.kind(other) == RegionKind::Corridor);
                assert!(
                    reaches_corridor,
                    "{w}x{h} seed {seed}: a room has no door to a corridor"
                );
            }
        }
    }
}

/// A room gets one to three doors, never more (`MAX_DOORS_PER_ROOM`) — a room
/// boxed in by corridors is not riddled with a door on every wall — and never
/// fewer than one, so no room is sealed off (§10.6 **[START]**).
#[test]
fn rooms_have_one_to_three_doors() {
    for &(w, h) in &[(18, 18), (40, 40), (24, 40), (60, 60)] {
        for seed in seed_sweep(64) {
            let layout = generate(w, h, &mut Rng::new(seed)).unwrap();
            let regions = layout.regions();
            for (id, region) in regions.regions() {
                if region.kind() != RegionKind::Room {
                    continue;
                }
                let doors = region.doors().len() as u32;
                assert!(
                        (1..=MAX_DOORS_PER_ROOM).contains(&doors),
                        "{w}x{h} seed {seed}: room {id:?} has {doors} doors, want 1..={MAX_DOORS_PER_ROOM}"
                    );
            }
        }
    }
}

/// Most rooms are calm — one or two doors — with three-door hubs the exception,
/// per the [`room_door_budget`] weighting **[START]**. Asserted in aggregate over
/// many seeds so the distribution, not any single room, is what's pinned.
#[test]
fn most_rooms_have_one_or_two_doors() {
    let (mut calm, mut total) = (0u32, 0u32);
    for seed in seed_sweep(200) {
        let layout = generate(40, 40, &mut Rng::new(seed)).unwrap();
        let regions = layout.regions();
        for (_, region) in regions.regions() {
            if region.kind() != RegionKind::Room {
                continue;
            }
            total += 1;
            if region.doors().len() <= 2 {
                calm += 1;
            }
        }
    }
    // Three-door rooms are rare; the overwhelming majority have one or two.
    assert!(
        calm * 100 >= total * 90,
        "only {calm}/{total} rooms have <= 2 doors; expected the vast majority"
    );
}

/// Every doorway is a valid §10.4 span of 3–6 cells on one straight wall line,
/// shaped by its kind (§10.4/#147): a **manual** door is 2 hinges around 1–4
/// panels, an **automatic** door is 3–6 panels and no hinges (the frameless span).
#[test]
fn doorways_are_well_formed_spans() {
    for seed in seed_sweep(64) {
        let layout = generate(40, 40, &mut Rng::new(seed)).unwrap();
        for (_, door) in layout.regions().doors() {
            match door.kind() {
                DoorKind::Manual => {
                    assert_eq!(door.hinges().len(), 2, "seed {seed}: a hinge at each end");
                    let panels = door.panels().len();
                    assert!(
                        (1..=4).contains(&panels),
                        "seed {seed}: {panels} panels, want 1..=4"
                    );
                }
                DoorKind::Automatic { delay } => {
                    assert!(
                        door.hinges().is_empty(),
                        "seed {seed}: automatic: no hinges"
                    );
                    let panels = door.panels().len();
                    assert!(
                        (3..=6).contains(&panels),
                        "seed {seed}: {panels} panels, want 3..=6"
                    );
                    assert_eq!(AUTO_CLOSE_DELAY, 5, "the [START] auto-close delay");
                    assert_eq!(delay, AUTO_CLOSE_DELAY, "seed {seed}: the [START] delay");
                }
            }
            let total = door.cells().count();
            assert!(
                (3..=6).contains(&total),
                "seed {seed}: {total} cells, want a 3..=6 span"
            );
            let cells: Vec<Cell> = door.cells().collect();
            let straight =
                cells.iter().all(|c| c.x == cells[0].x) || cells.iter().all(|c| c.y == cells[0].y);
            assert!(
                straight,
                "seed {seed}: a door must lie on one straight line"
            );
        }
    }
}

/// §10.4/#147: generation produces *both* door kinds — most manual, a minority
/// automatic (the [`AUTO_DOOR_PERCENT`] share) — and the split is deterministic
/// per seed (§12.4). Asserted in aggregate so the distribution, not one door, is
/// what's pinned.
#[test]
fn generation_produces_both_door_kinds_deterministically() {
    assert_eq!(AUTO_DOOR_PERCENT, 30, "the [START] automatic-door share");
    let (mut manual, mut automatic) = (0u32, 0u32);
    for seed in seed_sweep(200) {
        let a = generate(40, 40, &mut Rng::new(seed)).unwrap();
        let b = generate(40, 40, &mut Rng::new(seed)).unwrap();
        let kinds = |l: &Layout| -> Vec<bool> {
            l.regions().doors().map(|(_, d)| d.is_automatic()).collect()
        };
        assert_eq!(
            kinds(&a),
            kinds(&b),
            "seed {seed}: door kinds are deterministic"
        );
        for (_, door) in a.regions().doors() {
            if door.is_automatic() {
                automatic += 1;
            } else {
                manual += 1;
            }
        }
    }
    assert!(automatic > 0, "some doors are automatic");
    assert!(manual > automatic, "but most doors are manual");
}

/// #145: in a *placed* level a deterministic share of doorways starts open, and
/// the graph pose and panel terrain are stamped together — an open door reads
/// `DoorPanelOpen`, a closed one `DoorPanelClosed`, never a mismatch, and hinges
/// stay solid whatever the pose (§10.4). Same seed → the same open doors (§12.4).
#[test]
fn some_doors_start_open_deterministically_and_stamped_together() {
    for seed in seed_sweep(64) {
        let (a, _) = generate_level(&LevelConfig::V1, &mut Rng::new(seed)).unwrap();
        let (b, _) = generate_level(&LevelConfig::V1, &mut Rng::new(seed)).unwrap();

        // Determinism: the same seed opens exactly the same doors.
        let poses_a: Vec<bool> = a.regions().doors().map(|(_, d)| d.is_open()).collect();
        let poses_b: Vec<bool> = b.regions().doors().map(|(_, d)| d.is_open()).collect();
        assert_eq!(
            poses_a, poses_b,
            "seed {seed}: open set is not deterministic"
        );

        // Graph pose and grid terrain agree, cell for cell.
        for (_, door) in a.regions().doors() {
            let want = if door.is_open() {
                Terrain::DoorPanelOpen
            } else {
                Terrain::DoorPanelClosed
            };
            for &p in door.panels() {
                assert_eq!(
                    a.facility().terrain(p),
                    Some(want),
                    "seed {seed}: door pose and panel terrain disagree at {p:?}",
                );
            }
            for &h in door.hinges() {
                assert_eq!(
                    a.facility().terrain(h),
                    Some(Terrain::DoorHinge),
                    "seed {seed}: a hinge is not solid",
                );
            }
        }
    }
}

/// #145: a named [START] fraction (~20%) of doorways starts open — reliably some
/// open and some closed, in the neighbourhood of [`OPEN_DOOR_PERCENT`]. The knob
/// itself is pinned so a retune is a visible decision, not a silent drift.
#[test]
fn about_a_fifth_of_doors_start_open() {
    assert_eq!(
        OPEN_DOOR_PERCENT, 20,
        "the [START] open-door share is pinned"
    );

    let (mut open, mut total) = (0u32, 0u32);
    for seed in seed_sweep(128) {
        let (layout, _) = generate_level(&LevelConfig::V1, &mut Rng::new(seed)).unwrap();
        for (_, door) in layout.regions().doors() {
            total += 1;
            open += u32::from(door.is_open());
        }
    }
    assert!(total > 0, "the sweep generated no doors");
    assert!(open > 0, "no door ever started open across the sweep");
    assert!(open < total, "every door started open across the sweep");
    let frac = f64::from(open) / f64::from(total);
    assert!(
        (0.08..0.36).contains(&frac),
        "open share {frac:.3} strays far from the ~20% [START] target ({open}/{total})",
    );
}

/// The bare (unplaced) carve is still all-closed — opening is a placement-time
/// state layer (#145), so `generate` stays the canonical closed-door primitive
/// the door-mechanics tests build on. (The placed path opens doors; see above.)
#[test]
fn the_bare_carve_leaves_every_door_closed() {
    for seed in seed_sweep(64) {
        let layout = generate(40, 40, &mut Rng::new(seed)).unwrap();
        for (_, door) in layout.regions().doors() {
            assert!(!door.is_open(), "seed {seed}: a bare carve opened a door");
        }
    }
}

/// Whether a set of cells is a single 4-connected component.
fn is_4_connected(cells: &HashSet<Cell>) -> bool {
    let start = match cells.iter().next() {
        Some(&c) => c,
        None => return true,
    };
    // Bound the flood grid to just past the set's extent; membership *is* the
    // passability predicate.
    let w = cells.iter().map(|c| c.x).max().unwrap() + 1;
    let h = cells.iter().map(|c| c.y).max().unwrap() + 1;
    path::flood_from(start, w, h, |c| cells.contains(&c)).len() == cells.len()
}

/// A feature never seals a room: every room region's floor stays a single
/// 4-connected component. This is the operational form of the §10.1.5 "footprint
/// grown by 1 is clear" rule — a partition wall or pillar keeps its 1-cell moat,
/// so no pocket of floor is ever cut off (which would also break reachability, #13).
#[test]
fn room_floor_stays_connected_after_features() {
    for seed in seed_sweep(200) {
        let layout = generate(40, 40, &mut Rng::new(seed)).unwrap();
        for (id, region) in layout.regions().regions() {
            if region.kind() != RegionKind::Room {
                continue;
            }
            let cells: HashSet<Cell> = region.cells().iter().copied().collect();
            assert!(
                is_4_connected(&cells),
                "seed {seed}: room {id:?} floor split into pieces by a feature"
            );
        }
    }
}

/// The §10.5 payoff: a featured room records its *true* footprint, not its
/// bounding box. Over many seeds the vast majority of rooms end up with fewer
/// floor cells than their bounding-box area — a genuine non-rectangular shape
/// carved by a feature — proving the region graph reflects it (§10.1.5, §10.5).
#[test]
fn features_make_rooms_non_rectangular() {
    let (mut carved, mut total) = (0u32, 0u32);
    for seed in seed_sweep(200) {
        let layout = generate(40, 40, &mut Rng::new(seed)).unwrap();
        for (_, region) in layout.regions().regions() {
            if region.kind() != RegionKind::Room {
                continue;
            }
            total += 1;
            let (w, h) = bbox(region.cells());
            if (region.cells().len() as u32) < w * h {
                carved += 1;
            }
        }
    }
    // Every room is ≥6×6, so a partition wall always fits and a pillar usually
    // does — the overwhelming majority should carry a feature.
    assert!(
        carved * 100 >= total * 80,
        "only {carved}/{total} rooms are non-rectangular; features are barely landing"
    );
}

/// Feature walls stay strictly interior — a feature is stamped inside a room and
/// never onto the enclosing border (§10.6), which the border-seal test also
/// guards from the other direction.
#[test]
fn features_never_touch_the_border() {
    for seed in seed_sweep(200) {
        let layout = generate(40, 40, &mut Rng::new(seed)).unwrap();
        let f = layout.facility();
        for x in 0..f.width() {
            assert_eq!(f.terrain_at(x, 0), Some(Terrain::Wall));
            assert_eq!(f.terrain_at(x, f.height() - 1), Some(Terrain::Wall));
        }
        for y in 0..f.height() {
            assert_eq!(f.terrain_at(0, y), Some(Terrain::Wall));
            assert_eq!(f.terrain_at(f.width() - 1, y), Some(Terrain::Wall));
        }
    }
}

/// A pillar needs a 6×6 room (§10.1.5): below that there is no space for a
/// 2-cell block plus the freestanding 1-cell margin. A 5×5 room never yields
/// one; a 6×6 room can.
#[test]
fn pillars_need_a_six_by_six_room() {
    // A 5×5 interior: propose_pillar rejects every draw.
    let small = Facility::walled_box(7, 7);
    let small_room = Rect::new(1, 1, 5, 5);
    for seed in 0..64 {
        assert!(
            propose_pillar(&small_room, &small, &mut Rng::new(seed)).is_none(),
            "a 5x5 room must never host a pillar"
        );
    }
    // A 6×6 interior: at least one draw fits.
    let big = Facility::walled_box(8, 8);
    let big_room = Rect::new(1, 1, 6, 6);
    assert!(
        (0..64).any(|seed| propose_pillar(&big_room, &big, &mut Rng::new(seed)).is_some()),
        "a 6x6 room should be able to host a pillar"
    );
}

/// A proposed feature always respects its clearance: every stub/pillar cell and
/// its checked halo lies on floor inside the room. Asserted directly on the
/// proposals for a fresh room so a regression in the "grown by 1 is clear" rule
/// (§10.1.5) shows up close to the code that enforces it.
#[test]
fn proposed_features_stay_on_interior_floor() {
    let facility = Facility::walled_box(20, 20);
    let room = Rect::new(1, 1, 18, 18);
    for seed in 0..128 {
        let mut rng = Rng::new(seed);
        for _ in 0..FEATURE_ATTEMPTS {
            for proposal in [
                propose_partition(&room, &facility, &mut rng),
                propose_pillar(&room, &facility, &mut rng),
            ]
            .into_iter()
            .flatten()
            {
                for cell in proposal {
                    assert!(
                        room.contains(cell) && facility.terrain(cell) == Some(Terrain::Floor),
                        "seed {seed}: feature cell {cell:?} is off the room floor"
                    );
                }
            }
        }
    }
}

/// Every cell that renders as a hideout in a layout.
fn hideout_cells(layout: &Layout) -> Vec<Cell> {
    let f = layout.facility();
    (0..f.height())
        .flat_map(|y| (0..f.width()).map(move |x| Cell::new(x, y)))
        .filter(|&c| f.terrain(c) == Some(Terrain::Hideout))
        .collect()
}

/// The cover cells of a layout, grouped into 4-connected clusters — returns
/// `(total tables, cluster count, largest cluster)`. A lone stamp is a
/// one-cell cluster; a bench is a multi-cell one.
fn cover_clustering(layout: &Layout) -> (u32, u32, u32) {
    let f = layout.facility();
    let mut seen: HashSet<Cell> = HashSet::new();
    let (mut tables, mut clusters, mut largest) = (0u32, 0u32, 0u32);
    for y in 0..f.height() {
        for x in 0..f.width() {
            let c = Cell::new(x, y);
            if f.terrain(c) != Some(Terrain::PartialCover) {
                continue;
            }
            tables += 1;
            if !seen.insert(c) {
                continue;
            }
            clusters += 1;
            let (mut size, mut stack) = (0u32, vec![c]);
            while let Some(p) = stack.pop() {
                size += 1;
                for nb in f.neighbours(p) {
                    if f.terrain(nb) == Some(Terrain::PartialCover) && seen.insert(nb) {
                        stack.push(nb);
                    }
                }
            }
            largest = largest.max(size);
        }
    }
    (tables, clusters, largest)
}

/// #74: the §10.1a repair used to drop a lone `π` per over-long run — scattered
/// confetti. Now each placement extends into a **bench** across the space, so the
/// same cover reads as far fewer, organized pieces. Asserted in aggregate: distinct
/// cover clusters are markedly fewer than cover cells (benches formed), and a
/// multi-cell bench genuinely appears.
#[test]
fn cover_clusters_into_benches() {
    let seeds = seed_sweep(200);
    let (mut tables, mut clusters, mut largest) = (0u32, 0u32, 0u32);
    for &seed in &seeds {
        let (t, c, l) = cover_clustering(&generate(40, 40, &mut Rng::new(seed)).unwrap());
        tables += t;
        clusters += c;
        largest = largest.max(l);
    }
    assert!(tables > 0, "no cover placed at all");
    assert!(
        clusters * 10 < tables * 9,
        "cover barely clusters: {clusters} clusters over {tables} cells — benches are not forming"
    );
    assert!(
        largest >= 2,
        "no bench longer than a single cell formed over the sweep"
    );
}

/// The bench-length knobs stay sane §10.1a **[START]** values — a bench is at
/// least two cells (a lone table is the confetti the mechanism exists to
/// kill), and not so long it walls a wide space (`COVER_BENCH_MAX`).
#[test]
fn the_bench_cap_is_a_sane_start_value() {
    assert!(
        (2..=6).contains(&COVER_BENCH_MAX),
        "COVER_BENCH_MAX {COVER_BENCH_MAX} left the sane 2..=6 range"
    );
    assert!(
        (2..=COVER_BENCH_MAX).contains(&COVER_BENCH_MIN),
        "COVER_BENCH_MIN {COVER_BENCH_MIN} left the 2..=COVER_BENCH_MAX range"
    );
}

/// The bench rules as a per-level property (§10.1.6): every stamped table
/// belongs to a **bench** — a straight row of [`COVER_BENCH_MIN`]..=
/// [`COVER_BENCH_MAX`] cells, never a lone cell — every bench is **room
/// furniture** (each cell borders room floor and never corridor floor), and
/// every bench sits in a furniture pose ([`bench_pose`]): free-standing,
/// end-on to a wall, or flush along one.
#[test]
fn benches_are_room_furniture_in_furniture_poses() {
    for seed in seed_sweep(200) {
        let layout = generate(40, 40, &mut Rng::new(seed)).unwrap();
        let (f, g) = (layout.facility(), layout.regions());
        let mut seen: HashSet<Cell> = HashSet::new();
        for y in 0..f.height() {
            for x in 0..f.width() {
                let c = Cell::new(x, y);
                if f.terrain(c) != Some(Terrain::PartialCover) || !seen.insert(c) {
                    continue;
                }
                let mut bench = vec![c];
                let mut stack = vec![c];
                while let Some(p) = stack.pop() {
                    for nb in f.neighbours(p) {
                        if f.terrain(nb) == Some(Terrain::PartialCover) && seen.insert(nb) {
                            bench.push(nb);
                            stack.push(nb);
                        }
                    }
                }

                let len = bench.len() as u32;
                assert!(
                    (COVER_BENCH_MIN..=COVER_BENCH_MAX).contains(&len),
                    "seed {seed}: bench at {c:?} has {len} cells"
                );
                assert!(
                    bench.iter().all(|p| p.x == c.x) || bench.iter().all(|p| p.y == c.y),
                    "seed {seed}: bench at {c:?} is not one straight row"
                );
                assert!(
                    bench_pose(f, &bench).is_some(),
                    "seed {seed}: bench at {c:?} sits in no furniture pose"
                );
                // Room furniture: the bench opens onto room floor somewhere
                // (an along-wall piece slotted into a niche may have interior
                // cells touching only walls), and no cell of it ever borders
                // corridor floor.
                let kinds: Vec<RegionKind> = bench
                    .iter()
                    .flat_map(|&p| f.neighbours(p))
                    .filter_map(|n| g.region_at(n).map(|id| g.kind(id)))
                    .collect();
                assert!(
                    kinds.contains(&RegionKind::Room),
                    "seed {seed}: bench at {c:?} borders no room floor"
                );
                assert!(
                    !kinds.contains(&RegionKind::Corridor),
                    "seed {seed}: bench at {c:?} borders a corridor — tables are room furniture"
                );
            }
        }
    }
}

/// The number of floor cells flanked by two or more tables in a layout — the
/// §11.4 doubled-crouch clutter the cover pass tries to avoid (#75).
fn doubled_crouch_cells(layout: &Layout) -> u32 {
    let f = layout.facility();
    let mut doubles = 0;
    for y in 0..f.height() {
        for x in 0..f.width() {
            let c = Cell::new(x, y);
            if f.terrain(c) == Some(Terrain::Floor)
                && f.neighbours(c)
                    .filter(|&n| f.terrain(n) == Some(Terrain::PartialCover))
                    .count()
                    >= 2
            {
                doubles += 1;
            }
        }
    }
    doubles
}

/// #75: two tables flanking one floor cell put the *same* `crouch` hint on it
/// twice (§11.4). Both the bench seed and its extension steer around that, so
/// doubled-crouch cells stay rare — the residual (~0.13/level over the full sweep)
/// is forced seeds and the odd spot where a bench meets a crossing one. This pins
/// the preference is working, not that it is a hard guarantee (§11.4 keeps it a
/// preference — the arrow disambiguates any survivor); the gross-regression class
/// (a bench that over-covers into a haze of doubles) ran ~15/level and is caught.
#[test]
fn cover_rarely_doubles_the_crouch_hint() {
    let seeds = seed_sweep(1000);
    let doubles: u32 = seeds
        .iter()
        .map(|&seed| doubled_crouch_cells(&generate(40, 40, &mut Rng::new(seed)).unwrap()))
        .sum();
    // Ceiling of 0.3 doubled cells per level — ~2× the measured rate, floored so a
    // thin fast-mode sample never flakes, and far under the ~15/level regression class.
    let budget = (seeds.len() as u32 * 3 / 10).max(3);
    assert!(
            doubles <= budget,
            "{doubles} doubled-crouch cells over {} seeds (budget {budget}) — the §11.4 table preference has degraded",
            seeds.len()
        );
}

/// [`creates_table_double`] fires only when a candidate table would share a floor
/// neighbour with an existing one — the exact table+table adjacency #75 avoids,
/// and nothing else (a lone table, or one across the room, is fine).
#[test]
fn a_table_double_needs_a_shared_floor_neighbour() {
    let mut f = Facility::walled_box(8, 8);
    f.set_terrain(3, 3, Terrain::PartialCover);
    // (3,5)'s north neighbour (3,4) is floor and already borders the (3,3) table.
    assert!(creates_table_double(&f, Cell::new(3, 5)));
    // A candidate sharing no floor neighbour with any table does not double.
    assert!(!creates_table_double(&f, Cell::new(6, 6)));
}

/// The hiding game needs a *board* (§10.1a): the v1 config gets a healthy spread
/// of hideouts every seed, not the one-or-none the old harvester produced.
#[test]
fn the_v1_config_gets_a_board_of_hideouts() {
    for seed in seed_sweep(200) {
        let layout = generate(40, 40, &mut Rng::new(seed)).unwrap();
        let count = hideout_cells(&layout).len();
        assert!(
            count >= 6,
            "seed {seed}: only {count} hideouts — not a board"
        );
    }
}

/// The headline §10.1a fix: hideouts land on the corridor network, not only in
/// rooms — the flight path is exactly where the player needs cover. Asserted per
/// seed, since a chase can happen in any corridor.
#[test]
fn hideouts_land_on_corridors() {
    for seed in seed_sweep(200) {
        let layout = generate(40, 40, &mut Rng::new(seed)).unwrap();
        let regions = layout.regions();
        let on_corridor = hideout_cells(&layout).into_iter().any(|c| {
            regions
                .region_at(c)
                .is_some_and(|id| regions.kind(id) == RegionKind::Corridor)
        });
        assert!(on_corridor, "seed {seed}: no hideout on any corridor");
    }
}

/// Every hideout is a **flush recess** (§10.1.6): a wall-line cell with **exactly
/// one floor neighbour** — the mouth the player bumps it from — and **three solid
/// wall neighbours**. The three walls are the safety guarantee: the cupboard is
/// backed and flanked, so it can be neither walked nor seen *through* to the far
/// side, and it never clogs a door throat (a door cell on a flank would break the
/// exactly-three-wall count). This is the geometry the thicken pass and the pillars
/// manufacture.
#[test]
fn every_hideout_is_a_flush_recess() {
    for seed in seed_sweep(200) {
        let layout = generate(40, 40, &mut Rng::new(seed)).unwrap();
        let f = layout.facility();
        for c in hideout_cells(&layout) {
            let neighbours: Vec<Terrain> = f.neighbours(c).filter_map(|n| f.terrain(n)).collect();
            assert_eq!(
                neighbours.len(),
                4,
                "seed {seed}: hideout {c:?} is on the border, not an interior recess"
            );
            let floors = neighbours.iter().filter(|&&t| t == Terrain::Floor).count();
            let walls = neighbours.iter().filter(|&&t| t == Terrain::Wall).count();
            assert_eq!(
                (floors, walls),
                (1, 3),
                "seed {seed}: hideout {c:?} is not a flush recess (1 floor mouth + 3 wall), \
                     neighbours {neighbours:?}"
            );
        }
    }
}

/// The cupboard's **mouth**, and the two **back diagonals** behind it — the pair
/// of cells the cardinal neighbour scan never examines (#361).
fn cupboard_mouth_and_back_diagonals(f: &Facility, c: Cell) -> (Cell, [Cell; 2]) {
    let mouth = f
        .neighbours(c)
        .find(|&n| f.terrain(n) == Some(Terrain::Floor))
        .expect("a cupboard has exactly one floor mouth");
    let back = Direction::between(c, mouth)
        .expect("the mouth is a cardinal neighbour")
        .opposite();
    let behind = c.step(back).expect("an interior recess has a backing cell");
    let [left, right] = back.perpendicular();
    (
        mouth,
        [
            behind.step(left).expect("an interior backing has flanks"),
            behind.step(right).expect("an interior backing has flanks"),
        ],
    )
}

/// A cupboard is **fully backed — diagonals included** (§10.1.6, #361): it sits in
/// a 2×3 block of solid wall, its own cell excepted. Three solid *cardinal* sides
/// (pinned by [`every_hideout_is_a_flush_recess`]) leave the two back diagonals
/// unexamined, and where the backing course is only locally thick one of them is
/// floor of the space behind — which the always-seen touching ring (§6.1
/// **[SETTLED]**) then hands to the player as a peephole into a room the run had
/// not earned (§11.5a).
#[test]
fn a_cupboard_is_backed_on_its_diagonals_too() {
    for seed in seed_sweep(200) {
        let layout = generate(40, 40, &mut Rng::new(seed)).unwrap();
        let f = layout.facility();
        for c in hideout_cells(&layout) {
            let (_, diagonals) = cupboard_mouth_and_back_diagonals(f, c);
            for d in diagonals {
                assert_eq!(
                    f.terrain(d),
                    Some(Terrain::Wall),
                    "seed {seed}: cupboard {c:?} has a hollow back diagonal at {d:?}"
                );
            }
        }
    }
}

/// Ducked into a cupboard, the player sees nothing their **mouth** cannot already
/// see, bar solid wall faces — layout, which §11.5a lets them have. The recess is
/// no window of its own: what it shows is the view from the cell they stepped in
/// from, narrowed by the mouth.
///
/// Cast at **every** facing with the widest arc (the 360° of a waiting turn, §8.3)
/// and through [`field_of_view_with_peek`], so this covers the auto-peek as well as
/// the ring — including the facings where the lean origin would fall *inside* the
/// backing or flank wall. It is the assertion, not the argument, that settles those
/// (#361).
#[test]
fn a_cupboard_shows_no_more_than_its_mouth_does() {
    for seed in seed_sweep(200) {
        let layout = generate(40, 40, &mut Rng::new(seed)).unwrap();
        let f = layout.facility();
        for c in hideout_cells(&layout) {
            let (mouth, _) = cupboard_mouth_and_back_diagonals(f, c);
            let from_mouth: HashSet<Cell> = Direction::ALL
                .into_iter()
                .flat_map(|facing| {
                    field_of_view_with_peek(f, mouth, facing, FULL_SIGHT_ARC, PLAYER_SIGHT_RANGE)
                        .cells()
                        .collect::<Vec<_>>()
                })
                .collect();
            for facing in Direction::ALL {
                let fov = field_of_view_with_peek(f, c, facing, FULL_SIGHT_ARC, PLAYER_SIGHT_RANGE);
                for seen in fov.cells() {
                    let solid = f.terrain(seen).is_some_and(|t| t.blocks_sight());
                    assert!(
                        solid || from_mouth.contains(&seen),
                        "seed {seed}: cupboard {c:?} facing {facing:?} sees {seen:?}, \
                             which its mouth {mouth:?} cannot"
                    );
                }
            }
        }
    }
}

/// [`recess_site`] reads the **back diagonals**, not just the cardinal sides
/// (#361). The fixture is the exact geometry that leaked: a two-course wall whose
/// second course is one cell short at each end, so a candidate mid-course is fully
/// backed while the one over the short end has a room cell on a back diagonal —
/// same three solid sides, one peephole.
///
/// ```text
///   . . . . . . .      row 3 — corridor floor (the mouths)
///   # # # # # # #      row 4 — the wall line the cupboards recess into
///   . # # # # # .      row 5 — the backing course, one short at each end
///   . . . . . . .      row 6 — room floor beyond
/// ```
#[test]
fn a_recess_site_needs_its_back_diagonals_too() {
    let mut f = Facility::walled_box(9, 9);
    for x in 1..8 {
        f.set_terrain(x, 4, Terrain::Wall);
    }
    for x in 2..7 {
        f.set_terrain(x, 5, Terrain::Wall);
    }
    // Mid-course: three solid sides and both back diagonals solid.
    assert_eq!(recess_site(&f, Cell::new(4, 4)), Some(Cell::new(4, 3)));
    // Over the short end: still three solid sides, but the back diagonal (1,5) is
    // room floor — the leak. Rejected now, accepted before #361.
    assert_eq!(f.terrain(Cell::new(1, 5)), Some(Terrain::Floor));
    assert_eq!(recess_site(&f, Cell::new(2, 4)), None);
    // Extend the backing course under it and the same cell qualifies.
    f.set_terrain(1, 5, Terrain::Wall);
    assert_eq!(recess_site(&f, Cell::new(2, 4)), Some(Cell::new(2, 3)));
}

/// The flight path is where cover is needed (§10.1a/§7.6), so a **large** corridor
/// — one long enough for a chase to play out in — nearly always carries a cupboard.
/// Pinned as a budget over the sweep rather than per seed: the sightline rule is
/// what *guarantees* counterplay on a long run (a table or an obstruction also
/// count), and this is the stronger hiding-game property riding above it.
///
/// Measured 88% when #361 tightened the site test and [`WALL_THICKEN_ONE_IN`] rose
/// to compensate (81% at the old thickening rate, 98% before the fix — on sites
/// that were peepholes). Budgeted at 80%, so an erosion of the board shows up here
/// rather than in play.
#[test]
fn large_corridors_nearly_always_carry_a_cupboard() {
    let (mut large, mut served) = (0u32, 0u32);
    for seed in seed_sweep(200) {
        let layout = generate(40, 40, &mut Rng::new(seed)).unwrap();
        let (f, regions) = (layout.facility(), layout.regions());
        for (_, region) in regions.regions() {
            if region.kind() != RegionKind::Corridor || region.cells().len() < 24 {
                continue;
            }
            large += 1;
            served += u32::from(
                region
                    .cells()
                    .iter()
                    .any(|&c| f.terrain(c) == Some(Terrain::Hideout)),
            );
        }
    }
    assert!(
        served * 10 >= large * 8,
        "only {served}/{large} large corridors carry a cupboard — the flight paths are going bare"
    );
}

/// A corridor-facing cupboard is proof the thicken pass (§10.1.5) did structural
/// work: a bare corridor flank is one cell thick, so its wall cell has a corridor
/// floor on one side *and a room floor on the other* — two floor neighbours, which
/// [`recess_site`] rejects. A cupboard can open onto a corridor **only** where the
/// flank was thickened to two, giving the wall cell a solid back. So a corridor
/// hideout, backed by wall, with room floor two steps in, could not exist without
/// the pass. Asserted in aggregate over the sweep.
#[test]
fn corridor_cupboards_require_a_thickened_wall() {
    let mut found = 0;
    for seed in seed_sweep(200) {
        let layout = generate(40, 40, &mut Rng::new(seed)).unwrap();
        let (f, regions) = (layout.facility(), layout.regions());
        for c in hideout_cells(&layout) {
            let opens_on_corridor = regions
                .region_at(c)
                .is_some_and(|id| regions.kind(id) == RegionKind::Corridor);
            if !opens_on_corridor {
                continue;
            }
            // The mouth is the sole floor neighbour; the backing is opposite it.
            let mouth = f
                .neighbours(c)
                .find(|&n| f.terrain(n) == Some(Terrain::Floor))
                .unwrap();
            let (dx, dy) = (c.x as i32 - mouth.x as i32, c.y as i32 - mouth.y as i32);
            let backing = Cell::new((c.x as i32 + dx) as u32, (c.y as i32 + dy) as u32);
            assert_eq!(
                f.terrain(backing),
                Some(Terrain::Wall),
                "seed {seed}: corridor cupboard {c:?} is not solidly backed"
            );
            found += 1;
        }
    }
    assert!(
        found > 0,
        "no corridor cupboards over the sweep — the thicken pass is not producing backing"
    );
}

/// Cupboards are spread, not banked. The [`place_hideouts`] pass enforces the
/// spacing knobs outright; the §10.1a corridor repair ([`recess_run_hideout`])
/// treats them as a preference — a flight path's run must break even where the
/// only site is close to an existing cupboard (§10.1a: "a flight path with no
/// hideout on it is a failed flight path"). Two properties survive that:
///
/// - a **structural floor** — no two hideouts within Manhattan 2 of each
///   other, which the [`recess_site`] three-solid-walls geometry makes
///   impossible (a hideout flanking the candidate fails the wall count), so a
///   cupboard's backing is never itself hollowed out;
/// - **statistically spread** — pairs closer than the corridor spacing stay a
///   small fraction of all pairs (measured ~1.7% when the repair landed;
///   budgeted at 4%), so the board never rots into a honeycomb.
#[test]
fn hideouts_keep_their_spacing() {
    let (mut pairs, mut close) = (0u64, 0u64);
    for seed in seed_sweep(200) {
        let cells = hideout_cells(&generate(40, 40, &mut Rng::new(seed)).unwrap());
        for (i, &a) in cells.iter().enumerate() {
            for &b in &cells[i + 1..] {
                let d = a.manhattan_distance(b);
                assert!(
                    d >= 2,
                    "seed {seed}: hideouts {a:?} and {b:?} share backing"
                );
                pairs += 1;
                close += u64::from(d < HIDEOUT_MIN_SPACING_CORRIDOR);
            }
        }
    }
    assert!(
        close * 25 <= pairs,
        "{close}/{pairs} hideout pairs closer than the corridor spacing — the board is banking up"
    );
}

/// A hideout blocks pathing (§10.3), so the board must never wall a patrol route
/// off: the pathable space (everything a guard can route through, hideouts
/// excluded) stays a single connected component. This is what [`severs_pathing`]
/// guarantees, and it protects the reachability the placement ticket asserts (#13).
#[test]
fn hideouts_keep_guard_pathing_connected() {
    // `generate_once`, not `generate`: the §10.6 gate checks this exact
    // property, so going through the entry point would mask a regression in
    // `severs_pathing` as silent rejections instead of a red test.
    for seed in seed_sweep(200) {
        let layout = generate_once(40, 40, &mut Rng::new(seed), &Tuning::BIASED).unwrap();
        let f = layout.facility();
        let pathable: HashSet<Cell> = (0..f.height())
            .flat_map(|y| (0..f.width()).map(move |x| Cell::new(x, y)))
            .filter(|&c| f.terrain(c).is_some_and(|t| !t.blocks_pathing()))
            .collect();
        assert!(
            is_4_connected(&pathable),
            "seed {seed}: hideouts split guard pathing"
        );
    }
}

/// The longest counterplay-free straight run in the grid, measured
/// independently of the generator's own scanner: walk every row and column
/// counting consecutive cells that neither block sight, nor provide cover,
/// nor have a cupboard within two moves — the §10.1a measure (a table is
/// see-through but plants the crouch; a mouth is see-past but a bump from
/// vanishing — both are the counterplay the rule demands).
fn longest_straight_run(f: &Facility) -> u32 {
    let (w, h) = (f.width(), f.height());
    let mouth = |c: Cell| {
        f.neighbours(c)
            .any(|n| f.terrain(n) == Some(Terrain::Hideout))
    };
    let clear = |x: u32, y: u32| {
        let c = Cell::new(x, y);
        f.terrain_at(x, y)
            .is_some_and(|t| !t.blocks_sight() && !t.provides_cover())
            && !mouth(c)
            && !f
                .neighbours(c)
                .any(|n| f.terrain(n) == Some(Terrain::Floor) && mouth(n))
    };
    let mut longest = 0u32;
    for y in 0..h {
        let mut run = 0;
        for x in 0..w {
            run = if clear(x, y) { run + 1 } else { 0 };
            longest = longest.max(run);
        }
    }
    for x in 0..w {
        let mut run = 0;
        for y in 0..h {
            run = if clear(x, y) { run + 1 } else { 0 };
            longest = longest.max(run);
        }
    }
    longest
}

/// The headline §10.1a property: **no unbroken straight sightline longer than
/// L**, for every cell in each of the 4 cardinal directions — equivalently, no
/// maximal row or column run exceeds [`SIGHTLINE_MAX_RUN`]. Asserted on
/// [`generate`]'s accepted layouts across footprints: like reachability, the
/// rule is "repaired or the seed rejected" (§10.1a), so acceptance is where it
/// is unconditional.
#[test]
fn no_sightline_exceeds_the_cap() {
    for &(w, h) in &[(18, 18), (40, 40), (24, 40), (60, 60)] {
        for seed in seed_sweep(64) {
            let layout = generate(w, h, &mut Rng::new(seed)).unwrap();
            let run = longest_straight_run(layout.facility());
            assert!(
                run <= SIGHTLINE_MAX_RUN,
                "{w}x{h} seed {seed}: a {run}-cell sightline on an accepted level"
            );
        }
    }
}

/// In a **room**, the §10.1a repair is **furniture, not wall** (#52): the
/// pass stamps tables and only tables, so a room blocker never reads as a
/// floating wall cell. Driven on a bare gallery where every stamp must come
/// from the cover pass — and the crouch trade is visible in the terrain: the
/// gallery satisfies the counterplay measure while staying *optically* open
/// end to end (a guard still sees straight over every table).
#[test]
fn the_cover_pass_stamps_tables_not_walls_in_a_room() {
    let mut f = Facility::walled_box(30, 8);
    // Claim the interior as one room region, as the real partition would:
    // the pass releases each stamped cell, and only owned cells release.
    let mut regions = RegionGraph::new(30, 8);
    regions.add_region(RegionKind::Room, Rect::new(1, 1, 28, 6).cells());
    break_sightlines(&mut f, &mut regions, &mut Rng::new(7), &Tuning::BIASED);

    assert!(sightlines_bounded(&f), "the gallery must be repaired");
    let mut tables = 0;
    for y in 1..f.height() - 1 {
        for x in 1..f.width() - 1 {
            match f.terrain_at(x, y) {
                Some(Terrain::Floor) => {}
                Some(Terrain::PartialCover) => tables += 1,
                t => panic!("({x},{y}): the cover pass stamped {t:?}"),
            }
        }
    }
    assert!(tables > 0, "a 28-cell gallery cannot pass uncovered");

    // Optically the interior is still one open span per row: no stamped cell
    // blocks sight, so the pure-opacity run down row 1 spans the full 28.
    let opacity_run = (1..f.width() - 1)
        .take_while(|&x| f.terrain_at(x, 1).is_some_and(|t| !t.blocks_sight()))
        .count() as u32;
    assert_eq!(opacity_run, f.width() - 2, "tables must not cast shadows");
}

/// In a **corridor**, the §10.1a repair is architecture, never furniture
/// (§10.1.6): the pass recesses cupboards or raises structural pillars, and
/// no table ever lands. Driven on the same bare gallery claimed as a corridor
/// — walled 1-thick all round, so the first repairs must be pillars (no recess
/// backing exists yet; a later repair may then recess into a pillar it built).
#[test]
fn the_cover_pass_never_stamps_a_table_in_a_corridor() {
    let mut f = Facility::walled_box(30, 8);
    let mut regions = RegionGraph::new(30, 8);
    regions.add_region(RegionKind::Corridor, Rect::new(1, 1, 28, 6).cells());
    break_sightlines(&mut f, &mut regions, &mut Rng::new(7), &Tuning::BIASED);

    assert!(sightlines_bounded(&f), "the gallery must be repaired");
    let mut repairs = 0;
    for y in 1..f.height() - 1 {
        for x in 1..f.width() - 1 {
            match f.terrain_at(x, y) {
                Some(Terrain::Floor) => {}
                // Pillar wall, or a cupboard recessed into a pillar's backing —
                // both architecture. What must never appear is a table.
                Some(Terrain::Wall | Terrain::Hideout) => repairs += 1,
                t => panic!("({x},{y}): a corridor repair stamped {t:?}"),
            }
        }
    }
    assert!(repairs > 0, "a 28-cell corridor cannot pass unbroken");
}

/// The repair must stay a repair: [`break_sightlines`] satisfies §10.1a on
/// nearly every *raw* carve, with the §10.6 rejection reserved for genuinely
/// cornered geometry. Without this pin, the pass could silently rot into
/// "reject and redraw until lucky" and nothing above would notice — measured
/// at 1-in-1000 on the v1 config when the pass stamped tables anywhere; the
/// region-dispatched repair (no tables in corridors, benches of 2+ in
/// furniture poses) is a strictly harder constraint set, re-measured at 2%
/// (the residue is room lanes boxed in by earlier furniture, where any table
/// would sever pathing), budgeted at 4% here.
#[test]
fn the_cover_pass_repairs_almost_every_carve() {
    // Budget is the 4% rate scaled to the sweep width, floored at 1 so a single
    // unlucky sampled seed never flakes; the full CI sweep restores the 8/200 pin.
    let seeds = seed_sweep(200);
    let budget = (8 * seeds.len() / 200).max(1);
    let unrepaired = seeds
        .iter()
        .filter(|&&seed| {
            let layout = generate_once(40, 40, &mut Rng::new(seed), &Tuning::BIASED).unwrap();
            !sightlines_bounded(layout.facility())
        })
        .count();
    assert!(
        unrepaired <= budget,
        "{unrepaired}/{} carves left unrepaired (budget {budget}) — the cover pass has degraded",
        seeds.len()
    );
}

/// §10.1a **[START]** pins: *L* stays in the settled 10–12 band, "roughly a
/// guard's sight range". A tune that moves the knob out of the band — or lets
/// a guard's sight outgrow the cover that answers it — must move this pin
/// deliberately.
#[test]
fn the_sightline_cap_sits_in_the_settled_band() {
    assert!(
        (10..=12).contains(&SIGHTLINE_MAX_RUN),
        "SIGHTLINE_MAX_RUN {SIGHTLINE_MAX_RUN} left the §10.1a 10–12 band"
    );
    let range = crate::vision::GUARD_SIGHT_RANGE;
    assert!(
        SIGHTLINE_MAX_RUN.abs_diff(range) <= 2,
        "L {SIGHTLINE_MAX_RUN} drifted from GUARD_SIGHT_RANGE {range}"
    );
}

/// An empty straight gallery — enclosed, connected, and one naked sightline —
/// fails the gate on §10.1a alone; a hall shorter than the cap passes. The
/// sightline rule is a first-class §10.6 guarantee, not a style preference.
#[test]
fn a_long_gallery_fails_the_gate() {
    // 30×8 box: a 28-cell unbroken run down every interior row.
    let long = open_room(30, 8);
    assert!(fully_enclosed(long.facility()) && pathable_connected(long.facility()));
    assert!(!sightlines_bounded(long.facility()));
    assert!(!passes_guarantees(&long));

    // 13×8 box: interior runs of 11 = SIGHTLINE_MAX_RUN, exactly at the cap.
    let short = open_room(13, 8);
    assert!(sightlines_bounded(short.facility()));
    assert!(passes_guarantees(&short));
}

/// The §10.6 flood fill rejects the exact failure the old generator shipped:
/// a room sealed shut, its contents unreachable, nothing noticing. Built by
/// hand, since the real carve (correctly) never produces one.
#[test]
fn a_sealed_pocket_fails_the_gate() {
    let mut f = Facility::walled_box(12, 12);
    for y in 1..=10 {
        f.set_terrain(6, y, Terrain::Wall);
    }
    let layout = Layout::from_facility(f);
    assert!(fully_enclosed(layout.facility()), "border is intact");
    assert!(
        !pathable_connected(layout.facility()),
        "the east half is sealed off"
    );
    assert!(!passes_guarantees(&layout));
}

/// A closed door panel is transparent to pathing (§10.3/§10.4), so a room
/// whose only way out is a closed door is reachable — not a sealed pocket.
#[test]
fn a_closed_door_counts_as_reachable() {
    let mut f = Facility::walled_box(12, 12);
    for y in 1..=10 {
        f.set_terrain(6, y, Terrain::Wall);
    }
    f.set_terrain(6, 5, Terrain::DoorPanelClosed);
    assert!(passes_guarantees(&Layout::from_facility(f)));
}

/// §10.6 "fully enclosed" is asserted, not assumed: a breached border ring
/// fails the gate even though the interior stays connected.
#[test]
fn a_breached_border_fails_the_gate() {
    let mut f = Facility::walled_box(12, 12);
    f.set_terrain(0, 5, Terrain::Floor);
    assert!(!passes_guarantees(&Layout::from_facility(f)));
}

/// The entry point's contract (#13): every layout [`generate`] accepts passes
/// every §10.6 assertion — no caller ever receives an unsolvable level.
#[test]
fn accepted_seeds_always_pass_the_gate() {
    for seed in seed_sweep(200) {
        let layout = generate(40, 40, &mut Rng::new(seed)).unwrap();
        assert!(passes_guarantees(&layout), "seed {seed}: gate breached");
    }
}

/// The retry cap is a real cap: a config that can never validate fails loudly
/// with [`GenError::RetriesExhausted`] instead of spinning forever (§10.6
/// "fail loudly or retry the seed" — this is both, in order).
#[test]
fn an_unsatisfiable_config_fails_loudly() {
    let err = generate_where(40, 40, &mut Rng::new(0), |_| false, &Tuning::BIASED).unwrap_err();
    assert_eq!(
        err,
        GenError::RetriesExhausted {
            attempts: MAX_GEN_ATTEMPTS
        }
    );
}

/// §10.1.5/§11.4 (#387): **room pillars are door-clear by construction**, so no
/// runtime check was added for them — and this is the test that keeps that true.
///
/// `propose_pillar` demands a **1-cell floor moat** on every side of the grown block
/// ([`is_clear`], which also requires the cell to lie inside the room's floor
/// rectangle). So no pillar cell can be adjacent to the room's boundary wall, and a
/// doorway is cut *in* that boundary wall — two facts that together make the throat
/// rule vacuous here. The pass also runs at step 5, **before** `place_doorways`, so
/// there is not even a door on the grid to be adjacent to yet.
///
/// The property is asserted on the proposal rather than on the finished grid, because
/// that is where the moat lives: a later relaxation of the moat is exactly what would
/// quietly reintroduce the smell, and it would fail here rather than in a playtest.
#[test]
fn a_room_pillar_keeps_its_moat_and_so_can_never_touch_a_doorway() {
    // A room's floor rectangle inside a walled box: the boundary wall — where every
    // doorway is cut — is the ring at x=9/x=20 and y=9/y=20, just outside the rect.
    let room = Rect::new(10, 10, 20, 20);
    let facility = Facility::walled_box(31, 31);
    let mut proposals = 0;
    for seed in 0..200 {
        let mut rng = Rng::new(seed);
        let Some(block) = propose_pillar(&room, &facility, &mut rng) else {
            continue;
        };
        proposals += 1;
        for cell in block {
            for n in facility.neighbours(cell) {
                assert!(
                    room.contains(n),
                    "seed {seed}: pillar cell {cell:?} touches {n:?}, outside the room's \
                     floor rectangle — the 1-cell moat is gone, and with it the guarantee \
                     that a room pillar can never reach the boundary wall a doorway is \
                     cut into",
                );
            }
        }
    }
    assert!(
        proposals > 0,
        "no pillar was ever proposed — the sweep asserted nothing",
    );
}

/// §10.1.6/§11.4 (#387): duct entries **do not crowd their own kind**, asserted as a
/// property over a seed sweep rather than on one hand-built fixture.
///
/// Two checks, and they are not the same check. Two entries **shoulder to shoulder**
/// draw as one two-cell `=` and read as a single wide opening rather than two
/// crawlspaces. Two entries in **different walls** can share one floor mouth without
/// touching each other at all, and that cell then offers two `→ duct: enter` bumps.
///
/// Strict, not preferred: a duct is optional and capped, and entry combinations are
/// tried shortest-first, so refusing a crowded one costs the next combination rather
/// than the carve.
#[test]
fn no_two_duct_entries_crowd_each_other() {
    let mut entries_seen = 0;
    for seed in seed_sweep(120) {
        let (layout, _) =
            generate_level(&LevelConfig::V1, &mut Rng::new(seed)).expect("V1 generates");
        let facility = layout.facility();
        for y in 0..facility.height() {
            for x in 0..facility.width() {
                let cell = Cell::new(x, y);
                match facility.terrain(cell).expect("in bounds") {
                    Terrain::DuctEntry => {
                        entries_seen += 1;
                        for n in facility.neighbours(cell) {
                            assert_ne!(
                                facility.terrain(n),
                                Some(Terrain::DuctEntry),
                                "seed {seed}: entries at {cell:?} and {n:?} are shoulder to \
                                 shoulder — they draw as one wide opening, not two recesses",
                            );
                        }
                    }
                    Terrain::Floor => {
                        let mouths = facility
                            .neighbours(cell)
                            .filter(|&n| facility.terrain(n) == Some(Terrain::DuctEntry))
                            .count();
                        assert!(
                            mouths <= 1,
                            "seed {seed}: floor {cell:?} is the mouth of {mouths} duct \
                             entries — its usable line offers the same bump twice",
                        );
                    }
                    _ => {}
                }
            }
        }
    }
    assert!(
        entries_seen > 0,
        "the sweep placed no duct at all — it asserted nothing",
    );
}

/// §10.1.5 (#387): the throat rule **refines "cover near doors", it does not repeal
/// it.** Moving furniture out of the door frame must not empty the doorway's
/// neighbourhood — "a door you burst through should have something to duck behind on
/// the other side, or bursting through it accomplishes nothing".
///
/// So: over a sweep, a healthy majority of room doorways still have crouchable cover
/// **within a couple of cells** — just not in the frame itself. Loose and
/// direction-only (§13.4): what is pinned is that the fix did not clear the
/// neighbourhood, not a particular ratio.
#[test]
fn cover_survives_beside_doors_it_just_leaves_the_frame() {
    let mut served = 0;
    let mut doors = 0;
    for seed in seed_sweep(64) {
        let (layout, _) =
            generate_level(&LevelConfig::V1, &mut Rng::new(seed)).expect("V1 generates");
        let facility = layout.facility();
        for y in 0..facility.height() {
            for x in 0..facility.width() {
                let door = Cell::new(x, y);
                if !is_door_terrain(facility.terrain(door).expect("in bounds")) {
                    continue;
                }
                doors += 1;
                // Cover a cell or two into the room still serves the burst-through.
                let near = (0..facility.height())
                    .flat_map(|cy| (0..facility.width()).map(move |cx| Cell::new(cx, cy)))
                    .any(|c| {
                        facility.terrain(c) == Some(Terrain::PartialCover)
                            && door.manhattan_distance(c) <= 3
                    });
                if near {
                    served += 1;
                }
            }
        }
    }
    assert!(doors > 0, "the sweep found no doors");
    assert!(
        served * 4 >= doors,
        "only {served} of {doors} doorways have cover within 3 cells — the throat rule \
         has emptied the doorway's neighbourhood instead of just its frame (§10.1.5)",
    );
}

/// [`touches_door`] is the throat rule's one vocabulary (#387): a hinge and **both**
/// panel poses all clog a doorway, and the neighbourhood is **orthogonal only** — a
/// diagonal table does not clog a throat.
#[test]
fn the_throat_rule_reads_every_door_cell_and_only_orthogonally() {
    for door in [
        Terrain::DoorHinge,
        Terrain::DoorPanelClosed,
        Terrain::DoorPanelOpen,
    ] {
        let mut f = Facility::walled_box(8, 8);
        f.set_terrain(4, 4, door);
        assert!(
            touches_door(&f, Cell::new(4, 3)),
            "{door:?}: an orthogonal neighbour is in the throat",
        );
        assert!(
            !touches_door(&f, Cell::new(3, 3)),
            "{door:?}: a diagonal neighbour is not — two recesses, not one wide mouth",
        );
    }
}

/// §10.1.5/§10.6/§11.4 (#387): **a table in a door frame is now the rare exception,
/// not the rule.** The doorway is the one cell everything funnels through, and a
/// table jammed against its frame narrows the burst-through to a squeeze and gives
/// the mouth cell a doubled usable (`→ door: open` *and* `↑ table: crouch`).
///
/// A *preference*, not a guarantee, and deliberately so — the same shape
/// [`cover_rarely_doubles_the_crouch_hint`] above pins for the §11.4 rule, and for
/// the same §10.6 reason: the bench pass exists to repair a §10.1a sightline, and
/// §10.1a outranks the placement preference. So the throat rule lives in the
/// *preferring* pass and the mandatory last-resort pass overrides it rather than
/// failing the carve.
///
/// **Measured before shipping, which is what settled the shape.** Over 300 seeds the
/// rule took door-adjacent tables from **1582 (on 296 of 300 seeds) to 9 (on 6)** —
/// so the fallback is genuinely load-bearing on ~2% of seeds and could *not* be
/// dropped for a strict rule, which is the outcome this ticket asked to check for.
/// The carve rejection rate moved 309 → 313 carves per 300 levels (3.0% → 4.3%),
/// nowhere near §10.6's ~85%-rejection cautionary history.
#[test]
fn a_table_in_a_door_frame_is_the_rare_last_resort() {
    let seeds = seed_sweep(300);
    let framed: u32 = seeds
        .iter()
        .map(|&seed| {
            let layout = generate(40, 40, &mut Rng::new(seed)).unwrap();
            let f = layout.facility();
            (0..f.height())
                .flat_map(|y| (0..f.width()).map(move |x| Cell::new(x, y)))
                .filter(|&c| f.terrain(c) == Some(Terrain::PartialCover) && touches_door(f, c))
                .count() as u32
        })
        .sum();
    // Ceiling of 0.2 framed tables per level — ~6× the measured 0.03/level, floored
    // so a thin fast-mode sample never flakes, and far under the ~5/level the pass
    // produced before the rule.
    let budget = (seeds.len() as u32 / 5).max(3);
    assert!(
        framed <= budget,
        "{framed} door-adjacent tables over {} seeds (budget {budget}) — the throat \
         rule has stopped preferring, and the frame is filling up again",
        seeds.len(),
    );
}
