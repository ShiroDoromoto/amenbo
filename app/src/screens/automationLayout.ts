// Where every box of one picture is drawn, and how the lines between them run.
//
// **Two pictures, one layout** (`AMB-D-949`): an automation's boxes are the actions placed on it, and
// an action's are the steps inside it. Both are a name, the ways out it is left by and what it takes
// in, joined by edges and wires — so what is laid out here is that shape (`PicGraph`), and each
// screen hands its own definition over through the adapter beside it.
//
// **The picture holds no coordinates, and neither does the store** (`AMB-T-5255`). What a reader
// sees is worked out afresh from the definition every time it is drawn: the walk from the entry
// decides the order, and this file turns that order into pixels. A canvas a person drags boxes
// around on would mean the definition carried a spot for each box — and the one who builds an
// automation is an AI placing actions from the command line, which has nowhere to put one.
//
// **Two divisions, in that order.** First the stretch: a run walks one task at a time
// (`amenbo_core::model::AutomationRunTask`), and a box that takes a task begins the next
// stretch, so the boxes reached from it belong to that task's span and are drawn inside one
// dashed outline. Then the depth: how many boxes from the head of that stretch, with everything
// at the same depth side by side. Nothing is told apart by colour — the outline is the division.
//
// **A line is drawn between its two boxes only when they are neighbours in the same stretch.**
// Anything else goes out to a lane in the margin: what comes after a box to the left, what is
// handed on to the right. A line that goes back to a shallower row is dashed there, and one that
// jumps forward over a row keeps its solid stroke — the reader is being told it leaves the column,
// not that it runs backwards.
//
// **An action's picture has the action itself above and below it** (`AMB-D-949`): its input, a frame
// over the step it opens first, and its output, a frame under everything holding a mark for each way
// out it declares — so the steps read as running from the one to the other (`AMB-T-5369`). A step that leaves the action is a line into one of those, and what the action
// takes in or hands out is a wire from the top mark or into a bottom one — the boundary the core names
// `ACTION_BOUNDARY`. An automation's picture has no such boundary, and draws neither.
//
// **The error way out is drawn only where somebody changed it.** Every box is born carrying it
// with nothing said about what follows, which core reads as stopping the run and calling a person
// (`amenbo_core::ops::automation::edge_delete`). So an edge on that way out *is* the change, and a
// box that never had one has no line here to draw.
import type {
  AutomationActionDetailDto,
  AutomationDetailDto,
  AutomationEdgeDto,
  AutomationExitDto,
  AutomationPortDto,
  AutomationWireDto,
} from "../bindings/bindings";

/** One box of a picture: what it is called, the ways out it is left by, and what it takes in. */
export type PicBox = {
  id: number;
  name: string;
  exits: readonly AutomationExitDto[];
  inputs: readonly AutomationPortDto[];
};

/** One picture, whichever of the two it is: the boxes, the lines, and the box a run opens first. */
export type PicGraph = {
  /** The box a run opens first. Absent while the definition is still being built. */
  entryId?: number;
  boxes: readonly PicBox[];
  edges: readonly AutomationEdgeDto[];
  wires: readonly AutomationWireDto[];
  /**
   * The action itself, on an action's picture: what it takes in and the ways out it is left by.
   * Absent on an automation's, which has no boundary to cross.
   */
  boundary?: { inputs: readonly AutomationPortDto[]; exits: readonly AutomationExitDto[] };
};

/** One automation as a picture — its boxes are the actions placed on it. */
export function automationGraph(detail: AutomationDetailDto | null): PicGraph | null {
  if (detail === null) return null;
  return {
    entryId: detail.entryPlacementId,
    boxes: detail.placements,
    edges: detail.edges,
    wires: detail.wires,
  };
}

/** One library action as a picture — its boxes are the steps inside it. */
export function actionGraph(detail: AutomationActionDetailDto | null): PicGraph | null {
  if (detail === null) return null;
  return {
    entryId: detail.entryStepId,
    boxes: detail.steps,
    edges: detail.edges,
    wires: detail.wires,
    boundary: { inputs: detail.inputs, exits: detail.exits },
  };
}

