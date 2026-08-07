//! On-screen HUD composition for the character grid (§11.4, §11.7).
//!
//! The world render — terrain, fog, entities, the danger overlay — lives in the
//! parent [`render`](super) module as a pure state→[`Grid`](super::Grid). This
//! module owns the *chrome* laid over and around it: the always-on ability bar,
//! the near line and its message log, the status rows, and the click hit-tests
//! that pair with them. [`render_screen`] composes the two — it draws the world
//! grid, then overlays this. Both halves stay a pure function of state
//! (§11.1/§12.1) and golden-grid testable.
//!
//! # Which way up (§11.4, §15 Q9, #267)
//!
//! The chrome is laid out for **thumb reach**: the *action* surface — the ability
//! bar — sits at the **bottom right**, where a hand holding the device already
//! rests, and the *read-only* status — the near line, the usable line, the help
//! toggle — sits at the **top**, out of the way of both the thumb and the board.
//! It used to be the other way round (ability line as a header, status lines at
//! the foot), which put the one row you tap furthest from the one finger you tap
//! it with.
//!
//! # The bar names everything, and it provably fits (§11.4, #287)
//!
//! A run holds at most [`AbilityId::MAX_HELD`] economy abilities (§8.3), so the bar
//! can carry each one's **name** outright — no key-only compaction, no deploy
//! button, no panel unfolding over the board. Fitting four names and their `[N]` /
//! `/N/` numbers across a 40-wide board (§10.2) is a tight budget, and
//! [`MAX_BAR_WIDTH`] spends it under a `const` assertion: a longer bar name, a
//! three-digit cooldown or a bigger tech grant breaks the **build**, not the frame.

use std::ops::Range;

use super::*;
use crate::ability::{max_bar_name, AbilityId, AbilityState, AbilityStatus, MAX_BAR_ENTRY};
use crate::mnemonic;
use crate::place::LevelConfig;
use crate::status::near_line;

/// The rows the screen adds **above** the map (§11.4): the near line and the
/// usable line — read-only status, kept clear of the thumb (#267). A shell
/// fitting the screen sizes for `this + facility height + BOTTOM_ROWS`.
pub const TOP_ROWS: u32 = 2;

/// The row the screen adds **beneath** the map (§11.4): the always-on ability
/// bar, right-aligned so the action surface falls under the thumb (#267). A
/// shell fits for `TOP_ROWS + facility height + this`.
pub const BOTTOM_ROWS: u32 = 1;

/// The near line's row (§11.4/§11.7): the top of the screen, where the message
/// band's colour flash reads without competing with the thumb (#267).
pub(super) const NEAR_ROW: u32 = 0;

/// The usable line's row (§11.4): directly under the near line, still above the
/// map. Never blank — with nothing adjacent to offer it teaches the innate verbs
/// instead ([`usable_row`](super::usable::usable_row), #323).
pub(super) const USABLE_ROW: u32 = 1;

/// The screen row the ability bar occupies, on a map `map_h` tall: the last row
/// of the frame. Shared by the drawing ([`render_screen`]) and the hit-test
/// ([`ability_at`]) so a tap lands on the bar that is drawn.
fn ability_row(map_h: u32) -> u32 {
    TOP_ROWS + map_h
}

/// The trailing cell of every ability-bar slot (§11.4): one cell of air after the
/// entry, separating it from the next — and, on the last slot, from the frame's
/// edge, so the strip never runs into the corner.
const BAR_GAP: u32 = 1;

/// One ability's **fixed slot** on the bar, in cells (§11.4/#287): the widest entry
/// any ability can draw ([`MAX_BAR_ENTRY`]) plus its trailing [`BAR_GAP`].
///
/// Fixed, not fitted: an entry is drawn **left-aligned inside its slot** and the
/// slot is the same width whatever state the ability is in, so a cooldown ticking
/// from `/9/` to `/10/` — or appearing at all — never shifts the ability beside it.
/// A bar whose names slide around as numbers come and go is a bar you have to
/// *read* every time; one whose names hold still is one you learn the shape of and
/// then only glance at, which is the whole point of it being always-on (§11.4).
/// Position is muscle memory, and since #359 it *is* the key as well (§11.6).
const BAR_SLOT: u32 = MAX_BAR_ENTRY as u32 + BAR_GAP;

/// The cells an entry's **slot number** takes when the row draws one (§11.6/#266):
/// `1 ` — the digit and one space before the name.
///
/// It is spent *inside* the existing slot rather than widening it, which is what keeps
/// the numbered row on the same columns as the ordinary one: the hit-test, the layout
/// and the compile-time width bound below are all untouched by it. There is room because
/// the row that numbers itself is the exchange's, whose entries draw their bare names
/// (an exchange candidate has no clock to show) — the longest is 5 cells against a
/// [`MAX_BAR_ENTRY`] of 9.
const SLOT_NUMBER_WIDTH: u32 = 2;

/// The numbered row must fit the slot it is drawn in (§11.4): the widest **bar name**
/// plus the number's own cells, since a numbered entry never carries a state notation.
/// A longer name fails the build here rather than clipping a digit off the row a player
/// is choosing from.
const _: () = assert!(
    max_bar_name() + SLOT_NUMBER_WIDTH as usize <= MAX_BAR_ENTRY,
    "a numbered ability-bar entry must fit its slot (§11.4): shorten a bar name",
);

/// The widest the ability bar can ever be, in cells (§11.4/#287): one [`BAR_SLOT`]
/// for every ability a run can hold ([`AbilityId::MAX_HELD`]).
const MAX_BAR_WIDTH: u32 = AbilityId::MAX_HELD as u32 * BAR_SLOT;

/// **The bar must fit the board it is drawn under.** The whole point of naming every
/// entry (#287) is that the held set is small enough to; this is where that stops
/// being a hope. Every input is derived — the held cap from the innate set and the
/// tech grant (§8.3), the entry width from the ability names and the catalogue's own
/// durations and cooldowns (§8.2) — so renaming an ability, pushing a cooldown past
/// 99, or granting a fourth tech fails the *build* rather than quietly truncating the
/// row on a player's screen.
const _: () = assert!(
    MAX_BAR_WIDTH <= LevelConfig::V1.width,
    "the worst-case ability bar must fit the v1 board (§10.2): shorten a bar name, \
     lower a cooldown, or lower AbilityId::MAX_TECH_HELD",
);

