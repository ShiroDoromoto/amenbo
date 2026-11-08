// @vitest-environment jsdom
// The roles the task editor offers: read once per project, and again only when a role can have moved.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { TaskRoleDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  roles: [] as TaskRoleDto[],
  /** Which projects were asked about, in order. */
  asked: [] as number[],
}));

vi.mock("./mutations", () => ({
  fetchProjectRoles: (projectId: number) => {
    hoisted.asked.push(projectId);
    return Promise.resolve(hoisted.roles);
  },
}));

import { useProjectRoles } from "./projectRoles";
import { invalidateScopes } from "./query";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// The answers are cached by project across tests, so each test asks about a project no other one has.
let nextProject = 100;

let container: HTMLDivElement;
let root: Root;
let seen: TaskRoleDto[];

function Probe({ projectId }: { projectId: number | null }) {
  seen = useProjectRoles(projectId);
  return null;
}

const render = (projectId: number | null = nextProject) =>
  act(async () => { root.render(createElement(Probe, { projectId })); });

const role = (name: string) => ({ name }) as unknown as TaskRoleDto;

beforeEach(() => {
  nextProject += 1;
  hoisted.roles = [];
  hoisted.asked = [];
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the roles a project's tasks can be worked under", () => {
  it("asks about the project on screen, and once", async () => {
    hoisted.roles = [role("reviewer")];
    await render();

    expect(hoisted.asked).toEqual([nextProject]);
    expect(seen).toEqual([role("reviewer")]);
  });

  // A role defined on the build screen while a task is open beside it.
  it("reads again when an automation moves", async () => {
    await render();
    hoisted.roles = [role("reviewer")];

    await act(async () => { invalidateScopes(new Set(["automations"])); });

    expect(hoisted.asked).toEqual([nextProject, nextProject]);
    expect(seen).toEqual([role("reviewer")]);
  });

  it("does not read again when something else in the store moves", async () => {
    await render();

    await act(async () => { invalidateScopes(new Set(["tasks", "projects", "decisions"])); });

    expect(hoisted.asked).toEqual([nextProject]);
  });

  // A task in no project has no automation to take a role from.
  it("asks nothing for a task in no project", async () => {
    await render(null);

    expect(hoisted.asked).toEqual([]);
    expect(seen).toEqual([]);
  });
});
