use super::*;
use crate::ability::AbilityId;
use crate::alert::AlertTrigger;
use crate::cell::{Cell, Direction};
use crate::facility::Terrain;
use crate::guard::Guard;
use crate::guard::GuardState;
use crate::modifiers::{IntelGate, LevelModifiers};
use crate::state::Input;
use crate::test_support::{leave_by_the_tunnel, open_room, room_with_tunnel};

/// A walled box with the player at `player`, one intel console at `intel`,
/// and the exit far away — enough state to generate real messages.
fn state(player: Cell, intel: Cell) -> State {
    State::new(
        open_room(12, 12),
        player,
        Direction::North,
        Vec::new(),
        [intel],
        Cell::new(10, 10),
    )
}

/// §11.7: the near line shows the **highest-priority** message of the last
/// action. Taking intel also moves nothing and bumps nothing, so build the
/// contest directly: a turn whose events include both routine narration and
/// objective feedback shows the objective.
#[test]
fn the_highest_priority_message_wins() {
    let mut s = state(Cell::new(5, 6), Cell::new(5, 5));
    s.step(Input::Step(Direction::North)); // bump the console: intel taken
    let line = near_line(&s);
    assert_eq!(line.text, "all the intel — the exit is open");
    assert_eq!(line.category, Category::Interest);
    assert_eq!(line.priority, 20);
}

/// §11.7: messages **clear on the player's next action** — to the ambient
/// floor, never to an empty row.
#[test]
fn a_message_clears_to_ambient_on_the_next_action() {
    let mut s = state(Cell::new(5, 6), Cell::new(3, 3));
    s.step(Input::Step(Direction::West)); // a plain move: narrates nothing
    assert_eq!(near_line(&s).priority, i32::MIN, "a move narrates nothing");
    assert_eq!(near_line(&s).text, "objectives: 0/1");

    let mut s = state(Cell::new(3, 4), Cell::new(3, 3));
    s.step(Input::Step(Direction::North)); // take the intel: a loud message
    assert_eq!(near_line(&s).priority, 20);
    s.step(Input::Step(Direction::South)); // next action: the message clears
    let line = near_line(&s);
    assert_eq!(line.text, "objectives: 1/1");
    assert_eq!(line.category, Category::Interest);
}

/// The ambient floor tracks concealment first (§10.3): hidden and crouched
/// read as Owned — the same vocabulary as the recoloured cupboard and table.
#[test]
fn ambient_reports_concealment_as_owned() {
    let mut layout = open_room(12, 12);
    layout.place(Cell::new(5, 5), Terrain::Hideout);
    layout.place(Cell::new(8, 7), Terrain::PartialCover);
    let mut s = State::new(
        layout,
        Cell::new(5, 6),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(10, 10),
    );

    s.step(Input::Step(Direction::North)); // climb into the cupboard
    s.step(Input::Wait); // a quiet turn inside: the entry message has cleared
    let line = near_line(&s);
    assert_eq!(line.text, "hidden — the cupboard conceals you");
    assert_eq!(line.category, Category::Owned);

    let mut s = State::new(
        open_room(12, 12),
        Cell::new(5, 6),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(10, 10),
    );
    // No cover adjacent, so waiting narrates nothing and the floor shows. This
    // facility has no consoles at all, so the fraction is honest about that too
    // rather than claiming intel the player never took (#310).
    s.step(Input::Wait);
    assert_eq!(near_line(&s).text, "objectives: 0/0");
}

/// A crouch engaging is a message (Owned, §10.3); holding the crouch on the
/// next wait repeats nothing and the ambient takes over.
#[test]
fn a_crouch_reports_once_then_reads_as_ambient() {
    let mut layout = open_room(12, 12);
    layout.place(Cell::new(6, 6), Terrain::PartialCover);
    let mut s = State::new(
        layout,
        Cell::new(5, 6),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(10, 10),
    );
    s.step(Input::Step(Direction::East)); // bump the table: the crouch engages
    let line = near_line(&s);
    assert_eq!(line.text, "you duck behind the table");
    assert_eq!(line.category, Category::Owned);

    s.step(Input::Wait); // holding: no new event, the ambient shows the state
    assert_eq!(near_line(&s).text, "crouched behind cover");
}

/// §8.3: while dragging on open ground, the ambient floor names the held
/// state and its cost — the standing explanation of every half-speed turn.
#[test]
fn ambient_reports_dragging_as_owned() {
    let mut layout = open_room(12, 12);
    layout.place(Cell::new(5, 5), Terrain::Hideout);
    let mut s = State::new(
        layout,
        Cell::new(5, 5),
        Direction::North,
        vec![Guard::stationary(Cell::new(5, 4))],
        Vec::new(),
        Cell::new(10, 10),
    );
    s.step(Input::Step(Direction::North)); // takedown
    s.step(Input::Step(Direction::North)); // climb out onto the body
    s.step(Input::Wait); // stand on the body: take hold (§8.3/#451) — the message turn
    assert_eq!(near_line(&s).text, "you take hold of the body");

    s.step(Input::Step(Direction::East)); // step off, hauling: the message clears
    let line = near_line(&s);
    assert_eq!(line.text, "dragging the body — half speed");
    assert_eq!(line.category, Category::Owned);
}

/// The §11.7 threat ladder's bottom rung: a fresh detection (§7.6) is a
/// Danger message at priority 2 — above self-narration, below a found body
/// (4) and the capture (10) — matching the hunting `g` it announces.
#[test]
fn a_fresh_detection_reads_as_the_lowest_threat_rung() {
    let msg = message_for(Event::Detected {
        by: Cell::new(5, 5),
    })
    .expect("a detection is never silent");
    assert_eq!(msg.text, "a guard has seen you");
    assert_eq!(msg.category, Category::Danger);
    assert_eq!(msg.priority, 2);
}

