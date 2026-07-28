# Lockdown

**Salvaged tech (§8.3/#242)** — 1 turn to press, duration 8, cooldown 40. While
active, every door within `LOCKDOWN_RADIUS` (4) of **where you fired it** is shut and
sealed: a guard cannot work the handle, so its route goes the long way round
(§7.6/§10.4). A **snapshot**, not a travelling bubble. **You** are never refused — a
sealed door bumps open for you like any closed one, which is what stops a lockdown
boxing in its owner, *"and that costs the turn and leaves the door open, so a
lockdown fired across a route you still have to travel is a real mistake"*. Every
seal is released when the window ends. A lockdown with no door in reach is refused
for free (§4.4).

## What the cue says

`crates/sim/src/cue.rs`:

- **Flight only** — `Flee`. Route denial only means something against somebody
  following a route *to you*; a sealed door costs a patrol nothing it was not already
  walking past.
- **A cell of room to spend the turn in**, the same gap Run and Autodoors insist on.
- **Never across a route the bot still has to travel** — the §8.3 warning, in the two
  forms the cue can see: it will not fire while *standing in a doorway* (sealing the
  cell you are halfway through), and not when the step the plan would take leads into
  a door. Unmaking your own seal is paid in the very turns the ability was bought to
  save.
- **The size of the box grades the bid** — **strong** for more than one door, **plain**
  for exactly one. One door is a detour; a knot of them is a wall. Legality stays
  core's: a firing with no door in reach is `Unusable`.

## What the sim measured

```
cargo run --release -p intrusion-sim -- --bot --profile <NAME> --runs 100 --seed 0 --cap 1000
cargo run --release -p intrusion-sim -- --bot --profile <NAME> --abilities lockdown --runs 100 --seed 0 --cap 1000
```

Measured at `7d5dc01+cues-347-remeasured`, on the post-crouch bot (#382). Each profile against
**its own** control, never against another's (§13.4).

| Metric | baseline | cautious | aggressive | careless |
|---|---|---|---|---|
| `win_rate` | 0.38 → **0.36** | 0.60 → **0.59** | 0.50 → **0.49** | 0.49 → **0.46** |
| `turns_to_win_median` | 117.5 → **115.0** | 183.5 → **172.0** | 111.0 → **111.0** | 116.0 → **101.0** |
| detections / turn | 0.0527 → **0.0501** | 0.0248 → **0.0256** | 0.0432 → **0.0476** | 0.0704 → **0.0484** |
| `diversity` | 0.6730 → **0.6098** | 0.4867 → **0.4436** | 0.6230 → **0.5275** | 0.5835 → **0.5431** |
| `timeouts` | 3 → **2** | 2 → **1** | 1 → **0** | 1 → **0** |
| `usage.lockdown` | **102** | **128** | **89** | **90** |
| `usage_share.lockdown` | **0.0072** | **0.0056** | **0.0082** | **0.0086** |

Disjoint block, `--seed 100 --runs 100`:

| Metric | baseline | cautious | aggressive | careless |
|---|---|---|---|---|
| `win_rate` | 0.44 → **0.36** | 0.63 → **0.61** | 0.37 → **0.41** | 0.46 → **0.49** |
| `turns_to_win_median` | 117.5 → **115.5** | 204.0 → **195.0** | 92.0 → **97.0** | 110.0 → **117.0** |
| detections / turn | 0.0411 → **0.0441** | 0.0233 → **0.0246** | 0.0683 → **0.0619** | 0.0590 → **0.0454** |
| `diversity` | 0.5723 → **0.5876** | 0.4394 → **0.3805** | 0.6146 → **0.5440** | 0.5455 → **0.5102** |
| `timeouts` | 1 → **1** | 1 → **2** | 1 → **2** | 0 → **0** |
| `usage.lockdown` | **104** | **139** | **88** | **99** |
| `usage_share.lockdown` | **0.0078** | **0.0061** | **0.0083** | **0.0092** |

## The reading

- **`diversity` falls in seven of eight profile-blocks** (−0.03 … −0.10) — the third
  ability to move the boredom metric down, alongside Confusion and Camouflage, and
  against Dephase and Pierce Wall which move it up. The pattern across all seven cued
  verbs is now hard to miss: the abilities that **answer a hunt** make runs more
  alike, and the abilities that **change the geometry** make them less alike.
- **Win rate leans down** — five of eight profile-blocks, and `baseline` loses 0.02 and
  0.08. It is the only cue here whose win rate leans negative on both blocks, and the
  §8.3 row predicts exactly that: your own lock is never refused but it *is* paid for,
  a turn at a time, and a bot that seals the ground it is fleeing across will meet its
  own handiwork. Whether this cue is spending the ability badly, or the ability is
  genuinely double-edged, is precisely the shy-cue ambiguity #349's floor sweep exists
  to settle.
- **Wins get faster when they happen** (`baseline` median 117.5 → 115, `careless` 116 →
  101 at seed 0) and every temperament times out less at seed 0. Fewer wins, quicker
  ones — a real §13.3 flag rather than a verdict.
- **Not dominant** — 0.56%–0.92% of spent turns.
- **Coupled to §15 Q1.** Route denial during a chase *is* the chase question, more
  completely than for any other verb: Autodoors and Confusion are coupled to Q1, this
  one is made of it. Expect these numbers to move when Q1 does, and read them as the
  most provisional on the shelf.

## History

- `6cce986+stats-family-347` (#347) — page created; cue deliberately deferred, with
  the §15 Q1 coupling as the reason.
- `3d9f690+lockdown-cue-347` (#347) — **deferral reversed on request**, cue written;
  the slot went from a structural zero to 85–143 presses per 100-run batch. Writing
  the test caught the cue firing from *inside* a doorway — sealing the one cell the
  bot was halfway through — which the cue now refuses. `diversity` down in all eight
  profile-blocks, win rate leaning down, wins faster when they come.
- `7d5dc01+cues-347-remeasured` (#347) — **re-measured on the post-crouch bot**
  (#382 landed while this branch was open, changing the policy and refreshing the
  innate baseline, so every control column above moved). Slightly softened: `diversity` now falls in seven of eight profile-blocks rather than all eight, and the win-rate lean is five of eight.
