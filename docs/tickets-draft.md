# Tickets draft — v1 (quick play)

Generated from `docs/design.md` §14 "v1 — quick play, and nothing else".

**State of the repo:** the workspace scaffold + CI gate is done (#1, closed). Only
the seeded `Rng` exists in `core`. Everything below is unstarted.

**Scope:** this is the full v1 "Included" list (§14) sliced into single-PR units,
in dependency order (§10.5 spatial model → generation → vision → guards → sound →
abilities → render). The one question v1 exists to answer: *is the hiding game fun?*
(§7.6). Nothing here builds scaffolding around that question — no saves, options,
campaign, intel-as-currency, tiles, touch, or audio (those are v2/v3).

**Milestone note:** GitHub milestones aren't creatable via the API tooling in use
(see #1). Milestone is recorded in each body as **Milestone: v1** and via the `v1`
convention; attach the real milestone in the UI if/when it exists. All labels below
are applied on creation.

**Suggested creation order:** the tickets are numbered A1…H2 in dependency order.
The foundation (A + B) unblocks everything; guards/abilities/render are blocked
behind a level that generates, renders, and takes a turn. Create A+B first if you'd
rather land the frontier before committing the whole backlog.

---

## v1 — A. Core spatial foundation

### A1 — Grid, terrain, and occupancy model → #4
**Labels:** `area:core` `type:feature` `size:M`
**Milestone:** v1

## Summary
The cell grid every other system reads and writes: a square grid of integer cells,
a terrain enum, and the occupancy/fill rule that makes "bump = interact" possible.
This is the substrate — no generation, no entities yet, just the world's physics.

## Design reference
§4.1 — the grid. Square grid, integer cells; **4-directional movement, no
diagonals [SETTLED]**; Manhattan distances (except sight, §6.1); facility fully
enclosed by an indestructible 1-cell border.
§4.3 — occupancy. Every cell capacity **1.0**; every object declares a fill 0.0–1.0;
a move succeeds if existing fills + mover's ≤ 1.0. Fill 1.0: walls, closed panels,
hinges, guards, player, bodies, consoles. Fill 0.0: open panels, empty hideouts,
decoys. **A blocked move is an interaction, not a failure** — the "bump" is the
whole interaction verb **[SETTLED]**.
§10.3 — terrain table (glyphs / blocks move / blocks sight / blocks pathing /
blocks sound). Encode all six columns per terrain kind; vision blocked when summed
opacity reaches 1.0.

## Acceptance criteria
- [ ] `Grid` type with integer dimensions and a cell→terrain store.
- [ ] `Terrain` enum covering every row of the §10.3 table, each carrying its
      move/sight/path/sound-blocking and fill properties as data.
- [ ] Occupancy query: "can X (with fill f) enter cell c?" summing current fills.
- [ ] 4-directional neighbour + Manhattan distance helpers; no diagonal movement
      path anywhere.
- [ ] Constructor guarantees the enclosing border ring unconditionally (§10.6).
- [ ] Unit tests: border is solid; fill sums gate entry; opacity sum ≥ 1.0 blocks
      sight; movement helpers reject diagonals.
- [ ] fmt/clippy/test gate passes.

## Notes / risks
- Keep this pure data + queries; no turn logic, no entities. Those are A2 and later.
- The fill model must express "open door = 0.0, closed = 1.0" and "empty hideout =
  0.0, occupied = 1.0" cleanly — it's the seam the door/hideout tickets plug into.

---

### A2 — The turn loop: three phases, turn cost, win/lose → #5
**Labels:** `area:core` `type:feature` `size:M`
**Milestone:** v1

## Summary
The heartbeat: `state × input → state, events`. Resolves one turn in the fixed
three-phase order, enforces the turn-cost rule that §2.3 identifies as the thing
the old game got fatally wrong (a free ability), and implements the only win/lose
conditions.

## Design reference
§4.2 — the turn: (1) Player acts; turn doesn't advance until an action explicitly
ends it; (2) Sight recomputed for every viewer from current position+facing;
(3) Guards read sight, decide, act. **One full turn runs at level start.**
§4.4 — **every action that changes the world costs the turn.** Exceptions, and
there should be *very few*: moving into a wall = **free**; toggling an ability off
= **free**. This is the load-bearing rule (§2.3).
§4.5 — **Lose: a guard moving into your cell captures you** — the only loss
condition; no health/combat/damage **[SETTLED]**. Being seen is not losing. **Win:
satisfy objectives, then return to entry point;** bumping the exit early refuses
with a message. Capture is *contact* not detection — a guard patrolling into your
cell catches you even if it can't see you.
§12.1 — core is pure and deterministic; the loop returns `(state, events)`.

## Acceptance criteria
- [ ] A `step(state, input) -> (state, Vec<Event>)` entry point that resolves the
      three phases in order, with the sight phase as a hook (real FOV lands in C1).
- [ ] Turn does not advance on a free/failed input (wall bump); does advance on any
      world-changing action.
- [ ] Loss fires when a guard enters the player's cell; win fires only when all
      objectives are satisfied *and* the player is on the entry cell; early exit is
      refused with a message event.
- [ ] The startup full-turn runs before first player input.
- [ ] Deterministic: same seed + same input sequence → identical event stream
      (golden-style test).
- [ ] fmt/clippy/test gate passes.

## Notes / risks
- Guard/vision phases are stubs here and filled by C1/§7 tickets — keep the phase
  boundaries clean so those slot in without reshaping the loop.
- Pick and document the duration-tick convention now (§8.2's "N yields N−1" trap)
  even though abilities land later — the loop owns where the tick happens.

---

### A3 — The spatial region graph → #6
**Labels:** `area:core` `type:feature` `size:M`
**Milestone:** v1

## Summary
The single highest-leverage structural decision in the document (§10.5): give the
level a real spatial model — named regions of arbitrary shape (rooms *and*
corridors), explicit door edges between them, and a cell→region lookup. The old
version threw the generation graph away and everything downstream had to fake it;
keep the graph.

## Design reference
§10.5 — **[SETTLED]: keep the graph.** Corridors are nodes, rooms are nodes, doors
are edges. Provide: regions of arbitrary shape (not just rectangles — a room with a
pillar / an L-nook isn't a rectangle); **corridors are first-class regions** (the
old model couldn't address them at all); explicit door edges; a cell→region lookup.
"Nearly every 'guards should…' idea depends on it."
§7.5 — the patrol-territory weakness ("cover the east wing" is unsayable) is
downstream of this; the graph is what makes assigned coverage possible later.

## Acceptance criteria
- [ ] A `RegionGraph` (or equivalent) with `Region` nodes tagged room|corridor,
      each holding its cell set (arbitrary shape).
- [ ] Door edges connecting exactly two regions.
- [ ] O(1)-ish `cell -> Option<RegionId>` lookup.
- [ ] Query helpers: region of a cell, neighbouring regions across a door, the
      region kind.
- [ ] Built by the generator (A/B tickets populate it) but usable standalone with a
      hand-built fixture; tested against a small fixed level.
- [ ] fmt/clippy/test gate passes.

## Notes / risks
- This ticket defines the *types and lookups*; the generator (B-series) fills them.
  Land the shape here so generation writes into it directly rather than being
  retrofitted.
- Don't reintroduce the rectangle-only assumption anywhere — corridors and
  pillared rooms are the explicit reason it existed and failed.

---

## v1 — B. Generation (§10)

### B1 — Corridor-first binary partition → #7
**Labels:** `area:generation` `type:feature` `size:M`
**Milestone:** v1

## Summary
The primary structure: recursively carve corridors through the largest region,
splitting it, until neither axis fits; surviving regions become rooms. Corridors
are deliberate spaces, not plumbing. Populates the region graph (A3).

## Design reference
§10.1 steps 1–3. Start with interior `(W-2)×(H-2)`. Repeatedly carve through the
largest region: axis must fit `6+1+2+1+6 = 16`; **corridor width random 2–4, never
single-file [SETTLED]**; split so both leftovers ≥ 6 deep; 50/50 axis when both fit,
stop when neither; stamp **wall / corridor / wall** full span; **punch one cell
beyond each end** (the connectivity mechanism); replace region with its two
leftovers, re-sort by area. Every surviving region ≥ 6×6 becomes a room.
§10.2 — room count emerges, bounded ~12; below 18×18 no partition is possible.

## Acceptance criteria
- [ ] Seeded partition producing corridors + rooms for the v1 40×40 config.
- [ ] Corridor width always ∈ [2,4]; no single-file corridor ever.
- [ ] Every carve punches through one cell past each end (connectivity).
- [ ] Corridors and rooms are recorded as regions in the A3 graph (not painted and
      forgotten).
- [ ] Property test: over many seeds the corridor network is connected (tree
      property from the punch-throughs).
- [ ] fmt/clippy/test gate passes.

## Notes / risks
- Guard the minimum size (§10.2): a sub-18×18 or single-room result must be handled,
  not silently produce an unplaceable level.
- Determinism: all randomness from the run `Rng` (§12.4) — no fresh sources.

---

### B2 — Doorways and the door model → #8
**Labels:** `area:generation` `type:feature` `size:M`
**Milestone:** v1

## Summary
Cut doorways where rooms meet corridors, and implement the door object — a hinged,
multi-panel span you open by bumping a panel and close by bumping a hinge. Doors are
sight, sound, and connectivity all at once.

## Design reference
§10.1 step 4 — scan rows then columns; a run of wall with interior on both flanks
is a candidate; each **maximal** run of length ≥ 3 gets **exactly one** doorway,
length random 3..min(run,6) at a random offset. Every door connects a room to a
corridor (never room-to-room).
§10.4 — a door is 3–6 cells: a **hinge at each end** (permanently solid+opaque) and
**1–4 panels** that open/close as one unit. **Bump panel to open, bump hinge to
close.** Anyone operates any door (no keys, no locks) **[START]**. A door can't
close if anything occupies a panel. **Closed panels don't block pathfinding** —
guards route through and open them. **Auto-close: doors close behind their user
[START]** (restores structure, makes sound meaningful, an open door = evidence).
Door edges register in the region graph (A3).

## Acceptance criteria
- [ ] Doorway placement per §10.1.4 (maximal runs ≥ 3, one door each, correct
      length/offset), producing door edges in the graph.
- [ ] Door object with hinge/panel structure; bump-panel opens, bump-hinge closes,
      as a single unit.
- [ ] Closed panel: fill 1.0, opaque, attenuates sound, but **transparent to
      pathfinding**. Open panel: fill 0.0, walk-through.
- [ ] A door refuses to close while any panel cell is occupied.
- [ ] Auto-close behind a user (tunable/toggleable so playtest can compare).
- [ ] Tests: open/close via bump; can't close on an occupant; pathing ignores
      closed panels.
- [ ] fmt/clippy/test gate passes.

## Notes / risks
- Auto-close interacts with sound (E1) and with "guard traffic opens the facility
  up" — keep the behaviour data-driven so it's tunable, per §10.4 [START].

---

### B3 — Room features: partition walls and pillars → #9
**Labels:** `area:generation` `type:feature` `size:S`
**Milestone:** v1

## Summary
Break up room interiors with partition stubs and freestanding pillars, creating
alcoves, dead ends, and sight-line breaks — and, critically, the 2-cell-thick
geometry that hideouts (B4) need.

## Design reference
§10.1 step 5 — up to **4** attempts per room; each proposes a partition wall and a
pillar, viable ones pooled, one picked. **Partition wall** (needs ≥ 3×3): 1-cell
stub from a wall, length random 2..(axis−1), orientation weighted by the room's
perpendicular extent (tall rooms get horizontal stubs), rejected unless footprint
grown by 1 is clear. **Pillar** (needs ≥ 6×6): freestanding 2–4 by 2–4, rejected
unless footprint grown by 1 is clear. **Pillars must come before hideouts.**

## Acceptance criteria
- [ ] Up to 4 feature attempts per room, proposing+pooling+picking per §10.1.5.
- [ ] Partition walls: size/orientation/rejection rules honoured.
- [ ] Pillars: size/clearance rules honoured; run before hideout placement.
- [ ] Region cell sets (A3) updated so a pillared room's region is its true
      (non-rectangular) shape.
- [ ] Tests: features never overlap their grown footprint; pillars only in ≥6×6.
- [ ] fmt/clippy/test gate passes.

## Notes / risks
- The non-rectangular region shape is the payoff — make sure the graph reflects it,
  or A3's whole point is lost.

---

### B4 — Hideouts (rooms and corridors) → #10
**Labels:** `area:generation` `type:feature` `size:S`
**Milestone:** v1

## Summary
Carve the alcoves the hiding game happens in — and place them along corridors and
junctions, not just rooms, because §7.6 says the flight path is exactly where you
need to hide. A flight path with no hideout on it is a failed flight path.

## Design reference
§10.1 step 6 — a hideout is a wall cell with exactly **3** wall neighbours and
exactly **1** empty neighbour.
§10.1a (hideouts) **[SETTLED]** — the old placement (one attempt per room, stop at
first failure, never in corridors) left the hiding game with no board. **Place
hideouts along the corridor network and near junctions, and do not stop at first
failure.** Hidden-until-seen (§11.5a) so scouted flight paths are worth more.

## Acceptance criteria
- [ ] Hideout carving per the 3-wall/1-empty rule.
- [ ] Placement covers corridors and junctions, not only rooms.
- [ ] Placement pass does **not** stop at first failure.
- [ ] Empty hideout: fill 0.0, walk-through; occupied: fill 1.0, partial sound.
- [ ] Tests: hideouts exist on corridor cells; a fixed seed yields multiple
      reachable hideouts.
- [ ] fmt/clippy/test gate passes.

## Notes / risks
- The "every cell within N steps of a hideout" assertion is **[OPEN]** (§10.1a) —
  don't build it yet; note the metric for later once there's play evidence.
- Depends on B3 (pillars create the geometry) — sequence after it.

---

### B5 — Corridor cover + the sightline assertion → #11
**Labels:** `area:generation` `type:feature` `size:M`
**Milestone:** v1

## Summary
The generator's most important job after connectivity, and a direct fix for the
#1 reason the old game wasn't fun (§7.6): long dead-straight full-span corridors
are sightlines with no counterplay. Add cover to corridors and *assert* no unbroken
straight sightline exceeds L.

## Design reference
§10.1a **[SETTLED]** — **no unbroken straight sightline longer than L**, start
L ≈ **10–12** (≈ a guard's sight range) **[START]**. Testable property: *for every
cell, for each of the 4 cardinal directions, the unobstructed run length is ≤ L.*
Ways to satisfy (all **[START]**, experiment — this is what §13 is for): **jog the
corridors** (offset mid-span so they bend); **give corridors features** (the B3
step, sized for 2–4-wide space: recesses, buttresses, a 1-cell squeeze); **cover
near doors** (something to duck behind on the far side).
§7.6 fix 3 — "probably the single biggest contributor to the problem," invisible if
you only look at the AI.

## Acceptance criteria
- [ ] The sightline assertion implemented as a level property check (every cell ×
      4 directions, run ≤ L).
- [ ] At least one satisfying technique implemented (jogs and/or corridor features).
- [ ] Generation produces levels that pass the assertion for the v1 config; a level
      that fails is repaired or the seed rejected (ties into B7).
- [ ] L is a single named tunable, not a scattered literal.
- [ ] Property test over many seeds: assertion holds.
- [ ] fmt/clippy/test gate passes.

## Notes / risks
- This is the machinery for open question §15.2 (how much cover, placed how) —
  build it so jogs vs. features vs. both are switchable, not hardcoded to one.
- Over-covering makes the building stop reading as a building; keep L a knob.

---

### B6 — Placement and spacing guarantees → #12
**Labels:** `area:generation` `type:feature` `size:M`
**Milestone:** v1

## Summary
Place entry/exit, player, objectives, and guards — with the spacing guarantees the
old generator entirely lacked (player could spawn next to the exit, all intel in
one room, a guard that sees you on turn one). Fail loudly or retry the seed; never
silently drop a guard.

## Design reference
§10.1 steps 7–9 — entry/exit and player in the **largest room** at random empty
cells; objectives in any room *except* the start room; guards in any room except
the start room.
§10.2 — v1 config: 40×40, **5 guards**, **3 intel**, exit rule = all intel required.
§10.6 (spacing + placement) — **the starting area should be safe**: separate player
from exit; spread intel out; keep a guard from spawning where it sees you turn one.
**Placement must not fail silently** — guards were quietly dropped (asked 5, got 4);
objectives threw after 100 tries. **Fail loudly or retry the seed.**

## Acceptance criteria
- [ ] Entry/exit/player in the largest room; objectives + guards excluded from the
      start room.
- [ ] Spacing: min player↔exit distance; intel spread across distinct rooms; no
      guard's turn-one cone covers the player spawn.
- [ ] Exactly the requested counts placed (5 guards, 3 intel) or the seed is
      rejected — never a silent shortfall.
- [ ] Tests: requested counts always met on accepted seeds; start area safe on
      turn one across many seeds.
- [ ] fmt/clippy/test gate passes.

## Notes / risks
- Turn-one safety depends on the guard cone (C1); if C1 isn't landed yet, gate on a
  conservative box and tighten once FOV exists.
- Interacts with B7: rejection here and reachability rejection there should share
  one seed-retry loop.

---

### B7 — Reachability assertion + seed rejection → #13
**Labels:** `area:generation` `type:feature` `size:S`
**Milestone:** v1

## Summary
Assert the level is actually solvable — a path start → every objective → exit —
and reject the seed if not. The old generator never verified solvability and could
seal a room (with its objectives and guards) behind sub-3-cell wall runs, undetected.

## Design reference
§10.6 **[SETTLED intent]** — **do not rely on the structural argument; assert
reachability and reject the seed.** It's a flood fill; it costs nothing. Guarantees
to (re)confirm: fully enclosed; corridor network connected; every room reaches a
corridor; rooms ≥6×6, ≤~12; **a path exists start → every objective → exit.**

## Acceptance criteria
- [ ] Flood-fill reachability over walkable+pathable cells (remember closed panels
      don't block pathing).
- [ ] Assert start reaches every objective and the exit; reject+reseed on failure.
- [ ] A single generation entry point that only ever returns levels passing every
      §10.6 guarantee (shared retry loop with B5/B6 rejections).
- [ ] Test: a deliberately sealed fixture is rejected; accepted seeds are always
      solvable.
- [ ] fmt/clippy/test gate passes.

## Notes / risks
- Cap retries and surface a loud error if a config can't produce a valid level, so
  a bad parameter set fails fast instead of looping forever.

---

## v1 — C. Vision (§6)

### C1 — Shadowcast FOV: facing cone + 360° touching ring → #41
**Labels:** `area:vision` `type:feature` `size:M`
**Milestone:** v1

## Summary
Field of view: a facing-dependent forward cone via symmetric shadowcast over a
square box, plus the always-seen 8-neighbour ring. Shared by player and guards
(different arc widths). This is what the whole avoidance game — and the danger
overlay — reads from.

## Design reference
§6.1 — facing-dependent forward cone, blocked by walls/closed panels/hinges; an
opaque cell is itself seen but shadows behind it. **Range is a square box, not a
circle [START]** (range R → (2R+1)² box), no falloff. **The 8 cells around any
viewer are always seen, every direction, incl. behind [SETTLED]** — you can never
stand adjacent to a guard undetected.
§6.2 — implement the cone by treating out-of-arc cells of the viewer's own
8-neighbour ring as walls, so shadowcast carves the cone; artificial walls are
still marked seen → the 360° ring for free. Neighbour tiers 1–5 by angular
deviation; a neighbour is transparent iff `arc_width ≥ tier`. Arc width 2 = ~90°
(guards), 3 = ~180° (player), 5 = 360° (player waiting).
§5 / §7.1 — player range 15 arc 3; guard range 10 arc 2.

## Acceptance criteria
- [ ] Symmetric shadowcast over the square box, walls/panels/hinges opaque.
- [ ] Arc produced via the ring-as-walls trick; arc_width ↔ tier rule exact.
- [ ] The 8-neighbour ring is always in the visible set regardless of facing/arc.
- [ ] Player (R15/arc3) and guard (R10/arc2) both drive the same function; Wait
      gives arc 5 (§8.3).
- [ ] Golden tests on fixed fixtures: cone shape per arc width; ring always seen; an
      opaque cell shadows correctly.
- [ ] Recomputed in the turn loop's sight phase (A2) for every viewer.
- [ ] fmt/clippy/test gate passes.

## Notes / risks
- Box-vs-circle is open question §15.6 — keep range a knob so a circle can be tried.
- The one-turn cone-lag (§4.2 / §15.7) is **[OPEN]** — do **not** build it in; leave
  a clean seam so it could be reintroduced deliberately later.

---

## v1 — D. Guards (§7)

> **Read §7.6/§7.7 before touching any of these.** The individual guard must be
> *escapable* (loosen); the collective must be the threat (tighten). Get it
> backwards — sticky guards *and* cooperation — and you rebuild exactly what wasn't
> fun. Tune D3 and D6 as a pair.

### D1 — Guard baseline and state machine
**Labels:** `area:guards` `type:feature` `size:M`
**Milestone:** v1

## Summary
The guard entity and its five-state machine skeleton — the scaffold every guard
behaviour (patrol, chase, search, radio, cooperation) plugs into. State is read off
the guard's colour every turn (§11.2), so the machine *is* the visible AI.

## Design reference
§7.1 — range 10, arc 2, initial facing South, **speed 1/turn always [SETTLED]**,
alert duration 30 **[START]**.
§7.4 — states: **Calm** (yellow, patrol), **Alerted** (orange, walk to dest then
search), **Chasing** (red, dest ← player live cell, alert←30, step shortest path),
**Investigating** (red, toward source, lower severity), **Responding** (orange,
dispatched by missed ping, walk to silent guard's post). Encode entry conditions
and transitions.
§4.2 phase 3 — guards read current sight, decide, act.

## Acceptance criteria
- [ ] `Guard` struct (station, facing, state, alert timer, private memory hook).
- [ ] State enum + transition function matching the §7.4 table; alert timer decays.
- [ ] Guards act in phase 3 of the loop; speed strictly 1/turn in every state.
- [ ] Shortest-path stepping helper (respecting pathing rules: closed panels
      transparent, walls/hinges block).
- [ ] Category/colour derived from state each turn (feeds render tickets).
- [ ] Tests: transition table exercised; no state ever moves >1 cell/turn.
- [ ] fmt/clippy/test gate passes.

## Notes / risks
- This ticket ships the skeleton + Calm↔Chasing basics; Patrol (D2), zones (D3),
  search (D4), radio (D5), cooperation (D6) fill the rest. Keep transitions data-ish
  so those slot in.
- Avoid the §7.6 "tracking turret": don't hardcode chase to re-aim perfectly — D3
  replaces flat detection with zones.

---

### D2 — Patrol: farthest-uninspected sweep
**Labels:** `area:guards` `type:feature` `size:M`
**Milestone:** v1

## Summary
Emergent, purposeful-looking patrols with no authored routes: each guard walks to
the *farthest* cell it hasn't recently inspected in its territory, which makes it
pace across distances instead of shuffling locally.

## Design reference
§7.5 — each guard has a **station** (spawn) and **patrol radius 15 [START]**; keeps
private memory of inspected cells; with no destination walks to the **farthest**
uninspected currently-empty cell in its territory (*farthest*, not nearest — this is
what reads as purposeful); wipes memory and restarts when none remain.
§7.5 known weakness — box territories straddle walls/unreachable rooms; **downstream
of §10.5** — prefer territory defined via the region graph (A3) over a raw box.

## Acceptance criteria
- [ ] Per-guard inspected-cell memory, updated from its FOV each turn.
- [ ] Destination selection picks the farthest uninspected reachable cell in
      territory; memory wipes and restarts when exhausted.
- [ ] Territory derived from the region graph (A3) rather than a naive box, so it
      doesn't spill through walls into unreachable space.
- [ ] Tests: guards converge to full coverage then reset; destinations are reachable.
- [ ] fmt/clippy/test gate passes.

## Notes / risks
- Using regions for territory is the A3 payoff called out in §7.5 — take it here.
- Keep patrol radius a tunable [START].

---

### D3 — Two-zone detection and an escapable chase
**Labels:** `area:guards` `type:feature` `size:M`
**Milestone:** v1

## Summary
**The most important open question in the document (§15.1).** Replace the old flat
range-10 detection (a tracking turret with no exit) with two zones so a chase can
actually end, and so Run has a job. This is the fix the entire hiding game depends on.

## Design reference
§7.6 fix 1 **[SETTLED that the chase must end; mechanism [START]]** — two detection
zones: **Certain ≤ 5** → *Chasing*, tracks your live cell; **Glimpse 6–10** →
*Investigating*, moves toward where it *thinks* you are (your position when last in
the certain zone), imprecise; **Gone > 10** → search then patrol. The 5-cell certain→
glimpse gap is exactly Run's 5-cell gain — **design that relationship, don't leave
it coincidental**; retune together.
§7.6 fix 4 — **the tail is not the threat; the net is** (D6). A lone guard at your
exact speed can't catch you in the open, and shouldn't. Danger = cornered and cut
off, never out-jogged.
§15.1 — this is a *proposal*, not a decision; the harness (§13) is for trying
alternatives (tiring/breaking off, cone narrowing, real falloff). The known-bad
answer is "tracks perfectly at any range within 10."

## Acceptance criteria
- [ ] Detection resolves to Certain/Glimpse/Gone by range within the cone.
- [ ] Chasing tracks the live cell; Investigating targets the last-certain cell,
      not the live one.
- [ ] Losing the certain zone downgrades to Investigating, then to search (D4) —
      no instant re-lock across the whole range.
- [ ] Zone bounds and Run's gain are named tunables wired so they move together.
- [ ] Tests: a player who opens a 5-cell gap drops from Chasing to Investigating;
      a straight-line tail at parity speed never results in capture in open space.
- [ ] fmt/clippy/test gate passes.

## Notes / risks
- Do **fix 1 before making search harder** (§7.6 explicitly): a better search on an
  inescapable chase makes the game worse. D3 lands before/with D4.
- Paints as two shades of red in the danger overlay (F-series) — expose the zone so
  render can show it.

---

### D4 — Search on lost sight
**Labels:** `area:guards` `type:feature` `size:S`
**Milestone:** v1

## Summary
When a guard reaches its destination and finds nothing, it sweeps the surrounding
area for a while before resuming patrol — turning the chase from binary (glued or
gone) into the *hunted* phase that §7.6 calls the best part of the game.

## Design reference
§7.6 fix 2 — on reaching a destination and finding nothing, sweep the surrounding
area for a number of turns before resuming patrol. **Ordering matters:** the old
problem was never that guards gave up too fast — it was that you could never reach
the giving-up phase. **Fix 1 (D3) first.**
§7.4 — Alerted: "walks to its destination, then searches it."
§7.6 chase-shape table — Lost → Hunted → Released; "this region gets watched harder"
afterward.

## Acceptance criteria
- [ ] On arriving at a destination with nothing seen, a guard enters a bounded
      search (sweeps nearby cells) before returning to Calm patrol.
- [ ] Search duration is a named tunable [START].
- [ ] After release, the searched region is patrolled harder for a while (even
      lightweight: bias patrol toward it).
- [ ] Tests: a guard that loses the player searches the last-known area then resumes
      patrol; search is bounded.
- [ ] fmt/clippy/test gate passes.

## Notes / risks
- Depends on D3 — do not merge search improvements while the chase is still
  inescapable (§7.6 is explicit).
- Hideout-checking during search is open question §15.5 — default "only when
  alerted and only if they saw you enter or found a body nearby"; keep it a knob.

---

### D5 — Radio net: pings and missed-ping dispatch
**Labels:** `area:guards` `type:feature` `size:M`
**Milestone:** v1

## Summary
The mechanism that keeps permanent takedowns costly without guards waking up:
control pings each guard periodically; a downed guard can't answer; a miss
dispatches a responder and a second miss escalates the alert. Every takedown becomes
a future appointment.

## Design reference
§7.3 — ping interval **~20 turns/guard, jittered [START]**; **missed ping →** control
dispatches the nearest active guard to the silent guard's last post; **second missed
ping →** facility-wide alert step. Takedowns become a clock, not a one-time cost —
the strategy scales badly on its own (no rule needed to ban a full clear). Diegetic
and legible: the player *hears* pings (§9), so the clock is plannable. A **hidden**
body still misses its ping — hiding buys a *confused* investigation, not none.
§7.4 — Responding state (orange, walk to silent guard's post).

## Acceptance criteria
- [ ] Per-guard jittered ping schedule driven by the run Rng.
- [ ] A downed guard misses its ping; first miss dispatches the nearest active guard
      (Responding) to its last post; second miss steps the facility alert.
- [ ] A ping emits a low sound from the guard's position (feeds E1) so the clock is
      audible.
- [ ] Hidden body still counts as a miss (investigation, not silence).
- [ ] Tests: a takedown leads to a missed ping → dispatch on schedule; second miss
      escalates.
- [ ] fmt/clippy/test gate passes.

## Notes / risks
- Depends on takedowns/bodies (F3) to have anything to miss — sequence after, or
  drive tests with a synthetic "downed guard" fixture.
- The alert here is the concrete escalation source §7.3 wants — don't reintroduce a
  free-floating global alert number that's never read (§2.3).

---

### D6 — Cooperation: call it in and converge
**Labels:** `area:guards` `type:feature` `size:M`
**Milestone:** v1

## Summary
Where the threat actually lives. A chasing guard calls it in; other guards in radio
range switch to Responding and converge on the reported position *from ahead*. This
converts a chase from a boring unwinnable tail into a readable, solvable spatial
problem — and is the single biggest lever on difficulty.

## Design reference
§7.7 **[START]** — a chasing guard calls it in; guards within radio range → Responding,
converge on the reported position. A guard that finds a body calls it in **harder**.
**Guards don't merge into a hive mind** — they know what's been *said*, not what
each other sees; the player can exploit a guard out of radio contact or reach one
before it finishes reporting (**"silence the radio" is a real tactic**, gives
takedowns purpose beyond removal).
§7.6 fix 4 — the net, not the tail. Danger = converging responders cutting you off.
§7.7 tuning warning — tune with D3 as a pair: loosen the individual, tighten the
collective.

## Acceptance criteria
- [ ] A chasing guard broadcasts the reported position to guards within radio range,
      switching them to Responding/converging.
- [ ] A found body calls in harder (wider/stronger) than a sighting.
- [ ] Knowledge is *communicated*, not shared FOV; a guard out of radio range or
      interrupted mid-report doesn't propagate.
- [ ] Radio range is a named tunable [START].
- [ ] Tests: a single sighting pulls in-range guards toward the report; an
      out-of-range guard is unaffected; convergence approaches from multiple sides.
- [ ] fmt/clippy/test gate passes.

## Notes / risks
- This is where "levels adapt to your strategy" lives — keep it strong but paired
  with D3's escapable individual, or you rebuild the un-fun game (§7.7 warning).
- Ties to D5's radio net — share one radio-range/contact model between them.

---

### D7 — Guard mutual pathing (no deadlock)
**Labels:** `area:guards` `type:bug` `size:S`
**Milestone:** v1

## Summary
Guards are solid to each other but must path *around* one another. In the old
version they pathed *through* each other, failed the move, and stalled — guards
deadlocked in corridors.

## Design reference
§7.8 — guards solid to each other but **path around** each other (old bug: pathed
through, failed, stalled → corridor deadlock). Guards can't hurt each other.

## Acceptance criteria
- [ ] Pathfinding treats other guards as obstacles (or yields/reroutes) so two
      guards never deadlock trying to occupy each other's cells.
- [ ] No guard skips its turn indefinitely due to a blocked step.
- [ ] Test: two guards routed through a 2-wide corridor toward each other resolve
      without stalling.
- [ ] fmt/clippy/test gate passes.

## Notes / risks
- Keep it cheap — a local yield/step-aside is enough at a few dozen entities; no
  need for full multi-agent reservation.

---

## v1 — E. Sound (§9)

### E1 — Sound propagation and noise sources
**Labels:** `area:sound` `type:feature` `size:M`
**Milestone:** v1

## Summary
The single largest missing system, and the one most likely to make the game good.
Sound spreads cell-to-cell through walkable space (around walls, not through them),
losing intensity per step; guards hear it and investigate. Gives the player a
second information channel and a way to steer guard attention.

## Design reference
§9.1 **[SETTLED]** — sound spreads cell to cell through **walkable space**,
cardinally, losing intensity per step; **reach is path distance, not straight-line**
(flows around walls). **Closed doors attenuate heavily; open doors don't.** A guard
hears if intensity at its cell exceeds a threshold → switches to Investigating with
the source (or **best guess [OPEN]**) as destination.
§9.2 **[START]** (primary tension tuning surface) — noise table: move Low, run High,
door Medium, takedown Medium, body-drop Medium, drag Low-continuous, wait/camo/
dephase None, **radio ping Low from guard pos**, **guards talking/patrolling Low**
(so the player tracks guards through walls).

## Acceptance criteria
- [ ] Path-distance propagation (BFS over walkable/attenuating cells) with per-step
      falloff; walls block, closed doors attenuate heavily, open doors don't.
- [ ] A noise-source vocabulary with the §9.2 intensities as named tunables.
- [ ] Guards above threshold switch to Investigating toward the source (best-guess
      seam left for §9.1 [OPEN]).
- [ ] Guards and radio pings emit sound, giving the player a through-wall channel.
- [ ] Tests: sound routes around a wall (path not line); a closed door blocks what
      an open door passes; running is heard farther than walking.
- [ ] fmt/clippy/test gate passes.

## Notes / risks
- This is the primary tension surface — every intensity a tunable, no magic numbers.
- Presentation is a separate ticket (G6) and an open UI problem (§9.3/§15.3); this
  ticket is the model only.

---

## v1 — F. Abilities (§8)

### F1 — Ability model and the time economy
**Labels:** `area:abilities` `type:feature` `size:M`
**Milestone:** v1

## Summary
The data-driven ability framework the whole architecture exists to enable (§3):
most abilities are a declarative record over a small effect vocabulary; the economy
is **time** (turn cost, duration, cooldown), not mana. Trying "what if there were a
smoke grenade" should mean adding a row, not writing a system.

## Design reference
§8.1 **[SETTLED]** — hybrid: declarative record (cost, range, targeting, duration,
cooldown, effects from a small primitive vocabulary) for the common case; a
code escape hatch behind the same interface for the weird case. Start data-driven;
promote to code only when the vocabulary genuinely can't express it.
§8.2 **[no energy/mana/charges]** — economy is time. Duration ticks while active,
switches off at 0. Cooldown set at activation, **frozen for the whole duration**,
drains only while inactive → true lockout = duration + cooldown. Toggling off is
free and refunds nothing. **Name and document the tick convention** (the §8.2
"N yields N−1, activation turn unprotected" trap) and make the UI report the number
the player actually gets.
§2.3 — **cost is the load-bearing property.** An ability that costs nothing is not a
decision.

## Acceptance criteria
- [ ] An `Ability` record type (cost/range/targeting/duration/cooldown/effects) plus
      an effect-primitive vocabulary, and a code escape hatch behind one interface.
- [ ] Activation/duration/cooldown state machine matching §8.2 exactly (frozen
      cooldown during duration; free cancel; documented tick convention).
- [ ] The effective-turns number an ability grants is queryable for the UI.
- [ ] Tests: lockout = duration + cooldown; cancel is free and refunds nothing;
      the documented tick convention holds (no silent N−1).
- [ ] fmt/clippy/test gate passes.

## Notes / risks
- Resist growing the vocabulary for one-offs (§8.1) — that's how DSLs rot.
- Every [START] number here is a candidate `type:tuning` ticket later; file the
  framework now, note the numbers.

---

### F2 — Targeting system
**Labels:** `area:abilities` `type:feature` `size:S`
**Milestone:** v1

## Summary
Build targeting up front — its absence in the old version (auto-target-nearest) was
the direct cause of the free unlimited-range neutralise and distorted the whole
design. At minimum self, direction, and tile-within-range with a cursor.

## Design reference
§8.4 **[SETTLED]** — the old version had **no targeting system at all**; build it up
front. Minimum modes: **self**, **direction**, **tile within range** (with a
cursor). It unblocks most of the interesting ability space.

## Acceptance criteria
- [ ] Targeting modes: self, direction, tile-within-range (cursor-driven), selected
      per ability from its record (F1).
- [ ] Range/validity enforced by the targeting layer, not by each ability.
- [ ] Cursor targeting integrates with input (G5) and confirm/cancel.
- [ ] Tests: out-of-range tile rejected; direction target resolves to the faced
      cell; self target needs no cursor.
- [ ] fmt/clippy/test gate passes.

## Notes / risks
- This is the guardrail against re-creating the free-neutralise failure — range
  lives *here*, so no ability can quietly become unlimited-range.

---

### F3 — Innate abilities: Move, Wait, Run, Takedown, Drag (+ bodies)
**Labels:** `area:abilities` `type:feature` `size:M`
**Milestone:** v1

## Summary
The always-available verbs, including the central mechanic — the takedown — and the
body it leaves as its cost. This is the ability the whole design hangs on, and the
old one was free/unlimited-range and therefore *was* the game.

## Design reference
§8.3 innate table — **Move** (1 turn, sets facing), **Wait** (1 turn, **360° vision
that turn**), **Run** (1 turn, dur 5, cd 12, +1 free move/turn → 2 cells/turn),
**Takedown** (§7.2), **Drag** (1 turn/step, half speed while dragging, free release).
§7.2 **[SETTLED]** — takedown: **adjacent only**, requires target **has not detected
you this turn**, **costs the full turn**, target **permanently out, leaves a body**,
no cooldown (constraints are the cost). Because of the 360° ring, only unaware
guards are takedownable.
§7.2 bodies — a body is solid (fill 1.0), **can be seen** (a guard whose cone covers
it has *found* it), **can be dragged**, **can be hidden** (in a hideout it's gone).
**Finding a body is the loudest event in the game** — raises alert harder than being
seen (feeds D5/D6).
§8.3 notes — Run is a guaranteed escape (watch the pair with §7.6); Drag is half
speed.

## Acceptance criteria
- [ ] Move/Wait/Run/Takedown/Drag implemented against the F1 framework with the
      exact §8.3 numbers as tunables.
- [ ] Wait grants arc-5 (360°) FOV for that turn (C1 seam).
- [ ] Takedown: adjacent-only, unaware-only, full-turn cost, permanent, spawns a
      body (fill 1.0).
- [ ] Bodies can be seen (found by a covering cone), dragged (half speed), and
      hidden in a hideout (removed from play); finding one emits the loudest alert
      event.
- [ ] Tests: can't take down a guard that saw you this turn; takedown ends the turn;
      body blocks and is found when in a cone; drag halves speed.
- [ ] fmt/clippy/test gate passes.

## Notes / risks
- Cost is load-bearing (§2.3) — the takedown's cost is the body + the radio clock
  (D5); do not add a "free" fast path.
- Depends on C1 (unaware check), F1 (framework); bodies feed D5/D6.

---

### F4 — Salvaged tech: Camouflage, Decoy, Dephase
**Labels:** `area:abilities` `type:feature` `size:M`
**Milestone:** v1

## Summary
The found-in-facility ability set — the sandbox the architecture exists to
experiment in (§8.3). Three tech abilities with distinct, deliberately-constrained
effects, each with a real cost.

## Design reference
§8.3 salvaged table — **Camouflage** (dur 10, cd 20): undetectable **while you don't
move**; moving reveals you that turn. **Decoy** (dur 20, cd 30): a fake intruder in
the faced cell; draws **Investigating, not Chasing**; dies when anything steps on it.
**Dephase** (dur 3, cd 30): fill→0, walk through walls/doors/guards; **does not
conceal**; **while dephased you cannot bump** (no doors/consoles/win); **lethal if
duration expires inside a wall [START]**.
§8.3 notes — Camouflage doesn't stop capture (invisible ≠ safe, §4.5); Decoy draws
Investigating only (a guard that sees *you* ignores it) — works on guards that lost
you, not ones that have you.

## Acceptance criteria
- [ ] Camouflage: concealed only while stationary; a move reveals you that turn;
      does not prevent capture-by-contact.
- [ ] Decoy: fill-0 fake intruder in the faced cell, draws Investigating (never
      Chasing), consumed when stepped on.
- [ ] Dephase: fill→0 movement through solids; cannot bump (no interact/win) while
      active; expiring inside a wall is lethal.
- [ ] All three built as F1 records with the §8.3 numbers as tunables.
- [ ] Tests: moving while camouflaged reveals; a seeing guard ignores a decoy;
      dephase can't open a door; dephase death inside a wall.
- [ ] fmt/clippy/test gate passes.

## Notes / risks
- v1 has no intel-as-currency/campaign, so "found in the facility" here just means
  available/placed — don't build acquisition economy (that's v3).
- Keep the excellent constraints (Dephase can't bump, Camo doesn't stop capture) —
  they're what make these decisions, not buttons.

---

## v1 — G. Presentation and web (§11, §12.2)

### G1 — Renderer interface + ASCII character grid + glyph priority → #14
**Labels:** `area:render` `type:feature` `size:M`
**Milestone:** v1

## Summary
The game's identity and its cheapest art pipeline: render state to a grid of
(char, fg, bg) cells, as a **pure function of state** so the whole UI is assertable
in a test with no browser. A separate renderer interface so a tile renderer can drop
in later.

## Design reference
§11.1 **[SETTLED]** — grid of cells (char + fg + bg); pure function of state; prints
as text → assertable without a browser. Renderer is a **separate concern behind one
interface** (ASCII now, tiles later = same interface, `drawImage` for `fillText`);
the core must not know which is in use.
§11.3 — glyph table (@ player/decoy, g guard, z body, # wall, blank floor, + panel,
× hinge, } hideout, $ intel, E exit). **Overlapping glyphs need a priority order**
(old bug: last-writer-wins, guard in a doorway rendered arbitrarily) — define it.

## Acceptance criteria
- [ ] A `Renderer` trait/interface producing a grid of (char, fg-category, bg) from
      state; core has no knowledge of the concrete impl.
- [ ] An ASCII/text implementation usable in native tests (state → text grid).
- [ ] A defined glyph priority order resolving overlaps deterministically.
- [ ] Golden test: a fixed state renders to a fixed expected text grid.
- [ ] fmt/clippy/test gate passes.

## Notes / risks
- Colour is a *category* here (G2 owns the mapping) — don't bake concrete colours
  into the grid producer (§11.2).

---

### G2 — Colour category system → #15
**Labels:** `area:render` `type:feature` `size:S`
**Milestone:** v1

## Summary
Systems declare an information *category*; presentation owns the category→colour
mapping. Recolouring/accessibility is a one-table edit, and the guard glyph
re-categorised each turn means the player reads the AI state machine straight off
the colour.

## Design reference
§11.2 **[SETTLED]** — colours not chosen by game systems; systems declare a category,
presentation maps it. Table: Neutral white, Owned blue, Caution yellow, Warning
orange, Danger red, Interest purple, System tan. **Guard glyph re-categorised every
turn from its state** (yellow→orange→red = the guard's mind, visible); message colour
uses the same table. Base palette: 16-colour colour-blind-safe qualitative set, fg +
darkened bg variant. **[START]** full-range colour (old one washed everything into
0.1–0.9 with no true black/white).

## Acceptance criteria
- [ ] A `Category` enum (Neutral/Owned/Caution/Warning/Danger/Interest/System) and a
      single category→colour table.
- [ ] Guard glyph category derived from state each turn; message category uses the
      same table.
- [ ] Full-range palette (true black/white available), colour-blind-safe set, with
      darkened bg variants.
- [ ] Test: a chasing guard maps to Danger/red; recolour is a one-table change.
- [ ] fmt/clippy/test gate passes.

## Notes / risks
- No game system may reference a concrete colour — enforce the category seam or the
  §11.2 win is lost.

---

### G3 — FOV rendering + danger overlay + floor dots → #16
**Labels:** `area:render` `type:feature` `size:M`
**Milestone:** v1

## Summary
Draw live visibility and the best idea in the old game — the danger overlay, which
paints the literal guard detection set: *the lose condition, painted*. Plus the two
legibility fixes the old version needed.

## Design reference
§11.5 — FOV controls lighting not knowledge. In FOV → full colour; outside FOV →
same glyph dark gray (dim but legible); **watched by a guard & in FOV → red
background (the danger overlay)**. **[SETTLED]** the overlay paints the literal
detection set the AI queries — if your cell isn't red, no guard you can see detects
you. Two fixes: (1) watched-but-unseen must **not** render dark-on-dark (old bug made
the watched cells look safest); (2) **render floor as dots** so the FOV boundary is
visible on open ground.
§7.6/D3 — expose the two detection zones as **two shades of red**.

## Acceptance criteria
- [ ] In-FOV full colour; out-of-FOV dimmed but legible.
- [ ] Danger overlay = red bg on cells in a visible guard's detection set; certain
      vs glimpse zones as two red shades.
- [ ] Watched-but-unseen cells are not misleadingly safe-looking (fix #1).
- [ ] Floor rendered as dots so the FOV edge reads across open floor (fix #2).
- [ ] Golden test: a fixture with a guard cone paints the expected red set.
- [ ] fmt/clippy/test gate passes.

## Notes / risks
- The overlay must query the *same* detection data the guard AI uses (C1/D3) — not a
  re-implementation, or it can lie.

---

### G4 — Fog and tile memory: visible layout, hidden contents → #17
**Labels:** `area:render` `type:feature` `size:M`
**Milestone:** v1

## Summary
The three-state visibility model: geometry always visible (plan a route from turn
one), contents hidden until seen then remembered, live state only what's visible now.
This is new — the old version had no memory system — and it's what lets the player
plan escape routes before being spotted (§7.6).

## Design reference
§11.5a **[SETTLED]** — three layers: **Geometry** (walls/corridors/doors/room shapes)
**always visible, never fogged**; **Contents** (intel, hideouts, equipment, lore)
**hidden until seen, then remembered**; **Live state** (guards, bodies, door
open/closed, danger cones) **only what you see now, never remembered.** Pairing:
plan confidently, get surprised by contents not architecture. Hideouts hidden-until-
seen rewards thorough exploration. **Implementation note: "remembered" needs its own
visual state — three states, not two.**

## Acceptance criteria
- [ ] Geometry always drawn (full, from turn one).
- [ ] Contents hidden until first seen; once seen, remembered and drawn in a
      distinct "remembered" visual state.
- [ ] Live state drawn only within current FOV, never remembered.
- [ ] Per-cell memory tracked in state (deterministic).
- [ ] Golden test: an unseen intel is invisible; after entering FOV it becomes
      remembered and stays after leaving FOV; a guard does not persist out of FOV.
- [ ] fmt/clippy/test gate passes.

## Notes / risks
- Three visual states (live / remembered / never-seen) — don't collapse remembered
  into the dimming scheme (§11.5a note).

---

### G5 — Layout, input, and explicit hotkeys → #18
**Labels:** `area:render` `type:feature` `size:M`
**Milestone:** v1

## Summary
Assemble the screen — player-centred viewport, fixed ability column, full-width
message bar — and wire input with **explicitly assigned** ability hotkeys (the old
derived-from-label scheme silently changed a key when the list above it changed,
which is unacceptable in a game where a mis-key ends a run).

## Design reference
§11.4 — map viewport centred on player; abilities column fixed **14** cols,
right-anchored, label states `Name` / `Name [3]` active / `Name /2/` cooling /
`Name` unusable; message bar = bottom row, **solid band in the message's category
colour** (threat reads as a colour flash).
§11.6 — input: arrows/`4628` move, `5`/`w` wait, Enter/Space confirm, Escape
cancel/menu, letters = ability hotkeys. **Assign ability hotkeys explicitly** (not
derived from label). *(Touch is [OPEN] §15.8 — out of scope here.)*
§11.7 — messages carry category + priority + optional source cell; bar shows only
the highest-priority; **clears on next action** (status line, not scrollback)
**[START]**; priority ladder: self-narration ≤0, threat 2→4→10, objective 20.

## Acceptance criteria
- [ ] Player-centred viewport; 14-col right-anchored ability column with the four
      label states; full-width message bar coloured by category.
- [ ] Input mapping per §11.6; **ability→key assignments are explicit constants**,
      stable regardless of ability list order.
- [ ] Highest-priority message shown; message clears on next action; priorities per
      the §11.7 ladder.
- [ ] Golden test on a full-screen render fixture; a test asserting hotkeys don't
      shift when the ability list changes.
- [ ] fmt/clippy/test gate passes.

## Notes / risks
- Keep input handling in the web crate thin; the mapping (key→action) can live in
  core so it's testable (§12.1).

---

### G6 — Sound presentation → #19
**Labels:** `area:render` `type:feature` `size:S`
**Milestone:** v1

## Summary
Show the player a sound they can't see — an unsolved, genuinely interesting UI
problem (§9.3/§15.3). Sound is the #1 missing system; an invisible sound system is a
missing one. Ship one legible presentation and keep it swappable.

## Design reference
§9.3 **[OPEN]** — the player cannot see a sound. Options: a directional indicator at
the screen edge; a brief flash on the source cell if within range; a compass readout.
§15.3 — genuinely interesting, gates how good the whole sound system feels; a real UI
research problem — try one, keep it swappable.

## Acceptance criteria
- [ ] At least one sound presentation (e.g. source-cell flash within range +
      edge/direction indicator) driven by the E1 sound model.
- [ ] The presentation is a category-driven render concern, not baked into the sound
      model; swappable for experiments.
- [ ] Test: a sound within range produces the expected presentation event; out of
      range does not.
- [ ] fmt/clippy/test gate passes.

## Notes / risks
- **[OPEN]** by design — don't over-invest; the point is to make sound *legible*
  enough to play with, then iterate via §13.
- Depends on E1.

---

### G7 — Web shell: wasm-bindgen + canvas2d → #20
**Labels:** `area:build` `type:chore` `size:M`
**Milestone:** v1

## Summary
The thin web crate: a hand-rolled canvas2d glyph renderer implementing the G1
interface, wasm-bindgen wiring, the input pump, and the static page that ships to
GitHub Pages. No engine — a glyph grid is ~200 lines.

## Design reference
§12.2 **[SETTLED]** — web crate is wasm-bindgen + canvas2d + input, **thin**; core
must not know the platform. **Renderer: hand-rolled canvas2d [SETTLED]** (`fillText`
now, `drawImage` for tiles later, same interface). §3 — ships as a **static GitHub
Pages site**, no server/CLI/runtime dependency but a browser; must still build in
five years.
§12.2 layout — `web/` holds index.html, font, assets.

## Acceptance criteria
- [ ] Canvas2d implementation of the G1 renderer interface (glyph grid via
      `fillText`, fg/bg from the G2 category table).
- [ ] wasm-bindgen entry point driving the core loop; input pump feeding G5's
      mapping into `step`.
- [ ] Static `web/index.html` + font/assets; the game runs in a browser end-to-end.
- [ ] Builds for `wasm32-unknown-unknown`; bundle stays small; no non-browser
      runtime dep.
- [ ] fmt/clippy/test gate passes (plus wasm build in CI).

## Notes / risks
- Keep it genuinely thin — logic belongs in core (§12.1) so it stays testable
  natively. This crate is glue + draw only.
- This is the "build→play" half of the experiment loop (§13.1) — the deploy preview
  and seed-sharing UI ride on it.

---

## v1 — H. Determinism and testing (§12.4, §13.1)

### H1 — Replay `(seed, inputs)` + golden grid tests
**Labels:** `area:build` `type:chore` `size:M`
**Milestone:** v1

## Summary
Cash in the determinism the whole architecture is built on: a replay is
`(seed, [inputs])` and nothing else, which buys 40-byte bug repros, golden grid
tests, and regression detection almost for free. This ticket makes replay a
first-class, tested capability.

## Design reference
§12.4 **[SETTLED]** — one seed per run; PRNG pinned (done in #1); **a replay is
`(seed, [inputs])`, nothing else.** Buys: bug repro (40 bytes), seed sharing, bot
metrics (§13), golden tests (replay a run, assert the final grid), regression
detection (same seed+inputs → different result = you changed the game), rewind.
§13.1 — build→play loop needs **seed sharing** so a specific level can be handed
around and replayed exactly.
§11.1 — the character grid is a pure function of state → assertable in tests.

## Acceptance criteria
- [ ] A replay type `(seed, Vec<Input>)` and a runner that reconstructs final state
      deterministically.
- [ ] Golden grid tests: replay a fixed run, assert the exact rendered text grid.
- [ ] A regression test that fails if the same seed+inputs produce a different result.
- [ ] Seed sharing: a level/run is identified and reproducible from its seed (string
      the web shell G7 can surface).
- [ ] fmt/clippy/test gate passes.

## Notes / risks
- Depends on enough of the loop/render existing to have a grid worth asserting
  (A2 + G1 minimum); land it once the loop renders.
- This is the substrate the headless sim (v2, §13.2) and the `playtest` skill sit
  on — keep the replay format minimal and stable.

---

## Not in v1 (recorded, not filed)

Per §14, deliberately **out of v1** and **not** being filed here — they're
scaffolding around the one question v1 exists to answer:

- **v2:** headless sim + metrics (§13.2, the `playtest` skill is blocked on it),
  saves, options, help/legend, game-over-with-reason screen, alert indicator.
- **v3:** the campaign — facility map (a real graph), salvaged-tech accumulation,
  intel as currency with sinks, alert-scaled difficulty, an ending.
- **Later / backlog (§14):** prison level, smoke screen, deployable drone, rewind,
  keys/locked doors, circuits/powered doors, in-level lore, ability upgrade trees,
  partial cover, tiles. Experiments, not commitments — file only on request.

Every `[START]` number above is a latent `type:tuning` ticket (§12 is the machinery
for changing them). Not filing the tuning surface up front — file the feature, note
the numbers, tune under playtest.

---

# Batch — player FOV interpretability (2026-07-18)

**Goal:** make the player's field of view readable at a glance — what the player
can see renders **lit and fully coloured**; what they cannot renders **darkened and
desaturated** (dim but legible, §11.5), with the FOV edge visible even across open
floor.

**State of the repo today:** the renderer lives in core behind one interface
(#14/#37), colours are category-driven (`category.rs`), and the web shell draws the
grid. But **no FOV exists anywhere** — the turn loop's sight phase is an explicit
stub (`state.rs:398`), guards are scripted walkers with no cones, and `render()`
paints every cell at full colour. The path is: compute the FOV (C1, never filed),
then render it (#16, filed), on top of the colour table's dim variants (#15, filed).

**One new issue to file** — C1 above, with these repo-current notes folded in:

### C1 — Shadowcast FOV: facing cone + 360° touching ring → #41
**Labels:** `area:vision` `type:feature` `size:M`
**Milestone:** v1

Body: as drafted in section C above, plus:

- The sight phase hook already exists in `State::step` (phase 2 of §4.2) — this
  ticket replaces the stub, storing the player's visible set in `State` where
  `render()` (#16) and the guard AI (D-series, unfiled) will read it.
- Guards today are scripted (`Guard::stationary`/`patrolling`, no state machine).
  Compute their cones with the same function (R10/arc2) so #16's danger overlay
  and the future D-series read the same data, but guard *reactions* to sight stay
  out of scope — that's D1.
- Wait already exists as an `Input`; wire it to arc 5 (360°) here since the FOV
  function is what interprets it.

**Already filed — the rest of this batch, in order:**

| Order | Issue | Role in the goal |
|---|---|---|
| 1 | **#41 (C1)** | Compute the visible set. Everything else reads it. |
| 2 | **#15** — colour category system | The "coloured" half: full-range palette **and the darkened/desaturated dim variants** the out-of-FOV state needs. Partially landed (#38 separated the categories); the dim variant table is the remaining piece. |
| 3 | **#16** — FOV rendering + danger overlay + floor dots | The ask itself: in-FOV full colour, out-of-FOV dark gray, floor dots so the boundary reads on open ground. The guard danger-overlay half can ship against C1's guard cones; the two-shades-of-red zoning waits for D3 (unfiled), as the issue already notes. |
| 4 | **#17** — fog and tile memory | Companion, not blocker: the three-state memory (never-seen / remembered / live) that makes what the dimming *means* honest. Can follow #16. |

No other new issues needed — #16/#17/#15 already cover the render side; filing
duplicates would split the discussion.

---

# Batch — mobile tap controls (2026-07-18)

**Goal:** make the game playable on a phone for playtesting — taps drive the turn
loop. Requested directly ("tapping left of the screen to go left, tapping centre
to wait"); no existing issue covers it — #18 explicitly scopes touch out, and
§11.6's touch note is **[OPEN]**.

**One new issue to file:**

### T1 — Web: tap-to-move touch controls (edge zones step, centre waits) → #43
**Labels:** `area:render` `type:feature` `size:S`
**Milestone:** v1

## Summary
To playtest on mobile the turn loop must be drivable by touch: tapping the left
of the screen steps west, right steps east, top steps north, bottom steps south,
and tapping the centre waits. This is the first, deliberately narrow slice of
touch (§11.6 [OPEN]) — movement + wait only, enough to play; abilities, menus and
the manifest stay out of scope.

## Design reference
§11.6 — input. Movement is the arrow set; `5`/`w` is Wait. **Touch is a real
target and was never finished** — the old version's half-built touch trapped
users in dialogs; "either build touch properly or don't ship the manifest"
**[OPEN]**. §15.8 Q8 is the same question. §4.1 — 4-directional movement, no
diagonals **[SETTLED]**, so four edge zones + a centre zone cover the whole
input surface. §12.2 — the shell stays thin: the tap→input mapping is a pure,
natively-testable function; the DOM listener only feeds it.

## Acceptance criteria
- [ ] Tapping (or clicking) the viewport steps the game: left/right/top/bottom
      zones map to west/east/north/south, the centre zone maps to Wait.
- [ ] The zone rule is a pure function `(point, viewport) → Input` in the web
      crate with native unit tests: the four edges, the centre box, corner taps
      resolved by dominant axis, and a degenerate viewport.
- [ ] A tap neither scrolls, zooms, nor selects on mobile (`touch-action` +
      `preventDefault`); keyboard input keeps working unchanged.
- [ ] fmt/clippy/test gate passes.

## Notes / risks
- Half-built touch is worse than none (§11.6). No manifest ships yet, so the old
  trap (dialogs unreachable by touch) can't bite — but do not add one here.
- Corner taps: no diagonals exist (§4.1), so ties resolve by dominant axis; an
  exact tie goes horizontal. Documented in the function, pinned by a test.
- Screen-relative zones (not canvas-relative): letterboxed margins around the
  canvas still count, matching "tap the left of the screen".
