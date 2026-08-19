//! The `store` domain: this device's amenbo rather than anything filed in it — what comes out
//! (`export`), what is set aside (`backup`), what goes back in (`restore`), whether it is sound,
//! and the settings it keeps.

use std::path::Path;

use amenbo_scenario::{Args, Domain};

use crate::{opt_bool, path_str, req_bool, req_i64, req_str, unmapped, Driver, Outcome};
use crate::judge::judge_field;

impl Driver<'_> {
    pub(crate) fn store_action(&mut self, op: &str, with: &Args, bind: Option<&str>) -> Result<Outcome, String> {
        match op {
            "export" => {
                let out = self.artifact(bind, "export", "");
                // `--out` takes the attachment files along with the records, which is the shape a
                // move to another tool is actually made in; it answers with prose, not JSON.
                self.run_bare(&["export", "--out", path_str(&out)?])?;
                self.remember(bind, "export", out.clone());
                Ok(Outcome::action(format!("exported the store to {}", out.display())))
            }
            "backup" => {
                let path = self.artifact(bind, "backup", ".amenbo-backup");
                let v = self.run_json(&["backup", path_str(&path)?, "--json"])?;
                let bytes = v["bytes"].as_i64().unwrap_or(0);
                self.remember(bind, "backup", path.clone());
                Ok(Outcome::action(format!("wrote a {bytes}-byte snapshot to {}", path.display())))
            }
            // The two faces something keeping a copy of this store elsewhere uses. Asking is the cheap half
            // and is meant to be done often, so what it binds is the number itself — the only shape a
            // later step can say it moved in. The window it answered for rides in the note, since two
            // carriers on one device hold numbers that are not each other's.
            "sync-version" => {
                let v = self.run_json(&["sync", "version", "--json"])?;
                let version = v["version"]
                    .as_i64()
                    .ok_or("`sync version` did not report a version")?;
                let window = match v["project_id"].as_i64() {
                    Some(id) => format!("project {id}"),
                    None => "the whole device".to_string(),
                };
                self.remember_number(bind, "sync-version", version);
                Ok(Outcome::action(format!("{window} is at version {version}")))
            }
            // The sending half. The document *is* stdout — that is the shape a carrier pipes — so it is
            // caught into a file of the run's own, and bound the way `export` binds what it wrote.
            "sync-snapshot" => {
                let out = self.artifact(bind, "sync-snapshot", ".json");
                let document = self.run_stdout(&["sync", "snapshot", "--json"])?;
                if let Some(dir) = out.parent() {
                    std::fs::create_dir_all(dir)
                        .map_err(|e| format!("could not make {}: {e}", dir.display()))?;
                }
                std::fs::write(&out, &document)
                    .map_err(|e| format!("could not write {}: {e}", out.display()))?;
                self.remember(bind, "sync-snapshot", out.clone());
                Ok(Outcome::action(format!(
                    "took a {}-byte snapshot of this window into {}",
                    document.len(),
                    out.display()
                )))
            }
            "restore" => {
                let path = self.artifact_ref(with, "target")?;
                // A destructive replace asks first; the driver is unattended, so it answers up front.
                let v = self.run_json(&["restore", path_str(&path)?, "--yes", "--json"])?;
                let saved = v["previous_saved_to"].as_str().unwrap_or("(not reported)");
                Ok(Outcome::action(format!(
                    "restored the store from {} (what it replaced was set aside at {saved})",
                    path.display()
                )))
            }
            // The repairing face. `--yes` because the driver is unattended, and the verdict it comes
            // back with is judged the way the reading face's is — a store still unsound after a
            // repair is a value, not a driver failure.
            "doctor-fix" => {
                let v = self.run_check(&["doctor", "--fix", "--yes", "--json"])?;
                let left = v["issues"].as_array().map_or(0, Vec::len);
                Ok(Outcome::action(format!("swept the store ({left} issue(s) still standing after it)")))
            }
            // Age every blob the store is holding past `GC_MIN_AGE`, so the sweep will take the ones
            // nothing references. A blob a run just wrote is always too young — removing its
            // attachment spares it for the hour that protects an attach in flight — which leaves the
            // reclaim unprovable from a scenario until its bytes are made old. The file's mtime is
            // what the sweep reads, so that is what moves here; no row is touched.
            "age-blobs" => {
                let dir = self.session.home.join("blobs");
                let aged = age_files_in(&dir)?;
                Ok(Outcome::action(format!("aged {aged} blob file(s) past the collection boundary")))
            }
            // A device somebody has been coming back to — the world every usage nudge stands behind.
            // Neither half of it can be driven: the launch tally is raised by the app coming up, and
            // the days written on are days, so a run that tried to earn this would have to last a
            // week. Both are read straight off the store, so straight onto the store is where they
            // are written — the tallies into the two scalars that hold them, and the records already
            // filed spread back over that many separate days.
            //
            // It is the reach `age-blobs` makes, one file in rather than one out: state a road has to
            // arrive at and cannot reach by using amenbo. Nothing it leaves is a shape amenbo would
            // not have written itself — the scalars are the ones it tallies into, and a record whose
            // `updated_at` sits on an earlier day is a record somebody touched that day.
            "worn-in" => {
                let launches = req_i64(with, "launches")?;
                let days = req_i64(with, "days")?;
                if launches < 0 || days < 1 {
                    return Err("`worn-in` takes a launch tally of zero or more and at least one day"
                        .to_string());
                }
                let worn = wear_in(&self.session.home, launches, days)?;
                Ok(Outcome::action(worn))
            }
            "config-set" => {
                let key = req_str(with, "key")?;
                let value = req_str(with, "value")?;
                self.run_json(&["config", "set", key, value, "--json"])?;
                Ok(Outcome::action(format!("set `{key}` to `{value}`")))
            }
            _ => Err(unmapped(Domain::Store, op)),
        }
    }
    pub(crate) fn store_assert(&self, op: &str, with: &Args) -> Result<Outcome, String> {
        match op {
            "snapshot" => {
                let path = self.artifact_ref(with, "target")?;
                // A word that must not be in what amenbo handed out. Read as bytes and searched for
                // verbatim: a value stored in the clear is in the clear whatever the layout around
                // it is, and that is the whole question — no claim about what the archive *does*
                // carry is made here.
                if let Some(needle) = with.get("absent").and_then(|v| v.as_str()) {
                    if needle.is_empty() {
                        return Err("`absent` names nothing to look for".to_string());
                    }
                    let (carried, read) = carries_text(&path, needle)?;
                    return Ok(Outcome::assert(
                        !carried,
                        format!(
                            "{} ({read} bytes) {} `{needle}` ({})",
                            path.display(),
                            if carried { "carries" } else { "does not carry" },
                            if carried { "MISMATCH" } else { "as expected" }
                        ),
                    ));
                }
                // The other half of the same reading. A backup is the road home, so what it must carry
                // is as much the question as what an export must not — and a value missing there is one
                // a person has to type in again after every restore.
                if let Some(needle) = with.get("contains").and_then(|v| v.as_str()) {
                    if needle.is_empty() {
                        return Err("`contains` names nothing to look for".to_string());
                    }
                    let (carried, read) = carries_text(&path, needle)?;
                    return Ok(Outcome::assert(
                        carried,
                        format!(
                            "{} ({read} bytes) {} `{needle}` ({})",
                            path.display(),
                            if carried { "carries" } else { "does not carry" },
                            if carried { "as expected" } else { "MISMATCH" }
                        ),
                    ));
                }
                let present = opt_bool(with, "present").unwrap_or(true);
                // An archive is a file with bytes in it. Whether those bytes put a store back is
                // what `restore` answers — asking that here would only be guessing at the layout of
                // something this driver is meant to treat as a black box.
                let bytes = path.metadata().ok().filter(|m| m.is_file()).map(|m| m.len());
                let found = bytes.is_some_and(|n| n > 0);
                let pass = found == present;
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "{} {} (expected {}, {})",
                        path.display(),
                        match bytes {
                            Some(n) => format!("holds {n} bytes"),
                            None => "is not a file on disk".to_string(),
                        },
                        if present { "an archive" } else { "nothing" },
                        if pass { "as expected" } else { "MISMATCH" }
                    ),
                ))
            }
            // What the sweep leaves behind. The reclaim raises no issue and says nothing a machine
            // reads, so the count of files is the only place it is observable.
            "blobs" => {
                let expected = req_i64(with, "count")?;
                let actual = count_files_in(&self.session.home.join("blobs"))? as i64;
                Ok(Outcome::assert(
                    actual == expected,
                    format!(
                        "the store holds {actual} blob file(s) (expected {expected}, {})",
                        if actual == expected { "as expected" } else { "MISMATCH" }
                    ),
                ))
            }
            "doctor" => {
                let want = req_bool(with, "ok")?;
                let v = self.run_check(&["doctor", "--json"])?;
                // Naming a kind asks about the list rather than the verdict. Most of what doctor
                // raises is a warning, which leaves `ok` alone — so a problem appearing, and a repair
                // taking it away, are only visible from here.
                match with.get("issue").and_then(|v| v.as_str()) {
                    None => Ok(judge_check("doctor", want, &v)),
                    Some(kind) => {
                        let present = opt_bool(with, "present").unwrap_or(true);
                        let found = v["issues"]
                            .as_array()
                            .is_some_and(|rows| rows.iter().any(|i| i["kind"].as_str() == Some(kind)));
                        let pass = found == present && v["ok"].as_bool().unwrap_or(false) == want;
                        Ok(Outcome::assert(
                            pass,
                            format!(
                                "`doctor` {} `{kind}` issue (expected {}, {})",
                                if found { "raises a" } else { "raises no" },
                                if present { "raised" } else { "gone" },
                                if pass { "as expected" } else { "MISMATCH" }
                            ),
                        ))
                    }
                }
            }
            "validate" => {
                let want = req_bool(with, "ok")?;
                // With no target the whole store is checked, which is what a user typing it bare
                // gets; naming one narrows it to that object.
                let mut args: Vec<String> = vec!["validate".into()];
                if with.contains_key("target") {
                    args.push(self.resolve(with)?.to_string());
                }
                args.push("--json".into());
                let v = self.run_check(&args.iter().map(String::as_str).collect::<Vec<_>>())?;
                Ok(judge_check("validate", want, &v))
            }
            // Whether the number a carrier watches has moved since an earlier step read it. Asked as
            // moved-or-not and never as a value: the number means nothing outside the store that
            // issued it, and a road that named one would be pinning an implementation detail. It is
            // read the same way the carrier reads it, through the command rather than off the store.
            "version" => {
                let before = self.number_ref(with, "since")?;
                let moved = req_bool(with, "moved")?;
                let v = self.run_json(&["sync", "version", "--json"])?;
                let now = v["version"].as_i64().ok_or("`sync version` did not report a version")?;
                let did = now != before;
                Ok(Outcome::assert(
                    did == moved,
                    format!(
                        "the version went {before} → {now} ({}; expected it to have {}, {})",
                        if did { "moved" } else { "stood still" },
                        if moved { "moved" } else { "stood still" },
                        if did == moved { "as expected" } else { "MISMATCH" }
                    ),
                ))
            }
            "config" => {
                let v = self.run_json(&["config", "--json"])?;
                judge_field("this store's configuration", with, &v)
            }
            "identity" => {
                let v = self.run_json(&["whoami", "--json"])?;
                judge_field("this store's identity", with, &v)
            }
            "update" => {
                // `--print` is the face that opens nothing, which is the only one a scenario may
                // wear: a check is a read, and it must not launch a browser at whoever runs it.
                let v = self.run_json(&["update", "--print", "--json"])?;
                judge_field("the update check", with, &v)
            }
            _ => Err(unmapped(Domain::Store, op)),
        }
    }

    /// Is the object an earlier step bound in the document written by another? The document is read
    /// off disk as it stands, because that document is the whole promise of the capability: what the
    /// other side receives is this file, not what amenbo would say about it.
    ///
    /// Two capabilities hand one out and the question put to both is the same, so the reading is one:
    /// `exported` names an archive, whose records lie in an `export.json` beside the attachment files,
    /// and `synced` names a carrier's snapshot, which is that document on its own.
    pub(crate) fn judge_carried(&self, domain: Domain, op: &str, with: &Args) -> Result<Outcome, String> {
        let target = self.resolve(with)?;
        let written = self.artifact_ref(with, "from")?;
        let present = opt_bool(with, "present").unwrap_or(true);
        let file = if op == "synced" { written } else { written.join("export.json") };
        let text = std::fs::read_to_string(&file)
            .map_err(|e| format!("could not read the document at {}: {e}", file.display()))?;
        let v: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| format!("the document at {} is not JSON: {e}", file.display()))?;
        // The tables an object of this domain lands in. A comment is looked for on both timelines:
        // a bound comment id is whichever of the two the step that posted it made.
        let (noun, tables): (&str, &[&str]) = match domain {
            Domain::Task => ("task", &["task"]),
            Domain::Decision => ("decision", &["decision"]),
            Domain::Comment => ("comment", &["task_comment", "decision_comment"]),
            other => return Err(format!("`{op}` says nothing about domain `{other:?}`")),
        };
        let found = tables.iter().any(|t| {
            v["tables"][t]
                .as_array()
                .is_some_and(|rows| rows.iter().any(|r| r["id"].as_i64() == Some(target)))
        });
        let pass = found == present;
        Ok(Outcome::assert(
            pass,
            format!(
                "{noun} {target} {} the document at {} under {} (expected {}, {})",
                if found { "is in" } else { "is missing from" },
                file.display(),
                tables.join("/"),
                if present { "carried out" } else { "left behind" },
                if pass { "as expected" } else { "MISMATCH" }
            ),
        ))
    }
}

