//! The help panel's **Abilities** tab (§11.4/§8.2/§8.3, #343): what each of the
//! run's abilities actually *does*.
//!
//! The ability bar tells a player an ability's state and the Legend tells them the
//! standing controls, but until this tab nothing anywhere told them what a thing
//! **does**. A run is handed three salvaged tech it has never seen (§8.3), so the
//! only way to tell Dephase from Confusion was to spend a turn firing one and
//! watch what happened. This is where that is read instead — while the game is
//! paused, which is what the modal panel is for.
//!
//! Each entry completes the chain the bar starts: **bar name → hotkey → full name →
//! behaviour → price**.
//!
//! - The **full name** (§8.3) — the one the near line's messages speak.
//! - The `hotkey / bar name` pairing — the two things a player is actually holding
//!   in their head ("the `Camo` down there, what fires it?"). This moved here from
//!   the Legend's controls card (#296), which now keeps only the standing keys.
//! - The **economy line**, derived from the [`Ability`](crate::ability::Ability)
//!   definition — turn cost, duration, cooldown, uses per level (§8.2).
//! - The **blurb**, [`AbilityId::blurb`] — behaviour only, never numbers, so a
//!   retuned `[START]` value moves the panel on its own (§11.3).
//!
//! **Held abilities only** (§11.4), in [`Loadout::iter`]'s ability-bar slot order —
//! never [`AbilityId::ALL`]. A run holds at most [`AbilityId::MAX_HELD`] of the
//! catalogue's eight, and a tab that named the other four would advertise tech this
//! run has not got. A codex of everything that exists is a different feature and
//! belongs with the salvaged-tech caches (#209).

use super::{ability_keys_column_start, CONTENT_INDENT};
use crate::ability::{AbilityId, Loadout};
use crate::category::Category;
use crate::place::LevelConfig;
use crate::render::{draw, Grid};

/// The column the blurb and economy line hang from — one further in than the name,
/// so an entry reads as a heading with its detail indented under it.
const DETAIL_INDENT: u32 = CONTENT_INDENT + 2;

/// How wide a wrapped detail line may be on the narrowest board a real run renders
/// on (the v1 board, 40 — §10.2), leaving the same one-column right margin the rest
/// of the panel keeps.
///
/// [`draw`] clamps rather than asserts, so an over-long line would be silently
/// truncated — the failure mode that put `…one guard conver` in a screenshot
/// (see `CAPTION_MAX`). Wrapping to this bound is what stops it, and
/// [`every_blurb_fits_the_board`] is what stops a new blurb slipping past.
const DETAIL_WIDTH: usize = (LevelConfig::V1.width - DETAIL_INDENT - 1) as usize;

/// The most rows one ability's blurb may wrap to. Four entries have to share the
/// panel with the tab bar and the footer (§11.4's height budget), and the tab has no
/// paging — so a blurb that will not say it in four lines is a blurb to shorten, not
/// a reason to add scrolling.
///
/// The draw **clamps** to it, like every other bound on this panel, so an over-long
/// blurb can never push an entry through the footer; [`every_blurb_fits_the_board`]
/// is what stops one ever getting that far and losing its last sentence in silence.
const BLURB_MAX_ROWS: usize = 4;

/// Draw the **Abilities** tab: one entry per ability the run holds, in ability-bar
/// slot order (§11.4).
pub(super) fn draw_abilities(grid: &mut Grid, mut y: u32, loadout: Loadout) {
    draw(grid, 2, y, "ABILITIES", Category::System);
    y += 2;

    let mut any = false;
    for id in loadout.iter() {
        any = true;
        y = draw_entry(grid, y, id);
    }
    if !any {
        // No run boots this — a loadout always carries its innate set (§8.3) — but a
        // hand-built state can, and an empty tab reads as a broken panel rather than
        // as an answer. Legible, like the Level info tab's "none active".
        draw(grid, CONTENT_INDENT, y, "none held", Category::Ground);
    }
}

