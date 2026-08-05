//! **Piloting** — the turn loop's half of the control-transfer seam (§8.1/#273).
//!
//! [`control`](crate::control) says what a remote unit *is*; this says what the loop
//! does with one. Four things, and they are the whole mechanic:
//!
//! - **Launching** it ([`deploy_remote`](State::deploy_remote)) — a spent turn, from
//!   the cell the player is standing on.
//! - **Driving** it ([`pilot_step`](State::pilot_step)) — phase 1 for a turn where the
//!   player's body is not what moves. Every drone move is a full §4.2 turn: the sight
//!   phase runs, the guards run, and the body stands still through both.
//! - **Handing the keys back and taking them again**
//!   ([`release_control`](State::release_control) /
//!   [`take_control`](State::take_control)) — free one way (§4.4's toggle-off), a spent
//!   turn the other, on the same window throughout.
//! - **Seeing through it** ([`remote_fov`](State::remote_fov)) — the union that makes
//!   the §11.5a fog lift wherever the machine has been, whether or not anybody is
//!   flying it.
//!
//! # The cost is the body, so the body has to be worth something
//!
//! §2.3 asks what an ability costs. This one costs *the turns your body spends
//! unattended in a patrolled facility* — capture is contact (§4.5), and you are looking
//! through a camera somewhere else while a patrol walks its beat. That is the entire
//! price, which makes it fragile in exactly one place: **a body that cannot be reached
//! pays nothing.** Inside a duct the player is concealed and contact-safe (§10.7), so
//! piloting from a crawlspace would be free scouting and the ability would have no cost
//! at all. Hence the one precondition in the §8.4 ladder — you launch, and take the
//! controls back, **on your feet** — which is not fussiness but the thing that keeps
//! the §2.3 answer true.
//!
//! A **cupboard** is deliberately *not* held to the same rule (§10.3), even though it
//! conceals too. The exemption it grants is weaker and it is bought: a guard that
//! watched you climb in flushes you out and takes you (§15 Q5), you spent turns walking
//! to a specific piece of furniture and will spend more climbing out, and you cannot act
//! on anything you learn until you do. Hiding somewhere sensible before you fly is the
//! play this ability *should* reward. A duct is none of that — it is a travel network,
//! it is blanket contact-safe, and the run opens inside one (§4.5/#466), so the same
//! permission there would let a player read the facility before setting foot in it.
//!
//! # What piloting is not
//!
//! It is not a second player. The drone has **no interaction verb**: it cannot bump, so
//! it opens no door, takes no intel, touches no guard, and cannot win the run (§4.3's
//! one verb belongs to hands). It changes nothing in the world at all — it only looks.
//! A step into a wall is the free mis-input a wall bump has always been (§4.4).
//!
//! Nor is it incorporeal. It flies **over a table and through a shut door's vents**
//! ([`Terrain::admits_drone`]) because it is hand-sized and airborne, and it is stopped
//! dead by the building's fabric — a wall, a door frame, a duct entry — and by the solid
//! usables. It passes through a door **without opening it**, which is the whole
//! difference between a machine that reads a wing and a machine that unlocks one.

use super::*;
use crate::control::{remote_kind, Remote, RemoteKind};
use crate::vision::field_of_view;

impl State {
    /// The remote unit in the facility, if one is out (§8.1/#273) — for the renderer,
    /// the §11.4 surfaces and tests. `None` whenever nothing is deployed, which is
    /// every state that has never pressed a control-transfer ability.
    pub fn remote(&self) -> Option<Remote> {
        self.remote
    }

    /// Whether the player's input is currently driving the remote rather than their own
    /// body (§8.1/#273) — the one question every surface asks about this ability.
    ///
    /// Stored rather than derived from the ability's window, because the two are
    /// deliberately different facts: the window says the machine is alive, this says who
    /// is holding the keys, and the whole shape of the ability is that you can end the
    /// second without ending the first.
    pub fn piloting(&self) -> bool {
        self.piloting
    }

    /// Whether the player is standing somewhere they could take a remote's controls
    /// (§10.7/#273) — the §8.4 precondition behind both the launch and the resume.
    ///
    /// **On your feet, never from a crawlspace.** A player inside a duct is concealed
    /// and cannot be touched (§10.7), so their body is not at risk while they fly — and
    /// the exposed body is this ability's entire cost (§2.3). Piloting from a tunnel
    /// would be scouting for free, and the run's own entry tunnel would let you read the
    /// facility before setting foot in it.
    pub(super) fn can_take_controls(&self) -> bool {
        !self.in_duct()
    }

