#!/usr/bin/env bash
# unpack-appimage.sh — open an AppImage without executing it.
#
# The tool's own `--appimage-extract` is the documented way, but it runs the
# AppImage's runtime, and that runtime is a static-pie binary: under an emulated
# amd64 container on an arm64 host it cannot be executed at all. The payload is a
# plain squashfs appended to that runtime, so unsquashfs reads it directly and
# the runtime is never asked to run.
#
# The offset the payload starts at is not recorded anywhere readable, so it is
# found: every occurrence of the squashfs magic is a candidate, and the one the
# superblock reader accepts is the payload. Candidates before it are the literal
# bytes inside the runtime, which is why the first hit is not simply taken.
#
# Usage: unpack-appimage.sh <appimage> <destination-dir>
set -euo pipefail

img="${1:?an AppImage path is required}"
dest="${2:?a destination directory is required}"

while IFS= read -r off; do
  unsquashfs -s -o "$off" "$img" >/dev/null 2>&1 || continue
  rm -rf "$dest"
  if unsquashfs -q -n -o "$off" -d "$dest" "$img" >/dev/null 2>&1; then
    echo "→ unpacked $img at offset $off"
    exit 0
  fi
done < <(grep -abo hsqs "$img" | cut -d: -f1)

echo "✗ no squashfs payload could be read out of $img" >&2
exit 1
