// What the pressed step holds, on the action build screen (`AMB-T-5315`).
//
// **It is the step's own panel, not a placement's.** A box inside an action is a step, and a step is
// one terminal: the prompt it runs on, who is asked to carry it out, the model and the three flags
// are all its own (`AMB-D-950`). What the action declares to the automations placing it is the other
// panel's (`./AutomationActionDeclaresPanel`), and nothing here writes on that layer.
//
// **What comes after a way out is written here too.** Inside an action the line and the box it
// leaves are read together — one way out decides one thing — so the way out's row carries the
// pulldown that says where the run goes next, rather than sending a reader to the picture to draw it.
//
// **A wire is picked from a list, not drawn** (`./automationWires`), the way it is on an automation,
// and what does not fit is not offered.
//
// **A refusal is drawn, once, at the top**, for `./AutomationStepPanel`'s reason: every press here
// can be refused by core — a name already taken, the error way out, a line that leaves the picture —
// and the last refusal stands where the reader is looking.
import { useState } from "react";
import {
  clearAutomationEdge,
  clearAutomationWire,
  declareAutomationExit,
  declareAutomationInput,
  editAutomationInput,
  editAutomationStep,
  removeAutomationExit,
  removeAutomationInput,
  removeAutomationStep,
  renameAutomationExit,
  setAutomationActionEntry,
  setAutomationEdge,
  setAutomationWire,
} from "../core/automations";
import { confirmDialog } from "../core/dialog";
import { errText, t, tf } from "../core/i18n";
import { ErrorNote } from "../components/ErrorNote";
import { actionGraph, ERROR_EXIT } from "./automationLayout";
import {
  choicesOfKinds,
  DeclareRow,
  DeclEdit,
  exitLabel,
  useAgents,
  useDraft,
  useModels,
  type Run,
} from "./automationPanel";
import { choiceKey, wireChoices, wireInto } from "./automationWires";
import { AutomationOutputAdd } from "./AutomationOutputAdd";
import { kindLabel, PORT_KINDS } from "./automationPortKinds";
import type {
  AutomationActionDetailDto,
  AutomationExitDto,
  AutomationPortDto,
  AutomationStepDto,
} from "../bindings/bindings";

/** What the pulldown on a way out hands back — the three things an edge can say. */
const ENDS = ["go", "done", "halt"] as const;

/** What one way out says happens next, as the pulldown holds it. Empty is nothing said yet. */
function onwardKey(ends: (typeof ENDS)[number], to?: number): string {
  return ends === "go" ? `go:${to}` : ends;
}

/**
 * One way out of a step: what it is called, what leaving by it hands on, where the run goes next, and
 * the press that takes it away.
 *
 * **The error way out has a row too**, and only the pulldown on it: it is carried from birth and
 * neither renamed nor removed (core refuses both), while what follows it is exactly what a builder
 * may want to say — with nothing said, it stops the run and calls a person.
 */
function ExitRow({
  action,
  step,
  exit,
  onAddOutput,
  run,
}: {
  action: AutomationActionDetailDto;
  step: AutomationStepDto;
  exit: AutomationExitDto;
  onAddOutput: () => void;
  run: Run;
}) {
  const [name, setName] = useDraft(exit.name ?? "");
  const was = exit.name ?? null;
  const isError = exit.name === ERROR_EXIT;
  const edge = action.edges.find(
    (one) => one.fromId === step.id && (one.exitName ?? null) === was,
  );
  const picked = edge === undefined ? "" : onwardKey(edge.ends, edge.toId);

  const say = (chosen: string) => {
    if (chosen === "") {
      void run(clearAutomationEdge("action", { boxId: step.id, exitName: exit.name }));
      return;
    }
    const onward = chosen.startsWith("go:")
      ? ({ ends: "go", to: Number(chosen.slice("go:".length)) } as const)
      : ({ ends: chosen as "done" | "halt" } as const);
    void run(setAutomationEdge("action", { boxId: step.id, exitName: exit.name }, onward));
  };

  return (
    <li className={isError ? "autostep__exiterr" : "autostep__exit"}>
      {isError ? (
        <span className="autostep__said">{t("auto.pic.errorExit")}</span>
      ) : (
        <input
          className="autostep__declname"
          placeholder={t("auto.step.exitUnnamed")}
          aria-label={t("auto.step.exits")}
          value={name}
          onChange={(e) => setName(e.target.value)}
          onBlur={() => {
            const now = name.trim() === "" ? null : name.trim();
            if (now !== was) void run(renameAutomationExit("step", step.id, was, now));
          }}
        />
      )}

      {exit.outputs.map((port) => (
        <span key={port.name} className="autostep__out">
          {port.name}
          <span className="autostep__outkind">{kindLabel(port.kind)}</span>
        </span>
      ))}

      <select aria-label={t("auto.act.next")} value={picked} onChange={(e) => say(e.target.value)}>
        <option value="">{t("auto.act.nextNone")}</option>
        {action.steps
          .filter((one) => one.id !== step.id)
          .map((one) => (
            <option key={one.id} value={onwardKey("go", one.id)}>
              {tf("auto.act.nextGo", { step: one.name })}
            </option>
          ))}
        <option value="done">{t("auto.pic.endsDone")}</option>
        <option value="halt">{t("auto.pic.endsHalt")}</option>
      </select>

      {!isError && (
        <>
          <button type="button" className="btn autostep__outadd" onClick={onAddOutput}>
            {t("auto.step.outputAdd")}
          </button>
          <button
            type="button"
            className="btn"
            onClick={() => void run(removeAutomationExit("step", step.id, was))}
          >
            {t("auto.step.remove")}
          </button>
        </>
      )}
    </li>
  );
}

