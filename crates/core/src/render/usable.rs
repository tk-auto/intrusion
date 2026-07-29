//! The **usable line** (§11.4): what you can act on from where you stand.
//!
//! One row directly under the near line, listing the bump affordances
//! ([`State::affordances`](crate::State::affordances)) adjacent to the player
//! right now — each with an arrow giving the bump's direction — or, when nothing
//! is adjacent, the innate-verb floor (#323). Like the rest of the render it is a
//! **pure derived function of state**, recomputed every frame with nothing to
//! clear (§11.1/§11.4).
//!
//! # The row is a compass (§11.4, #384)
//!
//! The arrow says a direction; the *layout* shows it. A **west** affordance draws
//! flush left, **north and south** draw centred, and an **east** affordance draws
//! flush right with its arrow **trailing** the words, so it points off the right
//! edge the way the west entry's arrow points off the left. The row becomes a tiny
//! compass around the player — press towards the words — which is what the packed
//! left-to-right list could never be: with two affordances the left-most entry was
//! as likely as not to be the one on your right.
//!
//! The aiming is layout only. What the entries *say*, their categories, the FOV
//! gate and the floor are all unchanged, and nothing hit-tests this row — it is
//! read-only chrome, so moving its glyphs cannot break a tap target (§11.4).

use super::*;
use crate::cell::Direction;
use crate::place::LevelConfig;
use crate::state::Affordance;

use super::hud::{status_row, InputModality};

/// The blank cell each side of the row's contents: [`status_row`]'s own one-cell
/// left margin, mirrored on the right so a flush-right group stops short of the
/// frame's edge instead of running into the corner.
const MARGIN: u32 = 1;

/// The air between two entries sharing the row, in cells — [`status_row`]'s gap,
/// used here to separate both the entries *within* a group and the groups from
/// each other, so an aimed row and a packed one breathe the same way.
const SEGMENT_GAP: u32 = 2;

/// The usable line's floor, on touch (§11.4/§11.6/#323): the two innate verbs in
/// the gesture vocabulary. The third gesture — a press held in place, which
/// repeats Wait — is deliberately unnamed: the hint is a floor, not a manual, and
/// the help panel's Help card is where the full set is read.
const TOUCH_HINT: [&str; 2] = ["swipe: move", "tap: wait"];

/// The usable line's floor, on keys (§11.4/§11.6/#323): the same two verbs off
/// §11.6's own table — the arrows the row already draws, and `w` to wait.
///
/// It names `w` alone rather than the `5/w` it used to (#369). The wait digit is the
/// **numpad**'s `5`, and a floor hint has no room to say which `5` it means — a
/// player reading it off a laptop and pressing the top row would get nothing at all.
/// `w` is the key that is there on every keyboard; the full spelling is one `?` away.
const KEYS_HINT: [&str; 2] = ["↑↓←→: move", "w: wait"];

/// How many **cells** a hint segment occupies: its `char` count, since every glyph
/// the hints use is one grid cell wide. Counts the UTF-8 lead bytes rather than
/// calling `chars`, so the budget below can be spent at compile time.
///
/// It is also the row's *only* width arithmetic (#384): the arrows are multi-byte,
/// so a placement counting `len()` would aim every group three cells wrong.
const fn hint_cells(text: &str) -> u32 {
    let bytes = text.as_bytes();
    let (mut i, mut cells) = (0, 0);
    while i < bytes.len() {
        // Every byte that is not a continuation byte (`10xxxxxx`) starts a char.
        if bytes[i] & 0xC0 != 0x80 {
            cells += 1;
        }
        i += 1;
    }
    cells
}

/// The width a two-segment hint draws to, in cells: the one-cell left margin, the
/// two segments, and the two spaces between them.
const fn hint_width(hint: [&str; 2]) -> u32 {
    MARGIN + hint_cells(hint[0]) + SEGMENT_GAP + hint_cells(hint[1])
}

