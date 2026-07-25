# intrusion-sim — the headless harness (design §13.2)

Runs *N* seeded games natively — no browser, no canvas — with a player policy
behind a trait, and emits machine-readable metrics. A run boots exactly as the
web build does (`Rng::new(seed)` → `generate_level(V1)` → `State::new`), so a
seed here is the same level that seed gives a player, and every metric is
counted from the core's `Event` stream (§12.1), never scraped from state or
the rendered grid.

The sim reports **numbers, never verdicts** (§13.4): it is a smoke detector,
not a judge.

## Running

```
cargo run --release -p intrusion-sim -- [--runs N] [--seed S] [--cap N] [--bot | --script MOVES] [--emit-replay]
```

| Flag | Meaning | Default |
|---|---|---|
| `--runs N` | how many runs; seeds are `S, S+1, … S+N-1` | 100 |
| `--seed S` | the first seed | 0 |
| `--cap N` | inputs issued per run before it is ruled a `timeout` | 1000 |
| `--bot` | play each run with the baseline stealth bot instead of a script | off |
| `--script MOVES` | inputs replayed from the start of every run (notation below); after the script the player waits out the run | empty |
| `--emit-replay` | capture one run (seed `S`) and print its `(level, inputs)` replay — the `seed` field a level-seed string (#245) — instead of the metrics batch | off |

### The script notation

One input per token, the exact string `--emit-replay` prints back:

| Token | Input |
|---|---|
| `N` / `E` / `S` / `W` | step north/east/south/west (case-insensitive) |
| `.` | wait |
| `+<key>` | activate the ability with §11.6 hotkey `<key>` — `+r` Run, `+c` Camouflage, `+d` Decoy, `+x` Dephase |
| `-<key>` | deactivate that ability |

The takedown-bump and the drag-grab are steps *into* a target (§7.2/§8.3), so
they need no token of their own — `N`/`E`/`S`/`W` already spell them. Whitespace
between tokens is ignored, so a long captured stream can be wrapped for reading.
An unknown token, or a `+`/`-` with no ability key, is a hard error: a malformed
replay never silently drops an input (§12.4).

The cap counts **issued inputs**, not spent turns: free actions (a bump into a
wall, an idle deactivate) never advance the turn counter (§4.4), so a
turn-based cap could hang on a stuck policy; an input cap terminates every run.

`--bot` is the **balance-signal mode**: instead of a fixed script, every run is
played by the baseline stealth bot (below), so the metrics describe a facility
someone is actually *raiding*. `--bot` and `--script` are mutually exclusive.
Without either, the empty default script is the idle baseline — how often
patrols stumble onto a player who never moves. A `(seed, script)` pair is a
replay (§12.4): with `--runs 1` it reproduces one run exactly, which is also
the bug-report format.

## Capturing a replay (`--emit-replay`, §12.4)

A replay is `(level, [inputs])` (§12.4): the reproducible unit widened from a bare
seed to the whole config `(seed, modifiers, abilities)` once modifiers (#225) and a
seeded loadout (#244) shaped a run. `--emit-replay` plays one run — seed `S`, the
chosen policy — records the exact input stream it issued, and prints that pair on
stdout as one JSON line. The `seed` field is the run's **level-seed string** (#245),
so a baked replay reproduces the captured preset — the sim's `AtLeastOne` intel gate,
not quick play's stricter one — and not just the geometry:

```
$ cargo run --release -p intrusion-sim -- --bot --seed 42 --emit-replay
{"seed":"L1-42-4-rcdxa","inputs":"NNE+rN..SS-r…"}
seed 42: win in 214 turns, 187 inputs          # (human summary, on stderr)
```

The `inputs` string is the script notation above, so it feeds straight back:
`--script "$(…)" --seed 42 --runs 1` reproduces the run byte-for-byte, and the
same pair is what the web replay-viewer plays and what an Artifact bakes in
(#197/#245). The `seed` field is a level-seed string the core decodes
(`LevelSeed::decode`) — a bare seed still decodes (to quick play), so an older
bare-number replay stays loadable. stdout carries only the machine-readable pair (the summary goes to
stderr), so it pipes cleanly into a consumer. The round-trip is asserted
natively in `src/replay.rs` — capturing a bot run and replaying the emitted
stream lands on an identical record — which is the §12.4 determinism property
end to end.

## The baseline stealth bot (`--bot`, §13.2–§13.4)

A greedy [`StealthBot`](src/bot.rs) that plays each run through the **same
information a player is shown** — never the raw state. It is a *smoke detector,
not a good player* (§13.4): the point is that it plays legibly and the same way
every seed, so the numbers measure the game, not a hand-tuned solver.

- **Geometry** (walls, floors, doors) is always known, and so is the **exit** —
  it is the player's own tunnel. **Intel** is fogged: the bot cannot route to a
  console it has never seen (`memory`, §11.5a), so it *explores* to find them.
- **Guards** are read through `perceive_guard` (§9.2): it routes around the
  cones of guards it can *see* (the danger overlay, §11.5) and keeps clear of
  the bare dots of guards it can only *sense*.
- **Loop:** explore → take each intel → leave by the exit, ducking into a
  hideout (or cloaking with Camouflage) when a patrol closes, and fleeing to
  cover when hunted. It uses Run to open a gap, a takedown to clear a guard
  blocking the only way, and hideouts/Camouflage to wait a hunt out — so the
  ability histogram has something real to measure.

It plays nothing like a human — no fear, perfect recall of what it has seen —
so its win rate is not a difficulty verdict (§13.4). **Flag, never judge:** a
histogram spike or a win-rate cliff under `--bot` is a seed to go *play*, not a
ruling. The bot is deliberately crude; sharper policies are follow-up work.

## Output schema

One JSON object per line on stdout: one **run row** per run, then one final
**summary row**. This schema is what the playtest skill parses — field order
is fixed, the tests in `src/report.rs` pin it byte-for-byte, and any change to
it is a deliberate, visible break.

### Run row

```json
{"seed":17,"outcome":"win","turns":214,"detections":2,"takedowns":1,"bodies_found":0,"usage":{"wait":90,"run":6,"camouflage":2,"decoy":0,"dephase":1,"autodoors":0,"takedown":1,"drag":1},"alert_peak":null}
```

| Field | Meaning |
|---|---|
| `seed` | the run's seed — with the script, the whole replay |
| `outcome` | `"win"` \| `"capture"` \| `"entombed"` \| `"timeout"` |
| `turns` | spent turns at the end of the run (free actions excluded) |
| `detections` | fresh detections (`Event::Detected`): how often stealth broke — a held chase counts once, not once per turn |
| `takedowns` | takedowns landed (`Event::TakenDown`) |
| `bodies_found` | bodies found by guards (`Event::BodyFound`) |
| `usage` | the **ability-usage histogram** (§13.2): a count per verb spent this run. Keys, in fixed order: `wait`, `run`, `camouflage`, `decoy`, `dephase`, `autodoors`, `takedown`, `drag`. Counted from core events — a *refused* activation costs no turn and emits none, so it never counts (§4.4); `wait` is the one verb with no event of its own and is counted from its spent turn. `Move` is not counted (it is the default nothing-else verb). The counts sum to `≤ turns` |
| `alert_peak` | **always `null` for now**: the facility-wide alert is the radio net's value (#107), which does not exist yet — `null` says "not measured", where a `0` would lie that it was quiet |

### Summary row

```json
{"summary":{"runs":100,"wins":3,"captures":90,"entombed":0,"timeouts":7,"win_rate":0.0300,"turns_to_win_mean":211.5,"turns_to_win_median":208.0,"detections":312,"takedowns":45,"bodies_found":12,"usage":{"wait":9000,"run":600,"camouflage":120,"decoy":20,"dephase":80,"autodoors":0,"takedown":45,"drag":40},"usage_share":{"wait":0.8500,"run":0.0567,"camouflage":0.0113,"decoy":0.0019,"dephase":0.0076,"autodoors":0.0000,"takedown":0.0043,"drag":0.0038},"diversity":0.1837,"alert_peak":null}}
```

`win_rate` is over all runs; `turns_to_win_mean`/`_median` are over the
*winning* runs only and `null` when nothing won. `detections`/`takedowns`/`bodies_found`
are batch totals of the per-run metrics.

The §13.2 signature metrics (#137):

- `usage` — the ability-usage histogram summed across every run (same keys and
  order as the run row). A dominant ability (or a dead one) is legible here.
- `usage_share` — each verb's **share of turns**: its batch count over the
  batch's total spent turns (the "used 94% of turns is a scream" number). Shares
  need not sum to 1, since a `Move` turn is counted for no verb.
- `diversity` — the batch **strategy-diversity** score `[START]`: the mean
  pairwise Euclidean distance between the runs' L1-normalised usage signatures.
  `0` when every run played identically, larger as strategies spread — win rate
  says whether the game is *hard*, diversity whether it is *interesting* (§13.2).

Both the signature (normalised usage vector) and the diversity distance are
`[START]` definitions, named in `src/usage.rs` so they are easy to swap.

**Flag, never judge (§13.4):** these are numbers, not verdicts. A histogram spike
or a near-zero diversity is a seed to *go play*, not a ruling that the game is
broken — the playtest skill (#140) owns that framing.

## Determinism

Same `(seed, policy)` → byte-identical rows, asserted in `src/harness.rs`.
That property is what makes the batch a regression instrument: same seeds +
same script producing different rows means the game changed.
