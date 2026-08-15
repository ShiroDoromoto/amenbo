// The project settings screen. Not a modal but a screen (a nav destination), taking rename / notes / colour /
// default-view edits plus archive, unarchive and delete across several sections. The snapshot's ProjectDto does not
// carry notes or archived, so we hydrate with `fetchProjectSettings` on open and prefill from that. Saving sends the
// diff (only the fields that changed) to `updateProject`. Destructive operations go through a plugin-dialog confirmation.
import { useEffect, useState } from "react";
import {
  bindFolder, clearAgentHookConsent, deleteProject, fetchAgentHookConsent, fetchAgentHookRequests,
  fetchBoundFolders, fetchProjectSettings, openTerminal, pickFolder, revealFolder, setProjectArchived,
  unbindFolder, updateProject,
} from "../core/mutations";
import { useAgentHookWiring } from "./AgentHookWiringRow";
import { McpSetup } from "./McpSetup";
import { PluginCrossingRow } from "../components/PluginCrossingRow";
import { usePluginInstalls } from "../core/pluginInstalls";
import { inTauri } from "../core/snapshot";
import { confirmDialog } from "../core/dialog";
import { errText, t, tf, viewLabel } from "../core/i18n";
import type { AgentHookRequestsDto, BoundFolderDto, ProjectSettingsDto } from "../bindings/bindings";
import { asTyped } from "../core/keys";
import { ErrorNote } from "../components/ErrorNote";
import { Icon } from "../components/Icon";

type View = ProjectSettingsDto["view"];
const VIEWS: View[] = ["list", "board", "calendar", "timeline"];

// onBack: back to the board (after saving, or on giving up). onGone: called after an operation that removes this
// project from the list (archive/delete), so AppShell can escape to the next destination (the first project, or onboarding).
export function ProjectSettingsScreen({
  projectId, onBack, onGone, onOpenMcp,
}: { projectId: number; onBack: () => void; onGone: () => void; onOpenMcp: () => void }) {
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
        <button className="btn" onClick={onBack} disabled={busy}>← {t("projset.back")}</button>
      </div>

      <div className="settings">
        <div className="settings__section">
          <div className="settings__h">{t("projset.general")}</div>
          <div className="settings__form">
            <label className="field">
              <span className="fieldlabel">{t("projset.nameLabel")}</span>
              <input {...asTyped} className="textinput" value={name} onChange={(e) => setName(e.target.value)} />
            </label>
            <label className="field">
              <span className="fieldlabel">{t("projset.notesLabel")}</span>
              <textarea
                {...asTyped}
                className="textinput"
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

            {error && <ErrorNote>{error}</ErrorNote>}

            <div className="actions">
              <button className="btn btn--primary" onClick={() => void save()} disabled={!canSave}>
                {busy ? t("projset.saving") : saved ? <><Icon name="check" /> {t("projset.saved")}</> : t("projset.save")}
              </button>
            </div>
          </div>
        </div>

        {inTauri() && <FoldersSection projectId={projectId} />}

        {inTauri() && <HarnessSection projectId={projectId} onOpenMcp={onOpenMcp} />}

        {inTauri() && <PluginsSection projectId={projectId} />}

        <div className="settings__section">
          <div className="settings__h">{t("projset.danger")}</div>
          <div className="settings__form">
            <div className="field">
              <span className="hint">{t("projset.archiveHint")}</span>
              <div className="buttonrow">
                <button className="btn" onClick={() => void toggleArchive()} disabled={busy}>
                  {loaded.archived ? t("projset.unarchive") : t("projset.archive")}
                </button>
              </div>
            </div>
            <div className="field">
              <span className="hint">{t("projset.deleteHint")}</span>
              <div className="buttonrow">
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
 *
 * A pointer another store wrote (`foreign`, `AMB-D-685`) takes the "AI-ready" badge away rather than merely adding a
 * warning beneath it: the CLI refuses to run in that folder at all, so a row that went on calling itself ready would
 * be saying the opposite of what the folder does. The relink is offered there too — it is this build claiming the
 * folder for itself, which is one of the two ways out the CLI names.
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
      <div className="settings__form">
        <span className="hint">{t("projset.foldersHint")}</span>

        {folders && folders.length === 0 && (
          <span className="faint">{t("projset.noFolders")}</span>
        )}

        {folders?.map((f) => (
          <div key={f.path} className="field">
            <div className="folderrow">
              <code className="path">{f.path}</code>
              {/* The state of the folder, in the row's own voice — it wears the note's look and its
                  mark, but not its `role`: what is wrong is announced once, by the hint below, and a
                  reader hearing the same fault twice learns nothing the second time. */}
              {f.exists && !f.pointerMissing && !f.foreign ? (
                <span className="faint">· {t("projset.aiReady")}</span>
              ) : (
                <span className="errortext">
                  <Icon name="warning" />
                  {!f.exists
                    ? t("projset.folderStale")
                    : f.pointerMissing
                      ? t("projset.folderNoPointer")
                      : t("projset.folderOtherStore")}
                </span>
              )}
            </div>
            {f.pointerMissing && (
              <ErrorNote>{t("projset.folderNoPointerHint")}</ErrorNote>
            )}
            {f.foreign && (
              <ErrorNote>
                {tf("projset.folderOtherStoreHint", {
                  recorded: f.foreign.recorded,
                  running: f.foreign.running,
                })}
              </ErrorNote>
            )}
            {f.mismatch && (
              <ErrorNote>
                {tf("projset.folderElsewhere", {
                  recorded: f.mismatch.recorded,
                  projectId: f.mismatch.projectId,
                  actual: f.mismatch.actual ?? t("projset.folderNoSlug"),
                })}
              </ErrorNote>
            )}
            {f.legacy && (
              <ErrorNote>{t("projset.folderLegacyPointer")}</ErrorNote>
            )}
            <div className="buttonrow">
              {f.exists && <button className="btn" onClick={() => void terminal(f.path)} disabled={busy}>⌨️ {t("newproj.openTerminal")}</button>}
              {f.exists && <button className="btn" onClick={() => void reveal(f.path)} disabled={busy}>📂 {t("newproj.openFinder")}</button>}
              {f.exists && (f.mismatch || f.legacy || f.pointerMissing || f.foreign) && <button className="btn" onClick={() => void relink(f.path)} disabled={busy}>🔗 {t("projset.relink")}</button>}
              <button className="btn btn--danger" onClick={() => void unbind(f.path)} disabled={busy}>{t("projset.unbind")}</button>
            </div>
          </div>
        ))}

        {error && <ErrorNote>{error}</ErrorNote>}

        <div className="buttonrow">
          <button className="btn" onClick={() => void add()} disabled={busy}>📂 {t("projset.addFolder")}</button>
        </div>
      </div>
    </div>
  );
}

