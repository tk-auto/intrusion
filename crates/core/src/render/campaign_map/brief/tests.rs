//! What the facility brief owes a run (§11.1/§14 v3/#215): that picking a facility does
//! not raid it, that the price is drawn before it is charged, that a purchase changes the
//! screen it was made on, and that a sale the hub cannot honour is never offered.

use super::*;
use crate::ability::Loadout;
use crate::campaign::{Campaign, CampaignStage, Offer, Outlay, MANIFEST_COST, SCOUT_COST};
use crate::modifiers::{Composite, IntelGate, LevelModifiers, ModifierSources};
use crate::render::campaign_map::{map_activation, map_hit, MapScreen};
use crate::verdict::{Ending, RunStats, Verdict};

/// The v1 frame (§10.2/§11.4): the board's width, and its height plus the status rows.
const W: u32 = 40;
const H: u32 = 43;

/// A run at its first choice point holding `intel` — the state every purchase here is made
/// from, since a fresh run has an empty wallet and no offers to spend it on.
fn at_a_choice_point_holding(seed: u64, intel: usize) -> Campaign {
    let mut run = Campaign::new(seed);
    run.enter();
    run.complete(&Verdict {
        ending: Ending::Escaped,
        stats: RunStats {
            intel,
            // The raid walks out holding what it walked in with (§8.3/#266) — a default
            // `RunStats` reports an *empty* set, and a run holding no innate ability is
            // not a run the level-seed token can spell, which would make every facility
            // unscoutable for a reason that has nothing to do with this sink.
            held: Loadout::innate(),
            ..RunStats::default()
        },
    });
    assert_eq!(run.stage(), CampaignStage::Choosing);
    run
}

/// The first facility on offer that is not behind a locked road — what a brief is opened
/// for.
fn open_offer(run: &Campaign) -> Offer {
    run.ahead()
        .into_iter()
        .find(|offer| !offer.locked)
        .expect("a choice point offers an open road")
}

/// The screen row a brief row sits on — the new geometry's answer, unwrapped for tests
/// that already know the row is there.
fn row_at(run: &Campaign, node: NodeId, i: usize) -> u32 {
    row_of_brief(run, node, H, i).expect("the row is drawn")
}

/// The brief's block as the player reads it — every drawn line, trimmed, in order. Read off
/// the layout rather than off a slice of the screen, so a taller block (a bought manifest,
/// #550) is read whole rather than clipped by an assumption about where it starts.
fn rows_of(run: &Campaign, ui: MapUi, node: NodeId) -> Vec<String> {
    let text = render_brief(W, H, run, ui, node).to_text();
    laid_out(run, node, H)
        .into_iter()
        .map(|(y, _)| text[y as usize].trim().to_string())
        .collect()
}

/// **Picking a facility opens its brief; it does not raid it** (#215/§2.1). The run has not
/// moved when the screen comes up, which is what makes a scout bought here bought *before*
/// the run commits.
#[test]
fn a_facility_row_opens_a_brief_and_moves_nothing() {
    let run = at_a_choice_point_holding(8371, 3);
    let offer = open_offer(&run);
    let ui = MapUi::default();

    assert_eq!(
        map_activation(&run, ui),
        Some(MapHit::Facility(run.ahead()[0].node)),
        "the list's rows open briefs",
    );

    let opened = ui.opening(offer.node);
    assert_eq!(opened.screen, MapScreen::Brief(offer.node));
    assert_eq!(opened.brief_row, 0, "the brief opens on its first row");
    assert_eq!(
        map_activation(&run, opened),
        Some(MapHit::Enter(offer.node)),
        "and the raid is the first row of the screen it opened",
    );
    // The run itself is untouched: still choosing, still standing where it was.
    assert_eq!(run.stage(), CampaignStage::Choosing);
    assert!(!run.path().contains(&offer.node));
}

