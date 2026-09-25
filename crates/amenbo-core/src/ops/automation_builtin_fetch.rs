//! **The built-in that goes and fetches** — look at the place set where it is placed, and hand on what
//! was there (`AMB-D-970`).
//!
//! One of the three ways into an automation: a person sets up beforehand where to look, and the run
//! brings back what is there for the next step — most often one that files it as a task. What is looked
//! at is one of three forms, fixed so that the fetching is the code's and the prompt is left only with
//! reading what came back:
//!
//! - **a URL** — fetched with a `GET`;
//! - **a file path** — read, relative to the project's folder where it is not absolute;
//! - **a command** — run in the project's folder through the shell, and what it prints taken.
//!
//! **Nothing there is its own way out** ([`NOT_FOUND`]): a file that is not there, a URL answered `404`
//! or `410`, a command that finished and printed nothing — and anything that came back blank. The run
//! that fetches the issues nobody has looked at yet has, most days, none to fetch. Anything else that
//! goes wrong — a URL that cannot be reached, a command that exits non-zero, what came back not being
//! text — is the error way out every step carries.
//!
//! **No second fetch of the same thing is guarded against** (`AMB-D-970`). Whether two fetches brought
//! back the same thing is decided by reading them; the place looked at narrows what it hands back
//! instead.
//!
//! **The command is not a step's terminal** (`AMB-D-968`). The table of what may be typed in a step
//! governs what an agent types there; this command was written by the person who built the automation,
//! and Amenbo runs it the way it runs a hook. It is started without [`crate::session::STEP_VAR`], so
//! an Amenbo it calls is not taken for a step it has no part in.
//!
//! **Done before the step's transaction** ([`Work::Outside`]): a URL waits on its server and a command
//! on whatever it does, and every other writer to the store would wait with them. Both are cut off at
//! [`TIMEOUT`], since the run stands still until this is done.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use rusqlite::Connection;

use crate::error::{Error, Result};
use crate::model::{AutomationCfgKind, AutomationPortKind};
use crate::ops::automation_builtin::{
    answer, Builtin, BuiltinExit, BuiltinPort, BuiltinSetting, Outside, Work, Worked,
};

/// The way out it leaves by with what it brought back.
pub const FETCHED: &str = "取ってきた";
/// The way out it leaves by when there was nothing there.
pub const NOT_FOUND: &str = "見つからない";
/// The output what it brought back is handed on through.
pub const CONTENT: &str = "中身";
/// The setting that says which of the three forms [`TARGET`] is written in.
pub const FORM: &str = "形式";
/// The setting that says where it looks.
pub const TARGET: &str = "見に行く先";
/// The choices on [`FORM`].
pub const URL: &str = "URL";
pub const FILE_PATH: &str = "ファイルパス";
pub const COMMAND: &str = "コマンド";

/// How long a URL or a command is waited on.
const TIMEOUT: Duration = Duration::from_secs(60);
/// The most it hands on. What it hands on is a value on the run's rows, not a file.
const LIMIT: u64 = 1024 * 1024;

pub(super) const FETCH: Builtin = Builtin {
    key: "fetch",
    name: "取りに行く",
    does: "設定した先（URL・ファイルパス・コマンド）を見に行き、取ってきた中身を渡す。何も無ければ「見つからない」から出る",
    settings: &[
        BuiltinSetting {
            name: FORM,
            kind: AutomationCfgKind::Choice,
            required: true,
            options: Some(r#"["URL","ファイルパス","コマンド"]"#),
        },
        BuiltinSetting { name: TARGET, kind: AutomationCfgKind::Text, required: true, options: None },
    ],
    ins: &[],
    exits: &[
        BuiltinExit {
            name: FETCHED,
            outs: &[BuiltinPort { name: CONTENT, kind: AutomationPortKind::Value, required: true }],
        },
        BuiltinExit { name: NOT_FOUND, outs: &[] },
    ],
    waits: None,
    work: Work::Outside(fetch),
};

/// What looking came to: the text there, or nothing there.
enum Found {
    Text(String),
    Nothing(String),
}

fn fetch(outside: &Outside<'_>) -> Result<Worked> {
    let setting = |name: &str| -> Option<String> {
        answer(outside.cfg, name)
            .and_then(|value| serde_json::from_str::<String>(value).ok())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    };
    let form = setting(FORM).ok_or_else(|| Error::invalid(format!("the setting '{FORM}' is unanswered")))?;
    let target = setting(TARGET).ok_or_else(|| Error::invalid(format!("the setting '{TARGET}' is unanswered")))?;
    let found = match form.as_str() {
        URL => from_url(&target)?,
        FILE_PATH => from_file(&folder_for(outside.conn, outside.run.project_id, &target)?, &target)?,
        COMMAND => from_command(&folder(outside.conn, outside.run.project_id)?, &target)?,
        other => return Err(Error::invalid(format!("'{other}' is not a form this built-in fetches from"))),
    };
    Ok(match found {
        Found::Text(text) if !text.trim().is_empty() => Worked {
            exit: FETCHED,
            report: format!("fetched {} bytes from {form} {target}", text.len()),
            hands: vec![(CONTENT, text)],
        },
        Found::Text(_) => Worked { exit: NOT_FOUND, report: format!("{form} {target} gave back nothing"), hands: vec![] },
        Found::Nothing(why) => Worked { exit: NOT_FOUND, report: why, hands: vec![] },
    })
}

fn from_url(url: &str) -> Result<Found> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .http_status_as_error(false)
        .build()
        .into();
    let mut response = agent
        .get(url)
        .call()
        .map_err(|e| Error::Io(std::io::Error::other(format!("fetching {url}: {e}"))))?;
    let status = response.status();
    if status == 404 || status == 410 {
        return Ok(Found::Nothing(format!("{url} answered {status}")));
    }
    if !status.is_success() {
        return Err(Error::Io(std::io::Error::other(format!("{url} answered {status}"))));
    }
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| Error::Io(std::io::Error::other(format!("reading {url}: {e}"))))?;
    text(bytes, url).map(Found::Text)
}

