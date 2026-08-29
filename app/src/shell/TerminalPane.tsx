import { useEffect, useRef, useState } from "react";
import { mountAgentFrame } from "../talk/agent";
import { endTerminal, pasteIntoTerminal, quotedPath } from "../talk/terminal";
import { mountPlate, type Plate } from "../talk/plate";
import { confirmDialog, pickFiles } from "../core/dialog";
import { watchHostDrop } from "../core/hostDrop";
import { folderInbox } from "../files/folder";
import { stoppedLine } from "../files/stopped";
import { pushNotice } from "../core/notice";
import { Menu, MenuItem } from "../components/Menu";
import { PaneDropAsk } from "./PaneDropAsk";
import type { FrameNames, NamedBy } from "../talk/frames";
import type { PaneStart } from "../talk/terminal";
import type { SessionSaidDto, SessionWorkDto } from "../bindings/bindings";
import { currentLang, errText, t } from "../core/i18n";
import { invoke } from "../core/ipc";
import { setStatus } from "../core/mutations";
import { Icon } from "../components/Icon";

/**
 * What the volatile area says this session is holding, asked at the moment the way out is pressed
 * (`commands.rs::session_work`).
 *
 * **Read at the press and not kept.** The answer is only worth having about the instant it is acted
 * on — a reservation made in this pane a second ago is exactly the one nobody would think to look
 * for.
 *
 * **Silence is no reservations, and that is the honest answer.** A pane with nothing running in it,
 * a window not running under Tauri, a read that failed: what none of them can say is that something
 * is being left behind, and a question raised on a guess would be a question about nothing
 * (`AMB-D-758` — a move made outside a pane is not written here at all, and may not be guessed back).
 */
async function heldHere(session: string | null): Promise<readonly number[]> {
  if (session === null) return [];
  return invoke<SessionWorkDto>("session_work", { session })
    .then((work) => work.holding)
    .catch(() => []);
}

/**
 * Take what was dropped on a pane into the project's own inbox, and put where it landed in front of
 * whatever is running there (`AMB-D-800`).
 *
 * **The folder is the one the session was opened in**, which is a folder the project is bound to —
 * the host proves it against the store before writing anything (`crate::folder_write::folder_inbox`).
 * Where inside it the files go is Amenbo's own answer and not this side's, which is why what comes
 * back is whole paths: a name the day already held is numbered rather than refused, so what arrived
 * is not always what was dropped.
 *
 * **Nothing is typed for the reader.** The paths are pasted and the newline is not sent
 * (`../talk/terminal`), so what happens next is theirs.
 *
 * Each path is quoted on its own. A screenshot's name has spaces in it on all three machines, and
 * with several of them the space between two paths would otherwise be the same character as the
 * space inside one (`AMB-D-801`).
 */
async function handOver(project: number, folder: string, session: string, paths: string[]) {
  try {
    const inboxed = await folderInbox(project, folder, paths);
    if (inboxed.arrived.length > 0) {
      await pasteIntoTerminal(session, inboxed.arrived.map((one) => quotedPath(one)).join(" "));
    }
    const line = stoppedLine(inboxed);
    if (line !== null) pushNotice(line);
  } catch (e: unknown) {
    // The host's own sentence: the folder having gone since the pane opened in it is the whole of
    // what this can be, and it is worth saying rather than swallowing.
    pushNotice(errText(e));
  }
}

/**
 * One slot of the terminal face: a frame, and the terminal in it when there is one.
 *
 * **A frame is a place, so an empty one is not nothing.** It is a slot on this page with a way to
 * open a terminal in it, and it stays a place after the program in it exits — what is on the screen
 * is what a terminal ends with, and taking the pane away would be the app deciding the reader had
 * finished reading it. So this puts the frame up once and then keeps it, whatever happens to the
 * process: what runs in it, what is offered when nothing can be started, and the row a closed frame
 * carries are all the frame's (`../talk/agent`).
 *
 * The pane comes down when the slot stops being on the screen — the page turned, or fewer panes were
 * asked for — and **the terminal does not**: a pane is a drawing of a session, and detaching leaves
 * the session running for whichever slot draws it next (`../talk/terminal`). That is why the slot's
 * session id is handed back up: the frame is what remembers, and this is only what draws.
 *
 * **The one control on the row removes the place**, which is the only thing on this face that does
 * (`../talk/layout`). It is not the same act as a program ending: what a terminal exits with stays on
 * the screen to be read, and a page that closed up under a reader because a shell finished would be
 * the app deciding they were done with it. So this is asked before it happens, and it ends whatever is
 * running on the way out — a session whose place has gone is one nobody can reach.
 */
