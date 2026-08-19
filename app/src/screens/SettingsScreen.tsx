import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import { currentLang, doctorText, errText, langEndonym, LANGS, t, tn, tf, viewLabel, type Lang, type ViewKind } from "../core/i18n";
import { getSnapshot, subscribe } from "../core/snapshot";
import {
  answerTick,
  bindFolder, cancelDataOp, fetchDoctorReport, fetchStoreLocations, fileToAvatarDataUrl, listenDataProgress,
  openLogsDir, pickBackupPath, pickExportPath, pickRestoreArchive, resyncManagedBlocks, runBackup, runDoctorFix,
  runExport, runRestore, setAutostart, setDefaultView, setFacetNames, setLanguage, setFacetAvatar, setPerfLog,
  setUpdateCheck,
} from "../core/mutations";
import { ErrorNote } from "../components/ErrorNote";
import { DoneNote } from "../components/DoneNote";
import { useIsDevBuild } from "../core/devChannel";
import { doctorRepair, groupDoctorIssues, type DoctorRepair } from "../core/doctorKinds";
import { confirmDialog } from "../core/dialog";
import type { DoctorReportDto, StoreLocationsDto, DataProgressDto } from "../bindings/bindings";
import { perfMode } from "../core/ipc";
import { Identicon } from "../components/identicon";
import { DataProgressModal } from "../components/DataProgressModal";
import { facetColor, FacetAvatar, identiconSeed } from "../components/atoms";
import { getThemePref, setThemePref, type ThemePref } from "../core/theme";
import { asTyped, isEnterSubmit } from "../core/keys";
import { Icon } from "../components/Icon";

/** The four a project can open in, in the order the header draws its tabs. */
const VIEWS: ViewKind[] = ["list", "board", "calendar", "timeline"];

// Settings: profile, appearance, AI policy, developer, and data (export and backup). The store is a
// single local one, so there is no section for sharing, syncing, keys or members.
export function SettingsScreen() {
  const [theme, setTheme] = useState<ThemePref>(getThemePref);
  const devBuild = useIsDevBuild();
  const changeTheme = (p: ThemePref) => { setThemePref(p); setTheme(p); };

  return (
    <div className="settings">
      <Category title={t("settings.profile")}>
        <NameSetting />
        <AvatarSetting />
      </Category>

      <Category title={t("settings.appearance")}>
        <div className="settings__row">
          <span className="settings__k">{t("settings.theme")}</span>
          <select className="btn" value={theme} onChange={(e) => changeTheme(e.target.value as ThemePref)}>
            <option value="os">{t("settings.themeOs")}</option>
            <option value="dark">{t("settings.themeDark")}</option>
            <option value="light">{t("settings.themeLight")}</option>
          </select>
        </div>
        <LanguageSetting />
        <DefaultViewSetting />
      </Category>

      {/* Nor does a development build carry the startup section: it registers nothing at login, so the
          switch would be an offer it cannot keep. */}
      {!devBuild && (
        <Category title={t("settings.startup")}>
          <AutostartSetting />
        </Category>
      )}

      {/* The hourly tick has a section of its own, and a development build draws it: unlike the two
          above, nothing is registered here that was not asked for, so the build that was asked is the
          one that has to be able to take it back. */}
      <Category title={t("settings.dueWarning")}>
        <TickSetting />
      </Category>

      {/* A development build carries no update section. Its one control is a switch core does not read
          — the check is withheld from the channel outright — so what would be left is a heading over a
          setting that claims a check nothing runs. */}
      {!devBuild && (
        <Category title={t("settings.updates")}>
          <UpdateCheckSetting />
        </Category>
      )}

      <Category title={t("settings.developer")}>
        <PerfLogSetting />
      </Category>

      <Category title={t("settings.data")}>
        <DataLocationSetting />
        <LogsSetting />
        <ExportImportSetting />
        <BackupSetting />
      </Category>

      <Category title={t("settings.integrity")}>
        <DoctorSetting />
      </Category>
    </div>
  );
}

