// One dimensional model for filtering.
//
// A filter is defined once, as a dimension with a human-facing label, and the GUI's controls grow
// out of that definition. Dimensions, value sets, predicates and labels all live here, leaving the
// screens to lay out one labelled row of values per dimension and nothing more.
//
// Each dimension carries a `cliKey` that pairs it with the CLI's --filter grammar
// (crates/amenbo-core/src/query.rs), which is how a new dimension stays aligned across GUI and CLI.
// The predicates themselves cannot cross languages, so this file is the GUI's source of truth. Text
// search and AMB-T-NNN / AMB-D-NNN references are not dimensions; the search box resolves them
// (`parseRefQuery`). Dimensions the user defined are grown from the snapshot (see custom, below).
import type { DecisionDto, DimensionDto } from "../bindings/bindings";
import { parseRef } from "./idref";
import type { Priority, TaskCard } from "../mock/types";
import { priorityLabel, statusLabel, t } from "./i18n";
import { STATUS_ALL } from "./status";

/**
 * One choice within a dimension. `value` matches the CLI's value; `label` is lazy so it follows the
 * language. `T` is what the choice judges — a task on the board, a decision on the decisions tab —
 * and it is the only thing that differs between the two sides.
 */
export type FilterOption<T> = {
  value: string;
  label: () => string;
  test: (item: T) => boolean;
};

/** A single filter dimension (status, assignee, priority, and so on). */
export type FilterDimension<T> = {
  id: string;
  label: () => string;
  /** The matching CLI --filter key; this is what keeps the two in step with query.rs. */
  cliKey: string;
  options: FilterOption<T>[];
};

/**
 * The selection (dimension id to the values chosen on it). A missing key or an empty array means the
 * dimension does not filter, so choosing every value on an axis and choosing none of them narrow the
 * same way (`AMB-D-655`). No value names more than one state: a set is what the reader composes, and
 * a word standing for a group of states — "closed" for done-and-rejected — is what that replaces.
 */
export type FilterSelection = Record<string, string[]>;

const PRIORITIES: Priority[] = ["high", "medium", "low"];

/**
 * Task — or decision — to (dimension id to the value ids assigned on it). Dimension and value ids are
 * integer keys, and so is the outer one, carried as a string because that is what a `Record` key is.
 * One shape for both sides: a classification is the same fact whichever of the two carries it.
 *
 * The innermost value is a list because an axis may admit several at once (`AMB-D-826`); a
 * single-select axis is the one-element case of the same shape, so nothing downstream branches on
 * which kind of axis it is reading. An axis a record sits on no value of is absent, never `[]`.
 */
export type DimAssignments = Record<string, Record<number, number[]>>;

/**
 * The axes the board filters on. The assignee options me / me-ai are decided by facet kind (in a
 * single local world the human facet is me, the ai facet is my AI). Pass userDims/dimAssign and the
 * project's user-defined dimensions join the filters too: the value set is each dimension's values,
 * and the predicate is "is this task assigned this value on this dimension", read from the assignment
 * map (supplied in bulk from the read-model, by BoardScreen).
 */
export function filterDimensions(
  userDims: DimensionDto[] = [],
  dimAssign: DimAssignments = {},
): FilterDimension<TaskCard>[] {
  const builtin: FilterDimension<TaskCard>[] = [
    {
      id: "status",
      label: () => t("filter.dim.status"),
      cliKey: "status:",
      options: STATUS_ALL.map((s) => ({
        value: s,
        label: () => statusLabel(s),
        test: (task: TaskCard) => task.status === s,
      })),
    },
    {
      id: "assignee",
      label: () => t("filter.dim.assignee"),
      cliKey: "assignee:",
      options: [
        { value: "none", label: () => t("filter.opt.assignee.none"), test: (task) => !task.assignee },
        {
          value: "me",
          label: () => t("filter.opt.assignee.me"),
          test: (task) => task.assignee?.kind === "human",
        },
        {
          value: "me-ai",
          label: () => t("filter.opt.assignee.meAi"),
          test: (task) => task.assignee?.kind === "ai",
        },
      ],
    },
    {
      id: "priority",
      label: () => t("filter.dim.priority"),
      cliKey: "priority:",
      options: PRIORITIES.map((p) => ({
        value: p,
        label: () => priorityLabel(p),
        test: (task) => task.priority === p,
      })),
    },
  ];
  return [...builtin, ...customDimensions<TaskCard>(userDims, dimAssign, (task) => task.id)];
}

