//! The sense channel (§9), as one persist-and-fade system.
//!
//! The player perceives two things through walls, and since #192 they are recorded the
//! **same way**: a guard's exact cell (§9.1/§9.2) and a door that opened or shut away
//! from them (§9.4). Each is a [`SenseCue`] with a life in world turns, stamped the
//! turn the fact is true ([`record_guard_cues`](State::record_guard_cues),
//! [`record_door_cues`](State::record_door_cues)) and aged once per world turn
//! ([`decay_sense_cues`](State::decay_sense_cues)). The renderer reads the union as
//! [`SenseMark`]s — a cell and how many turns old it is — and paints the whole channel
//! in one colour at two strengths (§11.2/§11.5).
//!
//! **What the shared model buys.** The door cue was already a fading mark, because a
//! door change is discrete. The guard sense was a hard on/off dot, so the two halves of
//! one channel behaved differently: a live dot here, a fading mark there. Stamping the
//! guard's cell every turn gives the same fade one rule, and the rule produces both of
//! the things §9.4's [OPEN] note wanted:
//!
//! - a **trail** behind a guard moving inside the sense box — the freshest mark is the
//!   cell it stands in, the older ones the cells it just left; and
//! - a **ghost** when a guard leaves the box (or the widened Wait range lapses back to
//!   the walking one, §9.1): its last felt cell lingers a moment instead of blinking
//!   out, so "I have lost it, it was just there" is on the board.
//!
//! **What the trail must not become (§9.2).** The sense gives position, never intent.
//! A trail long enough to extrapolate would be an arrow — heading handed to the player
//! for free — so the guard cue's life ([`GUARD_CUE_DECAY_TURNS`]) is deliberately the
//! *shortest* thing in the channel: it says "was just here", never "is going that way".
//! Two properties keep that honest and are worth stating, because they are why the
//! trail leaks nothing the board did not already show:
//!
//! - a guard **standing still** stamps the same cell over and over, so it leaves no
//!   trail at all — the watcher whose facing you most want is exactly the one that
//!   gives you nothing; and
//! - a guard **moving** was already legible frame to frame (the dot was there, now it
//!   is here). The trail makes what an attentive player could already read *legible*,
//!   rather than adding a channel.

use super::*;

/// A fading mark in the sense channel (§9/§9.4): what was felt through a wall, and how
/// many more turns the mark shows. Stamped at full life the turn the fact holds and
/// decremented once per world turn, so a cue shows for [`life`](SenseSource::life)
/// renders and is gone on the next.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) struct SenseCue {
    /// What was sensed — and, with it, which cells the mark lights and how long it
    /// lives.
    pub(super) source: SenseSource,
    /// Turns of life left; decremented once per world turn and dropped at zero.
    pub(super) ttl: u32,
}

impl SenseCue {
    /// How many world turns ago the fact was stamped: `0` on the turn it happened,
    /// counting up as the mark fades. This is what the renderer shades by — the core
    /// emits an age and never a colour (§11.2).
    pub(super) fn age(self) -> u32 {
        self.source.life().saturating_sub(self.ttl)
    }
}

/// What a [`SenseCue`] was felt from (§9). The two halves of the one channel: a guard's
/// position (§9.1/§9.2) and a door change (§9.4). They differ in what they light and in
/// how long they live, and in nothing else.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum SenseSource {
    /// A door that changed state away from the player — the mark lights its **whole
    /// footprint** (§9.4), so the eye reads "that doorway" rather than one panel.
    Door(DoorId),
    /// A guard felt through a wall, at the exact cell it stood in when the mark was
    /// stamped (§9.2) — one cell, never a glyph, never a heading.
    Guard(Cell),
}

impl SenseSource {
    /// How many world turns this kind of cue lives (§9.4 **[START]**). The door cue is
    /// the longer of the two because it reports a **discrete** event that will not
    /// restate itself: miss it and it is gone. A guard cue is re-stamped every turn the
    /// guard is still felt, so its life is only the tail behind a live position — and
    /// it is kept the shortest thing in the channel so the tail stays "was just here"
    /// rather than a readable heading (§9.2).
    fn life(self) -> u32 {
        match self {
            SenseSource::Door(_) => DOOR_CUE_DECAY_TURNS,
            SenseSource::Guard(_) => GUARD_CUE_DECAY_TURNS,
        }
    }
}

/// One cell of the sense channel as the renderer reads it (§9/§11.2): where something
/// was felt through a wall, and how many turns ago.
///
/// The core states the **age**; the shell owns what age looks like (§11.2 — the core
/// never names a colour). Today's ramp is two steps — the fresh mark full strength, a
/// fading one quiet — which is exactly what the short trail §9.2 permits can carry.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SenseMark {
    /// The cell the mark lights.
    pub cell: Cell,
    /// World turns since the fact was stamped: `0` this turn, counting up as it fades.
    pub age: u32,
}

impl State {
    /// Fade every sense cue by one world turn (§9/§9.4), dropping any that have burned
    /// out. Runs once per spent turn, at the head of the world phases — *before* this
    /// turn's facts can stamp fresh cues, so a cue placed this turn keeps its full life
    /// and a re-stamp refreshes rather than double-decrements.
    pub(super) fn decay_sense_cues(&mut self) {
        self.sense_cues.retain_mut(|cue| {
            cue.ttl -= 1;
            cue.ttl > 0
        });
    }

    /// Stamp a cue on the cell of every guard the player currently **senses** (§9.1/
    /// §9.2) — felt through a wall, not seen. Run at the end of the world phases, so it
    /// reads the positions the guards finished the turn on and the sense range the
    /// turn's action bought (a Wait's widened box, §9.1).
    ///
    /// A guard the player can **see** stamps nothing: it is drawn in full, glyph, facing
    /// and cone (§9.2), and an orange trace under it would be the sense restating what
    /// sight already says. Cross the two and the seam shows exactly as it should — a
    /// guard stepping out of view leaves its last sensed cells fading behind it.
    pub(super) fn record_guard_cues(&mut self) {
        let sensed: Vec<Cell> = self
            .guards
            .iter()
            .filter(|guard| self.perceive_guard(guard) == Some(GuardPerception::Sensed))
            .map(Guard::pos)
            .collect();
        for cell in sensed {
            self.mark_guard_cue(cell);
        }
    }

    /// Light or refresh the guard cue on `cell` to full life (§9.2). One cue per cell:
    /// a guard that holds still (or walks back over its own trail) resets that mark
    /// rather than stacking a second, which is what keeps a stationary guard from
    /// drawing anything the live dot does not already say.
    fn mark_guard_cue(&mut self, cell: Cell) {
        self.refresh_or_push(SenseSource::Guard(cell));
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
        self.refresh_or_push(SenseSource::Door(door));
    }

    /// The one placement rule both halves of the channel share: a cue already standing
    /// for `source` is reset to full life, otherwise a fresh one is pushed. Keyed on the
    /// source, so the set stays small and bounded — a handful of doors, one cell per
    /// sensed guard per turn of its short life — and a plain `Vec` scan beats a map.
    fn refresh_or_push(&mut self, source: SenseSource) {
        let ttl = source.life();
        if let Some(cue) = self.sense_cues.iter_mut().find(|cue| cue.source == source) {
            cue.ttl = ttl;
        } else {
            self.sense_cues.push(SenseCue { source, ttl });
        }
    }
}

#[cfg(test)]
mod tests;
