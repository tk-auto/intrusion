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
//! old one-turn sensory lag (§4.2). **Guards** still move along a scripted route, a
//! deterministic placeholder for the patrol/chase AI (§7.4–7.6): their cones are
//! computed and stored, but nothing *reacts* to them yet — detection, chasing and the
//! state machine are the guard tickets, which will read the sight data this loop
//! already maintains.

use crate::category::Category;
use crate::cell::{Cell, Direction};
use crate::facility::Terrain;
use crate::generate::Layout;
use crate::region::DoorCell;
use crate::sound::{Loudness, Sound};
use crate::vision::{
    field_of_view, VisibleSet, GUARD_SIGHT_ARC, GUARD_SIGHT_RANGE, PLAYER_SIGHT_ARC,
    PLAYER_SIGHT_RANGE, WAIT_SIGHT_ARC,
};
use crate::DoorAction;

/// The player and every guard are solid and exclusive — fill 1.0 (§4.3). A cell
/// already holding one admits no other actor.
const ACTOR_FILL: f32 = 1.0;

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
/// Its movement is a **scripted route** walked one step per turn, cycling — the
/// simplest possible placeholder for the patrol/chase AI (§7.4–7.6). It has a real
/// field of view — the ~90° cone (§6.2/§7.1), recomputed every sight phase — and a
/// [`GuardState`], but no transitions and no reactions yet: it does not open doors,
/// and it exists so that capture (§4.5) is a real, tested consequence rather than
/// an untriggerable branch. The guard tickets replace the route with the real thing
/// behind the same phase, reading the sight this loop already maintains.
#[derive(Clone, Debug)]
pub struct Guard {
    pos: Cell,
    facing: Direction,
    route: Vec<Direction>,
    step: usize,
    fov: VisibleSet,
    state: GuardState,
}

/// Every guard looks **south** at spawn (§7.1). One definition, shared by the
/// constructors below and by placement's turn-one-safety check (§10.6, `place`) —
/// if the spawn facing ever changes, the "no guard eyes the player's spawn"
/// guarantee moves with it instead of silently lying.
pub(crate) const GUARD_INITIAL_FACING: Direction = Direction::South;

impl Guard {
    /// A guard that holds its cell — no patrol.
    pub fn stationary(pos: Cell) -> Self {
        Self {
            pos,
            facing: GUARD_INITIAL_FACING,
            route: Vec::new(),
            step: 0,
            fov: VisibleSet::default(),
            state: GuardState::Calm,
        }
    }

    /// A guard that walks `route`, one direction per turn, cycling back to the start.
    pub fn patrolling(pos: Cell, route: Vec<Direction>) -> Self {
        Self {
            pos,
            facing: GUARD_INITIAL_FACING,
            route,
            step: 0,
            fov: VisibleSet::default(),
            state: GuardState::Calm,
        }
    }

    /// The same guard in `state`. The §7.4 transitions are the guard AI tickets'
    /// job; until they land, this is how a scenario — a test, the sim — puts a
    /// guard in a non-[`Calm`](GuardState::Calm) state.
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

    /// The direction the guard will try this turn, or `None` if it has no route.
    fn next_dir(&self) -> Option<Direction> {
        if self.route.is_empty() {
            None
        } else {
            Some(self.route[self.step % self.route.len()])
        }
    }

