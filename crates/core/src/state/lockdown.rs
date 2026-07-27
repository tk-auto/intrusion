//! Lockdown (§8.3/§10.4, #242): shut and seal the doors around you for a window,
//! then hand them all back.
//!
//! The ability is a **snapshot** and a **release**, and everything delicate about it is
//! in those two words.
//!
//! # A temporary wall, and why it can only ever be temporary
//!
//! §10.4's baseline is *"anyone can operate any door. No keys, no locks"* **[START]**,
//! and this is the bounded exception the design already anticipated. What a seal buys
//! is a **detour**: a guard cannot work a sealed handle, so its route runs the long way
//! round and it loses the turns that costs (§7.6, "cornered and cut off" inverted in
//! the player's favour). It is not a hiding place and not invincibility — the wall is
//! made of time.
//!
//! §2.2/§7.2 forbid the class of ability that can *permanently* sever pathing (the
//! soft-lock, #170/#182), and the guarantee here is structural rather than careful:
//!
//! - **The window is the ability's own duration.** A seal carries no clock of its
//!   own ([`DoorLock::Sealed`](crate::DoorLock)), so there is no second timer to
//!   outlive the first, and the longest a door can be sealed is the §8.3 duration —
//!   a number on the catalog row, bounded by the type system at `u32` and by
//!   playtest in practice.
//! - **The release is total.** [`release_lockdown`](State::release_lockdown) walks
//!   *every* door in the level rather than a remembered set
//!   ([`Layout::release_sealed_doors`](crate::Layout)), so there is no bookkeeping to
//!   fall out of step with and no door that can be missed.
//! - **The player is never the one walled in.** A locked door refuses the *guards'*
//!   handle, never the player's — see below — so even mid-window the player's own
//!   connectivity is exactly what it was.
//!
//! # It is your lock, so it does not refuse you
//!
//! The player bumps a sealed door open exactly as they bump any other closed door
//! (§10.4/§4.3): nothing in the bump path consults the lock. That is the ticket's open
//! question settled, and it settles it in the direction that cannot go wrong — a
//! lockdown can never box its own owner in, whatever the geometry.
//!
//! It is not free, either, which is what stops the seal being a one-sided wall with no
//! decision in it. Opening one costs the turn (§4.4) and leaves the door **open**: the
//! seal locks a handle, it does not hold a door shut, so a door you reopened is a door
//! the pursuer behind you walks straight through. Firing a lockdown across a route you
//! still have to travel is therefore a real mistake a player can make, and the cost of
//! unmaking it is paid in exactly the turns the ability was bought to save.
//!
//! # What the guard does about it
//!
//! Two halves, both needed. Its **route** treats a closed sealed panel as solid, so it
//! plans the long way round instead of walking into a door it cannot open
//! ([`sealed_route_blocks`](State::sealed_route_blocks)). And should it arrive at one
//! anyway — a destination on the doorway itself, a route sealed under its feet — its
//! walk-in open is simply declined ([`door_locked_at`](State::door_locked_at)) and it
//! holds that turn. It never deadlocks: the window ends, and everything it was going to
//! do it does then.

use super::*;

impl State {
    /// The doors a lockdown fired **right now** would seal (§8.3/#242): every door with
    /// a cell inside the [`LOCKDOWN_RADIUS`] box of the player, reaching through walls
    /// like the guard sense (§9).
    ///
    /// A pure derived function of state, in the shape [`bore_target`](Self::bore_target)
    /// established (§11.4's derived-function rule): the turn loop gates the activation
    /// on it being non-empty, the seal itself is applied to exactly this set, and a test
    /// reads it to check the reach — so the rule, the effect and the assertion cannot
    /// drift apart.
    ///
    /// Measured to the door's **whole footprint** — hinges included — because a doorway
    /// is one object: half of a span inside the box and half outside is still a door
    /// you are standing next to, and sealing "most of a door" is not a thing a door
    /// can be.
    pub fn lockdown_doors(&self) -> Vec<DoorId> {
        // The geometry comes from the one firing seam
        // ([`lockdown_area`](Self::lockdown_area)), so the box the doors are picked out
        // of is the very box the player is shown — the rule and the picture are one
        // object rather than two that agree.
        let reach = self.lockdown_area();
        self.layout
            .regions()
            .doors()
            .filter(|(_, door)| door.cells().any(|cell| reach.contains(cell)))
            .map(|(id, _)| id)
            .collect()
    }

