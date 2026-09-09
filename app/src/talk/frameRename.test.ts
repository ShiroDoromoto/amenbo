// The copy of a pane's name that goes to the provider running in it (`AMB-D-872`).
//
// Amenbo's own name for a frame is the card; what the provider calls its session is a copy of it, and
// the copy is made at the one door both namings come through — the agent's `talk name` and a person's
// own word. What is pinned here is when it is *not* made: a naming the store refused is a name the
// frame does not have, and a frame nothing is running in has nobody to tell.
import { beforeEach, describe, expect, it, vi } from "vitest";
import { nameFrame } from "./frames";

const hoisted = vi.hoisted(() => ({
  /** What crossed to the host, in the order it crossed. */
  asked: [] as Array<{ cmd: string; args: Record<string, unknown> }>,
  /** What the store answers a naming with — the names as they now stand. */
  standing: [] as Array<{ frame: string; name: string }>,
}));

vi.mock("../core/ipc", () => ({
  invoke: vi.fn(async (cmd: string, args: Record<string, unknown>) => {
    hoisted.asked.push({ cmd, args });
    return cmd === "name_frame" ? hoisted.standing : undefined;
  }),
}));

const renames = () => hoisted.asked.filter((one) => one.cmd === "pty_rename");

beforeEach(() => {
  hoisted.asked = [];
  hoisted.standing = [];
});

describe("naming a frame something is running in", () => {
  it("tells the provider the name the frame came away with", async () => {
    hoisted.standing = [{ frame: "1", name: "the migration" }];

    const names = await nameFrame("1", "the migration", "session", "session-7");

    expect(names.get("1")).toBe("the migration");
    expect(renames()).toEqual([
      { cmd: "pty_rename", args: { session: "session-7", name: "the migration" } },
    ]);
  });

  it("tells it nothing when the store kept the name it had", async () => {
    // A person's word outranks an agent's, so `talk name` against a frame a person named comes back
    // with the person's name. Typing the agent's into the provider would be Amenbo copying a name
    // that is on nothing.
    hoisted.standing = [{ frame: "1", name: "what the person called it" }];

    const names = await nameFrame("1", "what the agent thought of", "session", "session-7");

    expect(names.get("1")).toBe("what the person called it");
    expect(renames()).toEqual([]);
  });
});

describe("naming a frame nothing is running in", () => {
  it("names the frame and tells nobody", async () => {
    hoisted.standing = [{ frame: "2", name: "for later" }];

    const names = await nameFrame("2", "for later", "person", null);

    expect(names.get("2")).toBe("for later");
    expect(renames()).toEqual([]);
  });
});
