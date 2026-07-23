---
name: playtest
description: >-
  Playtest Intrusion via the headless sim to judge balance and find dead/dominant
  strategies. Use when the user wants to playtest, run the sim, get balance metrics,
  check win rate, or evaluate whether an ability/guard change is any good. Runs the
  `crates/sim` headless harness (§13.2) over a batch of seeds, reports the metrics
  against a stored baseline, and flags suspicious seeds for a human to play.
---

# Playtest — drive the headless sim (§13.2–§13.4)

Run the `crates/sim` headless harness over a batch of seeds, read the §13.2
metrics, compare them against the stored baseline, and hand back the seeds worth
playing. The sim boots each run exactly as the web build does, so a seed here is
the same level that seed gives a player (§12.1/§12.4).

> **Flag, never judge (§13.3–§13.4).** The bot is a *smoke detector, not a fun
> oracle.* It has no fear, perfect recall of what it has seen, and will happily
> take a 5% capture risk forever — so its win rate is **not** a difficulty
> verdict and it cannot be bored. The output of a playtest run is always the same
> shape: *"these seeds look suspicious — go play them."* Never conclude from the
> numbers that the game is fun, unfun, too hard or too easy. That is a human
> judgement (§13.1); the sim only narrows *what is worth playing.* This framing is
> the point of the skill, not boilerplate — keep it in the report.

## 1. Build and run

The harness is the `sim` binary in `crates/sim`. Build and run it from the repo
root; release mode matters (a 100-run `--bot` batch is ~16 s released, minutes
in debug):

```
cargo run --release -p intrusion-sim -- --bot --runs 100 --seed 0
```

It prints **one JSON line per run** to stdout, then a final **summary line**
keyed `"summary"`. Capture the stream to a file so you can pick apart the
per-run rows for the seed flags below:

```
cargo run --release -p intrusion-sim -- --bot --runs 100 --seed 0 > /tmp/playtest.jsonl
tail -1 /tmp/playtest.jsonl        # the summary row
```

## 2. Config surface

The flags (full table in [`crates/sim/README.md`](../../../crates/sim/README.md)):

| Flag | Meaning | Default |
|---|---|---|
| `--bot` | play each run with the baseline stealth bot — **the balance-signal mode** | off |
| `--runs N` | how many runs; seeds are `S, S+1, … S+N-1` | 100 |
| `--seed S` | the first seed | 0 |
| `--cap N` | inputs issued per run before it is ruled a `timeout` | 1000 |
| `--script MOVES` | replay a fixed input list every run (`N`/`E`/`S`/`W` step, `.` wait) — a `(seed, script)` replay (§12.4), not a balance signal | empty |

- **Almost always run `--bot`.** Without it the empty default script is the *idle
  baseline* — how often patrols stumble onto a player who never moves — useful as
  a sanity floor, not for balance.
- **Widen the batch to trust a signal.** 100 seeds is enough to smell something;
  bump `--runs` (and vary `--seed`) before you believe a small effect. Seeds are
  contiguous from `--seed`, so `--seed 0 --runs 100` and `--seed 100 --runs 100`
  are disjoint batches.
- **A/B a tuning change is a code change, not a flag.** The `[START]` knobs
  (dwell probability, radio range, ability costs, …) are named constants in the
  core, not runtime flags. To A/B one: run the batch on `main` for the "before",
  change the constant on a branch, rebuild, run the *same* `--seed/--runs/--cap`
  for the "after", and diff the two summaries. Keep everything but the one knob
  fixed or the comparison is noise.

## 3. Read the output

The schema is the contract — documented and pinned byte-for-byte in
[`crates/sim/README.md`](../../../crates/sim/README.md); read it once. The §13.2
metrics and what each catches:

