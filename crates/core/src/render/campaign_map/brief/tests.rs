//! What the facility brief owes a run (§11.1/§14 v3/#215): that picking a facility does
//! not raid it, that the price is drawn before it is charged, that a purchase changes the
//! screen it was made on, and that a sale the hub cannot honour is never offered.

use super::*;
use crate::ability::Loadout;
use crate::campaign::{Campaign, CampaignStage, Offer, Outlay, SCOUT_COST};
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

/// The brief's rows as the player reads them, trimmed, one string apiece.
fn rows_of(run: &Campaign, ui: MapUi, node: NodeId) -> Vec<String> {
    render_brief(W, H, run, ui, node)
        .to_text()
        .into_iter()
        .skip(H.saturating_sub(LIST_ROWS) as usize)
        .take((LIST_ROWS - 1) as usize) // the footer is not a row of the list
        .filter(|row| !row.trim().is_empty())
        .map(|row| row.trim().to_string())
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

/// **The brief says what the facility is, what scouting costs, and how to leave** — and
/// the rows are the same three whichever way they are reached.
#[test]
fn the_brief_offers_the_raid_the_scout_and_the_way_back() {
    let run = at_a_choice_point_holding(8371, SCOUT_COST as usize);
    let offer = open_offer(&run);
    let ui = MapUi::default().opening(offer.node);

    assert_eq!(
        brief_rows(&run, offer.node),
        vec![
            BriefRow::Enter,
            BriefRow::Scout { bought: false },
            BriefRow::Back,
        ],
    );
    assert_eq!(
        rows_of(&run, ui, offer.node),
        vec![
            format!("> Enter the {}", offer.flavour.label()),
            format!("Scout the facility — {SCOUT_COST} intel"),
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
        let text = grid.to_text()[row_of_brief(H, i) as usize].clone();
        let x = text
            .chars()
            .position(|c| !c.is_whitespace())
            .expect("a row") as u32;
        grid.get(x, row_of_brief(H, i)).fg
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
    let expected = [MapHit::Enter(node), MapHit::Scout(node), MapHit::Back];

    for (i, want) in expected.iter().enumerate() {
        let row = row_of_brief(H, i);
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
    let widest = MAX_ROWS as usize - 1;
    assert!(
        row_of_brief(H, widest) + 1 < H - 1,
        "the last of {MAX_ROWS} rows sits on the footer",
    );
    let run = at_a_choice_point_holding(8371, SCOUT_COST as usize);
    let node = open_offer(&run).node;
    let ui = MapUi::default().opening(node);
    let footer = &render_brief(W, H, &run, ui, node).to_text()[(H - 1) as usize];
    assert!(footer.contains(FOOTER));
    assert!(footer.contains("theme [n]"), "the theme keeps its corner");
}
