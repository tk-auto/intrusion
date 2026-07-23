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
//!   take every objective, then return to the exit you came in by; bumping it early
//!   refuses. There is no health, no combat.
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
//! and it stands back down to patrol once the lead runs out. The rest of the §7.4
//! state machine — searching, decoys — is the later guard tickets, which set a
//! guard's destination the same way and reuse the same walk-toward-it movement.

use crate::ability::{
    AbilityId, AbilityState, AbilityStatus, Behaviour, Deck, Effect, TargetingMode,
};
use crate::body::Body;
use crate::category::Category;
use crate::cell::{Cell, Direction};
use crate::cover;
use crate::facility::Terrain;
use crate::generate::Layout;
use crate::guard::{Guard, GUARD_CLOSE_CHANCE_PERCENT};
use crate::radio;
use crate::region::{DoorCell, DoorId};
use crate::rng::Rng;
use crate::targeting::Targeting;
use crate::vision::{
    field_of_view_with_peek, VisibleSet, PLAYER_SIGHT_ARC, PLAYER_SIGHT_RANGE, WAIT_SIGHT_ARC,
};
use crate::DoorAction;

mod abilities;

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

/// What the player asks to do on their phase. Input mapping (which key is which,
/// §11.6) lives in the web shell; the loop knows only the actions.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Input {
    /// Step one cell. If the target is blocked this becomes the *bump* — the game's
    /// one interaction verb (§4.3): open a door, take the intel, leave by the exit.
    Step(Direction),
    /// Let the turn pass without moving. There is no turn-in-place action (§5), so
    /// waiting is the only way to spend a turn where you stand — which is what makes
    /// holding at a corner a real choice.
    Wait,
    /// Activate an ability (§8.2). A turn-costing action (§4.4): if the ability is
    /// ready it switches on and the turn is spent; if it is active or cooling this
    /// is a mis-input and resolves as a **free** no-op. The shell picks this over
    /// [`Deactivate`](Input::Deactivate) from the ability's current state (§11.6).
    Activate(AbilityId),
    /// Toggle an active ability off early (§4.4's free exception). Always free and
    /// never refunds — the full cooldown still runs (§8.2). A no-op on an ability
    /// that is not active.
    Deactivate(AbilityId),
}

/// Something the loop did this turn, reported in resolution order. Each event knows
/// its information [`Category`] ([`Event::category`]) so a message drawn from it
/// colours through the same §11.2 table as everything else; display priority and
/// the bar itself (§11.7) are the message ticket's job — the loop reports facts.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Event {
    /// The player stepped to `to`.
    Moved { to: Cell },
    /// A move was refused and nothing changed — a *free* bump (§4.4): a wall, a
    /// hinge, a body, or a guard that has detected you (§7.2).
    Bumped { into: Cell },
    /// The player bumped an empty hideout and climbed in (§4.3, §10.3): they now
    /// occupy the cupboard and are concealed. Climbing back out is an ordinary
    /// [`Event::Moved`] off the cell.
    EnteredHideout { at: Cell },
    /// The player bumped a table and ducked behind it (§4.3, §10.3): they are
    /// now crouched, concealed from any viewer whose line of sight crosses any
    /// table of the run `behind` belongs to (the whole §10.1a bench). Reported
    /// only when the crouch *engages* — re-bumping any table of that run is a
    /// free no-op. Waiting holds the pose, and so does a **crouch-walk** — a
    /// plain step that lands still hugging the run, its corners included; any
    /// other spent action stands you up, no special event.
    Crouched { behind: Cell },
    /// A closed door was opened by a bump on a panel (§10.4): the player's
    /// (§4.3), or a guard's — a guard's route runs straight through closed
    /// doors, and walking into the panel is the bump that opens them.
    DoorOpened { at: Cell },
    /// A **Calm** guard closed a hinged door behind itself after passing through
    /// it (§10.4, #146): the counter-pressure to guard traffic's monotonic opening,
    /// restoring the level's structure and keeping an open door meaningful as
    /// evidence that someone came this way. `at` is a panel of the shut door.
    DoorClosed { at: Cell },
    /// The player took the intel at a console; `remaining` objectives are still out.
    IntelTaken { remaining: usize },
    /// The player bumped the exit with objectives still outstanding — refused (§4.5).
    ExitRefused,
    /// Every objective was in hand and the player reached the exit: the run is won.
    Won,
    /// A guard moved into the player's cell: captured (§4.5) — the only loss.
    Captured { by: Cell },
    /// The player took an unaware adjacent guard down (§7.2): the guard is
    /// permanently out, and a body now lies at `at`.
    TakenDown { at: Cell },
    /// A guard at `by` **freshly** detected the player this turn (§7.6): its
    /// look found them after a turn (or a lifetime) of not seeing them. Fired on
    /// the transition only — a chase that holds the player in sight turn after
    /// turn is one detection, not one per turn — so counting these counts how
    /// often stealth actually broke (§13.2's "detection events per run"). The
    /// certain sighting and the glimpse both count: either one turns the guard
    /// hunting ([`GuardState`](crate::GuardState)).
    Detected { by: Cell },
    /// A guard's cone covered a body (§7.2) — the loudest event in the game,
    /// fired once per body. The finder's alert is raised harder than a sighting
    /// raises it; the radio escalation is §7.3/§7.7's tickets.
    BodyFound { at: Cell },
    /// A downed guard missed a radio ping (§7.3): control noticed the silence and
    /// dispatched the nearest active guard to the silent guard's last known
    /// `post`. The player reads it as a near-line message and as the responder's
    /// own sensed dot peeling off toward the post (§9) — the visual tell that
    /// replaces the old sound (§9.3).
    RadioSilence { post: Cell },
    /// A second missed radio ping stepped the facility-wide alert to `level`
    /// (§7.3): the concrete, explainable escalation the alert system was always
    /// meant to provide (§2.3). Written here, read on the near line (§11.4).
    AlertRaised { level: u32 },
    /// The player took hold of an adjacent body (§8.3): they are now dragging
    /// it, at half speed, until they release it or the run ends.
    BodyGrabbed { at: Cell },
    /// The player let the dragged body go where it lies (§8.3) — free (§4.4),
    /// and it refunds nothing because there is nothing to refund.
    BodyReleased { at: Cell },
    /// The decoy was stepped on — by anything, the player included — and died
    /// (§8.3). Its ability drops into the full cooldown, as an early toggle-off
    /// would. Expiry by duration is [`Event::AbilityExpired`], not this.
    DecoyDied { at: Cell },
    /// Dephase ran out while the player stood somewhere that cannot admit a
    /// solid body — inside a wall, a door, furniture, or another actor — and
    /// rematerializing there is lethal (§8.3): the run ends. A distinct loss
    /// from [`Event::Captured`], so the game-over reason stays truthful.
    Entombed { at: Cell },
    /// The player activated an ability (§8.2) — a turn-costing action (§4.4).
    AbilityActivated { ability: AbilityId },
    /// The player toggled an ability off early (§4.4) — free; its cooldown still
    /// runs (§8.2).
    AbilityDeactivated { ability: AbilityId },
    /// An ability's duration ran out at end of turn and it switched off (§8.2).
    AbilityExpired { ability: AbilityId },
}

