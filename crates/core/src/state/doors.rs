//! Doors, as the world works them (§10.4).
//!
//! [`door_phase`](super::State::door_phase) ticks every automatic door's self-close
//! (§10.4), and the Calm guard's chance to pull a hinged door shut behind itself rides
//! on [`door_exited`](super::State::door_exited) /
//! [`close_behind_door`](super::State::close_behind_door). Guard traffic opens the
//! facility up over a level; this is the counter-pressure.
//!
//! **What the player *feels* of all this lives next door**, in [`sense`](super::sense):
//! a door that opens or shuts out of sight is a discrete event, latched the turn it
//! happens and faded over a few turns — the same persist-and-fade machinery as the
//! guard sense, because they are one channel (§9.4/#192).
//!
//! The **Autodoors** ability's close-behind set (§8.3/§7.6) lives here too: a door it
//! opened in the player's path is armed
//! ([`arm_autodoor_close`](super::State::arm_autodoor_close)) and swings shut on the
//! first world turn its throat is clear, through the §10.4 crush-safe close
//! ([`close_armed_autodoors`](super::State::close_armed_autodoors)) — doors never
//! crush.

use super::*;

impl State {
    /// The automatic doors' self-close tick (§10.4/#147), run once per world turn
    /// after everyone has moved so it reads final positions. Each open automatic door
    /// whose doorway is clear counts down and, when its timer runs out, shuts —
    /// exactly as a manual close does, panels restamped solid so vision, sound
    /// occlusion and the renderer all track it. An occupied doorway rearms instead:
    /// an automatic door never crushes (§10.4). Every shut the player might see is
    /// reported as a [`DoorClosed`](Event::DoorClosed), the same event a guard-close
    /// (#146) raises.
    pub(super) fn door_phase(&mut self, events: &mut Vec<Event>) {
        let player = self.player;
        let guards = &self.guards;
        let bodies = &self.bodies;
        let closed = self
            .layout
            .tick_auto_doors(|c| actor_occupies(player, guards, bodies, c));
        for id in closed {
            let at = self.layout.regions().door(id).panels()[0];
            events.push(Event::DoorClosed {
                at,
                by_player: false,
            });
        }
    }

