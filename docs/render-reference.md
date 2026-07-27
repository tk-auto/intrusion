# Render reference — glyphs and palette

The complete table of what Intrusion draws and what colour it draws it in, with
the reasoning behind each choice.

This is a **reference, not a design document**. [`docs/design.md`](design.md) owns
the rules — §11.1 the grid, §11.2 colour, §11.3 glyphs, §11.5 field of view,
§11.5a fog — and this file records the concrete values those rules resolve to, in
one place, so a question like *"what does `≈` mean?"* or *"why is the exit purple?"*
has an answer that is not spread across four sections and two source files. Where
the two disagree, the design doc wins and this file is stale.

**The values live in code, not here.** The glyph table is
[`Terrain::glyph`](../crates/core/src/facility.rs) plus the entity constants in
[`crates/core/src/render.rs`](../crates/core/src/render.rs); the palette is the one
table in [`crates/web/src/lib.rs`](../crates/web/src/lib.rs). The in-game glyph
legend (the `[?]` panel's **Help** tab) derives its rows from those same sources, so
it cannot drift from the board. This file is the prose companion to all three.

---

## 1. The seam

The core never names a colour. It emits a grid of cells, each carrying a **glyph**,
an **information category**, an optional **background category**, and a
**knowledge state**; the shell turns category + knowledge into pixels through a
single table. That is the §11.2 **[SETTLED]** rule, and it is what makes an
accessibility reskin a one-table edit.

So there are three independent channels, and each says a different kind of thing:

| Channel | Says | Owned by |
|---|---|---|
| **Glyph** | What is there | core (§11.3) |
| **Foreground colour** | What it *means* — the information category | core declares, shell colours (§11.2) |
| **Background colour** | Threat and reach — the danger overlay, the sense cue, an effect's footprint | core declares, shell colours (§11.5) |

Knowledge (§11.5a) modulates the first two. It never touches the third: **threat
outranks knowledge**, always.

---

## 2. Glyphs

### 2.1 Entities

| Glyph | Entity | Category |
|---|---|---|
| `@` | The player | Owned |
| `@` | A decoy you placed | Owned |
| `g` | A guard you can see | Caution / Warning / Danger, by its state |
| *(none)* | A guard you only sense through a wall | Sensed — a background highlight on its cell, no glyph |
| `z` | A body | Caution; **Owned** while carried, or stowed in a cupboard |

A seen guard's colour *is* the AI state machine, read directly: yellow → orange →
red. A sensed guard deliberately has no glyph, because a glyph would imply a
readable mind and the player has no such information — only a position.

### 2.2 Terrain

| Glyph | Terrain | Category |
|---|---|---|
| `#` | Wall | Neutral |
| `·` | Floor | Ground |
| `+` | Door panel, closed | System |
| *(blank)* | Door panel, open | Ground — the gap in the wall *is* its rendering |
| `×` | Door frame (hinge) | System |
| `}` | Cupboard | System; **Owned** while you are hiding in it |
| `π` | Table (partial cover) | System; **Owned** while it conceals you |
| `=` | Duct mouth | System |
| `$` | Intel console | Interest; **Neutral** once spent |
| `Ψ` | Comms console | Interest; **Neutral** once used |
| `E` | The exit | Interest |

Floor draws as a **dot rather than a blank** (§11.5 fix #2). A blank cell has no
foreground, so the dimming that encodes the sight boundary was invisible across
open ground — you could only see the edge of your vision where it happened to
cross a wall. The dots give every floor cell something for the lighting to act on.
They are deliberately the quietest thing on the board.

Two glyphs recolour rather than change shape when their meaning shifts — a spent
console (`$` Interest → Neutral) and a hiding place holding *you* (`}` System →
Owned). Shape is *what it is*; colour is *what it means to you now*.

### 2.3 The schematic

| Glyph | Means | Category |
|---|---|---|
| `≈` | Building fabric you have never had eyes on | Neutral |
| `~` | Floor space you have never had eyes on | Ground |

You always have the building's **plans** (§11.5a **[SETTLED]**: geometry is never
fogged, from turn one), but plans are not the same as having been there. The
schematic is what the plans give you: the **load-bearing fabric** — wall runs and
the recesses cut back into them — and everything that is **not** holding the
building up. Walk in and it resolves into what is really there, permanently (tile
memory is monotonic).

`≈` is the mathematical *approximately*, which is exactly the claim a plan makes
about a stretch of building nobody has walked.

**What is fabric and what is floor** is an architectural line, not a mechanical
one — it does not follow passability:

| Reads as `≈` fabric | Reads as `~` floor space |
|---|---|
| Wall | Floor |
| Door **frame** | Door**way** |
| Cupboard alcove | Table |
| Duct mouth | Intel and comms consoles |

The test is *does it hold the building up*. A cupboard alcove and a duct mouth are
recesses cut back into a run, still backed by structure, so they belong to the run.
A **doorway** bears no load, so it draws as the **gap in the wall line** an
architectural plan would show — its frame stays `≈`, so an unexplored wing reads
`≈≈≈~≈≈≈` and the ways between its rooms can be planned before you set foot in
them. What the doorway does *not* tell you is the panel's pose: a door's
open/closed state is live state and is never remembered.

A table stands *in* a room rather than holding it up, so it reads `~` — and a table
blocks movement, which means the schematic can be optimistic about a route through
an unscouted room. That is deliberate: what you can plan is the building, and what
a room turns out to contain is what exploring is for.

**Everything unexplored collapses to exactly two glyphs in exactly two colours.**
This is load-bearing, not tidiness: a cupboard drawn as the one System-tan mark in
a Neutral wall run would give away, through the colour channel, precisely the alcove
the glyph channel is hiding. Both channels have to mask, or neither does.

**The exit is the one exception.** It keeps its `E` and its Interest purple from
turn one, never schematic, because the player dug that tunnel and came in by it
(§4.5) — it is the one part of this building that is theirs, and it anchors every
escape plan (§7.6).

**A duct's interior is not on this ladder at all.** The crawl path between two
mouths is a private fourth layer (§10.7): it is never absorbed into tile memory, so
after crawling it the cells still read as whatever the building around them reads
as. Only the two mouths are ever drawn — and, being fabric, they must be found.

### 2.4 Why shape rather than a fourth brightness level

The alternative was a fourth rung on the §11.5 dimming ladder: never-explored
geometry drawn darker than explored. It was rejected, and the reasons are worth
keeping:

- **There is no room at the bottom.** Ground's dim shade is already deliberately
  quiet (the dots whisper). A step below it, on a true-black backdrop, is close to
  invisible — and geometry that cannot be read is de-facto fog, which §11.5a settles
  against.
- **Shape survives a small screen.** The board is fitted whole, with no camera
  (§11.4), so on a phone the cells are small. A one-stroke-versus-two-stroke
  difference reads at sizes where a twenty-point luminance step does not.
- **It costs no colour.** The threat channels (Danger red, Sensed orange) keep the
  background entirely to themselves, and a knowledge readout can never compete with
  them.
- **A second palette gets it for free.** Light mode (#189) needs no extra values
  for a shape channel.

> **Tried against a denser mark, and kept.** The schematic was built twice on the
> same seed and compared on a real 40×40 board: `≈` as shipped, and `▒` — a shade
> block, the architectural hatch for a wall in section.
>
> `▒` unquestionably reads the *building* better. Corridors, room shapes and
> doorway gaps are legible across the whole unexplored region at a glance, where
> with `≈` they are closer to texture. The reason is worth knowing: in the explored
> picture wall-versus-floor is carried by **ink density** (`#` is dense, `·` is one
> dot) far more than by the colour gap between the Neutral and Ground dims, and two
> marks of similar density leave that colour gap working alone.
>
> It was rejected anyway, because it **inverts the lighting**. A filled block puts
> down so much ink that unexplored territory becomes the loudest thing on the
> board — the explored room reads as a dark patch inside a bright mass — which is
> backwards for §11.5, where live is bright and the unknown recedes, and it puts a
> heavy fill in the register the danger overlay needs to own. A quieter plan that
> never competes with threat beat a legible one that does.
>
> The live option if this is revisited is neither mark but a third: `▒`'s density
> with a **darker shade of its own** for the schematic fabric, so structure reads
> without shouting. That spends a palette value, which the shape channel was chosen
> to avoid — so it is a real trade, not a free improvement. Whatever is tried, judge
> it on a screenshot of a full board and never on a unit test: the text frame looks
> correct in every one of these variants.

---

## 3. Knowledge states

Four states, and they are disjoint and well-ordered because memory accumulates
*from* the field of view — every cell in the FOV is in memory, so a cell is exactly
one of these.

| State | Meaning | Drawn as |
|---|---|---|
| **Live** | In your field of view right now | Full category colour |
| **Explored** | You have had eyes on it; not right now | Real glyph, the row's dim shade |
| **Unexplored** | Never in your field of view | The schematic, same dim shade |
| **Remembered** | A *content* you saw earlier | Its real glyph, in the memory slate |

`Explored` and `Unexplored` share a colour on purpose — the schematic separates
itself by shape, so the distinction needs no shade of its own. The core still
records which is which, because it is a fact about the player's knowledge that
things other than the renderer need (a modifier that hands over the full layout;
a future rule that fogs geometry outright).

`Remembered` is a genuinely separate visual state, not a rung on the dimming
ladder — a muted slate, distinct from every live category *and* from the dim gray,
so memory reads as memory rather than as a thing that is merely far away.

The three layers those states apply to (§11.5a **[SETTLED]**):

| Layer | Visibility |
|---|---|
| **Geometry** — walls, floor, room shapes | Always drawn, from turn one — as the schematic until explored |
| **Contents** — intel, cupboards, ducts, doors, furniture | Hidden until seen; then remembered |
| **Live state** — guards, bodies, a door's pose | Only what you can see right now; never remembered |

The pairing is the point: **you plan confidently against the building's bones and
get surprised by what is in it**, not by the architecture. Being surprised by a wall
is annoying; finding an empty room where you expected the intel is a decision.

---

## 4. Colour

### 4.1 Categories

Systems declare an **information category**; presentation owns the mapping. No game
system anywhere names a colour.

| Category | Colour | Row | Means |
|---|---|---|---|
| **Neutral** | White | `#ffffff` | Inert scenery, walls, spent objectives |
| **Ground** | Dark gray | `#4a4a4a` | Traversable floor — drawn to recede |
| **Owned** | Blue | `#4ea6ff` | You, and what you made |
| **Caution** | Yellow | `#f0e442` | A threat that is unaware |
| **Warning** | Orange | `#e69f00` | A threat that is hunting |
| **Danger** | Red | `#ff3333` | A threat that has you |
| **Interest** | Purple | `#bd6bd6` | Goals and rewards |
| **System** | Tan | `#9a7040` | Doors, cupboards — neutral furniture |
| **Sensed** | Orange | `#e69f00` | Felt through a wall — **background only** |
| **Effect** | Cyan | `#2ee6d6` | An ability effect of your own making — **background only** |

The base palette is a **16-colour, colour-blind-safe qualitative set**, hues leaning
on Okabe–Ito and brightened for the dark backdrop. Ten rows carry categories today;
the spares are claimed by naming them.

Constraints the tests enforce, so a recolour cannot quietly break them:

- **Every pair is visibly distinct** at a minimum RGB distance. The old palette had
  a tan that blurred into Caution's yellow; that specific regression is pinned.
- **The threat ladder is separated by luminance as well as hue**, so
  yellow → orange → red survives a red-green deficiency.
- **Ground recedes beneath every other category**, and its live and dim shades stay
  far enough apart that the sight boundary reads across open floor.
- **The memory slate stands apart from every live colour** and from the dim gray —
  memory that could be mistaken for a live glyph would defeat the whole three-state
  scheme.
- **The dim exit still reads as purple**, well clear of both wall gray and the memory
  slate.

### 4.2 Two orange categories, and why they never collide

**Warning** and **Sensed** share a hue. They never share a cell role: Sensed only
ever paints a *background*, never a glyph, and Warning only ever a glyph. A hunting
guard you can see is an orange `g`; a guard you can only feel through a wall is an
orange *filled cell* with no glyph at all. The bloom from one to the other, the
moment you round the corner, is the seen/sensed distinction made visible.

### 4.3 Full range

Each row carries four values: a full-strength foreground, the **dim** shade the same
glyph draws in outside your field of view, and two darkened **background** variants
— one for a cell you can see, one for a cell beyond it.

The palette is deliberately **full-range**: true black and true white are both in it.
The old game pushed every colour through a gamma curve that compressed everything
into a washed 0.1–0.9 band, and six of its sixteen colours were never used at all.
Compression gets added back only if something demands it. **[START]**

Three rows carry their own dim rather than the shared dark gray, each for a reason:

- **Ground** recedes further than everything else — the floor dots must whisper.
- **Interest** keeps a readable purple tint, because the exit anchors every escape
  plan and must never sink into wall gray.
- **Effect** keeps its cyan tint, so the help card's colour key names it in a shade
  nothing else claims. (Since #338 the layer paints no glyph on the board at all, so
  this row's dim shade is chrome, not board ink.)

---

## 5. Backgrounds

Backgrounds are the threat channel, and there is a fixed precedence:

**Danger > Effect mark on a thing > Sensed > Effect wash.**

| Background | Means |
|---|---|
| **Danger** (red) | This cell is watched by a guard **you can see** |
| **Effect** on a thing (cyan) | The guard here is held by one of your effects, or the `@` here is a live decoy rather than you |
| **Sensed** (orange) | A guard felt through a wall, or a door that just changed away from you |
| **Effect** wash (cyan) | Where your own gadget acted — a blast's box, a bored cell, the doorways a lockdown holds |

The effect layer appears twice on purpose (#338). Its **wash** is advisory geometry and
the weakest cue on the board. Its mark on a **thing** is not a competing claim about the
cell but a *refinement of the cue that thing already draws* — "exactly here" becomes
"exactly here, and it cannot move"; a second Owned `@` becomes "and that one is the
ability running" — so it sits above the orange it refines and still below the red that
outranks everything.

**The danger overlay is the best idea in the old game, and it is [SETTLED].** It
paints the *literal* detection set — the same sight data the guard AI queries, not a
re-implementation that could disagree. If your cell is not red, no guard you can see
will detect you. **The lose condition, painted.** It is what makes stealth plannable
rather than guessy.

Two consequences that follow, and must not regress:

- It covers watched cells **outside** your own field of view, because a visible
  guard's cone is knowledge you have. The old version rendered those dark-on-dark,
  which made the watched cells you could not see into look like the *safest* on the
  map — actively misleading.
- It paints over the schematic exactly as it paints over explored ground. The
  schematic changes what a glyph *claims*; it never changes what the detection set
  says.

Cones of guards you **cannot** see paint nothing — that is information you have not
earned, and painting it would leak what you have not scouted.

The cyan channel carries two things, and the difference is worth knowing. The
**footprint** is the one-frame wash that answers *how far* — Confusion's bubble,
Lockdown's radius — and then goes. The **marks** are what carry the state for the rest
of the window and cost almost no ink: a frozen guard recoloured cyan (§8.3 Confusion)
and every cell of a **sealed door** while Lockdown holds it (#242). A mark says *this
one*, where the footprint said *this far*.

**Sensed and Effect are not fogged.** Both are certain, position-only knowledge that
travels through walls, so they paint at full strength regardless of the knowledge
state of the cell underneath. Fogging an effect's footprint would teach you its
extent only where you were already looking, which is exactly the corner the flash
exists to light.
