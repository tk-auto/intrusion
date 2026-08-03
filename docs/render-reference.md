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
| `z` | A body | Caution — trouble waiting to be found |
| `z` | A body in your hands | Owned — yours, and in play |
| `z` | A body stowed in a cupboard | Neutral — the cupboard is spent |

A seen guard's colour *is* the AI state machine, read directly: yellow → orange →
red. A sensed guard deliberately has no glyph, because a glyph would imply a
readable mind and the player has no such information — only a position.

The body's three colours are one question asked once: *what is this doing for me
right now?* Loose, it is a liability on the §7.3 clock — Caution. In your hands, it
is yours and in play — Owned. Stowed, the cupboard has swallowed it: you cannot
climb in, the bump is a no-op, nothing about it is still working for you. That is a
**spent object**, and it takes the spent object's colour, exactly as the drained
`$` console does two tables down. The `z` stays put either way, because the mark's
job — telling you at a glance which cupboards you have used up — never depended on
the colour. Owned is the tighter word for it: on a cupboard, Owned already means
*you are hidden in this one* (`}`, §2.2 below), so a spent cupboard wearing it would
put two opposite readings in one ink on one piece of furniture.

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
| `=` | Duct mouth | System; **memory slate** once scouted and out of view (§3) |
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

The **contents** rows — `}`, `=`, `$`, `Ψ` — take the memory slate rather than the
dim shade once they are out of view, which is the §3 knowledge state and not a
category of their own. Only `=` names it in the table above, because it is the one
whose layer moved (#450) and the note is there to stop it drifting back.

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
escape plan (§7.6). It keeps that face **while you are crawling it**, too (#466): the
occupied-run pass below lights the tunnel `=` up to the mouth and leaves the mouth alone,
so `E` is never an anonymous stretch of crawlspace.

**A duct's interior is not on this ladder at all.** The crawl path between two
mouths is a private fourth layer (§10.7): it is never absorbed into tile memory, so
after crawling it the cells still read as whatever the building around them reads
as. Only the two mouths are ever drawn — and, being fabric, they must be found.

**The player's own tunnel is drawn by that same rule, and turn one is when you see it**
(§4.5/#466). Every run begins *inside* it, on the level border, so the occupied run —
border cell to `E`, one connected `=` — is the opening frame: a bright line pointing from
where you are to where you are about to be, on a board that otherwise gives the eye
nothing. Climb out and it hides again like any other duct; `E` stays drawn, so the way
back is never lost.

**It wears Interest, where a found shortcut wears System.** A shortcut is furniture — the
band the doors and cupboards are in — but the tunnel is the thing `E` anchors, so the run
takes the exit's own colour and the opening frame reads as one continuous purple line
rather than a gray thread ending in a purple letter. The **glyph** stays `=` either way:
that is what a crawlspace is, and a second `E` on the board would lie about where the
mouth is.

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
| **Geometry** — walls, floor, room shapes, doors, furniture | Always drawn, from turn one — as the schematic until explored, then the row's dim shade |
| **Contents** — intel, comms, cupboards, duct mouths | Hidden until seen; then **remembered**, in the memory slate |
| **Live state** — guards, bodies, a door's pose | Only what you can see right now; never remembered |

The pairing is the point: **you plan confidently against the building's bones and
get surprised by what is in it**, not by the architecture. Being surprised by a wall
is annoying; finding an empty room where you expected the intel is a decision.

**Doors and furniture sit in Geometry here, and that is the render's own rule.**
This table is about what a cell draws as when it leaves your sight, and on that
question both take the shared dim shade rather than the memory slate — deliberately,
for two different reasons:

- **A door's pose is live state**, redrawn canonically closed every frame out of
  view (`real(Terrain::DoorPanelClosed)`). A slate door would be a memory colour on
  a drawing that is not a memory, competing with the live pose the moment you can
  see it again.
- **Doors are everywhere.** The memory slate earns its distinctness by being rare —
  it is the ink that says *you found this*. Slating every panel in the facility would
  bury the two or three marks that actually change a plan under a building's worth of
  doorways.

Furniture keeps the same shade for the second reason. Neither is a claim that a door
is load-bearing: on the *schematic* a doorway still draws as the gap in the wall line
a plan would show, and a table still draws as floor space, which is §2.3–§2.4's
question, not this one.

**A duct mouth is Contents** (#450) — it moved here from Geometry, where it drew its
`=` in the same dim gray a wall dims to and so read as one more piece of building the
moment you looked away. §10.7 makes a duct an escape a pursuer cannot follow, which is
exactly why a mouth found once should stay on the map in its own ink: it is a route
you plan with, in the way §2.3's exit anchors every escape plan. The mouths are still
masked as fabric until scouted, and the duct's *interior* is not on this table at all
— it is the private fourth layer §10.7 gives it, never absorbed into tile memory.

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
the spares are claimed by naming them. The hex above is the **dark** theme's; §4.4
covers the light one, which uses the same rows for the same categories and different
values for all of them.

Constraints the tests enforce, so a recolour cannot quietly break them. The first two
are asserted **at background strength as well as foreground** (#419) — they used to be
claims about the `fg` column alone, which is how a threat ladder of three near-blacks
shipped without anything objecting:

- **Every pair is visibly distinct** at a minimum RGB distance. The old palette had
  a tan that blurred into Caution's yellow; that specific regression is pinned. The
  background half measures what the screen actually paints, so an out-of-view `Sensed`
  fill (full strength, never fogged) is compared against an out-of-view `Danger` fill
  (dimmed) — the pair a player really has side by side.
- **The threat ladder is separated by luminance as well as hue**, so
  yellow → orange → red survives a red-green deficiency — and so does the band of a
  near line carrying the alert condition, which is what the fills are for.
- **A cell you can see and a cell beyond it are tellable apart**, for every category
  that paints a fill — the general form of §11.5's *watched must never look safe*,
  which had only ever been checked for the danger overlay. `Sensed` and `Effect` are
  outside it: they have no second shade by design (§5).
- **The near line's words read over every band.** The row is a solid category band
  with its text in Neutral (§11.4), so how far the backgrounds may be lifted off the
  page is bounded by the ink that has to stay legible on them.
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
glyph draws in outside your field of view, and two **background** variants — one for a
cell you can see, one for a cell beyond it. Both recede *toward the page* (§4.4), which
is a darkening on the dark theme and a lightening on the light one.

**Two surfaces choose between the background pair, and they choose on different
grounds** (#420). The **map** chooses by fog: in view, the full fill; beyond it, the
quiet one. The **HUD** has no fog to consult and chooses by what a row *is* — **an
ambient band paints the quiet fill, a message band the full one** — so the near line's
colour separates the facility's standing mood, permanently on screen and therefore not
news, from something that has just happened and flashes. It also keeps a standing
Danger row from spending the §5 danger overlay's own fill, which means *a threat has
you right now*; a permanent row wearing it would dilute the one place that is true.

A cell carries the answer, not the reason: `GlyphCell.fill`, alongside the knowledge
state that styles its glyph. The alternative — handing a HUD row `Visibility::Explored`
to get the quiet shade for free — would pick the right colour by telling a lie, since
`Visibility` means fog knowledge and a status row has none.

They are **placed by luminance, and they sit well off the page** (#419). Each rung of
the threat ladder is a real step below the last, in the same direction the foregrounds
take, so a fill reads as *which* rung rather than as "a dark warm colour". Where two
hues would otherwise compress together at background strength — tan against orange,
cyan against blue, either against the neutral gray — the shade carries more saturation
than a straight scaling of its foreground would give it, because a dark colour needs
more of it to read as a hue at all.

The values they replaced are worth naming, since they are the failure the ticket was
filed for: the dark theme's beyond-view ladder was `#302e0d` / `#2e2000` / `#521717` on
a black page — three near-blacks separated almost entirely by hue, at the luminance
where hue discrimination is worst, with the red *brighter* than the orange so the ladder
doubled back on itself. The bound on lifting them is the near line's ink: the words are
Neutral over the band (§4.1), and they have to stay readable.

The palette is deliberately **full-range**: true black and true white are both in it.
The old game pushed every colour through a gamma curve that compressed everything
into a washed 0.1–0.9 band, and six of its sixteen colours were never used at all.
Compression gets added back only if something demands it. **[START]**

Three rows carry their own dim rather than the shared gray, each for a reason:

- **Ground** recedes further than everything else — the floor dots must whisper.
- **Interest** keeps a readable purple tint, because the exit anchors every escape
  plan and must never sink into wall gray.
- **Effect** keeps its cyan tint, so the help card's colour key names it in a shade
  nothing else claims. (Since #338 the layer paints no glyph on the board at all, so
  this row's dim shade is chrome, not board ink.)

### 4.4 Two themes

There are two palettes — **dark** (the default) and **light** — and switching between
them is the whole payoff §11.2 was written to buy: no game system names a colour, so a
theme is a second column of the one category→colour table and nothing else. The core
carries a `Theme` flag on its view state and never a value from either column.

| Category | Dark | Light |
|---|---|---|
| **Neutral** | `#ffffff` | `#000000` |
| **Ground** | `#4a4a4a` | `#b8b8b8` |
| **Owned** | `#4ea6ff` | `#0060c0` |
| **Caution** | `#f0e442` | `#b09600` |
| **Warning** / **Sensed** | `#e69f00` | `#cc4c00` |
| **Danger** | `#ff3333` | `#b00000` |
| **Interest** | `#bd6bd6` | `#7b2fa0` |
| **System** | `#9a7040` | `#6b4320` |
| **Effect** | `#2ee6d6` | `#00857c` |
| *page backdrop* | `#000000` | `#ffffff` |
| *memory slate* | `#667a8a` | `#3f5f80` |

Every constraint in §4.1 is enforced over **both** palettes — they are claims about
what a player can tell apart, not about a particular set of hex values. Two of them
had to be restated to survive the second theme, because the dark table let a rule and
its accident be spelled the same way:

- **"Darkened variants" was really "variants that recede toward the page."** On black
  those are the same sentence; on white they are opposites. Dim shades and background
  variants are now measured by *distance from the backdrop*, which reads correctly in
  both directions.
- **"Ground recedes" was really "Ground stands off the page least."** On black the
  floor dots are the darkest thing on screen; on white they are the lightest.

What is *not* generalisable is the hues. A light theme is not the dark one inverted:
Okabe–Ito's yellow is bright by construction and disappears on white, so Caution
becomes a dark gold; Danger trades brightness for depth. The gold/orange/tan cluster
is the whole difficulty — three warm hues that must stay pairwise distinct while all
three are dark enough to read on a white page — and it is the reason the light
Caution keeps as much green as it can while Warning is pushed toward red. That cluster
is the hardest part of §4.3's background re-tone too, and on the pale tier it is
*four* warm rows rather than three, since System's tan lands among them: they are
separated there by spacing the rungs out in luminance, with System pulled well clear
below the ladder rather than by making the tan warmer, which would only have walked it
into the orange.

**The toggle is `n`** (for *night*), listed on the help panel's controls and offered
there as an `[n]` footer button for touch. It is the one key the modal help panel
forwards rather than swallows, because the panel is where the option lives until v2
grows an options screen. Nothing persists it yet: a reload comes back dark.

### 4.5 The alert rung's colour

The facility alert ladder (§7.3) has three rungs and no way back down, and each maps to
a §4.1 category:

| Rung | Category | Why that one |
|---|---|---|
| **0** | System | Not a threat statement — an unnoticed raid claims nothing |
| **1** | Caution | *A threat that is unaware* — the facility knows somebody is in it and not where |
| **2** | Warning | *A threat that is hunting* — three sightings, or it knows what you came for |
| **3** | Danger | *A threat that has you* — the top of the ladder |

**It invents no colour vocabulary.** 1–3 are the standing threat ladder, the same
yellow → orange → red the player already reads off a guard's glyph, so the facility's
mind escalates in the colours one guard's does and there is nothing new to learn. It
inherits §4.1's luminance separation for free, so it survives a red-green deficiency,
and it inherits §4.4's second column, so it works on both themes. The mapping is
written once, in `render::alert`, and every surface that shows the rung reads it —
which is what stops a colour saying *danger* over a line saying *condition 1*.

**The `[?]` toggle used to wear this colour, and no longer does** (#375, reverted by
#420). The argument for tinting it was sound while it held: the near line could state a
step only on the turn it happened, anything louder overwrote it (§11.7), and the panel
behind the button was the only place the standing state could be read — so the control
changed colour to say there was something new there. Once the near line began carrying
the standing alert itself, in words and in the colour of its band, the tint became a
second and quieter statement of what the row directly beneath it already said: at the
top rung, a red `[?]` sitting on a red band. The `[?]` is furniture again — the one
System tan every HUD control wears — and the ladder's always-visible half is the row,
not the button. The panel's ALERT section still carries the effects, so nothing was
lost but the duplicate.

**Guard presentation is deliberately unchanged.** A guard the ladder has made never-calm
still draws as Calm: a guard's colour is *its own state* (§11.2), not the facility's
mood, and folding one into the other would break the "the colour of `g` is the AI state
machine" rule the whole scheme rests on.

*(**Rung** is the design and code word for the ladder's steps; the screen says
**condition** — see §11.8 of the design doc for the whole design↔player glossary. This
table is a developer reference, so it says rung.)*

---

## 5. Backgrounds

Backgrounds are the threat channel, and there is a fixed precedence:

**Danger > Effect mark on a thing > Sensed > Effect wash.**

| Background | Means |
|---|---|
| **Danger** (red) | This cell is watched by a guard **you can see** — or it lies on the **watcher line**, the sightline of a guard you cannot see that is watching you right now |
| **Effect** on a thing (cyan) | The guard here is held by one of your effects; the `@` here is a live decoy rather than you; the `@` here is you, hidden by Camouflage *this turn*; or the `@` here is you **inside a solid with Phase Out running** — the cell the safety eject would throw you out of if the window ended now |
| **Sensed** (orange) | A guard felt through a wall, or a door that just changed away from you |
| **Effect** wash (cyan) | Where your own gadget acted — a blast's box, a bored cell, the doorways a lockdown holds |

The effect layer appears twice on purpose (#338). Its **wash** is advisory geometry and
the weakest cue on the board. Its mark on a **thing** is not a competing claim about the
cell but a *refinement of the cue that thing already draws* — "exactly here" becomes
"exactly here, and it cannot move"; a second Owned `@` becomes "and that one is the
ability running"; your own `@` becomes "and right now they cannot see you" — so it sits
above the orange it refines and still below the red that outranks everything.

The marks on the player are the ones that **blink while their ability runs**, and both
are on the board for that reason alone. Camouflage (#341) conceals only on the turns you
stand still, so the mark goes dark the turn you move and returns the next still turn,
while the bar reads `Camo[n]` throughout. Phase Out (#416) marks you only while you are
somewhere a solid body cannot stand — nothing on open floor, lit inside a wall, dark
again when you step out — while the bar reads `Phase[n]` throughout. That is deliberate:
a mark is worth its ink only when it can disagree with the bar. Note what neither of them
says — *the ability is on*. The bar owns that, and a mark that repeated it would earn
nothing; what these add is the condition the bar has no room to carry.

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

### The watcher line (#222/#465)

That last rule leaves one hole, and the red channel closes it. A guard watching from
a room you cannot see into paints no cone, so without this the board would say you
are safe right up to the capture. Whenever a guard **detects you right now** and you
**cannot see it**, the straight line between it and you is painted `Danger` — not the
cone, just the line: where the threat is, and which way to run.

It is **standing**, not a flash. Drawn on every turn that guard still has you, gone
the turn it loses you, so it keeps answering *"it is still looking at you"* rather
than only *"something just saw you"*. Read it as *"it can see you right now"* and
never as *"it is after you"*: a chaser that has lost you draws nothing. Three cases
draw nothing at all — a guard you can **see** (its real cone paints already, and the
line must never double-draw one), a **confused** guard (blind, §8.3, so it has no
cone to be honest about), and a player **concealed** from it (§10.3, matching the
overlay's own spare — red under you means detected).

It is painted among the *weakest* backgrounds, below the marks and the sensed dot,
even though it is red: the line is a route, and the cues along it are about
particular cells. So a **sensed** watcher keeps its orange position dot with the red
line running up to it, and a watcher that is neither seen nor sensed is marked only
by the line's own far end. The danger overlay still paints last and still outranks
everything.

**What it costs, stated rather than discovered:** while an unseen guard is looking at
you, you get its exact position, through walls, at any distance, for free. That is a
deliberate exception to §9's bound on what may be known about a guard — the sense
range, the wait's widening, the duct's shrinking. §2.2/§2.3 buy it: you may not be
caught by something you could not perceive. The exception is bounded by the condition
it exists for, and expires with it.

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
