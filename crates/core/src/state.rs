//! The turn loop and the running game state (§4.2, §4.4, §4.5, §12.1).
//!
//! This is the heartbeat: `state × input → state, events`. [`State::step`] resolves
//! one turn in the fixed three-phase order (§4.2) — **player, sight, guards** — and
//! returns the events it produced. The core is pure and deterministic (§12.1): the
//! same state and the same input always yield the same next state and the same event
//! stream, which is what makes a run a `(seed, [inputs])` replay (§12.4).
//!
//! Three rules the loop is built around:
//!
//! - **Turn cost (§4.4), the rule that matters most.** *Every action that changes the
//!   world costs the turn.* A move, a bump that opens a door, taking the intel — all
//!   advance the turn, which is what lets the guards act. The exceptions are few and
//!   enumerated: moving into a wall is **free** (it's a mis-input, not a decision),
//!   and — once abilities exist — toggling one off is free. A free action does not
//!   end the turn, so the world does not move and the guards do not get a go.
//! - **Win and lose (§4.5), the only two.** *Lose:* a guard moving into your cell
//!   captures you — contact, not detection, so being unseen is not being safe. *Win:*
//!   take enough intel for the run's gate ([`exit_ready`](State::exit_ready), §4.5/
//!   §12.6 — quick play wants all of it, the sim one), then return to the exit you
//!   came in by; bumping it short of the gate refuses. There is no health, no combat.
//! - **The startup turn (§4.2).** One full turn runs at level start, before the first
//!   input, so guards have position and sight established when the player first acts.
//!
//! **Sight is real** (§6): phase 2 recomputes every viewer's field of view — the
//! player's ~180° half-disc (360° on a turn spent waiting, §8.3) and each guard's
//! ~90° wedge — from its *current* position and facing, which is what designs out the
//! old one-turn sensory lag (§4.2). **Guards patrol** (§7.5): phase 3 runs each
//! guard's `decide` step, which reads the sight this loop just recomputed and, for a
//! Calm guard, sweeps its territory toward the farthest cell it has not yet looked at.
//! Guards detect on **vision alone** (§9 **[SETTLED]** — there is no sound, no
//! hearing): a player in a guard's cone flips it to Chasing or Investigating (§7.6),
//! and a lost lead searches its area, watches, and only then stands back down. The
//! whole §7.4 state machine is built — the reactive states, the bounded search, the
//! radio dispatch (§7.3), the decoy — and every one of them sets a guard's destination
//! the same way and reuses the same walk-toward-it movement.
//!
//! # Where the loop lives
//!
//! This file owns the [`State`] itself, [`step`](State::step), the player phase and
//! the sight phase. The rest is next door, each in its own module: the public
//! [`Input`]/[`Event`]/[`Affordance`] vocabulary ([`events`]), the read surface the
//! renderer and the §13.2 bot ask ([`view`]), phase 3 ([`guards`]), the doors' own turn
//! ([`doors`]), the §9 sense channel both halves of which fade on one model
//! ([`sense`]), the ability effects ([`abilities`]) and the #57 auto-slide
//! ([`traversal`]). They are all `impl State` blocks over the *same* struct — plain
//! structs, not an ECS (§12.3), so the coupling stays visible in the types.

use crate::ability::{
    AbilityId, AbilityState, AbilityStatus, Behaviour, Deck, Effect, Loadout, TargetingMode,
};
use crate::alert::{Alert, AlertReadout, AlertTrigger, AlertTuning};
use crate::body::Body;

use crate::category::Category;
use crate::cell::{Cell, Direction};
use crate::cover;
use crate::duct::Duct;
use crate::exchange::{Choice, Exchange};
use crate::facility::{Facility, Terrain};
use crate::generate::Layout;
use crate::guard::{
    Dwell, Guard, GuardState, PatrolStyle, Plan, GUARD_CLOSE_CHANCE_PERCENT,
    GUARD_DWELL_CHANCE_PERCENT,
};
use crate::level_seed::LevelSeed;
use crate::modifiers::{DebugModifiers, LevelModifiers};
use crate::radio;
use crate::region::{DoorCell, DoorId, RegionId};
use crate::rng::Rng;
use crate::status::MessageHistory;
use crate::targeting::Targeting;
use crate::verdict::{Ending, RunStats, Verdict};
use crate::vision::{
    field_of_view_with_peek, BlindPolicy, VisibleSet, ENHANCED_SIGHT_RANGE, FULL_SIGHT_ARC,
    PLAYER_SIGHT_ARC, PLAYER_SIGHT_RANGE,
};
use crate::DoorAction;

mod abilities;
mod activation;
mod bore;
mod doors;
mod effects;
mod events;
mod guards;
mod lockdown;
mod reinforcements;
mod sense;
mod traversal;
mod view;

pub use bore::BoreRefusal;
pub use effects::EffectArea;
pub use events::{Affordance, Event, Input};
pub(crate) use reinforcements::{RUNG_THREE_REINFORCEMENTS, RUNG_TWO_REINFORCEMENTS};
pub use sense::SenseMark;

use activation::Aimed;
use effects::EffectMark;
use sense::{SenseCue, SenseSource};

/// The player and every guard are solid and exclusive — fill 1.0 (§4.3). A cell
/// already holding one admits no other actor.
pub(crate) const ACTOR_FILL: f32 = 1.0;

/// The player's **guard-sense** range (§9.1 **[START]**): the player always knows the
/// exact cell of every guard within this Chebyshev box, **through walls** — a 21×21
/// box, the same shape as sight (§6.1) at a smaller size. It reveals *position only*;
/// facing and the cone are shown only for a guard actually seen (§9.2). Pinned by a
/// test so a later change is a deliberate, visible edit.
pub const PLAYER_SENSE_RANGE: u32 = 10;

/// The guard-sense range on a turn the player spent **waiting** (§9.1 **[START]**): a
/// 41×41 box. Wait already buys 360° vision for the turn (§8.3); it now *also* widens
/// the sense, 10 → 20 — "stop and take stock of the whole area", cost-is-load-bearing
/// applied to information (§2.3). Pinned by a test.
pub const PLAYER_SENSE_RANGE_WAITING: u32 = 20;

/// The guard-sense range while **inside a duct** (§10.7 **[START]**): a reduced box,
/// smaller than the open-floor [`PLAYER_SENSE_RANGE`]. Degraded information is the
/// duct's whole cost (§2.3): mid-crawl you perceive only your memory of the building
/// and this shortened sense, and the safe way to resurface is the mouth peek (§6.1).
/// **Waiting does not widen it** — the [`PLAYER_SENSE_RANGE_WAITING`] extension is an
/// open-floor affordance; a crawlspace is exactly where you should *not* be able to
/// take stock of the whole area. Pinned by a test.
pub const DUCT_SENSE_RANGE: u32 = 5;

/// The player's **door-sense** range (§9.4/§10.4 **[START]**): a door that opens or
/// shuts away from the player — a guard walking through one, or an automatic door
/// timing shut (§10.4) — is felt at this Chebyshev range, **through walls**, and
/// leaves a fading on-grid cue in the [`Category::Sensed`](crate::Category::Sensed)
/// channel — the same one as a guard felt through a wall. It is a
/// louder, coarser event than a guard's exact position, so it carries **farther**
/// than the guard sense: `DOOR_SENSE_RANGE > `[`PLAYER_SENSE_RANGE`] (pinned by a
/// test). Doors are the facility breathing around you — you feel that from across a
/// wing even when you could not pinpoint the guard that did it. It is a `[START]`
/// tuning number: large enough to reach the next wing, small enough that the whole
/// facility does not pulse on every guard step (§2.3 in the other direction).
///
/// **Waiting does not widen it** (unlike the guard sense): the Wait extension is
/// about taking careful stock of *precise* guard positions, whereas a door change is
/// already loud enough to feel regardless. **Inside a duct it shrinks** to
/// [`DUCT_SENSE_RANGE`] with the rest of the crawlspace's degraded perception
/// (§10.7) — see [`door_sense_range`](State::door_sense_range).
pub const DOOR_SENSE_RANGE: u32 = 15;

/// The door sense reaches **farther** than the guard sense (§9.4/§10.4) — a coarse
/// "a door moved over there" carries past the precise "that guard is on that cell".
/// Pinned at compile time so the asymmetry can never silently invert.
const _: () = assert!(DOOR_SENSE_RANGE > PLAYER_SENSE_RANGE);

/// How many turns a door-change cue stays lit before it fades (§9.4/§10.4
/// **[START]**): a door open/close is a **discrete** event that will not restate
/// itself, so its cue is inherently a fading mark. It reads like sensed evidence —
/// visible while the fact is fresh — rather than a single-frame flash. Placed at full
/// life the turn the door changes and decremented once per world turn, so the cue
/// shows for this many renders and is gone on the next. Pinned by a test.
///
/// It is the **longer** of the sense channel's two lives (§9.4/#192): the guard cue
/// beside it ([`GUARD_CUE_DECAY_TURNS`]) is re-stamped every turn its guard is still
/// felt, so it needs to carry only the tail behind a live position.
pub const DOOR_CUE_DECAY_TURNS: u32 = 3;

/// How many turns a **guard** cue stays lit before it fades (§9.2/§9.4 **[START]**,
/// #192): the sense stamps the cell of every guard it feels through a wall, once per
/// world turn, and the stamp fades over this many turns. It is the other half of the
/// one persist-and-fade channel [`DOOR_CUE_DECAY_TURNS`] governs, and the tuning story
/// is shared: what varies between them is only how long each fact stays worth showing.
///
/// **Two**, and deliberately the shortest life in the channel. Its job is a **trail**
/// short enough to say *was just here* and no longer — a mark that outlived that would
/// be a readable heading, and §9.2 gives position, never intent. Two turns of tail
/// behind the live dot is enough to see a guard is on the move (and, when it leaves the
/// box, to leave a ghost of the last cell it was felt in) without handing the player a
/// vector to extrapolate. Raising it is the one knob that can turn the trail into an
/// arrow, so it is pinned by a test.
pub const GUARD_CUE_DECAY_TURNS: u32 = 2;

/// The guard trail stays **no longer** than the door cue (§9.4/#192) — the tail behind
/// a position the sense re-states every turn cannot outlive the discrete event that
/// gets one chance to be read. Pinned at compile time so the pair cannot silently
/// invert into a heading-length trail.
const _: () = assert!(GUARD_CUE_DECAY_TURNS <= DOOR_CUE_DECAY_TURNS);

/// The **Confusion** blast radius (§8.3/§9/#240/#325 **[START]**): the ability fires
/// once, and every guard standing within this Chebyshev box of the player *at that
/// moment* is dazed — measured the same way as the guard sense (§6.1 box metric) and,
/// like it, reaching **through walls** (§9).
///
/// It stays **smaller** than [`PLAYER_SENSE_RANGE`] (asserted below) so the blast can
/// never reach a guard the player cannot already sense — but it covers a guard's whole
/// *certain* zone (`CERTAIN_RANGE` = 5, §7.6), so the guards actually bearing down on
/// you are the ones it catches. Raised from the first pass's 4, where the blast was
/// tight enough that a chaser could sit just outside it. Pinned by a test so a later
/// change is a visible edit.
///
/// This is the **cap**, not the answer: the reach actually fired is
/// [`confusion_blast`](State::confusion_blast), which clamps it down to the live
/// [`sense_range`](State::sense_range) so the blast can never outreach what the player
/// perceives — inside a duct, that is [`DUCT_SENSE_RANGE`].
pub const CONFUSION_RADIUS: u32 = 6;

/// The **Lockdown** seal radius (§8.3/§10.4/#242 **[START]**): activating Lockdown
/// shuts and seals every door with a cell inside this Chebyshev box of the player —
/// measured the same way as the guard sense and Confusion's bubble (§6.1 box metric)
/// and, like them, reaching **through walls**.
///
/// Deliberately **small**, and smaller than [`CONFUSION_RADIUS`]: the doors it takes
/// are the ones in the room you are standing in and the corridor you just left, not a
/// whole wing. A radius that sealed a wing would freeze the level's traffic for its
/// whole window — an ability that plays the map for you — where this one buys a
/// pursuer's detour and nothing more (§7.6). It is the ability's main power lever, so
/// it is pinned by a test and expected to move in playtest.
pub const LOCKDOWN_RADIUS: u32 = 4;

/// Lockdown's reach stays **within the guard sense** (§9/§8.3), for the same reason
/// Confusion's does: a door sealed beyond the range the player can perceive would be a
/// wall they had no way to read. Pinned at compile time.
const _: () = assert!(LOCKDOWN_RADIUS <= PLAYER_SENSE_RANGE);

/// How long a guard caught by a Confusion blast stays **dazed** (§8.3/#325
/// **[START]**): blinded and frozen for this many turns, counted down on the guard's
/// own clock ([`Guard::shake_off_daze`](crate::Guard)) rather than on the player's.
///
/// Six, on §8.2's convention — the firing turn is the first of them, every phase of
/// which already saw the guard frozen, so N means N. It is deliberately the same
/// number as the six-turn *window* the fired model replaced: what changed is when the
/// set of guards is decided and where the timer lives, not how much time the panic-buy
/// buys, so the retune (if the sim wants one, §13.2) stays a separate, visible edit.
/// Pinned by a test.
pub const CONFUSION_DAZE_TURNS: u32 = 6;

/// How many turns a **momentary** effect mark stays painted (§11.5 **[START]**,
/// #308/#338): the cyan wash that reports an effect which *is* a moment — Confusion's
/// box, the cell a bore opened, and every later fixed-cell effect. **One turn** — a
/// true flash, the acting frame and nothing after it. It exists to answer *what just
/// happened, and where* once, at the moment the player asks it, and a 13×13 field of
/// background is a great deal of ink to leave on the board while the danger overlay is
/// the thing that matters (§11.5 [SETTLED]).
///
/// What carries a *state* for longer is a **standing** mark instead (a guard still
/// frozen, later a live decoy or concealment in force), which costs no ink beyond the
/// cell it rides. That division is why one turn is the right life here: a momentary
/// mark reports **an event** — where it landed and how far it went — while *what is
/// still held* is a standing fact the other lifetime says better, and says truthfully
/// for a guard that has since walked out of the box (§8.3/#325).
///
/// Lit at full life the turn the effect acts and decremented once per spent turn, so
/// the mark shows for this many renders and is gone on the next — the same
/// persist-and-fade shape as [`DOOR_CUE_DECAY_TURNS`], which is why raising it is a
/// one-number change if playtest wants the boundary visible for longer. Pinned by a
/// test.
pub const EFFECT_FLASH_TURNS: u32 = 1;

/// Confusion's blast stays **within the guard sense** (§9/#240): a guard is never
/// frozen before the player can even sense its dot, so the effect is always legible
/// on the map. Pinned at compile time so the two ranges can never silently invert.
///
/// This is the **open-floor** half of the promise, and only that. Where the sense
/// itself shrinks below the cap — inside a duct, §10.7 — what keeps the promise is the
/// clamp in [`confusion_blast`](State::confusion_blast). The two are deliberately not
/// interchangeable: this one states a fact about the catalog's numbers at compile time,
/// the clamp states the rule at every firing, and neither stands in for the other.
const _: () = assert!(CONFUSION_RADIUS <= PLAYER_SENSE_RANGE);

/// The flat part of the **stun** the safety eject costs (§8.3 **[START]**, #329) —
/// the turns owed for being thrown *at all*, on top of one turn per cell thrown
/// ([`phase_eject_stun`]).
///
/// While the counter runs, every [`Input`] resolves as a *stunned pass* — the turn is
/// spent, the world phases run, and the player changes nothing (see
/// [`player_phase`](State::player_phase)). It is a real price in a patrolled facility:
/// capture is contact (§4.5 **[SETTLED]**), so standing still on a cell the RNG picked
/// can end the run just as the wall used to — only now by a guard the player could see
/// coming. Pinned by a test so a later change is a deliberate, visible edit.
pub const PHASE_EJECT_STUN_BASE: u32 = 1;

