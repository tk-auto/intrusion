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
  **walkable** far side the bot has *actually seen* (§11.5a), where crossing saves at
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

Measured at `7d5dc01+cues-347-remeasured`, on the post-crouch bot (#382). Each profile against
**its own** control, never against another's (§13.4).

| Metric | balanced | cautious | aggressive | careless |
|---|---|---|---|---|
| `win_rate` | 0.38 → **0.38** | 0.60 → **0.64** | 0.50 → **0.52** | 0.49 → **0.52** |
| `turns_to_win_median` | 117.5 → **104.0** | 183.5 → **192.0** | 111.0 → **119.0** | 116.0 → **127.0** |
| detections / turn | 0.0527 → **0.0477** | 0.0248 → **0.0286** | 0.0432 → **0.0398** | 0.0704 → **0.0519** |
| `diversity` | 0.6730 → **0.7083** | 0.4867 → **0.5453** | 0.6230 → **0.7152** | 0.5835 → **0.7164** |
| `timeouts` | 3 → **4** | 2 → **1** | 1 → **1** | 1 → **0** |
| `entombed` | 0 → **0** | 0 → **0** | 0 → **0** | 0 → **0** |
| `usage.dephase` | **88** | **159** | **59** | **55** |
| `usage_share.dephase` | **0.0052** | **0.0070** | **0.0051** | **0.0047** |

Disjoint block, `--seed 100 --runs 100`:

| Metric | balanced | cautious | aggressive | careless |
|---|---|---|---|---|
| `win_rate` | 0.44 → **0.43** | 0.63 → **0.63** | 0.37 → **0.39** | 0.46 → **0.43** |
| `turns_to_win_median` | 117.5 → **113.0** | 204.0 → **226.0** | 92.0 → **103.0** | 110.0 → **116.0** |
| detections / turn | 0.0411 → **0.0393** | 0.0233 → **0.0262** | 0.0683 → **0.0586** | 0.0590 → **0.0611** |
| `diversity` | 0.5723 → **0.6438** | 0.4394 → **0.4603** | 0.6146 → **0.6865** | 0.5455 → **0.6160** |
| `timeouts` | 1 → **1** | 1 → **1** | 1 → **0** | 0 → **0** |
| `entombed` | 0 → **0** | 0 → **0** | 0 → **0** | 0 → **0** |
| `usage.dephase` | **82** | **172** | **52** | **58** |
| `usage_share.dephase` | **0.0063** | **0.0068** | **0.0047** | **0.0052** |

## The reading

- **`diversity` rises in all eight profile-blocks** — the only metric any cue has
  moved consistently in the same direction across both seed blocks, and here it is
  the largest effect too (`careless` +0.133 and +0.071, `aggressive` +0.092 and
  +0.072). §13.2 reads diversity as the boredom metric, so a shortcut that varies by
  facility geometry making runs *less* alike is exactly the shape you would hope for.
  It is also the mirror image of Confusion, which moved the same number the other
  way — a useful contrast, and the strongest argument in this ticket that the metric
  is measuring something real rather than the bot's mood.
- **Win rate is not a finding.** Up in all four at seed 0, down in three of four at
  seed 100. Block noise.
- **`cautious` presses it two to three times as often** as the striking temperaments
  (159/172 against 52–59), consistently across both blocks. That reads as the
  temperament: it walks longer, more careful routes, so more crossings clear the
  saving margin.
- **The eject never fired.** `entombed` is 0 in every arm, and the invariant test
  over 40 seeds × 4 profiles asserts the sharper form — the player is never stunned,
  and the phase eject is the only thing in the game that stuns.
- **Not dominant** — 0.47%–0.70% of spent turns.

## History

- `5d8c079+dephase-cue-347` (#347) — cue written; slot went from a structural zero to
  56–199 presses per 100-run batch. `diversity` up in all eight profile-blocks, win
  rate not reliably moved, no ejects.
- `a010a3b+pierce-wall-cue-347` (#347) — **correction, numbers above re-measured.**
  Pierce Wall's own test caught the shared crossing accepting a far side that is
  *wanted* but not *walkable*: the router seeds its goals whether or not they can be
  stood on, and a console is solid (§4.3), so a wall backing one looked like a
  crossing. Dephase would have phased into it and been ejected on expiry — the eject
  simply had not come up in these seeds. The crossing now requires walkable floor.
  Presses fell by a few per batch (161 from 169 under `cautious`); the diversity
  finding is unchanged.
- `7d5dc01+cues-347-remeasured` (#347) — **re-measured on the post-crouch bot**
  (#382 landed while this branch was open, changing the policy and refreshing the
  innate baseline, so every control column above moved). The finding survived intact — `diversity` still up in all eight profile-blocks.
