// What the picture of one automation comes out as, checked without a screen (`AMB-T-5255`).
//
// What these guard is the reading, not the pixels: **a task's span is one outline** and the spot
// that takes the next task starts the next one; **depth runs down and the same depth runs across**;
// **what comes after a spot is tied down the left, and what it hands on is not drawn**; **a line
// whose two boxes are not neighbours leaves for a lane**, dashed where it goes back up; **the error
// way out is drawn on every box, as the stop it is where nobody said what follows it**; and **a spot nothing reaches is still
// drawn**, which is the state every half-built automation is in.
import { describe, expect, it } from "vitest";
import { automationGraph, edgeWord, layOut, type PicGraph } from "./automationLayout";
import type {
  AutomationDetailDto,
  AutomationEdgeDto,
  AutomationPortDto,
  AutomationPlacementDto,
  AutomationWireDto,
} from "../bindings/bindings";

let nextId = 1;

function step(
  over: Partial<AutomationPlacementDto> & { id: number; name: string },
): AutomationPlacementDto {
  return {
    actionId: 900 + over.id, global: false,
    prompt: "",
    interactive: false,
    reportToTask: false,
    showHistory: true,
    showNotes: true,
    showDecisions: true,
    showComments: true,
    // What core writes at birth: the unnamed way out, and the error one nobody can delete.
    exits: [
      { id: nextId++, name: "完了", outputs: [] },
      { id: nextId++, name: "*", outputs: [] },
    ],
    inputs: [],
    settings: [],
    steps: [],
    ...over,
  };
}

/**
 * A spot that takes the next task — the one that begins a stretch. What makes it one is a
 * `task_take` **output** on a way out, which is where core looks for it.
 */
function taker(
  id: number,
  name: string,
  over: Partial<AutomationPlacementDto> = {},
): AutomationPlacementDto {
  const one = step({ id, name, ...over });
  return {
    ...one,
    exits: one.exits.map((exit, nth) =>
      nth === 0 ? { ...exit, outputs: [...exit.outputs, port("task", "task_take")] } : exit,
    ),
  };
}

function port(name: string, kind: AutomationPortDto["kind"], required = true): AutomationPortDto {
  return { name, kind, required };
}

function edge(over: Partial<AutomationEdgeDto> & { id: number; fromId: number }): AutomationEdgeDto {
  return { ends: "go", exitName: "完了", ...over };
}

function wire(over: AutomationWireDto): AutomationWireDto {
  return over;
}

/** One automation, as the picture reads it — the definition the screen fetches, laid out. */
function detail(over: Partial<AutomationDetailDto> = {}): PicGraph {
  return automationGraph({
    id: 7,
    projectId: 1,
    name: "Morning round",
    notes: "",
    archived: false,
    placements: [],
    edges: [],
    wires: [],
    heldBy: [],
    ...over,
  })!;
}

const at = (picture: ReturnType<typeof layOut>, boxId: number) =>
  picture.nodes.find((one) => one.boxId === boxId)!;

