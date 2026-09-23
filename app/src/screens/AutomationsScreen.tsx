// The automations of one project — one screen with three tabs in it.
//
// **One screen, not three entrances.** What a reader does here moves between the three: a definition
// they have just built is the one they want to watch run, and the action a step carries is edited in
// the library and takes effect in every automation using it. Three sidebar entries would make three
// places out of one subject, and each of them would have to carry the way back to the other two.
//
// The tabs are "running" (`./RunningTab`), "automations" — this one — and "actions", the library
// (`./AutomationActionsTab`).
//
// **"Running" crosses projects and the other two do not.** What is under way is a claim on this
// machine's lanes, and the lanes are not divided up per project; a definition and a library action
// belong to the project they were built in. So the tab takes no `projectId` and names the project on
// each row instead (`./RunningTab`).
//
// **A definition opens into the build screen.** It is not a pane beside the list: what is being
// looked at is one automation's whole picture, and a list kept beside it would take the width the
// picture needs (`./AutomationBuildScreen`).
//
// **A library action opens the same way**, into the screen its steps are drawn in
// (`./AutomationActionBuildScreen`). It is the same move one layer down (`AMB-D-949`), so the list
// it replaces is the "actions" tab's rather than the "automations" tab's.
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
import { RunningTab } from "./RunningTab";
import { addAutomation, useAutomations, useLaunchCheck } from "../core/automations";
import { useBoundFolders } from "../core/boundFolders";
import { asTyped, isEnterSubmit } from "../core/keys";
import { t, tf } from "../core/i18n";
import type { AutomationCardDto } from "../bindings/bindings";

/** Which of the three tabs the screen is on. */
type Tab = "running" | "automations" | "actions";

// Spelled out rather than built from the id, so the key gate can see every label a reader can be
// shown (`core/i18n/sourceKeys.test.ts`).
const TABS: readonly { id: Tab; label: () => string }[] = [
  { id: "running", label: () => t("auto.tab.running") },
  { id: "automations", label: () => t("auto.tab.automations") },
  { id: "actions", label: () => t("auto.tab.actions") },
];

export function AutomationsScreen({
  projectId,
  workspaceOpen,
  onGoToRun,
}: {
  projectId: number | null;
  /** Whether the workspace is standing — the build screen's to hand to the press (`./AutomationBuildScreen`). */
  workspaceOpen: boolean;
  /** Go to the pane a run is drawn in, for a press on a row of the "running" tab. */
  onGoToRun?: (project: number, run: number) => void;
}) {
  const [tab, setTab] = useState<Tab>("automations");
  // Which definition is open, or nothing while the list is. The build screen replaces the list
  // rather than standing beside it, so this is where the screen is and not a selection within it.
  const [open, setOpen] = useState<number | null>(null);
  // Which library action is open, for the same reason and in the same spot: the build screen stands
  // in place of the list rather than beside it.
  const [openAction, setOpenAction] = useState<number | null>(null);
  const automations = useAutomations(projectId);
  const folders = useBoundFolders(projectId).live.map((one) => one.path);

  if (openAction !== null) {
    return (
      <AutomationActionBuildScreen
        id={openAction}
        projectId={projectId}
        onBack={() => setOpenAction(null)}
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

          {tab === "running" && <RunningTab onGoToRun={onGoToRun} />}

          {tab === "actions" && (
            <AutomationActionsTab projectId={projectId} onOpen={setOpenAction} />
          )}

          {tab === "automations" && <AutomationNew projectId={projectId} onMade={setOpen} />}

          {tab === "automations" && automations.length === 0 && (
            <div className="auto__empty">{t("auto.empty")}</div>
          )}

          {tab === "automations" && automations.length > 0 && (
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
 * With no project there is nowhere to make one, so there is no press — the screen is standing on a
 * project that has not been chosen.
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
