// What the pressed spot holds — the build screen's "placement" place, drawn in the panel pinned to the
// picture's right (`AMB-T-5256`, `AMB-T-5282`, `AMB-T-5360`).
//
// **It holds what is this placement's own, and reads the rest** (`AMB-D-954`, `AMB-T-5374`). A box on
// the picture is a placement of a library action. What that action declares — its ways out, what
// each hands on, its inputs, its settings — and what its steps carry — the prompt and the flags — are
// the action's, and are written on the action's own screen, where a rewrite reaches every placement
// of it. Written in two places, a reader would not know which one they were changing, nor that a
// rewrite on one picture reached another.
//
// **Which is which is told by the shape, not by a sentence** (`AMB-T-5521`). The action is one card
// with nothing to type in and a single press, "open the action", that goes to where it is written;
// what is this spot's own is a control. A way out is its mark, read, and the pulldown after the arrow
// is the one thing on the row to change. A family the action declares none of is not drawn at all,
// rather than drawn empty with a line saying so.
//
// **What is written here is this spot's alone**: the answer a setting takes, the wire into an input,
// what happens after each way out, and who carries out each step of the action (`AMB-D-960`). One action placed on two pictures answers, is wired, goes on and is run by
// different agents on each.
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
// decide against it. It leaves the library action where it is, which the confirmation says.
//
// **The spot a run starts at is changed here, not chosen** (`AMB-D-977`). The first thing placed is the
// start, one of the three built-ins a run can start at, and its panel offers the other two in its
// place. The lines out of it go with the one replaced, so the change asks first. While anything else
// stands on the picture the start is not taken off: a picture with the rest and no start could not
// be run, and would have no way to get one back.
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
  chooseAutomationAgent,
  clearAutomationWire,
  ENTRY_BUILTINS,
  removeAutomationPlacement,
  replaceAutomationEntry,
  setAutomationWire,
  useAutomationAction,
  useAutomationBuiltins,
} from "../core/automations";
import { confirmDialog } from "../core/dialog";
import { getSnapshot } from "../core/snapshot";
import { errText, t, tf } from "../core/i18n";
import { builtinShown, builtinWord } from "../core/builtinWords";
import { ErrorNote } from "../components/ErrorNote";
import { Icon } from "../components/Icon";
import { ExitMark, filterValueLabel, ReachChip, usedCount } from "./automationParts";
import { automationGraph, ERROR_EXIT, readAtLaunch } from "./automationLayout";
import { exitLabel, NextRow, useAgents, useDraft, useModels, type Run } from "./automationPanel";
import { Sec } from "./automationDeclParts";
import {
  dimRows,
  dimToken,
  DIM_KEY,
  filterRows,
  pressed,
  readFilter,
  readNumber,
  readSort,
  readText,
  sortChoices,
  writeFilter,
  writeNumber,
  writeText,
  type TaskFilter,
} from "./automationCfg";
import { choiceKey, wireChoices, wireInto } from "./automationWires";
import { kindLabel } from "./automationPortKinds";
import { AxisChips, CLASSIFY, ClassRows, makeTaskControl, NumberLines } from "./AutomationMakeTaskCfg";
import { useBoundFolders } from "../core/boundFolders";
import type {
  AutomationCfgDto,
  AutomationDetailDto,
  AutomationPlacementDto,
  AutomationPlacementStepDto,
  AutomationPortDto,
  WakeCandidateDto,
} from "../bindings/bindings";

/** What one task filter row is called. */
function rowLabel(key: string): string {
  if (key === "status") return t("filter.dim.status");
  if (key === "assignee") return t("filter.dim.assignee");
  return t("auto.step.ready");
}

/** One order a task filter can take its tasks in, in words. One written on the command line that the
 *  list has no words for is shown as `task list --sort` spells it. */
function sortLabel(sort: string): string {
  if (sort === "priority") return t("auto.step.sort.priority");
  if (sort === "due") return t("auto.step.sort.due");
  if (sort === "created") return t("auto.step.sort.created");
  return sort;
}

/**
 * **Which task the filter takes, said as a sentence with the order in it** — "[ highest priority
 * first ▾ ] …, and take the one on top". It carries no label: the rows above say which tasks are in,
 * and this line says what is done with them, so it reads as the end of that rather than one more row.
 * The sentence is the language's (`auto.step.sortTake`), so the list is put where the language puts
 * the order rather than always in front.
 */
