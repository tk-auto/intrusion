//! The guard: its §7.4 state of mind and its §7.5 patrol.
//!
//! A guard is a plain struct the [`State`](crate::State) owns directly (§12.3). Its
//! sight is recomputed each phase like any viewer's (§6); what lives here is the
//! *mind* — the [`GuardState`] vocabulary, the Calm patrol (§7.5), and the first
//! reactive transition: **hearing** ([`hear`](Guard::hear)), a heard noise (§9.1)
//! turning the guard to Investigating toward its source. Every reactive state (chasing,
//! investigating, responding) plugs into the same [`decide`](Guard::decide) seam: it
//! sets a `destination` and reuses the shared walk-toward-it movement, so the
//! remaining guard tickets add transitions, not new machinery — and a reactive guard
//! whose lead runs out stands back down to patrol on its own. Movement rides on the
//! deterministic primitives in [`crate::path`].

use std::cmp::Reverse;

use crate::category::Category;
use crate::cell::{Cell, Direction};
use crate::facility::Facility;
use crate::path;
use crate::sound::{audible_field, Sound, HEARING_THRESHOLD};
use crate::state::ACTOR_FILL;
use crate::vision::{field_of_view, VisibleSet, GUARD_SIGHT_ARC, GUARD_SIGHT_RANGE};

/// The guard's mind — the §7.4 state machine's vocabulary.
///
/// The *transitions* (detection, timers, dispatch) are the guard AI tickets; what
/// is settled now is the seam the presentation reads: every state declares the
/// information [`Category`] it presents as ([`GuardState::category`]), and the
/// renderer re-categorises the `g` glyph from it every turn (§11.2) — yellow →
/// orange → red *is* the guard's mind, made visible.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GuardState {
    /// The default: nothing seen, nothing suspected. Patrols (§7.5).
    Calm,
    /// Alert timer > 0 but nothing seen this turn: walking to a destination, then
    /// searching it (§7.6).
    Alerted,
    /// The player was detected this turn: heading for their live cell (§7.6).
    Chasing,
    /// A decoy or a sound was detected: as chasing, but toward the source and
    /// reported at lower severity (§7.4).
    Investigating,
    /// Dispatched by a missed radio ping (§7.3): walking to the silent guard's post.
    Responding,
}

impl GuardState {
    /// The information category this state presents as — the §7.4 colour column,
    /// spoken in §11.2's vocabulary (never a concrete colour): an unaware threat is
    /// Caution, a hunting one Warning, one that has you Danger.
    pub fn category(self) -> Category {
        match self {
            GuardState::Calm => Category::Caution,
            GuardState::Alerted | GuardState::Responding => Category::Warning,
            GuardState::Chasing | GuardState::Investigating => Category::Danger,
        }
    }
}

/// A guard on the level.
///
/// A Calm guard **patrols** (§7.5): from its station it sweeps toward the farthest
/// cell in its territory it has not recently looked at, keeping a private memory of
/// the cells its cone has covered and wiping it to start over once the territory is
/// exhausted. It has a real field of view — the ~90° cone (§6.2/§7.1), recomputed
/// every sight phase — a [`GuardState`], and a `destination` it walks to along the
/// shortest patrollable path (routing *around* furniture, cover and cupboards, and
/// not through doors it cannot yet open). The reactive §7.4 states (chasing,
/// investigating, responding) are the later guard tickets: they set `destination`
/// their own way and reuse this same walk-toward-it movement.
#[derive(Clone, Debug)]
pub struct Guard {
    pos: Cell,
    facing: Direction,
    /// The spawn cell and the centre of the patrol territory (§7.5).
    station: Cell,
    /// Whether this guard patrols. `false` is a held-in-place fixture — a guard that
    /// only looks, for the sight and placement tests that need a fixed cone; `true`
    /// is the live §7.5 sweep.
    patrols: bool,
    /// Private memory of the cells this guard has looked at (§7.5): the running union
    /// of its fields of view, accumulated exactly as the player's tile memory is.
    /// Patrol heads for the farthest cell *not* in here; when the territory is fully
    /// inspected this is wiped and the sweep restarts.
    inspected: VisibleSet,
    /// The cell the guard is walking to, if any. Calm patrol picks it (§7.5); the
    /// reactive states will set it to their own targets (§7.4).
    destination: Option<Cell>,
    fov: VisibleSet,
    state: GuardState,
}

