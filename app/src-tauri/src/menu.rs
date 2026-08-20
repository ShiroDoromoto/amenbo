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

/// The words the native menu needs, in one language.
///
/// This is the whole of the exception carved out in `AMB-D-396`: everywhere else the words live in
/// the webview's dictionary, but the menu bar is assembled before the webview runs, so these seven
/// have to be reachable from Rust. Seven is the ceiling — anything the OS labels for us (Quit,
/// Copy, Minimize on macOS) is not here, and nothing else should join them.
///
/// Which fields are read depends on the platform: macOS builds the app / Edit / Window submenus and
/// lets the OS label Quit, while Windows and Linux build File / Help and label Exit themselves. All
/// seven stay in one table anyway, because the unit a translator works in is a language, not a
/// platform — splitting them would mean writing one language's menu in two places.
#[allow(dead_code)]
struct Labels {
  about: &'static str,
  check_updates: &'static str,
  edit: &'static str,
  window: &'static str,
  file: &'static str,
  help: &'static str,
  exit: &'static str,
}

/// The nineteen languages of `AMB-D-394`, each written the way that language's own menu bars are.
/// A term is the platform's conventional one where there is one — `Édition` and not `Modifier`,
/// `Правка` and not `Редактировать` — because a menu that translates literally reads as foreign
/// even when every word is right.
const EN: Labels = Labels {
  about: "About Amenbo", check_updates: "Check for Updates",
  edit: "Edit", window: "Window", file: "File", help: "Help", exit: "Exit",
};
const JA: Labels = Labels {
  about: "Amenbo について", check_updates: "更新を確認",
  edit: "編集", window: "ウインドウ", file: "ファイル", help: "ヘルプ", exit: "終了",
};
const ZH_HANS: Labels = Labels {
  about: "关于 Amenbo", check_updates: "检查更新",
  edit: "编辑", window: "窗口", file: "文件", help: "帮助", exit: "退出",
};
const ZH_HANT: Labels = Labels {
  about: "關於 Amenbo", check_updates: "檢查更新",
  edit: "編輯", window: "視窗", file: "檔案", help: "說明", exit: "結束",
};
const KO: Labels = Labels {
  about: "Amenbo 정보", check_updates: "업데이트 확인",
  edit: "편집", window: "윈도우", file: "파일", help: "도움말", exit: "종료",
};
const ES: Labels = Labels {
  about: "Acerca de Amenbo", check_updates: "Buscar actualizaciones",
  edit: "Edición", window: "Ventana", file: "Archivo", help: "Ayuda", exit: "Salir",
};
const PT_BR: Labels = Labels {
  about: "Sobre o Amenbo", check_updates: "Verificar atualizações",
  edit: "Editar", window: "Janela", file: "Arquivo", help: "Ajuda", exit: "Sair",
};
const FR: Labels = Labels {
  about: "À propos d’Amenbo", check_updates: "Rechercher les mises à jour",
  edit: "Édition", window: "Fenêtre", file: "Fichier", help: "Aide", exit: "Quitter",
};
const DE: Labels = Labels {
  about: "Über Amenbo", check_updates: "Nach Updates suchen",
  edit: "Bearbeiten", window: "Fenster", file: "Datei", help: "Hilfe", exit: "Beenden",
};
const IT: Labels = Labels {
  about: "Informazioni su Amenbo", check_updates: "Verifica aggiornamenti",
  edit: "Modifica", window: "Finestra", file: "File", help: "Aiuto", exit: "Esci",
};
const RU: Labels = Labels {
  about: "О программе Amenbo", check_updates: "Проверить обновления",
  edit: "Правка", window: "Окно", file: "Файл", help: "Справка", exit: "Выход",
};
const HI: Labels = Labels {
  about: "Amenbo के बारे में", check_updates: "अपडेट जाँचें",
  edit: "संपादन", window: "विंडो", file: "फ़ाइल", help: "सहायता", exit: "बाहर निकलें",
};
const ID: Labels = Labels {
  about: "Tentang Amenbo", check_updates: "Periksa Pembaruan",
  edit: "Edit", window: "Jendela", file: "Berkas", help: "Bantuan", exit: "Keluar",
};
const VI: Labels = Labels {
  about: "Giới thiệu về Amenbo", check_updates: "Kiểm tra bản cập nhật",
  edit: "Chỉnh sửa", window: "Cửa sổ", file: "Tệp", help: "Trợ giúp", exit: "Thoát",
};
const TH: Labels = Labels {
  about: "เกี่ยวกับ Amenbo", check_updates: "ตรวจสอบการอัปเดต",
  edit: "แก้ไข", window: "หน้าต่าง", file: "ไฟล์", help: "ช่วยเหลือ", exit: "ออก",
};
const TR: Labels = Labels {
  about: "Amenbo Hakkında", check_updates: "Güncellemeleri Denetle",
  edit: "Düzen", window: "Pencere", file: "Dosya", help: "Yardım", exit: "Çıkış",
};
const PL: Labels = Labels {
  about: "O programie Amenbo", check_updates: "Sprawdź aktualizacje",
  edit: "Edycja", window: "Okno", file: "Plik", help: "Pomoc", exit: "Zakończ",
};
const NL: Labels = Labels {
  about: "Over Amenbo", check_updates: "Controleren op updates",
  edit: "Bewerken", window: "Venster", file: "Bestand", help: "Help", exit: "Afsluiten",
};
const UK: Labels = Labels {
  about: "Про Amenbo", check_updates: "Перевірити оновлення",
  edit: "Редагування", window: "Вікно", file: "Файл", help: "Довідка", exit: "Вихід",
};