describe("the picture of an automation", () => {
  it("has nothing to draw for nothing, and for an automation with no steps", () => {
    expect(layOut(null).nodes).toEqual([]);
    expect(layOut(detail()).nodes).toEqual([]);
    expect(layOut(detail()).width).toBe(0);
  });

  it("puts each depth on its own row, in the order the walk reaches them", () => {
    const one = detail({
      entryPlacementId: 1,
      placements: [taker(1, "take"), step({ id: 2, name: "work" }), step({ id: 3, name: "check" })],
      edges: [edge({ id: 1, fromId: 1, toId: 2 }), edge({ id: 2, fromId: 2, toId: 3 })],
    });
    const picture = layOut(one);
    expect(at(picture, 1).y).toBeLessThan(at(picture, 2).y);
    expect(at(picture, 2).y).toBeLessThan(at(picture, 3).y);
    expect(at(picture, 1).x).toBe(at(picture, 2).x);
  });

  it("puts what is at the same depth side by side", () => {
    const one = detail({
      entryPlacementId: 1,
      placements: [
        // Both ways out hand the task on, so both of what follows are that task's.
        taker(1, "take", {
          exits: [
            { id: 91, name: "yes", outputs: [] },
            { id: 92, name: "no", outputs: [port("task", "task_take")] },
          ],
        }),
        step({ id: 2, name: "left" }),
        step({ id: 3, name: "right" }),
      ],
      edges: [
        edge({ id: 1, fromId: 1, exitName: "yes", toId: 2 }),
        edge({ id: 2, fromId: 1, exitName: "no", toId: 3 }),
      ],
    });
    const picture = layOut(one);
    expect(at(picture, 2).y).toBe(at(picture, 3).y);
    expect(at(picture, 2).x).not.toBe(at(picture, 3).x);
    // Each name is written over the middle of its own line's leg across, not stacked under the step.
    for (const key of ["edge-1", "edge-2"]) {
      const line = picture.lines.find((one) => one.key === key)!;
      const [, turn, across] = line.points;
      expect(line.align).toBe("middle");
      expect(line.at.x).toBe(Math.round((turn!.x + across!.x) / 2));
      expect(line.at.y).toBeLessThan(turn!.y);
    }
  });

  it("outlines the steps one task is worked by, and starts a new outline at the next taker", () => {
    const one = detail({
      entryPlacementId: 1,
      placements: [taker(1, "first"), step({ id: 2, name: "work" }), taker(3, "second")],
      edges: [edge({ id: 1, fromId: 1, toId: 2 }), edge({ id: 2, fromId: 2, toId: 3 })],
    });
    const picture = layOut(one);
    expect(picture.laps.map((lap) => lap.headBoxId)).toEqual([1, 3]);
    // The second taker heads its own stretch, so it is back at the top of one rather than three deep.
    expect(picture.laps[0]!.y + picture.laps[0]!.h).toBeLessThanOrEqual(picture.laps[1]!.y);
    expect(at(picture, 3).y).toBeGreaterThan(at(picture, 2).y);
  });

  it("puts a step one row under the lowest step that leads to it, not the nearest", () => {
    const one = detail({
      entryPlacementId: 1,
      placements: [
        taker(1, "take"),
        step({ id: 2, name: "work" }),
        step({ id: 3, name: "check" }),
        step({ id: 4, name: "report" }),
      ],
      edges: [
        edge({ id: 1, fromId: 1, toId: 2 }),
        edge({ id: 2, fromId: 2, toId: 3 }),
        edge({ id: 3, fromId: 3, toId: 4 }),
        // A short way to the same step: the long one decides its row.
        edge({ id: 4, fromId: 2, exitName: "*", toId: 4 }),
      ],
    });
    const picture = layOut(one);
    expect(at(picture, 4).y).toBeGreaterThan(at(picture, 3).y);
  });

  it("does not let a line that goes back push the step it goes to further down", () => {
    const one = detail({
      entryPlacementId: 1,
      placements: [taker(1, "take"), step({ id: 2, name: "work" }), step({ id: 3, name: "check" })],
      edges: [
        edge({ id: 1, fromId: 1, toId: 2 }),
        edge({ id: 2, fromId: 2, toId: 3 }),
        edge({ id: 3, fromId: 3, exitName: "*", toId: 2 }),
      ],
    });
    const picture = layOut(one);
    expect(at(picture, 2).y).toBeLessThan(at(picture, 3).y);
    expect(picture.lines.find((line) => line.key === "edge-3")!.back).toBe(true);
  });

  it("leaves out of a task's outline what follows the way out that hands no task on", () => {
    const one = detail({
      entryPlacementId: 1,
      placements: [
        taker(1, "take", {
          exits: [
            { id: 91, name: "found", outputs: [] },
            { id: 92, name: "none left", outputs: [] },
          ],
        }),
        step({ id: 2, name: "work" }),
        step({ id: 3, name: "tidy up" }),
      ],
      edges: [
        edge({ id: 1, fromId: 1, exitName: "found", toId: 2 }),
        edge({ id: 2, fromId: 1, exitName: "none left", toId: 3 }),
      ],
    });
    const picture = layOut(one);
    const lap = picture.laps[0]!;
    const inside = (boxId: number) => at(picture, boxId).y + at(picture, boxId).h <= lap.y + lap.h;
    expect(picture.laps.map((one) => one.headBoxId)).toEqual([1]);
    expect(inside(2)).toBe(true);
    expect(inside(3)).toBe(false);
  });

  it("takes no task at a box whose settings keep it off the way out that hands one on (AMB-T-5669)", () => {
    // "File a task", answered to leave the task it files untaken: core says it never leaves by the
    // way out that takes it, so nothing is handed on there however that way out is drawn.
    const filer = (never: string) => step({
      id: 1,
      name: "file one",
      builtin: "make_task",
      neverLeavesBy: never,
      exits: [
        { id: nextId++, name: "made", outputs: [port("task", "task_make")] },
        { id: nextId++, name: "made and taken", outputs: [port("task", "task_take")] },
        { id: nextId++, name: "*", outputs: [] },
      ],
    });
    const untaken = layOut(detail({ entryPlacementId: 1, placements: [filer("made and taken")] }));
    expect(untaken.laps, "a stretch was drawn for a task nobody takes").toEqual([]);
    expect(at(untaken, 1).takes, "the box was marked as taking the next task").not.toBe(true);

    const taken = layOut(detail({ entryPlacementId: 1, placements: [filer("made")] }));
    expect(taken.laps.map((lap) => lap.headBoxId)).toEqual([1]);
    expect(at(taken, 1).takes).toBe(true);
  });

  it("draws no outline around steps that answer to no task, and still places what nothing reaches", () => {
    const one = detail({
      entryPlacementId: 1,
      placements: [step({ id: 1, name: "entry that takes nothing" }), step({ id: 2, name: "stranded" })],
    });
    const picture = layOut(one);
    expect(picture.laps).toEqual([]);
    expect(picture.nodes.map((node) => node.boxId).sort()).toEqual([1, 2]);
  });

  it("ties what comes after a step down its left, and draws no line for what it hands on (AMB-D-1001)", () => {
    const one = detail({
      entryPlacementId: 1,
      placements: [
        taker(1, "take", {
          exits: [{ id: 91, name: "完了", outputs: [port("note", "value")] }, { id: 92, name: "*", outputs: [] }],
        }),
        step({ id: 2, name: "work", inputs: [port("note", "value")] }),
      ],
      edges: [edge({ id: 1, fromId: 1, toId: 2 })],
      wires: [wire({ id: 1, fromId: 1, fromExitName: "完了", fromPortName: "note", toId: 2, toPortName: "note" })],
    });
    const picture = layOut(one);
    const middle = at(picture, 1).x + at(picture, 1).w / 2;
    // The second box's way out says nothing yet, and hangs as its own dashed line (`AMB-D-1003`).
    expect(picture.lines.map((one) => one.key)).toEqual(["edge-1", "open-2-完了"]);
    expect(picture.lines[0]!.points[0]!.x).toBeLessThan(middle);
  });

  it("sends a line back to a shallower row out to a lane, dashed", () => {
    const one = detail({
      entryPlacementId: 1,
      placements: [
        taker(1, "take"),
        step({ id: 2, name: "work" }),
        step({ id: 3, name: "check", exits: [{ id: 93, name: "完了", outputs: [] }, { id: 94, name: "again", outputs: [] }] }),
      ],
      edges: [
        edge({ id: 1, fromId: 1, toId: 2 }),
        edge({ id: 2, fromId: 2, toId: 3 }),
        edge({ id: 3, fromId: 3, exitName: "again", toId: 2 }),
      ],
    });
    const picture = layOut(one);
    const back = picture.lines.find((line) => line.key === "edge-3")!;
    expect(back.back).toBe(true);
    const leftmost = Math.min(...picture.nodes.map((node) => node.x));
    expect(Math.min(...back.points.map((p) => p.x))).toBeLessThan(leftmost);
  });

  it("draws the step that goes on to the next task straight down, outline or no outline", () => {
    const one = detail({
      entryPlacementId: 1,
      placements: [taker(1, "first"), step({ id: 2, name: "work" }), taker(3, "second")],
      edges: [edge({ id: 1, fromId: 1, toId: 2 }), edge({ id: 2, fromId: 2, toId: 3 })],
    });
    const picture = layOut(one);
    // Four points is the shape of a line drawn between its own two boxes; an aside one has six.
    expect(picture.lines.find((line) => line.key === "edge-2")!.points).toHaveLength(4);
  });

  it("sends a line that skips a stretch out to a lane, and leaves it solid", () => {
    const one = detail({
      entryPlacementId: 1,
      placements: [
        taker(1, "first"),
        step({ id: 2, name: "work" }),
        taker(3, "second"),
        step({ id: 4, name: "check" }),
        taker(5, "third"),
      ],
      edges: [
        edge({ id: 1, fromId: 1, toId: 2 }),
        edge({ id: 2, fromId: 2, toId: 3 }),
        edge({ id: 3, fromId: 3, toId: 4 }),
        edge({ id: 4, fromId: 4, toId: 5 }),
        // Past the whole of the second stretch: forward, and no neighbour.
        edge({ id: 5, fromId: 2, exitName: "*", toId: 5 }),
      ],
    });
    const picture = layOut(one);
    const across = picture.lines.find((line) => line.key === "edge-5")!;
    expect(across.back).toBe(false);
    const leftmost = Math.min(...picture.nodes.map((node) => node.x));
    expect(Math.min(...across.points.map((p) => p.x))).toBeLessThan(leftmost);
  });

  it("joins two lines from one margin into the same box into one line, with one arrowhead", () => {
    const one = detail({
      entryPlacementId: 1,
      placements: [
        taker(1, "take", { exits: [{ id: 91, name: "a", outputs: [] }, { id: 92, name: "b", outputs: [] }] }),
        step({ id: 2, name: "work" }),
        step({ id: 3, name: "check" }),
        step({ id: 4, name: "close" }),
      ],
      edges: [
        edge({ id: 1, fromId: 1, exitName: "a", toId: 2 }),
        edge({ id: 2, fromId: 2, toId: 3 }),
        edge({ id: 3, fromId: 3, toId: 4 }),
        edge({ id: 4, fromId: 4, toId: 2 }),
        edge({ id: 5, fromId: 3, exitName: "*", toId: 2 }),
      ],
    });
    const picture = layOut(one);
    const line = (key: string) => picture.lines.find((one) => one.key === key)!;
    const laneX = (key: string) => Math.min(...line(key).points.map((p) => p.x));
    const into = picture.lines.find((one) => one.key.startsWith("into-"))!;
    // On one lane, each ending there without an arrowhead, and one line on from there into the box.
    expect(laneX("edge-4")).toBe(laneX("edge-5"));
    expect(line("edge-4").joins).toBe(true);
    expect(line("edge-5").joins).toBe(true);
    expect(into.joins).toBeUndefined();
    expect(into.points[0]!.x).toBe(line("edge-4").points.slice(-1)[0]!.x);
    const work = picture.nodes.find((one) => one.boxId === 2)!;
    expect(into.points.slice(-1)[0]!.y).toBe(work.y);
    // Neither lies over the other along the lane: the one further out stops where the nearer joins.
    const [far, near] = [line("edge-4"), line("edge-5")].sort((a, b) => b.points[1]!.y - a.points[1]!.y);
    expect(far!.points.slice(-1)[0]!.y).toBe(near!.points[2]!.y);
    expect(near!.points.slice(-1)[0]!.y).toBe(into.points[0]!.y);
  });

  it("gives two lines crossing the same rows into different boxes a lane each, or a margin each", () => {
    const one = detail({
      entryPlacementId: 1,
      placements: [
        taker(1, "take"),
        step({ id: 2, name: "work" }),
        step({ id: 3, name: "check", exits: [{ id: 31, name: "again", outputs: [] }] }),
        step({ id: 4, name: "close", exits: [{ id: 41, name: "again", outputs: [] }] }),
      ],
      edges: [
        edge({ id: 1, fromId: 1, toId: 2 }),
        edge({ id: 2, fromId: 2, toId: 3 }),
        edge({ id: 3, fromId: 3, toId: 4 }),
        edge({ id: 4, fromId: 3, exitName: "again", toId: 1 }),
        edge({ id: 5, fromId: 4, exitName: "again", toId: 2 }),
      ],
    });
    const picture = layOut(one);
    const lane = (key: string) => picture.lines.find((line) => line.key === key)!.points[2]!.x;
    expect(lane("edge-4")).not.toBe(lane("edge-5"));
  });

  it("keeps lines that go back apart, the longest outermost and each into its own place", () => {
    const again = (id: number) => [
      { id, name: "完了", outputs: [] },
      { id: id + 1, name: "again", outputs: [] },
      { id: id + 2, name: "*", outputs: [] },
    ];
    // Two lines back into the third step and one back to the top — the shape a development loop takes.
    const one = detail({
      entryPlacementId: 1,
      placements: [
        taker(1, "take"),
        step({ id: 2, name: "plan" }),
        step({ id: 3, name: "build" }),
        step({ id: 4, name: "check", exits: again(100) }),
        step({ id: 5, name: "review", exits: again(110) }),
        step({ id: 6, name: "merge" }),
        step({ id: 7, name: "close", exits: again(120) }),
      ],
      edges: [
        edge({ id: 1, fromId: 1, toId: 2 }),
        edge({ id: 2, fromId: 2, toId: 3 }),
        edge({ id: 3, fromId: 3, toId: 4 }),
        edge({ id: 4, fromId: 4, toId: 5 }),
        edge({ id: 5, fromId: 5, toId: 6 }),
        edge({ id: 6, fromId: 6, toId: 7 }),
        edge({ id: 7, fromId: 4, exitName: "again", toId: 3 }),
        edge({ id: 8, fromId: 5, exitName: "again", toId: 3 }),
        edge({ id: 9, fromId: 7, exitName: "again", toId: 1 }),
      ],
    });
    const picture = layOut(one);
    const line = (key: string) => picture.lines.find((one) => one.key === key)!;
    const lane = (key: string) => line(key).points[2]!.x;
    // The two back into the third step share a lane; the one past every row stands outside it, so
    // neither crosses it.
    expect(lane("edge-7")).toBe(lane("edge-8"));
    expect(lane("edge-9")).toBeLessThan(lane("edge-8"));
    // Into the third step, two lines: the one the two back join into, and the one from the row
    // above. Each lands on a place of its own, and turns down at a height of its own.
    const joined = picture.lines.find((one) => one.key.startsWith("into-") && one.points.slice(-1)[0]!.y === picture.nodes.find((box) => box.boxId === 3)!.y)!;
    const into = [joined, line("edge-2")].map((one) => one.points.slice(-2));
    expect(new Set(into.map(([, foot]) => foot!.x)).size).toBe(2);
    expect(new Set(into.map(([turn]) => turn!.y)).size).toBe(2);
  });

  it("writes the name of a line in the margin over the leg it leaves its box by, beside that box", () => {
    const one = detail({
      entryPlacementId: 1,
      placements: [
        taker(1, "take"),
        step({ id: 2, name: "work" }),
        step({ id: 3, name: "check", exits: [{ id: 93, name: "again", outputs: [] }, { id: 94, name: "*", outputs: [] }] }),
      ],
      edges: [
        edge({ id: 1, fromId: 1, toId: 2 }),
        edge({ id: 2, fromId: 2, toId: 3 }),
        edge({ id: 3, fromId: 3, exitName: "again", toId: 2 }),
        edge({ id: 4, fromId: 3, exitName: "*", toId: 1 }),
      ],
    });
    const picture = layOut(one);
    const line = (key: string) => picture.lines.find((one) => one.key === key)!;
    const laneX = (key: string) => Math.min(...line(key).points.map((p) => p.x));
    const check = picture.nodes.find((one) => one.boxId === 3)!;
    const [inner, outer] = [line("edge-3"), line("edge-4")];
    expect(inner.align).toBe("end");
    // Just left of the box it leaves, and over the leg that runs from there out to its lane.
    expect(inner.at.x).toBeLessThan(check.x);
    expect(check.x - inner.at.x).toBeLessThanOrEqual(14);
    expect(inner.at.y).toBeGreaterThan(check.y + check.h);
    expect(inner.at.y).toBeLessThan(inner.points[1]!.y);
    // Side by side on one row, the inner line's name nearest the box, and neither over the other.
    expect(outer.at.y).toBe(inner.at.y);
    expect(outer.at.x).toBeLessThanOrEqual(inner.at.x - "again".length * 7);
    // Every lane stands past both names, so none of them crosses one.
    // As wide as the picture guesses the name is written: a wide character twelve points, any other seven.
    const left = outer.at.x - [...edgeWord(outer)].reduce((sum, one) => sum + (one.codePointAt(0)! > 0x2e80 ? 12 : 7), 0);
    expect(laneX("edge-3")).toBeLessThan(left);
    expect(laneX("edge-4")).toBeLessThan(left);
    // The outline stands past the names too, and the picture keeps the room for them.
    expect(picture.laps[0]!.x).toBeLessThan(left);
    expect(picture.laps[0]!.x).toBeGreaterThan(laneX("edge-3"));
    expect(Math.min(...picture.laps.map((lap) => lap.x))).toBe(Math.max(...picture.laps.map((lap) => lap.x)));
    expect(laneX("edge-4")).toBeGreaterThanOrEqual(0);
  });

  it("writes the names of lines in the margin by their own rows, not in one column", () => {
    const picture = layOut(
      detail({
        entryPlacementId: 1,
        placements: [
          taker(1, "take"),
          step({ id: 2, name: "a", exits: [{ id: 21, name: "skip", outputs: [] }, { id: 22, name: "完了", outputs: [] }] }),
          step({ id: 3, name: "b" }),
          step({ id: 4, name: "c" }),
          step({ id: 5, name: "d", exits: [{ id: 51, name: "again", outputs: [] }] }),
        ],
        edges: [
          edge({ id: 1, fromId: 1, toId: 2 }),
          edge({ id: 2, fromId: 2, exitName: "完了", toId: 3 }),
          edge({ id: 3, fromId: 3, toId: 4 }),
          edge({ id: 4, fromId: 4, toId: 5 }),
          edge({ id: 5, fromId: 2, exitName: "skip", toId: 4 }),
          edge({ id: 6, fromId: 5, exitName: "again", toId: 2 }),
        ],
      }),
    );
    const line = (key: string) => picture.lines.find((one) => one.key === key)!;
    const box = (id: number) => picture.nodes.find((one) => one.boxId === id)!;
    for (const [key, id] of [["edge-5", 2], ["edge-6", 5]] as const) {
      expect(line(key).at.y).toBeGreaterThan(box(id).y + box(id).h);
      expect(line(key).at.y).toBeLessThan(box(id).y + box(id).h + 20);
      expect(line(key).at.x).toBeLessThan(box(id).x);
      expect(box(id).x - line(key).at.x).toBeLessThanOrEqual(14);
    }
  });

  it("sends a line out of a box on the right of its row to the right margin, its name beside that box", () => {
    const two = [{ id: 21, name: "a", outputs: [] }, { id: 22, name: "b", outputs: [] }];
    const picture = layOut(
      detail({
        entryPlacementId: 1,
        placements: [
          taker(1, "take"),
          step({ id: 2, name: "split", exits: two }),
          step({ id: 3, name: "left", exits: [{ id: 31, name: "again", outputs: [] }] }),
          step({ id: 4, name: "right", exits: [{ id: 41, name: "retry", outputs: [] }] }),
        ],
        edges: [
          edge({ id: 1, fromId: 1, toId: 2 }),
          edge({ id: 2, fromId: 2, exitName: "a", toId: 3 }),
          edge({ id: 3, fromId: 2, exitName: "b", toId: 4 }),
          edge({ id: 4, fromId: 3, exitName: "again", toId: 2 }),
          edge({ id: 5, fromId: 4, exitName: "retry", toId: 1 }),
        ],
      }),
    );
    const line = (key: string) => picture.lines.find((one) => one.key === key)!;
    const [left, right] = [3, 4].map((id) => picture.nodes.find((one) => one.boxId === id)!);
    expect(left!.y).toBe(right!.y);
    // The right box's line goes right, not under the box beside it, and its name stands just right
    // of it.
    expect(line("edge-5").points[2]!.x).toBeGreaterThan(right!.x + right!.w);
    expect(line("edge-5").align).toBe("start");
    expect(line("edge-5").at.x).toBeGreaterThan(right!.x + right!.w);
    expect(line("edge-5").at.x - (right!.x + right!.w)).toBeLessThanOrEqual(14);
    // The left box's stands just left of the row.
    expect(line("edge-4").at.x).toBeLessThan(left!.x);
    expect(left!.x - line("edge-4").at.x).toBeLessThanOrEqual(14);
  });

  it("writes the name of a line going back by the box it leaves, not halfway up its lane", () => {
    const picture = layOut(
      detail({
        entryPlacementId: 1,
        placements: [
          taker(1, "take"),
          step({ id: 2, name: "a" }),
          step({ id: 3, name: "b" }),
          step({ id: 4, name: "c" }),
          step({ id: 5, name: "d" }),
        ],
        edges: [
          edge({ id: 1, fromId: 1, toId: 2 }),
          edge({ id: 2, fromId: 2, toId: 3 }),
          edge({ id: 3, fromId: 3, toId: 4 }),
          edge({ id: 4, fromId: 4, toId: 5 }),
          edge({ id: 5, fromId: 5, exitName: "again", toId: 1 }),
        ],
      }),
    );
    const back = picture.lines.find((one) => one.key === "edge-5")!;
    const from = picture.nodes.find((one) => one.boxId === 5)!;
    const over = picture.nodes.find((one) => one.boxId === 4)!;
    // Under the box it leaves, and below every other box it runs past.
    expect(back.at.y).toBeGreaterThan(from.y + from.h);
    expect(back.at.y).toBeLessThan(from.y + from.h + 30);
    expect(back.at.y).toBeGreaterThan(over.y + over.h);
  });

  it("gives two lines leaving one box for the margin a leg each", () => {
    const one = detail({
      entryPlacementId: 1,
      placements: [
        taker(1, "take"),
        step({ id: 2, name: "work" }),
        step({ id: 3, name: "check", exits: [{ id: 93, name: "again", outputs: [] }, { id: 94, name: "*", outputs: [] }] }),
      ],
      edges: [
        edge({ id: 1, fromId: 1, toId: 2 }),
        edge({ id: 2, fromId: 2, toId: 3 }),
        edge({ id: 3, fromId: 3, exitName: "again", toId: 2 }),
        edge({ id: 4, fromId: 3, exitName: "*", toId: 1 }),
      ],
    });
    const picture = layOut(one);
    const leg = (key: string) => picture.lines.find((one) => one.key === key)!.points.slice(0, 3);
    const [inner, outer] = [leg("edge-3"), leg("edge-4")];
    // Tied at two places, and the one going further out turns lower and from further right.
    expect(inner[0]!.x).toBeLessThan(outer[0]!.x);
    expect(inner[1]!.y).toBeLessThan(outer[1]!.y);
  });

  it("draws the error way out only where somebody drew a line from it — the legend says the rest", () => {
    const steps = [taker(1, "take"), step({ id: 2, name: "work" })];
    const plain = layOut(
      detail({
        entryPlacementId: 1,
        placements: steps,
        edges: [edge({ id: 1, fromId: 1, toId: 2 })],
      }),
    );
    expect(plain.lines.filter((line) => line.exitName === "*")).toEqual([]);
    expect(plain.inserts.map((one) => one.edgeId)).toEqual([1]);

    const changed = layOut(
      detail({
        entryPlacementId: 1,
        placements: steps,
        edges: [
          edge({ id: 1, fromId: 1, toId: 2 }),
          edge({ id: 2, fromId: 1, exitName: "*", ends: "halt" }),
        ],
      }),
    );
    const error = changed.lines.filter((line) => line.exitName === "*");
    expect(error.map((line) => line.key)).toEqual(["edge-2"]);
    expect(error[0]!.ends).toBe("halt");
    expect(error[0]!.tone).toBe("error");
  });

  it("hangs a way out that names no step below the step it leaves, and marks how it ends", () => {
    const picture = layOut(
      detail({
        entryPlacementId: 1,
        placements: [taker(1, "take")],
        edges: [edge({ id: 1, fromId: 1, ends: "done" })],
      }),
    );
    const line = picture.lines[0]!;
    expect(line.ends).toBe("done");
    expect(line.points).toHaveLength(2);
    expect(line.points[1]!.y).toBeGreaterThan(at(picture, 1).y + at(picture, 1).h);
    // The way out and its ending are written under the foot of the stub, starting at the stub.
    expect(line.at.y).toBeGreaterThan(line.points[1]!.y);
    expect(line.align).toBe("start");
    expect(Math.abs(line.at.x - line.points[1]!.x)).toBeLessThan(10);
  });

  it("writes the endings of two ways out that go nowhere on rows of their own", () => {
    const picture = layOut(
      detail({
        entryPlacementId: 1,
        placements: [
          taker(1, "take", {
            exits: [
              { id: 91, name: "完了", outputs: [port("task", "task_take")] },
              { id: 92, name: "*", outputs: [] },
            ],
          }),
        ],
        edges: [edge({ id: 1, fromId: 1, ends: "done" }), edge({ id: 2, fromId: 1, exitName: "*", ends: "halt" })],
      }),
    );
    const [left, right] = [...picture.lines].sort((a, b) => a.points[0]!.x - b.points[0]!.x);
    // The one on the left runs further down, so its words pass under the shorter one's foot.
    expect(left!.points[1]!.y).toBeGreaterThan(right!.at.y);
    expect(left!.at.y).toBeGreaterThan(right!.at.y);
  });

  it("is wide enough to hold the words of a way out that goes nowhere, off the last box", () => {
    const picture = layOut(
      detail({
        entryPlacementId: 1,
        placements: [
          taker(1, "take", {
            exits: [
              { id: 91, name: "完了", outputs: [port("task", "task_take")] },
              { id: 92, name: "着手できるタスクが無い", outputs: [] },
              { id: 93, name: "*", outputs: [] },
            ],
          }),
          step({ id: 2, name: "work" }),
        ],
        // Hung second along the box, so its words start well to the right of the box's left side.
        edges: [
          edge({ id: 1, fromId: 1, toId: 2 }),
          edge({ id: 2, fromId: 1, exitName: "着手できるタスクが無い", ends: "done" }),
        ],
      }),
    );
    const line = picture.lines.find((one) => one.exitName === "着手できるタスクが無い")!;
    // The way out and where the run goes after it, both: the ending is the half that got cut.
    const words = edgeWord(line);
    expect(words.length).toBeGreaterThan("着手できるタスクが無い".length);
    const wide = [...words].reduce((sum, one) => sum + (one.codePointAt(0)! > 0x2e80 ? 12 : 7), 0);
    expect(line.at.x + wide).toBeLessThanOrEqual(picture.width);
  });

  it("ties a named way out first when the ways out before it have no line", () => {
    const picture = layOut(
      detail({
        entryPlacementId: 1,
        placements: [
          taker(1, "take", {
            exits: [
              { id: 90, name: "完了", outputs: [] },
              { id: 91, name: "*", outputs: [] },
              { id: 92, name: "found", outputs: [port("task", "task_take")] },
            ],
          }),
          step({ id: 2, name: "work" }),
        ],
        edges: [edge({ id: 1, fromId: 1, exitName: "found", toId: 2 })],
      }),
    );
    const line = picture.lines.find((one) => one.key === "edge-1")!;
    const box = at(picture, 1);
    // Where the first way out is tied, as the line into the next step is tied to that one.
    expect(line.points[0]!.x - box.x).toBe(line.points[3]!.x - at(picture, 2).x);
  });

  it("writes the name of a second line down to a neighbour past its right end, off the first one's corner", () => {
    const picture = layOut(
      detail({
        entryPlacementId: 1,
        placements: [
          taker(1, "take", {
            exits: [
              { id: 90, name: "完了", outputs: [port("task", "task_take")] },
              { id: 91, name: "*", outputs: [] },
              { id: 92, name: "やり直す", outputs: [] },
            ],
          }),
          step({ id: 2, name: "work" }),
        ],
        edges: [
          edge({ id: 1, fromId: 1, toId: 2 }),
          edge({ id: 2, fromId: 1, exitName: "やり直す", toId: 2 }),
        ],
      }),
    );
    const first = picture.lines.find((one) => one.key === "edge-1")!;
    const second = picture.lines.find((one) => one.key === "edge-2")!;
    // Both turn at one height; the second's leg across is shorter than its name.
    expect(second.points[1]!.y).toBe(first.points[1]!.y);
    expect(second.align).toBe("start");
    const legEnd = Math.max(second.points[1]!.x, second.points[2]!.x);
    expect(second.at.x).toBeGreaterThan(legEnd);
    expect(second.at.x).toBeGreaterThan(Math.max(first.points[0]!.x, first.points[3]!.x));
  });

  it("writes the name of a line straight down on its left, and staggers the + of lines side by side in the margin", () => {
    const picture = layOut(
      detail({
        entryPlacementId: 1,
        placements: [
          taker(1, "take"),
          step({
            id: 2,
            name: "work",
            exits: [
              { id: 80, name: "完了", outputs: [] },
              { id: 81, name: "*", outputs: [] },
              { id: 82, name: "again", outputs: [] },
            ],
          }),
        ],
        edges: [
          edge({ id: 1, fromId: 1, exitName: "next", toId: 2 }),
          edge({ id: 2, fromId: 2, toId: 1 }),
          edge({ id: 3, fromId: 2, exitName: "again", toId: 1 }),
        ],
      }),
    );
    const down = picture.lines.find((one) => one.key === "edge-1")!;
    expect(down.align).toBe("end");
    expect(down.at.x).toBeLessThan(down.points[0]!.x);
    expect(down.at.x - "next".length * 7).toBeGreaterThanOrEqual(0);
    // The two join into one line into the top box, and each keeps a `+` of its own, a `+` apart.
    const plus = (id: number) => picture.inserts.find((one) => one.edgeId === id)!;
    expect(Math.hypot(plus(2).x - plus(3).x, plus(2).y - plus(3).y)).toBeGreaterThanOrEqual(20);
  });

  it("puts the + on the line: over its leg across, or on its lane in the margin", () => {
    const picture = layOut(
      detail({
        entryPlacementId: 1,
        placements: [taker(1, "take"), step({ id: 2, name: "work" }), step({ id: 3, name: "check" })],
        edges: [
          edge({ id: 1, fromId: 1, toId: 2 }),
          edge({ id: 2, fromId: 2, toId: 3 }),
          edge({ id: 3, fromId: 3, exitName: "again", toId: 2 }),
        ],
      }),
    );
    const plus = (id: number) => picture.inserts.find((one) => one.edgeId === id)!;
    const across = picture.lines.find((one) => one.key === "edge-1")!;
    expect(plus(1).y).toBe(across.points[1]!.y);
    const lane = picture.lines.find((one) => one.key === "edge-3")!;
    const [, , turn, foot] = lane.points;
    expect(plus(3).x).toBe(turn!.x);
    expect(plus(3).y).toBeGreaterThan(Math.min(turn!.y, foot!.y));
    expect(plus(3).y).toBeLessThan(Math.max(turn!.y, foot!.y));
    // The name is written between the lane and the box, where no lane crosses it, and the picture
    // keeps room for it.
    expect(lane.align).toBe("end");
    expect(lane.at.x - "again".length * 7).toBeGreaterThan(turn!.x);
    expect(turn!.x).toBeGreaterThanOrEqual(0);
  });

  /// A long lane's `+` halfway along it stood beside some other box, apart from its name (`AMB-T-5596`).
  it("puts the + of a line in the margin by its name, at the box it leaves", () => {
    const picture = layOut(
      detail({
        entryPlacementId: 1,
        placements: [
          taker(1, "take"),
          step({ id: 2, name: "work" }),
          step({ id: 3, name: "check" }),
          step({ id: 4, name: "review", exits: [{ id: 41, name: "again", outputs: [] }] }),
        ],
        edges: [
          edge({ id: 1, fromId: 1, toId: 2 }),
          edge({ id: 2, fromId: 2, toId: 3 }),
          edge({ id: 3, fromId: 3, toId: 4 }),
          edge({ id: 4, fromId: 4, exitName: "again", toId: 1 }),
        ],
      }),
    );
    const lane = picture.lines.find((one) => one.key === "edge-4")!;
    const plus = picture.inserts.find((one) => one.edgeId === 4)!;
    const [, , turn, foot] = lane.points;
    expect(plus.x).toBe(turn!.x);
    expect(plus.y).toBeLessThan(turn!.y);
    expect(plus.y).toBeGreaterThan(foot!.y);
    expect(Math.abs(plus.y - lane.at.y)).toBeLessThanOrEqual(20);
    // Nearer the box it leaves than the one it goes back to.
    expect(turn!.y - plus.y).toBeLessThan(plus.y - foot!.y);
  });

  it("puts a + on every edge and on no wire", () => {
    const picture = layOut(
      detail({
        entryPlacementId: 1,
        placements: [
          taker(1, "take", { exits: [{ id: 91, name: "完了", outputs: [port("note", "value")] }] }),
          step({ id: 2, name: "work", inputs: [port("note", "value")] }),
        ],
        edges: [edge({ id: 1, fromId: 1, toId: 2 }), edge({ id: 2, fromId: 2, ends: "done" })],
        wires: [wire({ id: 1, fromId: 1, fromExitName: "完了", fromPortName: "note", toId: 2, toPortName: "note" })],
      }),
    );
    expect(picture.inserts.map((one) => one.edgeId).sort()).toEqual([1, 2]);
  });

  it("names the required inputs nothing reaches, and says nothing about the ones that are fed", () => {
    const steps = [
      taker(1, "take", { exits: [{ id: 91, name: "完了", outputs: [port("note", "value")] }] }),
      step({
        id: 2,
        name: "Review",
        actionId: 3,
        inputs: [port("note", "value"), port("draft", "file"), port("hint", "value", false)],
      }),
    ];
    const picture = layOut(
      detail({
        entryPlacementId: 1,
        placements: steps,
        edges: [edge({ id: 1, fromId: 1, toId: 2 })],
        wires: [wire({ id: 1, fromId: 1, fromExitName: "完了", fromPortName: "note", toId: 2, toPortName: "note" })],
      }),
    );
    expect(at(picture, 2).unfed).toEqual(["draft"]);
    expect(at(picture, 2).name).toBe("Review");
    expect(at(picture, 1).unfed).toEqual([]);
  });

  it("counts no wire from a box's own way out, nor from a box only reached through it, as reaching it", () => {
    // take → work → check → work: "note" into work comes from work itself and from check, which a run
    // only comes to after work — so the first time a run arrives at work, nothing is there.
    const out = (id: number, name: string) => ({ id, name: "完了", outputs: [port(name, "value")] });
    const looped = (wires: ReturnType<typeof wire>[]) =>
      layOut(
        detail({
          entryPlacementId: 1,
          placements: [
            taker(1, "take", { exits: [out(91, "seed")] }),
            step({ id: 2, name: "work", inputs: [port("note", "value")], exits: [out(92, "again")] }),
            step({ id: 3, name: "check", exits: [out(93, "back")] }),
          ],
          edges: [edge({ id: 1, fromId: 1, toId: 2 }), edge({ id: 2, fromId: 2, toId: 3 }), edge({ id: 3, fromId: 3, toId: 2 })],
          wires,
        }),
      );
    const fromSelf = wire({ id: 1, fromId: 2, fromExitName: "完了", fromPortName: "again", toId: 2, toPortName: "note" });
    const fromAfter = wire({ id: 2, fromId: 3, fromExitName: "完了", fromPortName: "back", toId: 2, toPortName: "note" });
    const fromBefore = wire({ id: 3, fromId: 1, fromExitName: "完了", fromPortName: "seed", toId: 2, toPortName: "note" });
    expect(at(looped([fromSelf]), 2).unfed).toEqual(["note"]);
    expect(at(looped([fromSelf, fromAfter]), 2).unfed).toEqual(["note"]);
    expect(at(looped([fromSelf, fromAfter, fromBefore]), 2).unfed).toEqual([]);
  });
});

