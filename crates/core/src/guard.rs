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
use std::collections::HashSet;

use crate::vision::{
    field_of_view_with_blind_spot, BlindPolicy, VisibleSet, GUARD_SIGHT_ARC, GUARD_SIGHT_RANGE,
};

mod patrol;
use patrol::{patrollable, pick_farthest, roll_dwell};
pub(crate) use patrol::{Dwell, PatrolStyle};

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

/// A guard's already-decided turn (§4.2/#430): the state of mind and the walking
/// destination it entered phase 3 with, snapshotted before this turn's look is
/// folded in ([`Guard::plan`]). The movement pass hands it back to
/// [`Guard::decide_planned`] for a guard whose look this turn *first* alerted it,
/// so the guard finishes the turn it had planned instead of acting on the fresh
/// sighting.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Plan {
    pub(crate) state: GuardState,
    pub(crate) destination: Option<Cell>,
}

/// A guard on the level.
///
/// A Calm guard **patrols** (§7.5): it sweeps toward the farthest cell in its
/// territory it has not recently looked at, keeping a private memory of
/// the cells its cone has covered and wiping it to start over once the territory is
/// exhausted. On a generated level its territory is a region **beat** (§10.5, see
/// [`crate::beat`]): rooms *and the corridors joining them*, grown across door edges
/// from the region the guard **stood in** when the beat was cut, so the sweep walks
/// room → corridor → room.
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
    /// The cells of this guard's region beat (§7.5/§10.5): the region the guard
    /// **stood in when the beat was cut**, grown across door edges ([`crate::beat`]),
    /// so every cell is walkable from that anchor and no territory straddles a wall.
    /// The sweep filters it to the patrollable cells each pick, so later-stamped
    /// solids (a console) never become targets.
    ///
    /// The anchor is a *live position*, not a spawn cell: a guard that arrived
    /// mid-level (§7.3/#374) is cut a beat around where its errand **finished**, not
    /// around the room it walked in by. That is why growth is called only at
    /// placement and when the guard set changes — never per turn, or a moving anchor
    /// would make patrols churn.
    ///
    /// Empty for a guard built without a graph — a hand-placed fixture, or a
    /// reinforcement still on its errand — and a guard with no beat has **no
    /// territory**: it holds rather than sweeping a box drawn round a phantom
    /// anchor (§7.5).
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
    /// widens back to the beat territory.
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
    /// The consoles this guard's **beat** touches (§12.6/#319), worked out once and
    /// kept: the `$` and `Ψ` cells orthogonally adjacent to a cell of
    /// [`beat`](Self::beat). `None` until the first patrol pick asks for it, and
    /// dropped whenever the beat changes — see
    /// [`learn_beat_consoles`](Self::learn_beat_consoles) for why it is learned late
    /// rather than at placement. Read only under
    /// [`PatrolStyle::WatchedConsoles`](PatrolStyle); the field costs a baseline run
    /// nothing but the `None`.
    beat_consoles: Option<Vec<Cell>>,
    /// Which of those consoles this guard has stood beside **this cycle**
    /// (§12.6/#319). The console leg prefers one that is not in here; when they all
    /// are, the cycle is wiped and starts over — §7.5's inspected-memory wipe, over
    /// the watched set rather than over the ground. This is what makes coverage
    /// bounded ([`CONSOLE_CYCLE_TURNS`]) instead of lucky.
    consoles_visited: Vec<Cell>,
    /// Whether the **next** patrol pick is a console leg (§12.6/#319) — the whole of
    /// the interleaving rule, flipped at each repick so at most every second leg is
    /// diverted and §7.5's farthest-uninspected sweep keeps covering the plain ground
    /// between them. `true` at spawn, so a watched beat's consoles are visited early
    /// rather than after a full sweep.
    console_leg_due: bool,
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
/// beat territory, so coverage there is briefly raised before the sweep drifts
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