/// How long the safety eject leaves the player **stunned** (§8.3 **[START]**, #329):
/// [`PHASE_EJECT_STUN_BASE`] plus **one turn per cell thrown** — the §6.1 box distance
/// from the solid they were stuck in to where they landed.
///
/// The length scales with the throw because that is what *prices recklessness*. A
/// phase that ends clipping the corner of a table is one cell out and costs the
/// smallest stun there is; burying yourself in the middle of a thick wall block is
/// three cells out and costs three times as much helplessness to undo. A flat rate
/// charged both the same, which let the worst case — deep inside a structure, far from
/// anywhere to stand — be as cheap as the near miss. The distance is the one the eject
/// search already found ([`eject_from_solid`](State::eject_from_solid)), so the price
/// cannot disagree with the throw that set it.
///
/// The flat base is what stops the cheapest case being free: you are on the floor
/// however short the trip was.
pub fn phase_eject_stun(cells_thrown: u32) -> u32 {
    PHASE_EJECT_STUN_BASE + cells_thrown
}

/// What bumping an orthogonally adjacent cell would do (§4.3) — the interaction a
/// cell offers, in the one priority order shared by execution and prediction. This
/// is the single source of truth [`State::bump_kind`] produces; `resolve_step`
/// performs the effect and `affordances` labels it, so the usable line can never
/// drift from the bump (§11.4). Purely a classification: it carries the target's
/// interaction, never a mutation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum BumpKind {
    /// A guard; `aware` is whether it detected the player this turn (§7.2).
    /// Unaware, the bump is the takedown; aware, it is a free no-op.
    Guard { aware: bool },
    /// The body currently being dragged: bumping it lets go — free (§4.4).
    /// Taking hold is not a bump — a body is non-solid and you grab it by
    /// walking over it and off its cell (§8.3, the [`BumpKind::Move`] arm).
    BodyRelease,
    /// An empty cupboard while dragging a body (§7.2/§10.3): the bump stows the
    /// body inside and locks the cupboard — it is no longer a hideout. A spent
    /// turn; the player stays put and their hands come free.
    DepositBody,
    /// The **exit**, answered by the run's intel gate (§4.5/§12.6, `ready` is
    /// [`exit_ready`](State::exit_ready)) — win vs. refused. Two bumps reach it since
    /// #466, and only one of them can win:
    ///
    /// - the step **off the board** from the tunnel's border cell, the one target that
    ///   is not a cell of the grid, classified by
    ///   [`way_out_kind`](State::way_out_kind) rather than by
    ///   [`bump_kind`](State::bump_kind), which only ever looks at cells;
    /// - the mouth `E` bumped short of the gate, which refuses at the near end rather
    ///   than sending the player down a crawl that will refuse at the far one. A bump
    ///   on `E` *with* the gate met is [`EnterExitDuct`](BumpKind::EnterExitDuct): the
    ///   climb in, not the win.
    Exit { ready: bool },
    /// The exit `E` bumped from the facility side **with the intel gate met**
    /// (§4.5/§10.7/#466): the inner mouth of the player's own tunnel, climbed into
    /// exactly as any duct entry is. Distinct from [`EnterDuct`](BumpKind::EnterDuct)
    /// only in what the usable line calls it — one of the two is a shortcut and the
    /// other is the way home, and the row that tells you what a bump does should not
    /// read the same for both. Short of the gate the same cell is
    /// [`Exit { ready: false }`](BumpKind::Exit) — the refusal.
    EnterExitDuct,
    /// An objective console still holding its intel.
    Intel,
    /// An **equipment cache** still holding its tech (§2.2/§8.3/§14 v3/#209): bumping it
    /// salvages the ability inside — usable from this turn on, and carried by the run
    /// into every facility after this one. A spent turn, like taking intel. Once opened
    /// the same cell classifies as [`Solid`](BumpKind::Solid): there is nothing left in
    /// it, so a second bump is a free no-op (§4.4) and the usable line offers nothing —
    /// the spent console's rule over the spent crate.
    Salvage,
    /// A cache holding tech the run **already carries** (§8.3/#209): the bump refuses,
    /// free, like a refused exit — and it is named apart from [`Solid`](BumpKind::Solid)
    /// for the reason [`HingeHeld`](BumpKind::HingeHeld) is: the cell is anything but
    /// inert, and the usable line owes the player the *reason* rather than a silence
    /// they would have to work out by pressing.
    SalvageRefused,
    /// A cache the run has **no room for** (§8.3/#266): the bump opens the
    /// [exchange](crate::Exchange) — the crate offers its tech and the run picks which
    /// of the four to drop. **Free**, like the refusal it replaces: opening an offer
    /// takes nothing and changes nothing (§4.4), and it is the trade that spends the
    /// turn, one turn, exactly as a plain [`Salvage`](BumpKind::Salvage) does.
    SalvageSwap,
    /// The comms console while the radio net is still live (§7.3/§7.7): bumping it
    /// silences the net for the rest of the level — a spent turn. Once silenced the
    /// same cell classifies as [`Solid`](BumpKind::Solid): there is nothing left to
    /// switch off, so a second bump is a free no-op (§4.4) and the usable line offers
    /// nothing.
    SilenceRadio,
    /// A door cell whose bump is a real door action (open, close, or a crush-refused
    /// close). Both handles of a closed door open it — a panel and, since #148, a
    /// hinge. Only an **open panel** is *not* a door action: it classifies as
    /// [`BumpKind::Move`], the walk-through, exactly as the bump resolves.
    Door { action: DoorAction },
    /// The **hinge of a door the player's immediately preceding action opened**
    /// (#320): the close is withheld for exactly that one action, so the frame is a
    /// **dead bump** — offered to the #57 lateral shift, which rounds the player onto
    /// the open panel — instead of shutting what they just opened. Named apart from
    /// [`Solid`](BumpKind::Solid) because the cell is anything but: the door is open,
    /// the close is merely one action away, and the §11.4 usable line must go quiet on
    /// it rather than promise a *close door* the bump will not deliver. Any other
    /// hinge bump on an open door — one already open, one a guard opened, one whose
    /// mark has expired — is the ordinary [`Door`](BumpKind::Door) close (§10.4).
    HingeHeld,
    /// A closed door **panel** the player walks straight through because the
    /// **Autodoors** ability is active (§8.3/§7.6): the door opens as they step into
    /// it — no manual bump — and is armed to shut behind them. Movement, not a
    /// standalone door op, so it obeys the drag half-speed and reuses the plain
    /// [`Move`](BumpKind::Move) step; classified apart only so the executor can open
    /// the door and arm the close first. Offered only when the opened cell is
    /// walkable — a hinge, which stays solid, is a plain [`Door`](BumpKind::Door).
    AutoDoor,
    /// An empty concealment cupboard to climb into (§10.3).
    Hide,
    /// A cupboard that cannot be entered — already holding an actor, or **locked**
    /// by a body stowed inside (§7.2): a free no-op bump either way.
    HideoutBlocked,
    /// A duct entry to climb into from its mouth (§10.7): bumping it enters the
    /// crawlspace, a spent turn. Offered only when the player is not already in a
    /// duct and not dragging a body (a body cannot follow into the walls).
    EnterDuct,
    /// A crawl one cell along the duct the player is inside (§10.7): a spent turn
    /// that moves them to the adjacent path cell. Not an interaction offered on the
    /// usable line — it is movement, like [`BumpKind::Move`].
    DuctCrawl,
    /// A partial-cover table not already crouched behind (§10.3).
    Crouch,
    /// The table already crouched behind — a free bump.
    CrouchHeld,
    /// Plain enterable floor — a normal move.
    Move,
    /// Anything else solid — a wall, a pillar: a free bump (§4.4). A closed hinge is
    /// *not* here anymore — since #148 it opens the door ([`BumpKind::Door`]).
    Solid,
}

impl BumpKind {
    /// The §11.4 usable-line label for this interaction, or `None` when a bump does
    /// nothing worth offering — a guard, a solid cell, a held pose, or a close that
    /// would be refused (doors never crush, so it is never promised).
    fn affordance(self) -> Option<Affordance> {
        match self {
            BumpKind::Guard { aware: false } => Some(Affordance::Takedown),
            BumpKind::BodyRelease => Some(Affordance::ReleaseBody),
            BumpKind::DepositBody => Some(Affordance::StoreBody),
            BumpKind::Exit { ready: true } => Some(Affordance::Leave),
            BumpKind::Exit { ready: false } => Some(Affordance::ExitRefused),
            BumpKind::Intel => Some(Affordance::TakeIntel),
            BumpKind::Salvage => Some(Affordance::SalvageTech),
            BumpKind::SalvageRefused => Some(Affordance::SalvageCarried),
            BumpKind::SalvageSwap => Some(Affordance::SalvageSwap),
            BumpKind::SilenceRadio => Some(Affordance::SilenceRadio),
            BumpKind::Door {
                action: DoorAction::Opened,
            }
            // Autodoors opens as you step through — the usable line still reads
            // "open", truthfully, since that is what the step does to the door.
            | BumpKind::AutoDoor => Some(Affordance::OpenDoor),
            BumpKind::Door {
                action: DoorAction::Closed,
            } => Some(Affordance::CloseDoor),
            BumpKind::Hide => Some(Affordance::Hide),
            BumpKind::EnterDuct => Some(Affordance::EnterDuct),
            BumpKind::EnterExitDuct => Some(Affordance::EnterExit),
            BumpKind::Crouch => Some(Affordance::Crouch),
            // A crawl is movement, not an offered interaction (§10.7) — like a plain
            // Move it shows nothing on the usable line.
            BumpKind::Guard { aware: true }
            | BumpKind::Door {
                action: DoorAction::Obstructed,
            }
            // The withheld frame (#320) offers nothing: while the suppression stands
            // the bump will not close the door, so the line must not say it will.
            | BumpKind::HingeHeld
            | BumpKind::HideoutBlocked
            | BumpKind::DuctCrawl
            | BumpKind::CrouchHeld
            | BumpKind::Move
            | BumpKind::Solid => None,
        }
    }
}

/// How the player perceives a guard this frame (§9.2) — the two states of a
/// perceived guard, and the gap between them is the whole §9 design. A guard the
/// player perceives at all is in exactly one; a guard in neither is out of reach and
/// [`perceive_guard`](State::perceive_guard) returns `None`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GuardPerception {
    /// In the player's field of view, line of sight clear (§6): the full threat —
    /// glyph in its state colour, facing, vision cone, and the danger overlay (§11.5).
    Seen,
    /// Within the guard-sense box ([`PLAYER_SENSE_RANGE`]), through walls, but **not**
    /// in the player's FOV: a bare position marker — the exact cell, nothing about
    /// where it is looking (§9.2). Never carries a danger overlay.
    Sensed,
}

/// Whether the run is still going, and if not, how it ended (§4.5).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
    /// The run is live; the player may act.
    Playing,
    /// Objectives taken and the exit reached.
    Won,
    /// A guard walked into the player.
    Lost,
}

/// One objective: an intel console and whether it has been taken. How many of them
/// the exit demands is the run's gate ([`exit_ready`](State::exit_ready), §4.5/§12.6),
/// not a fixed rule — all of them in quick play, one in the sim, none in campaign.
#[derive(Clone, Copy, Debug)]
struct Objective {
    cell: Cell,
    taken: bool,
}

/// One of the facility's **equipment caches** (§2.2/§8.3/§14 v3/#209): where the crate
/// stands, what is in it, and whether it has been opened.
///
/// **What it holds is decided before the level boots**, by
/// [`cache_contents`](crate::cache_contents) over the facility seed alone — so a facility
/// handed to somebody else holds the same crates, and a run that already carries this
/// tech is simply out of luck (#209). Once taken the crate stays on the state as spent
/// scenery: the renderer recolours it Neutral like a used console (§11.2), and what it
/// gave is on the loadout, not here.
#[derive(Clone, Copy, Debug)]
struct Cache {
    cell: Cell,
    holds: AbilityId,
    taken: bool,
}