/// **Both hints must fit the board they are drawn on**, the way the ability bar's
/// worst case does (#287): a hint clipped mid-word teaches nothing, and discovering
/// that in a screenshot is discovering it too late. Rewording either variant past
/// the v1 width (§10.2) fails the *build*, not the frame.
const _: () = assert!(
    hint_width(TOUCH_HINT) <= LevelConfig::V1.width
        && hint_width(KEYS_HINT) <= LevelConfig::V1.width,
    "the usable line's move/wait hint must fit the v1 board (§10.2): shorten a segment",
);

/// The usable line's **floor** (§11.4/#323): how to move and how to wait, in the
/// vocabulary the player's hands are using, drawn whenever there is no affordance
/// to offer instead.
///
/// The row is the one piece of permanent screen the HUD would otherwise give away
/// for nothing, and it sits directly above a board on which the player has to work
/// out unaided that **waiting is an action at all** — the only 360° look (§9.1),
/// the way a crouch is held (§10.3) and the way a cone is let past (§7.6). Wait has
/// no ability-bar entry by design (the bar is the ability *economy*, §8.3), so
/// without this the two innate verbs live on a row that never mentions them.
///
/// It is the same move the near line already makes one row up — ambient status
/// instead of an empty line (§11.4) — and it is a **floor, never a competitor**:
/// the moment anything is adjacent, the affordances take the row back whole. It is
/// not aimed either (#384): the verbs describe the keys, not the geometry, so they
/// keep the packed left-to-right shape they have always had.
///
/// The words draw in [`Owned`](Category::Owned) — *you, and the things you made*
/// (§11.2). [`Ground`](Category::Ground) was the first answer and it was the wrong
/// one on the screen: Ground's meaning is **absence**, drawn to recede so that
/// everything else pops against it, which is precisely the wrong instruction for a
/// row whose whole job is to be read. Owned says what these two verbs actually are
/// — not scenery, not something to bump, but *yours*, the pair you always hold —
/// and it puts them in the same blue as the ability bar's ready entries, so the
/// two surfaces that answer "what can I do right now" answer in one colour.
fn usable_hint(modality: InputModality) -> Vec<(String, Category)> {
    let hint = match modality {
        InputModality::Keys => KEYS_HINT,
        InputModality::Touch => TOUCH_HINT,
    };
    hint.iter()
        .map(|text| ((*text).to_string(), Category::Owned))
        .collect()
}

/// The usable line's direction glyph (§11.4): which way to bump for the
/// affordance beside it.
fn arrow(dir: Direction) -> char {
    match dir {
        Direction::North => '↑',
        Direction::East => '→',
        Direction::South => '↓',
        Direction::West => '←',
    }
}

/// Which of the row's three aimed groups a bump direction belongs to (#384): west
/// flush left, north and south centred, east flush right. The index is also the
/// left-to-right order the groups are placed in, which is what makes the collision
/// check in [`aimed_starts`] a single forward sweep.
fn group_of(dir: Direction) -> usize {
    match dir {
        Direction::West => 0,
        Direction::North | Direction::South => 1,
        Direction::East => 2,
    }
}

/// One affordance's words, arrow included (#384). The arrow **leads** everywhere
/// except the east, where it **trails** so that it points off the right edge — the
/// mirror of the west entry's arrow pointing off the left. Either way it is the
/// same glyph and the same width, so a group's arithmetic does not care which.
fn entry(dir: Direction, a: Affordance) -> (String, Category) {
    let text = match dir {
        Direction::East => format!("{} {}", a.label(), arrow(dir)),
        _ => format!("{} {}", arrow(dir), a.label()),
    };
    (text, a.category())
}

/// How wide a group of entries draws, in cells: the entries themselves plus a
/// [`SEGMENT_GAP`] between each pair. `0` for an empty group, which is how an
/// absent direction says so.
fn group_width(entries: &[(String, Category)]) -> u32 {
    let words: u32 = entries.iter().map(|(text, _)| hint_cells(text)).sum();
    words + SEGMENT_GAP * (entries.len().saturating_sub(1) as u32)
}

