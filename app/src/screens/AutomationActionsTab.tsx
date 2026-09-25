// The library, as the "actions" tab draws it — every prompt a step in this project can be pointed
// at, and what rewriting one would reach.
//
// **One list, two reaches.** An action lives either in the device's own library, which every project
// on this machine reaches, or in one project's. They are drawn as one list with a column saying
// which, because what a reader is choosing between is the prompts themselves: splitting them into
// two lists would make them go and look in both to answer "is this already in here".
//
// **The count is the whole warning.** A prompt rewritten here is carried by every step pointing at
// this action, so each row says how many automations that is before the box is opened. It counts
// automations rather than steps: two steps of one automation is one automation whose runs change,
// and what a reader weighs is how far the rewrite carries, not how often the pointer occurs.
//
// **Under the name, the row carries the first line of what the action is for** (`AMB-D-952`) — the
// note written on the build screen, which is what tells two actions of like names apart.
//
// **It is searched and narrowed in place** — by what the name and the note say, and by reach — since
// a library grows past what a reader scans by eye, and the two reaches stay one list either way.
//
// **Opened from the sidebar, it is the device's library alone** (`AMB-D-954`), with no project to
// reach from: a global action is made and changed there and nowhere else, and a project's own are
// its project's. So the reach narrowing has nothing to narrow, and what is made is global.
//
// **A reach is moved from the row, on the entrance that owns it now** (`AMB-D-954`): a project's
// own action goes to the device's library from that project, and a global one goes into a project
// from the sidebar, which asks which. A move into a project that another project's automation still
// places is refused by core, naming each — the row keeps the sentence, and nothing is copied.
//
// **A row opens into the action build screen** (`AMB-T-5315`), where its steps are drawn and its
// prompts written. Making one here asks for a name and a reach and no prompt: an action is born
// empty, and the press that makes it is named for the screen it lands on ("make and open"), so no
// sentence under the form has to say where the words go.
//
// **The built-ins are a third reach** (`AMB-D-964`), at both entrances: Amenbo's own actions, listed
// from the code's definition after the library's rows. Nothing moves one or builds one, so a row
// carries the lock and no menu, and a press on it opens it to be read (`./AutomationBuiltinScreen`).
//
// **Shape says it, not a sentence** (`AMB-T-5525`). The narrowing is a square segmented switch,
// apart from the round reach chip a row wears, so what can be pressed and what only labels do not
// look alike; moving and deleting sit behind a row's "⋯" and are both confirmed under the row the same
// way; an action with no step says "empty" in the colour of something that will stop a launch.
import { useEffect, useMemo, useState } from "react";
import {
  addAutomationAction,
  removeAutomationAction,
  setAutomationActionScope,
  useAutomationActions,
  useAutomationBuiltins,
} from "../core/automations";
import { dataAdapter } from "../mock/adapter";
import { asTyped } from "../core/keys";
import { errText, t } from "../core/i18n";
import { builtinShown } from "../core/builtinWords";
import { ErrorNote } from "../components/ErrorNote";
import { Icon } from "../components/Icon";
import { Menu, MenuItem } from "../components/Menu";
import { LockMark, ReachChip, usedCount } from "./automationParts";
import type { AutomationActionCardDto } from "../bindings/bindings";

/**
 * **The row carries the note's first line, and no more.** It is what tells two actions of like names
 * apart, which one line does; the whole of it is on the build screen a press on the row opens. The
 * automations tab's rows carry an automation's notes the same way (`./AutomationsScreen`).
 */
export function firstLine(note: string): string {
  return note.split("\n").find((line) => line.trim() !== "")?.trim() ?? "";
}

/** Which reach the list is narrowed to. */
type Reach = "all" | "global" | "project" | "builtin";

function matches(one: AutomationActionCardDto, words: string, reach: Reach): boolean {
  if (reach === "builtin") return false;
  if (reach === "global" && !one.global) return false;
  if (reach === "project" && one.global) return false;
  return said(`${one.name} ${one.note}`, words);
}

/** Whether what a row says holds the words typed in the box. */
function said(text: string, words: string): boolean {
  const w = words.trim().toLowerCase();
  return w === "" || text.toLowerCase().includes(w);
}

/**
 * **A square switch of a few words, one of them lit** — the narrowing, and the reach a new action is
 * made in. Square on purpose: the reach a row is in is a round chip that cannot be pressed, and two
 * things of one shape would be read as one kind of thing.
 */