    /// Whether `id` is the ability whose remote the player is **currently flying**
    /// (§8.1/#273) — the one ability a press can still reach while the keys are the
    /// machine's, because its key is the way back out of the mode.
    pub(super) fn flying(&self, id: AbilityId) -> bool {
        self.piloting && self.remote.is_some_and(|remote| remote.source == id)
    }

    /// Whether `id`'s remote is **out and unattended** — deployed, still inside its
    /// window, and nobody flying it (§8.2/#273). This is the state a press *resumes*
    /// from, and the reason a control ability's key stays live for its whole duration
    /// instead of going dead the moment the player lets go.
    pub(super) fn remote_awaits(&self, id: AbilityId) -> bool {
        !self.piloting && self.remote.is_some_and(|remote| remote.source == id)
    }

    /// Launch `id`'s remote from `at` and take its controls (§8.1/#273) — the
    /// activation half, called once the deck has actually switched the ability on, so a
    /// refused press launches nothing.
    ///
    /// The launch cell is the player's own: you let it go from your hands. Nothing needs
    /// to be clear for that — a remote is not an actor (§4.3) and shares its cell with
    /// whatever is standing there, starting with you.
    pub(super) fn deploy_remote(&mut self, id: AbilityId, at: Cell, events: &mut Vec<Event>) {
        let Some(kind) = remote_kind(id) else {
            return;
        };
        self.remote = Some(Remote {
            cell: at,
            kind,
            source: id,
        });
        self.take_control(events);
    }

    /// Hand the controls **to** the remote (§8.1/#273): the transfer itself, shared by
    /// the launch and the later resume, so there is one definition of "the keys are the
    /// drone's now".
    pub(super) fn take_control(&mut self, events: &mut Vec<Event>) {
        let Some(remote) = self.remote else {
            return;
        };
        self.piloting = true;
        events.push(Event::ControlTaken { at: remote.cell });
    }

    /// Hand the controls **back** to the player's body (§4.4/#273): free, refunds
    /// nothing, and — the point of the whole arrangement — **does not end the window**.
    /// The remote stays exactly where it was left and keeps feeding its camera for the
    /// rest of the duration (§8.2).
    ///
    /// Also the path every *involuntary* return takes — the window expiring under a
    /// player who was still flying — so "the keys are mine again" is one transition
    /// however it is reached. The caller decides what happens to the machine.
    pub(super) fn release_control(&mut self, events: &mut Vec<Event>) {
        if !self.piloting {
            return;
        }
        self.piloting = false;
        if let Some(remote) = self.remote {
            events.push(Event::ControlReleased { at: remote.cell });
        }
    }

    /// End `id`'s remote (§8.2/#273): the window is over — by expiry, the only way it
    /// ends — so the machine dies and, if the player was still flying it, the keys come
    /// back with it.
    ///
    /// Called from the end-of-turn expiry sweep beside the decoy's and the lockdown's,
    /// keyed on the ability rather than on the kind, so the remote a future second
    /// control ability deploys ends on its own clock without a second arm.
    pub(super) fn end_remote(&mut self, id: AbilityId, events: &mut Vec<Event>) {
        if !self.remote.is_some_and(|remote| remote.source == id) {
            return;
        }
        // The release first, so the player is told they have their body back *before*
        // being told the machine is gone: the near line shows the loudest of a turn's
        // messages (§11.7), and "you have the keys" is the one that changes what the
        // next press does. The ability's own `AbilityExpired` says the rest.
        self.release_control(events);
        self.remote = None;
    }

