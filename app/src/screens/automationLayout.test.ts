// What the picture of one automation comes out as, checked without a screen (`AMB-T-5255`).
//
// What these guard is the reading, not the pixels: **a task's span is one outline** and the spot
// that takes the next task starts the next one; **depth runs down and the same depth runs across**;
// **what comes after a spot is tied down the left and what it hands on down the right**; **a line
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
          exits: [{ id: 91, name: "完了", outputs: [port("note", "value")] }, { id: 92, name: "*", outputs: [] }],
        }),
        step({ id: 2, name: "work", inputs: [port("note", "value")] }),
      ],
      edges: [edge({ id: 1, fromId: 1, toId: 2 })],
      wires: [wire({ id: 1, fromId: 1, fromExitName: "完了", fromPortName: "note", toId: 2, toPortName: "note" })],
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
    const laneX = (key: string) => Math.min(...line(key).points.map((p) => p.x));
    // The one past every row outside the two that run past fewer, so neither crosses it.
    expect(laneX("edge-9")).toBeLessThan(laneX("edge-8"));
    expect(laneX("edge-8")).toBeLessThan(laneX("edge-7"));
    // Into the third step, three lines: the two back and the one from the row above. Each lands on
    // a place of its own, and turns down at a height of its own.
    const last = (key: string) => line(key).points.slice(-2);
    const into = ["edge-7", "edge-8", "edge-2"].map(last);
    expect(new Set(into.map(([, foot]) => foot!.x)).size).toBe(3);
    expect(new Set(into.map(([turn]) => turn!.y)).size).toBe(3);
    // Nearer the lanes, the one on the inner lane — so the outer one's last leg passes over it.
    expect(last("edge-7")[1]!.x).toBeLessThan(last("edge-8")[1]!.x);
    expect(last("edge-8")[0]!.y).toBeLessThan(last("edge-7")[0]!.y);
  });

  it("writes the name of a line in the margin past every lane, level with the leg it leaves its box by", () => {
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
    // The inner line's name is written outside the outer line's lane, so that lane does not cross it.
    expect(laneX("edge-3")).toBeGreaterThan(laneX("edge-4"));
    expect(line("edge-3").at.x).toBeLessThan(laneX("edge-4"));
    expect(line("edge-3").align).toBe("end");
    // Level with the leg out of its box, and the picture keeps the room for it.
    expect(line("edge-3").at.y - line("edge-3").points[1]!.y).toBe(4);
    expect(line("edge-3").at.x - "again".length * 7).toBeGreaterThanOrEqual(0);
    // Two lines leaving one box write their names a row apart.
    expect(Math.abs(line("edge-3").at.y - line("edge-4").at.y)).toBeGreaterThanOrEqual(15);
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

  it("draws the error way out of every box, as the stop it is where nobody said what follows it", () => {
    const steps = [taker(1, "take"), step({ id: 2, name: "work" })];
    const plain = layOut(
      detail({
        entryPlacementId: 1,
        placements: steps,
        edges: [edge({ id: 1, fromId: 1, toId: 2 })],
      }),
    );
    const unsaid = plain.lines.filter((line) => line.exitName === "*");
    expect(unsaid.map((line) => line.points[0]!.y)).toEqual([
      at(plain, 1).y + at(plain, 1).h,
      at(plain, 2).y + at(plain, 2).h,
    ]);
    for (const line of unsaid) {
      expect(line.ends).toBe("halt");
      expect(line.tone).toBe("error");
    }
    // It hangs right of the line the box does have, and there is no edge under it to put a box in on.
    const onFirst = unsaid[0]!;
    const next = plain.lines.find((line) => line.key === "edge-1")!;
    expect(onFirst.points[0]!.x).toBeGreaterThan(next.points[0]!.x);
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
    const error = changed.lines.filter((line) => line.exitName === "*" && line.points[0]!.y === at(changed, 1).y + at(changed, 1).h);
    expect(error.map((line) => line.key)).toEqual(["edge-2"]);
    expect(error[0]!.ends).toBe("halt");
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
    const plus = (id: number) => picture.inserts.find((one) => one.edgeId === id)!;
    expect(plus(2).x).not.toBe(plus(3).x);
    expect(Math.abs(plus(2).y - plus(3).y)).toBeGreaterThanOrEqual(20);
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
    // The name is written outside the lane, where there are no steps, and the picture keeps room for it.
    expect(lane.align).toBe("end");
    expect(lane.at.x).toBeLessThan(turn!.x);
    expect(lane.at.x - "again".length * 7).toBeGreaterThanOrEqual(0);
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
      { from: "title", to: ["title"] },
      { from: "reason", to: ["reason"] },
    ]);
    // Into the way out's mark — down into its top, since the next way out stands beside it.
    const gaveUp = picture.marks.find((one) => one.exitName === "gave up")!;
    const end = wires[1]!.branches![0]!.slice(-1)[0]!;
    expect(end.y).toBe(gaveUp.y);
    expect(end.x).toBeGreaterThan(gaveUp.x);
    expect(end.x).toBeLessThan(gaveUp.x + gaveUp.w);
  });

  it("draws no marks on an automation's picture", () => {
    const picture = layOut(detail({ placements: [step({ id: 1, name: "one" })] }));
    expect(picture.marks).toEqual([]);
    expect(picture.outFrame).toBeUndefined();
  });
});

