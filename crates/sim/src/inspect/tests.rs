use super::*;
use intrusion_core::{parse_replay_link, parse_script, Direction};

/// The link this mode was built to read, kept verbatim — the run a player copied
/// out of a preview build and pasted back (#411). Parsing it is the core's job
/// (pinned there); what is pinned *here* is that inspecting it reproduces the run
/// the player actually had.
const PASTED: &str = "https://6dcafcf6-cb7b-4f80-9d6f-db85c4366efa.frame.claudeusercontent.com\
     /_f/1785420630-6a43/?__frame_t=uUXWTnWCKDUNRtSJccvSP10v.3dcc06db-1137-4123\
     -b2ec-7027f73c03ca.fd285f8d-60c8-459b-8dcc-abc57cc530f5.1785424456\
     &__frame_v=manifest.ec369d2e020e53f6.json\
     #seed=hwqcwzlhzanrdsdfzd&inputs=NNNNNEEEEEESS";

/// **The whole point, end to end**: a pasted link becomes the run it names.
///
/// The player walked five north and six east to the exit, then pressed south into
/// it — and the exit refused them, because that level asks for all three intel
/// (§4.5). Every claim here is one a reader of the report needs to be able to
/// trust: the trajectory, the free action that spent no move, and the refusal
/// carrying *how much* was still wanted.
#[test]
fn the_pasted_link_reproduces_the_run_that_was_played() {
    let (level, inputs) = parse_replay_link(PASTED).expect("a real link");
    let seen = inspect(&level, &inputs).expect("the v1 footprint carves");

    assert_eq!(seen.start, Cell { x: 31, y: 12 }, "where they opened");
    assert_eq!(seen.turns.len(), 13, "one record per pasted input");

    // Five north, then six east: every one of them a real move.
    for turn in &seen.turns[..11] {
        assert!(!turn.stayed_put(), "input {} moved", turn.index);
    }
    assert_eq!(
        seen.turns[4].to,
        Cell { x: 31, y: 7 },
        "after the five north"
    );
    assert_eq!(
        seen.turns[10].to,
        Cell { x: 37, y: 7 },
        "after the six east"
    );

    // The twelfth step puts them on the exit's doorstep; the thirteenth is the
    // refusal — the player did not move, and the game said how much was missing.
    let last = seen.turns.last().expect("thirteen records");
    assert_eq!(last.input, Input::Step(Direction::South));
    assert!(last.stayed_put(), "the exit refused, so they stayed");
    assert!(
        last.events
            .iter()
            .any(|e| matches!(e, Event::ExitRefused { still_needed: 3 })),
        "the refusal names what is still needed: {:?}",
        last.events,
    );
    assert_eq!(seen.outcome(), Outcome::Playing, "the run was still live");
}

/// The report says the things a reader came for, in the game's own words: the
/// level, the rule that was bending the run, the refusal, and how it ended.
#[test]
fn the_report_narrates_the_run_in_the_games_own_words() {
    let (level, inputs) = parse_replay_link(PASTED).expect("a real link");
    let report = inspect(&level, &inputs)
        .expect("the v1 footprint carves")
        .report();

    assert!(report.contains("hwqcwzlhzanrdsdfzd"), "the level's token");
    assert!(report.contains("seed 18900"));
    assert!(report.contains("Intel to exit"), "the rule in force");
    assert!(report.contains("stayed"), "the input that did not move");
    // The near line's own words for the refusal, not a second vocabulary.
    let refusal = intrusion_core::message_for(Event::ExitRefused { still_needed: 3 })
        .expect("the exit refusal speaks");
    assert!(
        report.contains(&refusal.text),
        "the report speaks the game's words: {report}",
    );
    assert!(report.contains("still playing"), "how it ended");
}

/// **It stops when the inputs stop** — the difference from `--script`, which pads
/// with waits and plays on to a capture or the cap. A short stream is a short run:
/// the turn count is the stream's, and nothing was invented after it.
#[test]
fn a_short_stream_is_a_short_run_not_a_padded_one() {
    let level = LevelSeed::sim(7);
    let seen = inspect(&level, &parse_script("...").expect("a legal script"))
        .expect("the v1 footprint carves");
    assert_eq!(seen.turns.len(), 3);
    assert_eq!(seen.state.turn(), 3, "three waits, three turns, no padding");
    assert_eq!(seen.outcome(), Outcome::Playing);
}

