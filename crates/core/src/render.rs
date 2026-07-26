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

use crate::category::Category;
use crate::cell::Cell;
use crate::facility::{Facility, Terrain};
use crate::state::{GuardPerception, State};

/// The entity glyphs (§11.3), named once so the world render and the help legend
/// (#139) draw the same characters — a legend that hand-copied them could drift from
/// what the game shows. Terrain glyphs already have their single source in
/// [`Terrain::glyph`]; these are the entity half of the §11.3 table.
pub(crate) const PLAYER_GLYPH: char = '@';
pub(crate) const GUARD_GLYPH: char = 'g';
pub(crate) const BODY_GLYPH: char = 'z';
/// Floor draws as a dot, not blank (§11.5): a glyph for the FOV dimming to act on
/// across open ground. Named so the legend shows the same mark the board does.
pub(crate) const FLOOR_DOT: char = '·';

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
/// once out of range. A door's *change* is sensed the same way, in the same
/// [`Category::Sensed`] channel, but at its own longer range (§9.4/§10.4): a door that
/// opens or shuts away from the player leaves a fading orange background over its
/// **whole footprint** — evidence someone passed, also position only and also painted
/// through walls.
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
/// # The effect layer (§8.3/§11.5, #308)
///
/// An **area effect** of the player's own making — Confusion's bubble today, Lockdown's
/// radius next — draws in `Category::Effect`, in two places and one meaning. Its
/// **footprint** washes the §6.1 box it reaches for the one frame of its flash, and
/// every guard it **holds** is marked for as long as it holds them: the seen guard's `g`
/// recolours out of the threat ladder, and a guard felt only through a wall takes the
/// mark on its sensed highlight, since it has no glyph to recolour. The precedence is
/// pinned by paint order — `Danger` > `Sensed` > the footprint — because an advisory
/// layer must never masquerade as the detection set, nor hide it (§11.5 **[SETTLED]**).
/// Both readings come from one query apiece on [`State`], keyed by the effect rather
/// than by any one ability, so the picture cannot disagree with what is frozen.
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
                FLOOR_DOT
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

    // A spent objective is Neutral scenery (§11.2): once its intel is taken a console
    // stops being a live goal, so it recolours from Interest to Neutral while keeping
    // its `$` glyph — "there was intel here, it's collected" — instead of staying
    // indistinguishable from a live console. Terrain stays `Console` (geometry is
    // static); only the category changes, so the core stays colour-blind (§11.2) and
    // the shell's one table owns the actual colour. Runs on the terrain layer, before
    // the entity/overlay passes, so a guard or the player standing on a spent console
    // still draws over it. Taking intel requires reaching (thus seeing) the console and
    // memory is monotonic (§11.5a), so a spent console is always at least remembered —
    // recolour only where it actually shows, both live and in memory, never a masked
    // floor dot standing in for a never-seen console.
    for cell in state.spent_consoles() {
        if fov.contains(cell) || memory.contains(cell) {
            cells[(cell.y * width + cell.x) as usize].fg = Category::Neutral;
        }
    }

    // The duct interior view (§11.5a/§10.7, #134). A duct's path is shown **only
    // while the player is crawling it**: the whole occupied run lights as one
    // connected `=` — the crawlspace read as a single space (the player's own cell is
    // overwritten by the `@` below; glyph priority `@` > `=`). The interior carries no
    // tell on the base map and is never remembered once the player climbs out
    // (§11.5a): the path lives in its own layer, so the shortcut's route is given away
    // to nobody. The two **entries** are the exception — they are geometry, drawn `=`
    // from turn one by the fog above whether occupied or not — so nothing here needs
    // to draw an unoccupied duct at all.
    if let Some(duct) = state.occupied_duct() {
        for &c in duct.cells() {
            cells[(c.y * width + c.x) as usize] = GlyphCell {
                glyph: '=',
                fg: Category::System,
                bg: None,
                vis: Visibility::Live,
            };
        }
    }

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
            put(decoy, PLAYER_GLYPH, Category::Owned);
        }
    }
    // A body (§7.2) is live state like any entity — drawn inside the FOV as the `z`
    // a downed guard reads as (§10.3), in Caution: an unaware threat's colour,
    // because what a loose body *means* is trouble waiting to be found (§11.3). Two
    // states speak the Owned vocabulary instead: the body **in your hands** (§8.3),
    // yours while you hold it, and a body **stowed in a cupboard** (§7.2) — gone to
    // every guard, but shown to *you* as an Owned `z` marking the **locked** cupboard
    // (no longer a hideout). A **loose** body is never remembered; the locked-cupboard
    // status **is**, persisted out of view by the memory pass below.
    for body in state.bodies() {
        if !fov.contains(body.cell()) {
            continue;
        }
        let stowed = facility.terrain(body.cell()) == Some(Terrain::Hideout);
        let fg = if stowed || state.dragging() == Some(body.cell()) {
            Category::Owned
        } else {
            Category::Caution
        };
        put(body.cell(), BODY_GLYPH, fg);
    }
    // A **seen** guard (in the FOV, §9.2) draws as the full state-coloured `g`; the
    // `g` glyph is re-categorised every turn from the guard's state (§11.2): yellow →
    // orange → red is the guard's mind, made visible. A **sensed** guard is a
    // *background* highlight instead, painted below alongside the danger overlay — no
    // glyph of its own. A guard perceived neither way draws nothing and is never
    // remembered (§11.5a), so leaving both view and sense range erases it.
    // A guard an **area effect** holds (§8.3/#308) leaves the §11.2 threat ladder
    // altogether: a frozen mind is not a rung on yellow → orange → red, so its `g`
    // draws in `Category::Effect` instead of its state colour — "this one cannot
    // move", said in the very channel that exists to show what a guard is thinking.
    // The absence of its cone from the danger overlay (dropped upstream in
    // `visible_cone_cells`) is truthful but negative; this is the positive half.
    for guard in state.guards() {
        if state.perceive_guard(guard) == Some(GuardPerception::Seen) {
            let fg = if state.guard_under_effect(guard) {
                Category::Effect
            } else {
                guard.state().category()
            };
            put(guard.pos(), GUARD_GLYPH, fg);
        }
    }
    // The player, always Owned — trivially inside their own FOV. Inside a hideout
    // the player is concealed: the cupboard keeps its `}` glyph but recolours to
    // Owned (§10.3/§11.3) — the "you are hidden here" signal — instead of drawing
    // the `@`. Read through the same `hidden` query the loop and vision use, so
    // the picture cannot disagree.
    let player_glyph = if state.hidden() { '}' } else { PLAYER_GLYPH };
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

    // The locked-cupboard signal persists in memory (§11.5a/§7.2): a cupboard you
    // have seen with a body stowed in it is a permanent fact — a spent hideout — so
    // out of view it is **remembered** as an Owned `z`, the same way a seen console
    // is remembered (§11.5a), rather than reverting to the empty
    // `}`. Only the stowed lock persists; a loose body is live state and is never
    // remembered, so it is not drawn here. Runs after the entity layer, writing only
    // out-of-FOV cells (in view they are already the live `z` above).
    for body in state.bodies() {
        let cell = body.cell();
        if fov.contains(cell) {
            continue;
        }
        if facility.terrain(cell) == Some(Terrain::Hideout) && memory.contains(cell) {
            cells[(cell.y * width + cell.x) as usize] = GlyphCell {
                glyph: BODY_GLYPH,
                fg: Category::Owned,
                bg: None,
                vis: Visibility::Remembered,
            };
        }
    }

    // The effect layer's footprint (§8.3/§11.5, #308): the §6.1 box a just-fired area
    // effect reaches, washed over the board in `Category::Effect` for the flash's own
    // turn ([`EFFECT_FLASH_TURNS`](crate::EFFECT_FLASH_TURNS)) so the player learns
    // where Confusion's bubble actually ends — the one thing the window's start/end
    // messaging cannot say. Painted **first of all the backgrounds**, so it is the
    // weakest cue on the board: every mark below overwrites it, and an advisory layer
    // can never hide the §11.5 [SETTLED] detection set or a sensed cue. It reaches
    // through walls and over unseen ground because that is what the effect does — your
    // own gadget's range is not something the fog can keep from you — and it is the
    // *live* box ([`State::effect_footprint`]), re-measured every turn, so it travels
    // with the player exactly as the freeze does.
    for cell in state.effect_footprint() {
        cells[(cell.y * width + cell.x) as usize].bg = Some(Category::Effect);
    }

    // The spot flash (§11.5/§9.2/§7.6, #222): the one-beat sightline of a guard that
    // *freshly* spotted the player from **outside their view** — the "a guard just saw
    // you, and here is where it is" cue the loop was missing (§7.6). It lights the
    // straight line between spotter and player red (`Danger`): honest, because that
    // guard's cone genuinely watches those cells, and a strict momentary *subset* of
    // the overlay, gone on the next action. Painted **first**, the weakest background
    // cue, so the marks below win where they coincide: a *sensed* spotter keeps its
    // orange position dot with the red line running up to it, and a guard that is
    // neither seen nor sensed is marked by the red line's own endpoint. Guards the
    // player can see are filtered upstream ([`State::spot_flash`]) — their real cone
    // paints anyway (§9.2), so this never double-draws or restates a seen cone.
    for cell in state.spot_flash() {
        cells[(cell.y * width + cell.x) as usize].bg = Some(Category::Danger);
    }

    // The door-change cue (§9.4/§10.4): the whole footprint of every door that opened
    // or shut away from the player, within `DOOR_SENSE_RANGE`, gets a
    // `Category::Sensed` background — the *same* orange "sensed through a wall" channel
    // as a guard felt through a wall, a filled highlight that fades over a few turns
    // (evidence someone passed, position only, never who or which way). Painted
    // *before* the danger overlay so a coincident cone outranks it (§11.5: being seen
    // outranks). Painted with the sensed-guard pass below, which shares the category.
    for cell in state.door_cues() {
        cells[(cell.y * width + cell.x) as usize].bg = Some(Category::Sensed);
    }

    // The sensed highlight (§9.2): every guard the player *senses* through a wall but
    // cannot see gets an orange `Category::Sensed` background on its exact cell — a
    // filled, eye-catching marker over whatever geometry masks the cell, position only
    // and never a glyph of its own. It carries no cone and no danger overlay: knowing
    // where a guard is is not knowing whether it can see you. Painted *before* the
    // danger overlay so a coincident red still wins — a sensed guard's cell that a
    // *seen* guard also watches reads danger first (§11.5: being seen outranks) — and
    // *after* the door cue, so a sensed guard sitting on a just-changed door reads as
    // the guard, not the trace.
    // A **sensed** guard an area effect holds carries its mark here instead (#308):
    // it has no glyph to recolour, so the mark takes over its own highlight, cyan in
    // place of orange. Nothing is lost by the swap — a filled cell still says "a guard
    // is exactly here", which is all the orange ever claimed — and the swap is the
    // whole point of the layer through walls: the bubble freezes what you cannot see,
    // so this is the common case, not the corner one. It is only ever a *recolour* of
    // a guard already drawn ([`State::guard_under_effect`] gates on perception), never
    // a new mark, so the fog gives nothing away.
    for guard in state.guards() {
        if state.perceive_guard(guard) == Some(GuardPerception::Sensed) {
            let bg = if state.guard_under_effect(guard) {
                Category::Effect
            } else {
                Category::Sensed
            };
            cells[(guard.pos().y * width + guard.pos().x) as usize].bg = Some(bg);
        }
    }

    // The danger overlay's cone pass (§11.5), last, across terrain and entities
    // alike: the union of every visible guard's cone. Its definition — the "seen"
    // gate, the concealment spare (a player concealed from that guard is not
    // detected, §10.3), the in-duct mouth-peek clip (§10.7/#134), and the
    // `always_show_vision_cones` widening (§12.6) — lives in
    // [`State::visible_cone_cells`], so this paint and the held-movement guard
    // (#223) read one set and cannot disagree. Backgrounds compose with whatever
    // glyph is on the cell: a watched guard, a watched player, watched floor.
    for cell in state.visible_cone_cells() {
        cells[(cell.y * width + cell.x) as usize].bg = Some(Category::Danger);
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
        // A duct **entry** is geometry as well (§10.7): visible from turn one like a
        // door, an `=` in the wall you can plan a shortcut around. The crawl *path*
        // between the entries is not geometry — its interior cells keep their own
        // terrain (they may cross floor) and the path is drawn only while crawled,
        // never here (§11.5a/#134), so nothing gives the shortcut's route away.
        Terrain::Floor
        | Terrain::Wall
        | Terrain::DoorHinge
        | Terrain::Exit
        | Terrain::DuctEntry
        | Terrain::PartialCover => (terrain, Visibility::Dimmed),
        // A door's *position* is geometry but its open/closed pose is live state,
        // never remembered: out of view a panel always draws canonically closed.
        Terrain::DoorPanelClosed | Terrain::DoorPanelOpen => {
            (Terrain::DoorPanelClosed, Visibility::Dimmed)
        }
        // Contents: hidden until seen, then remembered (§11.5a). The comms console
        // (§7.3/§7.7) is contents like the intel console: the counterplay it offers
        // has to be *found*, so the map never advertises it before the player has
        // scouted the room.
        Terrain::Console | Terrain::CommsConsole | Terrain::Hideout if remembered => {
            (terrain, Visibility::Remembered)
        }
        // Never seen: masked by the geometry naturally in its place — plain floor
        // where a console stands, plain wall over a hideout alcove, so the map
        // gives neither away before the player has scouted it.
        Terrain::Console | Terrain::CommsConsole => (Terrain::Floor, Visibility::Dimmed),
        Terrain::Hideout => (Terrain::Wall, Visibility::Dimmed),
    }
}

