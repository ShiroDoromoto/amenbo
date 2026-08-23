import { describe, expect, it } from "vitest";
import { pathsOnRow, refFromUrl, refsOnRow, type Cell, type Rows } from "./refLinks";

/**
 * A buffer written the way a terminal draws one: each row is given as text, and a row prefixed with
 * `>` is the continuation of the one above. Text shorter than `cols` is padded with blank cells, as a
 * real row is — the padding is what a scan has to step over rather than close up.
 *
 * A character that takes two columns (any CJK one here) becomes the pair a terminal stores it as: the
 * cell holding it, and the zero-width one it spills into.
 */
function buffer(cols: number, ...lines: string[]): Rows {
  const rows = lines.map((line) => {
    const wrapped = line.startsWith(">");
    const drawn = wrapped ? line.slice(1) : line;
    const cells: Cell[] = [];
    for (const ch of drawn) {
      const wide = /[　-鿿＀-｠]/.test(ch);
      cells.push({ chars: ch, width: wide ? 2 : 1 });
      if (wide) cells.push({ chars: "", width: 0 });
    }
    while (cells.length < cols) cells.push({ chars: "", width: 1 });
    return { wrapped, cells };
  });
  return {
    length: rows.length,
    wrapped: (y) => rows[y]?.wrapped ?? false,
    cells: (y) => rows[y]?.cells ?? [],
  };
}

describe("refsOnRow", () => {
  it("finds a ref an agent drew, and says where it sits", () => {
    const rows = buffer(40, "reserved AMB-T-42 and started");
    const [hit, ...rest] = refsOnRow(rows, 0);
    expect(rest).toEqual([]);
    expect(hit.text).toBe("AMB-T-42");
    expect({ space: hit.space, num: hit.num }).toEqual({ space: "task", num: 42 });
    // 1-based, both ends inclusive: `A` is the 10th column, `2` the 17th.
    expect(hit.range).toEqual({ start: { x: 10, y: 1 }, end: { x: 17, y: 1 } });
  });

  it("finds one broken across the fold, and reaches it from either row", () => {
    // The pane is 12 wide, so a row that wrapped is a row with every column drawn in: the ref starts
    // at column 10, runs to the edge, and finishes on the row below.
    const rows = buffer(12, "see also AMB", ">-D-7 for why");
    for (const y of [0, 1]) {
      const [hit, ...rest] = refsOnRow(rows, y);
      expect(rest, `row ${y}`).toEqual([]);
      expect(hit.text, `row ${y}`).toBe("AMB-D-7");
      expect({ space: hit.space, num: hit.num }).toEqual({ space: "decision", num: 7 });
      expect(hit.range, `row ${y}`).toEqual({ start: { x: 10, y: 1 }, end: { x: 4, y: 2 } });
    }
  });

  it("places a ref by the column it was drawn in, not by how far along the text it is", () => {
    // Four wide characters stand before the ref: eight columns, four characters.
    const rows = buffer(40, "作業対象 AMB-T-9 を予約した");
    const [hit] = refsOnRow(rows, 0);
    expect(hit.text).toBe("AMB-T-9");
    expect(hit.range.start).toEqual({ x: 10, y: 1 });
    expect(hit.range.end).toEqual({ x: 16, y: 1 });
  });

  it("leaves the rest of a wrapped line to the rows that carry it", () => {
    const rows = buffer(12, "AMB-T-1 and", ">AMB-T-2 too");
    expect(refsOnRow(rows, 0).map((r) => r.num)).toEqual([1]);
    expect(refsOnRow(rows, 1).map((r) => r.num)).toEqual([2]);
  });

  it("does not join what the padding keeps apart, or split what a colour code never touched", () => {
    // The blank cells between them are columns, not nothing: two refs, not one long word.
    const rows = buffer(40, "AMB-T-1   AMB-D-2");
    expect(refsOnRow(rows, 0).map((r) => `${r.space}:${r.num}`)).toEqual(["task:1", "decision:2"]);
  });

  it("leaves alone what only looks like a ref", () => {
    // A foreign tracker's key, and our own namespace run onto a longer word — the two things the
    // namespaced pattern exists to tell apart.
    const rows = buffer(40, "T-42 XAMB-T-42 PROJ-42");
    expect(refsOnRow(rows, 0)).toEqual([]);
  });

  it("answers nothing for a row the buffer does not have", () => {
    const rows = buffer(20, "AMB-T-1");
    expect(refsOnRow(rows, -1)).toEqual([]);
    expect(refsOnRow(rows, 5)).toEqual([]);
  });
});

