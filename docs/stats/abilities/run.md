# Run

**Innate (§8.3)** — 2 cells/turn while active. The escape everybody starts with, and
the reason being seen was once free: a guaranteed 2-against-1 against a hard cap of
1. With searching guards, radio calls and converging responders, that escape stops
being the end of the problem and becomes the start of one.

## What the cue says

`crates/sim/src/cue.rs`, the first cue the seam ever had (#366):

- **Fleeing only.** The bot is not racing anybody while pushing for a console, and
  the turn spent activating buys nothing a step would not.
- **A cell of room to spend the turn in.** Activating is a turn standing still
  (§4.4), which a guard already at arm's length turns into a capture.
- **Decisive** — the one turn that decides whether the chase is outrunnable at all
  (§7.6). It outranks every other cue, and it stands alone: no follow-through, since
  the extra cell rides on every step that follows for free.

## What the sim measured

**There is no with/without pair for Run, and there cannot be**: it is innate, so
`--without run` is refused at the flag (§8.3). Every batch in this directory holds
it, which makes the committed baseline itself Run's measurement — the control column
of every other page is a Run column.

From [`baseline.json`](../../../.claude/skills/playtest/baseline.json) at
`b0828ae+bot-takedowns-316`, `--runs 100 --seed 0 --cap 1000`:

| Metric | baseline | cautious | aggressive | careless |
|---|---|---|---|---|
| `usage.run` | 397 | 202 | 190 | 195 |
| `usage_share.run` | 0.0242 | 0.0091 | 0.0148 | 0.0161 |

## The reading

- **The temperaments show up in it.** `baseline` runs twice as often as `cautious`
  does, which is the point of the profiles: the careful one is seen less and so has
  less to run from. `aggressive` and `careless` are seen far more but run less than
  `baseline` — they tolerate a cone rather than spending a turn to break it.
- **Around 1–2% of spent turns**, against `wait` at 23% for the careful profiles.
  Nowhere near dominant, and its cue is the strictest in the module — decisive, but
  only in a situation the bot has to already be losing.
- **Unlike every other page here, these numbers are gated.** CI re-runs this batch on
  every PR and fails if it drifts, so Run's histogram slot cannot go stale unnoticed.

## History

- `6cce986+stats-family-347` (#347) — page created. Numbers are the committed
  baseline's, not a fresh measurement: Run is innate, so its control *is* the batch.
