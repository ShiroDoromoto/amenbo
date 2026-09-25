// The English dictionary — the one language that carries every string. That gives it two jobs
// beyond being a translation: a key missing from another language is rendered from here, and the
// key set itself is read off this file (see ../keys.ts). A new string starts here.
//
// Sections other than `ui` are keyed by a union that comes from Rust (a status, an error code, a
// doctor issue kind). The ones that must cover every case say so with `satisfies`, so when the core
// side adds a case this file stops type-checking until the English prose exists; `err` is partial
// by design (a code with no template falls through to the message the command returned). `ui` has
// no source outside this file — it *is* the source — so its keys are whatever is written here.
import type { ErrorCode } from "../../errorCodes";
import type { DoctorIssueKind } from "../../doctorKinds";
import type { Priority, Status } from "../../../mock/types";
import type { DoctorTemplate, ViewKind } from "../keys";

const status = {
  todo: "To do", in_progress: "In progress", done: "Done", blocked: "Blocked", rejected: "Rejected",
} satisfies Record<Status, string>;

const priority = {
  high: "High", medium: "Med", low: "Low",
} satisfies Record<Priority, string>;

const view = {
  list: "List", board: "Board", calendar: "Calendar", timeline: "Timeline",
} satisfies Record<ViewKind, string>;

