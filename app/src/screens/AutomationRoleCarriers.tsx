// The table under the build screen's picture: every step the picked role's round reaches, with the
// agent and model that carry it out there (`AMB-D-960`).
//
// **The rows go in the picture's order** (`pictureOrder`), placement by placement, and each
// placement's steps in their display order. A reader can then go down the table and along the
// picture together.
//
// **A step no agent carries out is not a row** (`AMB-D-1016`): a script step has no `carrier`. A step
// none of the step, the role and Amenbo chose an agent for has none either, and is left out with it:
// the start press names that one (`useLaunchCheck`).
//
// **The ● is the row's, not a cell's.** Where a step was chosen at is one place for the agent and its
// model together (`AutomationChosenAtDto`, `AMB-D-865`), so a step that chose for itself marks both.
// The words for where each was chosen are the GUI's (`AMB-D-396`).
import { t } from "../core/i18n";
import { pictureOrder, type PicGraph } from "./automationLayout";
import { useAgents } from "./automationPanel";
import type { AutomationCarrierDto, AutomationDetailDto } from "../bindings/bindings";

/** One row of the table: a step of a placement on the round, and who carries it out there. */
export type CarrierRow = { placementId: number; stepId: number; name: string; carrier: AutomationCarrierDto };

/** The steps the round drawn in `graph` reaches, in the picture's order, each with who carries it out. */
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
        step.carrier === undefined
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
  const mark = <span className="autocarriers__mark" title={t("auto.build.carrierByStep")}> ●</span>;
  return (
    <div className="autocarriers">
      <span className="actbuild__sec">{t("auto.build.carriers")}</span>
      <table className="autocarriers__table">
        <thead>
          <tr>
            <th scope="col">{t("auto.build.carrierStep")}</th>
            <th scope="col">{t("auto.build.carrierAgent")}</th>
            <th scope="col">{t("auto.build.carrierModel")}</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => {
            const byStep = row.carrier.chosenAt.kind === "step";
            return (
              <tr key={`${row.placementId}-${row.stepId}`} className={byStep ? "autocarriers__row--step" : undefined}>
                <td>{row.name}</td>
                <td>
                  {agentLabel(row.carrier.agent)}
                  {byStep && mark}
                </td>
                <td>
                  {row.carrier.model ?? t("auto.step.modelDefault")}
                  {byStep && mark}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
      <p className="autocarriers__hint">{t("auto.build.carrierHint")}</p>
    </div>
  );
}