/// The menu's words for `config.language`, English for anything else (`AMB-D-394`: English is where
/// an unset setting and an unknown code both land).
///
/// A code is read down from its script or region, the way `language_label` in core reads one and
/// the way `normalizeLang` does on the TS side: Simplified and Traditional Chinese are separate
/// menus, and any Portuguese reaches the only Portuguese carried rather than falling to English.
/// Subtags are matched case-insensitively, as BCP 47 defines them.
///
/// Kept local so the menu can label itself at build time without opening the store.
fn labels(language: Option<&str>) -> &'static Labels {
  let code = language.unwrap_or_default();
  let mut subtags = code.split(['-', '_']);
  let primary = subtags.next().unwrap_or_default().to_ascii_lowercase();
  let secondary = subtags.next().unwrap_or_default().to_ascii_lowercase();
  match (primary.as_str(), secondary.as_str()) {
    ("ja", _) => &JA,
    ("zh", "hant" | "tw" | "hk" | "mo") => &ZH_HANT,
    ("zh", _) => &ZH_HANS,
    ("ko", _) => &KO,
    ("es", _) => &ES,
    ("pt", _) => &PT_BR,
    ("fr", _) => &FR,
    ("de", _) => &DE,
    ("it", _) => &IT,
    ("ru", _) => &RU,
    ("hi", _) => &HI,
    ("id", _) => &ID,
    ("vi", _) => &VI,
    ("th", _) => &TH,
    ("tr", _) => &TR,
    ("pl", _) => &PL,
    ("nl", _) => &NL,
    ("uk", _) => &UK,
    _ => &EN,
  }
}

/// Build the application menu. Reads `config.language` directly (a file outside the store, so no
/// version gate) to localize the labels, and stamps the running version + product site into the
/// About dialog's metadata.
pub fn build<R: tauri::Runtime>(handle: &tauri::AppHandle<R>) -> tauri::Result<Menu<R>> {
  let language = amenbo_core::config::Paths::resolve()
    .ok()
    .and_then(|p| amenbo_core::config::Config::load(&p.config_file).language);
  let l = labels(language.as_deref());

  let about_meta = AboutMetadataBuilder::new()
    .version(Some(handle.package_info().version.to_string()))
    .website(Some(WEBSITE))
    .website_label(Some("amenbo.work"))
    .build();
  let about = PredefinedMenuItem::about(handle, Some(l.about), Some(about_meta))?;

  // A manual, on-demand update check that sits with About (the version affordance). Its click is
  // handled in lib.rs; no accelerator (checking is a rare, deliberate action).
  //
  // A development build has no self-update to check on, so it carries no item either: the answer
  // there could only ever be "up to date", which on a build that is normally behind production is a
  // sentence that is not true. `upstream_release` in commands.rs is where that is decided.
  let check_updates = if amenbo_core::config::Paths::is_dev_channel() {
    None
  } else {
    Some(MenuItem::with_id(
      handle,
      CHECK_UPDATES_ID,
      l.check_updates,
      true,
      None::<&str>,
    )?)
  };

  #[cfg(target_os = "macos")]
  {
    // The first submenu becomes the application menu; macOS localizes the predefined items itself.
    let mut app_menu = SubmenuBuilder::new(handle, "Amenbo").item(&about);
    if let Some(check_updates) = &check_updates {
      app_menu = app_menu.separator().item(check_updates);
    }
    let app_menu = app_menu
      .separator()
      .services()
      .separator()
      .hide()
      .hide_others()
      .show_all()
      .separator()
      .quit()
      .build()?;
    let edit_menu = SubmenuBuilder::new(handle, l.edit)
      .undo()
      .redo()
      .separator()
      .cut()
      .copy()
      .paste()
      .select_all()
      .build()?;
    let window_menu = SubmenuBuilder::new(handle, l.window)
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
    let quit = PredefinedMenuItem::quit(handle, Some(l.exit))?;
    let file_menu = SubmenuBuilder::new(handle, l.file)
      .item(&quit)
      .build()?;
    let mut help_menu = SubmenuBuilder::new(handle, l.help).item(&about);
    if let Some(check_updates) = &check_updates {
      help_menu = help_menu.item(check_updates);
    }
    let help_menu = help_menu.build()?;
    MenuBuilder::new(handle)
      .items(&[&file_menu, &help_menu])
      .build()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  /// The one thing that can silently go wrong here: a code lands on the wrong table. The words
  /// themselves are a translator's business, so what is pinned is which table each code reaches.
  #[test]
  fn a_code_reaches_the_menu_its_reader_expects() {
    assert_eq!(labels(Some("ja")).about, JA.about);
    // What the platform hands over is a region, not a script — and the two Chinese menus are not
    // interchangeable, so the region has to decide which one.
    assert_eq!(labels(Some("zh-TW")).about, ZH_HANT.about);
    assert_eq!(labels(Some("zh-CN")).about, ZH_HANS.about);
    assert_eq!(labels(Some("zh")).about, ZH_HANS.about);
    // Only one Portuguese is carried, so European Portuguese reads as it rather than as English.
    assert_eq!(labels(Some("pt-PT")).about, PT_BR.about);
    // A region we do not narrow by is still the language it names.
    assert_eq!(labels(Some("de-AT")).about, DE.about);
    assert_eq!(labels(Some("ZH_hant")).about, ZH_HANT.about);
  }

  /// `AMB-D-394`: an unset setting and a code from outside the nineteen both land on English —
  /// never on a language nobody asked for.
  #[test]
  fn anything_else_is_english() {
    assert_eq!(labels(None).about, EN.about);
    assert_eq!(labels(Some("")).about, EN.about);
    assert_eq!(labels(Some("xx")).about, EN.about);
    assert_eq!(labels(Some("ar")).about, EN.about);
  }
}
