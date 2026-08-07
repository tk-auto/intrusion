//! The shell half of the panel's **Options tab** (§14 v2/#513): what firing a row
//! does, and the one thing a core may not — write a preference to storage.
//!
//! The tab itself is drawn by the core ([`intrusion_core::render_help`] hands the panel
//! body to it while [`ScreenUi::help_tab`] is `Options`), so what lives here is only the
//! shell's half: walking the marker, firing a row, and — for the two rows that are
//! *preferences* rather than session switches — persisting the result
//! ([`crate::settings`]).
//!
//! # One seam for a preference, whichever key moved it
//!
//! The theme has three ways to change: the tab's own row, the `n` key, and the
//! `theme [n]` control the campaign map still carries (§11.2/#189). All three land on
//! [`Game::toggle_theme`], which flips the flag *and* writes the record — so a
//! preference is stored because it changed, not because of where it was changed. That
//! is the whole reason the shortcut could stay when the setting moved: it is a second
//! key on one seam, not a second home.
//!
//! # Where the tab sits
//!
//! Inside the help panel, which is the one modal surface a *running* game can always
//! raise — so the §12.6 switches stay flippable mid-run now that their own tab is gone
//! (#459). Before a run it is reached the same way, from the title screen's `Options`
//! entry, which raises the panel over the menu; leaving the panel puts the menu back
//! untouched.

use intrusion_core::{HelpTab, SeedCopy, SettingsRow, SettingsUi};

use crate::settings::{self, Settings};
use crate::{menu, tiles, Game};

impl Game {
    /// Whether the panel's Options tab is the surface showing — the gate every handler
    /// below mirrors, so a key can only ever do what the drawn tab offers.
    pub(crate) fn settings_open(&self) -> bool {
        self.ui.help_open && self.ui.help_tab == HelpTab::Options
    }

    /// Raise the panel on its Options tab — what the title screen's *Options* entry
    /// does (§14 v2/#513). Mid-run the same tab is one `?` and a tab-step away, which is
    /// why this is the menu's path and not a second surface.
    ///
    /// The marker opens on the first row every time rather than remembering where it was
    /// left: the list is short, the row it opens on is the one most players came for, and
    /// a marker restored from a previous visit would be the one thing on a settings tab
    /// that is not what it says it is.
    pub(crate) fn open_settings(&mut self) {
        self.ui.help_open = true;
        self.ui.help_tab = HelpTab::Options;
        self.ui.settings = SettingsUi::default();
        self.ui.seed_copy = SeedCopy::default();
        self.draw();
        menu::set_screen(menu::SCREEN_SETTINGS);
    }

    /// Move the marker one row, wrapping — the panel's `↑`/`↓` and the vertical swipes,
    /// on the one tab that has rows to walk. On any other tab there is nothing to move
    /// and this does nothing, exactly as those keys always did.
    pub(crate) fn walk_settings(&mut self, back: bool) {
        if !self.settings_open() {
            return;
        }
        let (debug, replay) = self.settings_gates();
        let ui = self.ui.settings;
        self.ui.settings.selected = if back {
            ui.prev_row(debug, replay)
        } else {
            ui.next_row(debug, replay)
        };
    }

    /// Fire the marked row — the panel's `Enter`, and the same call a tap on the row
    /// makes ([`fire_setting`](Self::fire_setting)), so the two input paths cannot
    /// diverge.
    pub(crate) fn activate_setting(&mut self) {
        if !self.settings_open() {
            return;
        }
        let (debug, replay) = self.settings_gates();
        self.fire_setting(self.ui.settings.selection(debug, replay));
    }

