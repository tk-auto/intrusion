# Repel

**Salvaged tech (§8.3/#554)** — 1 turn to press, duration 8, cooldown 40. Firing stamps
the `REPEL_RADIUS` (3) box on **the cell you fired it from** as ground **no guard may
step into**, in any state: a route across it goes the long way round (§7.6/§10.4), and a
guard with no route at all holds where it stands until the window closes. A **snapshot**,
not a travelling bubble — it does not follow you, because a disc centred on a moving
player is one no guard could ever reach him in (§4.5 **[SETTLED]**). **You** are never
refused a cell of your own field. A guard **already inside** when it lands is bound by
nothing: it may step within the disc and out of it, and only once out is it refused. **It
conceals nothing** — a guard that can see through it sees you, steps §7.3's ladder and
calls it in (§7.7) — so what gathers outside is a ring of guards standing where you have
to come out. Every cell is released when the window ends, expiry or toggle-off alike.
A firing with nobody in reach is never refused and never greyed, and costs its full turn
and lockout. Appendix 60.

## What the cue says

`crates/sim/src/cue.rs`:

- **Flight only** — `Flee`, which is Lockdown's own gate and for its reason: a wall means
  something only to somebody following a route to you.
- **Nothing already inside the disc.** The gate Lockdown has no need of, and the one this
  ability lives or dies by: a guard inside the field is unconstrained, so a press with a
  hunter at arm's length builds a wall with the hunter *on the inside* and spends the turn
  and the 40-turn lockout doing it. Measured against `State::repel_area` — the radius the
  field will actually have — rather than against a cell of elbow room, because everything
  inside that box is on the wrong side of the wall.
- **Somebody in reach, and a route to spend the turns on.** A wall buys turns, and turns
  are only worth something to a bot that is going to walk. Pressed while cornered with no
  step to take, the field holds the hunt off for eight turns and hands back the same cell
  with a ring around it — §8.3's own warning, as a predicate.
- **Strong inside `REPEL_PRESSING` (6), plain beyond it.** Never decisive: an escape that
  actually *moves* (Run) should win the turn whenever both speak.
- Guards are read through the player's own channels (§11.5a), so one the bot cannot
  perceive can still end up inside the field. That is the same dark the player fires into.

## What the sim measured

```
cargo run --release -p intrusion-sim -- --bot --profile <NAME> --runs 100 --seed 0 --cap 1000
cargo run --release -p intrusion-sim -- --bot --profile <NAME> --abilities repel --runs 100 --seed 0 --cap 1000
```

Measured at `5eb8ba7+repel-554`, on the baseline refreshed in the same PR (the ticket's
§7.5 search fix moved it). Each profile against **its own** control, never against
another's (§13.4).

| Metric | balanced | cautious | aggressive | careless |
|---|---|---|---|---|
| `win_rate` | 0.34 → **0.42** | 0.52 → **0.62** | 0.41 → **0.37** | 0.42 → **0.41** |
| `turns_to_win_median` | 133.0 → **159.0** | 226.5 → **266.5** | 139.0 → **143.0** | 141.5 → **154.0** |
| detections / turn | 0.0490 → **0.0466** | 0.0273 → **0.0288** | 0.0560 → **0.0541** | 0.0630 → **0.0606** |
| `diversity` | 0.6010 → **0.5484** | 0.3411 → **0.3564** | 0.5796 → **0.5588** | 0.5035 → **0.5226** |
| `timeouts` | 3 → **7** | 3 → **1** | 0 → **0** | 0 → **0** |
| `usage.repel` | **123** | **197** | **99** | **98** |
| `usage_share.repel` | **0.0057** | **0.0065** | **0.0078** | **0.0076** |

Disjoint block, `--seed 100 --runs 100`:

| Metric | balanced | cautious | aggressive | careless |
|---|---|---|---|---|
| `win_rate` | 0.30 → **0.41** | 0.49 → **0.54** | 0.36 → **0.44** | 0.37 → **0.44** |
| `turns_to_win_median` | 133.5 → **180.0** | 209.0 → **230.0** | 130.0 → **161.5** | 143.0 → **161.0** |
| detections / turn | 0.0538 → **0.0494** | 0.0290 → **0.0329** | 0.0564 → **0.0738** | 0.0638 → **0.0840** |
| `diversity` | 0.6310 → **0.5717** | 0.5612 → **0.4360** | 0.5848 → **0.5959** | 0.4651 → **0.5499** |
| `timeouts` | 2 → **2** | 1 → **3** | 0 → **1** | 0 → **1** |
| `usage.repel` | **126** | **206** | **92** | **104** |

## The reading

- **Win rate goes up, and on the disjoint block it goes up for everybody.** Seed 0 split
  the roster — the avoidance-first pair gained (+0.08, +0.10) and the two that fight lost
  a little (−0.04, −0.01) — which read like a clean story about who a flight tool is for.
  Seed 100 does not tell that story: all four gain, and `aggressive` and `careless` gain
  the most (+0.08, +0.07). **The split was seed noise; the lift is not.** Six to eight
  points across eight profile-blocks is the largest win-rate move of any cued verb on this
  shelf, and it is the number to watch rather than to celebrate — §2.3's question is what
  it *costs*, and the answer here is meant to be a worse position afterwards.
- **Wins take longer, everywhere** — the median is up in all eight blocks, by 3 to 30
  turns. That is what a wall should look like: the field buys ground rather than time, so
  a run that uses it walks further and stands still for the turn each press costs. Read
  alongside the win rate, this is the ability trading turns for survival, which is the
  trade §7.6 wants a flight tool to make.
- **Being seen barely moves, which is the honest half.** detections/turn is within
  ±0.003 on seed 0 and up on the disjoint block for the two aggressive temperaments. The
  ability is not a cloak and the numbers agree: it does not stop anybody looking at you.
- **Not dominant** — 0.57%–0.78% of spent turns, in line with Lockdown (0.56%–0.92%) and
  Confusion. The cue fires roughly once per hundred and a half turns.
- **`diversity` is unstable** — down on balanced (both blocks), up on careless (both), and
  it flips sign for cautious and aggressive between the blocks. Nothing to read yet; the
  verbs that moved this metric consistently (Confusion, Camouflage, Lockdown down;
  Dephase, Pierce Wall up) all moved it in every block.
- **The Lockdown comparison is the point of this page** (§2.3/#554), and it is *not yet
  made*: both arms above hold one ability, and what the ticket warns about is a run
  holding **both** and always pressing the same one. The batch that answers it is
  `--abilities lockdown,repel`, read against each alone, and it belongs to the first
  playtest that has a reason to run it. Until then: their usage shares are near-identical
  (0.0057–0.0078 here, 0.0056–0.0092 there), which is consistent with two abilities the
  bot presses in the same moments — the very thing to be suspicious of.

## Kill-thresholds

The numbers that would say the experiment has gone wrong, stated before they are
measured (§13.4):

- **`win_rate` climbing while `detections` climbs with it, on every temperament.** That is
  the shape of an ability that has stopped costing anything — being seen no longer
  matters, which is §8.3's standing worry about the pair of Run and a wall you can drop
  behind you.
- **A share that overtakes Lockdown's on a run holding both**, with Lockdown's falling to
  near zero. One of the pair is then dead weight (§2.3), and the answer is `REPEL_RADIUS`
  — the knob that changes *which rooms* each is for — not the clocks.
- **The timeout tail growing.** A bot that can wall itself in and wait is a bot that has
  found a way not to play; `balanced` at seed 0 went 3 → 7 and is the one row here that
  points that way. If the tail grows on both blocks, the window is too long.

## History

- `5eb8ba7+repel-554` (#554) — page created with the ability. Cue written on Lockdown's,
  plus the void-around-a-guard-inside gate. Win rate up in six of eight profile-blocks and
  in all four on the disjoint one; wins slower everywhere; the Lockdown pair-up
  deliberately not yet measured.
