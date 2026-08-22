#!/usr/bin/env bash
# build-linux-gui.sh — runs INSIDE the linux-gui container (see Dockerfile.linux-gui).
# Copies the mounted source into a container-local build tree (so the host's target/
# and mac-native node_modules are never touched), builds the Tauri Linux bundle, and
# copies the AppImage out to the mounted /out with a stable, wharfy-friendly name.
#
# The AppImage is the whole of the Linux GUI distribution: one user-writable file, so the
# GUI self-update path (tauri-plugin-updater) can swap it in place with no /usr/bin
# permission wall; the user drops it on PATH themselves (e.g. ~/.local/bin). GUI-only (it
# does not own CLI PATH exposure — the CLI has its own install route). With the release
# signing key set it also carries its minisign updater signature (see SIGN_UPDATER below).
#
# The container's arch IS the build arch: on Apple Silicon, `docker run` defaults to
# linux/arm64, so `make dist-gui-linux` passes --platform to pin it and hands us
# TARGET_ARCH (amd64|arm64) to (a) name the artifact truthfully and (b) assert the
# produced AppImage actually matches — otherwise a silent emulation fallback would ship a
# mislabeled bundle.
#
# The same recipe builds the dev channel's preview (`make gui-dev-linux`), which is the shipped
# bundle with three things different: tauri is told the theme's own product name, identifier and
# executable name (DEV_CONFIG), the build is told its app-data name (DEV_APP_NAME), and the artifact
# is collected under a name that says which theme it is (OUT_IMG_NAME). It shares this file rather
# than getting one of its own because everything else — the Ubuntu 22.04 glibc floor, the tool
# cache, the arch guard, the rate-limit retry — is the same question on both channels.
#
# Mounts expected (set by `make dist-gui-linux` / `make gui-dev-linux`):
#   /src  (ro)  the repo
#   /out  (rw)  where dist artifacts are collected  (host: ./dist)
set -euo pipefail

VERSION="${VERSION:?VERSION must be passed in}"
TARGET_ARCH="${TARGET_ARCH:?TARGET_ARCH (amd64|arm64) must be passed in}"

# The dev channel's three inputs, or none of them. They arrive together or not at all: a config
# without an app-data name builds a bundle wearing the theme's identifier over the shared dev
# store, which is the one way two themes can still collide on a member's machine.
DEV_APP_NAME="${DEV_APP_NAME:-}"
DEV_CONFIG="${DEV_CONFIG:-}"
OUT_IMG_NAME="${OUT_IMG_NAME:-}"
DEV_CHANNEL=0
if [ -n "$DEV_APP_NAME" ] || [ -n "$DEV_CONFIG" ] || [ -n "$OUT_IMG_NAME" ]; then
  if [ -z "$DEV_APP_NAME" ] || [ -z "$DEV_CONFIG" ] || [ -z "$OUT_IMG_NAME" ]; then
    echo "✗ dev channel: DEV_APP_NAME, DEV_CONFIG and OUT_IMG_NAME must be passed together" >&2
    exit 2
  fi
  DEV_CHANNEL=1
fi

# The AppImage arch token, and the machine name `file` reads back out of the built ELF.
case "$TARGET_ARCH" in
  amd64) IMG_ARCH=x86_64;  ELF_MACHINE='x86-64' ;;
  arm64) IMG_ARCH=aarch64; ELF_MACHINE='aarch64' ;;
  *) echo "✗ unsupported TARGET_ARCH='$TARGET_ARCH' (want amd64|arm64)" >&2; exit 2 ;;
esac

echo "→ [container] building for TARGET_ARCH=$TARGET_ARCH (appimage=$IMG_ARCH), running arch=$(uname -m)"

echo "→ [container] syncing source /src → /build (excluding target/node_modules/.git/dist)"
rsync -a --delete \
  --exclude '.git' --exclude 'target' --exclude 'node_modules' \
  --exclude 'dist' --exclude 'app/dist' \
  /src/ /build/

cd /build/app
echo "→ [container] npm ci"
npm ci

# AppImage packaging uses linuxdeploy/appimagetool, which normally need FUSE. In a
# container FUSE is often unavailable, so force the extract-and-run path.
export APPIMAGE_EXTRACT_AND_RUN=1

BUNDLE=/build/app/src-tauri/target/release/bundle
mkdir -p /out
# Exactly one artifact exists in the bundle dir per build; glob it rather than guess the
# arch token in the filename.
shopt -s nullglob

DIST_IMG="/out/${OUT_IMG_NAME:-amenbo-app-linux-${IMG_ARCH}.AppImage}"
DIST_IMG_SIG="${DIST_IMG}.sig"

# Signed updater artifact: only when the release CI's signing key is present. With it set, the
# AppImage stage layers createUpdaterArtifacts (updater.conf.json) so tauri minisign-signs the
# AppImage in place and writes <name>.AppImage.sig beside it — the artifact the GUI self-update
# path (tauri-plugin-updater) consumes (Tauri v2 re-uses the .AppImage itself, not a .tar.gz). A
# local keyless build skips it: the AppImage is still produced, just unsigned.
# A preview never signs. It carries no release stamp, so it withholds the update check entirely
# (update_check::withheld_from_build) and the dev channel refuses self-update on top of that
# (self_update::apply) — an updater artifact would be one nothing can ever consume.
SIGN_UPDATER=0
if [ "$DEV_CHANNEL" = 0 ] && [ -n "${TAURI_SIGNING_PRIVATE_KEY:-}" ]; then SIGN_UPDATER=1; fi

