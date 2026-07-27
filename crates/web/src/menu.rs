//! The shell half of the title screen (§11.4/§14, #268): the menu's input, the
//! runs it starts, and the one piece of it that has to be real markup.
//!
//! The screen itself is drawn by the core ([`intrusion_core::render_screen`] hands
//! the frame to the menu while [`ScreenUi::menu`] is set), so what lives here is
//! only what the core cannot own: browser listeners, the seed text box, and the
//! boot path a chosen entry runs.
//!
//! # Why the seed box is DOM
//!
//! Everything else on the menu is glyphs on the canvas. The seed box is not,
//! because **a canvas cannot raise a phone's keyboard** — a grid-drawn text field
//! would make Seed play a desktop-only feature, which §11.6 ("touch is a real
//! target and was never finished") is exactly about. So the box is a real
//! `<input>` in `web/index.html`, floating over the band the core's seed prompt
//! leaves blank, revealed by the `data-screen` attribute this module sets on
//! `<body>`. Its *play* and *back* buttons are real buttons for the same reason:
//! a touch player must be able to leave the prompt without a keyboard (§11.6's
//! no-trap rule — the failure the old options dialog shipped).
//!
//! The box's listeners sit on the panel itself and stop their events there, before
//! the document-level pumps (§11.6) can read a keystroke as a move or a press as a
//! swipe — the same guard the old seed bar needed, for the same reason.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use intrusion_core::{LevelSeed, MenuEntry, MenuHit, MenuNav, MenuUi, ScreenUi, UiCommand};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{
    Document, Element, HtmlElement, HtmlInputElement, KeyboardEvent, MouseEvent, Node, PointerEvent,
};

use crate::{seed, Game};

/// What `<body data-screen>` reads on each of the shell's three surfaces. The CSS
/// reveals the seed box on `seed` and hides it everywhere else, and it is the one
/// signal outside the canvas that says which surface is up — which is also what
/// lets the headless smoke check (the artifact-build skill's `verify.mjs`) tell a
/// title screen from a live run.
const SCREEN_ATTR: &str = "data-screen";
const SCREEN_MENU: &str = "menu";
const SCREEN_SEED: &str = "seed";
const SCREEN_PLAY: &str = "play";

/// The view state a fresh load opens on: the menu's entry list, Quick play selected.
pub(crate) fn opening_ui() -> ScreenUi {
    ScreenUi {
        menu: Some(MenuUi::default()),
        ..ScreenUi::default()
    }
}

/// Publish the board's current glyph size to the page as the `--glyph` custom
/// property, in CSS pixels — the size the seed box types itself at.
///
/// The canvas scales its text to fit the viewport (§11.4: the whole level, always,
/// at any size), so the *same words* are ten pixels tall in a narrow frame and twice
/// that on a desktop. A form measured in fixed pixels does not follow, and in a
/// narrow frame it ends up shouting over a board drawn half its size. Handing the
/// fit's own number to the stylesheet keeps the one piece of DOM chrome the same
/// size as the glyphs beside it, at every fit and through every rotation — the shell
/// does not restyle anything, it just says how big a letter currently is.
pub(crate) fn set_glyph_size(css_px: f64) {
    if let Some(root) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.document_element())
        .and_then(|e| e.dyn_into::<HtmlElement>().ok())
    {
        let _ = root
            .style()
            .set_property("--glyph", &format!("{css_px:.2}px"));
    }
}

