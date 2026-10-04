// **The project's pause** (`AMB-D-1009`) — one ‖ on the header that asks every run of the project
// shown to pause before it takes its next task, so a task is not cut off between two of its actions.
// It asks them one by one, each as its own run (`AMB-D-1019`); a run that has ended in between is
// refused and said, and the rest are still asked.
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
  const askable = runs.filter((one) => one.pausableBeforeNextTask);
  const pausing = runs.filter(isPausing).length;
  const off = pressing || askable.length === 0;
  const said = askable.length > 0
    ? t("face.pauseAfterTask")
    : pausing > 0
      ? tf("face.pauseAfterTaskWaiting", { n: pausing })
      : t("auto.running.empty");
  const press = async () => {
    if (off) return;
    setPressing(true);
    for (const one of askable) {
      try {
        await pauseBeforeNextTask(one.run);
      } catch (e) {
        pushNotice(errText(e));
      }
    }
    setPressing(false);
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
