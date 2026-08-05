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
| **Geometry** | Always known — walls, floors, doors, hideouts (§11.5a: *"geometry always"*). So is the **exit**: it is the player's own tunnel, the way they came in. **One modifier makes this a cheat — see below** |
| **Contents** | *Fogged.* Intel consoles are unknown until seen and remembered after (`State::memory`). The bot cannot route to a console it has never laid eyes on — it explores to find them, exactly as a player must |
| **Guards** | Through `State::perceive_guard` (§9.2). A **seen** guard's cone is known and avoided (the danger overlay, §11.5); a **sensed** guard is a bare position to keep away from; one that is neither is invisible. The §9.5 fading marks (`State::sense_marks`) are deliberately *not* read: a trail is the board reminding a human of what it showed a turn ago, and the bot reads the live channel directly, so using it would double-count what it already knows |
| **Rules** | Asked of core, never re-implemented. Routing asks `Terrain::routes_player`; ability legality asks the contextual `AbilityState` |

That last row is load-bearing and easy to erode. A private copy of a game rule
inside the bot is how its metrics quietly stop describing *this* game — so the
routing predicate and the ability-legality predicate are both thin wrappers over
core's own answer, and there are tests whose only job is to keep them that way.

> **The geometry row has one known hole, and it is written here rather than left to be
> discovered** (#233). That row rests on §11.5a's **[SETTLED]** rule that geometry is
> never fogged — and the layout knob's hard end (`--modifier layout-knowledge-none`)
> deliberately overrides exactly that rule. Under it the *player* sees only what they
> have walked; the bot still reads the whole facility, so it routes confidently through
> walls it has never seen, never explores to find a way round, and never pays the price
> the modifier exists to charge.
>
> So: **a batch under that modifier measures the bot, not the game** (§13.3), and no
> number from one belongs in a balance argument.
>
> **It used to be kept out of the §12.6 directed pool partly for this reason. It is not
> any more** (#518). Pool membership asks whether a modifier is a difficulty change that
> bends in a documented direction, and not whether this policy can score it — the sim is
> *"a smoke detector, not a judge"* (§13.4), and withholding a rule from players because
> the harness cannot weigh it lets the detector decide what the game contains. So a `+N`
> draw **can** now put a batch here, and the obligation moved with it: check the resolved
> set before quoting a number, and read `--inspect` or a captured replay for what the
> *player* would have been shown, which is worth looking at either way.
>
> Closing the hole means giving the bot an **optimistic explorer**: route over known
> geometry, treat the unseen as passable until a bump says otherwise, and re-plan on
> what it learns — which is a change to the routing core of the policy, not a flag. It
> is its own piece of work, and until it lands this modifier is judged by playing it.

## 3. The decision loop

Each turn the bot names **one plan** — an `Intent` — and acts on it. The four are
tried in priority order, and the first that applies wins:

1. **`Flee`** — a guard has the player, or is about to (§7.6). Nothing else matters
   while that is true.
2. **`TakeCover`** — not seen yet, but a patrol is closing. This is where most
   detections are avoided: the player senses a guard as far out as it could see
   them (both range 10, §9.1), so there is time to get out of the way.
3. **`Pursue`** — head for the nearest *known* untaken console; once the intel is in
   hand, head for the exit (§4.5) and bump it, which climbs into the tunnel home.
4. **`Explore`** — nothing known to head for, so sweep toward the nearest frontier
   (a seen cell bordering the unseen) until the consoles reveal themselves.

**Above all four sits the crawl** (§4.5/§10.7/#466). Inside a duct there is exactly one
thing to do, so a crawler never reaches the ladder: it makes for the mouth and climbs out
— never back onto the path (an interior cell may overlie ordinary floor, so a step onto
one is another crawl), never into a cupboard (one mouth, and it is the cell you are
standing on), and only onto ground it can actually **enter**: climbing out is a step with
no bump behind it, so a closed door panel beside the mouth is not a way out however
happily a *route* would plan through one (§10.4/#481). The exception is its own tunnel
with the intel gate met: then it makes for
the **way out** instead and presses the `exit: leave` the usable line offers, off the
board. That is the opening and closing beat of every run — the bot starts on the border
cell like any player and crawls in, and it leaves the same way.

Three **steps without an `Intent` of their own** bracket those four, because each is
about a physical commitment rather than a plan (§7.2/§7.7/§8.3), and each is silent
for a temperament whose reach for it is zero:

- **The body in hand comes first.** A drag halves the bot's speed and refuses to
  stack with Run, so it is settled before anything else is planned — hauled to a
  cupboard, or let go. Picking one *up* is a separate step (`fetch`), and since #451
  it presses **Wait** while standing on the body rather than stepping off it: the
  grab is a spent turn now, so a bot that kept walking would leave the body behind.
  The press goes through core's own verb, so the bot takes hold exactly when a player
  pressing the same key would (§2's rules-asked-of-core rule).
- **The strike sits between fleeing and taking cover.** Ahead of cover deliberately:
  the commonest safe angle in the game is the one from *inside* a cupboard, where
  taking cover would only ever wait.
- **The comms switch sits between taking cover and pursuing** (§7.7/#405). A comms
  console *already adjacent* is bumped: one bump, and no guard calls another for the
  rest of the level. Below cover, because silencing while a patrol is closing spends
  the turn that was the escape; above the objective, because the bot is walking
  anyway and one turn buys the rest of the level.

  **It never detours to one**, and that restraint is §7.7's own: "*The cost is the
  route, not the switch.* One bump is cheap; getting to it is not. Placement distance
  is therefore the balance knob." A bot that routed to the console would price the
  switch instead of the route and make the placement knob measure the bot's
  pathfinding — a solver, not a temperament (§13.4). So there is no goal, no
  cost-field term and no frontier bias; the trigger is core's own
  `Affordance::SilenceRadio` (§2 above), which also makes it FOV-gated, so the
  console must have been *seen* (§11.5a) exactly as §7.7 intends.

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
- **It will not walk into a lock it cannot open** (§10.4/#236). With the locked-room
  modifier on, a key-gated doorway the bot holds no key for is barred like a wall: the
  bump is refused and changes nothing, so a router that treated the panel as the
  walk-through §10.4 usually makes it would plan at that door and press it for the rest
  of the run. Only the *closed* ones — a gated door a guard has just walked through is
  the slip-in the modifier is built around, and the bot takes it like any other open
  panel. The bars lift the instant a takedown puts the key in its hand.
- **It will not step onto a body it is carrying.** A bump into the body in hand
  *lets it go* (§8.3), so the direct route back to a cupboard is the move that drops
  what you are carrying. The body is barred from **this turn's step** but left
  routable in the field — it is one step behind the player and moves as the player
  moves, so the cell it sits on now is clear by the time a route reaches it. What
  that produces on the board is a small loop: to bring a body back to a cupboard
  mouth you must return by a square rather than by backtracking.

### 3.2 The takedown, and the rule that used to forbid it

The bot used to carry a fourth refusal — *"it will not spring a takedown that would
wall it in"* — on the grounds that a body dropped on the guard's cell seals a dead
end's only mouth (§7.2/#170). **That hazard stopped existing** when #187 made a loose
body non-solid: the mouth stays walkable, and the bot can stand on it and wait to take
hold (§8.3/#451). The rule outlived its reason and went on refusing *every* takedown the
bot was ever offered — because all of them are that exact shape, a hidden bot with a
patrol on its cupboard door (#316).

What is left at that mouth is not a hazard but a **choice**, so it is a profile field
(`takedown_reach`) rather than a rule. Zero declines the verb outright and keeps the
avoidance-first temperaments byte-identical; anything else lets the strike run:

- **At arm's length** — a perceived guard is adjacent and core's own gate is open, so
  bump it. That covers every legal angle without naming any: the rear blind spot
  (§155 carves the three cells at a guard's back out of its cone) and concealment
  (§7.2 — a hidden or crouched player is concealed from every viewer). Asking the
  core's gate rather than naming the angles is what let the #410 flank experiment be
  measured without touching the bot at all: widen the blind spot and the same cue
  reaches for the flank, because the gate it consults is the one that moved.
- **Otherwise** — walk to a guard's back, if one is within the profile's budget. Only
  a **seen** guard offers one: a sensed guard's facing is unknown (§9.2), so where its
  back is, is unknown too, and guessing would mean walking into cones the bot cannot
  see (§11.5a).

The approach is the one route costed with **no keep-away halo** — you cannot both
give a guard a wide berth and walk up behind it — while the cone penalty still
stands, so "within budget" means "reachable without being seen on the way" rather
than merely "near". And the bot will not strike while any *other* guard has eyes on
it, read through core's `guard_detects_now` rather than off the danger overlay,
because those two differ exactly where this play lives: a cupboard cell can sit
inside a cone and still be perfectly safe.

### 3.3 The crouch, and why it is not a profile knob

`TakeCover` has a **floor** below the cupboard and the cloak: bump the table at your
elbow and duck behind it (§10.3). It is tried last of the three because it is the
weakest of the three — a cupboard is omnidirectional and contact-safe, a bench
conceals only *across* itself and stops nobody walking into you (§4.5).

From your own cell it is a **reflex, not an appetite**, which is what makes it unlike
the takedown: ducking behind the table beside you when a patrol walks in is what
anybody does, careful or impatient. So among the profiles that spend turns on cover at
all there is nothing left to dial — they crouch, and at very different rates, on the
numbers they already carry: how near a patrol has to be before cover is worth a turn
(`threat_radius`), and how far a *cupboard* is worth walking to instead
(`cover_reach`).

The profile field is therefore a flag, `crouches`, and it exists for one temperament.
**`careless` declines**, for the same reason it declines cupboards (`cover_reach: 0`):
a bot that spends no turn on concealment must refuse all of it, or its §7.2 row stops
meaning what it is there to mean — with no concealment available, every takedown it
lands is a rear blind spot one (§155), while `aggressive` covers the concealed angle.
Concealment is one decision, not two, and the two profiles' rows only stay readable
while it is. Like `takedown_reach: 0`, the decline keeps `careless` byte-identical
across this seam rather than merely similar.

A **reach** — *how far will it walk to a bench* — was built and measured out again
(#379). A bench you walk to goes **stale**: the spot is chosen for where a guard stands
now, and by the time you arrive it has moved and the concealing side of the furniture
has flipped. Over 100 seeds a reach of 2 or more did not add crouches, it **replaced**
them — from ~51 down to ~1 — as the bot spent its cover turns walking to benches it
never ducked behind.

Three rules govern the pose, and all three ask **core**, never a local copy of §10.3
(`State::crouch_would_conceal`, `State::crouch_holds`):

- **Duck** when a table beside the bot hides it from *every* guard it perceives within
  `threat_radius`. Concealment from one of two patrols is not cover, it is a coin toss.
  A **sensed** guard counts here even though it does not for the rear strike, and the
  asymmetry is real: striking a back needs the guard's *facing*, which a sensed guard
  does not give up; hiding across a bench needs only where it stands, which it does.
- **Hold** while that stays true — waiting is the one action other than the duck that
  keeps the pose.
- **Crouch-walk** when it stops: a plain step landing still hugging the run keeps the
  pose (§10.3), so the bot shuffles along the furniture as a patrol comes round rather
  than standing up and re-ducking for two turns.

And when neither holding nor shuffling covers it any more, it **gives the pose up** and
falls back to the ordinary ladder. That last rule is load-bearing rather than tidy:
`being_hunted` reads the same `concealed_from` the crouch defeats, so a crouched bot
*believes* it is unseen. A first cut that simply waited out the patrol behind a bench
that had stopped covering it put `cautious`'s detections up 46% (549 → 803) — standing
still in the open while convinced you are hidden. Leaving before the cone arrives, not
after, is what the give-up buys.

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

The Saver (#243) is the case where that arm has to say more than "it is passive". It
is a passive with something to **spend** (§8.2's use budget), so the tempting cue is
one that plays bolder while the save is unspent — and that would be the bot deciding
the ability is good and then proving itself right. What is wanted from a §4.5 exception
is what the *unchanged* policy's outcomes do when one capture is survivable (§13.3), so
its arm declines and says why. A boldness knob, if it is ever wanted, belongs to
`Profile` with the other temperament dials, never to a cue.

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

### 4.4a The cues that exist, and the fact each one reads

Every activated verb is cued (#347). What a cue is *allowed* to key off
is a fact the surrounding policy has already computed and handed over — the sharing is
the seam's reason to exist — so the right-hand column is also the list of what the
`Moment` carries:

| Ability | It is for… | The fact it reads |
|---|---|---|
| **Run** | the turn that decides whether a chase is outrunnable at all | intent is `Flee`, and a cell of room to spend the activation turn |
| **Camouflage** | a hunt you cannot reach a cupboard from | the `refuge` the policy found — silent when there is a real one |
| **Decoy** | a guard that has **lost** you | nobody's cone live on the player, somebody searching, and the faked cell not on the bot's own route |
| **Autodoors** | flight through a door that shuts behind you | a door on the step the plan would take |
| **Confusion** | a panic-buy of six turns; **decisive when cornered** | how many guards core's clamped blast catches |
| **Dephase** | a short crossing you can see the far side of | the `crossing` the policy found on its own cost field |
| **Pierce Wall** | a route the facility does not offer | the same crossing, at three times the price — the budget is scarcer |
| **Lockdown** | sending a pursuit the long way round | how many doors the box would seal, and no door on the bot's own way out |
| **False Call** | emptying the ground you are walking away from | the route step, the guards inside core's reach, and whether that step opens the gap on **every** one of them |
| **Vision** | — **passive**, no activation to cue (§8.2/#264) | — |
| **Guide** | — **passive**, and a cue would have to cheat (§11.5a) | — |
| **Saver** | — **passive**, and deliberately uncued even though it has a budget (#243) | — |

Three of these are worth reading twice, because they are the seam's own rules biting:

- **Confusion is the only cue that speaks at a gap of one.** Run and Autodoors both
  decline there — the activation turn is spent standing still, which a guard at arm's
  length turns into a capture — so the cornered moment would have no answer at all if
  Confusion did not claim it. That is what "at most one cue should claim `100` for a
  given moment" looks like when it works.
- **Dephase and Pierce Wall share one `crossing`, computed by the policy.** The router
  cannot plan a path through a wall, so a cue that pressed for a crossing the policy
  would then decline to walk would be a shy cue *by construction* — the ability fires,
  the histogram fills, and nothing happens. One function answers both "is there a
  crossing worth it?" and "which way do I step while phased", which is what makes that
  impossible rather than merely unlikely.
- **False Call is the narrowest cue here, and the narrowness is the point.** The
  ability's value is entirely in the turns *after* the press — §8.3 calls it "a vacuum,
  not a trap" — so a cue that fired it without checking where the bot was about to walk
  would be measuring the bot summoning a search onto its own feet, and the histogram
  would be reporting the policy rather than the verb (§13.3). The predicate that keeps
  it honest is one line: the step the plan would take must **increase** the distance to
  every guard the call would pull. Fire it and stand still and it is a suicide button;
  the cue is not allowed to press it in a moment where standing still is what follows.
  It is also the only cue that declines in `Flee` outright, where most of the others
  live: what this does is give the guards a reason to come to you.
- **A phased step is judged by the cell it ends on, not the one it enters.** Run is
  innate and nothing forbids holding it with Dephase, so one press can move two cells
  — and phased there is no bump to stop the free second one at a wall. The policy
  therefore asks where the press *lands* before it will walk a phased step at all, and
  the crossing offer is withdrawn outright while the sprint is up (a two-cell step
  overshoots the far side). Both guard the same thing: a duration that expires inside
  a solid costs the safety eject plus a stun as long as the throw (§8.3).

**The Guide's zero is a third kind, and it is the interesting one.** Vision and the
Saver have no cue because there is no key; the Guide has no key *either*, but unlike
them it hands the policy something it could act on — a bearing to an objective. Acting
on it is exactly what the bot may not do. §11.5a's no-cheat gate is that the bot routes
only to intel it has **seen** (`known_intel`), and a policy that walked a compass needle
would be routing to a console it has never laid eyes on. So the ability cannot be cued
without punching a hole in the one rule that keeps the sim's exploration numbers honest,
and the measurement it wants is the with/without pair a human reads —
`docs/stats/abilities/guide.md` — watching whether the *player* holding it stops
exploring.

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
lives in a `Profile`. Four ship: `balanced`, `cautious`, `aggressive`, `careless`.

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

The verbs a profile can decline outright are the ones with a **reach** — a number
whose `0` means "does not want it" rather than "never got the chance":

| Field | `0` means | Who takes it |
|---|---|---|
| `takedown_reach` | never strikes an unaware guard; leaves it blocked and waits the patrol out | `aggressive` 4, `careless` 8 |
| `body_stow_reach` | leaves every body where it fell, so §7.3's clock stays exercised | `aggressive` 6 |
| `comms_reach` | never throws §7.7's comms switch | `balanced` 1, `cautious` 1 — adjacent-only |
| `crouches` (a flag, #379) | never ducks behind a bench | all but `careless` |

`comms_reach` splits the temperaments the **opposite way** from the takedown, and
that crossing is deliberate. There is a real argument that the striking temperaments
want the silence *most* — their bodies are what trigger the call-ins — but shipping
that guess inside the change that built the verb would have left nothing to compare
it against. Two declining profiles is a clean control for the sweep that asks the
question properly.

### 5.1 Why the takedown needs *two* striking temperaments

`aggressive` strikes when the route offers the angle cheaply and then **tidies up** —
it hauls the body to a nearby cupboard and stows it, which locks the cupboard behind
it (§10.3). `careless` strikes more readily and **never** tidies, so its bodies stay
on the floor.

That split is not decoration; without it one of the two §13.2 rows this bot exists to
feed cannot move. A stowed body is *gone* — no cone will ever find it — so the tidier
the temperament, the flatter `bodies_found` reads and the less §7.3's radio clock is
exercised. One profile covers the drag/stow chain, the other covers body discovery,
and only together do they cover §7.2's cost from end to end.

They split the **strike's two legal angles** the same way, and that is why `careless`
refuses concealment of every kind — cupboards (`cover_reach: 0`) and benches
(`crouches: false`) alike. With none available to it, every takedown it lands is a
rear blind spot one (§155); `aggressive` takes cover, so it gets the concealed ones
(§7.2). Loosen either half and both profiles cover both angles, which is one
temperament measured twice.

Taking hold is worth distinguishing from stowing here, and **#451 changed which of
them is a decision — both are now.** The grab used to ride the step *off* a body's
cell, so it happened automatically and even `careless` racked up grabs it never asked
for and immediately dropped; a `drag` count said nothing about temperament on its own.
It is now a **wait spent standing on the body**, which means the bot has to press for
it: `fetch` walks onto the body, waits, and only then leaves for the cupboard — three
turns where it used to be two.

That makes `drag` a truer signal than it was, but it does **not** make it the one to
read: a temperament that declines to stow never presses the wait at all, so the count
now separates the stowing profiles from the rest for the same reason the strike counts
do, rather than because everyone grabs by accident. Putting a body *away* is still the
choice worth measuring, and it has its own §13.2 slot since #381 — `stow`, counted
from `Event::BodyStored` — so the split is **read** off the histogram rather than
inferred from the gap between `takedowns`, `drag` and `bodies_found`. The release
beside it still counts as nothing, because it is free (§4.4).

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
5. **Measure it with/without, on all four temperaments, and re-run a marginal delta
   on a disjoint seed block.** The committed baseline pins the *innate* batch, so a
   cue for a piece of tech does not move it and there is nothing to refresh there; the
   pair to run is in the playtest skill (§4a). Record the result — commands, commit and
   table — on the ability's own page in `docs/stats/abilities/`, which is exhaustive
   over `AbilityId` so "no page" never has to mean two things.

## 8. Known gaps

Stated so they read as decisions rather than oversights:

- **Found ducts are still not routed to** (§10.7). Since #466 the bot *crawls* — it has
  to, since every run begins and ends inside the player's own tunnel — but a §10.7
  shortcut is still not something its cost field will plan through: climbing in is a mode
  change into a crawlspace with degraded perception, not a step a floor route can take,
  so a shortcut's entries remain unroutable and a duct only ever gets used by accident.
  What the crawl policy covers is being *in* one, whichever one it is.
- **The avoidance-first profiles land no takedowns, and that is correct.** `balanced`
  and `cautious` carry `takedown_reach: 0`: they steer wide of guards rather than
  hunting them, so a flat zero in their takedown row is the temperament working, not
  a defect (§13.3). Read the §7.2 chain off `aggressive` and `careless` instead.
- **A bench takedown does not exist, and the zero is the geometry** (#379). §7.2 opens
  the takedown gate against any guard the player is concealed from, whatever the angle —
  but concealment *across a bench* needs the viewer on the far side of the furniture,
  which puts it at least two cells away, while a takedown needs it orthogonally
  adjacent. The two cannot both hold, proven exhaustively in core's
  `cover::an_adjacent_viewer_is_never_concealed_by_a_bench`. So §7.2's concealed strike
  is reachable through the cupboard, the duct and the cloak, and **never** through the
  crouch: a batch reporting zero bench takedowns is right, not shy.
- **`careless` never crouches, and that is correct.** It carries `crouches: false`
  alongside its `cover_reach: 0`: a temperament that spends no turn on concealment
  refuses the bench as it refuses the cupboard, so a flat zero in its crouch row is the
  decline working (§13.3). It is what makes its §7.2 row readable as the rear blind
  spot alone. Read the crouch off the other three.
- **The crouch is a careful player's tool, not an impatient one's.** The profiles with
  a wide `threat_radius` duck early, while the patrol is still far enough that its
  angle is stable, and hold the pose for several turns; the tight-radius temperaments
  duck with a guard already on top of them and are almost always walked round within a
  turn. Over 60 seeds `cautious` keeps a working pose for ~11 turns per duck and
  `aggressive` for barely one —
  so a crouch on the impatient profiles is mostly a turn spent for nothing, which the
  give-up rule (§3.3) bounds at one.
- **Stowing has no verb in the histogram.** Takedown, drag and `bodies_found` are all
  metrics; deposit-and-lock (§10.3) is not, so a batch infers it from the gap between
  takedowns and bodies found rather than reading it directly.
- **Lockdown's cue is the most provisional of the set.** Route denial during a chase
  *is* §15 Q1, more completely than for Autodoors or Confusion, which are merely
  coupled to it — so its numbers should be expected to move when that question does.
- **The pocket half of Pierce Wall is uncued.** §8.3 offers a second use — bore a
  dead-end alcove and sit a sweep out in it — and judging "out of the through-routes"
  means knowing where guards patrol, which the bot may not know (§2). So the measured
  numbers bound the borer *as a shortcut*, not the ability.
- **Decoy never fires in pursuit.** §8.3 supports pulling a patrol off a route ahead
  of you; the cue only speaks while being hunted, so its numbers bound the fake as an
  escape tool.
- **No cue reads the alert level or the radio net** (§7.3/#107), so nothing bids
  differently as a facility winds up.
- **Nothing goes and *buys* a key** (§10.4/#236). The bot knows a locked doorway is not
  a way through (§3.1) and it opens the room the moment a takedown it took for its own
  reasons hands over the key — but no plan says *the console I need is behind that door,
  so strike a guard*. Under the sim preset's `--intel-gate one` that costs it a little:
  over 100 balanced seeds the modifier reads harder in the documented direction (35% →
  24% win rate, detections 848 → 1,086, diversity 0.60 → 0.54), because the locked room
  is a room it must route around rather than one it cannot enter. Under `--intel-gate
  all`, where the locked console is required, the win rate goes to **zero** — and that is
  the bot, not the game (§13.3). The modifier used to be kept out of the §12.6 directed
  pool for this reason; since #518 it is in, because pool membership is not decided by
  what this policy can score (§13.4). A `+N` batch can now draw it, so **check the
  resolved set before quoting a number** from one.
- **Loadout.** A batch holds the innate set unless `--abilities` grants otherwise, so
  every tech verb reads a structural zero in the committed baseline. That zero is the
  loadout, not the cue — see `docs/stats/abilities/`.

Every one of these is the same failure class: **a metric reading zero because no
policy ever tried.** Treat an unexercised metric as *inconclusive*, never as
*no impact*.
