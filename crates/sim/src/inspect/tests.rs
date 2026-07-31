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
/// The player walked five north up a corridor, turned east into its head, was
/// carried one more cell by the #57 **auto-slide**, then spent four inputs pressed
/// against the wall before turning south and ducking behind a table. Every claim
/// here is one a reader of the report needs to be able to trust: the trajectory, the
/// slide that moved them somewhere their key did not name, and the inputs that spent
/// a turn without moving — the free actions (§4.4) a bare position list would hide.
///
/// **The run this link names changes with every generation change** — #387, and now
/// #452 — and that is worth understanding rather than papering over: a level-seed
/// token encodes the *seed*, and the level is a function of the seed **and the
/// generator**. Any generation change therefore re-carves every previously shared
/// token; the link still parses and still names a v1 level, but not the one the
/// player walked. The fixture is refreshed with the generator, exactly as the
/// committed sim baseline is.
///
/// The assertions are rewritten with it rather than loosened, which is the point: the
/// facts this fixture exists to pin are *kinds* of turn — a real move, a spent turn
/// that changed posture instead of position, a blocked press, and the #57 auto-slide
/// that carries the player perpendicular to the key. Every one of them survives the
/// re-carve on this link; only which input is which has moved.
#[test]
fn the_pasted_link_reproduces_the_run_that_was_played() {
    let (level, inputs) = parse_replay_link(PASTED).expect("a real link");
    let seen = inspect(&level, &inputs).expect("the v1 footprint carves");

    assert_eq!(seen.start, Cell { x: 36, y: 12 }, "where they opened");
    assert_eq!(seen.turns.len(), 13, "one record per pasted input");

    // Four north up the corridor, every one of them a real move.
    for turn in &seen.turns[..4] {
        assert!(!turn.stayed_put(), "input {} moved", turn.index);
    }
    assert_eq!(
        seen.turns[3].to,
        Cell { x: 36, y: 8 },
        "after the four north"
    );

    // The fifth is pressed north too and the player does **not** move: the cell
    // ahead is a table, so the press ducks behind it instead (§10.3). Worth pinning
    // for the same reason the auto-slide was: it is a spent turn the report has to
    // show as a change of *posture*, not of position, or a reader cannot trust it.
    assert!(
        seen.turns[4].stayed_put(),
        "the crouch is a pose, not a step"
    );
    assert!(
        seen.turns[4]
            .events
            .iter()
            .any(|e| matches!(e, Event::Crouched { .. })),
        "input 5 ducked behind the table ahead: {:?}",
        seen.turns[4].events,
    );

    // The sixth turns east and moves; the seventh is pressed east and ducks again,
    // this time behind the bench on that side.
    assert_eq!(seen.turns[5].to, Cell { x: 37, y: 8 }, "east off the table");
    assert_eq!(seen.turns[6].input, Input::Step(Direction::East));
    assert!(seen.turns[6].stayed_put(), "another duck, not a step");

    // Then the #57 auto-slide: pressed east against the bench, the player is carried
    // **north** instead — a move the key did not name, which the report must show
    // rather than leave a reader suspecting it.
    assert_eq!(
        seen.turns[7].to,
        Cell { x: 37, y: 7 },
        "the auto-slide carried them on, perpendicular to the key pressed",
    );

    // An east against the wall: a turn spent, nobody moved.
    assert!(
        seen.turns[10].stayed_put(),
        "input 11 was blocked and should show as stayed: {:?}",
        seen.turns[10].events,
    );

    // The twelfth goes south; the thirteenth is pressed south and ducks behind the
    // bench there — a spent turn that changes posture rather than position (§10.3).
    let last = seen.turns.last().expect("thirteen records");
    assert_eq!(last.input, Input::Step(Direction::South));
    assert!(last.stayed_put(), "the crouch is a pose, not a step");
    assert!(
        last.events
            .iter()
            .any(|e| matches!(e, Event::Crouched { .. })),
        "the last input ducked behind the table: {:?}",
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
    // The near line's own words for what the run actually did, not a second
    // vocabulary. The event is whichever one this level's run produces — the claim
    // being pinned is that the report quotes the game rather than paraphrasing it.
    let crouch = intrusion_core::message_for(Event::Crouched {
        behind: Cell { x: 27, y: 4 },
    })
    .expect("the crouch speaks");
    assert!(
        report.contains(&crouch.text),
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
