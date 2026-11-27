//! **The run log** — the detail of each finished round of an automation run, kept outside the database.
//!
//! A run's step executions, their reports, the values that went in and out of them and the files handed
//! between them pile up in the store with every round. Written out here once the round is over, they no
//! longer have to stay in the store to be read again — which is what lets the store let old ones go.
//!
//! - **One folder per run**, `<base>/runs/<run>-<stamp>/` ([`crate::config::Paths::runs_dir`], [`dir_of`]).
//!   Beside `activity.jsonl` and `delivery.jsonl`, not under `logs/`: a report is the user's own content,
//!   and `logs/` is the folder a person sends away for a diagnosis (`AMB-D-382`). The stamp is the run
//!   row's `created_at`, because a restore can hand a run's number to a new run while the old run's
//!   folder is still here.
//! - **One JSONL file per round**, `round-<seq>.jsonl` ([`Round`]): the run as it stood when the round
//!   was written, the round itself, then one line per step execution carrying its values, then one line
//!   per file handed between its steps. Reports and values have no size cap, so neither does a line.
//! - **One more file for the steps no round holds**, `loose.jsonl` ([`LOOSE_FILE_NAME`]): the step
//!   executions of the run whose `run_task_id` is NULL — every step of a run a trigger started that takes
//!   no task, and the steps a run ran before its first round. The same lines as a round file, less the
//!   round's. Written once, as the run ends: no step joins them after that.
//! - **The step definitions once per run**, `defs/v<version>-p<placement>-s<step>.json` ([`def_path`]):
//!   each step a round ran, as the run read it off the saved version it ran on (`AMB-D-1219`), written
//!   the first time a round uses it. A run picked up on a newer version (`AMB-D-1015`) writes that
//!   version's steps beside the older ones. Writing them into every round would repeat the same text
//!   round after round. A build before v154 wrote the run's copies of the steps here instead, under
//!   their row ids (`defs/<id>.json`), and its round files name a step's copy by that id
//!   (`run_def_id`); both are still read ([`detail`]).
//! - **The files handed between steps**, `files/<attachment id>`: the bytes of each blob attachment,
//!   copied out of the blob store. The line for it in the round file carries its name and type.
//! - **Written after the commit** succeeds, as the activity ledger is — gathered inside the transaction
//!   ([`crate::store_engine::WriteTx::record_round`], [`crate::store_engine::WriteTx::record_loose`]) and
//!   written by [`crate::store_engine::WriteTx::commit`]. A crash before the commit leaves no file; a
//!   crash after it loses the round. The round is not lost from the store by that: the store lets a
//!   round's detail go only once its file is here — an old round of a run under way
//!   (`ops::automation_step`'s `let_go_of_a_round`), and every round and the steps no round holds of a
//!   finished run of any automation and role, a little as each run ends (`ops::automation`'s
//!   `let_finished_rounds_go`). A finished run whose steps no round holds have no file — lost to a crash,
//!   or ended before this file was kept — has them gathered there, and they go the time after.
//! - **When a round is over** is when the next one begins or the run ends — not when the round row is
//!   given its `ended_at`. A step that hands its task back closes the round there, and the steps after it
//!   still belong to it.
//! - **Concurrency without a lock.** A round file is written whole to a temporary name in the run's
//!   folder and renamed into place, so a reader never sees half of one, and two writers never share a
//!   file to interleave in. A definition file and a copied blob are written the same way, and one that is
//!   already there is left alone: neither changes once written.
//!
//! - **Read back with the store** ([`detail`]): a finished round from its file, the steps no round holds
//!   from theirs, and the round under way and what no file holds from the store, joined into one account
//!   of the run.
//! - **Removed with its run.** Whoever deletes a run row says so ([`crate::store_engine::WriteTx::forget_run`]),
//!   and the commit removes the run's folder after it succeeds ([`forget`]) — the run of a deleted
//!   automation or project, and an old run let go once its role holds more than it keeps
//!   (`ops::automation`'s `let_old_runs_go`). A crash between the two leaves a folder no run names.
//!
//! Machine-local, and outside every backup and export, as the other logs are (`AMB-D-149`, `AMB-D-361`).
//! Failure is never fatal here: a full disk or a read-only directory produces a `warn` and the mutation
//! carries on.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::model::{
    Attachment, AttachmentTarget, AutomationRun, AutomationRunDef, AutomationRunStep, AutomationRunTask, AutomationRunValue,
    RunDefKey,
};
use crate::store_engine::read;

