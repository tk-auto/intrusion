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
/// What this one names, since #466 re-carved it, is a player who never left their own
/// tunnel. They opened on the **way out** — the border cell at the far end of the
/// crawlspace they dug (§4.5) — pressed north five times into the border wall, pressed
/// east six times *off the board* and were refused for want of intel, and finished with
/// two more presses into the wall to the south. Thirteen inputs, thirteen free actions,
/// nought turns spent.
///
/// That is a *better* fixture than the one it replaces, because it is exactly the case
/// this mode's own doc names as the question a pasted link is really asking: `--script`
/// would have reported "capture at turn 61", when the true answer is "they never got
/// out of the tunnel, and the exit refused them six times". Every claim below is one a
/// reader of the report has to be able to trust: where they started, that nothing moved,
/// and which presses were the refusal rather than the wall.
///
/// **The run this link names changes with every generation change** — #387, then #452,
/// now #466 — and that is worth understanding rather than papering over: a level-seed
/// token encodes the *seed*, and the level is a function of the seed **and the
/// generator**. Any generation change therefore re-carves every previously shared
/// token; the link still parses and still names a v1 level, but not the one the
/// player walked. The fixture is refreshed with the generator, exactly as the
/// committed sim baseline is.
///
/// The assertions are rewritten with it rather than loosened. The *kinds* of turn this
/// file pins — a real move, a spent turn that changed posture instead of position, the
/// #57 auto-slide — moved with the re-carve to
/// [`inspecting_is_deterministic_and_matches_a_plain_boot`] and
/// [`the_opening_crawl_reads_as_the_moves_it_is`], which drive their own scripts and so
/// cannot be emptied out by the next one.
#[test]
fn the_pasted_link_reproduces_the_run_that_was_played() {
    let (level, inputs) = parse_replay_link(PASTED).expect("a real link");
    let seen = inspect(&level, &inputs).expect("the v1 footprint carves");

    assert_eq!(seen.start, Cell { x: 39, y: 1 }, "where they opened");
    assert_eq!(seen.turns.len(), 13, "one record per pasted input");

    // Nobody moved, all thirteen inputs: the report has to show that, or a bare
    // position list would read as a player standing still by choice.
    for turn in &seen.turns {
        assert!(turn.stayed_put(), "input {} moved", turn.index);
    }
    assert_eq!(
        seen.state.turn(),
        0,
        "and every one of them was free (§4.4)"
    );

    // The five norths are the border wall — a plain blocked bump.
    for turn in &seen.turns[..5] {
        assert!(
            turn.events
                .iter()
                .any(|e| matches!(e, Event::Bumped { .. })),
            "input {} was a wall bump: {:?}",
            turn.index,
            turn.events,
        );
    }

    // The six easts are the **way out** (§4.5/#466), aimed off the board from the
    // tunnel's border cell — refused, and saying what the gate still wants.
    for turn in &seen.turns[5..11] {
        assert!(
            turn.events
                .iter()
                .any(|e| matches!(e, Event::ExitRefused { still_needed: 3 })),
            "input {} was the refused way out: {:?}",
            turn.index,
            turn.events,
        );
    }

    // …and the last two are the wall again, so the report ends where it began.
    let last = seen.turns.last().expect("thirteen records");
    assert_eq!(last.input, Input::Step(Direction::South));
    assert_eq!(last.to, seen.start);
    assert_eq!(seen.outcome(), Outcome::Playing, "the run was still live");
}

/// The **opening crawl** reads as what it is (§4.5/§10.7/#466): every run now begins
/// inside the player's own tunnel, and the first inputs are real moves along it — not
/// the blocked presses a reader might take them for, and not free actions.
///
/// This is where the "a real move" claim lives now that the pasted link above spends
/// all thirteen of its inputs standing still. It drives its own script, so a later
/// re-carve moves *which* level it crawls out of, never whether there is a crawl.
#[test]
fn the_opening_crawl_reads_as_the_moves_it_is() {
    let level = LevelSeed::sim(7);
    // West, because this seed's tunnel comes out through the east border — the crawl
    // runs inward from the way-out cell it opens on.
    let inputs = parse_script("WWW").expect("a legal script");
    let seen = inspect(&level, &inputs).expect("the v1 footprint carves");

    let crawls = seen
        .turns
        .iter()
        .filter(|t| {
            t.events
                .iter()
                .any(|e| matches!(e, Event::DuctCrawled { .. }))
        })
        .count();
    assert!(crawls > 0, "the run opens with a crawl: {:?}", seen.turns);
    for turn in seen.turns.iter().take(crawls) {
        assert!(!turn.stayed_put(), "input {} moved a cell", turn.index);
    }
    assert_eq!(
        seen.state.turn(),
        crawls as u32,
        "a crawl is a spent turn (§4.4/§10.7)",
    );
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
