//! The shell's input side (§11.6): the keydown pump and the touch gesture pump,
//! both feeding the same one-input-at-a-time seam ([`Game::step_and_draw`]).
//!
//! The shell never interprets a key — the §11.6 bindings live in
//! `core::input_for_key` / `core::ui_command_for_key` / `core::ability_slot_for_code`
//! / `core::ability_slot_for_letter`, pinned by native tests. What lives *here* is the plumbing the core cannot own:
//! browser listeners, the gesture's live state, and the repeat timers — plus the
//! *order* those tables are consulted in ([`play_key`]), which is the shell's alone
//! because only the shell holds both halves of the event. Every pure rule of this
//! module is natively tested below like any core table.
//!
//! **The touch model** (replacing the old edge-zone tap slice): a **swipe**
//! along the drag's dominant axis *keeps* firing while the finger stays down, the
//! direction re-read live from the drag; a **press held in place** repeats; a
//! **quick tap** is a single input, resolved at the lift. Lifting the finger stops
//! everything instantly — fairness (§2.2/§4.5) demands nothing ever lands after
//! the lift, and every repeat is one ordinary input through the same seam as a
//! held arrow key, never a batch.
//!
//! **One pump, every surface** (#336). What a drag *did* — [`drag_gesture`] — is
//! surface-neutral: a direction, or a press. What that **means** is a binding, and
//! bindings live in the core beside the key tables, one per screen
//! (`core::input_for_gesture` / `menu_nav_for_gesture` / `help_nav_for_gesture`).
//! So the same thresholds, the same repeat cadence and the same
//! lift-stops-everything guarantee walk the board, the title screen's list and the
//! help panel's tab bar, and the next screen that wants touch costs a binding
//! table rather than a pump. The one thing that stays here is the *order* the
//! tables are consulted in ([`surface_command`], mirroring [`play_key`]).
//!
//! **Where** the finger is decides whether the ambiguous half of the model is
//! allowed at all — but only on the board: the Wait-producing gestures resolve
//! through [`Game::tap_at`](crate::tap) (§11.6/#306), so a tap or a held press that
//! landed on the chrome, in its dead band, or off the canvas does nothing instead
//! of silently spending a turn. Swipes are exempt — a directional drag is
//! unambiguous. A modal screen owns the whole viewport and has no board, so
//! neither that gate nor #223's danger gate applies there; both live in the one
//! `Play` arm of [`GesturePump::apply`], which is what keeps them from leaking.

use std::cell::RefCell;
use std::rc::Rc;

use intrusion_core::{
    ability_in_slot, ability_slot_for_code, ability_slot_for_letter, declines_exchange,
    end_nav_for_gesture, end_nav_for_key, help_nav_for_gesture, help_nav_for_key,
    input_for_gesture, input_for_key, key_for_code, map_nav_for_gesture, map_nav_for_key,
    menu_nav_for_gesture, menu_nav_for_key, ui_command_for_key, Cell, Direction, EndExit, EndNav,
    Gesture, HelpHit, HelpNav, HelpTab, Input, InputModality, MapNav, MenuNav, SeedCopy, UiCommand,
    BOTTOM_ROWS, TOP_ROWS,
};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{Document, KeyboardEvent, PointerEvent};

use crate::clipboard;
use crate::tap::{Control, Tap};
use crate::Game;

/// The input-facing half of [`Game`]: how a key or a gesture tick becomes a
/// turn. The rendering half (fit, paint) stays in `lib.rs`, the colour table in
/// [`palette`](crate::palette).
impl Game {
    /// Map a keypress through the core's §11.6 tables and, if it is one the loop
    /// takes, step and redraw. Returns whether the key was consumed (so the caller can
    /// stop the page from scrolling on the arrows). Every mapping lives in
    /// `core::input`, where native tests pin it — this shell never interprets a key.
    ///
    /// It takes **both** halves of the browser's event: the `key` character the layout
    /// produced, and the physical `code` under the finger. Most bindings are on the
    /// character, but the digits bind by position (#359) — the ability bar's `1`–`4`
    /// straight off `Digit1`–`Digit4`, and the numpad folded onto the arrows by
    /// `key_for_code` before any character table is consulted — so an AZERTY or
    /// Dvorak player presses the same physical keys as a QWERTY one. The abilities'
    /// mnemonic letters (#360) go the other way, on the character, because there the
    /// binding is the letter the bar is showing.
    fn handle_key(&mut self, key: &str, code: &str, is_repeat: bool) -> bool {
        // The numpad's meaning is its position, not the character the layout put on
        // it, so it is folded to the §11.6 key it duplicates — the arrows, and `w` for
        // wait — once, here, ahead of every table below (movement, the help panel's
        // tabs, the menu's list), so they cannot drift apart on what the numpad takes.
        // It folds onto the arrows rather than onto `8` `2` `4` `6` deliberately
        // (#369): those characters are the top row's too, and the top row is the bar's.
        let key = key_for_code(code).unwrap_or(key);
        // Before a run starts, the menu owns the keyboard (§14/#268): it is modal in
        // the strongest sense — there is no world to step underneath it. Everything
        // the game would claim is swallowed; a genuinely unowned key (F5, a browser
        // shortcut) is still left to the page.
        if self.ui.menu.is_some() {
            if let Some(nav) = menu_nav_for_key(key) {
                self.apply_menu_nav(nav);
                return true;
            }
            return ui_command_for_key(key).is_some() || self.game_claims_key(key, code);
        }
        // The campaign map next, and **before** the end screen (§14 v3/#208): between
        // facilities the `State` underneath is a finished raid, so its verdict keys would
        // otherwise answer presses aimed at the map drawn over it.
        if self.map_open() {
            if let Some(nav) = map_nav_for_key(key) {
                self.apply_map_nav(nav);
                return true;
            }
            return ui_command_for_key(key).is_some() || self.game_claims_key(key, code);
        }
        // A finished run owns the keyboard next (§14 v2/#138). The end screen is
        // modal in the menu's strong sense — there is no world left to step, since
        // the loop is inert once the run is over — so its own list keys are answered
        // here and everything the game would otherwise claim is swallowed. A
        // genuinely unowned key (F5, a browser shortcut) is still left to the page.
        if self.run_over() {
            if let Some(nav) = end_nav_for_key(key) {
                self.apply_end_nav(nav);
                return true;
            }
            return ui_command_for_key(key).is_some() || self.game_claims_key(key, code);
        }
        // While the help panel is open it is **modal** (§14 v2/#248): it captures
        // input, so keys route to help navigation first and the world never steps
        // underneath. `?`/Esc close it, Tab/←→ switch tabs. A key the panel does not
        // navigate by is *swallowed* if the game would otherwise own it (a move, an
        // ability, a UI toggle) — keeping the world frozen — but a genuinely unowned
        // key (F5, a browser shortcut) is still left to the page, as it is in play.
        if self.ui.help_open {
            if let Some(nav) = help_nav_for_key(key, self.ui.debug_mode) {
                self.apply_help_nav(nav);
                self.draw();
                return true;
            }
            return ui_command_for_key(key).is_some() || self.game_claims_key(key, code);
        }
        // An **open exchange** (§8.3/#266) claims `Escape`, and only `Escape`. Its four
        // candidates are the ability bar's own slots, so the digits and the mnemonic
        // letters below already reach every one of them — including the crate's, whose
        // discard *is* the decline. This is the second spelling of that one press, for
        // the hand that reaches for `Escape` when a game asks a question, and it resolves
        // to the same `Input::Discard` rather than to a cancel path of its own.
        if let Some(offer) = self.state.exchange() {
            if declines_exchange(key) {
                self.step_and_draw(Input::Discard(offer.offered()));
                return true;
            }
        }
        // UI commands (§11.4) come next: they toggle view state and redraw without
        // ever touching the turn loop. `m` deploys the message list; `?` opens help.
        if let Some(command) = ui_command_for_key(key) {
            self.apply_ui_command(command);
            self.draw();
            return true;
        }
        // A game action (movement/wait) or an ability key (§11.6): both resolve in the
        // core and drive the one turn seam. An ability has **two** keys, and they
        // differ in what binds them: the digit names a bar slot by *position*, off the
        // physical code (`ability_slot_for_code`, #359), and the mnemonic names one by
        // the *letter* the bar is highlighting, off the character the layout produced
        // (`ability_slot_for_letter`, #360) — a position binds by position, a letter by
        // letter. Both land on a slot, the core turns that slot into the ability drawn
        // there (`ability_in_slot`) and then into this turn's input
        // (`State::ability_input`) — a **toggle**, so the key that switched the ability
        // on switches it off again (§4.4/#304). A tap on the bar entry goes through the
        // same calls from `ability_at` on, so neither key can disagree with the entry
        // above it.
        let input = match play_key(key, code, |letter| {
            ability_slot_for_letter(&self.state, letter)
        }) {
            Some(PlayKey::Move(input)) => input,
            Some(PlayKey::Slot(slot)) => {
                // A **held** ability key is swallowed (§11.6/#304): now that the key is
                // a toggle, letting the browser's auto-repeat through would switch the
                // ability straight back off a frame after switching it on. Toggling
                // takes a deliberate press, in both directions — and the repeat was a
                // free no-op before this, so nothing is lost. Consumed, so the page is
                // still.
                if is_repeat {
                    return true;
                }
                // A digit past the run's held count fires nothing: no turn, no state
                // change (§11.6 — a miss is free). Still consumed, because the four
                // digits are the game's whether or not this run filled them, and a `4`
                // that scrolled the page on a three-ability run would be worse than one
                // that does nothing. (A mnemonic never lands here — a letter no entry
                // claimed resolves to no slot at all, and stays the page's.)
                let Some(id) = ability_in_slot(&self.state, slot) else {
                    return true;
                };
                self.state.ability_input(id)
            }
            None => return false,
        };
        // A keyboard **auto-repeat** (`KeyboardEvent.repeat`) that would walk the
        // player into visible danger is swallowed here (§11.6/#223): the deliberate
        // first press (`is_repeat` false) always lands, but its held repeat stops at
        // the edge of a seen guard's cone — going deeper takes a fresh press. Still
        // consumed (returns `true`) so the page never scrolls on the swallowed arrow.
        if is_repeat && self.repeat_into_danger(input) {
            return true;
        }
        self.step_and_draw(input);
        true
    }