fn from_file(base: &Path, path: &str) -> Result<Found> {
    let at = base.join(path);
    let file = match std::fs::File::open(&at) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Found::Nothing(format!("there is no file at {}", at.display())))
        }
        Err(e) => return Err(Error::Io(std::io::Error::other(format!("opening {}: {e}", at.display())))),
    };
    let mut bytes = Vec::new();
    file.take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| Error::Io(std::io::Error::other(format!("reading {}: {e}", at.display()))))?;
    text(bytes, &at.to_string_lossy()).map(Found::Text)
}

/// **Run the command through the shell**, in `dir`, and take what it prints. Its output is read on
/// threads of their own while it runs, so a command that prints more than a pipe holds is not stuck
/// waiting for a reader; one still running at [`TIMEOUT`] is killed.
fn from_command(dir: &Path, command: &str) -> Result<Found> {
    let mut shell = shell(command);
    shell
        .current_dir(dir)
        .env_remove(crate::session::STEP_VAR)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = shell
        .spawn()
        .map_err(|e| Error::Io(std::io::Error::other(format!("starting `{command}`: {e}"))))?;
    let drain = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            if let Some(pipe) = pipe {
                let _ = pipe.take(LIMIT + 1).read_to_end(&mut bytes);
            }
            bytes
        })
    };
    let stdout = drain(child.stdout.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let stderr = drain(child.stderr.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let deadline = Instant::now() + TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Error::invalid(format!("`{command}` was still running after {}s", TIMEOUT.as_secs())));
            }
            Err(e) => return Err(Error::Io(std::io::Error::other(format!("waiting on `{command}`: {e}")))),
        }
    };
    let out = stdout.join().unwrap_or_default();
    let err = stderr.join().unwrap_or_default();
    if !status.success() {
        let said = String::from_utf8_lossy(&err);
        let said = said.trim();
        let said: String = said.chars().rev().take(500).collect::<Vec<_>>().into_iter().rev().collect();
        return Err(Error::invalid(format!("`{command}` exited with {status}: {said}")));
    }
    text(out, command).map(Found::Text)
}

#[cfg(not(windows))]
fn shell(command: &str) -> std::process::Command {
    let mut shell = crate::sys::command("sh");
    shell.arg("-c").arg(command);
    shell
}

#[cfg(windows)]
fn shell(command: &str) -> std::process::Command {
    let mut shell = crate::sys::command("cmd");
    shell.arg("/C").arg(command);
    shell
}

/// What came back, as the text a value holds — refused where it is not text or too much of it.
fn text(bytes: Vec<u8>, from: &str) -> Result<String> {
    if bytes.len() as u64 > LIMIT {
        return Err(Error::invalid(format!("{from} gave back more than {} KiB", LIMIT / 1024)));
    }
    String::from_utf8(bytes).map_err(|_| Error::invalid(format!("what {from} gave back is not UTF-8 text")))
}

/// Where a file path is read from: nowhere to ask for an absolute one, the project's folder for any
/// other.
fn folder_for(conn: &Connection, project_id: i64, path: &str) -> Result<PathBuf> {
    match Path::new(path).is_absolute() {
        true => Ok(PathBuf::new()),
        false => folder(conn, project_id),
    }
}

