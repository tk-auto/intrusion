use super::*;
use crate::ability::AbilityId;
use crate::ability::Loadout;
use crate::alert::{AlertEffect, AlertTrigger, AlertTuning};
use crate::modifiers::{
    ActiveModifier, CacheCount, GuardCount, IntelCount, IntelGate, LayoutKnowledge,
};

/// A full-screen frame the size of the v1 board's screen (§10.2) — wide enough
/// that no row truncates, so a test can read the panel's content whole.
pub(super) const W: u32 = 40;
pub(super) const H: u32 = 43; // TOP_ROWS + 40 + BOTTOM_ROWS

pub(super) fn text_of(grid: &Grid) -> String {
    grid.to_text().join("\n")
}

/// One row of a rendered panel as text — what a reader would see on that line.
fn row_text(grid: &Grid, y: u32) -> String {
    grid.to_text()[y as usize].clone()
}

/// A facility that has not noticed you (§7.3) — the readout most of these tests
/// want, since every section but the alert one is the same at any rung. The tests
/// that *are* about the rung build their own.
pub(super) fn quiet_alert() -> AlertReadout {
    AlertReadout {
        rung: 0,
        effects: Vec::new(),
    }
}

/// The view state a test renders the panel under: the tab up and the copy
/// acknowledgement to print — the two [`ScreenUi`] fields the panel reads, spelled
/// positionally so a test says both where it renders.
pub(super) fn show(tab: HelpTab, copy: SeedCopy) -> ScreenUi {
    ScreenUi {
        help_tab: tab,
        seed_copy: copy,
        ..ScreenUi::default()
    }
}

/// The run a test draws the panel *about* — the facts the tabs read, in the order
/// they were passed one by one before [`PanelRun`] bundled them.
pub(super) fn panel_run<'a>(
    level: Option<LevelSeed>,
    modifiers: LevelModifiers,
    alert: &'a AlertReadout,
    loadout: Loadout,
) -> PanelRun<'a> {
    PanelRun {
        level,
        modifiers,
        alert,
        bar: loadout.iter().collect(),
    }
}

/// One tab of a baseline run's panel, at the v1 screen size — the shape most
/// tests want, including the [`abilities`] tab's, which is why it is
/// `pub(super)` rather than local.
pub(super) fn render_tab(tab: HelpTab, loadout: Loadout) -> Grid {
    render_help(
        W,
        H,
        show(tab, SeedCopy::default()),
        panel_run(None, LevelModifiers::default(), &quiet_alert(), loadout),
    )
}

/// The glyph legend is **derived**, not hand-copied (§11.3): every terrain row's
/// glyph and category equal the source table's, so an edit to [`Terrain::glyph`]
/// or [`Terrain::category`] moves the legend with it. The entity rows show the
/// same constants the world render draws.
#[test]
fn the_glyph_legend_matches_the_render_source() {
    let rows = glyph_rows();
    // Entity rows use the render constants.
    assert!(rows
        .iter()
        .any(|&(g, c, _)| g == PLAYER_GLYPH && c == Category::Owned));
    assert!(rows.iter().any(|&(g, _, _)| g == GUARD_GLYPH));
    assert!(rows.iter().any(|&(g, _, _)| g == BODY_GLYPH));
    assert!(rows
        .iter()
        .any(|&(g, c, _)| g == FLOOR_DOT && c == Category::Ground));
    // Terrain rows equal the §10.3 source exactly.
    for t in [
        Terrain::Wall,
        Terrain::DoorPanelClosed,
        Terrain::DoorHinge,
        Terrain::Hideout,
        Terrain::PartialCover,
        Terrain::DuctEntry,
        Terrain::Console,
        Terrain::CommsConsole,
        Terrain::Exit,
    ] {
        assert!(
            rows.iter()
                .any(|&(g, c, _)| g == t.glyph() && c == t.category()),
            "the legend must carry {t:?} exactly as the terrain table draws it",
        );
    }
}

/// **No row of the card clips.** Every column of the Help tab is measured in
/// *cells* against the v1 board's right margin — the glyph meanings, the colour
/// names and meanings, and both control columns.
///
/// The `const` guard above covers the colour key in bytes, which is conservative
/// but blind to the em-dashes the glyph rows carry ("cupboard — bump to hide"),
/// so this is the exact check over everything the card draws. It is the guard
/// `Sensed` and `Effect` did not have: they shipped clipped mid-word as
/// `guard or door, felt throug` and `what your gadget did, and `, and nothing
/// failed — [`draw`] truncates in silence, which is why a bound has to exist
/// somewhere that does not.
#[test]
fn no_row_of_the_help_card_is_clipped() {
    let fits = |text: &str, start: u32, what: &str| {
        let room = (LevelConfig::V1.width - start - 1) as usize;
        assert!(
            text.chars().count() <= room,
            "{what} {text:?} is {} cells and its column has {room}",
            text.chars().count(),
        );
    };
    for (_, _, meaning) in glyph_rows() {
        fits(meaning, GLYPH_MEANING_X, "glyph meaning");
    }
    for category in CATEGORIES {
        fits(
            category_meaning(category),
            COLOUR_MEANING_X,
            "colour meaning",
        );
        // A name has to clear the meaning column beside it, not just the margin.
        fits(category_name(category), COLOUR_MEANING_X, "colour name");
        let name_end = CONTENT_INDENT + category_name(category).chars().count() as u32;
        assert!(
            name_end < COLOUR_MEANING_X,
            "{category:?}'s name runs into the meaning beside it",
        );
    }
    for (keys, action) in control_rows() {
        fits(&keys, CONTROL_KEYS_X, "control keys");
        fits(&action, CONTROL_ACTION_X, "control action");
    }
}