/// How long a watched beat's console cycle may take, in turns (§12.6/#319,
/// **[START] = 300**): under `guards_watch_consoles`, every console inside a guard's
/// beat has that guard **stand orthogonally beside it** within this many turns of
/// uninterrupted Calm patrol, counted from the start of the level.
///
/// It is a *bound*, not a period: the cycle's real length is a beat's own business —
/// how many consoles it holds, how far apart they are, and the §7.5 dwell it pays at
/// every arrival, plus the ordinary farthest-uninspected leg alternating between them.
/// What the constant promises is that the cycle **closes**, which is the difference
/// between this modifier and a guard that merely happens past a console (§2.3).
///
/// Measured rather than chosen. Over 256 generated seeds run idle, the **typical**
/// console is stood beside inside ~150 turns; the slowest was 578, on a wide beat
/// holding three consoles, where a cycle pays six legs of a 40×40 building plus a dwell
/// at each. The bound sits above that with room, so an unlucky carve reads as a wider
/// beat rather than as a failing test.
///
/// It is worth stating what it is a bound *against*: at baseline, over the same seeds,
/// **a third of all consoles are never stood beside at all** within 600 turns. The
/// difference between "within 800" and "possibly never" is the whole modifier.
///
/// Pinned by `every_console_in_a_beat_is_stood_beside_within_the_cycle_bound`; if the
/// interleave or the dwell is retuned, that test is where it shows.
///
/// **Nothing reads it at runtime**, which is why it is test-gated: the cycle is what
/// bounds coverage, and a guard that consulted a turn budget would be a timer, not a
/// patrol. It lives here rather than in the test file because it is a statement about
/// the rule, and the rule is here.
#[cfg(test)]
pub(crate) const CONSOLE_CYCLE_TURNS: u32 = 800;

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
            beat_consoles: None,
            consoles_visited: Vec::new(),
            console_leg_due: true,
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

    /// The same guard in `state`. The §7.4 transitions are the reactive guard AI
    /// tickets' job; until they land, this is how a scenario — a test, the sim —
    /// puts a guard in a non-[`Calm`](GuardState::Calm) state.
    pub fn with_state(mut self, state: GuardState) -> Self {
        self.state = state;
        self
    }

    /// The same guard sweeping `beat` as its Calm territory (§7.5/§10.5) — the
    /// cells of the region beat grown across door edges from the region the guard
    /// stands in. This is how [`Placement::guards`](crate::Placement::guards) spawns
    /// every guard on a generated level, and how a reinforcement is cut one once its
    /// errand ends ([`State::settle_new_beats`](crate::State)); a guard given no beat
    /// has no territory and holds.
    pub fn with_beat(mut self, beat: Vec<Cell>) -> Self {
        self.set_beat(beat);
        self
    }

    /// Give this guard `beat` as its Calm territory (§7.5/§10.5) — the in-place form
    /// of [`with_beat`](Self::with_beat), for a beat cut after the guard already
    /// exists ([`State::settle_new_beats`](crate::State)).
    pub(crate) fn set_beat(&mut self, beat: Vec<Cell>) {
        self.beat = beat;
        // New ground, new consoles to watch (§12.6/#319): the cached set and the cycle
        // it is tracked against both belong to the *old* beat, so a recut drops them
        // rather than leaving a guard cycling consoles that are now somebody else's.
        // The alternation restarts with it, on the same reading a spawn takes — new
        // ground is ground whose consoles have not been looked at.
        self.beat_consoles = None;
        self.consoles_visited.clear();
        self.console_leg_due = true;
    }

    /// Whether this guard has a §7.5 beat to patrol. `false` is a guard with no
    /// territory — a hand-placed fixture, or a reinforcement whose beat has not been
    /// cut yet — and is the seam the reinforcement settle pass looks for
    /// ([`State::settle_new_beats`](crate::State)).
    pub(crate) fn has_beat(&self) -> bool {
        !self.beat.is_empty()
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

    /// Whether this guard is in the §7.6 post-search **watch** — Calm again, but still
    /// sweeping the area its search was centred on rather than its ordinary beat.
    #[cfg(test)]
    pub(crate) fn watching(&self) -> bool {
        self.watch > 0 && self.focus.is_some()
    }

    /// The cell an active search or watch is centred on (§7.6) — where this guard
    /// believes the trail ran out. Read by the §7.7 call-in: the cell a lost
    /// sighting reports is the one the loser is itself about to sweep, so the two
    /// can never disagree.
    pub(crate) fn focus(&self) -> Option<Cell> {
        self.focus
    }

    /// The turn this guard has already decided on (§4.2/#430) — its state of mind
    /// and the cell it is walking to, as a value the guard phase snapshots before
    /// any look is folded in ([`GuardSenses`](crate::State)).
    pub(crate) fn plan(&self) -> Plan {
        Plan {
            state: self.state,
            destination: self.destination,
        }
    }

    /// This guard's §7.5 beat — the cells of its territory, empty for a guard built
    /// without a region graph. The seam for asserting that a guard which arrived
    /// mid-level (§7.3/#374) got one, so it has somewhere to patrol once its errand
    /// ends rather than standing where it finished forever.
    #[cfg(test)]
    pub(crate) fn beat(&self) -> &[Cell] {
        &self.beat
    }

    /// Recompute this guard's cone from its current position and facing (§6.2/§7.1),
    /// with `blind` carved out of its touching ring (§155/#410): at
    /// [`BlindTier::REAR`] the three cells at the guard's back do not detect, so a
    /// takedown can be set up from directly behind. The sight phase calls this for
    /// every guard before any of them act, so the decisions below read a cone that
    /// matches where the guard actually stands.
    ///
    /// The **policy** is passed in, never stored — it belongs to the level's modifiers
    /// (§12.3), and a copy on the guard would be a second reading of the same fact,
    /// free to go stale against the live one (the #199/#200 shape). It arrives the
    /// same way [`PatrolStyle`] does: derived from state, handed down per call.
    ///
    /// The guard resolves it to a [`BlindTier`] against **its own mood**
    /// ([`BlindPolicy::tier`]), which is why the policy comes down rather than the
    /// tier: under [`BlindPolicy::FlankWhileCalm`] a Calm patrol is blind at its
    /// flanks and an alerted guard is not, and the guard is the only thing that knows
    /// which it is. Recomputed every sight phase, so a guard that goes from Calm to
    /// hunting gets its sides back on its next look.
    pub(crate) fn look(&mut self, facility: &Facility, blind: BlindPolicy) {
        self.fov = field_of_view_with_blind_spot(
            facility,
            self.pos,
            self.facing,
            GUARD_SIGHT_ARC,
            GUARD_SIGHT_RANGE,
            blind.tier(self.state),
        );
    }

    /// Apply a successful step (§4.2 phase 3): stand on `dest`, face `dir` — facing
    /// follows movement (§5) — and refresh the cone at once, so a frame never shows
    /// the guard in one place with its sight in another (§11.5).
    pub(crate) fn advance_to(
        &mut self,
        dest: Cell,
        dir: Direction,
        facility: &Facility,
        blind: BlindPolicy,
    ) {
        self.pos = dest;
        self.facing = dir;
        self.look(facility, blind);
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
    ///
    /// The lead does **not** have to cover the journey (§7.3/#409): a travelling
    /// responder does not spend it
    /// ([`keep_lead_for_the_road`](Self::keep_lead_for_the_road)), so
    /// [`ALERT_DURATION`] is the budget for what happens *at* the cell, however far
    /// away the cell is. That is what lets one constant serve a dispatch from next
    /// door, a §7.7 call-in across two rooms and a reinforcement walking in from the
    /// far edge of the facility (§7.3/#374) alike — the errand-sized lead each of
    /// those used to need was only ever paying for the commute.
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
        style: PatrolStyle,
        blind: BlindPolicy,
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
                    return self.commit_step(Some(step), facility, blind);
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
        //
        // **A responder does not burn its lead while it is still on its way**
        // (§7.3/#409). The lead bounds the *investigation*, not the commute: spending
        // it on the walk meant the further from the patrols you struck, the less the
        // radio cost you, and a dot that visibly peeled off toward the site would
        // wander halfway and turn yellow again — teaching a rule that is not the rule.
        // So a responder that is genuinely travelling refunds the turn `sense` cooled.
        // Scoping it to `Responding` is what keeps §7.6's anti-tracking-turret backstop
        // intact: a call carries a **fixed** cell that never updates (`respond_to`
        // drops the stale sighting), so a responder cannot follow the player, while a
        // *chase*, whose destination tracks you, still cools every turn above.
        // The refund is conditioned on a route existing this turn, not on the state:
        // a guard held up by a colleague (§7.8) or with nowhere to route still cools
        // and still stands down, so nothing paces forever.
        if self.state == GuardState::Responding {
            if self.alert > 0 {
                if let Some(step) = self.step_toward_destination(facility, blocked) {
                    self.keep_lead_for_the_road();
                    return self.commit_step(Some(step), facility, blind);
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
                    return self.commit_step(Some(step), facility, blind);
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

        self.repick_patrol_target(facility, style, rng);
        let step = self.step_toward_destination(facility, blocked);
        self.commit_step(step, facility, blind)
    }

    /// [`decide`](Self::decide), run on `plan` — the mind this guard entered the
    /// phase with — instead of on the live one (§4.2/#430): **a guard whose look
    /// first alerts it does not act on that look.** It carries out the turn it had
    /// already decided — its patrol step, its dwell, the slow quarter of a
    /// reversal — and the fresh state routes it from the next turn's decision.
    /// The caller gates this on the guard having been Calm at the head of the
    /// phase, so a guard already hunting still reacts the same turn, and a chase
    /// that re-acquires the player after a broken turn of sight is never handed a
    /// second delay.
    ///
    /// Mechanically a swap: the planned state and destination are put back for
    /// the decision, then the live mind — the state this turn's look set, the
    /// destination on the player — is restored over whatever the planned decision
    /// wrote there (a patrol repick's target is deliberately dropped; the chase
    /// owns the next turn). Everything else the decision did is *kept*, because
    /// it is the planned turn actually being spent: a dwell counts down, the
    /// inspected memory absorbs the cone, and a rotation turns the guard in
    /// place. The step returned is walked for real by the movement pass — one
    /// that lands on the player's cell is §4.5 contact and still captures.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn decide_planned(
        &mut self,
        plan: Plan,
        facility: &Facility,
        blocked: &[Cell],
        rng: &mut Rng,
        dwell: Dwell,
        style: PatrolStyle,
        blind: BlindPolicy,
    ) -> Option<Direction> {
        let (state, destination, facing) = (self.state, self.destination, self.facing);
        self.state = plan.state;
        self.destination = plan.destination;
        let step = self.decide(facility, blocked, rng, dwell, style, blind);
        self.state = state;
        self.destination = destination;
        // **The cone is re-aimed against the guard's real mood, never the planned
        // one** (§6.1/#410). A planned turn that spends itself rotating (§7.5's slow
        // quarter) re-aims through [`turn_in_place`], which resolves
        // [`BlindPolicy`] against `self.state` — and that was the *planned* Calm for
        // the length of the decision above. Left there, a guard that has just
        // spotted the player would look with a **patrol's** blind flanks, which is
        // exactly the pricing [`BlindPolicy::FlankWhileCalm`] exists to enforce: the
        // flank is somewhere to work from against a patrol you have read, never
        // somewhere to hide from a guard that is hunting you. It would also paint
        // that patrol's cone on the danger overlay for a guard the player can see
        // (§11.5), which is the overlay lying about a mood the `g` glyph is
        // simultaneously colouring Danger.
        //
        // So the rotation is the planned turn (it happens, and it is slow, because
        // that is the turn the guard had decided), while the **sight** it leaves
        // behind is this turn's. A guard that *stepped* needs nothing here: the
        // movement pass re-aims it through [`advance_to`] with the live state. Sight
        // is a pure recompute and draws no RNG (§12.4), so the extra cast costs a
        // frame's work and cannot perturb the stream.
        if self.facing != facing {
            self.look(facility, blind);
        }
        step
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
    fn commit_step(
        &mut self,
        step: Option<Direction>,
        facility: &Facility,
        blind: BlindPolicy,
    ) -> Option<Direction> {
        let dir = step?;
        if dir == self.facing {
            return Some(dir);
        }
        if dir == self.facing.opposite() {
            self.turn_in_place(self.facing.clockwise(), facility, blind);
            return None;
        }
        if self.state == GuardState::Calm {
            self.turn_in_place(dir, facility, blind);
            None
        } else {
            Some(dir)
        }
    }

    /// Rotate in place to `facing` without stepping (§7.5/§7.2): position unchanged,
    /// the cone re-aimed at once so the overlay stays honest (§11.5). The guard has
    /// spent its whole turn on the rotation.
    fn turn_in_place(&mut self, facing: Direction, facility: &Facility, blind: BlindPolicy) {
        self.facing = facing;
        self.look(facility, blind);
    }

    /// Give back the turn of lead [`sense`](Self::sense) cooled, because this turn was
    /// spent **travelling** rather than investigating (§7.3/#409).
    ///
    /// Every reactive timer cools once per turn at the top of the guard's turn, before
    /// anything knows whether the guard has a route to walk; a responder only learns it
    /// is still on its way when [`decide`](Self::decide) finds a step. So the rule is
    /// expressed as a refund rather than a skipped decrement — the arithmetic is exact
    /// either way, since sensing and deciding are gated on the same daze check and so
    /// happen exactly once each per turn (§8.3).
    ///
    /// The refund can never *grow* a lead past what the call granted. In the turn loop
    /// that clamp is a no-op — a responder's lead is only ever set by
    /// [`respond_to`](Self::respond_to), and `decide` only reaches here with `alert > 0`,
    /// so the refund restores exactly what the same turn took. It is stated anyway so
    /// the rule holds for a fixture that drives `decide` without a sense pass, and so
    /// "a responder does not spend its lead travelling" can never quietly become "a
    /// responder earns lead by travelling".
    fn keep_lead_for_the_road(&mut self) {
        self.alert = (self.alert + 1).min(ALERT_DURATION);
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
    /// widens back to the beat. The live lead — destination, alert, last-known cell
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
}

/// Whether a guard's walk may **route through** `cell` (§10.4): everything
/// [`patrollable`], plus a **closed door panel** — the §10.3 table's one
/// deliberate surprise (a closed panel does not block pathing): a guard heading
/// somewhere walks up to the door and opens it by bumping in, which is how guard
/// traffic monotonically opens the facility up over a level. Kept apart from
/// [`patrollable`] so a panel is walked *through*, never chosen as a sweep or
/// search target the guard could not stand on.
///
/// Shared with the radio (§7.3): control prices a dispatch by the route the guard
/// would actually walk ([`nearest_respondable`](crate::radio::nearest_respondable)),
/// and it must measure that walk with the same predicate the walk itself uses, or
/// the pick and the journey would disagree.
pub(crate) fn routable(facility: &Facility, cell: Cell) -> bool {
    patrollable(facility, cell) || facility.terrain(cell) == Some(Terrain::DoorPanelClosed)
}

#[cfg(test)]
mod tests;
