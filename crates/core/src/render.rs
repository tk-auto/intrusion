//! Rendering as a pure function of state (§11.1, §12.1) — **the one place rendering
//! lives**.
//!
//! The game draws as a grid of cells, each a character plus a foreground *category*
//! plus a background (§11.1). This is a **pure function of [`State`]**: it composes
//! the terrain grid **and** the entities on it — the player, the guards, later bodies
//! and decoys — into one grid, resolving overlaps by a defined **glyph priority**
//! (§11.3). Because it prints as text it is assertable in a native test with no
//! browser, which is what makes UI iteration agent-checkable (§11.1).
//!
//! # The seam, stated once so it stops drifting
//!
//! **All rendering is here.** A platform shell (the wasm/canvas web crate, §12.2) does
//! exactly one thing with the grid this produces: map each cell's [`Category`] to a
//! concrete colour and blit it. **A shell never decides a glyph, never resolves an
//! overlap, never picks a colour by looking at game state** — if it did, the core
//! would no longer be the single source of truth for what the game looks like, and
//! two renderers (say ASCII and tiles) could disagree. The renderer is a *separate
//! concern behind one interface* (§11.1): ASCII now, `drawImage` tiles later, same
//! grid. The core must not know which shell consumes it.
//!
//! Fog and tile memory (§11.5a) are applied here, because they are *presentation of
//! knowledge*, not physics: the [`State`] keeps the whole true world plus the
//! player's per-cell memory, and this function draws only what §11.5a says the
//! player knows — geometry always, contents once seen, live state only in the
//! current FOV. Each drawn cell carries a [`Visibility`] so the shell can style the
//! three knowledge states distinctly.
//!
//! The **danger overlay** (§11.5) is painted here too — [`GlyphCell::bg`] set to
//! `Danger` on every cell watched by a guard the player can see — because it must
//! read the *same* sight data the guard AI reads
//! ([`Guard::fov`](crate::state::Guard::fov)), not a re-implementation that could
//! lie. What is **not** here yet: the two red shades of the §7.6 two-zone
//! detection (certain vs glimpse) — detection zones are a guard ticket; until it
//! lands the whole cone is one zone. Colour *values* are the shell's table
//! (§11.2); this module only speaks in categories.

use crate::ability::{AbilityId, AbilityState, AbilityStatus};
use crate::category::Category;
use crate::cell::{Cell, Direction};
use crate::facility::{Facility, Terrain};
use crate::state::{GuardPerception, State};
use crate::status::near_line;

/// How much the player currently knows about what a drawn cell shows — the three
/// visual states of §11.5a's implementation note (live / remembered / never-seen,
/// where "never-seen" contents are simply not drawn and their cell falls back to
/// its geometry). The shell styles each distinctly; remembered must **not** be
/// collapsed into the §11.5 dimming scheme.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Visibility {
    /// Inside the player's FOV right now — drawn full colour (§11.5).
    Live,
    /// Outside the FOV, showing the always-visible layer: geometry, or the
    /// geometry masking a never-seen content. The shell renders this dark gray —
    /// dim but legible (§11.5).
    Dimmed,
    /// Outside the FOV, drawn from tile memory: a content seen earlier this run
    /// (§11.5a) — its own visual state, distinct from both live and dimmed.
    Remembered,
}

/// One rendered cell: a glyph, its foreground category, an optional background
/// category (§11.1), and the knowledge state it is drawn in (§11.5a).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct GlyphCell {
    /// The character to draw; a space is empty, painted as background only.
    pub glyph: char,
    /// What the glyph *means* (§11.2). The shell maps this to a colour.
    pub fg: Category,
    /// The background category, or `None` for the default backdrop. `Danger` is
    /// the §11.5 overlay: this cell is watched by a guard the player can see.
    pub bg: Option<Category>,
    /// The knowledge state this cell is drawn in (§11.5a): live, dimmed geometry,
    /// or remembered content. The shell styles the three distinctly.
    pub vis: Visibility,
}

/// A rendered frame: a `width × height` grid of [`GlyphCell`]s in row-major order —
/// the whole picture, ready for a shell to colour and blit (§11.1).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Grid {
    width: u32,
    height: u32,
    cells: Vec<GlyphCell>,
}

impl Grid {
    /// The grid width in cells.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// The grid height in cells.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// The cell at `(x, y)`, row-major. Panics if off the grid — the shell iterates
    /// `0..width × 0..height`, so an out-of-range read is a caller bug.
    pub fn get(&self, x: u32, y: u32) -> GlyphCell {
        self.cells[(y * self.width + x) as usize]
    }

    /// The glyphs as one `String` per row, top to bottom — the text view that makes
    /// a frame assertable in a native test (§11.1), and the basis of golden tests.
    pub fn to_text(&self) -> Vec<String> {
        (0..self.height)
            .map(|y| (0..self.width).map(|x| self.get(x, y).glyph).collect())
            .collect()
    }
}

/// Render `state` to a full [`Grid`] (§11.1): terrain through the §11.5a fog first,
/// then every *visible* entity on top — resolving overlaps by the glyph priority
/// below — then the §11.5 danger overlay across everything.
///
/// # The fog (§11.5a)
///
/// Terrain splits into the design's layers. **Geometry** — walls, floor, hinges,
/// door *positions*, and the exit (the door you came in by, §4.5, and the anchor of
/// every escape plan, §7.6) — draws as-is from turn one, never fogged. **Contents**
/// — a console, a hideout — draw only inside the current FOV or, once their cell is
/// in tile memory, as [`Visibility::Remembered`]; never seen, the cell masks as the
/// geometry naturally in its place (floor under a console, wall over a hideout
/// alcove — the scouting reward of §11.5a). **Live state** — guards, and a door's
/// open/closed pose — draws only inside the FOV and is never remembered: an
/// out-of-view panel always shows its canonical closed `+`, whatever it really is.
/// The one exception is a guard's *position*, known through walls within the
/// guard-sense box (§9): a guard out of the FOV but in range gets a flat orange
/// Sensed background on its cell — position only, no cone, and still never remembered
/// once out of range.
///
/// # Glyph priority (§11.3)
///
/// The old renderer was last-writer-wins, so a guard standing in a doorway rendered
/// arbitrarily. Here the order is **defined**: entities always draw over terrain, and
/// among glyphs the ranking is **player > guard > body > decoy** (§7.2/§8.3). We
/// write terrain, then the decoy, then bodies, then seen guards, then the player, so
/// the highest-priority glyph is the last writer at any cell — a defined order, not an
/// accident. A *sensed* guard (§9.2) is not a glyph at all — it is an orange
/// background highlight, painted with the danger overlay below — so it never competes
/// with the glyph layer.
///
/// # The danger overlay (§11.5)
///
/// The best idea in the old game, kept **[SETTLED]**: every cell watched by a
/// guard *the player can see* gets a `Danger` background — the literal detection
/// set, the same [`Guard::fov`](crate::state::Guard::fov) the guard AI reads, so
/// the picture cannot lie. If your cell isn't red, no guard you can see will
/// detect you: the lose condition, painted. It covers watched cells even
/// *outside* the player's FOV — a visible guard's cone is knowledge you have —
/// fixing the old bug where watched-but-unseen cells rendered dark-on-dark and
/// looked like the safest cells on the map. Cones of guards the player cannot
/// see are unknown information and paint nothing.
///
/// # Floor dots (§11.5)
///
/// Floor draws as `·`, not blank: a blank cell has no foreground, so the FOV
/// boundary was undetectable across open ground — you could only see the sight
/// edge where it crossed a wall. Dots give every floor cell a glyph for the
/// dimming to act on. An open door panel stays blank (§10.3): the gap in the
/// wall *is* its rendering.
pub fn render(state: &State) -> Grid {
    let facility = state.layout().facility();
    let (width, height) = (facility.width(), facility.height());
    let fov = state.player_fov();
    let memory = state.memory();

    // Terrain layer, through the fog: what the player knows of each cell.
    let mut cells: Vec<GlyphCell> = (0..height)
        .flat_map(|y| (0..width).map(move |x| (x, y)))
        .map(|(x, y)| {
            let terrain = facility
                .terrain_at(x, y)
                .expect("in-bounds by construction");
            let cell = Cell::new(x, y);
            let (shown, vis) = if fov.contains(cell) {
                (terrain, Visibility::Live)
            } else {
                fogged_view(terrain, memory.contains(cell))
            };
            // Floor dots (§11.5): give open ground a foreground so the FOV edge
            // reads across it. Masked contents dot too — they *show* floor.
            let glyph = if shown == Terrain::Floor {
                '·'
            } else {
                shown.glyph()
            };
            GlyphCell {
                glyph,
                fg: shown.category(),
                bg: None,
                vis,
            }
        })
        .collect();
    // Entities are live state: whatever is drawn here is being seen right now.
    let mut put = |cell: Cell, glyph: char, fg: Category| {
        cells[(cell.y * width + cell.x) as usize] = GlyphCell {
            glyph,
            fg,
            bg: None,
            vis: Visibility::Live,
        };
    };

    // Entity layers, lowest priority first so the top entity is the last writer.
    // The decoy (§8.3) draws lowest: an Owned `@` — a thing you made wearing
    // your own glyph, which is the whole trick (§10.3/§11.3). Live state like
    // every entity: in the FOV or not at all.
    if let Some(decoy) = state.decoy() {
        if fov.contains(decoy) {
            put(decoy, '@', Category::Owned);
        }
    }
    // A body (§7.2) is live state like any entity — drawn only inside the FOV,
    // never remembered — as the `z` a downed guard reads as (§10.3), in Caution:
    // an unaware threat's colour, because what a body *means* is trouble waiting
    // to be found (§11.3). Two exceptions speak the Owned vocabulary instead:
    // the body **in your hands** (§8.3) draws an Owned `z` — like the cupboard
    // you hide in, it is yours while you hold it — and a body **in a hideout**
    // is gone (§7.2): no `z` at all (the cupboard keeps its glyph; the recolour
    // joins the other Owned signals below).
    for body in state.bodies() {
        if !fov.contains(body.cell()) || facility.terrain(body.cell()) == Some(Terrain::Hideout) {
            continue;
        }
        let fg = if state.dragging() == Some(body.cell()) {
            Category::Owned
        } else {
            Category::Caution
        };
        put(body.cell(), 'z', fg);
    }
    // A **seen** guard (in the FOV, §9.2) draws as the full state-coloured `g`; the
    // `g` glyph is re-categorised every turn from the guard's state (§11.2): yellow →
    // orange → red is the guard's mind, made visible. A **sensed** guard is a
    // *background* highlight instead, painted below alongside the danger overlay — no
    // glyph of its own. A guard perceived neither way draws nothing and is never
    // remembered (§11.5a), so leaving both view and sense range erases it.
    for guard in state.guards() {
        if state.perceive_guard(guard) == Some(GuardPerception::Seen) {
            put(guard.pos(), 'g', guard.state().category());
        }
    }
    // The player, always Owned — trivially inside their own FOV. Inside a hideout
    // the player is concealed: the cupboard keeps its `}` glyph but recolours to
    // Owned (§10.3/§11.3) — the "you are hidden here" signal — instead of drawing
    // the `@`. Read through the same `hidden` query the loop and vision use, so
    // the picture cannot disagree.
    let player_glyph = if state.hidden() { '}' } else { '@' };
    put(state.player(), player_glyph, Category::Owned);

    // The crouch signal (§10.3/§11.3): while the player is crouched, the whole
    // run they ducked behind — that bench, not every table they stand beside —
    // recolours to Owned, the same vocabulary the occupied cupboard speaks
    // ("Owned = what is concealing you"), so the blue @-π pair reads as one
    // hidden unit whose π half is as long as the furniture. Read through the
    // same anchored run the concealment rule uses, so the picture cannot
    // disagree with the rules.
    for cover in state.crouch_cover() {
        cells[(cover.y * width + cover.x) as usize].fg = Category::Owned;
    }

    // The hidden-body signal (§7.2/§11.3): a cupboard with a body stowed in it
    // keeps its `}` glyph — the body is *gone* — but recolours to Owned while
    // seen, the same "something of yours is in here" vocabulary the occupied
    // cupboard and the covering table speak.
    for body in state.bodies() {
        if fov.contains(body.cell()) && facility.terrain(body.cell()) == Some(Terrain::Hideout) {
            cells[(body.cell().y * width + body.cell().x) as usize].fg = Category::Owned;
        }
    }

    // The sensed highlight (§9.2): every guard the player *senses* through a wall but
    // cannot see gets an orange `Category::Sensed` background on its exact cell — a
    // filled, eye-catching marker over whatever geometry masks the cell, position only
    // and never a glyph of its own. It carries no cone and no danger overlay: knowing
    // where a guard is is not knowing whether it can see you. Painted *before* the
    // danger overlay so a coincident red still wins — a sensed guard's cell that a
    // *seen* guard also watches reads danger first (§11.5: being seen outranks).
    for guard in state.guards() {
        if state.perceive_guard(guard) == Some(GuardPerception::Sensed) {
            cells[(guard.pos().y * width + guard.pos().x) as usize].bg = Some(Category::Sensed);
        }
    }

    // The danger overlay (§11.5), last, across terrain and entities alike: the
    // union of every visible guard's cone. Backgrounds compose with whatever
    // glyph is on the cell — a watched guard, a watched player, watched floor.
    // The one exception is the player's own cell while they are concealed from
    // that guard — in a cupboard, or crouched behind a table the guard looks
    // across (§10.3): the overlay's promise is "red under you = detected"
    // (§11.5), and a concealed player is not. The table itself stays red — the
    // guard watches the furniture, just not what is ducked behind it.
    for guard in state.guards() {
        if !fov.contains(guard.pos()) {
            continue; // an unseen guard's cone is unknown, not safe — just unknown
        }
        let spare_player = state.concealed_from(guard.pos());
        for cell in guard.fov().cells() {
            if spare_player && cell == state.player() {
                continue;
            }
            cells[(cell.y * width + cell.x) as usize].bg = Some(Category::Danger);
        }
    }

    Grid {
        width,
        height,
        cells,
    }
}