/// The running game: the world, the actors on it, the objectives, and the outcome.
///
/// Plain structs, not an ECS (§12.3). The level owns its layout, its player, and its
/// guards directly, so the coupling between them is visible in the types.
#[derive(Clone, Debug)]
pub struct State {
    layout: Layout,
    player: Cell,
    facing: Direction,
    /// The player's field of view, recomputed every sight phase (§4.2/§6).
    player_fov: VisibleSet,
    /// Tile memory (§11.5a): the running union of every FOV the player has ever
    /// had, absorbed each sight phase. Monotonic — a cell once seen stays seen for
    /// the whole run — and deterministic, since it is derived purely from the FOV
    /// sequence. The fog renderer reads it to decide which *contents* are
    /// remembered; live state never consults it (§11.5a keeps those apart).
    memory: VisibleSet,
    /// Which duct the player is **inside**, as an index into [`Layout::ducts`], or
    /// `None` on open floor (§10.7). This *is* the "in a duct" state — it must be
    /// stored, not derived from position, because a duct's interior may now cross
    /// room and corridor **floor** (§10.7 cross-room routing): those interior cells
    /// are ordinary walkable floor to a player who is not crawling, so the cell alone
    /// can no longer tell "walking the room" from "crawling the duct over it". Set
    /// when the player bumps a mouth to climb in ([`EnterDuct`](BumpKind::EnterDuct))
    /// and cleared when they step out ([`Move`](BumpKind::Move)); derived purely from
    /// those transitions, so it stays deterministic (§12.4).
    in_duct: Option<usize>,
    /// Whether the player has **set foot in the facility** this run (§4.5/#466) — false
    /// only during the opening crawl, before they first climb out of their own tunnel.
    ///
    /// It gates one thing: whether the §11.4 usable line offers the **way out**. On the
    /// border cell the row would otherwise open every run by pointing at the way home,
    /// which is the one thing turn one is not about — you have not been in yet. The gate
    /// is the *predictor's* alone, exactly like the FOV gate beside it: the bump itself
    /// still answers (§4.5's refusal), so a player who presses outward anyway is told
    /// why, and nothing the row does say has changed.
    ///
    /// Set the moment a step lands the player on facility floor
    /// ([`walk_into`](Self::walk_into) — a crawl is not one), and true from the start for
    /// a state that begins outside a duct, which is every hand-built fixture.
    entered_the_facility: bool,
    /// Whether the last **spent** turn was a Wait — which widens the next sight
    /// computation to the full 360° (§8.3). A free action (a wall bump) spends
    /// nothing and changes nothing (§4.4), so it does not clear this.
    waited: bool,
    /// Whether the player's cell changed on the last spent turn — the fact
    /// Camouflage reads (§8.3: undetectable **while you don't move**; the turn
    /// you move, you are revealed). Derived in [`step`](Self::step) from the
    /// position itself, never per-arm bookkeeping, so a sprint's extra cell, a
    /// hideout entry, or a stationary interaction (a bump, a grab, a wait) can
    /// never be misclassified. Free actions leave it alone (§4.4).
    moved_this_turn: bool,
    /// Turns of **stun** left (§8.3/#329): how many more inputs are swallowed as
    /// stunned passes after Dephase threw the player clear of a solid. Set to
    /// [`phase_eject_stun`] of the throw's length by the eject, and decremented once
    /// per spent turn,
    /// at the same end-of-turn beat as the ability clocks — so the eject's own turn
    /// (already spent by the action that ran the duration out) is not one of them and
    /// the player loses exactly that many turns of agency. Zero is the ordinary
    /// state: nothing else in the game writes it.
    stunned: u32,
    /// The table the player is crouched behind (§10.3), set by bumping it and
    /// cleared by any spent action other than a Wait (waiting holds the pose).
    /// Always orthogonally adjacent by construction: the bump that sets it is a
    /// bump into an adjacent cell, and every action that could move the player
    /// clears it.
    crouched_behind: Option<Cell>,
    /// The cupboard the player climbed into **this turn**, or `None` — the entry-turn
    /// signal the §15 Q5 witness check reads. Set in the [`BumpKind::Hide`] arm of
    /// phase 1 and consumed by [`guard_phase`](Self::guard_phase) the same turn; reset
    /// at the head of every [`step`](Self::step), so a guard only ever witnesses a dive
    /// on the turn it actually happens (a guard that later aims its cone at an occupied
    /// cupboard did not see the player go in, and never gains the flush, §10.3).
    entered_hideout: Option<Cell>,
    /// The door **the player's immediately preceding action opened from its hinge**
    /// (#320/#148), or `None`. It suppresses the very next frame bump on that door:
    /// bumping the hinge again would otherwise shut what you just opened, so while
    /// the mark stands the frame reads as a dead bump ([`BumpKind::HingeHeld`]) and
    /// is offered to the #57 lateral shift instead — you round the frame onto the
    /// open panel rather than undoing the open. Written only by phase 1's own door
    /// arm, so the world's doors (a guard's open, the #147 auto-close, the Autodoors
    /// step) never set it, and expired by
    /// [`expire_frame_bump_mark`](Self::expire_frame_bump_mark) at the end of every
    /// action, free or spent — the smallest window that fixes the catch and keeps
    /// "bump a hinge to close" one action away at all times. Derived purely from the
    /// input stream, so a replay reproduces it (§12.4).
    door_just_opened: Option<DoorId>,
    guards: Vec<Guard>,
    /// The bodies takedowns have left (§7.2) — solid entities the level owns
    /// (§12.3), each remembering its guard's post for the radio net (§7.3).
    /// Only ever appended, so an index into it is stable for the run.
    bodies: Vec<Body>,
    /// The body being dragged (§8.3), as an index into [`bodies`](Self::bodies)
    /// — `None` when the player's hands are free. Set by bumping a body (the
    /// grab, a spent turn); cleared by bumping it again (free, §4.4) or never.
    /// One body at a time.
    dragging: Option<usize>,
    /// The live decoy's cell (§8.3, #105), if one is out. At most one — the
    /// economy already guarantees it (duration 20 < cooldown 30, so a second
    /// activation can never overlap the first) — and its lifetime *is* the
    /// ability's active window, both ways: expiry and early toggle-off remove
    /// it, and being stepped on ends the ability into its full cooldown.
    decoy: Option<Cell>,
    /// The **half-speed convention** (§8.3: "you move at half speed while
    /// dragging"), documented here: a successful move while dragging leaves a
    /// haul debt, and the next spent turn pays it — a Step under debt is spent
    /// but stationary, and a Wait or an activation absorbs it too (resting
    /// counts; the §8.2 timing stays exact: N moves cost 2N spent turns at
    /// worst, and every turn is a real, guard-advancing turn). Free actions
    /// (§4.4) touch neither the debt nor anything else.
    drag_debt: bool,
    /// Per-ability economy runtime (§8.2): activation, duration, and cooldown for
    /// each activated ability, stepped by the turn loop. The v1 set is available
    /// from the start (§8.3/#104), so this begins all-ready.
    abilities: Deck,
    objectives: Vec<Objective>,
    /// The facility's comms console (§7.3/§7.7), or `None` for a facility that has
    /// none — a hand-built test state, which plays the unmodified radio game exactly
    /// as before. Derived once in [`new`](Self::new) from the stamped grid rather than
    /// taken as a parameter, so no boot path has to remember to pass it and the cell
    /// can never disagree with the terrain.
    comms_console: Option<Cell>,
    /// The facility's equipment caches (§2.2/§14 v3/#209), in placement order — empty on
    /// a facility that hides none, which is every quick-play level and every hand-built
    /// test state. Set at boot by [`with_caches`](Self::with_caches), which is where the
    /// crates' cells (the generator's) and their contents (the facility's stock) meet.
    caches: Vec<Cache>,
    /// The **exchange** a crate is offering right now (§8.3/#266), or `None` — which is
    /// almost always, since it takes a full run bumping a crate to open one.
    ///
    /// World state rather than view state, and that is the load-bearing part: while it
    /// is `Some` the turn loop answers nothing but [`Input::Discard`], so the facility
    /// is genuinely *waiting* on the player rather than a picture drawn over a game that
    /// carried on underneath (§12.1's split read the other way — this changes the world,
    /// so it is not the shell's to hold). It is cleared by the answer, either one.
    exchange: Option<Exchange>,
    /// Whether the radio net has been **killed** for the rest of the level (§7.3/§7.7)
    /// — set by bumping the comms console and never cleared. One-way and permanent by
    /// construction: nothing in the loop writes `false`, so there is no window in which
    /// the net could come back and no timer to read.
    radio_silenced: bool,
    exit: Cell,
    turn: u32,
    /// The facility-wide alert (§7.3): the [`Alert`] ladder's rung and the tallies
    /// its triggers count against. Every step has a concrete source — a confirmed
    /// sighting, a post that stopped answering the radio, a tampered console, a found
    /// body — and every step **does something**, which is the whole point after the
    /// old "never written to, never read" failure (§2.3). It never decays within a
    /// level (§7.3): a rung reached is a fact about the run.
    alert: Alert,
    /// Reinforcements an escalation has called for but the turn has not yet landed
    /// (§7.3/#374), one entry per guard, each holding the **trigger cell** it will
    /// walk to and search. Filled by [`raise_alert`](Self::raise_alert) wherever the
    /// rung rises and emptied by [`land_reinforcements`](Self::land_reinforcements) at
    /// the end of the same turn's world phases — never carried, so there is no backlog.
    ///
    /// The queue exists because phase 3 resolves its per-guard readings once, up front,
    /// indexed by position in [`guards`](Self::guards): growing that vector mid-phase
    /// would leave every later pass reading past the end of its own snapshot. See
    /// [`reinforcements`] for the whole argument.
    pending_reinforcements: Vec<Cell>,
    outcome: Outcome,
    /// **Why** the run ended (§14 v2/#138), latched from the terminal event the turn
    /// it fired — `None` while the run is live, and set exactly once, since the loop
    /// is inert afterwards ([`step`](Self::step)).
    ///
    /// It is latched rather than derived because the cause is only true at the
    /// instant of contact: the capturing guard's mood (§7.4) is a live reading that
    /// the finished board no longer holds, and a screen that re-derived it would be
    /// telling the player a mistake they did not make (§2.2's traceability promise).
    ending: Option<Ending>,
    /// Fresh detections this run (§7.6) — one per [`Event::Detected`], so a chase
    /// that holds the player in sight for ten turns counts once. The §13.2 sim counts
    /// the same event the same way; the end screen reads it from here (#138).
    detections: u32,
    /// Takedowns landed this run (§7.2) — one per [`Event::TakenDown`]. Counted
    /// rather than read off [`bodies`](Self::bodies), which a stow can hide and a
    /// drag can move: the run's ledger is what the player *did*, not what is still
    /// lying about.
    takedowns: u32,
    /// The events of the player's most recent action, free or spent — what the
    /// near line reads (§11.7: messages clear on the next action, so holding
    /// exactly one action's events *is* the clearing rule). Empty before the
    /// first input; frozen once the run ends, so the final message stays.
    last_events: Vec<Event>,
    /// What the near line said **before** the current action (§11.7/#300) — a bounded
    /// ring of the last few message-bearing actions, newest first, filled in
    /// [`step`](Self::step) as each action's events stop being live. The deployed log
    /// stacks it under the current block; the near line never reads it. Empty on a
    /// fresh [`State`], so a level starts with no memory of the last one (§12.6).
    message_history: MessageHistory,
    /// The live cues of the **sense channel** (§9/§9.4, #192): everything the player
    /// has felt through a wall recently enough to still show — the cell of each guard
    /// the sense stamped this turn and the turns just before it
    /// ([`record_guard_cues`](Self::record_guard_cues)), and every door that opened or
    /// shut away from them ([`record_door_cues`](Self::record_door_cues)). One store,
    /// because they are one channel: each entry fades on its own life in
    /// [`decay_sense_cues`](Self::decay_sense_cues), and the renderer reads the union —
    /// cell plus age — through [`sense_marks`](Self::sense_marks). A small set (a
    /// handful of doors, a short trail per sensed guard), so a plain `Vec` scan is
    /// cheaper than a map.
    sense_cues: Vec<SenseCue>,
    /// The §11.5 **effect layer**: every ability effect currently made visible as a
    /// background mark, one entry per (ability, placement) (#308/#338). Lit from the
    /// turn's events in [`record_effect_marks`](Self::record_effect_marks) with the very
    /// geometry the effect resolved against, aged on the one schedule in
    /// [`decay_effect_marks`](Self::decay_effect_marks) — momentary marks count down,
    /// standing ones end with the state they report — and dropped in
    /// [`clear_effect_marks`](Self::clear_effect_marks) the moment an effect *with* a
    /// window ends either way (§8.2 expiry, §4.4 toggle-off). A handful of entries at
    /// most, so a plain `Vec` scan beats a map.
    effect_marks: Vec<EffectMark>,
    /// Doors the **Autodoors** ability (§8.3/§7.6) opened in the player's path and
    /// still owes a close-behind, as [`DoorId`]s. A door is armed here the turn the
    /// player steps through it ([`BumpKind::AutoDoor`]) and swings shut — via the
    /// §10.4 crush-safe close ([`close_armed_autodoors`](Self::close_armed_autodoors))
    /// — on the first world turn its throat is clear of the player and any dragged
    /// body, then drops from the set. **Both** door kinds are armed: a manual door has
    /// no self-close, and an automatic one would otherwise dawdle open for its full
    /// `delay` (§10.4/#147) — too slow for the flight edge, so the ability shuts it
    /// promptly too. A small set — a player passes through one door at a time — so a
    /// plain `Vec` scan beats a map.
    autodoors_pending: Vec<DoorId>,
    /// The run's seeded random source (§12.4), carried through the turn loop for the
    /// two stochastic guard decisions: a Calm guard's chance to close a door behind
    /// itself (§10.4/#146) and its chance to dwell on reaching a patrol destination
    /// (§7.5/§153). It is the *continuation* of the generation
    /// stream — the same single seed the level was carved from — threaded in via
    /// [`with_rng`](Self::with_rng), never a fresh source (§12.4 rule 1). A state built
    /// without one keeps a fixed default stream, which is all a test that never
    /// exercises the close needs; the real game and the sim thread the run seed.
    rng: Rng,
    /// The percentage chance a Calm guard closes a hinged door behind itself
    /// (§10.4/§7.6), out of 100 — the playtest knob (§7.6 warns against always).
    /// Defaults to [`GUARD_CLOSE_CHANCE_PERCENT`]; `0` disables the close entirely
    /// (and draws no RNG, so it perturbs nothing), `100` always closes.
    close_chance: u32,
    /// The percentage chance a Calm guard dwells on reaching a patrol destination
    /// (§7.5 dwell, §153), out of 100 — the playtest knob paired with the takedown
    /// tickets. Defaults to [`GUARD_DWELL_CHANCE_PERCENT`]; `0` disables dwelling
    /// entirely (and draws no RNG, so it perturbs nothing), `100` always dwells.
    dwell_chance: u32,
    /// Whether the auto lateral-shift past an obstacle is on (§57/#57) — the
    /// runtime kill-switch for the traversal experiment. Defaults to
    /// [`AUTO_SLIDE_DEFAULT`](traversal::AUTO_SLIDE_DEFAULT) (on); `false` makes
    /// every dead bump the free §4.4 no-op again, so the feature can be disabled
    /// for a playtest — or wholesale — without touching the slide logic.
    auto_slide: bool,
    /// The level modifiers active for this facility (§12.6) — the one resolved
    /// value guards, vision, and render branch on, threaded in at boot by
    /// [`with_modifiers`](Self::with_modifiers). Defaults to the baseline (every
    /// modifier off), so a hand-built state and every current run play the
    /// unmodified game; a mode preset (#244) or a campaign source (#210) resolves
    /// a non-default set through [`ModifierSources`](crate::ModifierSources).
    modifiers: LevelModifiers,
    /// The **debug** modifiers this build was baked with (§12.6) — playtest-only
    /// switches over what the *player perceives*, threaded in by
    /// [`with_debug`](Self::with_debug) and read in the sight phase
    /// ([`recompute_sight`](Self::recompute_sight)). Deliberately *not* part of the
    /// [`LevelSeed`] above: no generation seam reads them and no shared token can
    /// carry them, and they never touch the facility or the guards, so a run under one
    /// plays exactly the run it plays without one. Defaults to all off — the game as
    /// everybody else gets it.
    debug: DebugModifiers,
    /// The run's reproducible starting config (§12.4/#245), threaded in at boot by
    /// [`with_level`](Self::with_level) — the one handle that reproduces *this* run
    /// exactly, which the help panel shows (#272). `None` for a hand-built state,
    /// which was assembled cell by cell and has no seed to reproduce it from. It is
    /// the **starting** config and never changes with play: a loadout the run later
    /// grows by salvaging tech (§8.3) belongs to the run, not to the token that
    /// boots it.
    level: Option<LevelSeed>,
}