    /// What a [`Gesture`] means on the surface that is up (§11.6/#336) — the touch
    /// counterpart of [`handle_key`](Self::handle_key)'s opening, and deliberately
    /// the *same* modal precedence: the menu owns the frame before a run starts, the
    /// open help panel owns it during one, and only underneath both is there a board
    /// to walk. One pump drives whichever of the three is showing.
    ///
    /// The tables themselves are the core's (§11.6/§12.1), so a screen's touch and
    /// key bindings sit side by side and are pinned by the same native tests. What
    /// lives here is only the *order* they are consulted in — the shell's, because
    /// only the shell holds the view state that decides it.
    ///
    /// `None` is a gesture the showing surface declines: a press on a modal, a
    /// sideways swipe across a vertical list. It spends nothing and changes nothing,
    /// exactly as an unbound key does.
    ///
    /// The rule itself is [`surface_command`], pure so it is pinned natively like
    /// every other §11.6 rule here; this reads the two view flags it needs.
    fn gesture_command(&self, gesture: Gesture) -> Option<GestureCommand> {
        surface_command(
            self.ui.menu.is_some(),
            self.map_open(),
            self.run_over(),
            self.ui.help_open,
            gesture,
        )
    }

    /// Whether the game would claim this press *in play*, used by the modal screens
    /// to decide what to swallow. Asking [`play_key`] rather than a list of tables is
    /// what keeps "the menu swallows everything the game would take" true as those
    /// tables move.
    fn game_claims_key(&self, key: &str, code: &str) -> bool {
        play_key(key, code, |letter| {
            ability_slot_for_letter(&self.state, letter)
        })
        .is_some()
    }

    /// Feed one [`Input`] to the loop and repaint — the single seam every input
    /// source (a key, a gesture tick) drives, one ordinary input at a time against
    /// the current frame's state (§2.2 fairness: never a batched multi-step).
    ///
    /// Being the single seam is what makes it the recorder's home (§12.4/#411):
    /// everything that reaches the world — a held key's repeats, a free wall bump, a
    /// swallowed post-end press — is appended exactly as fed, so the recording *is*
    /// the run and a stream taken anywhere shallower would drift from what was
    /// played. **Every build records** (#478): the recording costs one small `Copy`
    /// enum per turn and is what makes a strange run on the deployed page handable
    /// to someone else, which is the whole reason the debug session exists.
    pub(crate) fn step_and_draw(&mut self, input: Input) {
        self.recorded.push(input);
        self.state.step(input);
        // The frame a run ends on belongs to the verdict (§14 v2/#138): the deployed
        // message list is folded away as the loop stops, so what reads behind the
        // panel is the board the capture has to be traced on — and not a list left
        // hanging over it that the end screen's keys can no longer close.
        if self.run_over() {
            self.ui.message_log_open = false;
        }
        self.draw();
        // The **campaign** hears the verdict last (§12.7/#208), after the frame the raid
        // ended on has been painted: an escaped facility banks its haul and the map comes
        // up over that frame, and a run that ended for good leaves the end screen where
        // it is. A no-op in quick play, which has no layer above the level.
        if let Some(verdict) = self.state.verdict() {
            self.campaign_verdict(&verdict);
        }
    }

    /// Whether a held movement's *repeat* of `input` must be suppressed this tick
    /// because continuing the hold would carry the player into — or deeper through —
    /// visible danger (§11.5/§11.6, #223). Reads the core's own overlay set
    /// ([`State::in_visible_danger`](intrusion_core::State::in_visible_danger)) so
    /// the shell never recomputes detection; the pure rule is [`repeat_suppressed`],
    /// unit-tested natively below. Called for repeats only — a fresh press never
    /// routes here, so a single deliberate step into a cone is always allowed.
    fn repeat_into_danger(&self, input: Input) -> bool {
        let player = self.state.player();
        repeat_suppressed(player, input, |cell| self.state.in_visible_danger(cell))
    }

    /// Note that the player just used `modality`, and repaint if that is news
    /// (§11.6/#323). The only thing it changes is how the usable line's floor words
    /// move and wait — so the hint follows what the player's hands are *doing*, not
    /// what the device could theoretically do: a laptop with a touchscreen is a
    /// keyboard session until a finger lands on it, and a tablet with a keyboard
    /// attached is the reverse.
    ///
    /// The redraw is the point — the row is only ever read between turns, so a
    /// modality that changed without one would otherwise keep teaching the wrong
    /// vocabulary until the next step. It costs a paint on the rare frame the answer
    /// actually flips, and nothing at all on every other input.
    pub(crate) fn note_modality(&mut self, modality: InputModality) {
        if self.ui.modality != modality {
            self.ui.modality = modality;
            self.draw();
        }
    }

    /// Apply a shell-level [`UiCommand`] (§11.4) — a view toggle, never a game
    /// action, so it changes no [`State`](intrusion_core::State).
    pub(crate) fn apply_ui_command(&mut self, command: UiCommand) {
        match command {
            UiCommand::ToggleMessageLog => {
                self.ui.message_log_open = !self.ui.message_log_open;
            }
            UiCommand::ToggleHelp => {
                self.ui.help_open = !self.ui.help_open;
                // The seed-copy line answers a press the player made a moment ago
                // (§13.1/#353), so it does not survive the panel: reopening must not
                // greet them with an acknowledgement of a copy from another minute of
                // the run.
                self.ui.seed_copy = SeedCopy::default();
            }
            // The shell holds *both* colour tables and the core holds the flag
            // (§11.2/#189), so switching theme is this one line here and a column of
            // hex in [`palette`](crate::palette) — no game system learns a colour.
            UiCommand::ToggleTheme => {
                self.ui.theme = self.ui.theme.toggled();
            }
        }
    }

    /// Whether the run has ended, and the end screen is therefore up (§14 v2/#138).
    ///
    /// Asked of the **core**, never of a flag here: the screen is drawn exactly when
    /// the state has a verdict ([`State::verdict`](intrusion_core::State::verdict)),
    /// so what the shell routes input to and what the renderer paints cannot come
    /// apart.
    pub(crate) fn run_over(&self) -> bool {
        self.state.verdict().is_some()
    }

    /// Apply an [`EndNav`] from the end screen (§14 v2/#138) — walk the exits, or
    /// fire the marked one. Walking is a pure view move; firing starts a run or goes
    /// to the menu, and neither steps the finished world.
    pub(crate) fn apply_end_nav(&mut self, nav: EndNav) {
        match nav {
            EndNav::Prev => self.select_exit(self.ui.end.prev()),
            EndNav::Next => self.select_exit(self.ui.end.next()),
            EndNav::Activate => self.take_exit(self.ui.end.selected()),
        }
    }

    /// Move the end screen's marker and repaint.
    fn select_exit(&mut self, exit: EndExit) {
        self.ui.end.selected = exit;
        self.draw();
    }

    /// Take an exit — the one place a finished run leads anywhere (§14 v2/#138).
    ///
    /// **Which level each exit boots is the core's rule** ([`EndExit::level`]), not
    /// this shell's: retry hands back the identical [`LevelSeed`] and *new run* rolls
    /// quick play at the same options, both pinned by native tests. What is left here
    /// is the shell's own two contributions — the clock the fresh seed comes off, and
    /// the title screen, which is markup rather than a level.
    ///
    /// A tapped or fired exit is applied even when the marker was elsewhere, so the
    /// screen always does what the finger just pointed at.
    pub(crate) fn take_exit(&mut self, exit: EndExit) {
        self.ui.end.selected = exit;
        let options = self.ui.end.options;
        match exit.level(&self.level, options, crate::seed::clock_seed()) {
            Some(level) => self.start_run(level, options),
            None => self.show_menu(),
        }
    }

    /// Apply a [`HelpNav`] from the open modal panel (§14 v2/#248) — close it, cycle
    /// the shown tab, or copy the run's token (#353). Still a pure view action: no
    /// [`State`], no turn (§4.4), whichever arm runs.
    fn apply_help_nav(&mut self, nav: HelpNav) {
        let debug = self.ui.debug_mode;
        match nav {
            HelpNav::Close => self.close_help(),
            HelpNav::NextTab => self.show_help_tab(self.ui.help_tab.next(debug)),
            HelpNav::PrevTab => self.show_help_tab(self.ui.help_tab.prev(debug)),
            HelpNav::ToggleTheme => self.apply_ui_command(UiCommand::ToggleTheme),
            HelpNav::CopySeed => self.copy_seed(),
            HelpNav::CopyReplay => self.copy_replay(),
            HelpNav::ToggleReveal => self.toggle_reveal(),
        }
    }

