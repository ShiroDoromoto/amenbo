// The picture of one definition, on a build screen's middle place (`AMB-T-5255`).
//
// **It draws either picture** (`AMB-D-949`): the actions placed on an automation, or the steps inside
// one action. What it is handed is the laid-out shape both are read as (`./automationLayout`), so
// nothing here knows which of the two it is drawing — the screen above it does.
//
// **A box is one placement, and the name on it is the action standing there** (`AMB-D-949`). On the
// build screen nothing on the automation's picture is a box of its own for a step: a step is inside the
// action, and its name is written inside the action's box, in order, on a role's picture (below).
//
// **Boxes are elements and lines are one drawing under them.** The boxes carry a name that has to
// wrap and shorten, a press, and a focus ring, which is all a button already does; the lines are
// geometry and belong in one `svg` behind the lot. Boxes drawn inside the `svg` would mean laying
// out text by hand, and lines drawn as elements would mean a box per corner.
//
// **Where each of them goes is `./automationLayout`'s answer**, which holds no element and no
// stylesheet — so what the reader sees can be checked without a screen.
//
// **What a box is missing is a mark, not a sentence** (`AMB-T-5527`). A required input nothing reaches
// is a "⚠ n" at the box's top right, the names it is missing said on hover; an action with nothing in
// it is a dashed box marked "empty". Neither takes the second line, which keeps saying where the
// action comes from — or, on an action's picture, what the step takes in and hands on, a line each
// (below). An empty picture draws nothing: the screen puts its "+ first …" in the middle.
//
// **A line's `+` shows when the line is pointed at, and not before** (`AMB-T-5697`). One on every line
// all the time put a ring beside every name on the picture, and what a reader opens it for first is
// the order, not where a box could go in. It is still a button the whole time: Tab reaches it, and it
// shows while it holds the focus. Each edge is traced by a wider stroke nobody sees, so the pointer
// does not have to land on a line one and a half pixels wide.
//
// **A way out nothing has been decided for ends in a press, shown all the time** (`AMB-D-1003`). It is
// drawn dashed, apart from every line that goes somewhere, and the press at its end — "+ place the next
// action", or the next step on an action's picture — is how a reader goes on from a box whose ways out
// say nothing yet. Held shut, like every `+`, where nothing can be put in.
//
// **What a box hands on is not drawn** (`AMB-D-1001`). The dotted wires took the right of the
// picture and crossed the tops of the boxes, and nobody makes or drops one on it: the panel beside it
// does both, and says where each goes. What stays on a box is its "⚠" for an input nothing reaches.
// A step's card on an action's picture says it in words instead (`AMB-T-5798`): its inputs, each with
// the number of the step it comes from, and its outputs — so what flows through a step is read without
// opening its panel, and still no line crosses the picture. An action's box on a role's picture says
// the same of the action, each input with the number of the box it comes from.
//
// **On the build screen it draws one role's round** (`automationGraph` with a role), in one row from
// left to right inside one frame: "receives" — a failed round's incident, the triggers and the roles
// that hand this one its task, or people where nothing else does — then the actions the round passes
// through, each with its steps inside it, laid out across (`layOutAcross`), and "hands on" — a frame
// for each role it hands on to, with that role's icon and name and the task it hands on by its title
// (`PicRound.passes`), or one saying none where it ends at done — with an arrow between each of them,
// and none between the frames it hands on to. The row never wraps: in a frame too narrow the words of
// the ends wrap first, down to their heading on one line, and only then does the picture scroll
// across inside it. A frame handed on to keeps its width, so the role's name and the task's title
// read whole. Each box is as big as what it says (`roundSize`), so the actions of a round are
// seen side by side. How wide the row has to be for the whole picture to be seen is told to the
// screen (`onRowNeeds`, `rowNeeds`), which gives the row the screen's whole width where the middle of
// its three places is narrower. A line going back is no line there but a loop under the box it goes
// back to, saying which box and which way out it comes back from.
//
// **It scrolls, and it does nothing else.** No zoom, and no folding a stretch away by hand: an
// automation is tens of steps, and a picture with a state of its own is one more thing to put back
// where it was every time the definition is read again. What a box holds is opened by the run alone,
// never by the reader (below). The one move it makes is to bring a box newly picked into
// sight — the box a run stopped at arrives picked from its pane (`AMB-T-5594`), and a band over the
// screen can push it out of view. It stops moving once the reader's own hand has moved the screen,
// so a reader scrolling away from the box that stays picked is not pulled back.
import { Fragment, useEffect, useId, useLayoutEffect, useMemo, useRef, useState } from "react";
import {
  edgeWord,
  exitWord,
  layOut,
  openWord,
  pictureOrder,
  stepKindWord,
  ACTION_BOUNDARY,
  ERROR_EXIT,
  TAKES_INSET,
  type PicGraph,
  type PicLine,
  type PicMark,
  type PicNode,
  type PicPass,
  type PicRound,
} from "./automationLayout";
import { listLabel, t, tf } from "../core/i18n";
import { kindLabel } from "./automationPortKinds";
import { cardIo } from "./automationWires";
import { RoleIcon } from "./AutomationRoles";
import { Icon } from "../components/Icon";