impl State {
    /// Assemble a level and run the startup turn (§4.2).
    ///
    /// The objective cells are stamped as intel consoles and the exit as the exit
    /// tile (§10.3) so the loop's bump interactions meet solid, distinctly-typed
    /// terrain. Real levels get this from placement (#12); a hand-built state does it
    /// here. `facing` is the player's initial facing (it changes only by moving, §5).
    ///
    /// One full turn — sight, then guards — runs before this returns, so the first
    /// [`step`](Self::step) already faces settled guards (§4.2).
    ///
    /// **The run opens inside the tunnel** (§4.5/§10.7/#466). When `player` is the
    /// way-out cell of the layout's exit duct — which is where placement puts it — the
    /// state starts *in* that duct, facing along it into the facility: concealed,
    /// contact-safe, perceiving only the shortened sense (§10.7), and a few crawl steps
    /// from the mouth at `E`. The first inputs of the run are that crawl, and the peek
    /// out of the mouth is what reads the room before the player climbs out.
    ///
    /// That replaced the **opening look** (#383), where the first frame was computed as
    /// if the previous turn had been a Wait — a free 360° arc and the widened sense, to
    /// show a player the room they had materialised in. Nobody materialises any more, so
    /// the exemption has nothing left to paper over: the opening posture is now an
    /// ordinary consequence of §10.7, and the player looks where they chose to look.
    pub fn new(
        mut layout: Layout,
        player: Cell,
        facing: Direction,
        guards: Vec<Guard>,
        objectives: impl IntoIterator<Item = Cell>,
        exit: Cell,
    ) -> Self {
        let objectives: Vec<Objective> = objectives
            .into_iter()
            .map(|cell| {
                layout.place(cell, Terrain::Console);
                Objective { cell, taken: false }
            })
            .collect();
        layout.place(exit, Terrain::Exit);
        // The comms console (§7.3/§7.7), stamped here with the other solid usables
        // rather than by the generator, so the carve stays bare for the §10.5/§10.6
        // work that runs on it. Placement recorded the cell on the layout, so no boot
        // path has to pass it in.
        if let Some(cell) = layout.comms_console() {
            layout.place(cell, Terrain::CommsConsole);
        }
        // The equipment caches (§2.2/§14 v3/#209), stamped here with the other solid
        // usables and for the same reason. Only the *crates* land here: what is in them
        // arrives with the stock ([`with_caches`](Self::with_caches)), and a state that
        // never gets one plays a facility whose crates are scenery — which is why the two
        // halves are not allowed to drift apart, and why `with_caches` pairs its list
        // against the layout's rather than being told the cells a second time.
        for &cell in layout.equipment_caches().to_vec().iter() {
            layout.place(cell, Terrain::EquipmentCache);
        }
        // One source of truth for the cell: the grid. Reading it back also picks up a
        // hand-built fixture that stamped its own console without a placement.
        let comms_console = layout.facility().find(Terrain::CommsConsole);

        // The run begins **inside the tunnel** (§4.5/§10.7/#466), on its way-out cell at
        // the border — the one spawn that is not on the floor. Derived from the layout
        // rather than passed in, so no boot path has to remember it, and narrow on
        // purpose: only the way-out cell starts a player crawling, never an interior
        // cell a duct's path merely overlies (which is ordinary floor to anyone walking
        // it, §10.7). A hand-built fixture with no exit duct starts on foot, as before.
        // (Only the exit tunnel has a way out at all, so matching on it is the whole
        // test — no need to ask which duct is which.)
        let in_duct = layout
            .ducts()
            .iter()
            .position(|duct| duct.way_out() == Some(player));
        // Facing **into the facility**: along the tunnel, the way the crawl runs. The
        // caller's `facing` is what a player standing on the floor was given, and there
        // is nothing to see behind you in a crawlspace anyway (§10.7).
        let facing = in_duct
            .and_then(|i| {
                let cells = layout.ducts()[i].cells();
                Direction::between(cells[cells.len() - 1], cells[cells.len() - 2])
            })
            .unwrap_or(facing);
        debug_assert!(
            layout
                .exit_duct()
                .is_none_or(|duct| duct.cells()[0] == exit),
            "the exit tunnel must start at the exit it is the way out of (§4.5)"
        );

        let mut state = Self {
            layout,
            player,
            facing,
            player_fov: VisibleSet::default(),
            memory: VisibleSet::default(),
            in_duct,
            // A run that opens in the tunnel has not been inside yet (§4.5/#466); every
            // other state — a hand-built fixture, a scene staged mid-run — begins on the
            // floor and so begins having been.
            entered_the_facility: in_duct.is_none(),
            // The run opens with the **ordinary** posture — §5's half-disc and the plain
            // sense box. The free 360° opening look (#383) is gone with #466: it existed
            // to show a player the room they had materialised standing in, and nobody
            // materialises any more. You crawl in, you peek out of the mouth (§6.1/§10.7)
            // and you climb out looking where you chose to look — which is a decision,
            // where the old rule handed out a fact.
            waited: false,
            moved_this_turn: false,
            stunned: 0,
            crouched_behind: None,
            entered_hideout: None,
            door_just_opened: None,
            guards,
            bodies: Vec::new(),
            dragging: None,
            decoy: None,
            drag_debt: false,
            // A hand-built state holds the **innate** set and nothing else (§8.3):
            // loadouts are built up from empty, never carved down from everything,
            // so a state that uses salvaged tech says which tech it has
            // ([`with_loadout`](Self::with_loadout)) instead of inheriting the lot.
            // That matters most for a passive, which reshapes perception for the
            // whole run (§8.3/#265) — never something to acquire by default.
            abilities: Deck::new(Loadout::innate()),
            objectives,
            comms_console,
            // Filled by [`with_caches`](Self::with_caches): `State::new` stamps the
            // crates (below) but cannot say what is in them, because the stock is drawn
            // at boot and a bare state has none.
            caches: Vec::new(),
            exchange: None,
            radio_silenced: false,
            exit,
            turn: 0,
            alert: Alert::new(),
            pending_reinforcements: Vec::new(),
            outcome: Outcome::Playing,
            ending: None,
            detections: 0,
            takedowns: 0,
            last_events: Vec::new(),
            message_history: MessageHistory::default(),
            sense_cues: Vec::new(),
            effect_marks: Vec::new(),
            autodoors_pending: Vec::new(),
            // A fixed default stream until [`with_rng`](Self::with_rng) threads the
            // run seed. The startup world phase below draws nothing — a guard cannot
            // have passed through a door before it has taken a step — so setting the
            // real stream after construction observes the identical stream position.
            rng: Rng::new(0),
            close_chance: GUARD_CLOSE_CHANCE_PERCENT,
            dwell_chance: GUARD_DWELL_CHANCE_PERCENT,
            auto_slide: traversal::AUTO_SLIDE_DEFAULT,
            modifiers: LevelModifiers::default(),
            debug: DebugModifiers::default(),
            level: None,
        };
        // The level-start full turn (§4.2): sight and guards, no player phase.
        let _ = state.run_world_phases();
        state
    }

    /// Kill the radio net directly (§7.3/§7.7) — the seam a test uses to reach the
    /// flag without staging a comms-console bump, so a scene built for the §7.7
    /// call-ins can be replayed with the net dead and nothing else changed. The real
    /// game only ever sets it through the bump ([`BumpKind::SilenceRadio`]).
    #[cfg(test)]
    pub(crate) fn silence_radio_for_test(&mut self) {
        self.radio_silenced = true;
    }

    /// Thread the run's seeded random source into the state (§12.4) — the
    /// continuation of the very stream the level was generated from, so a whole run
    /// is one seed end to end. The loop uses it for the guard close-behind roll
    /// (§10.4/#146) and the patrol dwell roll (§7.5/§153); everything else in the
    /// loop stays deterministic without it. The
    /// real game and the headless sim call this; a test that never exercises the
    /// close can rely on the fixed default set in [`new`](Self::new).
    pub fn with_rng(mut self, rng: Rng) -> Self {
        self.rng = rng;
        self
    }

    /// Run the facility alert on `tuning` instead of the shipped §7.3 **[START]**
    /// thresholds (#376) — the seam the §13.2 sim sweeps a rung's difficulty through
    /// without a rebuild.
    ///
    /// Applied *after* [`new`](Self::new)'s startup turn, which is safe by
    /// construction: placement guarantees no guard's turn-one cone covers the spawn
    /// (§10.6), so the startup turn tallies no contact, and the tuning is written onto
    /// the ladder rather than replacing it — there is no tally for this to discard,
    /// and the sliding window is re-pruned against the live thresholds on the next
    /// turn regardless.
    ///
    /// Like [`with_debug`](Self::with_debug) this is **not** part of the
    /// [`LevelSeed`]: no shared token can carry it, so a swept run is an instrument
    /// reading rather than a game a player could be handed. A state built without it
    /// plays the shipped ladder, which is every real run.
    #[must_use]
    pub fn with_alert_tuning(mut self, tuning: AlertTuning) -> Self {
        self.alert.set_tuning(tuning);
        self
    }

    /// Thread the facility's resolved [`LevelModifiers`] into the state (§12.6) —
    /// the one value the guard search (§7.6) and the danger overlay (§11.5) read,
    /// resolved once at facility start from its sources
    /// ([`ModifierSources::resolve`](crate::ModifierSources::resolve)). A state
    /// built without this keeps the baseline (every modifier off), which is all a
    /// test of the unmodified game needs; the real game and the sim resolve and
    /// thread the set at boot.
    #[must_use]
    pub fn with_modifiers(mut self, modifiers: LevelModifiers) -> Self {
        self.modifiers = modifiers;
        // The startup turn (§4.2) has already run its sight phase by the time a
        // builder is called, so re-run it here — `calm_guards_detect_only_their_cone`
        // (#410) is a **sight** rule, and without this the opening frame would carry
        // turn-zero cones cast under the other arm: the danger overlay would lie
        // about the flank, and a first-turn flank takedown would be refused by a
        // stale reading. Sight is a pure recompute (no RNG, §12.4), so running it
        // twice costs a frame's work and changes nothing — the same reasoning, and
        // the same call, as [`with_debug`](Self::with_debug).
        self.recompute_sight();
        self
    }

    /// The level modifiers active for this facility (§12.6), for the systems that
    /// branch on them and for a test to assert what was resolved.
    #[must_use]
    pub fn modifiers(&self) -> LevelModifiers {
        self.modifiers
    }

    /// Thread this build's [`DebugModifiers`] into the state (§12.6) — the
    /// playtest-only perception switches, set by a baked build and by nothing else (a
    /// level-seed token cannot carry them). Separate from
    /// [`with_level`](Self::with_level) on purpose: the level is what the run *is*,
    /// and these are only how much of it whoever is watching gets to see.
    #[must_use]
    pub fn with_debug(mut self, debug: DebugModifiers) -> Self {
        self.debug = debug;
        // The startup turn (§4.2) has already run its sight phase by the time a
        // builder is called, so re-run it here for the switches that shape sight —
        // otherwise the reveal would only take hold from the player's first action,
        // and the opening frame would still be fogged. Sight is a pure recompute (no
        // RNG, §12.4), so running it twice costs a frame's work and changes nothing.
        self.recompute_sight();
        self
    }

    /// The debug modifiers in force (§12.6) — read in the sight phase, and nowhere in
    /// the rules.
    #[must_use]
    pub fn debug(&self) -> DebugModifiers {
        self.debug
    }

    /// Flip the **omni-vision** switch on a running game (§12.6/#459) — the live half
    /// of [`with_debug`](Self::with_debug), for the debug session's `omni [v]` control.
    ///
    /// A mid-run flip is safe for exactly the reason the baked switch is: it is applied
    /// in the sight phase and read by no rule, so the recompute it triggers changes
    /// what the player perceives and nothing else. Sight is pure (no RNG, §12.4), so
    /// this consumes nothing from the run's stream and the run replays identically
    /// whether it was flipped or not — the property the panel's control rests on, and
    /// the one the tests pin rather than assume.
    ///
    /// It costs no turn (§4.4): the world does not step, no guard moves, and the frame
    /// after is the frame before with more (or less) of it visible. What was seen while
    /// it was on stays **remembered** afterwards (§11.5a), because memory accumulates
    /// from sight like it does for any cell — switching it off is not a way to unsee.
    pub fn toggle_reveal(&mut self) {
        self.debug.reveal_whole_level = !self.debug.reveal_whole_level;
        self.recompute_sight();
    }

    /// Thread the run's whole [`LevelSeed`] into the state (§12.4/#245) — the boot
    /// path's one call, replacing separate
    /// [`with_modifiers`](Self::with_modifiers) /
    /// [`with_loadout`](Self::with_loadout) calls: it records the config *and*
    /// applies its two halves, so the token the help panel shows (#272) and the
    /// rules the run actually plays under cannot disagree. A hand-built state simply
    /// omits it and keeps the baseline.
    #[must_use]
    pub fn with_level(mut self, level: LevelSeed) -> Self {
        self.level = Some(level);
        self.with_modifiers(level.modifiers)
            .with_loadout(level.abilities)
    }

    /// The run's reproducible starting config (§12.4/#245), or `None` for a
    /// hand-built state. [`LevelSeed::encode`] turns it into the shareable
    /// level-seed token the help panel displays (#272).
    #[must_use]
    pub fn level(&self) -> Option<LevelSeed> {
        self.level
    }

    /// Thread the run's resolved ability **loadout** into the state (§8.3/#244) —
    /// the set of economy abilities the player actually holds. Quick play grants
    /// the innate set plus a seeded draw of tech, a campaign accumulates its set
    /// (§2.2); both resolve to a [`Loadout`] carried in the shareable level-seed
    /// string (#245). A state built without this keeps the full loadout (every
    /// ability), which is all a test of the whole set needs; the real game and the
    /// sim resolve and thread a preset's loadout at boot. Set before the first
    /// [`step`](Self::step): a fresh deck is all-ready, so this only chooses which
    /// abilities exist, never their live state.
    #[must_use]
    pub fn with_loadout(mut self, loadout: Loadout) -> Self {
        self.abilities = Deck::new(loadout);
        self
    }

    /// Thread in **what the facility's equipment caches hold** (§2.2/§8.3/§14 v3/#209):
    /// the pieces of salvaged tech a bump on each crate hands over, in the order the
    /// crates were placed.
    ///
    /// The cells come from the **layout** rather than from the caller, exactly as the
    /// comms console's does: [`new`](Self::new) stamped them from that same list, so the
    /// generator is the one source of truth for *where* and this decides only *what*. A
    /// crate with nothing paired against it is left as the scenery it is, and a stock
    /// longer than the crates it was drawn for is truncated — the two lists are zipped,
    /// so neither can promise something the other cannot show.
    ///
    /// Called by [`start_level`](crate::start_level) at boot. A hand-built fixture that
    /// stamped its own crate hands its stock the same way; there is no path that gives a
    /// crate contents without the grid having a crate on it.
    #[must_use]
    pub fn with_caches(mut self, stock: impl IntoIterator<Item = AbilityId>) -> Self {
        let cells: Vec<Cell> = if self.layout.equipment_caches().is_empty() {
            // A hand-built fixture stamps its crates and records nothing; read the grid
            // back so a fixture needs one line rather than a layout it has to build.
            self.layout.facility().find_all(Terrain::EquipmentCache)
        } else {
            self.layout.equipment_caches().to_vec()
        };
        self.caches = cells
            .into_iter()
            .zip(stock)
            .map(|(cell, holds)| Cache {
                cell,
                holds,
                taken: false,
            })
            .collect();
        self
    }

    /// The run's ability loadout (§8.3/#244) — the abilities it holds, for a test
    /// to assert what a preset resolved and for the level-seed token to carry.
    #[must_use]
    pub fn loadout(&self) -> Loadout {
        self.abilities.loadout()
    }

    /// Set the chance a Calm guard closes a hinged door behind itself, as a
    /// percentage 0–100 (§10.4/§7.6) — the playtest knob this behaviour's
    /// **[START]** value is tuned with, and the replacement for the old blanket
    /// auto-close switch. `0` turns the close off, `100` makes it certain; values
    /// above 100 saturate. Deterministic given the seed threaded by
    /// [`with_rng`](Self::with_rng).
    pub fn set_guard_close_chance(&mut self, percent: u32) {
        self.close_chance = percent.min(100);
    }

    /// Set the chance a Calm guard dwells on reaching a patrol destination, as a
    /// percentage 0–100 (§7.5 dwell, §153) — the playtest knob this behaviour's
    /// **[START]** value is tuned with, and the isolation switch a test that wants
    /// a fixed patrol cadence flips to `0`. `0` turns dwelling off (drawing no RNG,
    /// so the stream is untouched), `100` makes it certain; values above 100
    /// saturate. Deterministic given the seed threaded by [`with_rng`](Self::with_rng).
    pub fn set_guard_dwell_chance(&mut self, percent: u32) {
        self.dwell_chance = percent.min(100);
    }

    /// Turn the auto lateral-shift past an obstacle on or off (§57/#57) — the
    /// runtime kill-switch for the traversal experiment, on by default
    /// ([`AUTO_SLIDE_DEFAULT`](traversal::AUTO_SLIDE_DEFAULT)). `false` restores the
    /// plain §4.4 free bump on every dead-end step, so a playtest — or a later
    /// decision to drop the experiment — can disable it without a code change to the
    /// slide logic.
    pub fn set_auto_slide(&mut self, enabled: bool) {
        self.auto_slide = enabled;
    }

