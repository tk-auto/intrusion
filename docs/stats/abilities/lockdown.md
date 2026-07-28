# Lockdown

**Salvaged tech (§8.3/#242)** — 1 turn to press, duration 8, cooldown 40. While
active, every door within `LOCKDOWN_RADIUS` of **where you fired it** is shut and
sealed: a guard cannot work the handle, so its route goes the long way round
(§7.6/§10.4). A **snapshot**, not a travelling bubble. **You** are never refused — a
sealed door bumps open for you like any closed one, which is what stops a lockdown
boxing in its owner, though that costs the turn and leaves the door open. Every seal
is released when the window ends. A lockdown with no door in reach is refused for
free (§4.4).

## What the cue says

**Nothing yet — deferred, not omitted.**

`crates/sim/src/cue.rs` returns no bid for Lockdown, with the reason stated in the
arm itself. #347 wrote a cue for every other activated verb and stopped here on
purpose:

- **Its entire value is route denial during a chase** (§7.6) — sealing the doors
  behind you so a pursuer's route goes the long way round.
- **The shape of the chase is §15 Q1**, the design's most important open question.
  Autodoors and Confusion are already coupled to it and their cues will need
  revisiting when it moves; Lockdown is not merely coupled to it, it is *entirely*
  about it.
- So a cue written now would be measuring an answer that has not been given. The
  slot reads an honest zero instead, and the histogram's zero means "nobody has
  written the cue", which is exactly what this page is for.

Two things the cue will have to get right when it is written, both from the §8.3 row:
a lockdown fired across a route **you still have to travel** is a real mistake, paid
for in the very turns the ability was bought to save; and the seal is a snapshot of
where you fired it, so a cue that thinks of it as a bubble following the player will
be wrong in a way the metrics will not obviously show.

## What the sim measured

`usage.lockdown` is **0** in every batch, and that zero is truthful: no policy tries.
It is the same failure class as #260 and #316 — a metric reading zero because nothing
exercises it — and it stays that way deliberately until the cue exists.

## History

- `6cce986+stats-family-347` (#347) — page created; cue deliberately deferred, with
  the §15 Q1 coupling as the reason.
