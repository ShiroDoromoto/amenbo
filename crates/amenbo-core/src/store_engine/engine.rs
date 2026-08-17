//! The engine: a SQLite truth-source where every field write UPSERTs directly into an indexed
//! read-model table. The store is a **single local database**, so SQLite's write serialisation is the
//! total order and the last write to a field simply wins — no cross-replica diff/merge is needed.
//! [`set_field`](StoreEngine::set_field) is **generic over every dataset** in [`super::schema`],
//! validating the `(dataset, col)` pair against the registry whitelist before it writes. This engine is
//! the store's **sole truth source** ([`crate::store`] reads and writes through it).

use std::path::Path;

use rusqlite::types::Value;
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;

use super::schema;
use super::schema::col;
use super::search;
use super::sql::{Delete, Exists, Expr, Insert, Pred, Select, Sql, Update};

/// The plain tables this module speaks to, named through the generated identifiers: the store's
/// scalars and the change feed.
const META: col::store_meta::Cols = col::store_meta::ALL;
const FEED: col::change_feed::Cols = col::change_feed::ALL;

/// `SELECT EXISTS(… store_meta WHERE key = ?)` — the "has this scalar been written?" probe, shared by
/// the engine's own [`StoreEngine::is_populated`] and the DDL-free [`probe_is_populated`].
fn meta_exists(conn: &Connection, key: &str) -> rusqlite::Result<bool> {
    let mut sel = Select::new();
    let present = sel.pred(Exists::over(META.table).filter(Pred::eq(META.key, key)).pred());
    let sql = Sql::select(&sel);
    conn.query_row(sql.text(), rusqlite::params_from_iter(sql.params()), |r| present.get(r))
}

/// UPSERT one store-level scalar. `None` clears the value (the key stays, holding NULL — an unset
/// scalar round-trips as absent).
pub(super) fn upsert_meta(conn: &Connection, key: &str, value: Option<&str>) -> rusqlite::Result<usize> {
    Insert::into(META.table)
        .set(META.key, key)
        .set_opt(META.value, value)
        .on_conflict_update(META.key)
        .sql()
        .execute(conn)
}

/// Errors from the engine: SQLite failures, or a message naming a dataset/column the
/// read-model schema does not know (a corrupt or forward-version peer message).
#[derive(Debug, thiserror::Error)]
pub enum StoreEngineError {
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("unknown engine dataset: {0}")]
    UnknownDataset(String),
    #[error("column {col} is not writable on dataset {dataset}")]
    UnknownColumn { dataset: String, col: String },
    #[error("unknown sort key: {0}")]
    InvalidSort(String),
    /// A migration step read the store's own `CREATE TABLE` text and did not find the clause it was
    /// written to rewrite. Refusing is the point: a step that edits a store's declaration in place has to
    /// know exactly what it is editing, and one that guessed would leave the declaration saying something
    /// the table is not.
    #[error("cannot migrate {table}: its stored definition does not carry `{expected}`")]
    UnrecognisedDdl { table: &'static str, expected: &'static str },
    /// The read was asked for something outside the reach it declared. Carried as its own variant so
    /// the `out_of_reach` code survives the crossing into [`crate::error::Error`] — flattened into
    /// `Storage`, the boundary would report a storage failure for a containment refusal.
    #[error("{0}")]
    OutOfReach(crate::error::Error),
}

pub type Result<T> = std::result::Result<T, StoreEngineError>;

/// One record row a transaction touched, as SQLite reported it to the change feed's `update_hook`.
/// Carries the instruction and nothing more: which dataset, which id, which kind of change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowChange {
    /// The dataset's stable key (`task`, `decision`, …) — never the physical table name, which is an
    /// implementation detail the reader does not speak.
    pub dataset: &'static str,
    /// The row's id. Every record table is `INTEGER PRIMARY KEY AUTOINCREMENT`, so SQLite's rowid *is*
    /// the logical id the reader knows.
    pub row_id: i64,
    /// `insert` / `update` / `delete`.
    pub op: &'static str,
}

/// How many feed rows a store keeps. The feed is a window a reader catches up through, not a history: a
/// reader that has been away longer than this is told its cursor is gone and re-reads the store, which
/// costs one reconcile — cheaper than an unbounded table nobody ever prunes. Sized so an ordinary
/// session never sees a gap: a busy day is a few hundred operations, and one operation writes a handful
/// of rows; a reader away longer than that was going to re-read the store on startup anyway.
pub const CHANGE_FEED_RETAIN: i64 = 5_000;

/// How many rows may be written between two trims. The bound is enforced amortised — a trim runs on the
/// commit that crosses this, not on every commit — so the write path pays one extra DELETE per thousands
/// of rows instead of a scan per operation. The feed therefore sits at `CHANGE_FEED_RETAIN` plus at most
/// this many rows, which is the bound, not an exact length.
const CHANGE_FEED_TRIM_EVERY: u64 = 500;

/// `store_meta` key: the highest feed id truncation has removed. A reader whose cursor is at or below it
/// has missed changes — see [`super::read::changes_since`].
pub(super) const META_FEED_TRUNCATED_THROUGH: &str = "change_feed_truncated_through";

/// `store_meta` key: the highest feed id written **before** this store began stamping each change with
/// the window it belongs to (`AMB-D-582`). Those rows name no project and can no longer be attributed —
/// a deleted row has nothing left to ask — so a reader closed to one project whose cursor sits below
/// this has changes it will never be handed, and [`super::read::changes_since`] says so rather than
/// returning the empty page that reads as "nothing changed". Written once, by the migration step that
/// adds the column; a store born with it carries no row here, which reads as `0`.
pub(super) const META_FEED_WINDOWS_FROM: &str = "change_feed_windows_from";

/// What the `update_hook` collects between `BEGIN` and `commit`, shared with the callback SQLite owns.
/// `Arc<Mutex<_>>` because rusqlite requires the hook be `Send + 'static` — not because two threads
/// write it (the engine is one connection, and its writes are serialised by the transaction).
type ChangeBuffer = std::sync::Arc<std::sync::Mutex<Vec<RowChange>>>;

/// The store's truth source: the read-model connection, and the rows the transaction in flight has
/// touched — see [`super::write::WriteTx::commit`], which drains them into the feed.
pub struct StoreEngine {
    conn: Connection,
    changes: ChangeBuffer,
    /// Feed rows written since the last trim — the counter behind the amortised bound. In memory, so a
    /// fresh process starts at zero: at worst the first trim comes a little late, which costs a few
    /// hundred rows, and nothing has to be persisted on the write path to avoid it.
    rows_since_trim: std::sync::atomic::AtomicU64,
}

