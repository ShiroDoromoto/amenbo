//! The semantic-event layer: the store's raw change feed, read as named lifecycle events.
//!
//! amenbo's job at the edge of a plugin is to *fire events* — `task.created`, `comment.added`, and
//! their kin — and let the plugin decide what to do. This module is where a raw change ("row 42 of
//! `task` was inserted") becomes such an event and is handed to whoever is listening.
//!
//! It reads the **change feed**, and only the change feed. That feed is the one seam every committed
//! write passes through — a task moved from the CLI and a task moved from the GUI land the same feed
//! row — so an event fired from here is fired once, route-independent, whatever drove the write. That
//! is the whole reason to sit on the feed rather than on the CLI/GUI call sites, which would each need
//! their own emit and could not see the `ON DELETE CASCADE` children SQLite touches on their behalf.
//!
//! This is the plumbing, not the full catalog:
//!
//! - [`classify`] names the events a feed row settles **on its own** — an `insert` or a `delete` says
//!   what happened with no further reading. The events an `update` splits into (a task's status change
//!   vs. its completion vs. a reassignment vs. a move; a decision accepted vs. rejected) need the row's
//!   *new state* to tell apart, so they are not named here — the mapping that reads that state, and the
//!   payload each event carries, are layered on top of this seam.
//! - [`EventSink`] is the destination trait. This layer runs *after* the change is committed and
//!   durable, so a sink cannot fail the write; the sink that actually launches a plugin subprocess is
//!   built against this trait separately.
//! - [`EventLayer::pump`] drains the feed from a cursor, translates each change, and fans it out to the
//!   sinks — the same reconcile-don't-replay contract the GUI's feed reader follows.

use rusqlite::Connection;

use crate::store_engine::read::{self, FeedRow, FeedSlice};

/// The event names amenbo fires, each a namespace string of the form `<entity>.<verb>`. The string is
/// the event's *type* — a plugin dispatches on it — so these constants are the one source of truth for
/// the names, shared with the payload layer built above.
///
/// Only the events a feed row names on its own live here (see [`classify`]); the ones an `update`
/// disambiguates are added alongside the state-reading that tells them apart.
pub mod name {
    /// A task was created.
    pub const TASK_CREATED: &str = "task.created";
    /// A task was deleted.
    pub const TASK_DELETED: &str = "task.deleted";
    /// A comment was added to a task.
    pub const COMMENT_ADDED: &str = "comment.added";
}

/// A lifecycle event, ready to hand to a sink.
///
/// The skeleton carries only what a feed row settles route-independently: which event, and the id of
/// the record it happened to. The full payload a plugin receives — the actor and the timestamp, and the
/// record's new state — is filled in by the layer above this one; those are not in the feed row (the
/// feed is deliberately actor-free), so naming them is a separate concern from firing the event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    /// The event's namespace name, e.g. [`name::TASK_CREATED`].
    pub name: &'static str,
    /// The affected record's id — the conversational number a reader knows it by.
    pub id: i64,
}

/// A destination for fired events. The layer hands every event to every registered sink.
///
/// A sink runs after the write is committed, so it must not — and structurally cannot — fail the write.
/// `Send + Sync` because a sink may be driven off the write path (a plugin subprocess is launched
/// asynchronously, fire-and-forget).
pub trait EventSink: Send + Sync {
    /// Handle one fired event.
    fn emit(&self, event: &Event);
}

/// The semantic-event layer: a cursor into the change feed, and the sinks each translated event goes to.
///
/// One [`EventLayer`] is one reader of the feed, with its own cursor — independent of any other reader
/// (the GUI keeps its own). Drive it by calling [`pump`](Self::pump) after writes; it fires an event at
/// most once, and never re-fires one it has already passed.
pub struct EventLayer {
    sinks: Vec<Box<dyn EventSink>>,
    cursor: i64,
}

impl EventLayer {
    /// A layer that will fire events for changes *after* `cursor`. Take the cursor from
    /// [`read::change_feed_head`] before the writes you want to observe, so nothing that lands in
    /// between is missed.
    pub fn new(cursor: i64) -> Self {
        Self { sinks: Vec::new(), cursor }
    }

    /// Register a sink. Builder-style, so a layer can be assembled in one expression.
    #[must_use]
    pub fn with_sink(mut self, sink: Box<dyn EventSink>) -> Self {
        self.sinks.push(sink);
        self
    }

    /// The feed id this layer has consumed through — everything at or below it has been fired (or was a
    /// change with no event of its own).
    pub fn cursor(&self) -> i64 {
        self.cursor
    }

    /// Drain the feed from the cursor, fire an event for each change that names one, and advance the
    /// cursor past everything seen. Returns how many events were fired.
    ///
    /// `page` bounds a single feed read; the pump loops until the feed is caught up, so one call
    /// processes all pending changes however many pages that takes. If the cursor has fallen behind feed
    /// truncation, the feed can no longer say what changed, so the pump resyncs the cursor to the feed
    /// head and fires nothing — replaying stale ids would be worse than the gap. It re-reads the store on
    /// the next real change, which is the only honest thing to do.
    pub fn pump(&mut self, conn: &Connection, page: i64) -> crate::store_engine::Result<usize> {
        let mut fired = 0;
        loop {
            match read::changes_since(conn, self.cursor, page)? {
                FeedSlice::Changes { rows, more } => {
                    for row in &rows {
                        self.cursor = row.id;
                        if let Some(event) = classify(row) {
                            for sink in &self.sinks {
                                sink.emit(&event);
                            }
                            fired += 1;
                        }
                    }
                    if !more {
                        break;
                    }
                }
                FeedSlice::Gap => {
                    self.cursor = read::change_feed_head(conn)?;
                    break;
                }
            }
        }
        Ok(fired)
    }
}

/// The event a single feed row names, or `None` when it names none in v1.
///
/// This is the route-independent half of the mapping: an `insert` or a `delete` says what happened
/// outright. An `update` does not — the same row-touch backs a task's status change, its completion, a
/// reassignment and a move — so those return `None` here and are named by the state-reading layer above.
pub fn classify(row: &FeedRow) -> Option<Event> {
    let name = match (row.dataset.as_str(), row.op.as_str()) {
        ("task", "insert") => name::TASK_CREATED,
        ("task", "delete") => name::TASK_DELETED,
        ("task_comment", "insert") => name::COMMENT_ADDED,
        _ => return None,
    };
    Some(Event { name, id: row.row_id })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(dataset: &str, row_id: i64, op: &str) -> FeedRow {
        FeedRow { id: 0, dataset: dataset.to_string(), row_id, op: op.to_string() }
    }

    #[test]
    fn classifies_the_events_a_row_names_on_its_own() {
        assert_eq!(classify(&row("task", 7, "insert")), Some(Event { name: name::TASK_CREATED, id: 7 }));
        assert_eq!(classify(&row("task", 7, "delete")), Some(Event { name: name::TASK_DELETED, id: 7 }));
        assert_eq!(
            classify(&row("task_comment", 42, "insert")),
            Some(Event { name: name::COMMENT_ADDED, id: 42 })
        );
    }

    #[test]
    fn leaves_updates_and_unmapped_datasets_to_the_layer_above() {
        // An update needs the row's new state to name — not this layer's job.
        assert_eq!(classify(&row("task", 7, "update")), None);
        assert_eq!(classify(&row("decision", 3, "update")), None);
        // A dataset with no v1 event, and a cascade child a delete touches, are silent here.
        assert_eq!(classify(&row("dependency", 5, "delete")), None);
        assert_eq!(classify(&row("project", 1, "insert")), None);
    }
}
