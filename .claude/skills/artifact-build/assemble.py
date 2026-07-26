#!/usr/bin/env python3
"""Pack the wasm-bindgen output and web/index.html into one self-contained page.

Artifacts run under a CSP that blocks all external requests, so the page cannot
fetch its .wasm: the module glue is inlined into the page's script tag and the
wasm binary is embedded as base64 and handed to __wbg_init as a buffer. The
Artifact host wraps content in its own doctype/head/body skeleton, so those
tags are stripped from the output.

Every transform asserts its anchor was actually found — a silent no-op here
would publish a broken page, so drift in the glue's shape (a wasm-bindgen
version bump changing its export lines) fails the build instead.
"""

import argparse
import base64
import json
import pathlib
import re
import sys


def replace_once(text: str, old: str, new: str, what: str) -> str:
    if text.count(old) != 1:
        sys.exit(f"assemble: expected exactly one occurrence of {what!r} "
                 f"({old!r}), found {text.count(old)} — glue format drifted?")
    return text.replace(old, new)


# The debug switches the shell knows (crates/web/src/debug.rs) — the flag names it
# parses out of `window.__intrusionDebug`. Kept here so a typo fails the build where
# it is typed, rather than silently doing nothing in the browser; add a name here the
# day the shell learns one.
DEBUG_FLAGS = ("reveal",)


