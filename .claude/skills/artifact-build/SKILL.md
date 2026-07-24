---
name: artifact-build
description: >-
  Build Intrusion's wasm bundle locally, pack it into a single self-contained
  HTML page, smoke-verify it headlessly, and publish it as a Claude Artifact the
  user can play immediately — no waiting for a merge or the Pages deploy. Use
  when the user wants an artifact build, a preview build, to "refresh the
  artifact", or to test a change in the browser while iterating; also invoked by
  the work-ticket skill to validate player-visible PRs before merge.
---

# Artifact preview build

Produce a playable, single-file build of the current working tree and publish it
as a private Claude Artifact. This is the fast inner loop: seconds after a code
change, the user refreshes one stable URL and plays it. The canonical build is
still the Pages deploy from `main` (<https://tk-auto.github.io/intrusion/>) —
the artifact is a snapshot for iteration, never a substitute for the deploy.

## 1. Toolchain (once per container)

Two things beyond the native gate's toolchain, both matching the pins in
`pages.yml` / `crates/web/Cargo.toml`:

```
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.100 --locked
```

The CLI **must** be exactly the version the `wasm-bindgen` crate is pinned to
(`=0.2.100` — if the pin ever moves, install the matching CLI). The
`cargo install` takes a few minutes; run it in the background. Don't try to
download the prebuilt binary from GitHub releases — release downloads are
blocked in remote sessions (only the session's repos are reachable), so
crates.io is the path that works.

## 2. Build and generate the glue

Same pipeline as `pages.yml`, run locally (from the repo root; `$SCRATCH` is the
session scratchpad directory):

```
cargo build -p intrusion-web --release --target wasm32-unknown-unknown
rm -rf "$SCRATCH/dist" && mkdir -p "$SCRATCH/dist"
wasm-bindgen target/wasm32-unknown-unknown/release/intrusion_web.wasm \
  --out-dir "$SCRATCH/dist" --target web --no-typescript
```

## 3. Assemble the single-file page

Artifacts run under a strict CSP: no external requests, so the page can't fetch
`intrusion_web_bg.wasm` — everything must live in one HTML file. The
`assemble.py` script next to this skill does the packing:

```
python3 .claude/skills/artifact-build/assemble.py \
  --dist "$SCRATCH/dist" --index web/index.html \
  --out "$SCRATCH/intrusion-build.html"
```

What it does (so you can fix it if the glue's shape changes): inlines the
wasm-bindgen ES-module glue into the page's `<script type="module">` (stripping
the `export` statements), embeds the `.wasm` as base64 and passes the decoded
buffer to `__wbg_init({ module_or_path: ... })` so no fetch happens, and strips
the `<!doctype>`/`<html>`/`<head>`/`<body>` skeleton because the Artifact host
wraps the content itself. It fails loudly if any expected anchor is missing —
treat that as the glue format having drifted, and update the script.

## 4. Smoke-verify before publishing — not optional

Never publish a build you haven't watched boot. The `verify.mjs` script next to
this skill loads the assembled page in headless Chromium, fails on any page
error or missing/blank canvas, presses the arrow keys, and asserts the frame
changed (the `@` moved):

```
node .claude/skills/artifact-build/verify.mjs \
  "$SCRATCH/intrusion-build.html" "$SCRATCH/shots"
```

It writes `boot.png` and `after-input.png` into the shots directory — **Read
both screenshots** and confirm the facility actually renders sensibly (glyph
grid visible, colours right, player present). A green exit code plus your own
eyes on the screenshots is the bar. In remote sessions Chromium lives at
`/opt/pw-browsers/chromium` and Playwright at `$(npm root -g)/playwright`; the
script defaults to those and both can be overridden via `CHROMIUM_PATH` /
`PLAYWRIGHT_MODULE` env vars.

## 5. Publish (or refresh) the artifact

Publish `intrusion-build.html` with the **Artifact tool**. **One artifact per
ticket/PR — there is no shared "Intrusion" URL.** Two sessions previewing
different tickets must never publish onto the same artifact: they would clobber
each other, and a reviewer would refresh their tab and see the wrong ticket's
build. Key the artifact to *your* ticket, not to the game:

- **Title it by ticket** — e.g. `Intrusion — #181` — so it is identifiable in
  the list and on the PR.
- **Same session:** republish the same file path — the URL stays stable, the
  user just refreshes their tab. `force: true` is correct here: a later build of
  *your* ticket supersedes its own earlier one.
- **New session, same ticket:** find *that ticket's* artifact with the Artifact
  tool's `action: "list"` (match the ticket in the title), and republish with
  `url` set to it (`force: true`).
- **Never publish onto another ticket's artifact**, and never mint a second URL
  for your own — exactly one per ticket. If none of the conflict-avoidance above
  applies, mint a fresh one for this ticket.
- **At merge the preview is spent** — the Pages deploy becomes canonical, and
  work-ticket step 9 watches the main build to confirm it is clean. Just stop
  refreshing the artifact; don't try to tombstone it (the Artifact tool has no
  delete action, and a tombstone republish is more trouble than it's worth). The
  "say what the snapshot is of" guardrail below already keeps a stale tab from
  being mistaken for the merged game.
- Keep the favicon **🕹️** on every publish (a changed favicon reads as a
  different page), and pass a short `label` naming the change (e.g.
  `"guard-cone-fix"`) so the version picker stays navigable.

Hand the URL back with one line on what changed and which branch/PR it snapshots.

## 6. Hand off a specific seed (§13.1/#110)

The shell and the headless sim (§13.2) boot the **identical** path, so a given seed
reproduces the **same facility the bot played** — how a playtest seed (`sim --bot`
numbers a batch `S, S+1, …`; `/playtest` flags suspicious ones) becomes a level the
user can play by hand. There are two seeding channels, split by where the build runs.

**A Claude Artifact → bake the seed into the build.** The Artifact host strips a
`…#seed=N` hash before the framed page ever sees it, so a shared *link* cannot seed
an artifact. Instead pass the seed to `assemble.py`, which stamps a
`window.__intrusionSeed` global the shell reads ahead of the URL and clock — the
page then boots that exact facility with no URL and no typing:

```
python3 .claude/skills/artifact-build/assemble.py \
  --dist "$SCRATCH/dist" --index web/index.html \
  --out "$SCRATCH/intrusion-8371.html" --seed 8371
```

Smoke-verify and publish as usual (§§4–5). This is what you hand back when the ask
is "let me play the seed the bot flagged" — **one artifact per seed**: publish the
seed-locked build to *its own* URL (title it `Intrusion — #NNN seed 8371`), rather
than overwriting the ticket's main preview, so both stay reachable. Say plainly in
the handoff which seed it is locked to.

**The Pages deploy → a `?seed=N` / `#seed=N` URL.** The canonical
`ci`/`pages` build has no baked seed, and there the URL is the real document URL, so
`https://tk-auto.github.io/intrusion/?seed=8371` (or `#seed=8371`) boots that level.
Hand this form over only for the live Pages URL, never for an artifact.

> **The on-page seed box is hidden for now** (it sat over the board's top-left). The
> wiring is intact behind one CSS rule in `web/index.html`, so seeds currently come
> from the build (`--seed`) or the URL, not a box. If you re-enable it, the box
> loads any seed live and this section's "type it in" path returns.

## Guardrails

- **Never commit build output** — `dist/`, the assembled HTML, and screenshots
  stay in the scratchpad (work-ticket step 6 already forbids committing
  artifacts; this skill produces exactly those).
- **Say what the snapshot is of** — working tree, branch, or PR — when handing
  back the URL, so a stale tab is never mistaken for the merged game.
- The artifact starts private; whether to share it is the user's call.