    /// Resolve one turn: player, then — only if the turn was actually spent — sight
    /// and guards (§4.2). Returns the events, in order.
    ///
    /// Once the run is over the loop is inert: a call on a finished [`State`] changes
    /// nothing and returns no events.
    pub fn step(&mut self, input: Input) -> Vec<Event> {
        if self.outcome != Outcome::Playing {
            return Vec::new();
        }
        // **An open exchange takes nothing but its answer** (§8.3/#266). While a crate is
        // offering, the only input the loop resolves is the discard: a step, a wait, an
        // activation each stop here, so the world cannot move on around a decision it is
        // waiting for.
        //
        // It returns **before** the phases *and before the message bookkeeping below*,
        // which is the part worth stating. A swallowed input is not a free action that
        // happened and reported nothing — it is an input that never reached the loop at
        // all, so it must not file the standing question into the log and replace it with
        // silence. Pressing an arrow at a crate used to wipe the very line telling the
        // player what they were being asked (§11.7).
        //
        // The rule lives **here**, in the core, and not in the shell that draws the row.
        // A shell can only make its own input path obey it; this makes every path obey
        // it — the browser, a replayed script and the §13.2 sim alike — so a run cannot
        // walk away from a half-answered crate in one of them and not the others (§12.4).
        // Non-trapping either way (§11.6): the decline is one of the four presses, so
        // there is always a way out of the offer.
        if self.exchange.is_some() && !matches!(input, Input::Discard(_)) {
            return Vec::new();
        }

        let mut events = Vec::new();
        // A fresh turn: no hideout has been climbed into yet. The §15 Q5 witness check
        // in phase 3 reads whatever the Hide bump sets below, this turn only.
        self.entered_hideout = None;
        // Phase 1. A free action (wall bump, refused exit) does not end the turn.
        let from = self.player;
        // The frame-bump mark (#320) has to survive *into* this action to be readable
        // at all — `bump_kind` consults it, and so does the §11.4 usable line between
        // turns — so it is expired at the far end of phase 1 rather than at the head
        // of the turn like `entered_hideout`. Held across the resolve, spent by it.
        let carried = self.door_just_opened;
        let spent = self.player_phase(input, &mut events);
        self.expire_frame_bump_mark(carried);

        if self.outcome == Outcome::Playing && spent {
            self.turn += 1;
            // Whether this spent turn moved the player — read straight off the
            // position, the fact Camouflage's stillness rule consumes (§8.3).
            self.moved_this_turn = self.player != from;
            // Phases 2 and 3 only happen because the player spent the turn (§4.2/§4.4).
            events.extend(self.run_world_phases());
            // Ability durations tick HERE — at end of turn, after all three phases —
            // so a freshly activated N-turn ability yields N protected turns and the
            // activation turn itself is covered (§8.2's N-yields-N−1 trap): the
            // activation ran in phase 1 and every phase this turn saw it active; only
            // now does its remaining count drop. Cooldowns, frozen through the
            // duration, drain here too, but only for now-inactive abilities. Only a
            // *spent* turn reaches this, so a free action never advances the clock.
            let mut expired = Vec::new();
            self.abilities.tick(&mut expired);
            // The guards' daze counters run on the same convention, in the same beat
            // (§8.2/#325): a guard caught by this turn's blast gets its full N turns,
            // the firing turn — every phase of which already saw it frozen — being the
            // first, and only now does its own count drop. The clock lives on the
            // guard rather than on the player, which is what makes distance stop
            // mattering once the flash has gone off.
            self.tick_guard_daze();
            let phase_ended = expired.iter().any(|&id| declares(id, Effect::Phase));
            for &ability in &expired {
                // The decoy's lifetime is its ability's active window (§8.3):
                // expiry takes the fake with it.
                if declares(ability, Effect::SpawnDecoy) {
                    self.decoy = None;
                }
                // Every seal is released with the window that placed it (§8.3/#242).
                // This is the guarantee that a temporary wall is temporary: the
                // duration is the only clock, so a door cannot stay sealed past it.
                if declares(ability, Effect::SealDoors) {
                    self.release_lockdown();
                }
                // An effect's marks live exactly as long as its window (#308/#338):
                // whatever life a mark had left dies with the effect, never after it.
                self.clear_effect_marks(ability);
            }
            events.extend(
                expired
                    .into_iter()
                    .map(|ability| Event::AbilityExpired { ability }),
            );
            // Dephase expiring somewhere a solid body cannot stand throws the player
            // clear and leaves them stunned (§8.3/#329) — the cost that keeps phasing
            // from being free, paid in turns instead of in the run. It is *not* a
            // rescue: the landing cell is drawn at random from the nearest legal ones,
            // so a phase that ends in a wall never doubles as a reliable way through
            // one, and the stun that follows is spent helpless in a patrolled
            // facility. Skipped if the run already ended this turn (a capture is its
            // own, truthful loss).
            //
            // The stun's own clock is ticked first, beside the ability clocks: this
            // spent turn was a stunned pass if anything was owed, and paying it here
            // — *before* an eject can set a fresh count — keeps the turn the player is
            // thrown clear (already spent by the action that ran the duration out)
            // from ever being one of the turns they owe.
            self.stunned = self.stunned.saturating_sub(1);
            if phase_ended && self.outcome == Outcome::Playing && !self.can_rematerialize() {
                self.eject_from_solid(&mut events);
            }
            // Latch the marks of every effect that acted this turn (§11.5/#308/#338),
            // once, at the very end of it — *after* the fade at the head of the world
            // phases, exactly the door cues' shape (§9.4), so a flash lit this turn
            // keeps its full life instead of losing a turn to the very tick that placed
            // it, and after the last phase that can still produce an effect event.
            //
            // That last part is why this sits below the eject rather than beside the
            // world phases: the safety eject resolves after the ability clocks, so an
            // `Ejected` latched any earlier would be latched from a list it was not yet
            // in and never drawn at all (#339). Running here also means a mark from an
            // ability whose window ended this very turn survives `clear_effect_marks`
            // above — which is exactly right for the eject, an event whose whole
            // occasion *is* the window ending.
            self.record_effect_marks(&events);
        }

        // The run's ledger and its ending, read off this action's events (§14 v2/#138)
        // — one place, on the way out, so nothing that can end a run or break stealth
        // has to remember to keep score at its own site.
        self.record_verdict(&events);

        // Every action replaces the near line's source, free bumps included —
        // this assignment is §11.7's "messages clear on the next action".
        //
        // The outgoing set is filed first (#300): what the near line stops showing is
        // exactly what the deployed log remembers, so the two can never disagree about
        // what a past turn said. Silent actions file nothing.
        self.message_history.record(&self.last_events);
        self.last_events = events.clone();
        events
    }

    /// Keep the run's score, and latch its ending if this action ended it
    /// (§14 v2/#138).
    ///
    /// Every number the end screen shows is either already a fact of the state (the
    /// turn count, the intel in hand, the alert rung — which only rises, §7.3, so the
    /// standing rung *is* the peak) or is counted here, from the very events the §13.2
    /// sim counts. Two readers of one vocabulary, not two tallies that merely agree.
    fn record_verdict(&mut self, events: &[Event]) {
        for event in events {
            match *event {
                Event::Detected { .. } => self.detections += 1,
                Event::TakenDown { .. } => self.takedowns += 1,
                Event::Captured { guard, state, at } => {
                    self.ending = Some(Ending::Captured { guard, state, at });
                }
                Event::Entombed { at } => self.ending = Some(Ending::Entombed { at }),
                Event::Won => self.ending = Some(Ending::Escaped),
                _ => {}
            }
        }
    }

    /// Phase 1 (§4.2). Returns whether the turn was spent (a world-changing action)
    /// or was free (a mis-input that ends nothing).
    fn player_phase(&mut self, input: Input, events: &mut Vec<Event>) -> bool {
        // Stunned (§8.3/#329): the player cannot act, so **every** input — step, wait,
        // activation, toggle-off alike — resolves as the same stunned pass. The turn
        // is spent (the world moves on around a player who cannot), and nothing else
        // changes. One rule with no exceptions on purpose: carving out the two free
        // actions (§4.4) would leave a helpless player a free poke at the world, and
        // "you cannot act" would stop being true. It is emphatically **not** a Wait:
        // the flag is cleared, so a stunned turn buys neither the 360° look nor the
        // widened sense (§8.3/§9.1) — being knocked flat is not taking careful stock
        // of the room.
        if self.stunned > 0 {
            self.waited = false;
            return true;
        }
        match input {
            // Waiting is a real action: it spends the turn where you stand (§5) —
            // and buys the 360° look-around the coming sight phase grants (§8.3).
            // It also *holds* a crouch (§10.3): the pose survives exactly the
            // turns spent holding still.
            Input::Wait => {
                self.waited = true;
                // Resting pays off any haul debt (§8.3 half-speed convention):
                // the spent turn is the cost either way.
                self.drag_debt = false;
                // A wait spent standing on a body with free hands **takes hold**
                // (§8.3/#451). The look still happens — `waited` is already set, so
                // the coming sight phase grants its 360° all the same. The turn is
                // spent either way and the look costs nothing to keep, so the verb
                // loses nothing and no new key is invented; the only consequence is
                // that you cannot wait *over* a body without picking it up, and a
                // cell off is where you stand if that is what you wanted.
                self.take_body(events);
                true
            }
            Input::Step(dir) => {
                let posture = self.crouched_behind;
                let spent = self.resolve_step(dir, events);
                // Only a *spent* action stands the player up / narrows the arc: a
                // free action changes nothing, not even posture (§4.4). Two spent
                // actions keep the pose: the crouch itself — recognisable as the
                // action that changed it — and the **crouch-walk** (§10.3): plain
                // movement that lands still hugging the anchored run, corners of
                // the bench included. Any other spent step (an interaction, a
                // move that leaves the furniture) stands the player up.
                if spent {
                    self.waited = false;
                    if self.crouched_behind == posture && !self.crouch_walked(posture, events) {
                        self.crouched_behind = None;
                    }
                }
                spent
            }
            // Activating an ability spends the turn (§4.4) — but only if it actually
            // switched on; activating an unavailable ability is a mis-input and, like
            // a wall bump, is free and changes nothing. An ability whose per-level
            // budget is spent (§8.2/#302) is that same free no-op, refused inside the
            // deck — the turn cost is untouched by the budget in either direction
            // (§4.4 stands). A real activation is a spent action other than Wait, so
            // it stands the player up and narrows the arc.
            //
            // Everything *beyond* the economy — a decoy's faced cell, Pierce Wall's
            // one wall, a lockdown's doors, a blast with somebody in it — is settled
            // up front by the one precondition ladder ([`State::aim`], §8.4/#345),
            // before the deck is touched, so a refusal spends neither the turn nor a
            // use and the resolved target is carried straight to the effect that
            // consumes it. The bar reads that same ladder to grey the entry (§11.4's
            // contextual `Unusable`), which is what stops it advertising a press that
            // cannot fire.
            Input::Activate(id) => {
                let aimed = match self.aim(id) {
                    Ok(aimed) => aimed,
                    Err(refused) => {
                        // A run that does not hold the ability presses the key in
                        // silence (§4.4/#244): there is no rule to teach about a tool
                        // that was never theirs.
                        if self.abilities.loadout().contains(id) {
                            events.extend(refused.event());
                        }
                        return false;
                    }
                };
                if self.abilities.activate(id) {
                    if let Aimed::Decoy(cell) = aimed {
                        self.decoy = Some(cell);
                    }
                    // The budget's remaining count is read *after* the deck spent
                    // it (§8.2/#302), so the message speaks what is actually left.
                    events.push(Event::AbilityActivated {
                        ability: id,
                        uses_left: self.abilities.uses_left(id),
                    });
                    match aimed {
                        Aimed::Bore(wall) => self.bore_wall(wall, events),
                        Aimed::Seal(doors) => self.seal_doors(&doors, events),
                        Aimed::Blast(blast) => self.fire_confusion(blast, events),
                        Aimed::Nothing | Aimed::Decoy(_) => {}
                    }
                    self.waited = false;
                    self.crouched_behind = None;
                    // A spent turn pays the haul debt (§8.3), like a Wait.
                    self.drag_debt = false;
                    true
                } else {
                    false
                }
            }
            // Toggling an ability off is free (§4.4): it never spends the turn, so —
            // like every free action — it leaves posture and the waited flag alone.
            // Toggling the decoy's ability off takes the decoy with it: its lifetime
            // is the active window (§8.3). Toggling Dephase off somewhere a solid
            // body cannot stand is **refused** (a free no-op): there is nowhere to
            // rematerialize — the lethal squeeze is the duration's alone (§8.3),
            // never a mis-pressed key (§2.2: every death traceable to a decision).
            // The refusal *speaks* (§11.7): now that a player can reach this key
            // (#304), a press that changes nothing has to say why.
            Input::Deactivate(id) => {
                if declares(id, Effect::Phase) && !self.can_rematerialize() {
                    // Only a phase actually running has a toggle-off to refuse; a
                    // press with nothing on stays the silent no-op it always was.
                    if matches!(self.ability_state(id), AbilityState::Active { .. }) {
                        events.push(Event::RematerializeRefused);
                    }
                    return false;
                }
                if self.abilities.deactivate(id) {
                    self.unwind_effect(id);
                    events.push(Event::AbilityDeactivated { ability: id });
                }
                false
            }
            // **Answering the exchange** (§8.3/#266). The four candidates on the bar are
            // the run's three pieces of tech and the crate's one, and this names the one
            // to discard — a trade if it is held, the decline if it is the crate's.
            //
            // A press naming anything else, or arriving with no offer open, is a
            // mis-input: free, silent, nothing changed (§4.4). The same shape as
            // activating an ability that is cooling.
            Input::Discard(id) => {
                let Some(offer) = self.exchange else {
                    return false;
                };
                match offer.resolve(self.abilities.loadout(), id) {
                    // The trade: the crate opens, the old tech goes and the new arrives
                    // on the deck ready to use this turn — the same "usable immediately"
                    // a plain salvage promises (§14 v3). **This is the one spent turn in
                    // the whole exchange**, and it is the one a plain salvage would have
                    // cost: the bump that opened the offer was free, so trading at a
                    // crate costs exactly what taking from one costs.
                    Some(Choice::Trade { dropped }) => {
                        let taken = offer.offered();
                        let at = offer.at();
                        if let Some(cache) = self.caches.iter_mut().find(|c| c.cell == at) {
                            cache.taken = true;
                        }
                        // Dropped first, then granted: the two are one exchange, and in
                        // this order the run is never momentarily over the §8.3 cap.
                        self.abilities.revoke(dropped);
                        // Whatever the dropped ability was still doing to the world goes
                        // with it — the same unwind an early toggle-off does, because
                        // that is what `revoke` just did to its slot.
                        self.unwind_effect(dropped);
                        self.abilities.grant(taken);
                        self.exchange = None;
                        events.push(Event::Traded { taken, dropped });
                        self.waited = false;
                        self.crouched_behind = None;
                        // A spent turn pays the haul debt (§8.3), like a Wait.
                        self.drag_debt = false;
                        true
                    }
                    // The decline: the crate keeps its tech and stands where it was, so
                    // a run that comes back having traded that piece away finds it
                    // unopened. Free — nothing changed (§4.4).
                    Some(Choice::Decline) => {
                        self.exchange = None;
                        events.push(Event::ExchangeDeclined {
                            id: offer.offered(),
                        });
                        false
                    }
                    None => false,
                }
            }
        }
    }

    /// Take back whatever an ability's **window** was still doing to the world, after
    /// its slot has been switched off (§8.2): the decoy it was standing up, the doors it
    /// was holding shut, the marks it had painted.
    ///
    /// One helper for the two ways a window can end early — the player's free toggle-off
    /// (§4.4/#304) and the exchange trading the ability away (#266) — so an ability that
    /// grows something to unwind cannot have it unwound on one path and left behind on
    /// the other. Expiry has its own copy in the turn loop, where the whole expired set
    /// is walked at once.
    fn unwind_effect(&mut self, id: AbilityId) {
        // The decoy's lifetime is its ability's active window (§8.3).
        if declares(id, Effect::SpawnDecoy) {
            self.decoy = None;
        }
        // The seals are the window (§8.3/#242): ending it early hands every door back
        // at once. It refunds nothing — the full lockout still runs (§8.2).
        if declares(id, Effect::SealDoors) {
            self.release_lockdown();
        }
        // The effect is gone, so its marks go with it (#308/#338) — an early end
        // leaves no residue to fade over nothing.
        self.clear_effect_marks(id);
    }

    /// Whether the spent step just resolved was a **crouch-walk** (§10.3): the
    /// pose survives only plain movement — the turn's events carry a
    /// [`Event::Moved`], so an interaction that spends the turn in place (a
    /// door, a grab, a haul-debt payment) still stands the player up — that
    /// lands still hugging the anchored run ([`cover::run_hugs`]: within one
    /// cell of any of its tables, the diagonal past a bench's end included, so
    /// the walk can round the corner). A sprinting step (§8.3 Run) is judged
    /// where it *ends*, like every other consequence of the two-cell move.
    fn crouch_walked(&self, posture: Option<Cell>, events: &[Event]) -> bool {
        let Some(anchor) = posture else {
            return false;
        };
        events.iter().any(|e| matches!(e, Event::Moved { .. }))
            && cover::run_hugs(
                &cover::cover_run(self.layout.facility(), anchor),
                self.player,
            )
    }

    /// Whether `table` belongs to the run the player is currently crouched
    /// behind (§10.3) — the "is this bump the pose I already hold" question the
    /// interaction ladder asks to keep a held re-bump free (§4.4).
    fn crouch_covers(&self, table: Cell) -> bool {
        self.crouched_behind
            .is_some_and(|anchor| cover::cover_run(self.layout.facility(), anchor).contains(&table))
    }

