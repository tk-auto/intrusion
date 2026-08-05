# False Call

**Salvaged tech (§8.3/#504)** — 1 turn, instant, 30 cooldown. A **radio spoofer**:
firing transmits a forged control message naming **the cell you fired it from**, and
every guard within `FALSE_CALL_RADIUS` (**8** **[START]**) of it — through walls, and
**not** clamped to the guard sense, because it is a radio — converges on that cell and
**searches** it (§7.6). It adds no second verb to §7.7: it hands the player the one
that already exists, through the same `send_call` seam control's dispatch and both
call-ins run through. The called cell is a **snapshot**, so the play is *"call them
here, then be somewhere else"*. It **dies with the comms console** (§7.3): a silenced
net has nothing listening, and the press is refused for free.

## What the cue says

**Fire only where the call is a vacuum, not a trap** (`crates/sim/src/cue.rs`). Four
conditions, and the first three are the ability's §8.3 sentence written as a predicate:

1. **Intent is `Pursue` or `Explore`.** It is the only cue that declines in `Flee`
   outright — what this does is give guards a reason to come to *you*, which is the
   opposite of breaking contact.
2. **Every guard the call would reach is at least `FALSE_CALL_CLEARANCE` (8) away**,
   measured against the *called* set and not against `nearest_guard`: the call reaches
   past what the bot perceives, so a check on what it can see would miss exactly the
   guards it is summoning.
3. **The next route step opens the gap on every one of them** — only ever call them to
   a cell you are already walking away from.
4. **There is ground worth emptying**: some cell in the next `FALSE_CALL_SCOUT` (6)
   along the route is **watched** (§11.5's detection set).

Condition 4 exists because of a Catch-22 the first version walked into, and it is worth
recording: the router already prices watched cells out (`Profile::watched_penalty`), so
the step it hands over is almost never itself watched. Keyed on that single cell the cue
fired **once in 100 runs**. Looking a little way *down* the route — at the ground the bot
is heading for rather than the cell it has already decided to stand on — is what makes
the question the honest one.

## What the sim measured

`8ea5b7c+false-call-504`, `--abilities false-call` against the innate control, all four
temperaments, both seed blocks, `--runs 100 --cap 1000`.

| | balanced | cautious | aggressive | careless |
|---|---|---|---|---|
| `win_rate` ctl → arm (seed 0) | 0.35 → **0.34** | 0.53 → **0.53** | 0.40 → **0.38** | 0.43 → **0.44** |
| `win_rate` ctl → arm (seed 100) | 0.30 → **0.31** | 0.51 → **0.50** | 0.37 → **0.37** | 0.37 → **0.36** |
| detections ctl → arm (seed 0) | 848 → 845 | 722 → 709 | 728 → **681** | 1211 → 1196 |
| `diversity` ctl → arm (seed 0) | 0.603 → 0.617 | 0.341 → 0.341 | 0.574 → 0.588 | 0.497 → 0.528 |
| `false_call` presses / 100 runs | 11 | 2 | 10 | 6 |

**Flat, and flat on both seed blocks.** Nothing here is worth believing as an effect:
every win-rate delta is inside the wobble a 100-seed batch has on its own, and the one
that looks largest (aggressive, −0.02 with detections down 6%) does not reproduce at
`--seed 100`, where arm and control are identical to the run. Diversity is up in three
of four, by 0.01–0.03 — the smallest kind of "up", and the only column pointing
consistently anywhere.

**It is not dominant, and it cannot be from these numbers**: 11 presses across 100 runs
is under 0.1% of spent turns, two orders of magnitude below the ~50% watermark §13.2
watches for. What the batch says is that the verb is *reachable* and *harmless*, not
that it is good.

### The measurement that actually found something

**A looser cue turns it into a suicide button, and the sim says so loudly.** Before
conditions 2–4 were tightened, the cue asked only for a route and 4 cells of clearance
from the nearest *perceived* guard. That version pressed ~1.5 times per run and took the
balanced win rate from **0.35 to 0.05** — 94 captures in 100 runs, a degenerate outcome
profile:

| cue | radius | presses / 100 runs | balanced `win_rate` |
|---|---|---|---|
| clearance 4, perceived-guard check | 14 | 188 | **0.04** |
| clearance 8, called-set check | 14 | 147 | **0.05** |
| clearance 8, called-set check | 8 | 167 | **0.09** |
| + walking-away + watched-ground (shipped) | 8 | 11 | **0.34** |

This is the ticket's own first-named risk — *"the obvious failure mode is that it is a
suicide button"* — reproduced as data, and it is the most useful thing this page has.
The mechanism is legible in the replays: the responders arrive at the fired cell and a
guard **standing where you were** can see 15 cells (§6.1), so a player who walks in a
straight line afterwards is inside the cone of the search they called. The ability wants
a **break in line of sight** after the press — a corner, a cupboard, a duct — and the
turns spent buying one are its real cost.

The radius row is worth reading on its own: at **14** a firing covers a 29×29 box on a
40×40 board (§10.2) and summons most of the facility onto the player. That is what
settled the **[START]** at 8.

### What this page cannot tell you

The shipped cue fires 2–11 times per 100 runs, which is *shy*, and §13.3's standing
caveat applies at full strength: a near-zero slot means "weak ability **or** shy cue"
and no batch here separates them. The instrument that could is a sweep of
`Profile::cue_floor` — but it will not help much here, because what gates this cue is
its **predicate**, not its urge (the bids are `PLAIN`/`STRONG`, above the default floor
already). The honest reading is that the bot has no *follow-through* for this verb: the
play is fire → break line of sight → take the emptied ground, and the policy has the
first step and not the other two. That is the same shape of gap `drone.md` records, one
notch less severe — the press exists and is legal, and only the plan around it is
missing.

**Two knobs to sweep when someone gives the bot that plan**: the radius (8, and 14 is
known to be far too much) and the 30-turn lockout, which is the pair §13.2 should take
first — the ticket's second risk is that pulling a wing off its patrol trivialises
routing, and no measurement here has been able to test that, because the bot never
pulls a wing.

## History

- `8ea5b7c+false-call-504` (#504) — page created with the ability. Flat against the
  innate control on both seed blocks; the real finding is the cue sweep above, where a
  looser cue quarters the win rate.
