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
use crate::facility::{Facility, Terrain};
use crate::path;
use crate::radio::RadioClock;
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
/// exhausted. On a generated level its territory is a region **beat** (§10.5, see
/// [`crate::beat`]): rooms *and the corridors joining them*, grown from the
/// station's region across door edges, so the sweep walks room → corridor → room.
/// It has a real field of view — the ~90° cone (§6.2/§7.1), recomputed every sight
/// phase — a [`GuardState`], and a `destination` it walks to along the shortest
/// routable path (routing *around* furniture, cover and cupboards, and straight
/// **through closed doors**, which it opens by walking in, §10.4). The reactive
/// §7.4 states (chasing, investigating, responding) are the later guard tickets:
/// they set `destination` their own way and reuse this same walk-toward-it
/// movement.
#[derive(Clone, Debug)]
pub struct Guard {
    pos: Cell,
    facing: Direction,
    /// The spawn cell and the anchor of the patrol territory (§7.5).
    station: Cell,
    /// The cells of this guard's region beat (§7.5/§10.5): the station's region
    /// grown across door edges at placement ([`crate::beat`]), so every cell is
    /// walkable from the station and no territory straddles a wall. The sweep
    /// filters it to the patrollable cells each pick, so later-stamped solids (a
    /// console) never become targets. Empty for a guard built without a graph —
    /// a hand-placed fixture — which falls back to the [`PATROL_RADIUS`] flood.
    beat: Vec<Cell>,
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
    /// The cell a search/watch centres on — where the lead ran out (§7.6). Set when a
    /// spent chase turns into a search; drives the [`Alerted`](GuardState::Alerted)
    /// sweep and, after release, the raised-coverage patrol. `None` when the guard has
    /// no area of heightened interest.
    focus: Option<Cell>,
    /// Turns of active [`Alerted`](GuardState::Alerted) search remaining (§7.6 fix 2).
    /// Set to [`SEARCH_DURATION`] when a lead is lost on arrival, cooled by one each
    /// turn in [`sense`](Self::sense); at zero the search releases to Calm patrol.
    search: u32,
    /// Turns of post-search raised coverage remaining (§7.6 Released). While positive,
    /// Calm patrol draws its territory around [`focus`](Self::focus) with the tighter
    /// [`WATCH_RADIUS`], so the just-searched region is watched harder before the sweep
    /// widens back to the station territory.
    watch: u32,
    /// Whether this guard's most recent [`sense`](Self::sense) detected the player —
    /// the §7.2 takedown gate ("the target has not detected you this turn"
    /// **[SETTLED]**). Distinct from [`state`](Self::state): a Chasing guard whose
    /// current look missed the player (concealed, or out of the cone) has *not*
    /// detected them this turn, and is takedown-able — awareness is per-turn fact,
    /// the state is the lingering mood.
    detected: bool,
    fov: VisibleSet,
    state: GuardState,
    /// This guard's radio ping cadence (§7.3): how often control pings it, drawn
    /// once from the run seed at placement ([`RadioClock`]). It has no effect
    /// while the guard is alive — a live guard always answers — and is handed to
    /// the [`Body`](crate::body::Body) at a takedown, where a *missed* ping
    /// finally becomes a dispatch and, on the second, an alert step. Fixtures get
    /// the un-jittered [`RadioClock::DEFAULT`].
    radio: RadioClock,
}

/// Patrol radius (§7.5, **[START] = 15**): how far the *fallback* territory
/// reaches — the patrollable cells within this many steps of the station. On a
/// generated level the Calm territory is the region beat instead
/// ([`crate::beat::BEAT_REGIONS`] replaces this box, §10.5); the radius flood
/// remains for guards built without a region graph — hand-placed fixtures.
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

/// How many turns a guard **searches** a lost lead before releasing to patrol
/// (§7.6 fix 2, **[START] = 8**). When a reactive guard reaches its last-known cell
/// and finds nothing it does not snap back to patrol (the old instant give-up);
/// it sweeps the area for this many turns first — the Lost → Hunted phase, "the good
/// part" where the hidden player watches cones pass. Bounded, so a guard never
/// searches forever; long enough that holding still in a cupboard is a real wait.
pub(crate) const SEARCH_DURATION: u32 = 8;

