import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import type { CSSProperties, PointerEvent as ReactPointerEvent } from "react";
import { TopBar } from "./TopBar";
import { useNavHistory, NO_SELECTION } from "./navHistory";
import { isBlankSpaceClose } from "./outsideClose";
import { Sidebar } from "./Sidebar";
import { BoardScreen } from "../screens/BoardScreen";
import { ActivityFeed } from "../screens/ActivityFeed";
import { CommandCatalogScreen } from "../screens/CommandCatalogScreen";
import { PluginInstalledScreen } from "../screens/PluginInstalledScreen";
import { PluginMarketScreen } from "../screens/PluginMarketScreen";
import { PluginUpdateBanner } from "../components/PluginUpdateBanner";
import { UpdateBanner, UpdateCheckFeedback } from "../components/UpdateBanner";
import { HealthBanner } from "../components/HealthBanner";
import { ManagedBlockBanner } from "../components/ManagedBlockBanner";
import { OrphanBindingBanner } from "../components/OrphanBindingBanner";
import { HookSetupBanner } from "../components/HookSetupBanner";
import { ListScreen } from "../screens/ListScreen";
import { SearchScreen } from "../screens/SearchScreen";
import { SettingsScreen } from "../screens/SettingsScreen";
import { OnboardingScreen } from "../screens/OnboardingScreen";
import { HookConsentModal } from "../screens/HookConsentModal";
import { NudgeHost } from "../screens/NudgeHost";
import { NewProjectScreen } from "../screens/NewProjectScreen";
import { ProjectSettingsScreen } from "../screens/ProjectSettingsScreen";
import { McpAppsScreen } from "../screens/McpAppsScreen";
import { TaskDetailPane } from "../screens/TaskDetailPane";
import { DecisionDetailPane } from "../screens/DecisionDetailPane";
import { TaskComposePane } from "../screens/TaskComposePane";
import { dataAdapter } from "../mock/adapter";
import { checkForUpdatesFresh, inTauri, subscribe } from "../core/snapshot";
import { confirmDialog } from "../core/dialog";
import { clampRightpaneWidth, getRightpaneWidth, setRightpaneWidth } from "../core/rightpaneWidth";
import { clampSidebarWidth, getSidebarWidth, setSidebarWidth } from "../core/sidebarWidth";
import { getSidebarCollapsed, setSidebarCollapsed } from "../core/sidebarCollapsed";
import { RefNavProvider } from "../core/refNav";
import { currentLang, t } from "../core/i18n";

/**
 * `projectSettings` is the settings screen, carrying the project id in `id`. Reached from the ⚙ in the board toolbar.
 *
 * `pick` is the project a screen should arrive already holding — the one the creation screen just
 * raised, carried into the MCP screen so its rows open on it (`AMB-D-684`). It is part of where you
 * are rather than a message passed alongside, so ＜/＞ land back on the same screen holding the same
 * project; a way in that names no project simply leaves it off.
 */
export type Nav = { type: "view" | "project" | "projectSettings"; id: string; pick?: number };