impl Event {
    /// What this event *means* when shown to the player (§11.2) — the category a
    /// message reports it under, so a red message bar and a red `g` reinforce
    /// (§11.7 owns priority and display; the meaning is declared here, and no
    /// concrete colour ever is).
    pub fn category(self) -> Category {
        match self {
            // Routine self-narration: inert facts about scenery and your own steps.
            Event::Moved { .. } | Event::Bumped { .. } => Category::Neutral,
            // Things you made — including making yourself hidden (§10.3: the
            // occupied cupboard and the covering table recolour to Owned; their
            // messages match).
            Event::EnteredHideout { .. } | Event::Crouched { .. } => Category::Owned,
            // Your abilities are your tools — switching one on or off, or its fading,
            // is something you did or hold (§8), so it reads in the Owned band. The
            // decoy is a thing you made (§11.3): its death reads there too.
            Event::AbilityActivated { .. }
            | Event::AbilityDeactivated { .. }
            | Event::AbilityExpired { .. }
            | Event::DecoyDied { .. } => Category::Owned,
            // The takedown is something you did (§7.2) — your one offensive verb,
            // reading in the same band as your other tools. Handling the body it
            // left (§8.3) is the same hands: grabbing and releasing are Owned.
            Event::TakenDown { .. } | Event::BodyGrabbed { .. } | Event::BodyReleased { .. } => {
                Category::Owned
            }
            // A found body flips its finder to hunting (§7.2/§7.4): the threat is
            // aroused but does not have you — the Warning band. A radio silence
            // and the alert step it can lead to are the same kind of aroused
            // threat — control knows something is wrong but nothing has you yet.
            Event::BodyFound { .. } | Event::RadioSilence { .. } | Event::AlertRaised { .. } => {
                Category::Warning
            }
            // A guard that sees you is hunting *you* — the same Danger band as
            // its Chasing/Investigating glyph (§7.4), so the message and the `g`
            // reinforce (§11.2).
            Event::Detected { .. } => Category::Danger,
            // Neutral furniture doing furniture things (§10.4) — a door swinging
            // open or shut is scenery, whoever moved it.
            Event::DoorOpened { .. } | Event::DoorClosed { .. } => Category::System,
            // Goals and rewards — including the exit talking about the goal it
            // still refuses (§4.5) and the win itself.
            Event::IntelTaken { .. } | Event::ExitRefused | Event::Won => Category::Interest,
            // A threat that has you, literally (§4.5) — or the wall does (§8.3).
            Event::Captured { .. } | Event::Entombed { .. } => Category::Danger,
        }
    }
}

/// One thing a bump would do right now — the **usable line**'s vocabulary
/// (§11.4). Derived from adjacency by [`State::affordances`], never stored: the
/// line is a pure view of state, recomputed every frame, with nothing to clear.
///
/// The set is exactly the interactions [`State::step`]'s bump resolution
/// actually performs: the line must never offer what a bump will not do (§2.3).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Affordance {
    /// An adjacent guard that has not detected the player this turn: bump to
    /// take it down (§7.2). Offered only while the takedown would actually
    /// land — an aware guard's cell offers nothing.
    Takedown,
    /// An adjacent body, hands free: bump to take hold and drag it (§8.3).
    DragBody,
    /// The body being dragged: bump it to let go — free (§4.4).
    ReleaseBody,
    /// A closed door panel: bump to open (§10.4).
    OpenDoor,
    /// An open door's hinge: bump to close (§10.4).
    CloseDoor,
    /// An untaken intel console: bump to take the intel (§4.3).
    TakeIntel,
    /// An empty cupboard: bump to climb in and be concealed (§10.3).
    Hide,
    /// A table: bump to crouch behind it (§10.3).
    Crouch,
    /// The exit, with every objective in hand: bump to win (§4.5).
    Leave,
    /// The exit while intel is still out: bumping it will refuse (§4.5).
    ExitRefused,
}