describe("the inputs the start dialog hands over", () => {
  /** "File a task" at `at`, with the three inputs it reads at launch and one it does not. */
  function filing(at: number, entry: number): PicGraph {
    return detail({
      entryPlacementId: entry,
      placements: [
        taker(1, "take"),
        taker(2, "タスクを起票する", {
          builtin: "make_task",
          inputs: [port("タイトル", "value"), port("本文", "value"), port("選んだ分類", "value"), port("other", "value")],
        }),
      ].filter((one) => one.id === at || one.id === entry),
      edges: at === entry ? [] : [edge({ id: 1, fromId: entry, toId: at })],
    });
  }

  it("counts the entry's title, notes and chosen classification as reached, and nothing else", () => {
    expect(at(layOut(filing(2, 2)), 2).unfed).toEqual(["other"]);
  });

  it("asks for a wire into the same inputs where the box is not what a run starts at", () => {
    // Drawn in the screen's words, so only how many are asked for is the store's to say.
    expect(at(layOut(filing(2, 1)), 2).unfed).toHaveLength(4);
  });
});

describe("the picture inside an action", () => {
  /** An action of two steps, the first leaving the action by "done" and the second by the unnamed. */
  function inside(over: Partial<PicGraph> = {}): PicGraph {
    return {
      entryId: 1,
      boxes: [
        step({ id: 1, name: "draft", exits: [{ id: 11, name: "written", outputs: [port("draft", "file")] }] }),
        step({ id: 2, name: "review" }),
      ],
      edges: [
        edge({ id: 21, fromId: 1, exitName: "written", toId: 2 }),
        edge({ id: 22, fromId: 2, ends: "exit" }),
        edge({ id: 23, fromId: 1, exitName: "*", ends: "exit", exitTo: "gave up" }),
      ],
      wires: [],
      boundary: {
        inputs: [port("title", "value")],
        exits: [
          { id: 31, name: "完了", outputs: [] },
          { id: 32, name: "*", outputs: [] },
          { id: 33, name: "gave up", outputs: [port("reason", "file")] },
        ],
      },
      ...over,
    };
  }

  it("marks where a placement comes in over everything, and each way out under everything", () => {
    const picture = layOut(inside());
    const marks = picture.marks.map((one) => `${one.kind}:${one.exitName ?? ""}`);
    // The error way out last, the way the declaration lists them.
    expect(marks).toEqual(["in:", "out:完了", "out:gave up", "out:*"]);
    const lowest = Math.max(...picture.nodes.map((one) => one.y + one.h));
    for (const mark of picture.marks) {
      if (mark.kind === "in") expect(mark.y + mark.h).toBeLessThan(at(picture, 1).y);
      else expect(mark.y).toBeGreaterThan(lowest);
    }
    expect(picture.outFrame).toBeDefined();
  });

  it("draws a line from the top mark into the step opened first", () => {
    const picture = layOut(inside());
    const line = picture.lines.find((one) => one.key === "in")!;
    expect(line.leaves).toBe(true);
    expect(line.points[line.points.length - 1]!.y).toBe(at(picture, 1).y);
  });

  it("ends a line that leaves the action on the mark of the way out it returns to", () => {
    const picture = layOut(inside());
    const gaveUp = picture.marks.find((one) => one.exitName === "gave up")!;
    const line = picture.lines.find((one) => one.key.endsWith("23"))!;
    expect(line.leaves).toBe(true);
    const end = line.points[line.points.length - 1]!;
    expect(end.y).toBe(gaveUp.y);
    expect(end.x).toBeGreaterThanOrEqual(gaveUp.x);
    expect(end.x).toBeLessThanOrEqual(gaveUp.x + gaveUp.w);
  });

  it("draws no line for what the action takes in or hands out (AMB-D-1001)", () => {
    const boxes = [
      step({
        id: 1,
        name: "draft",
        inputs: [port("title", "value")],
        exits: [{ id: 11, name: "*", outputs: [port("reason", "file")] }],
      }),
      step({ id: 2, name: "review" }),
    ];
    const picture = layOut(
      inside({
        boxes,
        wires: [
          wire({ id: 41, fromId: 0, fromPortName: "title", toId: 1, toPortName: "title" }),
          wire({ id: 42, fromId: 1, fromExitName: "*", fromPortName: "reason", toId: 0, toPortName: "reason" }),
        ],
      }),
    );
    // The same picture with no wire at all: the wires change no line, and take no room.
    const without = layOut(inside({ boxes }));
    expect(picture.lines.map((one) => one.key)).toEqual(without.lines.map((one) => one.key));
    expect(picture.width).toBe(without.width);
  });

  it("draws no marks on an automation's picture", () => {
    const picture = layOut(detail({ placements: [step({ id: 1, name: "one" })] }));
    expect(picture.marks).toEqual([]);
    expect(picture.outFrame).toBeUndefined();
  });
});

