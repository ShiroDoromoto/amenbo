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
// this screen cannot see. A setting's row keeps an empty dashed slot where the answer would be, so
// the shape says it is answered elsewhere rather than a sentence (`AMB-T-5522`).
import { useState } from "react";
import {
  declareAutomationCfg,
  declareAutomationExit,
  declareAutomationInput,
  editAutomationCfg,
  editAutomationInput,
  removeAutomationCfg,
  removeAutomationInput,
  clearAutomationWire,
  setAutomationWire,
  type CfgKind,
} from "../core/automations";
import { t } from "../core/i18n";
import { ACTION_BOUNDARY, actionGraph, ERROR_EXIT } from "./automationLayout";
import { CFG_KINDS, choicesOfKinds, DeclEdit, exitLabel, useDraft, type Run } from "./automationPanel";
import { DeclChip, DeclItem, DeclSec, ExitEdit, OutputPlus, PortChip } from "./automationDeclParts";
import { ExitMark } from "./automationParts";
import { AutomationOutputAdd } from "./AutomationOutputAdd";
import { PORT_KINDS } from "./automationPortKinds";
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
      <PortChip port={port} />
      <span className="autostep__arrow" aria-hidden="true">←</span>
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

/**
 * One way out of the action, as a card: its mark, what leaving by it hands on with the "＋" that adds
 * one more, and under it which step's output fills each of those.
 */
function ExitCard({
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
  return (
    <DeclItem
      className="autoexit"
      edit={<ExitEdit owner="action" ownerId={action.id} exit={exit} run={run} />}
      below={exit.outputs.map((port) => (
        <OutputRow key={port.name} action={action} exitName={exit.name} port={port} run={run} />
      ))}
    >
      <ExitMark name={exit.name} />
      <OutputPlus onPress={onAddOutput} />
    </DeclItem>
  );
}

/**
 * One setting the action declares, as a chip with the answer's place beside it left empty — the answer
 * is written where the action is placed, and the dashed slot says so by holding nothing. What renames,
 * re-kinds or removes it, and its choices, open on the "⋯".
 */
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
  const kindWord = CFG_KINDS.find((one) => one.id === cfg.kind)?.label() ?? cfg.kind;
  return (
    <DeclItem
      edit={
        <div className="autostep__cfg">
          <DeclEdit
            label={t("auto.step.cfg")}
            name={cfg.name}
            kind={cfg.kind}
            kinds={choicesOfKinds(CFG_KINDS)}
            required={cfg.required}
            onRename={(to) => void run(editAutomationCfg(actionId, cfg.name, { name: to }))}
            // A choice list belongs to a choice and to nothing else, so the move takes it with it in
            // the one call — core refuses a list left behind on a kind that would never show it.
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
                  void run(
                    editAutomationCfg(actionId, cfg.name, { options: writeChoices(choiceText) }),
                  )
                }
              />
            </label>
          )}
        </div>
      }
    >
      <DeclChip name={cfg.name} kind={kindWord} tone="cfg" required={cfg.required} />
      <span className="autodecl__slot">{t("auto.decl.answeredOnPlacement")}</span>
    </DeclItem>
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
        <DeclSec
          title={t("auto.decl.inputs")}
          what={t("auto.decl.inputName")}
          kinds={choicesOfKinds(PORT_KINDS)}
          onAdd={(declared, kind) =>
            run(
              declareAutomationInput("action", action.id, {
                name: declared,
                kind: kind as AutomationPortDto["kind"],
              }),
            )
          }
        >
          {action.inputs.map((input) => (
            <DeclItem
              key={input.name}
              edit={
                <DeclEdit
                  label={t("auto.decl.inputs")}
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
              }
            >
              <PortChip port={input} />
            </DeclItem>
          ))}
        </DeclSec>

        <DeclSec
          title={t("auto.step.cfg")}
          what={t("auto.step.cfgName")}
          kinds={choicesOfKinds(CFG_KINDS)}
          onAdd={(declared, kind) =>
            run(declareAutomationCfg(action.id, { name: declared, kind: kind as CfgKind }))
          }
        >
          {action.settings.map((cfg) => (
            <CfgRow key={cfg.name} actionId={action.id} cfg={cfg} run={run} />
          ))}
        </DeclSec>
      </div>
    );
  }

  return (
    <div className="autostep">
      <DeclSec
        title={t("auto.step.exits")}
        what={t("auto.step.exitName")}
        kinds={null}
        onAdd={(declared) => run(declareAutomationExit("action", action.id, declared))}
      >
        {action.exits
          .filter((one) => one.name !== ERROR_EXIT)
          .map((one) => (
            <ExitCard
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
        <DeclItem className="autoexit autoexit--error">
          <ExitMark name={ERROR_EXIT} />
        </DeclItem>
      </DeclSec>

      {adding !== null && (
        <AutomationOutputAdd
          exit={action.exits.find((one) => one.id === adding)!}
          onClose={() => setAdding(null)}
        />
      )}
    </div>
  );
}
