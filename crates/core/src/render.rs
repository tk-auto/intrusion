//! Rendering as a pure function of state (§11.1, §12.1) — **the one place rendering
//! lives**.
//!
//! The game draws as a grid of cells, each a character plus a foreground *category*
//! plus a background (§11.1). This is a **pure function of [`State`]**: it composes
//! the terrain grid **and** the entities on it — the player, the guards, the bodies,
//! the remote and the decoys — into one grid, resolving overlaps by a defined
//! **glyph priority** (§11.3). Because it prints as text it is assertable in a native test with no
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
//! lie. What is deliberately **not** here: the two red shades of the §7.6 two-zone
//! detection (certain vs glimpse). The zones themselves shipped
//! ([`Sighting`](crate::guard::Sighting)) — painting the whole cone as one zone is
//! a render choice, kept until the overlay earns a second shade (§11.5). Colour
//! *values* are the shell's table (§11.2); this module only speaks in categories.

use crate::category::{Category, Theme};
use crate::cell::{Cell, Direction};
use crate::facility::{Facility, Terrain};
use crate::modifiers::LayoutKnowledge;
use crate::state::{GuardPerception, State};

/// The entity glyphs (§11.3), named once so the world render and the help legend
/// (#139) draw the same characters — a legend that hand-copied them could drift from
/// what the game shows. Terrain glyphs already have their single source in
/// [`Terrain::glyph`]; these are the entity half of the §11.3 table.
pub(crate) const PLAYER_GLYPH: char = '@';
pub(crate) const GUARD_GLYPH: char = 'g';
pub(crate) const BODY_GLYPH: char = 'z';
/// A **remote unit** of yours in the facility (§8.3/§11.3/#273) — the drone. A small
/// mark for a small machine, and one no terrain or actor already speaks: the `@` is
/// yours and the decoy's, `g` and `z` are the guards' living and dead.
pub(crate) const REMOTE_GLYPH: char = '*';
/// Floor draws as a dot **while you can see it**, and as nothing once you cannot
/// (§11.5, #470): the dot is the FOV's own ink, so the sight boundary is the edge
/// between dots and bare page rather than a gap between two shades of dot. Named so
/// the legend shows the same mark the board does.
pub(crate) const FLOOR_DOT: char = '·';

/// The **schematic** mark (§11.5a, #307/#470): how geometry the player has never had
/// eyes on is drawn — the building as its *plans* give it, not as it has been seen.
///
/// A cell that has never been in the player's FOV collapses to this one mark or to
/// nothing at all: `□` is the building's **fabric** — a wall run and the recesses and
/// openings cut into it — and the **floor space** between it is left blank. Standing
/// somewhere resolves the schematic into what is really there, and permanently —
/// tile memory is monotonic (§11.5a).
///
/// **Why an outline square.** Fabric fills its cell the way `#` does, so a wall run
/// on the plan reads as structure; a baseline-hugging mark like `~` drew the same run
/// as a dotted line, which is the wrong reading for the load-bearing half of a plan.
/// It carries roughly a third of `#`'s ink, so the plan stays quieter than the
/// building, and it cannot be mistaken for `#`, `+`, `×` or `=`. The `≈` it replaced
/// could: at the cell sizes the board is fitted to, a double tilde reads as an equals
/// sign, and `=` is the duct mouth — so the mark for *unseen fabric* looked like a
/// specific piece of terrain, and an unseen duct mouth is itself fabric, which put the
/// confusion exactly where it cost most (#470).
///
/// **Why shape rather than a darker shade.** The obvious alternative was a fourth
/// rung on the §11.5 brightness ladder, and it is the worse channel: on a dark
/// palette the gap below Ground's already-quiet dim is too small to read on a
/// phone, and pushing it darker turns the readout into de-facto fog, which §11.5a
/// settles against. Shape costs no colour, cannot compete with the threat channels
/// (§11.2 Danger and Sensed keep the background to themselves), and needs nothing
/// extra from a second palette (#189).
pub(crate) const SCHEMATIC_WALL: char = '□';

/// How much the player currently knows about what a drawn cell shows — the
/// visual states of §11.5a's implementation note (live / remembered / never-seen).
/// The shell styles each distinctly; remembered must **not** be collapsed into the
/// §11.5 dimming scheme.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Visibility {
    /// Inside the player's FOV right now — drawn full colour (§11.5).
    Live,
    /// Outside the FOV but the cell has been in it before — geometry the player
    /// has stood in and looked at, drawn as itself in the §11.5 dim shade: dark
    /// grey, dim but legible.
    Explored,
    /// Never in the player's FOV — geometry known from the building's plans and
    /// nothing else (§11.5a). Drawn as the **schematic** (see [`SCHEMATIC_WALL`]):
    /// the fabric of the building, with the floor between it — and everything
    /// standing in the rooms yet to be discovered — left blank.
    ///
    /// It is the *knowledge* that is recorded here, not the styling — the shell
    /// paints this in the same dim shade as [`Explored`](Self::Explored), because
    /// the schematic separates itself by **shape**. That keeps the distinction on
    /// the seam for anything that needs to reason about coverage rather than draw
    /// it, and it is what lets the §12.6 layout knob
    /// ([`LayoutKnowledge`](crate::LayoutKnowledge)) move what such a cell *shows* —
    /// the real building at one end, nothing at all at the other — without moving what
    /// it honestly *is*. An unexplored cell reports itself unexplored under all three
    /// settings.
    Unexplored,
    /// Outside the FOV, drawn from tile memory: a content seen earlier this run
    /// (§11.5a) — its own visual state, distinct from both live and explored.
    Remembered,
}

/// How strongly a cell's **background** paints (§11.4/§11.5/#420) — which of the two
/// fills every palette row carries this cell is asking for.
///
/// Three surfaces ask the question and they answer it from different facts. The **map**
/// answers from fog: a cell inside the FOV paints the full fill, a cell beyond it the
/// quiet one ([`Fill::fogged`]). The **HUD** has no fog to consult and answers from
/// what the row is: a band announcing something that just happened paints full, and
/// the ambient floor — a standing fact, permanently on screen — paints quiet, so the
/// row's colour distinguishes *the facility's mood* from *something that just
/// happened*. The **sense channel** (§9/#192) answers from **age**: everything it marks
/// is outside the FOV by construction and is certain knowledge regardless (§11.5a), so
/// fog has nothing to say about it — what its two strengths carry instead is a mark made
/// this turn (full) against the fading tail behind it (quiet).
///
/// Carrying the answer rather than the reason is the point. Reaching for
/// [`Visibility::Explored`] on a HUD row would pick the quiet fill for free and would
/// be a lie: `Visibility` means fog knowledge and a status row has none, so every
/// later reader of that row would have to know the lie in order to read the code.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Fill {
    /// The full-strength fill: a watched cell in view, a band announcing an event.
    #[default]
    Full,
    /// The quieter fill the same row carries: a watched cell beyond the FOV (§11.5
    /// fix #1 — still visibly watched, never safe-looking), an ambient HUD band.
    Quiet,
}

