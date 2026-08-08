# Render reference — glyphs and palette

The complete table of what Intrusion draws and what colour it draws it in, with
the reasoning behind each choice.

This is a **reference, not a design document**. [`docs/design.md`](design.md) owns
the rules — §11.1 the grid, §11.2 colour, §11.3 glyphs, §11.5 field of view,
§11.5a fog — and this file records the concrete values those rules resolve to, in
one place, so a question like *"what does `□` mean?"* or *"why is the exit purple?"*
has an answer that is not spread across four sections and two source files. Where
the two disagree, the design doc wins and this file is stale.

**The values live in code, not here.** The glyph table is
[`Terrain::glyph`](../crates/core/src/facility.rs) plus the entity constants in
[`crates/core/src/render.rs`](../crates/core/src/render.rs); the palette is the one
table in [`crates/web/src/palette.rs`](../crates/web/src/palette.rs). The in-game glyph
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
| *(none)* | A guard you only sense through a wall | Sensed — a background highlight on its cell, no glyph, with a short fading trail behind it (§5). Never drawn at all under the §12.6 modifier that switches the sense off (#493) |
| `z` | A body | Caution — trouble waiting to be found |
| `z` | A body in your hands | Owned — yours, and in play |
| `z` | A body stowed in a cupboard | Neutral — the cupboard is spent |
| `*` | A drone you launched (§8.3/#273) | Owned; **Effect** background while you are flying it |

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

The drone is the second thing on the board wearing the "made by you" ink, and the
first that is a machine rather than a copy of you — hence a glyph of its own rather
than the decoy's borrowed `@`. Its **background** is what says who is driving: while
your input moves it, the cell carries the §11.5 effect mark, and the moment you hand
the controls back the mark goes dark while the `*` stays. That is the one thing the
§11.4 bar cannot say — the entry reads `Drone[23]` either way — and it is the one
thing a player can get wrong about the ability. It draws **under** a guard's `g` and
a body's `z` on purpose (a thing of yours never hides a threat), and its mark yields
to the danger overlay for the same reason, which is why the cue is a background and
not a glyph swap.

### 2.2 Terrain

| Glyph | Terrain | Category |
|---|---|---|
| `#` | Wall | Neutral |
| `·` | Floor **you can see** — floor beyond your sight draws blank | Ground |
| `+` | Door panel, closed | System; **Neutral** while a key gate holds it shut and you have no key (§10.4/#236) |
| *(blank)* | Door panel, open | Ground — the gap in the wall *is* its rendering |
| `×` | Door frame (hinge) | System |
| `}` | Cupboard | System; **Owned** while you are hiding in it |
| `π` | Table (partial cover) | System; **Owned** while it conceals you |
| `=` | Duct mouth | System; **memory slate** once scouted and out of view (§3) |
| `$` | Intel console | Interest; **Neutral** once spent |
| `Ψ` | Comms console | Interest; **Neutral** once used |
| `¤` | Equipment cache — salvaged tech (§2.2/§14 v3) | Interest; **Neutral** once emptied |
| `E` | The exit | Interest |

Floor draws as a **dot inside your field of view and as nothing outside it**
(§11.5 fix #2, sharpened by #470). The dot exists because a blank cell has no
foreground: the dimming that encodes the sight boundary was invisible across open
ground, and you could only see the edge of your vision where it happened to cross a
wall. Confining the dot to the FOV serves that same goal harder — the boundary is now
the edge between dotted ground and bare page, a hard line rather than a step between
two shades of dot. The dots stay deliberately the quietest thing on the board.

**This is not fix #2 being undone.** Fix #2's goal was *the sight boundary reading
across open ground*, and a board with no dots at all defeats it. The dots are still
what carries it; they have simply stopped being drawn where they were carrying
nothing. The full argument, and the consequences that fell out of it, are
[`docs/design-rulings.md`](design-rulings.md) appendix 33.

Two glyphs recolour rather than change shape when their meaning shifts — a spent
console (`$` Interest → Neutral) and a hiding place holding *you* (`}` System →
Owned). Shape is *what it is*; colour is *what it means to you now*. An emptied
equipment cache (`¤`) is the spent console's rule applied to a third glyph: the crate
is still standing there, and there is nothing left in it.

A **key-gated** door (§10.4/#236) is the spent console's rule applied to a doorway, and
in both directions. With the locked-room modifier on, the prize room's panels draw
Neutral rather than the working-furniture tan: to a player with no key they are a
door-shaped wall, which is exactly what a spent console is to a player who has already
taken its intel. The moment a takedown puts a key in hand every one of them goes back to
System, which is the one recolour on the board that runs *toward* usefulness — the price
just paid, made visible on the board rather than only in a message that has scrolled
away. Only the doors the fog already shows recolour; which room the building keeps locked
is something you learn by looking, or off the run's card.

**Why `¤` for the cache, and why not another console.** The three bump-to-use goals sit
in one category, so what tells them apart is shape alone — and the distinction is
worth drawing sharply, because *what a bump gets you* is the whole difference between
them: intel to spend, a radio net to kill, an ability to keep (§8.3). `¤` reads as a
crate rather than as a terminal, which is what a cache is, and it is a mark the board
uses nowhere else. It is deliberately **not** a letter: the board's letters are actors
(`g`, `z`) and the one thing the run is aiming at (`E`).

A facility may show **several** (§14 v3: a Vault hides three), all drawn the same. What
a particular crate holds is never on the board — the usable line (§11.4) is the only
thing that speaks about it, and only from the cell beside it: `cache: take tech`,
`cache: swap tech` for a run with no room for it (#266), `cache: recharge` for a duplicate
that refills a spent use budget (§8.2), or `cache: already yours` for the one refusal
left. That is the same bargain the exit strikes with its own refusal — you
learn what a bump *would* do by standing next to it, never by looking across the room.

While a **swap** is being decided the ability bar stops being the held set and becomes the
four candidates (§11.4/#266): your three pieces of tech, then the crate's — drawn in
Interest rather than Owned, the colour of the `¤` it is still sitting in, which is the
whole of how the row says which entry is the new one. It is the one row that draws its
**slot numbers** (`1 Camo`, the digit in the key colour the mnemonic mark wears): this row
is picked *from* rather than glanced at, and a candidate carries no clock, which is where
the width comes from. Pressing an entry drops it; the usable line carries the two answers
(`1-4: drop one`, `esc: decline`) and the near line keeps asking for as long as the offer
stands.

The **contents** rows — `}`, `=`, `$`, `Ψ`, `¤` — take the memory slate rather than the
dim shade once they are out of view, which is the §3 knowledge state and not a
category of their own. Only `=` names it in the table above, because it is the one
whose layer moved (#450) and the note is there to stop it drifting back.

### 2.3 The schematic

| Glyph | Means | Category |
|---|---|---|
| `□` | Building fabric you have never had eyes on | Neutral |

You always have the building's **plans** (§11.5a **[SETTLED]**: geometry is never
fogged, from turn one), but plans are not the same as having been there. The
schematic is what the plans give you: the **load-bearing fabric** — wall runs and
the recesses cut back into them — drawn, and everything that is **not** holding the
building up left blank. Walk in and it resolves into what is really there,
permanently (tile memory is monotonic).

The schematic is therefore **one shape and one absence**, which is what it can afford
to be now that floor beyond your sight draws blank too (§2.2): the plan carries its
whole message in the fabric channel, and the room shapes read as the negative space
inside it.

**Why `□`.** Fabric fills its cell the way `#` does, so a wall run on the plan reads
as *structure*; a baseline-hugging mark draws the same run as a dotted line, which is
the wrong reading for the load-bearing half of a plan. It carries roughly a third of
`#`'s ink, so the plan stays quieter than the building — the point of a schematic —
and an outline square *is* the claim being made: the shape of a wall, without the
substance of one you have seen. It is also unmistakable against `#`, `+`, `×` and `=`.

> **What `≈` was, and why it went** (#470). The old mark was the mathematical
> *approximately* — exactly the claim a plan makes about a stretch of building nobody
> has walked, and good reasoning. It was the wrong glyph in practice: at the cell sizes
> the board is fitted to, a double tilde reads as an **equals sign**, and `=` is the
> duct mouth (§2.2). So the mark for *unseen fabric* looked like a specific piece of
> terrain — and an unseen duct mouth is *itself* fabric, which put the confusion exactly
> where it cost most. `░` (light shade) was the considered alternative and was passed
> over: the shell scales cells by device pixel ratio, so a dither pattern is resampled
> at arbitrary fractional sizes and shimmers, where an outline stays clean. It remains
> the fallback if a heavier read is ever wanted.
>
> **Font coverage is a real constraint** for any replacement, not a formality: the
> artifact build ships one self-contained page, so a glyph the available fonts lack
> renders as tofu with nothing to fall back on. Whatever is chosen, look at it in a
> real build at several window sizes and device pixel ratios, in both themes.

**What is fabric and what is floor** is an architectural line, not a mechanical
one — it does not follow passability:

| Reads as `□` fabric | Drawn blank — floor space |
|---|---|
| Wall | Floor |
| Door **frame** | Door**way** |
| Cupboard alcove | Table |
| Duct mouth | Intel and comms consoles |

The test is *does it hold the building up*. A cupboard alcove and a duct mouth are
recesses cut back into a run, still backed by structure, so they belong to the run.
A **doorway** bears no load, so it draws as the **gap in the wall line** an
architectural plan would show — its frame stays `□`, so an unexplored wing reads
`□□□ □□□` and the ways between its rooms can be planned before you set foot in
them. What the doorway does *not* tell you is the panel's pose: a door's
open/closed state is live state and is never remembered.

A table stands *in* a room rather than holding it up, so it draws blank — and a table
blocks movement, which means the schematic can be optimistic about a route through
an unscouted room. That is deliberate: what you can plan is the building, and what
a room turns out to contain is what exploring is for.

**Everything unexplored collapses to exactly two appearances in exactly two
colours** — the fabric mark, or nothing at all. This is load-bearing, not tidiness: a
cupboard drawn as the one System-tan mark in a Neutral wall run would give away,
through the colour channel, precisely the alcove the glyph channel is hiding. Both
channels have to mask, or neither does. (A blank cell paints no ink, so the colour
channel has nothing to leak through there either.)

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

**The whole of this section is the layout knob's middle rung** (§12.6
`layout_knowledge`, #307/#233). The plans are what the baseline gives, and both ends
of the knob replace the table above rather than shading it:

| Setting | What a never-seen cell draws |
|---|---|
| `Full` (easier) | The **real building** — `#`, `+`, `=`, `π` and the rest, as if walked. Contents it is not entitled to (a console, a cupboard) stay masked by the geometry in their place |
| `Plans` (baseline) | The **schematic** — `□` for fabric, blank for floor space, as sorted above |
| `None` (harder) | **Nothing.** Blank, in Ground, for fabric and floor space alike |

The hard end (#233) is the *"everything unexplored collapses to exactly two
appearances"* rule with the count taken down to one: no mark and no ink, so neither
channel has anything to leak. It is a **[SETTLED]**-rule override and belongs to the
modifier alone (§11.5a); the exit keeps its `E` under it like it does under everything
else, which is the only reason the board is still readable at turn one.

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

- **There is no room at the bottom.** Ground was already the quietest row on the
  board. A step below the standard dim, on a true-black backdrop, is close to
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
> same seed and compared on a real 40×40 board: `≈` as it then shipped, and `▒` — a
> shade block, the architectural hatch for a wall in section.
>
> `▒` unquestionably read the *building* better. Corridors, room shapes and
> doorway gaps were legible across the whole unexplored region at a glance, where
> with `≈` they were closer to texture. The reason is worth knowing: in the explored
> picture wall-versus-floor is carried by **ink density** (`#` is dense, `·` is one
> dot) far more than by the colour gap between the Neutral and Ground dims, and two
> marks of similar density leave that colour gap working alone. `□` (#470) is the
> answer to that complaint from the other direction: it keeps the cell footprint that
> made `▒` legible without the fill that made it loud, and it no longer has a
> similarly-dense floor mark beside it to blur against.
>
> It was rejected anyway, because it **inverts the lighting**. A filled block puts
> down so much ink that unexplored territory becomes the loudest thing on the
> board — the explored room reads as a dark patch inside a bright mass — which is
> backwards for §11.5, where live is bright and the unknown recedes, and it puts a
> heavy fill in the register the danger overlay needs to own. A quieter plan that
> never competes with threat beat a legible one that does.
>
> The live option if this is revisited is neither of those but a third: `▒`'s density
> with a **darker shade of its own** for the schematic fabric, so structure reads
> without shouting. That spends a palette value, which the shape channel was chosen
> to avoid — so it is a real trade, not a free improvement. Whatever is tried, judge
> it on a screenshot of a full board and never on a unit test: the text frame looks
> correct in every one of these variants.

### 2.5 The campaign map (§14 v3, #208)

A **different screen**, drawn in the same grid — the facility graph between raids, not
the inside of a facility. Its glyphs are listed here because the whole point of one
table is that a glyph is not free to mean two things.

| Glyph | Means | Category |
|---|---|---|
| `@` | The facility you are standing on | Owned |
| `o` | An **Outpost** — thin, and thinly guarded | Neutral; **Interest** when marked |
| `▪` | A **Depot** — the ordinary facility | Neutral; **Interest** when marked |
| `$` | A **Vault** — worth robbing, and watched | Neutral; **Interest** when marked |
| `¤` | A **Workshop** — salvage, at intel's cost | Neutral; **Interest** when marked |
| `★` | The **archive** — the run's terminus *(and, in the score row, an earned star — below)* | Interest |
| `?` | An intel-locked route, not yet bought | Ground |
| `▫` | A facility on the map, not on offer | Ground |
| `·` | A road between two facilities | Ground |
| *(any of the above)* | A facility already raided | Ground |

**Four glyphs are borrowed, and each says the same thing it says on the board.** `@`
is you, so *you are here* needs no legend. `$` is the intel console, so the rich
facility reads as *the place with the loot in it*, and `¤` is the equipment cache, so a
Workshop is *the place with the crate in it* on the same terms — one glyph, one
meaning, across both screens. And a facility that has been raided
recolours to Ground exactly as a spent console does (§2.2's recolour rule): shape is
what it is, colour is what it means to you now.

`★` has **two readings on this screen**, and they are told apart by kind, by place and by
colour. As a **node in the picture** it is the archive — the one place a run is trying to
reach. As a **mark in the score row**, beside a named axis, it is a star the last raid
earned, with `☆` for one it missed (§4.6/#563):

| Glyph | Means | Category |
|---|---|---|
| `★` | *(a node in the picture)* the **archive** | Interest |
| `★` | *(in the score row)* an axis **earned** | **Owned** |
| `☆` | *(in the score row)* an axis **missed** | Ground |

**The colour is what completes the split.** The archive is Interest — the thing worth
reaching for — and an earned star is Owned, the player's own channel, because it is a
verdict on you and already yours. The picture still holds exactly **one** Interest star:
the terminus.

The pair was kept rather than swapped for a private mark, because `★` is the one glyph
every player already reads as *this is the good one* and a rating is exactly what that is
for — and because the score row never draws a bare `★★☆`: each mark stands beside the axis
it belongs to (`speed ★ · stealth ☆ · haul ☆`), which is both the ticket's own requirement
and what keeps the two readings apart at a glance. Appendix 61 records the call.

The end screen draws the same pair in the same Owned (§14 v2) and has no archive on it, so
there is nothing to tell apart there.

**Why `▫` and not the flavour.** The country's *shape* is public and its *contents* are
not — the §11.5a rule one scale up, which is why it reads without being taught.
There is no fog on the map (§14 v3): every facility is drawn, so a glance says how far
the archive is and how much room there is either side of your route. But a facility
says **what** it is when it is offered, and not a hop before — drawing every flavour
across the whole country would hand over for free what the scout sinks exist to sell
(#215). `▫` is the schematic's `□` one screen over, and deliberately so: it is making
the same claim about a facility that `□` makes about a wall — *something stands here,
and you have not been*. Most monospace fonts draw the two at much the same weight, and
nothing is lost by that, because **the two never share a screen**: `□` is only ever on
the board and `▫` only ever on the map.

**`▪` rather than `■` for a known facility.** The two squares are a matched pair at one
size, so *known* against *unknown* differs by **fill and nothing else** — the single
distinction the map is drawing. The large block was tried first and rejected on the
light theme for §2.4's reason, one screen over: it is the heaviest ink the grid can put
down, and it made an *unmarked* option pull the eye harder than the marked one, which
is the selection marker's job.

**The campaign alert is a line, not a glyph** (§14 v3/#210). What the last raid left on
the ground ahead is reported by a subtitle under the heading — `Condition 2 of 3 — Vault
alerted`, `Left unnoticed — Depot off guard` — rather than by a mark beside the row it
concerns. Two reasons, and the second is the load-bearing one:

- **There is no room.** The widest offer row (`Workshop — salvage, at intel's cost`,
  with its marker) already spends 38 of the board's 40 cells, so a second glyph on the
  row would have to be one cell wide with nothing either side of it.
- **A name is more legible than a mark.** No two open successors ever share a flavour
  (§14 v3 **[SETTLED]**), so *the Vault* picks out exactly one row — and the line has to
  carry the loudness itself (which condition, and whether it reached one road or all of
  them) regardless, which no per-row mark could.

Its colour is the **direction** it reports, and it is the same cue the help card gives
the same modifier (§12.6/#248): **Warning** for a rule bent against you, **Owned** for
one bent your way, **Ground** for a raid whose noise reached nothing on the list. That
is the one place on this screen where colour means *what a facility will play like*
rather than *where you stand in the list*, which is why it is on a line of its own and
never on a node: a node tinted by two unrelated meanings would be a colour you cannot
read.

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

**Floor is the exception, and the only one: it is drawn when Live and never
otherwise** (§2.2/#470). Out of your sight it draws blank in both middle states, so
explored and unexplored floor are the same absence. The distinction between them has
not gone anywhere — it moved entirely into the fabric channel, where explored geometry
reads `#`/`×`/`}` and unexplored reads `□`. The room shapes are what tell you where you
have been.

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
a plan would show, and a table still draws as blank floor space, which is §2.3–§2.4's
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
| **Sensed** | Orange | `#e69f00` | Felt through a wall — **background only**, two strengths (fresh / fading) |
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
  background half measures what the screen actually paints, so an out-of-view `Effect`
  fill (full strength, never fogged) is compared against an out-of-view `Danger` fill
  (dimmed) — the pair a player really has side by side. Both `Sensed` shades are held
  to the same bar against red, since a fading trail mark is as much a sensed cell as a
  live dot.
- **The threat ladder is separated by luminance as well as hue**, so
  yellow → orange → red survives a red-green deficiency — and so does the band of a
  near line carrying the alert condition, which is what the fills are for.
- **A cell you can see and a cell beyond it are tellable apart**, for every category
  that paints a fill — the general form of §11.5's *watched must never look safe*,
  which had only ever been checked for the danger overlay. `Effect` is outside it: it
  has no second shade by design (§5). `Sensed` has two, but they are not this claim —
  they are *fresh* and *fading* (§5), and they are asserted tellable apart on their
  own terms.
- **The near line's words read over every band.** The row is a solid category band
  with its text in Neutral (§11.4), so how far the backgrounds may be lifted off the
  page is bounded by the ink that has to stay legible on them.
- **Ground recedes beneath every other category** — the floor dots must whisper.
  (This bullet used to carry a second claim, that Ground's live and dim shades stay far
  enough apart for the sight boundary to read across open floor. #470 retired it with
  its mechanism: floor out of the FOV draws blank, so no Ground glyph is ever painted
  dimmed, and the boundary is ink against bare page instead of one shade against
  another. See appendix 33.)
- **The memory slate stands apart from every live colour** and from the dim gray —
  memory that could be mistaken for a live glyph would defeat the whole three-state
  scheme.
- **The dim exit still reads as purple**, well clear of both wall gray and the memory
  slate.

### 4.2 Two orange categories, and why they never collide

**Warning** and **Sensed** share a hue. Warning is a glyph — a hunting guard you can
see is an orange `g` — and Sensed only ever a *background*: a guard you can only feel
through a wall is an orange filled cell with no glyph at all. The bloom from one to the
other, the moment you round the corner, is the seen/sensed distinction made visible.

Since #224 Warning also paints one background: the **investigation area** a §7.6 search
is sweeping, under the `show_search_areas` modifier. That is not the collision the rule
above was guarding against — it is the *same* claim in two places. The guard hunting is
orange, and the ground it is hunting over is orange; a searcher standing inside its own
area reads as one thing rather than two. The two categories being the same palette row
also means their precedence against each other is invisible on screen, which is why §11.5
settles the ordering on the comparison that does show: red wins.

Sensed uses **both** of its row's background shades, and they mean something no other
row's pair means: the bright fill is a mark made **this turn**, the quiet one the
fading tail behind it (§9.5). Everything sensed is out of the field of view by
construction, so the fog has nothing to say about the channel and freshness is the
only thing a strength here could honestly carry.

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
  (Since #470 the board draws no floor at all outside your sight, so like `Effect`
  below this row's dim shade is a value the table keeps rather than board ink.)
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

**The toggle is `n`** (for *night*), listed on the help panel's controls and forwarded
rather than swallowed by the open panel — its colour key is the best thing on screen to
judge the flip against. Its **home is that panel's Options tab** (§14 v2/#513): a `theme`
row drawing the live value, and the record that persists it, so a reload comes back in
the theme that was chosen. The panel's own `[n]` footer button went with the setting; the
campaign map keeps a drawn `theme [n]`, since it has no route to the tab yet and a touch
player would otherwise be left with no control at all.

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

**Danger > Effect mark on a thing > Sensed (live dot) > watcher line > Sensed (fading
mark) > investigation area > Effect wash.**

| Background | Means |
|---|---|
| **Danger** (red) | This cell is watched by a guard **you can see** — or it lies on the **watcher line**, the sightline of a guard you cannot see that is watching you right now |
| **Effect** on a thing (cyan) | The guard here is held by one of your effects; the `@` here is a live decoy rather than you; the `π` here is **a piece of Cover you deployed** rather than furniture the building came with (§8.3/#562), and it rides the piece as you push it; the `@` here is you, hidden by Camouflage *this turn*; or the `@` here is you **inside a solid with Phase Out running** — the cell the safety eject would throw you out of if the window ended now |
| **Sensed**, full (orange) | A guard felt through a wall right now — its exact cell, position only |
| **Sensed**, quiet (orange) | Where the sense felt something a turn or two ago: the trail behind a moving guard, the ghost of one that left the box, a door that changed away from you (§9.5). It fades to nothing over a couple of turns — *was just here*, never a heading |

> **The `Sensed` channel is suppressible** (§9/§12.6/#493). With the *"nothing felt
> through walls"* modifier on, **neither** `Sensed` strength is ever painted: no guard
> dot, no trail, no ghost, no door cue, for the whole run. It is the only category here a
> level modifier can silence outright, and the board reads correspondingly quiet — which
> is why the Level info card names it before turn one, so a player does not read the
> absence as a broken render. Nothing else moves: `Danger` still paints the cones of the
> guards you can see *and* the watcher line of one you cannot (§11.5/#465), which is the
> fairness floor this modifier is bounded by (§2.2) rather than part of the channel it
> switches off.
| **Warning** (orange) | The area a §7.6 search is sweeping — the `SEARCH_RADIUS` box around a searching guard's focus. Only with the `show_search_areas` modifier on (§12.6); baseline the board draws no investigation area at all |
| **Effect** wash (cyan) | Where your own gadget acted — a blast's box, a bored cell, the doorways a lockdown holds, the reach a False Call broadcast over, **the line a Dart flew** (§8.3/#239), **the disc a Repel is holding** (§8.3/#554) — **or**, uniquely, where one is *pointing*: the Guide's bearing (§8.3/#505), one cell of the eight around you, **on one turn in three** |

**The Guide's cell is the one Effect cue that is not about a thing** (#505), and it is
worth flagging because cyan is already busy: it marks a held guard, a live decoy, your
own camouflage, a Phase Out eject cell and a gadget's reach — all of them *something
happening*. A wash one step from the player is a new kind of user of that colour, and it
sits exactly where the eye lives. It **pulses** rather than stands (one turn in three),
which is a balance decision first — a standing needle is a line you follow without
thinking — but it earns its keep here too: the colour is not permanently spoken for. It is at the **bottom** of the stack for that reason and not by accident: a
convenience must never sit on top of the thing that can kill you, so every threat
channel above paints straight over it. If it reads as *"an ability is running on that
cell"* rather than as *"that way"*, that is the thing to fix, and the fix is not to
promote it.

The **investigation area** (#224) is the second advisory layer, and it is advisory in
exactly the sense the wash is: orange says *a guard's attention is on this ground*, never
*you are detected*, which stays red's word alone. It is the literal box a hideout inside
the sweep is flushed from, so the picture and the rule are one set; it clears the turn the
search does, with no fade, because it makes no claim about the past. It is drawn for
**every** live search, seen guard or not — the "never a guess" contract binds the
detection set, and an area gated on perception would go dark exactly in the cupboard where
it is worth most.

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

**One thing can make the red mean something else, and it is not reachable from play.**
Under §12.6's **ghost** debug switch (#507) nothing detects the player, so the literal
detection set is empty and the overlay would go blank — deleting it exactly when someone
is debugging vision. It keeps painting the set that *would* detect a detectable player
instead: red reads as *this cell is watched*. A red cell under a ghost is the instrument
working, not the overlay broken. No link, token or sim run can turn that switch on.

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

It is painted among the *weakest* backgrounds, below the marks and the live sensed
dot, even though it is red: the line is a route, and the cues along it are about
particular cells. So a **sensed** watcher keeps its orange position dot with the red
line running up to it, and a watcher that is neither seen nor sensed is marked only
by the line's own far end. It does outrank the sense channel's **fading** marks: a
trace of where something was a turn ago must never cover a line that says a guard has
you now. The danger overlay still paints last and still outranks everything.

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

**The Repel field is the one wash that is itself a rule** (§8.3/#554), and it is worth
saying plainly because every other cue in this column *reports* something. A blast's box says
where a thing happened; a Dart's line says where a thing went. This one is the wall: the cells
drawn cyan are exactly the cells no guard may step into, read from the same [`EffectArea`] the
movement pass enforces, so there is no second derivation that could disagree. It is
**standing**, not a flash — it is up for the whole window and gone the frame it ends — and it
is the reason the precedence below is not a detail here but the whole of the cue's honesty:
this wash sits over ground guards are walking *beside*, so it and the danger overlay share
cells constantly. **Danger wins every one of them.** A wall drawn over the thing that can kill
you would read as *safe here* on the cell you are about to be taken on, which is the one thing
§11.5's **[SETTLED]** ordering exists to forbid — and it is a real risk for this ability
rather than a theoretical one, because the field does not conceal you and a guard with a line
into it goes on watching you the whole time.

**The Dart's wash is a line rather than an area** (§8.3/#239), and it is the only one: the ray it flew, cell by cell, for the firing frame alone. It answers the question a projectile raises that a radius does not — *how far did it get* — since a dart that stopped short stopped **on** something, and where it stopped is the whole report. It is safe to paint through the fog for the same reason the boxes are (your own gadget's reach is your own knowledge), plus one specific to a line: the flight is clamped inside the guard sense, so a short wash can only ever end on a cell already drawn (appendix 54).

**Sensed and Effect are not fogged.** Both are certain, position-only knowledge that
travels through walls, so neither dims with the knowledge state of the cell
underneath. Fogging an effect's footprint would teach you its extent only where you
were already looking, which is exactly the corner the flash exists to light — so
`Effect` paints one strength everywhere. `Sensed` is not fogged either; its two
strengths are spent on **age** instead (§4.2).

### The near line's band stops at its controls (#502)

The near line is the one HUD row that paints a background (§11.4), and its two
controls sit on it. Both wear the single static System tan every HUD control wears
(#420, §4.5), and against a quiet tint of the facility's standing mood that tan does
not separate — so the `[?]` and the deploy control, the two things on the row that
must *always* be legible, were the least legible things on it. The `[?]` is the fixed
landmark a lost player reaches for and the deploy control is the only way to the rest
of the messages; a row whose words read and whose controls do not has it backwards.

So **the band is not painted under either control**. Those cells carry the screen's own
background — black in the dark theme, paper in the light one — and the System tan is
read against exactly the backdrop every other control on the screen is read against.
The core says *no band here* and the shell paints whichever of its two columns is live
(§4.4/#189); nothing in `crates/core` names a colour. The band still runs edge to edge
everywhere else, including the cells the message does not fill, and it still paints
`Fill::Quiet` when ambient and `Fill::Full` for a message.

**The held-back span is the control's own cells — three each, and the band meets the
button edge to edge.** Two widths were built and compared side by side, one artifact
each: the control's three cells, and those three plus the blank cell between the
control and the words. The wider one gives the button a hairline of background all
round and reads as a chip lifted off the band; the narrower one keeps the band a
continuous run and reads as a band the control is set into. **Three won**: what made
the control unreadable was the tan sitting *on* the tint, and lifting the tan off it is
the whole of the fix — the extra cell buys a little more separation at the price of a
notch in a row whose job is to be one unbroken flash of colour across the top of the
screen. Neither width costs anything: the air cells are already outside the message's
budget (§11.4's `NEAR_LINE_CONTROL_CELLS`), so this is **paint, not layout** and the
row's 32-glyph capacity is identical either way. The comparison is a one-constant flip
(`HELD_BACK_AIR`) if it is ever worth re-running.

The span is derived from the same `NearLineControls` layout the drawing and both
hit-tests are read off (§11.4 **[SETTLED]**) — a `[?]` whose held-back cells and whose
hit-test disagree is the same class of bug as one whose band and words disagree.

Two non-fixes, both already tried: **do not dim the band** and **do not re-tint the
controls per alert rung** (§4.5 — that one was reverted in #420). Each trades one
legibility problem for another.

No other row has this problem to fix: the usable line, the alert row and the help
panel's own `[x]` and `copy [c]` draw no band at all, so their controls already sit on
the page background.

---

## 6. Tiles

An **optional second renderer** (§11.1 **[SETTLED]**, #460), off by default and turned
on with `?tiles=1`. The character grid stays the game's real picture; a tile is a
second way of painting the same grid, and the fact that it changes nothing about what
the grid *says* is the whole point of the seam.

**The rule is one sprite per glyph, and the sprite may be turned.** A sprite is chosen
by the cell's glyph, by which of its neighbours draw that same glyph (§6.2), and by the
facing the cell declares (§6.3) — and by nothing else. No animation, and nothing read
from game state. The colour is not chosen by the tileset either: the cell resolves to a
colour through §4 exactly as a character would, and the sprite is drawn *in that
colour*. So a tile and a glyph carry identical information, in all four knowledge
states (§3) and both themes (§4.4) — with the single, deliberate exception of facing
(§6.3) — and the two renderers can never disagree about what a cell means.

Sprites are therefore authored **greyscale with alpha**: the alpha is the shape, and
the greys are shading that the category tint multiplies through. Full-colour art is
not an option and never will be while §11.2 stands — a guard's yellow → orange → red
*is* the AI state machine made visible (§2.1), so a sprite carrying its own palette
would leave the threat ladder nowhere to live.

**What tiles do not touch:**

- **Backgrounds.** The §5 danger overlay, the sensed cue and the effect wash are the
  same `fill_rect`s in the same order, painted before the glyph layer. The board's
  most important read is outside the tile mode entirely.
- **The cell.** A sprite is squashed into the same 14×20 box a character is drawn in,
  so the fit and every hit test are untouched. Sprites are authored square, so they
  come out about 30% narrower than drawn — the price of not moving the grid. Square
  *cells* would need the map to carry its own metric while the §11.4 HUD rows kept the
  text one, and that is a much larger change.
- **Text.** A tile is drawn only where the glyph *is* the world — on cells the core
  tags `Surface::Board`. Status lines, the ability bar, panels, the deployed log and
  the verdict card are `Surface::Chrome` and always draw characters. That distinction
  is per **cell**, not per row, because the log and the verdict lay prose *across the
  map rows*: asked by row, every `g` in "a guard has seen you" would sprout a guard.

**An unmapped glyph draws as its character.** The table is allowed to be incomplete —
so is a sheet that has not finished decoding — and neither is an error. A glyph nobody
has drawn yet must render as itself, never as a hole.

### 6.1 The sheet

`web/assets/tiles.png` — a **16×16 grid of 48×48 slots**, most of them empty. It is
embedded in the wasm and handed to the browser as a `data:` URI, because the artifact
build packs one self-contained page under a CSP that blocks every external request,
and embedding for the Pages deploy too keeps it to one code path.

| | |
|---|---|
| Format | PNG, RGBA, 16 cells per row, each cell 48×48 |
| Slot | `row × 16 + col` |
| Alpha | The shape |
| Greys | Shading, multiplied through by the category tint |

**A slot number is permanent**, for the same reason an `AbilityId` slot is: art is
drawn against a number, so moving one silently repaints every cell that referenced
it. The headroom is what makes that affordable — claim the next free slot, never close
a gap. The bands, each with room left after it:

| Slots | Band |
|---|---|
| 0–15 | One sprite per **glyph** — what this section describes |
| 16–31 | The **wall autotile** run, keyed by a neighbour bitmask (`N=1, E=2, S=4, W=8`) — §6.2 |
| 32+ | Free |

**`web/assets/tiles.txt` names every allocated slot** — index, key, description — and
is the file an artist reads while drawing. It is not documentation that can rot:
`crates/web/src/tiles.rs` embeds it and a test asserts its own glyph → slot mapping
agrees with it in both directions, so the sheet, the table and the code cannot drift
apart in silence. The same test asserts that every shape the autotiler draws is
declared, and that no slot the table calls a **rotation** is ever indexed.

A `glyph:` or `wall:` slot listed in the table with nothing drawn in it is a slot
waiting for art, not a bug: the renderer falls back to the character. A `rotation:`
slot is empty *on purpose* — see §6.2.

Tinting bakes one copy of the sheet per colour rather than compositing per cell — a
40×40 board every frame is what would make this expensive — and only over the rows the
mapping actually reaches, so the empty headroom costs no canvas. The colour set is
closed and small (§4's categories × the knowledge states × the two themes, with most
rows sharing the one dim shade), so the cache is bounded by construction.

The sheet was **seeded** by `scripts/seed-tileset.py` from the source art in
`web/assets/source/` (which has its own README, including the autotile legend that
art's own run follows) and is **authored by hand from there on** — the script refuses
to overwrite it without `--force`, so a reflexive re-run cannot discard drawing.

The art is still **placeholder** and says so: the source sheets came out of an earlier
Godot experiment, and only two of the fourteen glyph sprites could honestly be cut from
them — the wall and the floor. The rest are crude generated shapes, including both
actors: the player sheet's hooded figure is drawn from above but carries no cue for
which way it is looking, and a renderer that turns sprites cannot use a facing nobody
can name (§6.3).

### 6.2 Autotiling: a surface is drawn from its neighbours

A glyph that reads as a **continuous surface** picks its sprite from which of its four
neighbours draw that same glyph — so a wall run joins along its length, turns at a
corner and closes at a crossing, instead of being the same block repeated. Only `#`
autotiles today, because only `#` has a run drawn for it; `□` is exactly as continuous
and costs a band and nothing else the day somebody draws it.

The neighbourhood is a **bitmask** — `N=1, E=2, S=4, W=8` — and the slot is `16 + mask`,
so the lookup is arithmetic and no table has to be kept in step with anything.

**The neighbours are read from the drawn grid, and from nothing else.** This is the
rule the whole feature rests on. Geometry the player has never seen is masked as the
schematic's `□` (§2.3/§11.5a), not as `#`, so a wall *cannot* join to it: joins follow
the glyph that is drawn, which means the shape channel can say no more than the glyph
channel already said. Ask the game state instead — "is there really a wall there?" —
and the masking is defeated through shape while glyph and colour are still telling the
truth, which is the §2.4 leak in its most invisible form, because it looks like better
art. Two other neighbours never join, for the same reason: one off the grid, and one on
the **chrome** surface, so a `#` in a sentence laid across the map (§11.7) is never
welded to the facility.

It follows that **the joins change as the fog lifts** — a wall that ended in a cap
grows into a run as the player sees what it continues into. That is correct, and it is
the visible form of the guarantee: what is drawn is what is known.

**Six shapes, sixteen neighbourhoods.** The masks fall into six rotation orbits, and
the shell turns a sprite at draw time, so the sheet stores each shape **once**:

| Slot | Mask | Shape |
|---|---|---|
| 16 | none | an isolated block, exposed on all four sides |
| 17 | N | an end cap |
| 19 | N–E | a corner |
| 21 | N–S | a straight run |
| 23 | N–E–S | a T |
| 31 | all four | a crossing — and the plain interior of a mass of wall |

The band's **other ten slots stay allocated and empty**, listed in `tiles.txt` as
`rotation:` with the slot and quarter-turn that reach them. They are not free space:
the band is indexed `16 + mask`, so closing the gap would slide every slot after it and
silently repaint whatever referenced them — the reason an `AbilityId` slot is permanent
(`CLAUDE.md`). Drawing one is still allowed; it is what a sheet does when a corner
deserves art its rotation cannot give it.

*(The band could in principle be deduplicated further — every tile is the plain fill
plus one boundary line per exposed side, so two images and four draws per cell would
do. It stores six because a cell must stay **one** draw: a 40×40 board every frame is
what makes anything here expensive. The measurements behind both the deduplication and
the source run's corrected legend are in `docs/design-rulings.md` appendix 37.)*

### 6.3 Facing: the one thing tiles say that characters do not

A cell may declare a **facing**, and its sprite is drawn turned to it. `GlyphCell`
carries it, so the grid is still the single interface and the ASCII renderer is
unchanged — it ignores the field, and the character picture is identical byte for byte.

**Who has one:**

| | Facing |
|---|---|
| The player | Yes — §5 makes "you cannot see behind you" a rule, so which way you are turned is worth drawing |
| A **seen** guard (§9.2) | Yes — the same facing its cone is drawn from, said a second way |
| A **sensed** guard (§9.2) | **No.** Position only, "no facing, no cone" — and it has no glyph to turn in the first place |
| Your decoy (§8.3) | Yes, and it is **yours**: the fake wears your glyph, your colour and your stance, so tiles cannot tell you from it when characters cannot |
| A player inside a hideout | No — the glyph is the cupboard, and a cupboard faces nowhere |
| Everything else | No |

**This is an addition of information, and it is the only one.** A character grid says
nothing about which way anything is turned; a tile does. That is deliberate for the
player and defensible for a seen guard (whose facing the danger overlay already gives),
and it is exactly why a sensed guard must not have one — the sense channel is *defined*
as position without state, and a turned sprite behind a wall would hand over the one
thing it withholds.

A directional sprite is drawn at a **rest facing of south** — down the screen, toward
the viewer, which is how top-down art is drawn — and turned from there, so the
commonest case costs no rotation. Such a sprite must be authored **square and
aspect-neutral**: a placeholder drawn in the cell's own 14×20 proportion would have its
squash land on the wrong axis the moment it was turned.
