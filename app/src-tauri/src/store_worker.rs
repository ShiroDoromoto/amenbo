//! The one thread the commands that touch the store run on, in the order the page sent them.
//!
//! A synchronous command runs on the thread that draws the webview. While one of them reads or writes
//! SQLite, nothing on the screen moves — and most of that time is reads (`task_page`, `snapshot`), not
//! writes (`AMB-T-5647` holds the measurements). So every synchronous command that touches the store is
//! handed, as it arrives, to a queue that one thread works through, and the screen stays free.
//!
//! Three things hold this together, and each is easy to undo without seeing what it breaks:
//!
//! - **The commands stay synchronous, and the [`Invoke`](tauri::ipc::Invoke) itself is what is queued.** Written `async`, a
//!   command goes to tokio whole — its arguments are taken out of the message there too — and tokio
//!   does not run them in the order the page sent them (Tauri 2.11.6, `body_async` in
//!   `tauri-macros`). The page leans on that order: a value chosen and then "finish creating" pressed
//!   straight after is two commands that must run in that order. A queue fed from [`route`](crate::store_worker::route) runs them
//!   exactly as they came in, which is what the webview's thread did.
//! - **All of them move at once.** A read moved here while a write stayed behind could run before the
//!   write the page sent first, and the page would draw what it had just changed as unchanged. So
//!   [`ON_WORKER`](crate::store_worker::ON_WORKER) is every synchronous command that reaches the store, and the test below holds every
//!   registered command to one side of the line.
//! - **What is not moved stays where it was.** The `async` commands (the Viewer's, backup / restore /
//!   export, doctor, git's fetch / pull / push, …) already run off the webview's thread and some of
//!   them wait tens of seconds on the network; queued here they would hold every store command
//!   behind them. The threads that are not commands (`automation_watch`, the store watcher, the ends of
//!   a terminal) keep their own, so their order against the commands is what it was. And the watcher's
//!   connection, with `release_watch_for_swap` that restore calls through core, is never queued: a
//!   restore running here would wait on itself.
//!
//! A job that panics is answered with an error, and the thread goes on to the next one — a queue that
//! stopped would leave every store command after it unanswered. The app waits for the queue to empty
//! before it ends ([`drain`](crate::store_worker::drain)), so a write the page sent just before the quit is not dropped.

use std::collections::HashSet;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::time::Duration;

use tauri::ipc::Invoke;
use tauri::Runtime;

use crate::error::CmdError;

