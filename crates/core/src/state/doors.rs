//! Doors, as the world works them (§10.4) and as the player feels them (§9.4).
//!
//! Two halves of one subsystem, kept together and out of [`state.rs`](super):
//!
//! - **The world's door turn.** [`door_phase`](super::State::door_phase) ticks every
//!   automatic door's self-close (§10.4), and the Calm guard's chance to pull a
//!   hinged door shut behind itself rides on
//!   [`door_exited`](super::State::door_exited) /
//!   [`close_behind_door`](super::State::close_behind_door). Guard traffic opens the
//!   facility up over a level; this is the counter-pressure.
//! - **The cues the player reads.** A door that opens or shuts out of sight is a
//!   *discrete* event — there is no standing position to re-read each frame — so it
//!   is latched the turn it happens ([`record_door_cues`](super::State::record_door_cues))
//!   and fades over [`DOOR_CUE_DECAY_TURNS`](super::DOOR_CUE_DECAY_TURNS) world turns
//!   ([`decay_door_cues`](super::State::decay_door_cues)). The renderer reads the
//!   footprint through [`door_cues`](super::State::door_cues) (§9.4).
//!
//! The **Autodoors** ability's close-behind set (§8.3/§7.6) lives here too: a door it
//! opened in the player's path is armed
//! ([`arm_autodoor_close`](super::State::arm_autodoor_close)) and swings shut on the
//! first world turn its throat is clear, through the §10.4 crush-safe close
//! ([`close_armed_autodoors`](super::State::close_armed_autodoors)) — doors never
//! crush.

use super::*;

impl State {
    /// Fade the door-change cues by one world turn (§9.4/§10.4), dropping any that
    /// have burned out. Runs once per spent turn, at the head of the world phases.
    pub(super) fn decay_door_cues(&mut self) {
        self.door_cues.retain_mut(|cue| {
            cue.ttl -= 1;
            cue.ttl > 0
        });
    }

    /// Latch a fading on-grid cue (§9.4/§10.4) on every door that changed state this
    /// turn **away from the player** and within [`door_sense_range`](Self::door_sense_range):
    /// the guard-driven opens and the guard/automatic closes (all `by_player: false`).
    /// A door *you* operated keeps its quiet near-line self-narration (§11.7) and
    /// lights no cue. A change beyond the door-sense box is felt as nothing. Range is
    /// measured to the panel the event named; the cue then lights the door's whole
    /// footprint (§9.4).
    pub(super) fn record_door_cues(&mut self, events: &[Event]) {
        let range = self.door_sense_range();
        for event in events {
            let at = match *event {
                Event::DoorOpened {
                    at,
                    by_player: false,
                }
                | Event::DoorClosed {
                    at,
                    by_player: false,
                } => at,
                _ => continue,
            };
            if self.player.sight_distance(at) > range {
                continue;
            }
            if let Some(door) = self.layout.regions().door_at(at) {
                self.mark_door_cue(door);
            }
        }
    }

    /// Light or refresh the cue on `door` to full life (§9.4/§10.4). A door that
    /// changes again while its mark still shows simply resets the fade — one door,
    /// one cue, its whole footprint lit.
    fn mark_door_cue(&mut self, door: DoorId) {
        if let Some(cue) = self.door_cues.iter_mut().find(|cue| cue.door == door) {
            cue.ttl = DOOR_CUE_DECAY_TURNS;
        } else {
            self.door_cues.push(DoorCue {
                door,
                ttl: DOOR_CUE_DECAY_TURNS,
            });
        }
    }

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
