// What putting a path in front of an agent is allowed to do, and the one thing it must not.
//
// The text is wrapped in a bracketed paste so a program that reads it reads text, and **no carriage
// return follows it** (`AMB-D-793`): the screen at that moment may be a first-run question, and a
// newline would answer it — one of the choices actually found there ran `curl … | sh`. The person is
// sitting in front of the pane, so the sentence is left where they can read it and press Enter.
import { describe, expect, it, vi } from "vitest";
import { pasteIntoTerminal } from "./terminal";

const hoisted = vi.hoisted(() => ({
  /** What was written to the terminal, as the command took it. */
  written: [] as Array<{ session: string; data: string }>,
}));

vi.mock("../core/ipc", () => ({
  invoke: vi.fn(async (cmd: string, args: { session: string; data: string }) => {
    if (cmd !== "pty_write") throw new Error(`unexpected command ${cmd}`);
    hoisted.written.push(args);
  }),
}));

describe("putting text in a terminal's input box", () => {
  it("pastes it, and sends nothing that would submit it", async () => {
    await pasteIntoTerminal("session-7", "/work/here/.amenbo-inbox/2026-08-29/shot.png");

    expect(hoisted.written).toEqual([{
      session: "session-7",
      data: "\x1b[200~/work/here/.amenbo-inbox/2026-08-29/shot.png\x1b[201~",
    }]);
    expect(hoisted.written[0]?.data, "a newline went with the paste").not.toMatch(/[\r\n]/);
  });
});