impl StoreEngine {
    /// Open (or create) a truth-source DB at `path`. The truth source is **plaintext** SQLite (on-device
    /// secrecy is delegated to OS full-disk encryption), so this opens plaintext unconditionally — there
    /// is no at-rest key. A legacy SQLCipher-encrypted file is refused by the store-open paths *before*
    /// this (`Store::open_at` errors out via `at_rest_status`); if one reaches here it surfaces as
    /// SQLCipher's "file is not a database" on the first schema access.
    pub fn open(path: &Path) -> Result<StoreEngine> {
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    /// Open an existing truth-source DB at `path` for **reading only — not one line of DDL**.
    /// [`open`](Self::open) reaches the connection through [`init`](Self::init), which issues the
    /// registry's `CREATE TABLE IF NOT EXISTS` and `PRAGMA journal_mode = WAL`. A *read* has no business
    /// doing any of that: [`crate::store::Store::open_read_at`] — the open behind every GUI read — holds
    /// **no exclusion at all** over other processes, so a DDL statement from here would rewrite the
    /// physical schema of a store another process may be writing. `PRAGMA query_only = ON` makes "writes
    /// nothing" an **invariant SQLite enforces**: any statement that would write returns
    /// `SQLITE_READONLY`. The connection is still opened read-write at the OS level, so a hot WAL left by
    /// a crashed writer still recovers normally. Never creates the file (callers check `exists()` first).
    pub fn open_read(path: &Path) -> Result<StoreEngine> {
        let conn = Connection::open(path)?;
        // `init`'s wait-don't-fail discipline: block for a contended lock rather than erroring.
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.execute_batch("PRAGMA query_only = ON;")?;
        // No feed collection: this connection cannot write, so it has nothing to report.
        Ok(StoreEngine { conn, changes: ChangeBuffer::default(), rows_since_trim: Default::default() })
    }

    /// Open an in-memory truth-source (tests / read-only probes).
    pub fn open_in_memory() -> Result<StoreEngine> {
        let conn = Connection::open_in_memory()?;
        Self::init(conn)
    }

    /// Open an in-memory engine with the registry's foreign keys **off**, for read-layer tests that seed
    /// a deliberately partial read model — a dependency edge to a task that was never inserted, an
    /// assignment to a value that has been deleted. A migrated store legitimately carries such orphans
    /// exactly what the read layer's cascade / reachability queries must tolerate and
    /// [`crate::validate::doctor`] must report), so these fixtures mirror it rather than seed a whole
    /// graph for every id they name. Every production write goes through [`super::WriteTx`] with
    /// enforcement on; this constructor is test-only.
    #[cfg(test)]
    pub(crate) fn open_in_memory_unchecked() -> Result<StoreEngine> {
        let e = Self::open_in_memory()?;
        e.conn.execute_batch("PRAGMA foreign_keys = OFF;")?;
        Ok(e)
    }

    fn init(mut conn: Connection) -> Result<StoreEngine> {
        // Wait — don't fail — when another local process (e.g. the GUI's watch/GC threads) holds the
        // write lock: block up to this long for the lock instead of erroring out with SQLITE_BUSY the
        // instant it is contended. Every batch of field writes is one transaction (all-or-nothing), and
        // this timeout keeps a contended write from failing needlessly. Set before any migration write.
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        // The journal mode first, because the DDL below is written into whichever one is live. Left
        // until after, a new store is built in the rollback journal — a file written, synced and
        // unlinked once per statement — where the WAL takes an append (`schema::JOURNAL_MODE_SQL`).
        let (journal_mode, ddl) = schema::genesis_sql();
        conn.execute_batch(journal_mode)?;
        // Is there anything here at all? An empty `sqlite_master` is a store being born, and the only
        // state in which the batch below writes rather than recognises. It decides how the batch is run
        // and nothing else — a store that answers "not empty" still goes through it, so an interrupted
        // genesis is still completed by the `IF NOT EXISTS` clauses, exactly as before.
        let being_born: i64 = conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get(0))?;
        // Create what is missing, and nothing else. `schema_sql` only ever issues
        // `CREATE … IF NOT EXISTS`, so this is genesis on a new file and a no-op on a store this
        // build already wrote. It does **not** evolve a store an older build left behind: bringing an
        // old store forward is the migration's job, and the migration is a numbered chain of steps
        // applied from the version the store carries — not a diff replayed on every open. The chain runs
        // on the engine this returns, so everything here necessarily runs *before* it: what this batch
        // names must already exist in the oldest store the chain still opens (see `schema::EXTRA_SQL`).
        //
        // On a store being born the whole batch is one transaction, so the sixty-odd objects it creates
        // cost one durable commit instead of one apiece. `Immediate` rather than the deferred default:
        // deferred starts read-only and asks to upgrade at the first write, and SQLite will not run the
        // busy handler on that upgrade — two processes reaching genesis together would meet `SQLITE_BUSY`
        // where today they queue. Taking the lock up front cannot cost anyone anything here, since the
        // store it locks is one nobody else has yet.
        //
        // On a store that already exists the batch stays exactly as it was, statement by statement in
        // autocommit. It writes nothing, so it takes no write lock — which is what keeps an open on the
        // ordinary path from contending with the GUI's.
        if being_born == 0 {
            let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            tx.execute_batch(&ddl)?;
            tx.commit()?;
        } else {
            conn.execute_batch(&ddl)?;
        }
        // Enforce the registry's `REFERENCES` for every write from here on.
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        // Keep the feed inside its bound even for a CLI-only store, where no process lives long enough
        // to trigger the amortised trim on the write path.
        trim_change_feed_on_open(&conn)?;
        let changes: ChangeBuffer = ChangeBuffer::default();
        install_change_hook(&conn, &changes)?;
        Ok(StoreEngine { conn, changes, rows_since_trim: Default::default() })
    }

    /// Borrow the read-model connection (for the read/query layer built on top of this engine).
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Trim the change feed back to [`CHANGE_FEED_RETAIN`] rows, but only once every
    /// [`CHANGE_FEED_TRIM_EVERY`] rows written, so no single commit carries the whole cost (`written` is
    /// what this commit just appended). Runs **inside the caller's transaction**, so the delete and the
    /// watermark that records it ([`META_FEED_TRUNCATED_THROUGH`]) land with the rows that displaced
    /// them, or not at all — a reader whose cursor is older than the watermark has lost changes it never
    /// saw, and an empty result would otherwise read as "nothing changed" and freeze a stale screen;
    /// recording how far the truncation reached is what lets [`super::read::changes_since`] say *"your
    /// cursor is gone, re-read the store"* instead.
    pub(super) fn trim_change_feed_if_due(
        &self,
        tx: &rusqlite::Transaction<'_>,
        written: u64,
    ) -> Result<()> {
        let due = self.rows_since_trim.fetch_add(written, std::sync::atomic::Ordering::Relaxed)
            + written
            >= CHANGE_FEED_TRIM_EVERY;
        if !due {
            return Ok(());
        }
        self.rows_since_trim.store(0, std::sync::atomic::Ordering::Relaxed);
        trim_change_feed(tx)
    }

