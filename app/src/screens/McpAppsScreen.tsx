// Where an AI is connected to Amenbo (`AMB-D-681`).
//
// **The screen is the app's, not the project's.** A server is one per app, and it reaches as many
// folders as the reader chose (`AMB-D-679`) — so the question this screen asks is "which projects may
// this app reach", once per app, rather than "which apps hold this project", once per project. An
// entrance on each project would put the reader in front of the second setup asking Amenbo to work
// out whether they meant to add or to replace.
//
// **What a row hands over is the whole selection, every time.** The request asks for the entry to be
// put in place of whatever is there, so the second time round is the same move as the first, and what
// is in the file afterwards is exactly what is ticked here.
//
// **Two roads, one per row** (`AMB-D-672`). The app that cannot run a command is handed a file to
// open; the rest have an AI of their own, which is given the request and does the merge. Which button
// a row draws is the catalog's word, not this screen's.
//
// **It wears the same shell as every other screen** (`AMB-D-690`): the heading in a `board__toolbar`
// band on top, the body inside a `settings__section` card. The heading says what the sidebar entrance
// says, so a reader who pressed "connect via MCP" arrives at that name rather than at another one.
//
// **Reading, unread and empty are three answers, not one** (`AMB-D-690`). Each says its own line, and
// a failure says it where it happened: the read's on the card, a write's on the row whose button wrote.
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { fetchMcpRequest, fetchMcpSetup, saveMcpBundle } from "../core/mutations";
import { errText, t, tf } from "../core/i18n";
import type { McpAppDto, McpProjectDto, McpSetupDto } from "../bindings/bindings";
import { ErrorNote } from "../components/ErrorNote";
import { Icon } from "../components/Icon";

// A burst of returns is one re-read, on the window the reconcile triggers already use (`core/snapshot.ts`).
const REREAD_THROTTLE_MS = 1500;

/**
 * `pick` is a project the screen arrives already holding — the one a reader just created and walked
 * here from (`AMB-D-684`). It is ticked on top of what each app already reaches rather than instead of
 * it, so arriving this way never quietly drops a folder an app was set up for. Nothing is written
 * until a row's own button is pressed, so a tick the reader did not mean costs an untick.
 */
export function McpAppsScreen({ pick = null }: { pick?: number | null }) {
  const [setup, setSetup] = useState<McpSetupDto | null>(null);
  // The read that failed, in its own words (`AMB-D-690`). A list nobody could read is not a list with
  // nothing in it, and neither is the moment before the answer — three answers a card standing empty
  // cannot tell apart, which is why each says its own line.
  const [unread, setUnread] = useState<string | null>(null);
  // Which app is open, at most one (`AMB-D-690`). It starts closed even where the screen was walked in
  // holding a project: what that project is ticked in is a row the reader opens, and guessing one of
  // eight for them would put a selection in front of them they did not ask to see.
  const [open, setOpen] = useState<string | null>(null);
  const lastReadAt = useRef(0);

  const load = useCallback(() => {
    lastReadAt.current = Date.now();
    let alive = true;
    fetchMcpSetup()
      .then((read) => { if (alive) { setSetup(read); setUnread(null); } })
      // A re-read that failed keeps the rows it last read: they are stale, but they are what the
      // screen knows, and the line above them says the newer answer never came.
      .catch((e) => { if (alive) setUnread(errText(e)); });
    return () => { alive = false; };
  }, []);

  useEffect(() => load(), [load]);

  // Read it again when the reader comes back to the window. On the request road the writer is the
  // other app's AI, so `onWritten` never fires here; and `mcp_probe` reads those settings files
  // directly rather than the store, so `store-changed` does not carry the change either. Coming back
  // is the only moment Amenbo has, and one read then is enough — a row still saying "not set up" is
  // read as a failure, and sends the reader to hand over the same request a second time. Both events
  // are listened for, as `installReconcileTriggers` does, since a window can come back by either.
  useEffect(() => {
    let drop: (() => void) | null = null;
    const onReturn = () => {
      if (Date.now() - lastReadAt.current < REREAD_THROTTLE_MS) return;
      drop?.();
      drop = load();
    };
    const onVisible = () => { if (document.visibilityState === "visible") onReturn(); };
    window.addEventListener("focus", onReturn);
    document.addEventListener("visibilitychange", onVisible);
    return () => {
      window.removeEventListener("focus", onReturn);
      document.removeEventListener("visibilitychange", onVisible);
      drop?.();
    };
  }, [load]);

  return (
    <>
      <div className="board__toolbar">
        <span className="board__title"><Icon name="link" size="md" /> {t("mcp.title")}</span>
      </div>

      <div className="settings">
        <div className="settings__section">
          <div className="settings__body">
            <span className="hint">{t("mcp.hint")}</span>

            {/* The wait says it is a wait. Until the first read answers there is nothing to draw, and
                a card standing empty is read as an answer — that this machine has no apps to set up. */}
            {setup === null && unread === null && (
              <div className="mcp__loading">{t("app.loading")}</div>
            )}

            {/* Said in amenbo's words with the door's own beneath, as the app says it elsewhere: the
                first line is what happened, the second is the only part that says why. */}
            {unread && (
              <ErrorNote>
                {t("app.loadError")}
                <div className="faint">{unread}</div>
              </ErrorNote>
            )}

            {/* A project with no folder bound has nowhere to point a server, so a screen with none of
                them says that rather than drawing rows whose ticks would write an entry naming
                nothing. */}
            {setup && setup.projects.length === 0 && (
              <div className="mcp__empty">{t("mcp.noProjects")}</div>
            )}

            {setup && setup.projects.length > 0 && (
              <ul className="mcp__apps">
                {setup.apps.map((app) => (
                  <McpAppRow
                    key={app.app}
                    app={app}
                    projects={setup.projects}
                    pick={pick}
                    open={open === app.app}
                    onToggle={() => setOpen((was) => (was === app.app ? null : app.app))}
                    onWritten={load}
                  />
                ))}
              </ul>
            )}
          </div>
        </div>
      </div>
    </>
  );
}

