// What the pressed spot holds — the build screen's "placement" place, drawn in the panel pinned to the
// picture's right (`AMB-T-5256`, `AMB-T-5282`, `AMB-T-5360`).
//
// **It holds what is this placement's own, and reads the rest** (`AMB-D-954`, `AMB-T-5374`). A box on
// the picture is a placement of a library action. What that action declares — its ways out, what
// each hands on, its inputs, its settings — and what its steps carry — the prompt, the agent, the
// model — are the action's, and are written on the action's own screen, where a rewrite reaches every
// placement of it. So here they are read, and the panel says where they are written, with the press
// that goes there. Written in two places, a reader would not know which one they were changing, nor
// that a rewrite on one picture reached another.
//
// **What is written here is this spot's alone**: the answer a setting takes, the wire into an input,
// what happens after each way out, and whether a run opens here. One action placed on two pictures
// answers, is wired and goes on differently on each.
//
// **It stands where it stands, whatever the picture does.** A definition of forty boxes draws a
// picture two thousand pixels tall, and a panel that followed the box a reader pressed would put the
// contents off the bottom of the screen — so the panel is pinned beside the picture
// (`./AutomationBuildScreen`) and this one is always in the same spot.
//
// **Every field writes on the spot.** There is no Save: what a reader changed is what the definition
// now says, and a panel with a button would leave a spot half-edited every time somebody pressed
// another box in the picture. A box of text writes when the caret leaves it.
//
// **Taking the spot off is here too**, because this is what a reader has in front of them when they
// decide against it. It leaves the library action where it is.
//
// **A setting is answered by the control its kind takes** — never by writing a filter expression. The
// shape each one is kept in is `./automationCfg`'s.
//
// **A wire is picked from a list, not drawn** (`./automationWires`), and what does not fit is not
// offered.
//
// **A refusal is drawn, once, at the top**, whichever write it came from.
import { useState } from "react";
import {
  answerAutomationCfg,
  clearAutomationWire,
  removeAutomationPlacement,
  setAutomationEntry,
  setAutomationWire,
  useAutomationAction,
} from "../core/automations";
import { confirmDialog } from "../core/dialog";
import { errText, isStatus, statusLabel, t, tn } from "../core/i18n";
import { ErrorNote } from "../components/ErrorNote";
import { ReachChip } from "./AutomationActionsTab";
import { automationGraph, ERROR_EXIT } from "./automationLayout";
import { CFG_KINDS, exitLabel, NextRow, useDraft, type Run } from "./automationPanel";
import {
  FILTER_ROWS,
  pressed,
  readFilter,
  readNumber,
  readText,
  writeFilter,
  writeNumber,
  writeText,
  type TaskFilter,
} from "./automationCfg";
import { choiceKey, wireChoices, wireInto } from "./automationWires";
import { kindLabel } from "./automationPortKinds";
import type {
  AutomationCfgDto,
  AutomationDetailDto,
  AutomationPlacementDto,
  AutomationPortDto,
} from "../bindings/bindings";

/** What one value of a task filter row is called. */
function rowValueLabel(key: string, value: string): string {
  if (key === "status") return isStatus(value) ? statusLabel(value) : value;
  if (key === "assignee") {
    if (value === "none") return t("filter.opt.assignee.none");
    if (value === "me") return t("filter.opt.assignee.me");
    return t("filter.opt.assignee.meAi");
  }
  return value === "yes" ? t("auto.step.readyYes") : t("auto.step.readyNo");
}

/** What one task filter row is called. */
function rowLabel(key: string): string {
  if (key === "status") return t("filter.dim.status");
  if (key === "assignee") return t("filter.dim.assignee");
  return t("auto.step.ready");
}

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

/** A declaration read: its name, what it carries, and whether the step is refused without it. */
function DeclChip({ name, kind, required, tone }: { name: string; kind: string; required: boolean; tone: string }) {
  return (
    <span className={`actport actport--${tone}`}>
      {name}
      <span className="actport__kind">
        {kind}
        {required && `・${t("auto.step.required")}`}
      </span>
    </span>
  );
}

