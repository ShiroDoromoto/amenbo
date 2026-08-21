// Prevents an additional console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
  // Before anything else, and before any thread exists: an inherited `TMPDIR` can name a directory the OS
  // has already removed — the .pkg installer relaunches the app from inside its own sandbox, and `open`
  // hands that environment straight through (`AMB-T-3461`). Disown it here, so neither the app nor any
  // plugin it starts is pointed at a path that is gone.
  amenbo_core::tmpdir::forget_if_gone();

  // Launched as a plugin runner rather than as the app (`AMB-T-2175`): work that one queue and exit, without
  // ever reaching the window. It is asked first because everything below it belongs to being an app.
  if app_lib::run_plugin_runner() {
    return;
  }
  app_lib::run();
}
