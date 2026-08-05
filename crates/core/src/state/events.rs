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
    ///
    /// `own_tunnel` says **which** crawlspace (§4.5/#466): the way home, or a shortcut
    /// found in the facility. It rides on the event because
    /// [`message_for`](crate::message_for) is pure over it and cannot ask the layout —
    /// and the near line must not call the tunnel a duct one frame after the usable
    /// line offered `exit: enter`.
    EnteredDuct { at: Cell, own_tunnel: bool },
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
    /// The player took the intel at a console; `remaining` objectives are still out
    /// and `still_needed` of them must be taken before the exit will open.
    ///
    /// The two counts differ (#310): the run's [`IntelGate`](crate::IntelGate) decides
    /// how much is *enough* (§4.5/#244), so under
    /// [`AtLeastOne`](crate::IntelGate::AtLeastOne) this take satisfies the gate with
    /// two still out, while under [`All`](crate::IntelGate::All) it is progress and
    /// nothing more. `still_needed` is
    /// [`intel_needed_to_exit`](State::intel_needed_to_exit) *after* the take — zero
    /// exactly when the exit is now open — and it is carried here because
    /// [`message_for`](crate::status::message_for) is pure over the event and cannot
    /// ask the gate itself.
    IntelTaken {
        remaining: usize,
        still_needed: usize,
    },
    /// The player bumped the exit before the intel gate was met — refused (§4.5).
    /// `still_needed` is how many more consoles the gate wants
    /// ([`intel_needed_to_exit`](State::intel_needed_to_exit)), so the refusal can
    /// name the real requirement rather than assume one fixed rule (#310). Always at
    /// least 1: a refusal *is* an unmet gate.
    ExitRefused { still_needed: usize },
    /// The intel gate was satisfied (§10.2) and the player reached the exit: won.
    Won,
    /// A guard moved into the player's cell: captured (§4.5) — the only ordinary loss.
    ///
    /// It carries the whole **cause**, because this is the one instant the cause is
    /// true (§2.2/#138): `guard` indexes [`State::guards`](crate::State::guards),
    /// `state` is the mood that guard held **as it made contact** (§7.4), and `at` is
    /// the cell. The end screen presents these; it never reconstructs them from the
    /// finished board, where the capturing guard has since been standing on the
    /// player's cell and its mood at contact is gone.
    Captured {
        guard: usize,
        state: GuardState,
        at: Cell,
    },
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
    /// A guard that had the player in the certain zone lost sight and **called it
    /// in** (§7.7): another guard is converging on `at` — the cell where contact
    /// broke — to search it. Fires only with the `sighting_lost_calls_a_guard`
    /// modifier on (§12.6), and only when someone was actually free to send. The
    /// player reads it as this near line plus the caller's own sensed dot changing
    /// course (§9) — the visual tell, sound being gone (§9.3).
    CalledIn { at: Cell },
    /// A guard that found a body **called it in** (§7.7/§7.2): two guards are
    /// converging on `at` — the body's cell — to search it. Distinct from
    /// [`CalledIn`](Event::CalledIn) because it reports a *body's* position, not
    /// the player's, and it outranks the bare [`BodyFound`](Event::BodyFound) on
    /// the §11.7 ladder: it says everything a find says, plus that help is on its
    /// way. Fires only with the `body_found_calls_two_guards` modifier on (§12.6),
    /// once per body, and only when someone was free to send.
    BodyCalledIn { at: Cell },
    /// The player bumped the **comms console** and killed the radio net for the rest
    /// of the level (§7.3/§7.7): control stops pinging, so no further body is ever
    /// missed and no further alert steps from that source, and both cooperation
    /// call-ins stop firing. One-way and permanent — there is no matching "the radio is
    /// back" event because there is no way back. Distinct from
    /// [`RadioSilence`](Event::RadioSilence), which is a single *guard* gone quiet and
    /// is bad news; this is the net itself, and it is the player's doing.
    CommsSilenced { at: Cell },
    /// The player bumped an **equipment cache** and salvaged the tech in it
    /// (§2.2/§8.3/§14 v3/#209): `id` is now theirs — usable this turn, in this facility,
    /// and in every facility the run reaches after it.
    ///
    /// It carries the ability rather than a cell, because the ability is the whole
    /// event: where the crate stood stops mattering the instant it is opened, and *what
    /// you now have* is the only thing the near line, the campaign layer and the run's
    /// ledger each want from it. One-way, like the silenced net — nothing takes a
    /// salvaged ability back within a run, and nothing carries it out of one (§2.2).
    TechSalvaged { id: AbilityId },
    /// The player bumped a crate the run **cannot take from** (§8.3/§14 v3/#209) — a
    /// free refusal, like the exit's (§4.4). `id` is what the crate holds, because both
    /// refusals are about a specific thing you are walking away from: the tech you have
    /// no room for, or the one you already have.
    ///
    /// The crate is left unopened, so a run that comes back with a free hand — or, once
    /// #266 ships the exchange, with something to trade — finds it exactly as it was.
    SalvageRefused {
        id: AbilityId,
        refusal: SalvageRefusal,
    },
    /// The facility alert climbed to `rung`, because of `trigger` (§7.3): the
    /// concrete, explainable escalation the alert system was always meant to provide
    /// (§2.3). Fired **once per escalation** — a trigger at or below the rung the
    /// facility has already reached says nothing — and it carries *why*, so the near
    /// line (§11.4) and the §13.2 sim's attribution (#376) both read the same fact
    /// rather than inferring it. A rung never falls, so there is no matching
    /// "the facility calmed down" event: there is no way back down (§7.3).
    AlertRaised { rung: u32, trigger: AlertTrigger },
    /// A **reinforcement** walked into the facility at `at` (§7.3/#374): rung 2 sends
    /// one, rung 3 sends two more, so a run driven 0 → 3 gains three guards. The cell
    /// is never inside the player's field of view and never adjacent to them — an
    /// arrival the player *watches* is a guard materialising out of nothing, which no
    /// amount of fiction repairs — so this reports where somebody came in, not
    /// something the player saw happen.
    ///
    /// A reinforcement is a guard in every other respect (§7.4/§11.3): no glyph of its
    /// own, no colour of its own, normal speed (§7.1 **[SETTLED]**), its own radio
    /// clock, and a body if you take it down.
    ReinforcementArrived { at: Cell },
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
    /// Dephase ran out while the player stood somewhere that cannot admit a solid
    /// body — inside a wall, a shut door, a table, a cupboard or a console — and the
    /// tech's **safety eject** threw them clear
    /// (§8.3/#329): they were standing in the solid `from` and now stand on `to`, a
    /// cell drawn at random from the nearest ones that can hold them, **stunned** for
    /// `stunned` turns. The run continues;
    /// what phasing costs is those turns and the position, not the run itself
    /// (§4.5 **[SETTLED]**: contact is the only loss).
    ///
    /// It carries **both ends** of the throw rather than the landing alone, because the
    /// two are one event and the distance between them is what set the stun
    /// ([`phase_eject_stun`](crate::phase_eject_stun)) — so the effect mark drawn from
    /// this event (§11.5/#338) names the very pair the price was measured from, instead
    /// of the layer re-deriving an origin the player has already left.
    Ejected { from: Cell, to: Cell, stunned: u32 },
    /// Dephase ran out somewhere solid and there was **nowhere in the facility** to
    /// throw the player clear to (§8.3): the run ends. The degenerate case only — no
    /// generated level can be without a standable cell (§10.6) — kept so the
    /// impossible board is a truthful loss rather than a silently impossible state.
    /// A distinct loss from [`Event::Captured`], so the game-over reason stays
    /// truthful.
    Entombed { at: Cell },
    /// A toggle-off of Dephase was **refused** because the player stands where no
    /// solid body can (§8.3/#304): there is nowhere to rematerialize, so the phase
    /// holds. Free and nothing changed, like a [`Bumped`](Event::Bumped) wall — the
    /// lethal squeeze belongs to the duration alone ([`Entombed`](Event::Entombed)),
    /// never to a mis-pressed key (§4.4: cancelling is never a trap).
    RematerializeRefused,
    /// The player activated an ability (§8.2) — a turn-costing action (§4.4).
    ///
    /// `uses_left` is what the ability's **per-level budget** has left *after* this
    /// use (§8.2/#302), or `None` for the abilities the clocks alone govern. It rides
    /// on the event rather than being looked up when the message is built, so the
    /// number the near line speaks is the number the deck actually decremented — the
    /// same one the bar draws — and the two cannot drift (§8.2's timing rule).
    AbilityActivated {
        ability: AbilityId,
        uses_left: Option<u32>,
    },
    /// The player toggled an ability off early (§4.4) — free; its cooldown still
    /// runs (§8.2).
    AbilityDeactivated { ability: AbilityId },
    /// An ability's duration ran out at end of turn and it switched off (§8.2).
    AbilityExpired { ability: AbilityId },
    /// Pierce Wall bored the wall at `at` into floor, permanently (§8.3/#303) — a
    /// turn-costing action (§4.4) that changes the level's geometry for everyone: the
    /// route it opens is a route the guards get too.
    WallBored { at: Cell },
    /// A Pierce Wall activation was **refused** by its precondition (§8.4/#303) —
    /// free and nothing changed, like a wall bump, and costing no use (§8.2/#302).
    ///
    /// It carries the reason because the reasons are different *things to do about
    /// it* — walk to a wall, step off the corner, find a thinner one — and a player
    /// who is only ever told "no" learns the rule slowly and by accident.
    BoreRefused { reason: BoreRefusal },
    /// Lockdown shut and sealed `count` doors around `at` (§8.3/§10.4/#242) — the one
    /// act, reported once, rather than a [`DoorClosed`](Event::DoorClosed) per door: a
    /// lockdown is not three doors swinging shut of their own accord, and narrating it
    /// door by door would spend the near line's single row on what the board has
    /// already drawn (§11.7).
    ///
    /// `count` is what the seal actually took, so a test and the record agree with the
    /// world rather than with the radius; `reach` is the very box the doors were picked
    /// out of, carried by value rather than as a recipe for redrawing one, so the wash
    /// the layer paints is the geometry the rule actually used (#338).
    DoorsSealed { reach: EffectArea, count: usize },
    /// A Lockdown activation was **refused** because no door is in reach (§8.3/#242) —
    /// free, and nothing changed, like a wall bump. Carries no reason, because there is
    /// only ever the one: the ability seals doors, and there were none to seal.
    LockdownRefused,
    /// Confusion fired (§8.3/#325): `blast` went off, and the `caught` guards standing
    /// inside it are dazed for [`CONFUSION_DAZE_TURNS`](crate::CONFUSION_DAZE_TURNS)
    /// from now. A turn-costing action (§4.4), pushed alongside the activation.
    ///
    /// It carries the count because a 45-turn lockout is the game's most expensive
    /// press and the player has to know what it bought — the frozen guards may all be
    /// behind walls, and a blast the board cannot show is one the near line must
    /// (§11.7). It carries the `blast` itself, rather than a recipe for redrawing one,
    /// so the footprint the flash paints is the very object the daze was computed
    /// from (#308/#324).
    ConfusionFired { blast: EffectArea, caught: u32 },
    /// A Confusion activation was **refused** because the blast would have caught
    /// nobody (§8.3/#325) — free and nothing changed, like a wall bump, and spending
    /// neither the turn nor the cooldown.
    ///
    /// Fair rather than fiddly because the blast is clamped inside the guard sense:
    /// every guard it could have caught is one the player was already shown, so this
    /// refuses only a press that was going to buy nothing.
    ConfusionMissed,
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
            // A bored wall is the most emphatically "something you did" of the lot
            // (§8.3/#303) — a permanent mark on the facility — and its refusal is the
            // same quiet band as the tool it belongs to.
            Event::AbilityActivated { .. }
            | Event::AbilityDeactivated { .. }
            | Event::AbilityExpired { .. }
            | Event::RematerializeRefused
            | Event::WallBored { .. }
            | Event::BoreRefused { .. }
            | Event::DoorsSealed { .. }
            | Event::LockdownRefused
            | Event::ConfusionFired { .. }
            | Event::ConfusionMissed
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
            // A call-in (§7.7) is the same aroused-but-not-on-you band: the guard
            // that had you has *lost* you, and the one converging has never seen
            // you. The threat is spreading, not closing.
            //
            // The safety eject leaving you helpless (§8.3/#329) reads in the
            // same band for the mirror-image reason: nothing has you — no guard need
            // even know — but the next two turns are not yours, and that is a bad
            // fact about now rather than self-narration. Not Danger: the Danger band
            // belongs to a threat that is on you (§11.2), and the wall has just let go.
            Event::Ejected { .. }
            | Event::BodyFound { .. }
            | Event::RadioSilence { .. }
            | Event::CalledIn { .. }
            | Event::BodyCalledIn { .. }
            | Event::AlertRaised { .. }
            | Event::ReinforcementArrived { .. } => Category::Warning,
            // A guard that sees you is hunting *you* — the same Danger band as
            // its Chasing/Investigating glyph (§7.4), so the message and the `g`
            // reinforce (§11.2).
            Event::Detected { .. } => Category::Danger,
            // Neutral furniture doing furniture things (§10.4) — a door swinging
            // open or shut is scenery, whoever moved it.
            Event::DoorOpened { .. } | Event::DoorClosed { .. } => Category::System,
            // Goals and rewards — including the exit talking about the goal it
            // still refuses (§4.5) and the win itself. Killing the radio net (§7.7) is
            // the reward for routing to the comms console, and it reads in that band
            // rather than the Owned one your abilities use: it is a fact about the
            // *facility* now, not a tool you are holding.
            // Salvaging tech (§8.3/#209) reads in the same band, and the same argument
            // decides it: the *find* is the reward the facility was hiding, which is
            // Interest. That the thing found is a tool you now hold is the ability bar's
            // to say in Owned, one row down.
            Event::IntelTaken { .. }
            | Event::ExitRefused { .. }
            | Event::Won
            | Event::CommsSilenced { .. }
            | Event::TechSalvaged { .. }
            // A refused crate is still the reward channel talking — it is the same find
            // reported as *not yours*, not a threat and not furniture.
            | Event::SalvageRefused { .. } => Category::Interest,
            // A threat that has you, literally (§4.5) — or the wall does (§8.3).
            Event::Captured { .. } | Event::Entombed { .. } => Category::Danger,
        }
    }
}

