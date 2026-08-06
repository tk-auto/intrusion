//! What the map screen owes a run (§11.1/§14 v3): that the picture is true to the graph
//! behind it, that the list is reachable by key **and** by finger (§11.6), that colour is
//! named and never chosen (§11.2), and that it is a pure view — no world touched, no turn
//! spent (§4.4/§12.1).

use super::*;
use crate::campaign::{CampaignStage, ALERTS_ALL, ALERTS_ONE};
use crate::render::GlyphCell;
use crate::verdict::{Ending, RunStats, Verdict};

/// The v1 frame (§10.2/§11.4): the board's width, and its height plus the status rows.
const W: u32 = 40;
const H: u32 = 43;

/// A raid walked out of at §7.3 `condition` — what the campaign alert reads (#210), and
/// so what the map's subtitle has to report.
fn escaped_at(condition: u32) -> Verdict {
    Verdict {
        ending: Ending::Escaped,
        stats: RunStats {
            alert_peak: condition,
            ..RunStats::default()
        },
    }
}

/// A run standing at its **first choice point**: the opening facility raided and left, so
/// the map is showing a fan of successors rather than the single row a fresh run gets.
fn at_a_choice_point(seed: u64) -> Campaign {
    at_a_choice_point_after(seed, 0)
}

/// The same, having left the opening facility at §7.3 `condition`.
fn at_a_choice_point_after(seed: u64, condition: u32) -> Campaign {
    let mut run = Campaign::new(seed);
    run.enter();
    run.complete(&escaped_at(condition));
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
            "    Left unnoticed — Depot off guard    ",
            "         Intel — nothing banked         ",
            "   ▫      ▫          ★       ▫       ▫  ",
            "                                        ",
            "                                        ",
            "                                        ",
            "                                        ",
            " ▫           ▫      ▫             ▫     ",
            "                           ▫            ",
            "                                        ",
            "    ▫                                   ",
            "            ▫                           ",
            "                   ▫      ▫         ▫   ",
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
            " ▫                                 ?    ",
            "                     ▪          ···     ",
            "         ▫          ·    $   ···        ",
            "                   ·   ······           ",
            "                  ·  ····               ",
            " ▫         ▫      @···     ▫        ▫   ",
            "                                        ",
            "                                        ",
            "                                        ",
            "                                        ",
            "  > Depot — an ordinary facility        ",
            "                                        ",
            "    Vault — worth robbing, and watched  ",
            "                                        ",
            "    Alternative route — 1 intel         ",
            "                                        ",
            "                                        ",
            "                                        ",
            "  ↑↓ choose · Enter/tap opens theme [n] ",
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
    // The list band only: the wallet line above it (#211) is written with the same dash,
    // and it is a readout rather than something the marker can rest on.
    let listed: Vec<&String> = rows[list_top(H) as usize..]
        .iter()
        .filter(|r| r.contains('—'))
        .collect();
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

/// **The map says what the last raid left on the ground ahead** (§14 v3/#210) — the
/// readout that keeps the campaign alert from being decoration.
///
/// The line has to arrive *before* the choice, because routing around an alerted
/// facility is the whole play at condition 2: it names the facility by its flavour (no
/// two open successors share one, §14 v3 **[SETTLED]**), and it names the same facility
/// the mapping actually bent.
#[test]
fn the_map_says_which_facility_ahead_the_last_raid_alerted() {
    for seed in 0..12 {
        let run = at_a_choice_point_after(seed, ALERTS_ONE);
        let line = alert_row(&run);
        assert!(
            line.starts_with(&condition_line(ALERTS_ONE)),
            "seed {seed}: {line}",
        );

        // Exactly one road ahead is alerted, and the line names that one — not another
        // row of the list, and not the locked edge.
        let alerted: Vec<Offer> = run
            .ahead()
            .into_iter()
            .filter(|offer| run.alert_reaches(offer.node).is_some())
            .collect();
        assert_eq!(alerted.len(), 1, "seed {seed}");
        assert!(
            line.contains(&format!("{} {ALERTED}", alerted[0].flavour.label())),
            "seed {seed}: {line}",
        );
        assert!(!alerted[0].locked, "seed {seed}: the lock is never alerted");
    }
}

/// **The top of the ladder reads as the one thing it is** (§7.3/#210): there is no
/// unwatched road left to name, so the line stops naming one.
#[test]
fn the_loudest_raid_says_every_road_ahead_is_alerted() {
    let run = at_a_choice_point_after(8371, ALERTS_ALL);
    let line = alert_row(&run);
    assert_eq!(
        line,
        format!("{}{SEPARATOR}{ALL_ALERTED}", condition_line(ALERTS_ALL)),
    );

    // Condition 1 is the loudness that carries nothing, and it says so rather than
    // leaving a blank row a player would read as a bug.
    let ordinary = at_a_choice_point_after(8371, 1);
    assert_eq!(
        alert_row(&ordinary),
        format!("{}{SEPARATOR}{NOTHING_FOLLOWS}", condition_line(1)),
    );
}

/// **A run with no raid behind it reports nothing** — the line is a statement about the
/// last raid, and a fresh run has not made one. The layout does not move for it either:
/// the map band starts at the same row with the line and without it.
#[test]
fn a_run_that_has_raided_nothing_has_no_alert_to_report() {
    let fresh = Campaign::new(8371);
    assert_eq!(alert_text(&fresh, &fresh.ahead()), None);
    assert_eq!(
        text(&fresh, MapUi::default())[ALERT_ROW as usize].trim(),
        ""
    );
}

/// **Colour is named, never chosen** (§11.2), and it is the *same* cue the help card
/// gives the same modifier (#248): a rule bent against you reads Warning, one bent your
/// way reads Owned, and a raid whose noise reached nothing is a fact rather than a
/// threat.
#[test]
fn the_alert_line_carries_the_direction_it_reports() {
    let alerted = at_a_choice_point_after(8371, ALERTS_ALL);
    assert_eq!(
        alert_text(&alerted, &alerted.ahead()).map(|(_, category)| category),
        Some(Category::Warning),
    );
    let ghost = at_a_choice_point_after(8371, 0);
    assert_eq!(
        alert_text(&ghost, &ghost.ahead()).map(|(_, category)| category),
        Some(Category::Owned),
    );
    let ordinary = at_a_choice_point_after(8371, 1);
    assert_eq!(
        alert_text(&ordinary, &ordinary.ahead()).map(|(_, category)| category),
        Some(Category::Ground),
    );

    // And on the approach to a facility the noise did not settle on, the line says so —
    // the tail is decided by what the list is showing, not by re-deriving the mapping.
    let mut past_it = at_a_choice_point_after(8371, ALERTS_ONE);
    let clear = past_it
        .offers()
        .into_iter()
        .find(|offer| !offer.locked && past_it.alert_reaches(offer.node).is_none())
        .expect("condition 2 leaves a road to route around it");
    assert!(past_it.choose(clear.node));
    let (line, category) = alert_text(&past_it, &past_it.ahead()).expect("a raid behind it");
    assert!(line.ends_with(NOTHING_AHEAD), "{line}");
    assert_eq!(category, Category::Ground);
}

/// The alert line as drawn, trimmed — the row under the heading.
fn alert_row(run: &Campaign) -> String {
    text(run, MapUi::default())[ALERT_ROW as usize]
        .trim()
        .to_string()
}

/// The wallet line as the player reads it.
fn wallet_row(run: &Campaign) -> String {
    text(run, MapUi::default())[WALLET_ROW as usize]
        .trim()
        .to_string()
}

/// **The map is the hub, so it says what there is to spend** (§2.2/§14 v3/#211).
///
/// Unconditional, both wordings, in the currency's own word (§11.8) — a run that has
/// banked nothing says so rather than showing no line, because a missing readout reads as
/// a broken one.
#[test]
fn the_map_says_what_the_run_has_to_spend() {
    let mut run = Campaign::new(8371);
    assert_eq!(wallet_row(&run), "Intel — nothing banked");

    run.enter();
    run.complete(&Verdict {
        ending: Ending::Escaped,
        stats: RunStats {
            intel: 7,
            ..RunStats::default()
        },
    });
    assert_eq!(run.intel(), 7);
    assert_eq!(wallet_row(&run), "Intel 7");

    // And it moves when the hub takes some — the balance on screen is the balance the
    // next price is read against, never a stale copy.
    assert!(run.spend(4).paid());
    assert_eq!(wallet_row(&run), "Intel 3");
}

/// **The wallet line does not move the picture** (§11.4). It is drawn on every frame, so
/// the map band beneath it is the same height at a choice point, on the approach, and
/// with the balance at zero — a readout that appeared with the first haul would make the
/// map jump exactly once, while the player was reading it.
#[test]
fn the_wallet_line_never_moves_the_map_band() {
    let fresh = text(&Campaign::new(8371), MapUi::default());
    let mut rich = at_a_choice_point(8371);
    assert!(rich.spend(0).paid());

    for rows in [&fresh, &text(&rich, MapUi::default())] {
        assert_eq!(rows.len(), H as usize);
        assert!(rows[WALLET_ROW as usize].contains("Intel"));
        assert!(
            rows[MAP_TOP as usize - 1].trim().starts_with("Intel"),
            "the band starts directly under the wallet line",
        );
    }
}

/// A run at a choice point with `intel` banked — what the hub's rows are read against.
fn at_a_choice_point_holding(seed: u64, intel: usize) -> Campaign {
    let mut run = Campaign::new(seed);
    run.enter();
    run.complete(&Verdict {
        ending: Ending::Escaped,
        stats: RunStats {
            intel,
            ..RunStats::default()
        },
    });
    assert_eq!(run.stage(), CampaignStage::Choosing);
    run
}

/// The list band as the player reads it, one string per row.
fn rows_of(run: &Campaign, ui: MapUi) -> Vec<String> {
    text(run, ui)[list_top(H) as usize..]
        .iter()
        .filter(|r| !r.trim().is_empty())
        .map(|r| r.trim().to_string())
        .collect()
}

/// **The priced row prints the campaign's price** (§14 v3/#212), not a number the screen
/// invented — so a change to [`ROUTE_UNLOCK_COST`] moves what the player is charged and
/// what they are told in one edit.
#[test]
fn the_alternative_route_row_says_what_it_costs() {
    let run = at_a_choice_point(8371);
    let priced = rows_of(&run, MapUi::default())
        .into_iter()
        .find(|r| r.contains(LOCKED_LABEL))
        .expect("a priced row");
    assert!(
        priced.ends_with(&format!("{ROUTE_UNLOCK_COST} intel")),
        "{priced}",
    );
}

/// **An unaffordable price reads as unaffordable** (§2.3): Ground, the meaning this screen
/// already gives the road behind you and gave the lock before it had a price — *on the
/// map, not available to you*. Affordable, it is as live as any other row.
///
/// Told at a glance rather than only when pressed, which is the courtesy of showing a cost
/// before charging for the discovery.
#[test]
fn a_price_the_run_cannot_meet_is_drawn_as_out_of_reach() {
    let row_category_of = |run: &Campaign| {
        let ahead = run.ahead();
        let i = ahead.iter().position(|o| o.locked).expect("a priced row");
        // Off the marker, so the marker's own Interest is not what is being read.
        let ui = MapUi {
            selected: if i == 0 { 1 } else { 0 },
            ..MapUi::default()
        };
        let grid = render_map(W, H, run, ui);
        grid.get(list_column(W, &ahead) + 2, row_of(H, i)).fg
    };

    let broke = at_a_choice_point_holding(8371, (ROUTE_UNLOCK_COST - 1) as usize);
    assert_eq!(row_category_of(&broke), Category::Ground);

    let flush = at_a_choice_point_holding(8371, ROUTE_UNLOCK_COST as usize);
    assert_eq!(row_category_of(&flush), Category::Neutral);
}

/// **A bought road becomes an ordinary row** (§14 v3 **[SETTLED]**: what is offered shows
/// its flavour). The purchase buys ground *and* the knowledge of what stands on it — and
/// it does not commit the run, which is what stops the sink being a blind coin flip.
#[test]
fn buying_the_route_turns_the_price_into_a_facility() {
    let mut run = at_a_choice_point_holding(8371, ROUTE_UNLOCK_COST as usize);
    let locked = run
        .ahead()
        .into_iter()
        .find(|o| o.locked)
        .expect("a priced row");
    assert!(rows_of(&run, MapUi::default())
        .iter()
        .any(|r| r.contains(LOCKED_LABEL)));
    assert_eq!(
        cells_of(&render_map(W, H, &run, MapUi::default()), LOCKED_GLYPH).len(),
        1
    );

    let outlay = run.unlock(locked.node);
    assert!(outlay.paid());

    // The row now names the facility, and the `?` on the picture is the flavour's glyph.
    let rows = rows_of(&run, MapUi::default());
    assert!(!rows.iter().any(|r| r.contains(LOCKED_LABEL)), "{rows:?}");
    let bought = run.map().flavour(locked.node);
    assert!(
        rows.iter().any(|r| r.contains(bought.label())),
        "the bought road names itself: {rows:?}",
    );
    let grid = render_map(W, H, &run, MapUi::default());
    assert!(cells_of(&grid, LOCKED_GLYPH).is_empty());
    assert!(cells_of(&grid, flavour_glyph(bought))
        .iter()
        .any(|c| c.fg != Category::Ground));
}

/// **The hub answers on the wallet line** (#211's `Outlay`, #212's press) — paid in Owned,
/// refused in Warning, and every message names the balance, so the readout it replaces is
/// not a fact the player loses.
#[test]
fn the_wallet_line_carries_what_the_hub_just_said() {
    let mut broke = at_a_choice_point_holding(8371, (ROUTE_UNLOCK_COST - 1) as usize);
    let locked = broke.ahead().into_iter().find(|o| o.locked).expect("a row");

    let refused = broke.unlock(locked.node);
    assert!(!refused.paid());
    let ui = MapUi::default().saying(refused);
    assert_eq!(
        text(&broke, ui)[WALLET_ROW as usize].trim(),
        refused.message(),
    );
    assert_eq!(
        render_map(W, H, &broke, ui)
            .get(
                centre(W, refused.message().chars().count() as u32),
                WALLET_ROW
            )
            .fg,
        Category::Warning,
        "a refusal is a warning, the meaning this screen already gives a rule bent against you",
    );

    let mut flush = at_a_choice_point_holding(8371, ROUTE_UNLOCK_COST as usize);
    let paid = flush.unlock(locked.node);
    assert!(paid.paid());
    let ui = MapUi::default().saying(paid);
    assert_eq!(text(&flush, ui)[WALLET_ROW as usize].trim(), paid.message());

    // Both wordings fit the board (§10.2/§11.4) — they are formatted rather than const,
    // so the bound is asserted here instead of at compile time.
    for outlay in [paid, refused, Outlay::Closed] {
        assert!(
            outlay.message().chars().count() <= W as usize,
            "{:?} does not fit the board",
            outlay,
        );
    }
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
    let marked = MapUi::default().next(&run);
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

/// **The marker rests on every row, the priced one included** (#268's rule, #212's row).
///
/// It used to step over the lock, because the marker only ever rests where Enter does
/// something. The rule has not changed and the row has: an intel-locked row is a **price**
/// now, and pressing Enter on it buys the road or says why it cannot.
#[test]
fn the_marker_walks_every_row_including_the_priced_one() {
    let run = at_a_choice_point(8371);
    let ahead = run.ahead();
    assert!(ahead.iter().any(|o| o.locked), "there is a priced row");

    let mut ui = MapUi::default();
    let mut walked = vec![ui.selected(&ahead)];
    for _ in 0..ahead.len() {
        ui = ui.next(&run);
        walked.push(ui.selected(&ahead));
    }
    assert_eq!(
        walked,
        (0..ahead.len()).chain([0]).collect::<Vec<usize>>(),
        "next walks every row and wraps to the first",
    );
    assert_eq!(
        MapUi::default().prev(&run).selected(&ahead),
        ahead.len() - 1,
        "prev past the first wraps to the last row",
    );

    // A marker left pointing past a shorter list resolves to a row that exists — the list
    // changes under it at every choice point.
    for stale in [ahead.len(), 99] {
        let resolved = MapUi {
            selected: stale,
            ..MapUi::default()
        }
        .selected(&ahead);
        assert!(
            resolved < ahead.len(),
            "stale marker {stale} landed off the list"
        );
    }

    // Moving the marker drops the hub\'s last word: a message about the row you have just
    // left is a message about nothing.
    let saying = MapUi::default().saying(Outlay::Closed);
    assert_eq!(saying.outlay, Some(Outlay::Closed));
    assert_eq!(saying.next(&run).outlay, None);
    assert_eq!(saying.prev(&run).outlay, None);
}

/// **Every row is reachable by finger too** (§11.6): a press anywhere along it raids that
/// facility — or **buys** it, where the row is the priced one (#212) — and the blank
/// between rows swallows a mis-aimed tap. The touch half of the marker's rule, and the two
/// verbs are told apart in one place so a tap and a keypress cannot come to disagree.
#[test]
fn a_press_on_a_row_does_what_that_row_says() {
    let run = at_a_choice_point(8371);
    let ahead = run.ahead();
    for (i, offer) in ahead.iter().enumerate() {
        let row = row_of(H, i);
        let expected = Some(if offer.locked {
            MapHit::Unlock(offer.node)
        } else {
            MapHit::Facility(offer.node)
        });
        assert_eq!(Some(hit_of(*offer)), expected, "row {i}");
        for x in [0, W / 2, W - 1] {
            assert_eq!(
                map_hit(W, H, &run, MapUi::default(), x, row),
                expected,
                "row {i} at column {x}"
            );
        }
        // The blank beneath each row is the buffer that keeps a low tap off its
        // neighbour, exactly as the title screen's is.
        assert_eq!(
            map_hit(W, H, &run, MapUi::default(), W / 2, row + 1),
            None,
            "the gap under row {i}",
        );
    }
    // The picture is not a target: a node is one cell, and one cell is not something a
    // finger can hit — the row underneath is how you pick it.
    let map = run.map();
    let (x, y) = plot(map.position(ahead[0].node), map.depth(), W, map_height(H));
    assert_eq!(map_hit(W, H, &run, MapUi::default(), x, y), None);
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
    assert_eq!(
        map_hit(W, H, &run, MapUi::default(), theme, H - 1),
        Some(MapHit::ToggleTheme)
    );
    assert_eq!(
        map_hit(W, H, &run, MapUi::default(), theme - 1, H - 1),
        None
    );
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
