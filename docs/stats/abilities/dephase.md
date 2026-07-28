# Dephase

**Salvaged tech (§8.3)** — 1 turn to press, duration 3, cooldown 30. Fill → 0:
*"walk through walls, doors, guards. **Does not conceal you.**"* A duration that
expires inside a solid costs a safety eject plus a stun as long as the throw it had
to make.

## What the cue says

`crates/sim/src/cue.rs`:

- **Never an escape.** Silent in `Flee` and `TakeCover`. A phased player is as
  visible as any other, so pressing it while hunted spends a turn and changes
  nothing about being seen — and walking into a wall to hide is exactly how the
  safety eject is found.
- **A short, known crossing, or nothing.** The cue wants one cell of solid with a
  routable far side the bot has *actually seen* (§11.5a), where crossing saves at
  least `CROSSING_MARGIN` (6 **[START]**) of the router's own cost — twice the three
  turns the crossing spends. "There is a wall here" is explicitly not a reason.
- **One cell, never two.** In at turn 1, out at turn 2, with a turn of the 3-turn
  duration in hand. A two-cell run would land on expiry, which is the trap.
- **Never decisive.** No crossing is worth losing the run over — that is the
  difference between a shortcut and an escape.

The crossing is computed by the *policy*, on the same field it routes with, and
handed to the cue. That is not tidiness: the router cannot plan a path through a
wall, so pressing Dephase for a crossing the policy would then decline to walk would
be the shy cue in its most literal form. The same function drives the follow-through
— step into the solid while phased — and **leaving the wall outranks every other
plan**, ahead of fleeing, because the eject is worse than being seen.

## What the sim measured

```
cargo run --release -p intrusion-sim -- --bot --profile <NAME> --runs 100 --seed 0 --cap 1000
cargo run --release -p intrusion-sim -- --bot --profile <NAME> --abilities dephase --runs 100 --seed 0 --cap 1000
```

Measured at `5d8c079+dephase-cue-347`.

| Metric | baseline | cautious | aggressive | careless |
|---|---|---|---|---|
| `win_rate` | 0.38 → **0.40** | 0.60 → **0.68** | 0.49 → **0.53** | 0.49 → **0.52** |
| `turns_to_win_median` | 117.0 → **111.5** | 183.5 → **192.0** | 111.0 → **122.0** | 116.0 → **122.0** |
| detections / turn | 0.0527 → **0.0475** | 0.0246 → **0.0275** | 0.0605 → **0.0393** | 0.0704 → **0.0526** |
| `diversity` | 0.6918 → **0.7080** | 0.5399 → **0.5754** | 0.6187 → **0.7130** | 0.5835 → **0.7182** |
| `timeouts` | 3 → **4** | 2 → **1** | 2 → **1** | 1 → **0** |
| `entombed` | 0 → **0** | 0 → **0** | 0 → **0** | 0 → **0** |
| `usage.dephase` | **101** | **169** | **63** | **62** |
| `usage_share.dephase` | **0.0058** | **0.0073** | **0.0054** | **0.0053** |

Disjoint block, `--seed 100 --runs 100`:

| Metric | baseline | cautious | aggressive | careless |
|---|---|---|---|---|
| `win_rate` | 0.44 → **0.43** | 0.62 → **0.68** | 0.40 → **0.38** | 0.46 → **0.42** |
| detections / turn | 0.0406 → **0.0366** | 0.0239 → **0.0265** | 0.0660 → **0.0576** | 0.0590 → **0.0622** |
| `diversity` | 0.6093 → **0.6699** | 0.4413 → **0.4665** | 0.6043 → **0.7014** | 0.5455 → **0.6379** |
| `usage.dephase` | **85** | **199** | **56** | **64** |
| `usage_share.dephase` | **0.0065** | **0.0077** | **0.0057** | **0.0057** |

## The reading

- **`diversity` rises in all eight profile-blocks** — the only metric any cue has
  moved consistently in the same direction across both seed blocks, and here it is
  the largest effect too (`careless` +0.135 and +0.092, `aggressive` +0.094 and
  +0.097). §13.2 reads diversity as the boredom metric, so a shortcut that varies by
  facility geometry making runs *less* alike is exactly the shape you would hope for.
  It is also the mirror image of Confusion, which moved the same number the other
  way — a useful contrast, and the strongest argument in this ticket that the metric
  is measuring something real rather than the bot's mood.
- **Win rate is not a finding.** Up in all four at seed 0, down in three of four at
  seed 100. Block noise.
- **`cautious` presses it two to three times as often** as the striking temperaments
  (169/199 against 56–64), consistently across both blocks. That reads as the
  temperament: it walks longer, more careful routes, so more crossings clear the
  saving margin.
- **The eject never fired.** `entombed` is 0 in every arm, and the invariant test
  over 40 seeds × 4 profiles asserts the sharper form — the player is never stunned,
  and the phase eject is the only thing in the game that stuns.
- **Not dominant** — 0.53%–0.77% of spent turns.

## History

- `5d8c079+dephase-cue-347` (#347) — cue written; slot went from a structural zero to
  56–199 presses per 100-run batch. `diversity` up in all eight profile-blocks, win
  rate not reliably moved, no ejects.
