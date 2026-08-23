// The talk window's entry — the second window of the one app (`AMB-T-3588`). The board (`main.tsx`)
// is "what has happened"; this one is "what is happening now", and what is happening now is a
// terminal: one pane, filling the window, with the user's shell running in it.
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
import type { SessionWorkDto, TaskCardDto } from "./bindings/bindings";
import { normalizeLang, t, type Lang } from "./core/i18n";
import { invoke } from "./core/ipc";
import { initTheme } from "./core/theme";
import { frameNames, nameFrame, ONLY_FRAME, type FrameNames, type NamedBy } from "./talk/frames";
import {
  FINISHED_HOLD_MS,
  mountNameplate,
  NO_CHANGEOVER,
  nowOf,
  sayOf,
  type Changeover,
  type Held,
  type Plate,
} from "./talk/nameplate";
import { closed, NO_SESSIONS, opened, said, type Sessions } from "./talk/sessions";
import { mountTerminal } from "./talk/terminal";
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

// The reader's language, as core answers it. The talk window loads no snapshot, so this is the only
// place it is known — everything drawn here is drawn in it.
let lang: Lang = "en";
let title = t("app.talkWindow", lang);

/** Name the window after the pane's frame, falling back to what the window is when it has no name. */
function retitle(): void {
  redraw();
  const named = names.get(ONLY_FRAME);
  void getCurrentWindow()
    .setTitle(named ?? title)
    // Outside Tauri (`npm run dev` in a browser) there is no window to name, and a title that could
    // not be set is not worth failing an otherwise-working window over.
    .catch(() => {});
}

void invoke<string | null>("ui_language")
  .then((code) => {
    lang = normalizeLang(code);
    title = t("app.talkWindow", lang);
    retitle();
  })
  .catch(() => {});

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

// What this pane's session is holding, as the ledger answers it, and where the changeover has got to.
// The tasks are hydrated through the same read every other screen uses — what the ledger alone can
// answer is *whose* they are.
let held: Held[] = [];
let finished = 0;
let changeover: Changeover = NO_CHANGEOVER;
let running: string | null = null;
let expiry: ReturnType<typeof setTimeout> | undefined;

// Set once the row is on the page. Before that a redraw has nowhere to put anything, which is the case
// while the language and the frame names are still being asked for.
let draw: (plate: Plate, lang: Lang) => void = () => {};

/** Put the row up again from what is known now. */
function redraw(): void {
  const { now, changeover: next } = nowOf(held, finished, changeover, Date.now());
  changeover = next;
  draw(
    {
      name: names.get(ONLY_FRAME) ?? null,
      now,
      say: sayOf(held, running === null ? undefined : sessions.get(running)),
    },
    lang,
  );
  // "Just finished" comes down on its own, so something has to come back for it. Nothing else on the
  // row expires, and a redraw that finds it already down does nothing.
  clearTimeout(expiry);
  if (now.kind === "finished" && next.shownAt !== null) {
    expiry = setTimeout(redraw, next.shownAt + FINISHED_HOLD_MS - Date.now());
  }
}

/** Ask the ledger what this pane's session is on, then draw it. */
function readWork(): void {
  const session = running;
  if (session === null) return;
  void invoke<SessionWorkDto>("session_work", { session })
    .then(async (work) => {
      const tasks = work.holding.length
        ? await invoke<TaskCardDto[]>("tasks_by_ids", { ids: work.holding })
        : [];
      // A session that ended while the read was out is one whose row is no longer ours to move.
      if (running !== session) return;
      held = tasks;
      finished = work.finished;
      redraw();
    })
    .catch(() => {});
}

// The pane. A terminal is a live process, so a failure to start one is not a thing to swallow — the
// window would come up empty and say nothing about why. What the host refused with goes on the page
// instead, in the only place there is to put it.
const root = document.getElementById("root");
if (root) {
  root.className = "talk";
  const plate = document.createElement("div");
  root.append(plate);
  draw = mountNameplate(plate);
  redraw();

  const pane = document.createElement("div");
  pane.className = "talk__pane";
  root.append(pane);
  void mountTerminal(pane, {
    opened: (session, startedAt) => {
      sessions = opened(sessions, { session, startedAt });
      running = session;
      readWork();
    },
    said: (statement) => {
      sessions = said(sessions, statement);
      redraw();
    },
    closed: (session) => {
      sessions = closed(sessions, session);
      if (running === session) running = null;
      held = [];
      redraw();
    },
    name,
  }).catch((e: unknown) => {
    pane.className = "talk__failed";
    pane.textContent = e instanceof Error ? e.message : String(e);
  });

  // A reservation is made by the agent in the pane, running the CLI — a write from outside this
  // webview, which is what the host says has happened when the store changes on disk.
  void import("@tauri-apps/api/event")
    .then(({ listen }) => listen("store-changed", () => readWork()))
    .catch(() => {});
}
