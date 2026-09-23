// What the action declares to the automations that place it, on the action build screen
// (`AMB-T-5315`) — drawn as two panels, the action's input and its output (`AMB-T-5369`): what it
// takes in and the settings it asks for, opened from the frame over the picture, and the ways out it
// is left by with what each hands on, opened from the frame under it. The screen reads the steps as
// running from the one to the other, so the two halves stand where the picture begins and ends.
//
// **This is the outside of the action**, and the panel beside it is the inside
// (`./AutomationActionStepPanel`). A way out declared here is one a placement of this action can be
// left by; an input is what a placement takes in; a setting is answered where the action is placed,
// and never here — the row an action carries is the declaration alone (`AMB-D-949`).
//
// **A choice list is written one per line**: a choice is a label a person reads on a pulldown, and
// any separator this could take is a character that belongs inside one.
//
// **This is the one place a declaration is written** (`AMB-D-954`). The panel a placement opens on
// an automation's picture reads what is declared here and answers it (`./AutomationStepPanel`), and
// writes none of it: a rewrite here reaches every placement of the action, and a second place to
// make it would hide that.
//
// **What a declaration is answered with is not drawn here at all.** There is nothing to answer until
// the action stands somewhere, and a control that took an answer would be writing on a placement
// this screen cannot see.
import { useState } from "react";
import {
  declareAutomationCfg,
  declareAutomationExit,
  declareAutomationInput,
  editAutomationCfg,
  editAutomationInput,
  removeAutomationCfg,
  removeAutomationExit,
  removeAutomationInput,
  renameAutomationExit,
  clearAutomationWire,
  setAutomationWire,
  type CfgKind,
} from "../core/automations";
import { t, tf } from "../core/i18n";
import { ACTION_BOUNDARY, actionGraph, ERROR_EXIT } from "./automationLayout";
import {
  CFG_KINDS,
  choicesOfKinds,
  DeclareRow,
  DeclEdit,
  exitLabel,
  useDraft,
  type Run,
} from "./automationPanel";
import { AutomationOutputAdd } from "./AutomationOutputAdd";
import { kindLabel, PORT_KINDS } from "./automationPortKinds";
import { boundaryChoices, choiceKey, wireOutOf } from "./automationWires";
import type {
  AutomationActionDetailDto,
  AutomationCfgDto,
  AutomationExitDto,
  AutomationPortDto,
} from "../bindings/bindings";

/** The choices a `choice` setting was declared with. One that was not readable offers none. */
function choicesOf(options: string | undefined): string[] {
  if (options === undefined) return [];
  try {
    const one = JSON.parse(options) as unknown;
    return Array.isArray(one) ? one.filter((v): v is string => typeof v === "string") : [];
  } catch {
    return [];
  }
}

/** The choice list as a reader writes it: one per line. */
function writeChoices(text: string): string | null {
  const choices = text.split("\n").map((one) => one.trim()).filter((one) => one !== "");
  return choices.length === 0 ? null : JSON.stringify(choices);
}

/**
 * One thing a way out of the action hands on, and which step's output fills it.
 *
 * **Only the steps that leave by this way out are offered** — core joins an output of the action to
 * the step it returns from, and a step that never leaves by it has nothing to hand out through it.
 */
