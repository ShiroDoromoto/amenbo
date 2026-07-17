#!/usr/bin/env bash
# verify-network-watch.sh — stands up REAL network filesystems and runs the one test that
# needs them: `store_watch::network_dir_picks_poll_and_wakes`.
#
# Why this exists: `is_network_dir` decides between kernel watching and polling by reading
# the filesystem's `statfs` magic, and getting it wrong means the GUI silently never sees
# another host's writes (inotify ARMS on NFS and then reports nothing from the server
# side). A table of magic numbers is only as good as the filesystems it has been held
# against, so the test is `#[ignore]`d until something hands it a real mount. This is that
# something: NFS, CIFS/SMB and 9P, each mounted from a server running on this machine.
#
# What it actually holds the table against: NFS (0x6969), SMB — the kernel mounts today's SMB3
# as smb2 (0xFE534D42) — and 9P (0x01021997), which is what WSL puts the Windows drives on, so
# it is a path real users stand on. The remaining rows (the legacy CIFS magic, AFS, Ceph) are
# still only asserted by the table, never by a mount: the legacy magic needs a server with SMB1
# switched back on, and AFS/Ceph cost a cluster to stand up for a single number. They stay in
# the table because a wrong "local" verdict silently loses another host's writes, and a wrong
# "network" verdict only costs polling.
#
# Needs Linux and root (mount(8), and the nfsd/cifs/9p kernel filesystems). It runs on the CI
# runner, and locally inside a privileged Linux container — nothing here is specific to
# either.
#
# This is the LINUX half only: `is_network_dir` is a different implementation per OS, so the
# other two have their own script (verify-network-watch-mac.sh / -win.ps1) — Linux being
# green says nothing about them.
#
#   scripts/verify-network-watch.sh [path/to/store_watch-<hash>]
#
# Hand it a pre-built test binary and it runs that. CI does, because it builds as the normal
# user: a `cargo test` under sudo would put root-owned artifacts in the cached target dir.
# With no argument it falls back to `cargo test` (what a local container run wants).
set -euo pipefail

TEST_BIN="${1:-}"

REPO="$(cd "$(dirname "$0")/.." && pwd)"
MNT_ROOT=/mnt/amenbo-netfs
EXPORT_ROOT=/srv/amenbo-export
SMB_USER=amenbo
SMB_PASS=amenbo-test-pw   # a throwaway loopback share; nothing reaches the network
P9_PORT=5640              # diod's default; the share is loopback-only too

[ "$(uname -s)" = "Linux" ] || { echo "✗ Linux only (mounting real network FS)"; exit 1; }
[ "$(id -u)" = "0" ] || { echo "✗ must run as root (mount(8))"; exit 1; }

BACKING_IMG=/var/tmp/amenbo-nfs-backing.img

