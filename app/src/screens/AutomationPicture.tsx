// The picture of one definition, on a build screen's middle place (`AMB-T-5255`).
//
// **It draws either picture** (`AMB-D-949`): the actions placed on an automation, or the steps inside
// one action. What it is handed is the laid-out shape both are read as (`./automationLayout`), so
// nothing here knows which of the two it is drawing — the screen above it does.
//
// **A box is one placement, and the name on it is the action standing there** (`AMB-D-949`). Nothing
// on the picture is a step: a step is inside the action, and which of them a run opens is read on the
// action's own screen.
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
// action comes from. An empty picture draws nothing: the screen puts its "+ first …" in the middle.
//
// **It scrolls, and it does nothing else.** No zoom, no folding a stretch away: an automation is
// tens of steps, and a picture with a state of its own is one more thing to put back where it was
// every time the definition is read again.
import { useId } from "react";
import { layOut, lineWord, ERROR_EXIT, type PicGraph, type PicLine, type PicMark } from "./automationLayout";
import { listLabel, t, tf } from "../core/i18n";
import { kindLabel } from "./automationPortKinds";
import { Icon } from "../components/Icon";

/** The way out a line hangs on, in a word. Empty for the unnamed one, which has no name to write. */
function exitWord(line: PicLine): string {
  if (line.exitName === ERROR_EXIT) return t("auto.pic.errorExit");
  return lineWord(line) ?? "";
}

/**
 * The name over a wire's trunk: what it hands on, and — where the box hands it on by a way out with
 * a name — that way out first, so two outputs of one name leaving one box by two ways out read apart.
 */
function wireWord(line: PicLine): string {
  const exit = exitWord(line);
  return exit === "" ? (line.hands?.from ?? "") : `${exit} · ${line.hands?.from ?? ""}`;
}

/** One way out of the action, in words: the unnamed one and the error one have names of their own. */
function markWord(mark: PicMark): string {
  if (mark.exitName === undefined) return t("auto.step.exitUnnamed");
  if (mark.exitName === ERROR_EXIT) return t("auto.pic.errorExit");
  return mark.exitName;
}

/** What the action takes in, on the input frame's second line: each name with its kind. */
function inputsLine(mark: PicMark): string {
  if (mark.ports.length === 0) return t("auto.act.none");
  return mark.ports
    .map((port) => `${port.name} ${kindLabel(port.kind)}${port.required ? `・${t("auto.step.required")}` : ""}`)
    .join("　");
}

/** Where the run goes where a line names no step, in a word. */
function endWord(line: PicLine): string {
  if (line.ends === "done") return t("auto.pic.endsDone");
  if (line.ends === "halt") return t("auto.pic.endsHalt");
  return "";
}

/**
 * What a line is, in a sentence: the words written beside an edge, and the one thing a reader who
 * cannot see the drawing is left with.
 */
function lineTitle(line: PicLine): string {
  if (line.kind === "wire" && line.hands !== undefined) {
    return tf("auto.pic.hands", { from: line.hands.from, to: listLabel([...line.hands.to]) });
  }
  return [exitWord(line), endWord(line)].filter((one) => one !== "").join(" — ");
}

/** Which arrowhead a line ends in — its colour, since a marker cannot take the line's own. */
type Head = "next" | "branch" | "error" | "back" | "leaves" | "wire";
const HEADS: readonly Head[] = ["next", "branch", "error", "back", "leaves", "wire"];

function headOf(line: PicLine): Head {
  if (line.back) return "back";
  if (line.leaves) return "leaves";
  if (line.kind === "wire") return "wire";
  return line.tone ?? "next";
}

/**
 * What the lines and marks on the picture mean, under it (`AMB-T-5424`, `AMB-T-5527`). One swatch per
 * kind of line, in the colour the line itself is — so the legend is the stylesheet read aloud, and
 * never a second list of colours to keep in step with it — then the marks a box or a stretch wears,
 * drawn with the same classes as on the picture. A line leaving by one of the action's ways out is
 * only ever drawn on an action's picture, so it is listed there alone.
 */
