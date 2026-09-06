import { describe, expect, it } from "vitest";
import { folderName, frameLabel } from "./frames";

describe("what a pane is called before anything has named it", () => {
  it("is the folder it works in", () => {
    expect(folderName("/work/amenbo")).toBe("amenbo");
    expect(folderName("C:\\work\\amenbo")).toBe("amenbo");
    expect(folderName("/work/repo/")).toBe("repo");
    expect(folderName("/")).toBeNull();
    expect(folderName(null)).toBeNull();
  });

  it("gives way to the name the moment there is one", () => {
    const names = new Map([["1", "the migration"]]);
    expect(frameLabel(names, "1", "/work/amenbo")).toBe("the migration");
    expect(frameLabel(names, "2", "/work/amenbo")).toBe("amenbo");
    expect(frameLabel(names, "2", null)).toBeNull();
  });
});
