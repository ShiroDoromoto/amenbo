// One dimensional model for filtering.
//
// A filter is defined once, as a dimension with a human-facing label, and the GUI's controls grow
// out of that definition. Dimensions, value sets, predicates and labels all live here, leaving the
// screens to lay out labelled <select>s and nothing more.
//
// Each dimension carries a `cliKey` that pairs it with the CLI's --filter grammar
// (crates/amenbo-core/src/query.rs), which is how a new dimension stays aligned across GUI and CLI.
// The predicates themselves cannot cross languages, so this file is the GUI's source of truth. Text
// search and AMB-T-NNN / AMB-D-NNN references are not dimensions; the search box resolves them
// (`parseRefQuery`). Dimensions the user defined are grown from the snapshot (see custom, below).
import type { DimensionDto } from "../bindings/bindings";
import { parseRef } from "./idref";
import type { Priority, Status, TaskCard } from "../mock/types";
import { priorityLabel, statusLabel, t } from "./i18n";

/** One choice within a dimension. `value` matches the CLI's value; `label` is lazy so it follows the language. */
export type FilterOption = {
  value: string;
  label: () => string;
  test: (task: TaskCard) => boolean;
};

/** A single filter dimension (status, assignee, priority, and so on). */
export type FilterDimension = {
  id: string;
  label: () => string;
  /** The matching CLI --filter key; this is what keeps the two in step with query.rs. */
  cliKey: string;
  options: FilterOption[];
};

/** The selection (dimension id to option value). A missing key or "" means the dimension does not filter. */
export type FilterSelection = Record<string, string>;

const STATUSES: Status[] = ["todo", "in_progress", "blocked", "done"];
const PRIORITIES: Priority[] = ["high", "medium", "low"];

/** Task to (dimension id to assigned value id). Dimension and value ids are integer keys. */
export type DimAssignments = Record<string, Record<number, number>>;

/**
 * Build the list of filter dimensions. The assignee options me / me-ai are decided against this
 * machine's own user_id, so the dimensions are built from meUserId. Pass userDims/dimAssign and the
 * project's user-defined dimensions join the filters too: the value set is each dimension's values,
 * and the predicate is "is this task assigned this value on this dimension", read from the
 * assignment map (supplied in bulk from the read-model, by BoardScreen). A dimension with no values
 * filters nothing, so it is left out. The CLI has the same dimension filter (`dim:axis=value` in
 * query.rs), which is what cliKey lines up with.
 */
export function filterDimensions(
  meUserId: string,
  userDims: DimensionDto[] = [],
  dimAssign: DimAssignments = {},
): FilterDimension[] {
  const builtin: FilterDimension[] = [
    {
      id: "status",
      label: () => t("filter.dim.status"),
      cliKey: "status:",
      options: STATUSES.map((s) => ({
        value: s,
        label: () => statusLabel(s),
        test: (task) => task.status === s,
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
          test: (task) => task.assignee?.userId === meUserId && task.assignee?.kind === "human",
        },
        {
          value: "me-ai",
          label: () => t("filter.opt.assignee.meAi"),
          test: (task) => task.assignee?.userId === meUserId && task.assignee?.kind === "ai",
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
  const custom: FilterDimension[] = userDims
    .filter((dim) => dim.values.length > 0)
    .map((dim) => ({
      id: `dim:${dim.id}`,
      label: () => dim.name,
      cliKey: `dim:${dim.id}=`,
      options: dim.values.map((v) => ({
        // `FilterSelection` is one Record across all dimensions, so its values are strings; an integer key is carried in that form.
        value: String(v.id),
        label: () => v.name,
        test: (task: TaskCard) => dimAssign[task.id]?.[dim.id] === v.id,
      })),
    }));
  return [...builtin, ...custom];
}

/** Whether the task matches the selection. Dimensions are ANDed. */
export function passesFilters(task: TaskCard, dims: FilterDimension[], sel: FilterSelection): boolean {
  return dims.every((d) => {
    const v = sel[d.id];
    if (!v) return true;
    const opt = d.options.find((o) => o.value === v);
    return opt ? opt.test(task) : true;
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

/** Serialise the selection into a stable key for pager/memo recomputation (unset dimensions are dropped). */
export function selectionKey(sel: FilterSelection): string {
  return Object.entries(sel)
    .filter(([, v]) => v)
    .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
    .map(([k, v]) => `${k}=${v}`)
    .join("&");
}