/// Every colour category has a meaning *and* a name in the key — an exhaustive
/// match guarantees the meaning, and the name list must stay complete too.
#[test]
fn every_category_is_documented() {
    assert_eq!(CATEGORIES.len(), 10, "all ten §11.2 categories are keyed");
    for &c in &CATEGORIES {
        assert!(!category_meaning(c).is_empty(), "{c:?} has a meaning");
        assert!(!category_name(c).is_empty(), "{c:?} has a name");
    }
}

/// The controls card keeps only the **standing** shortcuts (#296): the six rows
/// that are true of every run, and no *named* ability. The ability row earns its
/// place by naming the keys rather than what they fire (#359) — the pairing is the
/// Abilities tab's job, since it changes with the loadout. It documents its own
/// keys, too.
#[test]
fn the_control_rows_are_the_standing_shortcuts_only() {
    let rows = control_rows();
    for action in [
        "move",
        "wait & sense",
        "abilities",
        "messages",
        "this help",
        "colour theme",
    ] {
        assert!(
            rows.iter().any(|(_, a)| a == action),
            "the controls list {action:?}",
        );
    }
    assert_eq!(rows.len(), 6, "and nothing else — no per-ability rows");
    // The panel's own two keys document themselves.
    assert!(rows.iter().any(|(k, _)| *k == HELP_KEY.to_string()));
    assert!(rows.iter().any(|(k, _)| *k == THEME_KEY.to_string()));
}

/// **Nothing on the Legend varies with the run** (#296) — no ability name, no bar
/// name, no ability key pairing anywhere on the tab, whatever the loadout. That is
/// what makes it a legend rather than a per-run card, and it is what keeps the
/// Abilities tab (#343) the single place a loadout-derived ability list is drawn.
#[test]
fn no_ability_reaches_the_help_tab() {
    for loadout in [Loadout::full(), Loadout::innate(), Loadout::empty()] {
        let text = text_of(&render_tab(HelpTab::Help, loadout));
        for id in AbilityId::ALL {
            assert!(
                !text.contains(id.name()),
                "{} is on the Abilities tab, not the Legend",
                id.name(),
            );
            for slot in 0..AbilityId::MAX_HELD {
                assert!(
                    !text.contains(&format!("{} / {}", slot + 1, id.bar_name())),
                    "{}'s key pairing is on the Abilities tab, not the Legend",
                    id.name(),
                );
            }
        }
    }
}

/// The keys column has a width budget like everything else on a 40-wide board
/// (§11.4): it runs until the action column starts, and every row the card draws
/// has to leave a gutter rather than run into the action beside it.
#[test]
fn the_widest_control_row_clears_the_action_column() {
    let column = (CONTROL_ACTION_X - CONTROL_KEYS_X) as usize;
    for (keys, action) in control_rows() {
        assert!(
            keys.chars().count() < column,
            "{keys:?} is {} cells and the keys column has {column}, gutter included",
            keys.chars().count(),
        );
        assert!(
            CONTROL_ACTION_X as usize + action.chars().count() < LevelConfig::V1.width as usize,
            "{action:?} runs past the board's right margin",
        );
    }
}

/// The **Legend** tab still carries the whole reference card — the three
/// sections and a glyph derived from the real terrain table (the duct `=`, §10.7).
#[test]
fn the_help_tab_carries_the_glyphs_colours_and_controls() {
    let text = text_of(&render_tab(HelpTab::Help, Loadout::innate()));
    assert!(text.contains("GLYPHS") && text.contains("COLOURS") && text.contains("CONTROLS"));
    for glyph in [Terrain::DuctEntry.glyph(), Terrain::Exit.glyph(), '}', '$'] {
        assert!(text.contains(glyph), "the legend shows {glyph:?}");
    }
    for keys in ["arrows / num", "w / num 5 / .", "1234"] {
        assert!(text.contains(keys), "the controls show {keys:?}");
    }
    // The Legend tab is not the Level-info tab: its modifier section is elsewhere.
    assert!(
        !text.contains("MODIFIERS"),
        "MODIFIERS lives on the other tab"
    );
}

