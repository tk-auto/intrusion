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
- **Win: grab the intel, then return to your entry point.** You leave the
  way you came in. Bumping the exit before you hold enough intel refuses, with a
  message. **How much is "enough" is a level modifier** (`intel_to_exit`, §12.6/#244),
  not one fixed rule, so the modes gate the *same* facility differently:
  - **Quick play — all the intel** (§10.2/#244). Gather the whole set, then get out:
    a complete objective, and v1's default mode.
  - **The sim — at least one** (§13.2/§13.3). One objective is a complete run for the
    bot; the all-intel march kept it in the facility long enough to be caught nearly
    every seed, so the shorter gate keeps the outcome profile mixed. **[START]** on
    the value.
  - **Campaign — none** (§14 v3). Intel is currency (§2.2), not an exit key, so the
    exit never refuses.

  Pressing on for more than the gate demands is what an aggressive style trades extra
  exposure for. The gate is part of the run's reproducible config, carried in the
  shareable level-seed token (§12.4/#245/#333).

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

One thing erodes it **on purpose**: the **Vision** passive (§8.3/#265) lifts the
held player's arc to 360° and their range to 20, removing the "can't see behind
you" constraint that makes Wait and corners tense. That is not casual — it is a
permanent loadout slot spent to buy it, and it is watched in the sim for whether
it trivialises the hiding game.

---

## 6. Vision

### 6.1 Rules

- Sight is a **facing-dependent forward cone**, blocked by walls, closed door
  panels, and hinges. An opaque cell is itself seen — you see the wall face — but
  shadows everything behind it.
- **Range is a square box, not a circle.** Range 10 means a 21×21 box. There is
  no distance falloff. **[START]** — a circle would be more natural; the box is
  cheap and was never noticed. Worth trying.
- **The 8 cells immediately around a viewer are always seen** — with one
  exception, the guard rear blind spot below. For the **player** this is
  unqualified and **[SETTLED]**: you can never stand adjacent to the player
  undetected, in any direction including directly behind.
- **Guard rear blind spot (§155).** A guard does **not** detect the three cells
  at its back — the two rear diagonals (tier 4) and directly behind (tier 5) of
  §6.2. Its *forward* and *side* (tier 3) cells still detect, so you can never
  stand **beside or in front of** a guard undetected — but you *can* slip
  **directly behind** an unaware one. This narrows the old blanket 360° ring for
  guards only; it is the deliberate revision that makes a behind-the-back
  Takedown (§7.2) approachable. The rear cells stay §6.2 cone-carving walls (the
  wedge silhouette is unchanged) — only their membership in the *detection* set
  is dropped. Pairs with the patrol dwell (§7.5): a window to act, and a back to
  approach.
- **Auto-peek — the player only.** **[START]** The player's sight is the union
  of the cast from their cell and a cast from the cell one step ahead along
  their facing — where the head would be if they leaned forward — clipped to
  their own range box (on open floor the union adds nothing). It reads around
  adjacent corners and out of cupboard mouths: a hidden player watches the
  corridor at ~180°, not the mouth's ~90° wedge. **Guards never peek** — a
  corner the player can read still breaks a guard's line (§7.6), and detection
  stays with the guards' own cones, so the peek is an information channel in
  the §9 spirit, one-sided by design.

### 6.2 Implementing the cone

A symmetric shadowcast over the square box, with one trick: **the cone is
produced by treating the out-of-arc cells of the viewer's own 8-neighbour ring as
if they were walls.** Shadowcasting propagates outward, so those artificial walls
cast the shadows that carve the cone. Because artificial walls are still marked
seen — exactly like real ones — you get the 360° touching ring for free.

For a **guard**, the three rear ring cells (tiers 4–5) are then dropped from the
detection set — the §6.1 rear blind spot (§155). They still act as artificial
walls during the cast, so the wedge silhouette is untouched; they are only
unmarked afterwards, so the guard simply does not *notice* what is at its back.
The player keeps the full ring.

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

Because of the touching ring (§6.1), **an aware guard can always see you when you
are adjacent to it — beside it or in front.** The one gap is its **rear blind
spot** (§6.1/§155): the three cells directly behind and rear-diagonal do not
detect. So a takedown is possible either against a guard made unaware in front —
arranging to be adjacent without ever having been in its cone, a puzzle of
geometry, timing, doors and distraction — or by reaching the cell **directly
behind** an unaware guard. The rear approach is the intended path, and it needs a
window: pair it with the patrol dwell (§7.5), because a guard that never stops
moving cannot be lined up on. Either way it is not a button.

**The body is the cost.** A body is a **non-solid** object — it lies on the floor
and blocks nothing (neither movement, pathing, nor sight). Its cost is not that it
is a wall; it is that it is **evidence on a clock**:

- **Can be seen.** Any guard whose cone covers a body has *found* it — the loudest
  event in the game (below).
- **Can be moved.** You **drag** it (§8.3) — slowly. You take hold by walking over a
  body and stepping *off* its cell; you drop it by bumping it.
- **Can be hidden.** Stow it inside a cupboard (§10.3) and it is **gone** — no cone
  ever finds it. The cupboard is then **locked**: a body is in it, so it is no longer
  a hideout.
- **Runs a clock.** A downed guard misses its radio pings (§7.3); hiding the body
  confuses that investigation, it does not stop it.

> **Why non-solid `[SETTLED]`.** An earlier rule made a body a solid obstacle
> (fill 1.0). It read well — the body as a thing in the way — but it manufactured
> two soft-locks: a body dropped on a chokepoint could permanently freeze a guard
> pathing past it (#182), and a takedown from a cupboard could drop the body onto
> the cupboard's only mouth and trap the *player* inside (#170). Both are the same
> failure — an unmovable body becomes a wall nobody can pass — and §2.2 forbids a
> run ending to a dead end rather than a decision. Making the body non-solid deletes
> the whole class at the root; the cost stays real (it is loud evidence, it must be
> dragged and hidden, and it runs the §7.3 clock), it is just no longer a wall you
> can build against yourself.

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
| Missed ping → | Control dispatches the nearest active guard to the **takedown location** — the cell the guard fell in — where it **searches** (§7.6) rather than merely standing, **and** the facility alert steps to rung 1 (the ladder below) |
| Second missed ping → | Control gives up on the post and stops calling it. It escalates nothing on its own: a post already known to be quiet tells control nothing new, so the ladder counts **bodies**, not pings |

Why this is the right mechanic:

- **It makes takedowns a clock, not a cost you pay once.** Every guard you take
  down is a future appointment. Three takedowns is three clocks running at once.
  The strategy *scales badly on its own* — no rule is needed to ban a full clear,
  it collapses under its own weight.
- **It is diegetic and legible.** The player can *read* the pings — a near-line
  message when control pings, and, because guard positions are sensed through
  walls (§9), the dispatched responder is a **dot that visibly peels off toward
  the place you struck.** So the player knows the clock exists and roughly when it
  fires. That makes it plannable, which is the pillar about giving enough
  information to strategise. *(This tell was going to be a **sound** — a ping the
  player heard, §9. Sound is gone; make the tell visual from the start.)*
- **It gives moving a body a real payoff.** Control's last fix on a guard is where
  it went down, so that is where the responder searches — and a body you dragged
  or stowed elsewhere is no longer there when it arrives (§8.3). A hidden body
  still misses its ping: hiding it buys you the *investigation being confused*,
  not the investigation not happening.
- **It creates the escalation the alert system was always supposed to provide**,
  from a concrete, explainable source rather than a global number.

#### The alert ladder — what an escalation actually does

**[SETTLED]** — three rungs, fixed triggers, cumulative retaliation, **no decay**.
The old alert was §2.3's worst row: *never written to, never read*. A number that
announces an escalation which does not exist is worse than the old silence, so the
rung is defined by what it *does*.

| Rung | Triggers (**any** of) | Retaliation **added** at this rung |
|---|---|---|
| **1** | A confirmed sighting; **or** one missed radio ping | Guards are **never calm**: the §7.5 patrol dwell drops from **3–7** to **1–3** turns **[START]** |
| **2** | **3** confirmed sightings (cumulative) **[START]**; **or** an intel console tampered with while at rung ≥ 1 | **+1 guard** enters the facility |
| **3** | A body found; **or** two missed pings across **two** bodies | **+2 guards** enter the facility |

**Effects are cumulative.** A rung applies every effect at or below it, so a run
driven from 0 straight to rung 3 gets the dwell cut **and** +1 **and** +2 = three
extra guards. Rung 3 is the top; there is no rung 4.

**A confirmed sighting** is **3** turns **[START]** in which *any* guard has the
player in the **certain** zone (§7.6), inside a sliding **10-turn** window
**[START]**. The tally is facility-wide, not per guard: three guards catching one
turn each still counts three. The window must fall back to **0** — ten turns with no
certain-zone contact — before another sighting can be counted, which is what makes
"3 sightings" three separate events rather than one long chase. A **glimpse** counts
nothing.

**"Tampered" consoles** are the **intel consoles** (`$`). The **comms console**
(`Ψ`) is deliberately **not** a trigger: it is the one answer this section gives the
player to the net, and charging alert for it would tax the counterplay. **Rung 0 is
safe** — tampering below rung 1 triggers nothing at all, which is the incentive to
stay undetected.

**No decay** — decided. The rung is permanent for the level: a step is a fact about
the run, and nothing un-knows that a guard stopped answering or that you were seen.
Do **not** add a timer; §7.4's decaying alert is the *per-guard* lead and must not be
conflated with the facility rung. A console that lowers the rung is a possible later
addition; it does not exist now.

Two things the ladder may never do:

- **Guards never accelerate** (§7.1 **[SETTLED]**). No rung may make any guard
  faster. This is the tempting wrong answer, so it carries an assertion.
- **The dwell is shortened, never removed** (§7.5). Almost every run sits at rung 1
  after first contact and it never comes down, so a dwell that could reach 0 would
  take the Takedown (§7.2) off the table for the rest of the level. The floor
  matters more than the ceiling.

Guard *presentation* is unchanged: a never-calm guard still reads as Calm (§7.4's
colour column). The ladder's legibility is the Level info tab and the tinted help
button (§11.4).

#### The comms console — the counterplay to the net

The net above is pressure the player is meant to be able to *answer*, not merely
endure. The answer is a **comms console**: the facility's radio terminal, one per
level, **bumped** like everything else (§4.3's one interaction verb). Bumping it
**kills all radio for the rest of the level.**

| Property | Value |
|---|---|
| Glyph | `Ψ` — its own, never the intel console's `$` (§11.3) |
| Interaction | **Bump** (§4.3); the usable line reads `comms: silence radio` (§11.4) |
| Cost | **1 turn**, plus the detour that got you there |
| Effect | Control stops pinging (no dispatch, no alert step from a missed ping), and **both** §7.7 cooperation call-ins stop firing |
| Guards already sent | **Finish the errand** — silencing stops the next wave, it never recalls a search already under way |
| Permanence | **One-way**, for the whole level; the console then reads as spent (Neutral, §11.2) |
| Placement | A non-start room, at least **16** cells (Manhattan) from the spawn **[START]**, reachable by a bump (§10.6), hidden until seen (§11.5a) |

Why it is shaped this way:

- **The cost is the route, not the switch.** One bump is cheap; getting to it is
  not. Placement distance is therefore the balance knob (**[START]** — the sim,
  §13.2, sweeps it), and the reason the console is not simply free: a console
  found in the first few turns would make every later takedown free, which is
  exactly the collapse §7.3 exists to prevent.
- **It is findable, not given.** Contents are fogged (§11.5a), so the console has
  to be *scouted*; the map never advertises it. And it is asserted reachable like
  an objective (§10.6) — **counterplay the player cannot reach is not
  counterplay**, so a seed that seals it away is a generation reject.
- **Errands are not recalled**, which keeps it counterplay rather than a panic
  button. It also follows §7.7's own rule that a call, once made, is never queued
  or retried — there is no channel to un-send one down either.
- **A silenced facility is lonelier, never blind.** Nothing here touches what a
  guard does with its *own* eyes: the one that loses you still searches, the one
  that finds a body still hunts it (§7.6/§7.2). Only the *calling of others*
  stops.

**[OPEN]** — whether a run **score** exists, and whether takedowns cost score.
See §15.

### 7.4 State machine

| State | Colour | Entry | Behaviour |
|---|---|---|---|
| **Calm** | yellow | Default | Patrols (§7.5) |
| **Alerted** | orange | Alert timer > 0, nothing seen this turn | Walks to its destination, then **searches** it (§7.6) |
| **Chasing** | red | Player detected this turn | Destination ← player's live cell; alert timer ← 30; step along shortest path |
| **Investigating** | red | A decoy seen, or a glimpse in the outer zone (§7.6) | As Chasing, but toward where it thinks you are, and reported at lower severity |
| **Responding** | orange | Dispatched by a missed radio ping (§7.3), or called in by another guard (§7.7) | Walks to the cell it was sent to, then **searches** it (§7.6) |

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
- **Dwell (§153).** On reaching a patrol target, a Calm guard **holds in place for
  3–7 turns** before picking the next — facing unchanged, no free re-aim (§5).
  This is what makes a Takedown (§7.2) approachable: a guard that walks every
  single turn can never be lined up on, so the pause is the *window to act*,
  paired with the rear blind spot (§6.1/§155) for the behind-the-back strike.
  **Calm only** — a Chasing/Investigating/Alerted/Responding guard never dwells
  and a detection cancels an in-progress dwell the same turn (a hunt never slows,
  the mirror of §7.1's "guards never accelerate"). The dwell length is drawn from
  the run seed (§12.4); the **[START]** knobs are dwell chance (**100%** — every
  arrival) and dwell length (**3–7** turns). **The facility alert cuts the length**
  (§7.3): from rung 1 the range is **1–3** turns, which is what "guards are never
  calm again" means mechanically. It is cut, never removed — the pause is the window
  a Takedown needs, and the rung never comes back down. Dwelling lowers patrol coverage on
  purpose (§7.6/§7.7) — a sim knob to watch, and more so now it is unconditional.

  **Why it is unconditional, and why the window grew.** The dwell was a 50% roll
  over 3–5 turns, and at that rate it was not the thing a player saw. Measured
  over twelve seeded runs, **92% of every stationary spell a patrolling guard took
  lasted one or two turns** — not a dwell at all, but the slow 90° turn and the
  two-rotation 180° about-face below. 42% of the two-turn stops were immediately
  followed by the guard walking back the way it came, so *reach the end, spin,
  come straight back* read as the patrol's actual rhythm, and the real pause —
  under 8% of stops — was lost inside it. A pause that fires half the time is not
  a rhythm a player can plan against; it is a thing that sometimes happens.
  Note that the **stop the player sees** runs a little longer than the dwell: a
  guard turning to leave spends one more turn rotating for a 90° heading, or two
  for a reversal, so a 3–7 dwell reads as 3–9 turns of held ground. The dwell is
  the part with the facing pinned, which is the part a Takedown needs.

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
| **Spotted** | Guard chases. | Oh no |
| **Flight** | You need ~3–4 turns of broken sight to disappear. Run, doors, corners. | Urgent, but *achievable* |
| **Lost** | Guard reaches last known position. **It calls it in** (§7.7) and others converge. **The search begins.** | The good part |
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
fine — it *shouldn't*. The frightening thing is what happens when you break
contact: it **calls it in** (§7.7), and someone who was never chasing you starts
combing the ground you vanished into — arriving from wherever they happened to be,
quite possibly **from ahead**. That converts a chase from tail-gating (boring,
unwinnable, what you played) into a spatial problem (readable, solvable, tense).

Danger should come from **being cornered and cut off**, never from being
out-jogged.

### 7.7 Cooperation — where the threat actually lives

The old version had none: no communication, no shared knowledge, no reaction to a
downed colleague. Each guard was an island. Given §7.6, this is not a nice-to-have
— **it is where the difficulty is supposed to come from.**

Cooperation has exactly one verb: **a call sends a fixed number of guards to
search a cell.** That is the whole vocabulary, and it is deliberately this small.
Three parts use it.

**1. The radio net (§7.3) — baseline.** Control pings; a downed guard cannot
answer; control dispatches a guard to the **takedown location** and it searches
there. This runs in every facility. It is the cost that keeps a permanent takedown
from being free, so it is never gated behind a modifier.

**2. A confirmed sighting lost calls one guard** — a level modifier (§12.6,
harder). A guard that had you inside the **certain** zone (§7.6, `CERTAIN_RANGE`
**[START] = 5**) and then loses sight calls **one** **[START]** guard, which walks
to the cell where it last had you and searches it. A **glimpse**-zone contact
(6–10, imprecise by design) calls nobody.

**3. A body discovery calls two guards** — a level modifier (§12.6, harder). A
guard that finds a body (§7.2) calls **two** **[START]**, which converge on the
body's cell and search it. Finding a body is the loudest event in the game, and
here "louder" means simply **how many come** — not a longer reach, a priority
system, or a second alert channel.

**The guard that makes the discovery searches on its own, modifiers or not.** A
guard that loses a chase, or finds a body, hunts that area regardless (§7.6,
§7.2). The modifiers add only the *calling of others* — so turning them off makes
a facility lonelier, never blind.

**What cooperation deliberately is not.** No radio **range** — a call reaches
whoever control sends. No reporting **delay**, and no window to interrupt a call
mid-sentence. No re-broadcast, and no rules about a report going stale. No shared
field of view, and no hive mind. And nothing searches "along a patrol route":
§7.5 patrols are emergent, so there is no path to walk. A call names a **cell**.

Two things fall out of that simplicity for free — worth knowing before anyone adds
the machinery back:

- **"Silence it before it reports" costs nothing to build.** The sighting call
  fires when a guard **loses** you, so taking the chaser down before it breaks
  contact means no call is ever made. The tactic this section always wanted exists
  without a report timer to interrupt. Its facility-scale twin is the **comms
  console** (§7.3): one bump and *no* call fires again for the rest of the level —
  both call-ins here, and the radio net's own dispatch, go with it. That console is
  the deliberate answer to the pressure this section applies, and it is why the
  guard counts below can be tuned upward without the net becoming something the
  player can only suffer.
- **The searched cell is stale by construction.** It is where you were when
  contact broke, never where you are. Responders converge on a place you have
  already left — which is exactly the readable spatial problem §7.6 fix 4 asks
  for: **the tail is not the threat, the net is.**

**Legibility (§9.3).** Sound is gone, so a call needs a **visual** tell: a near
line when it is made (§11.7), and the called guard's own **sensed dot** (§9)
peeling off toward the cell it was sent to. Never a ping the player has to hear.

> **Tuning warning.** §7.6 and §7.7 pull in opposite directions and must be tuned
> as a pair. Loosen the individual guard (escapable), tighten the collective (the
> net closes). Get this backwards — sticky guards *and* cooperation — and you
> rebuild the exact thing that wasn't fun, only worse.

The guard counts — **one** on a sighting, **two** on a body — are the difficulty
dial, and both are **[START]**. They are the first thing to sweep once the
headless sim (§13.2) can measure what they do to a run.

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

**Passives: the cost is the slot, not the time.** **[SETTLED]** (#264) Some
abilities are never activated — they are simply **in effect for as long as you
hold them**. They have no turn cost, no duration and no cooldown, so the time
economy above has nothing to charge them with, and §2.3 is explicit that an
ability with no cost is not a decision. The reconciliation: **a passive pays with
the loadout slot it occupies** (§8.3, capped at 3 — #266). You hold it *instead
of* something else, for the whole run, and that is the whole price.

Three consequences worth stating, because they are what keep the model honest:

- **Held is on.** There is no activation moment — nothing for a replay, a save,
  or a mid-run pickup to get out of step with. Picking a passive up switches it
  on; dropping it is the only off.
- **It reads as its own state** — not `Ready`, not `Active [N]`. The four clock
  states all mean "and then it ends", and a passive never does; reusing `Active`
  would make the number the bar shows a fiction, which is the one thing the
  timing trap above forbids. What that state draws was left to the ability-bar
  rework and is now settled there (§11.4/#287): **`(on)`**, where an activated
  ability carries its clock. Undecorated it read as one more thing you could
  press, and it is the one entry on the bar you never can.
- **It is still an `Effect` list** (§8.1). A passive is not a parallel system —
  the same effect vocabulary, applied continuously instead of for a window.

The balance watch this creates: a passive that meaningfully changes play for the
price of a slot is exactly right; one that is strictly better than any activated
ability is a smell, and that is what the power grades (#263) and the sim (§13.2)
are for.

**Uses per level: a bound on the facility, not a resource.** **[SETTLED]** (#302)
"No charges" above is the right instinct and this is the one thing it rules out
that the game needs: an effect too strong to hand out on a cooldown alone —
rewriting the level's geometry (#303), say. So an ability may also declare **how
many times this facility lets it be used at all**. The reconciliation with the
sentence it crosses: **there is nothing to spend, refill or manage.** The number
only goes down, it goes down for the whole facility, and no decision anywhere in a
run is about *getting more of it*. That is what makes it a bound rather than a
bar, and it is why the time economy is still the only thing an ability charges you
turn to turn.

The fence, which is what stops it drifting into the charge economy this section
rejects:

- **Set at level start from the ability's own row. No recharge**, no
  regeneration, no pickup or console that tops it up, no way to earn one back. A
  fresh facility is the only thing that gives one, and it gives it by being fresh.
- **Single digits.** A bound, not a bar — ten uses is an inventory. Enforced at
  compile time, so raising it past that is a change the build has an opinion about.
- **It composes with the time economy, it does not replace it.** An ability may
  carry a cooldown *and* a use budget; the turn cost is untouched either way, and
  §4.4 stands unchanged — activation costs the turn, and an activation refused for
  want of a use is the free mis-input it already was, costing neither turn nor use.
- **The player is told both numbers, and they are different numbers.** The bar
  shows what is *left* (`Bore(2)` — a count in parentheses, the shape a passive's
  `(on)` uses, because neither is a timer); the help panel's **Abilities** tab
  shows what a level *grants* (`3/level`, #343). Spent reads as unusable, never as ready and never as
  `/0/`. The timing trap above applies to both: each surface reports the number the
  player actually gets.

If something later wants uses that refresh, or uses shared between abilities, that
is a different design conversation — not a quiet extension of this field.

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
| **Drag** | 1 turn/step | while held | — | A body is non-solid: **walk over it and step off to take hold**; **you move at half speed while dragging**; **bump the held body to release** (free), or **stow it in a cupboard** (§10.3). |

**Salvaged tech** — found in the facility:

| Ability | Cost | Duration | Cooldown | Effect |
|---|---|---|---|---|
| **Camouflage** | 1 turn | 10 | 20 | Undetectable **while you don't move**. Moving reveals you for that turn. |
| **Decoy** | 1 turn | 20 | 30 | A fake intruder in the cell you face. Draws Investigating, not Chasing. Dies when anything steps on it. |
| **Dephase** | 1 turn | 3 | 30 | Fill → 0. Walk through walls, doors, guards. **Does not conceal you.** |
| **Autodoors** | 1 turn | 16 | 40 | While active, a door in your path **opens as you step into it** — no bump, no lost turn — and **shuts behind you** once you clear it, **manual and automatic alike** (an automatic door is shut early rather than left to its slow `delay`). A door closed behind breaks line of sight (§10.3) and forces a pursuer to reopen it (§10.4): a §7.6 flight tool, not invincibility (#241). |
| **Confusion** | 1 turn | — (instant) | 45 | **Fired once**, from the cell you press it in (#325). Every guard standing within the blast at that moment — `CONFUSION_RADIUS`, through walls like the guard sense (§9) — is **blinded and frozen** for `CONFUSION_DAZE_TURNS` (**6** **[START]**), a countdown each guard carries itself. A costed panic-buy of time, not a kill: a dazed chaser **pauses** (keeps its lead), it does not reset, and resumes cleanly when its own count runs out. **After the flash, distance stops mattering** — a guard you run away from stays dazed, and one that walks into the cells the blast covered was never in it and is untouched. That is what keeps the ability from being a no-guard-may-act field you carry: it has no window, nothing to toggle off, and no `[6]` on the bar. Capture-is-contact still holds (§4.5) — a dazed adjacent guard cannot step into you, but the daze is no shield to walk into a guard the blast never caught, and a frozen guard's cell stays solid. **The clamp [SETTLED]:** the reach fired is `min(CONFUSION_RADIUS, sense_range())`, read off the live guard sense, so the blast can never freeze what you cannot sense — inert on open floor (`min(6, 10)`), and shrunk to **5** inside a duct (§10.7), where degraded information is the crawlspace's whole cost. It can only ever shrink the blast, never widen it. A firing with nothing in reach is a **free no-op** with a near-line message (§4.4/§8.4) — fair, because the clamp means anything it could have caught you were already shown — and a real firing says how many it caught (§11.7). The long cooldown is what keeps it rare (#240). |
| **Pierce Wall** | 1 turn | — (instant) | — | **Bore straight through your one adjacent wall**, permanently. Usable only when **exactly one** of your four neighbours is a wall, so the target is unique by precondition and there is nothing to aim (§8.4) — which also rules the panic-bore out by construction, since a corridor and a corner both have two. The facility's outer shell is never a candidate (§1/§4.5); nothing else is off limits. It does **not** ask what is behind the wall — boring a two-cell-thick run (§10.1.5) opens a one-cell **pocket** off the room rather than a route, and that is a use of the tool, not a waste of it: a dead-end alcove out of the through-routes is somewhere to sit a sweep out. It conceals nothing (it is not a cupboard, §10.3), so whether it is shelter or a trap is the player's judgement — and three walls around you means you can dig a hole to hide in, never a tunnel. Its scarcity is a **per-level use budget of 3** (§8.2), not a clock. The hole is real terrain in the one spatial model (§10.5) — guards route through it and see through it, for the rest of the level. |
| **Lockdown** | 1 turn | 8 | 40 | While active, every door within `LOCKDOWN_RADIUS` of **where you fired it** is **shut and sealed** — a guard cannot work the handle, so its route goes the long way round (§7.6/§10.4). A **snapshot**, not a travelling bubble: a door does not unseal because you walked away from it, or the wall you raised behind you would dissolve exactly as you fled down it. **You** are never refused — it is your lock, so a sealed door bumps open for you exactly as any closed door does, which is what stops a lockdown ever boxing its owner in. That costs the turn and leaves the door *open*, so a lockdown fired across a route you still have to travel is a real mistake, and unmaking it is paid in the very turns the ability was bought to save. **Every seal is released when the window ends**, expiry or early toggle-off alike (§8.2) — the duration is the only clock, which is what keeps a temporary wall from ever becoming the permanent one §2.2/§7.2 forbid. A lockdown with **no door in reach** is refused for free (§4.4), like a wall bump. |
| **Vision** | — | **passive** | — | **Always on while held** (§8.2): your sight arc is the full **360°** and your range box grows from 15 to **20** (§5/§6.1). No activation, no turn, no cooldown — it costs the loadout slot and nothing else. **Vision only**: the guard sense (§9) is a separate, innate channel and is deliberately *not* widened with it, so a wait still buys something (§9.1). It erodes the §5 "can't see behind you" constraint on purpose — that is what makes it worth a permanent slot, and what the sim watches (#265). |

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
- **A duration that expires while you're inside something solid throws you clear and
  leaves you stunned.** The tech's **safety eject** drops you on a cell drawn at
  random from the nearest ones that can hold a solid body, and afterwards you cannot
  act at all for **one turn per cell you were thrown, plus one**: every key is
  swallowed, the turn is spent, and the guards keep moving (§4.4's turn cost applied
  *to* you rather than by you). **[START]** — the rate and the flat `+1` are the
  numbers to tune; the shape is not.
  **The stun is as long as the throw**, because that is what prices recklessness.
  Clipping the corner of a table strands you one cell from open floor and costs the
  smallest stun there is; burying yourself a ring deeper into a wall block costs more,
  because the eject had to reach further to find you anywhere to stand. A flat rate
  charged the near miss and the deep dive the same, which made the worst case as
  cheap as the safest. In practice the ability caps its own damage: Dephase runs
  three turns counting its activation, so a phase begun outside buys two steps in and
  the stun tops out at three turns — the arithmetic goes further, the ability does
  not.
  It is *any* solid, not just a wall — a shut door, a table, a cupboard, a console —
  which is why the near line names the **tech** rather than the terrain
  ("safety eject — stunned"): a message that said "the wall" would be untrue in most
  of the cases it covers. Naming the tech also gives the fiction for why this is
  survivable at all: the salvaged rig throws you out rather than letting you set
  inside the furniture.
  This is the third answer to a question that has now been asked twice. It was
  free, which made phasing consequence-free; then it was **lethal**, which made it
  the one death §2.2 forbids — the timer is on screen but the *lethal half* never
  is (`can_rematerialize` is invisible, and while phased you cannot bump, so you
  cannot even probe the cell you stand in), and §4.5 is **[SETTLED]** that a guard's
  touch is the only loss condition. So the cost stays and the death goes: turns spent
  helpless on a cell you did not choose, in a facility where contact captures, are a
  price a player can see coming and choose to pay. The **randomness is
  load-bearing** — a predictable eject would make phasing into a wall a reliable way
  *through* one, and you may well be dropped back on the side you came from.
  Deliberately **not** extended to the early toggle-off (§4.4): pressing the key
  inside a wall is still refused, because a free press that teleported you clear
  would be exactly the escape tool this is designed not to be (#304/#329).
- **Decoy draws Investigating, never Chasing** — a guard that can see *you*
  ignores it. Decoys work on guards that have lost you, not on guards that have
  you.
- **Which tech you start with is a level modifier** (`starting_abilities`, §12.6/#244),
  not a fixed roster. Quick play grants the innate set plus a **seeded** draw of three
  tech from a pool that defaults to the shipped, non-experimental set (eight tech now
  ship, so "three random" is a genuine draw of three of the seven — the pool has
  outgrown the grant and it finally bites, #241); a campaign accumulates its set
  instead (§2.2). A **passive** (§8.2/#264) is drawn from that pool like any other
  tech — it competes for the same slot, which is exactly what it pays with. The
  resolved loadout is one of the three pieces of the shareable level-seed token
  (§12.4/#245).
- **Three tech is a cap, not just this preset's number** **[SETTLED]**. Whatever
  hands a run its abilities — the quick-play draw, a campaign's accumulation — it
  holds at most three tech, so at most **four** abilities counting Run. It is what
  a passive's slot price is a fraction *of* (§8.2), and it is what lets the ability
  bar name every held ability on a single row (§11.4): the bar's width bound is
  checked against it at compile time, so raising the cap is a change the *build*
  has an opinion about (#287).
- **Drag has no grab button.** A body is non-solid (§7.2), so you cross it like
  floor; the drag begins the moment you step *off* a cell with a body on it and your
  hands are free, and the body follows into each cell you vacate. **Bump the trailing
  body to drop it** (free), or **bump an empty cupboard while dragging to stow the
  body inside and lock it** (§10.3) — the two ways to end a drag. Half speed holds
  throughout (one cell per two turns), and **Run never stacks with Drag** — picking a
  body up suppresses the sprint's extra step. Ducts refuse a dragging player: a body
  cannot follow into the walls (§10.7), so let it go first.

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
> position is now **directly readable** as a moving dot on the map. The radio
> clock is built and tells its story that way (§7.3); every §7.7 call must do the
> same — a near-line message and the responder's own motion, never a sound.

### 9.4 Sensing doors

The sense has a **second channel**, built the same way (§9.2) and for the same
reason. A door opening or shutting **away from you** — a guard routing through a
closed door and walking it open (§10.4), a Calm guard shutting one behind itself,
an automatic door timing out — is *evidence that someone passed* (§10.4). As a
transient near-line word ("the door opens") that evidence was easy to miss, cleared
on your next action (§11.7), and never said *where*. So it becomes a **positional,
on-grid cue**, exactly like the sensed guard.

- **It is the *same* sense channel — the `Sensed` category (§11.2), the same orange
  background** as a guard felt through a wall. A door change is "sensed through a
  wall" just as a guard is, so it reads in one colour, not a second one to learn.
  It is position only — *where* a door changed, never *who* passed or *which way
  they went* — the same restraint the sensed dot keeps.
- **It lights the whole door.** The cue paints the door's **entire footprint** —
  both hinges and every panel — not just the one panel the guard touched, so the
  eye reads "that doorway, over there" rather than hunting a single highlighted
  cell.
- **It carries farther than the guard sense.** A door change is a louder, coarser
  event than a guard's exact position, so it reaches a new **`DOOR_SENSE_RANGE`
  [START]** that sits **above** the guard sense — `DOOR_SENSE_RANGE = 15 >
  PLAYER_SENSE_RANGE = 10` (§9.1), pinned by a test. Doors are the facility breathing
  around you; you feel that from across a wing even when you could not pinpoint the
  guard that did it. A change beyond that range shows nothing — it is not made so
  large the whole facility pulses on every guard step (§2.3 in the other direction).
  **Wait does not widen it** (unlike the guard sense — a door change is already loud
  enough), and **a duct shrinks it** to `DUCT_SENSE_RANGE` with the rest of the
  crawlspace's degraded perception (§10.7).
- **It is a fading mark, not a standing dot.** A door change is a **discrete** event,
  not a live position, so the cue decays over a short **`DOOR_CUE_DECAY_TURNS`
  [START]** (currently 3) and is then gone — visible while the fact is fresh, not a
  single-frame flash and not a permanent stain.
- **Open and shut share one cue** — it is the same evidence, and both drive it.
  Where the cue coincides with the danger overlay, being seen outranks it (§11.5).
- **A door *you* operate** keeps its quiet near-line self-narration (§11.7) and
  lights no cue — you already know; the cue is for the doors you did *not* move.

> **[OPEN] — the sense channel wants to unify around this fading model.** The door
> cue fades over a few turns because a door change is discrete. The **guard** sense
> today is instead a hard on/off dot at the live range. The intended direction is
> for the guard sense to gain the *same* persistence — a sensed guard leaves a
> **fading trail** as it moves, the mark decaying a few turns behind its live cell —
> so the two halves of the sense read as one coherent "sensed, and fading" system.
> The door cue's decaying-marker machinery (`DOOR_CUE_DECAY_TURNS`, the per-turn
> decay pass) is the seed of that shared model; unifying them is its own ticket.

This shares the `Sensed` category with the guard sense, so the light-mode reskin
(§11.2) covers both at once.

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

   **Step 5a — Thicken walls.** Thicken roughly **half** of the interior walls to
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
   solid wall neighbours**, and **solid back diagonals** too, becomes a hideout —
   flush with the wall and **fully backed**, so it can be neither walked nor seen
   *through* to the far side (no cupboard traversal, no peephole). That geometry is
   what step 5a's two-thick walls and the pillar faces manufacture. *Fully backed
   means the recess sits in a 2×3 block of solid structure: cardinal sides alone leave
   the two back diagonals unchecked, and where the backing course is only locally thick
   one of them is floor of the space behind — which §6.1's always-seen touching ring
   (unqualified for the player, **[SETTLED]**) then hands over the moment they duck in.
   The rule is enforced where the site is chosen, never by trimming the ring: a
   placement that should not have been offered is not a reason to special-case a
   settled guarantee (#361).* Recessing is a **wall → hideout** rewrite, so unlike a
   floor-cell cupboard it cannot pinch a patrol route (a wall and a hideout both block
   pathing); the recessed cell joins the region it opens onto so "which room am I in"
   still answers for a hidden player. Place them **along the corridor network and near
   junctions**, not only in rooms — the flight path is where cover is needed (§7.6,
   §10.1a) — and **do not stop at the first failure.** Space them out (a single
   **[START]** knob) so the facility still reads as a building; the spacing also keeps
   a cupboard's own backing intact, since the two faces of a thickened wall sit one
   cell apart. *(The §10.1a corridor repair recesses **extra** cupboards mid-run —
   and carves alcoves into one-thick walls whose diagonals already back them —
   wherever a corridor sightline demands
   one; for those, spacing is a preference, not a gate: breaking the run outranks
   it, and the fully-backed geometry alone keeps every backing intact.)* *(The original rule — "a wall cell with exactly 3 wall neighbours and 1
   empty neighbour, one attempt per room" — harvested only the rare natural pockets,
   which is what left the old game with no board. Same recess geometry now, tightened to
   the full backing above,
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

**The rule constrains the generator, not the player.** **[SETTLED]** (#303) Pierce
Wall can punch a hole into a corridor's long wall from the room side and create
exactly the uncovered straight run this rule forbids — and that is correct, not a
loophole to close. The rule exists so a level is never *born* with an unsurvivable
sightline; a player who cuts one has made a choice, and the danger overlay (§11.5)
draws the new cone the moment a guard's line reaches down it, so the consequence
reads as their own doing rather than as a bug. The assertion below is a property of
*generation*, and it is checked there.

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
| Guards | **4** **[START]** |
| Intel | **3** **[START]** |
| Exit rule | **A level modifier** (`intel_to_exit`, §4.5/§12.6/#244): quick play = **all three**, the sim = **at least one**, campaign = **none** |
| Starting abilities | **A level modifier** (`starting_abilities`, §8.3/#244): quick play grants the innate set **plus three random tech**, seeded (§12.4); the sim grants the **innate set only**; campaign accumulates instead (§2.2) |

**The sim plays bare, and that is the point.** The headless baseline (§13.2) holds
no salvaged tech — Run and the innate verbs, nothing else — because **a level must
be winnable with no tech**. Tech is what makes a run *better*, never what makes it
*possible*; measuring the bot with a full loadout hides a facility that is only
survivable because something was handed out. The guard count is tuned against the
bare number, so every tech draw on top is upside.

**Where 4 came from.** The `--guards` sweep (`sim --bot`, 300 seeds each, bare
loadout) traced the whole curve:

| Guards | 1 | 2 | 3 | **4** | 5 | 6 | 7 |
|---|---|---|---|---|---|---|---|
| Bare win rate | 80% | 61% | 48% | **37%** | 29% | 21% | 16% |
| Captures | 58 | 116 | 155 | **189** | 213 | 234 | 251 |
| Timeouts | 2 | 2 | 2 | **0** | 1 | 3 | 2 |

Roughly linear, about 8–10 points of win rate per guard — no cliff, so the number
is a taste call rather than a threshold. **4** is the forgiving-but-real end: a
bare run wins better than one in three, and it is the only row where *every* seed
resolved to a win or a capture rather than stalling. Read it against §13.4 — the
bot has perfect information and no fear, but plays greedily and badly, so a human
sits well above its number, and this is a floor, not a forecast. Nudge it back up
once guard cooperation (§7.7) and the radio net (§7.3) add pressure the bot
currently never feels.

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
| **Duct entry** | `=` | Yes (**player: Bump**) | Yes | Yes |
| **Partial cover (table)** | `π` | Yes | **No** | Yes |
| **Console** | `$` | Yes | No | No |
| **Comms console** | `Ψ` | Yes | No | No |
| **Exit** | `E` | Yes | No | No |
| **Player** | `@` | Yes | No | No |
| **Guard** | `g` | Yes | No | No |
| **Body** | `z` | No | No | No |
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
> crouch behind it — and behind its whole **run**: the contiguous piece of
> furniture that table belongs to (the §10.1a bench; benches are never a lone
> cell). While crouched you still see everything (your own sight is
> unchanged), but you are **concealed from any viewer across the furniture**:
> each straight **arm** of the run defines a **line**, and a viewer on the far
> side of one of those lines cannot see you — however far past that arm's ends
> they stand. **A viewer who has come round to *your* side of the bench sees
> you**, which is what keeps the crouch directional. A viewer standing *on* the
> line — looking along the bench rather than across it — is on neither side, so
> it is settled by the older test that still runs beside this one: a table of
> the run on the sight line between you also conceals, corner grazes included,
> out to the exact 45° diagonal. That is why looking straight down a bench is
> still blocked by its own tables, and why a lone table (which the generator
> never places, §10.1a) still grants exactly the quarter-plane it always did.
> Concealment is directional, per-guard, and per *the run you ducked behind* —
> not every table you happen to stand beside. A **bent** run — an L, where two
> stamped benches touch — has no single axis, so **each arm contributes its own
> half-plane and they union**: the whole L is one piece of cover, exactly as the
> flood fill already treats it. That is what keeps a bench weaker than a cupboard
> (omnidirectional, contact-safe) — and a crouched player **can still be
> captured by contact** (§4.5); unseen is not safe. The crouch spends the turn;
> **waiting holds it** (hold still, watch the cone sweep past, §7.6); and so
> does the **crouch-walk**: a plain step that lands still touching the run —
> orthogonally or on the diagonal at a corner, so you can round the end of the
> bench without standing — keeps the pose and moves you at full speed
> **[START]** (the constraint is hugging the furniture; if playtest shows
> bench-hugging dominance, the levers are Drag's half speed or Camouflage's
> reveal-on-move). Any other spent action — an interaction, a step that leaves
> the run's side — stands you up; a free action changes nothing, posture
> included (§4.4) — re-bumping any table of the run you are already behind is
> a free no-op.
> *(Waiting beside a table used to crouch automatically; that coupling is gone —
> wait is pure (§5, §8.3's 360° look), and the crouch shows its direction in
> the usable line (§11.4) like every other bump. And concealment has been rewritten
> twice, each time because the shape of the protected zone was wrong rather than
> because the geometry was computed wrongly. It began as the **quarter-plane**
> behind the single bumped table — which let a guard look down a bench and see you
> through its other tables, undercutting the exact cover §10.1a places. That was
> replaced by a **per-ray** test across the whole run, faithful and exact but too
> tight in the other direction: a short bench subtends a narrow wedge, so a guard
> only a little off the run's axis had a clear line, and — the deciding
> complaint — **the player cannot compute that wedge at a glance**, which made the
> crouch a turn spent on protection you could not predict and usually did not get.
> Since partial cover is the counterplay §10.1a places in every corridor, a
> coin-flip crouch means that counterplay is not there. The **half-plane** above
> replaced it: it costs the crouch some precision at the ends of a bench and buys
> back the one property a counterplay has to have, which is being readable before
> you spend the turn. It is deliberately the more generous of the two — if
> bench-hugging turns out to dominate, the levers are the ones already named here
> (the turn it costs, contact-vulnerability, the crouch-walk's requirement to keep
> hugging), **not** re-narrowing the geometry the player has to read.)* Legibility
> rides the same
> conventions as the cupboard: the covering run recolours to **Owned** while
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
> which the turn loop, the renderer, and vision (§6) complete together.

> **The one exception: a guard that saw you climb in can flush you out. [SETTLED]**
> (§15 Q5, resolving the "saw you go in" half.) A cupboard refuses contact *unless*
> a guard **witnessed** the dive — its cone covered the cupboard on the turn you
> climbed in, **and** it was already **alerted** (any non-Calm state: chasing,
> investigating, searching, responding). Such a guard re-engages the cupboard itself
> as a live lead (it Chases the alcove), walks to the mouth, and **captures the
> hidden player** — diving into a cupboard in plain sight of a hunter is not a free
> escape. The fact is stored **per guard** and it is the *only* way a cupboard is
> ever entered: a patrol that never saw you go in still routes around the occupied
> cupboard forever (§10.3), and a **Calm** guard whose cone merely grazes the
> cupboard as you enter is *not* alerted, so it does not check — you can still hide
> from a routine sweep. The witness is dropped when the guard's lead runs cold and
> it stands down (§7.1 — you waited it out) or when you leave that cell, so hiding
> still works the moment you **break sight first, then dive** (§7.6): out of every
> alerted cone on the entry turn, there is no witness. This keeps the §2.2 fairness
> promise — a cupboard capture is only ever the result of hiding *while watched*, a
> decision you could read straight off the danger overlay (the cone was on the
> cupboard, §11.5).
> **The second way in: a guard that finds a body nearby checks the cupboards it
> searches. [SETTLED]** (§15 Q5, the "found a body nearby" half.) Finding a body is the
> loudest event in the game (§7.2): the finder is thrown into a §7.6 search of the
> **area around the corpse** — the `SEARCH_RADIUS` disc it already sweeps — and, because
> a body is loud evidence the intruder is close, that search now **checks the occupied
> cupboards inside it**. An occupied hideout within the searched disc is flushed exactly
> as a witnessed dive is: the finder re-engages the cupboard, walks to the mouth, and
> captures — the same per-guard capture gate (`witnessed_hideout`), a second way to earn
> entry, not a new machinery. Only a **body** search checks: a search that began by
> *losing a chase* never opens a cupboard, so breaking sight and diving still works
> (§7.6). The readable rule (§2.2): the corpse is the signal. You cannot watch a cone
> fall on the cupboard here, but you *can* see the body you left — so **hiding within a
> guard's reach of a body you dropped is the traceable mistake**, and a body far from
> your cupboard never reaches it. A **stowed** body is *gone* (§7.2 — no cone finds it),
> so it never starts a search and never checks anything. Ducts are untouched: a duct is
> an escape a pursuer cannot follow (§10.7), so it stays contact-safe unconditionally.

> **A cupboard is also where you hide a body (§7.2), and doing so locks it.** Drag a
> body to a cupboard and **bump the empty cupboard to stow it inside**: the body
> slides in and is *gone* — no cone will ever find it — and the cupboard is now
> **locked**. A locked cupboard is no longer a hideout: it holds a body, so you
> cannot climb in, and bumping it is an inert no-op. It shows the body's **`z`** in
> the Owned colour (not the empty `}`), so a glance tells you which cupboards you
> have spent this way — and that status is **remembered** (§11.5a): once seen, a
> locked cupboard stays a remembered `z` out of view, like a seen console, rather
> than reverting to the empty `}`. This is the one place a body
> vanishes completely; everywhere else it stays visible evidence on the §7.3 clock.

### 10.4 Doors

A door is a span of **3–6 cells**: a **hinge at each end** (permanently solid,
opaque — they're the frame) and **1–4 panels** between them that open and close as
one unit.

- **Bump a panel to open. Bump a hinge to close.** The hinge is the handle, and
  it's why hinges stay solid forever. Since #148 a hinge on a *closed* door opens it
  too — cracking the door from beside the frame, with a peek along the door line.
  - **One exception, and only one (#320): the frame of the door you just opened does
    not shut it.** For exactly the **next action**, a bump on that same hinge is a
    *dead bump* — offered to the #57 lateral shift, so a player walking into a doorway
    slightly off-line rounds the frame onto the open panel instead of spending a
    second turn undoing their own open. If the slide declines it is the free §4.4
    no-op, never a close. The mark is spent by whatever the player does next, free or
    spent, so "bump a hinge to close" is never more than one action away — and a door
    a *guard* opened, or one that was already open, closes on the first bump as
    always. The window's length is the tuning lever, not the rule.
- **Anyone can operate any door.** No keys, no locks. **[START]** — keys are an
  obvious future axis, and one the fiction supports.
  - **One bounded exception so far (#242): a lock that expires.** The **Lockdown**
    tech (§8.3) seals the doors around you for its window — a sealed door refuses a
    *guard's* walk-in open, so guard routes treat it as solid and go the long way,
    while the player bumps it open as always. It is a lock on the **handle**, not a
    hold on the door: a sealed door standing open is as passable as any other. The
    lock lives on the door itself, one representation for every lock source, so the
    key-gated doors of the locked-doors modifier extend it rather than inventing a
    second — and the ability's duration is the only clock any seal has, which is
    what keeps this side of the **[START]** baseline and clear of §2.2/§7.2's
    soft-lock class.
- **A door cannot close if anything occupies a panel cell.** Doors never crush
  anyone.
- **Closed panels do not block pathfinding** — deliberately. Guards route through
  closed doors and open them by walking into them (#146). **This opens the facility
  up over a level, and every opened door is a new sightline** — but no longer a
  *permanent* one, now that doors close again (below).
- **Closing — two mechanisms** (§10.4 auto-close **[START]**). The old version had
  none — every door stayed open forever, so connectivity only ever increased and the
  level decayed into an open plan. Both restore the level's structure over time and
  turn an open door into evidence that someone passed:
  - **Manual (hinged) doors** are closed by hand — bump a hinge — and a **Calm** guard
    that passes through one *sometimes* closes it behind itself (#146, a seeded
    **[START]** chance, deliberately not always: §7.6 — a guard that always tidied up
    would erase the "traffic opens the facility up" pressure).
  - **Automatic doors** (#147) are a **[START]** fraction of doorways generated
    *frameless* — no hinges, the whole span is panels — so there is no handle to shut
    them by hand. They close *themselves* a few turns (**[START] ~5**) after the
    doorway is last vacated; an actor standing in the throat holds them open (never a
    crush). The delay is a stealth window: a guard passing through leaves the door open
    just long enough to slip after them.
- **You sense a door change away from you** (§9.4): a door opening or shutting that
  you did not cause lights a fading on-grid cue over its **whole footprint**, in the
  same orange **`Sensed`** channel as a guard felt through a wall, at its own longer
  `DOOR_SENSE_RANGE`, so "someone passed through there" stays legible around a corner
  rather than living only in a transient near-line word.

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
| **A path exists: start → every objective → the comms console → exit** | **Assert it. See below.** |
| **The comms console is a real detour** | ≥ 16 cells from the spawn, non-start room **[START]** (§7.3) |
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
distinct usables** (a door, a table, a cupboard, either console, the exit; a
multi-cell door counts once) is still *legible* — `→ door: open` and `↑ table:
crouch` are two aimed actions, not one ambiguous prompt — but it reads cleanest
at one. So every stamping stage **avoids crowding where it cheaply can**:
cupboard sites that would double up are skipped (sites are plentiful), and
console (intel and comms) and exit candidates prefer a clean cell, falling back
rather than failing the draw.

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

### 10.7 Ducts — player-only crawlspace shortcuts

**[START]** A **duct** is a crawlspace that spans the facility and only the
player can use: a shortcut between two far-apart parts, paid for not in time but in
**degraded information**. It extends §10 without touching any existing contract — a
guard experiences a duct's **entries** as ordinary wall and never perceives the crawl
route at all; the interior cells it may pass over keep their own terrain, so nothing a
guard sees, paths on, or looks through changes.

**Shape.** A duct is a path of cells with an **entry at each end, drawn `=`**
(`DuctEntry`, §10.3/§11.3). Each entry is **recessed like a cupboard** (§10.1.6):
exactly one floor **mouth** and solid backing on the other three sides, so a duct is
entered, exited and peeked from that one side. The **interior** cells between the
entries **route over the building** — the shortest path across plain **wall or floor**,
so a duct **spans across rooms** to join two far-apart regions. It crosses *inert*
geometry only: never a cupboard, door, console or table, since crawling over an
interactable would collide with the terrain it overlies. Each interior cell keeps
whatever terrain it already had (a path over floor stays floor to everyone but the
crawler); the only record that those cells are also a crawl route is the duct list on
the layout — nothing on the grid tells.

**The entry is wall-like to guards.** A `DuctEntry` blocks movement, sight and
pathing exactly as a wall does. So a guard never sees *through* an entry, never
routes *through* a duct, and never *enters* one — and converting a wall into an
entry changes neither reachability (§10.6) nor a sightline (§10.1a): the crawl route
is the player's alone. This is what lets the duct pass run last, after the §10.6
gate's inputs are fixed, and assert only its own geometry.

**Interaction (§4.3, the one verb).**

- **Enter** — from the mouth, **bump** the entry to climb in (a *decision*, like the
  cupboard; the turn is spent). A **dragging** player is refused: a body cannot
  follow into the walls — let it go first.
- **Crawl** — inside, a step moves one cell along the path (one turn, §4.4). You are
  **confined** to the path: the only way off it is stepping from an entry onto its
  mouth. A cell that merely touches floor mid-duct is never an exit.
- **Exit** — crawl to the far entry and step out its mouth: an ordinary move onto the
  floor.

**Concealment.** Inside a duct the player is concealed from every guard (no cone
detects a crawler) and **contact-safe**. The crawler is in the crawlspace, not on the
floor the guards walk: where a duct passes over room or corridor floor, a guard **walks
straight over the crawler's cell** — it neither blocks the patrol nor is blocked by it,
and cannot capture the concealed crawler (a duct changes nothing guard-facing). So a
duct is an escape a pursuer cannot follow — the cupboard's payoff, mobile.

**The cost is information (§2.3), and it is load-bearing.** Inside, normal vision is
off — you perceive only your **memory** of the building (§11.5a) and a **shortened
guard sense** (`DUCT_SENSE_RANGE`, below), with **one** live window: on an **entry**
cell the hideout-mouth **auto-peek** (§6.1) casts out the mouth, so you read the room
before you climb out. **Mid-duct there is no live vision at all**, and **Wait does not
widen the sense** the way it does on open floor (§9.1) — a crawlspace is exactly where
you should *not* be able to take stock of the whole area. The counterplay to the
degraded view is the deliberate pause on the entry cell; crawling straight out
without peeking is the risk you chose. *(The presentation of all this — the memory
view, the live peek window, the sensed dots through the reduced radius — is the
companion render ticket #134; §10.7 owns the model.)*

**Fog (§11.5a).** An **entry** (`=`) is **contents**: a mouth recessed into a wall
run, so it reads as plain schematic fabric until you have seen it, and then it is
remembered. *(This drops the earlier "visible from turn one like a door" rule —
another stated change, and the same one doors themselves took: the plans carry the
building's bones, and a crawlspace mouth is something you find. A duct you scouted
is worth more than one you didn't, exactly as with a cupboard.)* The **interior
path** is **not on the base map at all** — it carries no tell, so it can cross a room's floor without giving the
shortcut away — and lives in **its own layer, shown only while you are crawling it**.
It is **not remembered**: climb out and the path is hidden again. *(This drops the
earlier "the interior reads as plain wall, remembered once crawled" rule — a stated
change, not drift: now that the path can overlie floor, "reads as plain wall" is no
longer even true, and a remembered overlay would paint a tell on the room floor a duct
crosses. The `=` you plan around is the entry alone.)*

**Generation.** Place a small number of ducts, each connecting two regions **far
apart on the region graph** (a duct that shortcuts nothing is noise), routed as the
shortest cell path **over the building** — across plain wall *and* floor only (never an
interactable cell), forbidding also the two mouths so each entry's recessed backing and
its single climb-out survive — deterministic from the seed (§12.4). A candidate is kept only when crawling it saves
at least `DUCT_MIN_PAYOFF` steps over walking between its mouths. The pass asserts the
per-entry one-mouth geometry; the §10.6/§10.1a guarantees hold untouched, since only
the two entries are restamped (a wall→entry swap that is wall-like both ways) and
every interior cell keeps its terrain — reachability and sightlines on the finished
grid are byte-identical with the duct in place or reverted.

`[START]` knobs (all pinned by tests): **`DUCT_RUNS_PER_LEVEL`** = 2 (a spice, not the
main route — base solvability never depends on a duct); duct length in
**`DUCT_MIN_CELLS`** = 4 .. **`DUCT_MAX_CELLS`** = 22 cells; **`DUCT_MIN_PAYOFF`** = 8
steps saved; and the reduced in-duct sense **`DUCT_SENSE_RANGE`** = 5 (half the §9.1
floor range, and Wait does not extend it). If playtest shows ducts strictly dominant,
**add cost** (a longer minimum, a smaller sense, a slower crawl) — don't remove the
feature; the cost is information and it is meant to be tuned, not deleted.

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
| **Sensed** | Orange (background) | **Sensed through a wall** (§9) — a guard, or a door that just changed away from you (§9.4); an eye-catching highlight, position only |
| **Interest** | Purple | Goals and rewards |
| **System** | Tan | Doors, hideouts — neutral furniture |
| **Effect** | Cyan (background only) | An **ability effect of your own making** (§8.3) — where it acted, and what it holds; advisory, so it yields to Danger, and its wash yields to Sensed too |

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
so the two never collide; the door-change cue (§9.4) shares this same Sensed
background, so a sensed guard and a sensed door change read as one channel. The old
§9.3 cyan "Noise" slot — a heard sound's source — was freed when sound went; the
**Effect** layer now claims that cyan, and it is the one hue on the board that is
never a threat level.)*

**An ability effect always colourises the background** (#338) **[SETTLED]**. The glyph
keeps its own meaning — a guard stays on the yellow → orange → red ladder, a thing of
yours stays Owned — and the effect is the wash underneath it. One channel for every
effect the game grows; what varies is not *where it is said* but **where the mark lands**
and **how long it lives**:

- **Place.** Over an explicit **cell set**, fixed when the mark is lit — Confusion's
  §6.1 box, the cell Pierce Wall opened, the pair a safety eject threw you between — or
  over the **thing** in a cell, which carries the mark wherever it goes and for exactly
  as long as it exists (a guard a blast froze, a decoy still standing, the player while
  concealment holds).
- **Lifetime.** **Momentary** where the effect *is* a moment (a bore, a blast's reach:
  the firing frame and nothing after it, see §11.5), or **standing** where the effect is
  a state (a guard still held, a live decoy, concealment in force). A momentary mark may
  be given a longer stated life when the moment's *consequence* outlasts it: the safety
  eject's pair is lit on exactly the frames the player cannot act from, so the cue
  neither expires while its reader is still held down nor lingers into the frame they
  choose a real move from.

The two places also sit differently in the §11.5 precedence, because they make different
claims. A **cell** mark is a wash and the weakest background there is: a door cue, a
sensed guard and a danger cone all paint over it. A mark on a **thing** is a *recolour of
a cue that thing already draws* rather than a competing claim — on a guard felt through a
wall, cyan replaces the Sensed orange to say "exactly here, **and** it cannot move" — so
it outranks Sensed and still yields to Danger. Net: **Danger > a mark on a thing > Sensed
> the wash.** A mark on a thing only ever recolours something the player is already
shown, so it can never reveal what the fog is hiding — it inherits that thing's own
visibility rule rather than adding one, which for a decoy (§11.5a's second exception)
means it is drawn out of the FOV exactly as the `@` under it is.

If an effect background ever reads badly under the glyph standing on it, **shift the
Effect colour** — the channel is not negotiable, the hue is.

Base palette: a 16-colour, colour-blind-safe qualitative set, each usable as
foreground and as a background variant that recedes toward the page. **There are two
of them** — a dark theme and a light one (#189), toggled from the help panel until v2
grows an options screen — which is exactly the reskin this section's rule was written
to make cheap: the core gained a `Theme` flag and not one colour, and every §11.5
guarantee is asserted over both tables. The concrete rows, the constraints the tests
hold them to, and why each exception exists are in
[`docs/render-reference.md`](render-reference.md) §4.

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
| `=` | Duct entry | System — a player-only crawlspace mouth (§10.7); wall-like to guards, geometry-visible from turn one |
| `π` | Partial cover (table) | System |
| `π` | Partial cover, concealing you | **Owned** — the same convention as the occupied cupboard: while you are crouched behind it, the covering run recolours to Owned, every table of it, so the blue `@`-`π` pair reads as one hidden unit as long as the furniture (§10.3) |
| `$` | Intel | Interest |
| `E` | Exit | Interest |
| `≈` | Building fabric not yet seen — the §11.5a schematic | Neutral |
| `~` | Floor space not yet seen — the §11.5a schematic | Ground |

**Overlapping glyphs need a priority order.** The old version was
last-writer-wins, so a guard in a doorway rendered arbitrarily. Define the order.

> **The complete table, with the reasoning behind each mark and each colour, is
> [`docs/render-reference.md`](render-reference.md).** This section and §11.2 own
> the *rules*; the reference records what they resolve to, in one place, so a
> question like *"what does `≈` mean?"* has one answer rather than four. The values
> themselves live in code (`Terrain::glyph`, the shell's one palette table) and the
> in-game glyph legend (the help panel's **Help** tab) derives from those same
> sources, so neither can drift from the board.

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
┌────────────────────────────────────────────────────────────┐
│ Radio: a guard has gone silent                        [?]  │ ← near line
│ → door: open                                               │ ← usable line
├─ map: the whole story, fixed ──────────────────────────────┤
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
│                    Run/11/   Camo[9]   Decoy     Sight(on) │ ← ability bar
└────────────────────────────────────────────────────────────┘
```

**Which way up: action low, status high.** The chrome is ordered for **thumb
reach** on a held device. The one row you *tap* — the ability bar — sits
**bottom-right**, where a hand already rests; the rows you only *read* — near
line, usable line, and the `[?]` help toggle — sit at the **top**, clear of both
the thumb and the board. On a very wide desktop viewport the
bottom-right corner is a long way from anything, which is fine: the mobile grip
is the case this serves.

- **Map**: the full story, statically fitted to the screen (scaled, aspect
  preserved). No camera.
- **Near line** — *what is around you*: the highest-priority live message
  (§11.7) — guard caution, a radio event (§7.3), an alert change, intel
  collected — drawn as a **solid band in the message's category colour**. Threat
  reads as a colour flash across the top of the screen, legible without reading
  the words; that's a nice piece of design — keep it. When no message is live,
  the line falls back to quiet **ambient status** (alert level, an active
  ability's remaining turns) instead of sitting empty. Its right-hand corner
  carries the two view toggles: the live-message counter and the `[?]` help
  button.
- **Usable line** — *what you can act on*: the bump affordances adjacent to the
  player right now, each **with an arrow giving the bump's direction** (`→ door:
  open`, `↑ console: take intel`, `← table: crouch`, `↓ cupboard: hide`). Not a
  message — a **pure derived function of state**, recomputed every frame, no
  plumbing. When nothing is adjacent it falls back to the **innate-verb floor**
  below rather than sitting empty (#323). The arrow makes each bump an aimed
  "press this way, get that", so even the rare cell beside two usables stays
  unambiguous — one row lists each with its own direction. The generator
  *prefers* one usable per floor cell (§10.6, best-effort) to keep the common
  case to a single line, but does not guarantee it.

**Neither status row is ever blank.** The near line falls back to ambient status;
the usable line falls back to **how to move and how to wait** (#323), in the input
vocabulary the player is actually using — `swipe: move  tap: wait` on touch,
`↑↓←→: move  w: wait` on keys (§11.6's own table — `w`, not `5`, because the wait
digit is the *numpad*'s and a floor hint has no room to say so). One rule, two rows: permanent
screen real estate is never given away for nothing.

That row is where the innate verbs have to live. **Wait is the least discoverable
thing in the game and one of the most important** — the only 360° look (§9.1), the
way a crouch is held (§10.3), the way a cone is let past (§7.6) — and it
deliberately has no ability-bar entry, because the bar is the ability *economy*
(§8.3). Without this, the two verbs every run is built out of appear nowhere.

The fence that keeps it a hint rather than a control legend:

- **A floor, never a competitor.** The instant anything is adjacent, the
  affordances take the whole row back. No fade-out after N turns and no
  first-run-only flag either: the row is still a pure derived function of state,
  and it costs nothing either way.
- **Exactly the two verbs with no other home.** Takedown and Drag already appear
  here as real affordances (§7.2/§8.3) and the tech abilities have the bar; the
  full control set is the help panel's job.
- **Read-only, like the rows around it.** Nothing here is tappable — a hint that
  could be pressed would be a second, undiscoverable control surface at the top of
  the screen (§11.6's touch rule).
- **Owned** (§11.2, *you and the things you made*) — the same blue the ability
  bar's ready entries use, so the two surfaces answering *what can I do right now*
  answer in one colour. Ground was tried first and lost on screen: its meaning is
  **absence**, drawn to recede so everything else pops against it, which is exactly
  the wrong instruction for a row whose whole job is to be read.
- **The modality is the shell's only say.** It answers *is this a touch session?*
  — seeded from `pointer: coarse` at boot, then corrected to whichever modality was
  **last actually used**, so a laptop with a touchscreen and a tablet with a
  keyboard each get the hint that matches what the player's hands are doing. The
  words and the layout stay in the core, inside the golden tests (§11.2/§12.1).
- **Both wordings are budget-checked at compile time**, like the ability bar's
  worst case (#287): a reworded hint that would clip on the 40-wide board fails the
  build, not the frame.

**No ability column.** The old fixed 14-column list spent a seventh of the
screen on information consulted once a minute. Ability state (ready / active
`[3]` / cooling `/2/` / passive / unusable) must stay *discoverable*, and §15 Q9
asked where it should live. Three experiments answered it. Showing the list
*while waiting* buried the 360° guard-sense the wait exists to reveal (§9.1). A
left-aligned header strip put the tap target furthest from the thumb. A compact
bottom-right strip of bare hotkeys, with a deploy button unfolding the named
panel over the board, put it in the right corner but made every name a second
tap away.

**[SETTLED]** — a **one-row ability bar, flush to the bottom-right**, always on,
carrying **every held ability by name** with its `[3]` / `/2/` / `(on)` notation
tucked against it and its state in colour. No key letters on the bar, no
deploy button, no panel: the whole set is simply always readable, and the board
is never covered. A tap on any entry activates exactly what its key would. The
bar is the **source** of the §11.6 ability keys, not a projection of them: `1`–`4`
fire its first through fourth entries as drawn (#359), so the row's order is what
the keyboard names and a tap and a digit resolve through the one function. The help
panel's **Abilities** tab is where a player reads the pairing off, each entry given
the bar name it fires and the digit that fires it (`1 / Camo` → *Camouflage*), with
what the ability actually does (#343). The **Help** tab —
called *Legend* until the abilities left it — keeps only the **standing** controls:
move, wait, messages, help. It listed the whole eight-ability catalogue when a run
holds at most four, and a reference card that changes with the loadout is not a
reference card (#296).

**Fixed slots: the names never move.** Each ability owns a **10-cell slot** — 9
of entry, 1 of air — and its entry is drawn **left-aligned inside it**, whatever
state it is in. A cooldown appearing, or ticking from `/10/` to `/9/`, changes
nothing but its own cells. This matters more than it sounds: a bar whose words
slide about as numbers come and go is a bar you have to *read* every time you
look, and the whole case for it being always-on is that you learn its shape and
then only **glance**. An ability's column is a fact about the run, and since
#359 it *is* its key (§11.6) — position is muscle memory too. The slots are laid end to end and
the block is flush **right**, so a shorter loadout still keeps the bar under the
thumb (#267).

**Why names fit now, and the budget that makes them fit.** A run holds **at most
four** abilities — innate Run plus the three-tech grant (§8.3/§10.2) — so the
compression the old strip paid for bought nothing worth its cost. Four named
slots across a 40-wide board is still tight, and the arithmetic is exact:

| | cells |
|---|---|
| Longest state notation (`/45/` — the catalog's biggest cooldown, plus delimiters; a passive's `(on)` is deliberately no wider) | 4 |
| Longest **bar name** (`Decoy` / `Phase` / `Doors` / `Sight`) | 5 |
| Widest entry, so the slot's content width | **9** |
| Plus one cell of air → **one slot** | **10** |
| Four slots | **40** |

That is the whole board width, with nothing spare — which is why each ability
carries a short **bar name** (`Run`, `Camo`, `Decoy`, `Phase`, `Doors`, `Daze`,
`Sight`) distinct from the full §8.3 name the help panel, the messages and the
level-seed token use, and why the notation is tucked hard against it. **The
budget is checked at compile time.** Every input is derived — the held cap from
the innate set plus the tech grant, the notation width from the catalog's own
durations and cooldowns — so renaming an ability, pushing a cooldown past 99, or
granting a fourth tech fails the *build*, not the frame (#287).

The slot is also the **tap target**, all nine cells of it: a short name is no
harder to hit than a long one, and the target does not move when the ability's
state does (§11.6's touch rule). Touch is forgiven **one row of slack above and
below the bar** (#386) — the drawn slot is unchanged, and the slack rows are the
ones the router already answered with silence; see §11.6.

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

**The effect layer is advisory and never outranks red** (§8.3/#308/#325/#338). An
ability effect of the player's own making marks the board in cyan, always as a
background (§11.2), over a fixed cell set or over the thing in a cell.

A **momentary** mark — Confusion's blast box, the cell Pierce Wall opened — shows for
**`EFFECT_FLASH_TURNS`** turns after the effect acts: **one**, the acting frame
**[START]**. Enough to answer *what just happened, and where* at the moment the player
asks it, without leaving a 13×13 field of background over the board while the danger
overlay is the thing that matters. What carries a state for every turn after that is a
**standing** mark instead — a guard still held, the doorways a Lockdown has sealed
(§8.3/#242), a live decoy, concealment in force — which costs no ink beyond the place it
rides.

**A mark earns its place by saying what the bar cannot** (§11.4/#341). The ability bar
is a projection of state and already reports *the window is open*; a mark that lit
whenever an entry read `(on)` would carry nothing. So the player's own cell is marked for
**Camouflage** and for no other effect on them: its concealment holds only on the turns
they do not move, so the mark and the bar entry can **disagree** — `Camo[7]` while you
walk across a lit corridor in plain sight — and that disagreement *is* the mechanic.
Marking a Run, an Autodoors or a running Dephase would just restate the bar. A later
ability joins this rule when its effect is likewise **conditional**, not because it is an
ability.

**One firing may wear both**, and the two answer different questions. Confusion washes
the box it went off in and then rides the guards it froze; Lockdown washes the box it
sealed from and then holds the doorways themselves — *this far*, once, and *these ones*,
throughout. Neither substitutes for the other, which is why a mark is keyed by its
lifetime as well as its place: an ability may hold a momentary and a standing mark at
the same time, over the same kind of place, without one quietly replacing the other.

The precedence is fixed — **Danger > a mark on a thing > Sensed > the wash** — so an
advisory layer can never masquerade as the detection set, nor hide it; the wash yields
to the sense channel, while a mark on a thing merely refines the cue that thing already
draws. Every mark carries the geometry the mechanic resolved against, by value, so the
picture cannot disagree with the rule — and it stays where it happened rather than
following the player, because that is what the effect did. A **refusal** marks nothing:
a press that changed nothing is a message (§11.7), not an effect.

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
| **Geometry** — the building's load-bearing fabric, the openings in it, and the floor space between | **Always visible, from turn one.** Never fogged. Drawn as the **schematic** until explored (below). |
| **Contents** — intel, hideouts, ducts, furniture, equipment, lore, and a door's *pose* | **Hidden until seen.** Once seen, remembered. |
| **Live state** — guards, bodies, door open/closed, danger cones | **Only what you can see right now.** Never remembered. **One exception: a guard's *position* is also known through walls within the guard-sense range (§9)** — but only its position, never its cone, and never remembered once out of range. |
| **What you placed** — your live decoy (§8.3) | **Always drawn, wherever it is.** In the FOV or out of it, for as long as it exists. |
| **The exit** — the tunnel you dug and came in by (§4.5) | **Always drawn as itself**, from turn one, never schematic. Yours. |

> **The schematic (#307).** Geometry you have never had eyes on draws as the
> building's *plans*, not as it has been seen: the **fabric** (`≈`) — wall runs and
> the recesses and openings cut into them — and the **floor space** (`~`) between
> it. Walking somewhere resolves it into the real thing, permanently. It is a
> **shape** distinction, not a darker shade: a fourth rung on the §11.5 dimming
> ladder has nowhere to go below Ground's already-quiet dim, and geometry too dark
> to read is fog by another name, which this section settles against. See
> [`docs/render-reference.md`](render-reference.md) §2.3.
>
> **The line is load-bearing structure.** `≈` is what holds the building up — a
> wall run, a door's frame, and the recesses cut back into a run. `~` is everything
> that is not: a room's floor, the furniture and equipment standing in it, and a
> **doorway**, which bears no load and so draws as the **gap in the wall line** a
> plan would show. An unexplored wing reads `≈≈≈~≈≈≈`, so the ways between its
> rooms are still plannable and the *"you can plan your escape route before you're
> spotted"* promise above survives intact.
>
> **A stated change, not drift.** §10.7 promised a duct entry visible from turn one
> "like a door", and furniture was geometry too, on the grounds that being surprised
> by a table mid-flight is as bad as being surprised by a wall. Both now have to be
> **found** — a duct mouth is a recess backed by structure, and a table is something
> put in a room, not part of it. Cupboards were already hidden on exactly this
> reasoning (*"the flight paths you scouted are worth more than the ones you
> didn't"*), so ducts join them rather than sitting on the other side of an
> inconsistent line. Room shapes, wall runs and the openings between them still read
> from turn one, so you are still never lost and never mapping.
>
> **The cost is meant to be payable.** §12.6's `full_layout_known` modifier hands
> the whole layout over as an *easier*-direction modifier — so under the directed
> difficulty draw it is bought with pressure taken on elsewhere, never given away.

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

> **A decoy you placed is yours to see (§8.3).** The fourth row is not a hole in
> the live-state rule, it is a different layer: a decoy is neither the facility's
> live state nor a content to be discovered, but **the player's own placed
> object** — the same category of knowledge as their own cell or the body in
> their hands. The whole point of a fake is to *walk away from it* and let a
> guard investigate the wrong cell, so a marker you can only see by standing next
> to it is a marker the ability cannot use, and route-planning around your own
> bait — the "you plan confidently" half of this section — is exactly what its
> disappearance takes away.
>
> **It leaks nothing new.** A decoy dies the moment anything steps on it, so an
> always-drawn one might seem to announce "a guard just walked here" through a
> wall. The game already announces it, twice, on the turn it happens: the death
> ends the ability into its full cooldown, so the §11.4 bar flips from *active*
> to *cooling* wherever you are, and it prints the §11.7 message *"the decoy is
> trampled"* with no visibility filter. Drawing the `@` only puts that same fact
> where the player is already looking, instead of making them infer a location
> from a cooldown pip.
>
> Out of the FOV it draws **remembered**, not live: the marker persists at full
> Owned colour while the three-state discipline above keeps telling the truth
> about what is actually being seen. It stays the lowest entity layer (§11.3), so
> it never paints over a live entity and never hides the §11.5 danger overlay on
> its cell. This is the decoy alone — **a body you dropped is not covered**: it is
> the facility's live state and the §7.3 clock's evidence, and being unsure
> whether it has been found yet is load-bearing.

> **A duct's interior path is its own layer (§10.7).** The crawl path between a
> duct's two entries is *neither* geometry (it carries no tell on the base map — it
> can cross a room's floor without giving the shortcut away) *nor* remembered
> contents (it is never absorbed into tile memory). It is shown **only while the
> player is crawling that duct**, and hidden again the moment they climb out — a
> fourth, private layer over the three above. Only the two entries (`=`) are
> geometry, visible from turn one. See §10.7.

### 11.6 Input

| Key | Action |
|---|---|
| Arrows / numpad `4` `6` `8` `2` | Move |
| `w` / numpad `5` / `.` | Wait — the top row's `5` is *not* a wait key |
| `1` `2` `3` `4` (top row) | Fire ability bar slots 1–4 |
| The bar's marked letters | Fire the same four, by mnemonic |
| `Enter` / `Space` | Confirm |
| `Escape` | Cancel / menu |
| `m` / `?` / `n` | Messages, help, colour theme — view toggles, never a turn (§11.2/§11.4) |

**Digits bind by physical key, not by character.** The top row's `Digit1`–`Digit4`
and the numpad's `Numpad2` `4` `6` `8` `5` are resolved from `KeyboardEvent.code`,
so the binding is the key's *position*. An AZERTY top row is `& é " '` and a
character binding would want Shift to fire an ability in the turn things go wrong.
It also settles which digits are which: the **numpad** moves and the **top row**
fires abilities, a split the table above could not state while it said only `4`
`6` `8` `2`.

**No character binding may name a digit** (#369). The split above only holds if
nothing downstream can undo it, and a character can't: both blocks produce `"2"`,
so a table matching on `"2"` is answering a press it cannot identify — and whichever
table is asked first wins. So the digits appear *only* in the code tables, the
numpad folds onto the keys it duplicates (`ArrowDown`, `w`) rather than onto `8` `2`
`4` `6` `5`, and a **position outranks every character table** when a press is
resolved. Without all three, a top-row `2` steps south instead of firing slot 2 —
which is worse than nothing happening, because it spends the turn *and* moves you
(§2.2), and it hides: slots 1 and 3 work, because no movement key happened to claim
those characters.

**Ability keys are the bar's slots** (#359). `1`–`4` fire the first through fourth
entries **of the bar as drawn** (§11.4) — the *drawn* row, which is flush right, so
`1` is the leftmost entry on screen and never a gap. Four keys, forever: a run holds
at most four abilities (§8.3) while the catalogue keeps growing (salvaged tech —
§14 v3), so identity-keyed letters meant every new ability needing a free letter it
could keep for good, with the best mnemonics gone first and the twelfth ability
getting whatever was left. A digit past the run's held count does nothing: no turn,
no state change.

**What that trades away, and why it is safe.** A key is no longer a fact about an
*ability*: `c` was Camouflage in every run ever played, and `1` is not. That is the
cost, and it is real. What makes it payable is that the slots are **fixed for the
whole run** and drawn on screen at all times (§11.4), so a digit is never ambiguous
where it is pressed — and `Run` is innate and always first (§8.3), so the
most-pressed key in the game keeps its cross-run constancy anyway. This is *not* the
old regression coming back: that one let a key change **because another ability's
name changed**, silently, between one run and the next, with nothing on screen to
say so. A bar slot is visible, stable within the run, and the same thing your thumb
taps.

**A second way in: the mnemonic letter** (#360). Beside the digit, each entry
claims a **letter**: *its own initial, unless another entry in the same run took it
first* — the whole rule, and the whole rule a player has to know. And the bar **draws
that one letter in the ink colour**, lifted out of the name around it, in the entry it fires
(§11.4). `1` and `c` both fire `Camo`. Nothing is drawn behind it and nothing is
added beside it: the bar stays the quiet strip §11.4 settled on, and the mark costs
no width. **An entry you cannot use is not marked** — an exhausted or unusable entry
recedes whole (§11.2's Ground), because an ink letter says *press this* and the eye
should not be pulled to the one thing on the bar that is not on offer. Its letter
still resolves, and still refuses for free (§4.4). The digit is the primary key: stable by position, and there whether or
not a letter could be claimed. The letter is the one you reach for when you know
*what* you want rather than *where* it sits. An entry that could claim nothing keeps
its digit alone; nothing is silently reassigned. Letters resolve on the **character**
(`key`), not the code — you press the key labelled with the letter you can see, which
is the mirror of why a position binds by position.

**Only a held ability may push a mnemonic off its initial** (#368). A mnemonic still
may not shadow a movement or system key — a mis-key ends a run — but that guarantee
has to be one the rule above never *notices*, because a skip whose cause is off screen
is unreadable: Lockdown showed `o` with nothing in the run holding `l`, and no way to
find out why. So the tables give way instead of the letters. Movement is the arrows
and the numpad, and the vi keys `h` `j` `k` `l` are gone with it — a comfort binding
is not worth a quarter of the alphabet the bar can never use, and it was costing a
shipping ability its own name. What is left reserved — `w` `.` `m` `n` `?` — starts no
bar name in the catalogue, and a test says so, so the day it starts to is a failed
build rather than a letter a player cannot account for.

**Why this is not the derivation this section designed out.** The paragraph below
warns about exactly this shape of rule, and three things separate them. The claim set
is the **run's four**, not the catalogue's twelve-and-growing, so a letter can only be
taken by something you are also holding. It is **not silent** — the letter is drawn,
marked, on the entry it fires, so the key is a fact you read off the bar like its
state and its name; the old scheme's whole failure was invisibility. And the **digit
is always underneath**, so a player who never learns a letter loses nothing. What
remains true, and should not be glossed: **the same ability can carry different
letters in different runs** — Dephase is `p` alone and something else beside Pierce
Wall. It is a fact about the loadout, like a bar slot.

**An ability key is a toggle.** The key switches the ability on and, pressed
again while it is **active**, switches it off — the free action §4.4 grants, which
otherwise has no key at all. One key with two meanings is safe here because the
meaning is on screen before it is pressed: the bar draws `Run[3]` while active and
`Run` while ready (§11.4). A **passive** (§8.2) is not a toggle — holding it is the
whole of its state, so its key stays the free no-op it always was. The choice is
made from live state in **one place** for both input paths, and the slot→ability
resolution is one place too, so a tap on the bar and its digit can never diverge
(§11.4).

**Letters live on in the replay notation.** The identity→letter map (`+r` Run, `-c`
Camouflage) is what a replay script spells an ability with (§12.4), and *there*
identity-keyed is exactly right: a stored script has to name the same ability in
every run, so a letter moving would silently re-point old replays. It is pinned
letter by letter for that reason. It is **not** the mnemonic above and not a keyboard
binding: one is a fact about the ability, the other a fact about the run.

**Touch is a real target and was never finished.** The manifest pinned landscape
and installed standalone, but the options dialog could not be closed by touch and
the pause menu could not be opened by touch — together making it unreachable *and*
inescapable. Either build touch properly or don't ship the manifest. **[OPEN]**

**The touch model.** A **swipe** fires along the drag's dominant axis and *keeps*
firing while the finger stays down, the direction re-read live from the drag; a
**press held in place** repeats; a **quick tap** is a single input, resolved at the
lift. Lifting the finger stops everything instantly.

**A gesture is a binding, like a key** (#336). What a drag *did* is surface-neutral
— a **swipe** in one of the four cardinals, or a **press** (held or tapped: how long
it lasts changes *when* it fires, never what it means). What that means is a table
per surface, and those tables live in the core beside the key ones, so a screen's
two input paths are read — and pinned — side by side:

| Gesture | Board | Main menu | Help panel |
|---|---|---|---|
| Swipe ↑ / ↓ | Step north / south | Previous / next entry | — |
| Swipe ← / → | Step west / east | — (a vertical list) | Previous / next tab |
| Press | Wait | **nothing** | nothing |

One pump drives whichever surface is up, in the keyboard's own precedence — menu,
then help, then the board — so the thresholds, the repeat cadence and the
lift-stops-everything guarantee are written once and inherited, and the next screen
that wants touch costs a binding table rather than a pump.

**A press is deliberately unbound on both modal screens.** Resolving it to
*activate* would let a stray tap on empty menu space start a run by accident — the
class of bug #306 closed on the board, and worse here, because starting a run is not
undoable (§2.1). An entry fires by pressing *the entry*, on the arm-on-press /
fire-on-lift path below, and by nothing else. The consequence worth stating: a
swipe must **begin where the controls decline**, since a press that lands on an
entry arms that entry and starts no gesture.

The two board-only rules below — the dead band and the no-auto-walk-into-danger
gate — do not follow the pump onto a modal screen: both ask questions about a board,
and a full-screen menu has none.

**Waiting is a tap on the board, well clear of the bars** (#306). A tap produces
Wait only on a **map** row that no overlay owns *and* at least a **dead band** away
from the chrome's inner edges — the two read-only status rows above, the ability bar
below, and the lower edge of the deployed message list while it is up. Inside the
band, on the chrome, and off the canvas, a tap that hits no control does **nothing**:
no turn, no state change. The point is that the boundary is *forgiving*, not merely
correct — a near-miss on the flush-right ability block (§11.4) must cost nothing,
because in a permadeath run with no undo a silently spent turn is unrecoverable
(§2.1/§2.2). **The fix is never to move a drawn target, and never to grow one into
space that answers**: §11.4's nine-cell slot, its fixed position and the air between
slots are settled, and a miss stays free.

**The ability bar is forgiven one row of slack above and below it** (#386). A press
on the map row directly above the drawn bar, or within one row's height below the
frame's bottom edge, resolves to the slot in that column exactly as a press on the
bar does — armed on the press, fired on the lift like every other control. This
amends the rule above, and the distinction it stands on is that **nothing drawn
changes**: the slot is still nine cells at a fixed position, the block is still flush
right, and what grows is the invisible hit region, into two rows that are silent by
construction — the row above the bar is always inside the dead band, which is floored
at one full map row, and below the last row there is only letterbox, which owns
nothing. Forgiveness may turn silence into a hit and may never take a live board tap
away from the board, which is why it is applied only where the router was about to
answer with nothing. It never changes *which* ability fires: the slack column asks the
same hit-test as the bar column beneath it. The cost is honest — a tap one row above
the bar was free and can now spend a turn (§8.2) — and it is the one thing to watch in
play: nobody can be *aiming* a Wait at a dead-band row, but it is the row a thumb
reaching for the board's bottom edge brushes. **[START]** — if it misfires by thumb,
drop the upper row and keep the lower one rather than widening further. The near
line's `[?]` and message counter have the same one-row problem and deliberately do not
have this slack yet.

Two rules keep the band from taking anything away. **Swipes are exempt** — a
directional drag is unambiguous, so it may start anywhere, band included; only the
ambiguous zero-displacement gesture is gated. And the keyboard is untouched: `w` /
numpad `5` waits without touching the board at all, so the band never leaves a
player unable to wait. The band is one number for the touch feel, scaled from the swipe
threshold and floored at one full map row so it never shrinks below a cell at a
small fit. **[START]** — the cost is that a player near the bottom of the map can no
longer wait by tapping right beside themselves.

**Chrome controls resolve on the lift**, not the press: a press arms the control and
the lift over that same control fires it, so a mis-press can be slid off and
abandoned. That is the gesture pump's own fairness contract — *a turn must never
burn on a gesture the player didn't finish* (§2.2/§4.5) — applied to buttons, and it
puts both surfaces' resolution at the same moment so they behave alike.

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
- **Objective messaging derives from the gate, never from a fixed intel count**
  (§4.5/#310). Whether the exit is open is `exit_ready()`, and how much it still
  wants is `intel_needed_to_exit()` — which is *not* the tally of consoles still out
  (under `AtLeastOne` three can be out while one is needed). A message layer that is
  pure over its event must be *handed* that fact by the event; no take message may
  announce an exit that would refuse, and no refusal may misstate the requirement.
  **[SETTLED]**

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
- **A replay is `(seed, [inputs])`.** Nothing else — until level modifiers and a
  seeded ability loadout made the *config* part of the reproducible unit too, so the
  identity widened to `(seed, modifiers, abilities, [inputs])`, all but the inputs
  carried in one compact **level-seed token** (§12.6/#245/#333). The principle is
  unchanged: a small, serialisable token reproduces the run exactly.

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

### 12.6 Level modifiers

A **level modifier** is a named toggle or bounded knob that shifts a facility's
*baseline* — its difficulty or its rules — *before the level begins*. Each one
flips a rule an existing system already owns rather than adding a parallel one:
*"guards always search hideouts"* forces the §7.6 search to check occupied
cupboards unconditionally (harder); *"always show vision cones"* paints the §11.5
danger overlay in full (easier); *"full layout known"* draws the building's real
architecture where the §11.5a schematic would otherwise stand, so doorways, duct
mouths and furniture are all on the map from turn one (easier); the two
**cooperation call-ins** (§7.7) decide whether a lost sighting and a found body
summon anyone (harder). This is the
**mechanism** difficulty and mode rules flow through — the shared seam #210 (alert
scaling), #244 (quick play), and the v3 catalogue (#232–#236) all plug into
instead of each inventing its own knobs.

**One resolved value, read by many (§12.3).** All active modifiers are fields on a
single `LevelModifiers` value — plain, heterogeneous data (a toggle is a `bool`, a
knob a small enum or clamped integer). It is resolved **once at facility start**
and every system branches on *that* value — never a global bool queried in ten
places. Adding a modifier is adding a field, and the compiler then enumerates
every read site that must handle it. Each field carries a documented **direction**
(harder / easier); §2.3's anti-facade rule means every shipped modifier needs a
**directional assertion** — from the same seed and inputs, the harder one yields
at least as much pressure as baseline, the easier one reveals at least as much —
so a flag that changes nothing observable cannot pass for shipped.

**The source → modifier → config flow.** The mechanism is shared; the *sources*
that switch modifiers on are separate and stack on top of it. Three, kept
deliberately distinct:

- **Choice** (exogenous) — the player's chosen or seeded baseline. A **mode** is a
  named preset = a bundle of modifiers over the base rules (#244).
- **Alert** (endogenous) — the campaign alert (#210): a loud raid raises the
  alert, and higher alert switches on harder modifiers for later facilities. This
  is where *"levels adapt to the strategy you lean on"* (§2) lives.
- **Flavour** (per-node) — a facility's own character (#207).

They compose into the *same* resolved `LevelModifiers` (`ModifierSources::resolve`):
a toggle is active if **any** source requests it; a knob composes *harder-ward*.
Adding a source is a new field and a line in `resolve`, never a new difficulty
path — #210 owns the alert→modifier *mapping* and its own fairness (decay, floor,
§2.2); this seam owns only the merge and the application.

**Determinism (§12.4 [SETTLED]).** The resolved set is part of the reproducible
config: same seed + **same modifiers** + same inputs → identical run. It is plain
`Copy` data threaded through the boot alongside the seed, so — with the ability
**loadout** (§8.3/#244), the third piece — a run's identity is now
`(seed, modifiers, abilities, inputs)`. These three pieces compose to one
**level-seed token** (`LevelSeed`, #245/#333): **eighteen lowercase letters**, e.g.
`prbjdokbxcqgjnrnco`, extending the #110/#197 carrier rather than inventing a second
scheme. Fixed width, so a wrong length is rejected before anything is parsed;
all-alphabetic, so there is no `0`/`O` to misread; and one form, so the panel that
displays a run and the link that shares it cannot disagree about what it is. A token
that does not decode falls back to a fresh run, never a bricked page. One boot path
(`start_level`) turns a `LevelSeed` into a running state for the web shell, the replay
viewer, and the headless sim alike, and the one token format is what saves (§12.5) and
the replay Artifact build (#197 slice C) share so they cannot diverge.

> **The format is specified in [`docs/level-seed-token.md`](level-seed-token.md)** —
> field layout, the permanent-slot discipline, the integrity argument, and the sizing
> trade-offs. It is a spec rather than design notes: read it before changing anything
> that a token's meaning depends on. What follows here is only what the *design* turns
> on.

**A number is not a token** (#333, superseding #328). A bare `?seed=8371` named *this
build's quick-play preset applied to 8371* — not a run — so every shared link silently
re-resolved whenever the preset moved. It did move: when the Vision passive joined the
tech pool (#286) the seeded draw re-ran over a changed pool, and every link shared
before it began booting a different loadout with nothing saying so. The bare form is
gone as an **input** too, which is the cost worth stating: "try seed 8371" no longer
works, and pre-#333 links stop decoding. They were already booting the wrong run;
failing loudly is the better of the two. Numeric seeds remain a *programmatic*
concept — `LevelSeed::sim(n)` and §13.2's sweeps never touch the string.

**The format is sized for the roster it does not have yet.** Abilities and modifiers
are carried as combination indexes over **256 permanent slots**, not over the entries
that exist today, and the caps (§8.3's three tech, five modifiers) are what the token's
length tracks rather than the size of the catalogue. So the roster can grow to a
hundred entries without a single shared link breaking — adding one fills the next slot
and changes no radix. That is the property the previous carrier lacked, and #286 is
what its absence cost. It buys a discipline in exchange: **slot numbers are permanent**,
a retired entry leaves a tombstone, and nothing may ever be renumbered.

**[START]** on the sizing: eighteen characters, a 17-bit seed (131,072 facilities), and
the ~1-in-3,000 rejection that the leftover space provides. The two trade one-for-one —
every bit spent on the seed is a bit taken from integrity — and a character is worth
26× of whichever you want more of.

**Debug modifiers are not level modifiers.** A separate `DebugModifiers` value
carries playtest-only switches over **what the player perceives** — today one: *"see
the whole level"*, which makes the player's §6 field of view the entire facility, so
a build can be watched rather than played blind. It is stated as *sight* and applied
in the sight phase, not as a drawing rule, so everything downstream follows without a
special case: the §11.5a fog lifts into the ordinary live picture, every guard reads
as seen, and the §11.5 danger overlay paints every cone. It touches nothing else —
guards look with their own cones and walk the same beats, so the run plays identically
(seeing everything is not being everywhere). It is **never encoded into a level-seed
string** and has no URL form, so no shared level can arrive with the fog lifted; no
generation seam sees it; and it is baked into a build and nowhere else (the
artifact-build skill's `assemble.py --debug reveal`). The line is worth keeping sharp:
**a level modifier changes the game, a debug modifier changes only what you get to see
of it** — anything that bends a rule is a level modifier and belongs in the token with
the rest of the run's identity.

**Constraints.** The *"full layout known"* modifier reveals the **architecture and
nothing else**: contents stay fogged (§11.5a), so it never shortcuts the scouting
that finds the objectives, and the knowledge state on the render seam keeps telling
the truth — a cell handed over by the modifier still reports itself *unexplored*,
because the player has not been there. Being an **easier**-direction modifier is
what makes it *paid for* rather than given: under the directed difficulty draw its
budget has to be found by taking a harder rule elsewhere.

The *"always show vision cones"* modifier may only ever **widen**
the §11.5 overlay — it reveals unseen guards' cones on top of the seen ones, and
must never narrow or hide the red detection set (§11.5 is **[SETTLED]**). Modifiers
resolved before generation (guard count #232, safe zones #235, locked doors #236)
read the same value at the generation seam; runtime modifiers (the two shipped,
the intel gate #244, the two §7.7 cooperation call-ins) read it off the running
state. Same value, two horizons.

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

> **How the bot itself decides is [`docs/bot-behaviour.md`](bot-behaviour.md)** —
> the channels it is allowed to read, the plan it names each turn, the per-ability
> **cue** seam that decides which key it presses, and the per-ability threshold that
> separates *"weak ability"* from *"shy cue"*. This section and §13.4 own what the
> numbers are **for**; that doc records how they are produced, so a reading of the
> ability histogram can be checked against the policy that made it. The values
> themselves live in code (`crates/sim`), and the operator-facing half — flags and
> output schema — is `crates/sim/README.md`.

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
  radio (§7.3) as baseline, and **cooperation (§7.7)** — the radio net plus the
  two call-in modifiers (a lost sighting summons one guard, a found body two)
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
- Difficulty that scales with the alert level, driven through the level-modifier
  seam (§12.6) rather than a private knob set. **The whole point of the alert
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
5. **Do guards check hideouts?** *(Resolved — see §10.3.)* If never, hideouts are
   permanent safe rooms and patrol coverage has holes by design. If always, they're
   death traps. The settled answer: **only when alerted, and only if they saw you go
   in _or found a body nearby_.** *Saw you go in:* a guard that was alerted and whose
   cone covered the cupboard on the entry turn flushes you out. *Found a body nearby:*
   a guard thrown into a §7.6 search by finding a corpse (§7.2) checks the occupied
   cupboards within the disc it sweeps (`SEARCH_RADIUS`), flushing one the same way —
   the readable signal being the body you left, not a cone you could watch (§2.2). A
   search that began by *losing a chase* checks nothing, and a **stowed** body (never
   found) never triggers a check. Every other guard still routes around the occupied
   cupboard forever. This interacts with §7.6 as hoped — a cupboard entered *in* a
   hunter's cone, or beside a body a guard finds, is now a trap, so the hiding game
   rewards breaking sight *first* and not hiding beside your own handiwork.
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
9. **Where does ability state live on screen?** *(Resolved — see §11.4.)* The
   fixed column is gone, and so are the three experiments that followed it:
   show-on-wait buried the wait's own 360° sense (§9.1), a left-aligned header
   put the tap target furthest from the thumb, and a compact bottom-right strip
   of bare hotkeys needed a deploy button and a panel to say what anything was.
   The settled answer: an always-on **bottom-right bar naming every held
   ability** in a fixed 10-cell slot, with its `[3]` / `/2/` / `(on)` notation and
   its state colour, and nothing to deploy. What unlocked it was capping the held set at four (§8.3) —
   the names fit, so the compression was pure cost. Both constraints hold: every
   state stays discoverable on the bar, and the keys are legible off it — the bar
   draws no key itself, but its slots *are* the §11.6 digits (#359), and the help
   panel's Abilities tab pairs each with the bar name it fires (#287/#343).
