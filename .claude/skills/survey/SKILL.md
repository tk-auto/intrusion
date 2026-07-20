---
name: survey
description: >-
  Quick code-health survey of the Intrusion codebase to surface refactor and
  cleanup opportunities — files or functions that have grown too large or too
  complex, muddy or inconsistent naming, modules that have lost their single
  focus, duplication, stale comments and dead code, and drift from the design's
  conventions (docs/design.md, CLAUDE.md). Use whenever the user wants to
  survey, review, or audit the code for things to improve, asks "what should we
  clean up", "is anything getting too big or messy", "where's the tech debt", or
  wants a health check before planning the next slice of work. This is a
  structural survey, not a bug hunt: for runtime correctness in a working diff
  use /code-review, and for reviewing one specific PR use /review.
---

# Survey the code

Take a fast, honest read of the codebase and hand back a short, prioritised list
of things worth improving — the stuff that makes the next change harder than it
should be. This is the instinct that noticed `state.rs` had swollen to 2 000+
lines and owned five unrelated concerns; the skill makes that instinct routine
instead of accidental.

It is a **survey, not a rewrite**: you report, the user decides. Nothing here
edits code. The output is a ranked list that feeds `/create-tickets` (to file
the good ones) or `/work-ticket` (to fix one now).

## 1. Scope it

Default to the whole `crates/` tree. If the user names a path, a crate, or a
concern ("just the guard code", "naming only"), survey that instead and say so.
Skip `target/`, generated output, and vendored code.

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

Then **read the top offenders.** Size is a smell, not a defect: a long file that
is genuinely one cohesive thing is fine, and a short file can still be a mess.
Confirm every finding by looking at the code.

## 3. Look through these lenses

Sweep the surveyed code for each. They overlap; that's fine — dedupe at the end.

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

## 6. Hand off

Close by offering the next step, don't just drop the list:

- **File them:** the strong findings map cleanly onto tickets — offer
  `/create-tickets` to turn the approved ones into issues (it proposes in chat
  first, then labels them area/type/size). A survey finding's area and size
  slot straight into that taxonomy.
- **Fix one now:** if the user wants to act immediately, `/work-ticket` picks one
  up end-to-end.
- Let the user choose which findings are worth it. The survey informs the
  backlog; it doesn't dictate it.
