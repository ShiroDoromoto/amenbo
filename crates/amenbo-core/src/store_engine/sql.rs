//! The typed SQL layer amenbo builds on rusqlite.
//!
//! It is not an ORM and not a driver: it is the thin layer that makes three failures hand-built SQL
//! can only meet at runtime impossible to write.
//!
//! **A condition and its bind values are never separated.** Held as two lists — `Vec<String>` of
//! `WHERE` fragments and `Vec<Value>` of parameters, aligned by nothing but the order the pushes
//! happened in — a forgotten value or a reordering makes every `?` past the slip bind to its
//! neighbour's value, and SQLite sees a well-formed query and answers the wrong question, silently.
//!
//! [`Pred`] carries the fragment and its values as one value. Composing predicates ([`Pred::and`],
//! [`Pred::or`], `!`, [`Pred::all`], [`Pred::any`]) moves the values with the fragment, so a
//! predicate can be built in any order, nested, negated, or handed to two different statements, and
//! the binding still lines up — there is no other list to keep in step with. [`Sql`] does the same for
//! the statement around it: text and binds go in through the same seam, so the placeholders a query
//! ends with are exactly the values it carries.
//!
//! **A column is never written as a string.** [`Col`] is a column identifier carrying the column's
//! type ([`Text`] / [`Int`] / [`Bool`]), and every `Col` in the codebase is *generated from the schema
//! registry* ([`super::schema::col`]) off the same declaration that emits the column's DDL — so a
//! column that is not in the store is a name that does not compile, and a registry rename lands on
//! every reader at once. The type travels with the identifier: [`Pred::eq`] and friends take the
//! column's own value type, so comparing a `TEXT` column against an `i64` (a predicate SQLite answers
//! `false` to, in silence) is a type error, and the text-only shapes ([`Pred::like`],
//! [`Pred::is_blank`]) cannot be aimed at an integer column at all.
//!
//! A query names its tables by alias, so a `Col` carries a **qualifier** rather than assuming its
//! table's own name: `schema::col::task::of("t")` hands back the same columns spelled `t.title`,
//! `t.status`, … (see [`super::schema::col`]). The same call also hands back the [`Table`] that alias
//! stands for, so a `FROM` and a `JOIN` name their table through the registry as well — which is what
//! lets the correlated subqueries ([`Exists`]) be built out of columns instead of spelled out.
//!
//! **A write is built the same way.** [`Insert`], [`Update`] and [`Delete`] take a column and the value
//! it is given in one act, so the two lists an `execute` would otherwise be handed — the columns spelled
//! into the SQL, and the `params![…]` beside it — are not a shape they can express. They end as an [`Sql`]
//! like any other statement, which is what carries the values to the driver.
//!
//! **What is still raw is a category, not an oversight.** Every statement over the store's own tables
//! goes through this layer. What does not falls into four such categories, each carrying its reason where
//! it is written, because each is a place the registry is the wrong authority to ask:
//!
//! 1. **SQLite's own bookkeeping** — `sqlite_master`, `sqlite_sequence`, `PRAGMA`. Not the store's tables,
//!    so `col::` cannot name them: the id high-water mark, the integrity check, the emptiness probes.
//! 2. **A table that arrives as a name at runtime** — the dataset-generic paths (`hydrate`, `export`, the
//!    existence probe): the caller walks in with the table, so there is no static column to be had.
//! 3. **A store this build has not migrated** — the physical-schema probes (`archive`'s verification,
//!    `probe_live_projects`, `is_legacy_keyed`). The registry describes the schema this binary migrates
//!    *to*, which is exactly what a read of an older file must not assume — naming a column through
//!    `col::` would make a store fail for holding precisely what it is supposed to hold.
//! 4. **A migration step** ([`super::migrate::Apply::Sql`]) — frozen at the meaning it had when written.
//!    Built from the registry it would *follow* the registry, and a rename tomorrow would change what a
//!    step did to stores years ago.
//!
//! Plus one shape SQL has and this layer does not: the recursive CTE in `read::dependency_reaches`, whose
//! frontier table exists only for the length of the statement.

use rusqlite::types::Value;
use rusqlite::Connection;
use std::marker::PhantomData;

/// The type of a column, as the predicate layer sees it: what SQLite stores, not what the column
/// *means* (a day, an instant, an enum and a content hash are all [`Text`] — their shape is held by
/// the registry's `CHECK`, not by Rust): the distinction a `TEXT` column compared against an integer
/// turns on.
pub trait ColType {}

/// A `TEXT` column (a title, a status, a day, an instant, a hash — see [`ColType`]).
#[derive(Debug, Clone, Copy)]
pub struct Text;
/// A 64-bit integer column: a key, a reference, a count.
#[derive(Debug, Clone, Copy)]
pub struct Int;
/// A `BOOLEAN` column — `0`/`1`, never "any integer".
#[derive(Debug, Clone, Copy)]
pub struct Bool;

impl ColType for Text {}
impl ColType for Int {}
impl ColType for Bool {}

/// Whether a column can hold `NULL`, carried in the type so a reader cannot choose the wrong Rust
/// shape for it. The registry already knows — `OPT`, `ts_opt`, `fk_opt`, `enum_opt`, `date_opt`,
/// `hash_opt` and the slug are nullable, everything else is `NOT NULL` — and [`super::schema::col`]
/// hands that knowledge out with the column: [`Read::Out`] is `String` for a `NOT NULL` text column and
/// `Option<String>` for a nullable one, so `r.get::<String>` on a column that is sometimes `NULL` — a
/// runtime `InvalidColumnType` on the first row that has none — is a type error instead.
pub trait Nullability {}

/// A column declared `NOT NULL`.
#[derive(Debug, Clone, Copy)]
pub struct NotNull;
/// A column that admits `NULL` — its value reads as an `Option`.
#[derive(Debug, Clone, Copy)]
pub struct Nullable;

impl Nullability for NotNull {}
impl Nullability for Nullable {}

/// A column identifier: its qualifier (the table, or the alias a query gave it) and its name, with the
/// column's type ([`ColType`]) and its [`Nullability`] along for the ride.
///
/// Never construct one by hand — [`super::schema::col`] generates them from the registry, which is what
/// makes a `Col` a promise that the column exists:
///
/// ```
/// use amenbo_core::store_engine::{schema::col, sql::Pred};
///
/// const T: col::task::Cols = col::task::of("t");
/// assert_eq!(Pred::eq(T.status, "done").sql(), "t.status = ?");
/// ```
///
/// A column the registry does not declare is a name that does not compile — a typo that would otherwise
/// reach SQLite and come back as zero rows:
///
/// ```compile_fail
/// use amenbo_core::store_engine::{schema::col, sql::Pred};
///
/// const T: col::task::Cols = col::task::of("t");
/// let _ = Pred::eq(T.statuss, "done");
/// ```
///
/// And so is a value of the wrong type. `status` is `TEXT`; compared against an integer, SQLite
/// answers `false` for every row and says nothing:
///
/// ```compile_fail
/// use amenbo_core::store_engine::{schema::col, sql::Pred};
///
/// const T: col::task::Cols = col::task::of("t");
/// let _ = Pred::eq(T.status, 7i64);
/// ```
///
/// The text-only shapes cannot be aimed at an integer column at all — `project_id` has no `''` to read
/// as "not written":
///
/// ```compile_fail
/// use amenbo_core::store_engine::{schema::col, sql::Pred};
///
/// const T: col::task::Cols = col::task::of("t");
/// let _ = Pred::is_blank(T.project_id);
/// ```
pub struct Col<T, N = NotNull> {
    table: &'static str,
    qualifier: &'static str,
    name: &'static str,
    ty: PhantomData<T>,
    null: PhantomData<N>,
}

/// Derived `Clone`/`Copy`/`Debug` would demand the same of `T`/`N` for the `PhantomData`, and the type
/// parameters are markers that are never held.
impl<T, N> Clone for Col<T, N> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T, N> Copy for Col<T, N> {}

impl<T, N> std::fmt::Debug for Col<T, N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.qualifier, self.name)
    }
}

impl<T, N> Col<T, N> {
    /// The generated constructor's seam (`super::schema::col`) — `const` so a query can name its own
    /// alias's columns in a `const`. The column carries the table it is **of** as well as the qualifier
    /// the query spells it with, which is what lets it hand back its own [`Table`].
    pub const fn new(table: &'static str, qualifier: &'static str, name: &'static str) -> Self {
        Self { table, qualifier, name, ty: PhantomData, null: PhantomData }
    }

    /// The column's name, unqualified — for the seams that speak in bare column names (the registry's
    /// write whitelist, an `UPDATE`'s `SET` list).
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// The table this column is of, under the qualifier it is spelled with — so a statement over a
    /// single table can name its `FROM` **through a column of it** rather than spelling the table a
    /// second time. What it buys over naming the table directly: the projection and the `FROM` are then
    /// the same table by construction, which the read layer's generic id lookups rest on.
    pub fn table(&self) -> Table {
        Table::new(self.table, self.qualifier)
    }
}

/// A table as a query names it: the table's own name, and the qualifier its columns are spelled with —
/// its alias, or the name itself. Never construct one by hand: [`super::schema::col`] hands it out with
/// the columns (`col::task::of("t").table`), off the same declaration that emits the table's DDL, so a
/// `FROM` cannot name a table the store does not have, nor one its columns were not asked for.
#[derive(Debug, Clone, Copy)]
pub struct Table {
    name: &'static str,
    alias: &'static str,
}

impl Table {
    /// The generated constructor's seam (`super::schema::col`).
    pub const fn new(name: &'static str, alias: &'static str) -> Self {
        Self { name, alias }
    }

