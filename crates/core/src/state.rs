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
//!   take at least one intel ([`exit_ready`](State::exit_ready), §10.2 [START]), then
//!   return to the exit you came in by; bumping it empty-handed refuses. There is no
//!   health, no combat.
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
//! renderer and the §13.2 bot ask ([`view`]), phase 3 ([`guards`]), doors and their
//! §9.4 cues ([`doors`]), the ability effects ([`abilities`]) and the #57 auto-slide
//! ([`traversal`]). They are all `impl State` blocks over the *same* struct — plain
//! structs, not an ECS (§12.3), so the coupling stays visible in the types.

use crate::ability::{
    AbilityId, AbilityState, AbilityStatus, Behaviour, Deck, Effect, Loadout, TargetingMode,
};
use crate::body::Body;

use crate::category::Category;
use crate::cell::{Cell, Direction};
use crate::cover;
use crate::duct::Duct;
use crate::facility::Terrain;
use crate::generate::Layout;
use crate::guard::{Guard, GuardState, GUARD_CLOSE_CHANCE_PERCENT, GUARD_DWELL_CHANCE_PERCENT};
use crate::level_seed::LevelSeed;
use crate::modifiers::{DebugModifiers, LevelModifiers};
use crate::radio;
use crate::region::{DoorCell, DoorId};
use crate::rng::Rng;
use crate::targeting::Targeting;
use crate::vision::{
    field_of_view_with_peek, VisibleSet, ENHANCED_SIGHT_RANGE, FULL_SIGHT_ARC, PLAYER_SIGHT_ARC,
    PLAYER_SIGHT_RANGE,
};
use crate::DoorAction;

mod abilities;
mod doors;
mod events;
mod guards;
mod traversal;
mod view;

pub use events::{Affordance, Event, Input};

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
/// **[START]**): a door open/close is a **discrete** event, not a standing position
/// like a guard, so its cue is inherently a fading mark. It reads like sensed
/// evidence — visible while the fact is fresh — rather than a single-frame flash.
/// Placed at full life the turn the door changes and decremented once per world
/// turn, so the cue shows for this many renders and is gone on the next. Pinned by a
/// test.
pub const DOOR_CUE_DECAY_TURNS: u32 = 3;

/// The **Confusion** blast radius (§8.3/§9/#240 **[START]**): while the Confusion
/// ability is active ([`Effect::Confuse`](crate::Effect)), every guard within this
/// Chebyshev box of the player is blinded and frozen — measured the same way as the
/// guard sense (§6.1 box metric) and, like it, reaching **through walls** (§9).
///
/// It stays **smaller** than [`PLAYER_SENSE_RANGE`] (asserted below) so the bubble can
/// never reach a guard the player cannot already sense — but it now covers a guard's
/// whole *certain* zone (`CERTAIN_RANGE` = 5, §7.6), so the guards actually bearing
/// down on you are the ones it catches. Raised from the first pass's 4, where the
/// bubble was tight enough that a chaser could sit just outside it. Pinned by a test
/// so a later change is a visible edit.
pub const CONFUSION_RADIUS: u32 = 6;

/// Confusion's bubble stays **within the guard sense** (§9/#240): a guard is never
/// frozen before the player can even sense its dot, so the effect is always legible
/// on the map. Pinned at compile time so the two ranges can never silently invert.
const _: () = assert!(CONFUSION_RADIUS <= PLAYER_SENSE_RANGE);

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
    /// The exit; `ready` is [`exit_ready`](State::exit_ready) — at least one intel in
    /// hand (§10.2 [START]): win vs. refused.
    Exit { ready: bool },
    /// An objective console still holding its intel.
    Intel,
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
            BumpKind::Crouch => Some(Affordance::Crouch),
            // A crawl is movement, not an offered interaction (§10.7) — like a plain
            // Move it shows nothing on the usable line.
            BumpKind::Guard { aware: true }
            | BumpKind::Door {
                action: DoorAction::Obstructed,
            }
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