describe("what the action itself hands in", () => {
  it("counts an input wired from the action itself as fed, and draws that wire out to the right", () => {
    const picture = layOut({
      entryId: 1,
      boxes: [step({ id: 1, name: "draft", inputs: [port("title", "value")] })],
      edges: [],
      wires: [wire({ id: 41, fromId: 0, fromPortName: "title", toId: 1, toPortName: "title" })],
      boundary: { inputs: [port("title", "value")], exits: [] },
    });
    expect(at(picture, 1).unfed).toEqual([]);
    // Out past the boxes and back in, never straight down: a wire under a box would cross the names
    // of the lines that leave it there.
    const line = picture.lines.find((one) => one.kind === "wire")!;
    const box = at(picture, 1);
    expect(line.branches).toHaveLength(1);
    expect(line.points[1]!.x).toBeGreaterThan(box.x + box.w);
    expect(line.branches![0]!.slice(-1)[0]!.x).toBe(box.x + box.w);
  });
});

describe("the wires", () => {
  /** A step that hands `draft` on, and three that take it, one under the other. */
  const fanned = (wires: AutomationWireDto[]) =>
    layOut(
      detail({
        entryPlacementId: 1,
        placements: [
          taker(1, "write", { exits: [{ id: 91, name: "完了", outputs: [port("draft", "value"), port("notes", "value")] }] }),
          step({ id: 2, name: "check", inputs: [port("draft", "value")] }),
          step({ id: 3, name: "fix", inputs: [port("draft", "value")] }),
          step({ id: 4, name: "ship", inputs: [port("draft", "value"), port("notes", "value")] }),
        ],
        edges: [
          edge({ id: 1, fromId: 1, toId: 2 }),
          edge({ id: 2, fromId: 2, toId: 3 }),
          edge({ id: 3, fromId: 3, toId: 4 }),
        ],
        wires,
      }),
    );

  it("draws one output handed to several boxes as one line, split into each of them", () => {
    const picture = fanned([
      wire({ id: 1, fromId: 1, fromPortName: "draft", toId: 2, toPortName: "draft" }),
      wire({ id: 2, fromId: 1, fromPortName: "draft", toId: 3, toPortName: "draft" }),
      wire({ id: 3, fromId: 1, fromPortName: "draft", toId: 4, toPortName: "draft" }),
    ]);
    const wires = picture.lines.filter((one) => one.kind === "wire");
    expect(wires).toHaveLength(1);
    expect(wires[0]!.hands).toEqual({ from: "draft", to: ["draft", "draft", "draft"] });
    // Every branch comes off the one trunk, and ends at the right side of a box that takes it.
    const trunkX = wires[0]!.points[1]!.x;
    expect(wires[0]!.branches!.map((branch) => branch[0]!.x)).toEqual([trunkX, trunkX, trunkX]);
    expect(wires[0]!.branches!.map((branch) => branch.slice(-1)[0]!.x)).toEqual(
      [2, 3, 4].map((id) => at(picture, id).x + at(picture, id).w),
    );
  });

  it("gives every wire an end of its own down a box's right side, the legs in over the stems out", () => {
    const value = (name: string) => port(name, "value", false);
    // Handed on down the picture and back up it, with two outputs of one name leaving the same way.
    const picture = layOut(
      detail({
        entryPlacementId: 1,
        placements: [
          taker(1, "take"),
          step({ id: 2, name: "cut", exits: [{ id: 200, name: "完了", outputs: [value("tree")] }, { id: 201, name: "*", outputs: [] }] }),
          step({ id: 3, name: "build", inputs: [value("tree"), value("note")] }),
          step({
            id: 4,
            name: "review",
            inputs: [value("tree")],
            exits: [{ id: 400, name: "完了", outputs: [] }, { id: 401, name: "fix", outputs: [value("note")] }, { id: 402, name: "*", outputs: [] }],
          }),
          step({
            id: 5,
            name: "merge",
            inputs: [value("tree")],
            exits: [
              { id: 500, name: "完了", outputs: [value("commit")] },
              { id: 501, name: "red", outputs: [value("note")] },
              { id: 502, name: "*", outputs: [] },
            ],
          }),
          step({ id: 6, name: "close", inputs: [value("commit")] }),
        ],
        edges: [
          edge({ id: 1, fromId: 1, toId: 2 }),
          edge({ id: 2, fromId: 2, toId: 3 }),
          edge({ id: 3, fromId: 3, toId: 4 }),
          edge({ id: 4, fromId: 4, toId: 5 }),
          edge({ id: 5, fromId: 5, toId: 6 }),
          edge({ id: 6, fromId: 4, exitName: "fix", toId: 3 }),
          edge({ id: 7, fromId: 5, exitName: "red", toId: 3 }),
        ],
        wires: [
          wire({ id: 1, fromId: 2, fromPortName: "tree", toId: 3, toPortName: "tree" }),
          wire({ id: 2, fromId: 2, fromPortName: "tree", toId: 4, toPortName: "tree" }),
          wire({ id: 3, fromId: 2, fromPortName: "tree", toId: 5, toPortName: "tree" }),
          wire({ id: 4, fromId: 4, fromExitName: "fix", fromPortName: "note", toId: 3, toPortName: "note" }),
          wire({ id: 5, fromId: 5, fromExitName: "red", fromPortName: "note", toId: 3, toPortName: "note" }),
          wire({ id: 6, fromId: 5, fromPortName: "commit", toId: 6, toPortName: "commit" }),
        ],
      }),
    );
    const drawn = (key: string) => picture.lines.find((one) => one.key === key)!;
    const stemY = (key: string) => drawn(key).points[0]!.y;
    const landY = (key: string, nth = 0) => drawn(key).branches![nth]!.slice(-1)[0]!.y;
    // Into the fifth step one leg comes, and out of it two stems go: three heights, the leg highest.
    const into5 = landY("wire-1", 2);
    expect(new Set([into5, stemY("wire-5"), stemY("wire-6")]).size).toBe(3);
    expect(into5).toBeLessThan(Math.min(stemY("wire-5"), stemY("wire-6")));
    // Into the third step's one input two wires come up from below: each lands at its own height.
    expect(landY("wire-4")).not.toBe(landY("wire-5"));
    // The one from the trunk further out lands higher, so its leg passes over the nearer trunk.
    expect(drawn("wire-5").points[1]!.x).toBeGreaterThan(drawn("wire-4").points[1]!.x);
    expect(landY("wire-5")).toBeLessThan(landY("wire-4"));
    // And a wire says the way out it is handed on by, where that way out has a name.
    expect(drawn("wire-5").exitName).toBe("red");
    expect(drawn("wire-6").exitName).toBeUndefined();
  });

  it("comes down into the top of a box with another beside it, rather than through that one", () => {
    const picture = layOut(
      detail({
        entryPlacementId: 1,
        placements: [
          taker(1, "write", {
            exits: [
              { id: 91, name: "完了", outputs: [port("draft", "value")] },
              // Handing the task on too, so both of what follows are that task's and share a row.
              { id: 92, name: "other", outputs: [port("task", "task_take")] },
            ],
          }),
          step({ id: 2, name: "left", inputs: [port("draft", "value")] }),
          step({ id: 3, name: "right" }),
        ],
        edges: [edge({ id: 1, fromId: 1, toId: 2 }), edge({ id: 2, fromId: 1, toId: 3, exitName: "other" })],
        wires: [wire({ id: 1, fromId: 1, fromPortName: "draft", toId: 2, toPortName: "draft" })],
      }),
    );
    const left = at(picture, 2);
    const right = at(picture, 3);
    expect(left.y).toBe(right.y);
    const branch = picture.lines.find((one) => one.kind === "wire")!.branches![0]!;
    const end = branch.slice(-1)[0]!;
    // Into the top, inside the box, and the leg across runs over the row rather than through it.
    expect(end.y).toBe(left.y);
    expect(end.x).toBeLessThan(left.x + left.w);
    expect(branch.slice(-2)[0]!.y).toBeLessThan(right.y);
  });

  it("gives each output of a box a trunk of its own, and writes its name past every trunk", () => {
    const picture = fanned([
      wire({ id: 1, fromId: 1, fromPortName: "draft", toId: 4, toPortName: "draft" }),
      wire({ id: 2, fromId: 1, fromPortName: "notes", toId: 4, toPortName: "notes" }),
    ]);
    const wires = picture.lines.filter((one) => one.kind === "wire");
    expect(wires.map((one) => one.hands?.from)).toEqual(["draft", "notes"]);
    // Side by side rather than one over the other: the two run past the same rows.
    expect(wires[0]!.points[1]!.x).not.toBe(wires[1]!.points[1]!.x);
    // Each leaves at a height of its own, and lands in the input it names.
    expect(wires[0]!.points[0]!.y).not.toBe(wires[1]!.points[0]!.y);
    expect(wires[0]!.branches![0]!.slice(-1)[0]!.y).toBeLessThan(wires[1]!.branches![0]!.slice(-1)[0]!.y);
    // The name is past the outermost trunk, where no line runs through it, level with its own stem;
    // and the picture is wide enough to hold it.
    const outermost = Math.max(...wires.map((one) => one.points[1]!.x));
    for (const one of wires) {
      expect(one.at.x).toBeGreaterThan(outermost);
      expect(Math.abs(one.at.y - one.points[0]!.y)).toBeLessThan(8);
      expect(one.at.x).toBeLessThan(picture.width);
    }
  });
});