    /// The table's own name, unaliased — what a write names (an `INSERT INTO` / `DELETE FROM` takes the
    /// table itself, never an alias for it).
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// How the table is written in a `FROM` / `JOIN`: `task_comment tc`, or just `task_comment` when
    /// the query does not alias it.
    pub fn to_sql(&self) -> String {
        if self.name == self.alias {
            self.name.to_owned()
        } else {
            format!("{} {}", self.name, self.alias)
        }
    }
}

/// A correlated subquery, built the way the rest of a statement is: `EXISTS (SELECT 1 FROM <table>
/// [JOIN <table> ON <pred>]… WHERE <pred>)`.
///
/// The tables come from the registry ([`Table`]) and the conditions are [`Pred`]s over that table's own
/// [`Col`]s, so no column inside the subquery is spelled as text and a rename reaches in here too; the
/// binds ride along with the fragments they belong to — the join's before the filter's, which is the
/// order the placeholders end up in.
///
/// ```
/// use amenbo_core::store_engine::{schema::col, sql::{Exists, Expr, Pred}};
///
/// const T: col::task::Cols = col::task::of("t");
/// const TC: col::task_comment::Cols = col::task_comment::of("tc");
///
/// let p = Exists::over(TC.table)
///     .filter(Pred::plain(format!("{} = {}", TC.task_id.to_sql(), T.id.to_sql())))
///     .filter(Pred::like(TC.text.lower(), "%x%"))
///     .pred();
///
/// assert_eq!(
///     p.sql(),
///     "EXISTS (SELECT 1 FROM task_comment tc \
///      WHERE (tc.task_id = t.id AND LOWER(tc.text) LIKE ? ESCAPE '\\'))"
/// );
/// assert_eq!(p.params().len(), 1);
/// ```
#[derive(Debug, Clone)]
pub struct Exists(Correlated);

impl Exists {
    /// The subquery's driving table.
    pub fn over(table: Table) -> Self {
        Self(Correlated::over("1", table))
    }

    /// ` JOIN <table> ON <pred>` — another table the subquery reaches, and the condition it is reached
    /// by (which may carry binds of its own).
    pub fn join(mut self, table: Table, on: Pred) -> Self {
        self.0.join(table, on);
        self
    }

    /// A condition on the subquery's rows — `AND`-ed with whatever was asked for before it, so the
    /// correlation (`tc.task_id = t.id`) and the filter can be stated separately.
    pub fn filter(mut self, p: Pred) -> Self {
        self.0.filter(p);
        self
    }

    /// The predicate: `EXISTS (…)`, with every bind the subquery carries, in placeholder order.
    pub fn pred(self) -> Pred {
        let sub = self.0.finish();
        Pred::raw(format!("EXISTS {}", sub.text()), sub.params().to_vec())
    }
}

/// The other shape of the same subquery: `(SELECT COUNT(*) FROM <table> [JOIN <table> ON <pred>]… WHERE
/// <pred>)` — **how many** rows the correlation reaches, where [`Exists`] asks only whether there is one.
///
/// It goes into a projection, not a `WHERE` ([`Select::count_of`]) — a decision's linked-task count, a
/// project's task count — which is why it hands back an [`Sql`] rather than a [`Pred`]: a scalar with binds.
/// The tables and columns are the registry's, as they are inside an `Exists`, so a rename lands here too
/// and the count cannot come to walk a different edge than the rows it counts for.
///
/// ```
/// use amenbo_core::store_engine::{schema::col, sql::{Count, Select, Sql, same}};
///
/// const P: col::project::Cols = col::project::of("p");
/// const T: col::task::Cols = col::task::of("t");
///
/// let mut sel = Select::new();
/// let num_tasks = sel.count_of(Count::over(T.table).filter(same(T.project_id, P.id)));
/// let sql = Sql::from(&sel, P.table);
///
/// assert_eq!(
///     sql.text(),
///     "SELECT (SELECT COUNT(*) FROM task t WHERE t.project_id = p.id) FROM project p"
/// );
/// # let _ = num_tasks;
/// ```
#[derive(Debug, Clone)]
pub struct Count(Correlated);

impl Count {
    /// The subquery's driving table — the rows being counted.
    pub fn over(table: Table) -> Self {
        Self(Correlated::over("COUNT(*)", table))
    }

    /// ` JOIN <table> ON <pred>` — another table the count reaches (a link row counted only where its
    /// referent is live).
    pub fn join(mut self, table: Table, on: Pred) -> Self {
        self.0.join(table, on);
        self
    }

    /// A condition on the counted rows — `AND`-ed with whatever was asked for before it.
    pub fn filter(mut self, p: Pred) -> Self {
        self.0.filter(p);
        self
    }

    /// The subquery as a scalar expression, with every bind it carries.
    pub fn sql(self) -> Sql {
        self.0.finish()
    }
}

/// The body a correlated subquery is: a driving table, the tables it joins, and the conditions on its
/// rows. Shared by the two shapes the readers ask for — [`Exists`] projects `1`, [`Count`] projects
/// `COUNT(*)` — which differ only in that projection and in what they hand back.
#[derive(Debug, Clone)]
struct Correlated {
    from: Sql,
    filter: Option<Pred>,
}

impl Correlated {
    fn over(projection: &str, table: Table) -> Self {
        Self {
            from: Sql::new(format!("SELECT {projection} FROM {}", table.to_sql())),
            filter: None,
        }
    }

    fn join(&mut self, table: Table, on: Pred) {
        self.from.push(format!(" JOIN {} ON ", table.to_sql())).push_pred(&on);
    }

    fn filter(&mut self, p: Pred) {
        self.filter = Some(match self.filter.take() {
            Some(prev) => prev.and(p),
            None => p,
        });
    }

    /// The subquery, parenthesised — the form both a predicate and a select item take it in — with its
    /// binds in placeholder order: the joins' before the filter's, which is the order their fragments sit
    /// in.
    fn finish(mut self) -> Sql {
        self.from.push_where(self.filter.as_ref());
        let mut sql = Sql::new("(");
        sql.push_sql(&self.from).push(")");
        sql
    }
}

impl<T: ColType> Col<T, NotNull> {
    /// Read this `NOT NULL` column as an optional one — for a column whose optionality is the
    /// **query's**, not the registry's (an arm of a [`Union`] whose sibling's column admits `NULL`; a
    /// column reached through a [`Sql::left_join`], which comes back `NULL` when the join finds no row).
    /// Widening only — a column that cannot be `NULL` read as an `Option` is simply always `Some`, and
    /// there is no way back the other way.
    pub const fn nullable(self) -> Col<T, Nullable> {
        Col::new(self.table, self.qualifier, self.name)
    }
}

impl<T: ColType> Col<T, Nullable> {
    /// Read this nullable column as a required one — the other direction, and unlike [`Col::nullable`] it
    /// is a **claim**: the statement's own `WHERE` is what makes the column present (`t.project_id` is
    /// nullable in the registry, and a query that asks only for placed tasks has excluded the rows where
    /// it is not). Wrong, it is the runtime `InvalidColumnType` the type was there to prevent, so state it
    /// only where the predicate beside it says why.
    pub const fn required(self) -> Col<T, NotNull> {
        Col::new(self.table, self.qualifier, self.name)
    }
}

impl<N: Nullability> Col<Text, N> {
    /// `LOWER(<col>)` — a case-folded text expression, still a text one (so it can still only be
    /// compared against text).
    pub fn lower(self) -> Lower {
        Lower(self.to_sql())
    }

    /// `substr(<col>, 1, 10)` — the **day** of a stored instant. An instant is a fixed-width UTC RFC3339
    /// string, so its first ten characters are its date, and comparing that against a day is a text
    /// comparison like any other (`completed_at` on a given day).
    pub fn day(self) -> Day {
        Day(self.to_sql())
    }
}

/// Something a predicate can be built about: a column, or an expression over one.
pub trait Expr {
    /// The type of the value the expression yields — what a comparison against it must supply.
    type Ty: ColType;
    /// The expression, written out in SQL.
    fn to_sql(&self) -> String;
}

impl<T: ColType, N: Nullability> Expr for Col<T, N> {
    type Ty = T;

    fn to_sql(&self) -> String {
        format!("{}.{}", self.qualifier, self.name)
    }
}

/// The result of [`Col::lower`] — a text expression that is not itself a column.
#[derive(Debug, Clone)]
pub struct Lower(String);

impl Expr for Lower {
    type Ty = Text;

    fn to_sql(&self) -> String {
        format!("LOWER({})", self.0)
    }
}

/// The result of [`Col::day`] — the date part of an instant, still text.
#[derive(Debug, Clone)]
pub struct Day(String);

impl Expr for Day {
    type Ty = Text;

    fn to_sql(&self) -> String {
        format!("substr({}, 1, 10)", self.0)
    }
}

/// A value that may be compared against a column of type `T` — the seam that stops an `i64` from
/// being handed to a `TEXT` column. Implemented for exactly the Rust types each column type admits.
pub trait IntoSql<T: ColType> {
    /// The bind value.
    fn into_sql(self) -> Value;
}

impl IntoSql<Text> for &str {
    fn into_sql(self) -> Value {
        Value::Text(self.to_owned())
    }
}
impl IntoSql<Text> for String {
    fn into_sql(self) -> Value {
        Value::Text(self)
    }
}
impl IntoSql<Int> for i64 {
    fn into_sql(self) -> Value {
        Value::Integer(self)
    }
}
impl IntoSql<Bool> for bool {
    fn into_sql(self) -> Value {
        Value::Integer(self as i64)
    }
}

/// A `WHERE`-clause fragment together with the values its placeholders bind, as one value. The fragment
/// is always parenthesised when it is composite, so a predicate can be dropped into any context without
/// its precedence changing under it.
#[derive(Debug, Clone)]
pub struct Pred {
    sql: String,
    params: Vec<Value>,
}

