---
name: create-tickets
description: >-
  Turn the Intrusion design doc (docs/design.md) into GitHub issues for the next
  steps of work. Use when the user wants to create tickets, break the design into
  tasks, plan the next steps, populate the backlog, or "make issues from the design".
  Drafts proposed tickets to a review file first, then creates the approved ones as
  GitHub issues with v1/v2/v3 milestones and area/type/size labels.
---

# Create tickets from the design

Convert `docs/design.md` into actionable GitHub issues. The design doc is the
single source of truth for *what* the game should be; this skill turns slices of
it into *work*.

## Workflow: draft, review, then create

This skill is **two-phase on purpose**. Never create issues on GitHub before the
user has seen and approved the draft.

### Phase 1 — Draft

1. Read `docs/design.md` in full (or the sections the user named). If the user
   pointed at a specific area ("tickets for the guard AI"), scope to that;
   otherwise default to **the next unstarted slice of the roadmap (§14)** — for a
   greenfield repo that is v1.
2. Cross-check what already exists: run `git ls-files` and list open issues
   (`list_issues`) so you don't re-file work that is already tracked or done.
3. Write proposed tickets to `docs/tickets-draft.md` using the template below.
   Group them by milestone. Keep each ticket to a **single reviewable unit of
   work** — if it can't be finished in one PR, split it or make it an epic with
   checklist sub-tasks.
4. Show the user the draft path and a short summary (count per milestone). Ask
   them to review/edit the file and confirm before you create anything.

### Phase 2 — Create (only after explicit approval)

1. Ensure the milestones and labels below exist (create any that are missing —
   this is idempotent; skip ones that already exist).
2. For each approved ticket in the draft, create a GitHub issue with its title,
   body, milestone, and labels.
3. Record the created issue number back into `docs/tickets-draft.md` (e.g. append
   `→ #42`) so the file is a durable index, then report the list of created
   issues to the user.

Use the GitHub MCP tools (`issue_write`, `list_issues`, etc.). If the GitHub MCP
is unavailable, say so and leave the draft in place rather than inventing issues.

## The first ticket is always: scaffold the workspace

If the repo has no Rust workspace yet (no root `Cargo.toml`), the **first** ticket
in the draft must be the bootstrap task, because every other ticket depends on it:

> **Scaffold the cargo workspace and CI** — create `crates/core` (pure logic, no
> I/O, no wasm), `crates/web` (wasm-bindgen + canvas2d, thin), `crates/sim`
> (headless harness placeholder), a workspace `Cargo.toml`, a pinned PRNG choice
> (§12.4), `rustfmt.toml` + clippy config, and a CI workflow running
> `cargo fmt --check`, `cargo clippy -D warnings`, and `cargo test --workspace`.
> Acceptance: `cargo test --workspace` and the lint gate pass on an empty skeleton.

Label it `area:build`, `type:chore`, `size:M`, milestone **v1**.

## Milestones (from §14)

Map every ticket to exactly one:

| Milestone | Scope |
|---|---|
| **v1** | Quick play only — one generated facility, the full stealth loop, the hiding game. The §14 "Included" list. |
| **v2** | The headless sim + metrics (§13.2), saves, options, help/legend, game-over screen, alert indicator. |
| **v3** | The campaign — facility map, salvaged-tech accumulation, intel currency, alert-scaled difficulty, an ending. |

Backlog ideas (§14 "Later") get no milestone unless the user asks; note them in the
draft but don't file them by default — they are experiments, not committed work.

## Labels

Create these if missing, then apply one from each group per ticket.

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
- **Respect dependency order.** Scaffold → spatial model (§10.5) → generation →
  vision → guards → sound → abilities. The spatial model (§10.5) is called out as
  the highest-leverage structural decision; most guard work is blocked behind it.
