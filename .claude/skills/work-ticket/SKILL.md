---
name: work-ticket
description: >-
  Pick an open GitHub issue for Intrusion and implement it end-to-end on its own
  branch, with unit tests, the fmt/clippy/test gate, commit conventions, and a PR
  that closes the issue. Use when the user wants to work on a ticket, pick up an
  issue, implement the next task, or "grab something from the backlog". If it is
  unclear which ticket to work on, ask the user.
---

# Work a ticket

Take one GitHub issue from idea to a pushed, reviewable PR. This skill owns the
*how we work* conventions: branch-per-ticket, tests, linting, commits, and the PR.

## 1. Pick the ticket

1. List open issues (`list_issues`, or filter by milestone/label the user named).
2. **Choose one:**
   - If the user named a ticket or number, use it.
   - Otherwise, prefer the lowest-numbered unblocked ticket in the earliest
     milestone (respect dependency order — see the create-tickets skill; the
     workspace-scaffold ticket and the §10.5 spatial model come before most else).
   - **If more than one is a reasonable next pick, ask the user** with
     `AskUserQuestion` — list the top 2–4 candidates with a one-line rationale each.
     Do not guess when it's genuinely ambiguous.
3. Read the issue body *and* the design.md sections it references. The design doc
   is the contract; `[SETTLED]` rules are non-negotiable, `[START]` numbers may be
   tuned with justification recorded in the PR.

## 2. Branch

One branch per ticket, off the current default branch. **The branch name must
say what the change is about** — `<type>/<issue-number>-<slug>`, where the slug
is a short, human-readable description of the work:

```
git fetch origin main
git checkout -B <type>/<issue-number>-<slug> origin/main
```

e.g. `feat/12-shadowcast-cone`, `chore/1-scaffold-workspace`. `<type>` matches the
ticket's `type:` label (`feat`, `chore`, `fix`, `tune`). Never ship a branch whose
name is an opaque or auto-generated token (e.g. `claude/next-ticket-xxxxx`) — the
name is the first thing a reviewer reads.

> If the session was assigned a development branch whose name is already
> descriptive of this ticket, use it. If the assigned branch name is opaque or
> generic (a random token, `next-ticket`, etc.), **rename it to the descriptive
> convention above** before pushing (`git branch -m <old> <type>/<issue>-<slug>`),
> and push the renamed branch — do not push work under the opaque name.

## 3. Implement

Follow the architecture in §12:

- **`crates/core` is pure and deterministic.** No I/O, no wasm, no DOM, no clock,
  no `Date::now`, no unseeded RNG. `state × input → state, events`. If you reach
  for randomness, thread the seeded PRNG (§12.4) — never construct a fresh source.
- **Rendering is a pure function of state** producing the character grid (§11.1).
- **Keep the escape hatch honest:** abilities start data-driven; promote to code
  only when the vocabulary genuinely can't express it (§8.1).
- Match the surrounding code's idiom, naming, and comment density.

## 4. Test — this is not optional

The whole architecture (§12.1) exists to make the core testable natively in
milliseconds. Use it.

- **Unit tests** for the logic you added, in the same crate. Pure-core tests need
  no browser and run sub-second.
- **Golden / property tests where the design asks for them:**
  - Reachability: assert `start → every objective → exit` by flood fill; reject
    unsolvable seeds (§10.6).
  - Sightline rule: assert no unbroken straight run longer than *L* (§10.1a).
  - Determinism: same seed + same inputs → identical final grid (§12.4). A replay
    is `(seed, [inputs])` — assert against a golden grid.
- **A tuning ticket must add or update a test that pins the new numbers** so a
  later change that moves them is visible.
- Prefer a failing test first when fixing a `type:bug`.

## 5. The quality gate — must be green before you commit

Run all three from the repo root and fix anything they flag:

```
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace
```

Warnings are errors (`-D warnings`). If the workspace doesn't exist yet, the ticket
you picked is almost certainly the scaffold ticket — set the gate up as part of it.
Never commit with a red gate; if you can't get it green, stop and report why.

## 6. Commit

- **Conventional commits:** `<type>(<area>): <imperative summary>` — e.g.
  `feat(vision): carve the forward cone with artificial-wall shadowcast`.
  `<type>` ∈ {feat, fix, chore, refactor, test, tune}; `<area>` matches the
  ticket's `area:` label.
- **One logical change per commit.** A refactor that enables the feature is its own
  commit before the feature.
- Body: *why*, not just *what*. Note any `[START]` value you tuned and the reason.
- Reference the issue in the final commit: `Closes #<n>`.
- Do not commit generated artifacts, `target/`, or wasm build output.

## 7. Push and open the PR

```
git push -u origin <branch>
```

Retry on network failure with exponential backoff (2s, 4s, 8s, 16s). Then
**always open a PR** (`create_pull_request`) targeting `main` — a finished ticket
ends in a PR, every time; don't wait to be asked and don't stop at a pushed
branch:

