//! The §11.6 input mapping — **in the core so it is testable natively** (§12.1).
//!
//! A shell's input pump forwards raw key names here and feeds whatever comes back
//! to [`State::step`](crate::State::step); it never interprets a key itself, so
//! every binding is pinned by a native test instead of discovered in a browser.
//!
//! Keys arrive here two ways, and which way is itself a design decision (#359).
//! Most are matched on the **character** the layout produced (`w`, `h`, `?`) —
//! [`input_for_key`] and the tables beside it. A few bind to the **physical key**
//! instead, by its `KeyboardEvent.code`, because for those it is the position
//! under the finger that is the binding and not the character printed on it:
//! [`ability_slot_for_code`] fires the ability bar's slots off the top-row digits,
//! and [`key_for_code`] folds the numpad onto the §11.6 keys it means. An AZERTY
//! player's top row is `& é " '`, so a binding on the *character* `1` would want
//! Shift in the turn things go wrong.
//!
//! **No character table holds a digit** (#369). The two digit blocks mean different
//! things — the top row fires the bar, the numpad moves — and a character cannot
//! tell them apart, since both produce `"2"`. So the digits live only in the code
//! tables here, and the numpad folds onto the *names* of the keys it duplicates
//! (`ArrowDown`, `w`) rather than onto `"2"` and `"5"`. That is what stops a
//! movement row from answering a press meant for slot 2 — the shape of bug #369
//! reported, where the character table adjudicated a binding that meant a position.
//!
//! **Abilities are bound by bar slot, not by identity** (§11.6/§11.4). Each one
//! used to own a letter, keyed by identity so no reordering could ever move it —
//! but that made every new ability need a free letter it could keep forever, and
//! the catalogue grows (§8.3's salvaged tech) while the four keys a run can press
//! do not. `1`–`4` fire the first through fourth entries **of the bar as drawn**,
//! so the catalogue may grow without bound and the keyboard never notices. What
//! that trades away is cross-run constancy — `c` was Camouflage in every run ever
//! played and `1` is not — which is the deliberate half of the change; within a
//! run the loadout is fixed (§8.3) and the slots are on screen at all times, so a
//! digit is never ambiguous where it is pressed. The letters live on as the replay
//! script's notation ([`ability_script_letter`](crate::replay::ability_script_letter)),
//! which is where identity-keyed spelling was always the right answer.

use crate::ability::AbilityId;
use crate::cell::Direction;
use crate::state::Input;

/// A **shell-level** command a key drives that is *not* a game action (§11.4) —
/// it changes what the screen shows, never the world, so it never enters the turn
/// loop, costs no turn, and produces no [`Event`](crate::state::Event). Kept in
/// the core beside [`input_for_key`] so the binding is pinned by a native test
/// like every other, even though the state it toggles lives in the shell.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UiCommand {
    /// Deploy or dismiss the near line's full message list (§11.7). The near line
    /// always speaks the loudest live message; when more than one is live it shows
    /// a counter, and this expands the whole list and folds it back. The on-screen
    /// counter drives the same toggle for touch and mouse.
    ToggleMessageLog,
    /// Open or close the help overlay (§14 v2/#139): the glyph legend, colour key,
    /// and controls. A pure view toggle — no world change, no turn (§4.4) — so no
    /// guard moves while it is up. The header's `[?]` button drives the same toggle
    /// for touch and mouse.
    ToggleHelp,
    /// Flip the screen between the dark and the light colour table (§11.2/#189).
    /// Like the other two it is a pure view toggle — the world is untouched, no turn
    /// is spent (§4.4), and the *core* only ever moves a
    /// [`Theme`](crate::Theme) flag: which colours the flag names is presentation's
    /// business alone. The help panel drives the same toggle for touch and mouse,
    /// and answers the same key while it is up ([`HelpNav::ToggleTheme`]).
    ToggleTheme,
}