/** Under a step's name on an action's picture: what it takes in, then what it hands on — a line each,
 *  cut short where it runs out, and read in full on hover. Nothing at all where it has neither. */
function CardIo({ words }: { words: { takes?: string; hands?: string } }) {
  const all = [words.takes, words.hands].filter((one) => one !== undefined);
  if (all.length === 0) return null;
  return (
    <span className="autopic__nodesub autopic__io" title={all.join("\n")}>
      {all.map((one) => (
        <span key={one} className="autopic__iopart">
          {one}
        </span>
      ))}
    </span>
  );
}

/** Either end of a role's round: what it receives, or what it hands on to — the heading, and the words. */
function RoundEnd({ head, words }: { head: string; words: string }) {
  return (
    <div className="autopic__end">
      <span className="autopic__endhead">{head}</span>
      <span className="autopic__endwords">{words}</span>
    </div>
  );
}

/** One role the round hands on to: the heading, the role's icon and name, and the task by its title —
 *  marked where it depends on another. A line out to another role's box hands on no task, and has no
 *  third line. */
function PassEnd({ pass }: { pass: PicPass }) {
  return (
    <div className="autopic__end autopic__end--pass">
      <span className="autopic__endhead">{t("auto.pic.handsOn")}</span>
      <span className="autopic__endwords autopic__endrole">
        <RoleIcon role={pass.role} size={16} />
        {pass.role.name}
      </span>
      {pass.task !== undefined && (
        <span className="autopic__endtask">
          {tf(pass.depends ? "auto.pic.passTaskDepends" : "auto.pic.passTask", { title: pass.task })}
        </span>
      )}
    </div>
  );
}

/** Where a role's round receives its task from, in words: a failed round's incident, then the triggers
 *  by name, then the roles — or people, where none of them hands it one. */
function receivesWords(round: PicRound): string {
  const all = [
    ...(round.receives.failures ? [t("auto.pic.fromFailures")] : []),
    ...round.receives.triggers.map((name) => tf("auto.pic.fromTrigger", { name })),
    ...round.receives.roles.map((role) => tf("auto.pic.fromRole", { role })),
  ];
  return all.length === 0 ? t("auto.pic.fromPeople") : listLabel(all);
}

/** Under the box on a role's picture: the lines that come back into it, each from the box it leaves. */
function returnsWords(node: PicNode): string {
  return (node.returns ?? [])
    .map((one) => tf("auto.pic.loopBack", { no: one.no, exit: exitWord(one) }))
    .join("　");
}

/** One way out of the action, in words: by its name, the error one in the screen's words. Only an
 *  `out` mark is written this way, and it always has one. */
function markWord(mark: PicMark): string {
  const name = mark.exitName ?? "";
  return name === ERROR_EXIT ? t("auto.pic.errorExit") : name;
}

/** What the action takes in, on the input frame's second line: each name with its kind. */
function inputsLine(mark: PicMark): string {
  if (mark.ports.length === 0) return t("auto.act.none");
  return mark.ports
    .map((port) => `${port.name} ${kindLabel(port.kind)}${port.required ? `・${t("auto.step.required")}` : ""}`)
    .join("　");
}