/// The **Level info** tab lists the run's active modifiers by name, and a
/// baseline run reads clearly as "none active" (#248). The rows are derived from
/// [`LevelModifiers::active`], so the tab cannot drift from the resolved set.
#[test]
fn the_level_info_tab_lists_active_modifiers_or_none() {
    // Baseline: "none active", not blank.
    let baseline = render_help(
        W,
        H,
        show(HelpTab::LevelInfo, SeedCopy::default()),
        panel_run(
            None,
            LevelModifiers::default(),
            &quiet_alert(),
            Loadout::innate(),
        ),
    );
    let text = text_of(&baseline);
    assert!(text.contains("THIS RUN") && text.contains("MODIFIERS"));
    assert!(
        text.contains("none active"),
        "baseline reads none active: still legible"
    );

    // A harder toggle and the harder knob both surface, by name — and, for the
    // knob, its value.
    let modified = LevelModifiers {
        guards_always_search_hideouts: true,
        intel_to_exit: IntelGate::All,
        ..LevelModifiers::default()
    };
    let g = render_help(
        W,
        H,
        show(HelpTab::LevelInfo, SeedCopy::default()),
        panel_run(None, modified, &quiet_alert(), Loadout::innate()),
    );
    let text = text_of(&g);
    assert!(
        !text.contains("none active"),
        "an active run does not read none"
    );
    assert!(text.contains("Guards search hideouts"));
    assert!(
        text.contains("Intel to exit: all of it"),
        "a bounded knob renders its value: {text:?}"
    );
}

/// #248: **every** caption the card can draw fits the board it is drawn on,
/// with every modifier at once. The compile-time bound (`CAPTION_MAX`) already
/// makes an over-long caption a build failure; this is its runtime companion —
/// it renders the real panel and checks no row was clipped, so the bound is
/// tied to an actual frame rather than to arithmetic that could drift from the
/// layout. (Regression: "Sightings called in: one guard converges" was drawn
/// as `…one guard conver` on the v1 board.)
#[test]
fn no_modifier_caption_is_clipped_on_the_board() {
    // Every toggle on, and each knob at one of its non-baseline values, so every
    // caption in `CAPTIONS` is exercised across the two renders.
    for (gate, guard_count, intel_count, caches) in [
        (
            IntelGate::All,
            GuardCount::More,
            IntelCount::More,
            CacheCount::One,
        ),
        (
            IntelGate::None,
            GuardCount::Fewer,
            IntelCount::Fewer,
            CacheCount::Two,
        ),
        (
            IntelGate::None,
            GuardCount::Fewer,
            IntelCount::Fewer,
            CacheCount::Three,
        ),
    ] {
        let all_on = LevelModifiers {
            guards_always_search_hideouts: true,
            sighting_lost_calls_a_guard: true,
            body_found_calls_two_guards: true,
            always_show_vision_cones: true,
            layout_knowledge: LayoutKnowledge::Full,
            calm_guards_detect_only_their_cone: true,
            automatic_doors: true,
            guards_watch_consoles: true,
            show_search_areas: true,
            guard_count,
            intel_count,
            caches,
            prize_room_locked: true,
            narrowed_guard_cones: true,
            intel_to_exit: gate,
        };
        let g = render_help(
            W,
            H,
            show(HelpTab::LevelInfo, SeedCopy::default()),
            panel_run(None, all_on, &quiet_alert(), Loadout::innate()),
        );
        let text = text_of(&g);
        for m in all_on.active() {
            let caption = match m.detail {
                Some(d) => format!("{}: {}", m.name, d),
                None => m.name.to_string(),
            };
            assert!(
                text.contains(&caption),
                "caption {caption:?} was clipped on a {W}-wide board",
            );
        }
    }
    // And the bound itself is not vacuously large: a caption may not run past
    // the board's last column once indented.
    for m in CAPTIONS {
        assert!(
            CONTENT_INDENT as usize + m.caption_len() < W as usize,
            "{:?} does not fit the board with its indent",
            m.name,
        );
    }
}

/// The **level-seed token** on the Level info tab (§13.1/#245/#272): the run's
/// own token, drawn under its heading — and it **decodes back to the very run
/// showing it**, config and all, so the panel can never hand out a string that
/// boots a different game. There is one form now (#333), so what the player
/// reads here is character-for-character what a shared link carries: the panel
/// and the address bar can no longer disagree about what the run is.
#[test]
fn the_level_info_tab_shows_a_token_that_decodes_to_this_run() {
    for level in [
        // The default preset.
        LevelSeed::quick_play(8371),
        // A run carrying a chosen modifier set and loadout.
        LevelSeed {
            seed: 8371,
            modifiers: LevelModifiers {
                always_show_vision_cones: true,
                ..LevelModifiers::default()
            },
            abilities: Loadout::innate(),
        },
    ] {
        let g = render_help(
            W,
            H,
            show(HelpTab::LevelInfo, SeedCopy::default()),
            panel_run(
                Some(level),
                level.modifiers,
                &quiet_alert(),
                Loadout::innate(),
            ),
        );
        let text = text_of(&g);
        let token = level.encode().expect("a config a run can hold");
        assert!(text.contains("LEVEL SEED"), "the section is labelled");
        assert!(text.contains(&token), "the token is shown: {text:?}");
        // The round trip: what a player reads off the panel boots this run.
        assert_eq!(
            LevelSeed::decode(&token),
            Some(level),
            "the displayed token reproduces the run exactly"
        );
    }

    // The default preset is spelled out like any other config — it no longer
    // collapses to a bare seed, which named the preset rather than the run
    // (#333, superseding #328). This assertion is the reverse of the pin that
    // recorded the old decision, and is here to record the new one.
    let quick = LevelSeed::quick_play(8371);
    let token = quick.encode().expect("a config a run can hold");
    assert_ne!(token, "8371", "the link form is no longer a bare seed");
    assert_eq!(
        token.len(),
        crate::level_seed::TOKEN_LEN,
        "one fixed-width form"
    );
    let g = render_help(
        W,
        H,
        show(HelpTab::LevelInfo, SeedCopy::default()),
        panel_run(
            Some(quick),
            quick.modifiers,
            &quiet_alert(),
            Loadout::innate(),
        ),
    );
    assert!(
        text_of(&g).contains(&token),
        "quick play shows the same token it shares",
    );

    // A hand-built state has no reproducible token, so the section is absent
    // rather than showing a string that boots something else.
    let none = render_help(
        W,
        H,
        show(HelpTab::LevelInfo, SeedCopy::default()),
        panel_run(
            None,
            LevelModifiers::default(),
            &quiet_alert(),
            Loadout::innate(),
        ),
    );
    assert!(!text_of(&none).contains("LEVEL SEED"));
}

