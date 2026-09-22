// What the pressed spot holds, on the build screen's third place (`AMB-T-5256`, `AMB-T-5282`).
//
// **It stands where it stands, whatever the picture does.** A definition of forty boxes draws a
// picture two thousand pixels tall, and a panel that followed the box a reader pressed would put the
// contents off the bottom of the screen — so the picture scrolls inside its own place
// (`./AutomationPicture`) and this one is always in the same spot.
//
// **Every field writes on the spot.** There is no Save: what a reader changed is what the definition
// now says, and a panel with a button would leave a spot half-edited every time somebody pressed
// another box in the picture. A box of text writes when the caret leaves it, so a name is not written
// a letter at a time.
//
// **Each field writes on the layer it belongs to** (`AMB-D-949`). A box on the picture is a placement
// of a library action: what it declares — a way out, an input, a setting — is the action's, and
// reaches every other placement of it; what a setting is answered with is this placement's alone; and
// the prompt, the agent, the model and the three flags are the action's step's. The doors take
// whichever of the three the field lives on (`../core/automations`).
//
// **Two rows write on the automation rather than on any of the three**: whether a run opens here, and
// what happens after each way out is taken. Neither belongs to the action — one action placed on two
// pictures opens one of them and goes on to different boxes on each — so they are drawn beside the
// spot and written on the definition.
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
// **A refusal is drawn, once, at the top.** Answering a setting cannot be refused for anything a
// reader can see coming, but declaring one can — a name already taken, a blank one, the error way out
// — so every write this panel makes goes through one runner and the last refusal stands at the top.
import { useState } from "react";
import {
  answerAutomationCfg,
  clearAutomationWire,
  declareAutomationCfg,
  declareAutomationExit,
  declareAutomationInput,
  editAutomationAction,
  editAutomationCfg,
  editAutomationInput,
  editAutomationStep,
  removeAutomationCfg,
  removeAutomationExit,
  removeAutomationInput,
  removeAutomationPlacement,
  renameAutomationExit,
  setAutomationEntry,
  setAutomationWire,
  type CfgKind,
} from "../core/automations";
import { confirmDialog } from "../core/dialog";
import { errText, isStatus, statusLabel, t, tf } from "../core/i18n";
import { ErrorNote } from "../components/ErrorNote";
import { automationGraph, ERROR_EXIT } from "./automationLayout";
import {
  CFG_KINDS,
  choicesOfKinds,
  DeclareRow,
  DeclEdit,
  exitLabel,
  NextRow,
  useAgents,
  useDraft,
  useModels,
  type Run,
} from "./automationPanel";
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
import { AutomationOutputAdd } from "./AutomationOutputAdd";
import { kindLabel, PORT_KINDS } from "./automationPortKinds";
import type {
  AutomationCfgDto,
  AutomationDetailDto,
  AutomationExitDto,
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

/**
 * The choice list as a reader writes it: one per line.
 *
 * A line rather than a separator because a choice is a label a person reads on a pulldown, and any
 * separator this could take — a comma above all — is a character that belongs inside one. Nothing is
 * left to escape.
 */
function writeChoices(text: string): string | null {
  const choices = text.split("\n").map((one) => one.trim()).filter((one) => one !== "");
  return choices.length === 0 ? null : JSON.stringify(choices);
}

/** One way out: what it is called, what leaving by it hands on, and the presses that change either. */
function ExitRow({
  automation,
  placementId,
  actionId,
  exit,
  onAddOutput,
  run,
}: {
  /** The picture the edge below the row is drawn on — a way out is the action's, an edge is not. */
  automation: AutomationDetailDto;
  placementId: number;
  /** The library action that declares it — what a way out belongs to, never the placement. */
  actionId: number;
  exit: AutomationExitDto;
  onAddOutput: () => void;
  run: Run;
}) {
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
        {/* What leaving by this way out hands on. It hangs off the way out and not off the action,
            because an action with three ways out hands on three different things. */}
        {exit.outputs.map((port) => (
          <span key={port.name} className="autostep__out">
            {port.name}
            <span className="autostep__outkind">{kindLabel(port.kind)}</span>
          </span>
        ))}
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
      <NextRow
        graph={automationGraph(automation)!}
        picture="automation"
        boxId={placementId}
        exitName={exit.name}
        run={run}
      />
    </li>
  );
}