cleanup() {
  umount "$MNT_ROOT"/* 2>/dev/null || true
  exportfs -ua 2>/dev/null || true
  pkill smbd 2>/dev/null || true
  # Leave no server holding the container open after the mounts are gone.
  pkill diod 2>/dev/null || true
  umount "$EXPORT_ROOT/nfs" 2>/dev/null || true
  rm -f "$BACKING_IMG"
}
trap cleanup EXIT

export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq --no-install-recommends \
  nfs-kernel-server nfs-common samba samba-common-bin cifs-utils diod >/dev/null

mkdir -p "$EXPORT_ROOT"/{nfs,smb,9p} "$MNT_ROOT"/{nfs,cifs,9p}

# The NFS server can only export a filesystem that knows how to hand out file handles, and
# a container's overlayfs does not ("does not support NFS export"). Serve a small ext4 image
# instead — the same path then works on a CI runner and in a container.
echo "== NFS: back the export with an ext4 image (overlayfs cannot be exported)"
truncate -s 128M "$BACKING_IMG"
mkfs.ext4 -q "$BACKING_IMG"
mount -o loop "$BACKING_IMG" "$EXPORT_ROOT/nfs"
chmod 777 "$EXPORT_ROOT"/nfs "$EXPORT_ROOT"/smb

echo "== NFS: export $EXPORT_ROOT/nfs and mount it back over the loopback"
# no_subtree_check keeps exportfs quiet; insecure allows the client's high source port.
echo "$EXPORT_ROOT/nfs 127.0.0.1(rw,sync,no_subtree_check,no_root_squash,insecure,fsid=0)" > /etc/exports
rpcbind || true
exportfs -ra
# The kernel server needs nfsd(7) mounted; starting the service does it, but in a container
# there is no init, so raise the daemons directly.
mount -t nfsd nfsd /proc/fs/nfsd 2>/dev/null || true
rpc.nfsd 8
rpc.mountd
mount -t nfs -o vers=4,nolock 127.0.0.1:/ "$MNT_ROOT/nfs" \
  || mount -t nfs -o vers=3,nolock 127.0.0.1:"$EXPORT_ROOT/nfs" "$MNT_ROOT/nfs"

echo "== SMB: serve $EXPORT_ROOT/smb with samba and mount it back as cifs"
cat > /etc/samba/smb.conf <<EOF
[global]
   workgroup = WORKGROUP
   security = user
   smb ports = 445
   log level = 0
[amenbo]
   path = $EXPORT_ROOT/smb
   browseable = yes
   read only = no
   guest ok = no
   valid users = $SMB_USER
EOF
id "$SMB_USER" >/dev/null 2>&1 || useradd -M -s /usr/sbin/nologin "$SMB_USER"
printf '%s\n%s\n' "$SMB_PASS" "$SMB_PASS" | smbpasswd -s -a "$SMB_USER" >/dev/null
smbd -D
sleep 1
mount -t cifs "//127.0.0.1/amenbo" "$MNT_ROOT/cifs" \
  -o "username=$SMB_USER,password=$SMB_PASS,vers=3.0,uid=0,gid=0"

# 9P is not an exotic row in the table: it is what WSL mounts the Windows drives on, so a user
# keeping a store under /mnt/c stands on exactly this. diod serves it over TCP.
#
# The 9p client is a kernel filesystem; a kernel built without it cannot be argued with. That is
# the ONE reason this leg may be skipped — and it says so out loud, because a check that quietly
# drops a filesystem reads exactly like a check that held it. Anything else (no diod, a mount
# that refuses) is a real failure and stops the run.
FS_LIST="nfs cifs"
modprobe 9p 2>/dev/null || true
if grep -qw 9p /proc/filesystems; then
  echo "== 9P: serve $EXPORT_ROOT/9p with diod and mount it back over the loopback"
  chmod 777 "$EXPORT_ROOT/9p"
  diod -f -l "127.0.0.1:$P9_PORT" -e "$EXPORT_ROOT/9p" -n -N >/tmp/diod.log 2>&1 &
  sleep 2
  mount -t 9p 127.0.0.1 "$MNT_ROOT/9p" \
    -o "trans=tcp,port=$P9_PORT,version=9p2000.L,uname=root,access=user,msize=65536,aname=$EXPORT_ROOT/9p" \
    || { echo "✗ 9P mount failed"; cat /tmp/diod.log; exit 1; }
  FS_LIST="$FS_LIST 9p"
else
  echo "⚠ SKIPPED 9P: this kernel has no 9p filesystem — the 0x01021997 row goes UNHELD on this run"
fi

echo ""
echo "== what the kernel says these are (the statfs magic is_network_dir matches on)"
for fs in $FS_LIST; do stat -f -c '  %n: type=%T magic=0x%t' "$MNT_ROOT/$fs"; done

# The `cargo test` path builds the app crate, and tauri's build script refuses to start without
# the CLI sidecar for this triple (it is staged by the bundling step, which a test run never
# does). Stage a debug CLI there so a local container run is not blocked on a packaging artifact.
# CI hands us a pre-built test binary and skips all of this.
if [ -z "$TEST_BIN" ]; then
  triple="$(rustc -vV | awk '/^host:/{print $2}')"
  sidecar="$REPO/app/src-tauri/binaries/amenbo-$triple"
  if [ ! -f "$sidecar" ]; then
    echo "== stage the CLI sidecar tauri's build script insists on ($(basename "$sidecar"))"
    cargo build --manifest-path "$REPO/Cargo.toml" -p amenbo-cli
    mkdir -p "$(dirname "$sidecar")"
    cp "${CARGO_TARGET_DIR:-$REPO/target}/debug/amenbo" "$sidecar"
  fi
fi

# One run of the ignored test per mount: it asserts the mount is CALLED network (poll, not
# kernel watch) and then that a write from a separate process actually wakes the watcher.
for fs in $FS_LIST; do
  echo ""
  echo "== store_watch on $fs ($MNT_ROOT/$fs)"
  if [ -n "$TEST_BIN" ]; then
    AMENBO_TEST_NETWORK_DIR="$MNT_ROOT/$fs" \
      "$TEST_BIN" --ignored --nocapture --exact network_dir_picks_poll_and_wakes
  else
    AMENBO_TEST_NETWORK_DIR="$MNT_ROOT/$fs" \
      cargo test --manifest-path "$REPO/app/src-tauri/Cargo.toml" --test store_watch \
        -- --ignored --nocapture --exact network_dir_picks_poll_and_wakes
  fi
done

echo ""
echo "→ these all read as network filesystems, and all woke the watcher: $FS_LIST"