/// The synchronous commands that reach the store, directly or through what they call. They run on the
/// store thread, in the order they arrive.
pub const ON_WORKER: &[&str] = &[
    "snapshot", "automation_page", "automation_page_everywhere", "automation_add", "automation_edit",
    "automation_entry_replace", "automation_remove", "automation_action_page", "automation_action_add",
    "automation_action_edit", "automation_action_set_scope", "automation_action_remove",
    "automation_builtin_page", "automation_builtin_place", "automation_builtin_insert",
    "automation_placement_insert_new", "automation_placement_remove", "automation_step_edit",
    "automation_step_insert", "automation_step_add", "automation_step_remove",
    "automation_action_step_insert", "automation_action_entry_set", "automation_action_detail",
    "automation_output_add", "automation_edge_add", "automation_edge_edit", "automation_edge_remove",
    "automation_cfg_answer", "automation_placement_step_set", "automation_exit_declare",
    "automation_exit_rename", "automation_exit_remove", "automation_cfg_declare", "automation_cfg_edit",
    "automation_cfg_remove", "automation_input_declare", "automation_input_edit",
    "automation_input_remove", "automation_wire_set", "automation_wire_clear", "automation_detail",
    "automation_launch_check", "automation_launch", "automation_launch_asks", "automation_step_open",
    "automation_running_page", "automation_run_cards", "automation_history_page",
    "automation_run_acknowledge", "automation_run_pause", "automation_run_resume",
    "automation_run_stop", "store_signature", "version_status", "check_updates_fresh", "activity_page",
    "changes_since", "change_cursor", "task_activity", "task_page", "task_search", "tasks_by_ids",
    "decision_page", "decision_search", "search", "decisions_by_ids", "resolve_ref", "task_add",
    "task_finish_creating", "task_status", "task_done", "task_reject", "task_delete", "comment_add",
    "comment_remove", "comment_edit", "decision_comments", "decision_comment_add",
    "decision_comment_remove", "decision_comment_edit", "decision_add", "decision_finish_writing",
    "decision_reject", "decision_reopen", "decision_edit", "decision_supersede", "decision_amend",
    "decision_builds_on", "decision_unlink_edge", "decision_set_link", "decision_promote",
    "task_set_notes", "task_set_title", "task_set_priority", "task_set_due", "task_set_start",
    "project_add_folder", "project_get", "project_list_archived", "project_update", "project_move",
    "project_set_archived", "project_delete", "dimension_add", "dimension_rename", "dimension_set_slug",
    "dimension_update", "dimension_move", "dimension_rm", "dimension_value_add",
    "dimension_value_rename", "dimension_value_set_slug", "dimension_value_set_period",
    "dimension_value_set_closed", "dimension_value_move", "dimension_value_rm",
    "task_set_dimension_value", "task_unset_dimension_value", "task_dimensions",
    "decision_set_dimension_value", "decision_unset_dimension_value", "decision_dimensions",
    "project_dimension_assignments", "project_decision_dimension_assignments", "task_assign",
    "config_set_language", "set_facet_avatars", "set_facet_names", "read_receipts", "mark_task_seen",
    "mark_mailbox_seen", "mailbox_comment_tasks", "mailbox_triggered_at", "inbox_archived",
    "inbox_archive", "inbox_unarchive", "mailbox_notified_ids", "mailbox_notified_add",
    "pending_nudges", "mark_nudge_put", "attachments_for", "attachment_add", "attachment_open",
    "attachment_remove", "task_commits", "task_commit_add", "task_commit_remove", "task_made_in",
    "decision_made_in", "task_pane_opens_again", "decision_pane_opens_again", "project_bound_folders",
    "project_bind_folder", "project_unbind_folder", "folder_open", "stale_managed_blocks",
    "resync_managed_blocks", "orphan_bindings", "forget_orphan_bindings", "pointer_issues",
    "repair_pointers", "hook_offer", "hook_notices", "hook_answer", "handover_notice", "handover_told",
    "tick_banner", "tick_banner_later", "agent_hook_project_wiring", "agent_hook_requests",
    "agent_hook_answer", "agent_hook_consent", "agent_hook_consent_clear", "mcp_setup",
    "mcp_request_for", "mcp_bundle_write", "notify_targets", "notify_target_add", "notify_target_save",
    "notify_target_set_default", "notify_target_delete", "notify_target_check", "notify_target_test",
    "project_notify", "project_notify_set_enabled", "project_notify_set_mail_to",
    "project_notify_select_target", "project_notify_set_event", "viewer_state", "viewer_set_carrying",
    "wake_remember", "wake_chose", "wake_forget", "wake_rescan", "wake_register", "wake_amend",
    "wake_unregister", "wake_chose_model", "wake_forget_model", "name_frame", "project_memo",
    "set_project_memo", "talk_layout", "save_talk_layout", "folder_read", "folder_open_file",
    "folder_reveal_file", "folder_open_with", "folder_open_file_with", "folder_save", "folder_make",
    "folder_rename", "folder_move", "folder_copy", "folder_trash", "folder_import", "folder_clip_copy",
    "folder_clip_paste", "pty_open",
];

