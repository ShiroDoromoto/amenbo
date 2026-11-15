//! **Bringing the database in the user's own account up to what this build carries** (`AMB-D-886`).
//!
//! The database is not laid out once and left. Migrations are added as the Worker learns to hold more,
//! and every one of them has to reach a database stood up before it existed — in an account nobody here
//! can log in to, on a day nobody chooses.
//!
//! **So which migrations a database has had is a thing the database itself says.** Asking whether the
//! `records` table is there and leaving it alone if it is reads as "already set up" for a database laid
//! out by any older release, so a migration added afterwards would never be applied — and what breaks is
//! the user's next send, in an account with no way to see it from here.

use super::cloudflare::{Queried, Reached, Sky};

/// Where a database records which migrations it has had. Both the name and the shape are wrangler's
/// rather than ours: a user who later reaches for `wrangler d1 migrations` against their own database
/// finds the ledger it expects, and the Worker's own tests — which apply the same files through
/// `applyD1Migrations` — write into the same table under the same names. A second ledger of our own would
/// let the two disagree without either being wrong.
const LEDGER_TABLE: &str = "d1_migrations";

const CREATE_THE_LEDGER: &str = "CREATE TABLE IF NOT EXISTS d1_migrations (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  name       TEXT UNIQUE,
  applied_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP NOT NULL
);";

/// The two questions that place a database that was already there: has it been laid out at all, and does
/// it say what it has had.
const TABLES_THAT_SAY_WHERE_A_DATABASE_STANDS: &str =
    "SELECT name FROM sqlite_master WHERE type = 'table' AND name IN ('records', 'd1_migrations')";

const READ_THE_LEDGER: &str = "SELECT name FROM d1_migrations";

/// What a database with tables but no ledger has already had.
///
/// **A frozen list, not the migrations this build carries.** Releases before the ledger laid the whole
/// schema down in one go and recorded nothing, so the only honest thing to say about such a database is
/// which migrations existed back then — and that is these two, which is all the first release shipped.
/// Reading it off the current build instead would mark every future migration as applied on exactly the
/// databases that have not had it.
const LAID_DOWN_BEFORE_THE_LEDGER: &[&str] =
    &["0001_records_and_tokens.sql", "0002_where_the_order_stands.sql"];

/// The migrations this build carries, in the order they are applied — the Worker's own `migrations/`,
/// copied under the same names by `make -C worker baked`.
///
/// **The names are the list.** They are what a database records having had, so they are spelled here one
/// by one rather than read off a directory at build time: a file this list does not name is one no
/// database is ever given, and `tests::the_baked_migrations_are_the_ones_named_here` is what makes that
/// come due.
///
/// **Generated. Edit `worker/migrations/`, never the copies.**
pub(crate) const MIGRATIONS: &[(&str, &str)] = &[
    ("0001_records_and_tokens.sql", include_str!("migrations/0001_records_and_tokens.sql")),
    ("0002_where_the_order_stands.sql", include_str!("migrations/0002_where_the_order_stands.sql")),
    ("0003_placing_in_parts.sql", include_str!("migrations/0003_placing_in_parts.sql")),
    (
        "0004_where_this_placement_began.sql",
        include_str!("migrations/0004_where_this_placement_began.sql"),
    ),
    (
        "0005_the_key_the_records_were_sealed_with.sql",
        include_str!("migrations/0005_the_key_the_records_were_sealed_with.sql"),
    ),
    (
        "0006_one_code_and_not_one_per_phone.sql",
        include_str!("migrations/0006_one_code_and_not_one_per_phone.sql"),
    ),
];

/// What a database that was already there says about itself.
#[derive(Default)]
struct Standing {
    /// It has the tables, so something has been applied to it.
    laid_out: bool,
    /// It says what it has had, so nothing has to be assumed.
    keeps_a_ledger: bool,
    /// The migrations it has already been given.
    had: Vec<String>,
}

impl Standing {
    fn has(&self, name: &str) -> bool {
        self.had.iter().any(|already| already == name)
    }
}

/// What one run of [`lay_the_schema_down`] did, for the caller that says so out loud.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SchemaRun {
    /// The database was up to date and nothing was run.
    NothingToApply,
    /// This many migrations were applied, the last of them named.
    Applied { count: usize, up_to: String },
    /// The database had every migration already, and now says so.
    LedgerWrittenDown,
}

