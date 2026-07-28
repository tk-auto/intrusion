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

Measured at `7d5dc01+cues-347-remeasured`, on the post-crouch bot (#382). Each profile against
**its own** control, never against another's (§13.4).

| Metric | baseline | cautious | aggressive | careless |
|---|---|---|---|---|
| `win_rate` | 0.38 → **0.40** | 0.60 → **0.68** | 0.50 → **0.47** | 0.49 → **0.48** |
| `turns_to_win_median` | 117.5 → **118.5** | 183.5 → **196.0** | 111.0 → **102.0** | 116.0 → **104.0** |
| detections / turn | 0.0527 → **0.0520** | 0.0248 → **0.0231** | 0.0432 → **0.0466** | 0.0704 → **0.0672** |
| `diversity` | 0.6730 → **0.6323** | 0.4867 → **0.4519** | 0.6230 → **0.6286** | 0.5835 → **0.6177** |
| `timeouts` | 3 → **4** | 2 → **1** | 1 → **1** | 1 → **1** |
| `usage.autodoors` | **29** | **31** | **33** | **28** |
| `usage_share.autodoors` | **0.0017** | **0.0013** | **0.0027** | **0.0023** |

Disjoint block, `--seed 100 --runs 100`:

| Metric | baseline | cautious | aggressive | careless |
|---|---|---|---|---|
| `win_rate` | 0.44 → **0.42** | 0.63 → **0.61** | 0.37 → **0.38** | 0.46 → **0.43** |
| `turns_to_win_median` | 117.5 → **115.0** | 204.0 → **192.0** | 92.0 → **94.5** | 110.0 → **100.0** |
| detections / turn | 0.0411 → **0.0446** | 0.0233 → **0.0244** | 0.0683 → **0.0664** | 0.0590 → **0.0582** |
| `diversity` | 0.5723 → **0.6064** | 0.4394 → **0.4586** | 0.6146 → **0.6387** | 0.5455 → **0.5800** |
| `timeouts` | 1 → **1** | 1 → **2** | 1 → **1** | 0 → **0** |
| `usage.autodoors` | **23** | **35** | **31** | **37** |
| `usage_share.autodoors` | **0.0018** | **0.0015** | **0.0027** | **0.0033** |

## The reading

- **The slot fires, and remarkably evenly** — 23–37 presses across every
  temperament and both seed blocks, a tighter spread than Decoy's. The
  precondition is doing that: a door on the escape route is roughly as common
  whatever the temperament, because the facility's geometry decides it, not the
  bot's nerve.
- **Nothing moved.** Win rate is flat to ±0.03 in one block and flat to ±0.03 the
  other way in the other; detections per turn move in the third decimal. `cautious`
  at +0.08 win rate on the seed-0 block does **not** reproduce at seed 100 (0.63 →
  0.61), so it is block noise, not the ability.
- **The one consistent move is `diversity` under the striking temperaments**:
  `aggressive` and `careless` rise in both blocks (+0.006/+0.024 and
  +0.034/+0.035). Small, but it is the only delta that survived the second block in
  the same direction. Worth a look, not a conclusion.
- **Not dominant** — 0.13%–0.33% of spent turns, among the smallest shares of any
  cue here. Measured alone, though; the full-kit sweep is where dominance is really
  testable.
- **Coupled to §15 Q1.** Autodoors' value is entirely a chase-shape question (§7.6),
  which is the design's most important open one. When Q1 moves, this cue and these
  numbers should be expected to move with it — the coupling is noted rather than
  tuned around.

## History

- `4275e28+autodoors-cue-347` (#347) — cue written; slot went from a structural zero
  to 27–37 presses per 100-run batch. No headline metric moved beyond seed-block
  wobble; `diversity` rose slightly under both striking temperaments in both blocks.
- `7d5dc01+cues-347-remeasured` (#347) — **re-measured on the post-crouch bot**
  (#382 landed while this branch was open, changing the policy and refreshing the
  innate baseline, so every control column above moved). Unchanged in substance; the diversity rise under the striking temperaments survived.