/// A fading door-change cue (§9.4/§10.4): the door that opened or shut away from the
/// player, and how many more turns the mark shows. A door change is a **discrete**
/// event — unlike a guard, there is no standing position to re-read each frame — so
/// the fact is latched here the turn it happens (if within [`DOOR_SENSE_RANGE`]) and
/// fades over [`DOOR_CUE_DECAY_TURNS`] world turns. The renderer lights the door's
/// **whole footprint** as a [`Category::Sensed`] background — the same sense channel
/// as a guard felt through a wall ([`State::door_cues`]).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct DoorCue {
    /// The door whose whole footprint the cue lights.
    door: DoorId,
    /// Turns of life left; decremented once per world turn and dropped at zero.
    ttl: u32,
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

/// One objective: an intel console and whether it has been taken. The v1 exit rule
/// is *at least one intel required* ([`exit_ready`](State::exit_ready), §10.2 [START]),
/// so the run is won once any objective is in hand.
#[derive(Clone, Copy, Debug)]
struct Objective {
    cell: Cell,
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
    /// Whether the radio net has been **killed** for the rest of the level (§7.3/§7.7)
    /// — set by bumping the comms console and never cleared. One-way and permanent by
    /// construction: nothing in the loop writes `false`, so there is no window in which
    /// the net could come back and no timer to read.
    radio_silenced: bool,
    exit: Cell,
    turn: u32,
    /// The facility-wide alert level (§7.3): a count of escalations, each from a
    /// concrete source — a guard that stopped answering its radio (the second
    /// missed ping). Starts at zero and steps up in [`radio_phase`](Self::radio_phase);
    /// it is *written and read* (the near line surfaces it, §11.4), which is the
    /// whole point after the old "never written to, never read" failure (§2.3).
    /// It does not decay within a run yet — coupling it back into guard behaviour
    /// is the cooperation/tuning work (§7.7); here it first gets teeth.
    alert: u32,
    outcome: Outcome,
    /// The events of the player's most recent action, free or spent — what the
    /// near line reads (§11.7: messages clear on the next action, so holding
    /// exactly one action's events *is* the clearing rule). Empty before the
    /// first input; frozen once the run ends, so the final message stays.
    last_events: Vec<Event>,
    /// The live door-change cues (§9.4/§10.4): doors that opened or shut away from
    /// the player, each fading over [`DOOR_CUE_DECAY_TURNS`] turns. Placed in
    /// [`record_door_cues`](Self::record_door_cues) for the door events the player did
    /// not cause, and decayed each world turn in
    /// [`decay_door_cues`](Self::decay_door_cues); the renderer reads their whole
    /// footprint through [`door_cues`](Self::door_cues). A small set — at most a
    /// handful of doors change in any few-turn window — so a plain `Vec` scan is
    /// cheaper than a map.
    door_cues: Vec<DoorCue>,
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
    /// The guards that **freshly** detected the player on the last spent turn — the
    /// transition [`Event::Detected`] reports (§7.6) — as indices into
    /// [`guards`](Self::guards), for the momentary **spot flash** (§11.5/§9.2, #222).
    /// Filled in [`guard_phase`](Self::guard_phase) and cleared at the head of every
    /// [`step`](Self::step), so it holds exactly one turn's fresh spots and clears on
    /// the next action — the same one-beat life as the near-line message (§11.7). The
    /// renderer reads it through [`spot_flash`](Self::spot_flash) and lights the
    /// sightline of each spotter the player still cannot *see* (a seen guard's cone
    /// already paints, §9.2). Indices, not cells: they are set and consumed within a
    /// single turn — no guard is added or removed between — so the order is stable for
    /// that window, and the renderer reads each spotter's *current* position.
    spotters: Vec<usize>,
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
        // One source of truth for the cell: the grid. Reading it back also picks up a
        // hand-built fixture that stamped its own console without a placement.
        let comms_console = layout.facility().find(Terrain::CommsConsole);