def valid_level_seed(token: str) -> bool:
    """Whether `token` is a plausible level-seed string (crates/core/src/level_seed.rs).

    Two shapes, matching `LevelSeed::encode`: a bare decimal u64 (quick play), or a
    versioned `L1-…` token carrying a modifier set and loadout. This is a *shape*
    guard so a typo fails the build loudly instead of silently baking a token the
    shell can't decode (which would fall through to a random-seed page — the opposite
    of seed-locked). The core is the real validator; the split here is deliberately
    light so it cannot drift from `LevelSeed::decode`."""
    if token.isdigit():
        return 0 <= int(token) < 2**64
    # A versioned token: `L<version>-…`. Leave the field-level validation to the core.
    return bool(re.fullmatch(r"L\d+-[0-9A-Za-z-]+", token))


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--dist", required=True,
                    help="wasm-bindgen --target web output dir")
    ap.add_argument("--index", required=True, help="path to web/index.html")
    ap.add_argument("--out", required=True, help="output HTML path")
    ap.add_argument("--seed", default=None,
                    help="bake a fixed level into the page for live play. The build "
                         "boots this level with no URL and no typing — how a "
                         "seed-locked artifact pins the exact level the sim played "
                         "(#110). The value is a level-seed string (#245): a bare "
                         "u64 boots quick play (#244), or a full `L1-<seed>-<mods>-"
                         "<abils>` token carries a chosen modifier set and ability "
                         "loadout (e.g. `L1-8371-a-x` — cones shown, Dephase only). "
                         "Omit for the normal random-seed build. For a *replay* "
                         "build (boots the scrub viewer) use --replay-json instead.")
    ap.add_argument("--replay-json", default=None,
                    help="bake a captured replay into the page: a path (or '-' for "
                         "stdin) to the `{\"seed\":S,\"inputs\":\"...\"}` line that "
                         "`sim --bot --emit-replay` prints (#197). `S` is a "
                         "level-seed string (#245) — a bare seed or an `L1-…` token "
                         "carrying the captured preset — baked verbatim so the "
                         "playback boots the exact run, its modifiers and loadout "
                         "included. The build boots straight into the replay viewer "
                         "at K=0. It carries its own level, so do not also pass "
                         "--seed.")
    ap.add_argument("--debug", default=None, metavar="FLAGS",
                    help="bake playtest-only DEBUG switches into the page: a "
                         "comma-separated list from " + ", ".join(DEBUG_FLAGS) +
                         " (§12.6). `reveal` makes the player's field of view the "
                         "whole facility: the fog lifts into the ordinary live "
                         "picture, every guard draws, and the danger overlay paints "
                         "every cone — so a playtest can watch patrols it has not "
                         "met. These change only what the PLAYER PERCEIVES: guards "
                         "look with their own cones, so the run plays identically. "
                         "They are "
                         "deliberately not part of a level-seed string, and there "
                         "is no URL form — a build is the only way to set one, so a "
                         "shared level can never arrive with the fog lifted.")
    ap.add_argument("--title", default=None,
                    help="set the page <title>, which is what NAMES the published "
                         "artifact (the Artifact tool's own title arg is overridden "
                         "by this tag). Use the skill's convention, e.g. "
                         "intrusion-110-8371-1.")
    args = ap.parse_args()
    if args.seed is not None and not valid_level_seed(args.seed):
        sys.exit(f"assemble: --seed must be a level-seed string — a u64, or an "
                 f"`L1-…` token from LevelSeed::encode — got {args.seed!r}")

    # Debug flags are validated against the shell's own set: an unknown name would be
    # ignored in the browser, so a typo would publish a build that quietly lacks the
    # switch it was built for.
    debug_flags = []
    if args.debug is not None:
        debug_flags = [f.strip() for f in args.debug.split(",") if f.strip()]
        unknown = [f for f in debug_flags if f not in DEBUG_FLAGS]
        if unknown or not debug_flags:
            sys.exit(f"assemble: --debug takes a comma-separated list of "
                     f"{', '.join(DEBUG_FLAGS)} — got {args.debug!r}")

    # A replay carries its own level (§12.4/#245), so it and --seed are exclusive.
    # Parse the `{seed, inputs}` pair from --emit-replay's output; the `seed` is the
    # replay's **level-seed string** (an opaque token the core decodes, #245) which
    # pins the facility, modifiers, and loadout, and the inputs stream drives the
    # viewer. It is baked verbatim — the core validates it, not this script.
    replay_token = replay_inputs = None
    if args.replay_json is not None:
        if args.seed is not None:
            sys.exit("assemble: pass --seed or --replay-json, not both "
                     "(a replay carries its own level)")
        raw = (sys.stdin.read() if args.replay_json == "-"
               else pathlib.Path(args.replay_json).read_text())
        try:
            obj = json.loads(raw)
            replay_token = str(obj["seed"])
            replay_inputs = str(obj["inputs"])
        except (ValueError, KeyError, TypeError) as e:
            sys.exit(f"assemble: --replay-json is not a {{seed,inputs}} line "
                     f"(from `sim --emit-replay`): {e}")
        if not valid_level_seed(replay_token):
            sys.exit(f"assemble: --replay-json seed is not a level-seed string "
                     f"(a u64 or an `L1-…` token): {replay_token!r}")

    dist = pathlib.Path(args.dist)
    glue = (dist / "intrusion_web.js").read_text()
    wasm_b64 = base64.b64encode(
        (dist / "intrusion_web_bg.wasm").read_bytes()).decode()
    index = pathlib.Path(args.index).read_text()

    # The page <title> names the published artifact (the host reads it from the
    # content, overriding the Artifact tool's title arg), so set it here when asked —
    # committing a per-build name into web/index.html would be wrong. The tag is not
    # inside the stripped skeleton, so it survives packing.
    if args.title is not None:
        index, n = re.subn(r"<title>.*?</title>",
                           lambda m: f"<title>{args.title}</title>",
                           index, count=1, flags=re.S)
        if n != 1:
            sys.exit(f"assemble: expected one <title> in {args.index} to set, "
                     f"found {n}")

    # The glue is an ES module; inlined into one script tag its exports must go.
    glue = replace_once(glue, "export function start()",
                        "function start()", "start export")
    glue = replace_once(glue, "export { initSync };", "", "initSync export")
    glue = replace_once(glue, "export default __wbg_init;", "",
                        "default export")

    # Baked window globals stamped before start() runs, so the shell reads them
    # ahead of the URL and the clock (crates/web/src/seed.rs, crates/web/src/replay.rs).
    # This is the artifact-safe carrier: the host strips a `…#seed=N`/`inputs=` URL
    # before the framed page sees it, but a global set inside the page always wins.
    # A replay bakes both — the level-seed string pins the facility/modifiers/loadout,
    # the inputs boot the viewer (#197/#245); a bare --seed bakes only the seed, which
    # the shell decodes as quick play (#110/#244). Either way __intrusionSeed is a
    # string the core's `LevelSeed::decode` reads.
    bake_seed = replay_token if replay_inputs is not None else args.seed
    globals_js = ""
    if bake_seed is not None:
        globals_js += f"window.__intrusionSeed = {json.dumps(bake_seed)};\n"
    if replay_inputs is not None:
        globals_js += f"window.__intrusionReplay = {json.dumps(replay_inputs)};\n"
    # The debug carrier is its own global (crates/web/src/debug.rs), never a field of
    # the level: a playtest switch must not be able to ride along with a level someone
    # hands to someone else. It applies to whatever the page plays — a baked level, a
    # replay, or a run rolled from the menu.
    if debug_flags:
        globals_js += f"window.__intrusionDebug = {json.dumps(','.join(debug_flags))};\n"
    seed_line = globals_js

    boot = f"""
// --- artifact bootstrap: wasm embedded as base64, no fetch needed ---
{seed_line}const __b64 = "{wasm_b64}";
const __bin = Uint8Array.from(atob(__b64), c => c.charCodeAt(0));
__wbg_init({{ module_or_path: __bin.buffer }}).then(start);
"""
    script = '<script type="module">\n' + glue + boot + "\n</script>"

    # Replace the page's module script (which imports ./intrusion_web.js).
    out, n = re.subn(r'<script type="module">.*?</script>',
                     lambda m: script, index, flags=re.S)
    if n != 1:
        sys.exit(f"assemble: expected one module <script> in {args.index}, "
                 f"found {n}")

    # Strip the document skeleton the Artifact host provides itself.
    for tag in ["<body>", "</body>", "</html>", "<head>", "</head>"]:
        out = out.replace(tag, "")
    out = re.sub(r'<!doctype html>\s*<html[^>]*>\s*', "", out,
                 flags=re.I)

    pathlib.Path(args.out).write_text(out)
    print(f"assemble: wrote {args.out} ({len(out)} bytes, "
          f"wasm {len(wasm_b64) * 3 // 4} bytes)")


if __name__ == "__main__":
    main()
