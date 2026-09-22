// The automations screen's own seam: a project's definitions, one definition whole, whether that one
// could be started, the library its steps are pointed at — and the one write that is not the screen's
// at all, a run being stopped from the pane it is drawn in.
//
// It sits beside `core/reads.ts` rather than in it because what it reads is a different shape of
// thing: a task list is paged and an automation is not. An automation is tens of rows, and the build
// screen's picture, its step panel and its launch check all walk the same definition — so it is
// fetched whole, once, and every part of the screen reads that one answer.
//
// **The writes here sit beside their reads** rather than in `core/mutations`, because what they are
// about is this screen and nothing else. What the screen's own write does not do for itself is the
// invalidation — the ack goes through `mutations.invokeAck`, the same road every other write takes.
// The second one is pressed from a pane rather than from this screen, and is no ack at all
// (`stopRun`).
//
// **The launch check is read, not worked out here.** The rules live in core, where the launch itself
// reads them (`amenbo_core::ops::automation_run::check`), so what a screen says is in the way and
// what a press refuses cannot come to disagree. What this side supplies is the one fact the store
// cannot answer: which agents this machine can actually start.
import { useQuery } from "./query";
import { inTauri } from "./snapshot";
import { invoke } from "./ipc";
import { invokeAck, invokeForAck } from "./mutations";
import type {
  AutomationActionCardDto,
  AutomationCardDto,
  AutomationCfgDto,
  AutomationDetailDto,
  AutomationLaunchCheckDto,
  AutomationPortDto,
  AutomationRunCardDto,
  AutomationRunStartedDto,
  WakeDto,
} from "../bindings/bindings";

/** What kind of answer a setting takes, as the definition declares it. */
export type CfgKind = AutomationCfgDto["kind"];

/** A project's automations, in the order they were placed in. */
export async function fetchAutomations(projectId: number): Promise<AutomationCardDto[]> {
  if (!inTauri()) return [];
  return invoke<AutomationCardDto[]>("automation_page", { projectId });
}

/** Subscribing read of a project's automations. */
export function useAutomations(projectId: number | null): AutomationCardDto[] {
  const { data } = useQuery<AutomationCardDto[]>(
    ["automations", projectId ?? null],
    () => (projectId === null ? Promise.resolve([]) : fetchAutomations(projectId)),
  );
  return data ?? [];
}

/**
 * **Make an automation** in this project, and answer with its id so the screen can open the build
 * screen on it.
 *
 * A name is the whole press. There are no steps and no entry yet, and what refuses to launch one in
 * that state is the launch check rather than this — an automation is built in whatever order its
 * author likes (`amenbo_core::ops::automation::add`).
 *
 * The id is lifted out of the ack the write already returns (`./mutations`), so the list and the new
 * definition arrive from one call. `null` outside Tauri, where the browser mock holds no
 * automations.
 */
export async function addAutomation(projectId: number, name: string): Promise<number | null> {
  if (!inTauri()) return null;
  const ack = await invokeForAck("automation_add", { projectId, name });
  return ack.automations[0] ?? null;
}

/**
 * The library this project reaches — the device's own actions and the project's own, in one list.
 *
 * Both reaches come in one answer because both are one list on screen: what a reader is choosing
 * between is every prompt a step here could be pointed at, and which library holds one is a column.
 */
export async function fetchAutomationActions(projectId: number): Promise<AutomationActionCardDto[]> {
  if (!inTauri()) return [];
  return invoke<AutomationActionCardDto[]>("automation_action_page", { projectId });
}

/** Subscribing read of the library this project reaches. */
export function useAutomationActions(projectId: number | null): AutomationActionCardDto[] {
  const { data } = useQuery<AutomationActionCardDto[]>(
    ["automationActions", projectId ?? null],
    () => (projectId === null ? Promise.resolve([]) : fetchAutomationActions(projectId)),
  );
  return data ?? [];
}

/**
 * **Make a library action** — its name, and which library it lands in.
 *
 * `project` is `null` for the device's library, which every project on this machine reaches, and the
 * project's id for its own. There is no prompt: it is written afterwards in the box the list opens
 * on the new row (`editAutomationAction`), which is the only place a prompt is written.
 */
export async function addAutomationAction(name: string, project: number | null): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_action_add", { name, project });
}

/**
 * Rename a library action, or rewrite its prompt. Only what is passed is written.
 *
 * **The rewrite reaches every step pointing at this action**, which is what a library is for — and
 * why the screen says how many automations that is before the box is opened. A run already under way
 * is not reached: a step's prompt is resolved as the step opens, and what an open step carries is
 * settled.
 */
export async function editAutomationAction(
  id: number,
  patch: { name?: string; prompt?: string },
): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_action_edit", {
    id,
    name: patch.name ?? null,
    prompt: patch.prompt ?? null,
  });
}

