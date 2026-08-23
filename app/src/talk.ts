// The talk window's entry — the second window of the one app (`AMB-T-3588`). The board (`main.tsx`)
// is "what has happened"; this one is "what is happening now", and what is happening now is a
// terminal: one pane, filling the window, with this folder's agent running in it.
//
// **What goes in the pane is not decided here.** The frame around it asks the host which agent this
// folder opens with, and draws the offer or the install notice where that has no single answer
// (`./talk/agent`).
//
// **What is happening now is held here and nowhere else.** The window keeps what it knows about the
// sessions running in its panes (`./talk/sessions`) for as long as they run, because a session has no
// existence outside the terminal it runs in (`AMB-D-749`). What is kept between runs is what the panes
// are called (`./talk/frames`), which belongs to the frame rather than to the process in it.
//
// What it owns from the start is its own name. Two windows both titled "Amenbo" are indistinguishable
// in the window list, the taskbar and the switcher, and the words that tell them apart live in the
// webview's dictionary rather than in Rust (`AMB-D-396`) — so the title is set here rather than in
// `tauri.conf.json`, which can only hold one fixed string. The configured title is the product name,
// which is what shows for the moment before this runs. Once the pane's frame has a name, that is the
// title: it is the pane's own name, and it is what tells one window from another in a list of them.
//
// The language is asked of core directly instead of through the snapshot: the store is what a startup
// migration holds shut, and a window with nothing in it has no reason to be the one waiting on it.
import { getCurrentWindow } from "@tauri-apps/api/window";
import { type Lang, normalizeLang, t } from "./core/i18n";
import { invoke } from "./core/ipc";
import { initTheme } from "./core/theme";
import { mountAgentFrame } from "./talk/agent";
import { elevationBand } from "./talk/elevation";
import { frameNames, nameFrame, ONLY_FRAME, type FrameNames, type NamedBy } from "./talk/frames";
import { closed, NO_SESSIONS, opened, said, type Sessions } from "./talk/sessions";
import "./styles/tokens.css";
import "./styles/global.css";
import "./styles/talk.css";

initTheme();

// The sessions running in this window's panes. Nothing here is written down: it is gone when the
// window is, which is exactly as long as the processes it describes.
let sessions: Sessions = NO_SESSIONS;

// What the panes are called, as this device has them. Read once at the start, and again from whatever
// a naming answers with — the answer is the whole set, because a naming can be refused.
let names: FrameNames = new Map();

let title = t("app.talkWindow", "en");

/** Name the window after the pane's frame, falling back to what the window is when it has no name. */
function retitle(): void {
  const named = names.get(ONLY_FRAME);
  void getCurrentWindow()
    .setTitle(named ?? title)
    // Outside Tauri (`npm run dev` in a browser) there is no window to name, and a title that could
    // not be set is not worth failing an otherwise-working window over.
    .catch(() => {});
}

// The language this page is written in. The title is not the only thing that waits on it — a band may
// have to be worded too — so the answer is held as something the rest of the page can wait on rather
// than being spent where it lands.
const language: Promise<Lang> = invoke<string | null>("ui_language")
  .then((code) => normalizeLang(code))
  // Outside Tauri (`npm run dev` in a browser) nothing answers, and English is where every lookup
  // lands anyway when a language has no dictionary.
  .catch(() => normalizeLang(null));

void language.then((lang) => {
  title = t("app.talkWindow", lang);
  retitle();
});

void frameNames()
  .then((known) => {
    names = known;
    retitle();
  })
  .catch(() => {});

/** Offer a name for the pane's frame. Whether it takes is the store's ranking to decide, so what comes
 *  back is drawn rather than what was asked for. */
function name(text: string, by: NamedBy): void {
  void nameFrame(ONLY_FRAME, text, by)
    .then((known) => {
      names = known;
      retitle();
    })
    .catch(() => {});
}

// The frame, and the pane inside it. It waits on the language because everything the frame says
// before a terminal is running — the offer, the notice, the row on a closed pane — is a sentence out
// of the dictionary, and drawing it in English first would be a flicker of the wrong language rather
// than a pane arriving sooner.
const root = document.getElementById("root");
if (root) {
  root.className = "talk";
  void language.then((lang) =>
    mountAgentFrame(root, lang, {
      opened: (session, startedAt) => {
        sessions = opened(sessions, { session, startedAt });
      },
      said: (statement) => {
        sessions = said(sessions, statement);
      },
      closed: (session) => {
        sessions = closed(sessions, session);
      },
      name,
    }),
  );

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