/// A navigation command inside the **open** help panel (§14 v2/#248) — distinct
/// from the [`UiCommand`] that *opens* it, because while the panel is up it is
/// **modal**: it captures input, so the shell routes keys here first and the game
/// never steps underneath. Closing and tab-switching are the only actions; every
/// other key is swallowed by the open panel.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HelpNav {
    /// Dismiss the panel — `?` (the toggle, still closing what it opened) or the
    /// conventional `Escape`. The panel is full-screen, so this is the escape path
    /// the header `[?]` button used to be (§11.6: never inescapable).
    Close,
    /// Move to the next tab (Level info → Legend → …), cycling — `Tab`, or the
    /// rightward movement keys (`→` / `l`, and the numpad's `6` folded onto the
    /// arrow), read here as "next".
    NextTab,
    /// Move to the previous tab, cycling — the leftward movement keys (`←` / `h`,
    /// and the numpad's `4`).
    PrevTab,
    /// Flip the colour theme without leaving the panel (§11.2/#189) — the same
    /// [`UiCommand::ToggleTheme`] the board answers, re-offered here because the
    /// panel is where the option lives until v2 grows an options screen, and its
    /// colour key is the best thing on the screen to judge a theme against.
    ToggleTheme,
}

/// Map a key to the [`HelpNav`] it drives **while the help panel is open**, or
/// `None` for a key the modal panel simply swallows. The shell consults this
/// before every other table when [`ScreenUi::help_open`](crate::ScreenUi) is set:
/// the panel is modal (§14 v2/#248), so no key falls through to a game action or
/// another UI toggle while it is up.
///
/// The movement keys are re-read as tab motion — `→`/`l` next, `←`/`h` prev — so
/// the same left/right the board uses walks the tab bar; `Tab`, which binds to
/// nothing in game (#287), advances the tabs here. `?` and `Escape` both close, the
/// two conventional exits — and `n` flips the theme (#189), the one key the panel
/// forwards rather than swallows. The numpad reaches the tabs the way it reaches
/// everything else, by folding onto the arrows through [`key_for_code`]; no digit
/// *character* is listed, because a digit means a bar slot (#369).
pub fn help_nav_for_key(key: &str) -> Option<HelpNav> {
    match key {
        "?" | "Escape" => Some(HelpNav::Close),
        "Tab" | "ArrowRight" | "l" => Some(HelpNav::NextTab),
        "ArrowLeft" | "h" => Some(HelpNav::PrevTab),
        // The one binding the modal panel does **not** swallow (#189): the theme
        // toggle lives on this panel, so it has to work with the panel up.
        "n" => Some(HelpNav::ToggleTheme),
        _ => None,
    }
}

/// A navigation command on the **title screen / main menu** (§14/#268) — the menu's
/// counterpart of [`HelpNav`], for the same reason: while it is up it is modal, so
/// the shell routes keys here first and nothing falls through to a game action. The
/// screen it drives lives in [`menu`](crate::render::menu).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MenuNav {
    /// Move the selection to the previous enabled entry, wrapping.
    Prev,
    /// Move the selection to the next enabled entry, wrapping.
    Next,
    /// Choose the selected entry — start the run, or open the seed prompt. A
    /// disabled entry (§14 v2/v3) does nothing.
    Activate,
    /// Step back out of the seed prompt to the entry list. On the list itself there
    /// is nowhere further back — the menu *is* the root — so it does nothing there.
    Back,
    /// Flip the colour theme from the title screen (§11.2/#189) — the same
    /// [`UiCommand::ToggleTheme`] the board and the help panel answer. The menu is
    /// the first thing a load puts on screen, so it is where a player who cannot
    /// comfortably read the current theme most needs to be able to change it, rather
    /// than having to start a run first.
    ToggleTheme,
}

