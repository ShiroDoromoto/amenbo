// The talk window's entry — the window the terminal is split out into, so the two faces can sit on
// two displays (`AMB-D-753`). It is not opened at launch and is not declared in `tauri.conf.json`:
// the app comes up as one window showing the board, and this one is built when it is asked for
// (`crate::windows`).
//
// **What is in it is the terminal face, whole.** The rail, the pages, the split, the files beside
// them — the same component the board puts up, with the same arrangement under it
// (`./shell/TerminalFace`). Splitting out is meant to put the terminal on another display, so a
// window that arrived there with one pane and a way back would be a person carrying a terminal out
// rather than moving where they work. The only difference is the one the reader asked for: the
// ledger is not in this window, and the button that says so folds the app back.
//
// **Nothing is handed over to build it.** Which panes there are, whose project each is and which one
// was being worked in are all kept with the arrangement, and the terminals still running are asked
// of the host — the same two questions the board answers when the app folds back into one window.
// So the split says nothing and the window reads everything, which is one answer rather than two
// that can disagree.
//
// The terminals it draws are the ones that were already running in the board, and folding back hands
// them over the same way. Neither end restarts anything — a pane is a drawing of a session, not the
// session (`./talk/terminal`).
//
// What it owns from the start is a name that says which face it holds. Two windows both titled
// "Amenbo" are indistinguishable in the window list, the taskbar and the switcher, and the words
// that tell them apart live in the webview's dictionary rather than in Rust (`AMB-D-396`) — so the
// title is set here rather than in `tauri.conf.json`, which can only hold one fixed string. The
// configured title is the product name, which is what shows for the moment before this runs.
//
// The store is read before anything is drawn, the way the board reads it (`./App`): the face names
// projects, offers folders and writes in the reader's language, and every one of those is the
// snapshot's to answer. A migration is not waited on here — this window is only ever built by a
// board that is already past one.
import { StrictMode, useEffect, useMemo, useState, useSyncExternalStore } from "react";
import { createRoot } from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { AppErrorBoundary } from "./components/AppErrorBoundary";
import { currentLang, errText, t, tf } from "./core/i18n";
import { invoke } from "./core/ipc";
import { notifyTurn } from "./core/osNotify";
import { RefNavProvider, type RefNav } from "./core/refNav";
import { loadSnapshot, subscribe, watchStore } from "./core/snapshot";
import { initTheme } from "./core/theme";
import { ElevationBand } from "./talk/elevation";
import { TerminalFace } from "./shell/TerminalFace";
import "./styles/tokens.css";
import "./styles/global.css";
import "./components/components.css";
import "./styles/talk.css";

initTheme();

/**
 * Name the window, so a list of windows tells this one from the board (`AMB-D-396`).
 *
 * The name is the face's, not the window's: what was split out is the terminal, whole, and the
 * button that splits it out says "terminal" too. A window with a name of its own would be a second
 * word for the one thing the reader is looking at — so the title is written from `face.terminal`,
 * the key every other place naming that face already reads.
 */
function retitle(): void {
  void getCurrentWindow()
    .setTitle(tf("app.talkWindow", { face: t("face.terminal") }))
    // Outside Tauri (`npm run dev` in a browser) there is no window to name, and a title that could
    // not be set is not worth failing an otherwise-working window over.
    .catch(() => {});
}

/**
 * The window: the terminal face, and above it anything standing that has to be said about the
 * process it runs in.
 *
 * A record clicked here is read on the board, which is the other window — so the navigation seam
 * goes to the host rather than to a screen this page has (`crate::windows::show_ref`). It is the
 * same road a ref clicked inside a pane takes (`./talk/terminal`), and for the same reason: a window
 * cannot raise its sibling.
 */
function TalkWindow() {
  const [elevated, setElevated] = useState(false);
  useEffect(() => {
    // Asked once, and answered for the life of the process: a token is inherited at launch and
    // cannot be handed back (`./talk/elevation`). Nothing answering is nothing established to warn
    // about — a band raised on a failed question would be shown in every browser this page opens in.
    void invoke<boolean>("elevated").then(setElevated).catch(() => {});
  }, []);

  const nav = useMemo<RefNav>(() => ({
    selectTask: (id) => void invoke("show_ref", { kind: "task", id }).catch(() => {}),
    selectDecision: (id) => {
      if (id !== null) void invoke("show_ref", { kind: "decision", id }).catch(() => {});
    },
  }), []);

  return (
    <RefNavProvider value={nav}>
      <div className="talk">
        {elevated && <ElevationBand />}
        <TerminalFace
          ownWindow
          // Fold the app back to one window. The board is told nothing: this window going is what
          // says it, whether it went from here or from the title bar (`crate::windows`).
          onWindow={() => void invoke("talk_close").catch(() => {})}
          // Nothing here is the shell's to say about the face — the window that could not be built
          // is the board's news, and this is the window that was.
          note={null}
          // This window *is* the terminal, so there is no face to be behind: what says the person is
          // not looking is the window not having the keyboard. A turn that comes up while they are
          // here is one they are already being shown, on the label above the pane (`AMB-D-753`).
          onWaiting={(waiting) => {
            if (waiting && !document.hasFocus()) void notifyTurn();
          }}
        />
      </div>
    </RefNavProvider>
  );
}

/** The page once the store has answered, or what stopped it answering. */
function TalkPage() {
  // The whole page is rebuilt when the language changes, the way the shell is (`./shell/AppShell`):
  // anything that took a translated word into its own state at mount keeps the old one across a
  // plain re-render, and only a remount re-reads it.
  const lang = useSyncExternalStore(subscribe, currentLang);
  const [ready, setReady] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    let unlisten: (() => void) | undefined;
    loadSnapshot()
      .then(() => { if (alive) setReady(true); })
      .catch((e: unknown) => { if (alive) setError(errText(e)); });
    // The store moves under this window all the time — the board writes it, and so does this face
    // binding a project's first folder. Held from the first read, the projects on the rail would be
    // the ones there were when the window opened.
    void watchStore().then((un) => { if (alive) unlisten = un; else un(); });
    return () => { alive = false; unlisten?.(); };
  }, []);

  useEffect(retitle, [lang, ready]);

  if (error !== null) {
    return (
      <div style={{ padding: 24, fontFamily: "system-ui", color: "#c0392b" }}>
        <strong>{t("app.loadError")}</strong>
        <pre style={{ whiteSpace: "pre-wrap" }}>{error}</pre>
      </div>
    );
  }
  if (!ready) return <div style={{ padding: 24, fontFamily: "system-ui", opacity: 0.6 }}>{t("app.loading")}</div>;
  return <TalkWindow key={lang} />;
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <AppErrorBoundary>
      <TalkPage />
    </AppErrorBoundary>
  </StrictMode>,
);
