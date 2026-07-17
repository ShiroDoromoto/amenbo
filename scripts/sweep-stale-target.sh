#!/usr/bin/env bash
# sweep-stale-target.sh — bound the growth of the cargo build caches.
#
# Why this exists:
#   `target/` has no GC. Cargo names artifacts by content hash, so every change to the
#   code, the rustc version or the feature set leaves the *previous* hash behind, forever.
#   On this repo that runs to gigabytes a day, and a crate dropped from the workspace
#   leaves its artifacts behind too — nothing ever reaps them.
#
# Why deleting is safe:
#   `target/` is a *regenerable cache*. Cargo identifies artifacts by hashed filename and
#   rebuilds whatever is missing; it can never pick up a stale one. The price of deleting
#   too much is build time, never correctness. That is why this needs no dedicated tool —
#   `cargo-sweep` (unmaintained) would only add a dependency, not safety.
#
# Why atime (and what atime does NOT tell us):
#   mtime is the wrong axis: a dependency's `.rlib` is written once and never touched
#   again, so the *oldest* files are exactly the ones every build still links against.
#   atime separates the two versions of the same crate correctly — the one this build read
#   gets a fresh atime, the leftover feature-variant does not.
#
#   But atime means "recently read", not "still needed": cargo opens nothing when a build
#   is fully cached, so right after a no-op build even live artifacts look old — a
#   sub-hour window sweeps live dependencies and buys a cold rebuild. Hence
#   SWEEP_DAYS is in *days* — long enough that
#   any real (non-cached) build in the window refreshes the whole link graph. A rare
#   mis-eviction costs one rebuild, which is the intended price of an LRU.
#
# Why not a cron/launchd job: it would burn CPU while the project is idle. Sweeping is only
# ever needed after a build, so the gate calls this at the end — and only when actually fat.
#
# Portability, and where it stops:
#   This is developer tooling for macOS and Linux. CI never runs it (the workflows call cargo
#   directly, and their runners are thrown away), and the Windows GUI build does not use `make test`.
#   The spellings below (`du -sk`, `find -mindepth -atime +N -delete -empty`, `stat`) are the ones
#   BSD and GNU agree on; anything more exotic (busybox, …) is not guaranteed — and does not need to
#   be, since the worst outcome of this script misfiring is a rebuild.
#
#   atime itself is where the real variation lives, and it cannot be papered over:
#     - macOS / APFS  — updated on every read.
#     - Linux default `relatime` — mount(8): atime is written only when it is older than mtime/ctime
#       **or older than 24 hours**. So a live artifact read twice in one day gets its atime bumped
#       once. That is harmless *because SWEEP_DAYS is in days*; a sub-day window would delete
#       artifacts read hours ago. The guard below refuses DAYS < 1 for exactly this reason.
#     - `noatime` and friends — atime never moves, every file looks unread, and the sweep would
#       degrade into `cargo clean`. The probe below detects this and bails out.
#
# Usage (via `make sweep-stale`; env-tunable):
#   SWEEP_LIMIT_GB=20 SWEEP_DAYS=3 scripts/sweep-stale-target.sh
#
# A clean no-op when the caches are under the limit, when no target dir exists, or when the
# filesystem does not maintain atime — so it is safe to wire into a gate.
set -euo pipefail

LIMIT_GB="${SWEEP_LIMIT_GB:-20}"
DAYS="${SWEEP_DAYS:-3}"

# Sub-day windows are not merely aggressive, they are wrong under Linux's default `relatime`, which
# refreshes atime at most once per 24h (see above). Refuse rather than silently evict live artifacts.
if ! [ "$DAYS" -ge 1 ] 2>/dev/null; then
    echo "✗ sweep-stale: SWEEP_DAYS must be an integer ≥ 1 (got '${DAYS}') — atime has 24h granularity under relatime" >&2
    exit 1
fi

# The two cargo caches this repo owns. `app/src-tauri` is out-of-workspace, hence its own target.
# A git worktree's target lives under that worktree and goes away with it.
TARGETS=()
for d in target app/src-tauri/target; do
    [ -d "$d" ] && TARGETS+=("$d")
done
if [ ${#TARGETS[@]} -eq 0 ]; then
    exit 0
fi

# `du -sk` is the portable spelling (BSD and GNU both have -k); -g / --block-size are not.
total_kb=$(du -sk "${TARGETS[@]}" | awk '{s += $1} END {print s + 0}')
total_gb=$((total_kb / 1024 / 1024))

if [ "$total_gb" -lt "$LIMIT_GB" ]; then
    echo "→ sweep-stale: build caches ${total_gb}GB < ${LIMIT_GB}GB — nothing to do"
    exit 0
fi

# atime guard. On a filesystem mounted `noatime` (or any layer that does not maintain access
# times) every file looks untouched, so the find below would degenerate into `cargo clean` and
# force a cold rebuild of everything. Probe it once: write a file, read it back, and require the
# read to move atime past mtime. If it does not, do nothing and say why.
#
# The probe passes under `relatime` too — the fresh file's atime equals its mtime, which is exactly
# the case relatime does write through. It therefore proves "atime is maintained", not "atime is
# updated on every read"; the DAYS >= 1 guard above is what makes the weaker guarantee sufficient.
probe="${TARGETS[0]}/.sweep-atime-probe"
printf 'probe' > "$probe"
sleep 1
cat "$probe" > /dev/null
# Probe GNU first: `stat -c` is rejected by BSD stat, whereas `stat -f` *succeeds* on GNU with a
# completely different meaning (--file-system), so testing for `-f` first would silently read
# filesystem stats as timestamps.
if ! times=$(stat -c '%X %Y' "$probe" 2>/dev/null); then
    times=$(stat -f '%a %m' "$probe")                       # BSD / macOS
fi
read -r atime mtime <<< "$times"
rm -f "$probe"
if [ "$atime" -le "$mtime" ]; then
    echo "→ sweep-stale: this filesystem does not update atime — skipping (would delete live artifacts)"
    exit 0
fi

echo "→ sweep-stale: build caches ${total_gb}GB ≥ ${LIMIT_GB}GB — dropping files unread for >${DAYS}d"
# `-mindepth`, `-atime +N` and `-empty` are supported by both BSD and GNU find. Deleting a file whose
# sibling fingerprint survives is fine: cargo sees the missing output and rebuilds that unit.
# `-mindepth 1` keeps the target dir itself from being swept away once it empties out.
find "${TARGETS[@]}" -mindepth 1 -type f -atime "+${DAYS}" -delete
find "${TARGETS[@]}" -mindepth 1 -type d -empty -delete 2>/dev/null || true

after_kb=$(du -sk "${TARGETS[@]}" | awk '{s += $1} END {print s + 0}')
echo "→ sweep-stale: $(( (total_kb - after_kb) / 1024 ))MB freed, $(( after_kb / 1024 / 1024 ))GB left"
