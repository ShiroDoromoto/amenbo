//! **The built-in that folds a worktree away** — the task's checkout and its branch go, as
//! `worktree finish` takes them (`AMB-D-964`).
//!
//! What a prompt used to be asked to do by finding the main repository, `cd`-ing into it and typing
//! `worktree finish`, done here where the step is opened.
//!
//! **Measured against the remote's default branch, fetched fresh.** The work reaches the trunk through
//! the remote — a pull request merged there — and the main worktree is not pulled by anything in a run
//! ([`super::automation_builtin_cut`] cuts from the remote for the same reason). Measured against the
//! local branch, work merged a minute ago would read as unmerged until somebody pulled.
//!
//! **Unmerged is a way out, not a failure.** The fold refuses while the branch carries changes the trunk
//! does not have, and whether that means "wait", "go back and merge" or "call a person" is the picture's
//! to say: it leaves by [`UNMERGED`], and whoever built it draws where that goes. Anything else that
//! stops it, uncommitted changes included, leaves by the error way out.
//!
//! **Or it discards them**, where [`WHEN_UNMERGED`] is answered [`DISCARD`]: the work committed on the
//! branch and the changes left uncommitted go with the checkout, and it leaves by the way out it takes
//! when there was nothing to keep. Whether a branch reached the trunk and whether it may be folded are
//! two questions — a task that only looks into something leaves nothing for the trunk to take. Left
//! unanswered it discards nothing, so only a picture whose builder chose to throws work away.
//!
//! **Done before the step's transaction** ([`Work::Outside`]), as the cut is: the fetch waits on the
//! remote.

use crate::error::{Error, Result};
use crate::model::{AutomationCfgKind, DONE_EXIT};
use crate::ops::automation_builtin::{answer, Builtin, BuiltinExit, BuiltinSetting, Outside, Work, Worked};
use crate::ops::automation_builtin_cut::{refused, repository};
use crate::run_wording::builtin as say;
use crate::store_engine::read;
use crate::worktree_cut::{self, Refusal};

/// The way out it leaves by when the branch carries changes the trunk does not have.
pub const UNMERGED: &str = "未マージ";
/// The setting that says what it does with changes the trunk does not have.
pub const WHEN_UNMERGED: &str = "既定ブランチに無い変更があるとき";
/// The choice on [`WHEN_UNMERGED`] that keeps them — also what it does left unanswered.
pub const KEEP: &str = "捨てずに残す";
/// The choice on [`WHEN_UNMERGED`] that discards them, committed or not, and folds.
pub const DISCARD: &str = "コミットしていない変更も含めて捨てて畳む";

