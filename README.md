# Intrusion

A turn-based stealth roguelike that renders as a glyph grid and ships as a
static web page. Design lives in [`docs/design.md`](docs/design.md).

**Play (published build):** https://tk-auto.github.io/intrusion/

The site is built from the `main` branch and deployed to GitHub Pages on every
push (see [`.github/workflows/pages.yml`](.github/workflows/pages.yml)). It is a
fully static page — no server, no runtime dependency but a browser (§3).

## Layout

```
crates/
  core/    pure, deterministic game logic — no wasm, no I/O. Fast native tests.
  web/     thin wasm-bindgen + canvas2d shell: draws core state, feeds it input.
  sim/     headless harness for seeded playtest metrics (§13).
web/
  index.html + assets — the static shell the wasm module mounts into.
```

## Build and run locally

Native gate (what CI enforces):

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace
```

Build the web bundle and serve it (needs the wasm target and a matching
`wasm-bindgen` CLI):

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.100   # must match the pinned crate

cargo build -p intrusion-web --release --target wasm32-unknown-unknown
wasm-bindgen target/wasm32-unknown-unknown/release/intrusion_web.wasm \
  --out-dir dist --target web --no-typescript
cp web/index.html dist/

python3 -m http.server -d dist 8099   # then open http://localhost:8099/
```

Right now this draws the current slice: the facility as a walled rectangle. The
generator, guards, vision and input land in their own tickets.
