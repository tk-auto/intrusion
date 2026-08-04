//! What the map screen owes a run (§11.1/§14 v3): that the picture is true to the graph
//! behind it, that the list is reachable by key **and** by finger (§11.6), that colour is
//! named and never chosen (§11.2), and that it is a pure view — no world touched, no turn
//! spent (§4.4/§12.1).

use super::*;
use crate::campaign::CampaignStage;
use crate::render::GlyphCell;
use crate::verdict::{Ending, RunStats, Verdict};

/// The v1 frame (§10.2/§11.4): the board's width, and its height plus the status rows.
const W: u32 = 40;
const H: u32 = 43;

fn escaped() -> Verdict {
    Verdict {
        ending: Ending::Escaped,
        stats: RunStats::default(),
    }
}

/// A run standing at its **first choice point**: the opening facility raided and left, so
/// the map is showing a fan of successors rather than the single row a fresh run gets.
fn at_a_choice_point(seed: u64) -> Campaign {
    let mut run = Campaign::new(seed);
    run.enter();
    run.complete(&escaped());
    assert_eq!(run.stage(), CampaignStage::Choosing);
    run
}

/// The screen as rows of text — what a golden assertion reads.
fn text(run: &Campaign, ui: MapUi) -> Vec<String> {
    render_map(W, H, run, ui).to_text()
}

/// Every cell drawn with `glyph`, and the category it was drawn in.
fn cells_of(grid: &Grid, glyph: char) -> Vec<GlyphCell> {
    (0..grid.height())
        .flat_map(|y| (0..grid.width()).map(move |x| (x, y)))
        .map(|(x, y)| grid.get(x, y))
        .filter(|cell| cell.glyph == glyph)
        .collect()
}

/// **The golden screen** (§11.1): a fixed run at a fixed choice point draws exactly this.
///
/// It is the whole picture in one assertion — the country as outlines, the archive at the
/// head of the band, the road walked so far, the fan of offers with the locked edge two
/// lanes across, the marked list and the footer. A change to any of it shows up here as a
/// diff of the screen rather than as a property that still technically holds.
#[test]
fn the_map_draws_the_country_the_run_is_in() {
    let run = at_a_choice_point(8371);
    assert_eq!(
        text(&run, MapUi::default()),
        vec![
            "            THE FACILITY MAP            ",
            "                                        ",
            "   ▫      ▫          ★       ▫       ▫  ",
            "                                        ",
            "                                        ",
            "                                        ",
            "                                        ",
            " ▫           ▫      ▫             ▫     ",
            "                           ▫            ",
            "                                        ",
            "                                        ",
            "                                        ",
            "    ▫       ▫                           ",
            "                   ▫      ▫         ▫   ",
            "                                        ",
            "                                        ",
            "                                        ",
            "           ▫                        ▫   ",
            "   ▫                ▫        ▫          ",
            "                                        ",
            "                                        ",
            "                                        ",
            "                    ▫      ▫            ",
            "  ▫       ▫                       ▫     ",
            "                                        ",
            "                                        ",
            "                                        ",
            " ▫                   ▪             ?    ",
            "         ▫          ·    $      ···     ",
            "                   ·    ·    ···        ",
            "                   ·  ·· ····           ",
            "                  · ·····               ",
            " ▫         ▫      @···     ▫        ▫   ",
            "                                        ",
            "  > Depot — an ordinary facility        ",
            "                                        ",
            "    Vault — worth robbing, and watched  ",
            "                                        ",
            "    Alternative route — costs intel     ",
            "                                        ",
            "                                        ",
            "                                        ",
            "  ↑↓ choose · Enter/tap raids theme [n] ",
        ],
    );
}

/// **A fresh run opens on the map with one row: the facility under its feet** (#208).
///
/// This is what the title screen's *Story mode* leads to, so it is the first campaign
/// frame anybody sees, and it must read as a map rather than as a menu that happens to
/// have a picture over it.
#[test]
fn a_fresh_run_is_offered_the_facility_it_stands_on() {
    let run = Campaign::new(8371);
    let rows = text(&run, MapUi::default());
    let listed: Vec<&String> = rows.iter().filter(|r| r.contains('—')).collect();
    assert_eq!(listed.len(), 1, "one row: raid this one");
    assert!(listed[0].starts_with("  > Outpost — "), "{}", listed[0]);
    assert!(
        rows.iter().any(|r| r.contains('@')),
        "and the picture says which facility that is",
    );
    assert!(
        rows.iter().any(|r| r.contains('★')),
        "with the archive in view from the first frame — the map is not fogged (§14 v3)",
    );
}