// The UI chrome (fixed strings: nav, buttons, category headings). Keys read "area.name". Values
// carry no emoji — the JSX pairs an emoji with t(key), keeping decoration that does not depend on
// the language out of the dictionary.
const ui = {
  "topbar.refresh": "Refresh from source", "topbar.back": "Back", "topbar.forward": "Forward",
  "topbar.brandLink": "Open the product page",
  "side.smartViews": "Smart views", "side.projects": "Projects", "side.other": "Other",
  "side.newProject": "New project", "side.newProjectPh": "Project name",
  "side.archived": "Archived",
  "newproj.title": "New project", "newproj.nameLabel": "Name", "newproj.folderLabel": "Folder",
  "newproj.folderHint": "An AI launched in this folder is what operates this project, so one has to be chosen. It only places .amenbo and the AI guide; the folder's contents are never touched.",
  "newproj.chooseFolder": "Choose a folder", "newproj.changeFolder": "Change",
  "newproj.create": "Create", "newproj.cancel": "Cancel",
  "newproj.doneTitle": "Created project “{name}”",
  "newproj.doneCapability": "An AI launched in this folder can now operate this project.",
  "newproj.moreTitle": "Also",
  "newproj.copyStatus": "Copy {cmd} status", "newproj.copied": "Copied",
  "newproj.openTerminal": "Open in terminal", "newproj.openFinder": "Reveal in Finder",
  "newproj.openExplorer": "Reveal in Explorer", "newproj.openFileManager": "Reveal in the file manager",
  "newproj.openProject": "Open the project",
  "firstloop.title": "Your first loop",
  "firstloop.intro": "Ask your AI to register the first tasks.",
  "firstloop.start": "Start in the workspace",
  "firstloop.startHint": "It opens in the linked folder.",
  "firstloop.outside": "Start in your own terminal",
  "firstloop.outsideHint": "Any AI will do — launch yours and paste this text as it is.",
  "firstloop.copy": "Copy the request",
  "firstloop.copied": "Copied",
  "firstloop.appear": "The tasks it registers appear here",
  "firstloop.appearHint": "The moment your AI writes to Amenbo, the board you are looking at shows it.",
  "firstloop.prompt": "This folder is managed with Amenbo. Run {cmd} agent --json and follow it, managing the work in Amenbo as you go.",
  "noFolder.title": "This project has no folder linked",
  "noFolder.hint": "An AI launched in the linked folder can operate this project. The folder is what the workspace opens in, and where the request goes.",
  "noFolder.btn": "Link a folder",
  "projset.title": "Project settings", "projset.back": "Back to board",
  "projset.general": "General", "projset.nameLabel": "Name", "projset.notesLabel": "Notes",
  "projset.colorLabel": "Color", "projset.viewLabel": "Default view",
  "projset.agentLabel": "Agent", "projset.agentAuto": "Not set yet",
  "projset.iconLabel": "Icon", "projset.iconClear": "Remove image",
  "projset.iconHint": "Shown on the project tabs. Without one, the colour and the first letter of the name are shown.",
  "projset.save": "Save", "projset.saved": "Saved", "projset.saving": "Saving…",
  "projset.notesPh": "Notes for this project (optional)",
  "projset.danger": "Archive & delete", "projset.archivedBadge": "Archived",
  "projset.archive": "Archive", "projset.unarchive": "Unarchive",
  "projset.archiveHint": "Archiving removes it from the sidebar list (you can unarchive anytime).",
  "projset.delete": "Delete project",
  "projset.deleteHint": "Deletes the project and all of its tasks (use Archive if you want to keep them).",
  "projset.confirmArchive": "Archive project “{name}”?",
  "projset.confirmUnarchive": "Unarchive project “{name}”?",
  "projset.confirmDelete": "Delete project “{name}”? All of its tasks are deleted too.",
  "projset.folders": "Linked folders",
  "projset.foldersHint": "An AI launched in these folders can operate this project.",
  // Project settings > Notifications — what this project does with the device's shelf (`AMB-D-885`).
  // Nothing here names a connection: those are the device's, and its own settings word them.
  "projset.notify": "Notifications",
  "notify.projectSwitch": "Notifications for this project",
  "notify.on": "On",
  "notify.off": "Off",
  "notify.switchNote": "Off keeps the targets and the ticks below where they are.",
  "notify.targets": "Sent through",
  "notify.addTarget": "Add a target",
  "notify.dropTarget": "Stop sending through this one",
  "notify.targetsNote": "Chosen from what stands in Settings › Notification targets.",
  "notify.noneOnDevice": "This machine has no notification targets yet. One made here lands on the device's shelf, where every project picks from it.",
  "notify.events": "What it reports",
  "notify.eventsNote": "Only what was written to this project is reported. The last two are not writes — a day arrived, and you are told once that day.",
  "notify.mailTo": "Mail is addressed to",
  "notify.mailToPlaceholder": "Blank sends to the account the connection signs in as",
  "notify.mailToNote": "Shown while a mail target is among those above. Separate several with commas.",
  "notify.event.task.created": "A task was created",
  "notify.event.task.status_changed": "A task’s status changed",
  "notify.event.task.done": "A task was completed",
  "notify.event.task.rejected": "A task was decided against",
  "notify.event.task.assigned": "A task was assigned",
  "notify.event.task.moved": "A task moved to another project",
  "notify.event.task.deleted": "A task was deleted",
  "notify.event.decision.accepted": "A decision was written to the end",
  "notify.event.decision.rejected": "A decision was rejected",
  "notify.event.comment.added": "A comment was added",
  "notify.event.comment.removed": "A comment was taken back",
  "notify.event.task.due": "A task’s day has come (or gone)",
  "notify.event.task.due_tomorrow": "A task’s day is tomorrow",
  // Settings > Viewer — the phone that reads this store, and the server it reads from in the reader's
  // own Cloudflare account (`AMB-D-884`). The seven numbered items the plugin's form carried were a
  // procedure rather than settings; here the order is the layout. Three things are said out loud that
  // the old form never said: taking the read code away stops every phone at once, how many are reading
  // is not a number anybody has, and new keys make everything already on the server unreadable.
  "settings.viewer": "Viewer",
  "settings.viewerNote": "Read this store on a phone. One account, one key, one read code — this PC's, and not a project's.",
  "viewer.state": "Status",
  "viewer.syncing": "Syncing",
  "viewer.notSyncing": "Not syncing",
  "viewer.noServer": "No server on this PC yet",
  "viewer.noServerNote": "Nothing has been built in your Cloudflare account yet.",
  "viewer.paired": "A phone may read",
  "viewer.notPaired": "No phone may read",
  "viewer.pairingAsking": "Asking the server…",
  "viewer.lastPlaced": "Last sent {when}.",
  "viewer.neverPlaced": "Nothing has been sent yet.",
  "viewer.waiting.one": "{n} record waiting.",
  "viewer.waiting.other": "{n} records waiting.",
  "viewer.codeIssued": "The read code was issued {date}.",
  "viewer.sync": "Sync",
  "viewer.on": "On",
  "viewer.off": "Off",
  "viewer.syncNote": "Every project on this PC. There is no per-project choice. Off, nothing is sent and what is queued keeps.",
  "viewer.server": "Server and phones",
  "viewer.createServer": "Create the server on Cloudflare",
  "viewer.recreate": "Create the server again",
  "viewer.buildCurrent": "The server is running build {n}, which is the one this Amenbo carries.",
  "viewer.buildBehind": "The server is running build {have}. This Amenbo carries build {want} — create it again to move it up.",
  "viewer.buildUnknown": "Nothing has been sent to the server yet, so which build it is running is not known.",
  "viewer.createNote": "This builds a Worker and a database in your own Cloudflare account. Amenbo hosts none of it.",
  "viewer.recreateNote": "This builds over the server already in your account. The keys here are kept, so every phone paired to it keeps reading.",
  "viewer.token": "Cloudflare API token",
  "viewer.tokenNote": "Spent on this one press and saved nowhere. What stays behind cannot create anything in your account.",
  "viewer.tokenLink": "Create a token with the right permissions",
  "viewer.account": "Account ID",
  "viewer.accountPlaceholder": "Blank picks the only account",
  "viewer.accountNote": "Fill this in only if the token reaches more than one Cloudflare account.",
  "viewer.name": "Server name",
  "viewer.namePlaceholder": "Blank uses the usual name",
  "viewer.nameNote": "Name one to build a second server in the same account — or to say that a server already standing under the usual name is this Amenbo's.",
  "viewer.create": "Create",
  "viewer.back": "Back",
  "viewer.stoodKept": "The server is up. The keys already here were kept, so every phone paired to it still reads.",
  "viewer.stoodGenerated": "The server is up. New keys were drawn, so nothing already on the server can be read — pair every phone again.",
  "viewer.pairDevice": "Pairing code",
  "viewer.showQr": "Show the pairing code",
  "viewer.showQrAgain": "Show a new pairing code",
  "viewer.codeDrawn": "Read this with the Viewer app's camera.",
  "viewer.codeCarriesKey": "The code carries the key. Show it to a camera and to nothing else.",
  "viewer.codeReplaced": "Whatever phone held the code before this one has stopped reading.",
  "viewer.howMany": "Amenbo cannot tell how many phones are reading, or which.",
  "viewer.appNote": "A phone without the app installs it from here. Pairing a second phone is the same press as the first.",
  "viewer.unpair": "Stop every phone reading",
  "viewer.cutOff": "Take the read code away",
  "viewer.cutOffNote": "There is one read code, so every phone reading now stops. A single phone cannot be taken off.",
  "viewer.cutOffConfirm": "Take the read code away? Every phone paired to this store stops reading.",
  "viewer.cutOffDone": "The read code is gone, and every phone that held it has stopped reading.",
  "viewer.cutOffNothing": "The server was holding no read code, so no phone was reading.",
  "viewer.trouble": "When it is not keeping up",
  "viewer.send": "Send now",
  "viewer.sentPlaced": "Sent {placed}. {waiting} still waiting.",
  "viewer.sentAnotherTurn": "Another send is already running, so this one did nothing.",
  "viewer.sentSwitchedOff": "Sync is off, so nothing was sent. What is queued keeps.",
  "viewer.repair": "Compare with the server",
  "viewer.repairAgain": "Send the difference",
  "viewer.repairNote": "Compares what the server holds with what this PC holds. The first press counts the difference, the second sends it.",
  "viewer.repairLevel": "The server holds what this PC holds.",
  "viewer.repairCounted": "{place} to send and {drop} to drop. Press again to do it.",
  "viewer.repairPlaced": "Sent {placed}. {waiting} still waiting.",
  "viewer.repairElsewhere": "Another send is running, so nothing was compared. Try again once it is done.",
  "viewer.details": "The rest of it",
  "viewer.detailsWritten": "The server's address, the API token and the encryption key are written by the press above. There is nothing here to fill in.",
  "viewer.detailsKeys": "The key never leaves this PC. It is in no export and in no backup.",
  "projset.aiReady": "AI-ready", "projset.folderStale": "missing",
  "projset.addFolder": "Add folder", "projset.noFolders": "No linked folders yet.",
  "projset.unbind": "Unbind",
  "projset.confirmUnbind": "Unbind folder “{path}”? (removes .amenbo and managed blocks; the store is kept)",
  "projset.folderElsewhere": "This pointer came from somewhere else: the folder's .amenbo names project “{recorded}”, but #{projectId} is “{actual}”. Re-link to rewrite it.",
  "projset.folderNoSlug": "(no name)", "projset.relink": "Re-link",
  "projset.folderLegacyPointer": "This folder's .amenbo is in the old pre-migration format, so which project it names can't be read. Re-link to rewrite it in the current format.",
  "projset.folderNoPointer": "not linked", "projset.folderNoPointerHint": "This folder has no .amenbo, so an AI launched here does not resolve to this project. Re-link to write the pointer back.",
  "projset.folderOtherStore": "other store",
  "projset.folderOtherStoreHint": "This folder's .amenbo belongs to “{recorded}”, and this build is “{running}”. A command run there is refused, so an AI launched here does not reach this project. Re-link to hand the folder to “{running}”.",
  "projset.harness": "Starting your AI on Amenbo",
  "projset.harnessHint": "What this project answered when it was asked to have its AI run this work on Amenbo. Clearing the answer puts the notice back the next time you open this project.",
  "projset.harnessAnswer": "Answer",
  "projset.harnessYes": "Yes",
  "projset.harnessNo": "No",
  "projset.harnessUnanswered": "Not asked yet",
  "projset.harnessWaiting": "Folders here that still start their AI without Amenbo",
  "projset.harnessClear": "Clear the answer",
  "projset.harnessRequest": "The text for your AI",
  "projset.harnessRequestHint": "Pick the tool you work with and copy the text for it. It is here whether or not this project is wired already.",
  "projset.harnessRequestDirs": "Paste it into these folders:",
  "nav.settings": "Settings", "nav.onboarding": "Get started",
  "nav.decisions": "Decisions", "nav.mcp": "Connect via MCP", "nav.search": "Search",
  "dec.title": "Decisions",
  "dec.empty": "No decisions yet", "dec.new": "Record a decision", "dec.newTitlePh": "Decision title",
  "dec.newBodyPh": "Conclusion + rationale (don't paste raw discussion)", "dec.add": "Record", "dec.cancel": "Cancel",
  "dec.editTitlePh": "Rewrite the title", "dec.editBodyPh": "Rewrite the conclusion and rationale",
  "dec.accept": "Finish writing", "dec.reject": "Reject", "dec.reopen": "Return to draft",
  "dec.editAcceptedHint": "Editing a decision already decided is a wording fix, not a re-decision (the decided date does not change). To overturn it, supersede it with a new decision via “Link a decision” below.",
  "dec.status.draft": "Draft", "dec.status.decided": "Decided", "dec.status.rejected": "Rejected",
  "dec.filterSuperseded": "Superseded",
  "dec.supersededByRef": "superseded by {by}",
  "dec.supersedes": "Supersedes", "dec.supersededBy": "Superseded by", "dec.amends": "Amends", "dec.amendedBy": "Amended by", "dec.linkedTasks": "Linked tasks",
  "dec.buildsOn": "Builds on", "dec.builtOnBy": "Built on by",
  "dec.premiseStale": "This premise {premise} was superseded by {by} (worth a review)",
  "dec.edge.add": "Link a decision", "dec.edge.cancel": "Cancel", "dec.edge.unlink": "Unlink",
  "dec.edge.unlinkConfirm": "Remove the link to {target}. The decision itself is not undone. Continue?",
  "dec.edge.kind.supersedes": "Supersedes it", "dec.edge.kind.amends": "Amends it", "dec.edge.kind.buildsOn": "Builds on it",
  "dec.edge.supersedeRevisitConfirm": "These decisions stand on {target} — revisit them if you supersede it:\n{list}\n\nSupersede anyway?",
  "dec.edge.searchPh": "Search decisions to link (AMB-D-<n>, title)",
  "dec.edge.noCandidates": "No decision left to link",
  "dec.recorded": "Recorded", "dec.decided": "Decided", "dec.lastChanged": "Last changed",
  "dec.notFound": "Decision not found",
  "dec.unknownName": "(unknown)",
  "dec.comments": "Discussion", "dec.reasonPh": "Reason (optional, Markdown)…",
  "dec.revisit": "These decisions stand on this one — revisit them if you reject it:",
  "dec.searchFailed": "The search could not run",
  "dec.searchPh": "Search title / body / comments / AMB-D-<n> / <n>",
  "dec.sort": "Sort",
  "dec.sort.numberDesc": "Number, newest first", "dec.sort.numberAsc": "Number, oldest first",
  "dec.sort.decidedDesc": "Decided, newest first", "dec.sort.decidedAsc": "Decided, oldest first",
  "board.filters": "Filters",
  "filter.dim.status": "Status", "filter.dim.assignee": "Assignee", "filter.dim.priority": "Priority",
  "filter.opt.assignee.none": "Unassigned", "filter.opt.assignee.me": "Me", "filter.opt.assignee.meAi": "My AI",
  "board.searchPh": "Search tasks / AMB-T-<n> / <n>",
  "board.searchFailed": "The search could not run",
  "board.addDimension": "Category", "board.dimensionNamePh": "Category name (Enter to add)",
  "board.manageDimensions": "Manage categories",
  "dimmgr.title": "Manage categories", "dimmgr.close": "Close",
  "dimmgr.empty": "No categories yet. Add one to split columns by its values.",
  "dimmgr.namePh": "Category name", "dimmgr.notesPh": "Description (optional)",
  "dimmgr.values": "Values", "dimmgr.valueNamePh": "Value name",
  "dimmgr.addValue": "＋ Value", "dimmgr.addDimension": "＋ Add category",
  "dimmgr.removeDim": "Delete category", "dimmgr.removeValue": "Delete value",
  "dimmgr.ordered": "Ordered", "dimmgr.orderedHint": "Give the values an order so you can reorder them below",
  "dimmgr.multi": "Multi-select",
  "dimmgr.multiHint": "Let one task or decision answer this category with several values at once. Going back to one is refused while anything still answers with several, and a multi-select category cannot also be the time axis",
  "dimmgr.timeAxis": "Time axis",
  "dimmgr.timeAxisHint": "Make this category the project's time axis: its values carry periods, and the one covering today is marked current",
  "dimmgr.closable": "Closable",
  "dimmgr.closableHint": "Let this category's values be closed instead of deleted: a closed value keeps everything already filed under it and still works in filters, and only stops taking new records. A category holds one role, so this and the time axis exclude each other",
  "dimmgr.closeValue": "Close",
  "dimmgr.reopenValue": "Reopen",
  "dimmgr.closeValueHint": "Close this value. What already carries it keeps it and filters still find it — only new assignments are turned away, and reopening is always allowed",
  "dimmgr.showClosed.one": "Show {n} closed value", "dimmgr.showClosed.other": "Show {n} closed values",
  "dimmgr.hideClosed": "Hide closed values",
  "dimmgr.showOnCard": "Show on card",
  "dimmgr.showOnCardHint": "Put this category on the task card. The category carries the answer, so it changes for everyone — not just on this device",
  "dimmgr.required": "Required",
  "dimmgr.requiredHint": "Hold a task's creation until this category is answered. Tasks already created stay as they are",
  "dimmgr.requiredNoValuesHint": "Add a value first, or reopen a closed one — a category offering nothing open could never be answered",
  "dimmgr.appliesTo": "Applies to",
  "dimmgr.appliesToHint": "Which of the two this category is offered on. The category carries the answer, so it changes for everyone. Narrowing it takes nothing already answered away — it just stops meaning anything there",
  "dimmgr.appliesTo.both": "Tasks and decisions",
  "dimmgr.appliesTo.task": "Tasks",
  "dimmgr.appliesTo.decision": "Decisions",
  "dimmgr.slug": "Category key", "dimmgr.valueSlug": "Value key",
  "dimmgr.slugPh": "key",
  "dimmgr.slugHint": "The readable key this answers to outside Amenbo. Lower-case letters, digits and hyphens, starting with a letter. Unique, and never empty.",
  "dimmgr.moveUp": "Move up", "dimmgr.moveDown": "Move down",
  "dimmgr.periodStart": "Start date", "dimmgr.periodEnd": "End date",
  "dimmgr.periodStartOpen": "No start date", "dimmgr.periodEndOpen": "Ongoing",
  "dimmgr.current": "Current", "dimmgr.currentHint": "Today falls inside this period",
  "dimmgr.confirmRemoveDim": "Delete the category \"{name}\"? Its values and task assignments are removed too.",
  "dimmgr.confirmRemoveValue": "Delete the value \"{name}\"? Task assignments to this value are removed too.",
  "dimmgr.confirmRemoveValueMoving": "Delete the value \"{name}\"? The tasks answering with it move to \"{to}\".",
  "dimmgr.reassignTo": "Move its tasks to",
  "dimmgr.reassignPick": "Choose a value",
  "dimmgr.lastOpenValueHint": "A required category keeps the last value it still offers — turn off Required first",
  "dimmgr.cancel": "Cancel",
  "board.seeClosedInList": "See all {n} closed in a list",
  "board.rejectedCount": "{n} rejected",
  "board.seeMoreInList": "See {n} more in a list",
  "board.notFound": "Project not found",
  "cal.today": "Today", "cal.prevMonth": "Previous month", "cal.nextMonth": "Next month",
  "cal.noDue": "No due date ({n})", "cal.empty": "No tasks with a due date",
  "cal.more": "+{n} more", "cal.overdueDays": "{n}d overdue", "cal.inDays": "in {n}d",
  // The two days a task carries, as the fields that write them. One area rather than two, because the
  // detail pane and the compose pane put the same two fields in front of the same person.
  "date.due": "Due date", "date.start": "Start date",
  "date.clear": "Clear",
  "card.assignee": "Assignee",
  "detail.tab.detail": "Details", "detail.tab.activity": "Activity",
  "detail.notFound": "Task not found",
  "detail.unassign": "Unassign", "detail.assignAi": "Delegate to AI",
  "detail.assignee": "Assignee", "detail.unassigned": "Unassigned",
  "detail.project": "Project", "detail.none": "None",
  "detail.blockedBy": "Waiting on", "detail.blockedByHint": "blocked (dependency)",
  "detail.notStarted": "Starts on",
  // The fourth premise, and the move that ends it. Never "publish" or "approve": the one who created the
  // task is the one who ends the creation, and there is nobody to ask (`AMB-D-558`).
  "detail.draft": "Creation", "detail.finishCreating": "Finish creating",
  "detail.finishCreatingBlocked": "Fill in {names} first",
  "detail.linkedDecisions": "Motivated by",
  "detail.premiseUnsettled": "premise not settled — wait for it to be written to the end, or unlink it (cannot reserve)",
  "detail.priority": "Priority", "detail.priorityNone": "None",
  "detail.notes": "Notes (Markdown)", "detail.edit": "Edit", "detail.add": "Add", "detail.dimUnset": "Remove this value",
  "detail.notesPh": "Write notes in Markdown…", "detail.notesHint": "Markdown · save with ⌘/Ctrl+Enter · Esc to cancel",
  "detail.cancel": "Cancel", "detail.save": "Save", "detail.noNotes": "No notes yet",
  "detail.activityCategory": "Activity", "detail.noComments": "No comments yet",
  "detail.noActivity": "No activity yet",
  "detail.commentPh": "Write a comment… (Markdown, Enter for newline)",
  "detail.commentHint": "Markdown · send with ⌘/Ctrl+Enter", "detail.send": "Send",
  "detail.created": "Created", "detail.deleteNoUndo": "deletion cannot be undone",
  "detail.updated": "Updated", "detail.updatedHint": "any write moves this — a comment, a date, a title fix — not just a status change",
  "detail.delete": "Delete", "detail.deleteTip": "Delete this task (cannot be undone)",
  "detail.deleteConfirm": "Delete “{title}”?",
  "comment.edit": "Edit this comment",
  "comment.edited": "edited",
  "comment.quoted": "“{text}”",
  "comment.remove": "Delete this comment",
  "comment.removeConfirm": "Delete this comment? Its attachments go with it, and this cannot be undone.",
  "attach.section": "Attachments", "attach.add": "Attach file", "attach.none": "No attachments",
  "attach.dropHint": "Drop files here, or", "attach.dropActive": "Drop to attach",
  "attach.download": "Download", "attach.remove": "Remove",
  "attach.removeConfirm": "Remove attachment “{name}”?", "attach.notLocal": "Not stored on this device yet (fetch coming later)",
  "attach.unsupported": "Preview not supported for this type", "attach.link": "Link",
  "attach.failed": "Attachment failed",
  "madeIn.section": "Made in", "madeIn.unnamed": "An unnamed pane",
  "madeIn.go": "Go back into this pane",
  "commit.section": "Commits", "commit.add": "Record SHA", "commit.none": "No commits recorded",
  "commit.placeholder": "Commit SHA (full — 40 or 64 hex digits)",
  "commit.record": "Record", "commit.copy": "Copy SHA", "commit.copied": "Copied",
  "commit.remove": "Remove", "commit.removeConfirm": "Remove commit {sha}?",
  "compose.new": "New task", "compose.titlePh": "Title",
  "compose.notes": "Notes (Markdown, optional)", "compose.notesPh": "Write notes in Markdown… (optional)",
  "compose.hint": "Enter to create · Esc to cancel", "compose.cancel": "Cancel", "compose.create": "Create",
  // smart views (sidebar shows inbox/activity/due; archive is the header for the board-opened list)
  "smartview.inbox": "Inbox @me", "smartview.activity": "Activity",
  "smartview.due": "Due",
  "smartview.automations": "Automations",
  "smartview.dueStop": "Overdue or due today", "smartview.dueHeed": "Due tomorrow",
  "mailbox.notifyTitle": "Amenbo inbox", "mailbox.notifyBody.one": "{n} item needs your attention", "mailbox.notifyBody.other": "{n} items need your attention",
  "mailbox.notifyFailed": "Couldn't show an OS notification (allow Amenbo notifications in system settings)",
  "pager.range": "{from}–{to} of {total}", "pager.page": "page {page}/{pages}",
  // members screen
  // settings screen
  "settings.profile": "Profile", "settings.avatar": "Avatar",
  "settings.facetNames": "Display names (Human / AI)", "settings.facetNamesSave": "Save",
  "settings.humanNameLabel": "Human display name", "settings.aiNameLabel": "AI display name",
  "settings.facetNamesHint": "Changes the two roster names (Human / AI) (a blank field leaves that facet unchanged).",
  "settings.avatarChoose": "Choose image…", "settings.avatarReset": "Reset to identicon",
  "settings.avatarHint": "Register a face for Human and AI. The display version is downscaled to 96px, and the image you picked is kept as it is (a per-facet identicon is used when unset).",
  "settings.avatarImageFailed": "The image could not be loaded",
  "settings.avatarCanvasFailed": "The canvas could not be initialized",
  "settings.appearance": "Appearance", "settings.theme": "Theme", "settings.language": "Language",
  "settings.defaultView": "Default view",
  "settings.defaultViewNote": "The view a new project opens in. Projects that already exist keep the view they have.",
  "settings.themeOs": "Follow OS", "settings.themeDark": "Dark", "settings.themeLight": "Light",
  "settings.skin": "Skin",
  "settings.skinNone": "None (the colours Amenbo ships with)",
  "settings.skinUnreadable": "this file cannot be read",
  "settings.skinHow": "A skin is one file: a zip holding `skin.yaml` and its materials. Take one in with `amenbo skin add <file>` in a terminal.",
  "settings.skinTryOn": "Trying it on",
  "settings.skinTryOnSample": "The quick brown fox jumps over the lazy dog.",
  "settings.skinApply": "Use this one",
  "settings.skinKeep": "Keep what I had",
  "settings.skinThemePinned": "{title} was made for {side} only. To change the theme, take the skin off.",
  "settings.skinTakeOff": "Take it off",
  "settings.skinAdd": "Add a skin",
  "settings.skinAddPick": "Choose a file…",
  "settings.skinAddDrop": "or drop a .zip here",
  "settings.skinWriteOut": "Write one out",
  "settings.skinWriteOutNote": "Everything a skin can set, each with a line saying what it is for. Taken from the skin that is on.",
  "settings.skinWriteKept": "Hand this one on",
  "settings.skinWriteKeptNote": "The file it arrived as, byte for byte — materials and all.",
  "settings.skinAddTake": "Take it in",
  "settings.skinOnlySide": "{side} only",
  "settings.skinReplace": "A skin is already kept under this name: {held} is there, {coming} is coming in.",
  "settings.skinNoVersion": "no version",
  "settings.skinCarries": "In this file:",
  "settings.skinCarriedOther": "not a kind this build draws",
  "settings.skinCarriedUnnamed": "nothing in the skin names it",
  "settings.skinDropped": "Dropped:",
  "settings.skinWarnUnknown": "not a key this Amenbo has",
  "settings.skinWarnClosed": "not a skin’s to move",
  "settings.skinWarnNotText": "the value is not text",
  "settings.skinUnmeasured": "this build cannot read a number from it",
  "settings.skinCovered": "a picture is laid over it, so nothing on it was measured",
  "settings.skinContrastClear": "All {n} pairings clear their floor.",
  "settings.skinContrastShort": "{n} of {m} pairings are under their floor. You can take it in anyway.",
  "settings.skinFontMissing": "This font has no letters for: {langs}. Those read in the machine’s own face.",
  "settings.skinFont": "Font: {family} ({license})",
  "settings.skinFontLicence": "Licence in full",
  "settings.developer": "Developer",
  "settings.perfLog": "Perf log (instrumentation)",
  "settings.perfLogNote": "Times the read/write layers and WARNs on a budget bust (core to a rolling file, front to the console). Applies live.",
  "settings.startup": "Startup",
  "settings.autostart": "Start at login",
  "settings.autostartOn": "On",
  "settings.autostartOff": "Off",
  "settings.autostartNote": "Opens Amenbo when you sign in to this computer, exactly as opening it yourself does. What arrived in your inbox while it was closed is gathered up the next time it opens, so being up earlier is noticing earlier.",
  // The hourly tick, on the settings screen. The band says why the timer is wanted (`tickBanner.*`);
  // this says where things stand and how to change them.
  "settings.dueWarning": "Due warnings",
  "settings.tick": "Hourly check",
  "settings.tickOn": "On",
  "settings.tickOff": "Off",
  "settings.tickNote": "On, this computer's scheduler wakes Amenbo once an hour to look for tasks whose day is near, and the warning goes out to the notification targets the project has chosen. Off takes that registration away, and nothing of ours is left running.",
  "settings.tickRowRemains": "macOS keeps its own record of the row, so it stays in your login items — with nothing behind it.",
  "nudge.autostart.title": "Open Amenbo when you sign in?",
  "nudge.autostart.yes": "Yes, open it at login (recommended)",
  "nudge.autostart.no": "No thanks",
  "nudge.autostart.hint": "Settings › Startup switches it back whenever you like.",
  // The file panel's own question, and the only place it can be turned back on: the checkbox
  // that turns it off is drawn inside the question it silences (`AMB-D-777`).
  "settings.files": "Files",
  "settings.trashAsk": "Ask before binning",
  "settings.trashAskOn": "Ask",
  "settings.trashAskOff": "Do not ask",
  "settings.trashAskNote": "The file panel asks before it moves a row to the bin. Turning the question off inside it is one-way; this is the way back.",
  "settings.updates": "Updates",
  "settings.updateCheck": "Update check",
  "settings.updateCheckOn": "On",
  "settings.updateCheckOff": "Off",
  "settings.updateCheckNote": "Checks whether a newer release has been published (infra-side traffic only — no user data, timeout, failures ignored, about once a day). Turn off to skip the check.",
  "settings.perfLogOff": "Off",
  "settings.perfLogBudget": "Budget busts only",
  "settings.perfLogVerbose": "Verbose (all events)",
  // Settings > Notification targets — the device's shelf (`AMB-D-885`). What a project does with the
  // shelf is worded on the project's own screen; nothing here names a project.
  "settings.notifyTargets": "Notification targets",
  "settings.notifyTargetsNote": "A connection written here once, under a name. Projects select from this shelf rather than holding a connection of their own, so a webhook that changes is one edit.",
  "notify.kind.slack": "Slack",
  "notify.kind.mail": "Email",
  "notify.default": "Default",
  "notify.makeDefault": "Make this the default",
  "notify.defaultNote": "A new project starts out pointing at the default. Projects that already exist keep the target they chose.",
  "notify.empty": "No notification targets yet.",
  "notify.add": "Add a notification target",
  "notify.edit": "Edit",
  "notify.name": "Name",
  "notify.nameNote": "A project's settings offers this target under this name.",
  "notify.webhook": "Webhook URL",
  "notify.webhookSet": "Webhook saved",
  "notify.webhookUnset": "No webhook yet",
  "notify.mailUnset": "No server yet",
  "notify.smtpHost": "SMTP server",
  "notify.smtpPort": "Port",
  "notify.smtpUser": "Account",
  "notify.smtpPassword": "Password",
  "notify.appPassword": "Gmail refuses your own password and asks for an app password instead.",
  "notify.appPasswordLink": "Create a Gmail app password",
  "notify.mailFrom": "From",
  "notify.mailFromPlaceholder": "Blank sends from the account",
  "notify.keepSecret": "One is saved. Leave this blank to keep it.",
  "notify.needSecret": "Nothing is saved yet, so this target cannot send.",
  "notify.save": "Save",
  "notify.cancel": "Back",
  "notify.saved": "Saved",
  "notify.delete": "Delete",
  "notify.usedBy": "Projects sending through this: {n}",
  "notify.deleteLoses": "Deleting it takes the connection and what it holds, and every project sending through it loses it.",
  // The two presses that ask whether a connection works, and what each can honestly claim afterwards.
  // A mail relay is spoken to and the account offered to it; a Slack webhook has no door but posting, so
  // only the shape of its URL is read — and a webhook revoked yesterday still has it.
  "notify.check": "Check the connection",
  "notify.test": "Send a test message",
  "notify.checkReached": "The server accepted the account.",
  "notify.checkShape": "The URL has the shape of a Slack incoming webhook. Whether it still works is what a test message answers.",
  "notify.testSentSlack": "It went out — look for it in the channel.",
  "notify.testSentMail": "It went out — look for it in the mailbox this account reads.",
  "notify.deleteConfirm": "Delete “{name}”? Every project sending through it loses it.",
  "settings.data": "Data", "settings.dataPath": "Location",
  "settings.logs": "Logs",
  "settings.logsOpen": "Open the logs folder",
  "settings.logsNote": "Reporting a bug? Open this folder and attach what is in it (the diagnostic log, plus the perf log if you turned it on). No task or decision content is written to either.",
  "settings.exportImport": "Export",
  "settings.exportJson": "Export",
  "settings.dataNote": "Your data is stored locally on your device and never locked into a proprietary binary. Export writes everything (all projects) out into one folder — for migrating to other tools, one way: portable JSON (export.json) plus the attachment files themselves (attachments/, laid out under the task or decision they hang on). The way back into Amenbo is restoring a backup.",
  "settings.exportDialogTitle": "Export to (a folder is created)",
  "settings.exportDone": "Exported ({kb} KB, {attachments} attachment(s))",
  "settings.exportMissing": " · {missing} attachment(s) had no file left to take",
  "settings.transferCancelled": "Cancelled.",
  "settings.backup": "Backup & restore",
  "settings.backupBtn": "Back up everything",
  "settings.restoreBtn": "Restore from backup",
  "settings.backupDialogTitle": "Save backup as",
  "settings.restoreDialogTitle": "Choose a backup to restore",
  "settings.backupDone": "Backup saved ({kb} KB)",
  "settings.restoreDone": "Restored ({attachments} attachment(s) and {skins} skin(s) written)",
  "settings.restoreAside": "The previous state was set aside at {path} (restore from there to undo this).",
  "settings.restoreSwept.one": "Only the latest set-aside store can be rewound to, so {n} earlier one was removed.", "settings.restoreSwept.other": "Only the latest set-aside store can be rewound to, so {n} earlier ones were removed.",
  "settings.restoreMigrated": "The archive was not left in the shape it was backed up in — it was brought forward to this version (format v{from} → v{to}: {steps}).",
  "settings.restoreConfirm": "This replaces this device's data with the chosen backup. It's reversible — the current state is set aside with a timestamp first. Continue?",
  "settings.backupNote": "Writes this device's data (every project) — attachment files and all — to one verified file (no keys included). Restore is destructive but sets the current state aside first. If you keep a backup off this machine (e.g. iCloud), the file is plaintext — trusting that destination is your responsibility (it relies on the cloud's own encryption and account authentication).",
  "settings.integrity": "Integrity",
  "settings.doctor": "Check and repair problems",
  "settings.doctorNote": "Checks inside the store (orphan references and the like) and this device's bound folders (.amenbo / AI guidance). The check itself writes nothing. Same checks and same repairs as `amenbo doctor` on the CLI.",
  "settings.doctorChecking": "Checking…",
  "settings.doctorRecheck": "Check",
  "settings.doctorClean": "No problems found.",
  "settings.doctorFound": "{errors} error(s) / {warnings} warning(s)",
  "settings.doctorFix": "Sweep unreferenced files and leftover bindings",
  "settings.doctorMore": "… and {count} more",
  "settings.doctorNoneRepairable": "None of the problems above is fixed by this sweep.",
  "settings.doctorFixing": "Repairing…",
  "settings.doctorFixNote": "The sweep touches two things and no others: attachment files nothing references any more, and folder bindings no project claims. It does not fix the problems listed above. Bound-folder problems get their own button on the lines whose fix is unambiguous (for the rest, pick the project in Project settings > Folders).",
  "settings.doctorRebind": "Re-bind",
  "settings.doctorRepairing": "Working…",
  "settings.doctorRepairDone": "Fixed.",
  "settings.doctorFixDone": "Repaired ({attachments} attachment row(s) · {blobs} attachment file(s) · {bindings} folder binding(s))",
  "settings.doctorFixNothing": "Nothing to repair.",
  "settings.dataOpPreparing": "Preparing…",
  "settings.dataOpProgress": "[{done}/{total}] {phase}",
  "settings.dataOpProgressUnbounded": "[{done}] {phase}",
  "settings.dataOpCancel": "Cancel",
  "settings.dataOpCancelling": "Cancelling…",
  "settings.dataOpPhase.snapshotting": "Snapshotting",
  "settings.dataOpPhase.blobs": "Attachments",
  "settings.dataOpPhase.unpacking": "Unpacking",
  "settings.dataOpPhase.verifying": "Verifying",
  "settings.dataOpPhase.copying": "Writing",
  "settings.dataOpPhase.exporting": "Exporting",
  "settings.dataOpPhase.migrating": "Updating",
  "restart.title": "Amenbo has been updated",
  "restart.intro": "Another process updated the store to a newer format. This window is the old Amenbo, still in memory. What you see is already stale, and it will not refresh again.",
  "restart.how": "Restarting reopens it with the new Amenbo already on disk (the GUI and the CLI ship together).",
  "restart.button": "Restart",
  "restart.failed": "Could not restart. Quit Amenbo and open it again.",
  "restart.confirm": "Restart Amenbo? Every terminal open in it ends. The conversations come back on the next run; what they were running does not.",
  // Restarting ends every terminal the same way quitting does, so it says the same thing about the
  // panes that will not be in their conversation afterwards (`quit.confirmNotAll`).
  "restart.confirmNotAll": "Restart Amenbo? Every terminal open in it ends. The conversations come back on the next run, apart from the panes running {names}. What they were running does not come back either.",
  "restart.stuck.title": "If restarting does not help",
  "restart.stuck.intro": "Then the Amenbo on disk is still the old one. There is no downgrade — the way back is the pre-migration backup the update left behind.",
  "restart.stuck.how": "Either install the newer version (the GUI and the CLI ship together), or restore from that backup on the command line:",
  "restart.stuck.command": "{cmd} restore <pre-migration backup (.amenbo-backup)>",
  "restart.stuck.where": "The pre-migration backup sits in the same folder as this device's store, named starting with pre-migrate-.",
  "migrate.title": "Updating your data",
  "migrate.intro": "Carrying this device's store from format v{from} to v{to} ({steps} step(s)). A whole pre-migration backup is taken first.",
  "migrate.preparing": "Getting ready to update this device's store. If another process (the command line) got here first, this waits for it to finish.",
  "migrate.space": "The pre-migration backup needs ~{required} MiB (archive ~{archive} MiB + staging ~{staging} MiB). ~{free} MiB is free.",
  "migrate.safety": "Do not quit Amenbo until this is done. If it fails, the store is put back exactly as it was.",
  "migrate.doneTitle": "Your data has been updated",
  "migrate.doneIntro": "The store is now at format v{version}.",
  "migrate.backupTo": "The store as it was",
  "migrate.superseded.one": "Removed {n} older pre-migration backup nothing can go back to (only the newest one is a way back).", "migrate.superseded.other": "Removed {n} older pre-migration backups nothing can go back to (only the newest one is a way back).",
  "migrate.olderBuilds": "Older versions of Amenbo can no longer open this store (the GUI and the CLI ship together).",
  "migrate.continue": "Open Amenbo",
  "migrate.failedTitle": "The update failed",
  "migrate.retry": "Try again",
  // activity screen
  "activity.filterKind": "Kind", "activity.filterAll": "All",
  "activity.filterSystem": "System", "activity.filterComment": "Comments",
  "activity.filterFacet": "By", "activity.filterHuman": "Human", "activity.filterAi": "AI",
  "activity.note": "Humans and AI read the same stream (AI uses activity --json)",
  "activity.today": "Today", "activity.reply": "Reply",
  // search screen (where a word is written, across tasks, decisions and the comments on either)
  "search.placeholder": "Words to look for", "search.run": "Search",
  // The narrowing box, in the vocabulary of whichever side is picked (`AMB-D-563`) — the same key can
  // mean different things on the two, so the hint is the side's own. Off with no side picked: there
  // would be no grammar to read the expression in, and the box says which move turns it back on.
  "search.filterPh.task": "status:todo …", "search.filterPh.decision": "status:decided …",
  "search.filterPhOff": "Pick a kind first",
  // The two knobs the screen narrows by, each labelling its own axis (`AMB-D-562`). "In" used to head one
  // row holding both, which is what made a comment look like a third kind of record.
  "search.kind": "Kind", "search.kindAll": "Both",
  "search.kind.task": "Tasks", "search.kind.decision": "Decisions",
  "search.faceAxis": "Where", "search.faceAll": "Anywhere",
  // The scope, drawn as a pull-down and kept out of the box beside it (`AMB-D-564`).
  "search.project": "Project", "search.projectAll": "Every project",
  "search.note": "Where a word is written",
  "search.idle": "Type the words you are looking for.",
  "search.empty": "Nothing is written with those words.",
  "search.failed": "The search could not run:",
  "search.face.title": "Title", "search.face.body": "Body", "search.face.comment": "Comment",
  // A label hit is the name of a category or of one of its values, which is what the GUI calls a dimension.
  "search.face.label": "Category", "search.face.attachment": "Attachment",
  // What the words were found ON — the record, or a comment on it, on either side. The other axis from
  // `search.face.*`, and singular where the `search.kind.*` chips are plural: a chip narrows a set, this
  // names one hit.
  "search.on.task": "Task", "search.on.taskComment": "Comment on a task",
  "search.on.decision": "Decision", "search.on.decisionComment": "Comment on a decision",
  // The one line a screen shows in place of a command, where nothing on the machine reaches this
  // build's CLI yet (a Linux preview before its member installs it; see components/NoCli.tsx).
  "cli.none": "No command is on your PATH yet. This preview ships its CLI as a file beside the app — copy it into ~/.local/bin (or anywhere on your PATH) and it becomes the command to type.",
  "common.listSeparator": ", ",
  // One activity line per event kind. The backend sends the kind and the values, never the
  // sentence, so these are the only place a timeline line is worded — under Tauri and in the
  // browser fallback alike (see eventText in ../index.ts).
  "common.you": "You",
  "act.created": "Created “{title}”",
  "act.statusChanged": "Changed “{title}” to {status}",
  "act.assigned": "Assigned “{title}”", "act.assignedAi": "Delegated “{title}” to AI",
  "act.unassigned": "Unassigned “{title}”",
  "act.moved": "Moved “{title}”", "act.unblocked": "“{title}” is now unblocked (ready)",
  "act.proposed": "Recorded “{title}” as a draft",
  "act.decided": "Decided “{title}”",
  "act.rejected": "Rejected “{title}”",
  "act.deleted": "Deleted “{title}”",
  "act.deletedWith": "Deleted “{title}” ({tasks}, {decisions})",
  "act.updated": "Updated “{title}”",
  "act.nTasks.one": "{n} task", "act.nTasks.other": "{n} tasks",
  "act.nDecisions.one": "{n} decision", "act.nDecisions.other": "{n} decisions",
  // The stand-in for a target whose name is past recovering: it was deleted, and the ledger row
  // that carried the name is gone too. The backend sends an empty title and this fills the gap.
  "act.nameless": "(deleted)",
  // Relative time and the due chip are not written here: "3 days ago" and "tomorrow" are `Intl`'s
  // to word, from the bare timestamp the backend sends (see core/i18n/format).
  // app / shell chrome
  "app.loadError": "Failed to load data.", "app.loading": "Loading…",
  // The talk window names itself from here — `tauri.conf.json` can hold only one fixed string,
  // and two windows both called "Amenbo" cannot be told apart in a window list (`AMB-T-3588`).
  // `{face}` is the name of the face the window was split out with, taken from the same key the
  // rest of the UI calls that face by (`face.workspace`), so the window and the button that opened
  // it never say two different words for one thing.
  "app.talkWindow": "Amenbo — {face}",
  // talk window: the folder a pane opens in (`AMB-T-3606`), what it opens with — an agent, or the
  // folder's own shell (`AMB-T-3646`) — and the row a closed frame offers (`AMB-T-3591`).
  // `{commands}` is the list of program names that were looked for on the PATH. `talk.folder` says
  // what the folder is rather than what the agent will do in it: what a given tool asks before it
  // acts is not Amenbo's to promise (`AMB-D-749`).
  "talk.folder": "Choose the folder you show the AI. What you put there is all it can see.",
  "talk.chooseFolder": "Choose a folder",
  "talk.searching": "Looking for the agents on this machine…",
  "talk.ask": "Which agent do you work with in this project?",
  "talk.none": "No agent Amenbo can start was found on this machine.",
  "talk.noneHint": "Amenbo looked for these commands on your shell's PATH: {commands}. Install one, then search again.",
  "talk.retry": "Search again",
  "talk.open": "Open",
  "talk.startWith": "Open with",
  // The pane's own prompt, with nothing started at it. It stands wherever the frame puts a choice,
  // so `talk.startWith` names what the row does rather than naming agents alone.
  "talk.shell": "Plain shell",
  // The one line above a pane of the talk window: how long it has been quiet, when there is nothing
  // the agent said to put there instead. "{n}" is a count of whole minutes.
  // The sentence Amenbo opens an agent with, left sitting in the pane's input box. It names neither
  // the agent's product nor anything its screen says: what a program calls its input box is the
  // program's business and changes under us (`AMB-D-805`).
  // The two faces of the one window, and the moves between one window and two (`AMB-D-753`).
  "face.switch": "Tasks or workspace",
  "face.tasks": "Tasks",
  "face.workspace": "Workspace",
  "face.splitOut": "Open in a separate window",
  "face.merge": "Back to one window",
  "face.opening": "Opening the workspace in a window of its own…",
  // The one control a pane has: it takes the place away, and the terminal in it with it
  // (`app/src/shell/TerminalPane.tsx`). It asks first — nothing brings the frame back, and what a
  // program exits with is on the screen to be read (`AMB-T-3666`). **This is the heavier of the two
  // questions now**: the handle a conversation is resumed from is held against the place, so
  // removing the place is what closes the way back into it for good (`AMB-D-869`).
  "face.drop": "Remove this pane",
  "face.dropConfirm": "Remove this pane? The terminal in it ends, and the way back into what was said here goes with the place.",
  // The pane an automation run is drawn in (`app/src/talk/nameplate.ts`). The mark beside the name
  // says this pane is a run's, and the line under it says where the run has got to: which step is
  // running, how many moves in that is, which run it is, how many tasks in the run is, and the task
  // it is working. Every one is a value Amenbo holds, never the agent's word about itself
  // (`AMB-D-858`).
  //
  // **And the way out is the other act.** There is no conversation to lose here — a step opens one of
  // its own and leaves nothing of it behind — so what the question is about is the run: it stops, the
  // task it reserved goes back, and a line on that task says so (`AMB-T-5252`).
  "face.auto": "Automated",
  "face.runStep": "Step {n}",
  "face.runNo": "Run {n}",
  "face.runTask": "Task {n}",
  "face.dropRunLive": "Can't be removed while the run is going",
  "face.runStoppedAt": "Stopped at {exit}",
  // The way out of the whole app, which ends every terminal at once and is asked about for the same
  // reason one pane is (`app/src/shell/openPanes.ts`, `crate::quit`). It is its own sentence rather
  // than the pane's: what is being left behind is every session in the process, and one about "this
  // pane" would name the wrong thing at the moment it matters most. It says a terminal is going and
  // nothing about what any of them was doing — that was a key the world could rewrite behind the
  // pane, and it is gone (`AMB-D-858`).
  //
  // **What it warns about is the running and not the talk** (`AMB-D-869`). The conversations come
  // back on the next run, so naming them here would be asking about something nobody is losing;
  // what does not come back is whatever a pane was in the middle of doing. The question stays
  // because that is a real loss — and because one of the six writes a standing approval down as a
  // refusal when it is killed (`AMB-T-4630`).
  "quit.confirm": "Quit Amenbo? Every terminal open in it ends. The conversations come back on the next run; what they were running does not.",
  // The same question where a pane on the screen will **not** be in its conversation on the next run
  // (`AMB-T-4676`). Which panes those are is counted rather than written down here — a provider with
  // no way back, or a place whose way in was taken back (`crate::frames::TalkFace::without_a_way_back`)
  // — so this stays true as providers gain a way back and lose one. `{names}` is the providers, run
  // together the way this language runs a list together (`../format`).
  "quit.confirmNotAll": "Quit Amenbo? Every terminal open in it ends. The conversations come back on the next run, apart from the panes running {names}. What they were running does not come back either.",
  // The OS notification a pane raises when its turn has come and nobody is looking at the terminal
  // (`AMB-T-3611`). It says a turn is standing and not whose: which pane it was is drawn where it
  // happened, and a toast that named one would answer in the one place a person cannot act on it.
  "face.ended": "The program in this terminal has exited.",
  "face.builtinDoing": "Amenbo is carrying this out",
  "face.builtinDone": "Amenbo carried this out",
  "face.builtinExit": "Exit: {exit}",
  "face.endedGeminiUnset": "Gemini CLI has no authentication method set. The path it names is this pane's own and goes with it — the one to set is your own ~/.gemini/settings.json.",
  "face.noWayBack": "This conversation cannot be opened again.",
  "face.projects": "Projects",
  // The rail's two halves: the lists, and the folder tree that moved there off the other side of the
  // panes (`AMB-D-835`). Each word names what its half holds rather than what pressing it does, because
  // both are always drawn and one of them is always on.
  "face.railFolders": "Folders",
  // The fold button on the rail, which says what pressing it does rather than which state the rail is
  // in: a button that names a state reads as either the state it is in or the one it goes to, and a
  // person cannot tell which (`AMB-D-848`). Neither word says "project", because the sidebar on the
  // task face folds with the same two words.
  "face.tabsCompact": "Hide the names",
  "face.tabsNamed": "Show the names",
  // What a project tab is read out as once it has panes open: the project, then how many
  // (`face.panes`, counted). The label on the button is the whole of what is read, so the badge
  // drawn inside it is never reached and the number has to be said here. The two are joined by
  // this language's own punctuation rather than by a space, which is not a pause everywhere.
  "face.tabPanes": "{name}, {panes}",
  // What the grip on a pane's corner does (`app/src/shell/TerminalPane.tsx`). The grip is drawn and
  // says nothing by itself, so a screen reader has only what is written here.
  "face.paneSize": "How much of the page this pane takes",
  "pane.size.whole": "1/1", "pane.size.half": "1/2 tall", "pane.size.halfDown": "1/2 wide",
  "pane.size.quarter": "1/4", "pane.size.sixth": "1/6", "pane.size.eighth": "1/8",
  // How many panes a project has open, read out with the project's name on its tab
  // (`app/src/shell/ProjectTabs.tsx`). As words rather than a bare digit: what stands beside the
  // number is a name, so nothing but this says what the number counts.
  "face.panes.one": "{n} pane", "face.panes.other": "{n} panes",
  "face.pages": "Pages",
  "face.page": "Page {n}",
  // Moving a pane to another page (`app/src/shell/PaneOrder.tsx`). The button carries only an icon,
  // so what it opens is said here and nowhere else — and what it says is the crossing, because a
  // move within a page is made on the page itself now (`app/src/shell/paneDrag.ts`). "Use this
  // order" rather than "Save": nothing about the arrangement is written down, and the word has to
  // say that the press is what makes the order the real one. "From page {n}" is worn by a card that
  // has ended up on a page other than the one it began on — the grid cannot show that, and it is the
  // whole reason a drag across pages reads as one move. The pane with neither a name nor a folder is
  // one nothing has been opened in yet.
  "face.order": "Move a pane to another page",
  "face.orderTitle": "The panes, page by page",
  "face.orderApply": "Use this order",
  "face.orderCancel": "Cancel",
  "face.orderFrom": "From page {n}",
  "face.orderNoName": "This pane",
  "face.orderEnded": "Ended",
  // The line over the row of things a terminal can be opened with (`app/src/shell/EmptySlot.tsx`).
  // The pills say by their shape that they can be pressed; this says it in words, because a row read
  // as a caption is not something anybody tries. It does not say "choose an AI" — the plain shell is
  // on the row and is not one.
  "face.whichStart": "What does this pane open with?",
  "face.open": "Open a terminal here",
  // The same button before anybody has chosen — the first run, and only it. It says what to do
  // rather than being pressable and doing nothing: what is missing is written on the thing the
  // reader would press, so there is no refusal to explain afterwards (`AMB-T-3686`).
  "face.openPick": "Choose one",
  "face.moreStarts": "Not installed ({n})",
  // The row while the machine is still being asked what it can start. It says what is
  // happening rather than leaving an empty row to be read as an answer (`AMB-D-792`).
  "face.startsChecking": "Checking what this machine can start…",
  // The machine could not be asked at all — the probe's shell would not start, or was still
  // reading the profile when the deadline ran out. It must not say "not installed" in either
  // direction: on a machine with four agents on it, that would be a lie about their own
  // machine.
  "face.startsUnchecked": "Amenbo could not check what this machine can start.",
  "face.startsRecheck": "Check again",
  "face.startsOwn": "Commands you registered",
  "face.startAdd": "Register a command",
  "face.startName": "Name",
  "face.startNamePh": "Claude (Opus)",
  "face.startLine": "Command line",
  "face.startLinePh": "claude --model opus",
  "face.startRuns": "This is what runs in the terminal:",
  // The line over the row of models the chosen agent can be started on (`app/src/shell/EmptySlot.tsx`).
  // Amenbo holds no list of its own: what the row draws is whatever that agent's command answered
  // when it was asked, so the question is about the tool that is on and not about models in general.
  "face.whichModel": "What model does it start on?",
  "face.modelsChecking": "Asking which models it can be started on…",
  "face.modelDefault": "Its own default",
  "face.modelDefaultNamed": "Its own default ({model})",
  "face.modelFind": "Narrow the list",
  "face.modelName": "Model name",
  "face.modelsMore": "{n} more — narrow the list to reach them",
  // The row under a running pane, where the model the program is answering on is moved
  // (`app/src/shell/PaneModel.tsx`, `AMB-D-865`). A running program takes no flag, so what the press
  // does is type the provider's own command into the pane — which is why the sentence names the
  // command, and why it is three sentences: two of the six read a name on that line as a prompt and
  // charge for the answer, so those get the command alone and their own picker opens.
  "face.modelHere": "Model",
  "face.modelSwitch": "Move this pane to another model",
  "face.whichModelNow": "What model does this pane move to?",
  "face.modelPicking": "The terminal is waiting for a choice.",
  "face.modelSendsNamed": "Puts {command} in the pane with the name you press, and that settles it.",
  "face.modelSendsPicker": "Puts {command} in the pane on its own. Its own picker opens, and the choosing is yours.",
  "face.modelSendsFilter": "Puts {command} in the pane, then the name in its search box. Confirming it is yours.",
  "face.modelKeeps": "It also moves your own default, in {path}.",
  "face.startSave": "Save",
  "face.startCancel": "Cancel",
  "face.startEdit": "Edit",
  "face.startRemove": "Remove",
  "face.openHere": "Room for another pane in this project",
  "face.whichFolder": "Which folder does this pane work in?",
  "face.rename": "Rename this pane",
  "face.handHere": "Drop to paste the path",
  "face.more": "More",
  // The box under a pane, where a line is written before it is sent (`AMB-D-864`).
  "face.compose": "Write to this pane",
  "face.composeSend": "Send",
  // The same press, named with the keys that do the same thing (`AMB-D-876`). `{keys}` is the
  // machine's own spelling — `⌘Enter` or `Ctrl+Enter` — and is not translated: it is the marks on
  // the keyboard in front of the reader. It is a key of its own because the plain word above is
  // still what a button that only sends is called (`../../../shell/PaneModel`).
  "face.composeSendKeys": "Send ({keys})",
  // The press on the pane's own band that opens and shuts that box (`AMB-D-889`). Two keys rather
  // than one because the words say what the press would do, and that differs by which state the box
  // is in — a reader who stops on a folded pane is asking how to get the box back, not being told
  // again that it is folded. The mark on the press is the same drawing in both states, so these
  // words are the only place the difference is said.
  "face.composeOpen": "Open the box to write in",
  "face.composeFold": "Fold the box away",
  // The other thing on that band: how much this session has filed, and behind it the records
  // themselves (`AMB-D-897`, `../../../shell/PaneMade`). The count is written by `act.nTasks`
  // and `act.nDecisions`, which are already the pair for saying it — this is what the press
  // stands for, said where a reader stops on it.
  "face.madeHere": "What this session has filed",
  // The file face beside the terminal's pane: the project's folder, folded, with what git says
  // about each row drawn as a colour rather than as words (`AMB-D-785`). What a file turns out not
  // to be is said in its own words — a binary is not a failure, it is simply not something a panel
  // can show.
  "files.side": "Notes and files",
  "files.memo": "Notes",
  "files.memoTyping": "Typing",
  "files.memoKept": "Kept",
  "files.tree": "Folder tree",
  "files.find": "Find",
  "files.findClose": "Close the panel",
  "files.findNext": "Next match",
  "files.findPrev": "Previous match",
  "files.findCase": "Match case",
  "files.findRegex": "Regular expression",
  "files.findWord": "Whole word",
  "files.replaceAll": "Replace all",
  "files.searchReplaceWith": "Replace with",
  "files.searchReplaceShow": "Show replace",
  "files.searchReplaceHide": "Hide replace",
  "files.searchReplaceGo": "Replace",
  "files.searchDrop": "Leave this one out",
  "files.searchNoUndo": "This writes the files. Nothing here takes it back — git does, where the folder is in one.",
  "files.searchAsk": "Replace {hits} places in {files} files?",
  "files.searchAskNo": "Cancel",
  "files.searchReplaced": "Replaced {hits} places in {files} files.",
  "files.searchLeftAlone": "{files} files were left alone: they changed after the search.",
  "files.searchStopped": "It stopped at {path}: {reason}",
  "files.search": "Search",
  "files.searchIn": "Search in this folder",
  "files.searchIgnored": "Also search the files the repository ignores",
  "files.searchTreeNote": "The tree draws what the repository ignores; this leaves it out until you ask.",
  "files.searchNone": "Nothing in this folder holds that.",
  "files.searchCapped": "There are more than these — narrow the search.",
  "files.searchFound": "{hits} in {files} files",
  "files.searchLooking": "Looking…",
  "files.filterNames": "Filter by name",
  "files.filterClose": "Close the filter",
  "files.filterNone": "No name here holds that.",
  "files.filterCapped": "There are more than these — type more of the name.",
  "files.capped": "This folder is too big to look through all of.",
  "files.unwatched": "This machine has run out of watches.",
  "files.unwatchedHow": "The supply is per user and shared with the editor you have open. Raising fs.inotify.max_user_watches gives it more room.",
  "files.noFolder": "This project has no folder yet.",
  // The reading column with nothing in it. Finding a file is the rail's now, so what stands here is
  // the line saying so rather than the tree this column used to hold.
  "files.nothingOpen": "Nothing is open.",
  // One control with two ends, so the word names neither of them: what it says is the pair, and which
  // way this press goes is the mark on it (`AMB-D-835`).
  "files.width": "Wider or narrower",
  // The tab a reader cannot see, and the way to it. The row scrolls rather than paging, so what this
  // names is everything the column is holding — the answer to "where did the fourth one go".
  "files.openFiles": "Open files",
  // The mark on the tab of a file holding something that is not on the disk. Read out as the tab's
  // title and by anything reading the screen aloud, which is the whole of what a dot can say
  // (`../../../files/FilesPanel`).
  "files.unsaved": "Not saved yet",
  "files.folderGone": "This folder is not there any more.",
  "files.rootPick": "Which folder this window is on",
  "files.rootChanged": "Something in here has changed",
  "git.tabFiles": "Files",
  "git.tabGit": "Git",
  "git.noRepo": "This folder is not a repository.",
  // The frame around what git wrote in refusing to answer about the folder — a repository
  // whose `git status` came back non-zero (`../../../files/GitPanel`, `AMB-T-4982`). Without
  // it the reader is handed a sentence of git's with nothing saying what it is about, and the
  // line it replaces said the folder was no repository, which it is. git's own words go under
  // this one and are not translated (`AMB-D-906`, 3-4).
  "git.noAnswer": "git did not answer about this folder.",
  "git.detached": "Not on a branch",
  "git.ahead": "Ahead by {n}",
  "git.behind": "Behind by {n}",
  "git.staged": "Staged",
  "git.nothingStaged": "Nothing is staged yet.",
  "git.changes": "Changes",
  "git.nothingChanged": "Nothing has changed.",
  // The box on a row, said in full: the box stands away from the name in the reading order of
  // anything reading the row out, and what it does depends on which of the two lists it is in.
  "git.stageOne": "Stage {path}",
  "git.unstageOne": "Unstage {path}",
  // The box on the line the list is named on, which is the same act over the whole of it in one
  // call out to git. It says which of the two lists it is on, the way the row boxes do.
  "git.stageAll": "Stage everything on this list",
  "git.unstageAll": "Unstage everything on this list",
  // And what that box is called where the press is about a set rather than about the row it is
  // on: how many it takes, since the one path it stands beside is no longer the whole of it.
  "git.stagePicked": "Stage the {n} files picked out",
  "git.unstagePicked": "Unstage the {n} files picked out",
  "git.commitMessage": "Commit message",
  "git.commit": "Commit",
  // How many paths the commit is about to name to git. It is the hinge of this screen — naming
  // them is what keeps the pane's half-staged work out of the commit (`AMB-D-906`, 3-2) — so it
  // is said rather than left to be counted off the list above.
  "git.commitNaming": "Naming the {n} staged paths",
  "git.stash": "Stash",
  // What is put aside, and taken back out. Only paths git already follows go into a stash here:
  // naming an untracked one is refused by the pathspec, with nothing put aside.
  "git.stashPush": "Stash the changes",
  "git.stashEmpty": "Nothing is stashed.",
  "git.stashRestore": "Restore",
  "git.fetch": "Fetch",
  "git.pull": "Pull",
  "git.push": "Push",
  "git.running": "Working…",
  "git.quiet": "git said nothing.",
  "git.history": "History",
  "git.backToHistory": "Back to the history",
  "git.merge": "merge",
  "git.bytes": "bytes",
  "git.noHistory": "Nothing has been committed here yet.",
  "git.touchedNothing": "This commit touched nothing.",
  "git.noPatch": "git wrote no patch for this path.",
  "git.diff": "Diff",
  "git.diffNone": "Press a changed file to read what it is holding.",
  "git.diffEmpty": "git wrote no patch for what is picked.",
  "git.fileHistory": "This file's history",
  "git.ignore": "Add to .gitignore",
  "git.untrack": "Stop following it",
  "git.restore": "Throw the changes away",
  "git.wholeHistory": "The whole folder's history",
  "git.noFileHistory": "Nothing here has touched this path.",
  "git.restoreAsk": "Throw away what {name} is holding?",
  "git.restoreAskMany": "Throw away what {n} files are holding?",
  "git.restoreGone": "git has not recorded it, so nothing brings it back.",
  "git.restoreQuiet": "Do not ask me again",
  "git.restoreGo": "Throw it away",
  "git.restoreKeep": "Keep it",
  "settings.restoreAsk": "Before throwing changes away",
  "settings.restoreAskOn": "Ask me",
  "settings.restoreAskOff": "Do not ask me",
  "settings.restoreAskNote": "A change git has not recorded is nowhere once it is thrown away, and nothing in the window brings it back.",
  "git.branches": "Branches",
  "git.onBranch": "The branch you are on",
  "git.newBranch": "New branch",
  "git.fromBranch": "from {name}",
  "git.branchName": "Name of the new branch",
  // The second face of the branch list, and what a row of it does. The list is the same names as
  // the face it was reached from and does the opposite thing to the working tree, so every row here
  // says what it does rather than leaving it to be known from which face is up.
  "git.mergeFrom": "Bring a branch in",
  "git.mergeInto": "into {name}",
  "git.mergeOne": "Bring in {name}",
  // A merge underway, drawn under the branch line for the whole of it — including after every
  // conflict has been settled, when there is no list of them left to say so.
  "git.merging": "A merge is underway",
  "git.mergeAbort": "Stop the merge",
  // What the press costs. A conflict settled by hand and never written down is in no commit and no
  // reflog, which is why this question stands in front of the press every time.
  "git.mergeAbortGone": "What has been settled by hand is written down nowhere, and stopping throws it away.",
  "git.mergeAbortGo": "Stop it",
  "git.mergeAbortKeep": "Go back",
  // What the merge could not settle, and the one number on that list git did not give: how many
  // conflicts are still written into the file. It is counted off the file rather than off the
  // index, so it falls the moment the file is put right, whoever put it right (`AMB-D-906`, 2-7).
  // The press beside it is the reader saying so — nothing here stages a conflict by itself.
  "git.conflicts": "Conflicts",
  "git.conflictMarks": "{n} marks left",
  "git.settleOne": "Say it is settled",
  "git.conflictsLeft": "{n} still to settle",
  "git.conflictsSettled": "Everything is settled",
  "git.mergeContinue": "Continue the merge",
  "git.takeOurs": "Take this branch's side",
  "git.takeTheirs": "Take the other branch's side",
  // The frame around the question git puts to the person when it needs a password and has no
  // terminal to ask in (`../../../files/GitAsk`, `AMB-D-913`). The question itself is git's own and
  // is drawn as git wrote it, in git's language — none of it is translated here.
  //
  // `what` is the call waiting on the answer, as a person would have typed it: `git push`. The
  // question never says which one it belongs to.
  "git.askDoing": "{what} is waiting for an answer",
  "git.askSave": "Keep it on this machine",
  // Where it goes, said because the answer is what nobody can see afterwards. Amenbo holds none of
  // it: it goes where the git this person runs themselves already looks, which is also the one place
  // to take it back from (`AMB-D-913`).
  "git.askSaveWhere": "Amenbo holds none of it — it goes where git already keeps them on this machine.",
  "git.askGo": "Answer",
  "git.askCancel": "Cancel",
  // What the row above a file offers, now that the column holds several of them and the tree is in
  // the rail: closing this one, which is what leaving it comes to (`AMB-D-835`). It read "back to
  // the list" while a file lay over the tree and there was a list to go back to.
  "files.closeFile": "Close this file",
  // The question in front of the press that loses what a reader typed. The file is named because
  // several tabs can be holding something at once, and the one being closed is the answer to "which
  // of them" (`../../../files/FilesPanel`). Nothing is asked of a file that has been saved.
  "files.closeConfirm": "Close {name}? What you typed into it has not been saved, and closing it throws that away.",
  "files.edit": "Edit",
  "files.read": "Read",
  "files.reopenWith": "Reopen with an encoding",
  "files.lineEndingMixed": "Mixed newlines",
  // One word for a file and for a folder: what is copied is the row, and a menu that named the two
  // differently would be saying they were two different doors (`AMB-D-832`).
  "files.copyPath": "Copy the path",
  "files.pasteFilePath": "Paste the file's path",
  "files.pasteFolderPath": "Paste the folder's path",
  "files.pastePaths": "Paste the paths",
  "files.openWith": "Open with the usual application",
  "files.chooseApp": "Open with an application I pick",
  "files.appUsual": "{name} (the usual one)",
  "files.reveal": "Show in the file manager",
  // Taking a file out of the folder, which here means the machine's own bin and nothing further: a
  // press that was a slip costs a trip to the bin rather than the file (`AMB-D-777`). The question
  // says so, because what makes "yes" cheap to press is knowing where the file goes.
  "files.trash": "Move to the bin",
  "files.trashAsk": "Move {name} to the bin?",
  "files.trashAskMany.one": "Move {n} item to the bin?", "files.trashAskMany.other": "Move {n} items to the bin?",
  "files.trashUndoable": "It goes where this machine's deleted files go, and undo brings it back.",
  "files.trashQuiet": "Do not ask again",
  "files.trashGo": "Move it",
  "files.trashKeep": "Cancel",
  "files.newFile": "New file",
  "files.newFolder": "New folder",
  "files.rename": "Rename",
  "files.name": "Name",
  "files.notText": "This is not text, so it cannot be shown here.",
  "files.cut": "Only the beginning is shown.",
  "files.unreadable": "This file could not be read.",
  // A PDF whose pages could not be drawn — the file is there and the host answered for it, but what
  // draws it would not open it (`AMB-D-907`). The way on below is the same one, because another
  // application may well open what this could not.
  "files.pdfFailed": "This PDF could not be opened.",
  // What a drawn page is called for a reader who is not looking at it: a page is a picture as far
  // as anything reading the screen out is concerned, so its number has to be written on it.
  "files.pdfPage": "Page {page} of {of}",
  // The one way on out of every file this panel does not draw — the binary, the one that could not
  // be read, and the picture refused for its size. A line that only says no leaves the reader
  // holding a file they still want opened (`AMB-T-4352`).
  "files.openElsewhere": "Open it in another application",
  "files.dropStopped": "{name} could not be brought in: {why}",
  "files.dropPartly": "{count} came in. {name} did not: {why}",
  "files.stoppedTaken": "something with that name is already there",
  "files.stoppedInside": "a folder cannot be moved inside itself",
  "files.stoppedNameless": "it has no name to be brought in under",
  "files.stoppedNoBin": "that drive has no bin, so nothing would have been left to bring back",
  "files.stoppedEmptied": "it is not in the bin any more",
  // Saving what was typed into the editor (`../../../files/FilesPanel`). The button says which
  // of the three it is, so there is no separate place for a reader to look for the answer.
  "files.save": "Save",
  "files.saving": "Saving…",
  "files.saved": "Saved",
  // The file moving under a reader who has typed into it, and the two answers there are to it:
  // write their own text over the file, or take what the disk says and lose theirs. The panel
  // settles it rather than the pane's agent, which cannot see an unsaved editor (`AMB-D-863`).
  "files.changedUnderneath": "Somebody wrote to this file after it was opened here.",
  "files.seeDifference": "See the difference",
  "files.keepMine": "Keep what I typed",
  "files.readAgain": "Read it again",
  // And the other press that loses it: taking what the disk says over what is in the editor. It is
  // pressed from the notice and from the screen that puts the two texts side by side, and it is
  // asked on both (`AMB-D-863`).
  "files.readAgainConfirm": "Read this file again? What you typed is thrown away for what is on the disk.",
  // The screen the two texts are put side by side on (`../../../files/FileDiff`). The two sides are
  // named above them: which of the two stands is the question, and a screen that named neither
  // would leave it to be guessed from which text looks familiar (`AMB-D-863`).
  "files.diffTheirs": "What is on the disk",
  "files.diffMine": "What you have typed",
  "files.diffClose": "Close",
  // A file with both kinds of newline in it. Rounding to the commoner one would rewrite every
  // line of the other, so the reader is told and asked (`AMB-D-773`).
  "files.newlinesMixed": "This file has both kinds of line break. Saving makes them all the same.",
  "files.newlineChoose": "Which line break to save it in",
  "files.newlineLf": "Unix (LF)",
  "files.newlineCrlf": "Windows (CRLF)",
  // A file the panel would not draw — a picture (`AMB-D-783`) or a PDF (`AMB-D-907`). What it is
  // refused for travels with the refusal, because a reader shown nothing at all reads it as a
  // damaged file. The line below it is a picture's alone: a PDF is refused on its bytes.
  "files.tooBig": "This file is too large to show here.",
  "files.tooBigPixels": "{width} × {height} pixels",
  // The talk window's standing band, shown only while Amenbo itself holds an administrator's token
  // on Windows. What it warns about is not a right the user lacks but one they have too much of:
  // the terminal opens, and the tools behind scoop's links are unreachable from inside it
  // (`AMB-T-3565`). Amenbo will say they are not installed, and be wrong in a way only this can
  // explain.
  "talk.elevated.title": "Amenbo is running as administrator",
  "talk.elevated.body": "A terminal opened here inherits that, and an administrator does not follow the links scoop installs its packages behind. Those tools cannot be reached from this window — Amenbo will report them as not installed, though they are.",
  "talk.elevated.fix": "Quit Amenbo and start it again without “Run as administrator”, and they come back.",
  // A ```mermaid fence whose diagram could not be drawn: the raw source is shown in its place.
  "mermaid.failed": "The diagram could not be drawn",
  // lint hook consent: the question Amenbo asks before writing into .git/hooks
  "hooks.title": "Keep Amenbo's refs out of your commits?",
  "hooks.why": "A ref like AMB-T-… means nothing outside the store that issued it. A git hook stops one before it reaches a commit.",
  "hooks.scope": "Asked once. Your answer covers the repositories Amenbo works in, now and the ones you add later.",
  "hooks.where": "{project} — {dir}",
  "hooks.yes": "Yes (recommended)",
  "hooks.no": "No",
  "hooks.hint": "`{cmd} hooks install` sets this up later, whenever you want it. `{cmd} hooks uninstall` opts one repository out.",
  "hookSetup.title": "The lint is not running on your commits",
  "hookSetup.where": "{project} — {dir}",
  "hookSetup.unwired": "{slots}: no hook there. `{cmd}` installs it.",
  "hookRestored.title": "Amenbo restored its lint block",
  "hookRestored.slots": "{slots}: the block had been changed or removed — restored it to the current version.",
  // Where the plugins went, said once (`AMB-D-884`). The band names what was installed on this device
  // and what came across with it; the Viewer's line is there because its next send places the whole
  // backlog again, which is a wait worth warning about rather than a setting that moved.
  "handover.title": "The plugins are part of Amenbo now",
  "handover.plugins": "{plugins} — there is nothing left to install and nothing left to enable.",
  "handover.notify": "Your mail and Slack settings came with them: {targets} connection(s) on this device, and {projects} project(s) reporting through them.",
  "handover.viewer": "The Viewer is this device's own setting now, with the keys your phone was paired on. Its next send places the whole backlog again, so that one takes longer than usual.",
  "handover.worktree": "Cutting a task its own worktree is Amenbo's own command now.",
  // The standing row on a project's own screen — the whole of what the GUI says about the session-start
  // hook. It speaks for one project and names that project's folders, and the last of its buttons is the
  // "no" that ends it.
  "agentHookWiring.title": "Have your AI read Amenbo at the start of every session",
  "agentHookWiring.what": "{tool}: give the text below to your AI to edit {file}, and your AI reads how to work with Amenbo at the start of every session and records the work as tasks.",
  "agentHookWiring.folders.one": "Folder not set up yet (paste the text once):",
  "agentHookWiring.folders.other": "Folders not set up yet (paste the text once in each):",
  "agentHookWiring.pick": "Which tool do you use here?",
  "agentHookWiring.copy": "Copy the text",
  "agentHookWiring.copied": "Copied",
  "agentHookWiring.no": "Don't show this again",
  "agentHookWiring.later": "Later",
  // The offer to be woken once an hour, put across the whole app. The band says why the timer is
  // wanted; the settings row (`tickSetting.*`) says what is set now and how to change it.
  "tickBanner.title": "Be told before a task's day comes",
  "tickBanner.what": "Once an hour Amenbo wakes, looks for tasks whose day is near, and sends the warning on to the notification targets the project has chosen. It runs from this computer's own scheduler, so nothing stays running in the background.",
  "tickBanner.start": "Start checking due dates",
  "tickBanner.never": "Don't show this again",
  "tickBanner.later": "Later",
  // The automations screen: its three tabs, the list of definitions, and the build
  // screen's launch place — what stands between one automation and a launch.
  "auto.title": "Automations",
  "auto.tab.running": "Running",
  "auto.tab.automations": "Automations",
  "auto.tab.actions": "Actions",
  "auto.tab.history": "History",
  "auto.emptyEverywhere": "No project has an automation yet.",
  "auto.new": "New automation",
  "auto.new.name": "Name",
  "auto.new.make": "Create",
  "auto.new.cancel": "Cancel",
  "auto.newFirst": "Create your first automation",
  "auto.archivedFold": "Archived {count}",
  "auto.archived": "Archived",
  "auto.stepCount": "{count} placement(s)",
  "auto.id": "ID {id}",
  "auto.build.back": "Back to the list",
  "auto.build.picture": "Build",
  "auto.build.edit": "Edit",
  "auto.build.step": "What the placement holds",
  "auto.place.open": "Open the action",
  "auto.place.empty": "Nothing is in it yet. It cannot be started until it is built.",
  "auto.place.agents": "Run by",
  "auto.place.agentNone": "not chosen",
  "auto.place.agentOf": "Who carries out {step}",
  "auto.place.modelOf": "Model for {step}",
  "auto.start": "Start",
  "auto.launch.see": "See in the picture",
  "auto.startOne": "Start an automation",
  // The picture of the steps on the build screen: what it says about a step, and what it writes
  // beside a line. `auto.pic.lap` names the span of one task, which is what the dashed outline
  // around a group of steps is.
  "auto.pic.lap": "One task",
  "auto.pic.insert": "Put an action in here",
  "auto.pic.errorExit": "error",
  "auto.pic.endsDone": "end the run",
  "auto.pic.endsHalt": "stop and call a person",
  "auto.pic.unfed": "nothing reaches {names}",
  "auto.pic.hands": "hands {from} on to {to}",
  "auto.pic.entry": "Start",
  "auto.pic.legendNext": "Next",
  "auto.pic.legendBack": "Goes back",
  "auto.pic.legendBranch": "Branches",
  "auto.pic.legendWire": "Handed on",
  "auto.pic.legendLeaves": "Leaves by an exit",
  "auto.pic.legendUnfed": "Input nothing reaches",
  "auto.pic.emptyMark": "Empty",
  "auto.pic.place": "Place an action",
  "auto.pic.placeDo": "Place",
  "auto.pic.first": "＋ Place the first action",
  "auto.lib.here": "Here",
  "auto.lib.makeNew": "＋ Make a new one",
  "auto.lib.makeNamed": "＋ Make a new one named “{name}”",
  "auto.lockMark": "Can’t be changed here",
  "auto.lib.make": "Make an action and place it",
  "auto.make.go": "Make and open",
  // The panel beside the picture: what the pressed step holds, field by field. The three rows a
  // task filter is answered on borrow the words the board's filters already use
  // (`filter.dim.*`, `filter.opt.assignee.*`); only the premises row is this panel's own.
  "auto.step.none": "Press a placement in the picture and what it holds shows here",
  "auto.step.name": "Name",
  "auto.step.entry": "Make this placement the start",
  "auto.step.next": "What happens next",
  "auto.step.nextNothing": "Nothing said yet",
  "auto.step.nextErrorNothing": "Stop and call a person (the default)",
  "auto.step.nextGo": "Open {name}",
  "auto.step.maxTimes": "Taken at most",
  "auto.step.maxTimesNone": "No limit",
  "auto.step.nextGoBack": "Open {name} (goes back)",
  "auto.step.nextGroupPlacement": "Open a placement",
  "auto.step.nextGroupStep": "Open a step",
  "auto.step.nextGroupExit": "End at an exit of this action",
  "auto.step.nextGroupEnd": "End here",
  "auto.step.placementRemove": "Take this placement off",
  "auto.step.placementRemoveConfirm": "This takes the placement off, with the settings answered on it and every line naming it. The action itself stays in the library. It cannot be undone.",
  "auto.step.task": "The task it is about",
  "auto.step.takesTask": "It goes and takes the next task",
  "auto.step.prompt": "Prompt",
  "auto.decl.add": "＋ Add",
  "auto.decl.required": "● Required",
  "auto.decl.more": "Change or remove",
  "auto.decl.outputAdd": "Add something handed on",
  "auto.decl.inputs": "Takes in",
  "auto.decl.inputName": "Name of what it takes in",
  "auto.decl.answeredOnPlacement": "Answered where placed",
  "auto.give.title": "Handed to the AI",
  "auto.give.history": "The run so far",
  "auto.give.taskNotes": "Task notes",
  "auto.give.taskDecisions": "Linked decisions",
  "auto.give.taskComments": "Comments",
  "auto.step.howRuns": "How it runs",
  "auto.act.entrySwitch": "Start at this step",
  "auto.act.nameOf": "{place} name",
  "auto.actions.notePlaceholder": "A line for the list",
  "auto.act.placedOn": "Placed on",
  "auto.step.interactive": "It may wait for a person's reply",
  "auto.step.folderNone": "Where the pane opens",
  "auto.step.cfg": "Settings",
  "auto.step.declaresNone": "None declared",
  "auto.step.required": "required",
  "auto.step.inputs": "Inputs",
  "auto.step.unwired": "nothing reaches it",
  "auto.step.exits": "Exits",
  "auto.step.exitUnnamed": "exit",
  "auto.step.modelDefault": "the agent's own default",
  "auto.step.notHere": "{agent} — not on this machine",
  "auto.step.folder": "Where it runs",
  "auto.step.reportToTask": "Its report also lands on the task",
  "auto.step.ready": "Premises",
  "auto.step.readyYes": "Can be started",
  "auto.step.readyNo": "Can't be started",
  "auto.step.sortTake": "Order them {sort}, and take the one on top",
  "auto.step.sort.priority": "by priority, highest first",
  "auto.step.sort.due": "by due date, soonest first",
  "auto.step.sort.created": "oldest first",
  // The two dialogs the build screen opens: the one that puts a step in on a line, and the one that
  // declares what a way out hands on. `auto.kind.*` names what a port carries, which both take.
  "auto.kind.value": "Value",
  "auto.kind.file": "File",
  "auto.kind.taskTake": "Task (taken)",
  "auto.kind.taskMake": "Task (made)",
  "auto.add.namePh": "What it is called",
  "auto.add.cancel": "Cancel",
  "auto.out.kind": "What it carries",
  "auto.out.add": "Add",
  "auto.step.add": "Add",
  "auto.step.remove": "Remove",
  "auto.step.exitName": "Name of the exit",
  "auto.step.cfgName": "Name of the setting",
  "auto.step.choices": "Choices, one per line",
  "auto.step.cfgKind.taskfilter": "Task filter",
  "auto.step.cfgKind.folder": "Folder",
  "auto.step.cfgKind.choice": "Choice",
  "auto.step.cfgKind.number": "Number",
  "auto.step.cfgKind.text": "Text",
  "auto.actions.reach": "Reach",
  "auto.actions.toGlobal": "Move to global",
  "auto.actions.toProject": "Move to a project",
  "auto.actions.toWhich": "Which project",
  "auto.actions.move": "Move",
  "auto.actions.reachGlobal": "Global",
  "auto.actions.reachProject": "This project",
  "auto.actions.reachBuiltin": "Built-in",
  "auto.bi.takeTask.name": "Take a task",
  "auto.bi.takeTask.does": "Looks for the tasks the filter matches that are not started and ready, in the order it sets, and reserves the first it can, moving it to in progress",
  "auto.bi.takeTask.filter": "Filter",
  "auto.bi.takeTask.whenNone": "When there is no task to take",
  "auto.bi.takeTask.goOn": "Don’t wait — leave by the exit “No task to take”",
  "auto.bi.takeTask.wait": "Wait until there is a task to take",
  "auto.bi.takeTask.taken": "Took a task",
  "auto.bi.takeTask.noneToTake": "No task to take",
  "auto.bi.takeTask.task": "Task",
  "auto.bi.cutWorktree.name": "Cut a worktree",
  "auto.bi.cutWorktree.does": "Cuts a worktree for the task in hand from the newest of the remote’s default branch, and hands on its path",
  "auto.bi.cutWorktree.worktree": "worktree",
  "auto.bi.cutWorktree.done": "Done",
  "auto.bi.foldWorktree.name": "Fold a worktree",
  "auto.bi.foldWorktree.does": "Removes the worktree and branch of the task in hand. If they hold changes not yet on the remote’s default branch, it leaves them and goes out by “Unmerged”",
  "auto.bi.foldWorktree.unmerged": "Unmerged",
  "auto.bi.foldWorktree.done": "Done",
  "auto.bi.closeTask.name": "Close the task",
  "auto.bi.closeTask.does": "Marks the task in hand done. If it was handed a commit, it records that SHA too",
  "auto.bi.closeTask.commit": "Commit",
  "auto.bi.closeTask.done": "Done",
  "auto.actions.unused": "Unused",
  "auto.actions.usedBy.one": "Used in {n} automation", "auto.actions.usedBy.other": "Used in {n} automations",
  "auto.actions.name": "Name",
  "auto.actions.note": "Notes",
  "auto.actions.cancel": "Cancel",
  "auto.actions.make": "Make",
  "auto.actions.makeOpen": "Make and open",
  "auto.actions.makeFirst": "Make the first action",
  "auto.actions.stepsEmpty": "Empty",
  "auto.actions.more": "More",
  "auto.actions.remove": "Delete",
  "auto.builtin.back": "Back",
  "auto.act.aboutPlace": "Action",
  "auto.act.edit": "Edit",
  "auto.act.openInSidebar": "Open in the sidebar to change ↗",
  "auto.held.by": "In use by run #{run}",
  "auto.held.openPane": "Open pane",
  "auto.act.stepsPlace": "Steps",
  "auto.step.nextExit": "End at exit “{name}”",
  "auto.pic.actionIn": "Takes",
  "auto.pic.actionOut": "Exits",
  "auto.pic.actionSelf": "This action",
  "auto.actions.search": "Search names and notes",
  "auto.actions.all": "All",
  "auto.actions.noMatch": "No action matches",
  "auto.actions.colSteps": "Steps",
  "auto.actions.colUsed": "Used by automations",
  "auto.act.none": "None",
  "auto.act.close": "Close",
  "auto.act.about": "This action",
  "auto.act.step": "Step",
  "auto.act.stepNone": "Press a step in the picture and what it holds shows here",
  "auto.act.insert": "Put a step in here",
  "auto.act.addTitle": "Add a step",
  "auto.act.insertTitle": "Put a step in",
  "auto.act.addPut": "Add",
  "auto.act.insertPut": "Put in",
  "auto.act.firstStep": "＋ First step",
  "auto.act.stepAdd": "＋ Step",
  "auto.act.stepRemove": "Delete this step",
  "auto.act.stepRemoveConfirm": "Delete this step? What it declares and every line naming it go with it. This cannot be undone.",
  "auto.about.name": "Name",
  "auto.about.notes": "Notes",
  "auto.about.cli": "CLI",
  "auto.about.copy": "Copy",
  "auto.about.copied": "Copied",
  "auto.about.notesHint": "A line shown on the list",
  "auto.about.archive": "Archived",
  "auto.about.remove": "Delete this automation",
  "auto.about.removeConfirm": "Delete this automation? Its placements, what happens after each and the wires between them go with it. The actions it placed stay in the library. This cannot be undone.",

  // The "running" tab: what is under way, what each run is on, and the three presses that
  // move it (`app/src/screens/RunningTab.tsx`).
  "auto.running.empty": "Nothing is running.",
  "auto.history.all": "All",
  "auto.history.empty": "No runs match.",
  "auto.history.prev": "‹ Prev",
  "auto.history.next": "Next ›",
  "auto.history.count": "{from}–{to} of {total}",
  "auto.run.pause": "Pause",
  "auto.run.resume": "Resume",
  "auto.run.stop": "Stop",
  "auto.run.running": "Running",
  "auto.run.completed": "Completed",
  "auto.run.failed": "Failed",
  "auto.run.canceled": "Canceled",
  "auto.run.acknowledge": "Acknowledge",
  "auto.run.paused": "Paused",
  "auto.run.pausing": "Pausing",
  "auto.run.crashed": "its step ended without reporting",
  "auto.run.maxTimes": "it went round too many times",
  "auto.run.noAgent": "its agent could not be started",
  "auto.run.noWayOn": "there was nothing left to open",
  "auto.run.noInput": "a required input had nothing to fill it",
  "auto.run.halted": "an exit called for a person",
  "auto.run.leftTaskOpen": "it tried to go on without closing its task",
  "auto.run.step": "Step {n} · {step}",
  "auto.run.inAction": "{step} in {action}",
  "auto.run.reportWithheld": "Report kept off the task, which was closed: {steps}",
  "auto.run.waitingForTask": "Waiting for a task it can take",
  "auto.run.taskWait": "Waiting for a task",

  // The other way in: an AI whose host cannot open a folder reaches this project over MCP instead
  // (`AMB-D-671`, `AMB-D-672`, `AMB-D-673`). Folded away on both the creation and the settings screen,
  // because the reader who has Amenbo on the command line needs none of it.
  // The MCP screen's own heading, said the way the sidebar entrance says it (`nav.mcp`) so the name a
  // reader pressed is the name they arrive at (`AMB-D-690`); `mcp.setupTitle` is the line on a
  // project's settings screen, which names the subject there rather than that road.
  "mcp.title": "Connect via MCP",
  "mcp.setupTitle": "Reach your projects from an AI",
  "mcp.open": "Connect over MCP (for an AI that cannot open a folder)",
  "mcp.hint": "An app reaches the projects you choose for it, through one server. Choose them once per app — Amenbo writes the whole choice each time.",
  "mcp.configured": "Set up",
  "mcp.unconfigured": "Not set up",
  "mcp.write": "Save the settings file",
  "mcp.copyAdd": "Copy the request",
  "mcp.copyRemove": "Copy a request to remove it",
  "mcp.copied": "Copied",
  "mcp.written": "Saved to {path} — open it to add the server.",
  "mcp.projects": "Projects this app may reach",
  // Said beside the button, because both halves are what a reader has to know before pressing it and
  // neither is recoverable from its word: the ticks are the contents of what goes over, and what goes
  // over replaces rather than adds (`AMB-D-690`).
  "mcp.handover": "The projects you tick are what the request and the file are made of. Handing one over replaces this app's entry whole — the folders it already reaches included.",
  "mcp.noProjects": "No project has a folder yet, so there is nowhere to point a server.",
  // Why the button that hands the selection over is shut, said beside it: a greyed
  // button says only that it cannot be pressed, never what would open it.
  "mcp.pickNone": "Nothing ticked is nothing to hand over — tick a project first.",
  "mcp.stale": "Left by an older Amenbo",
  "app.crashTitle": "Something went wrong",
  "app.crashHint": "The screen failed to render. Reload to recover — your data is safe.",
  "app.crashReload": "Reload",
  "pane.close": "Close",
  "pane.discardConfirm": "You have unsaved input. Discard it and close?",
  "pane.resize": "Drag to resize",
  "sidebar.resize": "Drag to resize",
  "health.title": "Startup integrity check found problems",
  "health.hint": "Read-only check, no automatic repair. Settings > Integrity lists every problem and can repair them.",
  "health.dismiss": "Dismiss",
  "health.repair": "Repair the folder pointers",
  "health.repairing": "Repairing…",
  "health.repaired.one": "Repaired the pointers of {n} folder", "health.repaired.other": "Repaired the pointers of {n} folders",
  "update.title": "An update is available",
  "update.hint": "A newer version has been published. You can update in place from here — it is applied only when you press this button; Amenbo never updates itself silently.",
  "update.open": "Update now",
  "update.checking": "Checking for the update…",
  "update.downloading": "Downloading… {pct}%",
  "update.downloadingUnknown": "Downloading…",
  "update.installing": "Installing…",
  "update.ready": "The update is ready. Restart to apply it.",
  "update.restart": "Restart to apply",
  "update.dismiss": "Dismiss",
  "update.upToDate": "You're up to date (v{version})",
  "update.checkFailed": "Couldn't check for updates",
  "managedBlock.title": "The AI guidance (CLAUDE.md / AGENTS.md) is out of date",
  "managedBlock.hint.one": "An app update changed the guidance block format ({n} folder). Resyncing rewrites only what is inside the markers to the current version (your own content is preserved).", "managedBlock.hint.other": "An app update changed the guidance block format ({n} folders). Resyncing rewrites only what is inside the markers to the current version (your own content is preserved).",
  "managedBlock.resync": "Resync",
  "managedBlock.resyncing": "Resyncing…",
  "managedBlock.done": "Resynced the AI guidance to the current version.",
  "orphanBinding.title": "Some bound folders belong to no project",
  "orphanBinding.hint.one": "A deleted project left these rows in the index ({n} folder). Forgetting them only drops the index rows — the folder contents and the .amenbo pointer are untouched.", "orphanBinding.hint.other": "A deleted project left these rows in the index ({n} folders). Forgetting them only drops the index rows — the folder contents and the .amenbo pointer are untouched.",
  "orphanBinding.forget": "Forget them",
  "orphanBinding.forgetting": "Forgetting…",
  "orphanBinding.done": "Forgot the leftover folder bindings.",
  "common.equiv": "Equivalent:", "common.otherSession": "another session",
  // What joins the reasons under a refusal that has several (see errLabel's `parts`).
  "err.reasonSep": "; ",
  "common.loadMore": "Load more ({n} more)",
  "id.copyTip": "Click to copy task ID", "id.copied": "Copied",
  "facet.human": "Human", "facet.ai": "AI", "facet.named": "{name} ({facet})",
  // onboarding
  "onboard.welcome": "Welcome to Amenbo",
  "onboard.tagline": "People and AI, one team. No server required. Your data never leaves your device.",
  "onboard.createLabel": "Create a project", "onboard.createHint": "Give it a name to create a new project",
  "onboard.createGo": "Create in app",
  "onboard.openLabel": "Open an existing project", "onboard.openHint": "Pick a project already on this device and link a folder to it",
  "onboard.stepsTitle": "Reference: hand work to your AI",
  "onboard.stepsIntro": "Start from one of the buttons above. This is where they take you, and how work reaches your AI agent (the same works from the CLI).",
  "onboard.s1title": "Link a folder to the project",
  "onboard.s1a": "Places ", "onboard.s1b": " (the local binding) and ", "onboard.s1c": " in that folder. An AI started there can operate the project.",
  "onboard.s2title": "Ask your AI",
  "onboard.s2body": "The last screen of creating a project, and a board with nothing on it yet, each offer to open the workspace in that folder. If you would rather use your own terminal, the same request text is one press away, ready to paste.",
  "onboard.s3title": "Then just drop tasks",
  "onboard.s3body": "Assign a task to the AI and it starts and proceeds autonomously. Everything shows up in the activity feed.",
  // list empty states
  "list.empty": "No matching tasks", "list.emptyInbox": "Nothing pending for you (and your AI)",
  "list.emptyArchived": "No archived items",
  "list.emptyDue": "Nothing due today or tomorrow",
  "list.unread": "Unread", "list.archive": "Archive from inbox",
  // Inbox tabs (Inbox = the active inbox / Archived = set aside, restorable).
  "list.tabInbox": "Inbox", "list.tabArchived": "Archived",
  // Inbox row actions: marking read (clear the dot, item stays) and archiving (set aside, restorable) are distinct.
  "list.markRead": "Mark read", "list.dismiss": "Archive",
  // Archived tab row action: restore to the inbox (unarchive).
  "list.unarchive": "Restore", "list.unarchiveTitle": "Restore to inbox",
  // board / detail tooltips
  "status.changeTip": "Change status",
  "reject.title": "Reject {ref}",
  "reject.why": "The reasoning is kept (required). It lands as a comment on the timeline.",
  "reject.placeholder": "Why this will not be done",
  "reject.confirm": "Reject", "reject.cancel": "Cancel",
  "done.title": "Mark {ref} done",
  "done.why": "Say what was done (required). It lands as a comment on the timeline.",
  "done.placeholder": "What was done",
  "done.confirm": "Mark done", "done.cancel": "Cancel",
  "card.addTaskTip": "Add a task", "card.assigneeTip": "Assignee (whom it's delegated to)",
  "block.deps": "Cannot start (dependency): waiting on {names}",
  "block.decisions": "Cannot start (premise not settled): {refs}",
  "block.notStarted": "Cannot start (waiting on its start day): from {date}",
  "block.draft": "Cannot start (still being created): finish creating it first",
  // The state, as the chip and the detail row both name it (`AMB-D-558`).
  "chip.draft": "Being created",
  "premise.changed": "Premises changed after you reserved this: {detail}",
  "premise.warn": "Premises changed after you reserved this: {detail}. Finish only the part that stands on its own, or hand it back by setting it to todo.",
  "premise.noLongerSettled": "no longer settled",
  "detail.premiseChanged": "Changed since reserved",
  "detail.premiseChangedHint": "Premises that moved after you reserved this — pinned on, or no longer settled (readiness withdrawn)",
  "detail.premiseAdded": "Pinned on after you reserved this",
  "detail.premiseReopened": "Stopped being settled after you reserved this — reopened or superseded (the link is older)",
  // What a notification says one event was, said the same way to every carrier a project reports
  // through. Four slots and never more: who drove the write, which record it was, the second thing the
  // event names (a status, an assignee, a project), and the event's own name where there are no words
  // for it. Anything the reader typed — a title, a slug, a ref — travels through untouched.
  //
  // Two forms where an event names a second thing and may arrive without it: the plain key says it,
  // and the `Bare` one is what is said when it did not arrive.
  "notify.say.taskCreated": "{who} created {what}",
  "notify.say.statusChanged": "{who} moved {what} to {state}",
  "notify.say.statusChangedBare": "{who} moved {what}",
  "notify.say.taskDone": "{who} finished {what}",
  "notify.say.taskRejected": "{who} decided against {what}",
  "notify.say.taskAssigned": "{who} assigned {what} to {state}",
  "notify.say.taskAssignedBare": "{who} assigned {what}",
  "notify.say.taskMoved": "{who} moved {what} into {state}",
  "notify.say.taskMovedBare": "{who} moved {what} to another project",
  "notify.say.taskDeleted": "{who} deleted {what}",
  "notify.say.decisionAccepted": "{who} finished writing {what}",
  "notify.say.decisionRejected": "{who} rejected {what}",
  "notify.say.commentAdded": "{who} added a comment on {what}",
  "notify.say.commentAddedBare": "{who} added {what}",
  "notify.say.commentRemoved": "{who} took back a comment on {what}",
  "notify.say.commentRemovedBare": "{who} took back {what}",
  "notify.say.taskDue": "{what} is due",
  "notify.say.taskDueTomorrow": "{what} is due tomorrow",
  "notify.say.unknown": "{who} acted on {what} ({event})",
  "notify.say.test": "Test message from Amenbo — this project reports here.",
  // The subject's own vocabulary: what happened, in the fewest words that still say it. It is shorter
  // than the sentences above on purpose — a line says who did what to which record, and a subject says
  // only what happened, the record's ref following it.
  // `count` is what a message carrying a burst says instead, and `countOne` is what that says at one.
  // Every language writes both: where a numeral changes nothing — which is most of them outside Europe
  // — the two are the same words, and saying so here is what keeps a correction to one from leaving the
  // other behind.
  "notify.say.subject.taskCreated": "Task created",
  "notify.say.subject.statusChanged": "Task moved to {state}",
  "notify.say.subject.statusChangedBare": "Task status changed",
  "notify.say.subject.taskDone": "Task finished",
  "notify.say.subject.taskRejected": "Task decided against",
  "notify.say.subject.taskAssigned": "Task reassigned",
  "notify.say.subject.taskMoved": "Task moved",
  "notify.say.subject.taskDeleted": "Task deleted",
  "notify.say.subject.decisionAccepted": "Decision written",
  "notify.say.subject.decisionRejected": "Decision rejected",
  "notify.say.subject.commentAdded": "Comment added",
  "notify.say.subject.commentRemoved": "Comment removed",
  "notify.say.subject.taskDue": "Task due",
  "notify.say.subject.taskDueTomorrow": "Task due tomorrow",
  "notify.say.subject.count": "{n} updates",
  "notify.say.subject.countOne": "{n} update",
};