function SortLine({ sort, disabled, onSort }: {
  sort: string;
  disabled: boolean;
  onSort: (sort: string) => void;
}) {
  const [before, after = ""] = tf("auto.step.sortTake", { sort: "\u0000" }).split("\u0000");
  return (
    <div className="autostep__row">
      {before !== "" && <span>{before}</span>}
      <select
        aria-label={tf("auto.step.sortTake", { sort: sortLabel(sort) })}
        value={sort}
        disabled={disabled}
        onChange={(e) => onSort(e.target.value)}
      >
        {sortChoices(sort).map((one) => (
          <option key={one} value={one}>
            {sortLabel(one)}
          </option>
        ))}
      </select>
      {after !== "" && <span>{after}</span>}
    </div>
  );
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

/**
 * The built-in that waits (`amenbo_core::ops::automation_builtin_wait::WAIT`). Its three numbers are a
 * length of time, which core takes only as a whole number of zero or more (`AMB-D-983`).
 */
const WAIT = "wait";

/** One setting: what the action declares, read, and the control its kind takes for this spot's answer. */
function CfgRow({ placementId, projectId, builtin, cfg, siblings, run }: {
  placementId: number;
  projectId: number | null;
  builtin: string | undefined;
  cfg: AutomationCfgDto;
  /** Every setting of the same spot, this one among them — what one setting's control may read of another's. */
  siblings: readonly AutomationCfgDto[];
  run: Run;
}) {
  const answer = (value: string | null) =>
    void run(answerAutomationCfg(placementId, cfg.name, value));
  const special = makeTaskControl(builtin, cfg.name);
  // A folder is picked from the project's own; only a spot answering one asks for them.
  const folders = useBoundFolders(cfg.kind === "folder" ? projectId : null);
  const folderPaths = folders.all.map((one) => one.path);
  const [text, setText] = useDraft(cfg.kind === "number" ? "" : readText(cfg.value));
  const [number, setNumber] = useDraft(cfg.kind === "number" ? String(readNumber(cfg.value) ?? "") : "");
  const filter: TaskFilter = readFilter(cfg.value);
  const sort = readSort(cfg.value);
  const dims = cfg.kind === "taskfilter"
    ? dimRows(getSnapshot().projects.find((p) => p.id === projectId)?.dimensions ?? [])
    : [];
  const choices = choicesOf(cfg.options);
  // A built-in's setting and its choices are drawn in the screen's language; what is written is still
  // the store's word, which is what the built-in reads its answer by.
  const shown = builtinWord(builtin, cfg.name);

  return (
    <Sec
      title={
        <>
          {shown}
          {cfg.required && <span className="autostep__req">● {t("auto.step.required")}</span>}
        </>
      }
    >

      {cfg.kind === "taskfilter" && (
        <div className="autostep__rows">
          {filterRows(builtin).map((row) => (
            <div key={row.key} className="autostep__row">
              <span className="autostep__rowname">{rowLabel(row.key)}</span>
              {row.values.map((value) => (
                <button
                  key={value}
                  type="button"
                  className={`autostep__chip ${(filter[row.key] ?? []).includes(value) ? "autostep__chip--on" : ""}`}
                  aria-pressed={(filter[row.key] ?? []).includes(value)}
                  onClick={() => answer(writeFilter(pressed(filter, row.key, value, row.single), sort))}
                >
                  {filterValueLabel(row.key, value)}
                </button>
              ))}
            </div>
          ))}
          {dims.map((row) => (
            <div key={`${DIM_KEY}:${row.axis}`} className="autostep__row">
              <span className="autostep__rowname">{row.axis}</span>
              {row.values.map((value) => {
                const token = dimToken(row.axis, value);
                const on = (filter[DIM_KEY] ?? []).includes(token);
                return (
                  <button
                    key={value}
                    type="button"
                    className={`autostep__chip ${on ? "autostep__chip--on" : ""}`}
                    aria-pressed={on}
                    onClick={() => answer(writeFilter(pressed(filter, DIM_KEY, token, false), sort))}
                  >
                    {value}
                  </button>
                );
              })}
            </div>
          ))}
          {/* Nothing pressed is no answer at all, and an order is an order of something: the list waits
              for a row to be pressed rather than writing an answer with no filter in it. */}
          <SortLine
            sort={sort}
            disabled={Object.keys(filter).length === 0}
            onSort={(next) => answer(writeFilter(filter, next))}
          />
        </div>
      )}

      {cfg.kind === "choice" && (
        <select
          aria-label={shown}
          value={readText(cfg.value)}
          onChange={(e) => answer(writeText(e.target.value))}
        >
          <option value="">—</option>
          {choices.map((one) => (
            <option key={one} value={one}>
              {builtinWord(builtin, one)}
            </option>
          ))}
        </select>
      )}

      {cfg.kind === "number" && (
        <input
          type="number"
          aria-label={shown}
          {...(builtin === WAIT && { min: 0, step: 1 })}
          value={number}
          onChange={(e) => setNumber(e.target.value)}
          onBlur={() => writeNumber(number) !== (cfg.value ?? null) && answer(writeNumber(number))}
        />
      )}

      {special === "classes" && <ClassRows projectId={projectId} value={cfg.value} onAnswer={answer} />}
      {special === "axes" && (
        <AxisChips
          projectId={projectId}
          value={cfg.value}
          classified={siblings.find((one) => one.name === CLASSIFY)?.value}
          onAnswer={answer}
        />
      )}
      {special === "numbers" && <NumberLines label={shown} value={cfg.value} onAnswer={answer} />}

      {/* A folder answered with one this project has not got stays offered, so what is written shows. */}
      {cfg.kind === "folder" && folderPaths.length > 0 && (
        <select
          aria-label={shown}
          value={readText(cfg.value)}
          onChange={(e) => answer(writeText(e.target.value))}
        >
          <option value="">—</option>
          {readText(cfg.value) !== "" && !folderPaths.includes(readText(cfg.value)) && (
            <option value={readText(cfg.value)}>{readText(cfg.value)}</option>
          )}
          {folderPaths.map((path) => (
            <option key={path} value={path}>
              {path}
            </option>
          ))}
        </select>
      )}

      {special === undefined && (cfg.kind === "text" || (cfg.kind === "folder" && folderPaths.length === 0)) && (
        <input
          aria-label={shown}
          value={text}
          onChange={(e) => setText(e.target.value)}
          onBlur={() => writeText(text) !== (cfg.value ?? null) && answer(writeText(text))}
        />
      )}
    </Sec>
  );
}

/**
 * One step of the placed action, and who carries it out at this spot, on one line: the step's name,
 * the agent, then the model that agent offers. Choosing another agent leaves the model to that agent's own default, since a model is
 * one agent's name for it. The empty agent is nobody chosen, which the launch check names.
 */
function AgentRow({
  placementId,
  step,
  agents,
  run,
}: {
  placementId: number;
  step: AutomationPlacementStepDto;
  agents: WakeCandidateDto[];
  run: Run;
}) {
  const models = useModels(step.agent ?? "");
  const choose = (agent: string | null, model: string | null) =>
    void run(chooseAutomationAgent(placementId, step.stepId, agent, model));
  return (
    <div className="autostep__who">
      <span className="autostep__rowname">{step.name}</span>
      <select
        aria-label={tf("auto.place.agentOf", { step: step.name })}
        value={step.agent ?? ""}
        onChange={(e) => choose(e.target.value === "" ? null : e.target.value, null)}
      >
        <option value="">{t("auto.place.agentNone")}</option>
        {step.agent !== undefined && agents.every((one) => one.id !== step.agent) && (
          <option value={step.agent}>{step.agent}</option>
        )}
        {agents.map((one) => (
          <option key={one.id} value={one.id} disabled={!one.installed}>
            {one.installed ? one.label : tf("auto.step.notHere", { agent: one.label })}
          </option>
        ))}
      </select>
      <select
        aria-label={tf("auto.place.modelOf", { step: step.name })}
        value={step.model ?? ""}
        disabled={step.agent === undefined}
        onChange={(e) => {
          if (step.agent === undefined) return;
          choose(step.agent, e.target.value === "" ? null : e.target.value);
        }}
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
  const inputName = builtinWord(placement.builtin, input.name);
  const chip = (
    <div>
      <DeclChip name={inputName} kind={kindLabel(input.kind)} required={input.required} tone={input.kind} />
    </div>
  );
  // Handed over in the start dialog, not by a wire — so there is nothing to choose here.
  if (readAtLaunch(graph, placement.builtin, placement.id, input.name)) {
    return (
      <div className="autostep__wire">
        {chip}
        <span className="autostep__atlaunch">{t("auto.step.atLaunch")}</span>
      </div>
    );
  }
  return (
    <div className="autostep__wire">
      {chip}
      <select
        aria-label={inputName}
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
            {[
              builtinWord(one.builtin, one.boxName),
              ...(one.exitName === undefined ? [] : [exitLabel(builtinWord(one.builtin, one.exitName))]),
              builtinWord(one.builtin, one.portName),
            ].join(" · ")}
          </option>
        ))}
      </select>
    </div>
  );
}

