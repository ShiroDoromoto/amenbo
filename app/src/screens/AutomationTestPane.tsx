// **A test run, stepped through on the build screen** (`AMB-T-5804`) — the automation walked from its
// entry to its end with no agent started, no built-in carried out and nothing kept
// (`amenbo_core::ops::automation_rehearse`), so the reader sees how it would go before anything real
// is set moving.
//
// **The walk is done at once, and stepped through here.** Core answers every step it opened in one go;
// "next" is this pane moving along that list, and the picture marks the box the step it stands on was
// opened from. Asking core again for each step would be a second walk, and nothing it does between
// two presses would be any different.
//
// **It asks what a launch asks, in the launch's own dialog** (`../components/LaunchHanding`): a test
// run walks from what the entry would be handed, so the two are asked the same way.
//
// **The head says no agent is running**, on every step: the prompt under it is what one would be
// started on, and a reader who took it for a pane at work would wait for an answer that never comes.
import { useCallback, useState } from "react";
import type { AutomationTestRunDto } from "../bindings/bindings";
import { LaunchHanding } from "../components/LaunchHanding";
import { testRunAutomation } from "../core/automations";
import { builtinWord } from "../core/builtinWords";
import { errText, t, tf } from "../core/i18n";
import { runExitWord, runReasonWord } from "../core/runWords";

/** A test run under way on the screen: what came back, and which of its steps the pane stands on. */
export type Testing = { walked: AutomationTestRunDto; at: number };

/**
 * The test run's press: ask what to hand over, walk, and hold what came back.
 *
 * `refused` is core's sentence — the launch check refuses a test run where it would refuse a launch —
 * cleared by the next press.
 */
export function useAutomationTestRun(projectId: number | null, onWalked: () => void) {
  const [asking, setAsking] = useState<{ id: number; name: string; folders: readonly string[] } | null>(null);
  const [testing, setTesting] = useState<Testing | null>(null);
  const [refused, setRefused] = useState<string | null>(null);
  const [walking, setWalking] = useState(false);

  const test = useCallback((id: number, name: string, folders: readonly string[]) => {
    if (projectId === null) return;
    setRefused(null);
    setAsking({ id, name, folders });
  }, [projectId]);

  const handing = asking === null || projectId === null ? null : (
    <LaunchHanding
      id={asking.id}
      name={asking.name}
      testing
      onClose={() => setAsking(null)}
      onStart={(handed) => {
        setAsking(null);
        setWalking(true);
        testRunAutomation(asking.id, projectId, asking.folders, handed)
          .then((walked) => {
            if (walked === null) return;
            setTesting({ walked, at: 0 });
            onWalked();
          })
          .catch((e) => setRefused(errText(e)))
          .finally(() => setWalking(false));
      }}
    />
  );

  const close = useCallback(() => setTesting(null), []);
  const move = useCallback((to: number) => setTesting((had) => (had === null ? null : { ...had, at: to })), []);

  /** The box the picture marks: the one the step the pane stands on was opened from. */
  const marked = testing === null ? undefined : testing.walked.steps[testing.at]?.placement;

  return { test, testing, refused, walking, handing, close, move, marked };
}

/** How the walk ended, in one sentence. */
function endedWord(walked: AutomationTestRunDto): string {
  if (walked.cut === "looped") return t("auto.test.looped");
  if (walked.cut === "too_long") return t("auto.test.tooLong");
  if (walked.missing.length > 0) return tf("auto.test.missing", { inputs: walked.missing.join(", ") });
  if (walked.noAgent !== undefined) return tf("auto.test.cantStart", { agent: walked.noAgent });
  if (walked.status === "failed") {
    const reason = runReasonWord(walked);
    return reason === null ? t("auto.run.failed") : tf("auto.test.failed", { reason });
  }
  return t("auto.test.completed");
}

/**
 * **The pane's body**: the step it stands on — who would do it, where, on what prompt and by which way
 * out it leaves — with the way to the next and the one before. Past the last step it says how the walk
 * ended.
 */
export function AutomationTestPane({
  testing,
  onMove,
}: {
  testing: Testing;
  onMove: (to: number) => void;
}) {
  const { walked, at } = testing;
  const total = walked.steps.length;
  const step = walked.steps[at];
  return (
    <div className="autotest">
      {step === undefined ? (
        <div className="autotest__end">
          <span className="autostep__label">{t("auto.test.end")}</span>
          <p className="autostep__said">{endedWord(walked)}</p>
        </div>
      ) : (
        <>
          <div className="autotest__at">{tf("auto.test.step", { n: at + 1, total })}</div>
          <div className="autotest__name">{builtinWord(step.builtin, step.name)}</div>
          <dl className="autotest__facts">
            <dt>{t("auto.test.who")}</dt>
            <dd>
              {step.builtin !== undefined
                ? t("auto.test.builtin")
                : [step.agent, step.model].filter((one) => one !== undefined).join(" · ")}
            </dd>
            {step.folder !== undefined && (
              <>
                <dt>{t("auto.test.folder")}</dt>
                <dd className="autotest__folder">{step.folder}</dd>
              </>
            )}
            <dt>{t("auto.test.exit")}</dt>
            <dd>{runExitWord(step.builtin, step.exit)}</dd>
          </dl>
          {step.prompt !== undefined && (
            <div className="autotest__prompt">
              <span className="autostep__label">{t("auto.test.prompt")}</span>
              <pre>{step.prompt}</pre>
            </div>
          )}
        </>
      )}
      <div className="buttonrow">
        <button type="button" className="btn" disabled={at === 0} onClick={() => onMove(at - 1)}>
          {t("auto.test.prev")}
        </button>
        <button type="button" className="btn btn--primary" disabled={at >= total} onClick={() => onMove(at + 1)}>
          {t("auto.test.next")}
        </button>
      </div>
    </div>
  );
}
