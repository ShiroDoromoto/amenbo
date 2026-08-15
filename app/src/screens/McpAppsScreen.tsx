// Where an AI is connected to amenbo (`AMB-D-681`).
//
// **The screen is the app's, not the project's.** A server is one per app, and it reaches as many
// folders as the reader chose (`AMB-D-679`) — so the question this screen asks is "which projects may
// this app reach", once per app, rather than "which apps hold this project", once per project. An
// entrance on each project would put the reader in front of the second setup asking amenbo to work
// out whether they meant to add or to replace.
//
// **What a row hands over is the whole selection, every time.** The request asks for the entry to be
// put in place of whatever is there, so the second time round is the same move as the first, and what
// is in the file afterwards is exactly what is ticked here.
//
// **Two roads, one per row** (`AMB-D-672`). The app that cannot run a command is handed a file to
// open; the rest have an AI of their own, which is given the request and does the merge. Which button
// a row draws is the catalog's word, not this screen's.
import { useCallback, useEffect, useMemo, useState } from "react";
import { fetchMcpRequest, fetchMcpSetup, saveMcpBundle } from "../core/mutations";
import { errText, t, tf } from "../core/i18n";
import type { McpAppDto, McpProjectDto, McpSetupDto } from "../bindings/bindings";

/**
 * `pick` is a project the screen arrives already holding — the one a reader just created and walked
 * here from (`AMB-D-684`). It is ticked on top of what each app already reaches rather than instead of
 * it, so arriving this way never quietly drops a folder an app was set up for. Nothing is written
 * until a row's own button is pressed, so a tick the reader did not mean costs an untick.
 */
export function McpAppsScreen({ pick = null }: { pick?: number | null }) {
  const [setup, setSetup] = useState<McpSetupDto | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(() => {
    let alive = true;
    fetchMcpSetup()
      .then((read) => { if (alive) setSetup(read); })
      .catch(() => {}); // A list that could not be read is not a fault to report on this screen.
    return () => { alive = false; };
  }, []);

  useEffect(() => load(), [load]);

  return (
    <div className="settings mcp">
      <div className="settings__h">{t("mcp.title")}</div>
      <span className="newproj__hint">{t("mcp.hint")}</span>
      {error && <div className="newproj__error" role="alert">⚠ {error}</div>}

      {/* A project with no folder bound has nowhere to point a server, so a screen with none of them
          says that rather than drawing rows whose ticks would write an entry naming nothing. */}
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
              onError={setError}
              onWritten={load}
            />
          ))}
        </ul>
      )}
    </div>
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
 */
function McpAppRow({
  app,
  projects,
  pick,
  onError,
  onWritten,
}: {
  app: McpAppDto;
  projects: McpProjectDto[];
  pick: number | null;
  onError: (message: string | null) => void;
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
  // leave the first button still saying it was copied.
  const [copied, setCopied] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [saved, setSaved] = useState<string | null>(null);

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
      onError(null);
      setCopied(which);
      setTimeout(() => setCopied(null), 1200);
    } catch { /* where the clipboard is unavailable, quietly skip */ }
  };

  // The one button here that writes anything, and what it writes is a file of the reader's own — not
  // a settings file of the app's. Where it landed is said afterwards, because opening it is the next
  // move and a file nobody can find is one nobody opens.
  const save = async () => {
    setBusy(true);
    onError(null);
    try {
      const at = await saveMcpBundle(picked);
      if (at !== null) {
        setSaved(at);
        onWritten();
      }
    } catch (e) {
      onError(errText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <li className="mcp__app">
      <div className="mcp__name">{app.label}</div>
      <div className={app.configured ? "mcp__state" : "mcp__state faint"}>
        {app.configured ? t("mcp.configured") : t("mcp.unconfigured")}
        {app.folders.map((at) => <code className="newproj__path" key={at}>{at}</code>)}
      </div>

      <div className="mcp__pick">
        <div className="newproj__label">{t("mcp.projects")}</div>
        {projects.map((project) => (
          <label className="mcp__project" key={project.id}>
            <input
              type="checkbox"
              checked={picked.includes(project.id)}
              onChange={() => toggle(project.id)}
            />
            {project.name}
            <code className="newproj__path">{project.folder}</code>
          </label>
        ))}
      </div>

      <div className="mcp__actions">
        {app.writesFile ? (
          <button className="btn" disabled={busy || picked.length === 0} onClick={() => void save()}>
            {t("mcp.write")}
          </button>
        ) : (
          <button className="btn" onClick={() => void copy("add", texts.add)}>
            {copied === "add" ? t("mcp.copied") : t("mcp.copyAdd")}
          </button>
        )}
        {/* Offered only where there is something to remove: a request to delete an entry nobody has
            would send a reader looking through a file for a line that is not in it. */}
        {app.configured && !app.writesFile && (
          <button className="btn" onClick={() => void copy("remove", texts.remove)}>
            {copied === "remove" ? t("mcp.copied") : t("mcp.copyRemove")}
          </button>
        )}
      </div>
      {saved && <div className="mcp__saved">{tf("mcp.written", { path: saved })}</div>}

      {/* What an older amenbo left behind (`AMB-D-679`), drawn apart from the row's own state: an old
          entry is not this app being set up, it is something to take away. */}
      {app.stale.length > 0 && (
        <div className="mcp__stale">
          <div className="newproj__label">{t("mcp.stale")}</div>
          {app.stale.map((old) => (
            <div className="mcp__staleRow" key={old.name}>
              <code className="newproj__path">{old.name}</code>
              {old.folder && <code className="newproj__path">{old.folder}</code>}
              <button className="btn" onClick={() => void copy(old.name, old.removeRequest)}>
                {copied === old.name ? t("mcp.copied") : t("mcp.copyRemove")}
              </button>
            </div>
          ))}
        </div>
      )}
    </li>
  );
}
