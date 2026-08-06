---
name: survey
description: >-
  Quick code-health survey of the Intrusion codebase to surface refactor and
  cleanup opportunities — files or functions that have grown too large or too
  complex, muddy or inconsistent naming, modules that have lost their single
  focus, duplication, stale comments and dead code, drift from the design's
  conventions (docs/design.md, CLAUDE.md), and bloat in the always-loaded docs
  themselves. Use whenever the user wants to
  survey, review, or audit the code for things to improve, asks "what should we
  clean up", "is anything getting too big or messy", "where's the tech debt", or
  wants a health check before planning the next slice of work. This is a
  structural survey, not a bug hunt: for runtime correctness in a working diff
  use /code-review, and for reviewing one specific PR use /review. After
  reporting, asks the user how many findings to fix and then works the chosen
  ones one at a time — ticket, code, PR, merge on green — filing each ticket
  only when its fix starts.
---

# Survey the code

Take a fast, honest read of the codebase and hand back a short, prioritised list
of things worth improving — the stuff that makes the next change harder than it
should be. This is the instinct that noticed `state.rs` had swollen to 2 000+
lines and owned five unrelated concerns; the skill makes that instinct routine
instead of accidental.

**The survey itself changes nothing** — sections 1–5 only read and report, and
the user decides what is worth doing. But it does not stop at the list: section
6 asks how many of the findings to fix, then drives the chosen ones one at a
time — ticket, code, PR, merge on green — and files each ticket only when its
fix begins.

## 1. Scope it

Default to the whole `crates/` tree, plus the docs every session pays for —
`CLAUDE.md` and `docs/design.md` (§3, last lens). If the user names a path, a
crate, or a concern ("just the guard code", "naming only"), survey that instead
and say so. Skip `target/`, generated output, and vendored code.

## 2. Measure before you judge

Ground the survey in the real tree, not a vibe. Cheap measurements first, so
findings point at actual outliers:

- **Size:** line counts per file, and roughly per function/`impl`. A file far
  above its siblings is a *candidate*, not a verdict.
- **Shape:** the biggest files, the longest functions, the deepest nesting, the
  widest `match`. `cargo` and a couple of `grep`/`wc` passes are enough — you do
  not need a metrics tool.
- **Churn, if cheap:** a file that is both large *and* frequently touched
  (`git log --oneline -- <file> | wc -l`) is where cleanup pays back fastest.
- **Context weight:** `wc -l CLAUDE.md docs/*.md` — and note which of those are
  loaded every session (`CLAUDE.md`, `docs/design.md`) versus read on demand
  (`docs/design-rulings.md`, the references). Growth in the first two is the
  expensive kind.

Then **read the top offenders.** Size is a smell, not a defect: a long file that
is genuinely one cohesive thing is fine, and a short file can still be a mess.
Confirm every finding by looking at the code.

## 3. Look through these lenses

Sweep the surveyed code for each — and the docs for the last one. They overlap;
that's fine — dedupe at the end.

- **Too large / too complex.** A file or function doing too much at once; deep
  nesting; a function you can't hold in your head. Ask: *what are the seams?* A
  finding here should name the natural split, not just "it's big".
- **Lost cohesion.** A module whose name promises one thing but that owns
  several unrelated concerns (the `state.rs` case). The fix is usually
  extraction along an existing seam.
- **Naming.** Names that mislead, abbreviate cryptically, collide, or drift from
  the vocabulary the rest of the code and `docs/design.md` use (a `Guard`'s
  `station`, a sight `cone`, `[SETTLED]` terms). Inconsistent names for the same
  idea across files.
- **Duplication.** The same logic hand-rolled in several places — e.g. flood
  fills copied between modules — that wants one shared home. Name every site.
- **Incoherence / inconsistency.** Two modules solving the same kind of problem
  in different styles; an abstraction used one way here and another way there;
  an escape hatch (§8.1) promoted to code where data would do, or vice-versa.
- **Stale or dead.** Comments that no longer match the code (a doc that still
  says "guards are stationary"), `TODO`s overtaken by events, unused items,
  commented-out code, placeholders the real thing has replaced.
- **Convention drift.** Breaks from this repo's settled rules: `crates/core`
  purity (no I/O, clock, or unseeded RNG — §12.1/§12.4), rendering as a pure
  function of state, the fmt/clippy idiom, the comment-density and English-only
  conventions in `CLAUDE.md`.
- **Doc bloat.** `CLAUDE.md` and `docs/design.md` are read at the start of
  nearly every session, so every line in them is paid for on every task — they
  need to stay small, and they only grow. Flag:
  - **Duplication.** The same rule stated in two places — the design doc *and* a
    skill, §11.2 *and* the render reference, `CLAUDE.md` *and* the skill it
    points at. One home, cited from the others; `CLAUDE.md` points at the docs
    and skills, it does not restate them.
  - **Long prose.** Paragraphs where a sentence, a table row or a numbered rule
    would carry the same content. The design doc states *how the game is
    supposed to be*; it is not an essay about it.
  - **Post-mortems of discarded ideas.** The alternatives tried, the sim runs,
    the rework history — that is `docs/design-rulings.md`, appended and numbered
    and read only when someone relitigates. In the design doc, leave the ruling
    and a citation (*appendix 12*), not the argument.
  - **Dead weight.** Sections overtaken by shipped work, `[OPEN]` questions
    §15 has since settled, worked examples that only restate the rule above them.

  Cutting is not free: a `[SETTLED]` rule stated once, a starting value, or the
  short *why* behind a contentious decision is load-bearing — propose moving or
  compressing it, never dropping it. Each finding should say roughly how many
  lines it saves and where the content lands.