    /// Shut and seal every door of `doors` (§8.3/#242) — the whole world change the
    /// activation makes, applied after the turn loop has checked the set is non-empty.
    ///
    /// The shut goes through §10.4's crush-safe close, so a door with someone standing
    /// in its throat stays open and is merely locked; the lock always lands.
    ///
    /// The closes are **not** reported as [`DoorClosed`](Event::DoorClosed) events, one
    /// per door: they are not a door swinging shut, they are one act of the player's
    /// own, and narrating three of them would spend the near line's single row on a
    /// fact the board has already drawn (the same reasoning that keeps
    /// [`WallBored`](Event::WallBored) quiet, §11.7). What is reported is the act —
    /// [`DoorsSealed`](Event::DoorsSealed), carrying how many.
    pub(super) fn seal_doors(&mut self, doors: &[DoorId], events: &mut Vec<Event>) {
        for &id in doors {
            let player = self.player;
            let guards = &self.guards;
            let bodies = &self.bodies;
            self.layout
                .seal_door(id, |c| actor_occupies(player, guards, bodies, c));
        }
        events.push(Event::DoorsSealed {
            reach: self.lockdown_area(),
            count: doors.len(),
        });
    }

    /// Release every seal (§8.3/#242) — the end of the window, however it ended: the
    /// duration running out (§8.2) or the free toggle-off (§4.4). Idempotent, and safe
    /// to call when nothing is sealed.
    ///
    /// A released door is an ordinary door again: it does not spring open, and whatever
    /// pose it was left in is the pose it keeps. The wall was time, and the time is up.
    pub(super) fn release_lockdown(&mut self) {
        self.layout.release_sealed_doors();
    }

    /// Whether the door at `cell` is locked (§10.4/#242) — the question the guard's
    /// walk-in open asks before it works a handle.
    ///
    /// Deliberately keyed on the *door*, not on the cell's terrain: a lock is a fact
    /// about the doorway, and a sealed door standing open is still sealed. Every lock
    /// source answers here, so #236's key-gated doors will refuse a guard through the
    /// same call.
    pub(super) fn door_locked_at(&self, cell: Cell) -> bool {
        let regions = self.layout.regions();
        regions
            .door_at(cell)
            .is_some_and(|id| regions.door(id).is_locked())
    }

    /// The cells a **guard's route** must treat as solid because a seal holds them
    /// (§8.3/§7.6/#242): the panels of every locked door that is currently *closed*.
    ///
    /// This is what turns the seal into a wall rather than a wait. Without it a guard
    /// would keep planning straight through the doorway §10.4 makes routable and then
    /// bump a handle that refuses it, every turn until the window closed — technically
    /// not a deadlock, but a guard standing still in front of a door is not "routing
    /// the long way round", which is the whole thing the ability buys.
    ///
    /// Only **closed** panels: an open doorway is passable whoever locked it, and only
    /// **panels**, since hinges are permanently solid and no route crosses one anyway.
    /// It is handed to the router as blocked cells rather than stamped into terrain,
    /// because the lock is live state on the door (§11.3) and the *player's* routing is
    /// deliberately unchanged by it — the one asymmetry this ability is made of.
    pub(super) fn sealed_route_blocks(&self) -> Vec<Cell> {
        let regions = self.layout.regions();
        regions
            .doors()
            .filter(|(_, door)| door.is_locked() && !door.is_open())
            .flat_map(|(_, door)| door.panels().iter().copied())
            .collect()
    }

    /// Every cell of every sealed door (§11.5/#242) — the persistent **mark** the
    /// renderer paints in [`Category::Effect`](crate::Category), the door-side twin of
    /// the per-guard mark Confusion carries
    /// ([`guard_under_effect`](Self::guard_under_effect)).
    ///
    /// The mark, not the flash, is what holds the state: the footprint box shows for a
    /// turn to teach the reach, and then these say *which doors are actually sealed*
    /// for as long as they are — the fact the player is playing off. It is drawn
    /// through the fog because it is the player's own gadget: your lock is not
    /// something the building can keep from you (the same reasoning that lets the
    /// effect footprint reach over unseen ground, §11.5).
    pub fn sealed_door_cells(&self) -> impl Iterator<Item = Cell> + '_ {
        self.layout
            .regions()
            .doors()
            .filter(|(_, door)| door.is_locked())
            .flat_map(|(_, door)| door.cells().collect::<Vec<_>>())
    }
}