/// Judge an integrity read: the check reports a verdict of its own, and the step says which one it
/// expects. The issue count rides along in the note, since a red one is only useful with its list.
fn judge_check(tool: &str, want: bool, v: &serde_json::Value) -> Outcome {
    let ok = v["ok"].as_bool().unwrap_or(false);
    let issues = v["issues"].as_array().map(Vec::len).unwrap_or(0);
    Outcome::assert(
        ok == want,
        format!(
            "`{tool}` reports {} over {issues} issue(s) (expected {}, {})",
            if ok { "sound" } else { "problems" },
            if want { "sound" } else { "problems" },
            if ok == want { "as expected" } else { "MISMATCH" }
        ),
    )
}

/// Whether what amenbo wrote at `path` carries `needle` anywhere in its bytes, and how many bytes
/// were read to say so. What a `store` action hands out is one file for a backup and a whole folder
/// for an export, so both shapes are walked — a value that leaked into any one file in there leaked.
fn carries_text(path: &Path, needle: &str) -> Result<(bool, u64), String> {
    let meta = std::fs::metadata(path)
        .map_err(|e| format!("could not read {}: {e}", path.display()))?;
    if meta.is_file() {
        let bytes =
            std::fs::read(path).map_err(|e| format!("could not read {}: {e}", path.display()))?;
        let carried = bytes.windows(needle.len()).any(|w| w == needle.as_bytes());
        return Ok((carried, bytes.len() as u64));
    }
    let entries = std::fs::read_dir(path)
        .map_err(|e| format!("could not read {}: {e}", path.display()))?;
    let (mut carried, mut read) = (false, 0);
    for entry in entries {
        let entry = entry.map_err(|e| format!("could not read {}: {e}", path.display()))?;
        let (found, n) = carries_text(&entry.path(), needle)?;
        carried |= found;
        read += n;
    }
    Ok((carried, read))
}

