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
cargo run --release -p intrusion-sim -- [--runs N] [--seed S] [--cap N] \
    [--config TOKEN] [--guards N] [--intel-gate none|one|all] [--modifier NAME]... \
    [--abilities LIST] [--without LIST] \
    [--bot [--profile NAME] | --script MOVES] [--emit-replay]

cargo run --release -p intrusion-sim -- --inspect LINK
```

| Flag | Meaning | Default |
|---|---|---|
| `--runs N` | how many runs; seeds are `S, S+1, … S+N-1` | 100 |
| `--seed S` | the first seed | 0 |
| `--cap N` | inputs issued per run before it is ruled a `timeout` | 1000 |
| `--bot` | play each run with the baseline stealth bot instead of a script | off |
| `--profile NAME` | which **playstyle profile** the bot plays (below); needs `--bot` | `balanced` |
| `--script MOVES` | inputs replayed from the start of every run (notation below); after the script the player waits out the run | empty |
| `--emit-replay` | capture one run (seed `S`) and print its `(level, inputs)` replay on stdout — the `seed` field a level-seed token (#245) — instead of the metrics batch; a play link goes to stderr with the summary | off |
| `--inspect LINK` | read a replay someone pasted and narrate it turn by turn (below) — a mode of its own, taking no other flag | — |

And the **run config** (below): what every run of the batch boots from.

| Flag | Meaning | Default |
|---|---|---|
| `--config TOKEN` | a **level-seed token** (#245): the batch runs that token's modifiers and loadout, and its seed is the first seed unless `--seed` says otherwise | the sim preset |
| `--guards N` | guards to place per facility — the §10.2 recipe knob the balance sweep drives (all else stays v1) | 4 |
| `--intel-gate G` | how much intel the exit asks for (§4.5): `none`, `one`, `all` | `one` |
| `--modifier NAME` | switch a level modifier on (#225) — repeatable, and comma-separated | none on |
| `--alert NAME=N` | set one §7.3 **alert-ladder threshold** (#376) — how hard a rung is to reach; repeatable, and comma-separated | the §7.3 `[START]`s |
| `--abilities LIST` | the tech every run holds (§8.3), comma-separated | none — bare |
| `--without LIST` | tech to drop from the loadout | none |

### The run config — what a batch is measuring (#256)

A batch is *N* seeds over **one** config, so the config is the thing the batch is
asking about and the seed is what varies underneath it. The default is the **sim
preset** (§13.3): the v1 recipe, the baseline rules with the intel gate at
`AtLeastOne`, and the **bare, innate-only** loadout — a win rate that means the
core stealth loop is winnable with no tech at all. Every flag above states a
departure from it.

They compose in a **fixed order**, whatever order they are written in — `--config`,
`--guards`, `--intel-gate`, `--modifier`, `--alert`, `--abilities`, `--without` — so two
command lines naming the same flags describe the same batch. `--abilities` states
the *whole* tech set rather than adding to what a `--config` preset held, and
`--without` runs last, so it can never be undone by an earlier-resolved flag.

Names are matched by what they say: case, spaces, hyphens and underscores are
noise, so `pierce-wall`, `Pierce Wall` and `piercewall` are the same ability, and a
modifier can be typed the way the source spells its field or the way the flag does.
`--help` lists every value, read off the catalog rather than hand-copied, so a
newly shipped ability is spellable the day it lands. The modifier names are the
`LevelModifiers` field names in kebab case:

```
guards-always-search-hideouts   sighting-lost-calls-a-guard   body-found-calls-two-guards
always-show-vision-cones        layout-knowledge-full         layout-knowledge-none
automatic-doors                 guard-count-more              guard-count-fewer
```

`layout-knowledge-full` / `layout-knowledge-none` (#307/#233) are a knob's two ends
like the guard count below, over how much of the building a run is given before
setting foot in it. **The bot cannot honestly play the `none` end**: it is granted
geometry unconditionally ([`docs/bot-behaviour.md`](../../docs/bot-behaviour.md) §2, on
the authority of the §11.5a rule that end overrides), so it routes through walls it has
never seen. Naming it is useful for `--inspect` and for a replay a human plays; a batch
that names it measures the bot rather than the game (§13.3).

`automatic-doors` and the two `guard-count-*` ends are read by **generation** rather
than at runtime, and they are not read at the same depth (§12.6):

- `automatic-doors` (#452) decides whether a doorway is a hinged manual door or a
  frameless automatic one, so it reaches the **carve**: a batch that names it plays a
  *different facility* from the same seed. Both halves of a comparison are still the
  same seed sweep, which is what keeps it fair; just do not read a per-seed row across
  the two arms as if it were the same level.
- `guard-count-more` / `guard-count-fewer` (#232) are the two ends of one **knob** — a
  knob is not a toggle, so the name has to say which end, and naming both leaves the
  one named last. They reach **placement** only, so both arms carve the same building
  and place the same player, exit and intel; the guard sets are nested and only the
  pieces drawn after the guards (the comms console, the radio clocks) shift. A per-seed
  row *is* comparable across these two arms, up to that shift.

`--guards N` still exists and is a different thing: it overrides the **recipe** to any
count, where the knob is a bounded ±1 step that a real run can actually be dealt.

Two refusals are deliberate, and both are the §13.2 attribution rule — *a batch
whose rows claim a config it never ran is worse than a batch that did not start*:

- **A malformed `--config` token is a hard error**, with the usage text. The web
  seed surface falls gracefully to a fresh run on a bad token (#110); a batch must
  not, because nothing downstream would notice.
- **A loadout over the §8.3 cap** (`AbilityId::MAX_TECH_HELD`, three tech) is
  refused at the flag. It is not a run the game can produce — the ability bar is
  not sized for it (§11.4) and the level-seed token cannot carry it, so
  `--emit-replay` would print a replay that decodes to nothing. `--without` naming
  an **innate** ability is refused for the same reason: §8.3 makes the innate set
  unconditional and the token cannot describe its absence.

#### Sweeping the alert ladder (`--alert`, §7.3/#376)

Every threshold the alert ladder is built from is a `[START]` the design expects to
move, and until #376 moving one meant editing a constant and rebuilding — which is why
nothing had ever measured what any of them do. The knobs, spelled as the
`AlertTuning` field names in kebab case:

```
sighting-contact-turns        turns of certain-zone contact that make one sighting
sighting-window-turns         the sliding window they must fall inside
sightings-for-second-rung     sightings that reach rung 2
silent-posts-for-third-rung   quiet posts (bodies, not pings) that reach rung 3
dwell-turns-min               the shortest Calm dwell from rung 1 up
dwell-turns-max               the longest
rung-two-reinforcements       guards that walk in on reaching rung 2 (#374)
rung-three-reinforcements     …and on reaching rung 3, on top of rung 2's
```

**Sweep the triggers, not the counts.** §10.2's `--guards` sweep puts one guard at
~8–10 points of win rate, so the reinforcement counts are coarse by construction: the
interesting shape is in how hard each rung is to *reach*.

A sweep is then a shell loop, one batch per point on the curve:

```
for w in 4 6 8 10 14 20; do
  cargo run --release -p intrusion-sim -- --bot --profile careless --runs 100 --seed 0 \
    --alert sighting-window-turns=$w | tail -1
done
```

Two refusals, both the §13.2 rule that a batch measuring a game the design forbids
answers nothing: an **unknown knob** is refused with the vocabulary rather than
ignored (a silently-dropped knob would report a flat curve for a threshold that never
moved), and a **ladder §7.3/§7.5 forbids** — a dwell floor of `0`, which would delete
the Takedown window for the rest of every level; a window too short to ever hold a
sighting — is refused at the flag. The check runs once, after every flag is in, so a
dwell range spelled across two knobs is not rejected halfway through being written.

The tuning is **not** carried by a `--emit-replay` token: no shared config can encode
it (§12.4/#245), so a swept run reproduces only under the same `--alert` — the same
honest gap `--guards` has.

Measuring one toggle is then one batch against another over the same seeds:

```
cargo run --release -p intrusion-sim -- --bot --runs 100 --seed 0 | tail -1
cargo run --release -p intrusion-sim -- --bot --runs 100 --seed 0 --abilities decoy | tail -1
```

Generation is seed-derived and independent of modifiers and loadout (proven in
core's `level_seed.rs`), so both arms raid the **same facilities** — the delta is
about the toggle rather than about batch-to-batch luck. Running the two arms and
reading the delta *for* you is the A/B mode (#257); this ticket is the config the
arms are made of. Reading a delta is still §13.4: a number, not a verdict.

### The script notation

One input per token, the exact string `--emit-replay` prints back:

| Token | Input |
|---|---|
| `N` / `E` / `S` / `W` | step north/east/south/west (case-insensitive) |
| `.` | wait |
| `+<letter>` | activate the ability with script letter `<letter>` — `+r` Run, `+c` Camouflage, `+d` Decoy, `+x` Dephase |
| `-<key>` | deactivate that ability |

The takedown-bump, the drag-grab, the crouch, the body-stow and the comms-silence are steps *into* a
target (§7.2/§8.3/§10.3), so they need no token of their own — `N`/`E`/`S`/`W`
already spell them. Whitespace
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
stdout as one JSON line. The `seed` field is the run's **level-seed token** (#245),
carrying the config the run was **actually played under** — so a non-default batch's
replay reproduces rather than approximates, and the default one reproduces the sim's
`AtLeastOne` intel gate rather than quick play's stricter one:

```
$ cargo run --release -p intrusion-sim -- --bot --seed 42 --emit-replay
{"seed":"nfdxttsytrdorexcqn","inputs":"NNE+rN..SS-r…"}
seed 42: win in 214 turns, 187 inputs          # (human summary, on stderr)
play: https://tk-auto.github.io/intrusion/#seed=nfdxttsytrdorexcqn
```

The **play link** on stderr is for a person, not a pipe (§13.1/#572): it opens the
facility this run was measured on in the published build, so a flagged seed is one
click from being played rather than a number to be typed in somewhere. Nowhere in the
game takes a typed token any more — sharing is the URL — which is why the sim prints
one wherever it hands a run to a human. (For the *run* rather than the level, build a
`…#seed=<token>&inputs=<script>` link out of the stdout pair.)

The only ability in that stream is `r` (Run), because the default config boots the
**bare, innate-only** loadout (§8.3) — a level must be winnable with no salvaged
tech is the baseline the bot's win rate is measured against. Under
`--abilities camouflage` the token names *that* run instead, and the captured
stream can press `+c`.

The one thing the token does **not** carry is the facility **recipe**: `--guards` is
a §10.2 knob, not part of the shareable config, so a replay captured off a swept
guard count only reproduces under the same `--guards` (the web viewer plays v1).

The `inputs` string is the script notation above, so it feeds straight back:
`--script "$(…)" --seed 42 --runs 1` reproduces the run byte-for-byte, and the
same pair is what the web replay-viewer plays and what an Artifact bakes in
(#197/#245). The `seed` field is a level-seed token the core decodes
(`LevelSeed::decode`) — one 18-letter form carrying the captured preset, so a
replay is never played back against a config that drifted underneath it. stdout carries only the machine-readable pair (the summary goes to
stderr), so it pipes cleanly into a consumer. The round-trip is asserted
natively in `src/replay.rs` — capturing a bot run and replaying the emitted
stream lands on an identical record — which is the §12.4 determinism property
end to end.

## Inspecting a pasted replay (`--inspect`, §12.4/#411)

The read half of `--emit-replay`. A replay travels as
`seed=<token>&inputs=<script>`, and both the sim and the browser can now *write*
one — the help panel's `replay [r]` control copies the run a player just had.
`--inspect` takes one back and says what they did:

```
$ cargo run --release -p intrusion-sim -- --inspect '…#seed=hwqcwzlhzanrdsdfzd&inputs=NNNNNEEEEEESS'
level  hwqcwzlhzanrdsdfzd  (seed 18900)
play   https://tk-auto.github.io/intrusion/#seed=hwqcwzlhzanrdsdfzd
rules  Intel to exit: all of it
tech   Run, Camouflage, Decoy, Pierce Wall
start  (31,12) facing north, 13 input(s) to replay

  1. N   (31,12) → (31,11)
  …
 12. S   (37, 7) → (37, 8)   DoorOpened { at: (21,26), by_player: false }
 13. S   (37, 8) —  stayed   the exit needs 3 more intel

<the frame it ended on>

13 turn(s) played, still playing
```

**Take the link as pasted.** A whole URL, or just its fragment; `#` or `?`; either
field order; a host's own query in front of it. Cutting the fragment out by eye is
exactly the step that gets done wrong, so the parser does it
(`intrusion_core::parse_replay_link`). An `inputs=` field may be absent — that is a
*level* link, and it inspects to the opening frame.

**It is not `--script`.** A script pads with waits and plays a whole batch on to a
capture or the cap, then reports balance metrics; that answers a different question.
Feeding a 13-input link through `--script` reports *"capture at turn 61"*, when the
true answer is *"on turn 13 the exit refused them"*. `--inspect` replays exactly the
pasted inputs, stops where they stop, and reports the trajectory.

**It boots the level the way the browser does** (`start_level`), so it reproduces
the run that was played rather than the sim preset's variation on it. That is why it
takes no config flags: the sim's knobs (`--guards`, `--alert`) are not in the token,
so a shared link cannot have been played under them, and naming one beside a link
would describe a run other than the one being inspected. They are refused by name
rather than ignored.

**It withholds nothing.** Per-turn lines use the game's own words where the core has
them (`message_for`), but those words are the near line's, and the near line is a
player-facing filter — it stays deliberately silent about a door a guard opened
across the facility (§11.7). An inspector must not inherit a filter built to
withhold, so an event with no near-line words is still reported in plain form.

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
  hideout (or cloaking with Camouflage, or crouching behind a bench) when a
  patrol closes, and fleeing to cover when hunted. It uses Run to open a gap, a
  takedown to clear a guard blocking the only way, and hideouts/Camouflage to
  wait a hunt out — so the ability histogram has something real to measure.

It plays nothing like a human — no fear, perfect recall of what it has seen —
so its win rate is not a difficulty verdict (§13.4). **Flag, never judge:** a
histogram spike or a win-rate cliff under `--bot` is a seed to go *play*, not a
ruling. The bot is deliberately crude; sharper policies are follow-up work.

> **The whole of the bot's decision handling is
> [`docs/bot-behaviour.md`](../../docs/bot-behaviour.md)** — the channels it may
> read, the plan it names each turn, the routing refusals, the cue seam, and the
> checklist for adding a cue. This README owns the operator's half (flags, output
> schema); that doc owns the policy.

### Ability cues — when the bot presses a key (§13.2/§8.1)

The bot does not carry a list of abilities it knows. It names the plan it has
settled on — an **intent**, one of `Flee` / `TakeCover` / `Pursue` / `Explore` —
and puts the moment to **every held ability's cue** ([`cue.rs`](src/cue.rs)),
which answers for itself whether this is a moment it is *for*.

The cue table is an **exhaustive match on `AbilityId`**, so adding a row to the
§8.1 catalog fails to *compile* until somebody says what the ability is for. That
compile-time obligation is the whole point of the seam: before it, each new
ability landed as a silent zero in the usage histogram — and **a false zero is
indistinguishable from a dead ability**, which is exactly the signal §13.2 built
the histogram to catch.

A cue returns a **bid**, not a bare number: the concrete `Input` to issue, a
*reason* in the cue's own words (§13.3 — a flagged seed has to be traceable back
to *why*), and an **urge**. An ability that is a plan rather than a press —
Camouflage only pays out while you hold still — is followed through by re-bidding
each turn while it runs.

