//! **Control transfer** (§8.1's escape hatch, #273): the player's input driving
//! something other than the player's body.
//!
//! Every other ability changes what *your body* can do — how far it steps, what sees
//! it, what it can walk through. This one changes **who the keys are for**: while a
//! transfer is in force, `Step` moves a machine on the other side of the facility and
//! your body stands where you left it. That is not something the §8.1 effect
//! vocabulary can express, and it is the case the design names by hand when it
//! reserves the code hatch (*"piloting a drone, rewinding time"*), so it is built as
//! [`Behaviour::Coded`](crate::Behaviour::Coded) behaviour keyed on the ability rather
//! than as another [`Effect`](crate::Effect) row.
//!
//! # Two facts, deliberately kept apart
//!
//! A [`Remote`] **exists**; control **is transferred to it**. Those are separate
//! states because the drone's whole shape depends on them being separable: you deploy
//! it, fly it for a while, hand the keys back — and it stays out there watching, on
//! the same clock, until the window ends. One duration covers both halves (§8.2), so
//! *how much of it you spend flying* is the decision the ability sells, and the number
//! on the ability bar means one thing throughout: turns until the machine dies.
//!
//! # Why the seam is not called "drone"
//!
//! A drone is the first thing worth handing the keys to; it will not be the last (a
//! guard you have taken over is the obvious second). So what the state holds is a
//! *remote unit* with a [`RemoteKind`], and what decides whether an ability transfers
//! control is one table ([`remote_kind`]) rather than an identity test scattered
//! through the turn loop. Adding the next one is a row there plus its spawn rule; the
//! loop, the renderer and the vision union need not learn its name.
//!
//! The rules that are genuinely *the drone's* — that it cannot cross a wall, that its
//! camera is 360° and short, that guards cannot perceive it at all — live on the kind,
//! not on the seam.

use serde::{Deserialize, Serialize};

use crate::ability::AbilityId;
use crate::cell::Cell;
use crate::vision::FULL_SIGHT_ARC;

/// How far a **drone's** camera reaches (§6.1 box, §8.3 **[START]**, #273).
///
/// Deliberately **shorter than the player's own sight** ([`PLAYER_SIGHT_RANGE`]): the
/// drone is a small camera that goes where you cannot, not a better pair of eyes. What
/// it buys is *reach* — it can be somewhere else — and pricing that with a fat view box
/// on top would make flying it strictly better than looking, which is the §11.5a fog
/// deleted rather than paid for.
///
/// It is the ability's main information lever, so it is pinned by a test and expected
/// to move in playtest.
///
/// [`PLAYER_SIGHT_RANGE`]: crate::vision::PLAYER_SIGHT_RANGE
pub const DRONE_SIGHT_RANGE: u32 = 8;

/// A drone's camera stays **inside** the player's own sight box (§6.1/§8.3): it goes
/// where you cannot, it does not out-see you. Pinned at compile time so the pair can
/// never silently invert into a tool that is simply better eyes on a stick.
const _: () = assert!(DRONE_SIGHT_RANGE < crate::vision::PLAYER_SIGHT_RANGE);

/// What kind of thing the player has taken control of (§8.1/#273).
///
/// One variant today. It exists as an enum rather than as an implied drone because the
/// rules that differ between remotes are exactly the interesting ones — what geometry
/// stops it, how it sees, whether the facility can perceive it — and a second kind
/// should arrive by adding a variant and answering those questions, not by threading a
/// boolean through the turn loop.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum RemoteKind {
    /// A **drone** (§8.3/#273): a flying camera you launch from your own cell.
    ///
    /// - **It respects the building, at its own scale.** Everywhere a person could
    ///   squeeze, plus **over a table** and **through a shut door's ventilation holes**
    ///   — but never a wall, a door frame, a duct entry or a solid usable
    ///   ([`Terrain::admits_drone`](crate::Terrain::admits_drone)). A drone that ignored
    ///   geometry outright would be Dephase plus omniscience, and the facility's shape —
    ///   the thing every other system is about — would stop mattering. What makes it
    ///   sweep easily is that nothing threatens it, not that it is incorporeal; and it
    ///   crosses a shut door without **opening** it, which is the difference between
    ///   reading a wing and unlocking one.
    /// - **It is not an actor.** It blocks nothing and nothing blocks it: it flies over
    ///   guards, bodies and the player alike, and no door ever shuts on it.
    /// - **The facility cannot perceive it.** No cone detects it, no guard state
    ///   changes for it, nothing can be done to it. The only pressure the ability is
    ///   under is the body you left behind (§4.5).
    Drone,
}