/// #375/§2.2: the Level info tab carries the **facility alert** — the rung, and the
/// retaliation it has in force. Without it the ladder is perceptible for exactly one
/// turn (the near line's step message, overwritten by anything louder, §11.7) and
/// inert after that.
///
/// The section is drawn at **every** rung, rung 0 included: a heading that appeared
/// out of nowhere the turn you were first seen would teach the ladder exists at the
/// moment that knowledge stopped being useful, and a row that vanishes reads as a
/// bug rather than as a fact.
#[test]
fn the_level_info_tab_shows_the_alert_rung_and_what_it_is_doing() {
    let panel = |alert: &AlertReadout| {
        text_of(&render_help(
            W,
            H,
            show(HelpTab::LevelInfo, SeedCopy::default()),
            panel_run(None, LevelModifiers::default(), alert, Loadout::innate()),
        ))
    };

    let quiet = panel(&quiet_alert());
    assert!(quiet.contains("ALERT"), "the section is always there");
    assert!(
        quiet.contains(crate::alert::NO_ALERT),
        "a quiet facility says so rather than showing a blank: {quiet}",
    );
    assert!(
        !quiet.contains("Condition"),
        "…and claims no condition it has not reached",
    );

    // A raised rung names itself and lists the effects the ladder actually runs —
    // the numbers included, so "never calm" is a rule the player can plan against
    // rather than a mood.
    let raised = panel(&AlertReadout {
        rung: 2,
        effects: vec![AlertEffect {
            rung: 1,
            name: crate::alert::NEVER_CALM,
            detail: Some("pause 1–3 turns".to_string()),
        }],
    });
    assert!(raised.contains("Condition 2 of 3"), "{raised}");
    assert!(
        raised.contains("Guards never calm: pause 1–3 turns"),
        "{raised}"
    );
    assert!(
        !raised.contains(crate::alert::NO_ALERT),
        "and never both at once",
    );
}

/// §11.4's row-fits rule (#248's `CAPTION_MAX`, applied to the alert rows): every
/// row the ALERT section can draw fits the v1 board's content column. [`draw`] clips
/// in silence, and the effect rows carry **runtime** numbers off the live
/// [`AlertTuning`] — so unlike the modifier captions they cannot be bounded at
/// compile time, and this walks the real ladder instead of trusting them.
///
/// Walked over a **deliberately wide** tuning as well as the shipped one: two-digit
/// dwell numbers are legal (`validate` allows them) and are exactly what a §13.2
/// sweep would produce, so the row has to fit at the widest the ladder permits, not
/// only at its [START].
#[test]
fn no_row_of_the_alert_section_is_clipped() {
    let room = column_width(CONTENT_INDENT);
    assert!(
        crate::alert::NO_ALERT.chars().count() <= room,
        "the rung-0 row is too wide for the Level info column",
    );
    for tuning in [
        AlertTuning::default(),
        AlertTuning {
            dwell_turns_min: 98,
            dwell_turns_max: 99,
            rung_two_reinforcements: 98,
            rung_three_reinforcements: 99,
            ..AlertTuning::default()
        },
    ] {
        let mut alert = crate::alert::Alert::new();
        alert.set_tuning(tuning);
        for trigger in AlertTrigger::ALL {
            alert.raise(trigger);
            let readout = alert.readout();
            // Read from the drawing's own helper, so the row measured here is the
            // row the panel draws.
            let condition = super::super::alert::condition_line(readout.rung);
            assert!(condition.chars().count() <= room, "{condition:?}");
            for effect in &readout.effects {
                let text = match &effect.detail {
                    Some(detail) => format!("{}{CAPTION_SEPARATOR}{detail}", effect.name),
                    None => effect.name.to_string(),
                };
                assert!(
                    text.chars().count() <= room,
                    "the alert row {text:?} is {} cells and its column has {room}",
                    text.chars().count(),
                );
            }
        }
    }
}

