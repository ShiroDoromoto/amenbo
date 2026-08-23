// The talk window's entry — the second window of the one app (`AMB-T-3588`). The board (`main.tsx`)
// is "what has happened"; this one is "what is happening now", and what is happening now is a
// terminal: one pane, filling the window, with the agent this folder is worked with running in it.
//
// What goes in the pane is not decided here. The frame around it asks the host which agent this
// folder opens with, and draws the offer or the notice when that has no single answer
// (`./talk/frame`, `AMB-T-3591`).
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
import { type Lang, normalizeLang, t } from "./core/i18n";
import { invoke } from "./core/ipc";
import { initTheme } from "./core/theme";
import { mountFrame } from "./talk/frame";
import "./styles/tokens.css";
import "./styles/global.css";
import "./styles/talk.css";

initTheme();

// The language is settled before the frame is drawn, because everything the frame says — the offer,
// the notice, the row on a closed pane — is a sentence out of the dictionary. Outside Tauri
// (`npm run dev` in a browser) there is nothing to ask and nothing to name, and neither is worth
// failing an otherwise-working window over: the frame is drawn in the fallback language instead.
void invoke<string | null>("ui_language")
  .catch((): string | null => null)
  .then((code) => {
    const lang = normalizeLang(code);
    getCurrentWindow().setTitle(t("app.talkWindow", lang)).catch(() => {});
    return lang;
  })
  .then(draw);

/** Fill the page with the frame, in the language the window says it speaks. */
function draw(lang: Lang): void {
  const root = document.getElementById("root");
  if (!root) return;
  root.className = "talk";
  void mountFrame(root, lang);
}
