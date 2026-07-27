//! Where a pointer landed, and what it is allowed to do there (§11.6/#306) — the
//! shell's pointer→meaning half, kept out of the gesture pump so the hit-test order
//! and the dead-band arithmetic live in one auditable place.
//!
//! **The rule this module pins:** *waiting is a tap on the board, well clear of the
//! bars.* A tap resolves through [`Game::screen_cell`] first and produces
//! [`Input::Wait`](intrusion_core::Input) only on a **map** row that no overlay owns
//! **and** at least a [dead band](TapGeometry::dead_band_px) away from the chrome's
//! inner edges. Inside the band — and on the chrome, and off the canvas — a tap that
//! hits no control does **nothing**: no turn, no state change.
//!
//! Before this, the tap never consulted the grid at all: any sub-threshold press was
//! `Wait`, so a near-miss on the flush-right ability block (§11.4/#267), a tap on the
//! read-only status rows, a tap on the message list you had just opened to read, or a
//! press in the letterbox margin all silently spent a turn — in a permadeath run with
//! no undo (§2.1). The fix is that a **miss costs nothing**, never that a miss becomes
//! impossible: §11.4's nine-cell slot and its fixed position are [SETTLED] and no
//! target here moves or grows.
//!
//! **Swipes are exempt.** A directional drag is unambiguous, so it may start anywhere
//! the controls decline, the band included — the band gates only the ambiguous
//! zero-displacement gesture (the tap's Wait and a press held in place). Keyboard
//! input is untouched: `w` / `5` remains the way to wait without touching the board
//! at all (§11.6), so the band never leaves a player unable to wait.

use intrusion_core::{
    ability_at, help_hit, is_help_button, is_message_button, menu_hit, message_log_rows, AbilityId,
    HelpHit, MenuHit, UiCommand, TOP_ROWS,
};

use crate::input::SWIPE_THRESHOLD_PX;
use crate::Game;

/// A chrome control a pointer can land on (§11.4/§11.6). Resolved by **identity** —
/// the core owns every one of these geometries, so a tap fires exactly the control
/// the frame drew and never one derived from the column it hit.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Control {
    /// A title-screen entry (§14/#268). The menu is modal, so while it is up this is
    /// the only control there is.
    Menu(MenuHit),
    /// A control inside the open help panel (§14 v2/#248) — a tab, or the `[x]` close
    /// that keeps the touch path from ever trapping (§11.6).
    Help(HelpHit),
    /// The near line's `[?]` help toggle (§14 v2/#139/#267).
    HelpToggle,
    /// The near line's message-log counter (§11.7).
    MessageLog,
    /// An ability's slot on the always-on bar (§11.4) — the whole nine-cell slot.
    Ability(AbilityId),
}

/// What a tap at a viewport point resolves to (§11.6/#306) — the whole vocabulary of
/// the routing rule, so every pointer press has exactly one answer.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Tap {
    /// A control owns the point: armed on the press, fired on the lift over the same
    /// control.
    Control(Control),
    /// The board, clear of the chrome and its dead band: one [`Input::Wait`].
    Wait,
    /// Nothing owns the point — chrome no control claims, the dead band, off-canvas.
    /// No turn and no state change; a gesture started here may still *swipe*, since
    /// only the ambiguous Wait is gated.
    Nothing,
    /// A modal screen (the title menu, the help panel) owns the frame and captured
    /// the press: there is no board underneath, so nothing steps and no gesture
    /// starts.
    Captured,
}

/// The screen geometry a tap is routed against: what the frame drew and the fit it
/// drew at, so the rule itself is pure arithmetic over the numbers the shell already
/// has.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct TapGeometry {
    /// The map's height in rows — the screen is `TOP_ROWS + this + BOTTOM_ROWS`
    /// (§11.4).
    pub(crate) map_h: u32,
    /// One screen row's height in **CSS pixels** at the current fit. The grid is
    /// scaled to the viewport, so this is what turns the band's pixel margin into
    /// rows.
    pub(crate) row_px: f64,
    /// How many map rows the deployed message log covers, `0` when it is folded
    /// ([`message_log_rows`]). While it is up those rows are chrome (§11.7): the list
    /// is not modal, but tapping the list you opened to read must not burn a turn.
    pub(crate) log_rows: u32,
    /// Whether a modal screen owns the whole frame (§14/#268, §14 v2/#248).
    pub(crate) modal: bool,
}