impl Fill {
    /// The **map's** rule (§11.5): full inside the field of view, quiet beyond it.
    /// Every cell of the board is built through this, so the fog and the fill can
    /// never drift apart.
    pub fn fogged(vis: Visibility) -> Self {
        match vis {
            Visibility::Live => Self::Full,
            Visibility::Explored | Visibility::Unexplored | Visibility::Remembered => Self::Quiet,
        }
    }
}

/// Which **surface** a cell belongs to: the facility being drawn, or the frame drawn
/// around and over it (§11.4).
///
/// This is a fact about *what a cell is*, not about how it looks — the same kind of
/// presentation-neutral declaration [`Category`] and [`Visibility`] are, and it names
/// no colour and no glyph for the same §11.2 reason. The core has always known the
/// answer: it is the difference between [`GlyphCell::on_board`] and
/// [`GlyphCell::blank`], the two constructors every cell in every render comes from.
/// #460 gave it a name because a renderer needs it.
///
/// **Why a renderer needs it.** A tile renderer draws a sprite chosen by the glyph, so
/// it can only ever be right on cells whose glyph *is* the world. On chrome the glyph
/// is a letter of a sentence, and a sentence run through a glyph → sprite table shows a
/// guard wherever it happens to contain a `g`. Row geometry cannot answer this: the
/// deployed message log (§11.7) and the verdict card (§14 v2) lay prose **across the
/// map rows**, so "which rows are the map" is the wrong question and "what is this
/// cell" is the right one.
///
/// Nothing about the text renderer changes: it draws both the same way, which is why
/// the picture is identical whether or not anything reads this.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Surface {
    /// A cell **of the facility** — terrain, an entity, a mark on the ground. Its
    /// glyph says what is there (§11.3).
    Board,
    /// A cell of the **frame**: a status line, the ability bar, a panel, the deployed
    /// log, the verdict card. Its glyph is a character of text.
    Chrome,
}

/// One rendered cell: a glyph, its foreground category, an optional background
/// category (§11.1), the knowledge state it is drawn in (§11.5a), how strongly that
/// background paints (§11.4/#420), and which surface it belongs to (§11.4/#460).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct GlyphCell {
    /// The character to draw; a space is empty, painted as background only.
    pub glyph: char,
    /// What the glyph *means* (§11.2). The shell maps this to a colour.
    pub fg: Category,
    /// The background category, or `None` for the default backdrop. `Danger` is
    /// the §11.5 overlay: this cell is watched by a guard the player can see.
    pub bg: Option<Category>,
    /// The knowledge state this cell is drawn in (§11.5a): live, explored
    /// geometry, or remembered content. The shell styles the three distinctly.
    ///
    /// It styles the **glyph** and nothing else. Which of the row's two background
    /// fills paints is [`fill`](Self::fill)'s question (#420).
    pub vis: Visibility,
    /// Which fill [`bg`](Self::bg) paints in (§11.4/§11.5). Meaningless — and
    /// ignored — when `bg` is `None`.
    pub fill: Fill,
    /// Whether this cell is the facility or the frame around it ([`Surface`]).
    /// Carried per cell rather than derived from the row, because the log and the
    /// verdict lay chrome *over* the map (#460).
    pub surface: Surface,
    /// Which way the thing in this cell is **facing**, or `None` when it faces
    /// nowhere — which is every cell but an actor's (§11.1/#461).
    ///
    /// **Why the grid carries it at all.** A facing cannot be derived from the
    /// picture: `@` is `@` whichever way you are turned, so a renderer that wanted to
    /// draw a turned sprite would have to read [`State`] — and the moment a renderer
    /// reads state, the grid stops being the single interface and the two renderers
    /// can disagree about a cell without any test noticing (§11.1 **[SETTLED]**). So
    /// the fact travels the way every other fact does. It is a statement about the
    /// *actor*, not about tiles: a facing is as presentation-neutral as a
    /// [`Category`], and the ASCII renderer ignores it exactly as it ignores nothing
    /// else — the character grid is unchanged, byte for byte, by this field existing.
    ///
    /// **What may carry one, and what may not.** The player (§5 makes "you cannot see
    /// behind you" a rule, so drawing which way you face *adds* information, and that
    /// addition is deliberate) and a guard the player can **see**. Never a **sensed**
    /// guard: §9.2 gives the sense position and nothing else — "no facing, no cone" —
    /// and a sensed guard has no glyph in the first place, so a facing here would hand
    /// over through the shape channel the one thing that channel is defined not to
    /// give.
    pub facing: Option<Direction>,
}

impl GlyphCell {
    /// A **board** cell (§11.5): a glyph in its category, at a knowledge state, with
    /// no background of its own yet.
    ///
    /// Its fill follows its fog by construction ([`Fill::fogged`]), so an overlay that
    /// later paints a background on it — the §11.5 danger red, the §9.2 sensed orange —
    /// gets the right strength without having to ask, and the two can never drift apart.
    pub(crate) fn on_board(glyph: char, fg: Category, vis: Visibility) -> Self {
        Self {
            glyph,
            fg,
            bg: None,
            vis,
            fill: Fill::fogged(vis),
            surface: Surface::Board,
            facing: None,
        }
    }

    /// The same **board** cell, turned: what is drawn here faces `facing` (§11.1/#461).
    ///
    /// A separate constructor rather than a fifth argument to
    /// [`on_board`](Self::on_board), because a facing is the rare case — every cell of
    /// the terrain layer and every mark on the ground faces nowhere — and threading an
    /// `Option` through the hundreds of calls that would always pass `None` would put
    /// the exception in front of the rule.
    pub(crate) fn facing(glyph: char, fg: Category, vis: Visibility, facing: Direction) -> Self {
        Self {
            facing: Some(facing),
            ..Self::on_board(glyph, fg, vis)
        }
    }