/// #421, the whole point of the row: **both standing facts, together.** At rung 0
/// the objective alone; once the ladder has stepped, the objective *and* the
/// condition — neither expires, and the player needs both.
#[test]
fn the_ambient_floor_carries_the_objective_and_the_security_together() {
    assert_eq!(objective_and_security(1, 3, 0), "objectives: 1/3");
    assert_eq!(
        objective_and_security(1, 3, 2),
        "objectives: 1/3 - security: 2",
    );
    // Every objective taken: the fraction *is* the exit-open signal under the gate
    // quick play ships on (§4.5/#244), so the case is not silently lost.
    assert_eq!(objective_and_security(3, 3, 0), "objectives: 3/3");
    assert_eq!(
        objective_and_security(3, 3, crate::alert::TOP_RUNG),
        "objectives: 3/3 - security: 3",
    );
    // A facility with no consoles at all still says something true.
    assert_eq!(objective_and_security(0, 0, 0), "objectives: 0/0");
}

/// #421: the band **is** the alert indicator. Interest while the facility has not
/// noticed you, and the §7.3 ladder's own colour once it has — read off
/// [`rung_category`](crate::alert::rung_category), the same declaration the help
/// panel's condition line reads, so the row and the panel can never disagree.
#[test]
fn the_ambient_band_follows_the_rung() {
    let mut s = state(Cell::new(5, 6), Cell::new(3, 3));
    s.step(Input::Wait);
    assert_eq!(s.alert(), 0);
    assert_eq!(near_line(&s).category, Category::Interest);

    // The mapping itself, over every rung the ladder can reach — the arm this row
    // takes is `rung_category` and nothing hand-written beside it.
    for rung in 1..=crate::alert::TOP_RUNG {
        assert_ne!(
            crate::alert::rung_category(rung),
            Category::Interest,
            "a raised facility never wears the quiet objective colour",
        );
    }
}

/// §11.4/#421: **both forms fit the row**, measured against the capacity the layout
/// computes rather than a number written down beside it — worst case, with the
/// deploy control up, which it can be while the ambient floor shows (the history
/// outlives the action that cleared the near line).
///
/// Walked over two-digit counts as well, since `objectives: 10/12 - security: 3` is
/// the longest the line can get and is the one that would clip first.
#[test]
fn both_ambient_forms_fit_the_near_line() {
    let max = crate::render::near_line_text_max(crate::LevelConfig::V1.width);
    for total in [0, 3, 9, 12, 99] {
        for taken in [0, total] {
            for alert in 0..=crate::alert::TOP_RUNG {
                let line = objective_and_security(taken, total, alert);
                let len = line.chars().count();
                assert!(
                    len <= max,
                    "{line:?} is {len} cells, over the {max} the near line leaves \
                         beside its controls (§11.4)",
                );
            }
        }
    }
}

/// **The line the run opens on** (§4.5/§11.4/#466): turn one has no action behind it and
/// so no message, and a run that begins inside its own tunnel has to say so. The floor
/// says it — a standing fact for the length of the crawl, not one frame of narration —
/// and it names *which* crawlspace, because one is a shortcut you found and the other is
/// the way home.
///
/// It fits the row, which is the constraint every near-line string is under (§11.7): the
/// message's capacity is what is left beside the row's controls.
#[test]
fn the_run_opens_saying_where_you_are() {
    let exit = Cell::new(5, 4);
    let tunnel = crate::test_support::exit_tunnel_cells(12, 12, exit, Direction::North);
    let mut s = State::new(
        crate::test_support::room_with_tunnel(12, 12, exit, Direction::North),
        *tunnel.last().expect("a way out"),
        Direction::North,
        Vec::new(),
        [Cell::new(8, 8)],
        exit,
    );
    let opening = near_line(&s);
    assert_eq!(opening.text, "your own tunnel — crawl out");
    assert_eq!(opening.category, Category::Owned, "a state you are in");
    assert!(opening.is_ambient(), "the floor, not a message");
    let max = crate::render::near_line_text_max(crate::LevelConfig::V1.width);
    for line in ["your own tunnel — crawl out", "in the duct — memory only"] {
        let len = line.chars().count();
        assert!(
            len <= max,
            "{line:?} is {len} cells, over the {max} the row leaves"
        );
    }

    // It holds for the whole crawl, and gives the row back on the way out.
    s.step(Input::Step(Direction::South));
    assert_eq!(near_line(&s).text, "your own tunnel — crawl out");
    crate::test_support::climb_out_of_the_tunnel(&mut s);
    assert_eq!(near_line(&s).text, "objectives: 0/1", "back to the floor");
}

/// §11.4/#421: the momentary states still **pre-empt** the standing pair, and every
/// live message still pre-empts all of them. The floor gained a second fact; it did
/// not gain a claim on the row.
#[test]
fn the_momentary_states_still_pre_empt_the_standing_pair() {
    let mut layout = open_room(12, 12);
    layout.place(Cell::new(5, 5), Terrain::Hideout);
    let mut s = State::new(
        layout,
        Cell::new(5, 6),
        Direction::North,
        Vec::new(),
        [Cell::new(8, 8)],
        Cell::new(10, 10),
    );
    s.step(Input::Step(Direction::North)); // into the cupboard
    s.step(Input::Wait); // the entry message clears: hidden is the floor's arm
    assert_eq!(near_line(&s).text, "hidden — the cupboard conceals you");
    assert!(
        !near_line(&s).text.contains("objectives"),
        "a momentary state owns the row while it lasts",
    );

    // And a live message outranks even that — the floor is the floor (§11.7).
    s.step(Input::Step(Direction::South)); // out of the cupboard
    assert!(!near_line(&s).is_ambient() || near_line(&s).text.contains("objectives"));
}

/// §7.3/§11.7: the radio events read as Warning-band threat messages — a
/// silence (a guard stopped answering) below a found body, an alert step (the
/// facility-wide escalation) above it, both below being caught.
#[test]
fn the_radio_events_read_on_the_threat_ladder() {
    let silence = message_for(Event::RadioSilence {
        at: Cell::new(3, 3),
    })
    .expect("a radio silence is never silent");
    assert_eq!(silence.text, "a guard has gone silent");
    assert_eq!(silence.category, Category::Warning);
    assert_eq!(silence.priority, 3);

    let alert = message_for(Event::AlertRaised {
        rung: 2,
        trigger: AlertTrigger::RepeatSightings,
    })
    .expect("an alert step speaks");
    assert_eq!(alert.text, "security condition 2 of 3");
    assert_eq!(alert.category, Category::Warning);
    assert_eq!(alert.priority, 5);
}

