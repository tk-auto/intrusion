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

Measured at `7d5dc01+cues-347-remeasured`, on the post-crouch bot (#382). Each profile against
**its own** control, never against another's (§13.4).

| Metric | balanced | cautious | aggressive | careless |
|---|---|---|---|---|
| `win_rate` | 0.38 → **0.35** | 0.60 → **0.64** | 0.50 → **0.52** | 0.49 → **0.53** |
| `turns_to_win_median` | 117.5 → **116.0** | 183.5 → **206.0** | 111.0 → **111.0** | 116.0 → **133.0** |
| detections / turn | 0.0527 → **0.0433** | 0.0248 → **0.0250** | 0.0432 → **0.0406** | 0.0704 → **0.0534** |
| `diversity` | 0.6730 → **0.6598** | 0.4867 → **0.4809** | 0.6230 → **0.6360** | 0.5835 → **0.6335** |
| `timeouts` | 3 → **2** | 2 → **0** | 1 → **1** | 1 → **0** |
| `usage.decoy` | **56** | **95** | **26** | **40** |
| `usage_share.decoy` | **0.0038** | **0.0042** | **0.0023** | **0.0033** |

Disjoint block, `--seed 100 --runs 100`:

| Metric | balanced | cautious | aggressive | careless |
|---|---|---|---|---|
| `win_rate` | 0.44 → **0.44** | 0.63 → **0.65** | 0.37 → **0.35** | 0.46 → **0.43** |
| `turns_to_win_median` | 117.5 → **118.0** | 204.0 → **225.0** | 92.0 → **90.0** | 110.0 → **107.0** |
| detections / turn | 0.0411 → **0.0424** | 0.0233 → **0.0242** | 0.0683 → **0.0562** | 0.0590 → **0.0728** |
| `diversity` | 0.5723 → **0.6169** | 0.4394 → **0.4216** | 0.6146 → **0.6155** | 0.5455 → **0.5803** |
| `timeouts` | 1 → **1** | 1 → **3** | 1 → **1** | 0 → **1** |
| `usage.decoy` | **60** | **99** | **34** | **45** |
| `usage_share.decoy` | **0.0046** | **0.0039** | **0.0033** | **0.0036** |

## The reading

- **The slot stops reading a false zero.** Every temperament presses it, 26–99
  times over 100 runs, and the share is stable across two disjoint seed blocks
  (0.23%–0.46%). Whatever else is uncertain, the cue fires.
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
- `7d5dc01+cues-347-remeasured` (#347) — **re-measured on the post-crouch bot**
  (#382 landed while this branch was open, changing the policy and refreshing the
  innate baseline, so every control column above moved). Unchanged in substance: the cue still fires 26–99 times a batch and still moves no headline metric beyond seed-block wobble.
