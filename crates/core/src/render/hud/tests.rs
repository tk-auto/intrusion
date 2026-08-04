use super::*;
use crate::ability::{AbilityMode, Loadout};
use crate::cell::{Cell, Direction};
use crate::guard::Guard;
use crate::guard::GuardState;
use crate::modifiers::LevelModifiers;
use crate::state::{BoreRefusal, Event, Input, State};
use crate::test_support::open_room;
use crate::{Difficulty, EndExit, EndUi, RunMode, RunOptions};

/// A **legal** run loadout (§8.3/#244): innate Run plus a three-tech grant — the
/// shape quick play resolves, and the shape the bar's width bound is sized for.
/// A hand-built [`State::new`] boots the innate set alone, so the bar tests ask
/// for a full grant explicitly rather than measuring a one-entry bar.
fn granted() -> Loadout {
    Loadout::innate()
        .with(AbilityId::Camouflage)
        .with(AbilityId::Decoy)
        .with(AbilityId::Dephase)
}

/// The same grant with the **passive** in it (#264/#287) — Run, two activated
/// tech, and Vision — so the bar's always-on marker is exercised beside the
/// clocks it has to sit next to.
fn granted_with_passive() -> Loadout {
    Loadout::innate()
        .with(AbilityId::Camouflage)
        .with(AbilityId::Decoy)
        .with(AbilityId::Vision)
}

/// **The near-line message width bound** (§11.7): how many cells a message has
/// on the v1 board ([`LevelConfig::V1`], 40 wide — §10.2) before
/// [`status_row`] clips it. Computed from the very functions that lay the row
/// out — the words stop one cell short of the corner cluster, and the widest
/// that cluster gets in practice is the message counter beside the help button
/// — so the bound cannot drift from the layout it is meant to describe.
///
/// It lives here rather than as a `const` because most messages are built with
/// `format!` at runtime (an ability's name, an alert level): there is no const
/// string to measure, so the check is a test that walks the real
/// [`message_for`](crate::status::message_for) instead. ("the body has been
/// reported — guards are converging" was 49 cells and reached a screenshot cut
/// at "…reported —".)
fn near_line_text_max() -> usize {
    super::super::message_log::near_line_text_max(LevelConfig::V1.width)
}

/// Near-line messages that were **already** over the bound before
/// the bound existed (§11.7). They are player-facing wording, not a bug in any
/// one feature, so rewording them is its own change rather than a silent edit
/// smuggled in beside an unrelated one — see the follow-up ticket. Listed
/// explicitly so the bound still bites for every *new* message: adding one here
/// is a deliberate act, and the list only ever shrinks.
///
/// Both survivors overrun the 40-wide board itself — 42 and 45 cells (§10.2) —
/// and have nothing to do with the row's controls.
///
/// The exit's refusal left this list in #310: naming the gate's real requirement
/// ("the exit needs 2 more intel") is shorter than the fixed rule it replaced.
/// **The alert line left it in #375**: the one message about escalation was the one
/// the player could not read, at 34 cells in a 29-cell row, and #375 is where the
/// ladder's legibility was owed. Naming it the way the help panel does — "security
/// condition 3 of 3" — is both shorter and the same words twice. **Three left it in
/// #300 with no word rewritten**, as the chrome was tightened around them: merging
/// the deploy chevron and its `+N` counter into one three-cell control, then
/// reclaiming the blank cell beyond it, took the budget from 28 glyphs to 32, and
/// "all the intel — the exit is open", "the guard drops — a body is left" and "you
/// slip away — the run is won" all fit it.
///
/// That is the argument for [`NEAR_LINE_CONTROL_CELLS`] being derived from the
/// layout rather than written down — and for it counting *cells of message* rather
/// than the column the words stop at, which is one more and is what it used to be:
/// two of those three had been clipping by a single cell while the bound said they
/// were fine.
const PRE_EXISTING_OVERFLOW: [&str; 2] = [
    "you stow the body — the cupboard is sealed",
    "intel in hand — the exit is open (9 more out)",
];

/// §11.7: **every** message the near line can show fits the row it is shown on.
/// `status_row` clips rather than asserting — right for a hand-built test state,
/// but it means an over-long message fails silently in the one place it matters
/// ("the body has been reported — guards are converging" was drawn cut at
/// "…reported —"). Walking `message_for` covers the `format!`-built messages a
/// const bound could not reach.
#[test]
fn every_near_line_message_fits() {
    let at = Cell::new(3, 3);
    // One representative of every variant. The compiler does not enumerate a
    // match's arms for us here, so this list is the thing to extend when an
    // event is added — the assertion below is what makes forgetting expensive.
    let events = [
        Event::Moved { to: at },
        Event::Bumped { into: at },
        Event::EnteredHideout { at },
        Event::EnteredDuct {
            at,
            own_tunnel: false,
        },
        Event::DuctCrawled { to: at },
        Event::Crouched { behind: at },
        Event::DoorOpened {
            at,
            by_player: true,
        },
        Event::DoorOpened {
            at,
            by_player: false,
        },
        Event::DoorClosed {
            at,
            by_player: true,
        },
        Event::DoorClosed {
            at,
            by_player: false,
        },
        Event::IntelTaken {
            remaining: 0,
            still_needed: 0,
        },
        Event::IntelTaken {
            remaining: 9,
            still_needed: 0,
        },
        Event::IntelTaken {
            remaining: 9,
            still_needed: 9,
        },
        Event::ExitRefused { still_needed: 9 },
        Event::Won,
        Event::Captured {
            guard: 0,
            state: GuardState::Chasing,
            at,
        },
        Event::TakenDown { at },
        Event::Detected { by: at },
        Event::BodyFound { at },
        Event::RadioSilence { at },
        Event::CalledIn { at },
        Event::BodyCalledIn { at },
        Event::AlertRaised {
            rung: 3,
            trigger: crate::AlertTrigger::BodyFound,
        },
        Event::BodyGrabbed { at },
        Event::BodyReleased { at },
        Event::BodyStored { at },
        Event::DecoyDied { at },
        Event::Ejected {
            from: at,
            to: at,
            stunned: crate::phase_eject_stun(1),
        },
        Event::Entombed { at },
        Event::RematerializeRefused,
        Event::WallBored { at },
    ];
    // Every bore refusal is a near-line message of its own (§8.4/#303), so each
    // wording is measured rather than just one representative.
    let events = events.into_iter().chain(
        [
            BoreRefusal::NothingToBore,
            BoreRefusal::TooManyWalls,
            BoreRefusal::TheOuterShell,
            BoreRefusal::NoUsesLeft,
        ]
        .map(|reason| Event::BoreRefused { reason }),
    );
    let max = near_line_text_max();
    for event in events {
        let Some(m) = crate::status::message_for(event) else {
            continue; // a silent event says nothing to measure
        };
        let len = m.text.chars().count();
        if PRE_EXISTING_OVERFLOW.contains(&m.text.as_str()) {
            continue;
        }
        assert!(
            len <= max,
            "{:?} is {len} cells, over the {max} the near line leaves beside \
                 its controls: {:?}",
            event,
            m.text,
        );
    }

    // Every ability's activation line too — those are `format!`-built from a
    // name, so the longest name is what decides whether they fit. The budgeted
    // activation (§8.2/#302) is the longest of them, so both of its wordings —
    // the count and the spent-it-all one — are measured here as well.
    for ability in AbilityId::ALL {
        for event in [
            Event::AbilityActivated {
                ability,
                uses_left: None,
            },
            Event::AbilityActivated {
                ability,
                uses_left: Some(9),
            },
            Event::AbilityActivated {
                ability,
                uses_left: Some(0),
            },
            Event::AbilityDeactivated { ability },
            Event::AbilityExpired { ability },
        ] {
            // A budgeted activation is deliberately silent (§8.2/#302) — nothing
            // to measure, so nothing to fit.
            let Some(m) = crate::status::message_for(event) else {
                continue;
            };
            assert!(
                m.text.chars().count() <= max,
                "{:?} does not fit the near line",
                m.text,
            );
        }
    }
}

