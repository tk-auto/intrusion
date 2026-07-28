//! The facility alert ladder (§7.3) — three rungs, fixed triggers, no decay.
//!
//! The old alert was §2.3's worst row: *"never written to, never read"*. The radio
//! net (§7.3) fixed the first half — a guard that stops answering steps a number —
//! and this module fixes the second. The number is no longer an open-ended scalar
//! announcing an escalation that does not exist: it is a **three-rung ladder** whose
//! every step is a fact about the run with a concrete, explainable source, and whose
//! every step **does something**.
//!
//! | Rung | Triggers (**any** of) | Retaliation **added** at this rung |
//! |---|---|---|
//! | **1** | A confirmed sighting; **or** one missed radio ping | Guards are **never calm**: the §7.5 patrol dwell drops from 3–7 to [`ALERT_DWELL_TURNS_MIN`]–[`ALERT_DWELL_TURNS_MAX`] turns |
//! | **2** | [`SIGHTINGS_FOR_SECOND_RUNG`] confirmed sightings; **or** an intel console tampered with while at rung ≥ 1 | **+1 guard** enters the facility (#374) |
//! | **3** | A body found; **or** two missed pings across [`SILENT_POSTS_FOR_THIRD_RUNG`] bodies | **+2 guards** enter the facility (#374) |
//!
//! **Effects are cumulative** — a rung applies every effect at or below it — and
//! **rung 3 is the top**. The reinforcements of rungs 2 and 3 are #374; what ships
//! here is the ladder itself plus rung 1's teeth, which is a pressure system that
//! actually runs rather than a facade (§2.3).
//!
//! # Two rules this type exists to hold
//!
//! **No decay.** [`Alert::raise`] is the only writer and it only ever moves the rung
//! **up**: a step is a fact about the run, and nothing un-knows that a guard stopped
//! answering or that you were seen. There is deliberately no timer here. §7.4's
//! decaying lead is the *per-guard* alert and must not be conflated with the facility
//! rung; a console that lowers the rung is a possible later addition (and #213 is the
//! campaign-scale sink), and neither exists now.
//!
//! **Guards never accelerate** (§7.1 **[SETTLED]**). The ladder shortens the pause a
//! Calm guard takes; no rung touches how fast anything moves. That is the tempting
//! wrong answer, so [`State`](crate::State) asserts it.

/// How many turns of certain-zone contact make one **confirmed sighting** (§7.6,
/// **[START] = 3**), inside [`SIGHTING_WINDOW_TURNS`]. Any turn in which **any**
/// guard has the player in the certain zone counts one — the tally is facility-wide,
/// not per guard, so three guards catching one turn each still counts three. A
/// **glimpse**-zone contact counts nothing: being half-seen at 8 cells is not the
/// facility knowing where you are.
pub(crate) const SIGHTING_CONTACT_TURNS: u32 = 3;

/// The sliding window the contact turns must fall inside (§7.6, **[START] = 10**).
/// It must fall back to **zero** — this many turns with no certain-zone contact —
/// before another sighting can be counted, which is what makes "3 sightings" three
/// separate events rather than one long chase reported over and over.
pub(crate) const SIGHTING_WINDOW_TURNS: u32 = 10;

/// How many confirmed sightings reach rung 2 (§7.3, **[START] = 3**), cumulative
/// over the level — the count never falls, like the rung it feeds.
pub(crate) const SIGHTINGS_FOR_SECOND_RUNG: u32 = 3;

/// How many **distinct** posts must fall silent to reach rung 3 (§7.3,
/// **[START] = 2**): two missed pings across two bodies. One body missing its second
/// ping is not this — a post that has already gone quiet telling control nothing new
/// is why the trigger is written across *bodies* rather than across pings.
pub(crate) const SILENT_POSTS_FOR_THIRD_RUNG: u32 = 2;

