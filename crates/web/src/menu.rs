//! The shell half of the title screen (§11.4/§14, #268): the menu's input and the
//! runs it starts.
//!
//! The screen itself is drawn by the core ([`intrusion_core::render_screen`] hands
//! the frame to the menu while [`ScreenUi::menu`] is set), so what lives here is
//! only what the core cannot own: browser listeners and the boot path a chosen
//! entry runs.
//!
//! # There is no markup here any more (#572)
//!
//! One thing on this screen used to be DOM: a `<input id="seed-input">` with *play*
//! and *back* buttons, floating over a band the core's seed prompt left blank, so a
//! player could type a level-seed token in. It was there because **a canvas cannot
//! raise a phone's keyboard**, and it brought three event fences with it — a key
//! swallow, an Enter-on-button special case and a pointer-down stop — to keep the
//! panel's own input away from the document-level pumps (§11.6).
//!
//! It is gone because **sharing is the URL** (§13.1): the Level info tab's `copy [c]`
//! hands over a `…#seed=<token>` link, which the person on the other end opens rather
//! than transcribes. Nothing is left to type, so the box, its buttons, its fences, the
//! surface it floated over and the entry that opened it all went together — and the
//! whole game is the character grid again, with no exception outside the debug
//! session. What this module still does with the DOM is set one attribute
//! ([`set_screen`]) and listen; it creates and reads no elements at all.

use std::cell::RefCell;
use std::rc::Rc;

use intrusion_core::{
    Difficulty, LevelSeed, MenuEntry, MenuHit, MenuNav, MenuScreen, MenuUi, OptionsControl,
    RunMode, RunOptions, ScreenUi,
};
use wasm_bindgen::prelude::*;
use web_sys::Document;

use crate::{seed, Game};

/// What `<body data-screen>` reads on each of the shell's surfaces — the one signal
/// outside the canvas that says which surface is up, which is what lets the headless
/// smoke check (the artifact-build skill's `verify.mjs`) tell a title screen from a
/// live run. It reveals nothing: since #572 the page has no chrome to reveal.
const SCREEN_ATTR: &str = "data-screen";
const SCREEN_MENU: &str = "menu";
const SCREEN_OPTIONS: &str = "options";
/// The **global settings screen** (§14 v2/#513) — named apart from `options`, which is
/// the *pre-run* level dialog (#298), for the reason the two screens are apart at all.
pub(crate) const SCREEN_SETTINGS: &str = "settings";
pub(crate) const SCREEN_PLAY: &str = "play";
/// The campaign map (§14 v3/#208), named apart from `play` so the headless smoke
/// check can tell a map from a board.
pub(crate) const SCREEN_MAP: &str = "map";

/// The view state a fresh load opens on: the menu's entry list, with the marker on
/// the row a player is most likely to want.
///
/// `resumable` says whether the shell found a run to continue (§12.5/#514). When it
/// did, *Continue run* is listed **and** marked: someone with an interrupted run came
/// back for it, so resuming is one keypress from the load and starting a fresh run
/// over it stays a deliberate move down the list. Otherwise the screen is the one it
/// has always been, Quick play selected.
pub(crate) fn opening_ui(resumable: bool) -> ScreenUi {
    ScreenUi {
        menu: Some(MenuUi {
            continue_run: resumable,
            selected: if resumable {
                MenuEntry::ContinueRun
            } else {
                MenuEntry::default()
            },
            ..MenuUi::default()
        }),
        ..ScreenUi::default()
    }
}

/// Mirror the current surface onto `<body data-screen>`. Best-effort and purely
/// informational: nothing on the page is styled off it since #572, so a page whose
/// body is somehow unavailable simply plays on with a stale attribute.
pub(crate) fn set_screen(screen: &str) {
    if let Some(body) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.body())
    {
        let _ = body.set_attribute(SCREEN_ATTR, screen);
    }
}

/// The `data-screen` value for the menu surface `menu` is showing — the one place that
/// maps a [`MenuScreen`] to its name, so raising and leaving the options screen over the
/// menu restores exactly the value the menu set for itself.
pub(crate) fn screen_for(menu: MenuUi) -> &'static str {
    match menu.screen {
        MenuScreen::Entries => SCREEN_MENU,
        MenuScreen::LevelOptions => SCREEN_OPTIONS,
    }
}

/// Whether the **level-options** dialog is the surface showing.
fn on_options(menu: MenuUi) -> bool {
    menu.screen == MenuScreen::LevelOptions
}

