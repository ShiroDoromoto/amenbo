// The automations screen's own seam: a project's definitions, one definition whole, whether that one
// could be started, the library the actions on it are placed from, one library action whole — and the
// one write that is not the screen's at all, a run being stopped from the pane it is drawn in.
//
// **Three layers, and each write names the one the field lives on** (`AMB-D-949`): a placement is a
// spot on the picture, what it declares is its action's, and what it runs on is that action's step.
//
// It sits beside `core/reads.ts` rather than in it because what it reads is a different shape of
// thing: a task list is paged and an automation is not. An automation is tens of rows, and the build
// screen's picture, its panel and its launch check all walk the same definition — so it is fetched
// whole, once, and every part of the screen reads that one answer. One library action is fetched the
// same way, by the screen its steps are built in (`AMB-T-5315`).
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
  AutomationActionDetailDto,
  AutomationBuiltinDto,
  AutomationCardDto,
  AutomationCfgDto,
  AutomationDetailDto,
  AutomationEdgeDto,
  AutomationLaunchCheckDto,
  AutomationPortDto,
  AutomationRunCardDto,
  AutomationRunHistoryDto,
  AutomationRunStartedDto,
  EveryAutomationCardDto,
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
 * **The automations of every project**, each with the project it is in — the sidebar's list.
 *
 * Its key starts with `automations` like the one project's, so a write that says it moved the
 * automations re-reads both lists.
 */
export function useEveryAutomation(): EveryAutomationCardDto[] {
  const { data } = useQuery<EveryAutomationCardDto[]>(["automations", "everywhere"], () =>
    inTauri() ? invoke<EveryAutomationCardDto[]>("automation_page_everywhere") : Promise.resolve([]),
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
 * **Rename an automation, rewrite its notes, or put it out of the way.** Only what is passed is
 * written.
 *
 * Archiving takes nothing away and stops nothing already running. It keeps a definition nobody
 * launches any more out of a reader's way, and the row stays in the list with the mark on it —
 * which is what the list draws, rather than dropping the row.
 */
export async function editAutomation(
  id: number,
  patch: { name?: string; notes?: string; archived?: boolean },
): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_edit", {
    id,
    name: patch.name ?? null,
    notes: patch.notes ?? null,
    archived: patch.archived ?? null,
  });
}

/**
 * **Delete an automation and everything built into it** — its steps with their declarations and
 * the lines between them.
 *
 * **Refused while a run stands behind it**, naming how many: a run is filed under the automation it
 * was launched from, so core will not let that record lose what was run. The refusal reaches the
 * caller as core's own sentence, which is what the screen draws.
 */
export async function deleteAutomation(id: number): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_remove", { id });
}

/**
 * The library this project reaches — the device's own actions and the project's own, in one list.
 *
 * Both reaches come in one answer because both are one list on screen: what a reader is choosing
 * between is every action this automation could place, and which library holds one is a column.
 *
 * `null` is the device's library alone — the sidebar's list, which has no project to reach from.
 */
export async function fetchAutomationActions(projectId: number | null): Promise<AutomationActionCardDto[]> {
  if (!inTauri()) return [];
  return invoke<AutomationActionCardDto[]>("automation_action_page", { projectId });
}

/** Subscribing read of the library this project reaches — the device's alone for `null`. */
export function useAutomationActions(projectId: number | null): AutomationActionCardDto[] {
  const { data } = useQuery<AutomationActionCardDto[]>(
    ["automationActions", projectId ?? null],
    () => fetchAutomationActions(projectId),
  );
  return data ?? [];
}

/**
 * **The built-ins** (`AMB-D-964`) — Amenbo's own actions, read off the code's definition rather than
 * the library, since a built-in's action is only written the first time one is placed.
 *
 * Its key starts with `automationActions` so a placement, which moves how many automations place one,
 * re-reads it with the library.
 */