/// Every blob file the store is holding — the flat hash-named files, and nothing else. `tmp/` is the
/// staging area an ingest writes through, not stored bytes, so it is a directory and skipped by the
/// same rule that skips any directory.
fn blob_files(dir: &Path) -> Result<Vec<std::path::PathBuf>, String> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        // A store that has never held an attachment has no directory at all, which is zero blobs and
        // not a failure to read.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("could not read {}: {e}", dir.display())),
    };
    let mut files = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("could not read {}: {e}", dir.display()))?;
        if entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            files.push(entry.path());
        }
    }
    Ok(files)
}

fn count_files_in(dir: &Path) -> Result<usize, String> {
    blob_files(dir).map(|f| f.len())
}

/// The store file inside a run's own home, and the two scalars this device tallies its use into. The
/// names are the build's, and they are the whole of what is written down here — a store is a plain
/// SQLite file, and no shipped path ever keys one.
pub(crate) const STORE_FILE: &str = "store.sqlite";
const LAUNCH_COUNT_KEY: &str = "usage.launch_count";
const FIRST_LAUNCH_DAY_KEY: &str = "usage.first_launch_day";

/// Make the store read as one a person has been coming back to: `launches` counted, and the records
/// already filed spread back over `days` separate days.
///
/// The days are the store's own arithmetic (`date`/`strftime`), so nothing here has to know what day
/// it is or how the calendar carries — and the instants are written in the format every record in the
/// store already carries, since what reads them back compares them as text.
///
/// One task is moved per day, most recent first, and the store has to hold at least as many as the
/// days asked for: a premise that quietly spread three days over two records would leave the road
/// judging a threshold nobody reached.
fn wear_in(home: &Path, launches: i64, days: i64) -> Result<String, String> {
    let db = home.join(STORE_FILE);
    if !db.is_file() {
        return Err(format!(
            "there is no store at {} yet — `worn-in` wears in what is already filed, so the records come first",
            db.display()
        ));
    }
    let conn = rusqlite::Connection::open(&db)
        .map_err(|e| format!("could not open the store at {}: {e}", db.display()))?;
    let sql = |e: rusqlite::Error| format!("could not wear the store in: {e}");

    let ids: Vec<i64> = {
        let mut stmt = conn
            .prepare("SELECT id FROM task ORDER BY id DESC LIMIT ?1")
            .map_err(sql)?;
        let rows = stmt.query_map([days], |r| r.get(0)).map_err(sql)?;
        rows.collect::<Result<Vec<i64>, _>>().map_err(sql)?
    };
    if (ids.len() as i64) < days {
        return Err(format!(
            "the store holds {} task(s) and the road asks for {days} separate day(s) of use — one record is moved per day",
            ids.len()
        ));
    }
    for (back, id) in ids.iter().enumerate() {
        conn.execute(
            "UPDATE task SET updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', ?1) WHERE id = ?2",
            rusqlite::params![format!("-{back} days"), id],
        )
        .map_err(sql)?;
    }

    // The first-launch day is the reader's day and not UTC — that is the day amenbo writes there,
    // and a scalar two builds disagree about is worse than one nobody reads. The instants above are
    // UTC for the same reason from the other side: that is how every record in the store carries one.
    let first_day: String = conn
        .query_row("SELECT date('now', 'localtime', ?1)", [format!("-{} days", days - 1)], |r| {
            r.get(0)
        })
        .map_err(sql)?;
    for (key, value) in
        [(LAUNCH_COUNT_KEY, launches.to_string()), (FIRST_LAUNCH_DAY_KEY, first_day.clone())]
    {
        conn.execute(
            "INSERT OR REPLACE INTO store_meta (key, value) VALUES (?1, ?2)",
            rusqlite::params![key, value],
        )
        .map_err(sql)?;
    }

    Ok(format!(
        "wore the store in: {launches} launch(es) counted, {days} separate day(s) written on, first launched {first_day}"
    ))
}

