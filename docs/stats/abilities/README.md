# Ability stats — what the sim says about each verb

One file per ability, named for the verb: what its §8.3 row promises, what the sim
bot's **cue** says it is for (`crates/sim/src/cue.rs`), and what a batch measured
when the bot was actually given it. The set is exhaustive over `AbilityId` on
purpose — the same move the cue seam makes in code (#366), so a missing file means
"not an ability", never "nobody got round to it". A verb with no cue still gets a
file; it says so, and says why.

## These are numbers, not verdicts (§13.4)

The bot is a smoke detector, not a fun oracle. It has no fear, perfect recall of
what it has seen, and will take a 5% capture risk forever, so nothing here says an
ability is good, bad, fun or unfun — only what the bot did with it. The honest
output of a page is *"this is what moved; go play these seeds"*.

**And once a cue exists, the histogram measures the cue as much as the ability**
(#347): a low number means "weak ability **or** shy cue", and no batch on this page
can tell those apart. The instrument that can is a sweep of the per-ability cue
floor (`Profile::cue_floor`, #349) — a flat curve exonerates the cue. Until that
lands, read every number here as directional.

## How the numbers are produced

Nothing on these pages is committed as data, and nothing re-runs them — they are a
record of what was measured, not a gate. The gate is
[`baseline.json`](../../../.claude/skills/playtest/baseline.json), which pins the
**innate** batch and is checked by CI on every PR.

That is also what makes the measurement cheap. Each page compares two batches that
differ in exactly one thing:

```
# control: the innate loadout — Run and nothing salvaged
cargo run --release -p intrusion-sim -- --bot --profile <NAME> --runs 100 --seed 0 --cap 1000
# arm: the same batch, holding the one verb under test
cargo run --release -p intrusion-sim -- --bot --profile <NAME> --abilities <VERB> --runs 100 --seed 0 --cap 1000
```

The control side at `--runs 100 --seed 0 --cap 1000` **is** the committed baseline
block, so it costs nothing to state and cannot drift unnoticed. Conventions the
pages follow:

- **All four temperaments** (§13.4: a profile is a temperament, never a skill
  level, and they are never read against each other as a leaderboard). A cue that
  only fires under one of them is a finding.
- **Rates, not raw totals**, wherever a metric scales with run length — `cautious`
  plays much longer runs, so it accumulates detections while being seen *less
  often per turn*.
- **A marginal delta is re-run on a disjoint seed block** (`--seed 100`) before it
  is believed. A 100-seed batch wobbles by several points on its own; more than one
  effect that looked real at `--seed 0` has failed to survive this.
- **Dominance is checked with competition present.** A verb measured alone cannot
  be dominant, so the share that matters is the one from a full three-tech kit
  (§8.3's cap), recorded on the pages that have it.

## The pages

| Ability | Cue | Page |
|---|---|---|
| Run | innate escape, cued | [`run.md`](run.md) |
| Camouflage | cued | [`camouflage.md`](camouflage.md) |
| Decoy | cued | [`decoy.md`](decoy.md) |
| Dephase | cued | [`dephase.md`](dephase.md) |
| Autodoors | cued | [`autodoors.md`](autodoors.md) |
| Confusion | cued | [`confusion.md`](confusion.md) |
| Pierce Wall | cued | [`pierce-wall.md`](pierce-wall.md) |
| Lockdown | **none yet** — deferred, not omitted | [`lockdown.md`](lockdown.md) |
| Vision | **none** — passive, nothing to activate | [`vision.md`](vision.md) |
