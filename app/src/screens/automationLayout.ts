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
// **Two divisions.** The stretch: a run walks one task at a time
// (`amenbo_core::model::AutomationRunTask`), and a box that takes a task begins the next
// stretch, so the boxes reached from the way out it hands the task on by belong to that task's span
// and are drawn inside one dashed outline. The row: the longest way down to a box from the entry,
// leaving out the lines that go back, with everything on the same row side by side. The outline is
// the division, and nothing is told apart by its fill.
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
// **The error way out is drawn on every box, changed or not** (`AMB-T-5501`). Every box is born
// carrying it with nothing said about what follows, which core reads as stopping the run and calling
// a person (`AMB-D-966`). A box with no edge on it is drawn with a line of its own to that stop, so a
// reader sees where a run goes when a step fails without having to know the default. That line has no
// `+`: there is no edge under it to put a box in on.
import { builtinWord } from "../core/builtinWords";
import { t } from "../core/i18n";
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
  /** Where the action standing on this box is kept: the device's library, or this project's. Absent
   *  on an action's picture, whose boxes are its own steps and come from no library. */
  global?: boolean;
  /** The built-in the action standing on this box is, by its key — said in place of the library. */
  builtin?: string;
  /** Who carries out each step of the action standing here — one per step it holds. Absent on an
   *  action's picture; on an automation's, none at all is an action with nothing in it yet. */
  steps?: readonly unknown[];
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

/** How big a box is, and how much room is left around it. All of it fixed. Two lines tall — the
 *  name, and under it where the action comes from or what it is missing — as the mock draws it. */
const NODE_W = 220;
const NODE_H = 46;
/** Between two boxes standing side by side at the same depth. */
const COL_GAP = 24;
/** Between one depth and the next — the room a line and its `+` are drawn in. */
const ROW_GAP = 56;
/** Inside a stretch's dashed outline, and between one stretch and the next. */
const LAP_PAD = 16;
/** Inside the outline's top: a line's room for the word over the box that takes the task. */
const LAP_TOP = 26;
const LAP_GAP = 28;
/** Under the last row of a stretch, where a way out that goes nowhere hangs. */
const END_ROOM = 56;
/** One lane in the left margin, where the edges that skip rows run, and the margin outside it. */
const LANE_W = 16;
/** How far apart the wires' trunks stand — wider than an edge's lane, for the name over each one. */
const WIRE_LANE_W = 28;
/** How far in from a box's right edge a wire that comes down into its top lands. */
const WIRE_IN = 15;
/** How far over a box the leg of a wire that comes down into its top runs, per input. */
const WIRE_OVER = 7;
/** How far a wire's name stands past the outermost trunk, and how far under its stem its foot sits. */
const WIRE_WORD = 4;
const PAD = 18;
/** How far in from a box's edge the first line is tied, and how far apart the next ones are. */
const ATTACH = 28;
const EXIT_GAP = 28;
/** How far a line hangs below a box before it turns into a lane. */
const DROP = 14;
/**
 * Where two lines in the margin share a box: how much further from it each one on a lane further
 * out turns, and after how many they stop spreading. Out of a box the one further out turns lower
 * and is tied further right; into one it turns higher and lands further right — so neither crosses
 * the other, and each reads as a line of its own down to its arrowhead.
 */
const STAIR = 6;
const STAIRS = 2;
/** Where the lines in from the margin land on a box's top, and how far apart — left of any line
 *  that comes in from the row above, which lands after them. */
const LAND = 12;
const LAND_GAP = 12;
/**
 * A way out that goes nowhere: where its `+` sits on it, how far the shortest of them runs, and how
 * much further each one to its left runs — one line of words, so every name has a row of its own.
 */
const STUB_PLUS = 12;
const STUB = 26;
const WORD_H = 15;
/** How far a name is written from the `+` it sits beside, and from the line it sits over. */
const BESIDE = 14;
const OVER = 12;
/** How much lower a line's `+` sits on its lane for each lane further out. */
const LANE_PLUS = 24;
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
  /** The number the box is shown with — the same one a list naming the boxes gives it
   *  (`pictureOrder`). Absent on the action's own marks, which are not numbered. */
  no?: number;
  /** It goes and takes the next task: a stretch begins here. */
  takes?: boolean;
  /** Where the action standing here is kept, on an automation's picture (`PicBox`). */
  global?: boolean;
  /** The built-in standing here, by its key (`PicBox`). */
  builtin?: string;
  /** The action standing here has nothing in it yet, so a run cannot be started on it. */
  empty?: boolean;
};

/** The dashed outline around the boxes one task is worked by. */
export type PicLap = { headBoxId: number; x: number; y: number; w: number; h: number };