    /// **Fire a row.** Two of them are preferences and flip-and-persist; three are the
    /// debug session's and none of those is persisted (§12.6).
    ///
    /// Each mirrors the drawn row exactly — a row this session does not have cannot be
    /// selected ([`shown_rows`](intrusion_core::shown_rows)), and the handlers refuse
    /// again anyway, so a stale marker can do nothing a player could not see.
    ///
    /// Every arm is a **view** action: no [`State`](intrusion_core::State) is stepped
    /// and no turn is spent (§4.4). That is true of the ghost switch too, and worth
    /// saying because it is the one row that bends a rule (#507): flipping it changes
    /// what the *next* guard phase concludes, never the turn count, and the world does
    /// not move under the press.
    pub(crate) fn fire_setting(&mut self, row: SettingsRow) {
        // A tapped row leaves the marker on it too, so the panel agrees with what the
        // finger just did — the level-options dialog's rule (#298).
        self.ui.settings.selected = row;
        match row {
            SettingsRow::Theme => self.toggle_theme(),
            SettingsRow::Renderer => self.toggle_renderer(),
            SettingsRow::Reveal => self.toggle_reveal(),
            SettingsRow::Ghost => self.toggle_ghost(),
            SettingsRow::Replay => self.copy_replay(),
        }
    }

    /// Flip the colour theme and **store the preference** (§11.2/#189, #513).
    ///
    /// The one seam every path to the theme lands on: this tab's row, the `n` key on any
    /// surface that forwards it, and the campaign map's `theme [n]` control. The core
    /// only ever moves a flag; the shell holds both colour tables ([`crate::palette`])
    /// and, since #513, the record that outlives the page.
    pub(crate) fn toggle_theme(&mut self) {
        self.ui.theme = self.ui.theme.toggled();
        self.store_preferences();
    }

    /// Flip the renderer between the character grid and tiles (§11.1/#460, #513), and
    /// store it.
    ///
    /// The sheet decodes lazily, so switching *to* tiles for the first time in a session
    /// starts the load here; the frames before it lands paint as text and the shell
    /// redraws when it arrives, exactly as a `?tiles=1` boot always did. Switching back
    /// is free — the art stays decoded, since it is the shell's and not the run's.
    fn toggle_renderer(&mut self) {
        self.ui.renderer = self.ui.renderer.toggled();
        self.store_preferences();
        let _ = tiles::ensure_sheet(&self.tiles, self.handle.clone(), self.ui.renderer);
    }

    /// Flip the debug session's **omni-vision** switch (§12.6/#459) — the row that used
    /// to be the Debug tab's `omni [v]` control, key and tap alike.
    ///
    /// **Never stored.** It is the one row on this tab that no record remembers: a
    /// preference that re-armed omni-vision on the next visit would outlive the session
    /// gate the whole debug channel rests on.
    fn toggle_reveal(&mut self) {
        if !self.ui.debug_mode {
            return;
        }
        self.state.toggle_reveal();
    }

    /// Flip the debug session's **ghost** switch (§12.6/#507) — no guard detects the
    /// player while it is on.
    ///
    /// **Never stored**, for the same reason omni-vision is not, and then some: a record
    /// that re-armed a *rule-bend* on the next visit would outlive the session gate the
    /// whole channel rests on, and would do it to the facility rather than to the
    /// picture.
    ///
    /// Switching it on latches the run against export ([`State::ghosted`]) — see the
    /// core's own note. Nothing is done about the switch here beyond passing the press
    /// on: the latch is the run's, so a shell that forgot it could not weaken it.
    ///
    /// [`State::ghosted`]: intrusion_core::State::ghosted
    fn toggle_ghost(&mut self) {
        if !self.ui.debug_mode {
            return;
        }
        self.state.toggle_ghost();
    }

    /// The two facts the tab's row list is gated on (§12.6/#459, #333): whether this is a
    /// debug session, and whether the run has a level-seed token for the replay link to
    /// name. Read from the same places the frame was drawn from, so the marker walks
    /// exactly the rows the player can see.
    fn settings_gates(&self) -> (bool, bool) {
        (self.ui.debug_mode, self.state.level().is_some())
    }

    /// The `data-screen` value for whichever surface the panel is drawn **over** — what
    /// [`close_help`](Self::close_help) restores when the panel is dismissed, asked of
    /// the same fields the frame is drawn from rather than remembered from the way in.
    ///
    /// It exists because the panel can now be raised over the *menu* (#513): opening it
    /// there publishes `settings`, and leaving has to put the menu's own value back. Over
    /// a board it is the no-op it always was.
    pub(crate) fn current_screen(&self) -> &'static str {
        match (self.ui.menu, self.map_open()) {
            (Some(menu), _) => menu::screen_for(menu),
            (None, true) => menu::SCREEN_MAP,
            (None, false) => menu::SCREEN_PLAY,
        }
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
}
