// @vitest-environment jsdom
// The command catalogue is a reference, and what makes it one is that a row carries the command and
// nothing else. The CLI a build installs is named once at the top instead — because on a preview
// build that name is a 60-character path (`AMB-D-732`), and a path repeated down every row pushes the
// command off to the right and wraps the summary a reader came to scan.
//
// What these guard: the name is said **once**, the rows are **bare**, and a build that installs no CLI
// a reader can run says so in that same place rather than naming one.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { t } from "../core/i18n";

const hoisted = vi.hoisted(() => ({
  /** What this build installs, as `useCliCommandName` answers it — a path on a preview, `null` where there is none. */
  cli: "amenbo" as string | null,
}));

vi.mock("../core/cliCommand", () => ({
  PRODUCTION_CLI: "amenbo",
  useCliCommandName: () => hoisted.cli,
}));
vi.mock("../core/reads", () => ({
  useAgentSpec: () => ({
    spec: {
      commands: [
        { name: "task add", summary: "Creates a task in a project." },
        { name: "task show", summary: "Shows task details." },
      ],
      capabilities: [{ capability: "Work with tasks", commands: ["task add", "task show"] }],
    },
    loading: false,
  }),
}));

import { CommandCatalogScreen } from "./CommandCatalogScreen";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

const render = () => act(() => root.render(createElement(CommandCatalogScreen)));
/** The command each row leads with — the `code` at the head of the row's own button. */
const rowCommands = () =>
  Array.from(container.querySelectorAll("button.cmdcat__head code")).map((el) => el.textContent);

beforeEach(() => {
  hoisted.cli = "amenbo";
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("CommandCatalogScreen naming the CLI", () => {
  it("names the path once and leaves the rows bare", () => {
    hoisted.cli = "/Applications/amenbo (dev 3519).app/Contents/MacOS/amenbo-dev-3519";
    render();

    expect(container.textContent).toContain(t("commands.prefix"));
    // Once, and only in the line that says it — never again down the rows.
    expect(container.textContent!.split(hoisted.cli).length - 1).toBe(1);
    expect(rowCommands()).toEqual(["task add", "task show"]);
  });

  it("says there is no command to type where the build installs none", () => {
    hoisted.cli = null;
    render();

    expect(container.textContent).toContain(t("cli.none"));
    expect(container.textContent).not.toContain(t("commands.prefix"));
    expect(rowCommands()).toEqual(["task add", "task show"]);
  });
});
