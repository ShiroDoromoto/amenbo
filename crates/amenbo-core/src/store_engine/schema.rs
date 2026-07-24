//! SQLite truth-source schema for the engine.
//!
//! The read model is plain indexed SQLite: one table per record type, mirroring the production
//! [`crate::model`] shapes. Each field write UPSERTs its column directly into the read-model table —
//! the store is one local database, so SQLite's write serialisation is the total order and the
//! last write to a field simply wins.
//!
//! The dataset registry below is the single source of truth: the CREATE TABLE DDL and the
//! per-column write whitelist are both derived from it, so they cannot drift. A record may be
//! created field-by-field, so every `NOT NULL` column carries a `DEFAULT` — an `INSERT` of the `id`
//! alone succeeds before the rest of the fields land. `id` is the row key and is the only column not
//! writable through a field write.
//!
//! A column declares what it *is*, not merely how SQLite stores it: its type carries a `CHECK` that
//! says which values the column admits, and a `REFERENCES` that says which rows a reference may name.
//! Every record table's `id` is an `INTEGER PRIMARY KEY` (SQLite's rowid alias — the B-tree key
//! itself, not a TEXT index with a rowid indirection), and every reference column (`fk!`/`fk_opt!`)
//! is `BIGINT … REFERENCES`. The conversational number *is* the key — a task's id is the number it is
//! called by, a decision's id is what renders as `D-<id>` — so there is no separate `number` column,
//! and the same key crosses the boundary (an `i64` in Rust, a `number` in TS), so nothing renders a
//! key as text on the way through. The constructors are `col!` (plain text/integer), `ts!`/`ts_opt!`
//! (RFC3339Z instant), `date_opt!` (`%Y-%m-%d` day), `enum_col!`/`enum_opt!` (closed value set),
//! `bool_col!`, `hash_opt!` (blake3 hex) and `fk!`/`fk_opt!` (foreign key) — declared below in this
//! module.
//!
//! The **spelling** of an integer type says what the column is, not how SQLite stores it: `INTEGER` and
//! `BIGINT` carry the same affinity and the same 64-bit storage, so to the database they are one type.
//! Every key and count here is an `i64` in Rust and every flag a `bool`, so the declaration says so —
//! `BIGINT` for a key or a size, `BOOLEAN` for a truth value. The one exception is the primary key
//! itself, which must be spelled exactly `INTEGER PRIMARY KEY` to *be* the rowid alias.
//!
//! Two rules keep the constraints compatible with the seams around them:
//!
//! 1. **A `CHECK` must admit its column's own `DEFAULT`.** The `''` a required text column defaults to
//!    is the *not-yet-written* sentinel that lets a migration step's `ALTER TABLE ADD COLUMN`
//!    ([`super::migrate`]) add a `NOT NULL` column to an existing store, and that lets a record be
//!    created field-by-field
//!    (`INSERT INTO <table>(id)` then one `UPDATE` per field). A `CHECK` that rejected `''` would
//!    reject both. So `''` is admitted, and every *other* wrong value is rejected.
//! 2. **A `REFERENCES` is `DEFERRABLE INITIALLY DEFERRED`.** Same reason from the other side: mid-way
//!    through a field-by-field create the reference columns still hold their default — `0` for the
//!    INTEGER `fk!` (rowid aliases start at 1, so `0` is a value no real row holds — the direct analog
//!    of the `''` text sentinel) — and a record referencing its own table (`decision_edge`, decision →
//!    decision) may name a row a bulk projection has yet to write. Deferring the check
//!    to `COMMIT` makes intra-transaction order irrelevant while still refusing to *commit* a dangling
//!    reference — so every write of a reference must sit in a transaction (it does: one logical
//!    operation = one transaction).
//!
//! Deletion is physical (`DELETE`), not a `deleted_at` tombstone, so every reference has to say what
//! happens to it when its parent goes. The rule is that **the schema never silently deletes a
//! first-class entity**: deleting an entity's subtree is something a delete op does explicitly, in code
//! a reviewer can read, not something a `CASCADE` does behind it. So the action follows the *kind* of
//! relation, and each `fk!`/`fk_opt!` names its own:
//!
//! | kind | example | `ON DELETE` |
//! |---|---|---|
//! | required entity reference | `decision.project_id`, `dimension.project_id` | `RESTRICT` |
//! | entity reference the delete op cascades itself | `task.project_id` | `RESTRICT` (+ the op deletes the tasks first) |
//! | optional entity reference (keep the child, drop the reference) | none in the registry today | `SET NULL` |
//! | link / junction row | `decision_edge`, `decision_task_link`, `task_dependency`, `task_dimension_value` | `CASCADE` |
//! | dependent content | `task_comment.task_id`, `dimension_value.dimension_id` | `CASCADE` |
//!
//! `ON UPDATE CASCADE` is uniform and harmless (an `INTEGER` key does not change in place).
//! `attachment.target_id` is polymorphic — no `REFERENCES` can branch on a sibling `target_type`
//! column — so no constraint holds it and the delete ops sweep it by hand.
//!
//! Enforcement is per-connection (`PRAGMA foreign_keys = ON`, set by `super::engine::init`) and
//! per-table: `CREATE TABLE IF NOT EXISTS` leaves an existing table exactly as it was, so an existing
//! table keeps whatever constraints it was created with. New stores get them from the first row.

/// One column of a read-model table: its name and SQLite type/constraints declaration.
pub struct Column {
    pub name: &'static str,
    /// Type + constraints, e.g. `"TEXT NOT NULL DEFAULT ''"`. Concatenated after the name.
    pub decl: &'static str,
}

/// A record type the engine reads and writes: its stable dataset key, its read-model table, and its
/// columns. These are the tables `export`/`archive` carry, as opposed to the device-local plain tables.
pub struct Dataset {
    /// The dataset's stable key (`task`, `decision`, …) — the name a reader and `export` speak, never the
    /// physical table name.
    pub name: &'static str,
    /// Read-model table name.
    pub table: &'static str,
    /// Type-specific columns (the universal [`AUDIT`] columns are appended to every table and are
    /// not repeated here). All are writable through the engine's field-write path; `id` never is.
    pub columns: &'static [Column],
}

impl Dataset {
    /// Every column written through the log: the type-specific columns plus the universal audit
    /// columns. Excludes `id` (the row key — never written as a field) and
    /// the implicit PRIMARY KEY. This is the registry's truth for the writable-column whitelist, for what
    /// [`crate::export`] dumps, and for the shape a snapshot is verified against ([`crate::archive`]).
    pub fn all_columns(&self) -> impl Iterator<Item = &'static Column> {
        self.columns.iter().chain(AUDIT)
    }

    /// Is `col` a column this dataset writes through the log? Covers the type-specific columns and
    /// the universal audit columns; `id` is never writable.
    pub fn writable(&self, col: &str) -> bool {
        self.all_columns().any(|c| c.name == col)
    }

    /// This dataset's table and its row key, as the identifiers a statement names them by — the same
    /// [`Table`](super::sql::Table) and [`Col`](super::sql::Col) [`mod@col`] hands every other caller,
    /// for the one caller that cannot use it: the dataset-generic write path
    /// ([`super::StoreEngine::set_field`] and the deletes beside it) reaches a table through this
    /// registry entry, not by naming it in source, so its table and columns are not statically known,
    /// while the one column it does need is the same `INTEGER PRIMARY KEY` on every table.
    pub fn as_table(&self) -> super::sql::Table {
        super::sql::Table::new(self.table, self.table)
    }

    /// This dataset's row key — see [`as_table`](Self::as_table).
    pub fn id_col(&self) -> super::sql::Col<super::sql::Int> {
        super::sql::Col::new(self.table, self.table, "id")
    }
}