/// Mirror the current surface onto `<body data-screen>`. Best-effort: a page whose
/// body is somehow unavailable simply keeps the seed box hidden, which is the safe
/// direction — the box is never the only way out of anything.
fn set_screen(screen: &str) {
    if let Some(body) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.body())
    {
        let _ = body.set_attribute(SCREEN_ATTR, screen);
    }
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
            // The list walks only while the list is showing: with the seed prompt up,
            // up/down belong to the text box, not to a selection nobody can see.
            MenuNav::Prev if !menu.seed_entry => self.select(menu.selected.prev()),
            MenuNav::Next if !menu.seed_entry => self.select(menu.selected.next()),
            MenuNav::Activate if !menu.seed_entry => self.choose(menu.selected),
            // Held back on the seed prompt, where `n` is an ordinary letter of the
            // token being typed (§13.1/#245): the box has the keyboard there, and a
            // key that recoloured the screen mid-token would be a trap, not an
            // option. On the list it is the same free view toggle as everywhere else.
            MenuNav::ToggleTheme if !menu.seed_entry => {
                self.apply_ui_command(UiCommand::ToggleTheme);
                self.draw();
            }
            // Back out of the seed prompt. On the list itself there is nowhere
            // further back — the menu is the root — so Escape there does nothing.
            MenuNav::Back if menu.seed_entry => self.show_entries(),
            _ => {}
        }
    }

    /// Choose an entry — by key or by tap, one path for both (§11.6). A disabled
    /// entry (§14 v2/v3) does nothing at all, deliberately: they are listed so the
    /// menu has room to grow, and nothing more (#268).
    /// Apply a [`MenuHit`] from a tap on the title screen — choose an entry, or flip
    /// the theme from the footer control (§11.2/#189). The pointer counterpart of
    /// [`apply_menu_nav`](Self::apply_menu_nav), so a tap and a key do the same
    /// thing through the same two calls.
    pub(crate) fn apply_menu_hit(&mut self, hit: MenuHit) {
        match hit {
            MenuHit::Entry(entry) => self.choose(entry),
            MenuHit::ToggleTheme => {
                self.apply_ui_command(UiCommand::ToggleTheme);
                self.draw();
            }
        }
    }

    pub(crate) fn choose(&mut self, entry: MenuEntry) {
        match entry {
            MenuEntry::QuickPlay => self.start_run(seed::random_level()),
            MenuEntry::SeedPlay => self.show_seed_prompt(),
            MenuEntry::Options | MenuEntry::StoryMode => {}
        }
    }

    /// Move the selection marker and repaint.
    fn select(&mut self, entry: MenuEntry) {
        if let Some(menu) = self.ui.menu.as_mut() {
            menu.selected = entry;
        }
        self.draw();
    }

    /// Show the seed prompt: the core draws the instructions, the DOM box appears in
    /// the band they leave clear, and the box takes focus so a desktop player can
    /// type straight away and a phone raises its keyboard on the screen that exists
    /// to be typed into.
    fn show_seed_prompt(&mut self) {
        if let Some(menu) = self.ui.menu.as_mut() {
            menu.seed_entry = true;
        }
        self.draw();
        set_screen(SCREEN_SEED);
        focus_seed_input();
    }

    /// Return from the seed prompt to the entry list, hiding the box again.
    fn show_entries(&mut self) {
        if let Some(menu) = self.ui.menu.as_mut() {
            menu.seed_entry = false;
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
    fn start_run(&mut self, level: LevelSeed) {
        if self.reseed(level).is_ok() {
            seed::reflect_level(&level);
            set_screen(SCREEN_PLAY);
        }
    }
}

/// Focus the seed box, if the page has one. Best-effort like every other DOM reach
/// here: a page without the markup still shows the prompt, it just cannot be typed
/// into — which is why the prompt is never the only way to start a run.
fn seed_input() -> Option<HtmlInputElement> {
    web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("seed-input"))
        .and_then(|e| e.dyn_into::<HtmlInputElement>().ok())
}

fn focus_seed_input() {
    if let Some(input) = seed_input() {
        let _ = input.focus();
    }
}

/// Ignore any click on the panel whose **press began somewhere else** — the
/// "ghost click" a touch leaves behind (§11.6).
///
/// One tap is one `pointerdown` on the board followed, on release, by a `click`
/// aimed at whatever is under the finger *by then*. The press that chooses Seed
/// play therefore opens the prompt and *then* clicks whatever part of the
/// just-revealed panel happens to sit under that finger. On a narrow frame that is
/// the `back` button — the panel wraps it to a second row, centred, exactly where
/// the menu row that opened the prompt was tapped — so a single tap opened the
/// prompt and closed it again, the form and the keyboard flashing on the way past.
///
/// The rule that fixes it is the one a control already implies: **an interaction
/// that started on the board is not an interaction with this panel.** A pointerdown
/// or a keystroke *inside* the panel arms it; a pointerdown anywhere else disarms it
/// (both read in the capture phase, so they are seen before the panel stops the
/// event reaching the game's pumps); and an unarmed click is swallowed before any
/// button's own handler runs. Arming on the keystroke is what keeps Enter or Space
/// on a focused button working — the click those synthesise is indistinguishable
/// from a touch's by its own fields (both report a click count of zero), so *what
/// came before it* is the only honest test.
fn guard_ghost_clicks(document: &Document, panel: &Element) -> Result<(), JsValue> {
    let armed = Rc::new(Cell::new(false));

    {
        let armed = armed.clone();
        let panel = panel.clone();
        let cb = Closure::<dyn FnMut(PointerEvent)>::new(move |e: PointerEvent| {
            let inside = e
                .target()
                .and_then(|t| t.dyn_into::<Node>().ok())
                .is_some_and(|n| panel.contains(Some(&n)));
            armed.set(inside);
        });
        document.add_event_listener_with_callback_and_bool(
            "pointerdown",
            cb.as_ref().unchecked_ref(),
            true, // capture: before the panel stops the event going any further
        )?;
        cb.forget();
    }

    {
        let armed = armed.clone();
        let cb = Closure::<dyn FnMut(KeyboardEvent)>::new(move |_e: KeyboardEvent| armed.set(true));
        panel.add_event_listener_with_callback_and_bool(
            "keydown",
            cb.as_ref().unchecked_ref(),
            true,
        )?;
        cb.forget();
    }

    {
        let armed = armed.clone();
        let cb = Closure::<dyn FnMut(MouseEvent)>::new(move |e: MouseEvent| {
            if armed.replace(false) {
                return;
            }
            e.stop_propagation();
            e.prevent_default();
        });
        panel.add_event_listener_with_callback_and_bool(
            "click",
            cb.as_ref().unchecked_ref(),
            true, // capture: swallowed before any button's own handler runs
        )?;
        cb.forget();
    }

    Ok(())
}