/** The action itself, as either end of a wire inside it — core's `ACTION_BOUNDARY`. */
export const ACTION_BOUNDARY = 0;

/** The name core gives the error way out — the one every box of either picture is born with. */
export const ERROR_EXIT = "*";

/** How big a box is, and how much room is left around it. All of it fixed. */
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
/** How far a line hangs below a box before it turns, and how far a way out that goes nowhere runs. */
const DROP = 14;
const STUB = 40;
/** Where on a line's first leg the `+` that puts a box in sits. */
const INSERT_DROP = 20;
/** The action's input, a frame over the picture: its title, and what it takes in on one line. */
const IN_W = 260;
const IN_H = 54;
/** One way out of the action under the picture: its name, and what it hands on on one line. */
const OUT_W = 140;
const OUT_H = 44;
const OUT_GAP = 16;
/** The frame the ways out stand in: the room round them, and the room its title takes. */
const FRAME_PAD = 12;
const FRAME_TITLE = 22;

/** A point of a line, in the picture's own pixels. */
export type PicPoint = { x: number; y: number };

/** One box, laid out. */
export type PicNode = {
  boxId: number;
  name: string;
  x: number;
  y: number;
  w: number;
  h: number;
  /** The required inputs nothing reaches. Empty where every one of them is fed. */
  unfed: readonly string[];
};

/** The dashed outline around the boxes one task is worked by. */
export type PicLap = { headBoxId: number; x: number; y: number; w: number; h: number };

/** One line, drawn as a polyline through its points. */
export type PicLine = {
  /** Stable across renders: what this line *is*, so React keeps the same element. */
  key: string;
  kind: "edge" | "wire";
  points: readonly PicPoint[];
  /** Dashed: it goes back to a row above the one it left. */
  back: boolean;
  /** It crosses the action's own boundary: in from the top mark, or out into a way out's mark. */
  leaves?: boolean;
  /** The way out this edge hangs on, as core names it. Absent for the unnamed one and for a wire. */
  exitName?: string;
  /** How the run goes on where this edge names no box — it closes the task, or it stops. */
  ends?: "done" | "halt";
  /** What is handed on, for a wire: the way out's output and the input it lands in. */
  hands?: { from: string; to: string };
  /** Where the way out's name is written, beside the line's first leg. */
  at: PicPoint;
  /** Where the ending is written, at the foot of a line that names no box. */
  endAt?: PicPoint;
};

/** The `+` on a line, which puts a box in at that point. */
export type PicInsert = { edgeId: number; x: number; y: number };

/**
 * One mark for the action itself: its input over the picture (`in`), or one way out it is left by
 * (`out`) in the output frame under it. A mark is not a box: pressing one opens the action's own
 * input or output, not a step.
 */
export type PicMark = {
  key: string;
  kind: "in" | "out";
  /** The way out, for `out`. Absent for the unnamed one and for `in`. */
  exitName?: string;
  /** What the action takes in, for `in`; what leaving by this way out hands on, for `out`. */
  ports: readonly AutomationPortDto[];
  x: number;
  y: number;
  w: number;
  h: number;
};

/** A rectangle of the picture, in its own pixels. */
export type PicRect = { x: number; y: number; w: number; h: number };

/** One picture, laid out. */
export type Picture = {
  width: number;
  height: number;
  laps: readonly PicLap[];
  nodes: readonly PicNode[];
  lines: readonly PicLine[];
  inserts: readonly PicInsert[];
  /** The action's own marks. Empty on an automation's picture. */
  marks: readonly PicMark[];
  /** The frame the ways out of the action stand in, when there is that row. */
  outFrame?: PicRect;
};

/**
 * Whether this box is the one that takes the next task — which is what begins a stretch.
 *
 * It is a `task_take` **output** on one of its ways out: the box goes and finds a task, and what it
 * comes out holding is what the run is about from there on
 * (`amenbo_core::ops::automation_run::takes_a_task`).
 */
function takesTask(box: PicBox): boolean {
  return box.exits.some((exit) => exit.outputs.some((port) => port.kind === "task_take"));
}