/**
 * **What a run starts at, and the press that changes it** — drawn on the start's own panel only. The
 * pulldown holds the built-ins a run can start at, in the order the empty picture offers them; picking
 * another asks first, since the lines out of this spot go with the one it replaces.
 */
function EntryRow({ automationId, current, run }: { automationId: number; current: string; run: Run }) {
  const builtins = useAutomationBuiltins();
  const offered = ENTRY_BUILTINS.flatMap((key) => builtins.filter((one) => one.key === key)).map(builtinShown);
  const replace = async (key: string) => {
    if (key === current) return;
    if (!(await confirmDialog(t("auto.step.entryReplaceConfirm")))) return;
    void run(replaceAutomationEntry(automationId, key));
  };
  return (
    <Sec title={t("auto.pic.entry")}>
      <select
        aria-label={t("auto.pic.entry")}
        value={current}
        onChange={(e) => void replace(e.target.value)}
      >
        {offered.map((one) => (
          <option key={one.key} value={one.key}>
            {one.name}
          </option>
        ))}
      </select>
    </Sec>
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
  const agents = useAgents(automation?.projectId ?? null);
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

  // Taking the spot away takes the answers written on it and every line naming it, so it asks first
  // — and what it leaves is the library action, which outlives any one picture. The panel is drawn
  // from that spot, so the screen is told to stop showing it.
  const remove = async () => {
    if (!(await confirmDialog(t("auto.step.placementRemoveConfirm")))) return;
    if (await run(removeAutomationPlacement(placement.id))) onRemoved();
  };
  const named = placement.exits.filter((one) => one.name !== ERROR_EXIT);
  const graph = automationGraph(automation)!;
  const isEntry = automation.entryPlacementId === placement.id;
  // The start comes off only as the last thing on the picture, which leaves it empty.
  const removable = !isEntry || automation.placements.length === 1;

  return (
    <div className="autostep">
      {refused !== null && <ErrorNote tone="quiet">{refused}</ErrorNote>}

      {/* The action, read: nothing on the card takes a value, and the one press goes to where it is
          written. Outside the fieldset, so a run holding the automation still lets a reader go and
          read it (`AMB-D-961`). */}
      <div className="autoplace__action">
        <span className="autoplace__icon" aria-hidden="true">
          <Icon name="gear" />
        </span>
        <div className="autoplace__actionbody">
          <span className="autoplace__actionname">{builtinWord(placement.builtin, placement.name)}</span>
          {action !== null && (
            <div className="autoplace__meta">
              <ReachChip global={action.global} builtin={placement.builtin !== undefined} />
              <span>{usedCount(action.usedBy)}</span>
            </div>
          )}
          {action !== null && action.steps.length === 0 && (
            <div className="autoplace__empty">{t("auto.place.empty")}</div>
          )}
        </div>
        <button type="button" className="autoplace__open" onClick={() => onOpenAction(placement.actionId)}>
          {t("auto.place.open")}
          <span aria-hidden="true"> ↗</span>
        </button>
      </div>

      <fieldset className="autostep__writes" disabled={readOnly}>
        {isEntry && placement.builtin !== undefined && (
          <EntryRow automationId={automation.id} current={placement.builtin} run={run} />
        )}

        {/* Amenbo carries a built-in out itself, and core refuses anybody chosen for it (`AMB-D-964`). */}
        {placement.steps.length > 0 && placement.builtin === undefined && (
          <Sec title={t("auto.place.agents")}>
            {placement.steps.map((step) => (
              <AgentRow key={step.stepId} placementId={placement.id} step={step} agents={agents} run={run} />
            ))}
          </Sec>
        )}

        {placement.settings.map((cfg) => (
          <CfgRow
            key={cfg.name}
            placementId={placement.id}
            projectId={automation.projectId}
            builtin={placement.builtin}
            cfg={cfg}
            siblings={placement.settings}
            run={run}
          />
        ))}

        {placement.inputs.length > 0 && (
          <Sec title={t("auto.step.inputs")}>
            {placement.inputs.map((input) => (
              <InputRow key={input.name} automation={automation} placement={placement} input={input} run={run} />
            ))}
          </Sec>
        )}

        <Sec title={t("auto.step.exits")}>
          <ul className="autostep__exits autostep__exits--flow">
            {named.map((one) => (
              <li key={one.id} className="autostep__exit">
                <NextRow
                  graph={graph}
                  picture="automation"
                  boxId={placement.id}
                  exitName={one.name}
                  run={run}
                  head={<ExitMark name={one.name} builtin={placement.builtin} />}
                />
              </li>
            ))}
            {/* The error way out, always drawn and always last: every action carries one, and a list
                that left it off where nobody had said anything about it would read as a spot that
                cannot fail. Saying nothing after it stops the run and calls a person, and the row is
                where a picture says otherwise. */}
            <li className="autostep__exiterr">
              <NextRow
                graph={graph}
                picture="automation"
                boxId={placement.id}
                exitName={ERROR_EXIT}
                run={run}
                head={<ExitMark name={ERROR_EXIT} />}
              />
            </li>
          </ul>
        </Sec>

        {removable && (
          <div className="actpanel__foot">
            <button type="button" className="btn btn--danger" onClick={() => void remove()}>
              {t("auto.step.placementRemove")}
            </button>
          </div>
        )}
      </fieldset>
    </div>
  );
}