/// The transient **view state** a shell keeps between frames and hands to
/// [`render_screen`] (§11.4). It is deliberately *not* part of [`State`] — the
/// core stays pure game logic (§12.1), and what the player has merely chosen to
/// *look at* changes no world and costs no turn. The shell owns it, toggles it
/// from [`ui_command_for_key`](crate::input::ui_command_for_key) or a click on one
/// of the near line's controls, and passes it in.
///
/// The ability bar is **not** in here: it names every held ability on every frame
/// (§11.4/#287), so there is nothing about it left to toggle.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct ScreenUi {
    /// Whether the near line's full message list is deployed (§11.7). The near
    /// line always speaks the loudest live message; when more than one is live it
    /// also shows a counter, and this gates the expanded list of them all. The
    /// list is always the current step's live set — deployed or not, it clears on
    /// the next action (§11.7), never a scrollback.
    pub message_log_open: bool,
    /// Whether the help panel is up (§14 v2/#139/#248): a modal, full-screen
    /// reference — the tab bar, the active tab's content, the footer. A pure view
    /// toggle — no world change, no turn (§4.4) — opened by the `?` key
    /// ([`ui_command_for_key`](crate::input::ui_command_for_key)) or the near line's
    /// help button ([`is_help_button`]). While it is up it is modal: the shell
    /// routes input to it ([`help_nav_for_key`](crate::help_nav_for_key) /
    /// [`help_hit`](crate::help_hit)), so the game never steps underneath.
    pub help_open: bool,
    /// Whether the **level-start splash** is up (§11.4/§12.6/#497): the card that says
    /// what this raid is for and what is bending its rules, drawn over the frame before
    /// the first turn ([`splash`](super::splash)).
    ///
    /// A pure view flag like [`help_open`](Self::help_open) — no world change, no turn
    /// (§4.4), so no guard moves under it — raised for a **fresh facility** by
    /// [`for_fresh_run`](Self::for_fresh_run) and lowered by the first input of any
    /// kind, which the shell consumes rather than passing on. It is deliberately not a
    /// clock: nothing anywhere schedules its removal, because a card that dismissed
    /// itself would race the player's first keypress into the world (see
    /// [`splash`](super::splash)).
    ///
    /// [`Default`] is **down**, so a hand-built state, a test and the replay viewer all
    /// draw the frame they always drew; it is the *start of a run* that raises it, not
    /// the existence of one.
    pub splash_open: bool,
    /// The title screen / main menu, while it is up (§14/#268) — `None` once a run
    /// is playing. Like [`help_open`](Self::help_open) it is modal and full-screen:
    /// [`render_screen`] draws it *instead of* the game frame and the shell routes
    /// input to it ([`menu_nav_for_key`](crate::menu_nav_for_key) /
    /// [`menu_hit`](crate::menu_hit)). Its own screen lives in
    /// [`menu`](super::menu).
    pub menu: Option<MenuUi>,
    /// The **campaign map**, while it is up (§14 v3/#208) — `None` in quick play and
    /// inside a facility. Modal and full-screen exactly like [`menu`](Self::menu):
    /// [`render_map`](super::render_map) draws it *instead of* the game frame and the
    /// shell routes input to it ([`map_nav_for_key`](crate::map_nav_for_key) /
    /// [`map_hit`](crate::map_hit)).
    ///
    /// It is **not** drawn by [`render_screen`], and that is deliberate rather than an
    /// omission: the map is a view of the *campaign*, which sits above the level and is
    /// not reachable from a [`State`]. A shell holding a campaign asks for the map
    /// screen directly; one that is not in a campaign has nothing to ask with.
    pub map: Option<MapUi>,
    /// Where the marker rests on the panel's **Options** tab (§14 v2/#513). Ignored on
    /// every other tab and while the panel is closed; the [`Default`] is the first
    /// setting, so the tab always opens on the row most players came for.
    ///
    /// A plain field rather than an `Option`, because "is the tab showing" is already
    /// [`help_tab`](Self::help_tab)'s to answer — exactly as [`help_tab`](Self::help_tab)
    /// itself is a plain field beside [`help_open`](Self::help_open).
    pub settings: SettingsUi,
    /// Which help tab is showing while [`help_open`](Self::help_open) (§14 v2/#248).
    /// Ignored when the panel is closed; the [`Default`] is the leftmost tab, so the
    /// panel opens on Level info. The shell cycles it from
    /// [`help_nav_for_key`](crate::help_nav_for_key) or a tab tap ([`help_hit`]).
    pub help_tab: HelpTab,
    /// Which colour table the shell paints from (§11.2/#189). The core carries the
    /// *flag* and never the colours: it says which of presentation's two columns is
    /// live, and the shell owns both. Like every other field here it is a pure view
    /// choice — no world change, no turn (§4.4) — flipped by
    /// [`UiCommand::ToggleTheme`](crate::UiCommand::ToggleTheme), and from the title
    /// screen or the help panel, the option's home until v2's options screen lands.
    ///
    /// In-session only for now: nothing persists it, so a reload comes back on the
    /// [`Default`] dark theme.
    pub theme: Theme,
    /// Which cell primitive the shell paints the board with (§11.1/#460/#513). The
    /// core carries the *flag* and never a sprite: it says which of presentation's two
    /// implementations is live, and the shell owns both — the same split
    /// [`theme`](Self::theme) makes for colour, and what keeps §11.1's "the core must
    /// not know which is in use" true while the options screen still draws a row for
    /// it.
    ///
    /// A pure view choice like the rest — no world change, no turn (§4.4) — and, like
    /// the theme, a **preference**: the shell restores it from its settings record at
    /// boot and writes it back when the row is fired (#513), so it outlives the run.
    pub renderer: Renderer,
    /// Whether the last attempt to copy this run's level-seed token reached the
    /// clipboard (§13.1/#353) — the acknowledgement the Level info tab prints under
    /// the token. The shell performs the write and records the outcome here; the core
    /// only decides what that outcome *looks like*, so [`render_help`] keeps writing
    /// no state and the copy still costs no turn (§4.4).
    pub seed_copy: SeedCopy,
    /// Whether this is a **debug session** (§12.6/#459) — the one thing that puts the
    /// help panel's [`HelpTab::Debug`] tab on the bar, with the omni-vision switch and
    /// the replay export (§12.4/§13.1/#411) on it.
    ///
    /// It used to take a second flag beside this one, for whether the *build* had an
    /// input recorder behind that export. Every build has one since #478, so the two
    /// always agreed and one of them was answering a question nobody was asking any
    /// more: the live question is not "was this build made for previewing?" but "is
    /// this a debug session?", which is this.
    ///
    /// It is the shell's word, decided once at boot and never by the run: a build that
    /// stamped it in (the artifact preview channel) or a page opened with the
    /// activation parameter. Deliberately **not** part of [`State`](crate::State) or of
    /// the level-seed token — nothing a player can be handed may switch it on, which is
    /// what keeps a shared link a shared *level* (§13.1). Default `false`, so the
    /// public deploy, the sim and every test draw the three-tab panel.
    pub debug_mode: bool,
    /// The end screen's view state (§14 v2/#138) — the run's mode, which gates the
    /// exits it offers, and the exit the marker rests on.
    ///
    /// There is no flag for *whether* the screen is up: it is up exactly when the run
    /// has ended ([`State::verdict`](crate::State::verdict)), so an in-progress run
    /// draws neither verdict and a finished one always draws its own. That is the one
    /// thing about this screen a shell cannot get wrong.
    pub end: EndUi,
    /// Which input vocabulary to teach the innate verbs in (§11.6/#323): the
    /// wording of the usable line's floor ([`usable`](super::usable)), and nothing else.
    /// The shell answers only *is this a touch session?*; the core keeps the words
    /// and the layout, so the hint stays inside the golden tests (§11.2/§12.1).
    pub modality: InputModality,
}