pub(super) const FOLD_WORKTREE: Builtin = Builtin {
    key: "fold_worktree",
    version: 2,
    name: "worktree を畳む",
    does: "いま扱っているタスクの worktree とブランチを片付ける。リモートの既定ブランチにまだ入っていない変更があれば、畳まずに「未マージ」から出る。設定で捨てると決めてあれば、その変更も捨てて畳む",
    steps: &[
        "いま扱っているタスクのリポジトリを、「worktree を切る」と同じ決め方で決める",
        "リモートを fetch し、ブランチ task/<タスクの番号> をリモートの既定ブランチの最新と比べる",
        "既定ブランチに無い変更があれば、畳まずに「未マージ」から出る。無ければ、worktree とブランチを消して「完了」から出る。設定で捨てると決めてあれば、コミット済みの変更もコミットしていない変更も捨てて、「完了」から出る",
    ],
    halts: &[
        "この実行がまだタスクを扱っていない",
        "タスクに作業フォルダが無く、このプロジェクトに紐付けたフォルダが2つ以上のリポジトリにまたがっている。どのリポジトリで作業するかが決まらない",
        "このプロジェクトに紐付けたフォルダが、どれも git リポジトリに入っていない",
        "タスクの作業フォルダが、git リポジトリに入っていない",
        "このタスクの worktree が無い",
        "worktree にコミットしていない変更があり、設定で捨てると決めていない",
        "このマシンに git が無い",
        "git が失敗した（リモートを fetch できないときなど）",
    ],
    settings: &[BuiltinSetting {
        name: WHEN_UNMERGED,
        kind: AutomationCfgKind::Choice,
        required: false,
        options: Some(r#"["捨てずに残す","コミットしていない変更も含めて捨てて畳む"]"#),
    }],
    ins: &[],
    exits: &[BuiltinExit { name: DONE_EXIT, outs: &[] }, BuiltinExit { name: UNMERGED, outs: &[] }],
    waits: None,
    chooses: None,
    work: Work::Outside(fold),
};

fn fold(outside: &Outside<'_>) -> Result<Worked> {
    let lang = outside.language;
    let task_id = outside.task_id.ok_or_else(|| Error::invalid(say(lang, "noTaskToFold", &[])))?;
    let task = read::task(outside.conn, task_id)?
        .ok_or_else(|| Error::not_found(format!("task AMB-T-{task_id}")))?;
    let root = repository(outside.conn, lang, outside.run.project_id, task.at_binding_id)?;
    let cut = worktree_cut::layout(&root, &task_id.to_string());
    let base = worktree_cut::origin_default(&root).map_err(|r| refused(lang, r))?;
    let discard = answer(outside.cfg, WHEN_UNMERGED)
        .and_then(|value| serde_json::from_str::<String>(value).ok())
        .is_some_and(|value| value == DISCARD);
    match worktree_cut::finish(&cut, Some(&base), discard) {
        Ok(_) => Ok(Worked {
            exit: DONE_EXIT,
            report: say(lang, "folded", &[("path", &cut.worktree.display().to_string()), ("branch", &cut.branch)]),
            hands: Vec::new(),
        }),
        Err(Refusal::Unmerged { branch, base }) => Ok(Worked {
            exit: UNMERGED,
            report: say(lang, "unmerged", &[("branch", &branch), ("base", &base)]),
            hands: Vec::new(),
        }),
        Err(other) => Err(refused(lang, other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ActorKind, Automation, AutomationPictureOwner};
    use crate::ops::automation::{self, EdgeTarget, NewAutomation};
    use crate::ops::automation_builtin::action;
    use crate::ops::automation_builtin_cut::fixture::{bind, git, repositories};
    use crate::ops::automation_builtin_take::{NONE_TO_TAKE, TAKEN};
    use crate::ops::automation_report::Next;
    use crate::ops::automation_run::{launch_past_the_task_checks as launch, nothing_asked, Launcher};
    use crate::ops::automation_step::Opened;
    use crate::ops::test_support::open;
    use crate::ops::test_support::{mk_project, mk_task_in, way_out, with_tx};
    use crate::store_engine::WriteTx;
    use std::path::{Path, PathBuf};

    /// Take a task and fold its worktree, with [`WHEN_UNMERGED`] answered as `when_unmerged` says.
    /// Either way out ends the run: what follows is not what is being tested.
    fn picture(tx: &WriteTx<'_>, project: i64, when_unmerged: Option<&str>) -> Automation {
        let automation =
            automation::add(tx, project, NewAutomation { name: "fold".into(), ..Default::default() })
                .expect("automation");
        let on = AutomationPictureOwner::Automation;
        let take = automation::placement_add(tx, automation.id, action(tx, "take_task").expect("take").id)
            .expect("place take");
        let fold = automation::placement_add(tx, automation.id, action(tx, "fold_worktree").expect("fold").id)
            .expect("place fold");
        if let Some(choice) = when_unmerged {
            let value = serde_json::to_string(choice).expect("a JSON string");
            automation::cfg_set(tx, fold.id, WHEN_UNMERGED, Some(&value)).expect("answer the setting");
        }
        automation::edge_add(tx, on, take.id, Some(TAKEN), EdgeTarget::Go(fold.id), None).expect("take → fold");
        automation::edge_add(tx, on, take.id, Some(NONE_TO_TAKE), EdgeTarget::Done, None).expect("none");
        automation::edge_add(tx, on, fold.id, None, EdgeTarget::Done, None).expect("folded");
        automation::edge_add(tx, on, fold.id, Some(UNMERGED), EdgeTarget::Done, None).expect("unmerged");
        automation::set_entry(tx, automation.id, Some(take.id)).expect("entry")
    }

    /// A project bound to a fresh clone, a task for the AI in it, and that task's worktree cut with one
    /// commit of its own on the branch. Answers the clone, the task and the worktree.
    fn a_task_with_work(tx: &WriteTx<'_>, tag: &str, when_unmerged: Option<&str>) -> (Automation, PathBuf, i64, PathBuf) {
        let (app, _) = repositories(tag);
        let project = mk_project(tx, "amenbo");
        bind(tx, project, &[&app]);
        let automation = picture(tx, project, when_unmerged);
        let task = mk_task_in(tx, "直すもの", Some(project));
        crate::ops::task::set_assignee(tx, task, Some(ActorKind::Ai), ActorKind::Ai).expect("give it to the AI");
        let root = worktree_cut::git_root(&app).expect("root");
        let cut = worktree_cut::layout(&root, &task.to_string());
        worktree_cut::start_from_origin(&cut).expect("cut");
        git(&cut.worktree, &["commit", "--quiet", "--allow-empty", "-m", "the work"]);
        (automation, app, task, cut.worktree)
    }

    /// Launch, take the task, and carry out the fold. Answers its execution and where the run went.
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
        let Opened::Carried { next: Next::Step(fold), .. } = open(tx, run.id, entry.id, Some(&claude)).expect("take")
        else {
            panic!("the take goes on to the fold");
        };
        let Opened::Carried { run_step_id, next } = open(tx, run.id, fold.id, Some(&claude)).expect("fold") else {
            panic!("a built-in is carried out");
        };
        (run_step_id, next)
    }

    fn branch_there(app: &Path, task: i64) -> bool {
        !git(app, &["branch", "--list", &format!("task/{task}")]).is_empty()
    }

    /// **Work that reached the remote's trunk is folded away** — measured against the remote, though
    /// the project's own `main` was never pulled.
    #[test]
    fn work_merged_on_the_remote_is_folded_away() {
        with_tx(|tx| {
            let (automation, app, task, worktree) = a_task_with_work(tx, "builtin-fold-merged", None);
            // Merged the way a pull request lands it: on the remote, and nowhere else.
            git(&worktree, &["push", "--quiet", "origin", &format!("task/{task}:main")]);

            let (run_step_id, _) = walk(tx, &automation);
            let ran = read::automation_run_step(tx.conn(), run_step_id).expect("read").expect("row");
            assert_ne!(ran.exit_id, way_out(tx, run_step_id, UNMERGED), "{}", ran.report);
            assert!(!worktree.exists(), "the checkout is gone");
            assert!(!branch_there(&app, task), "and so is its branch");
        });
    }

    /// **Work the trunk does not have leaves by [`UNMERGED`]**, with the checkout and the branch left
    /// standing.
    #[test]
    fn work_not_merged_leaves_by_the_way_out_that_says_so() {
        with_tx(|tx| {
            let (automation, app, task, worktree) = a_task_with_work(tx, "builtin-fold-unmerged", None);

            let (run_step_id, _) = walk(tx, &automation);
            let ran = read::automation_run_step(tx.conn(), run_step_id).expect("read").expect("row");
            assert_eq!(ran.exit_id, way_out(tx, run_step_id, UNMERGED), "{}", ran.report);
            assert!(worktree.exists(), "the checkout is left standing");
            assert!(branch_there(&app, task), "and so is its branch");
        });
    }

    /// **Answered [`DISCARD`], work the trunk does not have is discarded and folded** — the commit on
    /// the branch and a change never committed alike — and it leaves by the way out it takes when
    /// there was nothing to keep.
    #[test]
    fn work_not_merged_is_discarded_where_the_setting_says_so() {
        with_tx(|tx| {
            let (automation, app, task, worktree) =
                a_task_with_work(tx, "builtin-fold-discard", Some(DISCARD));
            std::fs::write(worktree.join("uncommitted.txt"), "never committed").expect("an uncommitted change");

            let (run_step_id, _) = walk(tx, &automation);
            let ran = read::automation_run_step(tx.conn(), run_step_id).expect("read").expect("row");
            assert_eq!(ran.exit_id, way_out(tx, run_step_id, DONE_EXIT), "{}", ran.report);
            assert!(!worktree.exists(), "the checkout is gone");
            assert!(!branch_there(&app, task), "and so is its branch");
        });
    }

    /// **Answered [`KEEP`], it is folded as if left unanswered**: the work stays, and it leaves by
    /// [`UNMERGED`].
    #[test]
    fn work_not_merged_is_kept_where_the_setting_says_so() {
        with_tx(|tx| {
            let (automation, app, task, worktree) = a_task_with_work(tx, "builtin-fold-keep", Some(KEEP));

            let (run_step_id, _) = walk(tx, &automation);
            let ran = read::automation_run_step(tx.conn(), run_step_id).expect("read").expect("row");
            assert_eq!(ran.exit_id, way_out(tx, run_step_id, UNMERGED), "{}", ran.report);
            assert!(worktree.exists(), "the checkout is left standing");
            assert!(branch_there(&app, task), "and so is its branch");
        });
    }

    /// The choices written out on the setting are the two the code reads, the one it does unanswered
    /// first.
    #[test]
    fn the_choices_offered_are_the_ones_it_reads() {
        let options = FOLD_WORKTREE.settings.iter().find(|s| s.name == WHEN_UNMERGED).and_then(|s| s.options);
        let offered: Vec<String> = serde_json::from_str(options.expect("a choice list")).expect("JSON");
        assert_eq!(offered, vec![KEEP, DISCARD]);
    }
}
