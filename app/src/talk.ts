// The talk window's entry — the second window of the one app (`AMB-T-3588`). The board (`main.tsx`)
// is "what has happened"; this one is "what is happening now", and it is empty until the panes that
// belong here arrive.
//
// What it does own from the start is its own name. Two windows both titled "Amenbo" are
// indistinguishable in the window list, the taskbar and the switcher, and the words that tell them
// apart live in the webview's dictionary rather than in Rust (`AMB-D-396`) — so the title is set
// here rather than in `tauri.conf.json`, which can only hold one fixed string. The configured title
// is the product name, which is what shows for the moment before this runs.
//
// The language is asked of core directly instead of through the snapshot: the store is what a
// startup migration holds shut, and a window with nothing in it has no reason to be the one waiting
// on it.
import { getCurrentWindow } from "@tauri-apps/api/window";
import { normalizeLang, t } from "./core/i18n";
import { invoke } from "./core/ipc";
import { initTheme } from "./core/theme";
import "./styles/tokens.css";
import "./styles/global.css";

initTheme();

void invoke<string | null>("ui_language")
  .then((code) => getCurrentWindow().setTitle(t("app.talkWindow", normalizeLang(code))))
  // Outside Tauri (`npm run dev` in a browser) there is no window to name, and a title that could
  // not be set is not worth failing an otherwise-working window over.
  .catch(() => {});
