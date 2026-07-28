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
use crate::rng::Rng;
use crate::state::ACTOR_FILL;
use crate::vision::{
    field_of_view_with_rear_blind_spot, VisibleSet, GUARD_SIGHT_ARC, GUARD_SIGHT_RANGE,
};

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

/// What a single **look** made of the player (§7.6) — the two detection zones, as
/// the guard's own per-turn reading rather than as the mood it left behind.
///
/// The distinction is load-bearing for the facility alert (§7.3): a **confirmed
/// sighting** is certain-zone contact, and a glimpse counts nothing toward it.
/// Keeping both in one field is what stops the ladder ever re-deriving "was that
/// certain?" from the guard's state, where a hideout flush or a body search could
/// have moved it on since (#199/#200 — one reading, not two that merely agree).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Contact {
    /// The player was inside [`CERTAIN_RANGE`]: the guard knows where they are.
    Certain,
    /// The player was past [`CERTAIN_RANGE`] but inside [`GLIMPSE_RANGE`]: the guard
    /// knows *something* is there, and heads for where it last knew them to be.
    Glimpse,
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
/// **through closed doors**, which it opens by walking in, §10.4).
///
/// The reactive §7.4 states all sit on that same seam: [`Chasing`](GuardState::Chasing)
/// and [`Investigating`](GuardState::Investigating) from the §7.6 two zones,
/// [`Responding`](GuardState::Responding) from a missed radio ping (§7.3), and
/// [`Alerted`](GuardState::Alerted) for a lead being walked down and searched. Each
/// sets `destination` its own way and reuses this same walk-toward-it movement, which
/// is why they add transitions rather than machinery. A reactive guard whose lead
/// runs out (§7.1's alert duration) searches its area and then stands back down to
/// patrol on its own.
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
    /// What this guard's most recent [`sense`](Self::sense) made of the player, or
    /// `None` for a look that missed them — the §7.2 takedown gate ("the target has
    /// not detected you this turn" **[SETTLED]**). Distinct from
    /// [`state`](Self::state): a Chasing guard whose current look missed the player
    /// (concealed, or out of the cone) has *not* detected them this turn, and is
    /// takedown-able — awareness is per-turn fact, the state is the lingering mood.
    ///
    /// It keeps the **zone** (§7.6) rather than a bare flag because the facility
    /// alert counts certain-zone turns and nothing else (§7.3): one field, so a
    /// glimpse can never be read as a confirmed sighting by a second, drifting
    /// reading of the same look.
    contact: Option<Contact>,
    fov: VisibleSet,
    state: GuardState,
    /// Turns of Calm patrol **dwell** remaining (§7.5 dwell, §153): a guard that has
    /// reached a patrol destination holds in place for a few turns — facing
    /// unchanged (§5) — before picking the next, a stationary window a
    /// Takedown (§7.2) can exploit. Set on arrival by the seeded roll
    /// ([`GUARD_DWELL_CHANCE_PERCENT`]); counted down each Calm turn; **cleared the
    /// instant the guard turns reactive**, so a chase or search never pauses (§7.1
    /// "guards never accelerate" cuts one way — a reactive guard never *slows*
    /// either). `0` means not dwelling.
    dwell: u32,
    /// This guard's radio ping cadence (§7.3): how often control pings it, drawn
    /// once from the run seed at placement ([`RadioClock`]). It has no effect
    /// while the guard is alive — a live guard always answers — and is handed to
    /// the [`Body`](crate::body::Body) at a takedown, where a *missed* ping
    /// finally becomes a dispatch and, on the second, an alert step. Fixtures get
    /// the un-jittered [`RadioClock::DEFAULT`].
    radio: RadioClock,
    /// The cupboard this guard **saw the player climb into** while alerted, and is
    /// now flushing (§15 Q5): the one cell a guard may enter an occupied hideout on.
    /// `None` for a guard that never witnessed a dive — which keeps a cupboard the
    /// safe room it is against everyone else (§10.3). Set by
    /// [`flush_hideout`](Self::flush_hideout) on the entry turn, cleared by
    /// [`forget_hideout`](Self::forget_hideout) when the player leaves the cell and by
    /// [`stand_down`](Self::stand_down) when the lead runs cold — so a witness is a
    /// short, live lead, never a permanent grudge.
    witnessed_hideout: Option<Cell>,
    /// Whether the current §7.6 search was triggered by a **found body** (§15 Q5, the
    /// found-a-body-nearby half). A body is loud evidence the intruder is close (§7.2),
    /// so a search that began on one *checks the cupboards in the area it sweeps*: an
    /// occupied hideout within [`SEARCH_RADIUS`] of the [`focus`](Self::focus) is
    /// flushed like a witnessed dive ([`checks_hideout_at`](Self::checks_hideout_at)/
    /// [`check_hideout`](Self::check_hideout)). Set only by [`find_body`](Self::find_body);
    /// a **lost-chase** search ([`begin_search`](Self::begin_search)) leaves it `false`,
    /// so a cupboard stays the safe wait-out it is against a guard that merely lost
    /// sight of you (§10.3). Cleared whenever the search ends or a fresher lead
    /// supersedes it.
    body_search: bool,
    /// Turns of **daze** left (§8.3/#325): how much longer this guard is blinded and
    /// frozen by a Confusion blast it was standing inside when the flash went off.
    ///
    /// The counter lives *here*, on the guard, not on the player — which is the whole
    /// shape of the fired model. The blast decides its set once, at the moment it is
    /// pressed ([`State::guard_confused`](crate::State::guard_confused)); after that,
    /// distance stops mattering. A dazed guard the player runs away from stays dazed
    /// for its full count, and a guard that walks into the cells the blast passed
    /// through was never in it and is untouched.
    ///
    /// While it runs the guard takes no part in phase 3 at all — it does not sense,
    /// does not witness a dive, finds no body, checks no cupboard, is not drawn by a
    /// decoy and does not move. The freeze is a **pause, not a reset** (§8.2/§8.3):
    /// nothing here touches state, lead or destination, so the guard resumes exactly
    /// where it was when the count runs out. Counted down once per spent turn by
    /// [`shake_off_daze`](Self::shake_off_daze) — *outside* [`sense`](Self::sense),
    /// which a dazed guard never reaches.
    dazed: u32,
}

/// Patrol radius (§7.5, **[START] = 15**): how far the *fallback* territory
/// reaches — the patrollable cells within this many steps of the station. On a
/// generated level the Calm territory is the region beat instead
/// ([`crate::beat::BEAT_REGIONS`] replaces this box, §10.5); the radius flood
/// remains for guards built without a region graph — hand-placed fixtures.
pub(crate) const PATROL_RADIUS: u32 = 15;

/// How long a detection lead survives with nothing sensed (§7.1 alert duration,
/// **[START] = 30**). A fresh sighting resets the alert timer to this; each quiet
/// turn drops it by one, and a reactive guard gives up its lead once it hits zero.
/// It paces the bounded §7.6 fix-2 search: a guard that loses its lead sweeps the
/// area ([`SEARCH_DURATION`]/[`SEARCH_RADIUS`]) and then watches
/// ([`WATCH_DURATION`]) before standing down. This is the anti-tracking-turret
/// backstop — no guard pursues a stale lead forever.
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
/// (§7.6 fix 2, **[START] = 12**). When a reactive guard reaches its last-known cell
/// and finds nothing it does not snap back to patrol (the old instant give-up);
/// it sweeps the area for this many turns first — the Lost → Hunted phase, "the good
/// part" where the hidden player watches cones pass. Bounded, so a guard never
/// searches forever; long enough that holding still in a cupboard is a real wait.
///
/// Tuned up from 8, where a sweep barely crossed the [`SEARCH_RADIUS`] disc once
/// before releasing: at 12 the guard re-crosses the area, which is what makes waiting
/// it out a real wait. A `--bot` sweep of 8/12/16/20 over 300 seeds, run across all
/// three playstyle profiles (#198), put the knee here — and it is worth knowing *why*
/// the answer is not simply "longer is harder":
///
/// - **baseline** feels it most: the hunt lengthens sharply (detections +26%, ~56%
///   more turns spent holding still) and the win rate falls 0.41 → 0.34, then keeps
///   falling to 0.28 at 16.
/// - **cautious** — which hides early and long — barely moves (0.52 → 0.54), and its
///   *stalled* runs halve (12 → 6 of 300): a guard that commits to sweeping the disc
///   and then leaves beats one that released early and re-found it on patrol.
/// - **aggressive** — which keeps moving — is flat throughout (0.34 → 0.36).
///
/// So this is not a flat difficulty dial; it lengthens the hunt for a player who
/// stops, which is exactly the §7.6 phase it exists to create. 12 is where no profile
/// is worse off: 16 costs baseline another 0.06 win rate for nothing elsewhere, and
/// 20 finally drags cautious down (0.46). Diversity is unmoved at every value.
pub(crate) const SEARCH_DURATION: u32 = 12;

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

