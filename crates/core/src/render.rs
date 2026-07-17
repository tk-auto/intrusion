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
//! What is **not** here yet: fog and tile memory (§11.5a), the FOV dimming and the
//! danger overlay (§11.5) — those set a cell's background and land with vision (§6);
//! until then every [`GlyphCell::bg`] is `None`. Colour *values* are the shell's
//! table (§11.2); this module only speaks in categories.

use crate::category::Category;
use crate::facility::Facility;
use crate::state::State;

/// One rendered cell: a glyph, its foreground category, and an optional background
/// category (§11.1). `bg` is `None` until the FOV dimming and danger overlay (§11.5)
/// land with vision — today nothing paints a background.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct GlyphCell {
    /// The character to draw; a space is empty (floor), painted as background only.
    pub glyph: char,
    /// What the glyph *means* (§11.2). The shell maps this to a colour.
    pub fg: Category,
    /// The background category, or `None` for the default backdrop. Reserved for the
    /// FOV/danger overlay (§11.5); unused until vision lands.
    pub bg: Option<Category>,
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

/// Render `state` to a full [`Grid`] (§11.1): terrain first, then every entity on
/// top, resolving overlaps by the glyph priority below.
///
/// # Glyph priority (§11.3)
///
/// The old renderer was last-writer-wins, so a guard standing in a doorway rendered
/// arbitrarily. Here the order is **defined**: entities always draw over terrain, and
/// among entities the ranking is **player > guard** (bodies and decoys slot in when
/// they exist, §7.2/§8.3). We write terrain, then guards, then the player, so the
/// highest-priority glyph is the last writer at any cell — a defined order, not an
/// accident.
pub fn render(state: &State) -> Grid {
    let facility = state.layout().facility();
    let (width, height) = (facility.width(), facility.height());

    // Terrain layer: every cell is its terrain's glyph and category.
    let mut cells: Vec<GlyphCell> = (0..height)
        .flat_map(|y| (0..width).map(move |x| (x, y)))
        .map(|(x, y)| {
            let terrain = facility
                .terrain_at(x, y)
                .expect("in-bounds by construction");
            GlyphCell {
                glyph: terrain.glyph(),
                fg: terrain.category(),
                bg: None,
            }
        })
        .collect();
    let mut put = |cell: crate::cell::Cell, glyph: char, fg: Category| {
        cells[(cell.y * width + cell.x) as usize] = GlyphCell {
            glyph,
            fg,
            bg: None,
        };
    };

    // Entity layers, lowest priority first so the top entity is the last writer.
    for guard in state.guards() {
        // The guard glyph is re-categorised each turn from its state (§11.2). The
        // state machine (§7.4) is a later ticket; an un-alerted guard is Caution.
        put(guard.pos(), 'g', Category::Caution);
    }
    put(state.player(), '@', Category::Owned);

    Grid {
        width,
        height,
        cells,
    }
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
    use crate::state::{Guard, State};
    use crate::Layout;

    /// A hand-built state on a `w × h` walled box: the player, some guards, and a far
    /// exit, no objectives. Enough to render.
    fn state(w: u32, h: u32, player: Cell, guards: Vec<Guard>) -> State {
        State::new(
            Layout::from_facility(Facility::walled_box(w, h)),
            player,
            Direction::North,
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

        // A plain floor cell keeps its terrain glyph/category.
        assert_eq!(g.get(5, 5).glyph, ' ');
        assert_eq!(g.get(1, 1).fg, Category::Neutral); // interior floor
    }

    /// Glyph priority is *defined*, not last-writer-wins (§11.3): an entity always
    /// wins over the terrain beneath it, and the player wins over a guard. The old
    /// bug rendered a guard-in-a-doorway arbitrarily; here the order is fixed.
    #[test]
    fn entities_win_over_terrain_and_the_player_wins_over_a_guard() {
        // A guard standing on a console ($, terrain) renders as the guard, not the $.
        let s = State::new(
            Layout::from_facility(Facility::walled_box(10, 10)),
            Cell::new(2, 2),
            Direction::North,
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

    /// Terrain categories follow §11.2: an exit and a console are Interest, a hideout
    /// and a door are System, walls are Neutral.
    #[test]
    fn terrain_carries_its_category() {
        assert_eq!(Terrain::Wall.category(), Category::Neutral);
        assert_eq!(Terrain::Exit.category(), Category::Interest);
        assert_eq!(Terrain::Console.category(), Category::Interest);
        assert_eq!(Terrain::Hideout.category(), Category::System);
        assert_eq!(Terrain::DoorPanelClosed.category(), Category::System);
    }
}
