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
import pathlib
import re
import sys


def replace_once(text: str, old: str, new: str, what: str) -> str:
    if text.count(old) != 1:
        sys.exit(f"assemble: expected exactly one occurrence of {what!r} "
                 f"({old!r}), found {text.count(old)} — glue format drifted?")
    return text.replace(old, new)


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--dist", required=True,
                    help="wasm-bindgen --target web output dir")
    ap.add_argument("--index", required=True, help="path to web/index.html")
    ap.add_argument("--out", required=True, help="output HTML path")
    ap.add_argument("--seed", type=int, default=None,
                    help="bake a fixed run seed into the page (a u64). The build "
                         "boots this facility with no URL and no typing — how a "
                         "seed-locked artifact pins the exact level the sim played "
                         "(#110). Omit for the normal random-seed build.")
    ap.add_argument("--title", default=None,
                    help="set the page <title>, which is what NAMES the published "
                         "artifact (the Artifact tool's own title arg is overridden "
                         "by this tag). Use the skill's convention, e.g. "
                         "intrusion-110-8371-1.")
    args = ap.parse_args()
    if args.seed is not None and not (0 <= args.seed < 2**64):
        sys.exit(f"assemble: --seed must be a u64 (0 .. 2^64-1), got {args.seed}")

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

    # A baked seed is stamped as a window global before start() runs, so the shell's
    # initial_seed() reads it ahead of the URL and the clock (crates/web/src/seed.rs).
    # This is the artifact-safe way to pin a seed: the host strips a `…#seed=N` hash
    # before the framed page sees it, but a global set inside the page always wins.
    seed_line = (f'window.__intrusionSeed = "{args.seed}";\n'
                 if args.seed is not None else "")

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