/**
 * What this project answered about starting its AI on amenbo, and the way back out of that answer
 * (`AMB-D-459`, `AMB-D-460`).
 *
 * A no is silence from then on: the standing row it ends is the surface the answer is given on, so once
 * it is given there is nothing left on screen to take it back with. This is that way back — clearing
 * drops the record, the project returns to never having been answered, and opening it again brings the
 * row back.
 *
 * **Three states, not two.** Unanswered is not a no: it is what a project starts in, and a screen that
 * showed only yes/no would report a refusal from a project that has simply never said anything. Clearing
 * is offered only where there is an answer to clear.
 *
 * **The record and the wiring are two different things, and both are here.** A yes buys the text, never
 * the wiring — so what is answered is read from the store and what is actually in the folders is read
 * from disk. The board draws one standing notice and no more (`AMB-D-535`), which means the folders still
 * waiting are not always on it; this is the place they are always listed. The board is where the reader
 * acts, this is where they look over what there is.
 *
 * **What is looked over here goes quiet; the request face beside it does not** ({@link HarnessRequest},
 * `AMB-D-670`). The answer row and the waiting list both come from the notice, so a project with every
 * folder wired says nothing on either — which is the state a reader changing tools arrives in. Taking the
 * text is put on the same section rather than another screen, because it is the same subject; it just
 * hangs on nothing.
 */
