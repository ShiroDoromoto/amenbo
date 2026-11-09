// **The press that starts one of a project's automations** — the one the automations tab's rows and
// the build screen's head both make (`../screens/AutomationsScreen`, `../screens/AutomationBuildScreen`).
//
// **The press only opens the run, and carries nothing** (`AMB-D-1123`). Which tasks a run works, which
// folder its steps run in and what each step is asked are all the definition's, settled while it was
// built. Work reaches a run as a task filed under one of its roles — by a person on the task screen or
// by a trigger — and each role takes from its own waiting tasks. So there is no dialog asking what to
// hand over: what the reader chooses is which automation, and the press starts it.
//
// **Which is why there is no way in on a task's pane, nor on the workspace's empty frame.** A button
// under a task reads as "run this one on this task", and the task a reader was looking at cannot reach
// the run — the entry step takes its own. On the empty frame the press sat under the choice of agent
// and model, which reads as if that choice reached the run, and the run did not open there but in a
// pane of its own at the project's end (`AMB-D-994`). An entrance whose place says something the
// launch cannot do is worse than one press further away: choosing among the automations is what the
// automations screen is for.
import { useCallback, useState, type ReactNode } from "react";
import { launchAutomation } from "../core/automations";
import { errText, t } from "../core/i18n";
import { useRefNav } from "../core/refNav";

/**
 * The press behind every entrance: launch, and hold what came back.
 *
 * `refused` is core's sentence, cleared by the next press: what a reader is owed is the outcome of
 * the press they just made.
 *
 * **A closed workspace is refused at the press, before anything is sent** (`AMB-T-5590`). Core
 * refuses it too, but in its own sentence alone; the press answers it here in that same sentence
 * (`invalid_automation_workspace_closed`), with the way to open the workspace beside it. Core's check
 * stays, for a workspace that closes while the launch is on its way. The line goes once the
 * workspace opens.
 *
 * **A launch that starts is gone to** (`AMB-T-5530`): `onGoToRun` is handed the run and the launch the
 * press answered with, the same road a row of the "running" tab travels. Left where the press was made,
 * the reader would have to go looking for where it is going on — and a run opens its own pane without
 * moving the screen, because it may be one nobody here asked for (`../shell/WorkspaceFace`'s
 * `stepArrived`). This one somebody did.
 *
 * **Where the start finds no task to take, the run is null and the launch is gone to** — its
 * pane, with the roles and no seat in front. A press that opened nothing on the screen would read as a
 * press that did nothing.
 */
export function useAutomationStart(
  projectId: number | null,
  workspaceOpen: boolean,
  onGoToRun?: (project: number, run: number | null, launch?: number | null) => void,
) {
  const [refused, setRefused] = useState<string | null>(null);
  // A press turned away because the workspace was closed — drawn only while it still is.
  const [closed, setClosed] = useState(false);
  const { openWorkspace } = useRefNav();
  // A press already out. The answer carries the pane the run opens in, so a second press before the
  // first lands would be a second run nobody asked for.
  const [starting, setStarting] = useState(false);

  /** The press: start the run at once. */
  const start = useCallback(async (id: number, folders: readonly string[]) => {
    if (projectId === null) return;
    setRefused(null);
    setClosed(!workspaceOpen);
    if (!workspaceOpen) return;
    setStarting(true);
    try {
      const started = await launchAutomation(id, projectId, folders, workspaceOpen);
      if (started !== null && (started.run !== null || started.launch != null)) {
        onGoToRun?.(projectId, started.run, started.launch);
      }
    } catch (e) {
      setRefused(errText(e));
    } finally {
      setStarting(false);
    }
  }, [projectId, workspaceOpen, onGoToRun]);

  const refusal: ReactNode = closed && !workspaceOpen ? (
    <>
      {errText({ code: WORKSPACE_CLOSED, message_en: "the workspace is closed" })}
      {openWorkspace !== undefined && (
        <button type="button" className="btn autorefused__open" onClick={openWorkspace}>
          {t("auto.launch.openWorkspace")}
        </button>
      )}
    </>
  ) : refused;

  return { start, refused: refusal, starting };
}

// The refusal the press gives on its own is core's, so it is written from the same sentence.
const WORKSPACE_CLOSED = "invalid_automation_workspace_closed";