/// Where the west / centre / east groups start on a `width`-wide row (#384), given
/// each group's drawn width in cells (`0` for an absent one) — or `None` when they
/// cannot all be placed, which is the caller's cue to pack the row instead.
///
/// West sits behind the one-cell [`MARGIN`], east behind the same margin on the
/// right, and the centre group is centred on **the row** rather than on the gap
/// between its neighbours: the middle of the screen is where the player's own cell
/// is, so that is where a north/south arrow should point from.
///
/// The failure this returns `None` for is real, not theoretical: labels reach 21
/// cells (§11.4), so two long entries already overrun the 40-wide v1 board (§10.2).
/// Rather than clip a word or nudge a group off its aim — leaving a row that reads
/// as neither aimed nor packed — the whole line falls back to one deterministic
/// rule. Every column is checked against the margins and against the group before
/// it, so a `Some` is a promise the row fits and the drawing needs no clipping.
fn aimed_starts(width: u32, widths: [u32; 3]) -> Option<[u32; 3]> {
    let inner_end = width.checked_sub(MARGIN)?;
    let starts = [
        MARGIN,
        width.saturating_sub(widths[1]) / 2,
        inner_end.checked_sub(widths[2])?,
    ];
    // One forward sweep, west to east: each present group must clear the margins
    // and leave a gap behind the last one placed.
    let mut cursor = MARGIN;
    for (start, group) in starts.iter().zip(widths) {
        if group == 0 {
            continue;
        }
        let end = start.checked_add(group)?;
        if *start < cursor || end > inner_end {
            return None;
        }
        cursor = end + SEGMENT_GAP;
    }
    Some(starts)
}

/// Draw one group's entries into `cells` from column `start`, [`SEGMENT_GAP`]
/// between them. The caller has already proved the group fits ([`aimed_starts`]),
/// so this clips nothing — a write past the row would be a layout bug, not a long
/// message, and `debug_assert` is where it should surface.
fn draw_group(
    cells: &mut [GlyphCell],
    start: u32,
    entries: &[(String, Category)],
    blank: GlyphCell,
) {
    let mut x = start as usize;
    for (i, (text, category)) in entries.iter().enumerate() {
        if i > 0 {
            x += SEGMENT_GAP as usize;
        }
        for glyph in text.chars() {
            debug_assert!(
                x < cells.len(),
                "an aimed group must fit the row it was placed on"
            );
            if let Some(cell) = cells.get_mut(x) {
                *cell = GlyphCell {
                    glyph,
                    fg: *category,
                    ..blank
                };
            }
            x += 1;
        }
    }
}

