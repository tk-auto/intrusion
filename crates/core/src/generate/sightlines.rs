//! Sightline-breaking phase (§10.1a): find over-long straight runs and place
//! counterplay so no corridor or room offers an unbroken line past the cap.
//!
//! Part of the [`generate`](super) pipeline; `use super::*` pulls the shared
//! types, tuning constants, and sibling-phase helpers into scope.

use super::*;

/// Break every over-long straight sightline with counterplay (§10.1a).
///
/// Corridor-first partition has a severe emergent flaw that only shows up in play
/// (§7.6): it produces long, dead-straight, full-span corridors — and the
/// corridors are where the player flees. The rooms get features; the corridors got
/// nothing. So this pass scans the whole grid for straight runs longer than
/// [`SIGHTLINE_MAX_RUN`] with no counterplay in them and repairs each near its
/// middle — with the repair the run's *region* calls for (§10.1.6):
///
/// - A **room-dominated** run breaks with furniture: a **bench** of partial-cover
///   **tables** ([`Terrain::PartialCover`], see [`place_bench`]) — 2+ cells, posed
///   like a placed piece. A table is furniture, not a wall stub: it blocks movement
///   and pathing but a guard sees straight over it — the counterplay it plants is
///   the *crouch* (§10.3), not a shadow.
/// - A **corridor-dominated** run gets **no table — corridors never do**. Its
///   counterplay is the hiding game's own board: one more cupboard recessed
///   mid-run ([`recess_run_hideout`]), whose mouth is the cell a fleeing player
///   vanishes from (§7.6). Furniture stays out of the flight path; the corridor
///   keeps reading as a corridor.
///
/// A bench candidate that would sever guard pathing (§10.3) or split its own
/// region into pieces (§10.5) is skipped, so a pathing gap always survives. A
/// bench *may* land beside a multi-panel door span — that is §10.1a's "cover
/// near doors", something to duck behind on the far side — while a single-panel
/// door can never be sealed, because walling its only approach leaves the panel
/// no local detour and [`severs_pathing`] refuses.
///
/// The pass is a *repair*, not a proof: [`passes_guarantees`] re-measures the
/// finished grid, so a run this pass could not break (every candidate disqualified)
/// rejects the carve and redraws, exactly like a reachability failure (§10.6).
pub(super) fn break_sightlines(
    facility: &mut Facility,
    regions: &mut RegionGraph,
    rng: &mut Rng,
    tuning: &Tuning,
) {
    // Phase 1 — the hard §10.1a floor (§10.6): every straight run over
    // SIGHTLINE_MAX_RUN must break, or this carve is genuinely cornered and is
    // rejected. Each success strictly shrinks the set of counterplay-free cells
    // (a bench turns floor into cover; a recess turns its mouth into counterplay),
    // so the loop always terminates.
    while let Some(run) = sight_runs(facility)
        .into_iter()
        .find(|r| r.len > SIGHTLINE_MAX_RUN)
    {
        // A room-dominated run prefers furniture, but falls back to the cupboard
        // repair where no bench fits — the 1-wide lane behind a partition stub or
        // pillar, where every cell severs pathing and the flanks are wall: the
        // old pass dropped the lone-table confetti there; a cupboard recessed
        // into the lane's flank serves the same counterplay honestly.
        let broken = if run_is_room_dominant(regions, &run) {
            place_bench(facility, regions, &run, rng, false)
                || recess_run_hideout(facility, regions, &run, rng)
        } else {
            recess_run_hideout(facility, regions, &run, rng)
        };
        if !broken {
            return; // unbreakable — the §10.6 gate rejects this carve
        }
    }

    // Phase 2 — the room preference (#91): break room-dominated runs down to the
    // tighter room limit, so rooms carry proportionally more tables (§10.3). This
    // only ever *adds* cover on top of the satisfied floor, so it is best-effort — a
    // room run no bench can furnish is simply left (it still meets the hard floor)
    // and never rejects the carve. Retiring such a run keeps the loop terminating.
    if tuning.room_sightline_max_run >= SIGHTLINE_MAX_RUN {
        return;
    }
    let mut retired: Vec<(Line, u32, u32)> = Vec::new();
    while let Some(run) = sight_runs(facility).into_iter().find(|r| {
        r.len > tuning.room_sightline_max_run
            && r.len <= SIGHTLINE_MAX_RUN
            && run_is_room_dominant(regions, r)
            && !retired.contains(&(r.line, r.start, r.len))
    }) {
        if !place_bench(facility, regions, &run, rng, true) {
            retired.push((run.line, run.start, run.len));
        }
    }
}