impl Affordance {
    /// The words the usable line shows for this affordance.
    pub fn label(self) -> &'static str {
        match self {
            Affordance::Takedown => "guard: take down",
            Affordance::DragBody => "body: drag",
            Affordance::ReleaseBody => "body: release",
            Affordance::OpenDoor => "door: open",
            Affordance::CloseDoor => "door: close",
            Affordance::TakeIntel => "console: take intel",
            Affordance::Hide => "cupboard: hide",
            Affordance::Crouch => "table: crouch",
            Affordance::Leave => "exit: leave",
            Affordance::ExitRefused => "exit: needs the intel",
        }
    }

    /// What acting on this affordance is *about* (§11.2): doors, cupboards and
    /// tables are System furniture; the console and the exit are the goal,
    /// Interest; a takedown is about the unaware threat it targets — Caution,
    /// matching the yellow `g` it points at, as grabbing a loose body matches
    /// its Caution `z`; the body in your hands is Owned, like its recoloured
    /// glyph (§11.3).
    pub fn category(self) -> Category {
        match self {
            Affordance::Takedown | Affordance::DragBody => Category::Caution,
            Affordance::ReleaseBody => Category::Owned,
            Affordance::OpenDoor
            | Affordance::CloseDoor
            | Affordance::Hide
            | Affordance::Crouch => Category::System,
            Affordance::TakeIntel | Affordance::Leave | Affordance::ExitRefused => {
                Category::Interest
            }
        }
    }
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
    /// A body, hands free (§8.3): bumping it takes hold — the grab that starts
    /// the drag.
    BodyGrab,
    /// The body currently being dragged: bumping it lets go — free (§4.4).
    BodyRelease,
    /// A body while another is already in hand — just solid, a free bump (one
    /// body at a time; letting go first is free).
    BodyBlocked,
    /// The exit; `ready` is true iff every objective is in hand (win vs. refused).
    Exit { ready: bool },
    /// An objective console still holding its intel.
    Intel,
    /// A door cell whose bump is a real door action (open, close, or a crush-refused
    /// close). Both handles of a closed door open it — a panel and, since #148, a
    /// hinge. Only an **open panel** is *not* a door action: it classifies as
    /// [`BumpKind::Move`], the walk-through, exactly as the bump resolves.
    Door { action: DoorAction },
    /// An empty concealment cupboard to climb into (§10.3).
    Hide,
    /// A cupboard already holding an actor — solid, a free bump.
    HideoutBlocked,
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
            BumpKind::BodyGrab => Some(Affordance::DragBody),
            BumpKind::BodyRelease => Some(Affordance::ReleaseBody),
            BumpKind::Exit { ready: true } => Some(Affordance::Leave),
            BumpKind::Exit { ready: false } => Some(Affordance::ExitRefused),
            BumpKind::Intel => Some(Affordance::TakeIntel),
            BumpKind::Door {
                action: DoorAction::Opened,
            } => Some(Affordance::OpenDoor),
            BumpKind::Door {
                action: DoorAction::Closed,
            } => Some(Affordance::CloseDoor),
            BumpKind::Hide => Some(Affordance::Hide),
            BumpKind::Crouch => Some(Affordance::Crouch),
            BumpKind::Guard { aware: true }
            | BumpKind::BodyBlocked
            | BumpKind::Door {
                action: DoorAction::Obstructed,
            }
            | BumpKind::HideoutBlocked
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