/// §7.7/§11.7: killing the radio net at the comms console is **objective
/// feedback**, not a threat message — the Interest band, on the same rung as
/// taking intel (20), well above every guard event. That rung is the point: the
/// bump raises exactly one question — *did that work?* — and a detection or a
/// found body landing the same turn must not bury the answer.
#[test]
fn killing_the_radio_net_reads_as_objective_feedback() {
    let msg = message_for(Event::CommsSilenced {
        at: Cell::new(3, 3),
    })
    .expect("silencing the net is never silent");
    assert_eq!(msg.text, "the radio net goes dead");
    assert_eq!(msg.category, Category::Interest);
    assert_eq!(msg.priority, 20);

    // Louder than the loudest thing the net itself could have said.
    let alert = message_for(Event::AlertRaised {
        rung: 3,
        trigger: AlertTrigger::BodyFound,
    })
    .expect("an alert step speaks");
    assert!(
        msg.priority > alert.priority,
        "the answer to \"did that work?\" outranks a guard event on the same turn",
    );
}

/// §7.7/§11.7: the two call-ins sit on the same Warning band, at the rungs
/// their content earns. A **sighting** call reports *your* position and reads
/// with the radio silence (3). A **body** call reports the *body's* — never the
/// player's, who may be nowhere near — and outranks the bare find (4), because
/// it says everything the find says and adds that guards are on their way. Both
/// stay under the facility-wide alert step (5→) and being caught.
#[test]
fn the_call_ins_read_on_the_threat_ladder() {
    let at = Cell::new(3, 3);

    let sighting = message_for(Event::CalledIn { at }).expect("a call-in speaks");
    assert_eq!(sighting.text, "your position was called in");
    assert_eq!(sighting.category, Category::Warning);
    assert_eq!(sighting.priority, 3);

    let found = message_for(Event::BodyFound { at }).expect("a find speaks");
    let body = message_for(Event::BodyCalledIn { at }).expect("a body call speaks");
    assert_eq!(body.text, "a body has been reported");
    assert_eq!(body.category, Category::Warning);
    assert!(
        body.priority > found.priority,
        "a reported body outranks a merely found one ({} vs {})",
        body.priority,
        found.priority,
    );
    assert!(
        body.priority
            <= message_for(Event::AlertRaised {
                rung: 1,
                trigger: AlertTrigger::Sighting,
            })
            .expect("an alert speaks")
            .priority,
        "…but never over the facility-wide alert",
    );
}

/// §11.8: **the near line never says "rung"** — the ladder's steps are *conditions*
/// on screen, because a rung names the shape of the system and the player is reading
/// this without the design doc beside them. Walked over every rung the ladder can
/// reach, so a later reword cannot let the mechanism's name back out; the help
/// panel's half of the same pair is pinned in `render::alert`.
#[test]
fn the_near_line_speaks_conditions_not_rungs() {
    for rung in 1..=crate::alert::TOP_RUNG {
        let line = alert_line(rung);
        assert!(
            !line.to_lowercase().contains("rung"),
            "{line:?} says rung — the design's word, not the player's (§11.8)",
        );
        assert_eq!(line, format!("security condition {rung} of 3"));
    }
    // The same trip-wire over the reason lines (#418): they are the other half of
    // the same fact, and a mechanism word smuggled in underneath the headline is no
    // better than one in it. `trigger` is the design's word too, and so is the
    // ladder's own name.
    for reason in AlertTrigger::ALL.into_iter().filter_map(alert_reason) {
        let reason = reason.to_lowercase();
        for word in ["rung", "trigger", "ladder", "alert level"] {
            assert!(
                !reason.contains(word),
                "{reason:?} says {word:?} — the design's word, not the player's (§11.8)",
            );
        }
    }
}

/// §7.3/§11.4: once the radio has stepped the facility alert, the value is
/// *readable* — with no louder message live, the ambient floor surfaces it beside
/// the objectives, never written-but-invisible (§2.3), and the band takes the rung's
/// own colour (#421), which is what makes this row the standing alert indicator.
#[test]
fn ambient_surfaces_the_facility_alert() {
    use crate::radio::RadioClock;
    let mut layout = open_room(12, 12);
    layout.place(Cell::new(5, 5), Terrain::Hideout); // conceal the takedown
    let mut s = State::new(
        layout,
        Cell::new(5, 5),
        Direction::North,
        // A lone victim on a 2-turn clock: no responder needed for the alert
        // to step (both missed pings still land).
        vec![Guard::stationary(Cell::new(5, 4)).with_radio_clock(RadioClock::from_period(2))],
        Vec::new(),
        Cell::new(10, 10),
    );
    s.step(Input::Step(Direction::North)); // take the victim down (concealed)
    s.step(Input::Step(Direction::South)); // step out of the cupboard: no longer hidden
    for _ in 0..6 {
        s.step(Input::Wait); // wait out both pings; the last waits are quiet
    }
    assert!(s.alert() >= 1, "the second missed ping stepped the alert");
    assert!(
        !s.hidden(),
        "out of the cupboard, so the alert is the ambient fact"
    );

    // No message is live after the quiet waits: the near line rests on both standing
    // facts, and the band is the rung's own colour — the same mapping the help
    // panel's condition line reads (#375), so the two can never disagree.
    let line = near_line(&s);
    assert_eq!(line.text, "objectives: 0/0 - security: 1");
    assert_eq!(line.category, crate::alert::rung_category(s.alert()));
    assert_eq!(line.category, Category::Caution, "rung 1 is the low rung");
    assert_eq!(
        line.priority,
        i32::MIN,
        "it is the ambient floor, not a message"
    );
}

