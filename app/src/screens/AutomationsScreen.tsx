// The automations — one screen with four tabs in it, standing at two entrances (`AMB-D-954`): a
// project's toolbar, where it is that project's, and the sidebar's smart views, where it is every
// project's.
//
// **One screen, not four entrances.** What a reader does here moves between them: a definition they
// have just built is the one they want to watch run, and the action a step carries is edited in the
// library and takes effect in every automation using it. Four sidebar entries would make four places
// out of one subject, and each of them would have to carry the way back to the others.
//
// The tabs run from making to running, left to right: "automations" — this one — "actions", the
// library (`./AutomationActionsTab`), "running" (`./RunningTab`) and "history" (`./HistoryTab`).
//
// **Every tab is the entrance's** (`AMB-D-954`): opened from a project, the runs on "running" and
// "history" are that project's; opened from the sidebar, they are every project's, each row naming
// its project.
//
// **From the sidebar, what can be changed is only what has no project to be changed in.** An
// automation is its project's, so the list there names the project on each row, starts one from the
// row, and a press on the row goes to that project's own build screen rather than opening one here —
// and nothing is made there, since making one would first ask which project it is for. The actions
// are the device's library alone, made and changed there (`./AutomationActionsTab`). The two run
// tabs say over their rows that they are this device's whole, because the sidebar names no project
// to have narrowed them to.
//
// **A definition opens into the build screen.** It is not a pane beside the list: what is being
// looked at is one automation's whole picture, and a list kept beside it would take the width the
// picture needs (`./AutomationBuildScreen`).
//
// **A library action opens the same way**, into the screen its steps are drawn in
// (`./AutomationActionBuildScreen`). It is the same move one layer down (`AMB-D-949`), so the list
// it replaces is the "actions" tab's rather than the "automations" tab's. The automation build
// screen opens one as well — the action it has just made on the spot, to be built (`AMB-D-956`) —
// and "back" from there lands on that build screen again, which is still open underneath.
//
// **A row leads with the automation's ID**, the number the terminal names it by
// (`amenbo automation start <ID>`): a name can be changed, so the ID is what ties a row on this
// screen to a line typed there.
//
// **A row says whether its automation could be started now**, beside how many actions are placed on
// it: "what is this" and "is it built yet" are the two things the list is read for. It is the build
// screen's own launch check, so the list and the launch place cannot disagree (`AMB-T-5272`).
//
// **A new one is made from the list**, which is where this side of the app makes one at all. The
// press takes a name and nothing else, and lands in the build screen on what it just made — what an
// automation is for is the picture, and a form asking for its notes first would be asked before
// there is anything to write them about (`AutomationNew`).
import { useState } from "react";
import { AutomationActionBuildScreen } from "./AutomationActionBuildScreen";
import { AutomationActionsTab } from "./AutomationActionsTab";
import { AutomationBuildScreen } from "./AutomationBuildScreen";
import { HistoryTab } from "./HistoryTab";
import { RunningTab } from "./RunningTab";
import { useAutomationStart } from "../components/StartAutomation";
import { addAutomation, useAutomations, useEveryAutomation, useLaunchCheck } from "../core/automations";
import { useBoundFolders } from "../core/boundFolders";
import { asTyped, isEnterSubmit } from "../core/keys";
import { t, tf } from "../core/i18n";
import type { AutomationCardDto, EveryAutomationCardDto } from "../bindings/bindings";

/** Which of the four tabs the screen is on. */
type Tab = "automations" | "actions" | "running" | "history";

// Spelled out rather than built from the id, so the key gate can see every label a reader can be
// shown (`core/i18n/sourceKeys.test.ts`).
const TABS: readonly { id: Tab; label: () => string }[] = [
  { id: "automations", label: () => t("auto.tab.automations") },
  { id: "actions", label: () => t("auto.tab.actions") },
  { id: "running", label: () => t("auto.tab.running") },
  { id: "history", label: () => t("auto.tab.history") },
];