/// How often a **Calm** guard closes a hinged door behind itself after passing
/// through it (§10.4/§7.6 close-behind, **[START] = 25%**), as a percentage the
/// seeded run RNG rolls against (§12.4). *Sometimes*, never always: §7.6 warns that
/// a guard which always tidied up behind itself would make the level too static and
/// erase the "guard traffic opens the facility up" pressure — so this is a small but
/// nonzero fraction, and it is the playtest knob (see `State::set_guard_close_chance`).
pub(crate) const GUARD_CLOSE_CHANCE_PERCENT: u32 = 25;

/// How hard finding a body hits a guard's alert (§7.2, **[START] = 60**): the
/// lead a found body grants, **stronger than a sighting** ([`ALERT_DURATION`] =
/// 30) — finding a body is the loudest event in the game. The facility-wide
/// escalation it feeds is the radio/cooperation tickets (§7.3/§7.7); what lands
/// here is the finder's own, harder reaction.
pub(crate) const BODY_ALERT_DURATION: u32 = 60;
// The §7.2 relation itself, held at compile time: finding a body must always
// out-alert a sighting, whatever either number is retuned to.
const _: () = assert!(BODY_ALERT_DURATION > ALERT_DURATION);

/// How often a **Calm** guard, on reaching a patrol destination, dwells in place
/// before picking the next one (§7.5 dwell, §153), as a percentage the seeded run
/// RNG rolls against (§12.4, **[START] = 100%** — *every* arrival).
///
/// It was 50%, and at that rate the pause was not the thing a player saw. Measured
/// over twelve seeded runs, **92% of every stationary spell a patrolling guard took
/// was one or two turns** — not a dwell at all, but [`commit_step`](Guard::commit_step)'s
/// slow 90° turn and the two-rotation 180° about-face. A guard reached the end of
/// its sweep, stood for two turns while it spun, and walked back the way it came;
/// the real 3–5 turn pause fired on under 8% of stops. So the coin flip is gone: an
/// arrival always pauses, which is what makes the pause a rhythm a player can read
/// and plan a Takedown (§7.2) against, rather than a thing that sometimes happens.
///
/// Still a playtest knob (see `State::set_guard_dwell_chance`), because dwelling
/// lowers patrol coverage on purpose (§7.6/§7.7) and unconditional dwelling lowers
/// it further. Both extremes draw no RNG and perturb no stream — `0` never dwells,
/// `100` always does — so the default costs one draw per arrival, for the length.
pub(crate) const GUARD_DWELL_CHANCE_PERCENT: u32 = 100;

/// The shortest and longest a Calm dwell lasts, in turns (§7.5 dwell, §153,
/// **[START] = 3–7**): once a guard decides to dwell, its length is drawn
/// uniformly from this inclusive range on the seeded run RNG (§12.4). Long enough
/// that the stop reads as a guard *pausing* rather than a guard turning round —
/// the two-turn about-face is what the shorter 3–5 window kept getting lost behind
/// — and varied enough that the window is never a count the player can bank on.
///
/// The **stop the player sees** is a little longer than this: a guard that turns to
/// leave spends a further turn rotating for a 90° heading, or two for a reversal
/// (§7.5 slow turn), so a 3–7 dwell reads as 3–9 turns of held ground. The dwell is
/// the part with the facing pinned, which is the part a Takedown needs (§7.2).
pub(crate) const GUARD_DWELL_TURNS_MIN: u32 = 3;
pub(crate) const GUARD_DWELL_TURNS_MAX: u32 = 7;
// A dwell is always at least one turn and the range is well-formed, whatever the
// [START] numbers are retuned to.
const _: () = assert!(GUARD_DWELL_TURNS_MIN >= 1 && GUARD_DWELL_TURNS_MIN <= GUARD_DWELL_TURNS_MAX);