/** Change the user's language (config.language) after the fact. The choice is saved to config.json at once and takes effect without a restart. */
function LanguageSetting() {
  // Subscribe to snapshot.language so the choice lands immediately (an i18n re-render).
  const lang = useSyncExternalStore(subscribe, currentLang);
  const change = (e: React.ChangeEvent<HTMLSelectElement>) => { void setLanguage(e.target.value as Lang); };
  return (
    <div className="settings__row">
      <span className="settings__k">{t("settings.language")}</span>
      {/* The same nineteen the setup step offers, under the same names — each language written the
          way it writes itself. */}
      <select className="btn" value={lang} onChange={change}>
        {LANGS.map((l) => (
          <option key={l} value={l} lang={l}>{langEndonym(l)}</option>
        ))}
      </select>
    </div>
  );
}

/** Change the view a project created without one of its own opens in (config.default_view). It reads
 *  defaultView from the snapshot, so the pull-down follows a value written anywhere else — `config set`
 *  in the terminal included. The note under it is load-bearing: this setting looks inert, because every
 *  project that already exists carries its own view and nothing on screen moves when it changes. */
function DefaultViewSetting() {
  const view = useSyncExternalStore(subscribe, () => getSnapshot().defaultView);
  const change = (e: React.ChangeEvent<HTMLSelectElement>) => { void setDefaultView(e.target.value as ViewKind); };
  return (
    <div className="settings__row">
      <span className="settings__k">{t("settings.defaultView")}</span>
      <span>
        <select className="btn" value={view} onChange={change}>
          {VIEWS.map((v) => <option key={v} value={v}>{viewLabel(v)}</option>)}
        </select>
        <div className="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("settings.defaultViewNote")}</div>
      </span>
    </div>
  );
}

/** Edit the roster's two display names (human and AI). The inline inputs start from the current names
 *  (config.human_name/ai_name, as seen in snapshot.roster), and saving rewrites them everywhere.
 *  Surrounding whitespace is trimmed and an empty value leaves the name as it was (going back to the
 *  default is another path). If the current name changes elsewhere — through onboarding, say — an
 *  untouched draft follows it. */