    /// Consume this turn's route step, whether or not the move succeeded — a blocked
    /// guard still advances along its script rather than retrying the same wall.
    fn advance(&mut self) {
        if !self.route.is_empty() {
            self.step = self.step.wrapping_add(1);
        }
    }
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
    /// because guards will emit here too once they act (§9.2's last rows); today
    /// only the player does, so it holds at most one.
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
            if !self.player_fov.contains(target) || self.guard_at(target).is_some() {
                continue;
            }
            let affordance = if target == self.exit {
                if self.objectives_remaining() == 0 {
                    Some(Affordance::Leave)
                } else {
                    Some(Affordance::ExitRefused)
                }
            } else if self.objectives.iter().any(|o| o.cell == target && !o.taken) {
                Some(Affordance::TakeIntel)
            } else if let Some(id) = self.layout.regions().door_at(target) {
                let door = self.layout.regions().door(id);
                match (door.role(target), door.is_open()) {
                    (Some(DoorCell::Panel), false) => Some(Affordance::OpenDoor),
                    // A close with an actor on a panel would be refused — doors
                    // never crush (§10.4) — so it is not offered either.
                    (Some(DoorCell::Hinge), true)
                        if !door.panels().iter().any(|&c| self.occupied(c)) =>
                    {
                        Some(Affordance::CloseDoor)
                    }
                    _ => None,
                }
            } else if self.layout.facility().terrain(target) == Some(Terrain::Hideout)
                && !self.occupied(target)
            {
                Some(Affordance::Hide)
            } else if self.layout.facility().terrain(target) == Some(Terrain::PartialCover)
                && self.crouched_behind != Some(target)
            {
                // The table already crouched behind offers nothing: re-bumping
                // it is a free no-op.
                Some(Affordance::Crouch)
            } else {
                None
            };
            if let Some(a) = affordance {
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

        // A guard in the way would be a takedown (§7.2), which is its own ticket; for
        // now bumping a guard is a free no-op, the seam that fills in later.
        if self.guard_at(target).is_some() {
            events.push(Event::Bumped { into: target });
            return false;
        }

        // The exit: win if the objectives are done, else refuse — free either way, a
        // refused exit changes nothing (§4.5).
        if target == self.exit {
            if self.objectives_remaining() == 0 {
                self.outcome = Outcome::Won;
                events.push(Event::Won);
                return true;
            }
            events.push(Event::ExitRefused);
            return false;
        }

        // An objective console: take the intel. A console already emptied is just
        // solid — it falls through to a free bump below.
        if let Some(obj) = self
            .objectives
            .iter_mut()
            .find(|o| o.cell == target && !o.taken)
        {
            obj.taken = true;
            let remaining = self.objectives.iter().filter(|o| !o.taken).count();
            // Taking intel is not one of §9.2's noise sources, so it stays silent —
            // emit nothing rather than invent a level the design didn't name.
            events.push(Event::IntelTaken { remaining });
            return true;
        }

        // A door: bump a closed panel to open it (§4.3, §10.4). Opening or closing a
        // door changes the world and spends the turn; a close refused by an occupant
        // changed nothing and is free. The close consults the general occupancy
        // predicate — any actor on a panel refuses the close, so a door never crushes
        // (§10.4). Fields are captured so it can borrow them while `layout` is `&mut`.
        let action = {
            let player = self.player;
            let guards = &self.guards;
            self.layout
                .bump_door(target, |c| actor_occupies(player, guards, c))
        };
        if let Some(action) = action {
            return match action {
                // Working a door is Medium noise (§9.2), from the player's own cell:
                // they stay put operating the handle, so the noise starts beside the
                // door, not in the doorway. A closed door has no event of its own but
                // makes the same noise. An obstructed close changed nothing — free
                // and silent.
                DoorAction::Opened => {
                    events.push(Event::DoorOpened { at: target });
                    self.emit(self.player, Loudness::Medium);
                    true
                }
                DoorAction::Closed => {
                    self.emit(self.player, Loudness::Medium);
                    true
                }
                DoorAction::Obstructed => false,
            };
        }

        // A hideout: bump the empty cupboard to climb in (§4.3, §10.3). Unlike
        // floor, you do not drift onto it — entering is a *decision*. It moves you
        // into the cell, spends the turn, and conceals you ([`hidden`](Self::hidden));
        // climbing out is an ordinary step off the cell, no special case. Its whole
        // cost is time: while you hide you make no progress and the clock keeps
        // ticking (§2.3). A cupboard already holding an actor refuses — a free bump.
        if self.layout.facility().terrain(target) == Some(Terrain::Hideout) {
            if self.occupied(target) {
                events.push(Event::Bumped { into: target });
                return false;
            }
            self.player = target;
            self.facing = dir; // facing follows the last successful step (§5)
                               // Climbing into a cupboard is still a step — Low footstep noise (§9.2),
                               // from the cell just entered. Only once you *wait* inside it are you
                               // silent. A judgment call worth a later playtest: scrambling into cover
                               // isn't free of sound the way holding still is.
            self.emit(target, Loudness::Low);
            events.push(Event::EnteredHideout { at: target });
            return true;
        }

        // A table: bump it to crouch behind it (§4.3, §10.3). Like the cupboard,
        // ducking is a *decision*, aimed at a specific table — concealment is
        // across that table only. The crouch spends the turn; re-bumping the
        // table you are already behind changes nothing and is free (§4.4). The
        // player does not move — the table stays solid furniture.
        if self.layout.facility().terrain(target) == Some(Terrain::PartialCover) {
            if self.crouched_behind == Some(target) {
                events.push(Event::Bumped { into: target });
                return false;
            }
            self.crouched_behind = Some(target);
            // Ducking behind cover is a stealth move made in place — Silent (§9.2,
            // the "camouflaged" row): the whole point is to go unnoticed, so it
            // emits no sound.
            events.push(Event::Crouched { behind: target });
            return true;
        }

        // Plain movement, if the cell admits the player.
        if self.layout.facility().can_enter(target, ACTOR_FILL) {
            self.player = target;
            self.facing = dir; // facing follows the last successful step (§5)
                               // Moving normally is Low noise (§9.2), from the cell just stepped into.
            self.emit(target, Loudness::Low);
            events.push(Event::Moved { to: target });
            return true;
        }

        // Anything else solid — a wall, a closed hinge — is a free bump (§4.4).
        events.push(Event::Bumped { into: target });
        false
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
            guard.fov = field_of_view(
                facility,
                guard.pos,
                guard.facing,
                GUARD_SIGHT_ARC,
                GUARD_SIGHT_RANGE,
            );
        }
    }

    /// Phase 3 (§4.2): each guard acts. A guard moving into the player's cell is a
    /// capture and ends the run (§4.5). Otherwise it advances along its route onto any
    /// cell that admits it and holds no other actor; a blocked guard simply holds.
    fn guard_phase(&mut self, events: &mut Vec<Event>) {
        for i in 0..self.guards.len() {
            if self.outcome != Outcome::Playing {
                return;
            }
            let Some(dir) = self.guards[i].next_dir() else {
                continue;
            };
            let Some(target) = self.guards[i].pos.step(dir) else {
                self.guards[i].advance();
                continue;
            };
            self.guards[i].advance();

            if target == self.player {
                // Capture is contact (§4.5) — but a concealed player is the one
                // exception: the occupied cupboard is solid and a patrol routes
                // *around* it, so contact is refused (§10.3, §7.6). The guard cannot
                // enter; it holds this turn (its route already advanced). This is the
                // "hold still, watch the cone sweep past" payoff.
                if self.hidden() {
                    continue;
                }
                self.guards[i].pos = target;
                self.outcome = Outcome::Lost;
                events.push(Event::Captured { by: target });
                return;
            }
            // A guard moves onto a cell the terrain admits and no actor occupies. Its
            // own cell is a step behind `target`, so the mover is never in the way; the
            // player's cell was captured above but `occupied` still guards it.
            if self.layout.facility().can_enter(target, ACTOR_FILL) && !self.occupied(target) {
                self.guards[i].pos = target;
                self.guards[i].facing = dir; // facing follows the successful step (§5)
                                             // Refresh the moved guard's cone at once, so the sight a frame shows
                                             // never lags the position it shows — the danger overlay must not lie
                                             // (§11.5). The AI's *decisions* read the phase-2 sight; the next
                                             // phase 2 recomputes everything anyway.
                self.guards[i].fov = field_of_view(
                    self.layout.facility(),
                    target,
                    dir,
                    GUARD_SIGHT_ARC,
                    GUARD_SIGHT_RANGE,
                );
            }
        }
    }

    /// The index of a guard standing on `cell`, if any.
    fn guard_at(&self, cell: Cell) -> Option<usize> {
        self.guards.iter().position(|g| g.pos == cell)
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
    player == cell || guards.iter().any(|g| g.pos == cell)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facility::Facility;
    use crate::{generate, DoorId, Rng};

    /// An open room: a `w × h` walled box, all interior floor, wrapped as a bare
    /// layout. Enough to drive movement, objectives, and capture without generation.
    fn open_room(w: u32, h: u32) -> Layout {
        Layout::from_facility(Facility::walled_box(w, h))
    }

    /// A player in an empty room, facing north, no guards or objectives, exit unused
    /// in a far corner.
    fn solo(player: Cell) -> State {
        State::new(
            open_room(10, 10),
            player,
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(8, 8),
        )
    }

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
        // Guard at (6,4) patrolling west; player at (4,4). After the startup turn the
        // guard is at (5,4); the player waits, and the guard steps onto (4,4).
        let mut s = State::new(
            open_room(10, 10),
            Cell::new(4, 4),
            Direction::North,
            vec![Guard::patrolling(Cell::new(6, 4), vec![Direction::West])],
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

    /// A guard blocked by a wall holds rather than walking through it, and keeps
    /// advancing its script so it doesn't wedge.
    #[test]
    fn a_guard_blocked_by_a_wall_holds() {
        // Guard one cell from the west wall, marching into it forever.
        let mut s = State::new(
            open_room(10, 10),
            Cell::new(5, 5),
            Direction::North,
            vec![Guard::patrolling(Cell::new(1, 1), vec![Direction::West])],
            Vec::new(),
            Cell::new(8, 8),
        );
        // Startup already tried once; a few more waits never move it off (1,1).
        for _ in 0..3 {
            s.step(Input::Wait);
        }
        assert_eq!(s.guards()[0].pos(), Cell::new(1, 1));
        assert_eq!(s.outcome(), Outcome::Playing);
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
            let Some(from) = panel.step(opposite(dir)) else {
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

    fn opposite(dir: Direction) -> Direction {
        match dir {
            Direction::North => Direction::South,
            Direction::South => Direction::North,
            Direction::East => Direction::West,
            Direction::West => Direction::East,
        }
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
            let Some(hinge_dir) = dir_between(panel, hinge) else {
                continue;
            };
            // Approach the panel perpendicular to the door line, from floor.
            for perp in perpendicular(hinge_dir) {
                let Some(from) = panel.step(perp) else {
                    continue;
                };
                let f = layout.facility();
                if f.terrain(from) == Some(Terrain::Floor) && f.can_enter(from, ACTOR_FILL) {
                    return Some((id, from, opposite(perp), panel, hinge_dir));
                }
            }
        }
        None
    }

    /// The cardinal direction stepping `from` to the adjacent `to`, if they touch.
    fn dir_between(from: Cell, to: Cell) -> Option<Direction> {
        Direction::ALL
            .into_iter()
            .find(|&d| from.step(d) == Some(to))
    }

    /// The two directions perpendicular to `dir`.
    fn perpendicular(dir: Direction) -> [Direction; 2] {
        match dir {
            Direction::North | Direction::South => [Direction::East, Direction::West],
            Direction::East | Direction::West => [Direction::North, Direction::South],
        }
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
        // Guard at (6,4) marching west; player hidden in the cupboard at (4,4). After
        // the startup turn the guard is at (5,4), one step from the player's cell.
        let mut s = State::new(
            layout,
            Cell::new(4, 4),
            Direction::North,
            vec![Guard::patrolling(Cell::new(6, 4), vec![Direction::West])],
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
            vec![Guard::patrolling(Cell::new(6, 4), vec![Direction::West])],
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
            Cell::new(1, 1),
            Direction::South,
            vec![Guard::patrolling(Cell::new(8, 8), vec![Direction::West])],
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
                vec![Guard::patrolling(
                    Cell::new(8, 5),
                    vec![Direction::North, Direction::West],
                )],
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
            let side = dir_between(panel, from).expect("from is beside the panel");
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
            let Some(from) = panel.step(opposite(dir)) else {
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
}
