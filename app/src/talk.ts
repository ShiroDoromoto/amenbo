// The talk window's entry — the window someone splits the terminal out into, so the two faces can sit
// on two displays (`AMB-D-753`). It is not opened at launch and is not declared in `tauri.conf.json`:
// the app comes up as one window showing the board, and this one is built when it is asked for
// (`crate::windows`). What is in it is a terminal: one pane, filling the window, with this folder's
// agent running in it.
//
// **What goes in the pane is not decided here.** The frame around it asks the host which agent this
// folder starts with, and draws the offer or the install notice where that has no single answer
// (`./talk/agent`).
//
// The terminal it draws is the one that was already running in the board, and folding the app back
// hands it over the same way. Neither end restarts anything — a pane is a drawing of a session, not
// the session (`./talk/terminal`).
//
// **What is happening now is held above the pane and nowhere else.** The label there keeps what it
// knows about the session in it (`./talk/plate`) for as long as it runs, because a session has no
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
import { mountPlate } from "./talk/plate";
import "./styles/tokens.css";
import "./styles/global.css";
import "./styles/talk.css";

initTheme();

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

// The way back to a single window. The board is told nothing here — this window going is what says
// it, whether it went from this button or from the title bar, and the board watches for the window
// rather than for the press (`crate::windows`). Worded in English until the language answers, the way
// the title is: a bar that waited would be a window with no way out of it for as long as the wait.
function mergeButton(): HTMLElement {
  const row = document.createElement("div");
  row.className = "talk__bar";
  const merge = document.createElement("button");
  merge.className = "talk__action";
  merge.textContent = t("face.merge", "en");
  merge.addEventListener("click", () => void invoke("talk_close").catch(() => {}));
  void language.then((lang) => {
    merge.textContent = t("face.merge", lang);
  });
  row.append(merge);
  return row;
}

void frameNames()
  .then((known) => {
    names = known;
    plate?.named(known);
    retitle();
  })
  .catch(() => {});

/** Offer a name for the pane's frame. Whether it takes is the store's ranking to decide, so what comes
 *  back is drawn rather than what was asked for. */
function name(text: string, by: NamedBy): void {
  void nameFrame(ONLY_FRAME, text, by)
    .then((known) => {
      names = known;
      plate?.named(known);
      retitle();
    })
    .catch(() => {});
}

// The label above the pane, once there is a pane to put it above.
let plate: ReturnType<typeof mountPlate> | null = null;

// The pane. A terminal is a live process, so a failure to start one is not a thing to swallow — the
// window would come up empty and say nothing about why. What the host refused with goes on the page
// instead, in the only place there is to put it.
const root = document.getElementById("root");
if (root) {
  root.className = "talk";
  const label = document.createElement("div");
  const face = document.createElement("div");
  face.className = "talk__face";
  root.append(mergeButton(), label, face);
  // The language is not known yet, and the label is drawn before it answers. What it is drawn in until
  // then is English, the way the title bar and the merge button are: a window that waited would come up
  // with a blank line above the pane for as long as the wait.
  let lang: Lang = normalizeLang(null);
  void language.then((answered) => {
    lang = answered;
  });
  plate = mountPlate(label, () => lang);
  // The frame does wait on the language, unlike the label: everything it says before a terminal is
  // running — the offer, the install notice, the row on a closed pane — is a whole sentence, and one
  // drawn in English first would be a flicker of the wrong language rather than a pane arriving
  // sooner.
  void language.then((answered) =>
    mountAgentFrame(
      face,
      answered,
      {
        opened: (session, startedAt) => {
          plate?.opened(session, startedAt);
        },
        // This window draws one pane and holds no arrangement, so where its frame settled is nobody's
        // to be told: what puts a page in a project is the board's (`./talk/layout`).
        chose: () => {},
        said: (statement) => {
          plate?.said(statement);
        },
        closed: (session) => {
          plate?.closed(session);
          // What is on the screen stays as it was — that is what a terminal ends with — and this is
          // the part of it the screen cannot show for itself: a finished shell looks exactly like one
          // waiting to be typed at.
          const note = document.createElement("div");
          note.className = "talk__note";
          note.textContent = t("face.ended", answered);
          face.after(note);
        },
        name,
      },
      "talk__pane",
    ),
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
