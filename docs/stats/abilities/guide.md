# Guide

**Salvaged tech (§8.3/#505)** — **passive**. While held, one of the eight cells around
you is washed `Effect`: the one lying toward the nearest **unclaimed** intel console or
equipment cache. A **compass, not a route** — the bearing is straight-line and
wall-blind, the deliberate opposite of §7.3's *"nearest means the shortest walk"* — and
it reveals nothing else, so the objective's cell stays fogged until seen (§11.5a). No
activation, no turn, no cooldown: it pays with the loadout slot and nothing else.

## What the cue says

**Nothing, and it is the most interesting "nothing" in `cue.rs`.**

Vision and the Saver have no cue for the plain reason that a passive has no key to
press. That is true here too — but unlike them, the Guide hands the policy something it
*could* act on: a bearing. Acting on it is exactly what the bot may not do.

§11.5a's no-cheat gate is that the bot routes only to intel it has **seen**
(`Bot::known_intel`). A policy that walked a compass needle would be routing to a
console it has never laid eyes on, which is the one thing that gate exists to forbid —
so this ability cannot be cued without punching a hole in the rule that keeps the sim's
exploration numbers honest. The zero in `usage.guide` is therefore not a false zero and
not a shy cue; it is a passive, twice over.

## What the sim measured

`8ea5b7c+guide-505`, `--abilities guide` against the innate control, all four
temperaments, `--runs 100 --seed 0 --cap 1000`.

| | balanced | cautious | aggressive | careless |
|---|---|---|---|---|
| `win_rate` ctl → arm | 0.35 → 0.35 | 0.53 → 0.53 | 0.40 → 0.40 | 0.43 → 0.43 |
| detections | 848 → 848 | 722 → 722 | 728 → 728 | 1211 → 1211 |
| `turns_to_win` median | 137 → 137 | 235 → 235 | 136 → 136 | 142 → 142 |
| `diversity` | 0.603 → 0.603 | 0.341 → 0.341 | 0.574 → 0.574 | 0.497 → 0.497 |

**Identical, to the run.** Not "flat", not "within the wobble" — byte-identical, which
is the honest and *structurally guaranteed* result: the Guide's entire effect is a
`Category::Effect` background on one cell, the bot reads the game through `State` and
not through the rendered grid, and nothing in its policy consults `guide_bearing`. A
run holding it plays exactly the run that does not.

**So this table is a control, not a measurement.** Its value is negative and worth
having: it says the ability is *inert to the sim*, so no number anywhere else in §13.2
moves because somebody was dealt a Guide, and the baseline needs no refresh for it. The
with/without pair the ticket asks for cannot be run against this bot at all.

### The question that actually matters, and who can answer it

§11.5a's fog is a **[SETTLED]** pillar and finding the objectives is a large part of
what a run *is*. The real risk is not that the Guide is weak — it is that a bearing plus
the always-visible geometry is enough to walk more or less straight to every console,
which would delete the exploration reward the fog exists to create. The two numbers that
would show it are **turns-to-first-console** and **total cells explored**, with and
without.

Neither is measurable here, for the reason above, and neither would mean much if it
were: the bot already has perfect recall of everything it has seen and sweeps
farthest-first, so it is a poor model of the player whose exploration this might delete.
**This is a human playtest question**, and it is the one to put to a player holding it:
*did you still look around?*

If the answer is no, §8.3 names the lever — a **range cap**, so the compass only wakes
within N cells and is a local tool rather than a global one — and it is deliberately not
a nerf to the wash, which is already the weakest cue on the board.

### One thing the sim did catch

The Guide is the **first ability that paints the board purely by being held**, and it
broke a pre-existing generation test (`the_loadout_draw_never_perturbs_the_facility`)
that compared two boots frame-for-frame on the assumption that a loadout "shapes the
ability line, not the map". It does now. The test was narrowed to compare glyphs rather
than whole frames — the layer a *draw* must never touch, as opposed to the one an
ability is allowed to — which is the correct reading of what it was always asserting.

Worth knowing before a second always-on effect lands: the effect layer latches marks
from the turn's **events**, and a passive held from level start has no event to latch
from, so its cell is a live read (`effect_cell_marks` chains `guide_bearing`) on the
same footing as the two marks that blink.

## History

- `8ea5b7c+guide-505` (#505) — page created with the ability. Batches identical to the
  control by construction; the exploration question is handed to a human playtest.
