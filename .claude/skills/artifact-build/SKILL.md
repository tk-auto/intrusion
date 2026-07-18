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

Publish `intrusion-build.html` with the **Artifact tool**:

- **Same session:** republish the same file path — the URL stays stable, the
  user just refreshes their tab.
- **New session, artifact already exists:** don't mint a new URL. Find the
  existing one with the Artifact tool's `action: "list"` (title "Intrusion")
  and republish with `url` set to it.
- Keep the favicon **🕹️** on every publish (a changed favicon reads as a
  different page), and pass a short `label` naming the change (e.g.
  `"guard-cone-fix"`) so the version picker stays navigable.

Hand the URL back with one line on what changed in this build.

## Guardrails

- **Never commit build output** — `dist/`, the assembled HTML, and screenshots
  stay in the scratchpad (work-ticket step 6 already forbids committing
  artifacts; this skill produces exactly those).
- **Say what the snapshot is of** — working tree, branch, or PR — when handing
  back the URL, so a stale tab is never mistaken for the merged game.
- The artifact starts private; whether to share it is the user's call.