/**
 * **Raise a step's own prompt into the library**: make an action of it, move the step's declarations
 * onto that action, and point the step at it.
 *
 * It is the one road from the build screen into the library. The declarations **move** rather than
 * being copied — a step running an action declares nothing of its own — and the picture around the
 * step goes on reading, because an edge and a wire name a way out by its name.
 *
 * `project` is which library it lands in: the project's own, or `null` for the device's, which every
 * project on this machine reaches.
 */
export async function raiseStepToLibrary(
  step: number,
  name: string,
  project: number | null,
): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_action_from_step", { step, project, name });
}

/** One automation's whole definition, or nothing where that id names none. */
export async function fetchAutomation(id: number): Promise<AutomationDetailDto | null> {
  if (!inTauri()) return null;
  return invoke<AutomationDetailDto | null>("automation_detail", { id });
}

/** Subscribing read of one automation's definition. */
export function useAutomation(id: number | null): AutomationDetailDto | null {
  const { data } = useQuery<AutomationDetailDto | null>(
    ["automation", id ?? null],
    () => (id === null ? Promise.resolve(null) : fetchAutomation(id)),
  );
  return data ?? null;
}

/**
 * **Change one step.** Only what is passed is written, and the answer comes back as an ack, so the
 * definition and the launch check are both re-read (`./mutations`).
 *
 * `source` is where the prompt comes from, as one value rather than two fields: a step runs a library
 * action or carries a prompt of its own, and there is no state between the two. Switching it takes
 * the step's ways out, its settings and its inputs with it, which is why the panel says so beside the
 * control (`amenbo_core::ops::automation::step_update`).
 *
 * `model` and `workDir` each take `null` to mean "leave it to the default" — the agent's own model,
 * and a step that names no folder — as against not being passed, which leaves them alone.
 */
export async function editAutomationStep(
  id: number,
  patch: {
    name?: string;
    source?: { action: number } | { prompt: string };
    agent?: string;
    model?: string | null;
    interactive?: boolean;
    workDir?: string | null;
    reportToTask?: boolean;
    history?: boolean;
  },
): Promise<void> {
  if (!inTauri()) return;
  const source = patch.source;
  return invokeAck("automation_step_edit", {
    id,
    name: patch.name ?? null,
    action: source !== undefined && "action" in source ? source.action : null,
    prompt: source !== undefined && "prompt" in source ? source.prompt : null,
    agent: patch.agent ?? null,
    model: patch.model ?? null,
    clearModel: patch.model === null,
    interactive: patch.interactive ?? null,
    workDir: patch.workDir ?? null,
    clearWorkDir: patch.workDir === null,
    reportToTask: patch.reportToTask ?? null,
    history: patch.history ?? null,
  });
}

/**
 * **Answer one setting on one step**, or leave it unanswered with `null`.
 *
 * The answer is already in the shape its kind takes — the screen's control built it
 * (`../screens/automationCfg`) — and travels as the JSON text core keeps.
 */
export async function answerAutomationCfg(
  stepId: number,
  name: string,
  value: string | null,
): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_cfg_answer", { stepId, name, value });
}

/**
 * **Declare another way out of this step.**
 *
 * Every step is born carrying the unnamed way out and the error one, so this is the second and every
 * one after it. `*` is refused as a name — every step is read as carrying that one already.
 *
 * The three declaration families below name a row by **the step and the name**, the way the panel
 * holds it: a setting and an input have no id on screen, an action-backed step's declaration and its
 * answer being folded into the one row a screen draws. Only a step carrying its own prompt may be
 * written on; one that runs a library action reads the action's, and the door refuses rather than
 * rewriting the row that holds its answer.
 */
export async function declareAutomationExit(stepId: number, name: string): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_exit_declare", { stepId, name });
}

/**
 * **Rename one way out**, `null` being the unnamed one at either end.
 *
 * **Every edge and every wire that named the old name is parted from it.** Core leaves them pointing
 * at a name nobody declares rather than rewriting the graph around them, so the parting is visible in
 * the picture — which is where a reader can act on it.
 */
export async function renameAutomationExit(
  stepId: number,
  from: string | null,
  to: string | null,
): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_exit_rename", { stepId, from, to });
}

/** **Take one way out away**, with the outputs declared on it. The error one is refused. */
export async function removeAutomationExit(stepId: number, name: string | null): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_exit_remove", { stepId, name });
}

/**
 * **Declare a setting on this step** — the name it is answered under, the kind of answer it takes,
 * and whether it has to be answered. `options` is the choice list and belongs to `choice` alone.
 */