/// A blank full-screen [`Grid`] — every cell an empty, live, uncoloured space. The
/// starting canvas of a **panel** render (the help card, the menu): a surface that
/// replaces the game frame entirely rather than overlaying it, so it begins from
/// nothing and draws its own rows.
pub(super) fn blank_grid(width: u32, height: u32) -> Grid {
    let blank = GlyphCell {
        glyph: ' ',
        fg: Category::Neutral,
        bg: None,
        vis: Visibility::Live,
    };
    Grid {
        width,
        height,
        cells: vec![blank; (width * height) as usize],
    }
}

/// Write `text` onto `grid` from `(x, y)` in `category`, clamping at the right edge
/// and off the bottom — the one drawing primitive the panels share, so every row of
/// every panel truncates the same way on a small board.
pub(super) fn draw(grid: &mut Grid, x: u32, y: u32, text: &str, category: Category) {
    if y >= grid.height {
        return;
    }
    for (i, glyph) in text.chars().enumerate() {
        let cx = x + i as u32;
        if cx >= grid.width {
            break;
        }
        grid.cells[(y * grid.width + cx) as usize] = GlyphCell {
            glyph,
            fg: category,
            bg: None,
            vis: Visibility::Live,
        };
    }
}

mod help;
mod hud;
mod menu;
pub use help::{help_hit, HelpHit, HelpTab};
pub use hud::{
    ability_at, is_help_button, is_message_button, render_screen, ScreenUi, BOTTOM_ROWS, TOP_ROWS,
};
pub use menu::{menu_hit, MenuEntry, MenuUi};

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
    use crate::ability::Loadout;
    use crate::cell::{Cell, Direction};
    use crate::facility::{Facility, Terrain};
    use crate::guard::Guard;
    use crate::state::{Event, Input, State, CONFUSION_RADIUS};
    use crate::test_support::open_room;
    use crate::LevelModifiers;

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

    /// The same bare board, holding one salvaged-tech ability (§8.3/#244): a
    /// loadout is built up from the innate set, so a render test that drives a
    /// tech says which tech it has rather than inheriting the lot.
    fn state_holding(
        w: u32,
        h: u32,
        player: Cell,
        guards: Vec<Guard>,
        tech: crate::AbilityId,
    ) -> State {
        state(w, h, player, guards).with_loadout(Loadout::innate().with(tech))
    }

    /// The same board facing **north**, for the tests that post their guards up the
    /// column above the player — entities draw only inside the FOV (§11.5a), so which
    /// way the player looks is part of the fixture.
    fn state_holding_facing_north(
        w: u32,
        h: u32,
        player: Cell,
        guards: Vec<Guard>,
        tech: crate::AbilityId,
    ) -> State {
        State::new(
            open_room(w, h),
            player,
            Direction::North,
            guards,
            Vec::new(),
            Cell::new(w - 2, h - 2),
        )
        .with_loadout(Loadout::innate().with(tech))
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
        let mut s = state_holding(10, 10, Cell::new(4, 4), Vec::new(), AbilityId::Decoy);
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
        )
        .with_loadout(Loadout::innate().with(AbilityId::Camouflage));
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
    /// Owned `z` while in your hands, and an Owned `z` once stowed in a cupboard,
    /// marking the **locked** cupboard (§7.2/§10.3). A stowed body is gone to every
    /// guard, but shown to you so you can read which cupboards you have spent.
    #[test]
    fn a_dragged_body_and_a_stowed_one_both_read_owned_z() {
        let mut layout = open_room(10, 10);
        layout.place(Cell::new(5, 5), Terrain::Hideout);
        layout.place(Cell::new(3, 4), Terrain::Hideout); // the stow cupboard
        let mut s = State::new(
            layout,
            Cell::new(5, 5),
            Direction::North,
            vec![Guard::stationary(Cell::new(5, 4))],
            Vec::new(),
            Cell::new(8, 8),
        );
        s.step(Input::Step(Direction::North)); // takedown: body at (5,4)
        s.step(Input::Step(Direction::North)); // climb out onto the body
        s.step(Input::Step(Direction::West)); // step off to (4,4) — take hold
        assert_eq!(s.dragging(), Some(Cell::new(5, 4)));

        // Look around (Wait's 360°) to see the body behind you: yours, an Owned `z`.
        s.step(Input::Wait);
        let held = render(&s).get(5, 4);
        assert_eq!(held.glyph, 'z');
        assert_eq!(held.fg, Category::Owned, "the body in your hands is yours");

        // Stow it in the cupboard to the west: the locked cupboard shows an Owned `z`.
        s.step(Input::Step(Direction::West));
        let stowed = render(&s).get(3, 4);
        assert_eq!(
            stowed.glyph, 'z',
            "the locked cupboard shows the stowed body"
        );
        assert_eq!(stowed.fg, Category::Owned, "stowed and sealed by you");
    }

    /// §11.5a/§7.2: the locked-cupboard status persists in memory. Once you have
    /// seen a body stowed in a cupboard, walking away keeps it drawn as a
    /// **remembered** Owned `z` — a spent hideout you can still read — rather than
    /// reverting to the empty `}` the terrain fog would show.
    #[test]
    fn a_stowed_cupboard_is_remembered_out_of_view() {
        let mut layout = open_room(10, 10);
        layout.place(Cell::new(5, 5), Terrain::Hideout);
        layout.place(Cell::new(3, 4), Terrain::Hideout); // the stow cupboard
        let mut s = State::new(
            layout,
            Cell::new(5, 5),
            Direction::North,
            vec![Guard::stationary(Cell::new(5, 4))],
            Vec::new(),
            Cell::new(8, 8),
        );
        s.step(Input::Step(Direction::North)); // takedown: body at (5,4)
        s.step(Input::Step(Direction::North)); // climb out onto the body
        s.step(Input::Step(Direction::West)); // step off to (4,4) — take hold
        s.step(Input::Step(Direction::West)); // stow into the cupboard at (3,4)
        assert_eq!(
            s.bodies()[0].cell(),
            Cell::new(3, 4),
            "precondition: stowed"
        );

        // Walk away east until the cupboard leaves the FOV, then it is remembered.
        s.step(Input::Step(Direction::East));
        s.step(Input::Step(Direction::East));
        let g = render(&s);
        assert!(
            !s.player_fov().contains(Cell::new(3, 4)),
            "precondition: the cupboard is out of view",
        );
        let remembered = g.get(3, 4);
        assert_eq!(remembered.glyph, 'z', "the locked cupboard is still a z");
        assert_eq!(remembered.fg, Category::Owned, "and still Owned");
        assert_eq!(
            remembered.vis,
            Visibility::Remembered,
            "drawn from memory, not live",
        );
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

    /// §11.2 spent objectives: a live console is Interest `$`; once its intel is
    /// **taken** the same cell keeps its `$` glyph but recolours to Neutral — inert
    /// scenery — so the player can tell at a glance what they have already collected.
    /// The recolour holds in memory too: a spent console you have walked away from
    /// does not reappear as live Interest.
    #[test]
    fn a_spent_console_recolours_to_neutral() {
        // Player at (10,10) facing east; the console one cell east, in view.
        let mut s = State::new(
            open_room(40, 40),
            Cell::new(10, 10),
            Direction::East,
            Vec::new(),
            [Cell::new(11, 10)],
            Cell::new(38, 38),
        );

        // Untaken: a live Interest `$`.
        let live = render(&s).get(11, 10);
        assert_eq!(
            (live.glyph, live.fg, live.vis),
            ('$', Category::Interest, Visibility::Live),
            "a live console is Interest",
        );

        // Bump the console east to take the intel; the player does not move.
        assert_eq!(
            s.step(Input::Step(Direction::East)),
            vec![Event::IntelTaken {
                remaining: 0,
                still_needed: 0
            }],
        );
        assert_eq!(
            s.player(),
            Cell::new(10, 10),
            "taking intel is a bump, not a move"
        );

        // Spent: the `$` stays but the category drops to Neutral (§11.2).
        let spent = render(&s).get(11, 10);
        assert_eq!(
            (spent.glyph, spent.fg, spent.vis),
            ('$', Category::Neutral, Visibility::Live),
            "a spent console is Neutral scenery, glyph kept",
        );

        // Leave it behind (face and step west): remembered, and still Neutral —
        // never a live-purple ghost in memory.
        s.step(Input::Step(Direction::West)); // to (9,10), facing west
        let remembered = render(&s).get(11, 10);
        assert_eq!(
            (remembered.glyph, remembered.fg, remembered.vis),
            ('$', Category::Neutral, Visibility::Remembered),
            "a spent console stays Neutral in memory",
        );
    }

    /// §11.2/§7.7: the **comms console** takes the same spent recolour. Live it is an
    /// Interest `Ψ` — its own glyph, so it is never confused with the intel `$`
    /// (§11.3); once the radio net is dead it keeps the glyph and drops to Neutral,
    /// reading as the spent scenery it now is (there is nothing left to switch off).
    #[test]
    fn a_silenced_comms_console_recolours_to_neutral() {
        let mut layout = open_room(40, 40);
        layout.place(Cell::new(11, 10), Terrain::CommsConsole);
        let mut s = State::new(
            layout,
            Cell::new(10, 10),
            Direction::East,
            Vec::new(),
            Vec::new(),
            Cell::new(38, 38),
        );

        let live = render(&s).get(11, 10);
        assert_eq!(
            (live.glyph, live.fg, live.vis),
            ('Ψ', Category::Interest, Visibility::Live),
            "a live comms console is Interest, with its own glyph",
        );

        assert_eq!(
            s.step(Input::Step(Direction::East)),
            vec![Event::CommsSilenced {
                at: Cell::new(11, 10)
            }],
        );
        let spent = render(&s).get(11, 10);
        assert_eq!(
            (spent.glyph, spent.fg, spent.vis),
            ('Ψ', Category::Neutral, Visibility::Live),
            "a silenced comms console is Neutral scenery, glyph kept",
        );
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

    /// The `always_show_vision_cones` level modifier (§12.6), directional: it may
    /// only ever *widen* the overlay (§11.5 [SETTLED]). On the same scene as
    /// [`an_unseen_guards_cone_paints_no_danger`], turning it on paints the unseen
    /// guard's cone that baseline hides — so the painted danger set is a strict
    /// superset of baseline, never smaller, proving the modifier reveals more.
    #[test]
    fn the_show_vision_cones_modifier_paints_an_unseen_guards_cone() {
        // A guard behind the north-facing player, out of the FOV — sensed, not seen.
        let guard = Cell::new(10, 14);
        let scene = || {
            State::new(
                open_room(20, 20),
                Cell::new(10, 10),
                Direction::North,
                vec![Guard::stationary(guard)],
                Vec::new(),
                Cell::new(18, 18),
            )
        };
        let danger_cells = |g: &Grid| -> Vec<(u32, u32)> {
            let mut cells = Vec::new();
            for y in 0..g.height() {
                for x in 0..g.width() {
                    if g.get(x, y).bg == Some(Category::Danger) {
                        cells.push((x, y));
                    }
                }
            }
            cells
        };

        let baseline = scene();
        assert!(
            !baseline.player_fov().contains(guard),
            "the guard is unseen"
        );
        let baseline_danger = danger_cells(&render(&baseline));
        assert!(
            baseline_danger.is_empty(),
            "baseline: an unseen guard's cone paints no danger",
        );

        let modified = scene().with_modifiers(LevelModifiers {
            always_show_vision_cones: true,
            ..LevelModifiers::default()
        });
        let modified_danger = danger_cells(&render(&modified));

        // Widen-only (§11.5): every baseline-red cell is still red …
        for cell in &baseline_danger {
            assert!(
                modified_danger.contains(cell),
                "modifier must never hide a red cell: {cell:?}",
            );
        }
        // … and the unseen guard's cone is now painted, strictly more than baseline.
        // (Its own watched cell reads red too — a cone covers its origin, and the
        // danger overlay outranks the sensed marker there, §11.5.)
        assert!(
            modified_danger.len() > baseline_danger.len(),
            "modifier: the unseen guard's cone now paints danger",
        );
        assert!(
            modified_danger.contains(&(guard.x, guard.y)),
            "the sensed guard's own cell is inside its now-revealed cone",
        );
    }

    /// §11.5/§9.2/§7.6 (#222): the **spot flash**. A guard the player cannot see
    /// that *freshly* detects them lights the straight sightline to the player red
    /// — where the threat is, and which way to run — for exactly that one beat, then
    /// clears on the next action. The spotter keeps its orange **sensed** position
    /// dot (§9.2), with the red line running up to it. The lifetime (one action) is
    /// a **[START]** choice, pinned here.
    #[test]
    fn a_fresh_spot_flashes_the_sightline_then_clears() {
        // Guard at (10,5) facing south (spawn, §7.1); the player five cells south at
        // (10,10) facing *south* too — the guard is directly behind them, out of the
        // forward FOV, but its cone runs straight down over the player. So at level
        // start the guard freshly detects a player it is unseen by (§9.2): a spot with
        // nothing on screen to say where it came from — exactly what #222 fixes.
        let mut s = State::new(
            open_room(20, 20),
            Cell::new(10, 10),
            Direction::South,
            vec![Guard::stationary(Cell::new(10, 5))],
            Vec::new(),
            Cell::new(18, 18),
        );
        // Precondition: the spotter is *not* seen (it is behind the player). Within
        // sense range, so it is Sensed — its orange dot is the position channel the
        // flash draws a line up to (§9.2), not over.
        assert!(
            !s.player_fov().contains(Cell::new(10, 5)),
            "the guard is behind the player, unseen",
        );
        assert_eq!(
            s.perceive_guard(&s.guards()[0]),
            Some(GuardPerception::Sensed),
        );

        // The detection beat: the sightline from the spotter down to the player is
        // red — the cells (10,6)..=(10,10), the player's own cell included (they *are*
        // detected, the lose condition painted). The spotter's own cell keeps its
        // orange sensed dot; the red line stops there rather than painting over it.
        let g = render(&s);
        assert_eq!(
            g.get(10, 5).bg,
            Some(Category::Sensed),
            "the spotter keeps its orange position dot",
        );
        for y in 6..=10 {
            assert_eq!(
                g.get(10, y).bg,
                Some(Category::Danger),
                "the spot sightline is red at (10,{y})",
            );
        }
        // It is a *line*, not the cone: a cell beside it is untouched.
        assert_eq!(
            g.get(12, 8).bg,
            None,
            "off the sightline stays clear — a line, not the whole cone",
        );

        // The next quiet turn: the flash is momentary. Step *south*, away from the
        // guard and still facing away, so it stays behind and unseen (a Wait would
        // instead widen sight to 360° and reveal it, §8.3). The guard re-detects but
        // not *freshly*, so no line is drawn. The ambient sensed dot legitimately
        // stays — the guard is still there to be felt — but no **red** lingers
        // anywhere: the line is gone and, still unseen, its cone paints nothing
        // (§9.2/§11.5 [SETTLED] held).
        s.step(Input::Step(Direction::South));
        assert!(
            !s.player_fov().contains(Cell::new(10, 5)),
            "the guard is still behind the player, unseen",
        );
        let g = render(&s);
        for y in 0..g.height() {
            for x in 0..g.width() {
                assert_ne!(
                    g.get(x, y).bg,
                    Some(Category::Danger),
                    "no red lingers after the flash beat at ({x},{y})",
                );
            }
        }
        assert_eq!(
            g.get(10, 5).bg,
            Some(Category::Sensed),
            "the sensed dot remains — only the flash line was momentary",
        );
    }

    /// §9.2 [SETTLED] held: a guard the player **can see** when it detects them gets
    /// no separate flash line — its full cone already paints the danger overlay, so a
    /// sightline would only double-draw. The overlay is unchanged from the plain
    /// visible-cone golden.
    #[test]
    fn a_seen_guard_that_detects_gets_no_extra_flash_line() {
        // Guard adjacent at (9,9), in the FOV of a north-facing player at (10,10),
        // looking south so its cone covers the player: seen, and detecting.
        let s = State::new(
            open_room(20, 20),
            Cell::new(10, 10),
            Direction::North,
            vec![Guard::stationary(Cell::new(9, 9))],
            Vec::new(),
            Cell::new(18, 18),
        );
        assert_eq!(
            s.perceive_guard(&s.guards()[0]),
            Some(GuardPerception::Seen),
            "the guard is in view",
        );
        // No spot-flash cells are produced for a seen guard.
        assert_eq!(s.spot_flash().count(), 0, "a seen spotter flashes nothing");

        // The painted danger set is *exactly* the guard's cone — no line beyond it.
        let g = render(&s);
        let cone = s.guards()[0].fov();
        for y in 0..g.height() {
            for x in 0..g.width() {
                assert_eq!(
                    g.get(x, y).bg == Some(Category::Danger),
                    cone.contains(Cell::new(x, y)),
                    "bg at ({x},{y}) must mirror the cone, with no extra flash",
                );
            }
        }
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

    // --- Duct interior view (§10.7/#134) -------------------------------------

    /// A `9×9` fixture with a 4-cell duct in the wall band under the top border —
    /// entries at `(2,1)`/`(5,1)`, interior `(3,1)`/`(4,1)`, mouths `(2,2)`/`(5,2)`
    /// — opening into an open room below (mirrors the state-test fixture). The
    /// player starts on the near mouth, facing the entry, with `guards` in the room.
    fn duct_state(guards: Vec<Guard>) -> State {
        let mut f = Facility::walled_box(9, 9);
        for x in 1..=7 {
            f.set_terrain(x, 1, Terrain::Wall);
        }
        f.set_terrain(2, 1, Terrain::DuctEntry);
        f.set_terrain(5, 1, Terrain::DuctEntry);
        let duct = crate::Duct::new(vec![
            Cell::new(2, 1),
            Cell::new(3, 1),
            Cell::new(4, 1),
            Cell::new(5, 1),
        ]);
        let layout = crate::Layout::from_facility(f).with_ducts(vec![duct]);
        State::new(
            layout,
            Cell::new(2, 2),
            Direction::North,
            guards,
            Vec::new(),
            Cell::new(7, 7),
        )
    }

    /// With no duct occupied the view is ordinary (§11.5a): an **entry** is geometry,
    /// drawn `=` from turn one, but the **interior** is contents — plain wall until
    /// crawled, giving the shortcut's route away to nobody.
    #[test]
    fn an_unentered_duct_shows_entries_but_hides_its_path() {
        let g = render(&duct_state(Vec::new()));
        assert_eq!(g.get(2, 1).glyph, '=', "the near entry is visible geometry");
        assert_eq!(g.get(5, 1).glyph, '=', "the far entry is visible geometry");
        assert_eq!(
            g.get(3, 1).glyph,
            '#',
            "an un-crawled interior cell reads as plain wall"
        );
        assert_eq!(g.get(4, 1).glyph, '#');
    }

    /// While the player occupies a duct its whole path lights as a connected `=` run,
    /// with the `@` on their own cell (glyph priority `@` > `=`), and the world beyond
    /// renders as memory — no live guard glyph outside the (absent) mid-duct window.
    #[test]
    fn a_mid_duct_view_lights_the_path_and_fogs_the_world() {
        // A guard far down the room: beyond the reduced in-duct sense and out of any
        // window, so mid-duct it draws nothing at all.
        let mut s = duct_state(vec![Guard::stationary(Cell::new(7, 7))]);
        s.step(Input::Step(Direction::North)); // enter at (2,1)
        s.step(Input::Step(Direction::East)); // crawl to interior (3,1)
        let g = render(&s);

        // The occupied duct is one lit path of `=`, the player's cell an Owned `@`.
        assert_eq!(g.get(3, 1).glyph, '@', "the player's crawl cell");
        assert_eq!(g.get(3, 1).fg, Category::Owned);
        for &(x, y) in &[(2, 1), (4, 1), (5, 1)] {
            let c = g.get(x, y);
            assert_eq!(c.glyph, '=', "the rest of the path lights as =");
            assert_eq!(c.fg, Category::System);
            assert_eq!(
                c.vis,
                Visibility::Live,
                "the occupied duct is the live layer"
            );
        }
        // The far guard is neither seen nor sensed mid-duct: no glyph, no highlight.
        assert_ne!(g.get(7, 7).glyph, 'g', "no live guard beyond the walls");
        assert_eq!(
            g.get(7, 7).bg,
            None,
            "no sensed dot beyond the reduced range"
        );
    }

    /// On an **entry** the mouth peek is live: a guard down the mouth draws its full
    /// `g`, while the danger overlay is clipped to the window — every red cell is one
    /// the player can actually see (§11.5), nothing beyond the cast.
    #[test]
    fn an_entry_cell_peeks_live_and_clips_the_overlay_to_the_window() {
        let guard = Cell::new(2, 5); // straight down the mouth, in the peek
        let mut s = duct_state(vec![Guard::stationary(guard)]);
        s.step(Input::Step(Direction::North)); // enter at (2,1), peek out the mouth
        let g = render(&s);

        assert_eq!(g.get(2, 1).glyph, '@', "the player sits on the entry");
        assert_eq!(
            g.get(guard.x, guard.y).glyph,
            'g',
            "the peek sees the guard live"
        );

        // The danger overlay never paints a cell the player cannot see: inside a duct
        // the FOV is exactly the peek window, so every red cell lies within it.
        let fov = s.player_fov();
        for y in 0..9 {
            for x in 0..9 {
                if g.get(x, y).bg == Some(Category::Danger) {
                    assert!(
                        fov.contains(Cell::new(x, y)),
                        "a red cell at ({x},{y}) must be inside the peek window",
                    );
                }
            }
        }
    }

    /// A guard within the reduced in-duct sense but out of the window still shows as
    /// the §9.2 orange **Sensed** background through the memory view; one beyond the
    /// reduced range shows nothing.
    #[test]
    fn a_sensed_guard_shows_through_the_memory_view() {
        let near = Cell::new(3, 4); // Chebyshev 3 from the crawl cell (3,1): sensed
        let far = Cell::new(7, 7); // Chebyshev 6: beyond DUCT_SENSE_RANGE
        let mut s = duct_state(vec![Guard::stationary(near), Guard::stationary(far)]);
        s.step(Input::Step(Direction::North)); // enter
        s.step(Input::Step(Direction::East)); // crawl to (3,1)
        let g = render(&s);

        let sensed = g.get(near.x, near.y);
        assert_eq!(
            sensed.bg,
            Some(Category::Sensed),
            "the near guard is sensed"
        );
        assert_ne!(sensed.glyph, 'g', "sensed is a highlight, not a glyph");
        assert_eq!(
            g.get(far.x, far.y).bg,
            None,
            "the far guard is out of range"
        );
    }

    /// After the player crawls a duct and climbs out, its interior path is **hidden
    /// again** — it is shown only while crawled and never remembered (§11.5a/§10.7),
    /// so the shortcut's route is given away to nobody. The interior cells revert to
    /// their own terrain (plain wall in this fixture); only the two entries stay `=`,
    /// as geometry.
    #[test]
    fn a_left_duct_hides_its_path_again() {
        let mut s = duct_state(Vec::new());
        s.step(Input::Step(Direction::North)); // enter (2,1)
        for _ in 0..3 {
            s.step(Input::Step(Direction::East)); // crawl to (5,1)
        }
        s.step(Input::Step(Direction::South)); // climb out at (5,2)
        assert!(!s.in_duct(), "the normal view is restored on the same turn");
        let g = render(&s);

        // The interior cells are no longer part of the lit path — they read as the
        // plain wall band they overlie, live memory carries no `=` there.
        for &(x, y) in &[(3, 1), (4, 1)] {
            assert_eq!(
                g.get(x, y).glyph,
                '#',
                "a left duct's interior reverts to wall — no remembered `=`",
            );
        }
        // The entries remain visible geometry (§11.5a): `=` from turn one, occupied or not.
        assert_eq!(g.get(2, 1).glyph, '=', "the near entry stays geometry");
        assert_eq!(g.get(5, 1).glyph, '=', "the far entry stays geometry");
    }

    // --- The debug reveal (§12.6) --------------------------------------------

    /// The playtest reveal is a **sight** substitution, not a drawing rule (it lands
    /// in the sight phase, `State::recompute_sight`), so the frame needs no special
    /// case here and gets the plain live picture everywhere: a never-scouted console
    /// and cupboard draw their real glyphs, a far guard draws its `g`, and every cell
    /// is [`Visibility::Live`] — one colour scheme to read, no dimmed or remembered
    /// second layer over the board.
    #[test]
    fn the_debug_reveal_draws_the_whole_level_live() {
        use crate::DebugModifiers;
        // Player facing north; a console behind them, a cupboard across the room, a
        // guard 14 cells south — past the sense box, so none of the three shows.
        let guard = Cell::new(10, 24);
        let mut layout = open_room(40, 40);
        layout.place(Cell::new(20, 30), Terrain::Hideout);
        let fogged = State::new(
            layout,
            Cell::new(10, 10),
            Direction::North,
            vec![Guard::stationary(guard)],
            [Cell::new(10, 14)],
            Cell::new(38, 38),
        );
        let g = render(&fogged);
        assert_eq!(g.get(10, 14).glyph, '\u{b7}', "the console masks as floor");
        assert_eq!(g.get(20, 30).glyph, '#', "the cupboard masks as wall");
        assert_eq!(g.get(guard.x, guard.y).glyph, '\u{b7}', "no guard drawn");

        let revealed = fogged.with_debug(DebugModifiers {
            reveal_whole_level: true,
        });
        let g = render(&revealed);
        assert_eq!(g.get(10, 14).glyph, '$', "the console shows");
        assert_eq!(g.get(20, 30).glyph, '}', "the cupboard shows");
        assert_eq!(g.get(guard.x, guard.y).glyph, 'g', "and so does the guard");
        // Everything is the live layer — nothing on the board is dimmed or
        // remembered, so the whole picture reads in one scheme.
        for y in 0..g.height() {
            for x in 0..g.width() {
                assert_eq!(
                    g.get(x, y).vis,
                    Visibility::Live,
                    "({x},{y}) is not drawn live",
                );
            }
        }
        // The guard is seen, so the overlay paints its cone (§11.5) — the reveal
        // gives the cones for free rather than needing a second switch.
        assert_eq!(
            g.get(guard.x, guard.y).bg,
            Some(Category::Danger),
            "the seen guard's own cell is watched",
        );
        let watched = (0..g.height())
            .flat_map(|y| (0..g.width()).map(move |x| (x, y)))
            .filter(|&(x, y)| g.get(x, y).bg == Some(Category::Danger))
            .count();
        assert_eq!(
            watched,
            revealed.guards()[0].fov().cells().count(),
            "the whole cone paints — the reveal gives the cones for free",
        );
    }

    /// §8.3/§11.5 (#308): the **effect layer's footprint**. Firing Confusion washes the
    /// §6.1 box it reaches in `Category::Effect` — asserted against the rule's own
    /// [`EffectArea`](crate::EffectArea) rather than a hand-drawn shape, so the picture
    /// and the freeze can never drift apart, and painted **through walls and fog** (the
    /// reach of your own gadget is not something the fog can keep from you).
    #[test]
    fn the_effect_flash_paints_the_rules_own_box() {
        use crate::{AbilityId, Effect};
        // No guards: nothing to overwrite the wash, so the footprint is the whole
        // story of what this frame paints.
        let mut s = state_holding(30, 30, Cell::new(15, 15), Vec::new(), AbilityId::Confusion);
        // Before the fire, nothing on the board speaks the effect vocabulary at all.
        let quiet = render(&s);
        assert!(
            !any_effect_ink(&quiet),
            "with no effect running the frame is exactly today's",
        );

        s.step(Input::Activate(AbilityId::Confusion));
        let area = s
            .effect_area(Effect::Confuse)
            .expect("Confusion is running");
        let g = render(&s);
        for y in 0..g.height() {
            for x in 0..g.width() {
                assert_eq!(
                    g.get(x, y).bg == Some(Category::Effect),
                    area.contains(Cell::new(x, y)),
                    "({x},{y}): the painted box must be the rule's box",
                );
            }
        }
        // A box, not a disc: the diagonal corner is in, the cell past the edge is out.
        let corner = Cell::new(15 + CONFUSION_RADIUS, 15 + CONFUSION_RADIUS);
        assert_eq!(g.get(corner.x, corner.y).bg, Some(Category::Effect));
        assert_eq!(g.get(15, 15 + CONFUSION_RADIUS + 1).bg, None);
    }

    /// §8.3/§11.2 (#308): a frozen guard the player **sees** leaves the threat ladder.
    /// Its `g` recolours from its state category to `Category::Effect` — a mind
    /// switched off is not a rung on yellow → orange → red — and it climbs straight
    /// back on when the window ends, since the freeze is a pause, not a reset.
    #[test]
    fn a_seen_frozen_guard_wears_the_effect_colour() {
        use crate::AbilityId;
        // Guard three cells north, inside the bubble and in the FOV of a north-facing
        // player, so it is Seen and its glyph is what carries the mark.
        let mut s = state_holding_facing_north(
            20,
            20,
            Cell::new(10, 10),
            vec![Guard::stationary(Cell::new(10, 7))],
            AbilityId::Confusion,
        );
        s.step(Input::Wait); // establish sight
        let ladder = render(&s).get(10, 7).fg;
        assert!(
            matches!(
                ladder,
                Category::Caution | Category::Warning | Category::Danger
            ),
            "precondition: an awake guard sits on the §11.2 threat ladder, not off it",
        );

        s.step(Input::Activate(AbilityId::Confusion));
        let g = render(&s);
        assert_eq!(g.get(10, 7).glyph, 'g', "still the guard glyph");
        assert_eq!(g.get(10, 7).fg, Category::Effect, "…with its mind off");

        // End the window early (§4.4 — a free action, so nobody moves): the guard
        // resumes exactly where it was, and its colour climbs back onto the ladder.
        s.step(Input::Deactivate(AbilityId::Confusion));
        assert_eq!(
            render(&s).get(10, 7).fg,
            ladder,
            "the mark clears with the window — no residue, and the pause was a pause",
        );
    }

    /// §9.2/§8.3 (#308): a frozen guard felt only **through a wall** carries the mark on
    /// its sensed highlight instead — it has no glyph to recolour. This is the common
    /// case, not the corner one: the bubble reaches through walls, so most of what it
    /// freezes is exactly what the player cannot see.
    #[test]
    fn a_sensed_frozen_guard_takes_the_mark_on_its_highlight() {
        use crate::AbilityId;
        // A wall between the player at (10,10) and the guard at (10,7): inside the
        // bubble (distance 3) and inside the sense box, but out of sight.
        let mut layout = open_room(20, 20);
        for x in 8..13 {
            layout.place(Cell::new(x, 8), Terrain::Wall);
        }
        let mut s = State::new(
            layout,
            Cell::new(10, 10),
            Direction::North,
            vec![Guard::stationary(Cell::new(10, 7))],
            Vec::new(),
            Cell::new(18, 18),
        )
        .with_loadout(Loadout::innate().with(AbilityId::Confusion));
        s.step(Input::Wait);
        assert_eq!(
            s.perceive_guard(&s.guards()[0]),
            Some(GuardPerception::Sensed),
            "precondition: felt through the wall, not seen",
        );
        assert_eq!(
            render(&s).get(10, 7).bg,
            Some(Category::Sensed),
            "the orange position dot before the freeze",
        );

        s.step(Input::Activate(AbilityId::Confusion));
        assert!(s.guard_confused(&s.guards()[0]), "the wall spares nothing");
        assert_eq!(
            render(&s).get(10, 7).bg,
            Some(Category::Effect),
            "the same dot, now saying frozen as well as where",
        );
    }

    /// §11.5 **[SETTLED]** (#308): the effect layer is *advisory* and never outranks the
    /// detection set. A cell inside the footprint that a **seen** guard also watches
    /// paints `Danger`, and so does the frozen guard's own cell when another guard's
    /// live cone covers it — red still means "will detect you", everywhere it applies.
    #[test]
    fn red_still_wins_inside_the_bubble() {
        use crate::AbilityId;
        // The watcher at (10,2) is eight cells north — outside the bubble, so it stays
        // awake — looking south down the column, over the frozen guard at (10,10) and
        // on across the cells the bubble covers.
        let mut s = state_holding_facing_north(
            20,
            24,
            Cell::new(10, 16),
            vec![
                Guard::stationary(Cell::new(10, 10)),
                Guard::stationary(Cell::new(10, 2)),
            ],
            AbilityId::Confusion,
        );
        s.step(Input::Activate(AbilityId::Confusion));
        assert!(s.guard_confused(&s.guards()[0]), "the near guard is frozen");
        assert!(
            !s.guard_confused(&s.guards()[1]),
            "the watcher is outside the bubble",
        );
        let watcher_cone: Vec<Cell> = s.guards()[1].fov().cells().collect();
        assert!(
            watcher_cone.contains(&Cell::new(10, 10)),
            "precondition: the watcher's cone covers the frozen guard",
        );

        let g = render(&s);
        let frozen = g.get(10, 10);
        assert_eq!(frozen.fg, Category::Effect, "the freeze still shows");
        assert_eq!(
            frozen.bg,
            Some(Category::Danger),
            "…and the red it stands in outranks the wash",
        );
        // Every watched cell inside the footprint is red, not cyan.
        let area = s
            .effect_area(crate::Effect::Confuse)
            .expect("Confusion is running");
        for &cell in watcher_cone.iter().filter(|&&c| area.contains(c)) {
            assert_eq!(
                g.get(cell.x, cell.y).bg,
                Some(Category::Danger),
                "{cell:?} is watched, so it reads red inside the bubble too",
            );
        }
    }

    /// §9.4/§11.5 (#308): the **orange sense channel** beats the effect wash too. A door
    /// that shuts itself inside the bubble keeps its `Sensed` cue — evidence someone
    /// passed is a fact about the world, and an advisory layer never paints over one.
    #[test]
    fn a_sensed_door_cue_survives_the_footprint() {
        use crate::region::{DoorKind, RegionGraph, RegionKind};
        use crate::AbilityId;
        // Two rooms joined by an automatic door down column 3 (§10.4/#147); the player
        // stands beside it, well inside the bubble.
        let cells = |xs: std::ops::Range<u32>| {
            xs.flat_map(|x| (1..4).map(move |y| Cell::new(x, y)))
                .collect::<Vec<_>>()
        };
        let mut f = Facility::walled_box(7, 5);
        let mut graph = RegionGraph::new(7, 5);
        let left = graph.add_region(RegionKind::Room, cells(1..3));
        let right = graph.add_region(RegionKind::Room, cells(4..6));
        let panels: Vec<Cell> = (1..4).map(|y| Cell::new(3, y)).collect();
        for &p in &panels {
            f.set_terrain(p.x, p.y, Terrain::DoorPanelClosed);
        }
        graph.add_door(left, right, [], panels, DoorKind::Automatic { delay: 3 });
        let mut s = State::new(
            crate::Layout::from_parts(f, graph),
            Cell::new(2, 2),
            Direction::East,
            Vec::new(),
            Vec::new(),
            Cell::new(4, 3),
        )
        .with_loadout(Loadout::innate().with(AbilityId::Confusion));

        s.step(Input::Step(Direction::East)); // bump the panel open — the player's own, no cue
        s.step(Input::Wait);
        // Fire Confusion on the very turn the automatic door times out: the flash lasts
        // exactly this frame, and the door's self-close is nobody's doing, so it lights
        // the §9.4 cue over the whole doorway in the same render the wash covers it.
        let closed = s.step(Input::Activate(AbilityId::Confusion));
        assert!(
            closed.iter().any(|e| matches!(
                e,
                Event::DoorClosed {
                    by_player: false,
                    ..
                }
            )),
            "precondition: the door shut itself: {closed:?}",
        );
        let g = render(&s);
        assert!(
            g.get(2, 1).bg == Some(Category::Effect),
            "precondition: the flash is still washing the room",
        );
        for y in 1..4 {
            assert_eq!(
                g.get(3, y).bg,
                Some(Category::Sensed),
                "the door cue at (3,{y}) outranks the effect wash",
            );
        }
    }

    /// §8.3 (#308): the bubble travels with the player, so the marks are re-read every
    /// turn — a guard thaws the turn you step out of range of it, and freezes the turn
    /// you step back into range. The picture keeps up on the same turn, not the next.
    #[test]
    fn walking_moves_the_marks_the_same_turn() {
        use crate::AbilityId;
        // Guard seven cells north — one past the edge of the bubble — of a north-facing
        // player, so it starts outside and a single step north brings it in.
        let mut s = state_holding_facing_north(
            20,
            24,
            Cell::new(10, 14),
            vec![Guard::stationary(Cell::new(10, 7))],
            AbilityId::Confusion,
        );
        s.step(Input::Activate(AbilityId::Confusion));
        assert_ne!(
            render(&s).get(10, 7).fg,
            Category::Effect,
            "one cell past the edge: awake",
        );

        s.step(Input::Step(Direction::North)); // distance 6 — inside
        assert_eq!(
            render(&s).get(10, 7).fg,
            Category::Effect,
            "the step froze it, and the mark landed on the same turn",
        );

        s.step(Input::Step(Direction::South)); // back out to 7
        assert_ne!(
            render(&s).get(10, 7).fg,
            Category::Effect,
            "stepping away thaws it, and the mark clears with it",
        );
    }

    /// Whether any cell of `grid` speaks the effect vocabulary in either channel — the
    /// "with nothing running, the frame is exactly today's" check (#308).
    fn any_effect_ink(grid: &Grid) -> bool {
        (0..grid.height()).any(|y| {
            (0..grid.width()).any(|x| {
                let cell = grid.get(x, y);
                cell.fg == Category::Effect || cell.bg == Some(Category::Effect)
            })
        })
    }
}