/// A table that is not a record: the store's scalars, the change feed, the folder bindings and the
/// device-local task sets. It has no `id` surrogate, no audit columns and no field-write path — the
/// engine's field-write path never touches it — so it is declared apart from [`DATASETS`], but declared the
/// same way: one line per column, emitting both the DDL and the reader's typed identifier.
pub struct PlainTable {
    /// Table name.
    pub name: &'static str,
    /// The columns, in the order the DDL writes them.
    pub columns: &'static [Column],
    /// A table-level constraint written after the columns (a composite `PRIMARY KEY`), if it has one.
    pub constraint: Option<&'static str>,
}

const REQ: &str = "TEXT NOT NULL DEFAULT ''"; // required text
const OPT: &str = "TEXT"; // nullable text
const INT_OPT: &str = "BIGINT"; // nullable 64-bit integer (`attachment.size_bytes`)
/// A polymorphic reference (`attachment.target_id`): a key like `fk!`'s, minus
/// the `REFERENCES` — SQLite cannot branch a constraint on a sibling `target_type` column, so the ops
/// sweep these rows by hand. An integer because every key is one; a TEXT affinity here would
/// coerce the boundary's `i64` back into a decimal string on the way in.
const KEY_REF: &str = "BIGINT NOT NULL DEFAULT 0";
/// Fractional index: ordered by string comparison, never parsed as a number.
const ORDER_KEY: &str = "TEXT NOT NULL DEFAULT ''";
const ORDER_KEY_OPT: &str = "TEXT"; // an unplaced (inbox) task carries no order key
/// A project's human-readable identifier. Unique across the store, but the uniqueness is a
/// `CREATE UNIQUE INDEX` in `EXTRA_SQL`, not a `UNIQUE` in this decl: SQLite refuses
/// `ALTER TABLE … ADD COLUMN … UNIQUE`, and a migration step adds a column to an existing store that
/// way, so a decl an old store cannot take is a decl the fresh schema must not have either
/// (the two paths have to land on the same table). Nullable, so a row mid-create holds no value that
/// could collide with another's.
const SLUG: &str = "TEXT";

// The `GLOB` patterns the timestamp/date `CHECK`s match are spelled out inside `ts!`/`date_opt!`
// below rather than named here: `concat!` (which assembles each column's `decl`) takes string
// literals, not `const` references. An instant is the fixed-width `Timestamp::to_rfc3339_z` form
// (`YYYY-MM-DDThh:mm:ssZ`) — fixed width, so lexical order *is* chronological order and a text range
// scan is a time range scan; a date is the `%Y-%m-%d` day `NaiveDate::to_string` writes.

/// Plain text/integer column: `title: col(REQ)`.
macro_rules! col {
    ($name:ident, $decl:ident) => {
        Column { name: stringify!($name), decl: $decl }
    };
}

/// Required instant. Admits `''`, the not-yet-written sentinel (see the module rules).
macro_rules! ts {
    ($name:ident) => {
        Column {
            name: stringify!($name),
            decl: concat!(
                "TEXT NOT NULL DEFAULT '' CHECK(", stringify!($name), " = '' OR ", stringify!($name), " GLOB '",
                "[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z", "')"
            ),
        }
    };
}

/// Nullable instant (`completed_at`, `decided_at`). `NULL` satisfies a `CHECK` without a null guard,
/// so unset needs no clause of its own.
macro_rules! ts_opt {
    ($name:ident) => {
        Column {
            name: stringify!($name),
            decl: concat!(
                "TEXT CHECK(", stringify!($name), " GLOB '",
                "[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z", "')"
            ),
        }
    };
}

/// Nullable calendar day — a *day*, never an instant (`due_on` is not `completed_at`).
macro_rules! date_opt {
    ($name:ident) => {
        Column {
            name: stringify!($name),
            decl: concat!(
                "TEXT CHECK(", stringify!($name), " GLOB '",
                "[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]", "')"
            ),
        }
    };
}

/// Required closed value set, as the model's snake_case wire strings. Admits `''` (see the rules).
macro_rules! enum_col {
    ($name:ident, $($v:literal),+ $(,)?) => {
        Column {
            name: stringify!($name),
            decl: concat!("TEXT NOT NULL DEFAULT '' CHECK(", stringify!($name), " IN (''", $(", '", $v, "'",)+ "))"),
        }
    };
}

/// Nullable closed value set (an unset priority, an unset actor facet).
macro_rules! enum_opt {
    ($name:ident, $first:literal $(, $rest:literal)* $(,)?) => {
        Column {
            name: stringify!($name),
            decl: concat!(
                "TEXT CHECK(", stringify!($name), " IN ('", $first, "'", $(", '", $rest, "'",)* "))"
            ),
        }
    };
}

/// Truth value stored as `0`/`1` — not "any integer".
macro_rules! bool_col {
    ($name:ident) => {
        Column {
            name: stringify!($name),
            decl: concat!("BOOLEAN NOT NULL DEFAULT 0 CHECK(", stringify!($name), " IN (0, 1))"),
        }
    };
}

/// Nullable content address: blake3 as 64 lower-case hex digits, matching the blob's file name.
macro_rules! hash_opt {
    ($name:ident) => {
        Column {
            name: stringify!($name),
            decl: concat!(
                "TEXT CHECK(length(", stringify!($name), ") = 64 AND NOT ", stringify!($name), " GLOB '*[^0-9a-f]*')"
            ),
        }
    };
}

/// Required reference to `<parent>(id)` (an `i64` key). Deferred, so a field-by-field create may pass
/// through the `0` default (the not-yet-written sentinel — no real row holds `0`, ids start at 1) and a
/// bulk projection need not order parents before children — but the transaction cannot commit while the
/// reference dangles. `$on_delete` is what happens to this row when its parent is deleted — see the
/// module's delete policy above.
macro_rules! fk {
    ($name:ident, $parent:literal, $on_delete:literal) => {
        Column {
            name: stringify!($name),
            decl: concat!(
                "BIGINT NOT NULL DEFAULT 0 REFERENCES ", $parent, "(id) ",
                "ON DELETE ", $on_delete, " ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED"
            ),
        }
    };
}

/// Nullable reference to `<parent>(id)` (an integer key) — an unplaced task's project, a decision
/// that supersedes none. NULL is the unset value, so it needs no sentinel default.
macro_rules! fk_opt {
    ($name:ident, $parent:literal, $on_delete:literal) => {
        Column {
            name: stringify!($name),
            decl: concat!(
                "BIGINT REFERENCES ", $parent, "(id) ",
                "ON DELETE ", $on_delete, " ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED"
            ),
        }
    };
}

/// The actor facet: the whole subject signal a record carries.
macro_rules! actor_kind {
    ($name:ident) => {
        enum_opt!($name, "human", "ai")
    };
}

/// One registry line → its [`Column`] (the DDL half of what a line declares).
macro_rules! column {
    ($name:ident : col($decl:ident))                     => { col!($name, $decl) };
    ($name:ident : ts)                                   => { ts!($name) };
    ($name:ident : ts_opt)                               => { ts_opt!($name) };
    ($name:ident : date_opt)                             => { date_opt!($name) };
    ($name:ident : bool_col)                             => { bool_col!($name) };
    ($name:ident : hash_opt)                             => { hash_opt!($name) };
    ($name:ident : actor_kind)                           => { actor_kind!($name) };
    ($name:ident : enum_col($($v:literal),+ $(,)?))      => { enum_col!($name, $($v),+) };
    ($name:ident : enum_opt($($v:literal),+ $(,)?))      => { enum_opt!($name, $($v),+) };
    ($name:ident : fk($parent:literal, $od:literal))     => { fk!($name, $parent, $od) };
    ($name:ident : fk_opt($parent:literal, $od:literal)) => { fk_opt!($name, $parent, $od) };
}

