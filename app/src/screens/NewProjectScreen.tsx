// The project creation screen, in two steps: the form and the done step.
// Form step: collect a name and a folder, both required on the desktop, then create. The folder is
// what links the project to a place (a `.amenbo` pointer plus the AI guide land there, so an AI
// started in that folder can operate this project) — and a project with none is one no AI can reach,
// so the create waits for it (`AMB-D-532`). The field says what the folder buys, which is also the
// answer to why the button is not pressable yet. A successful create does not jump straight to the
// board; it shows the done step.
// Done step: "Created project X", what the folder enables and where it is, and then the first loop
// (`FirstLoop`), which is what the reader is meant to do next. The rest (reveal the folder, copy
// `amenbo status`, the way out to where an AI is connected over MCP) sits below it as a side offer.
// The primary action is "Open the board". This carries the same information as the CLI's own init/bind
// success output.
//
// The browser iteration is another thing: it writes no store and offers no folder field, so there is
// nothing there for a folder to bind and the create goes through on a name alone.
import { useState } from "react";
import { FirstLoop } from "../components/FirstLoop";
import { useCliCommandName } from "../core/cliCommand";
import { createProject, pickFolder, revealFolder } from "../core/mutations";
import { inTauri } from "../core/snapshot";
import { errText, t, tf } from "../core/i18n";
import { asTyped, isEnterSubmit } from "../core/keys";
import type { Nav } from "../shell/AppShell";
import { ErrorNote } from "../components/ErrorNote";

// The project that was created, handed to the done step: id = where the board opens, name = the heading, dir = the linked folder or null.
type Created = { id: number; name: string; dir: string | null };

export function NewProjectScreen({ onCreated, onCancel, onOpenMcp }: { onCreated: (nav: Nav) => void; onCancel: () => void; onOpenMcp: (projectId: number) => void }) {
  const [name, setName] = useState("");
  const [dir, setDir] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [created, setCreated] = useState<Created | null>(null);
  // The folder is asked for where there is one to ask about — on the desktop — and it is required there.
  const canCreate = name.trim().length > 0 && !busy && (!inTauri() || dir !== null);

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
    return (
      <DoneStep
        created={created}
        onOpenBoard={() => onCreated({ type: "project", id: String(created.id) })}
        onOpenMcp={onOpenMcp}
      />
    );
  }

  return (
    <>
      <div className="board__toolbar">
        <span className="board__title">🆕 {t("newproj.title")}</span>
      </div>
      <div className="newproj">
        <label className="newproj__field">
          <span className="fieldlabel">{t("newproj.nameLabel")}</span>
          <input
            {...asTyped}
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
            <span className="fieldlabel">{t("newproj.folderLabel")}</span>
            <span className="hint">{t("newproj.folderHint")}</span>
            {/* Changing the choice, but not undoing it: clearing would put the form back in the one
                state it cannot be created from, which is not a move worth offering. */}
            {dir ? (
              <div className="newproj__folder">
                <code className="path">{dir}</code>
                <button className="btn" onClick={() => void chooseFolder()} disabled={busy}>{t("newproj.changeFolder")}</button>
              </div>
            ) : (
              <button className="btn" onClick={() => void chooseFolder()} disabled={busy}>📂 {t("newproj.chooseFolder")}</button>
            )}
          </div>
        )}

        {error && <ErrorNote>{error}</ErrorNote>}

        <div className="newproj__actions">
          <button className="btn btn--primary" onClick={() => void create()} disabled={!canCreate}>{t("newproj.create")}</button>
          <button className="btn" onClick={onCancel} disabled={busy}>{t("newproj.cancel")}</button>
        </div>
      </div>
    </>
  );
}

/**
 * The done step: "Created project X", then "an AI started in this folder can operate this project",
 * the path, and the first loop, with "Open the board" as the primary action. All of it is about the
 * folder, and on the desktop there always is one (`AMB-D-532`). In the browser there is none — opening
 * a terminal or a file manager would be a no-op there anyway — so the step is the heading and the way
 * on to the board.
 */
function DoneStep({ created, onOpenBoard, onOpenMcp }: { created: Created; onOpenBoard: () => void; onOpenMcp: (projectId: number) => void }) {
  const { id, name, dir } = created;
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

  return (
    <>
      <div className="board__toolbar">
        <span className="board__title">✅ {tf("newproj.doneTitle", { name })}</span>
      </div>
      <div className="newproj newproj--done">
        {dir && (
          <div className="newproj__capability">
            <p>{t("newproj.doneCapability")}</p>
            <code className="path">{dir}</code>
          </div>
        )}

        {inTauri() && dir && (
          <>
            <FirstLoop dir={dir} />
            <div className="newproj__next">
              <span className="fieldlabel">{t("newproj.moreTitle")}</span>
              <div className="newproj__nextrow">
                <button className="btn" onClick={() => void reveal()}>📂 {t("newproj.openFinder")}</button>
                <button className="btn" onClick={() => void copyStatus()}>
                  {copied ? `✓ ${t("newproj.copied")}` : `📋 ${tf("newproj.copyStatus", { cmd: cli })}`}
                </button>
                {/* One line out to where an AI is connected, rather than a second place to hand the
                    request over from (`AMB-D-684`). It is named and not conditioned: the fold it
                    replaced asked the reader whether their AI can open a folder, which is the one
                    thing they cannot answer having just made the project. The project goes with it,
                    so the screen opens holding the one they came from. */}
                <button className="btn" onClick={() => onOpenMcp(id)}>🔗 {t("nav.mcp")}</button>
              </div>
            </div>
          </>
        )}

        {error && <ErrorNote>{error}</ErrorNote>}

        <div className="newproj__actions">
          <button className="btn btn--primary" autoFocus onClick={onOpenBoard}>{t("newproj.openBoard")}</button>
        </div>
      </div>
    </>
  );
}