/// #420, reversing #375: the `[?]` toggle wears **one static colour at every rung**
/// — the System tan the deploy control and the panel's `[x]` wear.
///
/// The tint was the ladder's always-visible half while the near line said the
/// condition once and was overwritten. The near line now carries the standing alert
/// itself, in words and in its band, so the tint became a second and quieter
/// statement of the same fact sitting on top of it — a red `[?]` on a red band at
/// the top rung. The job survives; the duplicate does not.
#[test]
fn the_help_toggle_is_one_colour_at_every_rung() {
    let mut layout = open_room(40, 14);
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
    // The `[?]` owns the row's first cell (#267/#300).
    let toggle = |s: &State| render_screen(s, ScreenUi::default()).get(0, NEAR_ROW).fg;

    assert_eq!(s.alert(), 0, "a fresh raid is unnoticed");
    assert_eq!(toggle(&s), Category::System);

    // A takedown whose body the second guard's cone covers: rung 3, the top — the
    // one case the old tint shouted loudest about.
    s.step(Input::Step(Direction::North));
    assert_eq!(s.alert(), crate::TOP_RUNG, "the find tops the ladder");
    assert_eq!(
        toggle(&s),
        Category::System,
        "the control is furniture at every rung — the row below it carries the alert",
    );
}

/// #420, the rule the near line's band now runs on: **an ambient band paints the
/// quiet fill, a message band the full one.** The row's colour separates the
/// facility's standing mood — permanently on screen, and so not news — from
/// something that has just happened, which flashes.
///
/// It matters most where the two shades mean different things: a standing Danger row
/// in the *live* fill would spend the §11.5 overlay's own colour, the one that means
/// a threat has you **right now**, on a fact that is simply true from here on.
#[test]
fn an_ambient_band_is_quiet_and_a_message_band_is_not() {
    let mut s = State::new(
        open_room(40, 14),
        Cell::new(5, 6),
        Direction::North,
        Vec::new(),
        [Cell::new(5, 5)],
        Cell::new(8, 8),
    );
    let band = |s: &State| {
        let cell = render_screen(s, ScreenUi::default()).get(0, NEAR_ROW);
        (cell.bg, cell.fill)
    };

    // A quiet turn: the near line rests on the ambient floor (§11.4).
    s.step(Input::Wait);
    assert!(near_line(&s).is_ambient(), "the floor, not a message");
    assert_eq!(band(&s), (Some(Category::Interest), Fill::Quiet));

    // Taking the intel raises a real message: the same row, the full fill.
    s.step(Input::Step(Direction::North));
    assert!(!near_line(&s).is_ambient(), "a live message");
    assert_eq!(band(&s), (Some(Category::Interest), Fill::Full));

    // …and the next action clears it back to the floor, and to the quiet fill with
    // it — the flash is exactly as long as the message (§11.7).
    s.step(Input::Step(Direction::South));
    assert!(near_line(&s).is_ambient());
    assert_eq!(band(&s).1, Fill::Quiet);
}

/// #420: the band's fill is the row's **whole** width, controls included. The `[?]`
/// and the deploy toggle draw over the band rather than beside it, so a control
/// painting the other shade would put a bright notch in a quiet row.
#[test]
fn the_near_lines_controls_share_the_bands_fill() {
    let mut s = State::new(
        open_room(40, 14),
        Cell::new(5, 6),
        Direction::North,
        Vec::new(),
        [Cell::new(5, 5)],
        Cell::new(8, 8),
    );
    s.set_auto_slide(false);
    for expected in [Fill::Quiet, Fill::Full] {
        let g = render_screen(&s, ScreenUi::default());
        for x in 0..g.width() {
            let cell = g.get(x, NEAR_ROW);
            assert_eq!(
                (cell.bg.is_some(), cell.fill),
                (true, expected),
                "column {x} of the near line breaks the band",
            );
        }
        s.step(Input::Step(Direction::North)); // take the intel: a live message
    }
}

/// §11.5/#420: the **map** is untouched. Its fills still follow the fog — full in
/// view, quiet beyond it — which is what `Fill::fogged` exists to guarantee, and
/// what the danger overlay's in-view/out-of-view pair is built on.
#[test]
fn the_boards_fills_still_follow_the_fog() {
    let s = State::new(
        open_room(20, 12),
        Cell::new(5, 8),
        Direction::North,
        vec![Guard::stationary(Cell::new(5, 4))],
        Vec::new(),
        Cell::new(18, 10),
    );
    let g = render(&s);
    let mut watched = 0;
    for y in 0..g.height() {
        for x in 0..g.width() {
            let cell = g.get(x, y);
            assert_eq!(
                cell.fill,
                Fill::fogged(cell.vis),
                "board cell ({x},{y}) paints against its own fog",
            );
            watched += u32::from(cell.bg == Some(Category::Danger));
        }
    }
    assert!(watched > 0, "the fixture puts a cone on the board");
}

/// The §11.4 golden test, whole screen (#267/#287): the near and usable lines on
/// top, then the map, then the always-on ability bar — one grid, printed as
/// text. The near line rests on ambient floor and carries the `[?]` toggle in
/// the top-right corner; the usable line offers the adjacent console; the bar
/// **names** every held ability, flush to the bottom-right with its one-cell
/// margin. Nothing covers the board — there is no panel left to deploy.
///
/// The map half also pins the §11.5a schematic (#307) end to end, which a
/// whole-frame golden shows better than any single assertion: the player's own
/// ~180° half-disc reads in real glyphs, and everything they have never had eyes
/// on — the run of wall behind them, the far two-thirds of the room — reads as
/// `□` fabric with blank floor space between it (#470). It shows the two absences
/// meeting, too: the far room is blank because it has never been seen, and the
/// strip of floor west of the player is blank because it has been walked and left
/// behind — the dots are the FOV's own ink. The exit keeps its `E` out there
/// regardless, the one thing on this map that is theirs (§4.5).
#[test]
fn the_full_screen_renders_golden() {
    let s = State::new(
        open_room(40, 6),
        Cell::new(2, 2),
        Direction::North,
        Vec::new(),
        [Cell::new(3, 2)], // a console east of the player
        Cell::new(38, 4),
    )
    .with_loadout(granted());
    let text = render_screen(&s, ScreenUi::default()).to_text();
    assert_eq!(
        text,
        vec![
            "[?] objectives: 0/1                     ".to_string(),
            // The console is east of the player, so its entry is flush right
            // with the arrow trailing — the row aims where it points (#384).
            "                  console: take intel → ".to_string(),
            "##################□□□□□□□□□□□□□□□□□□□□□□".to_string(),
            "#·················                     □".to_string(),
            "#·@$··············                     □".to_string(),
            "#·················                     □".to_string(),
            "□     ············                    E□".to_string(),
            "□□□□□□□□##########□□□□□□□□□□□□□□□□□□□□□□".to_string(),
            "Run       Camo      Decoy     Phase     ".to_string(),
        ]
    );
}