/** Which arrowhead a line ends in — its colour, since a marker cannot take the line's own. */
type Head = "next" | "branch" | "error" | "back" | "leaves";
const HEADS: readonly Head[] = ["next", "branch", "error", "back", "leaves"];

/** The arrowhead a line ends in, by the kind of line it is. */
function headOf(line: PicLine): Head {
  if (line.back) return "back";
  if (line.leaves) return "leaves";
  return line.tone ?? "next";
}

/**
 * What the lines and marks on the picture mean, under it (`AMB-T-5424`, `AMB-T-5527`). One swatch per
 * kind of line, in the colour the line itself is — so the legend is the stylesheet read aloud, and
 * never a second list of colours to keep in step with it — then the marks a box or a stretch wears,
 * drawn with the same classes as on the picture. A line leaving by one of the action's ways out is
 * only ever drawn on an action's picture, so it is listed there alone; a role's picture draws a line
 * going back as a loop under a box, so it lists the loop in its place. The error line's word also says
 * where a box with no such line goes, since the picture does not draw that on every box.
 */
function Legend({ inAction, inRound }: { inAction: boolean; inRound: boolean }) {
  const one = (kind: string, word: string) => (
    <span className="autopic__legenditem">
      <i className={`autopic__swatch autopic__swatch--${kind}`} aria-hidden="true" />
      {word}
    </span>
  );
  return (
    <div className="autopic__legend">
      {one("next", t("auto.pic.legendNext"))}
      {inRound ? (
        <span className="autopic__legenditem">
          <span className="autopic__loopmark">↺</span>
          {t("auto.pic.legendLoop")}
        </span>
      ) : (
        one("back", t("auto.pic.legendBack"))
      )}
      {one("branch", t("auto.pic.legendBranch"))}
      {one("error", t("auto.pic.legendError"))}
      {inAction && one("leaves", t("auto.pic.legendLeaves"))}
      {one("open", t("auto.pic.legendOpen"))}
      {one("lap", t("auto.pic.lap"))}
      <span className="autopic__legenditem">
        <span className="autopic__entry">{t("auto.pic.entry")}</span>
      </span>
      <span className="autopic__legenditem autopic__unfed">
        <Icon name="warning" />
        {t("auto.pic.legendUnfed")}
      </span>
    </div>
  );
}

/**
 * How wide a role's row (`.autopic__flow`) has to be for its picture to be seen whole, without
 * scrolling across: its frame, the gaps and arrows, the picture at its own width, each frame it hands
 * on to at its own width — those do not give way — and the other ends at their narrowest: they give
 * way before the picture does, down to their heading on one line.
 */
function rowNeeds(flow: HTMLElement): number {
  const px = (value: string) => Number.parseFloat(value) || 0;
  const style = getComputedStyle(flow);
  let needs =
    px(style.paddingLeft) + px(style.paddingRight) + px(style.borderLeftWidth) + px(style.borderRightWidth) +
    px(style.columnGap) * Math.max(0, flow.children.length - 1);
  for (const child of Array.from(flow.children)) {
    if (!(child instanceof HTMLElement)) continue;
    const own = getComputedStyle(child);
    if (child.classList.contains("autopic")) needs += child.scrollWidth + px(own.borderLeftWidth) + px(own.borderRightWidth);
    else if (child.classList.contains("autopic__end") && !child.classList.contains("autopic__end--pass")) {
      const head = child.querySelector<HTMLElement>(".autopic__endhead");
      needs +=
        (head?.offsetWidth ?? 0) +
        px(own.paddingLeft) + px(own.paddingRight) + px(own.borderLeftWidth) + px(own.borderRightWidth);
    }
    else needs += child.offsetWidth;
  }
  return Math.ceil(needs);
}