impl RemoteKind {
    /// The sight arc this remote looks through (§6.2). A drone's camera is the full
    /// circle: it has no face and nothing to turn, so an arc would be a fact invented
    /// for the sake of having one.
    pub fn sight_arc(self) -> u8 {
        match self {
            RemoteKind::Drone => FULL_SIGHT_ARC,
        }
    }

    /// How far this remote sees (§6.1 box) — [`DRONE_SIGHT_RANGE`] for the drone.
    pub fn sight_range(self) -> u32 {
        match self {
            RemoteKind::Drone => DRONE_SIGHT_RANGE,
        }
    }
}

/// A **remote unit** the player has put into the facility (§8.1/#273): where it is,
/// what it is, and which ability's window it lives inside.
///
/// It carries its `source` ability rather than being named by the turn loop, which is
/// what makes the clock arrangement work without a second timer: the remote dies when
/// that ability's duration ends (§8.2), whether the player was still flying it or had
/// handed the keys back twenty turns earlier. There is no linger counter, because the
/// linger *is* the rest of the duration.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Remote {
    /// The cell it occupies. It shares cells freely — it is not an actor (§4.3).
    pub(crate) cell: Cell,
    /// What it is, and so what rules it plays by ([`RemoteKind`]).
    pub(crate) kind: RemoteKind,
    /// The ability whose active window is this remote's life (§8.2). Read when that
    /// window ends, so nothing has to remember which ability put it there.
    pub(crate) source: AbilityId,
}

impl Remote {
    /// Where it is.
    pub fn cell(self) -> Cell {
        self.cell
    }

    /// What it is.
    pub fn kind(self) -> RemoteKind {
        self.kind
    }

    /// The ability whose window it lives in (§8.2).
    pub fn source(self) -> AbilityId {
        self.source
    }
}

/// What `id` puts into the facility and hands the keys to, or `None` for the abilities
/// that act on the player's own body (§8.1/#273) — **the one table** that says which
/// abilities transfer control.
///
/// The coded counterpart of [`declares`](crate::state::declares): the turn loop asks
/// *"does activating this hand the keys to something?"* without naming an ability, so
/// a second remote-controlling ability joins the loop, the renderer and the vision
/// union by adding a row here. `const` and total, so the answer is a fact about the
/// catalogue rather than a check somebody must remember to write.
pub const fn remote_kind(id: AbilityId) -> Option<RemoteKind> {
    match id {
        AbilityId::Drone => Some(RemoteKind::Drone),
        AbilityId::Run
        | AbilityId::Camouflage
        | AbilityId::Decoy
        | AbilityId::Dephase
        | AbilityId::Autodoors
        | AbilityId::Confusion
        | AbilityId::Vision
        | AbilityId::PierceWall
        | AbilityId::Lockdown
        | AbilityId::Saver
        | AbilityId::FalseCall
        | AbilityId::Guide
        // The dart is a projectile and not a machine (§8.3/#239): it resolves on the turn
        // it is fired and there is nothing left in the world to fly, so nothing changes
        // hands.
        | AbilityId::Dart
        // The field is ground, not a machine (§8.3/#554): it is laid on the floor and left
        // there, and there is nothing to drive — the player's keys stay their own for its
        // whole window.
        | AbilityId::Repel
        // A table is furniture, not a machine (§8.3/§10.3/#562): it is put on the floor
        // and shoved along by hand, so there is nothing to drive and the player's keys
        // stay their own for its whole window.
        | AbilityId::Cover => None,
    }
}

/// Whether `id` transfers control when activated — [`remote_kind`] with the kind
/// thrown away, for the callers that only need the question answered (the near line's
/// silence rule, the ability bar's ladder).
pub const fn transfers_control(id: AbilityId) -> bool {
    remote_kind(id).is_some()
}