impl ScreenUi {
    /// The view state a **fresh facility** opens with (#473): a clean default,
    /// except for what the *player* is and what the *build* is. A run is a new
    /// world and the screen should not carry the last one's — no menu still up,
    /// no help panel left open, no stale end screen — but three of these fields
    /// are not about the run at all:
    ///
    /// - [`modality`](Self::modality) is a fact about the player's hands
    ///   (§11.6/#323), so a fresh facility must not send a touch player back to
    ///   reading keys;
    /// - [`theme`](Self::theme) and [`renderer`](Self::renderer) are facts about the
    ///   person looking at the screen (§11.2/§11.1, #189/#513) — the settings they
    ///   chose are the ones the run must open in, and they outlive the page as well as
    ///   the run;
    /// - [`debug_mode`](Self::debug_mode) is a fact about the **session**
    ///   (§12.6/#459) — a page opened as a debug session stays one, whatever run it
    ///   is playing, and nothing a run can do turns it on or off.
    ///
    /// It lives here, next to the fields, rather than being spelled out at the
    /// shell's reset: that is how the theme came to be dropped in the first place —
    /// the second theme landed (#189) and the shell's hand-written carry list was
    /// simply not extended, so the choice made on the title screen died on the way
    /// into the run. One named seam means the next field added answers the question
    /// once, where the field is declared, and a test can name what survives.
    ///
    /// It is also where the **level-start splash** is raised (#497): a fresh facility
    /// opens on its own card, and this is the one seam every fresh facility crosses —
    /// the title screen's *Quick play*, a shared link, an end screen's *retry*, a
    /// campaign raid. The resume path uses this as its base and lowers the flag again,
    /// because a run picked up mid-raid is past its first turn.
    #[must_use]
    pub fn for_fresh_run(self) -> Self {
        Self {
            modality: self.modality,
            theme: self.theme,
            renderer: self.renderer,
            debug_mode: self.debug_mode,
            splash_open: true,
            ..Self::default()
        }
    }
}

/// The input vocabulary the player is actually using (§11.6/#323) — the one thing
/// a shell knows about the player's hands that the core cannot derive from state.
/// It decides how the usable line's floor words the two innate verbs
/// ([`usable`](super::usable)) and nothing else: keys and gestures are both live at all
/// times, whatever this says.
///
/// [`Keys`](Self::Keys) is the [`Default`], so a shell that never sets it — the
/// sim, a test, a native harness — teaches the keyboard, which is the modality
/// every one of them drives.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum InputModality {
    /// A keyboard session: arrows to move, `5` / `w` to wait (§11.6).
    #[default]
    Keys,
    /// A touch session: swipe to move, tap to wait (§11.6's touch model).
    Touch,
}

/// The help toggle on the near line (§14 v2/#139/#267): a `[?]` at the top-right
/// corner, three cells wide. Opens the help panel, and — since the panel is modal
/// and carries its own `[x]` — the pair is always escapable, the touch path that
/// never traps (§11.6).
const HELP_BUTTON: &str = "[?]";
pub(super) const HELP_BUTTON_LEN: u32 = 3;

/// How many of the near line's cells are **not** the message's, worst case
/// (§11.4/§11.7), counted out: the `[?]`, its cell of air, the cell of air before the
/// deploy control, and the control itself.
///
/// There is no margin beyond the deploy control — it sits flush against the row's last
/// column. A blank cell out there bought nothing: the near line is a solid category
/// band edge to edge (§11.4), so the "margin" was band, not air, and the only thing it
/// separated the control from was the end of the screen.
///
/// `width` minus this is the message's true **capacity in glyphs** — not a column
/// index, which is the off-by-one this constant exists to stop anyone making again. It
/// is derived from the controls rather than written down, so tightening one gives the
/// cells back to the words on its own.
pub(super) const NEAR_LINE_CONTROL_CELLS: u32 =
    HELP_BUTTON_LEN + 1 + 1 + super::message_log::DEPLOY_LEN;

