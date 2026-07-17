// Deciding what counts as an external change — firing only for those, and thinning out our own writes — is
// `watchStore`'s job (`sig === lastSignature`), and it depends on the Tauri host, so it is out of unit scope; the
// bridge below is called after that gate. What is pinned here is the pub-sub contract and nothing else.
import { describe, it, expect } from "vitest";

import {
  subscribeStoreChangeReflected,
  notifyStoreChangeReflected,
  type StoreChangeReflected,
} from "./snapshot";

const LIVE: StoreChangeReflected = { reason: "live", at: 1 };
const GAP: StoreChangeReflected = { reason: "gap", at: 2 };

describe("external-reflection notification bridge", () => {
  it("subscribers receive the fired payload as-is (reason/at/storeIds)", () => {
    const seen: StoreChangeReflected[] = [];
    const un = subscribeStoreChangeReflected((r) => seen.push(r));
    try {
      notifyStoreChangeReflected(LIVE);
      notifyStoreChangeReflected(GAP);
    } finally {
      un();
    }
    expect(seen).toEqual([LIVE, GAP]);
  });

  it("is delivered to every subscriber", () => {
    let a = 0;
    let b = 0;
    const unA = subscribeStoreChangeReflected(() => a++);
    const unB = subscribeStoreChangeReflected(() => b++);
    try {
      notifyStoreChangeReflected(LIVE);
    } finally {
      unA();
      unB();
    }
    expect([a, b]).toEqual([1, 1]);
  });

  it("no fire arrives after unsubscribe (an idempotent double-unsubscribe does not throw)", () => {
    let n = 0;
    const un = subscribeStoreChangeReflected(() => n++);
    un();
    un(); // unsubscribing twice is harmless
    notifyStoreChangeReflected(LIVE);
    expect(n).toBe(0);
  });

  it("firing with zero subscribers is safe (no-op)", () => {
    expect(() => notifyStoreChangeReflected(GAP)).not.toThrow();
  });
});