/** One setting: how it is declared, and then the control its kind takes for the answer. */
function CfgRow({
  placementId,
  actionId,
  cfg,
  run,
}: {
  /** The spot the answer is written on. */
  placementId: number;
  /** The library action that declares it — where the name, the kind and the choices are written. */
  actionId: number;
  cfg: AutomationCfgDto;
  run: Run;
}) {
  const answer = (value: string | null) =>
    void run(answerAutomationCfg(placementId, cfg.name, value));
  const [text, setText] = useDraft(cfg.kind === "number" ? "" : readText(cfg.value));
  const [number, setNumber] = useDraft(cfg.kind === "number" ? String(readNumber(cfg.value) ?? "") : "");
  const [choiceText, setChoiceText] = useDraft(choicesOf(cfg.options).join("\n"));
  const filter: TaskFilter = readFilter(cfg.value);
  const choices = choicesOf(cfg.options);

  return (
    <div className="autostep__cfg">
      <DeclEdit
        label={t("auto.step.cfg")}
        name={cfg.name}
        kind={cfg.kind}
        kinds={choicesOfKinds(CFG_KINDS)}
        required={cfg.required}
        onRename={(to) => void run(editAutomationCfg(actionId, cfg.name, { name: to }))}
        // A choice list belongs to a choice and to nothing else, so leaving one behind on another
        // kind is refused — the move has to take it with it, in the one call.
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
          value={number}
          onChange={(e) => setNumber(e.target.value)}
          onBlur={() => writeNumber(number) !== (cfg.value ?? null) && answer(writeNumber(number))}
        />
      )}

      {(cfg.kind === "text" || cfg.kind === "folder") && (
        <input
          value={text}
          onChange={(e) => setText(e.target.value)}
          onBlur={() => writeText(text) !== (cfg.value ?? null) && answer(writeText(text))}
        />
      )}
    </div>
  );
}

