#!/usr/bin/env bash
# check-no-wasm.sh — keep WebAssembly out of what the window is built to need.
#
# The window will not run wasm: the CSP refuses it, measured on two engines, and allowing it would
# mean deciding that arbitrary bytecode may run where the person's tasks are. What makes that easy to
# lose is that dependency bumps are merged unattended. `pdfjs-dist` is the one already on its way:
# 4.10.38 is what this tree holds, 5.0.375 is the first release to publish `wasm/openjpeg.wasm` and
# `wasm/qcms_bg.wasm` as files of their own, and 6.3.289 adds `wasm/jbig2.wasm` and
# `wasm/quickjs-eval.wasm` to them.
#
# What is measured, in the order it is read:
#
#  1. **A wasm file inside a package that ships.** `npm ls --omit=dev` is the shipped half of the
#     tree — the same split the license and audit gates take — and a `.wasm` in one of them is wasm
#     that arrived, whether or not this build happens to emit it. A reader handed one it may not run
#     is not a reader that still works; it is one that fails on the documents it wanted it for.
#  2. **A wasm file in the bundle.** `app/dist` is what Tauri ships (`frontendDist`), so a module
#     emitted beside it is one the window can fetch.
#  3. **A wasm module inlined into JavaScript**, which is the other form it arrives in — a `data:`
#     URL holding the module's bytes, bundled along with the code that instantiates it. One package
#     carries one today and is named below.
#
# What it does not catch: a module assembled at run time out of bytes that are not recognisable as
# one. That is not what this guards against — the question is what a bump drags in, not what
# someone determined could hide.
#
# Usage: guards/check-no-wasm.sh   (no args; run it after `npm --prefix app run build`)
# Exit codes: 0 = no wasm anywhere it was read for, 1 = some, or the tree is not in a state to read.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

app=app
dist=$app/dist

# The one package allowed to carry an inlined module, and what the allowance rests on.
#
# `pdfjs-dist` 4.10.38 inlines an Emscripten build of OpenJPEG — 260,823 bytes, as a `data:` URL —
# into its worker, its sandbox and its image decoders. It is the JPEG 2000 decoder, reached only from
# `JpxImage`, and it is allowed to sit there because the window never runs it: the CSP refuses to
# compile it, so a PDF holding a JPEG 2000 image loses that image and the rest of the document still
# draws. Nothing calls into it otherwise.
#
# **The allowance is a claim about that one decoder, not about the package.** A wasm file published
# beside the code is caught by the scan above it regardless of who published it.
INLINE_ALLOWED=pdfjs-dist

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
        find "$dir" -name '*.wasm' -not -path "$dir/node_modules/*" 2>/dev/null || true
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
        case "${dir##*/node_modules/}" in "$INLINE_ALLOWED"|"$INLINE_ALLOWED"/*) continue ;; esac
        [ -d "$dir" ] || continue
        grep -rl 'AGFzbQ' "$dir" --exclude-dir=node_modules 2>/dev/null || true
      done)
if [ -n "$inlined" ]; then
    echo "✗ no wasm: a package that ships carries a WebAssembly module inline (base64)." >&2
    printf '%s\n' "$inlined" | sed "s|^$root/||" >&2
    echo "  Only $INLINE_ALLOWED is allowed one, and only because the window never runs it — read the" >&2
    echo "  allowance in this guard before widening it." >&2
    exit 1
fi

# The allowance answers for something that is still there. Once it is not, it is a waiver suppressing
# a question that answers itself, and the gate says to drop it rather than keep it.
if ! grep -rlq 'AGFzbQ' "$app/node_modules/$INLINE_ALLOWED" --exclude-dir=node_modules 2>/dev/null; then
    echo "✗ no wasm: $INLINE_ALLOWED no longer carries an inlined module, so the allowance in this" >&2
    echo "  guard answers for nothing. Delete it — an exception nobody revisits is one nobody notices." >&2
    exit 1
fi

echo "✓ no wasm: none of the $(printf '%s\n' "$shipped" | wc -l | tr -d ' ') packages that ship publishes one, and the bundle emits none"
