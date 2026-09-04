// What the ink classes are worth is where they are read, not what they say. A utility and a
// component's base rule are both one class deep, so the cascade decides between them on nothing but
// the order the sheets were loaded in — and a chip that a premise holds back was drawn in the muted
// ink of `.chip` rather than the stop step for exactly that reason, once `.chip` moved into a sheet
// read after the one the steps were written in (`AMB-T-4321`).
//
// So the order is what is pinned here, and the order alone. Nothing else about it is reachable from a
// test: the cascade is not, jsdom keeping none to ask and every value here being a `var()` it does not
// resolve, and neither is the CSS itself, vitest running with stylesheets turned off so that `?raw`
// and `?inline` both hand back the empty string. What is left is the one line that decides the
// question, and it is the line that moved when this broke.
import { describe, expect, it } from "vitest";
import entry from "../main.tsx?raw";

/** The stylesheets `main.tsx` pulls in, in the order it pulls them. */
const sheets = Array.from(entry.matchAll(/^import "([^"]+\.css)";$/gm), (m) => m[1]);

describe("the sheet the ink classes live in", () => {
  it("is read after every other one", () => {
    expect(sheets.length).toBeGreaterThan(1);
    expect(sheets[sheets.length - 1]).toBe("./styles/utilities.css");
  });
});
