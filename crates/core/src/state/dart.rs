//! The **Dart** (§7.2/§8.3/§8.4, #239): a takedown at range, fired along the cardinal
//! you face.
//!
//! This is the experiment, and it is the one ability in the catalogue that deliberately
//! reopens the failure §2.3 records: *"the neutralise ability … unlimited range, no
//! cooldown, and it did not consume a turn"*, and *"auto-target-nearest-visible was the
//! path of least resistance"*. So the module is written to be read against that
//! paragraph, and every safeguard is in one place where it can be checked.
//!
//! # There is no target selection anywhere in here
//!
//! [`State::dart_shot`] walks **one line** — the cells due `facing`, out from the player,
//! in order — and stops. It never enumerates guards, never sorts candidates, never asks
//! which one is nearest or most visible, and never adjusts the line toward anything. What
//! is on the line is what the dart finds, up to and including nothing. That is the
//! §8.4/appendix 1 ban satisfied **by construction** rather than by a rule somebody has
//! to keep obeying: there is no candidate set for a later edit to start picking from.
//!
//! The aim is therefore paid for on the board. To take a guard down at range you have to
//! be in the corridor, on its line, pointing the right way, inside [`DART_RANGE`], with
//! the guard unaware and in your sight — a position that costs movement, turns and
//! exposure, all of which the guards can punish. A cursor would have cost two keypresses.
//!
//! # Stopping and hitting are two different questions
//!
//! - **What stops the dart** is physical: the first cell that a body could not walk into
//!   ([`Terrain::blocks_movement`]) — wall, hinge, closed door panel, table, duct entry,
//!   console, comms terminal, crate, exit — or the first **guard**, whether or not the
//!   player can see it. A body is non-solid (§7.2) and a decoy is a thing of your own, so
//!   neither stops anything; see [`State::dart_shot`] for why the decoy in particular
//!   must not.
//! - **What the dart hits** is §7.2's own gate, unmoved: the guard it stopped on is taken
//!   down only if it has **not detected the player** ([`State::guard_detects_now`]) *and*
//!   is in the player's **line of sight** ([`GuardPerception::Seen`], §6). Anything else
//!   is a miss, and a miss costs the turn, the lockout and the level's only dart.
//!
//! The separation is what stops the ability being an aiming aid. A guard the player can
//! only *sense* is a real obstacle to the dart and not a legal target, so the shot behaves
//! the same whether or not the player has worked out what is standing in the corridor.
//!
//! # The clamp, and the free information it exists to refuse
//!
//! The reach fired is `min(`[`DART_RANGE`]`, sense_range())` — Confusion's **[SETTLED]**
//! clamp, borrowed for a different reason and, like it, able only ever to shrink.
//! Confusion clamps because freezing a guard you cannot perceive has no readout at all.
//! This clamps because of what a *miss* would otherwise tell you: the flight is painted
//! (§11.5), and a wash that stopped six cells short on a guard the player could not
//! perceive would report a body in the dark, for the price of one press.
//!
//! **On open floor the clamp is inert, and that is provable rather than lucky.** Every
//! terrain that blocks **sight** also blocks **movement** — wall, hinge, closed panel and
//! duct entry are the whole opaque set, and all four are solid, which
//! [`the_dart_cannot_outreach_the_player_on_open_floor`] pins. So along a cardinal nothing
//! can break the sightline without also stopping the dart; add [`DART_RANGE`] < the §5
//! sight range and a ray that runs down the middle of the §5 arc, and every cell an
//! open-floor dart reaches is a cell the player can already see. `min(8, 10)` changes
//! nothing there.
//!
//! **The clamp is for the crawlspace** (§10.7), which is where all of that stops holding.
//! It is worth being concrete, because the first version of this module reasoned its way to
//! "no clamp needed" and was wrong twice over — and both mistakes came from assuming a duct
//! is made of wall:
//!
//! - **A mouth has a floor neighbour.** It is the cell you climb out onto, so a dart fired
//!   out of a mouth flies into the room.
//! - **A duct's interior keeps whatever terrain it already had** ([`Duct`](crate::Duct)
//!   records the path; nothing stamps the cells). A crawl may cross room and corridor
//!   *floor* to join two far regions, so a dart fired from mid-crawl is not stopped by the
//!   crawlspace either — the player's own exit tunnel is exactly this, and firing along it
//!   flies over its floor and stops at the `E`.
//!
//! Meanwhile a crawler's live sight is only the mouth peek, and a mid-duct cell sees
//! *nothing at all*, so an unclamped dart flew eight cells into a room the player had no
//! picture of and its wash reported what it found there. The clamp cuts the reach to
//! `DUCT_SENSE_RANGE` = 5, inside which every guard is at least a §9 dot, so the wash can
//! only ever stop on something already drawn.
//!
//! That leaves the §6 sight gate below doing real work rather than restating the geometry:
//! a shot fired blind from mid-duct reaches cells the player cannot see, and the gate is
//! what makes it a miss instead of a takedown fired at a hunch.
//!
//! # It is never refused, and that is deliberate
//!
//! [`State::aim`] always admits a dart the economy allows. Refusing the press for want of
//! a target — or greying the bar entry when the line is empty — would answer *"is there a
//! guard in front of me?"* for free, every frame, without spending the turn. That is the
//! reasoning §8.3 spells out for False Call, and it applies with more force here, because
//! the answer this one would give is worth a takedown. So the press always fires, always
//! costs its turn and its use, and the near line says so (§11.7).
//!
//! For the same reason the miss says **one thing** whatever it found. An empty line, an
//! aware guard, and a guard the player can only sense are one message: three messages
//! would be a detector with three settings.
//!
//! # The body is the counterweight (§7.3)
//!
//! A dart drops its guard **where it stood**, which is the whole point and also the whole
//! cost. That cell is usually several down a corridor the player was not planning to walk,
//! so the body often cannot be reached to stow (§7.2) — and it misses pings on the guard's
//! own radio cadence like any other (§7.3). §7.2's economy is that a takedown you cannot
//! hide is a takedown that finds you later; at range that stops being a choice.

