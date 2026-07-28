# Pierce Wall

**Salvaged tech (§8.3)** — 1 turn to press, instant, no cooldown, **a per-level use
budget of 3**. *"Bore straight through your one adjacent wall, permanently."* Usable
only when **exactly one** of your four neighbours is a wall, so the target is unique
by precondition and there is nothing to aim (§8.4). It does not ask what is behind:
boring a two-cell-thick run opens a one-cell **pocket**, not a route — *"a dead-end
alcove out of the through-routes is somewhere to sit a sweep out"*. It conceals
nothing. The hole is real terrain: guards route through it and see through it, for
the rest of the level.

## What the cue says

`crates/sim/src/cue.rs`. #347 names the failure mode this cue has to avoid: *"a cue
that spends the budget on the first legal wall makes the histogram look healthy
while measuring nothing"*.

- **Never while hunted.** A hole is not a cupboard and conceals nothing, so it is
  never an answer to being seen.
- **The wall the route wants, not merely a legal one.** Core owns the target — it is
  unique by precondition — and the cue's only job is to check that the unique answer
  and the crossing the router would actually use are the same wall. No crossing, no
  bid.
- **The budget sets the price.** A crossing must save `BORE_MARGIN` (18 **[START]**)
  of the router's own cost — three times what Dephase asks for the identical
  crossing. The two abilities answer the same question out of different pockets:
  Dephase spends a cooldown that comes back, the borer spends one of three permanent
  uses that does not. That ordering is also what stops the two cues bidding for the
  same ordinary shortcut.

**The pocket use is deliberately not cued.** §8.3 offers a second, real use — bore a
dead-end alcove and sit a sweep out in it — and this cue is silent about it. Judging
"out of the through-routes" means knowing where guards patrol, which the bot may not
know (§11.5a), and the alcove conceals nothing, so a wrong guess is a hole to be
cornered in. The numbers below therefore bound Pierce Wall **as a shortcut**, not
what the ability is worth.

## What the sim measured

```
cargo run --release -p intrusion-sim -- --bot --profile <NAME> --runs 100 --seed 0 --cap 1000
cargo run --release -p intrusion-sim -- --bot --profile <NAME> --abilities pierce-wall --runs 100 --seed 0 --cap 1000
```

Measured at `a010a3b+pierce-wall-cue-347`.

| Metric | baseline | cautious | aggressive | careless |
|---|---|---|---|---|
| `win_rate` | 0.38 → **0.40** | 0.60 → **0.61** | 0.49 → **0.52** | 0.49 → **0.50** |
| `turns_to_win_median` | 117.0 → **117.0** | 183.5 → **178.0** | 111.0 → **111.0** | 116.0 → **108.5** |
| detections / turn | 0.0527 → **0.0521** | 0.0246 → **0.0256** | 0.0605 → **0.0608** | 0.0704 → **0.0706** |
| `diversity` | 0.6918 → **0.7062** | 0.5399 → **0.5341** | 0.6187 → **0.6328** | 0.5835 → **0.6181** |
| `timeouts` | 3 → **3** | 2 → **1** | 2 → **2** | 1 → **1** |
| `usage.pierce_wall` | **12** | **23** | **8** | **13** |
| `usage_share.pierce_wall` | **0.0008** | **0.0011** | **0.0006** | **0.0011** |

Disjoint block, `--seed 100 --runs 100`:

| Metric | baseline | cautious | aggressive | careless |
|---|---|---|---|---|
| `win_rate` | 0.44 → **0.42** | 0.62 → **0.61** | 0.40 → **0.39** | 0.46 → **0.48** |
| detections / turn | 0.0406 → **0.0446** | 0.0239 → **0.0255** | 0.0660 → **0.0545** | 0.0590 → **0.0538** |
| `diversity` | 0.6093 → **0.6198** | 0.4413 → **0.4702** | 0.6043 → **0.6435** | 0.5455 → **0.5873** |
| `usage.pierce_wall` | **21** | **24** | **15** | **18** |
| `usage_share.pierce_wall` | **0.0015** | **0.0009** | **0.0014** | **0.0016** |

## The reading

- **The slot is non-zero, and that is most of what this measurement proves.** 8–24
  bores per 100-run batch, 0.06%–0.16% of spent turns — the smallest share of the
  five cues by some way.
- **The cue is shy, and it is worth saying so plainly.** Only 11 of 100 `baseline`
  runs bored at all, and no run spent more than its three uses (most spent one). So
  the honest statement is not "Pierce Wall is a small ability" but *"this cue asks a
  high price and the facility rarely offers it"* — the exact "weak ability **or** shy
  cue" ambiguity #347 warns the histogram now carries. `BORE_MARGIN` is a **[START]**
  knob and the instrument that would settle it is the cue-floor sweep (#349), not a
  quiet edit to the constant here.
- **Nothing else moved.** Win rate within ±0.03 in both directions across the two
  blocks; detections per turn flat at seed 0 and mixed at seed 100. `diversity` is up
  in seven of eight profile-blocks, the same mild direction Dephase moved it, which
  makes sense — both are shortcuts through geometry, so both make routes differ more
  between runs.
- **No pockets, no bores while hunted.** The invariant test re-derives both from the
  state over 40 seeds × 4 profiles rather than trusting the cue.

### What this measurement found in the code

Pierce Wall's own test caught a bug in the crossing that Dephase shares: the router
seeds its **goals** into the cost field whether or not they can be stood on — a
console is solid (§4.3) — so a wall backing a console looked like a crossing with a
walkable far side. Pierce Wall would have bored into a pocket and called it a route;
Dephase would have phased into the console and been ejected on expiry, at a stun as
long as the throw. The crossing now requires the far side to be walkable floor, and
`dephase.md`'s numbers were re-measured under the fix.

## History

- `a010a3b+pierce-wall-cue-347` (#347) — cue written; slot went from a structural zero
  to 8–24 bores per 100-run batch. No headline metric moved; the cue is shy by
  construction and the shy-versus-weak question is left open for #349 rather than
  tuned away.
