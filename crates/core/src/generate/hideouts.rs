//! Hideout phase (§10.1.6): recess cupboards into walls as player refuges,
//! each a single-mouth pocket that never severs the level's connectivity.
//!
//! Part of the [`generate`](super) pipeline; `use super::*` pulls the shared
//! types, tuning constants, and sibling-phase helpers into scope.

use super::*;

/// Carve the hiding-game board: concealment **cupboards recessed into the walls**,
/// spread along the flight paths (§10.1.6, §10.1a).
///
/// A cupboard is a wall-line cell the player *ducks into*: flush with the surrounding
/// wall, backed by solid structure. Empty it is walk-through yet blocks pathing, so a
/// guard patrol routes *around* it while the player slips *in*; occupied it is solid
/// and conceals its occupant (the vision ticket reads that concealment, §11.5a). The
/// old generator harvested rare three-walled floor pockets, one attempt per room,
/// stopping at the first failure — so during a chase there was nowhere to hide and
/// the hiding game had no board (§10.1a). This places them deliberately, into the
/// backing the thicken pass and the pillars manufacture: **corridors and junctions
/// first** (where the player flees), rooms after, spaced out, never stopping at the
/// first failure.
///
/// A recess is a **wall → hideout** rewrite, which is why it needs no `severs_pathing`
/// guard: a wall blocks pathing and so does a hideout, so the walkable graph is
/// untouched (only the current floor→hideout of the old design could pinch a route).
/// The site test ([`recess_site`]) demands three solid sides, so the cupboard can
/// never be walked or seen *through* to the far side; the spacing then keeps a
/// cupboard's own solid backing intact — the corridor-facing and room-facing cells of
/// one thickened stretch sit one apart, so taking one bars the other.
pub(super) fn place_hideouts(
    facility: &mut Facility,
    regions: &mut RegionGraph,
    rng: &mut Rng,
    tuning: &Tuning,
) {
    // Candidate recess sites — wall cells with one floor mouth — split so the flight
    // paths are served first: a cupboard opening onto a corridor outranks one onto a
    // room. Each carries the mouth it opens through, which names its bucket and,
    // later, the region it joins.
    let (w, h) = (facility.width(), facility.height());
    let mut corridor: Vec<(Cell, Cell)> = Vec::new();
    let mut room: Vec<(Cell, Cell)> = Vec::new();
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let cell = Cell::new(x, y);
            let Some(mouth) = recess_site(facility, cell) else {
                continue;
            };
            match regions.region_at(mouth).map(|id| regions.kind(id)) {
                Some(RegionKind::Corridor) => corridor.push((cell, mouth)),
                Some(RegionKind::Room) => room.push((cell, mouth)),
                None => {}
            }
        }
    }
    // Shuffle within each bucket so the board varies by seed, then take corridors
    // before rooms. Both are deterministic from `rng` (§12.4).
    shuffle(&mut corridor, rng);
    shuffle(&mut room, rng);

    // Each candidate's required spacing follows the region it opens onto — corridors
    // pack denser than rooms (#91), the flight path where cover is needed most.
    let corridor = corridor
        .into_iter()
        .map(|cm| (cm, tuning.hideout_spacing_corridor));
    let room = room.into_iter().map(|cm| (cm, tuning.hideout_spacing_room));

    let mut placed: Vec<Cell> = Vec::new();
    for ((cell, mouth), spacing) in corridor.chain(room) {
        // Spacing keeps cupboards spread along a path — and, since the two faces of a
        // thickened wall are one cell apart, keeps every cupboard's backing solid.
        if placed.iter().any(|&p| p.manhattan_distance(cell) < spacing) {
            continue;
        }
        // Prefer not to crowd the mouth's usable line (§11.4): a cupboard whose mouth
        // already borders a door, a table or another cupboard is skipped. Cupboards
        // are best-effort furniture with plentiful sites, so skipping only improves
        // the spread.
        if creates_usable_conflict(facility, cell) {
            continue;
        }
        // A recessed cupboard is walkable, so it joins the region it opens onto — the
        // space the player is *in* when ducked inside it (§10.5).
        let region = regions
            .region_at(mouth)
            .expect("a recess mouth is claimed floor");
        facility.set_terrain(cell.x, cell.y, Terrain::Hideout);
        regions.add_cell(region, cell);
        placed.push(cell);
    }
}

/// Whether the wall cell `cell` can host a **recessed** cupboard, and if so the floor
/// **mouth** the player bumps it from (§10.1.6).
///
/// A clean recess is a wall cell with **exactly one floor neighbour and three wall
/// neighbours** — flush with the wall line, backed and flanked by solid opaque wall.
/// The three-wall requirement is the safety guarantee: a cupboard cut here can be
/// neither walked nor seen *through* to whatever is on the far side (§10.1.6). It is
/// the very geometry a two-thick wall (its inner and outer courses) and a pillar face
/// both offer — which is why [`thicken_walls`] runs first. Any door cell on a flank
/// fails the exactly-three-wall count, so doorways stay clear without a special case.
pub(super) fn recess_site(facility: &Facility, cell: Cell) -> Option<Cell> {
    if facility.terrain(cell) != Some(Terrain::Wall) {
        return None;
    }
    let mut mouth = None;
    let mut walls = 0;
    for n in facility.neighbours(cell) {
        match facility.terrain(n) {
            Some(Terrain::Floor) => {
                if mouth.is_some() {
                    return None; // a second opening — not backed on three sides
                }
                mouth = Some(n);
            }
            Some(Terrain::Wall) => walls += 1,
            // A hinge, panel, table, hideout or open floor gap on a flank means the
            // recess is not cleanly walled in — skip it.
            _ => return None,
        }
    }
    // An interior cell has four neighbours, so this is exactly one floor + three wall.
    if walls == 3 {
        mouth
    } else {
        None
    }
}

