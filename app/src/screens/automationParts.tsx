// The small parts every automation screen draws the same way — the list, the build screens, their
// panels and the dialogs.
//
// **One meaning, one part, one word.** Where an action is kept, how many automations use it, that
// it cannot be changed here, a way out, and where a placement is about to go are each drawn once,
// here, because the same thing said three ways reads as three things. A screen hands a part the
// facts rather than writing its own words around them, so no screen needs a sentence explaining it.
import { axesFor } from "../core/appliesTo";
import { isStatus, statusLabel, t, tn } from "../core/i18n";
import { builtinWord } from "../core/builtinWords";
import { getSnapshot } from "../core/snapshot";
import { Icon } from "../components/Icon";
import { ERROR_EXIT } from "./automationLayout";
import { exitLabel } from "./automationPanel";
import type { DimensionDto } from "../bindings/bindings";

/**
 * The reach an action sits in, as a chip — the same chip wherever an action is named. A built-in is
 * kept on the device's shelf, but what it is to a reader is Amenbo's own.
 */
export function ReachChip({
  global,
  builtin = false,
  anyProject = false,
}: {
  global: boolean;
  builtin?: boolean;
  /** Whether rows of every project are listed, where "this project" would name none of them. */
  anyProject?: boolean;
}) {
  const tone = builtin ? "actscope actscope--builtin" : global ? "actscope actscope--global" : "actscope";
  return (
    <span className={tone}>
      <em aria-hidden="true" />
      {builtin
        ? t("auto.actions.reachBuiltin")
        : global
          ? t("auto.actions.reachGlobal")
          : anyProject
            ? t("auto.actions.reachAnyProject")
            : t("auto.actions.reachProject")}
    </span>
  );
}

/** How many automations use an action, in the one way every screen says it. */
export function usedCount(n: number): string {
  return n === 0 ? t("auto.actions.unused") : tn("auto.actions.usedBy", n);
}

/**
 * That what is shown cannot be changed here, as a mark rather than a sentence. Where it can be
 * changed is the button beside it, which the screen draws because only the screen knows where that is.
 */
export function LockMark() {
  return (
    <span className="lockmark" title={t("auto.lockMark")}>
      <Icon name="lock" label={t("auto.lockMark")} />
    </span>
  );
}

/**
 * A setting that is on or off, as a switch rather than a tick — the shape says it can be turned back.
 * It is a checkbox underneath, so the label around it and the keyboard work as they do for one; the
 * track is drawn over it (`.switch`).
 */
export function Switch({
  checked,
  onChange,
}: {
  checked: boolean;
  onChange: (next: boolean) => void;
}) {
  return (
    <span className="switch">
      <input type="checkbox" role="switch" checked={checked} onChange={(e) => onChange(e.target.checked)} />
      <i aria-hidden="true" />
    </span>
  );
}

/**
 * One way out, as a mark — the same one in the picture, a panel and a dialog. The error way out is
 * drawn dashed in the stop colour, so it reads apart from the ones a reader named. `builtin` is the
 * key of the built-in the name belongs to, which puts its words in the screen's language.
 */
export function ExitMark({
  name,
  builtin,
  outputs = [],
}: {
  name: string;
  builtin?: string | null;
  /** What the way out hands on, already in the words the screen shows. */
  outputs?: readonly string[];
}) {
  const error = name === ERROR_EXIT;
  return (
    <span className={error ? "actport actport--error" : "actport actport--exit"}>
      {exitLabel(error ? name : builtinWord(builtin, name))}
      {outputs.length > 0 && <span className="actport__kind">{outputs.join("・")}</span>}
    </span>
  );
}

/**
 * Where a placement is about to go: after one way out of one box, or onto a picture with no line.
 * `next` is the box that way out leads to now, which the placement will come to lead to instead.
 */
export type WhereTo = {
  box: string;
  /** The way out it hangs on, or undefined while the line has not been read. */
  exit: string | undefined;
  builtin?: string | null;
  next?: string;
} | null;

/**
 * Where a placement is about to go, as a small picture of the line it goes on: the box before it,
 * the way out it hangs on, a dashed "here", and the box after it where the line goes on to one.
 * Onto an empty picture there is nothing before it, so the "here" stands alone.
 */