    /// Forget whatever the hook has collected so far, and hand it back — how [`super::write::WriteTx`]
    /// starts a transaction with an empty ledger and ends it with exactly that transaction's rows.
    /// Anything a write outside a `WriteTx` leaves here is dropped at the next `BEGIN` rather than
    /// attributed to it: a change with no commit to ride into the feed with is not a change a reader may
    /// act on — the file-level signal covers those paths.
    pub(super) fn take_changes(&self) -> Vec<RowChange> {
        match self.changes.lock() {
            Ok(mut buf) => std::mem::take(&mut *buf),
            // A poisoned mutex means a hook callback panicked. Reporting no changes degrades the reader
            // to a gap (re-read everything), which is the safe direction; claiming stale rows is not.
            Err(poisoned) => std::mem::take(&mut *poisoned.into_inner()),
        }
    }

    /// Begin a transaction on the read-model connection so a batch of field writes commits all-or-
    /// nothing. The write method ([`set_field`](Self::set_field)) takes `&self` and runs on the same
    /// connection, so the returned guard scopes it: on `commit()` every write lands; on an early
    /// `?`/drop before commit, SQLite rolls the whole batch back — a failure partway (e.g. a contended
    /// `SQLITE_BUSY`) cannot leave a torn record with default-`''` timestamp columns that breaks every
    /// read. Opens **`BEGIN IMMEDIATE`**, taking the write lock up front: `BEGIN DEFERRED` — what
    /// rusqlite's `unchecked_transaction()` gives — starts read-only and only tries to upgrade at the
    /// first write; SQLite refuses to run the busy handler on that upgrade (it would deadlock two readers
    /// each waiting to become the writer) and returns `SQLITE_BUSY` *immediately*, so the connection's
    /// `busy_timeout` (5s, see [`Self::init`]) never applies. Every transaction here is a write batch,
    /// and several read-then-write (`add_task`'s `next_id` → INSERT, `task move`'s sibling
    /// `order_key` scan → UPDATE), so acquiring the lock at `BEGIN` is both correct — no lost update from
    /// a snapshot read — and the only way the wait-don't-fail discipline holds. Uses `new_unchecked` (not
    /// `Connection::transaction`) because the writes borrow `&self.conn` immutably alongside the guard,
    /// so the guard cannot take `&mut Connection`.
    pub fn transaction(&self) -> Result<rusqlite::Transaction<'_>> {
        Ok(rusqlite::Transaction::new_unchecked(&self.conn, rusqlite::TransactionBehavior::Immediate)?)
    }

    /// Write one field: UPSERT it straight into its read-model column. A single local store has no
    /// out-of-order writes, so the latest write to a field is simply the current value — no LWW check.
    /// Rejects a `(dataset, col)` the registry does not know (a coding defect at the mutation layer).
    ///
    /// A column the word index carries has its normalised copy rewritten in the same breath
    /// ([`super::search`]): this is the *only* path a record's text takes into the store, so an index
    /// kept here cannot fall behind by a write path that forgot it.
    pub fn set_field(&self, dataset: &str, row: i64, col: &str, val: Value) -> Result<()> {
        let ds = schema::dataset(dataset).ok_or_else(|| StoreEngineError::UnknownDataset(dataset.into()))?;
        if !ds.writable(col) {
            return Err(StoreEngineError::UnknownColumn { dataset: dataset.into(), col: col.into() });
        }
        // The dataset is a value here, so its columns cannot be named statically: `col` is a runtime name
        // the registry whitelist above vouches for. Its *value* still enters with it — see
        // `Insert::set_value`.
        Insert::into(ds.as_table())
            .set(ds.id_col(), row)
            .on_conflict_do_nothing(ds.id_col())
            .sql()
            .execute(&self.conn)?;
        Update::table(ds.as_table())
            .set_value(col, val.clone())
            .filter(Pred::eq(ds.id_col(), row))
            .sql()
            .execute(&self.conn)?;
        if search::indexes_field(dataset, col) {
            // Anything but text has no word in it, and the column is one the registry declares as text —
            // so a NULL is the absence of text, which the copy records by holding no row.
            let text = match &val {
                Value::Text(s) => s.as_str(),
                _ => "",
            };
            search::put_doc(&self.conn, dataset, row, col, text)?;
        }
        Ok(())
    }

    /// Whether this truth source already holds records — the "populated" signal the store's open path
    /// uses to tell an existing store (hydrate from the read-model) apart from a fresh/legacy one
    /// (genesis / one-time migration). It keys on `store_meta`: an initialised store has had its genesis
    /// scalars written ([`crate::Store::open_at`] writes `schema_version` there); a freshly-created
    /// engine file (schema only, no writes yet) has an empty `store_meta`.
    pub fn is_populated(&self) -> Result<bool> {
        Ok(meta_exists(&self.conn, super::META_SCHEMA_VERSION)?)
    }

    /// Whether this file carries **any** table at all — the "genesis" signal, one step below
    /// [`is_populated`](Self::is_populated). The two answer different questions and the write open needs
    /// this one: a store whose schema predates `store_meta` reports `is_populated() == false` while
    /// holding a user's tables and rows, and treating it as fresh would let `init` migrate it in place,
    /// which is exactly what the schema gate exists to prevent. "No tables" is the only state that is
    /// genuinely nothing to lose: a file that does not exist, or one `Connection::open` just created.
    /// Raw by necessity: the question is asked *of the file*, before any table of ours is known to be
    /// there, so it is asked through SQLite's own catalogue (`sqlite_master`) — which the registry does
    /// not declare and `col::` therefore cannot name.
    pub fn has_any_table(&self) -> Result<bool> {
        Ok(self
            .conn
            .query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table')", [], |r| {
                r.get::<_, i64>(0)
            })?
            != 0)
    }

    /// Does this store still carry the ULID key space of a pre-consolidation layout
    /// ([`schema::is_legacy_keyed`])? Such a store is a dead end for this build, so `open` refuses it by
    /// name rather than write a second key space into it.
    pub fn is_legacy_keyed(&self) -> Result<bool> {
        Ok(schema::is_legacy_keyed(&self.conn)?)
    }

    /// Whether this store holds **no record in any table** ([`schema::table_content_is_empty`]) — the
    /// engine twin of [`probe_is_empty`], for the read open, which already has the engine (opened by
    /// `open_read`, DDL-free) and must not spin up a second connection. It decides, on a legacy store the
    /// read path cannot itself reset, whether to defer to the writing open (empty → genesis) or refuse by
    /// name (non-empty).
    pub fn is_empty(&self) -> Result<bool> {
        Ok(schema::table_content_is_empty(&self.conn)?)
    }

    /// Upsert a store-level singleton scalar (`schema_version` / the format version) into the
    /// `store_meta` KV table. These have no per-record dataset, so they live in their own table. A `None`
    /// value clears the key (an unset scalar round-trips as absent).
    pub fn set_meta(&self, key: &str, value: Option<&str>) -> Result<()> {
        upsert_meta(&self.conn, key, value)?;
        Ok(())
    }

    /// Read a store-level singleton scalar from `store_meta` (`None` if the key was never set).
    pub fn get_meta(&self, key: &str) -> Result<Option<String>> {
        Ok(read_meta(&self.conn, key)?)
    }

    /// Read this store's `format_version`, treating a missing key as **v0** (the compat baseline: a
    /// store written before the guard existed carries no scalar). See [`crate::model::FORMAT_VERSION`].
    pub fn format_version(&self) -> Result<i64> {
        Ok(read_format_version(&self.conn)?)
    }

    /// Record this binary's [`crate::model::FORMAT_VERSION`] onto the store, with the app version that
    /// did it (what a later, older build names when it refuses the store it cannot open). Idempotent:
    /// only writes when the stored value differs. **This stamp is blind — it says nothing about whether
    /// the store was *carried* to that version — so only a caller that knows the answer may use it.**
    /// There is one: genesis ([`crate::store::Store::open_at`] creating a store), which is born at the
    /// latest shape with no step to run. An **existing** store's version is moved by the chain and
    /// nothing else ([`super::migrate::run`], stamping inside each step's transaction) — stamp it here
    /// and you claim a migration that never ran.
    pub fn stamp_format_version(&self) -> Result<()> {
        let stamped = read_format_stamp(&self.conn)?;
        if stamped.version != crate::model::FORMAT_VERSION {
            self.set_meta(super::META_FORMAT_VERSION, Some(&crate::model::FORMAT_VERSION.to_string()))?;
        }
        // The app version goes with it: the gate a *later, older* build hits names this when it refuses
        // the store. Backfilled when it is missing, so a store this build has opened can always
        // point its user at a version that opens it — even if the format version itself did not move.
        if stamped.version != crate::model::FORMAT_VERSION || stamped.set_by.is_none() {
            self.set_meta(super::META_FORMAT_VERSION_SET_BY, Some(crate::agent::VERSION))?;
        }
        Ok(())
    }

    /// Create or update a record as a batch of field writes (one UPSERT per field).
    pub fn put_record(&self, dataset: &str, id: i64, fields: &[(&str, Value)]) -> Result<()> {
        for (col, val) in fields {
            self.set_field(dataset, id, col, val.clone())?;
        }
        Ok(())
    }

    /// Physically delete one record row: `DELETE FROM <table> WHERE id = ?`. Whatever the registry's
    /// `ON DELETE` says happens to the rows that reference it — a comment, a dependency edge, a link is
    /// `RESTRICT`ed (`AMB-D-403`), so a child still there makes this fail rather than vanish outside any
    /// op, which is why a delete op deletes an entity subtree child-first itself; amenbo's own per-project
    /// settings go with the project instead (`CASCADE`). The polymorphic
    /// `attachment` carries no constraint at all; the caller sweeps it with
    /// [`delete_records_for_target`](Self::delete_records_for_target) before deleting the parent.
    pub fn delete_record(&self, dataset: &str, row: i64) -> Result<()> {
        let ds = schema::dataset(dataset)
            .ok_or_else(|| StoreEngineError::UnknownDataset(dataset.into()))?;
        Delete::from(ds.as_table()).filter(Pred::eq(ds.id_col(), row)).sql().execute(&self.conn)?;
        // The word index's copy of this record goes with it. Its rows name their record polymorphically,
        // so no `REFERENCES` can take them — this sweep is what stands in for the constraint, on the same
        // funnel every record delete passes through.
        if search::indexes_dataset(dataset) {
            search::drop_record(&self.conn, dataset, row)?;
        }
        Ok(())
    }

    /// Physically delete every `attachment` row whose polymorphic target is `(target_type, target_id)` —
    /// the sweep a delete op owes its entity, since no `REFERENCES` can branch on a sibling
    /// `target_type` column. Returns how many rows went. `attachment` is the only polymorphic child the
    /// store has, so it is named here rather than taken as a dataset: its two columns are then ordinary
    /// typed identifiers, and a registry rename lands on this sweep at compile time.
    pub fn delete_records_for_target(&self, target_type: &str, target_id: i64) -> Result<usize> {
        let att = col::attachment::ALL;
        let of_target =
            || Pred::eq(att.target_type, target_type).and(Pred::eq(att.target_id, target_id));
        // An attachment names itself in the word index, and this is the one delete that does not go
        // through `delete_record` — so the rows are read out first and their copies dropped by hand.
        // Without it the sweep would leave a word pointing at an attachment that no longer exists.
        let mut sel = Select::new();
        let id = sel.col(att.id);
        let mut sql = Sql::from(&sel, att.table);
        sql.push_where(Some(&of_target()));
        let doomed: Vec<i64> = self
            .conn
            .prepare(sql.text())?
            .query_map(rusqlite::params_from_iter(sql.params()), |r| id.get(r))?
            .collect::<rusqlite::Result<Vec<i64>>>()?;
        for row in doomed {
            search::drop_record(&self.conn, search::DATASET_ATTACHMENT, row)?;
        }
        Ok(Delete::from(att.table).filter(of_target()).sql().execute(&self.conn)?)
    }
}