/// The seed section does not disturb the modifier list it heads (#272): with a
/// token shown, the run's modifiers still render by name in their cue colour,
/// just two rows lower.
#[test]
fn the_seed_section_shifts_the_modifier_list_without_changing_it() {
    let level = LevelSeed {
        seed: 8371,
        modifiers: LevelModifiers {
            guards_always_search_hideouts: true,
            ..LevelModifiers::default()
        },
        abilities: Loadout::innate(),
    };
    let g = render_help(
        W,
        H,
        show(HelpTab::LevelInfo, SeedCopy::default()),
        panel_run(
            Some(level),
            level.modifiers,
            &quiet_alert(),
            Loadout::innate(),
        ),
    );
    let text = text_of(&g);
    assert!(text.contains("Guards search hideouts"));
    assert!(!text.contains("none active"));
    // THIS RUN@2, LEVEL SEED@4, the token@5, MODIFIERS@7, the first row@8.
    let token = level.encode().expect("a config a run can hold");
    assert_eq!(
        g.get(3, 5).glyph,
        token.chars().next().expect("a token has letters"),
        "the token sits under its heading",
    );
    assert_eq!(g.get(3, 5).fg, Category::Interest);
    assert_eq!(g.get(3, 8).glyph, 'G');
    assert_eq!(
        g.get(3, 8).fg,
        Category::Warning,
        "the caption keeps its direction cue"
    );
}

/// The modifier's **caption** is drawn in its direction's cue colour (§11.2/#248):
/// Warning for a harder rule, Owned for an easier one — so the direction reads at
/// a glance, and the colours come from the standing categories, not ad-hoc styling.
#[test]
fn the_caption_reads_in_its_direction_cue_colour() {
    // A harder toggle: its caption `Guards search hideouts` is drawn in Warning.
    let harder = LevelModifiers {
        guards_always_search_hideouts: true,
        ..LevelModifiers::default()
    };
    let g = render_help(
        W,
        H,
        show(HelpTab::LevelInfo, SeedCopy::default()),
        panel_run(None, harder, &quiet_alert(), Loadout::innate()),
    );
    // The MODIFIERS heading is at row 4 (THIS RUN@2, blank, heading@4), the first
    // modifier row at row 5; its caption starts at column 3.
    assert_eq!(g.get(3, 5).glyph, 'G');
    assert_eq!(
        g.get(3, 5).fg,
        Category::Warning,
        "a harder caption cues in Warning orange"
    );

    // An easier toggle's caption `All vision cones shown` cues in Owned.
    let easier = LevelModifiers {
        always_show_vision_cones: true,
        ..LevelModifiers::default()
    };
    let g = render_help(
        W,
        H,
        show(HelpTab::LevelInfo, SeedCopy::default()),
        panel_run(None, easier, &quiet_alert(), Loadout::innate()),
    );
    assert_eq!(g.get(3, 5).glyph, 'A');
    assert_eq!(
        g.get(3, 5).fg,
        Category::Owned,
        "an easier caption cues in Owned blue"
    );
}

/// The tab bar shows every tab it has, and the active one reads in Interest while
/// the rest are dim Ground — the at-a-glance "you are here" (#248), asserted on the
/// cell colour since a text render loses it. **One bar in every session** since #513
/// took the debug session's fourth tab to the options screen.
#[test]
fn the_tab_bar_highlights_the_active_tab() {
    {
        let layout = tab_layout();
        for &active in HelpTab::ALL.iter() {
            let ui = show(active, SeedCopy::default());
            let g = render_help(
                W,
                H,
                ui,
                panel_run(
                    None,
                    LevelModifiers::default(),
                    &quiet_alert(),
                    Loadout::innate(),
                ),
            );
            for &(tab, start, _len) in &layout {
                let expected = if tab == active {
                    Category::Interest
                } else {
                    Category::Ground
                };
                // The `[` at the tab's start carries its colour.
                assert_eq!(g.get(start, 0).glyph, '[', "{tab:?} draws its bracket");
                assert_eq!(
                    g.get(start, 0).fg,
                    expected,
                    "with {active:?} active, {tab:?} reads {expected:?}"
                );
            }
        }
    }
}

/// A hit-test on a full-height panel showing the [`Default`] tab of a run with
/// no level-seed token — the shape the tab-bar and footer tests want, where the
/// only thing that varies is where the finger landed.
fn hit(x: u32, y: u32) -> Option<HelpHit> {
    hit_on(H, x, y)
}

/// The same, on a panel `height` rows tall — for the footer, which is drawn from
/// the bottom edge and so must be hit-tested from it too.
fn hit_on(height: u32, x: u32, y: u32) -> Option<HelpHit> {
    help_hit(
        W,
        height,
        show(HelpTab::default(), SeedCopy::default()),
        None,
        x,
        y,
    )
}

/// The panel is escapable and switchable **by touch** (§11.6/#248): the `[x]`
/// close control hit-tests to [`HelpHit::Close`], each tab's cells to
/// [`HelpHit::Tab`], and the body to nothing (a press the modal panel swallows).
#[test]
fn the_panel_is_escapable_and_switchable_by_touch() {
    // The close control at the right edge → Close, and nothing just left of it.
    let close = close_button_start(W);
    assert_eq!(hit(close, 0), Some(HelpHit::Close));
    assert_eq!(hit(close + 1, 0), Some(HelpHit::Close));
    assert_ne!(hit(close - 1, 0), Some(HelpHit::Close));

    // Each tab's whole `[Label]` region resolves to that tab, by identity.
    for (tab, start, len) in tab_layout() {
        for x in start..start + len {
            assert_eq!(hit(x, 0), Some(HelpHit::Tab(tab)), "tab cell {x}");
        }
    }
    // The body (below the tab bar) and the gap left of the first tab are inert.
    assert_eq!(hit(5, 3), None, "the body swallows presses");
    assert_eq!(hit(0, 0), None, "the left margin is not a tab");
}