/// Map a key to the [`MenuNav`] it drives **while the menu is up**, or `None` for a
/// key the modal menu swallows (§14/#268).
///
/// The vertical movement keys walk the list — `↑`/`k` up, `↓`/`j` down, the same
/// spelling the board takes (§11.6) — and `Enter`/`Space` and `Escape` finally do
/// the *confirm* and *cancel* jobs §11.6 reserved for them ("arrive with the first
/// menu"). This is that menu. As on the help panel, the numpad walks the list by
/// folding onto the arrows ([`key_for_code`]) rather than by a digit character,
/// which belongs to the bar (#369).
pub fn menu_nav_for_key(key: &str) -> Option<MenuNav> {
    match key {
        "ArrowUp" | "k" => Some(MenuNav::Prev),
        "ArrowDown" | "j" => Some(MenuNav::Next),
        "Enter" | " " => Some(MenuNav::Activate),
        "Escape" => Some(MenuNav::Back),
        // The theme toggle reaches every modal screen (#189), here as on the help
        // panel. The seed prompt is where it stops: `n` is an ordinary letter of a
        // level-seed token, and a key that retyped the screen's colours mid-token
        // would be a trap — the shell holds it back there (`apply_menu_nav`).
        "n" => Some(MenuNav::ToggleTheme),
        _ => None,
    }
}

/// Map a key to the [`UiCommand`] it drives, or `None` for a key that is not a UI
/// control. The shell consults this *before* [`input_for_key`]: a key claimed here
/// toggles view state and redraws without ever touching [`State`](crate::State).
///
/// `m` deploys the message list: a free letter (no movement key), mnemonic for
/// *messages*; `n` flips the colour theme (#189), a free letter for *night* mode.
/// `Tab` used to deploy the ability panel and no longer binds to anything (#287) —
/// the bar names every held ability on every frame, so there is no panel left to
/// toggle.
pub fn ui_command_for_key(key: &str) -> Option<UiCommand> {
    match key {
        "m" => Some(UiCommand::ToggleMessageLog),
        // `?` opens the help card (§14 v2/#139): the conventional roguelike help key,
        // a free character that collides with no movement key — and the ability keys
        // are digits now (#359), so it cannot collide with one of those either.
        "?" => Some(UiCommand::ToggleHelp),
        // `n` for *night* (#189). The obvious mnemonics were all spoken for — `t`
        // for theme is Takedown's, `d` for dark is Decoy's, and `l` for light is the
        // vim-east step, which a binding may never shadow (§11.6) — so the key goes
        // to the one word for the choice whose letter is free. Pinned against every
        // other table below, like each of them.
        "n" => Some(UiCommand::ToggleTheme),
        _ => None,
    }
}

/// Map a key (a browser `KeyboardEvent.key` name) to the [`Input`] it drives, or
/// `None` for a key the game does not own — which the shell must then leave to
/// the page (scrolling, browser shortcuts).
///
/// The §11.6 table: arrows move and `w` waits — plus the vi keys `h` `j` `k` `l`
/// and `.`-to-wait as roguelike comfort. Note `w` *waits* (§11.6): it is not a WASD
/// movement key, and no movement binding may ever claim it. `Enter`/`Space` confirm
/// and `Escape` cancel arrive with the first menu; the abilities are on the digits
/// and resolve by *position*, through [`ability_slot_for_code`], before this table
/// is consulted at all.
///
/// **The digits are not here** (#369). §11.6's movement digits are the **numpad**'s
/// (#359), and the shell folds `Numpad4` and its siblings onto the arrow and wait
/// rows above through [`key_for_code`] before consulting this table — so a numpad
/// steps on any layout without the *character* `4` ever meaning a step. Listing it
/// here would put the two digit blocks back in one bucket and let a movement row
/// answer a press aimed at the bar's fourth slot.
pub fn input_for_key(key: &str) -> Option<Input> {
    Some(match key {
        "ArrowUp" | "k" => Input::Step(Direction::North),
        "ArrowDown" | "j" => Input::Step(Direction::South),
        "ArrowLeft" | "h" => Input::Step(Direction::West),
        "ArrowRight" | "l" => Input::Step(Direction::East),
        "w" | "." => Input::Wait,
        _ => return None,
    })
}