/// The same registry line → the **type** of the column it declares (the other half). What SQLite
/// stores, not what the column means: a day, an instant, an enum and a hash are all
/// [`Text`](crate::store_engine::sql::Text) — the shape that tells them apart is the `CHECK` in the
/// line above. The nullability half comes from the same line: a reader that maps a row into a struct
/// has to pick `String` or `Option<String>`, and the registry is the one that knows which — the
/// `*_opt` kinds and the nullable text decls (`OPT`, `SLUG`, `ORDER_KEY_OPT`, `INT_OPT`) admit `NULL`,
/// and everything else is declared `NOT NULL`. Getting it wrong would be an `InvalidColumnType` on the
/// first row that happens to hold no value; here it does not compile
/// ([`Read`](crate::store_engine::sql::Read)).
macro_rules! column_type {
    ($name:ident : col(REQ))            => { $crate::store_engine::sql::Col<$crate::store_engine::sql::Text> };
    ($name:ident : col(OPT))            => { $crate::store_engine::sql::Col<$crate::store_engine::sql::Text, $crate::store_engine::sql::Nullable> };
    ($name:ident : col(SLUG))           => { $crate::store_engine::sql::Col<$crate::store_engine::sql::Text, $crate::store_engine::sql::Nullable> };
    ($name:ident : col(ORDER_KEY))      => { $crate::store_engine::sql::Col<$crate::store_engine::sql::Text> };
    ($name:ident : col(ORDER_KEY_OPT))  => { $crate::store_engine::sql::Col<$crate::store_engine::sql::Text, $crate::store_engine::sql::Nullable> };
    ($name:ident : col(INT_OPT))        => { $crate::store_engine::sql::Col<$crate::store_engine::sql::Int, $crate::store_engine::sql::Nullable> };
    ($name:ident : col(KEY_REF))        => { $crate::store_engine::sql::Col<$crate::store_engine::sql::Int> };
    ($name:ident : ts)                  => { $crate::store_engine::sql::Col<$crate::store_engine::sql::Text> };
    ($name:ident : ts_opt)              => { $crate::store_engine::sql::Col<$crate::store_engine::sql::Text, $crate::store_engine::sql::Nullable> };
    ($name:ident : date_opt)            => { $crate::store_engine::sql::Col<$crate::store_engine::sql::Text, $crate::store_engine::sql::Nullable> };
    ($name:ident : hash_opt)            => { $crate::store_engine::sql::Col<$crate::store_engine::sql::Text, $crate::store_engine::sql::Nullable> };
    ($name:ident : actor_kind)          => { $crate::store_engine::sql::Col<$crate::store_engine::sql::Text, $crate::store_engine::sql::Nullable> };
    ($name:ident : enum_col($($v:literal),+ $(,)?)) => { $crate::store_engine::sql::Col<$crate::store_engine::sql::Text> };
    ($name:ident : enum_opt($($v:literal),+ $(,)?)) => { $crate::store_engine::sql::Col<$crate::store_engine::sql::Text, $crate::store_engine::sql::Nullable> };
    ($name:ident : bool_col)            => { $crate::store_engine::sql::Col<$crate::store_engine::sql::Bool> };
    ($name:ident : fk($parent:literal, $od:literal))     => { $crate::store_engine::sql::Col<$crate::store_engine::sql::Int> };
    ($name:ident : fk_opt($parent:literal, $od:literal)) => { $crate::store_engine::sql::Col<$crate::store_engine::sql::Int, $crate::store_engine::sql::Nullable> };
}