- **Title:** the conventional-commit summary.
- **Body:** what changed and why, the design section(s) honoured, how it was
  verified (paste the gate result), any `[START]` numbers tuned, and `Closes #<n>`.
- **This repo has no PR template** — don't go looking for one; the body sections
  above are the format.
- Link the PR back on the issue if useful; don't over-comment.

Report the branch, the gate result, and the PR URL to the user, then move
straight to validating the build (step 8) and watching CI (step 9).

## 8. Validate player-visible changes with an artifact build

If the PR touches anything a player would see or feel — `crates/core` logic or
rendering, `crates/web`, `web/` — **validate it in a real browser before it can
merge**, using the **artifact-build skill** on the PR branch: build the wasm
bundle, assemble the single-file page, run the headless smoke check, read the
screenshots, and publish/refresh the artifact.

- **A failed smoke check is a red gate.** The unit tests can be green while the
  page won't boot (a wasm-bindgen mismatch, a DOM regression, a blank canvas).
  Fix on the branch and push — do not merge a build you haven't watched run.
- **Hand the artifact URL back alongside the PR URL**, noting which branch the
  snapshot is of, so the user can play the change while CI runs.
- **For key changes, hold the merge for the user's playtest.** A change is
  *key* when its worth can only be judged by feel — game balance, guard
  behaviour, input handling, generation character, anything touching a
  `[SETTLED]` rule or tuning `[START]` numbers. For those, the headless check
  only proves it runs; say you're holding for their verdict on the artifact and
  leave the PR open. Mechanical changes (refactors, scaffolding, pure-core
  logic pinned by tests) don't need to wait.
- Purely internal PRs (docs, CI, sim-only, test-only) skip this step.

## 9. Merge once CI is green

A finished ticket ends **merged**, not just in an open PR. **Do NOT use GitHub
auto-merge** (`enable_pr_auto_merge`) — merge deliberately, after *watching* CI
go green:

- **Watch for CI completion yourself, with basic monitors only.** Do NOT use the
  Claude Code Remote tools (`subscribe_pr_activity`, `send_later`, triggers) —
  see `CLAUDE.md`. Pace the wait with a background timer (Monitor, or a Bash
  `run_in_background` sleep/until-loop — never a foreground sleep) and on each
  wake poll the head commit's check runs via the GitHub MCP tools
  (`pull_request_read` → `get_check_runs`) until every check is completed.
  The raw GitHub API is not reachable from Bash in these sessions — the poll
  itself must go through the MCP tools.
- **On green:** re-check for review comments that arrived meanwhile, then merge
  with `merge_pull_request` (`squash` — this repo's history is one squashed
  commit per issue, e.g. `… (#24)`).
- **On red:** diagnose from the failure logs (`get_job_logs`), fix on the same
  branch, push, and re-arm the watcher for the new run.
- **After the merge, wait for the deploy and hand back the play URL.** The merge
  to `main` kicks the Pages workflow (`pages.yml`); keep the same basic-monitor
  pattern going — poll the workflow's run for `main` (`actions_list` →
  `list_workflow_runs`) until it completes. On success, report the Pages URL
  <https://tk-auto.github.io/intrusion/> as the final deliverable so the user can
  play the merged change immediately; on failure, diagnose and fix rather than
  handing back a dead link. If the deploy is unusually slow, hand back the URL
  with a note that the deploy is still running rather than blocking forever.

**Do NOT merge — leave the PR open and hand it back — when any of these hold:**

- **The user asked to review** it, in this session or the ticket ("let me look
  first", "don't merge yet", "open it for review"). Their word overrides the
  default.
- **Step 8 flagged it as key** — the artifact is published but the user hasn't
  ruled on it yet, or the smoke check is still red.
- **You are unsure.** The change is risky, you couldn't fully verify it (e.g. a
  browser-only render you can't exercise headlessly), it bends a `[SETTLED]` rule,
  it tunes `[START]` numbers you're not confident in, or the ticket was ambiguous.
  When in doubt, surface it (`AskUserQuestion`) — don't merge doubt into `main`.
- **The PR is stacked** on another unmerged PR — merge the base first, then this.

In those cases, report the PR URL, the CI state, and exactly why you're holding.

## Guardrails

- **Don't ship a stub that looks like a feature (§2.3).** If the ticket's system
  can't be made to actually *run* in this PR, narrow the ticket and say so — a
  working half beats an inert whole.
- **Cost is load-bearing (§2.3).** Before adding any ability, answer in the PR:
  what does using it cost, and when would a good player choose not to? No answer →
  don't add it.
- **Watch the §7.6 trap** when touching guard AI: cone-tracks-free + flat range +
  un-outrunnable escape + straight corridors = the un-fun chase. Don't rebuild it.
- Keep the PR scoped to one ticket. New work you discover becomes a new ticket
  (use the create-tickets skill), not scope creep.