/// The shortest and longest a Calm dwell lasts once the facility is at rung ≥ 1
/// (§7.5/§7.3, **[START] = 1–3**), replacing the calm
/// [`GUARD_DWELL_TURNS_MIN`](crate::guard::GUARD_DWELL_TURNS_MIN)`..=`[`GUARD_DWELL_TURNS_MAX`](crate::guard::GUARD_DWELL_TURNS_MAX)
/// range. Guards are **never calm again** once the facility knows you are in it: the
/// pause is still there, so a Takedown (§7.2) still has a window to be lined up
/// against, but it is a third of the one a quiet facility gives you.
///
/// The **floor matters more than the ceiling**. Almost every run sits at rung 1 after
/// first contact and it never comes down (no decay), so a dwell that could reach 0
/// would take the Takedown off the table for the rest of the level — which §7.5
/// forbids: the pause *is* the window to act.
pub(crate) const ALERT_DWELL_TURNS_MIN: u32 = 1;
pub(crate) const ALERT_DWELL_TURNS_MAX: u32 = 3;
// Rung 1 must **shorten** the window and never remove it, whatever these [START]s
// are retuned to — held at compile time, like the §7.2 body-vs-sighting relation.
const _: () = assert!(ALERT_DWELL_TURNS_MIN >= 1 && ALERT_DWELL_TURNS_MIN <= ALERT_DWELL_TURNS_MAX);
const _: () = assert!(ALERT_DWELL_TURNS_MAX < crate::guard::GUARD_DWELL_TURNS_MAX);

/// The top of the ladder (§7.3). There is no rung 4: the design specifies three, and
/// control has nothing louder to say than "send everyone".
pub(crate) const TOP_RUNG: u32 = 3;

/// **Why** the facility alert stepped (§7.3) — carried by
/// [`Event::AlertRaised`](crate::Event::AlertRaised) so an escalation is always
/// explainable, on the near line and in the §13.2 sim's attribution (#376).
///
/// Each variant names the rung it reaches ([`rung`](Self::rung)); the ladder takes
/// the **highest** rung any trigger has reached and never comes back down.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AlertTrigger {
    /// A **confirmed sighting** (rung 1): [`SIGHTING_CONTACT_TURNS`] turns of
    /// certain-zone contact inside [`SIGHTING_WINDOW_TURNS`]. Somebody is definitely
    /// in the building, and control now knows it.
    Sighting,
    /// A **missed radio ping** (rung 1): one post stopped answering (§7.3). Control
    /// cannot tell an ambush from a broken radio, but either way the facility stops
    /// being calm.
    MissedPing,
    /// [`SIGHTINGS_FOR_SECOND_RUNG`] confirmed sightings (rung 2): not one long
    /// chase but three separate breaks in stealth — the intruder is *working* the
    /// building.
    RepeatSightings,
    /// An intel console tampered with while already at rung ≥ 1 (rung 2): control
    /// now knows what you came for. Tampering at rung 0 triggers nothing at all —
    /// that is the reward for staying unseen. The **comms console** (`Ψ`) is
    /// deliberately never this trigger: it is the one answer §7.3 gives the player
    /// to the net, and charging alert for it would tax the counterplay.
    ConsoleTampered,
    /// A guard's cone covered a **body** (rung 3): the loudest event in the game
    /// (§7.2) is also the loudest thing the facility can learn.
    BodyFound,
    /// A **second post** fell silent (rung 3): two missed pings across two bodies.
    /// One quiet post is a fault; two is an intruder taking the place apart.
    SecondPostSilent,
}

impl AlertTrigger {
    /// The rung this trigger reaches (§7.3). The ladder is defined by the trigger
    /// table, so this function *is* that table — and it is total over the enum, so a
    /// new trigger cannot be added without saying how loud it is.
    pub fn rung(self) -> u32 {
        match self {
            Self::Sighting | Self::MissedPing => 1,
            Self::RepeatSightings | Self::ConsoleTampered => 2,
            Self::BodyFound | Self::SecondPostSilent => TOP_RUNG,
        }
    }
}

