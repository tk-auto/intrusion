#!/usr/bin/env bash
# Build the static web bundle and publish it to itch.io through butler.
#
# The game ships as a static page with no server (§3), which is exactly what
# itch.io's HTML5 channel wants — so publishing there is the Pages build plus one
# upload. This script deliberately assembles the site the *same* way
# `.github/workflows/pages.yml` does (same cargo invocation, same wasm-bindgen
# call, same files copied into the output directory): itch and Pages must serve
# the same build, and the only way to keep that true is for the two pipelines to
# read as one.
#
#     scripts/publish-itch.sh [--dry-run] [--out DIR] [--target USER/GAME:CHANNEL]
#
# butler is itch.io's upload tool: it diffs against what is already on the
# channel and pushes only the changed blocks, so re-publishing an unchanged build
# is nearly free. Install it from <https://itch.io/docs/butler/> and authenticate
# once with `butler login` (or set BUTLER_API_KEY, which is what CI would use).
#
# This script does **not** run the quality gate — run `scripts/gate.sh` first, or
# publish from a commit that has already been through CI. It builds what is in
# the working tree, dirty or not, and says so before pushing.
set -euo pipefail

# Run from the repo root whatever the caller's CWD.
cd "$(dirname "$0")/.."

# The itch.io project and channel. `web` is the HTML5 channel: itch serves a
# push to it as a playable-in-browser build, which is the only distribution
# shape this game has.
ITCH_TARGET="${ITCH_TARGET:-thunderk/intrusion:web}"
OUT_DIR="${OUT_DIR:-dist}"
DRY_RUN=0

usage() {
    cat <<'EOF'
Build Intrusion's static web bundle and publish it to itch.io.

    scripts/publish-itch.sh [options]

Options:
    --dry-run                  Build and assemble the site, then stop before pushing.
    --out DIR                  Output directory for the assembled site (default: dist).
    --target USER/GAME:CHANNEL Butler target (default: thunderk/intrusion:web).
    -h, --help                 Show this help.

Environment: ITCH_TARGET and OUT_DIR set the same two values; BUTLER_API_KEY
authenticates butler non-interactively (otherwise run `butler login` once).
EOF
}

while [ $# -gt 0 ]; do
    case "$1" in
        --dry-run) DRY_RUN=1; shift ;;
        --out) OUT_DIR="${2:?--out needs a directory}"; shift 2 ;;
        --target) ITCH_TARGET="${2:?--target needs a USER/GAME:CHANNEL}"; shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) echo "publish-itch: unknown argument '$1'" >&2; usage >&2; exit 2 ;;
    esac
done

# ---------------------------------------------------------------------------
# Preflight — every check that can fail is made *before* the release build, so a
# missing tool costs a second rather than a few minutes of compiling.
# ---------------------------------------------------------------------------

if [ "$DRY_RUN" -eq 0 ] && ! command -v butler >/dev/null 2>&1; then
    echo "publish-itch: butler not found on PATH." >&2
    echo "  Install it from https://itch.io/docs/butler/ and run 'butler login'." >&2
    exit 1
fi

# Resolve the wasm-bindgen CLI. `cargo install` drops it in $CARGO_HOME/bin
# (default ~/.cargo/bin), a directory only rustup's own installer adds to PATH —
# with a distro-packaged cargo a perfectly successful install still looks missing.
# So fall back to the install root before declaring it absent.
CARGO_BIN="${CARGO_HOME:-$HOME/.cargo}/bin"
if command -v wasm-bindgen >/dev/null 2>&1; then
    WASM_BINDGEN=$(command -v wasm-bindgen)
elif [ -x "$CARGO_BIN/wasm-bindgen" ]; then
    WASM_BINDGEN="$CARGO_BIN/wasm-bindgen"
    echo "publish-itch: wasm-bindgen is not on PATH; using $WASM_BINDGEN."
    echo "  Add it permanently with: . \"\$HOME/.cargo/env\"  (in ~/.bashrc or ~/.zshrc)"
