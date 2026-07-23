//! Doorways phase (§10.4): choose wall runs to breach, cut the door cells,
//! and open the initial set — the region graph learns each opening.
//!
//! Part of the [`generate`](super) pipeline; `use super::*` pulls the shared
//! types, tuning constants, and sibling-phase helpers into scope.

use super::*;

/// Cut doorways where rooms meet corridors (§10.1.4), capping how many each room
/// gets so a room boxed in by corridors is not riddled with doors.
///
/// Two passes. **Collect:** scan every row, then every column, for **maximal runs**
/// of wall with interior on both flanks — the door candidates. "Maximal" is read
/// against the two regions a run separates: it breaks not only where a flank stops
/// being interior but also where the pair of flanking regions changes, so every
/// candidate joins one room to one corridor and never spans two rooms. **Choose:**
/// a room with a door on every wall is a thoroughfare, not a room, so each room
/// keeps only [`room_door_budget`]-many of its candidates (one or two usually,
/// three rarely) — but always at least one, so no room is sealed (§10.6). Because
/// rooms are always separated from each other by corridor-plus-two-walls, **every
/// door connects a room to a corridor** (§10.1.4, §10.6).
pub(super) fn place_doorways(facility: &mut Facility, regions: &mut RegionGraph, rng: &mut Rng) {
    let (w, h) = (facility.width(), facility.height());
    let mut candidates: Vec<Candidate> = Vec::new();
    // Rows first: a horizontal wall with floor north and south is a run in x, and
    // its doorway joins the region above to the region below.
    for y in 1..h - 1 {
        collect_runs(regions, Line::Row(y), w, &mut candidates);
    }
    // Then columns: a vertical wall with floor west and east is a run in y.
    for x in 1..w - 1 {
        collect_runs(regions, Line::Col(x), h, &mut candidates);
    }

    for idx in choose_doors(&candidates, rng) {
        cut_door(facility, regions, rng, &candidates[idx]);
    }
}

/// Walk one scan line, breaking it into maximal runs of wall separating a constant
/// pair of distinct regions, and push each run of length ≥ 3 as a [`Candidate`].
pub(super) fn collect_runs(
    regions: &RegionGraph,
    line: Line,
    extent: u32,
    out: &mut Vec<Candidate>,
) {
    // The run in flight: start position along the line, length, and the two regions
    // it separates.
    let mut run: Option<(u32, u32, RegionId, RegionId)> = None;
    let flush = |run: Option<(u32, u32, RegionId, RegionId)>, out: &mut Vec<Candidate>| {
        if let Some((start, len, a, b)) = run {
            if len >= MIN_DOOR_RUN {
                // Exactly one endpoint is a room (rooms never touch across a single
                // wall), so name the room and corridor for the per-room budget.
                let (room, corridor) = if regions.kind(a) == RegionKind::Room {
                    (a, b)
                } else {
                    (b, a)
                };
                out.push(Candidate {
                    line,
                    start,
                    len,
                    room,
                    corridor,
                });
            }
        }
    };
    for i in 1..extent - 1 {
        let candidate = door_candidate(regions, line, i);
        match (run, candidate) {
            (Some((start, len, a, b)), Some((ca, cb))) if (ca, cb) == (a, b) => {
                run = Some((start, len + 1, a, b));
            }
            _ => {
                flush(run.take(), out);
                run = candidate.map(|(a, b)| (i, 1, a, b));
            }
        }
    }
    flush(run.take(), out);
}

/// Whether the cell at `i` on `line` is a doorway candidate: a wall cell with a
/// distinct interior region on each flank. Returns that region pair, or `None`.
pub(super) fn door_candidate(
    regions: &RegionGraph,
    line: Line,
    i: u32,
) -> Option<(RegionId, RegionId)> {
    let (near, far) = line.flanks(i);
    // A wall separating two regions owns neither flank itself, so it is unclaimed.
    if regions.region_at(line.cell(i)).is_some() {
        return None;
    }
    let a = regions.region_at(near)?;
    let b = regions.region_at(far)?;
    if a == b {
        return None;
    }
    Some((a, b))
}

