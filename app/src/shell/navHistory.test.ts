import { describe, it, expect } from "vitest";
import { navReduce, NO_SELECTION, type NavState, type Location, type Selection } from "./navHistory";
import type { Nav } from "./AppShell";

const v = (id: string): Nav => ({ type: "view", id });
const p = (id: string): Nav => ({ type: "project", id });
const task = (id: number): Selection => ({ type: "task", id });
const decision = (id: number): Selection => ({ type: "decision", id });
const L = (nav: Nav, sel: Selection = NO_SELECTION): Location => ({ nav, sel });
const start = (l: Location): NavState => ({ stack: [l], index: 0 });
const push = (s: NavState, l: Location) => navReduce(s, { type: "push", loc: l });

describe("navReduce", () => {
  it("push appends and advances the index", () => {
    let s = start(L(v("a")));
    s = push(s, L(v("b")));
    expect(s).toEqual({ stack: [L(v("a")), L(v("b"))], index: 1 });
  });

  it("does not stack a push to the current location (same nav+sel)", () => {
    const s0 = start(L(v("a")));
    const s1 = push(s0, L(v("a")));
    expect(s1).toBe(s0); // the same state object comes back — no idle history entry
  });

  it("distinguishes same id across nav types", () => {
    let s = start(L(v("x")));
    s = push(s, L(p("x")));
    expect(s).toEqual({ stack: [L(v("x")), L(p("x"))], index: 1 });
  });

  it("treats a selection change on the same nav as a new location", () => {
    // Task → decision keeps the same Nav but is a different Location, so it gets its own history entry. This is what makes "task → decision → back" land on the original task.
    let s = start(L(p("proj"), task(1)));
    s = push(s, L(p("proj"), decision(1)));
    expect(s.stack).toEqual([L(p("proj"), task(1)), L(p("proj"), decision(1))]);
    expect(s.index).toBe(1);
  });

  it("does not stack when nav and selection are both unchanged", () => {
    const s0 = start(L(p("proj"), task(1)));
    expect(push(s0, L(p("proj"), task(1)))).toBe(s0);
  });

  it("distinguishes selection kinds and ids on the same nav", () => {
    let s = start(L(p("proj"), task(1)));
    s = push(s, L(p("proj"), task(2)));   // a different task
    s = push(s, L(p("proj"), NO_SELECTION)); // selection cleared
    expect(s.stack).toEqual([
      L(p("proj"), task(1)),
      L(p("proj"), task(2)),
      L(p("proj"), NO_SELECTION),
    ]);
  });

  it("back restores the previous location's selection (task→decision→back)", () => {
    let s = start(L(p("proj"), task(1)));
    s = push(s, L(p("proj"), decision(1)));
    s = navReduce(s, { type: "back" });
    expect(s.stack[s.index]).toEqual(L(p("proj"), task(1)));
  });

  it("back/forward move the index without mutating the stack", () => {
    let s = start(L(v("a")));
    s = push(s, L(v("b")));
    s = push(s, L(v("c"))); // [a,b,c] @2
    s = navReduce(s, { type: "back" }); // @1
    expect(s.index).toBe(1);
    s = navReduce(s, { type: "back" }); // @0
    expect(s.index).toBe(0);
    s = navReduce(s, { type: "forward" }); // @1
    expect(s).toEqual({ stack: [L(v("a")), L(v("b")), L(v("c"))], index: 1 });
  });

  it("is a no-op at the ends (back at 0, forward at last)", () => {
    const s0 = start(L(v("a")));
    expect(navReduce(s0, { type: "back" })).toBe(s0);
    expect(navReduce(s0, { type: "forward" })).toBe(s0);
  });

  it("push after going back truncates the forward tail", () => {
    let s = start(L(v("a")));
    s = push(s, L(v("b")));
    s = push(s, L(v("c"))); // [a,b,c] @2
    s = navReduce(s, { type: "back" }); // @1 (b)
    s = push(s, L(v("d"))); // drops c and everything after it, then pushes d
    expect(s).toEqual({ stack: [L(v("a")), L(v("b")), L(v("d"))], index: 2 });
  });
});