/// §11.6/#513: the options screen is reachable **by touch**, not just by key — the
/// footer's `options [o]` hit-tests to [`HelpHit::OpenSettings`] over exactly the cells
/// it is drawn on, and the rest of the footer row is inert like the body. A phone has
/// no `o` key, so without this the settings — the theme included, and in a debug
/// session the §12.6 switches — would be a desktop-only screen on a game that fits its
/// whole board to a phone.
///
/// **The word is a target too, not only the bracketed key.** `options` is the larger
/// and more obvious thing to reach for, and a bare `[o]` is only a target if you
/// already know what `o` means — so every cell of the label presses the control, which
/// is what this walks. It is the rule the `theme [n]` control that stood here was held
/// to (#189), inherited by the control that replaced it.
#[test]
fn the_options_control_is_reachable_by_touch() {
    let start = options_control_start(W);
    for x in start..start + options_control_len() {
        assert_eq!(
            hit(x, H - 1),
            Some(HelpHit::OpenSettings),
            "footer cell {x}"
        );
    }
    // The label's own first cell — the regression this guards is a control that
    // only answered on its last three cells.
    let label_end = start + OPTIONS_LABEL.chars().count() as u32;
    assert_eq!(hit(start, H - 1), Some(HelpHit::OpenSettings));
    assert_eq!(hit(label_end - 1, H - 1), Some(HelpHit::OpenSettings));

    assert_eq!(hit(start - 1, H - 1), None, "the hint is inert");
    assert_eq!(hit(start, H - 2), None, "only the footer row");
    // Measured from the *bottom* edge, so a shorter screen moves it with the
    // drawing rather than leaving the hit region a row adrift.
    assert_eq!(hit_on(20, start, 19), Some(HelpHit::OpenSettings));
    assert_eq!(hit_on(20, start, H - 1), None);
}

/// A run whose panel really does draw a token — quick play, spelled out in full
/// (#333) — for the copy-control tests below.
fn run_with_a_token() -> LevelSeed {
    LevelSeed::quick_play(8371)
}

/// The Level info panel for `level`, with the seed-copy acknowledgement in
/// `copy` — the frame the copy control is drawn on.
fn level_info(level: Option<LevelSeed>, copy: SeedCopy) -> Grid {
    render_help(
        W,
        H,
        show(HelpTab::LevelInfo, copy),
        panel_run(
            level,
            LevelModifiers::default(),
            &quiet_alert(),
            Loadout::innate(),
        ),
    )
}

/// **The token is takeable** (§13.1/#353): the Level info tab draws a `copy [c]`
/// control on the token's own row, and every cell of it — the word as much as the
/// bracketed key, for the reason `theme [n]` gives — hit-tests to
/// [`HelpHit::CopySeed`].
///
/// The rows around it stay inert, which is the point of testing the neighbours as
/// well as the control: the token row is in the middle of the panel body, where
/// every other press is swallowed, so a copy control with a sloppy region would
/// start eating presses that used to mean nothing.
#[test]
fn the_token_row_carries_a_copy_control_and_its_neighbours_do_not() {
    let level = Some(run_with_a_token());
    let hit = |x, y| {
        help_hit(
            W,
            H,
            show(HelpTab::LevelInfo, SeedCopy::default()),
            level,
            x,
            y,
        )
    };

    let start = copy_control_start(W);
    for x in start..start + copy_control_len() {
        assert_eq!(hit(x, SEED_TOKEN_ROW), Some(HelpHit::CopySeed), "cell {x}");
    }
    // The label's own first cell, and the last cell of the word before the key —
    // the whole `copy` run presses, not just the `[c]`.
    let label_end = start + COPY_LABEL.chars().count() as u32;
    assert_eq!(hit(start, SEED_TOKEN_ROW), Some(HelpHit::CopySeed));
    assert_eq!(hit(label_end - 1, SEED_TOKEN_ROW), Some(HelpHit::CopySeed));

    // Just left of it, and the rows above and below: the heading, the token's own
    // letters, and the acknowledgement line all stay body.
    assert_eq!(hit(start - 1, SEED_TOKEN_ROW), None, "the gap is inert");
    assert_eq!(
        hit(CONTENT_INDENT, SEED_TOKEN_ROW),
        None,
        "the token itself"
    );
    assert_eq!(
        hit(start, SEED_TOKEN_ROW - 1),
        None,
        "the LEVEL SEED heading"
    );
    assert_eq!(hit(start, SEED_TOKEN_ROW + 1), None, "the row beneath");
}