/// Backdate every blob file two hours, comfortably past the hour a young blob is spared for.
fn age_files_in(dir: &Path) -> Result<usize, String> {
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(2 * 60 * 60);
    let files = blob_files(dir)?;
    for path in &files {
        let f = std::fs::File::options()
            .write(true)
            .open(path)
            .map_err(|e| format!("could not open {}: {e}", path.display()))?;
        f.set_modified(old).map_err(|e| format!("could not age {}: {e}", path.display()))?;
    }
    Ok(files.len())
}

#[cfg(test)]
mod worn_in_tests {
    use super::*;

    /// A store carrying `tasks` records, all written today — what a run that made them a moment ago
    /// leaves behind, and the state wearing one in has to move. Only the two tables this reaches into
    /// are stood up: what the rest of the store holds is the shipped build's business, and a fixture
    /// that copied it would be a second declaration of the schema to keep in step.
    fn store_written_today(tasks: i64) -> crate::scratch::Session {
        let session = crate::scratch::session("worn-in-test", false).expect("a throwaway home");
        let conn =
            rusqlite::Connection::open(session.home.join(STORE_FILE)).expect("a store to open");
        conn.execute_batch(
            "CREATE TABLE task (id INTEGER PRIMARY KEY, updated_at TEXT NOT NULL);
             CREATE TABLE store_meta (key TEXT PRIMARY KEY, value TEXT);",
        )
        .expect("the two tables this reaches into");
        for _ in 0..tasks {
            conn.execute(
                "INSERT INTO task (updated_at) VALUES (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))",
                [],
            )
            .expect("a record written today");
        }
        session
    }

