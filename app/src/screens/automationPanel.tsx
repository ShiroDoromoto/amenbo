// The controls the build panels are made of — the panel for a spot on an automation
// (`./AutomationStepPanel`), the one for a step inside an action (`./AutomationActionStepPanel`) and
// the one for what the action declares (`./AutomationActionDeclaresPanel`).
//
// **Two layers, one set of controls** (`AMB-D-949`). What a reader does to a declaration is the same
// act whichever layer declares it — a step or the action: type a name, pick what it carries, say
// whether it has to be answered, take it away. So the rows live here, and each panel says which
// layer its presses write on (`../core/automations`). A spot on an automation declares nothing
// (`AMB-D-954`); what it shares with the others is what happens after a way out.
//
// **Every field writes on the spot.** There is no Save: what a reader changed is what the definition
// now says, and a panel with a button would leave a box half-edited every time somebody pressed
// another one in the picture. A box of text writes when the caret leaves it, so a name is not
// written a letter at a time.
import { useEffect, useState } from "react";
import {
  addAutomationEdge,
  editAutomationEdge,
  removeAutomationEdge,
  type CfgKind,
  type EdgeEnds,
  type Picture,
} from "../core/automations";
import { useBoundFolders } from "../core/boundFolders";
import { invoke } from "../core/ipc";
import { inTauri } from "../core/snapshot";
import { t, tf } from "../core/i18n";
import { builtinWord } from "../core/builtinWords";
import { ERROR_EXIT, pictureOrder, type PicGraph } from "./automationLayout";
import type {
  AgentModelListDto,
  AutomationEdgeDto,
  WakeCandidateDto,
  WakeDto,
} from "../bindings/bindings";

/**
 * Send a write and say whether it was taken. Every control on the panel goes through it, so a refusal
 * lands on the one line the panel keeps for it rather than in the console — and a control that has
 * something to do afterwards (emptying the box it was typed in) can wait for the answer.
 */
export type Run = (write: Promise<void> | void) => Promise<boolean>;

/** A name and what it is called, as the two pulldowns that pick a kind take them. */
export type Choice = { id: string; label: string };

/** A way out, as the list of them names it. The unnamed one has no name to put there. */
export function exitLabel(name: string | undefined): string {
  if (name === undefined) return t("auto.step.exitUnnamed");
  return name === ERROR_EXIT ? t("auto.pic.errorExit") : name;
}

/**
 * The kinds of answer a setting may be declared to take, in the order they are offered — core's own
 * (`amenbo_core::model::AutomationCfgKind`). Spelled out rather than built from the id, so the key
 * gate can see every label a reader can be shown (`core/i18n/sourceKeys.test.ts`).
 */
export const CFG_KINDS: readonly { id: CfgKind; label: () => string }[] = [
  { id: "taskfilter", label: () => t("auto.step.cfgKind.taskfilter") },
  { id: "folder", label: () => t("auto.step.cfgKind.folder") },
  { id: "choice", label: () => t("auto.step.cfgKind.choice") },
  { id: "number", label: () => t("auto.step.cfgKind.number") },
  { id: "text", label: () => t("auto.step.cfgKind.text") },
];

/** The kinds to choose between, in the language the panel is being drawn in. */
export function choicesOfKinds(kinds: readonly { id: string; label: () => string }[]): Choice[] {
  return kinds.map((one) => ({ id: one.id, label: one.label() }));
}

/** A box of text that writes when the caret leaves it rather than a letter at a time. */
export function useDraft(value: string): [string, (next: string) => void] {
  const [draft, setDraft] = useState(value);
  useEffect(() => setDraft(value), [value]);
  return [draft, setDraft];
}

