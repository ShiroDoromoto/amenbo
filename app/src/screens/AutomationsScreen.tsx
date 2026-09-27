// The automations — one screen with four tabs in it, standing at one entrance: the sidebar's
// (`AMB-D-992`). A project has no screen of its own; this one is narrowed to it by the pulldown over
// the tabs, which picks "every project" or one of them. A project's header opens it with that project
// already picked.
//
// **One screen, not four entrances.** What a reader does here moves between them: a definition they
// have just built is the one they want to watch run, and the action a step carries is edited in the
// library and takes effect in every automation using it. Four sidebar entries would make four places
// out of one subject, and each of them would have to carry the way back to the others.
//
// The tabs run from making to running, left to right: "automations" — this one — "actions", the
// library (`./AutomationActionsTab`), "running" (`./RunningTab`) and "history" (`./HistoryTab`).
//
// **Every tab is the pulldown's**: with a project picked, the runs on "running" and "history" are
// that project's and the library is what it reaches; with every project, they are every project's,
// each row naming its project.
//
// **What can be changed is read off each row's owner, not off what is picked** (`AMB-D-992`). An
// automation opens here into its own project's build screen whichever the pulldown says, and a new one
// asks which project it is for when none is picked. An action is changed wherever it is listed
// (`./AutomationActionsTab`).
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
// **A row starts its automation**, and whether it can be started is said by that
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
import { dataAdapter } from "../mock/adapter";
import { asTyped, isEnterSubmit } from "../core/keys";
import { errSentence, t, tf } from "../core/i18n";
import type { AutomationCardDto } from "../bindings/bindings";
import type { RunsTab } from "../core/refNav";

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
  pick,
  opening,
  openingBox,
  openingTab,
  workspaceOpen,
  onGoToRun,
}: {
  /** The project the pulldown arrives on, or nothing for every project — a project's header, and a
   *  run's pane, name theirs. */
  pick?: number;
  /** The definition to arrive already open on, in the project picked — from a run's pane. */
  opening?: number;
  /** The box that definition arrives with pressed on its picture (`./AutomationBuildScreen`). */
  openingBox?: number;
  /** The tab to arrive on — the one a run is listed on, for a run's pane once its run is over
   *  (`AMB-T-5539`): "history", or "running" for a failure nobody has acknowledged yet. */
  openingTab?: RunsTab;
  /** Whether the workspace is standing — the build screen's to hand to the press (`./AutomationBuildScreen`). */
  workspaceOpen: boolean;
  /** Go to the pane a run is drawn in, for a press on a row of the "running" tab. */
  onGoToRun?: (project: number, run: number) => void;
}) {
  // The project the pulldown is on, or `null` for every project.
  const [projectId, setProjectId] = useState<number | null>(pick ?? null);
  const [tab, setTab] = useState<Tab>(openingTab ?? "automations");
  // Which definition is open, and whose project it is, or nothing while the list is. The build screen
  // replaces the list rather than standing beside it, so this is where the screen is and not a
  // selection within it. The project is the row's own, since with every project picked the pulldown
  // does not say it.
  const [open, setOpen] = useState<Opened | null>(
    opening === undefined || pick === undefined ? null : { id: opening, projectId: pick },
  );
  // Which library action is open, for the same reason and in the same spot: the build screen stands
  // in place of the list rather than beside it.
  const [openAction, setOpenAction] = useState<number | null>(null);
  // Which built-in is open to be read, by its key — in the same spot again.
  const [openBuiltin, setOpenBuiltin] = useState<string | null>(null);

  // The tab the reader sees lit: the open screen's own while one is open, the chosen one otherwise.
  const lit: Tab =
    openAction !== null || openBuiltin !== null ? "actions" : open !== null ? "automations" : tab;

  function close() {
    setOpenBuiltin(null);
    setOpenAction(null);
    setOpen(null);
  }

  function choose(next: Tab) {
    close();
    setTab(next);
  }

  // Another project picked closes what is open and stays on the tab: what was open is one project's,
  // and the list it lands on is the one the pulldown now says.
  function narrow(next: number | null) {
    close();
    setProjectId(next);
  }

  // An automation an open action is placed on, opened here in place of the action — whichever
  // project it is in.
  const goToAutomation = (project: number, automation: number) => {
    setOpenAction(null);
    setOpen({ id: automation, projectId: project });
  };

  const head = (
    <>
      <ProjectPick value={projectId} onPick={narrow} />
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
    </>
  );

  if (openBuiltin !== null) {
    return (
      <div className="autoscreen autoscreen--build">
        {head}
        <AutomationBuiltinScreen builtinKey={openBuiltin} onBack={() => setOpenBuiltin(null)} />
      </div>
    );
  }

  if (openAction !== null) {
    return (
      <div className="autoscreen autoscreen--build">
        {head}
        <AutomationActionBuildScreen
          id={openAction}
          onBack={() => setOpenAction(null)}
          onGoToRun={onGoToRun}
          onGoToAutomation={goToAutomation}
        />
      </div>
    );
  }

  if (open !== null) {
    return (
      <div className="autoscreen autoscreen--build">
        {head}
        <AutomationBuildScreen
          id={open.id}
          projectId={open.projectId}
          workspaceOpen={workspaceOpen}
          // Only the definition arrived on: a later one opened from the list starts with nothing pressed.
          openingBox={open.id === opening ? openingBox : undefined}
          onBack={() => setOpen(null)}
          onOpenAction={setOpenAction}
          onGoToRun={onGoToRun}
        />
      </div>
    );
  }

  return (
    <div className="autoscreen">
      {head}

      {tab === "running" && <RunningTab projectId={projectId} onGoToRun={onGoToRun} />}

      {tab === "history" && <HistoryTab projectId={projectId} />}

      {tab === "actions" && (
        <AutomationActionsTab
          // A fresh list per project picked: a form half filled in for one is not the other's.
          key={projectId ?? "every"}
          projectId={projectId}
          onOpen={setOpenAction}
          onOpenBuiltin={setOpenBuiltin}
        />
      )}

      {tab === "automations" && projectId === null && (
        <EveryAutomationList workspaceOpen={workspaceOpen} onOpen={setOpen} onGoToRun={onGoToRun} />
      )}

      {tab === "automations" && projectId !== null && (
        <ProjectAutomationList
          key={projectId}
          projectId={projectId}
          workspaceOpen={workspaceOpen}
          onOpen={setOpen}
          onGoToRun={onGoToRun}
        />
      )}
    </div>
  );
}