function Legend({ inAction }: { inAction: boolean }) {
  const one = (kind: string, word: string) => (
    <span className="autopic__legenditem">
      <i className={`autopic__swatch autopic__swatch--${kind}`} aria-hidden="true" />
      {word}
    </span>
  );
  return (
    <div className="autopic__legend">
      {one("next", t("auto.pic.legendNext"))}
      {one("back", t("auto.pic.legendBack"))}
      {one("branch", t("auto.pic.legendBranch"))}
      {one("error", t("auto.pic.errorExit"))}
      {one("wire", t("auto.pic.legendWire"))}
      {inAction && one("leaves", t("auto.pic.legendLeaves"))}
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

export function AutomationPicture({
  graph,
  insertLabel,
  selectedBoxId,
  onPickBox,
  onInsert,
  onPickPart,
  selectedPart,
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
  onPickBox?: (boxId: number) => void;
  /**
   * Put a box in on this edge. Absent while the dialog that asks what goes there is still being
   * built, and every `+` is held shut until it is there.
   */
  onInsert?: (edgeId: number) => void;
  /**
   * The action's own input or output, pressed — its frame over or under the picture. Only an
   * action's picture has them; the panel beside it opens on the one pressed, as it does on a step.
   */
  onPickPart?: (part: "in" | "out") => void;
  /** Which of the action's two frames the panel is showing, if either. */
  selectedPart?: "in" | "out";
}) {
  // Arrowheads are looked up by id, and two pictures on one page must not answer for each other's.
  const ids = useId();
  const head = (kind: Head) => `url(#${ids}-${kind})`;
  const picture = layOut(graph);
  if (picture.nodes.length === 0) return null;

  return (
    <>
      <div className="autopic">
        <div
          className="autopic__sheet"
          style={{ width: `${picture.width}px`, height: `${picture.height}px` }}
        >
          <svg
            className="autopic__lines"
            width={picture.width}
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
                  markerWidth={kind === "wire" ? 5 : 6}
                  markerHeight={kind === "wire" ? 5 : 6}
                  orient="auto"
                >
                  <path className={`autopic__arrow autopic__arrow--${kind}`} d="M0 0 L10 5 L0 10 z" />
                </marker>
              ))}
              {/* Where a wire leaves its box: a dot, as its arrowhead is where it lands. */}
              <marker id={`${ids}-wire-out`} viewBox="0 0 10 10" refX={5} refY={5} markerWidth={4} markerHeight={4}>
                <circle className="autopic__arrow autopic__arrow--wire" cx={5} cy={5} r={5} />
              </marker>
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
                `autopic__line--${line.kind}`,
                line.back ? "autopic__line--back" : "",
                line.leaves ? "autopic__line--leaves" : "",
                line.tone !== undefined ? `autopic__line--${line.tone}` : "",
              ]
                .filter((one) => one !== "")
                .join(" ");
              return (
                <g key={line.key}>
                  <title>{lineTitle(line)}</title>
                  <polyline
                    className={drawn}
                    points={line.points.map((p) => `${p.x},${p.y}`).join(" ")}
                    // Into the box it goes to. A wire's stem ends on its trunk and takes none — its
                    // branches do — and a line that goes nowhere ends in its words instead.
                    markerEnd={line.kind === "edge" && line.points.length > 2 ? head(headOf(line)) : undefined}
                    markerStart={line.kind === "wire" ? `url(#${ids}-wire-out)` : undefined}
                  />
                  {/* A wire's legs off its trunk, one into each input it lands in. */}
                  {line.branches?.map((branch, nth) => (
                    <polyline
                      key={nth}
                      className={drawn}
                      points={branch.map((p) => `${p.x},${p.y}`).join(" ")}
                      markerEnd={head("wire")}
                    />
                  ))}
                  {line.kind === "edge" && lineTitle(line) !== "" && (
                    <text className="autopic__word" x={line.at.x} y={line.at.y} textAnchor={line.align}>
                      {lineTitle(line)}
                    </text>
                  )}
                  {/* A wire is named by what it hands on, past the trunks: the sentence saying where it
                      goes is the title, since the ends it lands in are drawn. */}
                  {line.kind === "wire" && line.hands !== undefined && (
                    <text className="autopic__word" x={line.at.x} y={line.at.y} textAnchor={line.align}>
                      {wireWord(line)}
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

          {/* Over the top-right corner of a box that takes the next task, outside it: inside, the
              name and its second line have the width, and the top-left is where lines come in. */}
          {picture.nodes
            .filter((node) => node.takes === true)
            .map((node) => (
              <span
                key={`takes-${node.boxId}`}
                className="autopic__takes"
                style={{ left: `${node.x + node.w}px`, top: `${node.y}px` }}
              >
                {t("auto.step.takesTask")}
              </span>
            ))}
          {picture.nodes.map((node) => (
            <button
              key={node.boxId}
              type="button"
              className={[
                "autopic__node",
                node.unfed.length > 0 ? "autopic__node--unfed" : "",
                node.empty === true ? "autopic__node--empty" : "",
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
                {node.boxId === graph?.entryId && (
                  <span className="autopic__entry">{t("auto.pic.entry")}</span>
                )}
                <span className="autopic__nodename">{node.name}</span>
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
              </span>
              {/* Which library the action standing here comes from, on an automation's picture — a
                  built-in's is Amenbo's own, though it is kept on the device's shelf. */}
              {node.global !== undefined && (
                <span className="autopic__nodesub">
                  <span className="autopic__lib">
                    {node.builtin !== undefined
                      ? t("auto.actions.reachBuiltin")
                      : t(node.global ? "auto.actions.reachGlobal" : "auto.actions.reachProject")}
                  </span>
                </span>
              )}
            </button>
          ))}

          {picture.inserts.map((insert) => (
            <button
              key={insert.edgeId}
              type="button"
              className="autopic__plus"
              style={{ left: `${insert.x}px`, top: `${insert.y}px` }}
              aria-label={insertLabel ?? t("auto.pic.insert")}
              disabled={onInsert === undefined}
              onClick={() => onInsert?.(insert.edgeId)}
            >
              <Icon name="plus" />
            </button>
          ))}
        </div>
      </div>
      {/* Under the picture rather than on its sheet, so it stays in view however far it is scrolled. */}
      <Legend inAction={graph?.boundary !== undefined} />
    </>
  );
}
