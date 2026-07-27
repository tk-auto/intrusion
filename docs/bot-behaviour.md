# How the sim bot decides

**The bot's decision handling, in one place.** The design doc (§13.2–§13.4) says
*why* there is a bot and what its numbers are for; this says *how* it picks the
input it issues each turn. The implementation is `crates/sim/src/bot.rs` (the loop),
`crates/sim/src/cue.rs` (the per-ability seam) and `crates/sim/src/profile.rs` (the
numbers), and the operator-facing half — flags, output schema — is
[`crates/sim/README.md`](../crates/sim/README.md).

Every number named here is **[START]** (§13.4). They are pinned by shape assertions
and one behaviour pin, never by a leaderboard.

---

## 1. What it is, and what it is not

**A smoke detector, not a good player** (§13.4). A bot with perfect recall and no
fear plays nothing like a human. The point is not that it plays *well* but that it
plays *at all* — legibly, and the same way every seed — so that win rate, detection
counts and the ability-usage histogram measure the **game** rather than a hand-tuned
solver. When a metric spikes, the policy has to be simple enough to trace the spike
back to bot or game.

Two consequences worth stating before anything else:

- **It never solves.** No search over futures, no scoring of ability combinations,
  no learning. Every decision below is a greedy answer to *this* turn.
- **Its output is a flag, never a verdict** (§13.3). A suspicious number is a seed
  to go and play.

## 2. What it is allowed to know

The bot decides from **the same information a player is shown** (§11.5a), never the
raw `State` internals. This is the constraint everything else is built inside:

| Channel | What the bot may use |
|---|---|
| **Geometry** | Always known — walls, floors, doors, hideouts (§11.5a: *"geometry always"*). So is the **exit**: it is the player's own tunnel, the way they came in |
| **Contents** | *Fogged.* Intel consoles are unknown until seen and remembered after (`State::memory`). The bot cannot route to a console it has never laid eyes on — it explores to find them, exactly as a player must |
| **Guards** | Through `State::perceive_guard` (§9.2). A **seen** guard's cone is known and avoided (the danger overlay, §11.5); a **sensed** guard is a bare position to keep away from; one that is neither is invisible |
| **Rules** | Asked of core, never re-implemented. Routing asks `Terrain::routes_player`; ability legality asks the contextual `AbilityState` |

That last row is load-bearing and easy to erode. A private copy of a game rule
inside the bot is how its metrics quietly stop describing *this* game — so the
routing predicate and the ability-legality predicate are both thin wrappers over
core's own answer, and there are tests whose only job is to keep them that way.

## 3. The decision loop

Each turn the bot names **one plan** — an `Intent` — and acts on it. The four are
tried in priority order, and the first that applies wins:

1. **`Flee`** — a guard has the player, or is about to (§7.6). Nothing else matters
   while that is true.
2. **`TakeCover`** — not seen yet, but a patrol is closing. This is where most
   detections are avoided: the player senses a guard as far out as it could see
   them (both range 10, §9.1), so there is time to get out of the way.
3. **`Pursue`** — head for the nearest *known* untaken console; once the intel is in
   hand, head for the exit (§4.5).
4. **`Explore`** — nothing known to head for, so sweep toward the nearest frontier
   (a seen cell bordering the unseen) until the consoles reveal themselves.

Naming the plan is not cosmetic. It is the thing an ability cue is asked its
question *against* (§4 below), and it is computed once per decision so that no cue
has to re-derive "am I being hunted?".

### 3.1 Routing: a Dijkstra potential, not a greedy step

Within a plan, movement is a step downhill on a cost field expanded from the plan's
goals. Guard cones and a keep-away halo around perceived guards are folded into the
cost of *entering* each cell, so the cheapest route is the one that gives patrols a
wide berth rather than skimming a cone's edge.

A true potential has **no local minima but its goals**, which is why the bot cannot
get stuck in a two-cell shuffle however hard the guard costs pull. Ties resolve in a
fixed cell order, so the field is reproducible (§12.4).

Three refusals are worth knowing about, because each was a stall the bot actually
fell into:

- **It will not dive into a cupboard an alerted guard is watching.** Climbing in
  under such a cone is *witnessed*, and a witness flushes you straight back out
  (§15 Q5). A **Calm** patrol's cone is fine — that is the ordinary duck-past play.