impl Pred {
    /// A fragment written out in SQL, with the values for its `?` placeholders — in placeholder order.
    /// This is the seam for the shapes the constructors below do not cover (a `CASE` over an enum's own
    /// words, a scalar subquery); a correlated subquery is not one of them — it has a shape of its own
    /// ([`Exists`]), which names its tables and columns through the registry. The two arguments are given
    /// together, which is the whole point: past this call there is no way to hold one without the other,
    /// and debug builds assert that the fragment's placeholder count matches the values handed with it.
    pub fn raw(sql: impl Into<String>, params: Vec<Value>) -> Self {
        let sql = sql.into();
        debug_assert_eq!(
            placeholder_count(&sql),
            params.len(),
            "predicate fragment and its bind values disagree: {sql}"
        );
        Self { sql, params }
    }

    /// A fragment with no bind values of its own (`t.assignee_kind = 'ai'`, `1 = 0`).
    pub fn plain(sql: impl Into<String>) -> Self {
        Self::raw(sql, Vec::new())
    }

    /// `<col> <op> ?` — the comparison and the value it compares against, in the column's own type.
    pub fn cmp<E: Expr>(col: E, op: &str, v: impl IntoSql<E::Ty>) -> Self {
        Self::raw(format!("{} {op} ?", col.to_sql()), vec![v.into_sql()])
    }

    /// `<col> = ?`
    pub fn eq<E: Expr>(col: E, v: impl IntoSql<E::Ty>) -> Self {
        Self::cmp(col, "=", v)
    }

    /// `<col> <> ?`
    pub fn ne<E: Expr>(col: E, v: impl IntoSql<E::Ty>) -> Self {
        Self::cmp(col, "<>", v)
    }

    /// `<col> IS NULL` — unset, which for a nullable column is a different question from [`is_blank`].
    ///
    /// [`is_blank`]: Pred::is_blank
    pub fn is_null<E: Expr>(col: E) -> Self {
        Self::plain(format!("{} IS NULL", col.to_sql()))
    }

    /// `<col> IS NOT NULL`.
    pub fn is_not_null<E: Expr>(col: E) -> Self {
        Self::plain(format!("{} IS NOT NULL", col.to_sql()))
    }

    /// `<col> LIKE ? ESCAPE '\'` — a substring/prefix match whose pattern binds as a value; text
    /// columns only. The `ESCAPE` is the pair to the caller's escaping of `%` / `_` / `\` in the
    /// pattern ([`super::search::escape_like`]): a user's literal `%` must not become a wildcard.
    pub fn like<E: Expr<Ty = Text>>(col: E, pattern: impl Into<String>) -> Self {
        Self::raw(format!("{} LIKE ? ESCAPE '\\'", col.to_sql()), vec![Value::Text(pattern.into())])
    }

    /// `<col> IN (?, …)` — one placeholder per value, however many there are, each in the column's own
    /// type. An empty set matches nothing (`1 = 0`), which is what `IN ()` would mean if SQLite would
    /// parse it.
    pub fn is_in<E: Expr>(col: E, values: impl IntoIterator<Item = impl IntoSql<E::Ty>>) -> Self {
        let params: Vec<Value> = values.into_iter().map(IntoSql::into_sql).collect();
        if params.is_empty() {
            return Self::never();
        }
        let marks = vec!["?"; params.len()].join(", ");
        Self::raw(format!("{} IN ({marks})", col.to_sql()), params)
    }

    /// `(<col> IS NULL OR <col> = '')` — the store's "not written" reading of a **text** column, where
    /// the empty string is the field-by-field create's not-yet-written sentinel (see [`super::schema`])
    /// and reads the same as absent. An integer column has no such sentinel, and the type says so.
    pub fn is_blank<E: Expr<Ty = Text>>(col: E) -> Self {
        let col = col.to_sql();
        Self::plain(format!("({col} IS NULL OR {col} = '')"))
    }

    /// A predicate no row satisfies.
    pub fn never() -> Self {
        Self::plain("1 = 0")
    }

    /// `(<self> AND <other>)`, values in fragment order.
    pub fn and(self, other: Pred) -> Self {
        self.join("AND", other)
    }

    /// `(<self> OR <other>)`, values in fragment order.
    pub fn or(self, other: Pred) -> Self {
        self.join("OR", other)
    }

    /// `NOT (<self>)` when `negate`, else `self` — the shape a `bool` filter takes (`ready:yes|no`).
    pub fn negated_if(self, negate: bool) -> Self {
        if negate {
            !self
        } else {
            self
        }
    }

    /// Every predicate, `AND`-ed; `None` when there are none (an unfiltered read carries no `WHERE`
    /// at all, not a tautological one).
    pub fn all(preds: impl IntoIterator<Item = Pred>) -> Option<Self> {
        preds.into_iter().reduce(Pred::and)
    }

    /// Any predicate, `OR`-ed; `None` when there are none.
    pub fn any(preds: impl IntoIterator<Item = Pred>) -> Option<Self> {
        preds.into_iter().reduce(Pred::or)
    }

    /// The fragment. Its placeholders bind [`Pred::params`], in this order — the two are only ever
    /// read out together, by [`Sql::push_pred`] and [`Sql::push_where`].
    pub fn sql(&self) -> &str {
        &self.sql
    }

    /// The values the fragment's placeholders bind, in placeholder order.
    pub fn params(&self) -> &[Value] {
        &self.params
    }

    fn join(mut self, op: &str, other: Pred) -> Self {
        self.sql = format!("({} {op} {})", self.sql, other.sql);
        self.params.extend(other.params);
        self
    }
}

/// `NOT (<pred>)`, keeping the values — negation is `!`, like it is everywhere else.
impl std::ops::Not for Pred {
    type Output = Pred;

    fn not(self) -> Pred {
        Pred { sql: format!("NOT ({})", self.sql), params: self.params }
    }
}

/// `<a> = <b>` — a join condition or a correlation, where both sides are columns and neither is a value
/// (so the predicate carries no bind: [`Pred::eq`]'s `?` has nothing to bind here). Both columns are the
/// registry's, and they must be of the same type — an `INTEGER` key is not joined to a `TEXT` one.
///
/// ```
/// use amenbo_core::store_engine::{schema::col, sql::same};
///
/// const T: col::task::Cols = col::task::of("t");
/// const D: col::task_dependency::Cols = col::task_dependency::of("d");
///
/// assert_eq!(same(T.id, D.task_id).sql(), "t.id = d.task_id");
/// ```
pub fn same<T: ColType, N: Nullability, M: Nullability>(a: Col<T, N>, b: Col<T, M>) -> Pred {
    Pred::plain(format!("{} = {}", a.to_sql(), b.to_sql()))
}

/// One key of an [`Sql::order_by`]: what the rows are ordered on, and which way. The default is
/// ascending, which is SQLite's too — a key says `DESC` only where the code means it, so the direction
/// is never carried by a `dir` variable spliced into the text.
///
/// ```
/// use amenbo_core::store_engine::{schema::col, sql::{Select, Sort, Sql}};
///
/// const T: col::task::Cols = col::task::of("t");
/// let mut sel = Select::new();
/// let id = sel.col(T.id);
/// let mut sql = Sql::from(&sel, T.table);
/// sql.order_by([Sort::by(T.due_on).desc(), Sort::by(T.id)]).limit(20);
///
/// assert_eq!(sql.text(), "SELECT t.id FROM task t ORDER BY t.due_on DESC, t.id LIMIT ?");
/// # let _ = id;
/// ```
#[derive(Debug, Clone)]
pub struct Sort(String);

impl Sort {
    /// Order on a column (or an expression over one) — ascending.
    pub fn by<E: Expr>(e: E) -> Self {
        Self(e.to_sql())
    }

    /// Order on something the registry cannot type — a `CASE` that ranks an enum's own words, a
    /// `COALESCE` that reads an unwritten column as empty. The counterpart of [`Select::expr`], and for
    /// the same reason: SQL can say it and the registry cannot.
    pub fn expr(sql: impl Into<String>) -> Self {
        Self(sql.into())
    }

    /// Descending.
    pub fn desc(mut self) -> Self {
        self.0.push_str(" DESC");
        self
    }

    /// Descending when `descending` — the shape a sort key parsed at runtime takes (`-due`), where the
    /// direction is the user's and the columns are still the registry's.
    pub fn dir(self, descending: bool) -> Self {
        if descending {
            self.desc()
        } else {
            self
        }
    }
}

/// A statement under construction: SQL text and the values its placeholders bind, appended through the
/// same seam so they cannot fall out of step.
#[derive(Debug, Clone, Default)]
pub struct Sql {
    text: String,
    params: Vec<Value>,
}

impl Sql {
    /// Start a statement from text that carries no placeholders.
    pub fn new(text: impl AsRef<str>) -> Self {
        let mut s = Self::default();
        s.push(text);
        s
    }

    /// Start a statement from a projection: `SELECT <list>`, with whatever values the list's items bind
    /// ([`Select`]). The statement goes on from there — ` FROM …`, a `WHERE`, an `ORDER BY`.
    pub fn select(sel: &Select) -> Self {
        let mut s = Self::default();
        s.push(if sel.is_distinct() { "SELECT DISTINCT " } else { "SELECT " }).push_select(sel);
        s
    }

    /// Start a read: `SELECT <list> FROM <table>` — the projection and the table it is read from, both
    /// named through the registry ([`Table`]).
    pub fn from(sel: &Select, table: Table) -> Self {
        let mut s = Self::select(sel);
        s.push_sql(&Self::from_table(table));
        s
    }

    /// The **tail** of a read — ` FROM <table>`, with the projection built elsewhere. That is the shape a
    /// [`Union`] arm hands back: the arm's `SELECT` list is the union's to place (the first arm's is the
    /// row shape), so the arm builds only what follows it.
    pub fn from_table(table: Table) -> Self {
        Self::new(format!(" FROM {}", table.to_sql()))
    }

