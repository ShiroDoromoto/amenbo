// The sentence a road that ends every terminal at once asks with (`AMB-T-4676`).
//
// The plain one promises that every conversation comes back, and that promise is only true while
// every pane on the screen has a way back into its session. What is pinned here is the other case:
// the host names the providers that have none, and the question names them too instead of promising
// something it cannot keep.
import { beforeEach, describe, expect, it, vi } from "vitest";
import { endingConfirm } from "./openPanes";

const hoisted = vi.hoisted(() => ({
  /** What the host answers `panes_without_a_way_back` with — the providers with no way in. */
  stranded: [] as string[],
  /** Whether the host refuses the question at all. */
  refuses: false,
}));

vi.mock("../core/ipc", () => ({
  invoke: vi.fn(async (cmd: string) => {
    if (hoisted.refuses) throw new Error("no host");
    return cmd === "panes_without_a_way_back" ? hoisted.stranded : undefined;
  }),
}));

beforeEach(() => {
  hoisted.stranded = [];
  hoisted.refuses = false;
});

describe("the question on the way out", () => {
  it("says every conversation comes back where every pane has a way in", async () => {
    expect(await endingConfirm("quit.confirm", "quit.confirmNotAll", "en"))
      .toBe("Quit Amenbo? Every terminal open in it ends. "
        + "The conversations come back on the next run; what they were running does not.");
  });

  it("names the provider whose pane will not be in its conversation", async () => {
    hoisted.stranded = ["Gemini CLI"];

    expect(await endingConfirm("quit.confirm", "quit.confirmNotAll", "en"))
      .toContain("apart from the panes running Gemini CLI.");
  });

  // Two names are run together by the language's own rule rather than by a separator kept here
  // (`../core/i18n/format`), which is what lets one dictionary entry serve any number of them.
  it("runs several names together the way the reader's language does", async () => {
    hoisted.stranded = ["Gemini CLI", "OpenCode"];

    expect(await endingConfirm("quit.confirm", "quit.confirmNotAll", "en"))
      .toContain("Gemini CLI and OpenCode");
    expect(await endingConfirm("quit.confirm", "quit.confirmNotAll", "ja"))
      .toContain("Gemini CLI、OpenCode");
  });

  // A question that failed to draw is worse than one that says less, so a host that cannot answer
  // leaves the plain sentence standing rather than taking the road out with it.
  it("falls back to the plain sentence where the host does not answer", async () => {
    hoisted.refuses = true;

    expect(await endingConfirm("restart.confirm", "restart.confirmNotAll", "en"))
      .toBe("Restart Amenbo? Every terminal open in it ends. "
        + "The conversations come back on the next run; what they were running does not.");
  });
});
