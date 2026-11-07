// The table under the picked role's default agent and model on the build screen: every step the role's
// round reaches, with the agent and model that carry it out there (`AMB-D-960`). It has no heading of
// its own; it goes on from the "Agent and model" one above it.
//
// **The rows go in the picture's order** (`pictureOrder`), placement by placement, and each
// placement's steps in their display order. A reader can then go down the table and along the
// picture together.
//
// **A step no agent carries out is not a row** (`AMB-D-1016`): a script step has no `carrier`. A step
// none of the step, the role and Amenbo chose an agent for has none either, and is left out with it:
// the start press names that one (`useLaunchCheck`).
//
// **A step that asks a person is not a row either** (`interactive`). An agent still opens it, so it
// keeps its `carrier` for the launch check and the pane; the table is for the steps an agent works
// through on its own, and leaves this one out as the mock does (`AMB-D-1121`).
//
// **The ● is a cell's, not the row's.** A step that chose for itself is marked only in the column it set
// apart from what it falls back to (`agentByStep`, `modelByStep`): one that took the role's agent and
// another model marks the model alone. A marked cell is in the AI's colour; a cell that falls back, to
// the role's default or Amenbo's, is muted and shows the value it fell back to. The words for where
// each was chosen are the GUI's (`AMB-D-396`).
import { t } from "../core/i18n";
import { pictureOrder, type PicGraph } from "./automationLayout";
import { useAgents } from "./automationPanel";
import type { AutomationCarrierDto, AutomationDetailDto } from "../bindings/bindings";

/** One row of the table: a step of a placement on the round, and who carries it out there. */
export type CarrierRow = { placementId: number; stepId: number; name: string; carrier: AutomationCarrierDto };

/**
 * The steps the round drawn in `graph` reaches, in the picture's order, each with who carries it out —
 * leaving out a step no agent carries out and a step that asks a person.
 */
export function roleCarrierRows(automation: AutomationDetailDto, graph: PicGraph | null): CarrierRow[] {
  if (graph === null) return [];
  const { numberOf } = pictureOrder(graph);
  const at = (id: number) => numberOf.get(id) ?? Number.POSITIVE_INFINITY;
  const onRound = new Set(graph.boxes.map((box) => box.id));
  return automation.placements
    .filter((one) => onRound.has(one.id))
    .sort((a, b) => at(a.id) - at(b.id))
    .flatMap((one) =>
      one.steps.flatMap((step) =>
        step.carrier === undefined || step.interactive
          ? []
          : [{ placementId: one.id, stepId: step.stepId, name: step.name, carrier: step.carrier }],
      ),
    );
}

export function AutomationRoleCarriers({
  automation,
  graph,
}: {
  automation: AutomationDetailDto;
  graph: PicGraph | null;
}) {
  const agents = useAgents(automation.projectId);
  const rows = roleCarrierRows(automation, graph);
  if (rows.length === 0) return null;
  const agentLabel = (id: string) => agents.find((one) => one.id === id)?.label ?? id;
  const cell = (value: string, byStep: boolean) => (
    <td className={byStep ? "autocarriers__cell--step" : "autocarriers__cell--fallen"}>
      {value}
      {byStep && (
        <span className="autocarriers__mark" title={t("auto.build.carrierByStep")}>
          {" ●"}
        </span>
      )}
    </td>
  );
  return (
    <div className="autocarriers">
      <table className="autocarriers__table">
        <thead>
          <tr>
            <th scope="col">{t("auto.build.carrierStep")}</th>
            <th scope="col">{t("auto.build.carrierAgent")}</th>
            <th scope="col">{t("auto.build.carrierModel")}</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => (
            <tr key={`${row.placementId}-${row.stepId}`}>
              <td>{row.name}</td>
              {cell(agentLabel(row.carrier.agent), row.carrier.agentByStep)}
              {cell(row.carrier.model ?? t("auto.step.modelDefault"), row.carrier.modelByStep)}
            </tr>
          ))}
        </tbody>
      </table>
      <p className="autocarriers__hint">{t("auto.build.carrierHint")}</p>
    </div>
  );
}
