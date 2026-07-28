# Vision

**Salvaged tech (§8.3)** — **passive**. *"Always on while held"* (§8.2): the sight
arc becomes the full 360° and the range box grows from 15 to 20 (§5/§6.1). No
activation, no turn, no cooldown — it costs the loadout slot and nothing else.
**Vision only**: the guard sense (§9) is a separate, innate channel and is
deliberately not widened with it, so a wait still buys something (§9.1).

## What the cue says

**Nothing, and that is the answer, not an oversight.** A passive has no activation to
cue: there is no key to press and no moment to press it in. `crates/sim/src/cue.rs`
states this in its own arm rather than leaving it to the exhaustive match's silence
(§8.2/#264), which is the same reason this page exists — so "no cue" reads as a
decision everywhere it could be read as a gap.

## What the sim measured

**Nothing here, and there is nothing this page's shape could measure.** The usage
histogram counts *presses*, so `usage.vision` is structurally zero for a passive and
always will be: a zero in that slot is not a false zero, it is the only honest number.

Vision is not therefore unmeasurable — it is just measured by a different question.
What it changes is what the bot **sees**, so its effect shows up in metrics like
detections and turns-to-win in a with/without pair, not in a usage count. That
measurement is #265's, which watches whether the ability erodes §5's "can't see
behind you" constraint too far. When it lands, its numbers belong on this page.

## History

- `6cce986+stats-family-347` (#347) — page created to state that a passive has no cue
  and why its histogram slot is honestly zero.