## 4. Judge honestly

The value of this skill is a *trustworthy* list, so guard against noise:

- **Rank by leverage** — impact on future work × how soon it bites — not by how
  easy it is to spot. Lead with the one or two things that, fixed, unblock the
  most.
- **Cap it.** Aim for the **top 5–10**. A survey that flags forty things gets
  ignored. If you dropped lesser findings to stay under the cap, say so in one
  line rather than padding the list.
- **Separate essential from accidental complexity.** Some things are hard
  because the problem is hard (shadowcast FOV, generation reachability). Don't
  flag those as debt; flag complexity that isn't earning its keep.
- **Respect deliberate design.** A `[SETTLED]` rule or a tuned `[START]` number
  is a decision, not an inconsistency. If you think one is genuinely wrong,
  frame it as a design question, not a cleanup.
- **No churn for its own sake.** If a change is risky and the payoff is only
  tidiness, say that plainly so the user can weigh it. Every finding must answer
  *what does fixing this make easier?*

## 5. Report

Reply in the conversation (this repo keeps proposals in chat, not draft files —
`CLAUDE.md`). Open with a one-line read of overall health, then the ranked
findings. For each:

```
### <n>. <short title>  ·  <area>  ·  ~<S|M|L>
**Where:** `path:line` (+ other sites)
**What:** the problem, in a sentence or two.
**Why it matters:** what it makes harder / what fixing it unlocks.
**Suggested fix:** the concrete move — the seam to cut, the name to use, the
shared home for the duplicate.
```

Size is rough effort: **S** ≈ an hour, **M** ≈ a focused session, **L** ≈ needs
its own plan. End with a short **What I looked at** note (scope, what you
measured) so the user can trust the coverage — and, if you deliberately skipped
areas, which.

## 6. Ask how many to fix — then work them one at a time

A list nobody acts on was not worth the tokens. So don't stop at the report and
don't leave the next step vague: **ask the user, explicitly, how many of the
findings to fix now.**

Ask in plain text, as a single question, per `CLAUDE.md` — not
`AskUserQuestion`, not a batch. Something like:

> That's 7 findings. How many do you want me to take on now — the top 3? I'll
> work them one at a time: file the ticket, build it, PR, merge on green, then
> the next one.

- **Ask; don't assume.** "None, I just wanted the picture" is a legitimate
  answer — take it and stop.
- **A count means the top N of your ranking**, since you ranked by leverage. If
  the user names specific findings instead, use theirs.
- **Read the chosen list back in one line** before starting, so a
  miscommunication costs a sentence rather than a PR.

### File tickets one at a time — never a stack up front

**Do not create issues for the whole chosen set before starting.** One ticket is
filed at the moment its fix begins, and not before. A stack filed in advance
goes stale the instant the first PR lands: the earlier fix moves the code the
later findings sit on, seams shift, sizes change, a finding gets fixed
incidentally — and the user may well stop after two, leaving the rest as
backlog noise nobody asked for.

For each chosen finding, in ranked order:

1. **Ticket.** File exactly one issue with the **create-tickets** skill's body
   template and labels (`area:` / `type:` / `size:` + `milestone:`). The
   finding already maps onto it: *Where/What* → Summary, the design sections
   and conventions it drifts from → Design reference, *Suggested fix* +
   observable outcome → Acceptance criteria, *Why it matters* → the rationale.
   The user approved this finding in the conversation, so that approval *is*
   create-tickets' phase 1 — file it directly, don't re-propose.
2. **Build it.** Hand the fresh issue to the **work-ticket** skill and follow it
   from its step 1b through step 8: the brief, a branch off freshly fetched
   `origin/main`, implementation, unit tests, `./scripts/gate.sh` green,
   conventional commits with `Closes #<n>`, the PR, and — if the cleanup can
   change anything a player sees — an artifact build to prove it still runs.
3. **Merge on green.** work-ticket step 9: check mergeability, watch CI yourself
   with a basic monitor (never the Claude Code Remote tools — `CLAUDE.md`),
   squash-merge when every check is green, then watch the `main` build. Its
   hold-instead-of-merge cases still apply (the user asked to review it, a key
   player-visible change awaiting their playtest, or you're not confident) — say
   which one and leave the PR open.
4. **Report, then re-read the next finding.** One line back to the user: issue
   number, PR URL, merge state. Before filing the next ticket, **check the next
   finding against the tree you just changed** — restate it if the seam moved,
   resize it, or drop it if the last fix already covered it. That re-check is
   the whole point of not filing in advance.

Then repeat. Stop when the count is reached, when the user says stop, or when a
held-open PR blocks the next fix (the next change would stack on unmerged work)
— in that last case say so and hand back rather than stacking silently.

**Findings you didn't work stay unfiled.** They live in this conversation, and a
later survey will surface them again if they still matter. If the user wants the
remainder on the backlog anyway, that's an explicit `/create-tickets` pass they
ask for — not something this skill does on its way out.