/// Whether the **entry list** is the surface showing — the screen whose selection
/// the list keys walk. On any sub-screen they belong to that screen instead.
fn on_list(menu: MenuUi) -> bool {
    menu.screen == MenuScreen::Entries
}

/// The menu half of [`Game`] — the input side, like [`crate::input`] for play.
impl Game {
    /// The menu's view state, or `None` once a run is playing.
    fn menu(&self) -> Option<MenuUi> {
        self.ui.menu
    }

    /// Apply a [`MenuNav`] from a key and repaint (§14/#268). Everything here is view
    /// state or a whole new run; nothing steps a turn, because until an entry is
    /// chosen there is no run to step.
    pub(crate) fn apply_menu_nav(&mut self, nav: MenuNav) {
        let Some(menu) = self.menu() else {
            return;
        };
        match nav {
            // The list walks only while the list is showing: on a sub-screen up/down
            // belong to that screen's own controls, not to a selection nobody can see.
            MenuNav::Prev if on_list(menu) => self.select(menu.prev_entry()),
            MenuNav::Next if on_list(menu) => self.select(menu.next_entry()),
            MenuNav::Activate if on_list(menu) => self.choose(menu.selection()),
            // The level-options dialog (#298). Up and down walk its two controls —
            // the ring is two long, so both steps are the same move — while the
            // horizontal pair sets the slider whichever control is marked, which is
            // what keeps the fast path Enter, Enter with the slider still reachable
            // without first walking to it.
            MenuNav::Prev | MenuNav::Next if on_options(menu) => {
                self.select_control(menu.options_control.other());
            }
            MenuNav::Easier if on_options(menu) => self.set_difficulty(menu.difficulty.easier()),
            MenuNav::Harder if on_options(menu) => self.set_difficulty(menu.difficulty.harder()),
            MenuNav::Activate if on_options(menu) => self.activate(menu.options_control),
            // Free on every menu surface. It used to be held back on the seed prompt,
            // where `n` was an ordinary letter of the token being typed and a key that
            // recoloured the screen mid-token would have been a trap; with typing gone
            // (§13.1/#572) no menu screen wants the letter for anything else.
            MenuNav::ToggleTheme => {
                self.toggle_theme();
                self.draw();
            }
            // Back out of a sub-screen. On the list itself there is nowhere further
            // back — the menu is the root — so Escape there does nothing.
            MenuNav::Back if !on_list(menu) => self.show_entries(),
            _ => {}
        }
    }

    /// Choose an entry — by key or by tap, one path for both (§11.6). A disabled
    /// entry (§14 v2/v3) does nothing at all, deliberately: they are listed so the
    /// menu has room to grow, and nothing more (#268).
    /// Apply a [`MenuHit`] from a tap on the title screen — choose an entry, or set the
    /// level-options slider. The pointer counterpart of
    /// [`apply_menu_nav`](Self::apply_menu_nav), so a tap and a key do the same thing
    /// through the same handlers.
    pub(crate) fn apply_menu_hit(&mut self, hit: MenuHit) {
        match hit {
            MenuHit::Entry(entry) => self.choose(entry),
            // A tapped slider stop is set directly rather than nudged towards — the
            // stop under the finger is the one the player meant.
            MenuHit::Difficulty(difficulty) => self.set_difficulty(difficulty),
            // A tapped control fires it *and* leaves the marker on it, so the screen
            // agrees with what the finger just did.
            MenuHit::OptionsControl(control) => {
                self.select_control(control);
                self.activate(control);
            }
        }
    }

    pub(crate) fn choose(&mut self, entry: MenuEntry) {
        match entry {
            // The saved run, straight back in (§12.5/#514) — no dialog in front of it:
            // the run's options were settled when it started, and the only thing this
            // row can be asked is *the one I was playing*.
            MenuEntry::ContinueRun => self.continue_run(),
            // Quick play opens its **pre-run** dialog rather than booting straight in
            // (#298): the difficulty is a choice about the run, so it is asked before
            // there is a run. Deliberately not `MenuEntry::Options`, which is §14 v2's
            // *global* settings screen — a different thing entirely (#513).
            MenuEntry::QuickPlay => self.show_level_options(),
            // Story mode goes **straight to the map** (§14 v3/#208), with no dialog in
            // front of it. There is nothing to ask: a campaign scales through its own
            // alert (#210) rather than through the §12.6 difficulty axis, so the one
            // control the quick-play dialog carries would have nothing to set.
            MenuEntry::StoryMode => self.start_campaign(),
            // The **global settings screen** (§14 v2/#513), raised over the menu: the
            // theme, the renderer, and in a debug session the §12.6 switches. It is
            // drawn over the entry list rather than replacing it, so leaving lands back
            // exactly here.
            MenuEntry::Options => self.open_settings(),
        }
    }