use super::*;

/// A resolved dart, as [`State::dart_shot`] worked it out (§8.3/#239) — the geometry the
/// firing, the event and the mark all read, so none of the three can disagree about where
/// the dart went.
///
/// Carried through [`Aimed`](super::activation::Aimed) rather than re-derived at the
/// firing seam, on the same discipline every other aimed effect follows: what the world
/// change acts on is the thing the precondition ladder approved.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DartShot {
    /// The cardinal it was fired along — the player's §5 facing at the moment of the
    /// press.
    dir: Direction,
    /// The cells it passed through, in flight order, starting with the cell next to the
    /// player. Empty only where the very first cell is off the board or solid.
    path: Vec<Cell>,
    /// The cell of the guard it **takes down**, or `None` for a miss (§7.2's gate).
    ///
    /// A cell rather than an index into [`State::guards`], so the firing looks the guard
    /// up for itself ([`guard_at`](State::guard_at)) exactly as the adjacent takedown
    /// does — nothing here holds a position in a list that another rule might shorten.
    hit: Option<Cell>,
}

impl DartShot {
    /// The cardinal the dart flew along.
    pub fn direction(&self) -> Direction {
        self.dir
    }

    /// The cells it flew through, in order.
    pub fn path(&self) -> &[Cell] {
        &self.path
    }

    /// How many cells it travelled before something stopped it.
    pub fn travelled(&self) -> u32 {
        self.path.len() as u32
    }

    /// The cell of the guard it takes down, or `None` for a miss.
    pub fn hit(&self) -> Option<Cell> {
        self.hit
    }
}

