# Intrusion — Design

## 0. About this document

This is a **from-scratch rebuild** of Intrusion. A previous version of this game
exists but is **not available** to anyone building from this document, and is not
meant to be. This document is therefore self-contained: everything needed to
rebuild the game's essence is written here as rules and numbers, not as a porting
guide.

Two things follow from that:

- **Every number in here is a starting value, not a law.** They come from a
  version that was played and found wanting. They are here so you start from a
  tuned-ish place instead of zero — not because they are right. Section 12 is the
  machinery for changing them.
- **Where this document and your instinct disagree, say so.** The goal is a game
  that is fun. This document is the current best guess at one.

Status markers used throughout:

| Marker | Meaning |
|---|---|
| **[SETTLED]** | Decided. Don't relitigate without a reason. |
| **[START]** | A starting value or approach, expected to change under playtest. |
| **[OPEN]** | Genuinely undecided. Listed in §15. |

---

## 1. The game

You are an intruder. You enter a government facility through a tunnel, at night,
alone. You are looking for data. The building is patrolled by guards who will
catch you if they touch you. You have your body, your training, and whatever
forbidden hardware you have salvaged. When you have what you came for, you leave
the way you came in — there is no other exit.

The setting is post-collapse: technology was outlawed after "the Great Hack".
You and an old contact use illegally-retained radios to raid state facilities for
data disks and confiscated spy-tech, hunting a hidden central archive. The
fiction exists to motivate the rules: you don't kill because hurting people gets
security tightened; you leave by your own tunnel because you have no other way
out; you are alone because there is nobody left to send.

Turn-based. Grid-based. Top-down. Rendered as a character grid.

---

## 2. Design pillars

Carried forward from the original design notes, with one revision.

- **Game over is permanent, and this is a roguelike, not a roguelite.** **[SETTLED]**
  You are captured, you lose. See §2.2 — the distinction is precise and it matters.
- **A complete successful run is 2–3 hours, maximum.**
- **The protagonist does not kill.** **[SETTLED]** — as *fiction*. See below.
- **Stealth is encouraged and measured risk-taking is rewarded**, but failing at
  stealth is not punished so hard that the player stops taking risks.
- **Enough information to plan, enough surprise to force adaptation.**
- **When an outcome depends on chance, the randomness is proportional to the risk
  taken.** More risk, more variance. A careful play should be reliable.
- **The player can plan and prepare escape routes** for when stealth fails.
- **Levels adapt to the strategies the player leans on**, to discourage running
  the same trick for a whole campaign.
- **Thorough exploration is rewarded, but not dragged out.**

### 2.1 The revision: "no killing" is fiction, not a mechanic

The original pillar read *"no permanent guard incapacitate (no killing)"*. That
bundles two constraints which do not have to travel together:

- **The fiction constraint**: the protagonist doesn't kill. *Keep this.* It costs
  nothing and it is the character.
- **The mechanical constraint**: threats are never permanently removed. *Drop
  this.*

The mechanical half also directly contradicts the *"thorough exploration is
rewarded"* pillar. Explore-thoroughly plus threats-rearm is a treadmill: you are
asked to own the space and denied the means. Games that do make guards wake up
(Invisible Inc, most obviously) pair it with an escalating alarm that shoves you
out the door — you are never meant to own the space. Intrusion wants both. Pick
one, and this design picks *ownership*.

**Takedowns are permanent. The cost is the body, not a timer.** See §7.

### 2.2 Permadeath: roguelike, not roguelite

**[SETTLED]** The pillar is specific, and the specificity is the point:

> **You do not start over stronger.** There is no meta-progression, no unlocks that
> persist across runs, no content earned in run *N* that makes run *N+1* easier.
> You are captured, you lose, and the next run starts exactly where the last one
> started. **The only thing that carries over is what you learned.**

**Progression within a run is the opposite — and is essential.** Over a 2–3 hour
campaign you accumulate salvaged tech, intel, and options. You get meaningfully
stronger *inside* a run. That arc is the campaign. It just doesn't survive you.

| | Within a run | Across runs |
|---|---|---|
| Salvaged tech abilities | **Accumulate** | **Nothing carries** |
| Intel | **Accumulates and is spent** | **Nothing carries** |
| Facility access | **Opens up** | **Resets** |
| Player skill and knowledge | — | **Everything** |

This is the Spelunky model — one of the original stated inspirations — and the
FTL / Invisible Inc model at campaign scale.

**Consequences to design around, honestly:**

- **A 2–3 hour permadeath run means a capture at hour 2.5 costs 2.5 hours.** That
  is a real cost and it is deliberate; it's what makes the last facility frightening.
  But it puts enormous weight on §7.6: **if you can be captured by something that
  isn't your fault, the pillar becomes cruelty rather than tension.** Permadeath is
  a promise that the game is fair. Unfair permadeath is just a bad game. Every
  capture must be traceable to a decision the player made.
- **The old version was not permadeath in any sense** — it offered unlimited "play
  the same level again" from a run-start snapshot, so a run could be retried
  forever. This is not that.
- **A parked idea, deliberately not designed yet:** a **prison level** — capture
  drops you into a cell with a chance to break out and rejoin the run, instead of
  ending it outright. It would soften the 2.5-hour cliff without adding
  meta-progression, and it's thematically perfect. **Later. Do not build it into v1**
  — it is a safety valve for a pressure that has to be shown to exist first.

> **Development tension, stated plainly.** Permadeath and "iterate fast to find the
> fun" pull hard against each other: you cannot playtest hour 3 of a run fifty
> times. Expect a debug/practice mode that starts anywhere with anything. It is
> *not* the real game, must never be reachable by accident, and must never be
> confused with a roguelite.

### 2.3 What actually went wrong last time — read this before tuning anything

The previous version was not fun. It is tempting to blame the design. The
evidence says otherwise: **every system that would have created pressure was
inert, and the one ability that resolved pressure was free.**

| System | Intended | What actually happened |
|---|---|---|
| Neutralise ability | A costed tactical option | Unlimited range, no cooldown, **and it did not consume a turn**. You could neutralise every guard in sight, for free, without ending your turn. |
| Sound | Noise draws guards | **Guards were deaf.** A full propagation model existed and was never given a single sound source. |
| Alert | Detection makes things harder | **Never written to, never read.** |
| Run | An escape option | 2 cells/turn against guards hard-capped at 1 → an *unconditional* escape. Being seen was never fatal. |
| Guards | Patrol, cooperate, search | No communication. No reaction to a downed colleague. No search at the last known position — arrive, find nothing, wander off. |
| Fog of war | — | None at all. The whole floor plan was legible from turn one. |

**The lesson is not "the design was wrong". It is that the design was never
actually running.** The version that got playtested was this design with all of
its tension removed and a free win button added.

Two consequences for this rebuild:

1. **Cost is the load-bearing property of every ability.** Not range, not
   duration — cost. An ability that costs nothing is not a decision. Before
   adding any ability, answer: *what does using this cost, and when would a good
   player choose not to?* If there's no answer, don't add it.
2. **This class of failure is invisible to a human playtester and obvious to a
   bot.** A human plays 5 levels and vaguely feels the game is flat. A bot
   playing 500 levels reports "the neutralise ability is used 94% of turns and
   win rate is 99%" on the first run. This is the single strongest argument for
   §12.

---

## 3. What this rebuild optimises for

In priority order:

1. **Experiment velocity.** The primary goal is to find the fun, and the fun was
   never found. So the thing being optimised is *how fast an ability, a guard
   behaviour, or a generation rule can be tried and judged*. Everything below —
   the language, the purity of the core, the determinism, the data-driven
   abilities — serves this.
2. **Honest pressure systems.** Every system in §7–§9 either works or is not
   shipped. No more stubs that look like features.
3. **A static page that still builds in five years.** The game ships as a static
   GitHub Pages site. No server, no CLI, no runtime dependency on anything but a
   browser.

Note the ordering. Shipping is third. The distribution target is a constraint,
not the goal.

---

## 4. Core rules

### 4.1 The grid

- Square grid of integer cells. **[SETTLED]**
- **Movement is 4-directional.** No diagonals. **[SETTLED]** — it keeps distance
  and vision coherent, and it is the game's texture.
- Distances are **Manhattan** (steps), except sight range, which is a **square
  box** (see §6.1).
- The facility is always fully enclosed by an indestructible 1-cell border.

### 4.2 The turn

A turn resolves in three fixed phases, always in this order:

1. **Player.** The player acts. The turn does not advance until an action
   explicitly ends it.
2. **Sight.** Every viewer's field of view is recomputed from its current
   position and facing.
3. **Guards.** Each guard reads the current sight data, decides, and acts.

One full turn runs at level start, so guards have established position and sight
before the player's first input.

> **Design note.** In the old version guards read a field-of-view snapshot from
> *before* their last move, giving a one-turn sensory lag. Recomputing sight in
> phase 2 removes it. That lag was accidental, but it created a real and
> interesting effect — a *moving* guard was always checking stale ground, giving
> the player a reliable one-turn window that a *stationary* guard did not. If
> playtest wants that back, reintroduce it **deliberately**, as a stated rule
> ("guards are checking where they were looking last turn"), and make the danger
> overlay show the lagged cone so the display stays truthful. **[OPEN]**

### 4.3 Occupancy

Every cell has a capacity of **1.0**. Every object declares a fill in 0.0–1.0. A
move into a cell succeeds if the fills already there, plus the mover's, are
≤ 1.0.

- Fill 1.0: walls, closed door panels, door hinges, guards, the player, bodies,
  consoles, **occupied hideouts**. Solid and exclusive.
- Fill 0.0: open door panels, decoys. Walk-through.

An **empty hideout** is the odd one out: it is walk-through for *pathing* accounting
yet you **bump** to climb in rather than drifting onto it (§10.3), so entering is a
decision, not an accident.

