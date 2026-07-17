import { describe, expect, it } from "vitest";
import { pushNotice, subscribeNotice } from "./notice";

describe("notice bus", () => {
  it("delivers messages to subscribers", () => {
    const seen: string[] = [];
    const off = subscribeNotice((m) => seen.push(m));
    pushNotice("hello");
    pushNotice("world");
    off();
    expect(seen).toEqual(["hello", "world"]);
  });

  it("nothing arrives after unsubscribe", () => {
    const seen: string[] = [];
    const off = subscribeNotice((m) => seen.push(m));
    off();
    pushNotice("dropped");
    expect(seen).toEqual([]);
  });

  it("drops the message when there are no subscribers (does not throw)", () => {
    expect(() => pushNotice("no listeners")).not.toThrow();
  });
});