/// The commands that stay where Tauri puts them: the synchronous ones that never reach the store (the
/// terminal's keystrokes, the skins, the windows, the quit), and every `async` one, which runs on
/// tokio. Nothing routes on this list — what is not in [`ON_WORKER`] goes straight through — so only
/// the test that holds every registered command to one side reads it.
#[cfg_attr(not(test), allow(dead_code))]
pub const OFF_WORKER: &[&str] = &[
    "automation_steps_standing", "skin_add", "skin_font_licence", "skin_in_use", "skin_read",
    "skin_template_to", "skin_write_out", "skin_list", "skin_tables", "skin_use", "store_locations",
    "dev_badge", "cli_command_name", "open_logs_dir", "config_set_perf_log", "config_set_update_check",
    "config_set_default_view", "config_set_autostart", "notify_os", "cancel_data_op", "ui_language",
    "restart_app", "migration_status", "attachment_save", "reveal_folder", "open_terminal",
    "tick_answer", "open_latest_installer", "viewer_app", "wake_model", "wake_switch",
    "agent_default_model", "frame_names", "panes_without_a_way_back", "frame_on_model", "frame_model",
    "elevated", "show_ref", "show_ledger", "folder_git_askpass_said", "folder_encodings",
    "folder_unwatch", "folder_search_stop", "folder_untrash", "clip_files", "pty_sessions", "pty_close",
    "pty_attach", "pty_write", "pty_brief", "pty_rename", "pty_paste_image", "pty_resize", "talk_close",
    "talk_ready", "talk_raise", "app_quit", "quit_written",
    "run_backup", "run_restore", "run_export", "migration_retry", "doctor_report", "doctor_fix",
    "viewer_setup", "viewer_pairing", "viewer_pair_code", "viewer_cut_off", "viewer_send",
    "viewer_repair", "wake_probe", "wake_choices", "agent_models", "folder_entries",
    "folder_git_status", "folder_git_log", "folder_git_show", "folder_git_diff", "folder_git_tree_diff",
    "folder_git_branches", "folder_git_stashes", "folder_git_marks", "folder_git_stage",
    "folder_git_unstage", "folder_git_commit", "folder_git_restore", "folder_git_untrack",
    "folder_git_ignore", "folder_git_stash", "folder_git_stash_pop", "folder_git_switch",
    "folder_git_branch_create", "folder_git_merge", "folder_git_merge_continue",
    "folder_git_merge_abort", "folder_git_take", "folder_git_fetch", "folder_git_pull",
    "folder_git_push", "folder_names", "drop_effect", "folder_watch", "folder_search", "folder_replace",
    "talk_open",
];

/// One command, and what answers it if it panics before it has answered itself.
struct Job {
    run: Box<dyn FnOnce() + Send>,
    fell: Box<dyn FnOnce() + Send>,
}

/// The queue and the count of jobs not finished yet, which [`Queue::drain`] waits on.
struct Queue {
    tx: Mutex<Sender<Job>>,
    owed: Arc<(Mutex<usize>, Condvar)>,
}

/// A poisoned lock here guards a count or a sender, both still whole, so it is taken as it is.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Queue {
    fn start(delay: Duration) -> Queue {
        let (tx, rx) = channel::<Job>();
        let owed = Arc::new((Mutex::new(0usize), Condvar::new()));
        let left = owed.clone();
        std::thread::Builder::new()
            .name("store-worker".into())
            .spawn(move || {
                for job in rx {
                    if !delay.is_zero() {
                        std::thread::sleep(delay);
                    }
                    if catch_unwind(AssertUnwindSafe(job.run)).is_err() {
                        let _ = catch_unwind(AssertUnwindSafe(job.fell));
                    }
                    let (count, done) = &*left;
                    let mut n = lock(count);
                    *n -= 1;
                    if *n == 0 {
                        done.notify_all();
                    }
                }
            })
            .expect("the store thread starts");
        Queue { tx: Mutex::new(tx), owed }
    }

    fn submit(&self, job: Job) {
        *lock(&self.owed.0) += 1;
        // The thread never leaves its loop while this side holds the sender, so a send cannot fail;
        // were it to, the job is still answered rather than left hanging.
        if let Err(unsent) = lock(&self.tx).send(job) {
            *lock(&self.owed.0) -= 1;
            (unsent.0.fell)();
        }
    }

    /// Wait until every job handed over has finished, or `limit` has passed. `true` when it emptied.
    fn drain(&self, limit: Duration) -> bool {
        let (count, done) = &*self.owed;
        let (n, _) = done
            .wait_timeout_while(lock(count), limit, |n| *n > 0)
            .unwrap_or_else(|e| e.into_inner());
        *n == 0
    }
}

