#!/usr/bin/env bash
# verify-network-watch-mac.sh — the macOS half of the network-FS check. Stands up a REAL
# SMB server, mounts it back over the loopback, and runs the one test that needs a real network
# mount: `store_watch::network_dir_picks_poll_and_wakes`.
#
# Why the Linux script (verify-network-watch.sh) doesn't cover this: `is_network_dir` is a
# different implementation per OS. macOS has no magic table — it asks the kernel whether the
# volume has local media (`statfs`'s MNT_LOCAL). Linux staying green says nothing about that
# branch, and getting it wrong means the GUI arms FSEvents on a network volume and then silently
# never sees another host's writes.
#
# The SMB server is a samba container, NOT macOS File Sharing: turning that on is a trip through
# System Settings (a human), docker is not. Nothing leaves the machine — the share is published
# on 127.0.0.1 only, and mount_smbfs needs no sudo.
#
#   scripts/verify-network-watch-mac.sh          (or: make verify-network-mac)
#
# Needs docker (as `make verify-network-linux` does) and port 445 free: macOS File Sharing holds
# it while it is on, so turn that off (System Settings → General → Sharing → File Sharing).
set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
CONTAINER=amenbo-smb-server
MNT="${TMPDIR:-/tmp}/amenbo-netfs-smb"
SMB_USER=amenbo
SMB_PASS=amenbo-test-pw   # a throwaway loopback share; nothing reaches the network

[ "$(uname -s)" = "Darwin" ] || { echo "✗ macOS only (for Linux use make verify-network-linux)"; exit 1; }
command -v docker >/dev/null 2>&1 || { echo "✗ docker is required"; exit 1; }
if nc -z -G 2 127.0.0.1 445 2>/dev/null; then
  echo "✗ port 445 is taken (is macOS file sharing ON?). Turn File Sharing OFF under System Settings →"
  echo "  General → Sharing before running (this verification stands up its own samba on 127.0.0.1:445)."
  exit 1
fi

cleanup() {
  umount "$MNT" 2>/dev/null || true
  docker rm -f "$CONTAINER" >/dev/null 2>&1 || true
  rmdir "$MNT" 2>/dev/null || true
}
trap cleanup EXIT
cleanup   # a previous run that died hard
mkdir -p "$MNT"

echo "== SMB: serve a share from a samba container on 127.0.0.1:445"
docker run -d --name "$CONTAINER" -p 127.0.0.1:445:445 debian:bookworm-slim bash -c "
set -e
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq && apt-get install -y -qq --no-install-recommends samba samba-common-bin >/dev/null
mkdir -p /srv/share && chmod 777 /srv/share
cat > /etc/samba/smb.conf <<EOF
[global]
   workgroup = WORKGROUP
   security = user
   smb ports = 445
   log level = 0
[amenbo]
   path = /srv/share
   read only = no
   guest ok = no
   valid users = $SMB_USER
EOF
useradd -M -s /usr/sbin/nologin $SMB_USER
printf '%s\n%s\n' '$SMB_PASS' '$SMB_PASS' | smbpasswd -s -a $SMB_USER >/dev/null
exec smbd -F --no-process-group
" >/dev/null

echo "== mount it back with mount_smbfs (no sudo — a user mount)"
# The container installs samba on first boot (~20s), and waiting on the port does NOT wait for
# that: docker publishes 445 the moment the container starts, so it answers long before smbd
# does. Retry the mount itself — the only thing that proves the server is really serving.
mounted=0
for _ in $(seq 30); do
  if mount_smbfs "//$SMB_USER:$SMB_PASS@127.0.0.1/amenbo" "$MNT" 2>/dev/null; then mounted=1; break; fi
  docker ps -q -f "name=$CONTAINER" | grep -q . || { docker logs "$CONTAINER"; echo "✗ samba died"; exit 1; }
  sleep 3
done
[ "$mounted" = 1 ] || { docker logs "$CONTAINER" | tail -5; echo "✗ could not mount the SMB share"; exit 1; }
# What the kernel now calls this volume (is_network_dir asks it the same question: MNT_LOCAL).
mount | grep -F " on $(cd "$MNT" && pwd -P) " | sed 's/^/  /' || true

# The app crate's build.rs verifies the CLI sidecar (externalBin) exists; `cargo test` alone does
# not stage it (tauri's beforeBuildCommand does), so stage it the same way `make test` does.
node "$REPO/app/scripts/prepare-cli-sidecar.mjs"

echo ""
echo "== store_watch on the SMB mount ($MNT)"
# Asserts the mount is CALLED network (poll, not FSEvents) and then that a write from a separate
# process actually wakes the watcher.
AMENBO_TEST_NETWORK_DIR="$MNT" \
  cargo test --manifest-path "$REPO/app/src-tauri/Cargo.toml" --test store_watch \
    -- --ignored --nocapture --exact network_dir_picks_poll_and_wakes

echo ""
echo "→ macOS reads the SMB mount as a network volume, and it woke the watcher"