    /// Apply a [`HelpHit`] from a tap on the open panel: switch to the tapped tab,
    /// copy the token, or close. A view action like
    /// [`apply_help_nav`](Self::apply_help_nav), and the same arms — the two halves of
    /// §11.6's key-*and*-touch pairing go through the same handlers below, so neither
    /// can do something the other does not.
    pub(crate) fn apply_help_hit(&mut self, hit: HelpHit) {
        match hit {
            HelpHit::Close => self.close_help(),
            HelpHit::Tab(tab) => self.show_help_tab(tab),
            HelpHit::ToggleTheme => self.apply_ui_command(UiCommand::ToggleTheme),
            HelpHit::CopySeed => self.copy_seed(),
            HelpHit::CopyReplay => self.copy_replay(),
            HelpHit::ToggleReveal => self.toggle_reveal(),
        }
    }

    /// Flip the debug session's **omni-vision** switch (§12.6/#459) — the shell half of
    /// the Debug tab's `omni [v]` control, key and tap alike.
    ///
    /// It mirrors the drawn control exactly, like every other handler here: no Debug
    /// tab in this session, or a different tab up, and the press does nothing. The flip
    /// itself is the core's ([`State::toggle_reveal`](intrusion_core::State)) — a sight
    /// recompute, no turn (§4.4), and no world change, which is what makes it a view
    /// action at all.
    fn toggle_reveal(&mut self) {
        if !self.ui.debug_mode || self.ui.help_tab != HelpTab::Debug {
            return;
        }
        self.state.toggle_reveal();
    }

    /// Dismiss the panel, dropping the seed-copy acknowledgement with it (#353).
    fn close_help(&mut self) {
        self.ui.help_open = false;
        self.ui.seed_copy = SeedCopy::default();
    }

    /// Show `tab`, dropping the acknowledgement for the same reason: it is a reply to
    /// a press on the Level info tab and belongs to the moment, not to the panel.
    fn show_help_tab(&mut self, tab: HelpTab) {
        self.ui.help_tab = tab;
        self.ui.seed_copy = SeedCopy::default();
    }

    /// Put this run's **level-seed token** on the system clipboard (§13.1/#353) — the
    /// one thing on the panel that exists to be taken away, and until now the only
    /// thing there the player could not actually take.
    ///
    /// It mirrors the drawn control exactly ([`seed_to_copy`]): on the Level info tab,
    /// and only when this run has a token at all. A press with no control under it
    /// does nothing — no acknowledgement either, because nothing was attempted.
    ///
    /// The clipboard is conditional, so the outcome comes back in two pieces. No
    /// clipboard on this page at all is known *now*, and says so on this frame; a
    /// clipboard that refuses (a frame without permission — which the artifact build's
    /// own `<iframe>` may well be) only rejects a microtask later, and corrects the
    /// line then. Either way the token stays printed one row above, and nothing claims
    /// a copy that did not happen.
    fn copy_seed(&mut self) {
        let Some(token) = self.seed_to_copy() else {
            return;
        };
        self.copy_to_clipboard(&token);
    }

    /// Put the whole **run** on the clipboard as a `…#seed=<token>&inputs=<script>`
    /// link (§12.4/§13.1/#411) — the keyboard/touch halves of the Level info tab's
    /// `replay [r]` control, [`copy_seed`](Self::copy_seed)'s sibling, through the
    /// very same clipboard plumbing and acknowledgement line.
    ///
    /// It mirrors the drawn control exactly ([`replay_to_copy`](Self::replay_to_copy)):
    /// offered by this build at all, on the Level info tab, and only when the run has
    /// a token for the link to carry. In every other build the control is not drawn
    /// and this is a no-op, so the `r` key does exactly what the panel shows.
    fn copy_replay(&mut self) {
        let Some(url) = self.replay_to_copy() else {
            return;
        };
        self.copy_to_clipboard(&url);
    }

    /// The one clipboard write both copy controls share (#353/#411): start the
    /// conditional write, and record what is known *now* — nothing while the
    /// browser's answer is still coming, [`SeedCopy::Unavailable`] when there was no
    /// clipboard to ask. The microtask half lands in
    /// [`note_seed_copy`](Self::note_seed_copy).
    fn copy_to_clipboard(&mut self, text: &str) {
        let handle = self.handle.clone();
        let started = clipboard::write_text(text, move |ok| {
            // The promise settles in a microtask, long after the borrow this call was
            // made under; a page that has gone away simply has nobody to tell.
            if let Some(game) = handle.upgrade() {
                game.borrow_mut().note_seed_copy(if ok {
                    SeedCopy::Copied
                } else {
                    SeedCopy::Unavailable
                });
            }
        });
        self.ui.seed_copy = if started {
            // The answer is still coming; say nothing until it does.
            SeedCopy::default()
        } else {
            SeedCopy::Unavailable
        };
    }

    /// The token the panel is currently offering to copy, or `None` when it is
    /// offering none — the shell's side of [`help_hit`](intrusion_core::help_hit)'s
    /// rule, read off the same [`State::level`](intrusion_core::State::level) the frame
    /// was drawn from so the key can never copy something the panel is not showing.
    fn seed_to_copy(&self) -> Option<String> {
        if self.ui.help_tab != HelpTab::LevelInfo {
            return None;
        }
        self.state.level().and_then(|level| level.encode())
    }

    /// The replay link the panel is currently offering to copy (§12.4/#411), or
    /// `None` when it is offering none — no Debug tab in this session (#459), a
    /// different tab up, or a run with no token for the link to carry. The link is
    /// the recording so far over this page's own URL ([`crate::replay::replay_url`]),
    /// so what a mid-run copy hands over is the run *up to this turn* — and a later
    /// copy, the longer one.
    ///
    /// **The link carries no debug state** (§12.6/#459): it is the level's token plus
    /// the input script, exactly as before the Debug tab existed, so replaying it hands
    /// over the run and never the session it was exported from.
    fn replay_to_copy(&self) -> Option<String> {
        if !self.ui.debug_mode || self.ui.help_tab != HelpTab::Debug {
            return None;
        }
        let token = self.state.level()?.encode()?;
        crate::replay::replay_url(&token, &intrusion_core::to_script(&self.recorded))
    }

    /// Record what the browser said about the copy and repaint (#353) — the microtask
    /// half of [`copy_seed`](Self::copy_seed). A view change like every other on this
    /// panel: no [`State`], no turn.
    fn note_seed_copy(&mut self, outcome: SeedCopy) {
        self.ui.seed_copy = outcome;
        self.draw();
    }

    /// Map a viewport point `(client_x, client_y)` to the **screen cell** under it at
    /// the current fit, or `None` for a point the pointer rule owns nowhere (a
    /// letterbox tap). The screen is `map + TOP_ROWS + BOTTOM_ROWS` rows fitted to the
    /// canvas, so a linear scale from the canvas rect gives the `(col, row)` the core
    /// drew — the one place the shell turns pixels into a grid coordinate, shared by
    /// every pointer hit-test so they can never disagree.
    ///
    /// It answers **one row past the frame's bottom edge** as well (§11.6/#386): a
    /// point in that strip, horizontally inside the canvas, reports row
    /// [`screen_height`](Self::screen_height) — the row [`tap_route`](crate::tap) reads
    /// as the ability bar's lower slack. A thumb aimed at the flush-right bar (§11.4)
    /// that lands a hair low is off the canvas entirely, and off-canvas is silence;
    /// that a fingertip is wider than a cell is a fact about touch, so the allowance
    /// lives here beside [`SWIPE_THRESHOLD_PX`] rather than in the core's exact
    /// hit-test. Bounded to one row and to the canvas's horizontal extent, so the
    /// letterbox margins stay as inert as they were.
    pub(crate) fn screen_cell(&self, client_x: f64, client_y: f64) -> Option<(u32, u32)> {
        let rect = self.canvas.get_bounding_client_rect();
        let (rw, rh) = (rect.width(), rect.height());
        if !(rw > 0.0 && rh > 0.0) {
            return None;
        }
        let (lx, ly) = (client_x - rect.left(), client_y - rect.top());
        if lx < 0.0 || ly < 0.0 || lx >= rw {
            return None; // outside the canvas (a letterbox tap)
        }
        let cols = self.state.layout().facility().width();
        let rows = self.screen_height();
        let col = (lx / rw * cols as f64).floor() as u32;
        if ly < rh {
            return Some((col, (ly / rh * rows as f64).floor() as u32));
        }
        // Below the frame: one row's height of slack, and no more.
        (ly < rh + rh / rows as f64).then_some((col, rows))
    }

    /// The screen's height in rows: the map plus the §11.4 status lines above it and
    /// the ability bar beneath. The one arithmetic every hit-test and the menu share,
    /// so none of them can disagree with the frame the core drew.
    pub(crate) fn screen_height(&self) -> u32 {
        self.state.layout().facility().height() + TOP_ROWS + BOTTOM_ROWS
    }
}

/// What a keypress means to the **running game** (§11.6): a bar slot to fire, or an
/// [`Input`] for the turn loop. Resolved by [`play_key`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PlayKey {
    /// An ability-bar slot, counting from `0` at the bar's leftmost drawn entry —
    /// which ability sits there is live state, so the rule stops at the slot.
    Slot(usize),
    /// A movement or wait, straight into the turn loop.
    Move(Input),
}

