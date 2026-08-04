//! Patrol-target selection (§7.5/§7.3): how a Calm guard chooses where to walk.
//!
//! The planner half of the guard, split out from the state machine and movement
//! (#435): the patrol vocabulary ([`PatrolStyle`], [`Dwell`]), the sweep's
//! territory and its next-target picks, and the dwell roll an arrival pays
//! before repicking. The cut is **choosing where to go**; walking there — the
//! destination step, the search walk, [`super::routable`] — stays with the
//! state machine. Same struct throughout: a second `impl Guard` block of
//! `pub(super)` helpers, the `state/abilities.rs` pattern.

use super::*;

/// The §7.5 dwell rule in force this turn: how often an arriving Calm guard pauses,
/// and how long it holds when it does.
///
/// It is handed to [`decide`](Guard::decide) rather than read from constants because
/// the length is **not** a constant any more: the facility alert shortens it from
/// rung 1 up (§7.3), so a guard's pause is a fact about the level's current state,
/// not about the guard. The chance stays the playtest knob it was
/// (`State::set_guard_dwell_chance`).
/// How a Calm guard chooses where to walk (§7.5/§7.3) — the shape of patrol, as one
/// plain value rather than a flag queried in several places (§12.3).
///
/// The facility's radio net is what decides it. With the net live, guards are
/// coordinated: each sweeps its own slice of the §7.5 partition, farthest-uninspected
/// first, and a player can learn a beat. With the net dead they are not coordinated at
/// all, and there is nothing left to divide the building between them.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum PatrolStyle {
    /// **A live net.** The guard sweeps its own beat, pacing to the farthest cell it
    /// has not looked at — deterministic, and therefore learnable.
    Beat,
    /// **A silenced net** (§7.3): no partition, so the territory is the whole level the
    /// guard can reach, and the next target is drawn **at random** from the part of it
    /// the guard has not inspected.
    ///
    /// This is what the comms console buys and what it costs. Killing the net stops
    /// both §7.7 call-ins and every dispatch, and in exchange the patrols stop being
    /// predictable: you can no longer stand somewhere and know a guard will not come.
    /// Nothing here touches what a guard *sees* or how fast it moves — a silenced
    /// facility is lonelier, never blinder (§7.3).
    Wander,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Dwell {
    /// The percentage chance an arrival pauses at all ([`GUARD_DWELL_CHANCE_PERCENT`]).
    pub chance: u32,
    /// The inclusive length range, in turns — [`GUARD_DWELL_TURNS_MIN`]..=[`GUARD_DWELL_TURNS_MAX`]
    /// in a quiet facility, shortened once the alert is up
    /// ([`Alert::dwell_turns`](crate::alert::Alert::dwell_turns)).
    pub turns: (u32, u32),
}

impl Dwell {
    /// The §7.5 rule as the design's own numbers state it — a quiet facility, every
    /// arrival pausing 3–7 turns. The fixtures and guard-level tests that have no
    /// [`State`](crate::State) to ask use this.
    #[cfg(test)]
    pub(crate) const CALM: Self = Self {
        chance: GUARD_DWELL_CHANCE_PERCENT,
        turns: (GUARD_DWELL_TURNS_MIN, GUARD_DWELL_TURNS_MAX),
    };

    /// The rule with the pause switched **off** — what `set_guard_dwell_chance(0)`
    /// produces. The scene tests that want a guard walking every turn use this.
    #[cfg(test)]
    pub(crate) const NEVER: Self = Self {
        chance: 0,
        ..Self::CALM
    };

    /// [`CALM`](Self::CALM) with the chance knob turned to `chance` — for the test
    /// that sweeps it.
    #[cfg(test)]
    pub(crate) fn with_chance(chance: u32) -> Self {
        Self {
            chance,
            ..Self::CALM
        }
    }
}

