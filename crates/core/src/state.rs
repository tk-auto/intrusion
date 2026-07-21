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
//! The first reactive transition is wired too — **hearing** (§9.1): a guard that hears
//! this turn's noise above the threshold turns to Investigating and walks to its
//! source, then stands back down to patrol once the lead runs out. The rest of the
//! §7.4 state machine — sight detection, chasing, searching — is the later guard
//! tickets, which set a guard's destination the same way and reuse the same
//! walk-toward-it movement.

use crate::category::Category;
use crate::cell::{Cell, Direction};
use crate::facility::Terrain;
use crate::generate::Layout;
use crate::guard::Guard;
use crate::sound::{Loudness, Sound};
use crate::vision::{
    field_of_view, VisibleSet, PLAYER_SIGHT_ARC, PLAYER_SIGHT_RANGE, WAIT_SIGHT_ARC,
};
use crate::DoorAction;

/// The player and every guard are solid and exclusive — fill 1.0 (§4.3). A cell
/// already holding one admits no other actor.
pub(crate) const ACTOR_FILL: f32 = 1.0;

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
    /// hinge, or (until takedowns land) a guard.
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
/// actually performs — a takedown affordance joins when takedowns land (§7.2),
/// not before: the line must never offer what a bump will not do (§2.3).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Affordance {
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
    /// Interest.
    pub fn category(self) -> Category {
        match self {
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
    /// A guard — bumping is a free no-op until takedowns land (§7.2).
    Guard,
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
            BumpKind::Guard
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
    objectives: Vec<Objective>,
    exit: Cell,
    turn: u32,
    outcome: Outcome,
    /// The events of the player's most recent action, free or spent — what the
    /// near line reads (§11.7: messages clear on the next action, so holding
    /// exactly one action's events *is* the clearing rule). Empty before the
    /// first input; frozen once the run ends, so the final message stays.
    last_events: Vec<Event>,
    /// The sounds the most recent *spent* action made (§9.2), for the presentation
    /// (§9.3) and the guard-hearing check (§9.1) to read via
    /// [`sounds_this_turn`](Self::sounds_this_turn). Cleared at the start of every
    /// [`step`], so a free action (which makes no noise) leaves it empty. A `Vec`
    /// because the guards emit here too (§9.2's last row): a spent turn holds the
    /// player's own noise, if any, followed by one Low source per guard.
    sounds: Vec<Sound>,
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
            objectives,
            exit,
            turn: 0,
            outcome: Outcome::Playing,
            last_events: Vec::new(),
            sounds: Vec::new(),
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
    /// 360° on a turn spent waiting — the only way to see behind you (§8.3). What is
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

    /// The sounds the most recent *spent* action made this turn (§9.2) — where each
    /// noise came from and how loud it started. Empty after a free action (a
    /// mis-input makes no noise) and after a silent one (waiting, crouching).
    ///
    /// This is the raw emission; a consumer spreads it with
    /// [`audible_field`](crate::audible_field) to learn the intensity at a cell —
    /// the sound presentation (§9.3) to show the player a noise, and — once guards
    /// react — the hearing check that flips a guard to Investigating (§9.1).
    pub fn sounds_this_turn(&self) -> &[Sound] {
        &self.sounds
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

    /// What a bump would do from here — the **usable line** (§11.4): each
    /// interaction orthogonally adjacent to the player, with the direction to
    /// bump it, in [`Direction::ALL`] order. The §10.6 one-usable guarantee
    /// keeps this to a single entry on generated boards; a hand-built state
    /// may list more, one per direction.
    ///
    /// This mirrors [`step`](Self::step)'s bump resolution case for case, so
    /// the line can never promise what a bump won't deliver: a guard is skipped
    /// (bumping one is a free no-op until takedowns land, §7.2), a spent
    /// console and an occupied cupboard are just solid, and door poses come
    /// from the same door graph the bump consults (§10.4). Each target must
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

        // This action's noise starts fresh (§9.2). A free action emits nothing, so
        // clearing here leaves `sounds` empty after a mis-input; a spent action fills
        // it from the branches in `resolve_step` / `player_phase`.
        self.sounds.clear();
        let mut events = Vec::new();
        // Phase 1. A free action (wall bump, refused exit) does not end the turn.
        let spent = self.player_phase(input, &mut events);

        if self.outcome == Outcome::Playing && spent {
            self.turn += 1;
            // Phases 2 and 3 only happen because the player spent the turn (§4.2/§4.4).
            events.extend(self.run_world_phases());
            // Ability durations will tick HERE — at end of turn, after all three
            // phases — so a freshly activated N-turn ability yields N protected turns
            // and the activation turn itself is covered (§8.2's N-yields-N−1 trap).
            // Abilities land in their own ticket; this is the spot the loop reserves.
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
                // Waiting is Silent (§9.2's "None" row): it spends the turn but makes
                // no noise — which is what lets holding still be a way to go unheard,
                // not just unseen. No `emit`, so `sounds` stays empty.
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
        }
    }

    /// Record the noise a spent action made (§9.2): push a [`Sound`] at `source`
    /// unless the action is [`Loudness::Silent`], in which case there is nothing to
    /// hear. The turn loop calls this from the world-changing branches; a free
    /// action never reaches it.
    fn emit(&mut self, source: Cell, loudness: Loudness) {
        let intensity = loudness.intensity();
        if intensity > 0 {
            self.sounds.push(Sound { source, intensity });
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
            // A guard in the way would be a takedown (§7.2), its own ticket; for now
            // bumping a guard is a free no-op, the seam that fills in later.
            BumpKind::Guard => {
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
                // Taking intel is not one of §9.2's noise sources, so it stays silent —
                // emit nothing rather than invent a level the design didn't name.
                events.push(Event::IntelTaken { remaining });
                true
            }
            // A door (§4.3, §10.4): opening or closing spends the turn and makes Medium
            // noise (§9.2) from the player's own cell — they stay put working the
            // handle, so the noise starts beside the door, not in the doorway. An
            // obstructed close changed nothing — free and silent; doors never crush.
            BumpKind::Door { action } => match action {
                DoorAction::Opened => {
                    self.operate_door(target);
                    events.push(Event::DoorOpened { at: target });
                    self.emit(self.player, Loudness::Medium);
                    true
                }
                DoorAction::Closed => {
                    self.operate_door(target);
                    self.emit(self.player, Loudness::Medium);
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
                self.facing = dir; // facing follows the last successful step (§5)
                                   // Climbing into a cupboard is still a step — Low footstep noise (§9.2),
                                   // from the cell just entered. Only once you *wait* inside it are you
                                   // silent. A judgment call worth a later playtest: scrambling into cover
                                   // isn't free of sound the way holding still is.
                self.emit(target, Loudness::Low);
                events.push(Event::EnteredHideout { at: target });
                true
            }
            // A table: bump it to crouch behind it (§4.3, §10.3). Ducking is a
            // *decision*, aimed at a specific table — concealment is across that table
            // only. The player does not move; the table stays solid furniture. Silent
            // (§9.2, the "camouflaged" row): the whole point is to go unnoticed.
            BumpKind::Crouch => {
                self.crouched_behind = Some(target);
                events.push(Event::Crouched { behind: target });
                true
            }
            // Plain movement into a cell that admits the player — Low noise (§9.2) from
            // the cell just stepped into.
            BumpKind::Move => {
                self.player = target;
                self.facing = dir; // facing follows the last successful step (§5)
                self.emit(target, Loudness::Low);
                events.push(Event::Moved { to: target });
                true
            }
            // A cupboard already holding an actor, the table already crouched behind, or
            // anything else solid (a wall, a closed hinge): a free bump (§4.4).
            BumpKind::HideoutBlocked | BumpKind::CrouchHeld | BumpKind::Solid => {
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
        self.layout
            .bump_door(target, |c| actor_occupies(player, guards, c));
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
        if self.guard_at(target).is_some() {
            return BumpKind::Guard;
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
    /// waiting (§8.3). Guards carve their ~90° wedge with the same function (§6.2).
    fn recompute_sight(&mut self) {
        let facility = self.layout.facility();
        let arc = if self.waited {
            WAIT_SIGHT_ARC
        } else {
            PLAYER_SIGHT_ARC
        };
        self.player_fov =
            field_of_view(facility, self.player, self.facing, arc, PLAYER_SIGHT_RANGE);
        // Tile memory (§11.5a) accumulates here, in the same phase that produced
        // the sight — every cell the player can see now is remembered forever.
        self.memory.absorb(&self.player_fov);
        for guard in &mut self.guards {
            guard.look(facility);
        }
    }

    /// Phase 3 (§4.2): the guards *sense*, then *act*, then *sound*. First every guard
    /// takes in this turn's information ([`Guard::sense`], §7.6/§9.1) — it hears the
    /// noise and sees the player from the cone phase 2 just recomputed: a loud enough
    /// sound turns it to Investigating toward the source, and a player in its cone
    /// flips it to Chasing (certain zone) or Investigating (glimpse zone), sight
    /// overriding sound. A player [`concealed_from`](Self::concealed_from) that guard
    /// is neither seen nor, being silent, heard — the cupboard's payoff (§10.3/§7.6).
    /// Then each guard `decide`s a step (§7.5); a guard moving into the player's cell
    /// is a capture and ends the run (§4.5). Otherwise it moves onto any cell that
    /// admits it and holds no other actor; a guard with nowhere to go, or whose step
    /// is blocked, simply holds.
    ///
    /// Once every guard has resolved its move, each one emits its §9.2 patrol noise
    /// from where it now stands ([`emit_guard_noise`](Self::emit_guard_noise)) — the
    /// "guards make noise too" row that gives the player a second information channel
    /// working around corners. Sensing runs on `self.sounds` *before* that emission,
    /// so a guard reacts to the player's noise, never to another guard's footsteps.
    fn guard_phase(&mut self, events: &mut Vec<Event>) {
        let facility = self.layout.facility();
        // Whether the player is concealed from each guard is a query over the whole
        // state (§10.3), so resolve it up front — one immutable read per guard —
        // before the loop takes each guard mutably to fold the senses in.
        let concealed: Vec<bool> = self
            .guards
            .iter()
            .map(|guard| self.concealed_from(guard.pos()))
            .collect();
        for (guard, &concealed) in self.guards.iter_mut().zip(&concealed) {
            guard.sense(facility, self.player, concealed, &self.sounds);
        }
        for i in 0..self.guards.len() {
            if self.outcome != Outcome::Playing {
                return;
            }
            let facility = self.layout.facility();
            let Some(dir) = self.guards[i].decide(facility) else {
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
        self.emit_guard_noise();
    }

    /// Emit each guard's §9.2 patrol/idle noise — a **Low** source at the guard's own
    /// cell, every turn, whether it moved or held (§9.2 last row, "guards make noise
    /// too"). This is the payoff that turns an audible guard into a *second
    /// information channel that works around corners*: because sound flows around
    /// walls (#58), the player can track a guard through a wall the 90° cone can't
    /// see past.
    ///
    /// It runs after all movement, so a guard sounds from where it now stands, and
    /// only while the run is live — a capturing guard shares the player's cell, and
    /// emitting there would masquerade as the *player's* own audibility (which
    /// [`audibility_range`](crate::audibility_range) reads off `source == player`).
    /// The guard's own silence has nothing to do with the player's: waiting, hiding
    /// and crouching keep the *player* unheard (§9.2), and none of that touches this.
    fn emit_guard_noise(&mut self) {
        for i in 0..self.guards.len() {
            self.emit(self.guards[i].pos(), Loudness::Low);
        }
    }

    /// The index of a guard standing on `cell`, if any.
    fn guard_at(&self, cell: Cell) -> Option<usize> {
        self.guards.iter().position(|g| g.pos() == cell)
    }

    /// Whether any actor occupies `cell` — the loop's single occupancy predicate.
    /// Actors are the player and the guards today; bodies, decoys and the rest fold in
    /// here (§4.3/§12.3) so occupancy is asked in one place and nothing — not the
    /// player, not guards — is special-cased at the call sites.
    fn occupied(&self, cell: Cell) -> bool {
        actor_occupies(self.player, &self.guards, cell)
    }
}

/// Whether an actor occupies `cell`, given the player and guards directly. The free
/// twin of [`State::occupied`], for callers that must borrow the actor fields apart
/// from the rest of the state (door closing borrows the layout mutably at the same
/// time). One definition of "an actor is here" — extend it, not the call sites, when
/// new actor kinds arrive.
fn actor_occupies(player: Cell, guards: &[Guard], cell: Cell) -> bool {
    player == cell || guards.iter().any(|g| g.pos() == cell)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guard::{GuardState, PATROL_RADIUS};
    use crate::test_support::{open_room, solo};
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
    /// spent, and they are now [`hidden`](State::hidden). Facing follows the step (§5).
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
        assert_eq!(s.facing(), Direction::East, "facing follows the step");
        assert_eq!(s.turn(), 1, "entering spends the turn");
        assert!(s.hidden(), "the player is now concealed");
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

    /// An adjacent guard offers **nothing**: bumping one is a free no-op until
    /// takedowns land (§7.2), and the usable line must never promise what a bump
    /// will not do (§2.3). An occupied cupboard is likewise just solid.
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

    /// §9.2 — moving normally makes a **Low** footstep, sourced at the cell just
    /// stepped into.
    #[test]
    fn a_move_emits_a_low_footstep() {
        let mut s = solo(Cell::new(4, 4));
        s.step(Input::Step(Direction::East));
        assert_eq!(
            s.sounds_this_turn(),
            &[Sound {
                source: Cell::new(5, 4),
                intensity: Loudness::Low.intensity(),
            }]
        );
    }

    /// §9.2's "None" row — waiting spends the turn but emits no sound, so holding
    /// still is a way to go *unheard*, not only unseen.
    #[test]
    fn waiting_makes_no_sound() {
        let mut s = solo(Cell::new(4, 4));
        s.step(Input::Wait);
        assert!(s.sounds_this_turn().is_empty());
    }

    /// §4.4/§9.2 — a free action (a wall bump) makes no noise, and clearing on every
    /// action means `sounds_this_turn` reflects only the latest: a footstep is
    /// replaced by the silence of the bump that follows it.
    #[test]
    fn a_free_bump_is_silent_and_replaces_prior_sound() {
        let mut s = solo(Cell::new(1, 4));
        assert!(!s.step(Input::Step(Direction::East)).is_empty()); // a Low footstep
        assert_eq!(s.sounds_this_turn().len(), 1);

        s.step(Input::Step(Direction::West)); // back onto (1,4)
        let events = s.step(Input::Step(Direction::West)); // into the west wall
        assert!(matches!(events.as_slice(), [Event::Bumped { .. }]));
        assert!(
            s.sounds_this_turn().is_empty(),
            "a free bump makes no sound and clears the prior footstep"
        );
    }

    /// §9.2 — opening a door is **Medium** noise, sourced at the player's own cell:
    /// they stay put working the handle, so the sound starts beside the doorway.
    #[test]
    fn opening_a_door_emits_medium_from_the_player() {
        let layout = generate(40, 40, &mut Rng::new(7)).unwrap();
        let panel = layout.regions().doors().next().unwrap().1.panels()[0];

        // One of the four approaches stands on floor and bumps the panel open.
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
            if s.step(Input::Step(dir)) != vec![Event::DoorOpened { at: panel }] {
                return false;
            }
            // Player did not move; the noise is Medium, at their cell.
            assert_eq!(
                s.sounds_this_turn(),
                &[Sound {
                    source: from,
                    intensity: Loudness::Medium.intensity(),
                }]
            );
            true
        });
        assert!(
            opened,
            "one approach must bump the panel open and sound Medium"
        );
    }

    /// Determinism (§12.4): the same start and the same inputs produce the same
    /// sounds, turn for turn — a run is `(seed, [inputs])` and its noise is part of
    /// what replays identically.
    #[test]
    fn identical_runs_make_identical_sounds() {
        let inputs = [
            Input::Step(Direction::East),
            Input::Wait,
            Input::Step(Direction::South),
            Input::Step(Direction::East),
        ];
        let run = || {
            let mut s = solo(Cell::new(3, 3));
            inputs
                .iter()
                .map(|&i| {
                    s.step(i);
                    s.sounds_this_turn().to_vec()
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(run(), run());
    }

    /// §9.2 last row — every guard emits a **Low** source at its own cell each turn,
    /// held or not: "guards make noise too", the player's second information channel.
    /// A silent player action (a wait) leaves only the guards' noise, so the sources
    /// read off directly, one per guard in order; the Low intensity is pinned here so
    /// a later change to the [START] value is a visible edit.
    #[test]
    fn every_guard_emits_low_from_its_cell_each_turn() {
        let mut s = State::new(
            open_room(12, 12),
            Cell::new(1, 1),
            Direction::North,
            vec![
                Guard::stationary(Cell::new(6, 4)),
                Guard::stationary(Cell::new(9, 8)),
            ],
            Vec::new(),
            Cell::new(11, 11),
        );

        s.step(Input::Wait); // silent: only the guards sound this turn
        assert_eq!(
            s.sounds_this_turn(),
            &[
                Sound {
                    source: Cell::new(6, 4),
                    intensity: Loudness::Low.intensity(),
                },
                Sound {
                    source: Cell::new(9, 8),
                    intensity: Loudness::Low.intensity(),
                },
            ],
        );

        // It recurs every turn — a second wait yields the same two Low sources.
        s.step(Input::Wait);
        assert_eq!(s.sounds_this_turn().len(), 2, "both guards sound again");
        assert!(
            s.sounds_this_turn()
                .iter()
                .all(|snd| snd.intensity == Loudness::Low.intensity()),
            "each guard's noise stays Low",
        );
    }

    /// §9.1/§9.2 — a guard's Low patrol noise rides #58's propagation: it reaches a
    /// listener sharing open floor with the guard, but a solid wall stops it. Proven
    /// by comparison — the *same* cell at the *same* path distance is audible with the
    /// wall gone and silent with it there — so it is the wall that muffles the guard,
    /// not mere range. This is the payoff: the player hears a guard through a wall the
    /// cone can't see past.
    #[test]
    fn a_guard_is_audible_along_shared_floor_but_not_through_a_wall() {
        use crate::sound::audible_field;
        // A 5×5 walled box (interior x∈1..=3, y∈1..=3); wall the whole x=2 interior
        // column so the guard's west strip (x=1) is sealed from the east strip (x=3).
        let mut layout = open_room(5, 5);
        for y in 1..=3 {
            layout.place(Cell::new(2, y), Terrain::Wall);
        }
        let guard = Cell::new(1, 2);
        let mut s = State::new(
            layout,
            Cell::new(3, 3), // player sealed away in the east strip, never captured
            Direction::North,
            vec![Guard::stationary(guard)],
            Vec::new(),
            Cell::new(3, 1),
        );

        s.step(Input::Wait);
        // The one sound this silent turn is the guard's, at its cell.
        assert_eq!(
            s.sounds_this_turn(),
            &[Sound {
                source: guard,
                intensity: Loudness::Low.intensity(),
            }],
        );
        let noise = s.sounds_this_turn()[0];

        // Audible one open step away on the guard's own side of the wall.
        let sealed = audible_field(s.layout().facility(), noise);
        assert!(
            sealed.is_audible_at(Cell::new(1, 1)),
            "heard across shared open floor",
        );
        // Silent two cells east — the sealing wall stops it dead.
        assert_eq!(
            sealed.intensity_at(Cell::new(3, 2)),
            0,
            "the solid wall stops the guard's noise",
        );
        // The same cell at the same distance *is* audible with the wall gone, so it is
        // the wall doing the muffling, not the Low sound simply running out of reach.
        let open = audible_field(open_room(5, 5).facility(), noise);
        assert!(
            open.is_audible_at(Cell::new(3, 2)),
            "with no wall, two open steps is well within Low's reach",
        );
    }

    /// The guard emission is orthogonal to the player's own silence (§9.2): with a
    /// guard sounding nearby, a waiting player still makes *no* sound of their own —
    /// only the guard's source appears, never one at the player's cell. Guards emit;
    /// the player's "hold still to go unheard" rule is untouched.
    #[test]
    fn guard_noise_leaves_the_players_own_silence_intact() {
        let mut s = State::new(
            open_room(10, 10),
            Cell::new(3, 3),
            Direction::North,
            vec![Guard::stationary(Cell::new(6, 6))],
            Vec::new(),
            Cell::new(8, 8),
        );
        s.step(Input::Wait);
        let player = s.player();
        assert!(
            s.sounds_this_turn().iter().all(|snd| snd.source != player),
            "a waiting player emits nothing of their own",
        );
        assert!(
            s.sounds_this_turn()
                .iter()
                .any(|snd| snd.source == Cell::new(6, 6)),
            "but the guard is still heard",
        );
    }

    /// Determinism (§12.4) with guards in play: the same start and inputs reproduce
    /// the same sounds turn for turn, patrol noise included — a moving guard sounds
    /// from the same cells on every replay.
    #[test]
    fn guard_noise_is_deterministic() {
        let inputs = [
            Input::Wait,
            Input::Wait,
            Input::Step(Direction::South),
            Input::Wait,
        ];
        let run = || {
            let mut s = State::new(
                open_room(20, 20),
                Cell::new(2, 2),
                Direction::North,
                vec![Guard::patrolling(Cell::new(10, 10))],
                Vec::new(),
                Cell::new(18, 18),
            );
            inputs
                .iter()
                .map(|&i| {
                    s.step(i);
                    s.sounds_this_turn().to_vec()
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(run(), run());
    }

    /// §9.1 wired through the real turn loop, now that sight rides alongside it
    /// (§7.6): a footstep is *heard* and turns a guard to Investigating even where the
    /// guard cannot *see* its source. A player scrambling into a cupboard beside a
    /// guard emits a Low footstep the guard one cell away hears (3 − 1 = 2 > the
    /// threshold), yet concealed in the hideout (§10.3) the player is not detected by
    /// sight — so the reaction is hearing's alone, not a Chase. (A footstep only ever
    /// carries one cell, and that cell is always in the touching ring; concealment is
    /// what keeps the two senses separable at such close range.)
    #[test]
    fn a_guard_hears_a_scramble_into_cover_it_cannot_see() {
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

        // Step East into the cupboard at (5,4): a Low footstep the guard one cell east
        // hears while the hideout conceals the player from its sight.
        s.step(Input::Step(Direction::East));
        assert!(s.hidden(), "the player scrambled into the cupboard");
        assert_eq!(
            s.guards()[0].state(),
            GuardState::Investigating,
            "heard the scramble but never saw the hidden player",
        );
    }

    /// A silent turn alerts no one (§9.2's "None" row): waiting emits nothing, so a
    /// guard that has no line to the player — it stands two cells behind the guard's
    /// back, past the touching ring and out of its cone — neither hears nor sees them
    /// and stays Calm. (Beside the guard, sight would give the player away regardless
    /// of silence, §7.6 — so silence buys safety only out of view.)
    #[test]
    fn a_silent_turn_alerts_no_one() {
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
            "a waiting player makes no noise to hear",
        );
    }

    /// §7.6 wired end to end: the two detection zones flip a guard between Chasing and
    /// Investigating as the player's distance crosses the certain→glimpse boundary. A
    /// stationary fixture isolates the state machine from patrol movement, and the
    /// footsteps are too far to be heard, so the reaction is sight's alone.
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

    /// Guards make Low noise too (§9.2) but never hear *each other*: hearing runs
    /// before the guards emit, so `self.sounds` holds only the player's noise. Two
    /// adjacent guards with a silent player both stay Calm turn after turn.
    #[test]
    fn guards_do_not_investigate_each_other() {
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
            "one guard's footsteps never trigger another's hearing",
        );
    }

    /// Determinism (§12.4) with hearing in play: the same start and inputs reproduce
    /// the same guard states and positions, reactions included.
    #[test]
    fn hearing_is_deterministic() {
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
}