// code → template (`{name}` interpolated from the error's fields). Only the codes whose full
// sentence can be reconstructed from the structured fields live here; everything else falls
// through to the message the command returned.
const err: Partial<Record<ErrorCode, string>> = {
  ambiguous_id: "The id “{prefix}” matches multiple candidates ({candidates})",
  binding_stale: "The linked project directory was not found: {path}",
  // The refusals the Tauri layer raises itself, for contexts core knows nothing about. Their
  // sentences live here rather than in Rust, so the reader gets them in their own language.
  pointer_other_store: "This folder's .amenbo was written by “{recorded}” and this build is “{running}”, so there is no project here to open: {path}",
  init_pointer_exists: "This folder (or one above it) is already bound to an Amenbo project: {path}",
  init_ambiguous_owners:
    "Several living projects claim this folder ({candidates}), so the lost marker cannot be put back for one of them: {path}",
  binding_nested_tree:
    "This folder is already inside an Amenbo-managed tree (bound at {path}). Binding a subfolder would hide the pointer above it.",
  migration_failed: "The store's update failed.",
  migration_running: "The store is being updated. Wait for it to finish.",
  pty_failed: "The terminal could not be started: {reason}",
  pty_gone: "That terminal is no longer open.",
  pty_paste_failed: "The pasted image could not be saved: {reason}",
  wake_unknown_agent: "Amenbo does not know how to start {agent}.",
  wake_no_folder: "That folder could not be read: {reason}",
  wake_no_config: "Amenbo could not find its own files.",
  wake_not_kept: "The choice could not be saved.",
  wake_not_registered: "A registered command needs both a name and a command line.",
  window_failed: "That window could not be opened: {reason}",
  talk_blank: "That window opened but never drew anything, so the workspace was put back in this one.",
  clip_refused: "The files could not be put on the clipboard: {reason}",
  // What the file panel answers a read with where the name is a link (`crate::folder`). It is not
  // the "not there" the other rules are refused with: the file is whole and the refusal is meant
  // (`AMB-D-782`), and somebody sharing one `CLAUDE.md` between projects meets it first.
  folder_link: "This is a link to another file, and Amenbo does not follow one — what it points at can be outside this project's folders.",
  folder_taken: "{name} is already there.",
  folder_name: "This machine will not take {name} as a name.",
  folder_make: "{name} could not be made: {reason}",
  folder_rename: "It could not be renamed to {name}: {reason}",
  // What the file panel answers a save with (`crate::folder_save`). The character is named
  // because writing it as `&#10003;` and saying nothing is the thing this exists to stop.
  folder_not_saved: "This file could not be saved: {reason}",
  folder_replace_read_only:
    "{count} of these files cannot be written to, so nothing was replaced in any of them.",
  folder_unwritable_character: "“{character}” cannot be written in {encoding}, so nothing was saved.",
  // And the save the panel does not make: the file moved between the read and the save,
  // so what the editor holds is older than the file (`AMB-D-784`).
  folder_changed_underneath: "Somebody wrote to this file after it was read here, so nothing was saved.",

  // The sentences the GUI shows a person, named one at a time (`AMB-D-413`). Which refusals get a code of
  // their own was settled by measuring what the front end actually surfaces: a form the screen already
  // guards (an empty title) never reaches a reader, and a code with no template here still reads — in
  // English, off the message the command returned.
  already_reserved: "{ref} is not “To do”, so it cannot be reserved — another session may already hold it.",
  // One refusal over a list of reasons. `{reasons}` is the parts, each written from its own template
  // below and joined with `err.reasonSep`.
  not_ready: "{ref} cannot be reserved yet: {reasons}",
  not_ready_open_blocker: "{ref} is not done",
  not_ready_premise_superseded: "{ref} was superseded by {successor}, so link this task to the newer one",
  not_ready_premise_rejected: "{ref} was rejected, so this task needs rethinking",
  not_ready_premise_unsettled: "{ref} is not settled — wait for it to be written to the end, or unlink it",
  // Core's English names the command that moves the day; a reader in the window has the field instead.
  not_ready_not_started: "it does not start until {start} — change the start date if that is wrong",
  not_ready_draft: "it is still being created — finish creating it first",
  // Pressing launch, and the list the build screen draws before anybody presses — one set of
  // sentences, because they are the same sentences. The list is a read that succeeded and the refusal
  // is a write that did not, so both hand over the code and the values, and both are written from
  // here (`core/i18n`'s `errSentence`).
  invalid_automation_archived: "“{automation}” is archived. Bring it back before starting it.",
  invalid_automation_workspace_closed:
    "The workspace is closed, and a run draws its steps in its panes. Open it and start again.",
  not_ready_automation: "“{automation}” is not ready to start: {reasons}",
  not_ready_automation_no_steps: "no action is placed on it",
  not_ready_automation_no_entry: "no placement is the start",
  not_ready_automation_action_empty: "the action “{action}” placed on it has no start step",
  not_ready_automation_entry_takes_no_task:
    "{step}: the start step takes no task, so every step after it would be about nothing",
  not_ready_automation_open_exit: "{step}: nothing is set to happen after {exit}",
  not_ready_automation_open_exit_unnamed: "{step}: nothing is set to happen after it",
  not_ready_automation_unwired_input: "{step}: nothing reaches the required input {port}",
  not_ready_automation_unanswered_cfg: "{step}: the required setting {cfg} is unanswered",
  not_ready_automation_agent_unchosen: "{step}: nobody is chosen to carry it out",
  not_ready_automation_agent_missing: "{step}: this machine cannot start {agent}",
  // Said of the model rather than of the agent, which the step's own row already names. Only ever
  // drawn for an agent that has said what it offers, so "does not offer" is an answer and not a
  // silence (`app/src-tauri/src/agent_models.rs`).
  not_ready_automation_model_missing: "{step}: its agent here does not offer the model {model}",
  not_ready_automation_task_left_open: "{step}: {exit} goes on to {to}, which takes another task, with this one still open",
  not_ready_automation_task_left_open_unnamed: "{step}: it goes on to {to}, which takes another task, with this one still open",
  not_ready_automation_task_left_open_at_end: "{step}: {exit} ends the run with the task still open",
  not_ready_automation_task_left_open_at_end_unnamed: "{step}: it ends the run with the task still open",
  store_busy: "The store is in use right now. Try again in a moment.",
  // The store itself giving way — a disk that would not answer, a database that could not be read.
  // What the engine said is a line in the diagnostic log, not a sentence for a reader: it names none
  // of what they were doing and nothing they could act on. This says the two things they can act on.
  storage_error:
    "The store could not be read or written, and nothing was changed. Restart Amenbo and try again — if it keeps happening, send what Settings > Logs holds.",
  not_found_task: "Task {ref} was not found.",
  not_found_decision: "Decision {ref} was not found.",
  not_found_project: "Project {ref} was not found.",
  not_found_user: "User {ref} was not found.",
  not_found_comment: "Comment {ref} was not found.",
  not_found_dimension: "Category {ref} was not found.",
  not_found_dimension_value: "Category value {ref} was not found.",
  not_found_blob: "This attachment's file is not on this device ({hash}).",
  invalid_commit_sha:
    "A commit SHA is the full 40 or 64 hex characters — a short SHA, a branch, a tag or a revision is not one.",
  invalid_attachment_too_large: "This file is {size} bytes, over the {max}-byte limit for its kind.",
  invalid_dimension_period_order: "A value's start date cannot fall after its end date.",
  invalid_dimension_required_without_values: "“{name}” offers no values, so it cannot be made required — add a value to it first.",
  invalid_dimension_required_unset: "“{name}” is a required category, so a task's value on it cannot be cleared — assign another value instead.",
  invalid_dimension_slug_shape: "“{slug}” cannot be a key — use at most {max} lower-case letters, digits and hyphens, starting with a letter.",
  invalid_dimension_slug_taken: "“{slug}” is already the key of something else here — pick another.",
  invalid_dimension_name_whitespace: "“{name}” cannot be a name — leave the spaces out, or write a hyphen instead.",
  invalid_dimension_demote_holders: "“{name}” cannot go back to single-select while records still answer it with more than one value (currently {count}). Clear the extra values off them first.",
  invalid_dimension_multi_time_axis: "“{name}” cannot be both the time axis and multi-select — the time axis holds one value at a time, so drop the time-axis role, or leave it single-select.",
  invalid_dimension_close_not_closable: "“{name}” does not close its values, so “{value}” cannot be closed — make the category closable first, or delete the value.",
  invalid_dimension_close_last_open: "“{name}” is a required category and “{value}” is the last value it still offers, so closing it would leave a demand nobody can meet — turn off Required first.",
  invalid_dimension_set_closed_value: "“{value}” is closed on “{name}”, so nothing new is filed under it — choose an open value, or reopen this one.",
  invalid_task_required_dimension: "This task carries no value on {names}, which this project requires.",
  invalid_task_status_draft: "{ref} is still being created, so its status cannot be changed — finish creating it, or delete it.",
  invalid_decision_required_dimension: "This decision carries no value on {names}, which this project requires.",
  invalid_dimension_values_unordered: "This category's values carry no order, so they cannot be re-ordered.",
  invalid_decision_edit_rejected: "{ref} was rejected, and a rejected decision cannot be edited.",
  invalid_decision_reject_accepted: "{ref} is decided. Supersede it rather than rejecting it.",
  invalid_decision_reopen_rejected: "{ref} was rejected, and a rejected decision cannot be reopened.",
  invalid_decision_self_supersede: "A decision cannot supersede itself.",
  invalid_decision_self_amend: "A decision cannot amend itself.",
  invalid_decision_self_builds_on: "A decision cannot stand on itself.",

  // Settings > Data, and the startup update screen. What a person meets there is the path or the
  // file they chose, or a disk with no room left — so the sentence names it, and says whether
  // anything was changed. The inner reason a failed update carries ({failure} / {rollback}) is
  // whatever went wrong underneath, and stays in core's English.
  invalid_backup_dest_is_dir: "{path} is a folder. A backup is written as one file — give a file name.",
  invalid_backup_dest_exists: "{path} already exists, and a backup overwrites nothing — choose another name.",
  invalid_restore_source_is_dir: "{path} is a folder. Restoring takes one backup file.",
  invalid_restore_not_an_archive: "{path} is not an Amenbo backup file.",
  invalid_restore_missing_snapshot: "{path} carries no data — the backup file is damaged. Nothing here was changed.",
  invalid_restore_layout_too_old: "This backup is in an older format (v{layout}); this version reads v{min} and later. Restore it with the Amenbo that wrote it. Nothing was changed.",
  invalid_restore_layout_too_new: "This backup is in a newer format (v{layout}); this version reads up to v{max}. Update Amenbo and try again. Nothing was changed.",
  invalid_restore_archive_newer: "This backup came from Amenbo v{app}, whose data is at v{found} — past the v{max} this version reads. Use Amenbo v{app} or later. Nothing was changed.",
  invalid_export_dest_exists: "{path} already exists. Export makes a new folder — choose a name that is free.",
  invalid_migration_no_space: "There is not enough free space for the backup taken before updating: ~{need} MiB is needed (archive ~{archive} MiB + staging ~{staging} MiB), and ~{free} MiB is free at {dir}. Free up space and try again — nothing has been changed yet.",
  invalid_migration_rolled_back: "The update failed and your data was put back exactly as it was ({failure}). The backup taken before it started is kept at {at}.",
  invalid_migration_rollback_failed: "The update failed ({failure}), and putting your data back failed too ({rollback}). It may be half updated — restore the backup taken before the update, at {at}.",
};