impl Guard {
    /// Keep the current patrol destination while it is still worth walking to;
    /// otherwise choose the next one (§7.5). "Still worth it" means not yet reached,
    /// still a cell the guard could stand on, and **still its own ground** — a
    /// destination it has arrived at, that has become solid, or that a recut has moved
    /// into somebody else's beat, is done, and the sweep picks again.
    ///
    /// That last clause is what stops a recut ([`State::recut_beats`](crate::State))
    /// stranding a guard: a beat can shrink out from under a live destination, and a
    /// guard that walked to it anyway would spend the trip patrolling a colleague's
    /// wing. It is dropped here rather than at the recut so the guard finishes the turn
    /// it is in and picks again on its own schedule.
    ///
    /// **A replacement is only ever drawn from ground the guard can walk to**
    /// ([`walkable_ground`](Self::walkable_ground)) — the fix for #477, and the reason
    /// the *keep* clause above needs no route test of its own: see that method for why
    /// a target that was walkable when it was picked stays walkable for the whole trip.
    pub(super) fn repick_patrol_target(
        &mut self,
        facility: &Facility,
        style: PatrolStyle,
        rng: &mut Rng,
    ) {
        let territory = self.territory(facility, style);
        if let Some(dest) = self.destination {
            // A guard with no beat has no territory for a destination to be *outside*
            // of, so the ground check is vacuous there and skipped — otherwise it would
            // drop the target a beatless fixture was handed on the turn it was built.
            let ours = territory.is_empty() || territory.contains(&dest);
            if dest != self.pos && facility.can_enter(dest, ACTOR_FILL) && ours {
                return;
            }
        }
        let walkable = self.walkable_ground(facility);
        let within_reach: Vec<Cell> = territory
            .into_iter()
            .filter(|cell| walkable.contains(cell))
            .collect();
        self.destination = self.next_target_in(&within_reach, style, rng);
    }

    /// Every cell this guard could walk to from where it stands, over bare terrain
    /// (§7.5/#477): [`routable`](super::routable) ground flooded from its own cell,
    /// **colleagues ignored**.
    ///
    /// # Why the sweep has to ask
    ///
    /// A beat is cut from the region graph (§10.5, [`crate::beat`]), and that graph does
    /// not know about the **solid usables** stamped into the building afterwards. An
    /// intel console, the comms console or the exit dropped into a one-cell throat seals
    /// the cells behind it off from every guard while the region it was cut from still
    /// claims them — they are §10.3's one asymmetry, terrain that blocks a *route*
    /// without blocking *pathing*, so neither the region graph nor §10.6's
    /// player-route assert has any reason to notice. Such a cell is then the farthest
    /// thing in the beat from the opposite corner, and it can never be struck off the
    /// guard's inspected memory either, because the guard can never get eyes into the
    /// pocket. Left in the candidate set it is picked, kept, and re-picked for the rest
    /// of the run, and the guard stands on one cell forever (#477).
    ///
    /// # Why colleagues are ignored
    ///
    /// That distinction is the whole of it. A route sealed by a guard standing in it is
    /// a *this turn* problem, and holding and retrying through it is exactly right
    /// (§7.8) — folding colleagues in here would make a guard throw away a good target
    /// every time one crossed the corridor ahead. A route sealed by the building never
    /// clears. So this is drawn over bare terrain, and only the step is blocked-aware.
    ///
    /// # Why picking is the only place that has to check
    ///
    /// Guard-routability only ever *grows* during a run: the sole terrain writes in play
    /// are a door panel swapping open/closed — [`routable`](super::routable) either way,
    /// §10.4 — and Bore turning a wall into floor (§8.3), which opens ground and never
    /// closes it. Reachability is symmetric within a component and the guard cannot
    /// leave the one it stands in, so a target walkable when it was picked is walkable
    /// from every cell of the walk. And a Calm destination has no other author: the
    /// reactive states all clear theirs on the way out (`stand_down`,
    /// `release_from_search`, `begin_search`), so nothing else can hand the sweep a
    /// target it never vetted. Checking here rather than every turn keeps the flood off
    /// the per-turn path — it runs once per patrol leg — which the §13.2 sweeps care
    /// about; the invariant itself is pinned by `a_calm_guard_never_holds_a_target_it_cannot_reach`.
    ///
    /// The guard's own cell is always in the set, so a guard walled into a one-cell
    /// pocket ends up with nothing to pick and holds — §7.5's stated answer for a guard
    /// with nowhere to sweep, rather than the freeze that used to pass for it.
    fn walkable_ground(&self, facility: &Facility) -> HashSet<Cell> {
        path::flood_from(self.pos, facility.width(), facility.height(), |cell| {
            routable(facility, cell)
        })
        .into_iter()
        .collect()
    }

