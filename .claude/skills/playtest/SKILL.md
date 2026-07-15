---
name: playtest
description: >-
  Playtest Intrusion via the headless sim to judge balance and find dead/dominant
  strategies. Use when the user wants to playtest, run the sim, get balance metrics,
  check win rate, or evaluate whether an ability/guard change is any good. NOTE:
  INCOMPLETE — the headless sim (design §13.2) does not exist yet; this skill is a
  scaffold to be filled in once that architecture lands.
---

# Playtest (INCOMPLETE — awaiting the headless sim)

> **Status: not yet usable.** This skill is a deliberate placeholder. It gets
> filled in once the `crates/sim` headless harness (§13.2) exists. Until then, if
> invoked, tell the user the sim isn't built yet, point them at the tracking work
> below, and stop — **do not fake metrics or hand-play the canvas.** An agent
> cannot playtest a canvas; it can playtest a headless sim (§13.2). That's the
> whole reason this skill waits.

## Why this skill exists but is empty

The design is explicit (§13, §2.3): the class of failure that killed the old game
— a free win button, deaf guards, an inert alert system — is *invisible to a human
playtester and obvious to a bot*. A bot playing 500 levels reports "ability X used
94% of turns, win rate 99%" on the first run. This skill is that bot's front door.
It cannot be written until the sim it drives exists.

## What must exist before this skill can be filled in

Blocked on the §13.2 work (roughly a v2 milestone item, may land earlier):

- [ ] `crates/sim` — a headless target (NOT a CLI, NOT the terminal UI) that runs
      *N* seeded games with a scripted or bot player and emits metrics.
- [ ] A bot/scripted player policy (even a crude one) driving `core` through the
      deterministic `state × input → state` loop (§12.1, §12.4).
- [ ] Metrics emitted as machine-readable output (JSON/CSV) so this skill can parse
      and summarise them.
- [ ] Seed sharing (§13.1) so a flagged seed can be replayed exactly in the real
      game — `(seed, [inputs])` is the whole replay (§12.4).

## The metrics this skill will report (§13.2, all [START])

When filled in, run the sim across a batch of seeds and surface:

| Metric | What it catches |
|---|---|
| Win rate | Difficulty |
| Turns to win | Pacing; the "don't drag exploration" pillar |
| **Ability usage histogram** | **Dominant strategies and dead abilities** (the 94%-neutralise scream) |
| Detection events / run | Whether stealth is actually happening |
| Takedowns / run | Whether §7.2's cost is real |
| Bodies found by guards | Whether §7.3's radio clock has teeth |
| Alert peak | Whether escalation escalates |
| **Strategy diversity across seeds** | **Boredom** — same sequence every seed = a puzzle with one answer |

## How this skill will behave once complete (intended shape — do not run yet)

1. Take a config: N seeds, which player policy, and any ability/guard overrides to
   A/B against the current tuning.
2. Build and run `crates/sim` headless over the batch.
3. Parse the metrics; compare against the previous baseline if one is stored.
4. **Flag, don't judge (§13.3–§13.4).** The bot is a smoke detector, not a fun
   oracle — it will happily take a 5% capture risk forever and cannot be bored.
   Output = "these N seeds look suspicious (dominant ability / zero diversity /
   win-rate cliff), go play them." Then hand the user the shareable seeds.
5. Never conclude "the game is fun/unfun" from sim numbers. Fun is a human
   judgement (§13.1); the sim narrows *what's worth playing*.

## When you fill this in

Replace this section with the concrete build/run commands and the parsing logic,
delete the INCOMPLETE banner and the `NOTE: INCOMPLETE` line from the frontmatter
description, and add an example invocation. Keep the §13.3–§13.4 "flag, don't
judge" framing — it is the point, not boilerplate.
