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
    args = ap.parse_args()

    dist = pathlib.Path(args.dist)
    glue = (dist / "intrusion_web.js").read_text()
    wasm_b64 = base64.b64encode(
        (dist / "intrusion_web_bg.wasm").read_bytes()).decode()
    index = pathlib.Path(args.index).read_text()

    # The glue is an ES module; inlined into one script tag its exports must go.
    glue = replace_once(glue, "export function start()",
                        "function start()", "start export")
    glue = replace_once(glue, "export { initSync };", "", "initSync export")
    glue = replace_once(glue, "export default __wbg_init;", "",
                        "default export")

    boot = f"""
// --- artifact bootstrap: wasm embedded as base64, no fetch needed ---
const __b64 = "{wasm_b64}";
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