/// One objective: an intel console and whether it has been taken. The v1 exit rule
/// is *all intel required* (§10.2), so the run is won only once none remain.
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
    /// The run's seeded random source (§12.4), carried through the turn loop for the
    /// one thing in the loop that is now stochastic: a Calm guard's chance to close a
    /// door behind itself (§10.4/#146). It is the *continuation* of the generation
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

        let mut state = Self {
            layout,
            player,
            facing,
            player_fov: VisibleSet::default(),
            memory: VisibleSet::default(),
            waited: false,
            moved_this_turn: false,
            crouched_behind: None,
            guards,
            bodies: Vec::new(),
            dragging: None,
            decoy: None,
            drag_debt: false,
            abilities: Deck::new(),
            objectives,
            exit,
            turn: 0,
            alert: 0,
            outcome: Outcome::Playing,
            last_events: Vec::new(),
            // A fixed default stream until [`with_rng`](Self::with_rng) threads the
            // run seed. The startup world phase below draws nothing — a guard cannot
            // have passed through a door before it has taken a step — so setting the
            // real stream after construction observes the identical stream position.
            rng: Rng::new(0),
            close_chance: GUARD_CLOSE_CHANCE_PERCENT,
        };
        // The level-start full turn (§4.2): sight and guards, no player phase.
        let _ = state.run_world_phases();
        state
    }

    /// Thread the run's seeded random source into the state (§12.4) — the
    /// continuation of the very stream the level was generated from, so a whole run
    /// is one seed end to end. The loop uses it for the guard close-behind roll
    /// (§10.4/#146); everything else in the loop stays deterministic without it. The
    /// real game and the headless sim call this; a test that never exercises the
    /// close can rely on the fixed default set in [`new`](Self::new).
    pub fn with_rng(mut self, rng: Rng) -> Self {
        self.rng = rng;
        self
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

    /// The level geometry (§10.5) — read-only outside the core.
    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    /// Where the player stands.
    pub fn player(&self) -> Cell {
        self.player
    }

    /// The player's facing — the direction of their last successful step (§5).
    pub fn facing(&self) -> Direction {
        self.facing
    }

    /// Open a targeting session (§8.4) for `mode`, anchored on the player's cell
    /// and facing (§5). The shell drives the returned [`Targeting`] — steering the
    /// cursor with cardinals and confirming — while core owns validity: a `Tile`
    /// cursor is bounded to the §6.1 range box on this facility, and cancelling is
    /// just dropping the session (free, no turn — §4.4). Nothing here auto-targets;
    /// that absence is the whole point of building targeting up front (§8.4).
    pub fn begin_targeting(&self, mode: TargetingMode) -> Targeting {
        Targeting::begin(mode, self.player, self.facing)
    }

    /// Open a targeting session for `ability` by its declared [`TargetingMode`]
    /// (§8.4) — the seam a hotkey or an ability-panel click resolves an ability's
    /// target through, so no ability ever falls back to auto-targeting (the exact
    /// §8.4/§2.3 regression this system exists to prevent).
    pub fn begin_ability_targeting(&self, ability: AbilityId) -> Targeting {
        self.begin_targeting(ability.def().targeting())
    }

    /// The player's field of view (§6): the ~180° forward half-disc, or the full
    /// 360° on a turn spent waiting — the only way to see behind you (§8.3) —
    /// including the auto-peek around adjacent corners (#121,
    /// [`field_of_view_with_peek`]). What is
    /// in it renders lit and what is not renders dimmed (§11.5); the renderer
    /// reads this set for the live layer, and tile memory
    /// ([`memory`](Self::memory)) accumulates from it.
    pub fn player_fov(&self) -> &VisibleSet {
        &self.player_fov
    }

    /// The player's tile memory (§11.5a): every cell that has *ever* been inside
    /// their FOV this run, accumulated each sight phase and never forgotten. The
    /// fog renderer reads it to draw remembered contents — intel, hideouts —
    /// distinct from live and never-seen; live state (guards, door open/closed)
    /// deliberately never consults it, so nothing transient is ever "remembered".
    pub fn memory(&self) -> &VisibleSet {
        &self.memory
    }

    /// Whether the player is concealed — standing inside a hideout (§10.3).
    ///
    /// This is the one concealment query everything reads: the loop refuses a
    /// guard's contact against a hidden player (§4.5/§7.6), the renderer recolours
    /// the occupied cupboard to Owned, and — once vision lands (§6) — a guard's
    /// detection set excludes a hidden player by AND-ing this in, so the danger
    /// overlay cannot claim the player is seen while hidden. It is *derived* from
    /// position rather than stored, so it can never desync: the only way onto a
    /// hideout cell is to bump into it, and moving off clears it.
    pub fn hidden(&self) -> bool {
        self.layout.facility().terrain(self.player) == Some(Terrain::Hideout)
    }

    /// Whether the player is **crouched** behind partial cover (§10.3): they
    /// bumped a table to duck behind it and have not spent a turn on anything
    /// but waiting since. Crouching is weaker than the cupboard — concealment
    /// is directional, per-viewer, and only across the chosen table
    /// ([`concealed_from`](Self::concealed_from)) — and it is not contact-safe:
    /// a guard walking into a crouched player still captures (§4.5).
    pub fn crouched(&self) -> bool {
        self.crouched_behind.is_some()
    }

    /// The table the player ducked behind (§10.3), if any — the *anchor* naming
    /// the crouched-behind run. It stays the originally bumped cell while a
    /// crouch-walk moves the player along the bench, so it may no longer be the
    /// nearest table; the run it names is what matters. Rendering reads
    /// [`crouch_cover`](Self::crouch_cover) for the whole run; everything
    /// rule-side goes through [`concealed_from`](Self::concealed_from).
    pub fn crouched_behind(&self) -> Option<Cell> {
        self.crouched_behind
    }

    /// Whether the player is concealed from a viewer standing at `viewer` — the
    /// per-viewer concealment query the guard AI's detection will AND in and the
    /// danger overlay already honours (§11.5: the overlay must not claim the
    /// player is seen while they are not).
    ///
    /// Three ways to be concealed:
    /// - **In a cupboard** ([`hidden`](Self::hidden)): omnidirectional — no
    ///   viewer anywhere detects the player (§10.3).
    /// - **Camouflaged and still** (§8.3, [`Effect::ConcealWhileStill`]):
    ///   omnidirectional while the ability is active and the last spent turn
    ///   did not move the player; the turn they move, this clause lapses and
    ///   they are revealed for that turn. It resumes the next still turn. Like
    ///   every concealment it blocks *detection* only — never contact (§4.5):
    ///   invisible is not safe.
    /// - **Crouched behind a run of tables** ([`crouched`](Self::crouched)):
    ///   directional — from viewers whose line of sight to the player crosses
    ///   **any table of the run the player ducked behind** (the whole §10.1a
    ///   bench, not just the bumped cell — a guard cannot look down a bench and
    ///   see the player through its other tables), grazing a table's corner
    ///   included. Other runs the player happens to stand beside cover nothing.
    ///   Integer arithmetic throughout ([`cover::run_conceals`]), so it is
    ///   exactly deterministic (§12.4).
    ///
    /// Concealment is not cover from *contact*: a guard can still walk into a
    /// crouched player and capture (§4.5). And it composes with sight, not
    /// replaces it — a viewer that cannot see the player's cell at all needs no
    /// concealing.
    pub fn concealed_from(&self, viewer: Cell) -> bool {
        if self.hidden() {
            return true;
        }
        if self.abilities.effect_active(Effect::ConcealWhileStill) && !self.moved_this_turn {
            return true;
        }
        let Some(anchor) = self.crouched_behind else {
            return false;
        };
        let run = cover::cover_run(self.layout.facility(), anchor);
        cover::run_conceals(&run, self.player, viewer)
    }

    /// The cells of the partial-cover run the player is crouched behind (§10.3)
    /// — the whole §10.1a bench, in flood order — or empty when standing. The
    /// renderer recolours every cell of it to Owned (§11.3): the run is one
    /// piece of furniture, so it hides as one.
    pub fn crouch_cover(&self) -> Vec<Cell> {
        self.crouched_behind
            .map(|anchor| cover::cover_run(self.layout.facility(), anchor))
            .unwrap_or_default()
    }

    /// The guards, for rendering and tests.
    pub fn guards(&self) -> &[Guard] {
        &self.guards
    }

    /// The bodies takedowns have left (§7.2), for rendering and tests.
    pub fn bodies(&self) -> &[Body] {
        &self.bodies
    }

    /// The cell of the body the player is dragging (§8.3), if any. The renderer
    /// recolours that `z` to Owned — the body in your hands, like the cupboard
    /// you hide in (§11.3) — and the ambient status reads the state from here.
    pub fn dragging(&self) -> Option<Cell> {
        self.dragging.map(|i| self.bodies[i].cell())
    }

    /// The live decoy's cell (§8.3), if one is out — the fake intruder the
    /// renderer draws as an Owned `@` (§10.3/§11.3: a thing you made, wearing
    /// your own glyph, which is the whole trick).
    pub fn decoy(&self) -> Option<Cell> {
        self.decoy
    }

    /// The player's current guard-sense range (§9.1): [`PLAYER_SENSE_RANGE`] normally,
    /// widened to [`PLAYER_SENSE_RANGE_WAITING`] on the turn the player's spent action
    /// was a Wait — the same `waited` signal that buys the 360° look (§8.3). A free
    /// action changes nothing, so a mis-input never widens or narrows the sense.
    pub fn sense_range(&self) -> u32 {
        if self.waited {
            PLAYER_SENSE_RANGE_WAITING
        } else {
            PLAYER_SENSE_RANGE
        }
    }

    /// How the player perceives `guard` this frame (§9.2), or `None` if it is neither
    /// seen nor sensed (out of range — it draws nothing, live, and is not remembered,
    /// §11.5a). This is the pure §9 classification the renderer reads:
    ///
    /// - **Seen** — the guard's cell is in the player's FOV (§6): line of sight is
    ///   clear, so its facing, cone and danger overlay are all known.
    /// - **Sensed** — not in the FOV, but within the guard-sense box
    ///   ([`sense_range`](Self::sense_range)) measured by the §6.1 box metric
    ///   ([`sight_distance`](Cell::sight_distance)) **through walls**: the exact cell
    ///   is known, nothing about where it looks.
    ///
    /// Seen wins over Sensed by construction — a guard in the FOV is Seen even if it is
    /// also inside the (larger or smaller) sense box — so the dot never coexists with
    /// the full guard on the same cell.
    ///
    /// A guard visible only through the auto-peek (#121) is **Seen**, cone and
    /// all: the lean is a real line of sight, so it earns the full picture, not
    /// the sensed dot. The overlay that cone paints stays truthful — the guard's
    /// own detection uses its plain cast, which cannot see around the corner
    /// back ([`field_of_view_with_peek`]'s one-sidedness).
    pub fn perceive_guard(&self, guard: &Guard) -> Option<GuardPerception> {
        if self.player_fov.contains(guard.pos()) {
            Some(GuardPerception::Seen)
        } else if self.player.sight_distance(guard.pos()) <= self.sense_range() {
            Some(GuardPerception::Sensed)
        } else {
            None
        }
    }

    /// How many objectives are still out. The run can be won only at zero (§10.2).
    pub fn objectives_remaining(&self) -> usize {
        self.objectives.iter().filter(|o| !o.taken).count()
    }

    /// The count of completed turns (the startup turn is turn zero).
    pub fn turn(&self) -> u32 {
        self.turn
    }

    /// The facility-wide alert level (§7.3): how many times the radio net has
    /// escalated this run (a guard going fully silent). Read by the near line's
    /// ambient status (§11.4) and available to the shell and the sim's alert-peak
    /// metric (§13.2).
    pub fn alert(&self) -> u32 {
        self.alert
    }

    /// Whether the run is live, won, or lost (§4.5).
    pub fn outcome(&self) -> Outcome {
        self.outcome
    }

    /// The events of the player's most recent action — the near line's source
    /// (§11.7). Empty before the first input; frozen once the run ends.
    pub fn last_events(&self) -> &[Event] {
        &self.last_events
    }

    /// The economy state of ability `id` (§8.2), as the panel reads it (§11.4):
    /// `Ready`, `Active` with the duration left, or `Cooling` with the cooldown
    /// left — the exact number the player gets (§8.2 timing). The show-on-wait
    /// render ticket wires the panel to this; the display's contextual `Unusable`
    /// (a missing target) is not an economy state and is never returned here.
    pub fn ability_state(&self, id: AbilityId) -> AbilityState {
        self.abilities.state(id)
    }

    /// The run's ability line/panel (§11.4): one [`AbilityStatus`] per economy
    /// ability, in the fixed deck order ([`AbilityId::ALL`]), each carrying its
    /// real slot state ([`ability_state`](Self::ability_state)). This is what the
    /// always-on line and the deployed panel draw ([`render_screen`]) and what a
    /// click hit-tests against ([`ability_at`]) — assembled from live runtime, so
    /// there is no roster to drift from the economy.
    ///
    /// The set is exactly the *activated* abilities the time economy governs
    /// (§8.2). The innate **bump** verbs — Takedown and Drag (§7.2, §8.3) — are
    /// not here: they have no duration or cooldown to show and are not
    /// [`Input::Activate`]d but done by walking into their target, so their
    /// availability already speaks through the **usable line**
    /// ([`affordances`](Self::affordances)), not this line.
    ///
    /// [`render_screen`]: crate::render_screen
    /// [`ability_at`]: crate::ability_at
    pub fn ability_statuses(&self) -> Vec<AbilityStatus> {
        AbilityId::ALL
            .into_iter()
            .map(|id| AbilityStatus {
                id,
                state: self.ability_state(id),
            })
            .collect()
    }

    /// What a bump would do from here — the **usable line** (§11.4): each
    /// interaction orthogonally adjacent to the player, with the direction to
    /// bump it, in [`Direction::ALL`] order. The §10.6 one-usable guarantee
    /// keeps this to a single entry on generated boards; a hand-built state
    /// may list more, one per direction.
    ///
    /// This mirrors [`step`](Self::step)'s bump resolution case for case, so
    /// the line can never promise what a bump won't deliver: an **unaware**
    /// guard offers the takedown while an aware one offers nothing (§7.2), a
    /// spent console and an occupied cupboard are just solid, and door poses
    /// come from the same door graph the bump consults (§10.4). Each target must
    /// also be in the player's FOV — which the touching ring always is (§6.2) —
    /// so the line can never leak what the fog still hides (§11.5a).
    pub fn affordances(&self) -> Vec<(Direction, Affordance)> {
        let mut out = Vec::new();
        for dir in Direction::ALL {
            let Some(target) = self.player.step(dir) else {
                continue;
            };
            // The FOV gate is the predictor's alone — the line must never leak what the
            // fog hides (§11.5a); a bump itself needs no sight. What the *interaction*
            // is comes from the one shared ladder, so the label can't drift from it.
            if !self.player_fov.contains(target) {
                continue;
            }
            if let Some(a) = self.bump_kind(target).affordance() {
                out.push((dir, a));
            }
        }
        out
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
            Input::Deactivate(id) => {
                if declares(id, Effect::Phase) && !self.can_rematerialize() {
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
            && matches!(kind, BumpKind::Move | BumpKind::Hide)
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
                // The body inherits the downed guard's post *and* its radio
                // cadence (§7.3): the clock that was silent while it lived starts
                // ticking now, its first missed ping a full period out.
                self.bodies.push(Body::new(
                    target,
                    guard.station(),
                    guard.radio_clock(),
                    self.turn,
                ));
                events.push(Event::TakenDown { at: target });
                true
            }
            // An aware guard has you in its cone (§7.2's gate): the bump is a
            // free no-op — no half-takedown, no shove.
            BumpKind::Guard { aware: true } => {
                events.push(Event::Bumped { into: target });
                false
            }
            // Grabbing an adjacent body (§8.3): the player is now dragging it,
            // at half speed, and the grab itself is a world-changing, spent turn
            // (§4.4). No debt is owed yet — the first haul is the first move.
            BumpKind::BodyGrab => {
                self.dragging = self.body_at(target);
                self.drag_debt = false;
                events.push(Event::BodyGrabbed { at: target });
                true
            }
            // Letting the body go where it lies (§8.3): free, the §4.4 toggle-off
            // exception — and it refunds nothing, there is nothing to refund.
            BumpKind::BodyRelease => {
                self.dragging = None;
                self.drag_debt = false;
                events.push(Event::BodyReleased { at: target });
                false
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
                    events.push(Event::DoorOpened { at: target });
                    true
                }
                DoorAction::Closed => {
                    self.operate_door(target);
                    true
                }
                DoorAction::Obstructed => false,
            },
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
                events.push(Event::EnteredHideout { at: target });
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
            // Plain movement into a cell that admits the player.
            BumpKind::Move => {
                self.haul_body_to(self.player);
                self.player = target;
                self.facing = dir; // facing follows the last successful step (§5)
                events.push(Event::Moved { to: target });
                // Stepping onto your own decoy kills it (§8.3) — anything's step
                // does, the maker's included; a sprint checks its second cell too.
                self.stomp_decoy(target, events);
                self.run_extra_step(dir, events);
                true
            }
            // A body while one is already in hand, a cupboard already holding an
            // actor, the table already crouched behind, or anything else solid
            // (a wall, a pillar): a free bump (§4.4). A closed hinge is no longer
            // here — it opens the door now (#148, `BumpKind::Door`).
            BumpKind::BodyBlocked
            | BumpKind::HideoutBlocked
            | BumpKind::CrouchHeld
            | BumpKind::Solid => {
                events.push(Event::Bumped { into: target });
                false
            }
        }
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
        if let Some(i) = self.guard_at(target) {
            return BumpKind::Guard {
                aware: self.guards[i].detected_player(),
            };
        }
        if let Some(i) = self.body_at(target) {
            return match self.dragging {
                Some(held) if held == i => BumpKind::BodyRelease,
                Some(_) => BumpKind::BodyBlocked,
                None => BumpKind::BodyGrab,
            };
        }
        if target == self.exit {
            return BumpKind::Exit {
                ready: self.objectives_remaining() == 0,
            };
        }
        if self.objectives.iter().any(|o| o.cell == target && !o.taken) {
            return BumpKind::Intel;
        }
        if let Some(action) = self.layout.preview_door_bump(target, |c| self.occupied(c)) {
            return BumpKind::Door { action };
        }
        match self.layout.facility().terrain(target) {
            Some(Terrain::Hideout) => {
                if self.occupied(target) {
                    BumpKind::HideoutBlocked
                } else {
                    BumpKind::Hide
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
        self.recompute_sight();
        self.radio_phase(&mut events);
        self.guard_phase(&mut events);
        self.door_phase(&mut events);
        events
    }

    /// The automatic doors' self-close tick (§10.4/#147), run once per world turn
    /// after everyone has moved so it reads final positions. Each open automatic door
    /// whose doorway is clear counts down and, when its timer runs out, shuts —
    /// exactly as a manual close does, panels restamped solid so vision, sound
    /// occlusion and the renderer all track it. An occupied doorway rearms instead:
    /// an automatic door never crushes (§10.4). Every shut the player might see is
    /// reported as a [`DoorClosed`](Event::DoorClosed), the same event a guard-close
    /// (#146) raises.
    fn door_phase(&mut self, events: &mut Vec<Event>) {
        let player = self.player;
        let guards = &self.guards;
        let bodies = &self.bodies;
        let closed = self
            .layout
            .tick_auto_doors(|c| actor_occupies(player, guards, bodies, c));
        for id in closed {
            let at = self.layout.regions().door(id).panels()[0];
            events.push(Event::DoorClosed { at });
        }
    }

    /// The radio net (§7.3): control's pings, resolved once per world turn. A
    /// downed guard cannot answer, so each body runs a personal clock
    /// ([`Body::ping_due`](crate::body::Body)); the turn a ping comes due it is
    /// **missed**:
    ///
    /// - **First miss** — control dispatches the nearest still-active guard
    ///   ([`radio::nearest_respondable`]) to the silent guard's last known post,
    ///   switching it to [`Responding`](crate::GuardState::Responding). If every
    ///   guard has the live player, nobody is free and the silence goes
    ///   un-investigated — the second miss still lands.
    /// - **Second miss** — the facility-wide alert steps (§7.3); control has
    ///   escalated as far as the design specifies and stops pinging the corpse
    ///   ([`MAX_MISSED_PINGS`](crate::radio) caps it).
    ///
    /// A **hidden** body still misses its pings (§7.3): hiding a body confuses the
    /// investigation — the responder walks to a post the body has been dragged
    /// away from — it does not cancel it. Both events are surfaced (§11.7): the
    /// silence as a near-line message, the responder as its own sensed dot (§9).
    fn radio_phase(&mut self, events: &mut Vec<Event>) {
        // Index-walk: `bodies` is only ever appended to (§7.2), so indices are
        // stable across the loop, and the dispatch borrows `guards` separately.
        for i in 0..self.bodies.len() {
            if !self.bodies[i].ping_due(self.turn) {
                continue;
            }
            let post = self.bodies[i].post();
            if self.bodies[i].miss_ping() == 1 {
                // First miss: send the nearest guard who isn't already on the
                // player. `respond_to` sets its destination and lead (§7.4).
                if let Some(g) = radio::nearest_respondable(&self.guards, post) {
                    self.guards[g].respond_to(post);
                }
                events.push(Event::RadioSilence { post });
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
        let arc = if self.waited {
            WAIT_SIGHT_ARC
        } else {
            PLAYER_SIGHT_ARC
        };
        self.player_fov =
            field_of_view_with_peek(facility, self.player, self.facing, arc, PLAYER_SIGHT_RANGE);
        // Tile memory (§11.5a) accumulates here, in the same phase that produced
        // the sight — every cell the player can see now is remembered forever.
        self.memory.absorb(&self.player_fov);
        for guard in &mut self.guards {
            guard.look(facility);
        }
    }

    /// Phase 3 (§4.2): the guards *sense*, then *act*. First every guard takes in this
    /// turn's information ([`Guard::sense`], §7.6) — it sees the player from the cone
    /// phase 2 just recomputed: a player in its cone flips it to Chasing (certain zone)
    /// or Investigating (glimpse zone). Detection is vision alone (§9 **[SETTLED]** —
    /// guards do not hear). A player [`concealed_from`](Self::concealed_from) that
    /// guard is not seen — the cupboard's payoff (§10.3/§7.6). Then each guard
    /// `decide`s a step (§7.5); a guard moving into the player's cell is a capture and
    /// ends the run (§4.5). Otherwise it moves onto any cell that admits it and holds
    /// no other actor; a guard with nowhere to go, or whose step is blocked, simply
    /// holds.
    fn guard_phase(&mut self, events: &mut Vec<Event>) {
        // Whether the player is concealed from each guard is a query over the whole
        // state (§10.3), so resolve it up front — one immutable read per guard —
        // before the loop takes each guard mutably to fold the senses in.
        let concealed: Vec<bool> = self
            .guards
            .iter()
            .map(|guard| self.concealed_from(guard.pos()))
            .collect();
        for (guard, &concealed) in self.guards.iter_mut().zip(&concealed) {
            // Awareness is per-turn, so the pre-sense reading is last turn's: a
            // guard aware now that was not aware then has *freshly* found the
            // player — the transition [`Event::Detected`] reports, and the §13.2
            // sim counts. A held chase re-detects every turn and stays silent.
            let was_aware = guard.detected_player();
            guard.sense(self.player, concealed);
            if guard.detected_player() && !was_aware {
                events.push(Event::Detected { by: guard.pos() });
            }
        }
        // The found-body scan (§7.2): a body is *found* the first time any cone
        // covers it — a body does not block sight, so the cones just recomputed
        // decide. Every guard seeing it reacts ([`Guard::find_body`]: the harder
        // alert, and the search unless the live player has it busy); the loudest
        // event in the game fires exactly once per body.
        for body_index in 0..self.bodies.len() {
            if self.bodies[body_index].found() {
                continue;
            }
            let at = self.bodies[body_index].cell();
            // A body in a hideout is *gone* (§7.2): the cupboard conceals it
            // completely, like the player it was built for — no cone finds it.
            // (It still misses its radio pings — that confusion is the §7.3
            // payoff, delivered by the radio ticket.)
            if self.layout.facility().terrain(at) == Some(Terrain::Hideout) {
                continue;
            }
            let mut seen = false;
            for guard in &mut self.guards {
                if guard.fov().contains(at) {
                    guard.find_body(at);
                    seen = true;
                }
            }
            if seen {
                self.bodies[body_index].mark_found();
                events.push(Event::BodyFound { at });
            }
        }
        // The decoy scan (§8.3, #105): a guard whose cone covers the decoy —
        // and whose look did *not* detect the player this turn — turns to
        // Investigate it. The precedence is the whole point: a guard that can
        // see you ignores the fake; decoys work on guards that have lost you.
        if let Some(decoy) = self.decoy {
            for guard in &mut self.guards {
                if !guard.detected_player() && guard.fov().contains(decoy) {
                    guard.investigate_decoy(decoy);
                }
            }
        }
        for i in 0..self.guards.len() {
            if self.outcome != Outcome::Playing {
                return;
            }
            let facility = self.layout.facility();
            // Guards are solid to each other and path *around* a colleague (§7.8) —
            // and around a body (§7.2, solid like any actor): the decider routes only
            // through cells no other guard or body holds. Positions are read fresh
            // here, so a guard sees where the colleagues that already moved this turn
            // now stand.
            let blocked: Vec<Cell> = self
                .guards
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .map(|(_, g)| g.pos())
                .chain(self.bodies.iter().map(|b| b.cell()))
                .collect();
            let Some(dir) = self.guards[i].decide(facility, &blocked) else {
                continue;
            };
            let Some(target) = self.guards[i].pos().step(dir) else {
                continue;
            };

            if target == self.player {
                // Capture is contact (§4.5) — but a concealed player is the one
                // exception: the occupied cupboard is solid and a patrol routes
                // *around* it, so contact is refused (§10.3, §7.6). The guard cannot
                // enter; it holds this turn. This is the "hold still, watch the cone
                // sweep past" payoff.
                if self.hidden() {
                    continue;
                }
                self.guards[i].place_at(target);
                self.outcome = Outcome::Lost;
                events.push(Event::Captured { by: target });
                return;
            }
            // A closed door does not stop a guard: its route runs straight through
            // (§10.3's deliberate closed-panel rule), and the walk-in is the bump
            // that opens it (§10.4) — the door is the guard's whole action this
            // turn; it steps through on a later one. Guard traffic opens the facility
            // up over a level; the Calm close-behind below is the counter-pressure
            // (§10.4/#146), not a symmetric one — an open door still spreads.
            if self.layout.facility().terrain(target) == Some(Terrain::DoorPanelClosed) {
                self.operate_door(target);
                events.push(Event::DoorOpened { at: target });
                continue;
            }
            // A guard moves onto a cell the terrain admits and no actor occupies. Its
            // own cell is a step behind `target`, so the mover is never in the way; the
            // player's cell was captured above but `occupied` still guards it.
            // `advance_to` refreshes the moved guard's cone at once, so the sight a
            // frame shows never lags the position it shows (§11.5); the next phase 2
            // recomputes everything anyway.
            if self.layout.facility().can_enter(target, ACTOR_FILL) && !self.occupied(target) {
                let from = self.guards[i].pos();
                let facility = self.layout.facility();
                self.guards[i].advance_to(target, dir, facility);
                // A guard arriving on the decoy's cell tramples it (§8.3):
                // walking into the "intruder" is how the fake is found out.
                self.stomp_decoy(target, events);
                // §10.4/#146: a Calm guard that has just stepped clear of a doorway
                // sometimes closes it behind itself. The guard is now off the panel
                // (it stands on `target`), so the crush check sees only anyone *else*
                // still in the throat — the player included — and refuses on them.
                if self.guards[i].closes_doors() {
                    if let Some(door) = self.door_exited(from, target) {
                        if self.rolls_a_close() && self.close_behind_door(door) {
                            events.push(Event::DoorClosed { at: from });
                        }
                    }
                }
            }
        }
    }

    /// The door a guard just walked *out of*, if its step from `from` to `to` left a
    /// doorway behind (§10.4/#146): `from` is one of that door's panels and `to` is
    /// no longer part of the same door. `None` otherwise — the guard was not on a
    /// panel, or it merely slid along a wide opening from one panel to another and is
    /// still in the throat. Only a **manual** door qualifies: an automatic door has no
    /// handle for a guard to shut and closes itself on a timer instead (§10.4/#147),
    /// so a guard passing through one leaves the auto-close to do the work.
    fn door_exited(&self, from: Cell, to: Cell) -> Option<DoorId> {
        let regions = self.layout.regions();
        let id = regions.door_at(from)?;
        let door = regions.door(id);
        if door.role(from) != Some(DoorCell::Panel) || door.is_automatic() {
            return None;
        }
        if regions.door_at(to) == Some(id) {
            return None; // still within the same doorway
        }
        Some(id)
    }

    /// Roll the seeded run RNG (§12.4) against the Calm close-behind chance
    /// (§10.4/§7.6). Draws nothing at the extremes — a `0` chance never closes and a
    /// `100` chance always does — so forcing the knob either way (a test, a playtest)
    /// leaves the rest of the stream untouched; only the tuned middle consumes a draw.
    fn rolls_a_close(&mut self) -> bool {
        match self.close_chance {
            0 => false,
            c if c >= 100 => true,
            c => self.rng.below(100) < c,
        }
    }

    /// Close `door` behind the guard that just left it (§10.4/#146), reporting whether
    /// it actually shut. Refuses — and returns `false` — when another actor still
    /// stands on a panel (the crush rule, §10.4), so a door never shuts on the player
    /// waiting in the throat. Fields are captured so the occupancy predicate can borrow
    /// them while `layout` is borrowed `&mut`, exactly as [`operate_door`] does.
    fn close_behind_door(&mut self, door: DoorId) -> bool {
        let player = self.player;
        let guards = &self.guards;
        let bodies = &self.bodies;
        matches!(
            self.layout
                .close_behind(door, |c| actor_occupies(player, guards, bodies, c)),
            Some(DoorAction::Closed)
        )
    }

    /// The index of a guard standing on `cell`, if any.
    fn guard_at(&self, cell: Cell) -> Option<usize> {
        self.guards.iter().position(|g| g.pos() == cell)
    }

    /// The index of a body lying on `cell`, if any.
    fn body_at(&self, cell: Cell) -> Option<usize> {
        self.bodies.iter().position(|b| b.cell() == cell)
    }

    /// Whether any actor occupies `cell` — the loop's single occupancy predicate.
    /// Actors are the player, the guards, and the bodies takedowns leave (§7.2);
    /// decoys and the rest fold in here (§4.3/§12.3) so occupancy is asked in one
    /// place and nothing is special-cased at the call sites.
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
/// new actor kinds arrive. A body counts (§7.2: solid, fill 1.0), which is also what
/// keeps a door from ever closing on one (§10.4 — doors never crush).
fn actor_occupies(player: Cell, guards: &[Guard], bodies: &[Body], cell: Cell) -> bool {
    player == cell
        || guards.iter().any(|g| g.pos() == cell)
        || bodies.iter().any(|b| b.cell() == cell)
}

#[cfg(test)]
mod tests;
