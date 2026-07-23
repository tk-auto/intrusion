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

    // The duct interior view (§11.5a/§10.7, #134). While the player **occupies** a
    // duct, its whole path lights as a connected run of `=` — the crawlspace read as
    // one space (the player's own cell is overwritten by the `@` below; glyph
    // priority `@` > `=`). A duct the player has **crawled but since left** keeps its
    // interior cells **remembered** as `=` rather than reverting to blank wall
    // (§11.5a: the flight paths you scouted are worth more than the ones you didn't).
    // An **entry** is geometry — drawn `=` from turn one by the fog above — so only
    // the interior is treated as remembered contents here.
    for duct in state.layout().ducts() {
        let occupied = duct.contains(state.player());
        let path = duct.cells();
        for (i, &c) in path.iter().enumerate() {
            let idx = (c.y * width + c.x) as usize;
            let is_entry = i == 0 || i == path.len() - 1;
            let vis = if occupied {
                // The whole occupied duct is the live layer — you are in it.
                Some(Visibility::Live)
            } else if !is_entry && state.duct_memory().contains(&c) {
                // A **crawled** interior cell is remembered content, exactly like a
                // seen hideout (§11.5a): drawn live while its face is in view,
                // remembered once it is not. The signal is duct memory, not sight
                // memory — looking at the wall band from the room never reveals a
                // crawlspace you have not been inside, so an un-crawled interior stays
                // plain wall (its route given away to nobody).
                Some(if fov.contains(c) {
                    Visibility::Live
                } else {
                    Visibility::Remembered
                })
            } else {
                None
            };
            if let Some(vis) = vis {
                cells[idx] = GlyphCell {
                    glyph: '=',
                    fg: Category::System,
                    bg: None,
                    vis,
                };
            }
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
        put(body.cell(), BODY_GLYPH, fg);
    }
    // A **seen** guard (in the FOV, §9.2) draws as the full state-coloured `g`; the
    // `g` glyph is re-categorised every turn from the guard's state (§11.2): yellow →
    // orange → red is the guard's mind, made visible. A **sensed** guard is a
    // *background* highlight instead, painted below alongside the danger overlay — no
    // glyph of its own. A guard perceived neither way draws nothing and is never
    // remembered (§11.5a), so leaving both view and sense range erases it.
    for guard in state.guards() {
        if state.perceive_guard(guard) == Some(GuardPerception::Seen) {
            put(guard.pos(), GUARD_GLYPH, guard.state().category());
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
    // Inside a duct the only live window is the mouth peek (§6.1/#134): a guard seen
    // through it is real, but the parts of its cone that fall **beyond** the peek cast
    // are cells the player perceives only as memory, so the overlay must not paint
    // them red — everything past the window stays memory (§11.5). On open floor a
    // seen guard's whole cone paints, as §11.5 intends (knowledge you have); the clip
    // is the in-duct case alone, and the player FOV *is* the peek there.
    let in_duct = state.in_duct();
    for guard in state.guards() {
        if !fov.contains(guard.pos()) {
            continue; // an unseen guard's cone is unknown, not safe — just unknown
        }
        let spare_player = state.concealed_from(guard.pos());
        for cell in guard.fov().cells() {
            if spare_player && cell == state.player() {
                continue;
            }
            if in_duct && !fov.contains(cell) {
                continue; // the peek window only — beyond the cast is memory
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
        // A duct **entry** is geometry as well (§10.7): visible from turn one like a
        // door, an `=` in the wall you can plan a shortcut around. Its interior stays
        // Wall — the crawl *path* is contents, hidden until crawled then remembered by
        // the interior view (#134), so nothing here gives the shortcut's route away.
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
        // Contents: hidden until seen, then remembered (§11.5a).
        Terrain::Console | Terrain::Hideout if remembered => (terrain, Visibility::Remembered),
        // Never seen: masked by the geometry naturally in its place — plain floor
        // where a console stands, plain wall over a hideout alcove, so the map
        // gives neither away before the player has scouted it.
        Terrain::Console => (Terrain::Floor, Visibility::Dimmed),
        Terrain::Hideout => (Terrain::Wall, Visibility::Dimmed),
    }
}

mod help;
mod hud;
pub use hud::{
    ability_at, is_ability_button, is_help_button, is_message_button, render_screen, ScreenUi,
    HEADER_ROWS, STATUS_ROWS,
};

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

    /// After the player crawls a duct and climbs out, its interior cells stay
    /// **remembered** as `=` rather than reverting to blank wall (§11.5a) — the
    /// scouted flight path is worth keeping.
    #[test]
    fn a_crawled_duct_is_remembered_after_the_player_leaves() {
        let mut s = duct_state(Vec::new());
        s.step(Input::Step(Direction::North)); // enter (2,1)
        for _ in 0..3 {
            s.step(Input::Step(Direction::East)); // crawl to (5,1)
        }
        s.step(Input::Step(Direction::South)); // climb out at (5,2)
        assert!(!s.in_duct(), "the normal view is restored on the same turn");
        // Walk down the room until the duct band is out of the forward view.
        for _ in 0..3 {
            s.step(Input::Step(Direction::South));
        }
        assert!(
            s.duct_memory().contains(&Cell::new(3, 1)),
            "the crawl is remembered as duct knowledge",
        );
        assert!(
            !s.player_fov().contains(Cell::new(3, 1)),
            "…and the duct is now out of view",
        );
        let g = render(&s);

        for &(x, y) in &[(3, 1), (4, 1)] {
            let c = g.get(x, y);
            assert_eq!(c.glyph, '=', "the crawled interior does not vanish to wall");
            assert_eq!(c.vis, Visibility::Remembered, "…it is remembered, not live");
        }
    }
}
