// @vitest-environment jsdom
// The shape this machine was last used in, remembered across launches (`AMB-D-753`).
import { beforeEach, describe, expect, it } from "vitest";
import { getWindowShape, setWindowShape } from "./windowShape";

beforeEach(() => localStorage.clear());

describe("the shape a machine is using Amenbo in", () => {
  it("starts as one window, so nothing is split out until somebody asks", () => {
    expect(getWindowShape()).toBe("one");
  });

  it("comes back as it was left", () => {
    setWindowShape("two");
    expect(getWindowShape()).toBe("two");
    setWindowShape("one");
    expect(getWindowShape()).toBe("one");
  });

  it("reads anything else as one window — a value from a future version is not a second window", () => {
    localStorage.setItem("amenbo.windowShape", "three");
    expect(getWindowShape()).toBe("one");
  });
});
