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

**Those three are the whole vocabulary — there is no experimental tier** (#564). An
ability either ships, in which case it is in the draw pool (§8.3) and gets played and
measured like every other, or it does not exist; a shipped ability nobody can draw is
inert (§2.3). Scepticism about one goes in its prose, where it can say what it actually
means, and every player-facing change is playtested by hand before it merges regardless
— so a marker singling one out promised a gate that was never a separate thing.

### What goes where

**This document says how the game is supposed to be.** Rules, values, and the
consequences that follow from them. Where a decision was controversial or hard to
shape, it also says *why* — in a sentence, not a case.

The long "why" lives next door, in
**[`docs/design-rulings.md`](design-rulings.md)**: the argument, the alternatives
that were tried, the reworks, and the sim evidence, for the decisions that cost a
long discussion or a measurement to reach. Rulings are numbered appendices, appended
in the order they were written and never reordered; this document links to them by
number — *(appendix 12)*. If you are about to relitigate something, read its
appendix first.

Three companion references sit beside both, each owning the *values* of one surface
while this document owns the rules:

| Document | Owns |
|---|---|
| [`design-rulings.md`](design-rulings.md) | Why a hard-won decision came out this way |
| [`render-reference.md`](render-reference.md) | Every glyph and every palette choice (§11.2/§11.3) |
| [`bot-behaviour.md`](bot-behaviour.md) | How the sim bot decides (§13.2–§13.4) |
| [`level-seed-token.md`](level-seed-token.md) | The token's field layout and integrity argument (§12.6) |

`docs/stats/abilities/` records what the sim measured per ability. Where any of them
disagrees with this document, this document wins.

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

The original pillar read *"no permanent guard incapacitate (no killing)"*. It bundles
two constraints which do not have to travel together, and only the first is kept:

- **The fiction constraint**: the protagonist doesn't kill. *Keep.* It costs nothing
  and it is the character.
- **The mechanical constraint**: threats are never permanently removed. *Drop* —
  because explore-thoroughly plus threats-rearm is a treadmill, and this design picks
  *ownership* of the space (appendix 2).

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
| Alert | **Carries one hop** — the last raid's condition, replaced not accumulated (§14 v3/#210) | **Nothing carries** |
| Facility access | **Opens up** | **Resets** |
| Player skill and knowledge | — | **Everything** |

This is the Spelunky model — one of the original stated inspirations — and the
FTL / Invisible Inc model at campaign scale.

**The fairness promise this creates.** A 2–3 hour run means a capture at hour 2.5
costs 2.5 hours, which puts enormous weight on §7.6: **if you can be captured by
something that isn't your fault, the pillar becomes cruelty rather than tension.**
Permadeath is a promise that the game is fair. **Every capture must be traceable to a
decision the player made.**

The cost this imposes on development, and the **prison level** parked as a possible
later relief for it (§14), are appendix 3.

**Where the pillar is in force: the campaign, and not quick play.** **[SETTLED]** The
pillar above is about **the run**, and the run is the campaign (§14 v3). **Quick play
is training** — one tuned facility (§10.2) played to learn the building, the guards and
your own verbs — so its end screen (§14 v2) offers *retry this level* and *new run*,
and the campaign's will offer neither. The exits are therefore a property of the run's
**mode**, not of the screen, so the campaign cannot inherit a retry button by reusing
it (appendix 31).

The consequence, stated rather than left to be discovered: **while v1 ships quick play
only, the shipped game has no permadeath in force.** That is intended — a training mode
you cannot replay teaches nothing — and it is why the mode gate exists in shape now,
before the mode that needs it does.

### 2.3 The rule the last version's failure leaves behind

The previous version was not fun, and the cause was not the design: **every system
that would have created pressure was inert, and the one ability that resolved
pressure was free.** The audit, system by system, is appendix 1. Two consequences
bind this rebuild:

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

**A guard's first alert does not move it (#430).** A guard that was Calm at the
head of phase 3 finishes the turn it had already planned — its patrol step, its
dwell, the slow quarter of a reversal — even when this turn's look flips it to
Chasing or Investigating. The fresh state routes it from the *next* turn's
decision. Three edges, all deliberate: only the first alert defers — a guard
already reactive reacts the same turn, so a chase that re-acquires you is never
handed a second delay; only the *action* defers — the mind updates at once (the
state flip, the §7.3 sighting tally, the §7.7 call-ins), so being seen stays
expensive; and a planned step that lands on your cell is still §4.5 contact — it
walked into you, it did not react to seeing you. The rule exists to kill the
unreadable capture (a guard converting its first sight of you into a step in the
same turn), not to make capture rarer.

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

  > **The one declared exception, and it is on trial** (#243). The **Saver**
  > (§8.3) is a passive that turns the *first* capture of a facility into a takedown
  > of the guard that made it: that guard goes down where it stood, a body is left
  > (§7.2/§7.3), and the run continues. It is bounded by the level rather than by a
  > clock — **one use per facility, and no recharge** (§8.2) — so the settled rule is
  > suspended once and then holds for the rest of the building, second guard of the
  > same turn included. It is held like any other tech and it costs a slot like any
  > other passive; nothing about it is free. **[START]**, and on trial: appendix 43
  > records why it is shaped this way and what the sim measured, which is that a bot
  > handed a free first capture wins nearly twice as often. Promote to a §12.6
  > modifier, retune, or reject — but do not read it as softening the rule above.
  > **If it proves too strong the budget is already at its floor**, so the two levers
  > are changes to the effect: leave the player **stunned for N turns** after the save
  > (§8.3's eject stun, so being caught still costs the position it was caught in), or
  > **stun the guard instead of taking it down** — one capture *deferred* rather than
  > refunded, which loses the free body along with it. Appendix 43 weighs both.
- **Being seen is not losing.** It is the beginning of a problem.
- **Win: grab the intel, then return to your entry point.** You leave the
  way you came in — and the way you came in is a **place**, not a tile (§10.7/#466).
  `E` is the inner mouth of a **linear duct** running from it out through the level
  border to the outside world; the run **starts with you inside that duct, on the
  border cell**, and the first inputs crawl you to `E` and out into the facility.
  Leaving is the same thing backwards: bump `E` to climb in (the usable line reads
  **`exit: enter`**, never `duct: enter` — one is a shortcut you found and the other is
  the way home), crawl back to the border cell, and **bump outward**, off the grid. That
  step off the board is the win.

  **The gate is answered at the mouth.** Bumping `E` before you hold enough intel
  refuses, with the message, exactly as bumping the exit always did — you are told in
  the facility, where you can act on it, rather than after a crawl that ends in a wall.
  The border cell answers a press too, since that is where the run *begins* and you can
  stand there empty-handed — but the usable line (§11.4) **does not offer** the way out
  until you have set foot inside. A row whose only entry is *leave* is the wrong first
  thing to say to someone who has not been in yet. **How much is "enough" is a level modifier** (`intel_to_exit`, §12.6/#244),
  not one fixed rule, so the modes gate the *same* facility differently:
  - **Quick play — all the intel** (§10.2/#244). Gather the whole set, then get out:
    a complete objective, and v1's default mode.
  - **Campaign, and the sim — at least one** (§14 v3/§13.2/§13.3, #574). The
    **minimum haul**: one **objective** taken — an intel console *or* an equipment
    cache — and the exit opens. Nothing is spent and nothing is handed over; the gate
    only checks that the haul is not empty, which is why intel stays a currency (§2.2)
    rather than becoming a toll (appendix 59). A facility must be *raided*, and how
    much of it you stay for is still yours. For the sim the two readings coincide —
    it plants no crates (§8.3) — and the shorter gate is what keeps the bot's outcome
    profile mixed: the all-intel march kept it in the facility long enough to be caught
    nearly every seed. **[START]** on the value, and *one is the number*: two would be a
    quota, and a quota is a toll wearing a different hat.
    - **The exception is the archive** (#217), whose node sets `IntelGate::All`: the
      terminus asks for the whole set.
    - `IntelGate::None` — the exit that never refuses — remains as the union identity
      (§12.6) and a value the token can carry, but **nothing ships on it** since #574.

  Pressing on for more than the gate demands is what an aggressive style trades extra
  exposure for. The gate is part of the run's reproducible config, carried in the
  shareable level-seed token (§12.4/#245/#333).

> **Consequence to preserve:** because capture is *contact*, not *detection*,
> being invisible does not make you safe. A guard patrolling into the cell you
> are standing in catches you even if it cannot see you. Hiding is not the same
> as being somewhere safe. This is a good rule; keep it.

---

### 4.6 The score — three stars, one per axis

**[SETTLED]** on the shape, **[START]** on every number. A facility the player **got out
of** is scored out of **three stars, awarded one per axis** (#563):

| Star | Earned when | On screen |
|---|---|---|
| **Speed** | Turns taken ≤ the facility's **par** (below) | `speed ★ out inside par` |
| **Stealth** | The run ended at **security condition 0** (§7.3) — nobody ever knew | `stealth ★ never noticed` |
| **Thoroughness** | **Every** intel console *and* every equipment cache the building held was taken | `haul ★ took everything` |

**One per axis, independently earned, is the whole design.** A weighted total mapped onto
three tiers would say *"two stars"* and nothing else; three independent stars say **which
one you missed**, which is the only thing a rating is good for in a game with no
meta-progression. §2.2 says the one thing carrying between runs is what you learned —
*"you had it all, but you were seen"* is that sentence given a surface.

**The range is 0–3, and zero is a real reading.** Escaping is not the first star: a floor
that made winning worth one would spend the most legible mark on the fact the player
already knows, and would stop the readout being diagnostic. A run that crawled out slow,
loud and half-empty earned none of the three, and the screen says so plainly.

**Only a run that got out is scored.** A capture has no score at all — not zero stars.
The end screen owes a lost run a *reason* (§14 v2), and a rating standing where the reason
belongs would be answering a question the run never finished asking.

#### Par — derived from the building, never a constant

**Par is computed at level start from the facility's own contents**, because a *Vault* has
one more of everything (§14 v3) and legitimately takes longer than an *Outpost*. A flat
number would be wrong for half the facilities on the map and would read as the game being
arbitrary, which is worse than having no speed star at all.

    par = 2 × span + 90 × consoles + 50 × crates      (span = width + height)

All three **[START]**. The span term is the ground: a 40×40 is eighty cells across and
back, doubled because the way in is not a straight line — a raid that walked one would be
walking through guards. **The console term carries the number**, and it is large on
purpose: a console is not a detour off a route, it is a *search*, since the room it stands
in is fogged until you have been in it (§11.5a) and exploration is most of a raid (§10). A
crate is worth less, because a crate is a detour the player chooses rather than one the
level asks for.

| | Outpost | quick play | Workshop | Depot | Archive | Vault |
|---|---|---|---|---|---|---|
| **par** | 340 | 430 | 440 | 480 | 610 | 670 |

**The first set of numbers was wrong, and how it was wrong is worth keeping.** Par shipped
at `span + 25 × consoles + 15 × crates` — quick play on 155 — and *no* all-intel run ever
came in under it: a played run with the fog lifted **and** the guards blinded missed it,
and a 100-seed bot batch at the quick-play gate scored zero speed stars against a median
of 428 turns. The numbers had been set against the **sim's** gate, where a run takes one
console and leaves, which is a different job from taking all three. Appendix 61 §4.

**Par lives on the help panel's Level info tab, not on the board.** It is a fact about the
level like every active modifier (§12.6), so it is available on demand and never nagging.
A par counting down in the corner would turn a stealth game into a speedrun and push
against exactly the patience §1 and §7.6 are built to reward.

#### It grants nothing — with exactly one planned exception

**Nothing reads the score.** Not an ability, not a modifier, not the wallet (#211), and
nothing at all across runs — §2.2's no-meta-progression rule is not negotiable, and a
score a system can *spend* is a currency the player plays toward instead of playing well.
The one exception the design has chosen is the campaign's **archive gate** (#573, v3): the
run's accumulated stars set how hard the terminus is, earned and spent inside one run
exactly as intel is. Until that ships the set of readers is empty, and a test asserts it.

#### Where the stars are shown

- **Quick play** — on the end screen, beside the run's ledger.
- **The campaign** — on the **map**, per facility, because §14 v3 settles that a completed
  facility does not raise the end screen; the map comes up instead, so if the stars are
  not there they are nowhere. The run keeps the score of every facility it completed, in
  raid order, and its total is the sum.

Either way the axes are **named**, never a bare `★★☆`: knowing which one you missed is the
entire point.

#### Two known states, recorded rather than hidden

- **The thoroughness star is free in quick play.** Quick play sets `intel_to_exit: All`
  (§4.5) and plants no crates (§10.2), so *taking every objective* is exactly the win
  condition — the star follows the escape. Quick play therefore scores meaningfully out of
  two, and the campaign is where the axis bites. Accepted for v2, to be revisited once the
  campaign exists; the alternative on the table is a different third axis for quick play
  (**no bodies left** is the strongest candidate).
- **The stealth star is demanding, not impossible.** Rung 1 costs one sighting *or* one
  missed ping and never comes back down (§7.3), so this was the number most likely to be
  unreachable. The sim says otherwise: over 400 bare-bot runs it was earned by **60–75% of
  winning runs** across all four profiles (appendix 61). The threshold stays at condition
  0; if a played run says otherwise, it moves to ≤ 1 before the design is blamed.
- **Par must be measured against the mode's own gate.** How long a raid takes is decided
  more by *how many objectives the exit asks for* than by anything about the building: the
  same facility is a ~140-turn job at the minimum haul (§4.5) and a ~430-turn one when the
  exit wants all the intel. So a par tuned on one gate is wrong on the other — which is
  exactly the mistake the first set of numbers made. The consequence to keep in view is
  that the speed star is **cheap at the minimum haul**: a campaign run that grabs one
  objective and leaves clears par easily, and pays for it in the haul star. That tension is
  the axes working, not a hole — but if beelining ever becomes the *only* sane play, par is
  the wrong lever and the gate is the right one.

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

**The run opens with the ordinary arc.** There is no free opening look: the player is
never dropped anywhere to need one. They start inside their own tunnel (§4.5/§10.7),
crawl to its mouth, read the room through the **entry-cell peek** (§6.1) — the one live
window a crawlspace grants — and climb out looking where they chose to look. That is a
*decision* where a free 360° frame was a fact, and §10.6's "the starting area should be
safe" is enforced by the crawlspace's own concealment rather than confirmed by a
handout. *(This replaced the #383 wait's-look opening, whose whole job was to show a
player the room they had materialised standing in; #466 deleted the materialising and
the exemption with it — appendix 32.)*

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
- **A guard's arc and range are a level's, not a constant's** (§12.6/#495). §7.1's
  guard is the ~90° wedge out to 10 and every run plays it unless the difficulty draw
  says otherwise; the *short-sighted guards* modifier shortens the **reach** to 6 and
  leaves the wedge alone. **The arc is deliberately not a difficulty knob**: §6.2's
  ladder gives a guard exactly one narrower rung (~53°) and no settings in between, and
  that rung measures at roughly three guards' worth of pressure (appendix 51) — a step
  too large for an axis that promises *slightly easier*. Making the arc finer-grained is
  its own piece of work. The **player's** sight is not on this seam and no modifier moves
  it. Placement is not either: the §10.1.9 turn-one spawn check always uses §7.1's own
  cone, so a shorter one cannot pass spawn cells the baseline refuses and the ±N arms of
  a comparison stay the same building (§12.6). **This is a different knob from the blind
  spot above**, on the same subsystem: the blind spot decides what a guard's touching
  ring *detects*, this decides how far the cone reaches, and neither supersedes the
  other — a short-sighted Calm guard detects exactly its own shortened wedge.
- **The 8 cells immediately around a viewer are always seen** — with one
  exception, the guard rear blind spot below. For the **player** this is
  unqualified and **[SETTLED]**: you can never stand adjacent to the player
  undetected, in any direction including directly behind.
- **Guard blind spot — and it depends on the guard's mood (#155, #410/#442).** A
  guard never detects the three cells at its back: the two rear diagonals (tier 4)
  and directly behind (tier 5) of §6.2. **A Calm guard is blind at its *sides*
  (tier 3) as well** — a patrol detects exactly its ~90° cone, and the free
  touching ring is the player's alone. Any guard that is **not** Calm — chasing,
  investigating, searching, answering a call — watches its sides again, so against
  a guard that is hunting you the old rule holds in full: you cannot stand beside
  or in front of it undetected. **[SETTLED]** since #442; the wider carve was the
  #410 experiment, and appendix 28 is what it measured.

  Two things follow, and both are the reward for **reading a patrol**: a takedown
  can come from a Calm guard's flank as well as its back — five approach cells,
  not three — and you can **tail** one, because its 90° turn at a corner no longer
  brings you into a detecting cell. A 180° reversal still catches you: that lands
  you dead ahead, tier 1.

  **The condition is the design, not a tuning fudge.** A patrol you have read is
  predictable; a guard hunting you is not. So the flank is somewhere to *work
  from* and never somewhere to *hide*. It costs no new state and no timer — the
  mood is already on the guard and the cone is recomputed every sight phase, so a
  guard's sides come back the turn it stops being Calm.

  Either way the carved cells stay §6.2 cone-carving walls (the wedge silhouette
  is unchanged) — only their membership in the *detection* set is dropped. This
  narrows the old blanket 360° ring for guards only; the player's ring is
  untouched. Pairs with the patrol dwell (§7.5): a window to act, and a back — now
  a back and two sides — to approach.
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

For a **guard**, ring cells are then dropped from the detection set — the §6.1
blind spot — and **how many depends on the guard's mood** (#155, #410/#442):
tiers 4–5 (its back) for a guard that is alerted in any way, and tiers **3**–5
(its back and its two sides) for one that is Calm. They still act as artificial
walls during the cast, so the wedge silhouette is identical either way; they are
only unmarked afterwards, so the guard simply does not *notice* what is there.
The player keeps the full ring.

The carve is resolved **per guard, at look time**, from the mood the guard is in
at that moment — never stored — which is what makes "a guard's sides come back
the turn it stops being Calm" fall out rather than need machinery. Because the
§11.5 danger overlay is drawn from that same one cone, a patrol and a searcher
standing side by side paint **differently**: the rule is legible on screen rather
than remembered.

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

Because of the touching ring (§6.1), **a guard that is hunting you can always see
you when you are adjacent to it — beside it or in front.** Its one gap is the
**rear blind spot** (§6.1/#155): the three cells directly behind and rear-diagonal
do not detect. Against a hunting guard, then, a takedown means either making it
unaware in front — arranging to be adjacent without ever having been in its cone,
a puzzle of geometry, timing, doors and distraction — or reaching the cell
**directly behind** it.

**Against a Calm patrol the surface is wider: its two flanks are blind too**
(§6.1/#410/#442), so five of the eight adjacent cells work rather than three, and
you can walk up beside a guard that is looking the other way and take it. That is
deliberately the *reward for reading a patrol*, and it is priced by the same
condition that grants it: the moment the guard is alerted its sides are live, so
the flank is somewhere to work from and never somewhere to hide. A guard that has
spotted you is exactly as hard to take down as it always was.

The rear and flank approaches both need a **window**: pair them with the patrol
dwell (§7.5), because a guard that never stops moving cannot be lined up on. And
neither is safety — **capture is contact** (§4.5), so a guard that walks onto you
still catches you, blind flank or not. Either way it is not a button.

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

> **Why non-solid `[SETTLED]`.** A solid body is a wall nobody can pass, and it
> soft-locked runs two ways — freezing a guard on a chokepoint, and sealing the
> player inside the cupboard they struck from. §2.2 forbids a run ending to a dead
> end rather than a decision. Appendix 4.

> **The one declared exception to the range (#239).** *Adjacent only* stays **[SETTLED]**,
> and the **Dart** (§8.3) is a costed exception on trial against it rather than a relaxation of it:
> one dart a facility, fired along the cardinal you face with nothing to aim, and the two
> requirements that matter here — **unaware**, and a **body** left behind — are untouched.
> What the range changes is which of those two costs bites: the body drops where the guard
> stood, so a shot down a corridor leaves evidence on a §7.3 clock in a cell you may not be
> able to walk back to. It is on trial and may be rejected. Appendix 54.

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
| "Nearest" means | the shortest **walk**, not the shortest straight line — a guard two rooms down the corridor is nearer than one just the other side of a wall |
| A responder's lead | bounds the **investigation, not the commute**: it does not run down on a turn the responder is still walking to the cell. However far the call is, the guard **arrives** — §7.6's cold-lead backstop then fires at the site, not on the road (appendix 27) |
| Second missed ping → | Control gives up on the post and stops calling it. It escalates nothing on its own: a post already known to be quiet tells control nothing new, so the ladder counts **bodies**, not pings |

What the shape buys:

- **Takedowns become a clock, not a cost you pay once.** Every guard you take down
  is a future appointment, so the strategy *scales badly on its own* — no rule is
  needed to ban a full clear, it collapses under its own weight.
- **The clock is readable.** A near-line message when control pings, and — because
  guard positions are sensed through walls (§9) — the dispatched responder is a **dot
  that visibly peels off toward the place you struck.** The player knows the clock
  exists and roughly when it fires, which is what makes it plannable. *(This tell was
  going to be a sound; sound is gone, so it is visual from the start — appendix 13.)*
- **Moving a body pays off.** Control's last fix on a guard is where it went down, so
  that is where the responder searches — and a body you dragged or stowed elsewhere
  is no longer there when it arrives (§8.3). A hidden body still misses its ping:
  hiding it buys the *investigation being confused*, not the investigation not
  happening.

#### The alert ladder — what an escalation actually does

**[SETTLED]** — three rungs, fixed triggers, cumulative retaliation, **no decay**.
The rung is defined by what it *does*, because a number that announces an escalation
which does not exist is worse than the old silence (appendix 1).

| Rung | Triggers (**any** of) | Retaliation **added** at this rung |
|---|---|---|
| **1** | A confirmed sighting; **or** one missed radio ping | Guards are **never calm**: the §7.5 patrol dwell drops from **3–7** to **1–3** turns **[START]** |
| **2** | **3** confirmed sightings (cumulative) **[START]**; **or** an intel console tampered with while at rung ≥ 1 | **+1 guard** enters the facility |
| **3** | A body found; **or** two missed pings across **two** bodies | **+2 guards** enter the facility |

**Effects are cumulative.** A rung applies every effect at or below it, so a run
driven from 0 straight to rung 3 gets the dwell cut **and** +1 **and** +2 = three
extra guards. Rung 3 is the top; there is no rung 4.

##### Reinforcements — where they come from, and the rule that makes them fair

**New guards enter mid-level**, reversing an earlier "explicitly out" line, because
rungs 2 and 3 otherwise announced an escalation and did nothing. The three-rung ceiling
is what keeps it from spiralling — however loud a run gets, the facility gains **at
most three**. Appendix 6 for this and every *why* below.

**Never in view.** The arrival cell is outside the player's field of view and never
adjacent to them, diagonals included — *an arrival the player witnesses is a guard
materialising out of nothing.* If the facility offers no cell that honours it,
**nobody arrives**: breaking the rule is worse than missing the reinforcement. The
player's **guard sense** (§9.1) is deliberately *not* gated with it — a new dot inside
the sense box is position-only information the player earned.

They come in at the far end (the §10.5 region whose nearest cell is furthest from the
player), and then they **walk**. Nothing teleports to the trigger.

**What they do.** They head for the **trigger cell** — the body that was found, the
console that was tampered with, the player's last known cell — and **search** it
(§7.6), exactly as a radio dispatch does; then they patrol from where they finished,
with a region beat like any guard (§7.5/§10.5) — **cut when the errand ends, around
where they came to rest**, never around the room they walked in by. *Reinforcements
search, they do not hunt.* More guards converging on a stale cell is **the net
closing**, which is what §7.6 asks for; more guards tracking the player's live position
is the un-fun chase §7.6 exists to prevent.

Their lead is the ordinary §7.4 one, and it covers the walk however long it is,
because **no responder spends its lead travelling** (§7.3 above): a reinforcement
starting at the far end by construction needs no special case. §7.1's cold-lead
backstop still applies — just at the end of the errand rather than in the middle of it.

**Guards in every other respect** (§7.4/§11.3): no glyph of their own, no colour of
their own, **normal speed** (§7.1 **[SETTLED]** — a reinforcement never accelerates),
their own radio clock drawn from the run seed (§12.4), takedown-able, and a body left
behind that runs its own §7.3 clock — a loop the ceiling above is what caps.

**A silenced radio does not stop them.** The comms console's effects are the enumerated
ones, and the ladder's rungs are not among them: a found body still reaches rung 3, and
rung 3 still walks guards in. Silencing the net buys you the *internal* net, not the
escalation. **[OPEN]** whether that is the right reading (appendix 6).

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

**"Rung" is our word, not the player's.** On screen a rung is a **condition** —
*"security condition 2 of 3"* — because a rung names the shape of this system and a
condition names the state of the building. See §11.8 for the rule and the rest of the
glossary.

##### What the sim measured (#376/#374)

Every threshold above is a knob the headless sim turns without a rebuild (`--alert`,
§13.2), and each run records the **rung it reached, the turn it reached it, and the
trigger that got it there**. Three results shape the numbers above; the sweeps behind
them are appendix 5.

- **The contact threshold is the ladder's reach knob; the window is not.** So the
  **3** is load-bearing and the **10** is not.
- **Rung 3 is a takedown player's rung.** Its triggers are a found body and a second
  quiet post, so a player who strikes nobody never reaches it at all.
- **The [START] values stay where they are.** Reach is tunable, but the *outcome*
  curve was flat until reinforcements gave the ladder teeth, and retuning a threshold
  against a flat curve is tuning noise, not evidence (§13.3). With reinforcements in,
  the cost lands **proportional to how loudly you play** — the avoidance-first
  profiles are unmoved, the two that leave bodies pay 2 and 5 points of win rate.

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
| Effect | Control stops pinging (no dispatch, no alert step from a missed ping), **both** §7.7 cooperation call-ins stop firing, and the player's own **False Call** (§8.3/#504) stops working — it is radio, and a facility with nothing listening cannot be spoofed |
| Cost, beyond the turn | **Patrols stop being predictable.** With no coordination left to divide the building, every Calm guard's territory becomes the whole level and its next target is drawn **at random** from the ground it has not inspected, rather than by the deterministic farthest-first sweep (§7.5) |
| Guards already sent | **Finish the errand** — silencing stops the next wave, it never recalls a search already under way |
| Permanence | **One-way**, for the whole level; the console then reads as spent (Neutral, §11.2) |
| Placement | A non-start room, at least **16** cells (Manhattan) from the spawn **[START]**, reachable by a bump (§10.6), hidden until seen (§11.5a) |

Four properties hold it in place; appendix 7 has the reasoning.

- **The cost is the route, not the switch.** Placement distance is the balance knob
  (**[START]** — the sim sweeps it): a console found in the first few turns would
  make every later takedown free.
- **It is findable, not given.** Contents are fogged (§11.5a), so it must be
  *scouted* — and it is asserted reachable like an objective (§10.6), because
  counterplay the player cannot reach is not counterplay.
- **Errands are not recalled**, which keeps it counterplay rather than a panic button.
- **It costs the player a tool, not only the guards one.** A run holding False Call
  (§8.3/#504) gives it up at the same switch, which is what keeps the console from
  being a pure upgrade for the runs best placed to exploit it: silence the net and you
  lose your own best repositioning tool along with their coordination.
- **The trade is coordination for predictability.** A dead net buys the loss of guard
  cooperation and pays with the loss of a learnable patrol: you can no longer stand
  somewhere and *know* a guard will not come. **Wandering is not an upgrade to the
  sweep**; farthest-first is what makes patrols read as purposeful (§7.5) and it is
  deliberately given up here. The replacement draw is **random** rather than
  farthest-over-the-whole-level, which would cluster every guard onto the map's
  extreme corners. It comes off the run's own seeded stream, so a silenced run
  reproduces like any other (§12.4).

**A silenced facility is lonelier, never blind.** Nothing here touches what a guard
does with its *own* eyes: the one that loses you still searches, the one that finds a
body still hunts it (§7.6/§7.2). Only the *calling of others* stops — and where a
*calm* one chooses to walk.

**Takedowns cost score, and this ladder is how.** *(Resolved — §4.6 owns the answer,
and §15 Q4 is closed with it.)* A run **is** scored, out of three stars, and one of them
is *stealth*: left at condition 0. A takedown leaves a body (§7.2), a found body is a
rung-3 trigger above, and any rung above zero costs that star. So aggression is priced
through the clock this section already builds — **no second rule, and no leaderboard.**
That is the *"which may well be enough"* branch §15 Q4 named, taken deliberately: a
separate score penalty on takedowns would charge twice for one decision, and the charge
the radio already makes is the one the player can see coming.

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

- Each guard has a **territory**: one part of a **partition of the level** into as
  many connected pieces as there are guards (§10.5). There is no spawn cell and no
  patrol radius — a guard with no beat has no territory and holds, rather than
  sweeping a box drawn round a cell it has long since walked away from.
- **The partition comes from the building, the assignment from the guards.** The
  split is a pure function of the region graph and the walls: the same facility
  divides the same way however the guards happen to be arranged. Only *who patrols
  which part* reads where they stand, and only so nobody is handed a wing on the far
  side of the facility. Two regions count as joined when a guard can walk between
  them without crossing a third — a door, a shared edge, or a doorway's own cells —
  because door edges alone under-describe the building badly enough to leave whole
  wings unclaimed.
- **Every region belongs to exactly one guard.** No wing goes uncovered and no two
  guards grind the same ground. There is no per-beat size ceiling: a beat is "the
  level, split *N* ways", so **territory size falls as headcount rises** and a
  reinforcement (§7.3) raises coverage *density* rather than only adding a body.
  Balance is best-effort — a facility is hub-shaped and only a few rooms across, so
  the largest part is large because the building has a large wing.
- **The partition is recut when the guard set changes**, and only then: when a
  reinforcement's errand ends and it needs ground of its own. Never per turn — the
  assignment reads live positions, so a per-turn recut would make patrols churn. A
  guard's inspected memory survives a recut; a destination the recut moved out of its
  beat is dropped at the next repick.
- **All of this is conditional on a live radio net.** Beats are what *coordination*
  produces, so killing the net (§7.3) leaves nothing to divide the building with: no
  partition is computed or recut, every Calm guard's territory becomes the whole level,
  and its next target is drawn at random rather than farthest-first. That is the price
  the comms console charges — see §7.3.
- It keeps a private memory of inspected cells.
- With no destination, it walks to the **farthest** uninspected, currently-empty
  cell in its territory. *Farthest*, not nearest — this is what makes guards pace
  across distances instead of shuffling locally, and it is why the emergent
  patrols read as purposeful. Keep it.
- When no uninspected cell remains, it wipes its memory and starts over.
- **Watched consoles (#319) — a level modifier (§12.6), off at baseline.** With it on,
  a Calm guard **alternates**: one leg to a cell beside a **console** its beat touches
  (an intel `$` or the comms `Ψ`, §10.3 — both solid, so the destination is always a
  cell *beside* one, never the console itself), then one ordinary
  farthest-uninspected leg, and so on. It remembers which of its consoles it has stood
  beside this cycle and prefers one it has not; when they are all taken the memory is
  wiped and the cycle starts over, exactly as the inspected memory above is. So
  **coverage is bounded rather than lucky**: every console in a beat is stood beside
  within a **[START] 800** turns of Calm patrol, against a baseline where a third of
  all consoles are never stood beside at all over 600 turns. The alternation is what
  keeps the ordinary sweep alive — a guard that only shuttled between consoles would
  have turned the level into two watched rooms and a free corridor network, which is
  easier, not harder (§2.3). It bends **destination choice and nothing else**: the
  arrival takes the ordinary dwell below and then leaves, so it raises how *often* a
  console is looked at and never how *long* a guard holds it (§7.6). Calm only, like
  everything else here. **A silenced net takes it away with the beats** (§7.3): the
  cycle is over the consoles a guard's own beat touches, and a dead net leaves no
  beats — one more thing the comms console buys. The measurements, the alternatives
  and what the sim does and does not support are *appendix 39*.
- **Dwell (#153).** On reaching a patrol target, a Calm guard **holds in place for
  3–7 turns** before picking the next — facing unchanged, no free re-aim (§5).
  This is what makes a Takedown (§7.2) approachable: a guard that walks every
  single turn can never be lined up on, so the pause is the *window to act*,
  paired with the rear blind spot (§6.1/#155) for the behind-the-back strike.
  **Calm only** — a Chasing/Investigating/Alerted/Responding guard never dwells
  and a detection cancels an in-progress dwell the same turn (a hunt never slows,
  the mirror of §7.1's "guards never accelerate"). The dwell length is drawn from
  the run seed (§12.4); the **[START]** knobs are dwell chance (**100%** — every
  arrival) and dwell length (**3–7** turns). **The facility alert cuts the length**
  (§7.3): from rung 1 the range is **1–3** turns, which is what "guards are never
  calm again" means mechanically. It is cut, never removed — the pause is the window
  a Takedown needs, and the rung never comes back down. Dwelling lowers patrol coverage on
  purpose (§7.6/§7.7) — a sim knob to watch, and more so now it is unconditional.

  It is unconditional because **a pause that fires half the time is not a rhythm a
  player can plan against**; the earlier 50%-over-3–5 dwell was lost inside the
  ordinary turning-round stops and never read as a pause at all (appendix 8). Note
  that the **stop the player sees** runs a little longer: a guard turning to leave
  spends one more turn rotating for a 90° heading, or two for a reversal, so a 3–7
  dwell reads as 3–9 turns of held ground. The dwell is the part with the facing
  pinned, which is the part a Takedown needs.

**What full coverage costs is not small** (appendix 9). Patrolling the whole facility
is a real difficulty increase, not a tidy-up: over 100 seeded bot runs per playstyle a
careful profile loses ~2 points of win rate and a bold one **13–17**, because the
ground a bold plan crosses is now patrolled ground. **Strategy diversity falling is the
part to watch** (§13.2). If it proves too harsh the lever is the **guard count**
(§10.2, ~9 points of win rate per guard) — not going back to leaving wings empty.

### 7.6 The chase and the hiding game — read this before touching guard AI

**This is the known reason the game was not fun, from direct play:** *guards that
saw you tailed you relentlessly; breaking out of sight was neither easy nor fun,
even with Run.*

That was not a tuning problem — four separate rules combined into a tracking turret
with no exit, and the chase came out **binary: glued, or gone. Never hunted.** The
four rules and how they compounded are appendix 10.

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

**The search is legible** (§11.5/§11.7, #224). It used to run silently — a guard's glyph
turned orange and nothing else said a search was under way, so the Hunted phase above was
a thing the player could only infer from a cone wandering past. The near line now says
when a search opens and when it is called off, facility-wide and once each however many
guards answered the call, which is what makes the timer a bet the player can actually
place. *Where* it is sweeping is a separate, opt-in layer: the `show_search_areas`
modifier (§12.6) washes the `SEARCH_RADIUS` box orange — the same box a cupboard inside it
is flushed from — as an *easier* setting, because knowing which ground is being combed is
a real advantage.

**Watched harder means covered, not crowded.** Every guard that answered one call
carries the same `focus` (§7.7), so the raised-coverage territory is the watch disc
**intersected with the guard's own §7.5 slice**, not substituted for it: sharing a
focus is right, sharing a *territory* makes responders converge into the single moving
clump this section exists to prevent (appendix 11). The plain disc is the fallback when
the intersection is empty — a guard called clear across the facility must still watch
something — and on a silenced net (§7.3) there is no partition to clip against, which
is harmless because call-ins do not fire on a dead net.

It also makes the watch slightly **kinder**, which was not the aim: splitting an area
covers it less densely than two guards pacing all of it. If the watch wants its bite
back, the knobs are `WATCH_RADIUS` and `WATCH_DURATION`, not un-splitting the
responders.

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

**The zones are the cone's own halves** (#495): certain is the inner half of a guard's
sight range and glimpse is the rest of it, which at §7.1's range 10 is exactly the 5
and the 6–10 above. Stated as a proportion rather than as two absolutes because a level
modifier now shortens the cone (§6.1/§12.6), and the zones have to shorten with it: a
certain zone pinned at 5 against a range of 6 would leave a single glimpse ring and make
a short-sighted guard *more* certain of what it caught than an ordinary one — an easier
rule biting harder on contact. Every cone the game can deal keeps both zones, asserted
at compile time. The Run relation follows the same proportion, and against a narrowed
cone it overshoots — 5 cells of gain against a 3-cell gap — which is a break in the
player's favour on the side of the axis where it belongs. Appendix 51 has the argument,
and the measurement that kept the **arc** out of the modifier: the one narrower rung
§6.2 offers is worth roughly three guards, where the shortened range is worth one.

**2. Losing sight must lead to a search, not an instant give-up.**

On reaching a destination and finding nothing, a guard sweeps the surrounding area
for a number of turns before resuming patrol. **But note the ordering: fix 1 first.**
Making guards search harder while the chase is still inescapable makes the game
*worse*, not better (appendix 10).

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

Most abilities are a declarative record — turn cost, duration, cooldown, an optional
per-level use budget (§8.2), and a list of effects drawn from a small vocabulary of
primitives. Trying "what if there were a smoke grenade" should mean **adding a row**,
not writing a system.

**How an ability aims is not in the record, and neither is its reach.** Aiming is
one of the three §8.4 ways, applied by the ability's own precondition where the
press is resolved; a reach (`CONFUSION_RADIUS`, `DART_RANGE`) is that effect's own
constant. The record used to declare a targeting mode as a fourth field, and it was
stored and never read — the aim was always taken at the press — so #556 removed it
rather than leave the record asserting something nothing checked (§2.3, appendix 61).

When a primitive won't stretch — piloting a drone, rewinding time — there is an
escape hatch to plain code behind the same interface. Both named cases now have a
user: Pierce Wall turns a solid into floor (#303), and the **Drone** (§8.3/#273) is
the literal one — it transfers the player's *input* to another entity, which no
arrangement of effect primitives expresses. That seam is deliberately a **remote
unit** rather than a drone (`control::remote_kind`), so taking over a guard later is
a row in one table plus a spawn rule rather than a second control system
(appendix 45). **Start data-driven; promote
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
> phase and activation happens after, a duration of *N* yields *N−1* effective turns,
> and the activation turn itself is unprotected. That inconsistency was live and
> undocumented in the old version. Pick a convention, write it down, and **make every
> surface report the number the player actually gets.**

**Passives: the cost is the slot, not the time.** **[SETTLED]** (#264) Some abilities
are never activated — they are simply **in effect for as long as you hold them**, so
the time economy has nothing to charge them with. **A passive pays with the loadout
slot it occupies** (§8.3, capped at 3 — #266): you hold it *instead of* something else,
for the whole run, and that is the whole price. Three consequences keep the model
honest (appendix 23):

- **Held is on.** There is no activation moment. Picking a passive up switches it on;
  dropping it is the only off.
- **It reads as its own state** — **`(on)`** on the bar (§11.4/#287), never `Ready` or
  `Active [N]`, because the four clock states all mean "and then it ends".
- **It is still an `Effect` list** (§8.1), applied continuously instead of for a window.

The balance watch this creates: a passive that meaningfully changes play for the price
of a slot is exactly right; one that is strictly better than any activated ability is a
smell, and that is what the power grades (#263) and the sim (§13.2) are for.

**Uses per level: a bound on the facility, not a resource.** **[SETTLED]** (#302) An
ability may declare **how many times this facility lets it be used at all** — the one
thing "no charges" rules out that the game needs, for an effect too strong to hand out
on a cooldown alone (#303). It is a bound rather than a bar because **there is nothing
to spend, refill or manage**: the number only goes down, and no decision in a run is
about *getting more of it*. The fence that keeps it out of the charge economy
(appendix 23):

- **Set at level start from the ability's own row. No recharge, with one named
  exception** — no regeneration, no tick, no console that tops it up, and nothing you can
  earn one back with. The exception is **finding another copy of the tool itself** in an
  equipment cache (§8.3/#266): a crate holding tech you already carry refills that
  ability's budget to the level's grant and is spent doing it. It is not a way to *manage*
  the number — there is still nothing to spend, refill on demand or plan around — and it
  is bounded by how many crates the building hides (§14 v3, at most three). What it does
  is give a duplicate crate, otherwise pure bad luck, one thing to be worth. Appendix 44.
- **Single digits**, enforced at compile time. Ten uses is an inventory.
- **It composes with the time economy, it does not replace it.** An ability may carry a
  cooldown *and* a use budget; §4.4 stands unchanged, and an activation refused for want
  of a use is the free mis-input it already was.
- **The player is told both numbers.** The bar shows what is *left* (`Bore(2)`); the
  help panel's **Abilities** tab shows what a level *grants* (`3/level`, #343). Spent
  reads as unusable, never as ready and never as `/0/`.
- **It composes with the *slot* price too, not just with the clocks** (#243). A
  **passive may carry a budget**: held is still on, there is still nothing to press —
  and the level still only allows it so many times. What spends it is the world rather
  than a keypress (the Saver's is a guard reaching you, §4.5), and a passive whose
  budget is empty is **off**, not merely un-pressable, so the bar's spent-and-greyed
  and the game's behaviour are one fact. Such an entry reads `(1)` and then `—`, never
  `(on)`: the parenthetical is the same shape either way because both mean *not a
  timer*, but a standing `(on)` would advertise a rescue the run has already spent.

If something later wants uses that refresh, or uses shared between abilities, that is a
different design conversation — not a quiet extension of this field.

### 8.3 The starting set

Everything here is **[START]**. This is the sandbox to experiment in — it is the
whole reason the architecture looks the way it does.

**Innate** — always available:

| Ability | Cost | Duration | Cooldown | Effect |
|---|---|---|---|---|
| **Move** | 1 turn | — | — | One cell, cardinal. Sets facing. Not shown in the UI. |
| **Wait** | 1 turn | — | — | **360° vision for that turn.** The only way to see behind you. Standing on a body with free hands, it **also takes hold** (Drag, below) — and still gives the look. |
| **Run** | 1 turn | 5 | 12 | One free move per turn while active → 2 cells/turn. |
| **Takedown** | 1 turn | — | — | §7.2. Adjacent, unaware target only. Permanent. Leaves a body. |
| **Drag** | 1 turn/step | while held | — | A body is non-solid: walk over it, and **wait while standing on it to take hold** (#451); **you move at half speed while dragging**; **bump the held body to release** (free), or **stow it in a cupboard** (§10.3). |

**Salvaged tech** — found in the facility:

> **Where it is found: the equipment cache** (§14 v3, #209). A campaign facility hides
> **as many crates as its flavour says** — an Outpost none, a Depot one, a Workshop two,
> a Vault three **[START]** — each a `¤` (§10.3/§11.2 Interest) you **bump** to salvage
> the tech inside (§4.3's one interaction verb, the console's pattern with a different
> prize). What you find is **usable that turn** and carried by the run for every facility
> after it (§2.2); nothing carries out of the run. Crates are planted a real detour from
> the way in and spread across rooms (§10.6), and every one is **optional** — the
> campaign's exit never asks for a *crate* (§4.5), so walking past one is a legal and
> sometimes correct choice — the minimum haul (§4.5/#574) is met by a console just as well,
> so a crate is a second *way* to satisfy it and never a second thing it asks for. Quick
> play plants none: a single facility has no *rest of the
> run* to accumulate into (§2.2).
>
> **What a crate holds is a property of the building, not of you.** It is drawn from the
> facility's own seed, so a facility is stocked before anyone breaks into it. Within one
> building the crates are all different; **across a run they may repeat**, and finding tech
> you already carry is bad luck rather than a bug — the world is not rearranged to spare
> you the walk.
>
> **You carry three, and the cap is kept at the crate.** `MAX_TECH_HELD` (§8.3) is
> enforced where the pickup happens, and what a full run meets there is **the exchange**
> (#266), not a refusal: the crate offers what is in it, and you choose which of the four
> — your three, and its one — to **drop**. Dropping one of yours is the trade: the new
> tech is on the deck that turn and the old one is gone for the rest of the run. Dropping
> the crate's own is the decline, and the crate is left standing for a run that comes back
> having traded that piece away. The bump that opens the offer is **free** (§4.4) and the
> trade spends the one turn a plain salvage would have — so a swap costs a walk and a
> turn, and a decline costs nothing.
>
> **The exchange is the ability bar, not a screen** (§8.4). While it is open the bar draws
> the four candidates in its own four slots — **numbered** `1`–`4`, since this row is
> picked from rather than glanced at, and the crate's drawn in the reward colour, which is
> the whole of how it says it is the new one. The keys, the mnemonics and the taps that
> fire an ability answer the offer instead; `Esc` is a second spelling of dropping the
> crate's own. **Nothing else happens while it is open**: the turn loop takes only the
> choice, so no guard moves while a run is deciding, the near line keeps stating the
> question for as long as it stands, and the decline is always one press away (§11.6 —
> never a trap).
> A crate holding tech you **already carry** is the one refusal left — there is no decision
> in a second copy, so it is refused for free and says so — **unless that tech has a
> per-level use budget you have spent from** (§8.2), in which case the second copy is worth
> a real detour: the crate refills it to the level's grant, says so (`Bore recharged`), and
> is spent. That is the only thing anywhere that moves a budget upward. Appendix 44.

| Ability | Cost | Duration | Cooldown | Effect |
|---|---|---|---|---|
| **Camouflage** | 1 turn | 10 | 20 | Undetectable **while you don't move**. Moving reveals you for that turn. |
| **Decoy** | 1 turn | 20 | 30 | A fake intruder in the cell you face. Draws Investigating, not Chasing. Dies when anything steps on it. |
| **Dephase** | 1 turn | 4 | 30 | Fill → 0. Walk through walls, doors, guards. **Does not conceal you.** The window counts its own activation, so a phase begun on open floor buys **three steps** into a solid — and the safety eject below can therefore throw you back from three cells deep, for a worst case of four turns stunned (#449, appendix 12). |
| **Autodoors** | 1 turn | 16 | 40 | While active, a door in your path **opens as you step into it** — no bump, no lost turn — and **shuts behind you** once you clear it, **manual and automatic alike** (an automatic door is shut early rather than left to its slow `delay`). *"Alike"* is the rule, not a description of a typical level: a run plays one door vocabulary (§10.4/§12.6/#452), so the automatic clause is inert in a baseline facility and the manual clause is inert with the modifier on — the ability behaves the same either way, which is why it is worded over both. A door closed behind breaks line of sight (§10.3) and forces a pursuer to reopen it (§10.4): a §7.6 flight tool, not invincibility (#241). |
| **Confusion** | 1 turn | — (instant) | 45 | **Fired once**, from the cell you press it in (#325). Every guard standing within the blast at that moment — `CONFUSION_RADIUS`, through walls like the guard sense (§9) — is **blinded and frozen** for `CONFUSION_DAZE_TURNS` (**6** **[START]**), a countdown each guard carries itself. A costed panic-buy of time, not a kill: a dazed chaser **pauses** (keeps its lead), it does not reset. **After the flash, distance stops mattering** — a guard you run away from stays dazed, and one that walks into the cells the blast covered was never in it and is untouched, which is what keeps it from being a no-guard-may-act field you carry. Capture-is-contact still holds (§4.5): a dazed adjacent guard cannot step into you, but the daze is no shield to walk into a guard the blast never caught, and a frozen guard's cell stays solid. **The clamp [SETTLED]:** the reach fired is `min(CONFUSION_RADIUS, sense_range())`, so the blast can never freeze what you cannot sense — inert on open floor (`min(6, 10)`), and shrunk to **5** inside a duct (§10.7). It can only ever shrink the blast, never widen it. A firing with nothing in reach is a **free no-op** with a near-line message (§4.4/§8.4); a real firing says how many it caught (§11.7). The long cooldown is what keeps it rare (#240). Appendix 24. |
| **Pierce Wall** | 1 turn | — (instant) | — | **Bore straight through your one adjacent wall**, permanently. Usable only when **exactly one** of your four neighbours is a wall, so the target is unique by precondition and there is nothing to aim (§8.4) — which also rules the panic-bore out by construction, since a corridor and a corner both have two. The facility's outer shell is never a candidate (§1/§4.5); nothing else is off limits. It does **not** ask what is behind the wall — boring a two-cell-thick run (§10.1.5) opens a one-cell **pocket**, which is a use of the tool rather than a waste of it. It conceals nothing (it is not a cupboard, §10.3), so **three walls around you means you can dig a hole to hide in, never a tunnel**. Its scarcity is a **per-level use budget of 3** (§8.2), not a clock. The hole is real terrain in the one spatial model (§10.5) — guards route through it and see through it, for the rest of the level. Appendix 24. |
| **Lockdown** | 1 turn | 8 | 40 | While active, every door within `LOCKDOWN_RADIUS` of **where you fired it** is **shut and sealed** — a guard cannot get it open, so its route goes the long way round (§7.6/§10.4). (It used to say *"cannot work the handle"*; on an all-automatic level there is no handle to work — §10.4/#452 — and the seal holds the door shut whatever kind it is.) A **snapshot**, not a travelling bubble: a door does not unseal because you walked away from it. **You** are never refused — a sealed door bumps open for you exactly as any closed door does, which is what stops a lockdown ever boxing its owner in; that costs the turn and leaves the door *open*, so a lockdown fired across a route you still have to travel is a real mistake. **Every seal is released when the window ends**, expiry or early toggle-off alike (§8.2) — the duration is the only clock, which is what keeps a temporary wall from ever becoming the permanent one §2.2/§7.2 forbid. A lockdown with **no door in reach** is refused for free (§4.4). Appendix 24. |
| **Saver** | — | **passive** | — | **[START] (#243).** The **first guard to reach you in a facility takes you down — and instead goes down itself**: §4.5's capturing step is turned into a §7.2 takedown of that guard, which drops **where it stood** (a lunge turned over never arrives, and your own cell may be a cupboard where a body means something else), leaves a body and starts the §7.3 clock. Then it is **spent for the rest of the level** — `1/level` (§8.2), no recharge — so a second guard reaching you the same turn captures you exactly as the settled rule says. There is **no activation**: held is on, which is the whole reason it is a passive rather than the toggle first proposed — a defensive window you have to predict is one you mistime, and §8.2's timing trap needs an activation turn to be a trap at all. **Surviving is not free**: you are left standing beside a body you did not choose the place of, in a cell a guard was walking to, with the radio already counting; and the slot it holds is a flight tool you do not have for every *other* crisis of the run. **This is the one declared exception to a [SETTLED] rule (§4.5)** — it is on trial, and the sim says it is strong: a fearless bot handed it wins nearly twice as often (appendix 43). |
| **Vision** | — | **passive** | — | **Always on while held** (§8.2): your sight arc is the full **360°** and your range box grows from 15 to **20** (§5/§6.1). No activation, no turn, no cooldown — it costs the loadout slot and nothing else. **Vision only**: the guard sense (§9) is a separate, innate channel and is deliberately *not* widened with it, so a wait still buys something (§9.1). It erodes the §5 "can't see behind you" constraint on purpose — that is what makes it worth a permanent slot, and what the sim watches (#265). |
| **Guide** | — | **passive** | — | **[START] (#505).** A second passive after Vision: one of the **eight cells around you** is washed `Effect` — the one lying in the direction of the nearest **unclaimed objective** (an intel console `$` or an equipment cache `¤`; both are things you go and *take*, and both drop out of the set once taken). It is a **compass, not a route**: the bearing is taken **as the crow flies**, with no regard for walls, doors or reachability — the deliberate opposite of §7.3's *"nearest means the shortest walk"*, which is control routing a guard where this is a needle pointing. **Expect it to point straight through a wall**; that is the tool working, not failing, and the restraint is the whole design — a guide that pathed would be a solver and would answer §10's exploration outright. It **reveals nothing else** (§11.5a **[SETTLED]**): the objective's cell, glyph and distance stay fogged until seen, so what you gain is an eighth of a circle and not a location — which is what leaves #215's v3 intel sink something to sell. **Not** the comms console (§7.3 — the cost is the route, not the switch, and its distance is a knob the sim sweeps) and **not** the exit (drawn as itself from turn one). Diagonals are kept though movement is cardinal: rounding a bearing to the nearest cardinal would throw away half of it. Ties break by the level's own ordering, never a draw (§12.4). Nothing left unclaimed and it **goes dark** — which itself says *there is nothing left in this building*. The wash is the **weakest** background there is (§11.5), so every threat cue paints over it: a convenience must never sit on top of the thing that can kill you. Two knobs if it plays too strong: a **range cap**, which makes it a local tool rather than a global one, and pointing home when everything is claimed. **It pulses**: the bearing shows on one turn in `GUIDE_BLINK_TURNS` (**3** **[START]**) and is dark on the rest, **turn zero included**. That is the ability's main balance lever, and both halves earn their place — a standing needle is a line you simply follow, and the fog stops being something you plan around, where a pulse gives you a *fix* you then walk on your own memory of; and a run that opened already pointing would make the first move free. The phase is the turn counter and nothing else, so it is deterministic (§12.4) and a player can count to it. Held is on: no key, no turn, no cooldown, `(on)` on the bar; it pays with the loadout slot and nothing else. |
| **Drone** | 1 turn | 40 | 40 | **[START] (#273).** Launch a drone from the cell you stand on and **fly it yourself**: your input moves the machine and your body stands still, so every drone move is a full turn the guards get (§4.2) while nobody is watching where you left yourself. Press again and the controls come back — **free** (§4.4), and the window does **not** end: the drone holds the cell you left it in and keeps feeding you its camera for the rest of the duration, and you can take the controls back later for the price of a turn. **One clock covers both halves**, which is the design: how much of the 40 you spend flying rather than watching is the decision the ability sells, and the bar's `[N]` means *turns of machine* throughout (appendix 45). Its camera is the full **360°** at `DRONE_SIGHT_RANGE` (**8** **[START]**) and is unioned into your own sight — your eyes keep working — and what it sees is remembered (§11.5a). The **guard sense (§9) is not widened with it**: that is your body's channel, and your body is what this costs. It **respects the building at its own scale**: everywhere a person could squeeze, plus **over a table** and **through a shut door's ventilation holes** — it is hand-sized and airborne — but never a wall, a **door frame**, a duct entry or a solid usable (`Terrain::admits_drone`). It crosses a shut door **without opening it**, which is the difference between reading a wing and unlocking one; the consequence worth stating is that a closed-door wing is scoutable and a Lockdown seals nothing against a camera. It is **not an actor** (nothing blocks it, it blocks nothing, no door shuts on it), has **no interaction verb at all** — it opens nothing, takes nothing, wins nothing — and **guards cannot perceive it** in any way. While flying, every other ability is greyed and refused: your hands are on the controls. **Launching, and taking the controls back, needs you on your feet** — a crawlspace refuses both (§10.7), because a body nothing can reach pays nothing, and the exposed body is the whole cost (§2.3). |
| **False Call** | 1 turn | — (instant) | 30 | **[START] (#504).** A **radio spoofer**. Firing transmits a forged control message to every guard within `FALSE_CALL_RADIUS` (**10** **[START]**) of you, through walls like the guard sense (§9), naming **the cell you fired it from** — and they converge on it and **search** (§7.6), because it *is* a real call with a forged source. It adds **no second verb to §7.7**: cooperation has exactly one — *a call sends guards to search a cell* — and this hands the player that one, through the same seam control's dispatch and both call-ins run through. **The play is a vacuum, not a trap**: you call them here and then you are not here. The cell is a **snapshot**, stale the moment you leave it — the property §7.7 already names for genuine calls — so fired and then stood on it is a way to be caught, and a cupboard inside the search it opened is no answer (§10.3/§7.6). **The reach is the transmitter's, not the eyes'** and is deliberately **unclamped** by the guard sense, unlike Confusion's blast: a called guard is not held out of view, it *walks to you* and arrives inside the §9 sense long before it arrives anywhere else, and the near line says how many answered at the moment of firing (§11.7). So a **crawling player broadcasts in full** — a duct degrades perception, and a transmitter does not perceive (§10.7). **Who answers is still §7.7's rule**: a guard that has the live player is never pulled off it, a guard already on an errand *is* redirected, and a call nobody was free for goes unanswered (a spent turn). The **alert never steps** — nothing was seen and no ping was missed (§7.3). **It is always pressable, and it never reports what it found** — the one place it parts company with Confusion, and the reason is §9. Confusion may refuse an empty blast and may name a count, because its reach is clamped inside the guard sense: everything it could have caught was already drawn for the player. This reach is **not** clamped, so it covers guards behind the §5 cone and, in a duct, well past the §9 sense — and a refusal would answer *"is anyone within ten cells of me?"* while a greyed bar entry would answer it **every frame, for free, without spending the turn**. That is a detector, and this is a transmitter. So a firing into an empty facility goes out anyway and costs its turn and lockout, and the near line says the call went out and stops: what answered is learned the way everything about guards is learned, by watching the §9 dots turn (§9.3). **It dies with the net** (§7.3): a silenced facility has nothing listening, so the press is refused for free — see the comms console's table. Unlike every other call, which sends a *fixed number*, this one sends however many are in the box, so the response size varies with where you fire it; if that plays too swingy the fallback is the **nearest N within the reach**. |
| **Dart** | 1 turn | — (instant) | — | **[START] (#239).** A **takedown at range**, and the one row in the catalogue that deliberately reopens the ability §2.3 records as having *been* the old game (*"unlimited range, no cooldown, and it did not consume a turn"*). It is on trial: filed with no milestone, because it contradicts §7.2's **[SETTLED]** *adjacent only*. **You do not pick a guard.** Firing sends a dart along **the cardinal you face**: it travels up to `DART_RANGE` (**8** **[START]**) cells, stopping at the first **solid** (`blocks_movement` — wall, hinge, closed panel, duct entry, a table, any solid usable) or the first **guard**, whichever comes first, and resolves entirely on that turn. The guard it stops on goes down **only if it is unaware** (§7.2) **and in your line of sight** (§6) — a guard that has seen you, or one known only as a §9 sense dot, is not a legal hit — and the body drops **where it stood**, on that guard's own radio cadence, with the §7.3 clock running exactly as an adjacent takedown's does. **Aiming by facing is a stronger safeguard than a cursor**: the §2.3 failure was *auto-target-nearest-visible*, which asked nothing of where you stood, and a cursor would have asked for two keypresses; this asks you to be in the corridor, on the line, pointing the right way, unseen — paid for in movement, exposure and turns, on the board, where the guards can punish it. There is **no target list anywhere in the implementation** to snap to (§8.4/appendix 1). **A miss costs everything a hit does**: nothing on the line, an aware guard, a guard you can only sense — all three spend the turn and the level's dart, all three read as the *same* near line, and the press is **never refused and the bar entry never greys for an empty line**, because a refusal would answer *"is there a guard in front of me?"* for free, every frame (False Call's reasoning, and it bites harder here since the answer is worth a takedown). **The cost is the body, and at range you often cannot reach it** (§7.3): a takedown you cannot hide is a takedown that finds you later, and shooting one down a corridor makes that a fact rather than a choice. Its scarcity is a **per-level use budget of 1** (§8.2) and **no cooldown at all** — Pierce Wall's shape, and the ticket's *"very large cooldown"* is met by something stricter: a lockout ends and this does not, so the bar reads `(1)` then `—` (appendix 54). It flies **over** a loose body and over your own decoy — a dart spent on your own prop is a joke you get once. The reach is clamped to `min(DART_RANGE, sense_range())`, inert on open floor and cutting a crawler's shot to **5**: the flight is painted (§11.5), so an unclamped duct shot would report a guard in the dark. **Watch the corridor**, which is the risk: if standing at the end of one becomes a reliable free kill the lever is `DART_RANGE`, not the budget — though §10.1a already stamps a table into every over-long straight, and a table stops the dart. Appendix 54. |
| **Repel** | 1 turn | 8 | 40 | **[START] (#554).** A **wall you can put down in the open**: firing stamps the `REPEL_RADIUS` (**3** **[START]**) box on **the cell you fired it from** as ground **no guard will stand in**. One sentence, two halves. **Nobody gets in** — the boundary refuses a step inward in any state, Calm, investigating, searching or chasing alike, so a route across it goes the long way round (§7.6/§10.4's *"a guard cannot get it open, so its route goes the long way round"*, read over open floor), and a guard the field has cut off **entirely closes on the boundary and waits there, facing in** rather than stopping wherever its route failed: a cordon, not a stall, and the two are the same hold mechanically. **Anybody inside walks out**, by the shortest way, the turn after it lands and every turn until they are clear — the stamp moves nobody, a guard spends its own turn leaving and keeps its mood, its lead and its errand, and once out it is one of the guards the boundary refuses, so it cannot come back (nothing is remembered). It is **Lockdown's trade where Lockdown is inert** — a hub room, open floor, a corridor with nothing to shut — which is why it carries Lockdown's own clocks: if the two play as one press the knob is the radius, not the pair of clocks. A **snapshot**, not a travelling bubble: the disc stays where it fell, because one centred on a moving player is one no guard could ever reach him in, and §4.5's capture-is-contact is **[SETTLED]**. **You** are never refused — you walk your own field freely, in and out. **It conceals nothing**: a guard that can see through it sees you, steps §7.3's ladder and calls it in (§7.7), so what gathers outside is a **ring of guards** standing exactly where you must come out, one step closer than they would have been. **Because guards leave rather than being trapped inside, firing it late works** — a chaser at arm's length is put out and held out, which is the ability's one genuinely free moment and the thing the sim watches (it has no *"when would a good player not press this"* left beyond the turn and the lockout). **Every cell is released when the window ends**, expiry or early toggle-off alike (§8.2). A firing with nobody in reach is **never refused and never greyed** and costs its full turn and lockout: the only precondition worth asking would be *"is a guard near enough?"*, and answering it — every frame, for free — is a detector, not terrain (False Call's reasoning). Appendix 62. |
| **Cover** | 1 turn | 12 | 35 | **[START] (#562).** **Cover you can walk.** Firing puts a §10.3 **partial-cover table** in the cell you face — the same terrain, the same `π`, solid to movement and pathing, transparent to sight, drone flies over it — with one addition: **bumping it pushes it one cell directly away and steps you into the cell it vacated, crouched**. One turn, one verb, and repeated it walks the table across an open room ahead of you, concealing you from the far side the whole way. Where the level has no bench and the crossing is open, this is the answer; §10.1a stamps furniture into every over-long *straight*, so the ground this is bought for is the ground the generator left bare. **A push with nowhere to go falls back to the plain §10.3 crouch** — *crouch always, push when it can* — so the bump keeps one meaning and only the extra cell is conditional. **Placement is the cell you face and only plain, empty floor takes it**: a wall, a doorway, a hideout, a duct mouth, any solid usable, the exit, a guard, a body all refuse it **for free** (§4.4), and so does being inside a crawlspace (§10.7 — the drone's *needs you on your feet*). That is also why removal needs no eject rule: nothing can ever be *inside* a piece of cover. **Cover placed touching furniture extends that run** and conceals as it does (§10.3, arms and half-plane union included); a **lone** piece is the game's first one-cell run, and it defines the line **perpendicular to the direction you are covering from** (§10.3). **The deploy does not duck you behind it** — that is the bump, on the turn after — so the entry price is a turn spent standing in the open. **Every trace is released when the window ends**, expiry or early toggle-off alike (§8.2): the cell goes back to plain floor, nothing is left behind in any model, and a player crouched behind it is **simply standing** — a run that spends the last of its twelve turns halfway across a room is a standing figure in the open at the exact moment its concealment evaporates. There is no grace turn; that moment is the ability. The window is also the whole safety model: a solid that expires on its own clock, can be pushed, and can be dismissed for free cannot make a facility unsolvable, which is why **§10.6's severance check is deliberately absent** — plugging a corridor and making a patrol take the long way round is the *tactic*, and it is the one Lockdown already sells with a door. **No guard reacts to the furniture appearing or vanishing**: guards detect on vision (§9) and nothing in §7 notices that a room changed shape, so cover in the middle of a corridor is routed around without comment and its disappearance is not evidence either — *"guards notice new furniture"* is a new system and its own ticket. **Watch it against the crouch-walk** (appendix 14): §10.3 already lets a crouched player move at full speed while hugging a run, and this makes that pose available anywhere for twelve turns — which is the ability's whole point and the first place it will prove too strong. The levers are the turn it costs, contact-vulnerability, the requirement to keep hugging, and this one's own duration — **not** re-narrowing the geometry the player has to read. Tune the duration against the **room**, not the corridor: too short and it is a crouch with extra steps, too long and it is a portable bench with a cooldown. Appendix 63. |

Notes carried forward, because they are good and non-obvious:

- **Run is a guaranteed escape** — 2 cells/turn against a hard cap of 1. Combined
  with guards that don't search (§7.6), this made being seen *free* in the old
  version. With searching guards, radio calls and converging responders, the
  escape stops being the end of the problem and becomes the start of one. **Watch
  this pair closely in playtest** — if being seen is still free, the answer is
  more consequence, not a slower player.
- **Camouflage does not stop capture.** Invisible is not safe (§4.5).
- **The drone's cost is your body, and nothing else is allowed to be.** It cannot be
  seen, chased or destroyed, so the only pressure on it is the turns your body spends
  standing unattended in a patrolled facility while you look elsewhere (§4.5: capture is
  contact). If playtest says it is too strong, the levers are the clock, the camera's
  reach, and drone-only vision while flying — **not** letting guards shoot it down, which
  is a different ability and a separate ticket. Watch it against §11.5a especially: a
  tool that cheaply reveals every console before you move deletes the exploration reward
  the fog exists to create.
- **Dephase does not conceal.** It's a movement tool. And while dephased you
  cannot *bump*, so you cannot open doors, use consoles, or win — you pass
  straight through everything you came for. That constraint is excellent; keep it.
- **A duration that expires while you're inside something solid throws you clear and
  leaves you stunned.** The tech's **safety eject** drops you on a cell drawn at
  random from the nearest ones that can hold a solid body, and afterwards you cannot
  act at all for **one turn per cell you were thrown, plus one**: every key is
  swallowed, the turn is spent, and the guards keep moving (§4.4's turn cost applied
  *to* you rather than by you). **[START]** — the rate and the flat `+1` are the
  numbers to tune; the shape is not. **The stun is as long as the throw**, because
  that is what prices recklessness, and the **randomness is load-bearing** — a
  predictable eject would make phasing into a wall a reliable way *through* one. It is
  *any* solid, not just a wall, which is why the near line names the **tech** rather
  than the terrain ("safety eject — stunned"). Deliberately **not** extended to the
  early toggle-off (§4.4): pressing the key inside a wall is still refused, because a
  free press that teleported you clear would be exactly the escape tool this is
  designed not to be (#304/#329). This is the third answer to the question — it was
  free, then it was lethal, and a death here is the one §2.2 forbids (appendix 12).
- **Decoy draws Investigating, never Chasing** — a guard that can see *you*
  ignores it. Decoys work on guards that have lost you, not on guards that have
  you.
- **A passive can now change the board without an activation** (§11.5/#505). The
  Guide is the first ability whose whole effect is a standing background wash, held
  from level start, so the effect layer's "latch a mark from the turn's events" shape
  does not reach it — its cell is a **live read** on the same footing as the two marks
  that blink (Camouflage's concealment, a running Dephase inside a solid). Worth knowing
  before adding a second: an always-on effect has no moment to latch, and the layer's
  extension point assumes there is one.
- **Which tech you start with is a level modifier** (`starting_abilities`, §12.6/#244),
  not a fixed roster. Quick play grants the innate set plus a **seeded** draw of three
  tech from a pool that defaults to the whole shipped set — **the whole of it, with
  nothing held back** (§0/#564): the roster and the pool are one list, so anything in
  this table can be drawn here and stocked into a crate, and "three random" is a genuine
  draw of three of the thirteen (the pool has outgrown the grant and it finally bites,
  #241). A campaign accumulates its set
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
- **Drag has no grab button, but it does have a grab *turn*** (#451). A body is
  non-solid (§7.2), so you cross it like floor — and **waiting while you stand on it
  takes hold**, if your hands are free. From then on the body follows into each cell
  you vacate. **Bump the trailing body to drop it** (free), or **bump an empty
  cupboard while dragging to stow the body inside and lock it** (§10.3) — the two
  ways to end a drag. Half speed holds throughout (one cell per two turns), and **Run
  never stacks with Drag**. Ducts refuse a dragging player: a body cannot follow into
  the walls (§10.7), so let it go first.

  > **It used to ride the step off the body's cell, and that was the wrong shape.**
  > The grab was something that happened *to* you: you could not walk across a body
  > at all without picking it up, and the drag that followed costs half speed — so the
  > accident landed exactly when you least wanted it, mid-escape, over the guard you
  > had just put down. Making it a spent turn makes it a **decision**, which is what
  > §7.2's body economy is supposed to be asking of you.
  >
  > **The overload is on Wait, and the wait keeps its look.** A take-hold wait still
  > gives the 360° for that turn: the turn is spent either way and the look costs
  > nothing to keep, so the verb loses nothing and no new key is invented. The only
  > consequence is that you cannot wait *over* a body without picking it up — stand a
  > cell off if that is what you want. Wait is also the verb a player taps most for
  > information, so the usable line names the new one (`body: wait to grab`,
  > §11.4); the hint is the mitigation, not a nicety.
  >
  > **It straightens the cupboard sequence.** A takedown made from inside a cupboard
  > leaves the body next to you, and stowing it used to need a square move — out, off,
  > back — because the grab only landed on the step *away*. Now it is a straight line:
  > takedown → step out onto the body → wait → bump the cupboard.
  >
  > **The pickup carries no haul debt.** The old grab rode a step, so the player got a
  > whole cell of movement on the turn they picked up and the weight caught up on the
  > next one. This one rides a wait — a full turn already paid, and nowhere gone — so
  > charging the debt on top would charge twice for the same grab. Half speed starts
  > from the first step, which is where §8.3's "one cell per two turns" lives.

### 8.4 Aiming

The old version had **no targeting system at all**, and its absence is the direct
cause of the free unlimited-range neutralise: auto-target-nearest-visible was the path
of least resistance (appendix 1). Everything below exists because of that sentence.

**An ability aims by where you stand and which way you face.** **[SETTLED]** The
vocabulary is **closed**, and it is these three:

1. **Itself** — the player's own cell. Run, Camouflage, Dephase, Autodoors.
2. **The facing cardinal** — the cell in front of you (Decoy), or the ray out from
   it (the Dart, §8.3/appendix 54).
3. **An area around the cell you fired from** — a radius taken as a **snapshot** at
   the press. Confusion, Lockdown, False Call. Walking away does not move it.

And the prohibitions, which are the load-bearing half:

- **No auto-target-nearest, ever, for anything.** Not the nearest guard, not the
  nearest door, not the nearest anything. This is the sentence the section exists
  for (appendix 1), and there is **no target list anywhere in the implementation**
  to snap to. Where an ability needs to know what it caught — the blast's guards,
  the dart's line — it measures from the aim, never the reverse.
- **No cursor, and no modal picking step.** An ability is answered by **one
  keypress**, from the bar or its mnemonic (§11.4/§11.6). Aiming is paid for in
  movement, exposure and turns — on the board, where the guards can punish it —
  rather than in a UI the guards cannot see.
- **A fourth way to aim is a design conversation**, not a quiet extension — the
  same fence §8.2 puts around the use budget.

One seam resolves all three: the press settles what it acts on *before* the ability
commits (`state::activation`), so no ability grows its own way of picking a thing
and therefore its own way of picking the wrong one (appendix 44). An aim that comes
back with nothing to act on refuses the press for **free** — no turn, no cooldown,
no use spent (§4.4) — and the §11.4 bar greys the entry from the same answer, which
is why a refusal is never a thing the player discovers by spending a turn. What the
record holds is the economy; **the aim is not a stored field** — it is the rule each
ability's own precondition applies.

> **[SETTLED] reversed here.** This section used to read *"build targeting up front
> — self, direction, and tile within range (with a cursor)"*. The cursor half was
> built, used by nothing, and cut in #556; the ban on auto-nearest — the part the
> section was actually written for — stands unchanged. **Appendix 61.**

---

## 9. Sensing guards

Sound was meant to be the channel that let the player steer guard attention and track
threats around corners. It was built in this rebuild — propagation field, hearing
guards, loudness ladder — and **came out obscure and not fun**: an invisible field
doing its work behind the UI, because **an invisible sound system is a missing one**
(appendix 13).

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
- **A run can be dealt it switched off** (§12.6/#493). The *"nothing felt through
  walls"* modifier suppresses this channel and the door channel (§9.4) whole: no dot
  at any range, through any wall, and no door cue. It takes away **scouting**
  information and never **fairness** information — a seen guard, its cone and the
  §11.5 danger overlay are untouched, and so is the standing watcher line of an
  unseen guard that has you (§11.5/#465), which is §2.2's floor rather than part of
  this channel. Three consequences follow, and they are stated rather than left to be
  found:
  - **Wait keeps its sight half and loses its sense half.** The 360° look remains —
    still the only way to see behind you (§8.3) — and the widening above does nothing.
    That is the innate verb narrowed on purpose, not hollowed out.
  - **Confusion is unaffected**, because the suppression is of the *channel* and not
    of the range. §8.3's **[SETTLED]** clamp `min(CONFUSION_RADIUS, sense_range())`
    reads the range, which keeps its value, so the blast still freezes every guard a
    sensing player would have sensed — you simply do not see it land. A modifier that
    had zeroed the range would have deleted an ability instead of taking away
    information, which is the dead-verb failure §13.2's histogram exists to catch.
  - **A duct's cost narrows with it** (§10.7): `DUCT_SENSE_RANGE` has nothing left to
    shrink, so the crawlspace still costs its blinded sight and no longer its degraded
    sense. Ducts are relatively *safer* under this modifier.

### 9.2 Seen vs. sensed — the two states of a perceived guard

A guard the player perceives is in exactly one of two display states, and the gap
between them is the whole design:

| State | When | What the player sees |
|---|---|---|
| **Sensed** | In sense range, **not** in the player's field of view | An orange **background** highlight on the guard's **exact cell** (no glyph of its own), with a short **fading trail** of the cells it was just felt in (§9.5). No facing, no cone, no danger overlay. You know *where*, not *which way it looks*. |
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

### 9.3 What the sense keeps, and what the swap obliges

The sense gives **position, not attention**: the dangerous unknown — *is it looking at
me?* — stays tied to line of sight, which is where the whole game already lives. It
rewards Wait, the game's one "spend a turn to know more" verb, instead of bolting on a
parallel system. And it costs a range check and a render state where sound cost a
subsystem, which is §3's "honest pressure systems" applied: a system that isn't fun
doesn't earn its complexity (appendix 13).

> **The obligation it leaves on §7.3 and §7.7.** Both leaned on sound for legibility —
> the player was meant to *hear* a ping. Every radio event and every call therefore
> needs a **visual / near-line** cue instead: a near-line message and the responder's
> own sensed dot peeling off toward the cell it was sent to, **never a sound.**

### 9.4 Sensing doors

The sense has a **second channel**, built the same way (§9.2) and for the same
reason. A door opening or shutting **away from you** — a guard routing through a
closed door and walking it open (§10.4), a Calm guard shutting one behind itself,
an automatic door timing out — is *evidence that someone passed* (§10.4). As a
transient near-line word ("the door opens") that evidence was easy to miss, cleared
on your next action (§11.7), and never said *where*. So it becomes a **positional,
on-grid cue**, exactly like the sensed guard.

> **Which of those three sources fires depends on the run's door vocabulary**
> (§10.4/§12.6/#452). The channel is fed by guards opening and closing manual doors in
> the **baseline** facility, and a timeout never fires there because there is no
> automatic door to time out. With the modifier on it is the other way about: no guard
> ever closes a door by hand, and the timeouts are the whole of the second half. The
> channel works either way — it is not a claim about a mixture, and never was — but a
> reading of it has to know which vocabulary the level is speaking, because *what a cue
> means* differs: a manual door shutting is somebody's hand, a timeout is only that
> nobody has been in the doorway for a few turns.

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
  single-frame flash and not a permanent stain. Since #192 the *guard* sense fades the
  same way, on the same machinery: see §9.5.
- **Open and shut share one cue** — it is the same evidence, and both drive it.
  Where the cue coincides with the danger overlay, being seen outranks it (§11.5).
- **A door *you* operate** keeps its quiet near-line self-narration (§11.7) and
  lights no cue — you already know; the cue is for the doors you did *not* move.

This shares the `Sensed` category with the guard sense, so the light-mode reskin
(§11.2) covers both at once — and it is switched off with it: the §12.6 *"nothing felt
through walls"* modifier (#493) suppresses **both** halves of the one channel, so a run
that cannot feel a guard through a wall cannot feel a door change either. One channel,
one switch (§9.5).

### 9.5 One channel, one fade [SETTLED]

The sense had two halves that behaved differently: the door cue persisted and faded,
the guard sense was a hard on/off dot at the live range. They now run on **one rule**
(#192, appendix 34):

> **Every turn, the sense stamps a mark where it felt something. Marks fade.**

A mark is a **cell and an age**, nothing more. The core states the age; presentation
owns what age looks like (§11.2). What the one rule produces:

- **A trail.** A sensed guard on the move leaves the cells it just occupied marked
  behind its live cell — *was just here*, fading to nothing.
- **A ghost.** A guard that leaves the sense box — walks out of it, or falls out when
  a Wait's widened box (§9.1) lapses back to the walking one — leaves its last felt
  cell on the board for a moment instead of blinking out. "I have lost it, and it was
  just there" is a fact worth acting on, and it is the half of this that is genuinely
  new information.
- **A door cue that fades visually**, not just in its own bookkeeping: its first turn
  is the bright mark, the rest of its life the quiet one.

**The trail is deliberately the shortest thing in the channel.** `GUARD_CUE_DECAY_TURNS`
**[START]** = **2**, against the door cue's 3, so at most the live cell and the one
behind it are lit at once. §9.2's restraint is the bound: the sense gives **position,
never intent**, and a trail long enough to extrapolate is an **arrow** — heading handed
over for free. Two properties keep it honest, and they are why the trail leaks nothing
the board did not already show:

- **A guard standing still leaves no trail at all** — it re-stamps one cell. The
  watcher whose facing you would most like to know is exactly the one that gives you
  nothing.
- **A guard on the move was already legible frame to frame** (the dot was there, now
  it is here). The trail makes what an attentive player could already read *legible*;
  it does not add a channel.

**A guard you can *see* stamps nothing.** Sight already draws it whole — glyph, facing,
cone (§9.2) — so a trace under it would be the sense restating sight in the one colour
that means *not seen*.

**Two strengths, and they mean age, not fog.** `Sensed` paints the full fill on a mark
made this turn and the quiet one on the fading tail (§11.2/§11.5). Nothing sensed is
ever inside the FOV, so fog has nothing to say about the channel; freshness is the only
thing a strength here can honestly mean. Two steps are what a two-turn trail can
carry — a longer ramp would need a longer trail, which is the arrow.

**Precedence, refined** (§11.5): the **live** sensed dot keeps its place above the
watcher line (#465) — the line's own endpoint is that guard. The channel's **fading**
marks sit below it: a trace of where something was a turn ago must never cover a red
line that says a guard has you *now*. The danger overlay still paints last and still
wins over all of it — **being seen outranks**.

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
   it, and the fully-backed geometry alone keeps every backing intact.)* The backing
   is **manufactured** by step 5a and the cupboard placed deliberately, rather than
   harvested from the rare natural pockets as it once was (appendix 15).
7. **Entry/exit and player** go in the **largest room**, at random empty cells.
8. **Objectives** go in any room *except* the start room.
9. **Guards** go in any room *except* the start room.

### 10.1a Corridors must have cover — the sightline rule

**[SETTLED]** — this is a direct consequence of §7.6, and it is the generator's
most important job after connectivity.

Corridor-first partition is the right structure (see §10.1) but it has a severe
emergent flaw that only shows up in play: **it produces long, dead-straight,
full-span corridors with no cover, and those corridors are where the player flees.**
A 38-cell straight 3-wide corridor with a guard in it has no counterplay. It is not a
space; it is a sightline. Appendix 15 has the history, including how the rule's
wording moved.

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
the mouth's lane flees to the same cupboard one step later.

**The rule constrains the generator, not the player.** **[SETTLED]** (#303) Pierce
Wall can punch a hole into a corridor's long wall and create exactly the uncovered
straight run this rule forbids — and that is correct, not a loophole to close. The
rule exists so a level is never *born* with an unsurvivable sightline; a player who
cuts one has made a choice, and the danger overlay (§11.5) draws the new cone the
moment a guard's line reaches down it. The assertion below is a property of
*generation*, and it is checked there.

This is a **testable property of a generated level**, not a vibe. Assert it, the
same way reachability is asserted (§10.6):

> For every cell, for each of the 4 cardinal directions, the run length without
> an obstruction, a cover cell, or a cupboard within two moves is ≤ *L*.

**Which counterplay a run gets follows its region [SETTLED]: tables are room
furniture, corridors get architecture.** A lone table reads as noise, and a table in a
corridor reads as a barricade in a hallway — so neither is generated, and both are
asserted away (appendix 15). Concretely:

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
  behind on the other side, or bursting through it accomplishes nothing. **Near, not
  *in the frame*** (#387): a table or a repair pillar stamped orthogonally against a
  door cell clogs the one cell everything funnels through, narrows the burst-through
  to a squeeze, and gives the mouth a doubled usable — `→ door: open` *and*
  `↑ table: crouch`. Cover a cell or two into the room still serves the
  burst-through, so the throat rule moves furniture out of the doorway rather than
  clearing its neighbourhood. It is a **preference**, like the rest of §10.6's
  placement rules: the bench pass exists to repair a §10.1a sightline and §10.1a
  outranks the preference, so a last-resort bench may still take the frame rather
  than fail the carve. Measured over 300 seeds, that fallback is needed on ~2% of
  them. **Two duct entries, by contrast, are refused outright** — a duct is optional
  and capped, so refusing a crowded entry costs the next combination, not the level.

A run none of the repairs can break rejects the carve like a reachability
failure. (**Jogging the corridors** mid-carve — offsetting a corridor a cell or
two mid-span — remains the unimplemented alternative if §15.2 wants it.)

**Hideouts must be reachable while fleeing. [SETTLED]**

Place hideouts along the corridor network and near junctions, not only in rooms, and
do not stop the placement pass at the first failure. **A flight path with no hideout
on it is a failed flight path** — the old placement left the hiding game with no board
at all (appendix 15). This is worth asserting too **[OPEN]** — something like "every
cell is within *N* steps of a hideout" — but the right metric is unclear and probably
wants play evidence first.

### 10.2 Parameters

**v1 ships quick play only** (§14), and quick play is **training**: one tuned
configuration, played to learn the building rather than to be survived once. That is
what earns its end screen a *retry this level* and a *new run* where the campaign's
will have neither (§2.2, appendix 31) — the exits belong to the mode.

| Parameter | Value |
|---|---|
| Size | **40 × 40** **[START]** |
| Guards | **4** **[START]** — a **level modifier** moves it (`guard_count`, §12.6/#232/#565): a signed delta of up to two either way, within **2…6**. The *Archive* names two more on its own (#217) |
| Intel | **3** **[START]** — the same knob on the reward axis (`intel_count`, §12.6/#207/#565), within **2…5**. The *Archive* names two more (#217), which is what took the ceiling from four to five |
| Exit rule | **A level modifier** (`intel_to_exit`, §4.5/§12.6/#244): quick play = **all three**, campaign and the sim = **at least one objective** — the minimum haul, a console or a crate, nothing spent (#574) — except the **archive**, whose node sets *all of it* and makes the run's one mandatory complete objective (#217) |
| Starting abilities | **A level modifier** (`starting_abilities`, §8.3/#244): quick play grants the innate set **plus three random tech**, seeded (§12.4); the sim grants the **innate set only**; campaign accumulates instead (§2.2) |

**The sim plays bare, and that is the point.** The headless baseline (§13.2) holds
no salvaged tech — Run and the innate verbs, nothing else — because **a level must
be winnable with no tech**. Tech is what makes a run *better*, never what makes it
*possible*; measuring the bot with a full loadout hides a facility that is only
survivable because something was handed out. The guard count is tuned against the
bare number, so every tech draw on top is upside.

**Where 4 came from.** The `--guards` sweep is roughly linear at about **8–10 points
of win rate per guard**, with no cliff, so the number is a taste call rather than a
threshold; **4** is the forgiving-but-real end, giving a bare bot run a 37% win rate
(appendix 26). Read it against §13.4: this is a floor, not a forecast.

**And one guard is one difficulty step.** That same linearity is what makes the count a
good level modifier (§12.6/#232): the knob moves it a step at a time, bounded to **2…6**, so
each end is worth roughly one sweep row — measured at 43% / 35% / 25% over 300 bare bot
seeds. The envelope is stated rather than open-ended: at its floor a facility is a walk
rather than a raid, and at its ceiling a screen-bound 40×40 board (§11.4) crowds and the
§7.5 beats cut too small. Six is the top because the **archive** asks for it (#217) — the
one facility a run is told about from the first frame and walks six raids to reach — and
it stops there: an alerted terminus is still six. The knob is a **step, not a setter** — it never moves the
count the way it does not name, so a sim sweep already outside the envelope is left
where it is rather than dragged into it.

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
| **Door panel, closed** | `+` | Yes | Yes | **No** — the bump *opens* it |
| **Door panel, open** | (blank) | No | No | No |
| **Hideout, empty** | `}` | **Bump** | No | Yes |
| **Hideout, occupied** | `}` **(you)** | Yes | No | Yes |
| **Duct entry** | `=` | Yes (**player: Bump**) | Yes | Yes |
| **Partial cover (table)** | `π` | Yes | **No** | Yes |
| **Console** | `$` | Yes | No | Yes — the bump *uses* it |
| **Comms console** | `Ψ` | Yes | No | Yes — the bump *uses* it |
| **Equipment cache** | `¤` | Yes | No | Yes — the bump *uses* it |
| **Exit** | `E` | Yes | No | Yes — the bump *uses* it |
| **Player** | `@` | Yes | No | No |
| **Guard** | `g` | Yes | No | No |
| **Body** | `z` | No | No | No |
| **Decoy** | `@` | **No** | No | No |

Vision is blocked when a cell's summed opacity reaches 1.0 — opacity itself is
still all-or-nothing, no half-shadows, no glass. **Partial cover exists as the
table**, and its concealment is *behavioural*, not optical: sight passes over it
freely; what it grants is the crouch (below). **[START]** — low walls / vaulting
stay a future axis.

**A bump that *uses* is not a bump that *opens*.** A closed panel is pathable
because walking into it opens the way (§10.4); a console or the exit is not,
because nothing a bump does to one lets anyone past. So a solid usable stamped
into a one-cell throat seals the ground behind it off from guards and player
alike — which §10.6's assert does not catch, since it proves the objective route
and orphaned ground holds no objective (#477, #481). **Placement therefore refuses
such a cell outright** (§10.6, appendix 38): a usable is a wall to anything asking
what a stamp severs.

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
> line, looking along the bench rather than across it, is on neither side: there a
> table of the run on the sight line between you also conceals, corner grazes
> included, out to the exact 45° diagonal. A **bent** run — an L, where two stamped
> benches touch — has no single axis, so **each arm contributes its own half-plane
> and they union**. A **one-cell** run has no arm at all — the generator never makes one
> (§10.1a benches are 2+), and the deployable **Cover** (§8.3/#562) is the only thing that
> can — so it takes the degenerate arm: **the line perpendicular to the direction you are
> covering from**, through the piece's own cell. Push a piece east and stand behind it and
> you are hidden from everything east of it, read exactly as a bench's half-plane is;
> hugged on the **exact diagonal**, where neither axis dominates, there is no honest line
> and the ray test grants the quarter-plane it always did. The alternative — leaving a lone
> piece on the ray alone, the pre-#377 wedge — would have made the one piece of cover a
> player places themselves the one piece they cannot read at a glance. Concealment is directional, per-guard, and per *the run you
> ducked behind* — not every table you happen to stand beside, which is what keeps a
> bench weaker than a cupboard (omnidirectional, contact-safe). A crouched player
> **can still be captured by contact** (§4.5); unseen is not safe.
> The crouch spends the turn; **waiting holds it** (hold still, watch the cone sweep
> past, §7.6); and so does the **crouch-walk**: a plain step that lands still
> touching the run — orthogonally or on the diagonal at a corner, so you can round
> the end of the bench without standing — keeps the pose and moves you at full speed
> **[START]**. Any other spent action stands you up; a free action changes nothing,
> posture included (§4.4).
> *(The half-plane is the third shape this concealment has taken, and deliberately
> the more generous one, because a counterplay §10.1a places in every corridor has to
> be **readable before you spend the turn**. If bench-hugging turns out to dominate,
> the levers are the turn it costs, contact-vulnerability, and the crouch-walk's
> requirement to keep hugging — **not** re-narrowing the geometry the player has to
> read. Appendix 14.)* Legibility rides the same
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
> **locked**. From inside a cupboard the whole sequence is a straight line since
> #451: takedown → step out onto the body → **wait** to take hold → bump the cupboard.
> It used to need a square move — out, off, and back — because the grab only landed
> on the step *away* from the body (§8.3). A locked cupboard is no longer a hideout: it holds a body, so you
> cannot climb in, and bumping it is an inert no-op. It shows the body's **`z`** in
> the **Neutral** colour (not the empty `}`), so a glance tells you which cupboards you
> have spent this way — Neutral because that list of three is *the* definition of a
> spent object (§11.2, "inert scenery, spent objectives"), the same transition a
> drained console makes from Interest to Neutral. **Owned is reserved for what is
> working for you right now** — the body still in your hands is Owned; the cupboard
> that swallowed it is not. On this furniture especially: Owned on a cupboard already
> means *you are concealed in this one* (§11.3), and one colour cannot also mean
> *this one is used up*. That status is **remembered** (§11.5a): once seen, a
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
  - **The second exception, and the axis itself (#236): a lock with a key.** The
    **locked prize room** is a level modifier (§12.6), and where the Lockdown seal
    below is the player's, this one is the building's. One room — a room hiding an
    **equipment cache** if the facility hides one, otherwise a room holding an
    **intel console** — has *every* one of its doorways key-gated. So what the lock
    gates depends on the run: in quick play, where there are no crates and the exit
    wants all the intel, it is a gate on the **win**; in a campaign facility rich
    enough to hide crates it gates **loot**, and skipping it is a real choice.
    - **Every guard carries the key**, so the price is exactly one **takedown**
      (§7.2) and its whole §7.3 cost — a body, a clock, evidence to hide — and not a
      hunt for one particular `g` the player has no way to pick out. It goes straight
      to hand: the body is the price, and a key on the floor would be a second errand.
    - **The gated doors are automatic** (below), and that is what makes the lock
      hold rather than the flavour it looks like. Guards have keys, so they open the
      door walking through it as they always did; a lock that let the doorway stand
      open would last until the first patrol came past and never again. Frameless and
      self-closing, it shuts a few turns after it is last vacated — and those turns
      are the modifier's one bypass: **slip in behind a guard**, with nothing in your
      pockets, at the price of standing next to the guard that just opened it.
    - **The lock is on the way in, never on the way out.** From inside the room the
      door always opens, key or no key, so the slip-in can cost a run its stealth but
      never the run itself (§2.2/§7.2's soft-lock class). Every door joins a room to
      a corridor, so "inside" needs nothing remembered.
    - **§10.6 gains a second assertion** (appendix 46): with every keyed doorway
      treated as a wall, the player must still reach the exit, everything outside the
      locked room, and **a guard** — a lock with no key in the building is the soft
      lock in a different hat. A board that fails it is redrawn.
  - **One bounded exception so far (#242): a lock that expires.** The **Lockdown**
    tech (§8.3) seals the doors around you for its window — a sealed door refuses a
    *guard's* walk-in open, so guard routes treat it as solid and go the long way,
    while the player bumps it open as always. It is a lock on the **handle**, not a
    hold on the door: a sealed door standing open is as passable as any other. The
    lock lives on the door itself, one representation for every lock source, so the
    key-gated doors above extend it rather than inventing a second — and the ability's
    duration is the only clock any seal has, which is what keeps this side of the
    **[START]** baseline and clear of §2.2/§7.2's soft-lock class.
    - The two sources **compose**, so the lock on a door is a **set** rather than one
      value (#236): a lockdown window over a key-gated doorway seals it for the window
      and leaves the key gate standing when the window closes. As one value apiece, an
      ability whose whole promise is that it is temporary would have unlocked the prize
      room for the rest of the run.
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
  - **Automatic doors** (#147) are a **level modifier** (§12.6/#452), all or nothing:
    the baseline facility is entirely **manual and hinged**, and with the modifier on
    *every* door is automatic. An automatic door is generated **frameless** — no
    hinges, the whole span is panels — so there is no handle to shut it by hand. They
    close *themselves* a few turns (**[START] ~5**) after the doorway is last vacated;
    an actor standing in the throat holds them open (never a crush). The delay is a
    stealth window: a guard passing through leaves the door open just long enough to
    slip after them.

    **A run speaks one door vocabulary.** They used to be a **[START]** *fraction* of
    every facility, drawn per doorway, so every level was a mixture and which
    vocabulary a given door spoke was a coin flip you found out by walking up to it —
    which is not a decision, only a surprise. Which vocabulary a run plays is now a
    stated property of the run.

    The geometry differs, and that is most of what the modifier is: an automatic door
    is a **3–6 panel span** where a manual one is two hinges around 1–4 panels, so an
    all-automatic level has systematically **wider throats** and longer sightlines
    through every doorway (§10.1a's concern, and #387's).
- **You sense a door change away from you** (§9.4): a door opening or shutting that
  you did not cause lights a fading on-grid cue over its **whole footprint**, in the
  same orange **`Sensed`** channel as a guard felt through a wall, at its own longer
  `DOOR_SENSE_RANGE`, so "someone passed through there" stays legible around a corner
  rather than living only in a transient near-line word.

### 10.5 The spatial model — fix this properly

**The generator already builds a graph — corridors are nodes, rooms are nodes, doors
are edges — and then discards it.** **[SETTLED]: keep the graph.** The level's spatial
model is named regions of arbitrary shape (including corridors), explicit door edges
between them, and a cell→region lookup.

This is the highest-leverage structural decision in the document. Nearly every
"guards should…" idea depends on it — the old version's single axis-aligned rectangle
could not address a corridor at all, and that one missing abstraction is what kept
guard cooperation, assigned patrols, keys and circuits unbuilt (appendix 16).

### 10.6 Guarantees

**Guarantee, and test:**

| Guarantee | Basis |
|---|---|
| Fully enclosed | Unconditional border ring |
| Corridor network connected | Each corridor punches into its parent → the network is a tree |
| Every room reaches a corridor | Every room is bounded by corridor walls, which qualify as door candidates |
| Every room ≥ 6×6, ≤ ~12 rooms | Partition constants |
| **A path exists: start → every objective → the comms console → exit** | **Assert it. See below.** The flood starts where the player first sets foot in the facility: the cells they can climb out of `E` onto (#466) |
| **No solid usable seals walkable ground off** | A console, an equipment cache or the exit is a wall to a route (§10.3), so a candidate cell whose stamping would disconnect the walkable graph — under *either* movement rule, guard and player — is skipped, and the finished board is asserted one component per rule. A filter with a fallback, not an assert-and-redraw; appendix 38 |
| **The exit's tunnel reaches the border, over inert geometry** | A straight run of plain wall or floor, 4–12 cells, sharing no cell with a §10.7 shortcut; a candidate `E` without one is redrawn |
| **The comms console is a real detour** | ≥ 16 cells from the spawn, non-start room **[START]** (§7.3) |
| **Every equipment cache is a real detour, and reachable** | ≥ 16 cells from the spawn, non-start room **[START]**, one per room where the carve allows, and in the reachability flood above (§14 v3). Optional to *take*, never optional to *reach*: a crate you cannot bump is not a choice you declined |
| **One usable beside any floor cell (preferred)** | Conflict-aware stamping, best-effort; the arrow disambiguates the rest. See below. |

**Do not rely on a structural argument. Assert reachability and reject the seed.**
It is a flood fill. It costs nothing. It is exactly the kind of property a
generator must never merely *believe* — the old generator's structural argument had a
hole that sealed rooms with their objectives inside, and nothing ever detected it
(appendix 17).

**One usable per cell — a preference, not a guarantee.** The usable line
(§11.4) points each bump with its own arrow, so a floor cell beside **two
distinct usables** (a door, a table, a cupboard, either console, the exit; a
multi-cell door counts once) is still *legible* — `→ door: open` and `↑ table:
crouch` are two aimed actions, not one ambiguous prompt — but it reads cleanest
at one. So every stamping stage **avoids crowding where it cheaply can**:
cupboard sites that would double up are skipped (sites are plentiful), and
console (intel and comms) and exit candidates prefer a clean cell, falling back
rather than failing the draw.

**It is not asserted, because connectivity and the sightline rule (§10.1a) outrank
it** — §10.1a's repairs must land where the run is, so a doubling with a nearby door
is sometimes unavoidable. Best-effort placement plus the arrow, *not* a
flood-fill-style assert-and-redraw: as a hard guarantee it rejected ~85% of carves and
stalled generation (appendix 17).

Two more spacing rules, both fixing real old-generator faults (appendix 17):

- **Space the placements.** Nothing used to separate the player from the exit (they
  could spawn adjacent), spread the intel out, or keep a guard from spawning where it
  saw you on turn one. The pillar says *"the starting area should be safe"*; make it so.
  Since #466 the first of those is the **tunnel's own length** rather than a spawn-to-exit
  distance — the player starts at the way out, so `EXIT_DUCT_MIN_CELLS` (**[START]** = 8,
  cap 16) is what keeps a run from starting on top of its objective — and the turn-one
  cone rule now protects the **mouth** they climb out of, the crawl itself being
  concealed and contact-safe (§10.7).
- **Fail loudly or retry the seed.** Placement must never fail silently — asking for 5
  guards and getting 4 with a log line nobody reads is not acceptable.

### 10.7 Ducts — player-only crawlspace shortcuts

**[START]** A **duct** is a crawlspace that spans the facility and only the
player can use: a shortcut between two far-apart parts, paid for not in time but in
**degraded information**. It extends §10 without touching any existing contract — a
guard experiences a duct's **entries** as ordinary wall and never perceives the crawl
route at all; the interior cells it may pass over keep their own terrain, so nothing a
guard sees, paths on, or looks through changes.

**The player's own tunnel is a duct too** (§4.5/#466, appendix 32). The exit `E` is the
inner mouth of a **linear** run of duct cells going out to the level border, and the
far end of it is not an entry but the **way out**: no mouth, nothing to climb onto but
the world, and a step off the board there is the §4.5 win. Its mouth answers the intel
gate before it lets you in, so a crawl home is never begun in vain; and the occupied run
is drawn in the **exit's** colour rather than the crawlspace's (§11.2), so the opening
frame is one line from the border to `E`. It is the same model in
every other respect — concealed and contact-safe inside, memory and a shortened sense for
perception, the mouth peek at `E` — and it stamps nothing: the border cell keeps its wall
terrain (so §10.6's enclosure is untouched) and `E` keeps its own glyph. The one thing it
adds is where the run *starts*: on that border cell, inside the crawlspace, a few crawl
steps from the mouth.

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
remembered. The **interior path** is **not on the base map at all** — it carries no
tell, so it can cross a room's floor without giving the shortcut away — and lives in
**its own layer, shown only while you are crawling it**. It is **not remembered**:
climb out and the path is hidden again. *(Both rules replaced earlier ones — the entry
was to be visible from turn one, the interior remembered once crawled. Appendix 18.)*

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

> **What the crawlspace costs when the sense is already off** (§9.1/§12.6/#493). The
> reduced `DUCT_SENSE_RANGE` is half of what a duct charges, and a run whose sense is
> suppressed has nothing there to shrink — so what a duct still costs is its blinded
> sight and the deliberate pause on the entry cell, and it is *relatively* safer under
> that modifier than under the baseline. Said here so §9 and §10.7 are not later found
> to disagree; it is a consequence taken deliberately, not a hole to plug by
> special-casing the crawlspace.

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
| **Ground** | Dark gray | Traversable floor — the §11.5 dots, drawn to recede; dotted only while in sight |
| **Owned** | Blue | You, and things you made |
| **Caution** | Yellow | A threat that is unaware |
| **Warning** | Orange | A threat that is hunting |
| **Danger** | Red | A threat that has you |
| **Sensed** | Orange (background), two strengths | **Sensed through a wall** (§9) — a guard, or a door that just changed away from you (§9.4); an eye-catching highlight, position only. Full strength on a mark made **this turn**, quiet on the fading tail behind it (§9.5) — the strengths mean *age*, never fog |
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

The two places sit at different heights in the §11.5 precedence, which owns that
ordering: a **cell** mark is a wash and the weakest background there is, while a mark on
a **thing** refines a cue that thing already draws and so outranks Sensed. One
consequence belongs here rather than there: a mark on a thing **inherits that thing's
own visibility rule** and adds none, so it can never reveal what the fog is hiding —
which for a decoy (§11.5a's second exception) means it is drawn out of the FOV exactly
as the `@` under it is.

If an effect background ever reads badly under the glyph standing on it, **shift the
Effect colour** — the channel is not negotiable, the hue is.

Base palette: a 16-colour, colour-blind-safe qualitative set, each usable as
foreground and as a background variant that recedes toward the page. **There are two
of them** — a dark theme and a light one (#189), whose **home is the help panel's
Options tab** (§14 v2/#513): a `theme` row that draws the live value and stores it, so a
reload comes back in the theme the player chose. The `n` key stays a standing shortcut on
every surface that forwards it, and the campaign map keeps a drawn `theme [n]` control
because it has no route to that tab yet; the panel's own footer button went with the
setting. This is exactly the reskin this section's rule was written
to make cheap: the core gained a `Theme` flag and not one colour, and every §11.5
guarantee is asserted over both tables. The concrete rows, the constraints the tests
hold them to, and why each exception exists are in
[`docs/render-reference.md`](render-reference.md) §4.

**[START]** — **full-range colour**, no gamma compression unless something demands
it. The old palette compressed everything into 0.1–0.9, so there was no true black and
no true white (appendix 1).

### 11.3 Glyphs

> **The glyph table itself is [`docs/render-reference.md`](render-reference.md)
> §2** — every mark, its category, and the reasoning behind it, in one place. This
> section owns the *rules* a glyph has to obey; the reference records what they
> resolve to, and the values live in code (`Terrain::glyph`, the entity constants in
> `render.rs`), from which the in-game legend also derives. Do not keep a second copy
> of the table here: one drifted for a whole release — §11.3 still called a duct entry
> geometry visible from turn one after §11.5a had made it contents.

Four rules the glyphs answer to, all **[SETTLED]**:

- **A glyph says what is there; the colour says what it means to you now.** So a
  shifting meaning **recolours rather than changes shape** — a spent console, a
  cupboard holding *you*, the run of tables concealing you (§10.3, §11.2).
- **A seen guard's colour is the state machine, read directly** — Caution → Warning →
  Danger, plus its facing and cone (§9.2).
- **A sensed guard has no glyph at all** — only the Sensed background on its cell
  (§9.2). A glyph would imply a readable mind, and the player has position only.
- **Overlapping glyphs need a priority order** — define it. Last-writer-wins renders a
  guard in a doorway arbitrarily (appendix 1).

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
  the line falls back to quiet **ambient status** instead of sitting empty. Its
  two view toggles sit at **opposite ends** of the row: the
  `[?]` help button in the screen's **top-left corner**, the message log's deploy
  control flush against the row's **last column**, and the message between them. They do different jobs and belong at different ends — `[?]` is the
  fixed landmark a lost player reaches for, and column 0 makes it the one control
  whose position depends on nothing at all (not the screen width, not what else is
  up), while the deploy control comes and goes with what there is to read, so it
  takes the end that is allowed to change. Splitting them also stops the pair from
  taking one contiguous bite out of the row's right-hand side, which is where a
  long message ran out of room.

  **The deploy control is three cells, always**, and one glyph carries its whole
  state: `!` when this action raised more than the near line is showing (the one
  state worth interrupting for — it is new, and the next action clears it), `▾`
  when only the remembered turns are behind it, `▴` while it is deployed. It used
  to be `[+2 ▾]`, a chevron plus a count of the further messages: six cells on
  every frame it was up, spent on a number the player gets for free by deploying
  it. The near line's words are the scarcest space on the screen, and merging the
  two — then reclaiming the blank cell that had sat beyond the control, which was
  band rather than air and separated it from nothing but the edge of the screen —
  took the budget from 28 glyphs to 32. Three messages that had been clipping since
  before the bound existed now fit, with no wording changed. The `[?]` wears **one
  static colour** — the System tan every HUD control wears — at every alert rung. It
  was tinted by the rung (§7.3) for as long as the near line could only state a step
  on the turn it happened; now that the row itself carries the standing alert, in
  words and in the colour of its band, a tinted button was a second and quieter
  statement of what the row already said (see
  [`docs/render-reference.md`](render-reference.md) §4.5).

  **An ambient band paints the dim background shade; a message band paints the live
  one.** The row's colour then distinguishes *the facility's standing mood* — a quiet
  permanent tint — from *something that just happened*, which flashes and is gone on
  the next action (§11.7). It is also what lets the row carry the alert condition
  without spending the §11.5 danger overlay's own fill on a standing fact: that shade
  means **a threat has you right now**, and a permanent row wearing it would dilute
  the one place it is true. A HUD row has no fog to derive the choice from and says
  outright which shade it wants — reaching for the map's "explored" knowledge state
  to get the dim one for free would pick the right colour by telling a lie.

  **The band stops at the controls.** It runs edge to edge across every cell the words
  can use — including the ones they do not fill — but under the `[?]` and the deploy
  control the row keeps the screen's own background, so the one static tan those two
  wear is read against the same backdrop as every other control on the screen rather
  than against a tint of the standing mood, which is not enough separation and left
  the row's two most important controls its least legible. Held back the control's own
  three cells each, so the band meets the button edge to edge and stays one continuous
  run; this is **paint and never layout**, and the row's capacity is untouched. The
  span is read off the same layout as the drawing and both hit-tests (see
  [`docs/render-reference.md`](render-reference.md) §5).

  > **The row is laid out once, and the message's width comes *from* it.**
  > **[SETTLED]** Both controls, both hit-tests and the words' span are read off a
  > single layout: the message starts clear of the `[?]` and stops a cell short of
  > the deploy control. Deriving each position separately and letting the budget
  > follow whichever control happened to be nearest is exactly the arrangement
  > where adding a control silently runs the words underneath it.
  >
  > **A new near-line message must fit that budget**, and a new control must take
  > its width from the same layout rather than assume the old one. The budget is the
  > row minus the `[?]`, its cell of air, the cell of air before the deploy control,
  > and the control — **32 cells** on the 40-wide v1 board (§10.2). Count it in **cells of message**, never as the column the words stop
  > at: those differ by one, and the bound spent its whole life as the column,
  > quietly passing messages one cell too long. A control that has to say more
  > belongs in a **fixed** width with a glyph that varies, not a width that grows
  > with what it says: widening the chrome takes the cells straight off the message. It is pinned by a test that walks every message
  > `message_for` can build, with an explicit, only-ever-shrinking list of the few
  > that predate the bound; an over-long message clips silently on a real screen,
  > which is why the check is a test and not a hope.
- **Usable line** — *what you can act on*: the affordances available where the
  player stands, each **with an arrow giving the bump's direction** (`↑
  console: take intel`, `← table: crouch`, `↓ cupboard: hide`, `door: open →`).
  Not a message — a **pure derived function of state**, recomputed every frame, no
  plumbing. When nothing is available it falls back to the **innate-verb floor**
  below rather than sitting empty (#323). The arrow makes each bump an aimed
  "press this way, get that", so even the rare cell beside two usables stays
  unambiguous — one row lists each with its own direction. The generator
  *prefers* one usable per floor cell (§10.6, best-effort) to keep the common
  case to a single line, but does not guarantee it.

  **One entry has no direction** (#451). Taking hold of a body is about the cell the
  player is standing **on**, and its press is a **wait**, not a bump — so the line
  carries `body: wait to grab` with **no arrow at all**, drawn among the centred
  group where the player's own cell is. An arrow would be a lie in the one channel
  this row exists to be trusted in: the whole promise of the aimed row is *press
  towards the words*, and there is nowhere to press. The discipline is unweakened,
  only widened from *mirror the bump* to **mirror the press** — the entry is derived
  from the same state the Wait acts on, so it appears exactly when the wait would
  take hold and never otherwise, and a stunned or phased player is offered it no more
  than they are offered anything else (§8.3).

  **Every label is bounded at 21 cells** (`LABEL_MAX`), checked at compile time over
  the complete set. The row already accepts that *two* long labels overrun a 40-wide
  board and falls back to the packed list for them, so the bound is not a promise that
  any pair fits — it is a guard against the case #451 shipped and a phone found: one
  label so much wider than the rest (`body: wait to take hold`, 23 cells) that a pair
  which used to fit started clipping. `status_row` clips in silence, so nothing but a
  bound catches it before a screenshot does.

  It says **`wait`** rather than carrying a clock glyph because §11.3's table has none
  for *this costs a turn*, and the shipped font stack falls back to a generic
  `monospace` on some devices — an unguaranteed codepoint would come out as tofu in
  the one row whose job is to teach a verb. Adding a timer glyph is a §11.3 change
  with its own justification to make, not a side effect of naming an affordance.

  **The row is aimed, not packed** (#384): each entry draws where its direction
  points — **west** flush left, **north and south** centred, **east** flush right
  with its arrow **trailing** the words so it points off the right edge the way
  the west entry's points off the left. The row is a tiny compass around the
  player: *press towards the words*. Labels reach 21 cells, so two long entries
  already overrun a 40-wide board (§10.2); when the groups will not all fit, the
  **whole** row falls back to the packed left-to-right list with every arrow
  leading — one rule, no half-aligned hybrid, and never a clipped word. The
  innate-verb floor is never aimed: it describes the keys, not the geometry.

**The ambient floor carries the two standing facts together** (#421). The momentary
states come first and own the row while they last — stunned, hidden, crouched,
dragging — because each is a state you are *in* rather than a fact about the run, and
each ends. Underneath them the floor states both of the facts that never expire:

```
objectives: 1/3                    a quiet facility        15 of 32 glyphs
objectives: 1/3 - security: 2      once the ladder steps   29 of 32 glyphs
```

It used to **choose**: at rung ≥ 1 it said the condition and the objective vanished;
at rung 0 it said the objective and the facility went unmentioned. The player needs
both, and the row has space for both — `objectives: 10/12 - security: 3` is 31 of the
32 the near line leaves beside its controls, so it fits at two digits. The band
follows the rung (Interest at 0, the §7.3 ladder's colour above it), which is what
makes this row the **always-visible alert indicator** and is why the `[?]` no longer
needs to be one.

The security half is a **label**, not the raise's own phrase. *"Security condition 2
of 3"* is 24 cells and cannot share the row; the ceiling it drops is still stated
where it changes — the raise announces itself in full, and says why (§11.7) — and the
help panel carries the effects. What a standing row owes the player is the number,
every turn, without spending the row on it.

**Why the bare fraction is honest here, and where it would stop being.** §11.7 forbids
reporting the tally of consoles still out as if it were the requirement: a tally that
implies the wrong goal is the #310 bug. The row survives that rule because it is a
**progress tally over the loot**, labelled as one, and never a statement about the exit:

- quick play sets `IntelGate::All` (§12.6), where the tally and the requirement are the
  same number said differently and `3/3` *is* the exit-open signal;
- the campaign sets `IntelGate::AtLeastOne` (§14 v3/#574), where intel is currency (§2.2)
  and what the exit asks is **one thing, of either kind** — a number the fraction neither
  states nor contradicts, and which the level-start card and the Level info tab both name
  outright (the level-start card below, and the Level info tab's own objective section).
  A run at `0/3` is told at the mouth, free (§4.5).

**The condition it stays truthful under** is that no row may *promise an exit that will
refuse*. It held when the campaign's gate was `IntelGate::None` and it holds under the
minimum haul; what would break it is a gate whose requirement the fraction could be
mistaken for — so that is the thing to check before changing a mode's gate. Appendix 59
records the check this rule was last put through.

**Neither status row is ever blank.** The near line falls back to ambient status;
the usable line falls back to **how to move and how to wait** (#323), in the input
vocabulary the player is actually using — `swipe: move  tap: wait` on touch,
`↑↓←→: move  w: wait` on keys. One rule, two rows: permanent screen real estate is
never given away for nothing.

That row is where the innate verbs have to live. **Wait is the least discoverable
thing in the game and one of the most important** — the only 360° look (§9.1), the
way a crouch is held (§10.3), the way a cone is let past (§7.6) — and it
deliberately has no ability-bar entry, because the bar is the ability *economy*
(§8.3). Without this, the two verbs every run is built out of appear nowhere.

The fence that keeps it a hint rather than a control legend (appendix 25):

- **A floor, never a competitor.** The instant anything is adjacent, the affordances
  take the whole row back. No fade-out, no first-run-only flag.
- **Exactly the two verbs with no other home.** Takedown and Drag already appear here
  as real affordances; the full control set is the help panel's job.
- **Read-only, like the rows around it.** Nothing here is tappable.
- **Owned** (§11.2) — the same blue the ability bar's ready entries use, so the two
  surfaces answering *what can I do right now* answer in one colour.
- **The modality is the shell's only say**, corrected to whichever input was last
  actually used. The words and the layout stay in the core, inside the golden tests.
- **Both wordings are budget-checked at compile time** (#287): a reworded hint that
  would clip on the 40-wide board fails the build, not the frame.

**No ability column.** The old fixed 14-column list spent a seventh of the screen on
information consulted once a minute. Ability state (ready / active `[3]` / cooling
`/2/` / passive / unusable) must stay *discoverable*, and three experiments answered
where it should live before this one settled it (appendix 20).

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
what the ability actually does (#343). The **Help** tab — called *Legend* until the
abilities left it — keeps only the **standing** controls: move, wait, messages, help,
because **a reference card that changes with the loadout is not a reference card**
(#296).

**Fixed slots: the names never move.** Each ability owns a **10-cell slot** — 9 of
entry, 1 of air — and its entry is drawn **left-aligned inside it**, whatever state it
is in. A cooldown appearing, or ticking from `/10/` to `/9/`, changes nothing but its
own cells, because a bar whose words slide about is a bar you have to *read* rather
than **glance** at. An ability's column is a fact about the run, and since #359 it *is*
its key (§11.6) — position is muscle memory too. The slots are laid end to end and the
block is flush **right**, so a shorter loadout still keeps the bar under the thumb
(#267).

**The names fit because a run holds at most four abilities** — innate Run plus the
three-tech grant (§8.3/§10.2) — so the old strip's compression bought nothing worth its
cost. Four slots of 10 cells is exactly the 40-wide board, with nothing spare: hence
each ability's short **bar name** (`Run`, `Camo`, `Decoy`, `Phase`, `Doors`, `Daze`,
`Sight`), distinct from the full §8.3 name the help panel, the messages and the
level-seed token use. **The budget is checked at compile time** and every input is
derived, so renaming an ability, pushing a cooldown past 99, or granting a fourth tech
fails the *build*, not the frame (#287). The arithmetic is appendix 20.

The slot is also the **tap target**, all nine cells of it: a short name is no
harder to hit than a long one, and the target does not move when the ability's
state does (§11.6's touch rule). Touch is forgiven **one row of slack above and
below the bar** (#386) — the drawn slot is unchanged, and the slack rows are the
ones the router already answered with silence; see §11.6.

**A run opens on a card that says what the job is** (#497). Before the first turn, a
small block is laid over the middle of the map — the same overlay the verdict is
(§14 v2), so the facility reads above and below it and the fit never changes — carrying
exactly two sections: the **objective**, and the **modifiers** bending this run (§12.6).

```
┌──────────────────────────────────────┐
│                                      │
│ THE JOB                              │
│                                      │
│ OBJECTIVE                            │
│  Take all 3 intel                    │
│  Get back out through your tunnel    │
│                                      │
│ MODIFIERS                            │
│  Intel to exit: all of it            │
│                                      │
│ any key to begin                     │
│                                      │
└──────────────────────────────────────┘
```

**It is a box, walled on all four sides**, and that is a fix rather than decoration. Two
horizontal rules alone — the verdict's own bounding, which reads correctly on a *finished*
board — made the first frame of a run look **cut in half**: board above, board below, and
nothing at a glance to say the middle was one object laid on top rather than the facility
itself. Sides and corners say *dialog*. The walls cost the words nothing: every row of the
card is drawn from the panels' standing indents, and the modifier captions already leave
the last column free (the one-cell right margin every card keeps), so the frame lands on
cells no row was using.

It is a **reduced** Level info tab, and what it leaves out is as deliberate as what it
carries: no level-seed token and no copy control — those are the run's setup, read and
shared at leisure from a panel you can call up — and no facility alert, which is the one
section of that tab that moves while you play (§7.3) and has nothing to say before the
first turn. The modifier rows are the tab's own, from the one derivation both draw
(§12.6), so a rule in force on one is in force on the other. The **objective line is
derived from the run's own gate and its contents** — consoles and crates — and not from
the modifier list: §4.5's baseline gate surfaces no row there (§12.6), so a card built
from that list alone would leave a baseline run with no statement of what it is being
asked to do. It names the way out as well as the taking, because there is only one way
out and it is the tunnel you dug (§1/§4.5). Under the minimum haul it names the **crate**
too, where the facility has crates (#574): the gate counts them and nothing on the board
says so.

**The Level info tab carries the same two lines**, under its own `OBJECTIVE` heading and
from the same derivation (#574). It went without one while every human-facing gate was a
*departure* the modifier list surfaced on its own; the minimum haul put the campaign on
§4.5's baseline gate, where that list is silent — and a panel that exists to state a run's
rules cannot be the one surface that omits the rule which ends it. It is a section rather
than a modifier row because the two answer different questions: the list says what
*departs* from the baseline, and the gate a run is held to is a fact whether or not it
departs from anything.

**It dismisses on any input, and on nothing else — there is no timeout.** An auto-dismiss
has a losing race in it: press at roughly the moment it fires and the keypress meant as
*"I have read it"* lands in the game as your first move, possibly a step into a cone, in
a run with no undo (§2.1). Making the window smaller does not close it. So the card
waits; every key but a bare modifier dismisses it, every gesture dismisses it, and the
dismissing input is **consumed** rather than also reaching the world — which is the same
failure arriving by the other door. Like the help panel it is a pure view state: no world
change, no turn (§4.4), and escapable by construction (§11.6's no-trap rule), since there
is no input that does not dismiss it. The dismissal is not an `Input`, so it never enters
a replay's recorded stream and `(seed, [inputs])` stays about the game (§12.4).

It is raised for a **fresh facility** — quick play, a shared link, *retry*, a campaign
raid — and not for a **resumed** run (§12.5), which is already underway and past the turn
the card stands in front of.

### 11.5 Field of view and the danger overlay

Field of view controls *lighting*, not knowledge — what is **fogged** is settled
separately in §11.5a, and the two are independent. This section is about how live
visibility is drawn.

| Cell state | Rendering |
|---|---|
| In player's FOV | Full category colour |
| Outside player's FOV | Same glyph, dark gray — dim but legible. Two exceptions: **floor is not drawn at all** (the dot is the FOV's own ink — see below), and the exit keeps a dark Interest tint, since it anchors every escape plan (§7.6) and must not sink into wall gray |
| Watched by a guard, in player's FOV | **Red background** — the danger overlay |
| Watched by a guard the player **cannot see** | The straight sightline from that guard to the player is red — the **watcher line**, standing for as long as it has them. See below |
| Watched by a guard, outside player's FOV | Dark gray on dark gray — *unreadable* |
| A guard **sensed but not seen** (§9.2), any FOV | Its cell gets the orange **Sensed** background highlight regardless of line of sight; **no cone, no danger overlay** — position is known, attention is not. Where a *seen* guard's cone also watches the cell, the red danger overlay wins (being seen outranks) |
| The cells it was **just felt in** (§9.5) | The same orange at the quiet strength, fading out over `GUARD_CUE_DECAY_TURNS` — *was just here*, never a heading. It is the weakest cue on the board bar the effect wash: the watcher line and the danger overlay both paint over it |

The **live** dot and the **fading** trail behind it are the same channel at two
strengths, not two cues (§9.5): one rule stamps a mark where the sense felt something
and every mark fades, so a guard's trail, a guard's ghost after it leaves the box, and a
door's change all read as "sensed, and fading".

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

**The watcher line closes the overlay's one hole** (#222/#465). The overlay is built
from the cones of guards you can *see*; a guard watching from a room you cannot see
into paints nothing, so the board would say you are safe right up to the capture. So
a guard that (a) detects you right now, (b) is not confused (§8.3 — a dazed guard is
blind, so it has no cone to be honest about) and (c) is one you cannot see draws the straight sightline between it and you in red. It is
**standing**, not a flash: drawn on every turn it has you, gone the turn it loses
you. It means *"it can see you right now"* and never *"it is after you"* — a chaser
that has lost you draws nothing. A guard you *can* see draws none: its real cone
already paints the overlay, and the line must never double-draw one. A player
concealed from it (§10.3) draws none either, matching the overlay's own spare — red
under you means detected.

The cost should be stated rather than discovered: **while an unseen guard is looking
at you, you get its exact position, through walls, at any distance, for free.** That
is a deliberate exception to §9's bound on what may be known about a guard — the
sense range, the wait's widening, the duct's shrinking. §2.2/§2.3 buy it: you may not
be caught by something you could not perceive, and a guard with eyes on you is the
definition of something about to catch you. The exception is bounded by the same
condition it exists for, and expires with it.

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

**A mark earns its place by saying what the bar cannot** (§11.4/#341/#416). The ability
bar is a projection of state and already reports *the window is open*; a mark that lit
whenever an entry read `(on)` would carry nothing. So the player's own cell is marked for
**Camouflage**: its concealment holds only on the turns they do not move, so the mark and
the bar entry can **disagree** — `Camo[7]` while you walk across a lit corridor in plain
sight — and that disagreement *is* the mechanic. Marking a Run or an Autodoors would just
restate the bar. An ability joins this rule when its effect is likewise **conditional**,
not because it is an ability.

**Phase Out** is the second to join, on exactly that ground. A *running* Dephase is
unconditional and earns nothing; what is conditional is **where you are standing while
it runs**. The safety eject fires only if the window ends somewhere a solid body cannot
stand, so the player's cell is marked while — and only while — they are inside a solid:
nothing on open floor, lit the moment they step into a wall, dark again when they step
out, with the bar entry unchanged throughout. The bar says the clock is running; the
mark says *you are inside something while it does*, which is the half that changes what
the next turn is worth (§8.3/appendix 12 — the landing is random and the stun is as long
as the throw, so the risk cannot be planned around by eye). Being **entombed** — the
window ending with no legal landing cell anywhere — is a loss rather than an eject, and
the mark is lit there too: "you are inside a solid" is the truth in that case as well,
and it needs no special case.

**One firing may wear both**, and the two answer different questions. Confusion washes
the box it went off in and then rides the guards it froze; Lockdown washes the box it
sealed from and then holds the doorways themselves — *this far*, once, and *these ones*,
throughout. Neither substitutes for the other, which is why a mark is keyed by its
lifetime as well as its place: an ability may hold a momentary and a standing mark at
the same time, over the same kind of place, without one quietly replacing the other.

**The investigation area is the second advisory layer** (§7.6/#224), and it is switched
on by the `show_search_areas` level modifier (§12.6) rather than drawn every run. With it
on, the area every guard in a §7.6 search is sweeping washes **orange** (Warning): the
`SEARCH_RADIUS` box around that guard's `focus` — the literal set a hideout inside the
sweep is flushed from (§10.3/§15 Q5), so the picture is the rule's own geometry and cannot
drift from it. Orange means *a guard's attention is on this ground*; **that you are
detected stays red's word alone**, and where the two overlap red paints last and wins.

Two rules the area is stated with. **Every live search projects one, seen or not** — the
"never a guess" contract binds the *detection set*, and an advisory area gated on
perception would go dark exactly in the cupboard where it is worth most, since the whole
point of §7.6 is that you cannot see the guard combing the room. And it **clears when the
search does**, with no fade: unlike the §9.5 sense cue it makes no claim about the past,
and an area outliving its search would say a guard is combing ground it has already left.

Baseline — with the modifier off — a search is legible in **time** rather than in space:
the near line says when one opens and when it is called off (§11.7). That half is free
because it is the half a hidden player cannot get any other way; the spatial half is an
*easier* modifier because knowing which ground is being combed is a real advantage and has
to be paid for (§12.6).

**The precedence is fixed: Danger > a mark on a thing > Sensed > the investigation area >
the wash.** An advisory
layer can never masquerade as the detection set, nor hide it — the wash yields to the
sense channel, while a mark on a thing merely refines the cue that thing already draws.
The area sits above the wash and below the sense channel: a threat's attention outranks a
note about your own gadget, and neither may cover the cell a guard is actually standing
in. (Warning and Sensed are the same palette row, so that last comparison changes no
pixel — it is settled the sober way round because the free choice should go to the
stronger claim.)
Every mark carries the geometry the mechanic resolved against, by value, so the picture
cannot disagree with the rule, and it stays where it happened rather than following the
player, because that is what the effect did. A **refusal** marks nothing: a press that
changed nothing is a message (§11.7), not an effect.

Two rules the old version's failures leave behind (appendix 1): **a watched cell must
never render safer than an unwatched one**, and **floor renders as dots** so the FOV
boundary is visible across open ground at all.

The second is now served by drawing the dots **only inside the FOV** (appendix 33):
floor beyond your sight draws blank, so the boundary is a hard edge between dotted
ground and bare page rather than a step between two shades of dot. That is the same
rule pushed further, not withdrawn — a board with no dots anywhere is what it was
written against. Backgrounds are unaffected: they paint per cell whatever the glyph,
so a watched cell out of your sight still paints red, now with nothing on top of it.

**The one place the picture is allowed to diverge from the rule** is the §12.6 **ghost**
debug switch (#507, *appendix 57*), and it is written here so it is not discovered as a
bug. Under it the real detection set is empty, so the overlay would blank — exactly when
someone is debugging vision. It therefore keeps painting the set that *would* detect a
detectable player: red goes back to meaning *this cell is watched*. Nothing about the
**[SETTLED]** contract moves for anybody playing the game; a rule-bending instrument is
the only thing that can reach this, and it cannot be reached from a link, a token or the
sim.

> **What each background resolves to, row by row** — which cue means what, and the two
> guarantees that must not regress (the overlay covers watched cells outside your own
> FOV; cones of guards you cannot see paint nothing) — is
> [`docs/render-reference.md`](render-reference.md) §5. This section owns the rules; the
> reference records the board they produce.
### 11.5a Fog: the layout is visible, the contents are hidden

**[SETTLED]**

| Layer | Visibility |
|---|---|
| **Geometry** — the building's load-bearing fabric, the openings in it, and the floor space between | **Always visible, from turn one.** Never fogged. Drawn as the **schematic** until explored (below). |
| **Contents** — intel, hideouts, ducts, furniture, equipment, lore, and a door's *pose* | **Hidden until seen.** Once seen, remembered. |
| **Live state** — guards, bodies, door open/closed, danger cones | **Only what you can see right now.** Never remembered. **One exception: a guard's *position* is also known through walls within the guard-sense range (§9)** — but only its position, never its cone, and never remembered once out of range. |
| **What you placed** — your live decoy (§8.3) | **Always drawn, wherever it is.** In the FOV or out of it, for as long as it exists. |
| **The exit** — the tunnel you dug and came in by (§4.5) | **Always drawn as itself**, from turn one, never schematic. Yours. |

> **The schematic (#307, #470).** Geometry you have never had eyes on draws as the
> building's *plans*, not as it has been seen: the **fabric** (`□`) — wall runs and
> the recesses and openings cut into them — with the **floor space** between it left
> blank. Walking somewhere resolves it into the real thing, permanently. It is a
> **shape** distinction, not a darker shade, because geometry too dark to read is fog
> by another name. **The line is load-bearing structure**: `□` is what holds the
> building up, and everything that does not — a room's floor, the furniture standing
> in it, and a **doorway**, which shows as the gap in the wall line a plan would show —
> draws nothing, so the ways between an unexplored wing's rooms stay plannable. The
> plan is one mark and one absence, which is what it can afford to be now that floor
> out of your sight draws blank too (§11.5). Which mark each thing takes, why `□`
> replaced the `≈` this shipped with, and the denser alternative that was built and
> rejected, are [`docs/render-reference.md`](render-reference.md) §2.3–§2.4.
>
> **Floor is the one layer that is not drawn from memory.** Explored and unexplored
> floor are the same blank; the distinction lives entirely in the fabric channel, where
> explored geometry reads `#`/`×`/`}` and unexplored reads `□`, so a room you have
> cleared still reads as cleared by its shape (appendix 33).
>
> **Duct mouths and furniture are contents, not geometry** — a stated change from
> §10.7's earlier "visible from turn one like a door", on the grounds that a duct
> mouth is a recess backed by structure and a table is something put in a room rather
> than part of it (appendix 18). Room shapes, wall runs and the openings between them
> still read from turn one, so you are still never lost and never mapping.
>
> **"Remembered" is two questions, and the row above answers the first.** *Hidden
> until seen* is this table's axis. *Which ink it keeps once it leaves your sight* is
> the renderer's, and the two do not partition the same way:
> [`docs/render-reference.md`](render-reference.md) §3 owns that second answer and
> holds the layer split it implies. **Intel, comms, cupboards and duct mouths** take
> the **memory slate** — the ink that says *you found this*, and the reason #450 moved
> the duct mouth onto it: §10.7 makes a duct an escape a pursuer cannot follow, so a
> mouth scouted is a route you plan with, not a wall you have walked past. **Doors and
> furniture** take the ordinary dim shade instead, because a door's *pose* is live
> state redrawn every frame and a slate door would compete with it, and because doors
> are everywhere — slating all of them would bury the two or three marks that change a
> plan. A colour that marks everything marks nothing.
>
> **The cost is meant to be payable.** §12.6's layout knob (`layout_knowledge`) hands
> the whole layout over at its `Full` end as an *easier*-direction modifier — so under
> the directed difficulty draw it is bought with pressure taken on elsewhere, never
> given away.

> **The contents half has an override too, and it is bought rather than drawn** (#215).
> The campaign's pre-level **scout** (§14 v3) spends intel at the hub to put a facility's
> **points of interest** — its consoles, its crates and its cupboards — on the board from
> turn one, in the **remembered** state, so *hidden until seen* becomes *found, a raid
> early* for those cells. It is stated here for the reason the layout override is: a
> **[SETTLED]** rule that something quietly bends is not settled.
>
> Three things keep it from being a hole in the rule. It reveals **position only** — the
> live layer (guards, a door's pose, the cones) is never remembered even after it is seen,
> so there is nothing there for intel to buy, and the room around a scouted console is as
> unexplored as it ever was. It is **paid for, at a facility's whole haul** ([START] 3
> intel), not handed out by the §12.6 difficulty draw — it is deliberately kept out of the
> directed pool, because a draw that gave away the objectives would be giving away the
> thing the campaign's own currency exists to sell. And the **comms console is not in it**:
> §7.3's counterplay has to be found, and selling it would price that detour at three
> intel.
>
> It is the same mechanism as the memory the player earns, which is what makes it cheap to
> state: the scout marks those cells in tile memory at boot, and the renderer draws them
> exactly as it draws a room walked through and left. There is no third knowledge state.

> **The one deliberate override of this rule, and it is a modifier** (#233). The same
> knob's hard end, `layout_knowledge: None`, fogs the **geometry layer too**: ground the
> player has never had eyes on draws as nothing at all, and only what has been seen is
> on the board. The first row of the table above is then false for that run, on purpose.
>
> It is stated here rather than hidden in §12.6 because a **[SETTLED]** rule that
> something quietly bends is not settled. The bending is confined to one modifier, the
> base game keeps the visible layout, and a run that gives it up says so on its card
> (*"Layout unknown"*, harder).
>
> **What it costs is the pillar this section was written to protect.** Read the §7.6
> note two paragraphs down — *"a player who is chased and improvising in unknown
> geometry is not playing a stealth game, they're rolling dice"* — and then read it as
> a description of what this modifier turns on. That is the modifier's whole content:
> route-planning stops being free and becomes what exploring buys, and a chase through
> ground you have not scouted is exactly the dice-roll the doc warns of. It is therefore arguably
> **not a difficulty step** at all: a player who asked for *+1 harder* is handed a
> different game rather than a harder facility.
>
> **It is in the directed pool anyway since #518, and that reading is what it cost.** A
> modifier's job is to modify the level, and whether the ±N axis is the *tidiest* way to
> reach one is a weaker claim than the objection reads as — so this end was admitted with
> the consequence stated rather than argued away: a `+1` quick-play run can now be dealt
> a fogged-geometry facility the player never named, and the Level info card's *"Layout
> unknown"* is the only warning they get. It is the largest player-facing cost in that
> change (appendix 49), and the thing to watch is whether the card is enough or whether
> this end wants a louder cue on turn one.
>
> **The exit is the exception that keeps the run playable.** It draws as itself from
> turn one like always (row five), and with the building gone it is the one fixed point
> an escape plan can still be hung on. The **§9 guard sense** is likewise untouched: a
> sensed guard is still a bare position through walls, so the player gets a dot floating
> in blank space and does not know what stands between them and it. That reads oddly and
> is meant to — the sense was never line of sight, and special-casing it here would
> invent a second knowledge rule for one channel to paper over the first one's
> consequences.
>
> **The sim cannot yet weigh it.** The §13.2 bot is granted geometry unconditionally on
> this section's authority ([`bot-behaviour.md`](bot-behaviour.md) §2), so with the
> layout hidden it routes through walls it has never seen: a batch that names the
> modifier measures the bot, not the game (§13.3). Teaching the bot to explore unknown
> geometry is its own piece of work, and until it exists this modifier is judged by
> playing it.
>
> Why it is a knob's end rather than a second toggle, and why the pool cannot draw it:
> appendix 42.

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

> **A decoy you placed is yours to see (§8.3).** The fourth row is not a hole in the
> live-state rule but a different layer: a decoy is **the player's own placed
> object**, the same category of knowledge as their own cell. The whole point of a
> fake is to *walk away from it*, so a marker you can only see by standing next to it
> is a marker the ability cannot use (appendix 19).
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
| `Escape` | **Decline an open exchange** (§8.3) — and with no offer open, the help panel, exactly as `?` does (#551). Back out of a modal sub-screen: the seed prompt, the facility brief |
| `m` / `?` / `n` | Messages, help, colour theme — view toggles, never a turn (§11.2/§11.4) |
| `↑` `↓` / `Enter` | **On the help panel's Options tab only** — walk its rows and fire the marked one (§14 v2/#513). Free keys: no other tab had anything for them to do, so the settings claimed nothing from play |

**Digits bind by physical key, not by character.** The top row's `Digit1`–`Digit4`
and the numpad's `Numpad2` `4` `6` `8` `5` are resolved from `KeyboardEvent.code`,
so the binding is the key's *position* — an AZERTY top row is `& é " '`, and a
character binding would want Shift to fire an ability in the turn things go wrong. It
also settles which digits are which: the **numpad** moves and the **top row** fires
abilities.

**No character binding may name a digit** (#369). Three rules hold that split up: the
digits appear *only* in the code tables, the numpad folds onto the keys it duplicates
(`ArrowDown`, `w`) rather than onto `8` `2` `4` `6` `5`, and a **position outranks
every character table** when a press is resolved. Without all three a top-row `2` steps
south instead of firing slot 2 — spending the turn *and* moving you (appendix 21).

**Ability keys are the bar's slots** (#359). `1`–`4` fire the first through fourth
entries **of the bar as drawn** (§11.4) — the *drawn* row, which is flush right, so
`1` is the leftmost entry on screen and never a gap. Four keys, forever, because a run
holds at most four abilities (§8.3) while the catalogue keeps growing. A digit past the
run's held count does nothing: no turn, no state change. The cost is that a key is no
longer a fact about an *ability* — `c` was Camouflage in every run ever played, and `1`
is not — and what makes that payable is that the slots are visible, fixed for the whole
run, and the same thing your thumb taps (appendix 21).

**A second way in: the mnemonic letter** (#360). Beside the digit, each entry claims a
**letter**: *its own initial, unless another entry in the same run took it first* —
the whole rule, and the whole rule a player has to know. The bar **draws that one
letter in the ink colour**, lifted out of the name around it, in the entry it fires
(§11.4). `1` and `c` both fire `Camo`. Nothing is drawn behind it and nothing added
beside it, so the mark costs no width. **An entry you cannot use is not marked** — an
exhausted or unusable entry recedes whole (§11.2's Ground), because an ink letter says
*press this*. Its letter still resolves, and still refuses for free (§4.4). The digit
is the primary key: stable by position, and there whether or not a letter could be
claimed. An entry that could claim nothing keeps its digit alone; nothing is silently
reassigned. Letters resolve on the **character** (`key`), not the code — you press the
key labelled with the letter you can see.

**Only a held ability may push a mnemonic off its initial** (#368). A mnemonic still
may not shadow a movement or system key — a mis-key ends a run — but the reserved
tables give way rather than the letters, because **a skip whose cause is off screen is
unreadable**. So movement is the arrows and the numpad, and the vi keys `h` `j` `k` `l`
are gone with it. What is left reserved — `w` `.` `m` `n` `?` — starts no bar name in
the catalogue, and a test says so, so the day it starts to is a failed build rather
than a letter a player cannot account for. This is not the invisible derivation §11.6
designed out: the claim set is the run's four, the letter is drawn on the entry it
fires, and the digit is always underneath (appendix 21). What remains true and should
not be glossed: **the same ability can carry different letters in different runs.**

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
| Swipe ↑ / ↓ | Step north / south | Previous / next entry | Previous / next row, on the Options tab |
| Swipe ← / → | Step west / east | — (a vertical list) | Previous / next tab |
| Press | Wait | **nothing** | nothing |

One pump drives whichever surface is up, in the keyboard's own precedence — the options
help panel, then menu, then the board. **The panel leads**, which is the one place two
modal surfaces stack: the menu's `Options` entry raises it on the Options tab, so a press
that fell through would walk a list the player cannot see — so the thresholds, the repeat cadence and the
lift-stops-everything guarantee are written once and inherited, and the next screen
that wants touch costs a binding table rather than a pump.

**A press is deliberately unbound on every modal screen**, because resolving it to
*activate* would let a stray tap on empty menu space start a run by accident, which is
not undoable (appendix 21). An entry fires by pressing *the entry*, on the
arm-on-press / fire-on-lift path below, and by nothing else. The consequence worth
stating: a swipe must **begin where the controls decline**, since a press that lands
on an entry arms that entry and starts no gesture.

The two board-only rules below — the dead band and the no-auto-walk-into-danger
gate — do not follow the pump onto a modal screen: both ask questions about a board,
and a full-screen menu has none.

**Waiting is a tap on the board, well clear of the bars** (#306). A tap produces
Wait only on a **map** row that no overlay owns *and* at least a **dead band** away
from the chrome's inner edges — the two read-only status rows above, the ability bar
below, and the lower edge of the deployed message list while it is up. Inside the
band, on the chrome, and off the canvas, a tap that hits no control does **nothing**:
no turn, no state change. The boundary is *forgiving*, not merely correct, because in
a permadeath run with no undo a silently spent turn is unrecoverable (§2.1/§2.2).
**The fix is never to move a drawn target, and never to grow one into space that
answers**: §11.4's nine-cell slot, its fixed position and the air between slots are
settled, and a miss stays free.

**The ability bar is forgiven one row of slack above and below it** (#386). A press on
the map row directly above the drawn bar, or within one row's height below the frame's
bottom edge, resolves to the slot in that column exactly as a press on the bar does.
This amends the rule above and stands on the same principle: **nothing drawn changes**,
and the two rows it grows into are silent by construction (appendix 21). Forgiveness
may turn silence into a hit and may never take a live board tap away from the board,
and it never changes *which* ability fires. The cost is honest — a tap one row above
the bar was free and can now spend a turn — and it is the one thing to watch in play.
**[START]** — if it misfires by thumb, drop the upper row and keep the lower one rather
than widening further. The near line's `[?]` and message counter have the same one-row
problem and deliberately do not have this slack yet.

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
abandoned. That is the gesture pump's own fairness contract — *a turn must never burn
on a gesture the player didn't finish* (§2.2/§4.5) — applied to buttons.

### 11.7 Messages

Messages feed the **near line** (§11.4). The usable line is *not* part of this
system — it is derived from adjacency every frame and carries no state.

- Messages carry a **category**, a **priority**, and optionally a **source cell**.
- The near line shows only the **highest-priority** live message.
- **One event, one message — which may carry a subordinate line under it** (#418).
  An event whose headline cannot hold its *why* adds a second message that wears the
  headline's category and priority and is **spliced back directly underneath it after
  the ordering is done**, so it never sorts on its own. That coupling is the point:
  the turn an alert climbs is exactly a turn with other loud events, and a
  free-floating reason would be pulled away from its headline — below it by anything
  at an intermediate priority, or above it by the later-first tie-break. Nothing the
  turn does can come between a fact and its reason. The **near line never shows the
  subordinate line**: that row is one row and speaks the headline alone; the reason
  is what the player finds on deploying the log, live or remembered — which is where
  they go to ask *what just happened*. The alert raise is the first event to use the
  shape, and any later event with a *why* reuses it.
  - **A subordinate line is added only where nothing else on the turn says it.**
    Half the alert ladder's triggers fire on the *same turn* as the very event that
    reports them — a body found raises the ladder in the same event vector as
    `BodyFound`, a quiet post in the same one as `RadioSilence` — and there a reason
    is that message said twice, one row apart:

    ```
    security condition 3 of 3
    a guard found a body        ← the reason
    a body has been found       ← the event, the same turn
    ```

    So the rule is *explain what is otherwise unexplained*, not *always explain*.
    A trigger may stay silent **only** because a named sibling event speaks for it,
    which is a pairing the tests pin: if that event were ever silenced, the silence
    here fails rather than leaving an escalation unexplained anywhere.
  - **Why the facility climbed** (§7.3), exhaustive so a seventh trigger cannot ship
    without the question being answered either way: a confirmed sighting → *"you were
    seen"*; repeat sightings → *"seen three times now"*; a console tampered with →
    *"they know the intel was touched"* (the take message speaks the same turn, but
    about the intel, not about being noticed for it). A missed ping, a second post
    silent and a body found say **nothing** — `RadioSilence` and `BodyFound` already
    have. §11.8 applies to the lines that remain as to any player-facing string: they
    name the world, never the mechanism.
- **Messages clear on the player's next action** — a status line, not a
  scrollback — falling back to the ambient status of §11.4, never to an empty
  row. **[SETTLED]** for the **near line**: one row, one live message, wiped by
  the next action.
- **The deployed log behind the chevron keeps the last few actions** (#300). It is
  the screen's **full width**, like the near line it grows out of, and hangs from
  the row directly beneath it — **covering the usable line** rather than starting
  below it. That row lists what you could bump into *next*, which is the one thing
  you are provably not doing while you have the log open to read what already
  happened, so it is the cheapest row on the screen to spend; it is back the moment
  the log folds, and folding costs no turn either way (§4.4). The block never
  reaches the ability bar — the bar is always worth reading. It lists the current
  action's messages **the near line did not say** — its loudest is already the band
  an inch above, so the panel shows exactly what the counter promised and the two
  surfaces partition the turn instead of overlapping on it — then a **separator
  rule** in the System chrome colour, then the previous message-bearing action's
  block, and so on. **Every remembered block gets its rule, the first one
  included** — so when this action has nothing left to show, the block opens on a
  rule rather than on a past message pretending to be current. The rule's job is to
  say *what follows is not this turn*, and that claim is needed most at the top,
  directly under the near line's band. The block also **closes on a rule**, so it
  has a lower edge against the map: without one the oldest message just stops and
  the terrain resumes, leaving the eye to find the boundary. Rules top and bottom
  make the block read as one surface laid over the board rather than as text
  spilled onto it — and a frame too short to hold the whole block still gets its
  closing rule, on whatever row it truly ends at. That is where "with radio pings (§7.3) there is more to say" is answered:
  a silence, a call-in and a body find on three consecutive turns can be read back
  after the near line has moved on. Bounded twice — a cap on remembered actions
  (**5**) and a cap on total rows (**20**, half the v1 board), both **[START]** —
  and then clamped to the board, because it
  is drawn *over* the map and burying the §11.5 danger overlay is the failure mode.
  An action that said nothing contributes no block and no rule. **Now reads louder
  than then**: the current action's rows draw at full strength and every remembered
  row — and every rule — draws in its category's *dim* shade (§11.5's fog channel,
  reused as chrome), so a message keeps its §11.2 meaning and simply recedes. Still
  **no camera and no scrolling** (§11.4): if it feels short, move the bound, not the
  surface.
  The corner counter keeps counting **live** extras only — history never inflates
  it — and with nothing extra live it is the bare chevron.
- Modal messages anchor **near their source cell**, positioned so they never cover
  what they're talking about. That's a nice touch; keep it.
- **The loud rungs pop in** (#576). Some events are too important to be lost on the
  near line: the player's eye is on the board — on the guard two rooms over and on the
  cell they are about to step into — not on the top row of the screen, and a one-row
  band at the far edge of their attention is exactly what gets missed. The near line is
  the right home for *what is around you*; it is the wrong home for *the thing you came
  here for just happened* or *the building just got worse*. So the loud rungs get a
  **second surface**: a small **pop-in box** drawn over the board next to what it is
  about, bordered in the message's §11.2 category colour, which appears when the event
  fires, sits for about **2 s [START]**, and goes. Not a strobe and not a pulse — it
  appears, it is read, it disappears, with no animation in between. It is transient
  precisely so it can afford to be over the map.
  - **Which messages qualify is derived from the ladder, never hand-flagged.** The gate
    is a threshold on the priority every message already carries — the facility-
    escalation rung, **5 [START]**. Nothing gets a "loud" bit set at the raise site, so a
    future loud event inherits the pop-in for free and cannot be forgotten, and the
    ladder stays the single place importance is decided. That it derives from the ladder
    is **[SETTLED]**; the threshold is not.
    - **Where the line falls, and why there.** Below it a message is one guard's
      business — a look that found you, a radio gone quiet, a body found — and the
      *board already draws it*: the cone, the §9 sense mark, the watcher line. Those are
      facts the player reads by looking where they are already looking. At the rung and
      above, the message is the **facility's** or the **run's** — the §7.3 ladder
      climbing, a body reported, the phase safety firing, a capture evaded, an ending,
      an objective — and the board has no way to say any of it. The threshold started at
      20, objective feedback alone, and came down the first time the ladder was watched
      climbing in play: *the security condition just changed* is the fact that most
      changes what the next ten turns should be, and it was arriving on the row the
      player was not reading.
  - **The box takes its message; it does not copy it.** While a box is up, the message
    in it comes **off the near line and out of the live block of the deployed log**. The
    same words in two places an inch apart is one fact wearing two surfaces, and the row
    is better spent on the *next* thing the turn had to say — the box carrying *intel in
    hand* while the row says *a guard has seen you* is two facts on two surfaces. With
    nothing else live the row falls to the ambient floor, as it always does. Nothing is
    lost by it: the moment the box goes, the message is back in its expected place on
    both surfaces, and the history the log stacks under its rule is never touched.
    - **The block moves whole.** An event's subordinate line travels with its headline
      into the box, because §11.7 makes the pair inseparable — a reason left behind on
      the log while its headline is in the box is exactly the separation the rule
      forbids. The near line still never shows a subordinate: it is one row and speaks
      the headline alone. The box is not that row.
  - **It rides out its life.** A message dies on the player's next action; the box does
    **not**. A player who acts inside two seconds is the very case where the near line
    lost the message, so a box a quick key-press erases fails at its only job. It
    outlives the near line's copy and keeps drawing over the following turn — and it is
    not dismissible, for the same reason.
  - **It never queues.** A second qualifying message replaces the first and restarts the
    clock. One box at a time, always the newest fact; a backlog of stale boxes is the
    opposite of the point. It is not a second message log either — the deployed log
    already answers *what just happened*, and this surface answers *do not miss this*.
    The message the replaced box was holding returns to the near line and the log as it
    would have if the box had simply expired.
  - **It is anchored, not centred**, because the complaint being fixed is that the eye
    is on the board: a panel under the top rows would be missed for the same reason the
    near line is. Every message on the rung today is something the player did to a thing
    they are standing next to, or to themselves, so the anchor is the **player's cell**
    and the box is placed clear of that cell *and all eight of its neighbours* — the
    ring the console, crate or exit being reported is standing in, which is what makes
    "never covers what it's talking about" true for the whole rung without knowing which
    message is up. It is clamped to the board, never clipped; and among the legal
    placements it takes the **cheapest**: fewest *guards* covered, then fewest **§11.5
    danger cells**. Burying the lose condition is the failure mode every surface drawn
    over the map is bounded against, and a guard is its worst form — a guard's own cell is
    never inside its own cone, so counting cone cells alone will happily park the box on
    the `g` and leave the cone around it perfectly readable.
  - **The clock is the shell's, never the core's** (§12.1). The rules stay pure and
    turn-based: the core says what to draw and is told when the box has expired, so no
    wall clock reaches the rules and a replay of the same seed and inputs is identical
    whether or not a box was ever drawn.
- **Objective messaging derives from the gate, never from a fixed intel count**
  (§4.5/#310). Whether the exit is open is `exit_ready()`, and how much it still
  wants is `intel_needed_to_exit()` — which is *not* the tally of consoles still out
  (under `AtLeastOne` three can be out while one is needed). A message layer that is
  pure over its event must be *handed* that fact by the event; no take message may
  announce an exit that would refuse, and no refusal may misstate the requirement.
  **[SETTLED]**

- **A §7.6 search says when it opens and when it is called off** (#224), on the
  facility's behalf rather than each guard's. The whole hiding game is a bet on the
  search timer (§7.6), and a bet whose clock never visibly starts or stops is one the
  player cannot place: *has it given up yet?* is the question the bounded search exists
  to make askable, and watching a cone wander is not an answer — least of all from
  inside a cupboard, where the guard doing the searching is typically one you cannot
  see. So *a guard starts searching* fires when the level goes from nobody sweeping to
  somebody, and *the search is called off* when the last sweeper releases. **One
  announcement however many guards search**: a §7.7 call-in puts two or three on the
  same lead, and a per-guard line would both spend the one row three times and — far
  worse — announce that the search was called off while a second guard was still
  combing the ground you are hiding in. Neither line names a place; **where** is the
  investigation area's job (§11.5), and a cell here would name one searcher's focus as
  though it were the only one. The **bands differ, and the second one goes quiet** —
  Warning for the opening, Neutral for the calling-off — so relief reads as the row
  falling silent rather than as one threat colour swapped for another. Caution is the
  tempting answer and is wrong on this row: the ambient floor already paints the §7.3
  ladder's own colours, so a facility at condition 1 is *standing* in a Caution band,
  and a search is very often called off on exactly such a run.

Priority ladder **[START]**: routine self-narration ≤ 0; a search opening or being
called off sits on its own quiet rung at 1 — the consequence of something louder that
has usually already spoken, so it never takes the row a fresh detection wants; guard
threat escalates 2 → 4 → 10; objective feedback dominates at 20; ambient status sits
below everything (it is the floor, not a message).

### 11.8 Vocabulary — what the design calls things, what the screen calls them

**[SETTLED]** — **this document and the code name the mechanism; the screen names the
world.** They are allowed to disagree, and where they do, the pairing is written down
here.

The two vocabularies exist for different readers. *Rung* is exactly right in §7.3: it
says the alert is a monotone ladder with a fixed top and no way back down, which is the
property the code has to hold. It is exactly wrong on screen, because it describes the
shape of the implementation — a player reading "rung 2" is being asked to reverse-engineer
a system instead of being told about the building they are standing in. The facility's
own word for that state is a **condition**.

Two rules for a player-facing word:

- **It must not collide with a name already in play.** The candidate is checked against
  the §7.4 guard states and the §8.3 ability names. That is what ruled "hunting" and
  "lockdown" out as names for the top alert state: a guard reads Calm/Searching/Chasing,
  and *Lockdown* is an ability the player owns.
- **It must not lose information the design word carried.** "Condition 2 of 3" keeps the
  scale — how bad it is *and* how much worse it can get — which a bare mood word
  ("sweeping", "sealed") would throw away.

| Design / code | On screen | Where |
|---|---|---|
| alert **rung** (§7.3) | **condition** — *"security condition 2 of 3"*, `Condition 2 of 3` | near line, help panel |
| patrol **dwell** (§7.5) | **pause** — *"Guards never calm: pause 1–3 turns"* | help panel |
| `Hideout` (§10.3) | **cupboard** — *"cupboard — bump to hide"* | glyph legend, messages |
| `PartialCover` (§10.3) | **table** — *"table — bump to crouch"* | glyph legend, usable line |
| `DuctEntry` (§10.7) | **duct** — *"duct — bump to crawl in"*, `duct: enter` | glyph legend, usable line |
| `Exit` **from the facility side** (§4.5/#466) | **exit** — `exit: enter`, never `duct: enter`: one is a shortcut you found, the other is the way home | usable line |
| `Console` (§10.3) | **intel** — *"intel — bump to take"* | glyph legend, usable line |
| `CommsConsole` (§7.3) | **comms** — *"comms — bump to kill the radio"* | glyph legend, usable line |
| `Guide` (§8.3) | **Guide** — no row is needed for the mechanism, but one is for the *warning*: the screen never calls it a route, a path or a way, because it is a bearing and the words would promise a walk it does not know | help panel, ability bar |
| `FalseCall` (§8.3) | **False Call** — the bar's `Call` says what it *does*; the full name says the transmission is *forged*, which is the half that matters and the half a four-cell slot cannot carry | ability bar (as `Call`), help panel, near line |
| `Repel` (§8.3) | **Repel** — the mechanism and the screen say the same word, and the row is here for the *warning*: the screen never calls it a screen, a shield, a barrier or cover, because every one of those is a word for something you stand behind and this hides nothing (§7.3 still sees you through it). What it is allowed to say is **ground**: it *holds* a patch of floor, and what it holds it against is guards | help panel, ability bar, near line |
| `Dephase` (§8.3) | **Phase Out** — the bar's short `Phase` reads as its *opposite* beside *Dephase*, so the short name taught the wrong verb | ability bar (as `Phase`), help panel, near line |
| the **schematic** (§11.5a) | *"not yet seen"* — the building and the floor of it | glyph legend |
| the **run**, as the level-start card heads it (§11.4/#497) | **the job** — `THE JOB`. *Level* is the help panel's tab bar, *this run* that tab's own heading and *brief* the campaign map's facility brief (§14 v3), so all three collide; and unlike them this word is not meta — the card is read by the intruder standing in the building, not by the player choosing a run | level-start card |

**No row is the good case.** Where the design word is already the world's word — intel,
guard, body, door, exit, cupboard-as-terrain, Takedown, the ability names — there is
nothing to translate and nothing to record. Add a row only when a player-facing string
needs a noun this document spells differently.

**Meta vocabulary is deliberately untranslated.** *Seed*, *level modifier* and *loadout*
name the run's **setup**, not anything inside the facility: they belong to the player
choosing and sharing a run (§13.1), not to the intruder inside one, so `LEVEL SEED` and
`MODIFIERS` stay as they are on the Level info tab. Trying to make them diegetic would
be a fiction about the wrong thing.

The individual words in the table are wording and may be retuned; the **split** is the
settled part.

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

> The old version had none of this and paid for it: nothing was seeded, so "play
> again" needed a byte-for-byte snapshot of the entire level — a heavier, more
> fragile way to buy less (appendix 22).

### 12.5 Saves

The run is **snapshotted** to browser local storage — the whole [`State`], plus the
campaign layer above it (§12.7), the level-seed token, the run's framing and its input
recording — and read back to resume it. **[SETTLED]**: snapshot for saves, replay for
tests and bug reports (#514, appendix 50). `(seed, inputs)` is the smaller save and the
determinism (§12.4) is real, but a replay reproduces the run *the current build* would
play, so a save written before a tuning change resumes a different game without saying
so; a snapshot either restores exactly what was stored or fails to decode.

- **Autosave only. There is no save verb** — no key, no menu entry, no button. The run
  is written as it plays, and the title screen (§14) grows a **Continue run** row only
  when a valid save is there to resume.
- **Debounced, not per turn.** A turn arms a trailing write a few seconds out and each
  further turn re-arms it, so a held-key burst is one write; **[START]** 2 s, with a
  20-turn cap so the window is bounded in turns as well as seconds. Two things bypass
  the debounce: the page **hiding** (`visibilitychange`/`pagehide` — the reliable
  subset of the unload signals, and the moment a phone player is actually leaving), and
  a **terminal** event.
- **Permadeath shapes it** (§2.2). One slot, overwritten forward, emptied as part of
  resolving the turn that ends the run — so the slot never holds a pre-death state, and
  a save is *interruption resume*, never an undo and never a retry. Save-scumming is
  designed out rather than policed: the player cannot choose what is in the slot.
- **A save this build cannot read is simply no save** — discarded silently, and the
  menu shows the entries it always had. §12.6's "never a bricked page", applied to
  storage.
- The codec and the debounce clock live in the **shell** (§12.1); the core carries
  `serde` derives and nothing else.

### 12.6 Level modifiers

A **level modifier** is a named toggle or bounded knob that shifts a facility's
*baseline* — its difficulty or its rules — *before the level begins*. Each one
flips a rule an existing system already owns rather than adding a parallel one:

| Modifier | Bends | The rule it flips, and where |
|---|---|---|
| **Guards always search hideouts** | harder | the §7.6 search checks occupied cupboards unconditionally |
| **Always show vision cones** | easier | the §11.5 danger overlay paints in full |
| **Layout knob** (`layout_knowledge`) | both ends | how much of the building a run starts knowing, either side of §11.5a's schematic. *Full known* draws the real architecture from turn one — doorways, furniture and the duct mouths, the one *content* it hands over because a mouth reads off a plan like a doorway does (a scouted mouth still gets the memory slate, #450). *Unknown* draws nothing, fogging the geometry so route-planning is what exploring buys — #233, the **one modifier that overrides a [SETTLED] rule**, kept out of the directed pool for that reason until #518 admitted it (see §11.5a) |
| **The two cooperation call-ins** | harder | whether a lost sighting and a found body summon anyone (§7.7) |
| **All doors automatic** | harder | every doorway generates frameless instead of hinged (§10.4/#452 — **read by generation**, see the blockquote below) |
| **Guard count** (knob) | both ends | the §10.2 baseline, by a signed **delta** of up to two either way (#232/#565 — generation-time; contributions add, and two is one step per source that can name it, or the whole reach from one — the *Archive* names two, #217) |
| **Guards watch consoles** | harder | a Calm patrol prefers a cell beside a console its beat touches and cycles them, so the ground the player must reach is the ground that is patrolled (§7.5/#319; appendix 39) |
| **Search areas shown** | easier | the area every §7.6 search is sweeping paints in the Warning orange — the *when* is free and unconditional, only the *where* is priced (§11.5/#224) |
| **Locked room** | harder | key-gates every doorway of the room holding the facility's prize and puts a key on every guard (§10.4/#236 — the fifth generation-read entry, and the one reaching *past* placement: it draws nothing, so both settings are the same board and only one room's doorways differ) |
| **Guard cones: shorter** | easier | every guard's reach 10 → 6, the same ~90° wedge watched late, and §7.6's two zones shorten with it (§6.1/#495 — the first easier entry that bends a rule rather than handing over knowledge or slack) |
| **Guards watch their sides** | harder | the §6.2 flank carve withdrawn: a **Calm** patrol detects its two flank cells like every other mood, so the flank takedown and the tail through a corner are both off. The **harder arm** of the rule #442 settled the other way — a new field and a new slot, never a revival of the tombstone below — and **out of the directed pool**, because appendix 28 measured the unconditional carve as a real mover and a draw would re-open that call by lottery. Its one source is the archive (§14 v3/#217) |
| **Nothing felt through walls** | harder | the §9 sense is switched off, both channels: no guard felt through a wall (§9.1/§9.2), no door-change cue (§9.4). It takes away *scouting* information and never *fairness* information — seen guards, their cones, the §11.5 overlay and the #465 watcher line are all untouched, which is the whole shape of it. Read at the **perception** seam, so the board is the baseline's down to the cell. The **largest** harder step measured (14–34 points of bot win rate against §10.2's 8–10 for one guard), and the one whose magnitude most wants a human verdict — appendix 58 (#493) |
| **Calm guards detect only their cone** | easier | a Calm guard's two flank cells drop from detection while a hunting guard keeps its sides — shipped as an experiment (#410), then **adopted and retired in place** (#442, see below) |

This is the **mechanism** difficulty and mode rules flow through — the shared seam
#210 (alert scaling), #244 (quick play), and the v3 catalogue (#232–#236) all plug
into instead of each inventing its own knobs.

**One resolved value, read by many (§12.3).** All active modifiers are fields on a
single `LevelModifiers` value — plain, heterogeneous data (a toggle is a `bool`, a
knob a small enum or clamped integer). It is resolved **once at facility start**
and every system branches on *that* value — never a global bool queried in ten
places. Adding a modifier is adding a field, and the compiler then enumerates
every read site that must handle it. Each field carries a documented **direction**
(harder / easier) — **with one stated exception, the composite below**; §2.3's
anti-facade rule means every shipped modifier needs a **directional assertion** —
from the same seed and inputs, the harder one yields at least as much pressure as
baseline, the easier one reveals at least as much — so a flag that changes nothing
observable cannot pass for shipped.

**A composite modifier is one word for a combination** (#565). Some combinations have a
name the game already uses — the §14 v3 flavours are the first, and a difficulty preset,
a tutorial preset and a sim scenario are the same shape. A **composite** declares two
things and nothing else: its **name**, the word the player reads, and its **expansion**,
the primitive fields it sets. It is a *kind*, so a second one is a new entry rather than
new machinery. Three consequences, all load-bearing:

- **Expansion happens at resolution**, so nothing below `ModifierSources::resolve` learns
  the word *Vault*: the resolved value carries the extra guard, the extra console and the
  three crates as ordinary primitive fields, and every system branches on those exactly as
  it would if a primitive source had named them. What the composite's own value is kept
  for is the token and the help panel, and nothing else.
- **It occupies one wire slot, and the fields it stands for are then not encoded.** A Vault
  goes from three of `MODIFIER_CAP`'s five active slots to one, leaving four for the
  campaign's drawn rules (#210) — precisely on the facilities that could least afford the
  budget. What is scarce is *how many modifiers may be active at once*, never how many
  slots exist (token spec §3), which is why the fix is a composite and **not** a raised
  cap: the cap is what keeps the token's rejection rate honest.
- **It has no direction, and does not fake one.** A Vault is harder *and* richer — the
  whole of §14 v3's three-axis design. What stands in for the directional assertion is a
  stronger one: **equivalence**, asserted field by field against the combination the
  composite replaces, so *"nothing about any facility differs"* is proved rather than
  claimed. The direction is inherited by the parts, which keep their own — even where the
  facility inverts it: the *Archive*'s two extra consoles draw as an **easier** row because
  a console is loot everywhere else (§2.2), while at the terminus every one of them is
  required. A composite may not recolour its parts; the *Intel to exit* row beneath says
  what makes them hard.
- **A composite says what a facility is, never what the run is asked for.** No composite
  sets the intel gate: that is a mode knob (§4.5/#244), and a preset that moved it would be
  changing the run rather than the facility. The **archive** is where the line shows
  (#217): every rule of the terminus is in its composite, and its *mandatory* objective is
  set by the **node** — the end of the map, with nothing past it to spend a surplus in.
  Two facts about one word, kept in the two places that own them.

**A composite brings its own version of the modifiers, so they stack** — which is what
made the **count** knobs (guards, consoles) arithmetic. They are signed **deltas** on the
recipe's own number, and contributions **add**, clamped to the knob's reach: a Vault's
`+1 guard` and a condition's `+1 guard` are **two** guards, and the tab draws a row for
each. Not *reject the combination*, which would make the campaign's own stacking illegal;
not *last writer wins*, which is silent; and not *harder-ward*, which had the second source
land on a rung the first had taken and do nothing — §2.3's facade, on the source that
arrived second. The knob's reach is **two steps either way**, one per source that can name
it, and §10.2's guard envelope widened to **2–6** to make room (§10.2/appendix 55).

> **What arithmetic costs, stated rather than buried.** The older rule that *no
> contribution can relieve pressure another one asked for* does not survive a knob that is
> a count: an Outpost's `-1` and a drawn `+1` now sum to the recipe's own number instead of
> the drawn rule winning. That is the same trade in the other direction, and it is the
> honest one for a count — a facility told to hold one fewer guard and one more guard holds
> the number it started with, and both rows are on the tab so the player can do the sum. The
> invariant still holds everywhere it always meant something: the **intel gate**, every
> **toggle**, and the **layout** knob, none of which is a count and none of which composes
> by addition.

**The Level info tab lists one row per active rule, each with its owner** — `Vault: one
more guard`, and a rule the campaign drew on its own row beside it. A composite adds no
new row shape, only a name in front of a rule that is listed either way, so **no active
rule is hidden behind a label** and the tab's count of active rules stays honest. Every
source keeps its row, because with deltas every source's contribution is in the building:
no row here stands for a rule the facility does not have, and no rule of the facility's is
missing a row. The tab therefore grows **downward**, by exactly as many rows as there are
rules. The argument, and the two questions #565 had to answer, is *appendix 55*.

**Two surfaces list those rows, and they are one derivation** (#497): the Level info tab
you call up mid-raid, and the **level-start card** that opens a run (§11.4). Neither
hand-copies the other — both read `LevelModifiers::active`, in the same words and in the
same §11.2 direction colours — so a newly added modifier appears on both on its own, and
neither can list a rule the other omits.

> **Several modifiers reach generation, and they reach different depths of it**
> (§10.4/#452, §10.2/#232, §10.4/#236). Each is resolved **before** `generate_level`
> and threaded in as a parameter — never consulted from a global, or §12.4's
> determinism would be a claim nobody could check. The depths are graded and **pinned
> by a test**: the *doors* reach the carve and may move the building, the *locked
> room* may only re-role cells inside doorways the carve already cut, and every other
> entry leaves the grid byte-identical — so admitting a new generation-reaching entry
> is a deliberate act, not a discovery. A carve-reaching modifier **breaks seed
> stability** (every pre-#452 `#seed=N` link names a different facility now), which is
> a reason to prefer placement depth where the rule permits. The full argument — the
> three depths, the assertion each one buys, and why the break was taken rather than
> papered over — is appendix 52.

**The source → modifier → config flow.** The mechanism is shared; the *sources*
that switch modifiers on are separate and stack on top of it. Three, kept
deliberately distinct:

- **Choice** (exogenous) — the player's chosen or seeded baseline. A **mode** is a
  named preset = a bundle of modifiers over the base rules (#244).
- **Alert** (endogenous) — the campaign alert (#210): a loud raid alerts the ground
  ahead of it, and an alerted facility is dealt a harder rule from the same directed
  pool the difficulty axis draws from (§14 v3 has the mapping). This is where
  *"levels adapt to the strategy you lean on"* (§2) lives.
- **Flavour** (per-node) — a facility's own character on the campaign map (#207): an
  *Outpost* is thin and thinly guarded, a *Vault* rich and watched, a *Workshop* full of
  crates and thin on intel (#209). A flavour **is**
  the modifier set it contributes and nothing else, which is what stops the map's
  branches from being three differently-worded labels on the same facility (§2.3). Each
  is stated as one **composite** (#565), so what rides on the wire is the word and not
  the combination.

They compose into the *same* resolved `LevelModifiers` (`ModifierSources::resolve`):
a toggle is active if **any** source requests it; the **count** knobs add their deltas
(above); every other knob composes *harder-ward*.
**A contributing source starts from the neutral set, not from the default one**
[SETTLED] — the default is the *game's* baseline (an intel gate at `AtLeastOne`), and
composing harder-ward would let a source that never mentioned the exit tighten it. The
empty contribution is `LevelModifiers::neutral()`; only `chosen` speaks for the whole
run and starts from the default.
Adding a source is a new field and a line in `resolve`, never a new difficulty
path — #210 owns the alert→modifier *mapping* and its own fairness (decay, floor,
§2.2); this seam owns only the merge and the application.

**A difficulty is a directed draw over the pool** (#297). The *choice* source's
player-facing form is a **level from −2 to +2** over quick play: it resolves to a
concrete `LevelModifiers` by drawing `|level|` modifiers from the pool in the sign's
direction — harder for positive, easier for negative — and composing them onto the
quick-play base with `union`. Zero is exactly quick play, so the axis costs the
baseline nothing. The draw is a **pure function of `(level, seed)`** and runs *before*
the run boots, which is why the difficulty number needs no field in the level-seed
token: what travels is the **resolved set**, so a shared token hands over the run
rather than a recipe for re-rolling one. It takes a salted sub-stream of its own, so a
seed's **carve** is byte-identical at every difficulty — which is what makes the ±N
arms of a comparison the same building. The pool
is filtered on the fields' own documented **direction**, which makes §2.3's
directional assertion true by construction rather than by review; a level deeper than
its pool takes what exists rather than looping.

**What decides membership, and what does not** (#518). Two questions, and only two: is
the entry a **difficulty** change rather than a change of subject, and does it bend in a
**documented direction**? Whether the §13.2 bot can weigh it is deliberately *not* a
third. That criterion had crept in — three modifiers were being withheld from players
partly or wholly because the harness could not score them — and it inverts §13.4's
*"treat bot output as a smoke detector, not a judge"* and §13.1's *"you play and rule;
fun is a human judgement"*. A modifier's job is to modify the level; some are light and
sweep cleanly, others will only ever be judged by playing them, and the second kind is
not a lesser kind. Bot-blindness stays a **reporting** concern — a batch that draws a
modifier the policy cannot use measures the bot, and its numbers do not belong in a
balance argument — which is an argument for teaching the bot (#498, #517) and for
labelling the batch, never for a thinner game. It was never a small problem either:
three of the four *easier* entries are already bot-blind, so a `−N` bot batch has long
been drawing no-ops. The **suppressed sense** (#493) is the case that shows what the
criterion should have been all along: it is the first entry that bends what the *player
perceives* rather than what the world does, so the obvious guess is that the bot cannot
feel it — and the truth is the opposite, because the bot perceives through the player's
own channels by construction, and losing them costs it its keep-away, its flight and its
cover exactly as it costs a player theirs.

So the pool now holds **fourteen** entries, **nine harder and five easier**. The three
admitted by #518 are all harder, which is why the sides stopped being the same depth;
the short-sighted guards (#495) are the first step back the other way, and the
suppressed sense (#493) has since put the harder side back where it was. Nothing lies about the
gap (the slider's blurb counts *picks*, not pool depth), but a `+N` run still has more
variety than a `−N` one, and the easier side is still the one to grow.

**What kind of easier entry, not how many** (#495). Growing the easier side is not only
arithmetic: every entry on it before the short-sighted guards handed over **information**
(*"all vision cones shown"*, *"full layout known"*, *"search areas shown"*) or **slack**
(the guard count's easier end) — *knowledge or slack without touching the objective*,
the family appendix 29 named, and one with a ceiling. There are only so many things left
to reveal, and each one reveals a little more of the game the player is there to
discover. The short-sighted guards bend what a guard can **do** instead, so the easier side
now holds a rule-bending entry alongside its knowledge-and-slack ones — a facility with
four short-sighted guards is a different problem from one with three ordinary ones,
rather than another window onto the same run. That is the axis the easier side has left
to grow along (appendix 51).

> **What "the same building" now means, precisely** (#232). Every pool entry used to be
> read at runtime, so the ±N arms of a comparison were the same *level* down to the last
> radio clock. The guard-count knob is read at **placement**, so an arm that draws it
> gets the same carve and the same player, exit and intel — its guard set nested against
> the baseline's — but the pieces drawn after the guards (the comms console, the clocks)
> come off a shifted stream. The comparison is still between two runs of one building;
> it is no longer between two runs of one board. Said here rather than left to be
> discovered, because "byte-identical at every difficulty" was a stated property.

**Both sides are deeper than the axis reaches. [START]** With eight harder entries and
five easier ones, ±2 are both genuine draws that differ by seed. The easier side used to
be two deep and therefore exhaustive at −2 — the cost appendix 29 stated rather than hid
— and what closed it is the guard count's easier end (#232), exactly the "knowledge or
slack without touching the objective" that appendix said would; the shown search areas
(#224) are the third of that kind, and the short-sighted guards (#495) take −2 to a draw
of two-from-five.

Relaxing the **intel gate** would have been the other candidate, and it is still not
taken: the gate is a knob `union` composes harder-ward, and quick play already sits at
its hard end, so an easier draw could only relax it by learning to **replace** a knob
rather than compose with it — which would also mean the difficulty slider could change
quick play's objective. The guard count is a knob too and is drawn all the same,
because its baseline is a **neutral middle** rather than an end the base has already
walked to: composing an easier pick onto a base that asked for nothing leaves the pick
standing, so no replacement rule is needed. That distinction — where a knob's baseline
sits — is what decides whether the pool can reach it (appendix 31).

**A knob may also join the pool at one end only** (#233). The **layout knob** has a
neutral middle like the guard count, and its easier end (*"full layout known"*) is a pool
entry — but its harder end (*"layout unknown"*) is not, and the reason is neither of the
two above. It is read by the renderer on the same board, so nothing mechanical objects;
it is simply **not a difficulty step**. The axis promises the same game under more or
less pressure, and hiding the layout removes §11.5a's route-planning and with it the
§7.6 pillar the visible layout exists to support — a player who asked for *+1* would be
handed an unfamiliar mode rather than a harder facility. Keeping it out also keeps a
difficulty draw from putting a sim batch in a position the §13.2 bot cannot honestly play
(§11.5a). So the third question the pool asks of a candidate, after *can it be composed*
and *where is its baseline*, is **is this pressure or is it a different game** (appendix
42).

**The slider is a pre-run dialog** (#298). Quick play opens a **level options** screen
before it boots: the five stops, a *Play* and a *Back*, drawn in the character grid
(§11.1) and driveable by key and by finger alike (§11.6's no-trap rule — the exact
failure the old options dialog shipped). It **names** the difficulty and does not
preview which rules the draw will pick: the seed is not rolled until *Play*, and the
resolved set is the help panel's Level info tab to show once there is a run to
describe. It is deliberately **not** the menu's `Options` entry, which is §14 v2's
**global settings**, the panel's Options tab (#513, appendix 53) — the colour theme, the renderer, and behind the §12.6
session gate the debug switches. One screen asks about the *run you are starting*, the
other about *the game*; that boundary is why they are two screens and not one.

**The debug switches live on that tab** (#459 → #513, appendix 53), under their own
heading and in their own colour, after the widest gap on it — and they are drawn
only in a debug session, so with no session there is no heading and no row to reach by
key or by tap. The **rows** themselves read like every other row: they are live
controls, and dimming them would say *inert* about the section where a press does the
most, so the gate is the heading's job alone. They are **never persisted**: the preference record beside them
holds the theme and the renderer and nothing else, because a record that re-armed a
switch on the next visit would outlive the session gate this whole channel rests on —
and since #507 one of them bends a rule, so it would be re-arming that against the
facility rather than against the picture. Everything else about the gate is unchanged —
never in a level-seed token, activation stripped from the URL. Only the surface moved.
The **ghost** row (#507) is drawn under the same gate and reads like its neighbours;
what it adds to this screen is that the row it disables — the replay export — reads as
*unavailable* and answers a press by naming the switch, rather than going quietly dead.

**A modifier is also how an experiment ships, and how one is adopted.**
`calm_guards_detect_only_their_cone` (#410) bent a **[SETTLED]** sentence —
§6.1/§6.2/§7.2's *"you can never stand beside or in front of a guard undetected"*
— so it shipped as a knob to be measured rather than as a rule. Both arms of a
paired A/B then ran from **one build** on identical seeds, which is the only way
the comparison is exact; and because placement is pinned to the conservative rear
carve, the two arms generate the *same facility*, so nothing in the diff was
geometry. Appendix 28 records what it measured.

**It was then adopted (#442)**, which is a design-doc edit rather than a merge:
the three sections above moved together, and the knob was **retired in place**.
That last part is the format rule, not tidiness — a modifier's position is a
permanent slot the level-seed token encodes *by index*, so a retired one leaves a
tombstone that still round-trips and is never read, never drawn and never
captioned. Deleting it and closing the gap would silently re-point every token
ever shared (§12.5, the #286 break). Restoring a retired rule as a *harder*
modifier is a **new** slot appended to the end, never a revival of the old one.

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

**Sharing is the URL** (#572) **[SETTLED]**. A run is handed over as a
`…#seed=<token>` **link** and by no other route: the Level info tab's `copy [c]` puts
one on the clipboard, the address bar holds one from a run's first frame, and the
recipient clicks it. There is nowhere to type a token — the title screen's *Seed play*
entry, the sub-screen behind it and its DOM text box are all retired — which is what
makes the second half of the rule true: **the whole game is the character grid**
(§11.1), with no exception outside the debug session.

The **token stays displayed** on the panel, and that split is the decision rather than
an inconsistency. Eighteen letters is what a 40-column grid can print and a human can
read back off a screen; a URL is what the person on the other end can actually open.
So the *display* is the short form and the *copy* is the link, and when a link arrives
mangled — a chat client eating the fragment, a line wrap through a paste — reading the
token off the screen and rebuilding the URL by hand is the recovery path. That is also
why a clipboard that refuses says so instead of claiming a copy: the token is still
printed one row above.

A shared link is **the page's origin and path with a fresh fragment, and nothing
else**. Neither a `?debug=` activation nor the `&inputs=` of a replay the sharer
happens to be watching rides along — one strip, in one place, rather than a rule each
copy control remembers separately. And where the sim offers a run to a *person* rather
than to the playtest skill's parser, it offers a link too.

**A number is not a token** (#333, superseding #328). A bare `?seed=8371` names *this
build's quick-play preset applied to 8371* — not a run — so a shared link silently
re-resolves whenever the preset moves, and it did ([token spec](level-seed-token.md)
§7). The bare form is gone as an **input** too: "try seed 8371" no longer works, and
pre-#333 links stop decoding. Numeric seeds remain a *programmatic* concept —
`LevelSeed::sim(n)` and §13.2's sweeps never touch the string.

**The format is sized for the roster it does not have yet.** Abilities and modifiers
are carried as combination indexes over **256 permanent slots**, not over the entries
that exist today, so the roster can grow to a hundred entries without a single shared
link breaking. It buys a discipline in exchange: **slot numbers are permanent**, a
retired entry leaves a tombstone, and nothing may ever be renumbered ([token
spec](level-seed-token.md) §3).

**[START]** on the sizing: eighteen characters, a 17-bit seed (131,072 facilities), and
the ~1-in-3,000 rejection that the leftover space provides. Seed space and integrity
trade one-for-one ([token spec](level-seed-token.md) §8).

**Debug modifiers are not level modifiers.** A separate `DebugModifiers` value carries
playtest-only **instruments**. It is **never encoded into a level-seed string**, so no
shared level can arrive with one on, and no generation seam sees it. There are two:

**Omni-vision** — *"see the whole level"*, which makes the player's §6 field of view the
entire facility, so a build can be watched rather than played blind. It is stated as
*sight* and applied in the sight phase, not as a drawing rule, so everything downstream
follows without a special case: the §11.5a fog lifts into the ordinary live picture,
every guard reads as seen, and the §11.5 danger overlay paints every cone. It touches
nothing else — guards look with their own cones and walk the same beats, so the run
plays identically (seeing everything is not being everywhere).

**Ghost** (#507, *appendix 57*) — while it is on, **no guard ever detects the player**:
cones pass through you, sightings never fire, chases never start. It is one clause on
the §10.3 concealment path Camouflage already goes through — *camouflage that never
lapses* — so the guard sense pass, the §7.6 transitions and the §7.3 rung-1 trigger all
follow from the one seam rather than from a bend sprinkled through guard AI. It shares
Camouflage's consequences, the §7.2 front takedown included. **Contact still captures**
(§4.5): a guard that walks into your cell ends the run whether or not it ever saw you
coming, which is §8.3's own sentence about this state — *invisible is not safe*. Where
omni lets you watch a level you cannot see, this lets you **stand in one**: walk to the
corner where generation went wrong, park in a guard's face to read its cone maths,
follow a patrol for twenty turns, without the facility ending the run before you get
there. What it cannot show you is the threat model — no chase, search, §7.7 call-in or
rung escalation can be reproduced through it.

**Ghost is the exception this section is now stated with.** The rule used to be
absolute — *a level modifier changes the game, a debug modifier changes only what you
get to see of it* — and this switch breaks it: guards behave differently and the run's
outcome changes. The distinction is kept rather than deleted, because it is what decides
what a switch costs. A **perception** switch costs nothing: it can be flipped mid-run,
watched under, and exported from, because the run underneath it is unchanged. A
**rule-bending** switch costs the run's reproducibility, and is admitted only with all
four of these:

- **Never in the token.** A level-seed token copied from a ghost run boots an ordinary
  run. The token stays honest about the *facility*; what it stops being is a full
  account of what happened. Stated plainly rather than buried: **a ghost run is not
  reproducible from its token**, and that is the accepted price of not putting it in
  there.
- **Never in a replay.** Once the switch has been on, the run cannot be exported at all
  — the control goes inert and says why. It latches on the **run**, not on the switch:
  turning ghost back off does not restore the export, because the inputs already
  recorded were played under bent rules and no later toggle un-bends them. The
  alternative — teaching the replay link to carry the debug flags — would put a
  rule-bend inside a shareable link, which is the exact thing this section keeps out of
  the token; the containment is worth more than the export.
- **Never in the sim.** `crates/sim` cannot set it, so no §13.2 measurement can be
  taken through it.
- **Visibly on**, on the Options tab's own row, read live off the run.

The unchanged half of the rule is the load-bearing one: **anything that bends a rule and
is meant to be *played* is a level modifier and belongs in the token** with the rest of
the run's identity. Ghost is an instrument, not a way to play. If it ever wants to be
playable — an easier-direction *"unseen"* modifier — that is a different thing with a
token slot, a Level info caption, a §2.3 directional assertion and a place in the
difficulty pool, never this field wearing a second hat.

**The danger overlay keeps painting under a ghost, on purpose.** §11.5 is **[SETTLED]**
that the overlay is the *literal* detection set; under ghost that set is **empty**, so a
literal reading would blank the board exactly when someone is debugging vision — the
likeliest reason to have flipped the switch at all. So it carries on painting the set
that *would* detect a detectable player: red stops meaning *you are detected* and goes
back to meaning *this cell is watched*. That is a lie by §11.5's standard and it is the
right one here, because the alternative is an instrument that goes blank when you use
it.

**Debug mode ships hidden in every build** (#459, *appendix 35*). The switches used to
be reachable only by rebuilding, which meant a run that misbehaved on the deployed page
could not be looked at at all. So there is a **debug session**: the help panel carries
the switches — omni-vision and the ghost, both flippable mid-run — and the replay
export, and it is present when the build stamped it (every artifact preview) or
when the page was opened with `?debug=intruded`. The parameter is a **shibboleth**, not
a documented switch, and it is **stripped from the URL the moment it is consumed**, so
the address bar goes straight back to the shareable `#seed=` link: activation is a
thing you *do*, never a thing a link carries. That the gate is a convention rather than
a mechanism — anyone reading the shipped wasm can find the string — is exactly why the
rule above is load-bearing. It used to read *nothing behind it may ever touch the
facility, only the picture*; since #507 what carries that weight is the containment
above, and the promise the gate makes is the one that survives being guessed: **nothing
behind it can produce a run that passes for a real one.**

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
resolved before generation (**all doors automatic** #452 — the first one shipped —
plus guard count #232, safe zones #235, locked doors #236) read the same value at the
generation seam; runtime modifiers (the intel gate #244, the two §7.7 cooperation
call-ins, and the rest) read it off the running state. Same value, two horizons.

The **generation** horizon is the one with teeth, and #452 is where that got paid
for. A modifier read before the carve is a *hidden input to the generator*, so it has
to be threaded in as a parameter rather than reached for — and two consequences
follow that a runtime modifier never has: the same seed under the two settings is two
different facilities, so §2.3's directional assertion cannot be a same-scene
comparison and becomes distributional instead; and adding one **shifts the RNG
stream**, so every seed shared before it names a different building. Both are worth
paying, but they are the price of this horizon and should be stated when a new
modifier asks to sit on it.

### 12.7 The campaign layer — the run above the level

Everything above describes **one facility**. The run is a sequence of them (§2.2/§14
v3), so one layer sits above the turn loop: a **campaign** owning the facilities a run
will raid, where it stands in them, and what it carries between them. The level below
it is untouched — a campaign facility is an ordinary level with ordinary modifiers, and
a campaign of **one** facility is exactly the game v1 ships.

- **The sequence is forward-only.** You traverse toward the archive; there is no
  backtracking and no retry (§2.2). Its geography is the **facility map** (§14 v3): a
  graph with real edges, grown lazily, offering a choice at each node.
- **Each facility's seed is derived from `(run seed, node id)`** — never a fresh
  source (§12.4). A whole run therefore reproduces from `(run seed, [inputs])` exactly
  as one level does, which is what makes bug repro and golden tests possible across a
  2–3 hour run. It is derived **narrowed to the level-seed token's seed field**, so
  every facility of a campaign is also a *sayable level*: a link you can hand to
  someone, or the token inside it to the sim's `--config`, and play on its own (§13.1).
- **Three things carry between facilities, and nothing carries out of the run**
  (§2.2's table): the **salvaged-tech loadout** (§8.3), the **intel wallet** (#211) —
  intel is currency in the campaign and never a fee at the exit, which takes nothing
  (§4.5); the gate is the minimum haul (`IntelGate::AtLeastOne`, #574), a check that the
  raid happened rather than a debit, and the balance is banked at every completed raid and
  spent only at the map between facilities (§14 v3, appendices 47 and 59) — and the
  **campaign alert**, the run-level layer
  above the per-facility §7.3 ladder. A campaign is persisted in exactly one place and
  for exactly one purpose — the run's autosave slot (§12.5/#514), so an interrupted
  campaign can be resumed where it stopped — and that slot is emptied when the run
  ends, so "nothing carries across runs" is a property of the save's lifetime and still
  never a rule anything has to enforce.
- **The transitions are the whole layer:** *enter* a facility (the campaign hands out
  its level-seed), *complete* one (the haul is banked, the facility is dropped —
  geometry, guards and bodies do not persist, and the run arrives at a **choice
  point**), *choose* the next facility from what the map offers (§14 v3), *capture*
  (terminal for the run, anywhere, §2.2), and *leave the archive* — which its own exit
  permits only with everything it holds (#217) — and the run is **won**.
- **A campaign facility is an ordinary level with one extra source of modifiers**: the
  node's flavour (§12.6). Nothing below this layer knows the difference — which is why
  a campaign facility's level-seed token is a level anyone can play on its own.

**[START]** on the campaign's length, now stated as the map's **depth to the archive**
(six, so a run raids seven facilities) — the coarsest knob on the 2–3 hour target.

**The campaign alert is the condition the last completed raid ended at** (#210), and it
reaches exactly one hop: the facilities the map is about to offer, through the §12.6
modifier seam and nothing else. The rule it switches on is drawn from the **directed
pool** per node, so its direction is guaranteed by construction. See §14 v3 for the
mapping; three properties are the layer's own:

- **It is replaced, never added to.** A quiet raid returns it to zero however loud the
  one before it was, so §2.2's "escalation must stay recoverable" is a property of the
  type rather than a decay rate tuned to hope so — and there is no floor to state,
  because the floor is zero and every raid can reach it.
- **Determinism holds across the whole trajectory** (§12.4). Both draws — which facility
  the noise settles on, and which rule that facility is dealt — hang off the run seed
  and the node identity on their own salted sub-streams, so a run reproduces from
  `(run seed, [inputs])` with the same alert trajectory and the same scaled facilities.
  A facility's own seed is untouched by it: the loud and the quiet arms of a run walk
  into the *same building*, playing different rules.
- **A campaign facility stays a sayable level.** The contribution resolves into the
  facility's `LevelSeed` before it boots, so an alerted facility's token is the facility
  as it was actually played — like every other source (§12.7's opening rule holds:
  nothing below this layer knows there is a campaign).

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
| Alert peak, **and the path to it** | Whether escalation escalates — the rung reached, the turn it was reached, and which §7.3 trigger got it there. A run driven to rung 3 by bodies is a different game from one driven there by being seen, and both peak at 3, so the peak alone answers half the question (#376) |
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
  ever explained what `$`, `E`, `}` or `z` meant). The **Options tab** (#513) is the
  settings home: theme (#189) and renderer (#460), persisted in their own record beside
  the autosave's, plus the §12.6 debug switches in a gated section. It is the panel's
  fourth tab, and the fourth cost a label — *Abilities* is drawn **Actions**, because
  four `[Label]`s and the `[x]` are all a 40-column row holds
- A game-over screen that says **why you lost** (the old one didn't distinguish
  victory from defeat at all) — the capturing guard, the mood it made contact in and
  the cell it happened on, latched from the terminal event; a distinct win screen with
  the run's ledger; the seed on both; and the ways on, **gated by the run's mode**
  (§2.2, appendix 31)
- An alert indicator

### v3 — the campaign

**The campaign is the run** (§2.2): 2–3 hours, progression throughout, nothing
carried to the next one. The **layer** it all hangs off — the sequence of facilities,
the per-facility seed derivation, and what carries between them — is §12.7; each
bullet below fills one of its seams.

- The facility map. **A graph with real edges** — the old "map" was a flat list
  with no adjacency and no geography, where every unlocked facility was always
  selectable. Geography should mean something. The model (#207):
  - **A lattice of lanes, grown lazily** [SETTLED]. A node is a `(depth, lane)` pair;
    everything about it — its successors, its flavour, its drawn position — is derived
    on demand from `(run seed, node id)` and from nothing else (§12.4). Nothing is
    pre-generated, so there is no world-build and no fog, and the graph is still a
    function of the seed. Identity is a coordinate rather than a serial number, which
    is what lets a node *name* its successors before anything has built them.
  - **An open edge reaches only an adjacent lane** [SETTLED]. That is the whole of
    "geography means something": where you stand decides what is in front of you, and
    crossing the country is a sequence of choices rather than a selection from a list.
    A run against the edge of the map is offered fewer options, and that is the rule
    biting rather than a shortfall.
  - **2–3 open successors at each choice point** [START], **five lanes** [START],
    **depth six to the archive** [START] — the last being the coarsest knob on the 2–3
    hour target (§12.7).
  - **Flavours are visible when offered** [SETTLED] — no fog, because the choice *is*
    the mechanic and a choice made blind is a coin flip. The starting set [START] is
    *Outpost* (one guard and one console fewer), *Depot* (the §10.2 recipe untouched),
    *Vault* (one more of each, and **three** equipment caches), *Workshop* (**two**
    caches, and one console fewer to pay for them — #209) and the *Archive* (the terminus,
    and the one flavour that is an ending rather than a position on those axes — #217,
    below). A Depot
    hides **one** crate and an Outpost none, so the crate count rises with the richness
    of the facility. Three axes now, risk and the two rewards, so no option is simply the
    right answer; a flavour reaches the facility through the §12.6 flavour source, so what
    the map said and what the building is are one statement.
  - **No two open successors ever share a flavour** [SETTLED]. The flavours are a fixed
    cycle laid across the lanes and rotated per depth, so this is guaranteed by
    construction rather than by a rejection loop — a branch whose options generate the
    same facility is the flat list wearing a costume.
  - **One further successor is intel-locked** [SETTLED] — the alternative-route sink
    below, now priced and spendable (#212). It reaches a lane *two* across, which no open
    edge can, so what intel buys is **ground**: a part of the map that was not on offer,
    rather than a better facility handed over. What stands on that ground is whatever the
    seed put there.
  - **Every route converges on the archive** [SETTLED], the one node with no
    successors. Reaching it ends traversal.
  - **The archive is the ending, and it is one composite** (#217/#565). What the terminus
    *is* rides in `Composite::Archive` like every other flavour, and it expands to: **two
    more guards** and **two more consoles** (the count knobs' whole reach from one source
    — six and five on the §10.2 recipe), **one locked room** behind a key every guard
    carries (§10.4/#236 — no crates here, so it falls on a *console* room), **guards that
    watch their sides in every mood** (the §6.2 flank carve withdrawn, so the
    tail-through-a-corner and the flank takedown the rest of the campaign teaches are both
    off), and **no equipment caches**, because salvage on the last facility is a power
    curve rising after the last thing it could be spent on. It reaches placement and the
    lock's pass, never the carve — so the terminus is the same building its seed always
    carved, with more in it and one room shut.
  - **The archive is the campaign's one mandatory *complete* objective** [SETTLED], and
    the single exception to §4.5's minimum haul (#211/#217/#574). Everywhere else one
    thing is enough because intel is currency and the exit takes none of it; here it
    wants every console. **That gate is the
    node's, not the composite's**: a composite says what a *facility* is, and a gate says
    what the **run** is asked for — the end of this map, with nothing past it to spend a
    surplus in. Because one console is behind the lock and all five are required, a won
    run is one that committed a **takedown** (§7.2 — the protagonist never kills, so the
    price is a body to hide and a §7.3 radio clock to outrun). Leaving with the data is
    the **run won** (§2.2); capture there is terminal like capture anywhere, with no
    special case.
  - **The map screen is the campaign's surface** (#208) — the title screen's *Story
    mode* opens it, and it is where every raid is chosen and started. It draws in the
    same character grid as everything else (§11.1), as **a picture and a list**: the
    picture says *where* — nodes at their positions, the road walked behind you, the
    fan of offers, the archive at the head of the screen from the first frame — and the
    list says *which*, one marked row per facility you may walk into, reachable by key
    and by finger alike (§11.6). Splitting them is what fits the choice on a 40-column
    board: a map carrying its own captions has no room for them, and a list on its own
    is the flat list again.
  - **The map shows shape, not contents** [SETTLED]. A facility the run has not stood
    on and is not being offered draws as an outline: no fog on the geography (you can
    always see how far the archive is), and no flavour handed over a hop early. It is
    §11.5a's rule one scale up, and it leaves the scouting sinks (#215) something to
    sell.
  - **A completed facility does not raise the end screen.** The map comes up instead,
    with the haul banked — a "you won" card between every raid would be seven endings
    in a game that has one. The end screen keeps the endings that are endings: capture,
    and the archive left behind. The won run draws the **ordinary** end screen (§14 v2),
    which already tells a win from a loss, with the campaign's single exit: back to the
    title, because a run you can replay is not a permadeath run (appendix 31). A victory
    screen that says more than *ESCAPED* is later work, not the ending's.
- **Salvaged tech accumulating across facilities.** This is the run's power curve
  and it is the reason the campaign exists. It was fully built last time and
  reachable by nobody: no facility was ever generated with an equipment cache, so
  no ability could ever be unlocked. The progression axis existed only on paper.
  - **How many crates a facility hides is its flavour's to say** (#209) — Outpost
    **0**, Depot **1**, Workshop **2**, Vault **3** **[START]**. So the tech axis is
    something you *route toward* on the map, and the two rich flavours differ in **what
    they charge** rather than only in how much they give: a Vault pays out in crates
    *and* intel and charges a **guard**, a Workshop pays out in crates alone and charges
    a **console**. The choice between them is *which currency you are short of* — tech
    the run keeps against intel the run spends — rather than which one is better.
  - **Within a facility every crate is different; across a run they may repeat.** The
    stock is drawn from the facility's own seed, so a building is stocked before anyone
    breaks into it, and meeting tech you already carry is **bad luck** rather than
    something the world rearranges itself to prevent. Appendix 40.
  - **The §8.3 held cap is kept at the crate, and it is a choice rather than a wall**
    (#266). A run carries three pieces of tech; a bump with no room for a fourth opens
    the **exchange** — drop one of your three for the crate's, or decline and leave it
    standing. The trade spends the turn a salvage would have and the decline is free; a
    bump on a **duplicate** is still the one refusal, because there is no decision in a
    second copy — except where that tech has a spent per-level budget (§8.2), which the
    duplicate refills. This is what makes the tech axis a shape rather than a queue: the fourth
    crate of a run is where you start saying what this run *is*. Appendix 44.
  - **Found tech is usable the turn it is found**, in the facility it was found in.
    A reward that only switched on after extraction would make the detour a deposit
    toward a later raid rather than a tool for this one.
- **Intel as a real currency, with actual sinks**: reveal facility intel, unlock an
  alternative route, lower the alert, upgrade an ability. The wallet and the seam every
  sink spends through are #211; the sinks themselves are its sub-tickets. Appendix 47
  records the model.
  - **Nothing is handed over to leave, and everything past the first thing is surplus**
    [SETTLED]. A currency you must hand over to get out is a toll, not a currency — the
    two rules cannot both hold, and this is the one that survives. So the campaign's exit
    **takes nothing**: intel, caches and unlockables are all things you *chose* to stay
    for, and how deep you go is your call. (v1 quick play keeps *objectives required,
    early exit refused*: it has no hub to spend at.)
    - **The one thing it asks is that the raid happened**: the **minimum haul**
      (`IntelGate::AtLeastOne`, §4.5/#574) — at least one objective taken from this
      facility, a console `$` or a crate `¤`. Nothing is debited, the wallet never sees
      the exit, and the haul is kept in full, so this is a *check* and not a fee; the
      currency argument above is untouched. What closes is the case of walking out with
      nothing, which made a facility something you could stand in the doorway of and made
      the map's whole structure optional. **One is the number and stays one** — two would
      be a quota, and a quota is a toll wearing a different hat. Appendix 59.
    - **It removes the abort**, deliberately. A raid that goes wrong in the first ten
      turns can no longer be walked out of, and a run pinned at condition 3 with no
      console reached has no way out but through. The counter-argument is that a facility
      you cannot take one single thing from is a raid you were losing anyway, and
      permadeath games are allowed to say so. If play shows it produces
      unwinnable-but-not-yet-dead states, the escape hatch to consider is **opening the
      exit unconditionally at alert condition 3** — not softening the gate, which would
      put the revolving door back. [OPEN on that hatch, until a played run asks for it.]
  - **One spend context: the map between facilities** [SETTLED]. There is no in-level
    spending, so the map screen (#208) is also the **hub** — the balance is a line on it
    and the prices are rows of it, read in one glance. A wallet you could dip into
    mid-raid would let a player buy their way out of a §4.4 mistake, and every tight
    corner would become a shop.
  - **The wallet is the only debit path** [SETTLED]. A sink asks the campaign to spend
    and is told what happened — paid, not enough intel, or not at the hub — and a
    refusal changes *nothing*: no partial payment, no half-applied sink. The wording of
    the refusal belongs to the wallet, so every sink refuses in one voice (§11.7).
  - **The first sink: the alternative route** (#212). Spending **one intel** [START] at a
    choice point flips the map's intel-locked successor to takeable. What it buys is
    **ground** — a lane two across, which no open edge from there can reach — and *not* a
    better facility: what stands on it is whatever the seed put there.
    - **The price is what you know, not what you earn** [START]. Unbought ground draws as
      `?`, so the road is bought *unseen*, and a price has to be proportionate to that: a
      facility's whole haul is three consoles, and a blind road is not worth a raid. It
      still asks for something real — nothing is banked until a raid is walked out of, so
      the first choice point of a run cannot afford one. The bite is **opportunity cost**
      against the other sinks rather than scarcity; if a played run buys one reflexively
      at every junction, the first lever is not the price but that the player cannot see
      what they are buying, which is what the scouting sinks (#215/#216) are for.
      Appendix 48, which also records the price this one replaced.
    - **Buying does not commit the run** [SETTLED]. A bought edge becomes an ordinary
      offer with its flavour showing, and the run may still take an open road instead. So
      the purchase buys ground *and* the knowledge of what is on it, which is what keeps
      "flavours are visible when offered" true and stops the sink being a coin flip you
      paid for.
    - **A bought road is alerted at condition 3 like any other** [SETTLED]. The top of the
      ladder takes away *the route around it*; intel that bought immunity from the alert
      would be a second, unwritten rule about what the alert is. At condition 2 the
      locked edge is never the marked road — finding the unwatched one is the play there,
      and it must not cost intel.
  - **The second sink: scouting the facility ahead** (#215). Spending **three intel**
    [START] at the hub buys a **plan of one facility's contents** — where its consoles,
    crates and cupboards are — and it walks in with them drawn in §11.5a's **remembered**
    state instead of hidden until seen. It is the campaign's answer to *"I cannot see what
    I am buying"*: the route sink buys ground, this buys what is standing on it.
    - **Position only, never live state** [SETTLED]. Guards, door poses and cones are
      earned inside the facility exactly as they were, and the comms console stays hidden
      (§7.3's counterplay has to be found). The deliberate override of §11.5a's
      *contents are hidden until seen* is recorded there, in the section it bends.
    - **It costs a facility's whole haul** [START]. Three consoles is what the §10.2
      recipe puts in a building, so knowing a building costs robbing one: no run can scout
      its first facility, and few can scout two in a row. It is the expensive end of the
      hub against the road bought blind at one, and that ratio is the design — what is
      sold here is the §10 exploration of an entire facility, answered before turn one.
    - **Bought before the run commits, and spent if the run declines** [SETTLED]. The
      purchase is made at the choice point, on a facility the run has *not* walked to, so
      it is part of the choice rather than something done on arrival — and backing out to
      take another road leaves the intel spent. Scouting the road you are not sure of is
      exactly the purchase that can be wrong, which is what stops the sink being a toll on
      the way in.
    - **Picking a facility no longer raids it: it opens the facility brief** (#215). The
      map's list answers *which facility*, and the brief — the same picture with the rows
      swapped — answers *what about it*: enter it, scout it first, or go back. The one
      irreversible press of a campaign (§2.1) is now a row on a screen the player asked
      for rather than the first thing a finger lands on, and the hub's prices have
      somewhere to live that is not a third screen.
    - **A facility with no room left in its level-seed token is not offered the sale**
      (§12.7). A token carries a bounded number of rules, and a rich facility under an
      alerted campaign can already be spending them all; selling one more would hand the
      player a facility that cannot be written down, shared or replayed. The row is
      absent rather than drawn and refused — unlike a price the run merely cannot afford
      yet, which it can save up for.
  - **The third sink: the cache manifest** (#550). Spending **two intel** [START] at the
    hub tells you **which tech** the crates of one facility ahead hold — the set, never the
    positions. A flavour already says *how many* crates a facility hides; this sells the one
    thing it does not, which is what turns the optional detour to a `¤` from a gamble into a
    decision: §8.3 lets a run meet tech it already carries, and a full bar makes the find an
    exchange (#266).
    - **What, never where** [SETTLED]. The crates stay fogged until seen (§11.5a); their
      cells are #215's to sell, and the two sinks compose without either implying the
      other. The list is in the stocking draw's own order, which carries no spatial
      information — a manifest ordered by cell would hand over the other sink for free.
    - **It changes nothing about the facility**, so unlike the scout it is **not a level
      modifier** and takes **no level-seed token slot** (§12.7): there is nothing to carry
      into the raid, only something the hub knows how to say. What is bought is the
      *telling*.
    - **It cannot lie** [SETTLED]. The hub reads the crates through the same
      `cache_contents` draw the generator later stocks the facility from, on the same
      seed — §8.3 stocks a building before anyone breaks into it, so the answer exists
      before the level does and there is no second copy of the rule to drift.
    - **Not offered where there is nothing to sell.** A facility that hides no crates (an
      Outpost) has no row at all, rather than a row that always refuses. Flavours are
      visible when offered, so the absence says nothing the map had not already said.
    - **It is read on the facility brief, in place** (#215's screen). Once bought, the
      priced row becomes the heading of a short list of the tech itself. A third screen for
      at most three names would put the fact one press further from the two decisions it
      informs — walk the detour, and raid this facility at all.
  - **Walking out empty-handed is not a state that exists** [SETTLED, #574]. It was
    [OPEN on the tuning] while it was possible — *nothing is taken away for a wasted
    raid, and an explicit nudge is deferred until a played run says the emergent cost is
    too soft* — and the minimum haul closes the question rather than tuning it: the exit
    refuses an empty haul, so there is no empty-handed departure left to punish. A **thin**
    raid is still unpunished and still a real cost: one console taken and the rest left
    behind leaves the run poorer at a facility the alert may have made harder (#210), and
    caches are one-shot.
- Difficulty that scales with the alert level, driven through the level-modifier
  seam (§12.6) rather than a private knob set. **The whole point of the alert
  system is that being loud in facility 2 makes facility 3 harder.** Until that
  loop closes, alert is decoration. The loop is closed (#210, appendix 41): the
  campaign alert is the **condition the last raid ended at**, and it lands on the
  facilities the map is about to offer.
  - **The mapping** [START]. Condition 0 — nobody ever noticed you — leaves **one**
    open road ahead *off guard*, drawn one **easier** rule: the cherry on a ghost
    raid. Condition 1 carries nothing. Condition 2 leaves **one** open road
    *alerted*, drawn one **harder** rule. Condition 3 leaves **all** of them
    alerted, each drawn its own — the **alternative route included** (#212, appendix 48),
    so intel buys ground rather than a way out of the top rung.
  - **The step from 2 to 3 is breadth, not depth** [SETTLED]. Both switch on one
    rule; what the top of the ladder takes away is the *route around it*. At
    condition 2 there is still an unwatched road and finding it is the play; at
    condition 3 there is not. That is an escalation the player can read off the map
    and act on, which a second modifier stacked on one facility would not be.
  - **It reaches one hop and does not accumulate** [SETTLED]. Being loud in facility
    2 makes facility 3 harder — that is the sentence, and the sentence is the whole
    promise. A raid whose noise still bent facility 6 would be a difficulty curve
    nobody designed, and the player would have no way to tell which raid they were
    still paying for. It is also the answer to §2.2's death-spiral risk: a level that
    cannot add to itself cannot spiral, which is why there is no decay rate and no
    floor to tune. Lowering it deliberately, with intel, is #213's sink.
  - **The intel-locked edge is never alerted** [SETTLED]. The mark lands only on
    ground the run can walk onto; on the locked edge it would be an alert with
    nothing behind it, which is the §2.3 failure this closes. The road intel buys is
    the one the noise did not travel.
  - **It is legible before the choice, not after** [SETTLED]. The map screen names
    the loudness and the facility it settled on — a facility's flavour identifies it
    exactly, since no two open successors share one — because routing around an
    alerted facility is the play at condition 2, and a player told afterwards has been
    told nothing. Inside the facility the drawn rule is on the help panel's Level info
    tab with every other active modifier.
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
- ~~Keys and locked doors~~ *(shipped as the §10.4/§12.6 locked-room modifier, #236)*
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
   name *which* guard, or just "a guard"?
4. **Run score.** *(Resolved — §4.6 owns the answer: a completed facility is scored
   out of three stars, one per axis, and takedowns are charged through the **stealth**
   star rather than by a rule of their own. The ghost↔aggressive spectrum falls out of
   the §7.3 ladder, which is the "may well be enough" branch this question named. The
   bot's objective function is deliberately **not** changed: the sim reports the star
   distribution and optimises nothing, because a bot playing for stars would make the
   histogram measure the scorer rather than the game — §13.3.)*
5. **Do guards check hideouts?** *(Resolved — §10.3 owns the answer: only when
   alerted, and only if they saw you go in or found a body nearby.)*
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
9. **Where does ability state live on screen?** *(Resolved — §11.4 and appendix 20
   own the answer: the always-on bottom-right bar.)*
