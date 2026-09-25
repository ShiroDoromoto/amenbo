// @vitest-environment jsdom
// The parts every automation screen draws the same way (`./automationParts`): one way of saying how
// many automations use an action, and one mark for a way out whichever screen it is on.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { t, tn } from "../core/i18n";
import { ERROR_EXIT } from "./automationLayout";
import { ExitMark, usedCount, WhereMark } from "./automationParts";

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("how many automations use an action", () => {
  it("is said one way, and in a word where there are none", () => {
    expect(usedCount(2)).toBe(tn("auto.actions.usedBy", 2));
    expect(usedCount(0)).toBe(t("auto.actions.unused"));
  });
});

describe("a way out", () => {
  it("draws the error one apart from the ones a reader named", async () => {
    await act(async () => {
      root.render(
        createElement("div", null, createElement(ExitMark, { name: "done" }), createElement(ExitMark, { name: ERROR_EXIT })),
      );
    });
    const marks = [...container.querySelectorAll(".actport")];
    expect(marks.map((one) => one.className)).toEqual(["actport actport--exit", "actport actport--error"]);
    expect(marks[1]!.textContent).toBe(t("auto.pic.errorExit"));
  });

  it("carries what it hands on beside its name", async () => {
    await act(async () => {
      root.render(createElement(ExitMark, { name: "taken", outputs: ["task"] }));
    });
    expect(container.querySelector(".actport__kind")?.textContent).toBe("task");
  });
});

describe("where a placement goes", () => {
  it("is only the here onto a picture with no line yet", async () => {
    await act(async () => {
      root.render(createElement(WhereMark, { where: null }));
    });
    expect(container.querySelector(".wheremark__box")).toBeNull();
    expect(container.querySelector(".wheremark")?.textContent).toBe(t("auto.lib.here"));
  });
});
