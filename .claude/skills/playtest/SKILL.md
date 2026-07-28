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
| `--profile NAME` | which **playstyle temperament** the bot plays: `baseline`, `cautious`, `aggressive` (needs `--bot`) | `baseline` |
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
- **Run all three profiles by default.** They are one policy at three
  temperaments (`crates/sim/README.md`), and running them over the *same* seeds is
  the only way §13.2's headline metric — strategy diversity — becomes visible: a
  seed solvable both cautiously and aggressively is healthy; one where both
  collapse onto the same line is a puzzle with one answer.

  ```
  for p in baseline cautious aggressive; do
    cargo run --release -p intrusion-sim -- --bot --profile $p --runs 100 --seed 0 \
      > /tmp/playtest-$p.jsonl
    tail -1 /tmp/playtest-$p.jsonl
  done
  ```

  One batch per profile, each row and summary self-describing via its `profile`
  field. A single-profile run is fine when you are chasing one specific signal —
  say so in the report rather than leaving it implied.

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

Per-run rows carry the same metrics plus the `seed`, the `profile` that played
them, and the `outcome` (`win`/`capture`/`entombed`/`timeout`) — that is where you
find the seeds to flag.

### Reading the profiles against each other

**Never as a leaderboard (§13.4).** A profile is a *temperament*, not a better
player: `cautious` is meant to be slow and `aggressive` is meant to be seen more.
"Aggressive wins less" is not a finding. What the comparison is *for*:

- **Where the temperaments disagree on a seed** — one wins by waiting, the other
  is captured pushing. That is the §13.3 flag worth playing: the seed admits more
  than one line, and a human can rule on whether both are interesting.
  `join`-ing the per-profile JSONL files on `seed` and diffing `outcome` is the
  quickest way to find them.
- **Where they collapse onto the same line** — same outcome, near-identical usage
  signatures, across most seeds. That is the "puzzle with one answer" smell, and
  it is a batch-level flag, not a per-seed one.
- **Whether a change moved them the same way.** A tuning knob that helps one
  temperament and hurts another is a more interesting finding than one that shifts
  all three together.

Rates, not raw totals, when a metric scales with run length: `cautious` plays much
longer runs, so it accumulates more `detections` while being seen *less often per
turn*. Divide by the batch's total spent turns before comparing.

## 4. Compare against the baseline

A snapshot of the `--bot` batch lives beside this skill in
[`baseline.json`](baseline.json): the commit it was captured on, and — under
`profiles` — one `command` + `summary` block **per playstyle profile**. Read it and
report **deltas** for the headline metrics (`win_rate`, `timeouts`, `diversity`,
each `usage_share`, `detections`, `takedowns`, `bodies_found`) whenever the current
batch used the baseline's config (same policy, profile, `--runs`, `--seed`,
`--cap`). Each profile diffs against **its own** block — never against another
profile's, which would be the leaderboard reading §13.4 forbids. If the configs
differ, say the comparison is not apples-to-apples rather than diffing anyway.

**The baseline is meant to drift — that is its job.** A moved metric is the
signal. So:

- After a change expected to move the numbers (a tuning `[START]` knob, guard /
  vision / ability behaviour, generation, the bot policy itself, or a profile's
  numbers), **refresh the baseline in the same PR**:

  ```
  ./scripts/baseline.py --refresh      # re-runs every profile, rewrites the file
  ```

  It re-runs each block's own recorded `command`, replaces every `summary` and the
  `captured_at_commit`, and prints the deltas it wrote — so read them, then commit
  the file with the change that moved them. Never hand-edit the JSON: the script
  refreshes **all** the profile blocks together, and a file where one temperament
  is current and the others are months stale is worse than one that is uniformly
  old. A refresh that finds nothing moved writes nothing, so a no-op leaves no
  diff to review.

  The stamp defaults to the short HEAD sha. Since a refresh usually runs *before*
  the commit that moved the numbers exists, `--at` writes the file's existing
  "HEAD plus the pending work" convention — `--at search-duration-12` records
  `44d5e28+search-duration-12`.
- If the baseline's config no longer matches how the batch is run, update the
  `command`/`config` too — those the script reads rather than writes, so they stay
  the human's to state.

**CI enforces this** (`.github/workflows/ci.yml`, the `simbot baseline` job): every
PR re-runs the three batches and fails if any summary differs from the committed
one. That red tick is not "you broke the game" — it is "the numbers moved and the
snapshot has not caught up". Read the deltas, decide whether they are the change
you meant to make, and refresh. Run it locally before pushing with:

```
./scripts/baseline.py                 # exit 1 and a field-by-field diff on drift
```

## 5. The report — flag, never judge

Produce a short report, in this order:

1. **The metrics table** — one column per profile run, each next to *its own*
   baseline block, with deltas. Include the ability-usage histogram (counts and
   shares) and the diversity score. Side by side is for reading the temperaments
   against each other, never for ranking them.
