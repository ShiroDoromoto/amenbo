import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import type { CSSProperties, PointerEvent as ReactPointerEvent } from "react";
import type { RefTargetDto } from "../bindings/bindings";
import { TopBar } from "./TopBar";
import { TerminalFace } from "./TerminalFace";
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
import { TickBanner } from "../components/TickBanner";
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
import { type Face, getWindowShape, setWindowShape, type WindowShape } from "../core/windowShape";
import { badgeUp, knock, looked, NO_ATTENTION, turnCame } from "./terminalBadge";
import { notifyTurn } from "../core/osNotify";
import { invoke } from "../core/ipc";
import { confirmDialog } from "../core/dialog";
import { clampRightpaneWidth, getRightpaneWidth, setRightpaneWidth } from "../core/rightpaneWidth";
import { clampSidebarWidth, getSidebarWidth, setSidebarWidth } from "../core/sidebarWidth";
import { getSidebarCollapsed, setSidebarCollapsed } from "../core/sidebarCollapsed";
import { RefNavProvider } from "../core/refNav";
import { currentLang, errLabel, t, type CmdError } from "../core/i18n";
import { Icon } from "../components/Icon";

/**
 * `projectSettings` is the settings screen, carrying the project id in `id`. Reached from the gear in the board toolbar.
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
const LIST_VIEWS = ["inbox", "due"];

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

  // Which face this window is showing, and whether the terminal has a window of its own
  // (`AMB-D-753`). A launch shows the ledger; the shape is what the machine was last used in.
  const [face, setFace] = useState<Face>("tasks");
  const [shape, setShapeState] = useState<WindowShape>(() => getWindowShape());
  // Has the terminal been asked for in this window at all? Until it has, there is no pane and no
  // shell — a launch that never leaves the ledger starts no process it was not asked to. Once asked
  // it stays true, because the face is hidden rather than taken down (`TerminalFace`).
  const [terminalAsked, setTerminalAsked] = useState(false);
  const hostsTerminal = shape === "one" && terminalAsked;
  // Splitting out and folding back are the same move seen from either end, and both go through the
  // shape: the window is opened and closed by the effect below, so every way into two windows — the
  // button, and a launch that remembers being two — arrives at the same place.
  const setShape = useCallback((next: WindowShape) => setShapeState(setWindowShape(next)), []);
  // The platform refusing to build the window. It is the one failure here a person has to be told
  // about, because they asked for the window and would otherwise watch the button do nothing.
  const [windowError, setWindowError] = useState<string | null>(null);
  // Whether the window the shape is about to open was asked for by a press, and so comes to the
  // front. A launch restoring the shape was not asking for anything, and the window the user is
  // looking at is this one — "nothing comes forward but what somebody pressed" (`AMB-D-753`).
  const raiseTalk = useRef(false);
  useEffect(() => {
    if (!inTauri()) return;
    if (shape !== "two") {
      void invoke("talk_close").catch(() => {});
      return;
    }
    const raise = raiseTalk.current;
    raiseTalk.current = false;
    void invoke("talk_open", { raise }).catch((e: unknown) => {
      // Back to one window, where the terminal still is: what was split out is put back rather than
      // left pointing at a window that was never built.
      setWindowError(errLabel(e as CmdError));
      setShapeState(setWindowShape("one"));
      setFace("terminal");
    });
  }, [shape]);
  // The talk window going away, however it went: the button that folds the app back, and the title
  // bar's close, which is the one an app that only watched its own button would miss. Either way the
  // terminal is still running and is now nobody's to draw, so this window takes it and shows it —
  // landing where the user was looking when they closed the other window.
  useEffect(() => {
    if (!inTauri()) return;
    let unlisten: (() => void) | undefined;
    let disposed = false;
    void import("@tauri-apps/api/event")
      .then(({ listen }) =>
        listen("talk://closed", () => {
          setShape("one");
          setTerminalAsked(true);
          setFace("terminal");
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
  }, [setShape]);
  // Pressing a segment. With the terminal in a window of its own there is nothing to switch to here,
  // so the press raises that window instead — and never opens a second one (`windows::talk_open`).
  const selectFace = useCallback((next: Face) => {
    if (next === "terminal" && shape === "two") {
      void invoke("talk_open", { raise: true }).catch(() => {});
      return;
    }
    if (next === "terminal") setTerminalAsked(true);
    setFace(next);
  }, [shape]);
  // The folder the ledger has asked the terminal to work in, and a count of the asking: the face is a
  // component, so what it is handed is where to work rather than a call to make (`./TerminalFace`).
  const [openIn, setOpenIn] = useState<{ dir: string; nth: number } | null>(null);
  /**
   * "Start in the terminal" — the one move the first loop offers (`../components/FirstLoop`).
   *
   * With the terminal split out into a window of its own, this window has no face to hand the folder
   * to. What the press does then is raise that window: the terminal is where the reader is being sent
   * either way, and the folder is asked for there rather than promised here (`AMB-D-749`).
   */
  const startTerminalIn = useCallback((dir: string) => {
    if (shape === "two") {
      void invoke("talk_open", { raise: true }).catch(() => {});
      return;
    }
    setOpenIn((asked) => ({ dir, nth: (asked?.nth ?? 0) + 1 }));
    setTerminalAsked(true);
    setFace("terminal");
  }, [shape]);

  // "Open in a separate window". The pane comes down as the shape changes, leaving the terminal
  // running for the window that is about to draw it, and this window goes back to the ledger — the
  // face it is now the only one of.
  const splitOutTerminal = useCallback(() => {
    setWindowError(null);
    raiseTalk.current = true;
    setFace("tasks");
    setShape("two");
  }, [setShape]);

  // A turn standing in the terminal while the ledger is the face that is up (`./terminalBadge`). In
  // two windows there is no badge and nothing to feed it: the terminal is on screen already, with its
  // own nameplates, and this window stops hosting the pane that would speak.
  const [attention, setAttention] = useState(NO_ATTENTION);
  // The face as the pane finds it, not as the render that made the callback saw it: `noteWaiting` is
  // handed to a component that puts its terminal up once, so it has to keep the same identity for the
  // life of the pane — reading the face through a ref is what buys that.
  const facing = useRef(face);
  facing.current = face;
  const noteWaiting = useCallback((waiting: boolean) => {
    setAttention((was) => {
      const now = turnCame(was, waiting, facing.current === "terminal");
      // The badge going up is also the moment to knock on the OS: the same question — a turn came up
      // while the person was not looking at the terminal — answered on the screen for whoever is at it
      // and off the screen for whoever is not (`./terminalBadge`).
      if (knock(was, now)) void notifyTurn();
      return now;
    });
  }, []);
  // Every way onto the terminal face is being shown what is standing there — the segment, the other
  // window closing, a window that could not be built — so the badge is spent here rather than at each
  // of them.
  useEffect(() => {
    if (face === "terminal") setAttention(looked);
  }, [face]);
  useEffect(() => {
    if (!hostsTerminal) setAttention(NO_ATTENTION);
  }, [hostsTerminal]);

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
  // it guards against data loss before an outside click or the cross closes the pane (a ref, so we read the latest value
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
  // Closing via a click on blank space (outside a row or card), the cross, or Cancel. With unsaved input we interpose the discard confirmation and close only on OK.
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
      // Where a click on a toast lands: what it was about says which face, and the host has already
      // raised the window that face is in — with the terminal split out that is a different window,
      // and this one cannot raise it (`crate::notify`).
      .then(({ listen }) =>
        listen<string>("notification-activated", ({ payload }) => {
          if (payload === "turn") selectFace("terminal");
          else navTo({ type: "view", id: "inbox" });
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
  }, [navTo]);

  // A ref clicked in a pane of the talk window. The host has already brought this window forward and
  // settled what was clicked (`crate::windows::show_ref`); what is left is the move this shell alone
  // knows how to make, and it is the same one an in-body ref makes — down to the unsaved-input
  // confirmation, which a ref arriving from the other window has no more right to walk past than one
  // clicked here.
  useEffect(() => {
    if (!inTauri()) return;
    let unlisten: (() => void) | undefined;
    let disposed = false;
    void import("@tauri-apps/api/event")
      .then(({ listen }) => listen<RefTargetDto>("ref-activated", ({ payload }) => {
        if (payload.kind === "task") void selectTask(payload.id);
        else void selectDecision(payload.id);
      }))
      .then((un) => {
        if (disposed) un();
        else unlisten = un;
      });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [selectTask, selectDecision]);

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
        face={face}
        onSelectFace={selectFace}
        terminalBadge={badgeUp(attention)}
      />
      {/* Every band the app can raise, stacked in one place. They share a row of the shell rather than
          claiming one each: the grid needs a definite row per child, and news does not arrive one item
          at a time — an update and a broken pointer can stand together, and each further band would
          otherwise have to be given a row of its own before it could be added. */}
      <div className="shell__banners">
        <UpdateBanner recheck={updateRecheck} />
        <UpdateCheckFeedback state={updateCheck} onDismiss={() => setUpdateCheck(null)} />
        <PluginUpdateBanner onOpenInstalled={() => navTo({ type: "view", id: "pluginsInstalled" })} />
        <HealthBanner />
        <ManagedBlockBanner />
        <OrphanBindingBanner />
        <HookSetupBanner asked={hooksAsked} />
        <TickBanner />
      </div>
      {/* The terminal face, kept up from the moment it is first asked for. `hidden` is what the
          other face being up means here: taking this down would take the emulator with it, and the
          agent running in the pane would have nowhere to come back to (`AMB-D-753`). */}
      {hostsTerminal && (
        <div className="shell__terminal" hidden={face !== "terminal"}>
          <TerminalFace
            onSplitOut={splitOutTerminal}
            note={windowError}
            onWaiting={noteWaiting}
            projectId={nav.type === "project" ? Number(nav.id) : (dataAdapter.listProjects()[0]?.id ?? null)}
            onOpenLedger={() => setFace("tasks")}
            openIn={openIn}
          />
        </div>
      )}
      <div
        className={`shell__body ${showRight ? "" : "shell__body--no-right"}${sidebarCollapsed ? " shell__body--sidebar-collapsed" : ""}`}
        hidden={face === "terminal"}
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
              onStartTerminal={startTerminalIn}
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
              onStartTerminal={startTerminalIn}
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
          Amenbo has been used, which is exactly what someone still being asked the first question has not
          done yet. `hooksAsked` is that turn being over, the same latch the setup banner waits on. */}
      {hooksAsked && <NudgeHost />}
    </div>
    </RefNavProvider>
  );
}


function PaneHeader({ onClose }: { onClose: () => void }) {
  return (
    <div style={{ display: "flex", justifyContent: "flex-end", padding: "4px 8px", borderBottom: "1px solid var(--c-border)" }}>
      <button className="feed__action" onClick={onClose}><Icon name="close" /> {t("pane.close")}</button>
    </div>
  );
}
