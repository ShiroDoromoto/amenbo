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
// Anything else goes out to a lane in the margin on one side or the other, and the lines from one
// margin into the same box run on into it as one. A line that goes back to a shallower row is
// dashed there, and one that jumps forward over a row keeps its solid stroke — the reader is being
// told it leaves the column, not that it runs backwards.
//
// **An action's picture has the action itself above and below it** (`AMB-D-949`): its input, a frame
// over the step it opens first, and its output, a frame under everything holding a mark for each way
// out it declares — so the steps read as running from the one to the other (`AMB-T-5369`). A step that
// leaves the action is a line into one of those — the boundary the core names `ACTION_BOUNDARY`. An
// automation's picture has no such boundary, and draws neither.
//
// **What a box hands on is not drawn** (`AMB-D-1001`). The wires are only read here for the inputs
// nothing reaches, which a box wears as its "⚠". Drawn as lines they took the whole right margin and
// ran across the tops of the boxes, and nothing about them is done on the picture: a wire is made,
// dropped and read in the panel beside it.
//
// **A way out nothing has been decided for is drawn, dashed, with the press that puts the next box on
// it at its end** (`AMB-D-1003`). Without it a box's ways out had no line until one was decided, so
// no `+` stood anywhere to add the next box by, and a reader could not find how to go on. They are
// few, and each is one somebody has to fill before a launch, so the press stands there all the time.
// The row under a box with one stands lower, by as much as they hang, and the lines down to it turn
// under them — only there, so a picture with none keeps every box where it was (`AMB-T-5789`).
// Where the press beside a long name runs past its box, the next box in the row stands further right,
// so the press does not lie over the lines that box sends down (`AMB-T-5790`).
// The error way out is not one of them: a box leaves by it to a person without any line (below).
//
// **The error way out is drawn only where somebody drew a line from it.** Every box is born carrying
// it with nothing said about what follows, which core reads as stopping the run and calling a person
// (`AMB-D-966`). Drawn on every box, that one line said the same thing seven times over and crowded
// the picture it was meant to explain, so the legend says it once instead, and the panel beside the
// picture names it for the box picked.
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
  /** The way out the built-in standing here never leaves by, as its settings stand. Absent where it
   *  may leave by any of them, and on an action's picture. */
  neverLeavesBy?: string;
  /** Who carries out each step of the action standing here — one per step it holds. Absent on an
   *  action's picture; on an automation's, none at all is an action with nothing in it yet. */
  steps?: readonly unknown[];
  /** The action standing here was made on the spot and is still being made (`AMB-D-1005`). Absent on
   *  an action's picture, and wherever it is not. */
  draft?: boolean;
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
/** One lane in a margin, where the edges that skip rows run, and the margin outside it. */
const LANE_W = 16;
const PAD = 18;
/** How far in from a box's edge the first line is tied, and how far apart the next ones are. */
const ATTACH = 28;
const EXIT_GAP = 28;
/** How far a line hangs below a box before it turns into a lane. */
const DROP = 14;
/**
 * Where two lines leave one box for the same margin: how much lower each one on a lane further out
 * turns, and after how many they stop spreading. The one further out is also tied further from that
 * margin's side of the box — so neither crosses the other, and each reads as a line of its own.
 */
const STAIR = 6;
const STAIRS = 2;
/** Where the line in from the left margin lands on a box's top — left of any line that comes in from
 *  the row above, which lands one place after it — and, from the right margin, as far in from the
 *  right. */
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
/** How much lower each way out with nothing decided sits than the one to its right — the height of
 *  the press at its end, so one row of words and a press never lie over the next. */
const OPEN_H = 26;
/** How tall that press stands, and how far over what hangs lowest from a row the line down to the
 *  next row turns — the room its name is written in, over the turn. */
const PRESS_H = 24;
const LEG_CLEAR = 26;

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
  /** The action standing here is still being made (`PicBox`). It says so in place of "empty": an
   *  action being made is expected to have nothing in it yet. */
  draft?: boolean;
};

/** The dashed outline around the boxes one task is worked by. */
export type PicLap = { headBoxId: number; x: number; y: number; w: number; h: number };

/** One line, drawn as a polyline through its points. */
export type PicLine = {
  /** Stable across renders: what this line *is*, so React keeps the same element. */
  key: string;
  points: readonly PicPoint[];
  /** Dashed: it goes back to a box on the way down to the one it leaves. */
  back: boolean;
  /**
   * What colour an edge is drawn in, as the legend under the picture reads it: on to the next box,
   * one of several ways out of a box that branches, or the error way out and a stop. Absent on the
   * line in from the action's top mark, which is told apart otherwise.
   */
  tone?: "next" | "branch" | "error";
  /** It crosses the action's own boundary: in from the top mark, or out into a way out's mark. */
  leaves?: boolean;
  /** The way out this edge hangs on, as core names it. Absent on the line in from the top mark, which
   *  leaves by no way out. */
  exitName?: string;
  /**
   * The built-in the box this line leaves is, by its key: its way out is written in the screen's
   * language (`lineWord`), while `exitName` stays the store's word the edge is matched on.
   */
  builtin?: string;
  /** How the run goes on where this edge names no box — it closes the task, or it stops. */
  ends?: "done" | "halt";
  /**
   * Where the way out's name is written — and, on a line that names no box, how it ends. Over the
   * middle of the leg that runs across, over the leg a line in the margin leaves its box by just
   * left of that box's row, under the foot of one that goes nowhere.
   */
  at: PicPoint;
  /** Which end of the words `at` is: where they start, their middle, or where they finish. */
  align: "start" | "middle" | "end";
  /**
   * It ends on its lane, where it joins the other lines from the same margin into the same box: the
   * line that runs on into the box carries the one arrowhead. That line is no edge, and its key is
   * not an edge's.
   */
  joins?: boolean;
  /** It is a way out nothing has been decided for yet, and no edge: it ends in the press that puts
   *  the next box on it (`PicOpen`). */
  open?: boolean;
};