/** One input: how it is declared on the action, and what is wired into it at this spot. */
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
  const actionId = placement.actionId;
  const graph = automationGraph(automation)!;
  const now = wireInto(graph, placement.id, input.name);
  const choices = wireChoices(graph, placement.id, input);
  const picked = now === undefined ? "" : choiceKey(now.fromId, now.fromExitName, now.fromPortName);
  return (
    <div className="autostep__wire">
      <DeclEdit
        label={t("auto.step.inputs")}
        name={input.name}
        kind={input.kind}
        kinds={choicesOfKinds(PORT_KINDS)}
        required={input.required}
        onRename={(to) => void run(editAutomationInput("action", actionId, input.name, { name: to }))}
        onKind={(to) =>
          void run(
            editAutomationInput("action", actionId, input.name, {
              kind: to as AutomationPortDto["kind"],
            }),
          )
        }
        onRequired={(to) =>
          void run(editAutomationInput("action", actionId, input.name, { required: to }))
        }
        onRemove={() => void run(removeAutomationInput("action", actionId, input.name))}
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
  projectId,
  onRemoved,
}: {
  automation: AutomationDetailDto | null;
  /** The spot the picture is showing as pressed, or nothing while none is. */
  placementId: number | null;
  projectId: number | null;
  /** The spot this panel was drawn from is gone — there is nothing left for the picture to mark. */
  onRemoved: () => void;
}) {
  const placement = automation?.placements.find((one) => one.id === placementId) ?? null;
  const agents = useAgents(projectId);
  const models = useModels(placement?.agent ?? "");
  const [name, setName] = useDraft(placement?.name ?? "");
  // The way out an output artefact is being declared on, while that dialog is open
  // (`AMB-T-5257`).
  const [adding, setAdding] = useState<number | null>(null);
  const [prompt, setPrompt] = useDraft(placement?.prompt ?? "");
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

  const actionId = placement.actionId;
  // The step this spot opens — where the prompt, the agent, the model and the three flags are
  // written. An action that holds none is one the launch check names, and its fields are left off
  // rather than drawn over nothing.
  const stepId = placement.stepId;
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
  // Where the working folder may be taken from: a setting or an input this action holds, by name.
  const folderNames = [
    ...placement.settings.filter((one) => one.kind === "folder").map((one) => one.name),
    ...placement.inputs.filter((one) => one.kind === "file").map((one) => one.name),
  ];

  return (
    <div className="autostep">
      {refused !== null && <ErrorNote tone="quiet">{refused}</ErrorNote>}

      <label className="autostep__field">
        <span className="autostep__label">{t("auto.step.name")}</span>
        <input
          value={name}
          onChange={(e) => setName(e.target.value)}
          onBlur={() => name !== placement.name && void run(editAutomationAction(actionId, { name }))}
        />
      </label>

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

      {stepId !== undefined && (
        <label className="autostep__field">
          <span className="autostep__label">{t("auto.step.prompt")}</span>
          <textarea
            className="autostep__prompt"
            rows={6}
            value={prompt}
            onChange={(e) => setPrompt(e.target.value)}
            onBlur={() =>
              prompt !== placement.prompt && void run(editAutomationStep(stepId, { prompt }))
            }
          />
        </label>
      )}

      <div className="autostep__field">
        <span className="autostep__label">{t("auto.step.cfg")}</span>
        {placement.settings.length === 0 && (
          <span className="autostep__said">{t("auto.step.declaresNone")}</span>
        )}
        {placement.settings.map((cfg) => (
          <CfgRow
            key={cfg.name}
            placementId={placement.id}
            actionId={actionId}
            cfg={cfg}
            run={run}
          />
        ))}
        <DeclareRow
          what={t("auto.step.cfgName")}
          kinds={choicesOfKinds(CFG_KINDS)}
          onAdd={(declared, kind) =>
            run(declareAutomationCfg(actionId, { name: declared, kind: kind as CfgKind }))
          }
        />
      </div>

      <div className="autostep__field">
        <span className="autostep__label">{t("auto.step.inputs")}</span>
        {placement.inputs.length === 0 && (
          <span className="autostep__said">{t("auto.step.declaresNone")}</span>
        )}
        {placement.inputs.map((input) => (
          <InputRow
            key={input.name}
            automation={automation}
            placement={placement}
            input={input}
            run={run}
          />
        ))}
        <DeclareRow
          what={t("auto.step.inputName")}
          kinds={choicesOfKinds(PORT_KINDS)}
          onAdd={(declared, kind) =>
            run(
              declareAutomationInput("action", actionId, {
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
          {placement.exits
            .filter((one) => one.name !== ERROR_EXIT)
            .map((one) => (
              <ExitRow
                key={one.id}
                automation={automation}
                placementId={placement.id}
                actionId={actionId}
                exit={one}
                onAddOutput={() => setAdding(one.id)}
                run={run}
              />
            ))}
          {/* The error way out, always drawn and always last: every action carries one, and a list
              that left it off where nobody had said anything about it would read as a spot that
              cannot fail. It hands nothing on — what a step that fell over has to say is its report
              — and nothing here renames or removes it, which core refuses either way. What it does
              take is an edge: saying nothing stops the run and calls a person, and the row below is
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
        <DeclareRow
          what={t("auto.step.exitName")}
          kinds={null}
          onAdd={(declared) => run(declareAutomationExit("action", actionId, declared))}
        />
      </div>

      {stepId !== undefined && (
        <>
          <label className="autostep__field">
            <span className="autostep__label">{t("auto.step.agent")}</span>
            <select
              value={placement.agent}
              onChange={(e) => void run(editAutomationStep(stepId, { agent: e.target.value }))}
            >
              {agents.every((one) => one.id !== placement.agent) && (
                <option value={placement.agent}>{placement.agent}</option>
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
              value={placement.model ?? ""}
              onChange={(e) =>
                void run(
                  editAutomationStep(stepId, {
                    model: e.target.value === "" ? null : e.target.value,
                  }),
                )
              }
            >
              <option value="">{t("auto.step.modelDefault")}</option>
              {placement.model !== undefined &&
                (models?.models ?? []).every((one) => one.id !== placement.model) && (
                  <option value={placement.model}>{placement.model}</option>
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
              value={placement.workDirRef ?? ""}
              onChange={(e) =>
                void run(
                  editAutomationStep(stepId, {
                    workDir: e.target.value === "" ? null : e.target.value,
                  }),
                )
              }
            >
              <option value="">{t("auto.step.folderNone")}</option>
              {placement.workDirRef !== undefined && !folderNames.includes(placement.workDirRef) && (
                <option value={placement.workDirRef}>{placement.workDirRef}</option>
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
              checked={placement.interactive}
              onChange={(e) => void run(editAutomationStep(stepId, { interactive: e.target.checked }))}
            />
            {t("auto.step.interactive")}
          </label>

          <label className="autostep__check">
            <input
              type="checkbox"
              checked={placement.reportToTask}
              onChange={(e) =>
                void run(editAutomationStep(stepId, { reportToTask: e.target.checked }))
              }
            />
            {t("auto.step.reportToTask")}
          </label>

          <label className="autostep__check">
            <input
              type="checkbox"
              checked={placement.showHistory}
              onChange={(e) => void run(editAutomationStep(stepId, { history: e.target.checked }))}
            />
            {t("auto.step.history")}
          </label>
        </>
      )}

      <div className="settings__row">
        <button type="button" className="btn btn--danger" onClick={() => void remove()}>
          {t("auto.step.placementRemove")}
        </button>
      </div>
      <div className="autostep__said">{t("auto.step.placementRemoveWhat")}</div>

      {adding !== null && (
        <AutomationOutputAdd
          exit={placement.exits.find((one) => one.id === adding)!}
          onClose={() => setAdding(null)}
        />
      )}
    </div>
  );
}