    /// Resolve a step into a move or a bump (§4.3), pushing the event and reporting
    /// whether the turn was spent.
    fn resolve_step(&mut self, dir: Direction, events: &mut Vec<Event>) -> bool {
        // A step **off the board** is resolved first, because it has no target cell to
        // classify. On the exit tunnel's way-out cell, aimed outward, it is the win
        // check (§4.5/#466); everywhere else it is the free mis-input a wall bump has
        // always been (§4.3) — the border is solid wall, and there is nothing beyond it.
        //
        // One ladder decides what a bump does — the same `bump_kind` the usable line
        // reads — so execution and prediction can never disagree (§11.4). The match
        // below performs the effect; the classification and its priority live in one
        // place, and the off-board step gets the one classifier that can answer for a
        // target which is not a cell. It yields only `Exit`, whose arms read no target,
        // so the player's own cell stands in for one.
        let (target, kind) = match self.player.step(dir) {
            Some(cell) if self.layout.facility().in_bounds(cell) => (cell, self.bump_kind(cell)),
            _ => match self.way_out_kind(dir) {
                Some(kind) => (self.player, kind),
                None => return false,
            },
        };

        // The half-speed drag (§8.3): a step that would *move* the player while a
        // haul debt is owed pays the debt instead — the turn is spent hauling the
        // body along, and nothing moves. Interactions (doors, grabs, the exit) are
        // not movement and stay full price; free bumps stay free.
        if self.dragging.is_some()
            && self.drag_debt
            && matches!(kind, BumpKind::Move | BumpKind::Hide | BumpKind::AutoDoor)
        {
            self.drag_debt = false;
            return true;
        }

        match kind {
            // The takedown (§7.2): adjacent, against a guard that has not detected
            // the player this turn, costing the full turn. Permanent — the guard
            // is gone, and what remains is the body, which is the real cost. No
            // cooldown and no range: the constraints *are* the cost.
            BumpKind::Guard { aware: false } => {
                let i = self
                    .guard_at(target)
                    .expect("bump_kind classified a guard here");
                let guard = self.guards.remove(i);
                // The body inherits the downed guard's radio cadence (§7.3): the
                // clock that was silent while it lived starts ticking now, its
                // first missed ping a full period out. Where it falls is control's
                // last fix on the guard, and so where a responder will be sent.
                self.bodies
                    .push(Body::new(target, guard.radio_clock(), self.turn));
                events.push(Event::TakenDown { at: target });
                true
            }
            // An aware guard has you in its cone (§7.2's gate): the bump is a
            // free no-op — no half-takedown, no shove.
            BumpKind::Guard { aware: true } => {
                events.push(Event::Bumped { into: target });
                false
            }
            // Letting the body go where it lies (§8.3): free, the §4.4 toggle-off
            // exception — and it refunds nothing, there is nothing to refund.
            BumpKind::BodyRelease => {
                self.dragging = None;
                self.drag_debt = false;
                events.push(Event::BodyReleased { at: target });
                false
            }
            // Stowing the dragged body into a cupboard (§7.2/§10.3): the body slides
            // inside and is *gone* — no cone finds it (the found-body scan skips a
            // hideout cell) — and the cupboard is now **locked**, no longer a
            // hideout. The player does not move (they stow it from the mouth); the
            // turn is spent (an interaction, not free) and their hands come free.
            BumpKind::DepositBody => {
                let i = self
                    .dragging
                    .expect("bump_kind classified DepositBody only while dragging");
                self.bodies[i].move_to(target);
                self.dragging = None;
                self.drag_debt = false;
                events.push(Event::BodyStored { at: target });
                true
            }
            // The way out (§4.5/#466): a step off the board from the tunnel's border
            // cell. Win if the intel gate is met, else refuse — a refused exit changes
            // nothing and is free (§4.5), and the refusal is the same one it always was;
            // only the cell you are standing on when you read it has moved.
            BumpKind::Exit { ready: true } => {
                self.outcome = Outcome::Won;
                events.push(Event::Won);
                true
            }
            BumpKind::Exit { ready: false } => {
                events.push(Event::ExitRefused {
                    still_needed: self.intel_needed_to_exit(),
                });
                false
            }
            // An objective console: take the intel.
            BumpKind::Intel => {
                let obj = self
                    .objectives
                    .iter_mut()
                    .find(|o| o.cell == target && !o.taken)
                    .expect("bump_kind classified an untaken console here");
                obj.taken = true;
                // Both counts are read *after* the take, and they are not the same
                // number: what is still out is the tally, what the gate still wants is
                // the requirement (#310/§4.5).
                events.push(Event::IntelTaken {
                    remaining: self.objectives_remaining(),
                    still_needed: self.intel_needed_to_exit(),
                });
                // A console tampered with while the facility already knows you are in
                // it steps the alert ladder to rung 2 (§7.3): control now knows what
                // you came for. At rung 0 it triggers nothing at all — that is the
                // reward for staying unseen, and the reason a clean raid is quiet.
                if let Some(trigger) = self.alert.console_tampered() {
                    // The console is what control learned about, so it is where any
                    // reinforcement the rung sends is sent to search (#374) — a cell
                    // the player is standing next to *now* and will have left by the
                    // time anyone walks in, which is exactly §7.6's stale lead.
                    self.raise_alert(trigger, target, events);
                }
                true
            }
            // An equipment cache (§2.2/§8.3/§14 v3/#209): salvage the tech in it. The
            // ability joins the deck **now**, not at the end of the raid — §14 v3 asks
            // for a power curve, and one that only paid out after you had left would
            // make the detour a deposit rather than a find. It joins the *run* too: the
            // campaign layer reads it off the verdict ([`RunStats::salvaged`]) and folds
            // it into the loadout every later facility boots with (§2.2).
            //
            // Deliberately **quiet**. A crate is not a terminal control room reports on,
            // so opening one raises no alert (§7.3) — the console's `console_tampered`
            // step has no counterpart here. What it costs is the detour and the turn,
            // which is the §2.3 price the placement rule (`PLAYER_CACHE_MIN_DISTANCE`)
            // is there to make real.
            BumpKind::Salvage => {
                let cache = self
                    .caches
                    .iter_mut()
                    .find(|c| c.cell == target && !c.taken)
                    .expect("bump_kind classified an unopened cache here");
                cache.taken = true;
                let id = cache.holds;
                self.abilities.grant(id);
                events.push(Event::TechSalvaged { id });
                true
            }
            // A crate holding tech the run already carries (§8.3/#209). **Free** (§4.4)
            // and the crate is left unopened, so nothing is spent and nothing is lost —
            // the refused exit's shape, and for the same reason: a bump that changes
            // nothing must cost nothing. It is the luck of a facility stocked before
            // anyone knew who was coming, and there is no decision in it: a second copy
            // of what you hold would change nothing whichever way you answered.
            BumpKind::SalvageRefused => {
                events.push(Event::SalvageRefused {
                    id: self.live_cache_at(target),
                });
                false
            }
            // A crate the run has no room for (§8.3/#266): the crate **offers**, and the
            // run is now standing at the exchange. Free, exactly as the refusal it
            // replaces was — the crate is still shut, the loadout is untouched and no
            // turn has been spent. What has changed is that the game is waiting: until
            // the offer is answered, [`player_phase`](Self::player_phase) takes nothing
            // but the discard.
            //
            // Opening it is **idempotent** in the way that matters: the offer names the
            // crate, so a second bump on the same crate re-states the same offer rather
            // than stacking a second one.
            BumpKind::SalvageSwap => {
                let id = self.live_cache_at(target);
                self.exchange = Some(Exchange::new(id, target));
                events.push(Event::ExchangeOffered { id });
                false
            }
            // The comms console (§7.3/§7.7): one bump kills the radio net for the rest
            // of the level. A spent turn — the counterplay costs the detour that got
            // you here plus this turn, and nothing else, because the flag is one-way
            // and there is no upkeep to pay.
            BumpKind::SilenceRadio => {
                self.radio_silenced = true;
                events.push(Event::CommsSilenced { at: target });
                true
            }
            // A door (§4.3, §10.4): opening or closing spends the turn. An obstructed
            // close changed nothing — free; doors never crush.
            BumpKind::Door { action } => match action {
                DoorAction::Opened => {
                    self.operate_door(target);
                    // Remember a *hinge* open (#320) so the very next bump on this
                    // frame slides past the door instead of shutting it again. Only
                    // the hinge open is marked: opening from a panel leaves the player
                    // facing an open panel they walk through, never a frame they could
                    // bump next, so there is nothing there to catch on.
                    self.door_just_opened = self.hinge_door_at(target);
                    // Frame bump (#148): opening from a *hinge* turns the player to
                    // face along the door line toward the panels, so the recomputed
                    // FOV + #121 peek leans through the doorway from beside it — the
                    // "crack the door and peek from cover" move, reading the room
                    // without ever standing in the new sightline. A §5 exception, on
                    // the same footing as the #89 hideout-entry auto-face. Opening
                    // from a panel is not a hinge bump and leaves facing to §5 (an
                    // open door is not a move).
                    if let Some(peek) = self.hinge_peek_facing(target) {
                        self.facing = peek;
                    }
                    events.push(Event::DoorOpened {
                        at: target,
                        by_player: true,
                    });
                    true
                }
                DoorAction::Closed => {
                    self.operate_door(target);
                    true
                }
                DoorAction::Obstructed => false,
            },
            // Autodoors (§8.3/§7.6): the closed door in the player's path opens as
            // they step into it — no manual bump — and the same step carries them
            // through onto the panel, saving the open-turn. The door is armed to
            // swing shut behind them once the throat clears (the world phase's
            // [`close_armed_autodoors`]). The move itself is an ordinary
            // [`walk_into`], so dragging, facing (§5) and a Run sprint compose exactly
            // as they do through an already-open door.
            BumpKind::AutoDoor => {
                self.operate_door(target); // the door was closed, so this opens it
                self.arm_autodoor_close(target);
                events.push(Event::DoorOpened {
                    at: target,
                    by_player: true,
                });
                self.walk_into(dir, target, events);
                true
            }
            // A hideout: bump the empty cupboard to climb in (§4.3, §10.3). Unlike
            // floor, you do not drift onto it — entering is a *decision*. It moves you
            // into the cell, spends the turn, and conceals you ([`hidden`](Self::hidden));
            // climbing out is an ordinary step off, no special case. Its whole cost is
            // time: while you hide you make no progress and the clock keeps ticking (§2.3).
            BumpKind::Hide => {
                self.haul_body_to(self.player);
                self.player = target;
                self.stomp_decoy(target, events);
                // The §5 exception for the hideout interaction (§7.6/§10.3): entry
                // faces *out* of the cupboard, back toward the corridor — the
                // opposite of the entry bump, which points into the wall the hideout
                // is recessed in. So the ~180° half-disc (§6.2, arc 3) watches the
                // flight path the instant you hide, not the wall behind you, and you
                // get the "hold still, watch the cone sweep" moment without wasting a
                // turn re-aiming (there is no turn-in-place, §5). This is *not* a
                // general turn-in-place: only the Hide entry sets a meaningful facing;
                // climbing back out is an ordinary step whose facing follows its own
                // direction (see `BumpKind::Move`).
                self.facing = dir.opposite();
                // Mark the entry so phase 3 can decide which guards *witnessed* it
                // (§15 Q5): an alerted guard whose cone covers this cell saw the dive.
                self.entered_hideout = Some(target);
                events.push(Event::EnteredHideout { at: target });
                true
            }
            // A duct entry: climb in from the mouth (§4.3, §10.7). Like the hideout
            // this is a *decision*, not a drift — it moves the player onto the entry
            // cell, spends the turn, and conceals them. Facing is set out the mouth
            // (back the way you came, `dir.opposite()`) so the entry-cell peek (§6.1)
            // leans through the mouth to read the room before you climb out. No body
            // can be in hand here (`bump_kind` refuses EnterDuct while dragging), so
            // there is nothing to haul.
            // The exit `E` from the facility side (§4.5/#466): climbing into the tunnel
            // you dug. Mechanically the duct entry above — the same spent turn, the same
            // concealment, the same mouth peek — so it shares its arm. What differs is
            // only what the usable line called it on the way in (`exit: enter`).
            BumpKind::EnterDuct | BumpKind::EnterExitDuct => {
                self.player = target;
                // Record *which* duct we climbed into — the entry belongs to exactly
                // one (§10.7), and from here "in a duct" is this stored index, not the
                // cell (an interior cell may overlie floor the player could also walk).
                self.in_duct = self.layout.duct_index_containing(target);
                // Facing out the mouth, read from the duct itself rather than from the
                // bump, so the stored facing and the peek that is cast from it can never
                // disagree: for a recessed entry that is `dir.opposite()` anyway, and for
                // the exit `E` — whose mouth is the whole room it comes up in — it is the
                // tunnel's own axis, the way a crawl arrives. The fallback covers a
                // hand-built duct with no derivable mouth.
                self.facing = self.duct_entry_facing(target).unwrap_or(dir.opposite());
                events.push(Event::EnteredDuct {
                    at: target,
                    own_tunnel: matches!(kind, BumpKind::EnterExitDuct),
                });
                true
            }
            // A crawl one cell along the duct (§10.7): a spent turn that moves the
            // player to the adjacent path cell, staying concealed. Landing on an
            // entry re-aims facing out its mouth (the peek); a mid-duct cell just
            // faces the crawl direction. Guards and bodies can never be inside a
            // duct, so none of the move-arm side effects (haul, decoy stomp, the
            // Run extra step) apply.
            BumpKind::DuctCrawl => {
                self.player = target;
                self.facing = self.duct_entry_facing(target).unwrap_or(dir);
                events.push(Event::DuctCrawled { to: target });
                true
            }
            // A table: bump it to crouch behind it (§4.3, §10.3). Ducking is a
            // *decision*, aimed at a specific table — and it anchors the crouch to
            // that table's whole run (the §10.1a bench), which is what conceals.
            // The player does not move; the tables stay solid furniture.
            BumpKind::Crouch => {
                self.crouched_behind = Some(target);
                events.push(Event::Crouched { behind: target });
                true
            }
            // Plain movement into a cell that admits the player — floor, an open
            // doorway, or a cell holding a *loose* body (non-solid, §7.2: you walk
            // over it).
            BumpKind::Move => {
                self.walk_into(dir, target, events);
                true
            }
            // A cupboard already holding an actor or locked by a stowed body, the
            // table already crouched behind, the frame of the door just opened
            // (#320), or anything else solid (a wall, a pillar): a free bump (§4.4).
            // A closed hinge is no longer here — it opens the door now (#148,
            // `BumpKind::Door`). Before it no-ops, the traversal experiment (#57) gets
            // a chance to read the dead bump as an unambiguous sidestep and slide one
            // cell past it; only if it declines does the bump fall through to the free
            // wall-bump it has always been — and a declined slide never closes the
            // just-opened door in the same breath, which is the whole point of #320.
            BumpKind::HingeHeld
            | BumpKind::HideoutBlocked
            | BumpKind::CrouchHeld
            | BumpKind::Solid => {
                if self.try_lateral_shift(dir, events) {
                    true
                } else {
                    events.push(Event::Bumped { into: target });
                    false
                }
            }
        }
    }

    /// Move the player one cell into `target` — the plain-move executor (§4.3/§5)
    /// shared by [`BumpKind::Move`] and, once the door is open, [`BumpKind::AutoDoor`].
    /// It hauls a dragged body into the vacated cell, faces the step (§5), takes hold
    /// of a body stepped off (§8.3), stomps a decoy underfoot, and lets a Run sprint
    /// add its extra cell — every consequence of a step, in one place so a walk
    /// through an auto-opened door behaves exactly as one through an already-open one.
    fn walk_into(&mut self, dir: Direction, target: Cell, events: &mut Vec<Event>) {
        // A plain move while inside a duct is the climb-out at a mouth (§10.7) — the
        // only Move the confinement in `bump_kind` admits — so leaving the crawlspace
        // clears the stored state. (A phase-out, the one other way a Move fires from a
        // duct cell, ends the crawl just the same.)
        self.in_duct = None;
        // And a step onto the facility's floor is the run beginning in earnest
        // (§4.5/#466): from here the usable line offers the way out, which it holds back
        // while the player has still never left the tunnel they started in.
        self.entered_the_facility = true;
        let vacated = self.player;
        self.haul_body_to(vacated);
        self.player = target;
        self.facing = dir; // facing follows the last successful step (§5)
        events.push(Event::Moved { to: target });
        // **Stepping off a body no longer takes hold** (§8.3/#451). It used to, and the
        // grab landed on the one turn you least wanted it: you could not cross a body
        // at all without picking it up, and the drag that followed cost half speed —
        // so the accident happened mid-escape, over the guard you had just put down.
        // Taking hold is now a **wait** spent standing on the body ([`take_body`]),
        // which is a turn you chose to spend. Walking over one is walking over one.
        //
        // [`take_body`]: Self::take_body
        //
        // Stepping onto your own decoy kills it (§8.3) — anything's step does, the
        // maker's included; a sprint checks its second cell too.
        self.stomp_decoy(target, events);
        self.run_extra_step(dir, events);
    }