    /// The next cell to walk to in `territory` (§7.5): the **farthest** one the guard
    /// has not looked at on a live net, or a **random** uninspected one on a dead one
    /// ([`PatrolStyle`]).
    ///
    /// *Farthest* is what makes an ordinary patrol pace across distances instead of
    /// shuffling locally, and it is why the emergent patrols read as purposeful — keep
    /// it. Random is not an improvement on it; it is the predictability the comms
    /// console spends (§7.3).
    ///
    /// When every reachable cell has been inspected the memory is wiped and the sweep
    /// starts over, so a Calm guard never runs out of ground to cover. Takes the
    /// territory the caller has already drawn, so the per-turn repick reads it once
    /// rather than once to test the live destination and again to replace it.
    pub(super) fn next_target_in(
        &mut self,
        territory: &[Cell],
        style: PatrolStyle,
        rng: &mut Rng,
    ) -> Option<Cell> {
        let pick = |guard: &Self, inspected: &VisibleSet, rng: &mut Rng| match style {
            PatrolStyle::Beat => pick_farthest(territory, inspected, guard.pos),
            PatrolStyle::Wander => pick_random(territory, inspected, guard.pos, rng),
        };
        if let Some(cell) = pick(self, &self.inspected.clone(), rng) {
            return Some(cell);
        }
        self.inspected = VisibleSet::default();
        pick(self, &VisibleSet::default(), rng)
    }

    /// The guard's patrol territory (§7.5): the patrollable cells of its region
    /// **beat** — rooms and the corridors joining them, grown across door edges
    /// from the region the guard stood in when the beat was cut (§10.5,
    /// [`crate::beat`]) — so no territory straddles a wall into a space the guard
    /// cannot walk to, and corridors get real coverage instead of being crossed
    /// incidentally.
    ///
    /// A guard with **no beat** has no territory and holds. There is deliberately no
    /// box fallback: a flood around a remembered spawn cell was §7.5's named weakness
    /// ("territories are boxes around spawn points, which have no relationship to the
    /// building"), and an anchor a guard has since walked away from is the half of it
    /// that survived the region beat. Better to stand still than to sweep a phantom.
    pub(super) fn territory(&self, facility: &Facility, style: PatrolStyle) -> Vec<Cell> {
        // While a released search still watches the region (§7.6), the sweep draws its
        // territory around the searched area with the tighter [`WATCH_RADIUS`], so
        // coverage there stays raised; otherwise it is the full Calm territory.
        //
        // **Clipped to the guard's own beat, not substituted for it.** Every guard that
        // answered a call to one cell gets the same `focus` (§7.7 — a call carries its
        // own cell), so handing them all the same disc hands them all the same
        // territory: two responders spend the whole watch window pacing one region, and
        // `pick_farthest` being deterministic they frequently pick the same cell and
        // walk in lockstep. That reads as guards converging into a moving clump, which
        // is the failure §7.6's standing warning exists to prevent — *harder coverage
        // of one area* is the goal, and two guards tracing one path across it is not
        // coverage. Sharing a `focus` is correct; sharing a *territory* is the accident.
        //
        // Intersected, two responders watch different halves of the same area. The
        // plain disc is the fallback when the intersection is empty, because a guard
        // can always be called clear across the facility and must still watch
        // *something*.
        if let Some(focus) = self.focus {
            if self.watch > 0 {
                let disc =
                    path::reachable_within(focus, WATCH_RADIUS, |cell| patrollable(facility, cell));
                // A dead net has no partition to clip against (§7.3): every guard's
                // territory is the whole level, so the intersection would be the disc
                // anyway — and no two guards are sent to one cell in the first place,
                // since call-ins do not fire on a silenced net.
                if style == PatrolStyle::Wander {
                    return disc;
                }
                let watched: HashSet<Cell> = disc.iter().copied().collect();
                let mine: Vec<Cell> = self
                    .beat
                    .iter()
                    .copied()
                    .filter(|&cell| watched.contains(&cell) && patrollable(facility, cell))
                    .collect();
                return if mine.is_empty() { disc } else { mine };
            }
        }
        // A dead net leaves nothing dividing the building (§7.3): there is no beat to
        // sweep, so the territory is simply everywhere the guard can walk. Flooded from
        // where it stands rather than taken as the whole grid, so a guard still never
        // heads for ground it cannot reach.
        if style == PatrolStyle::Wander {
            return path::flood_from(self.pos, facility.width(), facility.height(), |cell| {
                routable(facility, cell)
            })
            .into_iter()
            .filter(|&cell| patrollable(facility, cell))
            .collect();
        }
        // Filtered at sweep time, not at placement: a console stamped in later,
        // furniture, or a cupboard is never picked as a target.
        self.beat
            .iter()
            .copied()
            .filter(|&cell| patrollable(facility, cell))
            .collect()
    }
}

