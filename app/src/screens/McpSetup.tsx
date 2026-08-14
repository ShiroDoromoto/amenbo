// The way to reach this project from an AI whose host cannot open a folder (`AMB-D-671`).
//
// It stands on two screens — the one that just made a project, and the one that looks after an old
// one — and says the same thing on both. MCP takes a setting per project, so a reader who has just
// made one is exactly the reader who needs it; and a folder moves, a tool is swapped, a project is
// finished with, so the reader of an old project needs it too.
//
// **Folded away, because most readers do not need it.** Somebody working from the command line has
// amenbo already; the offer is for the reader whose AI cannot open a folder at all. Folded, it is a
// line they can walk past.
//
// **The apps are listed, never asked about.** Which one a reader uses is not remembered and not
// guessed: they change tools, and a remembered answer is wrong by the next day (`AMB-D-671`). What is
// read off disk is the other question — whether an app already holds this project — and that is a
// fact about the settings, not a preference about the reader.
//
// **Two roads, one per row** (`AMB-D-672`). The app that cannot run a command is handed a file to
// open; the rest have an AI of their own, which is given the request and does the merge. Which button
// a row draws is the catalog's word, not this screen's.
import { useCallback, useEffect, useState } from "react";
import { fetchMcpApps, saveMcpBundle } from "../core/mutations";
import { errText, t, tf } from "../core/i18n";
import type { McpAppDto } from "../bindings/bindings";

/**
 * `projectId` is the project being set up — every row answers about that one, and the server the
 * reader ends up with is bound to its folder.
 *
 * It draws nothing at all where there is nothing to offer: a project with no folder bound to it has
 * nowhere to point a server, and the browser iteration has no app on the machine to ask about. A
 * heading over an empty list would be a promise with nothing behind it.
 */
export function McpSetup({ projectId }: { projectId: number }) {
  const [apps, setApps] = useState<McpAppDto[]>([]);
  const [open, setOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(() => {
    let alive = true;
    fetchMcpApps(projectId)
      .then((rows) => { if (alive) setApps(rows); })
      .catch(() => {}); // A list that could not be read is not a fault to report on this screen.
    return () => { alive = false; };
  }, [projectId]);

  useEffect(() => {
    setOpen(false);
    setError(null);
    return load();
  }, [projectId, load]);

  if (apps.length === 0) return null;

  return (
    <div className="mcp">
      <div className="newproj__label">{t("mcp.title")}</div>
      {/* A disclosure rather than a link to somewhere else: what is behind it is the whole of the
          subject, and a reader who opens it is already where they are going. */}
      <button className="btn mcp__toggle" aria-expanded={open} onClick={() => setOpen(!open)}>
        {open ? "▾" : "▸"} {t("mcp.open")}
      </button>

      {open && (
        <>
          <span className="newproj__hint">{t("mcp.hint")}</span>
          {error && <div className="newproj__error" role="alert">⚠ {error}</div>}
          <ul className="mcp__apps">
            {apps.map((app) => (
              <McpAppRow key={app.app} app={app} projectId={projectId} onError={setError} onWritten={load} />
            ))}
          </ul>
        </>
      )}
    </div>
  );
}

/**
 * One app's row: what it already holds, and the one move that changes it.
 *
 * **The folder is drawn beside "set up" and not instead of it.** Set up for *which* folder is the half
 * a reader cannot work out for themselves, and an entry someone edited into naming none is still an
 * entry — so the two are said separately (`AMB-D-673`).
 *
 * **The removal is offered only where there is something to remove.** A request to delete an entry
 * nobody has would send a reader looking through a file for a line that is not in it.
 */
function McpAppRow({
  app,
  projectId,
  onError,
  onWritten,
}: {
  app: McpAppDto;
  projectId: number;
  onError: (message: string | null) => void;
  onWritten: () => void;
}) {
  // Which text was last copied, by what it was — copying the removal after the addition should not
  // leave the first button still saying it was copied.
  const [copied, setCopied] = useState<"add" | "remove" | null>(null);
  const [busy, setBusy] = useState(false);
  const [saved, setSaved] = useState<string | null>(null);

  const copy = async (which: "add" | "remove", text: string) => {
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
      const at = await saveMcpBundle(projectId);
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
        {app.configured && app.folder && <code className="newproj__path">{app.folder}</code>}
      </div>
      <div className="mcp__actions">
        {app.writesFile ? (
          <button className="btn" disabled={busy} onClick={() => void save()}>{t("mcp.write")}</button>
        ) : (
          <button className="btn" onClick={() => void copy("add", app.addRequest)}>
            {copied === "add" ? t("mcp.copied") : t("mcp.copyAdd")}
          </button>
        )}
        {app.configured && !app.writesFile && (
          <button className="btn" onClick={() => void copy("remove", app.removeRequest)}>
            {copied === "remove" ? t("mcp.copied") : t("mcp.copyRemove")}
          </button>
        )}
      </div>
      {saved && <div className="mcp__saved">{tf("mcp.written", { path: saved })}</div>}
    </li>
  );
}