/// Lay the whole usable line out as one row of grid cells (§11.4).
///
/// With nothing adjacent it is the innate-verb floor ([`usable_hint`], #323),
/// packed from the left. With affordances it is the **aimed** row (#384): west
/// flush left, north and south centred, east flush right with a trailing arrow —
/// falling back to the packed left-to-right list, arrows leading, when the groups
/// would not all fit. The row carries no band either way: it is status, not a
/// message (§11.4/§11.7).
pub(super) fn usable_row(
    width: u32,
    affordances: &[(Direction, Affordance)],
    modality: InputModality,
) -> Vec<GlyphCell> {
    if affordances.is_empty() {
        return status_row(width, 1, width, &usable_hint(modality), None);
    }

    // The three groups, each keeping the `Direction::ALL` order `affordances`
    // arrives in, so a row with several entries reads the same way every frame.
    let mut groups: [Vec<(String, Category)>; 3] = Default::default();
    for &(dir, a) in affordances {
        groups[group_of(dir)].push(entry(dir, a));
    }
    let widths = [
        group_width(&groups[0]),
        group_width(&groups[1]),
        group_width(&groups[2]),
    ];

    let Some(starts) = aimed_starts(width, widths) else {
        // Overrun: one rule, no half-aligned hybrid — today's packing, arrows
        // leading, clipped at the row's edge by `status_row` as it always was.
        let packed: Vec<(String, Category)> = affordances
            .iter()
            .map(|&(dir, a)| (format!("{} {}", arrow(dir), a.label()), a.category()))
            .collect();
        return status_row(width, 1, width, &packed, None);
    };

    let blank = GlyphCell::blank();
    let mut cells = vec![blank; width as usize];
    for (start, entries) in starts.iter().zip(&groups) {
        if !entries.is_empty() {
            draw_group(&mut cells, *start, entries, blank);
        }
    }
    cells
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The arithmetic on its own, which is where the interesting failures are: a
    /// golden row only ever shows the one case it was written for, while the
    /// placement can be asserted for every combination of widths.
    #[test]
    fn the_three_groups_sit_left_centre_and_right() {
        // West alone: flush left behind the margin.
        assert_eq!(aimed_starts(40, [10, 0, 0]), Some([1, 20, 39]));
        // East alone: flush right behind a margin of its own — it ends at 39.
        let starts = aimed_starts(40, [0, 0, 10]).unwrap();
        assert_eq!(starts[2], 29);
        // Centre alone: centred on the row, not on what is beside it.
        let starts = aimed_starts(40, [0, 20, 0]).unwrap();
        assert_eq!(starts[1], 10);
        // All three, comfortably: each one keeps its own aim.
        let starts = aimed_starts(40, [8, 8, 8]).unwrap();
        assert_eq!(starts, [1, 16, 31]);
    }

    /// The fallback is a real path, not a theoretical one: two long labels already
    /// overrun a 40-wide board (§10.2), and a group that would touch its neighbour
    /// is refused rather than nudged.
    #[test]
    fn groups_that_would_collide_or_overrun_are_refused() {
        // West runs into the centre group: no room for the gap between them.
        assert_eq!(aimed_starts(40, [16, 20, 0]), None);
        // The centre group runs into the east one.
        assert_eq!(aimed_starts(40, [0, 20, 12]), None);
        // A single group too wide for the row's margins.
        assert_eq!(aimed_starts(40, [0, 0, 40]), None);
        assert_eq!(aimed_starts(40, [39, 0, 0]), None);
        // A row too narrow to carry anything at all: no panic, just a refusal.
        assert_eq!(aimed_starts(0, [4, 0, 0]), None);
        assert_eq!(aimed_starts(1, [4, 0, 0]), None);
        // Exactly touching the margins is still a fit.
        assert!(aimed_starts(40, [38, 0, 0]).is_some());
    }

    /// Widths count **characters, not bytes** (#384): the arrows are three bytes
    /// each, so a group measured in bytes aims every entry wrong.
    #[test]
    fn a_groups_width_counts_cells_not_bytes() {
        let one = vec![entry(Direction::West, Affordance::OpenDoor)];
        assert_eq!(group_width(&one), hint_cells("← door: open"));
        assert_eq!(group_width(&one), 12);

        let pair = vec![
            entry(Direction::North, Affordance::TakeIntel),
            entry(Direction::South, Affordance::Hide),
        ];
        assert_eq!(
            group_width(&pair),
            hint_cells("↑ console: take intel") + SEGMENT_GAP + hint_cells("↓ cupboard: hide")
        );
    }

    /// The east entry's arrow **trails** the words so it points off the right edge;
    /// every other direction leads with it.
    #[test]
    fn the_east_entry_trails_its_arrow() {
        assert_eq!(
            entry(Direction::East, Affordance::OpenDoor).0,
            "door: open →"
        );
        assert_eq!(
            entry(Direction::West, Affordance::OpenDoor).0,
            "← door: open"
        );
        assert_eq!(
            entry(Direction::North, Affordance::Crouch).0,
            "↑ table: crouch"
        );
        assert_eq!(
            entry(Direction::South, Affordance::Crouch).0,
            "↓ table: crouch"
        );
        // The category is the affordance's own, whichever way the arrow points.
        assert_eq!(
            entry(Direction::East, Affordance::TakeIntel).1,
            Affordance::TakeIntel.category()
        );
    }

    /// The whole row, as text, for each shape it can take.
    fn row(width: u32, affordances: &[(Direction, Affordance)]) -> String {
        usable_row(width, affordances, InputModality::Keys)
            .iter()
            .map(|c| c.glyph)
            .collect()
    }

    /// The row is a compass around the player: press towards the words.
    #[test]
    fn each_entry_is_placed_where_its_arrow_points() {
        assert_eq!(
            row(40, &[(Direction::West, Affordance::OpenDoor)]),
            " ← door: open                           "
        );
        assert_eq!(
            row(40, &[(Direction::East, Affordance::OpenDoor)]),
            "                           door: open → "
        );
        assert_eq!(
            row(40, &[(Direction::North, Affordance::Crouch)]),
            "            ↑ table: crouch             "
        );
        // North and south are centred as one group, the row's own gap between them.
        assert_eq!(
            row(
                40,
                &[
                    (Direction::North, Affordance::Crouch),
                    (Direction::South, Affordance::Hide),
                ]
            ),
            "   ↑ table: crouch  ↓ cupboard: hide    "
        );
        // All four directions at once, each still aimed — on a row wide enough to
        // hold them. Four affordances do not fit the 40-wide v1 board (§10.2);
        // that is the packed fallback's case, asserted below.
        let all = row(
            80,
            &[
                (Direction::North, Affordance::Hide),
                (Direction::East, Affordance::OpenDoor),
                (Direction::South, Affordance::Crouch),
                (Direction::West, Affordance::EnterDuct),
            ],
        );
        assert!(all.starts_with(" ← duct: enter"), "{all:?}");
        assert!(all.ends_with("door: open → "), "{all:?}");
        assert!(all.contains("↑ cupboard: hide  ↓ table: crouch"), "{all:?}");
    }

    /// Nothing is ever clipped mid-word or dropped in aid of the aim: when the
    /// groups will not fit, the *whole* row falls back to today's packing — arrows
    /// leading, left to right — rather than half-aiming what happens to fit.
    #[test]
    fn an_overrun_row_falls_back_to_packing() {
        let crowded = [
            (Direction::North, Affordance::SilenceRadio),
            (Direction::East, Affordance::ExitRefused),
            (Direction::South, Affordance::StoreBody),
            (Direction::West, Affordance::Takedown),
        ];
        let packed = row(40, &crowded);
        assert!(
            packed.starts_with(" ↑ comms: silence radio  → exit: needs"),
            "{packed:?}"
        );
        assert_eq!(
            packed.chars().count(),
            40,
            "the row is still the full width"
        );

        // Two long entries are already enough to overrun the v1 board (§10.2), so
        // the packed path is what a real crowded cell gets.
        let two = [
            (Direction::West, Affordance::SilenceRadio),
            (Direction::East, Affordance::ExitRefused),
        ];
        assert!(
            row(40, &two).starts_with(" ← comms: silence radio  → exit"),
            "{:?}",
            row(40, &two)
        );
        // The same pair aims cleanly once the row is wide enough for both.
        let wide = row(60, &two);
        assert!(wide.starts_with(" ← comms: silence radio"), "{wide:?}");
        assert!(wide.ends_with("exit: needs the intel → "), "{wide:?}");
    }

    /// The floor is not aimed (#323/#384): it describes the keys, not the geometry,
    /// so it keeps the packed shape — and it only ever draws with nothing adjacent.
    #[test]
    fn the_floor_stays_packed_from_the_left() {
        for modality in [InputModality::Keys, InputModality::Touch] {
            let text: String = usable_row(40, &[], modality)
                .iter()
                .map(|c| c.glyph)
                .collect();
            assert!(text.starts_with(' '), "{modality:?}: {text:?}");
            assert_eq!(text.chars().count(), 40);
            assert!(text
                .trim_start()
                .starts_with(if modality == InputModality::Keys {
                    "↑↓←→: move"
                } else {
                    "swipe: move"
                }));
        }
    }

    /// The row is status, not a message: no band, and each entry keeps its own
    /// §11.2 category wherever the aim puts it.
    #[test]
    fn the_aimed_row_carries_no_band_and_keeps_each_category() {
        let cells = usable_row(
            40,
            &[
                (Direction::East, Affordance::TakeIntel),
                (Direction::West, Affordance::Crouch),
            ],
            InputModality::Keys,
        );
        assert!(cells.iter().all(|c| c.bg.is_none()), "no band on this row");
        assert!(cells.iter().all(|c| c.vis == Visibility::Live));
        assert_eq!(cells[1].glyph, '←');
        assert_eq!(cells[1].fg, Affordance::Crouch.category());
        let intel = cells
            .iter()
            .rposition(|c| c.glyph == '→')
            .expect("the east entry trails an arrow");
        assert_eq!(cells[intel].fg, Affordance::TakeIntel.category());
        assert_eq!(intel as u32, 40 - MARGIN - 1);
    }
}