/// §11.4/#323: with nothing adjacent to act on, the usable line teaches the two
/// innate verbs instead of sitting blank — in the vocabulary the player's hands
/// are using ([`ScreenUi::modality`]). The words draw in Owned — *yours*, the
/// pair you always hold (§11.2), the ability bar's own ready colour — and carry
/// no band, so the row still reads as status rather than as a message.
#[test]
fn the_empty_usable_line_teaches_move_and_wait() {
    // Mid-corridor, nothing adjacent: the common case the blank row used to be.
    let s = State::new(
        open_room(40, 6),
        Cell::new(20, 3),
        Direction::North,
        Vec::new(),
        [Cell::new(2, 2)],
        Cell::new(38, 4),
    );
    assert!(s.affordances().is_empty(), "nothing to act on");

    let row = |ui: ScreenUi| {
        let g = render_screen(&s, ui);
        (0..g.width())
            .map(|x| g.get(x, USABLE_ROW).glyph)
            .collect::<String>()
    };
    assert_eq!(
        row(ScreenUi::default()),
        " ↑↓←→: move  w: wait                    ",
        "keys: §11.6's own table, in the row's `input: action` rhythm"
    );
    assert_eq!(
        row(ScreenUi {
            modality: InputModality::Touch,
            ..ScreenUi::default()
        }),
        " swipe: move  tap: wait                 ",
        "touch: the gesture model, the held press deliberately unnamed"
    );

    // Owned, and no band: the verbs are yours, and the row is still not a message.
    let g = render_screen(&s, ScreenUi::default());
    for x in 0..g.width() {
        let cell = g.get(x, USABLE_ROW);
        assert_eq!(cell.bg, None, "the usable line still has no band");
        if cell.glyph != ' ' {
            assert_eq!(
                cell.fg,
                Category::Owned,
                "the innate verbs are yours (§11.2), in the bar's ready colour"
            );
        }
    }
}

/// The hint is a **floor, never a competitor** (§11.4/#323): one adjacent usable
/// and the affordances have the whole row back, in both modalities — the player
/// never has to read past a control legend to find what they can bump.
#[test]
fn a_real_affordance_takes_the_whole_row_back() {
    let s = State::new(
        open_room(40, 6),
        Cell::new(2, 2),
        Direction::North,
        Vec::new(),
        [Cell::new(3, 2)], // a console east of the player
        Cell::new(38, 4),
    );
    for modality in [InputModality::Keys, InputModality::Touch] {
        let g = render_screen(
            &s,
            ScreenUi {
                modality,
                ..ScreenUi::default()
            },
        );
        let row: String = (0..g.width()).map(|x| g.get(x, USABLE_ROW).glyph).collect();
        // Aimed east — flush right, arrow trailing (#384) — and nothing of the
        // floor left on the row beside it.
        assert_eq!(row.trim(), "console: take intel →", "{modality:?}: {row:?}");
    }
}

/// The screen is the map plus the header and status rows, same width — and the
/// two status rows carry their §11.4 styling: the near line is a full-width
/// band in the message's category with Neutral words on top; the usable line
/// has no band and speaks each affordance's own category.
#[test]
fn status_rows_carry_the_band_and_the_categories() {
    let mut s = State::new(
        open_room(24, 6),
        Cell::new(2, 2),
        Direction::North,
        Vec::new(),
        [Cell::new(3, 2)],
        Cell::new(22, 4),
    );
    let map = render(&s);
    let g = render_screen(&s, ScreenUi::default());
    assert_eq!(g.width(), map.width());
    assert_eq!(g.height(), TOP_ROWS + map.height() + BOTTOM_ROWS);

    let (near_y, usable_y) = (NEAR_ROW, USABLE_ROW);
    let controls = near_line_controls(&s, g.width(), false);
    for x in 0..g.width() {
        let cell = g.get(x, near_y);
        assert_eq!(cell.bg, Some(Category::Interest), "the band spans the row");
        assert_eq!(cell.vis, Visibility::Live);
        if cell.glyph != ' ' && x >= controls.text_start && x < controls.text_max {
            assert_eq!(cell.fg, Category::Neutral, "words read Neutral on the band");
        }
        assert_eq!(g.get(x, usable_y).bg, None, "the usable line has no band");
    }
    // The `[?]` rides the band in the HUD control colour, not the words' Neutral,
    // and it owns the row's left end (#267).
    assert_eq!(g.get(0, near_y).glyph, '[');
    assert_eq!(g.get(0, near_y).fg, Category::System);
    // The affordance names its bump direction and speaks its own category:
    // `console: take intel →` is Interest (§11.2 — goals and rewards). The
    // console is east of the player, so the entry is flush right behind a
    // one-cell margin with the arrow trailing (#384).
    let right = g.width() - 2;
    assert_eq!(g.get(right, usable_y).glyph, '→');
    assert_eq!(g.get(right, usable_y).fg, Category::Interest);
    assert_eq!(g.get(right - 20, usable_y).glyph, 'c');
    assert_eq!(g.get(right - 20, usable_y).fg, Category::Interest);

    // A threat message flips the whole band to its category: get captured
    // and the near line reads Danger — the colour flash before the words.
    s = State::new(
        open_room(24, 6),
        Cell::new(2, 3),
        Direction::North,
        // Walking south, its spawn facing, straight into the player — no corner,
        // so no §229 turn tax delays the contact.
        vec![Guard::patrolling_to(Cell::new(2, 1), Cell::new(2, 4))],
        Vec::new(),
        Cell::new(22, 4),
    );
    s.step(Input::Wait); // the guard steps south into the player: caught
    let g = render_screen(&s, ScreenUi::default());
    assert_eq!(g.get(0, NEAR_ROW).bg, Some(Category::Danger));
    // The words start clear of the `[?]` at the row's left end (#300).
    assert_eq!(g.get(HELP_BUTTON_LEN + 1, NEAR_ROW).glyph, 'c'); // "caught"
}

/// The permanent home of ability state (§11.4/#267/#287): the **always-on
/// ability bar** on the frame's last row, assembled from the run's real economy
/// ([`State::ability_statuses`]). A fresh run has every held ability ready, so the
/// bar is their **names** in deck order, each in Owned, **right-aligned** into the
/// bottom-right corner behind a one-cell margin. The two bump verbs (Takedown,
/// Drag) are **not** on it: they live on the usable line, not the ability economy
/// (§7.2/§8.3).
#[test]
fn the_always_on_bar_names_every_held_ability() {
    let s = State::new(
        open_room(40, 10),
        Cell::new(15, 5),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(38, 8),
    )
    .with_loadout(granted());
    let g = render_screen(&s, ScreenUi::default());
    let bar = ability_row(10);
    assert_eq!(bar + BOTTOM_ROWS, g.height(), "the bar is the last row");

    // Four ready abilities in four ten-cell slots, filling the 40-wide row.
    for (col, name) in [(0, "Run"), (10, "Camo"), (20, "Decoy"), (30, "Phase")] {
        let drawn: String = (col..col + name.len() as u32)
            .map(|x| g.get(x, bar).glyph)
            .collect();
        assert_eq!(drawn, name, "{name} at col {col}");
        // Sampled one cell in: the entry's *first* cell is its mnemonic, lifted to
        // the ink colour (§11.6/#360), so the state colour is read off any other.
        assert_eq!(
            g.get(col + 1, bar).fg,
            Category::Owned,
            "{name} ready colour"
        );
    }
    // Each name left-aligned in its slot, the strip flush right, one cell of air
    // after the last: the four slots exactly fill the v1 row.
    let row: String = (0..g.width()).map(|x| g.get(x, bar).glyph).collect();
    assert_eq!(row, "Run       Camo      Decoy     Phase     ", "{row:?}");
    // The bump verbs never appear on the ability bar.
    assert!(
        !row.contains("Takedown"),
        "Takedown is not an economy ability"
    );
    assert!(!row.contains("Drag"), "Drag is not an economy ability");
    // Nor does an ability the run was not granted (#244).
    assert!(!row.contains("Doors"), "Autodoors was not in the loadout");
}