impl State {
    /// **The dart the player would fire right now** (§7.2/§8.3/§8.4/#239): the line out
    /// from their cell along their §5 facing, where it stops, and whether the guard it
    /// stopped on is a legal §7.2 target.
    ///
    /// A **pure derived function of state** (§11.4), so the sim bot's cue, the firing and
    /// the mark the player reads all ask this one question and get the one answer. It is
    /// cheap by construction — at most [`DART_RANGE`] cells — which matters because
    /// [`aim`](Self::aim) is asked every frame.
    ///
    /// # The walk, in the order the rules apply
    ///
    /// 1. **Reach.** `min(`[`DART_RANGE`]`, `[`acting_sense_range`](Self::acting_sense_range)`)`
    ///    — the clamp, which can only shrink, and which is inert everywhere but a
    ///    crawlspace (the module note has the argument and the counter-example). Read
    ///    through the *acting* sense for [`confusion_blast`](Self::confusion_blast)'s
    ///    reason: this is a pure function a cue may ask on any frame, and it must give the
    ///    answer the press itself would even on the frame straight after a Wait.
    /// 2. **One cell at a time**, from the player outward. Off the board ends the flight.
    /// 3. **A solid ends it, and the dart does not enter.** The stopper is
    ///    [`Terrain::blocks_movement`] — the one existing notion of solid, so the list is
    ///    the terrain table's rather than a second one kept in step here. Worth naming
    ///    because it is the interesting one: **a table stops the dart**, though sight goes
    ///    straight over it (§10.3). That is not an oversight and it is load-bearing — §10.1a
    ///    stamps partial cover into every over-long straight run, so the long clear firing
    ///    line down a corridor is the very geometry the sightline rule keeps a level from
    ///    being born with. The generator bounds the corridor shot before any number here
    ///    does.
    /// 4. **A guard ends it, and the dart does reach that cell.** Any guard, seen or
    ///    merely sensed or neither: a dart is a physical thing and stops at the first body
    ///    on its line, which is what keeps the flight from being a question about what the
    ///    player has worked out.
    ///
    /// **What it flies over.** A loose body — non-solid (§7.2), lying on the floor, and
    /// something Run already sprints across. A **decoy**: it is a fake intruder of the
    /// player's own making (§8.3), so stopping on one would mean spending the level's only
    /// dart on your own prop, which is a joke the player gets exactly once and never
    /// forgives. A **remote**: not an actor, blocks nothing, is blocked by nothing
    /// (§8.3/#273). None of the three is terrain, so all three simply are not consulted —
    /// the omission is the rule, and it is stated here because it would otherwise read as
    /// a gap.
    ///
    /// # The hit gate is §7.2's, unchanged
    ///
    /// The guard the dart stopped on goes down only if it has **not detected the player**
    /// ([`guard_detects_now`](Self::guard_detects_now)) — the same live cone read the
    /// adjacent bump takedown uses, so there is one definition of *unaware* in the game —
    /// **and** the player can **see** it ([`GuardPerception::Seen`], §6). The sight half is
    /// the requirement the range makes necessary: adjacency made it free (you are touching
    /// the thing), and at eight cells a guard known only as a §9 dot would be a takedown
    /// fired blind at a cell, with §7.2's unaware requirement uncheckable by the player. It
    /// is checked on every shot even though the geometry cannot currently produce a shot
    /// that fails it (see the module note) — the rule is the rule, and the containment it
    /// leans on is pinned rather than assumed.
    pub fn dart_shot(&self) -> DartShot {
        let reach = DART_RANGE.min(self.acting_sense_range());
        let facility = self.layout.facility();
        let mut path = Vec::new();
        let mut hit = None;
        let mut at = self.player;
        for _ in 0..reach {
            let Some(next) = at.step(self.facing) else {
                break; // off the north or west edge — nothing beyond the board (§4.1)
            };
            let Some(terrain) = facility.terrain(next) else {
                break; // off the south or east edge, likewise
            };
            if terrain.blocks_movement() {
                break; // the dart stops *against* the solid, never inside it
            }
            path.push(next);
            at = next;
            if let Some(i) = self.guard_at(next) {
                let guard = &self.guards[i];
                if !self.guard_detects_now(guard)
                    && self.perceive_guard(guard) == Some(GuardPerception::Seen)
                {
                    hit = Some(next);
                }
                break; // a body on the line stops the dart whether or not it was legal
            }
        }
        DartShot {
            dir: self.facing,
            path,
            hit,
        }
    }

    /// **Fire `shot`** (§7.2/§8.3/#239): take its guard down if it found a legal one, and
    /// report the flight either way.
    ///
    /// Called from the activation seam once the deck has actually switched the ability on,
    /// so nothing here fires on a refused press — and the shot handed over is the one
    /// [`aim`](Self::aim) resolved, never a fresh walk of the line.
    ///
    /// **The takedown is the ordinary one**, deliberately: the guard is removed, a
    /// [`Body`] is left in *its own* cell carrying its radio cadence, the §7.3 clock
    /// starts, the belt key is lifted (§10.4/#236), and the run's tally counts it. Every
    /// one of those is the adjacent takedown's behaviour reached through a different verb,
    /// which is what the ticket means by *"the §7.3 radio clock runs unchanged"* — and it
    /// is why no surface needs an arm of its own for a dart's body.
    ///
    /// Nothing steps the alert (§7.3). Nothing was seen and no ping has yet been missed;
    /// what a dart costs the player arrives later, when the body it left out of reach is
    /// found or goes quiet on the radio.
    pub(super) fn fire_dart(&mut self, shot: &DartShot, events: &mut Vec<Event>) {
        // The flight is reported first, so the near line reads the dart and then its
        // consequence in the order they happened (§11.7).
        events.push(Event::DartFired {
            from: self.player,
            dir: shot.direction(),
            travelled: shot.travelled(),
            hit: shot.hit().is_some(),
        });
        let Some(target) = shot.hit() else {
            return; // a miss: the turn, the lockout and the level's dart, for nothing
        };
        let i = self
            .guard_at(target)
            .expect("dart_shot resolved its hit on a guard standing here");
        let guard = self.guards.remove(i);
        // Where it stood, not where the player stands — the whole cost of shooting at
        // range (§7.3): the body lies out there, on a route you may not be able to walk
        // back down, missing pings on the downed guard's own cadence.
        self.bodies
            .push(Body::new(target, guard.radio_clock(), self.turn));
        events.push(Event::TakenDown { at: target });
        self.take_key_from_guard(target, events);
    }
}
