// @vitest-environment jsdom
// A write is sent once, and a second press before its answer comes back is dropped. The presses here all
// land in one go, before any render — the way presses piled up behind a stuck app arrive — which is why the
// guard has to be a ref and not the `busy` state alone.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, describe, expect, it } from "vitest";
import { useSingleFlight, useSingleFlightPerKey } from "./singleFlight";

beforeAll(() => {
  (globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});

/** A write whose answer the test lets go of. */
function held() {
  let release: () => void = () => {};
  const done = new Promise<void>((r) => { release = r; });
  return { done, release };
}

let host: HTMLDivElement;
let root: Root;
beforeEach(() => {
  host = document.createElement("div");
  document.body.appendChild(host);
  root = createRoot(host);
});
afterEach(() => {
  act(() => root.unmount());
  host.remove();
});

describe("useSingleFlight", () => {
  it("sends once for presses that arrive before the answer, and again once it has come back", async () => {
    let sent = 0;
    let write = held();
    let api: ReturnType<typeof useSingleFlight> | null = null;
    const Probe = () => {
      api = useSingleFlight();
      return createElement("button", { disabled: api.busy });
    };
    act(() => root.render(createElement(Probe)));
    const send = () => api!.run(() => { sent++; return write.done; });

    let answers: boolean[] = [];
    act(() => { answers = [send(), send(), send()]; });
    expect(answers).toEqual([true, false, false]);
    expect(sent).toBe(1);
    expect(host.querySelector("button")!.disabled).toBe(true);

    await act(async () => { write.release(); await write.done; });
    expect(host.querySelector("button")!.disabled).toBe(false);

    write = held();
    act(() => { send(); });
    expect(sent).toBe(2);
  });

  it("takes a press again after a write that failed", async () => {
    let api: ReturnType<typeof useSingleFlight> | null = null;
    const Probe = () => { api = useSingleFlight(); return null; };
    act(() => root.render(createElement(Probe)));

    await act(async () => { api!.run(() => Promise.reject(new Error("refused"))); });
    let again = false;
    act(() => { again = api!.run(() => undefined); });
    expect(again).toBe(true);
  });
});

describe("useSingleFlightPerKey", () => {
  it("holds a second move of the same card, and lets another card move meanwhile", async () => {
    const sent: number[] = [];
    const write = held();
    let moveOnce: ReturnType<typeof useSingleFlightPerKey<number>> | null = null;
    const Probe = () => { moveOnce = useSingleFlightPerKey<number>(); return null; };
    act(() => root.render(createElement(Probe)));
    const move = (id: number) => moveOnce!(id, () => { sent.push(id); return write.done; });

    act(() => { move(1); move(1); move(2); });
    expect(sent).toEqual([1, 2]);

    await act(async () => { write.release(); await write.done; });
    act(() => { move(1); });
    expect(sent).toEqual([1, 2, 1]);
  });
});