function HarnessSection({ projectId, onOpenMcp }: { projectId: number; onOpenMcp: () => void }) {
  // undefined while the record is being read, null for a project that has never been asked.
  const [answer, setAnswer] = useState<boolean | null | undefined>(undefined);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const { waiting } = useAgentHookWiring(projectId);
  // In core's order, and each folder once however many harnesses are waiting on it.
  const waitingDirs = [...new Set(waiting.flatMap((one) => one.dirs))];

  useEffect(() => {
    let alive = true;
    fetchAgentHookConsent(projectId)
      .then((a) => { if (alive) setAnswer(a); })
      .catch((e) => { if (alive) setError(errText(e)); });
    return () => { alive = false; };
  }, [projectId]);

  const clear = async () => {
    setBusy(true); setError(null);
    try {
      await clearAgentHookConsent(projectId);
      setAnswer(null);
    } catch (e) {
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="settings__section">
      <div className="settings__h">{t("projset.harness")}</div>
      <div className="settings__form">
        <span className="hint">{t("projset.harnessHint")}</span>

        {answer !== undefined && (
          <div className="settings__row">
            <span className="settings__k">{t("projset.harnessAnswer")}</span>
            <span className={answer === null ? "faint" : undefined}>
              {answer === null
                ? t("projset.harnessUnanswered")
                : answer ? t("projset.harnessYes") : t("projset.harnessNo")}
            </span>
          </div>
        )}

        {/* The inventory: every folder of this project that still starts its AI without amenbo, whichever
            notice the board happens to be carrying. Silent where there is nothing waiting — an empty list
            is what "all wired" looks like, and it needs no sentence of its own.
            The folders are listed once each, not once per tool. Core answers by harness, and a folder that
            names no tool it uses is waiting on every one in the catalog — so the same path grouped by tool
            comes out five times, reading as five folders left. Which tool to hand the text to is the
            board row's question, asked where the text is; what is outstanding is a folder. */}
        {waitingDirs.length > 0 && (
          <div className="settings__row">
            <span className="settings__k">{t("projset.harnessWaiting")}</span>
            <ul className="agenthookrow__dirs">
              {waitingDirs.map((dir) => <li key={dir}>{dir}</li>)}
            </ul>
          </div>
        )}

        <HarnessRequest projectId={projectId} />

        {/* The other way in, for the AI that cannot open a folder at all (`AMB-D-671`). Same section
            because it is the same subject — how this project is reached from an AI — and folded, since
            the road above it is the one most readers are on. */}
        <McpSetup onOpen={onOpenMcp} />

        {error && <ErrorNote>{error}</ErrorNote>}

        <div className="buttonrow">
          {/* Nothing to clear where nothing was answered, and the state row above says so. */}
          <button className="btn" onClick={() => void clear()} disabled={busy || answer === undefined || answer === null}>
            {t("projset.harnessClear")}
          </button>
        </div>
      </div>
    </div>
  );
}

/**
 * The way to the request text that is always open (`AMB-D-670`).
 *
 * **It hangs on no notice.** Everything else about the session-start hook is drawn from
 * `harness::setup_notice` — the board's standing row, and the waiting-folders list above this — and that
 * goes quiet once the wiring lands. Which leaves the reader who wired one tool and then moved to another
 * with no way to the text in the GUI at all, while the command line kept `agent-hook snippet <tool>`. So
 * this reads its own answer and shows the same thing before and after anything is wired.
 *
 * **The whole catalog is on offer, not what the folders trace.** The tool being moved to has left nothing
 * behind yet, which is the point of coming here. Picking is the reader's, and the first row is only where
 * the list starts.
 *
 * **One text, the folders listed under it.** Same as the board's row: the request for a tool is the same
 * wherever it goes, so it goes up once with this project's folders beside it.
 */
function HarnessRequest({ projectId }: { projectId: number }) {
  const [catalog, setCatalog] = useState<AgentHookRequestsDto>({ tools: [], dirs: [] });
  // Which tool the reader picked; unset is the head of the catalog. No inference from the folders here —
  // what is traced is what they are leaving.
  const [picked, setPicked] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    let alive = true;
    setPicked(null);
    setCopied(false);
    fetchAgentHookRequests(projectId)
      .then((got) => { if (alive) setCatalog(got); })
      .catch(() => {});
    return () => { alive = false; };
  }, [projectId]);

  if (catalog.tools.length === 0) return null;
  const tool = catalog.tools.find((one) => one.tool === picked) ?? catalog.tools[0];

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(tool.request);
      setCopied(true);
      setTimeout(() => setCopied(false), 1200);
    } catch { /* where the clipboard is unavailable, quietly skip */ }
  };

  return (
    <div className="settings__row">
      <span className="settings__k">{t("projset.harnessRequest")}</span>
      <div className="harnessreq">
        <span className="hint">{t("projset.harnessRequestHint")}</span>
        <select
          className="harnessreq__pick"
          aria-label={t("agentHookWiring.pick")}
          value={tool.tool}
          onChange={(e) => { setPicked(e.target.value); setCopied(false); }}
        >
          {catalog.tools.map((one) => (
            <option key={one.tool} value={one.tool}>{one.label}</option>
          ))}
        </select>
        <span className="hint">
          {tf("agentHookWiring.what", { tool: tool.label, file: tool.pasteInto })}
        </span>
        {/* Where to paste it. A project with nothing bound has nowhere, and that is said rather than left
            as a heading over an empty list — the folder section above is where it is answered. */}
        <span className="hint">
          {catalog.dirs.length > 0 ? t("projset.harnessRequestDirs") : t("projset.noFolders")}
        </span>
        {catalog.dirs.length > 0 && (
          <ul className="agenthookrow__dirs">
            {catalog.dirs.map((dir) => <li key={dir}>{dir}</li>)}
          </ul>
        )}
        <pre className="agenthookrow__request">{tool.request}</pre>
        <div className="buttonrow">
          <button className="btn" onClick={() => void copy()}>
            {copied ? t("agentHookWiring.copied") : t("agentHookWiring.copy")}
          </button>
        </div>
      </div>
    </div>
  );
}

