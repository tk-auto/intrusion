//! The turn loop's public vocabulary (§4.3/§4.4/§11.4).
//!
//! What the player asks for ([`Input`]), what the loop reports back ([`Event`]), and
//! what the usable line offers ([`Affordance`]) — the three enums the shell and the
//! §13.2 bot both speak, lifted out of [`state.rs`](super) so that file reads as the
//! phase machinery alone. Nothing here touches [`State`](super::State): these are
//! plain data, which is why they move as a unit.
//!
//! Each carries its own information [`Category`](crate::Category) (§11.2), so a
//! message drawn from an event or an affordance colours through the same table as
//! everything else — the loop reports facts, the presentation reads them.

use super::*;

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
    /// The player bumped a duct entry and climbed into the crawlspace (§4.3, §10.7):
    /// they now occupy the duct and are concealed, their perception cut to the mouth
    /// peek and a shortened sense. Crawling along is [`Event::DuctCrawled`]; climbing
    /// out is an ordinary [`Event::Moved`] off an entry onto its mouth.
    EnteredDuct { at: Cell },
    /// The player crawled one cell along a duct (§10.7): a spent turn (§4.4) that
    /// moves them to `to` but leaves them concealed inside the crawlspace.
    DuctCrawled { to: Cell },
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
    /// doors, and walking into the panel is the bump that opens them. `by_player`
    /// says which: a door *you* opened keeps its quiet near-line self-narration
    /// (§11.7), while a guard-opened one is instead felt on the grid as a §10.4
    /// door cue in the [`Category::Sensed`](crate::Category::Sensed) channel —
    /// evidence someone passed, readable around a corner.
    DoorOpened { at: Cell, by_player: bool },
    /// A door shut away from the player: a **Calm** guard closing a hinged door
    /// behind itself after passing through it (§10.4, #146) — the counter-pressure
    /// to guard traffic's monotonic opening, restoring the level's structure and
    /// keeping an open door meaningful as evidence — or an automatic door timing
    /// shut (#147). `at` is a panel of the shut door; `by_player` mirrors
    /// [`DoorOpened`](Event::DoorOpened) (always `false` today — the player has no
    /// close-by-bump — but a door cue is raised for every close the player did not
    /// cause).
    DoorClosed { at: Cell, by_player: bool },
    /// The player took the intel at a console; `remaining` objectives are still out.
    IntelTaken { remaining: usize },
    /// The player bumped the exit with objectives still outstanding — refused (§4.5).
    ExitRefused,
    /// The intel gate was satisfied (§10.2) and the player reached the exit: won.
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
    /// dispatched the nearest active guard to `at` — where the guard fell, which
    /// is control's last fix on it — to search there. The player reads it as a
    /// near-line message and as the responder's own sensed dot peeling off toward
    /// that cell (§9) — the visual tell that replaces the old sound (§9.3).
    RadioSilence { at: Cell },
    /// A second missed radio ping stepped the facility-wide alert to `level`
    /// (§7.3): the concrete, explainable escalation the alert system was always
    /// meant to provide (§2.3). Written here, read on the near line (§11.4).
    AlertRaised { level: u32 },
    /// The player took hold of a body by stepping off its cell (§8.3): they are
    /// now dragging it, at half speed, until they release it or the run ends.
    BodyGrabbed { at: Cell },
    /// The player let the dragged body go where it lies (§8.3) — free (§4.4),
    /// and it refunds nothing because there is nothing to refund.
    BodyReleased { at: Cell },
    /// The player stowed the dragged body inside a cupboard (§7.2): the body is
    /// *gone* — no cone will ever find it — and the cupboard is **locked**, no
    /// longer a hideout. A spent turn (§4.4); the player's hands come free.
    BodyStored { at: Cell },
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
            // Crawling a duct is the same kind of routine movement (§10.7).
            Event::Moved { .. } | Event::Bumped { .. } | Event::DuctCrawled { .. } => {
                Category::Neutral
            }
            // Things you made — including making yourself hidden (§10.3: the
            // occupied cupboard and the covering table recolour to Owned; their
            // messages match). Vanishing into a duct is the same move (§10.7).
            Event::EnteredHideout { .. } | Event::EnteredDuct { .. } | Event::Crouched { .. } => {
                Category::Owned
            }
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
            Event::TakenDown { .. }
            | Event::BodyGrabbed { .. }
            | Event::BodyReleased { .. }
            | Event::BodyStored { .. } => Category::Owned,
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
    /// The body being dragged: bump it to let go — free (§4.4). (Taking hold is
    /// not a bump: you drag by walking over a body and off its cell, §8.3.)
    ReleaseBody,
    /// An empty cupboard while dragging a body: bump to stow the body inside and
    /// lock the cupboard — it is no longer a hideout (§7.2/§10.3).
    StoreBody,
    /// A closed door panel: bump to open (§10.4).
    OpenDoor,
    /// An open door's hinge: bump to close (§10.4).
    CloseDoor,
    /// An untaken intel console: bump to take the intel (§4.3).
    TakeIntel,
    /// An empty cupboard: bump to climb in and be concealed (§10.3).
    Hide,
    /// A duct entry: bump to climb into the crawlspace shortcut (§10.7).
    EnterDuct,
    /// A table: bump to crouch behind it (§10.3).
    Crouch,
    /// The exit, with the intel gate met (§10.2): bump to win (§4.5).
    Leave,
    /// The exit while no intel is yet in hand: bumping it will refuse (§4.5).
    ExitRefused,
}

impl Affordance {
    /// The words the usable line shows for this affordance.
    pub fn label(self) -> &'static str {
        match self {
            Affordance::Takedown => "guard: take down",
            Affordance::ReleaseBody => "body: release",
            Affordance::StoreBody => "cupboard: stow body",
            Affordance::OpenDoor => "door: open",
            Affordance::CloseDoor => "door: close",
            Affordance::TakeIntel => "console: take intel",
            Affordance::Hide => "cupboard: hide",
            Affordance::EnterDuct => "duct: enter",
            Affordance::Crouch => "table: crouch",
            Affordance::Leave => "exit: leave",
            Affordance::ExitRefused => "exit: needs the intel",
        }
    }

    /// What acting on this affordance is *about* (§11.2): doors, cupboards and
    /// tables are System furniture; the console and the exit are the goal,
    /// Interest; a takedown is about the unaware threat it targets — Caution,
    /// matching the yellow `g` it points at; the body in your hands is Owned,
    /// like its recoloured glyph (§11.3). Stowing a body is a cupboard
    /// interaction — System furniture, like hiding in one.
    pub fn category(self) -> Category {
        match self {
            Affordance::Takedown => Category::Caution,
            Affordance::ReleaseBody => Category::Owned,
            Affordance::OpenDoor
            | Affordance::CloseDoor
            | Affordance::Hide
            | Affordance::StoreBody
            | Affordance::EnterDuct
            | Affordance::Crouch => Category::System,
            Affordance::TakeIntel | Affordance::Leave | Affordance::ExitRefused => {
                Category::Interest
            }
        }
    }
}