    /// An empty, live, uncoloured cell — the starting point of every **chrome** surface
    /// (a status row, a panel, the deployed log), which has no fog to consult and paints
    /// its own bands (§11.4).
    ///
    /// Every chrome cell in every render is built from this one, including the
    /// overlays that write *over* the map, so [`Surface::Chrome`] arrives here and
    /// nowhere else — which is what makes the split exhaustive rather than a list
    /// somebody has to remember to extend.
    pub(crate) fn blank() -> Self {
        Self {
            glyph: ' ',
            fg: Category::Neutral,
            bg: None,
            vis: Visibility::Live,
            fill: Fill::Full,
            surface: Surface::Chrome,
            facing: None,
        }
    }
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
/// — a console, a hideout, a duct mouth — draw only inside the current FOV or, once
/// their cell is in tile memory, as [`Visibility::Remembered`]; never seen, the cell
/// masks as the geometry naturally in its place (floor under a console, wall over a
/// hideout alcove or a duct mouth — the scouting reward of §11.5a). **Live state** — guards, and a door's
/// open/closed pose — draws only inside the FOV and is never remembered: an
/// out-of-view panel always shows its canonical closed `+`, whatever it really is.
/// The one exception is the **sense channel** (§9, #192), which is one system with two
/// halves and one persist-and-fade model. A guard out of the FOV but inside the
/// guard-sense box gets a full-strength orange `Sensed` background on its exact cell —
/// position only, no cone — and the cells it was felt in over the last couple of turns
/// keep a quieter mark behind it, a **trail** that says *was just here* and fades to
/// nothing (never a heading, §9.2). A door's *change* is sensed the same way, in the
/// same [`Category::Sensed`] channel, at its own longer range and its own slightly
/// longer life (§9.4/§10.4): a door that opens or shuts away from the player fades over
/// its **whole footprint** — evidence someone passed, also position only and also
/// painted through walls. None of it is *remembered*: a mark is the sense's own short
/// clock running out, not tile memory (§11.5a). The **decoy** is the other exception
/// (§8.3/#321): it is the player's
/// own placed object rather than the facility's live state, so it draws at its cell in
/// or out of the FOV, `Remembered` while unseen — see the decoy layer below.
///
/// # Glyph priority (§11.3)
///
/// The old renderer was last-writer-wins, so a guard standing in a doorway rendered
/// arbitrarily. Here the order is **defined**: entities always draw over terrain, and
/// among glyphs the ranking is **player > guard > body > remote > decoy** (§7.2/§8.3/
/// #273). We write terrain, then the decoy, then the remote, then bodies, then seen
/// guards, then the player, so
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
/// # The effect layer (§8.3/§11.5, #308/#338)
///
/// An **ability effect of the player's own making** always colourises the
/// **background**, in `Category::Effect`, and never the glyph: the glyph keeps its own
/// meaning (a guard's threat ladder, `Owned` for a thing of yours) and the effect is
/// the wash underneath it. The core owns *where* and *how long* — an explicit cell set
/// or the thing in a cell, momentary or standing (`crate::state::effects`) — so a new
/// effect becomes visible without this function changing at all.
///
/// It reads two queries because the two placements make different claims:
/// [`State::effect_cell_marks`] is the **wash**, the weakest background on the board,
/// and [`State::effect_thing_marks`] is a **recolour of a cue the thing already
/// draws**, which is why it outranks the `Sensed` channel it refines rather than
/// competing with it. The precedence is pinned by paint order — `Danger` > a mark on a
/// thing > `Sensed` > the investigation area > the wash — because an advisory layer
/// must never masquerade as the detection set, nor hide it (§11.5 **[SETTLED]**).
///
/// # The investigation area (§7.6/§11.5, #224)
///
/// A second advisory layer under that same contract, switched on by the
/// `show_search_areas` modifier (§12.6): the box every guard in a §7.6 search is
/// sweeping washes orange (`Category::Warning`), so *where* a search is combing is on
/// the board. Baseline it is off and a search is legible in **time** only — the near
/// line says when one opens and when it is called off (§11.7). Orange says a guard's
/// attention is on this ground; that you are *detected* stays red's word alone.
///
/// # Floor dots (§11.5/#470)
///
/// Floor you can see draws `·`; floor you cannot draws blank. The dot exists so the
/// FOV boundary reads across open ground — without a foreground there is nothing for
/// the sight edge to act on, and you could only see it where it crossed a wall. It
/// now carries that job by **being the FOV's own ink**: the boundary is the edge
/// between dotted ground and bare page, a harder line than the two shades of dot it
/// replaced. An open door panel stays blank in every state (§10.3): the gap in the
/// wall *is* its rendering.
pub fn render(state: &State) -> Grid {
    let facility = state.layout().facility();
    let (width, height) = (facility.width(), facility.height());

    // The board is painted in fixed passes, last writer wins. Glyphs first —
    // terrain under entities, the player over everything — then the recolours
    // and remembered facts, then the backgrounds from the weakest cue up to the
    // danger overlay, which always paints last and always wins (§11.5).
    let mut cells = terrain_pass(state);
    spent_console_recolour(state, &mut cells);
    locked_door_recolour(state, &mut cells);
    duct_pass(state, &mut cells);
    entity_pass(state, &mut cells);
    crouch_signal(state, &mut cells);
    stowed_body_memory(state, &mut cells);
    effect_wash(state, &mut cells);
    search_area_wash(state, &mut cells);
    sense_mark_wash(state, &mut cells);
    watcher_line_pass(state, &mut cells);
    sensed_guard_wash(state, &mut cells);
    effect_thing_wash(state, &mut cells);
    danger_overlay(state, &mut cells);

    Grid {
        width,
        height,
        cells,
    }
}

/// The terrain layer, through the fog: what the player knows of each cell.
///
/// The §12.6 layout knob moves what a never-seen cell is worth in both directions:
/// [`LayoutKnowledge::Full`] draws the architecture of cells the player has never had
/// eyes on instead of the schematic (contents stay hidden — it buys the building, not
/// the objectives), and [`LayoutKnowledge::None`] draws nothing there at all.
fn terrain_pass(state: &State) -> Vec<GlyphCell> {
    let facility = state.layout().facility();
    let (width, height) = (facility.width(), facility.height());
    let fov = state.player_fov();
    let memory = state.memory();
    let knowledge = state.modifiers().layout_knowledge;

    (0..height)
        .flat_map(|y| (0..width).map(move |x| (x, y)))
        .map(|(x, y)| {
            let terrain = facility
                .terrain_at(x, y)
                .expect("in-bounds by construction");
            let cell = Cell::new(x, y);
            let Fogged {
                shown,
                vis,
                schematic,
            } = if fov.contains(cell) {
                Fogged {
                    shown: terrain,
                    vis: Visibility::Live,
                    schematic: false,
                }
            } else {
                fogged_view(terrain, memory.contains(cell), knowledge)
            };
            // Floor dots (§11.5/#470): give open ground a foreground so the FOV edge
            // reads across it — and **only** inside the FOV, so the edge is dots
            // against bare page rather than two shades of dot. Masked contents dot
            // too, while they are in sight: they *show* floor.
            //
            // Unexplored geometry draws the schematic instead (§11.5a/#307).
            // `fogged_view` has already collapsed it to bare wall or bare floor, so
            // the swap is total: unexplored fabric wears the one mark, unexplored
            // floor space wears none, and neither can be told apart from its
            // neighbours by glyph *or* by category.
            let glyph = match (schematic, shown) {
                (true, Terrain::Wall) => SCHEMATIC_WALL,
                // Out-of-FOV floor — remembered or schematic alike — falls through to
                // `Terrain::Floor`'s own blank glyph.
                (_, Terrain::Floor) if vis == Visibility::Live => FLOOR_DOT,
                _ => shown.glyph(),
            };
            GlyphCell::on_board(glyph, shown.category(), vis)
        })
        .collect()
}

/// A spent objective is Neutral scenery (§11.2): once its intel is taken a console
/// stops being a live goal, so it recolours from Interest to Neutral while keeping
/// its `$` glyph — "there was intel here, it's collected" — instead of staying
/// indistinguishable from a live console. Terrain stays `Console` (geometry is
/// static); only the category changes, so the core stays colour-blind (§11.2) and
/// the shell's one table owns the actual colour. Runs on the terrain layer, before
/// the entity/overlay passes, so a guard or the player standing on a spent console
/// still draws over it. Taking intel requires reaching (thus seeing) the console and
/// memory is monotonic (§11.5a), so a spent console is always at least remembered —
/// recolour only where it actually shows, both live and in memory, never a masked
/// floor dot standing in for a never-seen console.
fn spent_console_recolour(state: &State, cells: &mut [GlyphCell]) {
    let width = state.layout().facility().width();
    let fov = state.player_fov();
    let memory = state.memory();
    for cell in state.spent_consoles() {
        if fov.contains(cell) || memory.contains(cell) {
            cells[(cell.y * width + cell.x) as usize].fg = Category::Neutral;
        }
    }
}

/// A **key-gated** door the player cannot open is Neutral scenery (§11.2/§10.4/#236):
/// the doorways of the locked prize room keep their `+`/`×` glyphs and recolour from
/// the working-furniture tan to the same white a spent console wears, because that is
/// what they are to a player without a key — a door-shaped wall.
///
/// It is the spent-console recolour's exact shape, and it says the same kind of thing:
/// *this looks like a thing you use, and it is not one*. The moment a takedown puts a
/// key in hand ([`State::holds_key`]) every one of them goes back to System tan on the
/// next frame, which is the payoff for the price the player just paid made visible on
/// the board rather than only in a message that has since scrolled away.
///
/// Recoloured only where the door actually shows, live or in memory — never on a
/// schematic cell standing in for geometry the player has not walked (§11.5a). Which
/// room the building keeps locked is something you learn by looking at it, or off the
/// run's card (§12.6); the fog does not give it away.
fn locked_door_recolour(state: &State, cells: &mut [GlyphCell]) {
    if state.holds_key() {
        return;
    }
    let width = state.layout().facility().width();
    let fov = state.player_fov();
    let memory = state.memory();
    for cell in state.keyed_door_cells() {
        if fov.contains(cell) || memory.contains(cell) {
            cells[(cell.y * width + cell.x) as usize].fg = Category::Neutral;
        }
    }
}

/// The duct interior view (§11.5a/§10.7, #134). A duct's path is shown **only
/// while the player is crawling it**: the whole occupied run lights as one
/// connected `=` — the crawlspace read as a single space (the player's own cell is
/// overwritten by the entity pass's `@`; glyph priority `@` > `=`). The interior
/// carries no tell on the base map and is never remembered once the player climbs
/// out (§11.5a): the path lives in its own layer, so the shortcut's route is given
/// away to nobody. The two **entries** are the exception — they are geometry, drawn
/// `=` from turn one by the terrain fog whether occupied or not — so nothing here
/// needs to draw an unoccupied duct at all.
///
/// **The exit keeps its own face** (§11.5a/#466). The player's own tunnel lights by this
/// same rule — one connected run from the border cell to the mouth, which is what turn
/// one opens on, a bright line pointing from where you are to where you are about to be
/// — but `E` itself is left alone. §11.5a draws it as itself from turn one, and it is
/// *yours*: not an anonymous `=`.
fn duct_pass(state: &State, cells: &mut [GlyphCell]) {
    let facility = state.layout().facility();
    let width = facility.width();
    if let Some(duct) = state.occupied_duct() {
        // **Your own tunnel wears the exit's colour** (§11.2/#466). A found shortcut is
        // System — the furniture band, where the doors and cupboards are — but the
        // tunnel is the thing `E` anchors, so lighting the run in Interest makes the
        // opening frame one continuous purple line from the border to the mouth rather
        // than a grey thread ending in a purple letter. Same glyph either way: `=` is
        // what a crawlspace is (§11.3), and a second `E` on the board would be a lie
        // about where the mouth is.
        let band = if duct.way_out().is_some() {
            Category::Interest
        } else {
            Category::System
        };
        for &c in duct.cells() {
            if facility.terrain(c) == Some(Terrain::Exit) {
                continue;
            }
            cells[(c.y * width + c.x) as usize] = GlyphCell::on_board('=', band, Visibility::Live);
        }
    }
}

/// Entity layers, lowest priority first so the top entity is the last writer.
///
/// The decoy (§8.3) draws lowest: an Owned `@` — a thing you made wearing
/// your own glyph, which is the whole trick (§10.3/§11.3). Alone among the
/// entities it draws **wherever it is**, in the FOV or out of it (§11.5a's
/// second exception, #321): the whole point of a fake is to walk away from it
/// and let a guard investigate the wrong cell, so a marker you can only see by
/// standing next to it is a marker the ability cannot use. Its cell is the
/// player's own knowledge, on the same footing as their own position — not a
/// content of the facility they have to keep looking at. It leaks nothing:
/// a decoy dies the turn anything steps on it, and that death already flips
/// the §11.4 bar into cooldown and prints the §11.7 message unconditionally,
/// so the `@` vanishing only puts a fact the player is already told where they
/// are already looking. Out of view it draws `Remembered`, not `Live`: the
/// marker persists at full Owned colour while the three-state discipline
/// (§11.5a) keeps telling the truth about what is actually being seen.
fn entity_pass(state: &State, cells: &mut [GlyphCell]) {
    let facility = state.layout().facility();
    let width = facility.width();
    let fov = state.player_fov();
    if let Some(decoy) = state.decoy() {
        // **The decoy wears your stance as well as your glyph** (§8.3/#461). It has no
        // facing of its own — it is a thing you put down, not a thing that looks — but a
        // decoy the player can tell from themselves at a glance is a decoy that has
        // stopped being a copy, and a renderer able to make that distinction would be
        // carrying information the character grid does not (§11.1). Reflecting the
        // player's own facing gives the two `@`s one appearance without inventing a
        // fact: what is drawn is *your* facing, on both of the cells that are you.
        cells[(decoy.y * width + decoy.x) as usize] = GlyphCell::facing(
            PLAYER_GLYPH,
            Category::Owned,
            if fov.contains(decoy) {
                Visibility::Live
            } else {
                Visibility::Remembered
            },
            state.facing(),
        );
    }

    // A **remote** of yours (§8.1/§8.3/#273) draws with the decoy, on the same terms and
    // for the same reason (§11.5a's second exception, #321): it is a machine you put
    // there, so its cell is your own knowledge rather than a content of the facility you
    // have to keep looking at, and a marker you could only see by standing next to it
    // would be one the ability cannot use. It is trivially in view anyway — it is inside
    // its own camera, and that camera is unioned into the FOV (§6/#273) — so unlike the
    // decoy it needs no remembered branch: while it exists, it is being seen.
    //
    // It draws **below** the guards and bodies deliberately (§11.3's priority): a remote
    // flies over everything, and a threat is never hidden by a thing of yours. What says
    // *this is the one your keys move* is the §11.5 effect mark underneath it, which no
    // glyph can cover.
    //
    // It faces nowhere: a drone's camera is the full circle (§6.2), so there is no
    // stance to draw and none is invented.
    if let Some(remote) = state.remote() {
        cells[(remote.cell().y * width + remote.cell().x) as usize] =
            GlyphCell::on_board(REMOTE_GLYPH, Category::Owned, Visibility::Live);
    }

    // Entities are live state: whatever is drawn here is being seen right now. The
    // fourth argument is the §11.1 facing (#461) — `None` for everything that faces
    // nowhere, which is every entity but an actor.
    let mut put = |cell: Cell, glyph: char, fg: Category, facing: Option<Direction>| {
        cells[(cell.y * width + cell.x) as usize] = match facing {
            Some(facing) => GlyphCell::facing(glyph, fg, Visibility::Live, facing),
            None => GlyphCell::on_board(glyph, fg, Visibility::Live),
        };
    };
    // A body (§7.2) is live state like any entity — drawn inside the FOV as the `z`
    // a downed guard reads as (§10.3), in Caution: an unaware threat's colour,
    // because what a loose body *means* is trouble waiting to be found (§11.3). The
    // body **in your hands** (§8.3) speaks the Owned vocabulary instead — yours while
    // you hold it, and really in play. A body **stowed in a cupboard** (§7.2) is
    // neither: it is gone to every guard, you cannot climb in after it and bumping it
    // is an inert no-op, which is Neutral's own definition (§11.2, "inert scenery,
    // spent objectives") and the same transition a spent console makes. So the locked
    // cupboard keeps its `z` — the glance that tells you which cupboards you have
    // spent (§10.3) — in **Neutral**, leaving Owned to mean only "this is working for
    // you right now". That matters on this furniture in particular: Owned is also the
    // "you are concealed here" signal on `}` (§10.3), and one colour cannot say both
    // *you are hidden in this cupboard* and *this cupboard is spent*. A **loose** body
    // is never remembered; the locked-cupboard status **is**, persisted out of view by
    // [`stowed_body_memory`].
    for body in state.bodies() {
        if !fov.contains(body.cell()) {
            continue;
        }
        let fg = if state.dragging() == Some(body.cell()) {
            Category::Owned
        } else if facility.terrain(body.cell()) == Some(Terrain::Hideout) {
            Category::Neutral
        } else {
            Category::Caution
        };
        // A body faces nowhere: someone taken down has stopped looking (§7.2).
        put(body.cell(), BODY_GLYPH, fg, None);
    }
    // A **seen** guard (in the FOV, §9.2) draws as the full state-coloured `g`; the
    // `g` glyph is re-categorised every turn from the guard's state (§11.2): yellow →
    // orange → red is the guard's mind, made visible. A **sensed** guard is a
    // *background* highlight instead, painted below alongside the danger overlay — no
    // glyph of its own. A guard perceived neither way draws nothing and is never
    // remembered (§11.5a), so leaving both view and sense range erases it.
    // A guard an **area effect** holds keeps its ladder colour (§11.2/#338): the effect
    // speaks in the background, always, so a frozen guard reads as "still a hunting
    // threat, currently held" rather than losing the one channel that says what it was
    // doing when the blast caught it. The mark itself is painted with the backgrounds
    // below. The absence of its cone from the danger overlay (dropped upstream in
    // `visible_cone_cells`) is truthful but negative; the mark is the positive half.
    for guard in state.guards() {
        if state.perceive_guard(guard) == Some(GuardPerception::Seen) {
            // A **seen** guard is turned the way it is looking (§11.1/#461) — the same
            // facing its cone is drawn from below, said a second way for a renderer
            // that can show it. A **sensed** guard never reaches this branch, which is
            // what keeps §9.2's "position only" true in the shape channel too.
            put(
                guard.pos(),
                GUARD_GLYPH,
                guard.state().category(),
                Some(guard.facing()),
            );
        }
    }
    // The player, always Owned — trivially inside their own FOV. Inside a hideout
    // the player is concealed: the cupboard keeps its `}` glyph but recolours to
    // Owned (§10.3/§11.3) — the "you are hidden here" signal — instead of drawing
    // the `@`. Read through the same `hidden` query the loop and vision use, so
    // the picture cannot disagree.
    //
    // Turned the way you are facing (§5/§11.1/#461) — **except inside a hideout**,
    // where the glyph is the cupboard and a cupboard faces nowhere. The facing rides
    // with the `@`, not with the cell: the moment you are drawn as the furniture you
    // are hiding in, there is no actor on the cell to turn.
    let (player_glyph, player_facing) = if state.hidden() {
        ('}', None)
    } else {
        (PLAYER_GLYPH, Some(state.facing()))
    };
    put(state.player(), player_glyph, Category::Owned, player_facing);
}

/// The crouch signal (§10.3/§11.3): while the player is crouched, the whole
/// run they ducked behind — that bench, not every table they stand beside —
/// recolours to Owned, the same vocabulary the occupied cupboard speaks
/// ("Owned = what is concealing you"), so the blue @-π pair reads as one
/// hidden unit whose π half is as long as the furniture. Read through the
/// same anchored run the concealment rule uses, so the picture cannot
/// disagree with the rules.
fn crouch_signal(state: &State, cells: &mut [GlyphCell]) {
    let width = state.layout().facility().width();
    for cover in state.crouch_cover() {
        cells[(cover.y * width + cover.x) as usize].fg = Category::Owned;
    }
}

/// The locked-cupboard signal persists in memory (§11.5a/§7.2): a cupboard you
/// have seen with a body stowed in it is a permanent fact — a spent hideout — so
/// out of view it is **remembered** as a Neutral `z`, the same way a seen console
/// is remembered (§11.5a), rather than reverting to the empty
/// `}`. Neutral because a locked cupboard is a spent object, not a thing working
/// for you (§11.2) — matching the live pass exactly, so walking away recolours
/// nothing. Only the stowed lock persists; a loose body is live state and is never
/// remembered, so it is not drawn here. Runs after the entity layer, writing only
/// out-of-FOV cells (in view they are already the entity pass's live `z`).
fn stowed_body_memory(state: &State, cells: &mut [GlyphCell]) {
    let facility = state.layout().facility();
    let width = facility.width();
    let fov = state.player_fov();
    let memory = state.memory();
    for body in state.bodies() {
        let cell = body.cell();
        if fov.contains(cell) {
            continue;
        }
        if facility.terrain(cell) == Some(Terrain::Hideout) && memory.contains(cell) {
            cells[(cell.y * width + cell.x) as usize] =
                GlyphCell::on_board(BODY_GLYPH, Category::Neutral, Visibility::Remembered);
        }
    }
}

/// The effect layer's **wash** (§11.5, #308/#325/#338): every fixed-cell mark an
/// ability effect has lit — the §6.1 box a blast reached, the cell a bore opened —
/// painted in `Category::Effect`. Painted **first of all the backgrounds**, so it is
/// the weakest cue on the board: every later mark overwrites it, and an advisory
/// layer can never hide the §11.5 [SETTLED] detection set or a sensed cue. It
/// reaches through walls and over unseen ground because that is what the effect does
/// — your own gadget's reach is not something the fog can keep from you — and each
/// set is the geometry the mechanic resolved against ([`State::effect_cell_marks`]),
/// fixed where it happened rather than following the player.
fn effect_wash(state: &State, cells: &mut [GlyphCell]) {
    let width = state.layout().facility().width();
    for cell in state.effect_cell_marks() {
        cells[(cell.y * width + cell.x) as usize].bg = Some(Category::Effect);
    }
}

/// The watcher lines (§11.5/§9.2/§7.6, #222/#465): the sightline of every guard
/// **currently** detecting the player from **outside their view** — the "something is
/// watching you, and here is where it is" cue the loop was missing (§7.6). It lights
/// the straight line between watcher and player red (`Danger`): honest, because that
/// guard's cone genuinely watches those cells, and a strict *subset* of the overlay.
/// **Standing**, not a flash (#465): drawn on every turn the watcher has the player
/// and gone the turn it loses them, so it keeps answering *"it is still looking at
/// you"* for as long as that is true. Painted among the **weakest** background cues,
/// so the later marks win where they coincide: a *sensed* watcher keeps its orange
/// position dot with the red line running up to it, and a guard that is neither seen
/// nor sensed is marked by the red line's own endpoint. It does outrank the sense
/// channel's **fading** marks (#192), which paint before it: a trace of where something
/// was a turn or two ago must never cover a line that says a guard has you *now*.
/// Guards the player can see,
/// dazed guards and a concealed player are filtered upstream
/// ([`State::watcher_lines`]) — a seen guard's real cone paints anyway (§9.2), so this
/// never double-draws or restates a seen cone.
fn watcher_line_pass(state: &State, cells: &mut [GlyphCell]) {
    let width = state.layout().facility().width();
    for cell in state.watcher_lines() {
        cells[(cell.y * width + cell.x) as usize].bg = Some(Category::Danger);
    }
}

/// The sense channel's **fading marks** (§9/§9.4, #192): every cell the player has felt
/// something through a wall in recently enough for the mark to still show — the trail
/// behind each sensed guard, and the whole footprint of each door that opened or shut
/// away from them — gets a `Category::Sensed` background, shaded by age.
///
/// The ramp is two steps, and it is the *core's* age that picks it, not the fog: a mark
/// stamped **this turn** paints [`Fill::Full`], an older one [`Fill::Quiet`]. Every mark
/// in this channel sits outside the FOV by construction (it is sensed, not seen), so the
/// fog has nothing to say about it; freshness is the only thing a strength here can
/// honestly mean (§11.5a — the position is certain either way).
///
/// Painted in two sub-passes, stale before fresh, so the freshest claim on a cell always
/// wins however the marks are ordered — a guard's live cell stays bright with its own
/// trail running back through it.
///
/// It is the **weakest** cue on the board after the effect wash: it paints before the
/// watcher line (#465), so a stale orange trace never covers a red line that says a
/// guard is looking at you *right now*, and long before the danger overlay (§11.5: being
/// seen outranks).
fn sense_mark_wash(state: &State, cells: &mut [GlyphCell]) {
    let width = state.layout().facility().width();
    for fill in [Fill::Quiet, Fill::Full] {
        let fresh = fill == Fill::Full;
        for mark in state.sense_marks().filter(|m| (m.age == 0) == fresh) {
            let cell = &mut cells[(mark.cell.y * width + mark.cell.x) as usize];
            cell.bg = Some(Category::Sensed);
            cell.fill = fill;
        }
    }
}

/// The §7.6 **investigation area** (§11.5/#224): the box every searching guard is
/// sweeping washes `Category::Warning` — orange, the same category its own `g` wears
/// while it hunts, so the area and the guard read as one state (§11.2).
///
/// Painted only with the `show_search_areas` modifier on (§12.6), which
/// [`State::search_area_cells`] owns along with the geometry: this pass knows nothing
/// but where to put the colour.
///
/// **Its place in the order is the whole of its claim.** It goes on *above* the effect
/// wash — a threat's attention outranks a note about your own gadget — and *below* the
/// sense channel, the watcher line and the danger overlay, so it can never hide a cue
/// that says where a guard actually is or what it can actually see. §11.5's fixed
/// precedence gains one rung and loses none: `Danger > a mark on a thing > Sensed >
/// investigation > the wash`.
///
/// The ticket asked for the reverse of that one comparison — investigation over Sensed.
/// It is a distinction with no picture behind it: `Warning` and `Sensed` are the *same*
/// orange row in the shell's table (§11.2), so ordering them changes no pixel, and given
/// the free choice the weaker claim (an area a guard's attention is on) should not
/// overwrite the stronger one (the cell a guard is standing in). The comparison that
/// does have a picture — red wins where they overlap — holds either way.
///
/// Sets no [`Fill`]: the board's own fog rule already gave every cell one
/// ([`Fill::fogged`]), so an investigation area beyond the player's sight reads at the
/// quiet strength exactly as a watched cell does — still visibly marked, never
/// safe-looking (§11.5 fix #1).
fn search_area_wash(state: &State, cells: &mut [GlyphCell]) {
    let width = state.layout().facility().width();
    for cell in state.search_area_cells() {
        cells[(cell.y * width + cell.x) as usize].bg = Some(Category::Warning);
    }
}

/// The **live** sensed dot (§9.2): every guard the player *senses* through a wall but
/// cannot see gets a full-strength orange `Category::Sensed` background on its exact
/// cell — a filled, eye-catching marker over whatever geometry masks the cell, position
/// only and never a glyph of its own. It carries no cone and no danger overlay: knowing
/// where a guard is is not knowing whether it can see you.
///
/// Derived live from [`State::perceive_guard`] rather than read off the recorded marks,
/// so the dot is exactly the §9.2 classification at the moment of the render and can
/// never lag it. The turn's own mark lands on the same cell at the same strength; what
/// this pass guarantees is that the *live* position is never the thing that fades.
///
/// Painted *after* [`sense_mark_wash`] and the watcher line — the dot is the sharpest
/// claim the sense makes, and the line's own endpoint is this very guard (§9.2/#465) —
/// and *before* the danger overlay, so a coincident red still wins: a sensed guard's
/// cell that a *seen* guard also watches reads danger first (§11.5: being seen outranks).
fn sensed_guard_wash(state: &State, cells: &mut [GlyphCell]) {
    let width = state.layout().facility().width();
    for guard in state.guards() {
        if state.perceive_guard(guard) == Some(GuardPerception::Sensed) {
            let cell = &mut cells[(guard.pos().y * width + guard.pos().x) as usize];
            cell.bg = Some(Category::Sensed);
            cell.fill = Fill::Full;
        }
    }
}

/// The effect layer's marks on **things** (§11.5, #308/#338/#340/#341): every actor
/// an ability effect currently holds — every guard a blast froze and the player can
/// perceive, the live decoy, whose `@` is otherwise the player's own ink told apart
/// by position alone, and the player themselves on the turns a **conditional** effect
/// is actually in force on them (Camouflage's concealment, which lapses the turn they
/// move). Painted *after* the sense channel and *before* the danger
/// overlay, because it is not a competing claim about the cell but a **refinement of
/// the cue the thing already draws**: a sensed guard's filled cell still says "a
/// guard is exactly here", and cyan adds "and it cannot move"; the decoy's `@` still
/// says "something of yours stands here", and cyan adds "and it is the ability
/// running"; the player's own `@` still says "you are here", and cyan adds "and right
/// now they cannot see you". Losing that to the orange it refines would throw away the
/// whole point of a layer that reaches through walls — the blast freezes what you
/// cannot see, so this is the common case, not the corner one. It is only ever a
/// recolour of a thing already drawn ([`State::effect_thing_marks`] carries each
/// thing's own visibility rule — perception for a guard, always for the decoy and the
/// player), never a new mark, so the fog gives nothing away.
///
/// The danger overlay still paints last and still wins, and the two cannot contradict
/// each other: a player concealed from a guard is dropped from that guard's cone
/// upstream (`visible_cone_cells`), so a red cell under a cyan-marked player would
/// have to come from a guard the camo is *not* hiding them from — which is the truth
/// worth shouting.
fn effect_thing_wash(state: &State, cells: &mut [GlyphCell]) {
    let width = state.layout().facility().width();
    for cell in state.effect_thing_marks() {
        cells[(cell.y * width + cell.x) as usize].bg = Some(Category::Effect);
    }
}

/// The danger overlay's cone pass (§11.5), last, across terrain and entities
/// alike: the union of every visible guard's cone. Its definition — the "seen"
/// gate, the concealment spare (a player concealed from that guard is not
/// detected, §10.3), the in-duct mouth-peek clip (§10.7/#134), and the
/// `always_show_vision_cones` widening (§12.6) — lives in
/// [`State::visible_cone_cells`], so this paint and the held-movement guard
/// (#223) read one set and cannot disagree. Backgrounds compose with whatever
/// glyph is on the cell: a watched guard, a watched player, watched floor.
fn danger_overlay(state: &State, cells: &mut [GlyphCell]) {
    let width = state.layout().facility().width();
    for cell in state.visible_cone_cells() {
        cells[(cell.y * width + cell.x) as usize].bg = Some(Category::Danger);
    }
}

/// What [`fogged_view`] decided about one out-of-FOV cell.
///
/// `schematic` is carried rather than re-derived from `vis` because the two can
/// legitimately disagree: with the §12.6 layout knob at [`LayoutKnowledge::Full`], a
/// cell is still honestly `Unexplored` — the player has not been there — but draws as
/// the real building. Keeping the decision in one place is what stops the glyph choice
/// and the masking choice from drifting into two different answers.
struct Fogged {
    /// The terrain to draw, after any masking.
    shown: Terrain,
    /// The knowledge state to draw it in.
    vis: Visibility,
    /// Draw the schematic mark instead of `shown`'s own glyph.
    schematic: bool,
}

/// What an out-of-FOV cell shows (§11.5a), given whether its cell is in tile
/// memory: the terrain to draw and the knowledge state to draw it in. One
/// exhaustive match, so every new terrain kind is forced to declare its layer —
/// geometry, contents, or live state — the day it is added.
///
/// # Explored versus unexplored (§11.5a, #307)
///
/// The `explored` flag is the whole fog in one bit. **Explored** — the cell has
/// been in the FOV at some point — draws the building as the player found it:
/// real glyphs, contents they saw kept as [`Remembered`](Visibility::Remembered).
/// **Unexplored** draws the **schematic** ([`SCHEMATIC_WALL`]) instead: the
/// player has the building's plans, so they read the fabric, and the floor space
/// between it as blank.
///
/// **What an unexplored cell is worth is a knob** ([`LayoutKnowledge`], §12.6), and
/// the schematic is only its middle rung. [`Full`](LayoutKnowledge::Full) draws the
/// real building there instead; [`None`](LayoutKnowledge::None) draws nothing at all,
/// so the whole section below on *which mark the plans give a thing* applies to the
/// baseline and to nothing else — there are no plans to read (#233).
///
/// The line the schematic draws is architectural rather than mechanical: `□` is
/// the building's **load-bearing fabric** — a wall run, and the recesses cut back
/// into it — and blank is everything that is not holding the building up. So a
/// hideout alcove and a duct mouth are backed by structure and read `□`; a table
/// and a console stand in a room and draw nothing. Neither reading follows
/// passability, and deliberately so — the plan shows the building's bones, not
/// what has been put in it.
///
/// A **doorway** is the case that makes the rule concrete: it bears no load, so it
/// draws blank and shows on the plans as a **gap in the wall line**, exactly as an
/// architectural plan draws one. Its *frame* is still structure and stays `□`, so
/// an unexplored wing reads `□□□ □□□` and the ways between its rooms can be
/// planned before setting foot in them — which is §11.5a's *"you can plan your
/// escape route before you're spotted"* surviving the schematic intact.
///
/// **Everything unexplored must collapse to exactly two appearances**, one of them
/// being nothing at all, or the schematic leaks what it is meant to withhold: a lone
/// real glyph among the schematic marks would advertise the very content the player
/// has not found. That is why the masking here returns bare [`Terrain::Wall`] and
/// [`Terrain::Floor`] — the glyph *and* the §11.2 category then both come from
/// the mask, so the colour channel cannot give away what the glyph channel hides.
///
/// The **exit** is the sole exception: it is the tunnel the player dug and came in
/// by (§4.5), the one piece of this building that is theirs and that they could
/// not fail to know. It keeps its `E` and its Interest tint from turn one, and
/// goes on anchoring every escape plan (§7.6).
///
/// # Geometry versus contents, in the *explored* half
///
/// The second match below asks a different question from the schematic's, and the
/// two do not partition the same way (`docs/render-reference.md` §3). Here the
/// question is **which ink a cell keeps once it leaves your sight**: the row's dim
/// shade, or the memory slate that says *you found this*.
///
/// **Contents take the slate** — intel, comms, cupboards, and duct mouths (#450).
/// The slate earns its distinctness by being rare, so the set is the handful of
/// things worth a mark on a plan. A duct mouth belongs among them because §10.7
/// makes a duct an escape a pursuer cannot follow: a mouth scouted is a route you
/// plan with, in the way §2.3's exit anchors every escape plan. Drawn as geometry it
/// took the shared dim grey — the very colour a wall dims to — and a route found
/// read as one more piece of building the moment you looked away.
///
/// **Doors and furniture take the dim shade**, and that is the render's decision
/// rather than an oversight. A door's *pose* is live state, redrawn canonically
/// closed every frame out of view, so a slate door would be a memory colour on a
/// drawing that is not a memory. And doors are everywhere: slating all of them would
/// bury the two or three marks that change a plan under a building's worth of
/// doorways. A colour that marks everything marks nothing.
fn fogged_view(terrain: Terrain, explored: bool, knowledge: LayoutKnowledge) -> Fogged {
    let vis = if explored {
        Visibility::Explored
    } else {
        Visibility::Unexplored
    };
    let real = |shown| Fogged {
        shown,
        vis,
        schematic: false,
    };
    // The exit first: known from turn one whatever else is (§4.5/§7.6). Under
    // `LayoutKnowledge::None` it is the *only* thing turn one draws, and that is what
    // keeps the run playable: with the building gone, the player's own tunnel is the
    // one fixed point an escape plan can be hung on.
    if terrain == Terrain::Exit {
        return real(terrain);
    }
    if !explored && knowledge == LayoutKnowledge::None {
        // **No plans at all** (§11.5a/#233), the layout knob's harder end. Ground the
        // player has never had eyes on is blank — not a dimmer schematic, which would
        // be the fog §11.5a already argues against, and not a second mark to read.
        //
        // The mask is *total*, and by the schematic's own rule taken one step further:
        // everything unexplored has to collapse to a single appearance in a single
        // colour or the fog leaks what it hides. Here that single appearance is bare
        // floor — no glyph, and so no ink for the §11.2 category channel to give
        // anything away through either. Nothing distinguishes an unwalked wall from
        // the room behind it, which is the whole modifier: the building has to be
        // walked to be known.
        return Fogged {
            shown: Terrain::Floor,
            vis: Visibility::Unexplored,
            // Moot rather than meaningful: the schematic arm draws only for a `Wall`,
            // and the mask above leaves no unexplored cell showing one. Carried as
            // `true` so the flag keeps saying what it says everywhere else — *this is
            // not the real building* — rather than claiming a drawn cell is.
            schematic: true,
        };
    }
    if !explored && knowledge == LayoutKnowledge::Plans {
        // The schematic (§11.5a/#307). Fabric — what holds the building up: a wall
        // run, a door's *frame*, and the recesses cut back into a run (a hideout
        // alcove, a duct mouth), which are backed by structure and read as part of
        // it. Everything else is floor space — the room's own area, the furniture and
        // equipment standing in it, and a **doorway**, which bears no load — and
        // floor space draws blank, which is how a plan shows the gap in a wall line.
        //
        // The crawl *path* between a duct's two entries is not classified here at
        // all: its interior cells keep their own terrain and are never in memory
        // (§11.5a/§10.7), so they read as whatever the building around them reads
        // as, giving the shortcut away to nobody.
        let fabric = matches!(
            terrain,
            Terrain::Wall | Terrain::DoorHinge | Terrain::DuctEntry | Terrain::Hideout
        );
        return Fogged {
            shown: if fabric {
                Terrain::Wall
            } else {
                Terrain::Floor
            },
            vis: Visibility::Unexplored,
            schematic: true,
        };
    }
    // Either the cell is explored, or the §12.6 layout knob is at
    // [`LayoutKnowledge::Full`] and handing the architecture over. Both draw the real
    // building; they differ only in what they do with a **content**, which the
    // modifier never reveals.
    match terrain {
        // Geometry the player has walked (or been given): drawn as itself, dim but
        // legible (§11.5).
        Terrain::Floor
        | Terrain::Wall
        | Terrain::DoorHinge
        | Terrain::Exit
        | Terrain::PartialCover => real(terrain),
        // A door's *position* is known once explored, but its open/closed pose is
        // live state, never remembered: out of view a panel draws canonically closed.
        Terrain::DoorPanelClosed | Terrain::DoorPanelOpen => real(Terrain::DoorPanelClosed),
        // Contents: hidden until seen, then remembered (§11.5a). The comms console
        // (§7.3/§7.7) is contents like the intel console: the counterplay it offers
        // has to be *found*, so the map never advertises it before the player has
        // scouted the room. An **equipment cache** (#209) is contents on the same
        // terms and for a sharper reason — it is an optional detour, and a detour the
        // plans hand over for free is not one the player chose to take.
        //
        // A **duct mouth** is contents too (#450), and for the reason the layer table
        // gives it: it is a route you plan with. §10.7 makes a duct an escape a
        // pursuer cannot follow, so a mouth found once is worth as much to the plan as
        // the cupboard beside it — and drawn as geometry it took the shared dim grey
        // and read as one more wall the moment you looked away. The memory slate is
        // what says *you found this*.
        Terrain::Console
        | Terrain::CommsConsole
        | Terrain::EquipmentCache
        | Terrain::Hideout
        | Terrain::DuctEntry
            if explored =>
        {
            Fogged {
                shown: terrain,
                vis: Visibility::Remembered,
                schematic: false,
            }
        }
        // Layout handed over but the cell never seen: the content is still hidden,
        // masked by the geometry naturally in its place. The modifier buys the
        // architecture, never the objectives.
        Terrain::Console | Terrain::CommsConsole | Terrain::EquipmentCache => real(Terrain::Floor),
        Terrain::Hideout => real(Terrain::Wall),
        // The one content the modifier **does** hand over, and deliberately left that
        // way by #450: a mouth is cut into the fabric and reads off a plan the way a
        // doorway does, which is why `LayoutKnowledge::Full` has always drawn it (§12.6,
        // and the modifier's own row in §11.5a). Being remembered once scouted is a
        // change to what *finding* one is worth; it is not an argument for taking it
        // off the plans, and quietly narrowing an *easier*-direction modifier is a
        // difficulty change wearing a render fix's clothes.
        Terrain::DuctEntry => real(terrain),
    }
}

/// A blank full-screen [`Grid`] — every cell an empty, live, uncoloured space. The
/// starting canvas of a **panel** render (the help card, the menu): a surface that
/// replaces the game frame entirely rather than overlaying it, so it begins from
/// nothing and draws its own rows.
pub(super) fn blank_grid(width: u32, height: u32) -> Grid {
    let blank = GlyphCell::blank();
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
            ..GlyphCell::blank()
        };
    }
}

