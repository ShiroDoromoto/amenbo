//! Progress + cancellation for the long-running bulk operations — whole-device backup/restore and
//! streaming export. **One** callback shape is shared by every bulk operation so the CLI (line
//! output) and GUI (progress modal) wire against a single type and core owns no presentation. A step
//! reports the phase and a bounded `done/total`; the callback returns [`ControlFlow`] so a consumer
//! can **cancel** at a step boundary.
//!
//! Granularity is deliberately coarse and honest: `VACUUM INTO` and `PRAGMA integrity_check` are
//! single statements that cannot be cancelled mid-run, so cancellation is observed at the phase seams.
//! Loops core owns itself poll far more finely — streaming export per row, [`Phase::Blobs`] and
//! [`Phase::Unpacking`] per attachment.

use std::ops::ControlFlow;

/// The phase a bulk operation is currently in, for the progress line/modal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Writing a store's physical snapshot (`VACUUM INTO`).
    Snapshotting,
    /// Streaming a store's attachment bytes (`blobs/`) into or out of the archive. Unlike the other
    /// whole-device phases this one ticks **per blob** (`done`/`total` count blobs, not stores), because
    /// the bytes — not the store count — are what makes a large archive slow.
    Blobs,
    /// Unpacking an archive's entries into the staging dir (restore). Ticks **per entry written**
    /// (`done`/`total` count the snapshot plus the blobs), for the same reason [`Phase::Blobs`] does: the
    /// attachment bytes are what makes a large archive slow, and this is the phase that writes them all
    /// back out. `total` comes from the manifest — the tar's first entry, read before extraction — so the
    /// count is known before the first byte is unpacked. It is advisory (a blob that vanished mid-backup
    /// is not in the tar), so a run may finish short of its total; it never overruns.
    Unpacking,
    /// Proving a snapshot is usable (bounded: `integrity_check` + schema/COUNT, no full hydrate).
    Verifying,
    /// Streaming rows out to a portable export.
    Exporting,
    /// Copying/placing a snapshot into its live location (restore).
    Copying,
    /// Walking a store up the version chain: `done`/`total` count the **steps** still pending, and
    /// a tick lands at each step's boundary — one step is one transaction, so there is no finer seam to
    /// report. Unlike every other phase this one is **not a cancellation point** (a migration cannot be
    /// abandoned: the store would stay at a version this build cannot open), so a `Break` here is not
    /// honoured — see [`crate::store_engine::migrate::run`].
    Migrating,
}

/// One progress tick.
///
/// A tick carries no label for what it is about: this device holds a single database, and an archive
/// records no name for it either — there is only one thing every tick could be about, so naming it
/// would be repeating the operation's own title back at the reader.
#[derive(Debug, Clone, Copy)]
pub struct Progress {
    /// What the operation is doing right now.
    pub phase: Phase,
    /// Units completed so far (rows written, blobs streamed, …). Monotonic within a phase.
    pub done: u64,
    /// Total units when known (e.g. a pre-counted row total); `None` when unbounded.
    pub total: Option<u64>,
}

/// A callback that observes [`Progress`] and may ask to cancel. Returning [`ControlFlow::Break`] tells
/// the operation to stop at the next boundary and unwind cleanly (partial output removed).
pub type ProgressFn<'a> = &'a mut dyn FnMut(&Progress) -> ControlFlow<()>;

/// A no-op progress sink for callers (and tests) that don't care to observe progress.
pub fn ignore(_p: &Progress) -> ControlFlow<()> {
    ControlFlow::Continue(())
}