fn queue() -> &'static Queue {
    static QUEUE: OnceLock<Queue> = OnceLock::new();
    QUEUE.get_or_init(|| Queue::start(delay()))
}

/// Held back before each job in a development build ([`amenbo_core::env::dev_store_delay_ms`]) — the
/// way to see the screen stay free while the store is slow. Read once, and never in a release build.
fn delay() -> Duration {
    if !amenbo_core::config::Paths::is_dev_channel() {
        return Duration::ZERO;
    }
    amenbo_core::env::dev_store_delay_ms().map(Duration::from_millis).unwrap_or(Duration::ZERO)
}

fn on_worker(command: &str) -> bool {
    static SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
    SET.get_or_init(|| ON_WORKER.iter().copied().collect()).contains(command)
}

/// Wrap the handler `generate_handler!` builds: a command in [`ON_WORKER`] is queued for the store
/// thread, and every other one goes straight through, where it went before.
pub fn route<R: Runtime>(
    handler: impl Fn(Invoke<R>) -> bool + Send + Sync + 'static,
) -> impl Fn(Invoke<R>) -> bool + Send + Sync + 'static {
    let handler = Arc::new(handler);
    move |invoke: Invoke<R>| {
        if !on_worker(invoke.message.command()) {
            return (*handler)(invoke);
        }
        let command = invoke.message.command().to_string();
        let resolver = invoke.resolver.clone();
        let handler = handler.clone();
        queue().submit(Job {
            run: Box::new(move || {
                (*handler)(invoke);
            }),
            // A panic is this build failing, not the store answering no, so it has no code of its own
            // and no template: the reader gets the English and the log gets the panic's own line.
            fell: Box::new(move || {
                resolver.reject(CmdError::from(format!(
                    "the command '{command}' stopped partway, and its answer was lost"
                )))
            }),
        });
        true
    }
}