/// Whether turning `cell` into a hideout would sever guard pathing (§10.3) — the
/// cupboard must never wall a patrol route off, nor strand an objective (#13).
///
/// Cutting a single cell can only disconnect the graph *at that cell*: the pieces it
/// would split each hold one of `cell`'s pathable neighbours. So the question is
/// purely local — can those neighbours still reach each other **without** stepping on
/// `cell`? If a detour exists through the 3×3 ring around `cell`, removing it cannot
/// disconnect anything (the ring cells are real and stay pathable). This is *sound*:
/// a local detour is a global one. It is also conservative — a candidate whose
/// neighbours only reconnect via a long way round is skipped rather than risked — which
/// costs at most a few cupboard sites and keeps the check O(1) instead of a per-candidate
/// flood fill over the whole level.
pub(super) fn severs_pathing(facility: &Facility, cell: Cell) -> bool {
    let pathable = |c: Cell| facility.terrain(c).is_some_and(|t| !t.blocks_pathing());
    // The pathable orthogonal neighbours — the cells that must stay mutually reachable.
    let targets: Vec<Cell> = facility.neighbours(cell).filter(|&n| pathable(n)).collect();
    if targets.len() <= 1 {
        return false; // nothing to keep connected
    }
    // Flood the pathable ring (Chebyshev ≤ 1 of `cell`, excluding `cell` itself) from
    // one target; if it reaches every other target, a detour exists and `cell` is safe.
    // This is deliberately the O(ring) local flood, *not* `path::flood_from` over the
    // whole level — folding it into the full-grid primitive would defeat the O(1) point.
    let in_ring = |c: Cell| c != cell && chebyshev(cell, c) <= 1;
    let mut seen = vec![targets[0]];
    let mut stack = vec![targets[0]];
    while let Some(c) = stack.pop() {
        for n in facility.neighbours(c) {
            if in_ring(n) && pathable(n) && !seen.contains(&n) {
                seen.push(n);
                stack.push(n);
            }
        }
    }
    !targets.iter().all(|t| seen.contains(t))
}

