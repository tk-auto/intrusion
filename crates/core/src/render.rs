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

use crate::category::{Category, Theme};
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

/// The **schematic** glyphs (§11.5a, #307): how geometry the player has never had
/// eyes on is drawn — the building as its *plans* give it, not as it has been seen.
///
/// A cell that has never been in the player's FOV collapses to one of these two
/// marks. `≈` is the building's **fabric** — a wall run and the recesses and
/// openings cut into it — and `~` is the **floor space** between it. Both are
/// deliberately *approximate*: `≈` is the mathematical "approximately", which is
/// exactly the claim the plan makes about a stretch of building nobody has walked.
/// Standing somewhere resolves the schematic into what is really there, and
/// permanently — tile memory is monotonic (§11.5a).
///
/// **Why shape rather than a darker shade.** The obvious alternative was a fourth
/// rung on the §11.5 brightness ladder, and it is the worse channel: on a dark
/// palette the gap below Ground's already-quiet dim is too small to read on a
/// phone, and pushing it darker turns the readout into de-facto fog, which §11.5a
/// settles against. Shape costs no colour, cannot compete with the threat channels
/// (§11.2 Danger and Sensed keep the background to themselves), and needs nothing
/// extra from a second palette (#189). It also degrades safely: even where the two
/// marks blur at a small font, wall and floor keep their own colour rows, so the
/// layout stays as readable as it is today.
pub(crate) const SCHEMATIC_WALL: char = '≈';
/// The floor half of the schematic — see [`SCHEMATIC_WALL`].
pub(crate) const SCHEMATIC_GROUND: char = '~';

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
    /// gray, dim but legible.
    Explored,
    /// Never in the player's FOV — geometry known from the building's plans and
    /// nothing else (§11.5a). Drawn as the **schematic** (see [`SCHEMATIC_WALL`]):
    /// the fabric of the building and the floor between it, with everything
    /// standing in the rooms yet to be discovered.
    ///
    /// It is the *knowledge* that is recorded here, not the styling — the shell
    /// paints this in the same dim shade as [`Explored`](Self::Explored), because
    /// the schematic separates itself by **shape**. That keeps the distinction on
    /// the seam for anything that needs to reason about coverage rather than draw
    /// it (the §12.6 full-layout modifier, a future fog-the-geometry rule).
    Unexplored,
    /// Outside the FOV, drawn from tile memory: a content seen earlier this run
    /// (§11.5a) — its own visual state, distinct from both live and explored.
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
    /// The knowledge state this cell is drawn in (§11.5a): live, explored
    /// geometry, or remembered content. The shell styles the three distinctly.
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
/// through walls. The **decoy** is the other exception (§8.3/#321): it is the player's
/// own placed object rather than the facility's live state, so it draws at its cell in
/// or out of the FOV, `Remembered` while unseen — see the decoy layer below.
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
/// thing > `Sensed` > the wash — because an advisory layer must never masquerade as the
/// detection set, nor hide it (§11.5 **[SETTLED]**).
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
    // The §12.6 `full_layout_known` modifier: draw the architecture of cells the
    // player has never had eyes on, instead of the schematic. Contents stay hidden —
    // it buys the building, not the objectives.
    let layout_known = state.modifiers().full_layout_known;

    // Terrain layer, through the fog: what the player knows of each cell.
    let mut cells: Vec<GlyphCell> = (0..height)
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
                fogged_view(terrain, memory.contains(cell), layout_known)
            };
            // Floor dots (§11.5): give open ground a foreground so the FOV edge
            // reads across it. Masked contents dot too — they *show* floor.
            //
            // Unexplored geometry draws the schematic instead (§11.5a/#307).
            // `fogged_view` has already collapsed it to bare wall or bare floor, so
            // the swap is total: every unexplored cell wears one of two marks and
            // none of them can be told apart by glyph *or* by category.
            let glyph = match (schematic, shown) {
                (true, Terrain::Wall) => SCHEMATIC_WALL,
                (true, Terrain::Floor) => SCHEMATIC_GROUND,
                (_, Terrain::Floor) => FLOOR_DOT,
                _ => shown.glyph(),
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

    // Entity layers, lowest priority first so the top entity is the last writer.
    // The decoy (§8.3) draws lowest: an Owned `@` — a thing you made wearing
    // your own glyph, which is the whole trick (§10.3/§11.3). Alone among the
    // entities it draws **wherever it is**, in the FOV or out of it (§11.5a's
    // second exception, #321): the whole point of a fake is to walk away from it
    // and let a guard investigate the wrong cell, so a marker you can only see by
    // standing next to it is a marker the ability cannot use. Its cell is the
    // player's own knowledge, on the same footing as their own position — not a
    // content of the facility they have to keep looking at. It leaks nothing:
    // a decoy dies the turn anything steps on it, and that death already flips
    // the §11.4 bar into cooldown and prints the §11.7 message unconditionally,
    // so the `@` vanishing only puts a fact the player is already told where they
    // are already looking. Out of view it draws `Remembered`, not `Live`: the
    // marker persists at full Owned colour while the three-state discipline
    // (§11.5a) keeps telling the truth about what is actually being seen.
    if let Some(decoy) = state.decoy() {
        cells[(decoy.y * width + decoy.x) as usize] = GlyphCell {
            glyph: PLAYER_GLYPH,
            fg: Category::Owned,
            bg: None,
            vis: if fov.contains(decoy) {
                Visibility::Live
            } else {
                Visibility::Remembered
            },
        };
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
    // A guard an **area effect** holds keeps its ladder colour (§11.2/#338): the effect
    // speaks in the background, always, so a frozen guard reads as "still a hunting
    // threat, currently held" rather than losing the one channel that says what it was
    // doing when the blast caught it. The mark itself is painted with the backgrounds
    // below. The absence of its cone from the danger overlay (dropped upstream in
    // `visible_cone_cells`) is truthful but negative; the mark is the positive half.
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

    // The effect layer's **wash** (§11.5, #308/#325/#338): every fixed-cell mark an
    // ability effect has lit — the §6.1 box a blast reached, the cell a bore opened —
    // painted in `Category::Effect`. Painted **first of all the backgrounds**, so it is
    // the weakest cue on the board: every mark below overwrites it, and an advisory
    // layer can never hide the §11.5 [SETTLED] detection set or a sensed cue. It
    // reaches through walls and over unseen ground because that is what the effect does
    // — your own gadget's reach is not something the fog can keep from you — and each
    // set is the geometry the mechanic resolved against ([`State::effect_cell_marks`]),
    // fixed where it happened rather than following the player.
    for cell in state.effect_cell_marks() {
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
    for guard in state.guards() {
        if state.perceive_guard(guard) == Some(GuardPerception::Sensed) {
            cells[(guard.pos().y * width + guard.pos().x) as usize].bg = Some(Category::Sensed);
        }
    }

    // The effect layer's marks on **things** (§11.5, #308/#338/#340/#341): every actor
    // an ability effect currently holds — every guard a blast froze and the player can
    // perceive, the live decoy, whose `@` is otherwise the player's own ink told apart
    // by position alone, and the player themselves on the turns a **conditional** effect
    // is actually in force on them (Camouflage's concealment, which lapses the turn they
    // move). Painted *after* the sense channel and *before* the danger
    // overlay, because it is not a competing claim about the cell but a **refinement of
    // the cue the thing already draws**: a sensed guard's filled cell still says "a
    // guard is exactly here", and cyan adds "and it cannot move"; the decoy's `@` still
    // says "something of yours stands here", and cyan adds "and it is the ability
    // running"; the player's own `@` still says "you are here", and cyan adds "and right
    // now they cannot see you". Losing that to the orange it refines would throw away the
    // whole point of a layer that reaches through walls — the blast freezes what you
    // cannot see, so this is the common case, not the corner one. It is only ever a
    // recolour of a thing already drawn ([`State::effect_thing_marks`] carries each
    // thing's own visibility rule — perception for a guard, always for the decoy and the
    // player), never a new mark, so the fog gives nothing away.
    //
    // The danger overlay still paints last and still wins, and the two cannot contradict
    // each other: a player concealed from a guard is dropped from that guard's cone
    // upstream (`visible_cone_cells`), so a red cell under a cyan-marked player would
    // have to come from a guard the camo is *not* hiding them from — which is the truth
    // worth shouting.
    for cell in state.effect_thing_marks() {
        cells[(cell.y * width + cell.x) as usize].bg = Some(Category::Effect);
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

/// What [`fogged_view`] decided about one out-of-FOV cell.
///
/// `schematic` is carried rather than re-derived from `vis` because the two can
/// legitimately disagree: with the §12.6 `full_layout_known` modifier on, a cell is
/// still honestly `Unexplored` — the player has not been there — but draws as the
/// real building. Keeping the decision in one place is what stops the glyph choice
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
/// player has the building's plans, so they read the fabric and the floor space,
/// and nothing else.
///
/// The line the schematic draws is architectural rather than mechanical: `≈` is
/// the building's **load-bearing fabric** — a wall run, and the recesses cut back
/// into it — and `~` is everything that is not holding the building up. So a
/// hideout alcove and a duct mouth are backed by structure and read `≈`; a table
/// and a console stand in a room and read `~`. Neither reading follows
/// passability, and deliberately so — the plan shows the building's bones, not
/// what has been put in it.
///
/// A **doorway** is the case that makes the rule concrete: it bears no load, so it
/// reads `~` and shows on the plans as a **gap in the wall line**, exactly as an
/// architectural plan draws one. Its *frame* is still structure and stays `≈`, so
/// an unexplored wing reads `≈≈≈~≈≈≈` and the ways between its rooms can be
/// planned before setting foot in them — which is §11.5a's *"you can plan your
/// escape route before you're spotted"* surviving the schematic intact.
///
/// **Everything unexplored must collapse to exactly two appearances**, or the
/// schematic leaks what it is meant to withhold: a lone real glyph among the
/// schematic marks would advertise the very content the player has not found.
/// That is why the masking here returns bare [`Terrain::Wall`] and
/// [`Terrain::Floor`] — the glyph *and* the §11.2 category then both come from
/// the mask, so the colour channel cannot give away what the glyph channel hides.
///
/// The **exit** is the sole exception: it is the tunnel the player dug and came in
/// by (§4.5), the one piece of this building that is theirs and that they could
/// not fail to know. It keeps its `E` and its Interest tint from turn one, and
/// goes on anchoring every escape plan (§7.6).
fn fogged_view(terrain: Terrain, explored: bool, layout_known: bool) -> Fogged {
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
    // The exit first: known from turn one whatever else is (§4.5/§7.6).
    if terrain == Terrain::Exit {
        return real(terrain);
    }
    if !explored && !layout_known {
        // The schematic (§11.5a/#307). Fabric — what holds the building up: a wall
        // run, a door's *frame*, and the recesses cut back into a run (a hideout
        // alcove, a duct mouth), which are backed by structure and read as part of
        // it. Everything else is floor space: the room's own area, the furniture and
        // equipment standing in it, and a **doorway**, which bears no load and so
        // draws as the gap in the wall line that a plan would show.
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
    // Either the cell is explored, or the §12.6 `full_layout_known` modifier is
    // handing the architecture over. Both draw the real building; they differ only
    // in what they do with a **content**, which the modifier never reveals.
    match terrain {
        // Geometry the player has walked (or been given): drawn as itself, dim but
        // legible (§11.5).
        Terrain::Floor
        | Terrain::Wall
        | Terrain::DoorHinge
        | Terrain::Exit
        | Terrain::DuctEntry
        | Terrain::PartialCover => real(terrain),
        // A door's *position* is known once explored, but its open/closed pose is
        // live state, never remembered: out of view a panel draws canonically closed.
        Terrain::DoorPanelClosed | Terrain::DoorPanelOpen => real(Terrain::DoorPanelClosed),
        // Contents: hidden until seen, then remembered (§11.5a). The comms console
        // (§7.3/§7.7) is contents like the intel console: the counterplay it offers
        // has to be *found*, so the map never advertises it before the player has
        // scouted the room.
        Terrain::Console | Terrain::CommsConsole | Terrain::Hideout if explored => Fogged {
            shown: terrain,
            vis: Visibility::Remembered,
            schematic: false,
        },
        // Layout handed over but the cell never seen: the content is still hidden,
        // masked by the geometry naturally in its place. The modifier buys the
        // architecture, never the objectives.
        Terrain::Console | Terrain::CommsConsole => real(Terrain::Floor),
        Terrain::Hideout => real(Terrain::Wall),
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

mod alert;
mod help;
mod hud;
mod menu;
mod message_log;
mod usable;
pub use help::{help_hit, HelpHit, HelpTab, SeedCopy};
pub use hud::{
    ability_at, ability_in_slot, ability_mnemonic, ability_slot_for_letter, is_help_button,
    render_screen, InputModality, ScreenUi, BOTTOM_ROWS, TOP_ROWS,
};
pub use menu::{menu_hit, MenuEntry, MenuHit, MenuUi};
pub use message_log::{is_message_button, message_log_rows};

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

    /// **The theme changes nothing the core renders** (§11.2/#189). Presentation owns
    /// the category→colour table, so a [`ScreenUi::theme`] flip must move no glyph, no
    /// category and no background anywhere on the screen — the same grid, painted from
    /// the other column of the shell's one table.
    ///
    /// This is what keeps the toggle free: it changes no world, costs no turn (§4.4)
    /// and cannot perturb a replay (§12.4), because the core never reads it at all.
    #[test]
    fn the_theme_changes_nothing_the_core_renders() {
        let mut s = state(
            12,
            12,
            Cell::new(3, 3),
            vec![Guard::stationary(Cell::new(5, 6))],
        );
        s.step(Input::Wait);
        for tab in HelpTab::ALL {
            for help_open in [false, true] {
                let ui = |theme| ScreenUi {
                    help_open,
                    help_tab: tab,
                    theme,
                    ..ScreenUi::default()
                };
                let dark = render_screen(&s, ui(Theme::Dark));
                let light = render_screen(&s, ui(Theme::Light));
                assert_eq!(
                    dark.to_text(),
                    light.to_text(),
                    "{tab:?} (help_open {help_open}): the theme moved a glyph",
                );
                for y in 0..dark.height {
                    for x in 0..dark.width {
                        let (d, l) = (dark.get(x, y), light.get(x, y));
                        assert_eq!(
                            (d.fg, d.bg, d.vis),
                            (l.fg, l.bg, l.vis),
                            "{tab:?}: cell ({x},{y}) declares a different meaning",
                        );
                    }
                }
            }
        }
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
        assert_eq!(masked.vis, Visibility::Explored);
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
        assert_eq!(g.get(4, 6).vis, Visibility::Live, "in view: drawn live");
    }

    /// §8.3/§11.5 (#338/#341): Camouflage's mark is the half of the ability the §11.4
    /// bar cannot say. The board carries "you are hidden **right now**" — lit on a still
    /// turn, dark on the turn you moved, back on the next still one — while the `@`
    /// stays Owned throughout and only the background moves.
    #[test]
    fn the_camouflaged_player_is_marked_on_the_turns_the_concealment_holds() {
        use crate::AbilityId;
        let mut s = state_holding(10, 10, Cell::new(4, 4), Vec::new(), AbilityId::Camouflage);
        s.step(Input::Activate(AbilityId::Camouflage));

        let still = render(&s).get(4, 4);
        assert_eq!(still.glyph, '@', "the glyph is untouched");
        assert_eq!(still.fg, Category::Owned, "…and so is its ink");
        assert_eq!(
            still.bg,
            Some(Category::Effect),
            "standing still: the concealment is in force and says so",
        );

        s.step(Input::Step(Direction::South));
        let moved = render(&s).get(4, 5);
        assert_eq!(moved.glyph, '@', "still the player, still Owned");
        assert_eq!(moved.fg, Category::Owned);
        assert_ne!(
            moved.bg,
            Some(Category::Effect),
            "a turn they moved is a turn they are visible, whatever the bar reads",
        );

        s.step(Input::Wait);
        assert_eq!(
            render(&s).get(4, 5).bg,
            Some(Category::Effect),
            "and the mark resumes with the concealment",
        );
    }

    /// §8.3/§11.5 (#338/#340): a live decoy is a **running ability**, not merely a
    /// thing of yours, so its cell carries the standing effect mark. The `@` above it
    /// stays Owned and the glyph priority is untouched — the effect speaks in the
    /// background, always — which makes the wash the one thing telling the two blue
    /// `@`s apart by anything other than position.
    #[test]
    fn a_live_decoy_draws_the_effect_mark_under_its_owned_at() {
        use crate::AbilityId;
        let mut s = state_holding(10, 10, Cell::new(4, 4), Vec::new(), AbilityId::Decoy);
        s.step(Input::Step(Direction::South)); // (4,5), facing south
        s.step(Input::Activate(AbilityId::Decoy)); // the fake at (4,6)
        let g = render(&s);

        let fake = g.get(4, 6);
        assert_eq!(fake.glyph, '@', "the glyph is untouched");
        assert_eq!(fake.fg, Category::Owned, "…and so is its ink");
        assert_eq!(
            fake.bg,
            Some(Category::Effect),
            "the ability running, said in the background",
        );
        assert_ne!(
            g.get(4, 5).bg,
            Some(Category::Effect),
            "the *real* player is not the ability: only the fake is washed",
        );

        // The mark goes with the fake, on the frame it dies (§8.3).
        s.step(Input::Step(Direction::South));
        assert_eq!(s.decoy(), None, "precondition: the player stomped it");
        assert_ne!(render(&s).get(4, 6).bg, Some(Category::Effect));
    }

    /// §8.3/§11.5a (#321/#340): the fake's mark follows the glyph it sits under, so it
    /// is painted out of the FOV too — a wash you could only read by standing next to
    /// the fake would be a wash the ability cannot use.
    #[test]
    fn a_live_decoy_keeps_its_effect_mark_out_of_view() {
        use crate::AbilityId;
        let mut s = state_holding(10, 10, Cell::new(4, 4), Vec::new(), AbilityId::Decoy);
        s.step(Input::Step(Direction::South)); // (4,5), facing south
        s.step(Input::Activate(AbilityId::Decoy)); // the fake at (4,6)
        let decoy = Cell::new(4, 6);
        while s.player_fov().contains(decoy) {
            s.step(Input::Step(Direction::North));
        }
        assert_eq!(
            s.decoy(),
            Some(decoy),
            "precondition: nothing stepped on it"
        );

        let cell = render(&s).get(decoy.x, decoy.y);
        assert_eq!(
            cell.bg,
            Some(Category::Effect),
            "still marked, out of sight"
        );
        assert_eq!(cell.vis, Visibility::Remembered, "…and honest about it");
    }

    /// §8.3/§11.5a (#321): a decoy is the player's *own* placed object, so it is
    /// drawn wherever it is — the whole point of the fake is to walk away from it
    /// and let a guard investigate the wrong cell. Out of the FOV it keeps the
    /// Owned `@` and drops to `Remembered`, so the marker persists without the
    /// frame claiming an unseen cell is being seen.
    #[test]
    fn a_decoy_out_of_view_stays_drawn_as_a_remembered_owned_at() {
        use crate::AbilityId;
        let mut s = state_holding(10, 10, Cell::new(4, 4), Vec::new(), AbilityId::Decoy);
        s.step(Input::Step(Direction::South)); // (4,5), facing south
        s.step(Input::Activate(AbilityId::Decoy)); // the fake at (4,6)
        let decoy = Cell::new(4, 6);
        assert_eq!(s.decoy(), Some(decoy), "precondition: the fake is out");

        // Turn around and walk away until the fake is behind the sight cone.
        while s.player_fov().contains(decoy) {
            s.step(Input::Step(Direction::North));
        }
        assert!(s.decoy().is_some(), "precondition: nothing stepped on it");

        let cell = render(&s).get(4, 6);
        assert_eq!(cell.glyph, '@', "the fake you placed is still on the board");
        assert_eq!(cell.fg, Category::Owned);
        assert_eq!(
            cell.vis,
            Visibility::Remembered,
            "out of sight, so not drawn as live",
        );
    }

    /// §8.3/§11.5a (#321): the persistent marker still tells the truth about the
    /// decoy's death. A guard walks onto the fake while the player cannot see the
    /// cell — the `@` is gone from the very next frame, alongside the message and
    /// the cooldown that already fire.
    #[test]
    fn a_decoy_stomped_out_of_view_leaves_no_stale_marker() {
        use crate::AbilityId;
        // One tall column: the player at the top, a guard walking up it from below,
        // and the fake planted between them.
        let decoy = Cell::new(4, 14);
        let mut s = state_holding(
            12,
            20,
            Cell::new(4, 12),
            vec![Guard::patrolling_to(Cell::new(4, 18), Cell::new(4, 2))],
            AbilityId::Decoy,
        );
        s.step(Input::Step(Direction::South)); // (4,13), facing south
        s.step(Input::Activate(AbilityId::Decoy)); // the fake at (4,14)
        assert_eq!(s.decoy(), Some(decoy), "precondition: the fake is out");

        // Walk away up the column while the guard comes up it. The player never
        // *waits*: a spent wait opens the full 360° arc (§6.2) and would put the
        // fake back in view. Only a footstep raises `DecoyDied` — the 20-turn expiry
        // raises `AbilityExpired` instead — so a fake that merely timed out fails
        // this loop rather than passing it by the back door.
        let mut trampled = false;
        for _ in 0..10 {
            if s.step(Input::Step(Direction::North))
                .iter()
                .any(|e| matches!(e, Event::DecoyDied { at } if *at == decoy))
            {
                trampled = true;
                break;
            }
        }
        assert!(trampled, "the guard walked onto the fake");
        assert!(
            !s.player_fov().contains(decoy),
            "and it died where the player could not see it",
        );
        assert_eq!(s.decoy(), None);

        let cell = render(&s).get(4, 14);
        assert_ne!(cell.glyph, '@', "no marker survives the decoy");
    }

    /// §11.5 **[SETTLED]** (#321): the persistent decoy marker is the *lowest*
    /// entity layer and paints no background, so it can never hide the danger
    /// overlay. A watched cell under an out-of-view fake still reads red — "red
    /// means a guard watches this" survives the marker sitting on it.
    #[test]
    fn a_decoy_out_of_view_never_hides_the_danger_overlay() {
        use crate::AbilityId;
        // A guard posted up the column, looking south down it (§7.1's spawn facing)
        // over the cell the fake will stand in. Cones of guards the player cannot see
        // paint only under the §12.6 modifier, which is what puts red on that cell
        // while the fake is behind the player.
        let decoy = Cell::new(4, 9);
        let mut s = state_holding_facing_north(
            12,
            20,
            Cell::new(4, 10),
            vec![Guard::stationary(Cell::new(4, 5))],
            AbilityId::Decoy,
        )
        .with_modifiers(LevelModifiers {
            always_show_vision_cones: true,
            ..LevelModifiers::default()
        });
        s.step(Input::Activate(AbilityId::Decoy)); // the fake at (4,9)
        assert_eq!(s.decoy(), Some(decoy), "precondition: the fake is out");

        // Walk away down the column until the fake is behind the sight cone.
        while s.player_fov().contains(decoy) {
            s.step(Input::Step(Direction::South));
        }
        assert_eq!(s.decoy(), Some(decoy), "precondition: nothing trampled it");

        let cell = render(&s).get(decoy.x, decoy.y);
        assert_eq!(cell.glyph, '@', "the fake still draws");
        assert_eq!(cell.vis, Visibility::Remembered);
        assert_eq!(
            cell.bg,
            Some(Category::Danger),
            "and the watched cell under it still reads red",
        );
    }

    /// §8.3/§11.5: the danger overlay keeps its promise under Camouflage — "red
    /// under you = detected". A cloaked, still player under a visible guard's
    /// cone shows no red on their own cell; before cloaking, the same cell is
    /// red. The cone itself stays painted — the guard watches the ground, it
    /// just cannot see what stands cloaked on it.
    ///
    /// Since #341 the spared cell is not *blank* but **cyan**: the same fact stated
    /// positively, by the effect mark, where it used to be readable only as an absence.
    /// The promise being asserted is unchanged — it was always "not red", never
    /// "nothing" — so it is written as the promise rather than as whatever happens to
    /// occupy the cell instead.
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
        assert_ne!(
            g.get(5, 6).bg,
            Some(Category::Danger),
            "cloaked and still: no red under you",
        );
        assert_eq!(
            g.get(5, 6).bg,
            Some(Category::Effect),
            "…and the cue that replaced the red says why (#341)",
        );
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

    /// §11.5 and §10.3 agree cell for cell under #377's half-plane too, not just the
    /// old per-ray rule: a guard off the **end** of the bench but on its far side
    /// paints its cone, and the crouched player's cell is spared — because the same
    /// `concealed_from` seam answers the overlay, the guard's own sight, and the
    /// §7.2 takedown gate. Red under you still means *detected*.
    #[test]
    fn the_overlay_agrees_with_the_half_plane_off_the_benchs_end() {
        // A two-cell bench at x = 5; the player crouches at (4,5) and crouch-walks
        // north to (4,4), round the bench's end and still hugging it on the
        // diagonal. The guard sits north-east at (6,2) — across the bench's line
        // but past its end, which is exactly the line the ray test misses.
        let mut layout = open_room(12, 12);
        layout.place(Cell::new(5, 5), Terrain::PartialCover);
        layout.place(Cell::new(5, 6), Terrain::PartialCover);
        let mut s = State::new(
            layout,
            Cell::new(4, 5),
            Direction::North,
            vec![Guard::stationary(Cell::new(6, 2))],
            Vec::new(),
            Cell::new(10, 10),
        );
        s.step(Input::Step(Direction::East)); // bump the bench: crouch
        s.step(Input::Step(Direction::North)); // crouch-walk round its end
        assert_eq!(s.player(), Cell::new(4, 4));
        assert!(s.crouched(), "the diagonal hug holds the pose (§10.3)");

        let guard = &s.guards()[0];
        assert!(
            guard.fov().contains(Cell::new(4, 4)),
            "the guard's cone reaches the player — sight passes over a table"
        );
        assert!(
            !s.guard_detects_now(guard),
            "but the bench is between them, so it does not see them (§10.3)"
        );
        let g = render(&s);
        assert_eq!(
            g.get(4, 4).bg,
            None,
            "the overlay spares the concealed player's cell"
        );
        assert_eq!(
            g.get(5, 4).bg,
            Some(Category::Danger),
            "while the rest of the visible cone stays red"
        );
    }

    /// §11.5a (#307): a table stands **in a room**, not in the building's fabric, so
    /// the plans do not carry it. Unexplored it reads as schematic floor like the rest
    /// of the room's area; walk in and it resolves into the `π` you can crouch behind,
    /// and stays that way (memory is monotonic).
    ///
    /// The furniture is genuinely a surprise, then — including the fact that it blocks
    /// the cell. That is the deliberate trade of putting only the bones on the plan:
    /// what you can *plan* is the building, and what a room turns out to contain is
    /// what exploring is for.
    #[test]
    fn a_table_is_discovered_not_given() {
        let mut layout = open_room(20, 20);
        layout.place(Cell::new(10, 14), Terrain::PartialCover); // behind the spawn facing
        let mut s = State::new(
            layout,
            Cell::new(10, 10),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(18, 18),
        )
        .without_the_opening_look();
        let cell = render(&s).get(10, 14);
        assert_eq!(
            (cell.glyph, cell.fg, cell.vis),
            (SCHEMATIC_GROUND, Category::Ground, Visibility::Unexplored),
            "unscouted, a table is indistinguishable from the floor around it"
        );

        // Turn to face it: seen, it is the real thing.
        s.step(Input::Step(Direction::South));
        let cell = render(&s).get(10, 14);
        assert_eq!(
            (cell.glyph, cell.fg, cell.vis),
            ('π', Category::System, Visibility::Live),
            "in view, a table draws as itself"
        );

        // And it stays drawn after looking away — geometry once explored.
        s.step(Input::Step(Direction::North));
        let cell = render(&s).get(10, 14);
        assert_eq!(
            (cell.glyph, cell.fg, cell.vis),
            ('π', Category::System, Visibility::Explored),
            "a table you found stays found"
        );
    }

    /// The #307 acceptance test: **walking somewhere lights it up, permanently.**
    /// A cell the player has never had eyes on draws the schematic; sweeping the FOV
    /// over it promotes it to the real geometry, and it stays that way after they
    /// look away, because tile memory is monotonic (§11.5a).
    ///
    /// Asserted on the built frame rather than on the memory set, so what is pinned
    /// is what the player actually sees.
    #[test]
    fn exploring_promotes_the_schematic_to_real_geometry_for_good() {
        let mut s = State::new(
            open_room(20, 20),
            Cell::new(10, 10),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(18, 18),
        )
        .without_the_opening_look();
        // A stretch of the south wall, behind the north-facing player.
        let behind = Cell::new(10, 19);
        let cell = render(&s).get(behind.x, behind.y);
        assert_eq!(
            (cell.glyph, cell.vis),
            (SCHEMATIC_WALL, Visibility::Unexplored),
            "never looked at: the plans and nothing more",
        );

        // Turn south and walk toward it until it is in view.
        for _ in 0..4 {
            s.step(Input::Step(Direction::South));
        }
        let cell = render(&s).get(behind.x, behind.y);
        assert_eq!(
            (cell.glyph, cell.vis),
            ('#', Visibility::Live),
            "in view: the real wall",
        );

        // Walk away north again — well out of sight range.
        for _ in 0..8 {
            s.step(Input::Step(Direction::North));
        }
        let cell = render(&s).get(behind.x, behind.y);
        assert_eq!(
            (cell.glyph, cell.vis),
            ('#', Visibility::Explored),
            "explored stays explored: memory never decays (§11.5a)",
        );
    }

    /// The schematic must not leak through **either** channel (§11.5a, #307). Every
    /// unexplored cell wears one of exactly two glyphs and one of exactly two
    /// categories, whatever is really on it — so a cupboard cannot be spotted as the
    /// one System-tan mark in a Neutral wall run, nor a console as the odd glyph in a
    /// field of floor. If this ever fails, the fog has a hole in it.
    #[test]
    fn nothing_unexplored_is_distinguishable_from_its_neighbours() {
        let mut layout = open_room(20, 20);
        // One of everything worth hiding, all behind the north-facing player.
        layout.place(Cell::new(8, 15), Terrain::Hideout);
        layout.place(Cell::new(9, 15), Terrain::DuctEntry);
        layout.place(Cell::new(10, 15), Terrain::DoorPanelClosed);
        layout.place(Cell::new(11, 15), Terrain::DoorHinge);
        layout.place(Cell::new(12, 16), Terrain::PartialCover);
        layout.place(Cell::new(13, 16), Terrain::CommsConsole);
        let s = State::new(
            layout,
            Cell::new(10, 10),
            Direction::North,
            Vec::new(),
            [Cell::new(14, 16)], // an intel console too
            Cell::new(18, 18),
        )
        .without_the_opening_look();
        let g = render(&s);

        let mut seen: Vec<(char, Category)> = Vec::new();
        for y in 0..g.height() {
            for x in 0..g.width() {
                // The exit is the one documented exception: yours, and never
                // schematic (§4.5/§7.6). It is asserted on its own below.
                if Cell::new(x, y) == Cell::new(18, 18) {
                    continue;
                }
                let cell = g.get(x, y);
                if cell.vis == Visibility::Unexplored && !seen.contains(&(cell.glyph, cell.fg)) {
                    seen.push((cell.glyph, cell.fg));
                }
            }
        }
        seen.sort_by_key(|&(glyph, _)| glyph);
        assert_eq!(
            seen,
            vec![
                (SCHEMATIC_GROUND, Category::Ground),
                (SCHEMATIC_WALL, Category::Neutral),
            ],
            "unexplored geometry must speak exactly two marks in two colours",
        );

        // And spot-check the maskings individually, so a failure says which leaked.
        for (cell, mark) in [
            (Cell::new(8, 15), SCHEMATIC_WALL), // hideout alcove — backed by structure
            (Cell::new(9, 15), SCHEMATIC_WALL), // duct mouth — likewise
            (Cell::new(10, 15), SCHEMATIC_GROUND), // doorway — an opening, bears no load
            (Cell::new(11, 15), SCHEMATIC_WALL), // door frame — structure
            (Cell::new(12, 16), SCHEMATIC_GROUND), // table
            (Cell::new(13, 16), SCHEMATIC_GROUND), // comms console
            (Cell::new(14, 16), SCHEMATIC_GROUND), // intel console
        ] {
            assert_eq!(
                g.get(cell.x, cell.y).glyph,
                mark,
                "{cell:?} leaks through the schematic",
            );
        }

        // The exit, by contrast, is meant to stand out from turn one.
        let exit = g.get(18, 18);
        assert_eq!(
            (exit.glyph, exit.fg, exit.vis),
            ('E', Category::Interest, Visibility::Unexplored),
            "the tunnel you came in by is never hidden (§4.5/§7.6)",
        );
    }

    /// A doorway in an unscouted wall run reads as a **gap** (#307): the run draws
    /// `≈`, the opening `~`, so the ways between rooms you have never entered are
    /// still plannable. This is §11.5a's *"you can plan your escape route before
    /// you're spotted"* holding under the schematic, and the reason the fabric line
    /// is load-bearing structure rather than "anything solid" — a door bears no
    /// load, and a plan draws it as a break in the wall.
    #[test]
    fn an_unscouted_doorway_reads_as_a_gap_in_the_wall_line() {
        // A wall run across the room, well behind the north-facing player, with a
        // framed doorway in the middle of it.
        let mut layout = open_room(20, 20);
        for x in 6..=12 {
            layout.place(Cell::new(x, 15), Terrain::Wall);
        }
        layout.place(Cell::new(8, 15), Terrain::DoorHinge);
        layout.place(Cell::new(9, 15), Terrain::DoorPanelClosed);
        layout.place(Cell::new(10, 15), Terrain::DoorHinge);
        let s = State::new(
            layout,
            Cell::new(10, 10),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(18, 18),
        )
        .without_the_opening_look();
        let g = render(&s);

        let run: String = (6..=12).map(|x| g.get(x, 15).glyph).collect();
        let expected = format!(
            "{w}{w}{w}{g}{w}{w}{w}",
            w = SCHEMATIC_WALL,
            g = SCHEMATIC_GROUND,
        );
        assert_eq!(
            run, expected,
            "the wall run should show its doorway as a gap, frame included",
        );
    }

    /// The §12.6 `full_layout_known` modifier, directional (§2.3's anti-facade
    /// guard): with it on, geometry the player has never had eyes on draws as the
    /// real building instead of the schematic — the same picture the game gave
    /// everyone before #307.
    ///
    /// And the line it must not cross: it buys the **architecture, not the
    /// objectives**. A console and a cupboard are contents (§11.5a), so they stay
    /// masked by the geometry in their place, exactly as they were without the
    /// modifier. If this test ever goes green on a revealed `$`, the modifier has
    /// quietly become a cheat rather than a difficulty knob.
    #[test]
    fn the_full_layout_modifier_reveals_the_building_but_not_its_contents() {
        let build = |layout_known: bool| {
            let mut layout = open_room(20, 20);
            layout.place(Cell::new(8, 15), Terrain::DoorPanelClosed);
            layout.place(Cell::new(9, 15), Terrain::DuctEntry);
            layout.place(Cell::new(10, 15), Terrain::Hideout);
            layout.place(Cell::new(12, 16), Terrain::PartialCover);
            State::new(
                layout,
                Cell::new(10, 10),
                Direction::North,
                Vec::new(),
                [Cell::new(14, 16)],
                Cell::new(18, 18),
            )
            .without_the_opening_look()
            .with_modifiers(LevelModifiers {
                full_layout_known: layout_known,
                ..LevelModifiers::default()
            })
        };

        // Baseline: the schematic, as everywhere else in this module.
        let g = render(&build(false));
        assert_eq!(
            g.get(8, 15).glyph,
            SCHEMATIC_GROUND,
            "the doorway shows only as a gap — the panel's pose is unknown",
        );
        assert_eq!(g.get(9, 15).glyph, SCHEMATIC_WALL, "duct mouth hidden");
        assert_eq!(g.get(12, 16).glyph, SCHEMATIC_GROUND, "table hidden");

        // Modifier on: the building, drawn — and still honestly reported as
        // never-explored on the seam, because it has not been.
        let g = render(&build(true));
        assert_eq!(g.get(8, 15).glyph, '+', "the doorway is on the plans");
        assert_eq!(g.get(9, 15).glyph, '=', "so is the duct mouth");
        assert_eq!(g.get(12, 16).glyph, 'π', "so is the furniture");
        assert_eq!(g.get(4, 4).glyph, '·', "and plain floor is floor again");
        assert_eq!(
            g.get(8, 15).vis,
            Visibility::Unexplored,
            "given, not explored — the seam still tells the truth",
        );

        // But the contents are untouched: still masked by their own geometry.
        assert_eq!(
            g.get(10, 15).glyph,
            '#',
            "a cupboard stays hidden — the modifier buys the building, not the goals",
        );
        assert_eq!(g.get(14, 16).glyph, '·', "and so does an unscouted console");
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

    /// §11.5a: **geometry is never fogged.** Walls far beyond sight range still
    /// draw, so a route can be planned before the first risky step — as the
    /// schematic `≈` where the player has never been (#307), the real `#` where
    /// they have. What is fogged is what is *in* the building, never the building.
    ///
    /// The **exit** is the exception that never fogs at all: the player dug that
    /// tunnel and came in by it (§4.5), so it keeps its `E` and its Interest tint
    /// from turn one and goes on anchoring every escape plan (§7.6).
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

        // The far corner wall is way outside the 15-range box, yet drawn — as the
        // plans give it, since nobody has been down there.
        let far_wall = g.get(39, 29);
        assert_eq!(far_wall.glyph, SCHEMATIC_WALL);
        assert_eq!(far_wall.fg, Category::Neutral, "still a wall's colour");
        assert_eq!(far_wall.vis, Visibility::Unexplored);
        // The exit shows as itself even so: yours, and never schematic.
        let exit = g.get(35, 5);
        assert_eq!(exit.glyph, 'E');
        assert_eq!(exit.fg, Category::Interest);
        assert_eq!(exit.vis, Visibility::Unexplored);
        // Wall the player has eyes on right now draws as the real thing, live. (On
        // turn one memory *is* the FOV, so no cell is explored-but-unlit yet; the
        // transition into `Explored` is pinned by the contents tests above.)
        let near_wall = g.get(0, 4);
        assert_eq!(near_wall.glyph, '#');
        assert_eq!(near_wall.vis, Visibility::Live);
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
        )
        .without_the_opening_look();

        // Never seen and out of sense range: the intel masks as the schematic floor
        // of the unexplored room it stands in, and the guard is not drawn at all.
        let g = render(&s);
        assert_eq!(
            g.get(10, 14).glyph,
            SCHEMATIC_GROUND,
            "unseen intel is invisible"
        );
        assert_eq!(
            g.get(10, 14).fg,
            Category::Ground,
            "…its cell reads as floor"
        );
        assert_eq!(
            g.get(guard.x, guard.y).glyph,
            SCHEMATIC_GROUND,
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
        assert_eq!(g.get(guard.x, guard.y).vis, Visibility::Explored);
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
        )
        .without_the_opening_look();

        // A cupboard is an alcove recessed into a wall run, so on the plans it is
        // fabric: schematic wall, exactly like the run it sits in (#307). Both the
        // glyph *and* the category come from the mask — a lone System-tan mark among
        // Neutral ones would give the alcove away through the colour channel.
        let cell = render(&s).get(10, 14);
        assert_eq!(
            (cell.glyph, cell.fg, cell.vis),
            (SCHEMATIC_WALL, Category::Neutral, Visibility::Unexplored),
            "an unscouted hideout reads as the wall run it is cut into"
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

    /// §11.5a: a door's **position** is part of the building's fabric, but its
    /// open/closed pose is live state — once explored, a panel out of the FOV draws
    /// canonically closed, *even after the player has seen it open*. Memory holds
    /// contents, never state.
    ///
    /// Before it is explored the panel's *pose* is unknown but its **position is
    /// not** (#307): a doorway bears no load, so the schematic draws it as the gap
    /// in the wall line a plan would show, and the ways between unscouted rooms stay
    /// plannable (§11.5a).
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
        )
        .without_the_opening_look();

        // Never explored: the opening shows, the pose does not.
        let cell = render(&s).get(10, 14);
        assert_eq!(
            (cell.glyph, cell.fg, cell.vis),
            (SCHEMATIC_GROUND, Category::Ground, Visibility::Unexplored),
            "an unscouted doorway reads as a gap in the wall line"
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
            ('+', Visibility::Explored),
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
        )
        .without_the_opening_look();
        let g = render(&s);

        let lit = g.get(10, 8); // ahead: floor in the FOV
        assert_eq!((lit.glyph, lit.vis), ('·', Visibility::Live));
        // Behind: never-explored floor takes the schematic mark instead of the dot,
        // which is the sight boundary reading across open ground just as well —
        // by shape now rather than only by shade (#307).
        let dark = g.get(10, 14);
        assert_eq!(
            (dark.glyph, dark.vis),
            (SCHEMATIC_GROUND, Visibility::Unexplored)
        );
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
        )
        .without_the_opening_look();
        let watched_unseen = Cell::new(9, 13);
        assert!(s.guards()[0].fov().contains(watched_unseen), "in the cone");
        assert!(!s.player_fov().contains(watched_unseen), "not in the FOV");

        let cell = render(&s).get(9, 13);
        assert_eq!(cell.bg, Some(Category::Danger), "red even though unseen");
        // Threat outranks knowledge (§11.5 **[SETTLED]**): the cone paints over
        // never-explored ground exactly as over explored ground. The schematic
        // changes what the glyph *claims*, never what the detection set says — so
        // fix #1 holds on a cell in a wing the player has not entered.
        assert_eq!(
            (cell.glyph, cell.vis),
            (SCHEMATIC_GROUND, Visibility::Unexplored),
            "the glyph below stays the geometry, schematic here"
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
        )
        .without_the_opening_look();
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
            .without_the_opening_look()
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
        )
        .without_the_opening_look();
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
        )
        .without_the_opening_look();
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
        // The glyph is the geometry the cell masks as (schematic floor here, the
        // room being unexplored), *not* a glyph of the guard's own — the sensed
        // marker is a background, not a `g`.
        assert_eq!(
            cell.glyph, SCHEMATIC_GROUND,
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
        )
        .without_the_opening_look();

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
        )
        .without_the_opening_look();
        assert_eq!(
            s.perceive_guard(&s.guards()[0]),
            None,
            "out of range entirely"
        );

        let cell = render(&s).get(guard.x, guard.y);
        assert_eq!(
            cell.glyph, SCHEMATIC_GROUND,
            "the guard's cell is just unexplored floor"
        );
        assert_eq!(cell.fg, Category::Ground, "…not a sensed highlight");
        assert_eq!(cell.bg, None, "…and no orange background");
        assert_eq!(cell.vis, Visibility::Unexplored);
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

    /// The crawl leaves **no trace on the base map** (§11.5a/§10.7, #307). A duct
    /// buried in a thick wall — interior invisible from either room — is crawled end
    /// to end, and afterwards its interior still reads as schematic wall,
    /// indistinguishable from the run it threads through.
    ///
    /// This is the payoff of keeping interior cells out of tile memory: memory is
    /// what tells explored geometry from unexplored, so a remembered interior would
    /// draw the shortcut as a line of known `#` across the plans, giving away a route
    /// the design puts in its own private layer precisely so that nobody gets it for
    /// free.
    #[test]
    fn a_crawled_duct_leaves_no_trace_on_the_base_map() {
        let mut f = Facility::walled_box(9, 9);
        for y in 3..=5 {
            for x in 1..=7 {
                f.set_terrain(x, y, Terrain::Wall);
            }
        }
        f.set_terrain(2, 3, Terrain::DuctEntry);
        f.set_terrain(6, 5, Terrain::DuctEntry);
        let duct = crate::Duct::new(vec![
            Cell::new(2, 3),
            Cell::new(2, 4),
            Cell::new(3, 4),
            Cell::new(4, 4),
            Cell::new(5, 4),
            Cell::new(6, 4),
            Cell::new(6, 5),
        ]);
        let layout = crate::Layout::from_facility(f).with_ducts(vec![duct]);
        let mut s = State::new(
            layout,
            Cell::new(2, 2),
            Direction::South,
            Vec::new(),
            Vec::new(),
            Cell::new(7, 7),
        );

        for step in [
            Direction::South, // climb in at (2,3)
            Direction::South,
            Direction::East,
            Direction::East,
            Direction::East,
            Direction::East,
            Direction::South, // reach the far entry (6,5)
            Direction::South, // climb out into the lower room
        ] {
            s.step(Input::Step(step));
        }
        assert!(!s.in_duct(), "climbed out the far side");

        let g = render(&s);
        // The band's own unexplored wall, for comparison: a cell of the same hidden
        // middle row that no duct passes through. It has to come from row 4 — rows 3
        // and 5 are the band's faces and are plainly visible from the rooms.
        let plain = g.get(7, 4);
        assert_eq!(plain.glyph, SCHEMATIC_WALL);
        for x in 2..=6 {
            let cell = g.get(x, 4);
            assert_eq!(
                (cell.glyph, cell.fg, cell.vis),
                (plain.glyph, plain.fg, plain.vis),
                "interior cell ({x},4) must be indistinguishable from plain wall",
            );
        }
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
        )
        .without_the_opening_look();
        let g = render(&fogged);
        assert_eq!(
            g.get(10, 14).glyph,
            SCHEMATIC_GROUND,
            "the console masks as schematic floor"
        );
        assert_eq!(
            g.get(20, 30).glyph,
            SCHEMATIC_WALL,
            "the cupboard masks as schematic wall"
        );
        assert_eq!(
            g.get(guard.x, guard.y).glyph,
            SCHEMATIC_GROUND,
            "no guard drawn"
        );

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

    /// §8.3/§11.5 (#308/#338): the effect layer's **wash**. Firing Confusion washes the
    /// §6.1 box it reached in `Category::Effect` — asserted against the rule's own
    /// [`EffectArea`](crate::EffectArea) rather than a hand-drawn shape, so the picture
    /// and the blast can never drift apart, and painted **through walls and fog** (the
    /// reach of your own gadget is not something the fog can keep from you).
    #[test]
    fn the_effect_wash_paints_the_rules_own_box() {
        use crate::AbilityId;
        // One guard, so the blast has something to catch and is not refused (#325). It
        // stands behind the player, out of the north-facing FOV, so it is *sensed*
        // rather than seen — and a dazed sensed guard's dot is `Effect` too, so it does
        // not disturb the wash it sits in.
        let guard = Cell::new(15, 17);
        let mut s = state_holding_facing_north(
            30,
            30,
            Cell::new(15, 15),
            vec![Guard::stationary(guard)],
            AbilityId::Confusion,
        );
        // Before the fire, nothing on the board speaks the effect vocabulary at all.
        let quiet = render(&s);
        assert!(
            !any_effect_ink(&quiet),
            "with no effect fired the frame is exactly today's",
        );

        let area = fired_blast(s.step(Input::Activate(AbilityId::Confusion)));
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

    /// §8.3/§11.2 (#308/#338): a dazed guard the player **sees** keeps its threat-ladder
    /// glyph and takes the mark on its **background**. The effect always speaks in the
    /// background, so the `g` never loses the one channel that says what the guard was
    /// doing when the blast caught it — and the mark clears with the daze, since the
    /// freeze is a pause, not a reset.
    #[test]
    fn a_seen_frozen_guard_takes_the_mark_on_its_background() {
        use crate::state::CONFUSION_DAZE_TURNS;
        use crate::AbilityId;
        // Guard three cells north, inside the blast and in the FOV of a north-facing
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
        assert_eq!(g.get(10, 7).fg, ladder, "…still on the ladder it was on");
        assert_eq!(
            g.get(10, 7).bg,
            Some(Category::Effect),
            "…with the freeze said in the background",
        );

        // Let the daze run out on the guard's own clock (#325 — there is no window to
        // switch off any more): it resumes exactly where it was, and the mark under it
        // goes with the daze.
        for _ in 0..CONFUSION_DAZE_TURNS {
            s.step(Input::Wait);
        }
        let thawed = render(&s).get(10, 7);
        assert_eq!(
            thawed.fg, ladder,
            "the pause was a pause — the ladder never moved",
        );
        assert_ne!(
            thawed.bg,
            Some(Category::Effect),
            "the mark clears with the daze — no residue",
        );
    }

    /// §9.2/§8.3 (#308/#338): a frozen guard felt only **through a wall** carries the
    /// same background mark, cyan in place of the orange it refines. This is the common
    /// case, not the corner one: the blast reaches through walls, so most of what it
    /// freezes is exactly what the player cannot see — and the mark outranking `Sensed`
    /// on the guard's own cell is what keeps "and it cannot move" readable there.
    #[test]
    fn a_sensed_frozen_guard_takes_the_mark_on_its_highlight() {
        use crate::AbilityId;
        // A wall between the player at (10,10) and the guard at (10,7): inside the
        // blast (distance 3) and inside the sense box, but out of sight.
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
    fn red_still_wins_inside_the_blast() {
        use crate::AbilityId;
        // The watcher at (10,2) is eight cells north — outside the blast, so it stays
        // awake — looking south down the column, over the frozen guard at (10,10) and
        // on across the cells the blast covers.
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
        let area = fired_blast(s.step(Input::Activate(AbilityId::Confusion)));
        assert!(s.guard_confused(&s.guards()[0]), "the near guard is frozen");
        assert!(
            !s.guard_confused(&s.guards()[1]),
            "the watcher is outside the blast",
        );
        let watcher_cone: Vec<Cell> = s.guards()[1].fov().cells().collect();
        assert!(
            watcher_cone.contains(&Cell::new(10, 10)),
            "precondition: the watcher's cone covers the frozen guard",
        );

        let g = render(&s);
        assert_eq!(
            g.get(10, 10).bg,
            Some(Category::Danger),
            "the red the frozen guard stands in outranks its own mark",
        );
        // Every watched cell inside the footprint is red, not cyan.
        for &cell in watcher_cone.iter().filter(|&&c| area.contains(c)) {
            assert_eq!(
                g.get(cell.x, cell.y).bg,
                Some(Category::Danger),
                "{cell:?} is watched, so it reads red inside the blast too",
            );
        }
    }

    /// §9.4/§11.5 (#308/#338): the **orange sense channel** beats the effect wash. A door
    /// that shuts itself inside the blast keeps its `Sensed` cue — evidence someone
    /// passed is a fact about the world, and an advisory wash never paints over one.
    /// (A mark on a *thing* is the other case, and rightly the other way round: it
    /// refines a cue rather than competing with it.)
    #[test]
    fn a_sensed_door_cue_survives_the_wash() {
        use crate::region::{DoorKind, RegionGraph, RegionKind};
        use crate::AbilityId;
        // Two rooms joined by an automatic door down column 3 (§10.4/#147); the player
        // stands beside it, well inside the blast.
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
        // A fixture guard in the far room, inside the blast, so the firing is not
        // refused for want of anything to catch (#325). It never moves, so the door's
        // own clock — what this test is about — is untouched.
        let mut s = State::new(
            crate::Layout::from_parts(f, graph),
            Cell::new(2, 2),
            Direction::East,
            vec![Guard::stationary(Cell::new(5, 1))],
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

    /// §8.3/§11.5 (#242/#338): Lockdown paints **both** marks, exactly as Confusion
    /// does — a momentary wash over the box it fired with, gone after
    /// [`EFFECT_FLASH_TURNS`](crate::EFFECT_FLASH_TURNS), and a standing mark over each
    /// sealed doorway that keeps for as long as the window holds the seal. The wash
    /// answers *this far*; the doorways answer *these ones*, which is what tells the
    /// player which doors the guards can no longer work.
    #[test]
    fn a_sealed_door_is_marked_for_the_whole_window() {
        use crate::AbilityId;
        let mut s = State::new(
            crate::test_support::region_strip(),
            Cell::new(2, 2),
            Direction::East,
            Vec::new(),
            Vec::new(),
            Cell::new(14, 4),
        )
        .with_loadout(Loadout::innate().with(AbilityId::Lockdown));

        assert!(
            render(&s).get(4, 2).bg != Some(Category::Effect),
            "precondition: nothing sealed yet",
        );
        // The firing frame washes the whole reach — the box says *this far*, once.
        s.step(Input::Activate(AbilityId::Lockdown));
        let g = render(&s);
        // A cell at the box's east edge: inside the reach, in bounds, and no doorway —
        // so it is washed on this frame and bare once the wash goes.
        let reach = s.lockdown_area();
        let edge = Cell::new(reach.centre().x + reach.radius(), reach.centre().y);
        assert!(s.layout().regions().door_at(edge).is_none(), "plain floor");
        assert_eq!(
            g.get(edge.x, edge.y).bg,
            Some(Category::Effect),
            "the wash covers the box on the firing frame",
        );

        // Several turns on — past the wash's life — only the doorways are still drawn,
        // because that mark is the state and not the moment.
        for _ in 0..crate::EFFECT_FLASH_TURNS + 2 {
            s.step(Input::Wait);
        }
        let g = render(&s);
        assert_ne!(
            g.get(edge.x, edge.y).bg,
            Some(Category::Effect),
            "the wash has burned out",
        );
        for y in 1..4 {
            assert_eq!(
                g.get(4, y).bg,
                Some(Category::Effect),
                "the sealed door is marked over its whole footprint at (4,{y})",
            );
        }
        assert!(
            g.get(7, 2).bg != Some(Category::Effect),
            "and the door out of reach is not",
        );
    }

    /// §8.3 (#308/#325): **walking moves nothing**. The blast decided its set the
    /// moment it fired, so the marks are a fact about the guards and not about where
    /// the player is standing: the one it caught stays marked as the player runs away
    /// from it, and the one it missed stays awake as the player walks toward it.
    ///
    /// This is the inversion #325 is for. The old bubble re-read distance every turn,
    /// which made the ability a mobile no-guard-may-act field rather than a panic-buy
    /// of time (§8.3's "no shield").
    #[test]
    fn walking_moves_no_marks() {
        use crate::AbilityId;
        // Two guards up the column from a north-facing player at (10, 14): one at
        // distance 4, inside the blast; one at 7, a cell past its edge.
        let (caught, missed) = (Cell::new(10, 10), Cell::new(10, 7));
        let mut s = state_holding_facing_north(
            20,
            24,
            Cell::new(10, 14),
            vec![Guard::stationary(caught), Guard::stationary(missed)],
            AbilityId::Confusion,
        );
        s.step(Input::Activate(AbilityId::Confusion));
        let marked: Vec<Cell> = s.effect_thing_marks().collect();
        assert_eq!(marked, vec![caught], "caught — and only it");
        // Asserted on the layer rather than on the frame, because the awake guard's own
        // cone paints its neighbour red and `Danger` outranks the mark (§11.5): what
        // this test is about is which guards are *held*, and the two render tests above
        // already pin how a held guard is drawn.

        // Run: four steps south carry the caught guard well outside the box the blast
        // covered, and behind the player's back besides. It is still dazed — the count
        // is its own — and the mark rides its sensed dot instead of its glyph (§9.2),
        // which is the same mark saying the same thing.
        for _ in 0..4 {
            s.step(Input::Step(Direction::South));
        }
        assert!(
            s.player().sight_distance(caught) > CONFUSION_RADIUS,
            "precondition: outside the box the blast covered",
        );
        assert!(
            s.effect_thing_marks().any(|c| c == caught),
            "running away thaws nobody",
        );

        // …and walking the other way, right up to the guard the blast missed, dazes it
        // no more than running dazed the first one. There is no field left to enter.
        for _ in 0..5 {
            s.step(Input::Step(Direction::North));
        }
        assert!(
            s.player().sight_distance(missed) <= CONFUSION_RADIUS,
            "precondition: inside the reach the blast had",
        );
        assert!(
            s.effect_thing_marks().next().is_none(),
            "walking into range dazes nobody: the blast is over",
        );
    }

    /// The [`EffectArea`](crate::EffectArea) a `step`'s events say Confusion fired with
    /// — the object the daze was computed from, and so the one a painted wash is
    /// asserted against.
    fn fired_blast(events: Vec<Event>) -> crate::EffectArea {
        events
            .into_iter()
            .find_map(|e| match e {
                Event::ConfusionFired { blast, .. } => Some(blast),
                _ => None,
            })
            .expect("the blast went off")
    }

    /// Whether any cell of `grid` speaks the effect vocabulary at all — the "with
    /// nothing running, the frame is exactly today's" check (#308). It looks at both
    /// channels even though #338 settles the layer on the background alone, so a glyph
    /// that ever claimed `Category::Effect` would fail this too.
    fn any_effect_ink(grid: &Grid) -> bool {
        (0..grid.height()).any(|y| {
            (0..grid.width()).any(|x| {
                let cell = grid.get(x, y);
                cell.fg == Category::Effect || cell.bg == Some(Category::Effect)
            })
        })
    }
}