    /// Fire a level-options control: play the run the slider names, or go back.
    fn activate(&mut self, control: OptionsControl) {
        match control {
            OptionsControl::Play => {
                let difficulty = self.menu().map(|menu| menu.difficulty).unwrap_or_default();
                // Quick play, and the setting the slider was left on — the run's
                // framing (§14 v2/#138), which its end screen reads for what to
                // offer and what *new run* re-rolls at.
                self.start_run(
                    seed::random_level_at(difficulty),
                    RunOptions {
                        mode: RunMode::QuickPlay,
                        difficulty,
                    },
                );
            }
            OptionsControl::Back => self.show_entries(),
        }
    }

    /// Move the slider and repaint. The difficulty is view state until *Play* is
    /// pressed: the draw (#297) happens at boot, off the seed rolled then, so moving
    /// the slider costs nothing and commits to nothing.
    fn set_difficulty(&mut self, difficulty: Difficulty) {
        if let Some(menu) = self.ui.menu.as_mut() {
            menu.difficulty = difficulty;
        }
        self.draw();
    }

    /// Move the level-options marker and repaint.
    fn select_control(&mut self, control: OptionsControl) {
        if let Some(menu) = self.ui.menu.as_mut() {
            menu.options_control = control;
        }
        self.draw();
    }

    /// Show the level-options dialog. Every control on it is glyphs, so all the
    /// screen attribute does is tell the smoke check which surface is up.
    fn show_level_options(&mut self) {
        if let Some(menu) = self.ui.menu.as_mut() {
            menu.screen = MenuScreen::LevelOptions;
        }
        self.draw();
        set_screen(SCREEN_OPTIONS);
    }

    /// Move the selection marker and repaint.
    fn select(&mut self, entry: MenuEntry) {
        if let Some(menu) = self.ui.menu.as_mut() {
            menu.selected = entry;
        }
        self.draw();
    }

    /// Return from a sub-screen to the entry list.
    fn show_entries(&mut self) {
        if let Some(menu) = self.ui.menu.as_mut() {
            menu.screen = MenuScreen::Entries;
        }
        self.draw();
        set_screen(SCREEN_MENU);
    }

    /// Start playing `level`: rebuild the run, drop the menu, and reflect the level
    /// into the URL so the address bar is a shareable link from the run's first frame
    /// (§13.1/#110). [`Game::reseed`] resets the view state, which is what clears the
    /// menu — one place decides that a fresh run shows no chrome.
    ///
    /// A level that somehow fails to generate leaves the menu exactly as it was
    /// rather than dropping the player onto a broken board (§10.6 says the v1
    /// footprint always carves, so this is belt-and-braces).
    pub(crate) fn start_run(&mut self, level: LevelSeed, options: RunOptions) {
        if self.reseed(level).is_ok() {
            // **A quick-play run is not part of a campaign**, so the last one is dropped
            // here (§2.2: nothing survives a run). Without this a finished campaign would
            // still be listening, and the verdict of a *quick-play* facility would arrive
            // at a layer that has nothing to do with it (#208).
            self.campaign = None;
            // The framing outlives the reset [`Game::reseed`] performs, because it is
            // a fact about *how the player is playing*, not about the facility they
            // are about to walk into — the same reasoning that keeps the modality and
            // the build's replay offer across a fresh run.
            self.ui.end.options = options;
            seed::reflect_level(&level);
            set_screen(SCREEN_PLAY);
        }
    }

