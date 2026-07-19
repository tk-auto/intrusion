# CLAUDE.md — session conventions for this repo

## Language

Instructions may be given in French. Regardless of the language of the
request, all replies, commit messages, code, comments, and other written
work must be in English.

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
