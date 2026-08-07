//! The end screen, pinned as text (§11.1/§12.1/#138).
//!
//! Every screen in this crate prints as a grid of characters, which is what makes a
//! verdict assertable without a browser: these tests read the rows the player reads.

use super::*;
use crate::ability::Loadout;
use crate::cell::Direction;
use crate::difficulty::Difficulty;
use crate::guard::Guard;
use crate::level_seed::LevelSeed;
use crate::place::LevelConfig;
use crate::render::{render_screen, ScreenUi, BOTTOM_ROWS, TOP_ROWS};
use crate::state::{Input, Outcome, State};
use crate::test_support::{leave_by_the_tunnel, open_room, room_with_tunnel};
use crate::verdict::{RunMode, RunOptions};
use crate::Cell;

/// A board wide as the v1 facility (§10.2) and tall enough to hold the panel with
/// board showing above and below it — the arrangement the screen is designed for.
const BOARD: (u32, u32) = (LevelConfig::V1.width, 24);

/// A run that ends in **capture**, by the shortest honest route: a responding guard
/// two cells away steps onto the player while they wait (the §7.6 fixture the guard
/// tests use, on a board the panel fits).
fn captured_run() -> State {
    let mut guard = Guard::patrolling(Cell::new(6, 4));
    guard.respond_to(Cell::new(1, 4));
    let mut state = State::new(
        open_room(BOARD.0, BOARD.1),
        Cell::new(4, 4),
        Direction::North,
        vec![guard],
        Vec::new(),
        Cell::new(8, 8),
    );
    state.step(Input::Wait);
    assert_eq!(state.outcome(), Outcome::Lost, "the fixture ends captured");
    state
}

/// A run that ends **won**: no objectives, so the intel gate is vacuously satisfied
/// (§4.5), and the player climbs into the tunnel beside them and crawls out (#466).
fn won_run() -> State {
    let mut state = State::new(
        room_with_tunnel(BOARD.0, BOARD.1, Cell::new(5, 4), Direction::East),
        Cell::new(4, 4),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(5, 4),
    );
    leave_by_the_tunnel(&mut state);
    assert_eq!(state.outcome(), Outcome::Won, "the fixture ends won");
    state
}

/// The frame as text, for a run and an end-screen view state.
fn screen(state: &State, end: EndUi) -> Vec<String> {
    render_screen(
        state,
        ScreenUi {
            end,
            ..ScreenUi::default()
        },
    )
    .to_text()
}

/// Whether any row of the frame contains `text`.
fn shows(rows: &[String], text: &str) -> bool {
    rows.iter().any(|row| row.contains(text))
}

/// The row index carrying `text`.
fn row_of(rows: &[String], text: &str) -> u32 {
    rows.iter()
        .position(|row| row.contains(text))
        .unwrap_or_else(|| panic!("{text:?} is on the screen: {rows:#?}")) as u32
}

/// **The loss screen names the cause** (§2.2/§14 v2): the guard's mood at contact,
/// over a board that still reads above and below the panel — which is where the *cell*
/// is answered, the panel deliberately printing no coordinates.
#[test]
fn a_capture_draws_the_cause_it_was_latched_with() {
    let state = captured_run();
    let rows = screen(&state, EndUi::default());

    assert!(shows(&rows, CAPTURED_HEADING), "the verdict: {rows:#?}");
    // The mood is Chasing — the guard that turned and stepped in — and the line is
    // that mood's story, not a generic "caught".
    assert!(
        shows(&rows, capture_cause(GuardState::Chasing)),
        "the cause line: {rows:#?}",
    );
    // The contact cell is on the board behind the panel, not spelled out on it: a
    // pair of coordinates is a thing to decode, and the picture already shows it.
    assert!(
        !rows.iter().any(|row| row.contains("4,4")),
        "the panel prints no coordinates: {rows:#?}",
    );
    assert!(!shows(&rows, ESCAPED_HEADING), "a loss is not a win");

    // …and the board is still there to trace it on: the panel is an overlay, so the
    // frame keeps rows of map above and below it.
    let heading = row_of(&rows, CAPTURED_HEADING);
    assert!(heading > TOP_ROWS, "board above the panel: {rows:#?}");
    assert!(
        rows.len() as u32 - heading > 1,
        "board below the panel: {rows:#?}",
    );
}

/// The cause line is **the guard's own mood**, one story per §7.4 state — the
/// distinction the whole screen exists to draw. A patrol that walked into you and a
/// hunt that ran you down are different mistakes, and they must not read alike.
#[test]
fn every_guard_mood_tells_its_own_capture_story() {
    let mut seen: Vec<&str> = Vec::new();
    for state in [
        GuardState::Calm,
        GuardState::Alerted,
        GuardState::Investigating,
        GuardState::Chasing,
        GuardState::Responding,
    ] {
        let cause = capture_cause(state);
        assert!(
            !seen.contains(&cause),
            "{state:?} reuses another mood's line"
        );
        seen.push(cause);
    }
}

