// What the pressed step holds, on the build screen's third place (`AMB-T-5256`).
//
// **It stands where it stands, whatever the picture does.** A definition of forty steps draws a
// picture two thousand pixels tall, and a panel that followed the box a reader pressed would put the
// contents off the bottom of the screen — so the picture scrolls inside its own place
// (`./AutomationPicture`) and this one is always in the same spot.
//
// **Every field writes on the spot.** There is no Save: what a reader changed is what the definition
// now says, and a panel with a button would leave a step half-edited every time somebody pressed
// another box in the picture. A box of text writes when the caret leaves it, so a name is not written
// a letter at a time.
//
// **Where the prompt comes from is one control** (`auto.step.source`). A step runs a library action
// or carries a prompt of its own, and there is no state between the two — drawn as "use" and "drop"
// it would be two presses for one change, with a step carrying neither in between. Switching takes
// the step's ways out, its settings and its inputs with it, because those are read off whichever of
// the two declares them, and the control says so beside itself
// (`amenbo_core::ops::automation::step_update`).
//
// **A setting is answered by the control its kind takes** — never by writing a filter expression. The
// shape each one is kept in is `./automationCfg`'s.
//
// **A wire is picked from a list, not drawn** (`./automationWires`), and what does not fit is not
// offered.
//
// **The agent and the model are this panel's even for a step that runs a library action.** The prompt
// is the library's; who carries it out is the automation's, and the same action is run by one step on
// one agent and by another step on another.
import { useEffect, useState } from "react";
import {
  answerAutomationCfg,
  clearAutomationWire,
  editAutomationStep,
  setAutomationWire,
  useAutomationActions,
} from "../core/automations";
import { useBoundFolders } from "../core/boundFolders";
import { invoke } from "../core/ipc";
import { inTauri } from "../core/snapshot";
import { isStatus, statusLabel, t, tf } from "../core/i18n";
import { ERROR_EXIT } from "./automationLayout";
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
import { kindLabel } from "./automationPortKinds";
import type {
  AgentModelListDto,
  AutomationCfgDto,
  AutomationDetailDto,
  AutomationStepDto,
  WakeCandidateDto,
  WakeDto,
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

/** A way out, as the list of them names it. The unnamed one has no name to put there. */
function exitLabel(name: string | undefined): string {
  if (name === undefined) return t("auto.step.exitUnnamed");
  return name === ERROR_EXIT ? t("auto.pic.errorExit") : name;
}

/** A box of text that writes when the caret leaves it rather than a letter at a time. */
function useDraft(value: string): [string, (next: string) => void] {
  const [draft, setDraft] = useState(value);
  useEffect(() => setDraft(value), [value]);
  return [draft, setDraft];
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

/** The agents this project could start a step with, in catalog order. */
function useAgents(projectId: number | null): WakeCandidateDto[] {
  const folders = useBoundFolders(projectId);
  const paths = folders.live.map((one) => one.path).join("\n");
  const [agents, setAgents] = useState<WakeCandidateDto[]>([]);
  useEffect(() => {
    if (!inTauri() || projectId === null) return;
    let live = true;
    void invoke<WakeDto>("wake_choices", { project: projectId, folders: paths === "" ? [] : paths.split("\n") })
      .then((wake) => {
        if (live) setAgents(wake.candidates);
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [projectId, paths]);
  return agents;
}

/** What this agent says it can be started on, or nothing while it has not answered. */
function useModels(agent: string): AgentModelListDto | null {
  const [said, setSaid] = useState<AgentModelListDto | null>(null);
  useEffect(() => {
    if (!inTauri() || agent === "") return;
    let live = true;
    setSaid(null);
    void invoke<AgentModelListDto>("agent_models", { agent })
      .then((answer) => {
        if (live) setSaid(answer);
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [agent]);
  return said;
}

/** One setting, drawn as the control its kind takes. */
function CfgRow({ step, cfg }: { step: AutomationStepDto; cfg: AutomationCfgDto }) {
  const answer = (value: string | null) => void answerAutomationCfg(step.id, cfg.name, value);
  const [text, setText] = useDraft(cfg.kind === "number" ? "" : readText(cfg.value));
  const [number, setNumber] = useDraft(cfg.kind === "number" ? String(readNumber(cfg.value) ?? "") : "");
  const filter: TaskFilter = readFilter(cfg.value);
  const choices = choicesOf(cfg.options);

  return (
    <div className="autostep__cfg">
      <div className="autostep__cfgname">
        {cfg.name}
        {cfg.required && <span className="autostep__req">{t("auto.step.required")}</span>}
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

export function AutomationStepPanel({
  automation,
  stepId,
  projectId,
}: {
  automation: AutomationDetailDto | null;
  /** The step the picture is showing as pressed, or nothing while none is. */
  stepId: number | null;
  projectId: number | null;
}) {
  const actions = useAutomationActions(projectId);
  const step = automation?.steps.find((one) => one.id === stepId) ?? null;
  const agents = useAgents(projectId);
  const models = useModels(step?.agent ?? "");
  const [name, setName] = useDraft(step?.name ?? "");
  // The way out an output artefact is being declared on, while that dialog is open
  // (`AMB-T-5257`).
  const [adding, setAdding] = useState<number | null>(null);
  const [prompt, setPrompt] = useDraft(step?.prompt ?? "");

  if (automation === null || step === null) {
    return <div className="auto__empty">{t("auto.step.none")}</div>;
  }

  const takesTask = step.exits.some((exit) => exit.outputs.some((port) => port.kind === "task_take"));
  // Where the working folder may be taken from: a setting or an input this step holds, by name.
  const folderNames = [
    ...step.settings.filter((one) => one.kind === "folder").map((one) => one.name),
    ...step.inputs.filter((one) => one.kind === "file").map((one) => one.name),
  ];

  return (
    <div className="autostep">
      <label className="autostep__field">
        <span className="autostep__label">{t("auto.step.name")}</span>
        <input
          value={name}
          onChange={(e) => setName(e.target.value)}
          onBlur={() => name !== step.name && void editAutomationStep(step.id, { name })}
        />
      </label>

      <div className="autostep__field">
        <span className="autostep__label">{t("auto.step.task")}</span>
        <span className="autostep__said">
          {takesTask ? t("auto.step.takesTask") : t("auto.step.carriesTask")}
        </span>
      </div>

      <label className="autostep__field">
        <span className="autostep__label">{t("auto.step.source")}</span>
        <select
          value={step.actionId === undefined ? "" : String(step.actionId)}
          onChange={(e) =>
            void editAutomationStep(step.id, {
              source: e.target.value === "" ? { prompt: step.prompt } : { action: Number(e.target.value) },
            })
          }
        >
          <option value="">{t("auto.step.sourceOwn")}</option>
          {actions.map((one) => (
            <option key={one.id} value={String(one.id)}>
              {one.name}
            </option>
          ))}
        </select>
        <span className="autostep__note">{t("auto.step.sourceMoves")}</span>
      </label>

      <label className="autostep__field">
        <span className="autostep__label">{t("auto.step.prompt")}</span>
        <textarea
          className="autostep__prompt"
          rows={6}
          readOnly={step.actionId !== undefined}
          value={prompt}
          onChange={(e) => setPrompt(e.target.value)}
          onBlur={() =>
            step.actionId === undefined &&
            prompt !== step.prompt &&
            void editAutomationStep(step.id, { source: { prompt } })
          }
        />
        {step.actionId !== undefined && (
          <span className="autostep__note">
            {tf("auto.step.promptFromAction", { action: step.actionName ?? "" })}
          </span>
        )}
      </label>

      <div className="autostep__field">
        <span className="autostep__label">{t("auto.step.cfg")}</span>
        {step.settings.length === 0 && <span className="autostep__said">{t("auto.step.declaresNone")}</span>}
        {step.settings.map((cfg) => (
          <CfgRow key={cfg.name} step={step} cfg={cfg} />
        ))}
      </div>

      <div className="autostep__field">
        <span className="autostep__label">{t("auto.step.inputs")}</span>
        {step.inputs.length === 0 && <span className="autostep__said">{t("auto.step.declaresNone")}</span>}
        {step.inputs.map((input) => {
          const now = wireInto(automation, step.id, input.name);
          const choices = wireChoices(automation, step.id, input);
          const picked =
            now === undefined ? "" : choiceKey(now.fromStepId, now.fromExitName, now.fromPortName);
          return (
            <label key={input.name} className="autostep__wire">
              <span className="autostep__cfgname">
                {input.name}
                {input.required && <span className="autostep__req">{t("auto.step.required")}</span>}
              </span>
              <select
                      value={picked}
                onChange={(e) => {
                  const chosen = choices.find((one) => one.key === e.target.value);
                  if (chosen === undefined) {
                    if (now !== undefined) void clearAutomationWire(now.id);
                    return;
                  }
                  void setAutomationWire(
                    { stepId: chosen.stepId, exitName: chosen.exitName, portName: chosen.portName },
                    { stepId: step.id, portName: input.name },
                  );
                }}
              >
                <option value="">{t("auto.step.unwired")}</option>
                {choices.map((one) => (
                  <option key={one.key} value={one.key}>
                    {`${one.stepName} · ${exitLabel(one.exitName)} · ${one.portName}`}
                  </option>
                ))}
              </select>
            </label>
          );
        })}
      </div>

      <div className="autostep__field">
        <span className="autostep__label">{t("auto.step.exits")}</span>
        <ul className="autostep__exits">
          {step.exits
            .filter((one) => one.name !== ERROR_EXIT)
            .map((one) => (
              <li key={one.id} className="autostep__exit">
                <span className="autostep__exitname">{exitLabel(one.name)}</span>
                {/* What leaving by this way out hands on. It hangs off the way out and not off the
                    step, because a step with three ways out hands on three different things. */}
                {one.outputs.map((port) => (
                  <span key={port.name} className="autostep__out">
                    {port.name}
                    <span className="autostep__outkind">{kindLabel(port.kind)}</span>
                  </span>
                ))}
                {/* Only for a step that declares its own. A step running a library action reads the
                    action's ways out, and an output declared on one of those is declared for every
                    step running that action — which is the library's to change, not this step's. */}
                {step.actionId === undefined && (
                  <button
                    type="button"
                    className="btn autostep__outadd"
                    onClick={() => setAdding(one.id)}
                  >
                    {t("auto.step.outputAdd")}
                  </button>
                )}
              </li>
            ))}
          {/* The error way out, always drawn and always last: every step carries one, and a list that
              left it off where nobody had said anything about it would read as a step that cannot
              fail. It hands nothing on — what a step that fell over has to say is its report. */}
          <li className="autostep__exiterr">{t("auto.pic.errorExit")}</li>
        </ul>
      </div>

      <label className="autostep__field">
        <span className="autostep__label">{t("auto.step.agent")}</span>
        <select
          value={step.agent}
          onChange={(e) => void editAutomationStep(step.id, { agent: e.target.value })}
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
            void editAutomationStep(step.id, { model: e.target.value === "" ? null : e.target.value })
          }
        >
          <option value="">{t("auto.step.modelDefault")}</option>
          {step.model !== undefined && (models?.models ?? []).every((one) => one.id !== step.model) && (
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
            void editAutomationStep(step.id, { workDir: e.target.value === "" ? null : e.target.value })
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
          onChange={(e) => void editAutomationStep(step.id, { interactive: e.target.checked })}
        />
        {t("auto.step.interactive")}
      </label>

      <label className="autostep__check">
        <input
          type="checkbox"
          checked={step.reportToTask}
          onChange={(e) => void editAutomationStep(step.id, { reportToTask: e.target.checked })}
        />
        {t("auto.step.reportToTask")}
      </label>

      <label className="autostep__check">
        <input
          type="checkbox"
          checked={step.showHistory}
          onChange={(e) => void editAutomationStep(step.id, { history: e.target.checked })}
        />
        {t("auto.step.history")}
      </label>

      {adding !== null && (
        <AutomationOutputAdd
          exit={step.exits.find((one) => one.id === adding)!}
          onClose={() => setAdding(null)}
        />
      )}
    </div>
  );
}
