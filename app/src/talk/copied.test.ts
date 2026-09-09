// What a copy off a pane is allowed to drop, and what it has to leave alone.
//
// The inputs here are the six agents as they were measured on a real PTY (`AMB-T-4606`): the padding
// GitHub Copilot CLI and OpenCode write to the right edge, Copilot's scrollbar in the last column,
// the mark Claude Code and Copilot draw at the head of an answer, Gemini CLI's line numbers, and the
// inset all six put their body in. Each rule is pinned twice — that it takes the drawing off, and
// that it stops at the drawing.
import { describe, expect, it } from "vitest";
import { tidiedCopy } from "./copied";

/** A Copilot row: the body padded to the pane's width, with its scrollbar standing in the last column. */
function copilotRow(text: string, width = 40): string {
  return text.padEnd(width - 1, " ") + "┃";
}

describe("the inset a selection shares", () => {
  it("comes off all of it, and what a line has beyond it stays", () => {
    // Claude Code insets a code block by six columns. Python is the reason the rest is kept.
    const drawn = "      def greet(name):\n          print(name)";
    expect(tidiedCopy(drawn, "claude-code")).toBe("def greet(name):\n    print(name)");
  });

  it("is not read off a blank line", () => {
    const drawn = "    first\n\n    second";
    expect(tidiedCopy(drawn, "cursor")).toBe("first\n\nsecond");
  });

  it("comes off a pane with no agent in it too", () => {
    expect(tidiedCopy("  ls -la\n  cat x", null)).toBe("ls -la\ncat x");
  });
});

describe("the padding at the end of a line", () => {
  it("comes off — OpenCode writes it to the right edge", () => {
    const drawn = "     hello   \n     world      ";
    expect(tidiedCopy(drawn, "opencode")).toBe("hello\nworld");
  });
});

describe("GitHub Copilot CLI's scrollbar", () => {
  it("comes off, and the padding it was holding up goes with it", () => {
    const drawn = [copilotRow("   hello"), copilotRow("   world")].join("\n");
    expect(tidiedCopy(drawn, "github-copilot")).toBe("hello\nworld");
  });

  it("is nobody else's bar to drop", () => {
    // Only Copilot draws one. A heavy bar in another agent's output is that agent's character.
    expect(tidiedCopy("  a┃\n  b┃", "opencode")).toBe("a┃\nb┃");
  });

  it("is not the rule a table is drawn with", () => {
    // Every one of the six draws tables with the light `│`, so the table comes across whole.
    const drawn = [
      copilotRow("   ┌───────┬───────┐"),
      copilotRow("   │ A     │ B     │"),
      copilotRow("   └───────┴───────┘"),
    ].join("\n");
    expect(tidiedCopy(drawn, "github-copilot")).toBe(
      "┌───────┬───────┐\n│ A     │ B     │\n└───────┴───────┘",
    );
  });
});

describe("the mark at the head of an answer", () => {
  it("becomes the space it stood in, so the code under it still lines up", () => {
    // Claude Code: the answer's body sits two columns in, the code six. Dropping the mark rather
    // than replacing it would take the first line two columns left of the rest.
    const drawn = "⏺ Here it is:\n\n      def greet(name):\n          print(name)";
    expect(tidiedCopy(drawn, "claude-code")).toBe(
      "Here it is:\n\n    def greet(name):\n        print(name)",
    );
  });

  it("is read where the agent draws it — Copilot's stands one column in", () => {
    const drawn = [copilotRow(" ● Here it is:"), copilotRow("       code()")].join("\n");
    expect(tidiedCopy(drawn, "github-copilot")).toBe("Here it is:\n    code()");
  });

  it("is another agent's character, not a mark", () => {
    expect(tidiedCopy("⏺ still here", "gemini-cli")).toBe("⏺ still here");
  });

  it("comes off every answer a selection reaches across, not only the first", () => {
    // Leaving the second mark would hold the shared inset at nothing and indent everything else.
    const drawn = "⏺ first\n  a line\n⏺ second";
    expect(tidiedCopy(drawn, "claude-code")).toBe("first\na line\nsecond");
  });

  it("is not there to find when the selection starts mid-answer", () => {
    expect(tidiedCopy("  the middle of it", "claude-code")).toBe("the middle of it");
  });
});

describe("Gemini CLI's line numbers", () => {
  it("come off, and a line it folded comes off by the same width", () => {
    // A continuation carries no number of its own, so the column is blank across it.
    const drawn = "     1 def greet():\n     2     print(name)\n       banana banana";
    expect(tidiedCopy(drawn, "gemini-cli")).toBe("def greet():\n    print(name)\nbanana banana");
  });

  it("stay when one line answers to neither — that block was never numbered", () => {
    // Prose that opens with a year. Cutting a guessed width off it would eat the prose.
    const drawn = "  2026 was the year\n  and then some";
    expect(tidiedCopy(drawn, "gemini-cli")).toBe("2026 was the year\nand then some");
  });

  it("are nobody else's column to drop", () => {
    expect(tidiedCopy("     1 def greet():\n     2     print(name)", "claude-code")).toBe(
      "1 def greet():\n2     print(name)",
    );
  });
});

describe("what is never dropped", () => {
  it("is a tree's depth, or the bullet standing at it", () => {
    const drawn = "  - one\n    - two\n      - three";
    expect(tidiedCopy(drawn, "claude-code")).toBe("- one\n  - two\n    - three");
  });

  it("is a diff's signs", () => {
    const drawn = "  --- a/x.rs\n  +++ b/x.rs\n  @@ -1 +1 @@\n  -old\n  +new";
    expect(tidiedCopy(drawn, "cursor")).toBe("--- a/x.rs\n+++ b/x.rs\n@@ -1 +1 @@\n-old\n+new");
  });
});

describe("an empty selection", () => {
  it("stays empty", () => {
    expect(tidiedCopy("", "claude-code")).toBe("");
  });
});
