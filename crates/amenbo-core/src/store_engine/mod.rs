//! The SQLite truth-source engine.
//!
//! Each field write UPSERTs straight into its indexed read-model table: a single local store, so
//! SQLite's write serialisation is the total order and the last write to a field simply wins.
//!
//! This engine is the store's sole truth source: [`crate::store`] reads through [`read`]'s indexed
//! SQL and writes through [`write::WriteTx`] — **one logical operation = one transaction**, issued
//! straight against the read-model tables.
//!
//! [`hydrate`] raises a whole `Database` out of the read-model on demand: [`crate::archive`]
//! proves a snapshot is complete that way, and the parity tests take their in-memory oracle from it.

mod engine;
pub mod hydrate;
pub mod migrate;
pub mod outbox;
pub mod queue;
pub mod read;
pub mod record;
pub mod runner;
pub mod schema;
#[cfg(test)]
pub mod schema_frozen;
pub mod sql;
pub mod write;

pub use engine::{
    at_rest_status, AtRestStatus,
    probe_format_stamp, probe_format_version, probe_is_database, probe_is_empty, probe_is_legacy_keyed,
    probe_is_populated, probe_live_projects,
    read_format_stamp, read_format_version, read_meta, FormatStamp,
    Result, RowChange,
    StoreEngine, StoreEngineError, CHANGE_FEED_RETAIN,
};
pub use hydrate::hydrate_database;
pub use outbox::{events_since, outbox_head, EventRow, OutboxRow, OutboxSlice};
pub use queue::{
    backlog, dequeue, queued_count, queued_for, queued_plugins, QueueDepth, QueueRow, QueuedEvent,
};
pub use runner::{lease_of, Lease};
pub use read::{
    decision_page, hydrate_task_cards, list_task_ids,
    project_name, project_overview, status_bucket_ids,
    task_title, waiting_on_start, DecisionPage,
    ProjectRow, StatusBucketIds, TaskPage, TaskQuery,
};
pub use record::Record;
pub use write::WriteTx;

/// `store_meta` keys for the store-level singleton scalars (see [`StoreEngine::set_meta`]).
pub const META_SCHEMA_VERSION: &str = "schema_version";
/// The monotonic store **format version** scalar. Unlike the frozen `schema_version` ("1"),
/// this integer is bumped only by a migration that destroys old readers (drops/renames a column or
/// table the old side's read SQL needs), so a lagging binary can detect a store that a newer amenbo
/// forward-migrated past what it understands. Written on the write-path open; **missing = v0**
/// (compat baseline — stores predating the guard, no flag day). See [`crate::model::FORMAT_VERSION`]
/// and [`engine::read_format_version`].
pub const META_FORMAT_VERSION: &str = "format_version";
/// The **app version** (semver) that stamped [`META_FORMAT_VERSION`] — what the too-new gate names when
/// it refuses a store this build cannot open. An app that stamped a format version is by
/// definition an app that can open a store at it, so this is the accurate thing to point a stranded user
/// at, and it needs no network to know. A store written before
/// this key existed carries none, and the gate falls back to "reinstall from the latest installer".
pub const META_FORMAT_VERSION_SET_BY: &str = "format_version_set_by";