/// True iff `path` is an existing **plaintext** SQLite file (header `SQLite format 3\0`). A fresh,
/// missing, or SQLCipher-encrypted file is `false` (an encrypted DB's first page is ciphertext, not
/// the plaintext header). The truth source is plaintext, so this is how `Store::open_at` detects a
/// legacy encrypted store to refuse and how `at_rest_status` reports the on-disk form.
pub(crate) fn is_plaintext_sqlite_file(path: &Path) -> bool {
    use std::io::Read;
    let Ok(mut f) = std::fs::File::open(path) else { return false };
    let mut hdr = [0u8; 16];
    f.read_exact(&mut hdr).map(|_| &hdr == b"SQLite format 3\0").unwrap_or(false)
}

/// The current at-rest form of the truth source at `db_path`: is it plaintext or a legacy
/// SQLCipher-encrypted file, and how big is it. A missing file reports `exists=false`. The truth source
/// is plaintext; this is what lets the store-open paths detect (and refuse) a not-yet-migrated legacy
/// encrypted store.
#[derive(Debug, Clone, Serialize)]
pub struct AtRestStatus {
    /// Whether `store.sqlite` exists at all (a fresh store that has never been opened has none).
    pub exists: bool,
    /// `true` when the on-disk file is plaintext (the current form); `false` when a legacy
    /// SQLCipher-encrypted file (or missing).
    pub plaintext: bool,
    /// The truth source's on-disk size in bytes (0 when missing).
    pub bytes: u64,
}