        let mut state = Self {
            layout,
            player,
            facing,
            player_fov: VisibleSet::default(),
            memory: VisibleSet::default(),
            in_duct: None,
            waited: false,
            moved_this_turn: false,
            crouched_behind: None,
            entered_hideout: None,
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
            radio_silenced: false,
            exit,
            turn: 0,
            alert: 0,
            outcome: Outcome::Playing,
            last_events: Vec::new(),
            door_cues: Vec::new(),
            autodoors_pending: Vec::new(),
            spotters: Vec::new(),
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

    /// The debug modifiers this build was baked with (§12.6) — read in the sight
    /// phase, and nowhere in the rules.
    #[must_use]
    pub fn debug(&self) -> DebugModifiers {
        self.debug
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
    /// level-seed string the help panel displays (#272).
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

    /// The run's ability loadout (§8.3/#244) — the abilities it holds, for a test
    /// to assert what a preset resolved and for the level-seed string to carry.
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

        let mut events = Vec::new();
        // A fresh turn: no hideout has been climbed into yet. The §15 Q5 witness check
        // in phase 3 reads whatever the Hide bump sets below, this turn only.
        self.entered_hideout = None;
        // The spot flash lasts exactly one action (§11.7, #222): clear last turn's
        // fresh spots here, so any this turn's guard phase records show for that beat
        // and nothing lingers on the next input — free action or spent.
        self.spotters.clear();
        // Phase 1. A free action (wall bump, refused exit) does not end the turn.
        let from = self.player;
        let spent = self.player_phase(input, &mut events);

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
            let phase_ended = expired.iter().any(|&id| declares(id, Effect::Phase));
            for &ability in &expired {
                // The decoy's lifetime is its ability's active window (§8.3):
                // expiry takes the fake with it.
                if declares(ability, Effect::SpawnDecoy) {
                    self.decoy = None;
                }
            }
            events.extend(
                expired
                    .into_iter()
                    .map(|ability| Event::AbilityExpired { ability }),
            );
            // Dephase expiring somewhere a solid body cannot stand is lethal
            // (§8.3 — the cost that keeps phasing from being free). No rescue,
            // no auto-eject to a safe cell: that would rebuild the old
            // consequence-free version. Skipped if the run already ended this
            // turn (a capture is its own, truthful loss).
            if phase_ended && self.outcome == Outcome::Playing && !self.can_rematerialize() {
                self.outcome = Outcome::Lost;
                events.push(Event::Entombed { at: self.player });
            }
        }

        // Every action replaces the near line's source, free bumps included —
        // this assignment is §11.7's "messages clear on the next action".
        self.last_events = events.clone();
        events
    }

    /// Phase 1 (§4.2). Returns whether the turn was spent (a world-changing action)
    /// or was free (a mis-input that ends nothing).
    fn player_phase(&mut self, input: Input, events: &mut Vec<Event>) -> bool {
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
            // a wall bump, is free and changes nothing. An ability that spawns into
            // the world (the decoy's faced cell, §8.4 Direction targeting) must also
            // have a valid target — a faced cell that could not hold an intruder
            // refuses the activation as the same free mis-input (§11.4's contextual
            // Unusable). A real activation is a spent action other than Wait, so it
            // stands the player up and narrows the arc.
            Input::Activate(id) => {
                let spawn = if declares(id, Effect::SpawnDecoy) {
                    match self.decoy_spawn_cell() {
                        Some(cell) => Some(cell),
                        None => return false,
                    }
                } else {
                    None
                };
                if self.abilities.activate(id) {
                    if spawn.is_some() {
                        self.decoy = spawn;
                    }
                    events.push(Event::AbilityActivated { ability: id });
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
                    if declares(id, Effect::SpawnDecoy) {
                        self.decoy = None;
                    }
                    events.push(Event::AbilityDeactivated { ability: id });
                }
                false
            }
        }
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
        let Some(target) = self.player.step(dir) else {
            // Off the north/west edge — the border is wall anyway, so a free mis-input.
            return false;
        };