/// **The brief says what the facility is, what each sink costs, and how to leave** — and
/// the rows are the same four whichever way they are reached.
#[test]
fn the_brief_offers_the_raid_the_two_sinks_and_the_way_back() {
    let run = at_a_choice_point_holding(8371, SCOUT_COST as usize);
    let offer = open_offer(&run);
    let ui = MapUi::default().opening(offer.node);

    assert_eq!(
        brief_rows(&run, offer.node),
        vec![
            BriefRow::Enter,
            BriefRow::Scout { bought: false },
            BriefRow::Manifest { bought: false },
            BriefRow::Back,
        ],
    );
    assert_eq!(
        rows_of(&run, ui, offer.node),
        vec![
            format!("> Enter the {}", offer.flavour.label()),
            format!("Scout the facility — {SCOUT_COST} intel"),
            format!("{MANIFEST_LABEL} — {MANIFEST_COST} intel"),
            "Back to the map".to_string(),
        ],
    );
}

/// **A bought scout says so on the screen it was bought from** (#215), and the row stops
/// being a price: a plan you already hold is not something to sell twice.
#[test]
fn a_scouted_facility_reads_as_scouted() {
    let mut run = at_a_choice_point_holding(8371, SCOUT_COST as usize);
    let offer = open_offer(&run);
    assert!(run.scout(offer.node).paid());
    let ui = MapUi::default().opening(offer.node);

    assert_eq!(
        brief_rows(&run, offer.node)[1],
        BriefRow::Scout { bought: true },
    );
    let rows = rows_of(&run, ui, offer.node);
    assert_eq!(rows[1], format!("{SCOUTED_LABEL} — {SCOUTED_BLURB}"));
    assert!(
        !rows[1].contains("intel"),
        "a paid price is not a price any more: {:?}",
        rows[1],
    );
}

/// **A price the run cannot meet reads as out of reach** (§2.3) — Ground, the meaning the
/// map already gives an unaffordable road — and a scout it has bought reads as **owned**,
/// the meaning it gives the intel in the wallet. Told at a glance rather than by pressing.
#[test]
fn the_scout_row_says_whether_it_can_be_had() {
    let category_of = |run: &Campaign, node: NodeId| {
        // Off the marker, so the marker's own Interest is not what is being read.
        let ui = MapUi {
            brief_row: 0,
            ..MapUi::default().opening(node)
        };
        let rows = brief_rows(run, node);
        let i = rows
            .iter()
            .position(|row| matches!(row, BriefRow::Scout { .. }))
            .expect("a scout row");
        let grid = render_brief(W, H, run, ui, node);
        let text = grid.to_text()[row_at(run, node, i) as usize].clone();
        let x = text
            .chars()
            .position(|c| !c.is_whitespace())
            .expect("a row") as u32;
        grid.get(x, row_at(run, node, i)).fg
    };

    let broke = at_a_choice_point_holding(8371, SCOUT_COST as usize - 1);
    assert_eq!(
        category_of(&broke, open_offer(&broke).node),
        Category::Ground
    );

    let flush = at_a_choice_point_holding(8371, SCOUT_COST as usize);
    assert_eq!(
        category_of(&flush, open_offer(&flush).node),
        Category::Neutral,
    );

    let mut bought = at_a_choice_point_holding(8371, SCOUT_COST as usize);
    let node = open_offer(&bought).node;
    assert!(bought.scout(node).paid());
    assert_eq!(category_of(&bought, node), Category::Owned);
}

/// **Every row is reachable by finger too** (§11.6): a press anywhere along it does what
/// the row says, and the blank between rows swallows a mis-aimed tap.
#[test]
fn a_press_on_a_brief_row_does_what_that_row_says() {
    let run = at_a_choice_point_holding(8371, SCOUT_COST as usize);
    let node = open_offer(&run).node;
    let ui = MapUi::default().opening(node);
    let expected = [
        MapHit::Enter(node),
        MapHit::Scout(node),
        MapHit::Manifest(node),
        MapHit::Back,
    ];

    for (i, want) in expected.iter().enumerate() {
        let row = row_at(&run, node, i);
        for x in [0, W / 2, W - 1] {
            assert_eq!(
                map_hit(W, H, &run, ui, x, row),
                Some(*want),
                "row {i} at column {x}",
            );
        }
        assert_eq!(
            map_hit(W, H, &run, ui, W / 2, row + 1),
            None,
            "the gap under row {i}",
        );
    }
}

