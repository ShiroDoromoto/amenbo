#!/usr/bin/env bash
# build-linux-gui.sh — runs INSIDE the linux-gui container (see Dockerfile.linux-gui).
# Copies the mounted source into a container-local build tree (so the host's target/
# and mac-native node_modules are never touched), builds the Tauri Linux bundles, and
# copies the .deb + .rpm + AppImage out to the mounted /out with stable, wharfy-friendly
# names.
#
# Two Linux routes with different jobs:
#   .deb / .rpm  — the SYSTEM install (root, /usr/bin). They ship the GUI *and* the amenbo CLI on
#     PATH: the CLI rides in as a Tauri sidecar (externalBin), installed to /usr/bin/amenbo (triple
#     stripped) next to the GUI /usr/bin/amenbo-app — both land on PATH from one install. Not
#     self-updating (a root-owned path needs elevation to replace); a new version means a new package.
#     The CLI-on-PATH guarantee is enforced below (a build fails loudly if /usr/bin/amenbo regresses).
#   AppImage — the SELF-UPDATE lead. One user-writable file, so the GUI self-update path
#     (tauri-plugin-updater) can swap it in place with no /usr/bin permission wall; the user drops it
#     on PATH themselves (e.g. ~/.local/bin). GUI-only (it does not own CLI PATH exposure). With the
#     release signing key set it also carries its minisign updater signature (see SIGN_UPDATER below).
#
# The container's arch IS the build arch: on Apple Silicon, `docker run` defaults to
# linux/arm64, so `make dist-gui-linux` passes --platform to pin it and hands us
# TARGET_ARCH (amd64|arm64) to (a) name the artifacts truthfully and (b) assert the
# produced .deb actually matches — otherwise a silent emulation fallback would ship a
# mislabeled bundle.
#
# Mounts expected (set by `make dist-gui-linux`):
#   /src  (ro)  the repo
#   /out  (rw)  where dist artifacts are collected  (host: ./dist)
set -euo pipefail

VERSION="${VERSION:?VERSION must be passed in}"
TARGET_ARCH="${TARGET_ARCH:?TARGET_ARCH (amd64|arm64) must be passed in}"

# Per-format arch tokens: .deb uses amd64/arm64; .rpm and AppImage use x86_64/aarch64.
case "$TARGET_ARCH" in
  amd64) DEB_ARCH=amd64; IMG_ARCH=x86_64 ;;
  arm64) DEB_ARCH=arm64; IMG_ARCH=aarch64 ;;
  *) echo "✗ unsupported TARGET_ARCH='$TARGET_ARCH' (want amd64|arm64)" >&2; exit 2 ;;
esac
RPM_ARCH="$IMG_ARCH"

echo "→ [container] building for TARGET_ARCH=$TARGET_ARCH (deb=$DEB_ARCH rpm=$RPM_ARCH appimage=$IMG_ARCH), running arch=$(uname -m)"

echo "→ [container] syncing source /src → /build (excluding target/node_modules/.git/dist)"
rsync -a --delete \
  --exclude '.git' --exclude 'target' --exclude 'node_modules' \
  --exclude 'dist' --exclude 'app/dist' \
  /src/ /build/

cd /build/app
echo "→ [container] npm ci"
npm ci

# Which bundle formats to produce. Default is all three; override for a fast partial
# build — `make dist-gui-linux BUNDLES=deb` when you only need the .deb to verify and
# don't want to wait on (or get rate-limited by) the AppImage download.
BUNDLES="${BUNDLES:-deb,rpm,appimage}"
want() { case ",$BUNDLES," in *",$1,"*) return 0 ;; *) return 1 ;; esac; }
# Reject an unknown token rather than silently producing nothing.
for tok in ${BUNDLES//,/ }; do
  case "$tok" in
    deb|rpm|appimage) ;;
    *) echo "✗ unknown bundle '$tok' in BUNDLES='$BUNDLES' (want deb|rpm|appimage)" >&2; exit 2 ;;
  esac
done
if ! want deb && ! want rpm && ! want appimage; then
  echo "✗ BUNDLES='$BUNDLES' selects no known bundle" >&2; exit 2
fi

