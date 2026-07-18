import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import type { CSSProperties, PointerEvent as ReactPointerEvent } from "react";
import { TopBar } from "./TopBar";
import { useNavHistory, NO_SELECTION } from "./navHistory";
import { isBlankSpaceClose } from "./outsideClose";
import { Sidebar } from "./Sidebar";
import { BoardScreen } from "../screens/BoardScreen";
import { ActivityFeed } from "../screens/ActivityFeed";
import { CommandCatalogScreen } from "../screens/CommandCatalogScreen";
import { ListScreen } from "../screens/ListScreen";
import { SettingsScreen } from "../screens/SettingsScreen";
import { OnboardingScreen } from "../screens/OnboardingScreen";
import { OnboardingSetup } from "../screens/OnboardingSetup";
import { HookConsentModal } from "../screens/HookConsentModal";
import { NewProjectScreen } from "../screens/NewProjectScreen";
import { ProjectSettingsScreen } from "../screens/ProjectSettingsScreen";
import { TaskDetailPane } from "../screens/TaskDetailPane";
import { DecisionDetailPane } from "../screens/DecisionDetailPane";
import { TaskComposePane } from "../screens/TaskComposePane";
import { dataAdapter } from "../mock/adapter";
import { getSnapshot, inTauri, subscribe } from "../core/snapshot";
import { confirmDialog } from "../core/dialog";
import { clampRightpaneWidth, getRightpaneWidth, setRightpaneWidth } from "../core/rightpaneWidth";
import { clampSidebarWidth, getSidebarWidth, setSidebarWidth } from "../core/sidebarWidth";
import { RefNavProvider } from "../core/refNav";
import { currentLang, doctorText, t, tf } from "../core/i18n";
import { fetchStaleManagedBlocks, resyncManagedBlocks, fetchOrphanBindings, forgetOrphanBindings, fetchPointerIssues, repairPointers, fetchHookNotices, openLatestInstaller } from "../core/mutations";
import type { DoctorIssueDto, HookNoticeDto, StaleBlockDto } from "../bindings/bindings";

/** `projectSettings` is the settings screen, carrying the project id in `id`. Reached from the ⚙ in the board toolbar. */
export type Nav = { type: "view" | "project" | "projectSettings"; id: string };

/**
 * Where a new task gets created. A task only gets a project membership; classification (assigning it to a
 * dimension) is added afterwards from the task detail. `label` is the heading of the compose pane (the project
 * name), while write routing resolves from `projectId`.
 */
export type ComposeTarget = { projectId: number; label: string };

/** The smart views opened in the list screen. Browsing completed work is a project's list view plus a status:done filter. */
const LIST_VIEWS = ["inbox"];

/**
 * The app frame. Nav and the right-pane selection are folded into a single Location on the history stack (so ＜/＞
 * restore both, and there is no second source of truth); only the transient compose pane stays off the history and
 * takes precedence over the selection when rendering. The language (config.language) lives outside the store, so it
 * never rides watchStore's store-changed — but setLanguage fires the snapshot listeners after its ack, so we
 * subscribe to `currentLang` and key the shell root on it: a switch remounts everything below, which is how
 * unsubscribed chrome and useQuery-driven screens pick up the new language (AppShell's own nav history and
 * right-pane width survive). Whether first-run setup is needed is decided by config.onboarded alone, never by the
 * presence of a store, so even on a first launch with no store yet it layers as a modal over OnboardingScreen and
 * the name entry is best-effort.
 */
