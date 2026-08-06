//! The shell half of the **options screen** (§14 v2/#513): the input it answers, what
//! each row does, and the one thing a core may not do — write a preference to storage.
//!
//! The screen itself is drawn by the core ([`intrusion_core::render_screen`] hands the
//! frame to it while [`ScreenUi::settings`] is set), so what lives here is only the
//! shell's half: opening and leaving, firing a row, and — for the two rows that are
//! *preferences* rather than session switches — persisting the result
//! ([`crate::settings`]).
//!
//! # One seam for a preference, whichever key moved it
//!
//! The theme has three ways to change: the screen's own row, the `n` key, and the
//! `theme [n]` control the campaign map still carries (§11.2/#189). All three land on
//! [`Game::toggle_theme`], which flips the flag *and* writes the record — so a
//! preference is stored because it changed, not because of where it was changed. That
//! is the whole reason the shortcut could stay when the setting moved: it is a second
//! key on one seam, not a second home.
//!
//! # Where the screen sits
//!
//! Over whatever raised it, and cleared when it is left. That is what lets the *one*
//! screen serve both entry points the ticket asks for: the title screen's `Options`
//! entry, and the help panel's `options [o]` control mid-run (which is how the §12.6
//! switches stay flippable now that their tab is gone). Nothing underneath is touched
//! — no menu surface changes, no tab is switched — so leaving restores the exact frame
//! the player opened it from.

use intrusion_core::{SeedCopy, SettingsHit, SettingsNav, SettingsRow, SettingsUi};

use crate::menu::{self, SCREEN_SETTINGS};
use crate::settings::{self, Settings};
use crate::{tiles, Game};

impl Game {
    /// Whether the options screen is the surface showing.
    pub(crate) fn settings_open(&self) -> bool {
        self.ui.settings.is_some()
    }

    /// Raise the options screen over whatever is up (§14 v2/#513) — the title screen's
    /// *Options* entry, or the help panel's `options [o]`.
    ///
    /// The marker opens on the first row every time rather than remembering where it
    /// was left: the screen is short, the row it opens on is the one most players came
    /// for, and a marker restored from a previous visit would be the one thing on a
    /// settings screen that is not what it says it is.
    pub(crate) fn open_settings(&mut self) {
        self.ui.settings = Some(SettingsUi::default());
        self.draw();
        menu::set_screen(SCREEN_SETTINGS);
    }

    /// Leave the screen, back to whatever raised it. Nothing else is touched, so the
    /// menu surface or the help tab underneath is exactly as it was left — and the copy
    /// acknowledgement goes with the screen, because it answered a press made on it.
    pub(crate) fn close_settings(&mut self) {
        self.ui.settings = None;
        self.ui.seed_copy = SeedCopy::default();
        self.draw();
        menu::set_screen(self.current_screen());
    }

    /// Apply a [`SettingsNav`] from a key — walk the rows, fire the marked one, or
    /// leave. Every arm is a view action: no [`State`](intrusion_core::State) is
    /// stepped and no turn is spent (§4.4), the omni-vision switch included (it is a
    /// sight recompute, §12.6).
    pub(crate) fn apply_settings_nav(&mut self, nav: SettingsNav) {
        let Some(ui) = self.ui.settings else {
            return;
        };
        let (debug, replay) = self.settings_gates();
        match nav {
            SettingsNav::Prev => self.select_setting(ui.prev_row(debug, replay)),
            SettingsNav::Next => self.select_setting(ui.next_row(debug, replay)),
            SettingsNav::Activate => self.fire_setting(ui.selection(debug, replay)),
            SettingsNav::Back => self.close_settings(),
            SettingsNav::ToggleTheme => {
                self.toggle_theme();
                self.draw();
            }
        }
    }

    /// Apply a [`SettingsHit`] from a tap — the pointer counterpart of
    /// [`apply_settings_nav`](Self::apply_settings_nav), through the same handlers, so
    /// a tap and a key can never do different things.
    pub(crate) fn apply_settings_hit(&mut self, hit: SettingsHit) {
        match hit {
            // A tapped row fires *and* leaves the marker on it, so the screen agrees
            // with what the finger just did — the level-options dialog's rule (#298).
            SettingsHit::Row(row) => {
                self.select_setting(row);
                self.fire_setting(row);
            }
            SettingsHit::Close => self.close_settings(),
        }
    }