/// The registry itself: each line declares one column **once**, and both things a column is are
/// generated from it — the DDL ([`DATASETS`], whence `CREATE TABLE` and the write whitelist) and the
/// typed identifier readers name it by ([`mod@col`]). They cannot drift, because there is no second place
/// to say it: renaming a column here is a compile error at every reader that still spells the old name.
/// A line is `<column>: <kind>`; the kinds are the constructors above. Every table also gets `id` and
/// the universal [`AUDIT`] columns without declaring them.
macro_rules! datasets {
    ($(#[$meta:meta])* $($dataset:ident => $table:ident { $($cname:ident : $ckind:tt $(($($cargs:tt)*))?),+ $(,)? })+) => {
        $(#[$meta])*
        pub const DATASETS: &[Dataset] = &[
            $(Dataset {
                name: stringify!($dataset),
                table: stringify!($table),
                columns: &[$(column!($cname : $ckind $(($($cargs)*))?)),+],
            }),+
        ];

        /// The registry's columns as **typed identifiers**, one module per table, generated from the
        /// same lines the DDL comes from — so a `Col` a reader can name is a column the store has.
        /// Reached through [`mod@col`], which holds these beside the plain tables' columns.
        #[doc(hidden)]
        pub mod record_cols {
            $(
                #[doc = concat!("The columns of `", stringify!($table), "`.")]
                pub mod $table {
                    /// This table's columns, each carrying its type and the qualifier they were asked
                    /// for. `Copy`, so naming one twice in a query costs nothing.
                    #[derive(Debug, Clone, Copy)]
                    pub struct Cols {
                        /// The table these columns are of, under the qualifier they were asked for —
                        /// what a `FROM` / `JOIN` names it by, so the alias is stated once.
                        pub table: $crate::store_engine::sql::Table,
                        /// The row key: an `INTEGER PRIMARY KEY` — the conversational number.
                        pub id: $crate::store_engine::sql::Col<$crate::store_engine::sql::Int>,
                        $(pub $cname: column_type!($cname : $ckind $(($($cargs)*))?),)+
                        /// When the row was created (universal audit column).
                        pub created_at: $crate::store_engine::sql::Col<$crate::store_engine::sql::Text>,
                        /// When the row was last written (universal audit column).
                        pub updated_at: $crate::store_engine::sql::Col<$crate::store_engine::sql::Text>,
                    }

                    /// This table's columns, qualified by `q` — the table's own name, or the alias the
                    /// query gave it (`of("t")` → `t.status`), along with the table itself under that
                    /// same alias.
                    pub const fn of(q: &'static str) -> Cols {
                        Cols {
                            table: $crate::store_engine::sql::Table::new(stringify!($table), q),
                            id: $crate::store_engine::sql::Col::new(stringify!($table), q, "id"),
                            $($cname: $crate::store_engine::sql::Col::new(stringify!($table), q, stringify!($cname)),)+
                            created_at: $crate::store_engine::sql::Col::new(stringify!($table), q, "created_at"),
                            updated_at: $crate::store_engine::sql::Col::new(stringify!($table), q, "updated_at"),
                        }
                    }

                    /// The columns spelled with the table's own name — for a query that does not alias it.
                    pub const ALL: Cols = of(stringify!($table));
                }
            )+
        }
    };
}

/// A **plain table**'s column declaration → the DDL it emits. The type word is the column's own
/// (`INTEGER` and `BIGINT` are both integers to Rust, but not to the store's key space); anything the
/// line adds in parentheses (a `PRIMARY KEY`, an `AUTOINCREMENT`) is written verbatim after it —
/// `NOT NULL` is not one of those things, because the type word says it: a plain column is `NOT NULL`
/// unless its kind ends in `_opt`, and the DDL is emitted to match, since the same word decides what
/// the column reads and writes as (`plain_type!`). Spelled in the constraint literal instead, the two
/// could disagree, and a registry that says a column's nullability twice can say it two ways.
macro_rules! plain_decl {
    (text)                => { "TEXT NOT NULL" };
    (text, $c:literal)    => { concat!("TEXT ", $c, " NOT NULL") };
    (text_opt)            => { "TEXT" };
    (integer)             => { "INTEGER NOT NULL" };
    (integer, $c:literal) => { concat!("INTEGER ", $c, " NOT NULL") };
    (bigint)              => { "BIGINT NOT NULL" };
    (bigint, $c:literal)  => { concat!("BIGINT ", $c, " NOT NULL") };
}

/// The same line → the **type** of the column it declares, the way [`column_type!`] does for the
/// registry: what SQLite stores, whether it admits `NULL` — and nothing about what the column means.
/// The kind word alone decides both halves, which is what keeps the DDL and the type from drifting
/// (see `plain_decl!`). A nullable kind the store has no column of yet (`integer_opt`) has no arm:
/// declaring one is a compile error until the arm is written, not a column silently typed `NOT NULL`.
macro_rules! plain_type {
    (text)     => { $crate::store_engine::sql::Col<$crate::store_engine::sql::Text> };
    (text_opt) => { $crate::store_engine::sql::Col<$crate::store_engine::sql::Text, $crate::store_engine::sql::Nullable> };
    (integer) => { $crate::store_engine::sql::Col<$crate::store_engine::sql::Int> };
    (bigint)  => { $crate::store_engine::sql::Col<$crate::store_engine::sql::Int> };
}

/// The plain tables, declared the way the registry declares a record's: one line per column, from which
/// both the DDL ([`PLAIN_TABLES`]) and the typed identifier readers name it by ([`mod@col`]) are
/// generated — so these tables cannot drift from their readers either. A line is
/// `<column>: <type>("<constraints>")`, the constraints written verbatim after the type word;
/// a table-level constraint follows the braces as `=> "<sql>"`. Nothing is added implicitly —
/// that is what makes a table *plain*: it has no `id` surrogate and no audit columns, because it is not
/// a record (see each table's own note below).
macro_rules! plain_tables {
    ($($(#[$meta:meta])* $table:ident { $($cname:ident : $ckind:ident $(($cc:literal))?),+ $(,)? } $(=> $tc:literal)?)+) => {
        /// The tables that are **not** records: they carry no `id` surrogate, no audit columns and no
        /// field-write path, so the engine's field-write machinery never touches them and they are not
        /// [`DATASETS`] entries. Their DDL is generated from the declarations below, as the registry's is.
        pub const PLAIN_TABLES: &[PlainTable] = &[
            $(PlainTable {
                name: stringify!($table),
                columns: &[$(Column { name: stringify!($cname), decl: plain_decl!($ckind $(, $cc)?) }),+],
                constraint: { #[allow(unused_mut, unused_assignments)] let mut c = None; $(c = Some($tc);)? c },
            }),+
        ];

        /// The plain tables' columns as typed identifiers. Reached through [`mod@col`].
        #[doc(hidden)]
        pub mod plain_cols {
            $(
                $(#[$meta])*
                pub mod $table {
                    /// This table's columns, each carrying its type and the qualifier they were asked for.
                    #[derive(Debug, Clone, Copy)]
                    pub struct Cols {
                        /// The table these columns are of, under the qualifier they were asked for —
                        /// what a `FROM` / `JOIN` names it by, so the alias is stated once.
                        pub table: $crate::store_engine::sql::Table,
                        $(pub $cname: plain_type!($ckind),)+
                    }

                    /// This table's columns, qualified by `q` — the table's own name, or the alias the
                    /// query gave it — along with the table itself under that same alias.
                    pub const fn of(q: &'static str) -> Cols {
                        Cols {
                            table: $crate::store_engine::sql::Table::new(stringify!($table), q),
                            $($cname: $crate::store_engine::sql::Col::new(stringify!($table), q, stringify!($cname)),)+
                        }
                    }

                    /// The columns spelled with the table's own name — for a query that does not alias it.
                    pub const ALL: Cols = of(stringify!($table));
                }
            )+
        }
    };
}

/// Every table's columns as typed identifiers, in **one** namespace: the record tables (from
/// [`DATASETS`]) and the plain ones (from [`PLAIN_TABLES`]). A reader names a column the same way on
/// either side — which side a table is on is the engine's business, not the reader's. A query names
/// its tables by alias, so the columns are handed out qualified: `of("t")` gives back `task`'s columns
/// spelled `t.title`, `t.status`, …, while each module's `ALL` const spells them with the table's own
/// name.
pub mod col {
    pub use super::plain_cols::*;
    pub use super::record_cols::*;
}

/// Timestamp/audit columns shared by every record (stored as RFC3339 TEXT). There is no `deleted_at`:
/// a delete is a physical `DELETE`, so a row that exists is a row that is live, and no read carries a
/// liveness predicate.
const AUDIT: &[Column] = &[ts!(created_at), ts!(updated_at)];

datasets! {
    /// The record types and their read-model columns (faithful subsets of [`crate::model`]).
    /// **Superset invariant:** this registry is the upper bound of every read-model table — a live
    /// table holds a *subset* of the columns declared here, never more. Declaring a column here is what a
    /// **new** store is born with; what restores the equality on a store an older build left behind is a
    /// numbered step in the version chain ([`super::migrate`]) that adds it — nothing runs on open. So
    /// adding a column is two things, and a column added here alone is one an existing store never grows
    /// (it fails at the first read with `no such column`).
    ///
    /// Two corollaries the registry alone cannot enforce, kept honest by tests
    /// instead: (a) the projection ([`super::record`]) must *write* every column declared here (a
    /// column that exists but is never populated is a silent gap); (b) round-trip tests must build
    /// their oracle independently of the code under test and seed non-empty values, or an
    /// empty-vs-empty compare hides a dropped column.

    project => project {
        name: col(REQ),
        notes: col(REQ),
        color: col(OPT),
        default_view: enum_col("list", "board", "calendar", "timeline"),
        archived: bool_col,
        order_key: col(ORDER_KEY),
        slug: col(SLUG),
    }

    task => task {
        title: col(REQ),
        notes: col(REQ),
        subtype: enum_col("default", "milestone"),
        // There is no separate "done" column: completion is derived from `status == 'done'`.
        completed_at: ts_opt,
        // Two of these are terminals — `done` (carried out) and `rejected` (decided against). Widening a
        // closed set is not a column addition, so the step that carries it rewrites the `CHECK` a store
        // already has rather than adding anything (`super::migrate`, v9).
        status: enum_col("todo", "in_progress", "done", "blocked", "rejected"),
        // When `status` last changed — stamped on a status transition only (`ops::task::set_status`), so
        // it answers "when did the current status begin" where `updated_at` (moved by any write) cannot.
        // Nullable: a task from a store that predates the column was never stamped (`AMB-D-366`).
        status_changed_at: ts_opt,
        created_by_kind: actor_kind,
        assignee_kind: actor_kind,
        start_on: date_opt,
        due_on: date_opt,
        priority: enum_opt("high", "medium", "low"),
        // A task has exactly one home, so its project and order key live on the task itself rather
        // than on a 1:1 satellite row. Both are nullable — an unplaced (inbox) task carries NULL,
        // including `order_key`.
        project_id: fk_opt("project", "RESTRICT"),
        order_key: col(ORDER_KEY_OPT),
    }

    decision => decision {
        project_id: fk("project", "RESTRICT"),
        title: col(REQ),
        body: col(REQ),
        // `superseded` is not a state — it is the far end of a `supersedes` edge. Currency is derived
        // (`current` = no live `supersedes` edge names this decision), never stored.
        status: enum_col("proposed", "accepted", "rejected"),
        // When `status` last changed — stamped on a status transition only (`ops::decision`), the mirror of
        // `task.status_changed_at`. `decided_at` cannot stand in for it: a reopen clears that one, and a
        // reopened decision is exactly what the comparison has to date (`AMB-D-373`). Nullable in the decl
        // because `ALTER TABLE ADD COLUMN` starts every existing row at NULL; the step that adds it
        // backfills them all in the same transaction, so no live row is left holding NULL.
        status_changed_at: ts_opt,
        decided_at: ts_opt,
        decided_by: col(OPT),
    }
    // Decision→decision edges: the closed, typed set of relations between decisions, one row per edge —
    // the shape `task_dependency` and `decision_task_link` already have, so a decision may point at many
    // others. The edge always runs new → old (`decision_id` authored it, `target_decision_id` is the
    // older one it names); the reverse view (who superseded me) is derived by looking the target up, and
    // the target's own row is never rewritten. `kind` is the behaviour that distinguishes them — how the
    // target is to be read from now on: a `supersedes` target is historicised (stop reading it), an
    // `amends` target stays current (read both together), a `builds_on` target stays current and is read
    // *first* (this decision stands on it, so overturning it puts this one up for review).
    // `decision_edge_pair` (below) keeps a pair from carrying two kinds at once — `supersedes`/`amends`
    // contradict, and both imply `builds_on`, so there is never a second edge to draw.
    decision_edge => decision_edge {
        decision_id: fk("decision", "CASCADE"),
        target_decision_id: fk("decision", "CASCADE"),
        kind: enum_col("supersedes", "amends", "builds_on"),
        // When the edge came to carry the `kind` it carries now — the third of the premise-change intent
        // columns (`AMB-D-372`), beside `task_dependency.established_at` and `decision_task_link.linked_at`.
        // It is what dates a supersession: the target's own row is never rewritten, so nothing on the
        // premise side records *when* it stopped being current, and its `status_changed_at` does not move
        // (being superseded is an edge, not a status). Re-stamped when a pair's kind is rewritten in place,
        // because a `builds_on` promoted to `supersedes` began superseding at the promotion, not at the
        // original insert. Nullable for the reason the other two are: `ALTER TABLE ADD COLUMN` starts every
        // existing row at NULL, and the step that adds it seeds them in the same transaction.
        drawn_at: ts_opt,
    }

    decision_task_link => decision_task_link {
        decision_id: fk("decision", "CASCADE"),
        task_id: fk("task", "CASCADE"),
        // When the link was drawn — the intent column the premise-change judgement dates a link by
        // (`AMB-D-372`). `created_at` is a record column and stays out of that judgement even though this
        // row is append-only: the threat is an out-of-band batch or restore rewriting record columns, which
        // "the app has no UPDATE path" does not defend against. Fixed at insert (`ops::decision::link`) and
        // never rewritten. Nullable for the reason `decision.status_changed_at` is: `ALTER TABLE ADD COLUMN`
        // starts every existing row at NULL, and the step that adds it backfills them in the same
        // transaction, so no live row is left holding one.
        linked_at: ts_opt,
    }

    // Permanent task comments. A comment is always task-scoped and always carries a body.
    task_comment => task_comment {
        task_id: fk("task", "CASCADE"),
        author_kind: actor_kind,
        text: col(REQ),
        // When the body was rewritten in place, if it ever was — the "edited" mark a reader needs to
        // see that the line no longer says what it said. `updated_at` cannot answer this: an instant is
        // second-resolution, so an edit within the same second leaves it equal to `created_at`.
        // NULL = never edited.
        edited_at: ts_opt,
    }

    // Permanent comments on a decision record. Mirrors `task_comment`, but kept a **separate** table
    // rather than a polymorphic shared one so each row holds a real FK (`decision_id → decision.id`).
    decision_comment => decision_comment {
        decision_id: fk("decision", "CASCADE"),
        author_kind: actor_kind,
        text: col(REQ),
        // The edit stamp, for the same reason as `task_comment`.
        edited_at: ts_opt,
    }

    dependency => task_dependency {
        task_id: fk("task", "CASCADE"),
        blocked_by_id: fk("task", "CASCADE"),
        created_by_kind: actor_kind,
        // When the edge was established — the twin of `decision_task_link.linked_at`, and for the same
        // reason (`AMB-D-372`): the premise-change judgement dates an edge by this column, never by
        // `created_at`. Fixed at insert (`ops::dependency::add`), nullable only so the migration that adds
        // it can backfill.
        established_at: ts_opt,
    }

    // A git commit SHA a task carries (1 task : many commits). amenbo stores the SHA as an opaque
    // string and never reads git — the anchor from history back to a task. `sha` is the full-length
    // lower-case hex the ops layer normalises to and admits at the door (40 = SHA-1, 64 = SHA-256);
    // short forms, refs and revisions are refused before they can land.
    task_commit => task_commit {
        task_id: fk("task", "CASCADE"),
        sha: col(REQ),
        created_by_kind: actor_kind,
    }

    // The unified dimension model: three datasets that put every classification axis on one mechanism.
    // Every axis is a plain user-editable one — there are no built-in fixed axes (no `kind`), no locked
    // values, no stable keys (no `builtin_key`). There are no tags either (multi-select, unordered): a
    // dimension is single-select, so it is not a tag, and a free-form topic name is found through the read
    // layer's `text:` substring match instead. `ordered` says whether the axis's values have an order;
    // `role` is what nominates one axis as the project's time axis.
    dimension => dimension {
        project_id: fk("project", "RESTRICT"),
        name: col(REQ),
        notes: col(REQ),
        // `single` is the only value the model admits; the column stays so reviving multi-select is
        // just adding an enum branch — here and in `model::DimensionCardinality`.
        cardinality: enum_col("single"),
        ordered: bool_col,
        role: enum_col("none", "time_axis"),
        order_key: col(ORDER_KEY),
    }

    dimension_value => dimension_value {
        dimension_id: fk("dimension", "CASCADE"),
        name: col(REQ),
        order_key: col(ORDER_KEY),
        // The period of a `role: time_axis` value (a day, like `task.start_on`). Every value carries
        // the columns; only a time_axis axis gives them meaning (model.rs).
        start_on: date_opt,
        end_on: date_opt,
    }

    task_dimension_value => task_dimension_value {
        task_id: fk("task", "CASCADE"),
        // Denormalised so the (task,dimension) single-select constraint and axis filters query
        // the row directly without joining through `dimension_value` (model.rs).
        dimension_id: fk("dimension", "CASCADE"),
        value_id: fk("dimension_value", "CASCADE"),
    }

    // Two-mode attachment (`blob` ingest / `url` link). The target is polymorphic
    // (`target_type` / `target_id`). Blob bytes never land here — the truth source carries only the
    // metadata; the content-addressed bytes live out-of-band.
    attachment => attachment {
        target_type: enum_col("task", "decision", "task_comment", "decision_comment"),
        // Polymorphic — no `REFERENCES` can branch on a sibling `target_type` column.
        target_id: col(KEY_REF),
        kind: enum_col("blob", "url"),
        // Blob-mode metadata (null in url mode): content-address + original filename + mime + size.
        blob_hash: hash_opt,
        filename: col(OPT),
        mime: col(OPT),
        size_bytes: col(INT_OPT),
        // Url-mode external link (null in blob mode).
        url: col(OPT),
        created_by_kind: actor_kind,
        order_key: col(ORDER_KEY),
    }

    // Per-project override of a plugin's **text (non-secret)** config value (`AMB-D-356` / `AMB-D-350`).
    // The machine default is a `config.json` field (`config.plugin_config`); this row overrides it for one
    // project — the same two-tier shape `hook_consent` (config) / `hook_optout` (store) already use, with
    // one deliberate difference: this **is** a record table, carried by `export`/`backup`, not a
    // device-local plain table — text config belongs in the ordinary tiers, backup included, whereas
    // `hook_optout` is an export-excluded veto. A `secret` field never reaches here — it lives in the
    // user-area secret file (`crate::plugin_secret`), off the store entirely. `plugin` is the plugin's
    // manifest name (a string; plugins live on disk, not in the store, so there is no FK for it) and
    // `field_key` the config field's key (spelled out because `key` is a SQLite keyword). The
    // `(project_id, plugin, field_key)` triple is unique (`plugin_config_triple` below). CASCADE: the
    // override is *about* the project, so deleting the project retires it.
    plugin_config => plugin_config {
        project_id: fk("project", "CASCADE"),
        plugin: col(REQ),
        field_key: col(REQ),
        value: col(REQ),
    }

    // The projects a `scope: project` plugin is **enabled in** (`AMB-D-379`). A set, not a two-answer
    // override: the author declares which single switch their plugin has, so a project-scoped one has no
    // machine tier under it to inherit or veto — a row means "on here" and no row means off. (That is
    // `hook_optout`'s shape after all; the `enabled` column the two-tier version carried is gone with the
    // tier it existed for.) A record table, so it is carried by `export`/`backup`: a restore that dropped
    // it would silently switch a project's plugins off.
    //
    // It carries the gate, never the **consent** — that stays machine-local and is the resolver's second
    // condition (`crate::plugin_trust::effective_enabled_in`), so a row restored onto a device that never
    // consented opens nothing. `plugin` is the manifest name (plugins live on disk, not in the store, so
    // there is no FK for it); the `(project_id, plugin)` pair is unique (`plugin_enable_pair` below).
    // CASCADE: the row is *about* the project.
    plugin_enable => plugin_enable {
        project_id: fk("project", "CASCADE"),
        plugin: col(REQ),
    }
}

/// The dataset a **record table** belongs to, or `None` when the table is not one of them — the
/// whitelist the change feed's `update_hook` filters on: SQLite reports every row it touches, which
/// includes `sqlite_sequence`, `store_meta` and the feed table itself. None of those are records a
/// reader can re-read by id, and the feed table would feed on its own writes. Only the registry's
/// tables pass.
pub fn dataset_of_table(table: &str) -> Option<&'static str> {
    DATASETS.iter().find(|d| d.table == table).map(|d| d.name)
}

/// Look up a dataset by its stable key.
pub fn dataset(name: &str) -> Option<&'static Dataset> {
    DATASETS.iter().find(|d| d.name == name)
}

plain_tables! {
    /// Store-level singleton scalars that have no per-record dataset, kept as a plain key/value table:
    /// `schema_version` and the format version. These are the store's versioning metadata, not record
    /// fields, so they live here rather than as read-model rows. `value` is nullable so an unset scalar
    /// round-trips.
    store_meta {
        key: text("PRIMARY KEY"),
        value: text_opt,
    }

    /// The change feed: one row per record row a committed transaction touched. It carries the
    /// *instruction* — which dataset, which id, which kind of change — and nothing else: no column
    /// names, no old value, no new value, so a reader that learns a task changed re-reads it from the
    /// truth source and notes bodies stay out of the feed. It records which rows moved, for a machine —
    /// the GUI reads it with a cursor to invalidate exactly the queries that went stale, instead of
    /// re-fetching everything — and it is written by `WriteTx::commit` **inside the operation's own
    /// transaction**, so a committed change always has its feed row (a reader that missed one would
    /// leave a screen wrong, which an after-the-fact append cannot rule out). `id` is the reader's
    /// cursor: monotonic, gap-free enough to say "everything after N" (AUTOINCREMENT, so a truncation
    /// can never hand a later row an id a reader has already passed).
    change_feed {
        id: integer("PRIMARY KEY AUTOINCREMENT"),
        dataset: text,
        row_id: bigint,
        op: text,
    }

    /// The **plugin observation outbox**: one row per semantic lifecycle event a committed operation
    /// fired (`task.created`, `task.status_changed`, `comment.added`, …). It is a *sibling* of
    /// `change_feed`, not a layer on it (`AMB-D-367`): the feed carries DB-row *instructions* for the
    /// GUI's cache and cannot say which of the six an `update` split into, nor who drove it — the outbox
    /// carries the **semantic event** for a plugin instead, with the two things the feed structurally
    /// lacks (`actor`, and the `new_state` an `update` disambiguates). The ops write point *composes* the
    /// event from what it alone knows (the operation kind, the actor, the new state) and `WriteTx` just
    /// appends the row it is handed — the store interprets none of these strings. Written **inside the
    /// operation's own transaction** (`WriteTx::emit_event`), so a committed change always
    /// has its event row; unlike the feed, it is *not* drained from `update_hook`, so the two write paths
    /// stay separate. `id` is the reader's cursor — monotonic and gap-free (AUTOINCREMENT), the same
    /// "everything after N" contract the feed offers, but each consumer (the dispatcher) keeps its **own**
    /// cursor. Retention is a separate policy from the feed's window-trim: an event must survive until it
    /// has been fanned out onto the queue of every plugin that observes it (`plugin_queue` below,
    /// `AMB-D-399`), so nothing here is trimmed on the "consumed or not" basis the feed uses.
    plugin_outbox {
        id: integer("PRIMARY KEY AUTOINCREMENT"),
        event: text,
        record_id: bigint,
        actor: text,
        at: text,
        new_state: text_opt,
    }

    /// One plugin's **work queue**: the events fanned out to it and not yet run (`AMB-D-399`). Delivery is
    /// two-layered, and this is the second layer — where the outbox is *what happened*, a queue is *what is
    /// still to do*, per plugin. The fan-out reads the outbox once, copies each event onto the queue of
    /// every plugin subscribed to it, and deletes the outbox rows it copied, all in one transaction: the
    /// outbox is then reclaimed independently of how fast any plugin runs, so one stalled plugin backs up
    /// only its own queue.
    ///
    /// The columns are the outbox's wire fields (opaque here too — the store classifies nothing) plus the
    /// two the split needs: `plugin` says whose queue the row is on, and `face` records the face the fan-out
    /// resolved the subscription on (`AMB-D-383`), so the runner can rebuild that plugin's invocation for
    /// this row whichever face gets to it. `id` is the queue's own order — a plugin's rows are run oldest
    /// first, and a row is deleted once it has been run. Being a per-row table rather than a cursor is what
    /// leaves room to record *this one failed*, which a position number has nowhere to say.
    plugin_queue {
        id: integer("PRIMARY KEY AUTOINCREMENT"),
        plugin: text,
        face: text,
        event: text,
        record_id: bigint,
        actor: text,
        at: text,
        new_state: text_opt,
    }

    /// Who is **running** a plugin's queue right now — at most one row per plugin, and the whole of the
    /// "one runner per plugin" rule (`AMB-D-399`). A drive that has just fanned out claims the row before
    /// it starts a runner: the row is there, so nobody starts a second one, and a runner leaves only by
    /// deleting it — in the same transaction that found its queue empty. Both sides pass through one
    /// transaction, so the order is always one or the other: a fan-out that lands first leaves the row
    /// standing and the running runner picks its event up; a runner that leaves first frees the row and
    /// the fan-out starts a new one.
    ///
    /// `expires_at` is what keeps the rule from becoming a deadlock. A runner killed with the machine, or
    /// with the process it rode in on, never deletes its row; without a horizon that plugin would be
    /// "already running" forever and never run again. A runner pushes its horizon out while it works, and
    /// a row past it is void — whoever finds it takes the queue over. `owner` names the runner the row is
    /// for, so the predecessor of such a takeover deletes nothing on its way out: the row it left is no
    /// longer the row that is there.
    plugin_runner {
        plugin: text("PRIMARY KEY"),
        owner: text,
        expires_at: text,
    }

    /// The device-local **folder bindings** — the project's main dir, one row per project
    /// ([`crate::binding::Registry`] resolves by it). The key is the project's `INTEGER` id, as a
    /// *natural* key: the record shape would force a surrogate `id`. No `REFERENCES project(id)`: a
    /// folder pointer outlives the project it names, which is what makes the stale-binding warning and
    /// pointer recovery possible.
    binding_path {
        project_id: integer("PRIMARY KEY"),
        dir: text,
    }

    /// The other half of the bindings: every dir pointing at a project — a set of `(project, dir)`
    /// pairs, the pair itself being the key.
    binding_project_dir {
        project_id: bigint,
        dir: text,
    } => "PRIMARY KEY (project_id, dir)"

    /// The device-local **read receipts** — a task's last-seen instant. Device-local, export-excluded
    /// overview state, so a plain `task_id → last_seen` table is the faithful shape and the row is
    /// UPSERTed directly; `task_id` is the task's `INTEGER` key, the same identifier a record carries.
    /// The mailbox's single last-seen instant is a scalar and lives in `store_meta` (keyed
    /// `read_receipt.mailbox_last_seen`), not a row.
    read_receipt {
        task_id: integer("PRIMARY KEY"),
        last_seen: text,
    }

    /// The device-local **inbox archive** — the set of inbox items this device has dismissed (a display
    /// filter, not a record table). This is the inbox's only *persistent* state; the mailbox itself is
    /// a computed view over tasks.
    inbox_archive {
        task_id: integer("PRIMARY KEY"),
    }

    /// The device-local **mailbox notified set** — the inbox items this device has already raised an OS
    /// notification for. Device-local: the same item may notify once on each separate install. It
    /// generalises the mailbox's old in-memory "seen this run" baseline into persistent state, so an
    /// arrival is announced exactly once even across restarts — a startup catch-up announces what landed
    /// while the app was closed, and never re-announces it on the next launch. Presence is the whole
    /// content (a set, like `inbox_archive`); the row is keyed by the task's `INTEGER` id.
    mailbox_notified {
        task_id: integer("PRIMARY KEY"),
    }

    /// The **lint-hook opt-out** — the projects amenbo must not wire the lint into on its own. A row is
    /// written by `hooks uninstall`: an explicit act on one repository, which the device-wide answer
    /// (`config.hook_consent`) would otherwise undo at the next startup by installing again.
    ///
    /// It is a set, not an answer: presence is the whole content, and there is no column for a `yes`
    /// because a `yes` is the absence of a veto plus the device's own answer ([`crate::hooks`]). It is
    /// never read as a mirror of what the hook directory actually holds.
    ///
    /// Keyed by the project's `INTEGER` id, and unlike `binding_path` it declares the reference: the
    /// veto is *about* the project, so it has nothing left to say once the project is gone, and the
    /// cascade retires the row without a GC pass.
    hook_optout {
        project_id: integer("PRIMARY KEY REFERENCES project(id) ON DELETE CASCADE"),
    }
}

/// Extra DDL not derived from any declaration: the journal mode and the read-model indexes the query
/// layer needs.
///
/// **Every index here must be creatable on the oldest store this build still opens** ([`super::migrate`]'s
/// baseline), because this batch runs at open, *before* the version chain has carried that store forward
/// — an index over a column a chain step adds would fail on exactly the store that step exists for. So an
/// index over a column a step adds belongs **in that step**, next to the `ALTER TABLE` that adds it, the
/// same division the tables already follow: this DDL is genesis, and the chain owns evolution
/// (`AMB-D-231`). A test holds the rule (`the_genesis_ddl_applies_to_a_baseline_store`).
const EXTRA_SQL: &str = r#"
PRAGMA journal_mode = WAL;

-- Foreign-key indexes for the read layer's correlated subqueries over child tables
-- (`read::list_task_ids`: the project placement EXISTS + the `order` sort's per-row placement
-- lookup and the ready/blocked dependency EXISTS). Without them every such subquery table-scans the
-- whole child table once per task — O(tasks × child), i.e. O(N²) on an unfiltered board page, which
-- is seconds rather than milliseconds on a board of ten thousand tasks.
CREATE INDEX IF NOT EXISTS task_dependency_by_task   ON task_dependency(task_id);
CREATE INDEX IF NOT EXISTS task_dependency_by_blocker ON task_dependency(blocked_by_id);
-- `text:` full-text spans comment bodies: `read::push_filter` adds an EXISTS over `task_comment` per
-- candidate task. Without this index that subquery scans every comment once per task; keyed by
-- `task_id` it seeks only the task's own comments (O(result)).
CREATE INDEX IF NOT EXISTS task_comment_by_task       ON task_comment(task_id);
-- A decision's `comment list` seeks its own comments by `decision_id` (mirrors `task_comment_by_task`),
-- so the read stays O(result) instead of scanning every decision comment.
CREATE INDEX IF NOT EXISTS decision_comment_by_decision ON decision_comment(decision_id);
-- The decision→decision edges, read in both directions. Forward (what this decision supersedes/amends)
-- seeks by `decision_id`; reverse (who superseded/amended it — a derived view, never a stored flag)
-- seeks by `target_decision_id`.
-- The forward index is UNIQUE over the pair, so one decision cannot hold two edges to the same target:
-- `supersedes` (the target is historicised) and `amends` (the target stays current) contradict. Dropping
-- an edge deletes the row, so it leaves the index and the pair can be drawn again.
CREATE UNIQUE INDEX IF NOT EXISTS decision_edge_pair ON decision_edge(decision_id, target_decision_id);
CREATE INDEX IF NOT EXISTS decision_edge_by_target ON decision_edge(target_decision_id);
-- FK index over the task↔value link of the dimension model, so an axis filter's EXISTS seeks a task's
-- own assignments instead of scanning the whole link table (the convention every child table follows:
-- task_dependency_by_task etc.). No read path consumes it yet; it is here so that moving the axes onto
-- the link table cannot reintroduce the O(N²) scan the other FK indexes exist to prevent.
CREATE INDEX IF NOT EXISTS task_dimension_value_by_task ON task_dimension_value(task_id);
-- A task's commit SHAs. The pair is UNIQUE so the same commit cannot be recorded twice on one task
-- (the ops layer reads this to stay idempotent, and the door normalises case so two spellings of one
-- SHA cannot slip past it). The `by_sha` index is the reverse chain (SHA → tasks) the later filter
-- seeks by; without it that lookup scans every row. `by_task` is the FK index every child table keeps,
-- so a task's own commits (and the ready/detail subqueries) seek instead of scanning the whole table.
CREATE UNIQUE INDEX IF NOT EXISTS task_commit_task_sha ON task_commit(task_id, sha);
CREATE INDEX IF NOT EXISTS task_commit_by_sha  ON task_commit(sha);
CREATE INDEX IF NOT EXISTS task_commit_by_task ON task_commit(task_id);
-- One plugin config override per (project, plugin, field): the triple is the natural key, so the write
-- boundary upserts a value by finding this row rather than appending a second. The unique index *is* that
-- constraint (the column decls carry no UNIQUE), and its leading columns also serve the reads that seek a
-- project's overrides — by `project_id`, or by `(project_id, plugin)` for one plugin's whole config.
CREATE UNIQUE INDEX IF NOT EXISTS plugin_config_triple ON plugin_config(project_id, plugin, field_key);
-- One enable override per (project, plugin): the pair is the natural key, so the trust boundary upserts the
-- gate by finding this row rather than appending a second answer. The unique index *is* that constraint,
-- and its leading column also serves a read that seeks one project's overrides.
CREATE UNIQUE INDEX IF NOT EXISTS plugin_enable_pair ON plugin_enable(project_id, plugin);
-- A plugin's queue, read the only way it is ever read: that plugin's own rows, oldest first. The pair is
-- the whole query (`plugin` seeks, `id` orders), so the runner reads its head without scanning the rows
-- queued for every other plugin, and the fan-out's "which plugins have work" seek stays on the index too.
CREATE INDEX IF NOT EXISTS plugin_queue_by_plugin ON plugin_queue(plugin, id);
-- The read layer's own two seeks over the task table: `status` narrows a mailbox query, and
-- `project_id` — placement is folded onto the task — scopes every list to one project.
CREATE INDEX IF NOT EXISTS task_by_status    ON task(status);
CREATE INDEX IF NOT EXISTS task_by_project   ON task(project_id);
-- The project slug's uniqueness. This index *is* the constraint — the column decl cannot carry
-- `UNIQUE` (see `SLUG`). NULLs are distinct in SQLite, so rows without a slug coexist.
CREATE UNIQUE INDEX IF NOT EXISTS project_by_slug ON project(slug);
"#;

/// The `CREATE TABLE IF NOT EXISTS` DDL for the plain tables: exactly the columns declared, in the order
/// declared, plus the table-level constraint if it has one. Nothing implicit — no `id`, no audit columns
/// (a plain table is not a record).
fn plain_tables_ddl(tables: &[PlainTable]) -> String {
    let mut sql = String::new();
    for t in tables {
        let width = t.columns.iter().map(|c| c.name.len()).max().unwrap_or(0);
        sql.push_str(&format!("CREATE TABLE IF NOT EXISTS {} (\n", t.name));
        let mut lines: Vec<String> =
            t.columns.iter().map(|c| format!("    {:<width$} {}", c.name, c.decl)).collect();
        lines.extend(t.constraint.map(|c| format!("    {c}")));
        sql.push_str(&lines.join(",\n"));
        sql.push_str("\n);\n");
    }
    sql
}

/// The per-dataset `CREATE TABLE IF NOT EXISTS` DDL for the registry: `id` primary key plus each
/// type-specific column and the universal [`AUDIT`] columns.
fn tables_ddl(datasets: &[Dataset]) -> String {
    let mut sql = String::new();
    for d in datasets {
        sql.push_str(&format!("CREATE TABLE IF NOT EXISTS {} (\n    id {RECORD_ID}", d.table));
        for c in d.columns.iter().chain(AUDIT) {
            sql.push_str(&format!(",\n    {} {}", c.name, c.decl));
        }
        sql.push_str("\n);\n");
    }
    sql
}

/// The `id` declaration for a **record** table: an `INTEGER PRIMARY KEY` — SQLite's rowid alias, so the
/// key is the B-tree itself and `task.id`/`decision.id` carry the conversational number.
/// `AUTOINCREMENT` is what makes that number a **name** now that deletes are physical: without it SQLite
/// reuses the largest deleted rowid, so deleting the newest task would hand its number to the next one,
/// and every reference to that number made outside the store would resolve to a different task. With it
/// the high-water mark is kept in `sqlite_sequence` and a delete never lowers it, so a number, once
/// issued, is retired ([`super::read::next_id`] reads that mark, not `MAX(id)`). The redundant-looking
/// `NOT NULL` is for whoever reads the schema back: a rowid alias never holds NULL, but SQLite does not
/// say so in `pragma_table_info` — the key reads as nullable there unless the declaration spells it out,
/// which would let the key be taken for an `Option` the store can never produce.
const RECORD_ID: &str = "INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL";

/// Build the base DDL for the truth-source: every read-model table (from the registry) plus the
/// engine and its indexes, and the [`PLAIN_TABLES`] beside them — the folder bindings and the
/// device-local task-keyed sets, none of which carries the record shape ([`crate::overview`] is the
/// read/write path onto those). The read model's secondary indexes ride along in `EXTRA_SQL`, under the
/// rule stated there. Every record table is born [`RECORD_ID`]-keyed; a store whose rows
/// are still ULID-keyed ([`is_legacy_keyed`]) is one `store::open` refuses rather than writing
/// integer-keyed tables beside its ULID-keyed ones.
pub fn schema_sql() -> String {
    let mut sql = tables_ddl(DATASETS);
    sql.push_str(&plain_tables_ddl(PLAIN_TABLES));
    sql.push_str(EXTRA_SQL);
    sql
}

/// The DDL for one dataset's table under `table` (which need not be the dataset's own name — a
/// rebuild migration creates the replacement beside the original). The column set comes from the
/// registry, so a rebuilt table is exactly what a fresh store would have gotten.
pub fn table_ddl(d: &Dataset, table: &str) -> String {
    let mut sql = format!("CREATE TABLE {table} (\n    id {RECORD_ID}");
    for c in d.all_columns() {
        sql.push_str(&format!(",\n    {} {}", c.name, c.decl));
    }
    sql.push_str("\n);\n");
    sql
}

/// Does this store still carry the **ULID `TEXT`** key space of a pre-consolidation store? Every store
/// this build writes keys its record tables on `INTEGER` ([`RECORD_ID`]), and this build has no way to
/// re-key a TEXT-keyed one, so the answer feeds a refusal rather than a branch
/// (`store::open::ensure_integer_keyed`) instead of half-migrating a store it cannot finish. Every
/// record table is asked, not just `task`, because a store can predate any one of them; a store
/// carrying none of them is new, and new means integer-keyed. The question is raw by necessity: what is
/// asked is *what the store's columns actually are* — the physical schema of a file this build has not
/// migrated — so it is asked through SQLite's own catalogue (`sqlite_master`, `pragma_table_info`).
/// Naming the `id` column through `col::` would only say what the registry believes, which is precisely
/// the belief under test.
pub fn is_legacy_keyed(conn: &rusqlite::Connection) -> rusqlite::Result<bool> {
    let tables: Vec<&str> = DATASETS.iter().map(|d| d.table).collect();
    let list = tables.iter().map(|t| format!("'{t}'")).collect::<Vec<_>>().join(",");
    conn.query_row(
        &format!(
            "SELECT EXISTS( \
               SELECT 1 FROM sqlite_master m JOIN pragma_table_info(m.name) c \
                 WHERE m.type = 'table' AND m.name IN ({list}) \
                   AND c.name = 'id' AND upper(c.type) <> 'INTEGER')"
        ),
        [],
        |r| r.get(0),
    )
}

/// Does this store hold **no record in any table** — the emptiness proof the writing open takes before
/// it may clear a pre-consolidation store to genesis (`store::open::reconcile_legacy_key_space`).
/// Every table SQLite carries is asked, **bar** its own `sqlite_%` bookkeeping and this
/// build's `store_meta` (store-level scalars — the schema/format stamps, never records); one row
/// anywhere else makes the store non-empty. The tables are enumerated from `sqlite_master`, not the
/// registry, on purpose: a pre-consolidation store carries tables the registry no longer declares
/// (`story`, `oplog`), and a row in one of those must count too — clearing a store that still holds one
/// would destroy it. Raw by necessity for the same reason [`is_legacy_keyed`] is: the question is asked
/// of a file this build has not migrated, so it is asked through SQLite's own catalogue.
pub fn table_content_is_empty(conn: &rusqlite::Connection) -> rusqlite::Result<bool> {
    let mut stmt = conn.prepare(
        "SELECT name FROM sqlite_master \
           WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name <> 'store_meta'",
    )?;
    let tables = stmt.query_map([], |r| r.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
    for table in tables {
        // `table` is an identifier read back from `sqlite_master`, not caller input; quote it as an
        // identifier (doubling any embedded quote) since it cannot be bound as a parameter.
        let has_row: bool = conn.query_row(
            &format!("SELECT EXISTS(SELECT 1 FROM \"{}\")", table.replace('"', "\"\"")),
            [],
            |r| r.get(0),
        )?;
        if has_row {
            return Ok(false);
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The device-local sets are keyed by the task's `INTEGER` id, like everything else the database
    /// holds, so no boundary that crosses them pays for a conversion.
    #[test]
    fn the_device_local_sets_carry_the_tasks_integer_key() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(&schema_sql()).unwrap();
        for table in ["read_receipt", "inbox_archive", "mailbox_notified"] {
            let ty: String = conn
                .query_row(
                    "SELECT type FROM pragma_table_info(?1) WHERE name = 'task_id'",
                    [table],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(ty, "INTEGER", "`{table}` is keyed by the task's integer id");
        }
    }

    /// A store this build writes is integer-keyed throughout; one whose record tables still carry ULID
    /// `TEXT` keys predates the consolidation, and saying so is what lets `open` refuse it instead of
    /// writing a second key space into it.
    #[test]
    fn a_ulid_keyed_store_is_recognised_as_pre_consolidation() {
        let fresh = rusqlite::Connection::open_in_memory().unwrap();
        fresh.execute_batch(&schema_sql()).unwrap();
        assert!(!is_legacy_keyed(&fresh).unwrap(), "a store this build creates is integer-keyed");

        let legacy = rusqlite::Connection::open_in_memory().unwrap();
        legacy.execute_batch("CREATE TABLE task (id TEXT PRIMARY KEY NOT NULL, title TEXT);").unwrap();
        assert!(is_legacy_keyed(&legacy).unwrap(), "a ULID-keyed record table gives it away");
    }
}
