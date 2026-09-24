// What the picture of one automation comes out as, checked without a screen (`AMB-T-5255`).
//
// What these guard is the reading, not the pixels: **a task's span is one outline** and the spot
// that takes the next task starts the next one; **depth runs down and the same depth runs across**;
// **what comes after a spot is tied down the left and what it hands on down the right**; **a line
// whose two boxes are not neighbours leaves for a lane**, dashed where it goes back up; **the error
// way out is drawn only where somebody said what follows it**; and **a spot nothing reaches is still
// drawn**, which is the state every half-built automation is in.
import { describe, expect, it } from "vitest";
import { automationGraph, layOut, type PicGraph } from "./automationLayout";
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
    actionId: 900 + over.id,
    prompt: "",
    interactive: false,
    reportToTask: false,
    showHistory: true,
    // What core writes at birth: the unnamed way out, and the error one nobody can delete.
    exits: [
      { id: nextId++, outputs: [] },
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
  return { ends: "go", ...over };
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
        taker(1, "take", { exits: [{ id: 91, name: "yes", outputs: [] }, { id: 92, name: "no", outputs: [] }] }),
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
    // Two ways out of one step leave a finger apart, so their names are written a step apart too.
    const word = (key: string) => picture.lines.find((line) => line.key === key)!.at;
    expect(word("edge-1").y).not.toBe(word("edge-2").y);
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

  it("draws no outline around steps that answer to no task, and still places what nothing reaches", () => {
    const one = detail({
      entryPlacementId: 1,
      placements: [step({ id: 1, name: "entry that takes nothing" }), step({ id: 2, name: "stranded" })],
    });
    const picture = layOut(one);
    expect(picture.laps).toEqual([]);
    expect(picture.nodes.map((node) => node.boxId).sort()).toEqual([1, 2]);
  });

  it("ties what comes after a step down its left and what it hands on down its right", () => {
    const one = detail({
      entryPlacementId: 1,
      placements: [
        taker(1, "take", {
          exits: [{ id: 91, outputs: [port("note", "value")] }, { id: 92, name: "*", outputs: [] }],
        }),
        step({ id: 2, name: "work", inputs: [port("note", "value")] }),
      ],
      edges: [edge({ id: 1, fromId: 1, toId: 2 })],
      wires: [wire({ id: 1, fromId: 1, fromPortName: "note", toId: 2, toPortName: "note" })],
    });
    const picture = layOut(one);
    const middle = at(picture, 1).x + at(picture, 1).w / 2;
    const line = (kind: "edge" | "wire") => picture.lines.find((one) => one.kind === kind)!;
    expect(line("edge").points[0]!.x).toBeLessThan(middle);
    expect(line("wire").points[0]!.x).toBeGreaterThan(middle);
  });

  it("sends a line back to a shallower row out to a lane, dashed", () => {
    const one = detail({
      entryPlacementId: 1,
      placements: [
        taker(1, "take"),
        step({ id: 2, name: "work" }),
        step({ id: 3, name: "check", exits: [{ id: 93, outputs: [] }, { id: 94, name: "again", outputs: [] }] }),
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

  it("gives two lines crossing the same rows a lane each", () => {
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
    const laneX = (key: string) =>
      Math.min(...picture.lines.find((line) => line.key === key)!.points.map((p) => p.x));
    expect(laneX("edge-4")).not.toBe(laneX("edge-5"));
  });

  it("draws the error way out only where somebody said what follows it", () => {
    const steps = [taker(1, "take"), step({ id: 2, name: "work" })];
    const plain = layOut(
      detail({
        entryPlacementId: 1,
        placements: steps,
        edges: [edge({ id: 1, fromId: 1, toId: 2 })],
      }),
    );
    expect(plain.lines.some((line) => line.exitName === "*")).toBe(false);

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
    const error = changed.lines.find((line) => line.exitName === "*")!;
    expect(error.ends).toBe("halt");
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
    // The ending is written at the foot of the stub, under the way out's own name.
    expect(line.endAt!.y).toBeGreaterThanOrEqual(line.points[1]!.y);
    // ...and beside the stub, not off to its left where the neighbouring names are.
    expect(line.endAt!.x).toBeGreaterThan(line.points[1]!.x);
    expect(line.endAt!.x - line.points[1]!.x).toBeLessThan(20);
  });

  it("puts a + on every edge and on no wire", () => {
    const picture = layOut(
      detail({
        entryPlacementId: 1,
        placements: [
          taker(1, "take", { exits: [{ id: 91, outputs: [port("note", "value")] }] }),
          step({ id: 2, name: "work", inputs: [port("note", "value")] }),
        ],
        edges: [edge({ id: 1, fromId: 1, toId: 2 }), edge({ id: 2, fromId: 2, ends: "done" })],
        wires: [wire({ id: 1, fromId: 1, fromPortName: "note", toId: 2, toPortName: "note" })],
      }),
    );
    expect(picture.inserts.map((one) => one.edgeId).sort()).toEqual([1, 2]);
  });

  it("names the required inputs nothing reaches, and says nothing about the ones that are fed", () => {
    const steps = [
      taker(1, "take", { exits: [{ id: 91, outputs: [port("note", "value")] }] }),
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
        wires: [wire({ id: 1, fromId: 1, fromPortName: "note", toId: 2, toPortName: "note" })],
      }),
    );
    expect(at(picture, 2).unfed).toEqual(["draft"]);
    expect(at(picture, 2).name).toBe("Review");
    expect(at(picture, 1).unfed).toEqual([]);
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
          { id: 31, outputs: [] },
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
    expect(marks).toEqual(["in:", "out:", "out:gave up", "out:*"]);
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

  it("wires what the action takes in from the top mark, and what it hands out into a way out's mark", () => {
    const picture = layOut(
      inside({
        boxes: [
          step({
            id: 1,
            name: "draft",
            inputs: [port("title", "value")],
            exits: [{ id: 11, name: "*", outputs: [port("reason", "file")] }],
          }),
          step({ id: 2, name: "review" }),
        ],
        wires: [
          wire({ id: 41, fromId: 0, fromPortName: "title", toId: 1, toPortName: "title" }),
          wire({ id: 42, fromId: 1, fromExitName: "*", fromPortName: "reason", toId: 0, toPortName: "reason" }),
        ],
      }),
    );
    const wires = picture.lines.filter((one) => one.kind === "wire");
    expect(wires.map((one) => one.hands)).toEqual([
      { from: "title", to: "title" },
      { from: "reason", to: "reason" },
    ]);
    const gaveUp = picture.marks.find((one) => one.exitName === "gave up")!;
    expect(wires[1]!.points[wires[1]!.points.length - 1]!.y).toBe(gaveUp.y);
  });

  it("draws no marks on an automation's picture", () => {
    const picture = layOut(detail({ placements: [step({ id: 1, name: "one" })] }));
    expect(picture.marks).toEqual([]);
    expect(picture.outFrame).toBeUndefined();
  });
});

describe("what the action itself hands in", () => {
  it("counts an input wired from the action itself as fed, and draws that wire straight down", () => {
    const picture = layOut({
      entryId: 1,
      boxes: [step({ id: 1, name: "draft", inputs: [port("title", "value")] })],
      edges: [],
      wires: [wire({ id: 41, fromId: 0, fromPortName: "title", toId: 1, toPortName: "title" })],
      boundary: { inputs: [port("title", "value")], exits: [] },
    });
    expect(at(picture, 1).unfed).toEqual([]);
    const line = picture.lines.find((one) => one.kind === "wire")!;
    expect(line.points).toHaveLength(4);
  });
});