/// §8.3/#329: the safety eject is a Warning-band message ranked above every
/// guard event short of the capture — the player has been moved somewhere they
/// did not choose and cannot act, and neither fact may be buried by a detection
/// landing the same turn. It names the **tech**, never the terrain: the same
/// eject fires out of a table or a shut door, so "the wall" would be a lie in
/// most of the cases it covers.
#[test]
fn the_eject_outranks_every_guard_event_but_the_capture() {
    let msg = message_for(Event::Ejected {
        from: Cell::new(3, 4),
        to: Cell::new(3, 3),
        stunned: crate::phase_eject_stun(1),
    })
    .expect("the safety eject is never silent");
    assert_eq!(msg.text, "safety eject — stunned");
    assert_eq!(msg.category, Category::Warning);

    let alert = message_for(Event::AlertRaised {
        rung: 3,
        trigger: AlertTrigger::BodyFound,
    })
    .expect("an alert speaks");
    let caught = message_for(Event::Captured {
        guard: 0,
        state: GuardState::Chasing,
        at: Cell::new(3, 3),
    })
    .expect("the capture speaks");
    assert!(
        msg.priority > alert.priority,
        "being thrown clear outranks the facility alert ({} vs {})",
        msg.priority,
        alert.priority,
    );
    assert!(
        msg.priority < caught.priority,
        "…and never outranks the run ending",
    );
}

/// §8.3/§11.4: the stun is a *standing state*, so it lives on the ambient floor
/// and counts down there — derived straight off the counter, above every other
/// ambient fact because it is the one that removes the decision entirely.
#[test]
fn ambient_counts_the_stun_down() {
    let mut layout = open_room(12, 12);
    layout.place(Cell::new(5, 4), Terrain::Wall);
    let mut s = State::new(
        layout,
        Cell::new(4, 4),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(10, 10),
    )
    .with_loadout(crate::Loadout::innate().with(crate::AbilityId::Dephase));
    s.step(Input::Activate(crate::AbilityId::Dephase)); // window turn 1
    s.step(Input::Step(Direction::East)); // turn 2: into the wall
                                          // Stand in there for the rest of the window; the last of these is the turn the
                                          // duration ends and the player is thrown clear and stunned. Counted off the
                                          // catalog so a retune (#449) moves the scene rather than breaking it.
    let duration = crate::AbilityId::Dephase
        .def()
        .economy()
        .expect("Dephase is activated")
        .duration();
    for _ in 2..duration {
        s.step(Input::Wait);
    }

    // The eject's own turn speaks the message; the floor carries the rest.
    assert_eq!(near_line(&s).text, "safety eject — stunned");
    assert_eq!(s.stunned(), 2);

    s.step(Input::Wait); // swallowed, and nothing else is live
    let line = near_line(&s);
    assert_eq!(line.text, "stunned — 1 more turn");
    assert_eq!(line.category, Category::Warning);
    assert_eq!(line.priority, i32::MIN, "the floor, not a message");

    s.step(Input::Wait); // the last owed turn
    assert_eq!(s.stunned(), 0);
    assert_ne!(
        near_line(&s).text,
        "stunned — 0 more turns",
        "the floor stops reporting a stun that is paid off",
    );
}

/// Once the run ends the loop is inert (§4.5) and the final message stays —
/// `caught` on a capture, in Danger, at the top of the threat ladder.
#[test]
fn the_final_message_persists_after_the_run_ends() {
    // A guard sent straight down the column into the player.
    let s = {
        let mut s = State::new(
            open_room(12, 12),
            Cell::new(5, 5),
            Direction::North,
            vec![Guard::patrolling_to(Cell::new(5, 3), Cell::new(5, 10))],
            Vec::new(),
            Cell::new(10, 10),
        );
        s.step(Input::Wait); // guard steps to (5,4)
        s.step(Input::Wait); // guard steps into the player: captured
        s
    };
    let line = near_line(&s);
    assert_eq!(line.text, "caught");
    assert_eq!(line.category, Category::Danger);
    assert_eq!(line.priority, 10);

    // Inert: further input changes nothing, the message included.
    let mut s = s;
    s.step(Input::Wait);
    assert_eq!(near_line(&s).text, "caught");
}

/// §11.7: a single step can raise more than one message — a takedown seen by a
/// second guard is `TakenDown`, `BodyFound` *and* the `AlertRaised` the find sends
/// up the §7.3 ladder — and [`live_messages`] returns them all, **loudest first**,
/// leading with exactly what [`near_line`] speaks so the deployed list and the
/// band never disagree.
#[test]
fn live_messages_lists_the_whole_step_loudest_first() {
    // A hidden strike north on an adjacent victim, with a witness two cells up
    // whose cone covers the fresh body: the same turn yields the takedown
    // (self-narration, priority 0), the found body (a threat, priority 4) and the
    // facility-wide escalation it causes (priority 5).
    let mut layout = open_room(10, 10);
    layout.place(Cell::new(5, 5), Terrain::Hideout);
    let mut s = State::new(
        layout,
        Cell::new(5, 5),
        Direction::North,
        vec![
            Guard::stationary(Cell::new(5, 4)),
            Guard::stationary(Cell::new(5, 2)),
        ],
        Vec::new(),
        Cell::new(8, 8),
    );
    s.step(Input::Step(Direction::North));

    let live = live_messages(&s);
    assert_eq!(
        live.iter().map(|m| m.priority).collect::<Vec<_>>(),
        vec![5, 4, 0],
        "loudest first: the escalation, then the found body, then the narration",
    );
    assert_eq!(live[0].text, "security condition 3 of 3");
    assert_eq!(live[1].text, "a body has been found");
    assert_eq!(live[2].text, "the guard drops — a body is left");
    assert_eq!(
        live.first().cloned(),
        Some(near_line(&s)),
        "the list leads with exactly the near line's band",
    );
}