/// **The bar greys a press that cannot fire** (§11.4/#345): the contextual
/// `Unusable` the catalog always documented and nothing ever produced. Pierce
/// Wall is the clearest case, because its precondition is *exactly one adjacent
/// wall* (§8.3/#303) and the same three cells of board decide it.
///
/// Three stands, one grid each:
///
/// - **in a room** — no wall touches the player, so there is nothing to bore;
/// - **in a corridor** — two side walls, and the target would be ambiguous, which
///   this ability never resolves (§8.4 [SETTLED]);
/// - **against one wall** — the one geometry it works in.
///
/// The first two draw `Bore—` receding into [`Category::Ground`] beside the other
/// things you cannot do; the third draws `Bore(3)` in Owned, its budget intact
/// throughout. Same run, same supply, three cells apart: what changed is the
/// board, which is exactly what the bar could not say before.
#[test]
fn the_bar_greys_an_ability_with_no_target() {
    let borer = |layout| {
        State::new(
            layout,
            Cell::new(15, 5),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(38, 8),
        )
        .with_loadout(Loadout::innate().with(AbilityId::PierceWall))
    };
    let bar = ability_row(10);
    let row = |s: &State| -> String {
        let g = render_screen(s, ScreenUi::default());
        (0..g.width()).map(|x| g.get(x, bar).glyph).collect()
    };
    // Bore's entry starts at column 30, so 30 is its mnemonic `B` and 31 is the
    // first cell carrying the plain state colour (§11.6/#360).
    let colour = |s: &State| render_screen(s, ScreenUi::default()).get(31, bar).fg;
    let letter = |s: &State| render_screen(s, ScreenUi::default()).get(30, bar).fg;

    // In the middle of the room: nothing to bore.
    let s = borer(open_room(40, 10));
    assert_eq!(row(&s), "                    Run       Bore—     ");
    assert_eq!(colour(&s), Category::Ground, "greyed, not promised");
    // …and its mnemonic greys with it (#360): an entry that is not on offer does
    // not advertise a key, so the letter is *not* lifted out of the name here.
    assert_eq!(letter(&s), Category::Ground, "the letter recedes too");

    // In a corridor: two side walls, so the target is ambiguous and refused.
    let mut layout = open_room(40, 10);
    layout.place(Cell::new(14, 5), Terrain::Wall);
    layout.place(Cell::new(16, 5), Terrain::Wall);
    let s = borer(layout);
    assert_eq!(row(&s), "                    Run       Bore—     ");
    assert_eq!(colour(&s), Category::Ground, "two walls is no target");

    // Square against one wall face: the one geometry it works in, and the budget
    // it had all along finally shows.
    let mut layout = open_room(40, 10);
    layout.place(Cell::new(16, 5), Terrain::Wall);
    let s = borer(layout);
    assert_eq!(row(&s), "                    Run       Bore(3)   ");
    assert_eq!(colour(&s), Category::Owned, "available, and says how often");
    // Usable again, so the `B` lifts back out of the name and says "press this".
    assert_eq!(letter(&s), Category::Neutral, "the letter marks the key");
}

/// The bar's live states (§11.4): an **active** ability tucks its `[n]` against
/// its name in Owned, a **cooling** one its `/n/` in System — the exact numbers
/// the economy hands over (§8.2). Driven to Run cooling and Camouflage active,
/// with Decoy and Dephase still ready, so all three notations show at once.
#[test]
fn the_bar_shows_active_and_cooling_state() {
    let mut s = State::new(
        open_room(40, 10),
        Cell::new(15, 5),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(38, 8),
    )
    .with_loadout(granted());
    // Run: activate (Active 4 after the turn's tick) then toggle off — a free
    // action that drops it straight into its full 12 cooldown. Then activate
    // Camouflage: that turn's tick drains Run's cooldown to 11 and leaves
    // Camouflage active with 9 of its 10 left.
    s.step(Input::Activate(AbilityId::Run));
    s.step(Input::Deactivate(AbilityId::Run));
    s.step(Input::Activate(AbilityId::Camouflage));
    assert_eq!(
        s.ability_state(AbilityId::Run),
        AbilityState::Cooling { remaining: 11 }
    );
    assert_eq!(
        s.ability_state(AbilityId::Camouflage),
        AbilityState::Active { remaining: 9 }
    );

    let g = render_screen(&s, ScreenUi::default());
    let bar = ability_row(10);
    let row: String = (0..g.width()).map(|x| g.get(x, bar).glyph).collect();
    // `Run/11/` cooling (System) and `Camo[9]` active (Owned) grew into their
    // slots — and **every name is still in the column it started in**.
    assert_eq!(row, "Run/11/   Camo[9]   Decoy     Phase     ", "{row:?}");
    // Read one cell in from each entry's start: cells 0 and 10 are the mnemonics
    // `R` and `C`, drawn in the ink colour whatever the state (§11.6/#360).
    assert_eq!(g.get(1, bar).fg, Category::System, "cooling reads System");
    assert_eq!(g.get(3, bar).glyph, '/', "cooling shows /N/");
    assert_eq!(g.get(10, bar).glyph, 'C');
    assert_eq!(g.get(11, bar).fg, Category::Owned, "active reads Owned");
    // The mark survives both states — a cooling ability's key is still its key.
    for x in [0, 10] {
        assert_eq!(g.get(x, bar).fg, Category::Neutral, "the mnemonic at {x}");
    }
    assert_eq!(g.get(14, bar).glyph, '[', "active shows [N]");
}

/// **Nothing moves** (§11.4/#287): the fixed slots mean an ability's column is a
/// fact about the run, not about the frame. Drive the deck through activation,
/// an early toggle-off, a two-digit cooldown draining to one digit, and back to
/// ready — and every ability starts on the same cell it started the run on. A
/// bar whose names slide as numbers come and go is one you re-read every glance.
#[test]
fn a_ticking_number_never_shifts_a_neighbour() {
    let mut s = State::new(
        open_room(40, 10),
        Cell::new(15, 5),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(38, 8),
    )
    .with_loadout(granted());
    let bar = ability_row(10);
    // Where each name sits on the very first frame, before anything is used.
    let columns = |s: &State| -> Vec<u32> {
        ability_line_layout(40, &s.ability_statuses())
            .into_iter()
            .map(|(_, start)| start)
            .collect()
    };
    let first = columns(&s);
    assert_eq!(first, vec![0, 10, 20, 30]);

    // Run: on, off, and then the whole 12-turn cooldown drained — `/12/` down
    // through `/9/` to nothing, the digit count changing on the way.
    s.step(Input::Activate(AbilityId::Run));
    s.step(Input::Deactivate(AbilityId::Run));
    for _ in 0..14 {
        assert_eq!(columns(&s), first, "the columns held at turn {}", s.turn());
        let name: String = (0..3)
            .map(|x| {
                render_screen(&s, ScreenUi::default())
                    .get(30 + x, bar)
                    .glyph
            })
            .collect();
        assert_eq!(name, "Pha", "…and the far slot is still Phase");
        s.step(Input::Wait);
    }
    assert_eq!(
        s.ability_state(AbilityId::Run),
        AbilityState::Ready,
        "the cooldown really did run out under the test",
    );
    assert_eq!(columns(&s), first, "and back to ready moved nothing");
}