/// Name of the folder that holds every run's folder, kept in the base directory.
pub const DIR_NAME: &str = "runs";

/// Schema version stamped on every line and file this build writes.
pub const LINE_VERSION: i64 = 1;

/// Where the run log is written, and where the bytes it copies are read from.
#[derive(Clone, Debug)]
pub struct Place {
    /// `<base>/runs` ([`crate::config::Paths::runs_dir`]).
    pub runs: PathBuf,
    /// The store's blob directory, `<base>/blobs`.
    pub blobs: PathBuf,
}

/// **The folder of one run** — `<runs>/<run id>-<created_at>`, the stamp kept to the characters a file
/// name takes on every platform. The run row alone names it, so whoever deletes the run can delete the
/// folder with it.
pub fn dir_of(runs: &Path, run: &AutomationRun) -> PathBuf {
    let stamp: String = run.created_at.to_rfc3339_z().chars().filter(char::is_ascii_alphanumeric).collect();
    runs.join(format!("{}-{stamp}", run.id))
}

/// The file one round is written to, in its run's folder.
pub fn round_file_name(seq: i64) -> String {
    format!("round-{seq}.jsonl")
}

/// The file the step executions no round holds are written to, in their run's folder ([`Round::gather_loose`]).
pub const LOOSE_FILE_NAME: &str = "loose.jsonl";

/// **The file one step is written to**, in its run's folder `dir` — named by the version it was read off,
/// the placement and the step ([`RunDefKey`]), so each step of each version a run ran on has one. A part
/// a step does not name reads `x`.
pub fn def_path(dir: &Path, at: RunDefKey) -> PathBuf {
    let part = |n: Option<i64>| n.map_or_else(|| "x".to_string(), |n| n.to_string());
    let name = format!("v{}-p{}-s{}.json", part(at.automation_version), part(at.placement_id), part(at.step_id));
    dir.join("defs").join(name)
}

/// One finished round, read out of the store inside the transaction that ended it, ready to write once
/// that transaction commits — or the steps of a finished run that no round holds, which are written the
/// same way.
#[derive(Clone, Debug)]
pub struct Round {
    pub run: AutomationRun,
    /// The round, or `None` for the steps no round holds ([`Round::gather_loose`]).
    pub round: Option<AutomationRunTask>,
    /// The step executions of the round, in the order they happened, each with its values.
    pub steps: Vec<Value>,
    /// The steps the round's executions ran, as the run read them.
    pub defs: Vec<AutomationRunDef>,
    /// The files attached to the round's step executions.
    pub files: Vec<Attachment>,
}

impl Round {
    /// **Read one round out of the store** — the round row `round_id` of run `run_id`, as the connection
    /// sees it now. `None` when either row is gone.
    pub fn gather(conn: &rusqlite::Connection, run_id: i64, round_id: i64) -> crate::error::Result<Option<Round>> {
        let (Some(run), Some(round)) = (read::automation_run(conn, run_id)?, read::automation_run_task(conn, round_id)?)
        else {
            return Ok(None);
        };
        let executions = read::automation_run_steps_of_task(conn, round.id)?;
        Self::of(conn, run, Some(round), executions).map(Some)
    }

