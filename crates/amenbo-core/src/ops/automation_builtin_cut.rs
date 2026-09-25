//! **The built-in that cuts a worktree** — the task the run is on gets its own checkout, and the path
//! is handed on (`AMB-D-964`).
//!
//! What a prompt used to be asked to do with `worktree start` and `step-out`, done here where the step
//! is opened. The next step takes the path as its working folder (`work_dir_ref`), so the agent opens
//! already standing in it.
//!
//! **Cut from the newest of the remote's default branch.** The remote is fetched first and the branch is
//! cut from `origin/<default>` ([`worktree_cut::start_from_origin`]). The main worktree is not touched:
//! whatever it stands on, and whatever it has not pulled, a person may be working there.
//!
//! **Done before the step's transaction** ([`Work::Outside`]): the fetch waits on the remote, and the
//! store is only read to find the task and its repository.
//!
//! **The repository is the project's.** A task that names one of its project's folders is cut from that
//! folder's repository. One that names none is cut from the repository the project's folders are in —
//! and refused where they are in more than one, since which of them the task belongs to is not written
//! anywhere. The CLI answers the same question from the folder it is typed in, and refuses a task
//! worked somewhere else; a run is typed in no folder, so the task and its project are all it has.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::error::{Error, Result};
use crate::model::{AutomationPortKind, DONE_EXIT};
use crate::ops::automation_builtin::{Builtin, BuiltinExit, BuiltinPort, Outside, Work, Worked};
use crate::store_engine::read;
use crate::worktree_cut::{self, Refusal};

/// The output the worktree's path is handed on through.
pub const WORKTREE: &str = "worktree";

pub(super) const CUT_WORKTREE: Builtin = Builtin {
    key: "cut_worktree",
    name: "worktree を切る",
    does: "いま扱っているタスクの worktree を、リモートの既定ブランチの最新から切り、そのパスを渡す",
    settings: &[],
    ins: &[],
    exits: &[BuiltinExit {
        name: DONE_EXIT,
        outs: &[BuiltinPort { name: WORKTREE, kind: AutomationPortKind::Value, required: true }],
    }],
    waits: None,
    chooses: None,
    work: Work::Outside(cut),
};

fn cut(outside: &Outside<'_>) -> Result<Worked> {
    let task_id = outside
        .task_id
        .ok_or_else(|| Error::invalid("there is no task to cut a worktree for — the run has not taken one"))?;
    let task = read::task(outside.conn, task_id)?
        .ok_or_else(|| Error::not_found(format!("task AMB-T-{task_id}")))?;
    let root = repository(outside.conn, outside.run.project_id, task.at_binding_id)?;
    let cut = worktree_cut::layout(&root, &task_id.to_string());
    let from = worktree_cut::start_from_origin(&cut).map_err(refused)?;
    let path = cut.worktree.to_string_lossy().into_owned();
    let report = format!("cut {path} on {} from {from}", cut.branch);
    Ok(Worked { exit: DONE_EXIT, report, hands: vec![(WORKTREE, path)] })
}

/// **The repository the task is worked in**: the one its own folder is in, or else the one every
/// folder of its project is in.
pub(super) fn repository(conn: &Connection, project_id: i64, at: Option<i64>) -> Result<PathBuf> {
    let folders: Vec<_> = crate::overview::bound_folders(conn)?
        .into_iter()
        .filter(|f| f.project_id == project_id)
        .collect();
    if let Some(named) = at.and_then(|id| folders.iter().find(|f| f.id == id)) {
        return worktree_cut::git_root(Path::new(&named.dir)).map_err(refused);
    }
    let roots: BTreeSet<PathBuf> =
        folders.iter().filter_map(|f| worktree_cut::git_root(Path::new(&f.dir)).ok()).collect();
    let mut roots = roots.into_iter();
    match (roots.next(), roots.next()) {
        (Some(root), None) => Ok(root),
        (None, _) => Err(Error::invalid(
            "no folder of this project is in a git repository, so there is nothing to cut a worktree from",
        )),
        (Some(_), Some(_)) => Err(Error::invalid(
            "this project's folders are in more than one repository, and the task names none of them — \
             which one to cut from is not written anywhere",
        )),
    }
}