/// How far past a near-line control the row **holds its band back** (§11.4/#502), in
/// cells of air on the message's side of it: the control's own cells always carry the
/// screen background, and this says whether the blank cell beside it does too.
///
/// **None — the band meets the button edge to edge.** Both widths were built and
/// compared side by side (the ticket's variants A and B, one artifact each), and the
/// control's own cells are enough: what made the `[?]` unreadable was the tan sitting
/// *on* the tint, and lifting the tan off it is the whole of the fix. Taking the air
/// cell as well reads as a gap punched in the row, and the band's job is the row.
///
/// Setting it to `1` is the whole of variant B, which is how the comparison was run —
/// and it would cost nothing either way, since the air cells are already outside the
/// message's budget ([`NEAR_LINE_CONTROL_CELLS`]). The choice is legibility against
/// the band's continuity, not capacity; see `docs/render-reference.md` §5.
const HELD_BACK_AIR: u32 = 0;

/// The near line's **controls** (§11.4/§11.7/#267), laid out once — where each one
/// sits, and the span of row the words are therefore left with.
///
/// One layout, four readers: the drawing of each button, both hit-tests, and the
/// message's own width budget. That is the point of computing it in one place. The old
/// code derived each start separately and let the text budget follow whichever control
/// happened to be leftmost, which is exactly the arrangement where adding a control
/// silently runs the words underneath it.
pub(super) struct NearLineControls {
    /// Where the `[?]` starts — the screen's **top-left corner**. Always drawn: help is
    /// never unavailable (§11.6).
    pub(super) help_start: u32,
    /// The message-log deploy control when there is anything to deploy (§11.7): its
    /// label and its start column, flush against the row's last column. `None` leaves
    /// the row's right end to the message.
    pub(super) log: Option<(String, u32)>,
    /// The first column the near line's message may use — clear of the `[?]` and a cell
    /// of air after it.
    pub(super) text_start: u32,
    /// **The column the message must stop before.** A cell short of the deploy control
    /// when it is up, the frame's margin otherwise — so a long message can never run
    /// under a control, and a *new* control cannot make it start doing so.
    pub(super) text_max: u32,
}

impl NearLineControls {
    /// How many glyphs of message the row can actually hold — the span between the
    /// controls, in cells. A *count*, never a column: the near line's words run
    /// `text_start .. text_max`.
    pub(super) fn capacity(&self) -> u32 {
        self.text_max.saturating_sub(self.text_start)
    }

    /// The spans of the row the band is **not** painted across (§11.4/#502) — the `[?]`
    /// and, when it is up, the deploy control, each with its [`HELD_BACK_AIR`] beside it.
    /// Those cells keep the screen's own background, so the one static System tan every
    /// HUD control wears (#420) is read against the same backdrop as every other control
    /// on the screen instead of against a quiet tint of the facility's standing mood.
    ///
    /// **Read off the layout, like everything else about this row.** The holdback is the
    /// third thing that has to agree with the drawing and the hit-tests, and a `[?]` whose
    /// held-back span and whose hit-test disagree is the same class of bug as one whose
    /// band and words disagree (§11.4 **[SETTLED]**). Derived from the control positions
    /// rather than written down beside them, so moving a control moves its holdback.
    ///
    /// This is **paint, not layout**: the spans cover cells the message was never allowed
    /// ([`capacity`](Self::capacity) is untouched), so holding the band back can never
    /// cost the row a glyph.
    pub(super) fn held_back(&self) -> impl Iterator<Item = Range<u32>> {
        let help = self.help_start..self.help_start + HELP_BUTTON_LEN + HELD_BACK_AIR;
        let log = self.log.as_ref().map(|(_, start)| {
            start.saturating_sub(HELD_BACK_AIR)..start + super::message_log::DEPLOY_LEN
        });
        std::iter::once(help).chain(log)
    }
}

/// Lay the near line's controls out on a screen `width` wide (§11.4/§11.7).
///
/// **The `[?]` owns the top-left corner and the deploy control the top-right**, with
/// the message between them. The two controls do different jobs and belong at
/// different ends: `[?]` is the fixed landmark a lost player reaches for, and putting
/// it at column 0 makes it the one control whose position never depends on anything —
/// not the screen width, not what else is up. The deploy control is the one that comes
/// and goes with what there is to read, so it takes the end that is allowed to change.
/// Splitting them also stops the pair from eating one contiguous bite out of the
/// row's right-hand side, which is where a long message ran out of room.
///
/// `log_open` picks the deploy glyph, never the width — the control is
/// [`DEPLOY_LEN`](super::message_log::DEPLOY_LEN) cells whatever it says — so a
/// hit-test may ask with either and land on the button the frame drew.
pub(super) fn near_line_controls(state: &State, width: u32, log_open: bool) -> NearLineControls {
    let label = super::message_log::deploy_label(state, log_open);
    debug_assert!(
        label
            .as_ref()
            .is_none_or(|l| l.chars().count() as u32 == super::message_log::DEPLOY_LEN),
        "the deploy control is a fixed three cells (§11.7): the layout budgets for it",
    );
    let log_start = width.saturating_sub(super::message_log::DEPLOY_LEN);

    let controls = NearLineControls {
        help_start: 0,
        // A cell of air after the `[?]`, so the words never touch the control.
        text_start: HELP_BUTTON_LEN + 1,
        // A cell of air before the deploy control so the words never touch it; with no
        // control up the words may run to the row's last column.
        text_max: if label.is_some() {
            log_start.saturating_sub(1)
        } else {
            width
        },
        log: label.map(|label| (label, log_start)),
    };
    debug_assert!(
        controls.log.is_none()
            || controls.capacity() == width.saturating_sub(NEAR_LINE_CONTROL_CELLS),
        "with both controls up the message's capacity is the row less exactly \
         NEAR_LINE_CONTROL_CELLS (§11.4)",
    );
    controls
}