/// Patrol radius (§7.5, **[START] = 15**): a guard sweeps the patrollable cells
/// within this many steps of its station.
pub(crate) const PATROL_RADIUS: u32 = 15;

/// Every guard looks **south** at spawn (§7.1). One definition, shared by the
/// constructors below and by placement's turn-one-safety check (§10.6, `place`) —
/// if the spawn facing ever changes, the "no guard eyes the player's spawn"
/// guarantee moves with it instead of silently lying.
pub(crate) const GUARD_INITIAL_FACING: Direction = Direction::South;

impl Guard {
    /// A guard that holds its cell — it looks but never patrols. The fixture for the
    /// sight and placement tests that pin a fixed, spawn-facing cone.
    pub fn stationary(pos: Cell) -> Self {
        Self {
            pos,
            facing: GUARD_INITIAL_FACING,
            station: pos,
            patrols: false,
            inspected: VisibleSet::default(),
            destination: None,
            fov: VisibleSet::default(),
            state: GuardState::Calm,
        }
    }

    /// A guard that patrols its territory around `pos` (§7.5).
    pub fn patrolling(pos: Cell) -> Self {
        Self {
            patrols: true,
            ..Self::stationary(pos)
        }
    }

    /// A patrolling guard already walking toward `destination` — the fixture that
    /// drives a guard along a known line before the §7.4 reactive transitions that
    /// set destinations themselves land. The guard heads there along the shortest
    /// patrollable path and, on arrival, resumes picking its own patrol targets.
    pub fn patrolling_to(pos: Cell, destination: Cell) -> Self {
        Self {
            destination: Some(destination),
            ..Self::patrolling(pos)
        }
    }

    /// The same guard in `state`. The §7.4 transitions are the reactive guard AI
    /// tickets' job; until they land, this is how a scenario — a test, the sim —
    /// puts a guard in a non-[`Calm`](GuardState::Calm) state.
    pub fn with_state(mut self, state: GuardState) -> Self {
        self.state = state;
        self
    }

    /// Where the guard stands.
    pub fn pos(&self) -> Cell {
        self.pos
    }

    /// Where the guard is looking: south at spawn (§7.1), then the direction of its
    /// last successful step — facing follows movement, for guards as for the player
    /// (§5), and a blocked step does not turn it.
    pub fn facing(&self) -> Direction {
        self.facing
    }

    /// The guard's field of view — the ~90° forward wedge (§6.2/§7.1), current as of
    /// the last time this guard stood still or moved. This is the set the danger
    /// overlay paints (§11.5) and the detection the guard AI will read: one truth,
    /// so the picture and the rules cannot disagree.
    pub fn fov(&self) -> &VisibleSet {
        &self.fov
    }

    /// The guard's §7.4 state — what its mind is doing. The renderer derives the
    /// `g` glyph's category from this every turn ([`GuardState::category`]), so
    /// the state machine is readable straight off the screen (§11.2).
    pub fn state(&self) -> GuardState {
        self.state
    }

    /// Recompute this guard's cone from its current position and facing (§6.2/§7.1).
    /// The sight phase calls this for every guard before any of them act, so the
    /// decisions below read a cone that matches where the guard actually stands.
    pub(crate) fn look(&mut self, facility: &Facility) {
        self.fov = field_of_view(
            facility,
            self.pos,
            self.facing,
            GUARD_SIGHT_ARC,
            GUARD_SIGHT_RANGE,
        );
    }

    /// Apply a successful step (§4.2 phase 3): stand on `dest`, face `dir` — facing
    /// follows movement (§5) — and refresh the cone at once, so a frame never shows
    /// the guard in one place with its sight in another (§11.5).
    pub(crate) fn advance_to(&mut self, dest: Cell, dir: Direction, facility: &Facility) {
        self.pos = dest;
        self.facing = dir;
        self.look(facility);
    }

