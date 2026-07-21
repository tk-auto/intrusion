//! The guard: its §7.4 state of mind and its §7.5 patrol.
//!
//! A guard is a plain struct the [`State`](crate::State) owns directly (§12.3). Its
//! sight is recomputed each phase like any viewer's (§6); what lives here is the
//! *mind* — the [`GuardState`] vocabulary, the Calm patrol (§7.5), and the reactive
//! transition folded in each turn by [`sense`](Guard::sense): **sight**
//! ([`see`](Guard::see)) flipping the guard to Chasing or Investigating by the §7.6
//! two zones (certain ≤ 5, glimpse ≤ 10). Guards detect on **vision alone** (§9
//! **[SETTLED]** — no sound, no hearing). Every reactive state (chasing,
//! investigating, responding) plugs into the same [`decide`](Guard::decide) seam: it
//! sets a `destination` and reuses the shared walk-toward-it movement, so the
//! remaining guard tickets add transitions, not new machinery — and a reactive guard
//! whose lead ([`ALERT_DURATION`]) runs out stands back down to patrol on its own.
//! Movement rides on the deterministic primitives in [`crate::path`].

use crate::category::Category;
use crate::cell::{Cell, Direction};
use crate::facility::Facility;
use crate::path;
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
    /// A decoy seen, or a glimpse in the outer zone (§7.4/§7.6): as chasing, but
    /// toward the last-known cell and reported at lower severity.
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
    /// reactive states set it to their own targets (§7.4) — a heard source, a seen
    /// player's cell.
    destination: Option<Cell>,
    /// The last cell the player was seen in the **certain** zone (§7.6). A glimpse
    /// heads *here* — where the guard last knew the player precisely — not toward the
    /// imprecise glimpse itself; a glimpse never updates it. `None` until the first
    /// certain sighting, and cleared when the lead runs out ([`stand_down`](Self::stand_down)).
    last_seen: Option<Cell>,
    /// How many turns of lead this guard still has (§7.1 alert timer). Refreshed to
    /// [`ALERT_DURATION`] by a fresh detection — a seen player — and decayed by one
    /// each turn nothing is sensed ([`sense`](Self::sense)); a reactive guard whose
    /// lead reaches zero stands back down (§7.4/§7.6).
    alert: u32,
    fov: VisibleSet,
    state: GuardState,
}

/// Patrol radius (§7.5, **[START] = 15**): a guard sweeps the patrollable cells
/// within this many steps of its station.
pub(crate) const PATROL_RADIUS: u32 = 15;

/// How long a detection lead survives with nothing sensed (§7.1 alert duration,
/// **[START] = 30**). A fresh sighting resets the alert timer to this; each quiet
/// turn drops it by one, and a reactive guard gives up
/// its lead and returns to patrol once it hits zero. The bounded search this timer
/// will pace (§7.6 fix 2) is a later ticket; here it is the honest backstop that
/// keeps a guard from pursuing a stale lead forever.
pub(crate) const ALERT_DURATION: u32 = 30;

/// The **certain** detection zone (§7.6, **[START] = 5**): a player seen within this
/// Chebyshev range (the §6.1 sight metric) is tracked precisely — the guard Chases
/// its live cell. This is the range Run is tuned against: its 5-cell gain is exactly
/// the certain→glimpse distance, so breaking from Chasing to Investigating is
/// designed to be *achievable* (§7.6 — "it gives Run a job").
pub(crate) const CERTAIN_RANGE: u32 = 5;