**A blocked move is an interaction, not a failure.** Walking into a door opens
it. Walking into a console uses it. Walking into an unaware guard takes them
down. **Bumping a hideout climbs into it** (and moving off climbs out). This "bump"
is the game's entire interaction verb — there is no separate *use* key. **[SETTLED]**
It is the reason the control scheme is four arrow keys and a handful of abilities.

### 4.4 Turn cost — the rule that matters most

**Every action that changes the world costs the turn.** Movement, bumping,
using a console, activating an ability, taking a guard down.

Explicitly enumerated exceptions, and there should be very few:

| Action | Cost | Why |
|---|---|---|
| Moving into a wall | **Free** | It's a mis-input, not a decision. Punishing it just punishes typos. |
| Toggling an ability *off* | **Free** | Cancelling should never be a trap. It already costs you the unused duration. |

If you are adding an ability and about to make it free, re-read §2.3.

### 4.5 Win and lose

- **Lose: a guard touches you.** A guard that attempts to move into your cell
  captures you. That is the only loss condition. There is no health, no combat,
  no damage. **[SETTLED]**
- **Being seen is not losing.** It is the beginning of a problem.
- **Win: satisfy the objectives, then return to your entry point.** You leave the
  way you came in. Bumping the exit early refuses, with a message.

> **Consequence to preserve:** because capture is *contact*, not *detection*,
> being invisible does not make you safe. A guard patrolling into the cell you
> are standing in catches you even if it cannot see you. Hiding is not the same
> as being somewhere safe. This is a good rule; keep it.

---

## 5. The player

| Property | Value |
|---|---|
| Sight range | **15** (a 31×31 box) **[START]** |
| Sight arc | **~180°** forward half-disc **[START]** |
| **Guard-sense range** | **10** (21×21 box), **20 while waiting** — through walls, position only (§9) **[START]** |
| Facing | Direction of last successful step |
| Speed | 1 cell/turn |

**There is no turn-in-place action.** Facing changes only by moving. A blocked
move does not change facing. **[SETTLED]** — this is what makes *Wait* meaningful
and what makes corners tense.

The player out-senses guards on both range (15 vs 10) and arc (180° vs 90°), and
— crucially — **sees them coming through walls** (§9): a guard within the
guard-sense range shows as a bare dot at its exact cell even with no line of
sight, though its *cone* stays hidden until you can actually see it. This
asymmetry is the foundation of the whole game: **avoidance is viable because you
see them first.** Do not erode it casually.

---

## 6. Vision

### 6.1 Rules

- Sight is a **facing-dependent forward cone**, blocked by walls, closed door
  panels, and hinges. An opaque cell is itself seen — you see the wall face — but
  shadows everything behind it.
- **Range is a square box, not a circle.** Range 10 means a 21×21 box. There is
  no distance falloff. **[START]** — a circle would be more natural; the box is
  cheap and was never noticed. Worth trying.
- **The 8 cells immediately around any viewer are always seen, in every
  direction, including directly behind.** **[SETTLED]** — this is load-bearing:
  **you can never stand adjacent to a guard undetected.** Sneaking up behind
  someone is never free. See §7.2 for how this interacts with takedowns.

### 6.2 Implementing the cone

A symmetric shadowcast over the square box, with one trick: **the cone is
produced by treating the out-of-arc cells of the viewer's own 8-neighbour ring as
if they were walls.** Shadowcasting propagates outward, so those artificial walls
cast the shadows that carve the cone. Because artificial walls are still marked
seen — exactly like real ones — you get the 360° touching ring for free.

Rank the 8 neighbours by angular deviation from facing:

| Neighbour | Tier |
|---|---|
| Directly ahead | 1 |
| Forward diagonals | 2 |
| Directly to the sides | 3 |
| Rear diagonals | 4 |
| Directly behind | 5 |

A neighbour is transparent if `arc_width >= tier`, else treated as opaque.

| Arc width | Resulting arc | Used by |
|---|---|---|
| 1 | Ahead only | — |
| **2** | **~90° forward wedge** | **Guards** |
| **3** | **~180° forward half-disc** | **Player** |
| 4 | ~270° | — |
| 5+ | 360° | Player while waiting |

This is elegant and it works. Keep it.

---

## 7. Guards

### 7.1 Baseline

| Property | Value |
|---|---|
| Sight range | **10** (21×21 box) **[START]** |
| Sight arc | **2** → ~90° forward wedge **[START]** |
| Initial facing | South |
| Speed | **1 cell/turn, always** **[SETTLED]** |
| Alert duration | **30** turns **[START]** |

**Guards never accelerate.** **[SETTLED]** A chasing guard moves exactly as fast
as a patrolling one. This is what makes an escape ability meaningful — but see
§8.2, because in the old version it also made being seen *entirely
consequence-free*, which is the opposite failure.

### 7.2 Takedown — the central mechanic

This replaces the old ranged "paralyze". It is the ability the whole design hangs
on, and the old one was free, unlimited-range and therefore the game.

| Property | Value |
|---|---|
| Range | **Adjacent only** **[SETTLED]** |
| Requires | The target **has not detected you this turn** **[SETTLED]** |
| Cost | **The full turn** **[SETTLED]** |
| Result | Target is **permanently** out. Leaves a **body**. |
| Cooldown | None — the constraints *are* the cost |

Because of the 360° touching ring (§6.1), **an aware guard can always see you
when you are adjacent to it.** So a takedown is only possible against a guard
that is unaware — which means you had to arrange to be adjacent without ever
having been in its cone. That is a puzzle you solve with geometry, timing, doors
and distraction. It is not a button.

**The body is the cost.** A body is a solid object (fill 1.0) that:

- **Can be seen.** Any guard whose cone covers a body has *found* it.
- **Can be moved.** You can drag it (§8.1) — slowly.
- **Can be hidden.** Put it in a hideout and it is gone.

**Finding a body is the loudest event in the game.** It should raise the alert
harder than being seen does. A guard that finds a body knows there is an intruder,
knows roughly where, and knows they are willing to act.

### 7.3 The radio — how permanence stays costly

Permanent takedowns need a cost that is not "they wake up". This is it.

**Guards are on a radio net. Periodically, control pings a guard. It has to
answer.** A guard that is down does not answer.

| Property | Value |
|---|---|
| Ping interval | **every ~20 turns per guard, jittered** **[START]** |
| Missed ping → | Control dispatches the nearest active guard to the silent guard's last known post |
| Second missed ping → | Facility-wide alert step |

Why this is the right mechanic:

- **It makes takedowns a clock, not a cost you pay once.** Every guard you take
  down is a future appointment. Three takedowns is three clocks running at once.
  The strategy *scales badly on its own* — no rule is needed to ban a full clear,
  it collapses under its own weight.
- **It is diegetic and legible.** The player can *read* the pings — a near-line
  message when control pings, and, because guard positions are sensed through
  walls (§9), the dispatched responder is a **dot that visibly peels off toward
  the silent post.** So the player knows the clock exists and roughly when it
  fires. That makes it plannable, which is the pillar about giving enough
  information to strategise. *(This tell was going to be a **sound** — a ping the
  player heard, §9. Sound is gone; make the tell visual from the start.)*
- **It gives hiding a body a real payoff** — a hidden body still misses its ping,
  so hiding buys you the *investigation* being confused rather than the
  investigation not happening.
- **It creates the escalation the alert system was always supposed to provide**,
  from a concrete, explainable source rather than a global number.

**[OPEN]** — whether a run **score** exists, and whether takedowns cost score.
See §15.

### 7.4 State machine

| State | Colour | Entry | Behaviour |
|---|---|---|---|
| **Calm** | yellow | Default | Patrols (§7.5) |
| **Alerted** | orange | Alert timer > 0, nothing seen this turn | Walks to its destination, then **searches** it (§7.6) |
| **Chasing** | red | Player detected this turn | Destination ← player's live cell; alert timer ← 30; step along shortest path |
| **Investigating** | red | A decoy seen, or a glimpse in the outer zone (§7.6) | As Chasing, but toward where it thinks you are, and reported at lower severity |
| **Responding** | orange | Dispatched by a missed radio ping (§7.3) | Walks to the silent guard's post |

### 7.5 Patrol

**Routes are not authored, and they are not random.** Each guard sweeps for cells
it has not recently looked at.

- Each guard has a **station** (spawn cell) and a **patrol radius** (**15**
  **[START]**).
- It keeps a private memory of inspected cells.
- With no destination, it walks to the **farthest** uninspected, currently-empty
  cell in its territory. *Farthest*, not nearest — this is what makes guards pace
  across distances instead of shuffling locally, and it is why the emergent
  patrols read as purposeful. Keep it.
- When no uninspected cell remains, it wipes its memory and starts over.

**Known weakness, worth fixing: territories are boxes around spawn points, which
have no relationship to the building.** They straddle walls, spill into
unreachable rooms, and overlap arbitrarily. Two guards spawned near each other
grind over the same ground while a wing goes uncovered. **This is downstream of
§10.5 — you cannot assign "cover the east wing" if nothing knows what the east
wing is.** Fix the spatial model and this fixes itself.

### 7.6 The chase and the hiding game — read this before touching guard AI

**This is the known reason the game was not fun, from direct play:** *guards that
saw you tailed you relentlessly; breaking out of sight was neither easy nor fun,
even with Run.*

That is not a tuning problem. **Four rules combined into a tracking turret.**

1. **Facing follows movement, and a chasing guard moves toward you.** So **its
   cone is re-aimed at you every single turn, for free.** You cannot leave a
   chasing guard's cone by moving. It is a turret that never needs to traverse.
2. **Detection is binary at a flat range of 10, with no falloff.** A guard tracks
   you exactly as perfectly at 10 cells as at 1. **Distance buys nothing.**
3. **Run gains 5 cells against a range of 10.** 2 cells/turn for 5 turns = a
   5-cell gap, into a 10-cell range. **Run cannot break contact — it
   arithmetically cannot.** Then 12 turns of cooldown at parity speed, cone still
   locked. The player does the obviously correct thing and the maths forbids it
   from working.