/// **No token, no control** (#353): a hand-built state has nothing that
/// reproduces it, so the panel shows no seed section — and the row where the
/// control would have been resolves to nothing at all rather than to a button
/// that copies an empty string.
#[test]
fn a_run_with_no_token_offers_nothing_to_copy() {
    let start = copy_control_start(W);
    for x in start..start + copy_control_len() {
        assert_eq!(
            help_hit(
                W,
                H,
                show(HelpTab::LevelInfo, SeedCopy::default()),
                None,
                x,
                SEED_TOKEN_ROW
            ),
            None,
            "cell {x} of a panel with no token",
        );
    }
    assert!(
        !text_of(&level_info(None, SeedCopy::Idle)).contains(&copy_control()),
        "and nothing is drawn there either",
    );
}

/// The control belongs to the **Level info** tab, because the token does: the very
/// same cells on the other tabs are body, whatever run is playing. Otherwise a tap
/// meant for a line of the Abilities card would copy a seed.
#[test]
fn the_copy_control_is_the_level_info_tabs_alone() {
    let level = Some(run_with_a_token());
    let start = copy_control_start(W);
    for tab in HelpTab::ALL {
        if tab == HelpTab::LevelInfo {
            continue;
        }
        // Every other tab, in the session that actually shows it — the debug one, so
        // the Debug tab is tested as itself rather than falling back to Level info.
        let ui = ScreenUi {
            debug_mode: true,
            ..show(tab, SeedCopy::default())
        };
        // Not *nothing* — the Debug tab's own control column reaches these very
        // cells (#459) — but never the seed copy, which is the claim: a tap on
        // another tab can no more copy a token than the tab can show one.
        assert_ne!(
            help_hit(W, H, ui, level, start, SEED_TOKEN_ROW),
            Some(HelpHit::CopySeed),
            "{tab:?} has no token on it",
        );
    }
}

/// The control is drawn where it is hit-tested, in the HUD-control colour the
/// `[x]` and the theme button wear (#353) — and it clears the token beside it, a
/// thing [`draw`] would otherwise resolve by silently overwriting the end of the
/// one string on this panel that has to be read character for character.
#[test]
fn the_copy_control_is_drawn_clear_of_the_token() {
    let level = run_with_a_token();
    let token = level.encode().expect("a config a run can hold");
    let g = level_info(Some(level), SeedCopy::Idle);

    let start = copy_control_start(W);
    let drawn: String = (start..start + copy_control_len())
        .map(|x| g.get(x, SEED_TOKEN_ROW).glyph)
        .collect();
    assert_eq!(
        drawn,
        copy_control(),
        "the control is drawn on the token row"
    );
    assert_eq!(g.get(start, SEED_TOKEN_ROW).fg, Category::System);

    // The token still reads whole, and the two do not touch.
    let token_end = CONTENT_INDENT + token.chars().count() as u32;
    assert!(
        token_end < start,
        "the token ({token_end} cells in) runs into the copy control at {start}",
    );
    assert_eq!(g.get(token_end - 1, SEED_TOKEN_ROW).fg, Category::Interest);
}

/// The acknowledgement (#353) is **honest and quiet**: nothing before a press,
/// a plain "copied" after one, and a failure that says the clipboard did *not*
/// take it rather than claiming it did. It lands in the blank spacer the token
/// already had beneath it, so saying so shifts no row of the modifier list below.
#[test]
fn the_copy_acknowledgement_says_only_what_happened() {
    let level = Some(run_with_a_token());
    let ack_row = SEED_ACK_ROW;

    let idle = level_info(level, SeedCopy::Idle);
    assert!(
        row_text(&idle, ack_row).trim().is_empty(),
        "nothing is claimed before anything is pressed",
    );

    let copied = level_info(level, SeedCopy::Copied);
    assert!(row_text(&copied, ack_row).contains(COPIED_ACK));
    assert_eq!(copied.get(CONTENT_INDENT, ack_row).fg, Category::System);

    let failed = level_info(level, SeedCopy::Unavailable);
    let text = row_text(&failed, ack_row);
    assert!(text.contains(UNAVAILABLE_ACK));
    assert!(
        !text.contains(COPIED_ACK) && !text.contains("copied"),
        "a failure never reads as a copy: {text:?}",
    );
    assert_eq!(failed.get(CONTENT_INDENT, ack_row).fg, Category::Warning);

    // Whatever it says, the token is still printed and the list below it has not
    // moved — the failure path degrades to exactly the panel that existed before
    // this control did.
    let token = run_with_a_token().encode().expect("a token");
    for copy in [SeedCopy::Idle, SeedCopy::Copied, SeedCopy::Unavailable] {
        let g = level_info(level, copy);
        assert!(text_of(&g).contains(&token), "the token stays readable");
        assert!(row_text(&g, ack_row + 1).contains("MODIFIERS"));
    }
}

/// The panel's key and its control name the same character (#353), the way `o`
/// and `options [o]` do — so the `[c]` the player reads is the `c` they press. It is
/// deliberately **not** a board key: there is no token drawn outside this panel
/// for it to copy, which is also what leaves the letter free for an ability
/// mnemonic (#360).
#[test]
fn the_copy_key_is_the_same_on_the_control_and_in_the_table() {
    let key = COPY_KEY.to_string();
    assert_eq!(
        crate::help_nav_for_key(&key),
        Some(crate::input::HelpNav::CopySeed),
    );
    assert!(copy_control().ends_with(&format!("[{key}]")));
    assert_eq!(
        crate::input::ui_command_for_key(&key),
        None,
        "panel-only: the board has no token to copy",
    );
    assert_eq!(
        crate::input_for_key(&key),
        None,
        "and it shadows no movement"
    );
}