/// **The marker walks the brief's own rows**, wrapping, and a marker left past the end of a
/// shorter brief resolves to a row that exists — the rows change under it as soon as a
/// scout is bought or the wallet empties.
#[test]
fn the_marker_walks_the_brief_and_survives_a_shorter_one() {
    let run = at_a_choice_point_holding(8371, SCOUT_COST as usize);
    let node = open_offer(&run).node;
    let rows = brief_rows(&run, node).len();

    let mut ui = MapUi::default().opening(node);
    let mut walked = vec![ui.brief_row];
    for _ in 0..rows {
        ui = ui.next(&run);
        walked.push(ui.brief_row);
    }
    assert_eq!(walked, (0..rows).chain([0]).collect::<Vec<usize>>());
    assert_eq!(
        MapUi::default().opening(node).prev(&run).brief_row,
        rows - 1,
        "prev past the first wraps to the last row",
    );

    let stale = MapUi {
        brief_row: 99,
        ..MapUi::default().opening(node)
    };
    assert_eq!(
        map_activation(&run, stale),
        Some(MapHit::Enter(node)),
        "a stale marker falls back to a row that exists",
    );

    // Walking the rows drops the hub's last word; leaving the brief keeps it, because it
    // is still the answer to the thing the player just did.
    let saying = MapUi::default().opening(node).saying(Outlay::Closed);
    assert_eq!(saying.next(&run).outlay, None);
    assert_eq!(saying.closing().outlay, Some(Outlay::Closed));
    assert_eq!(saying.closing().screen, MapScreen::List);
}

/// **A facility that cannot take the rule is never offered it** (#215): the row is absent
/// rather than drawn and refused, because no amount of intel would ever make it takeable —
/// which is exactly what separates it from a price the run merely cannot afford yet.
#[test]
fn a_facility_with_no_room_left_in_its_token_offers_no_scout() {
    let run = at_a_choice_point_holding(8371, 99);
    let crowded = run
        .ahead()
        .into_iter()
        .find(|offer| !offer.locked && !run.level_at(offer.node, true).is_sayable());
    let Some(offer) = crowded else {
        // Nothing on this seed's fan is full; the rule is then stated against the
        // predicate itself, which is what `brief_rows` reads.
        for offer in run.ahead().into_iter().filter(|o| !o.locked) {
            assert!(run.scoutable(offer.node));
            assert!(brief_rows(&run, offer.node).contains(&BriefRow::Scout { bought: false }));
        }
        return;
    };
    assert!(!run.scoutable(offer.node));
    assert_eq!(
        brief_rows(&run, offer.node),
        vec![BriefRow::Enter, BriefRow::Back],
        "a facility with no room for the rule shows no price",
    );
}

/// **The rows never run into the footer**, and the picture above still has room to be a
/// map — the same geometry the list is held to, one screen over.
#[test]
fn the_last_row_keeps_a_blank_between_itself_and_the_footer() {
    assert!(
        H.saturating_sub(LIST_ROWS) > 0 && LIST_ROWS < H - 1,
        "the widest block ({LIST_ROWS} rows) does not fit above the footer",
    );
    // And the **tallest** block there is — every row, and a Vault's three crates listed
    // under one of them — still ends a blank clear of the footer.
    let tallest = block_rows(MAX_ROWS, MAX_CRATES);
    let last = H.saturating_sub(LIST_ROWS) + tallest - 3;
    assert!(
        last + 1 < H - 1,
        "the tallest block's last line ({last}) sits on the footer",
    );
    let run = at_a_choice_point_holding(8371, SCOUT_COST as usize);
    let node = open_offer(&run).node;
    let ui = MapUi::default().opening(node);
    let footer = &render_brief(W, H, &run, ui, node).to_text()[(H - 1) as usize];
    assert!(footer.contains(FOOTER));
    assert!(footer.contains("theme [n]"), "the theme keeps its corner");
}