4. **Corridors are full-span straight sightlines, by construction.** The primary
   structure of every level runs the *entire span of its region* — up to 38 cells,
   dead straight, 2–4 wide — and **cover is only ever placed in rooms** (§10.1
   step 5). The space you flee through is a shooting gallery.

Cone tracks free + distance irrelevant + escape tool can't outrange + nowhere to
break sight = **the chase had no exit.**

And on the rare occasion sight *was* broken, the guard walked to the last known
cell, found nothing, and resumed patrol immediately. So the chase was **binary:
glued, or gone. Never hunted.**

> **The hunted phase is the entire game.** Break sight, slip into an alcove, hold
> still, watch the red cone sweep past, breathe out, move. **That experience did
> not exist in any form.** Everything below exists to create it.

#### The shape a chase should have

| Phase | What happens | How it feels |
|---|---|---|
| **Spotted** | Guard chases. **It calls it in** (§7.7). | Oh no |
| **Flight** | You need ~3–4 turns of broken sight to disappear. Run, doors, corners. | Urgent, but *achievable* |
| **Lost** | Guard reaches last known position. Others converge. **The search begins.** | The good part |
| **Hunted** | You are in a hideout / behind a pillar, holding still. Cones sweep. | **The best part** |
| **Released** | They give up. Alert decays. Patrol resumes — but *this region gets watched harder*. | Earned |

The old version had phase 1 and nothing else.

#### The fixes, in order of importance

**1. The chase must be able to end. [SETTLED that it must; mechanism [START]]**

Proposal: **two detection zones instead of one flat range.**

| Zone | Range | Guard behaviour |
|---|---|---|
| **Certain** | **≤ 5** **[START]** | **Chasing.** Tracks your live cell. |
| **Glimpse** | **6–10** **[START]** | **Investigating.** Moves toward where it *thinks* you are — your position when last in the certain zone. Imprecise. |
| Gone | > 10 | Search, then patrol. |

This keeps detection legible and binary-ish (two states, not a meter), it paints
cleanly in the danger overlay as **two shades of red**, and — critically — **it
gives Run a job**: 5 cells of gain is exactly the distance from the certain zone
to the glimpse zone. That relationship should be *designed*, not coincidental. If
you retune Run, retune the zones.

**2. Losing sight must lead to a search, not an instant give-up.**

On reaching a destination and finding nothing, a guard sweeps the surrounding area
for a number of turns before resuming patrol. **But note the ordering:** the old
problem was *never* that guards gave up too fast — it was that **you could never
reach the giving-up phase**. Fix 1 first. Making guards search harder while the
chase is still inescapable makes the game *worse*, not better.

**3. The geometry must allow breaking sight. This is a generator requirement.**

See §10.1a. A corridor 38 cells long and dead straight is not a space, it is a
sightline. **This is probably the single biggest contributor to the problem**,
and it is invisible if you only look at the AI.

**4. A lone guard tailing you should be escapable, and frankly unthreatening.**

This is the design pivot. **The tail is not the threat. The net is.** A single
guard moving at your exact speed can never catch you in the open, and that's
fine — it *shouldn't*. The frightening thing is that it **called it in**, and now
two more are converging on your reported position **from ahead**. That converts a
chase from tail-gating (boring, unwinnable, what you played) into a spatial
problem (readable, solvable, tense).

Danger should come from **being cornered and cut off**, never from being
out-jogged.

### 7.7 Cooperation — where the threat actually lives

The old version had none: no communication, no shared knowledge, no reaction to a
downed colleague. Each guard was an island. Given §7.6, this is not a nice-to-have
— **it is where the difficulty is supposed to come from.** **[START]**

- **A chasing guard calls it in.** Other guards within radio range switch to
  Responding and converge on the reported position. This is the single biggest
  lever on difficulty, the thing that makes a single sighting matter, and where
  the "levels adapt to your strategy" pillar lives.
- **A guard that finds a body calls it in**, harder.
- **Guards do not merge into a hive mind.** They know what has been *said*, not
  what each other can see. The player should be able to exploit a guard being out
  of radio contact, or reach one before it finishes reporting — **which makes
  "silence the radio" a real tactic and gives takedowns a purpose beyond removal.**

> **Tuning warning.** §7.6 and §7.7 pull in opposite directions and must be tuned
> as a pair. Loosen the individual guard (escapable), tighten the collective (the
> net closes). Get this backwards — sticky guards *and* cooperation — and you
> rebuild the exact thing that wasn't fun, only worse.

### 7.8 Guards and each other

- Guards are solid to each other but should **path around** each other. In the
  old version they pathed *through* each other, failed the move, and stalled —
  guards could deadlock in a corridor.
- Guards cannot hurt each other.

---

## 8. Abilities

### 8.1 The model

**Hybrid: data for the common case, code for the weird case.** **[SETTLED]**

Most abilities are a declarative record — cost, range, targeting mode, duration,
cooldown, and a list of effects drawn from a small vocabulary of primitives.
Trying "what if there were a smoke grenade" should mean **adding a row**, not
writing a system.

When a primitive won't stretch — piloting a drone, rewinding time — there is an
escape hatch to plain code behind the same interface. **Start data-driven; promote
to code only when the vocabulary genuinely can't express it.** Resist the urge to
grow the vocabulary to cover a one-off; that's how DSLs become bad programming
languages.

### 8.2 Economy

**No energy, no mana, no charges.** The economy is **time**: turn cost, duration,
and cooldown.

- **Duration** ticks down while active. On reaching 0 the ability switches off.
- **Cooldown** is set at activation, **frozen for the whole duration**, and only
  drains once the ability is inactive. So the true lockout is `duration +
  cooldown`.
- Toggling off early is free and refunds nothing. Cancelling costs you effect
  turns and saves you nothing.

> **A timing trap worth naming.** If durations tick at the start of the player's
> phase and activation happens after, a duration of *N* yields *N−1* effective
> turns, and the activation turn itself is unprotected. That inconsistency was
> live and undocumented in the old version — camouflage advertised 10 turns and
> concealed for 9, and the turn you switched it on you were fully visible. Pick a
> convention, write it down, and make the UI report the number the player
> actually gets.

### 8.3 The starting set

Everything here is **[START]**. This is the sandbox to experiment in — it is the
whole reason the architecture looks the way it does.

**Innate** — always available:

| Ability | Cost | Duration | Cooldown | Effect |
|---|---|---|---|---|
| **Move** | 1 turn | — | — | One cell, cardinal. Sets facing. Not shown in the UI. |
| **Wait** | 1 turn | — | — | **360° vision for that turn.** The only way to see behind you. |
| **Run** | 1 turn | 5 | 12 | One free move per turn while active → 2 cells/turn. |
| **Takedown** | 1 turn | — | — | §7.2. Adjacent, unaware target only. Permanent. Leaves a body. |
| **Drag** | 1 turn/step | while held | — | Drag an adjacent body. **You move at half speed while dragging.** Release is free. |

**Salvaged tech** — found in the facility:

| Ability | Cost | Duration | Cooldown | Effect |
|---|---|---|---|---|
| **Camouflage** | 1 turn | 10 | 20 | Undetectable **while you don't move**. Moving reveals you for that turn. |
| **Decoy** | 1 turn | 20 | 30 | A fake intruder in the cell you face. Draws Investigating, not Chasing. Dies when anything steps on it. |
| **Dephase** | 1 turn | 3 | 30 | Fill → 0. Walk through walls, doors, guards. **Does not conceal you.** |

Notes carried forward, because they are good and non-obvious:

- **Run is a guaranteed escape** — 2 cells/turn against a hard cap of 1. Combined
  with guards that don't search (§7.6), this made being seen *free* in the old
  version. With searching guards, radio calls and converging responders, the
  escape stops being the end of the problem and becomes the start of one. **Watch
  this pair closely in playtest** — if being seen is still free, the answer is
  more consequence, not a slower player.
- **Camouflage does not stop capture.** Invisible is not safe (§4.5).
- **Dephase does not conceal.** It's a movement tool. And while dephased you
  cannot *bump*, so you cannot open doors, use consoles, or win — you pass
  straight through everything you came for. That constraint is excellent; keep it.
- **Dephasing should be lethal if the duration expires while you're inside a
  wall.** It never was, which made it consequence-free. **[START]**
- **Decoy draws Investigating, never Chasing** — a guard that can see *you*
  ignores it. Decoys work on guards that have lost you, not on guards that have
  you.

### 8.4 Targeting

The old version had **no targeting system at all** — every ability was
self-targeted or auto-targeted at the nearest valid thing, because building a
targeting UI kept getting deferred. That is the direct cause of the free
unlimited-range neutralise: auto-target-nearest-visible was the path of least
resistance.

**Build targeting up front.** **[SETTLED]** At minimum: **self**, **direction**,
and **tile within range** (with a cursor). It unblocks most of the interesting
ability space, and its absence actively distorted the design.

---

## 9. Sensing guards

Sound was meant to be the channel that let the player steer guard attention and
track threats around corners — *"a second information channel that works around
corners"*. It was the most-built and most-praised idea in the old design, and it
was tried in this rebuild: a full cell-to-cell propagation field, guards that
hear, a loudness ladder, a "how far you were heard" overlay.

**It came out obscure and not fun.** An invisible field, tuned by numbers with no
on-screen consequence, doing its work behind the UI. §15 Q3 — *"how is sound
presented?"* — was never answered because the honest answer is *it wasn't*, and
**an invisible sound system is a missing one.** The complexity was real; the fun
was not.

So this rebuild **drops sound entirely** and keeps only the thing sound was
actually *for*: the player knowing, around corners, where the threats are. That
channel is now **direct**, and it is the inverse of sound's failure — sound was a
hidden model with a visible-nowhere presentation; the sense is a **visible model
with an obvious presentation.**

