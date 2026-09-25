// **The settings of the built-in that files a task that name several things** (`AMB-D-971`), each
// answered with the control what it names takes rather than a line of text.
//
// Core reads each of them one thing a line (`amenbo_core::ops::automation_builtin_make`): a
// classification as `axis=value`, an axis the step before it chooses on by its name, and a task or a
// decision it depends on or links to by its number. A one-line box cannot hold two lines, so each is
// drawn here instead: a row of values per axis, a row of axes, and a box that takes a line per number.
//
// **An axis already classified is not offered to the AI** — core refuses a value the step before it
// chooses on an axis `CLASSIFY` fixes, so the row leaves those axes out rather than let a refusal come at
// run time.
import type { DimensionDto } from "../bindings/bindings";
import { axesFor } from "../core/appliesTo";
import { getSnapshot } from "../core/snapshot";
import { t } from "../core/i18n";
import { classAxis, classLine, pressedClass, readLines, writeLines } from "./automationCfg";
import { useDraft } from "./automationPanel";

/** The built-in that files a task (`amenbo_core::ops::automation_builtin_make::MAKE_TASK`). */
const MAKE_TASK = "make_task";
/** Its settings that name several things, by the store's word — what core reads each answer by. */
export const CLASSIFY = "分類";
const AI_AXES = "AI に選ばせる軸";
const NUMBERS: readonly string[] = ["依存させる既存のタスク", "リンクする決定"];

/** Which of the controls here answers a setting, or `undefined` for one the panel answers itself. */
export function makeTaskControl(
  builtin: string | undefined,
  name: string,
): "classes" | "axes" | "numbers" | undefined {
  if (builtin !== MAKE_TASK) return undefined;
  if (name === CLASSIFY) return "classes";
  if (name === AI_AXES) return "axes";
  return NUMBERS.includes(name) ? "numbers" : undefined;
}

/** The axes a task is filed under in this project, with the values it can newly be filed under. */
function taskAxes(projectId: number | null): DimensionDto[] {
  const dims = getSnapshot().projects.find((p) => p.id === projectId)?.dimensions ?? [];
  return axesFor("task", dims).map((dim) => ({ ...dim, values: dim.values.filter((value) => !value.closed) }));
}

/** **`CLASSIFY`**: a row per axis, each value pressed on it one line of the answer. */
export function ClassRows({ projectId, value, onAnswer }: {
  projectId: number | null;
  value: string | undefined;
  onAnswer: (value: string | null) => void;
}) {
  const lines = readLines(value);
  const axes = taskAxes(projectId).filter((dim) => dim.values.length > 0);
  if (axes.length === 0) return <div className="actdecl__none">{t("auto.step.noAxes")}</div>;
  return (
    <div className="autostep__rows">
      {axes.map((dim) => (
        <div key={dim.id} className="autostep__row">
          <span className="autostep__rowname">{dim.name}</span>
          {dim.values.map((one) => {
            const on = lines.includes(classLine(dim.name, one.name));
            return (
              <button
                key={one.id}
                type="button"
                className={`autostep__chip ${on ? "autostep__chip--on" : ""}`}
                aria-pressed={on}
                onClick={() => onAnswer(writeLines(pressedClass(lines, dim.name, one.name, dim.cardinality === "single")))}
              >
                {one.name}
              </button>
            );
          })}
        </div>
      ))}
    </div>
  );
}

/** **`AI_AXES`**: the axes the step before it may choose a value on, leaving out those `CLASSIFY` fixes. */
export function AxisChips({ projectId, value, classified, onAnswer }: {
  projectId: number | null;
  value: string | undefined;
  /** The answer to `CLASSIFY` on the same spot. */
  classified: string | undefined;
  onAnswer: (value: string | null) => void;
}) {
  const lines = readLines(value);
  const fixed = new Set(readLines(classified).map(classAxis));
  const axes = taskAxes(projectId).filter((dim) => !fixed.has(dim.name));
  if (axes.length === 0) return <div className="actdecl__none">{t("auto.step.noAxes")}</div>;
  return (
    <div className="autostep__row">
      {axes.map((dim) => {
        const on = lines.includes(dim.name);
        return (
          <button
            key={dim.id}
            type="button"
            className={`autostep__chip ${on ? "autostep__chip--on" : ""}`}
            aria-pressed={on}
            onClick={() => onAnswer(writeLines(on ? lines.filter((one) => one !== dim.name) : [...lines, dim.name]))}
          >
            {dim.name}
          </button>
        );
      })}
    </div>
  );
}

/** **A task or a decision by number, one a line** — the box writes what it holds when it is left. */
export function NumberLines({ label, value, onAnswer }: {
  label: string;
  value: string | undefined;
  onAnswer: (value: string | null) => void;
}) {
  const written = readLines(value).join("\n");
  const [text, setText] = useDraft(written);
  return (
    <textarea
      aria-label={label}
      rows={3}
      placeholder={t("auto.step.oneALine")}
      value={text}
      onChange={(e) => setText(e.target.value)}
      onBlur={() => {
        const lines = text.split("\n").map((line) => line.trim()).filter((line) => line !== "");
        if (lines.join("\n") !== written) onAnswer(writeLines(lines));
      }}
    />
  );
}
