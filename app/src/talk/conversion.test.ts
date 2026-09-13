// @vitest-environment jsdom
// What happens to a word an input method was still writing when the keyboard left the pane.
//
// The emulator empties its own field on `blur`, so the characters are gone from the one place they
// were kept — and the emulator itself goes on believing a conversion is open, which is what turns the
// key that would accept them into a bare Enter. Both halves are pinned here: the characters go to the
// program rather than nowhere, and the emulator is told the conversion is over.
import { describe, expect, it } from "vitest";
import { settlesWhatTheKeyboardLeft } from "./terminal";

/** A field standing in for the emulator's own, with the one behaviour that matters: it is emptied
 *  when the keyboard leaves it, the way the emulator empties its own. */
function field(): { area: HTMLTextAreaElement; leave: () => void } {
  const area = document.createElement("textarea");
  document.body.appendChild(area);
  // Registered on the field itself, after the watcher is on the document — which is the order the
  // emulator's own listener stands in, and what the capture phase has to beat.
  area.addEventListener("blur", () => { area.value = ""; });
  return {
    area,
    leave: () => area.dispatchEvent(new FocusEvent("blur")),
  };
}

function writes(area: HTMLTextAreaElement, text: string): void {
  area.dispatchEvent(new CompositionEvent("compositionstart", { data: "" }));
  area.value = text;
  area.dispatchEvent(new CompositionEvent("compositionupdate", { data: text }));
}

describe("a conversion the keyboard left in the middle", () => {
  it("sends what it had written to the program", () => {
    const { area, leave } = field();
    const sent: string[] = [];
    const stop = settlesWhatTheKeyboardLeft(area, (text) => sent.push(text));
    writes(area, "にほんご");
    leave();
    expect(sent, "the characters were thrown away with the field").toEqual(["にほんご"]);
    stop();
  });

  it("tells the emulator the conversion is over, so the next Enter is an Enter", () => {
    const { area, leave } = field();
    const ended: (string | null)[] = [];
    area.addEventListener("compositionend", (e) => ended.push((e as CompositionEvent).data));
    const stop = settlesWhatTheKeyboardLeft(area, () => {});
    writes(area, "にほんご");
    leave();
    expect(ended).toEqual(["にほんご"]);
    stop();
  });

  it("leaves a pane with no conversion open alone", () => {
    const { area, leave } = field();
    const sent: string[] = [];
    const stop = settlesWhatTheKeyboardLeft(area, (text) => sent.push(text));
    area.value = "echo";
    leave();
    expect(sent).toEqual([]);
    stop();
  });

  it("leaves a conversion the person finished alone — that one went out on its own", () => {
    const { area, leave } = field();
    const sent: string[] = [];
    const stop = settlesWhatTheKeyboardLeft(area, (text) => sent.push(text));
    writes(area, "にほんご");
    area.dispatchEvent(new CompositionEvent("compositionend", { data: "にほんご" }));
    leave();
    expect(sent, "the emulator had already been given it").toEqual([]);
    stop();
  });

  it("sends nothing for a conversion that had written nothing yet", () => {
    const { area, leave } = field();
    const sent: string[] = [];
    const stop = settlesWhatTheKeyboardLeft(area, (text) => sent.push(text));
    area.dispatchEvent(new CompositionEvent("compositionstart", { data: "" }));
    leave();
    expect(sent).toEqual([]);
    stop();
  });

  it("keeps what stood in the field before the conversion out of it", () => {
    const { area, leave } = field();
    const sent: string[] = [];
    const stop = settlesWhatTheKeyboardLeft(area, (text) => sent.push(text));
    area.value = "echo";
    area.dispatchEvent(new CompositionEvent("compositionstart", { data: "" }));
    area.value = "echoにほんご";
    leave();
    expect(sent).toEqual(["にほんご"]);
    stop();
  });

  it("stops listening when the pane is taken down", () => {
    const { area, leave } = field();
    const sent: string[] = [];
    settlesWhatTheKeyboardLeft(area, (text) => sent.push(text))();
    writes(area, "にほんご");
    leave();
    expect(sent).toEqual([]);
  });
});