    /// **Read the steps no round holds out of the store** — the step executions of run `run_id` whose
    /// `run_task_id` is NULL, as the connection sees them now. `None` when the run is gone or has none.
    pub fn gather_loose(conn: &rusqlite::Connection, run_id: i64) -> crate::error::Result<Option<Round>> {
        let Some(run) = read::automation_run(conn, run_id)? else { return Ok(None) };
        let executions: Vec<AutomationRunStep> =
            read::automation_run_steps_of(conn, run_id)?.into_iter().filter(|s| s.run_task_id.is_none()).collect();
        if executions.is_empty() {
            return Ok(None);
        }
        Self::of(conn, run, None, executions).map(Some)
    }

    /// The executions `executions` of `run` with their values, files and the steps they ran.
    fn of(
        conn: &rusqlite::Connection,
        run: AutomationRun,
        round: Option<AutomationRunTask>,
        executions: Vec<AutomationRunStep>,
    ) -> crate::error::Result<Round> {
        let mut steps = Vec::with_capacity(executions.len());
        let mut files = Vec::new();
        for step in &executions {
            let values = read::automation_run_values_of(conn, step.id)?;
            steps.push(json!({ "step": step, "values": values }));
            for row in read::attachments_for_target(conn, AttachmentTarget::AutomationRunStep, step.id)? {
                files.extend(read::attachment(conn, row.id)?);
            }
        }
        let defs = crate::ops::automation_run::defs_named(conn, &run, &executions)?.into_values().collect();
        Ok(Round { run, round, steps, defs, files })
    }

    /// The file it is written to, in its run's folder.
    pub fn file_name(&self) -> String {
        self.round.as_ref().map_or_else(|| LOOSE_FILE_NAME.to_string(), |r| round_file_name(r.seq))
    }

    /// The round file's lines, each LF-terminated.
    fn to_lines(&self) -> Vec<u8> {
        let mut lines = vec![json!({ "v": LINE_VERSION, "kind": "run", "run": self.run })];
        lines.extend(self.round.iter().map(|r| json!({ "v": LINE_VERSION, "kind": "round", "round": r })));
        lines.extend(self.steps.iter().map(|s| json!({ "v": LINE_VERSION, "kind": "step", "step": s["step"], "values": s["values"] })));
        lines.extend(self.files.iter().map(|f| {
            let path = f.blob_hash.as_ref().map(|_| file_path(f.id));
            json!({ "v": LINE_VERSION, "kind": "file", "attachment": f, "path": path })
        }));
        let mut out = Vec::new();
        for line in &lines {
            out.extend(serde_json::to_vec(line).unwrap_or_default());
            out.push(b'\n');
        }
        out
    }
}

/// Where a copied file sits, relative to its run's folder.
fn file_path(attachment_id: i64) -> String {
    format!("files/{attachment_id}")
}

/// **Write one finished round** — or the steps no round holds — into its run's folder. Never fails the
/// caller: what cannot be written is a `warn`, and the rest of the round is still written where it can be.
pub fn write(place: &Place, round: &Round) {
    let dir = dir_of(&place.runs, &round.run);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        tracing::warn!(run = round.run.id, error = %e, "run log: cannot make the run's folder; round dropped");
        return;
    }
    for def in &round.defs {
        let bytes = serde_json::to_vec(&json!({ "v": LINE_VERSION, "def": def })).unwrap_or_default();
        if let Err(e) = write_once(&def_path(&dir, def.key()), |f| f.write_all(&bytes)) {
            tracing::warn!(run = round.run.id, step = ?def.key(), error = %e, "run log: step definition not written");
        }
    }
    let blobs = crate::blob::BlobStore::at(place.blobs.clone());
    for file in &round.files {
        let Some(hash) = file.blob_hash.as_deref() else { continue };
        let copied = blobs.open(hash).map_err(|e| std::io::Error::other(e.to_string())).and_then(|mut from| {
            write_once(&dir.join(file_path(file.id)), |to| std::io::copy(&mut from, to).map(|_| ()))
        });
        if let Err(e) = copied {
            tracing::warn!(run = round.run.id, attachment = file.id, error = %e, "run log: file not copied");
        }
    }
    let lines = round.to_lines();
    if let Err(e) = write_whole(&dir.join(round.file_name()), |f| f.write_all(&lines)) {
        tracing::warn!(run = round.run.id, file = %round.file_name(), error = %e, "run log: round not written");
    }
}