/// Whether screen cell `(x, y)` is the near line's help button (§14 v2/#139) — the
/// `[?]` toggle in the screen's **top-left** corner. A shell maps a click to a screen
/// cell and asks this; a hit flips [`ScreenUi::help_open`] instead of stepping. Kept
/// beside the drawing so the button a tap lands on is exactly the one drawn.
pub fn is_help_button(x: u32, y: u32) -> bool {
    y == NEAR_ROW && x < HELP_BUTTON_LEN
}

/// Render the full §11.4 **screen**: the two status lines, the map ([`render`]),
/// and the always-on ability bar beneath it — `TOP_ROWS + map height +
/// BOTTOM_ROWS` rows, same width, one [`Grid`], so a whole frame is still a pure
/// function of `(state, ui)` that prints as text (§11.1) and golden-testable to
/// the last row.
///
/// - **Near line** (row `0`): the highest-priority message of the last action, or
///   the ambient floor ([`near_line`], §11.7) — a solid band in the message's
///   category with the words in Neutral on top — plus the right-aligned help
///   toggle ([`is_help_button`]) and, when more than one message is live, the
///   message-log counter beside it.
/// - **Usable line** (row `1`): the adjacent bump affordances
///   ([`State::affordances`]), each in its own category, no band — or, when there
///   are none, the move/wait floor (§11.6/#323). Aimed where each arrow points
///   — west flush left, north/south centred, east flush right (#384).
/// - **Ability bar** (row `height-1`): the always-on named readout — every held
///   ability's bar name coloured by state, its active/cooling number tucked against
///   it ([`AbilityStatus::bar_entry`]) — **right-aligned** into the bottom-right
///   corner. This is the permanent home for ability state (§11.4/§15 Q9): one row,
///   glanceable, never covering the board, and under the thumb that taps it (#267).
///
/// # Named, always, with nothing to deploy (§11.4, §15 Q9, #287)
///
/// The bar used to compress each ability to a bare letter and hide the names
/// behind a deploy button that unfolded a panel over the board. With the held set
/// capped at [`AbilityId::MAX_HELD`] (§8.3) the compression bought nothing worth its
/// cost: the names fit, so they are simply always there and the button and panel are
/// gone. Two experiments preceded it and both lost — showing the list only *while
/// waiting* buried the 360° guard-sense the wait exists to reveal (§9.1), and a
/// left-aligned header strip put the tap target furthest from the thumb (#267).
///
/// The bar draws the run's **real** ability state ([`State::ability_statuses`]); a
/// click on an entry resolves to the ability under it ([`ability_at`]) and activates
/// it exactly as that slot's digit would. Since #359 the bar is the keys' **source**
/// rather than their projection — `1`–`4` fire its first through fourth entries
/// (§11.6) — so the order this row draws in is load-bearing, and the help panel's
/// Abilities tab is where a player reads each pairing off.
pub fn render_screen(state: &State, ui: ScreenUi) -> Grid {
    let facility = state.layout().facility();
    let width = facility.width();
    let height = TOP_ROWS + facility.height() + BOTTOM_ROWS;

    // **The help panel comes before the title screen** (#513). It is the one surface
    // that can be raised *over* another: the menu's `Options` entry opens the panel on
    // its Options tab, and leaving the panel puts the menu back untouched. The order
    // matters only for that case — nothing else opens the panel over a menu — but it is
    // what makes settings reachable before a run as well as during one.
    if ui.help_open {
        return super::help::render_help(
            width,
            height,
            ui,
            super::help::PanelRun {
                level: state.level(),
                modifiers: state.modifiers(),
                intel: state.intel_total(),
                caches: state.cache_total(),
                alert: &state.alert_readout(),
                bar: state.bar_statuses().iter().map(|s| s.id).collect(),
                debug: state.debug(),
                ghosted: state.ghosted(),
            },
        );
    }

    // The title screen (§14/#268) comes next: before a run starts there is
    // nothing of the game to show, so the menu takes the whole screen — sized to the
    // board behind it, so starting a run changes what is drawn and never the fit.
    if let Some(menu) = ui.menu {
        return super::menu::render_menu(width, height, menu);
    }

    // The bar's own row (§11.4/#266): the held set, or the exchange's four candidates
    // while a crate is offering. One derivation for what is drawn and what the keys
    // fire, so the two can never name different abilities.
    let statuses = state.bar_statuses();

    let map = render(state);

    // The near line (§11.4/§11.7): the loudest live message as a category band —
    // or the ambient floor when nothing is live — plus the right-aligned help
    // toggle and, when the log has more to show, its deploy control beside it.
    let top = near_line(state);
    // **An ambient band paints the quiet fill; a message band paints the full one**
    // (§11.4/§11.5, #420). The row's colour then separates the facility's standing mood
    // — a permanent tint the eye stops reading as news — from something that just
    // happened, which flashes. It also keeps a standing Danger row from spending the
    // §11.5 overlay's own fill: that shade means *a threat has you right now*, and a
    // permanent row wearing it dilutes the one place it is true.
    let band_fill = if top.is_ambient() {
        Fill::Quiet
    } else {
        Fill::Full
    };
    // One layout for both controls: where each goes, and the span of row that leaves
    // the message. The budget comes *from* the layout, so a row can never run under a
    // control that is up (§11.4).
    let controls = near_line_controls(state, width, ui.message_log_open);
    let mut near = status_row(
        width,
        controls.text_start,
        controls.text_max,
        &[(top.text, Category::Neutral)],
        Some((top.category, band_fill)),
    );
    // **The band stops at the controls** (§11.4/#502). It runs edge to edge across every
    // cell the words can use — including the ones they do not fill — but under the `[?]`
    // and the deploy control the row keeps the screen's own background, so the static
    // System tan those two wear is read against the backdrop every other control on the
    // screen is read against rather than against a tint of the standing mood.
    for span in controls.held_back() {
        hold_back_band(&mut near, span);
    }
    if let Some((label, start)) = &controls.log {
        super::message_log::draw_message_button(&mut near, width, *start, label);
    }
    // The help toggle, in the one static System colour every other HUD control wears
    // (#420). It is drawn last so it owns column 0 whatever the row said.
    draw_help_button(&mut near, width, controls.help_start);

    // One grid, top to bottom: the two status lines, the map, the ability bar.
    let mut cells = near;
    debug_assert_eq!(
        cells.len() as u32,
        USABLE_ROW * width,
        "the usable line follows the near line"
    );
    // The usable line owns its own row end to end (§11.4): the affordances aimed
    // where they point (#384) — plus, since #451, the one that points nowhere,
    // because it is about the cell underfoot and its press is a wait — or, with
    // nothing to act on, the innate-verb floor in the modality the shell says the
    // player is using (#323). A floor, never a competitor: one usable and the
    // affordances have the row back.
    // …unless a crate is offering (§8.3/#266), in which case the row says how to answer
    // the exchange instead. Not an extra entry beside the affordances but *instead* of
    // them: while an offer is open no press does anything else, so a row still naming a
    // bump would be promising what the next press will not deliver.
    cells.extend(match state.exchange() {
        Some(_) => super::usable::exchange_row(width, ui.modality),
        None => super::usable::usable_row(width, &state.affordances(), ui.modality),
    });
    cells.extend(map.cells);
    // The row is **numbered while a crate is offering** (#266): an exchange is picked
    // from rather than glanced at, and a player choosing what to give up should not have
    // to count slots to find the digit. It fits because a candidate draws its bare name
    // — no clock, no marker — which leaves the width the numbers need.
    cells.extend(ability_bar(width, &statuses, state.exchange().is_some()));

    let mut screen = Grid {
        width,
        height,
        cells,
    };
    // The deployed message log is laid over the finished frame (§11.7/#300), not over
    // the map alone: it hangs from the near line's band across the **whole** row,
    // covering the usable line and as much board as it needs. Nothing else overlays
    // the frame any more — the ability bar names its whole set on its own row (#287),
    // so the board stays whole while you are not reading the log. The log owns the
    // question of what it holds and whether it holds anything at all.
    if ui.message_log_open {
        super::message_log::overlay_message_log(&mut screen, state);
    }
    // The level-start splash (§11.4/§12.6/#497), laid over the finished frame: until it
    // is dismissed there is nothing underneath it the player may act on, so it goes over
    // the board and the chrome alike. It draws about the **run**, not about progress
    // through it — the gate the exit will hold the player to and the console count it
    // holds them to it with, plus the §12.6 rules in force — so a card and the Level info
    // tab can only ever say the same thing about the same facility.
    if ui.splash_open {
        super::splash::overlay_splash(
            &mut screen,
            state.modifiers(),
            state.intel_total(),
            state.cache_total(),
            ui.modality,
        );
    }
    // The verdict is the **last** thing laid on (§14 v2/#138): a finished run has one
    // thing left to say, and nothing — not the log, not the bar, not a card from a turn
    // that never happened — may sit on top of it. It is an overlay rather than a scene on purpose: the board that reads above
    // and below it is most of how a capture gets traced (§2.2).
    if let Some(verdict) = state.verdict() {
        super::verdict::overlay_verdict(&mut screen, verdict, ui.end, state.level());
    }
    screen
}