- **Guards detect only on vision (§6, §7). [SETTLED]** They do not hear. There is
  no noise, no propagation, no hearing check. Running, slamming a door, dropping a
  body — none of it draws a guard. **The only thing that gives you away is being
  *seen*.**
- **The player senses guards through walls.** Within a range, the player always
  knows the **exact cell** each guard stands in, wall or no wall — a location
  *hint*, and nothing more. **Facing and the vision cone are shown only for a
  guard the player can actually see** (§6). Sensed-not-seen is a dot on the map;
  seen is the full threat, with its cone and the danger overlay.

> **Consequence to state, not relitigate.** With guards deaf, **haste has no
> detection cost of its own** — running and slamming doors no longer draw anyone;
> the only downside of moving fast or loud is being *seen* doing it. And "close
> the door behind you" keeps its point through **sight**, not sound: a closed door
> still blocks line of sight (§10.3) and is still evidence someone passed
> (§10.4). It simply no longer muffles anything.

### 9.1 The sense

**[START]** — the two numbers below are the tuning surface, pinned by tests so any
later change is a deliberate, visible edit.

| Property | Value |
|---|---|
| Range | **10** (a 21×21 box, the shape of sight) **[START]** |
| Range while **waiting** | **20** (a 41×41 box) **[START]** |
| Reveals | The guard's **exact cell** |
| Does **not** reveal | Facing, vision cone — anything about where it is *looking* |

- **Range is a square box**, same shape as sight (§6.1) — cheap, consistent, and
  it makes "within sight range" and "within sense range" the same shape at
  different sizes. **[START]** — box or circle carries over from §15 Q6.
- **It passes through walls.** The sense is *not* line-of-sight; that is the entire
  point. A guard two rooms away, behind three walls, still shows as a dot if it is
  within range. Line of sight governs only the *cone* (§9.2), never the dot.
- **Waiting extends it, 10 → 20.** The innate Wait already buys 360° vision for the
  turn (§5, §8.3); it now *also* widens the sense. "Stop and take stock of the
  whole area" is the same verb that lets you see behind you, and it costs the turn
  — §2.3's "cost is load-bearing" applied to *information*. Peeking the ability
  state on Wait (§11.4) is the same principle; this stacks with it.
- **The sense is innate**, not salvaged tech — a baseline sense like vision, part
  of the player's out-senses-the-guards asymmetry (§5). No cost, no cooldown, no
  toggle; it is simply how the player perceives. It is *body and training* (§1),
  not hardware you have to find.

### 9.2 Seen vs. sensed — the two states of a perceived guard

A guard the player perceives is in exactly one of two display states, and the gap
between them is the whole design:

| State | When | What the player sees |
|---|---|---|
| **Sensed** | In sense range, **not** in the player's field of view | An orange **background** highlight on the guard's **exact cell** (no glyph of its own). No facing, no cone, no danger overlay. You know *where*, not *which way it looks*. |
| **Seen** | In the player's field of view (§6), line of sight clear | The full guard: glyph in its state colour (§11.2/§11.3), **facing, vision cone, and the danger overlay** (§11.5). |

**Knowing where a guard is is not knowing whether it can see you.** The cone — the
thing that actually captures you (§4.5) — is shown *only* when you can see the
guard. So the sense makes **route-planning legible** (read the live threat
positions, plan around them) without making **stealth trivial** (you still have to
break line of sight to read a guard's attention, and the danger overlay still
paints only cones you can see).

This is the §7.6 hiding game intact, and arguably sharper: pinned in a cupboard
behind a wall you know the hunter's dot is three cells away and closing — but you
cannot see its cone, so you hold still and hope. Exactly as intended, and now the
*where* is honest instead of inferred from a sound you couldn't quite place.

### 9.3 Why this is better than sound

- **It is visible.** Sound's fatal flaw was §15 Q3 — no good presentation. The
  sense's presentation is trivial and obvious: **draw the dot.** There is nothing
  left to solve.
- **It is legible without being omniscient.** You get *position*, not *attention*.
  The dangerous unknown — *is it looking at me?* — is preserved and tied to line of
  sight, which is where the whole game already lives.
- **It rewards Wait**, the game's one "spend a turn to know more" verb, instead of
  bolting on a parallel system.
- **It deletes a large, obscure subsystem** — propagation, emission, the loudness
  ladder, the hearing check, the noise overlay — in favour of a range check and a
  render state. Less code, less tuning surface, more clarity. That trade is the
  point of §3's "honest pressure systems": a system that isn't fun doesn't earn its
  complexity.

> **Consequence for guard cooperation (§7.7) and the radio (§7.3).** Both leaned on
> sound for legibility — the player was meant to *hear* a ping. With sound gone,
> "call it in" and the radio clock need a **visual / near-line** cue instead. The
> sense helps for free: a responder peeling off its patrol toward your last
> position is now **directly readable** as a moving dot on the map. Radio is still
> unbuilt (§7.3, **[START]**); design its tells visual from the start — a near-line
> message and the responder's own motion, not a sound.

---

## 10. The facility

### 10.1 Generation, step by step

**Corridor-first binary partition.** Corridors are the primary structure; rooms
are the leftovers. This is unusual — most roguelikes place rooms and then connect
them — and it is *right for this game*, because corridors are where stealth
happens, and generating them first makes them deliberate spaces rather than
plumbing.

1. **Start** with one region covering the interior `(W-2) x (H-2)`.
2. **Repeatedly carve a corridor through the largest remaining region**, splitting
   it in two:
   - The axis must be long enough to fit `6 (space) + 1 (wall) + 2 (corridor) + 1
     (wall) + 6 (space)` = **16**.
   - Corridor width: random **2–4**. Never single-file. **[SETTLED]** — a
     single-file corridor is a death trap with no counterplay.
   - Split position: random, subject to both leftovers being ≥ **6** deep.
   - If both axes fit, pick 50/50. If neither, stop.
   - Stamp: **wall line, corridor, wall line**, running the full span.
   - **Punch through one cell beyond each end.** This is the connectivity
     mechanism — it is what joins the new corridor to its parent. Without it you
     get disconnected boxes.
   - Replace the region with its two leftovers; re-sort by area.
3. **Every surviving region becomes a room.** Rooms are always rectangles, always
   ≥ 6×6.
4. **Doorways.** Scan every row, then every column. A run of wall cells with
   interior on both flanks is a door candidate. Each **maximal** run of length ≥ 3
   gets **exactly one** doorway, of random length **3 to min(run, 6)**, at a random
   offset. Because rooms are always separated by corridor-plus-two-walls, **every
   door connects a room to a corridor** — never a room to a room.
5. **Room features.** Up to **4** attempts per room. Each attempt proposes a
   partition wall and a pillar; whichever are viable go in a pool and one is
   picked:
   - **Partition wall** (needs ≥ 3×3): a 1-cell-thick stub jutting in from a wall,
     length random 2 to (axis−1). Makes alcoves, dead ends, and sight-line breaks.
     Orientation is weighted by the room's perpendicular extent, so tall rooms get
     horizontal stubs. Rejected unless its footprint grown by 1 is clear.
   - **Pillar** (needs ≥ 6×6): a freestanding block, random **2–4** by **2–4**.
     Rejected unless its footprint grown by 1 is clear.
   - **Pillars must come before hideouts** — a pillar is a ready-made ≥2-thick block,
     so a pillar face is valid backing for a recessed cupboard (step 6).

   **Step 5a — Thicken walls.** Thicken roughly **a third** of the interior walls to
   two cells (a single **[START]** knob), always growing **into a room, never into a
   corridor** — a corridor is 2–4 wide and eating a lane could single-file it, whereas
   a room is ≥6 and only loses an edge strip (never past the 6×6 minimum). Each eaten
   cell is validated exactly like a sightline blocker (it may not sever a patrol route
   or split a region) and kept clear of door throats. This gives cupboards their
   backing (step 6) and the facility some pilasters/buttresses; because it only *adds*
   wall it can never lengthen a sightline. Runs after doorways (so a thick wall dodges
   a throat) and before hideouts.
6. **Hideouts.** Furnish the hiding-game board with **cupboards recessed into the
   walls**: a wall-line cell with **exactly one floor neighbour (the mouth) and three
   solid wall neighbours** becomes a hideout — flush with the wall, backed and
   flanked, so it can be neither walked nor seen *through* to the far side (no cupboard
   traversal, no peephole). That geometry is what step 5a's two-thick walls and the
   pillar faces manufacture. Recessing is a **wall → hideout** rewrite, so unlike a
   floor-cell cupboard it cannot pinch a patrol route (a wall and a hideout both block
   pathing); the recessed cell joins the region it opens onto so "which room am I in"
   still answers for a hidden player. Place them **along the corridor network and near
   junctions**, not only in rooms — the flight path is where cover is needed (§7.6,
   §10.1a) — and **do not stop at the first failure.** Space them out (a single
   **[START]** knob) so the facility still reads as a building; the spacing also keeps
   a cupboard's own backing intact, since the two faces of a thickened wall sit one
   cell apart. *(The §10.1a corridor repair recesses **extra** cupboards mid-run —
   and carves alcoves into one-thick walls — wherever a corridor sightline demands
   one; for those, spacing is a preference, not a gate: breaking the run outranks
   it, and the three-solid-sides geometry alone keeps every backing intact.)* *(The original rule — "a wall cell with exactly 3 wall neighbours and 1
   empty neighbour, one attempt per room" — harvested only the rare natural pockets,
   which is what left the old game with no board. Same three-solid-sides geometry now,
   but the backing is **manufactured** by step 5a and the cupboard placed deliberately
   rather than harvested.)*
7. **Entry/exit and player** go in the **largest room**, at random empty cells.
8. **Objectives** go in any room *except* the start room.
9. **Guards** go in any room *except* the start room.

### 10.1a Corridors must have cover — the sightline rule

**[SETTLED]** — this is a direct consequence of §7.6, and it is the generator's
most important job after connectivity.