export function AppShell() {
  // The initial screen is the first real project, resolved without regard to the current directory. With none at
  // all (no store on this machine = an explicit empty state) we show onboarding. No task is selected by default.
  const [initialNav] = useState<Nav>(() => {
    const first = dataAdapter.listProjects()[0];
    return first ? { type: "project", id: String(first.id) } : { type: "view", id: "onboarding" };
  });
  const { loc, go, back, forward, canBack, canForward } = useNavHistory(initialNav);
  const nav = loc.nav;
  // The right-pane selection is derived from the current Location (no separate state; ＜/＞ restore the selection too).
  const selectedTaskId = loc.sel.type === "task" ? loc.sel.id : null;
  const selectedDecisionId = loc.sel.type === "decision" ? loc.sel.id : null;
  const [compose, setCompose] = useState<ComposeTarget | null>(null);

  const needsSetup = useSyncExternalStore(
    subscribe,
    () => !getSnapshot().onboarded,
  );

  // Has the hooks question had its turn? The setup banner reports on the same repositories the modal asks about, so it
  // waits for this rather than talking over it — and reads the disk only once the answers have been written to it.
  // The modal may report done more than once; latching a boolean is what makes that harmless.
  const [hooksAsked, setHooksAsked] = useState(false);
  const onHooksAsked = useCallback(() => setHooksAsked(true), []);

  const lang = useSyncExternalStore(subscribe, currentLang);

  // The slot the project header (the board toolbar) renders into. grid-area:header is a row spanning the full width
  // of main plus the right pane; BoardScreen portals its toolbar in here and the right pane sits below it.
  // A callback ref keeps the DOM node in state so the re-render after it settles can hand the portal target to the child.
  const [headerSlot, setHeaderSlot] = useState<HTMLDivElement | null>(null);

  // The left sidebar's width (a device-local, persisted UI setting). Dragging the right-edge handle widens it, up to
  // ~40% of the window; core/sidebarWidth owns the default and the bounds.
  const [sidebarWidth, setSidebarWidthState] = useState(() => getSidebarWidth());
  // pointerdown on the right-edge handle starts the drag: while moving we update state for immediate feedback, and
  // persist on pointerup. Width = the pointer's distance from the left edge of the viewport (sidebarWidth clamps it).
  const startSidebarResize = useCallback((e: ReactPointerEvent) => {
    e.preventDefault();
    const onMove = (ev: PointerEvent) => setSidebarWidthState(clampSidebarWidth(ev.clientX));
    const onUp = (ev: PointerEvent) => {
      document.removeEventListener("pointermove", onMove);
      document.removeEventListener("pointerup", onUp);
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
      setSidebarWidthState(setSidebarWidth(ev.clientX));
    };
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
    document.addEventListener("pointermove", onMove);
    document.addEventListener("pointerup", onUp);
  }, []);

  const rightpaneRef = useRef<HTMLDivElement>(null);
  // The right pane's width (a device-local, persisted UI setting). Dragging the left-edge handle widens it, up to
  // ~50% of the window; core/rightpaneWidth owns the default and the bounds.
  const [rightWidth, setRightWidth] = useState(() => getRightpaneWidth());
  // pointerdown on the left-edge handle starts the drag: while moving we update state for immediate feedback, and
  // persist on pointerup. Width = the distance from the right edge of the viewport to the pointer (rightpaneWidth clamps it).
  const startResize = useCallback((e: ReactPointerEvent) => {
    e.preventDefault();
    const onMove = (ev: PointerEvent) => setRightWidth(clampRightpaneWidth(window.innerWidth - ev.clientX));
    const onUp = (ev: PointerEvent) => {
      document.removeEventListener("pointermove", onMove);
      document.removeEventListener("pointerup", onUp);
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
      setRightWidth(setRightpaneWidth(window.innerWidth - ev.clientX));
    };
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
    document.addEventListener("pointermove", onMove);
    document.addEventListener("pointerup", onUp);
  }, []);
  // Whether the right pane (detail / compose) holds unsaved input. Each pane reports it through onDirtyChange, and
  // it guards against data loss before an outside click or the ✕ closes the pane (a ref, so we read the latest value
  // without adding a subscription).
  const rightDirtyRef = useRef(false);
  const setRightDirty = useCallback((dirty: boolean) => { rightDirtyRef.current = dirty; }, []);

  // The shared gate that interposes a discard confirmation when there is unsaved input. Returns true (and clears dirty) on OK, or when it was already clean.
  const guardDirty = useCallback(async (): Promise<boolean> => {
    if (rightDirtyRef.current && !(await confirmDialog(t("pane.discardConfirm")))) return false;
    rightDirtyRef.current = false;
    return true;
  }, [t]);

  const selectTask = useCallback(async (id: number): Promise<boolean> => {
    if (id === selectedTaskId && compose === null) return true;
    if (!(await guardDirty())) return false;
    setCompose(null);
    go({ nav, sel: { type: "task", id } });
    return true;
  }, [selectedTaskId, compose, guardDirty, nav, go]);
  const [replyFocus, setReplyFocus] = useState<{ taskId: number; nonce: number } | null>(null);
  const replyToTask = useCallback(async (id: number) => {
    if (await selectTask(id)) setReplyFocus((prev) => ({ taskId: id, nonce: (prev?.nonce ?? 0) + 1 }));
  }, [selectTask]);
  const [editFocus, setEditFocus] = useState<{ taskId: number; commentId: number; nonce: number } | null>(null);
  const editCommentInTask = useCallback(async (taskId: number, commentId: number) => {
    if (await selectTask(taskId)) {
      setEditFocus((prev) => ({ taskId, commentId, nonce: (prev?.nonce ?? 0) + 1 }));
    }
  }, [selectTask]);
  const selectDecision = useCallback(async (id: number | null) => {
    if (id === null && selectedDecisionId === null && compose === null) return;
    if (id !== null && id === selectedDecisionId && compose === null) return;
    if (!(await guardDirty())) return;
    setCompose(null);
    go({ nav, sel: id === null ? NO_SELECTION : { type: "decision", id } });
  }, [selectedDecisionId, compose, guardDirty, nav, go]);

  const openCompose = (target: ComposeTarget) => {
    setCompose(target);
  };
  // After a save, delete or create we simply close (nothing is unsaved). Clear dirty and push a Location with no selection.
  const closeRight = () => { rightDirtyRef.current = false; setCompose(null); setReplyFocus(null); go({ nav, sel: NO_SELECTION }); };
  const afterCreate = (newId: number | null) => {
    rightDirtyRef.current = false;
    setCompose(null);
    go({ nav, sel: newId !== null ? { type: "task", id: newId } : NO_SELECTION });
  };
  // Switching nav (project / view) does not carry the right-pane selection over: push a Location with no selection.
  const navTo = useCallback((n: Nav) => { go({ nav: n, sel: NO_SELECTION }); }, [go]);
  // The escape hatch for when a project leaves the list through archive or delete (the settings screen's onGone).
  // Go to the first project still in the refetched snapshot, or to onboarding if there is none.
  const goToFirstProject = useCallback(() => {
    const first = dataAdapter.listProjects()[0];
    navTo(first ? { type: "project", id: String(first.id) } : { type: "view", id: "onboarding" });
  }, [navTo]);
  const goBack = useCallback(async () => { if (await guardDirty()) back(); }, [back, guardDirty]);
  const goForward = useCallback(async () => { if (await guardDirty()) forward(); }, [forward, guardDirty]);
  // Closing via a click on blank space (outside a row or card), the ✕, or Cancel. With unsaved input we interpose the discard confirmation and close only on OK.
  const requestCloseRight = async () => {
    if (!(await guardDirty())) return;
    closeRight();
  };

  // Re-clamp the current widths on every resize, so shrinking the window cannot leave a pane over its cap (the right
  // pane at ~50%, the sidebar at ~40% of the window).
  useEffect(() => {
    const onResize = () => {
      setRightWidth((w) => clampRightpaneWidth(w));
      setSidebarWidthState((w) => clampSidebarWidth(w));
    };
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);

  useEffect(() => {
    if (!inTauri()) return;
    let unlisten: (() => void) | undefined;
    let disposed = false;
    void import("@tauri-apps/api/event")
      .then(({ listen }) => listen("notification-activated", () => navTo({ type: "view", id: "inbox" })))
      .then((un) => {
        if (disposed) un();
        else unlisten = un;
      });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [navTo]);

  const isTaskScreen = nav.type === "project" || (nav.type === "view" && LIST_VIEWS.includes(nav.id));
  const isActivityScreen = nav.type === "view" && nav.id === "activity";
  const showRight = (isTaskScreen || isActivityScreen) && (selectedTaskId !== null || compose !== null || selectedDecisionId !== null);

  const refNav = useMemo(() => ({ selectTask, selectDecision }), [selectTask, selectDecision]);

  useEffect(() => {
    if (!showRight) return;
    if (needsSetup) return;
    const onDown = (e: PointerEvent) => {
      if (isBlankSpaceClose(e.target as Node | null, rightpaneRef.current)) {
        void requestCloseRight(); // A blank-space click closes the pane (confirming the discard if there is unsaved input)
      }
    };
    document.addEventListener("pointerdown", onDown);
    return () => document.removeEventListener("pointerdown", onDown);
  }, [showRight, needsSetup]);

  return (
    <RefNavProvider value={refNav}>
    <div className="shell" key={lang}>
      <TopBar
        onBack={goBack}
        onForward={goForward}
        canBack={canBack}
        canForward={canForward}
      />
      <UpdateBanner />
      <HealthBanner />
      <ManagedBlockBanner />
      <OrphanBindingBanner />
      <HookSetupBanner asked={hooksAsked} />
      <div
        className={`shell__body ${showRight ? "" : "shell__body--no-right"}`}
        style={{ "--rightpane-w": `${rightWidth}px`, "--sidebar-w": `${sidebarWidth}px` } as CSSProperties}
      >
        <div className="sidebar-wrap">
          <Sidebar nav={nav} onNav={navTo} />
          <div
            className="sidebar__resizer"
            role="separator"
            aria-orientation="vertical"
            title={t("sidebar.resize")}
            onPointerDown={startSidebarResize}
          />
        </div>

        {/* The full-width slot for the project header (the board toolbar is portalled in here). */}
        <div className="shell__header" ref={setHeaderSlot} />

        <div className="main">
          {nav.type === "project" && (
            <BoardScreen
              projectId={Number(nav.id)}
              headerSlot={headerSlot}
              selectedTaskId={selectedTaskId}
              onSelectTask={selectTask}
              selectedDecisionId={selectedDecisionId}
              onSelectDecision={selectDecision}
              onComposeTask={openCompose}
              onOpenSettings={() => navTo({ type: "projectSettings", id: nav.id })}
            />
          )}
          {nav.type === "projectSettings" && (
            <ProjectSettingsScreen
              projectId={Number(nav.id)}
              onBack={() => navTo({ type: "project", id: nav.id })}
              onGone={goToFirstProject}
            />
          )}
          {nav.type === "view" && LIST_VIEWS.includes(nav.id) && (
            <ListScreen viewId={nav.id} headerSlot={headerSlot} selectedTaskId={selectedTaskId} onSelectTask={selectTask} />
          )}
          {nav.type === "view" && nav.id === "activity" && (
            <ActivityFeed
              onOpenTask={selectTask}
              onReplyToTask={replyToTask}
              onEditComment={editCommentInTask}
            />
          )}
          {nav.type === "view" && nav.id === "commands" && <CommandCatalogScreen />}
          {nav.type === "view" && nav.id === "settings" && <SettingsScreen />}
          {nav.type === "view" && nav.id === "onboarding" && <OnboardingScreen onNav={navTo} />}
          {nav.type === "view" && nav.id === "newProject" && (
            <NewProjectScreen onCreated={navTo} onCancel={goBack} />
          )}
        </div>

        {showRight && (
          <div className="rightpane-wrap" ref={rightpaneRef}>
            <div
              className="rightpane__resizer"
              role="separator"
              aria-orientation="vertical"
              title={t("pane.resize")}
              onPointerDown={startResize}
            />
            <div className="rightpane">
              <PaneHeader onClose={() => void requestCloseRight()} />
              {compose ? (
                <TaskComposePane
                  projectId={compose.projectId}
                  label={compose.label}
                  onCreated={afterCreate}
                  onCancel={() => void requestCloseRight()}
                  onDirtyChange={setRightDirty}
                />
              ) : selectedTaskId ? (
                <TaskDetailPane
                  taskId={selectedTaskId}
                  onDeleted={closeRight}
                  onDirtyChange={setRightDirty}
                  onSelectDecision={selectDecision}
                  focusCommentAt={replyFocus?.taskId === selectedTaskId ? replyFocus.nonce : undefined}
                  editCommentAt={editFocus?.taskId === selectedTaskId
                    ? { commentId: editFocus.commentId, nonce: editFocus.nonce }
                    : undefined}
                />
              ) : selectedDecisionId ? (
                <DecisionDetailPane
                  decisionId={selectedDecisionId}
                  onOpenTask={selectTask}
                  onOpenDecision={selectDecision}
                />
              ) : null}
            </div>
          </div>
        )}
      </div>

      {needsSetup && <OnboardingSetup />}
      {/* First-run setup owns the screen while it is up: the hooks question is asked about repositories, which
          is not what someone still choosing a language came here for, and it keeps its turn until then. */}
      {!needsSetup && <HookConsentModal onDone={onHooksAsked} />}
    </div>
    </RefNavProvider>
  );
}

// Version skew across surfaces, with no network involved: if another surface (CLI or GUI) has touched this store
// with a version newer than ours, we show "an update is available" right under the TopBar. The "open the installer"
// button only opens the current OS's all-in-one installer (GUI + CLI bundled) in the default browser — it never
// self-updates. It can be dismissed with the ✕ for this session (and is evaluated again on the next launch).
function UpdateBanner() {
  const vs = useSyncExternalStore(subscribe, () => getSnapshot().versionStatus);
  const [dismissed, setDismissed] = useState(false);
  const [busy, setBusy] = useState(false);
  if (dismissed || !vs.updateAvailable) return null;
  const onOpen = async () => {
    setBusy(true);
    try {
      await openLatestInstaller(); // core resolves the installer URL for the current OS and opens it (falling back to the releases page).
    } catch {
      // Leave the banner up even if it will not open (the user can retry, or download it by hand).
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="healthbanner" role="alert">
      <span className="healthbanner__icon" aria-hidden>⬆</span>
      <div className="healthbanner__body">
        <div className="healthbanner__title">{t("update.title")}{vs.newerVersion ? ` (${vs.newerVersion})` : ""}</div>
        <div className="healthbanner__hint">{t("update.hint")}</div>
      </div>
      {inTauri() && (
        <button className="healthbanner__action" onClick={onOpen} disabled={busy}>{t("update.open")}</button>
      )}
      <button className="healthbanner__close" onClick={() => setDismissed(true)}>✕ {t("update.dismiss")}</button>
    </div>
  );
}

// The banner speaks for two layers. What is inside the store (`startupHealth`) is carried by the snapshot on every
// tick, but issues with a bound folder's `.amenbo` (legacy format, or gone) are asked of core exactly once at startup
// (`pointer_issues`) — probing the environment costs an FS walk per folder, which is not a price to pay on every tick
// that tracks store changes. Broken pointers can be fixed from this banner (`repair_pointers`).
export function HealthBanner() {
  const health = useSyncExternalStore(subscribe, () => getSnapshot().startupHealth);
  const [pointers, setPointers] = useState<DoctorIssueDto[]>([]);
  const [dismissed, setDismissed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [repaired, setRepaired] = useState(0);

  useEffect(() => {
    if (!inTauri()) return;
    let alive = true;
    fetchPointerIssues()
      .then((p) => alive && setPointers(p))
      .catch(() => {}); // A failure to detect is swallowed (we just do not show that line).
    return () => {
      alive = false;
    };
  }, []);

  // A folder whose owner is not uniquely determined comes back from core as `unresolved`, so only the rows we could fix disappear and the rest remain.
  const onRepair = async () => {
    setBusy(true);
    try {
      const report = await repairPointers();
      setPointers(await fetchPointerIssues()); // Confirm they really are fixed, through the detection path itself.
      setRepaired(report.repaired.length);
    } catch {
      // On failure leave the rows where they are (never claim they are fixed).
    } finally {
      setBusy(false);
    }
  };

  const lines = [...health.issues, ...pointers].map((i) => doctorText(i).message);
  if (dismissed) return null;
  if (lines.length === 0) {
    // Right after a repair, and only then, stay up to say so (with nothing at all we never render).
    if (repaired === 0) return null;
    return (
      <div className="healthbanner" role="status">
        <span className="healthbanner__icon" aria-hidden>✓</span>
        <div className="healthbanner__body">
          <div className="healthbanner__title">{tf("health.repaired", { count: repaired })}</div>
        </div>
        <button className="healthbanner__close" onClick={() => setDismissed(true)}>✕ {t("health.dismiss")}</button>
      </div>
    );
  }
  return (
    <div className="healthbanner" role="alert">
      <span className="healthbanner__icon" aria-hidden>⚠</span>
      <div className="healthbanner__body">
        <div className="healthbanner__title">{t("health.title")}</div>
        {lines.map((message, i) => (
          <div key={i} className="healthbanner__line">{message}</div>
        ))}
        {health.issues.length > 0 && <div className="healthbanner__hint">{t("health.hint")}</div>}
      </div>
      {pointers.length > 0 && (
        <button className="healthbanner__action" onClick={onRepair} disabled={busy}>
          {busy ? t("health.repairing") : t("health.repair")}
        </button>
      )}
      <button className="healthbanner__close" onClick={() => setDismissed(true)} disabled={busy}>✕ {t("health.dismiss")}</button>
    </div>
  );
}

// After a binary update, a bound folder's CLAUDE.md/AGENTS.md can be left holding an older version of the managed
// block. The CLI fixes itself by following along whenever it starts in that folder, but the GUI starts in no folder
// at all, so every bound folder is in scope. When the same core detection path as CLI `doctor`
// (`stale_managed_blocks`) finds stale folders, we offer a line under the TopBar that resyncs them in one click (the
// `resync_managed_blocks` path, the same one CLI `sync-guide` takes). The only side effect is rewriting the md on
// disk (low churn, language label preserved, nothing outside the markers touched); the store is untouched, so no
// snapshot refetch. Detected once at startup, dismissible with the ✕ for the session. Outside Tauri (in the browser)
// it is always empty, hence hidden.
function ManagedBlockBanner() {
  const [stale, setStale] = useState<StaleBlockDto[]>([]);
  const [dismissed, setDismissed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState(false);

  useEffect(() => {
    if (!inTauri()) return;
    let alive = true;
    fetchStaleManagedBlocks()
      .then((s) => alive && setStale(s))
      .catch(() => {}); // A failure to detect is swallowed (we just do not show the banner).
    return () => {
      alive = false;
    };
  }, []);

  if (dismissed || stale.length === 0) return null;

  // How many folders hold a stale block (a folder whose CLAUDE.md and AGENTS.md are both stale still counts once).
  const folderCount = new Set(stale.map((s) => s.dir)).size;

  const onResync = async () => {
    setBusy(true);
    try {
      const report = await resyncManagedBlocks(); // Resync every bound folder to the current version.
      const remaining = await fetchStaleManagedBlocks(); // Check they actually followed along (folders that are gone or renamed can remain).
      setStale(remaining);
      setDone(report.updated.length > 0 && remaining.length === 0);
    } catch {
      // On failure leave the banner up (stale stays as it was).
    } finally {
      setBusy(false);
    }
  };

  if (done) {
    return (
      <div className="healthbanner managedblock-banner" role="status">
        <span className="healthbanner__icon" aria-hidden>✓</span>
        <div className="healthbanner__body">
          <div className="healthbanner__title">{t("managedBlock.done")}</div>
        </div>
        <button className="healthbanner__close" onClick={() => setDismissed(true)}>✕ {t("health.dismiss")}</button>
      </div>
    );
  }

  return (
    <div className="healthbanner managedblock-banner" role="alert">
      <span className="healthbanner__icon" aria-hidden>⚠</span>
      <div className="healthbanner__body">
        <div className="healthbanner__title">{t("managedBlock.title")}</div>
        <div className="healthbanner__line">{tf("managedBlock.hint", { count: folderCount })}</div>
      </div>
      <button className="healthbanner__action" onClick={onResync} disabled={busy}>
        {busy ? t("managedBlock.resyncing") : t("managedBlock.resync")}
      </button>
      <button className="healthbanner__close" onClick={() => setDismissed(true)} disabled={busy}>✕ {t("health.dismiss")}</button>
    </div>
  );
}

// Point out the bound-folder wreckage a deleted project left in the index (rows no live project claims) and forget
// them from the index in one click (the same core path as CLI `doctor --fix`, `forget_orphan_dirs`). The GUI's folder
// list is a reverse lookup per project, so a row with no claimant never shows up there. This drops the index row and
// nothing more — it touches neither the folder's contents nor its `.amenbo`. Detected once at startup, dismissible
// with the ✕ for the session. Outside Tauri (in the browser) it is always empty, hence hidden.
function OrphanBindingBanner() {
  const [dirs, setDirs] = useState<string[]>([]);
  const [dismissed, setDismissed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState(false);

  useEffect(() => {
    if (!inTauri()) return;
    let alive = true;
    fetchOrphanBindings()
      .then((d) => alive && setDirs(d))
      .catch(() => {}); // A failure to detect is swallowed (we just do not show the banner).
    return () => {
      alive = false;
    };
  }, []);

  if (dismissed || dirs.length === 0) return null;

  const onForget = async () => {
    setBusy(true);
    try {
      await forgetOrphanBindings();
      const remaining = await fetchOrphanBindings(); // Check they really were swept (rows added concurrently can remain).
      setDirs(remaining);
      setDone(remaining.length === 0);
    } catch {
      // On failure leave the banner up (dirs stays as it was).
    } finally {
      setBusy(false);
    }
  };

  if (done) {
    return (
      <div className="healthbanner managedblock-banner" role="status">
        <span className="healthbanner__icon" aria-hidden>✓</span>
        <div className="healthbanner__body">
          <div className="healthbanner__title">{t("orphanBinding.done")}</div>
        </div>
        <button className="healthbanner__close" onClick={() => setDismissed(true)}>✕ {t("health.dismiss")}</button>
      </div>
    );
  }

  return (
    <div className="healthbanner managedblock-banner" role="alert">
      <span className="healthbanner__icon" aria-hidden>⚠</span>
      <div className="healthbanner__body">
        <div className="healthbanner__title">{t("orphanBinding.title")}</div>
        <div className="healthbanner__line">{tf("orphanBinding.hint", { count: dirs.length })}</div>
      </div>
      <button className="healthbanner__action" onClick={onForget} disabled={busy}>
        {busy ? t("orphanBinding.forgetting") : t("orphanBinding.forget")}
      </button>
      <button className="healthbanner__close" onClick={() => setDismissed(true)} disabled={busy}>✕ {t("health.dismiss")}</button>
    </div>
  );
}

// The GUI's channel for what core's `hooks::setup_notice` found — the same report the CLI puts in its `--json`
// field and on stderr. It tells and stops nothing, and it takes no answer either, which is what keeps it apart
// from the modal that does (`HookConsentModal`). That is also why it carries no install button: consent has one
// surface, and a banner that installed on a click would be writing into the user's git plumbing from a line they
// never answered.
//
// **It is two banners, not one**, because it has two different things to say and they are not degrees of each
// other (core keeps the lists apart for this reason):
//
//   - unwired — the lint is wired to nothing in these slots, empty or held by another tool alike, so the refs
//     it exists to catch are going out uncaught. `hooks install` is the fix — it writes a standalone hook, or
//     slips amenbo's block in beside another tool's. A warning, and it reads as one. (There is no separate
//     hand-off any more: coexisting is always possible, so a stranger's slot is just a slot to install into.)
//   - restored — a block of ours was found damaged or stale this session and put back (something had changed
//     or removed it — a tool regenerating its hook, a hand-edit). Nothing is unfinished and nothing is asked;
//     it is a heads-up that amenbo repaired itself, so the reader knows the lint had briefly stopped.
//
// It renders only once the modal is done asking (`asked`), because asking about the hooks and warning about the
// hooks in the same breath says one thing twice. That order is what the notice is read after, too: `hook_offer`'s
// sweep has installed a yes's hooks and healed damaged blocks by then, so `unwired` names only what is still
// missing and `restored` names what the sweep just put back. A recorded "no" and an opted-out repository are
// both silent here (core decides), so this cannot become noise to tune out. Dismissible with the ✕ for the
// session. Outside Tauri (in the browser) it is always empty, hence hidden.
export function HookSetupBanner({ asked }: { asked: boolean }) {
  const [notices, setNotices] = useState<HookNoticeDto[]>([]);
  // One dismiss per banner: the two say different things, so closing the "restored" heads-up must not also
  // hide the "not wired" warning (and vice versa).
  const [unwiredDismissed, setUnwiredDismissed] = useState(false);
  const [restoredDismissed, setRestoredDismissed] = useState(false);

  useEffect(() => {
    if (!inTauri() || !asked) return;
    let alive = true;
    fetchHookNotices()
      .then((n) => alive && setNotices(n))
      .catch(() => {}); // A failure to detect is swallowed (we just do not show the banner).
    return () => {
      alive = false;
    };
  }, [asked]);

  const unwired = notices.filter((n) => n.unwired.length > 0);
  const restored = notices.filter((n) => n.restored.length > 0);
  const showUnwired = unwired.length > 0 && !unwiredDismissed;
  const showRestored = restored.length > 0 && !restoredDismissed;
  if (!showUnwired && !showRestored) return null;

  return (
    <>
      {showUnwired && (
        <div className="healthbanner managedblock-banner" role="alert">
          <span className="healthbanner__icon" aria-hidden>⚠</span>
          <div className="healthbanner__body">
            <div className="healthbanner__title">{t("hookSetup.title")}</div>
            {unwired.map((n) => (
              <div key={n.dir} className="healthbanner__line">
                <div>{tf("hookSetup.where", { project: n.projectName, dir: n.dir })}</div>
                <div>{tf("hookSetup.unwired", { slots: n.unwired.join(", "), cmd: `${n.cmd} hooks install` })}</div>
              </div>
            ))}
          </div>
          <button className="healthbanner__close" onClick={() => setUnwiredDismissed(true)}>✕ {t("health.dismiss")}</button>
        </div>
      )}
      {showRestored && (
        <div className="healthbanner managedblock-banner" role="alert">
          <span className="healthbanner__icon" aria-hidden>⚠</span>
          <div className="healthbanner__body">
            <div className="healthbanner__title">{t("hookRestored.title")}</div>
            {restored.map((n) => (
              <div key={n.dir} className="healthbanner__line">
                <div>{tf("hookSetup.where", { project: n.projectName, dir: n.dir })}</div>
                <div>{tf("hookRestored.slots", { slots: n.restored.join(", ") })}</div>
              </div>
            ))}
          </div>
          <button className="healthbanner__close" onClick={() => setRestoredDismissed(true)}>✕ {t("health.dismiss")}</button>
        </div>
      )}
    </>
  );
}

function PaneHeader({ onClose }: { onClose: () => void }) {
  return (
    <div style={{ display: "flex", justifyContent: "flex-end", padding: "4px 8px", borderBottom: "1px solid var(--c-border)" }}>
      <button className="feed__action" onClick={onClose}>✕ {t("pane.close")}</button>
    </div>
  );
}
