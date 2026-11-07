// @vitest-environment jsdom
// The lines Amenbo writes under a kept tail: the terminal's last lines, each on a row of its own and
// dimmed, so they read as the terminal's own and not as the program's.
import { describe, expect, it } from "vitest";

import { saidUnder } from "./terminal";

describe("the lines written under a kept tail", () => {
  it("each start a row of their own after the tail, dimmed", () => {
    expect(saidUnder("Error: upload failed", ["ended", "filed"])).toBe(
      "\r\n\x1b[2mended\x1b[22m\r\n\x1b[2mfiled\x1b[22m",
    );
  });

  it("start on the first row where nothing was kept", () => {
    expect(saidUnder("", ["ended"])).toBe("\x1b[2mended\x1b[22m");
  });

  it("are nothing where there is nothing to say", () => {
    expect(saidUnder("Error", [])).toBe("");
  });
});
