# Cover

**Salvaged tech (§8.3/§10.3/#562)** — 1 turn to press, duration **18**, cooldown **35**,
`Behaviour::Coded` (§8.1's escape hatch, on Pierce Wall's grounds: writing, moving and
unwriting terrain is not a primitive the effect vocabulary has). Firing puts a §10.3
**partial-cover table** in the cell you face — the *same* terrain kind the §10.1a sightline
pass stamps, so it draws the same `π`, blocks movement and pathing like a wall, blocks no
sight, admits a drone, and joins whatever run it touches. The one addition: **bumping it
pushes it a cell directly away and steps you into the cell it vacated, crouched.** Repeat
and the table walks across an open room ahead of you, a cell a turn, concealing you from
its far side the whole way.

**On the board** it is a `π` like any other — no new glyph — wearing the §11.5 **Effect**
mark on the thing for its whole window, so you can pick your own piece out of a room of
furniture from across the room. Identical in every rule, told apart by the picture: the
mark is a *background*, so §10.3's `Owned` recolour of a covering run is untouched and the
two compose.

**Where it may go:** plain, empty floor and nothing else. A wall, a doorway, a hideout, a
duct mouth, any solid usable, the exit, a guard or a body all refuse it **for free**
(§4.4), and so does being inside a crawlspace (§10.7). That is also why removal needs no
eject rule — nothing can ever be *inside* a piece of cover. A shove with nowhere to go is
not a refusal either: it falls back to the plain §10.3 crouch, *crouch always, push when
it can*.

**What it costs (§2.3):** a turn spent standing in the open, because the deploy does not
duck you behind what it puts down — that is the bump, on the turn after. Then a cell a
turn, forwards only, so cover that has to turn a corner is cover you stand up from. Then
the window: expiry hands back plain floor and takes the pose with it, so a run halfway
across a room on turn eighteen is a standing figure in the open at the moment its
concealment evaporates. There is no grace turn; that moment is the ability.

**Deliberately no §10.6 severance check.** A solid that expires on its own clock, can be
pushed, and can be dismissed for free cannot make a facility unsolvable, so plugging a
corridor is the *tactic* — the one Lockdown already sells with a door. Appendix 63.

## What the cue says

**Nothing — and the reason is aim, not value.** This is the Drone's kind of zero arrived
at by measurement rather than by inspection, so the measurement is on the record.

The ability is aimed by facing (§8.4) and the §13.2 bot **faces the way it last stepped**;
there is no turn-in-place (§5). Its router prices watched cells out and holds rather than
stepping into a cone, so by the time a patrol is close enough that taking cover is the
plan, every recent step has been *away* from that cone and the faced cell is on the wrong
side of the bot. Two cues were built and run before the arm was dropped:

| Cue shape | Presses | Ducks behind it |
|---|---|---|
| **Gated** on core's own geometry — *would ducking behind the piece this press puts down hide me from every guard I perceive?* (`State::crouch_would_conceal`) | **0** in 120 seeds × `balanced`/`cautious`/`aggressive` | — |
| Same, relaxed from the push's line to the plain crouch's (one cell more generous) | **0**, unchanged | — |
| **Ungated** — `TakeCover`, no cupboard, no bench at the elbow, a patrol closing | 12 in 40 seeds | **0** |

The ungated row is the one that decided it. Twelve presses and not one duck is a turn and
a 35-turn lockout spent on furniture the bot then walked away from: a histogram reading
*used* while measuring nothing, which is #347's failure mode and the exact tell
[`Verb::Cover`](../../../crates/sim/src/usage.rs)'s own doc names. A cue that provably
never fires is worse than no cue, because it claims a behaviour the bot does not have.

The half of the ability that actually sells it is served worse still. What it is *for* is
a crossing walked behind the piece — and the bot's route is a Dijkstra over cells the
player can walk **through**, which a table is not, so the router plans **around** the bot's
own cover. Teaching it otherwise means teaching the field that a pushable solid is passable
when and only when the cell beyond is free, and then following that route as a mode: a
second policy, exactly as the Drone's flight plan is, and its own ticket for the same
reason.

**What the bot did gain** is smaller and worth having anyway: its crouch ladder now asks
`State::cover_push` instead of assuming the furniture stays put, so on the run's own cover
it predicts the shove — the player one cell on, the table one cell further — rather than a
stationary duck the press does not produce. A scripted run exercises that today; a policy
that grows the crossing mode later meets a bot that is not blind to the mechanic.

## What the sim measured

```
cargo run --release -p intrusion-sim -- --bot --profile <NAME> --runs 100 --seed 0 --cap 1000
cargo run --release -p intrusion-sim -- --bot --profile <NAME> --abilities cover --runs 100 --seed 0 --cap 1000
```

Measured at `9efdb5f+cover-562`. Every arm is **byte-identical to its own control** — same
outcomes, same turns, same detections, same usage, `cover: 0` — for all four temperaments:

| Profile | win rate | turns to win (median) | detections | `usage.cover` |
|---|---|---|---|---|
| `balanced` | 0.34 → **0.34** | 133 → **133** | 853 → **853** | **0** |
| `cautious` | unchanged | unchanged | unchanged | **0** |
| `aggressive` | unchanged | unchanged | unchanged | **0** |
| `careless` | unchanged | unchanged | unchanged | **0** |

That is what an uncued verb looks like and it is the expected result, not a finding:
granting the bot a key it never presses measures nothing but the loadout slot it displaces,
and the draw already does that. The value of running it is the *identity* — it says the
ability's terrain writes, bump arm and teardown disturb nothing the sim already measures
when nobody presses the key.

The pinned baseline is likewise unmoved: refreshing it after this change added the
`usage.cover` / `usage_share.cover` columns and **changed no metric in any of the four
blocks**.

**So the balance question is open and belongs to a human.** The thing to watch is the one
§8.3 and appendix 14 both name: the **crouch-walk**. §10.3 already lets a crouched player
move at full speed while hugging a run, and this ability makes that pose available on bare
ground for eighteen turns at a time. If it plays too strong the levers are the turn it costs,
contact-vulnerability, the requirement to keep hugging, and this row's own duration —
**not** re-narrowing the geometry the player has to read.

## History

- `#562` — page created with the ability. Records the two cue shapes that were built and
  measured, why neither ships, and what a real cue would need (a crossing mode, and a
  router that plans through a pushable solid). Appendix 63.
- `#562` — duration **12 → 18** on the first play-through, before merge. The deploy spends
  a turn of the window before the piece has covered anybody, so the crossing a run gets is
  `duration − 1` cells: eleven was not enough to cross the open ground a §10.2 board
  actually has and get out the far side, so the window kept ending mid-room. Seventeen
  does, and it moves the failure from *"it never quite gets there"* to a mistimed press.
  The lockout is unchanged — the numbers here are untouched either way, since the bot
  never presses the key.