/**
 * This project's plugin crossings (`AMB-D-447`) — the same rows the plugin screen draws, listed from the
 * other end: there, one plugin and the projects it crosses; here, one project and the plugins.
 *
 * The crossing is the unit on both faces, so a row here carries everything about it — the switch for this
 * project, this project's settings, and the mark saying a `required` value is missing — and a person
 * refused an enable fills the value in without leaving the row that refused them. That is what this face
 * lacked: it reported the refusal and offered nowhere to answer it.
 *
 * **What is listed is what there is something to say about**: the plugins on in this project
 * (`AMB-D-412`) and the ones this project filled in without turning on (`AMB-D-434`). Another is
 * **added** from the picker rather than enabled by it, for the reason the plugin face adds a project —
 * the crossing has to exist before what would refuse it can be read or filled in.
 *
 * **A plugin its author declared the machine's is not one of those rows** (`AMB-D-601`). It crosses no
 * project, so a switch drawn here would move a gate this project does not own and read back as untouched
 * — which is what it did before the layer was drawn at all. It is named below the rows instead, with
 * whether it fires, because a device-wide plugin does fire *here* and a project's own settings that never
 * mentioned it would be hiding that; moving it stays on the plugin's own face, where its one row is.
 *
 * What is installed is read once for the whole store — an install already carries the rows it has — so
 * this section is a filter over that, with no reading of its own.
 */
function PluginsSection({ projectId }: { projectId: number }) {
  const { installs } = usePluginInstalls();
  // Plugins opened from the picker, which this project says nothing about yet. Kept until the screen is
  // left, so turning one off does not make the row someone is working in vanish.
  const [added, setAdded] = useState<string[]>([]);

  // The device's own, which no project crosses — kept out of the rows and out of the picker, and said
  // apart below.
  const deviceWide = installs.filter((i) => i.device != null);
  const crossable = installs.filter((i) => i.device == null);
  // In the store's own order, so the same two plugins do not swap places between two visits.
  const shown = crossable.filter(
    (i) => added.includes(i.name) || i.projects.some((row) => row.project === projectId),
  );
  const rest = crossable.filter((i) => !shown.includes(i));

  return (
    <div className="settings__section">
      <div className="settings__h">{t("projset.plugins")}</div>
      <div className="settings__form">
        <span className="hint">{t("projset.pluginsHint")}</span>

        {installs.length === 0 && <span className="faint">{t("plugins.emptyInstalled")}</span>}
        {/* Judged on what this project *could* cross: with only the device's own installed there is no
            crossing to have made, so saying none were made would be reporting an absence nobody could
            have filled. */}
        {crossable.length > 0 && shown.length === 0 && (
          <span className="faint">{t("projset.pluginsNone")}</span>
        )}

        {shown.map((i) => (
          <div key={i.name}>
            <PluginCrossingRow install={i} layer={projectId} name={i.name} />
            {/* Said per row, unlike the plugin face where every row is the same plugin: here each row is
                a different one, and only some of them are builds this amenbo cannot speak to. */}
            {!i.compatible && (
              <div className="pluggate__note">{i.incompatibleReason ?? t("plugins.incompatible")}</div>
            )}
          </div>
        ))}

        {/* The device's own, named rather than switched (`AMB-D-601`). Whether it fires is worth saying
            here because a device-wide plugin that is on fires in this project too — but the switch is one
            the whole machine shares, so it stays where its single row is. */}
        {deviceWide.length > 0 && (
          <>
            <span className="hint">{t("projset.pluginsDevice")}</span>
            {deviceWide.map((i) => (
              <div className="pluggate" key={i.name}>
                <span className="chip">{i.name}</span>
                {i.device?.enabled && <span className="chip">{t("plugins.enabledChip")}</span>}
                <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>
                  {t("plugins.scope.machine")}
                </span>
              </div>
            ))}
          </>
        )}

        {rest.length > 0 && (
          <div className="buttonrow">
            <select
              className="btn"
              value=""
              onChange={(e) => setAdded((a) => [...a, e.target.value])}
            >
              <option value="">{t("projset.pluginsAdd")}</option>
              {rest.map((i) => (
                <option key={i.name} value={i.name}>{i.name}</option>
              ))}
            </select>
          </div>
        )}
      </div>
    </div>
  );
}
