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
// **A setting is answered by the control its kind takes** — never by writing a filter expression. The
// shape each one is kept in is `./automationCfg`'s.
//
// **A wire is picked from a list, not drawn** (`./automationWires`), and what does not fit is not
// offered.
//
// **A refusal is drawn, once, at the top.** Answering a setting cannot be refused for anything a
// reader can see coming, but declaring one can — a name already taken, a blank one, the error way out
// — so every write this panel makes goes through one runner and the last refusal stands at the top.
import { useEffect, useState } from "react";
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
  renameAutomationExit,
  setAutomationWire,
  type CfgKind,
} from "../core/automations";
import { useBoundFolders } from "../core/boundFolders";
import { invoke } from "../core/ipc";
import { inTauri } from "../core/snapshot";
import { errText, isStatus, statusLabel, t, tf } from "../core/i18n";
import { ErrorNote } from "../components/ErrorNote";
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
import { kindLabel, PORT_KINDS } from "./automationPortKinds";
import type {
  AgentModelListDto,
  AutomationCfgDto,
  AutomationDetailDto,
  AutomationExitDto,
  AutomationPlacementDto,
  AutomationPortDto,
  WakeCandidateDto,
  WakeDto,
} from "../bindings/bindings";

/**
 * Send a write and say whether it was taken. Every control on the panel goes through it, so a refusal
 * lands on the one line the panel keeps for it rather than in the console — and a control that has
 * something to do afterwards (emptying the box it was typed in) can wait for the answer.
 */
type Run = (write: Promise<void> | void) => Promise<boolean>;

/** A name and what it is called, as the two pulldowns that pick a kind take them. */
type Choice = { id: string; label: string };

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

/**
 * The kinds of answer a setting may be declared to take, in the order they are offered — core's own
 * (`amenbo_core::model::AutomationCfgKind`). Spelled out rather than built from the id, so the key
 * gate can see every label a reader can be shown (`core/i18n/sourceKeys.test.ts`).
 */
const CFG_KINDS: readonly { id: CfgKind; label: () => string }[] = [
  { id: "taskfilter", label: () => t("auto.step.cfgKind.taskfilter") },
  { id: "folder", label: () => t("auto.step.cfgKind.folder") },
  { id: "choice", label: () => t("auto.step.cfgKind.choice") },
  { id: "number", label: () => t("auto.step.cfgKind.number") },
  { id: "text", label: () => t("auto.step.cfgKind.text") },
];

/** The kinds to choose between, in the language the panel is being drawn in. */
function choicesOfKinds(kinds: readonly { id: string; label: () => string }[]): Choice[] {
  return kinds.map((one) => ({ id: one.id, label: one.label() }));
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
  kinds: Choice[] | null;
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
      <button type="button" className="btn" disabled={name.trim() === ""} onClick={press}>
        {t("auto.step.add")}
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
  kinds: Choice[];
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
      <button type="button" className="btn" onClick={onRemove}>
        {t("auto.step.remove")}
      </button>
    </div>
  );
}

/** One way out: what it is called, what leaving by it hands on, and the presses that change either. */
function ExitRow({
  actionId,
  exit,
  onAddOutput,
  run,
}: {
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
      <input
        className="autostep__declname"
        placeholder={t("auto.step.exitUnnamed")}
        aria-label={t("auto.step.exits")}
        value={name}
        onChange={(e) => setName(e.target.value)}
        onBlur={() => {
          const now = name.trim() === "" ? null : name.trim();
          if (now !== was) void run(renameAutomationExit(actionId, was, now));
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
      <button type="button" className="btn" onClick={() => void run(removeAutomationExit(actionId, was))}>
        {t("auto.step.remove")}
      </button>
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
  const now = wireInto(automation, placement.id, input.name);
  const choices = wireChoices(automation, placement.id, input);
  const picked =
    now === undefined ? "" : choiceKey(now.fromPlacementId, now.fromExitName, now.fromPortName);
  return (
    <div className="autostep__wire">
      <DeclEdit
        label={t("auto.step.inputs")}
        name={input.name}
        kind={input.kind}
        kinds={choicesOfKinds(PORT_KINDS)}
        required={input.required}
        onRename={(to) => void run(editAutomationInput(actionId, input.name, { name: to }))}
        onKind={(to) =>
          void run(editAutomationInput(actionId, input.name, { kind: to as AutomationPortDto["kind"] }))
        }
        onRequired={(to) => void run(editAutomationInput(actionId, input.name, { required: to }))}
        onRemove={() => void run(removeAutomationInput(actionId, input.name))}
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
              {
                placementId: chosen.placementId,
                exitName: chosen.exitName,
                portName: chosen.portName,
              },
              { placementId: placement.id, portName: input.name },
            ),
          );
        }}
      >
        <option value="">{t("auto.step.unwired")}</option>
        {choices.map((one) => (
          <option key={one.key} value={one.key}>
            {`${one.placementName} · ${exitLabel(one.exitName)} · ${one.portName}`}
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
}: {
  automation: AutomationDetailDto | null;
  /** The spot the picture is showing as pressed, or nothing while none is. */
  placementId: number | null;
  projectId: number | null;
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
              declareAutomationInput(actionId, {
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
                actionId={actionId}
                exit={one}
                onAddOutput={() => setAdding(one.id)}
                run={run}
              />
            ))}
          {/* The error way out, always drawn and always last: every action carries one, and a list
              that left it off where nobody had said anything about it would read as a spot that
              cannot fail. It hands nothing on — what a step that fell over has to say is its report
              — and nothing here renames or removes it, which core refuses either way. */}
          <li className="autostep__exiterr">{t("auto.pic.errorExit")}</li>
        </ul>
        <DeclareRow
          what={t("auto.step.exitName")}
          kinds={null}
          onAdd={(declared) => run(declareAutomationExit(actionId, declared))}
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

      {adding !== null && (
        <AutomationOutputAdd
          exit={placement.exits.find((one) => one.id === adding)!}
          onClose={() => setAdding(null)}
        />
      )}
    </div>
  );
}