/// **Remove the folder of a run that was deleted**, everything in it. A folder that is not there is
/// nothing to do; one that cannot be removed is a `warn`.
pub fn forget(place: &Place, run: &AutomationRun) {
    let dir = dir_of(&place.runs, run);
    match std::fs::remove_dir_all(&dir) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => tracing::warn!(run = run.id, error = %e, "run log: the deleted run's folder not removed"),
    }
}

/// **One run read back whole** — the run log's rounds and the store's, joined ([`detail`]).
#[derive(Clone, Debug, Default)]
pub struct Detail {
    /// Every step the run's executions ran, as the run read them, in the order of [`RunDefKey`] — the
    /// run log's where it holds one, and read off the saved version otherwise. What an execution ran is
    /// the one whose [`AutomationRunDef::key`] is its [`AutomationRunStep::def_key`].
    pub defs: Vec<AutomationRunDef>,
    /// Every round of the run, from the store, which keeps each round row.
    pub rounds: Vec<AutomationRunTask>,
    /// Every step execution, each with its values, in the order they happened.
    pub steps: Vec<StepDetail>,
}

/// One step execution, with the values that went in and out of it.
#[derive(Clone, Debug)]
pub struct StepDetail {
    pub step: AutomationRunStep,
    pub values: Vec<AutomationRunValue>,
}

/// **Read one run back whole**: each round whose file the run log holds from that file, the steps no
/// round holds from theirs ([`LOOSE_FILE_NAME`]), and the rest from the store — the round under way, a
/// round written before the run log was kept or lost to a crash, and the steps no round holds of a run
/// with no such file. What is in both is read from its file once, as the store may already be letting
/// its detail go. `None` where the store has no such run: a folder no run names is not read.
///
/// **The steps the executions ran** are read from the run's folder where it holds them, and off the saved
/// versions they name otherwise ([`crate::ops::automation_run::defs_named`]). A round file a build before
/// v154 wrote names the run's copy of a step by its row id, and that copy is in the folder under the same
/// id: the execution is given the version, the placement and the step the copy names.
///
/// `runs` is `<base>/runs`, or `None` for a store that keeps no run log, which reads the store alone.
pub fn detail(conn: &rusqlite::Connection, runs: Option<&Path>, run_id: i64) -> crate::error::Result<Option<Detail>> {
    let Some(run) = read::automation_run(conn, run_id)? else { return Ok(None) };
    let rounds = read::automation_run_tasks_of(conn, run_id)?;
    let mut defs: std::collections::BTreeMap<RunDefKey, AutomationRunDef> = std::collections::BTreeMap::new();
    let mut steps = Vec::new();
    let mut from_file = std::collections::BTreeSet::new();
    let mut loose_from_file = false;
    if let Some(runs) = runs {
        let dir = dir_of(runs, &run);
        let written = read_defs(&dir);
        let copies: std::collections::BTreeMap<i64, RunDefKey> =
            written.iter().filter(|def| def.id != 0).map(|def| (def.id, def.key())).collect();
        for round in &rounds {
            if let Some(read) = read_steps(&dir, &round_file_name(round.seq), &copies) {
                from_file.insert(round.id);
                steps.extend(read);
            }
        }
        if let Some(read) = read_steps(&dir, LOOSE_FILE_NAME, &copies) {
            loose_from_file = true;
            steps.extend(read);
        }
        for def in written {
            defs.entry(def.key()).or_insert(def);
        }
    }
    for step in read::automation_run_steps_of(conn, run_id)? {
        let read = match step.run_task_id {
            Some(id) => from_file.contains(&id),
            None => loose_from_file,
        };
        if read {
            continue;
        }
        let values = read::automation_run_values_of(conn, step.id)?;
        steps.push(StepDetail { step, values });
    }
    steps.sort_by_key(|s| (s.step.seq, s.step.id));
    let unread: Vec<AutomationRunStep> =
        steps.iter().filter(|s| !defs.contains_key(&s.step.def_key())).map(|s| s.step.clone()).collect();
    for (key, def) in crate::ops::automation_run::defs_named(conn, &run, &unread)? {
        defs.entry(key).or_insert(def);
    }
    Ok(Some(Detail { defs: defs.into_values().collect(), rounds, steps }))
}