| Summary field | What it catches |
|---|---|
| `win_rate` | difficulty (but see the caveat — a bot win rate is not a player's) |
| `turns_to_win_mean` / `_median` | pacing — the "don't drag exploration" pillar (over winning runs only; `null` if none won) |
| `usage` + `usage_share` | **dominant and dead abilities** — a verb at a huge share of turns is the "used 94% of turns is a scream"; a verb at `0` is never exercised |
| `detections` | whether stealth is actually happening |
| `takedowns` | whether §7.2's cost is real |
| `bodies_found` | whether §7.3's radio clock has teeth |
| `diversity` | **boredom** — mean pairwise distance between runs' usage signatures; near `0` = every run played the same = a puzzle with one answer |
| `alert_peak` | **not measured yet** — always `null`; the facility-wide alert is the radio net's value (#107), which does not exist. A `null` says "not measured" where a `0` would lie. |

Per-run rows carry the same metrics plus the `seed` and `outcome`
(`win`/`capture`/`entombed`/`timeout`) — that is where you find the seeds to flag.

## 4. Compare against the baseline

A snapshot of the `--bot` batch lives beside this skill in
[`baseline.json`](baseline.json): its `command`, the commit it was captured on,
and the `summary` object. Read it and report **deltas** for the headline metrics
(`win_rate`, `timeouts`, `diversity`, each `usage_share`, `detections`,
`takedowns`, `bodies_found`) whenever the current batch used the baseline's
config (same policy, `--runs`, `--seed`, `--cap`). If the configs differ, say the
comparison is not apples-to-apples rather than diffing anyway.

**The baseline is meant to drift — that is its job.** A moved metric is the
signal. So:

- After a change expected to move the numbers (a tuning `[START]` knob, guard /
  vision / ability behaviour, generation, or the bot policy itself), **refresh
  the baseline in the same PR**: re-run its exact `command`, replace the
  `summary` and `captured_at_commit` in `baseline.json`, and commit it. A stale
  baseline compared silently is exactly the anti-pattern this file guards against.
- If the baseline's config no longer matches how the batch is run, update the
  `command`/`config` too.

## 5. The report — flag, never judge

Produce a short report, in this order:

1. **The metrics table** — the current batch's summary next to the baseline, with
   deltas. Include the ability-usage histogram (counts and shares) and the
   diversity score.
2. **Suspicious seeds — "go play these."** Scan the per-run rows and flag seeds
   that trip any of the watermarks below (all `[START]`, tune to taste). For each
   flagged seed give its number, the flag it tripped, and the exact replay
   command. **Never rule** — the human plays them and decides.
3. **The §13.4 disclaimer**, restated: these are numbers, not verdicts.

Watermarks for flagging a seed (or the batch):

- **Dominant ability** — a single *active* verb (anything but `wait`/`move`) at
  more than ~50% of a run's spent turns, or a batch `usage_share` for one active
  verb well above the rest. The 94%-neutralise scream.
- **Dead ability** — a verb at `0` across the whole batch. Note the ambiguity
  honestly: a dead verb may mean a useless ability **or** that the crude bot
  policy simply never reaches for it (§13.4 — the metric can measure the bot, not
  the game). Flag it as "never exercised", not "useless".
- **Win-rate cliff** — the batch `win_rate` far from the baseline, or a run of
  consecutive seeds all ending the same way.
- **Near-zero diversity** — `diversity` collapsing toward `0`: every run played
  the same, a one-answer puzzle.
- **Stall, not play** — a `timeout` whose `turns` are a small fraction of `--cap`
  (the cap counts *issued inputs*, not spent turns): the bot burned its inputs on
  free actions — bumping a wall — instead of playing. This flags the **bot**, not
  the game; call it out as such. (This exact signature was #171, fixed in #175 —
  its return is a bot regression, not a level.)

Each flagged seed replays exactly with `--runs 1`:

```
cargo run --release -p intrusion-sim -- --bot --runs 1 --seed <SEED>
```

The seed is the shareable handle (§13.1): the same seed reproduces the same
facility for a human to play. (Entering a seed in the web shell to play it is
#110; until it lands, the seed reproduces exactly in the sim as above.)

## Example invocation

> **Ask:** "Playtest current `main` — how's the balance looking?"

Run the batch and read the baseline:

```
cargo run --release -p intrusion-sim -- --bot --runs 100 --seed 0 > /tmp/playtest.jsonl
tail -1 /tmp/playtest.jsonl
```

**Batch vs. baseline** (`--bot --runs 100 --seed 0 --cap 1000`, commit `70c0cad`):

| Metric | Baseline | This run | Δ |
|---|---|---|---|
| win_rate | 0.0200 | 0.0200 | 0 |
| captures | 96 | 96 | 0 |
| timeouts | 2 | 2 | 0 |
| detections | 754 | 754 | 0 |
| takedowns | 28 | 28 | 0 |
| bodies_found | 20 | 20 | 0 |
| diversity | 0.4425 | 0.4425 | 0 |

Ability usage (share of turns): `wait 0.25`, `run 0.017`, `camouflage 0.003`,
`takedown 0.002`, **`decoy 0`, `dephase 0`, `drag 0`**.

**Suspicious seeds — go play these:**

- **`decoy`, `dephase`, `drag` never exercised** across all 100 seeds. Flagged as
  *never exercised*, not *useless* — the baseline bot has no logic that reaches
  for them, so this may measure the bot, not the game (§13.4). To rule, a human
  should play seeds where they *should* pay off — e.g. `--seed 4` (a capture with
  a body on the board that Drag would move) — and see.
- **Win-rate floor: 2/100.** Wins are `seed 61` and `seed 97`. Play both to see
  what a completed raid looks like, and a couple of captures (`--seed 4`,
  `--seed 15`) to see where the bot dies.
- **No stall in this batch.** Both timeouts (`seed 2`, `seed 54`) ran the full
  1000 turns — the bot played to the cap rather than burning inputs on free
  actions. The *stall* watermark (a `timeout` whose `turns` are far below `--cap`)
  is what caught #171 before its fix (#175); a future batch that shows it again is
  flagging the **bot**, not the level.

Replay any of them exactly, e.g. `cargo run --release -p intrusion-sim -- --bot
--runs 1 --seed 61`.

**These are numbers, not verdicts (§13.4).** The 2% win rate is the *bot's*, not a
player's, and the three unused abilities are as likely a bot blind spot as a
balance problem. Go play the flagged seeds and rule.

## Caveats to keep in the report

- **`alert_peak` is always `null`** until the radio-net alert value (#107) exists.
- **The bot is deliberately crude** (§13.4): a low win rate and unused abilities
  can be the policy, not the game (its close-behind-door stall was #171, fixed in
  #175, but it is still no substitute for a player). Sharper policies are
  follow-up work; the skill's job is to flag, not to fix the bot.
- **Keep the baseline honest** (§4): refresh it in the same PR as any change that
  moves the numbers, never compare a stale one silently.