/**
 * The axes the decisions tab filters on: the decision's own status, then the project's user-defined
 * axes — the same shape the board has, built from the same pieces, because narrowing a list of
 * decisions is the same act as narrowing a list of tasks and the two tabs should not ask for it
 * differently. The caller hands in the axes that classify decisions (`axesFor("decision", …)`,
 * `AMB-D-789`) and their assignments, read in bulk the way the board reads its own.
 *
 * "Superseded" rides on the status axis while keeping a label of its own: it is an edge and not a
 * status (`AMB-D-410`), so it is offered where a reader looks for it without the status namespace
 * being made to hold something that is not one.
 */
export function decisionFilterDimensions(
  userDims: DimensionDto[] = [],
  dimAssign: DimAssignments = {},
): FilterDimension<DecisionDto>[] {
  const status: FilterDimension<DecisionDto> = {
    id: "status",
    label: () => t("filter.dim.status"),
    cliKey: "status:",
    options: [
      ...DECISION_STATUSES.map((s) => ({
        value: s,
        label: () => t(`dec.status.${s}`),
        test: (d: DecisionDto) => d.status === s,
      })),
      {
        value: "superseded",
        label: () => t("dec.filterSuperseded"),
        test: (d: DecisionDto) => d.supersededBy.length > 0,
      },
    ],
  };
  return [status, ...customDimensions<DecisionDto>(userDims, dimAssign, (d) => d.id)];
}

const DECISION_STATUSES: DecisionDto["status"][] = ["proposed", "accepted", "rejected"];

/**
 * The user-defined axes as filter dimensions, for whichever side is asking. An axis with no values
 * filters nothing, so it is left out. The CLI has the same dimension filter (`dim:axis=value` in
 * query.rs), which is what cliKey lines up with.
 *
 * **A closed value is offered like any other** (`AMB-D-829`). Closing retires a value from what a
 * record is newly filed under — which is the picker's business, and the board's — and leaves every
 * record already filed under it exactly where it was. Asking what carried a finished release is the
 * whole reason to close one rather than delete it, so this is the one face that draws no distinction.
 */
function customDimensions<T>(
  userDims: DimensionDto[],
  dimAssign: DimAssignments,
  idOf: (item: T) => number,
): FilterDimension<T>[] {
  return userDims
    .filter((dim) => dim.values.length > 0)
    .map((dim) => ({
      id: `dim:${dim.id}`,
      label: () => dim.name,
      cliKey: `dim:${dim.id}=`,
      options: dim.values.map((v) => ({
        // `FilterSelection` is one Record across all dimensions, so what it holds are strings; an integer key is carried in that form.
        value: String(v.id),
        label: () => v.name,
        test: (item: T) => dimAssign[idOf(item)]?.[dim.id]?.includes(v.id) ?? false,
      })),
    }));
}

/**
 * Whether the task — or decision — matches the selection: dimensions are ANDed, and the values within
 * one are ORed, the same shape the CLI's grammar has (`status:todo,in_progress dim:…`, query.rs). A
 * value that names no option — one left over from a dimension value since deleted — is dropped rather
 * than failing the item, so an axis whose whole selection went stale narrows nothing instead of
 * emptying the list.
 */
export function passesFilters<T>(item: T, dims: FilterDimension<T>[], sel: FilterSelection): boolean {
  return dims.every((d) => {
    const chosen = d.options.filter((o) => sel[d.id]?.includes(o.value));
    return chosen.length === 0 || chosen.some((o) => o.test(item));
  });
}

/**
 * Read a number out of the search box: when the query is a reference, take it as an intent to filter
 * by that number. The grammar lives in `core/idref.ts` rather than here, so the box and body-text links
 * cannot drift apart. Anything unrecognised returns null and falls back to text search — the box does
 * double duty, and in number mode no text matching happens.
 */
export type RefQuery = { num: number; space: "task" | "decision" };
export function parseRefQuery(raw: string): RefQuery | null {
  return parseRef(raw);
}

/**
 * Serialise the selection into a stable key for pager/memo recomputation (dimensions narrowing
 * nothing are dropped). The values are sorted as well as the dimensions: the same set chosen in
 * another order is the same question, and a key that said otherwise would reset the pager on a
 * re-click that changed nothing.
 */
export function selectionKey(sel: FilterSelection): string {
  return Object.entries(sel)
    .filter(([, vs]) => vs.length > 0)
    .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
    .map(([k, vs]) => `${k}=${[...vs].sort().join(",")}`)
    .join("&");
}