/// How far around the last-known cell a searching guard pokes (§7.6, **[START] = 4**):
/// the radius of the disc its search sweep paces across.
pub(crate) const SEARCH_RADIUS: u32 = 4;

/// After a search **releases**, the region is watched harder for this many turns
/// (§7.6 Released row, **[START] = 20**): the guard keeps patrolling — Calm again —
/// but biased onto the searched area (see [`WATCH_RADIUS`]) rather than its whole
/// station territory, so coverage there is briefly raised before the sweep drifts
/// back to normal.
pub(crate) const WATCH_DURATION: u32 = 20;

/// The radius of the post-search watch territory (§7.6, **[START] = 8**): tighter
/// than [`PATROL_RADIUS`], so a released guard concentrates its sweep on the area it
/// just searched instead of ranging its full patrol.
pub(crate) const WATCH_RADIUS: u32 = 8;

/// How hard finding a body hits a guard's alert (§7.2, **[START] = 60**): the
/// lead a found body grants, **stronger than a sighting** ([`ALERT_DURATION`] =
/// 30) — finding a body is the loudest event in the game. The facility-wide
/// escalation it feeds is the radio/cooperation tickets (§7.3/§7.7); what lands
/// here is the finder's own, harder reaction.
pub(crate) const BODY_ALERT_DURATION: u32 = 60;
// The §7.2 relation itself, held at compile time: finding a body must always
// out-alert a sighting, whatever either number is retuned to.
const _: () = assert!(BODY_ALERT_DURATION > ALERT_DURATION);

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
            beat: Vec::new(),
            patrols: false,
            inspected: VisibleSet::default(),
            destination: None,
            last_seen: None,
            alert: 0,
            focus: None,
            search: 0,
            watch: 0,
            detected: false,
            fov: VisibleSet::default(),
            state: GuardState::Calm,
            radio: RadioClock::DEFAULT,
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

    /// The same guard sweeping `beat` as its Calm territory (§7.5/§10.5) — the
    /// cells of the region beat placement grew from its station's region across
    /// door edges. This is how [`Placement::guards`](crate::Placement::guards)
    /// spawns every guard on a generated level; a guard given no beat falls back
    /// to the [`PATROL_RADIUS`] flood around its station.
    pub fn with_beat(mut self, beat: Vec<Cell>) -> Self {
        self.beat = beat;
        self
    }

    /// The same guard carrying radio cadence `clock` (§7.3): its personal ping
    /// period, drawn from the run seed by [`Placement::guards`](crate::Placement::guards)
    /// so the whole schedule is deterministic (§12.4). A guard built without one
    /// keeps [`RadioClock::DEFAULT`].
    pub(crate) fn with_radio_clock(mut self, clock: RadioClock) -> Self {
        self.radio = clock;
        self
    }

    /// This guard's radio cadence (§7.3) — read at a takedown to seed the
    /// [`Body`](crate::body::Body)'s ping schedule.
    pub(crate) fn radio_clock(&self) -> RadioClock {
        self.radio
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

    /// Whether this guard's most recent look detected the player — the §7.2
    /// takedown gate: a bump against a guard that **has** detected you this turn
    /// is a free no-op, one against a guard that has not is the takedown. Because
    /// of the always-seen touching ring (§6.1 **[SETTLED]**) an adjacent player is
    /// always in the cone, so this is `false` beside a guard only when something
    /// else intervened — concealment, a decoy, a distraction — which is exactly
    /// the puzzle §7.2 wants solved.
    pub fn detected_player(&self) -> bool {
        self.detected
    }

    /// The guard's station (§7.5) — recorded on its body at a takedown as the
    /// "last known post" the radio net will dispatch a responder to (§7.3).
    pub(crate) fn station(&self) -> Cell {
        self.station
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
        // Every reactive timer cools by default; a sighting below resets the lead to
        // full and clears the search/watch a fresh detection supersedes. Awareness
        // is per-turn (§7.2): each look starts undetected and must re-earn it.
        self.alert = self.alert.saturating_sub(1);
        self.search = self.search.saturating_sub(1);
        self.watch = self.watch.saturating_sub(1);
        self.detected = false;
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
            self.detected = true;
            self.end_search_and_watch();
        } else if range <= GLIMPSE_RANGE {
            self.state = GuardState::Investigating;
            self.destination = self.last_seen.or(Some(player));
            self.alert = ALERT_DURATION;
            self.detected = true;
            self.end_search_and_watch();
        }
    }

    /// React to seeing a decoy (§8.3, #105): Investigate toward it — the §7.4
    /// "decoy seen" entry, lower severity than a chase — with a fresh lead. The
    /// caller enforces §8.3's precedence and never calls this for a guard that
    /// detected the player this turn: a guard that can see *you* ignores the
    /// fake entirely. Decoys work on guards that have lost you, not on guards
    /// that have you.
    pub(crate) fn investigate_decoy(&mut self, at: Cell) {
        self.state = GuardState::Investigating;
        self.destination = Some(at);
        self.alert = ALERT_DURATION;
        self.end_search_and_watch();
    }

    /// React to a radio dispatch (§7.3): a colleague has stopped answering, so
    /// control sends this guard to the silent guard's last known `post`. It
    /// switches to [`Responding`](GuardState::Responding) (§7.4) and walks there
    /// with a fresh lead — the same [`ALERT_DURATION`] backstop every reactive
    /// state carries, so a responder that cannot reach the post gives up cleanly
    /// rather than pacing forever. Any lingering search/watch is superseded: the
    /// dispatch is the new priority. The caller only ever dispatches a guard that
    /// does not have the live player ([`nearest_respondable`](crate::radio::nearest_respondable)),
    /// so this never pulls a guard off a chase.
    pub(crate) fn respond_to(&mut self, post: Cell) {
        self.state = GuardState::Responding;
        self.destination = Some(post);
        self.alert = ALERT_DURATION;
        self.end_search_and_watch();
    }

    /// React to finding a body (§7.2) — the loudest event in the game. The lead
    /// it grants is **harder than a sighting** ([`BODY_ALERT_DURATION`] >
    /// [`ALERT_DURATION`]), and — unless the guard is busy with the live player,
    /// who always outranks the dead — it drops straight into the §7.6 search,
    /// centred on the body: the same bounded Alerted sweep a lost chase ends in
    /// (a body *is* a lead whose trail is already cold), followed by the released
    /// watch on the area. Walking a destination would be wrong here — the body
    /// is solid (§4.3), so a guard can never arrive on it; the search paces
    /// *around* it instead. The radio broadcast a body-find escalates into is
    /// the cooperation ticket (§7.7); this is the finder's own reaction.
    pub(crate) fn find_body(&mut self, at: Cell) {
        self.alert = self.alert.max(BODY_ALERT_DURATION);
        if self.detected {
            return; // the live player outranks the body
        }
        self.state = GuardState::Alerted;
        self.search = SEARCH_DURATION;
        self.focus = Some(at);
        self.destination = None;
    }

    /// A fresh detection supersedes any lingering search or raised-coverage watch
    /// (§7.6): the guard re-engages the live lead, so the old area of interest is
    /// dropped rather than pacing on underneath the new chase.
    fn end_search_and_watch(&mut self) {
        self.search = 0;
        self.watch = 0;
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

        // A reactive guard pursues its live lead while the alert is warm. The moment
        // it can no longer chase, what happens next is the §7.6 fix 2 arc:
        if matches!(self.state, GuardState::Chasing | GuardState::Investigating) {
            if self.alert > 0 {
                if let Some(step) = self.step_toward_destination(facility, blocked) {
                    return Some(step);
                }
                if self.destination == Some(self.pos) {
                    // Arrived at the last-known cell with nothing seen: **Lost → Hunted**.
                    // It searches the area rather than snapping back to patrol.
                    self.begin_search();
                } else {
                    // The route is only blocked this turn (a colleague, §7.8): keep the
                    // lead and hold, retrying next turn — do not give it up as lost.
                    return None;
                }
            } else {
                // The lead went cold before the guard ever reached it (§7.1): the
                // anti-tracking-turret backstop gives it up cleanly, no search.
                self.stand_down();
            }
        }

        // A **Responding** guard (§7.3/§7.4) walks to the silent guard's post. It
        // carries a lead like any reactive state: while it is warm it heads for
        // the post; on arrival with nothing there it stands down to patrol (the
        // body may have been dragged off, or already found by its own cone en
        // route — either way the post itself holds no live lead), and if the route
        // is only blocked this turn it holds and retries. A cold lead — it never
        // got there — gives up cleanly, the same anti-tracking backstop (§7.6).
        if self.state == GuardState::Responding {
            if self.alert > 0 {
                if let Some(step) = self.step_toward_destination(facility, blocked) {
                    return Some(step);
                }
                if self.destination == Some(self.pos) {
                    self.stand_down();
                } else {
                    return None;
                }
            } else {
                self.stand_down();
            }
        }

        // **Hunted**: sweep the focus area for a bounded number of turns, then release.
        if self.state == GuardState::Alerted {
            if self.search > 0 {
                if let Some(step) = self.step_search(facility, blocked) {
                    return Some(step);
                }
                // Nothing left to poke at in the area — end the search early.
            }
            self.release_from_search(); // **Released**
        }

        self.repick_patrol_target(facility);
        self.step_toward_destination(facility, blocked)
    }

    /// Begin the §7.6 search: enter [`Alerted`](GuardState::Alerted) for
    /// [`SEARCH_DURATION`] turns, centred on where the lead ran out — the last cell
    /// known for certain, or, for a glimpse-only lead, the guard's own cell. The old
    /// destination is cleared so [`step_search`](Self::step_search) picks sweep targets.
    fn begin_search(&mut self) {
        self.state = GuardState::Alerted;
        self.search = SEARCH_DURATION;
        self.focus = Some(self.last_seen.unwrap_or(self.pos));
        self.destination = None;
    }

    /// One step of the search sweep: pace toward the farthest patrollable cell within
    /// [`SEARCH_RADIUS`] of the [`focus`](Self::focus). On arrival the next-farthest is
    /// the far side, so the guard crosses and re-crosses the area, sweeping its cone
    /// over it (§7.6) — the sweep a hidden player waits out. `None` when the guard
    /// cannot move (a one-cell pocket, or a colleague blocking), which ends the search.
    fn step_search(&mut self, facility: &Facility, blocked: &[Cell]) -> Option<Direction> {
        let focus = self.focus?;
        let need_target = self
            .destination
            .is_none_or(|d| d == self.pos || !facility.can_enter(d, ACTOR_FILL));
        if need_target {
            let area = path::reachable_within(focus, SEARCH_RADIUS, |c| patrollable(facility, c));
            // Farthest from the guard's current cell (no inspected filter): a plain
            // paced sweep across the neighbourhood, deterministic (§12.4).
            self.destination = pick_farthest(&area, &VisibleSet::default(), self.pos);
        }
        self.step_toward_destination(facility, blocked)
    }

    /// Release from a search (§7.6 Released): drop to Calm patrol but keep the region
    /// under raised coverage for [`WATCH_DURATION`] turns — the sweep stays biased onto
    /// the [`focus`](Self::focus) area (see [`territory`](Self::territory)) before it
    /// widens back to the station. The live lead — destination, alert, last-known cell
    /// — is cleared; the focus survives to steer the watch.
    fn release_from_search(&mut self) {
        self.state = GuardState::Calm;
        self.watch = WATCH_DURATION;
        self.destination = None;
        self.last_seen = None;
        self.alert = 0;
    }

    /// The first step of the shortest [`routable`] path to the current destination
    /// that routes **around** the cells in `blocked` (colleagues, §7.8), or `None`
    /// when there is nothing to walk to — no destination, already stood on it, or no
    /// unobstructed route reaches it (the guard then holds and retries next turn).
    /// The route may run through a **closed door** ([`routable`]): the turn loop
    /// turns the step into the panel into the opening bump (§10.4). The destination
    /// itself is exempt from `blocked` — as it is from the predicate (a guard may be
    /// sent onto a cell it cannot end on) — so a lead pointing at a colleague's cell
    /// still draws the guard toward it rather than freezing the sweep.
    fn step_toward_destination(&self, facility: &Facility, blocked: &[Cell]) -> Option<Direction> {
        let destination = self.destination?;
        if destination == self.pos {
            return None;
        }
        path::first_step_toward(self.pos, destination, |cell| {
            routable(facility, cell) && !blocked.contains(&cell)
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

    /// The guard's patrol territory (§7.5): the patrollable cells of its region
    /// **beat** — rooms and the corridors joining them, grown from the station's
    /// region across door edges (§10.5, [`crate::beat`]) — so no territory
    /// straddles a wall into a space the guard cannot walk to, and corridors get
    /// real coverage instead of being crossed incidentally. A fixture guard built
    /// without a beat sweeps the [`PATROL_RADIUS`] flood around its station
    /// instead — bounded by walkability either way.
    fn territory(&self, facility: &Facility) -> Vec<Cell> {
        // While a released search still watches the region (§7.6), the sweep draws its
        // territory around the searched area with the tighter [`WATCH_RADIUS`], so
        // coverage there stays raised; otherwise it is the full Calm territory.
        if let Some(focus) = self.focus {
            if self.watch > 0 {
                return path::reachable_within(focus, WATCH_RADIUS, |cell| {
                    patrollable(facility, cell)
                });
            }
        }
        if !self.beat.is_empty() {
            // Filtered at sweep time, not at placement: a console stamped in
            // later, furniture, or a cupboard is never picked as a target.
            return self
                .beat
                .iter()
                .copied()
                .filter(|&cell| patrollable(facility, cell))
                .collect();
        }
        path::reachable_within(self.station, PATROL_RADIUS, |cell| {
            patrollable(facility, cell)
        })
    }
}

/// Whether a guard may **patrol through** `cell` (§7.5/§10.3): a cell it can both
/// stand on and route across. That is floor and open door panels — but *not*
/// furniture, cover or a cupboard (which patrols flow around, §10.1), and not a
/// closed door (a sweep never *targets* a doorway; walking through one is
/// [`routable`]'s job). It is deliberately stricter than [`Facility::can_enter`]:
/// a hideout admits a mover but a patrol routes around it, so the two predicates
/// must be combined.
fn patrollable(facility: &Facility, cell: Cell) -> bool {
    facility
        .terrain(cell)
        .is_some_and(|terrain| !terrain.blocks_pathing() && facility.can_enter(cell, ACTOR_FILL))
}

/// Whether a guard's walk may **route through** `cell` (§10.4): everything
/// [`patrollable`], plus a **closed door panel** — the §10.3 table's one
/// deliberate surprise (a closed panel does not block pathing): a guard heading
/// somewhere walks up to the door and opens it by bumping in, which is how guard
/// traffic monotonically opens the facility up over a level. Kept apart from
/// [`patrollable`] so a panel is walked *through*, never chosen as a sweep or
/// search target the guard could not stand on.
fn routable(facility: &Facility, cell: Cell) -> bool {
    patrollable(facility, cell) || facility.terrain(cell) == Some(Terrain::DoorPanelClosed)
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

    /// §7.5: a *fixture* guard — one built without a region beat — falls back to
    /// the patrollable cells within [`PATROL_RADIUS`] of the station. The radius is
    /// pinned here so a later change to the **[START] = 15** value is visible — a
    /// floor cell exactly at the radius is in, one step past is out. (On generated
    /// levels the beat replaces this box: see the tests below.)
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

    /// §7.5/§10.5: a guard carrying a region beat sweeps *it* — the radius box is
    /// gone: a beat cell far beyond [`PATROL_RADIUS`] is territory, a cell beside
    /// the station that is not on the beat is not, and unsweepable terrain
    /// (furniture) is filtered out at sweep time rather than baked in.
    #[test]
    fn a_beat_guard_sweeps_its_beat_not_the_radius_box() {
        let mut facility = Facility::walled_box(40, 5);
        facility.set_terrain(20, 1, Terrain::PartialCover);
        let station = Cell::new(1, 1);
        let far = Cell::new(35, 1);
        assert!(station.manhattan_distance(far) > PATROL_RADIUS);

        let beat = vec![station, Cell::new(2, 1), Cell::new(20, 1), far];
        let territory = Guard::patrolling(station)
            .with_beat(beat)
            .territory(&facility);
        assert!(
            territory.contains(&far),
            "the beat, not the radius, bounds it"
        );
        assert!(
            !territory.contains(&Cell::new(20, 1)),
            "furniture on the beat is not a sweep target",
        );
        assert!(
            !territory.contains(&Cell::new(3, 1)),
            "off-beat cells are not territory, however close to the station",
        );
    }

    /// §7.5/§10.5 on generated levels: every placed guard's Calm territory is its
    /// region beat — every cell of it walkable from the station (no territory
    /// straddles a wall into a space the guard cannot reach), and the corridors
    /// adjacent to its rooms are genuinely part of the beat, not ground crossed
    /// incidentally.
    #[test]
    fn placed_guard_territories_are_reachable_and_cover_corridors() {
        use crate::generate::generate_level;
        use crate::place::LevelConfig;
        use crate::region::RegionKind;
        use crate::rng::Rng;
        use crate::test_support::seed_sweep;
        use std::collections::HashSet;

        for seed in seed_sweep(32) {
            let (layout, placement) =
                generate_level(&LevelConfig::V1, &mut Rng::new(seed)).expect("v1 generates");
            let facility = layout.facility();
            for guard in placement.guards(&layout) {
                let territory = guard.territory(facility);
                assert!(!territory.is_empty(), "seed {seed}: an empty beat");

                let reached: HashSet<Cell> =
                    path::flood_from(guard.station(), facility.width(), facility.height(), |c| {
                        routable(facility, c)
                    })
                    .into_iter()
                    .collect();
                for &cell in &territory {
                    assert!(
                        reached.contains(&cell),
                        "seed {seed}: territory cell {cell:?} is not walkable from \
                         the station {:?}",
                        guard.station(),
                    );
                }

                assert!(
                    territory.iter().any(|&c| {
                        layout
                            .regions()
                            .region_at(c)
                            .is_some_and(|id| layout.regions().kind(id) == RegionKind::Corridor)
                    }),
                    "seed {seed}: a beat with no corridor coverage",
                );
            }
        }
    }

    /// §7.6: the post-search raised-coverage watch overrides the beat exactly as
    /// it overrode the old radius box — while the watch runs, the sweep draws the
    /// tighter [`WATCH_RADIUS`] disc around the focus, beat or no beat, and the
    /// beat returns once the watch cools.
    #[test]
    fn the_released_watch_overrides_the_beat() {
        let facility = Facility::walled_box(40, 40);
        let focus = Cell::new(30, 30);
        let mut guard =
            Guard::patrolling(Cell::new(5, 5)).with_beat(vec![Cell::new(5, 5), Cell::new(6, 5)]);
        guard.focus = Some(focus);
        guard.watch = 1;

        let watched = guard.territory(&facility);
        assert!(
            watched
                .iter()
                .all(|&c| focus.manhattan_distance(c) <= WATCH_RADIUS),
            "the watch disc, not the beat",
        );
        assert!(watched.contains(&focus));

        guard.watch = 0;
        assert_eq!(
            guard.territory(&facility),
            vec![Cell::new(5, 5), Cell::new(6, 5)],
            "the beat returns once the watch cools",
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

    /// §7.6 fix 2 (Lost → Hunted → Released): a reactive guard that reaches its
    /// last-known cell and finds nothing does **not** snap back to patrol — it enters a
    /// bounded [`Alerted`](GuardState::Alerted) search, sweeps for exactly
    /// [`SEARCH_DURATION`] turns, and only then releases to Calm. Driven by sight (§9
    /// **[SETTLED]**): a glimpse sends the guard Investigating, and once standing on the
    /// lead with nothing more seen the search begins.
    #[test]
    fn a_lost_lead_searches_then_releases_to_patrol() {
        let facility = Facility::walled_box(15, 15);
        let mut guard = Guard::patrolling(Cell::new(7, 2)); // faces south (§7.1)
        guard.look(&facility);
        let glimpse = Cell::new(7, 9); // down the cone: the glimpse zone
        assert!(guard.fov().contains(glimpse), "precondition: in the cone");

        guard.sense(glimpse, false);
        assert_eq!(guard.state(), GuardState::Investigating);

        // Arrive at the lead with nothing more seen: the search begins, not patrol.
        guard.advance_to(glimpse, Direction::South, &facility);
        guard.decide(&facility, &[]);
        assert_eq!(
            guard.state(),
            GuardState::Alerted,
            "arrival begins a bounded search, not an instant give-up",
        );

        // Wait the search out (player concealed nearby — nothing seen). It stays
        // Alerted for SEARCH_DURATION turns, then releases to Calm.
        let mut alerted_turns = 0u32;
        for _ in 0..SEARCH_DURATION + 2 {
            guard.sense(glimpse, true);
            if guard.state() == GuardState::Alerted {
                alerted_turns += 1;
            }
            guard.decide(&facility, &[]);
        }
        assert_eq!(
            alerted_turns, SEARCH_DURATION,
            "the search lasts exactly SEARCH_DURATION turns",
        );
        assert_eq!(
            guard.state(),
            GuardState::Calm,
            "the search releases back to patrol",
        );
    }

    /// §7.6 search **[START]** pins: the search duration and its radii, and the
    /// released-watch window, are named constants a later tune must move deliberately.
    #[test]
    fn the_search_constants_are_pinned() {
        assert_eq!(SEARCH_DURATION, 8, "the [START] search duration");
        assert_eq!(SEARCH_RADIUS, 4, "the [START] search radius");
        assert_eq!(WATCH_DURATION, 20, "the [START] released-watch window");
        assert_eq!(WATCH_RADIUS, 8, "the [START] watch radius");
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

    /// §7.2's takedown gate is **per-turn fact, not mood**: a guard whose latest
    /// look detected the player is aware; one whose latest look missed them —
    /// concealment here — is not, even while its Chasing state lingers. That gap
    /// is the puzzle: arrange to be adjacent while the *current* look misses.
    #[test]
    fn detection_is_per_turn_not_state() {
        let facility = Facility::walled_box(11, 11);
        let mut guard = Guard::stationary(Cell::new(5, 3)); // faces south (§7.1)
        guard.look(&facility);
        let player = Cell::new(5, 5);
        assert!(!guard.detected_player(), "nothing sensed yet");

        guard.sense(player, false);
        assert!(guard.detected_player());
        assert_eq!(guard.state(), GuardState::Chasing);

        guard.sense(player, true); // concealed: this turn's look misses
        assert!(!guard.detected_player(), "awareness is per-turn");
        assert_eq!(guard.state(), GuardState::Chasing, "the mood lingers");
    }

    /// §7.2: finding a body is the loudest event in the game — the lead it grants
    /// is pinned **stronger than a sighting's**, and the finder drops into the
    /// §7.6 search centred on the body (a lead whose trail is already cold).
    #[test]
    fn finding_a_body_out_alerts_a_sighting_and_begins_the_search() {
        assert_eq!(BODY_ALERT_DURATION, 60, "the [START] body-found alert");
        // (That it out-alerts a sighting is a compile-time assert by the const.)

        let facility = Facility::walled_box(15, 15);
        let mut guard = Guard::patrolling(Cell::new(7, 2));
        guard.look(&facility);
        let body = Cell::new(7, 5);
        guard.find_body(body);
        assert_eq!(guard.state(), GuardState::Alerted);
        assert_eq!(guard.alert, BODY_ALERT_DURATION);
        assert_eq!(guard.search, SEARCH_DURATION);
        assert_eq!(guard.focus, Some(body), "the search centres on the body");
    }

    /// §7.2: the live player outranks the dead — a guard that detected the player
    /// this turn keeps its chase when it also sees a body; only the harder alert
    /// sticks.
    #[test]
    fn a_detecting_guard_keeps_its_chase_over_a_found_body() {
        let facility = Facility::walled_box(15, 15);
        let mut guard = Guard::patrolling(Cell::new(7, 2));
        guard.look(&facility);
        let player = Cell::new(7, 5);
        guard.sense(player, false);
        assert!(guard.detected_player());

        guard.find_body(Cell::new(8, 5));
        assert_eq!(guard.state(), GuardState::Chasing, "the chase holds");
        assert_eq!(guard.destination, Some(player), "still after the live cell");
        assert_eq!(guard.alert, BODY_ALERT_DURATION, "the alert still hardens");
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
