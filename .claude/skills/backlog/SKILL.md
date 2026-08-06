---
name: backlog
description: >-
  Run a backlog conversation: the user brings ideas from their head or from
  playing, each one is settled by a short back-and-forth, and every settled idea
  becomes a GitHub issue immediately — one at a time, never a batch at the end.
  Use when the user opens a backlog or brainstorm session, says "I've got some
  ideas", "let's talk through some tickets", "file that as a ticket", or starts
  describing changes they want without pointing at a design section. The sibling
  of create-tickets: that one mines docs/design.md, this one mines the user.
---

# Backlog conversation

A working session where the **user is the source**. They arrive with something
half-formed — a feeling from a playtest, a gap they noticed, a change they want —
and the job is to turn each one into a filed issue before moving to the next.

This is the sibling of `/create-tickets`, not a replacement:

| | `/create-tickets` | `/backlog` |
|---|---|---|
| Source | `docs/design.md`, a roadmap slice | the user's head |
| Shape | propose a batch → approve → file the batch | settle one → file it → next |
| Approval | an explicit review round on the proposal | the answers *are* the approval |

Everything about the **ticket itself** — milestone/area/type/size labels, the body
template, the slicing principles — lives in `create-tickets`. Read it and follow
it; do not restate it here or invent a second taxonomy.

## The loop

For each idea the user brings:

1. **Ground it before you speak.** Read the design sections it touches and the
   code that would change — the real files, not a guess at them.
2. **Ask the one question that most changes the ticket.** One at a time, plain
   text, with a recommendation.
3. **Repeat** until nothing load-bearing is unsettled.
4. **File it immediately**, then say you have and invite the next one.

Nothing accumulates. When the session ends — and it always ends abruptly, mid-idea,
because that is how these go — everything settled is already on GitHub.

## 1. Ground it before you speak

The first reply to an idea is not a question, it is a *read*. Before asking
anything: find the design sections that govern it (`docs/design.md`, and
`docs/design-rulings.md` if a numbered appendix already argued it), and open the
code that would actually change.

This is what makes the questions worth asking. A question you could have answered
by reading §11.7 yourself spends the user's attention for nothing, and — worse —
invites them to re-decide something the design already settled. Open the reply by
naming what you read, in one line, so the user can see the ground you are standing
on and correct it if you read the wrong thing.

Grounding is also where the **scope traps** surface: the field the message struct
does not have yet, the purity rule that stops a lookup, the sibling event that
would need the same treatment. Those belong in the ticket, and you cannot write
them from memory.

## 2. Ask the one question that most changes the ticket

**One question per message, in plain text.** `CLAUDE.md` forbids the
`AskUserQuestion` tool here — write the question out and let the user answer
freely, including in ways your options did not anticipate.

- **Order by leverage.** Ask the question whose answer redirects the most work
  first. Scope usually beats mechanism; mechanism usually beats presentation.
  Later questions are often moot once the first is answered — that is the point of
  asking singly, and you should notice and drop them rather than ask anyway.
- **Give named options and recommend one**, with the reason in a sentence or two.
  "Which do you want?" with no recommendation makes the user do your thinking;
  a recommendation with its reasoning lets them disagree cheaply and precisely.
  In this repo they often will, and the correction is usually simpler than the
  proposal — take it at face value rather than defending the design.
- **Flag, don't ask, what you can decide.** If the answer is obvious from the
  design or the code, state the decision in a line and carry on.
- **Say what is still to come.** A one-line "then I'll settle timing and
  placement" lets the user pre-empt a question or tell you it does not matter.
- **Watch for the word behind the word.** *Flashing*, *modal*, *toast* mean
  different things to different people; when an answer would change the ticket
  depending on the reading, ask which one they meant. Getting this wrong late
  costs a rewrite of the body.

## 3. Settled means filed — now, not later

The moment the last load-bearing question is answered, **write the issue**. No
second proposal, no "here's the draft, shall I file it?" — the answers were the
approval, and asking again spends a round to learn nothing.

Before creating it:

- **Search for a duplicate** (`search_issues` on the repo, open and closed) — the
  backlog is large and an idea from a playtest is often one someone already had.
- **Check the labels** against `create-tickets`, including the `milestone:*` one.
- **State any narrowing out loud.** If the ticket implements the user's answer
  in a smaller way than their words — a placement rule that satisfies "anchor to
  the source cell" without adding a source field yet — say so in the reply *and*
  in the body, with what would force the fuller version and when to file it.
  Silently shrinking an approved decision is the failure mode this guards.

Then report it in two or three lines: number, title, labels, URL, and a compact
list of the decisions it captured. That list is the user's receipt — it is how
they catch a misread while it is still cheap. Close with an invitation for the
next idea, not a summary of the session.

## 4. Write the answers down as decisions

Every answer in the conversation is a design decision that will otherwise live
only in a transcript nobody re-reads. The issue body is where it survives, so:

- Record each settled point with its **status marker** — `[SETTLED]` for the shape
  that should not be relitigated, `[START]` for a number expected to move. Mark
  them separately when they differ: *that* it derives from the ladder can be
  settled while the threshold is a starting value.
- Say **why**, briefly, for anything that cost a round of back-and-forth. Not the
  transcript — the reason that would stop the next person undoing it.
- Name what the **implementing PR must write into `docs/design.md`**. The ticket
  is not the source of truth; the design doc is, and a rule settled here reaches
  it through the PR that ships the behaviour. If a decision was hard-won enough
  to deserve the long *why*, say that it wants a `docs/design-rulings.md`
  appendix — appended as the next number, never renumbered.
- Keep the design's own voice: British English, the world's vocabulary rather than
  the mechanism's for anything player-facing (§11.8).

## Guardrails

- **Never batch.** Two settled ideas held back to file together is the one thing
  this skill exists to prevent. If the user gives you three ideas at once, settle
  and file the first, then move to the second.
- **Never file half-settled.** An issue whose body says "TBD" or invents an
  answer the user did not give is worse than no issue: it will be worked from.
  If the user goes quiet mid-idea, leave it unfiled and say which one is pending.
- **Do not implement.** This skill ends at filed issues; `/work-ticket` builds
  them. If the user asks for the work now, hand off to that skill rather than
  starting a branch here.
- **Do not edit `docs/design.md`.** The rule reaches the design through the
  implementing PR (§ above), so the doc and the behaviour land together.
- **Ideas that are not tickets are allowed.** Some things belong in a
  `docs/design-rulings.md` appendix, some are a §14 backlog experiment that
  `create-tickets` says not to file, and some are the user thinking aloud. Say so
  rather than manufacturing an issue for everything you are told.