impl TapGeometry {
    /// The **dead band** in CSS pixels, measured inward from each chrome edge: the
    /// [`SWIPE_THRESHOLD_PX`] touch scale — *roughly half a fingertip*, one number for
    /// the touch feel, as the swipe threshold already is — floored at one full map row
    /// so the band is never thinner than a cell at any fit. **[START]**, expected to
    /// move once it has been played on a real phone.
    fn dead_band_px(&self) -> f64 {
        SWIPE_THRESHOLD_PX.max(self.row_px)
    }

    /// The map row index of screen row `row`, or `None` for a chrome row — the two
    /// read-only status lines above the map and the ability bar beneath it (§11.4).
    fn map_row(&self, row: u32) -> Option<u32> {
        let i = row.checked_sub(TOP_ROWS)?;
        (i < self.map_h).then_some(i)
    }

    /// Whether map row `i` is too close to the chrome to read as the board: inside the
    /// deployed message log, or within [`dead_band_px`](Self::dead_band_px) of the
    /// chrome above (the status lines, or the log's lower edge when it is up) or the
    /// ability bar below.
    ///
    /// A fit that has not happened yet — a zero or non-finite row height — makes the
    /// whole board dead, which is the safe direction: a press before the first paint
    /// can then never spend a turn on geometry nobody has seen.
    fn in_dead_band(&self, i: u32) -> bool {
        if i < self.log_rows {
            return true; // the log's own rows are chrome while it is deployed
        }
        if !(self.row_px.is_finite() && self.row_px > 0.0) {
            return true;
        }
        let band = self.dead_band_px();
        let from_above = (i - self.log_rows) as f64 * self.row_px;
        let from_below = (self.map_h - 1 - i) as f64 * self.row_px;
        from_above < band || from_below < band
    }
}

/// Route a pointer point to its [`Tap`] (§11.6/#306) — the pure rule, in the spirit of
/// [`gesture_input`](crate::input) and `repeat_suppressed`, so the whole boundary is
/// natively testable with no browser.
///
/// `cell` is the screen cell under the point ([`Game::screen_cell`]), `None` for a
/// point off the canvas. `control_at` is the chrome hit-test — the core's own
/// geometries, passed in so this rule owns the *order* without owning the state.
///
/// The order matters and is the order §11.4 draws in: a control wins over everything
/// (it is drawn on top), a modal frame has no board under it, and only then does the
/// board answer.
pub(crate) fn tap_route(
    geometry: TapGeometry,
    cell: Option<(u32, u32)>,
    control_at: impl Fn(u32, u32) -> Option<Control>,
) -> Tap {
    let Some((col, row)) = cell else {
        // Off the canvas: the listeners sit on the whole document, so the letterbox
        // margins reach here too — and they own nothing at all.
        return Tap::Nothing;
    };
    if let Some(control) = control_at(col, row) {
        return Tap::Control(control);
    }
    if geometry.modal {
        return Tap::Captured;
    }
    match geometry.map_row(row) {
        Some(i) if !geometry.in_dead_band(i) => Tap::Wait,
        // A map row inside the band, or a chrome row no control claimed: the bar's
        // bare cells left of the flush-right block (§11.4/#267), the read-only status
        // rows, the deployed log. All silent — a no-op tap says nothing, because the
        // near line is scarce and a live threat message must not be pushed out by
        // input feedback (§11.7).
        _ => Tap::Nothing,
    }
}