/// The step executions in the file `name` of the run's folder `dir` — a round's, or the steps no round
/// holds. `None` where there is no file, or it cannot be read whole — a `warn`, and the caller reads the
/// store instead.
///
/// An execution a build before v154 wrote names the copy of its step by row id (`run_def_id`) rather
/// than by version and step: it is given the key `copies` holds for that id ([`RunDefKey`]).
fn read_steps(dir: &Path, name: &str, copies: &std::collections::BTreeMap<i64, RunDefKey>) -> Option<Vec<StepDetail>> {
    let path = dir.join(name);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "run log: round not read");
            return None;
        }
    };
    let parsed: Result<Vec<Option<StepDetail>>, serde_json::Error> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            let line: Value = serde_json::from_str(line)?;
            if line["kind"] != "step" {
                return Ok(None);
            }
            let mut step: AutomationRunStep = serde_json::from_value(line["step"].clone())?;
            if let Some(at) = line["step"]["run_def_id"].as_i64().and_then(|id| copies.get(&id)) {
                if step.def_key() == RunDefKey::default() {
                    step.automation_version = at.automation_version;
                    step.placement_id = at.placement_id;
                    step.step_id = at.step_id;
                }
            }
            let values = serde_json::from_value(line["values"].clone())?;
            Ok(Some(StepDetail { step, values }))
        })
        .collect();
    match parsed {
        Ok(steps) => Some(steps.into_iter().flatten().collect()),
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "run log: round not read");
            None
        }
    }
}

/// The steps written in the run's folder `dir` — under either name ([`def_path`], and a build before
/// v154's `<id>.json`). One that cannot be read is a `warn` and is left out.
fn read_defs(dir: &Path) -> Vec<AutomationRunDef> {
    let Ok(entries) = std::fs::read_dir(dir.join("defs")) else { return Vec::new() };
    let mut defs = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let read = std::fs::read(&path)
            .map_err(|e| e.to_string())
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).map_err(|e| e.to_string()))
            .and_then(|file| serde_json::from_value::<AutomationRunDef>(file["def"].clone()).map_err(|e| e.to_string()));
        match read {
            Ok(def) => defs.push(def),
            Err(e) => tracing::warn!(path = %path.display(), error = %e, "run log: step definition not read"),
        }
    }
    defs
}

/// Write `path` unless it is already there — for what never changes once written.
fn write_once(path: &Path, fill: impl FnOnce(&mut std::fs::File) -> std::io::Result<()>) -> std::io::Result<()> {
    if path.exists() {
        return Ok(());
    }
    write_whole(path, fill)
}

