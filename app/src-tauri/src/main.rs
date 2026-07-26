// Prevents an additional console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
  // Launched as a plugin runner rather than as the app (`AMB-T-2175`): work that one queue and exit, without
  // ever reaching the window. It is asked first because everything below it belongs to being an app.
  if app_lib::run_plugin_runner() {
    return;
  }
  app_lib::run();
}