# AppImage packaging uses linuxdeploy/appimagetool, which normally need FUSE. In a
# container FUSE is often unavailable, so force the extract-and-run path.
export APPIMAGE_EXTRACT_AND_RUN=1

BUNDLE=/build/app/src-tauri/target/release/bundle
mkdir -p /out
# Exactly one artifact exists per bundle dir per build; glob it rather than guess the
# arch token in the filename (tauri writes deb=amd64/arm64 but rpm/AppImage=x86_64/aarch64).
shopt -s nullglob

DIST_DEB="/out/amenbo-app-linux-${DEB_ARCH}.deb"
DIST_RPM="/out/amenbo-app-linux-${RPM_ARCH}.rpm"
DIST_IMG="/out/amenbo-app-linux-${IMG_ARCH}.AppImage"
DIST_IMG_SIG="${DIST_IMG}.sig"

# Signed updater artifact: only when the release CI's signing key is present. With it set, the
# AppImage stage layers createUpdaterArtifacts (updater.conf.json) so tauri minisign-signs the
# AppImage in place and writes <name>.AppImage.sig beside it — the artifact the GUI self-update
# path (tauri-plugin-updater) consumes (Tauri v2 re-uses the .AppImage itself, not a .tar.gz). A
# local keyless build skips it: the AppImage is still produced (GUI-only), just unsigned.
SIGN_UPDATER=0
if [ -n "${TAURI_SIGNING_PRIVATE_KEY:-}" ]; then SIGN_UPDATER=1; fi

# What we managed to collect vs what's missing, so the final summary can name both — a
# partial build must never read as "distributable" (a release needs all three).
collected=()
missing=()

# ── Stage 1: native packages (.deb / .rpm) ──────────────────────────────────────────
# These download nothing from GitHub, so a failure here is a real build error (not a 429)
# — fail fast, no retry. They are copied to /out *before* the AppImage stage runs, so even
# if AppImage later fails or rebundles, the native artifacts are already safely retained.
native=()
if want deb; then native+=(deb); fi
if want rpm; then native+=(rpm); fi
if [ "${#native[@]}" -gt 0 ]; then
  native_csv="$(IFS=,; echo "${native[*]}")"
  echo "→ [container] tauri build (native: $native_csv)"
  npm run tauri build -- --bundles "$native_csv"
fi