export function AutomationsScreen({
  projectId,
  opening,
  openingAction,
  workspaceOpen,
  onGoToRun,
  onGoToAutomation,
  onGoToGlobalAction,
}: {
  /** The project whose screen this is, or `null` for the sidebar's, which is every project's. */
  projectId: number | null;
  /** The definition to arrive already open on — a press on the sidebar's list that came here. */
  opening?: number;
  /** The library action to arrive already open on, on the "actions" tab — a global action a project
   *  sent here to be changed. */
  openingAction?: number;
  /** Whether the workspace is standing — the build screen's to hand to the press (`./AutomationBuildScreen`). */
  workspaceOpen: boolean;
  /** Go to the pane a run is drawn in, for a press on a row of the "running" tab. */
  onGoToRun?: (project: number, run: number) => void;
  /** Go to one automation's build screen in its project — a press on a row of the sidebar's list. */
  onGoToAutomation?: (project: number, automation: number) => void;
  /** Go to a global action on the sidebar's entrance, where it is changed — from a project's screen. */
  onGoToGlobalAction?: (action: number) => void;
}) {
  const everywhere = projectId === null;
  const [tab, setTab] = useState<Tab>(openingAction === undefined ? "automations" : "actions");
  // Which definition is open, or nothing while the list is. The build screen replaces the list
  // rather than standing beside it, so this is where the screen is and not a selection within it.
  const [open, setOpen] = useState<number | null>(opening ?? null);
  // Which library action is open, for the same reason and in the same spot: the build screen stands
  // in place of the list rather than beside it.
  const [openAction, setOpenAction] = useState<number | null>(openingAction ?? null);
  const automations = useAutomations(projectId);
  const folders = useBoundFolders(projectId).live.map((one) => one.path);

  if (openAction !== null) {
    return (
      <AutomationActionBuildScreen
        id={openAction}
        projectId={projectId}
        onBack={() => setOpenAction(null)}
        onGoToGlobal={onGoToGlobalAction}
        onGoToRun={onGoToRun}
      />
    );
  }

  if (open !== null) {
    return (
      <AutomationBuildScreen
        id={open}
        projectId={projectId}
        workspaceOpen={workspaceOpen}
        onBack={() => setOpen(null)}
        onOpenAction={setOpenAction}
        onGoToRun={onGoToRun}
      />
    );
  }

  return (
    <div className="settings">
      <div className="settings__section">
        <div className="settings__body">
          <div className="autotabs" role="tablist" aria-label={t("auto.title")}>
            {TABS.map((one) => (
              <button
                key={one.id}
                type="button"
                role="tab"
                aria-selected={tab === one.id}
                className={`autotabs__tab ${tab === one.id ? "autotabs__tab--on" : ""}`}
                onClick={() => setTab(one.id)}
              >
                {one.label()}
              </button>
            ))}
          </div>

          {everywhere && (tab === "running" || tab === "history") && (
            <div className="autotabs__head">
              <span className="actlib__sec">{tab === "running" ? t("auto.tab.running") : t("auto.tab.history")}</span>
              <span className="autotabs__scope">{t("auto.scope.device")}</span>
            </div>
          )}

          {tab === "running" && <RunningTab projectId={projectId} onGoToRun={onGoToRun} />}

          {tab === "history" && <HistoryTab projectId={projectId} />}

          {tab === "actions" && (
            <AutomationActionsTab projectId={projectId} onOpen={setOpenAction} />
          )}

          {tab === "automations" && everywhere && (
            <EveryAutomationList workspaceOpen={workspaceOpen} onGoTo={onGoToAutomation} />
          )}

          {tab === "automations" && !everywhere && <AutomationNew projectId={projectId} onMade={setOpen} />}

          {tab === "automations" && !everywhere && automations.length === 0 && (
            <div className="auto__empty">{t("auto.empty")}</div>
          )}

          {tab === "automations" && !everywhere && automations.length > 0 && (
            <ul className="autolist">
              {automations.map((one) => (
                <li key={one.id}>
                  <AutomationRow
                    automation={one}
                    projectId={projectId}
                    folders={folders}
                    onOpen={() => setOpen(one.id)}
                  />
                </li>
              ))}
            </ul>
          )}
        </div>
      </div>
    </div>
  );
}

/** The sidebar's list: every project's definitions, each with its project. */
function EveryAutomationList({
  workspaceOpen,
  onGoTo,
}: {
  workspaceOpen: boolean;
  onGoTo?: (project: number, automation: number) => void;
}) {
  const automations = useEveryAutomation();
  if (automations.length === 0) return <div className="auto__empty">{t("auto.emptyEverywhere")}</div>;
  return (
    <ul className="autolist">
      {automations.map((one) => (
        <li key={one.card.id}>
          <EveryAutomationRow
            row={one}
            workspaceOpen={workspaceOpen}
            onGoTo={onGoTo && (() => onGoTo(one.projectId, one.card.id))}
          />
        </li>
      ))}
    </ul>
  );
}

/**
 * One definition on the sidebar's list — the row goes to its project's build screen, and the start
 * press stands beside it.
 *
 * The press is not inside the row because a press inside a press is not one: the row would swallow
 * "start" and go to the project instead (the "running" tab's rows stand the same way). Where the
 * launch is refused, core's sentence goes under the row it was pressed on, as the build screen puts it
 * under its own press (`../components/StartAutomation`).
 */