/// A refusal from the git side, as one sentence.
pub(super) fn refused(refusal: Refusal) -> Error {
    Error::invalid(match refusal {
        Refusal::NoGit => "there is no git on this machine to cut a worktree with".to_string(),
        Refusal::NotARepository(why) => format!("the task's folder is in no repository: {why}"),
        Refusal::WorktreeExists(at) => format!("a worktree for this task is standing already at {}", at.display()),
        Refusal::BranchExists(branch) => format!("the branch {branch} is there already, with no worktree on it"),
        Refusal::Git(said) => said,
        Refusal::Io(said) => said,
        other => format!("{other:?}"),
    })
}

/// A remote and a clone of it on disk, for the built-ins that drive git to be tested against.
#[cfg(test)]
pub(super) mod fixture {
    use crate::store_engine::WriteTx;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    pub(crate) fn git(dir: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .current_dir(dir)
            .args(["-c", "user.name=Alice", "-c", "user.email=alice@example.com", "-c", "init.defaultBranch=main"])
            .args(args)
            .output()
            .expect("git");
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    /// A remote with one commit, a clone of it — the project's folder — and a second commit that lands
    /// on the remote after the clone, so the clone's own `main` is one behind. Answers the clone and the
    /// newest commit on the remote.
    pub(crate) fn repositories(tag: &str) -> (PathBuf, String) {
        let dir = amenbo_scratch::scratch(tag);
        let work = dir.join("work");
        std::fs::create_dir_all(&work).expect("mkdir");
        git(&work, &["init", "--quiet"]);
        git(&work, &["commit", "--quiet", "--allow-empty", "-m", "first"]);
        git(&dir, &["clone", "--quiet", "--bare", "work", "origin.git"]);
        git(&dir, &["clone", "--quiet", "origin.git", "app"]);
        git(&work, &["commit", "--quiet", "--allow-empty", "-m", "second"]);
        git(&work, &["push", "--quiet", "../origin.git", "main"]);
        (dir.join("app"), git(&work, &["rev-parse", "HEAD"]))
    }

    pub(crate) fn bind(tx: &WriteTx<'_>, project: i64, dirs: &[&Path]) {
        let mut reg = crate::binding::Registry::default();
        for dir in dirs {
            reg.project_dirs.entry(project).or_default().insert(dir.to_string_lossy().into_owned());
        }
        crate::overview::write_bindings(tx, &reg).expect("bind");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ActorKind, Automation, AutomationPictureOwner, AutomationRun};
    use crate::ops::automation::{self, EdgeTarget, NewAutomation};
    use crate::ops::automation_builtin::action;
    use crate::ops::automation_builtin_take::{NONE_TO_TAKE, TAKEN};
    use crate::ops::automation_report::Next;
    use crate::ops::automation_run::{launch_leaving_the_task_open as launch, nothing_asked, Launcher};
    use crate::ops::automation_step::Opened;
    use crate::ops::test_support::open;
    use crate::ops::test_support::{mk_placed, mk_project, mk_task_in, with_tx};
    use super::fixture::{bind, git, repositories};
    use crate::store_engine::WriteTx;

    /// Take a task, cut its worktree, and go on to an agent's step.
    fn picture(tx: &WriteTx<'_>, project: i64) -> Automation {
        let automation =
            automation::add(tx, project, NewAutomation { name: "cut".into(), ..Default::default() })
                .expect("automation");
        let on = AutomationPictureOwner::Automation;
        let take = automation::placement_add(tx, automation.id, action(tx, "take_task").expect("take").id)
            .expect("place take");
        let cut = automation::placement_add(tx, automation.id, action(tx, "cut_worktree").expect("cut").id)
            .expect("place cut");
        let (_, work) = mk_placed(tx, &automation, "work", "work on it", "claude");
        automation::edge_add(tx, on, take.id, Some(TAKEN), EdgeTarget::Go(cut.id), None).expect("take → cut");
        automation::edge_add(tx, on, take.id, Some(NONE_TO_TAKE), EdgeTarget::Done, None).expect("none");
        automation::edge_add(tx, on, cut.id, None, EdgeTarget::Go(work.id), None).expect("cut → work");
        automation::edge_add(tx, on, work.id, None, EdgeTarget::Done, None).expect("work → done");
        automation::set_entry(tx, automation.id, Some(take.id)).expect("entry")
    }

    /// Launch, take the task, and carry out the cut. Answers the run, the cut's execution and where the
    /// run went next.
    fn walk(tx: &WriteTx<'_>, automation: &Automation) -> (AutomationRun, i64, Next) {
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
        let Opened::Carried { next: Next::Step(cut), .. } = open(tx, run.id, entry.id, Some(&claude)).expect("take")
        else {
            panic!("the take goes on to the cut");
        };
        let Opened::Carried { run_step_id, next } = open(tx, run.id, cut.id, Some(&claude)).expect("cut") else {
            panic!("a built-in is carried out");
        };
        (run, run_step_id, next)
    }

    fn for_ai(tx: &WriteTx<'_>, project: i64) -> i64 {
        let id = mk_task_in(tx, "直すもの", Some(project));
        crate::ops::task::set_assignee(tx, id, Some(ActorKind::Ai)).expect("give it to the AI");
        id
    }

    /// **It cuts the task's worktree from the newest of the remote's default branch and hands the path
    /// on** — leaving the project's own checkout where it stood.
    #[test]
    fn it_cuts_from_the_remote_s_newest_and_hands_the_path_on() {
        with_tx(|tx| {
            let (app, newest) = repositories("builtin-cut");
            let before = git(&app, &["rev-parse", "HEAD"]);
            let project = mk_project(tx, "amenbo");
            bind(tx, project, &[&app]);
            let automation = picture(tx, project);
            let task = for_ai(tx, project);

            let (_, run_step_id, next) = walk(tx, &automation);
            assert!(matches!(next, Next::Step(_)), "it goes on to the work: {next:?}");
            let root = worktree_cut::git_root(&app).expect("root");
            let expected = worktree_cut::layout(&root, &task.to_string()).worktree;
            let handed: Vec<String> = read::automation_run_values_of(tx.conn(), run_step_id)
                .expect("values")
                .into_iter()
                .filter_map(|v| v.value)
                .collect();
            assert_eq!(handed, vec![expected.to_string_lossy().into_owned()]);
            assert_eq!(git(&expected, &["rev-parse", "HEAD"]), newest, "cut from what the remote has now");
            assert_eq!(git(&expected, &["rev-parse", "--abbrev-ref", "HEAD"]), format!("task/{task}"));
            assert_eq!(git(&app, &["rev-parse", "HEAD"]), before, "the project's checkout is not pulled");
            assert_eq!(git(&app, &["rev-parse", "--abbrev-ref", "HEAD"]), "main", "nor switched");
        });
    }

    /// **Inside the step's transaction it drives no git** — handed no work done beforehand, the step
    /// leaves by the error way out and nothing is cut.
    #[test]
    fn handed_nothing_done_beforehand_it_cuts_nothing() {
        with_tx(|tx| {
            let (app, _) = repositories("builtin-cut-inside");
            let project = mk_project(tx, "amenbo");
            bind(tx, project, &[&app]);
            let automation = picture(tx, project);
            let task = for_ai(tx, project);
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
            let Opened::Carried { next: Next::Step(cut), .. } = open(tx, run.id, entry.id, Some(&claude)).expect("take")
            else {
                panic!("the take goes on to the cut");
            };
            let opened = crate::ops::automation_step::open(tx, run.id, cut.id, Some(&claude), None).expect("cut");
            let Opened::Carried { next, .. } = opened else { panic!("a built-in is carried out") };
            assert!(matches!(next, Next::Halted(_)), "it leaves by the error way out: {next:?}");
            let root = worktree_cut::git_root(&app).expect("root");
            assert!(!worktree_cut::layout(&root, &task.to_string()).worktree.exists(), "nothing was cut");
        });
    }

    /// **A project with no folder in a repository has nothing to cut from**, and the step leaves by the
    /// error way out.
    #[test]
    fn with_no_repository_to_cut_from_the_step_fails() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            bind(tx, project, &[&amenbo_scratch::scratch("builtin-cut-nowhere")]);
            let automation = picture(tx, project);
            for_ai(tx, project);

            let (_, _, next) = walk(tx, &automation);
            assert!(matches!(next, Next::Halted(_)), "nothing follows the error way out: {next:?}");
        });
    }

    /// **Two repositories, and a task that names neither, is refused** rather than guessed at.
    #[test]
    fn a_project_in_two_repositories_is_not_guessed_at() {
        with_tx(|tx| {
            let (one, _) = repositories("builtin-cut-one");
            let (two, _) = repositories("builtin-cut-two");
            let project = mk_project(tx, "amenbo");
            bind(tx, project, &[&one, &two]);
            let err = repository(tx.conn(), project, None).expect_err("refused");
            assert!(err.to_string().contains("more than one repository"), "{err}");
        });
    }
}
