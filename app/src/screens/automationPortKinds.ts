// What a port carries, and what each of the four is called on screen (`AMB-T-5257`).
//
// Spelled out rather than built from the id, so the key gate can see every label a reader can be
// shown (`core/i18n/sourceKeys.test.ts`). The order is core's own
// (`amenbo_core::model::AutomationPortKind`), which is also the order a person meets them in: a value
// and a file are what a step hands on, and the two task kinds are what a run is about.
//
// **A way out is not offered the task a run works** (`AMB-D-964`). Only a built-in takes that task,
// and core refuses an output that carries it, so the row that declares one leaves it out
// (`OUTPUT_KINDS`). An input still takes it: that is how a step reads the task a built-in took.
import { t } from "../core/i18n";
import type { AutomationPortDto } from "../bindings/bindings";

/** The four kinds, in the order they are offered. */
export const PORT_KINDS: readonly { id: AutomationPortDto["kind"]; label: () => string }[] = [
  { id: "value", label: () => t("auto.kind.value") },
  { id: "file", label: () => t("auto.kind.file") },
  { id: "task_take", label: () => t("auto.kind.taskTake") },
  { id: "task_make", label: () => t("auto.kind.taskMake") },
];

/** What a way out can hand on — every kind but the task a run works. */
export const OUTPUT_KINDS = PORT_KINDS.filter((one) => one.id !== "task_take");

/** One kind, in words. A kind core knows and this does not is shown as core spells it. */
export function kindLabel(kind: string): string {
  return PORT_KINDS.find((one) => one.id === kind)?.label() ?? kind;
}