/// Lay the ability bar out (§11.4/#267/#287): the start column of each ability's
/// **fixed slot**, in draw order, as `(status index, slot start col)`. Slots are
/// [`BAR_SLOT`] wide regardless of what the ability is doing, laid end to end and
/// **right-aligned** so the action surface hugs the bottom-right corner.
///
/// The geometry depends only on *how many* abilities the run holds — never on their
/// state — and a loadout is fixed for the whole run (§8.3), so a column here is a
/// column for the entire run. That is the property worth having: a number appearing
/// or growing by a digit moves nothing.
///
/// A legal run loadout always fits: [`MAX_BAR_WIDTH`] says so at compile time. The
/// truncation below is for the oversized hand-built states that outrun that bound
/// (a `Loadout::full` test board, a narrow board): the **deck's tail** is dropped,
/// last slot first, and what remains stays flush right. Shared by [`ability_bar`]
/// (drawing) and [`ability_at`] (hit-testing) so a click can never land on a slot
/// the row did not draw.
fn ability_line_layout(width: u32, statuses: &[AbilityStatus]) -> Vec<(usize, u32)> {
    let mut shown = statuses.len();
    while shown > 0 && shown as u32 * BAR_SLOT > width {
        shown -= 1;
    }
    let x0 = width - shown as u32 * BAR_SLOT;
    (0..shown).map(|i| (i, x0 + i as u32 * BAR_SLOT)).collect()
}

