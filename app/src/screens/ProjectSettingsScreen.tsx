// The project settings screen. Not a modal but a screen (a nav destination), taking rename / notes / colour /
// default-view edits plus archive, unarchive and delete across several sections. The snapshot's ProjectDto does not
// carry notes or archived, so we hydrate with `fetchProjectSettings` on open and prefill from that. Saving sends the
// diff (only the fields that changed) to `updateProject`. Destructive operations go through a plugin-dialog confirmation.
import { useEffect, useState } from "react";
import {
  bindFolder, deleteProject, fetchBoundFolders, fetchProjectSettings, openTerminal,
  pickFolder, revealFolder, setProjectArchived, unbindFolder, updateProject,
} from "../core/mutations";
import { enabledIn, setPluginEnabled, usePluginInstalls } from "../core/pluginInstalls";
import { inTauri } from "../core/snapshot";
import { confirmDialog } from "../core/dialog";
import { errText, t, tf, viewLabel } from "../core/i18n";
import type { BoundFolderDto, ProjectSettingsDto } from "../bindings/bindings";

type View = ProjectSettingsDto["view"];
const VIEWS: View[] = ["list", "board", "calendar", "timeline"];

// onBack: back to the board (after saving, or on giving up). onGone: called after an operation that removes this
// project from the list (archive/delete), so AppShell can escape to the next destination (the first project, or onboarding).
export function ProjectSettingsScreen({
  projectId, onBack, onGone,
}: { projectId: number; onBack: () => void; onGone: () => void }) {
  const [loaded, setLoaded] = useState<ProjectSettingsDto | null>(null);
  const [name, setName] = useState("");
  const [notes, setNotes] = useState("");
  const [color, setColor] = useState("#9aa7b2");
  const [view, setView] = useState<View>("board");
  const [busy, setBusy] = useState(false);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    fetchProjectSettings(projectId).then((p) => {
      if (!alive || !p) return;
      setLoaded(p);
      setName(p.name);
      setNotes(p.notes);
      setColor(p.color);
      setView(p.view);
    });
    return () => { alive = false; };
  }, [projectId]);

  if (!loaded) {
    return (
      <div className="board__toolbar"><span className="board__title">⚙ {t("projset.title")}</span></div>
    );
  }

  // Send only the fields that changed (the rest stay put). Core rejects an empty name, but the UI disables save as well.
  const dirty = name.trim() !== loaded.name || notes !== loaded.notes || color !== loaded.color || view !== loaded.view;
  const canSave = dirty && name.trim().length > 0 && !busy;

  const save = async () => {
    if (!canSave) return;
    setBusy(true); setError(null);
    try {
      await updateProject(projectId, {
        name: name.trim() !== loaded.name ? name.trim() : undefined,
        notes: notes !== loaded.notes ? notes : undefined,
        color: color !== loaded.color ? color : undefined,
        view: view !== loaded.view ? view : undefined,
      });
      setLoaded({ ...loaded, name: name.trim(), notes, color, view });
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } catch (e) {
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  };

  const toggleArchive = async () => {
    const next = !loaded.archived;
    const ok = await confirmDialog(
      next ? tf("projset.confirmArchive", { name: loaded.name }) : tf("projset.confirmUnarchive", { name: loaded.name }),
    );
    if (!ok) return;
    setBusy(true); setError(null);
    try {
      await setProjectArchived(projectId, next);
      onGone();
    } catch (e) {
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  };

  const remove = async () => {
    const ok = await confirmDialog(tf("projset.confirmDelete", { name: loaded.name }));
    if (!ok) return;
    setBusy(true); setError(null);
    try {
      await deleteProject(projectId);
      onGone();
    } catch (e) {
      setError(errText(e));
      setBusy(false);
    }
  };

  return (
    <>
      <div className="board__toolbar">
        <span className="board__title">
          ⚙ {t("projset.title")}
          {loaded.archived && <span className="faint"> · {t("projset.archivedBadge")}</span>}
        </span>
        <div className="topbar__spacer" />
        <button className="btn" onClick={onBack} disabled={busy}>{t("projset.back")}</button>
      </div>

      <div className="settings">
        <div className="settings__section">
          <div className="settings__h">{t("projset.general")}</div>
          <div className="settings__body newproj">
            <label className="newproj__field">
              <span className="newproj__label">{t("projset.nameLabel")}</span>
              <input className="newproj__input" value={name} onChange={(e) => setName(e.target.value)} />
            </label>
            <label className="newproj__field">
              <span className="newproj__label">{t("projset.notesLabel")}</span>
              <textarea
                className="newproj__input"
                rows={4}
                value={notes}
                placeholder={t("projset.notesPh")}
                onChange={(e) => setNotes(e.target.value)}
              />
            </label>
            <div className="settings__row">
              <span className="settings__k">{t("projset.colorLabel")}</span>
              <input type="color" value={color} onChange={(e) => setColor(e.target.value)} />
            </div>
            <div className="settings__row">
              <span className="settings__k">{t("projset.viewLabel")}</span>
              <select className="btn" value={view} onChange={(e) => setView(e.target.value as View)}>
                {VIEWS.map((v) => <option key={v} value={v}>{viewLabel(v)}</option>)}
              </select>
            </div>

            {error && <div className="newproj__error" role="alert">⚠ {error}</div>}

            <div className="newproj__actions">
              <button className="btn btn--primary" onClick={() => void save()} disabled={!canSave}>
                {busy ? t("projset.saving") : saved ? t("projset.saved") : t("projset.save")}
              </button>
            </div>
          </div>
        </div>

        {inTauri() && <FoldersSection projectId={projectId} />}

        {inTauri() && <PluginsSection projectId={projectId} />}

        <div className="settings__section">
          <div className="settings__h">{t("projset.danger")}</div>
          <div className="settings__body newproj">
            <div className="newproj__field">
              <span className="newproj__hint">{t("projset.archiveHint")}</span>
              <div className="newproj__nextrow">
                <button className="btn" onClick={() => void toggleArchive()} disabled={busy}>
                  {loaded.archived ? t("projset.unarchive") : t("projset.archive")}
                </button>
              </div>
            </div>
            <div className="newproj__field">
              <span className="newproj__hint">{t("projset.deleteHint")}</span>
              <div className="newproj__nextrow">
                <button className="btn btn--danger" onClick={() => void remove()} disabled={busy}>
                  🗑 {t("projset.delete")}
                </button>
              </div>
            </div>
          </div>
        </div>
      </div>
    </>
  );
}

/**
 * The bound-folders section (desktop only). Lists, by reverse lookup, the folders whose `.amenbo` points at this
 * project, showing the path plus an "AI-ready" badge (the folder is there) or a stale mark (moved or deleted). Each
 * row opens in the terminal or the file manager, and the section takes folder-add (binding an arbitrary path) and
 * unbind. These writes never land in the snapshot, so we refetch the folder list here
 * each time. A row whose `.amenbo` slug disagrees with the store on disk (a pointer that came from somewhere else),
 * and a pointer still in the pre-migration legacy format, are both warned about with the reason, alongside a relink
 * that rewrites the pointer at this project in the current format (the equivalent of `bind --project`).
 */
function FoldersSection({ projectId }: { projectId: number }) {
  const [folders, setFolders] = useState<BoundFolderDto[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const reload = async () => {
    try {
      setFolders(await fetchBoundFolders(projectId));
    } catch (e) {
      setError(errText(e));
    }
  };
  useEffect(() => {
    let alive = true;
    fetchBoundFolders(projectId).then((f) => { if (alive) setFolders(f); }).catch((e) => { if (alive) setError(errText(e)); });
    return () => { alive = false; };
  }, [projectId]);

  const add = async () => {
    setError(null);
    try {
      const dir = await pickFolder();
      if (!dir) return;
      setBusy(true);
      await bindFolder(projectId, dir);
      await reload();
    } catch (e) {
      // Folders already nested under an amenbo-managed tree, and the like, are refused by Rust with a coded error (binding_nested_tree).
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  };

  const unbind = async (path: string) => {
    const ok = await confirmDialog(tf("projset.confirmUnbind", { path }));
    if (!ok) return;
    setBusy(true); setError(null);
    try {
      await unbindFolder(path);
      await reload();
    } catch (e) {
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  };

  const reveal = async (path: string) => { try { await revealFolder(path); } catch (e) { setError(errText(e)); } };
  const terminal = async (path: string) => { try { await openTerminal(path); } catch (e) { setError(errText(e)); } };

  const relink = async (path: string) => {
    setBusy(true); setError(null);
    try {
      await bindFolder(projectId, path);
      await reload();
    } catch (e) {
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="settings__section">
      <div className="settings__h">{t("projset.folders")}</div>
      <div className="settings__body newproj">
        <span className="newproj__hint">{t("projset.foldersHint")}</span>

        {folders && folders.length === 0 && (
          <span className="faint">{t("projset.noFolders")}</span>
        )}

        {folders?.map((f) => (
          <div key={f.path} className="newproj__field">
            <div className="newproj__folder">
              <code className="newproj__path">{f.path}</code>
              <span className={f.exists && !f.pointerMissing ? "faint" : "newproj__error"}>
                {!f.exists
                  ? `⚠ ${t("projset.folderStale")}`
                  : f.pointerMissing
                    ? `⚠ ${t("projset.folderNoPointer")}`
                    : `· ${t("projset.aiReady")}`}
              </span>
            </div>
            {f.pointerMissing && (
              <div className="newproj__error" role="alert">⚠ {t("projset.folderNoPointerHint")}</div>
            )}
            {f.mismatch && (
              <div className="newproj__error" role="alert">
                ⚠ {tf("projset.folderElsewhere", {
                  recorded: f.mismatch.recorded,
                  projectId: f.mismatch.projectId,
                  actual: f.mismatch.actual ?? t("projset.folderNoSlug"),
                })}
              </div>
            )}
            {f.legacy && (
              <div className="newproj__error" role="alert">⚠ {t("projset.folderLegacyPointer")}</div>
            )}
            <div className="newproj__nextrow">
              {f.exists && <button className="btn" onClick={() => void terminal(f.path)} disabled={busy}>⌨️ {t("newproj.openTerminal")}</button>}
              {f.exists && <button className="btn" onClick={() => void reveal(f.path)} disabled={busy}>📂 {t("newproj.openFinder")}</button>}
              {f.exists && (f.mismatch || f.legacy || f.pointerMissing) && <button className="btn" onClick={() => void relink(f.path)} disabled={busy}>🔗 {t("projset.relink")}</button>}
              <button className="btn btn--danger" onClick={() => void unbind(f.path)} disabled={busy}>{t("projset.unbind")}</button>
            </div>
          </div>
        ))}

        {error && <div className="newproj__error" role="alert">⚠ {error}</div>}

        <div className="newproj__nextrow">
          <button className="btn" onClick={() => void add()} disabled={busy}>📂 {t("projset.addFolder")}</button>
        </div>
      </div>
    </div>
  );
}

/**
 * The plugins turned on for this project (`AMB-D-412`) — the other face of the plugin screen's row, and
 * the same switch (`AMB-D-434`), not a second one.
 *
 * A project-wide switch is looked for in the project. Someone who turned a plugin on and later wants it
 * off opens the project it was bothering them in, not a catalogue of plugins — so the answer is here as
 * well, saying the same thing from the other end: there, one plugin and the projects it fires in; here,
 * one project and the plugins that fire for it.
 *
 * What is installed is read once for the whole store — a row already carries the projects it is on in,
 * so this section is a filter over that, with no reading of its own.
 */
function PluginsSection({ projectId }: { projectId: number }) {
  const { installs } = usePluginInstalls();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const move = async (name: string, next: boolean) => {
    setBusy(true); setError(null);
    try {
      await setPluginEnabled(name, projectId, next);
    } catch (e) {
      // A refusal is core's (an incompatible build, a `required` setting this project has no value
      // for), and it is the sentence worth showing rather than a switch that appears not to work.
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  };

  const on = installs.filter((i) => enabledIn(i, projectId));
  const off = installs.filter((i) => !enabledIn(i, projectId));

  return (
    <div className="settings__section">
      <div className="settings__h">{t("projset.plugins")}</div>
      <div className="settings__body newproj">
        <span className="newproj__hint">{t("projset.pluginsHint")}</span>

        {installs.length === 0 && <span className="faint">{t("plugins.emptyInstalled")}</span>}
        {installs.length > 0 && on.length === 0 && (
          <span className="faint">{t("projset.pluginsNoneOn")}</span>
        )}

        {on.map((i) => (
          <div key={i.name} className="settings__row">
            <span className="settings__k">{i.name}</span>
            {!i.compatible && <span className="chip chip--warn">{t("plugins.notFiring")}</span>}
            <button className="btn" onClick={() => void move(i.name, false)} disabled={busy}>
              {t("plugins.disable")}
            </button>
          </div>
        ))}

        {error && <div className="newproj__error" role="alert">⚠ {error}</div>}

        {/* Picking one is the enable, the same as on the plugin screen. Only what is off here is
            offered — a plugin already firing for this project has nothing to add. */}
        {off.length > 0 && (
          <div className="newproj__nextrow">
            <select
              className="btn"
              value=""
              disabled={busy}
              onChange={(e) => void move(e.target.value, true)}
            >
              <option value="">{t("projset.pluginsAdd")}</option>
              {off.map((i) => (
                <option key={i.name} value={i.name}>{i.name}</option>
              ))}
            </select>
          </div>
        )}
      </div>
    </div>
  );
}
