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

## Two of the four temperaments decline it outright, and that is the headline

`balanced` and `cautious` ship `takedown_reach: 0`, which that knob's own doc defines as
*"not 'never gets the chance', but 'does not want it'"* — and both report `takedowns: 0` in
their control columns. **A dart is §7.2's verb with the range changed, so the cue honours
the same appetite** (#316): those two profiles holding the Dart are **byte-identical** to
the same profiles without it, on every metric.

That is the correct behaviour and it is also most of what this page has to say. **It was
not the first behaviour.** The cue's first version ignored the appetite and fired for all
four, and the numbers it produced were confidently wrong in a specific and instructive way
— `balanced` went from 0 takedowns to 12, left 11 bodies where its control leaves none, and
its `alert_peak_mean` rose 0.76 → 0.95. Read as a measurement that looked like the design's
own claim coming true ("the unreachable body pays for the takedown"). It was not: it was an
avoidance-first bot being handed a kill it would never walk up and take, and then compared
against the baseline of the bot it had stopped being. §13.3's failure mode, in its purest
form. See appendix 54.

The consequence for this page is that **the sim exercises this ability much less than the
first draft claimed**, and the human playtest carries correspondingly more weight.

## What the cue says

`crates/sim/src/cue.rs`. Three gates, in the order they matter.

- **The temperament's takedown appetite comes first.** `!strikes` → no bid, at any range
  (above). It is threaded through `Moment` as a fact about the plan, like `Intent`, rather
  than read out of `Profile` inside the cue.
- **It has to check its own aim**, which no other cue does. Every other one leans on
  `status.state` for legality (#345), because every other ability reads `Unusable` when its
  target is missing. A dart is deliberately **never refused** — a greyed entry would answer
  *"is there a guard in front of me?"* for free, every frame (§8.4) — so `Ready` says
  nothing at all about the line. The cue therefore calls `State::dart_shot` and declines
  unless it resolved a legal hit. That is still core's answer: the same function the firing
  calls, re-implemented nowhere.
- **It asks the question that separates this from the free verb.** #347's failure mode is
  sharpest here, because the press can never be refused into a zero: a cue that fired at
  the first legal guard would spend the facility's dart on the first patrol in front of the
  bot and the histogram would read *used* while measuring nothing. So the bid needs the
  target to be **not adjacent** (the walk-up takedown costs the same turn and no use) and
  its **own cone** to watch the ground the bot is heading for — the watcher a route cannot
  simply be planned around.

**It is shy on top of all that.** The bot faces the way it last stepped and there is no
turn-in-place action (§5), so the line has to *happen* to be right; the bot cannot walk
round to set a shot up the way a player can. Read the counts below as a **floor**.

**The bot never misses.** The cue only bids on a resolved hit, so every dart in these
batches landed. The miss path — an empty line, an aware guard, the turn and the level's use
spent for nothing — is real, is what most of `crates/core/src/state/tests/dart.rs` pins,
and is **not measured here at all**. Only a human playtest can price it.

## What the sim measured

```
cargo run --release -p intrusion-sim -- --bot --profile <NAME> --runs 100 --seed 0 --cap 1000
cargo run --release -p intrusion-sim -- --bot --profile <NAME> --abilities dart --runs 100 --seed 0 --cap 1000
```

Measured at `9566794+dart-appetite-gate`. Each profile against **its own** control (§13.4).

`--seed 0`, all four temperaments:

| Metric | balanced | cautious | aggressive | careless |
|---|---|---|---|---|
| `win_rate` | 0.35 → **0.35** | 0.53 → **0.53** | 0.40 → **0.39** | 0.43 → **0.44** |
| detections / turn | 0.0484 → **0.0484** | 0.0259 → **0.0259** | 0.0566 → **0.0545** | 0.0865 → **0.0845** |
| `diversity` | 0.6027 → **0.6027** | 0.3410 → **0.3410** | 0.5738 → **0.5732** | 0.4974 → **0.4939** |
| `takedowns` | 0 → **0** | 0 → **0** | 22 → **26** | 21 → **21** |
| `bodies_found` | 0 → **0** | 0 → **0** | 3 → **5** | 17 → **16** |
| `alert_peak_mean` | 0.76 → **0.76** | 0.67 → **0.67** | 1.07 → **1.09** | 1.18 → **1.18** |
| `usage.dart` | **0** | **0** | **4** | **2** |
| `usage_share.dart` | **0** | **0** | **0.0003** | **0.0001** |

The first two columns are **identical, not merely close** — every metric, including the
ones not shown. That is the appetite gate working, and it is the same property #316 already
guarantees for the walk-up takedown.

Disjoint block, `--seed 100 --runs 100`, the two temperaments that strike:

| Metric | aggressive | careless |
|---|---|---|
| `win_rate` | 0.37 → **0.40** | 0.37 → **0.39** |
| `turns_to_win_median` | 130.0 → **132.5** | 143.0 → **144.0** |
| detections / turn | 0.0558 → **0.0551** | 0.0641 → **0.0851** |
| `diversity` | 0.5873 → **0.6180** | 0.4604 → **0.4806** |
| `takedowns` | 19 → **29** | 18 → **25** |
| `bodies_found` | 7 → **8** | 17 → **24** |
| `alert_peak_mean` | 1.11 → **1.19** | 1.26 → **1.34** |
| `timeouts` | 0 → **0** | 0 → **1** |
| `usage.dart` | **11** | **6** |
| `usage_share.dart` | **0.0009** | **0.0004** |

## The kill-thresholds

The ticket asks for these **before** the run. They were not: they were written after the
first batch, so nothing here has passed a pre-registered test and it should not be described
as having done so. What they bind is the next batch and every tuning decision after it.

**Rejected or re-costed** if any of these holds:

| Threshold | Reject at | Where these batches sit |
|---|---|---|
| **Win rate** — a free win is the §2.3 failure restated | **+0.10** or more on any temperament, reproduced on a disjoint block | +0.03 worst case (`aggressive`, seed 100), and −0.01 on the same profile at seed 0 |
| **Usage share** — the histogram scream (§13.2) | **0.005** of spent turns, or a share above `takedown`'s | 0.0009 at most; `takedown` outruns it 15–40× |
| **Alert** — the body is supposed to cost | `alert_peak_mean` **falling** while `usage.dart` rises | rises on 3 of 4 striking blocks, flat on the fourth |
| **Detections** — a stealth solvent | detections/turn down by **25%** or more | down 2%–4%, and **up 33%** on `careless` at seed 100 |

The **win-rate** bar is +0.10 because appendix 43 records what a genuinely too-strong
exception looks like in this harness: the Saver took a fearless bot's win rate up by nearly
a factor of two. And the **alert** row is inverted on purpose — it is the only threshold
that fails on a number going *down*, because the design's claim is that the unreachable
body pays for the takedown, and an alert that did not move would mean it does not.

## The reading

- **The measurement is thin, and that is the honest summary.** Two of four temperaments
  decline the verb, and the two that take it fire 2–11 darts per 100 runs. Nothing on this
  page is a strong claim about the ability; it is a claim that the ability is *not
  obviously broken* on the half of the roster that will touch it.
- **The counts wobble hard between seed blocks** — 4 → 11 for `aggressive`, 2 → 6 for
  `careless`. On a cue this shy that is what a hundred seeds looks like, and it is a reason
  to distrust any single column here rather than a finding.
- **Win rate is flat-to-mildly-up on the striking half**: −0.01 and +0.01 at seed 0, +0.03
  and +0.02 at seed 100. Within the noise band, but the direction is consistently positive
  at seed 100 and worth re-checking on a third block before anybody calls it flat.
- **The takedown count rises by more than the darts fired** — `aggressive` at seed 100
  went +10 takedowns for 11 darts, `careless` +7 for 6. That is a knock-on rather than
  double-counting: removing a guard changes the run, and a striking bot then finds different
  walk-up opportunities. Worth knowing, because it means the dart's effect on a
  takedown-happy profile is not confined to the one shot.
- **The counterweight still shows, on smaller numbers than the retracted version claimed.**
  `bodies_found` and `alert_peak_mean` both rise on 3 of the 4 striking blocks — `careless`
  at seed 100 most clearly, 17 → 24 bodies and 1.26 → 1.34. §7.2's economy is visible; it
  is just no longer the tidy 12-darts-11-bodies story, which was the bug.
- **One flag worth a human's eyes:** `careless` at seed 100 shows detections/turn **up 33%**
  (0.0641 → 0.0851) on six darts. That is the wrong direction and a big move for a rare
  verb. It may be six seeds' worth of noise on the least careful temperament, and it may be
  the dart tempting a bot into a corridor it should not be in. Nothing on this page settles
  it.

### What this measurement cannot tell you

**Everything about the miss** — the cue never takes a shot it knows will fail (above).

**Anything about the avoidance-first half of the roster**, which is where the ability is
arguably most interesting: a player who does not want to walk into a guard's blind spot is
exactly the player a ranged takedown is *for*. The cue cannot speak for them without
overriding a temperament, which is what it did wrong the first time. **The open question is
whether the Dart deserves an appetite of its own** rather than borrowing
`takedown_reach` — a walk-up takedown asks for a detour into a cone, and a dart asks for
none, so the two may not belong on one axis. That is a §13.4 temperament-design decision and
it is not this ticket's to take.

**And the corridor case.** The ticket names *"stand at the end of a corridor"* as the risk,
and the bot cannot test it: it fires down whichever line it happens to be facing, never one
it walked to on purpose. §10.1a's tables bound the geometry and a table stops the dart
(appendix 54), which is an argument rather than a measurement. If the shot plays as a
reliable free kill, `DART_RANGE` is the lever — the range changes the *play*, where the
budget only changes the frequency.

## History

- `3c8342e+dart-239` — first measurement, **retracted**. The cue ignored `takedown_reach`,
  so `balanced` and `cautious` fired 12 and 8 darts and left bodies their controls never
  leave; every number for those two profiles measured a changed bot against the old bot's
  baseline (§13.3). The apparent finding — 12 darts → 11 bodies → alert 0.76 → 0.95 — was an
  artefact of that and is not evidence of anything.
- `9566794+dart-appetite-gate` (#239) — cue gated on the temperament's takedown appetite.
  `balanced` and `cautious` are byte-identical to their controls; the striking half fires
  2–11 per 100 runs with win rate flat-to-mildly-up and the §7.2 counterweight visible on
  smaller numbers. All `[START]` values unmoved from the ticket except the **cooldown**,
  which is 0 rather than *"very large"* — the budget of 1 is stricter than any lockout, and
  the bar and the help panel both refuse to show a clock behind it (appendix 54).