function Segments<T extends string>({
  label,
  options,
  value,
  onPick,
}: {
  label: string;
  options: readonly { id: T; label: string }[];
  /** The lit one, or `null` while none is — which is how the make form asks for a reach. */
  value: T | null;
  onPick: (id: T) => void;
}) {
  return (
    <span className="actseg" role="group" aria-label={label}>
      {options.map((one) => (
        <button
          key={one.id}
          type="button"
          className={value === one.id ? "actseg__one actseg__one--on" : "actseg__one"}
          aria-pressed={value === one.id}
          onClick={() => onPick(one.id)}
        >
          {one.label}
        </button>
      ))}
    </span>
  );
}

export function AutomationActionsTab({
  projectId,
  onOpen,
  onOpenBuiltin,
}: {
  /** Whose library this is, besides the device's — `null` for the sidebar's, the device's alone. */
  projectId: number | null;
  /** Open the build screen on this action — a press on a row, and on the row a press just made. */
  onOpen: (id: number) => void;
  /** Open a built-in to be read, by its key. */
  onOpenBuiltin: (key: string) => void;
}) {
  const actions = useAutomationActions(projectId);
  const builtins = useAutomationBuiltins();
  // The ids the library held when Make was pressed, while the row that was made is still on its way.
  // Nothing while none is.
  const [born, setBorn] = useState<ReadonlySet<number> | null>(null);
  const [making, setMaking] = useState(false);
  const [words, setWords] = useState("");
  const [reach, setReach] = useState<Reach>("all");
  const shown = useMemo(
    () => actions.filter((one) => matches(one, words, reach)),
    [actions, words, reach],
  );
  const shownBuiltins = useMemo(
    () =>
      reach === "all" || reach === "builtin"
        ? builtins.map(builtinShown).filter((one) => said(`${one.name} ${one.does}`, words))
        : [],
    [builtins, words, reach],
  );

  // **The new row is the one the library did not hold before.** A write answers with an ack — the
  // ids it touched and what to re-read, never a body — so which row was made is read off the list
  // that came back, the way a new project is picked out of the refreshed snapshot
  // (`core/mutations.createProject`).
  useEffect(() => {
    if (born === null) return;
    const fresh = actions.find((one) => !born.has(one.id));
    if (fresh === undefined) return;
    setBorn(null);
    onOpen(fresh.id);
  }, [actions, born, onOpen]);

  async function make(name: string, project: number | null) {
    const before = new Set(actions.map((one) => one.id));
    await addAutomationAction(name, project);
    setMaking(false);
    setBorn(before);
  }

  const form = making && (
    <ActionAdd projectId={projectId} onMake={make} onCancel={() => setMaking(false)} />
  );

  // **Nothing to list is one press in the middle**, the way an empty list of automations is: a
  // sentence saying the library is empty would only be telling the reader what they can see.
  if (actions.length === 0 && builtins.length === 0) {
    return (
      <div className="actlib">
        {form || (
          <div className="actlib__none">
            <button type="button" className="btn btn--primary" onClick={() => setMaking(true)}>
              <Icon name="plus" /> {t("auto.actions.makeFirst")}
            </button>
          </div>
        )}
      </div>
    );
  }

  const reaches: { id: Reach; label: string }[] = [
    { id: "all", label: t("auto.actions.all") },
    { id: "project", label: t("auto.actions.reachProject") },
    { id: "global", label: t("auto.actions.reachGlobal") },
    ...(builtins.length > 0
      ? [{ id: "builtin" as const, label: t("auto.actions.reachBuiltin") }]
      : []),
  ];

  return (
    <div className="actlib">
      <div className="actlib__tools">
        <input
          {...asTyped}
          type="search"
          className="actlib__search"
          placeholder={t("auto.actions.search")}
          value={words}
          onChange={(e) => setWords(e.target.value)}
        />
        {projectId !== null && (
          <Segments label={t("auto.actions.reach")} options={reaches} value={reach} onPick={setReach} />
        )}
        {!making && (
          <button type="button" className="btn actlib__make-open" onClick={() => setMaking(true)}>
            <Icon name="plus" /> {t("auto.actions.make")}
          </button>
        )}
      </div>
      {form}
      {shown.length === 0 && shownBuiltins.length === 0 ? (
        <div className="actlib__none">{t("auto.actions.noMatch")}</div>
      ) : (
        <div className="actlib__table">
          <div className="actlib__cols" aria-hidden="true">
            <span>{t("auto.actions.name")}</span>
            <span>{t("auto.actions.reach")}</span>
            <span className="actlib__num">{t("auto.actions.colSteps")}</span>
            <span className="actlib__num">{t("auto.actions.colUsed")}</span>
          </div>
          <ul className="actlib__rows">
            {shown.map((one) => (
              <ActionRow key={one.id} action={one} projectId={projectId} onOpen={onOpen} />
            ))}
            {shownBuiltins.map((one) => (
              <li key={one.key}>
                <div className="actlib__line">
                  <button type="button" className="auto__row actlib__row" onClick={() => onOpenBuiltin(one.key)}>
                    <span className="auto__name">
                      <span className="actlib__title">
                        {one.name}
                        <LockMark />
                      </span>
                      <span className="auto__note">{one.does}</span>
                    </span>
                    <span>
                      <ReachChip global builtin />
                    </span>
                    {/* A built-in is one thing Amenbo does, not steps a reader counts. */}
                    <span className="actlib__num actlib__zero">—</span>
                    <span className={one.usedBy === 0 ? "actlib__num actlib__zero" : "actlib__num"}>
                      {usedCount(one.usedBy)}
                    </span>
                  </button>
                  {/* Nothing moves or deletes a built-in. */}
                  <span className="actlib__moveslot" />
                </div>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}

/** What a row's "⋯" was used for, while it waits under the row to be confirmed. */
type Pending = "toGlobal" | "toProject" | "remove";

/**
 * **One action of the library, with its "⋯".** Only the entrance that owns it now moves or deletes it
 * (`AMB-D-954`): a project its own, the sidebar a global one — on any other row the slot stands empty,
 * so the columns stay under their heads.
 *
 * **Every item is picked, then confirmed under the row, the same way**, so a press that looks like
 * another never takes a different number of presses to act: all three wait under the row for a
 * second press, and moving into a project asks there which one.
 *
 * A refusal is core's sentence and stays under the row until the next press: it names the automations
 * of other projects that place the action, or how many placements stand on one being deleted, which is
 * what a reader needs in front of them to decide again (`amenbo_core::ops::automation`).
 */
function ActionRow({
  action,
  projectId,
  onOpen,
}: {
  action: AutomationActionCardDto;
  projectId: number | null;
  onOpen: (id: number) => void;
}) {
  const owned = (projectId === null) === action.global;
  const [menuAt, setMenuAt] = useState<{ x: number; y: number } | null>(null);
  const [pending, setPending] = useState<Pending | null>(null);
  const [to, setTo] = useState("");
  const [busy, setBusy] = useState(false);
  const [refused, setRefused] = useState<string | null>(null);

  const pick = (what: Pending) => {
    setMenuAt(null);
    setRefused(null);
    setTo("");
    setPending(what);
  };
  const cancel = () => {
    setPending(null);
    setTo("");
    setRefused(null);
  };
  const confirm = async () => {
    setRefused(null);
    setBusy(true);
    try {
      if (pending === "remove") await removeAutomationAction(action.id);
      else await setAutomationActionScope(action.id, pending === "toProject" ? Number(to) : null);
      setPending(null);
      setTo("");
    } catch (err) {
      setRefused(errText(err));
    } finally {
      setBusy(false);
    }
  };

  const steps = action.steps === 0 ? (
    <span className="actlib__num actlib__empty">{t("auto.actions.stepsEmpty")}</span>
  ) : (
    <span className="actlib__num">{action.steps}</span>
  );

  return (
    <li>
      <div className="actlib__line">
        <button type="button" className="auto__row actlib__row" onClick={() => onOpen(action.id)}>
          <span className="auto__name">
            {action.name}
            {firstLine(action.note) !== "" && (
              <span className="auto__note">{firstLine(action.note)}</span>
            )}
          </span>
          <span>
            <ReachChip global={action.global} />
          </span>
          {steps}
          <span className={action.usedBy === 0 ? "actlib__num actlib__zero" : "actlib__num"}>
            {usedCount(action.usedBy)}
          </span>
        </button>
        <span className="actlib__moveslot">
          {owned && (
            <button
              type="button"
              className="actlib__more"
              title={t("auto.actions.more")}
              aria-label={t("auto.actions.more")}
              aria-haspopup="menu"
              onClick={(e) => {
                // Under the button, not at the pointer: a press from the keyboard has no pointer.
                const box = e.currentTarget.getBoundingClientRect();
                setMenuAt({ x: box.left, y: box.bottom });
              }}
            >
              <Icon name="more" />
            </button>
          )}
        </span>
        {menuAt !== null && (
          <Menu at={menuAt} onClose={() => setMenuAt(null)}>
            {projectId !== null ? (
              <MenuItem onClick={() => pick("toGlobal")}>{t("auto.actions.toGlobal")}</MenuItem>
            ) : (
              <MenuItem onClick={() => pick("toProject")}>{t("auto.actions.toProject")}</MenuItem>
            )}
            <MenuItem apart onClick={() => pick("remove")}>{t("auto.actions.remove")}</MenuItem>
          </Menu>
        )}
      </div>
      {/* Under the row, the width of it: the second press, which project, and what core said. */}
      {(pending !== null || refused !== null) && (
        <div className="actlib__moveplace">
          {pending === "toProject" && (
            <select aria-label={t("auto.actions.toWhich")} value={to} onChange={(e) => setTo(e.target.value)}>
              <option value="">{t("auto.actions.toWhich")}</option>
              {dataAdapter.listProjects().map((p) => (
                <option key={p.id} value={String(p.id)}>{p.name}</option>
              ))}
            </select>
          )}
          {pending !== null && (
            <>
              <button
                type="button"
                className={pending === "remove" ? "btn btn--danger" : "btn btn--primary"}
                disabled={busy || (pending === "toProject" && to === "")}
                onClick={() => void confirm()}
              >
                {pending === "remove"
                  ? t("auto.actions.remove")
                  : pending === "toGlobal"
                    ? t("auto.actions.toGlobal")
                    : t("auto.actions.move")}
              </button>
              <button type="button" className="btn" onClick={cancel}>
                {t("auto.actions.cancel")}
              </button>
            </>
          )}
          {refused !== null && <ErrorNote>{refused}</ErrorNote>}
        </div>
      )}
    </li>
  );
}

/**
 * **Make an action from the list itself**, which until now could only be done from the CLI or by
 * raising a step that already carried the words (`AutomationStepPanel`).
 *
 * **It asks for a name and a reach, and no prompt.** The prompt belongs to a step inside the action,
 * and a field here would be writing one before there is a step to write it on — so what this makes
 * is the row, and the press lands in the build screen on it (`./AutomationActionBuildScreen`), which
 * is what its label says.
 *
 * **The reach is asked for with nothing picked** (`AMB-D-954`). Whether an action is general or this
 * project's own is read off what it does, which only the person making it knows — so the switch
 * starts with neither lit and makes nothing until one is, rather than filing it in a reach they did
 * not choose. From the sidebar there is one reach to make in, so there is no switch.
 */
function ActionAdd({
  projectId,
  onMake,
  onCancel,
}: {
  projectId: number | null;
  onMake: (name: string, project: number | null) => Promise<void>;
  onCancel: () => void;
}) {
  const [name, setName] = useState("");
  // With no project there is one reach to make in, so it is the one already picked.
  const [reach, setReach] = useState<"global" | "project" | null>(projectId === null ? "global" : null);
  const [making, setMaking] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const make = async () => {
    setError(null);
    setMaking(true);
    try {
      await onMake(name.trim(), reach === "global" ? null : projectId);
    } catch (err) {
      setError(errText(err));
    } finally {
      setMaking(false);
    }
  };

  return (
    <div className="actlib__make">
      <input
        {...asTyped}
        className="actlib__makename"
        aria-label={t("auto.actions.name")}
        placeholder={t("auto.actions.name")}
        value={name}
        onChange={(e) => setName(e.target.value)}
      />
      {projectId !== null && (
        <Segments
          label={t("auto.actions.reach")}
          options={[
            { id: "project" as const, label: t("auto.actions.reachProject") },
            { id: "global" as const, label: t("auto.actions.reachGlobal") },
          ]}
          value={reach}
          onPick={setReach}
        />
      )}
      <button
        type="button"
        className="btn btn--primary"
        disabled={making || name.trim() === "" || reach === null}
        onClick={() => void make()}
      >
        {t("auto.actions.makeOpen")}
      </button>
      <button type="button" className="btn" onClick={onCancel}>
        {t("auto.actions.cancel")}
      </button>
      {error !== null && <ErrorNote>{error}</ErrorNote>}
    </div>
  );
}