export function useAutomationBuiltins(): AutomationBuiltinDto[] {
  const { data } = useQuery<AutomationBuiltinDto[]>(["automationActions", "builtins"], () =>
    inTauri() ? invoke<AutomationBuiltinDto[]>("automation_builtin_page") : Promise.resolve([]),
  );
  return data ?? [];
}

/**
 * **Put a built-in on the picture**, standing on its own — `placeAutomationAction` for a built-in,
 * named by its key. Its library action is written the first time any automation places it.
 */
export async function placeAutomationBuiltin(automationId: number, key: string): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_builtin_place", { automationId, key });
}

/**
 * **Put a built-in in on a line** — `insertAutomationAction` for a built-in: the way out that was
 * pressed comes to point at the new spot, and the new spot goes on to where that way out used to.
 */
export async function insertAutomationBuiltin(edgeId: number, key: string): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_builtin_insert", { edgeId, key });
}

/**
 * **Make a library action** — its name, and which library it lands in.
 *
 * `project` is `null` for the device's library, which every project on this machine reaches, and the
 * project's id for its own. **It is born empty** — no steps, no entry — and the first step, with its
 * prompt, is written in the build screen the press lands in (`../screens/AutomationActionBuildScreen`).
 */
export async function addAutomationAction(name: string, project: number | null): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_action_add", { name, project });
}

/**
 * **Move a library action to another reach** — `null` for the device's library, a project's id for
 * that project's. Core refuses a move into a project while another project's automation places it,
 * naming each; the refusal reaches the caller as core's own sentence
 * (`amenbo_core::ops::automation::action_set_scope`).
 */
export async function setAutomationActionScope(id: number, projectId: number | null): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_action_set_scope", { id, projectId });
}

/**
 * Rename a library action, or rewrite what it is for. The name and the note are all that is the
 * action's own: the prompt and the flags belong to its steps (`editAutomationStep`), and who carries
 * each step out to where the action is placed (`chooseAutomationAgent`).
 *
 * **Every picture standing on this action reads the new name at once**, a placement pointing at it
 * by key — which is what a library is for. The note reaches no launch (`AMB-D-952`).
 */
export async function editAutomationAction(
  id: number,
  patch: { name?: string; note?: string },
): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_action_edit", {
    id,
    name: patch.name ?? null,
    note: patch.note ?? null,
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

/**
 * **Put an action on the picture**, standing on its own with nothing pointing at it yet.
 *
 * It is the road `insertAutomationAction` is not: that one joins a picture already drawn, by the line
 * the `+` was pressed on, and an automation with nothing on it has no line to press. Where a run
 * begins is said separately (`setAutomationEntry`) — putting a box down is not choosing the entry.
 */
export async function placeAutomationAction(automationId: number, actionId: number): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_placement_add", { automationId, actionId });
}

/**
 * **Say which placement a run opens first**, or take the entry away with `null`.
 *
 * Whether the action standing there takes a task — what actually makes it a usable entry — is the
 * launch check's to say. A picture is built in whatever order its author likes, so naming an entry
 * that is not usable yet is allowed and drawn among the reasons a launch is not offered.
 */
export async function setAutomationEntry(id: number, placementId: number | null): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_entry_set", { id, placementId });
}

/** What a way out is said to do: open a placement, close the task, or stop the run. */
/** Which picture a line is drawn on: an automation's boxes are placements, an action's are steps. */
export type Picture = "automation" | "action";

export type EdgeEnds = AutomationEdgeDto["ends"];

/**
 * **Say what happens after one box leaves through one way out** — a placement on an automation, a
 * step inside an action, as `picture` says (`AMB-D-949`).
 *
 * One way out decides one thing, so a second edge on the same one is refused rather than leaving the
 * run to pick between them — which is why the panels edit the edge already there instead of drawing
 * another (`editAutomationEdge`).
 *
 * A new `go` edge is born with the limit core's callers give the silence; nothing is passed here, and
 * the number is then a field on the panel. An `exit` edge — a step inside an action leaving it — names
 * the way out of the action it returns to in `exitTo`, absent for the unnamed one.
 */