- **It will not hide within reach of a body it left.** A guard that finds a body
  searches the cupboards beside it (§15 Q5), so a bolthole next to your own handiwork
  is a trap.
- **It will not spring a takedown that would wall it in.** A takedown drops the body
  on the guard's cell; when the bot's *only* routable way out holds that guard,
  striking seals the mouth for the rest of the run (§7.2). It leaves the guard be and
  waits for the patrol to step off.

## 4. Ability cues: which key it presses, and why

The bot does **not** carry a list of abilities it knows how to use. It puts the
moment to **every held ability's cue**, and each answers for itself whether this is
a moment it is *for*.

### 4.1 Why a seam at all

Before it, the bot knew exactly two abilities as hard-coded branches, and every
other one was **dead by omission**. That is worse than it sounds: each new ability
landed as a silent zero in the §13.2 usage histogram, and **a false zero is
indistinguishable from a dead ability** — so the one metric that *"would have caught
the free neutralise on day one"* was quietly not measuring the game.

The cue table is an **exhaustive match on `AbilityId`**. Adding a row to the §8.1
catalogue fails to *compile* until somebody says what the ability is for. That
compile-time obligation is the whole point of the design — it is the same move §8.1
makes with `Behaviour::Effects`: a small declared vocabulary you cannot silently
skip. A passive is handled by *saying so* in its arm (§8.2/#264), so "no cue" reads
as a decision rather than an omission.

### 4.2 A cue returns a bid

| Field | What it carries |
|---|---|
| `input` | The concrete `Input` to issue. There is no second place that turns an ability into a keypress |
| `urge` | How badly this cue wants the moment, on the anchored scale below |
| `reason` | Why, in the cue's own words — the string a §13.3 investigation reads back off a flagged seed |
| `then_hold` | Turns of follow-through the cue is committing to. Some abilities are a *plan*, not a press: Camouflage is only worth the turn if you then hold still (§8.3) |

### 4.3 The urge scale, and what every value on it means

Urge runs `0..=100`, with an anchor written down for each rung. The anchors are what
stop the scale becoming a handful of independently curve-fitted functions — a cue
author picks a number *against these words*, not against what makes the bot win:

| Urge | What a bid at this level is claiming |
|---:|---|
| `100` | **The moment the ability exists for.** Not pressing it now loses something the run does not get back. At most one cue should claim this for a given moment |
| `75` | **A strong fit.** Squarely the situation the ability's §8.3 row describes, and the turn is better spent activating than stepping |
| `50` | **A plain fit.** It would help, and there is nothing better to hand. This is the default floor, so a plain fit is the weakest thing that presses a key |
| `25` | **A faint fit.** It might help; a step is probably worth more, and by default the bot takes the step |
| `0` | **No fit.** Never pressed, whatever the floor is turned to — declining to bid and bidding zero are the same thing |

Values in between are fair; the anchors say what their neighbourhood *means*.

### 4.4 Legality is core's answer, never re-derived

A cue is handed the ability's live status, whose state is the **contextual** one:
`Unusable` when a held, economically-available ability would be refused for want of a
target (§11.4 — Pierce Wall with anything but exactly one adjacent wall, Decoy facing
a cell that cannot hold one).

Both bid constructors refuse to build a press the state says would not fire, so **a
cue offered for an ability that cannot fire is a bug, not a low bid** — and it is a
bug the types make unrepresentable rather than one review has to catch. A cue that
re-implements a precondition is the same drift as a private terrain table (§2), and
it is ruled out the same way.

### 4.5 Arbitration

Deterministic end to end (§12.4), with **no RNG anywhere**: held abilities are cued
in `AbilityId::ALL` order, a bid below its floor is dropped, the keenest urge wins,
and ties go to the earlier slot.

**The comparison it deliberately does not make.** §4.4 of the design makes the real
question *"is this turn better spent activating than stepping?"*, and the bot already
knows a step's worth as a cost-field delta. Weighing an urge against a cost-field
delta is fuzzy, and the fuzziness is accepted rather than solved: a common currency
between the two is a much larger change and probably a worse one. In practice this
means a cue's urge is judged against *other cues*, and the surrounding plan decides
whether an ability gets a look-in at all.

## 5. Temperament: profiles

Every threshold the bot weighs its options by — how wide a berth it gives a patrol,
how early it ducks into cover, how long it waits there, how keen a cue must be —
lives in a `Profile`. Three ship: `baseline`, `cautious`, `aggressive`.

A profile is **one row of numbers over the same policy**, never a second bot. This
is a constraint, not a convenience: if a temperament ever wants a different
*decision* rather than a different number, that is a signal to stop and reconsider,
not to fork the loop. (Pressing on for a second console, for instance, is
deliberately *not* a profile field for exactly that reason.)

Why more than one at all: §13.2 calls strategy diversity *"the most important and the
least obvious"* metric — **win rate tells you if the game is hard, strategy diversity
tells you if it is interesting.** A single fixed bot can never surface it, because it
always plays the one way. Running two temperaments over the *same* seeds is how the
sim says a facility is solvable two ways (healthy) or that both collapse onto the
same line (a puzzle with one answer). Where the two **disagree** — one wins by
waiting, the other is caught pushing — is precisely the §13.3 flag worth playing.

And `aggressive` is not a better player. It is an impatient one, and it *should* be
detected more often; that is the cost of its temperament, not a verdict on it.

## 6. The per-ability floor, and the ambiguity it exists to resolve

Each profile carries **one urge floor per ability**, not one shared threshold —
otherwise there would be nothing to turn for one verb without turning it for all of
them.

That dial matters because the cue seam introduces a real cost. Once cues exist, a
near-zero histogram slot means **"weak ability *or* shy cue"**. Nothing about the
architecture resolves that on its own. What does is sweeping one ability's floor from
`0` to past `100` and reading the curve:

- a curve that **climbs** as the floor drops → the ability was being held back by its
  cue;
- a curve that stays **flat** → the cue is exonerated, and the number is about the
  ability.

**The shape is the finding.** Read a low number as directional, and treat a
suspicious one as a seed to play (§13.3) — never tune a cue until the number looks
reasonable, which is how the histogram stops measuring the game.

## 7. Adding a cue

The checklist, in the order that keeps the metrics attributable:

1. **Justify it from the ability's §8.3 row**, in the code comment, and from nothing
   else. *"Decoys work on guards that have lost you, not on guards that have you"* is
   a cue specification; "it wins more" is not. Bidding Decoy at a guard that can
   currently see the player is a **cue bug**, not a tuning question.
2. **Answer the cost question** (§2.3): what does using it cost, and when would a
   good player choose not to? The cue's declining condition is that answer.
3. **Land it on its own**, one ability per commit or PR, with its own before/after
   metrics. One diff that switches five abilities on at once leaves every histogram
   move unattributable — which is the whole reason the seam landed
   behaviour-preserving in the first place.
4. **Check nothing became dominant.** If one verb's share jumps sharply, say so and
   flag seeds to play, rather than quietly tuning the cue until the number looks
   reasonable.
5. **Refresh the playtest baseline** in the same change, with the command and commit
   recorded.

## 8. Known gaps

Stated so they read as decisions rather than oversights:

- **Ducts.** The bot has no crawl policy (§10.7): climbing in is a mode change into
  the crawlspace with degraded perception, not a step a floor route can take, so duct
  entries are simply not routable for it.
- **Takedowns are incidental.** A takedown lands only from a guard's rear blind spot
  or under concealment (§7.2), and this avoidance-first bot steers wide of guards
  rather than hunting them, so it reaches that angle only rarely. Deliberate rear
  takedown play — and with it the body chain, §7.3's clock — is unmeasured.
- **Most tech has no cue yet.** Each is landing one at a time; until one does, its
  histogram slot honestly reads zero, and that zero means "no policy tried it", not
  "the ability is dead".
- **Loadout.** The sim's preset holds the innate set, so a cue for a piece of tech
  cannot fire in a batch that never grants it. A run that wants to weigh a specific
  ability grants it back and asserts on that.

Every one of these is the same failure class: **a metric reading zero because no
policy ever tried.** Treat an unexercised metric as *inconclusive*, never as
*no impact*.
