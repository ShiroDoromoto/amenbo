import { useCallback, useEffect, useId, useMemo, useState } from "react";
import type { WakeDto } from "../bindings/bindings";
import { invoke } from "../core/ipc";
import { asTyped } from "../core/keys";
import { onAgentChosen, onAgentsInstalled, wakeRescan } from "./wake";
import { SHELL } from "../talk/terminal";
import { errText, t, tf } from "../core/i18n";
import { Icon } from "../components/Icon";

/**
 * The empty frame on a page with room: the dashed place that says a terminal can be opened here, and
 * the one press that opens it.
 *
 * **It says there is room, and nothing else.** A frame with nothing open on it once carried the
 * project's leftovers as well — work reserved with nobody at it, decisions nobody would close — and
 * that is gone. The reading was true only of a machine that had been running a while:
 * just after the app comes up no session is alive, so everything ever reserved from a pane read as
 * left behind and the frame opened on a standing inventory. A signal that fires always is not one.
 * The fact underneath is real and worth showing; the place for it is the ledger, not this frame.
 *
 * **There is one of these on a page and only where the page has a gap** — that is the face's to decide,
 * and it decides it from the frames (`./TerminalFace`).
 *
 * **What a terminal is opened with is chosen here**, on the frame, and pressed once. The row above
 * the button is every agent Amenbo knows how to start plus the plain shell, with the answer the host
 * arrived at already on — so the common press is the same one press it always was, and choosing
 * something else costs a press and no dialog (`AMB-T-3667`). **The row is asked about in words as
 * well as drawn as pressable**: shape alone leaves it to be worked out, and a reader who takes the
 * names for a label of the button under them never tries one. The question is what the row opens
 * with, not which AI — the shell on it is neither (`AMB-T-3682`). What comes up on is the project's
 * pin where it has one, else what this person last opened with, and the choice made here is kept as
 * the second of those (`../talk/agent` · `crate::wake`).
 *
 * **A reader can put their own command on the row** (`AMB-D-794`). The catalog is a shortcut and not
 * a census — it goes out of date faster than it can be corrected, and it names bare programs where
 * `claude --model opus` is an ordinary thing to want — so under the row there is a name and a command
 * line to fill in, and what is registered stands among the choices like anything else. What is
 * registered runs in a terminal, so the form says what will run before it is saved and the list
 * under the row keeps saying it afterwards: nothing here is composed by Amenbo, and a reader who
 * cannot read the line cannot judge it.
 *
 * **The row holds what this machine has not got as well, folded away** (`AMB-D-792`). An agent that
 * is not installed is drawn greyed and cannot be pressed — pressing it would open a pane on
 * `command not found` — and it is on the row all the same, because a reader who cannot see that
 * Cursor is one of the choices has no way of learning that installing it would give them one. They
 * are folded because they are usually the many: two agents installed out of six leaves four rows
 * that do nothing standing over the two that do.
 *
 * **The first run is the one time nothing is on.** Nobody has chosen, there is more than one thing
 * to choose between, and the frame says so on the button rather than guessing: it reads "choose one"
 * and does not press. A reader is never told why a press did nothing, because there is no press that
 * does nothing (`AMB-T-3686`). A machine with a single startable agent is not that case — one thing
 * to open with is not a question — and neither is a machine with none, where the shell is the whole
 * of what can be started and is on, whatever else the row draws beside it.
 *
 * `onOpen` is what this is for: opening a pane in this project, with the agent that was chosen —
 * {@link SHELL} for a prompt with nothing started at it, and null where the read that would have
 * said never came back, which leaves the answer to be settled on the pane's own side
 * (`../talk/agent`).
 */
/** One thing the row offers: what a press says, what it is called, and — for a command the reader
 *  registered — the line it runs (`AMB-D-794`). A catalogued row has none: what it starts is the
 *  catalog's business, not something a reader wrote and may want to read back. */