/** The boxes one task is worked by, in the rows the walk put them in. */
type Lap = {
  /** The box that took the task, or nothing where these answer to no task at all. */
  head: number | null;
  rows: number[][];
};

/** What the walk came to: the stretches, and which boxes a run could actually reach. */
type Walk = { laps: Lap[]; live: Set<number> };

/**
 * Walk the picture from its entry box and hand back the stretches, each cut into rows by depth.
 *
 * A box that takes a task is not walked into: it is queued as the head of the next stretch, so the
 * span of one task never runs on into the next. **Every box is placed**, reached or not — building
 * is always half-finished, and a box nothing points at yet is exactly the one its builder is
 * looking for.
 */
function walk(graph: PicGraph): Walk {
  const boxes = new Map(graph.boxes.map((box) => [box.id, box]));
  const out = new Map<number, AutomationEdgeDto[]>();
  for (const edge of graph.edges) {
    if (edge.ends !== "go" || edge.toId === undefined) continue;
    if (!boxes.has(edge.toId)) continue;
    const from = out.get(edge.fromId) ?? [];
    from.push(edge);
    out.set(edge.fromId, from);
  }

  const placed = new Set<number>();
  // Reached from the entry along the edges that go on to a box — core's own reading, and the one
  // that decides which boxes its launch check even looks at
  // (`amenbo_core::ops::automation_run::reachable`).
  const live = new Set<number>();
  const queued: number[] = [];
  if (graph.entryId !== undefined && boxes.has(graph.entryId)) {
    queued.push(graph.entryId);
    live.add(graph.entryId);
  }

  const nextRoot = (): number | undefined => {
    while (queued.length > 0) {
      const id = queued.shift();
      if (id !== undefined && !placed.has(id)) return id;
    }
    return graph.boxes.find((box) => !placed.has(box.id))?.id;
  };

  const laps: Lap[] = [];
  for (let root = nextRoot(); root !== undefined; root = nextRoot()) {
    const head = takesTask(boxes.get(root)!) ? root : null;
    const rows: number[][] = [];
    let frontier = [root];
    placed.add(root);
    while (frontier.length > 0) {
      rows.push(frontier);
      const next: number[] = [];
      for (const from of frontier) {
        for (const edge of out.get(from) ?? []) {
          const to = edge.toId!;
          if (placed.has(to)) continue;
          if (live.has(from)) live.add(to);
          if (takesTask(boxes.get(to)!)) {
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

/** How the picture reads its boxes: the number each is shown with, and which lines go back. */
export type PicOrder = {
  /** Counted from 1, stretch by stretch and row by row — top to bottom, left to right. */
  numberOf: ReadonlyMap<number, number>;
  /** A line to a row no lower than the one it leaves — the one the picture draws dashed. */
  goesBack: (fromId: number, toId: number) => boolean;
  /** The box begins a stretch of its own, the next task being what it goes and finds. */
  takesTask: (boxId: number) => boolean;
};

/**
 * Read the picture's order off the same walk it is laid out by, so that a list naming the boxes
 * numbers them as the picture does and calls back the lines the picture draws going back.
 */
export function pictureOrder(graph: PicGraph): PicOrder {
  const boxes = new Map(graph.boxes.map((box) => [box.id, box]));
  const numberOf = new Map<number, number>();
  const at = new Map<number, { lap: number; row: number }>();
  walk(graph).laps.forEach((lap, nth) =>
    lap.rows.forEach((row, depth) =>
      row.forEach((boxId) => {
        numberOf.set(boxId, numberOf.size + 1);
        at.set(boxId, { lap: nth, row: depth });
      }),
    ),
  );
  return {
    numberOf,
    goesBack: (fromId, toId) => {
      const a = at.get(fromId);
      const b = at.get(toId);
      if (a === undefined || b === undefined) return false;
      return b.lap < a.lap || (b.lap === a.lap && b.row <= a.row);
    },
    takesTask: (boxId) => {
      const box = boxes.get(boxId);
      return box !== undefined && takesTask(box);
    },
  };
}

/**
 * Whether anything actually reaches one required input — **core's rule, read off the same three
 * conditions** (`amenbo_core::ops::automation_run::fed`): the wire comes from a box a run
 * reaches, that box still exists, and the way out it leaves by really hands on a port of that name.
 *
 * It is worked out here because the launch check answers by box *name*, which is no way to find a
 * box. What a box says is a remark; the launch place above the picture is what refuses, and it reads
 * core's answer whole (`./AutomationBuildScreen`).
 */
function fed(
  graph: PicGraph,
  boxes: Map<number, PicBox>,
  live: Set<number>,
  boxId: number,
  port: string,
): boolean {
  return graph.wires.some((wire) => {
    if (wire.toId !== boxId || wire.toPortName !== port) return false;
    // What the action itself was handed is there from the start, whatever the walk reached.
    if (wire.fromId === ACTION_BOUNDARY) {
      return graph.boundary?.inputs.some((one) => one.name === wire.fromPortName) ?? false;
    }
    if (!live.has(wire.fromId)) return false;
    const from = boxes.get(wire.fromId);
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
  const inward = Math.min(ATTACH + nth * EXIT_GAP, node.w - ATTACH);
  return side === "left" ? node.x + inward : node.x + node.w - inward;
}

/** What one line is called, which is also what tells two of them apart. */
function lineKey(kind: "edge" | "wire", id: number): string {
  return `${kind}-${id}`;
}

/**
 * Where the name of the nth way out of a box is written: beside the line, and a line further down
 * for each way out after the first. Two names on one line would sit on top of each other — the lines
 * they belong to are only a finger apart at the box they leave.
 */
function word(sx: number, sy: number, nth: number): PicPoint {
  return { x: sx + 12, y: sy + 14 + nth * 15 };
}

/**
 * Lay one picture out.
 *
 * Everything is placed with the boxes starting at x=0 and shifted right at the end by however many
 * lanes the left margin turned out to need — which is not known until every line has one.
 */
export function layOut(graph: PicGraph | null): Picture {
  const empty: Picture = { width: 0, height: 0, laps: [], nodes: [], lines: [], inserts: [], marks: [] };
  if (graph === null || graph.boxes.length === 0) return empty;

  const boxes = new Map(graph.boxes.map((box) => [box.id, box]));
  const { laps, live } = walk(graph);
  // The action's ways out, the error one last — the order the declaration lists them in.
  const outs = [...(graph.boundary?.exits ?? [])].sort(
    (a, b) => Number(a.name === ERROR_EXIT) - Number(b.name === ERROR_EXIT),
  );
  const contentW = Math.max(
    NODE_W,
    IN_W,
    outs.length * OUT_W + Math.max(0, outs.length - 1) * OUT_GAP + FRAME_PAD * 2,
    ...laps.flatMap((lap) => lap.rows.map((row) => row.length * NODE_W + (row.length - 1) * COL_GAP)),
  );

  // Which row of which stretch each box landed in — what says whether two boxes are neighbours.
  const at = new Map<number, { lap: number; row: number }>();
  const nodes: PicNode[] = [];
  const outlines: PicLap[] = [];
  // The mark a placement comes in by stands over everything, with a row's room under it.
  const over = graph.boundary === undefined ? 0 : IN_H + ROW_GAP;
  let y = PAD + over;
  laps.forEach((lap, nth) => {
    const pad = lap.head === null ? 0 : LAP_PAD;
    const top = y;
    lap.rows.forEach((row, depth) => {
      const rowY = top + pad + depth * (NODE_H + ROW_GAP);
      const startX = rowStart(contentW, row.length);
      row.forEach((boxId, column) => {
        const box = boxes.get(boxId)!;
        at.set(boxId, { lap: nth, row: depth });
        nodes.push({
          boxId,
          name: box.name,
          x: startX + column * (NODE_W + COL_GAP),
          y: rowY,
          w: NODE_W,
          h: NODE_H,
          unfed: !live.has(boxId)
            ? []
            : box.inputs
                .filter((port) => port.required && !fed(graph, boxes, live, boxId, port.name))
                .map((port) => port.name),
        });
      });
    });
    const inner = lap.rows.length * NODE_H + (lap.rows.length - 1) * ROW_GAP + END_ROOM;
    const height = inner + pad * 2;
    if (lap.head !== null) {
      outlines.push({ headBoxId: lap.head, x: -LAP_PAD, y: top, w: contentW + LAP_PAD * 2, h: height });
    }
    y = top + height + LAP_GAP;
  });
  const bottom = y - LAP_GAP;

  // The action's own marks, laid out as boxes are so a line can be tied to them the same way. They
  // join the map the lines are routed by and no list of boxes: they are not pressed.
  const marks: PicMark[] = [];
  const spots: PicNode[] = [];
  const outOf = new Map<string, number>();
  const spot = (id: number, x: number, y: number, w: number, h: number): PicNode => ({
    boxId: id,
    name: "",
    x,
    y,
    w,
    h,
    unfed: [],
  });
  let outFrame: PicRect | undefined;
  let height = bottom + PAD;
  if (graph.boundary !== undefined) {
    const inX = Math.round((contentW - IN_W) / 2);
    spots.push(spot(ACTION_BOUNDARY, inX, PAD, IN_W, IN_H));
    marks.push({ key: "in", kind: "in", ports: graph.boundary.inputs, x: inX, y: PAD, w: IN_W, h: IN_H });
    // The frame stands half a row under the last stretch; the ways out sit inside it, under its title.
    const frameY = bottom + Math.round(ROW_GAP / 2);
    const outY = frameY + FRAME_TITLE;
    const rowW = outs.length * OUT_W + Math.max(0, outs.length - 1) * OUT_GAP;
    const startX = Math.round((contentW - rowW) / 2);
    outs.forEach((exit, nth) => {
      const id = -(nth + 1);
      const x = startX + nth * (OUT_W + OUT_GAP);
      outOf.set(exit.name ?? "", id);
      spots.push(spot(id, x, outY, OUT_W, OUT_H));
      marks.push({
        key: `out:${exit.name ?? ""}`,
        kind: "out",
        exitName: exit.name,
        ports: exit.outputs,
        x,
        y: outY,
        w: OUT_W,
        h: OUT_H,
      });
      // The row sits after every stretch, so a box on the last row of the last one is its neighbour.
      at.set(id, { lap: laps.length, row: 0 });
    });
    outFrame = {
      x: startX - FRAME_PAD,
      y: frameY,
      w: rowW + FRAME_PAD * 2,
      h: FRAME_TITLE + OUT_H + FRAME_PAD,
    };
    height = frameY + outFrame.h + PAD;
  }

  const node = new Map([...nodes, ...spots].map((one) => [one.boxId, one]));
  /** The mark of the way out of the action an `exit` edge returns to. */
  const returnsTo = (edge: AutomationEdgeDto | undefined): number | undefined =>
    edge?.ends === "exit" ? outOf.get(edge.exitTo ?? "") : undefined;
  // Two boxes are neighbours when one sits on the row under the other — which the last row of a
  // stretch and the head of the next one do, the outline between them being the only thing in the
  // way. The box that goes on to take the next task is the commonest line there is, and sending it
  // out to a lane would put the one line every automation has in the margin.
  const neighbours = (from: number, to: number): boolean => {
    // The top mark stands right over the first row of the first stretch.
    if (from === ACTION_BOUNDARY && graph.boundary !== undefined) {
      const b = at.get(to);
      return b !== undefined && b.lap === 0 && b.row === 0;
    }
    const a = at.get(from);
    const b = at.get(to);
    if (a === undefined || b === undefined) return false;
    if (a.lap === b.lap) return b.row === a.row + 1;
    return b.lap === a.lap + 1 && b.row === 0 && a.row === laps[a.lap]?.rows.length - 1;
  };

  // The lines, in two passes: the ones drawn straight between their boxes, and the ones that have to
  // be given a lane first. An aside line's x is written as an offset from the content, because how
  // far out the lanes reach is not known until all of them are handed out.
  type Aside = { key: string; top: number; bottom: number; draw: (laneX: number) => PicLine };
  const lines: PicLine[] = [];
  const inserts: PicInsert[] = [];
  const asideLeft: Aside[] = [];
  const asideRight: Aside[] = [];

  // Where a placement comes in: a line from the top mark into the step it opens first.
  const entered = graph.entryId === undefined ? undefined : node.get(graph.entryId);
  const door = node.get(ACTION_BOUNDARY);
  if (graph.boundary !== undefined && entered !== undefined && door !== undefined) {
    const sx = door.x + Math.round(door.w / 2);
    const sy = door.y + door.h;
    const tx = attach(entered, 0, "left");
    const mid = Math.round((sy + entered.y) / 2);
    lines.push({
      key: "in",
      kind: "edge",
      points: [{ x: sx, y: sy }, { x: sx, y: mid }, { x: tx, y: mid }, { x: tx, y: entered.y }],
      back: false,
      leaves: true,
      at: { x: sx, y: sy },
    });
  }

  for (const edge of graph.edges) {
    const from = node.get(edge.fromId);
    if (from === undefined) continue;
    const box = boxes.get(edge.fromId)!;
    const nth = Math.max(0, box.exits.findIndex((exit) => exit.name === edge.exitName));
    const sx = attach(from, nth, "left");
    const sy = from.y + NODE_H;
    const key = lineKey("edge", edge.id);
    inserts.push({ edgeId: edge.id, x: sx, y: sy + INSERT_DROP });

    const toId = edge.ends === "go" ? edge.toId : returnsTo(edge);
    if (toId === undefined || !node.has(toId)) {
      lines.push({
        key,
        kind: "edge",
        points: [{ x: sx, y: sy }, { x: sx, y: sy + STUB }],
        back: false,
        exitName: edge.exitName,
        ends: edge.ends === "done" || edge.ends === "halt" ? edge.ends : undefined,
        at: word(sx, sy, nth),
        endAt: { x: sx + 10, y: sy + STUB + 4 },
      });
      continue;
    }

    const to = node.get(toId)!;
    const tx = attach(to, 0, "left");
    const ty = to.y;
    if (neighbours(edge.fromId, toId)) {
      const mid = Math.round((sy + ty) / 2);
      lines.push({
        key,
        kind: "edge",
        points: [{ x: sx, y: sy }, { x: sx, y: mid }, { x: tx, y: mid }, { x: tx, y: ty }],
        back: false,
        leaves: edge.ends === "exit",
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
        leaves: edge.ends === "exit",
        exitName: edge.exitName,
        at: word(sx, sy, nth),
      }),
    });
  }

  for (const wire of graph.wires) {
    // Out of the action itself comes one of its inputs, from the top mark; into it goes an output of
    // the way out the source step leaves the action by, into that way out's mark.
    const toId =
      wire.toId === ACTION_BOUNDARY
        ? returnsTo(
            graph.edges.find((one) => one.fromId === wire.fromId && one.exitName === wire.fromExitName),
          )
        : wire.toId;
    const from = node.get(wire.fromId);
    const to = toId === undefined ? undefined : node.get(toId);
    if (from === undefined || to === undefined || toId === undefined) continue;
    const outputs =
      wire.fromId === ACTION_BOUNDARY
        ? graph.boundary?.inputs ?? []
        : boxes.get(wire.fromId)?.exits.find((one) => one.name === wire.fromExitName)?.outputs ?? [];
    const inputs = boxes.get(toId)?.inputs ?? [];
    const sx = attach(from, Math.max(0, outputs.findIndex((p) => p.name === wire.fromPortName)), "right");
    const sy = from.y + from.h;
    const tx = attach(to, Math.max(0, inputs.findIndex((p) => p.name === wire.toPortName)), "right");
    const ty = to.y;
    const key = lineKey("wire", wire.id);
    const hands = { from: wire.fromPortName, to: wire.toPortName };
    if (neighbours(wire.fromId, toId)) {
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
    marks: marks.map((one) => ({ ...one, x: one.x + dx })),
    outFrame: outFrame === undefined ? undefined : { ...outFrame, x: outFrame.x + dx },
  };
}