function EveryAutomationRow({
  row,
  workspaceOpen,
  onGoTo,
}: {
  row: EveryAutomationCardDto;
  workspaceOpen: boolean;
  onGoTo?: () => void;
}) {
  const { card, projectId } = row;
  const folders = useBoundFolders(projectId).live.map((one) => one.path);
  const check = useLaunchCheck(card.id, projectId, folders);
  const { start, refused, starting } = useAutomationStart(projectId, workspaceOpen);
  return (
    <>
      <div className="autolist__line">
        <button
          type="button"
          className={card.archived ? "autolist__row autolist__row--everywhere autolist__row--archived" : "autolist__row autolist__row--everywhere"}
          disabled={!onGoTo}
          onClick={onGoTo}
        >
          <span className="autolist__name">
            <span className="autoid">{tf("auto.id", { id: card.id })}</span>
            {card.name}
            {card.archived && <span className="autolist__tag">{t("auto.archived")}</span>}
          </span>
          <span className="autolist__project">{row.projectName}</span>
          <span className="autolist__meta">{tf("auto.stepCount", { count: card.placements })}</span>
          <span
            className={
              check === null ? "autolist__ready" : `autolist__ready autolist__ready--${check.ready ? "ok" : "no"}`
            }
          >
            {check === null ? "" : check.ready ? t("auto.ready") : t("auto.notReady")}
          </span>
        </button>
        <button
          type="button"
          className="btn btn--primary"
          disabled={check?.ready !== true || starting}
          onClick={() => void start(card.id, folders)}
        >
          {t("auto.start")}
        </button>
      </div>
      {refused !== null && <div className="autolist__refused">{refused}</div>}
    </>
  );
}

/** One definition on the list — the whole row is the press that opens it. */
function AutomationRow({
  automation,
  projectId,
  folders,
  onOpen,
}: {
  automation: AutomationCardDto;
  projectId: number | null;
  folders: readonly string[];
  onOpen: () => void;
}) {
  const check = useLaunchCheck(automation.id, projectId, folders);
  return (
    <button
      type="button"
      className={automation.archived ? "autolist__row autolist__row--archived" : "autolist__row"}
      onClick={onOpen}
    >
      <span className="autolist__name">
        <span className="autoid">{tf("auto.id", { id: automation.id })}</span>
        {automation.name}
        {automation.archived && <span className="autolist__tag">{t("auto.archived")}</span>}
      </span>
      <span className="autolist__meta">{tf("auto.stepCount", { count: automation.placements })}</span>
      {/* Nothing until the check answers: a guess either way would be a word the build screen may
          then contradict. */}
      <span
        className={
          check === null ? "autolist__ready" : `autolist__ready autolist__ready--${check.ready ? "ok" : "no"}`
        }
      >
        {check === null ? "" : check.ready ? t("auto.ready") : t("auto.notReady")}
      </span>
    </button>
  );
}

/**
 * **Make an automation from the list** — a name, and then the build screen it was made for.
 *
 * It is drawn above the list and above the empty text alike, because the press a reader with no
 * automations needs is the same one, and a screen that offered it only when the list was empty would
 * take it away as soon as it had been used once.
 *
 * The name is held here while it is being typed and the box closes on the press, so nothing is left
 * open behind the build screen that arrives. A blank name is refused by not making anything: core
 * refuses it too, and a message about it would be a sentence in place of a button that simply does
 * not fire.
 *
 * With no project there is nowhere to make one, so there is no press — the screen is the sidebar's,
 * which makes nothing (`AMB-D-954`).
 */
function AutomationNew({
  projectId,
  onMade,
}: {
  projectId: number | null;
  /** Open the build screen on what was just made. */
  onMade: (id: number) => void;
}) {
  const [open, setOpen] = useState(false);
  const [name, setName] = useState("");
  // The press is held down while the write is under way, so a second Enter does not make a second
  // automation out of one name.
  const [making, setMaking] = useState(false);

  if (projectId === null) return null;

  async function make() {
    if (projectId === null || making || name.trim() === "") return;
    setMaking(true);
    try {
      const id = await addAutomation(projectId, name.trim());
      setOpen(false);
      setName("");
      if (id !== null) onMade(id);
    } finally {
      setMaking(false);
    }
  }

  if (!open) {
    return (
      <div className="autolist__head">
        <button type="button" className="btn btn--primary" onClick={() => setOpen(true)}>
          {t("auto.new")}
        </button>
      </div>
    );
  }

  return (
    <div className="autolist__head">
      <div className="autolist__new">
        <label className="autolist__newname">
          <span className="actlib__sec">{t("auto.new.name")}</span>
          <input
            {...asTyped}
            autoFocus
            value={name}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => { if (isEnterSubmit(e)) void make(); }}
          />
        </label>
        <button
          type="button"
          className="btn btn--primary"
          disabled={making || name.trim() === ""}
          onClick={() => void make()}
        >
          {t("auto.new.make")}
        </button>
        <button
          type="button"
          className="btn"
          onClick={() => {
            setOpen(false);
            setName("");
          }}
        >
          {t("auto.new.cancel")}
        </button>
      </div>
    </div>
  );
}
