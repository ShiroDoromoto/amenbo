// The picture of one definition, on a build screen's middle place (`AMB-T-5255`).
//
// **It draws either picture** (`AMB-D-949`): the actions placed on an automation, or the steps inside
// one action. What it is handed is the laid-out shape both are read as (`./automationLayout`), so
// nothing here knows which of the two it is drawing — the screen above it does.
//
// **Boxes are elements and lines are one drawing under them.** The boxes carry a name that has to
// wrap and shorten, a press, and a focus ring, which is all a button already does; the lines are
// geometry and belong in one `svg` behind the lot. Boxes drawn inside the `svg` would mean laying
// out text by hand, and lines drawn as elements would mean a box per corner.
//
// **Where each of them goes is `./automationLayout`'s answer**, which holds no element and no
// stylesheet — so what the reader sees can be checked without a screen.
//
// **It scrolls, and it does nothing else.** No zoom, no folding a stretch away: an automation is
// tens of steps, and a picture with a state of its own is one more thing to put back where it was
// every time the definition is read again.
import { layOut, ERROR_EXIT, type PicGraph, type PicLine } from "./automationLayout";
import { listLabel, t, tf } from "../core/i18n";
import { Icon } from "../components/Icon";

/** The way out a line hangs on, in a word. Empty for the unnamed one, which has no name to write. */
function exitWord(line: PicLine): string {
  if (line.exitName === ERROR_EXIT) return t("auto.pic.errorExit");
  return line.exitName ?? "";
}

/** Where the run goes where a line names no step, in a word. */
function endWord(line: PicLine): string {
  if (line.ends === "done") return t("auto.pic.endsDone");
  if (line.ends === "halt") return t("auto.pic.endsHalt");
  return "";
}

/** What a line is, in a sentence — the one thing a reader who cannot see the drawing is left with. */
function lineTitle(line: PicLine): string {
  if (line.kind === "wire" && line.hands !== undefined) {
    return tf("auto.pic.hands", { from: line.hands.from, to: line.hands.to });
  }
  return [exitWord(line), endWord(line)].filter((one) => one !== "").join(" — ");
}

export function AutomationPicture({
  graph,
  empty,
  insertLabel,
  selectedBoxId,
  onPickBox,
  onInsert,
}: {
  graph: PicGraph | null;
  /**
   * What an empty picture says, and what the `+` on a line is called. Both name what goes in a box,
   * which is the one thing the two pictures do not share — an automation takes actions, an action
   * takes steps — so the screen says it and the drawing stays the same.
   */
  empty?: string;
  insertLabel?: string;
  /** The box whose contents the panel beside this is showing (`AMB-T-5256`). */
  selectedBoxId?: number;
  onPickBox?: (boxId: number) => void;
  /**
   * Put a box in on this edge. Absent while the dialog that asks what goes there is still being
   * built, and every `+` is held shut until it is there.
   */
  onInsert?: (edgeId: number) => void;
}) {
  const picture = layOut(graph);
  if (picture.nodes.length === 0) {
    return <div className="auto__empty">{empty ?? t("auto.pic.empty")}</div>;
  }

  return (
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
          {picture.lines.map((line) => (
            <g key={line.key}>
              <title>{lineTitle(line)}</title>
              <polyline
                className={`autopic__line autopic__line--${line.kind}${line.back ? " autopic__line--back" : ""}`}
                points={line.points.map((p) => `${p.x},${p.y}`).join(" ")}
              />
              {line.kind === "edge" && exitWord(line) !== "" && (
                <text className="autopic__word" x={line.at.x} y={line.at.y}>
                  {exitWord(line)}
                </text>
              )}
              {line.endAt !== undefined && (
                <text className="autopic__word" x={line.endAt.x} y={line.endAt.y}>
                  {endWord(line)}
                </text>
              )}
            </g>
          ))}
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

        {picture.nodes.map((node) => (
          <button
            key={node.boxId}
            type="button"
            className={[
              "autopic__node",
              node.unfed.length > 0 ? "autopic__node--unfed" : "",
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
            <span className="autopic__nodename">{node.name}</span>
            {node.unfed.length > 0 && (
              <span className="autopic__unfed">
                <Icon name="warning" />
                <span>{tf("auto.pic.unfed", { names: listLabel([...node.unfed]) })}</span>
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
  );
}