export function TerminalPane({
  frame, project, names, start, autoStart, focused,
  onOpened, onSaid, onPath, onClosed, onDrop, onName, onFocus, onWaiting,
}: {
  /** Which of the arrangement's places this is (`../talk/layout`). */
  frame: string;
  /** Which project this place belongs to — whose answer the agent it opens with is kept against
   *  (`../talk/agent`). */
  project: number;
  /** What every frame is called, so a naming from anywhere reaches this row. */
  names: FrameNames;
  /** Which terminal to draw here, and where to start one. */
  start: PaneStart;
  /** True for the slot that puts a terminal up without being asked — the one the face comes up with,
   *  and the one a person has just pressed the way in on. */
  autoStart: boolean;
  focused: boolean;
  onOpened: (frame: string, session: string, folder: string | null) => void;
  onSaid: (statement: SessionSaidDto) => void;
  /** A file path drawn in this pane was clicked, as it was drawn. */
  onPath: (frame: string, target: string) => void;
  onClosed: (session: string) => void;
  /** Take this place away — the frame and not the program in it (`../talk/layout`). */
  onDrop: (frame: string) => void;
  onName: (frame: string, name: string, by: NamedBy) => void;
  onFocus: (frame: string) => void;
  /** Whether a turn is standing in this pane. The face gathers them: behind the ledger no label can
   *  be seen at all, so what the shell badges is the face and not a pane (`./terminalBadge`). */
  onWaiting: (frame: string, waiting: boolean) => void;
}) {
  const paneRef = useRef<HTMLDivElement>(null);
  const labelRef = useRef<HTMLDivElement>(null);
  const plateRef = useRef<Plate | null>(null);
  // Once a terminal has been asked for here it stays asked for: a slot whose program exited keeps the
  // frame, and a person who pressed the button once has not un-pressed it.
  const [running, setRunning] = useState(autoStart);
  // The fact of the program having exited, which the screen cannot show on its own — what a finished
  // shell leaves behind looks exactly like one waiting to be typed at.
  const [ended, setEnded] = useState(false);
  // The session running here, while one is. It is what the way out names, and it is null at exactly
  // the two moments there is nothing to end: before a terminal has opened, and after one has closed.
  const [live, setLive] = useState<string | null>(null);
  // The reservations the way out is asking about, while it is asking. Null is nobody being asked
  // anything — what is held is read at the press and not before, so there is nothing here to keep
  // true between one and the next.
  const [asking, setAsking] = useState<readonly number[] | null>(null);
  // The folder the terminal here actually runs in, which is what a file handed to this pane is taken
  // into. It comes off the session rather than off `start` for the reason the row above does: a pane
  // that took up a running terminal is drawing one that was opened somewhere else.
  const [folder, setFolder] = useState<string | null>(null);
  // Where the row's menu was opened, while it is open. It is placed at the press rather than under
  // the button for the reason every other menu in the app is (`../components/Menu`).
  const [menuAt, setMenuAt] = useState<{ x: number; y: number } | null>(null);
  // Whether a drag from outside is over this pane. It is the whole of the receiving surface: nothing
  // is drawn until something is being carried, and what is carried is only known while it hangs there.
  const [handing, setHanding] = useState(false);

  // What the face wants done with what happens here, read at the moment it happens. The pane is put up
  // once and lives longer than any one render, so the effect below must not be re-run to see a newer
  // callback — that would take the terminal down to learn something it could have been told.
  const on = useRef({ onOpened, onSaid, onPath, onClosed, onName, onWaiting });
  on.current = { onOpened, onSaid, onPath, onClosed, onName, onWaiting };

  /** Take the place away, once the person has said so. The terminal in it is ended first: a session
   *  whose pane has gone is one nobody can get back to. */
  const goDrop = async () => {
    if (live !== null) await endTerminal(live).catch(() => {});
    onDrop(frame);
  };

  /** The way out was pressed. What is asked depends on what stands to be lost: a pane holding
   *  nothing is the plain confirmation it has always been, and one holding reservations is asked
   *  about them by name (`./PaneDropAsk`). **Nothing is named where nothing is held** — a second
   *  box over the same press, saying a session had no work on it, is a box about nothing. */
  const drop = async () => {
    const holding = await heldHere(live);
    if (holding.length > 0) {
      setAsking(holding);
      return;
    }
    if (!await confirmDialog(t("face.dropConfirm"))) return;
    await goDrop();
  };

  useEffect(() => {
    if (!running) return;
    const host = paneRef.current;
    const label = labelRef.current;
    if (!host || !label) return;
    let taken = false;
    let detach: (() => void) | null = null;
    setEnded(false);
    // The line above the pane. It holds what is known about the session running there for as long as
    // it runs (`../talk/plate.ts`).
    const plate = mountPlate(
      label,
      currentLang,
      (waiting) => on.current.onWaiting(frame, waiting),
      frame,
    );
    plateRef.current = plate;
    void mountAgentFrame(host, currentLang(), {
      opened: (session, startedAt, where) => {
        // The folder is what the row above the pane calls it until something names the frame
        // (`../talk/frames`), and it is the one the terminal actually runs in — which is not always
        // the one this slot was handed.
        plate.opened(session, startedAt, where ?? start.cwd ?? null);
        setLive(session);
        setFolder(where ?? start.cwd ?? null);
        // Where the terminal actually runs, which is not always the folder this slot was handed: a
        // pane that took one up learns it from the session (`../talk/layout`).
        on.current.onOpened(frame, session, where ?? start.cwd ?? null);
      },
      // A path drawn in this pane was clicked. Where it leads is the face's to work out — it knows
      // the folder this frame is in and the one the file face is rooted at (`AMB-T-3630`).
      path: (target) => on.current.onPath(frame, target),
      // Straight through and nowhere else: the row above the pane is the only thing that reads it, and
      // what it reads is the time (`../talk/moving`).
      output: () => plate.output(),
      // Nothing on this face is opened without a folder — the question is answered before the pane
      // is made (`./FolderChoice`) — so there is no choice for the frame to report.
      chose: () => {},
      said: (statement) => {
        plate.said(statement);
        on.current.onSaid(statement);
      },
      closed: (session) => {
        plate.closed(session);
        setEnded(true);
        setLive(null);
        on.current.onClosed(session);
      },
      // The window's own title is not the pane's to say — a face holds several panes, in either of
      // the windows it is drawn in. The name goes to the store, and what draws it is the line above
      // the pane.
      name: (text, by) => on.current.onName(frame, text, by),
    }, start, project)
      .then((take) => {
        // Taken away while the host was still answering. Detaching leaves the terminal running for
        // whatever draws it next, which is exactly what a pane that never got shown should do.
        if (taken) take();
        else detach = take;
      })
      .catch(() => {});
    return () => {
      taken = true;
      detach?.();
      plate.stop();
      plateRef.current = null;
      // **The turn is not taken down with the pane.** A pane goes away when the person turns to
      // another page, which is exactly when they are not looking at it — saying the turn was over
      // because the page turned would erase the one fact the dot on that page exists to carry
      // (`AMB-T-3610`). What ends a turn is the pane saying so, or the session ending.
    };
    // Only `running` is a reason to do any of this again. `start` and `frame` are what this pane *is*
    // — a change of either would be a different pane, and the face gives that one a different key.
  }, [running]);

  // Which pane is being worked in decides one thing on the row: whether a long silence says how long
  // (`../talk/moving`). It is told rather than read, because the arrangement is the face's.
  useEffect(() => {
    plateRef.current?.focused(focused);
  }, [focused, running]);

  // A naming reaches every row, not only the one it happened in: the rail renames a pane that is not
  // the one being worked in, and the row above that pane is where the answer shows.
  useEffect(() => {
    plateRef.current?.named(names);
  }, [names]);

  // Files dragged in from the desktop, which the host hands over as paths (`../core/hostDrop`).
  //
  // **The watch stands only while there is a terminal here to hand them to.** A slot with nothing
  // running in it has nowhere to put a path, and a surface that lit up over one would be a promise
  // the pane cannot keep. It is taken up per pane and matched on this pane's own frame, so a drop
  // that landed on the pane beside it is one this watch is never told about.
  useEffect(() => {
    if (live === null || folder === null) return;
    let alive = true;
    let stop: (() => void) | null = null;
    void watchHostDrop({
      select: `[data-hand="${frame}"]`,
      over: ({ el }) => { if (alive) setHanding(el !== null); },
      leave: () => { if (alive) setHanding(false); },
      drop: (_at, paths) => {
        if (!alive) return;
        setHanding(false);
        void handOver(project, folder, live, paths);
      },
    }).then((off) => { if (alive) stop = off; else off(); });
    return () => {
      alive = false;
      stop?.();
    };
  }, [frame, project, live, folder]);

  return (
    <div
      className={`slot${focused ? " slot--focused" : ""}`}
      data-hand={frame}
      onMouseDown={() => onFocus(frame)}
    >
      {/* What is said about this terminal, and the one control the place has. They share the row
          because the row is what is said about this pane, and removing it is the last thing there is
          to say. The control is drawn whether or not anything is running: a frame kept from the last
          run has no session and is still a place somebody has to be able to get rid of. */}
      <div className="slot__bar">
        {/* The line above the pane, which is empty until there is a session to say something about
            — and holds the row's width open either way, so the control does not walk across it. */}
        <div className="slot__plate" ref={labelRef} />
        {/* What the row can do besides end the place. It is drawn only while a terminal is running,
            because everything in it is a way of handing that terminal something — and it is a menu
            rather than a row of buttons so that a face split four ways does not draw the same button
            four times over. */}
        {live !== null && folder !== null && (
          <button
            className="slot__more"
            title={t("face.more")}
            aria-label={t("face.more")}
            aria-haspopup="menu"
            onClick={(e) => setMenuAt({ x: e.clientX, y: e.clientY })}
          >
            <Icon name="more" />
          </button>
        )}
        <button
          className="slot__end"
          title={t("face.drop")}
          aria-label={t("face.drop")}
          onClick={() => { void drop(); }}
        >
          <Icon name="close" />
        </button>
      </div>
      {menuAt !== null && live !== null && folder !== null && (
        <Menu at={menuAt} onClose={() => setMenuAt(null)}>
          {/* The other way in, for a reader whose file is not somewhere they can drag it from. It
              ends where the drop ends: the file is carried into the project's inbox and the path it
              is at now is put in front of the agent. */}
          <MenuItem
            onClick={() => {
              setMenuAt(null);
              void pickFiles().then((paths) => {
                if (paths.length > 0) return handOver(project, folder, live, paths);
              });
            }}
          >
            <Icon name="inbox" />
            {t("face.upload")}
          </MenuItem>
        </Menu>
      )}
      {/* The receiving surface, drawn over the pane while a drag hangs on it and never otherwise. It
          takes no pointer events: what is under the drag has to stay the pane, or the point the host
          resolves would land on the surface itself and the highlight would flicker itself away. */}
      {handing && <div className="slot__handing">{t("face.handHere")}</div>}
      {asking !== null && (
        <PaneDropAsk
          holding={asking}
          onHandBack={async () => {
            // One at a time, so a refusal stops at the one it refused: the tasks after it are still
            // held, and saying otherwise is the mistake this whole box exists to prevent.
            for (const id of asking) await setStatus(id, "todo");
            await goDrop();
          }}
          onLeave={goDrop}
          onCancel={() => setAsking(null)}
        />
      )}
      {running
        ? (
          <>
            {ended && <span className="termface__note">{t("face.ended")}</span>}
            <div className="termface__face" ref={paneRef} />
          </>
        )
        : (
          <button className="slot__open" onClick={() => setRunning(true)}>
            {t("face.open")}
          </button>
        )}
    </div>
  );
}
