// The automations screen's own seam: a project's definitions, one definition whole, whether that one
// could be started, the library the actions on it are placed from — and the one write that is not the
// screen's at all, a run being stopped from the pane it is drawn in.
//
// **Three layers, and each write names the one the field lives on** (`AMB-D-949`): a placement is a
// spot on the picture, what it declares is its action's, and what it runs on is that action's step.
//
// It sits beside `core/reads.ts` rather than in it because what it reads is a different shape of
// thing: a task list is paged and an automation is not. An automation is tens of rows, and the build
// screen's picture, its panel and its launch check all walk the same definition — so it is fetched
// whole, once, and every part of the screen reads that one answer.
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
import { invokeAck } from "./mutations";
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
 * The library this project reaches — the device's own actions and the project's own, in one list.
 *
 * Both reaches come in one answer because both are one list on screen: what a reader is choosing
 * between is every action this automation could place, and which library holds one is a column.
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
 * Rename a library action, and rewrite the prompt the step it opens runs on. Only what is passed is
 * written, and `step` names the row the prompt is on — the action's entry, as the listing hands it
 * back.
 *
 * **The rewrite reaches every placement of this action**, which is what a library is for — and why
 * the screen says how many automations that is before the box is opened. A run already under way is
 * not reached: a run takes its copy at the launch, and what an open step carries is settled.
 */
export async function editAutomationAction(
  id: number,
  patch: { name?: string; step?: number; prompt?: string },
): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_action_edit", {
    id,
    name: patch.name ?? null,
    step: patch.step ?? null,
    prompt: patch.prompt ?? null,
  });
}

/**
 * **Take one action off a picture**, with the answers written on it and every line naming it. The
 * action itself stays in the library: the library outlives any one picture.
 */
export async function removeAutomationPlacement(id: number): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_placement_remove", { id });
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
 * **Change the step one library action opens.** Only what is passed is written, and the answer comes
 * back as an ack, so the definition and the launch check are both re-read (`./mutations`).
 *
 * The fields are the step's: a prompt, who is asked to carry it out, the model and the three flags
 * are the terminal's, and an action holds the steps. Writing one reaches every placement of that
 * action, which is what the library is for.
 *
 * `model` and `workDir` each take `null` to mean "leave it to the default" — the agent's own model,
 * and a step that names no folder — as against not being passed, which leaves them alone.
 */
export async function editAutomationStep(
  id: number,
  patch: {
    name?: string;
    prompt?: string;
    agent?: string;
    model?: string | null;
    interactive?: boolean;
    workDir?: string | null;
    reportToTask?: boolean;
    history?: boolean;
  },
): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_step_edit", {
    id,
    name: patch.name ?? null,
    prompt: patch.prompt ?? null,
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
 * **Answer one setting on one placement**, or leave it unanswered with `null`.
 *
 * The answer is already in the shape its kind takes — the screen's control built it
 * (`../screens/automationCfg`) — and travels as the JSON text core keeps.
 */
export async function answerAutomationCfg(
  placementId: number,
  name: string,
  value: string | null,
): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_cfg_answer", { placementId, name, value });
}

/**
 * **Declare another way out of this action.**
 *
 * Every action is born carrying the unnamed way out and the error one, so this is the second and
 * every one after it. `*` is refused as a name — every action is read as carrying that one already.
 *
 * The three declaration families below name a row by **the action and the name**, the way the panel
 * holds it: a setting and an input have no id on screen, a setting's declaration and each placement's
 * answer being folded into the one row a screen draws.
 */
export async function declareAutomationExit(actionId: number, name: string): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_exit_declare", { actionId, name });
}

/**
 * **Rename one way out**, `null` being the unnamed one at either end.
 *
 * **Every edge and every wire that named the old name is parted from it.** Core leaves them pointing
 * at a name nobody declares rather than rewriting the graph around them, so the parting is visible in
 * the picture — which is where a reader can act on it.
 */
export async function renameAutomationExit(
  actionId: number,
  from: string | null,
  to: string | null,
): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_exit_rename", { actionId, from, to });
}

/** **Take one way out away**, with the outputs declared on it. The error one is refused. */
export async function removeAutomationExit(actionId: number, name: string | null): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_exit_remove", { actionId, name });
}

/**
 * **Declare a setting on this action** — the name it is answered under, the kind of answer it takes,
 * and whether it has to be answered. `options` is the choice list and belongs to `choice` alone. Each
 * placement of the action answers it on a row of its own.
 */
export async function declareAutomationCfg(
  actionId: number,
  decl: { name: string; kind: CfgKind; required?: boolean; options?: string },
): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_cfg_declare", {
    actionId,
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
  actionId: number,
  name: string,
  patch: { name?: string; kind?: CfgKind; required?: boolean; options?: string | null },
): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_cfg_edit", {
    actionId,
    name,
    rename: patch.name ?? null,
    kind: patch.kind ?? null,
    required: patch.required ?? null,
    options: patch.options ?? null,
    clearOptions: patch.options === null,
  });
}

/** **Take a setting away**, leaving the answers written for it on the placements. */
export async function removeAutomationCfg(actionId: number, name: string): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_cfg_remove", { actionId, name });
}

/**
 * **Declare an input on this action** — what it takes in, and whether a run may open a placement of
 * it with nothing reaching that input. An output belongs to the way out that produced it and is not
 * declared here.
 */
export async function declareAutomationInput(
  actionId: number,
  decl: { name: string; kind: AutomationPortDto["kind"]; required?: boolean },
): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_input_declare", {
    actionId,
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
  actionId: number,
  name: string,
  patch: { name?: string; kind?: AutomationPortDto["kind"]; required?: boolean },
): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_input_edit", {
    actionId,
    name,
    rename: patch.name ?? null,
    kind: patch.kind ?? null,
    required: patch.required ?? null,
  });
}

/** **Take an input away.** The wires that fed it are left where they are, parted. */
export async function removeAutomationInput(actionId: number, name: string): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_input_remove", { actionId, name });
}

/**
 * **Say what fills one of a spot's inputs.** Naming the same input twice answers the wire already
 * there rather than drawing a second one, so the control sends what was picked without first taking
 * the old one away.
 */
export async function setAutomationWire(
  from: { placementId: number; exitName?: string; portName: string },
  to: { placementId: number; portName: string },
): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_wire_set", {
    fromPlacementId: from.placementId,
    fromExitName: from.exitName ?? null,
    fromPortName: from.portName,
    toPlacementId: to.placementId,
    toPortName: to.portName,
  });
}

/**
 * **Put an action in on a line.** The way out that was pressed comes to point at the new spot, and
 * the new spot goes on to whatever that way out used to reach — one act, one transaction.
 *
 * `source` is a library action to place, or a prompt to write one from and then place. `exits` and
 * `inputs` are what the dialog took, and belong to the action being written.
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