/// Resolve a keypress against §11.6's tables, **positions first** — the pure rule
/// behind #369, in the spirit of [`gesture_input`] and so natively tested below.
///
/// `key` is the character the layout produced, already folded through
/// `core::key_for_code` so a numpad key arrives as the arrow it means; `code` is the
/// physical key. `slot_for_letter` is the run's mnemonic lookup
/// (`core::ability_slot_for_letter`, #360) — a closure because *which* letters the
/// bar claims is a fact about the live loadout, and this rule is not.
///
/// The order is the fix. The bar's **digit** is asked first, off the code (#359),
/// because it names a *position* and a character table cannot see which of the two
/// digit blocks was pressed: consulting movement first is what made a top-row `2`
/// step south instead of firing slot 2, spending the turn and moving the player in
/// the bargain (§2.2). Then the **character** tables: movement and wait. Then the
/// **mnemonic letter** last, so a letter can never shadow a movement key even if the
/// mnemonic scheme's own reservation rule (`core::mnemonic`) were to change.
fn play_key(
    key: &str,
    code: &str,
    slot_for_letter: impl FnOnce(&str) -> Option<usize>,
) -> Option<PlayKey> {
    if let Some(slot) = ability_slot_for_code(code) {
        return Some(PlayKey::Slot(slot));
    }
    if let Some(input) = input_for_key(key) {
        return Some(PlayKey::Move(input));
    }
    slot_for_letter(key).map(PlayKey::Slot)
}

/// Install the keydown pump: each keypress drives one [`Game::handle_key`]. The
/// closure owns a clone of the `Rc` so the game outlives `start`; `forget` hands it to
/// the browser for the page's lifetime (the shell never tears down).
pub(crate) fn install_input(document: &Document, game: &Rc<RefCell<Game>>) -> Result<(), JsValue> {
    let game = game.clone();
    let cb = Closure::<dyn FnMut(KeyboardEvent)>::new(move |e: KeyboardEvent| {
        let mut game = game.borrow_mut();
        // A key — whatever it turns out to mean — says the player is on a keyboard
        // (§11.6/#323), including one they just picked up mid-session.
        game.note_modality(InputModality::Keys);
        // `e.repeat()` is the browser's own held-key auto-repeat flag (§11.6): the
        // first keydown is fresh, every held-down repeat after it carries `repeat ==
        // true`. The shell forwards it so the core rule (#223) can treat a held
        // repeat differently from a deliberate press without the pump interpreting.
        //
        // `e.code()` rides along beside `e.key()` because some bindings are physical
        // (#359): `key` is what the layout printed, `code` is which key it was, and
        // the core decides which of the two each binding reads.
        if game.handle_key(&e.key(), &e.code(), e.repeat()) {
            e.prevent_default();
        }
    });
    document.add_event_listener_with_callback("keydown", cb.as_ref().unchecked_ref())?;
    cb.forget();
    Ok(())
}

/// What a press of this `PointerEvent.pointerType` says about the player's hands
/// (§11.6/#323), or `None` when it says nothing: a **finger or a pen** is a touch
/// session outright, and a **mouse** is left alone deliberately — a click is
/// neither of the gestures the touch hint teaches, and a desktop player who reaches
/// for the ability bar with the pointer has not stopped being a keyboard player.
/// An unknown `pointerType` is treated like the mouse: no claim.
///
/// Pure, so the rule is pinned natively below like [`gesture_input`].
fn modality_of_pointer(pointer_type: &str) -> Option<InputModality> {
    match pointer_type {
        "touch" | "pen" => Some(InputModality::Touch),
        _ => None,
    }
}

/// The modality a fresh load opens on (§11.6/#323), from the `pointer: coarse`
/// media query: what the *device*'s primary pointer is, before the player has
/// touched anything. It is only a seed — the first key or finger corrects it
/// ([`Game::note_modality`]) — which is what makes the query's known weakness
/// harmless here: it answers for the device, and a hybrid device's answer is a
/// guess either way. A browser without `matchMedia` gets [`InputModality::Keys`],
/// the same default the core has.
pub(crate) fn boot_modality() -> InputModality {
    let coarse = web_sys::window()
        .and_then(|w| w.match_media("(pointer: coarse)").ok().flatten())
        .is_some_and(|q| q.matches());
    if coarse {
        InputModality::Touch
    } else {
        InputModality::Keys
    }
}

/// How far a drag must travel from its press point — CSS pixels, on either axis —
/// before it reads as a **swipe** rather than a press held in place. Roughly half
/// a fingertip: short enough that a flick registers, long enough that the jitter
/// of a resting finger never walks the player. Shared with the replay scrub pump
/// ([`crate::replay`]) so the touch feel of a swipe is one number across modes.
pub(crate) const SWIPE_THRESHOLD_PX: f64 = 24.0;

/// The pause between a gesture's first input and its first repeat — the touch
/// counterpart of the keyboard's auto-repeat delay (§11.6's reference cadence).
/// Long enough that one deliberate swipe or press stays a single input.
pub(crate) const REPEAT_DELAY_MS: i32 = 300;

/// The cadence of repeats while the finger stays down — one ordinary [`Input`]
/// per tick through the same seam as a held arrow key, never a batch (§4.1/§4.3).
pub(crate) const REPEAT_INTERVAL_MS: i32 = 120;

/// Map a drag displacement `(dx, dy)` — CSS pixels from where the finger went
/// down to where it is now — to the [`Gesture`] it *is*: **what the finger did**,
/// with no surface's meaning attached (§11.6/#336). Pure, so the rule is testable
/// natively; what the gesture then *means* is a core binding table
/// ([`Game::gesture_command`]), the way a key's meaning is.
///
/// Inside [`SWIPE_THRESHOLD_PX`] on both axes the finger has stayed put: a
/// [`Press`](Gesture::Press). Past it, the drag is a [`Swipe`](Gesture::Swipe)
/// along its dominant axis — no diagonals (§4.1 [SETTLED]) — with an exact tie
/// going horizontal. The pump re-reads the live displacement on every repeat tick,
/// so dragging to a new heading re-aims mid-hold and pulling back inside the
/// threshold turns it into a press again; nothing is cached but the gesture's
/// origin. A non-finite displacement maps to nothing rather than a garbage turn.
fn drag_gesture(dx: f64, dy: f64) -> Option<Gesture> {
    if !(dx.is_finite() && dy.is_finite()) {
        return None;
    }
    if dx.abs() < SWIPE_THRESHOLD_PX && dy.abs() < SWIPE_THRESHOLD_PX {
        return Some(Gesture::Press);
    }
    let direction = if dx.abs() >= dy.abs() {
        if dx < 0.0 {
            Direction::West
        } else {
            Direction::East
        }
    } else if dy < 0.0 {
        Direction::North
    } else {
        Direction::South
    };
    Some(Gesture::Swipe(direction))
}

/// What a gesture resolved to on the surface currently up (§11.6/#336) — one
/// variant per command type the three bindings produce, so the pump can carry a
/// resolved gesture across a `RefCell` borrow without knowing which screen it came
/// from.
///
/// Only [`Play`](Self::Play) reaches the turn loop. That is the line the two
/// board-only gates are drawn along: the #223 danger gate and #306's dead band both
/// ask questions about a board, and a title screen has none.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum GestureCommand {
    /// An ordinary board input — the only kind that steps the world and spends a
    /// turn (§4.1/§4.3).
    Play(Input),
    /// A title-screen navigation (§14/#268): walk the list. Costs no turn, changes
    /// no [`State`](intrusion_core::State).
    Menu(MenuNav),
    /// A navigation on the campaign map (§14 v3/#208): walk the facilities on offer.
    /// Free like every other screen navigation (§4.4) — the campaign moves when a row is
    /// fired, and a swipe never fires one.
    Map(MapNav),
    /// A navigation inside the open help panel (§14 v2/#248): walk the tab bar.
    /// Likewise free (§4.4).
    Help(HelpNav),
    /// A navigation on the end screen (§14 v2/#138): walk the exits. Free too — a
    /// finished run has no turn left to spend.
    End(EndNav),
}

/// Which surface a [`Gesture`] is aimed at, and what it means there (§11.6/#336) —
/// the pure rule behind [`Game::gesture_command`], in the spirit of
/// [`drag_gesture`], so the *order* the shell owns is pinned natively too.
///
/// The precedence is the keyboard's, deliberately: menu, then the campaign map, then the
/// end screen, then help, then the board. The map sits above the end screen because
/// between facilities there is a *finished* raid on the state underneath it (#208).
/// Only the last arm can produce a [`Play`](GestureCommand::Play), which is what
/// makes the two board-only gates in [`GesturePump::apply`] structurally
/// unreachable from a modal screen — #223 asks about visible danger and #306's dead
/// band about the board's edges, and a title screen has neither.
fn surface_command(
    menu_up: bool,
    map_open: bool,
    run_over: bool,
    help_open: bool,
    gesture: Gesture,
) -> Option<GestureCommand> {
    if menu_up {
        return menu_nav_for_gesture(gesture).map(GestureCommand::Menu);
    }
    if map_open {
        return map_nav_for_gesture(gesture).map(GestureCommand::Map);
    }
    if run_over {
        return end_nav_for_gesture(gesture).map(GestureCommand::End);
    }
    if help_open {
        return help_nav_for_gesture(gesture).map(GestureCommand::Help);
    }
    input_for_gesture(gesture).map(GestureCommand::Play)
}