/// §7.3/§11.7/#418: a trigger that keeps a reason says it under its own headline,
/// at the same priority and in the same category, because the two are one fact.
/// Walked over [`AlertTrigger::ALL`], so a seventh trigger cannot ship without the
/// question being answered one way or the other.
#[test]
fn a_trigger_that_speaks_says_it_under_its_own_headline() {
    let mut spoken = Vec::new();
    for trigger in AlertTrigger::ALL {
        let rung = trigger.rung();
        let raised = loudest_first(&[Event::AlertRaised { rung, trigger }]);
        assert_eq!(raised[0].text, alert_line(rung), "{trigger:?}");

        let Some(reason) = alert_reason(trigger) else {
            assert_eq!(
                raised.len(),
                1,
                "{trigger:?} says nothing of its own, so the raise stands alone",
            );
            continue;
        };
        assert_eq!(raised.len(), 2, "{trigger:?}");
        assert_eq!(raised[1].text, reason, "{trigger:?}");
        assert!(!reason.is_empty(), "{trigger:?} has an empty reason line");
        assert_eq!(
            raised[1].category, raised[0].category,
            "{trigger:?}: one fact"
        );
        assert_eq!(
            raised[1].priority, raised[0].priority,
            "{trigger:?}: one fact"
        );
        spoken.push(reason);
    }
    // Every line that is kept is a *different* line: a reason shared by two triggers
    // would tell the player the wrong thing about one of them.
    let distinct: std::collections::BTreeSet<&str> = spoken.iter().copied().collect();
    assert_eq!(distinct.len(), spoken.len(), "{spoken:?}");
    assert!(!spoken.is_empty(), "the ladder explains itself somewhere");
}

/// #418, corrected: **a trigger stays silent only because something else speaks.**
///
/// Three of the six fire on the same turn as the very event that reports them, and
/// a reason line there was that message said twice, one row apart — `a guard found a
/// body` sitting directly above `a body has been found`. This pins the substitution
/// that justifies each silence: the stand-in event must exist, must raise a message
/// of its own, and must already say the thing the dropped line would have said.
///
/// So a future change that silenced one of those events (§11.7 is free to) would
/// fail here, rather than quietly leaving an escalation with no explanation
/// anywhere.
#[test]
fn a_silent_trigger_has_another_event_speaking_for_it() {
    let at = Cell::new(3, 3);
    // The event the loop pushes into the *same* vector as the raise, one statement
    // earlier — `find_bodies` and `miss_ping` respectively.
    let stands_in_for = |trigger| match trigger {
        AlertTrigger::BodyFound => Some(Event::BodyFound { at }),
        AlertTrigger::MissedPing | AlertTrigger::SecondPostSilent => {
            Some(Event::RadioSilence { at })
        }
        AlertTrigger::Sighting | AlertTrigger::RepeatSightings | AlertTrigger::ConsoleTampered => {
            None
        }
    };

    for trigger in AlertTrigger::ALL {
        match (alert_reason(trigger), stands_in_for(trigger)) {
            (None, Some(event)) => {
                let spoken = message_for(event)
                    .unwrap_or_else(|| panic!("{trigger:?}'s stand-in is silent"));
                // Both halves of a raise turn, in the order the near line lists
                // them: the escalation leads and the stand-in follows, so what the
                // dropped reason would have said is read one row later anyway.
                let turn = loudest_first(&[
                    event,
                    Event::AlertRaised {
                        rung: trigger.rung(),
                        trigger,
                    },
                ]);
                assert_eq!(turn.len(), 2, "{trigger:?}: no third row, no duplicate");
                assert_eq!(turn[0].text, alert_line(trigger.rung()));
                assert_eq!(turn[1].text, spoken.text, "{trigger:?}");
            }
            (Some(_), None) => {}
            (reason, event) => panic!(
                "{trigger:?} must either say why or have an event that does \
                     (reason {reason:?}, stand-in {event:?})",
            ),
        }
    }
}

/// The bug this correction is for (#418): the raise turn is **two rows, not three**.
/// A witnessed takedown is the exact case the screenshot caught — the body find, the
/// escalation it causes, and formerly a reason restating the find.
#[test]
fn a_found_body_does_not_report_itself_twice() {
    let at = Cell::new(4, 4);
    let texts: Vec<String> = loudest_first(&[
        Event::TakenDown { at },
        Event::BodyFound { at },
        Event::AlertRaised {
            rung: 3,
            trigger: AlertTrigger::BodyFound,
        },
    ])
    .into_iter()
    .map(|m| m.text)
    .collect();
    assert_eq!(
        texts,
        vec![
            "security condition 3 of 3".to_string(),
            "a body has been found".to_string(),
            "the guard drops — a body is left".to_string(),
        ],
    );
    assert!(
        !texts.iter().any(|t| t == "a guard found a body"),
        "the find is reported once: {texts:?}",
    );
}

/// #418's whole reason for the splice: **nothing gets between a fact and its
/// reason**. The turn an alert climbs is exactly a turn with other loud events, and
/// they sort around the pair — never through it — however loud they are.
#[test]
fn no_event_can_come_between_the_raise_and_its_reason() {
    let at = Cell::new(3, 3);
    // Everything the same turn could plausibly raise, on both sides of the raise's
    // own rung: the capture (10), the eject (6), a body call (5, the raise's equal),
    // a found body (4) and the takedown's self-narration (0).
    let events = [
        Event::TakenDown { at },
        Event::BodyFound { at },
        Event::AlertRaised {
            rung: 2,
            trigger: AlertTrigger::ConsoleTampered,
        },
        Event::BodyCalledIn { at },
        Event::Ejected {
            from: at,
            to: Cell::new(3, 4),
            stunned: crate::phase_eject_stun(1),
        },
        Event::Captured {
            guard: 0,
            state: GuardState::Chasing,
            at,
        },
    ];
    let texts: Vec<String> = loudest_first(&events).into_iter().map(|m| m.text).collect();
    let headline = texts
        .iter()
        .position(|t| t == "security condition 2 of 3")
        .expect("the raise speaks");
    assert_eq!(
        texts.get(headline + 1).map(String::as_str),
        Some("they know the intel was touched"),
        "the reason follows its headline immediately: {texts:?}",
    );

    // …and it holds for every rotation of the same turn's events, so the guarantee
    // is the splice and not the order the loop happened to report them in.
    for shift in 0..events.len() {
        let mut rotated = events;
        rotated.rotate_left(shift);
        let texts: Vec<String> = loudest_first(&rotated)
            .into_iter()
            .map(|m| m.text)
            .collect();
        let i = texts
            .iter()
            .position(|t| t == "security condition 2 of 3")
            .expect("the raise speaks");
        assert_eq!(
            texts.get(i + 1).map(String::as_str),
            Some("they know the intel was touched"),
            "rotation {shift}: {texts:?}",
        );
    }
}

