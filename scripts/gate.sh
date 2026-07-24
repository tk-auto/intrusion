#!/usr/bin/env bash
# The one quality gate — fmt, then clippy, then test — run as a single fail-fast
# command so local dev and CI (.github/workflows/ci.yml) execute the *identical*
# checks. This is the command half of the #167 story: rust-toolchain.toml pins the
# toolchain so both sides resolve the same compiler; this pins the commands so both
# sides run the same checks. A green run here is a green run in CI.
#
# `set -euo pipefail` and running each check on its own line is deliberate: it makes
# a failing check stop the script with its real exit code, closing the trap that
# masked a red gate as green — piping `cargo fmt --all -- --check` through `tail`
# (or `grep`) and gating an "OK" on the pipeline, whose status is the last stage's,
# not cargo's. Never do that; run the gate through this script.
#
# Local `cargo test` samples generation seeds to stay fast; CI sets
# INTRUSION_SLOW_TESTS=1 around this script to restore the exhaustive §10.6 sweep
# (#60). Same command, the env is the intended local/CI difference.
set -euo pipefail

# Run from the repo root whatever the caller's CWD.
cd "$(dirname "$0")/.."

echo "== gate 1/3: cargo fmt --all -- --check =="
cargo fmt --all -- --check

echo "== gate 2/3: cargo clippy --all-targets --all-features -- -D warnings =="
cargo clippy --all-targets --all-features -- -D warnings

echo "== gate 3/3: cargo test --workspace =="
cargo test --workspace

echo "== gate: all green =="