# The AppImage stage downloads linuxdeploy/appimagetool from GitHub, which is the 429-prone
# part: with XDG_CACHE_HOME on a persistent volume the tools are fetched once and reused, but
# a cold cache can still hit a transient rate limit. Retry with backoff so a cold first run
# self-heals.
#
# Retry only what a retry can fix. The failure has to SAY it is rate-limited or a network
# fault before we wait on it; anything else fails immediately, with the tail of what it
# actually said.
#
# The signature is deliberately narrow. A bare `429` would match a line number or a byte
# count in anyone's output, so an HTTP status only counts when it is written as one.
retryable() {
  grep -qiE 'too many requests|rate.?limit|(http|status|code|error)[^0-9]{0,10}429|429 (too many|rate)|connection (reset|refused|timed out)|network is unreachable|temporary failure in name resolution|tls handshake' "$1"
}

# The AppImage build args. With signing, layer updater.conf.json (createUpdaterArtifacts) so
# tauri emits the signed updater artifact; the path is relative to the tauri project dir (cwd is
# /build/app), mirroring the windows release job. Keyless builds omit it — createUpdaterArtifacts
# with a pubkey but no private key fails the build, so a local `make dist-gui-linux` must not pass it.
img_args=(--bundles appimage)
if [ "$SIGN_UPDATER" = 1 ]; then img_args+=(-c src-tauri/updater.conf.json); fi
# The dev channel's icons and defaults come from the committed tauri.dev.conf.json, and the theme's
# own three names are merged over it — the same two --config arguments, in the same order, that the
# mac dev bundle is built with (Makefile, gui-dev).
if [ "$DEV_CHANNEL" = 1 ]; then
  img_args+=(--config src-tauri/tauri.dev.conf.json --config "$DEV_CONFIG")
  # Which app-data the built binaries address. Read at compile time by amenbo-core, so it has to be
  # in the environment of the build, not of the run.
  export AMENBO_APP_NAME="$DEV_APP_NAME"
  echo "→ [container] dev channel: app-data $DEV_APP_NAME, config $DEV_CONFIG"
fi

[ "$SIGN_UPDATER" = 1 ] && sign_note=", signed updater artifact" || sign_note=""
echo "→ [container] tauri build (appimage${sign_note})"
build_ok=0
log="$(mktemp)"
for attempt in 1 2 3; do
  if npm run tauri build -- "${img_args[@]}" 2>&1 | tee "$log"; then build_ok=1; break; fi
  if ! retryable "$log"; then
    echo "✗ [container] appimage build failed for a reason a retry cannot fix — its own last words:" >&2
    tail -n 20 "$log" >&2
    exit 1
  fi
  if [ "$attempt" = 3 ]; then break; fi
  echo "→ [container] appimage build attempt $attempt hit a rate limit / network fault; backing off $((attempt*30))s…" >&2
  sleep $((attempt*30))
done
if [ "$build_ok" != 1 ]; then
  echo "✗ AppImage build rate-limited on all 3 attempts (the amd64 release build runs in CI on a native runner)" >&2
  exit 1
fi

imgs=("$BUNDLE"/appimage/*.AppImage)
[ "${#imgs[@]}" -eq 1 ] || { echo "✗ expected exactly one .AppImage, found ${#imgs[@]}: ${imgs[*]:-none}" >&2; exit 1; }
cp "${imgs[0]}" "$DIST_IMG"
chmod +x "$DIST_IMG"

# Arch guard: the AppImage runtime is an ELF for the arch it was built on, so reading its
# machine back is what catches a silent emulation/platform mismatch — a truthfully-named but
# wrong-arch bundle would otherwise ship.
built_machine="$(file -b "$DIST_IMG")"
case "$built_machine" in
  *"$ELF_MACHINE"*) ;;
  *) echo "✗ arch mismatch: requested $IMG_ARCH but the AppImage reads as '$built_machine'." >&2
     echo "  (on Apple Silicon, pass --platform linux/$TARGET_ARCH to docker — make dist-gui-linux does)" >&2
     exit 1 ;;
esac

echo "→ [container] AppImage self-check (unpack):"
"$DIST_IMG" --appimage-extract >/dev/null 2>&1 && echo "  AppImage unpacks OK" || echo "  (AppImage extract check skipped)"
rm -rf /build/app/squashfs-root 2>/dev/null || true

collected=("$DIST_IMG")
# The signed updater artifact rides beside the AppImage. tauri writes <name>.AppImage.sig; the
# signature is over the bytes, so renaming the AppImage to its release name keeps it valid.
if [ "$SIGN_UPDATER" = 1 ]; then
  sigs=("$BUNDLE"/appimage/*.AppImage.sig)
  [ "${#sigs[@]}" -eq 1 ] || { echo "✗ signing was requested but expected exactly one .AppImage.sig, found ${#sigs[@]}: ${sigs[*]:-none}" >&2; exit 1; }
  cp "${sigs[0]}" "$DIST_IMG_SIG"
  echo "→ [container] updater signature: $DIST_IMG_SIG"
  collected+=("$DIST_IMG_SIG")
fi

echo "→ [container] collected artifacts:"
ls -1 "${collected[@]}"