    /// **Phase 1 while flying** (§4.2/§4.4/#273): the whole of what an input does when
    /// the keys belong to the remote. Returns whether the turn was spent, exactly as
    /// [`player_phase`](State::player_phase) does.
    ///
    /// Four answers, and there are no others:
    ///
    /// - **Step** flies the machine ([`pilot_step`](Self::pilot_step)) — a spent turn,
    ///   so the world moves around the body you left behind.
    /// - **Wait** hovers: a spent turn that changes nothing. It pointedly does **not**
    ///   set [`waited`](State::waited), so it buys neither the 360° look nor the widened
    ///   guard sense (§8.3/§9.1) — those are what a person gets for standing in a room
    ///   and taking stock of it, and a player watching a camera two rooms away is doing
    ///   the opposite of that.
    /// - **The remote's own key** hands the controls back — free (§4.4), and the window
    ///   runs on.
    /// - **Everything else** is the free no-op of a key that does nothing: another
    ///   ability's activation, another ability's toggle-off, a discard answering an
    ///   exchange that cannot be open (#266 — an offer arrives by bumping a crate, and
    ///   bumping is something hands do). Your hands are on the controls. It is silent,
    ///   because the reason is already on the board (§11.7's rule for a refusal the
    ///   player can see): the drone is drawn under its own mark and the §11.4 bar has
    ///   greyed every other entry.
    pub(super) fn piloted_phase(&mut self, input: Input, events: &mut Vec<Event>) -> bool {
        match input {
            Input::Step(dir) => self.pilot_step(dir, events),
            Input::Wait => {
                self.waited = false;
                true
            }
            Input::Deactivate(id) if self.flying(id) => {
                self.release_control(events);
                false
            }
            Input::Activate(_) | Input::Deactivate(_) | Input::Discard(_) => false,
        }
    }

    /// Phase 1 for a turn spent **flying** (§4.2/#273): move the remote one cell, or
    /// refuse for free.
    ///
    /// A move is a spent turn like any other — the sight phase and the guard phase both
    /// run, and the player's body stands still through them, which is what the ability
    /// costs. A step into anything the remote cannot enter is the free §4.4 mis-input a
    /// wall bump has always been: there is no bump to resolve, because a remote has no
    /// hands (§4.3).
    ///
    /// **Occupancy is not consulted.** A drone flies over a guard, a body and the player
    /// alike — it is not an actor and takes no space (§4.3) — so the only thing that
    /// stops it is the building itself.
    pub(super) fn pilot_step(&mut self, dir: Direction, events: &mut Vec<Event>) -> bool {
        let Some(remote) = self.remote else {
            return false;
        };
        let Some(target) = remote.cell.step(dir) else {
            return false;
        };
        if !self.remote_can_enter(remote.kind, target) {
            events.push(Event::Bumped { into: target });
            return false;
        }
        self.remote = Some(Remote {
            cell: target,
            ..remote
        });
        events.push(Event::RemoteMoved { to: target });
        true
    }

    /// Whether `kind` may occupy `cell` (§10.3/#273).
    ///
    /// A drone respects the building, and what "the building" means for a hand-sized
    /// flying machine is the terrain's own answer ([`Terrain::admits_drone`], §10.5's
    /// one spatial model): everywhere a person could squeeze, **plus over a table and
    /// through a shut door's vents** — never a wall, a door frame, a duct entry or a
    /// solid usable.
    ///
    /// It is a predicate of its own rather than the actor fill test, because a drone is
    /// neither an actor nor a phased player. The zero-fill phase (§8.3, Dephase) is
    /// emphatically *not* what it gets: a remote that crossed **walls** would make the
    /// level's shape stop mattering, and being unthreatened is what lets it sweep, not
    /// being incorporeal. A vented door is a passage that happens to be shut; a wall is
    /// a wall at any scale.
    fn remote_can_enter(&self, kind: RemoteKind, cell: Cell) -> bool {
        match kind {
            RemoteKind::Drone => self
                .layout
                .facility()
                .terrain(cell)
                .is_some_and(Terrain::admits_drone),
        }
    }

    /// What the remote can see, for the sight phase to fold into the player's own view
    /// (§6/§11.5a/#273) — `None` when nothing is deployed.
    ///
    /// A plain cast (§6.2), never the player's auto-peek: the peek is a fact about a
    /// person leaning around a corner (#121), and a machine hovering in a corridor is
    /// not leaning anywhere. The arc and the range are the kind's
    /// ([`RemoteKind::sight_arc`], [`RemoteKind::sight_range`]) — for a drone, the full
    /// circle at a short reach, which is a camera rather than a better pair of eyes.
    ///
    /// The facing handed to the cast is the player's own and is **immaterial**: a full
    /// circle has no orientation to be wrong about. It is passed rather than invented
    /// because the caster takes one, and a remote with a stored facing would be a field
    /// nothing reads.
    ///
    /// **It is fed whether or not anybody is flying** — that is precisely the value of
    /// the half of the window the player spends back in their own body: a camera left in
    /// a junction is watching that junction while you walk the other way.
    pub(super) fn remote_fov(&self) -> Option<VisibleSet> {
        let remote = self.remote?;
        Some(field_of_view(
            self.layout.facility(),
            remote.cell,
            self.facing,
            remote.kind.sight_arc(),
            remote.kind.sight_range(),
        ))
    }
}
