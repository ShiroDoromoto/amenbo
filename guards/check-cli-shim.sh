#!/usr/bin/env bash
# check-cli-shim.sh — keep the GUI/CLI version-skew window architecturally shut.
#
# The unified installer ships one GUI and one CLI, and a per-user self-update replaces
# the GUI in place. Skew — a CLI left on the old version while the GUI moves — is the
# most serious risk that design carries, and it is meant to be *impossible by
# construction*, not merely avoided: the CLI that answers on PATH has to BE the binary
# the GUI update replaces, never a separate copy that the update leaves behind.
#
# Two mechanisms hold that, one per platform, and nothing else watches them: a build
# succeeds whether the CLI on PATH is a live shim or a frozen copy, so a later edit that
# swaps `ln -s` for `cp`, or flips the Windows install mode, would ship a skew bug that
# only surfaces on a user's machine one update later. This guard asks the question
# directly, at gate time, against the source that decides it.
#
#   macOS — scripts/build-pkg-mac.sh's postinstall must put the CLI on PATH as a
#     *symlink into the installed .app bundle interior* (…/Contents/MacOS/amenbo), and
#     must never *copy* the CLI onto PATH. The GUI updater swaps the .app at its fixed
#     path, so a symlink resolves to the new binary the instant the app is replaced; a
#     copy would freeze the version the installer wrote.
#
#   Windows — app/src-tauri/tauri.conf.json must ship the CLI as a Tauri `externalBin`
#     (so NSIS lands amenbo.exe inside $INSTDIR, beside the GUI) with NSIS
#     installMode=currentUser (so $INSTDIR is the per-user dir the updater reinstalls in
#     place). Windows cannot cheaply symlink, so co-location in the updated dir is what
#     makes the CLI ride the same atomic replacement. perMachine/both, or dropping the
#     externalBin, would break that premise.
#
# Linux is out of scope here: the AppImage is a single self-updating file, so there is
# no second artifact to keep in step.
#
# Usage: guards/check-cli-shim.sh          (no args; reads the two source files above)
# Exit codes: 0 = invariants hold, 1 = one broke.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
pkg=$root/scripts/build-pkg-mac.sh
conf=$root/app/src-tauri/tauri.conf.json

rc=0
fail() { echo "✗ $*" >&2; rc=1; }

# --- macOS: CLI on PATH is a symlink into the .app, never a copy ---------------------
if [ ! -f "$pkg" ]; then
  fail "cli shim (mac): $pkg is missing — the mac arm of the unified installer moved?"
else
  # The symlink into the bundle interior must be present…
  if ! grep -Eq 'ln -s.*Contents/MacOS/amenbo' "$pkg"; then
    fail "cli shim (mac): $pkg no longer symlinks the CLI on PATH into the .app (…/Contents/MacOS/amenbo).
    Without the symlink the CLI cannot ride the GUI's in-place update — that is the skew window."
  fi
  # …and the CLI must not be *copied* onto PATH (a frozen version the GUI update leaves behind).
  if grep -Eq 'cp .*(BIN_DIR/amenbo|\.local/bin/amenbo)"?[[:space:]]*$' "$pkg"; then
    fail "cli shim (mac): $pkg copies the CLI onto PATH instead of symlinking it.
    A copy freezes the installed version; the GUI updater then moves on without it."
  fi
fi

# --- Windows: CLI co-located in the per-user $INSTDIR the updater replaces ------------
if [ ! -f "$conf" ]; then
  fail "cli shim (win): $conf is missing — the GUI bundle config moved?"
else
  python3 - "$conf" <<'PY' || rc=1
import json, sys

with open(sys.argv[1]) as f:
    conf = json.load(f)
bundle = conf.get("bundle", {})

ok = True
def fail(msg):
    global ok
    ok = False
    print(f"✗ cli shim (win): {msg}", file=sys.stderr)

ext = bundle.get("externalBin") or []
if not any("amenbo" in str(e) for e in ext):
    fail("tauri.conf.json bundle.externalBin no longer ships the CLI sidecar.\n"
         "    Without it NSIS drops no amenbo.exe into $INSTDIR, so the CLI cannot ride the GUI update.")

mode = bundle.get("windows", {}).get("nsis", {}).get("installMode")
if mode != "currentUser":
    fail(f"NSIS installMode is {mode!r}, not 'currentUser'.\n"
         "    Only the per-user $INSTDIR is what a no-elevation self-update reinstalls in place.")

sys.exit(0 if ok else 1)
PY
fi

if [ "$rc" -eq 0 ]; then
  echo "✓ cli shim: GUI/CLI version-skew window held shut (mac symlink + win co-location)"
fi
exit "$rc"
