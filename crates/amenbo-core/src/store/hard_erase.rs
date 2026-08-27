//! Hard-erase: the store's native capability to make specific content *physically gone* from the truth
//! source — removed from the file, its pages returned to the OS, unrecoverable.
//!
//! The truth source is plaintext SQLite (on-device secrecy is delegated to OS full-disk encryption). With
//! no cipher layer masking freed-but-unreclaimed pages, the physical DELETE / in-place overwrite *and the
//! reclaiming VACUUM below* are what actually make the bytes gone.
//!
//! The read model is the store's only copy of a value. Even so, ordinary commands cannot make content
//! *gone*: a delete is physical (and a comment posted by mistake has its own delete — `comment rm`), but
//! the freed pages stay in the file with their bytes readable until something reclaims them; and
//! overwriting a decision body in place (`decision edit`) likewise replaces the row but leaves the
//! prior bytes in those freed pages, while supersede keeps the old body in the chain by design.
//! Erasing content therefore needs a deliberate, gated exception.
//!
//! This is a maintenance capability, not an everyday op — a surgery a human runs deliberately. The
//! caller (the CLI) owns the human gate and the backup-first
//! safety; this method does the surgery — DELETE the read-model row / overwrite the field in place,
//! plus the out-of-band bytes an erased comment's attachments held — and the VACUUM that
//! returns the freed pages to the OS.
//!
//! A comment is named by its table, never by its id alone: task comments and decision comments number
//! independently (`AMB-D-377`), so the same number is a live row in both and an erase that guessed which
//! one was meant would destroy the wrong one.

use rusqlite::types::Value;
use serde::Serialize;

use crate::error::{Error, Result};
use crate::model::AttachmentTarget;
#[cfg(any(test, debug_assertions))]
use crate::store_engine::schema::col;
#[cfg(any(test, debug_assertions))]
use crate::store_engine::sql::{Pred, Select, Sql, Table};

use super::Store;

/// One unit of content to physically erase.
///
/// The two comment tables are named apart for the same reason their refs are (`AMB-D-377`): they number
/// independently, so the id alone cannot say which row an erase means — and guessing would destroy the
/// wrong one.
#[derive(Debug, Clone)]
pub enum HardEraseTarget {
    /// Remove a task comment in full — its read-model row.
    TaskComment { id: i64 },
    /// Remove a decision comment in full — its read-model row. A comment's number is not a
    /// conversational one and nothing cites it, so unlike a decision's body (which is redacted in place,
    /// keeping the decision) the row simply goes.
    DecisionComment { id: i64 },
    /// Redact a surviving decision's body: overwrite it with `new_body` in place (the section is
    /// gone; the decision, its number and other fields stay).
    DecisionBody { id: i64, new_body: String },
}

/// What [`Store::hard_erase`] removed: the ids acted on, the total read-model rows physically
/// affected (deleted or overwritten) before the reclaiming VACUUM, and the attachment bytes an erased
/// comment let go of.
#[derive(Debug, Clone, Default, Serialize)]
pub struct HardEraseReport {
    pub task_comments_erased: Vec<i64>,
    pub decision_comments_erased: Vec<i64>,
    pub decisions_redacted: Vec<i64>,
    pub rows_removed: usize,
    /// Blob files deleted because the erased comments were their last reference.
    pub blobs_reclaimed: u64,
    pub bytes_reclaimed: u64,
}

/// The project an erase target belongs to — through [`super::owner`], the same walk the reach checks use,
/// so a target reaches its project one way in this store and not two.
fn owner_of(conn: &rusqlite::Connection, target: &HardEraseTarget) -> Result<Option<i64>> {
    match target {
        HardEraseTarget::TaskComment { id } => super::owner::task_comment(conn, *id),
        HardEraseTarget::DecisionComment { id } => super::owner::decision_comment(conn, *id),
        HardEraseTarget::DecisionBody { id, .. } => super::owner::decision(conn, *id),
    }
}

