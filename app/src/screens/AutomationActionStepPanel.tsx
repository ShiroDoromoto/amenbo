// What the pressed step holds, on the action build screen (`AMB-T-5315`).
//
// **It is the step's own panel, not a placement's.** A box inside an action is a step, and a step is
// one terminal: the prompt it runs on, what it is handed and how it runs are its own. Who carries it
// out is not — that is chosen where the action is placed, step by step (`./AutomationStepPanel`,
// `AMB-D-960`), since the same action may be run by different models on two pictures. What the action
// declares to the automations placing it is the other panel's (`./AutomationActionDeclaresPanel`), and nothing here writes on that layer.
//
// **What comes after a way out is written here too.** Inside an action the line and the box it
// leaves are read together — one way out decides one thing — so the way out's row carries the
// pulldown that says where the run goes next, rather than sending a reader to the picture to draw it.
//
// **A wire is picked from a list, not drawn** (`./automationWires`), the way it is on an automation,
// and what does not fit is not offered.
//
// **What each part is, is said by its shape, not by a sentence under it** (`AMB-T-5522`): the name is
// the panel's head (`./AutomationActionBuildScreen`), the entry is a switch, a declaration is a chip
// with its "⋯", and a way out is a card (`./automationDeclParts`).
//
// **A refusal is drawn, once, at the top**, for `./AutomationStepPanel`'s reason: every press here
// can be refused by core — a name already taken, the error way out, a line that leaves the picture —
// and the last refusal stands where the reader is looking.
import { useState } from "react";
import {
  clearAutomationWire,
  declareAutomationExit,
  declareAutomationInput,
  editAutomationInput,
  editAutomationStep,
  removeAutomationInput,
  removeAutomationStep,
  setAutomationActionEntry,
  setAutomationWire,
} from "../core/automations";
import { confirmDialog } from "../core/dialog";
import { errText, t } from "../core/i18n";
import { ErrorNote } from "../components/ErrorNote";
import { ACTION_BOUNDARY, actionGraph, ERROR_EXIT } from "./automationLayout";
import { DeclEdit, choicesOfKinds, exitLabel, NextRow, useDraft, type Run } from "./automationPanel";
import { DeclItem, DeclSec, ExitEdit, OutputPlus, PortChip, Sec, Switch } from "./automationDeclParts";
import { ExitMark } from "./automationParts";
import { choiceKey, wireChoices, wireInto } from "./automationWires";
import { AutomationOutputAdd } from "./AutomationOutputAdd";
import { PORT_KINDS } from "./automationPortKinds";
import type {
  AutomationActionDetailDto,
  AutomationExitDto,
  AutomationPortDto,
  AutomationStepDto,
} from "../bindings/bindings";

/**
 * One way out of a step, as a card: its mark, what leaving by it hands on with the "＋" that adds one
 * more, and where the run goes next.
 *
 * **The error way out has a card too**, with the pulldown alone: it is carried from birth and neither
 * renamed nor removed (core refuses both), while what follows it is exactly what a builder may want to
 * say — with nothing said, it stops the run and calls a person.
 */
function ExitCard({
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
  const isError = exit.name === ERROR_EXIT;
  return (
    <DeclItem
      className={isError ? "autoexit autoexit--error" : "autoexit"}
      edit={isError ? undefined : <ExitEdit owner="step" ownerId={step.id} exit={exit} run={run} />}
      below={
        <NextRow
          graph={actionGraph(action)!}
          picture="action"
          boxId={step.id}
          exitName={exit.name}
          arrow
          run={run}
        />
      }
    >
      <ExitMark name={exit.name} />
      {exit.outputs.map((port) => (
        <PortChip key={port.name} port={port} />
      ))}
      {!isError && <OutputPlus onPress={onAddOutput} />}
    </DeclItem>
  );
}

/**
 * One thing a step takes in, read the way the automation's panel reads a way out: where it comes from,
 * an arrow, and what it is.
 */
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
  const choices = wireChoices(graph, step.id, input, t("auto.pic.actionSelf"));
  const picked = now === undefined ? "" : choiceKey(now.fromId, now.fromExitName, now.fromPortName);
  return (
    <DeclItem
      edit={
        <DeclEdit
          label={t("auto.decl.inputs")}
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
      }
    >
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
            {/* What the action was handed comes in from the action itself, which leaves by no way
                out — so it is named without one. */}
            {one.boxId === ACTION_BOUNDARY
              ? `${one.boxName} · ${one.portName}`
              : `${one.boxName} · ${exitLabel(one.exitName)} · ${one.portName}`}
          </option>
        ))}
      </select>
      <span className="autodecl__arrow" aria-hidden="true">→</span>
      <PortChip port={input} />
    </DeclItem>
  );
}