/// The pointer-facing half of [`Game`]: where a point lands, and how the control it
/// lands on is applied. The gesture pump ([`crate::input`]) drives both.
impl Game {
    /// The current [`TapGeometry`]: the frame's rows, the fit's row height in CSS
    /// pixels (read off the canvas box, the same rect [`screen_cell`](Game::screen_cell)
    /// scales through, so the two can never disagree), and the deployed log's height
    /// as the core reports it.
    fn tap_geometry(&self) -> TapGeometry {
        let rows = self.screen_height();
        let row_px = self.canvas.get_bounding_client_rect().height() / rows as f64;
        TapGeometry {
            map_h: self.state.layout().facility().height(),
            row_px,
            log_rows: message_log_rows(&self.state, self.ui),
            modal: self.ui.menu.is_some() || self.ui.help_open,
        }
    }

    /// The [`Control`] at screen cell `(col, row)`, or `None`. Every geometry here is
    /// the core's own — [`menu_hit`], [`help_hit`], [`is_help_button`],
    /// [`is_message_button`], [`ability_at`] — so a tap resolves to exactly the control
    /// drawn and can never hit one that is not there.
    ///
    /// The two modal screens come first and exclusively: while the title menu or the
    /// help panel is up it owns every press (§14/#268, §14 v2/#248), so the in-play
    /// chrome underneath is not reachable.
    fn control_at(&self, col: u32, row: u32) -> Option<Control> {
        let width = self.state.layout().facility().width();
        if let Some(menu) = self.ui.menu {
            return menu_hit(width, self.screen_height(), menu, col, row).map(Control::Menu);
        }
        if self.ui.help_open {
            return help_hit(width, self.screen_height(), col, row).map(Control::Help);
        }
        if is_help_button(width, col, row) {
            return Some(Control::HelpToggle);
        }
        if is_message_button(&self.state, col, row) {
            return Some(Control::MessageLog);
        }
        ability_at(&self.state, col, row).map(Control::Ability)
    }

    /// Route a viewport point through [`tap_route`] — the one call every pointer
    /// press, hold and lift asks, so the press and the lift can never disagree about
    /// what is under the finger.
    pub(crate) fn tap_at(&self, client_x: f64, client_y: f64) -> Tap {
        tap_route(
            self.tap_geometry(),
            self.screen_cell(client_x, client_y),
            |col, row| self.control_at(col, row),
        )
    }

