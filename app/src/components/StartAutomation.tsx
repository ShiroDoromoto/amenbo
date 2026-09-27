// **The press that starts one of a project's automations** — the one the automations tab's rows and
// the build screen's head both make (`../screens/AutomationsScreen`, `../screens/AutomationBuildScreen`).
//
// **The press carries what the person hands over, and nothing else.** Which tasks a run works, which
// folder its steps run in and what each step is asked are all the definition's, settled while it was
// built. What the reader chooses is which automation, and then — in the dialog every press opens —
// what to hand the run as it starts: what its entry reads (`./LaunchHanding`, `AMB-D-970`).
//
// **Which is why there is no way in on a task's pane, nor on the workspace's empty frame.** A button
// under a task reads as "run this one on this task", and the task a reader was looking at cannot reach
// the run — the entry step takes its own. On the empty frame the press sat under the choice of agent
// and model, which reads as if that choice reached the run, and the run did not open there but in a
// pane of its own at the project's end (`AMB-D-994`). An entrance whose place says something the
// launch cannot do is worse than one press further away: choosing among the automations is what the
// automations screen is for.
import { useCallback, useState, type ReactNode } from "react";
import { launchAutomation, type Handed } from "../core/automations";
import { errText, t } from "../core/i18n";
import { useRefNav } from "../core/refNav";
import { LaunchHanding } from "./LaunchHanding";

/**
 * The press behind every entrance: launch, and hold what came back.
 *
 * `refused` is core's sentence, cleared by the next press: what a reader is owed is the outcome of
 * the press they just made.
 *
 * **A closed workspace is refused at the press, before the dialog asks anything** (`AMB-T-5590`).
 * Core refuses it too, but only once the launch is sent — after the reader has written what to hand
 * over, which the refusal then throws away. So the press answers it here in core's own sentence
 * (`invalid_automation_workspace_closed`), with the way to open the workspace beside it; core's check
 * stays, for a workspace that closes while the dialog is up. The line goes once the workspace opens.
 *
 * **A run that starts is gone to** (`AMB-T-5530`): `onGoToRun` is handed the run the launch answered
 * with, the same road a row of the "running" tab travels. Left where the press was made, the reader
 * would have to go looking for where it is going on — and a run opens its own pane without moving the
 * screen, because it may be one nobody here asked for (`../shell/WorkspaceFace`'s `stepArrived`).
 * This one somebody did.
 */
export function useAutomationStart(
  projectId: number | null,
  workspaceOpen: boolean,
  onGoToRun?: (project: number, run: number) => void,
) {
  const [refused, setRefused] = useState<string | null>(null);
  // A press turned away because the workspace was closed — drawn only while it still is.
  const [closed, setClosed] = useState(false);
  const { openWorkspace } = useRefNav();
  // A press already out. The answer carries the pane the run opens in, so a second press before the
  // first lands would be a second run nobody asked for.
  const [starting, setStarting] = useState(false);
  // The automation whose press is asking what to hand over, while that dialog is open (`AMB-D-970`).
  const [asking, setAsking] = useState<{ id: number; name: string; folders: readonly string[] } | null>(null);

  const launch = useCallback(async (id: number, folders: readonly string[], handed: Handed) => {
    if (projectId === null) return;
    setRefused(null);
    setStarting(true);
    try {
      const started = await launchAutomation(id, projectId, folders, workspaceOpen, handed);
      if (started !== null) onGoToRun?.(projectId, started.run);
    } catch (e) {
      setRefused(errText(e));
    } finally {
      setStarting(false);
    }
  }, [projectId, workspaceOpen, onGoToRun]);

  /** The press: ask what to hand over first, and start once that is answered. */
  const start = useCallback((id: number, name: string, folders: readonly string[]) => {
    if (projectId === null) return;
    setRefused(null);
    setClosed(!workspaceOpen);
    if (!workspaceOpen) return;
    setAsking({ id, name, folders });
  }, [projectId, workspaceOpen]);

  // What the screen draws for the press, wherever the press is: nothing until one is made.
  const handing = asking === null ? null : (
    <LaunchHanding
      id={asking.id}
      name={asking.name}
      onClose={() => setAsking(null)}
      onStart={(handed) => {
        setAsking(null);
        void launch(asking.id, asking.folders, handed);
      }}
    />
  );

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

  return { start, refused: refusal, starting, handing };
}

// The refusal the press gives on its own is core's, so it is written from the same sentence.
const WORKSPACE_CLOSED = "invalid_automation_workspace_closed";
