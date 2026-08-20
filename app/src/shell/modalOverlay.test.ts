// One backdrop class, swept from the tree. `isBlankSpaceClose` reads `.modal__overlay` to tell that a
// pointerdown landed on an open modal rather than on blank body, so a modal whose backdrop wears a class
// of its own is invisible to it: grabbing that backdrop closes the right pane behind the modal the reader
// is still looking at. Nothing in the type system says so, and each of the three feature modals had its
// own `*__overlay` until this was found — hence a test rather than a convention.
//
// What it can see: a class spelled as a literal in the JSX. A backdrop whose class is computed would slip
// past, which is the trade for a check that needs no runtime. The stylesheet is not read alongside it —
// `?raw` on a `.css` file comes back empty under Vite — but a rule no element wears is dead either way.
import { describe, expect, it } from "vitest";

const tsx = import.meta.glob("../**/*.tsx", { query: "?raw", import: "default", eager: true }) as Record<string, string>;

describe("the modal backdrop", () => {
  it("is the one every modal in the tree wears", () => {
    const strays = Object.entries(tsx).flatMap(([file, source]) =>
      (source.match(/[A-Za-z0-9]+__overlay/g) ?? [])
        .filter((word) => word !== "modal__overlay")
        .map((word) => `${file}: ${word}`),
    );
    expect(strays).toEqual([]);
  });
});