type Start = { id: string; label: string; line?: string };

/** The form's two fields while it is open, and which row they belong to: an id to correct, or null
 *  for one being registered. Null instead of the form means it is not open. */
type Draft = { id: string | null; name: string; line: string };

export function EmptySlot({
  folders,
  project,
  onOpen,
}: {
  /** The folders the project being shown is bound to — what the agents are traced across
   *  (`crate::wake`). */
  folders: readonly string[];
  /** The project being shown, or null on a face that has none — which is a face with nothing to
   *  keep an answer against. */
  project: number | null;
  onOpen: (agent: string | null) => void;
}) {
  const folderKey = folders.join("\n");
  // What this machine can start, and what this project has settled on. Read once per project rather
  // than per press: a probe is a login shell reading the reader's own profile (`crate::wake`), and a
  // frame that ran one on every render would pay for it on every keystroke elsewhere on the page.
  const [wake, setWake] = useState<WakeDto | null>(null);
  // Whether the read is still out. It is its own flag because `wake` being null covers two things
  // that are drawn differently — nobody has answered yet, and the answer did not arrive — and a
  // frame that drew them the same would say "not installed" while it was still asking
  // (`AMB-D-792`).
  const [reading, setReading] = useState(true);
  // Which of them the next pane opens with, once the reader has said. Null until they do, because
  // what is on before that is the host's answer and that arrives with the read.
  const [chose, setChose] = useState<string | null>(null);
  // What names the row of choices, for a reader who is hearing the frame rather than seeing it. The
  // question is on the screen, so the row is pointed at it rather than given a second wording of its
  // own — two names for one thing is how the spoken frame and the drawn one drift apart.
  const askId = useId();
  // Whether the ones this machine has not got are unfolded. Folded to begin with, every time: what
  // the frame is for is opening a terminal, and the row that does it is the one that has to be in
  // front of the reader.
  const [shown, setShown] = useState(false);
  // The register form's two fields, or null while it is closed. It is one form for both jobs — the
  // fields are the same two either way, and a second form beside the first would be a second place
  // to look for the one thing (`AMB-D-794`).
  const [draft, setDraft] = useState<Draft | null>(null);
  // Why the last save or removal did not land, in the reader's own language. Kept beside the form
  // rather than thrown: what failed is a sentence about the two fields they are looking at.
  const [failed, setFailed] = useState<string | null>(null);

  // What this machine can start, asked again. It is a probe of a login shell, so it is not run per
  // render — but it is run after every registration, because whether a line can be started is
  // exactly what changed (`crate::wake`).
  const read = useCallback(
    () =>
      invoke<WakeDto>("wake_choices", { project, folders: folderKey === "" ? [] : folderKey.split("\n") }),
    [project, folderKey],
  );

  useEffect(() => {
    let alive = true;
    setWake(null);
    setReading(true);
    setChose(null);
    setDraft(null);
    setFailed(null);
    read()
      .then((said) => { if (alive) setWake(said); })
      // A read that failed leaves the row off and the button alone: the frame's job is to open a
      // pane, and the agent it opens with is settled again on the other side (`../talk/agent`).
      .catch(() => { if (alive) setWake(null); })
      .finally(() => { if (alive) setReading(false); })
    ;
    return () => { alive = false; };
  }, [read]);

  // The two ways the answer changes under a frame that is already drawn (`./wake`). The host asks
  // this machine again behind the window and says so when what it found differs from what was
  // remembered; and a press somewhere else on the page keeps what it opened with as this person's
  // answer, which is the rank this frame came up on. Neither word carries rows, so what both mean is
  // ask again — and the second is why the frame beside a pane just opened stops asking what the
  // person answered to open it (`AMB-T-4357`).
  useEffect(() => {
    let alive = true;
    let stop: (() => void)[] = [];
    const again = () => {
      void read().then((said) => { if (alive) setWake(said); }).catch(() => {});
    };
    void (async () => {
      const off = await Promise.all([onAgentsInstalled(again), onAgentChosen(again)]);
      if (alive) stop = off;
      else for (const one of off) one();
    })().catch(() => {});
    return () => { alive = false; for (const one of stop) one(); };
  }, [read]);

  // The row, in catalog order, in two groups: what this machine can start, and what it has not got
  // (`AMB-D-792`). The plain shell ends the first — it is not an agent and has no row in the
  // catalogue, so it is put here rather than found, and it is the one thing that can never be
  // missing, since a folder always has a prompt (`../talk/agent`).
  //
  // **Both empty until the read comes back, and empty again if it did not.** A frame that has not
  // been told cannot draw a row — the shell alone would say this machine has no agents on it, which
  // is a different answer from having no answer.
  //
  // **What the reader registered goes after the catalog in either group** (`AMB-D-794`): among the
  // usable it comes after the catalogued ones and before the shell, and among the missing it comes
  // last — right above the form, which is where somebody would go to correct the line that did not
  // start.
  const rows = useMemo(() => {
    if (wake === null) return { usable: [] as Start[], missing: [] as Start[] };
    const drawn = wake.offered.flatMap((id) => wake.candidates.filter((one) => one.id === id));
    const start = (one: (typeof drawn)[number]): Start => ({
      id: one.id,
      label: one.label,
      ...(one.line === undefined ? {} : { line: one.line }),
    });
    const catalogued = drawn.filter((one) => one.line === undefined);
    const own = drawn.filter((one) => one.line !== undefined);
    return {
      usable: [
        ...catalogued.filter((one) => one.installed).map(start),
        ...own.filter((one) => one.installed).map(start),
        { id: SHELL, label: t("talk.shell") },
      ],
      missing: [
        ...catalogued.filter((one) => !one.installed).map(start),
        ...own.filter((one) => !one.installed).map(start),
      ],
    };
  }, [wake]);
  // Every registered row, in the order they were registered — the list under the choices, which is
  // where correcting and dropping one lives. It reads off the row rather than off a second command:
  // one read answers what the frame draws and what the list holds.
  const own = useMemo(
    () => [...rows.usable, ...rows.missing].filter((one) => one.line !== undefined),
    [rows],
  );
  // What is on: what the reader said, else what the host arrived at, else — where there is one thing
  // to start — that one, since a row of one is not a choice. It is never one of the missing: the
  // host settles on what it can start and nothing else (`crate::wake::startable`). Null is nobody
  // having chosen: the first run, and the read that has not come back or did not come.
  const on = chose ?? wake?.settled ?? (rows.usable.length === 1 ? rows.usable[0]!.id : null);
  // The first run, and only it: the row is drawn, nothing on it is on, and the button says so. A
  // frame that never heard from the host is not this — it opens, and the pane settles what with.
  const asking = wake !== null && on === null;
  // **The machine was not reached**, which is not the same as it having nothing (`AMB-D-792`). Every
  // row says `installed: false` in this state, so drawing the row as usual would grey all of them at
  // once and tell somebody with four agents on their machine that they installed none. What is
  // drawn instead is that it could not be checked, and the press that checks again.
  const unreached = wake?.reach === "unreachable";

  /** Take what is in the form and keep it — a new registration, or a correction to one that is
   *  already there. The row is read again afterwards, because whether the line can be started is
   *  what the read answers and the line has just changed. */
  const keep = async () => {
    if (draft === null) return;
    const { id, name, line } = draft;
    setFailed(null);
    try {
      if (id === null) await invoke<string>("wake_register", { label: name, line });
      else await invoke<void>("wake_amend", { id, label: name, line });
      setWake(await read());
      setDraft(null);
    } catch (e) {
      setFailed(errText(e));
    }
  };

  /** Ask this machine again after an answer that never came. The row is read again either way: a
   *  press that reached it has put a fresh answer in the settings, and one that did not leaves the
   *  frame saying the same thing it already says. */
  const recheck = async () => {
    setReading(true);
    await wakeRescan().catch(() => false);
    await read()
      .then(setWake)
      .catch(() => setWake(null))
      .finally(() => setReading(false));
  };

  /** Drop a registration. What is on the row moves off it if it was this one — the host stops
   *  answering with an id it can no longer start (`crate::wake::settle`), and the frame should not
   *  be holding one either. */
  const drop = async (id: string) => {
    setFailed(null);
    try {
      await invoke<void>("wake_unregister", { id });
      setWake(await read());
      setChose((was) => (was === id ? null : was));
      setDraft((was) => (was?.id === id ? null : was));
    } catch (e) {
      setFailed(errText(e));
    }
  };

  /** One thing to open with, drawn as a pill. `missing` is a provider this machine has not got: it
   *  stays in the group so the row reads as "these are the choices, some of them not yet here", and
   *  it is not pressable, because pressing it would open a pane on `command not found`. */
  const pill = (one: Start, missing: boolean) => (
    <button
      key={one.id}
      className={`slot__start${one.id === on ? " slot__start--on" : ""}${missing ? " slot__start--missing" : ""}`}
      // Choosing between things, not turning one of them on: what a press does is say which of the
      // row the pane opens with, and exactly one of them is always on.
      role="radio"
      aria-checked={one.id === on}
      // Said rather than left to the greying, which no screen reader reports, and `aria-disabled`
      // rather than `disabled`: a row nothing can reach is a row nobody learns is there.
      aria-disabled={missing || undefined}
      onClick={() => { if (!missing) setChose(one.id); }}
    >
      {/* An agent wears the mark; the plain shell does not. What the row is choosing between
          is what runs in the pane, and one of the choices is nothing running at all — a mark
          on that one would say the shell is an agent of some kind, which is the distinction
          the row exists to draw. It is one mark for all of them and not one each: which
          agent it is, is what the name says. */}
      {one.id !== SHELL && <Icon name="robot" />}
      {one.label}
    </button>
  );

  return (
    <div className="slot slot--empty">
      {/* Still asking. The row's place is held and nothing in it can be pressed: what is not known
          yet is which of these can be started, and a row drawn before the answer would have to say
          something about that (`AMB-D-792`). */}
      {reading && (
        <div className="slot__pick">
          <p className="slot__ask">{t("face.whichStart")}</p>
          <div className="slot__starts slot__starts--waiting" aria-hidden="true">
            <span className="slot__start slot__start--waiting" />
            <span className="slot__start slot__start--waiting" />
          </div>
          <p className="slot__note" role="status">{t("face.startsChecking")}</p>
        </div>
      )}
      {!reading && rows.usable.length + rows.missing.length > 1 && (
        <div className="slot__pick">
          {/* The question the row is an answer to. It is put where the row is put and nowhere else:
              a frame with one thing to open with has nothing to ask about, and asking anyway would
              be a question with one answer. */}
          <p className="slot__ask" id={askId}>{t("face.whichStart")}</p>
          <div className="slot__starts" role="radiogroup" aria-labelledby={askId}>
            {rows.usable.map((one) => pill(one, false))}
            {!unreached && shown && rows.missing.map((one) => pill(one, true))}
          </div>
          {/* The machine was never reached, so nothing here is "not installed" — what is said is
              that it could not be checked, and the press that checks again. The folded group is not
              drawn at all in this state: every row is in it, and unfolding it would grey the whole
              catalog on a machine that may have all of it. */}
          {unreached && (
            <>
              <p className="slot__note" role="status">{t("face.startsUnchecked")}</p>
              <button className="slot__more" onClick={() => void recheck()}>
                {t("face.startsRecheck")}
              </button>
            </>
          )}
          {/* Folded away rather than always drawn: on a machine with two agents the other four would
              take more of the frame than the two that work. It is a press and not a choice, so it is
              outside the group — a button among radios is read as one of them. */}
          {!unreached && rows.missing.length > 0 && (
            <button className="slot__more" aria-expanded={shown} onClick={() => setShown(!shown)}>
              {tf("face.moreStarts", { n: rows.missing.length })}
            </button>
          )}
          {/* The reader's own commands: what is registered, and the way to register one. Outside the
              group above, because none of this chooses anything — a button among radios is read as
              one of them. */}
          <div className="slot__own">
            {own.length > 0 && (
              <>
                <p className="slot__ask" id={`${askId}-own`}>{t("face.startsOwn")}</p>
                <ul className="slot__ownlist" aria-labelledby={`${askId}-own`}>
                  {own.map((one) => (
                    <li key={one.id} className="slot__ownrow">
                      <span className="slot__ownname">{one.label}</span>
                      {/* The line as it was written. It is here for the same reason the form shows
                          it before saving: what runs in the terminal is this and nothing Amenbo
                          composed, so a reader who cannot read it cannot judge it. */}
                      <code className="slot__ownline">{one.line}</code>
                      <button
                        className="slot__more"
                        onClick={() => {
                          setFailed(null);
                          setDraft({ id: one.id, name: one.label, line: one.line ?? "" });
                        }}
                      >
                        {t("face.startEdit")}
                      </button>
                      <button className="slot__more" onClick={() => void drop(one.id)}>
                        {t("face.startRemove")}
                      </button>
                    </li>
                  ))}
                </ul>
              </>
            )}
            {draft === null ? (
              <button
                className="slot__more"
                onClick={() => { setFailed(null); setDraft({ id: null, name: "", line: "" }); }}
              >
                {t("face.startAdd")}
              </button>
            ) : (
              <form
                className="slot__form"
                onSubmit={(e) => { e.preventDefault(); void keep(); }}
              >
                <label className="slot__field">
                  <span>{t("face.startName")}</span>
                  <input
                    {...asTyped}
                    value={draft.name}
                    placeholder={t("face.startNamePh")}
                    onChange={(e) => setDraft({ ...draft, name: e.target.value })}
                    autoFocus
                  />
                </label>
                <label className="slot__field">
                  <span>{t("face.startLine")}</span>
                  <input
                    {...asTyped}
                    value={draft.line}
                    placeholder={t("face.startLinePh")}
                    onChange={(e) => setDraft({ ...draft, line: e.target.value })}
                  />
                </label>
                {/* Said before it is saved, not after it has been started: the line goes to the
                    pane's shell exactly as it stands here (`AMB-D-794`). */}
                {draft.line.trim() !== "" && (
                  <p className="slot__runs">
                    {t("face.startRuns")} <code>{draft.line.trim()}</code>
                  </p>
                )}
                {failed !== null && <p className="slot__failed" role="alert">{failed}</p>}
                <div className="slot__formrow">
                  <button
                    type="submit"
                    className="slot__save"
                    disabled={draft.name.trim() === "" || draft.line.trim() === ""}
                  >
                    {t("face.startSave")}
                  </button>
                  <button type="button" className="slot__more" onClick={() => setDraft(null)}>
                    {t("face.startCancel")}
                  </button>
                </div>
              </form>
            )}
            {/* A removal that did not land has no form to sit under, so it is said here. */}
            {draft === null && failed !== null && (
              <p className="slot__failed" role="alert">{failed}</p>
            )}
          </div>
        </div>
      )}
      {/* Not pressed while nothing is on, and it says which it is rather than explaining a refusal
          afterwards: what the reader has to do is written on the thing they would press. */}
      <button className="slot__open" onClick={() => onOpen(on)} disabled={asking}>
        {asking ? t("face.openPick") : t("face.open")}
      </button>
    </div>
  );
}