/// A **level link with no inputs** inspects to the opening facility: zero turns,
/// the player on their spawn, and a frame to look at. Being handed one is ordinary
/// — it is what the seed copy button (#353) produces — so it must read rather than
/// error.
#[test]
fn a_level_link_with_no_inputs_shows_the_opening_frame() {
    let level = LevelSeed::quick_play(8371);
    let token = level.encode().expect("a token");
    let (parsed, inputs) =
        parse_replay_link(&format!("#seed={token}")).expect("a level link parses");
    let seen = inspect(&parsed, &inputs).expect("the v1 footprint carves");

    assert!(seen.turns.is_empty(), "nothing was played");
    assert_eq!(seen.state.player(), seen.start);
    assert!(seen.report().contains("0 input(s) to replay"));
}

/// Inspection is **deterministic and faithful** (§12.4): the same link inspected
/// twice lands on the same run, and the run it lands on is the one the ordinary
/// boot produces — the property the whole feature rests on, since a report of a run
/// that never happened is worse than no report.
#[test]
fn inspecting_is_deterministic_and_matches_a_plain_boot() {
    let level = LevelSeed::quick_play(4242);
    let inputs = parse_script("NNEE.+rW-rSS").expect("a legal script");

    let a = inspect(&level, &inputs).expect("carves");
    let b = inspect(&level, &inputs).expect("carves");
    assert_eq!(a.report(), b.report(), "same link, same answer");

    // The same inputs fed to a plain `start_level` boot reach the same world.
    let mut plain = start_level(&level).expect("carves");
    for &input in &inputs {
        plain.step(input);
    }
    assert_eq!(a.state.player(), plain.player());
    assert_eq!(a.state.turn(), plain.turn());
    assert_eq!(
        intrusion_core::render(&a.state).to_text(),
        intrusion_core::render(&plain).to_text(),
        "the inspected frame is the played frame",
    );
}

/// An event the near line deliberately stays quiet about — a door a *guard* opened
/// across the facility (§11.7) — still reaches the report. An inspector must not
/// inherit a filter built to withhold from the player, or it would answer "nothing
/// happened" about turns where something did.
#[test]
fn an_event_the_near_line_withholds_is_still_reported() {
    let quiet = Event::DoorOpened {
        at: Cell { x: 29, y: 22 },
        by_player: false,
    };
    assert_eq!(
        intrusion_core::message_for(quiet),
        None,
        "the near line stays silent about it (§11.7) — the premise of this test",
    );
    let described = describe(quiet).expect("but the inspector still says it");
    assert!(
        described.contains("(29,22)"),
        "and it carries the cell, tidied: {described}",
    );

    // A move is the one thing not narrated — the columns already say it.
    assert_eq!(
        describe(Event::Moved {
            to: Cell { x: 1, y: 1 }
        }),
        None,
    );
}

/// The cell tidy-up is uniform and safe: it rewrites every cell in a string, leaves
/// everything else alone, and — the property that matters, since it runs over debug
/// output no one has read — never mangles a string it does not recognise.
#[test]
fn the_cell_tidy_up_rewrites_cells_and_nothing_else() {
    assert_eq!(
        tidy_cells("DoorOpened { at: Cell { x: 29, y: 22 }, by_player: false }"),
        "DoorOpened { at: (29,22), by_player: false }",
    );
    // Several in one string, and coordinates of different widths.
    assert_eq!(
        tidy_cells("A { p: Cell { x: 1, y: 2 }, q: Cell { x: 30, y: 400 } }"),
        "A { p: (1,2), q: (30,400) }",
    );
    // Nothing to do, and shapes that only look like a cell: left untouched rather
    // than half-rewritten.
    assert_eq!(tidy_cells("Won"), "Won");
    assert_eq!(tidy_cells(""), "");
    assert_eq!(tidy_cells("Cell { x: 1 }"), "Cell { x: 1 }");
    assert_eq!(tidy_cells("Cell { x: 1, y: 2"), "Cell { x: 1, y: 2");
}
