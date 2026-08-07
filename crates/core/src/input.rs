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
    /// rightward movement key (`→`, and the numpad's `6` folded onto the arrow),
    /// read here as "next".
    NextTab,
    /// Move to the previous tab, cycling — the leftward movement key (`←`, and the
    /// numpad's `4`).
    PrevTab,
    /// Flip the colour theme without leaving the panel (§11.2/#189) — the same
    /// [`UiCommand::ToggleTheme`] the board answers, re-offered here because the
    /// panel's colour key is the best thing on the screen to judge a theme against.
    /// The setting's *home* is the options screen ([`OpenSettings`](Self::OpenSettings),
    /// #513); this is the shortcut, and it is drawn nowhere on the panel any more.
    ToggleTheme,
    /// Move the marker to the previous row of the **Options** tab, wrapping
    /// (§14 v2/#513). It is the one tab whose rows are controls rather than reading, so
    /// it is the only one `↑`/`↓` mean anything on — every other tab swallows them, as
    /// the panel always did.
    PrevRow,
    /// Move the marker to the next row of the Options tab, wrapping.
    NextRow,
    /// **Fire the marked row** of the Options tab: flip the setting it names, or copy
    /// the run as a replay link. `Enter`/`Space`, the confirm keys §11.6 reserves — free
    /// here, because a modal panel has nothing else to confirm.
    Activate,
    /// Copy this run's **level-seed token** to the system clipboard (§13.1/#353) —
    /// the keyboard half of the Level info tab's `copy [c]` control, so the panel is
    /// reachable without a pointer and so is this (§11.6). The shell performs the
    /// write and mirrors the control exactly: a run whose panel draws no token has
    /// nothing to copy, and the key does nothing there, as the absent control does.
    CopySeed,
}

