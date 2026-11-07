#!/usr/bin/env bash
# check-no-wasm.sh — keep WebAssembly out of what the window is built to need.
#
# This gate is the whole of that: allowing a module would mean deciding that arbitrary bytecode may
# run where the person's tasks are, and nothing else refuses one. The CSP is not a second lock — it
# reaches the document and not a worker, measured on both engines (tauri sets the
# `Content-Security-Policy` header on `.html` and on nothing else, 2.11.5), and pdf.js reads a
# document in a worker. What makes that easy to lose is that dependency bumps are merged unattended.
#
# What is measured, in the order it is read:
#
#  1. **A wasm file inside a package that ships.** `npm ls --omit=dev` is the shipped half of the
#     tree — the same split the license and audit gates take — and a `.wasm` in one of them is wasm
#     that arrived, whether or not this build happens to emit it. A reader handed one it may not run
#     is not a reader that still works; it is one that fails on the documents it wanted it for. One
#     package is allowed the files it publishes, and only those, below.
#  2. **A wasm file in the bundle.** `app/dist` is what Tauri ships (`frontendDist`), so a module
#     emitted beside it is one the window can fetch. Nothing is allowed here, the package above
#     included.
#  3. **A wasm module inlined into JavaScript**, which is the other form it arrives in — a `data:`
#     URL holding the module's bytes, bundled along with the code that instantiates it. No package is
#     allowed one.
#
# What it does not catch: a module assembled at run time out of bytes that are not recognisable as
# one. That is not what this guards against — the question is what a bump drags in, not what
# someone determined could hide. pdf.js does build one that way, out of a document's PostScript
# functions, and what keeps it from running is `useWasm: false` in `app/src/files/pdfLoad.ts`, which
# the test beside that module holds in place.
#
# Usage: guards/check-no-wasm.sh   (no args; run it after `npm --prefix app run build`)
# Exit codes: 0 = no wasm anywhere it was read for, 1 = some, or the tree is not in a state to read.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

app=app
dist=$app/dist

# The one package allowed wasm files of its own, and which files they are.
#
# `pdfjs-dist` publishes four from 6.3.289 on (5.0.375 was the first, with two): the JPEG 2000 and
# JBIG2 decoders, the colour-profile engine and the script sandbox. **They sit in node_modules and
# go no further.** The reader opens every document with `useWasm: false`, so pdf.js neither fetches
# nor runs them, and the build carries none of them — the second scan below holds that. What decodes
# a JPEG 2000 or JBIG2 image instead is the JavaScript build of the same decoder that pdf.js
# publishes beside it (`*_nowasm_fallback.js`), which `app/vite.config.ts` emits into the bundle.
#
# **The allowance names files, not the package.** A fifth wasm file in pdfjs-dist, or one of these
# four moved, is caught like any other.
WASM_PACKAGE=pdfjs-dist
WASM_ALLOWED="wasm/openjpeg.wasm wasm/jbig2.wasm wasm/qcms_bg.wasm wasm/quickjs-eval.wasm"

[ -d "$app/node_modules" ] || {
    echo "✗ no wasm: $app/node_modules is missing — run \`npm ci\` in $app first." >&2
    exit 1
}
[ -d "$dist" ] || {
    echo "✗ no wasm: $dist is missing — run \`npm run build\` in $app first, so there is a bundle to read." >&2
    exit 1
}

# The shipped packages, one directory per line, minus the app itself: its own directory holds
# node_modules whole, dev dependencies and all, and every package under it that ships is listed here
# in its own right. A package's nested install is listed separately too, which is why the scans below
# stop at one rather than descending into it.
shipped=$(cd "$app" && npm ls --omit=dev --parseable --all 2>/dev/null | tail -n +2)
[ -n "$shipped" ] || {
    echo "✗ no wasm: \`npm ls --omit=dev\` named no packages at all — either the install is broken," >&2
    echo "  or the shape this guard reads changed. Fix the guard rather than deleting it." >&2
    exit 1
}

carried=$(printf '%s\n' "$shipped" \
    | while IFS= read -r dir; do
        [ -d "$dir" ] || continue
        find "$dir" -name '*.wasm' -not -path "$dir/node_modules/*" 2>/dev/null \
          | while IFS= read -r file; do
              case "${dir##*/node_modules/}" in
                "$WASM_PACKAGE") case " $WASM_ALLOWED " in *" ${file#"$dir"/} "*) continue ;; esac ;;
              esac
              printf '%s\n' "$file"
            done || true
      done)
if [ -n "$carried" ]; then
    echo "✗ no wasm: a package that ships carries a WebAssembly module of its own." >&2
    printf '%s\n' "$carried" | sed "s|^$root/||" >&2
    echo "  The window will not run one, so a dependency built around it cannot be taken as it is." >&2
    echo "  Hold the version that does without it, or take the decision again (\`AMB-D-769\`)." >&2
    exit 1
fi

emitted=$(find "$dist" -name '*.wasm' 2>/dev/null || true)
if [ -n "$emitted" ]; then
    echo "✗ no wasm: the bundle carries a WebAssembly module." >&2
    printf '%s\n' "$emitted" >&2
    exit 1
fi

# `AGFzbQ` is what a module's first four bytes (`\0asm`) come to in base64, which is the form an
# inlined one is carried in.
inlined=$(printf '%s\n' "$shipped" \
    | while IFS= read -r dir; do
        [ -d "$dir" ] || continue
        grep -rl 'AGFzbQ' "$dir" --exclude-dir=node_modules --exclude='*.wasm' 2>/dev/null || true
      done)
if [ -n "$inlined" ]; then
    echo "✗ no wasm: a package that ships carries a WebAssembly module inline (base64)." >&2
    printf '%s\n' "$inlined" | sed "s|^$root/||" >&2
    echo "  No package is allowed one — read the allowance in this guard before widening it." >&2
    exit 1
fi

# The allowance answers for something that is still there. Once it is not, it is a waiver suppressing
# a question that answers itself, and the gate says to drop it rather than keep it.
for file in $WASM_ALLOWED; do
    [ -f "$app/node_modules/$WASM_PACKAGE/$file" ] && continue
    echo "✗ no wasm: $WASM_PACKAGE no longer publishes $file, so the allowance for it in this guard" >&2
    echo "  answers for nothing. Delete it — an exception nobody revisits is one nobody notices." >&2
    exit 1
done

echo "✓ no wasm: of the $(printf '%s\n' "$shipped" | wc -l | tr -d ' ') packages that ship, only $WASM_PACKAGE publishes any — the four it is allowed — and the bundle emits none"