    /// The door a guard just walked *out of*, if its step from `from` to `to` left a
    /// doorway behind (§10.4/#146): `from` is one of that door's panels and `to` is
    /// no longer part of the same door. `None` otherwise — the guard was not on a
    /// panel, or it merely slid along a wide opening from one panel to another and is
    /// still in the throat. Only a **manual** door qualifies: an automatic door has no
    /// handle for a guard to shut and closes itself on a timer instead (§10.4/#147),
    /// so a guard passing through one leaves the auto-close to do the work.
    pub(super) fn door_exited(&self, from: Cell, to: Cell) -> Option<DoorId> {
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
    pub(super) fn rolls_a_close(&mut self) -> bool {
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
    pub(super) fn close_behind_door(&mut self, door: DoorId) -> bool {
        let player = self.player;
        let guards = &self.guards;
        let bodies = &self.bodies;
        matches!(
            self.layout
                .close_behind(door, |c| actor_occupies(player, guards, bodies, c)),
            Some(DoorAction::Closed)
        )
    }

    /// The door whose **hinge** `cell` is (§10.4) — the solid frame end you bump to
    /// open (#148) or close — or `None` for a panel, or a cell on no door at all. The
    /// one place hinge-ness is decided, so the #148 peek and the #320 frame mark ask
    /// the same question. A frameless **automatic** door (#147) has no hinges, so
    /// every one of its cells answers `None` and neither behaviour can reach it.
    pub(super) fn hinge_door_at(&self, cell: Cell) -> Option<DoorId> {
        let regions = self.layout.regions();
        let id = regions.door_at(cell)?;
        (regions.door(id).role(cell)? == DoorCell::Hinge).then_some(id)
    }

    /// Whether bumping `cell` is the **withheld frame** (#320): the hinge of the door
    /// the player's immediately preceding action opened, whose close is suppressed for
    /// exactly that one action so the bump can be read as a dead bump and slid past
    /// (#57) instead of undoing the open. False for every other hinge — a door already
    /// open, one a guard opened, one whose mark has expired — which close as always
    /// (§10.4: the hinge is the handle).
    pub(super) fn frame_bump_withheld(&self, cell: Cell) -> bool {
        self.door_just_opened.is_some() && self.hinge_door_at(cell) == self.door_just_opened
    }

    /// Expire the #320 frame mark at the end of an action, `carried` being the mark
    /// the action *started* with. The window is exactly one action — free or spent,
    /// a bump, a wait, an ability, anything — so unless phase 1 has just replaced the
    /// mark with a fresh hinge open, whatever was carried in is spent and cleared.
    /// Clearing it is pure bookkeeping, never a world change, so it does not make a
    /// free action cost the turn (§4.4).
    pub(super) fn expire_frame_bump_mark(&mut self, carried: Option<DoorId>) {
        if self.door_just_opened == carried {
            self.door_just_opened = None;
        }
    }

    /// Whether `cell` is a door **panel** — the walk-through part of a doorway, as
    /// opposed to a hinge (the solid handle, #148). The Autodoors step (§8.3) offers
    /// only a panel, since a hinge cannot be stood on even once the door is open.
    pub(super) fn door_panel_at(&self, cell: Cell) -> bool {
        let regions = self.layout.regions();
        regions
            .door_at(cell)
            .and_then(|id| regions.door(id).role(cell))
            == Some(DoorCell::Panel)
    }

    /// Arm the door at `cell` for the Autodoors close-behind (§8.3/§7.6): remember it
    /// so [`close_armed_autodoors`](Self::close_armed_autodoors) shuts it the moment
    /// the player clears the throat. Both kinds are armed: a **manual** door has no
    /// self-close, and an **automatic** door would otherwise linger open for its full
    /// `delay` (§10.4/#147) — too slow for a flight tool, so the ability shuts it
    /// early too (the edge is the *prompt* break of sight, §7.6). Its own
    /// [`tick_auto_doors`](Layout::tick_auto_doors) still governs it when opened any
    /// other way. Idempotent: a door already armed (the player lingering on its panel)
    /// is not queued twice.
    pub(super) fn arm_autodoor_close(&mut self, cell: Cell) {
        let regions = self.layout.regions();
        let Some(id) = regions.door_at(cell) else {
            return;
        };
        if self.autodoors_pending.contains(&id) {
            return;
        }
        self.autodoors_pending.push(id);
    }

    /// Shut every armed Autodoors door whose throat has cleared (§8.3/§7.6) — the
    /// player-driven half of the §10.4 close, run once per world turn after everyone
    /// has moved. Each armed door still open is closed the crush-safe way
    /// ([`close_behind_door`], so it never shuts on the player or a dragged body,
    /// §10.4); a door that closes drops from the set and reports a
    /// [`DoorClosed`](Event::DoorClosed) — `by_player`, since the player's ability
    /// caused it, so it self-narrates (§11.7) and lights no sensed cue. A door still
    /// obstructed stays armed for a later turn, and one already shut (the player
    /// walked back through, or it timed out) is simply forgotten.
    pub(super) fn close_armed_autodoors(&mut self, events: &mut Vec<Event>) {
        // Take the queue so the crush-safe close can borrow `self` freely; still-
        // obstructed doors are pushed back on.
        let pending = std::mem::take(&mut self.autodoors_pending);
        for id in pending {
            if !self.layout.regions().door(id).is_open() {
                continue; // already shut — nothing owed
            }
            let at = self.layout.regions().door(id).panels()[0];
            if self.close_behind_door(id) {
                events.push(Event::DoorClosed {
                    at,
                    by_player: true,
                });
            } else {
                self.autodoors_pending.push(id); // throat still occupied; retry
            }
        }
    }
}
