//! Native application menu. Application info (About / version) is served through the OS-native app
//! menu, and Windows gets a menu too — macOS had the default About, Windows had no route at all.
//! Built once at startup and shared by every window.
//!
//! On macOS the whole menu is replaced, so the standard app / Edit / Window submenus are rebuilt
//! here: dropping the Edit submenu would strip the webview of Cmd+C/V/X/A, which the default menu
//! had been providing. On Windows/Linux a single Help submenu carries About.

use tauri::menu::{
  AboutMetadataBuilder, Menu, MenuBuilder, MenuItem, PredefinedMenuItem, SubmenuBuilder,
};

/// Product site — the About dialog links here.
const WEBSITE: &str = "https://amenbo.work/";

/// Menu id of the manual "check for updates" item. `lib.rs`'s `on_menu_event` matches this and, on a
/// click, tells the webview to run a fresh check (which then shows the update banner or an "up to
/// date" note).
pub const CHECK_UPDATES_ID: &str = "check-updates";

/// Normalize `config.language` to "ja" or "en" (unset / unknown → ja), matching the GUI's own
/// fallback (`lang_code` in commands.rs). Kept local so the menu can label itself at build time
/// without opening the store.
fn lang_code(language: Option<&str>) -> &'static str {
  match language {
    Some(l) if l.starts_with("en") => "en",
    _ => "ja",
  }
}

/// Build the application menu. Reads `config.language` directly (a file outside the store, so no
/// version gate) to localize the labels, and stamps the running version + product site into the
/// About dialog's metadata.
pub fn build<R: tauri::Runtime>(handle: &tauri::AppHandle<R>) -> tauri::Result<Menu<R>> {
  let language = amenbo_core::config::Paths::resolve()
    .ok()
    .and_then(|p| amenbo_core::config::Config::load(&p.config_file).language);
  let lang = lang_code(language.as_deref());

  let about_meta = AboutMetadataBuilder::new()
    .version(Some(handle.package_info().version.to_string()))
    .website(Some(WEBSITE))
    .website_label(Some("amenbo.work"))
    .build();
  let about_text = if lang == "en" {
    "About amenbo"
  } else {
    "amenbo について"
  };
  let about = PredefinedMenuItem::about(handle, Some(about_text), Some(about_meta))?;

  // A manual, on-demand update check that sits with About (the version affordance). Its click is
  // handled in lib.rs; no accelerator (checking is a rare, deliberate action).
  let check_updates = MenuItem::with_id(
    handle,
    CHECK_UPDATES_ID,
    if lang == "en" { "Check for Updates" } else { "更新を確認" },
    true,
    None::<&str>,
  )?;

  #[cfg(target_os = "macos")]
  {
    // The first submenu becomes the application menu; macOS localizes the predefined items itself.
    let app_menu = SubmenuBuilder::new(handle, "amenbo")
      .item(&about)
      .separator()
      .item(&check_updates)
      .separator()
      .services()
      .separator()
      .hide()
      .hide_others()
      .show_all()
      .separator()
      .quit()
      .build()?;
    let edit_menu = SubmenuBuilder::new(handle, if lang == "en" { "Edit" } else { "編集" })
      .undo()
      .redo()
      .separator()
      .cut()
      .copy()
      .paste()
      .select_all()
      .build()?;
    let window_menu = SubmenuBuilder::new(handle, if lang == "en" { "Window" } else { "ウインドウ" })
      .minimize()
      .maximize()
      .separator()
      .close_window()
      .build()?;
    MenuBuilder::new(handle)
      .items(&[&app_menu, &edit_menu, &window_menu])
      .build()
  }
  #[cfg(not(target_os = "macos"))]
  {
    // A File submenu carries Exit so the menu bar is not a single lonely Help entry (macOS keeps
    // Quit under its app menu instead). Predefined quit exits the app; the label is localized.
    let quit = PredefinedMenuItem::quit(handle, Some(if lang == "en" { "Exit" } else { "終了" }))?;
    let file_menu = SubmenuBuilder::new(handle, if lang == "en" { "File" } else { "ファイル" })
      .item(&quit)
      .build()?;
    let help_menu = SubmenuBuilder::new(handle, if lang == "en" { "Help" } else { "ヘルプ" })
      .item(&about)
      .item(&check_updates)
      .build()?;
    MenuBuilder::new(handle)
      .items(&[&file_menu, &help_menu])
      .build()
  }
}