/// Pick which candidates become doors, capping each room to [`room_door_budget`]
/// of its own — but never below one, so no room is sealed (§10.6). Returns the
/// chosen indices into `candidates`, in ascending (scan) order so the cut pass is
/// deterministic. Rooms are budgeted in first-appearance order, itself fixed by the
/// scan, for the same reason.
pub(super) fn choose_doors(candidates: &[Candidate], rng: &mut Rng) -> Vec<usize> {
    // Group candidate indices by the room they belong to, keeping rooms in the
    // deterministic order they first appear in the scan.
    let mut by_room: Vec<(RegionId, Vec<usize>)> = Vec::new();
    for (i, c) in candidates.iter().enumerate() {
        match by_room.iter_mut().find(|(r, _)| *r == c.room) {
            Some((_, idxs)) => idxs.push(i),
            None => by_room.push((c.room, vec![i])),
        }
    }

    let mut chosen: Vec<usize> = Vec::new();
    for (_, mut idxs) in by_room {
        let budget = room_door_budget(rng) as usize;
        let keep = budget.clamp(1, idxs.len());
        // Partial Fisher–Yates: shuffle `keep` random candidates to the front.
        for i in 0..keep {
            let j = i + rng.below((idxs.len() - i) as u32) as usize;
            idxs.swap(i, j);
        }
        idxs.truncate(keep);
        chosen.extend(idxs);
    }
    chosen.sort_unstable();
    chosen
}

/// A room's doorway budget: **one or two usually, three rarely, never more**
/// (`MAX_DOORS_PER_ROOM`) **[START]**. Clamped afterwards to the candidates a room
/// actually has, and never below one.
pub(super) fn room_door_budget(rng: &mut Rng) -> u32 {
    // 40% one, 50% two, 10% three — most rooms have one or two ways in.
    match rng.below(10) {
        0..=3 => 1,
        4..=8 => 2,
        _ => MAX_DOORS_PER_ROOM,
    }
}

/// Cut the doorway for one chosen `candidate`. Length is random `3..=min(len, 6)`
/// at a random offset (§10.1.4). A **[START]** [`AUTO_DOOR_PERCENT`] share (drawn
/// from the seeded RNG, §12.4) become **automatic** — the whole span is closed
/// panels, no hinges (§10.4/#147) — and the rest are **manual**: the two ends are
/// hinges, the cells between are closed panels (§10.4).
pub(super) fn cut_door(
    facility: &mut Facility,
    regions: &mut RegionGraph,
    rng: &mut Rng,
    candidate: &Candidate,
) {
    let Candidate {
        line,
        start,
        len,
        room,
        corridor,
    } = *candidate;
    let door_len = rng.range_inclusive(MIN_DOOR_RUN as i32, len.min(MAX_DOOR_LEN) as i32) as u32;
    let offset = rng.range_inclusive(0, (len - door_len) as i32) as u32;
    let first = start + offset;
    let last = first + door_len - 1;

    // The automatic/manual draw is made here so the same seed makes the same doors
    // automatic (§12.4). An automatic door is frameless: every cell of the span is a
    // panel. A manual door frames the span with a hinge at each end.
    let automatic = rng.below(100) < AUTO_DOOR_PERCENT;
    let (hinges, panels): (Vec<Cell>, Vec<Cell>) = if automatic {
        (Vec::new(), (first..=last).map(|i| line.cell(i)).collect())
    } else {
        (
            vec![line.cell(first), line.cell(last)],
            (first + 1..last).map(|i| line.cell(i)).collect(),
        )
    };

    for &hinge in &hinges {
        facility.set_terrain(hinge.x, hinge.y, Terrain::DoorHinge);
    }
    for &panel in &panels {
        facility.set_terrain(panel.x, panel.y, Terrain::DoorPanelClosed);
    }
    let kind = if automatic {
        DoorKind::Automatic {
            delay: AUTO_CLOSE_DELAY,
        }
    } else {
        DoorKind::Manual
    };
    regions.add_door(room, corridor, hinges, panels, kind);
}

/// Open a deterministic [`OPEN_DOOR_PERCENT`] share of the level's doorways as
/// initial state (#145, §10.4/§11.3): each door draws once from the seeded `rng`,
/// and an opened one moves its graph flag and panel terrain to open together via
/// [`Layout::open_door_initial`]. Same seed → same open doors (§12.4).
///
/// Ordering matters and is the caller's contract (see [`generate_level`]): this
/// runs on a carve that has already passed the §10.6 gate — reachability is
/// identical whether a door is open or closed (both panels are pathable, §10.3),
/// and the §10.1a sightline rule describes the carve's geometry, not its live door
/// poses — and *before* placement, whose §10.1.9 turn-one cone check must account
/// for the extra sight an open doorway grants a guard.
pub(super) fn open_initial_doors(layout: &mut Layout, rng: &mut Rng) {
    let ids: Vec<DoorId> = layout.regions().doors().map(|(id, _)| id).collect();
    for id in ids {
        if rng.below(100) < OPEN_DOOR_PERCENT {
            layout.open_door_initial(id);
        }
    }
}

/// Whether `terrain` is part of a doorway — a hinge or either panel pose (§10.4).
pub(super) fn is_door_terrain(terrain: Terrain) -> bool {
    matches!(
        terrain,
        Terrain::DoorHinge | Terrain::DoorPanelClosed | Terrain::DoorPanelOpen
    )
}