/// §11.4/#418: the **near line is unchanged** by the reason. It is one grid row and
/// it speaks the loudest message only — the condition, in the words it always used.
#[test]
fn the_near_line_still_speaks_the_condition_alone() {
    let mut s = state(Cell::new(5, 6), Cell::new(3, 3));
    s.step(Input::Wait);
    let quiet = near_line(&s).text;

    for trigger in AlertTrigger::ALL {
        let rung = trigger.rung();
        let line = loudest_first(&[Event::AlertRaised { rung, trigger }])
            .into_iter()
            .next()
            .expect("the raise speaks");
        assert_eq!(line.text, alert_line(rung), "{trigger:?}");
        assert_eq!(line.category, Category::Warning);
        assert_eq!(line.priority, 5);
    }
    assert_eq!(near_line(&s).text, quiet, "and a quiet turn is untouched");
}

/// #418: the pair survives into [`MessageHistory`] and reads there in the order it
/// read live — the record and the near line cannot tell different stories about the
/// same turn.
#[test]
fn the_reason_follows_its_headline_into_the_history() {
    let at = Cell::new(4, 4);
    let events = [
        Event::Detected { by: at },
        Event::AlertRaised {
            rung: 1,
            trigger: AlertTrigger::Sighting,
        },
    ];
    let mut history = MessageHistory::default();
    history.record(&events);

    let filed: Vec<String> = history
        .blocks()
        .next()
        .expect("a loud action is filed")
        .iter()
        .map(|m| m.text.clone())
        .collect();
    let live: Vec<String> = loudest_first(&events).into_iter().map(|m| m.text).collect();
    assert_eq!(filed, live, "remembered exactly as it read live");
    assert_eq!(
        filed,
        vec![
            "security condition 1 of 3".to_string(),
            "you were seen".to_string(),
            "a guard has seen you".to_string(),
        ],
    );
}

/// A facility with three consoles ringing the player and the exit one step
/// south, gated by `gate` (§4.5/#244). Bumping north, east and west takes the
/// three intel in turn — a bump moves nobody — and [`answer_the_exit`] crawls the
/// tunnel south of the exit to answer it (§4.5/#466).
fn gated(gate: IntelGate) -> State {
    State::new(
        room_with_tunnel(12, 12, Cell::new(5, 6), Direction::South),
        Cell::new(5, 5),
        Direction::North,
        Vec::new(),
        [Cell::new(5, 4), Cell::new(6, 5), Cell::new(4, 5)],
        Cell::new(5, 6),
    )
    .with_modifiers(LevelModifiers {
        intel_to_exit: gate,
        ..LevelModifiers::default()
    })
}

/// The directions that take the three consoles of [`gated`], in order.
const TAKES: [Direction; 3] = [Direction::North, Direction::East, Direction::West];

/// Answer the exit from the [`gated`] fixture (§4.5/#466): with the gate met, climb into
/// the tunnel south of the player, crawl to the border and step off the board; short of
/// it, the mouth refuses on the spot. Returns the events of that last press — the win, or
/// the refusal, wherever it was answered.
fn answer_the_exit(s: &mut State) -> Vec<Event> {
    leave_by_the_tunnel(s)
}

/// Every phrase the near line uses about the intel gate, checked against the
/// state that produced it: a claim that the exit is open must have
/// [`State::exit_ready`] behind it, a claim that intel is still owed must not, and
/// nothing claims intel the player is not holding. This is the #310 invariant — the
/// objective band derives from the gate, never from a fixed intel count — so a
/// future gate or rewording that re-breaks it fails here rather than on screen.
fn assert_objective_messages_are_honest(s: &State) {
    let mut lines: Vec<String> = live_messages(s).into_iter().map(|m| m.text).collect();
    lines.push(near_line(s).text);
    lines.push(ambient(s).text);
    for text in lines {
        if text.contains("the exit is open") || text.contains("reach the exit") {
            assert!(
                s.exit_ready(),
                "{text:?} promises an exit that would refuse",
            );
        }
        if text.contains("more intel") || text.contains("more to go") {
            assert!(
                !s.exit_ready(),
                "{text:?} asks for intel the gate no longer wants",
            );
        }
        if text.contains("intel in hand") {
            assert!(
                s.intel_in_hand() > 0,
                "{text:?} claims intel that is not in hand",
            );
        }
    }
}

/// #310, the invariant the bug violated: across **all three gates** and every
/// amount of intel in hand, no message ever claims the exit is open unless it is —
/// take messages, refusals and the ambient floor alike.
#[test]
fn no_message_promises_an_exit_that_would_refuse() {
    for gate in [IntelGate::None, IntelGate::AtLeastOne, IntelGate::All] {
        for takes in 0..=TAKES.len() {
            let mut s = gated(gate);
            assert_objective_messages_are_honest(&s); // before touching anything
            for dir in TAKES.iter().take(takes) {
                s.step(Input::Step(*dir));
                assert_objective_messages_are_honest(&s);
            }
            // Answer the exit: a win if the gate is met, a refusal if it is not —
            // and either way the line that says so has to be true.
            let ready = s.exit_ready();
            let events = answer_the_exit(&mut s);
            assert_eq!(
                events.iter().any(|e| matches!(e, Event::Won)),
                ready,
                "{gate:?} with {takes} taken: the exit answered against its gate",
            );
            assert_objective_messages_are_honest(&s);
        }
    }
}