/**
 * Where a new task gets created. A task only gets placed in a project; classification (assigning it to a
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
 * right-pane width survive). Nothing is asked on a first launch: the language comes from the OS (App settles it),
 * the theme follows the OS, and the two display names start on their defaults — all three are the settings
 * screen's to change afterwards.
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

  // Has the hooks question had its turn? The setup banner reports on the same repositories the modal asks about, so it
  // waits for this rather than talking over it — and reads the disk only once the answers have been written to it.
  // The modal may report done more than once; latching a boolean is what makes that harmless. A nudge waits on it too,
  // for the other reason: not the same subject, but the same screen (see the question order at the end of the render).
  const [hooksAsked, setHooksAsked] = useState(false);
  const onHooksAsked = useCallback(() => setHooksAsked(true), []);

  const lang = useSyncExternalStore(subscribe, currentLang);

  // The app menu's "check for updates" action reports its progress here: `checking` while the fresh query runs, then
  // `null` when an update was found (the UpdateBanner takes over), or `uptodate` / `error` as a short-lived note.
  const [updateCheck, setUpdateCheck] = useState<"checking" | "uptodate" | "error" | null>(null);
  // Bumped each time a manual check surfaces an offer, so the UpdateBanner can lift a session dismissal the user has
  // now explicitly overridden by asking again (its persistent dismissal is already cleared in `checkForUpdatesFresh`).
  const [updateRecheck, setUpdateRecheck] = useState(0);

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

  // Whether the sidebar is collapsed (hidden) — a device-local, persisted UI setting. Toggled from the TopBar so the
  // control stays reachable even when the sidebar itself is hidden; core/sidebarCollapsed owns persistence.
  const [sidebarCollapsed, setSidebarCollapsedState] = useState(() => getSidebarCollapsed());
  const toggleSidebar = useCallback(() => {
    setSidebarCollapsedState((c) => setSidebarCollapsed(!c));
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
  // Reports whether the decision is now the selection, the way `selectTask` does: a refused discard is the one path
  // that leaves it where it was, and the callers below must not act on a move that did not happen.
  const selectDecision = useCallback(async (id: number | null): Promise<boolean> => {
    if (id === null && selectedDecisionId === null && compose === null) return true;
    if (id !== null && id === selectedDecisionId && compose === null) return true;
    if (!(await guardDirty())) return false;
    setCompose(null);
    go({ nav, sel: id === null ? NO_SELECTION : { type: "decision", id } });
    return true;
  }, [selectedDecisionId, compose, guardDirty, nav, go]);
  // The decision-side twins of replyToTask / editCommentInTask, for the activity feed's decision rows.
  const [decisionReplyFocus, setDecisionReplyFocus] = useState<{ decisionId: number; nonce: number } | null>(null);
  const replyToDecision = useCallback(async (id: number) => {
    if (await selectDecision(id)) setDecisionReplyFocus((prev) => ({ decisionId: id, nonce: (prev?.nonce ?? 0) + 1 }));
  }, [selectDecision]);
  const [decisionEditFocus, setDecisionEditFocus] =
    useState<{ decisionId: number; commentId: number; nonce: number } | null>(null);
  const editDecisionCommentIn = useCallback(async (decisionId: number, commentId: number) => {
    if (await selectDecision(decisionId)) {
      setDecisionEditFocus((prev) => ({ decisionId, commentId, nonce: (prev?.nonce ?? 0) + 1 }));
    }
  }, [selectDecision]);

  const openCompose = (target: ComposeTarget) => {
    setCompose(target);
  };
  // After a save, delete or create we simply close (nothing is unsaved). Clear dirty and push a Location with no selection.
  const closeRight = () => {
    rightDirtyRef.current = false;
    setCompose(null);
    setReplyFocus(null);
    setDecisionReplyFocus(null);
    go({ nav, sel: NO_SELECTION });
  };
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

  // The app menu's "check for updates" click arrives as a Tauri event (menu.rs → lib.rs). Run the fresh check and let
  // the outcome drive the feedback note; `available` clears the note because the UpdateBanner shows the offer instead.
  useEffect(() => {
    if (!inTauri()) return;
    let unlisten: (() => void) | undefined;
    let disposed = false;
    void import("@tauri-apps/api/event")
      .then(({ listen }) =>
        listen("menu://check-updates", () => {
          setUpdateCheck("checking");
          void checkForUpdatesFresh().then((r) => {
            setUpdateCheck(r === "available" ? null : r);
            if (r === "available") setUpdateRecheck((n) => n + 1); // lift any session dismissal for the surfaced offer
          });
        }),
      )
      .then((un) => {
        if (disposed) un();
        else unlisten = un;
      });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  const isTaskScreen = nav.type === "project" || (nav.type === "view" && LIST_VIEWS.includes(nav.id));
  const isActivityScreen = nav.type === "view" && nav.id === "activity";
  // Search shows the pane for the same reason activity does: its rows name tasks and decisions both, and
  // the excerpt is a pointer — pressing it has to land somewhere that holds the whole of what it points at.
  const isSearchScreen = nav.type === "view" && nav.id === "search";
  const showRight = (isTaskScreen || isActivityScreen || isSearchScreen)
    && (selectedTaskId !== null || compose !== null || selectedDecisionId !== null);

  const refNav = useMemo(() => ({ selectTask, selectDecision }), [selectTask, selectDecision]);

  useEffect(() => {
    if (!showRight) return;
    const onDown = (e: PointerEvent) => {
      if (isBlankSpaceClose(e.target as Node | null, rightpaneRef.current)) {
        void requestCloseRight(); // A blank-space click closes the pane (confirming the discard if there is unsaved input)
      }
    };
    document.addEventListener("pointerdown", onDown);
    return () => document.removeEventListener("pointerdown", onDown);
  }, [showRight]);

  return (
    <RefNavProvider value={refNav}>
    <div className="shell" key={lang}>
      <TopBar
        onBack={goBack}
        onForward={goForward}
        canBack={canBack}
        canForward={canForward}
        sidebarCollapsed={sidebarCollapsed}
        onToggleSidebar={toggleSidebar}
      />
      <UpdateBanner recheck={updateRecheck} />
      <UpdateCheckFeedback state={updateCheck} onDismiss={() => setUpdateCheck(null)} />
      <PluginUpdateBanner onOpenInstalled={() => navTo({ type: "view", id: "pluginsInstalled" })} />
      <HealthBanner />
      <ManagedBlockBanner />
      <OrphanBindingBanner />
      <HookSetupBanner asked={hooksAsked} />
      <div
        className={`shell__body ${showRight ? "" : "shell__body--no-right"}${sidebarCollapsed ? " shell__body--sidebar-collapsed" : ""}`}
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
              onOpenMcp={() => navTo({ type: "view", id: "mcp" })}
            />
          )}
          {nav.type === "view" && LIST_VIEWS.includes(nav.id) && (
            <ListScreen viewId={nav.id} headerSlot={headerSlot} selectedTaskId={selectedTaskId} onSelectTask={selectTask} />
          )}
          {nav.type === "view" && nav.id === "activity" && (
            <ActivityFeed
              onOpenTask={selectTask}
              onOpenDecision={selectDecision}
              onReplyToTask={replyToTask}
              onReplyToDecision={replyToDecision}
              onEditComment={editCommentInTask}
              onEditDecisionComment={editDecisionCommentIn}
            />
          )}
          {nav.type === "view" && nav.id === "search" && (
            <SearchScreen onOpenTask={selectTask} onOpenDecision={selectDecision} />
          )}
          {nav.type === "view" && nav.id === "commands" && <CommandCatalogScreen />}
          {nav.type === "view" && nav.id === "plugins" && (
            <PluginMarketScreen
              onOpenInstalled={() => navTo({ type: "view", id: "pluginsInstalled" })}
            />
          )}
          {nav.type === "view" && nav.id === "pluginsInstalled" && <PluginInstalledScreen />}
          {nav.type === "view" && nav.id === "mcp" && <McpAppsScreen pick={nav.pick ?? null} />}
          {nav.type === "view" && nav.id === "settings" && <SettingsScreen />}
          {nav.type === "view" && nav.id === "onboarding" && <OnboardingScreen onNav={navTo} />}
          {nav.type === "view" && nav.id === "newProject" && (
            <NewProjectScreen
              onCreated={navTo}
              onCancel={goBack}
              onOpenMcp={(projectId) => navTo({ type: "view", id: "mcp", pick: projectId })}
            />
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
                  focusCommentAt={decisionReplyFocus?.decisionId === selectedDecisionId
                    ? decisionReplyFocus.nonce
                    : undefined}
                  editCommentAt={decisionEditFocus?.decisionId === selectedDecisionId
                    ? { commentId: decisionEditFocus.commentId, nonce: decisionEditFocus.nonce }
                    : undefined}
                />
              ) : null}
            </div>
          </div>
        )}
      </div>

      {/* One question at a time, in this order: the hooks question, then a nudge. The second waits on the
          first by name rather than through a queue of questions — the two are not alike (a modal, and whatever
          a nudge's own view draws), so a queue would be a machine holding one member of each kind and deciding
          nothing. What it would centralise is the order, and the order is right here. Nothing precedes
          them: a first launch asks for no setup at all. */}
      <HookConsentModal onDone={onHooksAsked} />
      {/* A nudge is the less urgent of the two and goes last — it is raised on the strength of how much
          amenbo has been used, which is exactly what someone still being asked the first question has not
          done yet. `hooksAsked` is that turn being over, the same latch the setup banner waits on. */}
      {hooksAsked && <NudgeHost />}
    </div>
    </RefNavProvider>
  );
}


function PaneHeader({ onClose }: { onClose: () => void }) {
  return (
    <div style={{ display: "flex", justifyContent: "flex-end", padding: "4px 8px", borderBottom: "1px solid var(--c-border)" }}>
      <button className="feed__action" onClick={onClose}>✕ {t("pane.close")}</button>
    </div>
  );
}
