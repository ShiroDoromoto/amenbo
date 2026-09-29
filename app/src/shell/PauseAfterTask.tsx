// **The project's pause** (`AMB-D-1009`) — one ‖ on the header that asks every run of the project
// shown to pause before it takes its next task, so a task is not cut off between two of its actions.
//
// It carries no label: the mark is the pane's pause, and what it would do, or why it will not, is said
// on hover. It is pressable only while there is a run it would ask, so there is nothing to confirm
// and nothing to take back — a press once every run is asked would ask nobody.
import { useState } from "react";
import { pauseBeforeNextTask, useLiveRuns } from "../core/automations";
import { errText, t, tf } from "../core/i18n";
import { pushNotice } from "../core/notice";
import { isPausing } from "../core/runWords";
import { Icon } from "../components/Icon";

export function PauseAfterTask({ projectId }: { projectId: number }) {
  const [pressing, setPressing] = useState(false);
  const runs = useLiveRuns().filter((one) => one.project === projectId);
  const askable = runs.filter((one) => one.pausableBeforeNextTask).length;
  const pausing = runs.filter(isPausing).length;
  const off = pressing || askable === 0;
  const said = askable > 0
    ? t("face.pauseAfterTask")
    : pausing > 0
      ? tf("face.pauseAfterTaskWaiting", { n: pausing })
      : t("auto.running.empty");
  const press = async () => {
    if (off) return;
    setPressing(true);
    try {
      await pauseBeforeNextTask(projectId);
    } catch (e) {
      pushNotice(errText(e));
    } finally {
      setPressing(false);
    }
  };
  return (
    <button
      type="button"
      className="workspace__action workspace__pause"
      // Not `disabled`: a disabled button takes no hover, and the hover is where it says why.
      aria-disabled={off}
      aria-label={said}
      title={said}
      onClick={() => void press()}
    >
      <Icon name="pause" />
    </button>
  );
}
