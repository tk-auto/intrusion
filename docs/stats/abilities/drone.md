# Drone

**Salvaged tech (§8.3/#273)** — 1 turn to press, duration **40**, cooldown **40**,
`Behaviour::Coded` (§8.1's escape hatch, and the case the design names by hand).
Activating launches a drone from the cell you are standing on and **hands it your
input**: `Step` flies the machine and your body stands still. Pressing again hands the
controls back — free (§4.4) — and the drone **stays where you left it**, feeding you a
360° camera at `DRONE_SIGHT_RANGE` (8) for whatever is left of the window. One clock
covers both halves, so the number on the bar always means *turns of machine*. Guards
never perceive it: no cone detects it, nothing can be done to it. It respects the
building at its own scale — everywhere a person could squeeze, plus over a table and
through a shut door's vents, but never a wall, a door frame or a solid usable — and it
has no interaction verb at all, so a door it crosses stays shut.

**What it costs (§2.3):** every turn you fly is a turn your body stands unattended in a
patrolled facility. Capture is contact (§4.5), and a patrol walking into that body ends
the run while you are watching a corridor two rooms away. The one place that cost could
leak is a body nothing can reach, so launching — and taking the controls back — is
refused from inside a crawlspace (§10.7).

## What the cue says

**Nothing, and it is a different kind of nothing from Vision's.** Vision has no cue
because a passive has no press. This one has a press; what it does not have is a bot
that could survive making it.

Piloting is a **control mode**, and the §13.2 stealth bot does not have one. If its
policy pressed this key, the keys would transfer to a machine while the policy went on
issuing steps for a body that is no longer listening: it would fly its drone into a wall
for thirty turns, leave itself parked in a corridor, and report the result as a
measurement. So `crates/sim/src/cue.rs` declines in its own arm, with the reason written
there rather than left to the exhaustive match's silence.

What a real cue would take is a second policy, not a threshold: a flight plan over the
fog (§11.5a — *where has nobody looked?*), a judgement about how long the body can be
left standing where it stands, and a reason to come back. That is its own ticket.

## What the sim measured

**Nothing yet, and the histogram says which kind of nothing.** `usage.drone` exists as
a slot and will read **0** in every batch until a piloting policy lands. That zero is a
fact about the *bot*, not about the ability — the §13.2 false-zero ambiguity in its
purest form, which is exactly why the slot exists rather than being omitted: a verb with
no row could not report the day it starts being pressed.

The with/without pair every other page runs is not meaningful here either. Granting the
bot a verb it never presses measures nothing but the loadout slot it displaces, and the
draw already does that.

What **was** checked when the ability shipped is that adding it did not disturb what the
sim already measures: the pinned seed-42 run reproduces byte-for-byte (same outcome, same
turn, same usage) with the new column appended, and the baseline batch is unmoved. See
the PR for #273.

## History

- `#273` — page created with the ability. States that the zero in `usage.drone` is an
  unexercised verb rather than a dead one, and what a cue for it would have to be.
