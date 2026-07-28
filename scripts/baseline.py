#!/usr/bin/env python3
"""Check — or refresh — the committed simbot baseline (§13.2, #140).

`.claude/skills/playtest/baseline.json` is the snapshot the playtest skill diffs
a batch against. It is *supposed* to drift: any change that moves the numbers (a
`[START]` knob, guard/vision/ability behaviour, generation, the bot policy, a
profile's temperament) is expected to refresh it in the same PR. What must never
happen is drifting **silently** — a stale baseline compared without anyone
noticing is the exact anti-pattern the file exists to prevent, and it is
invisible precisely because nothing re-runs the snapshot.

So this script re-runs it. For each profile block it executes that block's own
recorded `command` — the file says "this command produced these numbers", and the
check takes it at its word, so the command can never quietly stop being the one
that was measured — parses the summary line, and compares it field by field:

    ./scripts/baseline.py             # check: exit 1 if the snapshot has drifted
    ./scripts/baseline.py --refresh   # re-run and rewrite the snapshot in place
    ./scripts/baseline.py --refresh --at search-duration-12   # ...naming the work

CI runs the check on every PR (`.github/workflows/ci.yml`), which turns "you
forgot to refresh the baseline" from something a reader has to notice into a red
tick. The fix for a red run is never to hand-edit the JSON: run `--refresh` and
commit the result, which also keeps the file's shape exactly as written here.

Exact equality is the comparison, because the sim is deterministic per
`(seed, profile)` (`crates/sim/README.md`) — a single moved digit is a real
behaviour change, and deciding *which* changes are big enough to matter is the
human judgement the playtest skill owns (§13.4), not something a gate should
guess at with a tolerance.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BASELINE = ROOT / ".claude" / "skills" / "playtest" / "baseline.json"
# Relative, so the messages read as something to type from the repo root.
BASELINE_REL = BASELINE.relative_to(ROOT)
REFRESH_HINT = (
    f"Refresh it in this PR:  ./scripts/baseline.py --refresh\n"
    f"then commit {BASELINE_REL}."
)


class Failure(Exception):
    """A problem that stops the run — a bad file, or a batch that would not run."""


def load_baseline() -> dict:
    try:
        snapshot = json.loads(BASELINE.read_text())
    except FileNotFoundError as error:
        raise Failure(f"no baseline at {BASELINE_REL}") from error
    except json.JSONDecodeError as error:
        raise Failure(f"{BASELINE_REL} is not valid JSON: {error}") from error
    if not snapshot.get("profiles"):
        raise Failure(f"{BASELINE_REL} has no profile blocks to check")
    return snapshot


def build() -> None:
    """Build the sim once, released.

    The per-profile commands are `cargo run --release`, so this is not strictly
    needed — but doing it up front means a compile error fails as a compile
    error, instead of surfacing as an unparseable batch three profiles later.
    """
    print("== building the sim (release) ==", flush=True)
    build = subprocess.run(
        ["cargo", "build", "--release", "-p", "intrusion-sim"], cwd=ROOT, check=False
    )
    if build.returncode != 0:
        raise Failure("the sim does not build")


def run_batch(name: str, block: dict, expected_runs: int | None) -> dict:
    """Run one profile's recorded command and return the summary it printed."""
    command = block.get("command")
    if not command:
        raise Failure(f"profile {name!r} has no command to run")
    print(f"== simbot baseline: {name} ==\n   {command}", flush=True)
    batch = subprocess.run(
        command, cwd=ROOT, shell=True, capture_output=True, text=True, check=False
    )
    if batch.returncode != 0:
        raise Failure(
            f"profile {name!r}: the batch failed (exit {batch.returncode})\n"
            f"{batch.stderr.strip()}"
        )
    lines = [line for line in batch.stdout.splitlines() if line.strip()]
    if not lines:
        raise Failure(f"profile {name!r}: the batch printed nothing")
    try:
        summary = json.loads(lines[-1])["summary"]
    except (json.JSONDecodeError, KeyError, TypeError) as error:
        raise Failure(
            f"profile {name!r}: the last line is not a summary row — the output "
            f"schema moved out from under this script (crates/sim/README.md):\n"
            f"{lines[-1][:200]}"
        ) from error

    # Two cheap consistency checks on the *file*, not on the game: they catch a
    # command edited out of step with the block it sits in, which would otherwise
    # compare a batch against a snapshot of something else entirely (§13.2's
    # attribution rule — a row must never claim a config it did not run).
    if summary.get("profile") != name:
        raise Failure(
            f"profile {name!r}: its command played {summary.get('profile')!r} — "
            f"the block key and its command disagree about which temperament "
            f"this is"
        )
    if expected_runs is not None and summary.get("runs") != expected_runs:
        raise Failure(
            f"profile {name!r}: its command ran {summary.get('runs')} runs, but "
            f"config.runs says {expected_runs}"
        )
    return summary


