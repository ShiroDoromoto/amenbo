import { useCallback, useEffect, useId, useRef, useState } from "react";
import type {
  AgentModelDto, AgentModelKeptDto, AgentModelListDto, AgentSwitchDto,
} from "../bindings/bindings";
import { invoke } from "../core/ipc";
import { asTyped, isEnterSubmit } from "../core/keys";
import { errText, t, tf } from "../core/i18n";
import { pasteIntoTerminal, sendIntoTerminal } from "../talk/terminal";
import { MANY } from "../talk/models";
import { Icon } from "../components/Icon";

/**
 * The row under a running pane that moves it to another model (`AMB-D-865`).
 *
 * **A running program cannot be told anything from outside it.** The model a pane opened on went on
 * its launch line and was settled before the first byte was drawn; an hour later that line is gone
 * and there is nothing left to pass. What is left is the provider's own command, typed into the pane
 * the way the person would type it — so this row does exactly that and no more, which is the same
 * passing-through the pane itself is (`AMB-D-747`).
 *
 * **The three shapes are the providers', not a design.** One takes the name on the command's line and
 * settles it; two read a name there as a *prompt* and bill for it, so they get the command alone and
 * their own picker opens; one takes the name in that picker's search box. Which is which is the
 * catalog's answer and is never worked out here (`crate::wake::wake_switch`, `AMB-T-4581`).
 *
 * **Nothing is read back off the screen.** Three of the six say in words that the model moved and
 * three change a value on their status line and say nothing; reading either would be Amenbo parsing a
 * provider's screen, which is the thing the pane exists not to do (`AMB-D-747`). So what this row
 * says after a press is what it *did*: the model, where the command settled it, and that the terminal
 * is waiting for a choice where it opened a picker instead.
 *
 * **Before any press it names what this pane is on** (`crate::frames::frame_model`), which is the
 * same kind of fact and not a reading either: it is the name Amenbo put on this pane's launch line,
 * or the one it last settled here. It is asked of the place rather than of the terminal, so that the
 * row and the launch line of the next run cannot disagree — each pane keeps a model of its own
 * (`AMB-T-4698`), and this is the one place a reader can tell two panes of one provider apart. A
 * pane started on no model of its own is named by the provider's own default, where the provider has
 * said one (`crate::agent_models::agent_default_model`). And every one of them goes the moment a
 * provider's own picker is opened over it: what the pane is on from then on is between the person
 * and the provider.
 *
 * **What the press will do is said before it is pressed** — which command goes in, and, for the three
 * that keep the change past this session, which of the reader's own files it lands in. A control that
 * quietly moved somebody's default would be Amenbo writing a provider's settings through the back
 * door, and `AMB-D-440` refuses that at the front.
 *
 * **It is drawn for a catalogued provider and for nothing else.** A pane running a command the reader
 * registered is a pane whose program Amenbo cannot name (`AMB-D-794`), and the plain shell has no
 * model at all — for both, the host answers with no road and this row is not there.
 */