/// Whether `run` lies mostly inside rooms — the §10.1.6/#91 test naming a run's
/// region, which decides both its repair (a bench for a room, a recessed cupboard
/// for a corridor — a tie counts as corridor, keeping tables out of anything
/// corridor-like) and whether the tighter room run-limit applies. A run bounded by
/// walls can still straddle a doorway into a corridor; the majority of its cells
/// names it.
pub(super) fn run_is_room_dominant(regions: &RegionGraph, run: &SightRun) -> bool {
    let (mut room, mut corridor) = (0u32, 0u32);
    for i in run.start..run.start + run.len {
        match regions
            .region_at(run.line.cell(i))
            .map(|id| regions.kind(id))
        {
            Some(RegionKind::Room) => room += 1,
            Some(RegionKind::Corridor) => corridor += 1,
            None => {}
        }
    }
    room > corridor
}

/// Whether `cell` carries the §10.1a *counterplay* that ends a sightline run:
/// sight-blocking terrain (§10.3 — wall, hinge, closed panel), **partial cover**
/// (a table does not stop a guard's sight, but it plants the crouch in the middle
/// of the straight), **or a cupboard within two moves** — the cell is a recessed
/// hideout's mouth (bump to vanish, §10.1.6), or one floor step from a mouth. A
/// guard sees straight past a flush recess, but the player here is gone before
/// the sight matters, which is the §10.1a rule's real demand — geometry between
/// the player and being seen, not darkness. Two moves and not just the mouth
/// itself because a corridor is up to four cells wide: the lane *beside* the
/// mouth's lane flees to the same cupboard, one step later.
pub(super) fn counterplay_at(facility: &Facility, cell: Cell) -> bool {
    let Some(terrain) = facility.terrain(cell) else {
        return false;
    };
    if terrain.blocks_sight() || terrain.provides_cover() {
        return true;
    }
    let mouth = |c: Cell| {
        facility
            .neighbours(c)
            .any(|n| facility.terrain(n) == Some(Terrain::Hideout))
    };
    mouth(cell)
        || facility
            .neighbours(cell)
            .any(|n| facility.terrain(n) == Some(Terrain::Floor) && mouth(n))
}

/// Every maximal counterplay-free run in the grid, rows then columns, in scan
/// order. A run is bounded wherever a cell offers counterplay ([`counterplay_at`]):
/// an obstruction, partial cover, or a cupboard mouth.
pub(super) fn sight_runs(facility: &Facility) -> Vec<SightRun> {
    let (w, h) = (facility.width(), facility.height());
    let mut runs = Vec::new();
    for y in 0..h {
        collect_sight_runs(facility, Line::Row(y), w, &mut runs);
    }
    for x in 0..w {
        collect_sight_runs(facility, Line::Col(x), h, &mut runs);
    }
    runs
}

/// Walk one scan line and push each maximal run of cells that neither block
/// sight nor provide cover.
pub(super) fn collect_sight_runs(
    facility: &Facility,
    line: Line,
    extent: u32,
    out: &mut Vec<SightRun>,
) {
    let mut start: Option<u32> = None;
    for i in 0..extent {
        let clear =
            facility.terrain(line.cell(i)).is_some() && !counterplay_at(facility, line.cell(i));
        match (start, clear) {
            (None, true) => start = Some(i),
            (Some(s), false) => {
                out.push(SightRun {
                    line,
                    start: s,
                    len: i - s,
                });
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        out.push(SightRun {
            line,
            start: s,
            len: extent - s,
        });
    }
}

/// The §10.1a sightline rule as a measured property of a finished grid: no
/// straight run without counterplay — an obstruction, a partial-cover cell, *or*
/// a cupboard mouth ([`counterplay_at`]) — exceeds [`SIGHTLINE_MAX_RUN`].
/// Covering every maximal row and column run covers every cell in each of the 4
/// cardinal directions.
pub(super) fn sightlines_bounded(facility: &Facility) -> bool {
    sight_runs(facility)
        .iter()
        .all(|r| r.len <= SIGHTLINE_MAX_RUN)
}