else
    echo "publish-itch: wasm-bindgen CLI not found on PATH or in $CARGO_BIN." >&2
    echo "  cargo install wasm-bindgen-cli --version <the pin in crates/web/Cargo.toml> --locked" >&2
    echo "  (the crate is wasm-bindgen-cli; the binary it installs is called wasm-bindgen)" >&2
    exit 1
fi

# The CLI and the crate must agree exactly or the generated glue is invalid, so
# read the pin out of the manifest rather than repeating the version here — one
# place to bump when it moves (the same reason rust-toolchain.toml exists, #167).
WASM_BINDGEN_PIN=$(sed -n 's/^wasm-bindgen = "=\([0-9.]*\)"$/\1/p' crates/web/Cargo.toml)
if [ -z "$WASM_BINDGEN_PIN" ]; then
    echo "publish-itch: could not read the wasm-bindgen pin from crates/web/Cargo.toml." >&2
    echo "  The dependency line's shape changed; update this script to match." >&2
    exit 1
fi
WASM_BINDGEN_HAVE=$("$WASM_BINDGEN" --version | awk '{print $2}')
if [ "$WASM_BINDGEN_HAVE" != "$WASM_BINDGEN_PIN" ]; then
    echo "publish-itch: wasm-bindgen CLI is $WASM_BINDGEN_HAVE but the crate is pinned to $WASM_BINDGEN_PIN." >&2
    echo "  cargo install wasm-bindgen-cli --version $WASM_BINDGEN_PIN --locked" >&2
    exit 1
fi

# A dirty tree is allowed — publishing a work-in-progress build is a legitimate
# thing to want — but never silently: what goes to itch should be identifiable.
VERSION=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)
USERVERSION="$VERSION"
if git rev-parse --git-dir >/dev/null 2>&1; then
    USERVERSION="$VERSION+$(git rev-parse --short HEAD)"
    if [ -n "$(git status --porcelain)" ]; then
        USERVERSION="$USERVERSION-dirty"
        echo "publish-itch: working tree is dirty; publishing it as $USERVERSION."
    fi
fi

# ---------------------------------------------------------------------------
# Build — the pipeline from pages.yml, verbatim.
# ---------------------------------------------------------------------------

echo "== publish 1/3: cargo build -p intrusion-web --release --target wasm32-unknown-unknown =="
cargo build -p intrusion-web --release --target wasm32-unknown-unknown

echo "== publish 2/3: assemble the site into $OUT_DIR =="
rm -rf "$OUT_DIR" && mkdir -p "$OUT_DIR"
"$WASM_BINDGEN" target/wasm32-unknown-unknown/release/intrusion_web.wasm \
    --out-dir "$OUT_DIR" --target web --no-typescript
cp web/index.html "$OUT_DIR/"
# Ship any static assets alongside (font, images) if present.
if [ -d web/assets ]; then cp -r web/assets "$OUT_DIR/assets"; fi

# Hand butler an absolute path. A relative `dist` resolves against whatever
# directory the command runs in, and the default one is relative to the repo
# root this script cd'd to — so the printed command below is copy-pasteable from
# anywhere, and butler can never be pointed at some other directory's `dist`.
OUT_ABS=$(cd "$OUT_DIR" && pwd)

if [ "$DRY_RUN" -eq 1 ]; then
    echo "== publish 3/3: --dry-run, not pushing =="
    echo "   Would run: butler push $OUT_ABS $ITCH_TARGET --userversion $USERVERSION"
    echo "   Serve it locally with: python3 -m http.server -d $OUT_ABS 8099"
    exit 0
fi

# ---------------------------------------------------------------------------
# Publish.
# ---------------------------------------------------------------------------

echo "== publish 3/3: butler push $OUT_ABS $ITCH_TARGET =="
butler push "$OUT_ABS" "$ITCH_TARGET" --userversion "$USERVERSION"

echo "== publish: pushed $USERVERSION to $ITCH_TARGET =="
echo "   itch.io needs the project's 'This file will be played in the browser'"
echo "   box ticked on the uploaded build for the page to embed it."
