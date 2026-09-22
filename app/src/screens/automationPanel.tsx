// The controls both build panels are made of — the panel for a spot on an automation
// (`./AutomationStepPanel`) and the one for a step inside an action (`./AutomationActionStepPanel`).
//
// **Two layers, one set of controls** (`AMB-D-949`). What a reader does to a declaration is the same
// act whichever layer declares it: type a name, pick what it carries, say whether it has to be
// answered, take it away. So the rows live here, and each panel says which layer its presses write
// on (`../core/automations`).
//
// **Every field writes on the spot.** There is no Save: what a reader changed is what the definition
// now says, and a panel with a button would leave a box half-edited every time somebody pressed
// another one in the picture. A box of text writes when the caret leaves it, so a name is not
// written a letter at a time.
import { useEffect, useState } from "react";
import { useBoundFolders } from "../core/boundFolders";
import { invoke } from "../core/ipc";
import { inTauri } from "../core/snapshot";
import { t } from "../core/i18n";
import { ERROR_EXIT } from "./automationLayout";
import type { CfgKind } from "../core/automations";
import type { AgentModelListDto, WakeCandidateDto, WakeDto } from "../bindings/bindings";

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