/// **The win screen is visibly distinct and carries the run's numbers** (§14 v2).
#[test]
fn a_won_run_draws_the_victory_screen_and_its_ledger() {
    let state = won_run();
    let rows = screen(&state, EndUi::default());

    assert!(shows(&rows, ESCAPED_HEADING), "the verdict: {rows:#?}");
    assert!(shows(&rows, ESCAPED_CAUSE), "and what it means: {rows:#?}");
    assert!(!shows(&rows, CAPTURED_HEADING), "a win is not a loss");

    let stats = state.run_stats();
    for row in ledger(stats) {
        assert!(
            shows(&rows, &row.0),
            "the ledger row {:?}: {rows:#?}",
            row.0
        );
    }
    // A level with no consoles is won on the exit bump alone, and the haul row says
    // so — the ledger reports the run, not a template.
    assert!(shows(&rows, "intel 0 of 0"), "{rows:#?}");
}

/// **A won run's score names every axis** (#563) — the whole point of three stars rather
/// than a tier is knowing *which* one you missed, so a screen that printed only `★★☆`
/// would have said nothing worth the rows.
#[test]
fn a_won_run_names_each_axis_and_whether_it_was_earned() {
    let state = won_run();
    let rows = screen(&state, EndUi::default());
    let score = state
        .verdict()
        .expect("a finished run")
        .score()
        .expect("a run that got out is scored");

    for axis in Axis::ALL {
        assert!(
            shows(&rows, axis.label()),
            "the {} axis is named: {rows:#?}",
            axis.label(),
        );
        assert!(
            shows(&rows, axis.blurb()),
            "…and says what it was for: {rows:#?}",
        );
    }
    assert!(
        shows(&rows, &score.marks()),
        "the glance form too: {rows:#?}"
    );
}

/// **A lost run is not scored** (§14 v2): the screen owes it a reason, and three empty
/// stars beside a capture would be a rating standing where the reason belongs.
#[test]
fn a_lost_run_shows_no_stars_at_all() {
    let state = captured_run();
    let rows = screen(&state, EndUi::default());
    assert_eq!(
        state.verdict().expect("a finished run").score(),
        None,
        "a capture has no score",
    );
    for axis in Axis::ALL {
        assert!(!shows(&rows, axis.blurb()), "{rows:#?}");
    }
    for mark in [STAR_EARNED, STAR_MISSED] {
        assert!(
            !rows.iter().any(|row| row.contains(mark)),
            "{mark} is drawn on a screen with nothing to score: {rows:#?}",
        );
    }
}

/// **A run in progress draws neither** — the property comes from the state, not from a
/// flag the shell has to remember to clear.
#[test]
fn a_live_run_draws_no_verdict_at_all() {
    let state = State::new(
        open_room(BOARD.0, BOARD.1),
        Cell::new(4, 4),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(8, 8),
    );
    assert_eq!(state.outcome(), Outcome::Playing);
    assert_eq!(state.verdict(), None, "a live run has no verdict to draw");

    let rows = screen(&state, EndUi::default());
    for heading in [CAPTURED_HEADING, ENTOMBED_HEADING, ESCAPED_HEADING] {
        assert!(!shows(&rows, heading), "{heading} is drawn mid-run");
    }
}

/// **The exit set is the mode's** (§2.2/appendix 31): quick play may play the level
/// again, and a campaign run — which is the run — may not. Asserted on the drawn rows
/// *and* on the hit-test, so neither path can offer what the other does not.
#[test]
fn the_exits_drawn_are_the_ones_the_mode_allows() {
    let state = captured_run();
    let height = state.layout().facility().height() + TOP_ROWS + BOTTOM_ROWS;
    let verdict = state.verdict().expect("a finished run");

    for (mode, offered) in [
        (
            RunMode::QuickPlay,
            vec![EndExit::Retry, EndExit::NewRun, EndExit::Menu],
        ),
        (RunMode::Campaign, vec![EndExit::Menu]),
    ] {
        let ui = EndUi {
            options: RunOptions {
                mode,
                ..RunOptions::default()
            },
            ..EndUi::default()
        };
        let rows = screen(&state, ui);
        for exit in [EndExit::Retry, EndExit::NewRun, EndExit::Menu] {
            let drawn = shows(&rows, exit.label());
            assert_eq!(
                drawn,
                offered.contains(&exit),
                "{mode:?} draws {exit:?}: {rows:#?}",
            );
            // …and what is not drawn is not reachable by finger either: no row of the
            // whole frame hit-tests to it.
            let hittable =
                (0..height).any(|y| verdict_hit(height, verdict, ui, None, y) == Some(exit));
            assert_eq!(
                hittable,
                offered.contains(&exit),
                "{mode:?} offers {exit:?}"
            );
        }
    }
}