/// Break an over-long **corridor** run by recessing one more cupboard mid-run —
/// the §10.1a repair for the region that never takes a table (§10.1.6). The new
/// mouth is a run cell, so the run splits there ([`counterplay_at`]): a fleeing
/// player at the mouth vanishes instead of being run down, which is exactly what
/// the sightline rule asks of a flight path (§7.6). Site choice mirrors
/// [`place_bench`]: centre-out from a jittered aim, preferring a ready-made
/// recess site (spaced and uncrowded first, §11.4), then an **alcove**
/// ([`alcove_site`]) — a cupboard whose solid backing is first carved out of the
/// space behind a one-thick flank wall — then, for a stretch too open for any
/// recess at all, a **2×2 structural pillar** ([`place_pillar`]), and finally a
/// 1-cell **buttress** against a flank wall (the §10.1a S-squeeze, for the
/// 2-wide corridor a pillar would choke). A run that admits none of them
/// anywhere along it reports failure and the §10.6 gate rejects the carve.
pub(super) fn recess_run_hideout(
    facility: &mut Facility,
    regions: &mut RegionGraph,
    run: &SightRun,
    rng: &mut Rng,
) -> bool {
    // Aim at the middle, jittered by up to a sixth of the run either way.
    let jitter = (run.len / 6).max(1) as i32;
    let aim = (run.start + run.len / 2) as i32 + rng.range_inclusive(-jitter, jitter);
    let aim = aim.clamp(run.start as i32, (run.start + run.len - 1) as i32) as u32;
    let mut order: Vec<u32> = (run.start..run.start + run.len).collect();
    order.sort_by_key(|&i| (i.abs_diff(aim), i));

    // Candidate `(wall, mouth)` pairs serving run position `i`, nearest first: a
    // wall flanking the run cell itself (the mouth *is* the run cell), then a
    // wall one lane out whose mouth is the run cell's lateral floor neighbour —
    // that mouth still puts the run cell within [`counterplay_at`]'s two-move
    // reach, which is what lets an *inner* lane of a 3–4 wide corridor (no
    // adjacent wall at all) still be served by a recess.
    let lateral = match run.line {
        Line::Row(_) => [Direction::North, Direction::South],
        Line::Col(_) => [Direction::West, Direction::East],
    };
    let candidates = |facility: &Facility, i: u32| {
        let cell = run.line.cell(i);
        let mut out: Vec<(Cell, Cell)> = Vec::new();
        for dir in lateral {
            let Some(near) = cell.step(dir) else { continue };
            out.push((near, cell));
            if facility.terrain(near) == Some(Terrain::Floor) {
                if let Some(far) = near.step(dir) {
                    out.push((far, near));
                }
            }
        }
        out
    };

    // The pass's spacing and crowding rules ([`place_hideouts`], §11.4) are
    // preferences here, not gates — breaking the run outranks both. Candidates
    // are tiered: a ready recess that keeps the corridor spacing *and* crowds no
    // usable line wins outright; then a clean-but-close one, then any ready one;
    // alcoves follow in the same order, because carving one eats a room cell.
    let hideouts: Vec<Cell> = (0..facility.height())
        .flat_map(|y| (0..facility.width()).map(move |x| Cell::new(x, y)))
        .filter(|&c| facility.terrain(c) == Some(Terrain::Hideout))
        .collect();
    let spaced = |cell: Cell| {
        hideouts
            .iter()
            .all(|&h| h.manhattan_distance(cell) >= HIDEOUT_MIN_SPACING_CORRIDOR)
    };
    let mut ready_close = None;
    let mut ready_crowded = None;
    let mut ready = None;
    'ready: for &i in &order {
        for (wall, mouth) in candidates(facility, i) {
            if recess_site(facility, wall) != Some(mouth) {
                continue;
            }
            ready_crowded.get_or_insert(wall);
            if creates_usable_conflict(facility, wall) {
                continue;
            }
            ready_close.get_or_insert(wall);
            if spaced(wall) {
                ready = Some(wall);
                break 'ready;
            }
        }
    }
    if let Some(wall) = ready.or(ready_close).or(ready_crowded) {
        let mouth = recess_site(facility, wall).expect("validated in the scan above");
        let region = regions
            .region_at(mouth)
            .expect("a recess mouth is claimed floor");
        facility.set_terrain(wall.x, wall.y, Terrain::Hideout);
        regions.add_cell(region, wall);
        return true;
    }

    // No ready backing anywhere along the run: carve an alcove instead — wall up
    // the room cell behind a one-thick flank wall, then recess into it.
    let mut alcove_close = None;
    let mut alcove_crowded = None;
    let mut alcove = None;
    'alcove: for &i in &order {
        for (wall, mouth) in candidates(facility, i) {
            if alcove_site(facility, regions, wall, mouth).is_none() {
                continue;
            }
            alcove_crowded.get_or_insert((wall, mouth));
            if creates_usable_conflict(facility, wall) {
                continue;
            }
            alcove_close.get_or_insert((wall, mouth));
            if spaced(wall) {
                alcove = Some((wall, mouth));
                break 'alcove;
            }
        }
    }
    if let Some((wall, mouth)) = alcove.or(alcove_close).or(alcove_crowded) {
        let back =
            alcove_site(facility, regions, wall, mouth).expect("validated in the scan above");
        regions.remove_cell(back);
        facility.set_terrain(back.x, back.y, Terrain::Wall);
        debug_assert_eq!(recess_site(facility, wall), Some(mouth));
        let region = regions
            .region_at(mouth)
            .expect("a recess mouth is claimed floor");
        facility.set_terrain(wall.x, wall.y, Terrain::Hideout);
        regions.add_cell(region, wall);
        return true;
    }

    // Still nothing: this is a wide-open stretch — a junction plaza, a corridor
    // whose flanks are all doors and cupboards already. Break it with a **2×2
    // structural pillar** instead (§10.1a "give corridors features too"): a
    // column in the hall, solid wall that blocks sight outright and forces the
    // squeeze. Architecture, not furniture — the corridors-carry-no-tables rule
    // (§10.1.6) stays intact.
    for &i in &order {
        for dir in lateral {
            if place_pillar(facility, regions, run, i, dir) {
                return true;
            }
        }
    }

    // Last resort: a 1-cell **buttress** — wall up a run cell flush against a
    // flank wall, the §10.1a S-squeeze as a pilaster. This serves the 2-wide
    // corridor whose walls are all doors: a pillar would fill its whole width
    // (severing pathing), but a single jutting cell narrows it to the 1-cell
    // squeeze the design wants. Never floating (it must touch solid wall, so it
    // reads as structure), never on a cupboard mouth, and the sever/split guards
    // keep the squeeze passable.
    for &i in &order {
        let cell = run.line.cell(i);
        let touches = |t: Terrain| {
            facility
                .neighbours(cell)
                .any(|n| facility.terrain(n) == Some(t))
        };
        if facility.terrain(cell) == Some(Terrain::Floor)
            && regions
                .region_at(cell)
                .is_some_and(|id| regions.kind(id) == RegionKind::Corridor)
            && touches(Terrain::Wall)
            && !touches(Terrain::Hideout)
            && !severs_pathing(facility, cell)
            && !splits_region(regions, cell)
        {
            regions.remove_cell(cell);
            facility.set_terrain(cell.x, cell.y, Terrain::Wall);
            return true;
        }
    }
    false
}