    /// Move onto `cell` without re-aiming — the capturing step (§4.5), after which
    /// the run is over and the cone no longer matters.
    pub(crate) fn place_at(&mut self, cell: Cell) {
        self.pos = cell;
    }

    /// React to the turn's noise (§9.1) — the guard's hearing, its whole mind's worth.
    /// If any of `sounds` reaches this guard's own cell above [`HEARING_THRESHOLD`],
    /// the guard turns to Investigating toward the **loudest** such source — where
    /// that noise happened. A sound the field cannot reach (walled off) never clears
    /// the threshold, so the propagation's "flows around, not through" carries into
    /// hearing for free; the guard evaluates each sound's field at its own cell.
    ///
    /// Ties between equally-loud sources break toward the north-west (smallest `y`,
    /// then `x`), so the reaction is deterministic (§12.4) whatever order the sounds
    /// were emitted in. The turn loop calls this before the guards emit their own
    /// noise, so a guard reacts to the player, never to another guard's footsteps.
    ///
    /// [`HEARING_THRESHOLD`]: crate::HEARING_THRESHOLD
    pub(crate) fn hear(&mut self, facility: &Facility, sounds: &[Sound]) {
        let here = self.pos;
        let loudest = sounds
            .iter()
            .map(|&sound| {
                (
                    audible_field(facility, sound).intensity_at(here),
                    sound.source,
                )
            })
            .filter(|&(intensity, _)| intensity > HEARING_THRESHOLD)
            .max_by_key(|&(intensity, source)| (intensity, Reverse(source.y), Reverse(source.x)));
        if let Some((_, source)) = loudest {
            self.investigate(source);
        }
    }

    /// Turn toward a heard noise (§9.1/§7.4): switch to Investigating with `source` —
    /// where the noise happened — as the destination, then walk there through the
    /// same [`decide`](Self::decide) movement as a patrol. It commits to the noise's
    /// *location*, not the player's live cell, and re-hearing simply re-aims it — so
    /// sound is a stale, last-known-position lead (§9.1's "best guess"), never a
    /// tracking turret (§7.6).
    fn investigate(&mut self, source: Cell) {
        self.state = GuardState::Investigating;
        self.destination = Some(source);
    }

    /// The direction the guard will try this turn, or `None` to hold (§7.4 phase 3).
    ///
    /// The guard first folds this turn's cone into its inspected-cell memory — it has
    /// *looked at* everything it can see. Then a **reactive** guard (Investigating,
    /// §9.1) walks the destination its transition set; the moment it can no longer
    /// make progress — it has arrived, or the noise came from somewhere it cannot
    /// route to — its lead is spent and it **stands back down to patrol**. With no
    /// search or alert-timer machinery yet (§7.6 fix #2 is a later ticket) that is the
    /// honest end of an investigation: reach the spot, find nothing, resume the sweep.
    /// A **Calm** guard picks its next patrol target and steps toward it (§7.5). A
    /// held-in-place guard, or a Calm one with nowhere to go, holds.
    pub(crate) fn decide(&mut self, facility: &Facility) -> Option<Direction> {
        if !self.patrols {
            return None;
        }
        self.inspected.absorb(&self.fov);

        if self.state != GuardState::Calm {
            if let Some(step) = self.step_toward_destination(facility) {
                return Some(step);
            }
            self.stand_down();
        }

        self.repick_patrol_target(facility);
        self.step_toward_destination(facility)
    }

    /// The first step of the shortest patrollable path to the current destination, or
    /// `None` when there is nothing to walk to — no destination, already stood on it,
    /// or no patrollable route reaches it.
    fn step_toward_destination(&self, facility: &Facility) -> Option<Direction> {
        let destination = self.destination?;
        if destination == self.pos {
            return None;
        }
        path::first_step_toward(self.pos, destination, |cell| patrollable(facility, cell))
    }

    /// Drop back to Calm patrol, clearing the reactive destination so the next
    /// [`repick_patrol_target`](Self::repick_patrol_target) chooses a fresh sweep.
    fn stand_down(&mut self) {
        self.state = GuardState::Calm;
        self.destination = None;
    }