// doctor issue kind → template (`{name}` interpolated from the issue's params).
const doctor = {
  self_dependency: {
    message: "Dependency {dep} waits on itself.",
    fix: "Remove that link from the task's dependencies.",
  },
  duplicate_order_key: {
    message: "Project {project} has tasks sharing the same ordering key ({order_key}).",
    fix: "Re-order the tasks and it resolves itself.",
  },
  orphan_attachment: {
    message: "{attachment} hangs off {target}, and there is no such record — the row outlived what it was attached to, and its file is held out of reach while nothing can get to it.",
    fix: "“Repair” drops the row and lets its file go. There is nothing to open first: what it hung off is already gone.",
  },
  stale_managed_block: {
    message: "The AI guidance (Amenbo managed block) in {path} is stale (v{version} → v{current}).",
    fix: "“Resync” brings this folder's guidance up to date (your own content is preserved).",
  },
  guidance_blocks_checkout: {
    message: "Amenbo wrote {path} and git does not track it on this branch, while {branches} does — checking that branch out is refused.",
    fix: "Settle the path one way: track the file here too, or stop tracking it on {branches}. To switch across just once, move the file aside first — Amenbo writes it again the next time it opens this folder.",
  },
  legacy_pointer: {
    message: "{path} is an old-format binding — an AI started there does not resolve to the project.",
    fix: "“Re-bind” binds the folder to project #{project} again.",
  },
  legacy_pointer_ambiguous: {
    message: "{path} is an old-format binding, and it does not point at one particular project.",
    fix: "Pick the project for this folder in Project settings > Folders.",
  },
  missing_pointer: {
    message:
      "{dir} is recorded as a folder bound to project #{project}, but the marker (.amenbo) is gone — an AI started there does not resolve to that project.",
    fix: "“Re-bind” puts the marker (.amenbo) back (project #{project}).",
  },
  missing_pointer_ambiguous: {
    message:
      "{dir} is recorded as a bound folder ({claims}), but the marker (.amenbo) is gone and it does not point at one particular project.",
    fix: "Pick the project for this folder in Project settings > Folders.",
  },
  orphan_binding: {
    message: "{dir} is still listed as a bound folder, but no project claims it (a leftover from a deleted project).",
    fix: "“Repair” forgets it from the list (the folder itself is untouched).",
  },
  dead_ref: {
    message: "The body at {at} points at {refs}, and there is nothing there — a reader sent after one comes back empty-handed.",
    fix: "Open the body and edit it: drop the ref, or point it at what stands in its place. Only the person who wrote it knows what it meant to say.",
  },
  start_after_due: {
    message: "{task} is set to start on {start_on} but was due on {due_on} — it stays out of the inbox until a day that is already past its deadline.",
    fix: "One of the two is wrong: correct either the start day or the due day. Nothing picks a winner between them, since either one could be the typo.",
  },
  project_without_folder: {
    message: "Project {name} (#{project}) has no folder — nothing can operate it, and an AI cannot see it at all.",
    fix: "Add a folder in Project settings > Folders.",
  },
  unwired_folder: {
    message: "{dir} does not start its AI on Amenbo: {tools} is set up here and is not wired to run it at session start.",
    fix: "In Project settings > Folders, copy the text Amenbo offers and give it to the AI you run in that folder — Amenbo writes no settings file for you.",
  },
  unwired_folder_ambiguous: {
    message: "{dir} does not start its AI on Amenbo, and shows no sign of which tool is used there.",
    fix: "In Project settings > Folders, pick the tool you use there and give its text to that AI.",
  },
} satisfies Record<DoctorIssueKind, DoctorTemplate>;

export const en = { status, priority, view, ui, err, doctor };