/// Bring the database up to what this build carries, applying nothing it has already had.
///
/// The whole run goes over in one call, migrations and ledger lines together: D1 takes the statements in
/// order and stops at the first that fails, so what is recorded as applied is what applied.
pub(crate) fn lay_the_schema_down(
    sky: &Sky,
    account: &str,
    database: &str,
    fresh: bool,
) -> Reached<SchemaRun> {
    let standing = where_the_database_stands(sky, account, database, fresh)?;

    let missing: Vec<&(&str, &str)> =
        MIGRATIONS.iter().filter(|(name, _)| !standing.has(name)).collect();
    if standing.keeps_a_ledger && missing.is_empty() {
        return Ok(SchemaRun::NothingToApply);
    }

    let mut run = String::from(CREATE_THE_LEDGER);
    run.push('\n');
    // A database laid out before the ledger existed is given one that says what it has, rather than one
    // that says it has nothing — which would re-apply the very tables it is standing on.
    if !standing.keeps_a_ledger {
        for name in &standing.had {
            run.push_str(&record_that(name));
            run.push('\n');
        }
    }
    for (name, sql) in &missing {
        run.push_str(sql);
        if !sql.ends_with('\n') {
            run.push('\n');
        }
        run.push_str(&record_that(name));
        run.push('\n');
    }

    sky.query(account, database, &run)?;

    Ok(match missing.last() {
        Some((name, _)) => SchemaRun::Applied { count: missing.len(), up_to: (*name).to_string() },
        None => SchemaRun::LedgerWrittenDown,
    })
}

/// Work out what has already been applied.
///
/// A database made a moment ago has had nothing, and there is nothing to ask it. One that was already
/// there is asked, in this order: its ledger if it keeps one, and the frozen list if it has tables but no
/// ledger. A database with neither is one an interrupted run left empty, and gets everything.
fn where_the_database_stands(
    sky: &Sky,
    account: &str,
    database: &str,
    fresh: bool,
) -> Reached<Standing> {
    if fresh {
        return Ok(Standing::default());
    }

    let mut standing = Standing::default();
    for name in names(sky.query(account, database, TABLES_THAT_SAY_WHERE_A_DATABASE_STANDS)?) {
        match name.as_str() {
            "records" => standing.laid_out = true,
            LEDGER_TABLE => standing.keeps_a_ledger = true,
            _ => {}
        }
    }

    if standing.keeps_a_ledger {
        standing.had = names(sky.query(account, database, READ_THE_LEDGER)?);
    } else if standing.laid_out {
        standing.had = LAID_DOWN_BEFORE_THE_LEDGER.iter().map(|n| (*n).to_string()).collect();
    }
    Ok(standing)
}

/// The `name` column off every row D1 handed back.
fn names(outcomes: Vec<Queried>) -> Vec<String> {
    outcomes
        .into_iter()
        .flat_map(|outcome| outcome.results)
        .filter_map(|row| row.get("name").and_then(|v| v.as_str()).map(str::to_string))
        .filter(|name| !name.is_empty())
        .collect()
}

/// Put a string into SQL as a literal. The names are this repository's own filenames, so there is nothing
/// here to escape — which is exactly why doing it anyway costs nothing.
fn quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

/// The line that marks one migration as had. It goes in the same call as the migration it records, so a
/// run that fails part way through leaves the ledger saying what actually landed rather than what was
/// meant to.
fn record_that(name: &str) -> String {
    format!("INSERT INTO {LEDGER_TABLE} (name) VALUES ({});", quoted(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The list above is what the baked directory holds — no more, no less, in the same order. A file the
    /// list does not name is one no database is ever given; a name the directory does not hold is a build
    /// that would not compile, which is the half `include_str!` already answers.
    #[test]
    fn the_baked_migrations_are_the_ones_named_here() {
        let at = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/viewer/migrations");
        let mut on_disk: Vec<String> = std::fs::read_dir(&at)
            .expect("the baked migrations are beside the script")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name.ends_with(".sql"))
            .collect();
        on_disk.sort();

        let named: Vec<String> = MIGRATIONS.iter().map(|(name, _)| (*name).to_string()).collect();
        assert_eq!(named, on_disk, "every baked migration is named here, in the order it applies");
    }

    /// The order is the number each name starts with, which is how the Worker's own tests sort the same
    /// files. The padding makes it alphabetical too, and that is the convention the number is for.
    #[test]
    fn the_migrations_are_in_the_order_they_apply() {
        let mut sorted = MIGRATIONS.to_vec();
        sorted.sort_by_key(|(name, _)| name.to_string());
        assert_eq!(sorted, MIGRATIONS.to_vec());
        assert!(MIGRATIONS.len() >= 2, "the frozen list below names two of them");
    }

    /// The frozen list names migrations this build still carries. It is what a database laid out before
    /// the ledger is credited with, so a name that has left the tree would credit it with nothing.
    #[test]
    fn what_a_pre_ledger_database_is_credited_with_is_still_carried() {
        for name in LAID_DOWN_BEFORE_THE_LEDGER {
            assert!(
                MIGRATIONS.iter().any(|(carried, _)| carried == name),
                "{name} is credited to an older database and is no longer carried"
            );
        }
    }

    /// A ledger line records the name a database keeps, quoted.
    #[test]
    fn a_ledger_line_records_the_name_under_which_it_applied() {
        assert_eq!(
            record_that("0001_records_and_tokens.sql"),
            "INSERT INTO d1_migrations (name) VALUES ('0001_records_and_tokens.sql');"
        );
        assert_eq!(quoted("it's"), "'it''s'");
    }
}
