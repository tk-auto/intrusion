---
name: create-tickets
description: >-
  Turn the Intrusion design doc (docs/design.md) into GitHub issues for the next
  steps of work. Use when the user wants to create tickets, break the design into
  tasks, plan the next steps, populate the backlog, or "make issues from the design".
  Proposes tickets in the conversation for approval first, then creates the approved
  ones as GitHub issues with milestone:v1/v2/v3 labels and area/type/size labels.
---

# Create tickets from the design

Convert `docs/design.md` into actionable GitHub issues. The design doc is the
single source of truth for *what* the game should be; this skill turns slices of
it into *work*.

## Workflow: propose in conversation, then create

This skill is **two-phase on purpose**. Never create issues on GitHub before the
user has seen and approved the proposal. **There is no draft file** — the proposal
lives in the conversation itself; do not write tickets to `docs/` (a
`docs/tickets-draft.md` used to exist and was deliberately removed).

### Phase 1 — Propose

1. Read `docs/design.md` in full (or the sections the user named). If the user
   pointed at a specific area ("tickets for the guard AI"), scope to that;
   otherwise default to **the next unstarted slice of the roadmap (§14)** — for a
   greenfield repo that is v1.
2. Cross-check what already exists: run `git ls-files` and list open issues
   (`list_issues`) so you don't re-file work that is already tracked or done.
3. Post the proposed tickets **as a message to the user**, using the template
   below, grouped by milestone, with a short summary (count per milestone). Keep
   each ticket to a **single reviewable unit of work** — if it can't be finished
   in one PR, split it or make it an epic with checklist sub-tasks.
4. Ask the user to approve, edit, or drop tickets before you create anything.
   If they directly asked for a specific ticket to be created, that request is
   the approval — skip the wait, not the proposal quality.

### Phase 2 — Create (only after explicit approval)

1. Ensure the labels below exist (create any that are missing — this is idempotent;
   skip ones that already exist). Applying a label on `issue_write` auto-creates it
   if the repo doesn't have it yet.
2. For each approved ticket, create a GitHub issue with its title, body, and
   labels — including its `milestone:*` label.
3. Report the created issue numbers back to the user. The issues themselves are
   the durable index — there is no draft file to update.

Use the GitHub MCP tools (`issue_write`, `list_issues`, etc.). If the GitHub MCP
is unavailable, say so and stop rather than inventing issues.

## Milestones as labels (from §14)

**GitHub milestone objects are not created via the API tooling in use — use a
`milestone:*` label instead.** Also record the milestone in each ticket body
(`**Milestone:** v1`) so the issue stays self-describing. Map every ticket to
exactly one:

| Label | Scope |
|---|---|
| **`milestone:v1`** | Quick play only — one generated facility, the full stealth loop, the hiding game. The §14 "Included" list. |
| **`milestone:v2`** | The headless sim + metrics (§13.2), saves, options, help/legend, game-over screen, alert indicator. |
| **`milestone:v3`** | The campaign — facility map, salvaged-tech accumulation, intel currency, alert-scaled difficulty, an ending. |

Backlog ideas (§14 "Later") get no milestone label unless the user asks; mention
them in the proposal but don't file them by default — they are experiments, not
committed work.

## Labels

Create these if missing, then apply one from each group per ticket.

**milestone:** (the roadmap slice — see the table above; stands in for a GitHub
milestone object) — one of `milestone:v1`, `milestone:v2`, `milestone:v3`.

**area:** (the system the ticket touches)

- `area:build` — workspace, CI, tooling, determinism plumbing
- `area:generation` — facility generation, corridors, cover, reachability (§10)
- `area:guards` — guard AI, patrol, chase, search, cooperation, radio (§7)
- `area:vision` — FOV, shadowcast, cones, danger overlay (§6, §11.5)
- `area:sound` — propagation and presentation (§9)
- `area:abilities` — ability model, targeting, the starting set (§8)
- `area:render` — the character grid, colour, layout, fog/memory (§11)
- `area:sim` — headless harness and metrics (§13)
- `area:core` — turn loop, rules, occupancy, spatial model (§4, §10.5, §12)

**type:**

- `type:feature` — new behaviour
- `type:chore` — tooling, refactor, scaffolding
- `type:bug` — something is wrong
- `type:tuning` — a `[START]` number to try (§12 is the machinery for changing them)

**size:** `size:S` (a sitting), `size:M` (a day), `size:L` (split it before filing).

## Ticket body template

```
## Summary
<one paragraph: what and why, in the game's terms>

## Design reference
§<section> — <title>. Paste the specific rules/numbers this ticket must honour.
(The design doc is the contract. `[SETTLED]` items are non-negotiable; `[START]`
items are starting values the ticket may tune with justification.)

## Acceptance criteria
- [ ] <observable, testable outcome>
- [ ] <the assertion or golden test that proves it, where §10.6 / §10.1a-style
      properties apply>
- [ ] Tests + lint gate pass (see the work-ticket skill).

## Notes / risks
<known traps from the design doc — e.g. the §7.6 "tracking turret" failure mode>
```

## Principles when slicing the design

- **Prefer a working pressure system to a stubbed feature.** §2.3 is explicit: the
  old game failed because systems were inert. A ticket that ships half a system
  that *runs* beats one that ships a facade.
- **Every `[START]` number is a candidate `type:tuning` ticket**, but don't file
  the whole tuning surface up front — file the feature, note the numbers to try.
- **Assertions are tickets too.** The reachability check (§10.6) and the sightline
  rule (§10.1a) are testable properties; file them as their own work.
- **Respect dependency order.** Spatial model (§10.5) → generation → vision →
  guards → sound → abilities. The spatial model (§10.5) is called out as the
  highest-leverage structural decision; most guard work is blocked behind it.