/// What an out-of-FOV cell shows (§11.5a), given whether its cell is in tile
/// memory: the terrain to draw and the knowledge state to draw it in. One
/// exhaustive match, so every new terrain kind is forced to declare its layer —
/// geometry, contents, or live state — the day it is added.
fn fogged_view(terrain: Terrain, remembered: bool) -> (Terrain, Visibility) {
    match terrain {
        // Geometry: always visible, never fogged (§11.5a). The exit is geometry —
        // the player entered by it (§4.5) and plans escape routes around it (§7.6).
        // A table is geometry too: it replaced a stamped wall (§10.1a), and being
        // surprised by furniture mid-flight is as bad as being surprised by a wall.
        Terrain::Floor
        | Terrain::Wall
        | Terrain::DoorHinge
        | Terrain::Exit
        | Terrain::PartialCover => (terrain, Visibility::Dimmed),
        // A door's *position* is geometry but its open/closed pose is live state,
        // never remembered: out of view a panel always draws canonically closed.
        Terrain::DoorPanelClosed | Terrain::DoorPanelOpen => {
            (Terrain::DoorPanelClosed, Visibility::Dimmed)
        }
        // Contents: hidden until seen, then remembered (§11.5a).
        Terrain::Console | Terrain::Hideout if remembered => (terrain, Visibility::Remembered),
        // Never seen: masked by the geometry naturally in its place — plain floor
        // where a console stands, plain wall over a hideout alcove, so the map
        // gives neither away before the player has scouted it.
        Terrain::Console => (Terrain::Floor, Visibility::Dimmed),
        Terrain::Hideout => (Terrain::Wall, Visibility::Dimmed),
    }
}

/// The rows the screen adds beneath the map (§11.4): the near line and the
/// usable line. A shell fitting the screen sizes for `HEADER_ROWS + facility
/// height + this`.
pub const STATUS_ROWS: u32 = 2;

/// The row the screen adds **above** the map (§11.4): the always-on ability line.
/// A shell fits for `this + facility height + STATUS_ROWS`.
pub const HEADER_ROWS: u32 = 1;

/// The transient **view state** a shell keeps between frames and hands to
/// [`render_screen`] (§11.4). It is deliberately *not* part of [`State`] — the
/// core stays pure game logic (§12.1), and what the player has merely chosen to
/// *look at* changes no world and costs no turn. The shell owns it, toggles it
/// from [`ui_command_for_key`](crate::input::ui_command_for_key) or a click on the
/// deploy button ([`is_ability_button`]), and passes it in.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct ScreenUi {
    /// Whether the full ability panel is deployed (§11.4). The compact ability
    /// line is always drawn; this gates only the expanded, named panel.
    pub ability_panel_open: bool,
}

/// The deploy button's label on the ability line (§11.4): a downward chevron when
/// the panel is closed (bump it *open*), an upward one when it is open. Both are
/// three cells wide, so the button's footprint is fixed regardless of state.
const BUTTON_CLOSED: &str = "[▾]";
const BUTTON_OPEN: &str = "[▴]";
const BUTTON_LEN: u32 = 3;

/// The column the deploy button starts at on a screen `width` wide: right-aligned
/// with a one-cell margin. Shared by the drawing ([`ability_line`]) and the
/// hit-test ([`is_ability_button`]) so the button a click lands on is exactly the
/// button drawn.
fn button_start(width: u32) -> u32 {
    width.saturating_sub(1 + BUTTON_LEN)
}

/// Whether screen cell `(x, y)` is the deploy button (§11.4) — the header row's
/// right-aligned toggle. A shell maps a click to a screen cell and asks this; a
/// hit flips [`ScreenUi::ability_panel_open`] instead of stepping. It is the one
/// piece of the button's geometry the shell needs, kept here beside the drawing so
/// the two can never disagree.
pub fn is_ability_button(width: u32, x: u32, y: u32) -> bool {
    let start = button_start(width);
    y == 0 && x >= start && x < start + BUTTON_LEN
}

/// Render the full §11.4 **screen**: the always-on ability line, the map
/// ([`render`]), and the two status lines beneath it — `HEADER_ROWS + map height +
/// STATUS_ROWS` rows, same width, one [`Grid`], so a whole frame is still a pure
/// function of `(state, ui)` that prints as text (§11.1) and golden-testable to
/// the last row.
///
/// - **Ability line** (row `0`): the always-on compact readout — every ability's
///   hotkey coloured by state, its active/cooling number inline
///   ([`AbilityStatus::compact`]) — plus the right-aligned **deploy button**
///   ([`is_ability_button`]). This is the permanent home for ability state
///   (§11.4): one row, glanceable, never covering the board.
/// - **Near line** (row `height-2`): the highest-priority message of the last
///   action, or the ambient floor ([`near_line`], §11.7) — a solid band in the
///   message's category with the words in Neutral on top.
/// - **Usable line** (row `height-1`): the adjacent bump affordances
///   ([`State::affordances`]), each in its own category, no band.
///
/// # The deployable ability panel (§11.4, §15 Q9)
///
/// When the shell has the panel **deployed** (`ui.ability_panel_open`, driven by
/// the deploy button or the `Tab` toggle), the full named panel — each ability's
/// `<key> <Name> <state>` ([`AbilityStatus::label`]) — is overlaid on the map, in
/// the **corner opposite the player** ([`panel_origin_opposite_player`]) so it
/// never covers where the action is. It is not tied to waiting: an earlier
/// experiment showed it on the wait turn, which buried exactly the 360° guard-sense
/// the wait exists to reveal (§9.1) — so the panel is now on demand, and waiting
/// stays a clear look around. Both the line and the panel draw the run's **real**
/// ability state ([`State::ability_statuses`]); a click on either resolves to the
/// ability under it ([`ability_at`]) and activates it exactly as its hotkey would.
pub fn render_screen(state: &State, ui: ScreenUi) -> Grid {
    let statuses = state.ability_statuses();

    // The map layer, with the deployed panel overlaid opposite the player.
    let mut map = render(state);
    if ui.ability_panel_open {
        let origin =
            panel_origin_opposite_player(map.width(), map.height(), state.player(), &statuses);
        overlay_ability_panel(&mut map, origin, &statuses);
    }
    let width = map.width();
    let height = HEADER_ROWS + map.height() + STATUS_ROWS;

    // One grid, top to bottom: the ability line, the map, the two status lines.
    let mut cells = ability_line(width, &statuses, ui.ability_panel_open);
    cells.extend(map.cells);

    let message = near_line(state);
    cells.extend(status_row(
        width,
        &[(message.text, Category::Neutral)],
        Some(message.category),
    ));
    let usable: Vec<(String, Category)> = state
        .affordances()
        .into_iter()
        .map(|(dir, a)| (format!("{} {}", arrow(dir), a.label()), a.category()))
        .collect();
    cells.extend(status_row(width, &usable, None));

    Grid {
        width,
        height,
        cells,
    }
}

