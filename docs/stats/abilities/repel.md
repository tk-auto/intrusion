# Repel

**Salvaged tech (§8.3/#554)** — 1 turn to press, duration 8, cooldown 40. Firing stamps
the `REPEL_RADIUS` (3) box on **the cell you fired it from** as ground **no guard will
stand in**. Two halves: **nobody gets in** — in any state, so a route across it goes the
long way round (§7.6/§10.4), and a guard cut off entirely **closes on the boundary and
waits there**; and **anybody inside walks out**, by the shortest way, keeping its mood and
its errand for when it is clear. A **snapshot**, not a travelling bubble — it does not
follow you, because a disc centred on a moving player is one no guard could ever reach him
in (§4.5 **[SETTLED]**). **You** are never refused a cell of your own field. **It conceals
nothing** — a guard that can see through it sees you, steps §7.3's ladder and calls it in
(§7.7) — so what gathers outside is a ring of guards one step from the disc, standing
where you have to come out. Every cell is released when the window ends. A firing with
nobody in reach is never refused and never greyed, and costs its full turn and lockout.
Appendix 60.

## What the cue says

`crates/sim/src/cue.rs`:

- **Flight only** — `Flee`, which is Lockdown's own gate and for its reason: a wall means
  something only to somebody following a route to you.
- **Somebody in reach, and a route to spend the turns on.** A wall buys turns, and turns
  are only worth something to a bot that is going to walk. Pressed while cornered with no
  step to take, the field holds the hunt off for eight turns and hands back the same cell
  with a ring around it — §8.3's own warning, as a predicate.
- **Strong inside `REPEL_PRESSING` (6), plain beyond it.** Never decisive: an escape that
  actually *moves* (Run) should win the turn whenever both speak.
- **It used to carry a third gate and no longer does.** While a guard inside the disc was
  unconstrained, a press with a hunter at arm's length was the worst in the ability and the
  cue declined it. Guards now walk out, so that press is no longer a mistake — a cue that
  went on checking for it would be measuring its own memory of a rule the game has stopped
  having. Dropping it roughly **doubled** the press count, which is most of why the numbers
  below moved.

## What the sim measured

```
cargo run --release -p intrusion-sim -- --bot --profile <NAME> --runs 100 --seed 0 --cap 1000
cargo run --release -p intrusion-sim -- --bot --profile <NAME> --abilities repel --runs 100 --seed 0 --cap 1000
```

Measured at `5eb8ba7+repel-554`, on the baseline refreshed in the same PR (the ticket's
§7.5 search fix moved it; the guard rules below did not — with no field up they are inert,
and the committed baseline re-checks clean). Each profile against **its own** control,
never against another's (§13.4).

| Metric | balanced | cautious | aggressive | careless |
|---|---|---|---|---|
| `win_rate`, seed 0 | 0.34 → **0.40** | 0.52 → **0.65** | 0.41 → **0.46** | 0.42 → **0.50** |
| `win_rate`, seed 100 | 0.30 → **0.47** | 0.49 → **0.60** | 0.36 → **0.49** | 0.37 → **0.44** |
| `turns_to_win_median`, seed 0 | 133.0 → **154.0** | 226.5 → **264.0** | 139.0 → **140.0** | 141.5 → **140.0** |
| detections / turn, seed 0 | 0.0490 → **0.0405** | 0.0273 → **0.0244** | 0.0560 → **0.0530** | 0.0630 → **0.0500** |
| `diversity`, seed 0 | 0.6010 → **0.5096** | 0.3411 → **0.2993** | 0.5796 → **0.5175** | 0.5035 → **0.5204** |
| `timeouts`, seed 0 | 3 → **5** | 3 → **1** | 0 → **1** | 0 → **0** |
| `usage.repel`, seed 0 | **242** | **241** | **155** | **154** |
| `usage_share.repel`, seed 0 | **0.0116** | **0.0078** | **0.0111** | **0.0108** |

### The two changes, told apart

The guard rules changed after the first playtest (appendix 60): a cut-off guard now
**closes on the boundary** instead of holding where it stood, and a guard caught **inside
walks out** instead of milling about. The cue's third gate went with them. Those pull in
opposite directions, so they are worth separating — measured with the *original* cue, which
declined the late press:

| `win_rate`, seed 0 | balanced | cautious |
|---|---|---|
| control | 0.34 | 0.52 |
| first version (guards stuck inside; hold where cut off) | **0.42** | **0.62** |
| shipped rules, original cue | **0.36** | **0.62** |
| shipped rules, cue un-gated | **0.40** | **0.65** |

**The cordon pays for the ejection.** Letting a hunter out of the disc is a straight gift;
making every cut-off guard queue at the perimeter takes most of it back, and on `balanced`
rather more than all of it. What the last row adds is the cue being allowed to use the new
rule — twice the presses, and the lift that comes with them.

## The reading

- **Win rate is up in all eight profile-blocks**, +0.05 to +0.17. That is the largest and
  the *only* uniform lift on this shelf — every other cued verb moves some temperaments and
  not others — and it is the number to watch rather than to celebrate. §2.3's question is
  what the ability costs, and "wins more, everywhere, whoever is holding it" is the shape of
  an answer that says *not much*.
- **Being seen goes down, not up** — detections/turn falls in all four at seed 0. The
  ability is not a cloak and does not stop anybody looking; what it stops is the *arrival*,
  and the bot spends fewer turns in a cone as a result. This is the metric that would flag
  the panic-button reading if it inverted, so it is worth watching more than the win rate.
- **Wins take longer for the careful pair** (balanced +21, cautious +37 turns at seed 0) and
  are flat for the two that fight. A wall buys ground rather than time, and the
  temperaments that use it most are the ones that were already walking around trouble.
- **Not dominant, but no longer negligible** — around 1% of spent turns, against Lockdown's
  0.56%–0.92% and Confusion's 0.62%–0.98%. The press count roughly doubled when the cue's
  stale gate came off.
- **`diversity` falls in seven of eight**, which puts Repel with Confusion, Camouflage and
  Lockdown — the verbs that *answer a hunt* make runs more alike — rather than with the
  geometry verbs that make them less alike. Consistent with what it is.
- **The Lockdown comparison is the point of this page** (§2.3/#554), and it is *not yet
  made*: both arms hold one ability, and what the ticket warns about is a run holding
  **both** and always pressing the same one. The batch that answers it is
  `--abilities lockdown,repel`, read against each alone, and it belongs to the first
  playtest that has a reason to run it.

## Kill-thresholds

The numbers that would say the experiment has gone wrong, stated before they are
measured (§13.4):

- **`win_rate` climbing while `detections` climbs with it, on every temperament.** That is
  the shape of an ability that has stopped costing anything — being seen no longer matters,
  which is §8.3's standing worry about the pair of Run and a wall you can drop behind you.
  Today the win rate climbs and detections *fall*, so this threshold is not tripped — but
  the win-rate half of it is already the largest on the shelf, and the ability has no
  "when would a good player not press this" left beyond the turn and the lockout since
  guards started leaving the disc (appendix 60 §3). **The named repair, if it trips:**
  exempt a guard that currently *has* the player from the exit rule, so the field clears
  patrols out of itself but never rescues you from a hunt that has already arrived. One
  condition in `State::repel_exit_step`, and it restores the ability's old failure case
  without touching the radius, the clocks or the cordon.
- **A share that overtakes Lockdown's on a run holding both**, with Lockdown's falling to
  near zero. One of the pair is then dead weight (§2.3), and the answer is `REPEL_RADIUS`
  — the knob that changes *which rooms* each is for — not the clocks.
- **The timeout tail growing.** A bot that can wall itself in and wait is a bot that has
  found a way not to play; `balanced` at seed 0 goes 3 → 5, and 0 → 0 on the disjoint
  block. If the tail grows on both blocks, the window is too long.

## History

- `5eb8ba7+repel-554` (#554) — page created with the ability. Cue written on Lockdown's,
  plus a void-around-a-guard-inside gate. Win rate up in six of eight profile-blocks; wins
  slower everywhere.
- `5eb8ba7+repel-554` (#554, after the first playtest) — **guard rules reworked and
  re-measured.** A cut-off guard now closes on the boundary rather than freezing where it
  stood, and a guard caught inside walks out by the shortest way; the cue's stale gate came
  off with them. Win rate now up in **all eight** blocks (+0.05 … +0.17) on twice the
  presses, detections down in all four, and the ability has lost its stated failure case —
  see the kill-thresholds and appendix 60 §3 for the repair that would give it back.
