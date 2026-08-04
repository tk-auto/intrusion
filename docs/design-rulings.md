# Intrusion — Design rulings

The long "why" behind decisions in [`docs/design.md`](design.md).

The design doc says **how the game is supposed to be**. It states each rule, and —
where a decision was controversial or hard to shape — one short sentence of why.
This document holds the rest: the argument, the alternatives that were tried, the
reworks, and the sim evidence. A ruling lands here when the decision cost a long
discussion, a measurement or a rewrite; a decision that was obvious the first time
needs no appendix.

**Nothing here is a rule.** Where this document and the design doc disagree, the
design doc wins and this one is out of date.

## How this file is organised

It isn't, and that is deliberate. Appendices are **appended in the order they were
written**, never sorted, never renumbered. The heading is the index: grep for
`Appendix 18`, or for a word in its title.

- A design-doc section links to a ruling by number — *(appendix 12)*.
- A new ruling takes the **next free number** and goes at the **end**.
- **Numbers are permanent.** A ruling that is superseded is rewritten in place, or
  marked superseded and left where it is. Nothing is renumbered and nothing is
  deleted, because the links in the design doc are by number.
- Each appendix names the design-doc sections it belongs to, so the trail runs both
  ways.

---

## Appendix 1 — Why the previous version was not fun

*(§2.3, and the origin of nearly everything else in this file.)*

The previous version was not fun. It is tempting to blame the design. The evidence
says otherwise: **every system that would have created pressure was inert, and the
one ability that resolved pressure was free.**

| System | Intended | What actually happened |
|---|---|---|
| Neutralise ability | A costed tactical option | Unlimited range, no cooldown, **and it did not consume a turn**. You could neutralise every guard in sight, for free, without ending your turn. |
| Sound | Noise draws guards | **Guards were deaf.** A full propagation model existed and was never given a single sound source. |
| Alert | Detection makes things harder | **Never written to, never read.** |
| Run | An escape option | 2 cells/turn against guards hard-capped at 1 → an *unconditional* escape. Being seen was never fatal. |
| Guards | Patrol, cooperate, search | No communication. No reaction to a downed colleague. No search at the last known position — arrive, find nothing, wander off. |
| Fog of war | — | None at all. The whole floor plan was legible from turn one. |

**The lesson is not "the design was wrong". It is that the design was never actually
running.** The version that got playtested was this design with all of its tension
removed and a free win button added.

Two consequences carry forward into the design doc itself, which is why they are
stated there rather than only here: **cost is the load-bearing property of every
ability**, and **this class of failure is invisible to a human playtester and obvious
to a bot** — a human plays 5 levels and vaguely feels the game is flat; a bot playing
500 reports "the neutralise ability is used 94% of turns and win rate is 99%" on the
first run.

Smaller faults from the same audit, each recorded with its own system:

- No targeting system at all — every ability was self-targeted or auto-targeted at
  the nearest valid thing, because building a targeting UI kept getting deferred.
  That is the *direct* cause of the free unlimited-range neutralise: auto-target-
  nearest-visible was the path of least resistance (§8.4).
- Watched-but-unseen cells rendered dark gray on dark gray, so the danger overlay's
  red downgraded to grey and **the safest-looking cells on the map were the watched
  ones you could not see into** (§11.5).
- FOV was invisible on open floor: floor is a space, a space has no foreground, so
  the dimming that encodes the FOV boundary was undetectable across open ground
  (§11.5).
- The palette pushed every colour through a gamma curve that compressed everything
  into 0.1–0.9, so there was no true black and no true white and the whole image sat
  in a washed, low-contrast band. Six of the sixteen colours were never used at all
  (§11.2).
- Overlapping glyphs were last-writer-wins, so a guard in a doorway rendered
  arbitrarily (§11.3).
- There was never a legend. Nothing ever explained what `$`, `E`, `}` or `z` meant,
  and the game-over screen did not distinguish victory from defeat (§14 v2).

---

## Appendix 2 — "No killing" is fiction, not a mechanic

*(§2.1, §7.2.)*

The original pillar read *"no permanent guard incapacitate (no killing)"*. That
bundles two constraints which do not have to travel together:

- **The fiction constraint**: the protagonist doesn't kill. *Keep this.* It costs
  nothing and it is the character.
- **The mechanical constraint**: threats are never permanently removed. *Drop this.*

The mechanical half also directly contradicts the *"thorough exploration is
rewarded"* pillar. Explore-thoroughly plus threats-rearm is a treadmill: you are
asked to own the space and denied the means. Games that do make guards wake up
(Invisible Inc, most obviously) pair it with an escalating alarm that shoves you out
the door — you are never meant to own the space. Intrusion wants both. Pick one, and
this design picks *ownership*.

The cost that replaces the timer is the body and the radio clock it runs (§7.2,
§7.3), which is what keeps permanence from being free.

---

## Appendix 3 — Permadeath: the cost, and the safety valve not built

*(§2.2.)*

**A 2–3 hour permadeath run means a capture at hour 2.5 costs 2.5 hours.** That is a
real cost and it is deliberate; it's what makes the last facility frightening. It is
also what puts enormous weight on the fairness promise in §2.2: permadeath is a
promise that the game is fair, and unfair permadeath is just a bad game.

**The old version was not permadeath in any sense** — it offered unlimited "play the
same level again" from a run-start snapshot, so a run could be retried forever. Half
the point of writing the pillar down this precisely is that the previous version's
behaviour did not resemble it at all.

**The prison level** — capture drops you into a cell with a chance to break out and
rejoin the run, instead of ending it outright — would soften the 2.5-hour cliff
without adding meta-progression, and it is thematically perfect. It is parked in the
§14 backlog rather than built, because **it is a safety valve for a pressure that has
to be shown to exist first.** Build the cliff, feel it, then decide whether it wants
relieving.