/** One setting: what the action declares, read, and the control its kind takes for this spot's answer. */
function CfgRow({ placementId, cfg, run }: { placementId: number; cfg: AutomationCfgDto; run: Run }) {
  const answer = (value: string | null) =>
    void run(answerAutomationCfg(placementId, cfg.name, value));
  const [text, setText] = useDraft(cfg.kind === "number" ? "" : readText(cfg.value));
  const [number, setNumber] = useDraft(cfg.kind === "number" ? String(readNumber(cfg.value) ?? "") : "");
  const filter: TaskFilter = readFilter(cfg.value);
  const choices = choicesOf(cfg.options);

  return (
    <div className="autostep__cfg">
      <div>
        <DeclChip
          name={cfg.name}
          kind={CFG_KINDS.find((one) => one.id === cfg.kind)?.label() ?? cfg.kind}
          required={cfg.required}
          tone="cfg"
        />
      </div>

      {cfg.kind === "taskfilter" && (
        <div className="autostep__rows">
          {FILTER_ROWS.map((row) => (
            <div key={row.key} className="autostep__row">
              <span className="autostep__rowname">{rowLabel(row.key)}</span>
              {row.values.map((value) => (
                <button
                  key={value}
                  type="button"
                  className={`autostep__chip ${(filter[row.key] ?? []).includes(value) ? "autostep__chip--on" : ""}`}
                  aria-pressed={(filter[row.key] ?? []).includes(value)}
                  onClick={() => answer(writeFilter(pressed(filter, row.key, value, row.single)))}
                >
                  {rowValueLabel(row.key, value)}
                </button>
              ))}
            </div>
          ))}
        </div>
      )}

      {cfg.kind === "choice" && (
        <select
          aria-label={cfg.name}
          value={readText(cfg.value)}
          onChange={(e) => answer(writeText(e.target.value))}
        >
          <option value="">—</option>
          {choices.map((one) => (
            <option key={one} value={one}>
              {one}
            </option>
          ))}
        </select>
      )}

      {cfg.kind === "number" && (
        <input
          type="number"
          aria-label={cfg.name}
          value={number}
          onChange={(e) => setNumber(e.target.value)}
          onBlur={() => writeNumber(number) !== (cfg.value ?? null) && answer(writeNumber(number))}
        />
      )}

      {(cfg.kind === "text" || cfg.kind === "folder") && (
        <input
          aria-label={cfg.name}
          value={text}
          onChange={(e) => setText(e.target.value)}
          onBlur={() => writeText(text) !== (cfg.value ?? null) && answer(writeText(text))}
        />
      )}
    </div>
  );
}