/// The bar is the keys' **source** (§11.6/#267/#359): every entry the row draws
/// is the slot its digit fires, counted from the leftmost drawn entry — and each
/// ability state still reads its own colour, ready and active Owned, cooling
/// System, so the states stay discoverable without a letter on the row.
#[test]
fn the_bar_slots_are_the_keys_and_the_states_keep_their_colours() {
    let mut s = State::new(
        open_room(40, 10),
        Cell::new(15, 5),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(38, 8),
    )
    .with_loadout(granted());
    let bar = ability_row(10);

    // Every entry the bar draws is the slot its digit fires, and the tap on that
    // entry resolves to the same ability — the one seam, both ways in.
    for (slot, (i, start)) in ability_line_layout(40, &s.ability_statuses())
        .into_iter()
        .enumerate()
    {
        let id = s.ability_statuses()[i].id;
        assert_eq!(
            ability_at(&s, start, bar),
            Some(id),
            "{id:?} under its own entry"
        );
        assert_eq!(
            ability_in_slot(&s, slot),
            Some(id),
            "the {} key fires {id:?}",
            slot + 1,
        );
    }

    // The state colours, in the corner: Run cooling, Camouflage active, the rest
    // ready.
    s.step(Input::Activate(AbilityId::Run));
    s.step(Input::Deactivate(AbilityId::Run));
    s.step(Input::Activate(AbilityId::Camouflage));
    let g = render_screen(&s, ScreenUi::default());
    let entry = |id: AbilityId| {
        let statuses = s.ability_statuses();
        let i = statuses.iter().position(|st| st.id == id).expect("in deck");
        ability_line_layout(40, &statuses)
            .into_iter()
            .find(|(j, _)| *j == i)
            .expect("drawn")
            .1
    };
    // One cell in from each entry's start, since the first cell is its mnemonic
    // and carries the ink colour whatever the state (§11.6/#360).
    assert_eq!(g.get(entry(AbilityId::Run) + 1, bar).fg, Category::System);
    assert_eq!(
        g.get(entry(AbilityId::Camouflage) + 1, bar).fg,
        Category::Owned
    );
    assert_eq!(g.get(entry(AbilityId::Decoy) + 1, bar).fg, Category::Owned);
}

/// A **passive** on the bar (#264/#287): it reads `Sight(on)` — named like every
/// other entry, marked always-on where an activated ability carries its clock,
/// and in the Owned colour because it is in effect. Undecorated it would have
/// looked exactly like the ready abilities beside it, which is the one thing it
/// is not: there is nothing to press.
#[test]
fn a_held_passive_reads_as_always_on() {
    let s = State::new(
        open_room(40, 10),
        Cell::new(15, 5),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(38, 8),
    )
    .with_loadout(granted_with_passive());
    let g = render_screen(&s, ScreenUi::default());
    let bar = ability_row(10);
    let row: String = (0..g.width()).map(|x| g.get(x, bar).glyph).collect();
    assert_eq!(row, "Run       Camo      Decoy     Sight(on) ", "{row:?}");

    // In effect, so Owned — the same colour as the ready entries it sits beside,
    // with the marker rather than the colour carrying "you cannot press this".
    let sight = row.find("Sight").expect("the passive's entry") as u32;
    // One cell in: `S` is the entry's mnemonic and carries the ink colour (#360).
    assert_eq!(g.get(sight + 1, bar).fg, Category::Owned);
    assert_eq!(
        s.ability_state(AbilityId::Vision),
        AbilityState::Passive,
        "held is on (§8.2/#264)",
    );
    // And it still hit-tests to itself, marker included.
    for x in sight..sight + "Sight(on)".len() as u32 {
        assert_eq!(ability_at(&s, x, bar), Some(AbilityId::Vision), "col {x}");
    }
}

/// **The width budget, end to end** (§11.4/#287). The worst bar a run can ever
/// produce — [`AbilityId::MAX_HELD`] abilities, each the widest entry the catalog
/// allows — is drawn whole on the v1 board: nothing truncated, the right margin
/// intact, and not a cell past the frame's left edge. This is the runtime twin of
/// the `const` assertion on [`MAX_BAR_WIDTH`]; if either ever fails, the other is
/// what tells you why.
#[test]
fn the_widest_possible_bar_fits_the_v1_board() {
    let width = LevelConfig::V1.width;
    assert_eq!(BAR_SLOT, 10, "nine cells of entry, one of air");
    assert_eq!(MAX_BAR_WIDTH, 40, "four slots of ten");
    assert!(
        MAX_BAR_WIDTH <= width,
        "and the board is at least that wide"
    );

    // Every ability in the widest state its own mode can reach — the longest
    // cooling number, or the passive marker — and the worst `MAX_HELD` kept.
    let mut worst: Vec<AbilityStatus> = AbilityId::ALL
        .into_iter()
        .map(|id| AbilityStatus {
            id,
            state: match id.def().mode() {
                AbilityMode::Passive => AbilityState::Passive,
                AbilityMode::Activated(economy) => AbilityState::Cooling {
                    remaining: economy.cooldown(),
                },
            },
        })
        .collect();
    worst.sort_by_key(|s| std::cmp::Reverse(s.bar_entry().chars().count()));
    worst.truncate(AbilityId::MAX_HELD);

    let layout = ability_line_layout(width, &worst);
    assert_eq!(layout.len(), AbilityId::MAX_HELD, "no entry is dropped");
    // Even at their widest, no entry overruns its slot — and the last one still
    // leaves the trailing cell of air at the frame's edge.
    for (i, start) in &layout {
        let len = worst[*i].bar_entry().chars().count() as u32;
        assert!(len <= MAX_BAR_ENTRY as u32, "{:?} fits its slot", worst[*i]);
        assert!(start + len <= width - BAR_GAP, "…inside the row");
    }
    assert_eq!(layout[0].1, 0, "and the four slots fill the row exactly");
}

/// A bar wider than its row **truncates** rather than panicking or wrapping. No
/// legal loadout gets here — [`MAX_BAR_WIDTH`] is asserted against the board at
/// compile time — but a hand-built [`Loadout::full`] state or a narrow test board
/// can, and the deck's last slots are what go.
#[test]
fn an_oversized_bar_drops_its_tail_and_stays_flush_right() {
    let s = State::new(
        open_room(24, 4),
        Cell::new(2, 2),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(22, 2),
    )
    .with_loadout(Loadout::full());
    assert_eq!(
        s.ability_statuses().len(),
        AbilityId::ALL.len(),
        "every ability at once — well over the cap",
    );
    let g = render_screen(&s, ScreenUi::default());
    assert_eq!(
        g.height(),
        TOP_ROWS + 4 + BOTTOM_ROWS,
        "the frame is intact"
    );
    let row: String = (0..g.width())
        .map(|x| g.get(x, ability_row(4)).glyph)
        .collect();
    assert_eq!(row.chars().count(), 24, "exactly one grid row wide");
    // Seven slots need 70 cells and 24 are on offer, so the deck's last five go
    // — and the two that remain keep their full slots, flush right.
    assert_eq!(row, "    Run       Camo      ", "{row:?}");
}

/// The pointer→identity hit-test (§11.4) on the always-on bar: each entry's cells
/// resolve to *that* ability by identity, the gaps and the empty left of the row
/// resolve to nothing, and the map above is not the bar.
#[test]
fn ability_at_resolves_the_bar() {
    let s = State::new(
        open_room(40, 10),
        Cell::new(15, 5),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(38, 8),
    )
    .with_loadout(granted());
    let bar = ability_row(10);

    // Four fixed slots at 0/10/20/30, by identity not position — and the target
    // is the **whole slot**, so the blank cells after a short name hit it too.
    // That is what makes a slot a stable tap target rather than a moving word.
    for (slot, id) in [
        (0, AbilityId::Run),
        (10, AbilityId::Camouflage),
        (20, AbilityId::Decoy),
        (30, AbilityId::Dephase),
    ] {
        for x in slot..slot + MAX_BAR_ENTRY as u32 {
            assert_eq!(ability_at(&s, x, bar), Some(id), "col {x}");
        }
        // …but the trailing cell of air is dead, keeping the targets apart.
        assert_eq!(
            ability_at(&s, slot + MAX_BAR_ENTRY as u32, bar),
            None,
            "the gap after slot {slot} resolves to nothing",
        );
    }
    // The map above the bar is not the bar.
    assert_eq!(ability_at(&s, 0, bar - 1), None, "the row above is map");
    assert_eq!(ability_at(&s, 0, NEAR_ROW), None, "nor is the near line");
}