    /// Leave a finished run for the title screen (§14 v2/#138) — the one exit that
    /// starts no run ([`EndExit::Menu`](intrusion_core::EndExit)).
    ///
    /// The world underneath is left exactly as it was: the menu draws instead of the
    /// game frame, so there is nothing to reset, and nothing that could show through.
    ///
    /// This is the **mirror** of the reset [`Game::reseed`] performs, and deliberately
    /// the opposite shape (#473): going *into* a run drops everything except the few
    /// facts that outlive it, while coming *out* of one keeps the whole view state and
    /// only raises the menu over it. Nothing needs listing by name here — the theme and
    /// the modality carry because *everything* carries — and no leftover of the finished
    /// run can show through a modal menu that is drawn instead of the frame, nor survive
    /// the next run's own reset.
    pub(crate) fn show_menu(&mut self) {
        // Leaving for the title screen ends the run, whatever kind it was — and a
        // campaign is a run (§2.2). Dropping it here is the whole of "nothing carries
        // across runs": the value goes, and the next campaign is built from scratch.
        self.campaign = None;
        self.ui = ScreenUi {
            menu: Some(MenuUi::default()),
            // The level-start card belongs to the facility being left (#497), and the
            // title screen is not one — the same clear the campaign map makes, and for
            // the same reason: no surface the pumps ask about first may outlive the
            // frame it was drawn on.
            splash_open: false,
            ..self.ui
        };
        self.draw();
        set_screen(SCREEN_MENU);
    }
}

/// Wire the menu's DOM — which since #572 is **nothing but one attribute**: publish
/// which surface the shell opened on, and stop. The box, its buttons and their three
/// event fences are gone with the seed prompt (§13.1), so there is no element to look
/// up, no listener to install and no press to keep away from the game's pumps.
///
/// It stays a `install`-shaped function rather than collapsing into the boot because
/// the surface it publishes is the boot's own answer to *menu or run*, and
/// `data-screen` is read from outside the wasm (the artifact-build skill's smoke
/// check). `document` is taken for the same reason every sibling pump takes it: the
/// caller has already resolved it, and a shell reach that quietly found its own would
/// be the one place the boot's document and the module's could differ.
///
/// Called in live play only, never in the replay viewer (a replay has no menu: it
/// was told exactly which run to show).
pub(crate) fn install(_document: &Document, game: &Rc<RefCell<Game>>) -> Result<(), JsValue> {
    set_screen(if game.borrow().menu().is_some() {
        SCREEN_MENU
    } else {
        SCREEN_PLAY
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    /// The page as it is served — the one file the shell's DOM claims can be checked
    /// against without a browser.
    const PAGE: &str = include_str!("../../../web/index.html");

    /// **The game is the character grid, and the page carries no UI** (§11.1/#572).
    ///
    /// This is the assertion the seed box's removal is *for*: a control that is not in
    /// the markup cannot be revealed by a stray CSS rule, cannot swallow a keystroke
    /// the pumps wanted, and cannot be tapped by a ghost click. The check is on the
    /// markup rather than on a live DOM because the gate has no browser — but it is
    /// the honest half, since every element the shell ever reached for was authored
    /// here and this module now creates none.
    ///
    /// The replay HUD is the deliberate survivor and is *not* a control: it is a
    /// `<div>` of text with `pointer-events: none`, so a tap on it reaches the scrub
    /// surface underneath. Hence the check is for **interactive** markup, not for any
    /// element at all.
    #[test]
    fn the_page_carries_no_dom_ui() {
        for control in ["<input", "<button", "<select", "<textarea", "<form"] {
            assert!(
                !PAGE.contains(control),
                "web/index.html carries a `{control}` — the page is the canvas (§11.1)",
            );
        }
        // And nothing of the retired seed prompt is left behind to be revived by a
        // stylesheet or found by a lookup that outlived its element.
        for leftover in ["menu-seed", "seed-input", "seed-go", "seed-back", "--glyph"] {
            assert!(
                !PAGE.contains(leftover),
                "web/index.html still mentions `{leftover}` from the retired seed prompt",
            );
        }
    }

    /// The shell's own side of the same promise: this module **reaches for no
    /// element**. Every DOM lookup the menu ever made belonged to the seed box, so a
    /// new one appearing here is the exception growing back — which is exactly the
    /// thing #572 removed and the thing a reviewer would have to notice by eye.
    #[test]
    fn the_menu_shell_looks_up_no_element() {
        // The shipping half of this file — everything above these very tests, which
        // have to name the reaches in order to forbid them.
        let source = include_str!("menu.rs");
        let code = source
            .split_once("#[cfg(test)]")
            .expect("this module has tests")
            .0;
        // Comments are dropped too, so the prose above about the retired box does not
        // count against it.
        for line in code.lines().filter(|l| !l.trim_start().starts_with("//")) {
            for reach in ["get_element_by_id", "create_element", "query_selector"] {
                assert!(
                    !line.contains(reach),
                    "the menu shell reaches for an element again: {}",
                    line.trim(),
                );
            }
        }
    }
}
