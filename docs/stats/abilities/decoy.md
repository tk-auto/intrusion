# Decoy

**Salvaged tech (§8.3)** — 1 turn to press, duration 20, cooldown 30. *"A fake
intruder in the cell you face. Draws Investigating, not Chasing. Dies when anything
steps on it."*

## What the cue says

`crates/sim/src/cue.rs`, justified from the row above rather than from what makes
the bot win:

- **Only while somebody is looking** — `Flee` or `TakeCover`. Pressing it on the
  way to a console spends the fake and its whole cooldown on a facility that is not
  searching for anybody.
- **Never at a guard that has you.** If any guard's cone is live on the player this
  turn, no bid — the §8.3 rule *"draws Investigating, not Chasing"*. A guard that
  has you is already coming to the real intruder, and #347 calls bidding here a cue
  bug rather than a tuning question.
- **Somebody must actually be searching** — a perceived guard in `Alerted`,
  `Investigating` or `Responding`, the three states of a guard hunting something it
  cannot see (§7.6/§7.3). A Calm patrol is not searching, so there is nothing to
  redirect.
- **Never onto its own escape.** The fake stands in the cell faced, and the bot
  faces the way it last stepped, so pressing while still heading that way would
  plant the decoy on the very route it is about to walk — drawing the search *onto*
  the escape. The cue reads the step the policy would otherwise take and declines
  when the two agree.

Urge: **strong** while breaking contact, **plain** when a patrol is merely closing
(a cupboard is usually the better turn there). No follow-through — a decoy is worth
pressing precisely so the bot can leave while somebody else walks toward it.

## What the sim measured

Control is the committed innate baseline block; the arm adds Decoy and changes
nothing else.

```
cargo run --release -p intrusion-sim -- --bot --profile <NAME> --runs 100 --seed 0 --cap 1000
cargo run --release -p intrusion-sim -- --bot --profile <NAME> --abilities decoy --runs 100 --seed 0 --cap 1000
```

Measured at `f7a514a+decoy-cue-347`. Each profile against **its own** control, never
against another's (§13.4).

| Metric | baseline | cautious | aggressive | careless |
|---|---|---|---|---|
| `win_rate` | 0.38 → **0.36** | 0.60 → **0.64** | 0.49 → **0.51** | 0.49 → **0.53** |
| `turns_to_win_median` | 117.0 → **110.0** | 183.5 → **211.0** | 111.0 → **111.0** | 116.0 → **133.0** |
| detections / turn | 0.0527 → **0.0436** | 0.0246 → **0.0246** | 0.0605 → **0.0408** | 0.0704 → **0.0534** |
| `diversity` | 0.6918 → **0.6839** | 0.5399 → **0.5314** | 0.6187 → **0.6365** | 0.5835 → **0.6335** |
| `timeouts` | 3 → **2** | 2 → **0** | 2 → **1** | 1 → **0** |
| `usage.decoy` | **57** | **98** | **27** | **40** |
| `usage_share.decoy` | **0.0038** | **0.0044** | **0.0024** | **0.0033** |

Re-run on the disjoint block `--seed 100 --runs 100`, because the seed-0 detection
drop looked too good:

| Metric | baseline | cautious | aggressive | careless |
|---|---|---|---|---|
| `win_rate` | 0.44 → **0.45** | 0.62 → **0.64** | 0.40 → **0.36** | 0.46 → **0.43** |
| detections / turn | 0.0406 → **0.0410** | 0.0239 → **0.0252** | 0.0660 → **0.0699** | 0.0590 → **0.0728** |
| `diversity` | 0.6093 → **0.6592** | 0.4413 → **0.4298** | 0.6043 → **0.6144** | 0.5455 → **0.5803** |
| `usage.decoy` | **57** | **104** | **42** | **45** |
| `usage_share.decoy` | **0.0043** | **0.0042** | **0.0037** | **0.0036** |

## The reading

- **The slot stops reading a false zero.** Every temperament presses it, 27–104
  times over 100 runs, and the share is stable across two disjoint seed blocks
  (0.24%–0.44%). Whatever else is uncertain, the cue fires.
- **Nothing moved reliably.** The seed-0 block showed detections per turn falling
  in three profiles of four — and the seed-100 block showed them *rising* in three
  of four. Win rate is mixed in both directions in both blocks. On this evidence the
  honest statement is that a decoy at this rate does not move the headline metrics
  outside seed-block wobble; the seed-0 detection drop was noise, and it would have
  been reported as a finding without the second block.
- **Not dominant.** At well under 1% of spent turns it is nowhere near the ~50%
  watermark, and `wait` still dwarfs it in every profile. Dominance is not really
  testable in this shape, though — a verb measured alone has no competition — so the
  claim that matters comes from the full-kit sweep.
- **Where the cue is deliberately shy.** §8.3's row supports a use this cue does not
  bid: dropping a fake to pull a patrol off a route you are about to take, while
  pushing for the objective. That would fire in `Pursue`, where this cue is silent
  by construction. So the numbers above bound what Decoy does *as an escape tool*,
  not what the ability is worth.

## History

- `f7a514a+decoy-cue-347` (#347) — cue written; slot went from a structural zero
  (the sim never granted the tech) to 27–104 presses per 100-run batch. No headline
  metric moved beyond seed-block wobble.