/// The **physical** codes of the ability keys, in bar order: the top row's digits,
/// one per slot a run can hold ([`AbilityId::MAX_HELD`], §8.3).
///
/// Sized by the cap rather than written to length, so raising the held count is a
/// compile error here — a fifth slot with no key would be a silently unpressable
/// ability, and this is where that gets noticed.
const ABILITY_SLOT_CODES: [&str; AbilityId::MAX_HELD] = ["Digit1", "Digit2", "Digit3", "Digit4"];

/// The **ability bar slot** a physical key code fires (§11.6/§11.4, #359), or `None`
/// for a code that is not one of the four — counting from `0` at the bar's leftmost
/// *drawn* entry, as [`ability_in_slot`](crate::ability_in_slot) resolves it.
///
/// This is the keyboard half of the resolution a pointer tap also drives
/// ([`ability_at`](crate::ability_at)): both land on a slot and both turn that slot
/// into an ability through the one function, so a digit and the entry under the
/// thumb can never name different abilities (§11.4).
///
/// It takes a `KeyboardEvent.code`, not a character, because the binding **is** the
/// position: the key left of `2` fires the bar's first entry whether the layout
/// prints `1`, `&` or `"` on it. And it stops at the slot deliberately — which
/// ability sits there, and whether the key activates or deactivates it (§4.4's free
/// toggle, #304), are facts about live state that this pure table has no business
/// knowing.
pub fn ability_slot_for_code(code: &str) -> Option<usize> {
    ABILITY_SLOT_CODES.iter().position(|c| *c == code)
}

