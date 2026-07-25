---
name: list-issues
description: >-
  List the Intrusion repo's GitHub issues — open, closed, or both — as a compact
  index without drowning in the large bodies the API returns. Use when the user
  wants to see the issues, the backlog, what's open or closed, what's left to do,
  what shipped, or asks "list the issues", "show the tickets", "what's still open".
  Filters by state/label, pages in small batches, and presents number/title/state/
  labels — only deep-reading a single issue's full body on demand.
---

# List issues

Hand back a readable index of the repo's GitHub issues. The point of this skill
is **restraint**: the issue API returns full bodies, and this repo's tickets use
the long body template from the create-tickets skill (summary, design reference,
acceptance criteria, notes). Listing thirty of those in full buries the answer
and burns the context window. So list *lean by default*, and fetch a full body
only when the user asks about one issue.

GitHub state is only reachable through the GitHub MCP tools — there is no `gh`
CLI and `api.github.com` is blocked from Bash (see CLAUDE.md). Repo is
`tk-auto/intrusion`.

## Which state does the user want?

Map the request to the `state` filter on `list_issues`:

- **Open** (the default when unsure) — "what's left", "the backlog", "what's
  open" → `state: OPEN`.
- **Closed** — "what shipped", "what's done", "closed tickets" → `state: CLOSED`.
- **Both** — "all the issues", "everything" → omit `state` (returns both).

If it's genuinely ambiguous, default to **open** and say so in one line rather
than asking.

## The large-content problem, and how to dodge it

`list_issues` returns each issue's **full body**, and `minimal_output: true`
does **not** drop it (it trims some author/label metadata but keeps the whole
body) — so it is *not* a size lever here; don't reach for it expecting relief.
This repo's bodies are long enough that a wide page blows the tool-result token
limit and the harness **spills the whole payload to a file** instead of
returning it (a 40-issue page is ~150 KB). Don't fetch a wide page and echo it
back. Instead:

1. **Filter before you fetch.** Narrow the set with `state` and, when the user
   named an area/type/milestone, `labels` (e.g. `["milestone:v1"]`,
   `["area:guards"]`). A smaller result set is the cheapest win.
2. **Page in small batches.** Set `perPage` to **10** (not the 100 max). If there
   are more, use `pageInfo.endCursor` from the response as the `after` parameter
   on the next call. Stop once you have what the user asked for — don't page to
   the end reflexively.
3. **If a call spills to a file anyway, don't read it back into context —
   extract the index with `jq`.** Even a small page can spill when the bodies are
   large. The saved file is one JSON object keyed `issues`, each issue carrying
   `number`, `title`, `state`, and `labels` (an array of **plain strings**, not
   objects). Turn it straight into the compact index:

   ```
   jq -r '.issues[] | "#\(.number)\t[\(.labels | join(","))]\t\(.title)"' <saved-file>
   ```

   (`.totalCount` and `.pageInfo.endCursor` are in the same object if you need
   the count or the next cursor.) This keeps the 150 KB of bodies out of the
   context window entirely — only the one-line-per-issue index comes back.
4. **Present a compact index, not the bodies.** For each issue show only:

   ```
   #<num>  <title>   [<state>]   <labels>
   ```

   Group by milestone label when there is more than a handful. Do **not** paste
   the summary/acceptance-criteria bodies into the reply — they are noise at the
   index level.
5. **Deep-read one issue on demand.** When the user wants the detail of a
   specific ticket ("what's in #12?"), call `issue_read` with `method: get` for
   that one issue, and `method: get_comments` (paginated, `perPage: 10`) if they
   want the discussion. That is the only time a full body belongs in the reply.

## Targeted lookups: use search instead of list

When the user is hunting for issues *about* something ("issues mentioning
shadowcast", "anything about guard radio") rather than browsing by state, reach
for `search_issues` — it is already scoped to `is:issue`, matches semantically,
and pages with `page`. It still returns bodies, so apply the same restraint:
report the matches as a compact index, deep-read only on request.

## Counting

If the user only wants a **count** ("how many are open?"), read `totalCount`
from the response — it is the full count for the filter, so a single call with a
small `perPage` answers it; no need to page to the end and tally. Report the
number, and offer the compact index as a follow-up rather than dumping it
unprompted.

## Output shape

Lead with the direct answer (the count or the index), grouped by milestone when
it helps. Keep it to titles, numbers, states, and labels. Offer to open any one
ticket in full as the next step — that is where the body goes, one issue at a
time.
