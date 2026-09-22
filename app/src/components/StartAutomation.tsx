// **Start one of this project's automations from the empty frame** — the way in that is not the
// build screen (`AMB-T-5260`).
//
// **The press carries nothing.** Which tasks a run works, which folder its steps run in and what
// each step is asked are all the definition's, settled while it was built, so what the reader
// chooses here is which automation and nothing else (`amenbo_core::ops::automation_run`).
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
import { launchAutomation, useAutomations } from "../core/automations";
import { errText, t } from "../core/i18n";

/**
 * The press behind either entrance: launch, and hold what came back.
 *
 * `refused` is core's sentence, cleared by the next press: what a reader is owed is the outcome of
 * the press they just made.
 */
export function useAutomationStart(projectId: number | null, workspaceOpen: boolean) {
  const [refused, setRefused] = useState<string | null>(null);
  // A press already out. The answer carries the pane the run opens in, so a second press before the
  // first lands would be a second run nobody asked for.
  const [starting, setStarting] = useState(false);

  const start = useCallback(async (id: number, folders: readonly string[]) => {
    if (projectId === null) return;
    setRefused(null);
    setStarting(true);
    try {
      await launchAutomation(id, projectId, folders, workspaceOpen);
    } catch (e) {
      setRefused(errText(e));
    } finally {
      setStarting(false);
    }
  }, [projectId, workspaceOpen]);

  return { start, refused, starting };
}

export function StartAutomation({
  projectId,
  folders,
  workspaceOpen,
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
}) {
  const automations = useAutomations(projectId);
  const { start, refused, starting } = useAutomationStart(projectId, workspaceOpen);
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
            onClick={() => void start(one.id, folders)}
          >
            {one.name}
          </button>
        ))}
      </div>
      {refused !== null && <p className="autostart__refused">{refused}</p>}
    </div>
  );
}