/// **A bought manifest expands the brief in place** (#550): the row it was bought on
/// becomes the heading, and the crates are listed under it — on the same screen as the
/// price that revealed them and the row that raids the facility.
#[test]
fn a_bought_manifest_lists_the_crates_under_its_heading() {
    let mut run = at_a_choice_point_holding(8371, MANIFEST_COST as usize);
    let node = run
        .ahead()
        .into_iter()
        .find(|offer| run.manifest_on_sale(offer.node))
        .expect("a facility with crates on offer")
        .node;
    assert!(run.buy_manifest(node).paid());
    let crates = run.manifest(node).expect("the manifest is bought");
    assert!(!crates.is_empty(), "a facility on sale hides crates");

    let ui = MapUi::default().opening(node);
    let drawn = rows_of(&run, ui, node);
    let heading = drawn
        .iter()
        .position(|line| line.contains(MANIFEST_HEADING))
        .expect("the heading is drawn");
    for (i, tech) in crates.iter().enumerate() {
        let line = &drawn[heading + 1 + i];
        assert!(
            line.contains(tech.name()) && line.starts_with(CRATE_BULLET),
            "crate {i} reads {line:?}",
        );
    }
    // The way out is still the last row, under the list rather than buried in it.
    assert_eq!(drawn.last().map(String::as_str), Some(BACK_LABEL));
}

/// **A crate line is not a row** (#268/#550): the marker steps over it and a press on it is
/// swallowed, because there is nothing for either to do there. Three names that answered
/// taps would be three buttons that do nothing.
#[test]
fn the_marker_and_the_finger_both_step_over_the_crates() {
    let mut run = at_a_choice_point_holding(8371, MANIFEST_COST as usize);
    let node = run
        .ahead()
        .into_iter()
        .find(|offer| run.manifest_on_sale(offer.node))
        .expect("a facility with crates on offer")
        .node;
    assert!(run.buy_manifest(node).paid());
    let ui = MapUi::default().opening(node);

    let rows = brief_rows(&run, node).len();
    let lines = laid_out(&run, node, H);
    assert!(lines.len() > rows, "the manifest expanded the block");

    for (y, line) in lines {
        match line {
            Line::Crate(_) | Line::Note(_) | Line::Rule(_) => assert_eq!(
                map_hit(W, H, &run, ui, W / 2, y),
                None,
                "a caption line answered a press at row {y}",
            ),
            Line::Row(row) => assert_eq!(map_hit(W, H, &run, ui, W / 2, y), Some(row.hit(node))),
        }
    }

    // And walking the marker visits exactly the rows, crates skipped.
    let mut walked = Vec::new();
    let mut ui = ui;
    for _ in 0..rows {
        walked.push(ui.brief_row);
        ui = ui.next(&run);
    }
    assert_eq!(walked, (0..rows).collect::<Vec<usize>>());
}

/// **A facility with no crates is never offered the sale** (#550) — the row is absent, not
/// present-and-refusing. The flavour is visible when offered (§14 v3 **[SETTLED]**), so the
/// absence tells the player nothing the map had not already said.
#[test]
fn a_facility_with_no_crates_shows_no_manifest_row() {
    let run = at_a_choice_point_holding(8371, 99);
    for offer in run.ahead().into_iter().filter(|o| !o.locked) {
        let has_crates = run
            .map()
            .flavour(offer.node)
            .composite()
            .expansion()
            .caches
            .crates()
            > 0;
        let listed = brief_rows(&run, offer.node)
            .iter()
            .any(|row| matches!(row, BriefRow::Manifest { .. }));
        assert_eq!(
            listed, has_crates,
            "{:?} ({:?}) offers a manifest iff it hides crates",
            offer.node, offer.flavour,
        );
    }
}

/// A run standing on the **archive**, having walked the whole country and scored `stars`
/// on the way — the brief every assertion below is opened on.
fn at_the_archive(seed: u64, per_raid: u32) -> Campaign {
    let mut run = Campaign::new(seed);
    while !run.map().is_archive(run.node()) {
        run.enter();
        run.complete(&Verdict {
            ending: Ending::Escaped,
            stats: RunStats {
                // The three axes, switched on one at a time by `per_raid`: inside par,
                // never noticed, everything taken.
                turns: if per_raid >= 1 { 40 } else { 400 },
                par: 200,
                alert_peak: if per_raid >= 2 { 0 } else { 1 },
                intel: if per_raid >= 3 { 1 } else { 0 },
                intel_total: 1,
                held: Loadout::innate(),
                ..RunStats::default()
            },
        });
        let next = run.offers()[0].node;
        assert!(run.choose(next));
    }
    run
}