/// Map a key to the [`HelpNav`] it drives **while the help panel is open**, or
/// `None` for a key the modal panel simply swallows. The shell consults this
/// before every other table when [`ScreenUi::help_open`](crate::ScreenUi) is set:
/// the panel is modal (§14 v2/#248), so no key falls through to a game action or
/// another UI toggle while it is up.
///
/// The movement keys are re-read as tab motion — `→` next, `←` prev — so the same
/// left/right the board uses walks the tab bar; `Tab`, which binds to nothing in
/// game (#287), advances the tabs here. `?` and `Escape` both close, the two
/// conventional exits — and `n` flips the theme (#189), the one key the panel
/// forwards rather than swallows. The numpad reaches the tabs the way it reaches
/// everything else, by folding onto the arrows through [`key_for_code`]; no digit
/// *character* is listed, because a digit means a bar slot (#369), and no letter,
/// because §11.6's movement is arrows and numpad only (#368).
///
/// It took a `debug` argument while the panel had a Debug tab (#459): the two keys
/// whose controls lived there were offered only in a session that had it. Both keys
/// moved to the options screen with the tab in #513, so every key here is now offered
/// on every panel — which is the same rule stated the same way, that a key is offered
/// exactly where its control is drawn.
pub fn help_nav_for_key(key: &str) -> Option<HelpNav> {
    match key {
        "?" | "Escape" => Some(HelpNav::Close),
        "Tab" | "ArrowRight" => Some(HelpNav::NextTab),
        "ArrowLeft" => Some(HelpNav::PrevTab),
        // The one binding the modal panel does **not** swallow (#189): the theme
        // toggle reaches every modal screen, and this is the one whose colour key is
        // the best thing on screen to judge the flip against.
        "n" => Some(HelpNav::ToggleTheme),
        // The Options tab's own three (#513). They are bound for the whole panel rather
        // than per tab — a table cannot see which tab is up — and the shell mirrors the
        // drawn tab exactly, so on any other tab they are the no-ops they always were.
        "ArrowUp" => Some(HelpNav::PrevRow),
        "ArrowDown" => Some(HelpNav::NextRow),
        "Enter" | " " => Some(HelpNav::Activate),
        // `c` copies the run's level-seed token (#353). It is listed *here only* — the
        // panel is the one surface the token is drawn on, so a board-wide binding
        // would name a control that is not on screen, and leaving it off
        // `ui_command_for_key` also leaves the letter free for an ability mnemonic
        // (#360), which the modal panel swallows anyway while it is up.
        "c" => Some(HelpNav::CopySeed),
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
    /// Step back out of a sub-screen to the entry list. On the list itself there
    /// is nowhere further back — the menu *is* the root — so it does nothing there.
    Back,
    /// Nudge the level-options difficulty slider one stop **easier** (§12.6/#298).
    /// Named for what it does to the control rather than for the key's direction:
    /// `←` and a west swipe are two spellings of the same intent, and the slider
    /// clamps at its ends rather than wrapping.
    Easier,
    /// Nudge the difficulty slider one stop **harder**.
    Harder,
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
/// The vertical movement keys walk the list — `↑` up, `↓` down, the same spelling
/// the board takes (§11.6) — and `Enter`/`Space` and `Escape` finally do the
/// *confirm* and *cancel* jobs §11.6 reserved for them ("arrive with the first
/// menu"). This is that menu. As on the help panel, the numpad walks the list by
/// folding onto the arrows ([`key_for_code`]) rather than by a digit character,
/// which belongs to the bar (#369).
pub fn menu_nav_for_key(key: &str) -> Option<MenuNav> {
    match key {
        "ArrowUp" => Some(MenuNav::Prev),
        "ArrowDown" => Some(MenuNav::Next),
        // The horizontal pair drives the level-options slider (#298), and only it —
        // the entry list has nothing to nudge, so the shell drops them there. Same
        // spelling as everywhere else: `←` is towards the easier end.
        "ArrowLeft" => Some(MenuNav::Easier),
        "ArrowRight" => Some(MenuNav::Harder),
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

/// A navigation command on the **campaign map** (§14 v3/#208) — the surface a campaign
/// is played from between raids.
///
/// The same shape as [`MenuNav`], because it is the same shape of screen: a vertical
/// list of facilities, walked by `↑`/`↓` and fired by `Enter`. A player who has used the
/// title screen has already learned this one, which is most of why the map's list is a
/// list rather than something cleverer.
///
/// **`Back` closes the facility brief and nothing else** (#215). The map's own list is not
/// a panel you opened over something — between facilities it *is* the game, and there is
/// nothing underneath it to return to (§2.2: no retry, no snapshot, and the last facility
/// is gone) — so on the list the shell drops this. The brief is a sub-screen and does have
/// somewhere to go back to, which is the whole of what this command is for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MapNav {
    /// Move the marker to the previous row, wrapping.
    Prev,
    /// Move the marker to the next row, wrapping.
    Next,
    /// Fire the marked row — open a facility's brief, buy what it offers, or raid it, which
    /// is the one irreversible key on either screen (§2.1).
    Activate,
    /// **Leave the facility brief** for the map's list, changing nothing (#215).
    Back,
    /// Flip the colour theme (§11.2/#189), as on every other modal surface.
    ToggleTheme,
}

/// Map a key to the [`MapNav`] it drives **while the campaign map is up**, or `None` for
/// a key the modal screen swallows (§14 v3/#208).
pub fn map_nav_for_key(key: &str) -> Option<MapNav> {
    match key {
        "ArrowUp" => Some(MapNav::Prev),
        "ArrowDown" => Some(MapNav::Next),
        "Enter" | " " => Some(MapNav::Activate),
        // `Escape` leaves the **facility brief** (#215) and is dropped by the shell on the
        // map's own list, where there is nowhere back to: a key that looked like a way out
        // and did nothing would be worse than one that is plainly not there (§11.6's
        // no-trap rule read the other way round). The table is stateless, so which of the
        // two screens is up is the shell's to know.
        "Escape" => Some(MapNav::Back),
        "n" => Some(MapNav::ToggleTheme),
        _ => None,
    }
}

/// Map a gesture to the [`MapNav`] it drives (§11.6/#208) — the touch half of
/// [`map_nav_for_key`].
///
/// **A press is deliberately unbound**, for the reason [`menu_nav_for_gesture`] gives
/// and then some: entering a facility is not undoable in a permadeath game (§2.1), and
/// on this screen a stray tap would not merely start a run, it would spend the one
/// choice the map exists to offer. A facility is raided by pressing *its row*, and by
/// nothing else.
pub fn map_nav_for_gesture(gesture: Gesture) -> Option<MapNav> {
    match gesture {
        Gesture::Swipe(Direction::North) => Some(MapNav::Prev),
        Gesture::Swipe(Direction::South) => Some(MapNav::Next),
        _ => None,
    }
}

/// A navigation command on the **end screen** (§14 v2/#138) — the third modal
/// surface, and the narrowest: a finished run has nothing to step, so the only thing
/// left to do is choose a way on.
///
/// There is deliberately **no `Back`**. Escape closes a panel you opened; the end
/// screen is not one — it is what the run left behind, and there is nothing under it
/// to go back to. Every way on is a drawn row, by key and by finger alike (§11.6).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EndNav {
    /// Move the marker to the previous exit, wrapping.
    Prev,
    /// Move the marker to the next exit, wrapping.
    Next,
    /// Fire the marked exit — retry the level, roll a new run, or go to the menu.
    Activate,
}

/// Map a key to the [`EndNav`] it drives **while the end screen is up**, or `None`
/// for a key the modal screen swallows (§14 v2/#138).
///
/// The same spelling as the menu one table up, because it is the same shape of
/// screen: a vertical list of rows, walked by `↑`/`↓` (the numpad folding onto them
/// through [`key_for_code`]) and fired by `Enter`/`Space`. A player who has used the
/// title screen has already learned this one.
///
/// The theme toggle is **not** forwarded here, unlike on the menu and the help panel:
/// this screen's colours are its cause line's, and a key that recoloured the verdict
/// mid-read would be changing the evidence. It is a keypress away on either of the
/// screens this one leads to.
pub fn end_nav_for_key(key: &str) -> Option<EndNav> {
    match key {
        "ArrowUp" => Some(EndNav::Prev),
        "ArrowDown" => Some(EndNav::Next),
        "Enter" | " " => Some(EndNav::Activate),
        _ => None,
    }
}

/// Map a gesture to the [`EndNav`] it drives **while the end screen is up**
/// (§11.6/#336/#138) — the touch half of [`end_nav_for_key`].
///
/// The vertical swipes walk the list, exactly as they walk the menu's. **A press is
/// unbound**, for the menu's reason and more sharply: the exits start runs, one of
/// them by throwing away the level you were just reading, and a stray tap on the
/// board behind the panel must not do that. An exit fires by pressing *the exit*.
pub fn end_nav_for_gesture(gesture: Gesture) -> Option<EndNav> {
    match gesture {
        Gesture::Swipe(Direction::North) => Some(EndNav::Prev),
        Gesture::Swipe(Direction::South) => Some(EndNav::Next),
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
        // `Escape` is a plain **second spelling** of it (#551), not a command of its
        // own: the conventional key for *"show me what is going on"*, and the one a
        // hand reaches for when it wants this run's Level info (§12.6/#248). It costs
        // nothing to claim — the board bound it to nothing otherwise — and it makes the
        // key an honest toggle, since it already closes the panel ([`HelpNav::Close`]).
        // The one claim that outranks it is an open exchange's decline
        // ([`declines_exchange`], §8.3/#266), which the shell asks about first.
        "Escape" => Some(UiCommand::ToggleHelp),
        // `n` for *night* (#189). The obvious mnemonics were all spoken for — `t`
        // for theme is Takedown's, `d` for dark is Decoy's, and `l` for light is the
        // vim-east step, which a binding may never shadow (§11.6) — so the key goes
        // to the one word for the choice whose letter is free. Pinned against every
        // other table below, like each of them.
        "n" => Some(UiCommand::ToggleTheme),
        _ => None,
    }
}

/// The keys that are **not** a dismissal of the level-start splash (§11.6/#497): the
/// bare modifiers, which a browser reports as keydowns of their own.
///
/// Holding Shift before typing, tabbing away with Alt, reaching for Control — none of
/// those is the press the card is waiting for, which is *"yes, I have read it"*. Every
/// other key is, including the ones the game owns and the ones it does not: a card that
/// picked and chose would be a card the player has to guess at.
const SPLASH_HELD_KEYS: [&str; 6] = ["Shift", "Control", "Alt", "Meta", "AltGraph", "CapsLock"];

/// Whether `key` **dismisses the level-start splash** (§11.4/§11.6/#497) — every key
/// but the bare modifiers ([`SPLASH_HELD_KEYS`]).
///
/// The card is up before the first turn and carries no control of its own, so there is
/// nothing here to navigate: the only question a key can answer is *have you read it*,
/// and the shell **consumes** the answer rather than letting it fall through to a step,
/// an ability or a menu underneath. That consumption is the whole rule — the failure a
/// timeout would have caused (the press meant as a dismissal landing in the game as a
/// first move) arrives by this door too if the key is passed on.
///
/// It is a table here, beside the other §11.6 bindings, so *which* presses count is
/// pinned by a native test rather than discovered in a browser — and so nothing about it
/// can quietly become a clock, which the core has none of in any case (§12.1).
pub fn dismisses_splash(key: &str) -> bool {
    !SPLASH_HELD_KEYS.contains(&key)
}

/// Whether a gesture dismisses the level-start splash (§11.6/#336/#497) — the touch half
/// of [`dismisses_splash`], and **every** gesture there is.
///
/// A press, a swipe in any direction: the card has no list to walk and no control to
/// aim at, so a finger cannot miss it. This is the one screen where an unbound press
/// would be the trap rather than the safeguard — §11.6's no-trap rule is satisfied here
/// by *everything* working, not by one drawn `[x]`.
pub fn gesture_dismisses_splash(gesture: Gesture) -> bool {
    match gesture {
        Gesture::Swipe(_) | Gesture::Press => true,
    }
}

/// Whether `key` **declines an open exchange** (§8.3/§11.6/#266) — `Escape`, and
/// nothing else.
///
/// The exchange needs no navigation table of its own: its four candidates are the
/// ability bar's four slots, so the digits ([`ability_slot_for_code`]) and the mnemonic
/// letters already reach every one of them, the crate's own included — and discarding
/// *that* one **is** the decline. This is the conventional second spelling of it, for a
/// player whose hand reaches for `Escape` when a game asks a question, and the shell
/// resolves it to the very same [`Input::Discard`](crate::Input) rather than to a cancel
/// verb of its own.
///
/// **This claim is the prior one and outranks the board's** (§11.6/#551). `Escape` bound
/// to nothing here when the decline took it; it opens the help panel now
/// ([`ui_command_for_key`]), so what was an absence of competition is stated as a
/// precedence instead: an offer open makes the key the decline, and with no offer it is
/// the panel. The rule is the shell's to apply — it asks this before the UI table — and
/// the key goes back to the panel the moment the offer is answered.
///
/// There is deliberately no
/// gesture counterpart: a finger declines by pressing the entry the bar marks `(+)`,
/// which is a control that is *on screen*, and no swipe dismisses a decision (#336).
pub fn declines_exchange(key: &str) -> bool {
    key == "Escape"
}

/// Map a key (a browser `KeyboardEvent.key` name) to the [`Input`] it drives, or
/// `None` for a key the game does not own — which the shell must then leave to
/// the page (scrolling, browser shortcuts).
///
/// The §11.6 table: arrows move, `w` and `.` wait. Note `w` *waits* (§11.6): it is
/// not a WASD movement key, and no movement binding may ever claim it.
/// `Enter`/`Space` confirm and `Escape` cancel arrive with the first menu; the
/// abilities are on the digits and resolve by *position*, through
/// [`ability_slot_for_code`], before this table is consulted at all.
///
/// **The vi keys are not here** (#368). `h` `j` `k` `l` stepped for a while as
/// roguelike comfort, and what that cost was a quarter of the alphabet the ability
/// **mnemonics** could never claim: a movement key is off-limits to a mnemonic
/// (a mis-key ends a run, §2.2), so `Lock` could not have `l` in any loadout, alone
/// or not, with nothing on the bar to say why. §11.6's movement is the arrows and
/// the numpad; the letters are the abilities', and this table stays out of them.
///
/// **The digits are not here** (#369). §11.6's movement digits are the **numpad**'s
/// (#359), and the shell folds `Numpad4` and its siblings onto the arrow and wait
/// rows above through [`key_for_code`] before consulting this table — so a numpad
/// steps on any layout without the *character* `4` ever meaning a step. Listing it
/// here would put the two digit blocks back in one bucket and let a movement row
/// answer a press aimed at the bar's fourth slot.
pub fn input_for_key(key: &str) -> Option<Input> {
    Some(match key {
        "ArrowUp" => Input::Step(Direction::North),
        "ArrowDown" => Input::Step(Direction::South),
        "ArrowLeft" => Input::Step(Direction::West),
        "ArrowRight" => Input::Step(Direction::East),
        "w" | "." => Input::Wait,
        _ => return None,
    })
}

/// What a finger did, with **no surface's meaning attached** (§11.6/#336) — the
/// touch counterpart of a `KeyboardEvent.key` name.
///
/// The shell owns the arithmetic that produces one of these (the dead band, the
/// dominant axis, the four-way quantisation) because only the shell holds the live
/// displacement; what a gesture *means* is a binding, so it lives here beside the
/// key tables and is pinned by native tests like every other. That split is the
/// whole point: the pump's thresholds, repeat cadence and lift-stops-everything
/// guarantee are written once and inherited by every surface, instead of being
/// re-implemented the next time a screen wants touch.
///
/// Which is why there is no `Tap` variant. How long a press lasts changes *when*
/// it fires — the pump's business — never *what* it means, so a quick tap and a
/// press held in place are the same [`Press`](Self::Press) to every table below.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Gesture {
    /// A drag that travelled far enough to declare a heading, quantised to one of
    /// the four cardinals — movement has no diagonals (§4.1 **[SETTLED]**), and
    /// neither does any list this walks.
    Swipe(Direction),
    /// A press that stayed put: held in place, or lifted before it went anywhere.
    Press,
}

/// Map a gesture to the [`Input`] it drives **on the board** (§11.6/#336) — the
/// touch half of [`input_for_key`], and the only one of the three gesture tables
/// whose commands cost a turn.
///
/// A swipe steps its heading and a press waits, which is the pairing the arrows and
/// `w` already make on the keyboard. Both are bound: the board is the one surface
/// where standing still is an action (§9.1's 360° look), so unlike the modal screens
/// it has something for a held press to do.
pub fn input_for_gesture(gesture: Gesture) -> Option<Input> {
    Some(match gesture {
        Gesture::Swipe(direction) => Input::Step(direction),
        Gesture::Press => Input::Wait,
    })
}

/// Map a gesture to the [`MenuNav`] it drives **while the menu is up** (§14/#268,
/// #336) — the touch half of [`menu_nav_for_key`], and the reason a title screen
/// can be walked by finger at all.
///
/// The list is vertical, so it takes the vertical swipes: up is
/// [`Prev`](MenuNav::Prev), down is [`Next`](MenuNav::Next), the same spelling the
/// arrows have one table up. Wrapping and the skipping of disabled entries are the
/// *menu's* rules, not the gesture's — both commands go through the same handler
/// the keys do, so the two input paths cannot disagree the first time an entry is
/// disabled (§14 v2/v3 entries already are).
///
/// **A press is deliberately unbound.** Resolving it to
/// [`Activate`](MenuNav::Activate) would let a stray tap on empty menu space start
/// a run by accident — precisely the class of bug #306 shipped to close on the
/// board, and worse here, because starting a run is not undoable in a permadeath
/// game (§2.1). An entry is fired by pressing *the entry*, on the arm-on-press /
/// fire-on-lift path, and by nothing else.
pub fn menu_nav_for_gesture(gesture: Gesture) -> Option<MenuNav> {
    match gesture {
        Gesture::Swipe(Direction::North) => Some(MenuNav::Prev),
        Gesture::Swipe(Direction::South) => Some(MenuNav::Next),
        // The horizontal swipes set the level-options slider (#298), the touch twin
        // of `←`/`→` — the one control on the menu that is a *value* rather than a
        // choice, and the one a swipe reads naturally as nudging.
        Gesture::Swipe(Direction::West) => Some(MenuNav::Easier),
        Gesture::Swipe(Direction::East) => Some(MenuNav::Harder),
        _ => None,
    }
}

/// Map a gesture to the [`HelpNav`] it drives **while the help panel is open**
/// (§14 v2/#248, #336) — the touch half of [`help_nav_for_key`].
///
/// The tab bar is horizontal, so it takes the horizontal swipes: left is the
/// previous tab, right is the next, exactly as `←`/`→` read them. Closing is not
/// bound to a gesture — the panel carries its own `[x]`, which is what keeps the
/// touch path from ever trapping (§11.6), and a swipe that dismissed a modal by
/// accident would work against that rather than for it.
pub fn help_nav_for_gesture(gesture: Gesture) -> Option<HelpNav> {
    match gesture {
        Gesture::Swipe(Direction::West) => Some(HelpNav::PrevTab),
        Gesture::Swipe(Direction::East) => Some(HelpNav::NextTab),
        // The **vertical** swipes walk the Options tab's rows (#513), exactly as `↑`/`↓`
        // do — free to bind, because no tab had anything for them to do before. A row is
        // *fired* by pressing it, never by a swipe, and **a press stays unbound**: a
        // stray tap on empty panel must not flip a setting, nor lift the fog on a run
        // mid-raid (§11.6/appendix 21).
        Gesture::Swipe(Direction::North) => Some(HelpNav::PrevRow),
        Gesture::Swipe(Direction::South) => Some(HelpNav::NextRow),
        _ => None,
    }
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

    /// The §11.6 movement table, pinned: the arrows step, `w` and `.` wait. `w`
    /// waiting is the regression to watch — a WASD binding once claimed it, and
    /// §11.6 says it waits. The numpad reaches these same rows through
    /// [`key_for_code`], which is why no digit appears in the table itself (#369).
    #[test]
    fn the_movement_table_maps_per_the_design() {
        for (keys, expected) in [
            (&["ArrowUp"][..], Input::Step(Direction::North)),
            (&["ArrowDown"][..], Input::Step(Direction::South)),
            (&["ArrowLeft"][..], Input::Step(Direction::West)),
            (&["ArrowRight"][..], Input::Step(Direction::East)),
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
    ///
    /// The **vi keys** are here now (#368). They stepped for a while, and a movement
    /// key is one no ability mnemonic may claim — which cost `Lock` its `l` for a
    /// reason no player could see. Movement is the arrows and the numpad; a letter
    /// this table does not own is one the bar can.
    #[test]
    fn unowned_keys_are_left_to_the_page() {
        for key in ["q", "F5", "Meta", " ", "PageDown", "h", "j", "k", "l"] {
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
            assert_eq!(end_nav_for_key(&key), None, "{key:?} walks no exit list");
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
        for key in ["w", "5", "r", "ArrowUp", "Tab"] {
            assert_eq!(
                ui_command_for_key(key),
                None,
                "key {key:?} owns no UI command"
            );
        }
    }

    /// #551: `Escape` opens the help panel too — the **same** `UiCommand` as `?`, so
    /// there is one code path and the panel comes up on whichever tab it was last left
    /// on for either key. It stays a UI command and never an [`Input`], so no turn is
    /// spent and no guard moves while the panel is up (§4.4).
    #[test]
    fn escape_is_a_second_spelling_of_the_help_key() {
        assert_eq!(ui_command_for_key("Escape"), Some(UiCommand::ToggleHelp));
        assert_eq!(
            ui_command_for_key("Escape"),
            ui_command_for_key("?"),
            "one command, not two — nothing to keep in step",
        );
        assert_eq!(input_for_key("Escape"), None, "and never a game action");
        // The toggle is honest in both directions: the key that opens the panel is the
        // key that closes it, from any tab (§11.6's never-inescapable rule).
        assert_eq!(help_nav_for_key("Escape"), Some(HelpNav::Close));
        // The surfaces that deliberately refuse the key still do. The map answers it
        // with `Back`, which the shell drops on the map's own list, and the end screen
        // has no `Back` at all — neither reaches this new binding, because each is
        // consulted before the board's tables.
        assert_eq!(map_nav_for_key("Escape"), Some(MapNav::Back));
        assert_eq!(
            end_nav_for_key("Escape"),
            None,
            "no way back from a verdict"
        );
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
        for key in ["Tab", "ArrowRight"] {
            assert_eq!(
                help_nav_for_key(key),
                Some(HelpNav::NextTab),
                "{key:?} → next tab"
            );
        }
        assert_eq!(
            help_nav_for_key("ArrowLeft"),
            Some(HelpNav::PrevTab),
            "← → prev tab"
        );
        // `c` copies the run's level-seed token (#353) — a panel-only binding, so it is
        // here and *not* in the board's table: outside this panel there is nothing
        // drawn for it to name.
        assert_eq!(help_nav_for_key("c"), Some(HelpNav::CopySeed));
        assert_eq!(
            ui_command_for_key("c"),
            None,
            "`c` does nothing on the board",
        );
        // A movement/wait/ability/other-UI key is swallowed by the open modal panel —
        // the vi keys among them, now that they navigate nothing anywhere (#368).
        // `Enter` and the vertical arrows are **not** among them since #513: they walk
        // and fire the Options tab's rows (see the test below).
        for key in ["k", "j", "l", "h", "w", "5", "t", "m", "r", "v", "o"] {
            assert_eq!(
                help_nav_for_key(key),
                None,
                "{key:?} is swallowed while help is open"
            );
        }
    }

    /// **The Options tab's rows are walked from the panel's own table** (§14 v2/#513):
    /// the vertical pair moves the marker and `Enter`/`Space` fires it. Each was free to
    /// take — no tab had anything for them to do — so nothing was claimed from play to
    /// reach the settings.
    ///
    /// The two keys the Debug tab used to own, `r` and `v`, are **gone from every
    /// table** (#459 → #513): their controls are rows on this tab now, fired by `Enter`
    /// like every other row, so a session without them has no key that silently does
    /// nothing.
    #[test]
    fn the_options_tab_is_walked_from_the_panels_own_table() {
        assert_eq!(help_nav_for_key("ArrowUp"), Some(HelpNav::PrevRow));
        assert_eq!(help_nav_for_key("ArrowDown"), Some(HelpNav::NextRow));
        for key in ["Enter", " "] {
            assert_eq!(
                help_nav_for_key(key),
                Some(HelpNav::Activate),
                "{key:?} fires the marked row",
            );
        }
        // The vertical swipes do the same walk; a press stays unbound, so a stray tap
        // flips nothing (§11.6/appendix 21).
        assert_eq!(
            help_nav_for_gesture(Gesture::Swipe(Direction::North)),
            Some(HelpNav::PrevRow),
        );
        assert_eq!(
            help_nav_for_gesture(Gesture::Swipe(Direction::South)),
            Some(HelpNav::NextRow),
        );
        assert_eq!(help_nav_for_gesture(Gesture::Press), None);

        // The retired Debug-tab keys bind nowhere at all now.
        for key in ["r", "v"] {
            assert_eq!(help_nav_for_key(key), None, "{key:?} left the panel");
            assert_eq!(ui_command_for_key(key), None, "{key:?} is not a board key");
            assert_eq!(input_for_key(key), None, "{key:?} shadows no movement");
        }
    }

    /// #268: the menu is modal too — while it is up the shell routes keys through
    /// [`menu_nav_for_key`] first. The vertical movement keys walk the list, and
    /// `Enter`/`Space` and `Escape` do the confirm/cancel jobs §11.6 always reserved
    /// for "the first menu". Every other key is swallowed, so nothing of the game
    /// runs underneath the title screen.
    #[test]
    fn the_open_menu_captures_input_and_walks_the_list() {
        assert_eq!(menu_nav_for_key("ArrowUp"), Some(MenuNav::Prev), "↑ → up");
        assert_eq!(
            menu_nav_for_key("ArrowDown"),
            Some(MenuNav::Next),
            "↓ → down"
        );
        for key in ["Enter", " "] {
            assert_eq!(
                menu_nav_for_key(key),
                Some(MenuNav::Activate),
                "{key:?} confirms"
            );
        }
        assert_eq!(menu_nav_for_key("Escape"), Some(MenuNav::Back));
        // The horizontal pair sets the level-options slider (#298) — `←` towards the
        // easier end, the same spelling every other left/right on the menu has. The
        // *screen* decides whether there is anything to set: the shell drops them on
        // the entry list, which has no value on it to nudge.
        assert_eq!(
            menu_nav_for_key("ArrowLeft"),
            Some(MenuNav::Easier),
            "← eases"
        );
        assert_eq!(
            menu_nav_for_key("ArrowRight"),
            Some(MenuNav::Harder),
            "→ hardens"
        );
        // A key the game would otherwise own is swallowed by the open menu.
        for key in ["h", "j", "k", "l", "w", "5", "r", "t", "m", "?", "Tab"] {
            assert_eq!(
                menu_nav_for_key(key),
                None,
                "{key:?} is swallowed while the menu is up"
            );
        }
    }

    /// #138: the end screen is modal like the other two — while it is up the shell
    /// routes keys through [`end_nav_for_key`] first, and a finished run has nothing
    /// for anything else to do. The vertical keys walk the exits and `Enter`/`Space`
    /// fires one, the same spelling the menu's list takes.
    #[test]
    fn the_end_screen_captures_input_and_walks_its_exits() {
        assert_eq!(end_nav_for_key("ArrowUp"), Some(EndNav::Prev));
        assert_eq!(end_nav_for_key("ArrowDown"), Some(EndNav::Next));
        for key in ["Enter", " "] {
            assert_eq!(
                end_nav_for_key(key),
                Some(EndNav::Activate),
                "{key:?} fires the marked exit"
            );
        }
        // Escape is **not** bound: this screen is not a panel laid over something to
        // go back to — every way on is one of its own drawn rows. Nor is the theme
        // key, the one key the other two modal screens forward: the verdict's colours
        // are its cause line's, and recolouring the evidence mid-read is not a view
        // toggle worth having here.
        for key in ["Escape", "n", "?", "m", "w", "5", "h", "j", "Tab", "c"] {
            assert_eq!(
                end_nav_for_key(key),
                None,
                "{key:?} is swallowed while the run's verdict is up",
            );
        }
    }

    /// **`Escape` declines an open exchange, and binds to nothing else on the board**
    /// (§8.3/§11.6/#266).
    ///
    /// The offer needs no table of its own — its four candidates are the bar's four
    /// slots, so the digits and the mnemonics already reach every one of them — and this
    /// is the conventional second spelling of the one press that declines.
    ///
    /// It was safe to claim because `Escape` was unbound in play; since #551 the board
    /// opens the help panel with it, so the two are stated as a **precedence** instead
    /// (the shell asks this table first). The decline is the older claim and keeps the
    /// key while an offer is up — pinned here on the tables, and on the shell's ordering
    /// by `an_open_offer_outranks_the_help_key` in `intrusion-web`.
    #[test]
    fn escape_declines_an_open_exchange_and_nothing_else_does() {
        assert!(declines_exchange("Escape"));
        for key in ["w", ".", "n", "m", "?", "Enter", " ", "c", "ArrowUp", "5"] {
            assert!(!declines_exchange(key), "{key:?} is not the decline");
        }
        // It is still no step (a decline spends the turn the discard does, never a
        // move), and the one thing it now competes with is the panel it opens.
        assert_eq!(input_for_key("Escape"), None);
        assert_eq!(ui_command_for_key("Escape"), Some(UiCommand::ToggleHelp));
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

    /// Every single-character key a **UI command** claims (§11.4). Abilities take
    /// letters again (#360's mnemonics), but those are checked against these tables
    /// in [`crate::mnemonic`]; the collision this pair guards is the older one, a UI
    /// key that also moves. A mis-key that opened the help card instead of stepping
    /// is the same lost run as one that walked the wrong way.
    const UI_KEYS: [&str; 3] = ["m", "?", "n"];

    /// Every single-character key the movement table owns — just the two wait
    /// spellings now. No digit is among them (#369): the movement digits are the
    /// numpad's, and they arrive folded onto the arrow names, which are not
    /// characters at all. And no letter steps (#368): the vi keys left, so the
    /// alphabet they held is the ability mnemonics' to claim.
    const MOVEMENT_KEYS: [&str; 2] = ["w", "."];

    /// The UI keys hold their half of the bargain, including the theme toggle (#189)
    /// — which went to `n` when `t`, `d` and `l` were all spoken for, `l` by the vi
    /// step that has since left (#368). `n` stays: a binding is not re-shuffled just
    /// because a better letter came free.
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

    /// The touch bindings, pinned beside the key ones (§11.6/#336): the whole point
    /// of the tables living here is that a screen's two input paths can be read —
    /// and asserted — side by side, so neither drifts from the other.
    #[test]
    fn every_surface_binds_the_gesture_vocabulary_like_its_keys() {
        // The board: a swipe steps its heading, a press waits — the arrows and `w`.
        for direction in Direction::ALL {
            assert_eq!(
                input_for_gesture(Gesture::Swipe(direction)),
                Some(Input::Step(direction)),
            );
        }
        assert_eq!(input_for_gesture(Gesture::Press), Some(Input::Wait));

        // The menu: a vertical list, walked by the vertical swipes.
        assert_eq!(
            menu_nav_for_gesture(Gesture::Swipe(Direction::North)),
            menu_nav_for_key("ArrowUp"),
        );
        assert_eq!(
            menu_nav_for_gesture(Gesture::Swipe(Direction::South)),
            menu_nav_for_key("ArrowDown"),
        );

        // …and the horizontal swipes set the level-options slider (#298), the same
        // pair `←`/`→` set, so the dialog is driveable by finger and by key alike.
        assert_eq!(
            menu_nav_for_gesture(Gesture::Swipe(Direction::West)),
            menu_nav_for_key("ArrowLeft"),
        );
        assert_eq!(
            menu_nav_for_gesture(Gesture::Swipe(Direction::East)),
            menu_nav_for_key("ArrowRight"),
        );

        // The end screen: a vertical list of exits, walked like the menu's.
        assert_eq!(
            end_nav_for_gesture(Gesture::Swipe(Direction::North)),
            end_nav_for_key("ArrowUp"),
        );
        assert_eq!(
            end_nav_for_gesture(Gesture::Swipe(Direction::South)),
            end_nav_for_key("ArrowDown"),
        );

        // The help panel: a horizontal tab bar, walked by the horizontal swipes.
        assert_eq!(
            help_nav_for_gesture(Gesture::Swipe(Direction::West)),
            help_nav_for_key("ArrowLeft"),
        );
        assert_eq!(
            help_nav_for_gesture(Gesture::Swipe(Direction::East)),
            help_nav_for_key("ArrowRight"),
        );
    }

    /// **A press never activates a menu entry** (§2.1/#306/#336). Starting a run is
    /// not undoable in a permadeath game, so the one gesture that says nothing about
    /// what the player meant must not be the one that starts it — an entry fires by
    /// pressing the entry, and by nothing else. The same restraint on the help
    /// panel: no gesture dismisses a modal, which is what `[x]` is for.
    #[test]
    fn a_press_activates_nothing_on_a_modal_screen() {
        assert_eq!(menu_nav_for_gesture(Gesture::Press), None);
        assert_eq!(help_nav_for_gesture(Gesture::Press), None);
        // And on the end screen, where the same restraint bites hardest: two of its
        // three exits start a run, one of them by throwing away the level the player
        // is still reading. An exit fires by pressing the exit.
        assert_eq!(end_nav_for_gesture(Gesture::Press), None);
        for direction in [Direction::East, Direction::West] {
            assert_eq!(
                end_nav_for_gesture(Gesture::Swipe(direction)),
                None,
                "the exit list runs vertically and answers only that axis",
            );
        }
        // The horizontal swipes on the menu set a *value* (#298) and never fire a
        // control, so the restraint above survives the level-options dialog: no
        // gesture on the menu starts a run.
        for direction in [Direction::East, Direction::West] {
            assert!(
                matches!(
                    menu_nav_for_gesture(Gesture::Swipe(direction)),
                    Some(MenuNav::Easier | MenuNav::Harder),
                ),
                "a horizontal swipe may only move the slider",
            );
        }
        // The panel answers both axes since #513 — the horizontal pair walks its tab
        // bar, the vertical pair the Options tab's rows — and **neither fires
        // anything**: a row is activated by pressing *the row*, on the arm-on-press /
        // fire-on-lift path, which is what this restraint is really about.
        for direction in Direction::ALL {
            assert!(
                !matches!(
                    help_nav_for_gesture(Gesture::Swipe(direction)),
                    Some(HelpNav::Activate | HelpNav::Close),
                ),
                "no swipe on the panel may fire a control or dismiss it",
            );
        }
    }
}
