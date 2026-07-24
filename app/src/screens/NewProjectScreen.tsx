// The project creation screen, in two steps: the form and the done step.
// Form step: collect a name (required) and a folder (optional — one can be added later), then
// create. Choosing a folder links it to the new project (a `.amenbo` pointer plus the AI guide land
// there, so an AI started in that folder can operate this project). A successful create does not
// jump straight to the board; it shows the done step.
// Done step: "Created project X", plus — when a folder was linked — what that enables and where it
// is, plus the next moves (copy `amenbo status`, open in the terminal or Finder, or add a folder if
// there is none). The primary action is "Open the board". This carries the same information as the
// CLI's own init/bind success output.
import { useState } from "react";
import { useCliCommandName } from "../core/cliCommand";
import { createProject, openTerminal, pickFolder, revealFolder } from "../core/mutations";
import { inTauri } from "../core/snapshot";
import { errText, t, tf } from "../core/i18n";
import { isEnterSubmit } from "../core/keys";
import type { Nav } from "../shell/AppShell";

// The project that was created, handed to the done step: id = where the board opens, name = the heading, dir = the linked folder or null.
type Created = { id: number; name: string; dir: string | null };

export function NewProjectScreen({ onCreated, onCancel }: { onCreated: (nav: Nav) => void; onCancel: () => void }) {
  const [name, setName] = useState("");
  const [dir, setDir] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [created, setCreated] = useState<Created | null>(null);
  const canCreate = name.trim().length > 0 && !busy;

  const chooseFolder = async () => {
    try {
      const picked = await pickFolder();
      if (picked) { setDir(picked); setError(null); }
    } catch (e) {
      setError(errText(e));
    }
  };

  const create = async () => {
    if (!canCreate) return;
    setBusy(true);
    setError(null);
    try {
      const id = await createProject(name.trim(), dir);
      if (id) setCreated({ id, name: name.trim(), dir });
      else onCancel();
    } catch (e) {
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  };

  if (created) {
    return <DoneStep created={created} onOpenBoard={() => onCreated({ type: "project", id: String(created.id) })} />;
  }

  return (
    <>
      <div className="board__toolbar">
        <span className="board__title">🆕 {t("newproj.title")}</span>
      </div>
      <div className="newproj">
        <label className="newproj__field">
          <span className="newproj__label">{t("newproj.nameLabel")}</span>
          <input
            className="newproj__input"
            autoFocus
            value={name}
            placeholder={t("side.newProjectPh")}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => { if (isEnterSubmit(e)) void create(); }}
          />
        </label>

        {inTauri() && (
          <div className="newproj__field">
            <span className="newproj__label">{t("newproj.folderLabel")}</span>
            <span className="newproj__hint">{t("newproj.folderHint")}</span>
            {dir ? (
              <div className="newproj__folder">
                <code className="newproj__path">{dir}</code>
                <button className="btn" onClick={() => void chooseFolder()} disabled={busy}>{t("newproj.changeFolder")}</button>
                <button className="btn" onClick={() => setDir(null)} disabled={busy}>{t("newproj.clearFolder")}</button>
              </div>
            ) : (
              <button className="btn" onClick={() => void chooseFolder()} disabled={busy}>📂 {t("newproj.chooseFolder")}</button>
            )}
          </div>
        )}

        {error && <div className="newproj__error" role="alert">⚠ {error}</div>}

        <div className="newproj__actions">
          <button className="btn btn--primary" onClick={() => void create()} disabled={!canCreate}>{t("newproj.create")}</button>
          <button className="btn" onClick={onCancel} disabled={busy}>{t("newproj.cancel")}</button>
        </div>
      </div>
    </>
  );
}

/**
 * The done step: "Created project X", plus — when a folder was linked — "an AI started in this folder
 * can operate this project", the path, and the next moves, with "Open the board" as the primary
 * action. The next moves only appear under Tauri, on the desktop; in the browser they would be no-ops,
 * so they are hidden. With no folder there is no `.amenbo` for anything to resolve through — neither
 * status nor a terminal can point at this project — so instead of next moves it invites the user to
 * add a folder.
 */
function DoneStep({ created, onOpenBoard }: { created: Created; onOpenBoard: () => void }) {
  const { name, dir } = created;
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // What is copied is meant to be pasted into a terminal, so it has to be the command this build installs.
  const cli = useCliCommandName();

  const copyStatus = async () => {
    try {
      await navigator.clipboard.writeText(`${cli} status`);
      setError(null);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch (e) {
      setError(errText(e));
    }
  };
  const reveal = async () => { try { if (dir) await revealFolder(dir); } catch (e) { setError(errText(e)); } };
  const terminal = async () => { try { if (dir) await openTerminal(dir); } catch (e) { setError(errText(e)); } };

  return (
    <>
      <div className="board__toolbar">
        <span className="board__title">✅ {tf("newproj.doneTitle", { name })}</span>
      </div>
      <div className="newproj newproj--done">
        {dir ? (
          <div className="newproj__capability">
            <p>{t("newproj.doneCapability")}</p>
            <code className="newproj__path">{dir}</code>
          </div>
        ) : (
          <div className="newproj__capability">
            <p className="muted">{t("newproj.doneNoFolder")}</p>
          </div>
        )}

        {inTauri() && dir && (
          <div className="newproj__next">
            <span className="newproj__label">{t("newproj.nextTitle")}</span>
            <div className="newproj__nextrow">
              <button className="btn" onClick={() => void copyStatus()}>
                {copied ? t("newproj.copied") : `📋 ${tf("newproj.copyStatus", { cmd: cli })}`}
              </button>
              <button className="btn" onClick={() => void terminal()}>⌨️ {t("newproj.openTerminal")}</button>
              <button className="btn" onClick={() => void reveal()}>📂 {t("newproj.openFinder")}</button>
            </div>
          </div>
        )}

        {error && <div className="newproj__error" role="alert">⚠ {error}</div>}

        <div className="newproj__actions">
          <button className="btn btn--primary" autoFocus onClick={onOpenBoard}>{t("newproj.openBoard")}</button>
        </div>
      </div>
    </>
  );
}