/** The agents this project could start a step with, in catalog order. */
export function useAgents(projectId: number | null): WakeCandidateDto[] {
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
export function useModels(agent: string): AgentModelListDto | null {
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
export function DeclareRow({
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
export function DeclEdit({
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

/**
 * What one way out is said to do, in the one word the pulldown holds it under: nothing said, an
 * ending, or the box it opens.
 */
function edgeKey(edge: AutomationEdgeDto | undefined): string {
  if (edge === undefined) return "";
  if (edge.ends === "go") return `go:${edge.toId ?? ""}`;
  // A way out's name is never empty, so the empty one after the colon is the unnamed way out.
  if (edge.ends === "exit") return `exit:${edge.exitTo ?? ""}`;
  return edge.ends;
}

/**
 * **What happens after this way out is taken** — the one row that writes an edge, on either picture
 * (`AMB-D-949`).
 *
 * A way out decides one thing, so there is one edge per way out and the pulldown writes that one:
 * picking where nothing was said adds it, picking again changes it, and picking "nothing said" takes
 * it away. Adding and changing are separate doors because they are separate writes in core, and this
 * row is where the screen knows which of the two it is looking at.
 *
 * **Nothing said is a real answer and not an empty field.** On the error way out it is what stops the
 * run and calls a person; on any other it leaves a run that takes it with nowhere to go, which the
 * launch check names rather than this row refusing it.
 *
 * **The boxes are offered as the picture numbers them**, and a line back up the picture says so
 * (`./automationLayout`'s `pictureOrder`). The box this way out leaves is not offered: a step that
 * wants another go is a line back to where it started, drawn from the step before it.
 *
 * The limit is drawn for a line that goes back within one task alone — the loop it is there to cap.
 * A line on down the picture is taken once per task, and one back to a box that takes a task starts
 * the next task rather than trying this one again. An edge that closes the task or stops the run
 * carries none, and core refuses one there.
 */
export function NextRow({
  graph,
  picture,
  boxId,
  exitName,
  run,
}: {
  /** The picture the line is drawn on, which is where the boxes to go on to are read from. */
  graph: PicGraph;
  picture: Picture;
  /** The box this way out leaves — an edge is the picture's, never the library action's. */
  boxId: number;
  /** The way out it hangs on, `undefined` being the unnamed one. */
  exitName: string | undefined;
  run: Run;
}) {
  const edge = graph.edges.find((one) => one.fromId === boxId && one.exitName === exitName);
  const [limit, setLimit] = useDraft(
    edge === undefined || edge.maxTimes === undefined ? "" : String(edge.maxTimes),
  );
  const order = pictureOrder(graph);
  const numbered = (one: { id: number; name: string; builtin?: string }) =>
    `${order.numberOf.get(one.id) ?? "?"}. ${builtinWord(one.builtin, one.name)}`;
  const self = graph.boxes.find((one) => one.id === boxId);
  const loops =
    edge?.ends === "go" &&
    edge.toId !== undefined &&
    order.goesBack(boxId, edge.toId) &&
    !order.takesTask(edge.toId);
  // In the picture's order rather than the order they were placed in. The box this way out leaves is
  // offered only while a line already goes there, so that the pulldown can still say what is written.
  const others = graph.boxes
    .filter((one) => one.id !== boxId || edge?.toId === boxId)
    .sort((a, b) => (order.numberOf.get(a.id) ?? 0) - (order.numberOf.get(b.id) ?? 0));

  const pick = (key: string) => {
    if (key === "") {
      if (edge !== undefined) void run(removeAutomationEdge(edge.id));
      return;
    }
    const back = key.slice("exit:".length);
    const target = key.startsWith("go:")
      ? { ends: "go" as EdgeEnds, to: Number(key.slice("go:".length)) }
      : key.startsWith("exit:")
        ? { ends: "exit" as EdgeEnds, exitTo: back === "" ? undefined : back }
        : { ends: key as EdgeEnds };
    void run(
      edge === undefined
        ? addAutomationEdge(picture, { boxId, exitName }, target)
        : editAutomationEdge(edge.id, target),
    );
  };

  const writeLimit = () => {
    if (edge === undefined) return;
    const typed = limit.trim();
    const now = typed === "" ? null : Number(typed);
    if (now !== null && !Number.isFinite(now)) return;
    if (now !== (edge.maxTimes ?? null)) void run(editAutomationEdge(edge.id, { maxTimes: now }));
  };

  return (
    <div className="autostep__next">
      <span className="autostep__label">{t("auto.step.next")}</span>
      <select
        aria-label={exitLabel(exitName === undefined ? undefined : builtinWord(self?.builtin, exitName))}
        value={edgeKey(edge)}
        onChange={(e) => pick(e.target.value)}
      >
        <option value="">{t("auto.step.nextNothing")}</option>
        <optgroup
          label={t(
            graph.boundary === undefined ? "auto.step.nextGroupPlacement" : "auto.step.nextGroupStep",
          )}
        >
          {others.map((one) => (
            <option key={one.id} value={`go:${one.id}`}>
              {tf(order.goesBack(boxId, one.id) ? "auto.step.nextGoBack" : "auto.step.nextGo", {
                name: numbered(one),
              })}
            </option>
          ))}
        </optgroup>
        {/* Inside an action a step may leave it, by one of the ways out the action declares — which
            is where the placement standing on it goes on from. An automation's picture has none. */}
        {graph.boundary !== undefined && (
          <optgroup label={t("auto.step.nextGroupExit")}>
            {[...graph.boundary.exits]
              .sort((a, b) => Number(a.name === ERROR_EXIT) - Number(b.name === ERROR_EXIT))
              .map((one) => (
                <option key={`exit:${one.name ?? ""}`} value={`exit:${one.name ?? ""}`}>
                  {tf("auto.step.nextExit", { name: exitLabel(one.name) })}
                </option>
              ))}
          </optgroup>
        )}
        <optgroup label={t("auto.step.nextGroupEnd")}>
          <option value="done">{t("auto.pic.endsDone")}</option>
          <option value="halt">{t("auto.pic.endsHalt")}</option>
        </optgroup>
      </select>
      {loops && (
        <label className="autostep__limit">
          <span className="autostep__label">{t("auto.step.maxTimes")}</span>
          <input
            type="number"
            min={1}
            placeholder={t("auto.step.maxTimesNone")}
            value={limit}
            onChange={(e) => setLimit(e.target.value)}
            onBlur={writeLimit}
          />
        </label>
      )}
    </div>
  );
}