/// Whether a held movement's **repeat** should be suppressed this tick — the pure
/// §11.6 rule behind #223, in the spirit of [`drag_gesture`]. Given the player's
/// cell, the repeat's [`Input`], and a membership test for the §11.5 danger set
/// (`in_danger`, wired to
/// [`State::in_visible_danger`](intrusion_core::State::in_visible_danger)), it says
/// whether this repeat would walk the player into visible danger and so must not
/// fire.
///
/// Only a `Step` is ever gated, and only when it touches the danger set: its
/// destination cell is watched ("a step would move you in"), or the player already
/// stands in one ("you have just entered one" — the deliberate first step landed on
/// the cone edge). A held press-in-place is `Wait` and must keep waiting (§11.6), so
/// it is never gated, and a repeat on safe ground fires normally. Fresh presses
/// never reach here — the caller routes only repeats — so a single deliberate step
/// into a cone is always allowed.
fn repeat_suppressed(player: Cell, input: Input, in_danger: impl Fn(Cell) -> bool) -> bool {
    let Input::Step(direction) = input else {
        return false;
    };
    in_danger(player) || player.step(direction).is_some_and(in_danger)
}

/// The browser timer currently driving a gesture's repeats: the one-shot initial
/// delay (`setTimeout`) or the steady cadence (`setInterval`). Whichever is
/// armed, release clears it by id — that clear is what guarantees no step or
/// wait ever fires after the finger lifts (§2.2/§4.5 fairness). Shared with the
/// replay scrub pump ([`crate::replay`]), which owns the same lift-stops-instantly
/// contract on the time cursor.
#[derive(Clone, Copy)]
pub(crate) enum RepeatTimer {
    Delay(i32),
    Interval(i32),
}

/// Clear an armed [`RepeatTimer`] with the browser. Clearing an id that already
/// fired is a harmless no-op, so teardown never has to know the timer's fate.
pub(crate) fn clear_timer(timer: RepeatTimer) {
    let win = web_sys::window().expect("a window");
    match timer {
        RepeatTimer::Delay(id) => win.clear_timeout_with_handle(id),
        RepeatTimer::Interval(id) => win.clear_interval_with_handle(id),
    }
}

/// What the pointer currently down is doing — decided once, at the press, by
/// [`Game::tap_at`] (§11.6/#306). The two are exclusive: a press on a control arms
/// **only** the control and never also a gesture, or a press dragged onto the board
/// would both abandon the button and walk the player.
enum Pointer {
    /// A press that landed on a chrome control: **armed**, and fired on the lift over
    /// that same control. Resolving on the lift rather than the press is what lets a
    /// mis-press be slid off and abandoned (§2.2/§4.5) — the same rule the gesture
    /// path already honours on `pointercancel` — and it puts both surfaces' resolution
    /// at the same moment, so they behave alike.
    Armed { pointer_id: i32, control: Control },
    /// A press the controls declined: the swipe / hold / tap gesture, aimed at
    /// whichever surface is up (#336).
    Drag(Drag),
}

impl Pointer {
    /// The pointer that owns this press; other fingers are ignored while it lives.
    fn pointer_id(&self) -> i32 {
        match self {
            Pointer::Armed { pointer_id, .. } => *pointer_id,
            Pointer::Drag(d) => d.pointer_id,
        }
    }
}

/// What a lift resolved to — decided while the pump's state is borrowed, applied
/// after it is released.
enum Lift {
    /// The armed control, to fire only if the lift is still over it.
    Control(Control),
    /// The unfired drag's own gesture: a tap's press, or a flick too fast for a
    /// pointermove to have seen.
    Drag(Gesture),
    /// Nothing to apply — a gesture that already fired, or an abandoned press.
    Nothing,
}

/// One finger's live drag: where it pressed, where it is now, and the timer
/// keeping it repeating. Exists only while that pointer is down — release (or a
/// browser cancel) destroys it and its timer together.
///
/// It holds no *meaning*, only geometry: what the finger is doing is re-read from
/// the live displacement through [`drag_gesture`] on every tick (#336).
struct Drag {
    /// The pointer that owns the gesture; other fingers are ignored while it lives.
    pointer_id: i32,
    /// Where the pointer went down, in viewport CSS pixels.
    origin: (f64, f64),
    /// Live displacement from `origin`, updated on every pointermove. Each repeat
    /// tick re-reads it through [`gesture_input`], so the heading is never stale.
    delta: (f64, f64),
    /// Whether the gesture has produced its first input yet — the threshold-crossing
    /// step of a swipe, or the first Wait of a matured hold. A release before either
    /// makes the gesture a tap, resolved at the lift.
    fired: bool,
    /// The armed repeat timer, cleared the moment the gesture ends.
    timer: RepeatTimer,
}

impl Drag {
    /// Where the finger is **now**, in viewport CSS pixels — the origin plus the live
    /// displacement. The Wait-producing gestures are routed against this rather than
    /// the origin (#306), so it is the same point a lift would resolve at: drag out of
    /// the dead band and a held press starts waiting, drag into it and it stops.
    fn point(&self) -> (f64, f64) {
        (self.origin.0 + self.delta.0, self.origin.1 + self.delta.1)
    }
}

/// The gesture pump — §11.6's touch half, replacing the old edge-zone tap model.
///
/// A **swipe** steps along the drag's dominant axis the instant it crosses
/// [`SWIPE_THRESHOLD_PX`], and *keeps* stepping while the finger stays down. A
/// **press held in place** matures into Wait after [`REPEAT_DELAY_MS`], and keeps
/// waiting. A **quick tap** (released before either) is a single Wait, resolved
/// at the lift — the gesture's own input, not a repeat. After a gesture's first
/// input, the next comes [`REPEAT_DELAY_MS`] later for a swipe (a matured hold is
/// already the delay timer firing), then every [`REPEAT_INTERVAL_MS`] — the held
/// arrow key's cadence (§11.6). Every tick re-reads the live displacement, so
/// dragging to a new heading re-aims the walk without lifting.
///
/// Fairness (§2.2/§4.5): each tick feeds exactly one ordinary [`Input`] through
/// [`Game::step_and_draw`] against the current frame — never queued ahead — and
/// release/cancel clears the timer before anything else can fire, so no step or
/// wait ever lands after the finger lifts. A cancelled gesture (the browser took
/// the pointer, or it left the page) emits nothing at all, not even the tap's
/// Wait — a turn must never burn on a gesture the player didn't finish. **A tap the
/// player aimed at a button is such a gesture** (#306), which is why every
/// Wait-producing resolution here is routed through [`Game::tap_at`] and every chrome
/// control resolves on the lift.
struct GesturePump {
    game: Rc<RefCell<Game>>,
    /// The live press, if a finger is down: an armed control or a gesture.
    active: RefCell<Option<Pointer>>,
    /// The repeat tick — **one closure for the page's lifetime**, registered with
    /// `setTimeout`/`setInterval` afresh for each gesture. Storing it here (an Rc
    /// cycle, deliberately never freed) mirrors the `Closure::forget` lifetime
    /// pattern of the listeners below without leaking a closure per gesture.
    tick: RefCell<Option<Closure<dyn FnMut()>>>,
}

impl GesturePump {
    /// Arm the repeat tick with the browser — the one-shot initial delay or the
    /// steady interval — and hand back the id for the gesture to own.
    fn arm(&self, ms: i32, as_interval: bool) -> i32 {
        let win = web_sys::window().expect("a window");
        let tick = self.tick.borrow();
        let f = tick
            .as_ref()
            .expect("the tick closure is installed at boot")
            .as_ref()
            .unchecked_ref();
        if as_interval {
            win.set_interval_with_callback_and_timeout_and_arguments_0(f, ms)
        } else {
            win.set_timeout_with_callback_and_timeout_and_arguments_0(f, ms)
        }
        .expect("the browser arms a timer")
    }

    /// A pointer pressed: route the point once (§11.6/#306) and either **arm** the
    /// control under it — nothing fires yet — or start the gesture. Only the primary
    /// button presses, and a second finger neither starts a second press nor re-aims
    /// the first.
    fn on_down(&self, e: &PointerEvent) {
        if e.button() != 0 {
            return; // secondary mouse buttons keep their browser meaning
        }
        // A finger on the glass says the player is on touch (§11.6/#323) — noted
        // before the press resolves, so the hint is already in the gesture
        // vocabulary on the frame this press draws.
        if let Some(modality) = modality_of_pointer(&e.pointer_type()) {
            self.game.borrow_mut().note_modality(modality);
        }
        let (x, y) = (e.client_x() as f64, e.client_y() as f64);
        let tap = self.game.borrow().tap_at(x, y);
        {
            let mut active = self.active.borrow_mut();
            if active.is_none() {
                *active = match tap {
                    // A control — the menu's entries, the help panel's tabs and `[x]`,
                    // the `[?]` toggle, the message counter, an ability slot. Armed
                    // only: it fires on the lift over the same control, and it starts
                    // no gesture.
                    Tap::Control(control) => Some(Pointer::Armed {
                        pointer_id: e.pointer_id(),
                        control,
                    }),
                    // Anything the controls declined starts a drag — **including a
                    // modal screen's captured press** (#336). There may be no world
                    // underneath, but there is always a surface: the menu's list and
                    // the help panel's tab bar are walked by the same pump, through
                    // their own bindings ([`Game::gesture_command`]). A drag may begin
                    // *anywhere* the controls declined — the dead band and the chrome
                    // included — because a swipe from there is unambiguous; it is only
                    // the board's Wait that the routing gates, at the moment the drag
                    // resolves. (The seed box's own presses never reach here; its panel
                    // stops them at itself.)
                    Tap::Captured | Tap::Wait | Tap::Nothing => Some(Pointer::Drag(Drag {
                        pointer_id: e.pointer_id(),
                        origin: (x, y),
                        delta: (0.0, 0.0),
                        fired: false,
                        timer: RepeatTimer::Delay(self.arm(REPEAT_DELAY_MS, false)),
                    })),
                };
            }
        }
        // Consumed either way (§11.6): gestures are game input, and the browser's
        // follow-ups (double-tap zoom, synthetic clicks) must not fire off them.
        e.prevent_default();
    }