export async function addAutomationEdge(
  picture: Picture,
  from: { boxId: number; exitName?: string },
  target: { ends: EdgeEnds; to?: number; exitTo?: string },
): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_edge_add", {
    picture,
    fromId: from.boxId,
    exitName: from.exitName ?? null,
    ends: target.ends,
    toId: target.to ?? null,
    exitTo: target.exitTo ?? null,
  });
}

/**
 * **Change where an edge goes, or how often it may be taken.** Only what is passed is written.
 *
 * `maxTimes` takes `null` to mean "no limit", as against not being passed, which leaves it alone.
 * The way out it hangs on is not a field: that pair is what the edge is, so moving it to another way
 * out is a remove and an add.
 */
export async function editAutomationEdge(
  id: number,
  patch: { ends?: EdgeEnds; to?: number; exitTo?: string; maxTimes?: number | null },
): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_edge_edit", {
    id,
    ends: patch.ends ?? null,
    toId: patch.to ?? null,
    exitTo: patch.exitTo ?? null,
    maxTimes: patch.maxTimes ?? null,
    clearMaxTimes: patch.maxTimes === null,
  });
}

/**
 * **Take away what a way out said it did.** The way out then says nothing: the error one stops the
 * run and calls a person, and any other leaves a run that takes it with nowhere to go — which the
 * launch check names.
 */
export async function removeAutomationEdge(id: number): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_edge_remove", { id });
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

/** One library action's whole definition, or nothing where that id names none. */
export async function fetchAutomationAction(
  id: number,
): Promise<AutomationActionDetailDto | null> {
  if (!inTauri()) return null;
  return invoke<AutomationActionDetailDto | null>("automation_action_detail", { id });
}

/** Subscribing read of one library action's definition. */
export function useAutomationAction(id: number | null): AutomationActionDetailDto | null {
  const { data } = useQuery<AutomationActionDetailDto | null>(
    ["automationAction", id ?? null],
    () => (id === null ? Promise.resolve(null) : fetchAutomationAction(id)),
  );
  return data ?? null;
}

/**
 * **Add a step to an action**, with the ways out and the inputs it is written with.
 *
 * It is the one way into an action whose picture is empty — every other way in is a line to put a
 * step on (`insertAutomationActionStep`). **An empty picture takes this step as its entry**, the
 * first box being the only one a run could open.
 */
export async function addAutomationStep(
  actionId: number,
  step: {
    name: string;
    prompt: string;
    interactive?: boolean;
    exits?: readonly string[];
    inputs?: readonly { name: string; kind: string; required: boolean }[];
  },
): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_step_add", {
    actionId,
    name: step.name,
    prompt: step.prompt,
    interactive: step.interactive ?? false,
    exits: [...(step.exits ?? [])],
    inputs: (step.inputs ?? []).map((one) => [one.name, one.kind, one.required]),
  });
}

/**
 * **Put a step in on a line inside an action.** The way out that was pressed comes to point at the
 * new step, and the new step goes on to whatever that way out used to reach — one act.
 *
 * A step inside an action always carries its own prompt: an action places no actions (`AMB-D-949`),
 * so there is nothing to pick out of the library here.
 */
export async function insertAutomationActionStep(
  edgeId: number,
  step: {
    name: string;
    prompt: string;
    interactive?: boolean;
    exits?: readonly string[];
    inputs?: readonly { name: string; kind: string; required: boolean }[];
  },
): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_action_step_insert", {
    edgeId,
    name: step.name,
    prompt: step.prompt,
    interactive: step.interactive ?? false,
    exits: [...(step.exits ?? [])],
    inputs: (step.inputs ?? []).map((one) => [one.name, one.kind, one.required]),
  });
}