Corridor-first partition is the right structure (see §10.1) but it has a severe
emergent flaw that only shows up in play: **it produces long, dead-straight,
full-span corridors with no cover, and those corridors are where the player flees.**
The rooms get pillars and stubs. The corridors — the majority of the map, the
connective tissue, the place every chase happens — got **nothing**. A 38-cell
straight 3-wide corridor with a guard in it has no counterplay. It is not a space;
it is a sightline.

**The rule: no straight sightline longer than *L* without counterplay in it.**
**[START]** — *L* around **10–12**, i.e. roughly a guard's sight range. Longer
than that and there is no geometry between you and being seen. *Counterplay* is
an obstruction (a wall, a closed door), **a partial-cover table (§10.3)** — a
table does not stop a guard's sight, but it plants the crouch in the middle of
the straight, which is what the rule actually demands — **or a cupboard within
two moves**: a cell that is a recessed hideout's mouth (bump to vanish,
§10.1.6), or one floor step from one. A guard sees straight past a flush
recess, but the player there is gone before the sight matters; two moves and
not just the mouth because a corridor is up to four wide, and the lane beside
the mouth's lane flees to the same cupboard one step later. (The rule was first
stated as "no unbroken sightline", and the pass stamped 1-cell *wall* blockers
— which read as floating wall noise, not a building. The table restatement
replaced them; the cupboard clause came with the no-tables-in-corridors rule
below: same assertion machinery, honest architecture.)

This is a **testable property of a generated level**, not a vibe. Assert it, the
same way reachability is asserted (§10.6):

> For every cell, for each of the 4 cardinal directions, the run length without
> an obstruction, a cover cell, or a cupboard within two moves is ≤ *L*.

**Which counterplay a run gets follows its region [SETTLED]: tables are room
furniture, corridors get architecture.** A lone table read as noise, and a
table in a corridor read as a barricade in a hallway — so neither is generated,
and both are asserted away. Concretely:

- **Rooms: stamp benches of tables.** *(Implemented.)* A repair pass scans the
  finished grid and, near the middle of every over-long room-dominated run, stamps
  a **bench**: a straight row of **2 to a `[START]` cap** of partial-cover tables —
  never a lone cell — grown across the run or along it, never into a cell that
  would sever guard pathing or split a region (so a pathing gap, the 1-cell
  squeeze, always survives). A bench must land in a **furniture pose** or the
  attempt is rolled back and re-sited: **free-standing** (touching no wall — a
  workbench in the open, cover on every side), **end-on** (square against a wall
  at exactly one end, a desk jutting into the room), or **along-wall** (flush
  along one wall, a counter — only its *ends* offer useful crouch cover, since
  the §10.3 concealment quarter-plane behind its long side is the wall itself).
  Anything else — a wall stub brushing mid-bench, wall contact at both ends —
  is not how furniture sits, and is rejected.
- **Corridors: never a table.** An over-long corridor-dominated run is repaired
  with the hiding game's own board instead: **one more cupboard recessed
  mid-run** (its mouth is the counterplay), preferring ready two-thick backing,
  then an **alcove** (wall up the single cell behind a one-thick flank wall and
  recess into it), and where a stretch is too open for any recess — a junction
  plaza, walls all doors and cupboards already — a **2×2 structural pillar**
  (§10.1.5's column, corridor-sized: it blocks sight outright and forces the
  squeeze), or, where even a pillar would choke a 2-wide corridor, a 1-cell
  **buttress** flush against a flank wall (the S-squeeze as a pilaster).
  Architecture, not furniture; the flight path stays clear. Rooms whose run no
  bench can furnish — the 1-wide lane behind a partition stub — fall back to the
  same cupboard repair.
- **Cover near doors.** A door you burst through should have something to duck
  behind on the other side, or bursting through it accomplishes nothing.

A run none of the repairs can break rejects the carve like a reachability
failure. (**Jogging the corridors** mid-carve — offsetting a corridor a cell or
two mid-span — remains the unimplemented alternative if §15.2 wants it.)

**Hideouts must be reachable while fleeing. [SETTLED]**

Hideouts were placed **one attempt per room, stopping at the first failure** — so
a level could easily have very few, and **never any in corridors**. Combined with
§7.6, this means that during a chase — the exact moment the hiding game is
supposed to happen — **there was nowhere to hide.** The hiding game had no board.

Place hideouts along the corridor network and near junctions, not only in rooms,
and do not stop the placement pass at the first failure. **A flight path with no
hideout on it is a failed flight path.** This is worth asserting too **[OPEN]** —
something like "every cell is within *N* steps of a hideout" — but the right metric
is unclear and probably wants play evidence first.

### 10.2 Parameters

**v1 ships quick play only** (§14). One tuned configuration:

| Parameter | Value |
|---|---|
| Size | **40 × 40** **[START]** |
| Guards | **5** **[START]** |
| Intel | **3** **[START]** |
| Exit rule | All intel required |

Size is **screen-bound**: the whole level renders on screen with no camera
(§11.4 **[SETTLED]**), so it cannot outgrow what one screen shows legibly. The
scale axis beyond a screen is more stories (§14 Later), not a bigger grid.

Room count emerges from the partition constants and is bounded at roughly **12**
regardless of map size — the partition loop budget caps it. Note that a 20×20
level supports at most ~4 rooms, and **below 18×18 no partition is possible at
all**, leaving one room; since objectives and guards are placed "in any room
except the start room", **a single-room level cannot place anything and will
fail**. Guard the minimum.

### 10.3 Terrain

| Object | Glyph | Blocks move | Blocks sight | Blocks pathing |
|---|---|---|---|---|
| Floor | (blank) | No | No | No |
| **Wall** | `#` | Yes | Yes | Yes |
| **Door hinge** | `×` | Yes | Yes | Yes |
| **Door panel, closed** | `+` | Yes | Yes | **No** — see below |
| **Door panel, open** | (blank) | No | No | No |
| **Hideout, empty** | `}` | **Bump** | No | Yes |
| **Hideout, occupied** | `}` **(you)** | Yes | No | Yes |
| **Partial cover (table)** | `π` | Yes | **No** | Yes |
| **Console** | `$` | Yes | No | No |
| **Exit** | `E` | Yes | No | No |
| **Player** | `@` | Yes | No | No |
| **Guard** | `g` | Yes | No | No |
| **Body** | `z` | Yes | No | No |
| **Decoy** | `@` | **No** | No | No |

Vision is blocked when a cell's summed opacity reaches 1.0 — opacity itself is
still all-or-nothing, no half-shadows, no glass. **Partial cover exists as the
table**, and its concealment is *behavioural*, not optical: sight passes over it
freely; what it grants is the crouch (below). **[START]** — low walls / vaulting
stay a future axis.

