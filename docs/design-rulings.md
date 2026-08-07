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
> draws as a schematic (`□` fabric, blank floor space) rather than as a fourth rung on the §11.5
> dimming ladder*, *why a doorway is a gap and its frame `□`*, and the A/B against a denser `▒` mark
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

## Appendix 33 — The dot is the field of view's own ink

*(§11.5, §11.5a; `docs/render-reference.md` §2.2–§2.3, §3, §4.1/§4.3. Ticket #470.)*

Two changes that only work together: **floor is dotted only inside the FOV**, and the
schematic's fabric mark moved from `≈` to **`□`**.

### This is not appendix 1's fix #2 coming undone

The render reference used to state fix #2 flatly — *floor draws as a dot rather than a
blank, because a blank cell has no foreground and the dimming that encodes the sight
boundary was invisible across open ground* — and a reader who finds that sentence in
the annex must not read what follows as the bug returning.

The **goal** of fix #2 is the sight boundary being legible across open floor. That goal
is kept. What changed is the mechanism, for a stronger one: instead of two shades of
dot either side of the boundary, there are **dots on one side and nothing on the
other**. The edge of vision becomes a hard edge rather than a shade difference — which
is what fix #2 wanted and had to approximate with a luminance step, on a row that is
deliberately the quietest on the board and therefore had the least room to make that
step with. A board with no dots anywhere is what fix #2 was written against, and that
board is not what this produces: inside your sight, every floor cell still carries one.

### Why `≈` had to go, and why `□`

`≈` was chosen as the mathematical *approximately* — exactly the claim a plan makes
about a stretch of building nobody has walked. Good reasoning, wrong glyph in practice:
at the cell sizes the board is fitted to (§11.4 puts the whole level on screen, so the
cells are small), a double tilde reads as an **equals sign**, and `=` is the duct mouth.
The mark for *unseen fabric* looked like a specific piece of terrain — and an unseen
duct mouth is *itself* fabric, so the confusion landed exactly where it did most damage.

`□` was picked for the cell it fills. Fabric fills its cell the way `#` does, so a wall
run on the plan reads as **structure**; a baseline-hugging mark drew the same run as a
dotted line, which is the wrong reading for the load-bearing half of a plan. It carries
roughly a third of `#`'s ink, so the plan stays quieter than the building, and an
outline square *is* the claim: the shape of a wall, without the substance of one you
have seen. `░` (light shade) is the traditional roguelike answer and reads heavier; it
was passed over because the shell scales cells by device pixel ratio, so a dither is
resampled at arbitrary fractional sizes and shimmers where an outline stays clean. It
stays the fallback if a heavier read is ever wanted — checked at several window sizes
and DPRs first.

**The two changes depend on each other.** Freeing the cell of floor dots is what lets
the schematic carry its whole message in one channel: afterwards the plan is **one
shape and one absence**, fabric drawn and space blank, and `~` leaves the game with
`≈`.

### The consequences, taken deliberately

- **Explored and unexplored floor are now the same blank.** The distinction does not
  disappear; it moves entirely into the fabric channel, where explored geometry reads
  `#`/`×`/`}` and unexplored reads `□`. A room you cleared still reads as cleared —
  by its geometry, not by its floor. The knowledge-state table (`render-reference.md`
  §3) calls floor out as the exception: drawn only when Live.
- **Ground's dim shade became dead ink.** Floor was the only Ground glyph ever drawn
  outside the FOV — an open door panel was already blank — so nothing is painted in it
  now. The palette test asserting *"its live and dim shades stay far enough apart that
  the sight boundary reads across open floor"* was guarding a property nothing depends
  on, and that half of the test was removed rather than left standing as a claim about
  a mechanism the game had stopped using. The bullet that Ground **recedes beneath
  every other category** is untouched and still live: the dots must whisper. The dim
  *value* stays in the table, like `Effect`'s, because a row with a hole in it is worse
  than a row whose value is currently unused.
- **Backgrounds are unaffected**, and that is what makes the change cheap. They paint
  per cell regardless of glyph, so the §11.5 danger overlay, the Sensed cue and the
  effect layer are identical — an out-of-FOV watched cell still paints red, now without
  a dot on top of it.
- **Contents in memory are untouched**: `}`, `=`, `$`, `Ψ` keep the memory slate.

### What would change the answer

A board with fewer marks on it is a quieter board, and quiet is the point (§11.5 — the
overlay is what matters). The thing to watch in play is the other end of it: whether an
unexplored wing goes so empty that it reads as *nothing there* rather than *not been
there*. The fabric glyph is now carrying that whole message alone, so if the plan stops
registering, the lever is the glyph's weight — `░`, or `▒` with a dim of its own
(`render-reference.md` §2.4) — and not putting the floor marks back.

---

## Appendix 34 — The sense is one channel: what fades, and how short the trail has to be

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

---

## Appendix 35 — Debug mode ships hidden in every build

*(§12.6; `crates/web/src/debug.rs`, `crates/core/src/render/help.rs`. Ticket #459.)*

This appendix exists because the decision it records **reverses a stated one**, and the
reversed decision was right for the reasons it gave. Both halves need to survive
together, or the next reader will re-argue whichever half is missing.

### What was decided before, and why it was right

`DebugModifiers` sits beside the level modifiers *as the contrast* (§12.6): a level is
shared, and everything in it is part of what the run **is**; a debug switch is the
opposite of shared. So the two got separate carriers, and the debug one was
deliberately narrow — a `window.__intrusionDebug` global that only a build could stamp.
The module said so in as many words: *there is deliberately no URL form and no in-game
surface — a `?debug=` parameter would make the fog liftable by anyone who read a link,
which is exactly what the split exists to prevent.*

That argument is sound. A link is the one thing that travels between people, and a
level travelling with the fog pre-lifted would be a level that plays a different game
than the one its sender played.

### What was wrong with it anyway

**A deployed build you cannot inspect is a build you cannot debug.** Every switch was
reachable only by rebuilding — which is available for an artifact preview and not at
all for the Pages deploy. So the loop for "a run on the live page did something
strange" was: fail to reproduce it, rebuild with `--debug reveal`, roll a different
run, and look at that instead. The switch was in the wrong place to be used at the one
moment it was wanted.

### The reversal, and the three things that keep it honest

Debug mode is now **compiled into every build, hidden rather than stripped**: on by
default in artifact builds, and activatable anywhere with `?debug=intruded`. It puts a
fourth **Debug** tab on the help panel carrying the omni-vision switch (now flippable
mid-run) and the replay export.

The residual risk — that the activation, once done, rides along in a link — is designed
against rather than accepted:

1. **The value is a shibboleth.** `intruded`, not `1`. Nobody arrives at it by reading
   a link or trying the obvious parameter; you have to have been told.
2. **The parameter is stripped the moment it is consumed** (`history.replaceState`).
   This is the load-bearing one. `seed.rs` reflects the live run into the URL hash to
   keep the address bar shareable, and a query parameter survives that untouched — so
   without the strip, "copy the URL, send it to someone" would hand over the reveal
   along with the run, which is *precisely* the failure the split was built to stop.
   With it, activation is a thing you **do**, not a thing the page carries.
3. **Nothing behind the gate may touch the facility.** The gate is a convention, not a
   mechanism: the string is in the shipped wasm for anyone who reads it. That is
   acceptable for a switch that only alters what one person sees, and it is exactly why
   the §12.6 line has to stay sharp. A future switch that would change the *game* does
   not belong on this tab; it is a level modifier and belongs in the token.

Neither the level-seed token nor the replay link may ever encode debug state, and
`State::with_debug` stays documented as not part of the level. The omni-vision toggle
is safe to expose at runtime for the same reason the baked switch was safe: it is
applied in the sight phase and read by no rule, which the tests assert (a run flipped
mid-way replays identically to one never flipped) rather than assume.

### Two consequences taken deliberately

- **`replay [r]` left the Level info tab.** Exporting a run is a debugging affordance;
  the Level info tab describes the run's rules, and its `copy [c]` — the level, for
  sharing — is what belongs there. The key moved with the control, and
  `help_nav_for_key` stops offering it where the tab is absent: a key that silently
  does nothing teaches a control that is not on screen to contradict it.
- **The tab bar had to give up a word.** Four `[Label]`s plus the `[x]` do not fit 40
  columns (§10.2), so *Level info* became **`[Level]`** on the bar — the tab's own
  heading already says `THIS RUN`, which made the extra word the most expendable five
  cells on the panel. The bar's fit is now a **compile-time** check over the whole tab
  vocabulary, so the fifth tab (§14 v2's options) fails the build rather than arriving
  half-drawn over the close control.

### Follow-up: the recorder ships everywhere (#478)

The tab shipped with `replay [r]` still behind the `debug-tools` cargo feature it had
carried since #411, which `pages.yml` does not build with — so the first deployed debug
session could lift the fog and could not hand over the run, which is the more useful
half. The feature was answering *"was this build made for previewing?"* when the live
question had become *"is this a debug session?"*, and the tab above already answers
that. Turning it on for the deploy would have left an axis with one setting in every
build shipped, so the feature is gone instead: every build records, and
`ScreenUi::offer_replay_copy` collapsed into `debug_mode` — one flag, one question.

It widens what the shibboleth unlocks, so it is worth restating that the rule holds:
exporting a replay **reads** the run and changes nothing in it, and the link is the
level's token plus the input script, carrying no debug state. The cost is a small
`Copy` enum pushed per turn in every session — a couple of thousand of them across a
long run, tens of kilobytes — for a strange run being reproducible by whoever it
happened to.

## Appendix 36 — An existence test pins its witness; the search is CI's

*(§13.2/§13.4; `crates/sim/src/test_support.rs`, `crates/sim/src/bot/tests.rs`.)*

The sim's tests are the only ones in the workspace that pay for **whole runs**. A run
costs ~30 ms to carve its facility and ~100 ms more to walk to an ending, and the bot
tests walk between 40 and 400 of them each. Measured on a 4-core runner: 88 sim tests
took **94 s** against 944 core tests in **14 s**, and ten tests accounted for 75% of it.

The cost was not the assertions. It was **searching for the thing being asserted
about**. A cue like the §7.7 comms bump only fires on a seed whose layout walks the bot
past a console — roughly one seed in fifty — so the test swept until it found one. Every
gate run, on every machine, re-derived the same seed number.

### The three shapes, and what each one gets

- **Existence** — "walk runs until the bot bores a wall, and assert every bore opens a
  route". **Pin one seed** on which the verb fires, in a `const WITNESS` beside the
  test, and walk that alone. `witness_sweep(WITNESS, 0..40)` returns the witness by
  default and the whole range under `INTRUSION_SLOW_TESTS`, so CI still checks the
  universal half at its original width.
- **Negative** — "`careless` never crouches". There is nothing to pin; that is what
  makes it a negative. `negative_sweep(0..60)` samples 8 spread seeds locally and the
  full range in CI — core's #60 bargain, unchanged.
- **Statistical** — "over a batch the outcomes are mixed", "fewer than 10% of dispatches
  stand down". These genuinely need the batch. They do not get a witness; they get
  `profile_batch`, which walks **one** batch per temperament for the whole test binary.

### Why a pinned seed, when #387 rejected a pinned window

#442's comment in `every_profile_ducks_behind_a_bench` argued the opposite case, and it
was right about the thing it was arguing against. A hand-picked **window** (`100..170`,
chosen because a duck happened to land inside it) fails *dishonestly*: a generation
change moves every seed's geometry, the window empties, and the red test reads as "§10.3
went inert" when the behaviour is intact. The fix then was to widen the range and search
it — correct, and it cost 43 s on every gate run.

A **witness** is not a window. It is one seed, named, with the verb it witnesses written
beside it, and a failure message that says what to do: *sweep with
`INTRUSION_SLOW_TESTS=1`, take a seed that still does it, pin that one*. The distinction
that matters is where the cost of re-finding lands. A window makes everyone pay a search
forever to protect against a change that may never come; a witness makes **the change
that moved generation** pay it, once, in the PR that moved it. That is the same trade
the repo already makes everywhere else: the expensive check runs in CI, the local gate
runs the cheap one, and a stale pin fails loudly rather than quietly widening.

The dishonest-failure worry survives intact, and is answered by the message rather than
by the search. `stale_witness` spells it once, so no red line ever reads as a bare "the
bot never bored a wall" and invites the next person to widen the sweep again.

### What it bought

Sim suite wall-clock **94 s → 50 s** on a 4-core runner, with no assertion weakened:
every universal claim keeps its full range under `INTRUSION_SLOW_TESTS`, which is what
CI runs. Three parts, measured separately:

- **The witnesses.** The searching tests collapse to one run each: the comms bump went
  from hunting up to 400 seeds to walking seed 3, the duck sweep from up to 400 per
  temperament to one, the ability-cue sweeps from 40 seeds to one.
- **`boot` memoised** — worth ~7 s on its own (93.6 → 86.7 measured in isolation).
  Cloning a booted `State` costs 25 µs against 30 ms to carve one.
- **`profile_batch`** — five tests were walking their own batches of the same four
  temperaments: 760 runs between them, now 240 walked once and shared.

**What is left is the shared batch**, and deliberately so. Those 240 runs are ~25 s of
the remaining 50, and they are the floor for a reason: the claims that read them are
*statistical* — a temperament's outcomes are mixed, `aggressive` stows some bodies,
`careless` gets some found — and a narrower batch would make them flap rather than fail.
Width is what those assertions are made of, so it stays; what the fixture removed was
paying for that width five times over.

Two tests keep a full sweep on purpose. `a_swept_alert_threshold_moves_the_measured_ladder`
compares two ladders over 40 seeds and its own comment records that 12 was too narrow to
see the difference — narrowing it again would re-make the mistake it documents. And
`the_bot_never_detours_to_the_comms_console` compares two policies turn by turn, where
the sweep *is* the claim rather than a hunt for an instance.

The convention is written up for the next test in `crates/sim/README.md`.

## Appendix 37 — The wall run's legend was inverted, and ten of its sixteen tiles were the same six turned

*(§11.1/§11.5a; #461, on top of #460. `scripts/seed-tileset.py`, `crates/web/src/tiles.rs`,
`web/assets/source/README.md`.)*

Two findings from putting the autotiler in front of the tile sheet #460 seeded. Neither
was visible before, because nothing had ever *drawn* the wall band: it was reserved art
waiting for a consumer, and a reserved band cannot be wrong in a way anybody notices.

### The sixteen slots hold six images

The band was seeded as one slot per neighbourhood, `16 + mask` with `N=1, E=2, S=4,
W=8`, and fifteen of the sixteen were drawn from the source run. Compared pixel by
pixel, they are **bit-identical to rotations of five images** — mean absolute
difference 0.00 over all four channels for every one of the ten duplicates. The
sixteen neighbourhoods are six rotation orbits: an isolated block, an end cap, a
corner, a straight, a T and a crossing. Four end caps are one sprite at four angles;
so are the corners and the Ts.

So the sheet stores the six and the shell turns them. A canvas quarter-turn is exact
and costs nothing measurable, the mapping stays the same arithmetic (`16 + canonical
mask`), and the ten duplicate slots become **tombstones** — allocated, listed in
`tiles.txt` as the rotation that reaches them, never drawn. Not closed up: the band is
indexed by mask, so closing a gap slides every slot after it and silently repaints
whatever referenced it, which is why an `AbilityId` slot is permanent too.

The same machinery does the actors' facing, which is why one ticket carries both: a
sprite that faces is one image at four angles, exactly as an end cap is.

**Why not go further.** Every tile in the run is the plain fill plus one boundary line
per exposed side — verified: the max-composition of the single-sided tiles reproduces
the two- and three-sided ones to within a pixel at the corners. Two images and four
draws per cell would deduplicate it completely. It stores six because **a cell must
stay one draw**: a 40×40 board painted every frame is the only thing here with a
performance budget worth respecting.

### The legend was the wrong way round

`web/assets/source/README.md` recorded the source run as indexed by *which sides the
wall continues along*, and #460's seed mapped it in that way. The art does not support
it. Source index 1 is a plain block with no line anywhere; every other index adds a
bright boundary line along the side it names.

Under the recorded reading, a plain block is a wall with **no** neighbours — so a lone
pillar would draw as featureless fill while every wall inside a corridor drew a seam
down the join, and the one tile the source lacks (all four sides) would be the *fully
surrounded interior*: the commonest cell in a facility, missing.

Under the inverted reading the lines are boundaries: a plain block is the interior of a
mass of wall, a run shows its two long edges and nothing across the joins, and the
missing tile is the isolated pillar — rare, and composable from the run. A mock render
of a hand-drawn board settles it visually: rooms come out as outlined rectangles with
solid wall between them, corners turning correctly.

Two consequences beyond the mapping:

- **The band is normalised as a group, not per tile.** Per tile, the interior block has
  no range to stretch, so the old code handed it back at full strength while every
  tile carrying a line normalised its fill down to the floor — a lone pillar brighter
  than the wall it belongs to. One range across the band keeps the fill the same grey
  in all six.
- **The `#` glyph sprite is the band's interior tile**, not a separately normalised
  copy of the same source. It is what a wall draws when nothing autotiles it, and a
  fallback in a different shade would make the wall flicker as the sheet decoded.

## Appendix 38 — A solid usable is a wall to a route, and placement has to draw like one

*(§10.3, §10.6; #477, #481.)*

An intel console, the comms console and the exit stamp in **solid** (§10.3), and unlike
a closed door panel there is no move that gets anyone past one: bumping a panel *opens
the way*, bumping a console *uses* it. So one of them dropped into a one-cell throat is
a wall, and the ground behind it belongs to nobody — guards cannot route to it, the
player cannot walk to it, and both can still see in, because a usable does not block
sight. A visible alcove nobody can ever enter.

Measured on the shipped generator over 300 quick-play seeds: **50 of them (17%)** had at
least one such pocket, 52 pockets in all, and on one seed a *guard* was placed inside
one — a patrol with two cells to pace for the whole run.

**Why every existing check was quiet.** Two blind spots, and they are different ones.
§10.6's post-placement assert proves the player's **objective route** survives the
stamping — start → every objective → the comms console → exit — and orphaned ground
holds no objective, so the flood never has a reason to visit it. It is the same hole
appendix 17 records one notch further out: that one sealed rooms *with* their objectives
inside, this one seals rooms with **nothing** inside, which is exactly why the route
check stayed silent. Meanwhile the §10.5 region graph and the guard beats cut from it
are computed on the **bare carve** — the usables are recorded by generation and stamped
later by `State::new` — so the graph the partition sees still shows plain floor where the
console will land. That ordering is why the beat-reachability property test passed over
the whole seed range while guards froze in play (#477).

**A candidate filter, not an assert-and-redraw.** Appendix 17 is the standing warning:
the one-usable rule as a *hard guarantee* rejected ~85% of carves and stalled
generation, which is why it is a preference with a fallback. An assert on the finished
board costs a redraw per bad seed; a filter at the moment of choosing costs none. So
placement draws every usable cell through the same shape `severs_pathing` already
applies to a table: **skip a cell whose stamping would disconnect the walkable graph**.
The check is the O(ring) local one — if every walkable neighbour of the candidate can
still reach every other within the 3×3 ring, any route through it has a local detour —
which is sound in the direction that matters (it never *passes* a cell that seals) and
merely conservative in the other.

Two details make it a guarantee rather than a heuristic:

- **The usables already chosen are masked solid.** Each candidate is judged on the graph
  the previous stamps left behind, so a *pair* that jointly pinches a two-cell throat is
  caught by whichever of them lands second. With the §10.6 gate proving the bare carve is
  one component, the induction over the sequence is the whole proof.
- **Both movement rules, not one.** A guard refuses a cupboard and partial cover where the
  player refuses neither, and neither rule implies the other: a detour through a cupboard
  saves the player and not the patrol, while a cupboard's single mouth (§10.1.6) is ground
  only the player could ever have lost. A pocket orphaned for guards alone is still a
  coverage hole worth refusing.

**And the assert anyway.** §10.6 is explicit that a generator must never merely *believe*
a reachability property, so the finished board is still checked one-component per rule
before a placement is accepted. The filter is what makes it free; the assert is what
makes it true rather than argued. If it ever fires, the filter has a hole — not the seed.

**What it cost.** Over the same 300 quick-play seeds, orphaned pockets went **50 → 0**
and the guard-in-a-nook case went with them (it was a consequence of orphaned ground, not
a separate bug). Generation barely noticed: **314 → 319 carve attempts** for the 300
levels (6 carve rejections either way), with placement rejections 8 → 13 — every one of
them a pool the filter narrowed, and **none** of them the finished-board assert firing.

**What this does *not* change.** `Terrain::blocks_pathing` still answers `false` for the
solid usables, and deliberately: generation asks it of a carve those cells are not stamped
into yet, and the routing predicates pair it with the move check, which is where a usable
becomes solid (`guard::routable`). §10.3's pathing column is about the finished board;
the method is about the carve, and the note on it says so. Nor does it retire #477's
guard-side route filter — the §7.5 sweep still refuses to target ground it cannot walk to,
which is the backstop for any future terrain that severs a route.

**One thing it dragged in, and it is worth naming.** Moving where the exit lands walked
a *pre-existing* sim-bot freeze onto a pinned witness seed. Climbing out of the tunnel
is a step with **no bump behind it** (§10.7 confines the crawl), so a closed door panel
beside `E` is not a way out — but the bot chose the mouth's exit by the *routing* rule,
which happily plans through a closed door because a walker opens one by bumping it
(§10.4). The press was refused for free, nothing about the mouth changed, and it pressed
again until the input cap: a whole run spent on turn fourteen. Seeds 231 and 288 hold it
on `main` and are now pinned in the bot's own anti-stall test. The lesson generalises
past the bot: **"can route through" and "can step onto" are two questions**, and a
closed panel is the one terrain where the answers differ.

---

## Appendix 39 — Watching the consoles: the cycle is the modifier, the alternation is its price

*(§7.5/§12.6; #319. `crates/core/src/guard/patrol.rs`, `crates/core/src/modifiers.rs`.)*

Baseline, a console is floor. The §7.5 sweep takes the farthest cell a guard has not
looked at, and a room holding the intel is no better watched than an empty corridor —
so *where the player must go* has no bearing on *where the guards are*, which is the
cheapest kind of stealth. `guards_watch_consoles` is the harder modifier that fixes
it: a Calm guard prefers a cell beside a console its beat touches, and cycles them.

### The thing worth measuring is not "does it prefer them", it is "does it close"

A bias is trivial to write and trivially a facade (§2.3): a guard that *sometimes*
heads for a console is a guard that already sometimes wandered past one. What makes
this a rule rather than a nudge is the **cycle** — a per-guard memory of which of its
consoles it has stood beside, preferring one it has not, wiped when the last one goes,
in deliberate imitation of §7.5's own inspected-memory wipe. Coverage then *closes*
instead of converging in expectation.

Measured over 256 generated seeds, run idle so the patrol is what is measured:

| | baseline | watched |
|---|---|---|
| consoles never stood beside within 600 turns | **a third of them** | none |
| typical console first stood beside | — | ~150 turns |
| slowest console over 256 seeds | 597 (of the ones reached at all) | **578** |

`CONSOLE_CYCLE_TURNS` is the **[START] 800** those numbers set, and it is a bound the
test holds the game to, not a budget the guard reads — nothing at runtime consults it,
which is why it is test-gated. The difference the modifier actually buys is the first
row: not "sooner", but "at all".

### Strict alternation, because the alternative is an easier level

The obvious implementation — prefer a console whenever one is unvisited — makes the
guard shuttle between consoles for the whole cycle and abandons the ground between
them. That is not a harder level; it is **two watched rooms and a free corridor
network**, which a player learns in one run. So the rule is strict alternation: one
console leg, one ordinary farthest-uninspected leg, and so on.

It is not free, and the cost is the bound above. A console cycle on a wide beat pays
*two* legs per console across a 40×40 building, plus a dwell at each arrival — which
is the whole of why the worst case is 578 turns rather than 200. The trade was taken
in that direction on purpose: the sweep's coverage of plain ground holds at **1.00×
baseline on average and never below 0.87×** over the swept seeds, and a bound that is
long but real beats a tight one bought by abandoning the corridors.

The console *within* a cycle is picked **nearest-first**, not farthest. §7.5's
farthest-first is what makes patrols pace across distances and read as purposeful, and
it is preserved exactly — on the alternating ordinary leg, which is still doing that
job. A console leg that also insisted on being farthest would double a cost the
alternation is already paying twice.

### A silenced net takes it away, and that is the design

The cycle is over the consoles a guard's **beat** touches. Killing the radio (§7.3)
leaves no partition to divide the building with: every Calm guard takes the whole
level and draws its target at random. There is then no "its consoles" left to cycle,
so the console watch goes the way the beat does.

That fell out of *where* the modifier is read rather than being bolted on. It resolves
into `PatrolStyle` — the value that already answers "how does a Calm guard choose where
to walk" — as a third variant beside `Beat` and `Wander`, at the one seam
(`State::patrol_style`) where a silenced net is already decided. So the modifier needed
no argument threaded down to `Guard::decide`, no field on the guard read at runtime, and
no second place that knows it exists. And it prices well: silencing the net already
costs a real detour (§7.3, ≥16 cells from spawn), and under this modifier it buys one
thing more.

### What the sim says, including the parts that do not support the label

120 bot seeds per profile, both arms, same facilities:

| profile | win rate | median turns to win | detections | diversity |
|---|---|---|---|---|
| balanced | 0.367 → 0.350 | 139 → 155 | 993 → 992 | 0.629 → 0.624 |
| cautious | **0.567 → 0.467** | 250 → 221 | 1213 → 1076 | 0.466 → 0.470 |
| aggressive | 0.383 → 0.408 | 151 → 156 | 870 → 794 | 0.544 → 0.543 |

The clean signal is **cautious**, and it is the one the design predicts: the profile
that lingers near an objective waiting for a gap is the profile a patrol that keeps
coming back punishes, and it loses ten points of win rate. Balanced and aggressive move
inside the noise of 120 runs (σ ≈ 4.5 points), and diversity does not move at all —
nothing collapses into one strategy.

**Detections fall in every profile, and that is not the label failing.** Guard time is
finite: legs spent walking to consoles are legs not spent sweeping the rest of the beat,
so a player who is *not* near an objective is seen less. The pressure this modifier adds
is deliberately not spread evenly over the level — it is concentrated exactly on the two
errands a run cannot skip (§4.5 take the intel, §7.3 silence the net). That is why the
§2.3 directional assertion is stated on **turns with a guard's cone on a console** — up
from 3403 to 5603 over 64 seeds, with no seed inverting — rather than on detections per
run, which measures a different thing and would have this modifier reading easier.

---

---

## Appendix 40 — Crates scale with the flavour, and what is in them is the building's business

*(§2.2/§8.3/§10.6/§14 v3; #209, on the seams #206 and #207 left. `crates/core/src/salvage.rs`,
`crates/core/src/campaign/map.rs`, `crates/core/src/place.rs`.)*

§14 v3's complaint about the old campaign is unusually specific: salvaged tech was
"fully built and reachable by nobody — no facility was ever generated with an equipment
cache, so no ability could ever be unlocked." Building the crate is not the interesting
part of fixing that. Four decisions around it were.

### How many, and who hides them

The ticket says "a facility whose flavour calls for one", and when it was written no
flavour did: #207 shipped *Outpost / Depot / Vault / Archive*, a ladder on two axes
(guards for risk, consoles for reward) with nothing about tech in it.

The first answer built was a **toggle** on a single new flavour — a Workshop hid one
crate, everything else hid none. It was rejected in review for making the tech axis a
coin flip on the map rather than a curve: a run that was never offered a Workshop never
salvaged anything, and one that was offered three got everything.

What shipped is a **count that rises with the facility's richness**: Outpost 0, Depot 1,
Workshop 2, Vault 3 **[START]**. Two consequences worth stating:

- **The ordinary road is not flat.** A Depot — the §10.2 recipe untouched in guards and
  consoles — still hides a crate, so a run that takes the plain route the whole way still
  has a power curve. What the rich flavours sell is *more* of it, not the only access to
  it.
- **The two rich flavours differ in what they charge, not in how much they give.** A
  Vault pays out in crates *and* intel and charges a guard; a Workshop pays out in crates
  alone and charges a console. So the choice between them is *which currency you are
  short of* — tech the run keeps against intel the run spends — rather than a ranking
  with one right answer. The Workshop still earns its place; it earns it as a different
  trade rather than as the only source.

The flavour cycle grew from three to four, and the §14 v3 "no two open successors ever
share a flavour" guarantee survived untouched: it rests on three consecutive lanes being
distinct modulo the cycle length, which holds for any cycle of three or more. What a
longer cycle costs is coverage — with four flavours in a three-wide window, one is
missing from any given choice point. That is the map having somewhere left to send you,
not a shortfall.

### What a crate holds: the building's business, not the intruder's

The ticket named two ways to handle a crate holding tech you already carry: avoid the
collision in the draw, or pay a consolation prize. **Neither.** The draw does not look at
the loadout at all.

A crate's contents come from the **facility seed alone**, so a building is stocked before
anybody breaks into it. Stocking it out of the intruder's pockets is the sort of thing
that is invisible in the code and loud in play: the same facility, shared as a level-seed
token, would hold different things for two people, and the world would be quietly
rearranging itself around whoever walked in.

So **a run can meet the same tech twice, and that is bad luck rather than a bug.** There
are eight pieces of tech and a run carries three; walking a long detour to a crate you
have no use for is a real loss, and the answer to it is that there are others. The bump
refuses for free and says *another Decoy — you have one*, so nothing is spent but the
walk.

The first build did avoid the collision by drawing over the unheld set. It reads well in
isolation and was rejected for what it implied: a reward economy that can never
disappoint is one where the detour is never a gamble, and the §2.3 question — *when would
a good player not take this?* — loses one of its two honest answers.

**Within one facility the crates are still all different**, because there the rule costs
nothing to keep and its absence would be absurd: the stock is a prefix of one seeded
shuffle of the catalogue, so a Vault's three crates hold three different things by
construction, with no rejection loop and no draw that can fail. It also makes the counts
**nest** — a Vault's three crates are the Workshop's two with one more behind them — so a
flavour's count reads as *how much of this building's stock you get at* rather than as a
different building.

### The held cap is kept at the crate

§8.3 settles `MAX_TECH_HELD` at three, and with a Vault hiding three crates a run meets
the cap early and often. It is enforced **at the pickup**: a bump on a crate the run has
no room for is refused, for free, and the near line names the tech being left behind.

Two rejected alternatives, both quieter and both worse. Enforcing it inside the loadout
would drop a find the player was never warned about — a rule you discover by noticing an
ability missing. Not enforcing it at all would make the §8.3 cap a comment: the ability
bar's one-row width bound (§11.4) is sized against it, and a passive's whole price is a
slot.

Refusing is deliberately the *unsatisfying* answer, and it was the honest one at the
time: the interesting version is a **swap**, and swapping needs somewhere to choose. That
is #266, and it has since shipped — a full run is now **offered** the crate's tech and
picks which of the four to drop (appendix 43). What survives of this section is the cap
itself and where it is kept; what has gone is the dead end. The two rejected alternatives
above are unchanged by it, because both are about enforcing the cap silently, and the
exchange is the loudest possible place to enforce it.

*Already carried* is the one refusal left, and it is still asked **first**: a crate
holding tech you already carry is no use to you whether or not your hands are full, and
offering a trade there would be offering a decision whose every branch is a loss. The
usable line says which of the two a bump would be (`cache: swap tech`, `cache: already
yours`) because they are different problems: one is a decision, the other is luck.

### Where they stand, and why they are still reachable

Every crate is planted at least `PLAYER_CACHE_MIN_DISTANCE` (16 **[START]**, the comms
console's own number) from the mouth the run climbs out of, and the crates of one
facility **prefer distinct rooms**. They are also solid usables, so appendix 38's rule
applies to them unchanged: a cell whose stamping would seal walkable ground off is out of
the pool, judged **per crate** rather than once for the set — the second crate has to be
weighed against a building the first is already standing in. §2.3 is the whole reason for both: a reward sat near
the way in is a free grab, and three crates in one room would be a single detour paying
out three times over — which is the price collapsing in a different direction.

And they are held to the §10.6 reachability flood, which reads at first like a
contradiction: the ticket calls a cache an *optional* reward, so why must every seed
guarantee you can reach it? Because **choosing to skip it and being unable to reach it
are not the same thing, and only the first is a decision.** A crate sealed behind geometry
is the campaign's power curve silently deleted from that facility — the failure §14 v3
records the axis having already suffered once, arrived at from a different direction. It
is the same argument that puts the comms console in the flood (§7.7: unreachable
counterplay is no counterplay), and it costs a redraw on the rare seed that trips it.

---

## Appendix 41 — The alert reaches the offers, not the run: breadth is the escalation

*(§2.2/§7.3/§12.6/§12.7/§14 v3; #210, closing the loop #107, #206 and #225 left open.
`crates/core/src/campaign/loudness.rs`, `crates/core/src/campaign.rs`,
`crates/core/src/render/campaign_map.rs`.)*

§14 v3 states the failure in one sentence — *"the whole point of the alert system is
that being loud in facility 2 makes facility 3 harder; until that loop closes, alert is
decoration"* — and the ticket that closes it proposed the obvious shape: a **persistent
campaign alert level** that rises with a raid's loudness, carries across facilities,
scales the next one, and is relieved by a slow decay and an intel sink, floored so it
cannot spiral.

What shipped is a different shape, and the difference is worth the appendix: the alert
does not accumulate at all, and what a loud raid buys is **breadth across the map**
rather than depth on one facility.

### Why not a rising level

The accumulating design has three moving parts — a contribution per raid, a decay per
hop, and a floor — and every one of them is a number nobody has played yet. Worse, they
only make sense *together*: a contribution without the right decay is a spiral, a decay
without the right contribution is a number that never leaves zero, and the floor is there
to catch the case where the first two were wrong. §2.2's fairness promise is that a
capture traces to a decision the player made, and a run losing at hour 2.5 to a facility
made hard by three raids' worth of accumulated arithmetic traces to nothing the player
can point at.

The version that shipped has **one** moving part: the condition the last raid ended at.
It is assigned, not added, so a quiet raid returns the campaign to calm however loud the
raid before it was — which means the spiral is not merely unlikely, it is
**unrepresentable**, and there is nothing to floor because the floor is zero and every
raid can reach it. The relief valve §2.2 asks for is the next raid itself. Spending intel
to clear a mark before choosing (#213) then becomes a real sink rather than a way to pay
down a debt the player did not choose to take on.

It also makes the loop *legible*, which is the half §14 v3 actually complains about. A
player can be told exactly what a loud raid cost, because it cost one thing on one road
that is on the screen in front of them. A campaign alert of `4`, decaying by one per hop
and switching on modifiers at thresholds, is a number you can display and not a
consequence you can read.

### Why breadth rather than depth

Given "condition 2 does something and condition 3 does something worse", the natural
reading is *more*: one harder rule at 2, two at 3. That was rejected for what it does to
the choice point. A second rule stacked on the same facility is a difference the player
meets **after** committing to it — inside the building, on the help panel — and cannot
act on.

Breadth puts the escalation on the map, where the player is making a decision. At
condition 2 exactly one of the roads ahead is alerted and the other is not, so the
consequence is *route around it*, which is a real play with a real cost (the unwatched
road is whatever flavour the seed put there — it may be the Outpost with nothing in it).
At condition 3 every open road is alerted and there is no steering left. The step from
2 to 3 is therefore the loss of an option rather than an increment of a number, and the
player feels it at the moment they are choosing.

Two consequences fall out of it that are worth stating:

- **Each alerted facility draws its own rule.** *Every road is watched* is not *every
  road is watched the same way*, so a condition-3 choice point is still a choice between
  differently-shaped raids rather than three copies of one penalty.
- **A ghost raid is the mirror image.** Condition 0 leaves one road ahead *off guard*,
  drawn one easier rule. It costs the same to state and it is the reason the alert is an
  axis rather than a tax: a campaign that only ever punished noise would make stealth
  the absence of a penalty rather than a thing you are paid for. It is deliberately the
  same size as the punishment — one rule, one road — because the point is symmetry, not
  a reward curve.

### The two edges

**The intel-locked road is never marked.** The mark lands only on the open successors —
the ones `Campaign::choose` will actually take. Marking uniformly across all successors
would put roughly one choice point in four's worth of consequence on ground the run
cannot walk onto: an alert with nothing behind it, which is §2.3's decoration failure
reappearing inside the ticket that exists to end it. The fiction agrees — the locked edge
reaches a lane two across that no open edge from here can, so it is the one road the noise
did not travel — and it gives #212's sink something real to sell at condition 3.

> **Revisited by #212 (appendix 48).** This held while the edge was inert. Once intel can
> open it, a bought road that the condition-3 sweep skipped would make intel a way out of
> the top rung — undoing the one thing that rung says. The rule now splits: never the
> marked road at condition 2 (the argument above, intact), reached like everything else at
> condition 3.

**The last hop still carries.** A node one short of the archive has exactly one
successor, so a loud raid there alerts the archive itself. That is the geography biting
rather than a special case: there is nowhere else for the noise to go, and the run's last
raid being the one it made hard for itself is the loop working.

### What the readout had to do, and what it could not

The line has to arrive **before** the choice or the mechanic does not exist — routing
around an alerted facility is the whole play at condition 2, and a player who learns
which road was watched after walking down it has been told nothing.

The obvious surface was a mark on the offer's own row, and it does not fit: the widest
row (`Workshop — salvage, at intel's cost`, with its marker) already spends 38 of the
board's 40 cells. So the readout is a **subtitle** under the heading, and it names the
facility by its **flavour** — which is exact, because no two open successors ever share
one (§14 v3 **[SETTLED]**). The line has to carry the loudness itself in any case (which
condition, and whether it reached one road or all of them), which no per-row mark could,
so the constraint and the better design pointed the same way.

### Where the numbers are, and what would move them

Everything above is **[START]**. Condition 2 as the first rung that carries is the one
worth naming: rung 1 is a fact of almost every honest run — first contact, and the ladder
never comes down within a facility — so a campaign that taxed it would be taxing playing
the game at all, and rung 2 is the first rung whose meaning is *this raid went wrong*
(§7.3: it is where the facility stops reacting and starts adding guards to itself).

**"Unnoticed" is condition 0, and it is not the same as never being seen.** The two are
different questions and the end screen already prints both, side by side, in its noise
row: `seen 2 · no alert — you are unnoticed` is a legal and unremarkable line. `seen` is
every fresh detection, glimpses included; the condition only leaves rung 0 on a
**confirmed** sighting — three turns of certain-zone contact inside a ten-turn window,
tallied facility-wide — or a missed radio ping, and a glimpse contributes nothing to that
tally ever. So the cherry goes to a raid the *building* never worked out, not to a raid
nobody ever laid eyes on.

That is deliberate, and it is where the ceiling is left rather than the floor: the
obvious follow-up is a **higher** reward for a true ghost — condition 0 *and* zero
detections — sitting above this one, rather than this one being tightened to mean it.
Tightening would leave the ordinary clean raid paying nothing, which is the wrong end to
adjust; adding a rung above keeps what is here and gives the hardest way to play
something of its own. `RunStats` already carries the detection count, so the mapping has
what it would need on the day someone decides what the better prize is.

What is **not** open is the seam. The alert reaches the facility through
`ModifierSources::alert` and the §12.6 directed pool, so it can only ever switch on a rule
the pool already documents with a direction — which is what makes the §2.3 assertion true
by construction rather than by review, and what keeps the campaign from growing the
private difficulty knob set the ticket was written to forbid.

---

## Appendix 42 — Fogging the layout is a knob's end, not a toggle, and not a difficulty step

*(§7.6/§11.5a/§12.6/§13.2; #233, on the seam #225 left. `crates/core/src/modifiers.rs`,
`crates/core/src/render.rs`, `crates/core/src/level_seed.rs`.)*

#233 asks for a hard modifier that fogs the **geometry** — the layer §11.5a settles as
*"always visible, from turn one. Never fogged."* The ticket knows what it is asking:
the doc keeps the layout visible so the player can plan an escape before being spotted
(§7.6), and it says of the alternative that *"a player who is chased and improvising in
unknown geometry is not playing a stealth game, they're rolling dice."* Building the
fog is a dozen lines. Three decisions around it were not.

### It is one knob with three rungs, not a second toggle

`full_layout_known` already shipped (#307): the *easier* rule that draws the real
building where the schematic stands. The obvious implementation of #233 was a second
`bool` beside it, and it is wrong for a reason that shows up the moment two sources
compose. A modifier set is a **union** (§12.6) — the alert may add pressure the player's
choice did not ask for — so two independent bools can arrive both set, and *"the full
layout is known and also unknown"* has no answer. Every resolution is a precedence rule
invented at the read site, which is precisely the coupling §12.3 wants visible.

They are two answers to one question, so they are one field:
`LayoutKnowledge { Full, Plans, None }`, baseline in the middle, composed by
`GuardCount`'s rule — a quiet source yields, and a genuine disagreement resolves
harder-ward. There is now exactly one place the question is answered, and the compiler
enumerates the three rungs at every read site.

The knob costs nothing in the token, which is what made the shape affordable. Slot 4 was
`full_layout_known`'s and keeps its meaning as the knob's easier end, so every token ever
minted decodes to exactly the run it named; the harder end is **appended at slot 16**,
and the ends being twelve slots apart is fine because a slot number is a position, never
a reading order. The both-ends-at-once rejection the guard knob introduced (#232) covers
it unchanged.

### It overrides a [SETTLED] rule, so the doc says so where the rule is

A modifier that bends a settled rule from inside §12.6 leaves §11.5a asserting something
false with nothing to notice it by. Every other modifier flips a rule its own section
already describes as a knob; this one contradicts a table. So the override is written
into §11.5a itself, next to the row it falsifies, with what it costs — the §7.6 pillar,
by name — and the base game keeps the visible layout. The rule is still settled; what is
settled now includes the one modifier allowed to bend it.

### The pool asks a third question of a candidate

The §12.6 directed pool (#297) had two filters for a candidate modifier: *can it be
composed* (the intel gate cannot, appendix 29) and *where is its baseline* (a neutral
middle can be drawn from both ends, appendix 31). By both, this end qualifies — it is a
renderer-only rule read on the same board, and its knob's baseline is a middle. It is
still out.

The reason is a third question neither of the first two asks: **is this pressure, or is
it a different game?** The −2…+2 axis promises the same game under more or less of the
former. Hiding the layout removes route-planning from turn one and hands back a run
where a chase is improvised through unknown ground — the dice-roll §11.5a warns of, on
purpose. A player who moved the slider to *+1* and got that would not have been given a
harder facility; they would have been given an unfamiliar mode without asking. So the
modifier stays reachable by name — a chosen set, a shared token, a node flavour — and
unreachable by the draw.

The second reason is measurement, and it is the honest half. The §13.2 bot is granted
geometry unconditionally, on §11.5a's authority — the very rule this end overrides — so
it routes confidently through walls it has never seen and pays none of the price. A
batch under this modifier measures the bot, not the game (§13.3). Keeping the end out of
the pool means no difficulty draw can quietly put a batch in that position. Closing the
hole properly means teaching the bot to route over *known* geometry and re-plan on what
it discovers — an optimistic explorer, which is a change to the core of the policy
rather than a flag, and so its own piece of work. Until it exists, this modifier is
judged by playing it, and `docs/bot-behaviour.md` §2 says so beside the row that is
lying.

---

## Appendix 43 — The Saver: a passive with a budget, and what a fearless bot did with it

*(§2.2/§2.3/§4.5/§7.2/§8.2/§8.3/§13.4; #243. `crates/core/src/ability.rs`,
`crates/core/src/state/guards.rs`, `docs/stats/abilities/saver.md`.)*

§4.5 is one of the shortest **[SETTLED]** rules in the document: *"a guard that attempts
to move into your cell captures you. That is the only loss condition."* #243 proposed an
ability that suspends it — the guard that catches you is taken down instead — and the
whole question is what may bound an exception to a settled rule.

### The ticket's shape was a toggle, and the toggle is the wrong instrument

As written, the ability was a self-targeted toggle: one turn to switch on, a window of
two or three turns, a very long cooldown. Every part of that fights what it is for.

A defensive window has to be **predicted**. You spend a turn standing still — in the one
moment a guard is closing — on a guess about the next three turns, and if the guess is
wrong you have spent a turn, opened the lockout, and are still caught. The ticket knew
this: it flags §8.2's timing trap and insists the activation turn itself be protected.
But that trap only exists if there *is* an activation turn. A passive has none, so the
protection cannot be mistimed, cannot be spent on the wrong turn, and cannot be forgotten
in the panic it exists for.

The cooldown is the second half of the same mistake. A lockout ends; a run long enough
sees the ability come back, and an insurance policy that returns is one you spend
carelessly. The bound wanted here is the one §8.2 already offers for effects too strong
for a clock — **uses per level** (#302) — and at one use it is stricter than any cooldown
can be: not "rarely", but "once, and then it is a §4.5 game again."

### What that combination cost in the model

Passive and budgeted was **unrepresentable**, not merely unbuilt, and the reason is worth
recording because it is a good failure. The budget lived inside `Economy` — the struct a
passive deliberately does not have (#264 made "a passive is not an ability with duration
0" a type-level fact). So the one axis §8.2 describes as *composing with* the time economy
was structurally a privilege of abilities that have one.

The fix is one field moved: `uses` sits on `Ability`, beside the mode rather than inside
it. §8.2's own prose is what says this is right — *"it composes with the time economy, it
does not replace it"* — and the non-time axis is now the non-time axis for both modes.
Two consequences fell out rather than being designed:

- **What spends it is the world, not a press** (`Deck::spend_effect`). A passive has no
  activation to charge, so the moment its effect is actually called upon is the only
  honest place to take the use — and that seam is keyed on the §8.1 **effect**, never on
  an identity, so a second ability that ever declares the same effect is charged on the
  same rules.
- **Spent is off, for a passive only.** A budgeted passive whose supply is gone is not
  merely un-pressable — there is nothing to press either way — it is *not in effect*, so
  the greyed bar entry and the game's behaviour are one fact. An **activated** ability is
  the opposite and must stay so: an active window keeps running after the use that bought
  it was the level's last, because the effect is what the use was spent on. `Deck::state`
  already ranked those two cases; `in_effect` now mirrors that ranking rather than
  inventing a second one.

### The body is where it stood, not where you are

A takedown leaves its body on the target cell. This one cannot: a lunge that is turned
over never arrives, and the player's own cell may be a **cupboard** (§10.3 — a witnessed
hideout is captured into, which is one of the two ways this fires while hidden), where a
body is a different noun entirely (stowing one locks the cupboard, §7.2). So the guard
drops where it stood. That is also the better fiction and the better cost: you are left
standing next to a body in a cell a guard was walking to, with the §7.3 clock already
counting, which is exactly §7.2's economy — a takedown you cannot hide is a takedown that
finds you later.

### What the sim measured, and why it is not a verdict

100 seeds a batch, all four temperaments, innate loadout against innate-plus-Saver:

| profile | win rate, innate → Saver (`--seed 0`) | replication (`--seed 100`) |
|---|---|---|
| `balanced` | 0.35 → **0.63** | 0.30 → **0.53** |
| `cautious` | 0.53 → **0.74** | 0.51 → **0.66** |
| `aggressive` | 0.40 → **0.69** | 0.37 → **0.59** |
| `careless` | 0.43 → **0.66** | 0.37 → **0.50** |

Eight batches out of eight, +0.13 to +0.28. Swapped into a real three-tech kit in place
of Confusion it is the strongest verb in the kit by a distance (`balanced` 0.45 → 0.62,
`aggressive` 0.42 → 0.74). The takedown counter moves with it — `balanced` goes from 0 to
62 takedowns in 100 runs — which is the save firing, since the bot itself lands almost
none: **62 of 100 runs reach a capture moment**, and about half of those go on to be
captured again anyway.

That last number is the finding, and it is a finding about the **bot**, not about the
game (§13.4). The bot has no fear: it will take a 5% capture risk forever, so it walks
into the moment this ability refunds far more often than a player who is frightened of
dying would. An ability that returns your first capture is worth exactly as much as your
first capture is likely, and the bot's is very likely. A human's is not — which is why
this appendix reports a measurement and not a balance decision. What the numbers *do*
establish is that the ability is nowhere near inert, and that if it is retuned the knob is
`SAVER_USES` and the direction is not up.

### If it needs nerfing, two levers before the budget

`SAVER_USES` is the obvious knob and it is already at its floor — one — so a version
that is too strong cannot simply be turned down. Two changes to the *effect* are on the
table instead, and they are recorded here so that a later playtest verdict has somewhere
to start rather than reopening the whole design:

- **Stun the player for N turns after the save.** The escape stops being clean: you
  survive, but you spend the next few turns unable to act while the guards keep moving
  — §8.3's safety-eject stun applied to a different rescue, and the machinery for it
  already exists (`Ejected`'s "every key is swallowed, the turn is spent"). It attacks
  exactly the thing the sim measured: the bot's saved runs mostly continue as if nothing
  happened, and a stun means being caught still costs you the position you were caught
  in. The number would be a fresh **[START]**, and the honest starting guess is small —
  two or three — because a long stun in a room with a second guard is just a slower
  capture, which is the §2.2 death this ability exists to avoid.
- **Stun the guard instead of taking it down.** The gentler and more interesting of the
  two: the guard that grabbed you is put down for a while (§8.3's Confusion daze is the
  existing vocabulary) and then **gets up**. You lose the free takedown and the body,
  which is a real loss — a takedown is permanent and a daze is not — and you gain a
  guard who knows exactly where you were. It also changes what the ability *is*: from
  "one capture refunded" to "one capture deferred", which is a much smaller promise and
  may be the right size for an exception to a **[SETTLED]** rule. The cost is that the
  §7.2/§7.3 body economy stops paying, so the save no longer prices itself; whatever
  replaces it would have to.

They are not exclusive, and the first is the smaller change. Neither should be tried
before a human has played the ability — the sim cannot tell a strong ability from a
frightening one.

### What is not open

That it is an **exception**, and stays declared as one. It is written into §4.5 beside the
settled rule rather than tucked into the ability table, because a reader who learns "a
guard touches you and the run ends" must not later discover a footnote that quietly means
otherwise. And it is bounded by the *level* rather than by time on purpose: whatever this
becomes — promoted to a §12.6 modifier, retuned, or rejected — a version that comes back
during a run is a different ability, and it is not this one.

---

## Appendix 44 — The exchange is the ability bar, and the crate is where the run says what it is

*(§8.2/§8.3/§8.4/§11.4/§11.6/§2.2; #266, on the seam #209 left. `crates/core/src/exchange.rs`,
`crates/core/src/state.rs`, `crates/core/src/state/view.rs`, `crates/core/src/render/hud.rs`,
`crates/core/src/status.rs`.)*

#209 shipped the crate and the cap together, and the cap shipped as a **dead end**: a run
already carrying `MAX_TECH_HELD` pieces of tech bumped a crate and was told *no hands
free for it*. Appendix 40 said outright that this was the unsatisfying answer and that
the interesting one is a swap. This is the swap.

The consequence is bigger than one refusal turning into a prompt. Under the refusal, the
tech axis was a **queue**: a campaign's third crate was the last decision it ever offered,
and every crate after it was scenery you walked past. Under the exchange, the fourth crate
is where a run starts saying what it is — you are no longer collecting tech, you are
choosing a loadout, one piece at a time, against what the next facility is likely to ask
for (§2.2's *"you get meaningfully stronger inside a run"*, read as *shaped* rather than
merely *bigger*).

### You press the one you drop, and that is the whole vocabulary

The four candidates are the run's three pieces of tech and the crate's one. Pressing any
of them **discards** it. Three of those presses are the trade; the fourth — the crate's
own — is the decline, because declining a crate *is* discarding what it was offering.

That collapse is the point. The obvious shape was *choose which to keep* plus a separate
*cancel*, and it carries two verbs that mean nearly the same thing and can drift: a cancel
path is code nothing else exercises, and the first time it forgets to close the offer or
forgets to leave the crate standing, the bug is invisible until someone plays it. One
input (`Input::Discard`), one resolution (`Exchange::resolve`), three outcomes — and
`Escape` is *mapped to* the decline rather than being a second way of causing it.

Innate abilities are not candidates. Run is never found, never drawn and never traded
(§8.3), so a row that listed it would be offering a press that has to refuse.

### Drawn on the bar, because the selection spine already existed

§8.4's rule is *build targeting up front and reuse it*, and the reason is the old
version's free unlimited-range neutralise: every ability that grew its own way of picking
a thing grew its own way of picking the wrong thing. A fifth modal screen for one decision
would have been that mistake in the UI layer — a second list to navigate, a second nav
enum, a second hit-test, a second set of keys, all for four entries.

The ability bar is already a four-slot selection surface: a digit binds by position
(#359), a mnemonic letter by the letter drawn (#360), a tap by the column (#267), and all
three resolve through one seam (`ability_in_slot`). So the exchange **is** the bar for as
long as it is open. `State::bar_statuses` decides which row that is — held set, or
candidates — and every key, letter and thumb follows it for free, which is also what makes
"the keys fire what the row draws" true by construction rather than by two matching
edits.

It fits by arithmetic, not by luck: `MAX_BAR_WIDTH` is sized for `MAX_HELD` entries, the
innate set plus the cap, and an exchange row is the cap plus one — the same four. The
crate's entry is drawn in **Interest**, the reward colour of the `¤` it is still sitting
in, against the three Owned ones beside it.

**The candidates carry no clocks**, and that was a deliberate reversal of the first
attempt. Drawing each candidate's real state looked more honest and read worse: a cooling
entry drew in the *unavailable* tan and an exhausted one in the receding grey that §11.4
reserves for "plainly not an option now" — greying out entries that are perfectly
droppable. A spent `Bore` is as tradeable as anything else. While the offer is open the
row is not a readout of the economy at all; it is the choice, and a number on it would be
about a press that is not on offer.

**What that width bought instead: the slot numbers.** The ordinary bar has never drawn
them — position is muscle memory (#287/#359) and the cells belong to the clocks — but the
exchange row is *picked from* rather than glanced at, and counting slots to find the digit
is exactly the friction a decision screen must not have. So the numbered form is the
exchange's alone (`1 Camo`, the digit in the same key colour the mnemonic mark wears), and
it is spent **inside** the existing slot rather than widening it, so the hit-test, the
layout and the width bound are all untouched. A candidate's bare name is 5 cells against a
9-cell entry, which is where the two cells come from.

The crate's entry wore a `(+)` for exactly one iteration, and the numbers are what
retired it: with a digit in front of every entry, the marker was three cells restating
what the colour already said, on the one row whose width was worth spending on keys.
Two channels for one fact is a channel too many when the other one is *which key do I
press*.

### The world stops, and the rule lives in the core

While an offer is open, `State::step` answers nothing but the discard. Every other input
— a step, a wait, an activation — stops at the door, so no guard moves while a run is
deciding.

It stops **before the message bookkeeping**, and that is not a detail: the first version
swallowed the input *inside* the player phase, which meant the turn loop still filed the
outgoing messages and replaced them with an empty set — so pressing an arrow at a crate
wiped the very near line telling the player what they were being asked. A swallowed input
is not a free action that happened and reported nothing; it is an input that never
arrived, and it must un-say nothing.

The belt to that brace is the **ambient floor** (§11.4): while an offer stands, the near
line's quiet floor *is* the question, above the stun and everything else on the ladder.
That is where a standing state belongs — the same argument the phase-eject countdown
already made — and it means the row keeps asking however long the player takes and
whatever they press, rather than relying on one message staying live.

Putting that in the **core** rather than in the shell that draws the row is the load-
bearing half. A shell can only make its own input path obey it, and there are three: the
browser, a replayed script, and the §13.2 sim. A run that could walk away from a
half-answered crate in one of them and not the others is a run that does not replay
(§12.4) — the seed-plus-inputs guarantee would hold only for shells that happened to
implement the same modality.

For the same reason the choice is an `Input` with a script token (`!<letter>`) rather than
a shell-side click. It is a third **sign** on the same letter, not a reuse of `-`: a
toggle-off and a trade are different actions on one ability, and a script that spelled
them alike would replay the run as a different one — the ability still held, every later
token fed to a loadout the original never had.

### The turn it costs, and the one it does not

The bump that opens the offer is free (§4.4 — nothing changed), and the trade spends the
turn a plain salvage would have. So trading at a crate costs exactly what taking from one
costs: a walk and a turn. A decline costs what the old refusal cost, which is nothing.

The alternative — charging the turn at the bump and resolving the choice out of time —
would make opening a crate you then decline a turn spent on nothing, and *deciding* would
become something you paid for. §4.4's line is that an action which changes nothing costs
nothing, and until a choice is made, nothing has changed.

### Revoking an ability, and the two traps in it

An activated ability is *in effect* by its **slot**, not by loadout membership. So trading
one away mid-window has to switch it off first, or the run keeps the effect of a tool it
no longer holds with nothing on the bar to say so. `Deck::revoke` does exactly that, and
the world half — a decoy still standing, a lockdown's seals — rides the same unwind an
early toggle-off takes (`State::unwind_effect`, now shared by both).

The in-wall case cannot arise, and pleasingly not by a guard: while phased there is **no
bump at all** (§8.3 — every in-bounds cell is a plain move), so a phased run can never
open an offer, and Dephase can never be traded away from inside a solid.

The slot and the per-level budget are otherwise left where they are. A run that trades a
cooling ability away and finds the same tech in a later crate picks it up exactly as cool
as it put it down: drop-and-refind is not a free recharge (§8.2's fence).

### The campaign takes the set, it no longer folds the finds

This is the change #266 forced one layer up. `Campaign::bank` used to union the raid's
`salvaged` set into the run's loadout, which is correct for as long as a loadout can only
**grow**. Once it can shrink, ordering matters: a run that swaps A for B, then later finds
A again and swaps C for it, ends holding a set that no union of *found* and *given up* can
reconstruct.

So the verdict carries `RunStats::held` — what the raid walked out holding — and the
campaign **assigns** it, the same shape `alert_peak` already had and for a related reason:
it is a fact about the raid that just ended, and the raid is the only thing that knows it.
`salvaged` stays beside it as what the *facility* was worth, which is the question the end
screen's ledger is actually asking.

### A duplicate pays out once, and it is the one crack in §8.2's fence

§8.2 settles use budgets with a fence, and the first clause of it is *"no recharge — no
regeneration, no pickup or console that tops it up, no way to earn one back."* This ticket
puts one exception in it, deliberately and by name: **a crate holding tech you already
carry refills that tech's per-level budget**, if you have spent from it, and is emptied
doing so. `Bore recharged`, a spent turn, the crate gone.

The case for it starts with what a duplicate was: appendix 40 rules that crates are
stocked from the facility's own seed and that meeting tech you already hold is *bad luck
rather than a bug* — the world is not rearranged to spare you the walk. That is still
right about the **draw**. What it left was a cell that was worth walking to only if you
did not know what was in it, and once you did, a crate you could see and never use again.
For seven of the eight pieces of tech that is fine; for the one with a **budget**, there
was an obvious thing a second copy could be for, and refusing it was the game declining
to notice.

What keeps it inside the spirit of the fence rather than through it:

- **It is not a resource to manage.** Nothing regenerates, nothing ticks, and there is no
  decision anywhere in a run about *getting more of it* — the fence's actual claim. You
  find another Bore or you do not.
- **What it costs is what everything at a crate costs**: the detour and the turn. The
  refill is not a reward for play, it is the crate's payout.
- **It is bounded by the building** (§14 v3): a facility hides at most three crates, all
  different, so the ceiling on refills in a level is the flavour's own number and not a
  rate anyone can farm.
- **It refills to the level's grant, not by one.** A partial refill would put a second
  number on the axis for no gain; the row states the grant, and the crate restores it.
- **A full budget is still the free refusal.** If there is nothing to give back the bump
  changes nothing, so it costs nothing (§4.4) and the crate is left standing.

The alternative considered and rejected was a consolation *elsewhere* — a duplicate paying
out intel, or partial progress toward something. That is a reward economy being made
whole, which appendix 40 rejected for good reasons and which would have made the draw's
honesty (it does not peek at your loadout) into a thing the game apologises for. Paying
out in the one currency the duplicated tool itself owns keeps the crate's contents
meaningful: what you find is *that ability*, and this is what having it twice means.

### What is not here

No consolation for a duplicate of anything unbudgeted. Seven of the eight pieces of tech
have nothing a second copy could restore, so those crates stay the free refusal, and
appendix 40's reading of them as bad luck stands.

No picking a dropped ability back up. What you trade away is gone for the run — there is
no floor to retrieve it from and no crate keeps what you put down. A takeback would make
the choice a rehearsal, and the whole reason the cap is interesting is that it is not one.

The sim measures none of this yet, and says so: the bot has no cue for a crate, so the
cache count still has no `--modifier` name (#209) and no batch can plant one. The
exchange is judged by playing it until the bot learns to salvage — at which point *what a
bot trades for* is the first thing worth watching, because a policy that always keeps what
it has is the same as one with no exchange at all.

## Appendix 45 — Transferring control: one window for the flying and the watching

*(§2.3/§4.4/§4.5/§8.1/§8.2/§8.3/§11.5a/§13.2; #273. `crates/core/src/control.rs`,
`crates/core/src/state/control.rs`, `docs/stats/abilities/drone.md`.)*

§8.1 reserves a code escape hatch and names two things it is for: *"piloting a drone,
rewinding time"*. #273 is the first of those, and building it turned out to be less
about the drone than about the seam under it — what it means for the player's input to
drive something that is not the player.

### The ability is a control mode, not an effect

Every other ability changes what your body can do: how far it steps (Run), what sees it
(Camouflage), what it can walk through (Dephase), what it can reach (Pierce Wall). No
arrangement of the §8.1 effect vocabulary expresses *the keys now move a different
thing*, and inventing a `TransferControl` primitive for one row would be exactly the
DSL-rot the section warns against. So it is `Behaviour::Coded`, which is the design's
own prescription rather than a shortcut.

What that buys, and the reason the state is called a **remote** rather than a drone:
taking over a guard (Messiah-style) is the obvious second use, and it should be a row in
one table (`control::remote_kind`) plus a spawn rule, not a rewrite of the turn loop.
The rules that are genuinely the drone's — that a wall stops it, that its camera is a
short full circle, that the facility cannot perceive it — live on the *kind*. The rules
that are the seam's — who the keys move, what a transfer costs, when it ends — live on
the loop and never name an ability.

### Two clocks became one, and the one clock is the ability

The ticket proposed two numbers: a pilot duration, and a **~50-turn linger** after
deactivation during which the drone keeps granting vision. That is two timers, and they
interact badly in three ways.

- **The bar can only honestly show one of them.** §8.2's timing rule is that every
  surface reports the number the player actually gets. `Drone[7]` would mean seven turns
  of flying and then fifty of something else — a number that is true and useless.
- **Deactivation stops being the §4.4 free toggle-off and becomes a conversion.** The
  player is no longer switching something off; they are trading one currency for
  another, at a rate the interface cannot state.
- **Total time balloons.** Pilot + linger + cooldown is three numbers to tune against
  each other, and the ability's real lever — *how long is the facility watched?* — is
  the sum of the first two, which nothing displays.

The window opened at 30 and was set to **40** on the first play-through: 30 is enough
machine to fly somewhere or to leave a camera behind, and not enough to do both, which
collapses the very decision the single clock exists to create. The lockout absorbs it —
80 turns is a long way from the next press either way.

So there is **one duration, covering both halves** — **40 turns [START]**, against a
40-turn cooldown, for an 80-turn lockout that is comfortably the longest in the
catalogue. Press to launch and fly; press again
and the controls come back to your body — free, refunding nothing, and **not ending the
window**. The drone holds the cell you left it in and keeps feeding its camera until the
duration runs out, and only then does the cooldown start.

This is a better ability than the two-clock version, not merely a tidier one. It makes
the decision *how much of the window do I spend flying?* — deep scouting now, or a long
watch on a junction you have to come back through — and that decision is made turn by
turn with the number on the bar in front of you. It also makes the key a three-state
toggle worth pressing more than twice: an unattended drone can be **taken back**, for
the same turn the launch cost, which turns the ability into a remote eye you can jump
into rather than a one-shot with a tail.

### The cost is the parked body, so the body must be reachable

§2.3 asks what an ability costs and when a good player declines it. An invisible,
indestructible, unblockable scout with free movement is the section's own failure mode
written out, so the answer has to be load-bearing rather than decorative: **while you
fly, your body stands still in a patrolled building and the world keeps running**
(§4.2). Capture is contact (§4.5), so a patrol walking into that body ends the run while
you are looking through a camera two rooms away. Scouting deep costs exactly as many
turns of blind exposure as it buys of vision, and the good player's *"when would I not
do this"* is answered by geometry: you fly from somewhere nobody walks, and if you have
nowhere like that, you do not fly.

That answer has exactly one leak, and it is worth naming because it is the sort of thing
that quietly makes an ability free. **A body inside a duct cannot be touched** (§10.7),
so piloting from a crawlspace costs nothing at all — and the run *opens* inside its own
tunnel (§4.5/#466), which would let a player read the whole facility before setting foot
in it. Hence the one precondition in the §8.4 ladder: you launch, and take the controls
back, on your feet. It is not fussiness; it is the thing that keeps the cost true, and
it is why the refusal speaks (§11.7) where the decoy's silent one does not — the rule it
enforces is invisible.

A **cupboard** is deliberately not held to the same rule, though it conceals too
(§10.3). What it grants is weaker and it is paid for: a guard that watched you climb in
flushes you out (§15 Q5), you walked to a specific piece of furniture to get it, and you
cannot act on anything the camera shows you until you climb back out. Hiding somewhere
sensible before you fly is the play this ability ought to reward. A duct is none of
those things — it is a travel network, it is blanket contact-safe, and the run starts
inside one.

The two knobs deliberately **not** used as cost are the drone's invulnerability and its
mobility. Letting guards see and shoot it down is a different ability and a separate
ticket; if playtest says this one is too strong, the honest levers are the clock, the
camera's reach, and drone-only vision while flying.

### What it is allowed to do, and the two things it is not

It **respects the building, at its own scale**. A wall-ignoring drone is Dephase plus
omniscience, and the facility's shape — the thing every other system in the game is
about — stops mattering; what makes this one sweep easily is that nothing threatens it,
not that it is incorporeal. But *scale* is the whole point of the exceptions: the thing
is hand-sized and airborne, so it goes **over a table** (furniture at waist height, solid
only to somebody walking) and **through a shut door's ventilation holes**. A door frame
stops it dead, because a hinge is structure rather than a door, and so do the solid
usables — a console is a thing you bump, not a passage, at any size.

The line that draws is *fabric versus passage*: a shut door is a passage that happens to
be closed and a wall is a wall at any scale. Two consequences are worth stating rather
than discovering — a **closed-door wing is scoutable**, and a §8.3 **Lockdown seals
nothing against a camera**. Neither is a leak, because the drone crosses a door
**without opening it**: it changes nothing in the world it flies through, so what it
buys is still only information, at the price of the body it left behind.

It has **no interaction verb**. §4.3's one verb is the bump, and a bump is hands: the
drone opens no door, takes no intel, touches no guard and cannot win the run. It changes
nothing in the world at all — it only looks. A step into a wall is the free mis-input a
wall bump has always been.

And **your hands are on the controls**: while flying, every other ability is refused
through the same §8.4 ladder, so the §11.4 bar greys the whole row rather than
advertising presses the loop will swallow, and the usable line is empty. One rule with
no carve-outs, on the stun's reasoning (#329): *"you are not driving your body"* stops
being true the moment it has exceptions.

### Vision is a union, and the guard sense is not part of it

`State::player_fov` was one cast from one viewer. It is now the union of the body's cast
and the remote's, folded in at the single place sight is produced — so everything
downstream follows on its own: the §11.5a fog lifts, entities draw, the §11.5 danger
overlay paints the cones the camera can see, and tile memory accumulates it all, which
is the ability's actual payoff.

Two deliberate asymmetries in that union:

- **Your own eyes keep working while you fly.** The alternative (drone-only vision) is
  tenser and more punishing and is worth a playtest, but it makes the ability's cost
  *two* things at once, and the parked body is the one that should be doing the work.
- **The §9 guard sense stays on the body.** It is your own innate channel, and leaving
  it there is what keeps the parked body a live risk you can read rather than one more
  thing the drone covers for you. The camera also reaches **less far than your own
  sight** (8 against 15, pinned at compile time): it goes where you cannot, it is not a
  better pair of eyes.

### Two things of yours on the board

The §11.5 effect layer already had the shape for the one legibility problem this
creates. The drone draws as an `Owned` `*` for as long as it exists, and the *piloted*
one carries a standing effect mark — the third **conditional** placement, joining
Camouflage's and Dephase's on their own rule: it says the thing the §11.4 bar cannot,
because the bar reads `Drone[23]` whether you are at the controls or standing three
corridors away. The mark goes dark when you let go and comes back if you take the keys
again.

It rides the *thing* rather than the cell for a second reason: a drone flies over
guards, and a guard's `g` outranks it in the glyph layer, because a threat is never
hidden by a thing of yours. A background survives that. It still yields to the danger
overlay, which is §11.5 **[SETTLED]** working exactly as intended — on a watched cell
the board says *you would be seen here* and says nothing else.

### What the sim does not say about it

The §13.2 bot gets **no cue**, and this is a third kind of "no cue" distinct from a
passive's. Vision has none because there is no press; this has a press and no bot that
could survive making it. Piloting is a control mode the stealth policy does not have, so
a bot that pressed the key would transfer control and then go on issuing steps for a
body that is no longer listening — flying into a wall for thirty turns and reporting the
result as a measurement.

So `usage.drone` exists as a histogram slot and reads zero until a piloting policy lands.
That zero is a fact about the bot, which is exactly why the slot exists rather than being
omitted: a verb with no row could not report the day it starts being pressed. The
alternative — omitting the slot — would have hidden the gap instead of dating it.

---

## Appendix 46 — The locked room: what a key costs, and why the door has to shut itself

*(§10.4 doors, §12.6 level modifiers, §7.2 the takedown, §10.6 guarantees, §2.2
soft locks. The ticket is #236; §14's "Keys and locked doors" backlog line is what it
closes.)*

§10.4's baseline is one sentence — *"anyone can operate any door. No keys, no locks"* —
and it has carried a **[START]** marker and an explicit invitation since it was written:
keys are an obvious future axis, and one the fiction supports. This is that axis, scoped
as a level modifier rather than as a change to the baseline, so a run either plays it or
does not and says which on its card.

The design decision was never *whether* a locked door is interesting. It is **what the
lock costs, who can lift it, and how it survives contact with a facility full of guards
who keep opening doors.** Four things had to be settled, and three of them are less
obvious than they look.

### 1. The key is on every guard, not on one of them

The ticket's first sketch hung the key on **a specific guard**: find that one, take it
down, get in. It reads well and it is the wrong price.

The §7.2 takedown is already the most expensive verb in the game. It is permanent, it
leaves a body, the body starts a §7.3 radio clock, and a found body is the loudest event
there is. §2.3 asks what an addition *costs*; a lock whose key is on one named guard
charges the takedown **plus a search** — and the search is for a `g` the player has no
way to pick out, because every guard draws the same glyph in the same three colours by
mood (§11.3). What that produces is not a hunt, it is a sequence of takedowns until one
of them happens to be the right one, each with the full §7.2 cost. The modifier would
have priced itself at three or four bodies and called it stealth.

With the key on **every** guard the price is exactly one takedown, stated plainly, and
the player chooses which guard, when and where — which is the decision §7.2 already
exists to make interesting. The lock adds a *reason* to commit to that decision. It does
not add a second currency.

For the same reason the key goes **straight to hand** rather than onto the body. The
ticket left the choice open and asked for it to be pinned. A key that had to be picked up
off the floor is a second errand for one rule, and a second thing that can be lost to a
guard walking past the cell — which is a §7.3 cost the takedown already charges once.

### 2. The gated doors must be automatic, or the lock lasts one patrol

This is the part that decides whether the modifier works at all, and it only becomes
visible once you put the lock on a board with guards on it.

Guards route straight through closed doors and open them by walking in (§10.4/#146).
Guards carry the key — they must, or the locked room is off every beat and the lock is
also a *patrol* change nobody asked for. So the very first patrol whose beat touches the
prize room opens the door. On a **manual** door it then stays open until something closes
it, and the only somethings are a Calm guard's seeded close-behind chance and the player.
In practice the lock would hold for the first thirty turns of a run and then quietly stop
existing, with nothing on the board to say it had.

Making the gated doorways **automatic** (§10.4/#147) fixes it at the root rather than by
tuning: frameless, no handle, and shut again a few turns after the doorway is last
vacated. The lock is then a standing fact about the building for the whole level.

And it turns the failure into the modifier's best moment. Those few turns are a real
**window**: a player who is standing in the right place when a guard walks through can
slip in with nothing in their pockets. It is thin — you have to be there, unseen, at that
moment, next to the guard that just opened it — and it is a *decision* rather than a
lottery, which is the shape §2.3 asks of a bypass. The modifier ends up with two ways
through it, one bought with a body and one bought with nerve.

The doors are converted **after placement**, not carved that way. Which room holds the
prize is decided by where the crates and consoles land, so the doorways cannot be chosen
while they are being cut. Converting folds the two hinges into the panel span, which lands
exactly inside `DoorKind::Automatic`'s documented 3–6 panel shape — a manual doorway is
already a 3–6 cell run with two of them solid — so nothing about the geometry had to be
widened to accommodate it.

### 3. The lock refuses entry and never exit

A player slips in behind a guard. The door times out and shuts. They have no key.

If the lock refused them from both sides, that is a **run ended by a mechanic the game
invited them to gamble on**, and §2.2 does not allow that to be merely unlikely — the
whole point of permadeath with no meta-progression is that a loss has to be readable as
a mistake. So the rule is: the key gate refuses the bump from the **corridor** side only.
From inside the room the door always opens.

This needs nothing remembered, which is why it is cheap enough to be the rule rather
than a special case. Every door joins a room to a corridor (§10.1.4, asserted since the
graph landed), so "inside" is simply the door's **Room** endpoint: the side the player is
standing on decides, and there is no stored set that can fall out of step with the board.

It also makes the bypass worth attempting. A gamble you can walk away from is a
decision; a gamble that can strand you is a trap, and players stop taking traps.

### 4. Two lock sources, one door, and why `DoorLock` became a set

Lockdown (§8.3/#242) already put a lock on a door, and its own note said the right thing:
one representation for every lock source, so *"is this door locked?"* has one answer. What
that note assumed was that a second source would be a second **variant**.

It cannot be. The two locks refuse different people for different reasons and neither
implies the other — a keyed door is one a guard walks straight through, and a Lockdown
window over that same doorway is exactly the wall the ability is bought for. As one
variant apiece, sealing a keyed door overwrites the key gate, and releasing the window
then **unlocks the prize room for the rest of the run**: an ability whose entire promise
is that it is temporary (§2.2) quietly destroying a lock it never placed, with nothing to
notice it by.

So the field holds a **set** of flags, one per source, and each source sets and clears
only its own. `is_locked` still answers the one question consumers that do not care ask;
the two seams that *do* care — a guard's walk-in open, and the player's bump — read the
flag they mean. The general lesson is worth keeping: *one representation* means one
place, not one value.

### 5. What §10.6 has to prove now

Placement's existing solvability check floods a board where a closed panel is routable,
which is the truth for a player holding the key. On its own it would happily accept a
facility whose lock had sealed the exit — or every guard in the building — behind the
same door as the prize.

So the lock owes a second assertion, stated in the frame a keyless player is actually in:
with every keyed doorway masked solid, the flood must still reach the **exit**, every
objective, crate and comms console **outside** the locked room, and **at least one guard
spawn**. The last one is the interesting clause: a lock whose key does not exist in the
reachable building is §2.2's soft lock wearing a different hat, and it is exactly the
failure a check written only about *objectives* would have let through. A board that
fails is rejected and the carve redrawn, like any other §10.6 shortfall.

### 6. What it cost to measure, and what the sim will not say

The pass draws **nothing** from the RNG. That is not tidiness — it buys the strongest
§2.3 claim any generation-time modifier here has. From one seed the two settings carve
the same building and place the same board down to each guard's radio clock, so the
directional assertion is exact and per seed: **a prize a keyless flood reaches at
baseline, it does not reach behind the lock.** Contrast `automatic_doors` (#452), which
reaches the carve and has to settle for a distributional claim over a sweep.

The bot is the honest gap. Over 100 balanced seeds under the sim preset's
`--intel-gate one` the modifier reads harder in the documented direction — win rate 35%
→ 24%, detections 848 → 1,086, diversity 0.60 → 0.54 — because the locked room is one it
routes around. Under `--intel-gate all`, where the locked console is *required*, the win
rate is **zero**: the bot knows a locked door is not a way through, and a takedown it
takes for its own reasons opens the room, but no plan of its says *the thing I need is
behind that door, so go and buy the key*. That is a fact about the bot, not about the
game (§13.3).

It is also the reason the modifier is **out of the §12.6 directed pool**, on
`LayoutKnowledge::None`'s ground rather than the intel count's: mechanically it would sit
in the pool happily, but a `+N` draw that picked it could stand a whole sweep in front of
a door the bot will not buy its way through, and the batch would then be measuring the
bot. It stays reachable by name, by token and by node flavour, where somebody asked for
it. Teaching the bot to go and get a key is its own ticket, and the day it lands the pool
row is one line.

### What was accepted rather than solved

- **A §10.7 duct that opens into the locked room walks past the lock.** Room selection
  prefers a duct-free room *within* its prize tier (a crate room ahead of a console room),
  so a crate room with a mouth in it is still locked ahead of a duct-free console room.
  Letting the duct outrank the prize would decide what the run is about on a generation
  accident, and choosing which prize sits behind the door is the more important of the
  two. On those seeds the modifier is thinner than it reads. Flagged, not fixed.
- **Pierce Wall bores in** (§8.3). That is counterplay bought with a limited-budget
  ability, which is what the tech is for, and it is priced.

---

## Appendix 47 — Intel is currency: one wallet, one counter, and no punishment for coming home empty

*(§2.2/§2.3/§4.4/§4.5/§12.7/§14 v3; #211. `crates/core/src/campaign/wallet.rs`,
`crates/core/src/campaign.rs`, `crates/core/src/render/campaign_map.rs`.)*

§2.2's table has said from the beginning that intel *"accumulates and is spent"* within a
run. Half of that was true in code long before this ticket — every completed raid banked
its haul into a `u32` on the campaign — and the other half was a comment pointing at a
future ticket. What #211 settles is not "add a spend function": it is the three questions
that had to be answered before a spend function could mean anything.

### Intel cannot be both the exit key and the currency

In quick play the exit asks for intel (§4.5): gather the set, then leave. That gate is
what makes a facility an objective rather than a shopping trip, and it is right for a
mode whose whole life is one building.

It cannot survive a campaign. A currency you must hand over to get out is a **toll**: the
run banks nothing it did not overshoot the gate by, and every sink is priced against the
surplus rather than against the haul. Worse, the two rules interfere in the direction that
kills the choice — a facility whose gate is *all* the intel makes "how much do I take"
a question with one answer, and the map's flavours (Vault against Outpost) stop being a
trade about how much you want.

So the campaign takes the gate off (`IntelGate::None`) and the consequence is stated
rather than discovered: **extraction is voluntary**. You may bump the exit and leave any
facility on the turn you entered it. Intel, caches, unlockables — everything in the
building is *surplus*, and what a raid was worth is settled at the hub afterwards.

The alternative considered and rejected was a *reduced* gate — one console, as the sim
uses (§13.2). It looks like a compromise and is not: one console is either trivial (in
which case it is a ceremony, not a rule) or it is the thing that kills a run on the seed
where the nearest console is behind a patrol. A rule that only bites by accident is the
worst kind, and §2.3's cost test is unanswerable for it — *when would a good player
choose not to?* There is no such moment; you always take one console.

### One spend context, and it is the map

The wallet could have been spendable from inside a facility. It is not, and the reason is
§4.4: turn cost is the game's central pressure, and a shop you can open mid-raid is a way
to pay money instead of turns. *Stuck in a corridor with a guard closing?* — buy something.
Every tight corner becomes a transaction, and the one resource the design actually cares
about stops being the one under pressure.

So spending happens at the map between facilities, and the check lives on the campaign
rather than in each sink. That placement is the load-bearing part: a sink that forgot to
ask *am I at the hub?* would be a shop open inside a building, and nothing about the
sink's own code would look wrong. Putting it in `Campaign::spend` makes forgetting
impossible instead of unlikely. It is why the refusal type has a third answer —
`Outlay::Closed` — rather than folding "not here" into "not enough": a player told they
are poor when the truth is they are in the wrong place has been told the wrong fact.

Both stages the map screen is the surface of are open for spending: standing on a facility
not yet raided, and at a choice point. That is not a loosening of "between facilities" but
a statement of what it means — the approach is the map too, and it is where a run buys the
ground it is about to walk onto (#212).

### The map screen *is* the hub

A separate shop screen was the obvious shape and it is the wrong one. Everything intel buys
is a fact about the country ahead — a route, an alert on a road, what is inside the next
facility — so the prices belong on the picture of that country. A second modal screen
would carry one list, teach the player a screen, and say nothing the map could not have
said in a line.

So the map grew a wallet line, and it is drawn on **every** frame, balance zero included.
A readout that appeared with the run's first haul would make the map band jump exactly
once — at the moment the player was reading it — which is the layout rule (§11.4) that
made the alert line a subtitle rather than a panel. Zero has its own wording ("nothing
banked") rather than a bare `Intel 0`, because for a run that has just walked out of a
facility with nothing, that line is the only feedback there is.

### Nothing is taken away for a wasted raid

The tempting rule is an explicit penalty: leave empty-handed and the alert bumps. It was
considered and deferred, and the argument against it is that the cost is **already there
and already compounding**.

A run that took nothing is a run that:

- has nothing to spend at the hub, so the route it wanted is not available;
- has burned one of a fixed number of facilities (depth six to the archive, §14 v3);
- meets the *next* facility with whatever noise the wasted raid made carried onto it
  (#210) — and a raid that achieved nothing was not necessarily a quiet one;
- has spent the caches in that building for good, because they are one-shot.

Adding a penalty on top charges twice for one mistake, and it charges in the currency that
is *already* the punishment. The emergent cost also has the property a designed one would
not: it scales with how badly the raid went, without anyone tuning a number.

The honest caveat is that nobody has played a full run yet, so this is `[OPEN]` on the
tuning rather than `[SETTLED]`. If a played campaign shows a run farming the easiest
Outposts and walking straight back out — the degenerate strategy this reasoning would
have missed — the lever is a small alert bump on an empty-handed extraction, and it is one
line at the `bank` seam. The bar for pulling it is a run that shows the exploit, not a
suspicion that it exists.

### What the wallet is, as a type

A newtype over a counter, and the constraint is the point: intel goes **in** whole (a
raid's haul) and comes **out** only through a spend that can refuse. Nothing outside the
type may set the balance, so "the wallet went negative" and "a sink debited without
checking" are not states the rest of the campaign can reach — the same argument
`CampaignStage` makes for not being a pair of flags.

A refusal changes nothing at all. There is no partial payment, so a sink that branches on
`Outlay::paid` cannot end up with the money gone and the effect unapplied — which is the
one bug class a currency with several buyers reliably grows.

---

## Appendix 48 — The alternative route: intel buys ground, and the top of the ladder still sweeps it

*(§2.3/§14 v3; #212, revisiting appendix 41. `crates/core/src/campaign.rs`,
`crates/core/src/campaign/loudness.rs`, `crates/core/src/render/campaign_map.rs`.)*

#207 shipped the map's intel-locked successor inert: an edge drawn, named and refused. This
is the ticket that gives it a price, and two of its three decisions were only decidable
once the edge could actually be taken.

### One intel — the price is what you know, not what you earn

The ticket landed at **four**, reasoned from income: a facility hides three consoles at the
§10.2 recipe, so four puts one unlock slightly above a whole facility's haul, a run cannot
buy a route at every junction, and *which* junction is worth it becomes the decision.

That reasoning was rejected on review, and the objection is the better one: **four intel is
too much for a route we know nothing about.** It priced the road as though the buyer could
see it. They cannot — the map draws unbought ground as `?` (§14 v3's "shape, not
contents"), so what four intel bought was an entire raid's income spent on a lane that
might hold an Outpost. A price has to be proportionate to what the buyer knows, and this
buyer knows one fact: where the road goes, not what is on it.

So **one**. It still asks for something real, and the shape of the ask is the interesting
part: nothing is banked until a raid has been walked out of, so the **first** choice point
of every run cannot afford one — the sink switches on when the run has actually done
something, which is a better gate than a number that scales with greed.

**Where the bite comes from instead.** The rejected argument put it in *scarcity*: make it
dear enough and choosing becomes hard. The real source is **opportunity cost** — the sinks
behind it (#213–#216) spend the same wallet, so a route bought here is an alert not lowered
or a facility not scouted there. That is the right axis for a purchase this cheap, and it
is the axis §14 v3 was describing all along when it listed four sinks over one currency.

**And if it turns out to be reflexive?** If a played run buys the route at every junction,
the temptation will be to raise the price back. That is treating the symptom. The reason
buying is reflexive is that the alternative — *not* buying — has no information behind it:
you cannot compare a road you cannot see to roads you can. The fix is to let the player
see, which is exactly what the scouting sinks (#215/#216) sell. Raise the price only if
buying stays reflexive *after* the player can tell what they are buying.

### Buying does not commit the run, and that is what stops it being a coin flip

The obvious shape is *pay, and you are on that road*. It is wrong, and the argument against
it is a rule the map already settled: **flavours are visible when offered** (§14 v3
[SETTLED]), because a choice made blind is a coin flip. An unbought edge draws as `?`, so a
purchase that also committed the run would be selling exactly the coin flip that rule
exists to forbid — and selling it for more than a facility's income.

So the purchase turns the edge into an **ordinary offer**: flavour showing, sitting in the
list beside the open roads, and the run may still take one of those instead. What four
intel buys is ground *and* the knowledge of what stands on it. The bet is real — the seed
may have put an Outpost on the lane you paid to reach — but it is a bet you get to decline
after seeing it, which is a different and much better decision than a bet you are locked
into before.

This is also where the sink's §2.3 answer comes from. *When would a good player choose not
to buy?* When the open roads already hold the flavour the run needs; when the intel is
wanted for a sink that has not landed yet; when four intel is most of a wallet that took
two raids to fill. All three are live at the same junction.

### The condition-3 sweep now reaches it — appendix 41 revisited

Appendix 41 reasoned that the alert should never mark the locked edge: marking ground the
run cannot walk onto is "an alert with nothing behind it", §2.3's decoration failure inside
the ticket meant to end it. That was right **while the edge was inert**, and it stops being
right the moment intel can open it.

Left alone, the old rule had a consequence nobody chose: at condition 3 — where §14 v3
[SETTLED] says *every* road ahead is watched and what the escalation takes away is the
route around it — a run with a single intel banked could buy an unwatched one. Intel would be a way
out of the alert's top rung, which is a second, unwritten rule about what the alert is, and
it would hollow out the one thing condition 3 says.

So the two rungs now treat the edge differently, on purpose:

- **Condition 2 — never the marked road.** The play at that rung is finding the road
  nobody is watching, and having that option must not cost intel. Appendix 41's argument
  survives here intact.
- **Condition 3 — reached like everything else**, bought or not. A road you paid for is
  still a road ahead.

The sink loses nothing by it. What it sells at condition 3 was never *safety*: it is a lane
the map was not offering, on a rung where the open roads are all alerted anyway — so the
choice becomes *which* alerted facility, from a wider set, which is exactly the kind of
option the map exists to sell.

The answer is a fact about the **edge**, not about the purchase: the map's alert line says
the same thing before and after the money changes hands, so a player cannot be surprised by
a road that changed its mind once they had paid for it.

### The priced row is a row like any other

The marker used to step over the locked row, on #268's rule that it only rests where Enter
does something. The rule has not changed — the row has. An intel-locked row is now a
**price**, and pressing Enter on it buys the road or says why it cannot; a row that answers
is a row the marker belongs on.

Two smaller consequences, both of them the §2.3 courtesy of showing a cost before charging
for the discovery:

- The row prints the campaign's own `ROUTE_UNLOCK_COST`, so a change to the price moves
  what the player is charged and what they are told in one edit.
- A price the wallet cannot meet draws in **Ground** — the meaning this screen already
  gives the road behind you — so it reads as out of reach at a glance rather than only
  when pressed.

And the hub's answer lands on the **wallet line**, replacing the balance rather than
sitting beside it. Every `Outlay` message already names the balance ("spent 1 intel — 3
left", "needs 1 intel — you have 0"), so the readout is not a fact the player loses; two
lines saying the balance twice would be the screen repeating itself. It clears the moment
the marker moves: a message about the row you have just left is a message about nothing.

---

## Appendix 49 — Pool membership is about the level, not about what the bot can score

*(§12.6 level modifiers and the difficulty draw, §13.1/§13.3/§13.4 the experiment loop,
§11.5a the fogged layout, §10.4 doors. The ticket is #518; the three modifiers it admits
are #452, #233 and #236.)*

The §12.6 **directed pool** is what a difficulty draws from: nine entries, five harder
and four easier, filtered on each field's own documented direction. Three harder
modifiers shipped outside it — *all doors automatic*, *layout unknown*, *locked room* —
each held back for its own stated reason. This is the ruling that admitted all three,
and the more useful half of it is not the three: it is **the criterion that had quietly
replaced the real one**.

### The criterion that crept in

Read the three exclusions together and a pattern appears that nobody chose:

- *locked room* was out **only** because the §13.2 bot has no cue for buying a key. It
  is a plain harder toggle; the two settings are the same building down to the cell.
  Nothing about it failed any test a pool entry is supposed to pass.
- *layout unknown* was out for two reasons, and the doc gave both: it is arguably not a
  difficulty step, **and** the bot routes through walls it has never seen.
- *all doors automatic* was out for two reasons as well: it reaches the carve, **and**
  the sim measured it as feel rather than difficulty — a win rate that did not move over
  four hundred runs.

In each case at least part of the reason was **"the harness cannot score it."** Stated
once, per modifier, that reads like diligence. Stated three times it is a rule, and the
rule is backwards. §13.4 is explicit: *"treat bot output as a smoke detector, not a
judge."* §13.1 is more explicit still: *"you play and rule. Fun is a human judgement."*
The sim exists to stop us shipping inert systems (§2.3) and to flag suspicious seeds for
a human — not to hold a veto over what the game contains.

A modifier's job is to **modify the level**. That is the whole of what §12.6 says one
is. Some will be light and sweep cleanly; others will only ever be judged by playing
them, and the second kind is not a lesser kind of modifier — it is most of the
interesting ones. Withholding those from players because the harness cannot put a number
on them is the §13.3 failure pointed the other way: instead of the bot's metrics quietly
becoming a description of the bot, the bot's *limitations* quietly become a description
of the game.

So membership asks two questions and only two:

1. Is it a **difficulty** change rather than a change of subject? The ±N axis promises
   the same game under more or less pressure.
2. Does it bend in a **documented direction**? That is what makes §2.3's directional
   assertion true by construction rather than by review.

Bot-blindness stays real and stays a **reporting** concern: a batch that draws a
modifier the policy cannot use is measuring the bot, and its numbers do not belong in a
balance argument. That is an argument for teaching the bot (#498, #517) and for labelling
the batch — never for a thinner game.

### It was never a small problem, which is the argument's strongest form

The objection would carry more weight if the pool were otherwise clean. It is not, and
counting was what settled the question. Of the four **easier** entries already in the
pool, **three are bot-blind**:

| Entry | Why the bot cannot use it |
|---|---|
| *All vision cones shown* | The policy builds its danger set from `perceive_guard == Seen` only; the widened overlay is never read |
| *Full layout known* | The bot is granted geometry unconditionally (`bot-behaviour.md` §2), so it already routes as if this were on |
| *Search areas shown* | Nothing in the policy reads a search area |
| *Guards: one fewer* | — it plays this one |

A `−1` bot batch has a three-in-four chance of drawing a no-op, and `−2` is guaranteed at
least one. Bot-blindness in this pool is the **standing state**, not something the three
admissions introduce.

There is a reason it clusters on that side, and it is worth stating because it will keep
being true: **a harder modifier lands on the bot whether it understands it or not; an
easier one has to be used.** Guards that search hideouts flush a bot that hid in one
without the bot knowing the rule exists. An overlay that reveals more cones does nothing
at all unless something reads it. So the easier side is where comprehension is required,
and comprehension is what the bot is short of.

### What each admission actually cost

**The locked room cost nothing.** It was the clean case, and the one that exposed the bad
criterion by having no other reason attached to it.

**Layout unknown cost the [SETTLED] override becoming reachable unnamed.** §11.5a is
**[SETTLED]** — geometry is never fogged — and this is the one modifier that overrides
it. Before, a player only met it by asking for it: a chosen set, a shared token, a node
flavour. Now a `+1` can deal it to them, and the Level info card's *"Layout unknown"* is
the whole of the warning. The objection that it is *not a difficulty step* has not been
answered; it has been accepted. Whether the card is enough, or whether that end wants a
louder cue on turn one, is the open question this leaves behind.

**Automatic doors cost the flat same-building guarantee**, and that one needed real work
rather than a shrug. `difficulty.rs` promised — and §12.6 repeated — that *a seed's
facility is byte-identical at every difficulty, and only the rules bending it differ;
that is what makes the ±N arms of a comparison worth anything.* The draw takes a salted
sub-stream precisely to buy it. But the salt governs the **stream**, not what a drawn
modifier then does, and this one is read by the carve.

Measuring it is what turned that from a worry into a specification. Diffing the generated
grid at every difficulty over a seed sweep gives **three tiers**, not two:

1. **The doors move the building.** Not merely doorway roles — on seed 42 a **duct
   mouth** relocates. A ±N comparison that draws this entry is comparing two facilities.
2. **The locked room re-roles cells the carve already cut.** It folds a doorway's two
   hinges into its panel span (§10.4) and touches nothing else; every differing cell
   belongs to a door the lock is on.
3. **Everything else leaves the grid byte-identical** — the property the arms are worth
   comparing on.

That is pinned by `a_difficulty_draw_moves_the_building_only_where_it_is_meant_to` rather
than by a sentence, because it is exactly the claim that stops being true silently. A new
generation-reaching entry fails that test, which is what makes admitting one a deliberate
act rather than a discovery.

### The asymmetry left behind

All three admissions are `Harder`, so the pool went to **twelve: eight harder, four
easier.** Nothing lies about it — the slider's blurb counts *picks*, not pool depth, so
both ±2 stops still promise exactly the two rules they will bend — but a `+N` run now has
meaningfully more variety than a `−N` one. The easier side is the one to grow next, and
the honest way to grow it is not to relabel a harder rule but to find rules that are
genuinely bent the player's way. That, and #258's difficulty/feeling split, are what this
appendix leaves open.

### The one thing that did **not** change

`automatic_doors` is still, on the sim's own evidence, a **feel** modifier rather than a
difficulty one: win rate flat over four hundred runs, with the run's *character* moving
instead. Admitting it to a pool labelled by difficulty does not settle that — it defers
it. `Harder` remains the safer of the two labels because the structural change (wider
throats, no hand-close) is the harder-direction one and is what §2.3's assertion can
actually hold. #258 is where that question gets answered properly.


## Appendix 50 — The save is a snapshot, because a replay resumes a different game

*(§12.5 saves, §12.4 determinism, §2.2 permadeath, §12.7 the campaign layer, §14 v2's
saves deliverable. The ticket is #514; the menu it grows a row on is #268.)*

§12.5 shipped as an open choice: snapshot the run, or store `(seed, inputs)` and re-feed
them. Determinism (§12.4) is [SETTLED] and the replay machinery already existed — the
shell records every input, the copy-replay control hands the pair around, the sim replays
it — so the replay save was the one that cost nothing to build and a fifth of a kilobyte
to store. It is still the wrong save, and the reason is worth writing down, because it is
not the reason the design doc guessed.

### The measurement that was *not* decisive

The obvious objection to a replay save is restore cost: it re-runs the run. Measured on
the release build, a turn costs about **174 µs** — 3000 turns is **0.52 s** natively, and
a wasm build is slower again, so a long run would resume with a second or more of frozen
page, growing with exactly the runs the feature exists to protect. That is a real cost
and it is not the decisive one: a "restoring…" frame would have covered it.

### The reason that is decisive

**A replay reproduces the run *this build* would play, not the run that was saved.**

The page updates whenever `main` merges. A player who leaves a run on Tuesday comes back
to Wednesday's build, and if a single §12.6 [START] number moved in between — a dwell
chance, a sighting window, a search radius — re-feeding their inputs produces a different
run: the same first move, a guard a cell to the left, an alert rung that never rose. It
does not error. Nothing about the page can tell the player that the run they are looking
at is not the run they left, because from the code's point of view nothing went wrong.

A snapshot has the opposite failure mode, and it is the safe one. It stores the state
rather than a recipe for it, so a build with a different rule restores the *stored* world
and plays on under the new rules; and a build whose state has a different **shape** fails
to decode, which is caught, which is discarded, which is a menu without a continue row.
The version stamp beside it covers the third case — same shape, different meaning —
where the format cannot notice on its own.

That asymmetry is the whole ruling. A save is a promise about the past; a replay is a
computation in the present, and the present moves.

### What it cost

`serde` derives across the state graph — about seventy types, one line each, plus the
`serde` feature on the pinned PRNG so the run's random position rides along. That is a
real dependency in a crate that had two, and it is the price of the snapshot branch. The
codec (`serde_json`) stays in the shell: §12.1's core is still pure, still clockless, and
gains no I/O.

The stored record is ~150 KB for a 300-turn quick-play run against a multi-megabyte
`localStorage` quota — two orders of magnitude of headroom, pinned by a test rather than
by this paragraph.

### Two things the ticket did not ask for, and why they are in anyway

**The input recording rides in the save.** A snapshot does not need it. The shell's
recorder does (§12.4/#411): a resumed run whose recording restarted at the resume would
hand the copy-replay control a script that reproduces a *different* run — the exact
failure this appendix is about, re-entering through the back door. A few bytes a turn
closes it.

**A reload of the run in progress resumes it.** The shell reflects a live run's token
into the address bar (§13.1/#110), so a refresh mid-run arrives carrying that token and
is indistinguishable from a shared link — and #268's rule for a link is *boot straight
in*, which would roll the level fresh and throw away the run. When the named level is the
saved run's own level, the load is read as a reload and the save wins. A token naming a
different level is a genuine link and starts fresh, overwriting the slot forward like any
new run.

### The campaign came along

§12.7 said "nothing persists a campaign anywhere", and #514 scoped campaign saves out to
v3. It turned out to be a handful of lines: the record carries `Option<Campaign>` and the
map screen's marker row, and the only genuinely new idea is that **the run being over is
not the same question as the facility being over** — a campaign facility's verdict is the
middle of the run (§12.7), so only `CampaignStage::is_over` may empty the slot. Two
moments outside the turn loop had to arm a write as well, because between raids no turn
is taken: the map coming up, and a road bought at the hub (#212) — intel spent and then
reloaded away would be a road bought twice.

§12.7's sentence is now about the *slot's lifetime* rather than about nothing existing:
the campaign is persisted in one place for one purpose, and that place is emptied when
the run ends. §2.2 is unmoved.

### What is left open

The debounce numbers are **[START]** and unmeasured against real play: 2 s and a 20-turn
cap were chosen against the 120 ms key repeat, not against a session. The right evidence
is how many turns players actually lose to a killed tab, which needs the feature in
players' hands first.

---

## Appendix 51 — Short-sighted guards: the zones follow the cone, and the arc stays out of it

*(§6.1 the cone and the blind spot, §7.6 two-zone detection, §12.6 the directed pool
and the same-building guarantee, §10.1.9 placement. The ticket is #495.)*

The §12.6 easier side had four entries and a shape problem. Every one of them handed
over **information** (*all vision cones shown*, *full layout known*, *search areas
shown*) or **slack** (*guards: one fewer*) — the family appendix 29 called *"knowledge
or slack without touching the objective"*, and a family with a ceiling: there are only
so many things left to reveal, and each one reveals a little more of the game the player
is there to discover. This is the first easier entry that bends what a guard can **do**.
Four short-sighted guards is a different problem from three ordinary ones — more bodies
to route around, less reach each — rather than another window onto the same run.

The ticket asked for a cone that was **shorter and thinner**. What shipped is shorter
only, and the arc is the interesting half of this ruling.

### 1. The zones are the cone's halves, not two absolutes

§7.6's certain zone was `5` and its glimpse edge was `GUARD_SIGHT_RANGE`, which is `10`.
Shorten the range and the glimpse edge follows for free — but a certain zone pinned at
`5` against a range of `6` leaves a **one-ring glimpse zone**, and past that it vanishes
entirely and every sighting becomes certain. That is an *easier* modifier biting
**harder** on contact than the baseline, which is the one thing a `−N` draw must never
hand back.

So the zones are stated as a proportion of the cone: **certain is its inner half,
glimpse the rest**. At §7.1's range 10 that is exactly `5` and `6–10` — the baseline is
untouched, which is what made this the cheap answer rather than a retune. A
short-sighted guard gets `3` and `4–6`: both zones shorter, the modifier easier in both.
The invariant — every cone has a certain zone at least a cell deep and a glimpse ring
outside it — is asserted at **compile time** over every sight configuration the game can
deal, the way §7.3's radio relations are, so a future cone that collapsed the ladder
fails the build rather than a playtest.

One relation moves and it moves the right way. §7.6 designed Run's gain to be *exactly*
the certain→glimpse distance; against a shortened cone the gain overshoots — 5 cells
against a 3-cell gap — so breaking contact is easier than designed. On the easier side,
that is the correct direction for a break.

### 2. Placement keeps §7.1's own cone

The §10.1.9 turn-one check refuses a guard spawn whose opening cone covers the player's
exit. A **smaller** cone passes *more* spawn cells, so reading the modifier there would
move where the guards stand — and the ±N arms of a comparison would stop being the same
building, which is most of what makes comparing them worth anything.

This was already settled once, for the flank experiment (#410): placement pins
`BlindTier::REAR` for the same reason, on the grounds that *"a paired A/B whose two arms
generate different geometry cannot attribute what it measures"*. The baseline is the
conservative bound in both directions — a cell safe under §7.1's reach and the rear
carve is safe under any shorter cone or any wider blind spot — so pinning costs only the
very-slightly-closer spawns a smaller cone would have allowed. The entry therefore sits
on the **byte-identical** tier of the pool's graded guarantee: same carve, same
placement, same board down to each guard's radio clock.

The ticket asked whether that guarantee had already been bent, since a promise nobody
keeps is worse than either answer. It had — `automatic_doors` reaches the carve — but
#518 had already restated it in three graded tiers, pinned by
`a_difficulty_draw_moves_the_building_only_where_it_is_meant_to`. Nothing needed
correcting; this entry simply joins the strictest tier.

### 3. The arc is not a difficulty knob, because it has no small settings

The obvious reading of *"shorter and thinner"* is that both halves are dials. They are
not. The **range** is an integer with as many settings as you like; the **arc** is a
rung on §6.2's five-tier ring ladder, and a guard has exactly one rung below its own —
`arc 1`, a ~53° wedge where §7.1's is ~90°. There is nothing in between.

The 2×2 that separates them, four temperaments, 100 seeds each, against the stored
baseline:

| arm | balanced | cautious | aggressive | careless | mean lift |
|---|---|---|---|---|---|
| baseline — arc 2, range 10 | 0.35 | 0.53 | 0.40 | 0.43 | — |
| **shorter** — arc 2, range 6 | 0.42 | 0.71 | 0.42 | 0.47 | **+8** |
| thinner — arc 1, range 10 | 0.57 | 0.77 | 0.67 | 0.70 | +25 |
| both — arc 1, range 6 | 0.63 | 0.89 | 0.72 | 0.68 | +30 |

*(win rate; lift in points, meaned over the four profiles)*

**The one available arc rung is worth 25 points, and §10.2 puts a guard at 8–10.** A
single `−1` pick that hands back three guards' worth of pressure is not a difficulty
step — it is a different guard, dealt to a player who asked for *slightly easier*. The
range alone lands at +8, which is one guard: the step the axis actually promises. So the
modifier ships as **range 6, arc untouched**, and a test asserts the 45° diagonal is
still seen so the arc cannot be quietly folded back in later.

**The two halves are not even the same kind of change**, which the win rate hides.
Thinner cuts detections on every profile. Shorter *raises* them for the striking
temperaments (aggressive 728 → **890**, careless 1211 → 1154) — a shorter cone lets a
player get closer before being seen, so a profile that wants to be close takes the
invitation and is caught at close range more often. A thinner cone gives ground back at
every distance; a shorter one **trades distance for proximity**, which is a more
interesting rule and the one that reads as *short-sighted* rather than *inattentive*.

**A finer arc is the real missing knob, and it is its own work.** The cast is already
rational-slope arithmetic (`min_col`/`max_col`/`is_symmetric` are integer rationals) and
the tier trick is a special case of it — arc 2 is the forward quadrant at slope `1`, arc
1 is the same sector clamped to `1/2`. An optional sub-tier slope clamp would give arcs
of `3/4` or `2/3` and make the arc a dial rather than a cliff. Two things constrain it:
the **[SETTLED]** touching ring is currently free *because* the out-of-arc ring cells are
artificial walls and walls are marked seen, so a slope-carved arc has to reinstate the
ring explicitly; and the tier ladder cannot be replaced wholesale, since the player's
~180° and 360° arcs are not slope-bounded forward sectors. So the shape is *keep the
tier, add a clamp inside it* — additive, and `BlindTier` (which carves detection after
the cast) is untouched either way.

### 4. The variant that was tried and dropped: narrow only while moving

*"You cannot scan properly while walking"* — give a guard the thin arc while it steps
and §7.1's full wedge back whenever it stands (dwelling, rotating, held). It reads well
and it is cheap: one bit on the guard, cleared at the head of each decision and set by
the step that follows.

It does not recover what it needs to. Against the both-halves arm it gives back about
**5 of the 30 points** (balanced 0.63 → 0.58, cautious 0.89 → 0.85, aggressive 0.72 →
0.65) and *raises* careless slightly. Guards spend **31.6%** of their turns standing —
measured over 16 seeds × 200 turns — so two thirds of the time the rule is simply the
thin arc, and the wide-arc third is spent on guards that are stationary and therefore
the easiest kind to read and route around anyway.

It also costs more than it looks. The §11.5 overlay **is** the detection set, so the red
would change shape whenever a guard started or stopped walking — a cell flipping between
safe and watched with the guard neither moving nor turning. And it points the wrong way
against §7.5's dwell, which exists as *"a stationary window a Takedown can be lined up
against"* (§153): the standing guard is precisely the one the design wants approachable,
and this hands it back its widest arc.

Dropped. The finer arc in §3 is the better answer to the same question, and it costs no
new rule at all.

### What the batch says, and what it does not

`--modifier narrowed-guard-cones`, four temperaments, 100 seeds each, against the stored
baseline:

| profile | win rate | detections | alert_peak_mean | takedowns | bodies found |
|---|---|---|---|---|---|
| balanced | 0.35 → 0.42 | 848 → 553 | 0.76 → 0.52 | 0 → 0 | 0 → 0 |
| cautious | 0.53 → 0.71 | 722 → 341 | 0.67 → 0.28 | 0 → 0 | 0 → 0 |
| aggressive | 0.40 → 0.42 | 728 → **890** | 1.07 → 0.64 | 22 → 12 | 3 → 2 |
| careless | 0.43 → 0.47 | 1211 → 1154 | 1.18 → 0.76 | 21 → 8 | 17 → 5 |

**The run gets quieter on every profile** — `alert_peak_mean` falls across the board, and
that is the clearest single reading of the modifier: the same raid, less of it noticed.
The win-rate lift is real but modest and very unevenly spread: the **avoidant**
temperaments take nearly all of it (+7 balanced, +18 cautious) and the **striking** ones
barely move (+2, +4). That is the shape §3 predicts — a shorter cone rewards keeping
your distance, and a profile that closes anyway simply meets the cone later and nearer.

Which is also why aggressive is **detected more** while winning slightly more often, and
why both striking profiles land **fewer takedowns** (22 → 12, 21 → 8) and leave far fewer
bodies (17 → 5 for careless). A shorter cone is not a longer approach window: the guard
still watches its full ~90° wedge, it just watches it late, so the run that walks up to
one has less warning rather than more room. The verb gets *harder* to set up, not easier
— the opposite of what the thinner arc did, and worth knowing before anyone reads the two
arms as interchangeable.

**The bot plays this one honestly**, which is worth saying because most of the easier
side it joins is bot-blind (§12.6/#518: three of the four existing easier entries are
no-ops to the policy). The bot builds its danger set from the cones of the guards it can
see, and those cones shrink with the modifier, so the ground it judges safe is the ground
that *is* safe. No cue was needed; what changed is the board the existing cues read.

None of which is a verdict. §13.4's rule stands — the sim is a smoke detector, not a
judge — and *how much easier a −1 draw should feel* is exactly the kind of question
§13.1 reserves for playing it.

## Appendix 52 — Generation-reaching modifiers: three depths, three assertions, and what a carve costs

*(Moved from §12.6, where only the rules remain; this is the argument behind them.
See #452, #232, #236, #518.)*

Several modifiers reach generation, and they reach **different depths** of it
(§10.4/#452, §10.2/#232, §10.4/#236). Every other field is read at runtime or by the
renderer, so a directional assertion can hold the *facility* fixed and vary only the
rule. Each of these is resolved **before** `generate_level` and threaded in as a
parameter — never consulted from a global, or the generator would have a hidden input
and §12.4's determinism would be a claim nobody could check. What they cost differs,
and the difference is exactly how deep into generation they reach.

`all doors automatic` decides what a doorway **is**, so it reaches the **carve**. From
one seed the two settings are two *different facilities*, so "same seed and inputs"
cannot be the frame; the assertion it is held to instead is **distributional** — the
sim's four temperaments over the same seed sweep, both settings.

The **guard count** reaches **placement** only, and that buys back the stronger claim.
The carve is already finished when it is read, and the pieces are drawn from one
stream in one order, so the three settings put the same player, exit and intel in the
same building — and because the guards come off a single shuffled pool by taking the
first *N*, their guard sets are strictly **nested**: fewer is the baseline's guards
minus its last, more is them plus one. The directional assertion is therefore exact
and per-seed — on one facility, more guards watch a **superset** of the cells fewer
guards watch. What does shift after placement is everything drawn from the stream
behind it (the comms console, the radio clocks), so the two settings are the same
*level* played out differently, not the same run.

The **locked room** reaches deepest in one sense and shallowest in another: it runs
*after* placement, on the finished board, because which room holds the prize is decided
by where the crates and consoles landed. And it draws **nothing** — no shuffle, no pick
— so it buys the strongest claim of the three: from one seed the two settings carve the
same building, place the same board down to each guard's radio clock, and differ in
exactly the cells of one room's doorways, where two hinges have become panels. That is
what lets its §2.3 assertion be stated per seed and in the simplest possible form: a
prize a keyless flood reaches at baseline, it does not reach behind the lock.

**Two of these are now in the difficulty pool** (#518), and that is what turns the
paragraphs above from a note about generation into a promise about comparisons. A ±N
comparison used to be two rulesets over one building, flatly. It is graded now, in
three tiers, and the tiers are pinned by a test rather than by this sentence: the
**doors** may move the building (measured, a duct mouth relocates on seed 42), the
**locked room** may only re-role cells inside doorways the carve already cut, and every
other entry leaves the grid byte-identical. A new generation-reaching entry fails that
test, which is what makes admitting one a deliberate act rather than a discovery.

It is also why adding to this set is not free: a modifier that reaches the **carve**
breaks **seed stability**. Dropping the old per-doorway draw shifted the RNG stream,
so every `#seed=N` link shared before #452 now names a different facility. The break
was taken deliberately rather than papered over with a throwaway draw that would have
bought compatibility by making the generator lie about what it does. A
placement-depth modifier does not carry that cost, which is a reason to prefer one
where the rule permits — not a licence to reach for generation by default.

---

## Appendix 53 — Options is a tab, and it is the *only* drawn home of a setting

*(§11.2, §11.6, §12.6, §14 v2. See #513, and #189, #460, #298, #459 before it.)*

Two settings had been camping. The colour theme lived on the help panel's footer as
`theme [n]`, explicitly "until v2 grows an options screen" (#189); the tile renderer
had no surface at all and hid behind `?tiles=1` (#460). The debug session's two
switches — omni-vision and the replay export — had a *fourth help tab* (#459). #513
gave all four a home. Three things about that were not obvious.

**A tab, and the price of it was a label.** `render::help`'s own doc comment had
predicted a tab — "*Options* land as a further tab" — and the first build of this ticket
refused, on arithmetic: the tab bar is one row of a 40-column board (§10.2) sharing that
row with the right-aligned `[x]`, and `[Level] [Abilities] [Options] [Help]` reaches
column 37 while the `[x]` starts at 36. Over by two. So it shipped as a screen of its
own, and that was the wrong call — the two columns were buyable and the screen was not
free.

What buys them is the **Abilities tab's label**, drawn *Actions*: `[Level] [Actions]
[Options] [Help]` ends at 32, four columns clear. The alternatives were worse. Deleting
the gaps between tabs saves three columns but takes away the dead cell between two hit
regions, which is what stops a mis-tap firing the neighbour (§11.6). *Tech* is shorter
still and is **wrong**: §8.3 uses it for the *salvaged* subset (`AbilityId::TECH`), and
the tab lists the innate verbs too, so the label would name a set the page does not show.
*Actions* costs three more columns than *Tech* and says what the tab is.

What the tab buys back is worth more than the columns: settings are one press from the
reference card rather than a surface away, there is no second modal to route input to,
and the panel keeps being the one thing a running game raises. The compile-time check
that decided all of this is still in `render::help`, now with a comment naming what it
decided — and it is the reason a *fifth* tab is not on the table: the next kind of
content is a **section** of an existing tab, not a new one.

**The panel is raised *over* the menu, not switched to.** That answers the ticket's third
open question — how the §12.6 switches stay flippable mid-run once their own tab is gone
— without a pause menu, which the game does not have: mid-run the tab is one `?` away,
and before a run the title screen's `Options` entry raises the panel on it. Leaving
clears `help_open` and touches nothing else, so the menu underneath is exactly as it was
left. It also fixes the input precedence: **the panel is asked before the menu**, by key
and by gesture and by tap alike, because it is the only surface that can have another one
underneath it. That is the one place two modal surfaces stack, and before #513 there was
no such place.

**One drawn home, several keys.** The tempting symmetry was to keep the `theme [n]`
button everywhere it was and add the tab beside it. That is what leaves a setting
with no home: three surfaces drawing a control for one value, three hit-tests, and no
answer to "where does the theme live?". So the drawn control was retired from the help
panel (whose Options tab is one press away) and from the title screen (whose `Options`
row is the door to it). What was **not** retired is the `n` key: it is a §11.6 table
row, it costs nothing, and a one-press legibility toggle is worth having wherever you
are when the screen is hard to read. The distinction that makes both true at once is
that a key is a *shortcut* and a drawn row is a *home* — and it is only safe because
every path lands on one shell seam (`Game::toggle_theme`), which flips the flag and
writes the record. A preference is stored because it **changed**, not because of where
it was changed.

**The campaign map kept its control**, and that is the exception worth stating rather
than tidying. The map has no route to the panel — wiring one is §14 v3 work —
so taking its `theme [n]` away would have left a touch player on that screen with no
way to change the theme at all. Removing a control with nothing in its place is a
regression whatever it does for consistency.

**What did *not* move.** The `?tiles=` flag survives, against #460's own expectation
that it would graduate into the screen. It does a job the screen cannot: a preview
artifact is framed and hash-stripped by its host, so a build stamps the renderer in
rather than being asked, and the flag is the only way to see the text renderer in the
very build the tile renderer is being judged against. What changed is its standing —
it states the renderer for *this load*, the record states the *preference*, and the
load wins. A URL override is deliberately never written back: a link that silently
rewrote someone's stored settings would be the shared-link channel reaching somewhere
it has no business (§13.1's level/debug split, in a smaller key).

**And the debug switches are still not persisted.** They sit on a tab whose other two
rows are written to storage, which is exactly the confusion §12.6 warns about, so
the split is drawn as well as coded: their own heading, in Warning rather than System,
after the widest gap on it. The preference record holds the theme and the
renderer and has no field for anything else — a record that re-armed omni-vision on the
next visit would outlive the session gate the whole debug channel rests on.

**Two things the first draft got wrong, and what they taught.** It printed
a line under the DEBUG heading — *sight only, never the facility* — restating §12.6's
promise where the switches are. On screen it read as a caption bolted to a heading, made
the two sections' spacing differ, and said to a player something they either already knew
or could not act on. The promise belongs in the design doc and in the module that keeps
it, not in the chrome. And the debug **rows** were drawn in Ground while unmarked, a
shade below the preferences, to mark them as a different kind of thing. Ground is the
§11.2 colour of *receding scenery* — it told the eye these controls were inert, in the
one section where a press does the most. Both were the same mistake in two forms:
loading the gate onto the rows instead of onto the heading that names it.

## Appendix 54 — The Dart: aiming by facing, and why the reopened neutralise has no cooldown

*(§2.3/§6/§7.2/§7.3/§8.1/§8.2/§8.3/§8.4/§10.7/§11.5/§11.7/§13.2; #239, and #556 for the
targeting rewrite it was rewritten against. `crates/core/src/state/dart.rs`,
`crates/core/src/ability.rs`, `crates/sim/src/cue.rs`,
`docs/stats/abilities/dart.md`.)*

§2.3 records one ability by name as the thing that broke the old game: *"the neutralise
ability … unlimited range, no cooldown, and it did not consume a turn"*, with
*"auto-target-nearest-visible was the path of least resistance"* as the cause. #239 is the
ticket that proposes building it again on purpose, so that the design can find out whether
a **heavily costed** ranged takedown can exist. This appendix records what the safeguards
turned out to be, and the three places the implementation went somewhere the ticket did
not.

### The aim is the ray, and that is a stronger fence than the cursor it replaced

The ticket was first written around §8.4's tile cursor and then rewritten (#556) around the
facing. The rewrite is the better version, and the reason is worth stating because it looks
like the weaker one: a cursor is *more* steps, so surely it is more cost?

It is not, because a cursor's cost is paid in the **UI** and the failure §2.3 records was a
failure of **position**. Auto-target-nearest asked nothing of where the player stood; a
cursor would have asked for two keypresses, which is nothing either. A dart along the facing
asks the player to *be in the corridor, on the guard's line, pointing the right way, inside
eight cells, and unseen* — every clause of which is bought with movement, turns and
exposure, on the board, where the guards can punish it. The cost lands in the fiction rather
than in a menu.

It also removes the temptation structurally rather than by rule. `dart_shot` walks one line
and stops; there is no candidate set anywhere in the ability for a later edit to start
sorting by distance. That is the appendix 1 ban satisfied by construction, which is the only
way it stays satisfied.

### Stopping and hitting are two questions, and conflating them would be an aiming aid

The dart stops at the first **solid** or the first **guard**, seen or not. It *hits* only if
that guard is unaware (§7.2) and in the player's line of sight (§6). Keeping the two apart
matters: a flight that skipped over illegal targets would shoot *through* the guard standing
in the way, which is both absurd and an aiming aid — the player would not have to know what
is in the corridor to shoot down it.

The stopper is the terrain table's own `blocks_movement`, so the list is not maintained
twice. The consequence worth naming is that **a table stops the dart** even though sight
goes straight over it (§10.3) — the one thing in the game you can see past and not shoot
past. That is load-bearing rather than an oddity: §10.1a stamps partial cover into every
over-long straight run, so the long clear firing line down a corridor is precisely the
geometry the sightline rule stops a level being *born* with. The generator bounds the
corridor shot before `DART_RANGE` does.

A loose body and the player's own decoy are flown over. Neither is terrain, so neither is
consulted — but the decoy is worth a sentence, because a dart that stopped on one would
spend the facility's only shot on the player's own prop, and that is a joke a player gets
exactly once.

### The clamp: a case the first implementation reasoned itself out of, wrongly

The reach fired is `min(DART_RANGE, sense_range())` — Confusion's **[SETTLED]** clamp,
borrowed for a different reason. The reason is the §11.5 wash: the flight is painted, and a
line that stopped six cells short on a guard the player could not perceive would report a
body in the dark.

The first version of the module removed that clamp, having proved it redundant. The proof
was good and the conclusion was wrong, and the shape of the mistake is the reason this
paragraph exists. Every terrain that blocks **sight** also blocks **movement** — wall,
hinge, closed panel, duct entry is the whole opaque set — so along a cardinal nothing can
break the sightline without also stopping the dart; with `DART_RANGE` shorter than the §5
sight range, every cell an open-floor dart reaches is a cell the player already sees. All
true. What it missed is the **crawlspace** (§10.7): a duct's interior cells are walls, but a
mouth has a floor neighbour, so a dart *can* be fired out of one — while a crawler's live
sight is the mouth peek alone, and a mid-duct cell sees nothing at all. Unclamped, a crawling
player's dart flew eight cells into a room they had no picture of, and the wash reported what
it found there.

So the clamp stays, inert on open floor (`min(8, 10)`) and cutting a crawler to
`DUCT_SENSE_RANGE` = 5, inside which every guard is at least a §9 dot. The general lesson is
the one §10.7 keeps teaching: a rule proved from open-floor geometry has to be re-proved for
the duct, because the duct is where perception stops matching position.

### The cooldown the ticket asked for does not exist, and the budget is why

#239 lists among its §2.3 safeguards *"a very large cooldown — a named constant, exaggerated
on purpose"*, and in the same acceptance criterion asks that every cost be **shown to the
player**, predicting the bar will read `(1)` then `—`. Those two cannot both happen, and
three surfaces say so independently:

- **The bar.** `Deck::state` ranks a spent budget *above* a running cooldown, deliberately,
  because *"a cooldown on an ability that is never usable again is a countdown to
  nothing"*. With one use a level, no `/60/` is ever drawn.
- **The help panel.** Its economy line has 34 cells.
  `1 turn · instant · 60 cooling · 1 a level` needs 41.
- **§8.2 and Pierce Wall.** The use budget is the *one non-time axis*, and Pierce Wall's row
  already argued this exact case: *"adding a cooldown on top would only blur which number
  the player is actually managing."*

So the row carries no clock at all. **The safeguard is not dropped, it is met by something
stricter**, and appendix 43 already wrote the sentence for the Saver: one use per facility
*"is stricter than any cooldown can be: a lockout ends, and this does not."* A 60-turn
lockout would let a 200-turn run fire three darts. `1/level` lets it fire one, for ever.

The consequence to keep in view is that `DART_USES` is now the *whole* economy, so raising
it to two is not a tune — it is a second ranged takedown with nothing to space it from the
first, and it goes past the sim and the kill-thresholds on
`docs/stats/abilities/dart.md` before it goes anywhere else.

### A miss is never refused, and never differentiated

Nothing on the line, an aware guard, and a guard the player can only sense all spend the
turn and the level's dart, and all three read as the **same** near line. Two things follow
from one rule, and the rule is §8.3's for False Call: an ability refused for want of a
target — or a bar entry that greyed when the line was empty — answers *"is there a guard in
front of me?"* for free, every frame, without spending the turn. That is a detector. It
bites harder here than it does for the spoofer, because the answer a dart's detector would
give is worth a takedown rather than a search.

The near line therefore has exactly two things to say. On a hit it ranks *above* the
`TakenDown` travelling beside it, and it says **where**: every other takedown in the game
leaves a body at arm's length, and this one leaves it several cells down a corridor, already
counting on the §7.3 clock, often somewhere the player cannot walk back to. That is the
ability's honest counterweight, and a line reading only *"the guard drops"* would report the
adjacent verb's cost instead of this one's.

### What the sim was asked, and the cue bug that made the first answer worthless

The instrument is `Verb::Dart` in the §13.2 histogram, read against `Verb::Takedown` — a
dart hit moves both, so the gap between them is how often the shot missed. The bot's cue
asks the one question that separates this ability from the §7.2 verb it copies: *is this a
guard I could not have reached on foot?* — never in flight (a hunter has seen you, so it is
not a legal target at all), never adjacent (the free verb is strictly better), and only when
the target's own cone watches the ground the bot is heading for. Without that last clause
the cue would spend the level's dart on the first patrol in front of it and the histogram
would read *used* while measuring nothing, which is #347's failure mode at its sharpest,
because here the press can never be refused.

**And the first version got a more basic thing wrong, which is worth recording because of
how convincing its output was.** `balanced` and `cautious` ship `takedown_reach: 0`, and
that knob's doc is explicit about what zero means: *"not 'never gets the chance', but 'does
not want it'"*, with *"a cautious profile reporting `takedowns: 0` is **correct behaviour**,
not a defect"*. The cue ignored it. So an avoidance-first bot started taking guards down at
range — 12 times per 100 runs on `balanced` — and every metric for those two profiles was
then a **changed bot measured against the old bot's baseline**, which is §13.3's failure
mode rather than a tuning question.

What makes it worth an appendix is that the wrong numbers looked like the *right* answer.
`balanced` went 0 → 12 takedowns, left 11 bodies where its control leaves none, and its
`alert_peak_mean` rose 0.76 → 0.95 — which reads exactly like this ability's design claim
coming true: the unreachable body pays for the takedown. It was reported that way. The tell
was there to be seen and was not: **a profile whose control column says `takedowns: 0` had
started leaving bodies.** A with/without pair can only be read when the two sides differ in
the one thing under test, and a cue that overrides a temperament breaks that quietly, in the
direction of the hypothesis.

The fix is that the dart is gated on the same appetite the walk-up strike is
(`Moment::strikes`, threaded from `takedown_reach` as a fact about the plan, like `Intent`),
so a temperament that declines the verb declines it **at every range** — and the
avoidance-first profiles are byte-identical holding the ability, which is the property #316
already guarantees for the adjacent takedown. What is left is a much thinner measurement,
honestly labelled: two profiles decline it, and the two that take it fire 2–11 darts per 100
runs.

**The design question the fix exposes is left open on purpose.** Borrowing `takedown_reach`
is right for *this* ticket, because that knob is the only statement of takedown appetite the
profiles have. But the two verbs do not obviously belong on one axis: a walk-up takedown asks
for a detour into a guard's cone, and a dart asks for none — so the player who least wants to
walk into a blind spot is arguably the player a ranged takedown is most *for*. If the Dart
deserves an appetite of its own, that is a §13.4 temperament decision and its own ticket, and
until it exists the sim simply has nothing to say about the avoidance-first half of the
roster. `docs/stats/abilities/dart.md` says so in those words rather than filling the gap
with the numbers the bug produced.

## Appendix 55 — A composite is one word for a combination, and its rows are its receipt

*(#565, §12.6/§14 v3 — the flavours went from one slot per field to one slot each. Two
questions had to be answered first, and the second one is the interesting one.)*

**The problem is not slots, it is simultaneity.** The level-seed token reserves 256
permanent slot positions and spends twenty-nine, so *"appending is free"* is literally
true; what the format actually bounds is `MODIFIER_CAP` — **five** modifiers active at
once — and that number is not free at all. It is what keeps `PAYLOAD_SPACE` under
`TOKEN_SPACE`, which is the whole of the format's integrity argument: there is no
checksum field, and a bad token is rejected exactly because most of the token space is
unused. Every bit spent on a sixth simultaneous modifier is a bit taken from that.

So a Vault — *one more guard, one more console, three crates* — spent **three of five**
on a combination the game already has a word for, and the campaign's own drawn rules
(#210) stack at conditions 0, 2 and 3 on precisely those facilities. Raising the cap
looks like the same fix and is not. Saying one thing once is free; raising the ceiling
is not.

### Question 1: what happens when a composite and a drawn rule touch the same field

Three options were on the table, and two of them fail for stated reasons. **Rejecting the
combination** — the way a knob's two ends are rejected together — would make the
campaign's own stacking illegal, which is the one thing the mechanism exists to enable.
**Last writer wins** is silent and unpredictable, and a drawn rule that quietly does
nothing is the §2.3 facade.

What was taken is **deltas over the baseline, clamped**: a Vault's `+1` and a drawn `+1`
are `+2`. The first cut of this shipped that as a *framing* and not a behaviour — the
guard envelope was three-either-side-of-four, the knob's documented reach was ±1, and the
second step clamped straight back onto the same fifth guard, so `plus` and the old
harder-ward rule computed the same answer on every input the game could produce. That was
wrong, and the reason it was wrong is worth keeping:

> A composite brings its **own version** of the modifiers. It is not a claim on a knob
> that the highest bidder wins; it is a source, saying what this facility is, alongside
> every other source. Two sources each putting a guard in the building are two guards. A
> rule that arrives second and lands on a rung the first already took has changed nothing
> observable — which is exactly the §2.3 test a shipped modifier has to pass, applied to
> the second source instead of the first.

So the **count** knobs are genuinely signed deltas now, composing by addition with a reach
of **two steps either way** — one rung per source that can name the knob, since a facility
has one flavour and one alert and there is no third. §10.2's guard envelope widened from
3–5 to **2–6** to make room: the sixth guard is what a Vault the campaign has also been
loud on costs, and the second guard off is what an Outpost taken quietly buys. Both are
reached only by paying twice, which is the shape §14 v3 wants its map to have.

The **intel** knob is arithmetic too and its envelope did **not** move, which is the one
asymmetry here. Nothing but a node flavour names it — neither of its ends is in the §12.6
directed pool, deliberately (§12.6: a difficulty draw that moved the intel count would be
deciding how much of the game you have to do) — so its two-step rungs are unreachable in
play, and §10.2's arguments for the console floor and the §10.6 carve's ceiling stand
untouched. Consistency in the *rule*, not in the numbers: if the pool ever admits an intel
rule, that envelope is what has to be argued again.

**What arithmetic costs.** §12.6's older invariant — *no contribution can relieve pressure
another one asked for* — does not survive it. An Outpost's `-1` and a drawn `+1` now sum to
the recipe's own count, where before the drawn rule won outright. That was a real
protection: it is what stopped a player's easier choice from talking the campaign alert out
of its extra guard. It is given up here because a **count** is the one kind of knob where
the arithmetic is the honest reading — a facility told to hold one fewer guard and one more
guard holds the number it started with — and because the tab now shows both rules, so the
sum is something the player can see rather than something the seam does behind them. The
invariant is untouched wherever it still means something: the intel gate, every toggle, and
the layout knob compose exactly as they did.

### Question 2: a composite has no direction, and the tab must not lie about it

Every `LevelModifiers` field carries a documented **harder / easier** direction with a
§2.3 directional assertion behind it, *"so a flag that changes nothing observable cannot
pass for shipped"*. A Vault is harder **and** richer — that is the entire point of §14
v3's three axes — so it cannot claim a direction, and forcing one would make the panel's
colour cue lie about half of it.

What replaces the directional assertion is stronger: **equivalence**. Each composite's
expansion is asserted field by field against a *frozen copy* of the combination it
replaced — frozen rather than re-derived, or the test would assert that the code equals
itself. If that holds, nothing about any facility differs afterwards, which is the whole
claim the change makes. The direction is then inherited by the **parts**, which keep
their own; a Vault's guard row is orange and its console row is blue, adjacent.

### The display: one row per rule, one rule per row

The Level info tab stays what it was — a flat list, one row per active rule — with the
composite's name in front of each row it set. Not a label with rules behind it: §12.6
promises the drawn rule is *"on the help panel's Level info tab with every other active
modifier"*, and a word standing in for three rules would be concealing two of them.

Two details fell out of building it. The rows are **derived twice from one derivation**:
a composite's rows come from running the caption derivation over its expansion, and the
rest come from running it over what the run asks for *beyond* the composite — the same
`departures_beyond_composite` the encoder writes its slots from, so the rows drawn and the
slots written can never describe different runs. And the rows needed a **second wording** —
the existing captions are `Guards: one more`, and `Vault: Guards: one more` is both ugly
and two columns past the panel's width bound on the v1 board. So each caption carries a
short phrase (`one more guard`) used only when it is drawn under an owner, and the
compile-time bound measures the longest label against the longest phrase. The tab can then
only grow **downward**, which is the direction it was built to grow in.

The ticket asked for two rows where a composite and a drawn rule touch one field:

```
Vault: one more guard
Guards: one more
```

and that is what it draws — because with deltas both rules are genuinely in the building
and the rows add up to it. The first cut drew **one** row here, on the grounds that under
the clamp the two rules were one guard and a second row would be a receipt for nothing.
That reasoning was sound *given* the clamp and wrong because of it: the fix was to make the
second rule real, not to stop reporting it. Which is the useful shape of the mistake — a
display rule quietly documenting a composition rule that had gone wrong upstream.

The rule that follows is worth stating plainly, because it is what makes a row worth
reading: **no row stands for a rule the building does not have, and no rule of the
building's is missing a row.** A cancelling pair is drawn as a pair — `Outpost: one fewer
guard` above `Guards: one more`, on a facility with the recipe's own count — because both
rules are in force and the sum is the player's to do.

### What made it a kind rather than a special case

The flavours are not the last preset this game will want; a difficulty preset, a tutorial
preset and a sim scenario are the same shape. So a composite declares a **name** and an
**expansion** and nothing else, expands at resolution — so no system below
`ModifierSources::resolve` learns the word *Vault* — and a second one is a new variant
with a new slot. Two smaller decisions inside that:

- **The Archive is a composite**, which #565 left open. What reaches the facility is a
  §12.6 flavour contribution exactly as the others' are, and as a raw combination it cost
  two slots. That the archive is *also* the campaign's terminal objective (#217) is a
  property of the **node**, and stays there. One word, two facts, kept in the two places
  that own them.
- **`Flavour` and `Composite` stay two enums.** A flavour is a property of a node on the
  map — what a branch offers, what its blurb says, which one the terminus is. A composite
  is a modifier *value* with a permanent wire slot. Collapsing them would put the map's
  vocabulary in the token's slot list and invert the crate's layering; the pairing is
  pinned by a test instead.


## Appendix 56 — The archive: the ending as one composite, and the gate that is the node's

*(§2.2/§2.3/§4.5/§6.2/§10.2/§10.4/§12.6/§12.7/§14 v2/§14 v3; #217, over #207's map,
#236's lock and #565's composites. `crates/core/src/modifiers.rs`,
`crates/core/src/campaign.rs`, `crates/core/src/campaign/map.rs`,
`crates/core/src/vision.rs`, `crates/core/src/place.rs`.)*

§14 v3's first complaint about the old campaign is that it *"had no reachable
conclusion"*. #207 gave the graph a terminus and left it deliberately thin — *"what the
archive actually holds, and what reaching it concludes, is #217's"* — with a placeholder
flavour of one extra guard and the hideout search. This appendix records what the ending
turned out to be, and the two decisions that were not obvious.

### What the archive is

| | |
|---|---|
| **+2 guards** | the count knob's whole reach, from one source — six on the §10.2 recipe |
| **+2 consoles** | five, and every one of them required to leave |
| **one locked room** | #236's rule; with no crates it falls on a *console* room |
| **guards watch their sides** | the §6.2 flank carve withdrawn, in every mood |
| **no equipment caches** | salvage on the last facility buys nothing |

Read together they are one statement — *the last raid is the hard one, and you cannot
leave it empty-handed* — and the locked room is the sharpest clause. With every console
required and one behind a key, **a won run is a run that committed a takedown** (§7.2;
the protagonist never kills, so the price is a body to hide and a §7.3 radio clock to
outrun). That is the ending asking, once and at the end, for the thing the game prices
highest.

### It arrived one merge after the mechanism it needed

The first implementation of this ticket was written against `main` as it stood, and it
hand-rolled a composite: a single `archive: bool` field expanding at every read site,
with accessors for the gate and the lock and a bespoke rule for the guard row. It was
sound, and #565 landed while it was in review with a *general* answer to the same
problem — composites as a kind, count knobs as signed deltas, expansion at resolution.

The work was rebuilt on it rather than merged beside it, and that is the note worth
keeping: two mechanisms for *"one word for a combination"* in one codebase is exactly
the parallel system §12.6 exists to prevent, and the cost of rebuilding (a day's diff)
was smaller than the cost of the reviewer who would later have to ask which one to use.
What survived the rebuild was the design — the six clauses, the numbers, the reasoning
below — and what was thrown away was the machinery, which is the right half to lose.

The dividend is visible in the diff: on #565's seam the archive needs **one new
primitive** (the ring rule) and one changed `expansion` arm. Everything else — two more
guards, two more consoles, the lock — was already sayable.

### The gate belongs to the node, not to the composite

The one genuine design question. The archive's mandatory objective could ride in
`Composite::Archive`'s expansion, which would make the whole terminus travel in one wire
slot and let a shared token play the ending anywhere.

It does not, and the rule it respects is #565's: **no composite sets the intel gate**. A
gate is a *mode* knob (§4.5/#244) — it says what the **run** is asked for — while a
composite says what a **facility** is. The archive is both things at once, and that is
precisely why the line has to be drawn rather than blurred: what makes the terminus hard
is a property of the building (guards, locks, sightlines), and what makes it *the end* is
a property of the **node** — there is nothing past it to spend a surplus in. So
`Campaign::level_at` sets `IntelGate::All` at the archive and `IntelGate::None`
everywhere else, and the composite carries the five clauses that are about the building.

Both halves still travel: the composite in its slot, the gate in its own token field. A
campaign facility's token is the facility as it was actually played, terminus included.

The cost, stated: a quick-play level carrying `Composite::Archive` would get the archive's
*building* and not its *ending*. Nothing offers one today, and if something ever does, the
honest answer is that it is playing the archive's facility rather than finishing a run.

### Two numbers moved, and one of them is a claim about the carve

**`INTEL_MAX` 4 → 5.** The intel knob's envelope was one step wide upward, and #565's own
note said why: *"nothing in play asks this knob for two"*. The archive does. That bound is
a claim about what a 40×40 carve can seat rather than about balance, so it was measured
rather than argued: five consoles *and* a locked room place with no rejection over a
60-seed sweep (`the_archive_recipe_carves`). Nothing below the terminus can reach it —
the ±1 sources cannot sum past one step upward on any facility the map offers — so the
widening costs the ordinary game nothing.

**Guards top out at six.** #565 had already widened the envelope to 2–6 for two stacking
sources; the archive spends the whole reach from one. An alerted terminus therefore does
not become seven, and an *easier* draw still bites — the ceiling is a clamp, not a floor
under the arithmetic.

### The ring rule is a new slot, and it stays out of the pool

*Guards watch their sides* is the **harder arm** of the carve #442 settled the other way:
slot 5 asked for a Calm guard to detect **only** its cone, that became the rule, and the
slot is a tombstone. Reviving it would re-point every token that ever named it at the
rule's own inverse — the #286 break in its purest form — so the arm is a new field at slot
29, exactly as the tombstone's own note said it would have to be.

It is kept **out of the §12.6 directed pool**, and this is the one entry there kept out on
evidence rather than on mechanics (#518's two questions would both admit it). Appendix 28
measured the unconditional carve as a real mover — the conditional rule was adopted
*because* the unconditional one gave avoidance-first play a win-rate rise with no new
decision attached — so putting the harder arm in the draw would re-open that call by
lottery, on runs nobody asked for. At the terminus it is a stated property of one
facility, which is a different thing from a rule a `+2` run can be dealt.

### Appending a primitive past the composites broke an unstated invariant

Worth recording because nothing pointed at it. `SlotSet::push` requires slots in ascending
order, and `modifier_slots` satisfied that by walking the primitives and then the
composite — which held only because every composite slot (24–28) was above every primitive
one. Slot 29 ended that silently: the encoder built a descending set and `ordinal`
returned `None`, so an archive token simply refused to encode.

The fix is to **sort**, not to renumber, and it changes no token: every set the old order
could build was already ascending. The lesson is the general one about permanent slots —
"the newest thing is the highest slot" is a fact about a moment, not an invariant, and any
code relying on it should say so or stop relying on it.

### What was dropped, and what is deliberately left thin

The placeholder's `guards_always_search_hideouts` is **gone**. It was #207's stand-in for
"hard", and the five clauses are a stated ending rather than a pile of pressure; keeping a
rule because the placeholder had it would be keeping a number nobody designed. If the
terminus plays soft, it is the cheapest thing to add back — after it has been played.

**A depth-zero campaign is no longer the plain game.** It used to be the degenerate country
§12.7 names — one facility, entered and left, *"the game v1 already ships"*. That one
facility is the archive, so it is now the shortest possible *campaign*, and the tests that
used it as a cheap single level had to say which of the two they meant.

**The ending's screen is the ordinary one, on purpose.** A won run draws §14 v2's end
screen — `ESCAPED`, the ledger, the seed — with the campaign's single exit back to the
title (appendix 31). #138 already tells a win from a loss, and the run-win is its
campaign-won case. A bespoke victory screen is a thing to build once there is a run worth
ending, which is the same reasoning that keeps the archive itself free of set-piece
geometry (§2.3, *find the fun first*).

**The §13.2 bot cannot play it, and that is reported rather than papered over.** Under an
all-consoles gate with a locked room the bot's win rate is zero (#236's note; #517 is the
cue it lacks — no plan of its says *the thing I need is behind that door, so buy the
key*). So neither the archive nor its ring rule takes a `--modifier` name in the sim, on
the cache count's precedent (#209): a batch that drew one would measure the bot, not the
game (§13.3). The terminus will be judged by playing it, which is §13.1's rule and not a
gap in this work.

### One legibility wrinkle, left as it is

On the Level info tab the archive's *two more consoles* row draws in the **easier**
colour, because a row keeps the direction of the primitive it came from and a console is
loot on every other facility (§2.2: intel is currency). At the terminus it is an errand.
Giving a composite the power to recolour its parts would be the label-over-rules move
#565 refused, and the row beneath it — *Intel to exit: all of it*, in the Warning colour —
is the fact that makes five consoles hard, stated where a player reads it. So the wrinkle
is documented rather than special-cased.

## Appendix 57 — The ghost: the first debug switch that touches the facility, and the four walls round it

*(§4.5/§7.6/§10.3/§11.5/§12.4/§12.6/§13.2; #507, over #459's debug session and #513's
Options tab. `crates/core/src/modifiers.rs`, `crates/core/src/state/view.rs`,
`crates/core/src/render/settings.rs`, `crates/web/src/input.rs`,
`crates/web/src/replay.rs`.)*

§12.6 said it three times and meant it: **nothing behind the debug gate may ever touch
the facility, only the picture.** The ghost switch — while it is on, no guard detects the
player — breaks that, on purpose. This appendix records why the exception was taken, what
was tried instead, and the containment that replaces an absolute rule with a priced one.

### Why an exception at all

#459's argument was that *a deployed build you cannot inspect is a build you cannot
debug*. Omni-vision paid that off halfway: you can now **watch** a run that misbehaved.
What you still could not do is **stand in one**. Walking to the corner where generation
went wrong, parking in a guard's face to read its cone maths, following a patrol for
twenty turns — every one of those is a run the facility ends before you arrive, and every
one of them is a thing someone actually needs to do to a bug report. An instrument that
only works on runs the guards happen not to notice is an instrument with a hole in it
exactly where the interesting bugs are.

The counter-argument is the one §12.6 spends its length on: a bent run masquerading as a
real one. That is a real risk and it is not answered by being careful. It is answered by
the four walls below, and if any of them had turned out to be unbuildable the switch
should not have shipped.

### The seam: camouflage that never lapses

The first thing tried on paper was a `ghost` check inside the guard AI — skip the sight
pass, skip the sighting count, skip the chase transition. That is three checks that must
each be remembered by the next person to add a way of noticing the player, and §12.6's
whole complaint about scattered rules applies with more force to a rule nobody plays
under.

What shipped is one clause on `State::concealed_from`, the §10.3 path Camouflage already
goes through. Ghost is exactly *Camouflage with the still-turn condition removed*, and
everything follows without a second thought: the guard sense pass reads the player as
concealed, no `Event::Detected` fires, the §7.3 sighting window never counts a contact,
and a chase flipped on mid-stride ends through the ordinary §7.6 lose-sight path with the
guard searching where it last had you.

**It drags Camouflage's consequences with it, and that is accepted rather than patched.**
`guard_detects_now` is also the §7.2 takedown gate, so a ghost may strike a guard from
the front — which a camouflaged player already may. Carving that out would mean a second
rule that has to be kept in step with the first, to protect a property nobody relies on
in an instrument. One seam, one set of consequences.

### Contact still captures, and it is not a caveat

The design already had the sentence, on Camouflage: *"Camouflage does not stop capture.
**Invisible is not safe** (§4.5)."* Ghost changes nothing about it. A guard that walks
into your cell ends the run whether or not it ever saw you coming, so walking a ghost
through a patrol route is still a way to lose. This falls out of the seam for free —
capture is contact, and contact never consults concealment — which is a small piece of
evidence that the seam is the right one.

### The four walls

- **Never in the token.** The cost is stated rather than buried: **a ghost run is not
  reproducible from its token.** The token stays honest about the *facility*; what it
  stops being is a full account of what happened. Putting the switch in the token would
  make it reproducible and would also make it a level modifier — see the last section.
- **Never in a replay.** Considered and rejected: teach the replay link to carry the debug
  flags. That is a rule-bend inside a shareable URL, which is the exact thing §12.6 keeps
  out of the token, so the export is **refused** for any run that has had the switch on.
- **Never in the sim.** Structural — a `RunConfig` carries a `LevelSeed`, and the switches
  are not on it, so there is no flag to add. Asserted at the boot path anyway, because
  what matters is the state the sim actually gets.
- **Visibly on**, on the Options tab's own row, read live off the run.

### The latch, and why it is on the run

The refusal latches on the **run**, not on the switch: turning ghost back off does not
restore the export. The inputs already recorded were played under bent rules and no later
toggle un-bends them. This mirrors what omni-vision's own row already says about tile
memory — *turning it back off does not restore what was already seen, which is honest
rather than surprising: you did see it.*

The flag therefore lives on `State` and rides the autosave, rather than on the shell: it
is a fact about the run, and a shell that forgot it could not weaken the guarantee.

**The cost is real and worth taking knowingly.** The session most likely to want an export
— someone poking at a run that misbehaved — is now the one that cannot produce one if they
reached for ghost first. Reach for omni while a run is still worth handing over; ghost is
the switch you use once you have given up on that.

### The half the ticket did not name: watching a replay

The export refusal closes the *production* side. It does not close the viewing side:
`state_at` rebuilds a replay from `(level, inputs)` and applies **the watching session's**
switches, so a ghost session watching somebody else's replay would desync just as badly,
with nothing to catch it. So the re-simulation takes `DebugModifiers::perception_only()`
— the ghost dropped, the reveal kept.

Together the two are what keep §12.4's **[SETTLED]** *"a replay is `(seed, inputs)` and
nothing else"* true now that a switch can bend a rule: a rule-bending session can neither
produce a replay nor alter one it watches.

### The overlay lie, taken deliberately

§11.5 is **[SETTLED]** that the danger overlay is the *literal* detection set. Under ghost
that set is empty, and a literal reading blanks the board — which deletes the overlay
exactly when someone is debugging vision, the likeliest reason to have flipped the switch
at all. The alternative to a lie here is an instrument that goes blank when you use it.

So the overlay carries on painting the set that *would* detect a detectable player, and
red goes back to meaning *this cell is watched* rather than *you are detected*. It is
implemented as a second, ghost-free reading of the same concealment query, so with the
switch off the two are the same function and no ordinary run can tell the difference.
Both `visible_cone_cells` and `watcher_lines` read it, so the two halves of the red layer
stay one claim.

### What the switch is not, and must not become

**Do not let ghost creep into being a level modifier.** If it ever wants to be *playable*
— an easier-direction "unseen" modifier — that is a different thing with a permanent token
slot, a Level info caption, a §2.3 directional assertion and a place in the difficulty
pool. The two must not be the same field wearing two hats: one is priced by the difficulty
draw, the other by giving up the run's reproducibility, and those are different currencies.

**And it should not be oversold on its own row.** A ghost session cannot reproduce any bug
whose trigger is *being detected* — chases, searches, the §7.7 call-ins, rung escalation.
It is an instrument for looking at the world, not at the threat model.

### One surface note

The ticket was written against #459's fourth **Debug** tab and asked for a key that did
not collide with `help_nav_for_key`. #513 had already retired that tab into the Options
one, where the switches are rows walked with `↑`/`↓` and fired with `Enter`, so there is
no key to pick and the collision question is moot. The row it disables — the replay export
— stays **drawn** and reads `unavailable` rather than vanishing: a row that disappeared
would look like a run with no token, which is the other reason that row is absent, and
would leave nothing to press and no way to be told why.

---

## Appendix 58 — Switching the sense off: suppress the channel, never the range

*(§9 the sense and §9.4 its door half, §9.5 the one fade, §8.3 Confusion's clamp,
§10.7 the crawlspace's cost, §11.5/§2.2 the fairness floor, §12.6 the directed pool.
The ticket is #493.)*

The *"nothing felt through walls"* modifier takes away the §9 sense whole: no guard felt
through a wall, no door-change cue, for the whole run. It is the first level modifier
that bends what the **player perceives** rather than what the world does, and that
difference is what the three findings below are all about.

### 1. The clamp is why the range had to survive

Confusion's reach is `min(CONFUSION_RADIUS, sense_range())` — a **[SETTLED]** clamp
whose wording is *"the blast can never freeze what you cannot sense"*. A modifier that
implemented "the sense is off" as `sense_range() == 0` would have zeroed the blast with
it: a level modifier silently deleting a verb from the loadout, which is precisely the
dead-ability case §13.2's histogram exists to catch. The Dart reads the same clamp, and
Lockdown's radius is bounded against the same constant.

So the modifier suppresses the **channel the player perceives** and leaves the **rule
input** alone. `sense_range()` and `door_sense_range()` keep their values; the two seams
that read them for the player — `perceive_guard`'s `Sensed` arm and the door-cue pass —
are what the flag is read at. Confusion therefore still fires at full reach and still
freezes every guard a sensing player would have sensed; what the player loses is the
sight of it landing. The clamp's [SETTLED] wording stays literally true, which is the
point: a rule stated over a range survives a modifier aimed at a channel, and one stated
over "what the player can perceive" would not have.

The general lesson is worth keeping: **a suppressed channel is not a shortened one.**
Zero is a value on the ladder and rules read the ladder; absence is a fact about the
view. Conflating them makes every clamp in the game a hostage to a display decision.

### 2. What it must not touch, and why the list is short

The bound on this modifier is §2.2 — *you may not be captured by something you could not
perceive*. So it removes **scouting** information and never **fairness** information: a
seen guard, its facing, its cone and the §11.5 danger overlay are identical to baseline
cell for cell, and so is the standing watcher line of an unseen guard that has you
(#465). That line looks like the sense — an unseen guard's position, through walls, for
free — and is deliberately not it: it is §11.5's promise that a watched cell reads as
watched, and it becomes *more* load-bearing here, not less, because it is the last cue
left that says *something has you*. Neither rule is to be later "fixed" to match the
other.

The **duct** loses half its price, and that is accepted rather than patched. §10.7
charges a crawler in information — blinded sight plus a sense shrunk to
`DUCT_SENSE_RANGE` — and with the sense already off there is nothing left to shrink, so
ducts are relatively *safer* under this modifier. Special-casing the crawlspace to
restore the gap would invent a second knowledge rule to paper over the first one's
consequences, which is the move §11.5a's fog note already refuses for this same channel.

**Wait keeps its sight half.** §8.3 calls it *"the only way to see behind you"* and §9.1
gives it a widened sense as well; here the 360° look stays and the widening does nothing.
Measured against the innate-verb floor (appendix 25) that is a verb narrowed, not
hollowed: it still buys the one thing no other verb does.

### 3. The bot feels it — hard — and that is a number to read carefully

Appendix 49 explained why bot-blindness clusters on the pool's easier side with a rule:
*a harder modifier lands on the bot whether it understands it or not; an easier one has
to be used.* A modifier aimed at the player's own perception looks like the counterexample
— and it is the opposite. The §13.2 policy is player-honest by construction: it perceives
guards through `State::perceive_guard`, and the **sensed** arm feeds five separate
decisions — the route's keep-away cost, the flee radius, the hide-until-clear hysteresis,
which bench it takes cover behind, and the `nearest_guard` every ability cue is weighed
against. Take the channel away and all five go quiet.

100 seeds per profile, against the committed baseline (the default batch is unmoved —
this modifier is off in every run the baseline pins):

| Profile | Win rate | Captures | Detections | Diversity | Turns to win (mean) |
|---|---|---|---|---|---|
| balanced | 0.35 → **0.21** | 62 → 79 | 848 → 459 | 0.60 → 0.56 | 157 → 108 |
| cautious | 0.53 → **0.19** | 44 → 80 | 722 → 820 | 0.34 → 0.55 | 272 → 106 |
| aggressive | 0.40 → **0.14** | 60 → 84 | 728 → 1337 | 0.57 → 0.45 | 149 → 89 |
| careless | 0.43 → **0.14** | 56 → 84 | 1211 → 1293 | 0.50 → 0.40 | 150 → 91 |

Every profile loses, and the direction is unambiguous — which is the §2.3 assertion this
entry owes, held in the strongest frame available (the same seed builds the same board,
so the two arms differ in the rule and in nothing else). The **magnitude** is the part to
distrust. Three readings, in order of confidence:

- **It is a large step, larger than any other harder entry.** §10.2 puts one guard at
  8–10 points of win rate; this is 14 to 34. Appendix 51 rejected the narrower-arc rung
  at ~25 points on the grounds that it *"reads as a different guard rather than an easier
  one"*, and the same question is open here in the harder direction.
- **The cautious profile falls furthest, and that is the tell.** It is the temperament
  whose whole game is waiting for a gap it can *see coming* — 53% to 19%, and its winning
  runs go from 272 turns to 106, because the patient runs are now the ones that get
  caught. Its diversity *rises*, which is not a health signal: with the channel it
  planned around gone, its runs stop resembling each other because it is improvising.
- **The bot has no counterplay and a player does.** A blinded human slows down, spends
  Wait for the 360°, hugs walls, peeks corners, and treats every unseen doorway as
  occupied. The policy has no cue for *"I am blind now, be more careful"* — it plays its
  ordinary game with five inputs reading empty. So the table is best read as an **upper
  bound on the harshness**, not an estimate of it, and the verdict on whether this is
  hard or merely unfair is a human one (§13.1/§13.4).

That last point is why the entry ships in the pool with the measurement recorded rather
than being softened on the strength of it. If play shows capture becoming *unreadable*
with this on, the finding is about the §11.5 overlay's coverage (§7.6/§2.2) — the floor
that keeps it fair — and not about giving the sense back a little.

### 4. One bug the ticket found on the way

`State::new` runs the level-start world turn (§4.2) *before* a builder can thread the
resolved modifiers in, and that turn stamps a sense cue on every guard it feels. So the
opening frame of a run whose sense is off carried a turn-zero dot the run could never
produce again — the same staleness `with_modifiers` already re-ran the sight phase to
fix, one channel over. The correction is to drop the cues there: this modifier can only
ever take marks away, so there is nothing to re-stamp.

## Appendix 59 — The minimum haul: why a check at the exit is not a toll

*(§4.5 the exit gate, §14 v3's voluntary-extraction bullet and its empty-handed [OPEN],
§2.2 intel as currency, §11.4 the ambient tally and the level-start card, §12.6 the
modifier seam, §10.2/§10.6 satisfiability. Revises the [SETTLED] rule appendix 47 set.
The ticket is #574.)*

The campaign's exit gate was `IntelGate::None`, and it followed from a rule worth keeping:
**a currency you must hand over to get out is a toll, not a currency**. Appendix 47 records
why that survived, and it is still the right argument. What it also produced, though, was a
facility that could be entered and left on the next turn having taken nothing — and a
building you can walk out of immediately is not a raid. The map's whole structure went
optional with it: the flavours, the alert carried forward (#210), the archive's star gate
(#573) and the choice of which road to take all assume a facility is something you *go
into*.

This appendix records the distinction that let the second problem be fixed without
reopening the first.

### 1. A haul is not a toll, and the difference is the debit

The toll argument is about **spending**. A gate that takes an intel to open is a fee on the
currency: the wallet is debited, the balance you carry to the hub is smaller than the
balance you earned, and every exit becomes a transaction. That would put two rules in
direct contradiction — *intel is currency* and *intel is the exit key* — and appendix 47
picked the one that survives.

A **minimum haul** is a different shape entirely:

| | toll | minimum haul |
|---|---|---|
| What the exit takes | one intel | **nothing** |
| The wallet (#211) | debited at the mouth | **never involved** |
| What reaches the hub | the haul, minus the fee | **the whole haul** |
| What it asks | a payment | that the raid **happened** |

Nothing is spent. The exit checks a predicate and opens; the haul is kept in full, and the
wallet's one debit path stays where #211 put it, at the map between facilities. So the
currency argument is untouched — there is no fee for it to be a fee on.

What genuinely changes is the second half of the settled bullet: *"everything in a
facility is surplus"* becomes **"everything past the first thing is surplus"**. The spirit
holds. You still choose how deep to go, how long to stay, and when to cut your losses; the
only thing that closes is *"or nothing at all"*.

### 2. One is the number, and two would be a quota

The bite the rule wants is exactly the removal of the zero case, and nothing more. Two
objectives would start to read as a quota, and a quota is a toll wearing a different hat —
it would make the exit a thing you work *towards* rather than a thing that is simply open
once you have raided the building. The rule is written as *at least one* and the tests pin
the surplus explicitly: with one thing taken and two still in the building, the gate wants
nothing further.

### 3. Reusing `AtLeastOne` rather than minting a variant

§4.5 already had `IntelGate::AtLeastOne` for the sim. Widening its meaning from *"one
console"* to *"one objective"* — a console `$` **or** an equipment cache `¤` — gives the
campaign exactly the rule it wants and costs the sim nothing, because crates are
campaign-only (§8.3): `LevelConfig::V1` plants none and no sim preset or `--modifier` name
asks for any. One objective and one console are the same thing there, so the histogram
measures the game it measured before. That was checked rather than argued: the committed
playtest baseline matched byte-for-byte across all four profiles after the change.

The alternative was a fourth enum arm. It would have meant a new token digit, a wider
`FORMAT_MAJOR` conversation, and two variants that differ only in whether a crate counts —
in a mode that has crates and a mode that does not. One variant, two modes, no new arm.

The widening is deliberately **`AtLeastOne`'s alone**. `IntelGate::All` is the
complete-the-set objective — quick play's, and the archive's (#217) — and a crate is no
part of the set, so it still counts consoles. That is what keeps the terminus's rule its
own.

### 4. Satisfiability is asserted, not hoped for

A facility the run cannot take one single thing from is a facility it cannot **leave**: a
softlock, and the one way this rule could ship as a bug. It is impossible by construction
on two counts, and both are asserted over a seed sweep, per flavour:

- **something is there.** §10.2's console count floors at `LevelConfig::INTEL_MIN`, which
  is two, whatever the §12.6 intel knob asks of it. That floor is precisely why the gate
  had to accept a console and not only a crate — an Outpost hides none.
- **it can be reached.** Placement refuses a carve where any objective is not bump-adjacent
  to the ground the run can walk from the mouth it climbs out of (§10.6). The sweep floods
  the board again from outside rather than trusting the generator to have checked itself.

### 5. Two surfaces had to move, and one of them was a latent hole

**The refusal could no longer be a count.** `Event::ExitRefused` carried `still_needed`
alone, and the near line read *"the exit needs N more intel"*. Under the widened gate that
number is always 1 and the sentence is half the rule: a player with a crate two cells away
would be told to go and find a console. So the event carries the gate, and `AtLeastOne`
gets its own line — *"the exit needs one thing taken"*. `All` keeps its counted line
verbatim; the two cannot be confused because they are no longer the same sentence.

**The Level info tab named no gate at all.** The modifier list is a list of *departures*
from baseline, which is right for a list of departures — and §4.5's baseline gate is the
very rule the campaign now plays under, so the tab would have gone silent about the rule
that ends the run, at the moment that rule got harder. The fix is not to make a baseline
knob draw a direction-coloured row (it departs from nothing, and `ModifierDirection` has no
honest value for it) but to give the tab an **objective section**, sharing the level-start
card's derivation. The card had needed that derivation since #497 for exactly this reason;
the tab simply had not been given it. One rule, two surfaces, one source.

### 6. The §11.4 tripwire, and why the tally survived it

§11.4 carried a standing condition on the ambient `objectives: 1/3` row: *"if a
human-facing mode ever ships on `AtLeastOne`, this line reverts — it is the one thing to
check before changing a mode's gate."* This change trips it, so it was checked rather than
waved through.

The rule #310 actually established is that **no row may promise an exit that will refuse**.
A tally reads as the requirement when it *is* the requirement, which is the `All` case and
is why `3/3` is a legitimate exit-open signal there. Under the minimum haul the fraction
states neither the requirement nor its negation: it is a labelled progress count over the
loot, the requirement is *one* and of either kind, and it is stated outright on the
level-start card and the Level info tab and answered free at the mouth. A run standing at
`0/3` is never told the exit is open.

So the line stays and the condition is rewritten to say what it was always for. What would
break it is a gate whose requirement the fraction could be *mistaken* for — that is the
thing to check next time, not the name of a particular enum arm.

### 7. It removes the abort, and that is the point

The honest cost is worth stating plainly: a raid that goes wrong in the first ten turns can
no longer be walked out of, and a run pinned at condition 3 having reached no console has no
way out but through. That is a real hardening. The counter-argument is that a facility you
cannot take one single thing from is a raid you were losing anyway, and a permadeath game
is allowed to say so.

If play shows it producing unwinnable-but-not-yet-dead states, the hatch to consider is
**opening the exit unconditionally at alert condition 3** — a pressure valve on the one
state where the building has genuinely closed around you. Not softening the gate, which
would put the revolving door straight back. That hatch is [OPEN] until a played run asks
for it.

## Appendix 60 — Three stars, one per axis: what the score is for, and the one number that had to be derived

**Ticket #563.** Closes §15 Q4 and the matching §7.3 `[OPEN]`. The design lives in §4.6;
this is the reasoning that cost the argument, and the measurements that set the numbers.

### 1. Why three marks and not one number

The brief could have been read as *"rate the run 1–3"*. It was not built that way, and the
reason is what a rating is **for** in a game with no meta-progression. §2.2 is explicit
that the only thing carrying between runs is what you learned. A weighted total mapped
onto tiers tells the player *"two stars"*, which they cannot act on; three independent
axes tell them *"you were quick and quiet and you left a console standing"*, which they
can. The readout is a **diagnosis**, and a diagnosis that averages its findings is not one.

That is also why the axes are never traded off against each other. A run cannot buy speed
with noise, because the two are answers to different questions about different mistakes.

**Zero is a real reading**, against the brief's 1/2/3. A floor — *"winning is the first
star"* — would spend the most legible mark on the fact the player already knows (they are
looking at the escape screen), and would make the first star meaningless. A run that
crawled out slow, loud and half-empty earned none of the three and is told so.

### 2. §15 Q4's other half: takedowns cost score, and it costs no second rule

Q4 asked whether takedowns should cost score, *"giving 'no killing' mechanical teeth via a
leaderboard rather than a rule"*. The answer taken is **yes to a score, no to a separate
takedown penalty**, because the stealth star already charges for it:

    takedown → a body (§7.2) → the body is found → rung 3 (§7.3) → the stealth star is gone

Aggression is therefore priced through the clock the design already built, with one
mechanism instead of two and no leaderboard at all. A second, explicit *"−1 star per
takedown"* would charge twice for one decision — and it would charge it in a currency the
player cannot see coming, where the radio's clock is deliberately readable (§7.3: the
responder is a dot that peels off toward where you struck).

The ghost↔aggressive spectrum Q4 wanted falls out of this for free: a ghost run keeps the
star, an aggressive run spends it, and the *cost* of spending it is the raid getting
harder rather than a number getting smaller.

### 3. Par is the hard number, and a flat one would have been wrong

The speed star needs a threshold, and a threshold is where this design could most easily
have gone wrong. Turn counts vary with facility size, flavour, modifiers and how much of
the building the run chose to see (§10 exploration is most of a raid). **A flat constant
would be wrong for half the facilities and would read as the game being arbitrary** —
which is worse than shipping no speed star at all.

So par is derived, from the building's own contents rather than from the recipe that asked
for them:

    par = 2 × span + 90 × consoles + 50 × crates      (span = width + height)

Three decisions inside that:

- **Span, not area, and doubled.** The ground a raid covers is *across and back*; 40×40
  gives 80 cells of span, and the factor of two is the admission that the way in is not a
  straight line — a raid that walked one would be walking through guards. Area would have
  made a bigger board quadratically more forgiving, and the board is screen-bound anyway
  (§10.2/§11.4), so the term has nowhere to run.
- **From the facility, not from `LevelConfig`.** The number is read off the built level —
  its carved span, the consoles seated in it, the crates hidden in it — so a hand-built
  fixture with no recipe behind it still has a par, and so the flavours are handled with no
  per-flavour table: an *Outpost* lands on 340 and a *Vault* on 670 because that is what
  they hold.
- **The console term carries the number, and a crate is worth less.** A console is not a
  detour off a route, it is a *search*: the room it stands in is fogged until you have been
  in it (§11.5a). A crate is a detour the player *chooses*; par allows for taking it rather
  than funding it comfortably, and the thoroughness star is what pays for the choice.

### 3a. The first set of numbers was wrong, and the way it was wrong is the lesson

Par shipped at `span + 25 × consoles + 15 × crates`, putting quick play on **155**. It was
wrong by a factor of nearly three, and both the report and the confirmation are worth
recording because the mistake is easy to make again.

**The report.** A played run with **omni-vision and ghost both on** — the fog lifted and
the guards blinded, which is about as close to the optimal walk as this game allows — did
not make par. That is a decisive kind of evidence: a threshold no *cheating* run can clear
is not demanding, it is broken.

**The confirmation.** A 100-seed bot batch at the **quick-play** gate:

| Profile (`--intel-gate all`) | Wins | Median turns | speed stars |
|---|---|---|---|
| balanced | 4 | 428 | **0** |
| cautious | 20 | 683 | **0** |

Not one all-intel win in two hundred runs came in under par.

**The mistake was measuring against the wrong gate.** The first numbers were sanity-checked
against the sim's own baseline, which runs at `IntelGate::AtLeastOne` — a bot that takes
*one* console and leaves, median 137 turns. Quick play asks for **all three**, and that is
not the same job with a bigger number on it: each further console is a fresh search of a
fogged building. 63% of those one-console wins were "inside par", which read like a healthy,
discriminating threshold and was in fact a measurement of a different game.

**The general rule this leaves behind:** *how long a raid takes is decided more by the
exit's gate than by anything about the building.* Par is a fact about the facility, but the
**calibration** of par is a fact about the mode, and the mode a human plays (quick play,
all the intel) is the one it has to be right for.

The tuned set puts quick play on **430** against that 428 median. Re-measured on the same
two batches:

| Profile (`--intel-gate all`) | Wins | Median turns | speed stars | best total |
|---|---|---|---|---|
| balanced | 4 | 428 | **2** (50%) | 3★ |
| cautious | 20 | 683 | **3** (15%) | 3★ |

Half of *balanced* wins clear it and a seventh of *cautious* ones do — the temperament that
buys safety with turns pays for it in the speed star, which is the trade the axis exists to
price. A three-star run appears for the first time. The consequence to keep in view is the other end: at the
campaign's minimum haul (§4.5/#574) the speed star is *cheap*, because a run that grabs one
objective and leaves clears par easily and pays for it in the haul star. That tension is the
axes doing their job; if beelining ever becomes the only sane play, the lever is the gate
and not par.

### 4. What the sim measured, and the one worry it retired

400 bare-bot runs, 100 per profile (`--bot --profile P --runs 100 --seed 0 --cap 1000`), at
the sim's own `intel_to_exit: AtLeastOne` gate:

| Profile | Wins | Median turns | 0★ | 1★ | 2★ | 3★ | speed | stealth | haul |
|---|---|---|---|---|---|---|---|---|---|
| balanced | 35 | 137 | 6 | 10 | 19 | 0 | 22 | 26 | 0 |
| cautious | 53 | 235 | 12 | 27 | 14 | 0 | 15 | 40 | 0 |
| aggressive | 40 | 136 | 13 | 10 | 17 | 0 | 23 | 21 | 0 |
| careless | 43 | 142 | 12 | 15 | 16 | 0 | 24 | 23 | 0 |

Three findings:

- **The stealth star is demanding, not impossible** — which was the risk the ticket named
  and the one thing worth measuring before shipping. It is earned by **60–75% of winning
  runs** across every temperament. The threshold stays at condition 0; had this read near
  zero, the honest response was to move it to ≤ 1 rather than blame the design.
- **The speed star discriminates between temperaments**, which is what a threshold is for:
  63% of *balanced* wins were inside the *original* par against 28% of *cautious* ones —
  the profile whose whole temperament is spending turns to stay safe. That the numbers
  themselves were wrong (§3a) does not touch the shape: a threshold every win or no win
  cleared would be a row with nothing behind it (§2.3), and this one separates the two
  temperaments at whatever value it sits at. **These rows are at the sim's own gate**, and
  §3a is the reason that sentence now has to be said out loud.
- **`haul` is zero everywhere, and that is the sim's gate rather than the axis.** The sim
  leaves after one console (§13.2), so it never takes everything; the axis is unearned
  rather than unearnable. It is exactly the opposite degeneracy to quick play's, where the
  gate is *all the intel* and no crates are planted, so the star is free. Both are recorded
  in §4.6 rather than papered over, and both are campaign-shaped problems: the campaign is
  the mode where objectives are surplus and the axis has teeth.

**The bot was not adapted, deliberately.** §13.2's policy decides by cues and has no
objective function. Handing it *maximise stars* would make the histogram measure the
scorer rather than the game (§13.3, `docs/bot-behaviour.md`'s standing warning), so the sim
**reports** the distribution and optimises nothing. That the bot's numbers above are
unchanged from the previous baseline is the evidence that nothing about how it plays moved.

### 5. The glyph that is doing double duty

`★` was already the campaign map's **archive** node, and `docs/render-reference.md` said in
so many words that it was the one glyph that screen did not reuse. The score now puts
`★`/`☆` on that same screen.

It was kept rather than swapped, for two reasons. The star is the one mark every player
already reads as *this is the good one*, and spending it on a rating is what it is for;
and the two readings are told apart by **kind and place** — a node standing in the picture,
against a mark on a named axis in a labelled report row (`speed ★ · stealth ☆ · haul ☆`).
The alternative was a private mark for the score, which would have made the most legible
row on the screen the one nobody had seen before. The render reference records the second
reading rather than leaving it to be discovered.

### 6. The criterion that has to survive

**Nothing reads the score.** The test that says so steps two campaigns side by side whose
raids differ *only* in what they score, and asserts every other thing the layer hands out —
the next facility's whole config, the loadout, the wallet, the alert, the offers — is
identical. It is the criterion that stops stars drifting into meta-progression, and the
first suggestion after this ships will be *"stars should give something else too"*.

#573 will **narrow** it to exactly one reader (the archive gate) and must not delete it.
Inside a run, a second consumer would make the score a resource the player plays toward
rather than a verdict on how they played — which is the thing §4.6 exists to not be.