/// The glyph an **overlay card** is bounded with, top and bottom — the same rule the
/// deployed message log closes on (§11.7/#300), so every surface laid *over* the frame
/// reads as one family rather than as a series of inventions.
pub(super) const RULE_GLYPH: char = '─';

/// Where an overlay card `rows` tall starts on a `height`-tall screen: the block
/// centred in the **map area**, so the board reads above and below it (§11.4 — the
/// screen is the board, and a card that covered it would take away the very thing
/// the player is being told about).
///
/// Clamped to the top of the map rather than allowed to climb into the status lines:
/// those rows are the near line's and the usable line's, and an overlay that ate them
/// would cover live state to show standing state.
pub(super) fn overlay_top(height: u32, rows: usize) -> u32 {
    let map_h = height.saturating_sub(hud::TOP_ROWS + hud::BOTTOM_ROWS);
    hud::TOP_ROWS + map_h.saturating_sub(rows as u32) / 2
}

/// Blank one row of the frame — an overlay card's own surface, so no board glyph
/// reads through the words laid over it.
pub(super) fn clear_row(grid: &mut Grid, y: u32) {
    for x in 0..grid.width {
        grid.cells[(y * grid.width + x) as usize] = GlyphCell {
            vis: Visibility::Live,
            ..GlyphCell::blank()
        };
    }
}