/// **The picture is true to the graph** (§14 v3): every facility the map offers is drawn
/// at its own position, the locked one included, and the run's own cell carries the
/// player's glyph rather than the facility's.
#[test]
fn every_offer_is_drawn_where_the_model_puts_it() {
    for seed in 0..12 {
        let run = at_a_choice_point(seed);
        let grid = render_map(W, H, &run, MapUi::default());
        let map = run.map();
        let map_h = map_height(H);
        for offer in run.ahead() {
            let (x, y) = plot(map.position(offer.node), map.depth(), W, map_h);
            let drawn = grid.get(x, y).glyph;
            let expected = if offer.locked {
                LOCKED_GLYPH
            } else {
                flavour_glyph(offer.flavour)
            };
            assert_eq!(drawn, expected, "seed {seed}: {offer:?} at ({x}, {y})");
        }
        let (x, y) = plot(map.position(run.node()), map.depth(), W, map_h);
        assert_eq!(
            grid.get(x, y).glyph,
            HERE_GLYPH,
            "nothing may be drawn over the player (§11.3)",
        );
    }
}

/// **The country's shape is public and its contents are not** — the §11.5a rule one scale
/// up. A facility the run has neither stood on nor been offered draws as an outline, so a
/// glance says *how much room there is either side of my route* and says nothing about
/// what is in any of it (#215's scouting sinks still have something to sell).
#[test]
fn an_unoffered_facility_says_where_it_is_and_not_what_it_is() {
    let run = at_a_choice_point(8371);
    let grid = render_map(W, H, &run, MapUi::default());
    let map = run.map();
    let named: Vec<NodeId> = run
        .path()
        .iter()
        .copied()
        .chain(run.ahead().iter().map(|o| o.node))
        .chain([map.archive()])
        .collect();

    let mut outlines = 0;
    for depth in 0..=map.depth() {
        for lane in 0..LANES {
            let node = NodeId::at(depth, lane);
            if named.contains(&node) {
                continue;
            }
            let (x, y) = plot(map.position(node), map.depth(), W, map_height(H));
            assert_eq!(grid.get(x, y).glyph, UNKNOWN_GLYPH, "{node:?}");
            outlines += 1;
        }
    }
    assert!(outlines > 20, "most of the country is still unnamed");
    assert!(
        Flavour::ALL
            .iter()
            .all(|&f| flavour_glyph(f) != UNKNOWN_GLYPH),
        "an outline must never be mistakable for a flavour",
    );
}

/// **Colour is named, never chosen** (§11.2). The categories on this screen each mean one
/// thing, and Interest — the "worth reaching for" cue — is spent on the goal and on the
/// choice in hand, and nowhere else.
#[test]
fn the_screen_names_a_category_for_everything_it_draws() {
    let run = at_a_choice_point(8371);
    let grid = render_map(W, H, &run, MapUi::default());

    assert!(cells_of(&grid, HERE_GLYPH)
        .iter()
        .all(|c| c.fg == Category::Owned));
    assert!(cells_of(&grid, '★')
        .iter()
        .all(|c| c.fg == Category::Interest));
    assert!(cells_of(&grid, UNKNOWN_GLYPH)
        .iter()
        .all(|c| c.fg == Category::Ground));
    assert!(cells_of(&grid, EDGE_GLYPH)
        .iter()
        .all(|c| c.fg == Category::Ground));
    assert!(
        cells_of(&grid, LOCKED_GLYPH)
            .iter()
            .all(|c| c.fg == Category::Ground),
        "an edge you cannot take yet must not read as one you can",
    );

    // The marked row moves its Interest with it, and takes the node's colour along.
    let ahead = run.ahead();
    let marked = MapUi::default().next(&ahead);
    let grid = render_map(W, H, &run, marked);
    let map = run.map();
    let (x, y) = plot(
        map.position(ahead[marked.selected(&ahead)].node),
        map.depth(),
        W,
        map_height(H),
    );
    assert_eq!(grid.get(x, y).fg, Category::Interest);
}

/// **The marker only ever rests where Enter does something** (#268's rule, one screen
/// over): it wraps both ways over the open offers and steps over the intel-locked row.
#[test]
fn the_marker_walks_the_open_offers_and_steps_over_the_lock() {
    let run = at_a_choice_point(8371);
    let ahead = run.ahead();
    assert!(ahead.iter().any(|o| o.locked), "there is a lock to skip");

    let open: Vec<usize> = (0..ahead.len()).filter(|&i| !ahead[i].locked).collect();
    let mut ui = MapUi::default();
    let mut walked = vec![ui.selected(&ahead)];
    for _ in 0..open.len() {
        ui = ui.next(&ahead);
        walked.push(ui.selected(&ahead));
    }
    assert_eq!(
        walked,
        open.iter()
            .copied()
            .chain([open[0]])
            .collect::<Vec<usize>>(),
        "next walks the open rows and wraps to the first",
    );
    assert_eq!(
        MapUi::default().prev(&ahead).selected(&ahead),
        *open.last().expect("an open row"),
        "prev past the first wraps to the last open row",
    );

    // A marker left pointing past a shorter list, or at the lock, resolves to a row that
    // can actually be fired — the list changes under it at every choice point.
    for stale in [ahead.len() - 1, ahead.len(), 99] {
        let resolved = MapUi { selected: stale }.selected(&ahead);
        assert!(!ahead[resolved].locked, "stale marker {stale} landed live");
    }
}