    /// Fire a [`Control`]. The view toggles change no [`State`](intrusion_core::State)
    /// and cost no turn (§4.4); an ability entry drives the same input its hotkey does
    /// (§11.4/§11.6) — the core resolves the toggle from the ability's live state, so
    /// tapping `Run[3]` switches the sprint off (#304) exactly as pressing `r` again
    /// would, and a cooling entry refuses for free in the economy (§4.4).
    pub(crate) fn apply_control(&mut self, control: Control) {
        match control {
            Control::Menu(hit) => self.apply_menu_hit(hit),
            Control::Help(hit) => {
                self.apply_help_hit(hit);
                self.draw();
            }
            Control::HelpToggle => {
                self.apply_ui_command(UiCommand::ToggleHelp);
                self.draw();
            }
            Control::MessageLog => {
                self.apply_ui_command(UiCommand::ToggleMessageLog);
                self.draw();
            }
            Control::Ability(id) => {
                let input = self.state.ability_input(id);
                self.step_and_draw(input);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use intrusion_core::{start_level, LevelSeed, Loadout, State, BOTTOM_ROWS};

    /// The v1 board (§10.2) at a comfortable desktop fit: a 40-row map, ~19 CSS pixels
    /// a row, nothing deployed.
    fn geometry() -> TapGeometry {
        TapGeometry {
            map_h: 40,
            row_px: 19.0,
            log_rows: 0,
            modal: false,
        }
    }

    /// Route with no chrome at all — the board's own answer, isolated.
    fn bare(geometry: TapGeometry, col: u32, row: u32) -> Tap {
        tap_route(geometry, Some((col, row)), |_, _| None)
    }

    /// The rule's headline (§11.6/#306): a tap on the board well clear of both bars is
    /// a Wait, and it is the *only* thing that is. The middle of a 40-row map is far
    /// from every edge at any fit.
    #[test]
    fn a_tap_on_open_board_waits() {
        let g = geometry();
        for row in TOP_ROWS + 4..TOP_ROWS + 36 {
            assert_eq!(bare(g, 20, row), Tap::Wait, "screen row {row}");
        }
    }

    /// The chrome §11.4 says you only *read* must not step the world: the near line,
    /// the usable line, and the ability bar's bare cells (the block is flush right, so
    /// a short loadout leaves cells to its left) all yield nothing.
    #[test]
    fn a_tap_on_the_chrome_does_nothing() {
        let g = geometry();
        let bar = TOP_ROWS + g.map_h;
        for row in [0, 1, bar] {
            assert_eq!(bare(g, 20, row), Tap::Nothing, "screen row {row}");
        }
        // And nothing off the bottom of the frame either, whatever a stray row means.
        assert_eq!(bare(g, 20, bar + BOTTOM_ROWS), Tap::Nothing);
    }

    /// A point off the canvas — the letterbox margin, which the document-wide
    /// listeners reach — owns nothing. This was a wasted turn before #306.
    #[test]
    fn a_tap_off_the_canvas_does_nothing() {
        assert_eq!(tap_route(geometry(), None, |_, _| None), Tap::Nothing);
    }

    /// The dead band is **at least one full map row** at every fit — the whole point
    /// of flooring a pixel margin at a row, since the grid is scaled to the viewport
    /// and a pixels-only band would vanish on a small screen. Asserted at a tiny fit
    /// (a row far thinner than the band) and a large one (a row far thicker).
    #[test]
    fn the_dead_band_is_never_thinner_than_a_map_row() {
        for row_px in [4.0, 6.5, SWIPE_THRESHOLD_PX, 40.0, 96.0] {
            let g = TapGeometry {
                row_px,
                ..geometry()
            };
            let bar = TOP_ROWS + g.map_h;
            assert_eq!(
                bare(g, 20, TOP_ROWS),
                Tap::Nothing,
                "the first map row is dead at {row_px}px"
            );
            assert_eq!(
                bare(g, 20, bar - 1),
                Tap::Nothing,
                "the last map row is dead at {row_px}px"
            );
        }
    }

    /// A thin fit spends the band over *several* rows — the margin is in pixels, so a
    /// 6px row buries four of them — while a fit thicker than the reference scale
    /// spends exactly one. Both leave the middle of the board waitable.
    #[test]
    fn the_dead_band_widens_in_rows_as_the_fit_shrinks() {
        let thin = TapGeometry {
            row_px: 6.0,
            ..geometry()
        };
        let band_rows = (SWIPE_THRESHOLD_PX / 6.0).ceil() as u32; // 4
        for i in 0..band_rows {
            assert_eq!(bare(thin, 20, TOP_ROWS + i), Tap::Nothing, "map row {i}");
        }
        assert_eq!(
            bare(thin, 20, TOP_ROWS + band_rows),
            Tap::Wait,
            "the first row past the band waits"
        );

        let thick = TapGeometry {
            row_px: 40.0,
            ..geometry()
        };
        assert_eq!(bare(thick, 20, TOP_ROWS), Tap::Nothing, "one row of band");
        assert_eq!(bare(thick, 20, TOP_ROWS + 1), Tap::Wait, "and no more");
    }

    /// A fit that has not happened yet (a zero-height canvas at boot, or a degenerate
    /// rect) makes the whole board dead rather than waitable: a press can never spend
    /// a turn against geometry nobody has painted.
    #[test]
    fn an_unfitted_screen_waits_nowhere() {
        for row_px in [0.0, -3.0, f64::NAN, f64::INFINITY] {
            let g = TapGeometry {
                row_px,
                ..geometry()
            };
            assert_eq!(bare(g, 20, TOP_ROWS + 20), Tap::Nothing, "row_px {row_px}");
        }
    }

    /// The deployed message log is chrome while it is deployed (§11.7): its own rows
    /// are swallowed — tapping the list you opened to read must not burn a turn — and
    /// the band is then measured from *its* lower edge, not the status lines'.
    #[test]
    fn the_deployed_message_log_is_chrome_and_moves_the_band_down() {
        const LOG_ROWS: u32 = 3;
        let g = TapGeometry {
            log_rows: LOG_ROWS,
            ..geometry()
        };
        for i in 0..LOG_ROWS {
            assert_eq!(bare(g, 20, TOP_ROWS + i), Tap::Nothing, "log row {i}");
        }
        // The band below the log, in rows at this fit: the same margin the status
        // lines get, just measured from lower down the board.
        let band_rows = (SWIPE_THRESHOLD_PX / g.row_px).ceil() as u32;
        for i in LOG_ROWS..LOG_ROWS + band_rows {
            assert_eq!(
                bare(g, 20, TOP_ROWS + i),
                Tap::Nothing,
                "map row {i} is the band under the log"
            );
        }
        assert_eq!(
            bare(g, 20, TOP_ROWS + LOG_ROWS + band_rows),
            Tap::Wait,
            "and the board resumes past it"
        );
        // Folded, those same rows are ordinary board again.
        let folded = TapGeometry { log_rows: 0, ..g };
        assert_eq!(bare(folded, 20, TOP_ROWS + LOG_ROWS), Tap::Wait);
    }

    /// A control wins over every board rule — it is drawn on top, so it answers first,
    /// even on a chrome row or inside the dead band.
    #[test]
    fn a_control_owns_its_cells_wherever_they_are() {
        let g = geometry();
        let bar = TOP_ROWS + g.map_h;
        let hit = |_: u32, row: u32| match row {
            0 => Some(Control::HelpToggle),
            r if r == bar => Some(Control::Ability(AbilityId::Run)),
            _ => None,
        };
        assert_eq!(
            tap_route(g, Some((37, 0)), hit),
            Tap::Control(Control::HelpToggle)
        );
        assert_eq!(
            tap_route(g, Some((30, bar)), hit),
            Tap::Control(Control::Ability(AbilityId::Run))
        );
    }

    /// A modal screen (§14/#268, §14 v2/#248) captures the press: its own controls
    /// resolve, and everything else is `Captured` — distinct from `Nothing` because
    /// the pump must not even start a *gesture* there, or a swipe would walk a player
    /// who has not started a run.
    #[test]
    fn a_modal_screen_captures_everything_but_its_own_controls() {
        let g = TapGeometry {
            modal: true,
            ..geometry()
        };
        assert_eq!(bare(g, 20, TOP_ROWS + 20), Tap::Captured);
        assert_eq!(bare(g, 20, 0), Tap::Captured);
        assert_eq!(
            tap_route(g, Some((10, 12)), |_, _| Some(Control::Help(
                HelpHit::Close
            ))),
            Tap::Control(Control::Help(HelpHit::Close)),
            "the panel's own `[x]` still resolves — the touch path must never trap"
        );
        assert_eq!(
            tap_route(g, None, |_, _| None),
            Tap::Nothing,
            "off-canvas is off-canvas, modal or not"
        );
    }

    /// A real v1 run with a chosen loadout, for the geometry tests below.
    fn run_with(abilities: Loadout) -> State {
        let level = LevelSeed {
            abilities,
            ..LevelSeed::quick_play(7)
        };
        start_level(&level).expect("the v1 footprint always carves (§10.6)")
    }

    /// The routing geometry for a real run at a comfortable fit, nothing deployed.
    fn geometry_for(state: &State) -> TapGeometry {
        TapGeometry {
            map_h: state.layout().facility().height(),
            row_px: 19.0,
            log_rows: 0,
            modal: false,
        }
    }

    /// **The near-miss this ticket exists for** (§11.4/#267), pinned for a two- and a
    /// three-ability loadout: the bar block is flush right, so a short loadout leaves
    /// bare cells to its left. Reaching for the leftmost slot and landing one cell
    /// short — or one row *above* it, on the board — must cost nothing, while the
    /// slot's own cells still activate. The targets themselves are untouched: the fix
    /// is that a miss is free, not that a miss is impossible.
    #[test]
    fn the_ability_bar_near_miss_costs_nothing() {
        for count in [2, 3] {
            let mut abilities = Loadout::empty();
            for id in AbilityId::ALL
                .iter()
                .filter(|id| !id.is_passive())
                .take(count)
            {
                abilities = abilities.with(*id);
            }
            let state = run_with(abilities);
            let statuses = state.ability_statuses();
            assert_eq!(statuses.len(), count, "the loadout holds {count}");

            let width = state.layout().facility().width();
            let g = geometry_for(&state);
            let bar_row = TOP_ROWS + g.map_h;
            let route = |col, row| {
                tap_route(g, Some((col, row)), |c, r| {
                    ability_at(&state, c, r).map(Control::Ability)
                })
            };

            // Every ability owns a **contiguous run** of bar cells, and every run is
            // the same width: §11.4's fixed slot *is* the tap target, all of it, so a
            // short name is no harder to hit than a long one.
            let mut runs: Vec<(AbilityId, u32, u32)> = Vec::new();
            for col in 0..width {
                let Tap::Control(Control::Ability(id)) = route(col, bar_row) else {
                    continue;
                };
                assert!(
                    statuses.iter().any(|s| s.id == id),
                    "column {col} resolves to a held ability"
                );
                match runs.last_mut() {
                    Some((last, _, end)) if *last == id => *end = col + 1,
                    _ => runs.push((id, col, col + 1)),
                }
            }
            assert_eq!(runs.len(), count, "one run of cells per held ability");
            let slot = runs[0].2 - runs[0].1;
            assert!(slot > 1, "the slot is a generous target, not a single cell");
            for (id, start, end) in &runs {
                assert_eq!(end - start, slot, "{id:?}'s slot is the same width");
            }
            let leftmost = runs[0].1;
            assert!(
                leftmost > 0,
                "a short loadout leaves bare cells to the left"
            );

            // One cell short of the block: bare bar row, so nothing at all.
            assert_eq!(
                route(leftmost - 1, bar_row),
                Tap::Nothing,
                "{count} abilities: the cell left of the block is free"
            );
            // One row above it: the board's last row, which the dead band owns.
            assert_eq!(
                route(leftmost, bar_row - 1),
                Tap::Nothing,
                "{count} abilities: the board cell above the block is free"
            );
            // And the slot itself still fires.
            assert!(
                matches!(route(leftmost, bar_row), Tap::Control(Control::Ability(_))),
                "{count} abilities: the leftmost slot still activates"
            );
        }
    }

    /// The near line's corner cluster (§11.4/#267): `[?]` resolves to the help toggle,
    /// and the cells beside it on that read-only row resolve to nothing rather than a
    /// turn — the corner near-miss.
    #[test]
    fn the_help_toggle_is_hittable_and_missing_it_is_free() {
        let state = run_with(Loadout::empty());
        let width = state.layout().facility().width();
        let g = geometry_for(&state);
        let route = |col, row| {
            tap_route(g, Some((col, row)), |c, r| {
                is_help_button(width, c, r).then_some(Control::HelpToggle)
            })
        };
        let start = width - 4; // `[?]`, right-aligned with a one-cell margin
        for col in start..start + 3 {
            assert_eq!(
                route(col, 0),
                Tap::Control(Control::HelpToggle),
                "col {col}"
            );
        }
        assert_eq!(route(start - 1, 0), Tap::Nothing, "just left of the button");
        assert_eq!(route(width - 1, 0), Tap::Nothing, "the margin beyond it");
        assert_eq!(route(start, 1), Tap::Nothing, "the usable line below it");
    }
}
