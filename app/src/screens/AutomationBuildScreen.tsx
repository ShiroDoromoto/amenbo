// One automation, opened — the screen it is built and started from.
//
// **Three places, each named on the screen**: "launch", which is this task's; "build", the picture of
// the steps (`AMB-T-5255`); and "step", the panel showing what the pressed step holds
// (`AMB-T-5256`). The two that are not built yet draw their heading and nothing under it, so the
// screen already reads as the three places it is rather than as one that will grow legs later.
//
// **The launch place refuses, the build place never does.** Building is always half-finished — a step
// with no way onward, an input nobody has wired — and every one of those saves
// (`amenbo_core::ops::automation`). What is unfinished only matters at the moment somebody is about
// to be let down by it, which is here: the reasons are listed, and the button is not offered while
// there is one.
//
// **The list of reasons is read, not worked out here** (`../core/automations`). A screen that made
// its own judgement would come to disagree with the refusal a launch actually gives, and the reader
// would meet both.
import { useAutomation, useLaunchCheck } from "../core/automations";
import { useBoundFolders } from "../core/boundFolders";
import { t, tf } from "../core/i18n";
import { Icon } from "../components/Icon";
import type { AutomationLaunchBlockDto } from "../bindings/bindings";

/**
 * One reason, in words. The unnamed way out has no name to put in the sentence — it is the one a
 * step with a single way out has — so it takes a line of its own rather than a sentence with a hole
 * where the name goes.
 */
function blockText(block: AutomationLaunchBlockDto): string {
  const step = block.stepName ?? "";
  const at = block.at ?? "";
  switch (block.reason) {
    case "no_steps":
      return t("auto.block.noSteps");
    case "exit_without_next":
      return block.at === undefined
        ? tf("auto.block.exitWithoutNextUnnamed", { step })
        : tf("auto.block.exitWithoutNext", { step, at });
    case "input_unfed":
      return tf("auto.block.inputUnfed", { step, at });
    case "agent_not_here":
      return tf("auto.block.agentNotHere", { step, at });
    case "task_undecided":
      return t("auto.block.taskUndecided");
    case "workspace_closed":
      return t("auto.block.workspaceClosed");
    default:
      return block.reason;
  }
}

/**
 * Whether the app has a workspace to open a run's panes in.
 *
 * From here it is always yes, and that is a fact about the shell rather than an assumption: in one
 * window the workspace is a face of this one, and split out it is a window whose closing folds the
 * app back into one (`../shell/AppShell`). The check still asks, because the same rule is read by a
 * launch made where no window is open at all — the CLI's — and a rule that only one caller could
 * answer would be written twice.
 */
const WORKSPACE_OPEN = true;

export function AutomationBuildScreen({
  id, projectId, onBack, onStart,
}: {
  id: number;
  /** Whose project this automation is — what the machine is asked about, and where its folders are. */
  projectId: number | null;
  onBack: () => void;
  /**
   * Start this automation. Absent while the road that writes a run is still being laid
   * (`AMB-T-5244`), and the button is held shut until it is there — a press with nothing behind it
   * is worse than a button that says it cannot be pressed yet.
   */
  onStart?: (id: number) => void;
}) {
  const automation = useAutomation(id);
  const folders = useBoundFolders(projectId);
  const check = useLaunchCheck(
    id,
    projectId,
    folders.live.map((one) => one.path),
    WORKSPACE_OPEN,
  );

  return (
    <div className="settings">
      <div className="settings__section">
        <div className="settings__body">
          <div className="auto__head">
            <button type="button" className="btn" onClick={onBack}>
              <Icon name="chevronLeft" /> {t("auto.build.back")}
            </button>
            <span className="auto__name">{automation?.name ?? ""}</span>
          </div>
        </div>
      </div>

      <div className="settings__section">
        <div className="settings__body">
          <h3 className="auto__place">{t("auto.build.launch")}</h3>

          {check === null && <div className="auto__empty">{t("app.loading")}</div>}

          {check?.ready && <div className="auto__ready">{t("auto.ready")}</div>}

          {check && !check.ready && (
            <>
              <div className="auto__notready">{t("auto.notReady")}</div>
              <ul className="auto__blocks">
                {check.blocks.map((block, nth) => (
                  <li key={`${block.reason}-${block.stepId ?? ""}-${block.at ?? ""}-${nth}`}>
                    {blockText(block)}
                  </li>
                ))}
              </ul>
            </>
          )}

          <button
            type="button"
            className="btn btn--primary"
            disabled={!check?.ready || !onStart}
            onClick={() => onStart?.(id)}
          >
            {t("auto.start")}
          </button>
        </div>
      </div>

      {/* The picture of the steps. `AMB-T-5255` draws it. */}
      <div className="settings__section">
        <div className="settings__body">
          <h3 className="auto__place">{t("auto.build.picture")}</h3>
        </div>
      </div>

      {/* What the pressed step holds. `AMB-T-5256` draws it. */}
      <div className="settings__section">
        <div className="settings__body">
          <h3 className="auto__place">{t("auto.build.step")}</h3>
        </div>
      </div>
    </div>
  );
}
