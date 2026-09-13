// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from "vitest";
import { composeStartsOpen, setComposeStartsOpen } from "./composeStartsOpen";

describe("where the box under the next pane starts", () => {
  beforeEach(() => localStorage.clear());

  // What `AMB-D-889` settled a pane starts as, and what a machine nobody has pressed anything on
  // answers.
  it("is folded until somebody says otherwise", () => {
    expect(composeStartsOpen()).toBe(false);
  });

  it("is open once that is what was last chosen, and stays so across reloads", () => {
    setComposeStartsOpen(true);
    expect(composeStartsOpen()).toBe(true);
  });

  it("goes back to folded on the press that folds one", () => {
    setComposeStartsOpen(true);
    setComposeStartsOpen(false);
    expect(composeStartsOpen()).toBe(false);
  });

  // A machine that will not hand anything back is a machine nobody has said anything on, which is
  // the same answer as never having pressed the control — and the pane the press was made in has
  // already folded by then, so nothing a reader can see disagrees.
  it("is folded where the machine will not remember", () => {
    const get = vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("this machine keeps nothing");
    });
    const set = vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("this machine keeps nothing");
    });

    expect(() => setComposeStartsOpen(true)).not.toThrow();
    expect(composeStartsOpen()).toBe(false);

    get.mockRestore();
    set.mockRestore();
  });
});
