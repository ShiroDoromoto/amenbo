import { describe, expect, it } from "vitest";
import { refFromUrl, refsOnRow, type Cell, type Rows } from "./refLinks";

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