function OutputRow({
  action,
  exitName,
  port,
  run,
}: {
  action: AutomationActionDetailDto;
  exitName: string | undefined;
  port: AutomationPortDto;
  run: Run;
}) {
  const graph = actionGraph(action)!;
  const now = wireOutOf(graph, exitName, port.name);
  const choices = boundaryChoices(graph, exitName, port);
  const picked = now === undefined ? "" : choiceKey(now.fromId, now.fromExitName, now.fromPortName);
  return (
    <div className="autostep__outline">
      <span className="autostep__out">
        {port.name}
        <span className="autostep__outkind">{kindLabel(port.kind)}</span>
      </span>
      <span aria-hidden="true">←</span>
      <select
        aria-label={port.name}
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
              { boxId: ACTION_BOUNDARY, portName: port.name },
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

/** One way out of the action: what it is called, and what leaving by it hands on. */
function ExitRow({
  action,
  exit,
  onAddOutput,
  run,
}: {
  action: AutomationActionDetailDto;
  exit: AutomationExitDto;
  onAddOutput: () => void;
  run: Run;
}) {
  const actionId = action.id;
  const [name, setName] = useDraft(exit.name ?? "");
  const was = exit.name ?? null;
  return (
    <li className="autostep__exit">
      <div className="autostep__exithead">
        <input
          className="autostep__declname"
          placeholder={t("auto.step.exitUnnamed")}
          aria-label={t("auto.step.exits")}
          value={name}
          onChange={(e) => setName(e.target.value)}
          onBlur={() => {
            const now = name.trim() === "" ? null : name.trim();
            if (now !== was) void run(renameAutomationExit("action", actionId, was, now));
          }}
        />
        <button type="button" className="btn autostep__outadd" onClick={onAddOutput}>
          {t("auto.step.outputAdd")}
        </button>
        <button
          type="button"
          className="btn"
          onClick={() => void run(removeAutomationExit("action", actionId, was))}
        >
          {t("auto.step.remove")}
        </button>
      </div>
      {exit.outputs.map((port) => (
        <OutputRow key={port.name} action={action} exitName={exit.name} port={port} run={run} />
      ))}
    </li>
  );
}

/** One setting the action declares — its name, the answer it takes, and the choices where it has any. */
function CfgRow({
  actionId,
  cfg,
  run,
}: {
  actionId: number;
  cfg: AutomationCfgDto;
  run: Run;
}) {
  const [choiceText, setChoiceText] = useDraft(choicesOf(cfg.options).join("\n"));
  return (
    <div className="autostep__cfg">
      <DeclEdit
        label={t("auto.step.cfg")}
        name={cfg.name}
        kind={cfg.kind}
        kinds={choicesOfKinds(CFG_KINDS)}
        required={cfg.required}
        onRename={(to) => void run(editAutomationCfg(actionId, cfg.name, { name: to }))}
        // A choice list belongs to a choice and to nothing else, so the move takes it with it in the
        // one call — core refuses a list left behind on a kind that would never show it.
        onKind={(to) =>
          void run(
            editAutomationCfg(actionId, cfg.name, {
              kind: to as CfgKind,
              ...(to === "choice" ? {} : { options: null }),
            }),
          )
        }
        onRequired={(to) => void run(editAutomationCfg(actionId, cfg.name, { required: to }))}
        onRemove={() => void run(removeAutomationCfg(actionId, cfg.name))}
      />
      {cfg.kind === "choice" && (
        <label className="autostep__field">
          <span className="autostep__label">{t("auto.step.choices")}</span>
          <textarea
            rows={2}
            value={choiceText}
            onChange={(e) => setChoiceText(e.target.value)}
            onBlur={() =>
              writeChoices(choiceText) !== (cfg.options ?? null) &&
              void run(editAutomationCfg(actionId, cfg.name, { options: writeChoices(choiceText) }))
            }
          />
        </label>
      )}
    </div>
  );
}

export function AutomationActionDeclaresPanel({
  action,
  part,
  run,
}: {
  action: AutomationActionDetailDto;
  /** Which half: what the action takes in and asks for, or the ways out it is left by. */
  part: "in" | "out";
  /** The screen's one runner, so a refusal lands where every other one does. */
  run: Run;
}) {
  // The way out an output artefact is being declared on, while that dialog is open.
  const [adding, setAdding] = useState<number | null>(null);

  if (part === "in") {
    return (
      <div className="autostep">
        <div className="autostep__field">
          <span className="autostep__label">{t("auto.step.inputs")}</span>
          {action.inputs.length === 0 && (
            <span className="autostep__said">{t("auto.step.declaresNone")}</span>
          )}
          {action.inputs.map((input) => (
            <DeclEdit
              key={input.name}
              label={t("auto.step.inputs")}
              name={input.name}
              kind={input.kind}
              kinds={choicesOfKinds(PORT_KINDS)}
              required={input.required}
              onRename={(to) =>
                void run(editAutomationInput("action", action.id, input.name, { name: to }))
              }
              onKind={(to) =>
                void run(
                  editAutomationInput("action", action.id, input.name, {
                    kind: to as AutomationPortDto["kind"],
                  }),
                )
              }
              onRequired={(to) =>
                void run(editAutomationInput("action", action.id, input.name, { required: to }))
              }
              onRemove={() => void run(removeAutomationInput("action", action.id, input.name))}
            />
          ))}
          <DeclareRow
            what={t("auto.step.inputName")}
            kinds={choicesOfKinds(PORT_KINDS)}
            onAdd={(declared, kind) =>
              run(
                declareAutomationInput("action", action.id, {
                  name: declared,
                  kind: kind as AutomationPortDto["kind"],
                }),
              )
            }
          />
        </div>

        <div className="autostep__field">
          <span className="autostep__label">{t("auto.step.cfg")}</span>
          <span className="autostep__said">{tf("auto.act.declaresWhat", { name: action.name })}</span>
          {action.settings.length === 0 && (
            <span className="autostep__said">{t("auto.step.declaresNone")}</span>
          )}
          {action.settings.map((cfg) => (
            <CfgRow key={cfg.name} actionId={action.id} cfg={cfg} run={run} />
          ))}
          <DeclareRow
            what={t("auto.step.cfgName")}
            kinds={choicesOfKinds(CFG_KINDS)}
            onAdd={(declared, kind) =>
              run(declareAutomationCfg(action.id, { name: declared, kind: kind as CfgKind }))
            }
          />
        </div>
      </div>
    );
  }

  return (
    <div className="autostep">
      <div className="autostep__field">
        <span className="autostep__label">{t("auto.step.exits")}</span>
        <ul className="autostep__exits">
          {action.exits
            .filter((one) => one.name !== ERROR_EXIT)
            .map((one) => (
              <ExitRow
                key={one.id}
                action={action}
                exit={one}
                onAddOutput={() => setAdding(one.id)}
                run={run}
              />
            ))}
          {/* The error way out, always drawn and always last: every action carries one, and a list
              that left it off would read as an action that cannot fail. Nothing here renames or
              removes it, which core refuses either way. */}
          <li className="autostep__exiterr">{t("auto.pic.errorExit")}</li>
        </ul>
        <DeclareRow
          what={t("auto.step.exitName")}
          kinds={null}
          onAdd={(declared) => run(declareAutomationExit("action", action.id, declared))}
        />
      </div>

      {adding !== null && (
        <AutomationOutputAdd
          exit={action.exits.find((one) => one.id === adding)!}
          onClose={() => setAdding(null)}
        />
      )}
    </div>
  );
}
