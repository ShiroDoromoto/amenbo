//! Exercises store watching (`store_watch`) **against real OS behaviour**. Kernel-driven wakeups (macOS:
//! FSEvents / Linux: inotify / Windows: ReadDirectoryChangesW) say nothing merely by compiling, so these tests
//! make **real writes to a real filesystem** and watch what happens. Four properties are at stake: does a write
//! from another process wake us; does the watch survive the file being replaced wholesale; does it degrade to
//! polling where no kernel watch can be established; and does it stay asleep when idle (the whole point of
//! moving to wakeups).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

use app_lib::store_watch;

/// How long to wait for a wakeup. Locally it lands in tens of milliseconds; this leaves enough room that a slow
/// shared CI runner does not fail spuriously.
const WAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// Grace period for the watch to be armed (a write made before `watch()` is in place is lost on every OS).
const ARM: Duration = Duration::from_millis(500);

/// How long to wait for a wakeup across a network volume (`network_dir_picks_poll_and_wakes`). Orders of
/// magnitude looser than the local budget: the client's attribute cache (3–60 s by NFS default) keeps handing
/// stat the stale attributes, so polling spins through cycle after cycle without noticing the change.
const NETWORK_WAKE_TIMEOUT: Duration = Duration::from_secs(120);

/// Run `store_watch::run` on its own thread and hand back a channel that receives one item per wakeup (`run` never returns).
fn spawn_run(dir: Option<&Path>) -> Receiver<()> {
    let (tx, rx) = channel::<()>();
    let dir: Option<PathBuf> = dir.map(Path::to_path_buf);
    std::thread::spawn(move || {
        store_watch::run(dir.as_deref(), move || {
            let _ = tx.send(());
        })
    });
    std::thread::sleep(ARM);
    rx
}

/// Append to a file from **another process** (what a CLI/AI write looks like from the GUI's side). Writing with
/// this process's own `std::fs` risks proving nothing more than "our own write came back to us", so the write
/// always comes from outside.
fn external_append(path: &Path) {
    let p = path.display().to_string();
    #[cfg(windows)]
    let mut cmd = {
        // Redirection is `cmd`'s command-line syntax, not an argument, so Rust's argument escaping mangles it
        // (`>>` never reaches cmd). Hand over the raw command line instead.
        use std::os::windows::process::CommandExt;
        let mut c = Command::new("cmd");
        c.raw_arg(format!("/C echo x>>\"{p}\""));
        c
    };
    #[cfg(not(windows))]
    let mut cmd = {
        let mut c = Command::new("sh");
        c.arg("-c").arg(format!("printf x >> '{p}'"));
        c
    };
    let status = cmd.status().expect("could not launch the external process");
    assert!(status.success(), "the external process's write failed");
}

/// A write from another process wakes us — the OS-side entrance to the path by which another process's changes
/// reach the GUI.
#[test]
fn wakes_on_external_process_write() {
    let dir = tempfile::tempdir().unwrap();
    let store = dir.path().join("store.sqlite");
    std::fs::write(&store, b"seed").unwrap();

    let rx = spawn_run(Some(dir.path()));
    external_append(&store);

    rx.recv_timeout(WAKE_TIMEOUT)
        .expect("an external process's write did not wake us");
}

/// The watch survives the file being replaced wholesale (checkpoint / `stage_and_swap` / fold all swap the inode).
/// Watching the *directory* is what makes that possible — a per-file watch would go blind the moment the swap lands.
#[test]
fn survives_file_replacement() {
    let dir = tempfile::tempdir().unwrap();
    let store = dir.path().join("store.sqlite");
    std::fs::write(&store, b"old").unwrap();

    let rx = spawn_run(Some(dir.path()));

    // The swap: build the new file alongside, then rename it over the old one.
    let staged = dir.path().join("store.sqlite.new");
    std::fs::write(&staged, b"new").unwrap();
    std::fs::rename(&staged, &store).unwrap();
    rx.recv_timeout(WAKE_TIMEOUT)
        .expect("the replacement did not wake us");
    while rx.recv_timeout(ARM).is_ok() {} // Drain the rest of the events the swap fired

    // Here is what matters: a write **after** the swap still wakes us.
    external_append(&store);
    rx.recv_timeout(WAKE_TIMEOUT)
        .expect("the watch is dead after the swap (still blind to the file that replaced the old one)");
}

/// The degraded path: exactly where `spawn_store_watcher` lands on filesystems with no working kernel watch
/// (containers, NFS). Even on stat polling, a wakeup arrives within a few poll intervals at worst.
#[test]
fn poll_watcher_wakes_when_kernel_watch_is_unavailable() {
    let dir = tempfile::tempdir().unwrap();
    let store = dir.path().join("store.sqlite");
    std::fs::write(&store, b"seed").unwrap();

    let (tx, rx) = channel::<()>();
    let _watcher = store_watch::testonly::spawn_poll_watcher(dir.path(), tx)
        .expect("not even a poll watcher could be established");

    // Polling can only say "did this differ from the previous scan", so anything written **before the first
    // scan** has nothing to be compared against and goes unnoticed — the inherent coarseness of the degraded
    // path versus a kernel watch. Real writes are never a single event either, so keep writing until it wakes.
    let deadline = std::time::Instant::now() + WAKE_TIMEOUT;
    loop {
        external_append(&store);
        if rx.recv_timeout(store_watch::testonly::POLL_INTERVAL * 2).is_ok() {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the degraded path (stat polling) never woke us"
        );
    }
}