export async function declareAutomationCfg(
  stepId: number,
  decl: { name: string; kind: CfgKind; required?: boolean; options?: string },
): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_cfg_declare", {
    stepId,
    name: decl.name,
    kind: decl.kind,
    required: decl.required ?? false,
    options: decl.options ?? null,
  });
}

/**
 * **Change a setting's declaration.** `name` names the row and `patch.name` is what it becomes; only
 * what is passed is written.
 *
 * `options` takes `null` to mean "no choice list", as against not being passed, which leaves it
 * alone. **Moving a `choice` to another kind has to clear it in the same call**: a choice list on a
 * kind that would never show it is refused.
 */
export async function editAutomationCfg(
  stepId: number,
  name: string,
  patch: { name?: string; kind?: CfgKind; required?: boolean; options?: string | null },
): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_cfg_edit", {
    stepId,
    name,
    rename: patch.name ?? null,
    kind: patch.kind ?? null,
    required: patch.required ?? null,
    options: patch.options ?? null,
    clearOptions: patch.options === null,
  });
}

/** **Take a setting away**, with the answer written on it. */
export async function removeAutomationCfg(stepId: number, name: string): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_cfg_remove", { stepId, name });
}

/**
 * **Declare an input on this step** — what it takes in, and whether a run may open it with nothing
 * reaching that input. An output belongs to the way out that produced it and is not declared here.
 */
export async function declareAutomationInput(
  stepId: number,
  decl: { name: string; kind: AutomationPortDto["kind"]; required?: boolean },
): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_input_declare", {
    stepId,
    name: decl.name,
    kind: decl.kind,
    required: decl.required ?? false,
  });
}

/**
 * **Change an input's declaration.** Renaming parts every wire that named the old name, for
 * `renameAutomationExit`'s reason.
 */
export async function editAutomationInput(
  stepId: number,
  name: string,
  patch: { name?: string; kind?: AutomationPortDto["kind"]; required?: boolean },
): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_input_edit", {
    stepId,
    name,
    rename: patch.name ?? null,
    kind: patch.kind ?? null,
    required: patch.required ?? null,
  });
}

/** **Take an input away.** The wires that fed it are left where they are, parted. */
export async function removeAutomationInput(stepId: number, name: string): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_input_remove", { stepId, name });
}

/**
 * **Say what fills one of a step's inputs.** Naming the same input twice answers the wire already
 * there rather than drawing a second one, so the control sends what was picked without first taking
 * the old one away.
 */
export async function setAutomationWire(
  from: { stepId: number; exitName?: string; portName: string },
  to: { stepId: number; portName: string },
): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_wire_set", {
    fromStepId: from.stepId,
    fromExitName: from.exitName ?? null,
    fromPortName: from.portName,
    toStepId: to.stepId,
    toPortName: to.portName,
  });
}

/**
 * **Put a step in on a line.** The way out that was pressed comes to point at the new step, and the
 * new step goes on to whatever that way out used to reach — one act, one transaction
 * (`amenbo_core::ops::automation::step_insert`).
 *
 * `exits` and `inputs` are what the dialog took. A step running a library action declares neither,
 * so they are only ever sent for one carrying its own prompt.
 */
export async function insertAutomationStep(
  edgeId: number,
  step: {
    name: string;
    source: { action: number } | { prompt: string };
    agent: string;
    interactive: boolean;
    exits: readonly string[];
    inputs: readonly { name: string; kind: string; required: boolean }[];
  },
): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_step_insert", {
    edgeId,
    name: step.name,
    action: "action" in step.source ? step.source.action : null,
    prompt: "prompt" in step.source ? step.source.prompt : null,
    agent: step.agent,
    model: null,
    interactive: step.interactive,
    exits: [...step.exits],
    inputs: step.inputs.map((one) => [one.name, one.kind, one.required]),
  });
}

/** **Declare what a way out hands on.** It belongs to the way out, not to the step. */
export async function addAutomationOutput(
  exitId: number,
  port: { name: string; kind: string; required: boolean },
): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_output_add", { exitId, ...port });
}

/** **Take a wire away**, leaving the input it fed with nothing reaching it. */
export async function clearAutomationWire(id: number): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_wire_clear", { id });
}

/**
 * Whether this automation could be started, and what is in the way.
 *
 * `folders` are the project's bound folders: whether an agent is installed is asked of the machine
 * the way the empty frame asks it, over the folders this project actually works in
 * (`crate::wake::wake_choices`). **An answer that never came goes on as `null`, not as an empty
 * list** — no step is then judged on its agent at all, because telling a reader to install what they
 * already have is worse than saying nothing about it (`AMB-D-792`).
 */
export async function fetchLaunchCheck(
  id: number,
  projectId: number,
  folders: readonly string[],
): Promise<AutomationLaunchCheckDto | null> {
  if (!inTauri()) return null;
  const agents = await startableAgents(projectId, folders);
  return invoke<AutomationLaunchCheckDto>("automation_launch_check", { id, agents });
}

