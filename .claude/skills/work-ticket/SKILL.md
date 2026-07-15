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

One branch per ticket, off the current default branch:

```
git fetch origin main
git checkout -B <type>/<issue-number>-<slug> origin/main
```

e.g. `feat/12-shadowcast-cone`, `chore/1-scaffold-workspace`. `<type>` matches the
ticket's `type:` label (`feat`, `chore`, `fix`, `tune`).

> If the session was assigned a specific development branch, honour that instead —
> the assignment overrides this convention.

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

Retry on network failure with exponential backoff (2s, 4s, 8s, 16s). Then open a
PR (`create_pull_request`) targeting `main`:

- **Title:** the conventional-commit summary.
- **Body:** what changed and why, the design section(s) honoured, how it was
  verified (paste the gate result), any `[START]` numbers tuned, and `Closes #<n>`.
- If a PR template exists (`.github/pull_request_template.md`), fill its sections.
- Link the PR back on the issue if useful; don't over-comment.

Report the branch, the gate result, and the PR URL to the user. Offer to watch the
PR for CI/review activity (`subscribe_pr_activity`) rather than polling.

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
