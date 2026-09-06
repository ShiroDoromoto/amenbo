#!/usr/bin/env bash
# build-linux-cli.sh — runs INSIDE the linux-gui container (see Dockerfile.linux-gui).
# Builds the amenbo CLI FOR LINUX and drops the binary into the mounted /out.
#
# Who needs it: the release itself (`make dist-cli-linux`, one job per arch). The container's
# Ubuntu 22.04 is the glibc floor of the Linux distribution — the AppImage stands on it, and a CLI
# compiled anywhere newer links a glibc the same users do not have. The Linux GUI e2e
# (`make verify-gui-linux`) then writes to the store with this very binary: the AppImage it
# launches carries the GUI alone, and the check's whole question is whether a SEPARATE process
# repaints that GUI.
#
# The dev channel's preview builds through this same file, with one thing different: DEV_APP_NAME
# names the app-data the binary addresses — the theme's own, so a member's previews and their real
# store stand apart on one machine. It is read at compile time by amenbo-core, so it has to be in
# the environment of the build: the output file name says nothing about it, and a preview compiled
# without it opens the member's production store.
#
# Mounts expected (set by `make dist-cli-linux`):
#   /src                  (ro)  the repo
#   /out                  (rw)  where the binary is collected  (host: ./dist)
#   /build/target         (rw)  named volume — the workspace target dir (shared with lint-linux)
#   /root/.cargo/registry (rw)  named volume — the crates.io cache
set -euo pipefail

OUT_NAME="${OUT_NAME:?OUT_NAME (the dist file name, e.g. amenbo-linux-arm64) must be passed in}"
# Optional, and empty means production — in which case the app-data name is the AMENBO_APP_NAME the
# make target passed in (`-e`: a container inherits nothing). A non-empty one is the dev channel's
# and replaces it below; exporting it empty would compile in an empty app name.
DEV_APP_NAME="${DEV_APP_NAME:-}"

echo "→ [container] syncing source /src → /build (excluding target/node_modules/.git/dist)"
rsync -a --delete \
  --exclude '.git' --exclude 'target' --exclude 'node_modules' \
  --exclude 'dist' --exclude 'app/dist' \
  /src/ /build/

cd /build

# The same two knobs lint-linux.sh sets, for the same reason: Docker Desktop's VM has more
# CPUs than memory, and full parallelism OOM-kills an uncached run.
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-4}"

if [ -n "$DEV_APP_NAME" ]; then
  export AMENBO_APP_NAME="$DEV_APP_NAME"
  echo "→ [container] dev channel: app-data $DEV_APP_NAME"
fi
# amenbo-core has no app-data name of its own, so neither channel may leave this unset — and the
# compiler would only say so several minutes into the build.
: "${AMENBO_APP_NAME:?AMENBO_APP_NAME (the app-data this build addresses) must be passed in}"

cargo build --release -p amenbo-cli
mkdir -p /out
cp target/release/amenbo "/out/${OUT_NAME}"
echo "→ [container] Linux CLI collected: /out/${OUT_NAME}"