/// The **glimpse** zone's outer edge (§7.6, **[START] = 10**): past [`CERTAIN_RANGE`]
/// and out to here the guard only catches imprecise movement — it Investigates toward
/// where it *last knew* the player (the certain cell), not the glimpse. It equals the
/// guard's sight range ([`GUARD_SIGHT_RANGE`], §7.1): beyond it there is no cone to be
/// seen in, so "> 10 → detects nothing" falls out of the cone itself.
pub(crate) const GLIMPSE_RANGE: u32 = GUARD_SIGHT_RANGE;

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
            last_seen: None,
            alert: 0,
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

    /// The guard's whole turn of sensing (§4.2 phase 3), run before it acts: a lead
    /// **cools** by one turn, then sight gets its say and refreshes it if the guard
    /// detects the player. Detection is vision alone (§9 **[SETTLED]** — guards do not
    /// hear). `concealed` folds in the one concealment query (§10.3): a player in a
    /// cupboard or ducked behind the right table is not seen, so the lead just cools —
    /// which is exactly the "hold still and watch the cone sweep past" payoff (§7.6).
    pub(crate) fn sense(&mut self, player: Cell, concealed: bool) {
        // A lead cools by default; a sighting below resets it to full.
        self.alert = self.alert.saturating_sub(1);
        self.see(player, concealed);
    }

    /// React to seeing the player (§7.6 two-zone detection). Nothing happens if the
    /// player is [`concealed`](crate::State::concealed_from) from this guard or simply
    /// not in its cone this turn (the lead cools in [`sense`](Self::sense)). Otherwise
    /// the Chebyshev range decides:
    ///
    /// - **certain** (≤ [`CERTAIN_RANGE`]): Chase the player's *live* cell, and record
    ///   it as the last cell known precisely.
    /// - **glimpse** (≤ [`GLIMPSE_RANGE`]): Investigate toward that last-certain cell —
    ///   where the guard last *knew* the player, not the imprecise glimpse. Before any
    ///   certain sighting there is no such cell, so it falls back to the glimpse itself
    ///   — the only position it has.
    ///
    /// Either way the alert timer is refreshed. Because [`GLIMPSE_RANGE`] equals the
    /// cone's own range there is no "seen but past the glimpse" case to handle — a cell
    /// past 10 is simply not in the cone.
    fn see(&mut self, player: Cell, concealed: bool) {
        if concealed || !self.fov.contains(player) {
            return;
        }
        let range = self.pos.sight_distance(player);
        if range <= CERTAIN_RANGE {
            self.state = GuardState::Chasing;
            self.destination = Some(player);
            self.last_seen = Some(player);
            self.alert = ALERT_DURATION;
        } else if range <= GLIMPSE_RANGE {
            self.state = GuardState::Investigating;
            self.destination = self.last_seen.or(Some(player));
            self.alert = ALERT_DURATION;
        }
    }

    /// The direction the guard will try this turn, or `None` to hold (§7.4 phase 3).
    ///
    /// The guard first folds this turn's cone into its inspected-cell memory — it has
    /// *looked at* everything it can see. Then a **reactive** guard (Chasing or
    /// Investigating, §7.6) walks the destination its transition set; the moment it
    /// can no longer make progress — it has arrived, or the lead led somewhere it
    /// cannot route to — its lead is spent and it **stands back down to patrol**. With no
    /// search or alert-timer machinery yet (§7.6 fix #2 is a later ticket) that is the
    /// honest end of an investigation: reach the spot, find nothing, resume the sweep.
    /// A **Calm** guard picks its next patrol target and steps toward it (§7.5). A
    /// held-in-place guard, or a Calm one with nowhere to go, holds.
    /// `blocked` are the cells other guards currently stand on: guards are solid to
    /// each other and must **path around** a colleague, not through one (§7.8). A
    /// route the pass finds steps only into cells no other guard holds, so a guard
    /// whose direct line is blocked reroutes down the parallel lane (corridors are
    /// 2–4 wide, §10.1) instead of stalling. When a colleague genuinely seals the
    /// only route this turn, the guard holds and retries next turn as the colleague
    /// clears — a local wait-and-retry, no reservation system (§12.3), and no
    /// deadlock the old path-through-each-other stall produced.
    pub(crate) fn decide(&mut self, facility: &Facility, blocked: &[Cell]) -> Option<Direction> {
        if !self.patrols {
            return None;
        }
        self.inspected.absorb(&self.fov);

        if self.state != GuardState::Calm {
            // Pursue the lead only while it is still warm: a guard that has arrived,
            // has no route, or whose alert has fully cooled (§7.1) gives it up and
            // stands back down to patrol. The bounded search that would fill the
            // gap before giving up (§7.6 fix 2) is a later ticket.
            if self.alert > 0 {
                if let Some(step) = self.step_toward_destination(facility, blocked) {
                    return Some(step);
                }
            }
            self.stand_down();
        }

        self.repick_patrol_target(facility);
        self.step_toward_destination(facility, blocked)
    }

    /// The first step of the shortest patrollable path to the current destination that
    /// routes **around** the cells in `blocked` (colleagues, §7.8), or `None` when
    /// there is nothing to walk to — no destination, already stood on it, or no
    /// unobstructed route reaches it (the guard then holds and retries next turn).
    /// The destination itself is exempt from `blocked` — as it is from `patrollable`
    /// (a guard may be sent onto a cell it cannot end on) — so a lead pointing at a
    /// colleague's cell still draws the guard toward it rather than freezing the sweep.
    fn step_toward_destination(&self, facility: &Facility, blocked: &[Cell]) -> Option<Direction> {
        let destination = self.destination?;
        if destination == self.pos {
            return None;
        }
        path::first_step_toward(self.pos, destination, |cell| {
            patrollable(facility, cell) && !blocked.contains(&cell)
        })
    }

    /// Drop back to Calm patrol, clearing the reactive lead — destination, alert
    /// timer and last-known cell — so the next
    /// [`repick_patrol_target`](Self::repick_patrol_target) chooses a fresh sweep and
    /// a later encounter starts clean rather than heading for a stale sighting.
    fn stand_down(&mut self) {
        self.state = GuardState::Calm;
        self.destination = None;
        self.last_seen = None;
        self.alert = 0;
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
    use crate::facility::Facility;
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

    /// §7.6: a reactive guard that reaches its destination and finds nothing — there
    /// is no search machinery yet — stands back down to patrol rather than freezing on
    /// the spot. Driven by sight (§9 **[SETTLED]** — guards do not hear): a glimpse
    /// down the cone sends the guard Investigating toward that cell, and once standing
    /// on it with nothing more seen the lead is spent.
    #[test]
    fn a_reactive_guard_stands_down_on_arrival() {
        let facility = Facility::walled_box(11, 13);
        let mut guard = Guard::patrolling(Cell::new(5, 2)); // faces south (§7.1)
        guard.look(&facility);
        let glimpse = Cell::new(5, 9); // 7 down the cone: the glimpse zone
        assert!(guard.fov().contains(glimpse), "precondition: in the cone");

        // A glimpse with no prior certain sighting Investigates toward the glimpse
        // itself — the only position the guard has.
        guard.sense(glimpse, false);
        assert_eq!(guard.state(), GuardState::Investigating);

        // Stand the guard on that destination, then decide with nothing seen: arrived,
        // so the lead is spent and it resumes patrol.
        guard.advance_to(glimpse, Direction::South, &facility);
        let _ = guard.decide(&facility, &[]);
        assert_eq!(
            guard.state(),
            GuardState::Calm,
            "arrived with nothing found → resume patrol",
        );
    }

    /// §7.6 two-zone detection **[START]**: the boundaries and the alert duration are
    /// pinned so a later change is a visible edit, and the glimpse edge is exactly the
    /// cone's own range — past it there is no cone to be seen in.
    #[test]
    fn the_detection_zones_and_alert_are_pinned() {
        assert_eq!(CERTAIN_RANGE, 5, "the [START] certain zone");
        assert_eq!(GLIMPSE_RANGE, 10, "the [START] glimpse-zone edge");
        assert_eq!(ALERT_DURATION, 30, "the [START] alert duration");
        assert_eq!(
            GLIMPSE_RANGE, GUARD_SIGHT_RANGE,
            "the glimpse edge is the cone's own range",
        );
    }

    /// §7.6 certain zone: a player seen within [`CERTAIN_RANGE`] flips the guard to
    /// Chasing its **live** cell and refreshes the alert timer. The last-known-precise
    /// cell is recorded for a later glimpse to fall back on.
    #[test]
    fn a_player_in_the_certain_zone_is_chased_at_its_live_cell() {
        let facility = Facility::walled_box(11, 11);
        let mut guard = Guard::stationary(Cell::new(5, 3)); // faces south (§7.1)
        guard.look(&facility);
        let player = Cell::new(5, 7); // 4 cells down the cone: certain
        assert!(guard.fov.contains(player), "precondition: in the cone");

        guard.see(player, false);
        assert_eq!(guard.state(), GuardState::Chasing);
        assert_eq!(guard.destination, Some(player), "tracks the live cell");
        assert_eq!(guard.last_seen, Some(player), "records the certain cell");
        assert_eq!(guard.alert, ALERT_DURATION);
    }

    /// §7.6 glimpse zone: past [`CERTAIN_RANGE`] but within [`GLIMPSE_RANGE`] the guard
    /// only catches imprecise movement, so it Investigates toward where it *last knew*
    /// the player — the certain cell — not the imprecise glimpse itself.
    #[test]
    fn a_glimpse_investigates_toward_the_last_certain_cell() {
        let facility = Facility::walled_box(11, 13);
        let mut guard = Guard::stationary(Cell::new(5, 2)); // faces south
        guard.look(&facility);
        let certain = Cell::new(5, 6); // 4 down: certain — sets the precise memory
        let glimpse = Cell::new(5, 10); // 8 down: glimpse
        assert!(guard.fov.contains(glimpse), "precondition: in the cone");

        guard.see(certain, false);
        assert_eq!(guard.last_seen, Some(certain));

        guard.see(glimpse, false);
        assert_eq!(guard.state(), GuardState::Investigating);
        assert_eq!(
            guard.destination,
            Some(certain),
            "heads for where it last knew you, not the glimpse",
        );
        assert_eq!(guard.alert, ALERT_DURATION);
    }

    /// §10.3/§7.6: a concealed player — in a cupboard, or ducked behind the right
    /// table — is not detected by sight even standing in the cone. This is the AND-in
    /// the danger overlay already honours (§11.5), carried into the guard's mind.
    #[test]
    fn a_concealed_player_in_the_cone_is_not_seen() {
        let facility = Facility::walled_box(11, 11);
        let mut guard = Guard::stationary(Cell::new(5, 3));
        guard.look(&facility);
        let player = Cell::new(5, 7);
        assert!(guard.fov.contains(player), "precondition: in the cone");

        guard.see(player, true); // concealed from this guard
        assert_eq!(
            guard.state(),
            GuardState::Calm,
            "concealment blocks detection"
        );
        assert_eq!(guard.destination, None);
        assert_eq!(guard.alert, 0);
    }

    /// §7.6 "gone" zone: beyond [`GLIMPSE_RANGE`] there is no cone to be seen in, so a
    /// player past the guard's range is simply not in its FOV and detection does
    /// nothing this turn.
    #[test]
    fn a_player_beyond_the_glimpse_range_is_not_seen() {
        let facility = Facility::walled_box(11, 20);
        let mut guard = Guard::stationary(Cell::new(5, 2));
        guard.look(&facility);
        let far = Cell::new(5, 2 + GLIMPSE_RANGE + 1); // one past the cone's range
        assert!(!guard.fov.contains(far), "precondition: out of the cone");

        guard.see(far, false);
        assert_eq!(guard.state(), GuardState::Calm, "> 10 detects nothing");
    }

    /// §7.1/§7.6: a lead cools by one each turn nothing is sensed, and a reactive guard
    /// whose alert reaches zero gives it up and stands back down to patrol — the honest
    /// end of a chase whose sight was broken, ahead of the bounded search (§7.6 fix 2)
    /// a later ticket adds. This is the anti-tracking-turret backstop: the guard cannot
    /// pursue a stale lead forever.
    #[test]
    fn a_cold_lead_stands_the_guard_down() {
        let facility = Facility::walled_box(11, 11);
        let mut guard = Guard::patrolling(Cell::new(5, 3));
        guard.look(&facility);
        guard.see(Cell::new(5, 7), false);
        assert_eq!(guard.state(), GuardState::Chasing);
        assert_eq!(guard.alert, ALERT_DURATION);

        // The player vanishes (concealed each turn): the lead cools turn by turn.
        for remaining in (0..ALERT_DURATION).rev() {
            guard.sense(Cell::new(5, 7), true);
            assert_eq!(guard.alert, remaining, "the lead cools by one a turn");
        }

        // With the lead cold, deciding stands the guard down to patrol.
        guard.decide(&facility, &[]);
        assert_eq!(guard.state(), GuardState::Calm, "a cold lead is given up");
    }
}
