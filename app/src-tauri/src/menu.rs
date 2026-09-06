//! Native application menu. Application info (About / version) is served through the OS-native app
//! menu, and Windows gets a menu too — macOS had the default About, Windows had no route at all.
//! Built once at startup and shared by every window — both of them (`crate::windows`): About, the
//! update check and Quit are the app's, not one face's, so either window is a fair place to reach them.
//!
//! On macOS the whole menu is replaced, so the standard app / Edit / Window submenus are rebuilt
//! here: dropping the Edit submenu would strip the webview of Cmd+C/V/X/A, which the default menu
//! had been providing. On Windows/Linux a single Help submenu carries About.

use tauri::menu::{
  AboutMetadataBuilder, Menu, MenuBuilder, MenuItem, PredefinedMenuItem, SubmenuBuilder,
};

use crate::quit;

/// Product site — the About dialog links here.
const WEBSITE: &str = "https://amenbo.work/";

/// Menu id of the manual "check for updates" item. `lib.rs`'s `on_menu_event` matches this and, on a
/// click, tells the webview to run a fresh check (which then shows the update banner or an "up to
/// date" note).
pub const CHECK_UPDATES_ID: &str = "check-updates";

/// The words the native menu needs, in one language.
///
/// This is the whole of the exception carved out in `AMB-D-396`: everywhere else the words live in
/// the webview's dictionary, but the menu bar is assembled before the webview runs, so every word on
/// it has to be reachable from Rust.
///
/// **Nothing on this menu is labelled by the OS.** The predefined items look as though they would be
/// — Copy and Minimize are the platform's own, and the platform has a name for each in every language
/// it ships. What reaches the menu bar is not that name: `muda` carries an English string per
/// predefined kind and writes it into the `NSMenuItem` title, and a title that is already filled in
/// leaves macOS nothing to translate. So the ones this app puts there are named here, in the reader's
/// language, alongside the ones it invents.
///
/// **Quit is written here rather than left to the OS**, which is what `quit_app` is. macOS labels
/// its own predefined quit, and the item that comes with that label is wired to the platform's
/// terminate — a way out that ends the process without this side ever being asked, and so without
/// the question a pane full of running agents deserves (`crate::quit`). An item of the app's own
/// carries the click here, and has to carry the words too.
///
/// Which fields are read depends on the platform: macOS builds the app / Edit / Window submenus and
/// names the quit the way that platform's menus name it, while Windows and Linux build File / Help
/// and label Exit themselves. They stay in one table anyway, because the unit a translator works in
/// is a language, not a platform — splitting them would mean writing one language's menu in two
/// places.
#[allow(dead_code)]
struct Labels {
  about: &'static str,
  check_updates: &'static str,
  edit: &'static str,
  window: &'static str,
  file: &'static str,
  help: &'static str,
  exit: &'static str,
  quit_app: &'static str,
  // The app menu's predefined items. `hide_app` carries the product name the way `about` and
  // `quit_app` do — the platform writes "Hide <app>" and the translations follow it.
  services: &'static str,
  hide_app: &'static str,
  hide_others: &'static str,
  show_all: &'static str,
  // The Edit menu. Without these the webview keeps its shortcuts but reads as an English menu inside
  // a translated one.
  undo: &'static str,
  redo: &'static str,
  cut: &'static str,
  copy: &'static str,
  paste: &'static str,
  select_all: &'static str,
  // The Window menu. `zoom` is what macOS calls the maximize item, and the name follows the platform
  // rather than the builder method that reaches it.
  minimize: &'static str,
  zoom: &'static str,
  close_window: &'static str,
}

