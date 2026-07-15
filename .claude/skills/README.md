# Project skills

Claude Code skills for building **Intrusion** (see `docs/design.md`). Invoke with a
slash command (e.g. `/create-tickets`) or by describing the task.

| Skill | Purpose | Status |
|---|---|---|
| [`create-tickets`](create-tickets/SKILL.md) | Turn `docs/design.md` into GitHub issues — drafts for review, then creates them with v1/v2/v3 milestones and area/type/size labels. | Ready |
| [`work-ticket`](work-ticket/SKILL.md) | Pick an open issue and implement it end-to-end: branch-per-ticket, unit tests, the fmt/clippy/test gate, commit conventions, and a PR that closes the issue. | Ready |
| [`playtest`](playtest/SKILL.md) | Playtest via the headless sim (§13.2) and report balance metrics. | **Incomplete** — waits on `crates/sim`. |

## The intended loop

1. `/create-tickets` — break the next roadmap slice into issues (the first is always
   "scaffold the cargo workspace").
2. `/work-ticket` — pick one, build it, ship a PR.
3. `/playtest` — once the sim exists, let a bot flag suspicious seeds for a human to
   play and rule on (§13.1, §13.3).

Conventions live inside each skill: ticket taxonomy in `create-tickets`, the branch/
test/lint/commit/PR rules in `work-ticket`.
