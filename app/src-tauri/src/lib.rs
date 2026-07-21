// This crate is not published, and whoever reads its docs sees the private items too (the gate runs
// with `--document-private-items`). Linking from a public item to a private helper is therefore
// correct here; only a broken link should warn.
#![allow(rustdoc::private_intra_doc_links)]

mod blobproto;
mod commands;
mod error;
#[cfg(target_os = "macos")]
mod macos_notify;
mod menu;
/// Migration at startup. It runs through the same execution site as the CLI, ahead of anything that
/// opens the store: neither the watcher nor a command may read a store caught mid-version, and if
/// the CLI got there first we wait at the same lock — one path, mutually exclusive. The waiting
/// happens on a thread, not in `setup`: waiting in setup means no window is ever created, and a long
/// migration becomes an app that hangs in silence. So the window comes up first, as the migration
/// screen, while `migrate::gate()` blocks every path that would open the store.
mod migrate;
mod perf;
/// OS-specific file watching — the half that wakes `commands::watch_store`. It does not depend on
/// tauri, so the integration test (`tests/store_watch.rs`) can drive the real behaviour on all three
/// operating systems.
pub mod store_watch;
#[cfg(target_os = "windows")]
mod windows_notify;

/// Name of the custom protocol that streams content-addressed blobs to the webview. Rather than
/// inline a large file (audio, video, PDF) as a data URL, `<scheme>://localhost/<store>/<hash>?mime=…`
/// serves the bytes out of the blob store with Range support. The frontend hands the URL that
/// [`blobproto`] builds to the src of an img, audio, video or iframe element.
const BLOB_SCHEME: &str = "amenboblob";

/// Starts the long-lived threads that keep the store open. Call it **only once migration is
/// through**, so nothing ever reads a store caught mid-version or left at an old one — which is why
/// there are exactly two callers: startup (no migration needed, or migration succeeded) and a
/// successful `migration_retry`. Garbage-collecting device state (read receipts, inbox archive)
/// scans, so it goes to a thread of its own rather than hold up launch; a failure there is not fatal
/// and is only logged.
fn start_store_threads(app: tauri::AppHandle) {
  std::thread::spawn(move || commands::watch_store(app));
  std::thread::spawn(|| {
    if let Err(e) = commands::gc_device_state() {
      log::warn!("gc_device_state failed: {e}");
    }
  });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    .menu(menu::build)
    .register_asynchronous_uri_scheme_protocol(BLOB_SCHEME, |_ctx, request, responder| {
      // Keep the file IO (the Range read) off the webview and main threads.
      std::thread::spawn(move || responder.respond(blobproto::serve(&request)));
    })
    .setup(|app| {
      let config = amenbo_core::config::Paths::resolve()
        .map(|p| amenbo_core::config::Config::load(&p.config_file))
        .unwrap_or_default();
      perf::install(&config);
      if cfg!(debug_assertions) {
        app.handle().plugin(
          tauri_plugin_log::Builder::default()
            .level(log::LevelFilter::Info)
            .build(),
        )?;
      }
      // The folder picker ("open a folder" = bind to an existing store).
      app.handle().plugin(tauri_plugin_dialog::init())?;
      app.handle().plugin(tauri_plugin_notification::init())?;
      app.handle().plugin(tauri_plugin_opener::init())?;
      // GUI self-update (desktop only; the plugin has no mobile target). It only exposes check /
      // download+install to the front end — the apply is a user action from the update banner, and
      // minisign verification is mandatory (the pubkey lives in tauri.conf.json's `plugins.updater`).
      #[cfg(desktop)]
      app.handle().plugin(tauri_plugin_updater::Builder::new().build())?;
      #[cfg(target_os = "macos")]
      macos_notify::init(app.handle().clone());
      let handle = app.handle().clone();
      if migrate::is_pending() {
        migrate::begin(); // set before the window mounts, so the first `migration_status` says `running`.
        std::thread::spawn(move || {
          if migrate::run(&handle) {
            start_store_threads(handle);
          }
          // On failure this build cannot open this store, so the long-lived threads stay down: the
          // screen states why, and a successful "retry" starts them via `migration_retry`, the same way.
        });
      } else {
        start_store_threads(handle);
      }
      Ok(())
    })
    .invoke_handler(tauri::generate_handler![
      commands::snapshot,
      commands::store_signature,
      commands::version_status,
      commands::store_locations,
      commands::activity_page,
      commands::changes_since,
      commands::change_cursor,
      commands::task_activity,
      commands::task_page,
      commands::tasks_by_ids,
      commands::decision_page,
      commands::decisions_by_ids,
      commands::resolve_ref,
      commands::task_add,
      commands::task_status,
      commands::task_delete,
      commands::comment_add,
      commands::comment_remove,
      commands::comment_edit,
      commands::decision_comments,
      commands::decision_comment_add,
      commands::decision_comment_remove,
      commands::decision_comment_edit,
      commands::decision_add,
      commands::decision_accept,
      commands::decision_reject,
      commands::decision_reopen,
      commands::decision_edit,
      commands::decision_supersede,
      commands::decision_amend,
      commands::decision_builds_on,
      commands::decision_unlink_edge,
      commands::decision_set_link,
      commands::decision_promote,
      commands::task_set_notes,
      commands::task_set_title,
      commands::task_set_priority,
      commands::project_add,
      commands::project_add_folder,
      commands::project_get,
      commands::project_list_archived,
      commands::project_update,
      commands::project_move,
      commands::project_set_archived,
      commands::project_delete,
      commands::dimension_add,
      commands::dimension_rename,
      commands::dimension_update,
      commands::dimension_move,
      commands::dimension_rm,
      commands::dimension_value_add,
      commands::dimension_value_rename,
      commands::dimension_value_set_period,
      commands::dimension_value_move,
      commands::dimension_value_rm,
      commands::task_set_dimension_value,
      commands::task_unset_dimension_value,
      commands::task_dimensions,
      commands::project_dimension_assignments,
      commands::task_assign,
      commands::onboarding_save,
      commands::config_set_language,
      commands::config_set_perf_log,
      commands::config_set_update_check,
      commands::set_facet_avatars,
      commands::set_facet_names,
      commands::agent_spec,
      commands::read_receipts,
      commands::mark_task_seen,
      commands::mark_mailbox_seen,
      commands::mailbox_comment_tasks,
      commands::mailbox_triggered_at,
      commands::inbox_archived,
      commands::inbox_archive,
      commands::inbox_unarchive,
      commands::mailbox_notified_ids,
      commands::mailbox_notified_add,
      commands::notify_os,
      commands::run_backup,
      commands::run_restore,
      commands::run_export,
      commands::cancel_data_op,
      commands::ui_language,
      commands::restart_app,
      commands::migration_status,
      commands::migration_retry,
      commands::attachments_for,
      commands::attachment_add,
      commands::attachment_add_bytes,
      commands::attachment_open,
      commands::attachment_save,
      commands::attachment_remove,
      commands::task_commits,
      commands::task_commit_add,
      commands::task_commit_remove,
      commands::reveal_folder,
      commands::open_terminal,
      commands::project_bound_folders,
      commands::project_bind_folder,
      commands::project_unbind_folder,
      commands::stale_managed_blocks,
      commands::resync_managed_blocks,
      commands::orphan_bindings,
      commands::forget_orphan_bindings,
      commands::pointer_issues,
      commands::repair_pointers,
      commands::hook_offer,
      commands::hook_notices,
      commands::hook_answer,
      commands::doctor_report,
      commands::doctor_fix,
      commands::open_latest_installer,
    ])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