/// Roll the seeded run RNG (§12.4) for a Calm patrol dwell (§7.5/§153): `Some(n)`
/// to dwell for `n` turns, `None` to walk on. `chance` is the percentage a dwell
/// starts at all — [`GUARD_DWELL_CHANCE_PERCENT`] holds it at 100, so the shipped
/// game always pauses and only the length is drawn. Mirrors the close-behind
/// discipline
/// ([`State::rolls_a_close`](crate::State)): the extremes draw nothing against the
/// *chance* — a `0` never dwells and perturbs no stream, a `100` always does — so
/// only the tuned middle spends a chance draw; a dwell that *does* start then
/// spends one more draw for its length in [`GUARD_DWELL_TURNS_MIN`]..=[`GUARD_DWELL_TURNS_MAX`].
pub(super) fn roll_dwell(rng: &mut Rng, dwell: Dwell) -> Option<u32> {
    let dwelling = match dwell.chance {
        0 => false,
        c if c >= 100 => true,
        c => rng.below(100) < c,
    };
    let (min, max) = dwell.turns;
    dwelling.then(|| rng.range_inclusive(min as i32, max as i32) as u32)
}

/// Whether a guard may **patrol through** `cell` (§7.5/§10.3): a cell it can both
/// stand on and route across. That is floor and open door panels — but *not*
/// furniture, cover or a cupboard (which patrols flow around, §10.1), and not a
/// closed door (a sweep never *targets* a doorway; walking through one is
/// [`routable`]'s job). It is deliberately stricter than [`Facility::can_enter`]:
/// a hideout admits a mover but a patrol routes around it, so the two predicates
/// must be combined.
pub(super) fn patrollable(facility: &Facility, cell: Cell) -> bool {
    facility
        .terrain(cell)
        .is_some_and(|terrain| !terrain.blocks_pathing() && facility.can_enter(cell, ACTOR_FILL))
}

/// The farthest uninspected cell in `territory` from `origin`, or `None` when every
/// cell has been looked at (§7.5). Ties are broken deterministically — nearest the
/// north-west (smallest `y`, then `x`) — so the same board always yields the same
/// sweep (§12.4). The guard's own cell is never a target.
/// A uniformly random uninspected cell of `territory`, or `None` when every cell has
/// been looked at (§7.5/§7.3) — the dead-net counterpart to [`pick_farthest`].
///
/// Drawn from the run's own seeded stream (§12.4), never a fresh source, so a silenced
/// facility reproduces exactly like any other. The guard's own cell is never a target.
///
/// **Why random rather than farthest-over-the-whole-level.** Handing every guard the
/// whole map while keeping `pick_farthest`'s deterministic tie-break would make
/// clustering *worse*: the attractors become the map's extreme corners, drawn from one
/// shared candidate set, and the per-guard `inspected` memory that would otherwise
/// separate two guards converges the moment their cones overlap — which they will,
/// since everyone is walking to the same corners. Random removes the determinism
/// causing the lockstep instead of patching around it.
pub(super) fn pick_random(
    territory: &[Cell],
    inspected: &VisibleSet,
    origin: Cell,
    rng: &mut Rng,
) -> Option<Cell> {
    let candidates: Vec<Cell> = territory
        .iter()
        .copied()
        .filter(|&cell| cell != origin && !inspected.contains(cell))
        .collect();
    if candidates.is_empty() {
        return None;
    }
    Some(candidates[rng.below(candidates.len() as u32) as usize])
}

pub(super) fn pick_farthest(
    territory: &[Cell],
    inspected: &VisibleSet,
    origin: Cell,
) -> Option<Cell> {
    territory
        .iter()
        .copied()
        .filter(|&cell| cell != origin && !inspected.contains(cell))
        .min_by_key(|&cell| {
            (
                std::cmp::Reverse(origin.manhattan_distance(cell)),
                cell.y,
                cell.x,
            )
        })
}
