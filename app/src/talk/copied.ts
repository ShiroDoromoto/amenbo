// What a copy off a pane carries, once the agent's drawing has been taken back out of it.
//
// A pane is a real terminal, so what an agent answers with is a picture: the width of the pane is in
// it, and so are the marks the agent draws around its own words. Selecting three lines of Python and
// pressing `⌘C` puts all of that on the clipboard — the padding to the right edge, the scrollbar
// standing in the last column, the round mark at the head of the answer, the line numbers, and the
// two columns every line of the answer is inset by. None of it is what the person selected.
//
// So the picture is taken back out on the way to the clipboard rather than out of the pane
// (`AMB-D-867`): what is drawn stays drawn, and what is *carried* is the text. Everything dropped
// here is the agent's drawing and never a character a person chose — which is what makes it safe to
// do without asking, and why there is no second way to copy (`AMB-D-867`).
//
// **The rules are the measured ones** (`AMB-T-4606`, six agents on a real PTY), not a guess about
// what a terminal might contain. Three of the five belong to one agent each, and which agent is in
// the pane is something the pane knows — it started it — so they are keyed by that rather than
// recognised in the drawing.
//
// **What this cannot put back is a line the agent folded itself.** All six fold long lines at the
// pane's width and re-inset the continuation, so nothing in the buffer says the line was ever one
// (`AMB-T-4606`), and no rule here invents it.

/**
 * The bar GitHub Copilot CLI stands in the rightmost column, its scrollbar. It is not the table
 * rule: tables are drawn with the light `│`, so dropping this one leaves them whole.
 */
const SCROLLBAR = "┃";

/** The agent whose drawing has that bar in it. */
const SCROLLBAR_AGENT = "github-copilot";

/**
 * The mark an agent draws at the head of an answer, by the agent that draws it.
 *
 * It is replaced by as many spaces rather than dropped, so the first line keeps the inset the lines
 * under it have — which is what lets the common inset come off all of them together and leaves the
 * code as it was written.
 */
const HEAD_MARK: Record<string, string> = {
  "claude-code": "⏺ ",
  "github-copilot": "● ",
};

/** The agent that numbers the lines of a code block. */
const NUMBERING_AGENT = "gemini-cli";

/** A line's number column: spaces, the right-aligned number, and the one space after it. */
const NUMBER_COLUMN = /^ *\d+ /;

/**
 * The text a selection puts on the clipboard.
 *
 * `agent` is the catalogued id of what is running in the pane (`amenbo_core::harness`), `null` for a
 * bare prompt. The rules that belong to one agent are skipped for every other, so a shell's output
 * is carried exactly as it was drawn but for the two rules that hold everywhere — the padding at the
 * end of a line, and the inset the whole selection shares.
 */
export function tidiedCopy(selection: string, agent: string | null): string {
  if (selection === "") return "";
  let lines = selection.split("\n");
  // First, because the bar is not a space: leave it and the padding in front of it never ends a line.
  if (agent === SCROLLBAR_AGENT) lines = lines.map(withoutScrollbar);
  lines = lines.map((line) => line.replace(/\s+$/, ""));
  const mark = agent === null ? undefined : HEAD_MARK[agent];
  if (mark !== undefined) lines = withoutHeadMark(lines, mark);
  if (agent === NUMBERING_AGENT) lines = withoutNumberColumn(lines);
  return withoutCommonInset(lines).join("\n");
}

/** The scrollbar off the end of a line. */
function withoutScrollbar(line: string): string {
  return line.endsWith(SCROLLBAR) ? line.slice(0, -SCROLLBAR.length) : line;
}

/**
 * The mark that stands at the head of an answer, turned into the space it stood in.
 *
 * Every line that carries one, not only the first: a selection can reach across two answers, and
 * leaving the second mark where it is would hold the whole selection's shared inset at nothing and
 * leave every other line indented by it.
 */
function withoutHeadMark(lines: string[], mark: string): string[] {
  return lines.map((line) => {
    const inset = /^ */.exec(line)?.[0] ?? "";
    if (!line.startsWith(mark, inset.length)) return line;
    return inset + " ".repeat(mark.length) + line.slice(inset.length + mark.length);
  });
}

/**
 * The number column off every line, or off none of them.
 *
 * The width is read off the first line and every other line has to answer to it — numbered to the
 * same width, or blank across it, which is what a line the agent folded looks like (a continuation
 * carries no number of its own). One line that answers to neither means this was never a numbered
 * block, and nothing is dropped: taking a guessed number of characters off prose would eat the
 * prose.
 */
function withoutNumberColumn(lines: string[]): string[] {
  const width = NUMBER_COLUMN.exec(lines[0] ?? "")?.[0].length;
  if (width === undefined) return lines;
  const numbered = (line: string) => NUMBER_COLUMN.exec(line)?.[0].length === width;
  const blankAcross = (line: string) => line === "" || line.startsWith(" ".repeat(width));
  if (!lines.every((line) => numbered(line) || blankAcross(line))) return lines;
  return lines.map((line) => line.slice(width));
}

/**
 * The inset the whole selection shares, off all of it.
 *
 * What each line has *beyond* the shared inset is kept, so a Python block comes off with its own
 * indentation intact. Blank lines are not counted — a line with nothing on it says nothing about how
 * far in the block sits.
 */
function withoutCommonInset(lines: string[]): string[] {
  let common: number | null = null;
  for (const line of lines) {
    if (line === "") continue;
    const inset = /^ */.exec(line)?.[0].length ?? 0;
    if (common === null || inset < common) common = inset;
  }
  if (common === null || common === 0) return lines;
  const shared = common;
  return lines.map((line) => line.slice(shared));
}
