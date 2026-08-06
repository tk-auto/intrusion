# Dart

**Salvaged tech (§8.3) — the experiment (#239).** 1 turn to press, instant, **no
cooldown**, **a per-level use budget of 1**. *"Fires the way you face. The first guard on
the line drops if it has not seen you."* The dart travels up to `DART_RANGE` (**8**
**[START]**) cells along the cardinal the player faces, stops at the first solid or the
first guard, and takes that guard down **only if it is unaware** (§7.2) **and in line of
sight** (§6). The body drops **where it stood**, on that guard's own radio cadence.

It is the one row in the catalogue that deliberately reopens the ability §2.3 records as
having *been* the old game, so it is filed with **no milestone** and contradicts §7.2's
**[SETTLED]** *adjacent only*. Appendix 54 has the reasoning; this page has the numbers.

## What the cue says

`crates/sim/src/cue.rs`. Two things make this cue unlike the other eight.

- **It has to check its own aim.** Every other cue leans on `status.state` for legality
  (#345), because every other ability reads `Unusable` when its target is missing. A dart
  is deliberately **never refused** — a greyed entry would answer *"is there a guard in
  front of me?"* for free, every frame (§8.4) — so `Ready` says nothing at all about the
  line. The cue therefore calls `State::dart_shot` and declines unless it resolved a legal
  hit. That is still core's answer: the same function the firing calls, re-implemented
  nowhere.
- **It asks the question that separates this from the free verb.** #347's failure mode is
  sharpest here, because the press can never be refused into a zero: a cue that fired at
  the first legal guard would spend the facility's dart on the first patrol in front of the
  bot and the histogram would read *used* while measuring nothing. So the bid needs the
  target to be **not adjacent** (the innate takedown costs the same turn and no use) and
  its **own cone** to watch the ground the bot is heading for — the watcher a route cannot
  simply be planned around.

Never in flight, and that one is *legality* rather than judgement: a guard hunting the bot
has detected it, so it is not a legal target at all.

**It is shy by construction, and the shyness is honest.** The bot faces the way it last
stepped and there is no turn-in-place action (§5), so the line has to *happen* to be right
— the bot cannot walk round to set the shot up the way a player can. Read the counts below
as a **floor** on the ability, not a measure of it.

**The bot never misses.** The cue only bids on a resolved hit, so every dart in these
batches landed (`usage.dart` equals the takedown delta exactly). The miss path — an empty
line, an aware guard, the turn and the use spent for nothing — is real, is what most of
`crates/core/src/state/tests/dart.rs` pins, and is **not measured here at all**. A human
playtest is the only thing that can price it.

## What the sim measured

```
cargo run --release -p intrusion-sim -- --bot --profile <NAME> --runs 100 --seed 0 --cap 1000
cargo run --release -p intrusion-sim -- --bot --profile <NAME> --abilities dart --runs 100 --seed 0 --cap 1000
```

Measured at `3c8342e+dart-239`. Each profile against **its own** control, never against
another's (§13.4).

| Metric | balanced | cautious | aggressive | careless |
|---|---|---|---|---|
| `win_rate` | 0.35 → **0.37** | 0.53 → **0.54** | 0.40 → **0.39** | 0.43 → **0.44** |
| `turns_to_win_median` | 137.0 → **147.0** | 235.0 → **239.0** | 136.0 → **133.0** | 142.0 → **145.5** |
| detections / turn | 0.0484 → **0.0456** | 0.0259 → **0.0254** | 0.0566 → **0.0545** | 0.0865 → **0.0845** |
| `diversity` | 0.6027 → **0.5951** | 0.3410 → **0.3418** | 0.5738 → **0.5732** | 0.4974 → **0.4939** |
| `timeouts` | 3 → **3** | 3 → **3** | 0 → **0** | 1 → **1** |
| `takedowns` | 0 → **12** | 0 → **8** | 22 → **26** | 21 → **21** |
| `bodies_found` | 0 → **11** | 0 → **8** | 3 → **5** | 17 → **16** |
| `alert_peak_mean` | 0.76 → **0.95** | 0.67 → **0.79** | 1.07 → **1.09** | 1.18 → **1.18** |
| `usage.dart` | **12** | **8** | **4** | **2** |
| `usage_share.dart` | **0.0007** | **0.0003** | **0.0003** | **0.0001** |

Disjoint block, `--seed 100 --runs 100` (the two temperaments that fired most):

| Metric | balanced | cautious |
|---|---|---|
| `win_rate` | 0.30 → **0.32** | 0.51 → **0.52** |
| `turns_to_win_median` | 133.5 → **133.5** | 214.0 → **211.5** |
| detections / turn | 0.0523 → **0.0508** | 0.0280 → **0.0275** |
| `diversity` | 0.6323 → **0.6333** | 0.5614 → **0.5612** |
| `timeouts` | 3 → **3** | 1 → **2** |
| `takedowns` | 0 → **11** | 0 → **11** |
| `bodies_found` | 0 → **11** | 0 → **10** |
| `alert_peak_mean` | 0.86 → **1.02** | 0.78 → **0.93** |
| `usage.dart` | **11** | **11** |
| `usage_share.dart` | **0.0006** | **0.0004** |

## The kill-thresholds

The ticket asks for these **before** the run. They were not: they are written here after
the batch above, so this batch has not passed a pre-registered test and should not be
described as having done so. What they bind is the next batch and every tuning decision
after it. The batch above is nowhere near any of them, which is the weaker claim, honestly
made.

The ability is **rejected or re-costed** if any of these holds:

| Threshold | Reject at | Where this batch sits |
|---|---|---|
| **Win rate** — a free win is the §2.3 failure restated | **+0.10** or more on any temperament, reproduced on a disjoint block | +0.02 worst case, and it flips sign across profiles |
| **Usage share** — the histogram scream (§13.2) | **0.005** of spent turns (≈7× here), or a share above `takedown`'s | 0.0007 at most; `takedown` outruns it by 20–50× on the profiles that use both |
| **Alert** — the body is supposed to cost | `alert_peak_mean` **falling** while `usage.dart` rises, i.e. the body stopping being a cost | rises in every profile that fires (+0.12 … +0.19) |
| **Detections** — a stealth solvent | detections/turn down by **25%** or more | down 2%–6% |

Two of those deserve a word. The **win-rate** bar is set at +0.10 because appendix 43
records what a genuinely too-strong exception looks like in this harness: the Saver took a
fearless bot's win rate up by nearly a factor of two. +0.02 is not that. And the **alert**
row is inverted on purpose — it is the only threshold that fails on a number going *down*,
because the design's whole claim about this ability is that the unreachable body pays for
the takedown, and an alert that did not move would mean it does not.

## The reading

- **The ability is real, small, and it pays for itself in the currency the design said it
  would.** The clearest number on the page is not a headline metric at all: in `balanced`,
  12 darts → 12 takedowns → **11 bodies found**. The bot shoots a guard down a corridor,
  never goes to fetch the body, and the facility finds it — which is §7.2's economy (*"a
  takedown you cannot hide is a takedown that finds you later"*) turning up in the data as
  `alert_peak_mean` climbing 0.76 → 0.95. Same shape on `cautious` (8 → 8 → 8, alert 0.67 →
  0.79) and on both disjoint blocks. This is the counterweight working, measured, and it is
  the finding worth keeping.
- **Win rate does not move.** +0.02, +0.01, −0.01, +0.01 at seed 0; +0.02, +0.01 at seed
  100. The README's own noise band for a 100-seed batch is several points, and the sign
  flips between temperaments, so the honest statement is *flat*. The reopened neutralise
  is not, at these numbers, buying the run.
- **Detections per turn fall slightly in all six profile-blocks** (2%–6%). One watcher off
  the route is one watcher off the route; that it is this small is the use budget doing its
  job.
- **`diversity` is flat** — six blocks within ±0.008 but for `balanced` at seed 0 (−0.008).
  A once-a-level verb cannot reshape a run's turn profile, and it does not.
- **Usage falls as the temperament gets braver**: 12 / 8 / 4 / 2 across balanced, cautious,
  aggressive, careless. That reads correctly rather than oddly — `aggressive` and `careless`
  already take 21–22 guards down on foot, so the watcher the dart is *for* is one they have
  usually already walked up to. The verb is worth most to a run that does not want to be in
  the same corridor as a guard.
- **The `careless` column is nearly inert** (2 darts, alert and bodies unchanged) and should
  not be read as evidence either way; two firings in 100 runs is not a measurement.

### What this measurement cannot tell you

**Everything about the miss.** The cue never takes a shot it knows will fail, so the batch
above measures a dart that always lands. A player's dart will miss — into an empty
corridor, at a guard that turned round, at a `g` they had not registered — and the whole
turn and the level's only use go with it. That cost is the ability's main balance surface
and it is invisible here (§13.4: the bot is a smoke detector, not a fun oracle).

**And the corridor case.** The ticket names *"stand at the end of a corridor"* as the risk,
and the bot cannot test it: it fires down whichever line it happens to be facing, never one
it walked to on purpose. §10.1a's tables bound the geometry and a table stops the dart
(appendix 54), which is an argument rather than a measurement. If the shot plays as a
reliable free kill, `DART_RANGE` is the lever — the range changes the *play*, where the
budget only changes the frequency.

## History

- `3c8342e+dart-239` (#239) — ability shipped and first measured. Win rate flat across four
  temperaments and two seed blocks; usage 2–12 per 100 runs; every dart that fired left a
  body, and nearly every body was found, taking `alert_peak_mean` up in each profile that
  used the verb. The `[START]` numbers are unmoved from the ticket except the **cooldown**,
  which is 0 rather than the *"very large"* one asked for — the budget of 1 is stricter than
  any lockout, and the bar and the help panel both refuse to show a clock behind it
  (appendix 54).