    /// Start a read over a **derived table**: `SELECT <list> FROM (<inner>) <alias>` — the rows of another
    /// statement, read as if they were a table. The inner statement goes in through [`Sql::push_sql`], so
    /// whatever it binds is carried along; the outer projection names the derived table's columns by the
    /// alias it is given here (`col::task::of("g")`), which is what keeps the two halves speaking of the
    /// same rows.
    pub fn from_sub(sel: &Select, inner: &Sql, alias: &str) -> Self {
        let mut s = Self::select(sel);
        s.push(" FROM (").push_sql(inner).push(format!(") {alias}"));
        s
    }

    /// Append ` GROUP BY <expr>, …` — the rows folded into one per distinct value of these expressions.
    /// Grammar, not data: an aggregate has no value to bind, and the expressions are the registry's
    /// columns ([`Expr::to_sql`]), so a fold cannot name a column the table does not have. Pair it with
    /// [`Sql::having`] to keep only some of the groups.
    pub fn group_by(&mut self, exprs: impl IntoIterator<Item = String>) -> &mut Self {
        let list = exprs.into_iter().collect::<Vec<_>>().join(", ");
        self.push(format!(" GROUP BY {list}"))
    }

    /// Append ` HAVING <pred>` — a condition on the **groups** a [`Sql::group_by`] made, where a `WHERE`
    /// is a condition on the rows that went into them. A predicate like any other, binds and all.
    pub fn having(&mut self, p: &Pred) -> &mut Self {
        self.push(" HAVING ").push_pred(p)
    }

    /// Append ` ORDER BY <key>, …` — the order the rows come back in, each key a [`Sort`] over the
    /// registry's columns. Grammar like [`Sql::group_by`]: an order has no value to bind, and the columns
    /// it reads cannot be ones the table does not have.
    pub fn order_by(&mut self, keys: impl IntoIterator<Item = Sort>) -> &mut Self {
        let list = keys.into_iter().map(|k| k.0).collect::<Vec<_>>().join(", ");
        self.push(format!(" ORDER BY {list}"))
    }

    /// Append ` LIMIT ?` — how many rows at most, as a **bound value**: a page size is data (it comes from
    /// the caller), where the order is grammar. `-1` is SQLite's "no limit", which is what an unpaged read
    /// asks for when it still wants an `OFFSET`.
    pub fn limit(&mut self, n: i64) -> &mut Self {
        self.push(" LIMIT ").bind(n)
    }

    /// Append ` OFFSET ?` — how many rows to skip, likewise a bound value.
    pub fn offset(&mut self, n: i64) -> &mut Self {
        self.push(" OFFSET ").bind(n)
    }

    /// Append ` JOIN <table> ON <pred>` — another table this statement reaches, and the condition it is
    /// reached by (which may carry binds of its own, like any predicate). Written out as text, `ON t.id =
    /// d.task_id` spells its columns by hand, and swapping one for a **column that also exists**
    /// (`d.blocked_by_id`) still prepares: SQLite walks the other edge and answers, in silence, about rows
    /// nobody asked for. A [`Pred`] over the registry's [`Col`]s ([`same`]) cannot name a column the table
    /// does not have, and cannot compare two columns of different types.
    pub fn join(&mut self, table: Table, on: Pred) -> &mut Self {
        self.push(format!(" JOIN {} ON ", table.to_sql())).push_pred(&on)
    }

    /// Append ` LEFT JOIN <table> ON <pred>` — a table this statement reaches **if there is a row there**:
    /// the rows of the statement stand whether or not the join finds one, and every column it reaches comes
    /// back `NULL` when it does not (a decision whose project was deleted still lists, with no project
    /// name). That is the one thing the registry cannot say: the column is `NOT NULL`, and through this
    /// join it is absent anyway. The projection says so with [`Col::nullable`], so the row still reads
    /// through the registry's own column rather than a hand-typed expression.
    pub fn left_join(&mut self, table: Table, on: Pred) -> &mut Self {
        self.push(format!(" LEFT JOIN {} ON ", table.to_sql())).push_pred(&on)
    }

    /// Append a projection's list and the values its items bind — the seam a `SELECT` list goes in
    /// through, so an item carrying a bind ([`Select::count_if`]) cannot lose it on the way.
    pub fn push_select(&mut self, sel: &Select) -> &mut Self {
        let (list, params) = sel.parts();
        self.text.push_str(&list);
        self.params.extend(params.iter().cloned());
        self
    }

    /// Append text with no bind values of its own (a join, an `ORDER BY`).
    pub fn push(&mut self, text: impl AsRef<str>) -> &mut Self {
        let text = text.as_ref();
        debug_assert_eq!(placeholder_count(text), 0, "placeholder without a value: {text}");
        self.text.push_str(text);
        self
    }

    /// Append a predicate's fragment and its values.
    pub fn push_pred(&mut self, p: &Pred) -> &mut Self {
        self.text.push_str(p.sql());
        self.params.extend(p.params().iter().cloned());
        self
    }

    /// Append another statement — its text and the values it carries — as a piece of this one: the seam
    /// for a subquery built the same way (a derived table an outer query groups over). The inner
    /// statement's binds land in the outer list at the position its placeholders sit at, which is the
    /// whole reason a subquery may be composed at all: text and values still go in together.
    pub fn push_sql(&mut self, inner: &Sql) -> &mut Self {
        self.text.push_str(inner.text());
        self.params.extend(inner.params().iter().cloned());
        self
    }

    /// Append ` WHERE <pred>` — or nothing at all when there is no predicate.
    pub fn push_where(&mut self, p: Option<&Pred>) -> &mut Self {
        if let Some(p) = p {
            self.text.push_str(" WHERE ");
            self.push_pred(p);
        }
        self
    }

    /// Append a `?` and the value it binds.
    pub fn bind(&mut self, v: impl Into<Value>) -> &mut Self {
        self.text.push('?');
        self.params.push(v.into());
        self
    }

    /// The statement text.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The values, in placeholder order — ready for `rusqlite::params_from_iter`.
    pub fn params(&self) -> &[Value] {
        &self.params
    }

    /// Run the statement on `conn`, binding exactly the values it carries; hands back the rows it
    /// changed. `conn` may be a transaction (it derefs to a connection), which is how a write goes
    /// inside the operation's batch. The binding is not something the caller does: text and values leave
    /// through the same seam they entered by, so there is no second list to hand `execute` alongside the
    /// SQL.
    pub fn execute(&self, conn: &Connection) -> rusqlite::Result<usize> {
        conn.execute(self.text(), rusqlite::params_from_iter(self.params()))
    }
}

/// What a row-writing statement does with a column: name it, and carry the value it takes. The three
/// builders below ([`Insert`], [`Update`], [`Delete`]) share it, so a column and its value are entered
/// in one act on every write — a list of column names beside a `params![…]` list, counted off against
/// each other by eye, is not a shape they can express.
#[derive(Debug, Clone, Default)]
struct Assignments {
    cols: Vec<String>,
    values: Vec<Value>,
}

impl Assignments {
    fn push(&mut self, col: &str, v: Value) {
        self.cols.push(col.to_owned());
        self.values.push(v);
    }
}

/// A column of a write, quoted — a registry name is a plain identifier, and quoting it keeps one that
/// happens to be spelled like a keyword from ever being read as one.
fn quoted(col: &str) -> String {
    format!("\"{col}\"")
}