/// The nineteen languages of `AMB-D-394`, each written the way that language's own menu bars are.
/// A term is the platform's conventional one where there is one — `Édition` and not `Modifier`,
/// `Правка` and not `Редактировать` — because a menu that translates literally reads as foreign
/// even when every word is right.
const EN: Labels = Labels {
  about: "About Amenbo", check_updates: "Check for Updates",
  edit: "Edit", window: "Window", file: "File", help: "Help", exit: "Exit",
  quit_app: "Quit Amenbo",
  services: "Services", hide_app: "Hide Amenbo", hide_others: "Hide Others", show_all: "Show All",
  undo: "Undo", redo: "Redo", cut: "Cut", copy: "Copy", paste: "Paste", select_all: "Select All",
  minimize: "Minimize", zoom: "Zoom", close_window: "Close Window",
};
const JA: Labels = Labels {
  about: "Amenbo について", check_updates: "更新を確認",
  edit: "編集", window: "ウインドウ", file: "ファイル", help: "ヘルプ", exit: "終了",
  quit_app: "Amenbo を終了",
  services: "サービス", hide_app: "Amenbo を隠す", hide_others: "ほかを隠す", show_all: "すべてを表示",
  undo: "取り消す", redo: "やり直す", cut: "カット", copy: "コピー", paste: "ペースト", select_all: "すべてを選択",
  minimize: "しまう", zoom: "拡大/縮小", close_window: "ウインドウを閉じる",
};
const ZH_HANS: Labels = Labels {
  about: "关于 Amenbo", check_updates: "检查更新",
  edit: "编辑", window: "窗口", file: "文件", help: "帮助", exit: "退出",
  quit_app: "退出 Amenbo",
  services: "服务", hide_app: "隐藏 Amenbo", hide_others: "隐藏其他", show_all: "全部显示",
  undo: "撤销", redo: "重做", cut: "剪切", copy: "拷贝", paste: "粘贴", select_all: "全选",
  minimize: "最小化", zoom: "缩放", close_window: "关闭窗口",
};
const ZH_HANT: Labels = Labels {
  about: "關於 Amenbo", check_updates: "檢查更新",
  edit: "編輯", window: "視窗", file: "檔案", help: "說明", exit: "結束",
  quit_app: "結束 Amenbo",
  services: "服務", hide_app: "隱藏 Amenbo", hide_others: "隱藏其他", show_all: "全部顯示",
  undo: "還原", redo: "重做", cut: "剪下", copy: "拷貝", paste: "貼上", select_all: "全選",
  minimize: "縮到最小", zoom: "縮放", close_window: "關閉視窗",
};
const KO: Labels = Labels {
  about: "Amenbo 정보", check_updates: "업데이트 확인",
  edit: "편집", window: "윈도우", file: "파일", help: "도움말", exit: "종료",
  quit_app: "Amenbo 종료",
  services: "서비스", hide_app: "Amenbo 가리기", hide_others: "다른 항목 가리기", show_all: "모두 보기",
  undo: "실행 취소", redo: "실행 복귀", cut: "오려두기", copy: "복사하기", paste: "붙여넣기", select_all: "전체 선택",
  minimize: "최소화", zoom: "확대·축소", close_window: "윈도우 닫기",
};
const ES: Labels = Labels {
  about: "Acerca de Amenbo", check_updates: "Buscar actualizaciones",
  edit: "Edición", window: "Ventana", file: "Archivo", help: "Ayuda", exit: "Salir",
  quit_app: "Salir de Amenbo",
  services: "Servicios", hide_app: "Ocultar Amenbo", hide_others: "Ocultar otros", show_all: "Mostrar todo",
  undo: "Deshacer", redo: "Rehacer", cut: "Cortar", copy: "Copiar", paste: "Pegar", select_all: "Seleccionar todo",
  minimize: "Minimizar", zoom: "Zoom", close_window: "Cerrar ventana",
};
const PT_BR: Labels = Labels {
  about: "Sobre o Amenbo", check_updates: "Verificar atualizações",
  edit: "Editar", window: "Janela", file: "Arquivo", help: "Ajuda", exit: "Sair",
  quit_app: "Encerrar Amenbo",
  services: "Serviços", hide_app: "Ocultar Amenbo", hide_others: "Ocultar Outros", show_all: "Mostrar Tudo",
  undo: "Desfazer", redo: "Refazer", cut: "Recortar", copy: "Copiar", paste: "Colar", select_all: "Selecionar Tudo",
  minimize: "Minimizar", zoom: "Zoom", close_window: "Fechar Janela",
};
const FR: Labels = Labels {
  about: "À propos d’Amenbo", check_updates: "Rechercher les mises à jour",
  edit: "Édition", window: "Fenêtre", file: "Fichier", help: "Aide", exit: "Quitter",
  quit_app: "Quitter Amenbo",
  services: "Services", hide_app: "Masquer Amenbo", hide_others: "Masquer les autres", show_all: "Tout afficher",
  undo: "Annuler", redo: "Rétablir", cut: "Couper", copy: "Copier", paste: "Coller", select_all: "Tout sélectionner",
  minimize: "Réduire", zoom: "Réduire/Agrandir", close_window: "Fermer la fenêtre",
};
const DE: Labels = Labels {
  about: "Über Amenbo", check_updates: "Nach Updates suchen",
  edit: "Bearbeiten", window: "Fenster", file: "Datei", help: "Hilfe", exit: "Beenden",
  quit_app: "Amenbo beenden",
  services: "Dienste", hide_app: "Amenbo ausblenden", hide_others: "Andere ausblenden", show_all: "Alle einblenden",
  undo: "Widerrufen", redo: "Wiederholen", cut: "Ausschneiden", copy: "Kopieren", paste: "Einsetzen", select_all: "Alles auswählen",
  minimize: "Im Dock ablegen", zoom: "Zoomen", close_window: "Fenster schließen",
};
const IT: Labels = Labels {
  about: "Informazioni su Amenbo", check_updates: "Verifica aggiornamenti",
  edit: "Modifica", window: "Finestra", file: "File", help: "Aiuto", exit: "Esci",
  quit_app: "Esci da Amenbo",
  services: "Servizi", hide_app: "Nascondi Amenbo", hide_others: "Nascondi altre", show_all: "Mostra tutto",
  undo: "Annulla", redo: "Ripristina", cut: "Taglia", copy: "Copia", paste: "Incolla", select_all: "Seleziona tutto",
  minimize: "Riduci a icona", zoom: "Zoom", close_window: "Chiudi finestra",
};
const RU: Labels = Labels {
  about: "О программе Amenbo", check_updates: "Проверить обновления",
  edit: "Правка", window: "Окно", file: "Файл", help: "Справка", exit: "Выход",
  quit_app: "Завершить Amenbo",
  services: "Службы", hide_app: "Скрыть Amenbo", hide_others: "Скрыть остальные", show_all: "Показать все",
  undo: "Отменить", redo: "Повторить", cut: "Вырезать", copy: "Скопировать", paste: "Вставить", select_all: "Выбрать все",
  minimize: "Убрать в Dock", zoom: "Масштабировать", close_window: "Закрыть окно",
};
const HI: Labels = Labels {
  about: "Amenbo के बारे में", check_updates: "अपडेट जाँचें",
  edit: "संपादन", window: "विंडो", file: "फ़ाइल", help: "सहायता", exit: "बाहर निकलें",
  quit_app: "Amenbo छोड़ें",
  services: "सेवाएँ", hide_app: "Amenbo छिपाएँ", hide_others: "अन्य छिपाएँ", show_all: "सभी दिखाएँ",
  undo: "पूर्ववत करें", redo: "फिर से करें", cut: "काटें", copy: "कॉपी करें", paste: "पेस्ट करें", select_all: "सभी चुनें",
  minimize: "मिनिमाइज़ करें", zoom: "ज़ूम", close_window: "विंडो बंद करें",
};
const ID: Labels = Labels {
  about: "Tentang Amenbo", check_updates: "Periksa Pembaruan",
  edit: "Edit", window: "Jendela", file: "Berkas", help: "Bantuan", exit: "Keluar",
  quit_app: "Keluar dari Amenbo",
  services: "Layanan", hide_app: "Sembunyikan Amenbo", hide_others: "Sembunyikan Lainnya", show_all: "Tampilkan Semua",
  undo: "Urungkan", redo: "Ulangi", cut: "Potong", copy: "Salin", paste: "Tempel", select_all: "Pilih Semua",
  minimize: "Perkecil", zoom: "Zoom", close_window: "Tutup Jendela",
};
const VI: Labels = Labels {
  about: "Giới thiệu về Amenbo", check_updates: "Kiểm tra bản cập nhật",
  edit: "Chỉnh sửa", window: "Cửa sổ", file: "Tệp", help: "Trợ giúp", exit: "Thoát",
  quit_app: "Thoát Amenbo",
  services: "Dịch vụ", hide_app: "Ẩn Amenbo", hide_others: "Ẩn mục khác", show_all: "Hiển thị tất cả",
  undo: "Hoàn tác", redo: "Làm lại", cut: "Cắt", copy: "Sao chép", paste: "Dán", select_all: "Chọn tất cả",
  minimize: "Thu nhỏ", zoom: "Thu phóng", close_window: "Đóng cửa sổ",
};
const TH: Labels = Labels {
  about: "เกี่ยวกับ Amenbo", check_updates: "ตรวจสอบการอัปเดต",
  edit: "แก้ไข", window: "หน้าต่าง", file: "ไฟล์", help: "ช่วยเหลือ", exit: "ออก",
  quit_app: "ออกจาก Amenbo",
  services: "บริการ", hide_app: "ซ่อน Amenbo", hide_others: "ซ่อนหน้าต่างอื่น", show_all: "แสดงทั้งหมด",
  undo: "เลิกทำ", redo: "ทำซ้ำ", cut: "ตัด", copy: "คัดลอก", paste: "วาง", select_all: "เลือกทั้งหมด",
  minimize: "ย่อ", zoom: "ซูม", close_window: "ปิดหน้าต่าง",
};
const TR: Labels = Labels {
  about: "Amenbo Hakkında", check_updates: "Güncellemeleri Denetle",
  edit: "Düzen", window: "Pencere", file: "Dosya", help: "Yardım", exit: "Çıkış",
  quit_app: "Amenbo'dan Çık",
  services: "Hizmetler", hide_app: "Amenbo'yu Gizle", hide_others: "Diğerlerini Gizle", show_all: "Tümünü Göster",
  undo: "Geri Al", redo: "Yinele", cut: "Kes", copy: "Kopyala", paste: "Yapıştır", select_all: "Tümünü Seç",
  minimize: "Küçült", zoom: "Yakınlaştır", close_window: "Pencereyi Kapat",
};
const PL: Labels = Labels {
  about: "O programie Amenbo", check_updates: "Sprawdź aktualizacje",
  edit: "Edycja", window: "Okno", file: "Plik", help: "Pomoc", exit: "Zakończ",
  quit_app: "Zakończ Amenbo",
  services: "Usługi", hide_app: "Ukryj Amenbo", hide_others: "Ukryj inne", show_all: "Pokaż wszystko",
  undo: "Cofnij", redo: "Ponów", cut: "Wytnij", copy: "Kopiuj", paste: "Wklej", select_all: "Zaznacz wszystko",
  minimize: "Zminimalizuj", zoom: "Powiększ", close_window: "Zamknij okno",
};
const NL: Labels = Labels {
  about: "Over Amenbo", check_updates: "Controleren op updates",
  edit: "Bewerken", window: "Venster", file: "Bestand", help: "Help", exit: "Afsluiten",
  quit_app: "Stop Amenbo",
  services: "Diensten", hide_app: "Verberg Amenbo", hide_others: "Verberg andere", show_all: "Toon alles",
  undo: "Herstel", redo: "Opnieuw", cut: "Knip", copy: "Kopieer", paste: "Plak", select_all: "Selecteer alles",
  minimize: "Minimaliseer", zoom: "Zoom", close_window: "Sluit venster",
};
const UK: Labels = Labels {
  about: "Про Amenbo", check_updates: "Перевірити оновлення",
  edit: "Редагування", window: "Вікно", file: "Файл", help: "Довідка", exit: "Вихід",
  quit_app: "Завершити Amenbo",
  services: "Служби", hide_app: "Сховати Amenbo", hide_others: "Сховати інші", show_all: "Показати все",
  undo: "Скасувати", redo: "Повторити", cut: "Вирізати", copy: "Копіювати", paste: "Вставити", select_all: "Вибрати все",
  minimize: "Згорнути", zoom: "Масштабувати", close_window: "Закрити вікно",
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
    // The way out, with the shortcut this platform's readers reach for. It is the app's own item and
    // not the predefined quit, so the click arrives here and the question can be asked before
    // anything ends (`crate::quit`) — the predefined one is the OS's terminate, which never offers.
    let quit = MenuItem::with_id(handle, quit::QUIT_ID, l.quit_app, true, Some("CmdOrCtrl+Q"))?;
    // The first submenu becomes the application menu. Its own title is the product name and stays
    // untranslated; everything under it is named here, predefined items included — the `*_with_text`
    // half of the builder exists because the plain call writes English (`Labels`).
    let mut app_menu = SubmenuBuilder::new(handle, "Amenbo").item(&about);
    if let Some(check_updates) = &check_updates {
      app_menu = app_menu.separator().item(check_updates);
    }
    let app_menu = app_menu
      .separator()
      .services_with_text(l.services)
      .separator()
      .hide_with_text(l.hide_app)
      .hide_others_with_text(l.hide_others)
      .show_all_with_text(l.show_all)
      .separator()
      .item(&quit)
      .build()?;
    let edit_menu = SubmenuBuilder::new(handle, l.edit)
      .undo_with_text(l.undo)
      .redo_with_text(l.redo)
      .separator()
      .cut_with_text(l.cut)
      .copy_with_text(l.copy)
      .paste_with_text(l.paste)
      .select_all_with_text(l.select_all)
      .build()?;
    // Maximize is the builder's name for the item macOS calls Zoom, which is why the label it is
    // given is `zoom` — the menu bar says what this platform's menu bars say.
    let window_menu = SubmenuBuilder::new(handle, l.window)
      .minimize_with_text(l.minimize)
      .maximize_with_text(l.zoom)
      .separator()
      .close_window_with_text(l.close_window)
      .build()?;
    MenuBuilder::new(handle)
      .items(&[&app_menu, &edit_menu, &window_menu])
      .build()
  }
  #[cfg(not(target_os = "macos"))]
  {
    // A File submenu carries Exit so the menu bar is not a single lonely Help entry (macOS keeps
    // Quit under its app menu instead). It is the app's own item rather than the predefined quit,
    // for the reason the macOS branch gives: the click has to reach this side before anything ends
    // (`crate::quit`). No accelerator, which is what the predefined one carried here too.
    let quit = MenuItem::with_id(handle, quit::QUIT_ID, l.exit, true, None::<&str>)?;
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
