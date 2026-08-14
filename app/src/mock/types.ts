// Domain types for the GUI. The wire shapes are generated from the Rust DTOs in
// `app/src-tauri/src/dto.rs` by ts-rs; this file re-exports them under the app-facing names (and
// extends the few that carry client-only fields), so a Rust-side rename/add/remove is a TypeScript
// error here rather than a runtime `undefined`. Regenerate with `cargo test` in `app/src-tauri` (the
// derive plants an `export_bindings_*` test that writes `app/src/bindings/bindings.ts`).
import type {
  ActorDto,
  PlacementDto,
  TaskCardDto,
  ProjectDto,
  ActivityItemDto,
} from "../bindings/bindings";

// The enum vocabularies are projected off the generated DTO field types (the sole source of these literal unions is Rust's #[ts(type=...)]).
export type Facet = ActorDto["kind"];
export type Status = TaskCardDto["status"];
export type Priority = NonNullable<TaskCardDto["priority"]>;

/** An actor on an activity item: a person seen through one facet. */
export type Actor = ActorDto;

/** assignee = a person + facet. kind:"ai" means "that person's AI". null = unassigned. */
export type Assignee = Actor | null;

/** Where a task sits. The real data behind the project field in the detail pane. */
export type Placement = PlacementDto;

/**
 * A task card. The wire part is the generated `TaskCardDto` (core always sends every field). The last two are
 * **client-only** and core never sends them: the inbox trigger time (`triggeredAt` = when the activity that put it in
 * the inbox last happened, RFC3339 UTC), and the unread dot for things addressed to you (`unread`).
 */
export type TaskCard = TaskCardDto & {
  triggeredAt?: string | null;
  unread?: boolean;            // Inbox: whether an unread comment addressed to you remains (drives the unread dot only)
  unseen?: boolean;            // Inbox source C: this device entered it after last viewing it (or never) — notification only, no dot
};

export type Project = ProjectDto;

/**
 * Last-seen (read) state: per device, and never synced. Drives the inbox's unread marks and badge. It is device-local
 * and does not ride the wire, so it is not generated (it stays hand-written).
 */
export interface ReadReceipts {
  tasks: Record<string, string>;     // taskId -> when it was last viewed (RFC3339 UTC)
  mailboxLastSeen: string | null;    // when the inbox as a whole was last viewed (badge freshness)
}

/**
 * One activity item. The wire part is the generated `ActivityItemDto`. `burstCount` is **client-only** (the UI flag
 * for showing a burst collapsed).
 */
export type ActivityItem = ActivityItemDto & {
  burstCount?: number;
};

export interface SmartView {
  id: string;
  label: string;
  icon: string;
  count?: number;
}
