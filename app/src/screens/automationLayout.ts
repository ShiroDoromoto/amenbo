// Where every step of one automation is drawn, and how the lines between them run.
//
// **The picture holds no coordinates, and neither does the store** (`AMB-T-5255`). What a reader
// sees is worked out afresh from the definition every time it is drawn: the walk from the entry
// step decides the order, and this file turns that order into pixels. A canvas a person drags
// boxes around on would mean the definition carried a place for each step — and the one who builds
// an automation is an AI adding steps from the command line, which has nowhere to put a box.
//
// **Two divisions, in that order.** First the stretch: a run walks one task at a time
// (`amenbo_core::model::AutomationRunTask`), and a step that takes a task begins the next stretch,
// so the steps reached from it belong to that task's span and are drawn inside one dashed outline.
// Then the depth: how many steps from the head of that stretch, with everything at the same depth
// side by side. Nothing is told apart by colour — the outline is the division.
//
// **A line is drawn between its two boxes only when they are neighbours in the same stretch.**
// Anything else goes out to a lane in the margin: what comes after a step to the left, what is
// handed on to the right. A line that goes back to a shallower row is dashed there, and one that
// jumps forward over a row keeps its solid stroke — the reader is being told it leaves the column,
// not that it runs backwards.
//
// **The error way out is drawn only where somebody changed it.** Every step is born carrying it
// with nothing said about what follows, which core reads as stopping the run and calling a person
// (`amenbo_core::ops::automation::edge_delete`). So an edge on that way out *is* the change, and a
// step that never had one has no line here to draw.
import type {
  AutomationDetailDto,
  AutomationEdgeDto,
  AutomationStepDto,
} from "../bindings/bindings";

/** The name core gives the error way out — the one every step and every action is born with. */
export const ERROR_EXIT = "*";

/** How big a step's box is, and how much room is left around it. All of it fixed. */
const NODE_W = 220;
const NODE_H = 96;
/** Between two boxes standing side by side at the same depth. */
const COL_GAP = 24;
/** Between one depth and the next — the room a line and its `+` are drawn in. */
const ROW_GAP = 56;
/** Inside a stretch's dashed outline, and between one stretch and the next. */
const LAP_PAD = 16;
const LAP_GAP = 28;
/** Under the last row of a stretch, where a way out that goes nowhere hangs. */
const END_ROOM = 56;
/** One lane in the margin, and the margin outside the outermost one. */
const LANE_W = 16;
const PAD = 18;
/** How far in from a box's edge the first line is tied, and how far apart the next ones are. */
const ATTACH = 28;
const EXIT_GAP = 22;
/** How far a line hangs below a step before it turns, and how far a way out that goes nowhere runs. */
const DROP = 14;
const STUB = 40;
/** Where on a line's first leg the `+` that inserts a step sits. */
const INSERT_DROP = 20;

/** A point of a line, in the picture's own pixels. */
export type PicPoint = { x: number; y: number };

/** One step's box. */
export type PicNode = {
  stepId: number;
  name: string;
  x: number;
  y: number;
  w: number;
  h: number;
  /** What the library action this step runs is called, or nothing where it carries its own prompt. */
  action?: string;
  /** The required inputs nothing reaches. Empty where every one of them is fed. */
  unfed: readonly string[];
};

/** The dashed outline around the steps one task is worked by. */
export type PicLap = { headStepId: number; x: number; y: number; w: number; h: number };

/** One line, drawn as a polyline through its points. */
export type PicLine = {
  /** Stable across renders: what this line *is*, so React keeps the same element. */
  key: string;
  kind: "edge" | "wire";
  points: readonly PicPoint[];
  /** Dashed: it goes back to a row above the one it left. */
  back: boolean;
  /** The way out this edge hangs on, as core names it. Absent for the unnamed one and for a wire. */
  exitName?: string;
  /** How the run goes on where this edge names no step — it closes the task, or it stops. */
  ends?: "done" | "halt";
  /** What is handed on, for a wire: the way out's output and the input it lands in. */
  hands?: { from: string; to: string };
  /** Where the way out's name is written, beside the line's first leg. */
  at: PicPoint;
  /** Where the ending is written, at the foot of a line that names no step. */
  endAt?: PicPoint;
};

/** The `+` on a line, which puts a step in at that point. */
export type PicInsert = { edgeId: number; x: number; y: number };

/** One automation, laid out. */
export type Picture = {
  width: number;
  height: number;
  laps: readonly PicLap[];
  nodes: readonly PicNode[];
  lines: readonly PicLine[];
  inserts: readonly PicInsert[];
};

/**
 * Whether this step is the one that takes the next task — which is what begins a stretch.
 *
 * It is a `task_take` **output** on one of its ways out: the step goes and finds a task, and what it
 * comes out holding is what the run is about from there on
 * (`amenbo_core::ops::automation_run::takes_a_task`).
 */