/** One input of a step: how it is declared, and what the step before it hands into it. */
function InputRow({
  action,
  step,
  input,
  run,
}: {
  action: AutomationActionDetailDto;
  step: AutomationStepDto;
  input: AutomationPortDto;
  run: Run;
}) {
  const graph = actionGraph(action)!;
  const now = wireInto(graph, step.id, input.name);
  const choices = wireChoices(graph, step.id, input);
  const picked = now === undefined ? "" : choiceKey(now.fromId, now.fromExitName, now.fromPortName);
  return (
    <div className="autostep__wire">
      <DeclEdit
        label={t("auto.step.inputs")}
        name={input.name}
        kind={input.kind}
        kinds={choicesOfKinds(PORT_KINDS)}
        required={input.required}
        onRename={(to) => void run(editAutomationInput("step", step.id, input.name, { name: to }))}
        onKind={(to) =>
          void run(
            editAutomationInput("step", step.id, input.name, {
              kind: to as AutomationPortDto["kind"],
            }),
          )
        }
        onRequired={(to) =>
          void run(editAutomationInput("step", step.id, input.name, { required: to }))
        }
        onRemove={() => void run(removeAutomationInput("step", step.id, input.name))}
      />
      <select
        aria-label={input.name}
        value={picked}
        onChange={(e) => {
          const chosen = choices.find((one) => one.key === e.target.value);
          if (chosen === undefined) {
            if (now !== undefined) void run(clearAutomationWire(now.id));
            return;
          }
          void run(
            setAutomationWire(
              "action",
              { boxId: chosen.boxId, exitName: chosen.exitName, portName: chosen.portName },
              { boxId: step.id, portName: input.name },
            ),
          );
        }}
      >
        <option value="">{t("auto.step.unwired")}</option>
        {choices.map((one) => (
          <option key={one.key} value={one.key}>
            {`${one.boxName} · ${exitLabel(one.exitName)} · ${one.portName}`}
          </option>
        ))}
      </select>
    </div>
  );
}