/// The always-on ability bar (§11.4/#267/#287): the frame's last row, carrying every
/// held ability by name with its state notation tucked against it
/// ([`AbilityStatus::bar_entry`]), each **left-aligned in a fixed slot**
/// ([`ability_line_layout`]) and coloured by state ([`bar_category`]), the whole
/// strip right-aligned into the bottom-right corner. Slots are a fixed width so a
/// number appearing or ticking never shifts a neighbour. No band — the bar reads as
/// a quiet HUD strip, not a message.
///
/// One cell of each entry is the **mnemonic** (§11.6/#360): the letter that entry
/// answers to, lifted to [`Category::Neutral`] — the ink colour, the brightest thing
/// the palette has — so it stands out of the name around it as *the key you press*.
/// It recolours a cell the entry had already drawn, so it costs no width and §11.4's
/// compile-time slot arithmetic is untouched.
///
/// **An entry you cannot use keeps its letter dim.** A [`Category::Ground`] entry —
/// exhausted or unusable, the two states §11.4 draws as "plainly not an option now" —
/// recedes whole: brightening one cell of it would advertise a key for something that
/// is not on offer, and the eye would be pulled to exactly the entry it should skip.
/// The letter still *works* there (it resolves like any other, and refuses for free in
/// the economy, §4.4); it simply does not shout.
fn ability_bar(width: u32, statuses: &[AbilityStatus], numbered: bool) -> Vec<GlyphCell> {
    let blank = GlyphCell::blank();
    let mut cells = vec![blank; width as usize];

    let put = |cells: &mut [GlyphCell], at: u32, text: &str, category: Category| {
        for (i, glyph) in text.chars().enumerate() {
            let x = at + i as u32;
            if x < width {
                cells[x as usize] = GlyphCell {
                    glyph,
                    fg: category,
                    ..blank
                };
            }
        }
    };

    let indent = if numbered { SLOT_NUMBER_WIDTH } else { 0 };
    let layout = ability_line_layout(width, statuses);
    let mnemonics = mnemonic::claim(&drawn_bar_names(&layout, statuses));
    for ((i, start), letter) in layout.into_iter().zip(mnemonics) {
        let status = &statuses[i];
        let category = bar_category(status.state);
        // The slot's **number**, when the row is being picked from rather than read
        // (§11.6/#266): `1 Camo`, in the key colour the mnemonic mark wears, so the two
        // keys that fire an entry read as the one kind of thing. Off on the ordinary
        // bar, where position is muscle memory and the width belongs to the clocks.
        if numbered {
            put(&mut cells, start, &format!("{} ", i + 1), Category::Neutral);
        }
        put(&mut cells, start + indent, &status.bar_entry(), category);
        // The mark is the letter's own colour — no band behind it, nothing added
        // beside it — so the bar stays the quiet strip §11.4 asks for and the entry
        // reads as one word with one letter picked out of it.
        if category == Category::Ground {
            continue; // an entry you cannot use does not advertise its key
        }
        if let Some(x) = letter
            .map(|l| start + indent + l as u32)
            .filter(|x| *x < width)
        {
            cells[x as usize].fg = Category::Neutral;
        }
    }
    cells
}

/// The bar names of the **drawn** slots, in slot order — what the mnemonic claim
/// (§11.6/#360) runs over.
///
/// Drawn rather than held, so a letter is never claimed by an entry the row
/// truncated away: the mark *is* the binding's only announcement, so a key nobody
/// can see must not exist.
fn drawn_bar_names(layout: &[(usize, u32)], statuses: &[AbilityStatus]) -> Vec<&'static str> {
    layout
        .iter()
        .map(|&(i, _)| statuses[i].id.bar_name())
        .collect()
}

/// The **mnemonic letter** of bar slot `slot`, or `None` where that slot has none
/// (§11.6/#360) — the secondary key beside the slot's digit.
///
/// Derived from the run's own drawn bar, so it is the letter the row is highlighting
/// at this very moment ([`ability_bar`]) and the letter the help panel prints — one
/// derivation, three readers, no chance of the screen naming a key that does not fire.
pub fn ability_mnemonic(state: &State, slot: usize) -> Option<char> {
    let statuses = state.bar_statuses();
    let layout = ability_line_layout(state.layout().facility().width(), &statuses);
    let names = drawn_bar_names(&layout, &statuses);
    let index = (*mnemonic::claim(&names).get(slot)?)?;
    Some(mnemonic::letter_at(names[slot], index))
}

/// The bar slot a **mnemonic letter** fires (§11.6/#360), or `None` for a character
/// no entry of this run claimed — the letter counterpart of
/// [`ability_slot_for_code`](crate::ability_slot_for_code).
///
/// Case is folded, so a stray Shift (or Caps Lock) still fires the ability rather
/// than silently costing the turn it was meant to spend. It lands on a *slot*, like
/// the digit and like a tap, so all three meet at [`ability_in_slot`] and none of
/// them can name a different ability from the one under the highlight.
pub fn ability_slot_for_letter(state: &State, key: &str) -> Option<usize> {
    let mut chars = key.chars();
    let ch = match (chars.next(), chars.next()) {
        (Some(c), None) => c.to_ascii_lowercase(),
        _ => return None, // named keys ("Tab", "ArrowUp") are never a mnemonic
    };
    let statuses = state.bar_statuses();
    let layout = ability_line_layout(state.layout().facility().width(), &statuses);
    let names = drawn_bar_names(&layout, &statuses);
    mnemonic::claim(&names)
        .into_iter()
        .enumerate()
        .find(|&(slot, index)| index.is_some_and(|i| mnemonic::letter_at(names[slot], i) == ch))
        .map(|(slot, _)| slot)
}

/// Hold the near line's band back across `span` (§11.4/#502): those cells drop their
/// background category and carry the screen's own backdrop instead — black in the dark
/// theme, paper in the light one. The core says *no band here* and the shell paints
/// whichever of presentation's two columns is live (§11.2/#189); nothing here names a
/// colour.
///
/// The cell's [`Fill`] is left alone deliberately: it is meaningless — and ignored —
/// when `bg` is `None` ([`GlyphCell::fill`]), so clearing it would only invite the
/// reader to wonder which of the two shades a cell with no background paints in.
fn hold_back_band(row: &mut [GlyphCell], span: Range<u32>) {
    for x in span {
        if let Some(cell) = row.get_mut(x as usize) {
            cell.bg = None;
        }
    }
}

/// Draw the help toggle over the already-built near line `row` (§14 v2/#139/#267):
/// [`HELP_BUTTON`] in [`Category::System`] — the HUD-control colour the deploy button
/// and the panel's `[x]` wear — on the screen's own background, which the row holds its
/// band back from underneath it ([`hold_back_band`], #502).
///
/// **It used to be tinted by the alert rung** (#375), on the argument that the button is
/// the ladder's always-visible half: the panel behind it is where the standing alert
/// state can be read, so the control changed colour to say there was something new
/// there. That argument transferred wholesale to the near line the moment the row began
/// saying the condition in words *and* in its band (#420/#421) — and a red `[?]` sitting
/// on a red band at condition 3 is a second, quieter statement of what the row already
/// says. The mechanism goes; the job it was doing is done better one row down.
///
/// The alert stays readable in the help panel's ALERT section throughout (§7.3/#375), so
/// what went is a redundant channel and not the only one.
///
/// **It is not handed a band at all** (#502), rather than being handed one and told not
/// to use it: the button's whole problem was the tan sitting on the row's tint, and a
/// drawer that cannot be given a background cannot drift back into painting one.
fn draw_help_button(row: &mut [GlyphCell], width: u32, start: u32) {
    for (i, glyph) in HELP_BUTTON.chars().enumerate() {
        let x = start + i as u32;
        if x < width {
            row[x as usize] = GlyphCell {
                glyph,
                fg: Category::System,
                ..GlyphCell::blank()
            };
        }
    }
}

