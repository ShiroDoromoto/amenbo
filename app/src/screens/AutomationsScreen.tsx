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
// tabs carry no heading over their rows: the tab already says which it is, and each row names its
// project.
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
// **A built-in opens the same way, to be read** (`./AutomationBuiltinScreen`): it is Amenbo's own, so
// what stands in place of the list is its definition rather than a screen to build it in.
//
// **The tabs stay over a build screen** (`AMB-T-5419`), so another tab is one press away rather than
// "back" and then the tab. The tab lit is the one the open screen belongs to — "actions" over an
// action, "automations" over an automation — and a press on any tab, that one included, closes what is
// open and lands on that tab's list, as "back" does.
//
// **A row leads with the automation's ID**, the number the terminal names it by
// (`amenbo automation start <ID>`): a name can be changed, so the ID is what ties a row on this
// screen to a line typed there.
//
// **A row starts its automation**, at either entrance, and whether it can be started is said by that
// press alone: it cannot be pressed while something is in the way, and what is in the way is read off
// it on hover. It is the build screen's own launch check, so the list and the build screen cannot
// disagree (`AMB-T-5272`). A badge beside it would say the same thing a second time (`AMB-T-5523`).
//
// **Archived rows are folded at the end**, counted, so the list is the automations in use and the
// ones put away stay one press off rather than mixed in among them.
//
// **A new one is made from the list**, which is where this side of the app makes one at all. The
// press takes a name and nothing else, and lands in the build screen on what it just made — what an
// automation is for is the picture, and a form asking for its notes first would be asked before
// there is anything to write them about (`AutomationNew`). With none yet, that press is the whole
// list: a sentence saying the list is empty would stand beside the one move it leaves.
import { useState } from "react";
import { AutomationActionBuildScreen } from "./AutomationActionBuildScreen";
import { AutomationActionsTab, firstLine } from "./AutomationActionsTab";
import { AutomationBuiltinScreen } from "./AutomationBuiltinScreen";
import { AutomationBuildScreen } from "./AutomationBuildScreen";
import { HistoryTab } from "./HistoryTab";
import { RunningTab } from "./RunningTab";
import { useAutomationStart } from "../components/StartAutomation";
import { addAutomation, useAutomations, useEveryAutomation, useLaunchCheck } from "../core/automations";
import { Icon } from "../components/Icon";
import { useBoundFolders } from "../core/boundFolders";
import { asTyped, isEnterSubmit } from "../core/keys";
import { errSentence, t, tf } from "../core/i18n";
import type { AutomationCardDto } from "../bindings/bindings";

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
  // Which built-in is open to be read, by its key — in the same spot again.
  const [openBuiltin, setOpenBuiltin] = useState<string | null>(null);
  const automations = useAutomations(projectId);

  // The tab the reader sees lit: the open screen's own while one is open, the chosen one otherwise.
  const lit: Tab =
    openAction !== null || openBuiltin !== null ? "actions" : open !== null ? "automations" : tab;

  function choose(next: Tab) {
    setOpenBuiltin(null);
    setOpenAction(null);
    setOpen(null);
    setTab(next);
  }

  // An automation an open action is placed on: this project's is opened here, in place of the action;
  // the sidebar's screen goes to it where its project draws it. A project's screen offers no other
  // project's (`./AutomationActionBuildScreen`).
  const goToAutomation =
    projectId === null
      ? onGoToAutomation
      : (_project: number, automation: number) => {
          setOpenAction(null);
          setOpen(automation);
        };

  const tabs = (
    <div className="autotabs" role="tablist" aria-label={t("auto.title")}>
      {TABS.map((one) => (
        <button
          key={one.id}
          type="button"
          role="tab"
          aria-selected={lit === one.id}
          className={`autotabs__tab ${lit === one.id ? "autotabs__tab--on" : ""}`}
          onClick={() => choose(one.id)}
        >
          {one.label()}
        </button>
      ))}
    </div>
  );

  if (openBuiltin !== null) {
    return (
      <div className="autoscreen autoscreen--build">
        {tabs}
        <AutomationBuiltinScreen builtinKey={openBuiltin} onBack={() => setOpenBuiltin(null)} />
      </div>
    );
  }

  if (openAction !== null) {
    return (
      <div className="autoscreen autoscreen--build">
        {tabs}
        <AutomationActionBuildScreen
          id={openAction}
          projectId={projectId}
          onBack={() => setOpenAction(null)}
          onGoToGlobal={onGoToGlobalAction}
          onGoToRun={onGoToRun}
          onGoToAutomation={goToAutomation}
        />
      </div>
    );
  }

  if (open !== null) {
    return (
      <div className="autoscreen autoscreen--build">
        {tabs}
        <AutomationBuildScreen
          id={open}
          projectId={projectId}
          workspaceOpen={workspaceOpen}
          onBack={() => setOpen(null)}
          onOpenAction={setOpenAction}
          onGoToRun={onGoToRun}
        />
      </div>
    );
  }

  return (
    <div className="autoscreen">
      {tabs}

      {tab === "running" && <RunningTab projectId={projectId} onGoToRun={onGoToRun} />}

      {tab === "history" && <HistoryTab projectId={projectId} />}

      {tab === "actions" && (
        <AutomationActionsTab projectId={projectId} onOpen={setOpenAction} onOpenBuiltin={setOpenBuiltin} />
      )}

      {tab === "automations" && everywhere && (
        <EveryAutomationList workspaceOpen={workspaceOpen} onGoTo={onGoToAutomation} onGoToRun={onGoToRun} />
      )}

      {tab === "automations" && projectId !== null && (
        <>
          <AutomationNew projectId={projectId} first={automations.length === 0} onMade={setOpen} />
          <AutomationRows
            cards={automations.map((card) => ({ card, projectId }))}
            workspaceOpen={workspaceOpen}
            onOpen={(row) => setOpen(row.card.id)}
            onGoToRun={onGoToRun}
          />
        </>
      )}
    </div>
  );
}