export function AutomationPicture({
  graph,
  insertLabel,
  selectedBoxId,
  placedForYou,
  onPickBox,
  onInsert,
  onOpenExit,
  onPickPart,
  selectedPart,
  onRowNeeds,
}: {
  graph: PicGraph | null;
  /**
   * What the `+` on a line is called. It names what goes in a box, which is the one thing the two
   * pictures do not share — an automation takes actions, an action takes steps — so the screen says
   * it and the drawing stays the same.
   */
  insertLabel?: string;
  /** The box whose contents the panel beside this is showing (`AMB-T-5256`). */
  selectedBoxId?: number;
  /** Boxes Amenbo put on for the reader rather than the reader themselves (`AMB-T-5797`) — marked,
   *  with what they are there for on their second line. */
  placedForYou?: ReadonlySet<number>;
  onPickBox?: (boxId: number) => void;
  /**
   * Put a box in on this edge. Absent while the dialog that asks what goes there is still being
   * built, and every `+` is held shut until it is there.
   */
  onInsert?: (edgeId: number) => void;
  /** Put the next box on after a way out nothing has been decided for (`AMB-D-1003`). Absent, the
   *  press at its end is held shut. */
  onOpenExit?: (from: { boxId: number; exitName: string }) => void;
  /**
   * The action's own input or output, pressed — its frame over or under the picture. Only an
   * action's picture has them; the panel beside it opens on the one pressed, as it does on a step.
   */
  onPickPart?: (part: "in" | "out") => void;
  /** Which of the action's two frames the panel is showing, if either. */
  selectedPart?: "in" | "out";
  /** How wide a role's row has to be for its picture to be seen whole (`rowNeeds`), told each time it
   *  is laid out again. Only a role's round has the row. */
  onRowNeeds?: (width: number) => void;
}) {
  // Arrowheads are looked up by id, and two pictures on one page must not answer for each other's.
  const ids = useId();
  const head = (kind: Head) => `url(#${ids}-${kind})`;
  // Laid out again only when the graph changes: a draw that leaves it as it was keeps the layout it had.
  const picture = useMemo(() => layOut(graph), [graph]);
  const round = graph?.round;
  const flowRef = useRef<HTMLDivElement | null>(null);
  // **A role's sheet is as wide as what is drawn on it.** The layout sizes the sheet from estimates of
  // how wide its words are, and a press or name drawn a few pixels wider than estimated stood past the
  // sheet's right edge: the picture then scrolled across by those few pixels with nothing hidden. The
  // reach of what is drawn is measured once laid out, and the sheet widened to it.
  const sheetRef = useRef<HTMLDivElement | null>(null);
  const [reach, setReach] = useState<{ of: typeof picture; width: number } | null>(null);
  const sheetW = reach !== null && reach.of === picture ? reach.width : picture.width;
  useLayoutEffect(() => {
    const sheet = sheetRef.current;
    if (round === undefined || sheet === null) return;
    const width = Math.ceil(sheet.scrollWidth);
    setReach(width > picture.width ? { of: picture, width } : null);
  }, [picture, round]);
  // Told again once the sheet is widened to what is drawn on it, so the screen makes room for that too.
  useEffect(() => {
    const flow = flowRef.current;
    // A picture with no row — an empty one, or not a role's — needs nothing of the screen.
    onRowNeeds?.(flow === null ? 0 : rowNeeds(flow));
  }, [picture, sheetW, onRowNeeds]);
  // What a card says it takes in and hands on — a step's on an action's picture, an action's on a
  // role's — each input with the number of the box that hands it its value, or the action's own input.
  // On a role's picture that box may be on another role's round, and is numbered as the whole
  // automation numbers it.
  const numberOf = useMemo(() => (round === undefined ? undefined : pictureOrder(round.whole).numberOf), [round]);
  const ioWords = (boxId: number): { takes?: string; hands?: string } => {
    if (graph === null) return {};
    const io = cardIo(graph, boxId);
    const from = (one: { name: string; from?: number }) => {
      if (one.from === undefined) return one.name;
      if (one.from === ACTION_BOUNDARY) return tf("auto.pic.ioFromAction", { name: one.name });
      const no = numberOf?.get(one.from) ?? picture.nodes.find((node) => node.boxId === one.from)?.no;
      return no === undefined ? one.name : tf("auto.pic.ioFrom", { name: one.name, no });
    };
    return {
      takes: io.inputs.length > 0 ? tf("auto.pic.ioIn", { names: listLabel(io.inputs.map(from)) }) : undefined,
      hands: io.outputs.length > 0 ? tf("auto.pic.ioOut", { names: listLabel(io.outputs) }) : undefined,
    };
  };
  const pickedRef = useRef<HTMLButtonElement | null>(null);
  // The edge the pointer is on, whose `+` is shown.
  const [near, setNear] = useState<number | null>(null);
  // The edges into and out of the picked box, whose `+` is shown without being pointed at: a `+` that
  // only a pointer brings up is one nobody finds, and showing every one of them brings back the
  // crowding `AMB-T-5697` took away. Around the box being looked at, they are where the eye is
  // (`AMB-D-1003`).
  const around = new Set(
    (graph?.edges ?? [])
      .filter((edge) => selectedBoxId !== undefined && (edge.fromId === selectedBoxId || (edge.ends === "go" && edge.toId === selectedBoxId)))
      .map((edge) => edge.id),
  );
  // A box picked before the definition has loaded has no element yet, so the move waits for the
  // render that draws it. Whether the box is in sight is asked of an observer rather than read at
  // once, and asked for as long as the screen is still settling: the panel the pick opens and the
  // bands over the picture land on renders after the box is drawn, and can push it out of view after
  // it was seen whole or brought into sight. A box in sight stays where it is; one that is not is
  // brought to the middle, with the lines around it, as often as the screen pushes it out. The watch
  // ends with the reader's own wheel, touch, key or press — from then on where the screen stands is
  // theirs.
  //
  // The observer speaks only when the box crosses into or out of sight between two of its looks, so
  // it alone cannot be trusted with a screen that is still moving. Sent from a run's pane, the box
  // was brought into sight and the band over the board grew in the same frame, pushing it back
  // below the fold: out of sight at both looks, it crossed nothing, and nothing moved it again
  // (`AMB-T-5595`). The band is not around the box, and the boxes that are keep their size while
  // their content overflows, so no size of theirs says it grew. What does is the page changing around
  // the box, and each such change asks the observer again from the start: a change inside the
  // picture's frame; something coming or going beside the frame or beside any box it stands in, out
  // to the page; one of those changing size, as a band does when it grows; and the box's own size
  // changing (a face coming up). The rest of the document is not watched. The workspace's terminal
  // writes its output into the document as it comes, and asking again at each of those writes made
  // typing in it slow, though none of them moves the box.
  const drawn = picture.nodes.some((node) => node.boxId === selectedBoxId);
  useEffect(() => {
    const box = pickedRef.current;
    if (!drawn || box === null || typeof IntersectionObserver === "undefined") return;
    // The picture grows as tall as it draws, and the screen scrolls it down, so its box is brought
    // into sight on the screen.
    const bring = () => box.scrollIntoView?.({ block: "center", inline: "nearest" });
    const reader = ["wheel", "touchstart", "keydown", "pointerdown"] as const;
    const stop = () => {
      watch.disconnect();
      sized?.disconnect();
      changed.disconnect();
      beside.disconnect();
      for (const kind of reader) window.removeEventListener(kind, stop, true);
    };
    const watch = new IntersectionObserver(([seen]) => {
      // A box with no size is on a face not shown yet and has no place to be brought to.
      const laidOut = seen !== undefined && (seen.boundingClientRect?.height ?? 1) > 0;
      if (laidOut && seen.intersectionRatio < 1) bring();
    }, { threshold: 1 });
    const askAgain = () => {
      watch.unobserve(box);
      watch.observe(box);
    };
    const sized = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(askAgain);
    const changed = new MutationObserver(askAgain);
    // A band that lands beside the frame, or beside any box the frame stands in, is watched for its
    // size from then on, so its growing later asks again too.
    const beside = new MutationObserver((records) => {
      for (const record of records) {
        record.addedNodes.forEach((added) => {
          if (added instanceof Element) sized?.observe(added);
        });
      }
      askAgain();
    });
    const sheet = box.closest<HTMLElement>(".autopic") ?? box;
    watch.observe(box);
    sized?.observe(box);
    changed.observe(sheet, { childList: true, subtree: true, characterData: true });
    for (let at: Element = sheet; at !== document.body && at.parentElement !== null; at = at.parentElement) {
      beside.observe(at.parentElement, { childList: true });
      for (const one of Array.from(at.parentElement.children)) sized?.observe(one);
    }
    for (const kind of reader) window.addEventListener(kind, stop, true);
    return stop;
  }, [selectedBoxId, drawn]);
  if (picture.nodes.length === 0) {
    // A role whose round reaches nothing, on an automation that has something on it: said in words, so
    // the middle is not left blank. With nothing on the automation, the screen's "+ first …" says it.
    return round !== undefined && round.whole.boxes.length > 0 ? (
      <p className="autopic__roundempty">{t("auto.pic.roundEmpty")}</p>
    ) : null;
  }
  // Where a run opens: where the round starts, or on an action's picture the step it opens first.
  const startId = graph?.entryId;

  const drawing = (
    <div className="autopic">
      <div
        className="autopic__sheet"
        ref={sheetRef}
        style={{ width: `${sheetW}px`, height: `${picture.height}px` }}
      >
        <svg
          className="autopic__lines"
          width={sheetW}
          height={picture.height}
          aria-hidden="true"
        >
          <defs>
            {HEADS.map((kind) => (
              <marker
                key={kind}
                id={`${ids}-${kind}`}
                viewBox="0 0 10 10"
                refX={9}
                refY={5}
                markerWidth={6}
                markerHeight={6}
                orient="auto"
              >
                <path className={`autopic__arrow autopic__arrow--${kind}`} d="M0 0 L10 5 L0 10 z" />
              </marker>
            ))}
          </defs>
          {picture.laps.map((lap) => (
            <rect
              key={lap.headBoxId}
              className="autopic__lap"
              x={lap.x}
              y={lap.y}
              width={lap.w}
              height={lap.h}
              rx={8}
            />
          ))}
          {picture.lines.map((line) => {
            const drawn = [
              "autopic__line",
              line.back ? "autopic__line--back" : "",
              line.leaves ? "autopic__line--leaves" : "",
              line.open === true ? "autopic__line--open" : "",
              line.tone !== undefined ? `autopic__line--${line.tone}` : "",
            ]
              .filter((one) => one !== "")
              .join(" ");
            // The line in from the action's top mark is no edge, and puts no box in. A piece of a lane
            // only one edge runs along yet is that edge's, and brings up its `+`.
            const edgeId = line.key.startsWith("edge-")
              ? Number(line.key.slice("edge-".length))
              : line.carries?.length === 1
                ? line.carries[0]
                : undefined;
            return (
              <g key={line.key}>
                <title>{edgeWord(line)}</title>
                <polyline
                  className={drawn}
                  points={line.points.map((p) => `${p.x},${p.y}`).join(" ")}
                  // Into the box it goes to. A line that goes nowhere ends in its words instead, and one
                  // that joins others on its lane ends there.
                  markerEnd={line.points.length > 2 && line.joins !== true ? head(headOf(line)) : undefined}
                />
                {/* Last in the group, so it lies over the line it traces. */}
                {edgeId !== undefined && (
                  <polyline
                    className="autopic__hit"
                    points={line.points.map((p) => `${p.x},${p.y}`).join(" ")}
                    onMouseEnter={() => setNear(edgeId)}
                    onMouseLeave={() => setNear((was) => (was === edgeId ? null : was))}
                  />
                )}
                {edgeWord(line) !== "" && (
                  <text className="autopic__word" x={line.at.x} y={line.at.y} textAnchor={line.align}>
                    {edgeWord(line)}
                  </text>
                )}
              </g>
            );
          })}
        </svg>

        {picture.laps.map((lap) => (
          <span
            key={lap.headBoxId}
            className="autopic__lapword"
            style={{ left: `${lap.x + 8}px`, top: `${lap.y}px` }}
          >
            {t("auto.pic.lap")}
          </span>
        ))}

        {/* The action itself: its input, a frame over everything, and its output, a frame under
            everything holding each way out it is left by. Pressing either opens it in the panel. */}
        {picture.outFrame !== undefined && (
          <button
            type="button"
            className={selectedPart === "out" ? "autopic__frame autopic__frame--on" : "autopic__frame"}
            style={{
              left: `${picture.outFrame.x}px`,
              top: `${picture.outFrame.y}px`,
              width: `${picture.outFrame.w}px`,
              height: `${picture.outFrame.h}px`,
            }}
            aria-pressed={selectedPart === "out"}
            disabled={onPickPart === undefined}
            onClick={() => onPickPart?.("out")}
          >
            <span className="autopic__frametitle">{t("auto.pic.actionOut")}</span>
          </button>
        )}
        {picture.marks.map((mark) =>
          mark.kind === "in" ? (
            <button
              key={mark.key}
              type="button"
              className={selectedPart === "in" ? "autopic__frame autopic__frame--in autopic__frame--on" : "autopic__frame autopic__frame--in"}
              style={{
                left: `${mark.x}px`,
                top: `${mark.y}px`,
                width: `${mark.w}px`,
                height: `${mark.h}px`,
              }}
              aria-pressed={selectedPart === "in"}
              disabled={onPickPart === undefined}
              onClick={() => onPickPart?.("in")}
            >
              <span className="autopic__frametitle">{t("auto.pic.actionIn")}</span>
              <span className="autopic__frameline">{inputsLine(mark)}</span>
            </button>
          ) : (
            <span
              key={mark.key}
              className={mark.exitName === ERROR_EXIT ? "autopic__mark autopic__mark--error" : "autopic__mark"}
              style={{
                left: `${mark.x}px`,
                top: `${mark.y}px`,
                width: `${mark.w}px`,
                height: `${mark.h}px`,
              }}
            >
              <span className="autopic__markname">{markWord(mark)}</span>
              {mark.ports.length > 0 && (
                <span className="autopic__markports">{mark.ports.map((one) => one.name).join("・")}</span>
              )}
            </span>
          ),
        )}

        {/* Over the top-right of a box that takes the next task, outside it: inside, the name and
            its second line have the width, and the top-left is where lines come in. It ends short of
            the corner, where the line in from the right margin lands. */}
        {picture.nodes
          .filter((node) => node.takes === true)
          .map((node) => (
            <span
              key={`takes-${node.boxId}`}
              className="autopic__takes"
              style={{ left: `${node.x + node.w - TAKES_INSET}px`, top: `${node.y}px` }}
            >
              {t("auto.step.takesTask")}
            </span>
          ))}
        {picture.nodes.map((node) => (
          <button
            key={node.boxId}
            ref={node.boxId === selectedBoxId ? pickedRef : undefined}
            type="button"
            className={[
              "autopic__node",
              node.unfed.length > 0 ? "autopic__node--unfed" : "",
              node.empty === true ? "autopic__node--empty" : "",
              node.draft === true ? "autopic__node--draft" : "",
              node.boxId === selectedBoxId ? "autopic__node--on" : "",
            ]
              .filter((one) => one !== "")
              .join(" ")}
            style={{
              left: `${node.x}px`,
              top: `${node.y}px`,
              width: `${node.w}px`,
              height: `${node.h}px`,
            }}
            aria-pressed={node.boxId === selectedBoxId}
            disabled={onPickBox === undefined}
            onClick={() => onPickBox?.(node.boxId)}
          >
            <span className="autopic__nodehead">
              {/* The number a list naming the boxes calls this one by (`pictureOrder`). */}
              {node.no !== undefined && <span className="autopic__no">{node.no}</span>}
              {/* Where a run opens. It is drawn on the box rather than worked out from the picture:
                  the walk starts here, so a reader looking at two disconnected stretches has no
                  other way to tell which of them a launch enters by. */}
              {node.boxId === startId && (
                <span className="autopic__entry">{t("auto.pic.entry")}</span>
              )}
              {/* On a role's picture every box is an action, and says so before its name, the two
                  small and muted as one label, as the mock heads its box. */}
              {round !== undefined && <span className="autopic__actionword">{t("auto.pic.actionWord")}</span>}
              <span className={round !== undefined ? "autopic__nodename autopic__nodename--action" : "autopic__nodename"}>
                {node.name}
              </span>
              {node.unfed.length > 0 && (
                <span
                  className="autopic__unfed"
                  title={tf("auto.pic.unfed", { names: listLabel([...node.unfed]) })}
                  aria-label={tf("auto.pic.unfed", { names: listLabel([...node.unfed]) })}
                >
                  <Icon name="warning" />
                  {node.unfed.length}
                </span>
              )}
              {node.empty === true && <span className="autopic__emptymark">{t("auto.pic.emptyMark")}</span>}
              {node.draft === true && <span className="autopic__draftmark">{t("chip.draft")}</span>}
              {node.behind !== undefined && (
                <span
                  className="autopic__behindmark"
                  title={tf("auto.pic.behind", { version: node.behind.version, latest: node.behind.latest })}
                >
                  {tf("auto.pic.behindMark", { version: node.behind.version })}
                </span>
              )}
              {placedForYou?.has(node.boxId) === true && (
                <span className="autopic__formark">{t("auto.pic.placedForYou")}</span>
              )}
            </span>
            {graph?.boundary !== undefined && <CardIo words={ioWords(node.boxId)} />}
            {/* The steps inside the action, on a role's picture, in place of its library: one card
                each, its name over its kind, with an arrow between two (`stepKind`). Else which
                library the action standing here comes from, on an automation's picture — a
                built-in's is Amenbo's own, though it is kept on the device's shelf. */}
            {node.steps !== undefined && node.steps.length > 0 ? (
              <span className="autopic__steps">
                {node.steps.map((step, nth) => {
                  const kind = stepKindWord(step);
                  return (
                    <Fragment key={nth}>
                      {nth > 0 && (
                        <span className="autopic__steparrow" aria-hidden="true">
                          →
                        </span>
                      )}
                      <span
                        className={`autopic__step autopic__step--${step.kind}`}
                        style={{ width: `${step.w}px` }}
                        title={`${step.name}\n${kind}`}
                      >
                        <span className="autopic__stepname">{step.name}</span>
                        <span className="autopic__stepkind">{kind}</span>
                      </span>
                    </Fragment>
                  );
                })}
              </span>
            ) : node.global !== undefined && placedForYou?.has(node.boxId) === true ? (
              <span className="autopic__nodesub" title={t("auto.pic.placedForYouWhy")}>
                <span className="autopic__lib">{t("auto.pic.placedForYouWhy")}</span>
              </span>
            ) : node.global !== undefined && (
              <span className="autopic__nodesub">
                <span className="autopic__lib">
                  {node.builtin !== undefined
                    ? t("auto.actions.reachBuiltin")
                    : t(node.global ? "auto.actions.reachGlobal" : "auto.actions.reachProject")}
                </span>
              </span>
            )}
            {round !== undefined && <CardIo words={ioWords(node.boxId)} />}
            {node.returns !== undefined && (
              <span className="autopic__nodesub autopic__loop" title={returnsWords(node)}>
                <span className="autopic__iopart">{returnsWords(node)}</span>
              </span>
            )}
          </button>
        ))}

        {picture.inserts.map((insert) => (
          <button
            key={insert.edgeId}
            type="button"
            className={insert.edgeId === near || around.has(insert.edgeId) ? "autopic__plus autopic__plus--near" : "autopic__plus"}
            style={{ left: `${insert.x}px`, top: `${insert.y}px` }}
            aria-label={insertLabel ?? t("auto.pic.insert")}
            disabled={onInsert === undefined}
            onClick={() => onInsert?.(insert.edgeId)}
          >
            <Icon name="plus" />
          </button>
        ))}
        {picture.opens.map((open) => (
          <button
            key={`open-${open.boxId}-${open.exitName}`}
            type="button"
            className="autopic__open"
            style={{ left: `${open.x}px`, top: `${open.y}px` }}
            disabled={onOpenExit === undefined}
            onClick={() => onOpenExit?.({ boxId: open.boxId, exitName: open.exitName })}
          >
            {openWord(graph?.boundary !== undefined)}
          </button>
        ))}
      </div>
    </div>
  );

  return (
    <>
      {round === undefined ? (
        drawing
      ) : (
        <div className="autopic__flow" ref={flowRef}>
          <RoundEnd head={t("auto.pic.receives")} words={receivesWords(round)} />
          <span className="autopic__flowarrow" aria-hidden="true">
            →
          </span>
          {drawing}
          <span className="autopic__flowarrow" aria-hidden="true">
            →
          </span>
          {round.passes.length === 0 ? (
            <RoundEnd head={t("auto.pic.handsToNone")} words={t("auto.pic.handsNone")} />
          ) : (
            round.passes.map((pass, at) => <PassEnd key={at} pass={pass} />)
          )}
        </div>
      )}
      {/* Under the picture rather than on its sheet, so it stays in view however far it is scrolled. */}
      <Legend inAction={graph?.boundary !== undefined} inRound={round !== undefined} />
    </>
  );
}