/// The ability in bar **slot** `slot`, counting from `0` at the row's leftmost
/// *drawn* entry, or `None` for a slot this run does not fill (§11.4/§11.6, #359).
///
/// **The one place a slot becomes an ability.** The keyboard arrives here with a
/// digit ([`ability_slot_for_code`](crate::ability_slot_for_code)) and the pointer
/// with a column ([`ability_at`]), so `1` and the entry under the thumb can never
/// name different abilities — the divergence §11.4 pins against, now that the keys
/// *are* the bar's positions rather than a projection of identity.
///
/// Counted against what is **drawn**, not against the catalogue: the row is flush
/// right (#267) and shorter loadouts sit further in, so slot `0` is the leftmost
/// entry on screen and never a gap where the first catalogue row would have gone. A
/// slot the row truncated away (an oversized hand-built state — see
/// [`ability_line_layout`]) is `None` too, so no key fires an entry nobody can see.
pub fn ability_in_slot(state: &State, slot: usize) -> Option<AbilityId> {
    let statuses = state.bar_statuses();
    ability_line_layout(state.layout().facility().width(), &statuses)
        .get(slot)
        .map(|&(i, _)| statuses[i].id)
}

/// The ability entry at screen cell `(x, y)`, or `None` — the **pure**
/// pointer→identity hit-test for the always-on bar (§11.4). A shell maps a click to
/// a screen cell and asks this; a hit fires `Input::Activate(id)` on the returned
/// ability. It resolves the slot the click landed on and hands it to
/// [`ability_in_slot`], the one seam the keyboard's digits go through too — so it
/// opens no second activation path (the §8.4 regression) and, on a cooling/active
/// entry, refuses for free in the economy (§4.4) with no turn spent.
///
/// The geometry mirrors [`render_screen`] exactly, drawing from the same shared
/// layout ([`ability_line_layout`]) the render draws with, so a click can never miss
/// the entry that is shown — nor hit one the row truncated away.
///
/// The target is the **whole slot**, not just the glyphs in it: the slot is fixed
/// width (see [`BAR_SLOT`]), so a tap lands on the same ability whether it is
/// showing `Camo` or `Camo/20/`, and a short name is no harder to hit than a long
/// one. Only the trailing [`BAR_GAP`] is dead, keeping neighbouring targets apart.
pub fn ability_at(state: &State, x: u32, y: u32) -> Option<AbilityId> {
    let facility = state.layout().facility();
    if y != ability_row(facility.height()) {
        return None; // the bar is the frame's last row and nothing else is the bar
    }
    let slot = ability_line_layout(facility.width(), &state.bar_statuses())
        .into_iter()
        .position(|(_, start)| x >= start && x < start + MAX_BAR_ENTRY as u32)?;
    ability_in_slot(state, slot)
}

/// The §11.2 category an ability entry reads in, by its state: an available ability
/// — ready, active, or a passive in effect — is **Owned** (blue, "yours, in hand");
/// a cooling one is **System** (the muted furniture tan, "unavailable, will
/// return"); an unusable one is **Ground** (dim grey, receding) — discoverable but
/// plainly not an option now. The `[N]` / `/N/` / `(N)` / `(on)` notation carries the
/// rest, so those states share a colour without ambiguity.
///
/// The two #302 states take the colour of what they *are*, not of the axis they come
/// from: an ability with uses left is available, so it is Owned like any ready one;
/// an [`Exhausted`](AbilityState::Exhausted) one is not merely waiting — it is done
/// for this facility — so it recedes to Ground beside the other things you cannot do,
/// rather than to the System tan that promises a return.
fn bar_category(state: AbilityState) -> Category {
    match state {
        AbilityState::Ready
        | AbilityState::Active { .. }
        | AbilityState::Limited { .. }
        | AbilityState::Passive => Category::Owned,
        AbilityState::Cooling { .. } => Category::System,
        AbilityState::Exhausted | AbilityState::Unusable => Category::Ground,
        // The crate's own tech on an exchange row (#266) is **Interest** — the reward
        // channel the `¤` it came out of is drawn in, and the intel console beside it.
        // It is the one entry on that row that is not yours yet, and the colour is what
        // says so: three blue entries you hold, one gold one you are being offered.
        AbilityState::Offered => Category::Interest,
    }
}

/// Lay one status row out as grid cells: segments left to right from column `start`,
/// two spaces between segments, stopping at column `limit`; `band` paints every cell's
/// background (the §11.4 message band) or none. The row is `width` cells wide either
/// way — `start` and `limit` bound only the *words*, so the near line's controls (#267)
/// keep their cells at both ends instead of being written over by a long message.
///
/// The band carries its own [`Fill`] (§11.4/#420): a row has no fog to derive one from,
/// so it says outright which of the category's two shades it wants — the full one for a
/// message, the quiet one for a standing fact.
pub(super) fn status_row(
    width: u32,
    start: u32,
    limit: u32,
    segments: &[(String, Category)],
    band: Option<(Category, Fill)>,
) -> Vec<GlyphCell> {
    let blank = GlyphCell {
        bg: band.map(|(category, _)| category),
        fill: band.map_or(Fill::Full, |(_, fill)| fill),
        ..GlyphCell::blank()
    };
    let mut cells = vec![blank; width as usize];
    let limit = (limit as usize).min(cells.len());
    let mut x = start as usize;
    for (i, (text, category)) in segments.iter().enumerate() {
        if i > 0 {
            x += 2;
        }
        for glyph in text.chars() {
            if x >= limit {
                return cells;
            }
            cells[x] = GlyphCell {
                glyph,
                fg: *category,
                ..blank
            };
            x += 1;
        }
    }
    cells
}

#[cfg(test)]
mod tests;
