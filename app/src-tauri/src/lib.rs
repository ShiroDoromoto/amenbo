// This crate is not published, and whoever reads its docs sees the private items too (the gate runs
// with `--document-private-items`). Linking from a public item to a private helper is therefore
// correct here; only a broken link should warn.
#![allow(rustdoc::private_intra_doc_links)]

/// Start at login: the per-user registration each OS reads when the user signs in, and the one door
/// that writes or removes it (`AMB-D-541`).
mod autostart;
mod blobproto;
mod commands;
mod diag;
mod dto;
mod error;
/// What a terminal is started as on each operating system — the shell the user signed in with, and
/// what a terminal owes the program in it. Detecting a tool and starting it go through here
/// together, so a probe cannot find what the pane could not have started (`AMB-D-747`).
mod launch;
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
/// The long-lived mount of the plugin observation dispatcher: the drive the write seam runs after each
/// mutating command, over the store's own cursor (`AMB-D-380`), and the one this app makes as it comes up
/// for what a previous run left half delivered (`AMB-D-399`).
mod plugin_dispatch;
/// The pseudo-terminal a pane of the talk window is filled with: opening one, carrying its bytes
/// both ways, and telling it how large the pane on screen is (`AMB-D-747`).
mod pty;
/// One process per store: what turns a second launch into the window that is already open, coming to
/// the front. Desktop-only, because the claim it holds is an OS primitive with no shape elsewhere.
#[cfg(desktop)]
mod single_instance;
/// OS-specific file watching — the half that wakes `commands::watch_store`. It does not depend on
/// tauri, so the integration test (`tests/store_watch.rs`) can drive the real behaviour on all three
/// operating systems.
pub mod store_watch;
/// The hourly tick's startup pass: what this device answered about being woken, settled against what
/// its scheduler holds (`AMB-D-707`).
mod tick;
/// The labels of the two windows this app opens (board and talk), and which of them anything that
/// raises a window from outside the webview means.
mod windows;
#[cfg(target_os = "windows")]
mod windows_notify;

/// Name of the custom protocol that streams content-addressed blobs to the webview. Rather than
/// inline a large file (audio, video, PDF) as a data URL, `<scheme>://localhost/<store>/<hash>?mime=…`
/// serves the bytes out of the blob store with Range support. The frontend hands the URL that
/// [`blobproto`] builds to the src of an img, audio, video or iframe element.
const BLOB_SCHEME: &str = "amenboblob";

/// Emitted to the webview when the user picks "check for updates" from the app menu
/// (`menu::CHECK_UPDATES_ID`). The front end runs a fresh check and shows the update banner, or an
/// "up to date" note when there is nothing newer. Nothing is checked in the menu handler itself —
/// the network call belongs on the UI side, where its progress and result can be shown.
const CHECK_UPDATES_EVENT: &str = "menu://check-updates";

/// Starts the long-lived threads that keep the store open. Call it **only once migration is
/// through**, so nothing ever reads a store caught mid-version or left at an old one — which is why
/// there are exactly two callers: startup (no migration needed, or migration succeeded) and a
/// successful `migration_retry` — so this launch is tallied exactly once however the store was
/// reached. Device-local housekeeping (tallying the launch, then garbage-collecting read receipts and
/// the inbox archive) opens the store and scans, so it goes to a thread of its own rather than hold up
/// launch; a failure there is not fatal and is only logged.
fn start_store_threads(app: tauri::AppHandle) {
  std::thread::spawn(move || commands::watch_store(app));
  std::thread::spawn(|| {
    // One thread for both, in this order: they open the same store, so running them in turn is what
    // keeps them from contending for it — and the tally is what the nudges are judged on (`AMB-D-542`),
    // so it is written before anything reads it.
    if let Err(e) = commands::record_launch() {
      log::warn!("record_launch failed: {e}");
    }
    if let Err(e) = commands::gc_device_state() {
      log::warn!("gc_device_state failed: {e}");
    }
  });
  // What a previous run left half-delivered (`AMB-D-399`). Started with the other store threads, and after
  // a migration for the same reason they are: the store this reads is the migrated one or none at all.
  std::thread::spawn(plugin_dispatch::resume);
}