describe("what the action itself hands in", () => {
  it("counts an input wired from the action itself as fed", () => {
    const picture = layOut({
      entryId: 1,
      boxes: [step({ id: 1, name: "draft", inputs: [port("title", "value")] })],
      edges: [],
      wires: [wire({ id: 41, fromId: 0, fromPortName: "title", toId: 1, toPortName: "title" })],
      boundary: { inputs: [port("title", "value")], exits: [] },
    });
    expect(at(picture, 1).unfed).toEqual([]);
  });
});

describe("a way out nothing has been decided for (AMB-D-1003)", () => {
  it("hangs dashed from its box, with the press beside its name, and the error way out does not", () => {
    const one = detail({
      entryPlacementId: 1,
      placements: [step({ id: 1, name: "write" })],
    });
    const picture = layOut(one);
    const box = at(picture, 1);
    const line = picture.lines.find((each) => each.open === true)!;
    expect(picture.lines.filter((each) => each.open === true)).toHaveLength(1);
    expect(line.exitName).toBe("完了");
    expect(line.points[0]).toEqual({ x: line.points[0]!.x, y: box.y + box.h });
    expect(line.points[1]!.y).toBeGreaterThan(box.y + box.h);
    expect(picture.opens).toEqual([expect.objectContaining({ boxId: 1, exitName: "完了" })]);
    // The press stands right of the name, under the line's foot, and inside the picture.
    expect(picture.opens[0]!.x).toBeGreaterThan(line.at.x);
    expect(picture.opens[0]!.y).toBeGreaterThan(line.points[1]!.y);
    expect(picture.width).toBeGreaterThan(picture.opens[0]!.x);
  });

  it("is gone once a line leaves by it, and stands for each of a box's ways out that still says nothing", () => {
    const branching = step({
      id: 1,
      name: "review",
      exits: [
        { id: 51, name: "ok", outputs: [] },
        { id: 52, name: "fix", outputs: [] },
        { id: 53, name: "*", outputs: [] },
      ],
    });
    const decided = layOut(
      detail({
        entryPlacementId: 1,
        placements: [branching, step({ id: 2, name: "ship" })],
        edges: [edge({ id: 1, fromId: 1, exitName: "ok", toId: 2 })],
      }),
    );
    expect(decided.opens.map((one) => `${one.boxId}-${one.exitName}`).sort()).toEqual(["1-fix", "2-完了"]);
    const two = layOut(detail({ entryPlacementId: 1, placements: [branching] }));
    // Side by side along the box, the one further left hanging lower, so neither press lies over the other.
    const [left, right] = [...two.opens].sort((a, b) => a.x - b.x);
    expect(two.opens).toHaveLength(2);
    expect(left!.y).toBeGreaterThan(right!.y);
  });

  it("stands inside the outline of the task's span, however many hang under the last row", () => {
    const picture = layOut(
      detail({
        entryPlacementId: 1,
        placements: [
          taker(1, "take", {
            exits: [
              { id: 61, name: "took", outputs: [port("task", "task_take")] },
              { id: 62, name: "none left", outputs: [] },
              { id: 63, name: "*", outputs: [] },
            ],
          }),
        ],
      }),
    );
    const lap = picture.laps[0]!;
    expect(picture.opens).toHaveLength(2);
    for (const one of picture.opens) {
      expect(one.x).toBeGreaterThan(lap.x);
      expect(one.x + 150).toBeLessThan(lap.x + lap.w);
      expect(one.y + 12).toBeLessThan(lap.y + lap.h);
    }
  });

  it("pushes the row under its box down, so neither the next box nor the line down to it runs through a press", () => {
    const review = (id: number) =>
      step({
        id,
        name: "review",
        exits: [
          { id: 70 + id * 3, name: "ok", outputs: [] },
          { id: 71 + id * 3, name: "fix", outputs: [] },
          { id: 72 + id * 3, name: "*", outputs: [] },
        ],
      });
    const shipped = edge({ id: 9, fromId: 2, ends: "done" });
    // One way out says nothing, and two: the one further left hangs lower, and the row under it with it.
    for (const open of [["fix"], ["fix", "done"]]) {
      const first = review(1);
      first.exits = [...first.exits, ...open.slice(1).map((name) => ({ id: 99, name, outputs: [] }))];
      const picture = layOut(
        detail({
          entryPlacementId: 1,
          placements: [first, step({ id: 2, name: "ship" })],
          edges: [edge({ id: 1, fromId: 1, exitName: "ok", toId: 2 }), shipped],
        }),
      );
      const next = at(picture, 2);
      const down = picture.lines.find((one) => one.key === "edge-1")!;
      const turn = down.points[1]!.y;
      expect(picture.opens.filter((one) => one.boxId === 1)).toHaveLength(open.length);
      for (const press of picture.opens.filter((one) => one.boxId === 1)) {
        // Under the press (half of it stands under its middle): the name over the turn, then the next box.
        expect(press.y + 12).toBeLessThan(down.at.y - 12);
        expect(press.y + 12).toBeLessThan(turn);
        expect(press.y + 12).toBeLessThan(next.y);
      }
      // The line down still turns half a row over the box it goes into, as every other does.
      expect(next.y - turn).toBe(28);
    }
  });

  it("leaves the room between rows as it was where every way out says something", () => {
    const picture = layOut(
      detail({
        entryPlacementId: 1,
        placements: [step({ id: 1, name: "write" }), step({ id: 2, name: "ship" })],
        edges: [edge({ id: 1, fromId: 1, toId: 2 }), edge({ id: 2, fromId: 2, ends: "done" })],
      }),
    );
    expect(picture.opens).toEqual([]);
    expect(at(picture, 2).y - (at(picture, 1).y + at(picture, 1).h)).toBe(56);
  });

  it("is not drawn for the way out a built-in never leaves by", () => {
    const picture = layOut(
      detail({
        entryPlacementId: 1,
        placements: [step({ id: 1, name: "file", neverLeavesBy: "完了" })],
      }),
    );
    expect(picture.opens).toEqual([]);
  });
});
