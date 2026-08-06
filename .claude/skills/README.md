# Project skills

Claude Code skills for building **Intrusion** (see `docs/design.md`). Invoke with a
slash command (e.g. `/create-tickets`) or by describing the task.

| Skill | Purpose | Status |
|---|---|---|
| [`create-tickets`](create-tickets/SKILL.md) | Turn `docs/design.md` into GitHub issues — drafts for review, then creates them with v1/v2/v3 milestones and area/type/size labels. | Ready |
| [`work-ticket`](work-ticket/SKILL.md) | Pick an open issue and implement it end-to-end: branch-per-ticket, unit tests, the fmt/clippy/test gate, commit conventions, and a PR that closes the issue. | Ready |
| [`survey`](survey/SKILL.md) | Quick code-health survey — files/functions too large or complex, muddy naming, modules that lost their focus, duplication, stale comments, convention drift, and bloat in the always-loaded docs. Ranks the top few, asks how many to fix, then works them one at a time (ticket → code → PR → merge on green). | Ready |
| [`artifact-build`](artifact-build/SKILL.md) | Build the wasm bundle locally, pack it into one self-contained HTML page, smoke-verify it headlessly, and publish it as a Claude Artifact — a playable preview at a stable URL, no Pages deploy needed. | Ready |
| [`playtest`](playtest/SKILL.md) | Run the headless sim (§13.2) over a batch of seeds, report the balance metrics against a stored baseline, and flag suspicious seeds to play. | Ready |
| [`list-issues`](list-issues/SKILL.md) | List the repo's issues — open, closed, or both — as a compact index (number/title/state/labels), filtering by state/label and paging in small batches so the large issue bodies don't swamp the reply. Deep-reads one issue on demand. | Ready |

## The intended loop

1. `/create-tickets` — break the next roadmap slice into issues. (`/survey` comes
   at the work from the other direction: it reads the *code* for cleanup
   opportunities — oversized files, muddy naming, drift — and the always-loaded
   docs for bloat, then asks how many findings to fix and drives each chosen one
   through ticket → `/work-ticket` → merge, filing its ticket only when that fix
   starts.)
2. `/work-ticket` — pick one, brief what it asks and how you're closing whatever
   it left open, build it, ship a PR. Player-visible PRs get an
   `/artifact-build` preview before merge; key (feel/balance) changes hold for
   the user's playtest on it.
3. `/playtest` — let the headless sim (§13.2) flag suspicious seeds for a human to
   play and rule on (§13.1, §13.3).

Conventions live inside each skill: ticket taxonomy in `create-tickets`, the branch/
test/lint/commit/PR rules in `work-ticket`.
