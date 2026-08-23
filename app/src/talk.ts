// The talk window's entry — the second window of the one app (`AMB-T-3588`). The board (`main.tsx`)
// is "what has happened"; this one is "what is happening now", and what is happening now is a
// terminal: one pane, filling the window, with the user's shell running in it.
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
import { elevationBand } from "./talk/elevation";
import { mountTerminal } from "./talk/terminal";
import "./styles/tokens.css";
import "./styles/global.css";
import "./styles/talk.css";

initTheme();

const language: Promise<Lang> = invoke<string | null>("ui_language")
  .then((code) => normalizeLang(code))
  // Outside Tauri (`npm run dev` in a browser) nothing answers, and English is where every lookup
  // lands anyway when a language has no dictionary.
  .catch(() => normalizeLang(null));

void language
  .then((lang) => getCurrentWindow().setTitle(t("app.talkWindow", lang)))
  // A title that could not be set is not worth failing an otherwise-working window over.
  .catch(() => {});

// The pane. A terminal is a live process, so a failure to start one is not a thing to swallow — the
// window would come up empty and say nothing about why. What the host refused with goes on the page
// instead, in the only place there is to put it.
const root = document.getElementById("root");
if (root) {
  root.className = "talk";
  const pane = document.createElement("div");
  pane.className = "talk__pane";
  root.append(pane);
  void mountTerminal(pane).catch((e: unknown) => {
    pane.className = "talk__failed";
    pane.textContent = e instanceof Error ? e.message : String(e);
  });

  // The band above the pane, and only when there is something to say. It is asked for after the
  // terminal is mounted rather than before it: the pane is what the window is for, and a question
  // about this process's own token has no business delaying it. It goes in *above* the pane in the
  // page, which is why it is inserted at the top rather than appended.
  void Promise.all([invoke<boolean>("elevated"), language])
    .then(([elevated, lang]) => {
      if (elevated) root.prepend(elevationBand(lang));
    })
    // Nothing answered, so there is nothing established to warn about. A band raised on a failed
    // question would be shown to every browser this page is opened in.
    .catch(() => {});
}