/// One ability's entry, returning the row the next one starts on: the name and its
/// key pairing on one row, the economy line under it, then the blurb, then a blank
/// row of air.
fn draw_entry(grid: &mut Grid, mut y: u32, id: AbilityId) -> u32 {
    // Owned — "you and your things" (§11.2). An ability *is* one of your things, and
    // drawing the name in the player's own colour is what separates the four headings
    // from the prose hanging under them at a glance.
    draw(grid, CONTENT_INDENT, y, id.name(), Category::Owned);
    // System, the HUD-control colour: this half is the key you press, exactly as it
    // read on the controls card it came from (#287/#296).
    draw(
        grid,
        ability_keys_column_start(),
        y,
        &ability_row_keys(id),
        Category::System,
    );
    y += 1;

    draw(grid, DETAIL_INDENT, y, &economy_line(id), Category::Ground);
    y += 1;

    for line in wrap(id.blurb(), DETAIL_WIDTH)
        .into_iter()
        .take(BLURB_MAX_ROWS)
    {
        draw(grid, DETAIL_INDENT, y, &line, Category::Neutral);
        y += 1;
    }
    y + 1
}

/// An ability's **economy line** (§8.2) — the price, derived from the catalogue's own
/// [`Economy`](crate::ability::Economy) rather than written into the blurb beside it,
/// so retuning a `[START]` number updates the panel for free and the prose can never
/// contradict the game.
///
/// A **passive** (#264) states no clock, because it has none: its whole price is the
/// loadout slot it occupies, permanently (§8.2), and printing `0 active · 0 cooling`
/// would be a fiction of exactly the kind §8.2's timing trap forbids.
fn economy_line(id: AbilityId) -> String {
    let Some(economy) = id.def().economy() else {
        return "always on — the slot is the price".to_string();
    };

    let mut parts = vec![turns(economy.cost())];
    // Duration 0 is not "no time active", it is **instant** (§8.3: Confusion, Pierce
    // Wall) — fired from the cell you press it in, with no window to switch off. A
    // bare `0 active` would read as a bug.
    parts.push(match economy.duration() {
        0 => "instant".to_string(),
        n => format!("{n} active"),
    });
    // No cooldown at all is a real declaration (Pierce Wall's scarcity is its budget,
    // §8.2/#303), so it is stated by absence rather than as `0 cooling`.
    if economy.cooldown() > 0 {
        parts.push(format!("{} cooling", economy.cooldown()));
    }
    // What a level *grants*, which is the number the bar can never show — the bar
    // shows what is left (§8.2/#302).
    if let Some(uses) = economy.uses_per_level() {
        parts.push(format!("{uses} a level"));
    }
    parts.join(" · ")
}

/// The turn cost, pluralised — the one segment of the economy line that needs its
/// unit spelled out, because it is the only one that is not read off the bar too.
fn turns(n: u32) -> String {
    match n {
        1 => "1 turn".to_string(),
        n => format!("{n} turns"),
    }
}

/// An ability's key column (#287): the key that fires it and the
/// [bar name](AbilityId::bar_name) it appears under, so the two are read as one fact.
/// A **passive** has no key, so it shows its bar entry as the bar draws it —
/// `Sight (on)` — which is the thing on screen the row is there to explain.
///
/// A **per-level use budget** (§8.2/#302) is stated here too — `b / Bore (3/level)` —
/// because it is the other half of "what does this key do", and it is the half a
/// player cannot work out from the bar: the bar shows what is *left*, and only this
/// panel says what a level ever grants. The two numbers differ the moment the first
/// use is spent and that is not a contradiction — one is the supply, the other the
/// remainder.
pub(super) fn ability_row_keys(id: AbilityId) -> String {
    if id.is_passive() {
        return format!("{} {}", id.bar_name(), crate::ability::PASSIVE_MARKER);
    }
    activated_row_keys(
        id.hotkey(),
        id.bar_name(),
        id.def().economy().and_then(|e| e.uses_per_level()),
    )
}

/// The activated half of [`ability_row_keys`], as a pure function of the three things
/// it prints — so the widest entry a legal catalogue can produce can be measured
/// against the column even while no shipping ability declares both a long name and a
/// budget.
fn activated_row_keys(hotkey: char, bar_name: &str, uses_per_level: Option<u32>) -> String {
    let keys = format!("{hotkey} / {bar_name}");
    match uses_per_level {
        Some(uses) => format!("{keys} ({uses}/level)"),
        None => keys,
    }
}

