// One automation, opened — the screen it is built and started from.
//
// **Three places, each named on the screen**: "launch", which refuses; "build", the picture of the
// steps (`./AutomationPicture`); and "step", the panel showing what the pressed step holds, which
// draws its heading and nothing under it until `AMB-T-5256` fills it.
//
// **The picture is handed the definition and no handlers yet.** Pressing a step is what the step
// place is for and the `+` on a line opens a dialog nobody has built (`AMB-T-5257`), so both are
// held shut here rather than wired to something that would do nothing.
//
// **The launch place refuses, the build place never does.** Building is always half-finished — a step
// with no way onward, an input nobody has wired — and every one of those saves
// (`amenbo_core::ops::automation`). What is unfinished only matters at the moment somebody is about
// to be let down by it, which is here: the reasons are listed, and the button is not offered while
// there is one.
//
// **The press can still be refused, and its refusal is drawn where the list is.** Two things move
// between the screen being drawn and the button being pressed — the machine, and whether the
// workspace is standing — and core raises both as a sentence rather than as a reason on the list
// (`amenbo_core::ops::automation_run::launch`). A run that took no lane is not a refusal: it says so
// and waits.
//
// **The list of reasons is read, not worked out here** (`../core/automations`). It was worked out
// here once, beside core's own, and the two disagreed on five points — so the screen could say an
// automation was ready and the press then refuse it (`AMB-T-5272`).
//
// **A closed workspace is not on the list.** It is not about the definition and stops being true the
// moment a window opens, so the launch raises it at the press rather than the build screen drawing it
// among things somebody has to go and fix (`amenbo_core::ops::automation_run::launch`). Whether it is
// standing is handed down from the shell, which is the one place that knows which window holds it.
import { AutomationPicture } from "./AutomationPicture";
import { useAutomationStart } from "../components/StartAutomation";
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
    case "no_entry":
      return t("auto.block.noEntry");
    case "entry_takes_no_task":
      return tf("auto.block.entryTakesNoTask", { step });
    case "open_exit":
      return block.at === undefined
        ? tf("auto.block.openExitUnnamed", { step })
        : tf("auto.block.openExit", { step, at });
    case "unwired_input":
      return tf("auto.block.unwiredInput", { step, at });
    case "unanswered_cfg":
      return tf("auto.block.unansweredCfg", { step, at });
    case "agent_missing":
      return tf("auto.block.agentMissing", { step, at });
    default:
      return block.reason;
  }
}

export function AutomationBuildScreen({
  id, projectId, workspaceOpen, onBack,
}: {
  id: number;
  /** Whose project this automation is — what the machine is asked about, and where its folders are. */
  projectId: number | null;
  /**
   * Whether the workspace is standing. It is handed down rather than asked here: which window holds
   * it is the shell's to know (`../shell/AppShell`, `AMB-D-753`), and core refuses a launch without
   * it — last of its three refusals, so a half-built automation is named before a window is.
   */
  workspaceOpen: boolean;
  onBack: () => void;
}) {
  const automation = useAutomation(id);
  const folders = useBoundFolders(projectId);
  const check = useLaunchCheck(id, projectId, folders.live.map((one) => one.path));
  // The press itself is the one every entrance makes (`../components/StartAutomation`): this screen
  // is where an automation is built, not a third place for a launch to behave differently.
  const { start, refused, queued, starting } = useAutomationStart(projectId, workspaceOpen);

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
                  <li key={`${block.reason}-${block.stepName ?? ""}-${block.at ?? ""}-${nth}`}>
                    {blockText(block)}
                  </li>
                ))}
              </ul>
            </>
          )}

          <button
            type="button"
            className="btn btn--primary"
            disabled={!check?.ready || starting || projectId === null}
            onClick={() => void start(id, folders.live.map((one) => one.path))}
          >
            {t("auto.start")}
          </button>

          {queued && <div className="auto__ready">{t("auto.queued")}</div>}
          {refused !== null && <div className="auto__notready">{refused}</div>}
        </div>
      </div>

      <div className="settings__section">
        <div className="settings__body">
          <h3 className="auto__place">{t("auto.build.picture")}</h3>
          <AutomationPicture automation={automation} />
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