/// The plugin runner this process was launched as, if it was (`AMB-T-2175`): the plugin whose queue to work,
/// the lease taken on its behalf, and the store to work — read off `argv` in the order core appends them,
/// behind [`plugin_dispatch::RUNNER_FLAG`].
///
/// The match is exact and positional (the flag first, then exactly [`plugin_dispatch::RUNNER_ARGS`]
/// arguments), so nothing an operating system adds of its own — macOS's `-psn_…` on a launched app, say —
/// can be mistaken for it.
fn runner_argv() -> Option<(String, String, String)> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 + plugin_dispatch::RUNNER_ARGS || args[1] != plugin_dispatch::RUNNER_FLAG {
        return None;
    }
    Some((args[2].clone(), args[3].clone(), args[4].clone()))
}

/// Work one plugin's observation-event queue and return, instead of starting the app — what this executable
/// does when Amenbo launched it as a runner (`AMB-D-399`, `AMB-T-2175`). `true` when that is what happened,
/// which is the caller's signal to start nothing else: no window, no watcher, no migration screen.
///
/// The app is the runner for the events it queued itself, because a runner is *this same executable, re-run*:
/// one binary per face, and no second one to ship or to keep in step. Nothing is drawn and nothing is
/// reported — what each run did lands in the plugin execution log (`AMB-D-361`).
#[must_use = "start the app only when this says the process was not launched as a runner"]
pub fn run_plugin_runner() -> bool {
    let Some((plugin, owner, store)) = runner_argv() else {
        return false;
    };
    amenbo_core::plugin_runner::run_process(store.into(), &plugin, &owner);
    true
}