    /// Keep the current patrol destination while it is still worth walking to;
    /// otherwise choose the next one (§7.5). "Still worth it" means not yet reached
    /// and still a cell the guard could stand on — a destination it has arrived at,
    /// or that has become solid, is done, and the sweep picks again.
    fn repick_patrol_target(&mut self, facility: &Facility) {
        if let Some(dest) = self.destination {
            if dest != self.pos && facility.can_enter(dest, ACTOR_FILL) {
                return;
            }
        }
        self.destination = self.farthest_uninspected(facility);
    }

    /// The farthest cell in territory the guard has not looked at (§7.5) — *farthest*,
    /// so patrols pace across distances instead of shuffling locally. When every
    /// reachable cell has been inspected the memory is wiped and the sweep starts
    /// over, so a Calm guard never runs out of ground to cover.
    fn farthest_uninspected(&mut self, facility: &Facility) -> Option<Cell> {
        let territory = self.territory(facility);
        if let Some(cell) = pick_farthest(&territory, &self.inspected, self.pos) {
            return Some(cell);
        }
        self.inspected = VisibleSet::default();
        pick_farthest(&territory, &self.inspected, self.pos)
    }

    /// The guard's patrol territory (§7.5): the patrollable cells reachable from its
    /// station without leaving the [`PATROL_RADIUS`] disc. A bounded flood fill, so a
    /// box territory cannot spill through walls into a room the guard can't actually
    /// walk to — the cheap slice of the §10.5 fix the ticket asks for, short of full
    /// region assignment.
    fn territory(&self, facility: &Facility) -> Vec<Cell> {
        path::reachable_within(self.station, PATROL_RADIUS, |cell| {
            patrollable(facility, cell)
        })
    }
}

/// Whether a guard may **patrol through** `cell` (§7.5/§10.3): a cell it can both
/// stand on and route across. That is floor and open door panels — but *not*
/// furniture, cover or a cupboard (which patrols flow around, §10.1), and not a
/// closed door (which this guard cannot yet open). It is deliberately stricter than
/// [`Facility::can_enter`]: a hideout admits a mover but a patrol routes around it,
/// so the two predicates must be combined.
fn patrollable(facility: &Facility, cell: Cell) -> bool {
    facility
        .terrain(cell)
        .is_some_and(|terrain| !terrain.blocks_pathing() && facility.can_enter(cell, ACTOR_FILL))
}

