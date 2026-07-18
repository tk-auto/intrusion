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
use crate::state::State;

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
///
/// # Glyph priority (§11.3)
///
/// The old renderer was last-writer-wins, so a guard standing in a doorway rendered
/// arbitrarily. Here the order is **defined**: entities always draw over terrain, and
/// among entities the ranking is **player > guard** (bodies and decoys slot in when
/// they exist, §7.2/§8.3). We write terrain, then guards, then the player, so the
/// highest-priority glyph is the last writer at any cell — a defined order, not an
/// accident.
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
    for guard in state.guards() {
        // Live state (§11.5a): a guard exists on screen only while the player sees
        // it — never remembered, so leaving the FOV erases it from the picture.
        if !fov.contains(guard.pos()) {
            continue;
        }
        // The guard glyph is re-categorised every turn from its state (§11.2):
        // yellow → orange → red is the guard's mind, made visible. The §7.4
        // transitions are the guard AI tickets; the seam is already honest.
        put(guard.pos(), 'g', guard.state().category());
    }
    // The player, always Owned — trivially inside their own FOV. Inside a hideout
    // the player is concealed: the cupboard keeps its `}` glyph but recolours to
    // Owned (§10.3/§11.3) — the "you are hidden here" signal — instead of drawing
    // the `@`. Read through the same `hidden` query the loop and vision use, so
    // the picture cannot disagree.
    let player_glyph = if state.hidden() { '}' } else { '@' };
    put(state.player(), player_glyph, Category::Owned);

    // The danger overlay (§11.5), last, across terrain and entities alike: the
    // union of every visible guard's cone. Backgrounds compose with whatever
    // glyph is on the cell — a watched guard, a watched player, watched floor.
    for guard in state.guards() {
        if !fov.contains(guard.pos()) {
            continue; // an unseen guard's cone is unknown, not safe — just unknown
        }
        for cell in guard.fov().cells() {
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
        Terrain::Floor | Terrain::Wall | Terrain::DoorHinge | Terrain::Exit => {
            (terrain, Visibility::Dimmed)
        }
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
    use crate::state::{Guard, Input, State};
    use crate::Layout;

    /// A hand-built state on a `w × h` walled box: the player, some guards, and a far
    /// exit, no objectives. Enough to render. Faces **south**, toward where these
    /// tests put their guards — entities are live state (§11.5a) and draw only
    /// inside the FOV, so a guard the test asserts on must be in view.
    fn state(w: u32, h: u32, player: Cell, guards: Vec<Guard>) -> State {
        State::new(
            Layout::from_facility(Facility::walled_box(w, h)),
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

        // A plain floor cell renders as a dot (§11.5), Neutral.
        assert_eq!(g.get(5, 5).glyph, '·');
        assert_eq!(g.get(1, 1).fg, Category::Neutral); // interior floor
    }

    /// §11.2's payoff, on screen: the `g` glyph is re-categorised every turn from
    /// the guard's §7.4 state, so a chasing guard reads **Danger** — the player
    /// sees the AI state machine as yellow → orange → red, and no game system ever
    /// named a colour to do it.
    #[test]
    fn a_guards_glyph_category_tracks_its_state() {
        use crate::state::GuardState;
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
            Layout::from_facility(Facility::walled_box(10, 10)),
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
        let mut layout = Layout::from_facility(Facility::walled_box(10, 10));
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

    /// §11.5a: **geometry is never fogged.** Walls far beyond sight range — and the
    /// exit, part of the layout the player entered by — draw from turn one, so a
    /// route can be planned before the first risky step. Out-of-FOV geometry
    /// carries [`Visibility::Dimmed`]; what the player sees now is `Live`.
    #[test]
    fn geometry_draws_from_turn_one_even_far_out_of_sight() {
        let mut layout = Layout::from_facility(Facility::walled_box(40, 30));
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
    /// persist out of the FOV.
    #[test]
    fn contents_are_remembered_but_live_state_is_not() {
        // Player at (10,10) facing north; a console and a guard behind them to the
        // south, outside the half-disc (§6.2 sees at most one row behind — the
        // touching ring).
        let mut s = State::new(
            Layout::from_facility(Facility::walled_box(20, 20)),
            Cell::new(10, 10),
            Direction::North,
            vec![Guard::stationary(Cell::new(12, 14))],
            [Cell::new(10, 14)],
            Cell::new(18, 18),
        );

        // Never seen: the intel masks as plain floor and the guard is not drawn.
        let g = render(&s);
        assert_eq!(g.get(10, 14).glyph, '·', "unseen intel is invisible");
        assert_eq!(
            g.get(10, 14).fg,
            Category::Neutral,
            "…its cell reads as floor"
        );
        assert_eq!(g.get(12, 14).glyph, '·', "an unseen guard is not drawn");

        // Turn south: both enter the FOV, live.
        s.step(Input::Step(Direction::South)); // to (10,11), facing south
        let g = render(&s);
        let intel = g.get(10, 14);
        assert_eq!(
            (intel.glyph, intel.fg, intel.vis),
            ('$', Category::Interest, Visibility::Live)
        );
        let guard = g.get(12, 14);
        assert_eq!((guard.glyph, guard.vis), ('g', Visibility::Live));

        // Turn back north: the intel stays, remembered; the guard vanishes.
        s.step(Input::Step(Direction::North)); // to (10,10), facing north
        let g = render(&s);
        let intel = g.get(10, 14);
        assert_eq!(
            (intel.glyph, intel.fg, intel.vis),
            ('$', Category::Interest, Visibility::Remembered),
            "seen intel stays on the map after leaving the FOV, as memory"
        );
        assert_eq!(
            g.get(12, 14).glyph,
            '·',
            "a guard does not persist out of FOV"
        );
        assert_eq!(g.get(12, 14).vis, Visibility::Dimmed);
    }

    /// §11.5a's scouting reward: an unscouted hideout reads as plain **wall** — the
    /// alcove gives nothing away until the player has actually seen it. Once seen
    /// it is remembered like any content.
    #[test]
    fn an_unseen_hideout_masks_as_wall_until_scouted() {
        let mut layout = Layout::from_facility(Facility::walled_box(20, 20));
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
        let mut layout = Layout::from_facility(Facility::walled_box(20, 20));
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
        let mut layout = Layout::from_facility(Facility::walled_box(20, 20));
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
            Layout::from_facility(Facility::walled_box(20, 20)),
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
            Layout::from_facility(Facility::walled_box(20, 20)),
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
    /// paints nothing. Its cone is unknown information — painting it would leak
    /// what the player has not scouted ("no guard *you can see* will detect you").
    #[test]
    fn an_unseen_guards_cone_paints_nothing() {
        // The guard stands behind the north-facing player, out of the FOV.
        let s = State::new(
            Layout::from_facility(Facility::walled_box(20, 20)),
            Cell::new(10, 10),
            Direction::North,
            vec![Guard::stationary(Cell::new(10, 14))],
            Vec::new(),
            Cell::new(18, 18),
        );
        assert!(!s.player_fov().contains(Cell::new(10, 14)));

        let g = render(&s);
        for y in 0..g.height() {
            for x in 0..g.width() {
                assert_eq!(g.get(x, y).bg, None, "no red anywhere for ({x},{y})");
            }
        }
    }
}