/// Wait, up to `limit`, for the store thread to finish what it has been handed. Called on the way out
/// of the app; never from the store thread itself, which would wait on its own job.
pub fn drain(limit: Duration) -> bool {
    queue().drain(limit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jobs_run_in_the_order_they_were_handed_over() {
        let q = Queue::start(Duration::ZERO);
        let seen = Arc::new(Mutex::new(Vec::new()));
        for i in 0..50 {
            let seen = seen.clone();
            q.submit(Job { run: Box::new(move || lock(&seen).push(i)), fell: Box::new(|| {}) });
        }
        assert!(q.drain(Duration::from_secs(5)));
        assert_eq!(*lock(&seen), (0..50).collect::<Vec<_>>());
    }

    #[test]
    fn a_job_that_panics_is_answered_and_the_next_one_still_runs() {
        let q = Queue::start(Duration::ZERO);
        let (tx, rx) = channel();
        let fell = tx.clone();
        q.submit(Job { run: Box::new(|| panic!("a job falls over")), fell: Box::new(move || fell.send("fell").unwrap()) });
        q.submit(Job { run: Box::new(move || tx.send("ran").unwrap()), fell: Box::new(|| {}) });
        assert!(q.drain(Duration::from_secs(5)));
        assert_eq!(rx.try_iter().collect::<Vec<_>>(), vec!["fell", "ran"]);
    }

    #[test]
    fn drain_gives_up_at_its_limit_and_empties_once_the_job_is_let_go() {
        let q = Queue::start(Duration::ZERO);
        let (release, held) = channel::<()>();
        q.submit(Job { run: Box::new(move || held.recv().unwrap()), fell: Box::new(|| {}) });
        assert!(!q.drain(Duration::from_millis(50)), "a job still running keeps the queue owed");
        release.send(()).unwrap();
        assert!(q.drain(Duration::from_secs(5)));
    }

    /// The names `generate_handler!` registers, read out of this crate's own `lib.rs`.
    fn registered() -> Vec<String> {
        let lib = include_str!("lib.rs");
        let start = lib.find("generate_handler![").expect("lib.rs registers its commands") + "generate_handler![".len();
        let end = start + lib[start..].find(']').expect("the list closes");
        lib[start..end]
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.rsplit("::").next().unwrap().to_string())
            .collect()
    }

    fn sources() -> Vec<String> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        std::fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|x| x == "rs"))
            .map(|e| std::fs::read_to_string(e.path()).unwrap())
            .collect()
    }

    fn is_async(sources: &[String], name: &str) -> bool {
        let needle = format!("async fn {name}(");
        let generic = format!("async fn {name}<");
        sources.iter().any(|s| s.contains(&needle) || s.contains(&generic))
    }

    /// The body of the first `fn <name>` found, braces matched. Rough, and enough for what it is asked.
    fn body<'a>(sources: &'a [String], name: &str) -> Option<&'a str> {
        for s in sources {
            for needle in [format!("fn {name}("), format!("fn {name}<")] {
                if let Some(at) = s.find(&needle) {
                    let open = at + s[at..].find('{')?;
                    let mut depth = 0usize;
                    for (i, c) in s[open..].char_indices() {
                        match c {
                            '{' => depth += 1,
                            '}' => {
                                depth -= 1;
                                if depth == 0 {
                                    return Some(&s[open..open + i]);
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        None
    }

    #[test]
    fn every_registered_command_is_on_one_side_of_the_line() {
        let on: HashSet<&str> = ON_WORKER.iter().copied().collect();
        let off: HashSet<&str> = OFF_WORKER.iter().copied().collect();
        assert_eq!(on.len(), ON_WORKER.len(), "a command is named twice in ON_WORKER");
        assert_eq!(off.len(), OFF_WORKER.len(), "a command is named twice in OFF_WORKER");
        let both: Vec<_> = on.intersection(&off).collect();
        assert!(both.is_empty(), "on both sides: {both:?}");
        let registered = registered();
        let unsorted: Vec<_> = registered.iter().filter(|c| !on.contains(c.as_str()) && !off.contains(c.as_str())).collect();
        assert!(unsorted.is_empty(), "registered, and on neither side — put each in ON_WORKER or OFF_WORKER: {unsorted:?}");
        let gone: Vec<_> = on.union(&off).filter(|c| !registered.iter().any(|r| r.as_str() == **c)).collect();
        assert!(gone.is_empty(), "named here, and not registered any more: {gone:?}");
    }

    #[test]
    fn no_async_command_is_queued() {
        let sources = sources();
        let queued: Vec<_> = ON_WORKER.iter().filter(|c| is_async(&sources, c)).collect();
        assert!(queued.is_empty(), "async commands run on tokio and must stay off the store thread: {queued:?}");
    }

    #[test]
    fn a_synchronous_command_that_opens_the_store_is_queued() {
        const OPENERS: [&str; 6] =
            ["open_store(", "open_store_read(", "with_store_mut(", "with_store_read(", "find_in_store(", "Store::open"];
        let sources = sources();
        let stray: Vec<_> = OFF_WORKER
            .iter()
            .filter(|c| !is_async(&sources, c))
            .filter(|c| body(&sources, c).is_some_and(|b| OPENERS.iter().any(|o| b.contains(o))))
            .collect();
        assert!(stray.is_empty(), "these open the store and belong in ON_WORKER: {stray:?}");
    }
}
