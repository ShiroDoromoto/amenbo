#!/usr/bin/env bash
# lint-linux.sh — runs INSIDE the linux-gui container (see Dockerfile.linux-gui).
# Compiles the tree FOR LINUX so the `#[cfg(target_os = "linux")]` branches are actually
# checked. `make test` on the mac only ever compiles the macOS side of every cfg, so a
# Linux-only branch (store_watch's inotify/statfs path is the live example) can be broken
# for months and every local gate still passes — the breakage surfaces in CI, i.e. after
# it is already on main.
#
# It runs the same two clippy invocations as CI's `lint` and `app-rust` jobs, on the same
# system deps (the image already carries webkit2gtk/gtk for the Tauri host crate), so a
# green run here means those jobs are green too.
#
# Mounts expected (set by `make lint-linux`):
#   /src                        (ro)  the repo
#   /build/target               (rw)  named volume — the workspace target dir
#   /build/app/src-tauri/target (rw)  named volume — the host crate's target dir
#   /root/.cargo/registry       (rw)  named volume — the crates.io cache
# The volumes are what make a second run cheap: /build itself is container-local (the
# source is copied in, never built in place, so the mac's target/ and its native
# node_modules stay untouched), and without them every run would compile from scratch.
set -euo pipefail

echo "→ [container] syncing source /src → /build (excluding target/node_modules/.git/dist)"
# `target` is excluded, so --delete leaves the mounted target volumes alone.
rsync -a --delete \
  --exclude '.git' --exclude 'target' --exclude 'node_modules' \
  --exclude 'dist' --exclude 'app/dist' \
  /src/ /build/

cd /build

# Keep the cold run inside the container's memory. Docker Desktop hands the VM 8 CPUs but
# only ~8 GB, and cargo sizes its parallelism off the CPU count — eight rustc processes
# linking the Tauri dependency graph at once OOM-kill the container (SIGKILL/137) on an
# uncached run. Two knobs, both free for a lint: fewer parallel jobs, and no
# debuginfo (clippy never reads it — it also cuts the volume's disk footprint).
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-4}"
export CARGO_PROFILE_DEV_DEBUG=0

rustc --version && cargo clippy --version

# The core/cli workspace — CI's `lint` job, same flags (feature-gated code included, and
# the facet env funnel: AMENBO_* is read via amenbo_core::env, never raw std::env).
echo "→ [container] clippy: workspace (core/cli)"
cargo clippy --all-targets --all-features -- -D warnings -D clippy::disallowed_methods

# The Tauri host crate — CI's `app-rust` job. Its build.rs (tauri_build) validates
# tauri.conf.json's externalBin=binaries/amenbo, and binaries/ is gitignored, so the
# sidecar has to be staged before clippy can even run. Same script the GUI build
# uses; here it produces the LINUX CLI, which is the point.
echo "→ [container] staging CLI sidecar (externalBin) for the host crate"
node app/scripts/prepare-cli-sidecar.mjs

echo "→ [container] clippy: app/src-tauri (Tauri host crate)"
cargo clippy --manifest-path app/src-tauri/Cargo.toml --all-targets -- -D warnings -D clippy::disallowed_methods

echo "→ [container] Linux branches are clean"