/// What an `INSERT` does when the row it writes is already there.
#[derive(Debug, Clone)]
enum Conflict {
    /// Nothing: SQLite raises the constraint failure.
    Fail,
    /// `ON CONFLICT(<key>) DO NOTHING` — the row stands as it is.
    DoNothing(&'static str),
    /// `ON CONFLICT(<key>) DO UPDATE SET <every other column> = excluded.<column>` — an upsert.
    DoUpdate(&'static str),
}

/// An `INSERT`, with the columns it writes and the values they take entering together — and, where the
/// row may already be there, what to do about it (`DO NOTHING`, or an upsert onto the same columns).
///
/// ```
/// use amenbo_core::store_engine::{schema::col, sql::Insert};
///
/// const RR: col::read_receipt::Cols = col::read_receipt::ALL;
/// let stmt = Insert::into(RR.table)
///     .set(RR.task_id, 412i64)
///     .set(RR.last_seen, "2026-07-14T00:00:00Z")
///     .on_conflict_update(RR.task_id)
///     .sql();
///
/// assert_eq!(
///     stmt.text(),
///     "INSERT INTO read_receipt (\"task_id\", \"last_seen\") VALUES (?, ?) \
///      ON CONFLICT(\"task_id\") DO UPDATE SET \"last_seen\" = excluded.\"last_seen\""
/// );
/// assert_eq!(stmt.params().len(), 2);
/// ```
///
/// The value is the column's own type, as it is in a predicate — a `TEXT` column handed an integer does
/// not compile:
///
/// ```compile_fail
/// use amenbo_core::store_engine::{schema::col, sql::Insert};
///
/// const RR: col::read_receipt::Cols = col::read_receipt::ALL;
/// let _ = Insert::into(RR.table).set(RR.last_seen, 7i64);
/// ```
#[derive(Debug, Clone)]
pub struct Insert {
    table: Table,
    set: Assignments,
    conflict: Conflict,
}

impl Insert {
    /// Insert into `table` — named through the registry (`col::<table>::ALL.table`), or through the
    /// [`super::schema::Dataset`] a dataset-generic write reached its table by.
    pub fn into(table: Table) -> Self {
        Self { table, set: Assignments::default(), conflict: Conflict::Fail }
    }

    /// Write `v` into `col`, in the column's own type.
    pub fn set<T: ColType, N: Nullability>(mut self, col: Col<T, N>, v: impl IntoSql<T>) -> Self {
        self.set.push(col.name(), v.into_sql());
        self
    }

    /// Write `v` into `col`, or `NULL` when there is none — the shape a scalar that round-trips as
    /// absent takes. **Only a column that admits `NULL`**: the registry knows which do, so writing an
    /// absent value into a `NOT NULL` column is a type error here rather than the constraint failure
    /// SQLite would raise at runtime:
    ///
    /// ```compile_fail
    /// use amenbo_core::store_engine::{schema::col, sql::Insert};
    ///
    /// const RR: col::read_receipt::Cols = col::read_receipt::ALL;
    /// let _ = Insert::into(RR.table).set_opt(RR.last_seen, None::<&str>);
    /// ```
    pub fn set_opt<T: ColType>(
        mut self,
        col: Col<T, Nullable>,
        v: Option<impl IntoSql<T>>,
    ) -> Self {
        self.set.push(col.name(), v.map_or(Value::Null, IntoSql::into_sql));
        self
    }

    /// Write an already-typed value into a column the caller names at **runtime** — the seam for the
    /// dataset-generic write path ([`super::StoreEngine::set_field`]), where the dataset is a value and
    /// its columns cannot be named statically. The registry whitelist is what stands in for the type
    /// there ([`super::schema::Dataset::writable`]); what this still guarantees is the other half — the
    /// value goes in with its column, and past this call there is no way to hold one without the other.
    pub fn set_value(mut self, col: &str, v: Value) -> Self {
        self.set.push(col, v);
        self
    }

    /// `ON CONFLICT(<key>) DO NOTHING` — a row already keyed by `key` is left exactly as it is.
    pub fn on_conflict_do_nothing<T: ColType, N: Nullability>(mut self, key: Col<T, N>) -> Self {
        self.conflict = Conflict::DoNothing(key.name());
        self
    }

    /// `ON CONFLICT(<key>) DO UPDATE SET …` — an upsert: every column this insert names *except* the
    /// key takes the value the insert brought.
    pub fn on_conflict_update<T: ColType, N: Nullability>(mut self, key: Col<T, N>) -> Self {
        self.conflict = Conflict::DoUpdate(key.name());
        self
    }

    /// The statement, with its values.
    pub fn sql(&self) -> Sql {
        let mut sql = Sql::default();
        sql.push(format!("INSERT INTO {} (", self.table.name()));
        sql.push(self.set.cols.iter().map(|c| quoted(c)).collect::<Vec<_>>().join(", "));
        sql.push(") VALUES (");
        for (i, v) in self.set.values.iter().enumerate() {
            if i > 0 {
                sql.push(", ");
            }
            sql.bind(v.clone());
        }
        sql.push(")");
        match self.conflict {
            Conflict::Fail => {}
            Conflict::DoNothing(key) => {
                sql.push(format!(" ON CONFLICT({}) DO NOTHING", quoted(key)));
            }
            Conflict::DoUpdate(key) => {
                let updates: Vec<String> = self
                    .set
                    .cols
                    .iter()
                    .filter(|c| c.as_str() != key)
                    .map(|c| format!("{q} = excluded.{q}", q = quoted(c)))
                    .collect();
                sql.push(format!(
                    " ON CONFLICT({}) DO UPDATE SET {}",
                    quoted(key),
                    updates.join(", ")
                ));
            }
        }
        sql
    }
}

/// An `UPDATE`: the columns it writes with their values, and the predicate that picks the rows — one
/// value, so the `SET` list and the `WHERE` cannot bind each other's values.
#[derive(Debug, Clone)]
pub struct Update {
    table: Table,
    set: Assignments,
    filter: Option<Pred>,
}

impl Update {
    /// Update `table` — named through the registry, as [`Insert::into`] is.
    pub fn table(table: Table) -> Self {
        Self { table, set: Assignments::default(), filter: None }
    }

    /// Write an already-typed value into a column named at runtime — see [`Insert::set_value`].
    pub fn set_value(mut self, col: &str, v: Value) -> Self {
        self.set.push(col, v);
        self
    }

    /// The rows to write. An update with no predicate writes every row — which a caller has to mean,
    /// there being no `WHERE` to drop by accident.
    pub fn filter(mut self, p: Pred) -> Self {
        self.filter = Some(p);
        self
    }

    /// The statement, with its values: the `SET` values first, then the predicate's — the order their
    /// placeholders sit in, which is not the caller's to keep.
    pub fn sql(&self) -> Sql {
        let mut sql = Sql::default();
        sql.push(format!("UPDATE {} SET ", self.table.name()));
        for (i, (col, v)) in self.set.cols.iter().zip(&self.set.values).enumerate() {
            if i > 0 {
                sql.push(", ");
            }
            sql.push(format!("{} = ", quoted(col)));
            sql.bind(v.clone());
        }
        sql.push_where(self.filter.as_ref());
        sql
    }
}

/// A `DELETE`: the table, and the predicate that picks the rows it takes.
#[derive(Debug, Clone)]
pub struct Delete {
    table: Table,
    filter: Option<Pred>,
}

impl Delete {
    /// Delete from `table` — named through the registry, as [`Insert::into`] is.
    pub fn from(table: Table) -> Self {
        Self { table, filter: None }
    }

    /// The rows to delete; without one, every row of the table goes.
    pub fn filter(mut self, p: Pred) -> Self {
        self.filter = Some(p);
        self
    }

    /// The statement, with the predicate's values.
    pub fn sql(&self) -> Sql {
        let mut sql = Sql::new(format!("DELETE FROM {}", self.table.name()));
        sql.push_where(self.filter.as_ref());
        sql
    }
}

/// What a column reads back as: the Rust type its SQLite type and its [`Nullability`] together admit.
/// A `NOT NULL` text column is a `String`; the same column nullable is an `Option<String>`. There is no
/// other pairing, which is what makes a row mapping unable to guess wrong.
pub trait Read {
    /// The value this column yields from a row.
    type Out: rusqlite::types::FromSql;
}

impl Read for Col<Text, NotNull> {
    type Out = String;
}
impl Read for Col<Text, Nullable> {
    type Out = Option<String>;
}
impl Read for Col<Int, NotNull> {
    type Out = i64;
}
impl Read for Col<Int, Nullable> {
    type Out = Option<i64>;
}
impl Read for Col<Bool, NotNull> {
    type Out = bool;
}
impl Read for Col<Bool, Nullable> {
    type Out = Option<bool>;
}

/// A `SELECT` list being built, handing back a typed [`Slot`] for every item it takes: the list a query
/// selects and the reads that take its row apart come from the same act, so they cannot fall out of
/// step. Paired with a hand-written list of columns, a `r.get(0)`, `r.get(1)`, … counted off by eye
/// takes its neighbour's value the moment a column is inserted or two are reordered — same types, no
/// error, wrong row. Here the index is not written down at all: [`Select::col`] returns the slot that
/// *is* the position it just appended, and a slot can only be read from a row of the query it came from.
///
/// ```
/// use amenbo_core::store_engine::{schema::col, sql::{Count, Select, Sql, same}};
///
/// const T: col::task::Cols = col::task::of("t");
/// const C: col::task_comment::Cols = col::task_comment::of("c");
///
/// let mut sel = Select::new();
/// let id = sel.col(T.id);           // i64        — INTEGER PRIMARY KEY
/// let title = sel.col(T.title);     // String     — TEXT NOT NULL
/// let due = sel.col(T.due_on);      // Option<..> — nullable in the registry
/// let comments = sel.count_of(Count::over(C.table).filter(same(C.task_id, T.id)));
///
/// let sql = Sql::from(&sel, T.table);
/// assert_eq!(
///     sql.text(),
///     "SELECT t.id, t.title, t.due_on, \
///      (SELECT COUNT(*) FROM task_comment c WHERE c.task_id = t.id) FROM task t"
/// );
/// # let _ = (id, title, due, comments);
/// ```
///
/// **A select item may carry bind values of its own** ([`Select::count_if`]) —
/// a bucket counted inside an aggregate is a condition with a value in it, like any other. That is why
/// the list is only ever handed to a statement through [`Sql::select`] / [`Sql::push_select`], which take
/// its text and its values together: there is no way to splice the list in as a string and leave its
/// binds behind.
///
/// The nullability comes from the registry, so the shape a column reads back as is not a guess:
/// asking a nullable column for a bare `String` — an `InvalidColumnType` on the first row that has no
/// value, and only on that row — does not compile.
///
/// ```compile_fail
/// use amenbo_core::store_engine::{schema::col, sql::Select};
///
/// const T: col::task::Cols = col::task::of("t");
/// let mut sel = Select::new();
/// let due: amenbo_core::store_engine::sql::Slot<String> = sel.col(T.due_on);
/// ```
///
/// A query whose rows come from several arms ([`Union`]) projects one shape across all of them: each arm
/// builds its own list, and the compiler holds them to the same slots.
///
/// **One thing the registry cannot say: an outer join.** A `NOT NULL` column reached through a
/// [`Sql::left_join`] comes back `NULL` when the join finds no row — the optionality is the join's, not
/// the column's. The column is still selected as itself, widened by [`Col::nullable`] (`p.name` as an
/// `Option<String>` in `read::decision_list`): what the query knows and the registry does not is stated
/// at the one seam that knows it, and the identifier stays the registry's.
#[derive(Debug, Clone, Default)]
pub struct Select {
    items: Vec<String>,
    params: Vec<Value>,
    distinct: bool,
}

impl Select {
    /// An empty list.
    pub fn new() -> Self {
        Self::default()
    }

    /// `SELECT DISTINCT` — fold rows this list cannot tell apart into one. It belongs to the projection,
    /// not to the statement: what "the same row" means is exactly what the list names (the distinct blob
    /// hashes an attachment table holds, however many rows reference each).
    pub fn distinct(&mut self) -> &mut Self {
        self.distinct = true;
        self
    }

    /// `COUNT(*)` — how many rows the statement matches, as its own projection. The count of a `WHERE`,
    /// not of a group: the rows the predicate leaves.
    pub fn count_all(&mut self) -> Slot<i64> {
        self.push("COUNT(*)".to_owned(), Vec::new())
    }

    /// Append a column, and hand back the slot that reads it — typed by the registry (its SQLite type
    /// and whether it admits `NULL`).
    pub fn col<C: Read + Expr>(&mut self, c: C) -> Slot<C::Out> {
        self.push(c.to_sql(), Vec::new())
    }

    /// Append an expression the registry cannot name — an aggregate (`MAX`, `MIN`), a `COALESCE` over a
    /// column older rows never carried, a `CASE` — with the type the caller reads it as. The type is a
    /// claim (SQLite is asked for it at runtime); a column's is not. The shapes that have a seam of their
    /// own go through it instead: a plain row count is [`Select::count_all`], a correlated one is
    /// [`Select::count_of`] ([`Count`]), a condition read back per row is [`Select::pred`], and a bucket
    /// inside an aggregate is [`Select::count_if`]. What is left here is exactly what SQL can express and
    /// the registry cannot. The expression carries no bind values — an item that does is a condition with
    /// a value in it.
    pub fn expr<V: rusqlite::types::FromSql>(&mut self, sql: impl Into<String>) -> Slot<V> {
        self.push(sql.into(), Vec::new())
    }

    /// `COALESCE(SUM(<pred>), 0)` — how many of the grouped rows satisfy a condition, as a select item.
    /// A predicate is a `0`/`1` in SQLite, so a bucket is its sum; `COALESCE` because summing no rows is
    /// `NULL`, and a count of nothing is `0`. This is what the select-list bind seam is for: a bucket like
    /// "overdue" is a condition with a value in it (today), and it goes into the aggregate the same way it
    /// would go into a `WHERE` — carrying its own bind.
    pub fn count_if(&mut self, p: Pred) -> Slot<i64> {
        self.push(format!("COALESCE(SUM({}), 0)", p.sql()), p.params().to_vec())
    }

    /// A correlated `(SELECT COUNT(*) FROM …)` as a select item ([`Count`]) — how many rows another table
    /// holds for this one. Its tables and columns are the registry's.
    pub fn count_of(&mut self, c: Count) -> Slot<i64> {
        let sub = c.sql();
        self.push(sub.text().to_owned(), sub.params().to_vec())
    }

    /// A **predicate** as a select item, read back as a `bool` — SQLite yields a condition as `0`/`1`, and
    /// a reader that wants the answer per row rather than as a filter (the task card reads "is this premise
    /// unsettled" and splits its links in one pass) asks for it here. The point of the seam is the binds:
    /// written as text through [`Select::expr`], a predicate that carries values would leave them behind —
    /// silently, until a placeholder past the slip bound its neighbour's value.
    pub fn pred(&mut self, p: Pred) -> Slot<bool> {
        self.push(p.sql().to_owned(), p.params().to_vec())
    }

    /// The list as bare text (`"t.id, t.title, …"`) — for a projection whose items bind **nothing**, which
    /// is most of them. A list that carries values has to go in through [`Sql::select`] /
    /// [`Sql::push_select`] instead, or its binds would be left behind: debug builds assert that here, the
    /// way [`Sql::push`] asserts that text spliced in carries no placeholder.
    pub fn list(&self) -> String {
        debug_assert!(
            self.params.is_empty(),
            "a select list that binds values cannot be spliced in as text — use Sql::select / push_select"
        );
        self.items.join(", ")
    }

    /// The list and the values its placeholders bind. Read out together, by [`Sql::select`] and
    /// [`Sql::push_select`].
    fn parts(&self) -> (String, &[Value]) {
        (self.items.join(", "), &self.params)
    }

    /// Whether the rows this list cannot tell apart are folded into one ([`Select::distinct`]).
    fn is_distinct(&self) -> bool {
        self.distinct
    }

    fn push<V>(&mut self, sql: String, params: Vec<Value>) -> Slot<V> {
        debug_assert_eq!(
            placeholder_count(&sql),
            params.len(),
            "select item and its bind values disagree: {sql}"
        );
        self.items.push(sql);
        self.params.extend(params);
        Slot { index: self.items.len() - 1, out: PhantomData }
    }
}

/// A `UNION ALL` whose arms are made to project the same row shape — the [`Select`] guarantee, carried
/// across a seam that would otherwise break it.
///
/// A union takes the row shape of its **first** arm: that arm's projection is what the rows are read
/// through, and every later arm has to line up with it column for column. Spelled out by hand, an arm
/// with two of its columns swapped (or one dropped) is a query SQLite is perfectly happy with, whose
/// rows are read through the first arm's [`Slot`]s onto the wrong values, in silence, whenever the types
/// happened to agree.
///
/// Here an arm is a closure handed a fresh `Select`, which returns the slots it appended and the tail of
/// its statement (` FROM … [JOIN …] [WHERE …]`, with whatever binds those carry). **Every arm returns the
/// same slot type `S`**, so the compiler is what checks the arms agree — same number of items, same types,
/// same positions. The rows are read through the first arm's slots, which are the only ones handed back.
///
/// ```
/// use amenbo_core::store_engine::{schema::col, sql::{Pred, Sql, Union, same}};
///
/// const D: col::task_dependency::Cols = col::task_dependency::of("d");
/// const T: col::task::Cols = col::task::of("t");
///
/// let (project, sql) = Union::all(|sel| {
///     let project = sel.col(T.project_id);
///     let mut tail = Sql::from_table(D.table);
///     tail.join(T.table, same(T.id, D.blocked_by_id))
///         .push_where(Some(&Pred::eq(D.task_id, 412i64)));
///     (project, tail)
/// })
/// .arm(|sel| {
///     let project = sel.col(T.project_id);
///     let mut tail = Sql::from_table(D.table);
///     tail.join(T.table, same(T.id, D.task_id))
///         .push_where(Some(&Pred::eq(D.blocked_by_id, 412i64)));
///     (project, tail)
/// })
/// .into_parts();
///
/// assert_eq!(
///     sql.text(),
///     "SELECT t.project_id FROM task_dependency d JOIN task t ON t.id = d.blocked_by_id \
///      WHERE d.task_id = ? \
///      UNION ALL SELECT t.project_id FROM task_dependency d JOIN task t ON t.id = d.task_id \
///      WHERE d.blocked_by_id = ?"
/// );
/// assert_eq!(sql.params().len(), 2, "each arm's binds sit where its placeholders do");
/// # let _ = project;
/// ```
///
/// An arm that projects a different shape does not compile — the row it would hand the reader is not the
/// one the slots read:
///
/// ```compile_fail
/// use amenbo_core::store_engine::{schema::col, sql::{Sql, Union}};
///
/// const T: col::task::Cols = col::task::of("t");
///
/// let _ = Union::all(|sel| (sel.col(T.project_id), Sql::new(" FROM task t")))
///     .arm(|sel| (sel.col(T.title), Sql::new(" FROM task t")));
/// ```
pub struct Union<S> {
    sql: Sql,
    slots: S,
}

impl<S> Union<S> {
    /// The first arm: its projection is the union's row shape, and its slots are the ones every row is
    /// read through. `UNION ALL` — the arms are added up, not deduped; nothing here asks for a `UNION`
    /// that folds duplicate rows together, and a seam nothing asks for is one more thing to keep true.
    pub fn all(arm: impl FnOnce(&mut Select) -> (S, Sql)) -> Self {
        let mut sel = Select::new();
        let (slots, tail) = arm(&mut sel);
        let mut sql = Sql::select(&sel);
        sql.push_sql(&tail);
        Self { sql, slots }
    }

    /// Another arm, projecting the same shape (the compiler says so: it returns the same `S`). Its own
    /// slots are dropped — a union has one row shape, and the first arm already named it.
    pub fn arm(mut self, arm: impl FnOnce(&mut Select) -> (S, Sql)) -> Self {
        let mut sel = Select::new();
        let (_slots, tail) = arm(&mut sel);
        self.sql.push(" UNION ALL SELECT ").push_select(&sel).push_sql(&tail);
        self
    }

    /// The slots the rows are read through, and the statement that produces them.
    pub fn into_parts(self) -> (S, Sql) {
        (self.slots, self.sql)
    }
}

/// A position in a [`Select`]'s list, with the type the value there reads back as. The only way to make
/// one is to append the item it names, so there is no index to keep in step with anything.
pub struct Slot<V> {
    index: usize,
    out: PhantomData<V>,
}

impl<V> Clone for Slot<V> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<V> Copy for Slot<V> {}

impl<V> Slot<V> {
    /// This item's **1-based position** in the select list — the only name a compound query's `ORDER BY`
    /// can call it by. The arms of a [`Union`] each spell their own expressions, so there is no column
    /// the order could name; SQLite orders such a query by position instead, and taking the position
    /// from the slot keeps the order tied to the projection rather than to a number written out beside
    /// it (which a column inserted above would silently move).
    pub fn ordinal(&self) -> usize {
        self.index + 1
    }
}

impl<V: rusqlite::types::FromSql> Slot<V> {
    /// Read this item out of a row of the query its [`Select`] built.
    pub fn get(&self, row: &rusqlite::Row<'_>) -> rusqlite::Result<V> {
        row.get(self.index)
    }
}

/// Count the `?` placeholders in a fragment, ignoring the ones inside a string literal (`'…'`, where
/// `''` is an escaped quote) — a `LIKE ... ESCAPE '\'` or a status literal must not be read as a bind.
fn placeholder_count(sql: &str) -> usize {
    let mut count = 0;
    let mut in_literal = false;
    for c in sql.chars() {
        match c {
            // A doubled quote inside a literal flips out and straight back in, which lands on the
            // right state either way.
            '\'' => in_literal = !in_literal,
            '?' if !in_literal => count += 1,
            _ => {}
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store_engine::schema::col;

    /// The task's columns as the read layer names them (`FROM task t`).
    const T: col::task::Cols = col::task::of("t");

    fn ints(p: &[Value]) -> Vec<i64> {
        p.iter()
            .map(|v| match v {
                Value::Integer(i) => *i,
                other => panic!("not an integer: {other:?}"),
            })
            .collect()
    }

    #[test]
    fn a_predicate_carries_its_own_values() {
        let p = Pred::eq(T.project_id, 7i64);
        assert_eq!(p.sql(), "t.project_id = ?");
        assert_eq!(ints(p.params()), vec![7]);
    }

    #[test]
    fn composing_moves_the_values_with_the_fragment() {
        let p = Pred::eq(T.id, 1i64).and(Pred::eq(T.project_id, 2i64)).or(Pred::eq(T.id, 3i64));
        assert_eq!(p.sql(), "((t.id = ? AND t.project_id = ?) OR t.id = ?)");
        assert_eq!(ints(p.params()), vec![1, 2, 3]);
    }

    #[test]
    fn negation_keeps_the_values() {
        let p = !Pred::eq(T.id, 1i64).and(Pred::eq(T.project_id, 2i64));
        assert_eq!(p.sql(), "NOT ((t.id = ? AND t.project_id = ?))");
        assert_eq!(ints(p.params()), vec![1, 2]);
    }

    #[test]
    fn an_in_list_binds_one_value_per_placeholder() {
        let p = Pred::is_in(T.id, [1i64, 2, 3]);
        assert_eq!(p.sql(), "t.id IN (?, ?, ?)");
        assert_eq!(ints(p.params()), vec![1, 2, 3]);
    }

    #[test]
    fn an_empty_in_list_matches_nothing() {
        let p = Pred::is_in(T.id, Vec::<i64>::new());
        assert_eq!(p.sql(), "1 = 0");
        assert!(p.params().is_empty());
    }

    #[test]
    fn no_predicates_is_no_where_clause() {
        assert!(Pred::all(Vec::new()).is_none());
        let mut sql = Sql::new("SELECT 1 FROM task t");
        sql.push_where(None);
        assert_eq!(sql.text(), "SELECT 1 FROM task t");
        assert!(sql.params().is_empty());
    }

    #[test]
    fn a_statement_binds_what_its_placeholders_ask_for() {
        let pred = Pred::eq(T.status, "todo").and(Pred::is_in(T.id, [4i64, 5]));
        let mut sql = Sql::new("SELECT t.id FROM task t");
        sql.push_where(Some(&pred));
        sql.push(" LIMIT ").bind(10i64).push(" OFFSET ").bind(0i64);

        assert_eq!(
            sql.text(),
            "SELECT t.id FROM task t WHERE (t.status = ? AND t.id IN (?, ?)) LIMIT ? OFFSET ?"
        );
        assert_eq!(sql.params().len(), 5);
        assert_eq!(placeholder_count(sql.text()), sql.params().len());
    }

    /// A correlated subquery is built out of the registry's tables and columns, and the binds its joins
    /// and its filters carry land in the order their fragments do.
    #[test]
    fn a_correlated_subquery_names_its_tables_and_carries_their_binds() {
        const TC: col::task_comment::Cols = col::task_comment::of("tc");
        const D: col::task_dependency::Cols = col::task_dependency::of("d");

        let p = Exists::over(TC.table)
            .join(D.table, Pred::eq(D.blocked_by_id, 9i64))
            .filter(Pred::plain(format!("{} = {}", TC.task_id.to_sql(), T.id.to_sql())))
            .filter(Pred::eq(TC.text, "x"))
            .pred();

        assert_eq!(
            p.sql(),
            "EXISTS (SELECT 1 FROM task_comment tc JOIN task_dependency d ON d.blocked_by_id = ? \
             WHERE (tc.task_id = t.id AND tc.text = ?))"
        );
        assert_eq!(p.params().len(), 2, "the join's bind comes before the filter's");
        assert!(matches!(p.params()[0], Value::Integer(9)));
    }

    /// The table a `FROM` names is the one whose columns the query asked for — one call hands out both,
    /// so an alias cannot be spelled two ways.
    #[test]
    fn a_table_is_written_with_the_alias_its_columns_were_asked_for() {
        assert_eq!(col::task::of("t").table.to_sql(), "task t");
        assert_eq!(col::task::ALL.table.to_sql(), "task", "an unaliased table is not doubled");
    }

    #[test]
    fn a_literal_quote_is_not_a_placeholder() {
        assert_eq!(placeholder_count("LOWER(t.title) LIKE ? ESCAPE '\\'"), 1);
        assert_eq!(placeholder_count("t.status = 'done'"), 0);
    }

    /// A column is spelled by the qualifier the query gave it, not by its table's name — the same
    /// registry column serves `FROM task t` and an unaliased statement.
    #[test]
    fn a_column_is_spelled_with_the_qualifier_it_was_asked_for() {
        assert_eq!(Pred::eq(T.title, "x").sql(), "t.title = ?");
        assert_eq!(Pred::eq(col::task::ALL.title, "x").sql(), "task.title = ?");
        assert_eq!(Pred::like(T.notes.lower(), "%x%").sql(), "LOWER(t.notes) LIKE ? ESCAPE '\\'");
    }

    /// The text-only shapes are text-only *in the type*: the sentinel `''` a field-by-field create
    /// leaves behind is a text notion, and an integer column has no such value to read as absent.
    #[test]
    fn the_not_written_sentinel_is_a_text_reading() {
        assert_eq!(Pred::is_blank(T.due_on).sql(), "(t.due_on IS NULL OR t.due_on = '')");
        assert_eq!(Pred::is_null(T.assignee_kind).sql(), "t.assignee_kind IS NULL");
    }

    /// The `SELECT` list and the reads that take its row apart come from the same act: a slot *is* the
    /// position it appended, so the two cannot fall out of step.
    #[test]
    fn a_slot_reads_the_item_it_appended_wherever_it_lands() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(&crate::store_engine::schema::schema_sql()).unwrap();
        conn.execute(
            "INSERT INTO task (id, title, status, notes, due_on) VALUES (1, 'first', 'todo', 'n', NULL)",
            [],
        )
        .unwrap();

        // The same three columns, in two projections that order them differently — and, in the second,
        // with an unrelated expression wedged in front of them.
        let mut a = Select::new();
        let (a_id, a_title, a_status) = (a.col(T.id), a.col(T.title), a.col(T.status));
        let mut b = Select::new();
        let _count = b.expr::<i64>("COUNT(*) OVER ()");
        let (b_status, b_title, b_id) = (b.col(T.status), b.col(T.title), b.col(T.id));

        let mut sql_a = Sql::select(&a);
        sql_a.push(" FROM task t WHERE t.id = 1");
        let mut sql_b = Sql::select(&b);
        sql_b.push(" FROM task t WHERE t.id = 1");

        let from_a = conn
            .query_row(sql_a.text(), [], |r| Ok((a_id.get(r)?, a_title.get(r)?, a_status.get(r)?)))
            .unwrap();
        let from_b = conn
            .query_row(sql_b.text(), [], |r| Ok((b_id.get(r)?, b_title.get(r)?, b_status.get(r)?)))
            .unwrap();

        assert_eq!(from_a, (1, "first".to_string(), "todo".to_string()));
        assert_eq!(from_b, from_a, "the order of the list is not the reader's business");
    }

    /// A select item may be a condition with a value in it — a bucket counted inside an aggregate. Its
    /// bind goes in through the same seam its text does, *before* whatever the `WHERE` carries, which is
    /// the order the placeholders sit in.
    #[test]
    fn a_select_item_carries_its_own_bind_and_the_where_follows_it() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(&crate::store_engine::schema::schema_sql()).unwrap();
        conn.execute_batch(
            "INSERT INTO project (id, name) VALUES (1, 'Alpha'), (2, 'Beta');
             INSERT INTO task (id, title, status, due_on, project_id) VALUES
               (1, 'a', 'todo', '2026-07-14', 1),
               (2, 'b', 'todo', '2026-07-01', 1),
               (3, 'c', 'todo', '2026-07-14', 2)",
        )
        .unwrap();

        let mut sel = Select::new();
        let total = sel.count_all();
        let overdue = sel.count_if(Pred::cmp(T.due_on, "<", "2026-07-14"));
        let due_today = sel.count_if(Pred::eq(T.due_on, "2026-07-14"));

        let mut sql = Sql::select(&sel);
        sql.push(" FROM task t").push_where(Some(&Pred::eq(T.project_id, 1i64)));
        assert_eq!(placeholder_count(sql.text()), sql.params().len());

        let counts = conn
            .query_row(sql.text(), rusqlite::params_from_iter(sql.params()), |r| {
                Ok((total.get(r)?, overdue.get(r)?, due_today.get(r)?))
            })
            .unwrap();
        assert_eq!(counts, (2, 1, 1), "the buckets bind their own day, and the WHERE its project");
    }

    /// A union's arms project one row shape, and each arm's binds sit where its own placeholders do —
    /// so the rows of every arm are read through the first arm's slots and land on the value that arm
    /// selected.
    #[test]
    fn every_arm_of_a_union_is_read_through_one_shape() {
        const D: col::task_dependency::Cols = col::task_dependency::of("d");

        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(&crate::store_engine::schema::schema_sql()).unwrap();
        conn.execute_batch(
            "INSERT INTO project (id, name) VALUES (1, 'Alpha'), (2, 'Beta');
             INSERT INTO task (id, title, status, project_id) VALUES
               (1, 'a', 'todo', 1), (2, 'b', 'todo', 2), (3, 'c', 'todo', NULL);
             INSERT INTO task_dependency (id, task_id, blocked_by_id) VALUES (1, 1, 2), (2, 3, 1)",
        )
        .unwrap();

        // Task 1's peers: what it is blocked by (task 2, in Beta), and what it blocks (task 3, in the
        // inbox).
        let (project, sql) = Union::all(|sel| {
            let project = sel.col(T.project_id);
            let mut tail = Sql::from_table(D.table);
            tail.join(T.table, same(T.id, D.blocked_by_id))
                .push_where(Some(&Pred::eq(D.task_id, 1i64)));
            (project, tail)
        })
        .arm(|sel| {
            let project = sel.col(T.project_id);
            let mut tail = Sql::from_table(D.table);
            tail.join(T.table, same(T.id, D.task_id))
                .push_where(Some(&Pred::eq(D.blocked_by_id, 1i64)));
            (project, tail)
        })
        .into_parts();

        assert_eq!(placeholder_count(sql.text()), sql.params().len());
        let mut stmt = conn.prepare(sql.text()).unwrap();
        let peers: Vec<Option<i64>> = stmt
            .query_map(rusqlite::params_from_iter(sql.params()), |r| project.get(r))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(peers, vec![Some(2), None], "each arm's own row, read through one slot");
    }

    /// A join names its tables through the registry and states its condition in typed columns, and an
    /// **outer** join is where the registry stops knowing: `project.name` is `NOT NULL`, and a task with
    /// no project reaches no row at all, so the column comes back absent. The projection says so by
    /// widening the registry's own column ([`Col::nullable`]) rather than restating its type in text.
    #[test]
    fn an_outer_join_leaves_a_not_null_column_absent_and_the_projection_says_so() {
        const P: col::project::Cols = col::project::of("p");

        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(&crate::store_engine::schema::schema_sql()).unwrap();
        conn.execute_batch(
            "INSERT INTO project (id, name) VALUES (1, 'Alpha');
             INSERT INTO task (id, title, status, project_id) VALUES
               (1, 'placed', 'todo', 1), (2, 'inbox', 'todo', NULL)",
        )
        .unwrap();

        let mut sel = Select::new();
        let (id, project) = (sel.col(T.id), sel.col(P.name.nullable()));
        let mut sql = Sql::from(&sel, T.table);
        sql.left_join(P.table, same(P.id, T.project_id)).push(format!(" ORDER BY {}", T.id.to_sql()));
        assert_eq!(
            sql.text(),
            "SELECT t.id, p.name FROM task t LEFT JOIN project p ON p.id = t.project_id ORDER BY t.id"
        );

        let mut stmt = conn.prepare(sql.text()).unwrap();
        let rows: Vec<(i64, Option<String>)> = stmt
            .query_map(rusqlite::params_from_iter(sql.params()), |r| Ok((id.get(r)?, project.get(r)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(
            rows,
            vec![(1, Some("Alpha".to_string())), (2, None)],
            "the inbox task keeps its row, and the name the join could not reach is absent"
        );
    }

    /// A correlated count is a select item that carries its own subquery's tables and binds ([`Count`]) —
    /// the shape `project list` folds its per-project counts in by.
    #[test]
    fn a_correlated_count_names_its_tables_and_counts_per_row() {
        const P: col::project::Cols = col::project::of("p");

        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(&crate::store_engine::schema::schema_sql()).unwrap();
        conn.execute_batch(
            "INSERT INTO project (id, name) VALUES (1, 'Alpha'), (2, 'Beta');
             INSERT INTO task (id, title, status, project_id) VALUES
               (1, 'a', 'todo', 1), (2, 'b', 'done', 1), (3, 'c', 'todo', NULL)",
        )
        .unwrap();

        let mut sel = Select::new();
        let (id, open) = (
            sel.col(P.id),
            sel.count_of(
                Count::over(T.table).filter(same(T.project_id, P.id)).filter(Pred::ne(T.status, "done")),
            ),
        );
        let mut sql = Sql::from(&sel, P.table);
        sql.push(format!(" ORDER BY {}", P.id.to_sql()));
        assert_eq!(placeholder_count(sql.text()), sql.params().len());

        let mut stmt = conn.prepare(sql.text()).unwrap();
        let counts: Vec<(i64, i64)> = stmt
            .query_map(rusqlite::params_from_iter(sql.params()), |r| Ok((id.get(r)?, open.get(r)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(
            counts,
            vec![(1, 1), (2, 0)],
            "each project counts its own open tasks, and the inbox is nobody's"
        );
    }

    /// A list that binds values cannot be taken as bare text and spliced into a statement — the binds
    /// would be left behind, and the query would answer a question no one asked.
    #[test]
    #[should_panic(expected = "cannot be spliced in as text")]
    fn a_list_that_binds_values_refuses_to_be_a_string() {
        let mut sel = Select::new();
        let _ = sel.count_if(Pred::eq(T.status, "done"));
        let _ = sel.list();
    }

    /// A write names its columns and carries their values as one act, so the `SET` list and the `WHERE`
    /// cannot bind each other's values — the write-side reading of the failure this layer exists to make
    /// unwritable. The values come out in placeholder order: what is written first, what picks the rows
    /// second.
    #[test]
    fn a_write_binds_its_values_where_its_placeholders_sit() {
        let stmt = Update::table(col::task::ALL.table)
            .set_value("title", Value::Text("renamed".into()))
            .set_value("status", Value::Text("done".into()))
            .filter(Pred::eq(col::task::ALL.id, 412i64))
            .sql();
        assert_eq!(
            stmt.text(),
            "UPDATE task SET \"title\" = ?, \"status\" = ? WHERE task.id = ?"
        );
        assert_eq!(placeholder_count(stmt.text()), stmt.params().len());
        assert_eq!(stmt.params()[2], Value::Integer(412));

        let del = Delete::from(col::inbox_archive::ALL.table)
            .filter(Pred::is_in(col::inbox_archive::ALL.task_id, [1i64, 2]))
            .sql();
        assert_eq!(
            del.text(),
            "DELETE FROM inbox_archive WHERE inbox_archive.task_id IN (?, ?)"
        );
        assert_eq!(ints(del.params()), vec![1, 2]);

        // No predicate is no `WHERE` — a table-wide delete is something a caller has to mean.
        assert_eq!(Delete::from(col::binding_path::ALL.table).sql().text(), "DELETE FROM binding_path");
    }

    /// An upsert writes the columns it brought and, on a conflicting key, writes them again — every
    /// column except the key, which is the row it conflicted on.
    #[test]
    fn an_upsert_rewrites_every_column_but_the_key() {
        let meta = col::store_meta::ALL;
        let stmt = Insert::into(meta.table)
            .set(meta.key, "schema_version")
            .set_opt(meta.value, None::<&str>)
            .on_conflict_update(meta.key)
            .sql();
        assert_eq!(
            stmt.text(),
            "INSERT INTO store_meta (\"key\", \"value\") VALUES (?, ?) \
             ON CONFLICT(\"key\") DO UPDATE SET \"value\" = excluded.\"value\""
        );
        assert_eq!(stmt.params()[1], Value::Null, "an unset scalar binds NULL, not the empty string");
    }

    /// The store's writes go through the same connection its reads do, so a statement the layer built
    /// can be executed and read back — the round trip the builders are for.
    #[test]
    fn a_built_write_lands_in_the_store() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(&crate::store_engine::schema::schema_sql()).unwrap();
        let ia = col::inbox_archive::ALL;

        for _ in 0..2 {
            Insert::into(ia.table)
                .set(ia.task_id, 412i64)
                .on_conflict_do_nothing(ia.task_id)
                .sql()
                .execute(&conn)
                .expect("dismissing a task twice is a no-op, not a constraint failure");
        }
        let n: i64 =
            conn.query_row("SELECT count(*) FROM inbox_archive", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);

        let gone = Delete::from(ia.table)
            .filter(Pred::eq(ia.task_id, 412i64))
            .sql()
            .execute(&conn)
            .unwrap();
        assert_eq!(gone, 1, "the value the predicate carried is the row that went");
    }

    /// The **plain** tables carry their nullability too, so the store's scalars behave like every other
    /// column: an unset one is written as `NULL` through a seam only a nullable column has, and it reads
    /// back as `None` rather than as the `InvalidColumnType` a bare `String` would meet on that row.
    #[test]
    fn a_plain_table_says_which_of_its_columns_admit_null() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(&crate::store_engine::schema::schema_sql()).unwrap();
        let meta = col::store_meta::ALL;

        Insert::into(meta.table)
            .set(meta.key, "format_version")
            .set_opt(meta.value, None::<&str>)
            .sql()
            .execute(&conn)
            .expect("an unset scalar is a NULL the column admits");

        let mut sel = Select::new();
        let value = sel.col(meta.value);
        let read: Option<String> = conn
            .query_row(
                &format!("SELECT {} FROM store_meta WHERE key = 'format_version'", sel.list()),
                [],
                |r| value.get(r),
            )
            .unwrap();
        assert_eq!(read, None, "the scalar round-trips as absent, and its type says it can be");
    }

    /// A column's nullability comes from the registry, and the value's Rust shape with it: a `NOT NULL`
    /// column is its bare type, a nullable one an `Option` — so a row that holds no value is a `None`,
    /// not the `InvalidColumnType` a hand-picked `r.get::<String>` would meet on that row alone.
    #[test]
    fn a_nullable_column_reads_as_an_option_and_a_not_null_one_does_not() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(&crate::store_engine::schema::schema_sql()).unwrap();
        conn.execute("INSERT INTO task (id, title, status) VALUES (7, 'x', 'todo')", []).unwrap();

        let mut sel = Select::new();
        let (title, due_on, project_id) = (sel.col(T.title), sel.col(T.due_on), sel.col(T.project_id));

        let mut sql = Sql::select(&sel);
        sql.push(" FROM task t WHERE t.id = 7");
        let (title, due_on, project_id) = conn
            .query_row(sql.text(), [], |r| Ok((title.get(r)?, due_on.get(r)?, project_id.get(r)?)))
            .unwrap();

        assert_eq!(title, "x", "TEXT NOT NULL reads as a String");
        assert_eq!(due_on, None::<String>, "a nullable day reads as None, not an error");
        assert_eq!(project_id, None::<i64>, "and so does an unset reference");
    }
}