/// Lay the compact ability line out (§11.4): the start column of each entry that
/// fits before the deploy button, in draw order, as `(status index, start col)`.
/// Entries begin at a one-cell margin with a single space between; the strip stops
/// the moment the next entry would run into the button ([`button_start`]). Shared
/// by [`ability_line`] (drawing) and [`ability_at`] (hit-testing) so a click can
/// never land on an entry the row did not draw.
fn ability_line_layout(width: u32, statuses: &[AbilityStatus]) -> Vec<(usize, u32)> {
    let mut out = Vec::new();
    let mut x = 1; // the one-cell left margin
    for (i, status) in statuses.iter().enumerate() {
        let len = status.compact().chars().count() as u32;
        if x + len > button_start(width) {
            break;
        }
        out.push((i, x));
        x += len + 1; // one space between abilities
    }
    out
}

/// The always-on ability line (§11.4): one row carrying every ability's compact
/// readout ([`AbilityStatus::compact`]) from a one-cell left margin, each in its
/// state colour ([`panel_category`]), with the deploy button
/// ([`is_ability_button`]) right-aligned. Single spaces between abilities keep the
/// whole set on one row; the button's chevron points down when closed, up when
/// open. No band — the line reads as a quiet HUD strip, not a message.
fn ability_line(width: u32, statuses: &[AbilityStatus], open: bool) -> Vec<GlyphCell> {
    let blank = GlyphCell {
        glyph: ' ',
        fg: Category::Neutral,
        bg: None,
        vis: Visibility::Live,
    };
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

    for (i, start) in ability_line_layout(width, statuses) {
        let status = &statuses[i];
        put(
            &mut cells,
            start,
            &status.compact(),
            panel_category(status.state),
        );
    }

    let label = if open { BUTTON_OPEN } else { BUTTON_CLOSED };
    put(&mut cells, button_start(width), label, Category::System);
    cells
}

/// The ability entry at screen cell `(x, y)`, or `None` — the **pure**
/// pointer→identity hit-test for both the always-on line and the deployed panel
/// (§11.4), the sibling of [`is_ability_button`]. A shell maps a click to a screen
/// cell and asks this; a hit fires `Input::Activate(id)` on the returned ability,
/// resolving by **identity**, never by the row it landed on (§11.6) — so it opens
/// no second activation path (the §8.4 regression) and, on a cooling/active entry,
/// refuses for free in the economy (§4.4) with no turn spent.
///
/// The geometry mirrors [`render_screen`] exactly, drawing from the same shared
/// layout ([`ability_line_layout`]) and panel origin ([`panel_origin_opposite_player`])
/// the render draws with, so a click can never miss the entry that is shown. Row 0
/// is the compact line; when the panel is deployed, its rows are hit-tested on the
/// map layer beneath the header. The deploy button is never an ability — the line
/// stops before it and the shell tests the button first — so a tap there toggles the
/// panel and never falls through to an activation underneath.
pub fn ability_at(state: &State, ui: ScreenUi, x: u32, y: u32) -> Option<AbilityId> {
    let statuses = state.ability_statuses();
    let facility = state.layout().facility();
    let (map_w, map_h) = (facility.width(), facility.height());

    // Row 0: the always-on compact line.
    if y == 0 {
        for (i, start) in ability_line_layout(map_w, &statuses) {
            let len = statuses[i].compact().chars().count() as u32;
            if x >= start && x < start + len {
                return Some(statuses[i].id);
            }
        }
        return None;
    }

    // The deployed panel, overlaid on the map layer below the header (§11.4).
    if ui.ability_panel_open && y >= HEADER_ROWS {
        let (mx, my) = (x, y - HEADER_ROWS);
        let (ox, oy) = panel_origin_opposite_player(map_w, map_h, state.player(), &statuses);
        let band = panel_band_width(&statuses);
        if mx >= ox && mx < ox + band && my >= oy && my < map_h {
            let row = (my - oy) as usize;
            if row < statuses.len() {
                return Some(statuses[row].id);
            }
        }
    }
    None
}

/// The width of the deployed panel's cleared band (§11.4): one cell wider than the
/// longest label, for an even right edge and a hair of padding off the map. Shared
/// by the origin ([`panel_origin_opposite_player`]), the overlay
/// ([`overlay_ability_panel`]) and the hit-test ([`ability_at`]) so all three agree
/// on the block's footprint.
fn panel_band_width(statuses: &[AbilityStatus]) -> u32 {
    statuses
        .iter()
        .map(|s| s.label().chars().count())
        .max()
        .unwrap_or(0) as u32
        + 1
}

/// The map-space corner to anchor the deployed panel at, **opposite the player**
/// (§11.4): a player in the left half puts the panel on the right, a player in the
/// top half puts it at the bottom, and so on — so the panel is always as far from
/// the player as the board allows and never covers where they are acting. A
/// one-cell inset keeps a border of map around it; sizes are clamped so a tiny
/// hand-built board never underflows (the v1 board is 40×40, §10.2). Takes the map
/// dimensions rather than the [`Grid`] so the hit-test can reuse it without a
/// rendered frame.
fn panel_origin_opposite_player(
    map_w: u32,
    map_h: u32,
    player: Cell,
    statuses: &[AbilityStatus],
) -> (u32, u32) {
    let panel_w = panel_band_width(statuses);
    let panel_h = statuses.len() as u32;

    // Player left of centre → panel right; player above centre → panel bottom.
    let x0 = if player.x < map_w / 2 {
        map_w.saturating_sub(panel_w + 1)
    } else {
        1
    };
    let y0 = if player.y < map_h / 2 {
        map_h.saturating_sub(panel_h + 1)
    } else {
        1
    };
    (x0.max(1).min(map_w.saturating_sub(1)), y0.max(1))
}

/// Overlay the deployed ability panel onto the map `grid` at `(ox, oy)` (§11.4):
/// one row per ability, each `<key> <Name> <state>` ([`AbilityStatus::label`])
/// coloured by state ([`panel_category`]). Every row is cleared to a uniform width
/// first so the block reads as a solid panel over the board rather than text
/// tangled with the map beneath.
///
/// Bounds are clamped, never asserted: on a board too small to hold every row (only
/// hand-built test states get that small — the v1 board is 40×40, §10.2) the panel
/// shows as many abilities as fit and stops. It draws over the map layer only,
/// before the header and status rows are added, so it can never collide with them.
fn overlay_ability_panel(grid: &mut Grid, origin: (u32, u32), statuses: &[AbilityStatus]) {
    let (ox, oy) = origin;
    // A uniform band, one space wider than the longest label, so the cleared box
    // has an even right edge and a hair of padding off the map.
    let width = panel_band_width(statuses);

    for (i, status) in statuses.iter().enumerate() {
        let y = oy + i as u32;
        if y >= grid.height {
            break; // out the bottom of a tiny board — show what fits, drop the rest
        }
        // Clear the row's band to background, then write the label over it.
        for dx in 0..width {
            let x = ox + dx;
            if x >= grid.width {
                break;
            }
            grid.cells[(y * grid.width + x) as usize] = GlyphCell {
                glyph: ' ',
                fg: Category::Neutral,
                bg: None,
                vis: Visibility::Live,
            };
        }
        let category = panel_category(status.state);
        for (dx, glyph) in status.label().chars().enumerate() {
            let x = ox + dx as u32;
            if x >= grid.width {
                break;
            }
            grid.cells[(y * grid.width + x) as usize] = GlyphCell {
                glyph,
                fg: category,
                bg: None,
                vis: Visibility::Live,
            };
        }
    }
}

/// The §11.2 category an ability row reads in, by its state: an available ability
/// — ready or active — is **Owned** (blue, "yours, in hand"); a cooling one is
/// **System** (the muted furniture tan, "unavailable, will return"); an unusable
/// one is **Ground** (dim gray, receding) — discoverable but plainly not an option
/// now. The `[N]` / `/N/` notation carries the rest, so ready and active share a
/// colour without ambiguity.
fn panel_category(state: AbilityState) -> Category {
    match state {
        AbilityState::Ready | AbilityState::Active { .. } => Category::Owned,
        AbilityState::Cooling { .. } => Category::System,
        AbilityState::Unusable => Category::Ground,
    }
}

/// The usable line's direction glyph (§11.4): which way to bump for the
/// affordance beside it.
fn arrow(dir: Direction) -> char {
    match dir {
        Direction::North => '↑',
        Direction::East => '→',
        Direction::South => '↓',
        Direction::West => '←',
    }
}