2. **Where the profiles split** — the seeds two temperaments ended differently on
   (§13.3's flag), and, at batch level, whether they collapsed onto the same line.
3. **Suspicious seeds — "go play these."** Scan the per-run rows and flag seeds
   that trip any of the watermarks below (all `[START]`, tune to taste). For each
   flagged seed give its number, the profile it was played under, the flag it
   tripped, and the exact replay command. **Never rule** — the human plays them and
   decides.
4. **The §13.4 disclaimer**, restated: these are numbers, not verdicts.

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
- **Temperament-blind seeds** — a seed every profile ends the same way, with
  near-identical usage. One or two is nothing; a batch where it is the rule says
  the facility admits one line however you play it. This is the *cross-profile*
  form of the diversity flag, and the reason to run more than one profile at all.
- **Stall, not play** — a `timeout` whose `turns` are a small fraction of `--cap`
  (the cap counts *issued inputs*, not spent turns): the bot burned its inputs on
  free actions — bumping a wall — instead of playing. This flags the **bot**, not
  the game; call it out as such. (This exact signature was #171, fixed in #175 —
  its return is a bot regression, not a level.)

Each flagged seed replays exactly with `--runs 1`, under the profile that flagged
it (a seed reproduces the same *facility* whatever the temperament, but only the
same *run* under the same one):

```
cargo run --release -p intrusion-sim -- --bot --profile <NAME> --runs 1 --seed <SEED>
```

The seed is the shareable handle (§13.1): the same seed reproduces the same
facility for a human to play. (Entering a seed in the web shell to play it is
#110; until it lands, the seed reproduces exactly in the sim as above.)

## Example invocation

> **Ask:** "Playtest current `main` — how's the balance looking?"

Run the three temperaments over the same seeds and read each against its own
baseline block:

```
for p in baseline cautious aggressive; do
  cargo run --release -p intrusion-sim -- --bot --profile $p --runs 100 --seed 0 \
    > /tmp/playtest-$p.jsonl
  tail -1 /tmp/playtest-$p.jsonl
done
```

**Batch vs. baseline** (`--runs 100 --seed 0 --cap 1000`, commit `802c372+profiles`),
each profile against its own block — **not** against each other:

| Metric | baseline | cautious | aggressive |
|---|---|---|---|
| win_rate | 0.4400 (Δ0) | 0.4800 (Δ0) | 0.3300 (Δ0) |
| timeouts | 2 (Δ0) | 2 (Δ0) | 0 (Δ0) |
| turns_to_win_median | 131.5 | 199.0 | 138.0 |
| detections | 525 | 756 | 553 |
| detections / turn | 0.039 | 0.032 | 0.060 |
| `wait` share | 0.193 | 0.330 | 0.053 |
| diversity | 0.6480 | 0.4406 | 0.6739 |

The temperaments read as intended: `cautious` spends a third of its turns waiting
and is seen least *per turn* while taking half again as long to win; `aggressive`
barely waits, finishes fastest, and is seen most often per turn. Neither is
"better" (§13.4).

**Where the profiles split:** 38 of the 100 seeds ended differently under
`cautious` and `aggressive` (seeds 1, 2, 3, 5, 7, 9, 10, 11, 14, 20, …) — those
are the §13.3 flags, seeds that admit more than one line. Play a couple to see
whether both lines are actually interesting. Far more telling than the win rates:
a batch where that number collapsed toward zero would be the one-answer smell.

**Suspicious seeds — go play these:**

- **Every ability but `wait`/`run` never exercised** across all three profiles.
  Flagged as *never exercised*, not *useless* — the sim boots the bare
  innate-only loadout and the bot's policy reaches for nothing else, so this
  measures the bot, not the game (§13.4). Teaching it a cue per ability is #346/#347.
- **`takedowns 0` under every temperament.** Deliberate takedown play is #316's
  ticket; until it lands the row honestly reads zero rather than measuring a
  contrived hunt.
- **No stall in any batch.** The four timeouts ran the full turn budget rather than
  burning inputs on free actions. The *stall* watermark (a `timeout` whose `turns`
  are far below `--cap`) is what caught #171 before its fix (#175); a future batch
  that shows it again is flagging the **bot**, not the level.

Replay any of them exactly, e.g. `cargo run --release -p intrusion-sim -- --bot
--profile cautious --runs 1 --seed 61`.

**These are numbers, not verdicts (§13.4).** The win rates are the *bot's*, not a
player's, and the unused abilities are a bot blind spot before they are a balance
problem. Go play the flagged seeds and rule.

## Caveats to keep in the report

- **`alert_peak` is always `null`** until the radio-net alert value (#107) exists.
- **The bot is deliberately crude** (§13.4): a low win rate and unused abilities
  can be the policy, not the game (its close-behind-door stall was #171, fixed in
  #175, but it is still no substitute for a player). Sharper policies are
  follow-up work; the skill's job is to flag, not to fix the bot.
- **A profile is a temperament, not a skill level** (§13.4). Three profiles widen
  what the smoke detector can see; they do not turn the bot into a judge, and
  ranking them ("aggressive is worse") is a reading the numbers do not support.
- **Keep the baseline honest** (§4): refresh it in the same PR as any change that
  moves the numbers, never compare a stale one silently. CI's `simbot baseline`
  job checks this on every PR, so a forgotten refresh is a red tick rather than a
  quiet comparison against months-old numbers.