> **Development tension, stated plainly.** Permadeath and "iterate fast to find the
> fun" pull hard against each other: you cannot playtest hour 3 of a run fifty times.
> Expect a debug/practice mode that starts anywhere with anything. It is *not* the
> real game, must never be reachable by accident, and must never be confused with a
> roguelite. (The shipped shape of this is §12.6's `DebugModifiers`.)

---

## Appendix 4 — The body is non-solid

*(§7.2.)*

An earlier rule made a body a solid obstacle (fill 1.0). It read well — the body as a
thing in the way — but it manufactured two soft-locks:

- a body dropped on a chokepoint could permanently freeze a guard pathing past it
  (#182), and
- a takedown from a cupboard could drop the body onto the cupboard's only mouth and
  trap the *player* inside (#170).

Both are the same failure — an unmovable body becomes a wall nobody can pass — and
§2.2 forbids a run ending to a dead end rather than a decision.

Making the body non-solid deletes the whole class at the root. The cost stays real
(it is loud evidence, it must be dragged and hidden, and it runs the §7.3 clock); it
is just no longer a wall you can build against yourself.

---

## Appendix 5 — The alert ladder: what the sim measured

*(§7.3, #376 and #374.)*

Every threshold in the ladder is a knob the headless sim turns without a rebuild
(`--alert`, §13.2), and the ladder is measured: each run records the **rung it
reached, the turn it reached it, and the trigger that got it there**, so a batch
reports the *path* up the ladder rather than a single number. The first sweeps, 100
seeds each (`--seed 0 --cap 1000`), repeated on a disjoint block:

| `sighting-contact-turns` | 1 | 2 | **3** | 5 | 8 |
|---|---|---|---|---|---|
| Mean peak rung (balanced profile) | 0.98 | 0.88 | **0.70** | 0.45 | 0.15 |
| Runs never noticed (rung 0) | 25% | 28% | **42%** | 63% | 88% |
| Win rate | 0.38 | 0.34 | **0.35** | 0.36 | 0.38 |

Three findings, and the third is the one that matters:

- **The contact threshold is the ladder's real reach knob.** It moves the mean peak
  rung across nearly the whole range the ladder has. The **window** length barely
  does: 8, 10, 14 and 20 all read a mean peak of 0.69–0.70, because widening it makes
  one sighting easier and three *separate* sightings harder, and the two cancel. So
  the **10** is not load-bearing and does not need agonising over; the **3** is.
- **Rung 3 is unreachable without takedowns.** Over 200 balanced-profile seeds, at
  every threshold swept, **no run reached rung 3** — its triggers are a found body and
  a second quiet post, and a player who strikes nobody produces neither. The profiles
  that leave bodies reach it in ~8–9% of runs. The top of the ladder is a *takedown
  player's* rung, which is coherent with §7.2 but worth knowing.
- **Reach is tunable; consequence was not yet measurable.** Across every sweep the win
  rate stayed flat — 0.34–0.38 on one seed block and 0.41–0.44 on a disjoint one —
  while the mean peak rung moved from 0.15 to 0.98. Sweeping the rung-1 dwell cut
  itself, from *no cut at all* (3–7) to the harshest (1–1), moved the win rate by
  about three points, which is inside a 100-seed batch's own wobble.

**What the reinforcements then did** (#374, the same 100-seed batches, one per
playstyle profile):

| Profile | Win rate before → after | Reinforcements faced (100 runs) |
|---|---|---|
| baseline | 0.35 → 0.36 | 14 |
| cautious | 0.61 → 0.58 | 12 |
| aggressive | 0.53 → 0.51 | 42 |
| careless | 0.51 → 0.46 | 40 |

The ladder now has a consequence, and it is **proportional to how loudly you play**.
The avoidance-first temperaments barely reach rung 2, face about one arrival per seven
runs, and are unmoved. The two that leave bodies reach rung 3, face four times as many
guards, and pay 2 and 5 points for it — which is what §10.2's ~8–10 points per guard
predicts for ~0.4 extra guards a run. That is the flat curve above finally bending, and
it bends for the runs that earned it.

So **the [START] thresholds stay where they are.** The curve that would justify moving
one is the outcome curve, and it was flat: retuning a threshold against a flat curve is
tuning noise, not evidence (§13.3). What the flat curve actually says is that **rung 1's
teeth are its only teeth** — shortening a patrol dwell changes how a facility *feels*
without changing how often the raid succeeds.

Two caveats the numbers carry (§13.4): the bot has **no rung-aware policy** — it does
not play differently when the facility is loud, so this measures *pressure*, not *play*
— and a trigger reading zero is **inconclusive**, not harmless. `second-post-silent`
fires in no batch measured, and the honest reading is not "the trigger does nothing" but
that a found body has already taken the facility to rung 3 before a second post can, so
the escalation belongs to the louder event.

---

## Appendix 6 — Reinforcements walk in

*(§7.3.)*

**New guards entering mid-level reverses an earlier explicit "out"** (*"spawning new
guards mid-level — nothing in the design supports it; the guard count is a generation
knob"*), and the reversal is deliberate: rungs 2 and 3 announced an escalation and did
nothing, which is appendix 1's worst row. The three-rung ceiling is what keeps it from
spiralling — however loud a run gets, the facility gains **at most three**.

**Why never in view.** An arrival the player witnesses is a guard materialising out of
nothing, which no amount of fiction repairs. So the arrival cell is outside the player's
field of view and never adjacent to them, diagonals included — and if the facility
offers no cell that honours it (a small room a waiting player can see all of, since
waiting buys 360°), **nobody arrives**. Breaking the rule is worse than missing the
reinforcement.

**Why the guard sense is not gated with it.** A reinforcement arriving inside the sense
box reads as a new dot, which is position-only information the player earned rather than
a witnessed materialisation. Gating on it would also be unworkable — a turn spent waiting
widens the sense to a 41×41 box, the whole v1 footprint.

**Why their beat is cut where the errand ends, not where they arrived.** A beat grown at
the arrival cell would tether every reinforcement of a run to the same far-end room, since
the arrival region is chosen for its distance from the player and that answer barely
moves.

**Why they search rather than hunt.** More guards converging on a stale cell is *the net
closing*, which is what §7.6 asks for; more guards tracking the player's live position is
the un-fun chase §7.6 exists to prevent.

**Why their lead is *not* sized to the journey any more.** It was, originally: a
reinforcement starts at the far end by construction, and the ordinary §7.4 duration would
strand it halfway across the map having looked at nothing. That was a local patch on a
general bug — **every** responder was spending its investigation clock on its commute, so a
radio dispatch far from the patrols quietly cost the player nothing (appendix 27). With the
lead frozen while a responder is walking, the ordinary constant covers any distance and the
special case is gone; a walk-in and a dispatch from next door now behave identically,
which is what §7.3 claimed all along.

**Why a silenced radio does not stop them.** The comms console's effects are the enumerated
ones, and the ladder's rungs are not among them: silencing the net buys you the *internal*
net, not the escalation. This is the literal reading and it is what ships; **[OPEN]**
whether it is the right one, since the alternative (control cannot send what it cannot be
told about) is a coherent rule somebody may prefer.

---

## Appendix 7 — The comms console's price

*(§7.3.)*

- **The cost is the route, not the switch.** One bump is cheap; getting to it is not.
  Placement distance is therefore the balance knob (**[START]**; the sim sweeps it), and
  the reason the console is not simply free: a console found in the first few turns would
  make every later takedown free, which is exactly the collapse §7.3 exists to prevent.
- **It is findable, not given.** Contents are fogged (§11.5a), so the console has to be
  *scouted*; the map never advertises it. And it is asserted reachable like an objective
  (§10.6) — **counterplay the player cannot reach is not counterplay**, so a seed that
  seals it away is a generation reject.
- **Errands are not recalled**, which keeps it counterplay rather than a panic button. It
  also follows §7.7's own rule that a call, once made, is never queued or retried — there
  is no channel to un-send one down either.
- **A silenced facility is lonelier, never blind.** Nothing touches what a guard does with
  its *own* eyes: the one that loses you still searches, the one that finds a body still
  hunts it. Only the *calling of others* stops — and where a *calm* one chooses to walk.
- **The trade is coordination for predictability.** The console used to be one bump and all
  upside, which is exactly the appendix 1 failure it exists to answer: cost is the
  load-bearing property of every ability, and this one had none beyond the detour. So a
  dead net buys the loss of guard cooperation and pays with the loss of a learnable patrol.
  You can no longer stand somewhere and *know* a guard will not come. **Wandering is not an
  upgrade to the sweep** — farthest-first is what makes patrols read as purposeful (§7.5)
  and it is deliberately given up here.
- **Random, not farthest-over-the-whole-level.** Handing every guard the map while keeping
  the deterministic tie-break would make clustering *worse*: the attractors become the map's
  extreme corners, drawn from one shared candidate set, and the per-guard inspected memory
  that would otherwise separate two guards converges the moment their cones overlap — which
  they will, since everyone is walking to the same corners. Random removes the determinism
  causing the lockstep rather than patching round it. The draw comes off the run's own
  seeded stream, so a silenced run reproduces like any other (§12.4).

---

## Appendix 8 — The patrol dwell: why it is unconditional, and why the window grew

*(§7.5, #153.)*

The dwell was a 50% roll over 3–5 turns, and at that rate it was not the thing a player
saw. Measured over twelve seeded runs, **92% of every stationary spell a patrolling guard
took lasted one or two turns** — not a dwell at all, but the slow 90° turn and the
two-rotation 180° about-face. 42% of the two-turn stops were immediately followed by the
guard walking back the way it came, so *reach the end, spin, come straight back* read as
the patrol's actual rhythm, and the real pause — under 8% of stops — was lost inside it.

**A pause that fires half the time is not a rhythm a player can plan against; it is a thing
that sometimes happens.** So it became unconditional (every arrival) over a longer 3–7.

Note that the **stop the player sees** runs a little longer than the dwell: a guard turning
to leave spends one more turn rotating for a 90° heading, or two for a reversal, so a 3–7
dwell reads as 3–9 turns of held ground. The dwell is the part with the facing pinned,
which is the part a Takedown needs.

---

## Appendix 9 — Partitioning the level between the guards

*(§7.5, §10.5.)*

**The weakness this replaced.** Territories were once *"boxes around spawn points, which
have no relationship to the building"* — they straddled walls, spilled into unreachable
rooms, and overlapped arbitrarily. §10.5's region graph fixed the *shape* (a beat is rooms
and the corridors joining them, all of it walkable); dropping the spawn anchor fixed the
*tether*; and the partition fixed the last of it, the **overlap and the gaps** — two guards
grinding one wing while another had nobody, which a per-beat ceiling of four regions on a
seventeen-region level guaranteed.

**What it costs, measured — and it is not small.** Covering the whole facility is a real
difficulty increase, not a tidy-up. Over 100 seeded bot runs per playstyle:

| profile | win rate | diversity |
|---|---|---|
| baseline | 0.36 → **0.34** | 0.670 → 0.672 |
| cautious | 0.57 → **0.55** | 0.493 → **0.413** |
| aggressive | 0.50 → **0.37** | 0.602 → 0.584 |
| careless | 0.47 → **0.30** | 0.592 → **0.514** |

**The bold profiles are the finding.** A careful player loses 2 points of win rate; a bold
one loses 13–17, because the ground a bold plan crosses is now patrolled ground. Takedowns
fall with it (careless 14 → 8), so the striking line is *harder to run*, not merely riskier.
Alert peaks rise across the board and quiet runs nearly halve (careless rung-0 37 → 21).
Turns to win rise for the careful profiles (balanced 129 → 154) and barely move for the bold
ones — they are not playing longer, they are being caught. The single sharpest illustration
is the sim's pinned seed 42: a 111-turn win with **zero** detections through ground nobody
patrolled, now a capture at 216 with seven.

**Strategy diversity falling is the part to watch** (§13.2: *win rate tells you if the game
is hard, strategy diversity tells you if it is interesting*). Two profiles lose ~8 points of
it, which is the smell of a level admitting fewer distinct answers. If this proves too harsh
the lever is the **guard count** (§10.2, ~9 points of win rate per guard): four guards now
genuinely cover the facility where they used to cover something under two-thirds of it. It
is not a reason to go back to leaving wings empty.

---

## Appendix 10 — The chase had no exit

*(§7.6 — read this before touching guard AI.)*

**This is the known reason the game was not fun, from direct play:** *guards that saw you
tailed you relentlessly; breaking out of sight was neither easy nor fun, even with Run.*

That is not a tuning problem. **Four rules combined into a tracking turret.**

1. **Facing follows movement, and a chasing guard moves toward you.** So **its cone is
   re-aimed at you every single turn, for free.** You cannot leave a chasing guard's cone
   by moving. It is a turret that never needs to traverse.
2. **Detection is binary at a flat range of 10, with no falloff.** A guard tracks you
   exactly as perfectly at 10 cells as at 1. **Distance buys nothing.**
3. **Run gains 5 cells against a range of 10.** 2 cells/turn for 5 turns = a 5-cell gap,
   into a 10-cell range. **Run cannot break contact — it arithmetically cannot.** Then 12
   turns of cooldown at parity speed, cone still locked. The player does the obviously
   correct thing and the maths forbids it from working.
4. **Corridors are full-span straight sightlines, by construction.** The primary structure
   of every level runs the *entire span of its region* — up to 38 cells, dead straight, 2–4
   wide — and **cover is only ever placed in rooms**. The space you flee through is a
   shooting gallery.

Cone tracks free + distance irrelevant + escape tool can't outrange + nowhere to break sight
= **the chase had no exit.**

And on the rare occasion sight *was* broken, the guard walked to the last known cell, found
nothing, and resumed patrol immediately. So the chase was **binary: glued, or gone. Never
hunted.**

> **The hunted phase is the entire game.** Break sight, slip into an alcove, hold still,
> watch the red cone sweep past, breathe out, move. **That experience did not exist in any
> form.** Everything in §7.6 and §7.7 exists to create it.

**The ordering of the fixes matters.** The old problem was *never* that guards gave up too
fast — it was that **you could never reach the giving-up phase**. Make the chase able to end
first. Making guards search harder while the chase is still inescapable makes the game
*worse*, not better. And the geometry fix (§10.1a) is probably the single biggest
contributor, while being invisible if you only look at the AI.

---

## Appendix 11 — Clipping the post-search watch to the guard's own territory

*(§7.6.)*

Every guard that answered one call carries the same `focus` — a call carries its own cell and
inherits nobody's memory (§7.7). Handed the same *disc*, two responders spend the whole
window pacing one region and converge into the single moving clump §7.6 exists to prevent:
measured on a two-guard scene, mean separation over the watch runs **4.2 cells on a shared
disc against 7.9 on clipped territory**, closing to shoulder-to-shoulder against three cells
apart. Sharing a focus is right; sharing a *territory* is the accident.

**It also makes the watch slightly kinder, which was not the aim.** Two responders splitting
one area into halves cover it *less densely* than two pacing all of it, so over 100 seeded
bot runs the win rate rises — balanced 0.34 → 0.38, careless 0.30 → 0.35, cautious 0.55 →
0.53 — clawing back part of what the appendix 9 partition cost. The goal was legibility
rather than difficulty, and the honest reading is that a clump is both uglier *and* harder
than coverage. If the watch wants its bite back, the knobs are `WATCH_RADIUS` and
`WATCH_DURATION`, not un-splitting the responders.

---

## Appendix 12 — Dephase's safety eject: the third answer

*(§8.3.)*

This is the third answer to a question that has now been asked twice — what happens when a
Dephase duration expires while the player is inside something solid.

- It was **free**, which made phasing consequence-free.
- Then it was **lethal**, which made it the one death §2.2 forbids: the timer is on screen
  but the *lethal half* never is (`can_rematerialize` is invisible, and while phased you
  cannot bump, so you cannot even probe the cell you stand in), and §4.5 is **[SETTLED]**
  that a guard's touch is the only loss condition.

So the cost stays and the death goes: **turns spent helpless on a cell you did not choose,
in a facility where contact captures**, are a price a player can see coming and choose to
pay.

**The stun is as long as the throw**, because that is what prices recklessness. Clipping the
corner of a table strands you one cell from open floor and costs the smallest stun there is;
burying yourself a ring deeper into a wall block costs more, because the eject had to reach
further to find you anywhere to stand. A flat rate charged the near miss and the deep dive
the same, which made the worst case as cheap as the safest. In practice the ability caps its
own damage: Dephase runs four turns counting its activation, so a phase begun outside buys
three steps in and the stun tops out at four turns — the arithmetic goes further, the ability
does not.

That cap moved once, deliberately (#449): the window was three turns, two steps in, a stun
topping out at three. One more turn is the smaller half of the change — two steps in becomes
three, which is the depth the ability is actually for — and this paragraph is the larger
half. The stun is as long as the throw and the throw is bounded by how deep you can get, so
buying reach buys worst case at the same rate: the price of recklessness rose from three
turns helpless to four on the same turn the tool got useful. That is the §2.3 trade an
ability is supposed to carry, and it is why the lockout was left at 30 — one knob at a
time, or neither is measured.

**The randomness is load-bearing.** A predictable eject would make phasing into a wall a
reliable way *through* one, and you may well be dropped back on the side you came from.

**Why the near line names the tech, not the terrain.** It is *any* solid — a shut door, a
table, a cupboard, a console — so a message that said "the wall" would be untrue in most of
the cases it covers. Naming the tech ("safety eject — stunned") also gives the fiction for
why this is survivable at all: the salvaged rig throws you out rather than letting you set
inside the furniture.

**Why the early toggle-off is not extended the same way** (#304/#329): pressing the key
inside a wall is still refused, because a free press that teleported you clear would be
exactly the escape tool this is designed not to be.

**The mark, and why it was admissible (#416).** The player's cell is now marked while a
running Dephase has them somewhere a solid body cannot stand. §11.2's rule for marking
the player is strict — a mark must say what the §11.4 bar cannot — and it named a running
Dephase as its counter-example, so this needed a reason rather than a preference. The
reason is in the second bullet above: the case against the *lethal* answer was that
"`can_rematerialize` is invisible, and while phased you cannot bump, so you cannot even
probe the cell you stand in". The eject removed the death but left that invisibility
untouched — the risk was still live or not depending on where you stood, and nothing on
the board said which. The mark is that predicate made visible, and it is admissible under
§11.2's own rule rather than as an exception to it: what is conditional is not the
ability but the **cell**, so the mark blinks off and on as you walk while the bar entry
never changes. That the eject's landing is random (above) is what makes this worth ink at
all — a risk you could plan around by eye would need no cue.

It is deliberately gated on `can_rematerialize` itself rather than on a second reading of
the terrain, so the picture cannot claim a turn the rule would not — the same discipline
Camouflage's mark follows. **Entombment** — no legal landing cell anywhere, a loss rather
than an eject — is marked too, and needs no special case: "you are inside a solid" is
just as true there.

---

## Appendix 13 — Sound was dropped for the guard sense

*(§9, and the old §15 Q3.)*

Sound was meant to be the channel that let the player steer guard attention and track
threats around corners — *"a second information channel that works around corners"*. It was
the most-built and most-praised idea in the old design, and it was tried in this rebuild: a
full cell-to-cell propagation field, guards that hear, a loudness ladder, a "how far you were
heard" overlay.

**It came out obscure and not fun.** An invisible field, tuned by numbers with no on-screen
consequence, doing its work behind the UI. *"How is sound presented?"* was never answered
because the honest answer is *it wasn't*, and **an invisible sound system is a missing one.**
The complexity was real; the fun was not.

So the rebuild drops sound entirely and keeps only the thing sound was actually *for*: the
player knowing, around corners, where the threats are. That channel is now **direct**, and it
is the inverse of sound's failure — sound was a hidden model with a visible-nowhere
presentation; the sense is a **visible model with an obvious presentation.**

Why the sense is the better trade:

- **It is visible.** Sound's fatal flaw was that it had no good presentation. The sense's is
  trivial and obvious: **draw the dot.** There is nothing left to solve.
- **It is legible without being omniscient.** You get *position*, not *attention*. The
  dangerous unknown — *is it looking at me?* — is preserved and tied to line of sight, which
  is where the whole game already lives.
- **It rewards Wait**, the game's one "spend a turn to know more" verb, instead of bolting on
  a parallel system.
- **It deletes a large, obscure subsystem** — propagation, emission, the loudness ladder, the
  hearing check, the noise overlay — in favour of a range check and a render state. Less
  code, less tuning surface, more clarity. That trade is the point of §3's "honest pressure
  systems": a system that isn't fun doesn't earn its complexity.

**What the removal cost elsewhere, and how it was paid.** Guard cooperation (§7.7) and the
radio (§7.3) both leaned on sound for legibility — the player was meant to *hear* a ping.
With sound gone, every call and every radio event needs a **visual / near-line** cue instead.
The sense helps for free: a responder peeling off its patrol toward your last position is
directly readable as a moving dot on the map. The rule that falls out is that no §7.7 call
may ever depend on a sound the player has to hear.

---

## Appendix 14 — Bench concealment: the half-plane, after two rewrites

*(§10.3.)*

Concealment behind a bench of tables has been rewritten twice, each time because the shape of
the protected zone was wrong rather than because the geometry was computed wrongly.

1. It began as the **quarter-plane** behind the single bumped table — which let a guard look
   down a bench and see you through its other tables, undercutting the exact cover §10.1a
   places.
2. That was replaced by a **per-ray** test across the whole run: faithful and exact, but too
   tight in the other direction. A short bench subtends a narrow wedge, so a guard only a
   little off the run's axis had a clear line — and, the deciding complaint, **the player
   cannot compute that wedge at a glance**, which made the crouch a turn spent on protection
   you could not predict and usually did not get. Since partial cover is the counterplay
   §10.1a places in every corridor, a coin-flip crouch means that counterplay is not there.
3. The **half-plane** per arm replaced it. It costs the crouch some precision at the ends of a
   bench and buys back the one property a counterplay has to have, which is **being readable
   before you spend the turn.**

It is deliberately the more generous of the two. If bench-hugging turns out to dominate, the
levers are the ones §10.3 already names — the turn it costs, contact-vulnerability, the
crouch-walk's requirement to keep hugging — **not** re-narrowing the geometry the player has
to read.

One older behaviour is gone with it: **waiting beside a table used to crouch automatically.**
That coupling is removed — Wait is pure (its 360° look and nothing else), and the crouch shows
its direction in the usable line like every other bump.

---

## Appendix 15 — Corridors had no cover

*(§10.1a.)*

Corridor-first partition is the right structure (§10.1) but it has a severe emergent flaw that
only shows up in play: **it produces long, dead-straight, full-span corridors with no cover,
and those corridors are where the player flees.** The rooms got pillars and stubs. The
corridors — the majority of the map, the connective tissue, the place every chase happens —
got **nothing**. A 38-cell straight 3-wide corridor with a guard in it has no counterplay. It
is not a space; it is a sightline.

**How the rule's wording moved.** It was first stated as *"no unbroken sightline"*, and the
repair pass stamped 1-cell **wall** blockers — which read as floating wall noise, not a
building. The table restatement replaced them; the cupboard clause came with the
no-tables-in-corridors rule: same assertion machinery, honest architecture.

**Why the counterplay follows the region.** A lone table read as noise, and a table in a
corridor read as a barricade in a hallway — so neither is generated, and both are asserted
away. Rooms get benches of furniture; corridors get architecture (a recessed cupboard, an
alcove, a structural pillar, a buttress). The flight path stays clear.

**Why the rule constrains the generator and not the player** (#303). Pierce Wall can punch a
hole into a corridor's long wall from the room side and create exactly the uncovered straight
run this rule forbids — and that is correct, not a loophole to close. The rule exists so a
level is never *born* with an unsurvivable sightline; a player who cuts one has made a choice,
and the danger overlay draws the new cone the moment a guard's line reaches down it, so the
consequence reads as their own doing rather than as a bug.

**The hiding game had no board.** Hideouts were placed **one attempt per room, stopping at the
first failure** — so a level could easily have very few, and **never any in corridors**.
Combined with §7.6, that means during a chase — the exact moment the hiding game is supposed
to happen — there was nowhere to hide. The original harvest rule (*"a wall cell with exactly 3
wall neighbours and 1 empty neighbour"*) only ever found the rare natural pockets; the backing
that makes a recessed cupboard possible is now **manufactured** by the wall-thickening pass and
the cupboard placed deliberately rather than harvested.

*(**Jogging the corridors** mid-carve — offsetting a corridor a cell or two mid-span — remains
the unimplemented alternative if §15.2 wants it.)*

---

## Appendix 16 — The spatial model was one rectangle

*(§10.5.)*

The old version had exactly one spatial abstraction: **an axis-aligned rectangle**. It was
asked to be the level bounds, the partition regions, room identity, guard patrol territory,
*and* the UI viewport. It was not up to any of it.

The problems, which are worth understanding because they cascade:

- **It cannot describe the spaces the game has.** A room with a pillar isn't a rectangle. An
  L-shaped nook behind a stub isn't a rectangle.
- **Corridors are not regions at all.** They're painted into the plan and never recorded. So
  the connective tissue where most stealth gameplay happens is *spatially unaddressable*.
  Nothing can ask "which corridor is this?" or "does this corridor reach that room?".
- **The regions are generation scaffolding that gets thrown away.** Once the level exists it
  has **no concept of rooms**. No registry, no cell→room lookup.
- **Therefore everything downstream has to fake it.** Guards patrol a box around wherever they
  spawned, because there is no vocabulary in which to say "cover the east wing". *That* is why
  guard cooperation, assigned patrols, in-level lore placement, keys, and circuits all stayed
  unbuilt — they were all blocked behind this one missing abstraction.

The generator already builds the graph — corridors are nodes, rooms are nodes, doors are edges
— and then discards it. Keeping it is **the highest-leverage structural decision in the
document**: nearly every "guards should…" idea depends on it.

---

## Appendix 17 — Solvability, and the one-usable-per-cell preference

*(§10.6.)*

**The old generator never verified solvability.** It relied on a structural argument — every
room is bounded by corridor walls, which qualify as door candidates — which has a hole: **a
wall run shorter than 3 cells gets no door.** Punch-throughs fragment wall lines, and if every
run bounding a room came out < 3, that room seals, with its objectives and guards inside.
Nothing detected it, nothing repaired it, and no seed was ever rejected. Hence the rule: do not
rely on a structural argument, assert reachability and reject the seed. It is a flood fill, it
costs nothing, and it is exactly the kind of property a generator must never merely *believe*.

**Why "one usable beside any floor cell" is a preference and not an assertion.** Two guarantees
outrank it. Connectivity comes first, and so does the sightline rule (§10.1a), whose repairs
must land where the run is — a bench beside a room's door span, a repair cupboard close to an
existing usable — so a doubling with a nearby door is sometimes unavoidable. Forcing the piece
off-centre to dodge it only shortens the run instead of splitting it, multiplying generation
cost for a cosmetic win. And structural doors can cluster in a way no carve undoes.

An earlier draft made it a hard guarantee; **measured, it rejected ~85% of carves and stalled
generation.** The usable line's per-bump arrow already buys the legibility the guarantee was
chasing, so the honest rule is best-effort placement plus the arrow.

Also inherited from the old generator, all real, all fixed by the §10.6 spacing and
fail-loudly rules: nothing separated the player from the exit (they could spawn adjacent),
nothing spread intel out (all 3 could land in one room), nothing kept a guard from spawning
where it saw you on turn one — and placement failed *silently*, guards getting 10 attempts and
then being quietly dropped, so you asked for 5 and got 4 with a log line nobody read.

---

## Appendix 18 — Fog: what became "contents"

*(§11.5a, §10.7.)*

> **The shape-versus-shade argument is in the render reference, not here.** *Why unseen geometry
> draws as a schematic (`≈` fabric, `~` floor space) rather than as a fourth rung on the §11.5
> dimming ladder*, *why a doorway is `~` and its frame `≈`*, and the A/B against a denser `▒` mark
> on a real 40×40 board that was built and rejected, are
> [`docs/render-reference.md`](render-reference.md) §2.3–§2.4. That is the authority on what the
> schematic draws and why; this appendix covers only what the fog decided to *hide*.

**A stated change, not drift.** §10.7 originally promised a duct entry visible from turn one
"like a door", and furniture counted as geometry too, on the grounds that being surprised by a
table mid-flight is as bad as being surprised by a wall. Both now have to be **found** — a duct
mouth is a recess backed by structure, and a table is something put in a room, not part of it.
Cupboards were already hidden on exactly this reasoning (*"the flight paths you scouted are
worth more than the ones you didn't"*), so ducts join them rather than sitting on the other
side of an inconsistent line. Room shapes, wall runs and the openings between them still read
from turn one, so you are still never lost and never mapping.

**A second stated change: a duct's interior path is not remembered.** It used to read as plain
wall, remembered once crawled. Now that the path can overlie floor, "reads as plain wall" is no
longer even true, and a remembered overlay would paint a tell on the room floor a duct crosses.
The `=` you plan around is the entry alone.

**The cost is meant to be payable.** §12.6's `full_layout_known` modifier hands the whole layout
over as an *easier*-direction modifier — so under the directed difficulty draw it is bought with
pressure taken on elsewhere, never given away.

---

## Appendix 19 — The decoy is always drawn

*(§11.5a, §8.3.)*

An always-drawn decoy is not a hole in the live-state rule, it is a different layer: a decoy is
neither the facility's live state nor a content to be discovered, but **the player's own placed
object** — the same category of knowledge as their own cell or the body in their hands. The
whole point of a fake is to *walk away from it* and let a guard investigate the wrong cell, so a
marker you can only see by standing next to it is a marker the ability cannot use, and
route-planning around your own bait is exactly what its disappearance takes away.

**It leaks nothing new.** A decoy dies the moment anything steps on it, so an always-drawn one
might seem to announce "a guard just walked here" through a wall. The game already announces it,
twice, on the turn it happens: the death ends the ability into its full cooldown, so the ability
bar flips from *active* to *cooling* wherever you are, and it prints *"the decoy is trampled"*
with no visibility filter. Drawing the `@` only puts that same fact where the player is already
looking, instead of making them infer a location from a cooldown pip.

**This is the decoy alone.** A body you dropped is not covered: it is the facility's live state
and the §7.3 clock's evidence, and **being unsure whether it has been found yet is
load-bearing.**

---

## Appendix 20 — The ability bar

*(§11.4, and the old §15 Q9.)*

**The fixed 14-column list is gone.** It spent a seventh of the screen on information consulted
once a minute. Ability state must stay *discoverable*, so where it should live was an open
question, and **three experiments answered it**:

- Showing the list *while waiting* buried the 360° guard-sense the wait exists to reveal (§9.1).
- A left-aligned header strip put the tap target furthest from the thumb.
- A compact bottom-right strip of bare hotkeys, with a deploy button unfolding the named panel
  over the board, put it in the right corner but made every name a second tap away.

**What unlocked the settled answer was capping the held set at four** (§8.3): the names fit, so
the compression the old strip paid for bought nothing worth its cost.

**Why the names never move.** A bar whose words slide about as numbers come and go is a bar you
have to *read* every time you look, and the whole case for it being always-on is that you learn
its shape and then only **glance**. An ability's column is a fact about the run, and since #359
it *is* its key — position is muscle memory too.

**The width budget, which is what makes the names fit.** Four named slots across a 40-wide board
is tight, and the arithmetic is exact:

| | cells |
|---|---|
| Longest state notation (`/45/` — the catalogue's biggest cooldown, plus delimiters; a passive's `(on)` is deliberately no wider) | 4 |
| Longest **bar name** (`Decoy` / `Phase` / `Doors` / `Sight`) | 5 |
| Widest entry, so the slot's content width | **9** |
| Plus one cell of air → **one slot** | **10** |
| Four slots | **40** |

That is the whole board width, with nothing spare — which is why each ability carries a short
**bar name** distinct from its full §8.3 name, and why the notation is tucked hard against it.
Every input is derived, so renaming an ability, pushing a cooldown past 99, or granting a fourth
tech fails the *build*, not the frame (#287).

**Why the Help tab shed the catalogue.** It listed the whole eight-ability catalogue when a run
holds at most four, and **a reference card that changes with the loadout is not a reference
card** (#296). The per-run pairing moved to the Abilities tab; the Help tab keeps only the
standing controls.

---

## Appendix 21 — Input: digits, letters, and the touch slack

*(§11.6.)*

**Why digits bind by physical key.** An AZERTY top row is `& é " '`, and a character binding
would want Shift to fire an ability in the turn things go wrong.

**Why no character binding may name a digit** (#369). The numpad-moves / top-row-fires split only
holds if nothing downstream can undo it, and a character can't: both blocks produce `"2"`, so a
table matching on `"2"` is answering a press it cannot identify — and whichever table is asked
first wins. Without the three rules §11.6 states, a top-row `2` steps south instead of firing slot
2 — which is worse than nothing happening, because it spends the turn *and* moves you, and it
hides: slots 1 and 3 work, because no movement key happened to claim those characters.

**Why ability keys are slots rather than identities** (#359). Four keys, forever. A run holds at
most four abilities while the catalogue keeps growing (salvaged tech — §14 v3), so identity-keyed
letters meant every new ability needing a free letter it could keep for good, with the best
mnemonics gone first and the twelfth ability getting whatever was left.

**What that trades away, and why it is safe.** A key is no longer a fact about an *ability*: `c`
was Camouflage in every run ever played, and `1` is not. That is the cost, and it is real. What
makes it payable is that the slots are **fixed for the whole run** and drawn on screen at all
times, so a digit is never ambiguous where it is pressed — and `Run` is innate and always first,
so the most-pressed key in the game keeps its cross-run constancy anyway. This is *not* the old
regression coming back: that one let a key change **because another ability's name changed**,
silently, between one run and the next, with nothing on screen to say so. A bar slot is visible,
stable within the run, and the same thing your thumb taps.

**Why the reserved tables gave way instead of the letters** (#368). A mnemonic still may not
shadow a movement or system key — a mis-key ends a run — but that guarantee has to be one the
claim rule never *notices*, because a skip whose cause is off screen is unreadable: Lockdown
showed `o` with nothing in the run holding `l`, and no way to find out why. So movement is the
arrows and the numpad, and the vi keys `h` `j` `k` `l` are gone with it — **a comfort binding is
not worth a quarter of the alphabet the bar can never use**, and it was costing a shipping
ability its own name.

**Why the mnemonic is not the derivation this section designed out.** Three things separate them.
The claim set is the **run's four**, not the catalogue's twelve-and-growing, so a letter can only
be taken by something you are also holding. It is **not silent** — the letter is drawn, marked, on
the entry it fires. And the **digit is always underneath**, so a player who never learns a letter
loses nothing. What remains true, and should not be glossed: **the same ability can carry
different letters in different runs.** It is a fact about the loadout, like a bar slot.

**Why a press is unbound on the modal screens.** Resolving it to *activate* would let a stray tap
on empty menu space start a run by accident — the class of bug #306 closed on the board, and worse
here, because starting a run is not undoable.

**Why the dead band exists, and why the ability bar gets slack anyway** (#306, #386). In a
permadeath run with no undo a silently spent turn is unrecoverable, so the tap-to-wait boundary is
*forgiving* rather than merely correct, and a near-miss on the flush-right ability block must cost
nothing. **The fix is never to move a drawn target, and never to grow one into space that
answers.** The one row of slack above and below the bar stands on exactly that: nothing drawn
changes, and the two rows it grows into are silent by construction — the row above is always inside
the dead band, and below the last row there is only letterbox. Forgiveness may turn silence into a
hit and may never take a live board tap away from the board. The cost is honest — a tap one row
above the bar was free and can now spend a turn — and it is the one thing to watch in play: nobody
can be *aiming* a Wait at a dead-band row, but it is the row a thumb reaching for the board's
bottom edge brushes. If it misfires by thumb, drop the upper row and keep the lower one rather than
widening further.

**Why chrome resolves on the lift.** *A turn must never burn on a gesture the player didn't
finish.* A press arms the control and the lift over that same control fires it, so a mis-press can
be slid off and abandoned — and it puts both input surfaces' resolution at the same moment so they
behave alike.

---

## Appendix 22 — Determinism, and what the old version paid for lacking it

*(§12.4.)*

Nothing in the old version was seeded. Every generator built its own fresh unseeded source, and the
in-level random source handed out **a brand new generator on every single call** — so there was no
sense in which a run could be named, let alone reproduced.

"Play again" was therefore bought the expensive way: by serialising a byte-for-byte **snapshot of
the entire level** at run start and restoring it. That is a heavier, more fragile way to buy less
than `(seed, inputs)` gives for free — it reproduces one level rather than any level, it breaks on
every change to the level's shape, and it cannot be shared, replayed, diffed or bisected. Every row
of §12.4's table (bug repro, seed sharing, bot metrics, golden tests, regression detection, rewind)
was unavailable, and §13's whole experiment loop with it.

> **The token half of this ruling lives in the spec, not here.** *Why a bare `?seed=8371` is not a
> run*, *why abilities and modifiers are carried over 256 permanent slots*, *why slot numbers may
> never be renumbered and a retired entry leaves a tombstone*, and *how the eighteen characters
> trade seed space against integrity* are all specified — with the #286 Vision-passive incident that
> forced them, and the tests that pin them — in
> [`docs/level-seed-token.md`](level-seed-token.md) §3, §7 and §8. That document is the authority;
> this appendix deliberately does not restate it.

---

## Appendix 23 — Two things the time economy could not charge for

*(§8.2, #264 and #302.)*

§8.2 says the economy is **time**: turn cost, duration, cooldown. Two additions cross that
sentence, and each needed reconciling with it rather than quietly extending it.

**Passives (#264).** A passive is never activated, so there is no turn to charge, no duration to
run down and no cooldown to set — and appendix 1 is explicit that an ability with no cost is not a
decision. The reconciliation is that **the loadout slot is the price**: the set is capped at three
tech (#266), so holding a passive means *not* holding something else for the whole run. That is a
real cost, paid once, and it is the whole of it.

Why a passive reads as **`(on)`** rather than reusing an existing state: the four clock states all
mean *"and then it ends"*, and a passive never does, so `Active [N]` would make the number on the
bar a fiction — which is exactly what §8.2's timing trap forbids. Undecorated it read as one more
thing you could press, and it is the one entry on the bar you never can.

Why it is still an `Effect` list (§8.1) and not a parallel system: same vocabulary, applied
continuously instead of for a window. **Held is on** — there is no activation moment, so there is
nothing for a replay, a save, or a mid-run pickup to get out of step with.

**Uses per level (#302).** "No charges" is the right instinct, and this is the one thing it rules
out that the game needs: an effect too strong to hand out on a cooldown alone — rewriting the
level's geometry (#303), say. What makes a use budget a **bound** rather than a **bar** is that
there is nothing to spend, refill or manage: the number only goes down, it goes down for the whole
facility, and **no decision anywhere in a run is about getting more of it.**

The fence exists because that distinction is easy to erode a field at a time. No recharge of any
kind — a fresh facility is the only thing that gives a use, and it gives it by being fresh. Single
digits, enforced at compile time, because ten uses is an inventory. And both numbers are shown,
because they are different numbers: the bar shows what is *left* (`Bore(2)`, a count in
parentheses, since neither it nor `(on)` is a timer) and the help panel shows what a level *grants*
(`3/level`, #343).

If something later wants uses that refresh, or uses shared between abilities, that is a different
design conversation — not a quiet extension of this field.

---

## Appendix 24 — The fences on Confusion, Pierce Wall and Lockdown

*(§8.3, #240/#242/#303/#325.)*

Three of the newer tech are strong enough that most of their design is the fence around them. Each
fence answers the same question: *what stops this becoming the free win button of appendix 1?*

**Confusion — why it has no window.** The blast is fired once, from the cell you press it in, and
after the flash **distance stops mattering**: a guard you run away from stays dazed, and one that
walks into the cells the blast covered was never in it and is untouched. That is what keeps it from
being a no-guard-may-act field you carry around — it has no window, nothing to toggle off, and no
`[6]` on the bar. A dazed chaser **pauses** rather than resetting, so it is a costed panic-buy of
time and not a kill.

**Confusion — why the reach is clamped to the sense.** `min(CONFUSION_RADIUS, sense_range())`, read
off the live guard sense, so **the blast can never freeze what you cannot sense**. It is inert on
open floor (`min(6, 10)`) and bites only inside a duct, where it shrinks to 5 — degraded information
being the crawlspace's whole cost (§10.7). It can only ever shrink the blast, never widen it. This
is also what makes a firing with nothing in reach a *fair* free no-op: the clamp means anything it
could have caught, you were already shown.

**Pierce Wall — why the precondition is "exactly one adjacent wall".** The target is then unique by
construction, so there is nothing to aim (§8.4) — and, more importantly, **it rules the panic-bore
out by construction**, since a corridor and a corner both present two walls. You cannot dig your way
out of the place a chase has put you; you dig somewhere you chose to stand.

**Pierce Wall — why it does not ask what is behind the wall.** Boring a two-cell-thick run (§10.1.5)
opens a one-cell **pocket** off the room rather than a route, and that is a use of the tool rather
than a waste of it: a dead-end alcove out of the through-routes is somewhere to sit a sweep out. It
conceals nothing — it is not a cupboard — so whether the hole is shelter or a trap is the player's
judgement, and three walls around you means **you can dig a hole to hide in, never a tunnel**.

**Lockdown — why it is a snapshot, not a travelling bubble.** If a door unsealed because you walked
away from it, the wall you raised behind you would dissolve exactly as you fled down it.

**Lockdown — why the player is never refused.** It is your lock, so a sealed door bumps open for you
exactly as any closed door does, which is what stops a lockdown ever boxing its owner in. But that
costs the turn and leaves the door *open*, so a lockdown fired across a route you still have to
travel is a real mistake, and unmaking it is paid in the very turns the ability was bought to save.

**Lockdown — why every seal ends with the window.** The duration is the only clock any seal has,
which is what keeps a temporary wall from ever becoming the permanent one §2.2/§7.2 forbid. The same
representation carries the key-gated doors of the locked-doors modifier, so there is one lock on the
door rather than a second system (§10.4).

---

## Appendix 25 — The innate-verb floor

*(§11.4, #323.)*

**Wait is the least discoverable thing in the game and one of the most important** — the only 360°
look (§9.1), the way a crouch is held (§10.3), the way a cone is let past (§7.6) — and it
deliberately has no ability-bar entry, because the bar is the ability *economy* (§8.3). Without the
floor, the two verbs every run is built out of appear nowhere on screen.

The fence is what keeps it a hint rather than a control legend:

- **A floor, never a competitor.** The instant anything is adjacent, the affordances take the whole
  row back. No fade-out after N turns and no first-run-only flag either — the row stays a pure
  derived function of state, and it costs nothing either way.
- **Exactly the two verbs with no other home.** Takedown and Drag already appear there as real
  affordances (§7.2/§8.3) and the tech abilities have the bar; the full control set is the help
  panel's job.
- **Read-only.** A hint that could be pressed would be a second, undiscoverable control surface at
  the top of the screen, against §11.6's touch rule.
- **Owned, not Ground.** Ground was tried first and lost on screen: its meaning is **absence**,
  drawn to recede so everything else pops against it, which is exactly the wrong instruction for a
  row whose whole job is to be read. Owned is the blue the ability bar's ready entries use, so the
  two surfaces answering *what can I do right now* answer in one colour.
- **The wording follows the hands, not the hardware.** The modality is seeded from `pointer: coarse`
  at boot and then corrected to whichever modality was **last actually used**, so a laptop with a
  touchscreen and a tablet with a keyboard each get the hint that matches what the player is doing.
  The keyboard wording says `w`, not `5`, because the wait digit is the *numpad*'s and a floor hint
  has no room to say so.

---

## Appendix 26 — Where the guard count came from

*(§10.2.)*

The `--guards` sweep (`sim --bot`, 300 seeds each, **bare loadout**) traced the whole curve:

| Guards | 1 | 2 | 3 | **4** | 5 | 6 | 7 |
|---|---|---|---|---|---|---|---|
| Bare win rate | 80% | 61% | 48% | **37%** | 29% | 21% | 16% |
| Captures | 58 | 116 | 155 | **189** | 213 | 234 | 251 |
| Timeouts | 2 | 2 | 2 | **0** | 1 | 3 | 2 |

Roughly linear, about 8–10 points of win rate per guard — **no cliff**, so the number is a taste
call rather than a threshold. **4** is the forgiving-but-real end: a bare run wins better than one
in three, and it is the only row where *every* seed resolved to a win or a capture rather than
stalling.

Read it against §13.4. The bot has perfect information and no fear, but plays greedily and badly, so
a human sits well above its number: **this is a floor, not a forecast.** Nudge it back up once
guard cooperation (§7.7) and the radio net (§7.3) add pressure the bot currently never feels.

The same per-guard slope is the lever appendix 9 points at when the §7.5 partition proves too harsh,
and the units the appendix 5 reinforcement cost is measured in.

---

## Appendix 27 — The lead bounds the investigation, not the commute

*(§7.3/§7.6/§7.7.)*

A missed ping dispatches a guard to the takedown site, where it searches. That is the
appointment §7.3 sells: *"three takedowns is three clocks running at once"*, and the
strategy scales badly on its own. It did not, quite. The dispatched guard went
`Responding` with the ordinary §7.4 lead of 30, and the lead cooled by one on **every**
turn — travel turns included — so on a 40×40 board a site more than 30 steps away
(routinely, once doors and detours are counted) saw the responder stand down on the
road and drift back into patrol having looked at nothing.

The consequence inverted the mechanic: **the further from the patrols you struck, the
less the radio cost you.** It also made the game lie. §7.3 sells the tell as legible —
"a dot that visibly peels off toward the place you struck" — and a dot that peels off,
wanders halfway and turns yellow again teaches a rule that is not the rule.

**Measured before believing.** Over 480 bot runs (four §13.2 profiles × 120 seeds,
the sim preset), **12 of 64 finished errands — 19% — expired on the road**; on
`cautious` it was half of them. The distance sample says why: of the dispatches whose
site could be attributed, the median responder was 13 cells away *in a straight line*,
and a fifth were beyond 30 — and the straight line is a floor on the walk, never an
estimate of it. After the fix the same sweep expires **none**.

**The rule.** *A responder does not burn its lead while it is still on its way.* It is
scoped to `Responding` and to a turn on which the guard actually has a route to walk.

- **Why only `Responding`.** A call carries a **fixed cell that never updates** — the
  dispatch drops the responder's stale sighting — so a responder cannot follow the
  player, and freezing its clock cannot rebuild the tracking turret. A *chase's*
  destination follows you, and there the cooling lead **is** §7.6's anti-turret
  backstop; it is untouched, and pinned by its own test.
- **Why "has a route", not "is responding".** A guard held up by a colleague (§7.8) or
  sent somewhere it cannot reach still cools and still stands down. Nothing paces
  forever; only the distance penalty goes.
- **Why not simply raise `ALERT_DURATION`.** A bigger constant still couples the
  investigation's length to the commute — it only moves the distance at which the bug
  bites — and it lengthens every chase as a side effect. The two clocks wanted
  separating, not scaling.

**The second cause, found while auditing.** "The nearest active guard" was picked by
**Manhattan distance**, so the dispatch could go to the guard on the far side of a wall
with a sixty-step way round while one two rooms down the corridor stayed on patrol.
Over 8,312 (site, roster) cases across 200 generated levels the two orders name a
different guard **25% of the time**, and in *every* one of those the straight-line pick
was the longer walk — **1.36× the route length** on average. Not rare, then: a quarter
of all dispatches were handed to a guard that would take a third longer to arrive. One
flood from the site prices every guard's true journey in a single pass, which is cheap
enough on a 40×40 board to run per call. A guard with no route at all sorts behind
every guard that has one, and is still sent when nobody else is free — §7.7's rule is
that whoever is free answers, and control cannot know the way is severed.

**What it retired.** Reinforcements (appendix 6) had their own lead "sized to the
journey" for exactly this reason. That was a local patch on a general bug; with the
general rule in place a walk-in from the far edge and a dispatch from next door behave
identically, on one constant, and the special case is gone.

**What it deliberately did not touch.** The decoy's `Investigating` pull (§8.3) is also
a fixed cell, and so looks like the same shape — but the *bounded* pull is the decoy's
balance, not an accident. The §15 Q5 witness flush chases a cupboard with a fresh lead;
its sightline is bounded by the cone's own reach, so it is usually fine, but a long way
round through doors could still expire it. If that shows up in play it belongs in this
family and wants its own ticket rather than a quiet extra hunk here.

**The expected knock-on.** Responders that used to evaporate now arrive and search, so
bodies are found more often and the ladder is climbed harder. That is §7.3 working —
takedowns are *supposed* to scale badly — but it is a live balance change, and
`ALERT_DURATION`, `SEARCH_DURATION` and the ping interval are all **[START]** numbers
that may want to move once the sim has the new picture. Retuning them in the same
change would have been indistinguishable from the fix.

---

## Appendix 28 — The flank experiment, and why it is conditional on the guard's mood

*(§6.1, §6.2, §7.2, §12.6. Measured as a knob; **adopted as the rule** by #442 — see
the closing note.)*

§155 carved the three cells at a guard's back out of its detection set, so a takedown
can be lined up from directly behind. Its *sides* — §6.2 tier 3 — still detect, and
§6.1/§6.2/§7.2 all say so: *"you can never stand beside or in front of a guard
undetected."* Two things follow, and #410 asked whether they should:

- **A takedown must come from directly behind or rear-diagonal.** Step to a guard's
  flank and you are seen, though it is looking the other way.
- **You cannot tail a guard.** Walk in its blind spot and the moment it turns 90° at a
  corner you are at its side, tier 3, detected — so the one manoeuvre that should be
  the reward for reading a patrol is impossible.

**The experiment.** Carve tiers 3-5 instead of 4-5, so a guard detects exactly what its
cone covers and the free touching ring becomes player-only. A 180° turn still catches
you: that lands you at tier 1, dead ahead.

**Why it is a knob and not a constant.** It bends a **[SETTLED]** sentence, so it ships
as `calm_guards_detect_only_their_cone` (§12.6) and both arms run from one build. Two
properties make the comparison exact, and both are asserted:

- **The cone's silhouette is identical in both arms.** The carved cells stay §6.2
  artificial cone-carving walls; only their membership in the *detection* set changes.
  Anything else would change what walls shadow, which is a different experiment.
- **The two arms generate the same facility.** Spawn safety (`place.rs`) uses the same
  fov, so a narrower carve would pass more cells and shift where guards spawn. It is
  pinned to the shipped rear carve instead — the conservative rule, since a cell safe
  under it is safe under any wider blind spot — so **generation is not part of the
  diff** and a shifted seed's geometry can never be mistaken for a result.

### The unconditional form, and why it failed

Measured first without any condition: every guard, in every mood, blind at its flanks.
Paired A/B, 150 seeds x four profiles x two arms.

| | balanced | cautious | aggressive | careless |
|---|---|---|---|---|
| win rate | .360 → **.433** | .527 → **.573** | .387 → **.460** | .367 → **.520** |
| detections | 1352 → 1336 | 1684 → **1260** | 1206 → **959** | 1675 → **1222** |
| takedowns | 0 → 16 | 0 → 5 | 28 → **80** | 13 → **98** |
| alert peak | .76 → .94 | .77 → .82 | .99 → 1.17 | 1.08 → **1.45** |

The pre-registered sink was *"the striking profiles' win rate jumping while `detections`
collapses means the flank is simply cheaper stealth with no new decision"*. Its
**surface fired** — win rate up 7.3 and 15.3 points, detections down 20% and 27%. Its
stated *reason* did not: takedowns tripled and septupled and the facility got markedly
louder, so a new decision *was* being taken and paid for.

What sank it was something the pre-registration had not anticipated: **the win rate rose
on temperaments that never strike at all.** `cautious` gained 4.7 points on 5 takedowns
across 150 runs with a 25% fall in detections — almost pure free safety. The
unconditional form was doing two separable things: opening a genuine new play, and
quietly making a guard's side a safe place to stand. The second is exactly what §7.2
means when it says the takedown's constraints *are* the cost.

### The conditional form: blind at the flank **only while Calm**

A guard that is not Calm — chasing, investigating, searching, answering a call —
watches its sides exactly as it always did. Same seeds, same build:

| | balanced | cautious | aggressive | careless |
|---|---|---|---|---|
| win rate | .360 → .380 | .527 → **.567** | .387 → .393 | .367 → **.367** |
| detections | 1352 → 1372 | 1684 → 1735 | 1206 → 1170 | 1675 → 1297 |
| takedowns | 0 → **0** | 0 → **0** | 28 → **36** | 13 → **28** |
| bodies found | 0 → 0 | 0 → 0 | 12 → 9 | 12 → **21** |
| diversity | .653 → .639 | .436 → .434 | .586 → .584 | .514 → .533 |
| alert peak | .76 → .77 | .77 → .77 | .99 → .99 | 1.08 → 1.15 |
| reinforcements | 19 → 21 | 29 → 33 | 54 → **47** | 60 → **82** |

**Both pre-registered criteria are now clean.** No striking profile's win rate jumps —
`aggressive` +0.7 points, `careless` **exactly flat** — so the first does not fire, on
its surface or in its reason. Diversity moves −.014 to +.019, so the second does not
fire either.

**The free safety is gone, and the option is not.** The profiles that never strike are
back to baseline: `balanced` +2.0 points with **zero** takedowns, and its detections
actually rise. Meanwhile the striking ones take the new play — `careless` doubles its
takedowns (13 → 28) and its bodies found (12 → 21), pays for it in reinforcements
(60 → 82), and **wins exactly as often as before**. That is the shape an option is
supposed to have: it changes how a run is played without changing how often it is won.

`cautious` keeps +4.0 points, on zero takedowns and *more* detections than baseline.
That is the reward landing where it was aimed — it is the temperament built to read and
avoid patrols, and reading a patrol is what the rule pays for.

**Why the condition is the whole design and not a tuning fudge.** It says: a patrol you
have read is predictable, and a guard that is hunting you is not. The flank becomes
somewhere to **work from** and never somewhere to **hide**. It needs no new state and no
new timer — the mood is already on the guard, and the cone is already recomputed every
sight phase, so a guard's sides come back the turn it stops being calm. Because the
§11.5 overlay is drawn from the same one cone, a patrol and a searcher standing side by
side paint differently: the rule is legible on screen rather than remembered.

**Interaction with #430** (a guard cannot act on the turn it first spots you). The two
compose in the same direction and neither needs the other: #430 makes a first sighting
cost the guard its turn, and this makes a calm guard's flank not a sighting at all. Both
say the same thing — a patrol you have read is a patrol you can act against.

**Still a knob, still off.** Adopting it is a design-doc edit to §6.1, §6.2 and §7.2,
which is a human judgement about *feel* (§13.4) — tailing a guard through a corner is
not something these metrics can score. The numbers no longer argue against it.

### Adopted (#442)

That feel call was made, and the answer was yes: **a Calm guard detects exactly its
cone** is now the rule, and §6.1, §6.2 and §7.2 moved together to say so. Nothing above
was re-measured — the conditional table is what it was, and it is what the adoption
rests on. What changed is only that the knob stopped being a knob.

Three consequences worth recording, because each is the kind of thing a later reader
would otherwise have to rediscover:

- **The modifier is retired in place, not deleted.** Slot 5 is frozen: it still
  round-trips through `modifier_slots` so every token ever shared still decodes, and
  nothing reads it. A token minted with the bit set now plays the identical game,
  because what it asked for is the rule. Closing the gap instead would have re-pointed
  every older token at a different modifier — the appendix 26 (#286) break, with nothing
  to notice it by.
- **`BlindPolicy::Rear` survives as the control.** The old rule is no longer reachable
  from any level's config, but it is kept as the named arm the unit tests compare
  against: "what a flank used to do" is the contrast that makes the §6.2 tier ladder
  legible. Restoring it as a *harder* modifier would be a new slot appended to the end.
- **The seeded Calm-patrol pin moved, and had to.** A guard's cone feeds its `inspected`
  memory (§7.5), and patrol targets are chosen as the farthest cell *outside* it — so a
  narrower Calm cone inspects less per turn and the sweep walks a different route. No
  patrol logic changed. The pin catching this is the pin working.

**What it composes with.** #430 landed alongside: a guard cannot act on the turn it
first spots you. The two say the same thing from opposite ends — a first sighting costs
the guard its turn, and a calm guard's flank is not a sighting at all — so *a patrol you
have read is a patrol you can act against*. #430's rule that a mood change re-aims the
cone stops being defensive here and becomes load-bearing: a guard's flanks going live on
the turn it spots you is precisely that seam, and without it the §11.5 overlay would
paint a patrol's blind flanks under a Danger glyph.

---

## Appendix 29 — Why the difficulty draw leaves the intel gate alone

*(§12.6, #297/#298. Decided in the ticket, deliberately without evidence — this
records the shape of the choice so the next person does not have to rediscover it.)*

The −2…+2 difficulty axis draws `|level|` modifiers from the §12.6 pool in the sign's
direction. That works cleanly on the harder side, which had three candidates when the
axis landed: +1 is a genuine draw of one, +2 of two, and the resolved set differs by
seed. The easier side had **two**, so −2 is exhaustive — every seed resolves to the
same pair — and −1 is a coin flip between them.

**The obvious third easier candidate is the intel gate**, and it is not taken. Quick
play sets `IntelGate::All` (gather everything, then leave, §10.2); relaxing it to
`AtLeastOne` would be a real, large easing. Two things stood against it.

The first is mechanical. `LevelModifiers::union` is the one composition rule §12.6
gives, and it composes a bounded knob **harder-ward** so that sources add pressure and
never cancel each other — which is exactly what keeps the campaign alert (#210) from
being undone by a choice the player made. An easier draw could only relax the gate by
learning to **replace** a knob rather than compose with it, and that second rule would
then need a story about what happens when the alert source and the difficulty draw
disagree about the same knob. That is a real design question, not a line of code.

The second is about what a difficulty control is allowed to mean. Every toggle in the
pool bends a *rule*: how guards search, what they call in, how much of the building you
can see. The gate is the run's **objective**. A slider that quietly turned "take all the
intel and get out" into "grab one and go" would be changing what quick play *is*, not
how hard it is — and it would do so without saying which of the two games the player
was about to be handed, since the dialog names the difficulty rather than previewing
the draw.

**So the easier side stays thin, and the cost is stated rather than hidden.** The
slider's blurb counts what a level will *actually* bend, off the live pool size rather
than off the level, so −2 cannot claim two rules when only one exists to bend; the
directional assertion holds regardless, because a draw filtered on `ModifierDirection`
can never hand back a set bending the way it was not asked for.

**What would change the answer.** A third easier modifier — anything that gives the
player knowledge or slack without touching the objective — makes −1 and −2 properly
distinct and closes this on its own; that is the expected path, and it is why the pool
is a filter over the fields' own directions rather than a hand-kept list, so a new
modifier joins by declaring its direction and nothing else. The knob-replacement rule
is the other path, and it should be taken deliberately, with an answer for the alert
source, rather than as a side effect of wanting a third candidate.

---

## Appendix 30 — Where a knob's baseline sits decides whether the pool can reach it

*(§12.6, §10.2, #232. Closes the question appendix 29 left open, and records the
composition rule that made it closeable.)*

The guard-count modifier is a small feature — ±1 guard around §10.2's four — and it
was expected to be a straightforward consumer of the #225 seam. Three things about it
were not straightforward, and each is a general rule the next knob will meet.

### 1. Appendix 29's objection does not apply — and not because guards are special

Appendix 29 kept the **intel gate** out of the directed pool on a mechanical ground:
`LevelModifiers::union` composes a bounded knob **harder-ward** so sources add pressure
and never cancel, and quick play already sets the gate to its hardest value — so an
easier draw could only relax it by learning to **replace** a knob rather than compose
with it, which then needs a story about the campaign alert (#210) disagreeing with the
player's choice.

The guard count is a knob too, and it is in the pool. The difference is **where its
baseline sits**. The gate is ordered along one exposure axis with the base already at
the far end; the guard count's baseline is a **neutral middle** with a departure on
each side. Compose an easier pick onto a base that asked for *nothing*, and there is
nothing to relax — the pick simply stands.

That is the general rule, and it is worth stating in the abstract because it is not
about guards at all:

> **A pool can draw either end of a bounded knob when the base rests at its baseline.
> It cannot relax a knob the base has already turned.** What decides it is the
> baseline's position, not whether the field is a `bool` or an enum.

So the pool's exclusion list shrinks from "knobs" to "knobs the base has turned", which
today is exactly the intel gate, for exactly appendix 29's second reason as well: the
gate is the run's *objective*, and a difficulty slider that silently swapped the
objective would be changing what quick play **is**.

### 2. Harder-ward composition needed a definition for a symmetric knob

`IntelGate::harder_of` is `max` over an exposure rank. Applying `max` to a knob with a
middle baseline gets it **wrong**: `Baseline.max(Fewer)` is `Baseline`, so a source
that asked for *nothing* would overrule a source that asked for fewer guards. That is
not "sources add pressure"; it is a quiet source being counted as an objecting one.

The rule adopted instead — **the end that departs from the baseline wins, and pressure
breaks a tie**:

| | `Fewer` | `Baseline` | `More` |
|---|---|---|---|
| **`Fewer`** | `Fewer` | `Fewer` | `More` |
| **`Baseline`** | `Fewer` | `Baseline` | `More` |
| **`More`** | `More` | `More` | `More` |

It keeps the invariant §12.6 actually cares about — **no contribution can relieve
pressure another one asked for**, so the alert cannot be talked out of its extra guard
— while letting the only-source-that-spoke be heard. It is commutative, the baseline is
its identity, and it collapses to `max` for any knob whose baseline is an end, which is
why the gate's rule did not have to change.

### 3. Reaching generation is not one cost — carve depth and placement depth differ

§12.6 warned that a generation-time modifier breaks **seed stability** (#452's worked
example: dropping a per-doorway draw re-carved every shared `#seed=N`). The guard count
is read before generation too, so the same warning looked like it applied.

It does not, and the distinction is worth keeping: `automatic_doors` reaches the
**carve**, while the guard count reaches **placement**, which runs after the carve is
finished and validated. Placement draws its pieces from one stream in a fixed order and
takes the guards as the first *N* of a **single shuffled pool**, so from one seed the
three settings give:

- the same carve, cell for cell;
- the same player, exit and intel — every draw before the guards is untouched;
- **nested** guard sets: `Fewer` is the baseline's guards minus its last, `More` is
  them plus one.

That last property is worth more than it cost. §2.3 asks every modifier for a
directional assertion, and a carve-depth modifier can only manage a *distributional*
one (the sim's temperaments over a seed sweep, both arms). A placement-depth modifier
gets an **exact, per-seed** one: on one facility, more guards watch a strict superset
of the cells fewer guards watch. The sim then agrees with it in the aggregate — 43% /
35% / 25% bare bot win rate over 300 seeds at three, four and five guards — which is
appendix 26's own curve, re-measured through the modifier seam rather than through
`--guards`.

What *does* shift is everything drawn after the guards: the comms console comes from a
pool the guards are excluded from, and each guard draws a radio clock. So the arms are
the same **building** played out differently, not the same **board** — and since the
knob sits in the difficulty pool, that narrows §12.6's stated "byte-identical at every
difficulty" to the carve. Both are now said in the design doc rather than left to be
found.

### The format footnote

A bounded knob does **not** need a field in the level-seed token. The guard count
spends **one slot per end** (7 and 8), which changes no radix and leaves every token
ever shared decoding to the run it always named — where a third radix-3 field beside
the intel gate's would have been a format version bump. A set naming both ends is
refused on decode, since the encoder cannot produce one. The rule for the next knob:
**if its values can be spelled as slots, spell them as slots.**

---

## Appendix 31 — Quick play is training; the campaign is the run

*(§2.2, §10.2, §14, #138. Decided while building the end screen — the first surface
that had to answer "and now what?", and therefore the first that could get permadeath
wrong.)*

The end screen (§14 v2) carries the run's exits, and the obvious set is *retry this
level*, *new run*, *back to menu*. **Retry is the problem.** §2.2 marks permadeath
**[SETTLED]** without qualification — "you are captured, you lose, and the next run
starts exactly where the last one started" — and a button that hands the same facility
back, from turn one, with the same loadout, is the plainest possible contradiction of
it. Ship the screen without an answer and the pillar is quietly gone, not by a decision
anyone took but by a button that seemed obviously right.

**The answer is that the two things are different games.** The run — the thing
permadeath is a promise about — is the **campaign** (§2.2: 2–3 hours, progression
throughout, nothing carried to the next one). **Quick play is training.** It is one
tuned facility (§10.2), and what a player does with it is learn: how corridors carry
sight, how a patrol beats, what a takedown really costs. Learning a building means
walking it more than once. A training mode you may not replay teaches a fraction of
what it could, and refusing the retry there buys the pillar nothing, because there was
no 2.5-hour run to protect.

Two consequences, both worth writing down rather than leaving to be found.

**The exits belong to the mode, not to the screen.** §14 v3 does not exist, so the
temptation is to put the three buttons on the screen and gate them later. That is the
version that fails: the campaign's end screen will be *this* screen, and a screen with
a retry button on it inherits the retry. So the gate is a `RunMode` the run carries and
one function on it — `RunMode::exits` — that says what a mode may offer. A new mode has
to answer there. A test pins that the campaign offers neither retry nor new run, today,
years before there is a campaign to run it.

**While v1 ships quick play only, the shipped game has no permadeath in force.** That
reads worse than it is: it is the same fact as "v1 ships a training mode", said in the
pillar's vocabulary. The alternative — shipping the training mode with permadeath
enforced, to be seen to honour a pillar about a mode that does not exist — would make
v1 worse at the one job it has (§14: *is the hiding game fun?*), because the answer
comes from players who replayed a seed until they read it. It is written here, in §2.2
and in §10.2, so that "the game has no permadeath" is never discovered as a bug.

**What would change the answer.** Nothing about quick play. The thing to watch is the
campaign: if its end screen ever grows a way to play the run again — an "undo the last
hour" mercy, the prison level of appendix 3 taken too far — that is a change to §2.2
itself and belongs there, not in a mode's exit list.

---

## Appendix 32 — The way in is a real duct: play the entrance, don't watch it

*(§1, §4.5, §5, §10.6, §10.7, §11.4, §11.5a, #466 — closing #468, the animation this
replaced. Decided while building the diegetic entrance.)*

**The problem was legibility, and it was real.** Turn one on a 40×40 board draws forty
rows of fog and one `@`, and nothing points the eye at where you are. §1 has always said
*"you dug the tunnel and came up through it"*, but the tunnel was narration attached to a
single solid cell: `E` was a tile in a room, the player materialised beside it, and the
fiction was a sentence in a design document rather than anything the game did.

**The first answer was an animation** (#468): open on the exit, hold, pan or fade to the
player. It was closed unmerged. It cost a frame clock, a timeline and a skip control —
none of which the game has, all of which would have to be maintained — and it bought a
beat the player *watches*. Worse, it would have been five seconds of held control at the
one moment the player is most impatient to act, and a second time through it is pure
tax.

**The answer that shipped is that the tunnel is terrain.** §10.7 already had everything
needed and needed no new vocabulary: a duct is a path of cells with a mouth-bearing
entry, its crawler is concealed and contact-safe, its information is degraded to memory
plus a shortened sense, and it has exactly one live window — the entry cell's auto-peek
out of its mouth (§6.1). So `E` becomes the inner mouth of a **linear** duct running to
the level border, and the run starts inside it, on the border cell.

Everything the animation was for falls out of that, and is *played*:

- **The eye is pointed.** §10.7 draws the whole occupied run as one connected `=`, so
  turn one opens on a bright line from the border to the mouth you are about to climb
  out of. Nothing was added to the renderer to get this; the rule was already there —
  only the *colour* is the tunnel's own (Interest, the exit's band, where a found
  shortcut stays System), so the line and the letter it ends in read as one thing.
- **The row says where you are.** Turn one has no action behind it and so no message, so
  the §11.4 ambient floor gained the arm it was missing: *"your own tunnel — crawl out"*,
  standing for the length of the crawl, next to the cupboard's and the crouch's. A
  crawlspace is a state you are in, and the floor is where a standing fact belongs.
- **The opening is the player's own inputs.** Three or four crawl steps and a climb-out
  — real turns, so the facility is already moving when you arrive and you arrive knowing
  it, rather than materialising into a frozen tableau.
- **The first decision is a decision.** From the mouth, the peek reads the room; you
  climb out now, or hold on the entry cell and look again. The old opening handed the
  player a free 360° frame of the room they were standing in — a *fact* where this is a
  choice with a cost.

**It deleted a rule rather than adding one.** §5's wait's-look opening (#383) existed
precisely because the player materialised standing in a room and had to be shown it. With
a diegetic entrance nobody materialises, so the exemption had nothing left to paper over
and came out — implementation, tests and all. That is the §2.3 debt rule paying out in
the right direction: the new mechanic *removed* a special case.

**The fog needed no exception, which was the worry.** §10.7 hides a duct's interior
completely — no tell on the base map, not remembered once you climb out — and the fear
was that this would hide the player's own tunnel. It does not, because you **start
inside it**: the occupied-run rule draws it while you crawl, and `E` itself is drawn as
itself from turn one (§11.5a, *"Yours."*), so once you climb out what is hidden is only
the crawl behind the mouth — the same secret every other duct keeps. Nothing about
§10.7 or §11.5a changed.

**Three consequences worth naming, because each is a real change and not a detail.**

1. **The win check moved off the grid; the gate stayed at the mouth.** The *win* is no
   longer a cell you bump — it is a step **off the board** from the tunnel's border cell,
   and that is the first affordance whose arrow points off the grid (§11.4/#384). But the
   **intel gate** is answered at `E`, where it always was: bumping the mouth short of it
   refuses, free, with the same words. Splitting them that way is deliberate and was the
   first thing playtesting corrected. A gate at the far end only is *technically* the same
   rule and a worse game: the player is told no after four turns of crawling, in a
   crawlspace where the answer is useless to them, and the row that promised
   `exit: enter` turns out to have promised a dead end. Refusing at the mouth keeps the
   refusal where the player can still do something about it, and keeps the §11.4 rule that
   the row never says a bump will do what it will not. `E` with the gate met means *climb
   in*, so it gets its own label (`exit: enter`): two bumps that behave identically and
   mean completely different things must not read identically on the one row that says
   what a bump does. The **row** goes one step further and says nothing about the way out
   until the player has been inside: the run opens standing on that very cell, and an
   opening line reading *leave* points at the end of a run that has not started. The press
   still answers — the gate is a rule, the row is a prediction, and holding a prediction
   back is not the same as lying in one (§11.4's own FOV gate is the precedent).
2. **`PLAYER_EXIT_MIN_DISTANCE` retired.** Eight cells between the spawn and the exit
   existed so that no run started won. There is no spawn-to-exit distance any more — the
   player starts *at* the way out — so the hazard is re-closed as a floor on the
   **tunnel's own length** (`EXIT_DUCT_MIN_CELLS` = 8, capped at 16 because every cell of
   it is a turn the guards get). It shipped at 4–12 and the first playtest moved it: four
   cells reads as a hole in the skirting rather than something you dug, and the opening
   was over before it had said where you were. The pair is the knob the whole entrance
   hangs on, and it is the number to watch first. Placement's other spacing rules re-anchor on `E`: the
   comms detour is measured from it, and the turn-one guard-cone rule protects the mouth
   the player comes up out of rather than a cell they no longer stand on.
3. **You can dive into your own tunnel to break contact.** It is a duct, so it conceals
   and it is contact-safe, and nothing forbids climbing in mid-chase. That is a genuinely
   new escape and it is left in deliberately: it costs the walk back to `E`, it is the
   one place a pursuer knows to look, and coming out means coming out of the same mouth
   you went in by. If the sim ever shows it dominant, the fix is §10.7's own — add cost —
   not a rule saying your own tunnel is different from every other one.

**Two things the placement rules had to learn, both found by the bot timing out.** A
mouth must open onto the **building**: a candidate `E` whose only walkable neighbours are
its own tunnel path (the interior may overlie floor, §10.7) or a single cupboard leaves
the player sealed in a crawlspace on turn four, and the seed is now redrawn. And a
console must never be stamped on a duct cell, the exit tunnel's or a §10.7 shortcut's —
which nothing had ever checked, because before this ticket no crawl route was chosen
after placement had run.

**What would change the answer.** How long the opening takes is the number to watch: the
tunnel length is a `[START]`, and if the crawl reads as a chore the cap comes down before
anything else does. If the *entrance* turns out to feel worse than materialising — which
is the only thing the animation was ever competing for — the thing to revisit is this
appendix, not #468's branch.

---

## Appendix 33 — The sense is one channel: what fades, and how short the trail has to be

*(§9.2, §9.4, §9.5, §11.2, §11.5, #192 — settling the [OPEN] note §9.4 carried since
#188. Decided while unifying the two halves of the sense.)*

**The problem was that one channel had two behaviours.** The door cue (#188) persisted
and faded: a door change is discrete, so it was latched the turn it happened and decayed
over `DOOR_CUE_DECAY_TURNS`. The guard sense was a hard on/off dot at the live range —
present this frame, gone the instant the guard stepped out of the box or the widened
Wait lapsed. Both painted the same orange in the same `Sensed` category, so the player
had one colour meaning two different things: *a live dot here, a fading mark there*.

**What fades — a trail, or a ghost?** The ticket offered them as alternatives. They fall
out of the same rule, so the answer is both, and the rule is the deliverable:

> Every turn, the sense stamps a mark where it felt something. Marks fade.

Stamp each sensed guard's cell every turn and a moving guard leaves a **trail**; let the
stamps outlive the perception by a turn or two and a guard that leaves the box leaves a
**ghost** of its last known cell. One `record_*`/`decay_*` pair, the door cue's own
machinery generalised, and the door cue becomes a third thing the same rule produces.
Picking one of the two would have meant writing a *second* rule to exclude the other.

**Which half is actually new information.** The trail is not, and this is the whole
answer to "does this change what the player can exploit?". A guard continuously inside
the box was already legible frame to frame — the dot was there, now it is here — so the
trail only spares the player the memory work. The **ghost** is the genuinely new fact:
today the dot blinks out and the player is told nothing; with the ghost, "I have lost it,
and it was just there" is on the board. That is worth having, it is bounded by the same
short fade, and it is *less* information than the live dot it replaces, never more.

**The arrow, and why two turns.** The one thing this feature could break is §9.2's bound
— position, never intent. A long trail is an **arrow**: hand over four cells in a line
and the player extrapolates the next four, which is heading, which the design withholds
on purpose. `GUARD_CUE_DECAY_TURNS` is therefore **2** [START] — the shortest life in the
channel, and shorter than the door cue's 3, so at most the live cell and the one behind
it are lit at once. The asymmetry has a reason beyond caution: a door change gets **one**
chance to be read and never restates itself, where a guard mark is re-stamped every turn
the guard is still felt, so the guard cue only ever has to carry the tail.

Two properties make the bound self-enforcing rather than a number we hope holds:

- **A standing guard leaves no trail.** It re-stamps the same cell, so the mark never
  ages. The watcher whose facing you would most like to know is exactly the one the
  channel says nothing extra about — which is the opposite of what an arrow would do.
- **A seen guard stamps nothing.** Sight already draws it whole; a trace underneath
  would be the sense restating sight in the one colour that means *not seen*.

**The ramp is two steps, and they mean age.** `Sensed` had ignored the fill entirely and
painted its bright background regardless of visibility, on the honest ground that a
sensed cell is certain knowledge and the fog has nothing to say about it. That is still
true — everything sensed is outside the FOV by construction — so the two fills every
palette row already carries were free to mean something else here: **full for a mark made
this turn, quiet for the fading tail**. No new palette shades, no third `Fill` variant,
and the core still emits an age and a category and never a colour. A graduated three- or
four-step ramp was rejected for the reason the trail is short: more steps are only
readable on a longer trail, and a longer trail is the arrow.

**One precedence question had to be reopened.** The sense channel used to paint *above*
the watcher line (#465), which is right for the live dot — the line's own endpoint is
that guard, and the orange position mark should survive the red line running up to it.
It is wrong for a **fading** mark: a trace of where something was a turn ago must never
cover a line that says a guard has you *right now*. So the fading marks moved below the
line and the live dot stayed above it. The danger overlay still paints last (§11.5,
being seen outranks) and nothing about that changed.

**What would change the answer.** The decay is the knob, and it is the only one that can
turn the trail into an arrow — which is why it is pinned by a test rather than left to
drift. If play says the trail is too faint to read, the honest first move is the *ghost*
half (a guard leaving the box lingering longer) rather than a longer tail behind a live
dot the player can already see.