    /// The drag's pointer moved: track the live displacement, and the instant it
    /// first crosses the swipe threshold fire that swipe — the gesture declaring
    /// itself — restarting the repeat cadence from it exactly as a fresh keydown
    /// would. Whichever surface is up receives it (#336); on the board it is the
    /// first step, on the menu the first move of the selection.
    ///
    /// An armed control ignores moves entirely: the lift re-routes the point, so
    /// sliding off and back on again is decided once, at the end.
    fn on_move(&self, e: &PointerEvent) {
        let first_swipe = {
            let mut active = self.active.borrow_mut();
            let Some(Pointer::Drag(d)) =
                active.as_mut().filter(|p| p.pointer_id() == e.pointer_id())
            else {
                return;
            };
            d.delta = (
                e.client_x() as f64 - d.origin.0,
                e.client_y() as f64 - d.origin.1,
            );
            let gesture = drag_gesture(d.delta.0, d.delta.1);
            if !d.fired && matches!(gesture, Some(Gesture::Swipe(_))) {
                d.fired = true;
                clear_timer(d.timer);
                d.timer = RepeatTimer::Delay(self.arm(REPEAT_DELAY_MS, false));
                gesture.map(|g| (g, d.point()))
            } else {
                None
            }
        };
        if let Some((gesture, at)) = first_swipe {
            self.apply(gesture, at, false);
        }
    }

    /// The armed timer fired: feed one gesture re-read from the live displacement —
    /// a press, a swipe, whichever the finger says *now* — to whichever surface is
    /// up, and, if this was the one-shot delay, settle into the steady cadence. This
    /// is the repeat, so it is the tick the #223 danger gate reads.
    fn on_tick(&self) {
        let tick = {
            let mut active = self.active.borrow_mut();
            let Some(Pointer::Drag(d)) = active.as_mut() else {
                return; // released while the tick was in flight — nothing may fire
            };
            d.fired = true;
            if let RepeatTimer::Delay(_) = d.timer {
                d.timer = RepeatTimer::Interval(self.arm(REPEAT_INTERVAL_MS, true));
            }
            drag_gesture(d.delta.0, d.delta.1).map(|gesture| (gesture, d.point()))
        };
        if let Some((gesture, at)) = tick {
            self.apply(gesture, at, true);
        }
    }

    /// Apply one resolved [`Gesture`] to the surface that is up (§11.6/#336) — the
    /// single place the pump's *plumbing* meets a *meaning*, so the two board-only
    /// gates below are written once and can never leak onto a screen that has no
    /// board. `at` is where the finger is now; `repeat` says this came from the
    /// repeat cadence rather than the gesture's own first input.
    ///
    /// A gesture the showing surface declines spends nothing, and the cadence is
    /// deliberately left running either way: drag to a heading the surface *does*
    /// bind and the next tick fires.
    fn apply(&self, gesture: Gesture, at: (f64, f64), repeat: bool) {
        let mut game = self.game.borrow_mut();
        match game.gesture_command(gesture) {
            None => {}
            // The modal screens: a free view action (§4.4), so neither gate applies —
            // #223 asks about visible danger and #306's dead band about the board's
            // edges, and a title screen has neither. It owns the whole viewport, so
            // there is nowhere on it a swipe could be a misaimed something else.
            Some(GestureCommand::Menu(nav)) => game.apply_menu_nav(nav),
            Some(GestureCommand::Map(nav)) => game.apply_map_nav(nav),
            Some(GestureCommand::End(nav)) => game.apply_end_nav(nav),
            Some(GestureCommand::Help(nav)) => {
                game.apply_help_nav(nav);
                game.draw();
            }
            Some(GestureCommand::Play(input)) => {
                // A held swipe never auto-walks into visible danger (§11.6/#223): the
                // repeat is swallowed at the cone edge, the cadence left running so
                // dragging to a safe heading fires again — but going deeper needs a
                // fresh gesture. A held Wait (press-in-place) is never gated and keeps
                // waiting. Board-only, and repeats only: the deliberate first input
                // always lands.
                if repeat && game.repeat_into_danger(input) {
                    return;
                }
                // A press held **in place** only waits where a tap would (§11.6/#306): a
                // resting finger on the chrome, in the dead band or off the canvas is more
                // likely a missed button than a deliberate hold, so it produces nothing.
                // The cadence is left running — drag out onto clear board and it waits.
                if !gesture_lands(input, game.tap_at(at.0, at.1)) {
                    return;
                }
                game.step_and_draw(input);
            }
        }
    }

    /// The pointer lifted — **the one moment both surfaces resolve at** (§11.6/#306).
    ///
    /// An armed control fires only if the lift still lands on it, so a press slid off
    /// its cells (or onto a different control) is abandoned, spending nothing. A
    /// gesture stops every repeat immediately and, if it never fired, resolves as the
    /// tap it was — at the lift point, so a press in place is one Wait and a flick too
    /// fast for a pointermove still steps. That input is the gesture's own, not a
    /// repeat leaking past the lift.
    fn on_up(&self, e: &PointerEvent) {
        let (x, y) = (e.client_x() as f64, e.client_y() as f64);
        let lift = {
            let mut active = self.active.borrow_mut();
            if !matches!(active.as_ref(), Some(p) if p.pointer_id() == e.pointer_id()) {
                return;
            }
            match active.take().expect("matched just above") {
                Pointer::Armed { control, .. } => Lift::Control(control),
                Pointer::Drag(d) => {
                    clear_timer(d.timer);
                    match drag_gesture(x - d.origin.0, y - d.origin.1) {
                        Some(gesture) if !d.fired => Lift::Drag(gesture),
                        _ => Lift::Nothing,
                    }
                }
            }
        };
        e.prevent_default();
        match lift {
            Lift::Nothing => {}
            Lift::Control(armed) => {
                let mut game = self.game.borrow_mut();
                if armed_fires(armed, game.tap_at(x, y)) {
                    game.apply_control(armed);
                }
            }
            // The gesture's own input, not a repeat — so `repeat` is false and the
            // #223 gate does not read it. On the board the tap's Wait is still the
            // ambiguous half #306 gates, inside `apply`; on a modal screen a tap is
            // bound to nothing at all, which is what keeps a stray one from starting
            // a run (§2.1).
            Lift::Drag(gesture) => self.apply(gesture, (x, y), false),
        }
    }

    /// The browser took the press away (`pointercancel`) or the pointer left the
    /// page (`pointerleave`): tear down without emitting anything — not even the
    /// tap's Wait, nor an armed control. A turn must never burn on a gesture the
    /// player didn't end.
    fn on_abort(&self, e: &PointerEvent) {
        let mut active = self.active.borrow_mut();
        if !matches!(active.as_ref(), Some(p) if p.pointer_id() == e.pointer_id()) {
            return;
        }
        if let Some(Pointer::Drag(d)) = active.take() {
            clear_timer(d.timer);
        }
    }
}

/// Whether the `input` a gesture resolved to may land where the finger is (§11.6/#306)
/// — the pure half of the dead band, in the spirit of [`gesture_input`].
///
/// A `Wait` is the **ambiguous** gesture: zero displacement says nothing about what
/// the player meant, so it lands only on [`Tap::Wait`] — board clear of the chrome and
/// its dead band. Every other input is unconditional: **swipes are exempt**, because a
/// directional drag is unambiguous wherever it started, so the band costs no movement.
fn gesture_lands(input: Input, tap: Tap) -> bool {
    input != Input::Wait || tap == Tap::Wait
}

/// Whether an **armed** control fires on the lift: only when the lift still routes to
/// that same control (§11.6/#306). Sliding off its cells — onto the board, onto a
/// neighbouring control, into the letterbox — abandons it, spending nothing, the same
/// rule the gesture path already honours on `pointercancel` (§2.2/§4.5).
fn armed_fires(armed: Control, tap: Tap) -> bool {
    tap == Tap::Control(armed)
}