        // One ladder decides what a bump does — the same `bump_kind` the usable line
        // reads — so execution and prediction can never disagree (§11.4). This match
        // performs the effect; the classification and its priority live in one place.
        let kind = self.bump_kind(target);

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
            // The exit: win if the objectives are done, else refuse — a refused exit
            // changes nothing and is free (§4.5).
            BumpKind::Exit { ready: true } => {
                self.outcome = Outcome::Won;
                events.push(Event::Won);
                true
            }
            BumpKind::Exit { ready: false } => {
                events.push(Event::ExitRefused);
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
                let remaining = self.objectives.iter().filter(|o| !o.taken).count();
                events.push(Event::IntelTaken { remaining });
                true
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
            BumpKind::EnterDuct => {
                self.player = target;
                self.facing = dir.opposite();
                // Record *which* duct we climbed into — the entry belongs to exactly
                // one (§10.7), and from here "in a duct" is this stored index, not the
                // cell (an interior cell may overlie floor the player could also walk).
                self.in_duct = self.layout.duct_index_containing(target);
                events.push(Event::EnteredDuct { at: target });
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
            // table already crouched behind, or anything else solid (a wall, a
            // pillar): a free bump (§4.4). A closed hinge is no longer here — it
            // opens the door now (#148, `BumpKind::Door`). Before it no-ops, the
            // traversal experiment (#57) gets a chance to read the dead bump as an
            // unambiguous sidestep and slide one cell past it; only if it declines
            // does the bump fall through to the free wall-bump it has always been.
            BumpKind::HideoutBlocked | BumpKind::CrouchHeld | BumpKind::Solid => {
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
        let vacated = self.player;
        self.haul_body_to(vacated);
        self.player = target;
        self.facing = dir; // facing follows the last successful step (§5)
        events.push(Event::Moved { to: target });
        // Take hold on the way out (§8.3): stepping *off* a body cell with free hands
        // starts the drag — the body stays in the vacated cell and follows from here.
        // The pickup rides this full step; the weight then catches up at half speed
        // (`drag_debt`), so the next step is spent hauling. A dragging player never
        // reaches here for a second body (hands are full), and a sprint's extra step
        // below is suppressed the instant a grab lands, so Run never stacks with Drag
        // (§8.3/#103).
        if self.dragging.is_none() {
            if let Some(i) = self.body_at(vacated) {
                self.dragging = Some(i);
                self.drag_debt = true;
                events.push(Event::BodyGrabbed { at: vacated });
            }
        }
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
        let regions = self.layout.regions();
        let door = regions.door(regions.door_at(target)?);
        if door.role(target)? != DoorCell::Hinge {
            return None;
        }
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
        let facility = self.layout.facility();
        let mouth = facility
            .neighbours(cell)
            .find(|&n| facility.can_enter(n, ACTOR_FILL))?;
        Direction::between(cell, mouth)
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
        if target == self.exit {
            return BumpKind::Exit {
                ready: self.exit_ready(),
            };
        }
        if self.objectives.iter().any(|o| o.cell == target && !o.taken) {
            return BumpKind::Intel;
        }
        // The comms console (§7.3/§7.7), while the net is still live. A silenced one
        // falls through to the plain solid bump below — spent scenery, offering
        // nothing, which is precisely how the design wants it to read.
        if self.comms_console == Some(target) && !self.radio_silenced {
            return BumpKind::SilenceRadio;
        }
        if let Some(action) = self.layout.preview_door_bump(target, |c| self.occupied(c)) {
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
        // Fade the door cues one turn *before* this turn's door events can relight
        // them (§9.4/§10.4), so a cue placed this turn keeps its full life and a
        // re-change refreshes rather than double-decrements.
        self.decay_door_cues();
        self.recompute_sight();
        self.radio_phase(&mut events);
        self.guard_phase(&mut events);
        self.door_phase(&mut events);
        // Autodoors shuts the doors the player passed through this run once their
        // throats clear (§8.3/§7.6) — after the guards move, so a pursuer stepping
        // into the doorway holds it open exactly as any occupant does (§10.4).
        self.close_armed_autodoors(&mut events);
        // Latch a fading cue on every door the player did not cause (§10.4) — the
        // player's own open is emitted in phase 1 and never reaches here.
        self.record_door_cues(&events);
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
    ///   nobody is free and the silence goes un-investigated — the second miss
    ///   still lands.
    /// - **Second miss** — the facility-wide alert steps (§7.3); control has
    ///   escalated as far as the design specifies and stops pinging the corpse
    ///   ([`MAX_MISSED_PINGS`](crate::radio) caps it).
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
                for g in radio::nearest_respondable(&self.guards, at, 1) {
                    self.guards[g].respond_to(at);
                }
                events.push(Event::RadioSilence { at });
            } else {
                // Second (final) miss: the escalation gets a concrete source.
                self.alert += radio::ALERT_STEP;
                events.push(Event::AlertRaised { level: self.alert });
            }
        }
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
        self.memory.absorb(&self.player_fov);
        for guard in &mut self.guards {
            guard.look(facility);
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