/// Inspect-only at-rest status of the truth source at `db_path`. Reads only the file header (via
/// [`is_plaintext_sqlite_file`]) and its length — no connection, no key, no side effects.
pub fn at_rest_status(db_path: &Path) -> AtRestStatus {
    let exists = db_path.exists();
    AtRestStatus {
        exists,
        plaintext: exists && is_plaintext_sqlite_file(db_path),
        bytes: std::fs::metadata(db_path).map(|m| m.len()).unwrap_or(0),
    }
}

/// Cut the change feed back to [`CHANGE_FEED_RETAIN`] rows and record how far the cut reached. Runs on
/// the caller's connection — inside a transaction from the write path, in its own from the open path —
/// so the delete and the watermark ([`META_FEED_TRUNCATED_THROUGH`]) are never separated: a reader whose
/// cursor is older than the watermark has lost changes it never saw, and an empty result would otherwise
/// read as "nothing changed" and freeze a stale screen; recording the cut is what lets
/// [`super::read::changes_since`] answer *"your cursor is gone, re-read the store"* instead.
fn trim_change_feed(conn: &Connection) -> Result<()> {
    // Everything at or below this id goes. The retention window is arithmetic, not a query: SQL is asked
    // only for the newest id, and an empty feed answers `NULL` — nothing to cut.
    let mut sel = Select::new();
    let newest = sel.expr::<Option<i64>>(format!("MAX({})", FEED.id.to_sql()));
    let sql = Sql::from(&sel, FEED.table);
    let newest: Option<i64> =
        conn.query_row(sql.text(), rusqlite::params_from_iter(sql.params()), |r| newest.get(r))?;
    let Some(cut) = newest.map(|n| n - CHANGE_FEED_RETAIN).filter(|c| *c > 0) else {
        return Ok(())
    };
    let removed = Delete::from(FEED.table)
        .filter(Pred::cmp(FEED.id, "<=", cut))
        .sql()
        .execute(conn)?;
    if removed == 0 {
        return Ok(());
    }
    upsert_meta(conn, META_FEED_TRUNCATED_THROUGH, Some(&cut.to_string()))?;
    Ok(())
}

/// Enforce the feed's bound on open, as well as amortised on the write path
/// ([`StoreEngine::trim_change_feed_if_due`]). The in-process counter alone would never fire for a
/// CLI-only user: every command is a fresh process that writes a handful of rows and exits, so the
/// counter never reaches its threshold and the feed would grow without limit. Checking here costs one
/// indexed `MAX(id)` per open (the feed's id is its primary key), and trims only when the feed has
/// actually outgrown its window.
fn trim_change_feed_on_open(conn: &Connection) -> Result<()> {
    // The span the feed currently holds — its two ends, taken in one pass; whether that outgrows the
    // window is arithmetic, and belongs here rather than in the statement.
    let mut sel = Select::new();
    let (newest, oldest) = (
        sel.expr::<Option<i64>>(format!("MAX({})", FEED.id.to_sql())),
        sel.expr::<Option<i64>>(format!("MIN({})", FEED.id.to_sql())),
    );
    let sql = Sql::from(&sel, FEED.table);
    let span: Option<i64> = conn.query_row(sql.text(), rusqlite::params_from_iter(sql.params()), |r| {
        Ok(newest.get(r)?.zip(oldest.get(r)?).map(|(hi, lo)| hi - lo))
    })?;
    if span.is_none_or(|s| s <= CHANGE_FEED_RETAIN) {
        return Ok(());
    }
    let tx = rusqlite::Transaction::new_unchecked(conn, rusqlite::TransactionBehavior::Immediate)?;
    trim_change_feed(&tx)?;
    tx.commit()?;
    Ok(())
}

/// Wire SQLite's `update_hook` to the change buffer: every row the connection touches is reported here,
/// and the record rows among them are what the feed carries. The hook, rather than an emit at each write
/// site, because only SQLite knows what a statement really touched: a row a constraint took (`plugin_config`
/// and `plugin_enable` still ride the project's `CASCADE`) is named by no ops code at all, and even where
/// the delete op does sweep its children itself, a write nobody remembered to instrument still reports. The callback
/// only records: it cannot write to the feed table from here — SQLite forbids using the connection inside
/// the hook — so the rows are collected now and written inside the transaction that produced them
/// ([`super::write::WriteTx::commit`]).
fn install_change_hook(conn: &Connection, changes: &ChangeBuffer) -> Result<()> {
    let sink = changes.clone();
    conn.update_hook(Some(move |action: rusqlite::hooks::Action, _db: &str, table: &str, row_id: i64| {
        // The whitelist: SQLite also reports `sqlite_sequence`, `store_meta` and `change_feed` itself.
        // None of them are rows a reader re-reads by id, and the feed table would otherwise feed on its
        // own writes.
        let Some(dataset) = schema::dataset_of_table(table) else { return };
        let op = match action {
            rusqlite::hooks::Action::SQLITE_INSERT => "insert",
            rusqlite::hooks::Action::SQLITE_UPDATE => "update",
            rusqlite::hooks::Action::SQLITE_DELETE => "delete",
            _ => return,
        };
        if let Ok(mut buf) = sink.lock() {
            buf.push(RowChange { dataset, row_id, op });
        }
    }))?;
    Ok(())
}

/// Read a store-level singleton scalar from the `store_meta` KV table on a borrowed connection
/// (used by the reverse projection [`super::hydrate`], which has only a `&Connection`). `None` when
/// the key was never written or the table predates this store (written by an older binary).
pub fn read_meta(conn: &Connection, key: &str) -> rusqlite::Result<Option<String>> {
    let mut sel = Select::new();
    let value = sel.col(META.value);
    let mut sql = Sql::from(&sel, META.table);
    sql.push_where(Some(&Pred::eq(META.key, key)));
    conn.query_row(sql.text(), rusqlite::params_from_iter(sql.params()), |r| value.get(r))
        .optional()
        .map(Option::flatten)
}

/// Read the store's monotonic `format_version` scalar off a borrowed connection, treating a **missing or
/// unparseable** key as **v0** (the compat baseline — a store written before the guard carries no scalar,
/// no flag day). See [`crate::model::FORMAT_VERSION`]. The reverse projection ([`super::hydrate`]) and
/// the forward-migration guard both read through here.
pub fn read_format_version(conn: &Connection) -> rusqlite::Result<i64> {
    Ok(read_meta(conn, super::META_FORMAT_VERSION)?
        .and_then(|v| v.trim().parse::<i64>().ok())
        .unwrap_or(0))
}