    /// Move the marker and repaint.
    fn select_setting(&mut self, row: SettingsRow) {
        if let Some(ui) = self.ui.settings.as_mut() {
            ui.selected = row;
        }
        self.draw();
    }

    /// **Fire a row.** Two of them are preferences and flip-and-persist; two are the
    /// debug session's and are neither persisted nor allowed near the facility (§12.6).
    ///
    /// Each mirrors the drawn row exactly — a row this session does not have cannot be
    /// selected ([`shown_rows`](intrusion_core::shown_rows)), and the handlers refuse
    /// again anyway, so a stale marker can do nothing a player could not see.
    fn fire_setting(&mut self, row: SettingsRow) {
        match row {
            SettingsRow::Theme => self.toggle_theme(),
            SettingsRow::Renderer => self.toggle_renderer(),
            SettingsRow::Reveal => self.toggle_reveal(),
            SettingsRow::Replay => self.copy_replay(),
        }
        self.draw();
    }

    /// Flip the colour theme and **store the preference** (§11.2/#189, #513).
    ///
    /// The one seam every path to the theme lands on: this screen's row, the `n` key on
    /// any surface that forwards it, and the campaign map's `theme [n]` control. The
    /// core only ever moves a flag; the shell holds both colour tables
    /// ([`crate::palette`]) and, since #513, the record that outlives the page.
    pub(crate) fn toggle_theme(&mut self) {
        self.ui.theme = self.ui.theme.toggled();
        self.store_preferences();
    }

    /// Flip the renderer between the character grid and tiles (§11.1/#460, #513), and
    /// store it.
    ///
    /// The sheet decodes lazily, so switching *to* tiles for the first time in a
    /// session starts the load here; the frames before it lands paint as text and the
    /// shell redraws when it arrives, exactly as a `?tiles=1` boot always did. Switching
    /// back is free — the art stays decoded, since it is the shell's and not the run's.
    fn toggle_renderer(&mut self) {
        self.ui.renderer = self.ui.renderer.toggled();
        self.store_preferences();
        let _ = tiles::ensure_sheet(&self.tiles, self.handle.clone(), self.ui.renderer);
    }

    /// Flip the debug session's **omni-vision** switch (§12.6/#459) — the row that used
    /// to be the Debug tab's `omni [v]` control, key and tap alike.
    ///
    /// It mirrors the drawn row exactly: not a debug session, or the screen not up, and
    /// the press does nothing. The flip itself is the core's
    /// ([`State::toggle_reveal`](intrusion_core::State)) — a sight recompute, no turn
    /// (§4.4), and no world change, which is what makes it a view action at all, and
    /// what keeps a control behind a guessable gate safe to have (§12.6).
    ///
    /// **Never stored.** It is the one row on this screen that no record remembers: a
    /// preference that re-armed omni-vision on the next visit would outlive the session
    /// gate the whole debug channel rests on.
    fn toggle_reveal(&mut self) {
        if !self.ui.debug_mode || !self.settings_open() {
            return;
        }
        self.state.toggle_reveal();
    }

    /// The two facts the screen's row list is gated on (§12.6/#459, #333): whether this
    /// is a debug session, and whether the run has a level-seed token for the replay
    /// link to name. Read from the same places the frame was drawn from, so the marker
    /// walks exactly the rows the player can see.
    fn settings_gates(&self) -> (bool, bool) {
        (self.ui.debug_mode, self.state.level().is_some())
    }

    /// Write the current preferences to the settings record (#513).
    ///
    /// Best-effort and silent: a browser that refuses storage costs the player their
    /// preference on the *next* load and nothing on this one, and the setting they just
    /// changed is already live on screen.
    fn store_preferences(&self) {
        settings::store(Settings {
            theme: self.ui.theme,
            renderer: self.ui.renderer,
        });
    }

    /// The `data-screen` value for whichever surface is showing **under** the options
    /// screen — what [`close_settings`](Self::close_settings) restores, asked of the
    /// same fields the frame is drawn from rather than remembered from the way in.
    fn current_screen(&self) -> &'static str {
        match (self.ui.menu, self.map_open()) {
            (Some(menu), _) => menu::screen_for(menu),
            (None, true) => menu::SCREEN_MAP,
            (None, false) => menu::SCREEN_PLAY,
        }
    }
}