/// **The archive names the rules it is drawn, before the press that walks into it**
/// (§14 v3/#573) — *legible before the choice, not after*, at the one choice a run has
/// spent six facilities preparing for.
///
/// The rules are named in the **same captions the Level info tab will print** once the
/// player is inside, so the brief cannot come to describe the raid differently from the
/// panel that reports it.
#[test]
fn the_archive_brief_names_the_rules_the_gate_drew() {
    let run = at_the_archive(8371, 0);
    assert_eq!(run.stars(), 0, "a run that scored nothing");
    let node = run.node();
    let named = run.archive_rules();
    assert_eq!(named.len(), GATE_RULES_MAX);

    let ui = MapUi::default().opening(node);
    let rows = render_brief(W, H, &run, ui, node).to_text();
    assert!(
        rows.iter().any(|row| row.contains(GATE_HEADING)),
        "the gate has no heading: {rows:?}",
    );
    for rule in &named {
        assert!(
            rows.iter().any(|row| row.contains(&caption(rule))),
            "{:?} was drawn but never named: {rows:?}",
            rule.name,
        );
    }
    // And the level the press actually boots plays exactly those rules — the brief is a
    // receipt, not a claim (§2.3).
    let mut booting = run.clone();
    let level = booting.enter().expect("the archive is enterable");
    // The terminus as it would be with no gate on it — its own composite and the run's
    // §4.5 gate, and nothing else. What the booted level has beyond this is the gate's.
    let ungated = ModifierSources {
        chosen: LevelModifiers {
            intel_to_exit: IntelGate::All,
            ..LevelModifiers::default()
        },
        alert: None,
        flavour: Some(Composite::Archive.contribution()),
        score: None,
    }
    .resolve()
    .active();
    let played: Vec<_> = level
        .modifiers
        .active()
        .into_iter()
        .filter(|rule| !ungated.contains(rule))
        .collect();
    assert_eq!(played, named);
}

/// **A run that cleared every threshold is told so** (#573) — the gauge's one payout, and
/// it says the *reason* rather than showing an empty list the player has to interpret.
#[test]
fn a_cleared_gate_says_the_stars_paid_for_it() {
    let run = at_the_archive(8371, 3);
    assert!(
        run.archive_gate().rules() == 0,
        "the fixture has to clear the table: {} stars",
        run.stars(),
    );
    let node = run.node();
    assert!(run.archive_rules().is_empty());

    let ui = MapUi::default().opening(node);
    let rows = render_brief(W, H, &run, ui, node).to_text();
    assert!(
        rows.iter().any(|row| row.contains(GATE_CLEARED)),
        "a cleared gate said nothing at all: {rows:?}",
    );
    assert!(
        !rows.iter().any(|row| row.contains(GATE_HEADING)),
        "a heading with nothing under it: {rows:?}",
    );
}

/// **The two expansions can never share a screen** (#550/#573) — which is what makes the
/// brief's fixed block tall enough for both.
///
/// The archive hides no equipment caches (§14 v3), so its manifest is never on sale, so the
/// four-row shape and the four-caption shape are different briefs. If a future flavour
/// change gave the terminus a crate, this fails and `LIST_ROWS` is what has to grow.
#[test]
fn the_archive_never_offers_a_manifest_beside_its_gate() {
    for seed in [1_u64, 8371, 42] {
        let run = at_the_archive(seed, 0);
        let node = run.node();
        let rows = brief_rows(&run, node);
        assert!(
            !rows
                .iter()
                .any(|row| matches!(row, BriefRow::Manifest { .. })),
            "the archive offered a manifest",
        );
        assert!(rows.len() as u32 <= MAX_GATE_ROWS, "{rows:?}");
        // The block still fits the band the picture reserves for it, gate and all.
        let lines = laid_out(&run, node, H);
        let lowest = lines
            .iter()
            .map(|&(y, _)| y)
            .max()
            .expect("a brief has rows");
        assert!(lowest < H - 1, "the brief ran into the footer");
    }
}