/// **Why a crate cannot be taken from** (§8.3/§14 v3/#209) — the two ways a bump on a
/// live cache is refused, kept apart because they are different problems with different
/// answers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SalvageRefusal {
    /// The run already carries [`AbilityId::MAX_TECH_HELD`] pieces of tech (§8.3), and
    /// there is no room for another. A *decision* waiting to be made — which is exactly
    /// what #266's exchange screen is for; until it exists the crate is simply left where
    /// it stands, and coming back with a free hand finds it unopened.
    HandsFull,
    /// The crate holds tech the run already carries. A facility is stocked from its own
    /// seed and knows nothing of who is coming (#209), so this is luck rather than
    /// design — the world is not rearranged to spare you the walk.
    AlreadyCarried,
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
    /// not a bump either — it is a **wait**, see [`TakeBody`](Affordance::TakeBody).)
    ReleaseBody,
    /// A loose body **under the player's own feet**, hands free: wait to take hold
    /// (§8.3/#451). The one affordance that is not about a neighbour — it has no
    /// direction, which is why the usable line carries an
    /// [`Option<Direction>`](crate::Direction) rather than a `Direction`.
    ///
    /// Its label says **`wait`** because there is no glyph in §11.3's table for
    /// *this costs a turn*, and the shipped font stack falls back to a generic
    /// `monospace` on some devices — a clock codepoint nobody can guarantee would
    /// come out as tofu in the one row whose job is to teach a verb. The word costs
    /// five cells and cannot fail to render.
    ///
    /// Never offered while already dragging: the line shows `body: release` on the
    /// held body instead, and a second body is not something free hands can take.
    TakeBody,
    /// An empty cupboard while dragging a body: bump to stow the body inside and
    /// lock the cupboard — it is no longer a hideout (§7.2/§10.3).
    StoreBody,
    /// A closed door panel: bump to open (§10.4).
    OpenDoor,
    /// An open door's hinge: bump to close (§10.4).
    CloseDoor,
    /// An untaken intel console: bump to take the intel (§4.3).
    TakeIntel,
    /// The comms console with the radio net still live: bump to kill it for the rest of
    /// the level (§7.3/§7.7). Never offered on a console already used — a silenced net
    /// has nothing left to switch off.
    SilenceRadio,
    /// An **equipment cache** still holding its tech: bump to salvage it (§4.3/#209).
    /// Never offered on a crate already opened — an empty one is scenery, exactly as a
    /// spent console is.
    SalvageTech,
    /// A live crate the run has **no room for** (§8.3/#209): the bump will refuse, free.
    /// Offered rather than left silent because the refusal is a fact worth having before
    /// you spend the walk — the refused exit's precedent, one usable over.
    SalvageFull,
    /// A live crate holding tech the run **already carries** (#209): the bump will
    /// refuse, free.
    ///
    /// It tells the player the crate is a dud without their having to press, which is
    /// the point: the alternative is a line that says *take tech* and then does not, and
    /// §2.3 forbids exactly that. It gives away that the contents are a duplicate and
    /// nothing more — which is the same shape of hint the exit's refusal gives about the
    /// gate.
    SalvageCarried,
    /// An empty cupboard: bump to climb in and be concealed (§10.3).
    Hide,
    /// A duct entry: bump to climb into the crawlspace shortcut (§10.7).
    EnterDuct,
    /// The exit `E` from the facility side (§4.5/§10.7/#466): bump to climb into the
    /// tunnel you dug — the way home, whose far end is the level border and the world.
    ///
    /// Its own label rather than [`EnterDuct`](Affordance::EnterDuct)'s, because the two
    /// bumps behave identically and mean completely different things: one is a shortcut
    /// you *found*, the other is the only way out of the building. The one row that
    /// tells you what a bump does should not read the same for both.
    EnterExit,
    /// A table: bump to crouch behind it (§10.3).
    Crouch,
    /// The exit, with the intel gate met (§10.2): bump to win (§4.5).
    Leave,
    /// The exit while no intel is yet in hand: bumping it will refuse (§4.5).
    ExitRefused,
}