/// #360's named case, on a real bar: a loadout of Decoy + Doors + Daze — three bar
/// names starting `D` — gives three **distinct** letters, each firing its own
/// ability, none of them a key §11.6 had already bound.
///
/// The letters are the payload, but the assertion that matters is the last one:
/// each letter resolves to the slot whose entry the bar *highlighted*, so what the
/// row promises is what the keyboard does.
#[test]
fn three_names_starting_d_get_three_distinct_working_letters() {
    let held = [AbilityId::Decoy, AbilityId::Autodoors, AbilityId::Confusion];
    let s = State::new(
        open_room(40, 10),
        Cell::new(15, 5),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(38, 8),
    )
    .with_loadout(held.into_iter().fold(Loadout::empty(), Loadout::with));

    let letters: Vec<Option<char>> = (0..held.len())
        .map(|slot| ability_mnemonic(&s, slot))
        .collect();
    assert_eq!(
        letters,
        vec![Some('d'), Some('o'), Some('a')],
        "Decoy keeps `d`; Doors and Daze fall through their own names",
    );

    for (slot, id) in held.into_iter().enumerate() {
        let letter = letters[slot].expect("each of the three claimed a letter");
        assert_eq!(
            ability_slot_for_letter(&s, &letter.to_string()),
            Some(slot),
            "{letter:?} fires slot {slot}",
        );
        assert_eq!(
            ability_in_slot(&s, slot),
            Some(id),
            "…which is where {id:?} is drawn",
        );
        // Uppercase lands on the same slot: a stray Shift must not cost the turn.
        assert_eq!(
            ability_slot_for_letter(&s, &letter.to_uppercase().to_string()),
            Some(slot),
            "{letter:?} fires with Shift held too",
        );
    }
    // A letter nobody claimed fires nothing, and stays the page's.
    for key in ["z", "q", "ArrowUp", "1"] {
        assert_eq!(ability_slot_for_letter(&s, key), None, "{key:?}");
    }
}

/// The mark **is** the binding's announcement (§11.6/#360), so the cell the bar
/// lifts has to be the cell of the letter that fires it — for every entry of a
/// loadout, not just the ones whose initial was free.
#[test]
fn the_marked_cell_is_the_letter_that_fires_it() {
    let held = [
        AbilityId::Run,
        AbilityId::Decoy,
        AbilityId::Autodoors,
        AbilityId::Confusion,
    ];
    let s = State::new(
        open_room(40, 10),
        Cell::new(15, 5),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(38, 8),
    )
    .with_loadout(held.into_iter().fold(Loadout::empty(), Loadout::with));
    let grid = render_screen(&s, ScreenUi::default());
    let bar = ability_row(10);
    let layout = ability_line_layout(40, &s.ability_statuses());

    for (slot, start) in layout.iter().map(|&(_, start)| start).enumerate() {
        let letter = ability_mnemonic(&s, slot).expect("every entry here claims one");
        let state = bar_category(s.ability_statuses()[slot].state);
        // Exactly one cell **of the entry** is lifted to the ink colour. The
        // slot's trailing blanks are Neutral too — they are the row's own filler,
        // not part of the word — so the scan is over the glyphs the entry drew.
        let marked: Vec<u32> = (start..start + MAX_BAR_ENTRY as u32)
            .filter(|&x| {
                let cell = grid.get(x, bar);
                cell.glyph != ' ' && cell.fg == Category::Neutral
            })
            .collect();
        if state == Category::Ground {
            // Autodoors has no door to work in an open room, so its entry is
            // unusable and unmarked — the rule the sibling test pins, asserted
            // here too so this sweep cannot quietly skip an entry.
            assert!(marked.is_empty(), "slot {slot} is unusable and unmarked");
            continue;
        }
        assert_eq!(marked.len(), 1, "slot {slot} marks one cell");
        // …it is the letter that fires the slot…
        let cell = grid.get(marked[0], bar);
        assert_eq!(
            cell.glyph.to_ascii_lowercase(),
            letter,
            "slot {slot} marks the cell of {letter:?}",
        );
        assert_eq!(ability_slot_for_letter(&s, &letter.to_string()), Some(slot));
        // …and nothing is drawn behind it: the mark is the letter's own colour, so
        // the bar stays a quiet strip rather than growing a band (§11.4).
        assert_eq!(
            cell.bg, None,
            "slot {slot} paints no ground under its letter"
        );
        // The rest of the entry still carries the state colour, which is what the
        // mark must not swallow. Sampled off the first glyph that is *not* the
        // marked one — Daze claims `a`, its second character, so "the cell after
        // the start" is not a safe stand-in for "not the mnemonic".
        let plain = (start..start + MAX_BAR_ENTRY as u32)
            .find(|&x| x != marked[0] && grid.get(x, bar).glyph != ' ')
            .expect("an entry is more than its mnemonic");
        assert_eq!(
            grid.get(plain, bar).fg,
            state,
            "slot {slot} still says what state it is in",
        );
    }
}

/// An entry the player **cannot use** keeps its letter dim (§11.6/#360): the ink
/// mark says "press this", so putting one on an entry that is not on offer would
/// pull the eye to exactly the thing to skip. Pierce Wall in open ground has no
/// target, so its whole entry — the `B` included — reads Ground.
#[test]
fn an_unusable_entry_does_not_mark_its_letter() {
    let s = State::new(
        open_room(40, 10),
        Cell::new(15, 5),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(38, 8),
    )
    .with_loadout(Loadout::innate().with(AbilityId::PierceWall));
    let grid = render_screen(&s, ScreenUi::default());
    let bar = ability_row(10);
    let layout = ability_line_layout(40, &s.ability_statuses());
    let (_, bore_start) = layout[1];

    assert_eq!(
        s.ability_state(AbilityId::PierceWall),
        AbilityState::Unusable,
        "nothing to bore in an open room",
    );
    assert_eq!(
        ability_mnemonic(&s, 1),
        Some('b'),
        "it still claims its letter"
    );
    assert_eq!(
        ability_slot_for_letter(&s, "b"),
        Some(1),
        "…and the key still resolves — it refuses for free (§4.4), not silently",
    );
    for x in bore_start..bore_start + "Bore".len() as u32 {
        assert_eq!(
            grid.get(x, bar).fg,
            Category::Ground,
            "the whole entry recedes, mnemonic included (col {x})",
        );
    }
    // Run, beside it, is usable — so its letter *is* marked. Same frame, so this
    // is the contrast the rule is made of rather than a second run's.
    assert_eq!(grid.get(layout[0].1, bar).fg, Category::Neutral);
}