#### The urge scale, and what every value on it means

Urge runs `0..=100`, with an anchor written down for each rung. The anchors are
what stop the scale becoming a handful of independently curve-fitted functions —
a cue author picks a number *against these words*, not against what makes the bot
win:

| Urge | Anchor — what a bid at this level is claiming |
|---:|---|
| `100` | **The moment the ability exists for.** Not pressing it now loses something the run does not get back. At most one cue should claim this for a given moment |
| `75` | **A strong fit.** Squarely the situation the ability's §8.3 row describes, and the turn is better spent activating than stepping |
| `50` | **A plain fit.** It would help, and there is nothing better to hand. This is the default floor, so a plain fit is the weakest thing that presses a key |
| `25` | **A faint fit.** It might help; a step is probably worth more, and by default the bot takes the step |
| `0` | **No fit.** Never pressed, whatever the floor is turned to — declining to bid and bidding zero are the same thing |

Values in between are fair; the anchors say what their neighbourhood *means*.

Arbitration is deterministic (§12.4) and has **no RNG anywhere**: every held
ability is cued in `AbilityId::ALL` order, a bid below its floor is dropped, and
the keenest bid wins with ties going to the earlier slot. What it deliberately
does *not* do is weigh an urge against the value of the step it displaces — §4.4
makes that the real question ("is this turn better spent activating than
stepping?"), but a step's worth is a cost-field delta and a common currency
between the two is a much larger change, and probably a worse one.

#### The per-ability floor, and the ambiguity it exists to resolve

Each profile carries **one urge floor per ability** (`cue_floors`, reachable as
`Profile::cue_floor` and turned one verb at a time with
`Profile::with_cue_floor`) — not one shared threshold, because there would then
be nothing to turn for one ability without turning it for all of them.

That dial matters because the seam introduces a real ambiguity: once cues exist,
a near-zero histogram slot means *"weak ability **or** shy cue"*. Sweeping one
ability's floor from `0` to past `100` and reading the curve is what separates
the two — **a flat curve exonerates the cue.** Read a low number as directional
and go play the seed (§13.3); never tune a cue until the number looks reasonable.

### Playstyle profiles (`--profile`, §13.2)

The bot's behaviour is governed by a handful of thresholds — how wide a berth it
gives a patrol, how early it ducks into cover, how long it waits there. Those
numbers *are* its temperament, so they live in a [`Profile`](src/profile.rs) the
bot reads, and each profile is **one row of numbers over the same policy** — not
a second bot:

| Profile | Its temperament |
|---|---|
| `balanced` | the middle temperament: steers wide of a patrol, takes cover when one closes and waits it out for a while, but pushes on rather than sit in a cupboard all run. The default, and the numbers the policy carried as constants before the seam, so metrics stay comparable across it. **Throws §7.7's comms switch** when it walks past one |
| `cautious` | gives patrols a wide berth, ducks into cover early and waits long; even bolting, it would rather round a patrol than brush past one. Trades speed for not being seen. **Throws the comms switch** too |
| `aggressive` | pushes toward the objective, **tolerates a cone to save turns** (it walks a watched cell rather than waiting the sweep out), hides late and briefly — and clears a patrol out of its way when the route offers the angle, then **stows the body** in a nearby cupboard |
| `careless` | `aggressive` carried further: **strikes more readily and never tidies up**, so the bodies it leaves stay on the floor to be found |

Two of the four take takedowns, and they differ in what they do afterwards for a
reason. A stowed body is *gone* — no cone will ever find it (§10.3) — so a bot
that always tidies up drives `bodies_found` to zero and leaves §7.3's radio clock
untested. `aggressive` covers the drag/stow chain and `careless` covers body
discovery; only together do they cover §7.2's cost end to end. `balanced` and
`cautious` carry a `takedown_reach` of **zero**, which declines the verb outright:
their flat takedown row is the temperament working, not an opportunity that never
came (§13.3).

`comms_reach` splits them the **other way** (#405): the careful pair throws §7.7's
comms switch — one bump, and no guard calls another for the rest of the level — and
the striking pair declines it. **Adjacent-only, never a detour**: §7.7 puts the
balance on the route rather than the switch ("one bump is cheap; getting to it is
not"), so a bot that walked to the console would make the placement distance measure
its pathfinding instead of how likely a wandering intruder is to find the thing. The
crossing is deliberate — the strikers are the ones whose bodies trigger the call-ins,
so whether *they* want it most is the sweep this verb now makes possible, and it
needs a declining pair to compare against.

Why more than one: §13.2 calls strategy diversity *"the most important and the
least obvious"* metric — **win rate tells you if the game is hard, strategy
diversity tells you if it is interesting.** A single fixed bot can never surface
it, because it always plays the one way. Running two temperaments over the *same*
seeds is how the sim says a facility is solvable two ways (healthy) or that both
collapse onto the same line (a puzzle with one answer). Where the two **disagree**
— one wins by waiting, the other is caught pushing — is precisely the §13.3 flag
worth playing.

A profile is a **temperament, not a solver** (§13.4). `aggressive` is not a
min-maxer hunting the optimal line; it is an impatient player, and it *should* be
seen more often. Every number in a profile is a `[START]` value, pinned only by
shape assertions (each profile finishes its runs; the careful one is seen less
often per turn than the impatient one), never by a leaderboard. And profiles must
stay one policy: if a temperament ever wants a different *decision* rather than a
different number, that is a signal to stop and reconsider, not to fork `decide`.
Pressing on for a second console, for instance, is deliberately **not** a profile
field for exactly that reason.

Every emitted row names the profile that produced it, so a batch's output is
attributable and two profiles' rows can be read side by side.

## Output schema

One JSON object per line on stdout: one **run row** per run, then one final
**summary row**. This schema is what the playtest skill parses — field order
is fixed, the tests in `src/report.rs` pin it byte-for-byte, and any change to
it is a deliberate, visible break.

### Run row

```json
{"seed":17,"profile":"balanced","outcome":"win","turns":214,"detections":2,"takedowns":1,"bodies_found":0,"usage":{"wait":90,"run":6,"camouflage":2,"decoy":0,"dephase":1,"autodoors":0,"confusion":0,"takedown":1,"drag":1,"pierce_wall":0,"lockdown":0,"crouch":3,"stow":1,"silence_radio":1},"alert_peak":2,"alert_escalations":[{"turn":9,"rung":1,"trigger":"sighting"},{"turn":31,"rung":2,"trigger":"repeat-sightings"}],"reinforcements":1,"par":155,"stars":1,"score":{"speed":false,"stealth":false,"haul":true}}
```

| Field | Meaning |
|---|---|
| `seed` | the run's seed — with the script, the whole replay |
| `profile` | the **playstyle profile** that played the run (above), so a batch's rows are attributable. `null` under `--script`: a script has no temperament, and naming `"balanced"` there would claim a bot played it |
| `outcome` | `"win"` \| `"capture"` \| `"entombed"` \| `"timeout"` |
| `turns` | spent turns at the end of the run (free actions excluded) |
| `detections` | fresh detections (`Event::Detected`): how often stealth broke — a held chase counts once, not once per turn |
| `takedowns` | takedowns landed (`Event::TakenDown`) |
| `bodies_found` | bodies found by guards (`Event::BodyFound`) |
| `usage` | the **ability-usage histogram** (§13.2): a count per verb spent this run. Keys, in fixed order: `wait`, `run`, `camouflage`, `decoy`, `dephase`, `autodoors`, `confusion`, `takedown`, `drag`, `pierce_wall`, `lockdown`, `crouch`, `stow`, `silence_radio`. Counted from core events — a *refused* activation costs no turn and emits none, so it never counts (§4.4); `wait` is the one verb with no event of its own and is counted from its spent turn. `Move` is not counted (it is the default nothing-else verb), and neither is the body **release** — letting go is free (§4.4), so it is on the same side of the line, while `stow` beside it spends the turn and locks the cupboard (§10.3). The counts sum to `≤ turns` |
| `alert_peak` | the highest §7.3 **alert rung** the facility reached, `0`..=`3` (#311/#376). A `0` is a real reading — a raid nobody noticed — where this field's old `null` meant "nothing measures this" |
| `reinforcements` | guards the ladder walked into the facility this run (§7.3/#374) — rung 2 sends one, rung 3 two more. Counted rather than derived from `alert_peak`, because an arrival is **refused** when the facility offers no cell out of the player's sight: a run can reach rung 3 and face fewer than three newcomers, and the difference is a fact about the level rather than about the ladder |
| `par` | the **facility's par turn count** (§14 v2/#563) — derived from the building's own span, consoles and crates, and the number `stars`' speed axis is measured against. Carried on the row so a par that is plainly wrong for a recipe is visible in the data rather than only in the star that fell out of it |
| `stars` | the run's **star total**, `0`..=`3`, or `null` for a run that did not get out (a capture, an entombment, a run the cap stopped mid-raid). `null` and `0` are different findings: one raid never finished, the other finished badly |
| `score` | the three axes it is the count of: `{"speed":bool,"stealth":bool,"haul":bool}`, or `null` on the same terms as `stars`. Speed is `turns <= par`, stealth is a final alert rung of `0`, haul is every console **and** every crate taken. **Reported, never optimised for** — the bot has cues, not an objective function, and handing it *maximise stars* would make the histogram measure the scorer rather than the game (§13.3) |
| `alert_escalations` | the **path** up the ladder: one object per escalation, oldest first, each `{"turn":T,"rung":R,"trigger":"…"}`. At most three (the ladder is monotone and three rungs tall), and `[]` for a facility that stayed quiet. The peak alone cannot tell a run that reached rung 3 by leaving bodies from one that got there by being seen over and over; this can. Trigger keys, in ladder order: `sighting`, `missed-ping`, `repeat-sightings`, `console-tampered`, `body-found`, `second-post-silent` |

### Summary row

```json
{"summary":{"profile":"balanced","runs":100,"wins":3,"captures":90,"entombed":0,"timeouts":7,"win_rate":0.0300,"turns_to_win_mean":211.5,"turns_to_win_median":208.0,"detections":312,"takedowns":45,"bodies_found":12,"usage":{"wait":9000,"run":600,"camouflage":120,"decoy":20,"dephase":80,"autodoors":0,"confusion":0,"takedown":45,"drag":40,"pierce_wall":0,"lockdown":0,"crouch":18,"stow":9,"silence_radio":2},"usage_share":{"wait":0.8500,"run":0.0567,"camouflage":0.0113,"decoy":0.0019,"dephase":0.0076,"autodoors":0.0000,"confusion":0.0000,"takedown":0.0043,"drag":0.0038,"pierce_wall":0.0000,"lockdown":0.0000,"crouch":0.0017,"stow":0.0009,"silence_radio":0.0002},"diversity":0.1837,"alert_peak_mean":1.8700,"alert_rungs":{"0":4,"1":31,"2":22,"3":43},"alert_triggers":{"sighting":96,"missed-ping":12,"repeat-sightings":22,"console-tampered":9,"body-found":12,"second-post-silent":3},"reinforcements":58,"stars":{"0":0,"1":1,"2":2,"3":0},"star_axes":{"speed":3,"stealth":0,"haul":3}}}
```

`win_rate` is over all runs; `turns_to_win_mean`/`_median` are over the
*winning* runs only and `null` when nothing won. `detections`/`takedowns`/`bodies_found`
are batch totals of the per-run metrics. `profile` names the temperament only when
**every** run in the batch agrees on one — a scripted batch, or one whose rows mix
profiles, reports `null` rather than borrowing a label that describes part of it.

Comparing temperaments is therefore one batch per profile, each summary
self-describing:

```
for p in balanced cautious aggressive; do
  cargo run --release -p intrusion-sim -- --bot --profile $p --runs 100 --seed 0 | tail -1
done
```

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

And the §7.3 **alert ladder** (#311/#376) — §13.2's *"whether escalation escalates"*
row:

- `alert_peak_mean` — the mean peak rung over the batch. The single number a
  `--alert` sweep plots.
- `alert_rungs` — how the runs' peaks were **distributed**, one key per rung
  `0`..=`3`. This, not the maximum, is the finding: *"most runs end at rung 1"* and
  *"most runs end at rung 3"* are opposite balance verdicts and both peak at 3.
- `reinforcements` — guards the ladder walked in across the batch (#374): the count a
  run actually **faced**, which is not the same claim as the rung it reached.
- `alert_triggers` — **attribution**: how many escalations each §7.3 trigger caused,
  same keys as the run row's `trigger`. Which *path* a batch takes up the ladder is
  the interesting half — a facility driven to rung 3 by bodies is a different game
  from one driven there by sightings.

And the three stars (§15 Q4/#563), over the **winning** runs only — a capture has no
score, so folding losses in as zero would report the win rate again under another name:

- `stars` — how the wins' totals were **distributed**, one key per total `0`..=`3`.
  The distribution rather than a mean, for the reason `alert_rungs` is one: *"most wins
  are one-star"* and *"wins split between none and three"* are opposite balance findings
  with the same average.
- `star_axes` — how often each axis was earned across those wins. This is the column
  that settles the design's own stated risk: if `stealth` reads near zero over a batch,
  the condition-0 threshold is *impossible* rather than aspirational, and it moves to
  ≤ 1 before the design is blamed.

> **The bot does not play for stars.** It has cues, not an objective function
> (`docs/bot-behaviour.md`), and nothing in #563 changed that. Reporting the score is a
> balance readout; optimising it would make the histogram measure the scorer rather than
> the game (§13.3), which is the one thing the sim exists to avoid.

> **Reading a zero in `alert_triggers` (§13.4/#260).** The count is escalations a
> trigger **caused**, which is what the core reports: a trigger firing at or below the
> rung already reached escalates nothing and says nothing. So a `0` has two readings —
> the bot never did the thing, or something louder always got to that rung first — and
> neither is *"this trigger does not matter"*. Report it as **never exercised**, not as
> no impact. Every trigger is always a column, including the ones at zero, so an
> unexercised trigger is visible rather than absent.

**Flag, never judge (§13.4):** these are numbers, not verdicts. A histogram spike
or a near-zero diversity is a seed to *go play*, not a ruling that the game is
broken — the playtest skill (#140) owns that framing.

## Determinism

Same `(seed, policy)` → byte-identical rows, asserted in `src/harness.rs`; under
the bot that is per `(seed, profile)`, asserted for **every** shipped profile in
`src/bot.rs`. That property is what makes the batch a regression instrument: same
seeds + same script (or same seeds + same profile) producing different rows means
the game changed.

CI leans on exactly that. `scripts/baseline.py` re-runs the three fixed batches
recorded in `.claude/skills/playtest/baseline.json` and fails if any summary
differs from the committed one, so a change that moves the numbers cannot land
without the snapshot moving with it:

```
./scripts/baseline.py             # check — exit 1 and a field-by-field diff on drift
./scripts/baseline.py --refresh   # re-run and rewrite the snapshot in place
```

The check is **exact**, not tolerant, because determinism makes it able to be: a
single moved digit is a real behaviour change, and whether that change is *good*
is the human judgement the playtest skill owns (§13.4). A red run says the
snapshot is stale, not that the game is broken — refresh it and commit.

## Writing a test here: pin the witness, never hunt for one

This crate's tests are the only ones in the workspace that pay for **whole runs**,
and a run is expensive: ~30 ms to carve the facility, ~100 ms more to walk it to an
ending. A test that sweeps for a rare verb — "somewhere in 400 seeds the bot bumps a
comms console" — buys hundreds of runs on every gate run to re-derive a fact that
does not change between commits. Left unchecked that is how this suite grew to 94
seconds while the 944 tests in `intrusion-core` took 14.

So the rule, for **every** existence-shaped test — one that walks runs until the bot
does some particular thing and then asserts about it:

**Pin one seed on which the thing happens. Do not search for one at test time.**

```rust
/// The pinned seed on which `balanced` drops a fake (see [`witness_sweep`]).
const WITNESS: u64 = 2;

for seed in witness_sweep(WITNESS, 0..40) {
    // … walk the run, assert the rule on every press …
}
assert!(dropped > 0, "{}", stale_witness("drops a decoy", WITNESS));
```

[`witness_sweep`](src/test_support.rs) walks **the witness alone** by default and the
**whole range** under `INTRUSION_SLOW_TESTS`, which CI sets — so the universal half of
the claim ("*every* decoy is dropped at a search") keeps its original width on every
push, while the local gate pays for one run.

Three things follow from the rule, and they are the point of it:

- **A red witness is a finding, not a chore.** When generation moves and the pinned
  seed stops exhibiting the verb, the test fails naming its own remedy: sweep with
  `INTRUSION_SLOW_TESTS=1`, take a seed that still does, pin that one. The cost of
  re-finding a witness falls on the change that moved it, which is the change that
  should be paying it — not on every gate run by everybody else.
- **It is not the hand-picked *window* that #387 rejected.** A window (`100..170`,
  chosen because a duck happened to land in it) hides the failure: the range empties
  and the test reads as a policy regression. A witness is one seed, named, with the
  verb it witnesses written beside it — when it goes stale the message says so and
  says what to do.
- **Negatives keep a sweep.** "This temperament *never* does X" has no witness to pin,
  because that is what makes it a negative. Use
  [`negative_sweep`](src/test_support.rs): a spread of 8 seeds locally, the full range
  in CI — core's own #60 bargain.

Two companions to reach for while you are there:

- **[`boot`](src/test_support.rs) is memoised** — booting a seed twice costs 25 µs the
  second time, not 30 ms. Always boot through it rather than calling `generate_level`.
- **[`profile_batch`](src/test_support.rs) is the one shared batch per temperament.**
  A test that asserts on a batch's *shape* — outcomes are mixed, the striking profiles
  work the body chain, two temperaments do not play alike — reads this rather than
  walking its own 60 runs. Those tests were each buying the same four batches
  separately; now they share one walk. If you need a batch that differs (a modifier, a
  loadout, another seed range) walk your own, and say in the doc comment why the shared
  one would not do.

The rulings appendix for all of this is **appendix 36** in
[`docs/design-rulings.md`](../../docs/design-rulings.md).