/// Read one scalar off a store file through a **bare connection** — no [`StoreEngine::init`], hence not
/// one line of DDL. Backs the two DDL-free probes below. `None` when the file is absent (genesis), cannot
/// be opened, or the query fails (no `store_meta` table, a legacy SQLCipher store, corruption). Callers
/// map that to the *permissive* answer: the probe exists to answer one question early, never to invent a
/// reason to refuse — whatever is actually wrong with the file surfaces from the real open that follows,
/// with its own error. Carries `init`'s wait-don't-fail `busy_timeout` so a store contended by the GUI
/// reads its real value instead of falling back to the permissive one on `SQLITE_BUSY`.
fn probe<T>(path: &Path, read: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> Option<T> {
    if !path.exists() {
        return None; // Don't let `Connection::open` create the file we came to inspect.
    }
    let conn = Connection::open(path).ok()?;
    conn.busy_timeout(std::time::Duration::from_secs(5)).ok()?;
    read(&conn).ok()
}

/// Read a store's `format_version` **without running a line of DDL**. The forward-migration gate
/// (`store::open::ensure_format_supported`) is only a gate if it runs *before* the schema is touched, and
/// [`StoreEngine::open`] applies the whole migration chain (`init`) on the way to handing back a
/// connection — so reading the version *through the engine* means a lagging binary has already
/// `ALTER TABLE`-d and `DROP`-ped its way through a newer store by the time the gate sees the version
/// (the `format_version` scalar survives, but the physical schema does not). Every open path therefore
/// probes here first and gates, then opens the engine. Unreadable (absent / not a database / no
/// `store_meta`) reads as **v0**, the compat baseline that no gate refuses — see `probe`.
pub fn probe_format_version(path: &Path) -> i64 {
    probe(path, read_format_version).unwrap_or(0)
}

/// A store's format version **and the app version that put it there** — what the too-new gate needs to
/// refuse by name. The named version comes from the store, not from the network: the app that stamped a
/// format version is, by definition, an app that can open a store at it. A store that predates the stamp
/// simply carries no name, which is what `set_by == None` says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatStamp {
    /// The store's monotonic format version (missing = v0, the compat baseline).
    pub version: i64,
    /// The app version (semver, e.g. `0.2.0`) that stamped that format version, when the store records
    /// one. A store written before this was stamped carries `None`.
    pub set_by: Option<String>,
}

/// Read a store's [`FormatStamp`] off a borrowed connection.
pub fn read_format_stamp(conn: &Connection) -> rusqlite::Result<FormatStamp> {
    Ok(FormatStamp {
        version: read_format_version(conn)?,
        set_by: read_meta(conn, super::META_FORMAT_VERSION_SET_BY)?.filter(|v| !v.trim().is_empty()),
    })
}

/// [`probe_format_version`]'s twin, carrying the app version too — the DDL-free read the open-time gate
/// takes before it decides whether this build may touch the store at all.
pub fn probe_format_stamp(path: &Path) -> FormatStamp {
    probe(path, read_format_stamp).unwrap_or(FormatStamp { version: 0, set_by: None })
}

/// Whether the file at `path` can be read as a SQLite database at all, **without running a line of DDL**.
/// `true` for any readable database (even an empty or foreign one); `false` for a file that is not one —
/// a truncated write, a name collision. The migration is the caller that needs this:
/// [`probe_format_version`] reads an unreadable file as v0, the permissive baseline, so such a file walks
/// into the plan as "a v0 store to migrate" and only blows up later, inside the backup, as SQLite's `file
/// is not a database` — which names no file at all; here the plan can refuse before it writes anything,
/// and say which path it choked on. Absent reads as `true`: a store that is not there is not a broken one
/// (genesis), and the open path that follows decides. Raw by necessity: what is being asked is whether
/// SQLite can read the file at all, and the table it asks through — `sqlite_master` — is SQLite's own
/// bookkeeping, which the registry does not declare and `col::` therefore cannot name.
pub fn probe_is_database(path: &Path) -> bool {
    if !path.exists() {
        return true;
    }
    probe(path, |conn| conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0)))
        .is_some()
}

/// Whether a store file already holds records, **without running a line of DDL** — the probe twin of
/// [`StoreEngine::is_populated`] (same `store_meta` anchor), for [`crate::store::Store::init`]'s
/// "already initialized?" check, which runs *before* the version gate has had its say and so must not
/// open the engine. Unreadable reads as **not populated**: the real open that follows decides
/// (genesis, or a bilingual error for a legacy encrypted store).
pub fn probe_is_populated(path: &Path) -> bool {
    probe(path, |conn| meta_exists(conn, super::META_SCHEMA_VERSION)).unwrap_or(false)
}

/// Whether a store still carries the **ULID `TEXT`** key space of a pre-consolidation store, **without
/// running a line of DDL** — the probe twin of [`StoreEngine::is_legacy_keyed`] ([`schema::is_legacy_keyed`],
/// same `sqlite_master`/`pragma_table_info` catalogue read). The open-time key-space gate
/// (`store::open::ensure_integer_keyed`) takes this *before* it opens the engine, for the same reason the
/// version gate reads through [`probe_format_version`]: [`StoreEngine::open`] applies the registry DDL in
/// [`StoreEngine::init`] on the way to a connection, and that DDL — `CREATE UNIQUE INDEX … ON project(slug)`
/// among it — errors on a store whose `project` table predates the column, so the store would blow up on a
/// raw SQLite error before the gate that means to refuse it by name ever ran. Unreadable (absent / not a
/// database / a legacy encrypted store) reads as **not legacy**, the permissive answer: the real open that
/// follows surfaces whatever is actually wrong, with its own error.
pub fn probe_is_legacy_keyed(path: &Path) -> bool {
    probe(path, schema::is_legacy_keyed).unwrap_or(false)
}

/// Whether a store holds **no record in any table**, **without running a line of DDL** — the probe twin
/// of [`StoreEngine::is_empty`] ([`schema::table_content_is_empty`]). The writing open's key-space gate
/// (`store::open::reconcile_legacy_key_space`) takes this to decide whether a pre-consolidation store may
/// be cleared to genesis (nothing to lose) rather than refused by name; it must not open the
/// engine to ask, for the same reason [`probe_is_legacy_keyed`] must not — [`StoreEngine::open`]'s DDL
/// crashes on the very schema this gate exists to refuse. The polarity is deliberately strict: clearing
/// **destroys**, so an unreadable file, a legacy encrypted store, or any table that still holds a row all
/// read as **not empty** (`false`), and the store is refused rather than cleared.
pub fn probe_is_empty(path: &Path) -> bool {
    probe(path, schema::table_content_is_empty).unwrap_or(false)
}

