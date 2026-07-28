# Camouflage

**Salvaged tech (§8.3)** — 1 turn to press, duration 10, cooldown 20. Undetectable
**while you don't move**. Moving reveals you for that turn. A hideout you carry.

## What the cue says

`crates/sim/src/cue.rs`, written with the seam itself (#366):

- **Only a way out of being found** — `Flee` or `TakeCover`. Pressing it while
  pushing for the objective would spend a turn and the whole cooldown on nothing.
- **Only when there is no cupboard to reach.** A real hideout beats the carried one:
  it costs no cooldown and does not pin the bot still, so the cue reads the refuge the
  policy has already found and stays silent when there is one.
- **Strong, not decisive** — it is the fallback for having nowhere to hide, and an
  escape that actually moves (Run) should outrank it on the same turn.
- **The press is only half of it.** A *moving* cloaked player is seen like any other,
  so the cue commits to holding still for the cloak's whole duration, and keeps saying
  so for as long as it lasts.

## What the sim measured

The cue shipped with #366 but had never been measured: `Loadout::innate()` holds Run
alone, so the slot read a structural zero until #256 made the loadout a batch input.

```
cargo run --release -p intrusion-sim -- --bot --profile <NAME> --runs 100 --seed 0 --cap 1000
cargo run --release -p intrusion-sim -- --bot --profile <NAME> --abilities camouflage --runs 100 --seed 0 --cap 1000
```

Measured at `7d5dc01+cues-347-remeasured`, on the post-crouch bot (#382). Each profile against
**its own** control, never against another's (§13.4).

| Metric | baseline | cautious | aggressive | careless |
|---|---|---|---|---|
| `win_rate` | 0.38 → **0.34** | 0.60 → **0.60** | 0.50 → **0.48** | 0.49 → **0.51** |
| `turns_to_win_median` | 117.5 → **113.0** | 183.5 → **172.0** | 111.0 → **113.5** | 116.0 → **106.0** |
| detections / turn | 0.0527 → **0.0536** | 0.0248 → **0.0245** | 0.0432 → **0.0368** | 0.0704 → **0.0406** |
| `diversity` | 0.6730 → **0.5906** | 0.4867 → **0.4059** | 0.6230 → **0.6037** | 0.5835 → **0.5591** |
| `timeouts` | 3 → **3** | 2 → **1** | 1 → **1** | 1 → **0** |
| `usage.camouflage` | **44** | **44** | **52** | **88** |
| `usage_share.camouflage` | **0.0029** | **0.0021** | **0.0045** | **0.0082** |

Disjoint block, `--seed 100 --runs 100`:

| Metric | baseline | cautious | aggressive | careless |
|---|---|---|---|---|
| `win_rate` | 0.44 → **0.42** | 0.63 → **0.61** | 0.37 → **0.36** | 0.46 → **0.47** |
| `turns_to_win_median` | 117.5 → **118.5** | 204.0 → **204.0** | 92.0 → **98.5** | 110.0 → **127.0** |
| detections / turn | 0.0411 → **0.0375** | 0.0233 → **0.0214** | 0.0683 → **0.0573** | 0.0590 → **0.0400** |
| `diversity` | 0.5723 → **0.5416** | 0.4394 → **0.3697** | 0.6146 → **0.6307** | 0.5455 → **0.6044** |
| `timeouts` | 1 → **2** | 1 → **2** | 1 → **4** | 0 → **0** |
| `usage.camouflage` | **43** | **63** | **50** | **91** |
| `usage_share.camouflage` | **0.0032** | **0.0025** | **0.0049** | **0.0087** |

## The reading

- **`careless` presses it twice as often as anyone** (88/91 against 43–63), in both
  blocks. That is the temperament read straight off the cue: `careless` never diverts
  to a cupboard, so its refuge is *always* `None`, and the cue that only speaks when
  there is no cupboard to reach speaks to it constantly. A carried hideout is worth
  most to the profile that refuses real ones.
- **Detections per turn fall sharply for the striking temperaments** — `aggressive`
  0.0432 → 0.0368 and `careless` 0.0704 → 0.0406 at seed 0, both reproduced at seed
  100 — and barely move for the careful ones, which were already hiding in cupboards.
- **Win rate does not follow.** `baseline` goes *down* in both blocks (−0.04, −0.02),
  the only ability here that does. Being seen less while winning no more is a
  plausible signature of the pin: the cue commits to holding still for the cloak's
  full ten turns, and ten turns of not moving is ten turns of not making progress.
  Worth a human's eye rather than a conclusion.
- **`diversity` falls under the careful temperaments** (`cautious` −0.08 and −0.07,
  `baseline` −0.08 and −0.03) and is mixed under the striking ones. Same direction as
  Confusion, opposite to Dephase.
- **Not dominant** — 0.21%–0.87% of spent turns.

## History

- `6cce986+stats-family-347` (#347) — first measurement of a cue that shipped with
  #366: the slot's zero was the sim never granting the tech, not a dead cue. 44–91
  presses per 100-run batch; detections down sharply for the striking temperaments,
  `baseline` win rate down in both blocks, `diversity` down for the careful ones.
- `7d5dc01+cues-347-remeasured` (#347) — **re-measured on the post-crouch bot**
  (#382 landed while this branch was open, changing the policy and refreshing the
  innate baseline, so every control column above moved). Presses and shares are essentially unchanged; the `diversity` fall under the careful temperaments is smaller than first measured but still in the same direction.