/** An open definition, with the project it is in. */
type Opened = { id: number; projectId: number };

/**
 * **The pulldown the screen is narrowed by** — every project, or one of them, in the sidebar's order.
 * It stands over the tabs because every tab is narrowed by it.
 */
function ProjectPick({ value, onPick }: { value: number | null; onPick: (project: number | null) => void }) {
  return (
    <label className="autopick">
      <span className="autopick__label">{t("auto.pick.label")}</span>
      <select
        value={value === null ? "" : String(value)}
        onChange={(e) => onPick(e.target.value === "" ? null : Number(e.target.value))}
      >
        <option value="">{t("auto.pick.every")}</option>
        {dataAdapter.listProjects().map((p) => (
          <option key={p.id} value={String(p.id)}>{p.name}</option>
        ))}
      </select>
    </label>
  );
}

/** One project's definitions, and the press that makes one there. */
function ProjectAutomationList({
  projectId,
  workspaceOpen,
  onOpen,
  onGoToRun,
}: {
  projectId: number;
  workspaceOpen: boolean;
  onOpen: (opened: Opened) => void;
  onGoToRun?: (project: number, run: number) => void;
}) {
  const automations = useAutomations(projectId);
  return (
    <>
      <AutomationNew
        projectId={projectId}
        first={automations.length === 0}
        onMade={(id, project) => onOpen({ id, projectId: project })}
      />
      <AutomationRows
        cards={automations.map((card) => ({ card, projectId }))}
        workspaceOpen={workspaceOpen}
        onOpen={(row) => onOpen({ id: row.card.id, projectId })}
        onGoToRun={onGoToRun}
      />
    </>
  );
}