/** One input: what the action declares, read, and what is wired into it at this spot. */
function InputRow({
  automation,
  placement,
  input,
  run,
}: {
  automation: AutomationDetailDto;
  placement: AutomationPlacementDto;
  input: AutomationPortDto;
  run: Run;
}) {
  const graph = automationGraph(automation)!;
  const now = wireInto(graph, placement.id, input.name);
  const choices = wireChoices(graph, placement.id, input);
  const picked = now === undefined ? "" : choiceKey(now.fromId, now.fromExitName, now.fromPortName);
  return (
    <div className="autostep__wire">
      <div>
        <DeclChip name={input.name} kind={kindLabel(input.kind)} required={input.required} tone={input.kind} />
      </div>
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
              "automation",
              { boxId: chosen.boxId, exitName: chosen.exitName, portName: chosen.portName },
              { boxId: placement.id, portName: input.name },
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

export function AutomationStepPanel({
  automation,
  placementId,
  onRemoved,
  onOpenAction,
  readOnly = false,
}: {
  automation: AutomationDetailDto | null;
  /** The spot the picture is showing as pressed, or nothing while none is. */
  placementId: number | null;
  /** The spot this panel was drawn from is gone — there is nothing left for the picture to mark. */
  onRemoved: () => void;
  /** Go to the action's own build screen, where what it declares and what its steps carry are written. */
  onOpenAction: (actionId: number) => void;
  /**
   * Hold every write shut — a run is going on the automation (`AMB-D-961`). The press that goes to the
   * action stays live: it writes nothing, and it is where a reader goes to read what the steps carry.
   */
  readOnly?: boolean;
}) {
  const placement = automation?.placements.find((one) => one.id === placementId) ?? null;
  const action = useAutomationAction(placement?.actionId ?? null);
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

  if (automation === null || placement === null) {
    return <div className="auto__empty">{t("auto.step.none")}</div>;
  }

  const takesTask = placement.exits.some((exit) =>
    exit.outputs.some((port) => port.kind === "task_take"),
  );
  // Taking the spot away takes the answers written on it and every line naming it, so it asks first
  // — and what it leaves is the library action, which outlives any one picture. The panel is drawn
  // from that spot, so the screen is told to stop showing it.
  const remove = async () => {
    if (!(await confirmDialog(t("auto.step.placementRemoveConfirm")))) return;
    if (await run(removeAutomationPlacement(placement.id))) onRemoved();
  };
  const named = placement.exits.filter((one) => one.name !== ERROR_EXIT);

  return (
    <div className="autostep">
      {refused !== null && <ErrorNote tone="quiet">{refused}</ErrorNote>}

      <div className="autostep__field">
        <span className="autostep__label">{t("auto.place.action")}</span>
        <div className="autoplace__action">
          <div className="autoplace__actionhead">
            <span className="autoplace__actionname">{placement.name}</span>
            {action !== null && <ReachChip global={action.global} />}
          </div>
          {action !== null && action.note.trim() !== "" && (
            <div className="autoplace__note">{action.note}</div>
          )}
          {action !== null && action.steps.length === 0 && (
            <div className="autoplace__empty">{t("auto.place.empty")}</div>
          )}
          <div className="autoplace__meta">
            {action !== null && action.usedBy > 0 && <span>{tn("auto.actions.usedBy", action.usedBy)}</span>}
            <button type="button" className="btn" onClick={() => onOpenAction(placement.actionId)}>
              {t("auto.place.open")}
            </button>
          </div>
          <div className="autostep__said">{t("auto.place.ownedWhat")}</div>
        </div>
      </div>

      <fieldset className="autostep__writes" disabled={readOnly}>

        <label className="autostep__check">
          <input
            type="checkbox"
            checked={automation.entryPlacementId === placement.id}
            onChange={(e) =>
              void run(setAutomationEntry(automation.id, e.target.checked ? placement.id : null))
            }
          />
          {t("auto.step.entry")}
        </label>
        <div className="autostep__said">{t("auto.step.entryWhat")}</div>

        <div className="autostep__field">
          <span className="autostep__label">{t("auto.step.task")}</span>
          <span className="autostep__said">
            {takesTask ? t("auto.step.takesTask") : t("auto.step.carriesTask")}
          </span>
        </div>

        <div className="autostep__field">
          <span className="autostep__label">{t("auto.step.cfg")}</span>
          {placement.settings.length === 0 && (
            <span className="autostep__said">{t("auto.step.declaresNone")}</span>
          )}
          {placement.settings.map((cfg) => (
            <CfgRow key={cfg.name} placementId={placement.id} cfg={cfg} run={run} />
          ))}
        </div>

        <div className="autostep__field">
          <span className="autostep__label">{t("auto.step.inputs")}</span>
          {placement.inputs.length === 0 && (
            <span className="autostep__said">{t("auto.step.declaresNone")}</span>
          )}
          {placement.inputs.map((input) => (
            <InputRow key={input.name} automation={automation} placement={placement} input={input} run={run} />
          ))}
        </div>

        <div className="autostep__field">
          <span className="autostep__label">{t("auto.step.exits")}</span>
          <span className="autostep__said">{t("auto.place.exitsWhat")}</span>
          <ul className="autostep__exits">
            {named.map((one) => (
              <li key={one.id} className="autostep__exit">
                <div className="autostep__exithead">
                  <span className="actport actport--exit">
                    {one.name ?? t("auto.step.exitUnnamed")}
                    {one.outputs.length > 0 && (
                      <span className="actport__kind">{one.outputs.map((out) => out.name).join("・")}</span>
                    )}
                  </span>
                </div>
                <NextRow
                  graph={automationGraph(automation)!}
                  picture="automation"
                  boxId={placement.id}
                  exitName={one.name}
                  run={run}
                />
              </li>
            ))}
            {/* The error way out, always drawn and always last: every action carries one, and a list
                that left it off where nobody had said anything about it would read as a spot that
                cannot fail. Saying nothing after it stops the run and calls a person, and the row is
                where a picture says otherwise. */}
            <li className="autostep__exiterr">
              <span className="autostep__label">{t("auto.pic.errorExit")}</span>
              <NextRow
                graph={automationGraph(automation)!}
                picture="automation"
                boxId={placement.id}
                exitName={ERROR_EXIT}
                run={run}
              />
            </li>
          </ul>
        </div>

        <div className="settings__row">
          <button type="button" className="btn btn--danger" onClick={() => void remove()}>
            {t("auto.step.placementRemove")}
          </button>
        </div>
        <div className="autostep__said">{t("auto.step.placementRemoveWhat")}</div>
      </fieldset>
    </div>
  );
}
