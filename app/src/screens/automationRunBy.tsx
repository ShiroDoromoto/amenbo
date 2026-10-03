// Who carries a step out — an AI on its prompt, or a script (`AMB-D-1016`) — as the two presses the
// step's panel and the dialog that adds a step both draw. One shape for the one choice, so the dialog
// and the panel a reader lands on after it read alike.
import { t } from "../core/i18n";

export function RunBy({ script, onChange }: { script: boolean; onChange: (script: boolean) => void }) {
  return (
    <div className="autogive" role="group" aria-label={t("auto.step.howRuns")}>
      {[
        { script: false, label: t("auto.step.byAi") },
        { script: true, label: t("auto.step.byScript") },
      ].map((one) => (
        <button
          key={one.label}
          type="button"
          className={one.script === script ? "autogive__tog autogive__tog--on" : "autogive__tog"}
          aria-pressed={one.script === script}
          onClick={() => onChange(one.script)}
        >
          {one.label}
        </button>
      ))}
    </div>
  );
}