/// #310's headline case: under [`IntelGate::All`] — quick play, every real player's
/// first run — taking the first of three intel is **progress**, not a green light.
/// The old message announced an open exit that then refused the walk across the
/// facility; now the take says what is still owed, and only the last one opens it.
#[test]
fn the_all_intel_gate_does_not_announce_the_exit_on_the_first_take() {
    let mut s = gated(IntelGate::All);

    s.step(Input::Step(Direction::North));
    assert_eq!(near_line(&s).text, "intel in hand — 2 more to go");
    s.step(Input::Step(Direction::East));
    assert_eq!(near_line(&s).text, "intel in hand — 1 more to go");

    s.step(Input::Step(Direction::West));
    assert_eq!(
        near_line(&s).text,
        "all the intel — the exit is open",
        "the last take is the one that opens the exit",
    );
    assert!(s.exit_ready());
}

/// Under [`IntelGate::AtLeastOne`] the **take message** is unchanged — one intel does
/// open the exit, and the two still out are optional extra. This is the gate the
/// ambient fraction would misread (#310/#421): it says `objectives: 1/3` while the
/// exit is already open, because the fraction reports progress and not the gate.
///
/// That is why the line is only truthful under the gates a **human** actually plays:
/// `AtLeastOne` is the sim's baseline (§13.3) and the sim never reads the near line.
/// Pinned here so the day a human-facing mode ships on this gate, the mismatch is
/// already written down rather than discovered on screen.
#[test]
fn the_at_least_one_gate_is_the_one_the_ambient_fraction_cannot_speak_for() {
    let mut s = gated(IntelGate::AtLeastOne);
    s.step(Input::Wait); // a quiet turn: the near line rests on the floor
    assert_eq!(near_line(&s).text, "objectives: 0/3");
    assert_eq!(
        s.objectives_remaining(),
        3,
        "…with three consoles still out"
    );

    s.step(Input::Step(Direction::North));
    assert_eq!(
        near_line(&s).text,
        "intel in hand — the exit is open (2 more out)",
        "the take still says the gate's own answer",
    );
    s.step(Input::Wait); // the message clears back to the floor
    assert!(s.exit_ready(), "one intel opened the exit under this gate");
    assert_eq!(
        near_line(&s).text,
        "objectives: 1/3",
        "…and the floor reports progress, which under this gate is not the gate",
    );
}

/// Under [`IntelGate::None`] (§14 v3's campaign, where intel is currency, §2.2) the
/// exit never refuses, so no message ever asks for intel — and with nothing in hand
/// the floor says the exit is open without claiming intel the player has not taken.
#[test]
fn the_none_gate_never_asks_for_intel() {
    let mut s = gated(IntelGate::None);
    s.step(Input::Wait);
    // Intel is currency here (§2.2), never an exit key, so a progress fraction is
    // exactly the right thing for the floor to show (#421).
    assert_eq!(near_line(&s).text, "objectives: 0/3");

    let events = answer_the_exit(&mut s);
    assert!(
        events.contains(&Event::Won),
        "the exit accepts empty hands under `None`: {events:?}",
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, Event::ExitRefused { .. })),
        "`None` never refuses",
    );
}

/// §4.5/#310: a refusal names the **real** requirement — how many more consoles the
/// gate wants — so it can never contradict a take message from the same run. The
/// bug's compounding half: told the exit was open, the player crossed the facility
/// and was told it "needs intel in hand first" while holding intel.
#[test]
fn a_refusal_names_what_the_gate_still_wants() {
    let mut s = gated(IntelGate::All);
    let refused = answer_the_exit(&mut s);
    assert_eq!(refused, vec![Event::ExitRefused { still_needed: 3 }]);
    assert_eq!(near_line(&s).text, "the exit needs 3 more intel");

    // A fresh run, one intel in hand and two still owed: the same refusal counts down.
    let mut s = gated(IntelGate::All);
    s.step(Input::Step(Direction::North));
    let take = near_line(&s).text;
    let refused = answer_the_exit(&mut s);
    assert_eq!(refused, vec![Event::ExitRefused { still_needed: 2 }]);
    assert_eq!(near_line(&s).text, "the exit needs 2 more intel");
    assert!(
        !take.contains("the exit is open"),
        "the take and the refusal cannot disagree about the same gate: {take:?}",
    );

    // The baseline gate asks for the one console it needs, not the three that are
    // out — the tally is not the requirement.
    let mut s = gated(IntelGate::AtLeastOne);
    assert_eq!(
        answer_the_exit(&mut s),
        vec![Event::ExitRefused { still_needed: 1 }],
    );
    assert_eq!(near_line(&s).text, "the exit needs 1 more intel");
}

/// §8.2/§4.4-adjacent bookkeeping this ticket must not disturb: a refusal is free
/// and changes nothing, so a player who tries the way out early loses no turn to the
/// corrected message. The crawl that got them there cost turns, of course — that is
/// movement (§4.4); what must cost nothing is the refused step off the board itself.
#[test]
fn a_refusal_still_costs_nothing() {
    let mut s = gated(IntelGate::All);
    let (turn, at) = (s.turn(), s.player());
    // Empty-handed the mouth itself refuses (§4.5/#466) — the crawl is never begun, so
    // this is the whole of what the press costs.
    let refused = answer_the_exit(&mut s);
    assert_eq!(refused, vec![Event::ExitRefused { still_needed: 3 }]);
    assert_eq!(s.turn(), turn, "a refused exit is free (§4.5)");
    assert_eq!(s.player(), at, "and moves nobody");
    assert!(!s.in_duct(), "and never got as far as the tunnel");
}