/// The farthest uninspected cell in `territory` from `origin`, or `None` when every
/// cell has been looked at (§7.5). Ties are broken deterministically — nearest the
/// north-west (smallest `y`, then `x`) — so the same board always yields the same
/// sweep (§12.4). The guard's own cell is never a target.
fn pick_farthest(territory: &[Cell], inspected: &VisibleSet, origin: Cell) -> Option<Cell> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facility::{Facility, Terrain};
    use crate::sound::Loudness;
    use crate::vision::WAIT_SIGHT_ARC;

    /// §7.5: patrol territory is the patrollable cells within [`PATROL_RADIUS`] of the
    /// station. The radius is pinned here so a later change to the **[START] = 15**
    /// value is visible — a floor cell exactly at the radius is in, one step past is
    /// out.
    #[test]
    fn patrol_territory_is_bounded_by_the_radius() {
        // A room large enough that the radius, not a wall, is what bounds it.
        let facility = Facility::walled_box(60, 60);
        let station = Cell::new(30, 30);
        let territory = Guard::patrolling(station).territory(&facility);

        assert_eq!(PATROL_RADIUS, 15, "the [START] patrol radius");
        assert!(
            territory
                .iter()
                .all(|&c| station.manhattan_distance(c) <= PATROL_RADIUS),
            "no cell beyond the radius is in territory",
        );
        assert!(
            territory.contains(&Cell::new(30 + PATROL_RADIUS, 30)),
            "a floor cell exactly at the radius is in territory",
        );
        assert!(
            !territory.contains(&Cell::new(30 + PATROL_RADIUS + 1, 30)),
            "one step past the radius is out",
        );
    }

    /// §7.5: with no destination a Calm guard walks to the **farthest** uninspected
    /// cell in its territory — *farthest*, not nearest, so patrols pace across
    /// distances. Ties resolve toward the north-west, deterministically (§12.4).
    #[test]
    fn patrol_picks_the_farthest_uninspected_cell() {
        let nothing_seen = VisibleSet::default();
        let origin = Cell::new(1, 1);

        // (1,4) at distance 3 beats (3,1) at distance 2 — farthest, not nearest.
        let spread = [Cell::new(3, 1), Cell::new(1, 4)];
        assert_eq!(
            pick_farthest(&spread, &nothing_seen, origin),
            Some(Cell::new(1, 4)),
        );

        // Equidistant cells (both at distance 3) break toward the smaller y, then x.
        let tied = [Cell::new(1, 4), Cell::new(4, 1)];
        assert_eq!(
            pick_farthest(&tied, &nothing_seen, origin),
            Some(Cell::new(4, 1)),
        );
    }

    /// §7.5: when every cell in reach has been looked at, the inspected-cell memory
    /// is wiped and the sweep starts over — a Calm guard never runs out of ground.
    #[test]
    fn patrol_memory_wipes_when_the_territory_is_exhausted() {
        let facility = Facility::walled_box(5, 5); // a 3×3 interior
        let mut guard = Guard::patrolling(Cell::new(2, 2));
        // The guard has looked at its whole territory: fold a full-circle view in.
        let whole_room = field_of_view(
            &facility,
            Cell::new(2, 2),
            Direction::South,
            WAIT_SIGHT_ARC,
            2,
        );
        guard.inspected.absorb(&whole_room);

        let territory = guard.territory(&facility);
        assert!(
            pick_farthest(&territory, &guard.inspected, guard.pos()).is_none(),
            "precondition: nothing is left uninspected",
        );

        // Asking for the next target wipes the exhausted memory and finds one again.
        assert!(
            guard.farthest_uninspected(&facility).is_some(),
            "the sweep restarts instead of stalling",
        );
        assert!(
            pick_farthest(&guard.territory(&facility), &guard.inspected, guard.pos()).is_some(),
            "memory was wiped — cells read as uninspected again",
        );
    }

    /// §9.1: a sound loud enough at the guard's own cell flips a Calm guard to
    /// Investigating toward its source, and the guard then walks there through the
    /// shared patrol movement — hearing driving the reactive seam.
    #[test]
    fn a_heard_sound_sends_the_guard_investigating_toward_its_source() {
        let facility = Facility::walled_box(8, 3); // interior x∈1..=6 at y=1
        let mut guard = Guard::patrolling(Cell::new(1, 1));
        let source = Cell::new(5, 1);
        // A High sound four cells east: intensity 12 − 4 = 8, well over the threshold.
        guard.hear(
            &facility,
            &[Sound {
                source,
                intensity: Loudness::High.intensity(),
            }],
        );
        assert_eq!(guard.state(), GuardState::Investigating);
        assert_eq!(
            guard.decide(&facility),
            Some(Direction::East),
            "walks toward the source",
        );
    }

    /// §9.1 **[START]**: hearing needs the intensity at the guard's cell to *exceed*
    /// [`HEARING_THRESHOLD`]. A Low footstep two cells off lands exactly at the
    /// threshold and is missed; one cell off clears it. The threshold is pinned so a
    /// later change is visible.
    #[test]
    fn the_hearing_threshold_is_exact_and_pinned() {
        assert_eq!(HEARING_THRESHOLD, 1, "the [START] hearing threshold");
        let facility = Facility::walled_box(10, 3);

        let mut guard = Guard::patrolling(Cell::new(1, 1));
        // Two cells east: Low 3 − 2 = 1, which does not *exceed* 1. Not heard.
        guard.hear(
            &facility,
            &[Sound {
                source: Cell::new(3, 1),
                intensity: Loudness::Low.intensity(),
            }],
        );
        assert_eq!(guard.state(), GuardState::Calm, "3 − 2 = 1 is not above 1");

        // One cell east: Low 3 − 1 = 2 > 1. Heard.
        guard.hear(
            &facility,
            &[Sound {
                source: Cell::new(2, 1),
                intensity: Loudness::Low.intensity(),
            }],
        );
        assert_eq!(
            guard.state(),
            GuardState::Investigating,
            "3 − 1 = 2 is above 1"
        );
    }

    /// §9.1's headline carries into hearing: sound **flows around walls, not through
    /// them**. A loud source sealed behind a wall never reaches the guard's cell, so
    /// it is not heard even though it is only two cells away in a straight line.
    #[test]
    fn a_sound_behind_a_wall_is_not_heard() {
        // 5×5 box; wall the whole x=2 column, sealing the guard (x=1) from the source.
        let mut facility = Facility::walled_box(5, 5);
        for y in 1..=3 {
            facility.set_terrain(2, y, Terrain::Wall);
        }
        let mut guard = Guard::patrolling(Cell::new(1, 2));
        guard.hear(
            &facility,
            &[Sound {
                source: Cell::new(3, 2),
                intensity: Loudness::High.intensity(),
            }],
        );
        assert_eq!(
            guard.state(),
            GuardState::Calm,
            "a wall stops the sound, so it is never heard",
        );
    }

    /// §9.1: among several audible sounds the guard turns to the **loudest** at its
    /// own cell — the nearer of two equally-emitted sounds here.
    #[test]
    fn the_guard_investigates_the_loudest_source() {
        let facility = Facility::walled_box(13, 3);
        let mut guard = Guard::patrolling(Cell::new(6, 1));
        let near = Cell::new(4, 1); // 2 away → 12 − 2 = 10
        let far = Cell::new(11, 1); // 5 away → 12 − 5 = 7
        guard.hear(
            &facility,
            &[
                Sound {
                    source: far,
                    intensity: Loudness::High.intensity(),
                },
                Sound {
                    source: near,
                    intensity: Loudness::High.intensity(),
                },
            ],
        );
        assert_eq!(guard.state(), GuardState::Investigating);
        assert_eq!(
            guard.decide(&facility),
            Some(Direction::West),
            "toward the louder, nearer source",
        );
    }

    /// Ties break deterministically (§12.4): two equally-loud sources resolve to the
    /// north-west one (smallest `y`, then `x`), so the reaction never depends on the
    /// order the sounds were emitted in.
    #[test]
    fn equally_loud_sources_break_toward_the_north_west() {
        let facility = Facility::walled_box(7, 7);
        let west = Cell::new(1, 3); // 2 away, y = 3
        let north = Cell::new(3, 1); // 2 away, y = 1 — smaller y wins the tie
        let heard_from = |order: [Cell; 2]| {
            let mut g = Guard::patrolling(Cell::new(3, 3));
            g.hear(
                &facility,
                &order.map(|source| Sound {
                    source,
                    intensity: Loudness::High.intensity(),
                }),
            );
            g.decide(&facility)
        };
        assert_eq!(heard_from([west, north]), Some(Direction::North));
        assert_eq!(
            heard_from([north, west]),
            Some(Direction::North),
            "same result whatever the emission order",
        );
    }

    /// §7.6/§9.1: an Investigating guard that reaches the noise finds nothing — there
    /// is no search machinery yet — and stands back down to patrol rather than
    /// freezing on the spot.
    #[test]
    fn an_investigating_guard_stands_down_on_arrival() {
        let facility = Facility::walled_box(6, 3);
        let mut guard = Guard::patrolling(Cell::new(1, 1));
        let source = Cell::new(2, 1); // one step east
        guard.hear(
            &facility,
            &[Sound {
                source,
                intensity: Loudness::High.intensity(),
            }],
        );
        assert_eq!(guard.state(), GuardState::Investigating);

        let dir = guard.decide(&facility).expect("a step toward the source");
        assert_eq!(dir, Direction::East);
        guard.advance_to(source, dir, &facility);

        // Standing on the source now: nothing to find, so the lead is spent.
        let _ = guard.decide(&facility);
        assert_eq!(
            guard.state(),
            GuardState::Calm,
            "arrived with nothing found → resume patrol",
        );
    }
}
