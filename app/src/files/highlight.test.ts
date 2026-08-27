// Which colour a token is drawn in, which is the whole of what this bridge decides for itself.
import { describe, expect, it } from "vitest";

import { kindOf } from "./highlight";

describe("kindOf", () => {
  it("reads the innermost scope anything here recognises", () => {
    expect(kindOf(["source.rust", "meta.function.rust", "entity.name.function.rust"]))
      .toBe("function");
    // The innermost scope means nothing here, so the one outside it answers.
    expect(kindOf(["source.rust", "comment.line.double-slash.rust", "meta.nothing.rust"]))
      .toBe("comment");
  });

  // `keyword.operator` is a keyword by prefix and an operator by intent. Longest match first is
  // what keeps `=` from being drawn as `fn`.
  it("prefers the longer prefix where two would match", () => {
    expect(kindOf(["source.rust", "keyword.operator.assignment.equal.rust"])).toBe("operator");
    expect(kindOf(["source.rust", "keyword.other.fn.rust"])).toBe("keyword");
  });

  // A prefix continues at a dot or not at all: TextMate scopes are dotted paths, and matching on
  // raw text would read `constantly` as a constant.
  it("matches whole segments, not text", () => {
    expect(kindOf(["source.x", "constant.numeric.x"])).toBe("number");
    expect(kindOf(["source.x", "constantly.wrong.x"])).toBeNull();
  });

  // A key is a key whatever the format calls it: three grammars, three scopes, one colour.
  it("reads a data format's key the same way whichever format it is", () => {
    expect(kindOf(["source.yaml", "entity.name.tag.yaml"])).toBe("tag");
    expect(kindOf(["source.toml", "variable.other.key.toml"])).toBe("tag");
  });

  it("has no opinion about a scope it does not know", () => {
    expect(kindOf(["source.rust"])).toBeNull();
    expect(kindOf([])).toBeNull();
  });
});