/** One line, drawn as a polyline through its points. */
export type PicLine = {
  /** Stable across renders: what this line *is*, so React keeps the same element. */
  key: string;
  kind: "edge" | "wire";
  points: readonly PicPoint[];
  /** Dashed: it goes back to a box on the way down to the one it leaves. */
  back: boolean;
  /**
   * What colour an edge is drawn in, as the legend under the picture reads it: on to the next box,
   * one of several ways out of a box that branches, or the error way out and a stop. Absent on a
   * wire and on the line in from the action's top mark, which are told apart otherwise.
   */
  tone?: "next" | "branch" | "error";
  /** It crosses the action's own boundary: in from the top mark, or out into a way out's mark. */
  leaves?: boolean;
  /** The way out this edge hangs on, or a wire is handed on by, as core names it. Absent on the line
   *  in from the top mark and on a wire from the action's own inputs, which leave by no way out. */
  exitName?: string;
  /**
   * The built-in the box this line leaves is, by its key: its way out is written in the screen's
   * language (`lineWord`), while `exitName` stays the store's word the edge is matched on.
   */
  builtin?: string;
  /** How the run goes on where this edge names no box — it closes the task, or it stops. */
  ends?: "done" | "halt";
  /** What is handed on, for a wire: the way out's output, and every input it lands in. */
  hands?: { from: string; to: readonly string[] };
  /**
   * A wire's legs off its trunk, one per input it lands in. `points` is the stem out of the box it
   * leaves, and each branch runs from where the stem meets the trunk, along the trunk, and into
   * one input — so one output fed to three boxes is one line with three ends, not three lines
   * side by side.
   */
  branches?: readonly (readonly PicPoint[])[];
  /**
   * Where the way out's name is written — and, on a line that names no box, how it ends. Over the
   * middle of the leg that runs across, past every lane level with the `+` of a line in the margin,
   * under the foot of one that goes nowhere.
   */
  at: PicPoint;
  /** Which end of the words `at` is: where they start, their middle, or where they finish. */
  align: "start" | "middle" | "end";
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
  /** The way out, for `out`. Absent for `in`. */
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

/** What the walk came to: the stretches, which boxes a run could actually reach, and which lines go back. */
type Walk = { laps: Lap[]; live: Set<number>; back: Set<number> };

/**
 * Walk the picture from its entry box and hand back the stretches, each cut into rows.
 *
 * Three readings, in this order (`AMB-T-5423`):
 *
 * - **Which lines go back.** A walk down from the entry, depth first: a line to a box still on the
 *   path it came down by goes back. What nothing reaches is walked afterwards, in the order the boxes
 *   are listed, so it has an answer too.
 * - **The row, by the longest way there.** Every other line goes forward, and a box stands one row
 *   under the lowest box that leads to it — so a line that goes forward never runs up, and one that
 *   skips a row is the only one that leaves the column.
 * - **A task's span, by the way out the task is taken on.** From the box that takes a task, only the
 *   ways out that hand the task on are followed: the one that finds nothing left does not lead into
 *   that task's span. From there every line forward is followed, and a box that takes the next task
 *   is where the span stops. A box two spans reach belongs to the first.
 *
 * The spans are stacked by the row their head stands in; the boxes no span holds are stacked the same
 * way by their own row, and ones that come next to each other share one open stretch. **Every box is
 * placed**, reached or not — building is always half-finished, and a box nothing points at yet is
 * exactly the one its builder is looking for.
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
  const hasEntry = graph.entryId !== undefined && boxes.has(graph.entryId);

  // Reached from the entry along the edges that go on to a box — core's own reading, and the one
  // that decides which boxes its launch check even looks at
  // (`amenbo_core::ops::automation_run::reachable`).
  const live = new Set<number>();
  if (hasEntry) {
    const queue = [graph.entryId!];
    live.add(graph.entryId!);
    while (queue.length > 0) {
      for (const edge of out.get(queue.shift()!) ?? []) {
        if (live.has(edge.toId!)) continue;
        live.add(edge.toId!);
        queue.push(edge.toId!);
      }
    }
  }

  // Which lines go back, and the order the walk first came to each box — what breaks a tie.
  const back = new Set<number>();
  const seen = new Map<number, number>();
  const onPath = new Set<number>();
  const descend = (id: number): void => {
    seen.set(id, seen.size);
    onPath.add(id);
    for (const edge of out.get(id) ?? []) {
      const to = edge.toId!;
      if (onPath.has(to)) back.add(edge.id);
      else if (!seen.has(to)) descend(to);
    }
    onPath.delete(id);
  };
  if (hasEntry) descend(graph.entryId!);
  for (const box of graph.boxes) if (!seen.has(box.id)) descend(box.id);
  const order = (id: number): number => seen.get(id)!;
  const forward = (id: number): AutomationEdgeDto[] => (out.get(id) ?? []).filter((edge) => !back.has(edge.id));

  // The row: one under the lowest box that leads to it. Without the lines that go back what is left
  // has no loop, so a pass per box is enough for the longest way to settle.
  const rank = new Map(graph.boxes.map((box) => [box.id, 0]));
  for (let pass = 0; pass <= graph.boxes.length; pass++) {
    let moved = false;
    for (const box of graph.boxes) {
      for (const edge of forward(box.id)) {
        if (rank.get(edge.toId!)! < rank.get(box.id)! + 1) {
          rank.set(edge.toId!, rank.get(box.id)! + 1);
          moved = true;
        }
      }
    }
    if (!moved) break;
  }

  // The spans, one per box that takes a task, in the order the walk came to them.
  const owner = new Map<number, number>();
  const takers = graph.boxes.filter(takesTask).map((box) => box.id).sort((a, b) => order(a) - order(b));
  for (const head of takers) {
    const handsOn = new Set(
      boxes
        .get(head)!
        .exits.filter((exit) => exit.outputs.some((port) => port.kind === "task_take"))
        .map((exit) => exit.name),
    );
    owner.set(head, head);
    const queue = forward(head)
      .filter((edge) => handsOn.has(edge.exitName))
      .map((edge) => edge.toId!);
    while (queue.length > 0) {
      const id = queue.shift()!;
      if (owner.has(id) || takesTask(boxes.get(id)!)) continue;
      owner.set(id, head);
      for (const edge of forward(id)) queue.push(edge.toId!);
    }
  }

  // One group per span, and one per row for the boxes no span holds; stacked by the row they start
  // at, and where two start at the same row, by which the walk came to first.
  type Group = { head: number | null; members: number[] };
  const groups: Group[] = takers.map((head) => ({ head, members: [] }));
  const loose = new Map<number, Group>();
  for (const box of [...graph.boxes].sort((a, b) => order(a.id) - order(b.id))) {
    const head = owner.get(box.id);
    if (head !== undefined) {
      groups.find((one) => one.head === head)!.members.push(box.id);
      continue;
    }
    const row = rank.get(box.id)!;
    const group = loose.get(row) ?? { head: null, members: [] };
    if (!loose.has(row)) {
      loose.set(row, group);
      groups.push(group);
    }
    group.members.push(box.id);
  }
  const startsAt = (group: Group) => Math.min(...group.members.map((id) => rank.get(id)!));
  const firstSeen = (group: Group) => Math.min(...group.members.map(order));
  groups.sort((a, b) => startsAt(a) - startsAt(b) || firstSeen(a) - firstSeen(b));

  const laps: Lap[] = [];
  for (const group of groups) {
    const byRow = new Map<number, number[]>();
    for (const id of group.members) byRow.set(rank.get(id)!, [...(byRow.get(rank.get(id)!) ?? []), id]);
    const rows = [...byRow.keys()].sort((a, b) => a - b).map((row) => byRow.get(row)!);
    const last = laps[laps.length - 1];
    if (group.head === null && last !== undefined && last.head === null) last.rows.push(...rows);
    else laps.push({ head: group.head, rows });
  }
  return { laps, live, back };
}

/** How the picture reads its boxes: the number each is shown with, and which lines go back. */
export type PicOrder = {
  /** Counted from 1, stretch by stretch and row by row — top to bottom, left to right. */
  numberOf: ReadonlyMap<number, number>;
  /** A line that would close a loop — the one the picture draws dashed. It is asked of lines not
   *  drawn yet as well, so it is not read off the rows: a box with no line into it sits on the top
   *  row, and a line to it goes down once it is drawn. */
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
  const { laps, back } = walk(graph);
  laps.forEach((lap) =>
    lap.rows.forEach((row) =>
      row.forEach((boxId) => {
        numberOf.set(boxId, numberOf.size + 1);
      }),
    ),
  );
  // The lines that go down. They hold no loop, so a line goes back exactly when its far end already
  // leads down to where it leaves — the reading the walk gives the lines already drawn.
  const down = new Map<number, number[]>();
  for (const edge of graph.edges) {
    if (edge.ends !== "go" || edge.toId === undefined || back.has(edge.id)) continue;
    down.set(edge.fromId, [...(down.get(edge.fromId) ?? []), edge.toId]);
  }
  const leadsTo = (fromId: number, toId: number): boolean => {
    const seen = new Set([fromId]);
    const queue = [fromId];
    while (queue.length > 0) {
      const id = queue.shift()!;
      if (id === toId) return true;
      for (const next of down.get(id) ?? []) {
        if (seen.has(next)) continue;
        seen.add(next);
        queue.push(next);
      }
    }
    return false;
  };
  return {
    numberOf,
    goesBack: (fromId, toId) => boxes.has(fromId) && boxes.has(toId) && leadsTo(toId, fromId),
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
 * box. What a box says is a remark; the start press over the picture is what refuses, and it reads
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
 * Read as the interval each line covers, the shortest first: a line goes outside every line whose
 * rows lie within its own, and into the nearest lane no line running past the same rows holds.
 * Lane 0 is the one nearest the boxes. A long line on an inner lane would have every shorter one
 * cross it twice on its way out to its own lane and back in; outside them, it crosses none.
 */
function lanes(spans: readonly { key: string; top: number; bottom: number }[]): Map<string, number> {
  const placed: { top: number; bottom: number; lane: number }[] = [];
  const at = new Map<string, number>();
  const order = [...spans].sort(
    (a, b) => a.bottom - a.top - (b.bottom - b.top) || a.top - b.top || a.key.localeCompare(b.key),
  );
  for (const span of order) {
    const past = placed.filter((one) => one.top <= span.bottom && span.top <= one.bottom);
    const inside = past.filter((one) => span.top <= one.top && one.bottom <= span.bottom);
    let lane = inside.length === 0 ? 0 : Math.max(...inside.map((one) => one.lane)) + 1;
    while (past.some((one) => one.lane === lane)) lane++;
    placed.push({ top: span.top, bottom: span.bottom, lane });
    at.set(span.key, lane);
  }
  return at;
}

/** Where an edge is tied to a box: the nth way out, along its bottom from the left. A wire is tied
 *  down the box's right side instead, where the trunks are. */
function attach(node: PicNode, nth: number): number {
  return node.x + Math.min(ATTACH + nth * EXIT_GAP, node.w - ATTACH);
}

/** Where the nth line in from the margin lands along a box's top, from the left. */
function landAt(node: PicNode, nth: number): number {
  return node.x + Math.min(LAND + nth * LAND_GAP, node.w - LAND);
}

/** What one line is called, which is also what tells two of them apart. */
function lineKey(kind: "edge" | "wire", id: number): string {
  return `${kind}-${id}`;
}

/**
 * The way out a line hangs on, in the words it is written with: a built-in's in the screen's language,
 * anything else as core names it. Absent where the line leaves by no way out. The error way out is
 * written as a word of its own (`exitWord`).
 */
export function lineWord(line: Pick<PicLine, "exitName" | "builtin">): string | undefined {
  return line.exitName === undefined ? undefined : builtinWord(line.builtin, line.exitName);
}

/** The way out a line hangs on, in a word: the error one in the screen's words. Empty where the line
 *  leaves by no way out. */
export function exitWord(line: Pick<PicLine, "exitName" | "builtin">): string {
  if (line.exitName === ERROR_EXIT) return t("auto.pic.errorExit");
  return lineWord(line) ?? "";
}

/** Where the run goes where a line names no step, in a word. */
function endWord(line: Pick<PicLine, "ends">): string {
  if (line.ends === "done") return t("auto.pic.endsDone");
  if (line.ends === "halt") return t("auto.pic.endsHalt");
  return "";
}

/** The words written beside an edge: its way out, then where the run goes where it names no step. */
export function edgeWord(line: Pick<PicLine, "exitName" | "builtin" | "ends">): string {
  return [exitWord(line), endWord(line)].filter((one) => one !== "").join(" — ");
}

/**
 * About how wide a name on a line is written, for the room the margins keep for it. Only a guess:
 * the picture is laid out without a screen to measure on. A wide character (Japanese) takes the
 * font's size, anything else a little over half of it; the error way out is written as a word of
 * the reader's language, and none of them runs past five narrow letters.
 */
function wordW(exitName: string | undefined): number {
  if (exitName === undefined) return 0;
  if (exitName === ERROR_EXIT) return 40;
  return [...exitName].reduce((sum, one) => sum + (one.codePointAt(0)! > 0x2e80 ? 12 : 7), 0);
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
  const { laps, live, back: goesBack } = walk(graph);
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
    // Over the first row the outline stands higher, for the word over the box that takes the task.
    const over = lap.head === null ? 0 : LAP_TOP;
    const top = y;
    lap.rows.forEach((row, depth) => {
      const rowY = top + over + depth * (NODE_H + ROW_GAP);
      const startX = rowStart(contentW, row.length);
      row.forEach((boxId, column) => {
        const box = boxes.get(boxId)!;
        at.set(boxId, { lap: nth, row: depth });
        nodes.push({
          boxId,
          // A built-in's words are drawn in the screen's language; the box's own name stays the store's.
          name: builtinWord(box.builtin, box.name),
          x: startX + column * (NODE_W + COL_GAP),
          y: rowY,
          w: NODE_W,
          h: NODE_H,
          no: nodes.length + 1,
          takes: takesTask(box),
          global: box.global,
          builtin: box.builtin,
          empty: box.builtin === undefined && box.steps !== undefined && box.steps.length === 0,
          unfed: !live.has(boxId)
            ? []
            : box.inputs
                .filter((port) => port.required && !fed(graph, boxes, live, boxId, port.name))
                .map((port) => builtinWord(box.builtin, port.name)),
        });
      });
    });
    const inner = lap.rows.length * NODE_H + (lap.rows.length - 1) * ROW_GAP + END_ROOM;
    const height = inner + over + pad;
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
      outOf.set(exit.name, id);
      spots.push(spot(id, x, outY, OUT_W, OUT_H));
      marks.push({
        key: `out:${exit.name}`,
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
  /** A line in the margin: the rows it runs past, and the line once it has a lane. */
  type Aside = {
    key: string;
    top: number;
    bottom: number;
    /** The boxes an edge in the margin runs between. Absent on a wire's trunk. */
    fromId?: number;
    toId?: number;
    /** `stair`: where this line stands among the ones in the margin that leave its box, and among
     *  the ones that come into the box it goes to — 0 for the one on the innermost lane. */
    draw: (laneX: number, lane: number, stair: { out: number; in: number }) => PicLine;
  };
  const lines: PicLine[] = [];
  const inserts: PicInsert[] = [];
  const asideLeft: Aside[] = [];
  const asideRight: Aside[] = [];

  const toOf = (edge: AutomationEdgeDto): number | undefined => {
    const toId = edge.ends === "go" ? edge.toId : returnsTo(edge);
    return toId !== undefined && node.has(toId) ? toId : undefined;
  };
  // How many lines come into each box from the margin. They land first along its top, so a line
  // from the row above lands after them rather than on the last leg of one of them.
  const landing = new Map<number, number>();
  for (const edge of graph.edges) {
    const toId = toOf(edge);
    if (toId !== undefined && node.has(edge.fromId) && !neighbours(edge.fromId, toId)) {
      landing.set(toId, (landing.get(toId) ?? 0) + 1);
    }
  }
  /** Where a line from the row above lands on a box. */
  const fromAbove = (to: PicNode): number => {
    const aside = landing.get(to.boxId) ?? 0;
    return aside === 0 ? attach(to, 0) : landAt(to, aside);
  };

  // Where a placement comes in: a line from the top mark into the step it opens first.
  const entered = graph.entryId === undefined ? undefined : node.get(graph.entryId);
  const door = node.get(ACTION_BOUNDARY);
  if (graph.boundary !== undefined && entered !== undefined && door !== undefined) {
    const sx = door.x + Math.round(door.w / 2);
    const sy = door.y + door.h;
    const tx = fromAbove(entered);
    const mid = Math.round((sy + entered.y) / 2);
    lines.push({
      key: "in",
      kind: "edge",
      points: [{ x: sx, y: sy }, { x: sx, y: mid }, { x: tx, y: mid }, { x: tx, y: entered.y }],
      back: false,
      leaves: true,
      at: { x: sx, y: sy },
      align: "start",
    });
  }

  // Where each edge is tied to its box. Only a way out with a line is counted — the two every box is
  // born with are there on every box, and counting them would push every other line along. The
  // lines that leave for the left lane come first, since their first leg turns left; those that go
  // nowhere come last, so their words have the room to the right of every line.
  const reach = (edge: AutomationEdgeDto): number => {
    const toId = toOf(edge);
    if (toId === undefined) return 2;
    return neighbours(edge.fromId, toId) ? 1 : 0;
  };
  // The error way out of each box nobody drew a line from, as the line core reads it as: one that
  // halts the run. Its id is the box's, negated — no edge has one below zero.
  const unsaid: AutomationEdgeDto[] = graph.boxes
    .filter((box) => box.exits.some((exit) => exit.name === ERROR_EXIT))
    .filter((box) => !graph.edges.some((edge) => edge.fromId === box.id && edge.exitName === ERROR_EXIT))
    .map((box) => ({ id: -box.id, fromId: box.id, exitName: ERROR_EXIT, ends: "halt" }));
  const edges = [...graph.edges, ...unsaid];
  const slot = new Map<number, { nth: number; below: number }>();
  for (const box of graph.boxes) {
    const exitAt = (edge: AutomationEdgeDto) => box.exits.findIndex((exit) => exit.name === edge.exitName);
    const own = edges
      .filter((edge) => edge.fromId === box.id)
      .sort((a, b) => reach(a) - reach(b) || exitAt(a) - exitAt(b));
    const nowhere = own.filter((edge) => reach(edge) === 2).length;
    let seen = 0;
    own.forEach((edge, nth) => {
      // How many lines that go nowhere stand to this one's right: its words go that many rows lower.
      const below = reach(edge) === 2 ? nowhere - 1 - seen++ : 0;
      slot.set(edge.id, { nth, below });
    });
  }

  for (const edge of edges) {
    const from = node.get(edge.fromId);
    if (from === undefined) continue;
    const { nth, below } = slot.get(edge.id)!;
    const sx = attach(from, nth);
    const sy = from.y + NODE_H;
    const key = lineKey("edge", edge.id);

    const toId = toOf(edge);
    if (toId === undefined) {
      // The one further left runs further down, so its words pass under the shorter lines to its right.
      const foot = sy + STUB + below * WORD_H;
      if (edge.id > 0) inserts.push({ edgeId: edge.id, x: sx, y: sy + STUB_PLUS });
      lines.push({
        key,
        kind: "edge",
        points: [{ x: sx, y: sy }, { x: sx, y: foot }],
        back: false,
        exitName: edge.exitName,
        builtin: from.builtin,
        ends: edge.ends === "done" || edge.ends === "halt" ? edge.ends : undefined,
        at: { x: sx - 6, y: foot + OVER },
        align: "start",
      });
      continue;
    }

    const to = node.get(toId)!;
    const ty = to.y;
    if (neighbours(edge.fromId, toId)) {
      const tx = fromAbove(to);
      const mid = Math.round((sy + ty) / 2);
      const across = Math.round((sx + tx) / 2);
      inserts.push({ edgeId: edge.id, x: across, y: mid });
      // A line straight down has no leg across to write over: the name goes beside its `+`, on the
      // left, where the lines that go nowhere do not write theirs.
      const straight = Math.abs(tx - sx) < BESIDE * 2;
      lines.push({
        key,
        kind: "edge",
        points: [{ x: sx, y: sy }, { x: sx, y: mid }, { x: tx, y: mid }, { x: tx, y: ty }],
        back: false,
        leaves: edge.ends === "exit",
        exitName: edge.exitName,
        builtin: from.builtin,
        at: straight ? { x: Math.min(sx, tx) - BESIDE, y: mid + 4 } : { x: across, y: mid - OVER },
        align: straight ? "end" : "middle",
      });
      continue;
    }
    // The walk's own reading, not where the two boxes landed: a span stacked by the row it starts at
    // can put a line going forward above the box it leaves.
    const back = goesBack.has(edge.id);
    // The rows it runs past, however far along the stair at either end it turns.
    const top = Math.min(sy + DROP, ty - DROP - STAIRS * STAIR);
    const bottom = Math.max(sy + DROP + STAIRS * STAIR, ty - DROP);
    asideLeft.push({
      key,
      top,
      bottom,
      fromId: edge.fromId,
      toId,
      draw: (laneX, lane, stair) => {
        // Halfway down the lane, and a `+` lower for each lane further out: two lines running past
        // the same rows would otherwise have their `+` side by side.
        const middle = Math.min(Math.round((top + bottom) / 2) + lane * LANE_PLUS, bottom - LANE_PLUS / 2);
        inserts.push({ edgeId: edge.id, x: laneX, y: middle });
        // The lines of one box that leave for the margin are the first ways out along its bottom, so
        // they take those places in the order of their lanes, the innermost leftmost.
        const outX = attach(from, stair.out);
        const outY = sy + DROP + Math.min(stair.out, STAIRS) * STAIR;
        const inX = landAt(to, stair.in);
        const inY = ty - DROP - Math.min(stair.in, STAIRS) * STAIR;
        return {
          key,
          kind: "edge",
          points: [
            { x: outX, y: sy },
            { x: outX, y: outY },
            { x: laneX, y: outY },
            { x: laneX, y: inY },
            { x: inX, y: inY },
            { x: inX, y: ty },
          ],
          back,
          leaves: edge.ends === "exit",
          exitName: edge.exitName,
          builtin: from.builtin,
          // Level with the leg it leaves its box by, past the outermost lane: halfway along a long
          // lane the words would stand beside some other box, and read as that one's way out
          // (`AMB-T-5592`). Past every lane nothing crosses them; two lines leaving one box write
          // theirs a row apart, however many there are.
          at: { x: leftWordsX - BESIDE, y: sy + DROP + stair.out * WORD_H + 4 },
          align: "end",
        };
      },
    });
  }

  // The wires, one trunk per output: what one way out of one box hands on is a single line however
  // many boxes take it, split just before each input it lands in. They all go out to the right,
  // never straight down between two boxes — a wire under a box would cross the names of the lines
  // that leave it there.
  //
  // **Every end on a box's right side has a height of its own** (`AMB-T-5500`): the legs coming in
  // stand over the stems going out, one per wire even where two land in the same input, so a line
  // that leaves a box never reads as one coming into it and two that land together never run as
  // one. Which height goes to which is settled once the trunks have their lanes (`sideSlots`).
  type End = {
    toId: number;
    input: string;
    x: number;
    /** Where the leg lands, and the height it runs across at — the same as the landing, unless it
     *  comes down into the box from above. Both wait for `sideSlots` on a leg into the right side. */
    y: number;
    across: number;
    /** It comes over the row and down into the box's top, rather than into its right side. */
    over: boolean;
  };
  type Trunk = {
    key: string;
    fromId: number;
    exitName?: string;
    builtin?: string;
    port: string;
    sx: number;
    /** Where the stem leaves the box — waits for `sideSlots`. */
    sy: number;
    ends: End[];
  };
  const trunks = new Map<string, Trunk>();
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
    const id = `${wire.fromId}\u0000${wire.fromExitName ?? ""}\u0000${wire.fromPortName}`;
    let trunk = trunks.get(id);
    if (trunk === undefined) {
      trunk = {
        key: lineKey("wire", wire.id),
        fromId: wire.fromId,
        exitName: wire.fromExitName,
        builtin: from.builtin,
        port: builtinWord(from.builtin, wire.fromPortName),
        sx: from.x + from.w,
        sy: from.y + Math.round(from.h / 2),
        ends: [],
      };
      trunks.set(id, trunk);
    }
    const inputs = toId === ACTION_BOUNDARY ? [] : boxes.get(toId)?.inputs ?? [];
    const nth = Math.max(0, inputs.findIndex((p) => p.name === wire.toPortName));
    const input = builtinWord(to.builtin, wire.toPortName);
    // Into the right side, unless another box stands to the right on the same row: a leg across
    // would run through that one, so it comes over the row and down into this box's top instead.
    const hemmed = [...nodes, ...spots].some((one) => one.y === to.y && one.x > to.x);
    trunk.ends.push(
      hemmed
        ? {
            toId,
            input,
            x: to.x + to.w - WIRE_IN - nth * WORD_H,
            y: to.y,
            // A row apart per input, and all of them under the legs of the edges coming into the row.
            across: to.y - WIRE_OVER - nth * WIRE_OVER,
            over: true,
          }
        : { toId, input, x: to.x + to.w, y: to.y, across: to.y, over: false },
    );
  }
  for (const trunk of trunks.values()) {
    // The rows it runs past, read off the boxes at its ends: where along their sides it leaves and
    // lands is only settled once every trunk has its lane.
    const from = node.get(trunk.fromId)!;
    const ys = [from.y, from.y + from.h];
    for (const end of trunk.ends) {
      const to = node.get(end.toId)!;
      ys.push(end.over ? end.across : to.y, to.y + to.h);
    }
    asideRight.push({
      key: trunk.key,
      top: Math.min(...ys),
      bottom: Math.max(...ys),
      draw: (laneX) => ({
        key: trunk.key,
        kind: "wire",
        points: [{ x: trunk.sx, y: trunk.sy }, { x: laneX, y: trunk.sy }],
        branches: trunk.ends.map((end) => [
          { x: laneX, y: trunk.sy },
          { x: laneX, y: end.across },
          { x: end.x, y: end.across },
          ...(end.across === end.y ? [] : [{ x: end.x, y: end.y }]),
        ]),
        back: false,
        exitName: trunk.exitName,
        builtin: trunk.builtin,
        hands: { from: trunk.port, to: trunk.ends.map((one) => one.input) },
        // Past the outermost trunk, level with the stem: nothing runs out there, so no trunk crosses
        // the words, and each stem out of a box has a height of its own for them.
        at: { x: wordsX + WIRE_WORD, y: trunk.sy + WIRE_WORD },
        align: "start",
      }),
    });
  }

  const leftAt = lanes(asideLeft);
  const rightAt = lanes(asideRight);
  // `sideSlots`: the heights down each box's right side, handed out now that every trunk has its
  // lane. Legs in stand over stems out. Two lines at one box are put in the order that keeps them
  // from crossing: a leg from a trunk further out passes over the nearer trunk, so it lands on the
  // far side of that trunk's own leg — lower where the two come down from above, higher where they
  // come up from below — and a stem to a trunk further out leaves on the far side the same way.
  {
    type Slot = { group: number; order: number; set: (y: number) => void };
    const sides = new Map<number, Slot[]>();
    const put = (boxId: number, slot: Slot) => sides.set(boxId, [...(sides.get(boxId) ?? []), slot]);
    for (const trunk of trunks.values()) {
      const from = node.get(trunk.fromId)!;
      const lane = rightAt.get(trunk.key) ?? 0;
      const ends = trunk.ends.map((end) => node.get(end.toId)!.y);
      const up = ends.reduce((sum, y) => sum + y, 0) / ends.length < from.y;
      put(trunk.fromId, {
        group: up ? 2 : 3,
        order: up ? lane : -lane,
        set: (y) => (trunk.sy = y),
      });
      for (const end of trunk.ends) {
        if (end.over) continue;
        const above = from.y < node.get(end.toId)!.y;
        put(end.toId, {
          group: above ? 0 : 1,
          order: above ? lane : -lane,
          set: (y) => {
            end.y = y;
            end.across = y;
          },
        });
      }
    }
    for (const [boxId, slots] of sides) {
      const box = node.get(boxId)!;
      slots
        .sort((a, b) => a.group - b.group || a.order - b.order)
        .forEach((slot, nth) => slot.set(box.y + Math.round((box.h * (nth + 1)) / (slots.length + 1))));
    }
  }
  const leftLanes = asideLeft.length === 0 ? 0 : Math.max(...[...leftAt.values()]) + 1;
  const rightLanes = asideRight.length === 0 ? 0 : Math.max(...[...rightAt.values()]) + 1;
  /** Where each line in the margin stands among the ones sharing its box at one end, innermost first. */
  const stairOf = (end: (line: Aside) => number | undefined): Map<string, number> => {
    const shared = new Map<number, Aside[]>();
    for (const line of asideLeft) {
      const id = end(line);
      if (id !== undefined) shared.set(id, [...(shared.get(id) ?? []), line]);
    }
    const stair = new Map<string, number>();
    for (const group of shared.values()) {
      group
        .sort((a, b) => leftAt.get(a.key)! - leftAt.get(b.key)!)
        .forEach((line, nth) => stair.set(line.key, nth));
    }
    return stair;
  };
  const outStair = stairOf((line) => line.fromId);
  const inStair = stairOf((line) => line.toId);
  const leftWordsX = -LAP_PAD - leftLanes * LANE_W;
  for (const line of asideLeft) {
    const lane = leftAt.get(line.key)!;
    const stair = { out: outStair.get(line.key) ?? 0, in: inStair.get(line.key) ?? 0 };
    lines.push(line.draw(-LAP_PAD - (lane + 1) * LANE_W, lane, stair));
  }
  const wordsX = contentW + LAP_PAD + rightLanes * WIRE_LANE_W;
  for (const line of asideRight) {
    const lane = rightAt.get(line.key)!;
    lines.push(line.draw(contentW + LAP_PAD + (lane + 1) * WIRE_LANE_W, lane, { out: 0, in: 0 }));
  }

  // Everything was laid out with the boxes at x=0. Shift it right by the room the left margin took:
  // the lanes, and any name written further out than the boxes start.
  const leftRoom = Math.max(
    LAP_PAD + leftLanes * LANE_W,
    ...lines.map((line) => {
      if (line.kind !== "edge") return 0;
      const wide = wordW(edgeWord(line));
      return -(line.align === "end" ? line.at.x - wide : line.align === "middle" ? line.at.x - wide / 2 : line.at.x);
    }),
  );
  const dx = PAD + leftRoom;
  // The colour each edge is drawn in (`PicLine.tone`). A box with two or more named ways out is one
  // the run branches at, and every line out of it says so; the error way out, and a line that stops
  // the run, are the stop colour.
  const named = (box: PicBox | undefined) =>
    box?.exits.filter((exit) => exit.name !== undefined && exit.name !== ERROR_EXIT).length ?? 0;
  const tones = new Map<string, PicLine["tone"]>(
    edges.map((edge) => [
      lineKey("edge", edge.id),
      edge.exitName === ERROR_EXIT || edge.ends === "halt"
        ? "error"
        : named(boxes.get(edge.fromId)) > 1
          ? "branch"
          : "next",
    ]),
  );
  // And the room the right margin takes: the trunks, the name written past the outermost one, and the
  // words of an edge that run on past the boxes — the way out and where the run goes, of the last box.
  const rightRoom = Math.max(
    LAP_PAD + rightLanes * WIRE_LANE_W,
    ...lines.map((line) => {
      if (line.kind === "edge") {
        const wide = wordW(edgeWord(line));
        return (line.align === "start" ? line.at.x + wide : line.align === "middle" ? line.at.x + wide / 2 : line.at.x) - contentW;
      }
      if (line.kind !== "wire") return 0;
      // The way out it leaves by is written first where it has a name, with a separator after it.
      const exit = line.exitName === undefined ? 0 : wordW(lineWord(line)) + BESIDE;
      return line.at.x + exit + wordW(line.hands?.from) - contentW;
    }),
  );
  return {
    width: dx + contentW + rightRoom + PAD,
    height,
    laps: outlines.map((lap) => ({ ...lap, x: lap.x + dx })),
    nodes: nodes.map((one) => ({ ...one, x: one.x + dx })),
    lines: lines.map((line) => ({
      ...line,
      tone: line.kind === "edge" ? tones.get(line.key) : undefined,
      points: line.points.map((p) => ({ x: p.x + dx, y: p.y })),
      branches: line.branches?.map((branch) => branch.map((p) => ({ x: p.x + dx, y: p.y }))),
      at: { x: line.at.x + dx, y: line.at.y },
    })),
    inserts: inserts.map((one) => ({ ...one, x: one.x + dx })),
    marks: marks.map((one) => ({ ...one, x: one.x + dx })),
    outFrame: outFrame === undefined ? undefined : { ...outFrame, x: outFrame.x + dx },
  };
}