/// The `platforms` key this build asks the update manifest for — the machine's, not the build's
/// (`AMB-D-551`).
///
/// Left to itself the plugin keys the lookup on the architecture it was compiled for, so a build
/// running under emulation asks for its own kind forever and the machine never comes off the
/// translation layer. Only the architecture is asked of the machine, through the same door the CLI
/// uses ([`amenbo_core::update_check::native_arch`]); the operating system half has no such
/// question, no one running a Windows build on a Mac.
///
/// The vocabulary here is the updater plugin's, which is neither Amenbo's nor wharfy's: `darwin`
/// for macOS, and `aarch64` / `x86_64` where wharfy writes `arm64` / `x64`. Naming the key outright
/// also settles which one is read — the plugin otherwise tries a bundle-type-suffixed key first,
/// and the manifest Amenbo publishes (`scripts/build-tauri-manifest.sh`) has only the plain ones.
/// `None` is "nothing to say": an operating system or an architecture with no name in that
/// vocabulary leaves the plugin on its own default, which is what it had before this.
#[cfg(desktop)]
fn updater_target() -> Option<String> {
    let os = if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        return None;
    };
    let arch = match amenbo_core::update_check::native_arch() {
        "arm64" => "aarch64",
        "x64" => "x86_64",
        _ => return None,
    };
    Some(format!("{os}-{arch}"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  // Read once and handed to `run` at the end: the claim below needs the bundle identifier while the
  // app is still being built, and this is where that name lives (the build may override it — a
  // theme's preview does).
  let context = tauri::generate_context!();
  #[allow(unused_mut)]
  let mut builder = tauri::Builder::default();
  // One process per store, claimed before anything else this app does: a builder plugin is
  // initialized while the app is still being built, so a launch that finds the claim taken ends
  // before a window, a watcher or a migration screen exists to be doubled (`single_instance`).
  #[cfg(desktop)]
  if single_instance::guards_this_run() {
    builder = builder.plugin(single_instance::init(&context.config().identifier));
  }
  builder
    // The open terminals, held for the life of the app rather than of any command that touches one:
    // what a pane types into and what drains its output both reach for the same session, from
    // different threads and long after the call that opened it returned (`pty`).
    .manage(pty::Terminals::default())
    .menu(menu::build)
    .on_menu_event(|app, event| {
      if event.id() == menu::CHECK_UPDATES_ID {
        use tauri::Emitter;
        let _ = app.emit(CHECK_UPDATES_EVENT, ());
      }
    })
    .register_asynchronous_uri_scheme_protocol(BLOB_SCHEME, |_ctx, request, responder| {
      // Keep the file IO (the Range read) off the webview and main threads.
      std::thread::spawn(move || responder.respond(blobproto::serve(&request)));
    })
    .setup(|app| {
      let config = amenbo_core::config::Paths::resolve()
        .map(|p| amenbo_core::config::Config::load(&p.config_file))
        .unwrap_or_default();
      perf::install(&config);
      // What an earlier run left in the temporary directory when it ended without closing its terminals.
      // Off the launch path: it is a scan of a directory, and nothing here waits on it.
      std::thread::spawn(pty::sweep);
      // The diagnostic log (`AMB-D-382`), in every build — see the `diag` module for what it may hold and why
      // its size is bounded. A logger that cannot start is not a reason to refuse to start the app, so
      // the error is dropped rather than raised: there is nowhere left to report it to anyway.
      let _ = app.handle().plugin(diag::logger().build());
      // The folder picker ("open a folder" = bind to an existing store).
      app.handle().plugin(tauri_plugin_dialog::init())?;
      app.handle().plugin(tauri_plugin_notification::init())?;
      app.handle().plugin(tauri_plugin_opener::init())?;
      // GUI self-update (desktop only; the plugin has no mobile target). It only exposes check /
      // download+install to the front end — the apply is a user action from the update banner, and
      // minisign verification is mandatory (the pubkey lives in tauri.conf.json's `plugins.updater`).
      //
      // A development build does not get it. The endpoint compiled in is production's manifest, and
      // a dev build is normally behind what that manifest names, so an apply would overwrite the
      // bundle under test with the production one. Not registering the plugin is the half that fails
      // closed no matter who asks: the front end is never offered the update either (`upstream_release`
      // in commands.rs), and neither half depends on the other holding.
      //
      // What it is aimed at is the machine rather than this build (`updater_target`), so an Amenbo
      // running under emulation lands on the native one the next time the user applies an update,
      // instead of fetching its own kind forever.
      #[cfg(desktop)]
      if !amenbo_core::config::Paths::is_dev_channel() {
        let mut updater = tauri_plugin_updater::Builder::new();
        if let Some(target) = updater_target() {
          updater = updater.target(target);
        }
        app.handle().plugin(updater.build())?;
      }
      // Start at login (`AMB-D-541`). A development build does not get it either, and for a reason of
      // its own: the executable it would register sits in a working tree that gets rebuilt and thrown
      // away, so the registration outlives what it points at and tries to start a file that is gone
      // (`AMB-D-547`). Not registering the plugin is again the half that fails closed — the settings
      // screen withholds the switch, `autostart::set` refuses, and neither leans on the other.
      #[cfg(desktop)]
      if !amenbo_core::config::Paths::is_dev_channel() {
        app.handle().plugin(autostart::init())?;
        // The two states drift between runs — the registration is a file the user can delete, and it
        // holds a path that stops naming this executable once the app is moved — so they are settled
        // here, before the window can draw a switch out of a setting the login no longer honours.
        autostart::reconcile(app.handle());
      }
      // The hourly tick's own two states, settled the same way and on the same occasion. Unlike the
      // login registration it is not withheld from a development build: nothing is registered that was
      // not asked for, so a build that was asked is the one that has to tidy up after itself. On macOS
      // this is the only face that can make the pass at all — see the module docs.
      tick::reconcile();
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
      commands::check_updates_fresh,
      commands::store_locations,
      commands::dev_badge,
      commands::cli_command_name,
      commands::open_logs_dir,
      commands::activity_page,
      commands::changes_since,
      commands::change_cursor,
      commands::task_activity,
      commands::task_page,
      commands::task_search,
      commands::tasks_by_ids,
      commands::decision_page,
      commands::decision_search,
      commands::search,
      commands::decisions_by_ids,
      commands::resolve_ref,
      commands::task_add,
      commands::task_finish_creating,
      commands::task_status,
      commands::task_reject,
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
      commands::task_set_due,
      commands::task_set_start,
      commands::project_add_folder,
      commands::project_get,
      commands::project_list_archived,
      commands::project_update,
      commands::project_move,
      commands::project_set_archived,
      commands::project_delete,
      commands::dimension_add,
      commands::dimension_rename,
      commands::dimension_set_slug,
      commands::dimension_update,
      commands::dimension_move,
      commands::dimension_rm,
      commands::dimension_value_add,
      commands::dimension_value_rename,
      commands::dimension_value_set_slug,
      commands::dimension_value_set_period,
      commands::dimension_value_move,
      commands::dimension_value_rm,
      commands::task_set_dimension_value,
      commands::task_unset_dimension_value,
      commands::task_dimensions,
      commands::project_dimension_assignments,
      commands::task_assign,
      commands::config_set_language,
      commands::config_set_perf_log,
      commands::config_set_update_check,
      commands::config_set_default_view,
      commands::config_set_autostart,
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
      commands::pending_nudges,
      commands::mark_nudge_put,
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
      commands::tick_banner,
      commands::tick_answer,
      commands::tick_banner_later,
      commands::agent_hook_project_wiring,
      commands::agent_hook_requests,
      commands::agent_hook_answer,
      commands::agent_hook_consent,
      commands::agent_hook_consent_clear,
      commands::mcp_setup,
      commands::mcp_request_for,
      commands::mcp_bundle_write,
      commands::doctor_report,
      commands::doctor_fix,
      commands::open_latest_installer,
      commands::plugin_catalog_browse,
      commands::plugin_catalog_probe_source,
      commands::plugin_catalog_add_source,
      commands::plugin_catalog_remove_source,
      commands::plugin_repo_facts,
      commands::plugin_detail,
      commands::plugin_installs,
      commands::plugin_install,
      commands::plugin_set_enabled,
      commands::plugin_config_read,
      commands::plugin_config_set,
      commands::plugin_settings_check,
      commands::plugin_settings_action,
      commands::plugin_uninstall,
      commands::plugin_updates,
      commands::plugin_update_apply,
      commands::plugin_update_apply_all,
      commands::frame_names,
      commands::name_frame,
      pty::pty_open,
      pty::pty_write,
      pty::pty_resize,
      pty::pty_close,
    ])
    .run(context)
    .expect("error while running tauri application");
}

#[cfg(all(test, desktop))]
mod tests {
    /// The keys `scripts/build-tauri-manifest.sh` writes into the `platforms` object of every
    /// release. A target naming anything else finds no row and the update simply never arrives, so
    /// the mapping is worth holding against the list it has to hit.
    const PUBLISHED: [&str; 6] = [
        "darwin-aarch64",
        "darwin-x86_64",
        "windows-x86_64",
        "windows-aarch64",
        "linux-x86_64",
        "linux-aarch64",
    ];

    /// Whatever machine this runs on, the key it asks for is one the release actually carries.
    #[test]
    fn updater_target_names_a_published_platform() {
        let target = super::updater_target().expect("a machine Amenbo is built for names a target");
        assert!(
            PUBLISHED.contains(&target.as_str()),
            "the machine answered `{target}`, which no release publishes"
        );
    }
}
