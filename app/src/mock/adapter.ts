// The read port for the light, bounded metadata that does not depend on the store's size. The data comes from
// amenbo-core's snapshot cache (`../core/snapshot.ts` → the `snapshot` Tauri command). Reading a list of tasks
// belongs to the task_page hooks in core/reads.ts; what is returned here is only bounded metadata — the roster,
// the projects, activity, the smart-view definitions. The inbox count badge is not here either: the reactive
// `mailbox.useInboxCount` hook supplies it, arrival detection and all. In a plain browser (`npm run dev`) the
// snapshot falls back to fixtures.
import { getSnapshot } from "../core/snapshot";
import type {
  ActivityItem, Actor, Project, SmartView,
} from "./types";

export const dataAdapter = {
  listRoster(): Actor[] {
    return getSnapshot().roster;
  },

  listProjects(): Project[] {
    return getSnapshot().projects;
  },

  getProject(id: number): Project | undefined {
    return getSnapshot().projects.find((p) => p.id === id);
  },

  // mirrors `activity [--project/--task/--actor/--kind]`
  listActivity(): ActivityItem[] {
    return getSnapshot().activity;
  },

  // The smart views, as ids and nothing else. What each is called comes from the
  // translations and what it is drawn with from the sidebar (`AMB-D-689`), so neither
  // belongs in the data.
  smartViews(): SmartView[] {
    return [{ id: "inbox" }, { id: "activity" }];
  },
};
