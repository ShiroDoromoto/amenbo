// One automation, opened — the screen it is built and started from.
//
// **Three places, each named on the screen**: "launch", which refuses; "build", the picture of the
// steps (`./AutomationPicture`); and "step", what the pressed step holds (`./AutomationStepPanel`).
//
// **Which step is pressed is the screen's, not the picture's.** Two places read it — the picture
// marks that box and the panel draws that step — so it is held where both can see it. The `+` on a
// line opens a dialog nobody has built (`AMB-T-5257`), so that one is still held shut.
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
import { useState } from "react";
import { AutomationPicture } from "./AutomationPicture";
import { AutomationStepAdd } from "./AutomationStepAdd";
import { AutomationStepPanel } from "./AutomationStepPanel";
import { useAutomationStart } from "../components/StartAutomation";
import { useAutomation, useLaunchCheck } from "../core/automations";
import { useBoundFolders } from "../core/boundFolders";
import { errSentence, t } from "../core/i18n";
import { Icon } from "../components/Icon";
import type { AutomationDetailDto } from "../bindings/bindings";

/**
 * What carries out the step a line leaves — the likeliest answer for the step being put in front of
 * it, and what the dialog starts on. A definition that names none falls back to the first agent the
 * catalog lists, which is what `automation step add` asks for and never guesses.
 */
function agentOn(automation: AutomationDetailDto | null, edgeId: number): string {
  const edge = automation?.edges.find((one) => one.id === edgeId);
  return automation?.steps.find((one) => one.id === edge?.fromStepId)?.agent ?? "claude-code";
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
  // Which step the panel is showing. Nothing until a box is pressed — a definition opens on the
  // picture, and a step picked for the reader would be one they did not choose.
  const [step, setStep] = useState<number | null>(null);
  // The line a `+` was pressed on, while the dialog that puts a step in front of it is open. It is
  // the edge and not the step, because what the new step takes over is where that one line went.
  const [inserting, setInserting] = useState<number | null>(null);
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
                {/* Each reason names itself, so the sentence comes from the same place the press's
                    refusal writes its own from (`core/i18n`'s `errSentence`) — this list and that one
                    are the same words, and holding them apart is what let them drift. */}
                {check.blocks.map((block, nth) => (
                  <li key={`${block.code}-${nth}`}>{errSentence(block)}</li>
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
          <AutomationPicture
            automation={automation}
            selectedStepId={step ?? undefined}
            onPickStep={setStep}
            onInsertStep={setInserting}
          />
        </div>
      </div>

      <div className="settings__section">
        <div className="settings__body">
          <h3 className="auto__place">{t("auto.build.step")}</h3>
          <AutomationStepPanel automation={automation} stepId={step} projectId={projectId} />
        </div>
      </div>

      {inserting !== null && (
        <AutomationStepAdd
          edgeId={inserting}
          projectId={projectId}
          agent={agentOn(automation, inserting)}
          onClose={() => setInserting(null)}
        />
      )}
    </div>
  );
}
