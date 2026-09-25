// **Start one of this project's automations from the empty frame** — the way in that is not the
// build screen (`AMB-T-5260`).
//
// **The press carries what the person hands over, and nothing else.** Which tasks a run works, which
// folder its steps run in and what each step is asked are all the definition's, settled while it was
// built. What the reader chooses here is which automation, and then — in the dialog every press
// opens — what to hand the run as it starts: what its entry reads (`./LaunchHanding`, `AMB-D-970`).
//
// **Which is why there is no way in on a task's pane.** One stood there and was taken out: a button
// under a task reads as "run this one on this task", and the task a reader was looking at cannot
// reach the run — the entry step takes its own. An entrance whose place says something the launch
// cannot do is worse than one press further away. If a step is ever handed a task at launch, that is
// when the question is worth asking again.
//
// **It draws nothing where the project has none.** A project with no automations has no entrance to
// offer, and a heading over an empty row would put the subject in front of a reader who has never
// met it, in every pane they open.
//
// **The list is unfiltered on purpose.** Whether an automation could be started right now is the
// launch check's answer and it is a paragraph long (`../screens/AutomationBuildScreen`); asking it
// per row would probe this machine once per automation, to hide rows a reader is looking for. So
// every one is offered, and what refuses is the press — in core's own words, under the row.
import { useCallback, useState } from "react";
import { launchAutomation, useAutomations, type Handed } from "../core/automations";
import { errText, t } from "../core/i18n";
import { LaunchHanding } from "./LaunchHanding";

/**
 * The press behind either entrance: launch, and hold what came back.
 *
 * `refused` is core's sentence, cleared by the next press: what a reader is owed is the outcome of
 * the press they just made.
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
    setAsking({ id, name, folders });
  }, [projectId]);

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

  return { start, refused, starting, handing };
}

export function StartAutomation({
  projectId,
  folders,
  workspaceOpen,
  onGoToRun,
}: {
  /** The project whose automations these are, or null on a surface that has none. */
  projectId: number | null;
  /** The folders that project is bound to — what the machine is asked about (`crate::wake`). */
  folders: readonly string[];
  /**
   * Whether the workspace is standing. A run draws its steps in its panes, so core refuses a launch
   * without one — and which window holds it is not a thing this component can see (`AMB-D-753`).
   */
  workspaceOpen: boolean;
  /** Go to the pane the run a press started is drawn in. */
  onGoToRun?: (project: number, run: number) => void;
}) {
  const automations = useAutomations(projectId);
  const { start, refused, starting, handing } = useAutomationStart(projectId, workspaceOpen, onGoToRun);
  const live = automations.filter((one) => !one.archived);

  if (live.length === 0) return null;

  return (
    <div className="autostart">
      <p className="autostart__ask">{t("auto.startOne")}</p>
      <div className="autostart__rows">
        {live.map((one) => (
          <button
            key={one.id}
            type="button"
            className="btn"
            disabled={starting}
            onClick={() => start(one.id, one.name, folders)}
          >
            {one.name}
          </button>
        ))}
      </div>
      {refused !== null && <p className="autostart__refused">{refused}</p>}
      {handing}
    </div>
  );
}