/**
 * One app's row: which projects it may reach, what it already holds, and the one move that changes it.
 *
 * **The ticks open on what the app already reaches.** The row is read off the settings themselves, so
 * a reader who has set this app up before arrives at their own selection rather than at an empty one
 * they would have to rebuild before touching anything.
 *
 * **The folders are drawn beside "set up" and not instead of it.** Set up for *which* folders is the
 * half a reader cannot work out for themselves, and an entry someone edited into naming none is still
 * an entry — so the two are said separately (`AMB-D-673`).
 *
 * **Folded, and what shows folded is the reading** (`AMB-D-690`): the app's name, whether it is set up,
 * and the folders it reaches now. That is the whole of what a reader scanning the list is after, and it
 * is the same three things the row would be judged by open. Choosing is behind the fold, because the
 * choice is one app's and the list is eight of them — every row unfolded put the same column of
 * projects on the screen once per app, which is a page with nowhere to read.
 *
 * The row keeps its own ticks whether it is open or shut, so a reader who folded one away and came back
 * to it finds the selection they left rather than one rebuilt from the settings.
 *
 * **A write that failed is said here** (`AMB-D-690`), beside the button that failed. Eight rows draw
 * the same two buttons, so one message on top of the screen names none of them — and the row that has
 * it is also the row holding the ticks the reader would change before pressing again.
 */