function takesTask(step: AutomationStepDto): boolean {
  return step.exits.some((exit) => exit.outputs.some((port) => port.kind === "task_take"));
}

/** The steps one task is worked by, in the rows the walk put them in. */
type Lap = {
  /** The step that took the task, or nothing where these steps answer to no task at all. */
  head: number | null;
  rows: number[][];
};

/** What the walk came to: the stretches, and which steps a run could actually reach. */
type Walk = { laps: Lap[]; live: Set<number> };

/**
 * Walk the definition from its entry step and hand back the stretches, each cut into rows by depth.
 *
 * A step that takes a task is not walked into: it is queued as the head of the next stretch, so the
 * span of one task never runs on into the next. **Every step is placed**, reached or not — building
 * is always half-finished, and a step nothing points at yet is exactly the one its builder is
 * looking for.
 */
function walk(detail: AutomationDetailDto): Walk {
  const steps = new Map(detail.steps.map((step) => [step.id, step]));
  const out = new Map<number, AutomationEdgeDto[]>();
  for (const edge of detail.edges) {
    if (edge.ends !== "go" || edge.toStepId === undefined) continue;
    if (!steps.has(edge.toStepId)) continue;
    const from = out.get(edge.fromStepId) ?? [];
    from.push(edge);
    out.set(edge.fromStepId, from);
  }

  const placed = new Set<number>();
  // Reached from the entry along the edges that go on to a step — core's own reading, and the one
  // that decides which steps its launch check even looks at
  // (`amenbo_core::ops::automation_run::reachable`).
  const live = new Set<number>();
  const queued: number[] = [];
  if (detail.entryStepId !== undefined && steps.has(detail.entryStepId)) {
    queued.push(detail.entryStepId);
    live.add(detail.entryStepId);
  }

  const nextRoot = (): number | undefined => {
    while (queued.length > 0) {
      const id = queued.shift();
      if (id !== undefined && !placed.has(id)) return id;
    }
    return detail.steps.find((step) => !placed.has(step.id))?.id;
  };

  const laps: Lap[] = [];
  for (let root = nextRoot(); root !== undefined; root = nextRoot()) {
    const head = takesTask(steps.get(root)!) ? root : null;
    const rows: number[][] = [];
    let frontier = [root];
    placed.add(root);
    while (frontier.length > 0) {
      rows.push(frontier);
      const next: number[] = [];
      for (const from of frontier) {
        for (const edge of out.get(from) ?? []) {
          const to = edge.toStepId!;
          if (placed.has(to)) continue;
          if (live.has(from)) live.add(to);
          if (takesTask(steps.get(to)!)) {
            queued.push(to);
            continue;
          }
          placed.add(to);
          next.push(to);
        }
      }
      frontier = next;
    }
    laps.push({ head, rows });
  }
  return { laps, live };
}

/**
 * Whether anything actually reaches one required input — **core's rule, read off the same three
 * conditions** (`amenbo_core::ops::automation_run::fed`): the wire comes from a step a run
 * reaches, that step still exists, and the way out it leaves by really hands on a port of that name.
 *
 * It is worked out here because the launch check answers by step *name*, which is no way to find a
 * box. What a box says is a remark; the launch place above the picture is what refuses, and it reads
 * core's answer whole (`./AutomationBuildScreen`).
 */
function fed(
  detail: AutomationDetailDto,
  steps: Map<number, AutomationStepDto>,
  live: Set<number>,
  stepId: number,
  port: string,
): boolean {
  return detail.wires.some((wire) => {
    if (wire.toStepId !== stepId || wire.toPortName !== port) return false;
    if (!live.has(wire.fromStepId)) return false;
    const from = steps.get(wire.fromStepId);
    const exit = from?.exits.find((one) => one.name === wire.fromExitName);
    return exit?.outputs.some((one) => one.name === wire.fromPortName) ?? false;
  });
}

/** Where one row's boxes start, so that every row is centred on the same column of the picture. */
function rowStart(contentW: number, count: number): number {
  return Math.round((contentW - (count * NODE_W + (count - 1) * COL_GAP)) / 2);
}

/**
 * The lane each aside line is given, so that two of them running past the same rows never overlap.
 *
 * Read as the interval each line covers and coloured greedily: the first lane whose last line has
 * finished above this one's top takes it. Lane 0 is the one nearest the boxes.
 */
function lanes(spans: readonly { key: string; top: number; bottom: number }[]): Map<string, number> {
  const taken: number[] = [];
  const at = new Map<string, number>();
  for (const span of [...spans].sort((a, b) => a.top - b.top || a.key.localeCompare(b.key))) {
    let lane = taken.findIndex((bottom) => bottom < span.top);
    if (lane === -1) lane = taken.length;
    taken[lane] = span.bottom;
    at.set(span.key, lane);
  }
  return at;
}