export function WhereMark({ where }: { where: WhereTo }) {
  return (
    <div className="wheremark">
      {where !== null && (
        <>
          <span className="wheremark__box">{where.box}</span>
          <span className="wheremark__line" aria-hidden="true">─</span>
          {where.exit !== undefined && <ExitMark name={where.exit} builtin={where.builtin} />}
          <span className="wheremark__line" aria-hidden="true">─▶</span>
        </>
      )}
      <span className="wheremark__here">{t("auto.lib.here")}</span>
      {where?.next !== undefined && (
        <>
          <span className="wheremark__line" aria-hidden="true">─▶</span>
          <span className="wheremark__box">{where.next}</span>
        </>
      )}
    </div>
  );
}

/**
 * One value pressed on a task filter's row, in the words the row draws it with — the placement's
 * panel and a built-in waiting on its pane say the same value the same way.
 */
export function filterValueLabel(key: string, value: string): string {
  if (key === "status") return isStatus(value) ? statusLabel(value) : value;
  if (key === "assignee") {
    if (value === "none") return t("filter.opt.assignee.none");
    if (value === "me") return t("filter.opt.assignee.me");
    return t("filter.opt.assignee.meAi");
  }
  if (key === "ready") return value === "yes" ? t("auto.step.readyYes") : t("auto.step.readyNo");
  return `${key}:${value}`;
}

/**
 * **A task filter as the chips its values are**, read out of `task list --filter`'s words — what a
 * built-in waiting for a task looks for, drawn the way the placement's panel draws the answer rather
 * than as the expression. A part the rows have no words for is shown as the filter spells it.
 */
export function FilterChips({ expression }: { expression: string }) {
  const chips: { key: string; word: string }[] = [];
  for (const part of expression.split(/\s+/)) {
    const at = part.indexOf(":");
    if (at <= 0) continue;
    const key = part.slice(0, at);
    for (const value of part.slice(at + 1).split(",")) {
      if (value !== "") chips.push({ key: `${key}:${value}`, word: filterValueLabel(key, value) });
    }
  }
  return (
    <span className="filterchips">
      {chips.map((one) => (
        <span key={one.key} className="chip chip--block">{one.word}</span>
      ))}
    </span>
  );
}

/**
 * The axes a task Amenbo files on its own can be classified on in this project — a failed round's, or
 * a trigger's — each with the values that can be chosen: the open ones, and a closed one only where it
 * is already chosen (`AMB-D-829`). An axis with nothing to press is left out.
 */
export function classAxes(projectId: number, chosen: readonly number[]): DimensionDto[] {
  const dims = getSnapshot().projects.find((p) => p.id === projectId)?.dimensions ?? [];
  return axesFor("task", dims)
    .map((dim) => ({ ...dim, values: dim.values.filter((value) => !value.closed || chosen.includes(value.id)) }))
    .filter((dim) => dim.values.length > 0);
}

/** The classification with `value` pressed on `axis`: it alone on the axis, or none where it was on. */
export function pressedValue(chosen: readonly number[], axis: DimensionDto, value: number): number[] {
  const onAxis = new Set(axis.values.map((one) => one.id));
  const rest = chosen.filter((one) => !onAxis.has(one));
  return chosen.includes(value) ? rest : [...rest, value];
}

/**
 * **A classification as rows of chips**: a row per axis of {@link classAxes}, its name marked where
 * the project requires it, its values pressed one an axis. With no axis to classify on, it says so.
 */
export function ClassRows({
  projectId,
  chosen,
  onChange,
}: {
  projectId: number;
  chosen: readonly number[];
  onChange: (chosen: number[]) => void;
}) {
  const axes = classAxes(projectId, chosen);
  if (axes.length === 0) return <span className="autostep__hint">{t("auto.step.noAxes")}</span>;
  return (
    <div className="autostep__rows">
      {axes.map((dim) => (
        <div key={dim.id} className="autostep__row">
          <span className="autostep__rowname">
            {dim.name}
            {dim.required && <span className="autostep__req">{t("auto.side.required")}</span>}
          </span>
          {dim.values.map((one) => {
            const on = chosen.includes(one.id);
            return (
              <button
                key={one.id}
                type="button"
                className={`autostep__chip ${on ? "autostep__chip--on" : ""}`}
                aria-pressed={on}
                onClick={() => onChange(pressedValue(chosen, dim, one.id))}
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