def flatten(summary: dict) -> dict:
    """`{"usage": {"wait": 9}}` → `{"usage.wait": 9}`, so a diff names one metric."""
    flat = {}
    for key, value in summary.items():
        if isinstance(value, dict):
            flat.update({f"{key}.{inner}": v for inner, v in value.items()})
        else:
            flat[key] = value
    return flat


def drift(committed: dict, fresh: dict) -> list[str]:
    """Every field the two summaries disagree on, as one readable line each."""
    was, now = flatten(committed), flatten(fresh)
    lines = []
    for field in list(was) + [f for f in now if f not in was]:
        before, after = was.get(field, "(absent)"), now.get(field, "(absent)")
        if before != after:
            lines.append(f"{field:<26} {before}  ->  {after}")
    return lines


def captured_at(label: str | None) -> str:
    """The `captured_at_commit` a fresh snapshot records.

    A refresh happens *alongside* the change that moved the numbers, so the
    commit those numbers came from does not exist yet — HEAD is its parent. The
    file's existing convention handles that honestly with a hand-written suffix
    naming the pending work (`44d5e28+search-duration-12`), which is what `--at`
    is for. Bare HEAD is the default because it is right whenever the refresh
    follows the change rather than accompanying it.
    """
    head = (
        subprocess.run(
            ["git", "rev-parse", "--short", "HEAD"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
        ).stdout.strip()
        or "unknown"
    )
    return f"{head}+{label}" if label else head


def refresh(snapshot: dict, fresh: dict[str, dict], label: str | None) -> None:
    """Rewrite the snapshot in place, every profile block together.

    All the blocks at once, never one: a file where one temperament is current
    and the others are stale is worse than one that is uniformly old, because
    only the uniformly old one can be read as a set.
    """
    for name, summary in fresh.items():
        snapshot["profiles"][name]["summary"] = summary
    snapshot["captured_at_commit"] = captured_at(label)
    # `ensure_ascii=False` so the prose keeps its `§` and `—` as themselves rather
    # than being escaped into `§` the first time a refresh touches the file.
    BASELINE.write_text(json.dumps(snapshot, indent=2, ensure_ascii=False) + "\n")


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Check that the committed simbot baseline has not drifted.",
    )
    parser.add_argument(
        "--refresh",
        action="store_true",
        help="rewrite the baseline with what the sim produces now, instead of "
        "failing on the difference",
    )
    parser.add_argument(
        "--at",
        metavar="LABEL",
        help="suffix the recorded captured_at_commit, naming the uncommitted work "
        "the numbers came from (e.g. --at search-duration-12 records "
        "`44d5e28+search-duration-12`)",
    )
    args = parser.parse_args()
    if args.at and not args.refresh:
        parser.error("--at only means something with --refresh")

    try:
        snapshot = load_baseline()
        build()
        expected_runs = (snapshot.get("config") or {}).get("runs")
        fresh = {
            name: run_batch(name, block, expected_runs)
            for name, block in snapshot["profiles"].items()
        }
    except Failure as failure:
        print(f"\nbaseline: {failure}", file=sys.stderr)
        return 2

    drifted = {
        name: lines
        for name, summary in fresh.items()
        if (lines := drift(snapshot["profiles"][name]["summary"], summary))
    }

    if args.refresh:
        if not drifted:
            # Nothing moved, so nothing is written — not even the commit label. A
            # refresh that did not have to change anything should leave no diff to
            # review, and re-stamping the file would claim a re-capture that says
            # nothing the old stamp did not already say.
            print(f"\nbaseline: already current — {BASELINE_REL} left untouched")
            return 0
        refresh(snapshot, fresh, args.at)
        stamp = snapshot["captured_at_commit"]
        print(f"\nbaseline: refreshed {BASELINE_REL} at {stamp}")
        for name, lines in drifted.items():
            print(f"\n  {name}:")
            print("\n".join(f"    {line}" for line in lines))
        print("\nCommit it with the change that moved the numbers.")
        return 0

    if not drifted:
        print(f"\nbaseline: all {len(fresh)} profiles match {BASELINE_REL}")
        return 0

    print(
        f"\nbaseline: DRIFTED — the sim no longer produces what {BASELINE_REL} "
        f"records.\n",
        file=sys.stderr,
    )
    for name, lines in drifted.items():
        print(f"  {name}:", file=sys.stderr)
        print("\n".join(f"    {line}" for line in lines), file=sys.stderr)
        print(file=sys.stderr)
    print(
        "This is not automatically a bug: the baseline is meant to move when the\n"
        "game does. It is a bug when nobody looked. Read the deltas — if the\n"
        "change was expected to move them, they are the signal, not the problem.\n"
        f"{REFRESH_HINT}",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    sys.exit(main())