/** Every project's definitions, each with its project, and the press that makes one in any of them. */
function EveryAutomationList({
  workspaceOpen,
  onOpen,
  onGoToRun,
}: {
  workspaceOpen: boolean;
  onOpen: (opened: Opened) => void;
  onGoToRun?: (project: number, run: number) => void;
}) {
  const automations = useEveryAutomation();
  return (
    <>
      <AutomationNew
        projectId={null}
        first={automations.length === 0}
        onMade={(id, project) => onOpen({ id, projectId: project })}
      />
      <AutomationRows
        cards={automations.map((one) => ({ card: one.card, projectId: one.projectId, projectName: one.projectName }))}
        workspaceOpen={workspaceOpen}
        onOpen={(row) => onOpen({ id: row.card.id, projectId: row.projectId })}
        onGoToRun={onGoToRun}
      />
    </>
  );
}

/** One definition as a row: whose it is, and the name the list names that project by with every project picked. */
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
  /** Open the row's automation. */
  onOpen: (row: Row) => void;
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
        onOpen={() => onOpen(row)}
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
  onOpen: () => void;
  onGoToRun?: (project: number, run: number) => void;
}) {
  const { card, projectId, projectName } = row;
  const folders = useBoundFolders(projectId).live.map((one) => one.path);
  const check = useLaunchCheck(card.id, projectId, folders);
  const { start, refused, starting, handing } = useAutomationStart(projectId, workspaceOpen, onGoToRun);
  // What is in the way, read off the press on hover — the build screen's own list, in its words.
  const blocked = check === null || check.ready ? undefined : check.blocks.map((block) => errSentence(block)).join("\n");
  return (
    <>
      <div className="autolist__line">
        <button
          type="button"
          className={projectName === undefined ? "autolist__row" : "autolist__row autolist__row--everywhere"}
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
            onClick={() => start(card.id, card.name, folders)}
          >
            {t("auto.start")}
          </button>
        </span>
      </div>
      {refused !== null && <div className="autolist__refused">{refused}</div>}
      {handing}
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
 * **With every project picked, it asks which project first** (`AMB-D-992`): an automation is its
 * project's, and the pulldown does not say which. The press stays down until one is picked.
 */
function AutomationNew({
  projectId,
  first,
  onMade,
}: {
  /** The project picked, which is where it is made — or `null`, and then the form asks. */
  projectId: number | null;
  /** Whether the list has none yet — then the press is drawn alone, in the middle of the tab. */
  first: boolean;
  /** Open the build screen on what was just made, in the project it was made in. */
  onMade: (id: number, project: number) => void;
}) {
  const [open, setOpen] = useState(false);
  const [name, setName] = useState("");
  // Where it is made, where the pulldown does not say: nothing picked until the reader picks.
  const [into, setInto] = useState("");
  // The press is held down while the write is under way, so a second Enter does not make a second
  // automation out of one name.
  const [making, setMaking] = useState(false);
  const project = projectId ?? (into === "" ? null : Number(into));

  async function make() {
    if (project === null || making || name.trim() === "") return;
    setMaking(true);
    try {
      const id = await addAutomation(project, name.trim());
      setOpen(false);
      setName("");
      setInto("");
      if (id !== null) onMade(id, project);
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
        {projectId === null && (
          <select aria-label={t("auto.new.which")} value={into} onChange={(e) => setInto(e.target.value)}>
            <option value="">{t("auto.new.which")}</option>
            {dataAdapter.listProjects().map((p) => (
              <option key={p.id} value={String(p.id)}>{p.name}</option>
            ))}
          </select>
        )}
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
          disabled={making || name.trim() === "" || project === null}
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
            setInto("");
          }}
        >
          {t("auto.new.cancel")}
        </button>
      </div>
    </div>
  );
}
