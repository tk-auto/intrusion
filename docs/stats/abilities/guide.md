# Guide

**Salvaged tech (§8.3/#505)** — **passive**. While held, one of the eight cells around
you is washed `Effect`: the one lying toward the nearest **unclaimed** intel console or
equipment cache — **on one turn in `GUIDE_BLINK_TURNS` (3)**, and never on turn zero. A
**compass, not a route** — the bearing is straight-line and wall-blind, the deliberate
opposite of §7.3's *"nearest means the shortest walk"* — and it reveals nothing else, so
the objective's cell stays fogged until seen (§11.5a). No activation, no turn, no
cooldown: it pays with the loadout slot and nothing else.

**It pulses rather than stands**, which is the ability's main balance lever: a standing
needle is a line you follow without thinking, where a pulse gives you a *fix* you then
walk on your own memory of for two turns. The run opens dark for the same reason — a
compass already pointing on the frame you arrive would make the opening move free.

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

`c1f4a2e+guide-505`, `--abilities guide` against the innate control, all four
temperaments, `--runs 100 --seed 0 --cap 1000`. Re-run after the pulse landed, with the
same result — as it must be.

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

The **pulse** is the first answer to that risk, applied before anyone played it: a
bearing you get one turn in three is a fix to hold and walk on, not a line to follow, so
the route between fixes is still yours to work out. If a playtest says it is still too
much, `GUIDE_BLINK_TURNS` is the obvious next knob and §8.3 names a second — a **range
cap**, so the compass only wakes within N cells and is a local tool rather than a global
one. Neither is a nerf to the wash, which is already the weakest cue on the board.

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
- `c1f4a2e+guide-505` (#505) — the bearing made to **pulse** (one turn in three, dark on
  turn zero), pre-empting the exploration risk this page could not measure. Batches
  identical again, for the same structural reason.