    /// The facing a **frame bump** (#148) turns the player to: from the bumped
    /// `hinge`, the direction toward the door's panels — the cell one step *into*
    /// the doorway. Facing along the door line, the ~180° half-disc and its #121
    /// peek lean through the opening, so the player reads the room they just cracked
    /// from beside it (§6, §10.4). `None` when `target` is not a hinge — a panel
    /// open (or any non-door cell) leaves facing to §5, so the caller applies this
    /// only for the hinge case.
    fn hinge_peek_facing(&self, target: Cell) -> Option<Direction> {
        let door = self.layout.regions().door(self.hinge_door_at(target)?);
        // A door is a straight line of hinges around panels, so exactly one panel is
        // orthogonally adjacent to each end hinge: that neighbour is the way in.
        let panel = door
            .panels()
            .iter()
            .copied()
            .find(|&p| target.manhattan_distance(p) == 1)?;
        Direction::between(target, panel)
    }

    /// The direction out the **mouth** of the duct entry at `cell` (§10.7), or `None`
    /// if `cell` is not a duct entry. An entry has exactly one floor neighbour — the
    /// mouth — by its recessed geometry (§10.1.6), so this is the direction the
    /// entry-cell peek (§6.1) leans through and the facing a player landing on the
    /// entry takes, so the ~180° window watches the room before they climb out.
    fn duct_entry_facing(&self, cell: Cell) -> Option<Direction> {
        // `cell` is always on the duct the player is currently crawling (this is only
        // ever called for the occupied duct's cells), so read the mouth from that duct
        // rather than searching by position — an interior cell may overlie floor that
        // belongs to no duct's geometry.
        let duct = self.occupied_duct()?;
        if !duct.is_entry(cell) {
            return None;
        }
        // The exit `E` (§4.5/#466) is the one entry with no *recessed* mouth: it is a
        // cell of the room it comes up in, so it may have floor on several sides and
        // "the single floor neighbour" has no answer. Its mouth is the tunnel's own
        // **axis** — you come up looking the way the tunnel points, which is also the
        // direction a crawl arrives from, so the peek reads the room ahead of you.
        if duct.way_out().is_some() {
            return Direction::between(duct.cells()[1], cell);
        }
        let facility = self.layout.facility();
        let mouth = facility
            .neighbours(cell)
            .find(|&n| facility.can_enter(n, ACTOR_FILL))?;
        Direction::between(cell, mouth)
    }

    /// What a step **off the board** would do (§4.5/§10.7/#466) — the classifier for
    /// the one target that is not a cell, and the twin of [`bump_kind`](Self::bump_kind)
    /// that both [`resolve_step`](Self::resolve_step) (which executes) and
    /// [`affordances`](Self::affordances) (which labels the §11.4 usable line) read, so
    /// the row can no more promise a wrong exit than a wrong door.
    ///
    /// `Some(BumpKind::Exit { .. })` only from the exit tunnel's **way-out cell**, aimed
    /// **outward** — off the level border, the direction the tunnel points. Everywhere
    /// else, and in every other direction, a step off the grid is the free mis-input it
    /// has always been (§4.3): the border is wall, and there is nothing beyond it.
    ///
    /// **Phased, it offers nothing** (§8.3): while Dephase is up there is no bump at
    /// all — no door opens, no intel is taken, the exit does not win — and walking out
    /// through your own tunnel's wall is not an exception to that.
    fn way_out_kind(&self, dir: Direction) -> Option<BumpKind> {
        if self.abilities.effect_active(Effect::Phase) {
            return None;
        }
        let duct = self.occupied_duct()?;
        if duct.way_out() != Some(self.player) {
            return None;
        }
        // Outward is *away from the tunnel*: the way-out cell's one neighbour on the
        // path is the cell behind it, so the step that leaves the board is the opposite.
        let inward = Direction::between(self.player, duct.cells()[duct.cells().len() - 2])?;
        (dir == inward.opposite()).then_some(BumpKind::Exit {
            ready: self.exit_ready(),
        })
    }

    /// **What a bump on a crate holding `id` would do** (§8.3/#209/#266) — the three
    /// answers a live cache has, decided in this order:
    ///
    /// - **Already carried** ([`SalvageRefused`](BumpKind::SalvageRefused)). The crate
    ///   holds tech the run has. A facility is stocked from its own seed and knows
    ///   nothing of who is coming (#209), so this is luck rather than design — and a
    ///   second copy would be a turn spent on nothing. Asked **first**, because it is the
    ///   more specific answer and because it is the one case where full hands are beside
    ///   the point: there is nothing here worth trading *for*, so offering the exchange
    ///   would be offering a decision whose every branch is a loss.
    /// - **No room** ([`SalvageSwap`](BumpKind::SalvageSwap)). The run already carries
    ///   [`AbilityId::MAX_TECH_HELD`] pieces of tech, which §8.3 settles as the most a
    ///   run holds at once — it is what keeps the held set small enough for the ability
    ///   bar to name every entry on one row (§11.4), and what a passive pays with. The
    ///   cap is kept **here, at the crate**, because this is the one moment the player
    ///   can be told; what it now costs is a choice rather than the find.
    /// - **Room for it** ([`Salvage`](BumpKind::Salvage)) — the plain pickup.
    fn salvage_kind(&self, id: AbilityId) -> BumpKind {
        if self.abilities.loadout().contains(id) {
            BumpKind::SalvageRefused
        } else if self.abilities.loadout().tech_held() >= AbilityId::MAX_TECH_HELD {
            BumpKind::SalvageSwap
        } else {
            BumpKind::Salvage
        }
    }

    /// What the unopened crate at `cell` holds — for the arms that have already been
    /// told by [`bump_kind`](Self::bump_kind) that one is standing there.
    fn live_cache_at(&self, cell: Cell) -> AbilityId {
        self.caches
            .iter()
            .find(|c| c.cell == cell && !c.taken)
            .expect("bump_kind classified a live cache here")
            .holds
    }

    /// What bumping the orthogonally adjacent `target` would do (§4.3) — the **single**
    /// interaction ladder, read-only, that both [`resolve_step`](Self::resolve_step)
    /// (which executes) and [`affordances`](Self::affordances) (which labels the §11.4
    /// usable line) consume. Naming the interaction in one place is what keeps the
    /// arrow labels from ever promising what a bump won't deliver.
    ///
    /// The arms below are in priority order — exit → intel → door → hideout → table →
    /// move → bump. A new interaction is added as one [`BumpKind`] variant classified
    /// here; Rust's exhaustive matching then forces *both* consumers — the executor in
    /// `resolve_step` and the label in [`BumpKind::affordance`] — to handle it, so
    /// neither can silently drift (the §7.2 takedown slots in exactly this way). This
    /// classifies only; the mutation lives in `resolve_step`, so it can stay `&self`.
    fn bump_kind(&self, target: Cell) -> BumpKind {
        // Dephase (§8.3, [`Effect::Phase`]): while phased there *is* no bump —
        // every in-bounds cell is a plain move, walls, doors, furniture, guards
        // and bodies included (the player's fill is effectively 0, §4.3). This
        // one short-circuit is the whole "cannot bump" rule: no door opens, no
        // intel is taken, the exit does not win, no takedown, no grab, no climb
        // — you pass straight through everything you came for. And because the
        // usable line reads this same ladder, it truthfully offers nothing
        // while phased (§11.4).
        if self.abilities.effect_active(Effect::Phase) && self.layout.facility().in_bounds(target) {
            return BumpKind::Move;
        }
        // Inside a duct the player is confined to the crawlspace (§10.7): a step to
        // the next path cell crawls, a step off an **entry** onto its floor mouth
        // climbs out (a plain Move), and every other step is walled in — a solid
        // bump. This confinement is what keeps an interior duct cell that happens to
        // touch floor from ever being an unintended exit; you leave only at a mouth.
        if let Some(duct) = self.occupied_duct() {
            if duct.is_crawl_step(self.player, target) {
                return BumpKind::DuctCrawl;
            }
            // The only way off the path is stepping from an **entry** onto its floor
            // mouth. An interior cell that happens to overlie floor (§10.7 cross-room
            // routing) is never an exit — only an entry's mouth is, which the recessed
            // geometry keeps as the entry's single non-solid neighbour.
            if duct.is_entry(self.player) && self.layout.facility().can_enter(target, ACTOR_FILL) {
                return BumpKind::Move; // climb out through the mouth
            }
            return BumpKind::Solid;
        }
        if let Some(i) = self.guard_at(target) {
            return BumpKind::Guard {
                aware: self.guard_detects_now(&self.guards[i]),
            };
        }
        // The body **in hand** is the one interaction a body offers: bump it to let
        // go (§8.3). A body is otherwise non-solid (§7.2) — a *loose* body is not
        // caught here; it falls through to a plain move (you walk over it, and grab
        // it by stepping *off* its cell, the [`BumpKind::Move`] arm). Stowing a body
        // into a cupboard is the Hideout arm below, not a bump on the body itself.
        if let Some(held) = self.dragging {
            if self.bodies[held].cell() == target {
                return BumpKind::BodyRelease;
            }
        }
        // The exit `E` from the facility side (§4.5/#466): the inner mouth of the
        // player's own tunnel.
        //
        // **The intel gate is answered here**, at the mouth, not at the far end of the
        // crawl: short of it the bump refuses exactly as bumping the exit always did —
        // free, with the §4.5 message — rather than letting the player crawl four cells
        // to be told no somewhere they can do nothing about it. With the gate met the
        // bump **climbs in**, exactly as a §10.7 shortcut's entry does, and the win is
        // the step off the board at the far end ([`way_out_kind`](Self::way_out_kind)),
        // which is the one the run *starts* inside and so still answers the gate itself.
        //
        // Like any duct entry it refuses a dragging player: a body cannot follow into
        // the walls, so let it go before you leave.
        if target == self.exit {
            return if self.dragging.is_some() {
                BumpKind::Solid
            } else if self.exit_ready() {
                BumpKind::EnterExitDuct
            } else {
                BumpKind::Exit { ready: false }
            };
        }
        if self.objectives.iter().any(|o| o.cell == target && !o.taken) {
            return BumpKind::Intel;
        }
        // An equipment cache (§2.2/§14 v3/#209), while it still holds its tech. An opened
        // crate falls through to the plain solid bump below, on the spent console's own
        // terms. A live one is never silent: taking it, trading for it (#266) and being
        // refused it are three kinds of their own, because the usable line has to say
        // which (§11.4/§2.3).
        if let Some(cache) = self.caches.iter().find(|c| c.cell == target && !c.taken) {
            return self.salvage_kind(cache.holds);
        }
        // The comms console (§7.3/§7.7), while the net is still live. A silenced one
        // falls through to the plain solid bump below — spent scenery, offering
        // nothing, which is precisely how the design wants it to read.
        if self.comms_console == Some(target) && !self.radio_silenced {
            return BumpKind::SilenceRadio;
        }
        if let Some(action) = self.layout.preview_door_bump(target, |c| self.occupied(c)) {
            // The withheld frame (#320): the hinge of the door the player's *previous*
            // action opened does not shut it — for that one action it is a dead bump
            // that the #57 slide can round instead, so a player walking into a doorway
            // slightly off-line gets through rather than undoing their own open. Only
            // the close is withheld; an obstructed one is already a free no-op (doors
            // never crush) and is left exactly as it was.
            if action == DoorAction::Closed && self.frame_bump_withheld(target) {
                return BumpKind::HingeHeld;
            }
            // Autodoors (§8.3/§7.6): a closed door in the path opens and is walked
            // through in one step — but only when the opened cell is a walkable panel.
            // A hinge (#148) also opens the door on a bump, yet the hinge itself stays
            // solid, so there is nothing to step onto: that remains a plain door op.
            if action == DoorAction::Opened
                && self.abilities.effect_active(Effect::AutoDoors)
                && self.door_panel_at(target)
            {
                return BumpKind::AutoDoor;
            }
            return BumpKind::Door { action };
        }
        match self.layout.facility().terrain(target) {
            Some(Terrain::Hideout) => {
                // A cupboard holding a stowed body is **locked** — no longer a
                // hideout (§7.2) — and one holding an actor is occupied: either way
                // it refuses entry. Empty, a dragging player **stows** their body in
                // it (deposit-and-lock); hands free, they climb in to hide (§10.3).
                if self.body_at(target).is_some() || self.occupied(target) {
                    BumpKind::HideoutBlocked
                } else if self.dragging.is_some() {
                    BumpKind::DepositBody
                } else {
                    BumpKind::Hide
                }
            }
            Some(Terrain::DuctEntry) => {
                // Standing at the mouth, bump the entry to climb in (§10.7) — a
                // decision, like the cupboard. A body cannot follow into the walls,
                // so a dragging player is refused (the entry reads solid); let go
                // first. (Reached only when *not* already in a duct — the in-duct
                // confinement above owns crawling and climbing out.)
                if self.dragging.is_some() {
                    BumpKind::Solid
                } else {
                    BumpKind::EnterDuct
                }
            }
            Some(Terrain::PartialCover) => {
                // Any table of the run already crouched behind is the held pose
                // (§10.3 — the bench is one piece of furniture); a different
                // run's table re-anchors the crouch there.
                if self.crouch_covers(target) {
                    BumpKind::CrouchHeld
                } else {
                    BumpKind::Crouch
                }
            }
            _ if self.layout.facility().can_enter(target, ACTOR_FILL) => BumpKind::Move,
            _ => BumpKind::Solid,
        }
    }

    /// Phases 2 and 3 (§4.2): recompute sight, run the radio net, then let the
    /// guards act. Shared by the startup turn and every spent player turn. The
    /// radio sits between sight and the guards deliberately: a responder it
    /// dispatches this turn has its cone recomputed (it moved last turn) and then
    /// senses and steps *this* turn, so the dot peels off the moment control
    /// notices the silence (§7.3), not a turn late.
    fn run_world_phases(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        // Whether the facility was already searching when this turn's world opened
        // (§7.6/§11.7/#224). Read here rather than inside the guard phase because a
        // search can end in any of the phases below — the radio can pull the last
        // searcher onto an errand, a guard can release, a fresher lead can supersede
        // one — and the near line reports the *boundary*, not the mechanism that
        // crossed it. Reported at the foot of this function, against the same reading.
        let was_searching = self.search_under_way();
        // Fade the sense channel one turn *before* this turn's facts can relight it
        // (§9/§9.4) — the door cues and the guard trail alike — so a cue placed this
        // turn keeps its full life and a re-stamp refreshes rather than
        // double-decrements.
        self.decay_sense_cues();
        // The effect marks age on the same schedule and for the same reason
        // (§11.5/#308/#338): one turn of life spent before this turn's activation can
        // light a fresh one ([`record_effect_marks`](Self::record_effect_marks)).
        self.decay_effect_marks();
        self.recompute_sight();
        self.radio_phase(&mut events);
        self.guard_phase(&mut events);
        // Rungs 2 and 3 send guards in (§7.3/#374). *After* the guards have acted, and
        // deliberately so: phase 3 indexes its per-guard readings by position, so a
        // body found mid-phase must not grow the vector it is reading. The arrival is
        // the end of the turn it was called for, and a guard that has just walked in
        // acts from the next one — it has not looked yet, and its cone is recomputed
        // with everybody else's at the head of it.
        self.land_reinforcements(&mut events);
        // A reinforcement whose errand ended this turn changes the guard set, so the
        // level's §7.5 partition is recut here: the newcomer takes ground around where
        // it finished rather than where it walked in (§7.3/#374), and the incumbents'
        // beats shrink to make room. It sits after the arrivals so a guard that lands
        // and stands down on the same turn is still settled on that turn, and it is a
        // no-op on every other turn.
        self.recut_beats();
        self.door_phase(&mut events);
        // Autodoors shuts the doors the player passed through this run once their
        // throats clear (§8.3/§7.6) — after the guards move, so a pursuer stepping
        // into the doorway holds it open exactly as any occupant does (§10.4).
        self.close_armed_autodoors(&mut events);
        // Latch a fading cue on every door the player did not cause (§10.4) — the
        // player's own open is emitted in phase 1 and never reaches here.
        self.record_door_cues(&events);
        // Stamp the other half of the same channel (§9.2/#192): the cell of every guard
        // the player still feels through a wall, read *after* the guards have moved and
        // the reinforcements have landed, so the mark is where the turn actually left
        // them.
        self.record_guard_cues();
        // Last, so the pair reports what the whole world turn settled on rather than
        // what any one phase did (§11.7/#224). Its rung is below every threat message,
        // so being the final push costs it nothing: `loudest_first` sorts by priority,
        // and only a tie with another search boundary — which cannot happen, the two
        // being a diff of one bool — could turn on the order.
        self.report_search_boundary(was_searching, &mut events);
        events
    }