/// **The project's folder** — where a command runs and a relative path is read from. A project in
/// several folders of one repository is in that repository's root. Folders in more than one are
/// refused, since which of them was meant is written nowhere.
fn folder(conn: &Connection, project_id: i64) -> Result<PathBuf> {
    let folders: Vec<PathBuf> = crate::overview::bound_folders(conn)?
        .into_iter()
        .filter(|f| f.project_id == project_id)
        .map(|f| PathBuf::from(f.dir))
        .collect();
    match folders.as_slice() {
        [] => Err(Error::invalid(
            "this project has no folder, so there is nowhere to run a command or read a relative path from",
        )),
        [one] => Ok(one.clone()),
        _ => crate::ops::automation_builtin_cut::repository(conn, project_id, None).map_err(|_| {
            Error::invalid(
                "this project is in more than one folder, and they are not in one repository — \
                 which one to look from is not written anywhere; write an absolute path instead",
            )
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ActorKind, Automation, AutomationPictureOwner};
    use crate::ops::automation::{self, EdgeTarget, NewAutomation};
    use crate::ops::automation_builtin::action;
    use crate::ops::automation_builtin_cut::fixture::bind;
    use crate::ops::automation_builtin_take::{NONE_TO_TAKE, TAKEN};
    use crate::ops::automation_report::Next;
    use crate::ops::automation_run::{launch_leaving_the_task_open as launch, nothing_asked, Launcher};
    use crate::ops::automation_step::Opened;
    use crate::ops::test_support::{mk_placed, mk_project, mk_task_in, open, with_tx};
    use crate::store_engine::{read, WriteTx};

    /// Take a task, fetch, and go on to an agent's step on what came back; nothing there goes on to
    /// close the task. The entry takes the task because a run's entry has to (`EntryTakesNoTask`).
    fn picture(tx: &WriteTx<'_>, project: i64, form: &str, target: &str) -> Automation {
        crate::ops::task::set_assignee(tx, mk_task_in(tx, "one", Some(project)), Some(ActorKind::Ai))
            .expect("give it to the AI");
        let automation =
            automation::add(tx, project, NewAutomation { name: "fetch".into(), ..Default::default() })
                .expect("automation");
        let on = AutomationPictureOwner::Automation;
        let place = |key: &str| {
            automation::placement_add(tx, automation.id, action(tx, key).expect(key).id).expect("place")
        };
        let take = place("take_task");
        let fetch = place("fetch");
        let close = place("close_task");
        automation::cfg_set(tx, fetch.id, FORM, Some(&serde_json::to_string(form).unwrap())).expect("form");
        automation::cfg_set(tx, fetch.id, TARGET, Some(&serde_json::to_string(target).unwrap())).expect("target");
        let (_, work) = mk_placed(tx, &automation, "file", "file it", "claude");
        automation::edge_add(tx, on, take.id, Some(TAKEN), EdgeTarget::Go(fetch.id), None).expect("take → fetch");
        automation::edge_add(tx, on, take.id, Some(NONE_TO_TAKE), EdgeTarget::Done, None).expect("none");
        automation::edge_add(tx, on, fetch.id, Some(FETCHED), EdgeTarget::Go(work.id), None).expect("on");
        automation::edge_add(tx, on, fetch.id, Some(NOT_FOUND), EdgeTarget::Go(close.id), None).expect("none");
        automation::edge_add(tx, on, work.id, None, EdgeTarget::Done, None).expect("work → done");
        automation::edge_add(tx, on, close.id, None, EdgeTarget::Done, None).expect("close → done");
        automation::set_entry(tx, automation.id, Some(take.id)).expect("entry")
    }

    /// Launch, take the task and carry out the fetch. Answers the fetch's execution and where the run
    /// went next.
    fn walk(tx: &WriteTx<'_>, automation: &Automation) -> (i64, Next) {
        let claude = ["claude".to_string()];
        let by = Launcher {
            startable: Some(&claude),
            models: nothing_asked(),
            workspace_open: Some(true),
            by: Some(ActorKind::Ai),
        };
        let run = launch(tx, automation.id, &by).expect("launch");
        let entry = read::automation_run_defs_of(tx.conn(), run.id)
            .expect("defs")
            .into_iter()
            .find(|d| d.entry)
            .expect("the entry");
        let Opened::Carried { next: Next::Step(fetch), .. } = open(tx, run.id, entry.id, Some(&claude)).expect("take")
        else {
            panic!("the take goes on to the fetch");
        };
        let Opened::Carried { run_step_id, next } = open(tx, run.id, fetch.id, Some(&claude)).expect("fetch") else {
            panic!("a built-in is carried out");
        };
        (run_step_id, next)
    }

    /// Where the run went: to the agent's step (`None`), or to the built-in of that key.
    fn went_to(next: &Next) -> Option<&str> {
        match next {
            Next::Step(def) => def.builtin.as_deref(),
            other => panic!("the run goes on to a step, not {other:?}"),
        }
    }

    fn report_and_handed(tx: &WriteTx<'_>, run_step_id: i64) -> (String, Vec<String>) {
        let ran = read::automation_run_step(tx.conn(), run_step_id).expect("read").expect("row");
        let handed = read::automation_run_values_of(tx.conn(), run_step_id)
            .expect("values")
            .into_iter()
            .filter_map(|v| v.value)
            .collect();
        (ran.report, handed)
    }

    /// **A file relative to the project's folder is read and handed on**, and the run goes on to the
    /// step after it.
    #[test]
    fn a_file_is_read_from_the_project_s_folder_and_handed_on() {
        with_tx(|tx| {
            let dir = amenbo_scratch::scratch("builtin-fetch-file");
            std::fs::write(dir.join("inbox.md"), "one thing to do\n").expect("write");
            let project = mk_project(tx, "amenbo");
            bind(tx, project, &[&dir]);
            let automation = picture(tx, project, FILE_PATH, "inbox.md");

            let (run_step_id, next) = walk(tx, &automation);
            assert_eq!(went_to(&next), None, "it goes on with what it fetched");
            let (_, handed) = report_and_handed(tx, run_step_id);
            assert_eq!(handed, vec!["one thing to do\n".to_string()]);
        });
    }

    /// **A file that is not there, and a command that prints nothing, leave by the not-found way out** — not the
    /// error way out.
    #[test]
    fn nothing_there_leaves_by_not_found() {
        let mut nothing = vec![(FILE_PATH, "missing.md")];
        if cfg!(unix) {
            nothing.push((COMMAND, "true"));
        }
        for (form, target) in nothing {
            with_tx(|tx| {
                let dir = amenbo_scratch::scratch("builtin-fetch-nothing");
                let project = mk_project(tx, "amenbo");
                bind(tx, project, &[&dir]);
                let automation = picture(tx, project, form, target);

                let (run_step_id, next) = walk(tx, &automation);
                assert_eq!(went_to(&next), Some("close_task"), "{form}");
                let ran = read::automation_run_step(tx.conn(), run_step_id).expect("read").expect("row");
                assert_eq!(ran.exit_id, crate::ops::test_support::way_out(tx, run_step_id, NOT_FOUND), "{form}");
            });
        }
    }

    /// **A command runs in the project's folder**, not stood in a step, and what it prints is handed on.
    #[cfg(unix)]
    #[test]
    fn a_command_runs_in_the_project_s_folder_and_its_output_is_handed_on() {
        with_tx(|tx| {
            let dir = amenbo_scratch::scratch("builtin-fetch-command");
            std::fs::write(dir.join("here"), "").expect("write");
            let project = mk_project(tx, "amenbo");
            bind(tx, project, &[&dir]);
            let automation = picture(tx, project, COMMAND, "ls; echo \"step=${AMENBO_AUTOMATION_STEP:-none}\"");

            let (run_step_id, next) = walk(tx, &automation);
            assert_eq!(went_to(&next), None);
            let (_, handed) = report_and_handed(tx, run_step_id);
            assert_eq!(handed, vec!["here\nstep=none\n".to_string()]);
        });
    }

    /// **A command that fails leaves by the error way out**, with what it said as the report.
    #[cfg(unix)]
    #[test]
    fn a_failing_command_leaves_by_the_error_way_out() {
        with_tx(|tx| {
            let dir = amenbo_scratch::scratch("builtin-fetch-fails");
            let project = mk_project(tx, "amenbo");
            bind(tx, project, &[&dir]);
            let automation = picture(tx, project, COMMAND, "echo broken >&2; exit 3");

            let (run_step_id, next) = walk(tx, &automation);
            assert!(matches!(next, Next::Halted(_)), "the error way out halts: {next:?}");
            let (report, _) = report_and_handed(tx, run_step_id);
            assert!(report.contains("broken"), "{report}");
        });
    }

    /// **A URL that cannot be reached is the error way out**, not the not-found one.
    #[test]
    fn an_unreachable_url_leaves_by_the_error_way_out() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let automation = picture(tx, project, URL, "http://127.0.0.1:9/nothing");

            let (_, next) = walk(tx, &automation);
            assert!(matches!(next, Next::Halted(_)), "{next:?}");
        });
    }
}