/// Wire the menu's DOM: the seed box, its *play* and *back* buttons, and the event
/// guards that keep the panel's own input away from the game's document-level pumps
/// — and the board's presses away from the panel ([`guard_ghost_clicks`], §11.6).
/// Every element is optional — a page hosted without the markup still boots and
/// still plays, it just has no seed prompt — so each lookup falls out quietly rather
/// than failing the boot.
///
/// Called in live play only, never in the replay viewer (a replay has no menu: it
/// was told exactly which run to show).
pub(crate) fn install(document: &Document, game: &Rc<RefCell<Game>>) -> Result<(), JsValue> {
    set_screen(if game.borrow().menu().is_some() {
        SCREEN_MENU
    } else {
        SCREEN_PLAY
    });

    let (Some(panel), Some(input), Some(go), Some(back)) = (
        document.get_element_by_id("menu-seed"),
        seed_input(),
        document.get_element_by_id("seed-go"),
        document.get_element_by_id("seed-back"),
    ) else {
        return Ok(()); // no seed-prompt markup on this page — nothing to wire
    };

    guard_ghost_clicks(document, &panel)?;

    // Loading a token: decode the box and play it. An empty or unreadable box rolls
    // a fresh quick-play run rather than refusing (#110) — the prompt must never be
    // a dead end, and a typo costs a level, not the page.
    let load: Rc<dyn Fn()> = {
        let game = game.clone();
        let input = input.clone();
        Rc::new(move || {
            let level = LevelSeed::decode(&input.value()).unwrap_or_else(seed::random_level);
            input.set_value("");
            game.borrow_mut().start_run(level);
        })
    };

    {
        let load = load.clone();
        let cb = Closure::<dyn FnMut(MouseEvent)>::new(move |_e: MouseEvent| load());
        go.add_event_listener_with_callback("click", cb.as_ref().unchecked_ref())?;
        cb.forget();
    }

    // The touch player's way back to the entry list, beside the way forward (§11.6).
    {
        let game = game.clone();
        let cb = Closure::<dyn FnMut(MouseEvent)>::new(move |_e: MouseEvent| {
            game.borrow_mut().show_entries();
        });
        back.add_event_listener_with_callback("click", cb.as_ref().unchecked_ref())?;
        cb.forget();
    }

    // Keys typed into the panel are the box's own, never the menu's: swallow them
    // before the document key pump sees a `j` as "next entry" or an Enter as
    // "activate". Enter submits the token; Escape leaves the prompt, the keyboard
    // twin of the back button. Nothing is `preventDefault`ed, so the box still types
    // normally.
    //
    // Enter on a **focused button** is that button's own activation, not the box's
    // submit: the browser turns it into a click, so intercepting it here as well
    // would fire *play* while the click fires *back*.
    {
        let load = load.clone();
        let game = game.clone();
        let cb = Closure::<dyn FnMut(KeyboardEvent)>::new(move |e: KeyboardEvent| {
            e.stop_propagation();
            let on_button = e
                .target()
                .and_then(|t| t.dyn_into::<Element>().ok())
                .is_some_and(|el| el.tag_name() == "BUTTON");
            match e.key().as_str() {
                "Enter" if !on_button => load(),
                "Escape" => game.borrow_mut().show_entries(),
                _ => {}
            }
        });
        panel.add_event_listener_with_callback("keydown", cb.as_ref().unchecked_ref())?;
        cb.forget();
    }

    // A press on the panel is a UI interaction, not a menu tap or a swipe: keep it
    // from the gesture pump on the document, which would otherwise read it as one.
    {
        let cb =
            Closure::<dyn FnMut(PointerEvent)>::new(move |e: PointerEvent| e.stop_propagation());
        panel.add_event_listener_with_callback("pointerdown", cb.as_ref().unchecked_ref())?;
        cb.forget();
    }

    Ok(())
}