function McpAppRow({
  app,
  projects,
  pick,
  open,
  onToggle,
  onWritten,
}: {
  app: McpAppDto;
  projects: McpProjectDto[];
  pick: number | null;
  open: boolean;
  onToggle: () => void;
  onWritten: () => void;
}) {
  // Which projects this app may reach. It opens on the ones whose folder its entry already names —
  // matched on the folder, because that is what an entry carries and what a project is reached by —
  // and on the project the reader walked in holding, if the screen knows one and it has a folder.
  const reached = useMemo(() => {
    const already = projects.filter((p) => app.folders.includes(p.folder)).map((p) => p.id);
    const walkedIn = pick !== null && projects.some((p) => p.id === pick) && !already.includes(pick);
    return walkedIn ? [...already, pick] : already;
  }, [app.folders, projects, pick]);
  const [picked, setPicked] = useState<number[]>(reached);
  const [texts, setTexts] = useState<{ add: string; remove: string }>({ add: "", remove: "" });
  // Which text was last copied, by what it was — copying the removal after the addition should not
  // leave the addition's notice standing beside a button nobody pressed.
  const [copied, setCopied] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [saved, setSaved] = useState<string | null>(null);
  const [failed, setFailed] = useState<string | null>(null);

  // The texts follow the ticks rather than the button, so copying is a synchronous move on something
  // the row is already holding.
  useEffect(() => {
    let alive = true;
    fetchMcpRequest(app.app, picked)
      .then((read) => { if (alive) setTexts(read); })
      .catch(() => {});
    return () => { alive = false; };
  }, [app.app, picked]);

  const toggle = (id: number) =>
    setPicked((was) => (was.includes(id) ? was.filter((one) => one !== id) : [...was, id]));

  const copy = async (which: string, text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(which);
      setTimeout(() => setCopied(null), 1200);
    } catch { /* where the clipboard is unavailable, quietly skip */ }
  };

  // The one button here that writes anything, and what it writes is a file of the reader's own — not
  // a settings file of the app's. Where it landed is said afterwards, because opening it is the next
  // move and a file nobody can find is one nobody opens.
  const save = async () => {
    setBusy(true);
    // Both are what the last press left, and this press is not it: a place a file once landed, still
    // said beside a write that has just failed, reads as this write having landed there.
    setFailed(null);
    setSaved(null);
    try {
      const at = await saveMcpBundle(picked);
      if (at !== null) {
        setSaved(at);
        onWritten();
      }
    } catch (e) {
      setFailed(errText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <li className="mcp__app">
      {/* The whole folded row is what opens it: the three things it says are also the reason to open
          it, so there is nothing beside them for a separate control to point at. */}
      <button className="mcp__head" aria-expanded={open} onClick={onToggle}>
        <span className="mcp__headtext">
          <span className="mcp__name">{app.label}</span>
          <span className={app.configured ? "mcp__state" : "mcp__state faint"}>
            {app.configured ? t("mcp.configured") : t("mcp.unconfigured")}
            {app.folders.map((at) => <code className="path" key={at}>{at}</code>)}
          </span>
        </span>
        <span className="faint"><Icon name={open ? "chevronDown" : "chevronRight"} /></span>
      </button>

      {open && (
        <>
          <div className="mcp__pick">
            <div className="fieldlabel">{t("mcp.projects")}</div>
            {projects.map((project) => (
              <label className="mcp__project" key={project.id}>
                <input
                  type="checkbox"
                  checked={picked.includes(project.id)}
                  onChange={() => toggle(project.id)}
                />
                {project.name}
                <code className="path">{project.folder}</code>
              </label>
            ))}
          </div>

          {/* What the ticks above amount to, said beside the button that acts on them: they are not a
              filter on this screen, they are the contents of what is about to be handed over, and
              handing it over replaces rather than adds. Both halves are the reader's to know before
              they press, and neither is recoverable from the button's own word. */}
          <span className="hint">{t("mcp.handover")}</span>

          <div className="mcp__actions">
            {/* Nothing ticked is nothing to hand over, on both roads alike: the file names no folder,
                and the request carries a `--dir` with no value after it, which an AI writes into the
                settings as an entry that cannot run. The two used to answer the same emptiness
                differently — one shut, one live. */}
            {app.writesFile ? (
              <button className="btn" disabled={busy || picked.length === 0} onClick={() => void save()}>
                {t("mcp.write")}
              </button>
            ) : (
              <button
                className="btn"
                disabled={picked.length === 0}
                onClick={() => void copy("add", texts.add)}
              >
                {t("mcp.copyAdd")}
              </button>
            )}
            {/* Offered only where there is something to remove: a request to delete an entry nobody
                has would send a reader looking through a file for a line that is not in it. It is not
                shut on an empty selection, the ticks being no part of it — what it asks for is the
                whole entry gone, and a reader taking amenbo out has nothing to tick first. */}
            {app.configured && !app.writesFile && (
              <button className="btn" onClick={() => void copy("remove", texts.remove)}>
                {t("mcp.copyRemove")}
              </button>
            )}
            {!app.writesFile && <Copied on={copied === "add" || copied === "remove"} />}
          </div>
          {failed && <ErrorNote>{failed}</ErrorNote>}
          {saved && <div className="mcp__saved">{tf("mcp.written", { path: saved })}</div>}

          {/* What an older amenbo left behind (`AMB-D-679`), drawn apart from the row's own state: an
              old entry is not this app being set up, it is something to take away. */}
          {app.stale.length > 0 && (
            <div className="mcp__stale">
              <div className="fieldlabel">{t("mcp.stale")}</div>
              {app.stale.map((old) => (
                <div className="mcp__staleRow" key={old.name}>
                  <code className="path">{old.name}</code>
                  {old.folder && <code className="path">{old.folder}</code>}
                  <button className="btn" onClick={() => void copy(old.name, old.removeRequest)}>
                    {t("mcp.copyRemove")}
                  </button>
                  <Copied on={copied === old.name} />
                </div>
              ))}
            </div>
          )}
        </>
      )}
    </li>
  );
}

/**
 * That a copy went through, said beside the button instead of in it (`AMB-D-690`).
 *
 * A button that renames itself to "copied" is a button that has stopped saying what it does, in the
 * one moment a reader is looking at it to decide whether to press it again — and it changes width
 * doing so, taking the next button along with it. So the word lands outside, and after the buttons
 * rather than between them: the space it takes when it appears is space none of them was standing in.
 *
 * It is a live region because nothing else marks the move — the clipboard is silent, and the button
 * the reader pressed now looks exactly as it did before.
 */
function Copied({ on }: { on: boolean }) {
  return <span className="mcp__copied" role="status">{on ? t("mcp.copied") : ""}</span>;
}
