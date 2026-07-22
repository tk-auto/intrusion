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

use crate::ability::{AbilityId, AbilityState, Deck};
use crate::body::Body;
use crate::category::Category;
use crate::cell::{Cell, Direction};
use crate::facility::Terrain;
use crate::generate::Layout;
use crate::guard::Guard;
use crate::vision::{
    field_of_view_with_peek, VisibleSet, PLAYER_SIGHT_ARC, PLAYER_SIGHT_RANGE, WAIT_SIGHT_ARC,
};
use crate::DoorAction;

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
    /// now crouched, concealed from any viewer whose line of sight crosses the
    /// table at `behind`. Reported only when the crouch *engages* — re-bumping
    /// the table you are already behind is a free no-op. Waiting holds the
    /// pose; any other spent action stands you up, no special event.
    Crouched { behind: Cell },
    /// The player opened a closed door by bumping a panel (§4.3, §10.4).
    DoorOpened { at: Cell },
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
    /// A guard's cone covered a body (§7.2) — the loudest event in the game,
    /// fired once per body. The finder's alert is raised harder than a sighting
    /// raises it; the radio escalation is §7.3/§7.7's tickets.
    BodyFound { at: Cell },
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
            // is something you did or hold (§8), so it reads in the Owned band.
            Event::AbilityActivated { .. }
            | Event::AbilityDeactivated { .. }
            | Event::AbilityExpired { .. } => Category::Owned,
            // The takedown is something you did (§7.2) — your one offensive verb,
            // reading in the same band as your other tools.
            Event::TakenDown { .. } => Category::Owned,
            // A found body flips its finder to hunting (§7.2/§7.4): the threat is
            // aroused but does not have you — the Warning band.
            Event::BodyFound { .. } => Category::Warning,
            // Neutral furniture doing furniture things (§10.4).
            Event::DoorOpened { .. } => Category::System,
            // Goals and rewards — including the exit talking about the goal it
            // still refuses (§4.5) and the win itself.
            Event::IntelTaken { .. } | Event::ExitRefused | Event::Won => Category::Interest,
            // A threat that has you, literally (§4.5).
            Event::Captured { .. } => Category::Danger,
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
    /// matching the yellow `g` it points at.
    pub fn category(self) -> Category {
        match self {
            Affordance::Takedown => Category::Caution,
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
    /// A body (§7.2) — solid, and bumping it does nothing yet: grabbing it to
    /// drag is #103's verb.
    Body,
    /// The exit; `ready` is true iff every objective is in hand (win vs. refused).
    Exit { ready: bool },
    /// An objective console still holding its intel.
    Intel,
    /// A door cell whose bump is a real door action (open, close, or a crush-refused
    /// close). An open panel or closed hinge is *not* a door action — it classifies as
    /// [`BumpKind::Move`] or [`BumpKind::Solid`] instead, exactly as the bump resolves.
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
    /// Anything else solid — a wall or a closed hinge: a free bump (§4.4).
    Solid,
}

impl BumpKind {
    /// The §11.4 usable-line label for this interaction, or `None` when a bump does
    /// nothing worth offering — a guard, a solid cell, a held pose, or a close that
    /// would be refused (doors never crush, so it is never promised).
    fn affordance(self) -> Option<Affordance> {
        match self {
            BumpKind::Guard { aware: false } => Some(Affordance::Takedown),
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
            | BumpKind::Body
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
    /// The table the player is crouched behind (§10.3), set by bumping it and
    /// cleared by any spent action other than a Wait (waiting holds the pose).
    /// Always orthogonally adjacent by construction: the bump that sets it is a
    /// bump into an adjacent cell, and every action that could move the player
    /// clears it.
    crouched_behind: Option<Cell>,
    guards: Vec<Guard>,
    /// The bodies takedowns have left (§7.2) — solid entities the level owns
    /// (§12.3), each remembering its guard's post for the radio net (§7.3).
    bodies: Vec<Body>,
    /// Per-ability economy runtime (§8.2): activation, duration, and cooldown for
    /// each activated ability, stepped by the turn loop. The v1 set is available
    /// from the start (§8.3/#104), so this begins all-ready.
    abilities: Deck,
    objectives: Vec<Objective>,
    exit: Cell,
    turn: u32,
    outcome: Outcome,
    /// The events of the player's most recent action, free or spent — what the
    /// near line reads (§11.7: messages clear on the next action, so holding
    /// exactly one action's events *is* the clearing rule). Empty before the
    /// first input; frozen once the run ends, so the final message stays.
    last_events: Vec<Event>,
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
            crouched_behind: None,
            guards,
            bodies: Vec::new(),
            abilities: Deck::new(),
            objectives,
            exit,
            turn: 0,
            outcome: Outcome::Playing,
            last_events: Vec::new(),
        };
        // The level-start full turn (§4.2): sight and guards, no player phase.
        let _ = state.run_world_phases();
        state
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

    /// The table the player is crouched behind (§10.3), if any — always
    /// orthogonally adjacent. The renderer reads this to recolour the one
    /// concealing table to Owned (§11.3); everything rule-side goes through
    /// [`concealed_from`](Self::concealed_from).
    pub fn crouched_behind(&self) -> Option<Cell> {
        self.crouched_behind
    }

    /// Whether the player is concealed from a viewer standing at `viewer` — the
    /// per-viewer concealment query the guard AI's detection will AND in and the
    /// danger overlay already honours (§11.5: the overlay must not claim the
    /// player is seen while they are not).
    ///
    /// Two ways to be concealed:
    /// - **In a cupboard** ([`hidden`](Self::hidden)): omnidirectional — no
    ///   viewer anywhere detects the player (§10.3).
    /// - **Crouched behind a table** ([`crouched`](Self::crouched)): directional —
    ///   only from viewers whose line of sight crosses **the table the player
    ///   ducked behind** (not every table they happen to stand beside), i.e.
    ///   viewers in the quarter-plane that table faces: the viewer's offset from
    ///   the player leans at least as far *along* the player→table direction as
    ///   it strays perpendicular to it (a viewer exactly on the 45° diagonal
    ///   grazes the table's corner and still counts). Integer arithmetic
    ///   throughout, so it is exactly deterministic (§12.4).
    ///
    /// Concealment is not cover from *contact*: a guard can still walk into a
    /// crouched player and capture (§4.5). And it composes with sight, not
    /// replaces it — a viewer that cannot see the player's cell at all needs no
    /// concealing.
    pub fn concealed_from(&self, viewer: Cell) -> bool {
        if self.hidden() {
            return true;
        }
        let Some(cover) = self.crouched_behind else {
            return false;
        };
        let (px, py) = (i64::from(self.player.x), i64::from(self.player.y));
        let (vx, vy) = (i64::from(viewer.x) - px, i64::from(viewer.y) - py);
        let (dx, dy) = (i64::from(cover.x) - px, i64::from(cover.y) - py);
        // `(dx, dy)` is a unit cardinal: the viewer's components along and
        // across the player→table direction.
        let along = vx * dx + vy * dy;
        let across = (vx * dy - vy * dx).abs();
        along >= 1 && along >= across
    }

    /// The guards, for rendering and tests.
    pub fn guards(&self) -> &[Guard] {
        &self.guards
    }

    /// The bodies takedowns have left (§7.2), for rendering and tests.
    pub fn bodies(&self) -> &[Body] {
        &self.bodies
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
        let spent = self.player_phase(input, &mut events);

        if self.outcome == Outcome::Playing && spent {
            self.turn += 1;
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
            events.extend(
                expired
                    .into_iter()
                    .map(|ability| Event::AbilityExpired { ability }),
            );
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
                true
            }
            Input::Step(dir) => {
                let posture = self.crouched_behind;
                let spent = self.resolve_step(dir, events);
                // Only a *spent* action stands the player up / narrows the arc: a
                // free action changes nothing, not even posture (§4.4). The one
                // spent action that doesn't stand you up is the crouch itself —
                // recognisable as the action that changed the pose.
                if spent {
                    self.waited = false;
                    if self.crouched_behind == posture {
                        self.crouched_behind = None;
                    }
                }
                spent
            }
            // Activating an ability spends the turn (§4.4) — but only if it actually
            // switched on; activating an unavailable ability is a mis-input and, like
            // a wall bump, is free and changes nothing. A real activation is a spent
            // action other than Wait, so it stands the player up and narrows the arc.
            Input::Activate(id) => {
                if self.abilities.activate(id) {
                    events.push(Event::AbilityActivated { ability: id });
                    self.waited = false;
                    self.crouched_behind = None;
                    true
                } else {
                    false
                }
            }
            // Toggling an ability off is free (§4.4): it never spends the turn, so —
            // like every free action — it leaves posture and the waited flag alone.
            Input::Deactivate(id) => {
                if self.abilities.deactivate(id) {
                    events.push(Event::AbilityDeactivated { ability: id });
                }
                false
            }
        }
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
        match self.bump_kind(target) {
            // The takedown (§7.2): adjacent, against a guard that has not detected
            // the player this turn, costing the full turn. Permanent — the guard
            // is gone, and what remains is the body, which is the real cost. No
            // cooldown and no range: the constraints *are* the cost.
            BumpKind::Guard { aware: false } => {
                let i = self
                    .guard_at(target)
                    .expect("bump_kind classified a guard here");
                let guard = self.guards.remove(i);
                self.bodies.push(Body::new(target, guard.station()));
                events.push(Event::TakenDown { at: target });
                true
            }
            // An aware guard has you in its cone (§7.2's gate): the bump is a
            // free no-op — no half-takedown, no shove.
            BumpKind::Guard { aware: true } => {
                events.push(Event::Bumped { into: target });
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
                self.player = target;
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
            // *decision*, aimed at a specific table — concealment is across that table
            // only. The player does not move; the table stays solid furniture.
            BumpKind::Crouch => {
                self.crouched_behind = Some(target);
                events.push(Event::Crouched { behind: target });
                true
            }
            // Plain movement into a cell that admits the player.
            BumpKind::Move => {
                self.player = target;
                self.facing = dir; // facing follows the last successful step (§5)
                events.push(Event::Moved { to: target });
                true
            }
            // A body (grabbing it is #103's verb), a cupboard already holding an
            // actor, the table already crouched behind, or anything else solid
            // (a wall, a closed hinge): a free bump (§4.4).
            BumpKind::Body | BumpKind::HideoutBlocked | BumpKind::CrouchHeld | BumpKind::Solid => {
                events.push(Event::Bumped { into: target });
                false
            }
        }
    }

    /// Apply the door operation a bump triggers at `target` — the mutation half of a
    /// [`BumpKind::Door`] classification (the read-only verdict came from
    /// [`bump_kind`](Self::bump_kind)). Fields are captured so the occupancy predicate
    /// can borrow them while `layout` is borrowed `&mut`.
    fn operate_door(&mut self, target: Cell) {
        let player = self.player;
        let guards = &self.guards;
        let bodies = &self.bodies;
        self.layout
            .bump_door(target, |c| actor_occupies(player, guards, bodies, c));
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
        if let Some(i) = self.guard_at(target) {
            return BumpKind::Guard {
                aware: self.guards[i].detected_player(),
            };
        }
        if self.body_at(target).is_some() {
            return BumpKind::Body;
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
                if self.crouched_behind == Some(target) {
                    BumpKind::CrouchHeld
                } else {
                    BumpKind::Crouch
                }
            }
            _ if self.layout.facility().can_enter(target, ACTOR_FILL) => BumpKind::Move,
            _ => BumpKind::Solid,
        }
    }

    /// Phases 2 and 3 (§4.2): recompute sight, then let the guards act. Shared by the
    /// startup turn and every spent player turn.
    fn run_world_phases(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        self.recompute_sight();
        self.guard_phase(&mut events);
        events
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
            guard.sense(self.player, concealed);
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
            // A guard moves onto a cell the terrain admits and no actor occupies. Its
            // own cell is a step behind `target`, so the mover is never in the way; the
            // player's cell was captured above but `occupied` still guards it.
            // `advance_to` refreshes the moved guard's cone at once, so the sight a
            // frame shows never lags the position it shows (§11.5); the next phase 2
            // recomputes everything anyway.
            if self.layout.facility().can_enter(target, ACTOR_FILL) && !self.occupied(target) {
                let facility = self.layout.facility();
                self.guards[i].advance_to(target, dir, facility);
            }
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

    /// Whether any actor occupies `cell` — the loop's single occupancy predicate.
    /// Actors are the player, the guards, and the bodies takedowns leave (§7.2);
    /// decoys and the rest fold in here (§4.3/§12.3) so occupancy is asked in one
    /// place and nothing is special-cased at the call sites.
    fn occupied(&self, cell: Cell) -> bool {
        actor_occupies(self.player, &self.guards, &self.bodies, cell)
    }
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
mod tests {
    use super::*;
    use crate::guard::{GuardState, PATROL_RADIUS, SEARCH_RADIUS};
    use crate::test_support::{open_room, solo};
    use crate::vision::field_of_view;
    use crate::{generate, DoorId, Rng};

    #[test]
    fn a_move_into_open_floor_spends_the_turn_and_turns_the_player() {
        let mut s = solo(Cell::new(4, 4));
        let events = s.step(Input::Step(Direction::East));
        assert_eq!(
            events,
            vec![Event::Moved {
                to: Cell::new(5, 4)
            }]
        );
        assert_eq!(s.player(), Cell::new(5, 4));
        assert_eq!(s.facing(), Direction::East);
        assert_eq!(s.turn(), 1);
    }

    /// §4.4's load-bearing exception: bumping a wall is free — the turn does not
    /// advance, the player does not move, and facing is unchanged (§5).
    #[test]
    fn bumping_a_wall_is_free_and_does_not_advance_the_turn() {
        let mut s = solo(Cell::new(1, 1));
        let events = s.step(Input::Step(Direction::West)); // into the west wall
        assert_eq!(
            events,
            vec![Event::Bumped {
                into: Cell::new(0, 1)
            }]
        );
        assert_eq!(s.player(), Cell::new(1, 1), "no move");
        assert_eq!(s.facing(), Direction::North, "a blocked move keeps facing");
        assert_eq!(s.turn(), 0, "a free action does not spend the turn");
    }

    /// Waiting is a real action (§5): it spends the turn even though nothing moves.
    #[test]
    fn waiting_spends_the_turn() {
        let mut s = solo(Cell::new(4, 4));
        assert!(s.step(Input::Wait).is_empty());
        assert_eq!(s.turn(), 1);
        assert_eq!(s.player(), Cell::new(4, 4));
    }

    /// §4.4/§8.2: activating an ability is world-changing — it spends the turn and
    /// reports it (§11.7). By the time the panel reads it, the activation turn's
    /// end-of-turn tick has run, so 4 of Run's 5 remain — yet the activation turn
    /// itself was protected (the §8.2 N-yields-N−1 trap, designed out).
    #[test]
    fn activating_an_ability_spends_the_turn() {
        let mut s = solo(Cell::new(4, 4));
        let events = s.step(Input::Activate(AbilityId::Run));
        assert_eq!(
            events,
            vec![Event::AbilityActivated {
                ability: AbilityId::Run
            }]
        );
        assert_eq!(s.turn(), 1, "activation spends the turn");
        assert_eq!(
            s.ability_state(AbilityId::Run),
            AbilityState::Active { remaining: 4 },
        );
    }

    /// §4.4: toggling an ability off is one of the two free actions — the turn does
    /// not advance — and it still pays the full cooldown (§8.2 refunds nothing).
    #[test]
    fn toggling_an_ability_off_is_free() {
        let mut s = solo(Cell::new(4, 4));
        s.step(Input::Activate(AbilityId::Run)); // turn 1, Run active
        let events = s.step(Input::Deactivate(AbilityId::Run));
        assert_eq!(
            events,
            vec![Event::AbilityDeactivated {
                ability: AbilityId::Run
            }]
        );
        assert_eq!(s.turn(), 1, "toggling off does not spend the turn");
        assert_eq!(
            s.ability_state(AbilityId::Run),
            AbilityState::Cooling { remaining: 12 },
            "early cancel still pays the whole cooldown",
        );
    }

    /// Activating an ability that is not ready is a mis-input — free, like a wall
    /// bump (§4.4): nothing changes and the turn does not advance.
    #[test]
    fn activating_an_unavailable_ability_is_free() {
        let mut s = solo(Cell::new(4, 4));
        s.step(Input::Activate(AbilityId::Run)); // now active
        let events = s.step(Input::Activate(AbilityId::Run)); // already active
        assert!(events.is_empty(), "re-activating does nothing");
        assert_eq!(s.turn(), 1, "a mis-input is free");
    }

    /// The §8.2 timing convention through the whole loop: a freshly activated
    /// N-turn ability is protected for N turns — the activation turn included —
    /// then fades, and the full lockout is exactly `duration + cooldown` (Run: 5 +
    /// 12 = 17 turns), Ready again on the 18th.
    #[test]
    fn an_ability_is_protected_for_its_full_duration_then_locked_out() {
        let mut s = solo(Cell::new(4, 4));
        s.step(Input::Activate(AbilityId::Run)); // protected turn 1; tick 1 of 17
        assert_eq!(
            s.ability_state(AbilityId::Run),
            AbilityState::Active { remaining: 4 }
        );

        // Protected turns 2–4 keep it active; the 4th wait's tick ends the duration.
        for expected in [3, 2, 1] {
            assert!(s.step(Input::Wait).is_empty());
            assert_eq!(
                s.ability_state(AbilityId::Run),
                AbilityState::Active {
                    remaining: expected
                }
            );
        }
        let events = s.step(Input::Wait); // protected turn 5 ends here
        assert_eq!(
            events,
            vec![Event::AbilityExpired {
                ability: AbilityId::Run
            }]
        );
        assert_eq!(
            s.ability_state(AbilityId::Run),
            AbilityState::Cooling { remaining: 12 },
            "the frozen cooldown starts at its full 12",
        );

        // Cooldown drains one per turn: 11 more waits leave it locked, the 12th frees it.
        for _ in 0..11 {
            s.step(Input::Wait);
        }
        assert_ne!(
            s.ability_state(AbilityId::Run),
            AbilityState::Ready,
            "still cooling after 16 turns",
        );
        s.step(Input::Wait);
        assert_eq!(
            s.ability_state(AbilityId::Run),
            AbilityState::Ready,
            "Ready again after exactly duration + cooldown = 17 turns",
        );
    }

    /// Win path (§4.5): take every objective, then reach the exit. Bumping the exit
    /// with intel still out refuses and is free.
    #[test]
    fn win_requires_all_intel_then_the_exit() {
        // Player at (4,4); one intel at (5,4); exit at (4,5).
        let mut s = State::new(
            open_room(10, 10),
            Cell::new(4, 4),
            Direction::North,
            Vec::new(),
            [Cell::new(5, 4)],
            Cell::new(4, 5),
        );

        // Bumping the exit early: refused, free, still playing.
        let events = s.step(Input::Step(Direction::South));
        assert_eq!(events, vec![Event::ExitRefused]);
        assert_eq!(s.outcome(), Outcome::Playing);
        assert_eq!(s.turn(), 0);

        // Take the intel by bumping the console to the east.
        let events = s.step(Input::Step(Direction::East));
        assert_eq!(events, vec![Event::IntelTaken { remaining: 0 }]);
        assert_eq!(s.objectives_remaining(), 0);
        assert_eq!(
            s.player(),
            Cell::new(4, 4),
            "taking intel is a bump, not a move"
        );

        // Now the exit accepts.
        let events = s.step(Input::Step(Direction::South));
        assert_eq!(events, vec![Event::Won]);
        assert_eq!(s.outcome(), Outcome::Won);

        // A finished run is inert.
        assert!(s.step(Input::Step(Direction::North)).is_empty());
    }

    /// Loss (§4.5): a guard moving into the player's cell captures. Contact, not
    /// detection — the guard need not "see" anything.
    #[test]
    fn a_guard_stepping_into_the_player_captures() {
        // Guard at (6,4) heading west across the room; player at (4,4) in its path.
        // After the startup turn the guard is at (5,4); the player waits, and the
        // guard steps onto (4,4).
        let mut s = State::new(
            open_room(10, 10),
            Cell::new(4, 4),
            Direction::North,
            vec![Guard::patrolling_to(Cell::new(6, 4), Cell::new(1, 4))],
            Vec::new(),
            Cell::new(8, 8),
        );
        assert_eq!(
            s.guards()[0].pos(),
            Cell::new(5, 4),
            "startup turn moved the guard"
        );
        assert_eq!(s.outcome(), Outcome::Playing);

        let events = s.step(Input::Wait);
        assert_eq!(
            events,
            vec![Event::Captured {
                by: Cell::new(4, 4)
            }]
        );
        assert_eq!(s.outcome(), Outcome::Lost);
    }

    /// §7.2: the takedown. Bumping an adjacent guard that has not detected the
    /// player this turn removes it permanently, leaves a body at its cell, and
    /// costs the full turn. Concealment is how adjacency is arranged undetected —
    /// the touching ring otherwise always sees an adjacent player (§6.1) — so the
    /// strike comes from inside a cupboard. The usable line offers exactly this.
    #[test]
    fn an_unaware_adjacent_guard_is_taken_down_leaving_a_body() {
        let mut layout = open_room(10, 10);
        layout.place(Cell::new(5, 5), Terrain::Hideout);
        let mut s = State::new(
            layout,
            Cell::new(5, 5), // in the cupboard: concealed from the start
            Direction::North,
            vec![Guard::stationary(Cell::new(5, 4))],
            Vec::new(),
            Cell::new(8, 8),
        );
        assert!(s.hidden(), "precondition: concealed");
        assert!(
            !s.guards()[0].detected_player(),
            "precondition: the guard's look missed the hidden player",
        );
        assert_eq!(
            s.affordances(),
            vec![(Direction::North, Affordance::Takedown)],
            "the usable line offers the takedown (§11.4)",
        );

        let events = s.step(Input::Step(Direction::North));
        assert_eq!(
            events,
            vec![Event::TakenDown {
                at: Cell::new(5, 4)
            }]
        );
        assert!(s.guards().is_empty(), "the takedown is permanent");
        assert_eq!(s.bodies().len(), 1, "a body is left behind");
        assert_eq!(s.bodies()[0].cell(), Cell::new(5, 4));
        assert_eq!(s.turn(), 1, "a takedown costs the full turn");
        assert_eq!(
            s.player(),
            Cell::new(5, 5),
            "a takedown is a bump, not a move"
        );
    }

    /// §7.2's gate, enforced: a guard that **has** detected the player this turn
    /// refuses the takedown — the bump falls back to the free no-op, and the
    /// usable line never offered it.
    #[test]
    fn an_aware_guard_refuses_the_takedown() {
        let mut s = State::new(
            open_room(10, 10),
            Cell::new(5, 5),
            Direction::North,
            vec![Guard::stationary(Cell::new(5, 4))],
            Vec::new(),
            Cell::new(8, 8),
        );
        // The startup turn's touching ring saw the adjacent player (§6.1).
        assert!(s.guards()[0].detected_player(), "precondition: aware");
        assert_eq!(s.affordances(), Vec::new(), "no takedown is promised");

        let events = s.step(Input::Step(Direction::North));
        assert_eq!(
            events,
            vec![Event::Bumped {
                into: Cell::new(5, 4)
            }]
        );
        assert_eq!(s.guards().len(), 1, "the guard stands");
        assert!(s.bodies().is_empty());
        assert_eq!(s.turn(), 0, "a refused takedown is a free bump");
    }

    /// §7.2: a body does not block sight, so the first cone to cover it fires
    /// the found-body event — exactly once, found is found — and the finder goes
    /// hunting (the §7.6 search). The body is solid to the player: stepping into
    /// it is a free bump.
    #[test]
    fn a_body_is_found_once_by_a_covering_cone() {
        let mut layout = open_room(10, 10);
        layout.place(Cell::new(5, 5), Terrain::Hideout);
        let mut s = State::new(
            layout,
            Cell::new(5, 5), // hidden, striking north
            Direction::North,
            vec![
                Guard::stationary(Cell::new(5, 4)), // the victim, adjacent
                // A witness two cells up the column, cone south straight over
                // the victim's cell: it sees the body the turn it appears.
                Guard::stationary(Cell::new(5, 2)),
            ],
            Vec::new(),
            Cell::new(8, 8),
        );

        let body = Cell::new(5, 4);
        let events = s.step(Input::Step(Direction::North));
        assert_eq!(
            events,
            vec![Event::TakenDown { at: body }, Event::BodyFound { at: body }],
            "the witness's cone covers the fresh body: found the same turn",
        );
        assert_eq!(s.bodies()[0].cell(), body);
        assert!(s.bodies()[0].found());
        assert_eq!(
            s.guards()[0].state(),
            GuardState::Alerted,
            "the finder drops into the §7.6 search",
        );

        // Found is found: the cone keeps covering the body every turn, and the
        // loudest event in the game never repeats.
        for _ in 0..3 {
            let events = s.step(Input::Wait);
            assert!(
                !events.iter().any(|e| matches!(e, Event::BodyFound { .. })),
                "the found-body event fires exactly once per body",
            );
        }

        // Solid to the player: stepping into the body is a free bump.
        let turn = s.turn();
        let events = s.step(Input::Step(Direction::North));
        assert_eq!(events, vec![Event::Bumped { into: body }]);
        assert_eq!(s.player(), Cell::new(5, 5), "no move onto a body");
        assert_eq!(s.turn(), turn, "bumping a body is free");
    }

    /// §7.2: a body is solid to guards too — it blocks their movement and their
    /// pathing. A guard sent at the body's cell (and then hunting all around it)
    /// never stands on it.
    #[test]
    fn a_guard_never_enters_a_bodys_cell() {
        let mut layout = open_room(10, 10);
        layout.place(Cell::new(5, 5), Terrain::Hideout);
        let mut s = State::new(
            layout,
            Cell::new(5, 5),
            Direction::North,
            vec![
                Guard::stationary(Cell::new(5, 4)), // the victim
                // A walker aimed straight at the victim's cell.
                Guard::patrolling_to(Cell::new(5, 1), Cell::new(5, 4)),
            ],
            Vec::new(),
            Cell::new(8, 8),
        );

        let body = Cell::new(5, 4);
        s.step(Input::Step(Direction::North)); // the takedown; the body lies
        assert_eq!(s.bodies()[0].cell(), body);

        // The walker finds the body, searches all around it — and can never
        // stand on it: not routed through (pathing) and never entered (move).
        for _ in 0..12 {
            s.step(Input::Wait);
            assert_ne!(s.guards()[0].pos(), body, "a body's cell admits no guard");
            assert_eq!(s.outcome(), Outcome::Playing, "hidden all along");
        }
    }

    /// A patrolling guard with nowhere to sweep holds rather than wedging or
    /// panicking (§7.5). A patrol routes *around* walls, so the old "march into the
    /// wall forever" case cannot arise; the modern equivalent is a guard boxed into
    /// a single cell — its territory is just itself, so it never leaves.
    #[test]
    fn a_boxed_in_guard_has_nowhere_to_patrol_and_holds() {
        // Wall a guard into the single cell (2,2): all four neighbours are solid.
        let mut layout = open_room(10, 10);
        for wall in [
            Cell::new(1, 2),
            Cell::new(3, 2),
            Cell::new(2, 1),
            Cell::new(2, 3),
        ] {
            layout.place(wall, Terrain::Wall);
        }
        let mut s = State::new(
            layout,
            Cell::new(6, 6),
            Direction::North,
            vec![Guard::patrolling(Cell::new(2, 2))],
            Vec::new(),
            Cell::new(8, 8),
        );
        // Startup already ran a decide; a few more waits never move it off (2,2).
        for _ in 0..3 {
            s.step(Input::Wait);
        }
        assert_eq!(s.guards()[0].pos(), Cell::new(2, 2));
        assert_eq!(s.outcome(), Outcome::Playing);
    }

    /// §7.5: a Calm guard genuinely paces across its territory rather than shuffling
    /// by its spawn — over a patrol it reaches a cell well away from its station.
    #[test]
    fn a_calm_guard_paces_across_its_territory() {
        let station = Cell::new(15, 15);
        let mut s = State::new(
            open_room(30, 30),
            Cell::new(1, 28), // player parked in a far corner, out of the territory
            Direction::North,
            vec![Guard::patrolling(station)],
            Vec::new(),
            Cell::new(1, 1),
        );
        let mut farthest = 0;
        for _ in 0..40 {
            s.step(Input::Wait);
            farthest = farthest.max(station.manhattan_distance(s.guards()[0].pos()));
        }
        assert!(
            farthest > PATROL_RADIUS / 2,
            "the guard paced across its territory (reached {farthest} from station)",
        );
        assert_eq!(
            s.outcome(),
            Outcome::Playing,
            "the far player is never reached"
        );
    }

    /// Bumping a closed door opens it and spends the turn (§4.3, §10.4). Uses a
    /// generated facility, which is where real doors live: stand on a floor cell next
    /// to a panel and step into it.
    #[test]
    fn bumping_a_closed_door_opens_it() {
        let layout = generate(40, 40, &mut Rng::new(7)).unwrap();
        let (id, panel) = {
            let (id, door) = layout.regions().doors().next().unwrap();
            (id, door.panels()[0])
        };

        // One of the four orthogonal approaches stands on floor and bumps the panel.
        let opened = Direction::ALL.into_iter().any(|dir| {
            let Some(from) = panel.step(dir.opposite()) else {
                return false;
            };
            if !layout.facility().can_enter(from, ACTOR_FILL) {
                return false;
            }
            let mut s = State::new(
                layout.clone(),
                from,
                Direction::North,
                Vec::new(),
                Vec::new(),
                Cell::new(1, 1),
            );
            let opened = s.step(Input::Step(dir)) == vec![Event::DoorOpened { at: panel }];
            if opened {
                assert!(s.layout().regions().door(id).is_open());
                assert_eq!(s.turn(), 1, "opening a door spends the turn");
            }
            opened
        });
        assert!(opened, "one approach must bump the panel open");
    }

    /// §10.4: **a door never closes on an actor** — doors don't crush. Standing on a
    /// panel and bumping the hinge to shut the door must be refused, leaving the door
    /// open and the panel walk-through. (Regression: the close check once consulted
    /// only guards, so a player on a panel got shut in on themselves.)
    #[test]
    fn a_door_will_not_close_on_the_player() {
        // Find a door across seeds whose panel can be reached from a perpendicular
        // floor cell and has a hinge adjacent along the door line, then try to shut it
        // on ourselves.
        for seed in 0..64 {
            let layout = generate(40, 40, &mut Rng::new(seed)).unwrap();
            let Some((id, from, into, panel, hinge_dir)) = crush_scenario(&layout) else {
                continue;
            };

            // Exit parked on the border corner (always wall, never walked): a valid
            // Cell we never touch, so stamping it can't disturb the door.
            let mut s = State::new(
                layout,
                from,
                Direction::North,
                Vec::new(),
                Vec::new(),
                Cell::new(0, 0),
            );

            // Open the door, then step onto the now-open panel.
            assert_eq!(
                s.step(Input::Step(into)),
                vec![Event::DoorOpened { at: panel }]
            );
            assert_eq!(s.step(Input::Step(into)), vec![Event::Moved { to: panel }]);
            assert_eq!(s.player(), panel);

            // Bump the hinge to close: refused — we're on a panel. Nothing changes.
            let events = s.step(Input::Step(hinge_dir));
            assert!(events.is_empty(), "a refused close is a free no-op");
            assert!(
                s.layout().regions().door(id).is_open(),
                "seed {seed}: the door shut on the player"
            );
            assert_eq!(
                s.layout().facility().terrain(panel),
                Some(Terrain::DoorPanelOpen),
                "seed {seed}: the panel went solid under the player — crushed"
            );
            assert_eq!(s.player(), panel, "the player is unmoved and uncrushed");
            return;
        }
        panic!("no door with a reachable end panel found in 64 seeds");
    }

    /// A door setup for the crush test: a door id, the floor cell to start on, the
    /// direction to step into the panel, the end panel itself, and the direction from
    /// that panel to its adjacent hinge (what you bump to close).
    fn crush_scenario(layout: &Layout) -> Option<(DoorId, Cell, Direction, Cell, Direction)> {
        for (id, door) in layout.regions().doors() {
            let panel = door.panels()[0];
            // The end panel abuts a hinge; the door line runs panel→hinge.
            let Some(&hinge) = door
                .hinges()
                .iter()
                .find(|&&h| panel.manhattan_distance(h) == 1)
            else {
                continue;
            };
            let Some(hinge_dir) = Direction::between(panel, hinge) else {
                continue;
            };
            // Approach the panel perpendicular to the door line, from floor.
            for perp in hinge_dir.perpendicular() {
                let Some(from) = panel.step(perp) else {
                    continue;
                };
                let f = layout.facility();
                if f.terrain(from) == Some(Terrain::Floor) && f.can_enter(from, ACTOR_FILL) {
                    return Some((id, from, perp.opposite(), panel, hinge_dir));
                }
            }
        }
        None
    }

    /// §4.3/§10.3: a hideout is **bump-to-enter**, not a cell you drift onto. Stepping
    /// into an empty cupboard climbs in — the player occupies the cell, the turn is
    /// spent, and they are now [`hidden`](State::hidden). Entry auto-faces *out* of the
    /// cupboard, back toward the corridor (§7.6, #89) — the opposite of the entry bump —
    /// not into the wall the cupboard is recessed in.
    #[test]
    fn bumping_an_empty_hideout_enters_it_and_spends_the_turn() {
        let mut layout = open_room(10, 10);
        layout.place(Cell::new(5, 4), Terrain::Hideout);
        let mut s = State::new(
            layout,
            Cell::new(4, 4),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(8, 8),
        );
        assert!(!s.hidden(), "the player starts in the open");

        let events = s.step(Input::Step(Direction::East)); // bump the cupboard east
        assert_eq!(
            events,
            vec![Event::EnteredHideout {
                at: Cell::new(5, 4)
            }]
        );
        assert_eq!(s.player(), Cell::new(5, 4), "the player climbed in");
        assert_eq!(
            s.facing(),
            Direction::West,
            "entry faces out toward the corridor (§7.6), the opposite of the bump"
        );
        assert_eq!(s.turn(), 1, "entering spends the turn");
        assert!(s.hidden(), "the player is now concealed");
    }

    /// §7.6/§10.3/#89: a recessed cupboard's entry auto-faces the exit — the corridor
    /// side — so the ~180° half-disc (§6.2, arc 3) watches the flight path the moment
    /// you hide instead of the wall behind you. Fixture: a cupboard recessed into the
    /// top wall of a corridor, its only open face (the mouth) pointing south into the
    /// corridor. The player bumps in from the mouth (heading north) and must end facing
    /// south, seeing the corridor cells on *both* sides of the mouth.
    #[test]
    fn entering_a_hideout_faces_out_and_watches_the_corridor() {
        // Recess the cupboard at (5,3): walls on three sides, mouth (5,4) open to the
        // corridor row below.
        let mut layout = open_room(11, 11);
        for wall in [Cell::new(4, 3), Cell::new(6, 3), Cell::new(5, 2)] {
            layout.place(wall, Terrain::Wall);
        }
        layout.place(Cell::new(5, 3), Terrain::Hideout);
        let mut s = State::new(
            layout,
            Cell::new(5, 4), // in the corridor, at the cupboard mouth
            Direction::East, // arbitrary prior facing — entry must override it
            Vec::new(),
            Vec::new(),
            Cell::new(9, 9),
        );

        let events = s.step(Input::Step(Direction::North)); // bump north into the cupboard
        assert_eq!(
            events,
            vec![Event::EnteredHideout {
                at: Cell::new(5, 3)
            }]
        );
        assert!(s.hidden(), "the player is concealed");
        assert_eq!(
            s.facing(),
            Direction::South,
            "entry faces out (south) toward the corridor, not north into the wall"
        );

        // The 180° half-disc, facing the corridor, covers the mouth and the cells on
        // both sides of it — the sweep the hiding game is built around.
        for corridor_cell in [
            Cell::new(5, 4), // the mouth
            Cell::new(4, 4), // west of the mouth
            Cell::new(6, 4), // east of the mouth
            Cell::new(5, 5), // straight down the corridor
        ] {
            assert!(
                s.player_fov().contains(corridor_cell),
                "hiding must watch the corridor cell {corridor_cell:?}"
            );
        }

        // The auto-peek (#121): facing out means the head leans through the
        // mouth, so the corridor reads far past the flanking walls' wedge —
        // both directions — with no hideout special-case. The plain cast from
        // inside the recess cannot see these; the live FOV (peek-aware) must.
        let plain = field_of_view(
            s.layout.facility(),
            s.player(),
            s.facing(),
            PLAYER_SIGHT_ARC,
            PLAYER_SIGHT_RANGE,
        );
        for far_cell in [Cell::new(1, 4), Cell::new(9, 4)] {
            assert!(
                !plain.contains(far_cell),
                "{far_cell:?} is beyond the mouth's wedge for the plain cast"
            );
            assert!(
                s.player_fov().contains(far_cell),
                "the peek must read the corridor to {far_cell:?}"
            );
            assert!(
                s.memory().contains(far_cell),
                "peeked cells feed tile memory like any seen cell (§11.5a)"
            );
        }
    }

    /// #121: the auto-peek is the player's alone — one-sided by design. Around
    /// an L-corner the player reads the guard (**Seen**, the full picture — the
    /// lean is a real line of sight), while the guard's own plain cone cannot
    /// see the player back: no detection, no state change. A corner the player
    /// can read still breaks the guard's line, which is what keeps corners the
    /// player's flight tool (§7.6).
    #[test]
    fn the_peek_is_the_players_alone_a_guard_never_peeks() {
        let mut layout = open_room(11, 11);
        layout.place(Cell::new(4, 4), Terrain::Wall); // the corner block
        let mut guard = Guard::stationary(Cell::new(6, 3));
        // Face the guard straight at the corner — the worst case for the player.
        guard.advance_to(Cell::new(6, 3), Direction::West, layout.facility());
        let s = State::new(
            layout,
            Cell::new(3, 4), // one short of the corner, facing along it
            Direction::North,
            vec![guard],
            Vec::new(),
            Cell::new(9, 9),
        );

        let guard = &s.guards()[0];
        assert!(
            s.player_fov().contains(guard.pos()),
            "the peek shows the guard around the corner"
        );
        let plain = field_of_view(
            s.layout.facility(),
            s.player(),
            s.facing(),
            PLAYER_SIGHT_ARC,
            PLAYER_SIGHT_RANGE,
        );
        assert!(
            !plain.contains(guard.pos()),
            "the corner hides the guard from the body's own cast — the delta is the peek"
        );
        assert_eq!(
            s.perceive_guard(guard),
            Some(GuardPerception::Seen),
            "a peeked guard is Seen, cone and all, not the sensed dot"
        );
        assert!(
            !guard.fov().contains(s.player()),
            "the guard's plain cone must not read around the corner"
        );
        assert_eq!(
            guard.state(),
            GuardState::Calm,
            "seeing a guard through the peek is information, never detection"
        );
    }

    /// §4.3/§10.3: "move off to climb out." Stepping from a hideout onto floor is an
    /// ordinary move that clears the hidden state — no special key, no special event.
    #[test]
    fn moving_off_a_hideout_climbs_out() {
        let mut layout = open_room(10, 10);
        layout.place(Cell::new(5, 4), Terrain::Hideout);
        let mut s = State::new(
            layout,
            Cell::new(5, 4), // start already inside the cupboard
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(8, 8),
        );
        assert!(s.hidden(), "starting inside the cupboard is concealed");

        let events = s.step(Input::Step(Direction::West)); // step out onto floor
        assert_eq!(
            events,
            vec![Event::Moved {
                to: Cell::new(4, 4)
            }],
            "climbing out is an ordinary move"
        );
        assert_eq!(s.player(), Cell::new(4, 4));
        assert_eq!(
            s.facing(),
            Direction::West,
            "climbing out follows the step (§5) — only entry auto-faces (#89)"
        );
        assert!(!s.hidden(), "leaving clears the concealment");
    }

    /// §4.5/§7.6/§10.3: a concealed player is contact-safe. A guard patrolling into the
    /// player's cell captures in the open, but a cupboard is the one place contact is
    /// refused — the guard cannot enter, holds, and the run goes on. This is the
    /// "watch the cone sweep past" payoff; the same guard *would* capture if the player
    /// were not hidden (see [`a_guard_stepping_into_the_player_captures`]).
    #[test]
    fn a_guard_cannot_capture_a_hidden_player() {
        let mut layout = open_room(10, 10);
        layout.place(Cell::new(4, 4), Terrain::Hideout);
        // Guard at (6,4) sent to the cupboard cell (4,4) where the player hides.
        // After the startup turn the guard is at (5,4), one step from the player's
        // cell — the destination it will be refused entry to.
        let mut s = State::new(
            layout,
            Cell::new(4, 4),
            Direction::North,
            vec![Guard::patrolling_to(Cell::new(6, 4), Cell::new(4, 4))],
            Vec::new(),
            Cell::new(8, 8),
        );
        assert!(s.hidden());
        assert_eq!(
            s.guards()[0].pos(),
            Cell::new(5, 4),
            "startup moved the guard"
        );

        // The guard tries to step onto the player's cell: contact refused. It holds at
        // (5,4), no capture, still playing.
        let events = s.step(Input::Wait);
        assert!(
            !events.iter().any(|e| matches!(e, Event::Captured { .. })),
            "a hidden player is not captured"
        );
        assert_eq!(s.outcome(), Outcome::Playing, "the run continues");
        assert_eq!(
            s.guards()[0].pos(),
            Cell::new(5, 4),
            "the guard cannot enter the occupied cupboard"
        );
    }

    /// §7.6 fix 2 (Lost → Hunted → Released): a guard that loses sight of the player
    /// walks to the last-known cell, **searches** the area (Alerted) rather than snapping
    /// back to patrol, and only then releases to Calm and moves on. The player waits it
    /// out concealed in a cupboard: it is never captured, watches the guard search, and
    /// watches it leave — the payoff §14 exists to test.
    #[test]
    fn a_hidden_player_waits_out_a_search_and_watches_the_guard_leave() {
        let mut layout = open_room(16, 12);
        layout.place(Cell::new(4, 5), Terrain::Hideout); // a cupboard beside the player
                                                         // Guard at (5,1) facing south: its cone covers the player at (5,5), four cells
                                                         // down — the certain zone — so it detects and chases at the startup turn.
        let guards = vec![Guard::patrolling(Cell::new(5, 1))];
        let mut s = State::new(
            layout,
            Cell::new(5, 5),
            Direction::North,
            guards,
            Vec::new(),
            Cell::new(14, 10),
        );
        assert_eq!(
            s.guards()[0].state(),
            GuardState::Chasing,
            "the guard spots the player at spawn",
        );

        // The player ducks west into the cupboard and holds. The guard loses sight.
        s.step(Input::Step(Direction::West));
        assert!(s.hidden(), "the player is concealed");

        let focus = Cell::new(5, 5); // where the guard last knew the player
        let (mut searched, mut released, mut left_the_area) = (false, false, false);
        for _ in 0..60 {
            s.step(Input::Wait);
            assert_eq!(
                s.outcome(),
                Outcome::Playing,
                "a hidden player is never caught"
            );
            match s.guards()[0].state() {
                GuardState::Alerted => searched = true,
                GuardState::Calm if searched => {
                    released = true;
                    if s.guards()[0].pos().sight_distance(focus) > SEARCH_RADIUS {
                        left_the_area = true;
                    }
                }
                _ => {}
            }
        }
        assert!(
            searched,
            "the guard searched the area (Alerted) instead of giving up"
        );
        assert!(released, "the search released back to Calm patrol");
        assert!(
            left_the_area,
            "after releasing, the guard leaves the search area"
        );
        assert!(
            s.hidden(),
            "the player rode the whole search out from cover"
        );
    }

    /// §7.8: guards are solid to each other but **path around** a colleague instead of
    /// pathing through, failing the step, and stalling — the old deadlock. Two guards
    /// sweep a 2-wide corridor toward destinations past one another; they must pass
    /// (one drops to the parallel lane) without ever sharing a cell. The player waits,
    /// concealed off the corridor, so the sweep runs untouched.
    #[test]
    fn two_guards_meeting_in_a_corridor_pass_without_deadlock() {
        // A 2-wide corridor (rows 1–2) across a box; row 3 is wall except a recessed
        // cupboard the player hides in, off the guards' lanes.
        let mut layout = open_room(12, 5);
        for x in 1..=10 {
            layout.place(Cell::new(x, 3), Terrain::Wall);
        }
        layout.place(Cell::new(5, 3), Terrain::Hideout);
        let guards = vec![
            Guard::patrolling_to(Cell::new(1, 1), Cell::new(10, 1)),
            Guard::patrolling_to(Cell::new(10, 1), Cell::new(1, 1)),
        ];
        let mut s = State::new(
            layout,
            Cell::new(5, 3), // concealed in the cupboard, out of the lanes
            Direction::North,
            guards,
            Vec::new(),
            Cell::new(1, 3),
        );
        assert!(s.hidden(), "the player watches from cover");

        let mut passed = false;
        for turn in 0..40 {
            s.step(Input::Wait);
            let (a, b) = (s.guards()[0].pos(), s.guards()[1].pos());
            assert_ne!(a, b, "turn {turn}: guards must never share a cell (§7.8)");
            assert_eq!(s.outcome(), Outcome::Playing, "turn {turn}: no capture");
            // They start with a.x < b.x; passing swaps that order — the proof the
            // head-on meet resolved instead of deadlocking.
            if a.x > b.x {
                passed = true;
            }
        }
        assert!(
            passed,
            "the guards deadlocked instead of pathing around each other (§7.8)"
        );
    }

    /// §10.3: **bumping a table is the crouch** — ducking is a decision aimed at
    /// a specific table, like the cupboard's bump-to-enter. It spends the turn,
    /// reports once as the crouch engages, does not move the player, and
    /// re-bumping the same table is a free no-op. Waiting holds the pose; a
    /// plain wait away from cover crouches nothing.
    #[test]
    fn bumping_a_table_crouches_once() {
        let mut layout = open_room(10, 10);
        layout.place(Cell::new(5, 4), Terrain::PartialCover);
        let mut s = State::new(
            layout,
            Cell::new(4, 4),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(8, 8),
        );
        assert!(!s.crouched(), "standing until the table is bumped");
        s.step(Input::Wait);
        assert!(!s.crouched(), "waiting beside a table no longer crouches");

        let turn = s.turn();
        let events = s.step(Input::Step(Direction::East)); // bump the table
        assert_eq!(
            events,
            vec![Event::Crouched {
                behind: Cell::new(5, 4)
            }]
        );
        assert!(s.crouched());
        assert_eq!(s.crouched_behind(), Some(Cell::new(5, 4)));
        assert_eq!(s.player(), Cell::new(4, 4), "the crouch does not move you");
        assert_eq!(s.turn(), turn + 1, "the crouch spends the turn");

        // Waiting on: still crouched, nothing repeated.
        assert!(s.step(Input::Wait).is_empty());
        assert!(s.crouched());

        // Re-bumping the table you are already behind is a free no-op (§4.4).
        let turn = s.turn();
        let events = s.step(Input::Step(Direction::East));
        assert_eq!(
            events,
            vec![Event::Bumped {
                into: Cell::new(5, 4)
            }]
        );
        assert_eq!(s.turn(), turn, "a re-bump is free");
        assert!(s.crouched(), "and it does not break the crouch");
    }

    /// §10.3: **any spent action but a wait stands the player up** — the crouch
    /// is a posture, not a place — while a *free* action (a wall bump) changes
    /// nothing, not even posture (§4.4): the world does not move, so neither
    /// does the crouch.
    #[test]
    fn a_spent_step_stands_up_but_a_free_bump_does_not() {
        let mut layout = open_room(10, 10);
        layout.place(Cell::new(1, 2), Terrain::PartialCover);
        let mut s = State::new(
            layout,
            Cell::new(1, 1), // in the corner: west and north are wall
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(8, 8),
        );
        s.step(Input::Step(Direction::South)); // bump the table below: crouch
        assert!(s.crouched());

        // A mis-input into the wall is free: still crouched, turn unspent.
        let turn = s.turn();
        s.step(Input::Step(Direction::West));
        assert_eq!(s.turn(), turn, "a wall bump is free");
        assert!(s.crouched(), "a free action does not break the crouch");

        // A real step stands up — even though the new cell is still beside cover.
        s.step(Input::Step(Direction::East));
        assert!(!s.crouched(), "moving stands the player up");
    }

    /// §10.3: crouch concealment is **directional** — the table covers the
    /// quarter-plane it faces. A viewer across the cover (straight or leaning up
    /// to the 45° diagonal) is blinded; a viewer on the flank or behind the
    /// player is not; and without the crouch the same table conceals nothing.
    #[test]
    fn crouch_conceals_only_across_the_cover() {
        let mut layout = open_room(12, 12);
        layout.place(Cell::new(5, 4), Terrain::PartialCover); // east of the player
        let mut s = State::new(
            layout,
            Cell::new(4, 4),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(10, 10),
        );

        // Standing, the table conceals from no one.
        assert!(!s.concealed_from(Cell::new(7, 4)));

        s.step(Input::Step(Direction::East)); // bump the table: crouch
        assert!(s.crouched());
        // Straight across the table, near and far: concealed.
        assert!(s.concealed_from(Cell::new(6, 4)));
        assert!(s.concealed_from(Cell::new(9, 4)));
        // Leaning, within the quarter-plane (along ≥ across): concealed —
        // including the exact 45° diagonal, which grazes the table's corner.
        assert!(s.concealed_from(Cell::new(6, 3)));
        assert!(s.concealed_from(Cell::new(6, 2)));
        // Past the diagonal — the flank, the perpendicular, and behind: seen.
        assert!(!s.concealed_from(Cell::new(5, 2)));
        assert!(!s.concealed_from(Cell::new(4, 2)));
        assert!(!s.concealed_from(Cell::new(2, 4)));
    }

    /// §10.3: the cupboard is the stronger hide — omnidirectional. A hidden
    /// player is concealed from every direction, cover or none.
    #[test]
    fn a_hidden_player_is_concealed_from_every_direction() {
        let mut layout = open_room(10, 10);
        layout.place(Cell::new(4, 4), Terrain::Hideout);
        let s = State::new(
            layout,
            Cell::new(4, 4),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(8, 8),
        );
        assert!(s.hidden());
        for viewer in [
            Cell::new(4, 1),
            Cell::new(7, 4),
            Cell::new(4, 7),
            Cell::new(1, 4),
            Cell::new(6, 6),
        ] {
            assert!(
                s.concealed_from(viewer),
                "hidden must conceal from {viewer:?}"
            );
        }
    }

    /// §4.5: the crouch hides you from *sight*, not from *contact* — unlike the
    /// cupboard, a guard walking into a crouched player still captures. Being
    /// unseen is not being safe.
    #[test]
    fn a_crouched_player_is_still_captured_by_contact() {
        let mut layout = open_room(10, 10);
        layout.place(Cell::new(4, 3), Terrain::PartialCover); // cover to the north
        let mut s = State::new(
            layout,
            Cell::new(4, 4),
            Direction::North,
            vec![Guard::patrolling_to(Cell::new(6, 4), Cell::new(1, 4))],
            Vec::new(),
            Cell::new(8, 8),
        );
        assert_eq!(
            s.guards()[0].pos(),
            Cell::new(5, 4),
            "startup moved the guard"
        );

        // The bump crouches the player — and hands the guard its step into them.
        let events = s.step(Input::Step(Direction::North));
        assert!(events.contains(&Event::Crouched {
            behind: Cell::new(4, 3)
        }));
        assert!(
            events.contains(&Event::Captured {
                by: Cell::new(4, 4)
            }),
            "contact captures a crouched player"
        );
        assert_eq!(s.outcome(), Outcome::Lost);
    }

    /// §4.2: the startup turn establishes sight before the first input. A freshly
    /// built [`State`] already carries the player's half-disc and every guard's cone
    /// — and a guard that has not moved is looking **south**, its initial facing
    /// (§7.1).
    #[test]
    fn the_startup_turn_establishes_sight() {
        let s = State::new(
            open_room(12, 12),
            Cell::new(5, 5),
            Direction::North,
            vec![Guard::stationary(Cell::new(8, 8))],
            Vec::new(),
            Cell::new(10, 10),
        );

        // The player faces north: two ahead is lit, two directly behind is not (§6.2).
        assert!(s.player_fov().contains(Cell::new(5, 3)));
        assert!(!s.player_fov().contains(Cell::new(5, 7)));

        // The stationary guard looks south from spawn (§7.1): its wedge covers two
        // south, not two north.
        let g = &s.guards()[0];
        assert_eq!(g.facing(), Direction::South);
        assert!(g.fov().contains(Cell::new(8, 10)));
        assert!(!g.fov().contains(Cell::new(8, 6)));
    }

    /// §8.3: **Wait grants 360° vision for that turn** — the only way to see behind
    /// you (§5). The widened arc lasts until the next spent turn narrows it again.
    #[test]
    fn waiting_widens_sight_to_the_full_circle() {
        let mut s = solo(Cell::new(5, 5));
        s.step(Input::Step(Direction::North)); // now at (5,4), facing north

        let behind = Cell::new(5, 6); // two cells directly behind
        assert!(
            !s.player_fov().contains(behind),
            "the half-disc does not see directly behind"
        );

        s.step(Input::Wait);
        assert!(
            s.player_fov().contains(behind),
            "a turn spent waiting sees behind"
        );

        s.step(Input::Step(Direction::West)); // at (4,4), facing west; behind is east
        assert!(
            !s.player_fov().contains(Cell::new(6, 4)),
            "moving narrows the arc back to the half-disc"
        );
    }

    /// §11.5a: tile memory is the running union of every FOV the player has had —
    /// seeded by the startup turn, grown each sight phase, and never forgetting a
    /// cell that has since fallen out of view. It is derived purely from the FOV
    /// sequence, so it is as deterministic as the loop itself.
    #[test]
    fn tile_memory_accumulates_and_never_forgets() {
        let mut s = solo(Cell::new(5, 5)); // facing north
        let ahead = Cell::new(5, 3);
        assert!(s.player_fov().contains(ahead));
        assert!(s.memory().contains(ahead), "the startup turn seeds memory");

        // Turn around: (5,3) falls out of the FOV but stays in memory.
        s.step(Input::Step(Direction::South)); // to (5,6), facing south
        assert!(
            !s.player_fov().contains(ahead),
            "now behind, out of the FOV"
        );
        assert!(s.memory().contains(ahead), "memory keeps what the FOV lost");
    }

    /// §4.2's design note, honoured: there is **no one-turn sensory lag**. The sight
    /// phase runs after the player's move, so the stored FOV is always from the
    /// player's current position and facing.
    #[test]
    fn sight_is_recomputed_from_the_players_new_position_and_facing() {
        let mut s = State::new(
            open_room(12, 12),
            Cell::new(5, 5),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(10, 10),
        );
        // Facing north, the side line runs west: (2,5) is lit.
        assert!(s.player_fov().contains(Cell::new(2, 5)));

        s.step(Input::Step(Direction::East)); // now at (6,5), facing east
        assert!(
            s.player_fov().contains(Cell::new(9, 5)),
            "the cone points down the new facing"
        );
        assert!(
            !s.player_fov().contains(Cell::new(2, 5)),
            "what fell directly behind went dark this same turn"
        );
    }

    /// Guards: **facing follows the successful step** (§5, for guards as for the
    /// player), and a moved guard's stored cone is current when the turn ends — the
    /// frame never shows a guard in one place with its sight in another (§11.5).
    #[test]
    fn a_moved_guards_cone_is_current_when_the_turn_ends() {
        let mut s = State::new(
            open_room(12, 12),
            // Parked in the north-east, well behind the westbound guard's cone, so
            // detection (§7.6) never derails the patrol whose cone this test measures.
            Cell::new(10, 1),
            Direction::South,
            vec![Guard::patrolling_to(Cell::new(8, 8), Cell::new(1, 8))],
            Vec::new(),
            Cell::new(10, 10),
        );
        // The startup turn already walked the guard one west and turned it.
        let g = &s.guards()[0];
        assert_eq!(g.pos(), Cell::new(7, 8));
        assert_eq!(g.facing(), Direction::West);
        assert!(g.fov().contains(Cell::new(5, 8)), "the wedge points west");
        assert!(!g.fov().contains(Cell::new(9, 8)), "not behind it");

        s.step(Input::Wait);
        let g = &s.guards()[0];
        assert_eq!(g.pos(), Cell::new(6, 8));
        assert!(
            g.fov().contains(Cell::new(4, 8)) && !g.fov().contains(Cell::new(8, 8)),
            "the cone moved with the guard this very turn"
        );
    }

    /// The §7.4 colour column, pinned in §11.2's vocabulary: Calm is the unaware
    /// threat, Alerted and Responding are hunting, Chasing and Investigating have
    /// you. If a state's category moves, this test is where the change is owned.
    #[test]
    fn guard_states_declare_the_7_4_categories() {
        use crate::category::Category;
        assert_eq!(GuardState::Calm.category(), Category::Caution);
        assert_eq!(GuardState::Alerted.category(), Category::Warning);
        assert_eq!(GuardState::Responding.category(), Category::Warning);
        assert_eq!(GuardState::Chasing.category(), Category::Danger);
        assert_eq!(GuardState::Investigating.category(), Category::Danger);
        // A guard carries its state: Calm by default, overridable for scenarios.
        assert_eq!(Guard::stationary(Cell::new(1, 1)).state(), GuardState::Calm);
        let chasing = Guard::stationary(Cell::new(1, 1)).with_state(GuardState::Chasing);
        assert_eq!(chasing.state().category(), Category::Danger);
    }

    /// Events speak the same §11.2 table as the glyphs, so the message ticket can
    /// colour its bar without inventing meanings: taking intel is Interest, the
    /// capture is Danger, a step is routine Neutral.
    #[test]
    fn events_declare_their_message_category() {
        use crate::category::Category;
        let at = Cell::new(2, 3);
        assert_eq!(Event::Moved { to: at }.category(), Category::Neutral);
        assert_eq!(Event::Bumped { into: at }.category(), Category::Neutral);
        assert_eq!(Event::EnteredHideout { at }.category(), Category::Owned);
        assert_eq!(Event::Crouched { behind: at }.category(), Category::Owned);
        assert_eq!(Event::DoorOpened { at }.category(), Category::System);
        assert_eq!(
            Event::IntelTaken { remaining: 1 }.category(),
            Category::Interest
        );
        assert_eq!(Event::ExitRefused.category(), Category::Interest);
        assert_eq!(Event::Won.category(), Category::Interest);
        assert_eq!(Event::Captured { by: at }.category(), Category::Danger);
        assert_eq!(Event::TakenDown { at }.category(), Category::Owned);
        assert_eq!(Event::BodyFound { at }.category(), Category::Warning);
    }

    /// §12.4: the loop is pure and deterministic. The same starting state and the same
    /// input sequence produce an identical event stream and identical final state —
    /// the property that makes a run a `(seed, [inputs])` replay. The loop holds no
    /// randomness of its own, so this is structural, but the test pins it against a
    /// future change (a stray `HashMap` order, a clock read) that would break it.
    #[test]
    fn same_state_and_inputs_replay_identically() {
        let inputs = [
            Input::Step(Direction::East), // bump the console east: take the intel
            Input::Step(Direction::North),
            Input::Wait,
            Input::Step(Direction::West),
            Input::Step(Direction::South),
            Input::Step(Direction::South),
        ];

        let run = || {
            // Player, one intel to the east, a patrolling guard, exit to the south.
            let mut s = State::new(
                open_room(12, 12),
                Cell::new(5, 5),
                Direction::North,
                vec![Guard::patrolling(Cell::new(8, 5))],
                [Cell::new(6, 5)],
                Cell::new(5, 6),
            );
            let events: Vec<Event> = inputs.iter().flat_map(|&i| s.step(i)).collect();
            (
                events,
                s.player(),
                s.facing(),
                s.turn(),
                s.outcome(),
                s.objectives_remaining(),
                s.guards()[0].pos(),
                s.player_fov().clone(),
                s.memory().clone(),
            )
        };

        assert_eq!(run(), run(), "same state + inputs must replay identically");
    }

    /// The usable line's contract (§11.4): [`State::affordances`] offers exactly
    /// what a bump would do. A live console reads `TakeIntel`; once taken it is
    /// just solid and offers nothing; the exit answers by whether the intel is
    /// in hand; an empty cupboard offers `Hide`.
    #[test]
    fn affordances_mirror_what_a_bump_would_do() {
        let mut layout = open_room(12, 12);
        layout.place(Cell::new(4, 5), Terrain::Hideout);
        let mut s = State::new(
            layout,
            Cell::new(5, 5),
            Direction::North,
            Vec::new(),
            [Cell::new(6, 5)], // a console east
            Cell::new(5, 4),   // the exit north
        );

        // Console east, exit north (intel still out), cupboard west — each with
        // the direction to bump it.
        assert_eq!(
            s.affordances(),
            vec![
                (Direction::North, Affordance::ExitRefused),
                (Direction::East, Affordance::TakeIntel),
                (Direction::West, Affordance::Hide)
            ],
            "Direction::ALL order: north, east, … west"
        );

        // Take the intel: the console goes solid and the exit opens up.
        s.step(Input::Step(Direction::East));
        assert_eq!(
            s.affordances(),
            vec![
                (Direction::North, Affordance::Leave),
                (Direction::West, Affordance::Hide)
            ],
            "a spent console offers nothing; the exit now offers the win"
        );

        // In the middle of open floor, the line is empty.
        let s = solo(Cell::new(4, 4));
        assert_eq!(s.affordances(), Vec::new());
    }

    /// An adjacent **aware** guard offers nothing: its bump is a free no-op
    /// (§7.2's gate — the unaware case is the takedown test above), and the
    /// usable line must never promise what a bump will not do (§2.3). An
    /// occupied cupboard is likewise just solid.
    #[test]
    fn affordances_skip_guards_and_occupied_hideouts() {
        let mut layout = open_room(12, 12);
        layout.place(Cell::new(5, 4), Terrain::Hideout);
        let mut s = State::new(
            layout,
            Cell::new(5, 5),
            Direction::North,
            vec![Guard::stationary(Cell::new(6, 5))], // east of the player
            Vec::new(),
            Cell::new(10, 10),
        );
        // Enter the cupboard north; the guard east never shows.
        assert_eq!(s.affordances(), vec![(Direction::North, Affordance::Hide)]);
        s.step(Input::Step(Direction::North));
        assert!(s.hidden());

        // From inside, the cupboard's own cell is the player's — and stepping
        // back out is a plain move, not an affordance.
        assert_eq!(s.affordances(), Vec::new());
    }

    /// Door affordances speak the §10.4 door graph: a closed panel offers the
    /// open; an open hinge offers the close — except while an actor stands on a
    /// panel, when the close would be refused (doors never crush) and so is
    /// never offered.
    #[test]
    fn door_affordances_track_pose_and_obstruction() {
        for seed in 0..64 {
            let layout = generate(40, 40, &mut Rng::new(seed)).unwrap();
            let Some((_, from, into, panel, hinge_dir)) = crush_scenario(&layout) else {
                continue;
            };
            // The hinge's floor neighbour on the player's side of the wall: the
            // cell to close the door from. `side` steps off the door line back
            // toward `from`'s side.
            let hinge = panel.step(hinge_dir).expect("hinge adjacent to panel");
            let side = Direction::between(panel, from).expect("from is beside the panel");
            let Some(beside_hinge) = hinge.step(side) else {
                continue;
            };
            let f = layout.facility();
            if f.terrain(beside_hinge) != Some(Terrain::Floor)
                || !f.can_enter(beside_hinge, ACTOR_FILL)
            {
                continue;
            }

            let mut s = State::new(
                layout,
                from,
                Direction::North,
                Vec::new(),
                Vec::new(),
                Cell::new(0, 0), // border corner: never walked, never bumped
            );
            let offers =
                |s: &State, want: Affordance| s.affordances().iter().any(|&(_, a)| a == want);
            assert!(
                offers(&s, Affordance::OpenDoor),
                "seed {seed}: a closed panel offers the open"
            );
            assert!(!offers(&s, Affordance::CloseDoor));

            // Open it, then stand on the panel: the close would be refused, so
            // the hinge offers nothing.
            s.step(Input::Step(into));
            s.step(Input::Step(into));
            assert_eq!(s.player(), panel);
            assert!(
                !offers(&s, Affordance::CloseDoor),
                "seed {seed}: no close offered while standing on the panel"
            );

            // Step back off the panel, then along the wall to sit beside the
            // hinge: now the close is a real offer.
            s.step(Input::Step(side));
            s.step(Input::Step(hinge_dir));
            assert_eq!(s.player(), beside_hinge);
            assert!(
                offers(&s, Affordance::CloseDoor),
                "seed {seed}: an open hinge offers the close"
            );
            assert!(!offers(&s, Affordance::OpenDoor));
            return;
        }
        panic!("no usable door scenario found in 64 seeds");
    }

    /// §9 **[SETTLED]**: guards detect on **vision only** — they do not hear. A player
    /// who scrambles into a cupboard right beside a guard, concealed from its sight
    /// (§10.3), is *not* detected: with the old hearing branch gone, a guard the player
    /// could once "give away a footstep to" now stays **Calm**. This is the inverse of
    /// the deleted hearing test, pinning the new rule.
    #[test]
    fn a_guard_that_cannot_see_the_hidden_player_stays_calm() {
        let mut layout = open_room(10, 10);
        layout.place(Cell::new(5, 4), Terrain::Hideout);
        let mut s = State::new(
            layout,
            Cell::new(4, 4),
            Direction::North,
            vec![Guard::stationary(Cell::new(6, 4))],
            Vec::new(),
            Cell::new(8, 8),
        );
        assert_eq!(s.guards()[0].state(), GuardState::Calm);

        // Step East into the cupboard at (5,4), one cell from the guard: the hideout
        // conceals the player from its sight, and nothing is heard — so it stays Calm.
        s.step(Input::Step(Direction::East));
        assert!(s.hidden(), "the player scrambled into the cupboard");
        assert_eq!(
            s.guards()[0].state(),
            GuardState::Calm,
            "guards detect on vision only — a hidden player is not seen, and not heard",
        );
    }

    /// A player out of every cone alerts no one: standing two cells behind a
    /// south-facing guard's back — past the touching ring and out of its wedge — the
    /// player is not seen, so the guard stays Calm however they act (§9 — there is no
    /// hearing to give them away either).
    #[test]
    fn an_unseen_player_alerts_no_one() {
        let mut s = State::new(
            open_room(10, 10),
            Cell::new(5, 2), // two north of the south-facing guard: directly behind it
            Direction::North,
            vec![Guard::stationary(Cell::new(5, 4))],
            Vec::new(),
            Cell::new(8, 8),
        );
        assert_eq!(
            s.guards()[0].state(),
            GuardState::Calm,
            "unseen at the start"
        );
        s.step(Input::Wait);
        assert_eq!(
            s.guards()[0].state(),
            GuardState::Calm,
            "a player the guard cannot see stays undetected",
        );
    }

    /// §7.6 wired end to end: the two detection zones flip a guard between Chasing and
    /// Investigating as the player's distance crosses the certain→glimpse boundary. A
    /// stationary fixture isolates the state machine from patrol movement; detection is
    /// sight's alone (§9 — guards do not hear).
    #[test]
    fn detection_flips_between_chasing_and_investigating_by_zone() {
        // Guard looking straight down a long cone from (6,2); the player starts four
        // cells in — the certain zone — so the startup turn already has it Chasing.
        let mut s = State::new(
            open_room(13, 15),
            Cell::new(6, 6),
            Direction::North,
            vec![Guard::stationary(Cell::new(6, 2))],
            Vec::new(),
            Cell::new(11, 12),
        );
        assert_eq!(
            s.guards()[0].state(),
            GuardState::Chasing,
            "seen in the certain zone → Chasing",
        );

        // One step down the cone is still within the certain zone (5): still Chasing.
        s.step(Input::Step(Direction::South)); // (6,7): 5 cells
        assert_eq!(s.guards()[0].state(), GuardState::Chasing);

        // A second step crosses into the glimpse zone (6): drops to Investigating —
        // Run's five-cell gain is exactly this certain→glimpse distance (§7.6).
        s.step(Input::Step(Direction::South)); // (6,8): 6 cells
        assert_eq!(
            s.guards()[0].state(),
            GuardState::Investigating,
            "backed out to the glimpse zone → Investigating",
        );
    }

    /// Guards do not detect *each other* — detection reads the player alone (§7.8:
    /// guards cannot hurt each other). Two adjacent guards with the player far out of
    /// every cone both stay Calm turn after turn.
    #[test]
    fn guards_do_not_detect_each_other() {
        let mut s = State::new(
            open_room(10, 10),
            Cell::new(1, 1),
            Direction::North,
            vec![
                Guard::stationary(Cell::new(5, 5)),
                Guard::stationary(Cell::new(5, 6)),
            ],
            Vec::new(),
            Cell::new(8, 8),
        );
        for _ in 0..3 {
            s.step(Input::Wait);
        }
        assert!(
            s.guards().iter().all(|g| g.state() == GuardState::Calm),
            "a guard never reacts to another guard",
        );
    }

    /// Determinism (§12.4) with detection in play: the same start and inputs reproduce
    /// the same guard states and positions, reactions included.
    #[test]
    fn detection_is_deterministic() {
        let inputs = [
            Input::Step(Direction::East),
            Input::Step(Direction::East),
            Input::Wait,
            Input::Step(Direction::South),
        ];
        let run = || {
            let mut s = State::new(
                open_room(12, 12),
                Cell::new(3, 3),
                Direction::North,
                vec![
                    Guard::patrolling(Cell::new(6, 6)),
                    Guard::patrolling(Cell::new(9, 3)),
                ],
                Vec::new(),
                Cell::new(11, 11),
            );
            inputs
                .iter()
                .map(|&i| {
                    s.step(i);
                    s.guards()
                        .iter()
                        .map(|g| (g.pos(), g.state()))
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(run(), run());
    }

    /// §9.2 classification: from the player's position and facing, each guard is
    /// **Seen** (in the FOV), **Sensed** (within the guard-sense box but out of FOV),
    /// or neither (out of range → `None`). A guard in view is Seen even though it also
    /// sits inside the box — Seen wins, so the dot never doubles the full guard.
    #[test]
    fn guards_classify_as_seen_sensed_or_neither() {
        let seen = Cell::new(20, 16); // 4 north: in the forward half-disc
        let sensed = Cell::new(20, 25); // 5 south: behind the player, inside the box
        let gone = Cell::new(20, 33); // 13 south: behind and past the 10-box
        let s = State::new(
            open_room(40, 40),
            Cell::new(20, 20),
            Direction::North,
            vec![
                Guard::stationary(seen),
                Guard::stationary(sensed),
                Guard::stationary(gone),
            ],
            Vec::new(),
            Cell::new(38, 38),
        );

        assert!(
            s.player_fov().contains(seen),
            "precondition: seen guard in FOV"
        );
        assert!(
            !s.player_fov().contains(sensed),
            "precondition: sensed guard out of FOV"
        );
        assert_eq!(
            s.perceive_guard(&s.guards()[0]),
            Some(GuardPerception::Seen)
        );
        assert_eq!(
            s.perceive_guard(&s.guards()[1]),
            Some(GuardPerception::Sensed),
            "in the box but out of view → position only",
        );
        assert_eq!(
            s.perceive_guard(&s.guards()[2]),
            None,
            "past the box → nothing"
        );
    }

    /// §9.1's headline: the sense **passes through walls** — it is not line of sight.
    /// A guard sealed behind a wall, with no line to the player but inside the box, is
    /// **Sensed** (position only), not hidden. A walled-off fixture pins this.
    #[test]
    fn the_sense_passes_through_walls() {
        let mut layout = open_room(20, 20);
        // Wall the whole row y=8 across the interior, sealing the north strip from the
        // player's line of sight.
        for x in 1..=18 {
            layout.place(Cell::new(x, 8), Terrain::Wall);
        }
        let guard = Cell::new(10, 6); // 4 north of the player, behind the wall
        let s = State::new(
            layout,
            Cell::new(10, 10),
            Direction::North,
            vec![Guard::stationary(guard)],
            Vec::new(),
            Cell::new(18, 18),
        );

        assert!(
            !s.player_fov().contains(guard),
            "precondition: the wall blocks line of sight to the guard",
        );
        assert_eq!(
            s.perceive_guard(&s.guards()[0]),
            Some(GuardPerception::Sensed),
            "no line of sight but inside the box → sensed through the wall",
        );
    }

    /// §9.1 **[START]**: the sense box is **10**, widening to **20** on a turn the
    /// player spent waiting. Both are pinned so a later change is visible. A walled-off
    /// guard 11 cells away — just outside the box, no line of sight — is *not* sensed;
    /// the same guard becomes Sensed the turn the player waits (10 → 20).
    #[test]
    fn the_sense_range_is_ten_and_twenty_on_wait() {
        assert_eq!(PLAYER_SENSE_RANGE, 10, "the [START] sense range");
        assert_eq!(
            PLAYER_SENSE_RANGE_WAITING, 20,
            "the [START] wait sense range"
        );

        let mut layout = open_room(40, 40);
        // A full wall row seals the guard from sight, so it can only ever be *sensed*,
        // never seen — even under the 360° look a wait grants (§8.3).
        for x in 1..=38 {
            layout.place(Cell::new(x, 12), Terrain::Wall);
        }
        let guard = Cell::new(20, 9); // 11 north of the player: just past the 10-box
        let mut s = State::new(
            layout,
            Cell::new(20, 20),
            Direction::North,
            vec![Guard::stationary(guard)],
            Vec::new(),
            Cell::new(38, 38),
        );

        assert_eq!(s.sense_range(), 10, "no wait yet: the base box");
        assert_eq!(
            s.perceive_guard(&s.guards()[0]),
            None,
            "11 cells away is just outside the 10-box",
        );

        s.step(Input::Wait);
        assert_eq!(s.sense_range(), 20, "waiting widens the box");
        assert!(
            !s.player_fov().contains(guard),
            "still walled off from sight"
        );
        assert_eq!(
            s.perceive_guard(&s.guards()[0]),
            Some(GuardPerception::Sensed),
            "the wait pulls the guard into the widened box → sensed",
        );
    }
}
