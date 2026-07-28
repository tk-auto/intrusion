# Autodoors

**Salvaged tech (§8.3)** — 1 turn to press, duration 16, cooldown 40. A door in
your path *"opens as you step into it — no bump, no lost turn — and shuts behind
you once you clear it"*, manual and automatic alike. A door closed behind breaks
line of sight (§10.3) and forces a pursuer to reopen it (§10.4): *"a §7.6 flight
tool, not invincibility"*.

## What the cue says

`crates/sim/src/cue.rs`:

- **Flight only** — `Flee`, nothing else. Everything the ability gives is about what
  happens *behind* you, so pressing it on the way to a console buys a 40-turn
  cooldown and a door nobody is chasing you through.
- **A cell of room to spend the turn.** Activating is a turn standing still (§4.4),
  which a guard already at arm's length turns into a capture — the same gap Run
  insists on.
- **A door has to be on the way out.** The cue reads the step the policy would
  otherwise take and requires it to lead into a door panel, open or closed. Without
  that, the window burns down on open floor and shuts nothing. The door must also be
  one the bot has *seen* (§11.5a) — memory, not the map.

Urge: **strong**, not decisive. Opening a gap outright (Run) is the better turn when
both are to hand; this is the trick you play once the gap is open. No follow-through
— the door only shuts once the bot walks through it, so the cue wants the next turns
spent moving.

## What the sim measured

```
cargo run --release -p intrusion-sim -- --bot --profile <NAME> --runs 100 --seed 0 --cap 1000
cargo run --release -p intrusion-sim -- --bot --profile <NAME> --abilities autodoors --runs 100 --seed 0 --cap 1000
```

Measured at `4275e28+autodoors-cue-347`. Each profile against its own control.

| Metric | baseline | cautious | aggressive | careless |
|---|---|---|---|---|
| `win_rate` | 0.38 → **0.39** | 0.60 → **0.66** | 0.49 → **0.46** | 0.49 → **0.48** |
| `turns_to_win_median` | 117.0 → **117.0** | 183.5 → **193.5** | 111.0 → **101.0** | 116.0 → **104.0** |
| detections / turn | 0.0527 → **0.0522** | 0.0246 → **0.0229** | 0.0605 → **0.0633** | 0.0704 → **0.0672** |
| `diversity` | 0.6918 → **0.6525** | 0.5399 → **0.5064** | 0.6187 → **0.6262** | 0.5835 → **0.6177** |
| `timeouts` | 3 → **4** | 2 → **1** | 2 → **2** | 1 → **1** |
| `usage.autodoors` | **27** | **29** | **33** | **28** |
| `usage_share.autodoors` | **0.0016** | **0.0012** | **0.0025** | **0.0023** |

Disjoint block, `--seed 100 --runs 100`:

| Metric | baseline | cautious | aggressive | careless |
|---|---|---|---|---|
| `win_rate` | 0.44 → **0.44** | 0.62 → **0.62** | 0.40 → **0.38** | 0.46 → **0.43** |
| detections / turn | 0.0406 → **0.0425** | 0.0239 → **0.0237** | 0.0660 → **0.0639** | 0.0590 → **0.0582** |
| `diversity` | 0.6093 → **0.6161** | 0.4413 → **0.4564** | 0.6043 → **0.6372** | 0.5455 → **0.5800** |
| `usage.autodoors` | **28** | **35** | **34** | **37** |
| `usage_share.autodoors` | **0.0020** | **0.0014** | **0.0030** | **0.0033** |

## The reading

- **The slot fires, and remarkably evenly** — 27–37 presses across every
  temperament and both seed blocks, a tighter spread than Decoy's. The
  precondition is doing that: a door on the escape route is roughly as common
  whatever the temperament, because the facility's geometry decides it, not the
  bot's nerve.
- **Nothing moved.** Win rate is flat to ±0.03 in one block and flat to ±0.03 the
  other way in the other; detections per turn move in the third decimal. `cautious`
  at +0.06 win rate on the seed-0 block does **not** reproduce at seed 100 (0.62 →
  0.62), so it is block noise, not the ability.
- **The one consistent move is `diversity` under the striking temperaments**:
  `aggressive` and `careless` rise in both blocks (+0.008/+0.033 and
  +0.034/+0.035). Small, but it is the only delta that survived the second block in
  the same direction. Worth a look, not a conclusion.
- **Not dominant** — 0.12%–0.33% of spent turns, the smallest share of any cue so
  far. Measured alone, though; the full-kit sweep is where dominance is really
  testable.
- **Coupled to §15 Q1.** Autodoors' value is entirely a chase-shape question (§7.6),
  which is the design's most important open one. When Q1 moves, this cue and these
  numbers should be expected to move with it — the coupling is noted rather than
  tuned around.

## History

- `4275e28+autodoors-cue-347` (#347) — cue written; slot went from a structural zero
  to 27–37 presses per 100-run batch. No headline metric moved beyond seed-block
  wobble; `diversity` rose slightly under both striking temperaments in both blocks.