/// The facility's alert (§7.3): the rung it has reached and the tallies its triggers
/// count against. Owned by [`State`](crate::State), stepped from the turn loop, and
/// read by the near line (§11.4), the guards' dwell, and the §13.2 sim.
///
/// It is deliberately a **count of escalations, not a mood**: guard states (§7.4) are
/// per-guard and are never folded into this number.
#[derive(Clone, Debug, Default)]
pub(crate) struct Alert {
    /// The rung, 0..=[`TOP_RUNG`]. Only [`raise`](Self::raise) writes it, and only
    /// upward — that *is* the no-decay rule (§7.3).
    rung: u32,
    /// Confirmed sightings so far this level (§7.6), cumulative and never reset.
    sightings: u32,
    /// The turns inside the live window on which some guard had the player in the
    /// certain zone, oldest first. Pruned every turn, so its length is the window's
    /// current count and an empty vector *is* "the window fell back to 0".
    contacts: Vec<u32>,
    /// Whether the live window has already been counted as a sighting. Cleared when
    /// the window empties, which is what stops one long chase counting three times.
    counted: bool,
    /// How many **distinct** posts have missed a ping (§7.3) — bodies, not pings.
    silent_posts: u32,
}

impl Alert {
    /// A quiet facility: rung 0, nothing tallied.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// The rung the facility has reached (§7.3) — what
    /// [`State::alert`](crate::State::alert) reports.
    pub(crate) fn rung(&self) -> u32 {
        self.rung
    }

    /// How many confirmed sightings have been counted (§7.6). Read by the tests that
    /// pin the sliding window; the player reads the rung, not this.
    #[cfg(test)]
    pub(crate) fn sightings(&self) -> u32 {
        self.sightings
    }

    /// The §7.5 Calm dwell range this rung imposes: the quiet 3–7 at rung 0, the
    /// shortened [`ALERT_DWELL_TURNS_MIN`]–[`ALERT_DWELL_TURNS_MAX`] from rung 1 up.
    /// Cumulative by construction — every rung above 1 keeps rung 1's cut, because
    /// the branch asks "at least 1", not "exactly 1".
    pub(crate) fn dwell_turns(&self) -> (u32, u32) {
        if self.rung >= 1 {
            (ALERT_DWELL_TURNS_MIN, ALERT_DWELL_TURNS_MAX)
        } else {
            (
                crate::guard::GUARD_DWELL_TURNS_MIN,
                crate::guard::GUARD_DWELL_TURNS_MAX,
            )
        }
    }

    /// Step the ladder to `trigger`'s rung (§7.3). Returns the new rung when it
    /// actually **rose** — the escalation an [`Event::AlertRaised`](crate::Event)
    /// reports — and `None` when the facility was already there or higher.
    ///
    /// This is the single writer, and it is monotone: **no decay** (§7.3). A rung
    /// reached is a fact about the run.
    pub(crate) fn raise(&mut self, trigger: AlertTrigger) -> Option<u32> {
        let rung = trigger.rung();
        (rung > self.rung).then(|| {
            self.rung = rung;
            rung
        })
    }

    /// Run the sliding sighting window for one world turn (§7.6): `certain` says
    /// whether **any** guard had the player in its certain zone this turn. Returns
    /// the trigger a freshly-counted sighting fires, if one was counted.
    ///
    /// The window prunes to the last [`SIGHTING_WINDOW_TURNS`] turns first, so a
    /// sighting is [`SIGHTING_CONTACT_TURNS`] contact turns *inside* it. Once counted
    /// it latches until the window falls back to zero, which is what makes three
    /// sightings three separate events rather than one chase counted every turn.
    pub(crate) fn watch(&mut self, turn: u32, certain: bool) -> Option<AlertTrigger> {
        if certain {
            self.contacts.push(turn);
        }
        // Keep only the turns still inside the window — `t + WINDOW > turn` is the
        // half-open "within the last WINDOW turns, this one included".
        self.contacts.retain(|&t| t + SIGHTING_WINDOW_TURNS > turn);
        if self.contacts.is_empty() {
            // The window fell back to 0: the next run of contact is a *new* sighting.
            self.counted = false;
        }
        if self.counted || (self.contacts.len() as u32) < SIGHTING_CONTACT_TURNS {
            return None;
        }
        self.counted = true;
        self.sightings += 1;
        Some(if self.sightings >= SIGHTINGS_FOR_SECOND_RUNG {
            AlertTrigger::RepeatSightings
        } else {
            AlertTrigger::Sighting
        })
    }