/// #359's binding, against the row it is a binding *on*: a **three**-ability run
/// answers `1`, `2` and `3` — each firing the ability whose entry the bar drew at
/// that slot — and `4` fires nothing at all, because the run has no fourth entry.
///
/// The digits count the row **as drawn**, which is the trap the ticket named: the
/// bar is flush right (#267), so a short loadout starts well in from the left edge
/// and a slot counted from the catalogue instead would leave `1` dead. The
/// assertion below reads the id straight out of the drawn cells to keep the count
/// honest.
#[test]
fn a_three_ability_loadout_answers_one_two_three_and_ignores_four() {
    let held = [AbilityId::Run, AbilityId::Camouflage, AbilityId::Decoy];
    let s = State::new(
        open_room(40, 10),
        Cell::new(15, 5),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(38, 8),
    )
    .with_loadout(held.into_iter().fold(Loadout::empty(), Loadout::with));
    let grid = render_screen(&s, ScreenUi::default());
    let bar = ability_row(10);

    // Three entries on a 40-wide row, flush right: the first is drawn at column 10
    // and nothing at all is drawn at 0 — the cell a catalogue-counted `1` would
    // have fired.
    let layout = ability_line_layout(40, &s.ability_statuses());
    assert_eq!(
        layout.iter().map(|&(_, x)| x).collect::<Vec<_>>(),
        vec![10, 20, 30],
        "a short bar sits away from the left edge",
    );
    assert_eq!(grid.get(0, bar).glyph, ' ', "…leaving the left edge blank");

    for (slot, id) in held.into_iter().enumerate() {
        assert_eq!(
            ability_in_slot(&s, slot),
            Some(id),
            "the {} key fires {id:?}",
            slot + 1,
        );
        // …and that is the entry the row drew there: the slot's first cells spell
        // its bar name.
        let start = layout[slot].1;
        let drawn: String = (0..id.bar_name().chars().count() as u32)
            .map(|i| grid.get(start + i, bar).glyph)
            .collect();
        assert_eq!(
            drawn,
            id.bar_name(),
            "slot {} draws what it fires",
            slot + 1
        );
        // The tap on that same cell agrees, which is the seam both go through.
        assert_eq!(ability_at(&s, start, bar), Some(id));
    }

    // `4` is a key this run has no entry for: nothing to fire, so nothing happens.
    assert_eq!(
        ability_in_slot(&s, 3),
        None,
        "a digit past the held count fires nothing",
    );
}

/// The click **is** the key (§11.4/§11.6): a bar cell and that entry's digit
/// resolve through the one [`ability_in_slot`] seam to the same id, and both hand
/// it to the one `State::ability_input` toggle (#304) — so a click activates a
/// ready ability and, on a cooling one, refuses for free with no turn spent
/// (§4.4), exactly as the key.
#[test]
fn a_click_activates_by_the_same_path_as_the_digit() {
    let mut s = State::new(
        open_room(40, 10),
        Cell::new(15, 5),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(38, 8),
    )
    .with_loadout(granted());
    let bar = ability_row(10);

    // The bar's first slot resolves to the same id `1` fires — one path, one seam.
    let clicked = ability_at(&s, 0, bar).expect("Run under the pointer");
    assert_eq!(
        ability_in_slot(&s, 0),
        Some(clicked),
        "the click and the digit resolve to the same ability",
    );
    assert_eq!(
        s.ability_input(clicked),
        Input::Activate(clicked),
        "a ready ability switches on from either",
    );

    // A click on a ready ability activates it (a spent turn).
    let events = s.step(s.ability_input(clicked));
    assert_eq!(s.turn(), 1, "activating spends the turn");
    assert!(!events.is_empty(), "the ability activated");

    // The entry is `Run[4]` now, and the bar is a projection of the keys
    // (§11.4/#304): tapping it switches the sprint off, exactly as pressing `r`
    // again does. The tap resolving to `Activate` here was the whole of #304 —
    // there was no reachable way to stop a sprint.
    let active = ability_at(&s, 0, bar).expect("Run still under the pointer");
    assert_eq!(
        s.ability_input(active),
        Input::Deactivate(AbilityId::Run),
        "an active entry is the toggle-off, from tap and key alike",
    );

    // Drive Run to cooling, then a click on its (now cooling) entry refuses
    // cleanly: the same `Input::Activate` is a free no-op — no turn, no change.
    s.step(s.ability_input(active));
    assert!(matches!(
        s.ability_state(AbilityId::Run),
        AbilityState::Cooling { .. }
    ));
    // The entry widened to `Run/12/` inside its slot — which did not move, so
    // the very same cell is still Run (#287).
    let cooling = ability_at(&s, 0, bar).expect("Run still under the pointer");
    assert_eq!(
        s.ability_input(cooling),
        Input::Activate(cooling),
        "a cooling entry is still an activation — the one that refuses",
    );
    let turn_before = s.turn();
    let refused = s.step(s.ability_input(cooling));
    assert!(refused.is_empty(), "a cooling entry refuses");
    assert_eq!(s.turn(), turn_before, "the mis-click spends no turn");
}

/// A message longer than the row truncates at the edge instead of
/// panicking or wrapping — the status rows are single grid rows.
#[test]
fn a_long_status_line_truncates_at_the_edge() {
    let mut s = State::new(
        open_room(12, 6),
        Cell::new(2, 2),
        Direction::North,
        Vec::new(),
        [Cell::new(3, 2)],
        Cell::new(10, 4),
    );
    s.step(Input::Step(Direction::East)); // take the intel: a long message
    let g = render_screen(&s, ScreenUi::default());
    let near: String = (0..g.width()).map(|x| g.get(x, NEAR_ROW).glyph).collect();
    assert_eq!(near.chars().count(), 12, "exactly one grid row wide");
    assert!(
        near.starts_with("[?] "),
        "the [?] owns the row's left end, then a cell of air: {near:?}"
    );
    assert!(
        near.ends_with(" "),
        "…and the frame's right margin survives the words: {near:?}"
    );
}

// --- Help panel (§14 v2/#139/#248) ---------------------------------------

/// A plain full board to render the help panel over.
fn help_board() -> State {
    State::new(
        open_room(40, 40),
        Cell::new(10, 10),
        Direction::North,
        vec![Guard::stationary(Cell::new(20, 20))],
        Vec::new(),
        Cell::new(30, 30),
    )
}

/// §4.4/#139/#248: opening help is a pure view toggle. It changes the frame
/// while up, and **closing restores the exact frame** — the panel writes no
/// state, so the frame beneath is byte-identical before and after. The open
/// frame is the full screen, the game frame's exact size.
#[test]
fn help_is_a_modal_full_screen_frame_and_closing_restores_it() {
    let s = help_board();
    let closed = render_screen(&s, ScreenUi::default());
    let open = render_screen(
        &s,
        ScreenUi {
            help_open: true,
            ..ScreenUi::default()
        },
    );
    assert_ne!(open, closed, "the help panel changes the frame while up");
    // The panel is the full screen — the same width and height as the game frame.
    assert_eq!(open.width(), closed.width());
    assert_eq!(open.height(), closed.height());
    let reclosed = render_screen(&s, ScreenUi::default());
    assert_eq!(reclosed, closed, "closing restores the identical frame");
}

/// The open frame **is** the modal panel (#248): the whole screen, not an
/// overlay on the map. Row 0 is the tab bar — the tab labels and the `[x]` close
/// — not the game's ability line, and the run's active modifiers show through
/// from `state.modifiers()`.
#[test]
fn the_open_frame_is_the_panel_showing_the_run_modifiers() {
    // A run carrying one harder modifier, threaded as the real game would.
    let s = help_board().with_modifiers(LevelModifiers {
        guards_always_search_hideouts: true,
        ..LevelModifiers::default()
    });
    let g = render_screen(
        &s,
        ScreenUi {
            help_open: true,
            ..ScreenUi::default()
        },
    );
    let row0: String = (0..g.width()).map(|x| g.get(x, 0).glyph).collect();
    // The tab bar, not the near line: both tabs and the close control.
    assert!(
        row0.contains("[Level info]"),
        "the tab bar heads the panel: {row0:?}"
    );
    assert!(row0.contains("[Abilities]") && row0.contains("[Help]"));
    assert!(row0.contains("[x]"), "a touchable close control");
    assert!(
        !row0.contains("intel remaining"),
        "the game's own near line is gone while modal"
    );
    // The default tab is Level info, so the active modifier reads through.
    let text = g.to_text().join("\n");
    assert!(
        text.contains("Guards search hideouts"),
        "the run's modifier shows"
    );
}

