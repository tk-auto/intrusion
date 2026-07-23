# CLAUDE.md — session conventions for this repo

## The game

**Intrusion** is a turn-based, grid-based, top-down roguelike, rendered as a
character grid. You are a lone intruder raiding a patrolled government facility
for data, entering and leaving by your own tunnel — there is no other exit.
Stealth is encouraged and measured risk-taking rewarded; guards catch you on
contact. It is a true roguelike: permadeath, no meta-progression — you get
stronger *within* a 2–3 hour run by salvaging tech and intel, but nothing
carries across runs except what you learned. Takedowns are permanent (the cost
is the body, not a timer), and the protagonist never kills — that is fiction,
not a mechanic.

**Design notes: [`docs/design.md`](docs/design.md)** — the single source of
truth. It is a from-scratch rebuild, self-contained, and every number in it is a
starting value, not a law. Read it before changing behaviour; cite its sections
(e.g. §7, §13.2) when the design bears on the work. Status markers: **[SETTLED]**
(don't relitigate without a reason), **[START]** (a tuned starting value,
expected to move), **[OPEN]** (genuinely undecided, listed in §15).

## Skills

Project skills live in `.claude/skills/` (see its
[`README.md`](.claude/skills/README.md)). Invoke one with its slash command
(e.g. `/work-ticket`) or just by describing the task — reach for the matching
skill rather than improvising its workflow. The intended loop:

- **`/create-tickets`** — turn a slice of `docs/design.md` into GitHub issues.
  Proposals happen in conversation for approval, then become issues with
  v1/v2/v3 milestone and area/type/size labels.
- **`/survey`** — read the *code* for cleanup opportunities (oversized files,
  muddy naming, convention drift) and feed the good ones to `/create-tickets`.
- **`/work-ticket`** — pick an open issue and build it end-to-end: branch per
  ticket, unit tests, the fmt/clippy/test gate, commit conventions, and a PR
  that closes it.
- **`/artifact-build`** — pack the wasm build into a self-contained Claude
  Artifact at a stable private URL, for a playable preview before merge or while
  iterating.
- **`/playtest`** — balance metrics via the headless sim (§13.2). **Incomplete**
  until `crates/sim` exists.

Conventions live inside each skill (ticket taxonomy in `create-tickets`; the
branch/test/lint/commit/PR rules in `work-ticket`), so follow the skill rather
than duplicating its steps here.

## Language

Instructions may be given in French. Regardless of the language of the
request, all replies, commit messages, code, comments, and other written
work must be in English — **British English**. Use British spelling
consistently in identifiers, comments, and prose (`neighbour`, `behaviour`,
`colour`, `-ise`), so the same concept is never spelled two ways. The sole
exception is external vocabulary that is fixed by its source — CSS keywords
such as `color` and `background-color` keep their canonical spelling.

## Waiting on external state: basic monitors only

**Never use the Claude Code Remote MCP tools** — no `subscribe_pr_activity`,
no `send_later`, no triggers/routines. To wait on anything external (CI on a
PR, the Pages deploy, a long build), use a basic monitor instead: a background
timer (the Monitor tool, or a Bash `run_in_background` sleep/until-loop — never
a foreground sleep) that wakes you to poll the state and re-arms until done.

GitHub state (check runs, workflow runs, PRs, issues) is only reachable through
the GitHub MCP tools (`pull_request_read`, `actions_list`, …) — the raw
`api.github.com` is blocked from Bash in these sessions, and there is no `gh`
CLI.

## Pull requests

- **There is no PR template** in this repo — don't go looking for one. PR body
  format is defined in the work-ticket skill.
- History convention: one squash-merged commit per issue (`… (#24)`), merged
  deliberately after watching CI go green (see the work-ticket skill).
- After a merge to `main`, the Pages workflow deploys the playable build to
  <https://tk-auto.github.io/intrusion/> — wait for it and hand that URL back.
- For a playable preview *before* merge (or while iterating), use the
  artifact-build skill: it packs the wasm build into a Claude Artifact at a
  stable private URL. Player-visible PRs are validated this way before merging
  (see the work-ticket skill, step 8).

## Tickets

Ticket proposals happen **in conversation**, never in a draft file
(`docs/tickets-draft.md` was deliberately removed) — see the create-tickets
skill. Issues on GitHub are the durable index.