/// Greedy word wrap to `width` **cells**, breaking on whitespace.
///
/// Counts `char`s, not bytes: the blurbs carry `°` and `—` (§8.3's Vision reads
/// "360°"), and a byte count would wrap them early. A word longer than `width` gets a
/// line of its own and is left to [`draw`]'s clamp — [`no_blurb_word_outruns_a_line`]
/// is what guarantees the catalogue never has one.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let len = word.chars().count();
        if line.is_empty() {
            line.push_str(word);
        } else if line.chars().count() + 1 + len <= width {
            line.push(' ');
            line.push_str(word);
        } else {
            lines.push(std::mem::take(&mut line));
            line.push_str(word);
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::help::tests::{render_tab, text_of, H};
    use crate::render::help::HelpTab;

    /// A three-ability loadout: innate Run plus two tech, which is what a run that
    /// took its grant one short looks like.
    fn held(ids: [AbilityId; 3]) -> Loadout {
        ids.into_iter().fold(Loadout::empty(), Loadout::with)
    }

    /// The rendered tab for a run holding exactly `id`. Every catalogue-wide check
    /// goes through this rather than through [`Loadout::full`]: eight entries is
    /// twice [`AbilityId::MAX_HELD`] and does not fit the screen (`full` is explicitly
    /// not a loadout a run can hold), so a sweep over `ALL` on one grid would be
    /// asserting against a tab the game never draws.
    fn tab_holding(id: AbilityId) -> String {
        text_of(&render_tab(HelpTab::Abilities, Loadout::empty().with(id)))
    }

    /// The tab lists exactly the **held** abilities, in bar order, and an ability the
    /// run does not hold appears nowhere on it (§11.4) — the whole point of deriving
    /// from the loadout rather than from [`AbilityId::ALL`].
    #[test]
    fn the_tab_lists_the_held_abilities_and_only_those() {
        let loadout = held([AbilityId::Run, AbilityId::Decoy, AbilityId::Vision]);
        let text = text_of(&render_tab(HelpTab::Abilities, loadout));

        for id in [AbilityId::Run, AbilityId::Decoy, AbilityId::Vision] {
            assert!(text.contains(id.name()), "the tab names {}", id.name());
            assert!(
                text.contains(&ability_row_keys(id)),
                "…under {:?}",
                ability_row_keys(id),
            );
        }
        for id in [
            AbilityId::Camouflage,
            AbilityId::Dephase,
            AbilityId::Autodoors,
            AbilityId::Confusion,
            AbilityId::PierceWall,
        ] {
            assert!(
                !text.contains(id.name()),
                "{} is not held, so it is not on the tab",
                id.name(),
            );
        }

        // Bar order (§11.4): the entries read down in the order the bar reads across.
        let (run, decoy, vision) = (
            text.find(AbilityId::Run.name()).expect("Run is listed"),
            text.find(AbilityId::Decoy.name()).expect("Decoy is listed"),
            text.find(AbilityId::Vision.name())
                .expect("Vision is listed"),
        );
        assert!(run < decoy && decoy < vision, "entries follow bar order");
    }

    /// Every ability in the catalogue has a blurb — the drift guard (§11.3). The
    /// exhaustive match in [`AbilityId::blurb`] makes omitting one a build failure;
    /// this makes an empty one a test failure.
    #[test]
    fn every_ability_has_a_blurb() {
        for id in AbilityId::ALL {
            let blurb = id.blurb();
            assert!(!blurb.trim().is_empty(), "{} has a blurb", id.name());
            assert!(
                !blurb.contains("  "),
                "{}'s blurb has a doubled space — a line-continuation slip",
                id.name(),
            );
        }
    }

    /// The blurb says what an ability **does**; the economy line says what it costs.
    /// A number written into the prose would be a second source for a `[START]` value
    /// and would start lying the moment that value was tuned (§8.2/§11.3).
    #[test]
    fn no_blurb_writes_the_economy_numbers_out() {
        for id in AbilityId::ALL {
            let Some(economy) = id.def().economy() else {
                continue;
            };
            for n in [economy.duration(), economy.cooldown()] {
                // Zero is not a tuned number here — it *is* "instant" / "no cooldown",
                // which the economy line states in words.
                if n == 0 {
                    continue;
                }
                assert!(
                    !id.blurb().contains(&n.to_string()),
                    "{}'s blurb writes {n} out; the economy line derives it",
                    id.name(),
                );
            }
        }
    }

    /// The economy line is **derived** (§8.2): every number on it comes from the
    /// ability's own definition, so a tuning change updates the panel for free.
    #[test]
    fn the_economy_line_derives_every_number() {
        for id in AbilityId::ALL {
            let line = economy_line(id);
            let Some(economy) = id.def().economy() else {
                continue;
            };
            assert!(
                line.contains(&turns(economy.cost())),
                "{line:?} states cost"
            );
            if economy.duration() == 0 {
                assert!(line.contains("instant"), "{line:?} reads instant");
            } else {
                assert!(
                    line.contains(&format!("{} active", economy.duration())),
                    "{line:?} states duration",
                );
            }
            assert_eq!(
                line.contains("cooling"),
                economy.cooldown() > 0,
                "{line:?} states a cooldown exactly when there is one",
            );
            assert_eq!(
                line.contains("a level"),
                economy.uses_per_level().is_some(),
                "{line:?} states a use budget exactly when there is one",
            );
        }
    }

    /// A **passive** (#264) reads as a passive: no key to press, and no clock it does
    /// not have. Its price is the loadout slot, and the line says so.
    #[test]
    fn a_passive_states_its_slot_price_and_no_clock() {
        assert_eq!(ability_row_keys(AbilityId::Vision), "Sight (on)");
        let line = economy_line(AbilityId::Vision);
        assert!(line.contains("always on") && line.contains("slot"));
        for clock in ["turn", "active", "cooling", "a level"] {
            assert!(!line.contains(clock), "a passive claims no {clock:?}");
        }
    }

    /// The panel joins the bar's short name to the key that fires it and to the full
    /// name the messages speak (#287) — the whole reason it is safe for the bar to
    /// show neither the letter nor the long name. Moved here from the Legend's
    /// controls card, which no longer carries ability rows (#296).
    #[test]
    fn the_entries_explain_the_bar_names() {
        assert_eq!(ability_row_keys(AbilityId::Camouflage), "c / Camo");
        assert_eq!(ability_row_keys(AbilityId::Run), "r / Run");
        for (id, keys, name) in [
            (AbilityId::Camouflage, "c / Camo", "Camouflage"),
            (AbilityId::Vision, "Sight (on)", "Vision"),
            (AbilityId::Autodoors, "a / Doors", "Autodoors"),
        ] {
            let text = tab_holding(id);
            assert!(text.contains(keys), "the tab shows {keys:?}");
            assert!(text.contains(name), "…against {name:?}");
        }
    }

    /// The entries carry each **activated** ability's settled §11.6 hotkey, straight
    /// from [`AbilityId`] — so the key shown is the key that activates it, and cannot
    /// drift. Moved here from the Legend (#296), which is no longer where hotkeys for
    /// abilities are read off.
    #[test]
    fn the_entries_carry_the_real_ability_hotkeys() {
        for id in AbilityId::ALL {
            let keys = ability_row_keys(id);
            assert!(
                tab_holding(id).contains(&keys),
                "the tab must list {} under {keys:?}",
                id.name(),
            );
            if !id.is_passive() {
                assert!(
                    keys.starts_with(id.hotkey()),
                    "{keys:?} leads with the real hotkey",
                );
            }
        }
    }

    /// A **per-level use budget** is stated (§8.2/#302) — the number the bar can never
    /// show, because the bar shows what is left and this shows what a level grants.
    #[test]
    fn a_use_budget_is_stated_on_the_key_and_the_economy() {
        assert_eq!(
            activated_row_keys('b', "Bore", Some(3)),
            "b / Bore (3/level)"
        );
        assert_eq!(activated_row_keys('b', "Bore", None), "b / Bore");
        for id in AbilityId::ALL.into_iter().filter(|id| !id.is_passive()) {
            let declared = id
                .def()
                .economy()
                .and_then(|e| e.uses_per_level())
                .is_some();
            assert_eq!(
                ability_row_keys(id).contains("/level"),
                declared,
                "{} states its budget exactly when it has one",
                id.name(),
            );
        }
    }

    /// The keys column has a width budget like everything else on a 40-wide board
    /// (§11.4). The name column runs until the keys start and the keys run until the
    /// right margin, and the widest pair a legal catalogue can produce — the longest
    /// full name, and the longest bar name plus the widest single-digit budget (§8.2's
    /// fence) — has to leave a gutter rather than run off the grid.
    #[test]
    fn the_widest_entry_heading_fits_the_board() {
        let longest_name = AbilityId::ALL
            .into_iter()
            .map(|id| id.name().chars().count())
            .max()
            .expect("the catalogue is not empty");
        let name_column = (ability_keys_column_start() - CONTENT_INDENT) as usize;
        assert!(
            longest_name < name_column,
            "the longest name is {longest_name} cells and the column has {name_column}",
        );

        let longest_bar_name = AbilityId::ALL
            .into_iter()
            .map(|id| id.bar_name().chars().count())
            .max()
            .expect("the catalogue is not empty");
        let widest = activated_row_keys('x', &"W".repeat(longest_bar_name), Some(9));
        let right_margin = LevelConfig::V1.width - 1;
        assert!(
            ability_keys_column_start() + widest.chars().count() as u32 <= right_margin,
            "{widest:?} runs past the board's right margin",
        );
    }

    /// Every blurb wraps inside the content indent at the v1 board width, in no more
    /// rows than the tab's height budget allows — the guard the ticket asked for,
    /// because [`draw`] clamps silently and an over-long line would simply vanish off
    /// the right of the grid.
    #[test]
    fn every_blurb_fits_the_board() {
        for id in AbilityId::ALL {
            let lines = wrap(id.blurb(), DETAIL_WIDTH);
            assert!(
                lines.len() <= BLURB_MAX_ROWS,
                "{}'s blurb wraps to {} rows, over the {BLURB_MAX_ROWS} the tab budgets",
                id.name(),
                lines.len(),
            );
            for line in &lines {
                assert!(
                    line.chars().count() <= DETAIL_WIDTH,
                    "{line:?} is {} cells and the column has {DETAIL_WIDTH}",
                    line.chars().count(),
                );
            }
            assert!(
                economy_line(id).chars().count() <= DETAIL_WIDTH,
                "{}'s economy line does not fit the column",
                id.name(),
            );
        }
    }

    /// No blurb contains a word too long to wrap — the one input the greedy wrap
    /// cannot rescue, and the one that would reach [`draw`]'s silent clamp.
    #[test]
    fn no_blurb_word_outruns_a_line() {
        for id in AbilityId::ALL {
            for word in id.blurb().split_whitespace() {
                assert!(
                    word.chars().count() <= DETAIL_WIDTH,
                    "{word:?} in {}'s blurb cannot be wrapped",
                    id.name(),
                );
            }
        }
    }

    /// A full loadout of [`AbilityId::MAX_HELD`] worst-case entries still clears the
    /// footer on the v1 screen (§11.4's height budget). This is the tab that will need
    /// scrolling before the others do; the answer while it fits is to keep the entries
    /// tight, and this is what says when that stops being enough.
    #[test]
    fn the_fullest_tab_clears_the_footer() {
        let mut ids: Vec<AbilityId> = AbilityId::ALL.into_iter().collect();
        ids.sort_by_key(|id| std::cmp::Reverse(wrap(id.blurb(), DETAIL_WIDTH).len()));
        let loadout = ids
            .into_iter()
            .take(AbilityId::MAX_HELD)
            .fold(Loadout::empty(), Loadout::with);

        let grid = render_tab(HelpTab::Abilities, loadout);
        let rows = grid.to_text();
        let last_content = rows
            .iter()
            .rposition(|row| !row.trim().is_empty())
            .expect("the tab drew something");
        assert_eq!(
            last_content,
            (H - 1) as usize,
            "the footer is the last non-blank row, so nothing overran it",
        );
        // …and the row above the footer is blank, so the entries stopped short of it.
        assert!(
            rows[(H - 2) as usize].trim().is_empty(),
            "the fullest tab still leaves air above the footer",
        );
    }

    /// The wrap helper itself: greedy on whitespace, counting cells rather than bytes
    /// so the multi-byte `°` and `—` the blurbs carry do not wrap a line early.
    #[test]
    fn wrap_breaks_on_words_and_counts_cells() {
        assert_eq!(wrap("one two three", 7), vec!["one two", "three"]);
        assert_eq!(wrap("", 10), Vec::<String>::new());
        // Nine characters, fifteen bytes: a byte count would split it.
        assert_eq!(wrap("360° — full", 11), vec!["360° — full"]);
    }
}