/// Install the gesture pump (§11.6's touch half): pointer listeners anywhere on
/// the page — the letterbox margins count too — feed one [`GesturePump`], which
/// owns the repeat timer and the live gesture. `preventDefault` on the consumed
/// press stops the browser's gesture follow-ups (double-tap zoom, synthetic mouse
/// events); `touch-action: none` on the page covers the rest (see `web/index.html`).
/// Each listener closure is `forget`ed for the page's lifetime, like the key pump.
pub(crate) fn install_gestures(
    document: &Document,
    game: &Rc<RefCell<Game>>,
) -> Result<(), JsValue> {
    let pump = Rc::new(GesturePump {
        game: game.clone(),
        active: RefCell::new(None),
        tick: RefCell::new(None),
    });
    let p = pump.clone();
    *pump.tick.borrow_mut() = Some(Closure::<dyn FnMut()>::new(move || p.on_tick()));

    type Handler = fn(&GesturePump, &PointerEvent);
    let listeners: [(&str, Handler); 5] = [
        ("pointerdown", GesturePump::on_down),
        ("pointermove", GesturePump::on_move),
        ("pointerup", GesturePump::on_up),
        ("pointercancel", GesturePump::on_abort),
        ("pointerleave", GesturePump::on_abort),
    ];
    for (event, handler) in listeners {
        let p = pump.clone();
        let cb = Closure::<dyn FnMut(PointerEvent)>::new(move |e: PointerEvent| handler(&p, &e));
        document.add_event_listener_with_callback(event, cb.as_ref().unchecked_ref())?;
        cb.forget();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use intrusion_core::{AbilityId, MapNav, MenuEntry, MenuHit};

    /// #369, the reported bug, at the seam it lived on: a **top-row** `2` fires the
    /// bar's second slot. The press arrives as `code: "Digit2"`, `key: "2"` — the
    /// character the movement table used to answer first, stepping the player south
    /// and spending the turn instead of firing the ability. All four digits, since
    /// slots 1 and 3 worked by luck (nothing claimed `1` or `3`) and that is exactly
    /// what hid the bug.
    #[test]
    fn a_top_row_digit_fires_its_bar_slot_and_never_steps() {
        for (code, key, slot) in [
            ("Digit1", "1", 0),
            ("Digit2", "2", 1),
            ("Digit3", "3", 2),
            ("Digit4", "4", 3),
        ] {
            assert_eq!(
                play_key(key, code, |_| None),
                Some(PlayKey::Slot(slot)),
                "{code} fires slot {slot}",
            );
        }
    }

    /// …and the other half of the same split: the **numpad** still moves. It arrives
    /// with its own codes and is folded to the arrows before this rule sees it, so
    /// `Numpad2` steps south where `Digit2` fires a slot — the two digit blocks kept
    /// apart by the only thing that can tell them apart, the code.
    #[test]
    fn the_numpad_still_steps_and_waits() {
        for (code, expected) in [
            ("Numpad8", Input::Step(Direction::North)),
            ("Numpad2", Input::Step(Direction::South)),
            ("Numpad4", Input::Step(Direction::West)),
            ("Numpad6", Input::Step(Direction::East)),
            ("Numpad5", Input::Wait),
        ] {
            let key = key_for_code(code).expect("the numpad folds");
            assert_eq!(
                play_key(key, code, |_| None),
                Some(PlayKey::Move(expected)),
                "{code}",
            );
        }
    }

    /// The precedence in full (§11.6): a **position** outranks every character table,
    /// then movement, then the run's **mnemonic letter** last — so a letter can never
    /// shadow a step even if the mnemonic scheme stopped reserving the movement keys.
    /// A key no table owns is left to the page.
    #[test]
    fn play_resolves_position_then_movement_then_mnemonic() {
        // A mnemonic lookup greedy enough to claim anything it is offered: it still
        // never sees a digit or a movement key, because both are answered above it.
        let greedy = |_: &str| Some(7);
        assert_eq!(
            play_key("2", "Digit2", greedy),
            Some(PlayKey::Slot(1)),
            "the bar's digit outranks the mnemonic",
        );
        assert_eq!(
            play_key("ArrowLeft", "ArrowLeft", greedy),
            Some(PlayKey::Move(Input::Step(Direction::West))),
            "a movement key outranks the mnemonic",
        );
        for (key, code) in [("c", "KeyC"), ("l", "KeyL")] {
            assert_eq!(
                play_key(key, code, greedy),
                Some(PlayKey::Slot(7)),
                "{key:?} is a free letter and reaches the mnemonic lookup",
            );
        }
        for (key, code) in [("q", "KeyQ"), ("F5", "F5"), ("5", "Digit5")] {
            assert_eq!(
                play_key(key, code, |_| None),
                None,
                "{key:?} is left to the page",
            );
        }
    }

    /// **The board's** reading of a drag: the composition the pump makes there
    /// (§11.6/#336) — the shell's arithmetic, then the core's board binding. The
    /// gesture tests below are the ones that shipped with the pump and they assert
    /// exactly the [`Input`]s they always did: splitting the vocabulary out changed
    /// where the meaning is written down, and nothing about what a finger does.
    fn gesture_input(dx: f64, dy: f64) -> Option<Input> {
        drag_gesture(dx, dy).and_then(input_for_gesture)
    }

    /// §11.6's hold rule: a press that never crosses the swipe threshold is Wait —
    /// from the zero-displacement press up to the last sub-threshold pixel, on
    /// both axes and in every quadrant. The resting-finger jitter of a hold must
    /// never walk the player.
    #[test]
    fn a_press_inside_the_threshold_holds_to_wait() {
        let just_under = SWIPE_THRESHOLD_PX - 0.5;
        for (dx, dy) in [
            (0.0, 0.0),
            (just_under, 0.0),
            (0.0, -just_under),
            (-just_under, just_under),
            (just_under, just_under),
        ] {
            assert_eq!(
                gesture_input(dx, dy),
                Some(Input::Wait),
                "drag of ({dx}, {dy})"
            );
        }
    }

    /// A swipe resolves to the nearest cardinal: the dominant axis of the drag,
    /// in all four directions, including well off-axis drags — movement has no
    /// diagonals (§4.1).
    #[test]
    fn a_swipe_steps_its_dominant_axis() {
        for ((dx, dy), direction) in [
            ((-40.0, 10.0), Direction::West),
            ((40.0, -10.0), Direction::East),
            ((10.0, -40.0), Direction::North),
            ((-10.0, 40.0), Direction::South),
        ] {
            assert_eq!(
                gesture_input(dx, dy),
                Some(Input::Step(direction)),
                "drag of ({dx}, {dy})"
            );
        }
    }

    /// The threshold itself swipes — reaching it is crossing it — and an exact
    /// diagonal tie goes horizontal, the old tap model's convention kept.
    #[test]
    fn the_threshold_boundary_swipes_and_ties_go_horizontal() {
        let t = SWIPE_THRESHOLD_PX;
        assert_eq!(
            gesture_input(t, 0.0),
            Some(Input::Step(Direction::East)),
            "the boundary is a swipe"
        );
        assert_eq!(gesture_input(t, t), Some(Input::Step(Direction::East)));
        assert_eq!(gesture_input(-t, -t), Some(Input::Step(Direction::West)));
    }

    /// The live re-evaluation contract: the function is pure in the displacement,
    /// so a repeat tick re-reading the drag changes heading with the finger — a
    /// swipe dragged to a new quadrant re-aims, and one pulled back inside the
    /// threshold becomes a hold. No direction is ever cached.
    #[test]
    fn a_dragging_finger_re_aims_the_repeat_live() {
        assert_eq!(gesture_input(40.0, 0.0), Some(Input::Step(Direction::East)));
        assert_eq!(
            gesture_input(6.0, -35.0),
            Some(Input::Step(Direction::North))
        );
        assert_eq!(gesture_input(3.0, -3.0), Some(Input::Wait));
    }

    /// A non-finite displacement maps to nothing rather than a garbage turn.
    #[test]
    fn a_non_finite_drag_is_ignored() {
        assert_eq!(gesture_input(f64::NAN, 0.0), None);
        assert_eq!(gesture_input(0.0, f64::NEG_INFINITY), None);
    }

    /// #223's core rule, pure and native. Standing on safe ground, a held `Step`
    /// repeat is suppressed once its **destination** is a danger cell — the hold
    /// halts at the cone edge — while a repeat away from the danger fires normally.
    #[test]
    fn a_step_repeat_into_a_cone_is_suppressed_at_the_edge() {
        // A single danger cell to the north of the player: the overlay set as a set.
        let danger = Cell::new(5, 4);
        let player = Cell::new(5, 5);
        let in_danger = |c: Cell| c == danger;
        // North steps *into* the cone — suppressed; the other three are clear.
        assert!(repeat_suppressed(
            player,
            Input::Step(Direction::North),
            in_danger
        ));
        for dir in [Direction::South, Direction::East, Direction::West] {
            assert!(
                !repeat_suppressed(player, Input::Step(dir), in_danger),
                "a {dir:?} repeat onto clear ground fires"
            );
        }
    }

    /// The "just entered" half: once the player *stands* in a danger cell (the
    /// deliberate first step having landed on the cone), every held repeat is
    /// suppressed — even one stepping back out — so escaping the cone is a fresh,
    /// deliberate press each time, never a blind march.
    #[test]
    fn a_step_repeat_while_standing_in_a_cone_is_suppressed_in_any_direction() {
        let player = Cell::new(5, 5);
        let in_danger = |c: Cell| c == player; // the player's own cell is watched
        for dir in Direction::ALL {
            assert!(
                repeat_suppressed(player, Input::Step(dir), in_danger),
                "in danger, a {dir:?} repeat stops"
            );
        }
    }

    /// A held press-in-place is `Wait`, and an activation repeat is `Activate` —
    /// neither is a step across the cone edge, so neither is ever gated. A held Wait
    /// must keep waiting (§11.6's hold model), even while standing in danger.
    #[test]
    fn only_step_repeats_are_gated() {
        let player = Cell::new(5, 5);
        let all_danger = |_: Cell| true; // the worst case: everything is watched
        assert!(!repeat_suppressed(player, Input::Wait, all_danger));
        assert!(!repeat_suppressed(
            player,
            Input::Activate(AbilityId::Run),
            all_danger
        ));
    }

    /// #306's dead band, from the gesture's side: **a swipe is exempt.** A `Step`
    /// lands wherever the finger is — the band, the chrome, off the canvas — because a
    /// directional drag is unambiguous, so the band costs no movement. Only the
    /// zero-displacement `Wait` is gated, and it lands on [`Tap::Wait`] alone.
    #[test]
    fn only_the_ambiguous_wait_is_gated_by_where_the_finger_is() {
        for tap in [Tap::Wait, Tap::Nothing, Tap::Captured] {
            assert!(
                gesture_lands(Input::Step(Direction::North), tap),
                "a swipe steps at {tap:?}"
            );
        }
        assert!(
            gesture_lands(Input::Wait, Tap::Wait),
            "board clear of the bars"
        );
        for tap in [
            Tap::Nothing, // the chrome, the dead band, the letterbox
            Tap::Captured,
            Tap::Control(Control::HelpToggle),
        ] {
            assert!(
                !gesture_lands(Input::Wait, tap),
                "a tap or a held press at {tap:?} spends no turn"
            );
        }
    }

    /// #306's lift rule: an armed control fires only if the lift is still over that
    /// same control. Sliding off onto the board, onto a *different* control, or into
    /// the letterbox abandons the press — nothing fires and nothing steps.
    #[test]
    fn an_armed_control_fires_only_where_it_was_armed() {
        let armed = Control::Ability(AbilityId::Run);
        assert!(armed_fires(armed, Tap::Control(armed)));
        for tap in [
            Tap::Control(Control::HelpToggle),
            Tap::Control(Control::Ability(AbilityId::Camouflage)),
            Tap::Wait,
            Tap::Nothing,
            Tap::Captured,
        ] {
            assert!(
                !armed_fires(armed, tap),
                "lifting at {tap:?} abandons the press"
            );
        }
    }

    /// §11.6/#323: a press only claims the touch modality when it is a **finger or a
    /// pen**. A mouse — and anything the browser will not name — leaves the hint's
    /// vocabulary alone, because a click is neither of the two gestures the touch
    /// hint teaches and a desktop player clicking the bar is still on the keyboard.
    #[test]
    fn only_a_finger_or_a_pen_claims_the_touch_modality() {
        assert_eq!(
            modality_of_pointer("touch"),
            Some(InputModality::Touch),
            "a finger is touch"
        );
        assert_eq!(
            modality_of_pointer("pen"),
            Some(InputModality::Touch),
            "so is a stylus"
        );
        for kind in ["mouse", "", "trackpad"] {
            assert_eq!(
                modality_of_pointer(kind),
                None,
                "a {kind:?} press claims nothing"
            );
        }
    }

    /// A step that would leave the grid's north/west edge has no destination cell
    /// ([`Cell::step`] is `None`): the repeat is judged on the player's own cell
    /// alone — safe there, it fires (a harmless bump); in danger there, it stops.
    #[test]
    fn a_step_off_the_grid_edge_is_judged_on_the_player_cell_alone() {
        let corner = Cell::new(0, 0);
        // Off-grid destination, player on clear ground: not suppressed.
        assert!(!repeat_suppressed(
            corner,
            Input::Step(Direction::North),
            |_| { false }
        ));
        // Off-grid destination, but the player already stands in danger: suppressed.
        assert!(repeat_suppressed(
            corner,
            Input::Step(Direction::West),
            |c| c == corner
        ));
    }

    /// The arithmetic on its own, now that it answers in the surface-neutral
    /// vocabulary (§11.6/#336): a drag says *what the finger did*, and nothing about
    /// what any screen makes of it. The dead band and dominant-axis rules are the
    /// board tests' above, unchanged — this pins the shape they come back in.
    #[test]
    fn a_drag_resolves_to_the_surface_neutral_vocabulary() {
        assert_eq!(drag_gesture(0.0, 0.0), Some(Gesture::Press));
        assert_eq!(
            drag_gesture(SWIPE_THRESHOLD_PX - 0.5, 0.0),
            Some(Gesture::Press)
        );
        for ((dx, dy), direction) in [
            ((-40.0, 10.0), Direction::West),
            ((40.0, -10.0), Direction::East),
            ((10.0, -40.0), Direction::North),
            ((-10.0, 40.0), Direction::South),
        ] {
            assert_eq!(drag_gesture(dx, dy), Some(Gesture::Swipe(direction)));
        }
        assert_eq!(drag_gesture(f64::NAN, 0.0), None);
    }

    /// One pump, three surfaces, and the keyboard's own precedence (§11.6/#336):
    /// the menu owns the frame before a run starts, the open help panel owns it
    /// during one, and only underneath both is there a board to walk. The menu wins
    /// even with `help_open` set, exactly as `handle_key` reads them.
    #[test]
    fn the_gesture_goes_to_whichever_surface_is_up() {
        let up = Gesture::Swipe(Direction::North);
        let left = Gesture::Swipe(Direction::West);
        assert_eq!(
            surface_command(true, false, false, false, up),
            Some(GestureCommand::Menu(MenuNav::Prev)),
        );
        assert_eq!(
            surface_command(true, false, false, true, up),
            Some(GestureCommand::Menu(MenuNav::Prev)),
            "the menu outranks the panel, as it does on the keyboard",
        );
        assert_eq!(
            surface_command(false, false, false, true, left),
            Some(GestureCommand::Help(HelpNav::PrevTab)),
        );
        // A finished run outranks the panel underneath it, as it does on the
        // keyboard: there is nothing left to navigate but the way on (#138).
        assert_eq!(
            surface_command(false, false, true, true, up),
            Some(GestureCommand::End(EndNav::Prev)),
        );
        // And the **campaign map** outranks the finished run under *it* (§14 v3/#208):
        // between facilities there is a raid that ended on the state below, so its end
        // screen must not answer a swipe aimed at the map drawn over it.
        assert_eq!(
            surface_command(false, true, true, true, up),
            Some(GestureCommand::Map(MapNav::Prev)),
        );
        assert_eq!(
            surface_command(false, false, false, false, up),
            Some(GestureCommand::Play(Input::Step(Direction::North))),
        );
        assert_eq!(
            surface_command(false, false, false, false, Gesture::Press),
            Some(GestureCommand::Play(Input::Wait)),
        );
    }

    /// **The board-only gates stay board-only** (§11.6/#223, #306). Both live in the
    /// [`GestureCommand::Play`] arm of [`GesturePump::apply`], so this is the test
    /// that keeps them there: no gesture on either modal screen resolves to `Play`,
    /// and every gesture on the board does. A shared pump must not carry the danger
    /// gate onto a title screen — where `in_visible_danger` is meaningless — nor
    /// lose it on the board.
    #[test]
    fn the_danger_gate_and_the_dead_band_never_reach_a_modal_screen() {
        let every = [
            Gesture::Press,
            Gesture::Swipe(Direction::North),
            Gesture::Swipe(Direction::East),
            Gesture::Swipe(Direction::South),
            Gesture::Swipe(Direction::West),
        ];
        for gesture in every {
            for (menu_up, map_open, run_over, help_open) in [
                (true, false, false, false),
                (false, true, false, false),
                (false, false, true, false),
                (false, false, false, true),
            ] {
                assert!(
                    !matches!(
                        surface_command(menu_up, map_open, run_over, help_open, gesture),
                        Some(GestureCommand::Play(_))
                    ),
                    "{gesture:?} on a modal must never reach the board's gates",
                );
            }
            assert!(
                matches!(
                    surface_command(false, false, false, false, gesture),
                    Some(GestureCommand::Play(_))
                ),
                "{gesture:?} on the board still goes through the gates",
            );
        }
    }

    /// **A tap on empty menu space does nothing** (§2.1/#306/#336) — never
    /// `Activate`. Starting a run is not undoable in a permadeath game, so the
    /// ambiguous zero-displacement gesture must not be what starts it; entries fire
    /// only on the arm-on-press / lift-over-the-same-control path
    /// ([`armed_fires`]), which this leaves untouched.
    #[test]
    fn a_tap_on_empty_menu_space_never_activates_an_entry() {
        assert_eq!(
            surface_command(true, false, false, false, Gesture::Press),
            None
        );
        // Nor does a swipe across the list's grain: the horizontal pair sets the
        // level-options slider (#298) and fires no control at all, so no gesture on
        // the menu can start a run.
        for direction in [Direction::East, Direction::West] {
            assert!(
                matches!(
                    surface_command(true, false, false, false, Gesture::Swipe(direction)),
                    Some(GestureCommand::Menu(MenuNav::Easier | MenuNav::Harder)),
                ),
                "a horizontal swipe on the menu may only move the slider",
            );
        }
        // The one path that does activate is unchanged, and it still needs the lift
        // to land back on the very entry the press armed.
        let entry = Control::Menu(MenuHit::Entry(MenuEntry::QuickPlay));
        assert!(armed_fires(entry, Tap::Control(entry)));
        assert!(!armed_fires(entry, Tap::Captured));
    }

    /// The menu walks by swipe with the same wrap and the same skipping of disabled
    /// entries the keys have, because both go through the one [`MenuNav`] handler
    /// (§11.6/#336) — the commands are literally equal, so the two input paths
    /// cannot disagree the first time an entry is disabled (§14 v2/v3 entries
    /// already are).
    #[test]
    fn the_menu_swipe_and_the_menu_arrow_are_the_same_command() {
        for (gesture, key) in [
            (Gesture::Swipe(Direction::North), "ArrowUp"),
            (Gesture::Swipe(Direction::South), "ArrowDown"),
            (Gesture::Swipe(Direction::West), "ArrowLeft"),
            (Gesture::Swipe(Direction::East), "ArrowRight"),
        ] {
            assert_eq!(
                surface_command(true, false, false, false, gesture),
                menu_nav_for_key(key).map(GestureCommand::Menu),
                "{gesture:?} and {key} drive the menu the same way",
            );
        }
        // And the help panel's tab swipes match its own arrows, the second consumer
        // that proves the abstraction is not a board-plus-menu special case.
        for (gesture, key) in [
            (Gesture::Swipe(Direction::West), "ArrowLeft"),
            (Gesture::Swipe(Direction::East), "ArrowRight"),
        ] {
            assert_eq!(
                surface_command(false, false, false, true, gesture),
                help_nav_for_key(key, false).map(GestureCommand::Help),
            );
        }
    }
}
