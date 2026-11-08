// @vitest-environment jsdom
// The row that declares one more name, read from the keyboard. Enter adds what is in the box — but not
// the Enter that accepts a conversion while Japanese is being written, which belongs to the input method.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi, type Mock } from "vitest";

import { DeclareRow } from "./automationPanel";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

describe("Enter in the row that declares one more", () => {
  let root: Root;
  let host: HTMLDivElement;
  let onAdd: Mock<(name: string, kind: string) => Promise<boolean>>;

  beforeEach(() => {
    host = document.createElement("div");
    document.body.appendChild(host);
    root = createRoot(host);
    onAdd = vi.fn<(name: string, kind: string) => Promise<boolean>>(() => Promise.resolve(true));
    act(() => {
      root.render(createElement(DeclareRow, { what: "name", kinds: null, onAdd }));
    });
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  /** Write `text` in the box the way a person does, through React's controlled value. */
  async function write(text: string): Promise<void> {
    const field = host.querySelector("input")!;
    await act(async () => {
      const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set;
      setter?.call(field, text);
      field.dispatchEvent(new Event("input", { bubbles: true }));
    });
  }

  /** Press a key in the box, and let whatever it set off settle. */
  async function pressed(key: string, held: KeyboardEventInit = {}): Promise<void> {
    const event = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...held });
    await act(async () => { host.querySelector("input")!.dispatchEvent(event); });
    await act(async () => { await Promise.resolve(); });
  }

  it("adds the name on a plain Enter", async () => {
    await write("theme");
    await pressed("Enter");
    expect(onAdd).toHaveBeenCalledTimes(1);
    expect(onAdd).toHaveBeenCalledWith("theme", "");
  });

  it("adds nothing on the Enter that accepts a conversion", async () => {
    await write("テーマ");
    await pressed("Enter", { isComposing: true });
    expect(onAdd).not.toHaveBeenCalled();
  });

  it("adds nothing on an Enter sent with keyCode 229, as an older input method sends it", async () => {
    await write("テーマ");
    await pressed("Enter", { keyCode: 229 });
    expect(onAdd).not.toHaveBeenCalled();
  });
});