export function AutomationActionStepPanel({
  action,
  stepId,
  projectId,
  onRemoved,
}: {
  action: AutomationActionDetailDto | null;
  /** The step the picture is showing as pressed, or nothing while none is. */
  stepId: number | null;
  projectId: number | null;
  /** The panel has nothing left to draw once its step is gone. */
  onRemoved: () => void;
}) {
  const step = action?.steps.find((one) => one.id === stepId) ?? null;
  const agents = useAgents(projectId);
  const models = useModels(step?.agent ?? "");
  const [name, setName] = useDraft(step?.name ?? "");
  const [prompt, setPrompt] = useDraft(step?.prompt ?? "");
  // The way out an output artefact is being declared on, while that dialog is open.
  const [adding, setAdding] = useState<number | null>(null);
  const [refused, setRefused] = useState<string | null>(null);

  const run: Run = (write) => {
    setRefused(null);
    return Promise.resolve(write)
      .then(() => true)
      .catch((e: unknown) => {
        setRefused(errText(e));
        return false;
      });
  };

  if (action === null || step === null) {
    return <div className="auto__empty">{t("auto.act.stepNone")}</div>;
  }

  const isEntry = action.entryStepId === step.id;
  // Where the working folder may be taken from: what the action is answered with where it is placed,
  // and what reaches this step from inside. A name, never a path — the answer is written on the
  // placement.
  const folderNames = [
    ...action.settings.filter((one) => one.kind === "folder").map((one) => one.name),
    ...action.inputs.filter((one) => one.kind === "file").map((one) => one.name),
    ...step.inputs.filter((one) => one.kind === "file").map((one) => one.name),
  ];

  // Physical, and it takes what the step declared and every line naming it, so the confirm comes
  // before the write.
  const remove = async () => {
    if (!(await confirmDialog(t("auto.act.stepRemoveConfirm")))) return;
    if (await run(removeAutomationStep(step.id))) onRemoved();
  };

  return (
    <div className="autostep">
      {refused !== null && <ErrorNote tone="quiet">{refused}</ErrorNote>}

      <label className="autostep__field">
        <span className="autostep__label">{t("auto.step.name")}</span>
        <input
          value={name}
          onChange={(e) => setName(e.target.value)}
          onBlur={() => name !== step.name && void run(editAutomationStep(step.id, { name }))}
        />
      </label>

      <div className="autostep__field">
        <span className="autostep__label">{t("auto.act.entry")}</span>
        {isEntry ? (
          <span className="autostep__said">{t("auto.act.entryIs")}</span>
        ) : (
          <button
            type="button"
            className="btn"
            onClick={() => void run(setAutomationActionEntry(action.id, step.id))}
          >
            {t("auto.act.entrySet")}
          </button>
        )}
      </div>

      <label className="autostep__field">
        <span className="autostep__label">{t("auto.step.prompt")}</span>
        <textarea
          className="autostep__prompt"
          rows={6}
          value={prompt}
          onChange={(e) => setPrompt(e.target.value)}
          onBlur={() => prompt !== step.prompt && void run(editAutomationStep(step.id, { prompt }))}
        />
      </label>

      <div className="autostep__field">
        <span className="autostep__label">{t("auto.step.inputs")}</span>
        {step.inputs.length === 0 && (
          <span className="autostep__said">{t("auto.step.declaresNone")}</span>
        )}
        {step.inputs.map((input) => (
          <InputRow key={input.name} action={action} step={step} input={input} run={run} />
        ))}
        <DeclareRow
          what={t("auto.step.inputName")}
          kinds={choicesOfKinds(PORT_KINDS)}
          onAdd={(declared, kind) =>
            run(
              declareAutomationInput("step", step.id, {
                name: declared,
                kind: kind as AutomationPortDto["kind"],
              }),
            )
          }
        />
      </div>

      <div className="autostep__field">
        <span className="autostep__label">{t("auto.step.exits")}</span>
        <ul className="autostep__exits">
          {step.exits.map((one) => (
            <ExitRow
              key={one.id}
              action={action}
              step={step}
              exit={one}
              onAddOutput={() => setAdding(one.id)}
              run={run}
            />
          ))}
        </ul>
        <DeclareRow
          what={t("auto.step.exitName")}
          kinds={null}
          onAdd={(declared) => run(declareAutomationExit("step", step.id, declared))}
        />
      </div>

      <label className="autostep__field">
        <span className="autostep__label">{t("auto.step.agent")}</span>
        <select
          value={step.agent}
          onChange={(e) => void run(editAutomationStep(step.id, { agent: e.target.value }))}
        >
          {agents.every((one) => one.id !== step.agent) && (
            <option value={step.agent}>{step.agent}</option>
          )}
          {agents.map((one) => (
            <option key={one.id} value={one.id} disabled={!one.installed}>
              {one.installed ? one.label : tf("auto.step.notHere", { agent: one.label })}
            </option>
          ))}
        </select>
      </label>

      <label className="autostep__field">
        <span className="autostep__label">{t("auto.step.model")}</span>
        <select
          value={step.model ?? ""}
          onChange={(e) =>
            void run(
              editAutomationStep(step.id, { model: e.target.value === "" ? null : e.target.value }),
            )
          }
        >
          <option value="">{t("auto.step.modelDefault")}</option>
          {step.model !== undefined &&
            (models?.models ?? []).every((one) => one.id !== step.model) && (
              <option value={step.model}>{step.model}</option>
            )}
          {(models?.models ?? []).map((one) => (
            <option key={one.id} value={one.id}>
              {one.label}
            </option>
          ))}
        </select>
      </label>

      <label className="autostep__field">
        <span className="autostep__label">{t("auto.step.folder")}</span>
        <select
          value={step.workDirRef ?? ""}
          onChange={(e) =>
            void run(
              editAutomationStep(step.id, {
                workDir: e.target.value === "" ? null : e.target.value,
              }),
            )
          }
        >
          <option value="">{t("auto.step.folderNone")}</option>
          {step.workDirRef !== undefined && !folderNames.includes(step.workDirRef) && (
            <option value={step.workDirRef}>{step.workDirRef}</option>
          )}
          {folderNames.map((one) => (
            <option key={one} value={one}>
              {one}
            </option>
          ))}
        </select>
      </label>

      <label className="autostep__check">
        <input
          type="checkbox"
          checked={step.interactive}
          onChange={(e) => void run(editAutomationStep(step.id, { interactive: e.target.checked }))}
        />
        {t("auto.step.interactive")}
      </label>

      <label className="autostep__check">
        <input
          type="checkbox"
          checked={step.reportToTask}
          onChange={(e) =>
            void run(editAutomationStep(step.id, { reportToTask: e.target.checked }))
          }
        />
        {t("auto.step.reportToTask")}
      </label>

      <label className="autostep__check">
        <input
          type="checkbox"
          checked={step.showHistory}
          onChange={(e) => void run(editAutomationStep(step.id, { history: e.target.checked }))}
        />
        {t("auto.step.history")}
      </label>

      <div className="settings__row">
        <button type="button" className="btn btn--danger" onClick={() => void remove()}>
          {t("auto.act.stepRemove")}
        </button>
      </div>

      {adding !== null && (
        <AutomationOutputAdd
          exit={step.exits.find((one) => one.id === adding)!}
          onClose={() => setAdding(null)}
        />
      )}
    </div>
  );
}