export function PaneModel({ frame, session, agent }: {
  /** The place this row stands in (`../talk/layout`) — what the model it settles is written down
   *  against, so the pane comes back on it next run (`crate::frames::frame_on_model`). */
  frame: string;
  /** The terminal this row types into. */
  session: string;
  /** What is running in it, as the session says it was started (`crate::pty`) — null for a plain
   *  prompt, which is a pane with no provider to ask. */
  agent: string | null;
}) {
  const askId = useId();
  // How this provider is moved, asked without a model: the command, where the name may go, and where
  // the machine keeps the change. Null is both "not asked yet" and "no road" — the row is not drawn
  // either way, and the two are told apart by nothing the reader would see.
  const [how, setHow] = useState<AgentSwitchDto | null>(null);
  // Whether the candidates are on the screen. The list is asked for when it opens and not before: the
  // first ask starts a login shell and the provider on top of it (`crate::agent_models`), which is
  // not a thing to spend on every pane that comes up.
  const [open, setOpen] = useState(false);
  const [models, setModels] = useState<AgentModelDto[] | null>(null);
  const [kept, setKept] = useState<AgentModelKeptDto | null>(null);
  // What is in the box: the narrowing over a long list, and the name itself where the provider has no
  // list to offer.
  const [typed, setTyped] = useState("");
  // What this pane was moved to, where the command settled it on its own. It is what this row *did*
  // and never a reading of the screen: the providers whose picker opens leave it null, because what
  // was chosen in there is between the person and the provider.
  const [now, setNow] = useState<AgentModelDto | null>(null);
  // What this pane is on — what the button says with nothing pressed. Its own model where it was
  // started on one (`crate::frames::frame_model`), and the provider's own default where it was not
  // (`crate::agent_models::agent_default_model`).
  //
  // Null is a pane there is no name to put up for: one whose provider has never said what its own
  // default is, one whose provider's own picker has since been opened, and a pane running a plain
  // shell.
  const [on, setOn] = useState<AgentModelDto | null>(null);
  // The provider's own picker is open and the choosing is the person's. It stands until they come
  // back to this row, which is the one moment it is certainly over.
  const [waiting, setWaiting] = useState(false);
  const [failed, setFailed] = useState<string | null>(null);
  // The row and the box over it together. It is the row and not the box alone that counts as inside:
  // a press on the row's own button would otherwise close the box on the way down and the button's
  // own toggle would open it again on the way up, leaving a control that never shuts.
  const mine = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (agent === null) {
      setHow(null);
      return;
    }
    let alive = true;
    void invoke<AgentSwitchDto | null>("wake_switch", { agent, model: null })
      .then((said) => { if (alive) setHow(said); })
      .catch(() => { if (alive) setHow(null); });
    return () => { alive = false; };
  }, [agent]);

  // What this pane is on, asked as the row comes up — and again for a pane whose provider changed
  // under it, which is a pane that adopted another session.
  //
  // **A pane started on no model of its own is on the provider's default**, and that name is asked
  // for separately because it is a different fact: one is what Amenbo put on this pane's line, the
  // other is what the provider would do left alone. Neither ask starts anything — the second answers
  // out of what the provider said the last time it was asked for its list, and says nothing where
  // nobody has asked yet (`AMB-D-865`).
  //
  // The pane's own name takes a second question besides: the place holds the name that went on the
  // line, and what a reader calls that name is kept beside the choice they made
  // (`crate::wake::wake_chose_model`). A name nothing remembers a word for stands as the provider
  // spells it, which is what went on the line.
  useEffect(() => {
    if (agent === null) {
      setOn(null);
      return;
    }
    let alive = true;
    void (async () => {
      const id = await invoke<string | null>("frame_model", { frame }).catch(() => null);
      if (!alive) return;
      if (id === null) {
        const its = await invoke<AgentModelDto | null>("agent_default_model", { agent }).catch(() => null);
        if (alive) setOn(its);
        return;
      }
      const kept = await invoke<AgentModelKeptDto>("wake_model", { agent }).catch(() => null);
      if (!alive) return;
      const said = [kept?.chosen ?? null, ...(kept?.history ?? [])]
        .find((one) => one !== null && one.id === id);
      setOn(said ?? { id, label: id });
    })();
    return () => { alive = false; };
  }, [frame, agent]);

  const close = useCallback(() => setOpen(false), []);

  // Anything outside the popover closes it, and so does Escape — the same way every small box opened
  // at a press behaves (`../components/Menu`). It is written here rather than borrowed because this
  // box holds a text field: a menu focuses its first item on the way up, which would take the
  // keyboard out of the box the moment a reader with no list to press needed it.
  useEffect(() => {
    if (!open) return;
    const away = (event: Event) => {
      if (event.target instanceof Node && mine.current?.contains(event.target)) return;
      close();
    };
    const key = (event: KeyboardEvent) => { if (event.key === "Escape") close(); };
    document.addEventListener("pointerdown", away);
    document.addEventListener("keydown", key);
    return () => {
      document.removeEventListener("pointerdown", away);
      document.removeEventListener("keydown", key);
    };
  }, [open, close]);

  // Asked once the row is open, and asked again for a pane whose provider changed under it — which is
  // a pane that adopted another session.
  useEffect(() => {
    if (!open || agent === null) return;
    let alive = true;
    setModels(null);
    setKept(null);
    void invoke<AgentModelKeptDto>("wake_model", { agent })
      .then((said) => { if (alive) setKept(said); })
      .catch(() => {});
    // A read that failed is an empty row, the same as a provider that would not answer: the other
    // road is always open, which is to type the provider's own command in the pane (`AMB-D-865`).
    void invoke<AgentModelListDto>("agent_models", { agent })
      .then((said) => { if (alive) setModels(said.models); })
      .catch(() => { if (alive) setModels([]); });
    return () => { alive = false; };
  }, [open, agent]);

  /**
   * Put this provider's own command in the pane, for the model that was pressed.
   *
   * **The line comes back from the host and is not composed here** (`crate::wake::wake_switch`).
   * Whether the name may ride on it is the one thing that costs money to get wrong — Codex and
   * OpenCode were both watched sending `/model <name>` to the model and being billed for the answer
   * (`AMB-T-4581`) — so there is one answer to it and this asks for it.
   *
   * **The second half submits nothing.** Where a provider takes the name in its picker's search box,
   * the name is pasted and left there: what is on the screen at that moment is the provider's, and a
   * return sent into it would be Amenbo confirming a choice the person has not looked at
   * (`AMB-D-793`).
   */
  const move = async (model: AgentModelDto) => {
    if (agent === null) return;
    setFailed(null);
    try {
      const going = await invoke<AgentSwitchDto | null>("wake_switch", { agent, model: model.id });
      if (going === null) return;
      await sendIntoTerminal(session, going.line, agent);
      if (going.then !== null) await pasteIntoTerminal(session, going.then);
      if (going.settles) {
        setNow(model);
        setWaiting(false);
        // Kept against the provider the way a press on an empty frame keeps one, and for the same
        // reason: what somebody last worked with is what the next pane should come up on. It is also
        // the only row of candidates there is for a provider with no list to offer, so a name typed
        // here is a name they can press next time.
        await invoke<void>("wake_chose_model", { agent, model: model.id, label: model.label })
          .catch(() => {});
        // And against this place as well, which is what the pane itself comes back on. The two
        // answers are not the same one: the agent's is what the next pane opened with it starts on,
        // and this is what *this* pane resumes on, so moving one pane does not move the others
        // (`crate::frames::TalkFace::model_on`).
        await invoke<void>("frame_on_model", { frame, model: model.id }).catch(() => {});
      } else {
        // The provider's own picker is open, so whatever this pane was on is no longer something
        // Amenbo can stand behind. The button goes back to saying nothing rather than going on
        // naming the model the pane started on (`AMB-D-747`).
        setOn(null);
        setWaiting(true);
      }
      setOpen(false);
    } catch (e: unknown) {
      // The terminal having ended between the press and the write is the whole of what this can be.
      setFailed(errText(e));
    }
  };

  if (how === null) return null;

  // What the row draws. A short answer is the row itself; a long one is drawn as far as the row goes
  // and the box above it reaches the rest; a provider with no list at all draws what was chosen for
  // it before, which is the whole of what anybody can offer for one.
  const narrowed = (models ?? []).filter((one) =>
    typed.trim() === "" || `${one.id} ${one.label}`.toLowerCase().includes(typed.trim().toLowerCase()));
  const drawn = models !== null && models.length === 0 ? kept?.history ?? [] : narrowed.slice(0, MANY);
  // What a press does, in the provider's own terms. Three sentences because the providers take three
  // roads, and a reader judging the press is judging which of the three they are on.
  const sends = tf(
    how.carries === "named" ? "face.modelSendsNamed"
      : how.carries === "filter" ? "face.modelSendsFilter"
        : "face.modelSendsPicker",
    { command: how.command },
  );

  return (
    <div className="modelrow" ref={mine}>
      <button
        className={`modelrow__now${open ? " modelrow__now--open" : ""}`}
        type="button"
        aria-expanded={open}
        title={t("face.modelSwitch")}
        onClick={() => {
          // Coming back to the row is the one moment the provider's picker is certainly done with.
          setWaiting(false);
          setOpen(!open);
        }}
      >
        <Icon name="robot" label={t("face.modelSwitch")} />
        {now?.label ?? on?.label ?? t("face.modelHere")}
      </button>
      {/* What the row did, in place of the model it cannot claim. The providers whose picker opens
          leave the choosing to the person, and until they have made it there is nothing here that is
          true about the model — so what is said is that the terminal is waiting for one. */}
      {waiting && <span className="modelrow__note" role="status">{t("face.modelPicking")}</span>}
      {failed !== null && <span className="modelrow__failed" role="alert">{failed}</span>}
      {open && (
        <div className="modelpick">
          <p className="slot__ask" id={`${askId}-model`}>{t("face.whichModelNow")}</p>
          {models === null
            ? <p className="slot__note" role="status">{t("face.modelsChecking")}</p>
            : (
              <>
                {/* The box, in the two shapes it takes. Over a long list it narrows; where there is no
                    list at all it is the answer itself, and what was chosen before stands under it as
                    the only candidates anybody has. */}
                {models.length > MANY && (
                  <label className="slot__field">
                    <span>{t("face.modelFind")}</span>
                    <input {...asTyped} autoFocus value={typed} onChange={(e) => setTyped(e.target.value)} />
                  </label>
                )}
                {models.length === 0 && (
                  <label className="slot__field">
                    <span>{t("face.modelName")}</span>
                    <input
                      {...asTyped}
                      autoFocus
                      value={typed}
                      onChange={(e) => setTyped(e.target.value)}
                      onKeyDown={(e) => {
                        if (!isEnterSubmit(e)) return;
                        e.preventDefault();
                        const name = typed.trim();
                        if (name !== "") void move({ id: name, label: name });
                      }}
                    />
                  </label>
                )}
                <div className="slot__starts" role="group" aria-labelledby={`${askId}-model`}>
                  {drawn.map((one) => (
                    <button
                      key={one.id}
                      className="slot__start"
                      type="button"
                      onClick={() => { void move(one); }}
                    >
                      {one.label}
                    </button>
                  ))}
                  {/* The one press for a name nobody can offer a pill for. It is beside the row rather
                      than in the box, because what the box holds is a name and what this is, is
                      sending it. */}
                  {models.length === 0 && typed.trim() !== "" && (
                    <button
                      className="slot__start"
                      type="button"
                      onClick={() => { void move({ id: typed.trim(), label: typed.trim() }); }}
                    >
                      {t("face.composeSend")}
                    </button>
                  )}
                </div>
                {/* Said rather than left to be noticed: a row that stops at its own length looks like
                    the whole answer, and the reader would never learn the box above reaches the rest. */}
                {narrowed.length > drawn.length && models.length > 0 && (
                  <p className="slot__note">{tf("face.modelsMore", { n: narrowed.length - drawn.length })}</p>
                )}
              </>
            )}
          {/* What the press puts in the pane, before it is pressed. */}
          <p className="slot__runs">{sends}</p>
          {/* And where this machine keeps it afterwards, for the providers that keep it anywhere. It
              is the reader's own file, so it is named rather than described. */}
          {how.keeps !== null && (
            <p className="slot__note">{tf("face.modelKeeps", { path: how.keeps })}</p>
          )}
        </div>
      )}
    </div>
  );
}