impl Store {
    /// Physically erase the given targets from the plaintext truth source, then VACUUM so the bytes leave
    /// the file. All targets are validated against the truth source up front — a single unknown id fails
    /// the whole call before anything is touched (no partial erase). Destructive and irreversible; the
    /// caller owns the human gate and the backup-first safety net. Both the validation and the erase read
    /// and write the engine — the store keeps no in-memory copy of the very bytes this call exists to
    /// destroy. The erase goes through the ordinary write seam ([`crate::store_engine::WriteTx`]), so it
    /// lands in the change feed like any other mutation: the GUI is told *which* comment left and *which*
    /// decision was rewritten, instead of learning only that the file moved and re-reading the store. The
    /// feed carries the ids, never the values — the erased text goes nowhere.
    pub fn hard_erase(&mut self, targets: &[HardEraseTarget]) -> Result<HardEraseReport> {
        let live = &self.engine;

        let mut report = HardEraseReport::default();
        let mut orphaned: Vec<String> = Vec::new();
        {
            let tx = live.write()?;

            // Validate every target before mutating anything (all-or-nothing). Inside the transaction, so
            // the row a target names cannot leave between the check and the erase.
            for t in targets {
                let (dataset, id, noun) = match t {
                    HardEraseTarget::TaskComment { id } => ("task_comment", id, "task comment"),
                    HardEraseTarget::DecisionComment { id } => {
                        ("decision_comment", id, "decision comment")
                    }
                    HardEraseTarget::DecisionBody { id, .. } => ("decision", id, "decision"),
                };
                if !crate::store_engine::read::record_exists(tx.conn(), dataset, *id)? {
                    return Err(Error::not_found(format!("{noun} {id}")));
                }
                // This is the one write that does not go through `write_one`, so the sync version it
                // moves is declared here (`AMB-D-582`) — and here, before the erase, is the only place
                // the walk from a comment up to its project can still be made. An erased comment that
                // left the version standing would keep living in every copy carried out of this store,
                // which is the exact opposite of what this capability is for.
                if let Some(project) = owner_of(tx.conn(), t)? {
                    tx.touches_project(project);
                }
            }

            for t in targets {
                match t {
                    // Physically remove the row (a delete is physical); the VACUUM below is what
                    // this adds over an ordinary `comment rm`, returning the freed pages to the OS.
                    //
                    // The comment's attachments go with it: `attachment` is a polymorphic child
                    // (`target_type` / `target_id`), so no FK can cascade it and the deleting op sweeps its
                    // own (the same sweep `comment rm` runs). Erasing the text while the file the
                    // comment carried survives, bytes and all, would betray the very promise this
                    // capability exists for. The bytes are out-of-band, so the sweep only hands back the
                    // hashes it let go of — they are reclaimed after the commit, below.
                    HardEraseTarget::TaskComment { id } => {
                        orphaned.extend(crate::ops::sweep_polymorphic(&tx, AttachmentTarget::TaskComment, *id)?);
                        tx.delete_record("task_comment", *id)?;
                        report.task_comments_erased.push(*id);
                    }
                    HardEraseTarget::DecisionComment { id } => {
                        orphaned.extend(crate::ops::sweep_polymorphic(&tx, AttachmentTarget::DecisionComment, *id)?);
                        tx.delete_record("decision_comment", *id)?;
                        report.decision_comments_erased.push(*id);
                    }
                    // Overwrite the field in place: a field write UPSERTs straight into the read-model
                    // column, so the prior value is replaced — no history is left behind to leak it. This
                    // is how an accepted decision's body loses one section (scrubbed from the file) while the decision stays.
                    HardEraseTarget::DecisionBody { id, new_body } => {
                        tx.set_field("decision", *id, "body", Value::Text(new_body.clone()))?;
                        report.decisions_redacted.push(*id);
                    }
                }
                report.rows_removed += 1;
            }

            tx.commit()?;
        }

        // Reclaim the blob bytes the erased comments were the last reference to. **After** the
        // commit: deleting bytes inside the transaction would lose them for good if the rollback then put
        // the rows back. Blobs are content-addressed, so a hash another live attachment still points at is
        // kept (`Store::reclaim_blobs` asks the read model, per candidate).
        //
        // `min_age` is zero, unlike every other reclaim (`GC_MIN_AGE` spares young bytes in case another
        // process has an attach in flight that has not committed its row yet). An erase cannot wait an hour
        // to take effect — the comment posted minutes ago carrying the file that should never have been
        // attached is the case this capability exists for. The trade is deliberate and small: this is a
        // gated surgery on a quiet store (the CLI takes a backup and asks the human first), and losing a
        // concurrent attach's bytes costs a re-attach, where leaving the erased comment's file on disk
        // costs the erase its meaning. Unlike the delete path this is not best-effort — if the bytes cannot
        // be removed, the caller must hear it rather than be told the content is gone.
        let reclaimed = self.reclaim_blobs(&orphaned, std::time::Duration::ZERO)?;
        report.blobs_reclaimed = reclaimed.removed;
        report.bytes_reclaimed = reclaimed.freed_bytes;

        // Return the freed pages to the OS — a raw DELETE only marks them reusable within the file, and an
        // in-place overwrite can leave the old bytes on a freed page. VACUUM cannot run inside a
        // transaction, so it follows the commit, on the now-autocommit engine connection. It rewrites the
        // whole file, which no change feed can describe — a reader sees the file move and reconciles, and
        // the feed rows the commit above wrote still say what left.
        live.conn().execute("VACUUM", []).map_err(crate::store_engine::StoreEngineError::from)?;
        Ok(report)
    }
}

#[cfg(any(test, debug_assertions))]
impl Store {
    /// Test probe: how many read-model rows carry `needle` in a text column that hard-erase targets
    /// (`task_comment.text` / `decision_comment.text` / `decision.body`). Used by the hard-erase tests to
    /// assert content is *physically* gone from the file.
    pub fn debug_content_containing(&self, needle: &str) -> i64 {
        let like = format!("%{needle}%");
        let (c, dc, d) =
            (col::task_comment::ALL, col::decision_comment::ALL, col::decision::ALL);
        self.count_where(c.table, Pred::like(c.text, like.clone()))
            + self.count_where(dc.table, Pred::like(dc.text, like.clone()))
            + self.count_where(d.table, Pred::like(d.body, like))
    }

    /// Test probe: whether a read-model row exists in `dataset`'s table with id `row` (1 = present,
    /// 0 = gone after a hard-erase).
    pub fn debug_rows_for(&self, dataset: &str, row: i64) -> i64 {
        let Some(ds) = crate::store_engine::schema::dataset(dataset) else { return 0 };
        self.count_where(ds.as_table(), Pred::eq(ds.id_col(), row))
    }

    /// How many rows of `table` the predicate matches — 0 if the query cannot even run (a probe never
    /// fails a test on its own account).
    fn count_where(&self, table: Table, p: Pred) -> i64 {
        let mut sel = Select::new();
        let count = sel.count_all();
        let mut sql = Sql::from(&sel, table);
        sql.push_where(Some(&p));
        self.engine
            .conn()
            .query_row(sql.text(), rusqlite::params_from_iter(sql.params()), |r| count.get(r))
            .unwrap_or(0)
    }
}