/// Every exit is hittable **exactly where it is drawn**, at any column (§11.6: the
/// whole row is the target), and the rows between them are inert — a mis-aimed tap
/// lands on a blank and does nothing rather than on the neighbour.
#[test]
fn an_exit_is_hittable_across_its_whole_row_and_nowhere_else() {
    let state = captured_run();
    let facility = state.layout().facility();
    let (width, height) = (facility.width(), facility.height() + TOP_ROWS + BOTTOM_ROWS);
    let verdict = state.verdict().expect("a finished run");
    let ui = EndUi::default();
    let rows = screen(&state, ui);

    for exit in ui.exits() {
        let y = row_of(&rows, exit.label());
        for x in [0, width / 2, width - 1] {
            assert_eq!(
                verdict_hit(height, verdict, ui, None, y),
                Some(*exit),
                "column {x} of {exit:?}'s row",
            );
        }
        assert_eq!(
            verdict_hit(height, verdict, ui, None, y - 1),
            None,
            "the blank above {exit:?} is inert",
        );
    }
    // A press outside the frame lands on nothing rather than wrapping into a row.
    assert_eq!(verdict_hit(height, verdict, ui, None, height), None);
}

/// **Both screens carry the run's seed** (#138/#353): the sharing loop is "seed 8371
/// got me like this", and the end of a run is exactly when that is worth saying. A
/// hand-built state has no token and prints no row rather than one that boots a
/// different level (#333).
#[test]
fn the_seed_is_on_the_screen_when_the_run_has_one() {
    let level = LevelSeed::quick_play_at(8371, Difficulty::Harder);
    let token = level.encode().expect("a config a run can hold");

    let bare = captured_run();
    assert!(
        !shows(&screen(&bare, EndUi::default()), SEED_LABEL),
        "a hand-built run prints no seed row",
    );

    let state = captured_run().with_level(level);
    assert!(
        shows(&screen(&state, EndUi::default()), &token),
        "the token the run boots from",
    );
}

/// Nothing the panel prints overruns the v1 board (§10.2's 40 columns). Measured over
/// every line the screen can draw — each mood's cause line, both ledger rows at their
/// widest, the headings and the footers — because a clipped verdict is a verdict that
/// says something else.
#[test]
fn every_line_fits_the_v1_board() {
    let max = LevelConfig::V1.width as usize;
    let stats = RunStats {
        turns: 999,
        intel: 9,
        intel_total: 9,
        caches: 9,
        caches_total: 9,
        par: 999,
        takedowns: 99,
        detections: 999,
        alert_peak: 0,
        salvaged: Loadout::empty(),
        held: Loadout::innate(),
    };
    let mut lines: Vec<String> = vec![
        CAPTURED_HEADING.into(),
        ENTOMBED_HEADING.into(),
        ESCAPED_HEADING.into(),
        ESCAPED_CAUSE.into(),
        ENTOMBED_CAUSE.into(),
        FOOTER_CHOOSE.into(),
        FOOTER_ONE_WAY.into(),
    ];
    lines.extend(ledger(stats).into_iter().map(|(text, _)| text));
    lines.extend(
        ledger(RunStats {
            alert_peak: 3,
            ..stats
        })
        .into_iter()
        .map(|(text, _)| text),
    );
    for state in [
        GuardState::Calm,
        GuardState::Alerted,
        GuardState::Investigating,
        GuardState::Chasing,
        GuardState::Responding,
    ] {
        lines.push(capture_cause(state).into());
    }
    for exit in [EndExit::Retry, EndExit::NewRun, EndExit::Menu] {
        lines.push(format!("{MARKER}{}", exit.label()));
    }
    // The score block (#563) at its widest — every axis earned and every axis missed, so
    // both colourings of every row are measured.
    for score in [
        Score {
            speed: true,
            stealth: true,
            thoroughness: true,
        },
        Score::default(),
    ] {
        lines.extend(score_rows(score).into_iter().map(|(text, _)| text));
    }
    for line in lines {
        assert!(
            line.chars().count() <= max,
            "{line:?} is {} cells on a {max}-wide board",
            line.chars().count(),
        );
    }
}

/// The marker walks the exits the mode offers and nothing else — and a marker left on
/// an exit this mode does not offer resolves to the first one it does, rather than
/// letting a campaign run fire a retry it never drew.
#[test]
fn the_marker_stays_inside_the_modes_exits() {
    let quick = EndUi::default();
    assert_eq!(
        quick.selected(),
        EndExit::Retry,
        "the first exit by default"
    );
    assert_eq!(quick.next(), EndExit::NewRun);
    assert_eq!(quick.prev(), EndExit::Menu, "the list wraps");

    let campaign = EndUi {
        options: RunOptions {
            mode: RunMode::Campaign,
            ..RunOptions::default()
        },
        selected: EndExit::Retry,
    };
    assert_eq!(
        campaign.selected(),
        EndExit::Menu,
        "a retry the mode never offered is not selectable",
    );
    assert_eq!(campaign.next(), EndExit::Menu, "a one-exit list stays put");
    assert_eq!(campaign.prev(), EndExit::Menu);
}