/// The key a **physical** key stands for when its position is what binds, or `None`
/// for a code the character tables can read straight off `KeyboardEvent.key`.
///
/// Only the numpad folds (#359). Its digits are the §11.6 movement rows — `8` `2`
/// `4` `6` step and `5` waits — and a numpad is the same shape under every layout,
/// so binding them by code is what keeps that path working where the character
/// tables would need a modifier. The top row does not fold: its `1`–`4` belong to
/// [`ability_slot_for_code`], and a shell resolves those first.
///
/// **It folds onto the arrows and `w`, not onto `8` `2` `4` `6` `5`** (#369). The
/// fold has to name the binding in a spelling the character tables hold, and the
/// digit characters are exactly the spelling the top row *also* produces — folding
/// there would put the numpad and the bar back in the one bucket #359 split them
/// out of, which is how a top-row `2` came to step south instead of firing slot 2.
/// The arrow names are unambiguous: no key on the top row produces them.
///
/// The shell substitutes the folded key before consulting *any* character table, so
/// the numpad walks the board, the help panel's tabs and the menu's list with the
/// one fold rather than a code table per screen.
pub fn key_for_code(code: &str) -> Option<&'static str> {
    Some(match code {
        "Numpad8" => "ArrowUp",
        "Numpad2" => "ArrowDown",
        "Numpad4" => "ArrowLeft",
        "Numpad6" => "ArrowRight",
        "Numpad5" => "w",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The §11.6 movement table, pinned: arrows and vi keys step; `w` and `.` wait.
    /// `w` waiting is the regression to watch — a WASD binding once claimed it, and
    /// §11.6 says it waits. The numpad reaches these same rows through
    /// [`key_for_code`], which is why no digit appears in the table itself (#369).
    #[test]
    fn the_movement_table_maps_per_the_design() {
        for (keys, expected) in [
            (&["ArrowUp", "k"][..], Input::Step(Direction::North)),
            (&["ArrowDown", "j"][..], Input::Step(Direction::South)),
            (&["ArrowLeft", "h"][..], Input::Step(Direction::West)),
            (&["ArrowRight", "l"][..], Input::Step(Direction::East)),
            (&["w", "."][..], Input::Wait),
        ] {
            for key in keys {
                assert_eq!(input_for_key(key), Some(expected), "key {key:?}");
            }
        }
    }

    /// Keys the game does not own pass through untouched, so the page keeps its
    /// scrolling and shortcuts. `Tab` is *not* here: it is a UI control
    /// ([`ui_command_for_key`]), not a game action, but it still returns `None`
    /// from [`input_for_key`] — the two tables are disjoint.
    #[test]
    fn unowned_keys_are_left_to_the_page() {
        for key in ["q", "F5", "Meta", " ", "PageDown"] {
            assert_eq!(input_for_key(key), None, "key {key:?}");
        }
    }

    /// #369's invariant, and the one worth keeping: **no character table holds a
    /// digit.** A digit character cannot say which of the two blocks produced it —
    /// the top row means a bar slot, the numpad means a step — so any table matching
    /// on `"2"` is adjudicating a binding it cannot see, and whichever table is asked
    /// first wins. Asserted over the whole keypad rather than the four that broke, so
    /// a digit re-entering *any* of these tables fails here instead of in a chase.
    #[test]
    fn no_character_table_claims_a_digit() {
        for digit in '0'..='9' {
            let key = digit.to_string();
            assert_eq!(input_for_key(&key), None, "{key:?} is not a movement key");
            assert_eq!(ui_command_for_key(&key), None, "{key:?} owns no UI command");
            assert_eq!(
                help_nav_for_key(&key),
                None,
                "{key:?} navigates no help tab"
            );
            assert_eq!(menu_nav_for_key(&key), None, "{key:?} walks no menu list");
        }
        // …and the numpad fold cannot smuggle one back in: it names the arrows and
        // the wait key, the spellings the top row can never produce.
        for code in ["Numpad8", "Numpad2", "Numpad4", "Numpad6", "Numpad5"] {
            let folded = key_for_code(code).expect("the numpad folds");
            assert!(
                !folded.chars().any(|c| c.is_ascii_digit()),
                "{code} folds onto {folded:?}, a spelling the top row also produces",
            );
        }
    }

    /// The UI-command table (§11.4/§11.7): `m` deploys the message list, `?` the
    /// help card and `n` the colour theme, and all three are *shell* commands, never
    /// a game [`Input`] — so `input_for_key` stays `None` for them and no toggle
    /// enters the turn loop. Being UI keys, they also own no ability activation.
    /// Other keys own no UI command — including `Tab`, which stopped binding to
    /// anything when the ability bar started naming every held ability (#287) and
    /// there was no longer a panel to deploy.
    #[test]
    fn the_ui_keys_toggle_their_panels_and_are_not_game_inputs() {
        assert_eq!(ui_command_for_key("m"), Some(UiCommand::ToggleMessageLog));
        // `?` opens the help card (§14 v2/#139) — a view toggle, so it never steps
        // the world: no turn passes and no guard moves while help is up (§4.4).
        assert_eq!(ui_command_for_key("?"), Some(UiCommand::ToggleHelp));
        // `n` flips dark/light (§11.2/#189) — the same kind of pure view toggle, and
        // the only one of the three the *open* help panel forwards rather than
        // swallows, because the panel is where the option lives.
        assert_eq!(ui_command_for_key("n"), Some(UiCommand::ToggleTheme));
        assert_eq!(help_nav_for_key("n"), Some(HelpNav::ToggleTheme));
        for key in ["m", "?", "n"] {
            assert_eq!(input_for_key(key), None, "{key:?} is not a game action");
        }
        for key in ["w", "5", "r", "ArrowUp", "Escape", "Tab"] {
            assert_eq!(
                ui_command_for_key(key),
                None,
                "key {key:?} owns no UI command"
            );
        }
    }

    /// #248: while the help panel is open it is **modal** — the shell routes keys
    /// through [`help_nav_for_key`] first. `?`/`Escape` close it, `Tab` and the
    /// rightward/leftward keys switch tabs, and every other key is swallowed (`None`)
    /// so the game never steps underneath the open card.
    #[test]
    fn the_open_help_panel_captures_input_and_switches_tabs() {
        for key in ["?", "Escape"] {
            assert_eq!(
                help_nav_for_key(key),
                Some(HelpNav::Close),
                "{key:?} closes"
            );
        }
        for key in ["Tab", "ArrowRight", "l"] {
            assert_eq!(
                help_nav_for_key(key),
                Some(HelpNav::NextTab),
                "{key:?} → next tab"
            );
        }
        for key in ["ArrowLeft", "h"] {
            assert_eq!(
                help_nav_for_key(key),
                Some(HelpNav::PrevTab),
                "{key:?} → prev tab"
            );
        }
        // A movement/wait/ability/other-UI key is swallowed by the open modal panel.
        for key in ["k", "j", "w", "5", "r", "t", "m", "Enter"] {
            assert_eq!(
                help_nav_for_key(key),
                None,
                "{key:?} is swallowed while help is open"
            );
        }
    }

    /// #268: the menu is modal too — while it is up the shell routes keys through
    /// [`menu_nav_for_key`] first. The vertical movement keys walk the list, and
    /// `Enter`/`Space` and `Escape` do the confirm/cancel jobs §11.6 always reserved
    /// for "the first menu". Every other key is swallowed, so nothing of the game
    /// runs underneath the title screen.
    #[test]
    fn the_open_menu_captures_input_and_walks_the_list() {
        for key in ["ArrowUp", "k"] {
            assert_eq!(menu_nav_for_key(key), Some(MenuNav::Prev), "{key:?} → up");
        }
        for key in ["ArrowDown", "j"] {
            assert_eq!(menu_nav_for_key(key), Some(MenuNav::Next), "{key:?} → down");
        }
        for key in ["Enter", " "] {
            assert_eq!(
                menu_nav_for_key(key),
                Some(MenuNav::Activate),
                "{key:?} confirms"
            );
        }
        assert_eq!(menu_nav_for_key("Escape"), Some(MenuNav::Back));
        // A key the game would otherwise own is swallowed by the open menu.
        for key in ["ArrowLeft", "h", "w", "5", "r", "t", "m", "?", "Tab"] {
            assert_eq!(
                menu_nav_for_key(key),
                None,
                "{key:?} is swallowed while the menu is up"
            );
        }
    }

    /// #359's binding, pinned: the top row's four digits are the bar's four slots,
    /// left to right, and nothing else is. The codes matter more than the order —
    /// they are what makes the binding a *position* rather than a character.
    #[test]
    fn the_top_row_digits_are_the_bar_slots_in_order() {
        for (code, slot) in [("Digit1", 0), ("Digit2", 1), ("Digit3", 2), ("Digit4", 3)] {
            assert_eq!(ability_slot_for_code(code), Some(slot), "{code}");
        }
        // A fifth digit is not a slot — a run holds at most four (§8.3) — and neither
        // is a numpad digit (those step, §11.6) nor any named key.
        for code in ["Digit5", "Digit0", "Numpad1", "Numpad4", "KeyC", "ArrowUp"] {
            assert_eq!(ability_slot_for_code(code), None, "{code} fires no slot");
        }
    }

    /// The keys the bar can be driven by are exactly the slots a run can hold
    /// (§8.3): four keys, forever, however far the catalogue grows past them.
    #[test]
    fn there_is_one_ability_key_per_held_slot() {
        let slots: Vec<usize> = ABILITY_SLOT_CODES
            .iter()
            .filter_map(|code| ability_slot_for_code(code))
            .collect();
        assert_eq!(slots, (0..AbilityId::MAX_HELD).collect::<Vec<_>>());
        assert!(
            AbilityId::ALL.len() > AbilityId::MAX_HELD,
            "the catalogue already outgrows the keys — which is the point (#359)",
        );
    }

    /// The numpad binds by **position** (#359): each of its movement keys folds onto
    /// the §11.6 key it duplicates — the arrows, and `w` for wait — so the same
    /// physical key steps on QWERTY, AZERTY and Dvorak alike. The fold target is the
    /// arrow *name* rather than the digit character (#369), which is what keeps the
    /// numpad and the ability bar in separate buckets.
    #[test]
    fn the_numpad_folds_onto_the_movement_keys() {
        for (code, key, expected) in [
            ("Numpad8", "ArrowUp", Input::Step(Direction::North)),
            ("Numpad2", "ArrowDown", Input::Step(Direction::South)),
            ("Numpad4", "ArrowLeft", Input::Step(Direction::West)),
            ("Numpad6", "ArrowRight", Input::Step(Direction::East)),
            ("Numpad5", "w", Input::Wait),
        ] {
            assert_eq!(key_for_code(code), Some(key), "{code}");
            assert_eq!(input_for_key(key), Some(expected), "{code} → {key}");
        }
        // And it reaches the modal screens through the same fold, which is why they
        // need no digit rows of their own: the numpad walks the menu's list and the
        // help panel's tabs by arriving as an arrow.
        assert_eq!(
            menu_nav_for_key(key_for_code("Numpad2").unwrap()),
            Some(MenuNav::Next)
        );
        assert_eq!(
            menu_nav_for_key(key_for_code("Numpad8").unwrap()),
            Some(MenuNav::Prev)
        );
        assert_eq!(
            help_nav_for_key(key_for_code("Numpad6").unwrap()),
            Some(HelpNav::NextTab)
        );
        assert_eq!(
            help_nav_for_key(key_for_code("Numpad4").unwrap()),
            Some(HelpNav::PrevTab)
        );
        // The top row does not fold: its digits are the bar's, and a letter key needs
        // no folding at all — the character it produced is the binding.
        for code in ["Digit1", "Digit4", "Digit8", "KeyW", "ArrowUp"] {
            assert_eq!(key_for_code(code), None, "{code} is read as a character");
        }
    }

    /// The two digit paths are **disjoint**, which is the collision #359 had to
    /// resolve: no code both fires a slot and folds to a movement key, so a press is
    /// never both a step and an ability.
    #[test]
    fn no_key_is_both_an_ability_slot_and_a_movement_digit() {
        for code in ABILITY_SLOT_CODES {
            assert_eq!(key_for_code(code), None, "{code} is the bar's alone");
        }
        for code in ["Numpad8", "Numpad2", "Numpad4", "Numpad6", "Numpad5"] {
            assert_eq!(
                ability_slot_for_code(code),
                None,
                "{code} moves, so it fires no slot",
            );
        }
    }

    /// Every single-character key a **UI command** claims (§11.4). Abilities no
    /// longer take letters (#359), so the collision that mattered is the one left:
    /// a UI key must never also be a movement key. A mis-key that opened the help
    /// card instead of stepping is the same lost run as one that walked the wrong way.
    const UI_KEYS: [&str; 3] = ["m", "?", "n"];

    /// Every single-character key the movement table owns. No digit is among them
    /// (#369): the movement digits are the numpad's, and they arrive folded onto the
    /// arrow names, which are not characters at all.
    const MOVEMENT_KEYS: [&str; 6] = ["w", "k", "j", "h", "l", "."];

    /// The UI keys hold their half of the bargain, including the theme toggle (#189),
    /// which had to go to `n` precisely because `t`, `d` and `l` were already spoken
    /// for.
    #[test]
    fn the_ui_keys_collide_with_no_movement_key() {
        for key in UI_KEYS {
            assert!(
                ui_command_for_key(key).is_some(),
                "{key:?} owns a UI command"
            );
            assert!(!MOVEMENT_KEYS.contains(&key), "{key:?} is a movement key");
        }
    }
}