    /// Record that a **post** fell silent — a body missed its first radio ping
    /// (§7.3). Returns the trigger it fires: the first quiet post is rung 1, the
    /// [`SILENT_POSTS_FOR_THIRD_RUNG`]th is rung 3. Called once per body, on its
    /// first miss only: a post that has already gone quiet tells control nothing new.
    pub(crate) fn post_fell_silent(&mut self) -> AlertTrigger {
        self.silent_posts += 1;
        if self.silent_posts >= SILENT_POSTS_FOR_THIRD_RUNG {
            AlertTrigger::SecondPostSilent
        } else {
            AlertTrigger::MissedPing
        }
    }

    /// Record an intel console tampered with (§7.3). Returns the trigger **only at
    /// rung ≥ 1**: rung 0 is safe, and that is the whole incentive to stay
    /// undetected — a clean raid can empty the consoles and the facility never learns
    /// what you came for.
    pub(crate) fn console_tampered(&self) -> Option<AlertTrigger> {
        (self.rung >= 1).then_some(AlertTrigger::ConsoleTampered)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The §7.3/§7.6 thresholds are **[START]** values a later tune must move
    /// deliberately — pinned here so the edit is visible in the diff.
    #[test]
    fn the_ladder_constants_are_pinned() {
        assert_eq!(SIGHTING_CONTACT_TURNS, 3, "3 contact turns make a sighting");
        assert_eq!(SIGHTING_WINDOW_TURNS, 10, "inside a 10-turn window");
        assert_eq!(SIGHTINGS_FOR_SECOND_RUNG, 3, "3 sightings reach rung 2");
        assert_eq!(SILENT_POSTS_FOR_THIRD_RUNG, 2, "2 quiet posts reach rung 3");
        assert_eq!(ALERT_DWELL_TURNS_MIN, 1, "the [START] alerted dwell floor");
        assert_eq!(
            ALERT_DWELL_TURNS_MAX, 3,
            "the [START] alerted dwell ceiling"
        );
        assert_eq!(TOP_RUNG, 3, "three rungs, and no rung 4");
    }

    /// §7.3: the trigger table *is* the ladder — each trigger names its rung.
    #[test]
    fn every_trigger_names_its_rung() {
        assert_eq!(AlertTrigger::Sighting.rung(), 1);
        assert_eq!(AlertTrigger::MissedPing.rung(), 1);
        assert_eq!(AlertTrigger::RepeatSightings.rung(), 2);
        assert_eq!(AlertTrigger::ConsoleTampered.rung(), 2);
        assert_eq!(AlertTrigger::BodyFound.rung(), TOP_RUNG);
        assert_eq!(AlertTrigger::SecondPostSilent.rung(), TOP_RUNG);
    }

    /// §7.3 **no decay**, and the ladder never double-reports: a rung only ever
    /// rises, a trigger at or below the current rung reports nothing, and a jump
    /// straight to 3 does not walk through 1 and 2.
    #[test]
    fn the_rung_only_ever_rises() {
        let mut alert = Alert::new();
        assert_eq!(alert.rung(), 0, "a quiet facility starts at 0");
        assert_eq!(
            alert.raise(AlertTrigger::BodyFound),
            Some(3),
            "straight to 3"
        );
        assert_eq!(
            alert.raise(AlertTrigger::Sighting),
            None,
            "a rung-1 trigger at rung 3 escalates nothing",
        );
        assert_eq!(alert.raise(AlertTrigger::ConsoleTampered), None);
        assert_eq!(alert.raise(AlertTrigger::BodyFound), None, "already there");
        assert_eq!(alert.rung(), 3, "and it never comes back down");
    }

    /// §7.3: the effects are **cumulative** — rung 2 and rung 3 keep rung 1's
    /// shortened dwell, because the rule is "at least 1", not "exactly 1".
    #[test]
    fn the_dwell_cut_holds_at_every_rung_above_zero() {
        let mut alert = Alert::new();
        assert_eq!(
            alert.dwell_turns(),
            (
                crate::guard::GUARD_DWELL_TURNS_MIN,
                crate::guard::GUARD_DWELL_TURNS_MAX
            ),
            "a quiet facility dwells the full §7.5 window",
        );
        for trigger in [
            AlertTrigger::Sighting,
            AlertTrigger::ConsoleTampered,
            AlertTrigger::BodyFound,
        ] {
            alert.raise(trigger);
            assert_eq!(
                alert.dwell_turns(),
                (ALERT_DWELL_TURNS_MIN, ALERT_DWELL_TURNS_MAX),
                "rung {} keeps the cut",
                alert.rung(),
            );
        }
    }

    /// §7.6: [`SIGHTING_CONTACT_TURNS`] contact turns inside the window make a
    /// sighting; two do not. The window is the *sliding* one — contact on turns 0
    /// and 1 has aged out by the time a third lands ten turns later.
    #[test]
    fn three_contact_turns_in_the_window_make_a_sighting() {
        let mut alert = Alert::new();
        assert_eq!(alert.watch(0, true), None);
        assert_eq!(alert.watch(1, true), None, "two is not a sighting");
        for turn in 2..9 {
            assert_eq!(alert.watch(turn, false), None, "quiet turns count nothing");
        }
        // Turn 10: the contacts from turns 0 and 1 have left a 10-turn window
        // (0 + 10 > 10 is false), so this third contact stands alone.
        assert_eq!(
            alert.watch(10, true),
            None,
            "the window slid past the first two"
        );
        assert_eq!(alert.watch(11, true), None);
        assert_eq!(
            alert.watch(12, true),
            Some(AlertTrigger::Sighting),
            "three inside the window",
        );
        assert_eq!(alert.sightings(), 1);
    }

    /// §7.6: a **held** chase is one sighting, not one per turn. The window must fall
    /// back to 0 — [`SIGHTING_WINDOW_TURNS`] with no contact — before another can be
    /// counted, and the third sighting is what reaches rung 2.
    #[test]
    fn a_second_sighting_waits_for_the_window_to_empty() {
        let mut alert = Alert::new();
        let sighting = |alert: &mut Alert, from: u32| {
            let mut counted = None;
            for turn in from..from + SIGHTING_CONTACT_TURNS {
                counted = counted.or(alert.watch(turn, true));
            }
            counted
        };

        assert_eq!(sighting(&mut alert, 0), Some(AlertTrigger::Sighting));
        // A chase that holds the player in the certain zone for turns on end is still
        // the same sighting: the latch does not clear while the window has contact.
        for turn in 3..40 {
            assert_eq!(alert.watch(turn, true), None, "one chase, one sighting");
        }
        assert_eq!(alert.sightings(), 1);

        // Ten quiet turns empty the window; the next run of contact counts afresh.
        for turn in 40..40 + SIGHTING_WINDOW_TURNS {
            assert_eq!(alert.watch(turn, false), None);
        }
        assert_eq!(sighting(&mut alert, 50), Some(AlertTrigger::Sighting));
        assert_eq!(alert.sightings(), 2);

        for turn in 53..53 + SIGHTING_WINDOW_TURNS {
            assert_eq!(alert.watch(turn, false), None);
        }
        assert_eq!(
            sighting(&mut alert, 70),
            Some(AlertTrigger::RepeatSightings),
            "the third sighting is the rung-2 trigger",
        );
        assert_eq!(alert.sightings(), SIGHTINGS_FOR_SECOND_RUNG);
    }

    /// §7.3: the first quiet post is rung 1; the second — a *different* body — is
    /// rung 3. The trigger counts bodies, not pings.
    #[test]
    fn the_second_silent_post_is_the_rung_three_trigger() {
        let mut alert = Alert::new();
        assert_eq!(alert.post_fell_silent(), AlertTrigger::MissedPing);
        assert_eq!(alert.post_fell_silent(), AlertTrigger::SecondPostSilent);
        assert_eq!(
            alert.post_fell_silent(),
            AlertTrigger::SecondPostSilent,
            "a third quiet post is no louder — rung 3 is the top",
        );
    }

    /// §7.3: **rung 0 is safe.** A console tampered with by an unseen intruder
    /// triggers nothing at all; the same bump once the facility knows you are in it
    /// reaches rung 2.
    #[test]
    fn tampering_is_free_until_the_facility_knows_you_are_there() {
        let mut alert = Alert::new();
        assert_eq!(alert.console_tampered(), None, "rung 0 is safe");
        alert.raise(AlertTrigger::Sighting);
        assert_eq!(
            alert.console_tampered(),
            Some(AlertTrigger::ConsoleTampered),
            "the same bump at rung 1 is a rung-2 trigger",
        );
    }
}