/// Draw an overlay card's bounding rule across row `y`.
pub(super) fn draw_rule(grid: &mut Grid, y: u32) {
    for x in 0..grid.width {
        grid.cells[(y * grid.width + x) as usize] = GlyphCell {
            glyph: RULE_GLYPH,
            fg: Category::System,
            ..GlyphCell::blank()
        };
    }
}

mod alert;
mod campaign_map;
mod help;
mod hud;
mod menu;
mod message_log;
mod modifier_rows;
mod settings;
mod usable;
mod verdict;
pub use campaign_map::{
    brief_rows, flavour_glyph, hit_of, map_activation, map_hit, render_brief, render_map, BriefRow,
    MapHit, MapScreen, MapUi,
};
pub use help::{help_hit, HelpHit, HelpTab, SeedCopy};
pub use hud::{
    ability_at, ability_in_slot, ability_mnemonic, ability_slot_for_letter, is_help_button,
    render_screen, InputModality, ScreenUi, BOTTOM_ROWS, TOP_ROWS,
};
pub use menu::{menu_hit, MenuEntry, MenuHit, MenuScreen, MenuUi, OptionsControl};
#[cfg(test)]
pub(crate) use message_log::near_line_text_max;
pub use message_log::{is_message_button, message_log_rows};
pub use settings::{shown_rows, Renderer, SettingsRow, SettingsUi};
pub use verdict::{verdict_hit, EndUi};

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
mod tests;