/// **Every takeable row is reachable by finger too** (§11.6): a press anywhere along it
/// raids that facility, the blank between rows swallows a mis-aimed tap, and the locked
/// row is not a target at all — the touch half of the marker's rule.
#[test]
fn a_press_on_a_row_raids_the_facility_that_row_names() {
    let run = at_a_choice_point(8371);
    let ahead = run.ahead();
    for (i, offer) in ahead.iter().enumerate() {
        let row = row_of(H, i);
        let expected = (!offer.locked).then_some(MapHit::Facility(offer.node));
        for x in [0, W / 2, W - 1] {
            assert_eq!(
                map_hit(W, H, &run, x, row),
                expected,
                "row {i} at column {x}"
            );
        }
        // The blank beneath each row is the buffer that keeps a low tap off its
        // neighbour, exactly as the title screen's is.
        assert_eq!(
            map_hit(W, H, &run, W / 2, row + 1),
            None,
            "the gap under row {i}",
        );
    }
    // The picture is not a target: a node is one cell, and one cell is not something a
    // finger can hit — the row underneath is how you pick it.
    let map = run.map();
    let (x, y) = plot(map.position(ahead[0].node), map.depth(), W, map_height(H));
    assert_eq!(map_hit(W, H, &run, x, y), None);
}

/// **The theme control keeps its corner** (§11.2/#189), and the footer never runs into
/// it. The same pairing every other screen carries, so the one control a player who
/// cannot read the current theme needs is where they already know to look.
#[test]
fn the_footer_and_the_theme_control_never_meet() {
    let run = at_a_choice_point(8371);
    let footer = &text(&run, MapUi::default())[(H - 1) as usize];
    assert!(footer.contains(FOOTER));
    assert!(footer.contains("theme [n]"));
    assert!(
        FOOTER_INDENT + FOOTER.chars().count() as u32 <= theme_control_start(W),
        "the footer prose runs into the theme control",
    );

    let theme = theme_control_start(W);
    assert_eq!(map_hit(W, H, &run, theme, H - 1), Some(MapHit::ToggleTheme));
    assert_eq!(map_hit(W, H, &run, theme - 1, H - 1), None);
}

/// **The list never runs into the footer either**, in the widest offer a choice point
/// can make — three open edges and the locked one, which is also the common one. The
/// blank row beneath the last option is what keeps the rows reading as choices and the
/// footer reading as prose about the screen.
#[test]
fn the_last_row_keeps_a_blank_between_itself_and_the_footer() {
    let widest = MAX_ROWS as usize - 1;
    assert!(
        row_of(H, widest) + 1 < H - 1,
        "the last of {MAX_ROWS} rows sits on the footer",
    );
    // And the map band above still has room to be a map.
    assert!(
        map_height(H) > MAX_ROWS * ENTRY_SPACING,
        "the picture was squeezed out"
    );

    // The real four-row case, drawn: the row under the last option is blank.
    let run = at_a_choice_point(8371);
    let mut four = at_a_choice_point(8371);
    for seed in 0..40 {
        let candidate = at_a_choice_point(seed);
        if candidate.ahead().len() == MAX_ROWS as usize {
            four = candidate;
            break;
        }
    }
    assert_eq!(
        four.ahead().len(),
        MAX_ROWS as usize,
        "a four-row choice point"
    );
    let rows = text(&four, MapUi::default());
    let last = row_of(H, MAX_ROWS as usize - 1) as usize;
    assert!(!rows[last].trim().is_empty(), "the last option is drawn");
    assert!(rows[last + 1].trim().is_empty(), "and nothing crowds it");
    let _ = run;
}

/// **A screen that fits whatever board it is given** (§11.4). The country's spacing is a
/// `[START]` number the renderer scales rather than assumes, so a narrower or shorter
/// frame still draws the whole graph — no camera, no scrolling, and above all no panic.
#[test]
fn the_map_fits_whatever_frame_it_is_handed() {
    let run = at_a_choice_point(8371);
    for (w, h) in [(40, 43), (20, 24), (12, 12), (60, 60), (1, 1)] {
        let grid = render_map(w, h, &run, MapUi::default());
        assert_eq!((grid.width(), grid.height()), (w, h));
        for node in run.path() {
            let (x, y) = plot(
                run.map().position(*node),
                run.map().depth(),
                w,
                map_height(h),
            );
            assert!(x < w.max(1), "a node plotted off a {w}×{h} frame");
            assert!(y < h.max(MAP_TOP + 1), "a node plotted off a {w}×{h} frame");
        }
    }
}

/// **A pure view** (§4.4/§12.1): drawing the map twice draws the same screen, and neither
/// draw is a turn — the campaign is untouched by having been looked at.
#[test]
fn drawing_the_map_changes_nothing() {
    let run = at_a_choice_point(8371);
    let before = run.clone();
    let first = render_map(W, H, &run, MapUi::default());
    let second = render_map(W, H, &run, MapUi::default());
    assert_eq!(first.to_text(), second.to_text());
    assert_eq!(run, before, "looking at a map is not playing one");
}