describe("refFromUrl", () => {
  it("reads the addresses amenbo's own output writes", () => {
    expect(refFromUrl("amenbo://task/42")).toEqual({ space: "task", num: 42 });
    expect(refFromUrl("amenbo://decision/7")).toEqual({ space: "decision", num: 7 });
  });

  it("answers for nothing else — a pane draws output this app did not write", () => {
    for (const url of [
      "javascript:alert(1)",
      "https://example.com",
      "file:///etc/passwd",
      "amenbo://project/1", // a real ref space, but not one that has a destination here
      "amenbo://task/",
      "amenbo://task/1x",
      " amenbo://task/1",
      "amenbo://task/1 ",
    ]) {
      expect(refFromUrl(url), url).toBeNull();
    }
  });
});

describe("pathsOnRow", () => {
  const textsOn = (rows: Rows, y: number) => pathsOnRow(rows, y).map((one) => one.text);

  it("finds a path an agent mentioned, and says where it sits", () => {
    const rows = buffer(40, "wrote src/main.rs just now");
    const [hit, ...rest] = pathsOnRow(rows, 0);
    expect(rest).toEqual([]);
    expect(hit.text).toBe("src/main.rs");
    // 1-based, both ends inclusive: `s` is the 7th column, the last `s` the 17th.
    expect(hit.range).toEqual({ start: { x: 7, y: 1 }, end: { x: 17, y: 1 } });
  });

  it("takes a run with a separator and leaves a word alone", () => {
    expect(textsOn(buffer(40, "read README and notes/a.md"), 0)).toEqual(["notes/a.md"]);
    expect(textsOn(buffer(40, "/work/repo is where it runs"), 0)).toEqual(["/work/repo"]);
    expect(textsOn(buffer(40, "cd ../sibling"), 0)).toEqual(["../sibling"]);
    expect(textsOn(buffer(40, "wrote C:\\work\\a.md"), 0)).toEqual(["C:\\work\\a.md"]);
  });

  it("drops what a sentence left on the end, and the line number after a colon", () => {
    expect(textsOn(buffer(40, "failed at src/main.rs:12"), 0)).toEqual(["src/main.rs"]);
    expect(textsOn(buffer(40, "see notes/a.md."), 0)).toEqual(["notes/a.md"]);
    expect(textsOn(buffer(40, "(in notes/a.md)"), 0)).toEqual(["notes/a.md"]);
  });

  it("does not offer the tail of a URL as a file on this machine", () => {
    expect(textsOn(buffer(60, "docs at https://example.com/guide/x.md"), 0)).toEqual([]);
  });

  it("follows a path across the fold, the way a ref is followed", () => {
    // 20 columns: the path is drawn across two rows, and is on both of them.
    const rows = buffer(20, "wrote notes/deep/ano", ">ther/file.md now");
    expect(textsOn(rows, 0)).toEqual(["notes/deep/another/file.md"]);
    expect(textsOn(rows, 1)).toEqual(["notes/deep/another/file.md"]);
  });

  it("counts columns as a terminal does, not as characters", () => {
    // The Japanese takes two columns each, which is what the position is written in.
    const rows = buffer(40, "書いた notes/a.md");
    const [hit] = pathsOnRow(rows, 0);
    expect(hit.text).toBe("notes/a.md");
    expect(hit.range.start.x).toBe(8);
  });
});