    /// The radio net (§7.3): control's pings, resolved once per world turn. A
    /// downed guard cannot answer, so each body runs a personal clock
    /// ([`Body::ping_due`](crate::body::Body)); the turn a ping comes due it is
    /// **missed**:
    ///
    /// - **First miss** — control dispatches the nearest still-active guard
    ///   ([`radio::nearest_respondable`]) to **where the guard fell**
    ///   ([`Body::fell_at`](crate::body::Body::fell_at)) — control's last fix on
    ///   it — switching it to [`Responding`](crate::GuardState::Responding), and it
    ///   **searches** there on arrival (§7.6). If every guard has the live player,
    ///   nobody is free and the silence goes un-investigated — the rung steps anyway.
    ///   That first silence is also a **rung-1 trigger** on the facility alert ladder
    ///   (§7.3) — and the *second post* to fall silent is a rung-3 one: two missed
    ///   pings across two bodies is an intruder taking the place apart, where one
    ///   quiet post could be a fault.
    /// - **Second miss** — control has called this post twice and gives up on it
    ///   ([`MAX_MISSED_PINGS`](crate::radio) caps the pinging). It escalates nothing
    ///   on its own: a post that has already gone quiet tells control nothing new, so
    ///   the ladder counts **bodies**, not pings.
    ///
    /// A **hidden** body still misses its pings (§7.3): hiding a body confuses the
    /// investigation — the responder searches the cell the body was dragged away
    /// from — it does not cancel it. Both events are surfaced (§11.7): the silence
    /// as a near-line message, the responder as its own sensed dot (§9).
    ///
    /// A net the player has **killed** at the comms console (§7.7) runs none of this:
    /// control cannot ping what it has no radio for, so no ping is ever missed, no
    /// responder is dispatched, and the alert never steps from this source again. The
    /// bodies keep their clocks untouched — there is nothing to resume, since silencing
    /// is one-way — and a guard already walking to a takedown site keeps going (see
    /// [`call_guards_to`](Self::call_guards_to)).
    fn radio_phase(&mut self, events: &mut Vec<Event>) {
        if self.radio_silenced {
            return;
        }
        // Index-walk: `bodies` is only ever appended to (§7.2), so indices are
        // stable across the loop, and the dispatch borrows `guards` separately.
        for i in 0..self.bodies.len() {
            if !self.bodies[i].ping_due(self.turn) {
                continue;
            }
            let at = self.bodies[i].fell_at();
            if self.bodies[i].miss_ping() == 1 {
                // First miss: send the nearest guard who isn't already on the
                // player. `respond_to` sets its destination and lead (§7.4), and
                // the walk ends in a search of the takedown site (§7.6).
                let sent = radio::nearest_respondable(&self.guards, at, 1, self.layout.facility());
                for g in sent {
                    self.guards[g].respond_to(at);
                }
                events.push(Event::RadioSilence { at });
                // A post going quiet is the ladder's own trigger (§7.3): the first is
                // rung 1, the second — a *different* body — is rung 3.
                let trigger = self.alert.post_fell_silent();
                // Control's last fix on the quiet post is where the guard fell (§7.3),
                // so that is the cell a reinforcement is sent to search (#374) — the
                // same cell the dispatched responder above is walking to, and the same
                // one a dragged body is no longer lying on (§8.3).
                self.raise_alert(trigger, at, events);
            }
        }
    }

    /// Step the facility alert ladder (§7.3) and report it if it actually rose, `at`
    /// being the cell the escalation is **about** — the body that was found, the
    /// console that was tampered with, the post that went quiet, the player's own cell
    /// for a sighting.
    ///
    /// The one place the rung is written, so the no-decay rule (§7.3) and the
    /// "one event per escalation" rule are both held in a single line of code. A
    /// trigger at or below the current rung is silent: the ladder reports
    /// *escalations*, not occurrences, so a chase that keeps producing rung-1
    /// triggers at rung 3 says nothing.
    ///
    /// Being the only writer is also what makes rungs 2 and 3 send their guards
    /// **once** (#374): the rungs crossed are read here, from a number that only ever
    /// rises, so nothing has to remember what has already been paid for. `at` is the
    /// errand each arriving guard is given, which is why the cell is threaded in rather
    /// than looked up — control sends them to what it just learned about, not to
    /// wherever the player happens to be by the time they walk in (§7.6).
    fn raise_alert(&mut self, trigger: AlertTrigger, at: Cell, events: &mut Vec<Event>) {
        let from = self.alert.rung();
        if let Some(rung) = self.alert.raise(trigger) {
            events.push(Event::AlertRaised { rung, trigger });
            self.queue_reinforcements(from, rung, at);
        }
    }

    /// The §7.5 dwell rule in force this turn: the playtest chance knob, and the
    /// length range the facility alert imposes (§7.3 — rung 1's teeth). Read once
    /// per guard phase, so every guard in the same turn pauses under the same rule.
    fn dwell_rule(&self) -> Dwell {
        Dwell {
            chance: self.dwell_chance,
            turns: self.alert.dwell_turns(),
        }
    }

    /// How Calm patrol chooses its next target this turn (§7.5/§7.3) — the whole of
    /// what a **silenced radio** does to a patrol.
    ///
    /// With the net live, guards are coordinated and each sweeps its own slice of the
    /// §7.5 partition. Bumping the comms console kills that (§7.3): with nobody
    /// dispatching and nobody calling anyone in, there is no coordination left to
    /// divide the building between them, so every Calm guard takes the whole level and
    /// draws its next target at random. Silencing is one-way for the level, so the
    /// style never changes back.
    /// The `guards_watch_consoles` modifier (§12.6/#319) rides on the live-net style
    /// and only there, which is the one thing to read carefully here. Its cycle is over
    /// the consoles a guard's **beat** touches, and a silenced net leaves no beats at
    /// all — so the console watch goes the way coordination does, and the comms console
    /// (§7.3) buys off this modifier along with the dispatches. Resolved at this one
    /// seam, so no other read site learns the flag exists (§12.3).
    fn patrol_style(&self) -> PatrolStyle {
        if self.radio_silenced {
            PatrolStyle::Wander
        } else if self.modifiers.guards_watch_consoles {
            PatrolStyle::WatchedConsoles
        } else {
            PatrolStyle::Beat
        }
    }

    /// How much of a guard's touching ring is blind (§6.1/§6.2/#410/#442) — the
    /// **rule**, not a level's choice any more: [`BlindPolicy::FlankWhileCalm`],
    /// resolved per guard against its own mood. A **Calm** guard detects exactly its
    /// ~90° cone ([`BlindTier::FLANK`] — its flanks blind along with its back); every
    /// other mood watches its sides ([`BlindTier::REAR`], §155's three cells).
    ///
    /// It was the `calm_guards_detect_only_their_cone` experiment (§12.6) until the
    /// measurement came in clean (appendix 28) and #442 adopted it. The seam survives
    /// the adoption because the *policy* was never the interesting part: what matters
    /// is that the tier is resolved from the guard's mood at look time, so a guard's
    /// sides come back the turn it stops being Calm, with no new state and no timer.
    ///
    /// Still a function rather than a constant, and still read fresh on every use
    /// (§12.3) exactly like [`patrol_style`](Self::patrol_style) — one truth, so a
    /// guard's cone and the §11.5 danger overlay drawn from it cannot disagree.
    pub(crate) fn guard_blind_policy(&self) -> BlindPolicy {
        BlindPolicy::FlankWhileCalm
    }

    /// Phase 2 (§4.2): recompute every viewer's field of view from its current
    /// position and facing (§6). Running *after* the player acts and *before* the
    /// guards read it is what designs out the old one-turn sensory lag (§4.2). The
    /// player's arc is the ~180° half-disc — or the full 360° if this turn was spent
    /// waiting (§8.3) — and their sight carries the auto-peek (#121): the union
    /// with the cast from one cell ahead, which reads around adjacent corners and
    /// out of cupboard mouths. Guards carve their ~90° wedge with the **plain**
    /// cast (§6.2) — the peek is the player's alone, so a corner the player can
    /// read still breaks the guard's line (§7.6).
    fn recompute_sight(&mut self) {
        let facility = self.layout.facility();
        let blind = self.guard_blind_policy();
        // Inside a duct the normal cone is off (§10.7): the player perceives only
        // their memory of the building and the shortened guard sense, with one live
        // window — the mouth peek from an entry cell (§6.1). Mid-duct there is no
        // live vision at all. `duct_fov` builds exactly that restricted set.
        self.player_fov = if self.in_duct() {
            self.duct_fov()
        } else {
            field_of_view_with_peek(
                facility,
                self.player,
                self.facing,
                self.player_sight_arc(),
                self.player_sight_range(),
            )
        };
        // The debug reveal (§12.6): the player's sight is *replaced* by the whole
        // grid — the playtest build where you can see the level. It is done here, at
        // the one place sight is produced, rather than as a special case in each view
        // that reads it: everything downstream then follows from the substitution on
        // its own — the fog lifts, entities draw wherever they stand, and every guard
        // reads as **Seen** (§9.2), so the §11.5 danger overlay paints every cone.
        // Nothing switches this on in a real run; only a baked build can (#245 tokens
        // cannot carry it), and it changes only what the *player* perceives — guards
        // look with their own cones, so the facility plays exactly as it would.
        if self.debug.reveal_whole_level {
            self.player_fov = VisibleSet::everything(facility.width(), facility.height());
        }
        // Tile memory (§11.5a) accumulates here, in the same phase that produced
        // the sight — every cell the player can see now is remembered forever. A
        // duct's interior path is deliberately *not* accumulated: it lives in its own
        // layer, shown only while crawled and never remembered (§11.5a/§10.7), so the
        // crawlspace's route is given away to nobody once the player has left it.
        //
        // The crawl view (`duct_fov`) always contains the occupied cell, so honouring
        // that rule means holding that one cell back while it is an **interior** cell.
        // An **entry** is geometry (§10.7) and accumulates like any other cell. This
        // used to be prose only: the cell went in, harmlessly, because memory drove
        // nothing but *contents*. Once memory drives how geometry is drawn, a crawled
        // duct would otherwise light its interior as explored — a thread of known wall
        // tracing the shortcut across the map, which is exactly the tell §11.5a keeps
        // the path in its own layer to avoid.
        let crawled_interior = self
            .occupied_duct()
            .filter(|duct| !duct.is_entry(self.player))
            .map(|_| self.player);
        self.memory
            .absorb_except(&self.player_fov, crawled_interior);
        for guard in &mut self.guards {
            guard.look(facility, blind);
        }
    }

    /// The player's field of view while **inside a duct** (§10.7) — the restricted
    /// perception a crawlspace grants. Always the player's own cell; and, when they
    /// stand on an **entry**, the live **mouth peek**: a cast out through the mouth
    /// (§6.1), the deliberate, safe way to read the room before climbing out. A
    /// mid-duct cell has no mouth and so no live window — memory only, which is the
    /// duct's information cost (§2.3). The peek is one-sided like the cupboard's: it
    /// shows the player a guard, but the guard's own plain cone cannot see a concealed
    /// crawler back ([`concealed_from`](Self::concealed_from)).
    fn duct_fov(&self) -> VisibleSet {
        crate::vision::duct_field_of_view(
            self.layout.facility(),
            self.player,
            self.duct_entry_facing(self.player),
            self.player_sight_arc(),
            self.player_sight_range(),
        )
    }

    /// The player's sight arc this turn (§6.2): §5's ~180° half-disc, widened to the
    /// full 360° ([`FULL_SIGHT_ARC`]) by **either** a turn spent waiting (§8.3/§9.1)
    /// **or** the Vision passive (§8.3/#265).
    ///
    /// The two widenings meet here, in one expression, precisely so they cannot
    /// stack: 360° is the whole circle and there is nothing past it, so waiting
    /// while holding Vision buys the same arc it already had — the wait's other
    /// half, the widened guard sense (§9.1), is what still makes it worth a turn.
    fn player_sight_arc(&self) -> u8 {
        if self.waited || self.abilities.effect_active(Effect::EnhancedSight) {
            FULL_SIGHT_ARC
        } else {
            PLAYER_SIGHT_ARC
        }
    }

    /// The player's sight range this turn (§5/§6.1): the §5 box, or the enlarged one
    /// while the Vision passive is held (§8.3/#265). Waiting does **not** extend
    /// range — it only ever bought the arc (§9.1).
    fn player_sight_range(&self) -> u32 {
        if self.abilities.effect_active(Effect::EnhancedSight) {
            ENHANCED_SIGHT_RANGE
        } else {
            PLAYER_SIGHT_RANGE
        }
    }

    /// The index of a guard standing on `cell`, if any.
    fn guard_at(&self, cell: Cell) -> Option<usize> {
        self.guards.iter().position(|g| g.pos() == cell)
    }

    /// The index of a body lying on `cell`, if any.
    fn body_at(&self, cell: Cell) -> Option<usize> {
        self.bodies.iter().position(|b| b.cell() == cell)
    }

    /// Whether any actor occupies `cell` — the **door-crush** occupancy predicate
    /// (§10.4: a door never shuts on an actor). Actors are the player, the guards,
    /// and the bodies takedowns leave (§7.2), so a door refuses to close on a body
    /// even though a body is otherwise non-solid to movement and pathing. Movement
    /// checks read the player and guards directly, not this.
    fn occupied(&self, cell: Cell) -> bool {
        actor_occupies(self.player, &self.guards, &self.bodies, cell)
    }
}

/// Whether `id` declares `effect` in its data-driven behaviour (§8.1) — how the
/// loop asks "does activating this spawn a decoy?" without naming an ability. A
/// [`Behaviour::Coded`] ability declares nothing here; its behaviour would live
/// in code keyed on the id.
fn declares(id: AbilityId, effect: Effect) -> bool {
    matches!(id.def().behaviour(), Behaviour::Effects(effects) if effects.contains(&effect))
}

/// Whether an actor occupies `cell`, given the actor fields directly. The free
/// twin of [`State::occupied`], for callers that must borrow the actor fields apart
/// from the rest of the state (door closing borrows the layout mutably at the same
/// time). One definition of "an actor is here" — extend it, not the call sites, when
/// new actor kinds arrive. A body counts here so a door never closes on one (§10.4 —
/// doors never crush, and a body under a shut door would be unreachable). A body is
/// otherwise **non-solid** (§7.2): it blocks neither movement nor pathing, so the
/// loop's movement checks read guards and the player directly, not this predicate.
fn actor_occupies(player: Cell, guards: &[Guard], bodies: &[Body], cell: Cell) -> bool {
    player == cell
        || guards.iter().any(|g| g.pos() == cell)
        || bodies.iter().any(|b| b.cell() == cell)
}

#[cfg(test)]
mod tests;