if want deb; then
  debs=("$BUNDLE"/deb/*.deb)
  [ "${#debs[@]}" -eq 1 ] || { echo "✗ expected exactly one .deb, found ${#debs[@]}: ${debs[*]:-none}" >&2; exit 1; }
  # Assert the built arch matches what was requested — catches a silent emulation/platform
  # mismatch before it ships a truthfully-named-but-wrong-arch bundle.
  built_arch="$(dpkg-deb --field "${debs[0]}" Architecture)"
  [ "$built_arch" = "$DEB_ARCH" ] || {
    echo "✗ arch mismatch: requested $DEB_ARCH but .deb is $built_arch." >&2
    echo "  (on Apple Silicon, pass --platform linux/$TARGET_ARCH to docker — make dist-gui-linux does)" >&2
    exit 1
  }
  cp "${debs[0]}" "$DIST_DEB"
  # CLI-on-PATH guard: the native packages MUST carry the amenbo CLI at
  # /usr/bin/amenbo (the sidecar) beside the GUI /usr/bin/amenbo-app. If externalBin
  # ever regresses, this fails the build rather than silently shipping a GUI-only package.
  # NB: capture the listing into a var first, then grep the var — piping straight into
  # `grep -q` lets grep close the pipe early, and under `set -o pipefail` the SIGPIPE'd
  # dpkg-deb would fail the (correct) match. dpkg-deb -c prints the path in the last column
  # with or without a ./ prefix across versions; anchor on usr/bin/amenbo so the GUI
  # usr/bin/amenbo-app doesn't satisfy it.
  deb_contents="$(dpkg-deb -c "$DIST_DEB")"
  echo "→ [container] deb contents (bin + desktop entry):"
  printf '%s\n' "$deb_contents" | grep -E 'bin/|\.desktop' | head || true
  printf '%s\n' "$deb_contents" | grep -qE '[[:space:]](\./)?usr/bin/amenbo$' \
    || { echo "✗ .deb is missing the CLI at /usr/bin/amenbo — did the sidecar (externalBin) drop out?" >&2; exit 1; }
  collected+=("$DIST_DEB")
fi

if want rpm; then
  rpms=("$BUNDLE"/rpm/*.rpm)
  [ "${#rpms[@]}" -eq 1 ] || { echo "✗ expected exactly one .rpm, found ${#rpms[@]}: ${rpms[*]:-none}" >&2; exit 1; }
  rpm_arch="$(rpm -qp --qf '%{ARCH}' "${rpms[0]}" 2>/dev/null)"
  [ "$rpm_arch" = "$RPM_ARCH" ] || { echo "✗ arch mismatch: requested $RPM_ARCH but .rpm is ${rpm_arch:-unknown}." >&2; exit 1; }
  cp "${rpms[0]}" "$DIST_RPM"
  rpm_contents="$(rpm -qlp "$DIST_RPM")"
  echo "→ [container] rpm contents (bin):"
  printf '%s\n' "$rpm_contents" | grep -E '/usr/bin/' || true
  printf '%s\n' "$rpm_contents" | grep -qxE '/usr/bin/amenbo' \
    || { echo "✗ .rpm is missing the CLI at /usr/bin/amenbo — did the sidecar (externalBin) drop out?" >&2; exit 1; }
  collected+=("$DIST_RPM")
fi

# ── Stage 2: AppImage (GUI-only; downloads linuxdeploy/AppRun from GitHub) ───────────
# This is the 429-prone stage: with XDG_CACHE_HOME on a persistent volume the tools are
# fetched once and reused, but a cold cache can still hit a transient HTTP 429 (rate
# limit). Retry with backoff so a cold first run self-heals; if it still fails, keep the
# native artifacts already in /out and fall through to a NON-ZERO exit — so the release
# path never mistakes a partial build for a shippable set.
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

img_failed=0
if want appimage; then
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
  if [ "$build_ok" = 1 ]; then
    imgs=("$BUNDLE"/appimage/*.AppImage)
    [ "${#imgs[@]}" -eq 1 ] || { echo "✗ expected exactly one .AppImage, found ${#imgs[@]}: ${imgs[*]:-none}" >&2; exit 1; }
    cp "${imgs[0]}" "$DIST_IMG"
    echo "→ [container] AppImage self-check (unpack):"
    chmod +x "$DIST_IMG"
    "$DIST_IMG" --appimage-extract >/dev/null 2>&1 && echo "  AppImage unpacks OK" || echo "  (AppImage extract check skipped)"
    rm -rf /build/app/squashfs-root 2>/dev/null || true
    collected+=("$DIST_IMG")
    # The signed updater artifact rides beside the AppImage. tauri writes <name>.AppImage.sig; the
    # signature is over the bytes, so renaming the AppImage to its release name keeps it valid.
    if [ "$SIGN_UPDATER" = 1 ]; then
      sigs=("$BUNDLE"/appimage/*.AppImage.sig)
      [ "${#sigs[@]}" -eq 1 ] || { echo "✗ signing was requested but expected exactly one .AppImage.sig, found ${#sigs[@]}: ${sigs[*]:-none}" >&2; exit 1; }
      cp "${sigs[0]}" "$DIST_IMG_SIG"
      echo "→ [container] updater signature: $DIST_IMG_SIG"
      collected+=("$DIST_IMG_SIG")
    fi
  else
    img_failed=1
    missing+=("$DIST_IMG")
    echo "✗ AppImage build rate-limited on all 3 attempts (the amd64 release build runs in CI on a native runner)" >&2
  fi
fi

echo "→ [container] collected artifacts:"
if [ "${#collected[@]}" -gt 0 ]; then ls -1 "${collected[@]}"; else echo "  (none)"; fi
if [ "${#missing[@]}" -gt 0 ]; then
  echo "✗ [container] MISSING artifacts — this build is NOT a distributable set (do not release it):" >&2
  printf '  %s\n' "${missing[@]}" >&2
fi
# The native bundles are already retained in /out above; this non-zero exit only signals
# that the requested set is incomplete (AppImage 429), not that nothing was produced.
[ "$img_failed" = 0 ] || exit 1
