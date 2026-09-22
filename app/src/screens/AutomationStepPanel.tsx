// What the pressed step holds, on the build screen's third place (`AMB-T-5256`, `AMB-T-5282`).
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
// **What a step declares is written here too, not only answered** — a second way out, a setting, an
// input. Only for a step carrying its own prompt: one that runs a library action reads the action's
// declarations, so the rows are drawn as they stand and the control that would change them is not
// there. The three families are addressed by name rather than by id, which is the shape the doors
// take (`../core/automations`).
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
//
// **A refusal is drawn, once, at the top.** Answering a setting cannot be refused for anything a
// reader can see coming, but declaring one can — a name already taken, a blank one, the error way out
// — so the panel keeps one line for whatever core last said no to.
import { useEffect, useState } from "react";
import {
  answerAutomationCfg,
  clearAutomationWire,
  declareAutomationCfg,
  declareAutomationExit,
  declareAutomationInput,
  editAutomationCfg,
  editAutomationInput,
  editAutomationStep,
  removeAutomationCfg,
  removeAutomationExit,
  removeAutomationInput,
  renameAutomationExit,
  setAutomationWire,
  useAutomationActions,
  type CfgKind,
  type PortKind,
} from "../core/automations";
import { useBoundFolders } from "../core/boundFolders";
import { invoke } from "../core/ipc";
import { inTauri } from "../core/snapshot";
import { errText, isStatus, statusLabel, t, tf } from "../core/i18n";
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
import type {
  AgentModelListDto,
  AutomationCfgDto,
  AutomationDetailDto,
  AutomationExitDto,
  AutomationPortDto,
  AutomationStepDto,
  WakeCandidateDto,
  WakeDto,
} from "../bindings/bindings";

/**
 * Send a write and say whether it was taken. Every control on the panel goes through it, so a refusal
 * lands on the one line the panel keeps for it rather than in the console — and a control that has
 * something to do afterwards (emptying the box it was typed in) can wait for the answer.
 */
type Run = (write: Promise<void> | void) => Promise<boolean>;

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

/** The kinds of answer a setting may be declared to take, in the order the control offers them. */
const CFG_KINDS: readonly CfgKind[] = ["taskfilter", "folder", "choice", "number", "text"];

/** What a setting of this kind is called. */
function cfgKindLabel(kind: CfgKind): string {
  if (kind === "taskfilter") return t("auto.step.cfgKind.taskfilter");
  if (kind === "folder") return t("auto.step.cfgKind.folder");
  if (kind === "choice") return t("auto.step.cfgKind.choice");
  if (kind === "number") return t("auto.step.cfgKind.number");
  return t("auto.step.cfgKind.text");
}

/** What an input may be declared to carry, in the order the control offers them. */
const PORT_KINDS: readonly PortKind[] = ["value", "file", "task_take", "task_make"];