/// Write `path` whole: to a temporary name beside it first, then renamed into place, so nobody ever reads
/// it half written and two writers never write into the same file.
fn write_whole(path: &Path, fill: impl FnOnce(&mut std::fs::File) -> std::io::Result<()>) -> std::io::Result<()> {
    let dir = path.parent().ok_or_else(|| std::io::Error::other("no parent folder"))?;
    std::fs::create_dir_all(dir)?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let tmp = dir.join(format!(".{name}.{}-{nanos}.tmp", std::process::id()));
    let written = std::fs::File::create(&tmp).and_then(|mut f| {
        fill(&mut f)?;
        f.sync_all()
    });
    let renamed = written.and_then(|()| std::fs::rename(&tmp, path));
    if renamed.is_err() {
        std::fs::remove_file(&tmp).ok();
    }
    renamed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::Timestamp;

    fn place(tag: &str) -> (PathBuf, Place) {
        let dir = amenbo_scratch::scratch(&format!("run-log-{tag}"));
        let place = Place { runs: dir.join(DIR_NAME), blobs: dir.join(crate::blob::BLOBS_SUBDIR) };
        (dir, place)
    }

    fn run(id: i64, at: &str) -> AutomationRun {
        AutomationRun { id, created_at: Timestamp::parse_rfc3339(at).unwrap(), ..Default::default() }
    }

    /// The step every round below ran: step 3 of placement 2, in version 1.
    const AT: RunDefKey = RunDefKey { automation_version: Some(1), placement_id: Some(2), step_id: Some(3) };

    fn round(run: &AutomationRun, seq: i64, report: &str) -> Round {
        let step = AutomationRunStep {
            id: seq * 10,
            run_id: run.id,
            automation_version: AT.automation_version,
            placement_id: AT.placement_id,
            step_id: AT.step_id,
            report: report.into(),
            ..Default::default()
        };
        let def = AutomationRunDef {
            automation_version: AT.automation_version,
            placement_id: AT.placement_id,
            step_id: AT.step_id,
            name: "Alice".into(),
            ..Default::default()
        };
        Round {
            run: run.clone(),
            round: Some(AutomationRunTask { id: seq, run_id: run.id, seq, ..Default::default() }),
            steps: vec![json!({ "step": step, "values": [] })],
            defs: vec![def],
            files: vec![],
        }
    }

    fn lines(path: &Path) -> Vec<Value> {
        std::fs::read_to_string(path)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).expect("every line is a whole JSON object"))
            .collect()
    }

    /// A run whose number a restore handed out again gets a folder of its own: the folder is told apart
    /// by when the run was made, not by its number alone.
    #[test]
    fn a_reused_run_number_gets_a_folder_of_its_own() {
        let runs = Path::new("/base/runs");
        let old = dir_of(runs, &run(7, "2026-11-01T10:00:00Z"));
        let new = dir_of(runs, &run(7, "2026-11-13T10:00:00Z"));
        assert_ne!(old, new);
        let name = old.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with("7-"), "{name}");
        assert!(name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'), "{name}");
    }

    /// A round's file holds the run, the round and each step with its values — and a report keeps every
    /// byte, however long, since a line has no cap here.
    #[test]
    fn a_round_is_written_whole_into_its_run_folder() {
        let (dir, place) = place("whole");
        let run = run(3, "2026-11-13T10:00:00Z");
        let long = "山田".repeat(20_000);

        write(&place, &round(&run, 1, &long));

        let folder = dir_of(&place.runs, &run);
        let lines = lines(&folder.join(round_file_name(1)));
        let kinds: Vec<&str> = lines.iter().map(|l| l["kind"].as_str().unwrap()).collect();
        assert_eq!(kinds, vec!["run", "round", "step"]);
        assert_eq!(lines[0]["v"], json!(LINE_VERSION));
        assert_eq!(lines[0]["run"]["id"], json!(3));
        assert_eq!(lines[1]["round"]["seq"], json!(1));
        assert_eq!(lines[2]["step"]["report"], json!(long));
        assert!(folder.join("defs/v1-p2-s3.json").exists(), "the step the round ran is written beside it");
        assert_eq!(def_path(&folder, AT), folder.join("defs/v1-p2-s3.json"));
        let leftovers: Vec<_> = std::fs::read_dir(&folder).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().ends_with(".tmp")).collect();
        assert!(leftovers.is_empty(), "no temporary file is left behind");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A definition is written the first time a round uses it and left as it is after that.
    #[test]
    fn a_definition_is_written_once_per_run() {
        let (dir, place) = place("defs");
        let run = run(4, "2026-11-13T10:00:00Z");
        write(&place, &round(&run, 1, "first"));
        let def = def_path(&dir_of(&place.runs, &run), AT);
        std::fs::write(&def, b"kept").unwrap();

        write(&place, &round(&run, 2, "second"));

        assert_eq!(std::fs::read(&def).unwrap(), b"kept");
        assert!(dir_of(&place.runs, &run).join(round_file_name(2)).exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A file handed between steps is copied out of the blob store under its attachment's id.
    #[test]
    fn a_handed_file_is_copied_beside_the_round() {
        let (dir, place) = place("files");
        let blobs = crate::blob::BlobStore::at(place.blobs.clone());
        let blob = blobs.ingest_bytes(b"Bob's notes").unwrap();
        let run = run(5, "2026-11-13T10:00:00Z");
        let mut round = round(&run, 1, "done");
        round.files.push(Attachment {
            id: 42,
            target_type: AttachmentTarget::AutomationRunStep,
            target_id: 10,
            blob_hash: Some(blob.hash.clone()),
            filename: Some("notes.md".into()),
            ..Default::default()
        });

        write(&place, &round);

        let folder = dir_of(&place.runs, &run);
        assert_eq!(std::fs::read(folder.join("files/42")).unwrap(), b"Bob's notes");
        let lines = lines(&folder.join(round_file_name(1)));
        assert_eq!(lines[3]["kind"], json!("file"));
        assert_eq!(lines[3]["path"], json!("files/42"));
        assert_eq!(lines[3]["attachment"]["filename"], json!("notes.md"));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Rounds written at once, from many threads, each land whole in a file of their own — nothing is
    /// interleaved, however long the lines.
    #[test]
    fn rounds_written_at_once_never_interleave() {
        let (dir, place) = place("concurrent");
        let run = run(6, "2026-11-13T10:00:00Z");
        let handles: Vec<_> = (1..=8)
            .map(|seq| {
                let (place, run) = (place.clone(), run.clone());
                std::thread::spawn(move || write(&place, &round(&run, seq, &format!("{seq}").repeat(100_000))))
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        let folder = dir_of(&place.runs, &run);
        for seq in 1..=8 {
            let lines = lines(&folder.join(round_file_name(seq)));
            assert_eq!(lines.len(), 3);
            assert_eq!(lines[2]["step"]["report"], json!(format!("{seq}").repeat(100_000)));
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A deleted run's folder goes whole, and only its own: a run with the same number made at another
    /// time keeps its folder, and a run that never wrote one is nothing to remove.
    #[test]
    fn a_deleted_runs_folder_goes_whole() {
        let (dir, place) = place("forget");
        let old = run(9, "2026-11-01T10:00:00Z");
        let new = run(9, "2026-11-13T10:00:00Z");
        write(&place, &round(&old, 1, "old"));
        write(&place, &round(&new, 1, "new"));

        forget(&place, &old);
        forget(&place, &run(10, "2026-11-13T10:00:00Z"));

        assert!(!dir_of(&place.runs, &old).exists());
        assert!(dir_of(&place.runs, &new).join(round_file_name(1)).exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A round written is read back as it was written: its steps with their values, and the copies of
    /// the steps beside it. A round with no file, or one that does not read whole, is `None`.
    #[test]
    fn a_written_round_reads_back() {
        let (dir, place) = place("read");
        let run = run(11, "2026-11-13T10:00:00Z");
        let long = "さくら".repeat(10_000);
        write(&place, &round(&run, 1, &long));
        let folder = dir_of(&place.runs, &run);

        let none = std::collections::BTreeMap::new();
        let steps = read_steps(&folder, &round_file_name(1), &none).expect("the round reads back");
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].step.report, long);
        assert_eq!(steps[0].step.def_key(), AT);
        assert!(steps[0].values.is_empty());
        let defs = read_defs(&folder);
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "Alice");
        assert_eq!(defs[0].key(), AT);

        assert!(read_steps(&folder, &round_file_name(2), &none).is_none(), "no file, nothing read");
        std::fs::write(folder.join(round_file_name(3)), b"{\"v\":1,\"kind\":\"step\",\"step\":{}}\n").unwrap();
        assert!(read_steps(&folder, &round_file_name(3), &none).is_none(), "a round that does not read whole is not read");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **A run folder an older build wrote still reads**: its copies of the steps under their row ids
    /// (`defs/<id>.json`), and its round files naming a step's copy by that id (`run_def_id`). The step
    /// is given the version, placement and step its copy names, and the copy reads beside the steps
    /// written under the name a version and a step make.
    #[test]
    fn a_folder_an_older_build_wrote_reads_back() {
        let (dir, place) = place("older");
        let run = run(12, "2026-11-13T10:00:00Z");
        let folder = dir_of(&place.runs, &run);
        std::fs::create_dir_all(folder.join("defs")).unwrap();
        let copy = json!({ "v": 1, "def": {
            "id": 5, "run_id": 12, "placement_id": 2, "step_id": 3, "automation_version": 1, "name": "Bob",
            "agent": "claude", "interactive": false, "report_to_task": false, "show_history": false,
            "exits": "[]", "ins": "[]", "cfg": "[]", "roles": "[]",
            "created_at": "2026-11-13T10:00:00Z", "updated_at": "2026-11-13T10:00:00Z"
        } });
        std::fs::write(folder.join("defs/5.json"), serde_json::to_vec(&copy).unwrap()).unwrap();
        let step = json!({ "v": 1, "kind": "step", "values": [], "step": {
            "id": 10, "run_id": 12, "run_def_id": 5, "seq": 1, "report": "山田", "status": "done",
            "created_at": "2026-11-13T10:00:00Z", "updated_at": "2026-11-13T10:00:00Z"
        } });
        std::fs::write(folder.join(round_file_name(1)), format!("{step}\n")).unwrap();
        write(&place, &round(&run, 2, "now"));

        let defs = read_defs(&folder);
        assert_eq!(defs.len(), 2, "the copy under its id and the step under its version both read");
        let copies: std::collections::BTreeMap<i64, RunDefKey> =
            defs.iter().filter(|d| d.id != 0).map(|d| (d.id, d.key())).collect();
        assert_eq!(copies.get(&5), Some(&AT));
        let steps = read_steps(&folder, &round_file_name(1), &copies).expect("the older round reads back");
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].step.def_key(), AT, "the step is given what its copy names");
        assert_eq!(steps[0].step.report, "山田");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **The steps no round holds** are written to a file of their own, with no round line, and read
    /// back like a round's.
    #[test]
    fn the_steps_no_round_holds_are_written_and_read_back() {
        let (dir, place) = place("loose");
        let run = run(13, "2026-11-13T10:00:00Z");
        let mut loose = round(&run, 1, "佐藤");
        loose.round = None;

        write(&place, &loose);

        let folder = dir_of(&place.runs, &run);
        assert_eq!(loose.file_name(), LOOSE_FILE_NAME);
        let kinds: Vec<String> =
            lines(&folder.join(LOOSE_FILE_NAME)).iter().map(|l| l["kind"].as_str().unwrap().to_string()).collect();
        assert_eq!(kinds, vec!["run", "step"]);
        assert!(def_path(&folder, AT).exists(), "the step they ran is written beside them");
        let steps = read_steps(&folder, LOOSE_FILE_NAME, &std::collections::BTreeMap::new()).expect("they read back");
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].step.report, "佐藤");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A folder that cannot be made costs the round and nothing else.
    #[test]
    fn a_folder_that_cannot_be_made_is_only_a_warning() {
        let (dir, _) = place("blocked");
        let blocker = dir.join("not-a-dir");
        std::fs::write(&blocker, b"").unwrap();
        let place = Place { runs: blocker.join(DIR_NAME), blobs: dir.join("blobs") };
        write(&place, &round(&run(8, "2026-11-13T10:00:00Z"), 1, "x"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