/// A local filesystem keeps the kernel watch — i.e. the network-filesystem detection is not so eager that
/// ordinary setups get dropped to polling. (The network side of that decision needs a real mount, so it lives in
/// `network_dir_picks_poll_and_wakes`.)
#[test]
fn local_dir_keeps_the_kernel_watcher() {
    let dir = tempfile::tempdir().unwrap();
    assert!(
        !store_watch::testonly::picks_poll_watcher(dir.path()),
        "polling was chosen on a local filesystem (that kills the wakeup design wholesale)"
    );
}

/// On a **real network volume**, polling is chosen and an outside write wakes us. The network-filesystem test
/// (`is_network_dir`) is a different beast on every OS (macOS: `MNT_LOCAL` / Windows: UNC plus `GetDriveTypeW` /
/// Linux: `statfs` magic), and **a test that only ever touches a local filesystem can only exercise one half of
/// it** — it confirms "local is called local" and never reaches the part that matters, "network is called
/// network". A real mount is required, so this does not run by default: only when `AMENBO_TEST_NETWORK_DIR`
/// points at one. Two of the three OSes have a way to provide it here (`make verify-network-linux` /
/// `verify-network-mac`, each standing up a real server and handing over a real mount; Windows needs real
/// hardware, so its counterpart is a maintainer's local target). The elapsed time is
/// printed too, because across a network the client's attribute cache (NFS's `acregmin` and friends) sits in the
/// way and tightening the poll interval does not shorten it. The guarantee is that we notice, not that we notice
/// promptly.
#[test]
#[ignore = "needs a real network mount (pass one via AMENBO_TEST_NETWORK_DIR)"]
fn network_dir_picks_poll_and_wakes() {
    let Some(dir) = amenbo_core::env::test_network_dir() else {
        panic!("pass a directory on a network volume via AMENBO_TEST_NETWORK_DIR");
    };
    let dir = PathBuf::from(dir);
    assert!(dir.is_dir(), "{} does not show up as a directory", dir.display());

    // The crux: is this filesystem recognised as a network one? Get that wrong and everything below is a story
    // about a local disk.
    assert!(
        store_watch::testonly::picks_poll_watcher(&dir),
        "a kernel watch was chosen on a network volume ({}) = another host's writes are missed forever",
        dir.display()
    );

    // And that the chosen path actually delivers, entered through `run` exactly as production does.
    let store = dir.join("store.sqlite");
    std::fs::write(&store, b"seed").unwrap();
    let rx = spawn_run(Some(&dir));

    // Polling can only say "did this differ from the previous scan", so a write made before the first scan has
    // nothing to be compared against. Keep writing until it wakes (same reason as
    // `poll_watcher_wakes_when_kernel_watch_is_unavailable`).
    let start = std::time::Instant::now();
    let deadline = start + NETWORK_WAKE_TIMEOUT;
    loop {
        external_append(&store);
        if rx.recv_timeout(store_watch::testonly::POLL_INTERVAL * 2).is_ok() {
            println!(
                "woke after {:?} (the attribute cache can hold this back)",
                start.elapsed()
            );
            let _ = std::fs::remove_file(&store);
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "a write on the network volume did not wake us"
        );
    }
}

/// With nowhere to place a watch (the store does not exist yet), `run` falls back to a stat loop and keeps
/// checking on a cycle — it never gives up. Whether anything actually changed is the caller's signature check to
/// make, so waking up for nothing is fine here.
#[test]
fn falls_back_to_stat_loop_when_there_is_no_store() {
    let rx = spawn_run(None);
    let budget = store_watch::testonly::POLL_INTERVAL * 3;
    rx.recv_timeout(budget).expect("the stat loop is not running");
    rx.recv_timeout(budget)
        .expect("the stat loop stopped after one cycle");
}

/// When idle, we stay asleep — nothing shakes us awake every 1.5 seconds. This is the whole point of the wakeup
/// design: regress to an implementation that burns CPU while nothing is happening and this test fails.
#[test]
fn idle_does_not_wake() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("store.sqlite"), b"seed").unwrap();

    let rx = spawn_run(Some(dir.path()));
    // The write that seeds the store, made just before the watch is armed, can still wake us. A spurious wakeup
    // is **by design** — whether anything really changed is the caller's signature check. So the question is
    // whether we sleep *once things have settled*.
    while rx.recv_timeout(ARM).is_ok() {}

    let idle = store_watch::testonly::POLL_INTERVAL * 2;
    assert!(
        rx.recv_timeout(idle).is_err(),
        "woke once settled with nobody writing (we are back to polling)"
    );
}