/**
 * What a step is handed besides its prompt, as four toggles — each one a separate flag on the step.
 * Pressed is handed.
 */
function GiveToggles({ step, run }: { step: AutomationStepDto; run: Run }) {
  const toggles: { on: boolean; label: string; write: (to: boolean) => Parameters<typeof editAutomationStep>[1] }[] = [
    { on: step.showHistory, label: t("auto.give.history"), write: (to) => ({ history: to }) },
    { on: step.showNotes, label: t("auto.give.taskNotes"), write: (to) => ({ taskNotes: to }) },
    { on: step.showDecisions, label: t("auto.give.taskDecisions"), write: (to) => ({ taskDecisions: to }) },
    { on: step.showComments, label: t("auto.give.taskComments"), write: (to) => ({ taskComments: to }) },
  ];
  return (
    <div className="autogive">
      {toggles.map((one) => (
        <button
          key={one.label}
          type="button"
          className={one.on ? "autogive__tog autogive__tog--on" : "autogive__tog"}
          aria-pressed={one.on}
          onClick={() => void run(editAutomationStep(step.id, one.write(!one.on)))}
        >
          {one.label}
        </button>
      ))}
    </div>
  );
}

export function AutomationActionStepPanel({
  action,
  stepId,
  onRemoved,
}: {
  action: AutomationActionDetailDto | null;
  /** The step the picture is showing as pressed, or nothing while none is. */
  stepId: number | null;
  /** The panel has nothing left to draw once its step is gone. */
  onRemoved: () => void;
}) {
  const step = action?.steps.find((one) => one.id === stepId) ?? null;
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

      {/* An action opens at one step, so the switch is turned on at the step that is to take over and
          never off: the picture's entry mark moving is what says there is only one. */}
      <Switch
        boxed
        label={t("auto.act.entrySwitch")}
        checked={isEntry}
        disabled={isEntry}
        onChange={(to) => to && void run(setAutomationActionEntry(action.id, step.id))}
      />

      <Sec title={t("auto.step.prompt")}>
        <textarea
          className="autostep__prompt"
          aria-label={t("auto.step.prompt")}
          rows={6}
          value={prompt}
          onChange={(e) => setPrompt(e.target.value)}
          onBlur={() => prompt !== step.prompt && void run(editAutomationStep(step.id, { prompt }))}
        />
      </Sec>

      <DeclSec
        title={t("auto.decl.inputs")}
        what={t("auto.decl.inputName")}
        kinds={choicesOfKinds(PORT_KINDS)}
        onAdd={(declared, kind) =>
          run(
            declareAutomationInput("step", step.id, {
              name: declared,
              kind: kind as AutomationPortDto["kind"],
            }),
          )
        }
      >
        {step.inputs.map((input) => (
          <InputRow key={input.name} action={action} step={step} input={input} run={run} />
        ))}
      </DeclSec>

      <DeclSec
        title={t("auto.step.exits")}
        what={t("auto.step.exitName")}
        kinds={null}
        onAdd={(declared) => run(declareAutomationExit("step", step.id, declared))}
      >
        {/* The error way out last, as the action's own list has it: it is the one nobody named, and
            between two named ones it reads as one of them. */}
        {[...step.exits]
          .sort((a, b) => Number(a.name === ERROR_EXIT) - Number(b.name === ERROR_EXIT))
          .map((one) => (
            <ExitCard
              key={one.id}
              action={action}
              step={step}
              exit={one}
              onAddOutput={() => setAdding(one.id)}
              run={run}
            />
          ))}
      </DeclSec>

      <Sec title={t("auto.give.title")}>
        <GiveToggles step={step} run={run} />
      </Sec>

      <Sec title={t("auto.step.howRuns")}>
        <label className="autostep__pair">
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
        <Switch
          label={t("auto.step.interactive")}
          checked={step.interactive}
          onChange={(to) => void run(editAutomationStep(step.id, { interactive: to }))}
        />
        {/* What a closed task gets instead is said where it happens — the run's history — rather
            than under this switch (`AMB-D-963`). */}
        <Switch
          label={t("auto.step.reportToTask")}
          checked={step.reportToTask}
          onChange={(to) => void run(editAutomationStep(step.id, { reportToTask: to }))}
        />
      </Sec>

      <div className="actpanel__foot">
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