/// The footer's hint and its options control share one row, so the prose must stop
/// before the control starts — [`draw`] would clip the control in silence, and a
/// half-drawn control is one the player cannot see they can press.
#[test]
fn the_footer_hint_stops_short_of_the_options_control() {
    let end = FOOTER_INDENT + FOOTER_HINT.chars().count() as u32;
    assert!(
        end < options_control_start(W),
        "the footer hint runs into the options control ({end} vs {})",
        options_control_start(W),
    );
    // And the control itself clears the board's right margin.
    assert_eq!(
        options_control(),
        format!("{OPTIONS_LABEL} [{SETTINGS_KEY}]")
    );
    assert!(options_control_start(W) + options_control_len() <= W);
}

/// The panel's own door to the options screen cannot drift from its key (#513): the
/// drawn control ends with the bracketed character the table answers to, and — like
/// `c` — the key is panel-only, so it claims nothing from the board.
#[test]
fn the_options_key_is_the_same_on_the_control_and_in_the_table() {
    let key = SETTINGS_KEY.to_string();
    assert_eq!(
        crate::help_nav_for_key(&key),
        Some(crate::input::HelpNav::OpenSettings),
    );
    assert!(options_control().ends_with(&format!("[{key}]")));
    assert_eq!(
        crate::input::ui_command_for_key(&key),
        None,
        "panel-only: it claims no board letter from the ability mnemonics",
    );
    assert_eq!(crate::input_for_key(&key), None, "it shadows no movement");
    // …and it leaves the screen it opened, so the pair is `?`/`?` all over again.
    assert_eq!(
        crate::settings_nav_for_key(&key),
        Some(crate::input::SettingsNav::Back),
    );
}

/// The card and the key tables cannot drift (#189): the controls row and both
/// bindings name the same character. The theme is the one key the open panel
/// *forwards* rather than swallows — its colour key is the best thing on screen to
/// judge the flip against, even though the setting's home is the options screen now.
#[test]
fn the_theme_key_is_the_same_on_the_card_and_in_both_tables() {
    let key = THEME_KEY.to_string();
    assert_eq!(
        crate::input::ui_command_for_key(&key),
        Some(crate::input::UiCommand::ToggleTheme),
    );
    assert_eq!(
        crate::help_nav_for_key(&key),
        Some(crate::input::HelpNav::ToggleTheme),
        "the modal panel forwards the standing shortcut",
    );
    // It shadows nothing: not a movement key, and not an ability key — those are
    // the bar's four digits now (§11.6/#359), so a letter cannot collide with one.
    assert_eq!(crate::input_for_key(&key), None);
    assert_eq!(
        crate::ability_slot_for_code(&format!("Key{}", key.to_uppercase())),
        None
    );
}

/// The tabs cycle, wrapping at both ends (§14 v2/#248) — the Tab / arrow motion.
/// Written over [`HelpTab::ALL`] rather than naming pairs, so adding a tab (as #343
/// did) extends the cycle instead of breaking the test.
#[test]
fn the_tabs_cycle_both_ways() {
    assert_eq!(HelpTab::LevelInfo.next(), HelpTab::Abilities);
    for (i, &tab) in HelpTab::ALL.iter().enumerate() {
        let after = HelpTab::ALL[(i + 1) % HelpTab::ALL.len()];
        assert_eq!(tab.next(), after, "{tab:?} advances, wrapping at the end");
        assert_eq!(after.prev(), tab, "…and steps back the same way");
    }
}

/// **The bar is the same in every session** (#513). It was not while the debug
/// session had a fourth tab of its own (#459) — the cycle, the layout and the
/// hit-test all had to be written over a `shown` list, and a stale tab had to be
/// resolved before it could be drawn. The switches live on the options screen now, so
/// the panel has one bar, the three tabs a player always had.
#[test]
fn every_session_gets_the_same_three_tabs() {
    assert_eq!(HelpTab::ALL.len(), 3);
    for debug in [false, true] {
        let ui = ScreenUi {
            debug_mode: debug,
            ..show(HelpTab::LevelInfo, SeedCopy::default())
        };
        let bar = render_help(
            W,
            H,
            ui,
            panel_run(
                None,
                LevelModifiers::default(),
                &quiet_alert(),
                Loadout::innate(),
            ),
        )
        .to_text()[0]
            .clone();
        for tab in HelpTab::ALL {
            assert!(
                bar.contains(tab.label()),
                "{tab:?} is missing from the bar (debug: {debug}): {bar:?}",
            );
        }
        assert!(
            !bar.contains("Debug"),
            "the debug session draws no tab of its own (debug: {debug}): {bar:?}",
        );
    }
}

/// `ActiveModifier` is re-exported for shells and tests to read the descriptor
/// directly, not only through the rendered card — a light guard that the type
/// stays public and constructible.
#[test]
fn the_active_modifier_descriptor_is_readable() {
    let m = ActiveModifier {
        name: "x",
        direction: ModifierDirection::Harder,
        detail: None,
    };
    assert_eq!(m.direction, ModifierDirection::Harder);
}