/// #272, end to end: a **booted** run's help panel shows that run's own
/// level-seed token — the whole chain, `start_level` → `State::level` →
/// `render_screen` → the Level info tab — and looking at it is still free: no
/// turn, no state written, the frame beneath byte-identical afterwards (§4.4).
#[test]
fn the_help_panel_of_a_booted_run_shows_its_seed_for_free() {
    use crate::level_seed::{start_level, LevelSeed};

    let level = LevelSeed::quick_play(8371);
    let s = start_level(&level).expect("the v1 recipe places");
    let before = s.turn();
    let closed = render_screen(&s, ScreenUi::default());
    let open = render_screen(
        &s,
        ScreenUi {
            help_open: true,
            ..ScreenUi::default()
        },
    );
    let text = open.to_text().join("\n");
    assert!(text.contains("LEVEL SEED"), "the section is there");
    assert!(
        text.contains(&level.encode().expect("a config a run can hold")),
        "…showing this run's own token, in full"
    );
    assert_eq!(s.turn(), before, "looking costs no turn");
    assert_eq!(
        render_screen(&s, ScreenUi::default()),
        closed,
        "and writes no state"
    );
}

/// §11.6's no-trap rule, kept for the full-screen panel (#248): with the near
/// line's `[?]` now covered, the panel carries its own escape — the `[x]` close control
/// hit-tests to [`HelpHit::Close`], and each tab tap switches — while the near
/// line's `[?]` still opens it when the panel is closed.
#[test]
fn the_panel_is_reachable_to_open_and_escapable_once_open() {
    let s = help_board();
    let width = s.layout().facility().width();

    // Closed: the near line's `[?]` opens the panel (its hit-test, and it is
    // drawn).
    assert!(is_help_button(0, NEAR_ROW), "the [?] cell hit-tests");
    let closed = render_screen(&s, ScreenUi::default());
    let near: String = (0..width).map(|x| closed.get(x, NEAR_ROW).glyph).collect();
    assert!(
        near.contains("[?]"),
        "closed: the near line offers [?]: {near:?}"
    );

    // Open: the panel is escapable by touch — the `[x]` closes, a tab switches.
    let height = closed.height;
    assert!(matches!(
        help_hit(
            width,
            height,
            HelpTab::default(),
            s.level(),
            false,
            width - 2,
            0
        ),
        Some(HelpHit::Close)
    ));
    assert!(matches!(
        help_hit(width, height, HelpTab::default(), s.level(), false, 2, 0),
        Some(HelpHit::Tab(_))
    ));
}

/// The `[?]` toggle is the near line's alone (§11.4/#139/#267): it hit-tests on
/// the top row and nowhere else, so the bar's own right-hand corner — the same
/// columns, the frame's last row — can never swallow a tap meant for the board's
/// bottom-right, nor the other way round.
#[test]
fn the_help_toggle_belongs_to_the_near_line_only() {
    let height = TOP_ROWS + 40 + BOTTOM_ROWS;
    let bar = height - BOTTOM_ROWS;
    for x in 0..HELP_BUTTON_LEN {
        assert!(is_help_button(x, NEAR_ROW));
        assert!(!is_help_button(x, bar), "not on the bar's row at {x}");
    }
}

/// #268: the title screen takes the **whole** frame and takes it *first* — the
/// game's chrome does not show through, not even the help panel a stale
/// `help_open` would otherwise draw. It is the board's own size, so starting a
/// run swaps what is drawn without moving the fit, and it writes no state:
/// clearing it restores the identical frame.
#[test]
fn the_menu_replaces_the_whole_frame_and_leaves_it_untouched() {
    let s = help_board();
    let playing = render_screen(&s, ScreenUi::default());
    let menu = render_screen(
        &s,
        ScreenUi {
            menu: Some(MenuUi::default()),
            // Set alongside every other overlay: the menu still wins outright.
            help_open: true,
            message_log_open: true,
            ..ScreenUi::default()
        },
    );
    assert_ne!(menu, playing);
    assert_eq!(
        (menu.width(), menu.height()),
        (playing.width(), playing.height())
    );
    assert!(
        menu.to_text()
            .join("\n")
            .contains(MenuEntry::QuickPlay.label()),
        "the frame is the menu, not the help panel behind it",
    );
    assert_eq!(
        render_screen(&s, ScreenUi::default()),
        playing,
        "leaving the menu restores the identical frame",
    );
}

/// #473: **what outlives a run, named one by one.** The theme a player picks on
/// the title screen has to be the theme the run opens in — it is a fact about
/// their eyes, not about the facility — and so does the modality and the build's
/// replay offer; everything else is the last screen's and must go.
///
/// The result is destructured field-by-field rather than compared against a
/// hand-built `ScreenUi`, so adding a field to the struct fails to compile *here*
/// until someone answers the only question that matters about it: does a fresh
/// facility keep it? That is the check the theme itself never got (#189).
#[test]
fn a_fresh_run_keeps_the_player_and_the_build_and_drops_the_rest() {
    // Every field off its default, so nothing can pass by accident.
    let carried = ScreenUi {
        message_log_open: true,
        help_open: true,
        menu: Some(MenuUi::default()),
        help_tab: HelpTab::Abilities,
        theme: Theme::default().toggled(),
        seed_copy: SeedCopy::Copied,
        offer_replay_copy: true,
        end: EndUi {
            options: RunOptions {
                mode: RunMode::Campaign,
                difficulty: Difficulty::Harder,
            },
            selected: EndExit::NewRun,
        },
        modality: InputModality::Touch,
    };

    let ScreenUi {
        // Kept — the player's, and the build's.
        modality,
        theme,
        offer_replay_copy,
        // Dropped — the last screen's.
        message_log_open,
        help_open,
        menu,
        help_tab,
        seed_copy,
        end,
    } = carried.for_fresh_run();

    assert_eq!(modality, carried.modality, "the player's hands (§11.6)");
    assert_eq!(theme, carried.theme, "the player's eyes (§11.2)");
    assert!(offer_replay_copy, "the build's own offer (#411)");

    assert!(
        !message_log_open,
        "the last run's messages are not this run's"
    );
    assert!(
        !help_open,
        "a run opens on the board, not on the help panel"
    );
    assert!(menu.is_none(), "the run replaces the title screen");
    assert_eq!(help_tab, HelpTab::default());
    assert_eq!(
        seed_copy,
        SeedCopy::default(),
        "a new token, unacknowledged"
    );
    assert_eq!(end, EndUi::default(), "no verdict has been reached yet");
}

/// The carry is **idempotent and total**: a default view state comes back
/// unchanged, and a run started from the end screen of a run started from the menu
/// still opens in the theme chosen before any of it (#473). Chaining is the real
/// path — quick play, end screen, *new run* — and each hop must not shed a little
/// more.
#[test]
fn the_theme_survives_run_after_run() {
    assert_eq!(ScreenUi::default().for_fresh_run(), ScreenUi::default());

    let chosen = ScreenUi {
        theme: Theme::default().toggled(),
        ..ScreenUi::default()
    };
    let mut ui = chosen;
    for _ in 0..3 {
        ui = ui.for_fresh_run();
        assert_eq!(ui.theme, chosen.theme);
    }
}