/// Lay one status row out as grid cells: segments left to right from a one-cell
/// margin, two spaces between segments, truncated at the edge; `band` paints
/// every cell's background (the §11.4 message band) or none.
fn status_row(
    width: u32,
    segments: &[(String, Category)],
    band: Option<Category>,
) -> Vec<GlyphCell> {
    let blank = GlyphCell {
        glyph: ' ',
        fg: Category::Neutral,
        bg: band,
        vis: Visibility::Live,
    };
    let mut cells = vec![blank; width as usize];
    let mut x = 1; // the one-cell left margin
    for (i, (text, category)) in segments.iter().enumerate() {
        if i > 0 {
            x += 2;
        }
        for glyph in text.chars() {
            if x >= cells.len() {
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

/// Render a facility's **terrain only** to a grid of glyphs, one `String` per row
/// (§11.1) — no entities. This is the generator's debug view: generation works on a
/// [`Facility`] before any actor exists, so its tests read the bare terrain. The full
/// game picture (terrain + entities) is [`render`].
pub fn ascii_grid(facility: &Facility) -> Vec<String> {
    (0..facility.height())
        .map(|y| {
            (0..facility.width())
                .map(|x| {
                    facility
                        .terrain_at(x, y)
                        .expect("in-bounds by construction")
                        .glyph()
                })
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::{Cell, Direction};
    use crate::facility::{Facility, Terrain};
    use crate::guard::Guard;
    use crate::state::{Input, State};
    use crate::test_support::open_room;

    /// A hand-built state on a `w × h` walled box: the player, some guards, and a far
    /// exit, no objectives. Enough to render. Faces **south**, toward where these
    /// tests put their guards — entities are live state (§11.5a) and draw only
    /// inside the FOV, so a guard the test asserts on must be in view.
    fn state(w: u32, h: u32, player: Cell, guards: Vec<Guard>) -> State {
        State::new(
            open_room(w, h),
            player,
            Direction::South,
            guards,
            Vec::new(),
            Cell::new(w - 2, h - 2),
        )
    }

    /// The payoff of "render is a pure function that prints as text" (§11.1): a fixed
    /// state renders to a fixed grid we can eyeball. Terrain-only `ascii_grid` of a
    /// 6×4 walled box is a hollow rectangle of `#`.
    #[test]
    fn walled_box_renders_as_a_hollow_rectangle() {
        let grid = ascii_grid(&Facility::walled_box(6, 4));
        assert_eq!(
            grid,
            vec![
                "######".to_string(),
                "#    #".to_string(),
                "#    #".to_string(),
                "######".to_string(),
            ]
        );
    }

    #[test]
    fn grid_dimensions_match_the_facility() {
        let facility = Facility::walled_box(40, 30);
        let grid = ascii_grid(&facility);
        assert_eq!(grid.len(), 30);
        assert!(grid.iter().all(|row| row.chars().count() == 40));
        // The full render is the same shape.
        let g = render(&state(40, 30, Cell::new(5, 5), Vec::new()));
        assert_eq!((g.width(), g.height()), (40, 30));
        assert_eq!(g.to_text().len(), 30);
    }

    /// The full render composes entities over terrain: the player `@` and a guard `g`
    /// appear on the grid, each with its category (§11.2/§11.3).
    #[test]
    fn render_draws_the_player_and_guards_over_terrain() {
        let s = state(
            10,
            10,
            Cell::new(3, 3),
            vec![Guard::stationary(Cell::new(6, 4))],
        );
        let g = render(&s);

        let player = g.get(3, 3);
        assert_eq!(player.glyph, '@');
        assert_eq!(player.fg, Category::Owned);

        let guard = g.get(6, 4);
        assert_eq!(guard.glyph, 'g');
        assert_eq!(guard.fg, Category::Caution);

        // A plain floor cell renders as a dot (§11.5), Ground — the recessive
        // category, so the dots never compete with walls or entities for the eye.
        assert_eq!(g.get(5, 5).glyph, '·');
        assert_eq!(g.get(1, 1).fg, Category::Ground); // interior floor
    }

    /// §7.2/§10.3: a body in view draws as the Caution `z` — live state, like the
    /// guard it used to be. Behind the fog it draws nothing: masked as the floor
    /// naturally in its place, never remembered.
    #[test]
    fn a_body_in_view_draws_as_a_caution_z() {
        // The takedown that makes a body: strike an unaware guard from a cupboard
        // (concealment is the only way to be adjacent undetected, §6.1/§7.2).
        let mut layout = open_room(10, 10);
        layout.place(Cell::new(5, 5), Terrain::Hideout);
        let mut s = State::new(
            layout,
            Cell::new(5, 5),
            Direction::North,
            vec![Guard::stationary(Cell::new(5, 4))],
            Vec::new(),
            Cell::new(8, 8),
        );
        s.step(Input::Step(Direction::North));
        assert_eq!(s.bodies().len(), 1, "precondition: the takedown landed");

        let body = render(&s).get(5, 4);
        assert_eq!(body.glyph, 'z');
        assert_eq!(body.fg, Category::Caution);
        assert_eq!(body.vis, Visibility::Live);

        // Turn away and walk south until the body's cell leaves the FOV: it is
        // live state — not remembered — so the cell masks as plain floor again.
        while s.player_fov().contains(Cell::new(5, 4)) {
            s.step(Input::Step(Direction::South));
        }
        let masked = render(&s).get(5, 4);
        assert_eq!(masked.glyph, '·', "an unseen body draws as the floor dot");
        assert_eq!(masked.vis, Visibility::Dimmed);
    }

    /// §8.3/§10.3/§11.3: the decoy draws as an Owned `@` — a thing you made,
    /// wearing your own glyph; two identical blue `@`s on screen is the trick
    /// working as designed.
    #[test]
    fn a_decoy_draws_as_an_owned_at_glyph() {
        use crate::AbilityId;
        let mut s = state(10, 10, Cell::new(4, 4), Vec::new());
        s.step(Input::Step(Direction::South)); // (4,5), facing south
        s.step(Input::Activate(AbilityId::Decoy)); // the fake at (4,6)
        let g = render(&s);
        assert_eq!(g.get(4, 6).glyph, '@');
        assert_eq!(g.get(4, 6).fg, Category::Owned);
        assert_eq!(g.get(4, 5).glyph, '@', "the real player still draws");
    }

    /// §8.3/§11.5: the danger overlay keeps its promise under Camouflage — "red
    /// under you = detected". A cloaked, still player under a visible guard's
    /// cone shows no red on their own cell; before cloaking, the same cell is
    /// red. The cone itself stays painted — the guard watches the ground, it
    /// just cannot see what stands cloaked on it.
    #[test]
    fn the_danger_overlay_spares_a_cloaked_still_player() {
        use crate::AbilityId;
        // Guard at (5,2) looking south down the column; the player at (5,6),
        // facing north so the guard is in view and its cone paints.
        let mut s = State::new(
            open_room(12, 12),
            Cell::new(5, 6),
            Direction::North,
            vec![Guard::stationary(Cell::new(5, 2))],
            Vec::new(),
            Cell::new(10, 10),
        );
        assert_eq!(
            render(&s).get(5, 6).bg,
            Some(Category::Danger),
            "exposed: the watched cell is red",
        );

        s.step(Input::Activate(AbilityId::Camouflage));
        let g = render(&s);
        assert_eq!(g.get(5, 6).bg, None, "cloaked and still: no red under you");
        assert_eq!(
            g.get(5, 5).bg,
            Some(Category::Danger),
            "the cone itself is still painted",
        );
    }

    /// §8.3/§11.3: the body speaks the Owned vocabulary when it is yours — an
    /// Owned `z` while in your hands, and, once stowed in a cupboard, no `z` at
    /// all: the cupboard keeps its `}` and recolours Owned, the same signal the
    /// occupied cupboard gives. The body is gone (§7.2).
    #[test]
    fn a_dragged_body_reads_owned_and_a_stowed_one_vanishes() {
        let mut layout = open_room(10, 10);
        layout.place(Cell::new(5, 5), Terrain::Hideout);
        let mut s = State::new(
            layout,
            Cell::new(5, 5),
            Direction::North,
            vec![Guard::stationary(Cell::new(5, 4))],
            Vec::new(),
            Cell::new(8, 8),
        );
        s.step(Input::Step(Direction::North)); // takedown: body at (5,4)
        s.step(Input::Step(Direction::North)); // grab it

        let held = render(&s).get(5, 4);
        assert_eq!(held.glyph, 'z');
        assert_eq!(held.fg, Category::Owned, "the body in your hands is yours");

        s.step(Input::Step(Direction::South)); // step out: body follows into (5,5)
        let stowed = render(&s).get(5, 5);
        assert_eq!(stowed.glyph, '}', "no z: the body is gone (§7.2)");
        assert_eq!(stowed.fg, Category::Owned, "the cupboard signals the stash");
    }

    /// §11.2's payoff, on screen: the `g` glyph is re-categorised every turn from
    /// the guard's §7.4 state, so a chasing guard reads **Danger** — the player
    /// sees the AI state machine as yellow → orange → red, and no game system ever
    /// named a colour to do it.
    #[test]
    fn a_guards_glyph_category_tracks_its_state() {
        use crate::guard::GuardState;
        for (guard_state, category) in [
            (GuardState::Calm, Category::Caution),
            (GuardState::Alerted, Category::Warning),
            (GuardState::Responding, Category::Warning),
            (GuardState::Investigating, Category::Danger),
            (GuardState::Chasing, Category::Danger),
        ] {
            let s = state(
                10,
                10,
                Cell::new(3, 3),
                vec![Guard::stationary(Cell::new(6, 4)).with_state(guard_state)],
            );
            let cell = render(&s).get(6, 4);
            assert_eq!(cell.glyph, 'g');
            assert_eq!(
                cell.fg, category,
                "a {guard_state:?} guard must read {category:?}"
            );
        }
    }

    /// Glyph priority is *defined*, not last-writer-wins (§11.3): an entity always
    /// wins over the terrain beneath it, and the player wins over a guard. The old
    /// bug rendered a guard-in-a-doorway arbitrarily; here the order is fixed.
    #[test]
    fn entities_win_over_terrain_and_the_player_wins_over_a_guard() {
        // A guard standing on a console ($, terrain) renders as the guard, not the $.
        // The player faces south so the contested cell is live, not fogged (§11.5a).
        let s = State::new(
            open_room(10, 10),
            Cell::new(2, 2),
            Direction::South,
            vec![Guard::stationary(Cell::new(5, 5))],
            [Cell::new(5, 5)], // an objective stamps a console under the guard
            Cell::new(8, 8),
        );
        let g = render(&s);
        assert_eq!(g.get(5, 5).glyph, 'g', "entity draws over terrain");

        // Player and a guard on the same cell: the player wins.
        let both = state(
            10,
            10,
            Cell::new(4, 4),
            vec![Guard::stationary(Cell::new(4, 4))],
        );
        assert_eq!(render(&both).get(4, 4).glyph, '@', "player outranks guard");
    }

    /// §10.3/§11.3: the occupied cupboard is the "you are hidden here" signal. An
    /// empty hideout stays a System `}`; the one the player is concealed in keeps the
    /// `}` glyph but recolours to **Owned** — the `@` is not drawn, the cupboard is.
    #[test]
    fn an_occupied_hideout_recolours_to_owned_and_an_empty_one_stays_system() {
        let mut layout = open_room(10, 10);
        layout.place(Cell::new(4, 4), Terrain::Hideout); // the one the player hides in
        layout.place(Cell::new(7, 4), Terrain::Hideout); // an empty cupboard elsewhere
        let s = State::new(
            layout,
            Cell::new(4, 4),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(8, 8),
        );
        let g = render(&s);

        let occupied = g.get(4, 4);
        assert_eq!(occupied.glyph, '}', "the cupboard glyph, not the @");
        assert_eq!(occupied.fg, Category::Owned, "occupied recolours to Owned");

        let empty = g.get(7, 4);
        assert_eq!(empty.glyph, '}');
        assert_eq!(empty.fg, Category::System, "an empty cupboard stays System");
    }

    /// §10.3/§11.3: the crouch borrows the cupboard's vocabulary — **Owned = what
    /// is concealing you**. While the player is crouched, the covering *run* —
    /// the whole bench, not just the bumped table — keeps its `π` glyphs but
    /// recolours to Owned; standing back up returns it to System furniture. The
    /// `@` stays drawn — the player is beside the bench, not inside it.
    #[test]
    fn a_covering_bench_recolours_to_owned_while_crouched() {
        let mut layout = open_room(10, 10);
        layout.place(Cell::new(5, 4), Terrain::PartialCover);
        layout.place(Cell::new(5, 5), Terrain::PartialCover); // a two-table bench
        let mut s = State::new(
            layout,
            Cell::new(4, 4),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(8, 8),
        );

        // Standing: the bench is plain System furniture.
        let table = render(&s).get(5, 4);
        assert_eq!((table.glyph, table.fg), ('π', Category::System));

        s.step(Input::Step(Direction::East)); // bump a table: crouch (§10.3)
        let g = render(&s);
        for y in [4, 5] {
            let table = g.get(5, y);
            assert_eq!(
                (table.glyph, table.fg),
                ('π', Category::Owned),
                "the whole covering bench recolours while crouched"
            );
        }
        assert_eq!(g.get(4, 4).glyph, '@', "the player stays drawn beside it");

        s.step(Input::Step(Direction::West)); // step away: stand up
        let table = render(&s).get(5, 4);
        assert_eq!(table.fg, Category::System, "standing returns it to System");
    }

    /// §11.5's promise kept under the crouch: **red under you = detected.** A
    /// visible guard looking across a table paints its cone — the table included —
    /// but spares the cell of a player concealed from it; the moment the player
    /// stands, their cell paints red again.
    #[test]
    fn the_danger_overlay_spares_a_concealed_player() {
        // Guard at (5,3) looking south (spawn facing, §7.1) straight down the
        // column; a table at (5,6); the player one south of it at (5,7), facing
        // north so the guard is in view.
        let mut layout = open_room(12, 12);
        layout.place(Cell::new(5, 6), Terrain::PartialCover);
        let mut s = State::new(
            layout,
            Cell::new(5, 7),
            Direction::North,
            vec![Guard::stationary(Cell::new(5, 3))],
            Vec::new(),
            Cell::new(10, 10),
        );
        let cone = s.guards()[0].fov();
        assert!(
            cone.contains(Cell::new(5, 7)),
            "sight passes over the table"
        );

        // Standing: watched, and painted so.
        assert_eq!(render(&s).get(5, 7).bg, Some(Category::Danger));

        // Crouched: concealed from this guard — the player's cell is spared while
        // the table and the rest of the cone stay red.
        s.step(Input::Step(Direction::North)); // bump the table: crouch
        let g = render(&s);
        assert_eq!(g.get(5, 7).bg, None, "a concealed player's cell is not red");
        assert_eq!(
            g.get(5, 6).bg,
            Some(Category::Danger),
            "the table stays watched"
        );
        assert_eq!(
            g.get(5, 5).bg,
            Some(Category::Danger),
            "so does the open cone"
        );
    }

    /// §11.5a: a table is **geometry** — it replaced a stamped wall (§10.1a), so
    /// like a wall it draws from turn one, dimmed beyond the FOV, never masked.
    #[test]
    fn a_table_is_geometry_and_never_fogged() {
        let mut layout = open_room(20, 20);
        layout.place(Cell::new(10, 14), Terrain::PartialCover); // behind the spawn facing
        let s = State::new(
            layout,
            Cell::new(10, 10),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(18, 18),
        );
        let cell = render(&s).get(10, 14);
        assert_eq!(
            (cell.glyph, cell.fg, cell.vis),
            ('π', Category::System, Visibility::Dimmed),
            "an out-of-FOV table still draws, dimmed"
        );
    }

    /// Terrain categories follow §11.2: an exit and a console are Interest, a hideout
    /// and a door are System, walls are Neutral.
    #[test]
    fn terrain_carries_its_category() {
        assert_eq!(Terrain::Wall.category(), Category::Neutral);
        assert_eq!(Terrain::Floor.category(), Category::Ground);
        assert_eq!(Terrain::DoorPanelOpen.category(), Category::Ground);
        assert_eq!(Terrain::Exit.category(), Category::Interest);
        assert_eq!(Terrain::Console.category(), Category::Interest);
        assert_eq!(Terrain::Hideout.category(), Category::System);
        assert_eq!(Terrain::DoorPanelClosed.category(), Category::System);
    }

    /// §11.5a: **geometry is never fogged.** Walls far beyond sight range — and the
    /// exit, part of the layout the player entered by — draw from turn one, so a
    /// route can be planned before the first risky step. Out-of-FOV geometry
    /// carries [`Visibility::Dimmed`]; what the player sees now is `Live`.
    #[test]
    fn geometry_draws_from_turn_one_even_far_out_of_sight() {
        let mut layout = open_room(40, 30);
        layout.place(Cell::new(35, 5), Terrain::Exit); // far outside the FOV
        let s = State::new(
            layout,
            Cell::new(2, 2),
            Direction::South,
            Vec::new(),
            Vec::new(),
            Cell::new(35, 5),
        );
        let g = render(&s);

        // The far corner wall is way outside the 15-range box, yet drawn.
        let far_wall = g.get(39, 29);
        assert_eq!(far_wall.glyph, '#');
        assert_eq!(far_wall.vis, Visibility::Dimmed);
        // So is the exit: geometry, not a hidden content.
        let exit = g.get(35, 5);
        assert_eq!(exit.glyph, 'E');
        assert_eq!(exit.fg, Category::Interest);
        assert_eq!(exit.vis, Visibility::Dimmed);
        // What is in the FOV right now is live.
        assert_eq!(g.get(2, 4).vis, Visibility::Live);
    }

    /// The §11.5a golden test: an unseen intel is invisible (its cell reads as
    /// plain floor); after entering the FOV it is live; after leaving it stays,
    /// **remembered** — its own visual state — while a guard, live state, does not
    /// persist out of the FOV. The guard is placed **out of the guard-sense box** too
    /// (§9), so "not drawn" means neither seen nor sensed — isolating the memory rule
    /// from the sense (which is exercised in its own tests).
    #[test]
    fn contents_are_remembered_but_live_state_is_not() {
        // Player at (10,10) facing north; a console four cells behind (out of the
        // half-disc) and a guard far to the south — 14 cells off, past the 10-box, so
        // out of range entirely until the player faces it and closes in.
        let guard = Cell::new(10, 24);
        let mut s = State::new(
            open_room(40, 40),
            Cell::new(10, 10),
            Direction::North,
            vec![Guard::stationary(guard)],
            [Cell::new(10, 14)],
            Cell::new(38, 38),
        );

        // Never seen and out of sense range: the intel masks as plain floor and the
        // guard is not drawn at all.
        let g = render(&s);
        assert_eq!(g.get(10, 14).glyph, '·', "unseen intel is invisible");
        assert_eq!(
            g.get(10, 14).fg,
            Category::Ground,
            "…its cell reads as floor"
        );
        assert_eq!(
            g.get(guard.x, guard.y).glyph,
            '·',
            "an out-of-range guard is not drawn",
        );

        // Turn south: both enter the FOV, live.
        s.step(Input::Step(Direction::South)); // to (10,11), facing south
        let g = render(&s);
        let intel = g.get(10, 14);
        assert_eq!(
            (intel.glyph, intel.fg, intel.vis),
            ('$', Category::Interest, Visibility::Live)
        );
        let g_cell = g.get(guard.x, guard.y);
        assert_eq!((g_cell.glyph, g_cell.vis), ('g', Visibility::Live));

        // Turn back north: the intel stays, remembered; the guard vanishes (it is not
        // remembered, and out of range it is not sensed either).
        s.step(Input::Step(Direction::North)); // to (10,10), facing north
        let g = render(&s);
        let intel = g.get(10, 14);
        assert_eq!(
            (intel.glyph, intel.fg, intel.vis),
            ('$', Category::Interest, Visibility::Remembered),
            "seen intel stays on the map after leaving the FOV, as memory"
        );
        assert_eq!(
            g.get(guard.x, guard.y).glyph,
            '·',
            "a guard does not persist out of FOV",
        );
        assert_eq!(g.get(guard.x, guard.y).vis, Visibility::Dimmed);
    }

    /// §11.5a's scouting reward: an unscouted hideout reads as plain **wall** — the
    /// alcove gives nothing away until the player has actually seen it. Once seen
    /// it is remembered like any content.
    #[test]
    fn an_unseen_hideout_masks_as_wall_until_scouted() {
        let mut layout = open_room(20, 20);
        layout.place(Cell::new(10, 14), Terrain::Hideout); // behind the spawn facing
        let mut s = State::new(
            layout,
            Cell::new(10, 10),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(18, 18),
        );

        let cell = render(&s).get(10, 14);
        assert_eq!(
            (cell.glyph, cell.fg, cell.vis),
            ('#', Category::Neutral, Visibility::Dimmed),
            "an unscouted hideout reads as plain wall"
        );

        s.step(Input::Step(Direction::South)); // face it: live
        let cell = render(&s).get(10, 14);
        assert_eq!(
            (cell.glyph, cell.fg, cell.vis),
            ('}', Category::System, Visibility::Live)
        );

        s.step(Input::Step(Direction::North)); // leave: remembered
        let cell = render(&s).get(10, 14);
        assert_eq!(
            (cell.glyph, cell.fg, cell.vis),
            ('}', Category::System, Visibility::Remembered)
        );
    }

    /// §11.5a: a door's **position** is geometry but its open/closed pose is live
    /// state — out of the FOV a panel draws canonically closed, *even after the
    /// player has seen it open*. Memory holds contents, never state.
    #[test]
    fn a_doors_pose_is_live_state_never_remembered() {
        let mut layout = open_room(20, 20);
        layout.place(Cell::new(10, 14), Terrain::DoorPanelOpen);
        let mut s = State::new(
            layout,
            Cell::new(10, 10),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(18, 18),
        );

        // Out of the FOV: the actually-open panel draws in its closed pose.
        let cell = render(&s).get(10, 14);
        assert_eq!(
            (cell.glyph, cell.fg, cell.vis),
            ('+', Category::System, Visibility::Dimmed),
            "an unseen door always shows the canonical closed pose"
        );

        // In the FOV: the true, live pose — open, blank.
        s.step(Input::Step(Direction::South));
        let cell = render(&s).get(10, 14);
        assert_eq!((cell.glyph, cell.vis), (' ', Visibility::Live));

        // Look away again: back to the closed pose, not a remembered open one —
        // the cell is in tile memory now, but a pose is not a content.
        s.step(Input::Step(Direction::North));
        let cell = render(&s).get(10, 14);
        assert_eq!(
            (cell.glyph, cell.vis),
            ('+', Visibility::Dimmed),
            "door state is never remembered (§11.5a)"
        );
    }

    /// §11.5 fix #2: **floor renders as dots**, in and out of the FOV alike, so
    /// the sight boundary reads across open ground and not just where it crosses
    /// a wall. An open door panel stays blank (§10.3) — the gap is its rendering.
    #[test]
    fn floor_renders_as_dots_but_an_open_panel_stays_blank() {
        let mut layout = open_room(20, 20);
        layout.place(Cell::new(12, 8), Terrain::DoorPanelOpen);
        let s = State::new(
            layout,
            Cell::new(10, 10),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(18, 18),
        );
        let g = render(&s);

        let lit = g.get(10, 8); // ahead: floor in the FOV
        assert_eq!((lit.glyph, lit.vis), ('·', Visibility::Live));
        let dark = g.get(10, 14); // behind: floor out of the FOV
        assert_eq!((dark.glyph, dark.vis), ('·', Visibility::Dimmed));
        assert_eq!(g.get(12, 8).glyph, ' ', "an open panel renders blank");
    }

    /// The §11.4 golden test, whole screen: the always-on ability line on top, the
    /// map, then the near and usable lines — one grid, printed as text. The header
    /// carries the compact ability readout and the closed deploy button; with the
    /// panel not deployed the map is untouched, the near line rests on ambient
    /// floor, and the usable line offers the adjacent console.
    #[test]
    fn the_full_screen_renders_golden() {
        let s = State::new(
            open_room(24, 6),
            Cell::new(2, 2),
            Direction::North,
            Vec::new(),
            [Cell::new(3, 2)], // a console east of the player
            Cell::new(22, 4),
        );
        let text = render_screen(&s, ScreenUi::default()).to_text();
        // Row 0 is the always-on ability line: on a fresh run every economy ability
        // is ready, so the compact keys are the four bare hotkeys, deploy button
        // (closed chevron) right. (Its exact glyphs are pinned in the ability-line
        // test; here we assert its shape without pasting the chevron.)
        assert!(
            text[0].starts_with(" r c d x"),
            "the always-on ability line: {:?}",
            text[0]
        );
        assert!(
            text[0].trim_end().ends_with("[▾]"),
            "the closed deploy button: {:?}",
            text[0]
        );
        // Below it, the map and the two status lines — the panel is not deployed,
        // so the board is whole.
        assert_eq!(
            text[1..].to_vec(),
            vec![
                "########################".to_string(),
                "#······················#".to_string(),
                "#·@$···················#".to_string(),
                "#······················#".to_string(),
                "#·····················E#".to_string(),
                "########################".to_string(),
                " intel remaining: 1     ".to_string(),
                " → console: take intel  ".to_string(),
            ]
        );
    }

    /// The screen is the map plus the header and status rows, same width — and the
    /// two status rows carry their §11.4 styling: the near line is a full-width
    /// band in the message's category with Neutral words on top; the usable line
    /// has no band and speaks each affordance's own category.
    #[test]
    fn status_rows_carry_the_band_and_the_categories() {
        let mut s = State::new(
            open_room(24, 6),
            Cell::new(2, 2),
            Direction::North,
            Vec::new(),
            [Cell::new(3, 2)],
            Cell::new(22, 4),
        );
        let map = render(&s);
        let g = render_screen(&s, ScreenUi::default());
        assert_eq!(g.width(), map.width());
        assert_eq!(g.height(), HEADER_ROWS + map.height() + STATUS_ROWS);

        let near_y = HEADER_ROWS + map.height();
        let usable_y = near_y + 1;
        for x in 0..g.width() {
            let cell = g.get(x, near_y);
            assert_eq!(cell.bg, Some(Category::Interest), "the band spans the row");
            assert_eq!(cell.vis, Visibility::Live);
            if cell.glyph != ' ' {
                assert_eq!(cell.fg, Category::Neutral, "words read Neutral on the band");
            }
            assert_eq!(g.get(x, usable_y).bg, None, "the usable line has no band");
        }
        // The affordance leads with its bump direction and speaks its own
        // category: `→ console: take intel` is Interest (§11.2 — goals and
        // rewards), and the console is east of the player.
        assert_eq!(g.get(1, usable_y).glyph, '→');
        assert_eq!(g.get(1, usable_y).fg, Category::Interest);
        assert_eq!(g.get(3, usable_y).glyph, 'c');

        // A threat message flips the whole band to its category: get captured
        // and the near line reads Danger — the colour flash before the words.
        s = State::new(
            open_room(24, 6),
            Cell::new(2, 2),
            Direction::North,
            vec![Guard::patrolling_to(Cell::new(2, 4), Cell::new(2, 1))],
            Vec::new(),
            Cell::new(22, 4),
        );
        s.step(Input::Wait); // the guard steps north into the player: caught
        let g = render_screen(&s, ScreenUi::default());
        assert_eq!(g.get(0, near_y).bg, Some(Category::Danger));
        assert_eq!(g.get(1, near_y).glyph, 'c'); // "caught"
    }

    /// The permanent home of ability state (§11.4): the **always-on ability line**
    /// on row 0, assembled from the run's real economy ([`State::ability_statuses`]).
    /// A fresh run has every ability ready, so the line is the four economy keys in
    /// deck order, each the bare §11.6 hotkey in Owned — and the two bump verbs
    /// (Takedown `t`, Drag `g`) are **not** on it: they live on the usable line, not
    /// the ability economy (§7.2/§8.3).
    #[test]
    fn the_always_on_line_shows_every_economy_ability() {
        use crate::input::ability_hotkey;

        let s = State::new(
            open_room(30, 10),
            Cell::new(15, 5),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(28, 8),
        );
        let g = render_screen(&s, ScreenUi::default());

        // The four economy abilities laid left to right from the one-cell margin,
        // each ready → the bare key in Owned, each key its settled §11.6 hotkey.
        for (col, name, glyph) in [
            (1, "Run", 'r'),
            (3, "Camouflage", 'c'),
            (5, "Decoy", 'd'),
            (7, "Dephase", 'x'),
        ] {
            assert_eq!(g.get(col, 0).glyph, glyph, "{name} at col {col}");
            assert_eq!(g.get(col, 0).fg, Category::Owned, "{name} ready colour");
            assert_eq!(Some(glyph), ability_hotkey(name), "{name} hotkey");
        }
        // The bump verbs never appear on the ability line.
        let row0: String = (0..g.width()).map(|x| g.get(x, 0).glyph).collect();
        assert!(!row0.contains('t'), "Takedown is not an economy ability");
        assert!(!row0.contains('g'), "Drag is not an economy ability");

        // The deploy button, closed, right-aligned — and `is_ability_button` agrees
        // with where it is drawn.
        let start = 30 - 1 - 3;
        assert!(is_ability_button(30, start, 0));
        assert!(
            !is_ability_button(30, start - 1, 0),
            "just left is not the button"
        );
        assert!(
            !is_ability_button(30, start, 1),
            "row 1 is the map, not the button"
        );
        assert_eq!(g.get(start, 0).glyph, '[');
        assert_eq!(g.get(start, 0).fg, Category::System);
    }

    /// The line's live states (§11.4): an **active** ability tucks its `[n]` against
    /// the key in Owned, a **cooling** one its `/n/` in System — the exact numbers
    /// the economy hands over (§8.2). Driven to Run cooling and Camouflage active,
    /// with Decoy and Dephase still ready, so all three notations show at once.
    #[test]
    fn the_line_shows_active_and_cooling_state() {
        let mut s = State::new(
            open_room(30, 10),
            Cell::new(15, 5),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(28, 8),
        );
        // Run: activate (Active 4 after the turn's tick) then toggle off — a free
        // action that drops it straight into its full 12 cooldown. Then activate
        // Camouflage: that turn's tick drains Run's cooldown to 11 and leaves
        // Camouflage active with 9 of its 10 left.
        s.step(Input::Activate(AbilityId::Run));
        s.step(Input::Deactivate(AbilityId::Run));
        s.step(Input::Activate(AbilityId::Camouflage));
        assert_eq!(
            s.ability_state(AbilityId::Run),
            AbilityState::Cooling { remaining: 11 }
        );
        assert_eq!(
            s.ability_state(AbilityId::Camouflage),
            AbilityState::Active { remaining: 9 }
        );

        let g = render_screen(&s, ScreenUi::default());
        let row0: String = (0..g.width()).map(|x| g.get(x, 0).glyph).collect();
        // `r/11/` cooling (System), `c[9]` active (Owned), then the two ready keys.
        assert!(
            row0.starts_with(" r/11/ c[9] d x"),
            "the live ability line: {row0:?}"
        );
        assert_eq!(g.get(1, 0).glyph, 'r');
        assert_eq!(g.get(1, 0).fg, Category::System, "cooling reads System");
        assert_eq!(g.get(2, 0).glyph, '/', "cooling shows /N/");
        assert_eq!(g.get(7, 0).glyph, 'c');
        assert_eq!(g.get(7, 0).fg, Category::Owned, "active reads Owned");
        assert_eq!(g.get(8, 0).glyph, '[', "active shows [N]");
    }

    /// Deploying the panel (§11.4) overlays the named ability list in the corner
    /// **opposite the player**, so it never covers where the action is — and it is
    /// gone the moment the panel is not deployed. The corner tracks the player: a
    /// player top-left puts the panel bottom-right, and moving to the bottom-right
    /// flips it top-left.
    #[test]
    fn deploying_shows_the_panel_opposite_the_player() {
        // Player top-left → panel bottom-right. On a fresh run the widest label is
        // `c Camouflage` (12) → a 13-wide band, four rows: map origin (16,9), so the
        // first row sits at screen (16,10) (map row + the header).
        let s = State::new(
            open_room(30, 14),
            Cell::new(5, 5),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(28, 12),
        );
        let closed = render_screen(&s, ScreenUi::default());
        let open = render_screen(
            &s,
            ScreenUi {
                ability_panel_open: true,
            },
        );
        // Closed: that corner is plain map (interior floor). Open: the panel's first
        // row `r Run` starts there, in Owned.
        assert_eq!(
            closed.get(16, 10).glyph,
            '·',
            "not deployed: the board is whole"
        );
        assert_eq!(
            open.get(16, 10).glyph,
            'r',
            "deployed: panel opposite the player"
        );
        assert_eq!(open.get(18, 10).glyph, 'R', "…the label reads `r Run`");
        assert_eq!(open.get(16, 10).fg, Category::Owned);
        // The far side (near the player, top-left) stays board even when deployed.
        assert_eq!(
            open.get(2, 2).glyph,
            '·',
            "the panel never covers the player's corner"
        );

        // Player bottom-right → panel flips to the top-left corner (map origin
        // (1,1), screen row 2).
        let s2 = State::new(
            open_room(30, 14),
            Cell::new(24, 11),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(1, 1),
        );
        let open2 = render_screen(
            &s2,
            ScreenUi {
                ability_panel_open: true,
            },
        );
        assert_eq!(open2.get(1, 2).glyph, 'r', "the corner tracks the player");
    }

    /// The deployed panel clamps to a board too small to hold every row rather than
    /// panicking — only hand-built states get this small (the v1 board is 40×40),
    /// but the renderer must never index off the grid.
    #[test]
    fn the_deployed_panel_clamps_on_a_tiny_board() {
        let s = State::new(
            open_room(24, 4),
            Cell::new(2, 2),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(22, 2),
        );
        // A 4-tall map cannot fit all four panel rows within its inset; the render
        // shows what fits and stops — no panic, and the screen height is intact.
        let g = render_screen(
            &s,
            ScreenUi {
                ability_panel_open: true,
            },
        );
        assert_eq!(g.height(), HEADER_ROWS + 4 + STATUS_ROWS);
        // Player top-left → panel top-right; its first row draws at map (10,1),
        // screen (10,2).
        assert_eq!(g.get(10, 2).glyph, 'r', "the first row still draws");
    }

    /// The pointer→identity hit-test (§11.4) on the always-on line: each compact
    /// entry's cells resolve to *that* ability by identity, the gaps and the deploy
    /// button resolve to nothing (a tap there toggles the panel, it never falls
    /// through to an activation), and the map below is not the line.
    #[test]
    fn ability_at_resolves_the_compact_line() {
        let s = State::new(
            open_room(30, 10),
            Cell::new(15, 5),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(28, 8),
        );
        let ui = ScreenUi::default();

        // r@1 c@3 d@5 x@7 (all ready → one cell each), by identity not position.
        for (col, id) in [
            (1, AbilityId::Run),
            (3, AbilityId::Camouflage),
            (5, AbilityId::Decoy),
            (7, AbilityId::Dephase),
        ] {
            assert_eq!(ability_at(&s, ui, col, 0), Some(id), "col {col}");
        }
        // The space between entries is no ability.
        assert_eq!(
            ability_at(&s, ui, 2, 0),
            None,
            "the gap resolves to nothing"
        );
        // The deploy button is never an ability, even though it is on row 0 — the
        // line stops before it, so a tap there cannot fall through to an activation.
        let start = 30 - 1 - 3;
        assert!(is_ability_button(30, start, 0));
        assert_eq!(
            ability_at(&s, ui, start, 0),
            None,
            "the button is not an ability"
        );
        // The map below the header is not the line while the panel is closed.
        assert_eq!(ability_at(&s, ui, 1, 1), None, "row 1 is the map");
    }

    /// The hit-test on the **deployed panel** (§11.4): its rows, overlaid on the map
    /// beneath the header, resolve by identity to the ability they draw; cells off
    /// the band are nothing; and with the panel closed the same cells are just map.
    #[test]
    fn ability_at_resolves_the_deployed_panel() {
        // Same geometry as `deploying_shows_the_panel_opposite_the_player`: a fresh
        // run, player top-left, panel at map origin (16,9) → screen rows from 10.
        let s = State::new(
            open_room(30, 14),
            Cell::new(5, 5),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(28, 12),
        );
        let open = ScreenUi {
            ability_panel_open: true,
        };

        // One panel row per economy ability, top to bottom in deck order.
        for (screen_y, id) in [
            (10, AbilityId::Run),
            (11, AbilityId::Camouflage),
            (12, AbilityId::Decoy),
            (13, AbilityId::Dephase),
        ] {
            assert_eq!(
                ability_at(&s, open, 16, screen_y),
                Some(id),
                "row at y {screen_y}"
            );
        }
        // A cell left of the band is not the panel; nor is it while the panel closes.
        assert_eq!(ability_at(&s, open, 2, 10), None, "off the band");
        assert_eq!(
            ability_at(&s, ScreenUi::default(), 16, 10),
            None,
            "closed: the panel is not hit-testable"
        );
    }

    /// The click **is** the hotkey (§11.4/§11.6): the id a line cell resolves to is
    /// the very id its §11.6 shortcut fires, and firing it drives the one
    /// `Input::Activate` path — so a click activates a ready ability and, on a
    /// cooling one, refuses for free with no turn spent (§4.4), exactly as the key.
    #[test]
    fn a_click_activates_by_the_same_path_as_the_hotkey() {
        use crate::input::ability_input_for_key;

        let mut s = State::new(
            open_room(30, 10),
            Cell::new(15, 5),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(28, 8),
        );
        let ui = ScreenUi::default();

        // The line's Run cell resolves to the same id `r` fires — one path, by identity.
        let clicked = ability_at(&s, ui, 1, 0).expect("Run under the pointer");
        assert_eq!(
            ability_input_for_key("r"),
            Some(Input::Activate(clicked)),
            "the click and the shortcut resolve to the same activation",
        );

        // A click on a ready ability activates it (a spent turn).
        let events = s.step(Input::Activate(clicked));
        assert_eq!(s.turn(), 1, "activating spends the turn");
        assert!(!events.is_empty(), "the ability activated");

        // Drive Run to cooling, then a click on its (now cooling) entry refuses
        // cleanly: the same `Input::Activate` is a free no-op — no turn, no change.
        s.step(Input::Deactivate(AbilityId::Run));
        assert!(matches!(
            s.ability_state(AbilityId::Run),
            AbilityState::Cooling { .. }
        ));
        let cooling = ability_at(&s, ui, 1, 0).expect("Run still under the pointer");
        let turn_before = s.turn();
        let refused = s.step(Input::Activate(cooling));
        assert!(refused.is_empty(), "a cooling entry refuses");
        assert_eq!(s.turn(), turn_before, "the mis-click spends no turn");
    }

    /// A message longer than the row truncates at the edge instead of
    /// panicking or wrapping — the status rows are single grid rows.
    #[test]
    fn a_long_status_line_truncates_at_the_edge() {
        let mut s = State::new(
            open_room(12, 6),
            Cell::new(2, 2),
            Direction::North,
            Vec::new(),
            [Cell::new(3, 2)],
            Cell::new(10, 4),
        );
        s.step(Input::Step(Direction::East)); // take the intel: a long message
        let g = render_screen(&s, ScreenUi::default());
        let near_y = HEADER_ROWS + 6; // header + map height
        let near: String = (0..g.width()).map(|x| g.get(x, near_y).glyph).collect();
        assert_eq!(near.chars().count(), 12, "exactly one grid row wide");
        assert!(
            near.starts_with(" intel in h"),
            "the words run to the edge and stop: {near:?}"
        );
    }

    /// The §11.5 golden test: a guard cone the player can see paints the expected
    /// red set — `Danger` backgrounds on exactly the watched cells, including the
    /// player's own when they stand in it (the lose condition, painted), and
    /// nothing anywhere else.
    #[test]
    fn the_danger_overlay_paints_a_visible_guards_cone() {
        // Player at (10,10) facing north; guard adjacent at (9,9) — in the FOV —
        // looking south (spawn facing, §7.1), its wedge over the player's cell.
        let s = State::new(
            open_room(20, 20),
            Cell::new(10, 10),
            Direction::North,
            vec![Guard::stationary(Cell::new(9, 9))],
            Vec::new(),
            Cell::new(18, 18),
        );
        let g = render(&s);
        let guard_fov = s.guards()[0].fov();

        // Straight down the wedge: watched, red.
        assert!(guard_fov.contains(Cell::new(9, 11)));
        assert_eq!(g.get(9, 11).bg, Some(Category::Danger));
        // The player's own cell is watched: red under the `@`.
        assert!(guard_fov.contains(Cell::new(10, 10)));
        assert_eq!(g.get(10, 10).bg, Some(Category::Danger));
        assert_eq!(g.get(10, 10).glyph, '@');
        // The painted set is *exactly* the cone: every cell's background agrees
        // with the same detection data the AI reads.
        for y in 0..g.height() {
            for x in 0..g.width() {
                let expected = guard_fov.contains(Cell::new(x, y));
                assert_eq!(
                    g.get(x, y).bg.is_some(),
                    expected,
                    "bg at ({x},{y}) must mirror the guard's cone"
                );
            }
        }
    }

    /// §11.5 fix #1: a **watched-but-unseen** cell must not look safe. A visible
    /// guard's cone is knowledge the player has, so it paints red even where it
    /// reaches outside the player's own FOV — over a dimmed glyph, not dark-on-dark
    /// nothing.
    #[test]
    fn watched_cells_outside_the_players_fov_still_paint_red() {
        // Guard at (9,9), visible in the ring, looking south: its wedge runs down
        // *behind* the north-facing player, outside their half-disc.
        let s = State::new(
            open_room(20, 20),
            Cell::new(10, 10),
            Direction::North,
            vec![Guard::stationary(Cell::new(9, 9))],
            Vec::new(),
            Cell::new(18, 18),
        );
        let watched_unseen = Cell::new(9, 13);
        assert!(s.guards()[0].fov().contains(watched_unseen), "in the cone");
        assert!(!s.player_fov().contains(watched_unseen), "not in the FOV");

        let cell = render(&s).get(9, 13);
        assert_eq!(cell.bg, Some(Category::Danger), "red even though unseen");
        assert_eq!(
            (cell.glyph, cell.vis),
            ('·', Visibility::Dimmed),
            "the glyph below stays the dimmed geometry"
        );
    }

    /// The flip side of the overlay's honesty: a guard the player **cannot see**
    /// paints no **danger** overlay. Its cone is unknown information — painting it
    /// would leak what the player has not scouted ("no guard *you can see* will detect
    /// you"). Its *position* may still show as a sensed marker (§9.2), but that is the
    /// orange highlight on its one cell — never the red cone.
    #[test]
    fn an_unseen_guards_cone_paints_no_danger() {
        // The guard stands behind the north-facing player, out of the FOV — but within
        // the sense box, so its cell carries the sensed marker while its cone does not.
        let guard = Cell::new(10, 14);
        let s = State::new(
            open_room(20, 20),
            Cell::new(10, 10),
            Direction::North,
            vec![Guard::stationary(guard)],
            Vec::new(),
            Cell::new(18, 18),
        );
        assert!(!s.player_fov().contains(guard));

        let g = render(&s);
        for y in 0..g.height() {
            for x in 0..g.width() {
                assert_ne!(
                    g.get(x, y).bg,
                    Some(Category::Danger),
                    "no red danger anywhere for ({x},{y})",
                );
            }
        }
        // The only background painted is the sensed guard's own orange marker.
        assert_eq!(g.get(guard.x, guard.y).bg, Some(Category::Sensed));
    }

    /// §9.2/§11.3: a guard **sensed** through a wall paints an orange
    /// `Category::Sensed` **background** on its exact cell — no glyph of its own, no
    /// facing, no cone, and no danger overlay. The underlying geometry glyph shows
    /// through, highlighted; nothing anywhere reads danger, because knowing where a
    /// guard is is not knowing whether it can see you.
    #[test]
    fn a_sensed_guard_paints_an_orange_background_no_cone() {
        // Player at (10,10) facing north; a guard behind them at (10,14) — out of the
        // half-disc, four cells away, so inside the 10-box: sensed, not seen.
        let s = State::new(
            open_room(20, 20),
            Cell::new(10, 10),
            Direction::North,
            vec![Guard::stationary(Cell::new(10, 14))],
            Vec::new(),
            Cell::new(18, 18),
        );
        assert!(
            !s.player_fov().contains(Cell::new(10, 14)),
            "not in the FOV"
        );
        assert_eq!(
            s.perceive_guard(&s.guards()[0]),
            Some(GuardPerception::Sensed),
        );

        let g = render(&s);
        let cell = g.get(10, 14);
        assert_eq!(
            cell.bg,
            Some(Category::Sensed),
            "an orange highlight on the cell"
        );
        // The glyph is the geometry the cell masks as (dimmed floor here), *not* a
        // glyph of the guard's own — the sensed marker is a background, not a `g`.
        assert_eq!(
            cell.glyph, '·',
            "the geometry shows through, no guard glyph"
        );
        assert_eq!(
            cell.fg,
            Category::Ground,
            "…the glyph keeps its own category"
        );
        // A sensed guard projects no cone: nothing on the map reads danger.
        for y in 0..g.height() {
            for x in 0..g.width() {
                assert_ne!(
                    g.get(x, y).bg,
                    Some(Category::Danger),
                    "a sensed guard paints no danger overlay ({x},{y})",
                );
            }
        }
    }

    /// §9.2/§11.3: the sensed highlight **blooms** into the full guard as it crosses
    /// the FOV boundary. Behind the player it is a flat orange background with no
    /// overlay; the moment the player faces it — same guard, same cell — it becomes
    /// the state-coloured `g` and its cone paints the danger overlay.
    #[test]
    fn a_sensed_highlight_blooms_to_a_seen_guard_across_the_fov_boundary() {
        let guard = Cell::new(10, 14);
        let mut s = State::new(
            open_room(20, 20),
            Cell::new(10, 10),
            Direction::North,
            vec![Guard::stationary(guard)],
            Vec::new(),
            Cell::new(18, 18),
        );

        // North-facing: the guard is behind, only sensed — an orange cell, no `g`, no
        // danger overlay anywhere.
        let g = render(&s);
        assert_eq!(g.get(guard.x, guard.y).bg, Some(Category::Sensed));
        assert_ne!(g.get(guard.x, guard.y).glyph, 'g', "no guard glyph yet");
        let no_red = (0..g.height())
            .all(|y| (0..g.width()).all(|x| g.get(x, y).bg != Some(Category::Danger)));
        assert!(no_red, "sensed: no cone painted");

        // Turn to face it (step south): now seen — the full state-coloured guard, and
        // its cone paints the danger overlay somewhere.
        s.step(Input::Step(Direction::South)); // player to (10,11), facing south
        assert_eq!(
            s.perceive_guard(&s.guards()[0]),
            Some(GuardPerception::Seen),
        );
        let g = render(&s);
        let cell = g.get(guard.x, guard.y);
        assert_eq!(cell.glyph, 'g', "the highlight bloomed into the guard");
        assert_eq!(
            cell.fg,
            s.guards()[0].state().category(),
            "…in its state colour",
        );
        let some_red = (0..g.height())
            .any(|y| (0..g.width()).any(|x| g.get(x, y).bg == Some(Category::Danger)));
        assert!(some_red, "seen: the guard's cone now paints the overlay");
    }

    /// §11.5a: a guard neither seen nor sensed — out of both the FOV and the
    /// guard-sense box — draws **nothing** live. Its cell falls back to the geometry
    /// in its place (dimmed floor), with no highlight and no memory of a guard there.
    #[test]
    fn an_out_of_range_guard_draws_nothing() {
        // Player at (5,5) facing north; a guard far to the south-east, out of the FOV
        // and well past the 10-box (Chebyshev 12).
        let guard = Cell::new(17, 17);
        let s = State::new(
            open_room(24, 24),
            Cell::new(5, 5),
            Direction::North,
            vec![Guard::stationary(guard)],
            Vec::new(),
            Cell::new(22, 22),
        );
        assert_eq!(
            s.perceive_guard(&s.guards()[0]),
            None,
            "out of range entirely"
        );

        let cell = render(&s).get(guard.x, guard.y);
        assert_eq!(cell.glyph, '·', "the guard's cell is just dimmed floor");
        assert_eq!(cell.fg, Category::Ground, "…not a sensed highlight");
        assert_eq!(cell.bg, None, "…and no orange background");
        assert_eq!(cell.vis, Visibility::Dimmed);
    }
}