/// The §7.5 dwell rule in force this turn: how often an arriving Calm guard pauses,
/// and how long it holds when it does.
///
/// It is handed to [`decide`](Guard::decide) rather than read from constants because
/// the length is **not** a constant any more: the facility alert shortens it from
/// rung 1 up (§7.3), so a guard's pause is a fact about the level's current state,
/// not about the guard. The chance stays the playtest knob it was
/// (`State::set_guard_dwell_chance`).
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
            contact: None,
            fov: VisibleSet::default(),
            state: GuardState::Calm,
            dwell: 0,
            radio: RadioClock::DEFAULT,
            witnessed_hideout: None,
            body_search: false,
            dazed: 0,
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

    /// The same guard stationed at `station` while standing where it was built —
    /// the seam for pinning that a takedown is reported **where the guard fell**
    /// and not at the post it was assigned to (§7.3). Only a walked patrol
    /// separates the two in a real run, so tests set them apart directly.
    #[cfg(test)]
    pub(crate) fn with_station(mut self, station: Cell) -> Self {
        self.station = station;
        self
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

    /// The guard's field of view — the ~90° forward wedge (§6.2/§7.1) with the rear
    /// blind spot carved out (§155), current as of the last time this guard stood
    /// still or moved. This is the set the danger overlay paints (§11.5) and the
    /// detection the guard AI will read: one truth, so the picture and the rules
    /// cannot disagree — and the overlay stops reding the three cells at a guard's
    /// back because they are no longer in it.
    pub fn fov(&self) -> &VisibleSet {
        &self.fov
    }

    /// The guard's §7.4 state — what its mind is doing. The renderer derives the
    /// `g` glyph's category from this every turn ([`GuardState::category`]), so
    /// the state machine is readable straight off the screen (§11.2).
    pub fn state(&self) -> GuardState {
        self.state
    }

    /// Whether this guard's most recent **look** detected the player — the
    /// per-turn awareness latch (§7.2), set in phase 2's sight pass and cleared at
    /// the top of the next [`sense`](Self::sense). It drives the [`Detected`] event
    /// transition (§7.6) and the decoy precedence (a guard that sees you ignores a
    /// decoy, §8.3) — both read it *within* phase 3, where it is current.
    ///
    /// It is **not** the takedown gate. A guard that steps adjacent in phase 3 has
    /// a refreshed cone but a stale latch, so the gate reads the cone live instead
    /// ([`guard_detects_now`](crate::State::guard_detects_now)); this latch would
    /// let such a guard be taken down from directly in front. The touching ring
    /// (§6.1 **[SETTLED]**) sees an adjacent player everywhere except the guard's
    /// own **rear blind spot** (§155), so a live look is `false` beside a guard
    /// only when you are directly behind it, or when something intervened in front
    /// — concealment, a decoy, a distraction.
    ///
    /// [`Detected`]: crate::Event::Detected
    pub fn detected_player(&self) -> bool {
        self.contact.is_some()
    }

    /// Whether this guard's most recent look had the player in the **certain** zone
    /// (§7.6) — the facility alert's confirmed-sighting input (§7.3). Strictly
    /// narrower than [`detected_player`](Self::detected_player): a glimpse is a
    /// detection, and it is not a sighting.
    pub(crate) fn certain_contact(&self) -> bool {
        self.contact == Some(Contact::Certain)
    }

    /// Whether this guard is **dazed** right now (§8.3/#325): caught by a Confusion
    /// blast and still counting it down. The one fact the freeze is read from — by the
    /// guard phase, which skips a dazed guard entirely, and by the renderer, which
    /// drops its cone and marks it held. Public through
    /// [`State::guard_confused`](crate::State::guard_confused), which is where every
    /// caller asks.
    pub(crate) fn is_dazed(&self) -> bool {
        self.dazed > 0
    }

    /// Catch this guard in a Confusion blast (§8.3/#325): daze it for `turns`, blind
    /// and frozen, from now.
    ///
    /// A fresh blast **replaces** the count rather than adding to it — a second flash
    /// over a guard that is already dazed buys the same N turns from the moment it
    /// fires, never a stacked 2N. (With a 45-turn cooldown that overlap cannot happen
    /// today with one Confusion in the loadout; it is stated here so it stays a rule
    /// and not an accident of the numbers.) Nothing else is touched: state, lead,
    /// destination and focus all survive, which is what makes the freeze a pause
    /// (§8.2).
    pub(crate) fn daze(&mut self, turns: u32) {
        self.dazed = turns;
    }

    /// Count off one turn of daze (§8.3/#325), on §8.2's convention: run once per
    /// **spent** turn, at end of turn with the ability clocks, so a guard dazed for N
    /// is frozen for N turns *including* the one the blast went off in — every phase
    /// of which already saw it frozen.
    ///
    /// Deliberately **not** folded into [`sense`](Self::sense) with the other cooling
    /// timers: a dazed guard never reaches the sense pass, so a count that ticked there
    /// would never tick at all.
    pub(crate) fn shake_off_daze(&mut self) {
        self.dazed = self.dazed.saturating_sub(1);
    }

    /// Whether this guard is holding a §7.5 patrol dwell this turn (§153) — Calm,
    /// stationary by choice, and lined up for a behind-the-back Takedown (§7.2).
    /// Test-only: the renderer needs no special glyph, a dwelling guard is a Calm
    /// guard that simply is not moving.
    #[cfg(test)]
    pub(crate) fn is_dwelling(&self) -> bool {
        self.dwell > 0
    }

    /// Whether this guard closes a door it has just walked through (§10.4/§7.6):
    /// **Calm only**. A guard that is chasing, investigating, searching or responding
    /// is hunting, not tidying — it never pauses to shut a door behind itself, so
    /// opening a door during a chase leaves a lasting sightline the player can read.
    /// The turn loop pairs this with the seeded [`GUARD_CLOSE_CHANCE_PERCENT`] roll.
    pub(crate) fn closes_doors(&self) -> bool {
        self.state == GuardState::Calm
    }

    /// The cell this guard is currently walking to, if any (§7.4) — the seam the
    /// loop-level tests read a dispatch's target through, since they sit outside
    /// this module and the field is private.
    #[cfg(test)]
    pub(crate) fn destination(&self) -> Option<Cell> {
        self.destination
    }

    /// The cell an active search or watch is centred on (§7.6) — where this guard
    /// believes the trail ran out. Read by the §7.7 call-in: the cell a lost
    /// sighting reports is the one the loser is itself about to sweep, so the two
    /// can never disagree.
    pub(crate) fn focus(&self) -> Option<Cell> {
        self.focus
    }

    /// The guard's station (§7.5) — the anchor its Calm beat is grown from. The
    /// radio net no longer reads it: a dispatch heads for where the guard *fell*
    /// (§7.3), not the post it was assigned to, so this is the beat's seam for the
    /// §7.5/§10.5 territory tests alone.
    #[cfg(test)]
    pub(crate) fn station(&self) -> Cell {
        self.station
    }

    /// Recompute this guard's cone from its current position and facing (§6.2/§7.1),
    /// with the **rear blind spot** carved out (§155): the three cells at the guard's
    /// back do not detect, so a takedown can be set up from directly behind. The
    /// sight phase calls this for every guard before any of them act, so the
    /// decisions below read a cone that matches where the guard actually stands.
    pub(crate) fn look(&mut self, facility: &Facility) {
        self.fov = field_of_view_with_rear_blind_spot(
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
        self.contact = None;
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
            self.contact = Some(Contact::Certain);
            self.end_search_and_watch();
        } else if range <= GLIMPSE_RANGE {
            self.state = GuardState::Investigating;
            self.destination = self.last_seen.or(Some(player));
            self.alert = ALERT_DURATION;
            self.contact = Some(Contact::Glimpse);
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

    /// React to a call (§7.3/§7.7): control, or a colleague, wants this guard at
    /// `at` — a takedown site whose guard stopped answering the radio, or the cell
    /// a §7.7 call named. It switches to [`Responding`](GuardState::Responding)
    /// (§7.4) and walks there with a fresh lead — the same [`ALERT_DURATION`]
    /// backstop every reactive state carries, so a responder that cannot reach the
    /// cell gives up cleanly rather than pacing forever. Any lingering search/watch
    /// is superseded: the call is the new priority. The caller only ever sends a
    /// guard that does not have the live player
    /// ([`nearest_respondable`](crate::radio::nearest_respondable)), so this never
    /// pulls a guard off a chase.
    ///
    /// The **stale sighting is dropped** ([`last_seen`](Self::last_seen)): a guard
    /// answering a call goes to the cell it was *sent* to, and the search it opens
    /// on arrival ([`decide`](Self::decide)) centres there rather than on wherever
    /// this guard last happened to glimpse the player. A call carries its own cell;
    /// it does not inherit anybody's memory (§7.7 — no shared field of view).
    pub(crate) fn respond_to(&mut self, at: Cell) {
        self.state = GuardState::Responding;
        self.destination = Some(at);
        self.alert = ALERT_DURATION;
        self.last_seen = None;
        self.end_search_and_watch();
    }

    /// React to finding a body (§7.2) — the loudest event in the game. The lead
    /// it grants is **harder than a sighting** ([`BODY_ALERT_DURATION`] >
    /// [`ALERT_DURATION`]), and — unless the guard is busy with the live player,
    /// who always outranks the dead — it drops straight into the §7.6 search,
    /// centred on the body: the same bounded Alerted sweep a lost chase ends in
    /// (a body *is* a lead whose trail is already cold), followed by the released
    /// watch on the area. The body itself is non-solid (§7.2), so the guard can
    /// route and step straight over it — it never blocks the sweep, which is what
    /// keeps a body in a chokepoint from freezing an investigation (#182). The
    /// radio broadcast a body-find escalates into is the cooperation ticket (§7.7);
    /// this is the finder's own reaction.
    pub(crate) fn find_body(&mut self, at: Cell) {
        self.alert = self.alert.max(BODY_ALERT_DURATION);
        if self.detected_player() {
            return; // the live player outranks the body
        }
        self.state = GuardState::Alerted;
        self.search = SEARCH_DURATION;
        self.focus = Some(at);
        self.destination = None;
        // This is a *body* search (§15 Q5): while it runs, an occupied cupboard within
        // the swept area is checked — the found-a-body-nearby trigger the witness half
        // (#185) left open. A lost-chase search never sets this, so hiding still works
        // against a guard that only lost sight of you.
        self.body_search = true;
    }

    /// Act on **witnessing the player climb into a hideout** (§15 Q5) — the one case
    /// a cupboard does not refuse contact (§10.3/§4.5). The guard saw the dive while
    /// alerted, so it re-engages the *cupboard itself* as a live lead: it Chases the
    /// alcove with a fresh [`ALERT_DURATION`] window and walks to the mouth to flush
    /// the hidden player, capturing on the step in ([`State::guard_phase`]). The cell
    /// is recorded ([`witnessed_hideout`](Self::witnessed_hideout)) so **only this
    /// guard** may enter it — a patrol that never saw the entry still routes around
    /// the occupied cupboard (§10.3). The lead is the ordinary reactive one: if it
    /// runs cold before the guard reaches the mouth, [`stand_down`](Self::stand_down)
    /// gives it up and the player has waited it out (§7.1/§7.6).
    pub(crate) fn flush_hideout(&mut self, cell: Cell) {
        self.alert = ALERT_DURATION;
        self.engage_hideout(cell);
    }

    /// Act on a **body-triggered search reaching an occupied hideout** (§15 Q5, the
    /// found-a-body-nearby half). The loud found body (§7.2) says the intruder is
    /// close, so the §7.6 search checks the cupboards inside the area it is already
    /// sweeping ([`checks_hideout_at`](Self::checks_hideout_at)); on an occupied one it
    /// flushes it — a **second** way to earn entry to a hideout, alongside the witness
    /// (#185), not a new capture path. The hard found-body lead is kept
    /// (`alert.max(ALERT_DURATION)` never lowers the [`BODY_ALERT_DURATION`]
    /// [`find_body`](Self::find_body) granted); only the destination and the
    /// [`witnessed_hideout`](Self::witnessed_hideout) capture-gate flag are pointed at
    /// the cupboard, through the shared [`engage_hideout`](Self::engage_hideout) seam.
    pub(crate) fn check_hideout(&mut self, cell: Cell) {
        self.alert = self.alert.max(ALERT_DURATION);
        self.engage_hideout(cell);
    }

    /// Re-engage an occupied cupboard as a live lead — the shared core of the two ways
    /// a guard earns entry to a hideout (§15 Q5): it Chases the alcove itself, walks to
    /// the mouth, and captures the hidden player on the step in ([`State::guard_phase`]).
    /// The cell is recorded ([`witnessed_hideout`](Self::witnessed_hideout)) so **only
    /// this guard** may enter it — every other still routes around the occupied cupboard
    /// (§10.3). Any lingering search/watch is dropped; the caller sets the lead the
    /// engagement carries (a witness gets a fresh [`ALERT_DURATION`], a body-searcher
    /// keeps its harder one).
    fn engage_hideout(&mut self, cell: Cell) {
        self.state = GuardState::Chasing;
        self.destination = Some(cell);
        self.last_seen = Some(cell);
        self.witnessed_hideout = Some(cell);
        self.end_search_and_watch();
    }

    /// Whether this guard's active search (§15 Q5) covers an occupied hideout at
    /// `cell`: true while it is sweeping the disc — `cell` within [`SEARCH_RADIUS`]
    /// of its [`focus`](Self::focus), the area it is already pacing (§7.6, the §6.1
    /// sight metric) — *and* it has earned the check.
    ///
    /// Baseline, only a **body search** earns it ([`body_search`](Self::body_search),
    /// set by [`find_body`](Self::find_body)): a body found nearby is loud evidence
    /// the intruder is close, while a **lost-chase** search never checks, so a
    /// cupboard stays the safe wait-out it is against a guard that only lost sight of
    /// you (§10.3). The `guards_always_search_hideouts` level modifier (§12.6) passes
    /// `always_search`, which earns the check for *any* active search — including a
    /// lost chase — turning that wait-out off as a harder setting. The `Alerted`
    /// guard keeps `focus` after it releases to a watch (see
    /// [`release_from_search`](Self::release_from_search)), so the state gate is what
    /// stops a Calm watcher with a stale focus from flushing.
    ///
    /// The turn loop reads this each turn and calls
    /// [`check_hideout`](Self::check_hideout) when it holds.
    pub(crate) fn checks_hideout_at(&self, cell: Cell, always_search: bool) -> bool {
        let earns_check =
            self.body_search || (always_search && matches!(self.state, GuardState::Alerted));
        earns_check
            && self
                .focus
                .is_some_and(|focus| focus.sight_distance(cell) <= SEARCH_RADIUS)
    }

    /// Drop a witnessed-hideout lead because the player is **no longer in that cell**
    /// (§15 Q5): they climbed out, or slipped to another cupboard. Called by the turn
    /// loop when the stored cell stops matching the player's live hidden cell. If the
    /// guard was still marching on the now-empty alcove (its destination is that
    /// cell), it switches to a §7.6 **search** of the spot rather than stepping into
    /// the vacated cupboard — the natural "they were just here" sweep. A guard that
    /// has meanwhile re-acquired the player by sight keeps that fresher lead untouched.
    pub(crate) fn forget_hideout(&mut self) {
        let was_flushing = self.destination == self.witnessed_hideout;
        self.witnessed_hideout = None;
        if was_flushing {
            self.begin_search();
        }
    }

    /// The cupboard this guard is flushing after witnessing the player climb in
    /// (§15 Q5), or `None`. The turn loop reads it as the capture gate's one
    /// exception: a hidden player in *this* cell is caught by *this* guard.
    pub(crate) fn witnessed_hideout(&self) -> Option<Cell> {
        self.witnessed_hideout
    }

    /// A fresh detection supersedes any lingering search or raised-coverage watch
    /// (§7.6): the guard re-engages the live lead, so the old area of interest is
    /// dropped rather than pacing on underneath the new chase.
    fn end_search_and_watch(&mut self) {
        self.search = 0;
        self.watch = 0;
        self.body_search = false;
    }

    /// The direction the guard will try this turn, or `None` to hold (§7.4 phase 3).
    ///
    /// The guard first folds this turn's cone into its inspected-cell memory — it has
    /// *looked at* everything it can see. Then a **reactive** guard (Chasing or
    /// Investigating, §7.6) walks the destination its transition set; the moment it
    /// can no longer make progress — it has arrived, or the lead led somewhere it
    /// cannot route to — it enters the **§7.6 fix-2 arc** rather than snapping back to
    /// patrol: *Lost → Hunted → Released*. Arriving at the last-known cell with nothing
    /// seen begins a bounded [`begin_search`] of the surrounding disc
    /// ([`SEARCH_DURATION`]/[`SEARCH_RADIUS`]); when that runs dry it stands and
    /// **watches** ([`WATCH_DURATION`]/[`WATCH_RADIUS`]) before finally releasing to
    /// its patrol. A lead that simply cools to zero ([`ALERT_DURATION`]) releases the
    /// same way — the anti-tracking-turret backstop.
    /// A **Calm** guard picks its next patrol target and steps toward it (§7.5) —
    /// except that, on reaching a target, it **dwells** in place for a few turns
    /// first (§153, the seeded [`roll_dwell`]), a stationary window a Takedown
    /// (§7.2) can be lined up against. A held-in-place guard, or a Calm one with
    /// nowhere to go, holds. A Calm guard that must **change heading** by 90° first
    /// spends a turn rotating in place before it steps the new way, and no guard —
    /// Calm or reactive — flips 180° in one move: both are [`commit_step`]'s job,
    /// applied to whichever step the branches above chose. `rng` and [`dwell`](Dwell)
    /// drive the dwell roll and are the loop's only stochastic input here (§12.4); a
    /// `0` chance draws nothing. The dwell *length* comes in with the rule rather than
    /// from a constant, because the facility alert shortens it (§7.3/§7.5).
    /// `blocked` are the cells other guards currently stand on: guards are solid to
    /// each other and must **path around** a colleague, not through one (§7.8). A
    /// route the pass finds steps only into cells no other guard holds, so a guard
    /// whose direct line is blocked reroutes down the parallel lane (corridors are
    /// 2–4 wide, §10.1) instead of stalling. When a colleague genuinely seals the
    /// only route this turn, the guard holds and retries next turn as the colleague
    /// clears — a local wait-and-retry, no reservation system (§12.3), and no
    /// deadlock the old path-through-each-other stall produced.
    pub(crate) fn decide(
        &mut self,
        facility: &Facility,
        blocked: &[Cell],
        rng: &mut Rng,
        dwell: Dwell,
    ) -> Option<Direction> {
        if !self.patrols {
            return None;
        }
        self.inspected.absorb(&self.fov);

        // A dwell belongs to Calm patrol alone (§7.5/§153): the instant a guard is
        // reactive — chasing, investigating, searching, responding — any pause is
        // dropped, so a hunt never slows. Clearing it here, before the reactive
        // branches below, is what makes a detection mid-dwell preempt it at once.
        if self.state != GuardState::Calm {
            self.dwell = 0;
        }

        // A reactive guard pursues its live lead while the alert is warm. The moment
        // it can no longer chase, what happens next is the §7.6 fix 2 arc:
        if matches!(self.state, GuardState::Chasing | GuardState::Investigating) {
            if self.alert > 0 {
                if let Some(step) = self.step_toward_destination(facility, blocked) {
                    return self.commit_step(Some(step), facility);
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

        // A **Responding** guard (§7.3/§7.4/§7.7) walks to the cell it was called
        // to. It carries a lead like any reactive state: while it is warm it heads
        // there; on arrival it **searches** the area — the same §7.6 sweep a lost
        // chase ends in — because a call is a lead whose trail is already cold, and
        // a responder that merely stood on the cell would make every call a walk
        // rather than a hunt. If the route is only blocked this turn it holds and
        // retries. A cold lead — it never got there — gives up cleanly, the same
        // anti-tracking backstop (§7.6).
        if self.state == GuardState::Responding {
            if self.alert > 0 {
                if let Some(step) = self.step_toward_destination(facility, blocked) {
                    return self.commit_step(Some(step), facility);
                }
                if self.destination == Some(self.pos) {
                    // `respond_to` cleared any stale sighting, so this centres on
                    // the called cell the guard is now standing on.
                    self.begin_search();
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
                    return self.commit_step(Some(step), facility);
                }
                // Nothing left to poke at in the area — end the search early.
            }
            self.release_from_search(); // **Released**
        }

        // Calm patrol (§7.5). A dwell in progress holds the guard in place — facing
        // unchanged (§5) — a stationary window a Takedown can exploit (§7.2/§153).
        // It counts down; the turn it reaches zero the sweep resumes at once.
        if self.dwell > 0 {
            self.dwell -= 1;
            if self.dwell > 0 {
                return None;
            }
            // The dwell just ended — fall through to repick and step this turn.
        } else if self.destination == Some(self.pos) {
            // *Arrived* at a sweep target (as opposed to never having had one — a
            // fresh guard picks and walks without pausing, and this keeps the §4.2
            // startup turn drawing no RNG, as `State::new` relies on): a new target
            // is about to be picked, but a Calm guard dwells first (§7.5/§153) —
            // the pause comes **before** the repick, so the guard is never seen to
            // arrive and about-face in the same breath.
            if let Some(turns) = roll_dwell(rng, dwell) {
                self.dwell = turns;
                return None;
            }
        }

        self.repick_patrol_target(facility);
        let step = self.step_toward_destination(facility, blocked);
        self.commit_step(step, facility)
    }

    /// Reconcile a desired step against the guard's facing before it commits (§7.5
    /// slow turn, §7.2 no half-turn). `step` is the direction the patrol or reactive
    /// logic wants to walk; the return is what the guard actually does this turn —
    /// `Some(dir)` to step (the turn loop moves it and re-aims, §5), or `None` when
    /// the whole turn is spent rotating in place. Facing is quantised to the four
    /// cardinals (§4.3 **[SETTLED]**), so the desired step is one of three cases
    /// relative to the current facing:
    ///
    /// - **straight ahead** (same direction): every state steps, no re-aim (§7.1 — a
    ///   guard continuing forward pays nothing).
    /// - **a 180° reversal** (opposite): forbidden in one move for *every* state
    ///   (§7.2) — a guard cannot spin to face, and detect, a player lined up directly
    ///   behind it. It rotates one quarter **clockwise** in place — the fixed,
    ///   deterministic intermediate (§12.4) — and steps once aligned on a later turn,
    ///   so facing about takes ≥2 turns and always passes through a quarter.
    /// - **a 90° turn** (perpendicular): a **Calm** guard turns in place first (§7.5 —
    ///   the stationary quarter-turn that telegraphs a patrol's next heading, a second
    ///   window alongside the dwell); a **reactive** guard turns fast, re-aiming as it
    ///   steps exactly as before (§7.1 — a hunt is never slowed).
    ///
    /// A turn in place re-aims the cone at the new facing at once ([`turn_in_place`]),
    /// so the danger overlay a frame paints is honest about where a mid-turn guard is
    /// now looking (§11.5). There is no "mid-rotation" flag: the choice is recomputed
    /// from [`state`](Self::state) every turn, so the instant a Calm guard turns
    /// reactive its slow-turn tax is simply gone (§7.5, like the dwell) — a detection
    /// never waits on a pending rotation.
    fn commit_step(&mut self, step: Option<Direction>, facility: &Facility) -> Option<Direction> {
        let dir = step?;
        if dir == self.facing {
            return Some(dir);
        }
        if dir == self.facing.opposite() {
            self.turn_in_place(self.facing.clockwise(), facility);
            return None;
        }
        if self.state == GuardState::Calm {
            self.turn_in_place(dir, facility);
            None
        } else {
            Some(dir)
        }
    }

    /// Rotate in place to `facing` without stepping (§7.5/§7.2): position unchanged,
    /// the cone re-aimed at once so the overlay stays honest (§11.5). The guard has
    /// spent its whole turn on the rotation.
    fn turn_in_place(&mut self, facing: Direction, facility: &Facility) {
        self.facing = facing;
        self.look(facility);
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
        // A lost-chase search does not check hideouts (§15 Q5): only a found body earns
        // that. Clearing here keeps a search that *re-began* after a witnessed/body
        // flush fizzled (the player slipped out) from silently inheriting the check.
        self.body_search = false;
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
        self.body_search = false;
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
        // A cold lead includes a cupboard the guard saw the player dive into (§15 Q5):
        // giving up the chase means giving up the flush, so the player waited it out.
        self.witnessed_hideout = None;
        self.body_search = false;
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

/// Roll the seeded run RNG (§12.4) for a Calm patrol dwell (§7.5/§153): `Some(n)`
/// to dwell for `n` turns, `None` to walk on. `chance` is the percentage a dwell
/// starts at all — [`GUARD_DWELL_CHANCE_PERCENT`] holds it at 100, so the shipped
/// game always pauses and only the length is drawn. Mirrors the close-behind
/// discipline
/// ([`State::rolls_a_close`](crate::State)): the extremes draw nothing against the
/// *chance* — a `0` never dwells and perturbs no stream, a `100` always does — so
/// only the tuned middle spends a chance draw; a dwell that *does* start then
/// spends one more draw for its length in [`GUARD_DWELL_TURNS_MIN`]..=[`GUARD_DWELL_TURNS_MAX`].
fn roll_dwell(rng: &mut Rng, dwell: Dwell) -> Option<u32> {
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
    use crate::vision::{field_of_view, FULL_SIGHT_ARC};

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
            FULL_SIGHT_ARC,
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

    /// §7.5/§153: a Calm guard that reaches a patrol target dwells in place for a
    /// bounded window — holding, position and facing unchanged (§5) — then resumes
    /// the sweep. Forced on (`dwell_chance` 100) with a fixed seed so the length is
    /// deterministic; it must land inside the [START] range and the guard must move
    /// again once it elapses.
    #[test]
    fn a_calm_guard_dwells_on_arrival_then_resumes() {
        let facility = Facility::walled_box(9, 9);
        // Already standing on its patrol target (destination == pos): the fixture
        // for "just arrived", the moment a dwell is rolled. (A fresh guard with no
        // target picks one and walks without pausing.)
        let mut guard = Guard::patrolling_to(Cell::new(4, 4), Cell::new(4, 4));
        guard.look(&facility);
        let (start, facing) = (guard.pos(), guard.facing());
        let mut rng = Rng::new(7);

        // On arrival, with the chance forced to 100, it begins a dwell rather than
        // immediately picking the next target.
        let first = guard.decide(&facility, &[], &mut rng, Dwell::CALM);
        assert!(
            first.is_none() && guard.is_dwelling(),
            "reaching a target begins a dwell",
        );

        // Hold for the rest of the window: each Calm turn holds, unmoved and
        // un-re-aimed, until the dwell elapses and the guard steps off.
        let mut holds = 1;
        loop {
            let step = guard.decide(&facility, &[], &mut rng, Dwell::CALM);
            if !guard.is_dwelling() {
                // The dwell has ended and the sweep resumes: the guard is active
                // again. It may first spend a turn or two rotating toward its new
                // heading (§229) before it steps, but a step lands within a couple of
                // turns — never a permanent hold.
                let mut resumed = step.is_some();
                for _ in 0..3 {
                    if resumed {
                        break;
                    }
                    resumed = guard
                        .decide(&facility, &[], &mut rng, Dwell::CALM)
                        .is_some();
                }
                assert!(resumed, "the sweep resumes once the dwell ends");
                break;
            }
            assert_eq!(step, None, "a dwelling guard holds");
            assert_eq!(guard.pos(), start, "a dwell does not move");
            assert_eq!(guard.facing(), facing, "a dwell does not re-aim (§5)");
            holds += 1;
        }
        assert_eq!(
            (GUARD_DWELL_TURNS_MIN, GUARD_DWELL_TURNS_MAX),
            (3, 7),
            "the [START] dwell range",
        );
        assert!(
            (GUARD_DWELL_TURNS_MIN..=GUARD_DWELL_TURNS_MAX).contains(&holds),
            "the dwell lasted {holds} turns, outside the [START] {GUARD_DWELL_TURNS_MIN}..={GUARD_DWELL_TURNS_MAX} range",
        );
    }

    /// The pause a player actually *sees* (§7.5/§153), at the shipped
    /// [`GUARD_DWELL_CHANCE_PERCENT`] rather than a forced 100: **every** Calm
    /// arrival pauses, and the pause is the whole [START] window — never the two
    /// turns a 180° about-face costs on its own.
    ///
    /// The measured symptom this pins against. Over twelve seeded runs of the real
    /// generator, **92% of every stationary spell a patrolling guard took was one or
    /// two turns** — [`commit_step`](Guard::commit_step)'s slow 90° turn and the
    /// two-rotation reversal — and 42% of the two-turn ones were immediately
    /// followed by the guard walking back the way it came. "Reach the end of the
    /// corridor, spin, come straight back" was the patrol's whole visible rhythm,
    /// and the 3–5 turn dwell, firing on under 8% of stops, was lost inside it.
    /// Sweeping the seed here is the point: a single lucky draw proves nothing about
    /// a behaviour that used to be a coin flip.
    #[test]
    fn every_calm_arrival_pauses_for_the_whole_dwell_window() {
        let facility = Facility::walled_box(9, 9);
        for seed in 0..64u64 {
            // Standing on its own target: the "just arrived" fixture.
            let mut guard = Guard::patrolling_to(Cell::new(4, 4), Cell::new(4, 4));
            guard.look(&facility);
            let (start, facing) = (guard.pos(), guard.facing());
            let mut rng = Rng::new(seed);
            let chance = GUARD_DWELL_CHANCE_PERCENT;

            let first = guard.decide(&facility, &[], &mut rng, Dwell::with_chance(chance));
            assert!(
                first.is_none() && guard.is_dwelling(),
                "seed {seed}: an arrival must pause, not pick the next target and walk",
            );

            let mut holds = 1;
            loop {
                let step = guard.decide(&facility, &[], &mut rng, Dwell::with_chance(chance));
                if !guard.is_dwelling() {
                    break; // the window elapsed and the sweep resumed this turn
                }
                assert_eq!(step, None, "seed {seed}: a dwelling guard holds");
                assert_eq!(guard.pos(), start, "seed {seed}: a dwell does not move");
                assert_eq!(guard.facing(), facing, "seed {seed}: no re-aim (§5)");
                holds += 1;
            }
            assert!(
                (GUARD_DWELL_TURNS_MIN..=GUARD_DWELL_TURNS_MAX).contains(&holds),
                "seed {seed}: held {holds} turns, outside the [START] \
                 {GUARD_DWELL_TURNS_MIN}..={GUARD_DWELL_TURNS_MAX} range",
            );
        }
    }

    /// §153: a detection cancels an in-progress dwell the same turn — a reactive
    /// guard never pauses (§7.1). The guard is dwelling; a sighting flips it to
    /// Chasing via `sense`, and the next `decide` chases instead of holding the
    /// dwell out.
    #[test]
    fn a_detection_cancels_an_in_progress_dwell() {
        let facility = Facility::walled_box(15, 15);
        // Arrived at its target (faces south, §7.1), so the first decide dwells.
        let mut guard = Guard::patrolling_to(Cell::new(7, 2), Cell::new(7, 2));
        guard.look(&facility);
        let mut rng = Rng::new(3);

        guard.decide(&facility, &[], &mut rng, Dwell::CALM);
        assert!(guard.is_dwelling(), "precondition: dwelling");

        // A player appears down the cone (certain zone): the guard turns reactive.
        let player = Cell::new(7, 5);
        assert!(guard.fov().contains(player), "precondition: in the cone");
        guard.sense(player, false);
        assert_eq!(guard.state(), GuardState::Chasing);

        // The next decision clears the dwell and steps toward the player.
        let step = guard.decide(&facility, &[], &mut rng, Dwell::CALM);
        assert!(!guard.is_dwelling(), "going reactive cancels the dwell");
        assert!(step.is_some(), "a chasing guard moves, it does not dwell");
    }

    /// §7.5 slow turn (#229): a **Calm** guard that must change heading by 90° spends
    /// a turn rotating in place — position unchanged, the cone re-aimed the new way —
    /// before it steps, and continuing straight pays nothing. The first decide turns
    /// (returns no step, facing swung east, still on its cell, its cone now honestly
    /// facing east); the second, now aligned, steps.
    #[test]
    fn a_calm_guard_spends_a_turn_rotating_before_a_quarter_turn() {
        let facility = Facility::walled_box(12, 12);
        // Faces south (§7.1); its patrol target is due east, a 90° turn away.
        let mut guard = Guard::patrolling_to(Cell::new(5, 5), Cell::new(8, 5));
        guard.look(&facility);
        assert!(
            guard.fov().contains(Cell::new(5, 7)) && !guard.fov().contains(Cell::new(7, 5)),
            "precondition: the cone starts facing south",
        );

        // Turn one: rotate in place. No step, unmoved, but the cone now faces east —
        // the overlay a frame paints is honest about the mid-turn facing (§11.5).
        let first = guard.decide(&facility, &[], &mut Rng::new(0), Dwell::NEVER);
        assert_eq!(first, None, "the quarter-turn spends the whole turn");
        assert_eq!(
            guard.pos(),
            Cell::new(5, 5),
            "a turn in place does not move"
        );
        assert_eq!(guard.facing(), Direction::East, "it swung a quarter east");
        assert!(
            guard.fov().contains(Cell::new(7, 5)) && !guard.fov().contains(Cell::new(5, 7)),
            "the cone re-aimed east at once",
        );

        // Turn two: now aligned, it steps — straight ahead costs nothing.
        let second = guard.decide(&facility, &[], &mut Rng::new(0), Dwell::NEVER);
        assert_eq!(
            second,
            Some(Direction::East),
            "aligned, it walks the new way"
        );
    }

    /// §7.1/§7.5 (#229): a **Calm** guard continuing straight ahead pays no turn tax —
    /// its first decision on a target dead ahead is the step itself, no rotation.
    #[test]
    fn a_calm_guard_walking_straight_pays_no_turn_tax() {
        let facility = Facility::walled_box(12, 12);
        // Faces south (§7.1); the target is due south — straight ahead.
        let mut guard = Guard::patrolling_to(Cell::new(5, 5), Cell::new(5, 9));
        guard.look(&facility);
        assert_eq!(
            guard.decide(&facility, &[], &mut Rng::new(0), Dwell::NEVER),
            Some(Direction::South),
            "a guard already facing its heading steps at once",
        );
    }

    /// §7.1/§7.6 (#229): a **reactive** guard turns fast — no turn tax. A Responding
    /// guard (any reactive state) heading 90° off its facing steps immediately,
    /// re-aiming as it goes, where a Calm guard on the same line would first rotate in
    /// place. A hunt is never slowed.
    #[test]
    fn a_reactive_guard_turns_without_a_tax() {
        let facility = Facility::walled_box(12, 12);
        let post = Cell::new(8, 5); // due east, a 90° turn from the south spawn facing

        // Calm on this line first rotates in place (no step).
        let mut calm = Guard::patrolling_to(Cell::new(5, 5), post);
        calm.look(&facility);
        assert_eq!(
            calm.decide(&facility, &[], &mut Rng::new(0), Dwell::NEVER),
            None,
            "the Calm guard spends the turn rotating",
        );

        // Reactive on the same line steps at once — the fast turn re-aims with the step.
        let mut reactive = Guard::patrolling(Cell::new(5, 5));
        reactive.look(&facility);
        reactive.respond_to(post); // Responding, walking to the post, lead warm
        assert_eq!(
            reactive.decide(&facility, &[], &mut Rng::new(0), Dwell::NEVER),
            Some(Direction::East),
            "a reactive guard turns fast and steps the same turn",
        );
    }

    /// §7.3/§7.6/§7.7: a responder does not merely *stand* on the cell it was called
    /// to — on arrival it opens the bounded §7.6 sweep, the same one a lost chase
    /// ends in. A call is a lead whose trail is already cold, so without this every
    /// call in the game would resolve as a walk rather than a hunt.
    #[test]
    fn a_responder_searches_the_cell_it_was_called_to() {
        let facility = Facility::walled_box(12, 12);
        let called_to = Cell::new(5, 9);
        let mut guard = Guard::patrolling(Cell::new(5, 5));
        guard.look(&facility);
        guard.respond_to(called_to);

        // Walk it in — `decide` returns the heading, the loop applies it (§4.2), so
        // the test moves the guard itself. The lead ([`ALERT_DURATION`]) far
        // outlasts the four steps.
        for _ in 0..8 {
            if guard.pos == called_to {
                break;
            }
            let step = guard
                .decide(&facility, &[], &mut Rng::new(0), Dwell::NEVER)
                .expect("the responder is still walking");
            let next = guard.pos.step(step).expect("in bounds");
            guard.advance_to(next, step, &facility);
        }
        assert_eq!(guard.pos, called_to, "it reached the cell it was called to");
        assert_eq!(
            guard.state,
            GuardState::Responding,
            "still on the errand until the arriving turn resolves",
        );

        // The turn it has nowhere further to walk, the errand becomes a search.
        guard.decide(&facility, &[], &mut Rng::new(0), Dwell::NEVER);
        assert_eq!(guard.state, GuardState::Alerted, "arrival opens a search");
        assert_eq!(guard.search, SEARCH_DURATION);
        assert_eq!(
            guard.focus,
            Some(called_to),
            "the sweep centres on the called cell",
        );
    }

    /// §7.7: a call carries its own cell and inherits nobody's memory. A guard that
    /// glimpsed the player earlier must not drag that stale sighting into the search
    /// it opens on arrival — it searches where it was *sent*, not where it once
    /// thought you were.
    #[test]
    fn a_call_drops_the_responders_stale_sighting() {
        let facility = Facility::walled_box(12, 12);
        let stale = Cell::new(2, 2);
        let called_to = Cell::new(5, 9);

        let mut guard = Guard::patrolling(Cell::new(5, 5));
        guard.look(&facility);
        guard.last_seen = Some(stale); // it saw the player over there, a while ago
        guard.respond_to(called_to);
        assert_eq!(guard.last_seen, None, "the call clears the old sighting");

        for _ in 0..8 {
            if guard.pos == called_to {
                break;
            }
            let step = guard
                .decide(&facility, &[], &mut Rng::new(0), Dwell::NEVER)
                .expect("walking");
            let next = guard.pos.step(step).expect("in bounds");
            guard.advance_to(next, step, &facility);
        }
        guard.decide(&facility, &[], &mut Rng::new(0), Dwell::NEVER); // arrive → search
        assert_eq!(
            guard.focus,
            Some(called_to),
            "the search centres on the call, not the stale sighting at {stale:?}",
        );
    }

    /// §7.2 (#229): **no guard, in any state, flips 180° in one move** — a reversal
    /// passes through an intermediate quarter, always the clockwise one (§12.4), and
    /// takes ≥2 turns to face fully about. This is the load-bearing half: a guard
    /// cannot spin to face — and detect — a player lined up directly behind it.
    #[test]
    fn no_guard_flips_180_in_one_move() {
        let facility = Facility::walled_box(12, 12);

        // Reactive reversal: south-facing Responder sent due north. It never returns
        // North on the first move; it rotates a clockwise quarter (west), then — now
        // 90° off — turns fast and steps north. Two turns, through the quarter.
        let mut reactive = Guard::patrolling(Cell::new(5, 5));
        reactive.look(&facility);
        reactive.respond_to(Cell::new(5, 1)); // due north — a 180° reversal
        assert_eq!(
            reactive.decide(&facility, &[], &mut Rng::new(0), Dwell::NEVER),
            None,
            "a reactive guard cannot half-turn in one move",
        );
        assert_eq!(
            reactive.facing(),
            Direction::West,
            "it rotated through the fixed clockwise quarter, not straight about",
        );
        assert_eq!(
            reactive.decide(&facility, &[], &mut Rng::new(0), Dwell::NEVER),
            Some(Direction::North),
            "now a quarter off, the fast turn steps north",
        );

        // Calm reversal: same about-face costs more — a rotation to the clockwise
        // quarter (west), another to north, then the step. It faces north only on the
        // second rotation, never in one move.
        let mut calm = Guard::patrolling_to(Cell::new(5, 5), Cell::new(5, 1));
        calm.look(&facility);
        assert_eq!(
            calm.decide(&facility, &[], &mut Rng::new(0), Dwell::NEVER),
            None
        );
        assert_eq!(
            calm.facing(),
            Direction::West,
            "first the clockwise quarter"
        );
        assert_eq!(
            calm.decide(&facility, &[], &mut Rng::new(0), Dwell::NEVER),
            None
        );
        assert_eq!(calm.facing(), Direction::North, "then the second quarter");
        assert_eq!(
            calm.decide(&facility, &[], &mut Rng::new(0), Dwell::NEVER),
            Some(Direction::North),
            "aligned at last, it steps",
        );
    }

    /// §7.5 (#229), the dwell mirror: the moment a Calm guard turns reactive its
    /// slow-turn tax is simply gone — a detection never waits on a pending rotation.
    /// A guard mid-corner (already rotated a quarter east, still Calm) is dispatched
    /// on a fresh 90° heading; a Calm guard would rotate again, but the reactive one
    /// steps at once.
    #[test]
    fn going_reactive_drops_a_pending_slow_turn() {
        let facility = Facility::walled_box(12, 12);
        // Rotate in place once: south spawn facing, target due east — the Calm quarter.
        let mut guard = Guard::patrolling_to(Cell::new(5, 5), Cell::new(8, 5));
        guard.look(&facility);
        assert_eq!(
            guard.decide(&facility, &[], &mut Rng::new(0), Dwell::NEVER),
            None
        );
        assert_eq!(guard.facing(), Direction::East, "mid-corner, facing east");

        // Now dispatched due north — 90° off the current east facing. A Calm guard
        // would spend another turn rotating; going reactive drops the tax and steps.
        guard.respond_to(Cell::new(5, 1));
        assert_eq!(
            guard.decide(&facility, &[], &mut Rng::new(0), Dwell::NEVER),
            Some(Direction::North),
            "the reactive dispatch steps at once — no pending rotation to wait out",
        );
    }

    /// §7.5 (#229) liveness: the turn tax delays a corner by exactly one turn, it
    /// never freezes the sweep. A Calm guard rounding a 90° corner reaches its target
    /// after a single rotation plus the walk — one held turn, then steady progress.
    #[test]
    fn the_slow_turn_delays_a_corner_it_does_not_freeze_it() {
        let facility = Facility::walled_box(12, 12);
        let target = Cell::new(9, 2);
        let mut guard = Guard::patrolling_to(Cell::new(2, 2), target); // due east, a turn
        guard.look(&facility);

        let mut rotations = 0;
        let mut steps = 0;
        for _ in 0..20 {
            if guard.pos() == target {
                break;
            }
            match guard.decide(&facility, &[], &mut Rng::new(0), Dwell::NEVER) {
                Some(dir) => {
                    let dest = guard.pos().step(dir).expect("interior step");
                    guard.advance_to(dest, dir, &facility);
                    steps += 1;
                }
                None => rotations += 1,
            }
        }
        assert_eq!(guard.pos(), target, "the guard reaches its target");
        assert_eq!(
            rotations, 1,
            "exactly one turn is spent rotating at the corner"
        );
        assert_eq!(steps, 7, "then it walks the seven cells east");
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
        guard.decide(&facility, &[], &mut Rng::new(0), Dwell::NEVER);
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
            guard.decide(&facility, &[], &mut Rng::new(0), Dwell::NEVER);
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
        assert_eq!(SEARCH_DURATION, 12, "the [START] search duration");
        assert_eq!(SEARCH_RADIUS, 4, "the [START] search radius");
        assert_eq!(WATCH_DURATION, 20, "the [START] released-watch window");
        assert_eq!(WATCH_RADIUS, 8, "the [START] watch radius");
        assert_eq!(
            GUARD_CLOSE_CHANCE_PERCENT, 25,
            "the [START] guard close-behind chance",
        );
    }

    /// §10.4/§7.6: only a Calm guard closes a door behind itself — a hunting guard
    /// (chasing, investigating, searching, responding) never pauses to tidy up, so
    /// the door it opened stays a lasting sightline.
    #[test]
    fn only_calm_guards_close_doors() {
        let calm = Guard::patrolling(Cell::new(1, 1));
        assert!(calm.closes_doors(), "a Calm guard closes behind itself");
        for hunting in [
            GuardState::Alerted,
            GuardState::Chasing,
            GuardState::Investigating,
            GuardState::Responding,
        ] {
            assert!(
                !Guard::patrolling(Cell::new(1, 1))
                    .with_state(hunting)
                    .closes_doors(),
                "a {hunting:?} guard never closes doors",
            );
        }
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
    /// end of a chase whose sight was broken, and the outer bound on the §7.6 fix-2
    /// search arc. This is the anti-tracking-turret backstop: the guard cannot pursue a
    /// stale lead forever.
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
        guard.decide(&facility, &[], &mut Rng::new(0), Dwell::NEVER);
        assert_eq!(guard.state(), GuardState::Calm, "a cold lead is given up");
    }

    /// §15 Q5 (the found-a-body-nearby half) + §2.2 fairness: a **body** search checks
    /// the cupboards inside the disc it sweeps and nothing beyond — a hidden player *in
    /// range* of the found body is flushed, one *out of range* is left the safe cupboard
    /// it is. A **lost-chase** search checks nothing at all, so hiding still works
    /// against a guard that only lost sight of you (§10.3), and a spent search that
    /// stands down stops checking. The distinction is only ever the result of hiding
    /// within the search a body you left triggered.
    #[test]
    fn a_body_search_checks_only_hideouts_within_its_disc() {
        let body = Cell::new(20, 20);
        let mut finder = Guard::patrolling(body);
        finder.find_body(body);
        assert_eq!(
            finder.state(),
            GuardState::Alerted,
            "a found body opens a search"
        );

        assert_eq!(
            SEARCH_RADIUS, 4,
            "the [START] search radius the check rides on"
        );
        // In range (the §6.1 sight metric): a cupboard inside the swept disc is checked.
        assert!(
            finder.checks_hideout_at(Cell::new(20, 24), false),
            "a cupboard exactly SEARCH_RADIUS south is in the disc",
        );
        assert!(
            finder.checks_hideout_at(Cell::new(23, 23), false),
            "a diagonal cupboard within range is checked",
        );
        // Out of range: one step past the disc is never reached — the body was too far.
        assert!(
            !finder.checks_hideout_at(Cell::new(20, 25), false),
            "one step past the disc is left safe",
        );
        assert!(
            !finder.checks_hideout_at(Cell::new(25, 25), false),
            "a far diagonal cupboard is never checked",
        );

        // A lost-chase search (not a body) checks nothing — the cupboard stays the safe
        // wait-out it is against a guard that merely lost sight of you.
        let mut chaser = Guard::patrolling(body);
        chaser.begin_search();
        assert_eq!(
            chaser.state(),
            GuardState::Alerted,
            "a lost chase also opens a search"
        );
        assert!(
            !chaser.checks_hideout_at(body, false),
            "a lost-chase search never checks a hideout",
        );

        // A body search whose lead runs cold stands the guard down and stops checking.
        finder.stand_down();
        assert!(
            !finder.checks_hideout_at(Cell::new(20, 20), false),
            "a spent search checks nothing",
        );
    }

    /// The `guards_always_search_hideouts` level modifier (§12.6), directional: the
    /// harder setting must flush at least as much as baseline. With `always_search`
    /// on, a **lost-chase** search — the one that baseline leaves the safe wait-out
    /// (§10.3) — now checks the cupboards inside its disc, and nothing beyond it, so
    /// the modifier strictly *adds* pressure without widening the swept area. A Calm
    /// guard that has released to a watch keeps its `focus` but is no longer
    /// searching, so the modifier never makes it flush.
    #[test]
    fn the_always_search_hideouts_modifier_flushes_a_lost_chase() {
        let lead = Cell::new(20, 20);
        let mut chaser = Guard::patrolling(lead);
        chaser.begin_search();
        assert_eq!(chaser.state(), GuardState::Alerted, "a lost chase searches");

        // Baseline (modifier off): the lost chase checks nothing — the wait-out holds.
        assert!(
            !chaser.checks_hideout_at(lead, false),
            "baseline: a lost-chase search leaves the cupboard safe",
        );
        // Modifier on: the same search now flushes a cupboard within its disc …
        assert!(
            chaser.checks_hideout_at(lead, true),
            "modifier: a lost-chase search flushes a cupboard at its focus",
        );
        assert!(
            chaser.checks_hideout_at(Cell::new(20, 24), true),
            "modifier: … and one exactly SEARCH_RADIUS away, inside the disc",
        );
        // … but never one beyond the disc: the modifier widens *what* is searched,
        // not *where* — the swept area is unchanged.
        assert!(
            !chaser.checks_hideout_at(Cell::new(20, 25), true),
            "modifier: a cupboard one step past the disc is still left safe",
        );

        // A guard released from its search to a Calm watch keeps `focus` but is no
        // longer Alerted, so the modifier does not turn its stale focus into a flush.
        chaser.release_from_search();
        assert_eq!(chaser.state(), GuardState::Calm, "released to a watch");
        assert!(
            !chaser.checks_hideout_at(lead, true),
            "modifier: a Calm watcher with a stale focus never flushes",
        );
    }
}