/** What an input of this kind is called. */
function portKindLabel(kind: PortKind): string {
  if (kind === "value") return t("auto.step.portKind.value");
  if (kind === "file") return t("auto.step.portKind.file");
  if (kind === "task_take") return t("auto.step.portKind.taskTake");
  return t("auto.step.portKind.taskMake");
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

/**
 * The line that declares one more of something: the name it goes under, what it takes, and the press.
 *
 * The name is typed rather than generated. A generated one would have to be unique among what is
 * already declared, and the second press would be refused for a name the reader never chose — while
 * the name is the whole of what an edge or a wire will name this by.
 */
function DeclareRow({
  what,
  kinds,
  onAdd,
}: {
  /** What the empty box says it wants. */
  what: string;
  /** The kinds to choose between, or nothing where the family has none (a way out). */
  kinds: { id: string; label: string }[] | null;
  onAdd: (name: string, kind: string) => Promise<boolean>;
}) {
  const [name, setName] = useState("");
  const [kind, setKind] = useState(kinds?.[0]?.id ?? "");
  const press = () => {
    // Emptied only once it is written: a refusal leaves what was typed where the reader can fix it.
    void onAdd(name.trim(), kind).then((written) => {
      if (written) setName("");
    });
  };
  return (
    <div className="autostep__declare">
      <input
        className="autostep__declname"
        placeholder={what}
        aria-label={what}
        value={name}
        onChange={(e) => setName(e.target.value)}
      />
      {kinds !== null && (
        <select aria-label={what} value={kind} onChange={(e) => setKind(e.target.value)}>
          {kinds.map((one) => (
            <option key={one.id} value={one.id}>
              {one.label}
            </option>
          ))}
        </select>
      )}
      <button type="button" disabled={name.trim() === ""} onClick={press}>
        {t("auto.step.add")}
      </button>
    </div>
  );
}

/** One way out, as a step carrying its own prompt may write it. */
function ExitRow({ step, exit, run }: { step: AutomationStepDto; exit: AutomationExitDto; run: Run }) {
  const [name, setName] = useDraft(exit.name ?? "");
  const was = exit.name ?? null;
  return (
    <div className="autostep__decl">
      <input
        className="autostep__declname"
        placeholder={t("auto.step.exitUnnamed")}
        aria-label={t("auto.step.exits")}
        value={name}
        onChange={(e) => setName(e.target.value)}
        onBlur={() => {
          const now = name.trim() === "" ? null : name.trim();
          if (now !== was) void run(renameAutomationExit(step.id, was, now));
        }}
      />
      <button type="button" onClick={() => void run(removeAutomationExit(step.id, was))}>
        {t("auto.step.remove")}
      </button>
    </div>
  );
}

/** The name, the kind and the "has to be answered" of one declaration, with the press that ends it. */
function DeclEdit({
  name,
  kind,
  kinds,
  required,
  onRename,
  onKind,
  onRequired,
  onRemove,
  label,
}: {
  name: string;
  kind: string;
  kinds: { id: string; label: string }[];
  required: boolean;
  onRename: (to: string) => void;
  onKind: (to: string) => void;
  onRequired: (to: boolean) => void;
  onRemove: () => void;
  /** What this family is called, for a reader who hears the row rather than seeing it. */
  label: string;
}) {
  const [draft, setDraft] = useDraft(name);
  return (
    <div className="autostep__decl">
      <input
        className="autostep__declname"
        aria-label={label}
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={() => draft.trim() !== "" && draft.trim() !== name && onRename(draft.trim())}
      />
      <select aria-label={label} value={kind} onChange={(e) => onKind(e.target.value)}>
        {kinds.map((one) => (
          <option key={one.id} value={one.id}>
            {one.label}
          </option>
        ))}
      </select>
      <label className="autostep__check">
        <input type="checkbox" checked={required} onChange={(e) => onRequired(e.target.checked)} />
        {t("auto.step.required")}
      </label>
      <button type="button" onClick={onRemove}>
        {t("auto.step.remove")}
      </button>
    </div>
  );
}

/** One setting: how it is declared, and then the control its kind takes for the answer. */
function CfgRow({
  step,
  cfg,
  own,
  run,
}: {
  step: AutomationStepDto;
  cfg: AutomationCfgDto;
  /** Whether this step declares its own — a step running a library action reads the action's. */
  own: boolean;
  run: Run;
}) {
  const answer = (value: string | null) => void run(answerAutomationCfg(step.id, cfg.name, value));
  const [text, setText] = useDraft(cfg.kind === "number" ? "" : readText(cfg.value));
  const [number, setNumber] = useDraft(cfg.kind === "number" ? String(readNumber(cfg.value) ?? "") : "");
  const [choiceText, setChoiceText] = useDraft(choicesOf(cfg.options).join("\n"));
  const filter: TaskFilter = readFilter(cfg.value);
  const choices = choicesOf(cfg.options);

  return (
    <div className="autostep__cfg">
      {own ? (
        <DeclEdit
          label={t("auto.step.cfg")}
          name={cfg.name}
          kind={cfg.kind}
          kinds={CFG_KINDS.map((one) => ({ id: one, label: cfgKindLabel(one) }))}
          required={cfg.required}
          onRename={(to) => void run(editAutomationCfg(step.id, cfg.name, { name: to }))}
          // A choice list belongs to a choice and to nothing else, so leaving one behind on another
          // kind is refused — the move has to take it with it, in the one call.
          onKind={(to) =>
            void run(
              editAutomationCfg(step.id, cfg.name, {
                kind: to as CfgKind,
                ...(to === "choice" ? {} : { options: null }),
              }),
            )
          }
          onRequired={(to) => void run(editAutomationCfg(step.id, cfg.name, { required: to }))}
          onRemove={() => void run(removeAutomationCfg(step.id, cfg.name))}
        />
      ) : (
        <div className="autostep__cfgname">
          {cfg.name}
          {cfg.required && <span className="autostep__req">{t("auto.step.required")}</span>}
        </div>
      )}

      {own && cfg.kind === "choice" && (
        <label className="autostep__field">
          <span className="autostep__label">{t("auto.step.choices")}</span>
          <textarea
            rows={2}
            value={choiceText}
            onChange={(e) => setChoiceText(e.target.value)}
            onBlur={() =>
              writeChoices(choiceText) !== (cfg.options ?? null) &&
              void run(editAutomationCfg(step.id, cfg.name, { options: writeChoices(choiceText) }))
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

/** One input: how it is declared, and what is wired into it. */
function InputRow({
  automation,
  step,
  input,
  own,
  run,
}: {
  automation: AutomationDetailDto;
  step: AutomationStepDto;
  input: AutomationPortDto;
  own: boolean;
  run: Run;
}) {
  const now = wireInto(automation, step.id, input.name);
  const choices = wireChoices(automation, step.id, input);
  const picked = now === undefined ? "" : choiceKey(now.fromStepId, now.fromExitName, now.fromPortName);
  return (
    <div className="autostep__wire">
      {own ? (
        <DeclEdit
          label={t("auto.step.inputs")}
          name={input.name}
          kind={input.kind}
          kinds={PORT_KINDS.map((one) => ({ id: one, label: portKindLabel(one) }))}
          required={input.required}
          onRename={(to) => void run(editAutomationInput(step.id, input.name, { name: to }))}
          onKind={(to) => void run(editAutomationInput(step.id, input.name, { kind: to as PortKind }))}
          onRequired={(to) => void run(editAutomationInput(step.id, input.name, { required: to }))}
          onRemove={() => void run(removeAutomationInput(step.id, input.name))}
        />
      ) : (
        <span className="autostep__cfgname">
          {input.name}
          {input.required && <span className="autostep__req">{t("auto.step.required")}</span>}
        </span>
      )}
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
              { stepId: chosen.stepId, exitName: chosen.exitName, portName: chosen.portName },
              { stepId: step.id, portName: input.name },
            ),
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
  const [prompt, setPrompt] = useDraft(step?.prompt ?? "");
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

  if (automation === null || step === null) {
    return <div className="auto__empty">{t("auto.step.none")}</div>;
  }

  // Whether this step declares its own ways out, settings and inputs. One running a library action
  // reads the action's, and core refuses a declaration written on it.
  const own = step.actionId === undefined;
  const takesTask = step.exits.some((exit) => exit.outputs.some((port) => port.kind === "task_take"));
  // Where the working folder may be taken from: a setting or an input this step holds, by name.
  const folderNames = [
    ...step.settings.filter((one) => one.kind === "folder").map((one) => one.name),
    ...step.inputs.filter((one) => one.kind === "file").map((one) => one.name),
  ];

  return (
    <div className="autostep">
      {refused !== null && <div className="autostep__refused">{refused}</div>}

      <label className="autostep__field">
        <span className="autostep__label">{t("auto.step.name")}</span>
        <input
          value={name}
          onChange={(e) => setName(e.target.value)}
          onBlur={() => name !== step.name && void run(editAutomationStep(step.id, { name }))}
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
            void run(
              editAutomationStep(step.id, {
                source: e.target.value === "" ? { prompt: step.prompt } : { action: Number(e.target.value) },
              }),
            )
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
        {!own && (
          <span className="autostep__note">
            {tf("auto.step.declaresFromAction", { action: step.actionName ?? "" })}
          </span>
        )}
      </label>

      <label className="autostep__field">
        <span className="autostep__label">{t("auto.step.prompt")}</span>
        <textarea
          className="autostep__prompt"
          rows={6}
          readOnly={!own}
          value={prompt}
          onChange={(e) => setPrompt(e.target.value)}
          onBlur={() =>
            own && prompt !== step.prompt && void run(editAutomationStep(step.id, { source: { prompt } }))
          }
        />
        {!own && (
          <span className="autostep__note">
            {tf("auto.step.promptFromAction", { action: step.actionName ?? "" })}
          </span>
        )}
      </label>

      <div className="autostep__field">
        <span className="autostep__label">{t("auto.step.cfg")}</span>
        {step.settings.length === 0 && <span className="autostep__said">{t("auto.step.declaresNone")}</span>}
        {step.settings.map((cfg) => (
          <CfgRow key={cfg.name} step={step} cfg={cfg} own={own} run={run} />
        ))}
        {own && (
          <DeclareRow
            what={t("auto.step.cfgName")}
            kinds={CFG_KINDS.map((one) => ({ id: one, label: cfgKindLabel(one) }))}
            onAdd={(declared, kind) =>
              run(declareAutomationCfg(step.id, { name: declared, kind: kind as CfgKind }))
            }
          />
        )}
      </div>

      <div className="autostep__field">
        <span className="autostep__label">{t("auto.step.inputs")}</span>
        {step.inputs.length === 0 && <span className="autostep__said">{t("auto.step.declaresNone")}</span>}
        {step.inputs.map((input) => (
          <InputRow
            key={input.name}
            automation={automation}
            step={step}
            input={input}
            own={own}
            run={run}
          />
        ))}
        {own && (
          <DeclareRow
            what={t("auto.step.inputName")}
            kinds={PORT_KINDS.map((one) => ({ id: one, label: portKindLabel(one) }))}
            onAdd={(declared, kind) =>
              run(declareAutomationInput(step.id, { name: declared, kind: kind as PortKind }))
            }
          />
        )}
      </div>

      <div className="autostep__field">
        <span className="autostep__label">{t("auto.step.exits")}</span>
        {own ? (
          <>
            {step.exits
              .filter((one) => one.name !== ERROR_EXIT)
              .map((one) => (
                <ExitRow key={one.id} step={step} exit={one} run={run} />
              ))}
            {/* The error way out, always drawn and always last, and never a row a press can reach:
                every step carries one whether or not anything says so. */}
            <div className="autostep__exiterr">{t("auto.pic.errorExit")}</div>
            <DeclareRow
              what={t("auto.step.exitName")}
              kinds={null}
              onAdd={(declared) => run(declareAutomationExit(step.id, declared))}
            />
          </>
        ) : (
          <ul className="autostep__exits">
            {step.exits
              .filter((one) => one.name !== ERROR_EXIT)
              .map((one) => (
                <li key={one.id}>{exitLabel(one.name)}</li>
              ))}
            <li className="autostep__exiterr">{t("auto.pic.errorExit")}</li>
          </ul>
        )}
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
            void run(editAutomationStep(step.id, { model: e.target.value === "" ? null : e.target.value }))
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
            void run(editAutomationStep(step.id, { workDir: e.target.value === "" ? null : e.target.value }))
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
          onChange={(e) => void run(editAutomationStep(step.id, { reportToTask: e.target.checked }))}
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
    </div>
  );
}