/// A **budgeted** ability's activation is **silent** (§8.2/#302). The count it
/// could narrate is already on the bar, live and permanent, so a message would be
/// a duplicate paid for with the near line's one row — and a budgeted ability is
/// typically instant, so there is no "active" window to announce either. An
/// unbudgeted ability keeps the wording it has always had, which is every other
/// ability in the catalog.
#[test]
fn a_budgeted_activation_says_nothing_and_leaves_the_others_alone() {
    let spoken = |uses_left| {
        message_for(Event::AbilityActivated {
            ability: AbilityId::Dephase,
            uses_left,
        })
    };
    for left in [0, 1, 2, 9] {
        assert_eq!(spoken(Some(left)), None, "the bar already says {left}");
    }
    let unbudgeted = spoken(None).expect("an ordinary activation still speaks");
    assert_eq!(unbudgeted.text, "Phase Out active");
    assert_eq!(unbudgeted.category, Category::Owned, "your own tool");
    assert_eq!(unbudgeted.priority, 0, "quiet self-narration, like a bump");
}

/// A quiet action raises no message: [`live_messages`] is empty and the near
/// line rests on the ambient floor (§11.4), which is never a list entry.
#[test]
fn live_messages_is_empty_when_the_action_is_quiet() {
    let mut s = state(Cell::new(5, 6), Cell::new(3, 3));
    s.step(Input::Step(Direction::West)); // a plain move narrates nothing
    assert!(live_messages(&s).is_empty());
    assert_eq!(near_line(&s).priority, i32::MIN, "the ambient floor");
}

/// §7.6/§11.7/#224: the search's two lines sit on their own quiet rung — **1** —
/// above routine self-narration and below the threat ladder's bottom rung.
///
/// That placement is the whole of their manners. A search opening is the
/// *consequence* of something louder that has usually already spoken (a lost
/// sighting, a found body, a call-in), so it must never take the one row a fresh
/// detection wants; and the pair still has to clear the crouches and door bumps that
/// would otherwise bury it.
///
/// The **bands differ**, and the difference is the relief: a search opening is a
/// hunting threat (Warning), and a search called off wears the band that means
/// *nothing to report* (Neutral) — the row goes quiet rather than swapping one threat
/// colour for another. Caution is the trap here and is asserted against: the ambient
/// floor already paints the §7.3 ladder, so a rung-1 run's standing band is Caution
/// gold and this message would read as that row brightening rather than as news.
#[test]
fn the_search_boundary_reads_below_the_threat_ladder() {
    let began = message_for(Event::SearchBegan).expect("a search opening speaks");
    assert_eq!(began.text, "a guard starts searching");
    assert_eq!(began.category, Category::Warning);
    assert_eq!(began.priority, 1);

    let ended = message_for(Event::SearchEnded).expect("a search ending speaks");
    assert_eq!(ended.text, "the search is called off");
    assert_eq!(ended.category, Category::Neutral);
    assert_ne!(
        ended.category,
        crate::alert::rung_category(1),
        "the calling-off must not wear the band a rung-1 facility already stands in",
    );
    assert_eq!(ended.priority, 1);

    // Under the bottom rung of the threat ladder: a guard that has *found* you
    // outranks the news that somebody is looking.
    let detected = message_for(Event::Detected {
        by: Cell::new(3, 3),
    })
    .expect("a fresh detection speaks");
    assert!(
        began.priority < detected.priority && ended.priority < detected.priority,
        "the search boundary must not bury a detection ({} / {} vs {})",
        began.priority,
        ended.priority,
        detected.priority,
    );
    // …and over routine self-narration, which it would otherwise be buried by.
    let bump = message_for(Event::Bumped {
        into: Cell::new(1, 1),
    })
    .expect("a bump speaks");
    assert!(
        began.priority > bump.priority,
        "a search opening outranks a bump",
    );

    // The ordering holds through the near line itself, not just in the numbers: a
    // turn carrying both speaks the detection.
    assert_eq!(
        loudest_first(&[
            Event::SearchBegan,
            Event::Detected {
                by: Cell::new(3, 3)
            }
        ])[0]
            .text,
        "a guard has seen you",
    );
}

/// §11.8: both lines name the **world**, never the mechanism. The player is told a
/// guard is searching and that the search was called off — not that an `Alerted`
/// mood was entered, nor that a twelve-turn timer is running.
#[test]
fn the_search_lines_name_the_world() {
    for event in [Event::SearchBegan, Event::SearchEnded] {
        let text = message_for(event).expect("both lines speak").text;
        for word in [
            "alerted",
            "search radius",
            "timer",
            "turns",
            "focus",
            "state",
        ] {
            assert!(
                !text.contains(word),
                "{text:?} says {word:?} — the design's word, not the player's (§11.8)",
            );
        }
    }
}

/// **Every crate line fits the row, for every ability in the catalogue** (§11.4/§11.7,
/// #266) — the bound the overflow that produced it went looking for.
///
/// `another Autodoors — you have one` was 32 cells against a 32-cell budget and clipped
/// on a real screen the moment the deploy control was up; `Pierce Wall salvaged — it is
/// yours` was worse. The fix is the **bar name** throughout the family, and this is what
/// keeps it fixed: the check runs over `AbilityId::ALL`, so a longer name or a reworded
/// line fails here rather than in a screenshot.
///
/// Measured at the **worst case** — the narrow budget, with the deploy control up, which
/// it can be for any of these (the history outlives the action that cleared the row).
#[test]
fn the_crate_lines_fit_the_near_line() {
    let max = crate::render::near_line_text_max(crate::LevelConfig::V1.width);
    for id in AbilityId::ALL {
        let lines = [
            Event::TechSalvaged { id },
            Event::SalvageRefused { id },
            Event::UsesRecharged { id, uses: 3 },
            Event::ExchangeOffered { id },
            Event::ExchangeDeclined { id },
            // The trade names both halves, so it is measured with the same ability on
            // both sides — the longest that pairing can be.
            Event::Traded {
                taken: id,
                dropped: id,
            },
        ];
        for event in lines {
            let text = message_for(event).expect("every crate event speaks").text;
            let len = text.chars().count();
            assert!(
                len <= max,
                "{text:?} is {len} cells, over the {max} the near line leaves beside \
                 its controls (§11.4)",
            );
        }
    }
}
