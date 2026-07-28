# Confusion

**Salvaged tech (§8.3)** — 1 turn to press, instant, cooldown 45. **Fired once**
from the cell you press it in. Every guard standing within the blast at that moment
— `CONFUSION_RADIUS`, through walls like the guard sense — is blinded and frozen for
six turns. *"A costed panic-buy of time, not a kill"*: a dazed chaser **pauses** and
keeps its lead. The clamp is **[SETTLED]**: the reach fired is
`min(CONFUSION_RADIUS, sense_range())`, so the blast can never freeze what you could
not sense.

## What the cue says

`crates/sim/src/cue.rs`:

- **Panic only** — `Flee`. Freezing a patrol that has not seen you spends the longest
  cooldown in the catalog on a guard that was going to walk past anyway.
- **Cornered is the moment it exists for.** With a guard at arm's length, Run and
  Autodoors both decline — they need a cell of room to spend the activation turn in
  — and the next step is a capture (§4.5). A dazed adjacent guard cannot step into
  you, so this is the one turn that buys the run back, and the cue bids **decisive**
  for it. It is the only cue that speaks at a gap of one.
- **More than one hunter in the blast is worth the cooldown** — **strong**. Against
  a single chaser with room to run, outrunning it is the cheaper turn and Run says
  so; the bid there is only **plain**.
- **The count comes from core's own blast**, already clamped to the live guard sense,
  so reading it stays inside what the player was shown (§8.3's clamp rationale:
  anything it could catch, they were already sensing). Legality is core's too — a
  firing that catches nobody is `Unusable`.

## What the sim measured

```
cargo run --release -p intrusion-sim -- --bot --profile <NAME> --runs 100 --seed 0 --cap 1000
cargo run --release -p intrusion-sim -- --bot --profile <NAME> --abilities confusion --runs 100 --seed 0 --cap 1000
```

Measured at `690fb61+confusion-cue-347`. Each profile against its own control.

| Metric | baseline | cautious | aggressive | careless |
|---|---|---|---|---|
| `win_rate` | 0.38 → **0.44** | 0.60 → **0.68** | 0.49 → **0.55** | 0.49 → **0.51** |
| `turns_to_win_median` | 117.0 → **123.5** | 183.5 → **207.5** | 111.0 → **116.0** | 116.0 → **115.0** |
| detections / turn | 0.0527 → **0.0385** | 0.0246 → **0.0205** | 0.0605 → **0.0386** | 0.0704 → **0.0437** |
| `diversity` | 0.6918 → **0.5915** | 0.5399 → **0.4568** | 0.6187 → **0.5632** | 0.5835 → **0.5610** |
| `timeouts` | 3 → **3** | 2 → **1** | 2 → **2** | 1 → **1** |
| `usage.confusion` | **194** | **132** | **104** | **125** |
| `usage_share.confusion` | **0.0109** | **0.0058** | **0.0087** | **0.0094** |

Disjoint block, `--seed 100 --runs 100`:

| Metric | baseline | cautious | aggressive | careless |
|---|---|---|---|---|
| `win_rate` | 0.44 → **0.49** | 0.62 → **0.71** | 0.40 → **0.45** | 0.46 → **0.54** |
| detections / turn | 0.0406 → **0.0356** | 0.0239 → **0.0228** | 0.0660 → **0.0423** | 0.0590 → **0.0495** |
| `diversity` | 0.6093 → **0.5377** | 0.4413 → **0.4462** | 0.6043 → **0.5426** | 0.5455 → **0.5131** |
| `usage.confusion` | **152** | **173** | **115** | **135** |
| `usage_share.confusion` | **0.0096** | **0.0064** | **0.0099** | **0.0102** |

## The reading

This is the first cue whose effect **reproduced across both seed blocks**, and it
moved two things in opposite directions.

- **Win rate up everywhere, in both blocks** — +0.02 to +0.08 at seed 0, +0.05 to
  +0.09 at seed 100, every temperament, no exceptions. Detections per turn fall
  everywhere too, by as much as a third under `aggressive`. That is a coherent,
  ability-shaped effect and not noise.
- **`diversity` falls, and that is the flag.** Seven of the eight profile-blocks drop
  (only `cautious` at seed 100 ticks up), and `baseline` loses 0.10 and 0.07. §13.2
  calls diversity the boredom metric: runs are becoming *more alike*. An escape that
  always works makes every hunt end the same way, which is exactly the shape of "a
  puzzle with one answer" — and it is worth more attention than the win rate that
  looks like good news.
- **Not dominant by the watermark** — 0.58%–1.09% of spent turns, the highest of the
  cues so far but two orders of magnitude below the ~50% scream. Inside a single run
  the heaviest use is ~2% of that run's turns. **No cue has been tuned to make this
  number look reasonable**, per #347: what is written above is what the §8.3 row
  produced on the first measurement.
- **Coupled to §15 Q1**, the chase question — this ability is a chase-shaped tool, so
  expect the numbers to move when Q1 does.

### Suspicious seeds — go play these (§13.3)

Confusion turns some captures into **timeouts**: the bot buys six turns, again and
again, and rides out the input cap instead of ever finishing. Three under `baseline`
at seed 0:

| Seed | Confusion presses | Outcome without → with |
|---|---|---|
| 21 | 18 | capture → timeout |
| 40 | 16 | win → timeout |
| 50 | 16 | capture → timeout |

```
cargo run --release -p intrusion-sim -- --bot --profile baseline --runs 1 --seed 40 --abilities confusion
```

Seed 40 is the one to play first: it *won* without the ability and stalls out with
it. Whether that is the ability, the cue, or the bot's inability to convert bought
time into progress is a human call (§13.4) — the sim only says it is worth looking
at. 30 of 100 seeds changed outcome under `baseline`, so there is no shortage of
material.

## History

- `690fb61+confusion-cue-347` (#347) — cue written; slot went from a structural zero
  to 104–194 presses per 100-run batch. Win rate up and detections down in every
  profile across both seed blocks; `diversity` down in seven of eight, flagged as
  the boredom signal rather than tuned away.