impl Affordance {
    /// **Every affordance there is**, so the usable line's width bound can measure
    /// the complete set at compile time (§11.4). Written out rather than derived —
    /// the same shape [`CAPTIONS`](crate::modifiers) takes, and for the same reason:
    /// a variant missing from here is a label nothing checks, which is exactly the
    /// drift the bound exists to stop. `affordance_labels_fit_the_row` walks the enum
    /// against it so a new variant cannot quietly skip the list.
    pub(crate) const ALL: [Affordance; 17] = [
        Affordance::Takedown,
        Affordance::ReleaseBody,
        Affordance::TakeBody,
        Affordance::StoreBody,
        Affordance::OpenDoor,
        Affordance::CloseDoor,
        Affordance::TakeIntel,
        Affordance::SilenceRadio,
        Affordance::SalvageTech,
        Affordance::SalvageFull,
        Affordance::SalvageCarried,
        Affordance::Hide,
        Affordance::EnterDuct,
        Affordance::EnterExit,
        Affordance::Crouch,
        Affordance::Leave,
        Affordance::ExitRefused,
    ];

    /// The words the usable line shows for this affordance.
    ///
    /// `const` so the row's width bound ([`super::super::render::usable`]) can be a
    /// compile-time assertion rather than a test: a label too wide for the v1 board
    /// then fails the *build*, never the frame.
    pub const fn label(self) -> &'static str {
        match self {
            Affordance::Takedown => "guard: take down",
            Affordance::ReleaseBody => "body: release",
            Affordance::TakeBody => "body: wait to grab",
            Affordance::StoreBody => "cupboard: stow body",
            Affordance::OpenDoor => "door: open",
            Affordance::CloseDoor => "door: close",
            Affordance::TakeIntel => "console: take intel",
            Affordance::SilenceRadio => "comms: silence radio",
            Affordance::SalvageTech => "cache: take tech",
            Affordance::SalvageFull => "cache: hands full",
            Affordance::SalvageCarried => "cache: already yours",
            Affordance::Hide => "cupboard: hide",
            Affordance::EnterDuct => "duct: enter",
            Affordance::EnterExit => "exit: enter",
            Affordance::Crouch => "table: crouch",
            Affordance::Leave => "exit: leave",
            Affordance::ExitRefused => "exit: needs the intel",
        }
    }

    /// What acting on this affordance is *about* (§11.2): doors, cupboards and
    /// tables are System furniture; both consoles and the exit are the goal,
    /// Interest — the comms console is a place worth routing to (§7.7), which is what
    /// puts it there rather than with the furniture, and an equipment cache (#209) is
    /// there on the same reading; a takedown is about the unaware
    /// threat it targets — Caution,
    /// matching the yellow `g` it points at; the body in your hands is Owned,
    /// like its recoloured glyph (§11.3). Stowing a body is a cupboard
    /// interaction — System furniture, like hiding in one.
    pub fn category(self) -> Category {
        match self {
            Affordance::Takedown => Category::Caution,
            // The body in your hands is Owned; the one you are about to pick up is
            // still the §7.3 liability it was when it hit the floor — Caution, the
            // same yellow as its own `z` and as the takedown that left it there.
            Affordance::TakeBody => Category::Caution,
            Affordance::ReleaseBody => Category::Owned,
            Affordance::OpenDoor
            | Affordance::CloseDoor
            | Affordance::Hide
            | Affordance::StoreBody
            | Affordance::EnterDuct
            | Affordance::Crouch => Category::System,
            // The exit is the goal at both ends of the run — climbing into your own
            // tunnel is Interest, not the System colour a found duct's mouth wears
            // (§4.5/#466): what it is *for* is leaving.
            Affordance::TakeIntel
            | Affordance::SilenceRadio
            | Affordance::SalvageTech
            | Affordance::SalvageFull
            | Affordance::SalvageCarried
            | Affordance::EnterExit
            | Affordance::Leave
            | Affordance::ExitRefused => Category::Interest,
        }
    }
}

