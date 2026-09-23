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
// **A row opens into the action build screen** (`AMB-T-5315`), where its steps are drawn and its
// prompts written. Making one here asks for a name and a reach and no prompt: an action is born
// empty, and the screen the press lands on is where the words go.
import { useEffect, useMemo, useState } from "react";
import { addAutomationAction, useAutomationActions } from "../core/automations";
import { asTyped } from "../core/keys";
import { errText, t, tf } from "../core/i18n";
import { ErrorNote } from "../components/ErrorNote";
import type { AutomationActionCardDto } from "../bindings/bindings";

/**
 * **The row carries the note's first line, and no more.** It is what tells two actions of like names
 * apart, which one line does; the whole of it is on the build screen a press on the row opens.
 */
function firstLine(note: string): string {
  return note.split("\n").find((line) => line.trim() !== "")?.trim() ?? "";
}

/** Which reach the list is narrowed to. */
type Reach = "all" | "global" | "project";

/** The reach an action sits in, as a chip — the same one the build screen's declaration draws. */
export function ReachChip({ global }: { global: boolean }) {
  return (
    <span className={global ? "actscope actscope--global" : "actscope"}>
      <em aria-hidden="true" />
      {global ? t("auto.actions.reachGlobal") : t("auto.actions.reachProject")}
    </span>
  );
}

function matches(one: AutomationActionCardDto, words: string, reach: Reach): boolean {
  if (reach === "global" && !one.global) return false;
  if (reach === "project" && one.global) return false;
  const w = words.trim().toLowerCase();
  return w === "" || `${one.name} ${one.note}`.toLowerCase().includes(w);
}

export function AutomationActionsTab({
  projectId,
  onOpen,
}: {
  /** Whose library this is, besides the device's — `null` for the sidebar's, the device's alone. */
  projectId: number | null;
  /** Open the build screen on this action — a press on a row, and on the row a press just made. */
  onOpen: (id: number) => void;
}) {
  const actions = useAutomationActions(projectId);
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

  const reaches: { id: Reach; label: string }[] = [
    { id: "all", label: t("auto.actions.all") },
    { id: "global", label: t("auto.actions.reachGlobal") },
    { id: "project", label: t("auto.actions.reachProject") },
  ];

  return (
    <div className="actlib">
      <div className="actlib__head">
        <span className="actlib__sec">{t("auto.actions.library")}</span>
        {!making && (
          <button type="button" className="btn btn--primary" onClick={() => setMaking(true)}>
            {t("auto.actions.add")}
          </button>
        )}
      </div>
      {making && (
        <ActionAdd projectId={projectId} onMake={make} onCancel={() => setMaking(false)} />
      )}
      <div className="actlib__tools">
        <input
          {...asTyped}
          type="search"
          className="actlib__search"
          placeholder={t("auto.actions.search")}
          value={words}
          onChange={(e) => setWords(e.target.value)}
        />
        {projectId !== null && reaches.map((one) => (
          <button
            key={one.id}
            type="button"
            className={reach === one.id ? "actchip actchip--on" : "actchip"}
            aria-pressed={reach === one.id}
            onClick={() => setReach(one.id)}
          >
            {one.label}
          </button>
        ))}
      </div>
      {actions.length === 0 ? (
        <div className="actlib__none">{t("auto.actions.empty")}</div>
      ) : shown.length === 0 ? (
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
              <li key={one.id}>
                <button type="button" className="auto__row actlib__row" onClick={() => onOpen(one.id)}>
                  <span className="auto__name">
                    {one.name}
                    {firstLine(one.note) !== "" && (
                      <span className="auto__note">{firstLine(one.note)}</span>
                    )}
                  </span>
                  <span>
                    <ReachChip global={one.global} />
                  </span>
                  <span className={one.steps === 0 ? "actlib__num actlib__zero" : "actlib__num"}>
                    {one.steps === 0 ? t("auto.actions.noSteps") : one.steps}
                  </span>
                  <span className={one.usedBy === 0 ? "actlib__num actlib__zero" : "actlib__num"}>
                    {one.usedBy === 0
                      ? t("auto.actions.usedNone")
                      : tf("auto.actions.usedN", { n: one.usedBy })}
                  </span>
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}

/**
 * **Make an action from the list itself**, which until now could only be done from the CLI or by
 * raising a step that already carried the words (`AutomationStepPanel`).
 *
 * **It asks for a name and a reach, and no prompt.** The prompt belongs to a step inside the action,
 * and a field here would be writing one before there is a step to write it on — so what this makes
 * is the row, and the press lands in the build screen on it (`./AutomationActionBuildScreen`).
 *
 * **The reach is asked for with nothing picked** (`AMB-D-954`). Whether an action is general or this
 * project's own is read off what it does, which only the person making it knows — so the form leaves
 * the choice open and makes nothing until one is picked, rather than filing it in a reach they did not
 * choose.
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
  const [reach, setReach] = useState<"" | "global" | "project">(projectId === null ? "global" : "");
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
      <label className="actlib__field">
        <span>{t("auto.actions.name")}</span>
        <input {...asTyped} value={name} onChange={(e) => setName(e.target.value)} />
      </label>
      <label className="actlib__field">
        <span>{t("auto.actions.reach")}</span>
        <select
          value={reach}
          onChange={(e) => setReach(e.target.value as "" | "global" | "project")}
        >
          {projectId !== null && <option value="">{t("auto.actions.pickReach")}</option>}
          {projectId !== null && <option value="project">{t("auto.actions.reachProject")}</option>}
          <option value="global">{t("auto.actions.reachGlobal")}</option>
        </select>
      </label>
      <button
        type="button"
        className="btn btn--primary"
        disabled={
          making || name.trim() === "" || reach === ""
        }
        onClick={() => void make()}
      >
        {t("auto.actions.add")}
      </button>
      <button type="button" className="btn" onClick={onCancel}>
        {t("auto.actions.cancel")}
      </button>
      <p className="actlib__said">{t("auto.actions.makeSaid")}</p>
      {error !== null && <ErrorNote>{error}</ErrorNote>}
    </div>
  );
}