/**
 * **Take a step out of its action**, with what it declared and every line naming it.
 *
 * Losing the entry clears it rather than being refused — an action under construction has to be able
 * to lose any step, and one left without an entry is what the launch check names.
 */
export async function removeAutomationStep(id: number): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_step_remove", { id });
}

/** **Name the step a placement of this action opens first**, or clear it with `null`. */
export async function setAutomationActionEntry(
  actionId: number,
  step: number | null,
): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_action_entry_set", { actionId, step });
}

/**
 * **Change the step one library action opens.** Only what is passed is written, and the answer comes
 * back as an ack, so the definition and the launch check are both re-read (`./mutations`).
 *
 * The fields are the step's: a prompt and the three flags are the terminal's, and an action holds
 * the steps. Writing one reaches every placement of that action, which is what the library is for.
 * Who carries the step out is not among them — that is chosen where the action is placed
 * (`chooseAutomationAgent`, `AMB-D-960`).
 *
 * `workDir` takes `null` to mean a step that names no folder, as against not being passed, which
 * leaves it alone.
 */
export async function editAutomationStep(
  id: number,
  patch: {
    name?: string;
    prompt?: string;
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
 * **Choose who carries one step out at one placement** — the agent, and the model where one is
 * named (`null` is the agent's own default) — or, with `agent` `null`, leave nobody chosen
 * (`AMB-D-960`). The same action placed on two pictures is chosen for apart, step by step.
 */
export async function chooseAutomationAgent(
  placementId: number,
  stepId: number,
  agent: string | null,
  model: string | null,
): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_placement_step_set", { placementId, stepId, agent, model });
}

/** Which of the two declares a way out or an input: the library action, or one step inside it. */
export type Declarer = "action" | "step";

/**
 * **Declare another way out** — of a library action, or of one step inside it.
 *
 * Both are born carrying the unnamed way out and the error one, so this is the second and every one
 * after it. `*` is refused as a name — every declarer is read as carrying that one already.
 *
 * The declaration families below name a row by **the owner and the name**, the way the panels hold
 * it: a setting and an input have no id on screen, a setting's declaration and each placement's
 * answer being folded into the one row a screen draws.
 */
export async function declareAutomationExit(
  owner: Declarer,
  ownerId: number,
  name: string,
): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_exit_declare", { owner, ownerId, name });
}

/**
 * **Rename one way out**, `null` being the unnamed one at either end.
 *
 * **Every edge and every wire that named the old name is parted from it.** Core leaves them pointing
 * at a name nobody declares rather than rewriting the graph around them, so the parting is visible in
 * the picture — which is where a reader can act on it.
 */
export async function renameAutomationExit(
  owner: Declarer,
  ownerId: number,
  from: string | null,
  to: string | null,
): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_exit_rename", { owner, ownerId, from, to });
}

/** **Take one way out away**, with the outputs declared on it. The error one is refused. */
export async function removeAutomationExit(
  owner: Declarer,
  ownerId: number,
  name: string | null,
): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_exit_remove", { owner, ownerId, name });
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
  owner: Declarer,
  ownerId: number,
  decl: { name: string; kind: AutomationPortDto["kind"]; required?: boolean },
): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_input_declare", {
    owner,
    ownerId,
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
  owner: Declarer,
  ownerId: number,
  name: string,
  patch: { name?: string; kind?: AutomationPortDto["kind"]; required?: boolean },
): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_input_edit", {
    owner,
    ownerId,
    name,
    rename: patch.name ?? null,
    kind: patch.kind ?? null,
    required: patch.required ?? null,
  });
}

/** **Take an input away.** The wires that fed it are left where they are, parted. */
export async function removeAutomationInput(
  owner: Declarer,
  ownerId: number,
  name: string,
): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_input_remove", { owner, ownerId, name });
}

