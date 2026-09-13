// Why a pane says a program stopped, and the far commoner case of it saying nothing of the kind.
//
// What a program leaves on the screen is the whole of why it stopped, so a pane that added an
// account of its own would be talking over it. The exception is an ending Amenbo had a hand in: a
// provider pointed at a home of Amenbo's making names *that* path when it stops over a file it found
// nothing in, and a reader who follows the message edits a file that is thrown away with the pane.
import { describe, expect, it } from "vitest";

import { whyItStopped } from "./terminal";

describe("why a pane says the program stopped", () => {
  // Measured on Gemini CLI 0.59.0 against a home with nothing in it: it exits 41 and names the home
  // it was pointed at, which is the pane's own (`AMB-T-4729`).
  it("says which file to set where a provider stopped on one Amenbo pointed elsewhere", () => {
    expect(whyItStopped("gemini-cli", 41)).toBe("face.endedGeminiUnset");
  });

  // The same provider stopping over something else. 52 is a settings file it could not parse and 55
  // a folder it has not been told to trust — both its own message's business, and neither one this
  // has a word for.
  it("says nothing about that provider's other endings", () => {
    expect(whyItStopped("gemini-cli", 52)).toBeNull();
    expect(whyItStopped("gemini-cli", 55)).toBeNull();
    expect(whyItStopped("gemini-cli", 0)).toBeNull();
  });

  // The number belongs to one provider and means nothing on another: read without the agent it would
  // be Amenbo putting Gemini's words on somebody else's ending.
  it("says nothing where another provider left the same number", () => {
    expect(whyItStopped("claude-code", 41)).toBeNull();
    expect(whyItStopped("codex-cli", 41)).toBeNull();
    expect(whyItStopped(null, 41)).toBeNull();
  });

  // A program ended rather than ending — a signal, or Amenbo taking the pane away — leaves no status
  // to read, and nothing can be said about a stop nobody asked for.
  it("says nothing where there is no status to read", () => {
    expect(whyItStopped("gemini-cli", null)).toBeNull();
  });
});
