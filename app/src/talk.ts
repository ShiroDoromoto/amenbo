// The talk window's entry — the window someone splits the terminal out into, so the two faces can
// sit on two displays (`AMB-D-753`). It is not opened at launch and is not declared in
// `tauri.conf.json`: the app comes up as one window, and this one is built when it is asked for
// (`crate::windows`). What is in it is a terminal: one pane, filling the window, running the shell
// the user signed in with.
//
// The terminal it draws is the one that was already running in the board, and folding the app back
// hands it over the same way. Neither end restarts anything — a pane is a drawing of a session, not
// the session (`./talk/terminal`).
//
// What it does own from the start is its own name. Two windows both titled "Amenbo" are
// indistinguishable in the window list, the taskbar and the switcher, and the words that tell them
// apart live in the webview's dictionary rather than in Rust (`AMB-D-396`) — so the title is set
// here rather than in the window's definition, which can only hold one fixed string. The configured
// title is the product name, which is what shows for the moment before this runs.
//
// The language is asked of core directly instead of through the snapshot: the store is what a
// startup migration holds shut, and a window with nothing in it has no reason to be the one waiting
// on it.
import { getCurrentWindow } from "@tauri-apps/api/window";
import { type Lang, normalizeLang, t } from "./core/i18n";
import { invoke } from "./core/ipc";
import { initTheme } from "./core/theme";
import { mountTerminal } from "./talk/terminal";
import "./styles/tokens.css";
import "./styles/global.css";
import "./styles/talk.css";

initTheme();

// The bar above the pane, and the one thing on it: the way back to a single window. The board is
// told nothing here — closing this window is what says it, whether it was closed from this button or
// from the title bar, and the board watches for the window going rather than for the button being
// pressed (`crate::windows::TALK_CLOSED_EVENT`).
function bar(lang: Lang): HTMLElement {
  const row = document.createElement("div");
  row.className = "talk__bar";
  const merge = document.createElement("button");
  merge.className = "talk__action";
  merge.textContent = t("face.merge", lang);
  merge.addEventListener("click", () => void invoke("talk_close").catch(() => {}));
  row.append(merge);
  return row;
}

// The pane. A terminal is a live process, so a failure to start one is not a thing to swallow — the
// window would come up empty and say nothing about why. What the host refused with goes on the page
// instead, in the only place there is to put it.
function pane(lang: Lang): HTMLElement {
  const box = document.createElement("div");
  box.className = "talk__pane";
  const ended = () => {
    const note = document.createElement("div");
    note.className = "talk__note";
    note.textContent = t("face.ended", lang);
    box.after(note);
  };
  void mountTerminal(box, ended).catch((e: unknown) => {
    box.className = "talk__failed";
    box.textContent = e instanceof Error ? e.message : String(e);
  });
  return box;
}

// Both the title and the words on the page want the reader's language, so the window is drawn once
// the answer is in. Outside Tauri (`npm run dev` in a browser) there is neither a window to name nor
// a host to ask, and the page is drawn in English rather than not at all.
void invoke<string | null>("ui_language")
  .then((code) => {
    const lang = normalizeLang(code);
    getCurrentWindow().setTitle(t("app.talkWindow", lang));
    return lang;
  })
  .catch(() => normalizeLang(null))
  .then((lang) => {
    const root = document.getElementById("root");
    if (!root) return;
    root.className = "talk";
    root.append(bar(lang), pane(lang));
  });