    fn meta(home: &Path, key: &str) -> Option<String> {
        let conn = rusqlite::Connection::open(home.join(STORE_FILE)).expect("a store to open");
        conn.query_row("SELECT value FROM store_meta WHERE key = ?1", [key], |r| r.get(0)).ok()
    }

    /// The whole of what the premise claims: the launches are counted, and the records stand on that
    /// many separate days — which is the number the metric behind the offer answers with.
    #[test]
    fn wearing_a_store_in_counts_launches_and_spreads_the_days() {
        let session = store_written_today(3);
        let home = &session.home;
        wear_in(home, 5, 3).expect("a store with three records wears in over three days");

        let conn = rusqlite::Connection::open(home.join(STORE_FILE)).expect("a store to open");
        let days: i64 = conn
            .query_row("SELECT COUNT(DISTINCT substr(updated_at, 1, 10)) FROM task", [], |r| r.get(0))
            .expect("the days the records stand on");
        assert_eq!(days, 3, "one record is moved per day");
        assert_eq!(meta(home, LAUNCH_COUNT_KEY).as_deref(), Some("5"));
        let first: String = conn
            .query_row("SELECT date('now', 'localtime', '-2 days')", [], |r| r.get(0))
            .expect("the day two back");
        assert_eq!(meta(home, FIRST_LAUNCH_DAY_KEY), Some(first), "first launched on the earliest day");
    }

    /// Fewer records than days is refused rather than quietly spread thinner: a road judged against a
    /// threshold nobody reached says nothing about the threshold.
    #[test]
    fn fewer_records_than_days_is_refused() {
        let session = store_written_today(2);
        let err = wear_in(&session.home, 5, 3).expect_err("two records cannot stand on three days");
        assert!(err.contains("2 task(s)"), "{err}");
    }

    /// And a home with no store in it yet, which is what a premise that put this first would meet.
    #[test]
    fn a_home_with_no_store_yet_is_refused() {
        let session = crate::scratch::session("worn-in-test", false).expect("a throwaway home");
        let err = wear_in(&session.home, 5, 3).expect_err("there is nothing to wear in");
        assert!(err.contains("no store"), "{err}");
    }
}