/** The `+` on a line, which puts a box in at that point. */
export type PicInsert = { edgeId: number; x: number; y: number };

/**
 * **The press at the end of a way out nothing has been decided for** (`AMB-D-1003`): it puts the next
 * box on after that way out. `x` is where its left edge stands and `y` its middle.
 */
export type PicOpen = { boxId: number; exitName: string; x: number; y: number };

/** What the press at the end of a way out with nothing decided says — a picture's boxes are actions
 *  on an automation's and steps on an action's. */
export function openWord(inAction: boolean): string {
  return t(inAction ? "auto.act.openPut" : "auto.pic.openPut");
}

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
  /** The ways out nothing has been decided for, each with its press. */
  opens: readonly PicOpen[];
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
 *
 * **Not on the way out the box never leaves by** (`AMB-T-5669`): a built-in whose settings choose the
 * other of its two ways out — "file a task" left to leave the task it filed untaken — hands nothing on
 * through it, so it takes no task however that way out is drawn. Core decides the same way.
 */
function takesTask(box: PicBox): boolean {
  return box.exits.some((exit) =>
    exit.name !== box.neverLeavesBy && exit.outputs.some((port) => port.kind === "task_take"));
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
 * Whether anything actually reaches one required input — **core's rule, read off the same conditions**
 * (`amenbo_core::ops::automation_run::fed`): the wire comes from a box a run can come to **before it
 * first comes to this one**, that box still exists, and the way out it leaves by really hands on a port
 * of that name. A wire from the box's own way out, or from a box only reached through it, carries
 * nothing the first time a run arrives, and the run fails there on no_input (`AMB-T-5641`).
 *
 * It is worked out here because the launch check answers by box *name*, which is no way to find a
 * box. What a box says is a remark; the start press over the picture is what refuses, and it reads
 * core's answer whole (`./AutomationBuildScreen`).
 */
export function fed(graph: PicGraph, boxId: number, port: string): boolean {
  const boxes = new Map(graph.boxes.map((box) => [box.id, box]));
  if (readAtLaunch(graph, boxes.get(boxId)?.builtin, boxId, port)) return true;
  let before: Set<number> | undefined;
  return graph.wires.some((wire) => {
    if (wire.toId !== boxId || wire.toPortName !== port) return false;
    // What the action itself was handed is there from the start, whatever the walk reached.
    if (wire.fromId === ACTION_BOUNDARY) {
      return graph.boundary?.inputs.some((one) => one.name === wire.fromPortName) ?? false;
    }
    before ??= reachedBefore(graph, boxes, boxId);
    if (!before.has(wire.fromId)) return false;
    const from = boxes.get(wire.fromId);
    const exit = from?.exits.find((one) => one.name === wire.fromExitName);
    return exit?.outputs.some((one) => one.name === wire.fromPortName) ?? false;
  });
}

/**
 * **What a run can come to before it first comes to `boxId`** — from the entry along the edges that go
 * on to a box, never passing through that one (`amenbo_core::ops::automation_run::reachable_without`).
 * The box itself is not among them.
 */
function reachedBefore(graph: PicGraph, boxes: Map<number, PicBox>, boxId: number): Set<number> {
  const seen = new Set<number>();
  if (graph.entryId === undefined || !boxes.has(graph.entryId)) return seen;
  const queue = [graph.entryId];
  while (queue.length > 0) {
    const id = queue.shift()!;
    if (id === boxId || seen.has(id)) continue;
    seen.add(id);
    for (const edge of graph.edges) {
      if (edge.fromId !== id || edge.ends !== "go" || edge.toId === undefined) continue;
      if (boxes.has(edge.toId)) queue.push(edge.toId);
    }
  }
  return seen;
}

/** The built-in that files a task, and the inputs it reads from the start dialog, by the store's word. */
const MAKE_TASK = "make_task";
const READ_AT_LAUNCH: readonly string[] = ["タイトル", "本文", "選んだ分類"];

/**
 * **An input the entry is handed as a run starts** — the built-in that files a task, placed where the
 * automation starts, takes its title, notes and chosen classification from the start dialog rather than
 * a wire (`amenbo_core::ops::automation_builtin_make::read_at_launch`), so nothing has to reach it.
 * Only on the automation's own picture: inside an action there is no start dialog to hand anything.
 */
export function readAtLaunch(
  graph: PicGraph,
  builtin: string | undefined,
  boxId: number,
  port: string,
): boolean {
  return (
    graph.boundary === undefined && boxId === graph.entryId && builtin === MAKE_TASK && READ_AT_LAUNCH.includes(port)
  );
}

/** Where one row's boxes start, so that every row is centred on the same column of the picture. */
function rowStart(contentW: number, rowW: number): number {
  return Math.round((contentW - rowW) / 2);
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

/** Where an edge is tied to a box: the nth way out, along its bottom from the left. */
function attach(node: PicNode, nth: number): number {
  return node.x + Math.min(ATTACH + nth * EXIT_GAP, node.w - ATTACH);
}

/** Where the nth line in from the margin lands along a box's top, from the left. */
function landAt(node: PicNode, nth: number): number {
  return node.x + Math.min(LAND + nth * LAND_GAP, node.w - LAND);
}

/** What one line is called, which is also what tells two of them apart. */
function lineKey(id: number): string {
  return `edge-${id}`;
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
  const empty: Picture = { width: 0, height: 0, laps: [], nodes: [], lines: [], inserts: [], opens: [], marks: [] };
  if (graph === null || graph.boxes.length === 0) return empty;

  const boxes = new Map(graph.boxes.map((box) => [box.id, box]));
  const { laps, live, back: goesBack } = walk(graph);
  // The action's ways out, the error one last — the order the declaration lists them in.
  const outs = [...(graph.boundary?.exits ?? [])].sort(
    (a, b) => Number(a.name === ERROR_EXIT) - Number(b.name === ERROR_EXIT),
  );
  // How many of each box's ways out say nothing yet (`AMB-D-1003`) — each hangs a row lower than the
  // one to its right, and under the row they hang from they need the room.
  const undecidedOf = (box: PicBox): readonly AutomationExitDto[] =>
    box.exits.filter(
      (exit) =>
        exit.name !== ERROR_EXIT &&
        exit.name !== box.neverLeavesBy &&
        !graph.edges.some((edge) => edge.fromId === box.id && edge.exitName === exit.name),
    );
  // How far under a box what hangs from its ways out with nothing decided reaches: its lowest press,
  // or the name of a line of its that goes nowhere, which hangs under every press (below). Nothing
  // where every way out says something — and a row of those keeps the room it always had.
  const hangOf = (box: PicBox): number => {
    const undecided = undecidedOf(box).length;
    if (undecided === 0) return 0;
    const nowhere = graph.edges.filter(
      (edge) =>
        edge.fromId === box.id &&
        (edge.ends === "done" ||
          edge.ends === "halt" ||
          (edge.ends === "exit" && !outs.some((out) => out.name === edge.exitTo))),
    ).length;
    const press = STUB + (undecided - 1) * OPEN_H + OVER - 4 + PRESS_H / 2;
    return nowhere === 0 ? press : Math.max(press, STUB + undecided * OPEN_H + (nowhere - 1) * WORD_H + OVER + 4);
  };
  // How much room a box leaves on its right before the next box in its row. A press beside a long
  // name runs past the box's right edge, and under the next box it lay over the lines that one sends
  // down: the next box stands as much further right, so the press ends short of its first line
  // (`AMB-T-5790`). Where the lines of the box stand along its bottom is not known yet, so each press
  // is put after every line the box has — never further left than it will be drawn.
  const pressW = wordW(openWord(graph.boundary !== undefined)) + 28;
  const gapAfter = (box: PicBox): number => {
    const lines = graph.edges.filter((edge) => edge.fromId === box.id).length;
    const reach = Math.max(
      0,
      ...undecidedOf(box).map(
        (exit, nth) =>
          Math.min(ATTACH + (lines + nth) * EXIT_GAP, NODE_W - ATTACH) -
          6 +
          wordW(exitWord({ exitName: exit.name, builtin: box.builtin })) +
          BESIDE / 2 +
          pressW,
      ),
    );
    return Math.max(COL_GAP, reach + BESIDE - NODE_W - ATTACH);
  };
  /** Where each box of a row stands from the row's left edge, and how wide the row is. */
  const columnsOf = (row: readonly number[]): { xs: number[]; w: number } => {
    const xs: number[] = [];
    let x = 0;
    row.forEach((_, column) => {
      if (column > 0) x += NODE_W + gapAfter(boxes.get(row[column - 1])!);
      xs.push(x);
    });
    return { xs, w: row.length === 0 ? 0 : x + NODE_W };
  };
  const contentW = Math.max(
    NODE_W,
    IN_W,
    outs.length * OUT_W + Math.max(0, outs.length - 1) * OUT_GAP + FRAME_PAD * 2,
    ...laps.flatMap((lap) => lap.rows.map((row) => columnsOf(row).w)),
  );

  // Which row of which stretch each box landed in — what says whether two boxes are neighbours.
  const at = new Map<number, { lap: number; row: number }>();
  const nodes: PicNode[] = [];
  const outlines: PicLap[] = [];
  // How far under each box's row the lowest thing hanging from that row reaches — what the line down
  // to the next row turns under.
  const hangUnder = new Map<number, number>();
  // The mark a placement comes in by stands over everything, with a row's room under it.
  const over = graph.boundary === undefined ? 0 : IN_H + ROW_GAP;
  let y = PAD + over;
  laps.forEach((lap, nth) => {
    const pad = lap.head === null ? 0 : LAP_PAD;
    // Over the first row the outline stands higher, for the word over the box that takes the task.
    const over = lap.head === null ? 0 : LAP_TOP;
    const top = y;
    let rowY = top + over;
    let hang = 0;
    lap.rows.forEach((row, depth) => {
      // Under a row a press hangs from, the next row stands as much lower as it hangs, so neither that
      // row nor the lines down to it run through the press (`AMB-T-5789`). Every other row keeps its room.
      if (depth > 0) rowY += NODE_H + (hang === 0 ? ROW_GAP : Math.max(ROW_GAP, hang + LEG_CLEAR + ROW_GAP / 2));
      hang = Math.max(0, ...row.map((boxId) => hangOf(boxes.get(boxId)!)));
      const columns = columnsOf(row);
      const startX = rowStart(contentW, columns.w);
      row.forEach((boxId, column) => {
        const box = boxes.get(boxId)!;
        at.set(boxId, { lap: nth, row: depth });
        hangUnder.set(boxId, hang);
        nodes.push({
          boxId,
          // A built-in's words are drawn in the screen's language; the box's own name stays the store's.
          name: builtinWord(box.builtin, box.name),
          x: startX + columns.xs[column],
          y: rowY,
          w: NODE_W,
          h: NODE_H,
          no: nodes.length + 1,
          takes: takesTask(box),
          global: box.global,
          builtin: box.builtin,
          empty:
            box.draft !== true && box.builtin === undefined && box.steps !== undefined && box.steps.length === 0,
          draft: box.draft === true,
          unfed: !live.has(boxId)
            ? []
            : box.inputs
                .filter((port) => port.required && !fed(graph, boxId, port.name))
                .map((port) => builtinWord(box.builtin, port.name)),
        });
      });
    });
    // Under the last row, room for what hangs lowest from it.
    const endRoom = hang === 0 ? END_ROOM : Math.max(END_ROOM, hang + 12);
    const inner = rowY + NODE_H - (top + over) + endRoom;
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
  type Side = "left" | "right";
  /** A line between two boxes that are not neighbours, out to a lane in one of the margins. */
  type Aside = {
    key: string;
    edgeId: number;
    side: Side;
    back: boolean;
    leaves: boolean;
    exitName?: string;
    builtin?: string;
    from: PicNode;
    to: PicNode;
    /** The rows it runs past. */
    top: number;
    bottom: number;
    /** About how wide the name written over its leg is — 0 where it has none. */
    word: number;
  };
  const lines: PicLine[] = [];
  const inserts: PicInsert[] = [];
  const asides: Aside[] = [];
  const edges = graph.edges;

  const toOf = (edge: AutomationEdgeDto): number | undefined => {
    const toId = edge.ends === "go" ? edge.toId : returnsTo(edge);
    return toId !== undefined && node.has(toId) ? toId : undefined;
  };

  // `sideOf`: which margin each line between boxes that are not neighbours goes out to (`AMB-T-5767`).
  // With every one of them in the left margin, a picture of any size stood a row of same-coloured
  // lines side by side there, and a box on the right of its row sent its line under the box beside
  // it. Each line goes to the side where it crosses fewer: a line in a margin crosses another there
  // when their rows overlap without one lying inside the other, and one leaving a box — or landing on
  // one — that is not at that end of its row passes the boxes on that side. A line into a box that
  // already has one coming in on a side joins it there, so it goes that way unless it crosses more.
  // The longest are placed first, as they cross the most; a tie goes left.
  const sideOf = new Map<number, Side>();
  {
    const atEnd = (one: PicNode, side: Side): boolean => {
      const row = [...nodes, ...spots].filter((other) => other.y === one.y).map((other) => other.x);
      return one.x === (side === "left" ? Math.min(...row) : Math.max(...row));
    };
    const spans = edges.flatMap((edge) => {
      const toId = toOf(edge);
      const from = node.get(edge.fromId);
      if (toId === undefined || from === undefined || neighbours(edge.fromId, toId)) return [];
      const to = node.get(toId)!;
      const out = from.y + NODE_H + DROP;
      const into = to.y - DROP;
      return [{ id: edge.id, from, to, top: Math.min(out, into), bottom: Math.max(out, into) }];
    });
    const placed: { toId: number; side: Side; top: number; bottom: number }[] = [];
    for (const span of spans.sort((a, b) => b.bottom - b.top - (a.bottom - a.top) || a.id - b.id)) {
      const cost = (side: Side): number => {
        let crossed = 0;
        for (const one of placed) {
          if (one.side !== side || one.toId === span.to.boxId) continue;
          const overlap = one.top < span.bottom && span.top < one.bottom;
          const nested =
            (one.top <= span.top && span.bottom <= one.bottom) || (span.top <= one.top && one.bottom <= span.bottom);
          if (overlap && !nested) crossed++;
        }
        crossed += Number(!atEnd(span.from, side)) + Number(!atEnd(span.to, side));
        const joins = placed.some((one) => one.side === side && one.toId === span.to.boxId);
        return crossed - Number(joins);
      };
      const side: Side = cost("right") < cost("left") ? "right" : "left";
      sideOf.set(span.id, side);
      placed.push({ toId: span.to.boxId, side, top: span.top, bottom: span.bottom });
    }
  }
  // Whether a line comes into each box from the left margin. It lands first along the box's top, so
  // a line from the row above lands after it rather than on its last leg; one from the right margin
  // lands at the far end.
  const landing = new Map<number, number>();
  for (const edge of edges) {
    const toId = toOf(edge);
    if (toId !== undefined && sideOf.get(edge.id) === "left") landing.set(toId, 1);
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
      points: [{ x: sx, y: sy }, { x: sx, y: mid }, { x: tx, y: mid }, { x: tx, y: entered.y }],
      back: false,
      leaves: true,
      at: { x: sx, y: sy },
      align: "start",
    });
  }

  // Where each edge is tied to its box. Only a way out with a line is counted — the two every box is
  // born with are there on every box, and counting them would push every other line along. The
  // lines that leave for the left margin come first, since their first leg turns left, and those for
  // the right one last; those that go nowhere come before them, so their words have the room to the
  // right of every line down to a box.
  const reach = (edge: AutomationEdgeDto): number => {
    const toId = toOf(edge);
    if (toId === undefined) return 2;
    if (neighbours(edge.fromId, toId)) return 1;
    return sideOf.get(edge.id) === "right" ? 3 : 0;
  };
  // The ways out nothing has been decided for (`AMB-D-1003`): no line leaves by them yet. The error
  // one is left out — with no line it stops the run and calls a person — and so is the one a
  // built-in never leaves by, as its settings stand.
  const opens = graph.boxes.flatMap((box) =>
    !node.has(box.id)
      ? []
      : undecidedOf(box).map((exit) => ({ key: `open-${box.id}-${exit.name}`, boxId: box.id, exitName: exit.name })),
  );
  // `below` is how far under the shortest the foot of a line that goes nowhere hangs, in pixels.
  const slot = new Map<number, { nth: number; below: number }>();
  const openSlot = new Map<string, { nth: number; below: number }>();
  /** How many of its ways out each box has a line from — the places along its bottom. */
  const slots = new Map<number, number>();
  // The first of each box's lines that go down to a neighbour — the one with nothing of its box's
  // coming down on its left, where a name beside it can be written.
  const firstDown = new Map<number, number>();
  for (const box of graph.boxes) {
    const exitAt = (edge: AutomationEdgeDto) => box.exits.findIndex((exit) => exit.name === edge.exitName);
    const own = edges
      .filter((edge) => edge.fromId === box.id)
      .sort((a, b) => reach(a) - reach(b) || exitAt(a) - exitAt(b));
    // A way out with nothing decided stands after the lines that go nowhere and before the ones for
    // the right margin: it hangs down as they do, and its press runs off to the right of its words.
    const undecided = opens.filter((open) => open.boxId === box.id);
    const before = own.filter((edge) => reach(edge) <= 2).length;
    slots.set(box.id, own.length + undecided.length);
    const nowhere = own.filter((edge) => reach(edge) === 2).length;
    let seen = 0;
    own.forEach((edge, nth) => {
      // How many lines that go nowhere stand to this one's right: its words go that many rows lower,
      // and under every press to its right as well.
      const below = reach(edge) === 2 ? (nowhere - 1 - seen++) * WORD_H + undecided.length * OPEN_H : 0;
      slot.set(edge.id, { nth: nth < before ? nth : nth + undecided.length, below });
      if (reach(edge) === 1 && !firstDown.has(box.id)) firstDown.set(box.id, edge.id);
    });
    undecided.forEach((open, nth) => {
      openSlot.set(open.key, { nth: before + nth, below: (undecided.length - 1 - nth) * OPEN_H });
    });
  }

  // Each way out with nothing decided hangs as a line that goes nowhere does, its name under its foot,
  // and the press beside the name. Drawn after the edges, so they keep their order.
  const pressed: PicOpen[] = [];
  const undecidedLines: PicLine[] = [];
  for (const open of opens) {
    const from = node.get(open.boxId)!;
    const { nth, below } = openSlot.get(open.key)!;
    const sx = attach(from, nth);
    const sy = from.y + NODE_H;
    const foot = sy + STUB + below;
    const word = exitWord({ exitName: open.exitName, builtin: from.builtin });
    undecidedLines.push({
      key: open.key,
      points: [{ x: sx, y: sy }, { x: sx, y: foot }],
      back: false,
      open: true,
      exitName: open.exitName,
      builtin: from.builtin,
      at: { x: sx - 6, y: foot + OVER },
      align: "start",
    });
    pressed.push({ boxId: open.boxId, exitName: open.exitName, x: sx - 6 + wordW(word) + BESIDE / 2, y: foot + OVER - 4 });
  }

  for (const edge of edges) {
    const from = node.get(edge.fromId);
    if (from === undefined) continue;
    const { nth, below } = slot.get(edge.id)!;
    const sx = attach(from, nth);
    const sy = from.y + NODE_H;
    const key = lineKey(edge.id);

    const toId = toOf(edge);
    if (toId === undefined) {
      // The one further left runs further down, so its words pass under the shorter lines to its right.
      const foot = sy + STUB + below;
      inserts.push({ edgeId: edge.id, x: sx, y: sy + STUB_PLUS });
      lines.push({
        key,
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
      // Under a row a press hangs from, it turns past the lowest of them.
      const hang = hangUnder.get(edge.fromId) ?? 0;
      const mid = Math.max(Math.round((sy + ty) / 2), hang === 0 ? 0 : sy + hang + LEG_CLEAR);
      const across = Math.round((sx + tx) / 2);
      inserts.push({ edgeId: edge.id, x: across, y: mid });
      // A leg across shorter than the name has no room to write it over: centred there, it runs over
      // the lines leaving beside it (`AMB-T-5675`). A line straight down has no leg at all. Either
      // way the name goes level with the leg, on the left where the lines that go nowhere do not write
      // theirs — but only for the first of its box's lines down to a neighbour. Any other has one of
      // those turning at the same height on its left, so its name goes past its right end instead.
      const word = wordW(lineWord({ exitName: edge.exitName, builtin: from.builtin }));
      const short = Math.abs(tx - sx) < Math.max(BESIDE * 2, word + BESIDE);
      const left = firstDown.get(edge.fromId) === edge.id && tx <= sx + BESIDE * 2;
      const at = !short
        ? { x: across, y: mid - OVER }
        : left
          ? { x: Math.min(sx, tx) - BESIDE, y: mid + 4 }
          : { x: Math.max(sx, tx) + BESIDE, y: mid + 4 };
      lines.push({
        key,
        points: [{ x: sx, y: sy }, { x: sx, y: mid }, { x: tx, y: mid }, { x: tx, y: ty }],
        back: false,
        leaves: edge.ends === "exit",
        exitName: edge.exitName,
        builtin: from.builtin,
        at,
        align: !short ? "middle" : left ? "end" : "start",
      });
      continue;
    }
    // The walk's own reading, not where the two boxes landed: a span stacked by the row it starts at
    // can put a line going forward above the box it leaves.
    asides.push({
      key,
      edgeId: edge.id,
      side: sideOf.get(edge.id)!,
      back: goesBack.has(edge.id),
      leaves: edge.ends === "exit",
      exitName: edge.exitName,
      builtin: from.builtin,
      from,
      to,
      // The rows it runs past, however far along the stair out of its box it turns.
      top: Math.min(sy + DROP, ty - DROP),
      bottom: Math.max(sy + DROP + STAIRS * STAIR, ty - DROP),
      word: wordW(exitWord({ exitName: edge.exitName, builtin: from.builtin })),
    });
  }

  lines.push(...undecidedLines);

  // The lanes. Every line in one margin into the same box is one line from where it reaches the lane
  // on (`AMB-T-5767`): side by side, they stood as many arrowheads on the box's top. So a lane is
  // handed out per box a margin's lines go into, over every row any of them runs past.
  const intoKey = (line: Aside) => `into-${line.side}-${line.to.boxId}`;
  const laneOf = (side: Side): Map<string, number> => {
    const spans = new Map<string, { key: string; top: number; bottom: number }>();
    for (const line of asides.filter((one) => one.side === side)) {
      const key = intoKey(line);
      const was = spans.get(key);
      spans.set(key, {
        key,
        top: Math.min(was?.top ?? line.top, line.top),
        bottom: Math.max(was?.bottom ?? line.bottom, line.bottom),
      });
    }
    return lanes([...spans.values()]);
  };
  const laneAt = { left: laneOf("left"), right: laneOf("right") };
  const lane = (line: Aside) => laneAt[line.side].get(intoKey(line))!;
  const laneCount = (side: Side) => (laneAt[side].size === 0 ? 0 : Math.max(...laneAt[side].values()) + 1);
  // Where each line stands among the ones that leave its box for the same margin, innermost first.
  const outStair = new Map<string, number>();
  {
    const shared = new Map<string, Aside[]>();
    for (const line of asides) {
      const key = `${line.side}-${line.from.boxId}`;
      shared.set(key, [...(shared.get(key) ?? []), line]);
    }
    for (const group of shared.values()) {
      group.sort((a, b) => lane(a) - lane(b) || a.edgeId - b.edgeId).forEach((line, nth) => outStair.set(line.key, nth));
    }
  }
  /** Where a line leaves its box: the first places along its bottom for the left margin, the innermost
   *  leftmost, and the last for the right one, the innermost rightmost. */
  const outX = (line: Aside): number => {
    const stair = outStair.get(line.key)!;
    return attach(line.from, line.side === "left" ? stair : (slots.get(line.from.boxId) ?? 1) - 1 - stair);
  };
  const outY = (line: Aside): number => line.from.y + NODE_H + DROP + Math.min(outStair.get(line.key)!, STAIRS) * STAIR;

  // `wordsAt`: where the name of each line in a margin is written — over the leg it leaves its box
  // by, just beside that box on the side the line goes, so it reads as that box's way out. Past the
  // outermost lane the names of every row stood in one column, apart from the boxes they belong to
  // (`AMB-T-5768`); halfway along a lane they stood beside some other box (`AMB-T-5592`). Under a
  // row, between its bottom and the legs that turn out of it, only what leaves that row passes —
  // lines come into a box from above, and the lines to the next row turn lower — so the lanes are
  // moved out past the names and nothing crosses them. A box with another box on that side writes
  // its names in the gap between the two where nothing of that box's comes down through them; where
  // something does, they go past the row's end with the rest. Names that share a place stand side by
  // side, the innermost line's nearest its box.
  const wordsAt = new Map<string, PicPoint>();
  const wordsOut = { left: -LAP_PAD, right: contentW + LAP_PAD };
  {
    const rows = new Map<number, Aside[]>();
    for (const line of asides) {
      if (line.word > 0) rows.set(line.from.y, [...(rows.get(line.from.y) ?? []), line]);
    }
    /** How far across a name runs, where it runs over [top, bottom]. */
    const across = (line: PicLine, top: number, bottom: number): [number, number] | undefined => {
      const wide = wordW(edgeWord(line));
      if (wide === 0 || line.at.y + 3 < top || bottom < line.at.y - 11) return undefined;
      const left = line.align === "end" ? line.at.x - wide : line.align === "middle" ? line.at.x - wide / 2 : line.at.x;
      return [left, left + wide];
    };
    for (const [y, row] of rows) {
      const top = y + NODE_H;
      const bottom = top + DROP;
      // What already stands under the row: every line coming down out of it, and every name there.
      const taken: [number, number][] = [];
      for (const line of lines) {
        line.points.forEach((p, nth) => {
          const q = line.points[nth + 1];
          if (q !== undefined && p.x === q.x && Math.min(p.y, q.y) < bottom && top < Math.max(p.y, q.y)) {
            taken.push([p.x - 2, p.x + 2]);
          }
        });
        const words = across(line, top, bottom);
        if (words !== undefined) taken.push(words);
      }
      for (const line of asides) {
        if (line.from.y === y) taken.push([outX(line) - 2, outX(line) + 2]);
      }
      const free = (left: number, right: number) => taken.every(([a, b]) => b < left || right < a);
      const xs = [...nodes, ...spots].filter((one) => one.y === y);
      for (const side of ["left", "right"] as const) {
        // Outward: to the left on the left side, to the right on the right one.
        const sign = side === "left" ? -1 : 1;
        const beside = (box: PicNode) => (side === "left" ? box.x : box.x + box.w) + (sign * BESIDE) / 2;
        const end = side === "left" ? Math.min(...xs.map((one) => one.x)) : Math.max(...xs.map((one) => one.x + one.w));
        /** Write the names out from `x`, the nearest first; where the last one ends. */
        const put = (names: Aside[], x: number): number => {
          for (const line of names) {
            wordsAt.set(line.key, { x, y: bottom - 3 });
            taken.push(side === "left" ? [x - line.word, x] : [x, x + line.word]);
            x += sign * (line.word + BESIDE);
          }
          return x - sign * BESIDE;
        };
        const byBox = new Map<number, Aside[]>();
        for (const line of row
          .filter((one) => one.side === side)
          .sort((a, b) => outStair.get(a.key)! - outStair.get(b.key)!)) {
          byBox.set(line.from.boxId, [...(byBox.get(line.from.boxId) ?? []), line]);
        }
        const rest: Aside[] = [];
        for (const own of [...byBox.values()].sort((a, b) => sign * (b[0]!.from.x - a[0]!.from.x))) {
          const box = own[0]!.from;
          const wide = own.reduce((sum, line) => sum + line.word, 0) + (own.length - 1) * BESIDE;
          const near = beside(box);
          const far = near + sign * wide;
          const inside = side === "left" ? box.x > end : box.x + box.w < end;
          if (inside && free(Math.min(near, far), Math.max(near, far))) put(own, near);
          else rest.push(...own);
        }
        if (rest.length === 0) continue;
        const reached = put(rest, end + (sign * BESIDE) / 2) + (sign * LAP_PAD) / 2;
        wordsOut[side] = side === "left" ? Math.min(wordsOut.left, reached) : Math.max(wordsOut.right, reached);
      }
    }
  }
  // Every outline reaches out as far as the furthest name on either side, so none of them crosses it
  // and their edges stay in one line; the lanes start past it.
  // A press at the end of a way out that says nothing yet runs off to the right of its name, and the
  // outline reaches past it too.
  wordsOut.right = Math.max(wordsOut.right, ...pressed.map((one) => one.x + pressW + LAP_PAD / 2));
  for (const lap of outlines) {
    lap.w = wordsOut.right - wordsOut.left;
    lap.x = wordsOut.left;
  }
  const laneX = (line: Aside) =>
    line.side === "left" ? wordsOut.left - (lane(line) + 1) * LANE_W : wordsOut.right + (lane(line) + 1) * LANE_W;
  /** Where the lines of a margin land on a box's top: left of its first way out, or right of its last. */
  const inX = (line: Aside) => (line.side === "left" ? landAt(line.to, 0) : line.to.x + line.to.w - LAND);

  // The colour each edge is drawn in (`PicLine.tone`). A box with two or more named ways out is one
  // the run branches at, and every line out of it says so; the error way out, and a line that stops
  // the run, are the stop colour.
  const named = (box: PicBox | undefined) =>
    box?.exits.filter((exit) => exit.name !== undefined && exit.name !== ERROR_EXIT).length ?? 0;
  const tones = new Map<string, PicLine["tone"]>(
    edges.map((edge) => [
      lineKey(edge.id),
      edge.exitName === ERROR_EXIT || edge.ends === "halt"
        ? "error"
        : named(boxes.get(edge.fromId)) > 1
          ? "branch"
          : "next",
    ]),
  );
  // The lines themselves. One alone into its box runs all the way; where several go into one box from
  // one margin, each runs only as far along the lane as the next one's joining, so no two lie over
  // each other, and one more line takes the lane on from there into the box.
  const groups = new Map<string, Aside[]>();
  for (const line of asides) groups.set(intoKey(line), [...(groups.get(intoKey(line)) ?? []), line]);
  for (const [key, group] of groups) {
    const x = laneX(group[0]!);
    const to = group[0]!.to;
    const inY = to.y - DROP;
    const joined = group.length > 1;
    // Toward where the lane turns into the box, from above and from below: each one stops where the
    // next nearer one joins.
    const stops = new Map<string, number>();
    for (const below of [false, true]) {
      const run = group
        .filter((line) => outY(line) > inY === below)
        .sort((a, b) => Math.abs(outY(b) - inY) - Math.abs(outY(a) - inY) || a.edgeId - b.edgeId);
      run.forEach((line, nth) => stops.set(line.key, nth + 1 < run.length ? outY(run[nth + 1]!) : inY));
    }
    for (const line of group) {
      const sx = outX(line);
      const sy = line.from.y + NODE_H;
      const oy = outY(line);
      const stop = stops.get(line.key)!;
      const stair = outStair.get(line.key)!;
      // On the lane just past the turn it takes out of its box, beside the name written there:
      // halfway along a long lane the `+` stood beside some other box, apart from its name
      // (`AMB-T-5596`). Two lines leaving one box turn a few points apart on lanes closer than a
      // `+` is wide, so each one further out sits a `+` further along its lane. A lane too short
      // for that keeps it halfway, a `+` lower for each lane further out. One that joins another
      // line sooner than a `+` is tall has no lane of its own to hold it, so it sits on the leg
      // it comes out to the lane by, a `+` short of the lane.
      const along = Math.sign(stop - oy);
      const near = sy + DROP + along * (LANE_PLUS / 2 + stair * LANE_PLUS);
      const room = along * (stop - near) >= LANE_PLUS / 2 && along * (near - oy) > 0;
      const inward = line.side === "left" ? 1 : -1;
      const plus = room
        ? { x, y: near }
        : !joined
          ? { x, y: Math.min(Math.round((line.top + line.bottom) / 2) + lane(line) * LANE_PLUS, line.bottom - LANE_PLUS / 2) }
          : Math.abs(stop - oy) >= LANE_PLUS
            ? { x, y: Math.round((oy + stop) / 2) }
            : { x: x + (inward * LANE_PLUS) / 2, y: oy };
      inserts.push({ edgeId: line.edgeId, ...plus });
      const tail = joined
        ? [{ x, y: stop }]
        : [{ x, y: inY }, { x: inX(line), y: inY }, { x: inX(line), y: to.y }];
      lines.push({
        key: line.key,
        points: [{ x: sx, y: sy }, { x: sx, y: oy }, { x, y: oy }, ...tail],
        back: line.back,
        leaves: line.leaves,
        joins: joined || undefined,
        exitName: line.exitName,
        builtin: line.builtin,
        // Over the leg it leaves its box by, just beside that box (`wordsAt`).
        at: wordsAt.get(line.key) ?? { x: wordsOut[line.side], y: sy + DROP - 3 },
        align: line.side === "left" ? "end" : "start",
      });
    }
    if (!joined) continue;
    const toneOf = new Set(group.map((line) => tones.get(line.key)));
    lines.push({
      key,
      points: [{ x, y: inY }, { x: inX(group[0]!), y: inY }, { x: inX(group[0]!), y: to.y }],
      back: group.every((line) => line.back),
      leaves: group.every((line) => line.leaves) || undefined,
      tone: toneOf.size === 1 ? [...toneOf][0] : undefined,
      at: { x, y: inY },
      align: "start",
    });
  }

  // Everything was laid out with the boxes at x=0. Shift it right by the room the left margin took:
  // the lanes, and any name written further out than the boxes start.
  const leftRoom = Math.max(
    -wordsOut.left + laneCount("left") * LANE_W,
    ...lines.map((line) => {
      const wide = wordW(edgeWord(line));
      return -(line.align === "end" ? line.at.x - wide : line.align === "middle" ? line.at.x - wide / 2 : line.at.x);
    }),
  );
  const dx = PAD + leftRoom;
  // And the room the right margin takes: its lanes, and the words of an edge that run on past the
  // boxes — the way out and where the run goes, of the last box.
  const rightRoom = Math.max(
    wordsOut.right - contentW + laneCount("right") * LANE_W,
    ...lines.map((line) => {
      const wide = wordW(edgeWord(line));
      return (line.align === "start" ? line.at.x + wide : line.align === "middle" ? line.at.x + wide / 2 : line.at.x) - contentW;
    }),
    ...pressed.map((one) => one.x + pressW - contentW),
  );
  return {
    width: dx + contentW + rightRoom + PAD,
    height,
    laps: outlines.map((lap) => ({ ...lap, x: lap.x + dx })),
    nodes: nodes.map((one) => ({ ...one, x: one.x + dx })),
    lines: lines.map((line) => ({
      ...line,
      tone: tones.get(line.key) ?? line.tone,
      points: line.points.map((p) => ({ x: p.x + dx, y: p.y })),
      at: { x: line.at.x + dx, y: line.at.y },
    })),
    inserts: inserts.map((one) => ({ ...one, x: one.x + dx })),
    opens: pressed.map((one) => ({ ...one, x: one.x + dx })),
    marks: marks.map((one) => ({ ...one, x: one.x + dx })),
    outFrame: outFrame === undefined ? undefined : { ...outFrame, x: outFrame.x + dx },
  };
}