/** Where a line is tied to a box: the nth way out down the left side, the nth port down the right. */
function attach(node: PicNode, nth: number, side: "left" | "right"): number {
  const inward = Math.min(ATTACH + nth * EXIT_GAP, NODE_W - ATTACH);
  return side === "left" ? node.x + inward : node.x + NODE_W - inward;
}

/** What one line is called, which is also what tells two of them apart. */
function lineKey(kind: "edge" | "wire", id: number): string {
  return `${kind}-${id}`;
}

/**
 * Where the name of the nth way out of a step is written: beside the line, and a step further down
 * for each way out after the first. Two names on one line would sit on top of each other — the lines
 * they belong to are only a finger apart at the box they leave.
 */
function word(sx: number, sy: number, nth: number): PicPoint {
  return { x: sx + 12, y: sy + 14 + nth * 15 };
}

/**
 * Lay one automation out.
 *
 * Everything is placed with the boxes starting at x=0 and shifted right at the end by however many
 * lanes the left margin turned out to need — which is not known until every line has one.
 */
export function layOut(detail: AutomationDetailDto | null): Picture {
  const empty: Picture = { width: 0, height: 0, laps: [], nodes: [], lines: [], inserts: [] };
  if (detail === null || detail.steps.length === 0) return empty;

  const steps = new Map(detail.steps.map((step) => [step.id, step]));
  const { laps, live } = walk(detail);
  const contentW = Math.max(
    NODE_W,
    ...laps.flatMap((lap) => lap.rows.map((row) => row.length * NODE_W + (row.length - 1) * COL_GAP)),
  );

  // Which row of which stretch each step landed in — what says whether two boxes are neighbours.
  const at = new Map<number, { lap: number; row: number }>();
  const nodes: PicNode[] = [];
  const outlines: PicLap[] = [];
  let y = PAD;
  laps.forEach((lap, nth) => {
    const pad = lap.head === null ? 0 : LAP_PAD;
    const top = y;
    lap.rows.forEach((row, depth) => {
      const rowY = top + pad + depth * (NODE_H + ROW_GAP);
      const startX = rowStart(contentW, row.length);
      row.forEach((stepId, column) => {
        const step = steps.get(stepId)!;
        at.set(stepId, { lap: nth, row: depth });
        nodes.push({
          stepId,
          name: step.name,
          x: startX + column * (NODE_W + COL_GAP),
          y: rowY,
          w: NODE_W,
          h: NODE_H,
          action: step.actionName,
          unfed: !live.has(stepId)
            ? []
            : step.inputs
                .filter((port) => port.required && !fed(detail, steps, live, stepId, port.name))
                .map((port) => port.name),
        });
      });
    });
    const inner = lap.rows.length * NODE_H + (lap.rows.length - 1) * ROW_GAP + END_ROOM;
    const height = inner + pad * 2;
    if (lap.head !== null) {
      outlines.push({ headStepId: lap.head, x: -LAP_PAD, y: top, w: contentW + LAP_PAD * 2, h: height });
    }
    y = top + height + LAP_GAP;
  });
  const height = y - LAP_GAP + PAD;

  const node = new Map(nodes.map((one) => [one.stepId, one]));
  // Two boxes are neighbours when one sits on the row under the other — which the last row of a
  // stretch and the head of the next one do, the outline between them being the only thing in the
  // way. The step that goes on to take the next task is the commonest line there is, and sending it
  // out to a lane would put the one line every automation has in the margin.
  const neighbours = (from: number, to: number): boolean => {
    const a = at.get(from);
    const b = at.get(to);
    if (a === undefined || b === undefined) return false;
    if (a.lap === b.lap) return b.row === a.row + 1;
    return b.lap === a.lap + 1 && b.row === 0 && a.row === laps[a.lap]!.rows.length - 1;
  };

  // The lines, in two passes: the ones drawn straight between their boxes, and the ones that have to
  // be given a lane first. An aside line's x is written as an offset from the content, because how
  // far out the lanes reach is not known until all of them are handed out.
  type Aside = { key: string; top: number; bottom: number; draw: (laneX: number) => PicLine };
  const lines: PicLine[] = [];
  const inserts: PicInsert[] = [];
  const asideLeft: Aside[] = [];
  const asideRight: Aside[] = [];

  for (const edge of detail.edges) {
    const from = node.get(edge.fromStepId);
    if (from === undefined) continue;
    const step = steps.get(edge.fromStepId)!;
    const nth = Math.max(0, step.exits.findIndex((exit) => exit.name === edge.exitName));
    const sx = attach(from, nth, "left");
    const sy = from.y + NODE_H;
    const key = lineKey("edge", edge.id);
    inserts.push({ edgeId: edge.id, x: sx, y: sy + INSERT_DROP });

    if (edge.ends !== "go" || edge.toStepId === undefined || !node.has(edge.toStepId)) {
      lines.push({
        key,
        kind: "edge",
        points: [{ x: sx, y: sy }, { x: sx, y: sy + STUB }],
        back: false,
        exitName: edge.exitName,
        ends: edge.ends === "go" ? undefined : edge.ends,
        at: word(sx, sy, nth),
        endAt: { x: sx + 10, y: sy + STUB + 4 },
      });
      continue;
    }

    const to = node.get(edge.toStepId)!;
    const tx = attach(to, 0, "left");
    const ty = to.y;
    if (neighbours(edge.fromStepId, edge.toStepId)) {
      const mid = Math.round((sy + ty) / 2);
      lines.push({
        key,
        kind: "edge",
        points: [{ x: sx, y: sy }, { x: sx, y: mid }, { x: tx, y: mid }, { x: tx, y: ty }],
        back: false,
        exitName: edge.exitName,
        at: word(sx, sy, nth),
      });
      continue;
    }
    const back = ty < sy;
    asideLeft.push({
      key,
      top: Math.min(sy + DROP, ty - DROP),
      bottom: Math.max(sy + DROP, ty - DROP),
      draw: (laneX) => ({
        key,
        kind: "edge",
        points: [
          { x: sx, y: sy },
          { x: sx, y: sy + DROP },
          { x: laneX, y: sy + DROP },
          { x: laneX, y: ty - DROP },
          { x: tx, y: ty - DROP },
          { x: tx, y: ty },
        ],
        back,
        exitName: edge.exitName,
        at: word(sx, sy, nth),
      }),
    });
  }

  for (const wire of detail.wires) {
    const from = node.get(wire.fromStepId);
    const to = node.get(wire.toStepId);
    if (from === undefined || to === undefined) continue;
    const fromStep = steps.get(wire.fromStepId)!;
    const toStep = steps.get(wire.toStepId)!;
    const exit = fromStep.exits.find((one) => one.name === wire.fromExitName);
    const sx = attach(from, Math.max(0, exit?.outputs.findIndex((p) => p.name === wire.fromPortName) ?? 0), "right");
    const sy = from.y + NODE_H;
    const tx = attach(to, Math.max(0, toStep.inputs.findIndex((p) => p.name === wire.toPortName)), "right");
    const ty = to.y;
    const key = lineKey("wire", wire.id);
    const hands = { from: wire.fromPortName, to: wire.toPortName };
    if (neighbours(wire.fromStepId, wire.toStepId)) {
      const mid = Math.round((sy + ty) / 2);
      lines.push({
        key,
        kind: "wire",
        points: [{ x: sx, y: sy }, { x: sx, y: mid }, { x: tx, y: mid }, { x: tx, y: ty }],
        back: false,
        hands,
        at: { x: sx, y: mid },
      });
      continue;
    }
    const back = ty < sy;
    asideRight.push({
      key,
      top: Math.min(sy + DROP, ty - DROP),
      bottom: Math.max(sy + DROP, ty - DROP),
      draw: (laneX) => ({
        key,
        kind: "wire",
        points: [
          { x: sx, y: sy },
          { x: sx, y: sy + DROP },
          { x: laneX, y: sy + DROP },
          { x: laneX, y: ty - DROP },
          { x: tx, y: ty - DROP },
          { x: tx, y: ty },
        ],
        back,
        hands,
        at: { x: laneX, y: Math.round((sy + ty) / 2) },
      }),
    });
  }

  const leftAt = lanes(asideLeft);
  const rightAt = lanes(asideRight);
  const leftLanes = asideLeft.length === 0 ? 0 : Math.max(...[...leftAt.values()]) + 1;
  const rightLanes = asideRight.length === 0 ? 0 : Math.max(...[...rightAt.values()]) + 1;
  for (const line of asideLeft) lines.push(line.draw(-LAP_PAD - (leftAt.get(line.key)! + 1) * LANE_W));
  for (const line of asideRight) lines.push(line.draw(contentW + LAP_PAD + (rightAt.get(line.key)! + 1) * LANE_W));

  // Everything was laid out with the boxes at x=0. Shift it right by the room the left margin took.
  const dx = PAD + LAP_PAD + leftLanes * LANE_W;
  return {
    width: dx + contentW + LAP_PAD + rightLanes * LANE_W + PAD,
    height,
    laps: outlines.map((lap) => ({ ...lap, x: lap.x + dx })),
    nodes: nodes.map((one) => ({ ...one, x: one.x + dx })),
    lines: lines.map((line) => ({
      ...line,
      points: line.points.map((p) => ({ x: p.x + dx, y: p.y })),
      at: { x: line.at.x + dx, y: line.at.y },
    })),
    inserts: inserts.map((one) => ({ ...one, x: one.x + dx })),
  };
}