/// The live (non-archived) projects of the single store, **without running a line of DDL** — the probe
/// twin of a `project_list` for callers that must name the store's projects *before* deciding to open it.
/// The CLI's pointer guard (`no_pointer`) is the one such caller: it offers `bind --project <id>`
/// candidates from a bare directory, and opening the engine merely to enumerate them would
/// forward-migrate a store the user has not asked to touch. Unreadable (absent / legacy / pre-fold
/// schema) reads as **no candidates**: the guard just omits the list. A row that exists is live, so there
/// is no liveness predicate — and there must not be one: this probe runs no DDL, so it reads the store's
/// *current* physical schema, and naming a column a migrated store no longer has would turn every project
/// into "no candidates". Raw by necessity, and the one place a row is still taken apart **by position**:
/// the registry describes the schema this binary migrates *to*, and that is exactly what a probe must not
/// assume of a store it has not opened. The list is two columns wide and written right here, next to the
/// two reads — there is no distant projection to fall out of step with.
pub fn probe_live_projects(path: &Path) -> Vec<(String, String)> {
    probe(path, |conn| {
        let mut stmt =
            conn.prepare("SELECT id, name FROM project WHERE archived = 0 ORDER BY order_key")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
    })
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> Value {
        Value::Text(s.into())
    }

    /// `open_read` writes nothing — SQLite enforces it (`query_only = ON`), so the "read-only open" is an
    /// invariant, not a comment: a DDL statement bounces off the connection.
    #[test]
    fn open_read_cannot_write() {
        let dir = amenbo_scratch::scratch("open-read");
        let path = dir.join("store.sqlite");
        StoreEngine::open(&path).unwrap(); // create the schema through the write-path open.

        let e = StoreEngine::open_read(&path).unwrap();
        assert!(e.conn().execute_batch("ALTER TABLE task ADD COLUMN sneaky TEXT").is_err());
        assert!(e.conn().execute_batch("DROP TABLE change_feed").is_err());
        assert!(e.conn().execute("DELETE FROM store_meta", []).is_err());
        // …while reads work through the same connection.
        assert_eq!(
            e.conn().query_row("SELECT COUNT(*) FROM task", [], |r| r.get::<_, i64>(0)).unwrap(),
            0
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The bare legacy-key probe reads the catalogue without opening the engine, so it survives a
    /// pre-consolidation `project` that predates the `slug` column — the store whose registry DDL
    /// (`CREATE UNIQUE INDEX … ON project(slug)`) [`StoreEngine::open`] would crash on. A store this build
    /// wrote is integer-keyed and reads as `false`; a missing file reads as `false` (genesis).
    #[test]
    fn probe_is_legacy_keyed_reads_a_slugless_ulid_store_without_ddl() {
        let dir = amenbo_scratch::scratch("probe-legacy");
        let legacy = dir.join("legacy.sqlite");
        {
            let conn = Connection::open(&legacy).unwrap();
            conn.execute_batch(
                "CREATE TABLE project (id TEXT PRIMARY KEY NOT NULL, name TEXT NOT NULL DEFAULT '');
                 CREATE TABLE task (id TEXT PRIMARY KEY NOT NULL, title TEXT NOT NULL DEFAULT '');",
            )
            .unwrap();
        }
        assert!(probe_is_legacy_keyed(&legacy), "a slug-less ULID project is legacy — and no DDL ran to learn it");

        let fresh = dir.join("fresh.sqlite");
        StoreEngine::open(&fresh).unwrap();
        assert!(!probe_is_legacy_keyed(&fresh), "a store this build wrote is integer-keyed");
        assert!(!probe_is_legacy_keyed(&dir.join("absent.sqlite")), "a missing store is genesis, not legacy");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The bare emptiness probe: a store with no row anywhere reads as empty; one row
    /// makes it non-empty; `store_meta` scalars do not count (they are stamps, not records); and an
    /// absent file reads as **not empty** — the strict polarity, since a caller uses it to decide
    /// whether to clear a store, and clearing what it cannot read must never happen.
    #[test]
    fn probe_is_empty_is_strict() {
        let dir = amenbo_scratch::scratch("probe-empty");

        let empty = dir.join("empty.sqlite");
        {
            let conn = Connection::open(&empty).unwrap();
            // Record tables with no rows, plus a `store_meta` stamp that must be exempt.
            conn.execute_batch(
                "CREATE TABLE task (id TEXT PRIMARY KEY NOT NULL, title TEXT NOT NULL DEFAULT '');
                 CREATE TABLE story (id TEXT PRIMARY KEY NOT NULL, body TEXT NOT NULL DEFAULT '');
                 CREATE TABLE store_meta (key TEXT PRIMARY KEY NOT NULL, value TEXT);
                 INSERT INTO store_meta (key, value) VALUES ('format_version', '1');",
            )
            .unwrap();
        }
        assert!(probe_is_empty(&empty), "no record row anywhere — the store_meta stamp does not count");

        let with_row = dir.join("with-row.sqlite");
        {
            let conn = Connection::open(&with_row).unwrap();
            conn.execute_batch(
                "CREATE TABLE task (id TEXT PRIMARY KEY NOT NULL, title TEXT NOT NULL DEFAULT '');
                 INSERT INTO task (id, title) VALUES ('01ABC', 't');",
            )
            .unwrap();
        }
        assert!(!probe_is_empty(&with_row), "a single record row makes it non-empty");

        assert!(!probe_is_empty(&dir.join("absent.sqlite")), "an unreadable/absent file reads as not empty");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Guard the SQLCipher build: the workspace links `rusqlite` with `bundled-sqlcipher-vendored-openssl`,
    /// so `PRAGMA cipher_version` must return a non-empty version row (plain bundled SQLite returns *no
    /// row*). No production path keys a store; SQLCipher stays linked only so the `at_rest_sqlcipher`
    /// test can *fabricate* a legacy encrypted store (via `sqlcipher_export`) and prove the store-open
    /// paths refuse it. This fails loud if a dependency edit drops SQLCipher back to stock SQLite (which
    /// would silently break that legacy-refuse coverage).
    #[test]
    fn sqlcipher_is_linked() {
        let e = StoreEngine::open_in_memory().unwrap();
        let version: Option<String> = e
            .conn()
            .query_row("PRAGMA cipher_version", [], |r| r.get(0))
            .optional()
            .unwrap();
        assert!(
            version.as_deref().is_some_and(|v| !v.is_empty()),
            "expected SQLCipher (PRAGMA cipher_version non-empty); got {version:?} = stock SQLite, the legacy-encrypted-store refuse test can no longer fabricate its input"
        );
    }

    fn title_of(e: &StoreEngine, id: &str) -> String {
        e.conn()
            .query_row("SELECT title FROM task WHERE id = ?1", [id], |r| r.get(0))
            .unwrap()
    }

    /// A store that never had the `format_version` scalar written reads back as **v0** — the compat
    /// baseline that lets the guard-introducing binary open any existing store.
    #[test]
    fn format_version_missing_reads_as_v0() {
        let e = StoreEngine::open_in_memory().unwrap();
        assert_eq!(e.format_version().unwrap(), 0, "missing reads as v0 (the compat baseline)");
        // Garbage (non-integer) is also treated as v0 rather than panicking.
        e.set_meta(super::super::META_FORMAT_VERSION, Some("not-a-number")).unwrap();
        assert_eq!(e.format_version().unwrap(), 0, "a non-integer value is treated as v0 too");
    }

    /// Stamping records this binary's `FORMAT_VERSION`, and is idempotent — a second stamp on an
    /// already-current store writes nothing new (same value round-trips).
    #[test]
    fn stamp_format_version_records_and_is_idempotent() {
        let e = StoreEngine::open_in_memory().unwrap();
        e.stamp_format_version().unwrap();
        assert_eq!(e.format_version().unwrap(), crate::model::FORMAT_VERSION);
        // Idempotent: re-stamping keeps the same recorded value.
        e.stamp_format_version().unwrap();
        assert_eq!(e.format_version().unwrap(), crate::model::FORMAT_VERSION);
    }

    /// Hammering one field keeps the read-model at the latest write: each write UPSERTs its column in
    /// place, so the projection is always the most recent value with no history left behind.
    #[test]
    fn repeated_writes_project_last() {
        let e = StoreEngine::open_in_memory().unwrap();
        e.put_record("task", 1, &[("title", text("v0"))]).unwrap();
        for i in 1..200 {
            e.set_field("task", 1, "title", text(&format!("v{i}"))).unwrap();
        }
        assert_eq!(title_of(&e, "1"), "v199", "projection follows the newest write");
        // The task table holds exactly one row for the field (no append-only log accumulates).
        let rows: i64 =
            e.conn().query_row("SELECT count(*) FROM task WHERE id=1", [], |r| r.get(0)).unwrap();
        assert_eq!(rows, 1, "one read-model row, not a growing write log");
    }


    /// A batch of field writes that fails partway must leave **no** torn record. Without the
    /// transaction, the per-field UPSERT loop auto-commits each write, so an error mid-loop (a contended
    /// `SQLITE_BUSY`, a disk error) would leave a half-written row whose unwritten
    /// `created_at`/`updated_at` sit at their schema default (`TEXT NOT NULL DEFAULT ''` → `''`), and
    /// `hydrate::parse_ts("")` would then break every read store-wide.
    /// [`transaction`](StoreEngine::transaction) scopes the batch so a failure rolls the whole thing back.
    #[test]
    fn transaction_rolls_back_partial_field_writes() {
        let e = StoreEngine::open_in_memory().unwrap();

        // Within one transaction, write a comment's leading fields, then hit an error (an unknown column —
        // the same `UnknownColumn` a write propagates) *before* the created_at/updated_at writes. The whole
        // batch must roll back.
        let torn: Result<()> = (|| {
            let tx = e.transaction()?;
            e.set_field("task_comment", 1, "task_id", Value::Integer(1))?;
            e.set_field("task_comment", 1, "text", text("本文"))?;
            e.set_field("task_comment", 1, "does_not_exist", Value::Null)?; // fails here
            e.set_field("task_comment", 1, "created_at", text("2026-07-04T00:00:00Z"))?;
            tx.commit()?;
            Ok(())
        })();
        assert!(torn.is_err(), "the batch fails on the unknown column");

        let n: i64 =
            e.conn().query_row("SELECT count(*) FROM task_comment WHERE id=1", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0, "a failed flush leaves no partial row (no empty-timestamp torn record)");
    }

    /// The connection is opened with a non-zero `busy_timeout`, so a write contended by another local
    /// process (the GUI's watch/GC threads) waits for the lock instead of failing `SQLITE_BUSY` the
    /// instant it is held.
    #[test]
    fn busy_timeout_is_set() {
        let e = StoreEngine::open_in_memory().unwrap();
        let ms: i64 = e.conn().query_row("PRAGMA busy_timeout", [], |r| r.get(0)).unwrap();
        assert!(ms >= 5000, "busy_timeout is set to at least 5s (got {ms}ms)");
    }

    /// [`transaction`](StoreEngine::transaction) must open `BEGIN IMMEDIATE`, i.e. hold the write lock
    /// from the first statement. Structural proof, no timing: SQLite reports the connection's
    /// transaction state, and `Write` is reachable at `BEGIN` only under IMMEDIATE/EXCLUSIVE — a DEFERRED
    /// transaction sits at `None` until its first statement runs.
    #[test]
    fn transaction_takes_the_write_lock_at_begin() {
        let e = StoreEngine::open_in_memory().unwrap();
        let tx = e.transaction().unwrap();
        let state = e.conn().transaction_state(None::<&str>).unwrap();
        assert_eq!(
            state,
            rusqlite::TransactionState::Write,
            "the write lock is held at BEGIN (IMMEDIATE), not deferred to the first write"
        );
        drop(tx);
    }

    /// The whole point of `BEGIN IMMEDIATE` — a contended write **waits out** `busy_timeout` instead of
    /// erroring instantly. Under `BEGIN DEFERRED` the second writer would sail past `BEGIN`, read
    /// happily, and then take an immediate `SQLITE_BUSY` on the read→write upgrade, where SQLite
    /// deliberately refuses to run the busy handler (it would deadlock two would-be writers). Two
    /// connections to one file are the in-process stand-in for the CLI + GUI pair.
    #[test]
    fn a_contended_transaction_waits_out_busy_timeout() {
        let dir = amenbo_scratch::scratch("tx-immediate");
        let path = dir.join("store.sqlite");

        // Open both before either holds the lock: opening runs migrations, which would themselves wait.
        let writer = StoreEngine::open(&path).unwrap();
        let contender = StoreEngine::open(&path).unwrap();
        // Shorten the contender's 5s wait so the test costs a fraction of a second, not five.
        contender.conn().busy_timeout(std::time::Duration::from_millis(200)).unwrap();

        let held = writer.transaction().unwrap();
        let started = std::time::Instant::now();
        let blocked = contender.transaction();
        let waited = started.elapsed();
        drop(held);

        assert!(blocked.is_err(), "the write lock is held, so the contender cannot begin");
        assert!(
            waited >= std::time::Duration::from_millis(100),
            "the contender waited on busy_timeout rather than failing instantly (waited {waited:?})"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

}