/**
 * **Say what fills one of a box's inputs.** Naming the same input twice answers the wire already
 * there rather than drawing a second one, so the control sends what was picked without first taking
 * the old one away.
 */
export async function setAutomationWire(
  picture: Picture,
  from: { boxId: number; exitName?: string; portName: string },
  to: { boxId: number; portName: string },
): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_wire_set", {
    picture,
    fromId: from.boxId,
    fromExitName: from.exitName ?? null,
    fromPortName: from.portName,
    toId: to.boxId,
    toPortName: to.portName,
  });
}

/** Which library an action made at a picture lands in — this machine's, or this project's. */
export type ActionShelf = "device" | "project";

/**
 * **Put a library action in on a line.** The way out that was pressed comes to point at the new
 * spot, and the new spot goes on to whatever that way out used to reach — one act, one transaction.
 * It is what the library in the build screen's panel places.
 */
export async function insertAutomationAction(edgeId: number, actionId: number): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_step_insert", { edgeId, action: actionId });
}

/**
 * **Make an empty action and put it on the picture** — on the line pressed, or on a picture with no
 * line yet — and answer with the action's id, for the screen to go and build it (`AMB-D-956`).
 *
 * It takes a name and a library and nothing else: the inside of an action is its steps, written on
 * the action's own screen. Until one is, the launch check names the action as empty. `null` outside
 * Tauri, where the browser mock holds no automations.
 */
export async function makeAutomationAction(
  into: { edgeId: number } | { automationId: number },
  name: string,
  shelf: ActionShelf,
): Promise<number | null> {
  if (!inTauri()) return null;
  const ack =
    "edgeId" in into
      ? await invokeForAck("automation_placement_insert_new", { edgeId: into.edgeId, name, shelf })
      : await invokeForAck("automation_placement_add_new", {
          automationId: into.automationId,
          name,
          shelf,
        });
  return ack.actions[0] ?? null;
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
 * that is under way at once. A failure nobody has acknowledged stays in it too; what is over and
 * needs nobody is the "history" tab's (`fetchRunHistory`, `AMB-D-955`).
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

/** The one ending the "history" tab can be narrowed to, or all three. */
export type RunHistoryFilter = "all" | "completed" | "failed" | "canceled";

/**
 * **One page of the "history" tab** — completed, canceled, and acknowledged failures, newest first,
 * of one project or, for `null`, of every project (`AMB-D-955`, `AMB-D-954`). `page` counts from 0.
 *
 * A page at a time because the history only grows: the screen holds one page and no more, and the
 * answer says how many runs the whole narrowing holds so the pager can count its pages.
 */
export async function fetchRunHistory(
  filter: RunHistoryFilter,
  projectId: number | null,
  page: number,
): Promise<AutomationRunHistoryDto> {
  if (!inTauri()) return { runs: [], total: 0, pageSize: 20 };
  return invoke<AutomationRunHistoryDto>("automation_history_page", {
    only: filter === "all" ? null : filter,
    projectId,
    page,
  });
}

/**
 * Subscribing read of one page of the history. It sits under the same key as the running tab's, so
 * a run ending — which moves a row from one tab to the other — refreshes both.
 */
export function useRunHistory(
  filter: RunHistoryFilter,
  projectId: number | null,
  page: number,
): AutomationRunHistoryDto | null {
  const { data } = useQuery<AutomationRunHistoryDto>(
    ["automationRuns", "history", filter, projectId, page],
    () => fetchRunHistory(filter, projectId, page),
  );
  return data ?? null;
}

/**
 * **Say a failed run has been seen** — pressed on its row of the "running" tab, which it then leaves
 * for the "history" tab (`amenbo_core::ops::automation_stop::acknowledge`). Not a `WriteAck` write,
 * for `stopRun`'s reason.
 */
export async function acknowledgeRun(run: number): Promise<void> {
  if (!inTauri()) return;
  return invoke<void>("automation_run_acknowledge", { runId: run });
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