function NameSetting() {
  const snap = useSyncExternalStore(subscribe, getSnapshot);
  const curHuman = snap.roster.find((a) => a.kind === "human")?.name ?? "";
  const curAi = snap.roster.find((a) => a.kind === "ai")?.name ?? "";
  const [human, setHuman] = useState(curHuman);
  const [ai, setAi] = useState(curAi);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // When the current name changes elsewhere, an untouched draft follows it (this is also what settles the field right after a save).
  useEffect(() => { setHuman(curHuman); }, [curHuman]);
  useEffect(() => { setAi(curAi); }, [curAi]);

  const humanT = human.trim();
  const aiT = ai.trim();
  // A facet is only updated when its draft is non-empty and differs from the current name. Either one changing enables save.
  const humanChanged = humanT !== "" && humanT !== curHuman;
  const aiChanged = aiT !== "" && aiT !== curAi;
  const dirty = humanChanged || aiChanged;

  async function save() {
    if (!dirty) return;
    setBusy(true);
    setError(null);
    try {
      await setFacetNames(humanChanged ? humanT : null, aiChanged ? aiT : null);
    } catch (err) {
      setError(errText(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="settings__row">
      <span className="settings__k">{t("settings.facetNames")}</span>
      <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <Icon name="person" />
          <input
            {...asTyped}
            className="btn"
            value={human}
            disabled={busy}
            style={{ minWidth: 200 }}
            aria-label={t("settings.humanNameLabel")}
            onChange={(e) => setHuman(e.target.value)}
            onKeyDown={(e) => { if (isEnterSubmit(e)) void save(); }}
          />
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <Icon name="robot" />
          <input
            {...asTyped}
            className="btn"
            value={ai}
            disabled={busy}
            style={{ minWidth: 200 }}
            aria-label={t("settings.aiNameLabel")}
            onChange={(e) => setAi(e.target.value)}
            onKeyDown={(e) => { if (isEnterSubmit(e)) void save(); }}
          />
          <button className="btn" disabled={busy || !dirty} onClick={() => void save()}>
            {t("settings.facetNamesSave")}
          </button>
        </div>
        <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("settings.facetNamesHint")}</span>
        {error && <ErrorNote tone="quiet">{error}</ErrorNote>}
      </div>
    </div>
  );
}

/** One slot for setting or clearing a facet's avatar image (human or AI). With none set it shows the
 *  facet's identicon; with one set it shows the image, large. It is stored in `config.human_avatar` /
 *  `ai_avatar` (which is what snapshot.roster carries), and the subscription updates the preview as
 *  soon as it is saved. */
function AvatarSlot({ kind }: { kind: "human" | "ai" }) {
  const snap = useSyncExternalStore(subscribe, getSnapshot);
  const actor = snap.roster.find((a) => a.kind === kind);
  const fileRef = useRef<HTMLInputElement>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function pick(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0];
    e.target.value = ""; // So that picking the same file twice in a row still fires change
    if (!file) return;
    setBusy(true);
    setError(null);
    try {
      const dataUrl = await fileToAvatarDataUrl(file);
      await setFacetAvatar(kind, dataUrl);
    } catch (err) {
      setError(errText(err));
    } finally {
      setBusy(false);
    }
  }

  async function reset() {
    setBusy(true);
    setError(null);
    try {
      await setFacetAvatar(kind, null);
    } catch (err) {
      setError(errText(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
      <span className="avatar-preview" style={{ borderColor: facetColor(kind) }} title={actor?.name ?? ""}>
        {actor?.avatar
          ? <img className="avatar-preview__img" src={actor.avatar} alt="" />
          : <Identicon seed={actor ? identiconSeed(actor) : kind} size={48} />}
      </span>
      <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
        <span style={{ fontSize: "var(--fs-xs)" }}>
          {actor && <FacetAvatar actor={actor} />} {actor?.name ?? ""}
        </span>
        <div style={{ display: "flex", gap: 8 }}>
          <button className="btn" disabled={busy} onClick={() => fileRef.current?.click()}>
            {t("settings.avatarChoose")}
          </button>
          {actor?.avatar && (
            <button className="btn" disabled={busy} onClick={reset}>
              {t("settings.avatarReset")}
            </button>
          )}
        </div>
        {error && <ErrorNote tone="quiet">{error}</ErrorNote>}
      </div>
      <input ref={fileRef} type="file" accept="image/*" style={{ display: "none" }} onChange={pick} />
    </div>
  );
}

/** Set or clear the roster's two faces (human and AI). The counterpart to `NameSetting` for display
 *  names: each facet's identicon can be replaced with an image of the user's choosing. */
function AvatarSetting() {
  return (
    <div className="settings__row">
      <span className="settings__k">{t("settings.avatar")}</span>
      <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
        <AvatarSlot kind="human" />
        <AvatarSlot kind="ai" />
        <span className="faint">{t("settings.avatarHint")}</span>
      </div>
    </div>
  );
}

/**
 * What a move the reader pressed left behind, and whether it went through. The tick belongs to the
 * second half alone: a transfer the reader cancelled themselves is neither something that worked nor
 * something that failed, so it is said in the same place in the quieter voice.
 */
type Note = { text: string; done: boolean };

function NoteLine({ note }: { note: Note | null }) {
  if (!note) return null;
  return note.done ? <DoneNote>{note.text}</DoneNote> : <span className="faint">{note.text}</span>;
}

/** Export, under Settings > Data. It writes everything out as a directory — `export.json` plus the
 *  attachments themselves (`attachments/`) — for moving to another tool. It is one-way: there is no
 *  way back in (the only road back into amenbo is restoring a backup). The GUI is a thin call on
 *  core's single API (`run_export`), and progress comes over the same `data-progress` event and
 *  `DataProgressModal` (cancellable) that backup and restore use. There is only one shape, a single
 *  JSON covering everything. Backup/restore below is a different thing: that one is disaster recovery
 *  within amenbo. */
function ExportImportSetting() {
  const [busy, setBusy] = useState<null | "export">(null);
  const [progress, setProgress] = useState<DataProgressDto | null>(null);
  const [msg, setMsg] = useState<Note | null>(null);
  const [error, setError] = useState<string | null>(null);
  // While the user is cancelling, core's abort error is shown as a neutral message, not a red error.
  const cancelling = useRef(false);

  async function exportJson() {
    setMsg(null); setError(null);
    const path = await pickExportPath();
    if (!path) return; // Cancelled
    cancelling.current = false;
    setBusy("export"); setProgress(null);
    const unlisten = await listenDataProgress(setProgress);
    try {
      const r = await runExport(path);
      setMsg({
        done: true,
        text: tf("settings.exportDone", {
          kb: Math.max(1, Math.round(r.bytes / 1024)),
          attachments: r.attachments,
        }) + (r.missing > 0 ? tf("settings.exportMissing", { missing: r.missing }) : ""),
      });
    } catch (e) {
      if (cancelling.current) setMsg({ done: false, text: t("settings.transferCancelled") });
      else setError(errText(e));
    } finally {
      unlisten(); setBusy(null); setProgress(null); cancelling.current = false;
    }
  }

  return (
    <div className="settings__row">
      <span className="settings__k">{t("settings.exportImport")}</span>
      <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
        <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
          <button className="btn" disabled={busy !== null} onClick={() => void exportJson()}>{t("settings.exportJson")}</button>
        </div>
        <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("settings.dataNote")}</span>
        <NoteLine note={msg} />
        {error && <ErrorNote tone="quiet">{error}</ErrorNote>}
      </div>
      {busy && <DataProgressModal progress={progress} onCancel={() => { cancelling.current = true; void cancelDataOp(); }} />}
    </div>
  );
}

/** Backup and restore of everything (every project plus the root overview). Choosing where to save
 *  and which file to open goes through plugin-dialog (save/open), and the destructive confirmation
 *  before a restore through confirmDialog, because `window.confirm` does nothing in a Tauri webview.
 *  The work itself is a thin call on core's single API (`run_backup` / `run_restore`); per-store
 *  progress arrives on the `data-progress` event and is shown in the progress modal (cancellable),
 *  and the outcome is inline success/error text. A restore is destructive, but core sets aside each
 *  replaced store's prior state first. The result says exactly what the CLI says — attachments
 *  restored, where the prior state was set aside, which old set-asides were swept, the version chain
 *  — and it names the set-aside location outright, because that is the way back we promised in the
 *  destructive confirmation. While the user is cancelling, core's abort error is shown as a neutral
 *  message rather than a red error, exactly as export does. */
function BackupSetting() {
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<Note | null>(null);
  const [error, setError] = useState<string | null>(null);
  // The progress modal: null hides it. It mirrors the per-store progress while the work runs.
  const [progress, setProgress] = useState<DataProgressDto | null>(null);
  const cancelling = useRef(false);

  async function backup() {
    setMsg(null); setError(null);
    const path = await pickBackupPath();
    if (!path) return; // Cancelled
    cancelling.current = false;
    setBusy(true); setProgress(null);
    const unlisten = await listenDataProgress(setProgress);
    try {
      const r = await runBackup(path);
      setMsg({ done: true, text: tf("settings.backupDone", { kb: Math.max(1, Math.round(r.bytes / 1024)) }) });
    } catch (e) {
      if (cancelling.current) setMsg({ done: false, text: t("settings.transferCancelled") });
      else setError(errText(e));
    } finally {
      unlisten(); setBusy(false); setProgress(null); cancelling.current = false;
    }
  }

  async function restore() {
    setMsg(null); setError(null);
    const path = await pickRestoreArchive();
    if (!path) return; // Cancelled
    if (!(await confirmDialog(t("settings.restoreConfirm")))) return;
    cancelling.current = false;
    setBusy(true); setProgress(null);
    const unlisten = await listenDataProgress(setProgress);
    try {
      const r = await runRestore(path);
      const lines = [tf("settings.restoreDone", { attachments: r.blobs })];
      if (r.previousSavedTo) lines.push(tf("settings.restoreAside", { path: r.previousSavedTo }));
      if (r.superseded > 0) lines.push(tn("settings.restoreSwept", r.superseded));
      const m = r.migration;
      if (m) lines.push(tf("settings.restoreMigrated", { from: m.from, to: m.to, steps: m.applied.join(", ") }));
      setMsg({ done: true, text: lines.join("\n") });
    } catch (e) {
      if (cancelling.current) setMsg({ done: false, text: t("settings.transferCancelled") });
      else setError(errText(e));
    } finally {
      unlisten(); setBusy(false); setProgress(null); cancelling.current = false;
    }
  }

  return (
    <div className="settings__row">
      <span className="settings__k">{t("settings.backup")}</span>
      <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
        <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
          <button className="btn" disabled={busy} onClick={() => void backup()}>{t("settings.backupBtn")}</button>
          <button className="btn" disabled={busy} onClick={() => void restore()}>{t("settings.restoreBtn")}</button>
        </div>
        <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("settings.backupNote")}</span>
        <NoteLine note={msg} />
        {error && <ErrorNote tone="quiet">{error}</ErrorNote>}
      </div>
      {busy && (
        <DataProgressModal
          progress={progress}
          onCancel={() => { cancelling.current = true; void cancelDataOp(); }}
        />
      )}
    </div>
  );
}

/** Doctor, as a GUI surface. It runs the same core path as `amenbo doctor` (`doctor::report`), lists
 *  the issues found inside the store and in this machine's bound folders (`.amenbo`, the AI guide),
 *  and runs the same sweeps as `doctor --fix` behind the fix button — the startup health banner only
 *  looks inside the store, so this is the only place where the binding issues show up too. Repair
 *  comes in two tiers. The fix button runs the sweeps together (reclaiming unreferenced blobs,
 *  clearing rows for folders that are gone); neither is destructive, so it asks for no confirmation.
 *  But that does not fix an issue with a bound folder, which is what the per-row buttons
 *  (`doctorRepair`) are for. A row can only carry a button where the fix is unambiguous (`bind`, or
 *  resyncing the guide); where it is not (`*_ambiguous`) the issue stays as prose, because we will not
 *  silently pick a different project. */
function DoctorSetting() {
  const [report, setReport] = useState<DoctorReportDto | null>(null);
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<Note | null>(null);
  const [error, setError] = useState<string | null>(null);

  const check = async () => {
    setBusy(true); setError(null);
    try {
      setReport(await fetchDoctorReport());
    } catch (e) {
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => { void check(); }, []); // Show the state as of opening Settings (the check is read-only).

  /** Repair a single row. It calls the one core path the difference (dir / project) determines, and
   *  shows whether it worked by checking again — if it did not, the issue is simply still there, so the
   *  success message cannot lie. Neither is destructive, so neither asks for confirmation (`bind` only
   *  lays down `.amenbo` and the guide again; a resync rewrites nothing outside the markers). */
  const repairOne = async (repair: DoctorRepair) => {
    setBusy(true); setMsg(null); setError(null);
    try {
      if (repair.action === "rebind") await bindFolder(repair.project, repair.dir);
      else await resyncManagedBlocks(repair.dir);
      setMsg({ done: true, text: t("settings.doctorRepairDone") });
      setReport(await fetchDoctorReport());
    } catch (e) {
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  };

  // What the panel draws, and whether anything it is showing can be acted on from here — the
  // sweep below fixes neither of the two commonest kinds (a dead ref, a duplicate order key).
  const groups = groupDoctorIssues(report?.issues ?? []);
  const anyRepairable = (report?.issues ?? []).some((iss) => doctorRepair(iss) !== null);

  const fix = async () => {
    if (!report) return;
    setBusy(true); setMsg(null); setError(null);
    try {
      const r = await runDoctorFix();
      const touched = r.reclaimedBlobs + r.forgottenBindings;
      setMsg({
        done: true,
        text: touched === 0
          ? t("settings.doctorFixNothing")
          : tf("settings.doctorFixDone", { blobs: r.reclaimedBlobs, bindings: r.forgottenBindings }),
      });
      setReport(await fetchDoctorReport()); // Check again to see what was fixed; anything the sweeps cannot fix stays.
    } catch (e) {
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="settings__row">
      <span className="settings__k">{t("settings.doctor")}</span>
      <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
        <div style={{ display: "flex", gap: 8, flexWrap: "wrap", alignItems: "center" }}>
          <button className="btn" disabled={busy} onClick={() => void check()}>
            {busy && !report ? t("settings.doctorChecking") : t("settings.doctorRecheck")}
          </button>
          {report && (
            <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>
              {report.issues.length === 0
                ? <DoneNote>{t("settings.doctorClean")}</DoneNote>
                : tf("settings.doctorFound", { errors: report.errors, warnings: report.warnings })}
            </span>
          )}
        </div>
        {/* One block per kind, not one per issue: the fix hint belongs to the kind, and a real store
            can hold hundreds of a single kind — 411 dead refs, measured — which is a panel nobody
            reads. The same fold the terminal already does (`print_grouped`). */}
        {groups.map((group) => {
          const { fixHint } = doctorText(group.shown[0]);
          return (
            <div key={group.kind} style={{ display: "flex", flexDirection: "column", gap: 2, fontSize: "var(--fs-xs)" }}>
              {group.shown.map((iss, i) => {
                const { message } = doctorText(iss);
                const repair = doctorRepair(iss);
                return (
                  <span key={i} style={{ display: "flex", flexDirection: "column", gap: 2 }}>
                    {/* The line the mark sits in is what carries the step: a fault stops the reader, a
                        warning is theirs to know while everything keeps working. */}
                    <span className={iss.severity === "error" ? "step-stop" : "step-heed"}>
                      <Icon name={iss.severity === "error" ? "error" : "warning"} /> {message}
                    </span>
                    {repair && (
                      <span>
                        <button className="btn" disabled={busy} onClick={() => void repairOne(repair)}>
                          {busy ? t("settings.doctorRepairing")
                            : repair.action === "rebind" ? t("settings.doctorRebind") : t("managedBlock.resync")}
                        </button>
                      </span>
                    )}
                  </span>
                );
              })}
              {group.withheld > 0 && (
                <span className="faint">{tf("settings.doctorMore", { count: group.withheld })}</span>
              )}
              <span className="faint">{fixHint}</span>
            </div>
          );
        })}
        <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("settings.doctorNote")}</span>
        {/* The sweep is its own act, set apart from the counts above on purpose: it does not repair
            what the list is showing, and standing next to "0 errors / 412 warnings" is exactly how it
            got read as the button that clears them. Its label names what it sweeps instead. */}
        <div style={{ display: "flex", gap: 8, flexWrap: "wrap", alignItems: "center", marginTop: 4 }}>
          <button className="btn" disabled={busy || !report} onClick={() => void fix()}>
            {busy && report ? t("settings.doctorFixing") : t("settings.doctorFix")}
          </button>
          {report && report.issues.length > 0 && !anyRepairable && (
            <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("settings.doctorNoneRepairable")}</span>
          )}
        </div>
        <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("settings.doctorFixNote")}</span>
        <NoteLine note={msg} />
        {error && <ErrorNote tone="quiet">{error}</ErrorNote>}
      </div>
    </div>
  );
}

/** Switch the level of perf instrumentation (config.perf_log). The choice is saved to config.json at
 *  once and moves both core's tracing filter and the front-end instrumentation gate while the app
 *  runs. What is displayed is the effective level as ipc resolved it (with nothing set, dev builds
 *  default to on), followed through the snapshot subscription. */
function PerfLogSetting() {
  // Subscribing to the snapshot re-renders the effective level once setPerfLog lands (loadSnapshot notifies).
  const mode = useSyncExternalStore(subscribe, perfMode);
  const change = (e: React.ChangeEvent<HTMLSelectElement>) => { void setPerfLog(e.target.value); };
  return (
    <div className="settings__row">
      <span className="settings__k">{t("settings.perfLog")}</span>
      <span>
        <select className="btn" value={mode} onChange={change}>
          <option value="off">{t("settings.perfLogOff")}</option>
          <option value="budget-only">{t("settings.perfLogBudget")}</option>
          <option value="verbose">{t("settings.perfLogVerbose")}</option>
        </select>
        <div className="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("settings.perfLogNote")}</div>
      </span>
    </div>
  );
}

/** Turn the update check (config.update_check) on or off. It subscribes to updateCheck in the snapshot,
 *  so once setUpdateCheck lands (loadSnapshot notifies) both the control and the upstream update banner
 *  follow. Off means upstream's latest.json is never queried, and the banner can then only be raised by
 *  a version mismatch between our own surfaces.
 *
 *  Only a shipped build draws this. Core withholds the query from a development channel outright
 *  (`update_check::is_disabled`), so there the switch would write a value nothing reads, leaving "on" to
 *  claim a check that never runs — which is why the section holding it is not built on that channel, the
 *  same reasoning that keeps the menu item that would run one from being built. */
function UpdateCheckSetting() {
  const on = useSyncExternalStore(subscribe, () => getSnapshot().updateCheck);
  const change = (e: React.ChangeEvent<HTMLSelectElement>) => { void setUpdateCheck(e.target.value === "on"); };
  return (
    <div className="settings__row">
      <span className="settings__k">{t("settings.updateCheck")}</span>
      <span>
        <select className="btn" value={on ? "on" : "off"} onChange={change}>
          <option value="on">{t("settings.updateCheckOn")}</option>
          <option value="off">{t("settings.updateCheckOff")}</option>
        </select>
        <div className="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("settings.updateCheckNote")}</div>
      </span>
    </div>
  );
}

/** Turn "start when I log in" (config.autostart) on or off. It subscribes to autostart in the snapshot,
 *  which core rewrites only after the OS registration was actually written, so a switch that moved is a
 *  login that changed. What comes up is the ordinary window — there is no tray-resident shape behind it.
 *
 *  Only a shipped build draws this. A development build registers nothing at login, because the
 *  executable it would name sits in a working tree that gets rebuilt and thrown away, and the leftover
 *  registration would go on trying to start a file that is gone. */
function AutostartSetting() {
  const on = useSyncExternalStore(subscribe, () => getSnapshot().autostart);
  const change = (e: React.ChangeEvent<HTMLSelectElement>) => { void setAutostart(e.target.value === "on"); };
  return (
    <div className="settings__row">
      <span className="settings__k">{t("settings.autostart")}</span>
      <span>
        <select className="btn" value={on ? "on" : "off"} onChange={change}>
          <option value="on">{t("settings.autostartOn")}</option>
          <option value="off">{t("settings.autostartOff")}</option>
        </select>
        <div className="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("settings.autostartNote")}</div>
      </span>
    </div>
  );
}

/** Turn the hourly tick on or off (`config.tick_consent`). On is a yes and the registration written into
 *  this machine's scheduler; off is a no and the registration taken away — core writes the scheduler
 *  first either way, so a switch that moved is a timer that moved (`tick_answer`).
 *
 *  **Two positions over three states.** The answer has a third — never asked — and it is not a setting to
 *  sit in: what it means on the machine is that no timer is held, which is what off already says. What
 *  the third state decides is whether the band may still put the question (`AMB-D-718`), and that is
 *  core's to judge and never this row's.
 *
 *  **It says where things stand, and the band says why they should change.** The two texts are written
 *  apart on purpose: a reader arrives here to change a setting, not to be sold one.
 *
 *  Unlike the two rows above it, a development build draws this one. Nothing is registered that was not
 *  asked for, so the build that was asked is the build that has to be able to take it back — and every
 *  target this screen is built for has a door into its scheduler. */
function TickSetting() {
  const consent = useSyncExternalStore(subscribe, () => getSnapshot().tickConsent);
  const leavesARow = useSyncExternalStore(subscribe, () => getSnapshot().tickRemovalLeavesARow);
  const [error, setError] = useState<string | null>(null);
  // Said once, right after a switch-off that went through, and only where the OS keeps a row of its own
  // past the removal. It is the moment the reader would otherwise read the row still sitting in their
  // login items as the removal having failed — the same sentence `tick uninstall` says, on the face it
  // was pressed on.
  const [rowRemains, setRowRemains] = useState(false);
  const change = async (e: React.ChangeEvent<HTMLSelectElement>) => {
    const yes = e.target.value === "on";
    setError(null);
    setRowRemains(false);
    try {
      await answerTick(yes);
      setRowRemains(!yes && leavesARow);
    } catch (err) {
      setError(errText(err));
    }
  };
  return (
    <div className="settings__row">
      <span className="settings__k">{t("settings.tick")}</span>
      <span>
        <select className="btn" value={consent === "yes" ? "on" : "off"} onChange={(e) => void change(e)}>
          <option value="on">{t("settings.tickOn")}</option>
          <option value="off">{t("settings.tickOff")}</option>
        </select>
        <div className="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("settings.tickNote")}</div>
        {rowRemains && (
          <div style={{ fontSize: "var(--fs-xs)" }}><DoneNote>{t("settings.tickRowRemains")}</DoneNote></div>
        )}
        {error && <ErrorNote tone="quiet">{error}</ErrorNote>}
      </span>
    </div>
  );
}

function Category({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="settings__section">
      <div className="settings__h">{title}</div>
      <div className="settings__body">{children}</div>
    </div>
  );
}
/** Show where the data actually lives, under Settings > Data. On mount it asks core (`store_locations`)
 *  for the real path — OS-independent, the app-data root that holds the single store — and shows it. */
function DataLocationSetting() {
  const [loc, setLoc] = useState<StoreLocationsDto | null>(null);
  useEffect(() => {
    void fetchStoreLocations().then(setLoc).catch(() => {});
  }, []);
  return (
    <div className="settings__row">
      <span className="settings__k">{t("settings.dataPath")}</span>
      <code style={{ fontSize: "var(--fs-xs)", wordBreak: "break-all" }}>{loc ? loc.root : "…"}</code>
    </div>
  );
}

/** The logs, one click from the path they sit under (`AMB-D-382`). Reporting a bug means attaching them,
 *  and "open your data folder, then find logs/" is where that ask stops being followed. It opens the
 *  folder rather than a file because `amenbo.log` and `perf.log` are both wanted. Core answers when
 *  there is nothing there yet, and that answer is shown as it came. */
function LogsSetting() {
  const [error, setError] = useState<string | null>(null);
  const open = async () => {
    setError(null);
    try {
      await openLogsDir();
    } catch (e) {
      setError(errText(e));
    }
  };
  return (
    <div className="settings__row">
      <span className="settings__k">{t("settings.logs")}</span>
      <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
        <div><button className="btn" onClick={() => void open()}>{t("settings.logsOpen")}</button></div>
        <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("settings.logsNote")}</span>
        {error && <ErrorNote tone="quiet">{error}</ErrorNote>}
      </div>
    </div>
  );
}
