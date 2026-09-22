// The picture of the steps, on the build screen's middle place (`AMB-T-5255`).
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
import { layOut, ERROR_EXIT, type PicLine } from "./automationLayout";
import { listLabel, t, tf } from "../core/i18n";
import { Icon } from "../components/Icon";
import type { AutomationDetailDto } from "../bindings/bindings";

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
  automation,
  selectedStepId,
  onPickStep,
  onInsertStep,
}: {
  automation: AutomationDetailDto | null;
  /** The step whose contents the panel beside this is showing (`AMB-T-5256`). */
  selectedStepId?: number;
  onPickStep?: (stepId: number) => void;
  /**
   * Put a step in on this edge. Absent while the dialog that asks what step is still being built
   * (`AMB-T-5257`), and every `+` is held shut until it is there.
   */
  onInsertStep?: (edgeId: number) => void;
}) {
  const picture = layOut(automation);
  if (picture.nodes.length === 0) return <div className="auto__empty">{t("auto.pic.empty")}</div>;

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
              key={lap.headStepId}
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
            key={lap.headStepId}
            className="autopic__lapword"
            style={{ left: `${lap.x + 8}px`, top: `${lap.y}px` }}
          >
            {t("auto.pic.lap")}
          </span>
        ))}

        {picture.nodes.map((node) => (
          <button
            key={node.stepId}
            type="button"
            className={[
              "autopic__node",
              node.action !== undefined ? "autopic__node--action" : "",
              node.unfed.length > 0 ? "autopic__node--unfed" : "",
              node.stepId === selectedStepId ? "autopic__node--on" : "",
            ]
              .filter((one) => one !== "")
              .join(" ")}
            style={{
              left: `${node.x}px`,
              top: `${node.y}px`,
              width: `${node.w}px`,
              height: `${node.h}px`,
            }}
            aria-pressed={node.stepId === selectedStepId}
            disabled={onPickStep === undefined}
            onClick={() => onPickStep?.(node.stepId)}
          >
            <span className="autopic__nodename">{node.name}</span>
            {node.action !== undefined && (
              <span className="autopic__from">{tf("auto.pic.fromAction", { action: node.action })}</span>
            )}
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
            aria-label={t("auto.pic.insert")}
            disabled={onInsertStep === undefined}
            onClick={() => onInsertStep?.(insert.edgeId)}
          >
            <Icon name="plus" />
          </button>
        ))}
      </div>
    </div>
  );
}