/** The sidebar's list: every project's definitions, each with its project. */
function EveryAutomationList({
  workspaceOpen,
  onGoTo,
  onGoToRun,
}: {
  workspaceOpen: boolean;
  onGoTo?: (project: number, automation: number) => void;
  onGoToRun?: (project: number, run: number) => void;
}) {
  const automations = useEveryAutomation();
  if (automations.length === 0) return <div className="auto__empty">{t("auto.emptyEverywhere")}</div>;
  return (
    <AutomationRows
      cards={automations.map((one) => ({ card: one.card, projectId: one.projectId, projectName: one.projectName }))}
      workspaceOpen={workspaceOpen}
      onOpen={onGoTo && ((row) => onGoTo(row.projectId, row.card.id))}
      onGoToRun={onGoToRun}
    />
  );
}

/** One definition as a row: whose it is, and the name the sidebar's list names that project by. */
type Row = { card: AutomationCardDto; projectId: number; projectName?: string };

/**
 * The rows of either list — the ones in use, then the archived ones folded under a count.
 *
 * The fold is closed on arrival, and stays as it was left while the list is re-read underneath.
 */
function AutomationRows({
  cards,
  workspaceOpen,
  onOpen,
  onGoToRun,
}: {
  cards: readonly Row[];
  workspaceOpen: boolean;
  /** Open the row's automation. Absent where there is nowhere to go, and then the rows are read. */
  onOpen?: (row: Row) => void;
  onGoToRun?: (project: number, run: number) => void;
}) {
  const [unfolded, setUnfolded] = useState(false);
  if (cards.length === 0) return null;
  const live = cards.filter((row) => !row.card.archived);
  const archived = cards.filter((row) => row.card.archived);
  const line = (row: Row) => (
    <li key={row.card.id}>
      <AutomationLine
        row={row}
        workspaceOpen={workspaceOpen}
        onOpen={onOpen && (() => onOpen(row))}
        onGoToRun={onGoToRun}
      />
    </li>
  );
  return (
    <ul className="autolist">
      {live.map(line)}
      {archived.length > 0 && (
        <li>
          <button
            type="button"
            className="autolist__fold"
            aria-expanded={unfolded}
            onClick={() => setUnfolded(!unfolded)}
          >
            <Icon name={unfolded ? "chevronDown" : "chevronRight"} />
            {tf("auto.archivedFold", { count: archived.length })}
          </button>
        </li>
      )}
      {unfolded && archived.map(line)}
    </ul>
  );
}

/**
 * One definition on a list — the row opens it, and the start press stands beside it.
 *
 * The press is not inside the row because a press inside a press is not one: the row would swallow
 * "start" and open the automation instead (the "running" tab's rows stand the same way). Where the
 * launch is refused, core's sentence goes under the row it was pressed on, as the build screen puts it
 * under its own press (`../components/StartAutomation`).
 */
function AutomationLine({
  row,
  workspaceOpen,
  onOpen,
  onGoToRun,
}: {
  row: Row;
  workspaceOpen: boolean;
  onOpen?: () => void;
  onGoToRun?: (project: number, run: number) => void;
}) {
  const { card, projectId, projectName } = row;
  const folders = useBoundFolders(projectId).live.map((one) => one.path);
  const check = useLaunchCheck(card.id, projectId, folders);
  const { start, refused, starting } = useAutomationStart(projectId, workspaceOpen, onGoToRun);
  // What is in the way, read off the press on hover — the build screen's own list, in its words.
  const blocked = check === null || check.ready ? undefined : check.blocks.map((block) => errSentence(block)).join("\n");
  return (
    <>
      <div className="autolist__line">
        <button
          type="button"
          className={projectName === undefined ? "autolist__row" : "autolist__row autolist__row--everywhere"}
          disabled={!onOpen}
          onClick={onOpen}
        >
          <span className="autolist__name">
            <span className="autoid">{tf("auto.id", { id: card.id })}</span>
            {card.name}
            {firstLine(card.notes) !== "" && <span className="auto__note">{firstLine(card.notes)}</span>}
          </span>
          {projectName !== undefined && <span className="autolist__project">{projectName}</span>}
          <span className="autolist__meta">{tf("auto.stepCount", { count: card.placements })}</span>
        </button>
        {/* Held down until the check answers: a press offered before it would be a guess. */}
        <span className="autolist__start" title={blocked}>
          <button
            type="button"
            className="btn btn--primary"
            disabled={check?.ready !== true || starting}
            onClick={() => void start(card.id, folders)}
          >
            {t("auto.start")}
          </button>
        </span>
      </div>
      {refused !== null && <div className="autolist__refused">{refused}</div>}
    </>
  );
}

/**
 * **Make an automation from the list** — a name, and then the build screen it was made for.
 *
 * It is drawn above the list, and in place of the list while there is none: the press a reader with
 * no automations needs is the same one, drawn where there is nothing else to look at.
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
  first,
  onMade,
}: {
  projectId: number | null;
  /** Whether the project has none yet — then the press is drawn alone, in the middle of the tab. */
  first: boolean;
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

  if (!open && first) {
    return (
      <div className="autolist__first">
        <button type="button" className="btn btn--primary" onClick={() => setOpen(true)}>
          <Icon name="plus" /> {t("auto.newFirst")}
        </button>
      </div>
    );
  }

  if (!open) {
    return (
      <div className="autolist__head">
        <button type="button" className="btn" onClick={() => setOpen(true)}>
          <Icon name="plus" /> {t("auto.new")}
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