#[cfg(test)]
mod affordance_tests {
    use super::*;

    /// [`Affordance::ALL`] really is **all** of them (§11.4/#451). The list is what
    /// the usable line's compile-time width bound measures, so a variant missing from
    /// it would be a label nothing checks — the drift the bound exists to stop.
    ///
    /// The match is the mechanism: adding a variant fails to compile here until it is
    /// named, and naming it without adding it to `ALL` then fails this assertion.
    #[test]
    fn every_affordance_is_in_all() {
        for a in Affordance::ALL {
            // Exhaustive on purpose — the compiler enumerates the work (§12.2).
            let named = match a {
                Affordance::Takedown
                | Affordance::ReleaseBody
                | Affordance::TakeBody
                | Affordance::StoreBody
                | Affordance::OpenDoor
                | Affordance::CloseDoor
                | Affordance::TakeIntel
                | Affordance::SilenceRadio
                | Affordance::SalvageTech
                | Affordance::SalvageFull
                | Affordance::SalvageCarried
                | Affordance::Hide
                | Affordance::EnterDuct
                | Affordance::EnterExit
                | Affordance::Crouch
                | Affordance::Leave
                | Affordance::ExitRefused => true,
            };
            assert!(named, "{a:?}");
        }
        let mut seen: Vec<&str> = Affordance::ALL.iter().map(|a| a.label()).collect();
        seen.sort_unstable();
        let before = seen.len();
        seen.dedup();
        assert_eq!(seen.len(), before, "two affordances share a label");
    }
}