> **The table is partial cover, and the crouch is a bump.** A table blocks
> movement and pathing like a wall — patrols route around it — but a guard sees
> straight over it. **Bump the table** (§4.3's one interaction verb, same as
> the cupboard: ducking is a *decision*, aimed at a specific table) and you
> crouch behind it: while crouched you still see everything (your own sight is
> unchanged), but you are **concealed from any viewer whose line of sight
> crosses that table** — the quarter-plane the cover faces, out to the 45°
> diagonals. Concealment is directional, per-guard, and per *the table you
> ducked behind* — not every table you happen to stand beside. The flanks and
> your back are open, which is what keeps a table weaker than a cupboard
> (omnidirectional, contact-safe) — and a crouched player **can still be
> captured by contact** (§4.5); unseen is not safe. The crouch spends the turn;
> **waiting holds it** (hold still, watch the cone sweep past, §7.6); any other
> spent action stands you up; a free action changes nothing, posture included
> (§4.4) — re-bumping the table you are already behind is a free no-op.
> *(Waiting beside a table used to crouch automatically; that coupling is gone —
> wait is pure (§5, §8.3's 360° look), and the crouch shows its direction in
> the usable line (§11.4) like every other bump.)* Legibility rides the same
> conventions as the cupboard: the covering table recolours to **Owned** while
> it conceals you (§11.3), the crouch reports itself once as an Owned message,
> and the §11.5 danger overlay spares your cell — red under you always means
> *detected*.

> **The hideout is a cupboard, and entering it is a decision.** You **bump** into an
> empty cupboard to climb in (§4.3 — hiding is an *interaction*, not a cell you
> drift onto), and you move off it to climb out. While you are inside you are
> **concealed** — no guard's cone detects you, so this is the "hold still, watch the
> cone sweep past" of §7.6 — and the cell is **solid**, so a guard cannot walk into
> your space (capture is contact, §4.5; a cupboard is the one place contact is
> refused because a patrol routes *around* it). The occupied cupboard also
> **recolours to Owned** (§11.2/§11.3) so you can always see which cell you are
> hidden in.
> Placement is the generator's job (§10.1.6); this behaviour — bump-to-enter, the
> concealed state, and the occupied glyph — is the hideout **interaction** ticket,
> which the turn loop, the renderer, and vision (§6) complete together. **Whether a
> guard can ever flush you out** (search a cupboard when alerted) stays **[OPEN]**
> (§15 Q5).

### 10.4 Doors

A door is a span of **3–6 cells**: a **hinge at each end** (permanently solid,
opaque — they're the frame) and **1–4 panels** between them that open and close as
one unit.

- **Bump a panel to open. Bump a hinge to close.** The hinge is the handle, and
  it's why hinges stay solid forever.
- **Anyone can operate any door.** No keys, no locks. **[START]** — keys are an
  obvious future axis, and one the fiction supports.
- **A door cannot close if anything occupies a panel cell.** Doors never crush
  anyone.
- **Closed panels do not block pathfinding** — deliberately. Guards route through
  closed doors and open them by walking into them. **This has a big emergent
  consequence: guard traffic monotonically opens the facility up over a level, and
  every opened door is a permanent new sightline.**
- **Auto-close.** The old version had none — every door stayed open forever, so
  connectivity only ever increased and the level decayed into an open plan.
  **Doors should close behind their user** **[START]**, which restores the level's
  structure over time, keeps sightlines from decaying into an open plan, and turns
  an open door into evidence that someone passed.

### 10.5 The spatial model — fix this properly

The old version had exactly one spatial abstraction: **an axis-aligned rectangle**.
It was asked to be the level bounds, the partition regions, room identity, guard
patrol territory, *and* the UI viewport. It was not up to any of it.

The problems, which are worth understanding because they cascade:

- **It cannot describe the spaces the game has.** A room with a pillar isn't a
  rectangle. An L-shaped nook behind a stub isn't a rectangle.
- **Corridors are not regions at all.** They're painted into the plan and never
  recorded. So the connective tissue where most stealth gameplay happens is
  *spatially unaddressable*. Nothing can ask "which corridor is this?" or "does
  this corridor reach that room?".
- **The regions are generation scaffolding that gets thrown away.** Once the level
  exists it has **no concept of rooms**. No registry, no cell→room lookup.
- **Therefore everything downstream has to fake it.** Guards patrol a box around
  wherever they spawned, because there is no vocabulary in which to say "cover the
  east wing". *That* is why guard cooperation, assigned patrols, in-level lore
  placement, keys, and circuits all stayed unbuilt — they were all blocked behind
  this one missing abstraction.

**The generator already builds a graph — corridors are nodes, rooms are nodes,
doors are edges — and then discards it.** **[SETTLED]: keep the graph.** The
level's spatial model should be named regions of arbitrary shape (including
corridors), explicit door edges between them, and a cell→region lookup.

This is the highest-leverage structural decision in the document. Nearly every
"guards should…" idea depends on it.

### 10.6 Guarantees

**Guarantee, and test:**

| Guarantee | Basis |
|---|---|
| Fully enclosed | Unconditional border ring |
| Corridor network connected | Each corridor punches into its parent → the network is a tree |
| Every room reaches a corridor | Every room is bounded by corridor walls, which qualify as door candidates |
| Every room ≥ 6×6, ≤ ~12 rooms | Partition constants |
| **A path exists: start → every objective → exit** | **Assert it. See below.** |
| **One usable beside any floor cell (preferred)** | Conflict-aware stamping, best-effort; the arrow disambiguates the rest. See below. |

**The old generator never verified solvability.** It relied on the structural
argument above — which has a hole: **a wall run shorter than 3 cells gets no
door.** Punch-throughs fragment wall lines, and if every run bounding a room came
out < 3, that room seals, with its objectives and guards inside. Nothing detected
it, nothing repaired it, and no seed was ever rejected.

**Do not rely on a structural argument. Assert reachability and reject the seed.**
It is a flood fill. It costs nothing. It is exactly the kind of property a
generator must never merely *believe*.

**One usable per cell — a preference, not a guarantee.** The usable line
(§11.4) points each bump with its own arrow, so a floor cell beside **two
distinct usables** (a door, a table, a cupboard, a console, the exit; a
multi-cell door counts once) is still *legible* — `→ door: open` and `↑ table:
crouch` are two aimed actions, not one ambiguous prompt — but it reads cleanest
at one. So every stamping stage **avoids crowding where it cheaply can**:
cupboard sites that would double up are skipped (sites are plentiful), and
console and exit candidates prefer a clean cell, falling back rather than
failing the draw.

**Two of the §10.6 guarantees outrank it, so it is not asserted.** Connectivity
and the sightline rule (§10.1a) come first, and §10.1a's repairs must land where
the run is — a bench beside a room's door span, a repair cupboard close to an
existing usable — so a doubling with a nearby door is sometimes unavoidable.
Forcing the piece off-centre to dodge it only shortens the run instead of
splitting it, multiplying generation cost for a cosmetic win. And structural
doors can cluster in a way no carve undoes. The honest rule is therefore
best-effort placement plus the arrow — *not* a flood-fill-style
assert-and-redraw. (An earlier draft made it a hard guarantee; measured, it
rejected ~85% of carves and stalled generation — the arrow already buys the
legibility the guarantee was chasing.)

Also worth fixing, all real:

- **No spacing guarantees at all.** Nothing separates the player from the exit
  (they can spawn adjacent). Nothing spreads intel out — all 3 can land in one
  room. Nothing keeps a guard from spawning where it sees you on turn one. The
  pillar says *"the starting area should be safe"*; make it so.
- **Placement can fail silently.** Guards got 10 attempts, then were quietly
  dropped — you asked for 5 and got 4 with a log line nobody read. Objectives got
  100 attempts and then threw. Neither is acceptable: **fail loudly or retry the
  seed.**

---

## 11. Presentation

### 11.1 The grid

The game renders to a **grid of cells, each a character plus a foreground colour
plus a background colour**. **[SETTLED]**

This survives the terminal's removal for a specific reason: **it is a pure
function of game state, and it prints as text.** That makes the entire UI
assertable in a test without a browser — which is what makes UI iteration
agent-checkable. It is also the cheapest possible art pipeline, and it is the
game's identity.

The renderer is a **separate concern behind one interface**. ASCII now; a tile
renderer later is a second implementation of the same interface, swapping
`fillText` for `drawImage`. The core must not know which is in use.

### 11.2 Colour

Colours are **not chosen by game systems**. Systems declare an **information
category**; presentation owns the mapping. **[SETTLED]** — this is a genuinely
good piece of the old design. Recolouring or reskinning for accessibility is a
one-table edit.

| Category | Colour | Meaning |
|---|---|---|
| **Neutral** | White | Inert scenery, spent objectives |
| **Ground** | Dark gray | Traversable floor — the §11.5 dots, drawn to recede |
| **Owned** | Blue | You, and things you made |
| **Caution** | Yellow | A threat that is unaware |
| **Warning** | Orange | A threat that is hunting |
| **Danger** | Red | A threat that has you |
| **Sensed** | Orange (background) | A guard **sensed through a wall** (§9) — an eye-catching cell highlight, position only, mind and facing unknown |
| **Interest** | Purple | Goals and rewards |
| **System** | Tan | Doors, hideouts — neutral furniture |

**A guard the player can *see* is re-categorised every turn from its own state**,
so the player reads the AI state machine directly off the colour of `g`: yellow →
orange → red *is* the guard's mind, visible. Message colour uses the same table,
so a red near line (§11.4) and a red `g` reinforce. **A guard the player only
*senses* (§9.2) has no readable mind** — its cell renders with the flat **Sensed**
**background** highlight (orange), a filled marker that says *a guard is here* and
nothing about what it is doing — the eye-catching parallel of the red danger
overlay, orange not red. The bloom from an orange cell to a state-coloured
`g`-with-cone *is* the seen/sensed distinction, made visible. Keep all of this.
*(Sensed reuses Warning's orange hue but only ever as a background, never a glyph,
so the two never collide; the old §9.3 cyan "Noise" slot — a heard sound's source
— is freed, since sound is gone.)*

Base palette: a 16-colour, colour-blind-safe qualitative set, each usable as
foreground and as a darkened background variant.

> The old palette pushed every colour through a gamma curve that compressed
> everything into 0.1–0.9, so **there was no true black and no true white** and
> the whole image sat in a washed, low-contrast band. Six of the sixteen colours
> were never used at all. **[START]** — start with full-range colour and add
> compression only if something demands it.

### 11.3 Glyphs

| Glyph | Entity | Category |
|---|---|---|
| `@` | Player | Owned |
| `@` | Decoy | Owned |
| `g` | Guard, **seen** | Caution / Warning / Danger, by state — plus facing + cone (§9.2) |
| *(none)* | Guard, **sensed** (through a wall, §9.2) | **Sensed** — an orange **background** highlight on its exact cell (no glyph of its own), no cone; blooms to the state-coloured `g` once seen |
| `z` | Body | Caution |
| `#` | Wall | Neutral |
| `·` | Floor | Ground — recessive by design; blank until the §11.5 floor dots gave it a glyph |
| `+` | Door panel | System |
| `×` | Door hinge | System |
| `}` | Hideout (empty) | System |
| `}` | Hideout (occupied) | **Owned** — you are in it, so it recolours to Owned (blue) like the rest of "things you made"; the colour shift is how you see which cell hides you (§10.3) |
| `π` | Partial cover (table) | System |
| `π` | Partial cover, concealing you | **Owned** — the same convention as the occupied cupboard: while you are crouched behind it, the covering table recolours to Owned, so the blue `@`-`π` pair reads as one hidden unit (§10.3) |
| `$` | Intel | Interest |
| `E` | Exit | Interest |

**Overlapping glyphs need a priority order.** The old version was
last-writer-wins, so a guard in a doorway rendered arbitrarily. Define the order.

### 11.4 Layout

**[SETTLED]** — **the whole level on screen, no camera, no scrolling.** The
screen *is* the board. The danger overlay (§11.5) only earns its "the lose
condition, painted" title when the player can see all of it at once; a scrolled
map hides exactly the threats a plan needs to account for, and demands camera
math, off-screen threat indicators, and scroll handling in exchange. The level
is therefore **screen-bound**: it cannot outgrow what one screen shows legibly.
The scale axis beyond that is **more stories, not a bigger grid** — multi-story
facilities (stairs, elevators, one story per screen) are parked in the §14
backlog until a single screen-bound story proves fun.

```
┌─ map: the whole story, fixed ──────────────────────────────┐
│    ################                                        │
│    #              #        ####                            │
│    #    $         +        #  #                            │
│    #              #        #} #                            │
│    ##########×#####        ####                            │
│                                                            │
│              g                                             │
│    ##############        #########                         │
│    #            #        #       #                         │
│    #      @     ×        +   g   #                         │
│    #            #        #       #                         │
│    ##############        #########                         │
│                       E                                    │
├────────────────────────────────────────────────────────────┤
│ Radio: a guard has gone silent                             │ ← near line
│ → door: open                                               │ ← usable line
└────────────────────────────────────────────────────────────┘
```

- **Map**: the full story, statically fitted to the screen (scaled, aspect
  preserved). No camera.
- **Near line** — *what is around you*: the highest-priority live message
  (§11.7) — guard caution, a radio event (§7.3), an alert change, intel
  collected — drawn as a **solid band in the message's category colour**. Threat reads as a
  colour flash across the bottom of the screen, legible without reading the
  words; that's a nice piece of design — keep it. When no message is live, the
  line falls back to quiet **ambient status** (alert level, an active ability's
  remaining turns) instead of sitting empty.
- **Usable line** — *what you can act on*: the bump affordances adjacent to the
  player right now, each **with an arrow giving the bump's direction** (`→ door:
  open`, `↑ console: take intel`, `← table: crouch`, `↓ cupboard: hide`). Not a
  message — a **pure derived function of state**, recomputed every frame, no
  plumbing. Empty when nothing is adjacent. The arrow makes each bump an aimed
  "press this way, get that", so even the rare cell beside two usables stays
  unambiguous — one row lists each with its own direction. The generator
  *prefers* one usable per floor cell (§10.6, best-effort) to keep the common
  case to a single line, but does not guarantee it.

**No ability column.** The old fixed 14-column list spent a seventh of the
screen on information consulted once a minute. Ability state (ready / active
`[3]` / cooling `/2/` / unusable) must stay *discoverable*, but where it lives
is **[OPEN]** — first experiment: **show the ability list while waiting**
(peeking costs a turn, which is exactly the §2.3 "cost is load-bearing"
principle applied to UI); alternatives: a toggle key, or a strip that appears
only while something is active or cooling. Whatever wins, hotkeys stay explicit
and stable (§11.6) — never dependent on a visible list.

### 11.5 Field of view and the danger overlay

Field of view controls *lighting*, not knowledge — what is **fogged** is settled
separately in §11.5a, and the two are independent. This section is about how live
visibility is drawn.

| Cell state | Rendering |
|---|---|
| In player's FOV | Full category colour |
| Outside player's FOV | Same glyph, dark gray — dim but legible. Two exceptions: Ground dims further (the dots whisper), and the exit keeps a dark Interest tint — it anchors every escape plan (§7.6) and must not sink into wall gray |
| Watched by a guard, in player's FOV | **Red background** — the danger overlay |
| Watched by a guard, outside player's FOV | Dark gray on dark gray — *unreadable* |
| A guard **sensed but not seen** (§9.2), any FOV | Its cell gets the orange **Sensed** background highlight regardless of line of sight; **no cone, no danger overlay** — position is known, attention is not. Where a *seen* guard's cone also watches the cell, the red danger overlay wins (being seen outranks) |

Note the sensed highlight and a guard's *own* danger overlay never coincide: a guard
you can only sense projects no overlay (you cannot see its cone), and the instant you
*can* see it the orange highlight blooms into the full state-coloured guard and its
cone paints the overlay. The overlay stays exactly what §11.5 promises — *the detection set you
can see* — never a guess.

**The danger overlay is the best idea in the old game.** It paints the *literal*
detection set — the same data the AI queries. If your cell isn't red, no guard you
can see will detect you. **The lose condition, painted.** It makes stealth
plannable rather than guessy, which is the whole "enough information to strategise"
pillar. **[SETTLED]** — keep it.

Two problems from the old version to fix:

1. **Watched-but-unseen cells render dark gray on dark gray** — so the red
   downgrades to grey and the *safest-looking cells on the map are the watched ones
   you can't see into*. Actively misleading. Fix.
2. **FOV is invisible on open floor.** Floor is a space, a space has no
   foreground, so the dimming that encodes the FOV boundary is undetectable across
   open ground. You can only see the FOV edge where it crosses a wall. **Render
   floor as dots.** Trivial fix, big legibility win.
### 11.5a Fog: the layout is visible, the contents are hidden

**[SETTLED]**

| Layer | Visibility |
|---|---|
| **Geometry** — walls, corridors, doors, room shapes | **Always visible, from turn one.** Never fogged. |
| **Contents** — intel, hideouts, equipment, lore | **Hidden until seen.** Once seen, remembered. |
| **Live state** — guards, bodies, door open/closed, danger cones | **Only what you can see right now.** Never remembered. **One exception: a guard's *position* is also known through walls within the guard-sense range (§9)** — but only its position, never its cone, and never remembered once out of range. |

This resolves the tension between two pillars that pull against each other:

- *"Enough information to design a strategy"* → **you can always read the
  building.** You can plan a route, spot the chokepoints, pick your escape path
  before you take a single risky step. Route-planning is a first-class activity
  from turn one, and it stays one — you are never lost, never mapping.
- *"Some surprises should force adaptation"* → **you never know what's in it.**
  Where the intel actually is, where the hideouts are, where the guards are right
  now. Exploration has something to find, and the thing it finds is the thing that
  changes your plan.

The pairing is the point: **you plan confidently and then get surprised by
contents, not by architecture.** Being surprised by a wall is annoying; being
surprised by an empty room where you expected the intel is a *decision*.

Note the interaction with §7.6: a known layout means **you can plan your escape
route before you're spotted**, which is exactly the *"the player can plan escape
routes for when failing at stealth"* pillar. A player who is chased and improvising
in unknown geometry is not playing a stealth game, they're rolling dice. This is a
big part of why the layout stays visible.

Hideouts being **hidden until seen** is a notable consequence — the flight paths
you scouted are worth more than the ones you didn't. That's a good reward for
thorough exploration, and it's the pillar *"thorough exploration is rewarded"*
finally having a mechanism.

> **Implementation note.** "Remembered" content needs its own visual state,
> distinct from both *live* and *never-seen*. Three states, not two. The old
> version had no memory system at all, so this is new — don't assume the dimming
> scheme above covers it.

### 11.6 Input

| Key | Action |
|---|---|
| Arrows / `4` `6` `8` `2` | Move |
| `5` / `w` | Wait |
| `Enter` / `Space` | Confirm |
| `Escape` | Cancel / menu |
| Letters | Ability hotkeys |

**Assign ability hotkeys explicitly.** The old version derived them from the
label — each ability claimed the first letter not already claimed by one above it
— which meant `Dephase` became `e` because `Decoy` took `d`, and **an ability's
key silently changed when the list above it changed.** Muscle memory is not
optional in a game where a mis-key ends a run.

**Touch is a real target and was never finished.** The manifest pinned landscape
and installed standalone, but the options dialog could not be closed by touch and
the pause menu could not be opened by touch — together making it unreachable *and*
inescapable. Either build touch properly or don't ship the manifest. **[OPEN]**

### 11.7 Messages

Messages feed the **near line** (§11.4). The usable line is *not* part of this
system — it is derived from adjacency every frame and carries no state.

- Messages carry a **category**, a **priority**, and optionally a **source cell**.
- The near line shows only the **highest-priority** live message.
- **Messages clear on the player's next action** — a status line, not a
  scrollback — falling back to the ambient status of §11.4, never to an empty
  row. **[START]** — the old TODO wanted an expandable log, and with radio pings
  (§7.3) there is more to say, so this probably needs to grow.
- Modal messages anchor **near their source cell**, positioned so they never cover
  what they're talking about. That's a nice touch; keep it.

Priority ladder **[START]**: routine self-narration ≤ 0; guard threat escalates
2 → 4 → 10; objective feedback dominates at 20; ambient status sits below
everything (it is the floor, not a message).

---

## 12. Architecture

### 12.1 Principles

1. **The core is pure.** Game logic knows nothing about rendering, input, the DOM,
   the clock, or the platform. `state × input → state, events`.
2. **The core is deterministic.** Same seed, same inputs, same result. Always.
3. **Rendering is a pure function of state**, producing the character grid.
4. **The core is testable natively, in milliseconds, with no browser.**

Every one of these exists to make experiments cheap. They are not architectural
aesthetics.

### 12.2 Layout

```
crates/
  core/    pure game logic. No wasm, no I/O, no platform. Fast native tests.
  web/     wasm-bindgen + canvas2d renderer + input. Thin.
  sim/     headless harness: run N seeded games, emit metrics. (§13)
web/
  index.html, font, assets
```

**Language: Rust, compiled to wasm via wasm-bindgen.** **[SETTLED]**

The reasons, in order: enums plus exhaustive matching mean that **adding an
ability variant surfaces every site that must handle it** — the compiler
enumerates the work, which is exactly what you want when an agent is making the
change. Determinism is easy. Tests are sub-second. The bundle is small. The cost
is a compile loop of seconds rather than milliseconds; that's the price.

**Renderer: hand-rolled canvas2d.** **[SETTLED]** A glyph grid is ~200 lines and
near-zero dependencies. A game engine would fight every other decision here: it
would own the main loop, inflate the bundle, and slow the compile — all to draw
characters in a grid. Tiles later are `drawImage` instead of `fillText`, behind
the same interface.

### 12.3 Data model

**Plain structs and an arena, not an ECS.** **[SETTLED]**

A level holds its player, its guards, its doors, its bodies, with generational
ids for references. At a few dozen entities, archetype storage solves a problem
this game does not have. More importantly, an ECS's dynamic queries **hide exactly
the coupling that should be visible** — when an ability touches guards and doors
and vision, the type system should say so out loud.

### 12.4 Determinism

**This is the load-bearing decision, and it is not a nice-to-have.**

- **One seed per run.** Everything random derives from it. **[SETTLED]**
- **Pin the PRNG algorithm.** Do not use a generator whose implementation may
  change between library versions — the standard one in Rust's `rand` explicitly
  does not guarantee reproducibility across releases. Use a small, explicitly
  versioned algorithm.
- **A replay is `(seed, [inputs])`.** Nothing else.

What this single property buys:

| | |
|---|---|
| **Bug repro** | A bug report is 40 bytes. |
| **Seed sharing** | "Try seed 8371, it's brutal" — a real playtest tool. |
| **Bot metrics** | §13 is impossible without it. |
| **Golden tests** | Replay a run, assert the final grid. |
| **Regression detection** | Same seed + inputs → different result = you changed the game. Often the *whole* test. |
| **Rewind** | The "go back a few turns" ability becomes replay-minus-N instead of a nightmare. |

> The old version got here by accident and paid for it. Nothing was seeded — every
> generator built its own fresh unseeded source, and the in-level random source
> handed out **a brand new generator on every single call**. "Play again" worked
> only by serialising a byte-for-byte snapshot of the entire level at run start
> and restoring it. That's a heavier, more fragile way to buy less.

### 12.5 Saves

Serialise the run state (seed, level, progress) to browser local storage.
**[START]** — with true determinism, `(seed, inputs)` is also a valid save, and a
much smaller one. Snapshotting is simpler and survives design changes; a replay
save doesn't. Probably: snapshot for saves, replay for tests and bug reports.

---

## 13. The experiment loop

This is the point of the whole rebuild.

### 13.1 Now: you play, agents build

v1's loop is short build→play latency. Agents ship experiments; **you play and
rule.** Fun is a human judgement.

What this needs: a fast build, a deploy preview, and **seed sharing** so a specific
interesting level can be handed around and replayed exactly.

### 13.2 Early goal: the headless sim

**Not a CLI.** The player-facing terminal UI is gone for good. This is a `sim`
target that runs *N* seeded games with a scripted or bot player and emits numbers.

**An agent cannot playtest a canvas. It can playtest a headless sim.** This is the
difference between an agent that writes plausible-looking abilities and an agent
that can tell you whether one is any good.

Metrics to start with **[START]**:

| Metric | What it catches |
|---|---|
| Win rate | Difficulty, obviously |
| Turns to win | Pacing, and the "don't drag exploration" pillar |
| **Ability usage histogram** | **Dominant strategies and dead abilities.** This alone would have caught the free neutralise on day one — 94% usage is a scream. |
| Detection events per run | Whether stealth is actually happening |
| Takedowns per run | Whether §7.2's cost is real |
| Bodies found by guards | Whether §7.3's clock has teeth |
| Alert peak | Whether escalation escalates |
| **Strategy diversity across seeds** | **Boredom.** If every seed is solved by the same ability sequence, the game is a puzzle with one answer. |

That last one is the most important and the least obvious. **Win rate tells you if
the game is hard. Strategy diversity tells you if it's interesting.** They are not
the same, and only the second one was ever the problem.

### 13.3 Later: bot metrics guide, you decide

Bots narrow what's worth playing. They never rule on fun. The loop:

**bot flags a suspicious signal → you play the seeds it flags → you rule.**

### 13.4 What the sim is not

It is not a difficulty oracle. A bot with perfect information and no fear plays
nothing like a human — it will happily take a 5% capture chance forever, and it
cannot be bored. **Treat bot output as a smoke detector, not a judge.**

---

## 14. Roadmap

### v1 — quick play, and nothing else

One generated facility. Sneak in, take the intel, get out.

Included:

- 40×40 generation with the full corridor-partition algorithm, features, hideouts
- **Corridor cover + the sightline assertion (§10.1a)** and reachability (§10.6)
- Guards: cones, patrols, chasing, **a chase that can end (§7.6)**, searching,
  radio (§7.3), cooperation
- **The guard sense** (§9) — vision-only guards, the player senses guard positions through walls
- Innate abilities + the starting tech set
- Takedowns, bodies, dragging, hiding
- The character grid, danger overlay, the near + usable status lines (§11.4)
- **Visible layout / hidden contents, with tile memory (§11.5a)**
- Seeded determinism + seed sharing
- Native test suite + golden grid tests

Explicitly **not** in v1:

- Story mode, the facility map, campaign progression
- Saves, options
- Intel as currency
- Tiles, touch, audio

**Why:** *find out whether the loop is fun before building scaffolding around it.*
The old version had a facility map, a campaign, story conversations, an unlock
screen, save/load, and a config system — wrapped around a core loop that had a
free win button. **Everything outside the loop was scaffolding around an
unanswered question.** Don't do that again.

**The one question v1 exists to answer:** *is the hiding game fun?* (§7.6.) Break
sight, get hunted, hold still, slip away. If that loop isn't tense, nothing built
on top of it will save it — and every hour spent on a campaign before knowing is an
hour bet on an unanswered question. **If v1 says no, change the loop, not the
scaffolding.**

### v2 — the loop is fun; make it a game

- The headless sim + metrics (§13.2) — *this may well come before v2 proper*
- Saves, options, a help screen and a legend (there was never a legend; nothing
  ever explained what `$`, `E`, `}` or `z` meant)
- A game-over screen that says **why you lost** (the old one didn't distinguish
  victory from defeat at all)
- An alert indicator

### v3 — the campaign

**The campaign is the run** (§2.2): 2–3 hours, progression throughout, nothing
carried to the next one.

- The facility map. **A graph with real edges** — the old "map" was a flat list
  with no adjacency and no geography, where every unlocked facility was always
  selectable. Geography should mean something.
- **Salvaged tech accumulating across facilities.** This is the run's power curve
  and it is the reason the campaign exists. It was fully built last time and
  reachable by nobody: no facility was ever generated with an equipment cache, so
  no ability could ever be unlocked. The progression axis existed only on paper.
- Intel as a real currency, with actual sinks: reveal facility intel, unlock an
  alternative route, lower the alert, upgrade an ability.
- Difficulty that scales with the alert level. **The whole point of the alert
  system is that being loud in facility 2 makes facility 3 harder.** Until that
  loop closes, alert is decoration.
- An ending. The old campaign had no reachable conclusion.

### Later — the idea backlog

Deliberately parked. Each is an experiment for the loop in §13, not a commitment:

- **The prison level** (§2.2) — capture drops you into a cell with a chance to
  break out and rejoin the run, rather than ending it. Softens the 2.5-hour cliff
  **without** adding meta-progression, and the fiction is perfect. Needs the
  pressure it relieves to be shown to exist first.
- Smoke screen
- A deployable drone with its own abilities
- Rewind a few turns *(nearly free given §12.4)*
- Keys and locked doors
- Electrical circuits and powered doors
- In-level lore
- Ability upgrade trees
- Low walls / vaulting *(partial cover itself shipped as the §10.3 table)*
- **Multi-story facilities** — stairs and elevators, each story contained on one
  screen (§11.4). The scale axis once a single screen-bound story is fun. The
  §10.6 solvability flood must then span stories (start → every objective → exit
  *through* the stairs); elevators are the interesting object — a chokepoint, a
  sightline that opens and closes, maybe a door that moves.
- Tiles

---

## 15. Open questions

Genuinely undecided. Listed so they get decided deliberately rather than by
default.

**The first two gate the fun. The rest can wait for play evidence.**

1. **How does a chase actually end?** (§7.6) The two-zone proposal — certain ≤ 5,
   glimpse 6–10 — is a *proposal*, not a decision. Alternatives worth trying:
   guards tire and break off pursuit after *N* turns without closing; a chasing
   guard's cone narrows (tunnel vision) so corners work; sight range falls off with
   distance properly. **This is the most important open question in the document.**
   The known-bad answer is "guard tracks you perfectly at any range within 10" —
   anything is better than that, so try several.
2. **How much cover do corridors need, and how does the generator place it?**
   (§10.1a) The *L* ≤ 10–12 sightline rule is a guess. Jogs vs. features vs. both
   is untested. Too much cover and the building stops reading as a building; too
   little and §7.6 comes back. **Directly gates whether the hiding game exists.**
3. **How much does the guard sense give, and how does it tune?** (§9) The mechanic
   is settled — vision-only guards, the player senses positions through walls — but
   the dials are open. Range `10`, `20` on wait: on a 40×40 map, does waiting come
   too close to omniscient (every guard a dot for one turn's cost)? Should the
   sensed dot stay a **flat presence marker**, or convey *some* state — say, a
   distinct tint when a guard is Chasing — trading legibility for tension? Does it
   name *which* guard, or just "a guard"? *(This slot used to be "how is sound
   presented?" — the deepest UI problem in the old design. Dropping sound for the
   sense (§9) dissolved it: the sense's presentation is "draw the dot." What's left
   is tuning, not a research problem — which is why this no longer gates the fun.)*
4. **Run score.** Does a run have a score? If so, takedowns cost score — giving
   "no killing" mechanical teeth via a leaderboard rather than a rule, and creating
   a ghost↔aggressive play spectrum. If not, the radio clock (§7.3) is the only
   takedown cost, which may well be enough. Note a score also gives the bot in §13
   a far better objective function, which is an argument for it beyond the game.
5. **Do guards check hideouts?** If never, hideouts are permanent safe rooms and
   patrol coverage has holes by design. If always, they're death traps. Probably:
   **only when alerted, and only if they saw you go in or found a body nearby.**
   Interacts hard with §7.6 — a hideout that gets checked during a search is a much
   more interesting object than one that doesn't.
6. **Sight and sense: box or circle?** The box is cheap and nobody noticed. A
   circle is more natural and slightly less exploitable at the diagonals. Whatever
   wins should apply to **both** the vision box (§6.1) and the guard-sense box (§9.1)
   — they are the same shape at different sizes, and splitting them would be a
   needless inconsistency.
7. **Does the one-turn cone lag come back?** (§4.2) It was a bug that created a
   real mechanic — a *moving* guard checks stale ground, giving a reliable one-turn
   window that a *stationary* guard doesn't. Note this would partially address §7.6
   on its own. If reintroduced it must be deliberate, stated, and visible in the
   danger overlay.
8. **Touch.** A real target, or drop the manifest? Half-built touch is worse than
   none — the old version could trap a touch user in a dialog they couldn't close.
9. **Where does ability state live on screen?** (§11.4) The fixed column is gone.
   Show-on-wait (peeking costs a turn) is the first experiment; a toggle key and
   an only-while-active strip are the alternatives. Constraints: ready / active /
   cooling must stay discoverable, and hotkeys (§11.6) must never depend on a
   visible list.