/**
 * The agent ids this machine can start, over the folders this project works in, or `null` where the
 * probe did not answer.
 *
 * The check asks it and so does the press, and both have to ask the same question: a list that said
 * one thing while a screen was drawn and another when the button was pressed would refuse a launch
 * the screen had just called ready.
 */
async function startableAgents(
  projectId: number,
  folders: readonly string[],
): Promise<string[] | null> {
  return invoke<WakeDto>("wake_choices", { project: projectId, folders: [...folders] })
    .then((wake) => wake.candidates.filter((one) => one.installed).map((one) => one.id))
    .catch(() => null);
}

/**
 * **Start a run of this automation.** Answers the run's id.
 *
 * `workspaceOpen` is this side's to answer and is passed rather than worked out by the host: the
 * workspace is a face of this window in one shape of the app and a window of its own in the other
 * (`AMB-D-753`). Core refuses a launch with it closed, last of the three refusals, and the sentence
 * it raises is what the screen puts in front of the reader.
 *
 * It returns no `WriteAck`. What a launch changes on screen is the pane the run's first step opens
 * in, which arrives as an event (`talk/automationStep`) rather than a query this side would
 * invalidate.
 */
export async function launchAutomation(
  id: number,
  projectId: number,
  folders: readonly string[],
  workspaceOpen: boolean,
): Promise<AutomationRunStartedDto | null> {
  if (!inTauri()) return null;
  const agents = await startableAgents(projectId, folders);
  return invoke<AutomationRunStartedDto>("automation_launch", { id, agents, workspaceOpen });
}

/** Subscribing read of the launch check for one automation. */
export function useLaunchCheck(
  id: number | null,
  projectId: number | null,
  folders: readonly string[],
): AutomationLaunchCheckDto | null {
  const { data } = useQuery<AutomationLaunchCheckDto | null>(
    ["automationLaunchCheck", id ?? null, projectId ?? null, [...folders].join("\n")],
    () =>
      id === null || projectId === null
        ? Promise.resolve(null)
        : fetchLaunchCheck(id, projectId, folders),
  );
  return data ?? null;
}

/**
 * **What is under way right now**, across every project — the rows of the "running" tab.
 *
 * It crosses projects because a terminal does, and this is the one place a reader sees everything
 * that is under way at once. Runs that are `done` are not in it: what a finished run did is reached
 * from the task it worked, never listed here.
 */
export async function fetchLiveRuns(): Promise<AutomationRunCardDto[]> {
  if (!inTauri()) return [];
  return invoke<AutomationRunCardDto[]>("automation_running_page", {});
}

/** Subscribing read of the runs under way. Empty until the first answer lands. */
export function useLiveRuns(): AutomationRunCardDto[] {
  const { data } = useQuery<AutomationRunCardDto[]>(["automationRuns"], fetchLiveRuns);
  return data ?? [];
}

/**
 * **Stop a run now** — what closing the pane a run is drawn in means (`../shell/TerminalPane`).
 *
 * The cleanup is core's and is the same one every other stop goes through: the task the run reserved
 * goes to `todo`, and a line on that task says the run is not coming back
 * (`amenbo_core::ops::automation_stop`).
 *
 * **It is not a `WriteAck` write.** What it moves is a run, a task and a comment, and every screen
 * that draws one of those is already following the change feed — which is how a run started in
 * another project reaches this window in the first place (`./changes`).
 *
 * Answers whether this press was the one that stopped it: a run that had already finished is `false`
 * and not a refusal, the press having been about the pane.
 */
export async function stopRun(run: number): Promise<boolean> {
  if (!inTauri()) return false;
  return invoke<boolean>("automation_run_stop", { runId: run });
}

/**
 * **Ask a run to pause** — pressed on a row of the "running" tab (`../screens/RunningTab`).
 *
 * A step under way cannot be cut in half, so the run goes on until that step reports and settles
 * there; a run with nothing under way pauses on the spot
 * (`amenbo_core::ops::automation_stop::pause`). Not a `WriteAck` write, for `stopRun`'s reason.
 */
export async function pauseRun(run: number): Promise<void> {
  if (!inTauri()) return;
  return invoke<void>("automation_run_pause", { runId: run });
}

/**
 * **Pick a paused run up again**, from the way out its last step left through. It opens a terminal
 * on the spot — nothing caps how many runs may be under way (`AMB-D-947`).
 *
 * Refused for a run that is not paused, which is the answer a reader gets rather than nothing
 * happening: the row they pressed was drawn from a picture that has since moved.
 */
export async function resumeRun(run: number): Promise<void> {
  if (!inTauri()) return;
  return invoke<void>("automation_run_resume", { runId: run });
}
