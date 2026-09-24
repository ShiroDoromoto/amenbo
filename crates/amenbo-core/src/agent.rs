//! The `amenbo agent --json` spec — the single source of truth for an AI agent. This spec alone
//! teaches everything an AI needs to operate Amenbo: the philosophy, every command and flag, the
//! workflow, the rules, and what state to read. It lives in core, and `amenbo agent` is the one face
//! that hands it out: the GUI carries no command reference at all (`AMB-D-836`), because a list of
//! commands and flags is for whoever types them. That every subcommand in `cli.rs` shows up here is
//! held by an integration test on the CLI side, which catches a command that was never registered.
//! Amenbo is a single local store — there is no sharing, sync, key or multi-device face — so the
//! spec always returns the one personal shape (`mode: personal`).

use crate::config::Paths;
use serde_json::{json, Value};
use std::collections::HashSet;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// The shape of what this build hands out. Bumped at `2` when `agentCycle` stopped being a list of
/// numbered lines and became a described list of steps, each with the `id` that names it.
pub const SCHEMA_VERSION: &str = "2";

/// Builds the spec. A single local store, so there is one shape: personal mode. Every exit —
/// [`build_index`], [`command_spec`] — comes through here or repeats its last step, so the retarget
/// reaches all of them.
pub fn build() -> Value {
    let mut spec = spec_as_authored();
    retarget(&mut spec, Paths::command_name());
    spec
}

/// The spec as it is written in this file: every command spelled with the production CLI, the name a
/// reader of the source recognises. [`build`] is what hands it out, and it retargets that name
/// first — this is not a second source of truth, it is the one source before that one step.
fn spec_as_authored() -> Value {
    json!({
        "amenbo": "A local-first task manager with no central server. You (the AI agent) operate it on the user's behalf.",
        "operating": [
            "The user runs Amenbo here on purpose — setting it up in this folder (amenbo init) is their standing instruction that the work here be tracked with Amenbo. So manage the work with it as you go, without waiting to be told to each time: this is operating the tool the user chose, not automating something they did not ask for.",
            "Amenbo — not the chat log or your own memory — is where the work and its state live. That is why a substantive request belongs on it before you start, why its status is worth keeping current while you work, and why the outcome belongs on its timeline: the next session reads what is filed there, and whatever never reached it goes down with this conversation. The steps themselves are in `agentCycle` — step 0 puts the request there, steps 2–4 carry it.",
            "Right granularity, so the tasks stay signal not noise. Make a task for: a concrete change or deliverable, multi-step work, anything that outlives a single session, or work you are handing to a person or their AI. Do not make one for: a question you answer on the spot, a throwaway one-off with no follow-up, or something an open task already covers (comment on that task instead of duplicating it). When the work is substantive and you are unsure, prefer a task — but never pad Amenbo with trivia.",
            "Explore by narrowing, not by dumping. The store is a corpus you read a slice of, not a file you load: hold a word and `search <word> …` answers with the places it is written; hold a condition and `--filter` (`status:`, `assignee:`, `decision:`, `task:`) with `--limit` cuts the listing down. Read that answer, then open the few that matter with `task show` / `decision show` (and `comment list` for a task's timeline). Reach for a full dump — `task list --json` with no filter, `decision list --with-body` — only when you genuinely need every row, which is rare: the corpus grows without bound, so what fits today is what times out in a year, and the tokens you spend reading it are tokens you no longer have for the work. The same shape is why this document indexes its commands instead of inlining them: find, then pull.",
            "A decision is a premise in force, not a log of what happened, and only what the human settled with you belongs on one: a choice you made alone goes in a comment on the task instead, and `decision promote` raises it later if the work turns out to rest on it. You write it in two stages — `decision add`, then `decision finish-writing` — and nobody else rules on it. It weighs less than filing the task — that duty is standing, this one is a nudge, so don't paper every change with a decision. Where the judgement is made is step 0, and nowhere else."
        ],
        "mode": "personal",
        "version": VERSION,
        "schemaVersion": SCHEMA_VERSION,
        "updateAvailable": false,
        "principles": [
            "No central server (the data always lives on the user's device).",
            "Works fully offline (local-first).",
            "Data can be fully exported at any time (no lock-in).",
            "A delete is physical and irreversible; archiving a project is what keeps a record you no longer work on (there is no task-level archive — a task you have finished is done, not archived)."
        ],
        "conventions": {
            "id": "The id **is** the conversational number, and every ref Amenbo shows carries the `AMB-` namespace: a task whose `id` is `<n>` is shown as AMB-T-<n>, and a decision whose `id` is `<n>` as AMB-D-<n>. In --json, `id` carries the number and the `ref` field beside it carries that rendering — which is what the human-readable output leads with. Numbers are device-global — one number names one task, with no project context needed — and they come from two sibling spaces (tasks and decisions), so a number alone can name both task AMB-T-<n> and decision AMB-D-<n>; the kind code is what disjoins them. Quote the namespaced form in anything you write — a bare `T-<n>` is another tracker's ref as much as ours, which is exactly what `AMB-` settles. Reading is looser than writing: commands still accept the bare forms (`<n>` / `#<n>` / `T-<n>` / `D-<n>`) alongside the namespaced ones, so a ref copied off the screen pastes straight back.",
            "output": "Output defaults to human-readable text. Read commands produce machine-readable output with --json.",
            "markdown": "Body / free-text fields — task notes (--notes), comments (--text), and decision bodies (--body) — are Markdown, rendered in the GUI (the CLI shows the raw source). Write for a reader scanning fast: lead with the conclusion / TL;DR, prefer bullet lists and tables over paragraphs, keep one point per line (even a single newline shows as a line break, so never run everything onto one line), and split anything long under headings. Renders: GFM tables and task lists, and ```mermaid fenced diagrams (flowchart / sequenceDiagram / stateDiagram / erDiagram; broken syntax falls back to the source, so it never breaks the page). Does not render: raw HTML is ignored, and images are not inline — attach them with task attach / comment attach instead. Reach for Mermaid only when a relationship, flow, state machine, or sequence genuinely reads better than prose or a table; since the CLI shows the raw diagram source, put the key point in one line of text first and let the diagram support it — never carry meaning in the diagram alone.",
            "dates": "Dates are YYYY-MM-DD. Relative forms like 'today' / 'tomorrow' / '+3d' are also accepted.",
            "destructive": "Destructive operations prompt for confirmation by default. Pass --yes / -y to run them non-interactively.",
            "globalFlags": ["--json", "--yes", "--quiet", "--no-color", "--actor <human|ai>", "--project <name|id> (human only — see reach)"],
            "explicitTarget": "By default the project is fixed by location — the .amenbo pointer found upward from the CWD. For a human, one pre-subcommand override acts like `git -C`: `--project <name|id>` overrides the effective project context (defaults like `decision add`'s project; note a ref no longer depends on it — numbers are device-global). Resolution is `--project` (explicit) > `.amenbo` (CWD) > error, with no silent guessing. It is a side path for driving a project from outside its folder; the everyday route is to bind a folder. **An AI has no such override** (see reach): for it, location is the only answer. This device holds a single store, so there is none to select.",
            "reach": "An AI works inside the project its folder is bound to, and nowhere else. The `.amenbo` pointer is not a way of naming a store; it **is** the AI's reach: what it can list, read, and write. Three consequences, all enforced (an AI does not have to remember them — the CLI refuses with `out_of_reach`, never a silent empty result): (1) **you do not pick a project.** `--project` and the `project:` filter are human-only vocabulary; passing either is refused even when it names your own bound project. The flip side is that you rarely need one — `task add` / `decision add` / `dimension add` / `dimension list` take their project from the binding, so just omit it. (2) **you read only your project.** Lists, timelines, `status` and `project list` show your project's rows; an id from another project — a task, a decision, a comment id, an attachment id, a dimension — is refused rather than served. (3) **you write only inside it.** Mutations outside are refused, and so is creating anything outside (a new project included). Being out of reach is not being absent: the answer is `out_of_reach`, never `not_found` — the row exists, you just cannot get to it from here. A folder with no binding at all reaches nothing, so an AI there is refused outright: ask the human to `amenbo bind --project <name or id>`, or work in a bound folder. This closes Amenbo's own surface, and nothing more: the human is not scoped, and a shell can still read the file underneath.",
            "facet": "The facet of an operation (a person / that person's AI) is declared with --actor, and there alone: no environment variable carries it, and it is never defaulted. Every operation that uses a facet requires one — the writes that stamp it onto created_by / assign / activity, and the reads that surface store content, which draw an AI's reach from it (see reach). One without it is refused with facet_required (exit 2), so pass --actor ai on every command, reads included. Writes echo the facet back as acted_facet, so a mis-set one is immediately visible.",
            "exitCodes": { "0": "success", "1": "error", "2": "unknown command or bad arguments (also facet_required)" }
        },
        "agentCycle": agent_cycle(),
        "cycles": cycles(),
        "inspect": [
            "amenbo config --json",
            "amenbo status --json",
            "amenbo project list --json",
            "amenbo task list --json",
            "amenbo doctor --json",
            "amenbo validate --json"
        ],
        "commands": all_commands(),
        "capabilities": capabilities(),
        "filterGrammar": {
            "description": "task list's --filter combines key:value pairs (whitespace-separated) with AND. Words are not one of the keys: to find where something is written, `amenbo search <word> …` is the one command that reads them, and its own --filter takes this grammar under `--kind task` (decision list's under `--kind decision`; none without one).",
            "keys": {
                "done": ["true", "false", "(closed = done or rejected — not \"was it carried out\")"],
                "status": ["todo", "in_progress", "done", "blocked", "rejected", "todo,in_progress (comma = any-of)"],
                "due": ["today", "overdue", "week", "none", "YYYY-MM-DD"],
                "start": ["today", "future", "none"],
                "priority": ["high", "medium", "low", "none", "high,low (comma = any-of)"],
                "project": ["<id>", "<name (exact)>", "(human only — an AI's list is already its bound project)"],
                "number": ["AMB-T-<n>", "<n>", "#<n> / T-<n> (bare forms, still accepted)", "AMB-D-<n>"],
                "ref": ["alias of number"],
                "assignee": ["none", "me", "me-ai", "me,me-ai (comma = any-of)"],
                "ai": ["true", "false"],
                "ready": ["yes", "no"],
                "draft": ["yes", "no"],
                "blocked": ["none", "open"],
                "decision": ["AMB-D-<n>", "D-<n>", "<n> (the tasks a decision links to)"],
                "commit": ["<full 40/64-hex sha>", "(the tasks recording this commit sha — the reverse chain git → task)"],
                "dim": ["<axis>=<value>", "<axis>=none (no value on that axis)", "dim:A=x dim:A=y (the same axis twice = any-of)"],
                "dimension": ["alias of dim"],
                "time_axis": ["<value>", "none", "(sugar for dim:<the time_axis axis>=…)"]
            },
            "example": "amenbo task list --filter \"done:false due:today priority:high\" --sort due --json",
            "note": "Different keys combine with AND, and a key naming several values ORs them — a filter picks a set of values per axis. `status:` / `priority:` / `assignee:` take that set comma-separated (`status:todo,in_progress` = todo OR in_progress; `priority:high,low`; `assignee:me,me-ai`), and repeating one of them adds to the same set. `dim:` (alias `dimension:`) has the repetition alone, a value's name being your own text and free to hold a comma: different axes AND (`dim:Category=bug dim:Area=core`), the same axis twice ORs (`dim:Area=core dim:Area=gui`, `=none` an element of that set like any value). An axis and a value each resolve by id (exact), then by slug (exact), then by name (exact, case-insensitive), and `=none` selects tasks with no live value on that axis. A name that resolves to nothing is an error, not an empty result — including on `=none`, where an axis that does not exist would otherwise match *every* task. (Axes are per-project, so a name two projects share resolves to both and the filter ORs them; scope with `project:` to mean one of them.) `time_axis:` is sugar for the axis carrying `role: time_axis` — it keys on the role, not on the axis's name, so it works whatever the user named it; with no time axis declared it names nothing, so it is an error too. A name cannot hold whitespace: the filter splits on it first, so the door refuses one (an older store's is reached by its slug or id). The AI mailbox query is defined once in agentCycle step 1 (referenced, not repeated here). `ready:yes` = every declared premise is met, so the task can be reserved; `ready:no` (= `blocked:open`) = a premise is unmet — an open blocker (blocked_by_open), a linked decision that is not settled (blocked_by_decisions), a declared start day that has not come (not_started_until), or a creation nobody has finished (draft). `draft:` asks for that fourth premise on its own, the way `start:` asks for the third: `draft:yes` is the creations still open — listed everywhere, reservable nowhere, until `task finish-creating` ends them — and `draft:no` is the rest. `start:` takes named arms only, no bare date (unlike `due:`): `start:future` is the queue waiting on its day, `start:today` what has come, `start:none` what declared no day. `number:` (alias `ref:`) filters by the conversational number: a bare number / `AMB-T-<n>` (or the bare `#<n>` / `T-<n>`) match a task's number; an `AMB-D-<n>` names a decision and so matches no task (tasks and decisions have separate number spaces). `done:` is closed-or-not, so `done:false` is what is still outstanding; ask `status:done` for what was carried out. `ai:true` narrows to work delegated to any AI (`assignee_kind=ai`) — an independent axis from `assignee:` (whereas `assignee:me-ai` is *your* AI); `ai:false` excludes AI-delegated work. Ordering (don't grab future work yet) is not merely shown here: the same predicate guards the reserve transition, so a `ready:no` task is rejected with not_ready even when named by number. `decision:` walks the decision⇄task link from the task side — `--filter \"decision:AMB-D-<n> status:todo\"` is the open work a decision produced — and `decision list --filter \"task:AMB-T-<n>\"` walks it from the other side (the decisions a task rests on); a ref from the wrong space (`decision:AMB-T-<n>`) is an error, not an empty result. `commit:<full sha>` walks the reverse chain git → task (the tasks that recorded a commit); a SHA is a free value, not a name the store knows, so one matching nothing is an empty result, not an error — and since the door stores full hex only, a short SHA simply matches nothing."
        },
        "notes": [
            "Amenbo assigns the id. The AI does not create with a specified id (use the id returned after creation).",
            "One task belongs to exactly one project. task move re-homes it to another project.",
            "Priority and due date are independent. A task can be both high and due in a week.",
            "The AI (--actor ai) can only delete tasks it created as the AI. Deleting tasks created by others and archiving/deleting projects are denied by ai_guardrail (ask a human; the local policy config ai_allow_project_ops can allow those). Reversible ops on the bound project itself (update/move/unarchive) are allowed — the guard covers only the destructive/hiding ones."
        ]
    })
}

/// The fields of the spec that are lines to **type**, not prose about them: every command's
/// `examples`, the `inspect` list, and `filterGrammar.example`. Named here because the retarget
/// below has to know which strings it may rewrite.
const RUNNABLE_LINE_FIELDS: [&str; 3] = ["examples", "example", "inspect"];

/// The fields that hold a **reference** rather than something a reader reads: the commands a step
/// names ([`Cmd`]), the bucket an item was found in, and a command's own name in the index. The scan
/// below judges prose by guessing — is the word after `amenbo` a command? — and a guess has no
/// business over a name that is already exact, so these are where it stops.
///
/// `commands` is two fields under one name: a step's is a list of names, and the spec's own is the
/// command registry, whose entries are objects carrying prose like any other. What tells them apart
/// is that a reference is always a leaf — the key rides down the array with the value, and only the
/// strings it reaches are left alone.
///
/// `id` is deliberately not here, though a step and a cycle both answer to one: `conventions.id` is a
/// paragraph filed under the same key, and reading a field by its name alone cannot tell the two
/// apart. Nothing is lost — an id is one lowercase word, so no rule would rewrite it either way.
const REFERENCE_FIELDS: [&str; 3] = ["commands", "kind", "name"];

/// Retargets the whole spec to `cli` — the CLI this build actually installs
/// ([`Paths::command_name`]). Everything is authored with the production spelling and rewritten on
/// the way out, so the source stays literal, readable and copy-pasteable and no author has to
/// remember to interpolate anything. Without this a dev build hands an AI commands that are not
/// installed there.
///
/// Three rules, because the spec holds three kinds of string. A runnable line
/// ([`RUNNABLE_LINE_FIELDS`]) is nothing but a command, so every standalone occurrence goes. Prose
/// spells the product `Amenbo` — the capital is the product's name, which no rule here matches —
/// but still says `amenbo`
/// wherever it words a command to type (`amenbo init`), and a following command name is what marks
/// those — a guess, and the reason the third kind exists: a reference ([`REFERENCE_FIELDS`]) is a
/// name already exact, so the guess is not run over it at all. Production rewrites nothing (the
/// authored spelling is already its own), but it still walks — one path for both channels means a
/// break shows up wherever the tests run.
fn retarget(spec: &mut Value, cli: &str) {
    let commands = command_words(spec);
    retarget_node(spec, "", cli, &commands);
}

/// Retargets one piece of **prose** written elsewhere — the `--help` text clap builds out of the doc
/// comments in the CLI's command definitions ([`crate::config::Paths::command_name`]).
///
/// The same problem as the spec's, arriving through a different door: a doc comment is a literal that
/// clap prints verbatim, so a runnable line inside one is authored with the production spelling and
/// names a command a dev build does not answer to. The derive takes literals only, and there is
/// nothing to interpolate into — but the text is a plain string by the time clap holds it, which is
/// where this reaches it. The authoring rule is therefore the same everywhere in the source: write
/// `amenbo`, and let the way out do the swapping.
///
/// It is the prose rule ([`names_a_command`]), not the runnable-line one: help text says `amenbo` for
/// the product beside `amenbo` the command, and only what follows tells them apart.
pub fn retarget_prose(text: &str) -> String {
    let commands = command_words(&spec_as_authored());
    rewrite(text, Paths::command_name(), |after| names_a_command(after, &commands))
}

/// A command name that is a plain English noun as often as it is a command, and so cannot be read as
/// one in prose: "a minimum Amenbo version" is about the product, not a line to type. Retargeting it
/// would rename the product; not retargeting it costs one prose mention of `amenbo version`, which
/// the examples carry anyway.
const NOT_A_COMMAND_IN_PROSE: [&str; 1] = ["version"];

/// The first word of each command name (`task add` → `task`) — what has to follow `amenbo` in prose
/// for it to be a command someone is being told to type. Read off the spec itself, so a command added
/// to it is covered with no second list to keep in step.
fn command_words(spec: &Value) -> HashSet<String> {
    spec["commands"]
        .as_array()
        .map(|cmds| {
            cmds.iter()
                .filter_map(|c| c["name"].as_str())
                .filter_map(|n| n.split_whitespace().next())
                .filter(|w| !NOT_A_COMMAND_IN_PROSE.contains(w))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Walks the spec, carrying the field name each string was written under: an array hands its own key
/// down, so a list of references is judged by what it is a list of rather than by being a list.
fn retarget_node(node: &mut Value, key: &str, cli: &str, commands: &HashSet<String>) {
    match node {
        Value::Object(map) => {
            for (key, value) in map.iter_mut() {
                if RUNNABLE_LINE_FIELDS.contains(&key.as_str()) {
                    retarget_line(value, cli);
                } else {
                    retarget_node(value, key, cli, commands);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|i| retarget_node(i, key, cli, commands)),
        Value::String(prose) if !REFERENCE_FIELDS.contains(&key) => {
            *prose = rewrite(prose, cli, |after| names_a_command(after, commands));
        }
        _ => {}
    }
}

/// Swaps the command word in one runnable line (or in each line of a list of them).
fn retarget_line(node: &mut Value, cli: &str) {
    match node {
        Value::Array(items) => items.iter_mut().for_each(|i| retarget_line(i, cli)),
        Value::String(line) => *line = rewrite(line, cli, |_| true),
        _ => {}
    }
}

/// Rewrites each standalone occurrence of the authored command word that `accept` takes, given the
/// text that follows it. Not merely a leading one: a line can wrap the command
/// (`eval "$(amenbo worktree start …)"`), and one that names it in the middle is as unrunnable on the
/// dev channel as one that opens with it.
fn rewrite(text: &str, cli: &str, accept: impl Fn(&str) -> bool) -> String {
    let authored = Paths::PRODUCTION_APP_NAME;
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(authored) {
        let (before, after) = (&rest[..at], &rest[at + authored.len()..]);
        out.push_str(before);
        let swap = standalone(before, after) && accept(after);
        out.push_str(if swap { cli } else { authored });
        rest = after;
    }
    out.push_str(rest);
    out
}

/// Whether the text right after the command word is a space and then a command name, or a flag — the
/// two things that make `amenbo …` in prose an instruction rather than the product's name.
///
/// A flag counts because a global one may be placed ahead of the subcommand
/// (`amenbo --project <name> decision add …`), which puts a dash where the command word would
/// otherwise be. Nothing is lost to it: prose about the product never carries a flag behind the name.
fn names_a_command(after: &str, commands: &HashSet<String>) -> bool {
    let Some(tail) = after.strip_prefix(' ') else { return false };
    if tail.starts_with('-') {
        return true;
    }
    let word: String = tail.chars().take_while(|c| c.is_ascii_lowercase() || *c == '-').collect();
    commands.contains(&word)
}

/// Whether the command word between these two sides is a word of its own. What may touch it is
/// punctuation a shell puts there — a space, a quote, `$(`, `)`. What may not is anything that would
/// make it part of a longer name: `amenbo-dev` (already retargeted), `.amenbo` (the pointer file),
/// `work.amenbo.amenbo` (an app-data path).
fn standalone(before: &str, after: &str) -> bool {
    let edge = |c: char| !(c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | '/'));
    before.chars().next_back().is_none_or(edge) && after.chars().next().is_none_or(edge)
}

/// Declares [`Cmd`] — the variant a command is named by, and the name the registry answers to — in
/// one table, so the two can never be written apart.
macro_rules! commands {
    ($($variant:ident => $name:literal,)*) => {
        /// **Every command, as a value rather than a spelling.** A step used to carry the name as
        /// text, where a command renamed or dropped left a reference that read exactly like a working
        /// one and that nothing caught until an AI typed it. Naming one is now `Cmd::TaskAdd`, and a
        /// command that is not in the table below does not compile (`AMB-D-574`: what a type can
        /// refuse never needs a test).
        ///
        /// It holds every command the registry ([`all_commands`]) carries, not only the ones a step
        /// names, because it is also where each one says whether a run's step may type it
        /// ([`Cmd::in_a_step`], `AMB-D-968`). The registry is still written as text, so
        /// `the_table_and_the_registry_name_the_same_commands` holds the two against each other in
        /// both directions.
        #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
        pub enum Cmd { $($variant,)* }

        impl Cmd {
            /// Every variant, in the registry's order.
            pub const ALL: &'static [Cmd] = &[$(Cmd::$variant,)*];

            /// The name the registry answers to — what is emitted, and what the prose spells out.
            pub const fn name(self) -> &'static str {
                match self { $(Cmd::$variant => $name,)* }
            }
        }
    };
}

commands! {
    Amenbo => "amenbo",
    Agent => "agent",
    Version => "version",
    Update => "update",
    Notify => "notify",
    NotifyTargetList => "notify target-list",
    NotifyTargetAdd => "notify target-add",
    NotifyTargetSet => "notify target-set",
    NotifyTargetDefault => "notify target-default",
    NotifyTargetRm => "notify target-rm",
    NotifyTargetCheck => "notify target-check",
    NotifyTargetTest => "notify target-test",
    NotifyOn => "notify on",
    NotifyOff => "notify off",
    NotifyUse => "notify use",
    NotifyUnuse => "notify unuse",
    NotifyTo => "notify to",
    NotifyEvent => "notify event",
    ViewerSetup => "viewer setup",
    ViewerQr => "viewer qr",
    ViewerApp => "viewer app",
    ViewerPhones => "viewer phones",
    ViewerRevoke => "viewer revoke",
    ViewerSend => "viewer send",
    ViewerRepair => "viewer repair",
    Config => "config",
    ConfigSet => "config set",
    Whoami => "whoami",
    Init => "init",
    Bind => "bind",
    Unbind => "unbind",
    Status => "status",
    Search => "search",
    Activity => "activity",
    SyncGuide => "sync-guide",
    Doctor => "doctor",
    Validate => "validate",
    Lint => "lint",
    WorktreeStart => "worktree start",
    WorktreeFinish => "worktree finish",
    HooksInstall => "hooks install",
    HooksUninstall => "hooks uninstall",
    HooksStatus => "hooks status",
    TickInstall => "tick install",
    TickUninstall => "tick uninstall",
    TickStatus => "tick status",
    AgentHookSnippet => "agent-hook snippet",
    AgentHookAnswer => "agent-hook answer",
    Mcp => "mcp",
    ProjectAdd => "project add",
    ProjectList => "project list",
    ProjectShow => "project show",
    ProjectUpdate => "project update",
    ProjectMove => "project move",
    ProjectArchive => "project archive",
    ProjectUnarchive => "project unarchive",
    ProjectDelete => "project delete",
    DimensionAdd => "dimension add",
    DimensionList => "dimension list",
    DimensionShow => "dimension show",
    DimensionUpdate => "dimension update",
    DimensionMove => "dimension move",
    DimensionRm => "dimension rm",
    DimensionValueAdd => "dimension value-add",
    DimensionValueUpdate => "dimension value-update",
    DimensionValueMove => "dimension value-move",
    DimensionValueClose => "dimension value-close",
    DimensionValueReopen => "dimension value-reopen",
    DimensionValueRm => "dimension value-rm",
    DimensionSet => "dimension set",
    DimensionUnset => "dimension unset",
    TaskAdd => "task add",
    TaskFinishCreating => "task finish-creating",
    TaskList => "task list",
    TaskShow => "task show",
    TaskUpdate => "task update",
    TaskDone => "task done",
    TaskReject => "task reject",
    TaskReopen => "task reopen",
    TaskStatus => "task status",
    TaskBlock => "task block",
    TaskMove => "task move",
    TaskDepend => "task depend",
    TaskUndepend => "task undepend",
    TaskCommitAdd => "task commit-add",
    TaskCommitList => "task commit-list",
    TaskCommitRm => "task commit-rm",
    TaskAssign => "task assign",
    TaskUnassign => "task unassign",
    TaskDelete => "task delete",
    CommentRm => "comment rm",
    CommentEdit => "comment edit",
    CommentAdd => "comment add",
    CommentList => "comment list",
    CommentAttach => "comment attach",
    DecisionCommentAttach => "decision comment-attach",
    DecisionAdd => "decision add",
    DecisionList => "decision list",
    DecisionShow => "decision show",
    DecisionEdit => "decision edit",
    DecisionFinishWriting => "decision finish-writing",
    DecisionReject => "decision reject",
    DecisionReopen => "decision reopen",
    DecisionDelete => "decision delete",
    DecisionSupersede => "decision supersede",
    DecisionAmend => "decision amend",
    DecisionBuildsOn => "decision builds-on",
    DecisionUnlink => "decision unlink",
    DecisionLink => "decision link",
    DecisionPromote => "decision promote",
    DecisionCommentRm => "decision comment-rm",
    DecisionCommentEdit => "decision comment-edit",
    DecisionCommentAdd => "decision comment-add",
    DecisionCommentList => "decision comment-list",
    TaskAttach => "task attach",
    DecisionAttach => "decision attach",
    AttachLs => "attach ls",
    AttachShow => "attach show",
    AttachOpen => "attach open",
    AttachSave => "attach save",
    AttachRm => "attach rm",
    Export => "export",
    Backup => "backup",
    SkinList => "skin list",
    SkinAdd => "skin add",
    SkinUse => "skin use",
    SkinRm => "skin rm",
    SkinValidate => "skin validate",
    SkinTemplate => "skin template",
    SkinWriteOut => "skin write-out",
    Restore => "restore",
    HardEraseComment => "hard-erase comment",
    HardEraseDecisionComment => "hard-erase decision-comment",
    HardEraseDecision => "hard-erase decision",
    AutomationAdd => "automation add",
    AutomationUpdate => "automation update",
    AutomationRm => "automation rm",
    AutomationList => "automation list",
    AutomationShow => "automation show",
    AutomationEntrySet => "automation entry-set",
    AutomationPlaceAdd => "automation place-add",
    AutomationPlaceRm => "automation place-rm",
    AutomationStart => "automation start",
    AutomationPause => "automation pause",
    AutomationResume => "automation resume",
    AutomationStop => "automation stop",
    AutomationStepTake => "automation step-take",
    AutomationStepOut => "automation step-out",
    AutomationStepDone => "automation step-done",
    AutomationActionAdd => "automation action-add",
    AutomationActionList => "automation action-list",
    AutomationActionShow => "automation action-show",
    AutomationActionUpdate => "automation action-update",
    AutomationActionEntrySet => "automation action-entry-set",
    AutomationActionScopeSet => "automation action-scope-set",
    AutomationActionRm => "automation action-rm",
    AutomationStepAdd => "automation step-add",
    AutomationStepUpdate => "automation step-update",
    AutomationStepRm => "automation step-rm",
    AutomationExitAdd => "automation exit-add",
    AutomationExitRename => "automation exit-rename",
    AutomationExitRm => "automation exit-rm",
    AutomationPortAdd => "automation port-add",
    AutomationPortUpdate => "automation port-update",
    AutomationPortRm => "automation port-rm",
    AutomationCfgAdd => "automation cfg-add",
    AutomationCfgUpdate => "automation cfg-update",
    AutomationCfgSet => "automation cfg-set",
    AutomationAgentSet => "automation agent-set",
    AutomationCfgRm => "automation cfg-rm",
    AutomationEdgeAdd => "automation edge-add",
    AutomationEdgeUpdate => "automation edge-update",
    AutomationEdgeRm => "automation edge-rm",
    AutomationWireAdd => "automation wire-add",
    AutomationWireRm => "automation wire-rm",
    AutomationRunList => "automation run-list",
    AutomationRunShow => "automation run-show",
}

/// Whether a command reaches the terminal a run opened for a step. It is an allow-list: a command
/// reaches only where [`Cmd::in_a_step`] says so, so one written down in the wrong arm, or added
/// without thought, lands on the side that is refused rather than the one that quietly lets it
/// through (`AMB-D-968`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InAStep {
    /// One of the three that hand the step's work back. The step's entry teaches them in full.
    HandsBack,
    /// Reaches the step, and does not hand anything back: it reads, it writes a comment, or it is
    /// something the prompts a run is carried out on still do for themselves.
    Reaches,
    /// Refused: it moves a task's status or who it is assigned to. Taking a task, ending it, and
    /// handing it to a person are the run's, and a step that did them itself would close a task with
    /// no commit on it, or hand it to another run that takes it straight back.
    MovesTheTask,
    /// Refused: it belongs outside a run. A step that could start a run would let a run make runs,
    /// and one that rewrote a definition or a setting would change what comes next where the person
    /// who started the run is not looking.
    OutsideARun,
}

impl Cmd {
    /// **Where each command may be typed, as a match over every one of them** — what the list a
    /// step's entry teaches ([`build_step`]) is made from, and the refusal inside a step with it (that
    /// one is still the CLI's `typed_in`, over the `automation` verbs alone, until `AMB-T-5470`).
    ///
    /// **There is no `_` arm, deliberately.** A command added to the table does not compile until
    /// this says which side it is on.
    pub const fn in_a_step(self) -> InAStep {
        match self {
            Cmd::AutomationStepTake
            | Cmd::AutomationStepOut
            | Cmd::AutomationStepDone => InAStep::HandsBack,

            // To read where the step stands. `attach save` is on this side because it is how an agent
            // reads an attachment; `attach open` is not, since it puts the file in front of the person.
            Cmd::Amenbo
            | Cmd::Agent
            | Cmd::Version
            | Cmd::Notify
            | Cmd::NotifyTargetList
            | Cmd::Config
            | Cmd::Whoami
            | Cmd::Status
            | Cmd::Search
            | Cmd::Activity
            | Cmd::Doctor
            | Cmd::Validate
            | Cmd::Lint
            | Cmd::HooksStatus
            | Cmd::TickStatus
            | Cmd::ProjectList
            | Cmd::ProjectShow
            | Cmd::DimensionList
            | Cmd::DimensionShow
            | Cmd::TaskList
            | Cmd::TaskShow
            | Cmd::TaskCommitList
            | Cmd::CommentList
            | Cmd::DecisionList
            | Cmd::DecisionShow
            | Cmd::DecisionCommentList
            | Cmd::AttachLs
            | Cmd::AttachShow
            | Cmd::AttachSave
            | Cmd::SkinList
            | Cmd::AutomationList
            | Cmd::AutomationShow
            | Cmd::AutomationActionList
            | Cmd::AutomationActionShow
            | Cmd::AutomationRunList
            | Cmd::AutomationRunShow
            // The timeline is the step's to write on, a task's and a decision's alike.
            | Cmd::CommentAdd
            | Cmd::CommentEdit
            | Cmd::CommentAttach
            | Cmd::DecisionCommentAdd
            | Cmd::DecisionCommentEdit
            | Cmd::DecisionCommentAttach
            // What the prompts a run is carried out on still type for themselves. An entry step may
            // file the task it then takes; the worktree and the commit are Amenbo's to handle once the
            // built-in steps do it (`AMB-D-964`), and until then a step that could not reach them
            // could not do its work. `mcp` is a host's, and every call through it is this table's
            // again, since it re-runs this executable.
            | Cmd::TaskAdd
            | Cmd::TaskFinishCreating
            | Cmd::TaskCommitAdd
            | Cmd::WorktreeStart
            | Cmd::WorktreeFinish
            | Cmd::Mcp => InAStep::Reaches,

            Cmd::TaskStatus
            | Cmd::TaskDone
            | Cmd::TaskReject
            | Cmd::TaskReopen
            | Cmd::TaskBlock
            | Cmd::TaskAssign
            | Cmd::TaskUnassign => InAStep::MovesTheTask,

            Cmd::Update
            | Cmd::NotifyTargetAdd
            | Cmd::NotifyTargetSet
            | Cmd::NotifyTargetDefault
            | Cmd::NotifyTargetRm
            | Cmd::NotifyTargetCheck
            | Cmd::NotifyTargetTest
            | Cmd::NotifyOn
            | Cmd::NotifyOff
            | Cmd::NotifyUse
            | Cmd::NotifyUnuse
            | Cmd::NotifyTo
            | Cmd::NotifyEvent
            | Cmd::ViewerSetup
            | Cmd::ViewerQr
            | Cmd::ViewerApp
            | Cmd::ViewerPhones
            | Cmd::ViewerRevoke
            | Cmd::ViewerSend
            | Cmd::ViewerRepair
            | Cmd::ConfigSet
            | Cmd::Init
            | Cmd::Bind
            | Cmd::Unbind
            | Cmd::SyncGuide
            | Cmd::HooksInstall
            | Cmd::HooksUninstall
            | Cmd::TickInstall
            | Cmd::TickUninstall
            | Cmd::AgentHookSnippet
            | Cmd::AgentHookAnswer
            | Cmd::ProjectAdd
            | Cmd::ProjectUpdate
            | Cmd::ProjectMove
            | Cmd::ProjectArchive
            | Cmd::ProjectUnarchive
            | Cmd::ProjectDelete
            | Cmd::DimensionAdd
            | Cmd::DimensionUpdate
            | Cmd::DimensionMove
            | Cmd::DimensionRm
            | Cmd::DimensionValueAdd
            | Cmd::DimensionValueUpdate
            | Cmd::DimensionValueMove
            | Cmd::DimensionValueClose
            | Cmd::DimensionValueReopen
            | Cmd::DimensionValueRm
            | Cmd::DimensionSet
            | Cmd::DimensionUnset
            | Cmd::TaskUpdate
            | Cmd::TaskMove
            | Cmd::TaskDepend
            | Cmd::TaskUndepend
            | Cmd::TaskCommitRm
            | Cmd::TaskDelete
            | Cmd::CommentRm
            | Cmd::DecisionAdd
            | Cmd::DecisionEdit
            | Cmd::DecisionFinishWriting
            | Cmd::DecisionReject
            | Cmd::DecisionReopen
            | Cmd::DecisionDelete
            | Cmd::DecisionSupersede
            | Cmd::DecisionAmend
            | Cmd::DecisionBuildsOn
            | Cmd::DecisionUnlink
            | Cmd::DecisionLink
            | Cmd::DecisionPromote
            | Cmd::DecisionCommentRm
            | Cmd::TaskAttach
            | Cmd::DecisionAttach
            | Cmd::AttachOpen
            | Cmd::AttachRm
            | Cmd::Export
            | Cmd::Backup
            | Cmd::SkinAdd
            | Cmd::SkinUse
            | Cmd::SkinRm
            | Cmd::SkinValidate
            | Cmd::SkinTemplate
            | Cmd::SkinWriteOut
            | Cmd::Restore
            | Cmd::HardEraseComment
            | Cmd::HardEraseDecisionComment
            | Cmd::HardEraseDecision
            | Cmd::AutomationAdd
            | Cmd::AutomationUpdate
            | Cmd::AutomationRm
            | Cmd::AutomationEntrySet
            | Cmd::AutomationPlaceAdd
            | Cmd::AutomationPlaceRm
            | Cmd::AutomationStart
            | Cmd::AutomationPause
            | Cmd::AutomationResume
            | Cmd::AutomationStop
            | Cmd::AutomationActionAdd
            | Cmd::AutomationActionUpdate
            | Cmd::AutomationActionEntrySet
            | Cmd::AutomationActionScopeSet
            | Cmd::AutomationActionRm
            | Cmd::AutomationStepAdd
            | Cmd::AutomationStepUpdate
            | Cmd::AutomationStepRm
            | Cmd::AutomationExitAdd
            | Cmd::AutomationExitRename
            | Cmd::AutomationExitRm
            | Cmd::AutomationPortAdd
            | Cmd::AutomationPortUpdate
            | Cmd::AutomationPortRm
            | Cmd::AutomationCfgAdd
            | Cmd::AutomationCfgUpdate
            | Cmd::AutomationCfgSet
            | Cmd::AutomationAgentSet
            | Cmd::AutomationCfgRm
            | Cmd::AutomationEdgeAdd
            | Cmd::AutomationEdgeUpdate
            | Cmd::AutomationEdgeRm
            | Cmd::AutomationWireAdd
            | Cmd::AutomationWireRm => InAStep::OutsideARun,
        }
    }
}

/// Declares [`Cyc`] — the variant a step names a cold-path cycle by, and the key that cycle is
/// emitted under — in one table, the way [`Cmd`] is declared for commands.
macro_rules! cycle_ids {
    ($($variant:ident => $key:literal,)*) => {
        /// A cold-path cycle, as a value rather than a spelling. Both ends of the branch are written
        /// with it: [`CYCLES`] files each cycle under one, and [`Step::cycles`] names the ones that
        /// can fire at that step, so a step pointing at a cycle nobody wrote does not compile
        /// (`AMB-D-574`: what a type can refuse never needs a test). What a type cannot refuse is a
        /// cycle no step points *at* — `every_cycle_is_reachable_from_a_step` is that half.
        #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
        pub enum Cyc { $($variant,)* }

        impl Cyc {
            /// The key the cycle is emitted under, and the name a step branches to it by.
            pub const fn key(self) -> &'static str {
                match self { $(Cyc::$variant => $key,)* }
            }
        }
    };
}

cycle_ids! {
    TaskShaping => "taskShaping",
    Decision => "decision",
    DecisionAudit => "decisionAudit",
    ExecutionExceptions => "executionExceptions",
    Commit => "commit",
    Worktree => "worktree",
}

/// One step of a run — the hot-path backbone ([`AGENT_CYCLE`]) and the cold-path cycles
/// ([`CYCLES`]) are both written out of this. It is a **structure with an id**, not a numbered line
/// of prose: the number and the name used to live inside the sentence, where nothing but a substring
/// match could reach them, so no test could say more than "that text is in there somewhere" and no
/// cross-reference could be checked at all.
///
/// [`Step::n`] and [`Step::trigger`] are the two ways a step can be reached, and every step declares
/// at least one of them: `n` says *this is a place in a run* — arrive at it by having done the one
/// before — and `trigger` says *this fires when*. The hot path is mostly the first (an entry point
/// carries both), a cycle's `optional` items are only ever the second.
///
/// What it does **not** carve up is the writing. One step is one block of prose, authored whole —
/// [`Step::prose`] is the last field for that reason. Cutting it into per-aspect fields would turn
/// the job from "write so it lands" into "fill the boxes", and the output would read like a
/// generated document, which is the opposite of the point.
struct Step {
    /// What everything else addresses this step by, and what a machine can hold. Unique within the
    /// run it belongs to.
    id: &'static str,
    /// Where the step sits in the run, and — on the backbone — the number the prose elsewhere names
    /// it by ("step 0 puts the request there", "defined once in agentCycle step 1"). Held to its
    /// place in the array by a test, so the two can never drift. Absent where there is no run to sit
    /// in: a self-gated `optional` item is reached by its trigger and nothing else, and numbering it
    /// would invent an order the AI would then feel bound by.
    n: Option<u8>,
    /// What puts you *at* this step to begin with. On the backbone only an entry point has one —
    /// every other step is simply the next, and declaring a trigger there would invite entering the
    /// run in the middle. An `optional` item is the other way round: the trigger is the whole of why
    /// it is ever done, so it always carries one.
    trigger: Option<&'static str>,
    /// The commands the step tells you to run. The prose names them too, in the sentences that say
    /// when and why; this is the same set with the sentence taken off, and a reference rather than a
    /// spelling ([`Cmd`]), so one that names nothing is a build error rather than something to test
    /// for.
    commands: &'static [Cmd],
    /// The cold-path cycles that can fire here — the branch the backbone's description tells the
    /// reader to take when a trigger fires, written down rather than left to be inferred from a
    /// cycle's `when`. Empty where nothing branches off this step, which is most of them.
    ///
    /// It is what makes the branch checkable from both ends: naming one is [`Cyc`], so a branch to a
    /// cycle nobody wrote is a build error, and a cycle nothing here names is unreachable — written,
    /// emitted, paid for by every session, and arrived at by no one.
    cycles: &'static [Cyc],
    /// The step itself, in one block.
    prose: &'static str,
}

impl Step {
    /// The step as it is emitted. `kind` is not the step's own — it is which bucket of a cycle the
    /// step was found in, so the caller that knows passes it in, and the backbone (which has one
    /// bucket and needs no word for it) passes `None`.
    fn to_value(&self, kind: Option<&str>) -> Value {
        let mut value = json!({
            "id": self.id,
            "commands": self.commands.iter().map(|c| c.name()).collect::<Vec<&str>>(),
            "step": self.prose,
        });
        let Some(map) = value.as_object_mut() else { return value };
        if let Some(kind) = kind {
            map.insert("kind".to_string(), json!(kind));
        }
        if let Some(n) = self.n {
            map.insert("n".to_string(), json!(n));
        }
        if let Some(trigger) = self.trigger {
            map.insert("trigger".to_string(), json!(trigger));
        }
        if !self.cycles.is_empty() {
            map.insert("cycles".to_string(), json!(self.cycles.iter().map(|c| c.key()).collect::<Vec<&str>>()));
        }
        value
    }
}

/// The hot path: the backbone every session runs, in order. Authored here as steps and assembled by
/// [`agent_cycle`] — the emitted `agentCycle` is this list, and there is nowhere else it is written.
const AGENT_CYCLE: &[Step] = &[
    Step {
        id: "intake",
        n: Some(0),
        trigger: Some("the human handed you work in this session"),
        commands: &[Cmd::TaskAdd, Cmd::TaskFinishCreating, Cmd::TaskList, Cmd::DecisionAdd],
        cycles: &[Cyc::TaskShaping, Cyc::Decision],
        prose: "work the human hands you in this session lands here first, and the question to ask of it is what it leaves behind. **What to do** belongs in a task if it is not there yet, and filing it takes two commands: `amenbo task add ... --to me-ai` when you are the one continuing, then `amenbo task finish-creating <id>` once the dependencies, premises and classification it needs are on it — until that lands nobody can reserve it, you included. Look first at what is already being created (`task list --filter \"draft:yes\"`): a creation someone left open is the task you would otherwise file twice. Then join at step 2 (reserve) with the id `add` handed back. **Why it is being done this way** — a choice picked among real alternatives — is worth offering as a decision (`amenbo decision add`), and one request can leave both. Asking once, here, is the whole of it: no later step asks again. Work you pulled out of Amenbo yourself is already recorded, so it needs no intake — enter at step 1.",
    },
    Step {
        id: "list",
        n: Some(1),
        trigger: Some("you are going looking for work yourself"),
        commands: &[Cmd::TaskList],
        cycles: &[Cyc::DecisionAudit],
        prose: "your mailbox is `amenbo task list --filter \"assignee:me-ai status:todo ready:yes\" --sort priority --json`. Take the highest-priority task; if it comes back empty, widen once to `assignee:none status:todo ready:yes` and assign what you take. (`status:todo` is fresh, unreserved work — a task already `in_progress` is one another session is on, so it stays out of the mailbox and you never double-book; `blocked` is deliberately out, being an external stall — a second machine, a human go/no-go — that you should not self-assign. Waiting on a ruling, on a day, or on the writing being finished is not one of those: an unsettled premise, a start day still ahead and a creation still open are all derived as `ready:no`, never declared as `blocked`. `ready:yes` hides work whose declared premises are unmet — an open blocker, a linked decision that is not settled, a start day still ahead, or a creation nobody has finished — so you never grab it early; query `ready:no` to see each task's blocked_by_open, blocked_by_decisions, not_started_until and draft, `start:future` for the queue waiting on its day alone, and `draft:yes` for the creations still open.)",
    },
    Step {
        id: "reserve",
        n: Some(2),
        trigger: None,
        commands: &[
            Cmd::TaskStatus,
            Cmd::TaskShow,
            Cmd::TaskUndepend,
            Cmd::DecisionFinishWriting,
            Cmd::DecisionLink,
            Cmd::TaskUpdate,
            Cmd::TaskFinishCreating,
        ],
        cycles: &[Cyc::Worktree, Cyc::ExecutionExceptions],
        prose: "`amenbo task status <id> in_progress` (todo→in_progress), then re-confirm it is in_progress with `amenbo task show <id>` before starting. `status` is the whole double-work guard, and reserving is a compare-and-swap: `→ in_progress` succeeds only when the task is currently `todo`. If another session reserved it first, your reserve is rejected with `already_reserved` (a non-zero exit). The reserve also requires `ready`, so a task with an open blocker, an unsettled premise, a start day still ahead, or a creation still unfinished is rejected with `not_ready` (also a non-zero exit), and there is no `--force`. The two failures pull in opposite directions. On `already_reserved` the task is taken by someone else: go back to step 1 and pick the next one. On `not_ready` it is your own declaration that holds it: resolve the premise — finish the blocker or `task undepend` it; `decision finish-writing` the linked decision, or `decision link --unlink` it; correct a start day you declared wrong with `task update <id> --start today` (or `--clear-start`); end a creation that is still open with `task finish-creating <id>` — and reserve only then. Both guards judge this transition only: an edge added, or a decision rejected, under a running task never strips its status, and `→ todo` stays unconditional, so the hand-back path is never closed. While a creation is unfinished the task stays at `todo`: `→ done` / `→ blocked` / `→ rejected` are refused as `invalid_task_status_draft`; a draft leaves by `task delete`.",
    },
    Step {
        id: "execute",
        n: Some(3),
        trigger: None,
        commands: &[Cmd::CommentList, Cmd::TaskStatus],
        cycles: &[Cyc::Commit, Cyc::TaskShaping, Cyc::Decision, Cyc::ExecutionExceptions],
        prose: "first read the task's latest comments and whatever they reference — linked notes, decisions, attachments (`amenbo comment list <id>`) — if the direction feels undecided, you have not read enough; then do the work, moving state with `amenbo task status <id> blocked` if you stall.",
    },
    Step {
        id: "finish",
        n: Some(4),
        trigger: None,
        commands: &[Cmd::TaskDone, Cmd::TaskReject, Cmd::TaskStatus],
        cycles: &[Cyc::Worktree],
        prose: "a task ends one of two ways — `amenbo task done <id> --report ...` for work you carried out, `amenbo task reject <id> --reason ...` for work you concluded should not be done (neither `done` nor `delete`). Either way, the text lands on the task's timeline as a comment in the same write as the transition, so the next session can pick up. (If you decide not to take a reserved task — as opposed to deciding it should not be done at all — hand it back with `amenbo task status <id> todo`.)",
    },
];

/// Assembles [`AGENT_CYCLE`] into the emitted shape: the standing description, then the steps in
/// order, each carrying the id that names it. Where a step used to open with `0. intake:`, the
/// number and the name are now fields beside the prose — one place, readable by a machine, and no
/// longer said twice.
fn agent_cycle() -> Value {
    json!({
        "description": "The AI's recommended execution backbone (pass --actor ai on every command) — a proven default for proceeding autonomously while avoiding parallel collisions, not a mandate. When you follow it, enter at the step whose `trigger` describes where you are and run from there in order; a step with no trigger is simply the next one. A step's `cycles` are the cold-path cycles that branch off it — take one only when its own trigger fires, then come back. **None of it applies inside a step of an automation run** — a terminal a run opened, where `agent --json` answers with the step's own entry instead of this one.",
        "steps": AGENT_CYCLE.iter().map(|s| s.to_value(None)).collect::<Vec<Value>>(),
    })
}

/// cold path: the trigger-indexed catalog the backbone branches to. Each cycle separates a
/// `backbone` — what always applies within that cycle, in order — from `optional` items, done only
/// when the item's `trigger` situation matches. This is how Amenbo *shows what you can do* without
/// directing you to do it: the AI self-gates on the trigger.
///
/// The items are [`Step`]s, the same structure the hot path is written in, so every one of them has
/// an id to be named by and the two runs can be checked by one rule rather than two.
struct Cycle {
    /// Which cycle this is: the key it is emitted under, and the value a step names it by when it
    /// branches here ([`Cyc`]). One variant is written by exactly one cycle, held by
    /// `cycles_are_addressable`.
    id: Cyc,
    /// The situation that sends you into this cycle at all — the branch condition, above the
    /// per-item triggers inside it.
    when: &'static str,
    /// What always applies within the cycle, in order. Each item carries its place as
    /// [`Step::n`].
    backbone: &'static [Step],
    /// What is done only when the item's own trigger matches. No order and no numbers: these are
    /// self-gated, and numbering them would invent a sequence the AI would then feel bound by.
    optional: &'static [Step],
}

impl Cycle {
    fn to_value(&self) -> Value {
        json!({
            "when": self.when,
            "backbone": self.backbone.iter().map(|s| s.to_value(Some("backbone"))).collect::<Vec<Value>>(),
            "optional": self.optional.iter().map(|s| s.to_value(Some("optional"))).collect::<Vec<Value>>(),
        })
    }
}

/// Every cold-path cycle, in the order they are emitted. Referencing only real command names, and
/// carrying ids nothing else answers to, is guarded by the `cycles_are_addressable` test; that one of
/// these is reached at all, by `every_cycle_is_reachable_from_a_step`.
const CYCLES: &[Cycle] = &[
    Cycle {
        id: Cyc::TaskShaping,
        when: "You are registering or decomposing work.",
        backbone: &[
            Step {
                id: "decompose",
                n: Some(0),
                trigger: None,
                commands: &[Cmd::TaskAdd],
                cycles: &[],
                prose: "There are no subtasks: decompose larger work into separate tasks, each belonging to exactly one project.",
            },
            Step {
                id: "order",
                n: Some(1),
                trigger: None,
                commands: &[Cmd::TaskDepend, Cmd::TaskUndepend],
                cycles: &[],
                prose: "Before you leave the split, draw a dependency edge wherever one task has to be done first: the edge is what holds the order once the session that knew it is gone, so whoever arrives later takes the work in the order it has rather than inferring it from titles. Looking is the step, not the linking — parts with no order between them are an answer. Declare the order you actually mean.",
            },
            Step {
                id: "finish-creating",
                n: Some(2),
                trigger: None,
                commands: &[Cmd::TaskFinishCreating],
                cycles: &[],
                prose: "End each creation once its task is written — the edges, the premises and the classification all on it. Until then it cannot be reserved, which is what keeps a half-written task from being taken mid-split. Filing several at once, create them all, draw the edges between them, and finish them last.",
            },
            Step {
                id: "settle-the-parent",
                n: Some(3),
                trigger: None,
                commands: &[Cmd::TaskDone, Cmd::TaskReject],
                cycles: &[],
                prose: "Settle the task you split from, once the parts are filed and the edges are drawn. With no subtasks it never becomes a parent that empties itself: either it keeps one of the parts as its own work, or it is closed — done if the work moved out of it entirely, rejected if it turned out not to be work at all. Left standing with nothing in it, it comes back to whoever reads the tasks next, who finds nothing there to do.",
            },
        ],
        optional: &[
            Step {
                id: "time-axis",
                n: None,
                trigger: Some("the work spans ordered time-direction stages"),
                commands: &[Cmd::DimensionAdd, Cmd::DimensionSet],
                cycles: &[],
                prose: "Add a --time-axis dimension and place tasks on its ordered values, gating later stages behind earlier ones with a dependency.",
            },
            Step {
                id: "delegate",
                n: None,
                trigger: Some("you are handing work to a person, that person's AI, or yourself (`--to me-ai`) to continue"),
                commands: &[Cmd::TaskAdd, Cmd::TaskAssign],
                cycles: &[],
                prose: "Delegate at creation (--to/--ai) or afterward.",
            },
            Step {
                id: "record-the-why",
                n: None,
                trigger: Some("the work rests on a choice worth recording"),
                commands: &[Cmd::DecisionAdd, Cmd::DecisionLink],
                cycles: &[Cyc::Decision],
                prose: "Record the rationale as a decision when the choice was settled with the human — the `decision` cycle carries it from there, linking it to the tasks that implement it. A choice you made on your own is not one of these: comment it on the task instead (`AMB-D-917`).",
            },
            Step {
                id: "check-the-shape",
                n: None,
                trigger: Some("a task's shape looks off (missing project/priority, malformed)"),
                commands: &[Cmd::Validate],
                cycles: &[],
                prose: "Check its shape before relying on it.",
            },
        ],
    },
    Cycle {
        id: Cyc::Decision,
        when: "You are putting a 'why' on the record — writing a new one, or fitting it to the decisions already there (step 0 is where you judge that it is worth recording).",
        backbone: &[
            Step {
                id: "read-the-neighbourhood",
                n: Some(0),
                trigger: None,
                commands: &[Cmd::Search, Cmd::DecisionShow],
                cycles: &[],
                prose: "Read the neighbourhood before you write, not after — a duplicate you find first is one you never propose. Pull a bounded, relevant slice (search <term> --kind decision --limit N), not the whole corpus, and read the bodies it points at (decision show). Don't settle for one term: prose is written in the human's language while identifiers and code spans stay English, so pull again on another word for the same idea. If one contradicts what you are about to record, propose it to the human as a candidate supersede/amend with your reasoning; do not author the edge yourself (detection proposes, the human disposes).",
            },
            Step {
                id: "freeze-the-rationale",
                n: Some(1),
                trigger: None,
                commands: &[Cmd::DecisionAdd, Cmd::DecisionPromote],
                cycles: &[],
                prose: "Freeze the rationale as an append-only decision — it opens as a draft, which says the writing is unfinished, not that a ruling is pending (`AMB-D-918`).",
            },
            Step {
                id: "link-the-work",
                n: Some(2),
                trigger: None,
                commands: &[Cmd::DecisionLink],
                cycles: &[],
                prose: "Before you leave it, look for the work this decision governs and link it: the task then names the decision it rests on, and the decision names the work it produced, so whoever picks the work up reads why. Looking is the step, not the linking — finding nothing to link is an answer. Link implementation tasks only: never the task of ruling on the decision itself, and never a rejected decision, or one that has been superseded, that you merely want to cite (that belongs in the body).",
            },
            Step {
                id: "end-the-writing",
                n: Some(3),
                trigger: None,
                commands: &[Cmd::DecisionFinishWriting],
                cycles: &[],
                prose: "End the writing yourself — `decision finish-writing` lowers the draft flag, settles it and stamps who finished it. Until then every task you linked reads ready:no, so a half-written decision stalls the work it governs. It is also where a classification the project requires of decisions is read, so an axis left blank holds the writing. Then say what you recorded and what rests on it.",
            },
        ],
        optional: &[
            Step {
                id: "supersede",
                n: None,
                trigger: Some("a new decision wholly replaces an existing one"),
                commands: &[Cmd::DecisionSupersede],
                cycles: &[],
                prose: "Chain it as a supersession — the old one stops being current (the edge says so; no status changes).",
            },
            Step {
                id: "amend",
                n: None,
                trigger: Some("a new decision partially revises an existing one that stays current"),
                commands: &[Cmd::DecisionAmend],
                cycles: &[],
                prose: "Chain it as an amendment — the old one is not superseded; read the two together.",
            },
            Step {
                id: "edit-in-place",
                n: None,
                trigger: Some("a settled decision needs a minor fix (typo / stale line)"),
                commands: &[Cmd::DecisionEdit],
                cycles: &[],
                prose: "Edit it in place — settling it does not freeze the body, so there is no reopen/re-settle round-trip. Supersede stays for a change of mind; reopen for un-settling a too-hasty one.",
            },
            Step {
                id: "check-for-contradiction",
                n: None,
                trigger: Some("you just recorded a decision, or finished writing one"),
                commands: &[Cmd::Search, Cmd::DecisionShow, Cmd::DecisionSupersede, Cmd::DecisionAmend],
                cycles: &[],
                prose: "Check whether it semantically contradicts an existing one. Pull a bounded, relevant neighbourhood — search the new decision's key terms (search <term> --kind decision --limit N), not the whole corpus — and read the bodies it points at (decision show). If one contradicts, propose it to the human as a candidate supersede/amend with your reasoning; do not author the edge yourself (detection proposes, the human disposes).",
            },
        ],
    },
    Cycle {
        id: Cyc::DecisionAudit,
        when: "Periodically — contradictions accumulate over time as new decisions land far from older ones.",
        backbone: &[
            Step {
                id: "sweep",
                n: Some(0),
                trigger: None,
                commands: &[Cmd::DecisionList],
                cycles: &[],
                prose: "Sweep the decisions that are settled and written out for semantically contradicting pairs, but keep each pass bounded: page through a slice you can actually reason over (decision list --filter \"status:decided draft:no\" --with-body --limit N --offset …) rather than loading everything at once, and rotate the window across passes. Detection is best-effort recall by design — as the corpus outgrows one pass, coverage stays bounded to what you can reach now and repeated passes widen it over time.",
            },
            Step {
                id: "surface-the-pairs",
                n: Some(1),
                trigger: None,
                commands: &[Cmd::DecisionSupersede, Cmd::DecisionAmend],
                cycles: &[],
                prose: "Surface each suspected contradiction to the human as a candidate supersede/amend with your reasoning. Never author the edge yourself, and only run supersede/amend once the human confirms — detection proposes, the human disposes, so a false positive cannot silently kill a good decision.",
            },
        ],
        optional: &[],
    },
    Cycle {
        id: Cyc::ExecutionExceptions,
        when: "You hit something that breaks the straight-line cycle.",
        backbone: &[
            Step {
                id: "state-is-not-assignment",
                n: Some(0),
                trigger: None,
                commands: &[],
                cycles: &[],
                prose: "Progress state and assignment are orthogonal: reassignment is plain (the task just moves to its new assignee — no special status, no round-trip counter), and status=blocked is reserved for a physical blocker no one can move past. An unmet premise is not one: a pending dependency, an unsettled linked decision, and a start day that has not come are all derived as ready:no, so declaring them blocked duplicates a truth Amenbo already computes.",
            },
            Step {
                id: "rejoin-the-cycle",
                n: Some(1),
                trigger: None,
                commands: &[Cmd::CommentAdd],
                cycles: &[],
                prose: "An exception is a detour, not an exit. Once you have handled it, rejoin the cycle where it broke — a task handed back puts you at the mailbox again, a premise resolved at the reserve, a blocker cleared at the work itself. What must not happen is stopping here with the task still reserved and nothing on its timeline: the reservation is what keeps it out of the mailbox, so it is not waiting for anyone, it is lost.",
            },
        ],
        optional: &[
            Step {
                id: "hand-to-the-human",
                n: None,
                trigger: Some("you cannot proceed and need a human decision or action"),
                commands: &[Cmd::TaskAssign, Cmd::CommentAdd],
                cycles: &[],
                prose: "Reassign to the human and say what is needed (a silent reassignment is unhelpful).",
            },
            Step {
                id: "declare-blocked",
                n: None,
                trigger: Some("a physical blocker no one can move past"),
                commands: &[Cmd::TaskBlock],
                cycles: &[],
                prose: "Mark it blocked with the reason.",
            },
            Step {
                id: "resolve-the-premise",
                n: None,
                trigger: Some("your reserve was rejected with not_ready"),
                commands: &[Cmd::TaskDone, Cmd::TaskUndepend, Cmd::DecisionFinishWriting, Cmd::DecisionLink, Cmd::TaskUpdate, Cmd::TaskFinishCreating],
                cycles: &[],
                prose: "Resolve the premise you declared, rather than working around it: finish the blocker or drop the edge; get the linked decision settled, unlink it, or relink it to its successor; move or clear a start day that has not come; end a creation that is still open. There is no --force. Two resolve without anyone else: a start day resolves itself, so if the work is meant to wait, leave it and take the next task; an open creation resolves where you stand, once you have read what is there and it is a task to work on.",
            },
            Step {
                id: "hand-back",
                n: None,
                trigger: Some("you decide not to take a task you reserved"),
                commands: &[Cmd::TaskStatus],
                cycles: &[],
                prose: "Hand it back with `task status <id> todo` so another session can take it.",
            },
        ],
    },
    Cycle {
        id: Cyc::Commit,
        when: "You are about to send text out of this store — a commit message or a diff, a PR body, an issue, a message.",
        backbone: &[
            Step {
                id: "lint-what-leaves",
                n: Some(0),
                trigger: None,
                commands: &[Cmd::Lint],
                cycles: &[],
                prose: "Lint what is leaving, before it leaves: with no arguments the staged diff, and by path or `--stdin` any other text on its way out. It reports and never edits, so a ref it names is yours to rewrite out of the text.",
            },
            Step {
                id: "anchor-the-sha",
                n: Some(1),
                trigger: None,
                commands: &[Cmd::TaskCommitAdd],
                cycles: &[],
                prose: "Once the commit lands, record its SHA on the task — a task takes many, and a SHA already there is a no-op, so anchor every commit you merge.",
            },
        ],
        optional: &[
            Step {
                id: "install-the-hooks",
                n: None,
                trigger: Some("the human wants every commit linted without anyone having to remember"),
                commands: &[Cmd::HooksInstall, Cmd::HooksStatus],
                cycles: &[],
                prose: "Offer the lint hooks (opt-in, asked once for the device).",
            },
        ],
    },
    Cycle {
        id: Cyc::Worktree,
        when: "You are working a task in a worktree — from the one you cut to the one you fold.",
        backbone: &[
            Step {
                id: "cut-per-task",
                n: Some(0),
                trigger: None,
                commands: &[Cmd::WorktreeStart],
                cycles: &[],
                prose: "Cut a worktree per task, whenever the work will produce commits. Reserving guards the task, not the files, so two sessions on two tasks still share one working tree. Work that lands no commit — a local-only edit under gitignore — needs none.",
            },
            Step {
                id: "cut-outside",
                n: Some(1),
                trigger: None,
                commands: &[],
                cycles: &[],
                prose: "Cut it outside the project folder. A worktree cut inside inherits that folder's `.amenbo` through the upward walk, and Amenbo refuses to run there (`nested_worktree`).",
            },
            Step {
                id: "leave-a-standing-one",
                n: Some(2),
                trigger: None,
                commands: &[],
                cycles: &[],
                prose: "A worktree already standing for that task is not yours to enter: someone cut it and may still be in it, and what you reserved was the task's status, not the checkout on disk. Leave it untouched and take the next task — do not read it to judge whether it is live, and do not remove it to clear your way.",
            },
            Step {
                id: "run-the-line",
                n: Some(3),
                trigger: None,
                commands: &[],
                cycles: &[],
                prose: "What `worktree start` writes on stdout is one `cd` line, and that line is to run rather than to read — wrap it (`eval \"$(…)\"`, or `iex (…)` in PowerShell). Read it and stop, and you are still standing in the project folder with a worktree nobody entered. Everything written for you to read is on stderr beside it, and `--json` answers with the path and the branch instead.",
            },
            Step {
                id: "operate-in-the-project-folder",
                n: Some(4),
                trigger: None,
                commands: &[],
                cycles: &[],
                prose: "Operate Amenbo in the project folder itself. A worktree outside it carries no `.amenbo` and so reaches nothing — that is the intent, and binding this checkout is not the way around it.",
            },
            Step {
                id: "fold-it",
                n: Some(5),
                trigger: None,
                commands: &[Cmd::WorktreeFinish],
                cycles: &[],
                prose: "Fold the worktree once its commits are merged, in the session that cut it — nothing else will. A reserved task is out of the mailbox, so the task and the worktree left standing for it go unseen together, and the next session that reaches for that task is turned away by the checkout still on disk.",
            },
        ],
        optional: &[],
    },
];

/// Assembles [`CYCLES`] into the emitted map: the standing description, then each cycle under the
/// key that is its id — the key the runtime reaches for when it drops a cycle that does not apply
/// here.
fn cycles() -> Value {
    let mut map = serde_json::Map::new();
    map.insert("description".to_string(), json!("Cold-path catalog: each cycle here is named by the `cycles` of the step it branches off, and taken when its own trigger fires. `backbone` always applies within its cycle (in order); each `optional` item is done only when its `trigger` matches (the AI self-gates — Amenbo shows what you can do, it does not direct you). Every item carries `kind` and an `id`; a `backbone` item carries its place as `n`, an `optional` item its `trigger` instead."));
    for cycle in CYCLES {
        map.insert(cycle.id.key().to_string(), cycle.to_value());
    }
    Value::Object(map)
}

/// **Takes a cold-path cycle out of a built spec, and every branch to it with it.** The runtime drops
/// a cycle that does not apply where the caller stands (the `worktree` one off a git checkout), and a
/// step would otherwise go on naming it in its `cycles` — a branch to a key the reader cannot find,
/// which reads as a document with a piece missing rather than one that never had it.
///
/// A step left naming nothing drops the key entirely, the same way [`Step::to_value`] never writes an
/// empty one: the absence is the answer, and an empty array is a line of context that says it worse.
pub fn drop_cycle(spec: &mut Value, cycle: Cyc) {
    if let Some(steps) = spec.get_mut(BACKBONE).and_then(|b| b.get_mut("steps")) {
        drop_branch(steps, cycle);
    }
    if let Some(Value::Object(cycles)) = spec.get_mut("cycles") {
        cycles.remove(cycle.key());
        for (_, other) in cycles.iter_mut() {
            for bucket in ["backbone", "optional"] {
                if let Some(items) = other.get_mut(bucket) {
                    drop_branch(items, cycle);
                }
            }
        }
    }
}

/// The steps only someone with git can carry out, named one by one because the cycle they sit in is
/// not all one audience. `commit` is that case: linting what leaves this store is the same advice
/// either way — `lint` opens no store and reads a path or stdin — while anchoring a commit's SHA on
/// a task, and wiring git's own hook slots, are not things a reader without git can do. `AMB-D-335`
/// keeps advice nobody can act on out of the document; where the line runs through a cycle rather
/// than around it, it is drawn here instead of by [`drop_cycle`].
///
/// A pair here is a spelling, not a type, so `every_git_only_step_is_written` is what holds it
/// against the steps as written — a step renamed out from under this table would otherwise go on
/// reaching a reader who cannot run it.
const GIT_ONLY: &[(Cyc, &str)] = &[(Cyc::Commit, "anchor-the-sha"), (Cyc::Commit, "install-the-hooks")];

/// [`GIT_ONLY`] taken out of an already-built spec, for a run with no git in play. The numbering of
/// what is left closes up, so the reader is handed 0, 1, 2… rather than a run with a hole in it
/// where something they were never shown used to sit.
pub fn drop_git_only_steps(spec: &mut Value) {
    let Some(Value::Object(cycles)) = spec.get_mut("cycles") else { return };
    for (cycle, id) in GIT_ONLY {
        let Some(Value::Object(cyc)) = cycles.get_mut(cycle.key()) else { continue };
        for bucket in ["backbone", "optional"] {
            let Some(Value::Array(items)) = cyc.get_mut(bucket) else { continue };
            items.retain(|step| step["id"].as_str() != Some(*id));
            renumber(items);
        }
    }
}

/// `n` closed up over one bucket: a step that carries a place gets the place it now sits at. An
/// `optional` item carries none and is left alone, which is why this asks rather than assumes.
fn renumber(items: &mut [Value]) {
    for (at, step) in items.iter_mut().enumerate() {
        let Some(map) = step.as_object_mut() else { continue };
        if map.contains_key("n") {
            map.insert("n".to_string(), json!(at));
        }
    }
}

/// The branch to `cycle` taken out of every step in one emitted bucket.
fn drop_branch(steps: &mut Value, cycle: Cyc) {
    for step in steps.as_array_mut().into_iter().flatten() {
        let Some(map) = step.as_object_mut() else { continue };
        let Some(Value::Array(branches)) = map.get_mut("cycles") else { continue };
        branches.retain(|named| named.as_str() != Some(cycle.key()));
        if branches.is_empty() {
            map.remove("cycles");
        }
    }
}

/// The key the hot path is emitted under — and so the run's name where a step is addressed from
/// outside, `agentCycle.<step>` (`AMB-D-571`). A cold-path cycle is addressed by its own id, the key
/// [`cycles`] files it under.
const BACKBONE: &str = "agentCycle";


/// The emitted step `id` names within `run` — the backbone's steps under [`BACKBONE`], a cycle's under
/// its own key, both buckets of it, since which bucket an item sits in is the cycle's business and not
/// the namer's. `None` where the run, the step, or the whole cycle is not in this document.
///
/// Test-only: the shelf a plugin hung its call lines on went with the mechanism, and what reaches for a
/// step by id now is the tests that hold the drop the runtime makes.
#[cfg(test)]
fn find_step<'a>(spec: &'a mut Value, run: &str, id: &str) -> Option<&'a mut Value> {
    let buckets: Vec<&mut Value> = if run == BACKBONE {
        vec![spec.get_mut(BACKBONE)?.get_mut("steps")?]
    } else {
        let cycle = spec.get_mut("cycles")?.get_mut(run)?.as_object_mut()?;
        cycle
            .iter_mut()
            .filter(|(key, _)| *key == "backbone" || *key == "optional")
            .map(|(_, items)| items)
            .collect()
    };
    buckets
        .into_iter()
        .filter_map(Value::as_array_mut)
        .flatten()
        .find(|step| step.get("id").and_then(Value::as_str) == Some(id))
}

fn cmd(name: &str, summary: &str, flags: Value, examples: Value) -> Value {
    json!({ "name": name, "summary": summary, "flags": flags, "examples": examples })
}

/// Lists what Amenbo can do, phrased by intent and neutral about order. Each capability names the
/// commands that realise it in `commands`; no sequence, no recommended workflow — capability first.
/// Only commands that actually exist in `all_commands()` may be named, which the
/// `capabilities_reference_real_commands` test enforces, catching both a typo and a command nobody
/// listed.
fn capabilities() -> Value {
    let caps = vec![
        cap("Register a task — filed in one command, then finished in another once it is fully written", &["task add", "task finish-creating"]),
        cap("Find and filter tasks (see filterGrammar)", &["task list"]),
        cap("See a task's details, project, classification, blockers and dependents", &["task show"]),
        cap(
            "Edit a task's fields (title / notes / due / start / priority / the folder it is worked in)",
            &["task update"],
        ),
        cap(
            "Track progress and reserve a task by moving it to in_progress (todo / in_progress / done / blocked / rejected), and end it either way — carried out, or decided against",
            &["task status", "task done", "task reject", "task reopen", "task block"],
        ),
        cap(
            "Split larger work into separate tasks and link blockers (there are no subtasks)",
            &["task depend", "task undepend"],
        ),
        cap(
            "Anchor a task to the git commits that implemented it — record / list / forget SHAs (the chain from history back to a task)",
            &["task commit-add", "task commit-list", "task commit-rm"],
        ),
        cap(
            "Re-home a task to another project and reorder it",
            &["task move"],
        ),
        cap(
            "Assign a task to a person or that person's AI, hand it back, or clear it",
            &["task assign", "task unassign"],
        ),
        cap("Discuss on a task's timeline (a comment posted by mistake can be edited or deleted)", &["comment add", "comment list", "comment edit", "comment rm"]),
        cap("Discuss on a decision's timeline (why it was settled or rejected lands here; a comment posted by mistake can be edited or deleted)", &["decision comment-add", "decision comment-list", "decision comment-edit", "decision comment-rm"]),
        cap(
            "Attach what text cannot hold — screenshots, raw logs, benchmarks — to tasks, decisions, comments",
            &["task attach", "decision attach", "comment attach", "decision comment-attach", "attach ls", "attach show", "attach open", "attach save", "attach rm"],
        ),
        cap(
            "Find where a word is written (tasks, decisions, the comments on both)",
            &["search"],
        ),
        cap(
            "Read the shared activity timeline (system events plus comments)",
            &["activity"],
        ),
        cap("Record a decision — a premise that holds now, settled with the human", &["decision add"]),
        cap("Find and filter decisions (words are search's)", &["decision list"]),
        cap("See a decision, its supersession chain, and the premises it stands on", &["decision show"]),
        cap(
            "Record that a decision stands on an older one (read that first; revisit this if it is overturned)",
            &["decision builds-on"],
        ),
        cap(
            "Move a decision through its lifecycle (finish-writing / reject / reopen / edit / supersede / delete)",
            &["decision finish-writing", "decision reject", "decision reopen", "decision edit", "decision supersede", "decision delete"],
        ),
        cap("Undo a decision-to-decision edge drawn at the wrong target", &["decision unlink"]),
        cap("Link a decision to its implementation tasks", &["decision link"]),
        cap("Promote a task or decision comment into a decision", &["decision promote"]),
        cap(
            "Organize work into projects and order them (classification is via dimensions)",
            &[
                "project add", "project list", "project show", "project update", "project move",
                "project archive", "project unarchive",
            ],
        ),
        cap(
            "Define classification axes (dimensions) with values and assign them to tasks",
            &[
                "dimension add", "dimension list", "dimension show",
                "dimension update", "dimension move", "dimension rm",
                "dimension value-add", "dimension value-update",
                "dimension value-move", "dimension value-close", "dimension value-reopen",
                "dimension value-rm", "dimension set", "dimension unset",
            ],
        ),
        cap(
            "Set up where a project's notifications go — the device's shelf of connections, and what this project reports through it",
            &[
                "notify", "notify target-list", "notify target-add", "notify target-set",
                "notify target-default", "notify target-rm", "notify target-check", "notify target-test",
                "notify on", "notify off", "notify use", "notify unuse", "notify to", "notify event",
            ],
        ),
        cap(
            "Read this store on a phone — the server in the reader's own Cloudflare account, which phone may read it, and keeping the two ends level",
            &[
                "viewer setup", "viewer qr", "viewer app", "viewer phones",
                "viewer revoke", "viewer send", "viewer repair",
            ],
        ),
        cap("See what to do now (overdue / today / in progress)", &["status"]),
        cap("Inspect configuration and this store's identity", &["config", "config set", "whoami"]),
        cap("Update Amenbo: open the installer, or self-update the standalone CLI in place (`--apply` / undo with `--rollback`)", &["update"]),
        cap("See projects", &["project list", "project show"]),
        cap(
            "Allow an AI launched in a folder to operate Amenbo (bind a folder to a project, unbind it, or re-sync its managed guidance block)",
            &["init", "bind", "unbind", "sync-guide"],
        ),
        cap(
            "Take all data out (data sovereignty; no lock-in — export is one way, `restore` is the way back in)",
            &["export"],
        ),
        cap(
            "Check data integrity",
            &["doctor", "validate"],
        ),
        cap(
            "Catch an Amenbo ref in text on its way out of this store, before it lands somewhere it means nothing (read-only; reports path:line and exits non-zero)",
            &["lint"],
        ),
        cap(
            "Give a task a git worktree of its own — cut when the work starts, folded once its commits have landed",
            &["worktree start", "worktree finish"],
        ),
        cap(
            "Run the lint on every commit, by installing it as a git hook (asked once for the lint as a feature, on this device — one answer covers the repositories Amenbo works in, later ones included; Amenbo touches only the hook it wrote)",
            &["hooks install", "hooks uninstall", "hooks status"],
        ),
        cap(
            "Have this machine's scheduler wake Amenbo once an hour, with no app open and nothing resident (asked once for the tick as a feature, on this device)",
            &["tick install", "tick uninstall", "tick status"],
        ),
        cap(
            "Hand the human the text that asks their AI to wire this folder to run `agent` at every session start (Amenbo writes no settings file — their AI makes the edit)",
            &["agent-hook snippet"],
        ),
        cap(
            "Write back what the human answered about that (you put the question — Amenbo cannot ask on this face; recording it is all this does)",
            &["agent-hook answer"],
        ),
        cap(
            "Serve one folder over MCP, for an AI whose host cannot open a folder itself (a host starts it — not a command you type)",
            &["mcp"],
        ),
        cap(
            "Dress this device in a skin: what is held, take one in, wear or drop one, check a file before handing it on, write one out",
            &["skin list", "skin add", "skin use", "skin rm", "skin validate", "skin template", "skin write-out"],
        ),
        cap(
            "Write a verified full snapshot of the store's truth source to a single file",
            &["backup"],
        ),
        cap(
            "Restore the store's truth source from a verified snapshot (the recovery side of backup)",
            &["restore"],
        ),
        cap(
            "Physically erase content from the store — a comment on a task or a decision in full, or one settled decision's body (human-gated maintenance)",
            &["hard-erase comment", "hard-erase decision-comment", "hard-erase decision"],
        ),
        cap(
            "Build an automation — the library of actions, the steps in them, the placements on a picture, and the settings each answers",
            &[
                "automation add", "automation update", "automation rm",
                "automation place-add", "automation place-rm",
                "automation action-add", "automation action-update", "automation action-entry-set",
                "automation action-scope-set", "automation action-rm",
                "automation step-add", "automation step-update", "automation step-rm",
                "automation cfg-add", "automation cfg-update", "automation cfg-set", "automation cfg-rm",
                "automation agent-set",
            ],
        ),
        cap(
            "Read an automation back — a project's automations, one definition in full, the library it reaches, and one action with what is inside it",
            &["automation list", "automation show", "automation action-list", "automation action-show"],
        ),
        cap(
            "Read what a run did — reached from the task it worked or the automation it came from, never searched for",
            &["automation run-list", "automation run-show"],
        ),
        cap(
            "Run an automation — start one, pause it, pick it up again, stop it",
            &["automation start", "automation pause", "automation resume", "automation stop"],
        ),
        cap(
            "Report the step of a run you are carrying out — take its task, hand things on, and say you are done",
            &["automation step-take", "automation step-out", "automation step-done"],
        ),
        cap(
            "Draw the picture a run is walked along — where it starts, the ways out of each step, what happens after each one is taken, and what is handed along",
            &[
                "automation entry-set",
                "automation exit-add", "automation exit-rename", "automation exit-rm",
                "automation port-add", "automation port-update", "automation port-rm",
                "automation edge-add", "automation edge-update", "automation edge-rm",
                "automation wire-add", "automation wire-rm",
            ],
        ),
    ];
    Value::Array(caps)
}

fn cap(capability: &str, commands: &[&str]) -> Value {
    json!({ "capability": capability, "commands": commands })
}

fn all_commands() -> Value {
    json!([
        cmd("amenbo", "No arguments. Shows today's tasks and suggested next operations (discover).",
            json!([{ "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo", "amenbo --json"])),
        cmd("agent", "Presents how to work here — the workflow and rules in full, plus an index of the commands (this JSON). The AI's entry point. A command's own flags and examples are pulled on demand with --command <name>, so the entry point stays small; --full prints them all inline.",
            json!([{ "name": "--command <name>", "help": "print one command's full spec (flags, args, examples) instead of the entry point" },
                   { "name": "--full", "help": "print every command's full spec inline (scripts / verification)" },
                   { "name": "--json", "help": "machine-readable output (recommended)" }]),
            json!(["amenbo agent --json", "amenbo agent --command \"task add\" --json", "amenbo agent --full --json"])),
        cmd("version", "Shows version information.",
            json!([{ "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo version --json"])),
        cmd("update", "Updates Amenbo. By default opens this OS's one-piece installer (GUI + CLI) — resolved from the published latest.json — in your browser. There is no page to fall back to: a manifest that cannot be read is an error, one that names no installer for your platform is reported with no address, and the check being switched off here says so. Because you typed it, the lookup asks upstream rather than answering from the detection cache, so it never reports on an entry up to 24 hours old (offline it still falls back to the last one it had). `--apply` self-updates the standalone CLI in place instead: it downloads the new CLI over TLS and swaps this binary (no installer, no elevation), keeping the replaced binary beside it; a GUI-managed CLI is updated from the desktop app, not here. `--rollback` undoes the last `--apply` offline, restoring that kept binary. Applying is always your explicit call — Amenbo never updates in the background.",
            json!([{ "name": "--print", "help": "print the installer URL instead of opening a browser (headless / scripted use)" },
                   { "name": "--apply", "help": "self-update the standalone CLI in place (download + swap this binary) instead of opening the installer" },
                   { "name": "--rollback", "help": "undo the last --apply, restoring the previous binary kept beside this one (offline, no download)" },
                   { "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo update", "amenbo update --print", "amenbo update --apply", "amenbo update --rollback"])),
        cmd("notify", "Shows where this project's notifications go: the device's shelf of connections, and what the bound project does with it — on or off, which targets carry it, which of the thirteen events it reports, and where a mail target's message is addressed. A connection is written once under a name and every project selects from the shelf, so a webhook that changes is one edit.",
            json!([{ "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo notify", "amenbo notify --json"])),
        cmd("notify target-list", "Every connection on this device's shelf, in the order they were raised — the kind, the name, the default mark, and whether the credential is held. The credential itself is never read back.",
            json!([{ "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo notify target-list"])),
        cmd("notify target-add", "Raises a connection on the shelf under a name. The connection is written afterwards with `notify target-set`, which is what gives its credential a row to hang off. The first one ever raised carries the default mark.",
            json!([{ "name": "--kind <slack|mail>", "help": "what carries it", "required": true },
                   { "name": "<name>", "help": "the name a project's settings offers it under", "required": true }]),
            json!(["amenbo notify target-add --kind slack team", "amenbo notify target-add --kind mail inbox"])),
        cmd("notify target-set", "Writes one target's connection. Only what is named is written. The credential is `--secret`, and `-` reads it from stdin — a webhook URL or a password on the command line is visible in the process list and lands in shell history; an empty value clears it.",
            json!([{ "name": "<target>", "help": "the target on the shelf", "required": true },
                   { "name": "--name <str>", "help": "the name it is offered under" },
                   { "name": "--smtp-host <str>", "help": "the relay a mail target hands the message to" },
                   { "name": "--smtp-port <n>", "help": "the port that relay listens on (587 on nearly every provider)" },
                   { "name": "--smtp-user <str>", "help": "the account to authenticate as; empty for a relay that asks for none" },
                   { "name": "--mail-from <str>", "help": "the address to send from; empty falls back to the account" },
                   { "name": "--secret <value|->", "help": "the credential; `-` reads it from stdin" }]),
            json!(["printf %s \"$WEBHOOK\" | amenbo notify target-set 1 --secret -",
                   "amenbo notify target-set 2 --smtp-host smtp.example.com --smtp-user someone@example.com"])),
        cmd("notify target-default", "Moves the default mark — where a newly created project starts out pointing. The projects already standing keep the selection they made, so the mark never becomes a tier.",
            json!([{ "name": "<target>", "help": "the target on the shelf", "required": true }]),
            json!(["amenbo notify target-default 1"])),
        cmd("notify target-rm", "Removes a target, and with it every project's selection of it and the credential it held. It says how many projects lose it before asking; `--yes` answers ahead of the ask.",
            json!([{ "name": "<target>", "help": "the target on the shelf", "required": true },
                   { "name": "--yes", "help": "skip the confirmation" }]),
            json!(["amenbo notify target-rm 1"])),
        cmd("notify target-check", "Asks whether the connection is usable, without sending anything. How much that means is the kind's: a mail relay is connected to and the account offered to it, a Slack webhook has only the shape of its URL read — it has no door but posting, and a revoked webhook still has the shape.",
            json!([{ "name": "<target>", "help": "the target on the shelf", "required": true },
                   { "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo notify target-check 1"])),
        cmd("notify target-test", "Sends one message through it, which is the only thing that answers whether it still works. It carries no project, so a mail target sends to the account it authenticates as.",
            json!([{ "name": "<target>", "help": "the target on the shelf", "required": true },
                   { "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo notify target-test 1"])),
        cmd("notify on", "Starts reporting from this project, through whatever it has selected.",
            json!([{ "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo notify on"])),
        cmd("notify off", "Stops reporting from this project. The selection and the events stay where they are, so a fortnight away costs one command and finds the settings standing on the way back.",
            json!([{ "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo notify off"])),
        cmd("notify use", "Sends this project's notifications through one more target. A project may carry several, which is what lets one reach a channel and an inbox at once.",
            json!([{ "name": "<target>", "help": "the target on the shelf", "required": true }]),
            json!(["amenbo notify use 1"])),
        cmd("notify unuse", "Stops sending this project's notifications through one target. The target stays on the shelf.",
            json!([{ "name": "<target>", "help": "the target on the shelf", "required": true }]),
            json!(["amenbo notify unuse 1"])),
        cmd("notify to", "Where a mail target's message is addressed — several addresses on one line, separated by commas. Empty falls back to the account each relay authenticates as.",
            json!([{ "name": "<addresses>", "help": "comma-separated addresses (empty clears them)", "required": true }]),
            json!(["amenbo notify to 'ops@example.com, lead@example.com'", "amenbo notify to ''"])),
        cmd("notify event", "Starts or stops reporting one event. `notify` lists the ones a project may report; `store.changed` is not among them.",
            json!([{ "name": "<name>", "help": "the event's name, as the catalog spells it", "required": true },
                   { "name": "--off", "help": "stop reporting it" }]),
            json!(["amenbo notify event task.done", "amenbo notify event task.due --off"])),
        cmd("viewer setup", "Stands the Viewer's server up in a Cloudflare account: a Worker and a database, in the reader's own account, with the address, the write token and the encryption key left in this store. The API token is read from stdin and written down nowhere — it is used for this one run. Keys already here are kept, because a new one opens nothing already up there. Where a server of the usual name is already standing and this store holds no key that opens it, it is refused rather than replaced — `--name` is both ways past that.",
            json!([{ "name": "--account <id>", "help": "which account to build in, where the token reaches more than one" },
                   { "name": "--name <name>", "help": "what to call the server, for a second one in the same account — or to say that one already standing under the usual name is this store's" },
                   { "name": "--json", "help": "machine-readable output" }]),
            json!(["printf %s \"$CF_API_TOKEN\" | amenbo viewer setup --json",
                   "printf %s \"$CF_API_TOKEN\" | amenbo viewer setup --name amenbo-viewer-demo"])),
        cmd("viewer qr", "Draws a new read code for a phone's camera. It carries the encryption key, so it is drawn on a screen and nowhere a pipe can take it — `--json` is refused. It replaces whatever code the server was holding, so the phone that had the one before stops reading; pairing a second phone is this same command.",
            json!([{ "name": "--terminal", "help": "draw it even where stdout is not a terminal" }]),
            json!(["amenbo viewer qr"])),
        cmd("viewer app", "Where the Viewer app is got — one code per kind of phone, and the addresses in words for whoever cannot point a camera at one. Nothing on these codes is a secret, and it answers before the server exists.",
            json!([{ "name": "--terminal", "help": "draw the codes even where stdout is not a terminal" },
                   { "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo viewer app", "amenbo viewer app --json"])),
        cmd("viewer phones", "Asks the server whether a phone may read, and since when. There is one read code and the server never learns which phone offered it, so this says whether any phone may read — never how many do.",
            json!([{ "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo viewer phones --json"])),
        cmd("viewer revoke", "Takes the read code away, so whatever was holding it stops reading. There is one code, so this takes every phone off at once — there is nothing to name and so nothing to single out. Pairing again is one `viewer qr`.",
            json!([{ "name": "--yes", "help": "skip the confirmation" },
                   { "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo viewer revoke --yes --json"])),
        cmd("viewer send", "Carries what has moved: copies the backlog's changes into the queue, then empties as much of that queue as the server will take. A server that asks to be left alone, and a day's rows already spent, both leave the queue where it is rather than failing.",
            json!([{ "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo viewer send --json"])),
        cmd("viewer repair", "Compares what the server holds with what this machine holds and places the difference — both halves: the records that never arrived, and the keys up there that are gone from here. Comparing is cheap and placing is not, so this counts and says the number; `--send` places it, and so does running it again within ten minutes.",
            json!([{ "name": "--send", "help": "place the difference rather than only counting it" },
                   { "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo viewer repair --json", "amenbo viewer repair --send --json"])),
        cmd("config", "Shows this store's configuration: where its files are, and every setting `config set` writes — the same settings `--json` carries, each line naming the key alongside its value, so a value that was set can be read back by whoever set it.",
            json!([{ "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo config", "amenbo config --json"])),
        json!({ "name": "config set", "summary": "Changes a configuration value. Known keys: default_view, language, date_locale (how dates are written, as a BCP-47 tag; unset follows language — the GUI reads it, the CLI never does), human_name, ai_name (the display names of the two actors — this is the only way to rename either), human_avatar, ai_avatar (their icons, as a data:image/png;base64 URI), skin (the name of a skin kept under <base>/skins; empty takes it off), ai_allow_project_ops, startup_integrity_check (read-only integrity doctor at open; warnings only; default on), update_check (asks Amenbo's update endpoint whether a newer release is out; infra-side only — no user data; timeout + silent-fail + cached; default on; AMENBO_UPDATE_CHECK=0 overrides).",
            "args": [{ "name": "key", "required": true, "help": "config key" },
                     { "name": "value", "required": true, "help": "config value" }],
            "flags": [], "examples": ["amenbo config set default_view board", "amenbo config set language ja", "amenbo config set startup_integrity_check false"] }),
        cmd("whoami", "Shows this store's identity (display name / hardware-copy check).",
            json!([{ "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo whoami --json"])),
        json!({ "name": "init", "summary": "Initializes a folder so an AI launched there is allowed to operate Amenbo. Amenbo does not read or write the project's contents (source or files). The store itself lives in app-data (a single database for the whole device); only .amenbo (a dir→project pointer) and AGENTS.md (the AI guide) are placed in the folder. On a device that already holds an Amenbo store, init makes a new project in this folder — it does not start a second store. Secrets (keys) are also kept in the user area, not in the project directory. AGENTS.md is English-based and embeds the global user language (config language / --language) as a 'communicate with the human in this language' directive. A folder already bound to another project via .amenbo is rejected by default (init_pointer_exists; prevents clobbering the production pointer). A folder that has no .amenbo but already holds an Amenbo managed block (in CLAUDE.md/AGENTS.md) is no longer rejected on the marker alone: init reverse-looks-up the bindings registry and, if exactly one live project claims the folder, recovers the lost pointer (a bind, not a new project); if several claim it, it stops as ambiguous (init_ambiguous_owners); if none do, it proceeds and idempotently regenerates the block. Use bind to re-bind to an existing one, or --force to truly recreate and overwrite.",
            "args": [], "flags": [{ "name": "--name <str>", "help": "the first local user name (at initial genesis)" },
                                   { "name": "--language <code>", "help": "sets the user language (ja/en etc.) in the global config and embeds it in AGENTS.md" },
                                   { "name": "--force", "help": "create a new project and overwrite even if a .amenbo already exists (default rejects clobbering)" }],
            "examples": ["amenbo init", "amenbo init --name Alice --language ja"] }),
        cmd("bind", "Allows an AI launched in this folder to operate an existing project (it does not touch the contents; it just places a .amenbo pointer and locally registers project→dir). Shows the current binding when --project is omitted. Several folders may point at the same project (many-to-one). Binding a subdirectory of a folder that is already managed (a parent has a .amenbo) is rejected (binding_nested_tree) so a stray bind cannot shadow the root pointer; pass --force to bind it intentionally. If the target is gone, binding_stale — and the answer lists the vanished bindings by id, because what a folder that moved wants is a re-point rather than a second binding: --rebind <binding-id> makes the binding already recorded name the folder being bound, keeping its id, so whatever points at that binding (a task filed at the folder) follows it instead of being left naming nothing. A number nobody bound is refused (not_found) with the ids there are. By default the pointer lands in the current directory; pass --dir <path> to place it in another existing folder (bind a folder from outside it).",
            json!([{ "name": "--project <id>", "help": "project ID to bind (omit to show)" },
                   { "name": "--dir <path>", "help": "place the .amenbo pointer in this existing directory instead of the current one (bind a folder from outside it, git -C style)" },
                   { "name": "--rebind <binding-id>", "help": "re-point this already-recorded binding at the folder being bound instead of recording a new one — it keeps its id, which is what a folder that moved, was renamed or was restored elsewhere needs. Requires --project" },
                   { "name": "--force", "help": "bind even inside an already-managed tree (a parent has a .amenbo); default rejects to avoid shadowing the root pointer" },
                   { "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo bind --project 3", "amenbo bind --project \"Site rebuild\" --dir /work/repo", "amenbo bind --project 3 --rebind 7", "amenbo bind --json"])),
        cmd("unbind", "Removes this folder's .amenbo binding (and Amenbo's managed blocks in AGENTS.md/CLAUDE.md, keeping your own content), the inverse of bind/init. The project itself is kept: this is a many-to-one unbind, so only this folder's pointer is removed and other folders bound to the same project are untouched. It also forgets this folder from the local project→folder reference registry. If the folder has no .amenbo of its own it is not unbound (unbind_no_binding); an inherited binding from an ancestor is reported, not silently removed, so the whole tree is never unbound by accident. Taking the last folder off a project goes through like any other: there is no count that refuses, since re-homing folders means removing them all before putting them back. What the answer does instead is say so — the human line reports that the project has no folder left and that nothing can operate it until one is linked again, and --json carries the same fact as project_folders_left.",
            json!([{ "name": "--dir <path>", "help": "folder to unbind (defaults to the current directory)" },
                   { "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo unbind", "amenbo unbind --dir /work/repo --json"])),
        cmd("status", "Shows a summary of what to do now (overdue / today / in progress).",
            json!([{ "name": "--scope <today|overdue|week>", "help": "scope (default today)" },
                   { "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo status --json", "amenbo status --scope week"])),
        json!({ "name": "search", "summary": "Finds where words are written, across tasks, decisions and the comments on both — plus the labels a task is filed under and the names of what is attached. Answers with one line per PLACE, not per record: the face the words landed on (title / body / comment / label / attachment), the record it belongs to (AMB-T-<n> / AMB-D-<n>), the comment to open where there is one, and a short excerpt. Words are ANDed and match as substrings, with no word boundaries (so a search for part of a compound word finds it); full-width, case and kana spellings are folded together. Every word has to land somewhere on the record — not all on one face — and each face carrying one is a line, which is what makes the answer 'here is where each of your words is written'. A word written as a ref (AMB-T-<n> / AMB-D-<n>) pins that record to the top, so holding a number takes the same command as holding a phrase. A number alone (12 / #12) is a ref as well: the two sides number themselves apart, so it pins the task AND the decision that carry it, and --kind keeps the side you named. Default order is the face first (a name outranks a paragraph, and that a remark), newest within it. Every line also says where the record it points at stands — a task's status, its priority and what it is filed under, a decision's status — so a hit on work that is already over says so without a second read. This read has a ceiling of its own — a hit carries an excerpt — so with no --limit it returns the first 20 and reports the total.",
            "args": [{ "name": "word...", "required": true, "help": "the words to look for (ANDed)" }],
            "flags": [{ "name": "--project <id>", "help": "look in this project alone (human only — an AI is already scoped to its bound project). An argument of its own, not a --filter key: a project is an axis tasks and decisions carry alike, so scoping to one keeps decisions in the answer" },
                      { "name": "--filter <expr>", "help": "narrow structurally, in the grammar of the side --kind names: task list's for a task (see filterGrammar), decision list's for a decision. It requires --kind, because the two grammars share spellings that mean different things — status:rejected is work decided against on one side and a decision turned down on the other" },
                      { "name": "--kind <task|decision>", "help": "keep one side: the words on a task, or on a decision" },
                      { "name": "--face <title|body|comment|label|attachment>", "help": "keep one face of the record: its name, its text, a remark on it, a label it is filed under, the name of what is attached. The axis beside --kind and judged apart from it, so naming both is the product of the two — --kind decision --face comment is the remarks on decisions, which neither axis alone can ask for" },
                      { "name": "--sort <face|time|-time>", "help": "face (default: the face first, newest within it), -time (newest first), time (oldest first). The instant is the hit's own — a comment's posting time, or when the text it sits in was last written" },
                      { "name": "--limit <n>", "help": "max hits (default 20). total_matched says what the ceiling left behind" },
                      { "name": "--offset <n>", "help": "number of hits to skip in sort order (paging)" },
                      { "name": "--json", "help": "machine-readable output — { query, count, total_matched, hits[face,kind,ref,title,comment,at,snippet,matches,standing] }. matches[start,end] are where the words landed in snippet, in its own characters; standing is the record's own state (status, draft where a decision's writing is unfinished — its status is `decided` from the moment it is saved, so only this tells the two apart — and a task's priority and labels)" }],
            "examples": ["amenbo search notification target --json", "amenbo search AMB-T-<n> --json", "amenbo search rollout --kind decision --json", "amenbo search rollout --kind decision --face comment --json", "amenbo search notification --kind task --filter \"status:todo\" --limit 5 --json", "amenbo search rollout --kind decision --filter \"status:decided\" --json"] }),
        cmd("activity", "Shows activity (system events plus comments) as one timeline. History reads newest-first; passing a cursor to --since reads the increment oldest-first (an agent's poll-for-what-changed). Humans and the AI read the same stream. Every response carries an opaque cursor; --for me narrows it to what a facet should act on.",
            json!([{ "name": "--task <id>", "help": "this task only" },
                   { "name": "--project <id>", "help": "only tasks belonging to this project" },
                   { "name": "--since <date|cursor>", "help": "a date (today / +3d / YYYY-MM-DD) reads history on/after it, newest-first; an opaque cursor from a prior response reads only what is strictly newer, oldest-first (incremental) — pass the response's cursor to resume where you left off" },
                   { "name": "--kind <system|comment>", "help": "filter by which stream an item came from: `system` for the events Amenbo stamps itself, `comment` for what a facet wrote. Distinct from the `kind` a system item carries in its payload, which names the event — task.created / status_changed / assigned / moved / deleted" },
                   { "name": "--by <human|ai>", "help": "filter by the issuer's facet (a read filter separate from the global --actor)" },
                   { "name": "--for <me|human|ai>", "help": "narrow to what a facet should act on: activity on tasks assigned to that facet (destination axis; me = your own facet). Distinct from --by, which filters by who issued the event" },
                   { "name": "--limit <n>", "help": "max count (history: newest-first window; incremental: oldest items after the cursor). has_more marks when the window was cut" },
                   { "name": "--offset <n>", "help": "number of items to skip (newest first; paging / going back through history)" },
                   { "name": "--json", "help": "machine-readable output — { count, cursor, has_more, items }" }]),
            json!(["amenbo activity --json", "amenbo activity --task 12 --json", "amenbo activity --limit 100 --offset 100 --json", "amenbo activity --for me --since cur1_… --limit 50 --json"])),
        cmd("sync-guide", "Re-syncs Amenbo's managed guidance block in bound folders to this binary's current format version. A folder follows on its own the moment you run Amenbo in it, so this is for the folders you have not been in — and for a block Amenbo could not write (a read-only checkout). Idempotent and low-churn: a folder's CLAUDE.md/AGENTS.md is rewritten only when its managed block actually changed, each folder's own language label is preserved (never downgraded), and your content outside the markers is untouched. By default it targets every folder locally bound on this machine (the machine's binding registry, store-independent) so its scope matches what doctor scans; pass --dir to resync just one folder. Moved/renamed folders are skipped silently.",
            json!([{ "name": "--dir <path>", "help": "resync just this folder (defaults to every locally bound folder)" },
                   { "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo sync-guide", "amenbo sync-guide --dir /work/repo --json"])),
        cmd("doctor", "Data integrity check (orphan references, broken ordering, bound folders whose CLAUDE.md/AGENTS.md still carry an outdated managed-block version after a binary update, bound folders whose .amenbo pointer is still in a pre-migration format, projects no folder on this machine leads to, bound folders that do not start their AI on Amenbo, etc.). Side-effect-free by default: this face reports, it never rewrites. What it reports about a folder heals on its own the next time you run Amenbo there — the managed block follows this binary and a legacy .amenbo is upgraded — so what stays listed here is the folders you have not been in (sync-guide resyncs every bound folder's block at once). --fix repairs fixable problems: it sweeps attachment rows whose record is gone (the one reference no foreign key can hold, so the one that can dangle), sweeps attachment files nothing references any more (the delete path reclaims its own, so this collects only what it had to spare) and forgets folder bindings no live project claims. Every repair is non-destructive — it drops nothing you can read.",
            json!([{ "name": "--fix", "help": "repair fixable problems (sweep attachment rows whose record is gone, reclaim unreferenced attachment files, forget folder bindings no live project claims; all are non-destructive)" },
                   { "name": "--yes/-y", "help": "skip the --fix confirmation" },
                   { "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo doctor --json", "amenbo doctor --fix --yes"])),
        json!({ "name": "validate", "summary": "Checks the shape of the given tasks (all data when omitted). Side-effect-free.",
            "args": [{ "name": "id...", "required": false, "help": "ID(s) to check (multiple allowed)" }],
            "flags": [{ "name": "--json", "help": "machine-readable output (issues carry a fix_hint)" }],
            "examples": ["amenbo validate --json", "amenbo validate AMB-T-<n> --json"] }),
        json!({ "name": "lint", "summary": "Finds Amenbo refs (AMB-T-<n> / AMB-D-<n> …) in text on its way out of this store — a commit message, a diff, a file — reports each as path:line, and exits non-zero if there is one. An id resolves only for someone holding this store; anywhere else it is a reference into nothing. Read-only: it reports and never edits (there is no --fix). With no arguments it reads the staged diff (git diff --cached) and scans what the commit ADDS — a ref in untouched or deleted text is not what this commit is leaking. Pass file paths to lint those instead (the message file git hands a commit-msg hook included), or --stdin for piped text. A bare #<n> is left alone: that is a GitHub issue, and a T-<n> may be another tracker's — which is exactly what the AMB- namespace settles. It opens no store and resolves no id (the AMB- prefix is the whole test), so it answers the same in a checkout, in CI, and over any text at all, and needs no .amenbo to run. The exit code is the verdict: 0 clean, 1 a ref was found (or the input could not be read).",
            "args": [{ "name": "path...", "required": false, "help": "file(s) to lint (default: the staged diff)" }],
            "flags": [{ "name": "--stdin", "help": "lint the text piped on stdin instead" },
                      { "name": "--json", "help": "machine-readable output (ok / count / hits[path,line,ref])" },
                      { "name": "--quiet", "help": "report nothing and let the exit code speak — for a caller that wants only the verdict (the hook Amenbo installs does not pass it: a refused commit has to say what refused it)" }],
            "examples": ["amenbo lint", "amenbo lint --json", "amenbo lint .git/COMMIT_EDITMSG", "amenbo lint --stdin < message.txt"] }),
        json!({ "name": "worktree start", "summary": "Cuts this task a git worktree of its own and hands back the way into it. Where it goes is not a question anyone is asked: <the repository's parent>/<its name>-worktrees/<id>, on branch task/<id>. Beside the project rather than inside it, because a checkout cut within inherits the project's .amenbo through the upward walk and would drive the real backlog from a throwaway folder — which is what Amenbo refuses to run in (nested_worktree). THE RETURN VALUE ON STDOUT IS ONE `cd` LINE and nothing else, so the caller enters the checkout with eval \"$(amenbo worktree start 123)\" — iex (amenbo worktree start 123) in PowerShell, the same line either way; everything a person reads goes to stderr beside it. This is the one command whose stdout is a value to run rather than an account of what happened, and read-and-stop leaves you standing in the project folder with a worktree nobody entered. --json answers with the path and the branch instead, and needs no shell quoting at all. It touches the backlog not at all: reserving the task is its own act (`task status <id> in_progress`), and this is only git. Run it in the repository the task is worked in — what is cut is derived from the folder the command was typed in, never from the task, and a task naming a folder in another repository is refused with the repository to type it in (worktree_elsewhere). Refused, without creating anything, when a worktree is already standing for this task (worktree_exists — that is someone else's, so take a different task rather than looking inside it) or when task/<id> is already a branch (worktree_branch_exists).",
            "args": [{ "name": "id", "required": true, "help": "the task" }],
            "flags": [{ "name": "--base <branch>", "help": "the branch to cut from (default: the branch the repository is standing on)" },
                      { "name": "--json", "help": "machine-readable output (path / branch) instead of the `cd` line" }],
            "examples": ["eval \"$(amenbo worktree start 123)\"", "iex (amenbo worktree start 123)", "amenbo worktree start 123 --base release/26 --json"] }),
        json!({ "name": "worktree finish", "summary": "Takes the task's worktree and its branch away again, once its commits have landed. It refuses while there is anything left to lose: work nobody committed (worktree_dirty), or a branch carrying changes the base does not have (worktree_unmerged). Whether the changes landed is measured by the patch each commit carries rather than by lineage (git cherry), so a squash merge and a rebase merge read as merged — which is what keeps --force out of everyday use. --force overrides both and is the only way to discard work on purpose. A change that landed differently than it left — a conflict resolved by hand — has no equivalent patch to find and reads as unmerged: that is the safe way to be wrong, and `git diff <base>...task/<id>` is how to check before forcing. There is no return value: stdout carries the account of what happened, like every other command. The task itself is untouched — close it with `task done <id>`, or hand it back with `task status <id> todo`. Fold it in the session that cut it: a reserved task is out of the mailbox, so the task and the worktree left standing for it go unseen together.",
            "args": [{ "name": "id", "required": true, "help": "the task" }],
            "flags": [{ "name": "--base <branch>", "help": "the branch the worktree is measured against (default: the branch the repository is standing on)" },
                      { "name": "--force", "help": "tear it down regardless — discarding uncommitted work and unmerged commits" },
                      { "name": "--json", "help": "machine-readable output (path / branch / base)" }],
            "examples": ["amenbo worktree finish 123", "amenbo worktree finish 123 --force"] }),
        cmd("hooks install", "Writes the git hooks that run `amenbo lint` on every commit: `pre-commit` reads the staged diff, and `commit-msg` reads the message, which is the only place git offers it (at pre-commit time no message exists yet). One lint, two of git's doors. Installing means writing into your git plumbing, which Amenbo does not do unasked: it asks once — for the lint as a feature, on this device — and that one answer covers every slot and every repository, the ones bound after it included. `install` is the explicit face of that, wiring the repository it runs in; it is usable any time, including after a `no`, and it takes back an earlier `uninstall` here. Amenbo marks the hooks it writes and touches nothing else: a hook from husky, lefthook or your own hand is NEVER overwritten — install steps around it, wiring the slots it may own and naming the one line to add to the rest (`amenbo lint || exit 1`, or `amenbo lint \"$1\" || exit 1` for commit-msg). Only an install with no slot to write at all is refused. Re-running over Amenbo's own hooks rewrites them, which is how a newer build's hooks land. They honour core.hooksPath, exit 0 when Amenbo is not on PATH (a convenience, not a gate), and one commit is bypassed with `git commit --no-verify`.",
            json!([]), json!(["amenbo hooks install"])),
        cmd("hooks uninstall", "Removes the lint hooks Amenbo wrote from this repository, and opts it out so a device-wide yes does not re-wire it at the next startup (this is per repository — it does not touch the device's answer). The mirror of install, refusal for refusal and partial for partial: a hook Amenbo did not write is not Amenbo's to delete and is left alone, and only a call with nothing of ours to remove and a stranger in the way is refused. With no hooks of ours there, it records the opt-out and does nothing else. It closes the question for this repository, not the door — `hooks install` re-wires it whenever you want it back.",
            json!([]), json!(["amenbo hooks uninstall"])),
        cmd("hooks status", "Shows the two facts side by side: what is in each hook slot (no hook / Amenbo's, with its marker version / one Amenbo did not write), and what this device answered (not asked yet / yes / no) — plus a line when this repository is opted out. They are independent on purpose — the answer says what was answered and is NEVER read as a mirror of the disk, which is what makes a hook deleted or added by hand a state Amenbo can see rather than one that breaks it. Read-only.",
            json!([{ "name": "--json", "help": "machine-readable output (in_git_repo / hooks / consent)" }]), json!(["amenbo hooks status --json"])),
        cmd("tick install", "Registers the hourly tick with this machine's scheduler, and records that this device consented. What is registered carries no meaning: it wakes Amenbo once an hour, and Amenbo works out once awake what is due — so however many things come to depend on it, this stays one row in your system settings, and switching that row off stops all of them. Registering writes into your scheduler, which Amenbo does not do unasked: it asks once, for the tick as a feature, on this device, and `install` is the explicit face of that yes. Idempotent — run over a registration that is already there it writes it again, which is how the timer comes to name the build running now after an upgrade. The registration is written first and the answer only after it succeeded, so Amenbo never claims a timer that is not there.",
            json!([]), json!(["amenbo tick install"])),
        cmd("tick uninstall", "Takes the registration away, and records that this device does not want it. Unlike the lint's uninstall this is a device-wide no, because the device is the only scale a timer has: one machine, one registration, one answer. It closes the question and not the door — `tick install` registers it again whenever you want it back. Idempotent: with nothing registered it succeeds, having left the machine in the state you asked for. On macOS the row itself outlives this: the OS keeps its own record of the item, so it stays in the login items with nothing behind it — say so, or the reader takes the row for a removal that failed (`row_remains` in the JSON says which machines this is).",
            json!([]), json!(["amenbo tick uninstall"])),
        cmd("tick status", "Shows the two facts side by side: what the scheduler is holding, and what this device answered (not asked yet / yes / no). They are read independently on purpose — the answer says what was answered and is NEVER read as a mirror of the scheduler, which is what makes a registration you switched off yourself a state Amenbo can see rather than one it talks over. A system Amenbo cannot register on yet says so on the scheduler's line and leaves the answer alone. Read-only.",
            json!([{ "name": "--json", "help": "machine-readable output (supported / registered / consent)" }]), json!(["amenbo tick status --json"])),
        json!({ "name": "agent-hook snippet", "summary": "Prints a request for one AI tool's session-start wiring, written to be given to the AI the human works with, so that tool runs `amenbo agent` at the start of every session — the reach the managed block in CLAUDE.md/AGENTS.md cannot have, since it works only if the file is read. The catalog covers the settings-only providers: claude-code, github-copilot, cursor, codex-cli, gemini-cli (the argument refuses anything else, naming what it takes). The request carries the settings, the file they belong in, and that whatever is already in that file stays — so a folder whose settings are not empty needs no merge worked out by hand. What it injects is the launch instruction — the one line that says to run `agent --json` and follow it — never the spec itself, which the agent holding the instruction fetches for itself. **Amenbo writes no settings file**: it hands over text, and the AI the human gives it to makes the edit. So this is what you hand a human who asks how to stop their AI from missing Amenbo. stdout is that text and nothing else (it pipes to a clipboard); where it is going is said on stderr, and `--json` carries the request beside the settings on their own (`configuration`, for a caller doing the edit itself). Opens no store and needs no bound folder — a fresh checkout can be wired before anything else. It reports nothing about whether this folder is already wired: it is the handing-over face, not the state.",
            "args": [{ "name": "tool", "required": true, "help": "the AI tool to be wired, by the catalog's name for it (claude-code / github-copilot / cursor / codex-cli / gemini-cli)" }],
            "flags": [{ "name": "--copy", "help": "put it on this machine's clipboard instead of printing it on stdout — it is still shown on stderr (refused, naming the pipe, where the machine has no clipboard tool)" },
                      { "name": "--json", "help": "machine-readable output (tool / label / paste_into / request / configuration / copied)" }],
            "examples": ["amenbo agent-hook snippet claude-code", "amenbo agent-hook snippet cursor --json", "amenbo agent-hook snippet codex-cli --copy"] }),
        json!({ "name": "agent-hook answer", "summary": "Records what a person answered when asked whether this folder's AI may be started on Amenbo. Amenbo puts that question only where someone can answer it — a terminal — so on your face it arrives as a report to hand over (setup_incomplete.agent_hook.record_answer names this command), and this is the way the answer gets back in. Ask the human, then record what they said; without it the question stays open for ever and the report is carried on every response. It records and nothing else: no settings file is read or written, so a yes is an answer and not a wiring — the text is still `agent-hook snippet <tool>`, and giving it to an AI that makes the edit stays the human's. A no only stops Amenbo asking; it forbids nothing. The answer belongs to the project, so it covers every folder bound to it, and it replaces whatever was answered before. Never answer it yourself: what is recorded here is a person's answer.",
            "args": [{ "name": "answer", "required": true, "help": "what the person answered: yes or no" }],
            "flags": [{ "name": "--json", "help": "machine-readable output (allowed / next)" }],
            "examples": ["amenbo agent-hook answer yes --actor ai", "amenbo agent-hook answer no --actor ai"] }),
        json!({ "name": "mcp", "summary": "Speaks MCP (JSON-RPC on stdin/stdout) so an AI whose host cannot open a folder can still reach one. A host starts it, never a hand: typing it on a terminal only leaves a process waiting for a protocol nobody is speaking. It is a mediator and not a second Amenbo — every tool call re-runs this same executable in the folder that call named and hands back what that run wrote, so the startup, the integrity check and the reach are the CLI's own, and a folder that is not bound is refused in the words a person typing there would read. One server serves the folders --dir was given, and every call names one of them — required even when the set holds a single folder, so a call never leaves where it lands to a default. The set is the person's: it arrives on the command line the host was given, nothing sent over the streams widens it, and a folder outside it is out of reach with the set named in the answer. Three tools: `agent` (how to work in one folder, in full), `agent_command` (one command's spec), and `run`, which types the caller's own words at any command; each carries the folders it may be called for, so the first call can be right about where it is going. Two things are named rather than passed through — the facet is the server's to declare, so a `--actor` the caller wrote is dropped and `ai` put in its place; and `bind` and `init` are refused, either of them being a way for an AI to re-point a folder it was given and step outside it. Nothing else is added: `--yes` in particular is not, so a destructive command still stops at the confirmation a person gives.",
            "args": [],
            "flags": [{ "name": "--dir <path>...", "required": true, "help": "the folders this server works in — one flag takes them all, every tool call runs in the one it names, and each folder's `.amenbo` decides its project" }],
            "examples": ["amenbo mcp --dir ~/work/site-redesign", "amenbo mcp --dir ~/work/site-redesign ~/work/greenhouse"] }),
        cmd("project add", "Creates a project and links a folder to it, in one act. --dir names an existing folder and is required: a project no folder points at is one no AI can reach, so this door does not make one. What lands in the folder is what bind places — the .amenbo pointer and the managed guidance block. It is refused, before the project exists, when that folder is already linked (project_dir_bound), when a folder above it is already managed (binding_nested_tree), or when it is a git worktree cut inside a managed tree (nested_worktree); and if the linking itself fails, the project it was raised for is taken back with it. The other door is init, which takes the folder you are standing in and names the project after it.",
            json!([{ "name": "--name <str>", "required": true, "help": "project name (required, non-empty)" },
                   { "name": "--dir <path>", "required": true, "help": "the existing folder to link the project to (it receives the .amenbo pointer and the managed guidance block)" },
                   { "name": "--view <list|board|calendar|timeline>", "help": "the view this project opens on; omitted, the configured default_view answers" },
                   { "name": "--notes <str>", "help": "description (Markdown)" },
                   { "name": "--color <str>", "help": "color" }]),
            json!(["amenbo project add --name \"Site Redesign\" --dir ~/work/site-redesign --view board"])),
        cmd("project list", "Lists projects.",
            json!([{ "name": "--archived", "help": "include archived ones too" },
                   { "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo project list --json"])),
        json!({ "name": "project show", "summary": "Shows project details (counts, etc.) plus bound_folders: the folders whose .amenbo points at this project (the reverse of bind), each inspected — exists (false = the folder moved or was deleted), pointer_missing (the folder is there but its .amenbo is gone), legacy (a pre-migration pointer), mismatch (the slug written beside the id is another project's, so the pointer came from elsewhere) and foreign (the pointer names another store outright, so this build is refused in that folder).",
            "args": [{ "name": "id", "required": true, "help": "project ID" }],
            "flags": [{ "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo project show AMB-P-<n> --json"] }),
        json!({ "name": "project update", "summary": "Updates a project.",
            "args": [{ "name": "id", "required": true, "help": "project ID" }],
            "flags": [{ "name": "--name <str>", "help": "" }, { "name": "--notes <str>", "help": "" },
                      { "name": "--view <list|board|calendar|timeline>", "help": "" }, { "name": "--color <str>", "help": "" }],
            "examples": ["amenbo project update AMB-P-<n> --name \"Redesign Project\""] }),
        json!({ "name": "project move", "summary": "Reorders a project.",
            "args": [{ "name": "id", "required": true, "help": "project ID" }],
            "flags": [{ "name": "--before <id>", "help": "before the given ID" }, { "name": "--after <id>", "help": "after the given ID" },
                      { "name": "--top", "help": "to the top" }, { "name": "--bottom", "help": "to the bottom" }],
            "examples": ["amenbo project move AMB-P-<n> --top"] }),
        json!({ "name": "project archive", "summary": "Archives a project.",
            "args": [{ "name": "id", "required": true, "help": "project ID" }],
            "flags": [], "examples": ["amenbo project archive AMB-P-<n>"] }),
        json!({ "name": "project unarchive", "summary": "Unarchives a project.",
            "args": [{ "name": "id", "required": true, "help": "project ID" }],
            "flags": [], "examples": ["amenbo project unarchive AMB-P-<n>"] }),
        json!({ "name": "project delete", "summary": "Deletes a project — permanently, with its tasks and everything hanging off them (a delete is physical and irreversible; archive instead if you want it kept).",
            "args": [{ "name": "id", "required": true, "help": "project ID" }],
            "flags": [{ "name": "--yes", "help": "skip confirmation" }],
            "examples": ["amenbo project delete AMB-P-<n> --yes"] }),

        cmd("dimension add", "Adds a dimension (a user-defined classification axis) to a project. New projects seed no dimensions — create the axes you need. --cardinality says how many of its values one record may hold (single by default; multi lets a record sit on several at once), --ordered gives the values an explicit order, --time-axis marks it as the ordered time lane, --closable marks it as one whose values can be closed rather than deleted, --show-on-card marks it to show on the task card, --applies-to narrows it to one side of the store (left out, it classifies tasks and decisions alike). --time-axis and --cardinality multi are refused together: the current era resolves to one value. --required is refused here too: an axis is born with no values, and one nobody can answer cannot demand an answer — add its values, then raise it with dimension update.",
            json!([{ "name": "--project <id>", "help": "owning project (defaults to the bound project; an AI omits it)" },
                   { "name": "--name <str>", "required": true, "help": "dimension name" },
                   { "name": "--notes <str>", "help": "description / notes (Markdown)" },
                   { "name": "--cardinality <single|multi>", "help": "how many of this axis's values one record may hold: `single` (the default) or `multi`. Refused alongside --time-axis" },
                   { "name": "--ordered", "help": "give the values an explicit order" },
                   { "name": "--time-axis", "help": "mark as the time axis (an ordered view lane)" },
                   { "name": "--closable", "help": "mark this axis as one whose values can be closed — retired from what a record is newly filed under, keeping everything already filed under them. One axis holds one role, so this and --time-axis are refused together" },
                   { "name": "--show-on-card", "help": "mark this axis to show on the task card" },
                   { "name": "--required", "help": "demand a value on this axis before a creation can be finished (refused here — a new axis has no values to answer with)" },
                   { "name": "--applies-to <task|decision|both>", "help": "which side of the store this axis classifies (default: both)" },
                   { "name": "--slug <str>", "help": "readable key for naming it outside Amenbo (lower-case letters, digits and hyphens, starting with a letter). Omit it and the key is derived from the id" }]),
            json!(["amenbo dimension add --name \"Category\" --ordered"])),
        cmd("dimension list", "Lists a project's dimensions in display order, each with its readable key (slug) and its open values. Closed values are left out until --closed asks for them.",
            json!([{ "name": "--project <id>", "help": "target project (defaults to the bound project; an AI omits it)" },
                   { "name": "--closed", "help": "list the closed values too, each marked [closed]" },
                   { "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo dimension list --json"])),
        json!({ "name": "dimension show", "summary": "Shows a dimension: name, its readable key (slug), notes, kind (single or multi, ordered, time-axis, closable, show-on-card, required, and the one side it is narrowed to, if it is), and its open values with their own keys. Closed values are left out until --closed asks for them.",
            "args": [{ "name": "id", "required": true, "help": "dimension id, slug or name" }],
            "flags": [{ "name": "--closed", "help": "show the closed values too, each marked [closed]" },
                      { "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo dimension show Category --json"] }),
        json!({ "name": "dimension update", "summary": "Updates a dimension's name, notes, how many of its values one record may hold, value ordering, its role (time axis or closable), whether it goes on the task card, whether it must be answered, which side of the store it classifies, and/or its readable key. Only the given fields change.",
            "args": [{ "name": "id", "required": true, "help": "dimension id, slug or name" }],
            "flags": [{ "name": "--name <str>", "help": "new name" },
                      { "name": "--notes <str>", "help": "new notes (Markdown). Pass `-` to read it from stdin (a shell eats code spans out of a quoted argument)." },
                      { "name": "--cardinality <single|multi>", "help": "how many of this axis's values one record may hold (`single` / `multi`). Widening is free; going back to single is refused while any record answers with several, and the refusal names how many — clear the extra values off them first. Refused on the time axis either way" },
                      { "name": "--ordered <bool>", "help": "whether the values carry an explicit order" },
                      { "name": "--time-axis <bool>", "help": "name this axis the project's time axis (its values then carry periods), or unname it" },
                      { "name": "--closable <bool>", "help": "name this axis one whose values can be closed, or unname it. The two role flags name one slot, so each reads only its own half — --time-axis false leaves a closable axis closable. An axis that gives the role up keeps whatever was closed under it closed; reopening is free on any axis" },
                      { "name": "--show-on-card <bool>", "help": "whether this axis is marked to show on the task card" },
                      { "name": "--required <bool>", "help": "whether a task must carry a value here before its creation can be finished; raising it needs the axis to offer at least one value" },
                      { "name": "--applies-to <task|decision|both>", "help": "which side of the store this axis classifies; narrowing it leaves the assignments already made on the other side in place, meaning nothing" },
                      { "name": "--slug <str>", "help": "rename the readable key it is named by outside Amenbo" }],
            "examples": ["amenbo dimension update AMB-DIM-<n> --notes \"how we slice work\"",
                         "amenbo dimension update Era --time-axis true",
                         "amenbo dimension update Area --show-on-card true",
                         "amenbo dimension update Product --required true",
                         "amenbo dimension update Occupancy --applies-to task"] }),
        json!({ "name": "dimension move", "summary": "Reorders a dimension within its project.",
            "args": [{ "name": "id", "required": true, "help": "dimension id, slug or name" }],
            "flags": [{ "name": "--before <id>", "help": "" }, { "name": "--after <id>", "help": "" },
                      { "name": "--top", "help": "" }, { "name": "--bottom", "help": "" }],
            "examples": ["amenbo dimension move AMB-DIM-<n> --after AMB-DIM-<m>"] }),
        json!({ "name": "dimension rm", "summary": "Deletes a dimension permanently; its values and task assignments go with it (alias: delete).",
            "args": [{ "name": "id", "required": true, "help": "dimension id, slug or name" }],
            "flags": [{ "name": "--yes", "help": "skip confirmation" }],
            "examples": ["amenbo dimension rm AMB-DIM-<n> --yes"] }),
        json!({ "name": "dimension value-add", "summary": "Adds a value to a dimension (appended after existing values). On a time-axis dimension the value can carry a period.",
            "args": [{ "name": "dimension", "required": true, "help": "dimension id, slug or name" }],
            "flags": [{ "name": "--name <str>", "help": "value name" },
                      { "name": "--start <date>", "help": "first day of the value's period (time-axis dimensions only)" },
                      { "name": "--end <date>", "help": "last day of the value's period; omit to leave it ongoing (time-axis dimensions only)" },
                      { "name": "--slug <str>", "help": "readable key for naming it outside Amenbo (lower-case letters, digits and hyphens, starting with a letter). Omit it and the key is derived from the id" }],
            "examples": ["amenbo dimension value-add Category --name \"Design\"",
                         "amenbo dimension value-add Era --name \"Beta\" --start 2026-07-08"] }),
        json!({ "name": "dimension value-update", "summary": "Updates a dimension value's name, its readable key and/or its period (time-axis dimensions only). Only the given fields change; an open end means the period is ongoing.",
            "args": [{ "name": "dimension", "required": true, "help": "dimension id, slug or name" },
                     { "name": "value", "required": true, "help": "value id, slug or name (within the dimension)" }],
            "flags": [{ "name": "--name <str>", "help": "new name" },
                      { "name": "--start <date>", "help": "first day of the value's period (time-axis dimensions only)" },
                      { "name": "--end <date>", "help": "last day of the value's period; omit to leave it ongoing (time-axis dimensions only)" },
                      { "name": "--clear-start", "help": "open the period's start" },
                      { "name": "--clear-end", "help": "open the period's end (the value becomes ongoing)" },
                      { "name": "--slug <str>", "help": "rename the readable key it is named by outside Amenbo" }],
            "examples": ["amenbo dimension value-update Era Beta --end 2026-12-31",
                         "amenbo dimension value-update Era Beta --clear-end"] }),
        json!({ "name": "dimension value-move", "summary": "Reorders a value within its dimension.",
            "args": [{ "name": "dimension", "required": true, "help": "dimension id, slug or name" },
                     { "name": "value", "required": true, "help": "value id, slug or name (within the dimension)" }],
            "flags": [{ "name": "--before <id>", "help": "" }, { "name": "--after <id>", "help": "" },
                      { "name": "--top", "help": "" }, { "name": "--bottom", "help": "" }],
            "examples": ["amenbo dimension value-move Category Design --top"] }),
        json!({ "name": "dimension value-close", "summary": "Closes a dimension value: nothing is newly filed under it, and everything already filed under it keeps it — the name, the key, the place in the order and every assignment stay, so a filter naming it goes on resolving. That is what closing is for, and what deleting cannot do. Only an axis marked closable (dimension add/update --closable) can close a value, and a required axis keeps one open value to offer: closing the last of them would leave a demand nobody can meet, so lower the requirement first.",
            "args": [{ "name": "dimension", "required": true, "help": "dimension id, slug or name" },
                     { "name": "value", "required": true, "help": "value id, slug or name (within the dimension)" }],
            "examples": ["amenbo dimension value-close リリース v18.0.0"] }),
        json!({ "name": "dimension value-reopen", "summary": "Reopens a closed dimension value, so records can be filed under it again. Free on any axis, whatever role it carries now — an axis that gave the closable role up would otherwise strand what was closed under it. Name a closed value the way you name an open one: it is out of the default listing, not out of reach (dimension list --closed shows it).",
            "args": [{ "name": "dimension", "required": true, "help": "dimension id, slug or name" },
                     { "name": "value", "required": true, "help": "value id, slug or name (within the dimension)" }],
            "examples": ["amenbo dimension value-reopen リリース v18.0.0"] }),
        json!({ "name": "dimension value-rm", "summary": "Deletes a dimension value permanently; its task assignments go with it unless --reassign-to moves them to another value of the same axis (alias: value-delete). A required axis refuses to empty tasks out: with tasks classified as the value, --reassign-to is required, and its last open value is refused outright — a closed value is not an answer, so it does not count, and a requirement nobody can answer would stop every creation on the project, so lower it first.",
            "args": [{ "name": "dimension", "required": true, "help": "dimension id, slug or name" },
                     { "name": "value", "required": true, "help": "value id, slug or name (within the dimension)" }],
            "flags": [{ "name": "--reassign-to <value>", "help": "move the tasks classified as this value to another value of the same dimension" },
                      { "name": "--yes", "help": "skip confirmation" }],
            "examples": ["amenbo dimension value-rm Category Design --yes",
                         "amenbo dimension value-rm テーマ 検索の作り直し --reassign-to メイン --yes"] }),
        json!({ "name": "dimension set", "summary": "Assigns a task or a decision a value of a dimension — one axis and one set of values, whichever of the two is being filed. What happens to what was there is the axis's own answer: a single axis replaces the value it held, a multi one keeps it and adds this. Taking one off is dimension unset either way. The target must say which kind it is: a bare number is refused, tasks and decisions numbering independently.",
            "args": [{ "name": "target", "required": true, "help": "task or decision ref (AMB-T-n / AMB-D-n); a bare number is refused" },
                     { "name": "dimension", "required": true, "help": "dimension id, slug or name" },
                     { "name": "value", "required": true, "help": "value id, slug or name (within the dimension)" }],
            "examples": ["amenbo dimension set AMB-T-42 Category Design",
                         "amenbo dimension set AMB-D-42 Category Design"] }),
        json!({ "name": "dimension unset", "summary": "Clears a task's or a decision's value of a dimension. The target names its kind, as it does on set.",
            "args": [{ "name": "target", "required": true, "help": "task or decision ref (AMB-T-n / AMB-D-n); a bare number is refused" },
                     { "name": "dimension", "required": true, "help": "dimension id, slug or name" },
                     { "name": "value", "required": true, "help": "value id, slug or name (within the dimension)" }],
            "examples": ["amenbo dimension unset AMB-T-42 Category Design"] }),

        cmd("task add", "Creates a task in a project — the first of the two stages every creation has. What it returns is still being created: on the board and in every listing, but out of the mailbox and refused a reservation (ready:no), so the dependencies, premises and classification it needs can be drawn before anyone can pick it up. Finish it with task finish-creating; the response carries the id and that command. Break larger work into separate tasks linked with task depend (no subtasks).",
            json!([{ "name": "--title <str>", "required": true, "help": "title (required, non-empty)" },
                   { "name": "--project <id>", "help": "owning project (a project-less task is refused). A human must name one — omit it to list the existing projects. An AI does not: the binding fills the slot, and naming a project is refused" },
                   { "name": "--due <date>", "help": "due date (YYYY-MM-DD / today / +3d)" },
                   { "name": "--start <date>", "help": "start date" },
                   { "name": "--priority <high|medium|low>", "help": "priority" },
                   { "name": "--notes <str>", "help": "description (Markdown). Pass `-` to read it from stdin (a shell eats code spans out of a quoted argument)." },
                   { "name": "--to <who>", "help": "delegate at creation to a facet — a name / `me` / `human` (the human), or `me-ai` / `ai` (the human's AI); same as a follow-up task assign, saving the create+assign round trip" },
                   { "name": "--ai", "help": "with --to, delegate to 'that person's AI' (assignee_kind=ai)" },
                   { "name": "--dim <axis>=<value>", "help": "classify at creation, resolving names as dimension set does; repeatable for different axes, and for one multi axis named several times (a single axis named twice is refused). It saves the create→dimension set round trip, and what you name wins over the time-axis default" },
                   { "name": "--at <folder>", "help": "the bound folder this task is to be worked in — one of the project's own linked folders, named by its path or just its folder name. Only what you name here lands: the folder the create was typed in is never taken as the default. Having one refuses nothing — no reservation and no worktree is stopped for it" }]),
            json!(["amenbo task add --title \"Create wireframes\" --due tomorrow --priority high",
                   "amenbo task add --title \"Triage logs\" --to Alice --ai",
                   "amenbo task add --title \"Ship the installer\" --dim \"Category=release\"",
                   "amenbo task add --title \"Fix the sender\" --at amenbo-site"])),
        json!({ "name": "task finish-creating", "summary": "Ends the second stage of a creation: the task stops being held back and becomes work anyone can take. Nobody is being asked to approve it — the one who created it is the one who says the writing is finished — so run it as the last step of filing, once the edges and classification the task needs are on it. This is where a required classification is read: an axis its project marked required (dimension update --required) and the task carries no value on refuses the finish with invalid_task_required_dimension, naming the axes to fill in — put a value on each with dimension set. One way only: a task filed by mistake ends with task reject (decided against) or task delete. Idempotent — finishing a creation that is already finished reports a no-op.",
            "args": [{ "name": "id", "required": true, "help": "task ID" }],
            "flags": [], "examples": ["amenbo task finish-creating AMB-T-<n>"] }),
        cmd("task list", "Lists tasks. --limit/--offset page in sort order (JSON carries total_matched = the count before paging, count = this page).",
            json!([{ "name": "--project <id>", "help": "filter by project (human only — an AI is already scoped to its bound project)" },
                   { "name": "--filter <expr>", "help": "filter expression (see filterGrammar)" },
                   { "name": "--sort <key>", "help": "sort (order/due/priority/created/title; prefix - for descending)" },
                   { "name": "--limit <n>", "help": "max count (in sort order; pairs with --offset for paging)" },
                   { "name": "--offset <n>", "help": "number of items to skip in sort order (paging)" },
                   { "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo task list --json",
                   "amenbo task list --filter \"done:false due:today\" --json",
                   "amenbo task list --sort -created --limit 20 --offset 20 --json"])),
        json!({ "name": "task show", "summary": "Shows task details — project, classification (dimensions: the axis=value pairs it is filed under, absent when it is filed under none), the folder it is worked in (folder / `at` in --json, absent when it names none), blockers (blocked_by) and dependents (blocks: what finishing this task would unblock). It carries the timeline too: the count of what has been said on the task, the newest three previewed a line each (cut where they run long), and the way to the rest — --json returns every comment whole, under `comments`. It dates the task itself too: `created`, and `updated` once anything has written to it (both RFC3339 UTC, and `created_at` / `updated_at` in --json). **Any** write moves `updated` — a comment, a due date, a title fix — so it is not when the status last moved, and neither stamp is something to judge from (`AMB-D-372`). Where a task was filed from a pane of the talk window it says which one (`made in` / `made_in` in --json, the pane's name and id, with the handle its session is resumed from beside them) — the line is absent where no pane made it, while the --json key is written either way.",
            "args": [{ "name": "id", "required": true, "help": "task ID" }],
            "flags": [{ "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo task show AMB-T-<n> --json"] }),
        json!({ "name": "task update", "summary": "Updates a task. --start is not a note to self: a day still ahead holds the task at ready:no and refuses its reserve, so declare one only when you mean it (--clear-start takes it back).",
            "args": [{ "name": "id", "required": true, "help": "task ID" }],
            "flags": [{ "name": "--title <str>", "help": "" }, { "name": "--notes <str>", "help": "" },
                      { "name": "--due <date>", "help": "" }, { "name": "--start <date>", "help": "" },
                      { "name": "--priority <high|medium|low>", "help": "" },
                      { "name": "--at <folder>", "help": "the bound folder this task is to be worked in, named by its path or just its folder name — the same slot task add fills, set or moved after the fact" },
                      { "name": "--clear-due", "help": "clear the due date" },
                      { "name": "--clear-start", "help": "clear the start date" },
                      { "name": "--clear-priority", "help": "clear the priority" },
                      { "name": "--clear-at", "help": "clear the folder it is worked in" }],
            "examples": ["amenbo task update AMB-T-<n> --due +2d --priority medium",
                         "amenbo task update AMB-T-<n> --at amenbo-site"] }),
        json!({ "name": "task done", "summary": "Marks a task done — the terminal for work carried out, beside task reject. --report is the completion report, reject's --reason for work that was done: it lands as a comment in the same write as the transition. Idempotent — re-marking a done task changes nothing and does not pile the report on.",
            "args": [{ "name": "id", "required": true, "help": "task ID" }],
            "flags": [{ "name": "--report <str>", "help": "what was done (recorded as a comment). Pass `-` to read it from stdin (a shell eats code spans out of a quoted argument)." }],
            "examples": ["amenbo task done AMB-T-<n> --report \"fixed the empty-title crash; a test covers it\""] }),
        json!({ "name": "task reject", "summary": "Ends a task that will not be done — the terminal beside done, differing only in whether the work was carried out. --reason is required and lands as a comment (no field of its own): a rejection is kept for its reasoning, which is what marking it done (a history that claims what never happened) or deleting it (the reasoning gone with the row) both lose. Closed either way, so it releases the dependents it was holding back and leaves done:false; what was carried out stays status:done. Idempotent — re-rejecting changes nothing and does not pile the reason on.",
            "args": [{ "name": "id", "required": true, "help": "task ID" }],
            "flags": [{ "name": "--reason <str>", "help": "why it will not be done (required, recorded as a comment). Pass `-` to read it from stdin (a shell eats code spans out of a quoted argument)." }],
            "examples": ["amenbo task reject AMB-T-<n> --reason \"measured it — the branch is too thin to be worth the change\""] }),
        json!({ "name": "task reopen", "summary": "Returns an ended task to not-done (sugar for status=todo) — the way back from either terminal, whether it was carried out or decided against.",
            "args": [{ "name": "id", "required": true, "help": "task ID" }],
            "flags": [], "examples": ["amenbo task reopen AMB-T-<n>"] }),
        json!({ "name": "task status", "summary": "Explicitly changes the progress state (todo/in_progress/done/blocked/rejected). Setting in_progress reserves the task: a compare-and-swap that succeeds only from todo, so a second session's reserve is rejected with already_reserved (the double-work guard), and only a ready task can be reserved, so an open blocker, an unsettled premise, a start day still ahead, or a creation nobody has finished is rejected with not_ready (there is no --force; correct the declaration with task update --start, or end the creation with task finish-creating). A creation nobody has finished also refuses done/blocked/rejected with invalid_task_status_draft, so a task still being created stays at todo — end the creation, or, if it was written in error, remove it with task delete. todo hands it back. done marks completed. rejected ends it as decided against — reach it through task reject, which asks for the reasoning this route does not. blocked declares an external stall only — an unmet premise is derived as ready:no, never declared here.",
            "args": [{ "name": "id", "required": true, "help": "task ID" },
                     { "name": "status", "required": true, "help": "todo / in_progress / done / blocked / rejected" }],
            "flags": [], "examples": ["amenbo task status AMB-T-<n> in_progress", "amenbo task status AMB-T-<n> done"] }),
        json!({ "name": "task block", "summary": "Marks blocked (stuck) — for an external stall only (a second machine, a human go/no-go); an unmet premise is derived as ready:no instead. --reason is recorded as a comment.",
            "args": [{ "name": "id", "required": true, "help": "task ID" }],
            "flags": [{ "name": "--reason <str>", "help": "reason it is stuck (recorded as a comment). Pass `-` to read it from stdin (a shell eats code spans out of a quoted argument)." }],
            "examples": ["amenbo task block AMB-T-<n> --reason \"Awaiting client confirmation\""] }),
        json!({ "name": "task move", "summary": "Re-homes a task to another project and reorders it (a task belongs to exactly one project).",
            "args": [{ "name": "id", "required": true, "help": "task ID" }],
            "flags": [{ "name": "--project <id>", "help": "destination project (omit it to reorder within the current project). An AI cannot re-home a task — every other project is outside its reach" },
                      { "name": "--before <id>", "help": "" }, { "name": "--after <id>", "help": "" },
                      { "name": "--top", "help": "" }, { "name": "--bottom", "help": "" }],
            "examples": ["amenbo task move AMB-T-<n> --project AMB-P-<n> --top"] }),
        json!({ "name": "task depend", "summary": "Makes this task depend on another task (--on is the one that has to be done first). The edge is what carries the order past the session that knew it: task show reads it from both ends — blocked_by, and blocks (what finishing this one would release) — so whoever arrives later takes the work in the order it actually has, instead of inferring it from titles. Self-reference and cycles are rejected, and so is an edge that would cross projects (a project's context must not leak into another — both ends must sit in the same project; an inbox task, belonging to none, is not a crossing). Idempotent.",
            "args": [{ "name": "id", "required": true, "help": "the task ID being blocked" }],
            "flags": [{ "name": "--on <id>", "required": true, "help": "the task ID of the blocker that must be done first" }],
            "examples": ["amenbo task depend AMB-T-<n> --on AMB-T-<m>"] }),
        json!({ "name": "task undepend", "summary": "Removes a dependency (idempotent). If removal makes the task startable, it emits task.unblocked.",
            "args": [{ "name": "id", "required": true, "help": "the task ID being blocked" }],
            "flags": [{ "name": "--on <id>", "required": true, "help": "the blocker task ID to remove" }],
            "examples": ["amenbo task undepend AMB-T-<n> --on AMB-T-<m>"] }),
        json!({ "name": "task commit-add", "summary": "Records a git commit SHA on a task (1 task : many commits) — the anchor from history back to a task, since a public commit carries no store-local reference. Amenbo stores the SHA as an opaque string: it never reads git, verifies the commit, or knows which forge it lives on. The SHA is validated at the door — only full-length lower-case hex is admitted (40 for SHA-1, 64 for SHA-256), case is folded, and short forms, branches, tags and revisions are refused. Idempotent: a SHA already on the task is a no-op (the `(task_id, sha)` index sees bytes only).",
            "args": [{ "name": "task", "required": true, "help": "task ID" },
                     { "name": "sha", "required": true, "help": "the full commit SHA — 40 hex for SHA-1, 64 for SHA-256 (short forms, branches, tags and revisions are refused)" }],
            "flags": [],
            "examples": ["amenbo task commit-add AMB-T-<n> 0123456789abcdef0123456789abcdef01234567"] }),
        json!({ "name": "task commit-list", "summary": "Lists a task's recorded commit SHAs, oldest first. To go the other way — the task a SHA belongs to — read the commit with `git show <sha>` (Amenbo does not read git).",
            "args": [{ "name": "task", "required": true, "help": "task ID" }],
            "flags": [{ "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo task commit-list AMB-T-<n> --json"] }),
        json!({ "name": "task commit-rm", "summary": "Forgets a commit SHA on a task — a hard delete (idempotent; the SHA is normalised the way it was stored, so any case removes it). The commit itself and the task are untouched.",
            "args": [{ "name": "task", "required": true, "help": "task ID" },
                     { "name": "sha", "required": true, "help": "the commit SHA to forget (any case — normalised the way it was stored)" }],
            "flags": [{ "name": "--yes", "help": "skip confirmation" }],
            "examples": ["amenbo task commit-rm AMB-T-<n> 0123456789abcdef0123456789abcdef01234567 --yes"] }),
        json!({ "name": "task assign", "summary": "Assigns an assignee to a task. Use --ai to delegate to 'that person's AI' (assignee_kind=ai). Reassignment is plain — the task just moves to its new assignee, with no special status.",
            "args": [{ "name": "id", "required": true, "help": "task ID" }],
            "flags": [{ "name": "--to <who>", "required": true, "help": "assignee facet: `me` / `self` / `human` or the human's display name → the human; `me-ai` / `ai` → the human's AI. The account-id / public-key forms are gone with the account reference dimension." },
                      { "name": "ai", "help": "delegate to 'that person's AI' (assignee_kind=ai)" }],
            "examples": ["amenbo task assign AMB-T-<n> --to Sato", "amenbo task assign AMB-T-<n> --to Sato --ai"] }),
        json!({ "name": "task unassign", "summary": "Removes a task's assignee.",
            "args": [{ "name": "id", "required": true, "help": "task ID" }],
            "flags": [], "examples": ["amenbo task unassign AMB-T-<n>"] }),
        json!({ "name": "task delete", "summary": "Deletes a task permanently, with its comments, dependency edges and attachments (a delete is physical and irreversible).",
            "args": [{ "name": "id", "required": true, "help": "task ID" }],
            "flags": [{ "name": "--yes", "help": "skip confirmation" }],
            "examples": ["amenbo task delete AMB-T-<n> --yes"] }),


        json!({ "name": "comment rm", "summary": "Deletes a comment posted by mistake — permanently, and its attachments go with it. Identify the comment by id; `comment list` prints it.",
            "args": [{ "name": "comment", "required": true, "help": "target task comment ref, AMB-TC-n (from `comment list`)" }],
            "flags": [{ "name": "--yes", "help": "skip confirmation" }],
            "examples": ["amenbo comment rm AMB-TC-<n> --yes"] }),
        json!({ "name": "comment edit", "summary": "Rewrites a comment's body in place — the id, its place on the timeline, and its attachments all stay, so links to it keep resolving. Prefer this over deleting and re-posting when you only need to fix what a comment says. Identify the comment by id; `comment list` prints it.",
            "args": [{ "name": "comment", "required": true, "help": "target task comment ref, AMB-TC-n (from `comment list`)" }],
            "flags": [{ "name": "--text <str>", "required": true, "help": "the new body, as Markdown — it replaces the old one outright. Pass `-` to read it from stdin (a shell eats code spans out of a quoted argument)." }],
            "examples": ["amenbo comment edit AMB-TC-<n> --text \"Corrected: the benchmark was 10k, not 1k\""] }),
        json!({ "name": "comment add", "summary": "Adds a comment to a task.",
            "args": [{ "name": "task", "required": true, "help": "target task ID" }],
            "flags": [{ "name": "--text <str>", "required": true, "help": "comment body (Markdown). Pass `-` to read it from stdin (a shell eats code spans out of a quoted argument)." }],
            "examples": ["amenbo comment add AMB-T-<n> --text \"Awaiting client confirmation\""] }),
        json!({ "name": "comment list", "summary": "Shows a task's comments, oldest first. --limit/--offset page (JSON carries total_matched = the count before paging, count = this page).",
            "args": [{ "name": "task", "required": true, "help": "target task ID" }],
            "flags": [{ "name": "--limit <n>", "help": "max count (oldest first; pairs with --offset for paging)" },
                      { "name": "--offset <n>", "help": "number of items to skip, oldest first (paging)" },
                      { "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo comment list AMB-T-<n> --json", "amenbo comment list AMB-T-<n> --limit 20 --offset 20 --json"] }),
        json!({ "name": "comment attach", "summary": "Attaches a file (content-addressed `blob`) or external link (--url) to a single TASK comment, kept separate from the parent task's own attachments so a comment's own attachment timeline is preserved. Same two modes as `task attach`, and the same judgement of what is worth attaching — read it there. Identify the comment by id; find ids with `comment list <task> --json`. A decision comment is attached to with `decision comment-attach` — the two comment tables number apart, so the command, not the id, says which table an id belongs to. List them with `attach ls --task-comment <id>`.",
            "args": [{ "name": "comment", "required": true, "help": "target task comment ref, AMB-TC-n (from `comment list`)" },
                     { "name": "source", "required": true, "help": "file path to ingest as a blob, or the external URL with --url" }],
            "flags": [{ "name": "--url", "help": "treat <source> as an external URL link instead of ingesting a file" },
                      { "name": "--name <str>", "help": "display label (defaults to the file name / URL; on a file it keeps that file's suffix, and what the file is stays read from the file)" }],
            "examples": ["amenbo comment attach AMB-TC-<n> ./note.png", "amenbo comment attach AMB-TC-<n> https://example.com/spec --url --name spec"] }),
        json!({ "name": "decision comment-attach", "summary": "Attaches a file (content-addressed `blob`) or external link (--url) to a single DECISION comment — the mirror of `comment attach` (which takes task comments), down to the judgement of what is worth attaching (`task attach`). Identify the comment by id; find ids with `decision comment-list <decision> --json`. List them with `attach ls --decision-comment <id>`.",
            "args": [{ "name": "comment", "required": true, "help": "target decision comment ref, AMB-DC-n (from `decision comment-list`)" },
                     { "name": "source", "required": true, "help": "file path to ingest as a blob, or the external URL with --url" }],
            "flags": [{ "name": "--url", "help": "treat <source> as an external URL link instead of ingesting a file" },
                      { "name": "--name <str>", "help": "display label (defaults to the file name / URL; on a file it keeps that file's suffix, and what the file is stays read from the file)" }],
            "examples": ["amenbo decision comment-attach AMB-DC-<n> ./benchmark.csv"] }),

        json!({ "name": "decision add", "summary": "Opens a new decision, as a draft — an append-only \"why we chose X\", and a premise that holds now rather than a note of what happened. Only what the human settled with you belongs on one (`AMB-D-917`); a choice you made on your own goes in a comment on the task, and `decision promote` raises it later if the work turns out to rest on it. A decision is a Task sibling (project-scoped), NOT a task: it has no mailbox workflow and never appears in task lists. Decisions have their own device-global number space, shown as AMB-D-N (tasks are AMB-T-N); the kind code keeps AMB-D-<n> / AMB-T-<n> unambiguous. The body should be the conclusion + rationale (compress; do not paste raw discussion, and keep PII out). The project defaults to the bound project. Classify it here with --dim: nothing is demanded at the record (`AMB-D-925`) — the axes a project marked required are read one stage on, at `decision finish-writing`, which will not end the writing until they are answered. A record left blank on one goes through, and the response names what is still to classify (`unmet_required_dimensions` in --json); fill it in with `dimension set`, or save the round trip by naming it here. The time axis is filled from the era that contains today, so it is rarely among them.",
            "args": [], "flags": [{ "name": "--title <str>", "required": true, "help": "decision title" },
                                   { "name": "--body <str>", "help": "conclusion + rationale (Markdown). Pass `-` to read it from stdin (a shell eats code spans out of a quoted argument)." },
                                   { "name": "--dim <axis>=<value>", "help": "classify at creation, resolving names as dimension set does; repeatable for different axes, and for one multi axis named several times (a single axis named twice is refused, and so is an axis that does not classify decisions). It saves the create→dimension set round trip; a required axis left blank is not refused here, it is refused at `decision finish-writing`" },
                                   { "name": "--project <id>", "help": "project (name or ID; defaults to the bound project — an AI omits it, and naming one is refused)" }],
            "examples": ["amenbo decision add --title \"Adopt UTC for stored timestamps\" --body \"store in UTC and localize on display — removes timezone ambiguity\" --project AMB-P-<n>"] }),
        cmd("decision list", "Lists decisions (status:decided|rejected — a decision has the two ends and no third (`AMB-D-918`), so being still written is not one of them: that is draft:yes|no, the premise that holds a task resting on it out of ready, superseded:yes|no — the edge itself, so superseded:yes lists the decisions another decision draws a supersedes edge at — project:, dim:<axis>=<value> and its time_axis:<value> sugar, which read the same axes the tasks are classified on and fold the same way (different axes AND, the same axis ORs), decided_before:/decided_after: over the day a decision was settled (YYYY-MM-DD, or today/-30d; both ends inclusive; a decision still being written has no such day and matches neither)). Words are not among the keys — search <word> --kind decision is what finds where they are written. To ask which policies were settled by a date, compose this filter with superseded: — there is no separate as-of switch, and the composition recovers neither status transitions nor deleted decisions. Sort by decided/created/number/title/status (prefix - for descending; default -created). --limit/--offset page (JSON carries total_matched = the count before paging, count = this page). --with-body adds each decision's body to the rows — a projection that composes with --filter/--limit/--offset (narrow by status and page; it does not dump the whole corpus).",
            json!([{ "name": "--project <id>", "help": "limit to the given project (human only)" },
                   { "name": "--filter <expr>", "help": "e.g. status:decided draft:no superseded:no dim:Theme=main" },
                   { "name": "--sort <key>", "help": "decided/created/number/title/status (- for descending; default -created)" },
                   { "name": "--limit <n>", "help": "max count (in sort order; pairs with --offset for paging)" },
                   { "name": "--offset <n>", "help": "number of items to skip in sort order (paging)" },
                   { "name": "--with-body", "help": "include each decision's body (projection; composes with --filter/--limit/--offset)" },
                   { "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo decision list --filter \"status:decided draft:no\" --json", "amenbo decision list --sort -decided", "amenbo decision list --filter \"status:decided superseded:no\" --with-body --limit 20 --json"])),
        json!({ "name": "decision show", "summary": "Shows a decision: body, status, the supersession chain (both directions), the premises it builds on (read those first — a premise another decision has overturned is flagged, because this decision then stands on rotten ground) and the decisions that build on it (the impact radius: overturn this one and they want revisiting), and its linked tasks. It dates itself as well — when it was recorded, when it was settled and by which facet, and when it last changed where that is neither of the two — so how fresh the record is, and who ended its writing, come off the page rather than being guessed at. Its timeline comes with it, in the same shape task show gives its own: the count of what has been said, the newest three previewed a line each, and the way to the rest — which is where the reason a decision was settled or rejected lives, since --reason writes one as a comment (--json returns every comment whole, under `comments`). Which session recorded it comes with it too, in the shape and on the terms task show gives its own (`made in` / `made_in`).",
            "args": [{ "name": "id", "required": true, "help": "decision ref (AMB-D-n)" }],
            "flags": [{ "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo decision show AMB-D-<n> --json"] }),
        json!({ "name": "decision edit", "summary": "Edits a decision's title/body in place — one still being written and a settled one alike. Editing is not re-deciding, so a settled decision's decided_at/decided_by are left untouched, and there is no revision history. Supersede when a new decision replaces it; a rejected decision is terminal and cannot be edited.",
            "args": [{ "name": "id", "required": true, "help": "decision ref (AMB-D-n)" }],
            "flags": [{ "name": "--title <str>", "help": "new title" }, { "name": "--body <str>", "help": "new body (Markdown). Pass `-` to read it from stdin (a shell eats code spans out of a quoted argument)." }],
            "examples": ["amenbo decision edit AMB-D-<n> --body \"…refined rationale…\""] }),
        json!({ "name": "decision finish-writing", "summary": "Ends the writing of a decision — the second stage of recording one, the twin of task finish-creating. It lowers the draft flag, settles the decision, stamps decided_at/decided_by with whoever finished it, and releases the tasks that rest on it. A human or an AI may say it: what it says is that the writing is finished, which only its writer knows. It is where a classification the project requires of decisions is read (the axes whose applies_to covers decisions): an axis left blank holds the writing and is named in the refusal. No-op on a decision already written — reopen it first to write it again. --reason records why it was settled as a decision comment; the reason lives on the timeline, not in a dedicated field.",
            "args": [{ "name": "id", "required": true, "help": "decision ref (AMB-D-n)" }],
            "flags": [{ "name": "--reason <str>", "help": "why it was settled (recorded as a decision comment). Pass `-` to read it from stdin (a shell eats code spans out of a quoted argument)." }],
            "examples": ["amenbo decision finish-writing AMB-D-<n>", "amenbo decision finish-writing AMB-D-<n> --reason \"agreed after the perf review\""] }),
        json!({ "name": "decision reject", "summary": "Turns down a decision still being written (draft → rejected), for one considered and not taken; a settled one is refused, and a record written in error goes by decision delete. --reason records the reason for rejecting as a decision comment — the reason lives on the timeline, not in a dedicated field.",
            "args": [{ "name": "id", "required": true, "help": "decision ref (AMB-D-n)" }],
            "flags": [{ "name": "--reason <str>", "help": "reason for rejecting (recorded as a decision comment). Pass `-` to read it from stdin (a shell eats code spans out of a quoted argument)." }],
            "examples": ["amenbo decision reject AMB-D-<n>", "amenbo decision reject AMB-D-<n> --reason \"superseded by the simpler approach in AMB-D-<m>\""] }),
        json!({ "name": "decision reopen", "summary": "Puts a settled decision back in hand: raises the draft flag again and clears decided_at/decided_by, which sends the tasks that rest on it back to ready:no. The status stays decided — what a reopen undoes is the end of the writing, not the verdict (`AMB-D-918`). Use it to write out again one you finished too soon — neither reject (turning it down) nor supersede (a replacement) says that. It is not needed to edit: a settled decision edits in place. No-op if the writing is open already; refused for rejected decisions. A decision another one supersedes can be reopened too, since being superseded is derived from the edge, not a status.",
            "args": [{ "name": "id", "required": true, "help": "decision ref (AMB-D-n)" }],
            "flags": [], "examples": ["amenbo decision reopen AMB-D-<n>"] }),
        json!({ "name": "decision delete", "summary": "Deletes (retires) a decision — settled ones included. The delete is physical and irreversible, its comments and edges go with it, and linked tasks are unlinked. Use this to retire a decision outright; use supersede when a new decision replaces it (which keeps the old one readable).",
            "args": [{ "name": "id", "required": true, "help": "decision ref (AMB-D-n)" }],
            "flags": [{ "name": "--yes", "help": "skip the confirmation prompt" }],
            "examples": ["amenbo decision delete AMB-D-<n> --yes"] }),
        json!({ "name": "decision supersede", "summary": "Records that a new decision replaces an existing one (supersession chain): the new one draws a `supersedes` edge at the old one, which stops being current (the old row itself is not touched — currency is derived from the edge, not stored). A decision may supersede several others — each supersede draws its own edge, none replaces the last. Neither side's row is rewritten: settling the new one is `decision finish-writing`'s job, said separately (`AMB-D-918`).",
            "args": [{ "name": "decision", "required": true, "help": "the new decision (it replaces the old one)" }],
            "flags": [{ "name": "--replaces <id>", "required": true, "help": "the decision being replaced" }],
            "examples": ["amenbo decision supersede AMB-D-<n> --replaces AMB-D-<m>"] }),
        json!({ "name": "decision amend", "summary": "Records that a new decision amends (partially revises) an existing one: the new one draws an `amends` edge at the old one, which stays current (not superseded) — read the two together. A decision may amend several others (one edge each). Amend only records the revision link; it touches neither side's row — ending the amending side's writing is `decision finish-writing`'s job, said separately (`AMB-D-918`). Use supersede when the new decision fully replaces the old one.",
            "args": [{ "name": "decision", "required": true, "help": "the new decision (it amends the old one)" }],
            "flags": [{ "name": "--amends <id>", "required": true, "help": "the decision being amended (stays current)" }],
            "examples": ["amenbo decision amend AMB-D-<n> --amends AMB-D-<m>"] }),
        json!({ "name": "decision builds-on", "summary": "Records that a decision builds on (takes as a premise) an existing one: the standing decision draws a `builds_on` edge at the premise, which stays current and is not corrected — the edge only says read the premise first, and revisit this decision if the premise is ever overturned. Draw it only when that revisiting test says yes: same topic, cited in the body, or merely consulted is not a premise. supersedes / amends already imply it, so drawing it on a pair that carries one of them is a no-op (one pair, one edge). The reverse lookup is the impact radius `decision supersede` / `reject` / `delete` show you.",
            "args": [{ "name": "decision", "required": true, "help": "the decision that stands on the premise" }],
            "flags": [{ "name": "--on <id>", "required": true, "help": "the premise it stands on (stays current)" }],
            "examples": ["amenbo decision builds-on AMB-D-<n> --on AMB-D-<m>"] }),
        json!({ "name": "decision unlink", "summary": "Removes a decision-to-decision edge that should never have been drawn (supersedes / amends / builds_on alike — a pair carries one edge, so naming the pair names it). This is a correction, not a reversal of the decision: superseding a decision back is a new decision, whereas an edge drawn at the wrong target is a miswiring with nothing to remember. Removing a `supersedes` edge makes its target current again on its own (currency is derived from the edges, not stored). No-op when the pair carries no edge.",
            "args": [{ "name": "decision", "required": true, "help": "the decision the edge was drawn from (the newer one)" }],
            "flags": [{ "name": "--from <id>", "required": true, "help": "the decision it points at (the older one)" }],
            "examples": ["amenbo decision unlink AMB-D-<n> --from AMB-D-<m>"] }),
        json!({ "name": "decision link", "summary": "Links (or --unlink) a decision and a task — the decision is the task's premise (many-to-many). The edge is how the reasoning reaches whoever picks the work up: the task names the decision it rests on, and the decision names the work it produced, so a session arriving later reads why the work has this shape instead of deciding it over again. Draw it while you are the one who knows — a task linked to nothing carries only its notes. Link implementation tasks only: a purely historical reference belongs in the decision's body, not in an edge, and a decision linked to the task of ruling on it locks that task forever. The task must sit in the decision's own project (no edge crosses projects; an inbox task, belonging to none, is not a crossing).",
            "args": [{ "name": "decision", "required": true, "help": "decision ref (AMB-D-n)" }, { "name": "task", "required": true, "help": "task ref (AMB-T-n)" }],
            "flags": [{ "name": "--unlink", "help": "remove the link instead of creating it" }],
            "examples": ["amenbo decision link AMB-D-<n> AMB-T-<n>", "amenbo decision link AMB-D-<n> AMB-T-<n> --unlink"] }),
        json!({ "name": "decision promote", "summary": "Promotes a comment into a decision: the comment text becomes the body, and the project defaults to the project of what the comment sits on. What is drawn afterwards differs by kind. A task comment (AMB-TC-n) links the new decision to that task — the decision is that task's premise. A decision comment (AMB-DC-n) draws no edge: a record raised out of a decision's thread is a question that turned into its own, and a link would claim a relation this cannot know — where one holds, name it yourself with builds-on / amend / supersede. The two tables number independently, so a bare <n> naming a row in each is refused: spell the kind code. Classify it here with --dim, the same as `decision add`: nothing is demanded at the record (`AMB-D-925`), and the axes a project marked required are read at `decision finish-writing`. The time axis is filled by the create, from the era containing today.",
            "args": [{ "name": "comment", "required": true, "help": "the comment ref to promote, AMB-TC-n (on a task) or AMB-DC-n (on a decision)" }],
            "flags": [{ "name": "--title <str>", "required": true, "help": "decision title" },
                      { "name": "--project <id>", "help": "project (defaults to the project of the comment's task or decision)" },
                      { "name": "--dim <AXIS=VALUE>", "help": "classify the new decision (repeatable for different axes; refused on an axis named twice or one that does not classify decisions)" }],
            "examples": ["amenbo decision promote AMB-TC-<n> --title \"Standardize on ISO-8601 dates\"", "amenbo decision promote AMB-DC-<n> --title \"Standardize on ISO-8601 dates\" --dim \"Theme=main\""] }),
        json!({ "name": "decision comment-rm", "summary": "Deletes a comment posted by mistake — permanently, and its attachments go with it. Identify the comment by id; `decision comment-list` prints it.",
            "args": [{ "name": "comment", "required": true, "help": "target decision comment ref, AMB-DC-n (from `decision comment-list`)" }],
            "flags": [{ "name": "--yes", "help": "skip confirmation" }],
            "examples": ["amenbo decision comment-rm AMB-DC-<n> --yes"] }),
        json!({ "name": "decision comment-edit", "summary": "Rewrites a comment's body in place — the id, its place on the timeline, and its attachments all stay. This edits a comment, not the decision's own body (conclusion + rationale); the two are separate. Identify the comment by id; `decision comment-list` prints it.",
            "args": [{ "name": "comment", "required": true, "help": "target decision comment ref, AMB-DC-n (from `decision comment-list`)" }],
            "flags": [{ "name": "--text <str>", "required": true, "help": "the new body, as Markdown — it replaces the old one outright. Pass `-` to read it from stdin (a shell eats code spans out of a quoted argument)." }],
            "examples": ["amenbo decision comment-edit AMB-DC-<n> --text \"Corrected: the benchmark was 10k, not 1k\""] }),
        json!({ "name": "decision comment-add", "summary": "Adds a comment to a decision's timeline. The decision's own body (conclusion + rationale) holds what was decided; comments are the way to discuss it or record why it was settled or rejected (`decision finish-writing/reject --reason` is thin sugar over this).",
            "args": [{ "name": "decision", "required": true, "help": "target decision ref (AMB-D-n)" }],
            "flags": [{ "name": "--text <str>", "required": true, "help": "comment body (Markdown). Pass `-` to read it from stdin (a shell eats code spans out of a quoted argument)." }],
            "examples": ["amenbo decision comment-add AMB-D-<n> --text \"revisited after the 10k benchmark — still holds\""] }),
        json!({ "name": "decision comment-list", "summary": "Shows a decision's comments, oldest first. --limit/--offset page (JSON carries total_matched = the count before paging, count = this page).",
            "args": [{ "name": "decision", "required": true, "help": "target decision ref (AMB-D-n)" }],
            "flags": [{ "name": "--limit <n>", "help": "max count (oldest first; pairs with --offset for paging)" },
                      { "name": "--offset <n>", "help": "number of items to skip, oldest first (paging)" },
                      { "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo decision comment-list AMB-D-<n> --json"] }),

        json!({ "name": "task attach", "summary": "Attaches a file or external link to a task. WHAT TO ATTACH: an attachment's bytes are not searchable — `search` reaches an attachment's filename and its link address, never what is inside it — so text is always the body's job, and attaching is giving up on ever finding it again. The test: if the content holds a word you might one day search for, that word stays in the body. An attachment is not where words go to disappear; it is where the backing evidence sits. Attach non-text you generated yourself — the screenshot of a GUI check (so the next session can see what you verified instead of taking your word for it), images, video, PDF, binary samples; they could never be searched anyway, so nothing is lost, but write one line in the body saying what is in it. Attach long raw data behind a conclusion — a failing run's log, a profile, a before/after benchmark: keep the conclusion and the fragments worth searching (the error line, the identifier) in the body, and attach only the raw data. Do not attach text that fits in the body (short output, a minimal repro), source or diffs (anchor those with `task commit-add`), or reasoning and history (the body and the comments carry those). A URL belongs in the body; `--url` adds a click path for the human GUI, it does not replace writing the link down. MECHANICS: a file is ingested as a content-addressed `blob` (the bytes are copied into the store keyed by their BLAKE3 digest; the truth source records only metadata — hash/filename/mime/size); --url instead records an external link (`url` mode, not managed). MIME is guessed from the file extension. The blob is checked against the per-file size cap before ingest. Manage attachments with `attach ls/show/open/rm`.",
            "args": [{ "name": "id", "required": true, "help": "target task ref (AMB-T-n)" },
                     { "name": "source", "required": true, "help": "file path to ingest, or the external URL with --url" }],
            "flags": [{ "name": "--url", "help": "treat <source> as an external URL link instead of ingesting a file" },
                      { "name": "--name <str>", "help": "display label (defaults to the file name / URL; on a file it keeps that file's suffix, and what the file is stays read from the file)" }],
            "examples": ["amenbo task attach AMB-T-<n> ./design.png", "amenbo task attach AMB-T-<n> https://example.com/spec --url --name spec"] }),
        json!({ "name": "decision attach", "summary": "Attaches a file (content-addressed `blob`) or external link (--url) to a decision. Same two modes as `task attach`, and the same judgement of what is worth attaching — read it there. Manage with `attach ls/show/open/rm`.",
            "args": [{ "name": "id", "required": true, "help": "target decision ref (AMB-D-n)" },
                     { "name": "source", "required": true, "help": "file path to ingest, or the external URL with --url" }],
            "flags": [{ "name": "--url", "help": "treat <source> as an external URL link instead of ingesting a file" },
                      { "name": "--name <str>", "help": "display label (defaults to the file name / URL; on a file it keeps that file's suffix, and what the file is stays read from the file)" }],
            "examples": ["amenbo decision attach AMB-D-<n> ./benchmark.csv"] }),
        cmd("attach ls", "Lists the attachments on a task, decision, or a single comment, in attach order. WHETHER TO OPEN ONE: decide from the metadata this prints — name, mime, size — and from what the body already told you. The body carries the conclusion and the searchable words (see `task attach`); an attachment is the backing evidence behind them, so most of the time the listing alone answers your question and reading the bytes only spends context. Open one when you actually need the evidence — the body's claim is what you must check, or the raw data is what you were sent for — and when in doubt, do not: an attachment you read and did not need has cost you the very context it was put there to save. To read one, `attach save --out <path>` writes the bytes to a file you can open (`attach open` hands it to the OS's default opener, which is the human's route, not yours). A comment is named by a flag, not by the positional target: the task and decision comment tables number apart, so a bare id cannot say which table it belongs to.",
            json!([{ "name": "target", "help": "task / decision ref (AMB-T-n / AMB-D-n — the kind code is what disjoins the two spaces)" },
                   { "name": "--task-comment <id>", "help": "list this task comment's attachments (id from `comment list`)" },
                   { "name": "--decision-comment <id>", "help": "list this decision comment's attachments (id from `decision comment-list`)" }]),
            json!(["amenbo attach ls AMB-T-<n> --json", "amenbo attach ls --task-comment 42 --json"])),
        cmd("attach show", "Shows one attachment's metadata (kind, filename, mime, size, blob hash or url).",
            json!([{ "name": "id", "required": true, "help": "attachment id" }]),
            json!(["amenbo attach show 01ATT…"])),
        cmd("attach open", "Opens an attachment — a blob via the OS default opener, or the external URL. This puts it in front of the human at their screen; an agent reads an attachment with `attach save` instead. A blob whose bytes are not present locally reports not_found.",
            json!([{ "name": "id", "required": true, "help": "attachment id" }]),
            json!(["amenbo attach open 01ATT…"])),
        cmd("attach save", "Saves a blob attachment's bytes to a file — the CLI counterpart of the GUI's download (`open` only spills to a temp file, and `export` takes the whole store), and the way an agent reads an attachment: save it, then read the file. Decide whether it is worth reading before you save it, from `attach ls`'s metadata — the bytes land in your context and rarely repay it. `--out` is a file path, or an existing directory to save under the attachment's own filename; with no `--out` that filename lands in the current directory. Refuses to overwrite an existing destination unless `--force`. A URL attachment has no bytes to save (open the link with `attach open`); a blob whose bytes are not present locally reports not_found.",
            json!([{ "name": "id", "required": true, "help": "attachment id" },
                   { "name": "--out <path>", "help": "file path, or a directory to save under the attachment's filename (default: that filename in the CWD)" },
                   { "name": "--force", "help": "overwrite the destination if it exists (default refuses)" }]),
            json!(["amenbo attach save 01ATT… --out ./spec.pdf", "amenbo attach save 01ATT… --out ~/Downloads"])),
        cmd("attach rm", "Removes an attachment — permanently. The blob bytes are reclaimed with the attachment once nothing else references them (content-addressing means another attachment may share the same bytes — those are left alone). Bytes ingested within the last hour are kept for now, in case an attach is in flight elsewhere; the sweep in `doctor --fix` collects them later. Destructive — confirms unless --yes.",
            json!([{ "name": "id", "required": true, "help": "attachment id" },
                   { "name": "--yes", "help": "skip confirmation" }]),
            json!(["amenbo attach rm 01ATT… --yes"])),

        cmd("export", "Exports all data — everything on this device, as JSON, and nothing narrower: export exists for moving to another tool, which an excerpt or a human-readable table does not serve. The core of data sovereignty, and one way: Amenbo writes your data out for whatever you move to next, and reads nothing back in — the way back is `restore` from a `backup` archive. `--out <dir>` writes an **export directory**: `export.json` plus `attachments/`, holding every attachment's actual file under the task or decision it hangs on (each row names its `export_path`). With no `--out` the same JSON streams to stdout — a stream has nowhere to put the files, so that shape carries records only. **A closed reach is never handed that stream** (`AMB-D-224`): the whole device's content landing in the caller's terminal is the one thing a closed reach is closed to, so where no `--out` is named a destination is chosen instead — `amenbo-export-<UTC stamp>` under the current folder — and what comes back is its path and a count. Redirecting such a call catches the summary line and leaves the export sitting in that folder. The secrets are the one thing left behind (`AMB-D-434`): this file goes out to another tool and stays in its hands, and a credential in the clear is not something to hand over on the way past — they ride `backup` instead.",
            json!([{ "name": "--out <path>", "help": "the export directory to create (must not exist yet). Default: stdout, except on a closed reach, which gets a directory named amenbo-export-<UTC stamp> under the current folder" }]),
            json!(["amenbo export --out ./amenbo-export",
                   "amenbo export > ./amenbo-export.json"])),
        json!({ "name": "backup", "summary": "Backs up everything on this device — one database, holding every project — into one verified `.amenbo-backup` archive at the given path (VACUUM INTO: checkpointed, transactionally consistent, no torn DB+WAL; bounded-verified; the manifest records its migration generation). The attachment bytes (blobs) are bundled too, so a restore elsewhere brings the files back and not just the rows referencing them, and the device's skins (`<base>/skins/`, each in the shape it is kept in) ride along for the same reason — the config names one by a word, and the word alone restores nothing. The device's own secrets (at-rest key / identity) are not part of the engine, so none are included; the secrets a feature holds are store rows, so those do ride along and come back working (`AMB-D-434`). The destination must not already exist (managed generation rotation is retired).",
            "args": [{ "name": "path", "required": false, "help": "destination .amenbo-backup archive that must not already exist" }],
            "flags": [{ "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo backup ./everything.amenbo-backup"] }),
        json!({ "name": "skin list", "summary": "Every skin that can be worn here, and which of them is on. Four ship with the build (high-contrast, washi, terminal, retro) and are listed first; the rest are the device's own, kept one file per skin at `<base>/skins/<name>.zip` — a zip holding `skin.yaml` and the skin's materials, which is the one shape a skin arrives in. The one that is on is a name in config.json (`config set skin`), never synced, because it points at this machine. A file in the directory that will not read is listed as unreadable rather than left out. The four shipped names are reserved: no file may be taken in under one, and none of them is on the device to remove.",
            "args": [], "flags": [{ "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo skin list --json"] }),
        json!({ "name": "skin add", "summary": "Takes a skin file in, to `<base>/skins/<name>.zip` under the name the file gives itself, byte for byte. One shape: a zip holding `skin.yaml` at its root beside the materials it names — a bare document is turned away, and the message says to pack it. It is read, checked and measured first. A zip is turned away whole where an entry's name points outside it (`../`, or an absolute path), where one file unpacks past 8MB, where the whole unpacks past 32MB — counted while unpacking, so an index that lies does not get past — and where there is no `skin.yaml` in it. One name is one file. A key this build does not know, a name it keeps to itself, a value that is not text, and a value that is text but not a shape a declaration can hold (empty, over 512 characters, or carrying `;` `{` `}` `<` `>` `\\`, a newline, a comment delimiter or `url(`) are dropped with a warning naming each and why; a later `skin_v`, a `themes` line that disagrees with the tables, a name that cannot be a filename, and an embedded font with no `license_text` turn the whole file away. A skin may carry `titles:`, the name per language (language code to the name), which the window shows a reader in their own language and falls back from to `title`; the CLI stays on `title`. A skin may carry one font (`font_file`: family/format/file/license/license_text), the face being a file in the zip and `file` its name there; it is taken only as woff2 under 2MB, and anything else about it — a name nothing in the zip answers to included — drops the font with a warning and keeps the colours. What sets a screen in that face is a stack naming the family: a skin carrying one that neither `font` nor `font-mono` names keeps it, and is warned that nothing is set in it. A skin may lay a picture behind a surface (`backgrounds:`, one entry per place, keyed by the colour token drawn there — `c-bg`, `c-pane-bg`, `c-sunken`, `c-surface`): `file` is a file in the zip, `fit` takes contain/cover/tile (cover where nothing is written) and `at` takes one of the nine spots (center by default). A skin may hand over its own drawing for an icon (`icons:`, one entry per icon, keyed by the name it is drawn under — the 51 the window draws, which `skin template` writes out in full): the value is the file, one per name, and a name the document leaves out keeps this build's own drawing. The line width is not an author's to move. A place, an icon name, or a word this build does not have is dropped with a warning naming it, and so is a filename a skin cannot hold (one reaching out of the zip, or carrying what an address is cut at). What a file is is read off its bytes rather than off its name: png, jpeg, webp and svg are drawn as a background, and an icon takes the same less jpeg, being laid as a mask and needing transparency. The four ladders (text, spacing, radius, shadow) are not written out name by name — a skin sets `fs-scale` / `s-scale` / `r-scale` / `shadow-scale` and amenbo multiplies its own values. The ceiling is 1.30 on the first three (measured); the floor is 0.85 on the type and the spacing, and 0 on the radius and the shadow, which have no size that stops them working. A number outside that is brought inside and both values reported; only text that is no number is dropped. The frame pair is held to what this build draws: `border-style` takes solid/double/none, `border-w` is put back inside 0-4px, and a `double` frame is drawn at 3px whatever was asked for (the widths that split are not one range). `font-smooth` is held the same way and takes antialiased/auto/none; a word outside those is dropped. A ground with a picture behind it is not measured at all and is reported as covered: what a ratio would be saying is that a colour is under the text, and what is under it is a picture. A colour pairing under WCAG AA is reported with its number and does NOT stop the import — an author has to be able to try work in progress. A name already held is refused with both versions named, unless --yes replaces it; one of the four shipped names is refused outright, which --yes does not answer.",
            "args": [{ "name": "path", "required": true, "help": "the zip to take in" }],
            "flags": [{ "name": "--yes/-y", "help": "replace the skin already held under that name" }, { "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo skin add ./mine.zip", "amenbo skin add ./mine.zip --yes"] }),
        json!({ "name": "skin use", "summary": "Puts one of the held skins on, or takes whatever is on off (`none`). One is worn at a time, so this is a choice rather than a switch. A name nothing is held under is refused rather than recorded. `none` is also the way back from a skin that left the window unreadable — it is typed outside the window, so it works whatever the screen is doing, and the window's own menu and `skin use high-contrast` are the other two ways back.",
            "args": [{ "name": "name", "required": true, "help": "the skin's name, or none" }],
            "flags": [{ "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo skin use washi", "amenbo skin use none"] }),
        json!({ "name": "skin rm", "summary": "Takes a skin off the device. If it was the one on, it is taken off too — the config naming a file that is gone would say the device is wearing something it has not got. Removing a name nothing is held under is not a failure; it says so and stops. One of the four names this build ships is refused: it is not a file on the device.",
            "args": [{ "name": "name", "required": true, "help": "the skin's name" }],
            "flags": [{ "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo skin rm mine"] }),
        json!({ "name": "skin validate", "summary": "Runs the reading, the check and the contrast measure over a file — the same zip `skin add` takes — without taking it in — the author's face of what `skin add` does, so the reason a file will be turned away is learned before it is handed to anybody. A ground a picture is laid over is named as covered rather than measured. Exits non-zero on a refusal; warnings and short pairings are reported and exit 0.",
            "args": [{ "name": "path", "required": true, "help": "the zip to read" }],
            "flags": [{ "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo skin validate ./mine.zip --json"] }),
        json!({ "name": "skin template", "summary": "Writes a skin to start from at the given path: a zip holding `skin.yaml` and nothing else, already valid. Both sides and every name a skin may set — the colours, the frame, the font stacks, the weights and widths, and the four ladder multipliers at what was asked for — each with a line saying what it is for. What only the author can fill in (author, version, license, homepage, `titles`, `font_file`) is written as a commented shape to copy. Taken from the skin that is on where there is one, so an author editing what they are looking at starts from those values, and filled in from this build for everything that skin left alone; the name it gives itself is not the one it was taken from, since a file calling itself what is already held would replace it on the way back in. A zip rather than a document on stdout, because a skin is a zip: the author unpacks it, edits, puts the materials it names beside the document and packs it back up. A path a file is already at is refused, and there is no flag that answers that.",
            "args": [{ "name": "path", "required": true, "help": "where to write the zip; it must not already be a file" }],
            "flags": [{ "name": "--json", "help": "machine-readable output (the path written, and its size)" }],
            "examples": ["amenbo skin template ./my-skin.zip"] }),
        json!({ "name": "skin write-out", "summary": "Writes a skin this device holds back out, as the file it arrived in and byte for byte — the materials the author packed with it ride along, which rebuilding the document from the values this build read could not do. The four that ship with the build are refused: they are not files on this device, and `skin template` is the road from one of those (put it on, then write it out). A name nothing is held under is refused rather than leaving an empty file behind, and so is a path a file is already at.",
            "args": [{ "name": "name", "required": true, "help": "the skin's name, as `skin list` shows it" }, { "name": "path", "required": true, "help": "where to write the file; it must not already be a file" }],
            "flags": [{ "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo skin write-out mine ./mine.zip"] }),
        json!({ "name": "restore", "summary": "Restores this device from a verified `.amenbo-backup` archive at the given path — a destructive replace of the database the archive carries (all-or-nothing stage-and-swap; the replaced truth source is set aside with a timestamp; an archive newer than this build is refused — update first). It is the one command that runs on a store this build cannot open, because it replaces the truth source instead of reading it — which is what makes the pre-migration backup a real way back from a store a newer Amenbo carried past this build (there is no downgrade). The snapshot is validated before anything is swapped in, so an unusable archive is refused without harm. The archive's attachment bytes (blobs) and its skins are placed additively — a blob the machine already holds, and a skin name it already has a file for, are left alone, and none are ever deleted. An archive written before the consolidation carries the pre-consolidation shape (a list of stores) and is refused whole by its layout version, before its manifest is even parsed, rather than partially applied: restore it with the build that wrote it. Destructive — confirms unless --yes.",
            "args": [{ "name": "path", "required": false, "help": "the .amenbo-backup archive to restore from (must exist and pass verification)" }],
            "flags": [{ "name": "--json", "help": "machine-readable output" }, { "name": "--yes/-y", "help": "skip confirmation" }],
            "examples": ["amenbo restore ./everything.amenbo-backup --yes"] }),
        json!({ "name": "hard-erase comment", "summary": "Physically erases one or more task comments from this store's truth source — deletes the read-model row outright — then VACUUMs so the bytes leave the file (unrecoverable). An ordinary delete removes the row — and `comment rm` deletes a comment posted by mistake — but the freed pages keep their bytes readable until something reclaims them, so this is the deliberate, gated exception: use it for content that must be GONE from the file. Identify comments by id; find ids with `comment list <task> --json`. Human-gated maintenance: takes a safety backup first (a `pre-erase-*.amenbo-backup` archive next to the store, which `restore` puts the store back from — only the newest is kept), confirms unless --yes, and is refused for the AI actor (a human must run it). The safety backup still holds the erased content — delete it after verifying.",
            "args": [{ "name": "ids", "required": true, "help": "task comment ref(s) to erase, AMB-TC-n" }],
            "flags": [{ "name": "--yes/-y", "help": "skip confirmation" }, { "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo hard-erase comment AMB-TC-<n> --yes"] }),
        json!({ "name": "hard-erase decision-comment", "summary": "Physically erases one or more decision comments — the same surgery `hard-erase comment` performs on the task side, on the other comment table. It is a separate command rather than a flag because the two tables number independently: a bare id belongs to whichever table the command names, and an erase that guessed would destroy the wrong row. The comment's row goes outright (a comment's number is not a conversational one, so nothing is left pointing at it) along with the bytes of any file attached to it, then a VACUUM takes the freed pages out of the file. Find ids with `decision comment-list <decision> --json`. Human-gated maintenance, on the same footing as the task side: a safety backup first, confirms unless --yes, and refused for the AI actor. The safety backup still holds the erased content — delete it after verifying.",
            "args": [{ "name": "ids", "required": true, "help": "decision comment ref(s) to erase, AMB-DC-n" }],
            "flags": [{ "name": "--yes/-y", "help": "skip confirmation" }, { "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo hard-erase decision-comment AMB-DC-<n> --yes"] }),
        json!({ "name": "hard-erase decision", "summary": "Redacts a settled decision's body: overwrites it with the given text in place (the prior body is physically replaced, not merely superseded), then VACUUMs — so one section can be removed while the decision keeps its number, links and other fields. The replacement body comes from --body, --body-file, or stdin. Destructive maintenance: takes a safety backup first (a `pre-erase-*.amenbo-backup` archive next to the store, which `restore` puts the store back from — only the newest is kept), confirms unless --yes, and is refused for the AI actor (a human must run it). The safety backup still holds the old body — delete it after verifying.",
            "args": [{ "name": "id", "required": true, "help": "decision reference (AMB-D-n)" }],
            "flags": [{ "name": "--body <text>", "help": "replacement body (Markdown); omit to use --body-file or stdin" }, { "name": "--body-file <path>", "help": "read the replacement body from this file instead of --body/stdin" }, { "name": "--yes/-y", "help": "skip confirmation" }, { "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo hard-erase decision AMB-D-<n> --body-file ./redacted.md --yes"] }),

        // Automations — the building side, in three layers (`AMB-D-949`): an automation places
        // library actions, an action holds steps, and one step is one terminal. Every part is named
        // by the id its `add` printed: they carry no conversational ref, being named by what they sit
        // in rather than by a number anybody types back. Nothing here refuses an unfinished
        // automation; the launch check does.
        cmd("automation add", "Creates an automation: a name and what it is for. What every step is told before its own prompt is not one of its fields — it is the same sentence on every automation, so Amenbo writes it at each launch. It starts with nothing placed on it and no entry, which is not refused here: the launch check is where an unfinished one is caught. Prints the id the place commands take.",
            json!([{ "name": "--name <str>", "help": "what this automation is called", "required": true },
                   { "name": "--notes <text>", "help": "what it is for, in Markdown (`-` reads stdin)" },
                   { "name": "--project <id|name>", "help": "which project (defaults to the bound one)" },
                   { "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo automation add --name \"Review and fix\" --notes \"Runs over the review queue\""])),
        cmd("automation update", "Changes an automation's name, notes, or whether it is archived. Only the fields given change.",
            json!([{ "name": "<id>", "help": "automation id", "required": true },
                   { "name": "--name <str>", "help": "rename it" },
                   { "name": "--notes <text>", "help": "what it is for (`-` reads stdin)" },
                   { "name": "--archived <true|false>", "help": "whether it is archived" }]),
            json!(["amenbo automation update 3 --notes \"Runs over the review queue\"", "amenbo automation update 3 --archived true"])),
        cmd("automation rm", "Deletes an automation with every placement, edge and wire built onto it, and the answers written on each placement. The library actions those placements stood on are left where they are. It rides one transaction, so there is no half-deleted picture. Confirms unless --yes.",
            json!([{ "name": "<id>", "help": "automation id", "required": true },
                   { "name": "--yes/-y", "help": "skip the confirmation" }]),
            json!(["amenbo automation rm 3 --yes"])),
        cmd("automation list", "The automations of one project, in the order they were placed in: the id, the name, how many actions are placed on it, and whether it is archived. Archived ones are listed too — archiving keeps one out of the way rather than removing it, and a listing that hid them would leave an id nothing explains.",
            json!([{ "name": "--project <name|id>", "help": "project (defaults to the bound project)" },
                   { "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo automation list --json"])),
        cmd("automation show", "One automation in full, which is the only way to read a definition back from a terminal. It prints the automation's own notes, then every placement with what it runs under — the action standing there, what it takes in, what it is answered with, each way out with what leaving through it hands on, what happens after each one is taken, and what is wired from it to a later placement. What is inside an action is `automation action-show`'s. An edge hanging on a way out the action no longer declares is written out as such, that being why a picture stops walking.",
            json!([{ "name": "<id>", "help": "automation id", "required": true },
                   { "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo automation show 3", "amenbo automation show 3 --json"])),
        cmd("automation entry-set", "Names the placement a run starts at, or clears it with --clear. From it the edges are walked, so where every other placement sits in the picture falls out of this one answer. An automation with no entry saves; launching one is refused at the launch check.",
            json!([{ "name": "<id>", "help": "automation id", "required": true },
                   { "name": "--placement <id>", "help": "the placement to start at" },
                   { "name": "--clear", "help": "leave it starting nowhere" }]),
            json!(["amenbo automation entry-set 3 --placement 11", "amenbo automation entry-set 3 --clear"])),
        cmd("automation place-add", "Puts a library action on an automation. What stands on a picture is a placement of an action, never a prompt of its own: the same action placed twice gives two spots that share the prompt and answer their settings apart. The action has to be within reach — this project's library or the device's. Prints the id the edge, wire and cfg-set commands take.",
            json!([{ "name": "<automation>", "help": "automation id", "required": true },
                   { "name": "--action <id>", "help": "the library action to place", "required": true },
                   { "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo automation place-add 3 --action 7 --json"])),
        cmd("automation place-rm", "Takes a placement off its automation, with the answers written on it and every edge and wire naming it. The action itself stays in the library. Confirms unless --yes.",
            json!([{ "name": "<id>", "help": "placement id", "required": true },
                   { "name": "--yes/-y", "help": "skip the confirmation" }]),
            json!(["amenbo automation place-rm 11 --yes"])),

        cmd("automation start", "Starts an automation: checks it, copies what is placed on it into the run, and starts it. While the run is running or paused it holds its definition: an edit, a delete, an archive or a scope move of the automation or of any action placed on it is refused as conflict, naming the run — stop it with `automation stop`, or let it finish, to edit again. Nothing caps how many runs may be going at once, so a start never waits. It takes nothing else — which tasks a step works on and which folder it runs in are the automation's own answers, given while it was built. An unfinished one is refused as not_ready_automation (a code of its own, apart from a reservation's not_ready), naming every reason: a way out with nothing after it, a required input nothing reaches — both asked of the automation's picture and of the picture inside every action placed on it — a required setting nobody answered, an agent this machine cannot start, nothing placed on it, no entry, an entry that takes no task, an action placed on it with no step to open. A model the agent does not have here is NOT among them from a terminal: knowing costs a login shell and that provider starting up, and the answers the app keeps are in the app's own process — so the GUI's launch place names it and a step started from here meets it in the pane instead. An archived one is refused as invalid_automation_archived — bring it back with `automation update <id> --archived false`. Prints the run id that pause / resume / stop take.",
            json!([{ "name": "<id>", "help": "automation id", "required": true },
                   { "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo automation start 3 --actor ai"])),
        cmd("automation pause", "Asks a run to pause. A step under way finishes first and the run pauses at the end of it, keeping the task it is working. `resume` picks it up from the same way out.",
            json!([{ "name": "<run>", "help": "run id", "required": true },
                   { "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo automation pause 7 --actor ai"])),
        cmd("automation resume", "Picks a paused run up again, from the way out the step before it left through.",
            json!([{ "name": "<run>", "help": "run id", "required": true },
                   { "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo automation resume 7 --actor ai"])),
        cmd("automation stop", "Stops a run, which ends canceled: hands the task it was working back to todo, and leaves a comment on that task saying how far it got — unless the task is closed (done or rejected), which gets no comment. Unlike pause it does not wait for the step under way — use it where a pause will not land.",
            json!([{ "name": "<run>", "help": "run id", "required": true },
                   { "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo automation stop 7 --actor ai"])),

        cmd("automation step-take", "Takes the task this stretch of the run is about: reserves it and declares it in one act, so no window exists where it is held by nobody the store can name. It succeeds only from todo, and is refused with already_reserved otherwise — the same compare-and-swap every reservation goes through. Typed inside the terminal a run opened for a step, which is where the step it speaks for is read from; outside one it is refused.",
            json!([{ "name": "<task>", "help": "the task to take (AMB-T-n)", "required": true },
                   { "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo automation step-take AMB-T-812 --actor ai"])),
        cmd("automation step-out", "Puts down one thing this step produced, on the output its id names — the step's text lists each output with its id, under the way out it belongs to. Written <id>=<value>, or <id> --file <path> for a file, which is attached to this step execution first. WHAT THE OUTPUT WAS DECLARED TO CARRY DECIDES HOW THE WORDS ARE READ: on a value port they are the answer, and on a task_make port they name a task this step raised along the way, which is written as the task. The task the run is about does not come this way and is refused here, naming `automation step-take` — reserving it and declaring it are one act. An id the step declares no output of is refused, naming the ones it does, and so is a payload of the wrong kind. Putting the same output down twice replaces the first. Two ways out may each declare an output of one name; they are two ids, and only the one on the way out the step leaves by is handed on.",
            json!([{ "name": "<id>=<value>", "help": "what is handed on, or just <id> beside --file", "required": true },
                   { "name": "--file <path>", "help": "a file to hand on, instead of a value" },
                   { "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo automation step-out 31=\"nothing to fix\" --actor ai",
                   "amenbo automation step-out 32 --file ./report.md --actor ai",
                   "amenbo automation step-out 33=AMB-T-9001 --actor ai"])),
        cmd("automation step-done", "Says this step is finished: which way out it took, and what it did. The run reads the way out to decide what happens next — another step, the run closing, or the run stopping for a person. --exit takes the way out's id — the step's text lists each one with it, the error way out included — and left out it is the unnamed way out. An id the step does not declare is refused before anything is written, naming the ones it does, and the step stays running. A missing required output is refused, named, before anything is written. --out repeats for anything not already put down with `automation step-out`. Prints what the run does next.",
            json!([{ "name": "--report <text>", "help": "what this step did (`-` reads stdin)", "required": true },
                   { "name": "--exit <id>", "help": "the way out taken, by the id the step's text lists for it" },
                   { "name": "--out <id>=<value>", "help": "one more thing produced — repeat for several" },
                   { "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo automation step-done --report - --exit 21 --actor ai"])),

        cmd("automation action-add", "Adds an action to the library — a unit worth using twice. It is born empty: `automation step-add` writes the steps it holds, and `automation action-entry-set` names the one a placement of it opens first. It names no agent and no model, those being chosen where it is placed (`automation agent-set`), and it carries the unnamed way out and the error one (`*`) from birth. --note is what it is for, drawn where it is built and never carried into a launch. --global puts it in the device's library, which every project on this machine reaches; that shelf is a human's to write, and an AI bound to a project is turned away from it.",
            json!([{ "name": "--name <str>", "help": "what this action is called", "required": true },
                   { "name": "--note <text>", "help": "what it is for, in Markdown (`-` reads stdin) — shown where it is built, never carried into a launch" },
                   { "name": "--global", "help": "put it in the device's library rather than this project's (human only)" },
                   { "name": "--project <id|name>", "help": "which project's library (defaults to the bound one)" }]),
            json!(["amenbo automation action-add --name \"Write the tests\""])),
        cmd("automation action-list", "The library a project reaches: the device's shelf first, then the project's own, each row with the id, the name, whether it is the device's, how many steps it holds and how many automations place it. The two answer as one list because an automation here may place either. --global narrows it to the device's shelf rather than opening a second place to look. The count is of automations, not placements: one action placed twice on one automation is one automation whose runs change when the prompt is rewritten.",
            json!([{ "name": "--project <name|id>", "help": "project (defaults to the bound project)" },
                   { "name": "--global", "help": "the device's library alone" },
                   { "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo automation action-list --json", "amenbo automation action-list --global"])),
        cmd("automation action-show", "One library action in full: which shelf holds it, what it is for, how many automations place it, what it declares to every placement — the ways out, what each hands on, what it takes in, and the settings a placement has to answer — and then the picture inside it, every step with its whole prompt, its own ways out and inputs, and what runs after what. Prompts are printed whole: a definition is read back to check what was built, and a snippet would send the reader to a screen to finish the sentence. An action's settings rows are the declaration alone; the answer written while building sits on each placement, and `automation show` is where the two are read together.",
            json!([{ "name": "<id>", "help": "action id", "required": true },
                   { "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo automation action-show 7"])),
        cmd("automation action-update", "Renames a library action, or rewrites what it is for. Only the fields given change. The prompt is its step's, and `automation step-update` is where that is rewritten.",
            json!([{ "name": "<id>", "help": "action id", "required": true },
                   { "name": "--name <str>", "help": "rename it" },
                   { "name": "--note <text>", "help": "rewrite what it is for (`-` reads stdin)" }]),
            json!(["amenbo automation action-update 7 --name \"Review\""])),
        cmd("automation action-entry-set", "Names the step a placement of this action opens first, or clears it with --clear. From it the action's own edges are walked. An action with no entry saves; a picture that places it is refused at the launch check.",
            json!([{ "name": "<id>", "help": "action id", "required": true },
                   { "name": "--step <id>", "help": "the step to open first" },
                   { "name": "--clear", "help": "leave it opening nothing" }]),
            json!(["amenbo automation action-entry-set 7 --step 11"])),
        cmd("automation action-scope-set", "Moves a library action to the device's library (--global) or a project's, and puts it at the bottom there. The steps, declarations and pictures inside it go with it, and the placements on it stay where they are. Out to the device's library is never refused. Into a project is refused while an automation of another project places it: the refusal names each such automation and its project, and nothing is copied to get round it — take the action off those pictures first. Both libraries are written, so an AI bound to a project is turned away either way.",
            json!([{ "name": "<id>", "help": "action id", "required": true },
                   { "name": "--global", "help": "move it to the device's library (human only)" },
                   { "name": "--project <id|name>", "help": "which project's library (defaults to the bound one)" }]),
            json!(["amenbo automation action-scope-set 7 --global", "amenbo automation action-scope-set 7 --project amenbo"])),
        cmd("automation action-rm", "Deletes a library action with the steps inside it, the picture they are drawn into, and the ways out, ports and settings it declared. Refused while it is placed on a picture — the placement would be left standing on nothing. Confirms unless --yes.",
            json!([{ "name": "<id>", "help": "action id", "required": true },
                   { "name": "--yes/-y", "help": "skip the confirmation" }]),
            json!(["amenbo automation action-rm 7 --yes"])),

        cmd("automation step-add", "Adds a step to a library action — one step is one terminal, and it carries its own prompt. Who carries it out is not the step's: it is chosen where the action is placed (`automation agent-set`), so the same action can be run by different agents on two automations. It is born with the unnamed way out and the error one (`*`). --work-dir names the setting or the input the working folder is taken from: a name, not a path, and the name is one the action declares. Prints the id the action's own edge and wire commands take.",
            json!([{ "name": "<action>", "help": "action id", "required": true },
                   { "name": "--name <str>", "help": "what this step is called", "required": true },
                   { "name": "--prompt <text>", "help": "the prompt this step runs on (`-` reads stdin)", "required": true },
                   { "name": "--interactive", "help": "let it wait for a person" },
                   { "name": "--work-dir <name>", "help": "the setting or input the working folder is taken from" },
                   { "name": "--report-to-task", "help": "also land its report as a comment on the task, unless the task is closed by then" },
                   { "name": "--no-history", "help": "do not hand it the run's story so far" }]),
            json!(["amenbo automation step-add 7 --name \"Review\" --prompt -",
                   "amenbo automation step-add 7 --name \"Fix\" --prompt - --work-dir folder"])),
        cmd("automation step-update", "Changes a step. Only the fields given change. Rewriting the prompt reaches every placement of the action holding it, which is what the library is for — and is refused while a run placing that action is running or paused.",
            json!([{ "name": "<id>", "help": "step id", "required": true },
                   { "name": "--name <str>", "help": "rename it" },
                   { "name": "--prompt <text>", "help": "rewrite the prompt (`-` reads stdin)" },
                   { "name": "--interactive <true|false>", "help": "whether it may wait for a person" },
                   { "name": "--work-dir <name> / --clear-work-dir", "help": "where the working folder is taken from" },
                   { "name": "--report-to-task <true|false>", "help": "whether its report lands as a comment on the task, unless the task is closed by then" },
                   { "name": "--history <true|false>", "help": "whether it is handed the run's story so far" }]),
            json!(["amenbo automation step-update 11 --interactive true"])),
        cmd("automation step-rm", "Deletes a step with its declarations, and every edge and wire of its action naming it. Deleting the one the action opens first clears that too. Confirms unless --yes.",
            json!([{ "name": "<id>", "help": "step id", "required": true },
                   { "name": "--yes/-y", "help": "skip the confirmation" }]),
            json!(["amenbo automation step-rm 11 --yes"])),

        cmd("automation exit-add", "Declares a way out of a step or a library action — the whole condition the next box is chosen by. An action's are what a placement of it is left by; a step's are what the picture inside that action is drawn with. Both are born carrying the unnamed way out and the error one (`*`), so this is for the second and every one after it, and `*` is refused as a name.",
            json!([{ "name": "--step <id>", "help": "the step that declares it (one carrying its own prompt)" },
                   { "name": "--action <id>", "help": "the library action that declares it" },
                   { "name": "--name <str>", "help": "what this way out is called", "required": true }]),
            json!(["amenbo automation exit-add --step 11 --name \"something to fix\"",
                   "amenbo automation exit-add --action 7 --name approved"])),
        cmd("automation exit-rename", "Renames a way out, or makes it the unnamed one with --clear. Edges and wires key a way out by its id, so everything on it stays on it. The error way out's name is fixed and refused at both ends.",
            json!([{ "name": "<id>", "help": "way out id", "required": true },
                   { "name": "--name <str>", "help": "the new name" },
                   { "name": "--clear", "help": "make it the unnamed way out" }]),
            json!(["amenbo automation exit-rename 21 --name \"needs work\""])),
        cmd("automation exit-rm", "Deletes a way out with the outputs declared on it, and the edges and wires keyed to it. The error way out is refused — every step and every action carries one. Confirms unless --yes.",
            json!([{ "name": "<id>", "help": "way out id", "required": true },
                   { "name": "--yes/-y", "help": "skip the confirmation" }]),
            json!(["amenbo automation exit-rm 21 --yes"])),

        cmd("automation port-add", "Declares a port. The direction is not asked for — it falls out of what the port hangs off: --step and --action declare what is taken in, --exit declares what that way out hands on. Neither is sayable on the other's owner, which is what lets a review step hand on a file only when it left through \"something to fix\". --kind says what it carries: value, file, task_take (the task the run is working) or task_make (a task the step created).",
            json!([{ "name": "--step <id>", "help": "the step that takes it in" },
                   { "name": "--action <id>", "help": "the library action that takes it in" },
                   { "name": "--exit <id>", "help": "the way out that hands it on" },
                   { "name": "--name <str>", "help": "what this port is called", "required": true },
                   { "name": "--kind <value|file|task_take|task_make>", "help": "what it carries", "required": true },
                   { "name": "--required", "help": "refuse to run the step without it" }]),
            json!(["amenbo automation port-add --exit 21 --name report --kind file",
                   "amenbo automation port-add --step 12 --name report --kind file --required"])),
        cmd("automation port-update", "Changes a port's name, what it carries, or whether it is required. Only the fields given change. Renaming parts every wire that named the old name: a wire names the ports at its ends.",
            json!([{ "name": "<id>", "help": "port id", "required": true },
                   { "name": "--name <str>", "help": "rename it" },
                   { "name": "--kind <value|file|task_take|task_make>", "help": "what it carries" },
                   { "name": "--required <true|false>", "help": "whether the step is refused without it" }]),
            json!(["amenbo automation port-update 33 --required false"])),
        cmd("automation port-rm", "Deletes a port. The wires that named it are left where they are, parted. Confirms unless --yes.",
            json!([{ "name": "<id>", "help": "port id", "required": true },
                   { "name": "--yes/-y", "help": "skip the confirmation" }]),
            json!(["amenbo automation port-rm 33 --yes"])),

        cmd("automation cfg-add", "Declares a setting on a library action: its name, what kind of answer it takes, and whether it has to be answered. --options is the choice list, as a JSON array, and belongs to --kind choice alone. The row is the declaration by itself; the answer is written on each placement of the action (`automation cfg-set`). A step declares none — it reaches one by the name the action gave it.",
            json!([{ "name": "--action <id>", "help": "the library action that declares it", "required": true },
                   { "name": "--name <str>", "help": "what this setting is called", "required": true },
                   { "name": "--kind <taskfilter|folder|choice|number|text>", "help": "what kind of answer it takes", "required": true },
                   { "name": "--required", "help": "refuse to run the step until it is answered" },
                   { "name": "--options <json>", "help": "the choices, as a JSON array (--kind choice only)" }]),
            json!(["amenbo automation cfg-add --action 7 --name queue --kind taskfilter --required",
                   "amenbo automation cfg-add --action 7 --name depth --kind choice --options '[\"quick\",\"deep\"]'"])),
        cmd("automation cfg-update", "Changes a setting's declaration — its name, kind, whether it is required, or its choice list. Only the fields given change. The answer is `automation cfg-set`, not this.",
            json!([{ "name": "<id>", "help": "setting id", "required": true },
                   { "name": "--name <str>", "help": "rename it" },
                   { "name": "--kind <taskfilter|folder|choice|number|text>", "help": "what kind of answer it takes" },
                   { "name": "--required <true|false>", "help": "whether the step is refused until it is answered" },
                   { "name": "--options <json> / --clear-options", "help": "the choice list (--kind choice only)" }]),
            json!(["amenbo automation cfg-update 41 --required true"])),
        cmd("automation cfg-set", "Answers a setting on one placement, by the name the action declared it under. The placement takes a row of its own under that name, so one action placed twice is answered twice and apart. --clear leaves it unanswered. THE ANSWER IS WRITTEN IN THE SHAPE ITS KIND TAKES, NEVER AS ONE FILTER STRING: --folder / --choice / --number / --text each take one value, and a taskfilter is built from --status / --priority / --assignee / --dim / --ready / --done / --due — the same option twice is any-of, two different options are both, which is the reading filterGrammar gives; --sort names the order its tasks are taken in, with the keys `task list --sort` takes (left out, highest priority first), and the step is handed the filter and the order as one `task list --filter … --sort=…`. The expression that builds is parsed before the answer is written, so a value nothing accepts is refused here rather than at the launch of a run. Nothing reads the declaration on the way through (no command lists one), so an answer in the wrong shape for its kind is caught when the run reads it.",
            json!([{ "name": "<placement>", "help": "placement id", "required": true },
                   { "name": "--name <str>", "help": "the setting's name, as declared", "required": true },
                   { "name": "--folder <path> / --choice <v> / --number <n> / --text <s>", "help": "the answer, for those four kinds" },
                   { "name": "--status / --priority / --assignee / --dim / --ready / --done / --due", "help": "the parts of a taskfilter (repeat one for any-of)" },
                   { "name": "--sort <key>", "help": "the order a taskfilter's tasks are taken in — a `task list --sort` key (default priority)" },
                   { "name": "--clear", "help": "leave it unanswered" }]),
            json!(["amenbo automation cfg-set 12 --name queue --status todo --status in_progress --priority high --sort due",
                   "amenbo automation cfg-set 12 --name depth --choice deep"])),
        cmd("automation agent-set", "Chooses who carries one step out at one placement — the agent, and the model where one is named; left out, the agent's own default stands. The step is one inside the action standing on the placement, and each placement of the same action chooses apart, step by step. A step a run could open with nobody chosen for it is refused at launch. --clear leaves nobody chosen. `automation show` lists every step of each placement with who carries it out.",
            json!([{ "name": "<placement>", "help": "placement id", "required": true },
                   { "name": "--step <id>", "help": "the step, inside the placed action", "required": true },
                   { "name": "--agent <str>", "help": "who is asked to carry it out (required unless --clear)" },
                   { "name": "--model <str>", "help": "which model; left out, the agent's own default stands" },
                   { "name": "--clear", "help": "leave nobody chosen for it" }]),
            json!(["amenbo automation agent-set 12 --step 11 --agent claude --model opus",
                   "amenbo automation agent-set 12 --step 11 --clear"])),
        cmd("automation cfg-rm", "Deletes one setting row — an action's declaration, or one placement's answer to it. Confirms unless --yes.",
            json!([{ "name": "<id>", "help": "setting id", "required": true },
                   { "name": "--yes/-y", "help": "skip the confirmation" }]),
            json!(["amenbo automation cfg-rm 41 --yes"])),

        cmd("automation edge-add", "Says what happens after one box leaves through one way out: go on to another box (--to), leave the action by one of the ways out it declares (--exit-to, inside an action only — bare for its unnamed way out), close the run (--done), or stop the run and call a person (--halt). Both pictures are drawn with this one verb: --in-action draws the line inside a library action, between its steps, and left out it is drawn on an automation, between its placements. The edge carries no condition of its own — the way out is the condition — and one way out says one thing, so a second edge on the same one is refused. --from is written as one token, <box>:<way out>: `4:` is the unnamed way out and `4:*` the error one. --max-times caps how often a --to edge may be taken FOR ONE TASK, counted afresh at the next task; left out it is 10, which guards a loop that never converges without stopping a review that goes round three times. --no-max takes the cap off. A --exit-to, --done or --halt edge is taken once and carries no cap at all.",
            json!([{ "name": "--from <box>:<way out>", "help": "where it leaves from (`4:` is the unnamed way out, `4:*` the error one)", "required": true },
                   { "name": "--in-action", "help": "draw it inside a library action, between its steps" },
                   { "name": "--to <id>", "help": "go on to this box" },
                   { "name": "--exit-to [name]", "help": "leave the action by the way out it declares under this name — bare, its unnamed one (--in-action only)" },
                   { "name": "--done", "help": "close the run" },
                   { "name": "--halt", "help": "end the run failed (halted) and call a person: its task goes back to todo, assigned to the human, with the step's report on it" },
                   { "name": "--max-times <n>", "help": "how often this edge may be taken for one task (default 10)" },
                   { "name": "--no-max", "help": "let it be taken as often as the run reaches it" }]),
            json!(["amenbo automation edge-add --from 11:approved --done",
                   "amenbo automation edge-add --from \"11:something to fix\" --to 12 --max-times 3",
                   "amenbo automation edge-add --from 12: --to 11",
                   "amenbo automation edge-add --in-action --from 21:approved --exit-to"])),
        cmd("automation edge-update", "Changes where an edge goes, or how often it may be taken. Only the fields given change. Which picture it is on is read off the row, so it is not asked for again. The way out it hangs on is not among them either — that pair is what the edge is, so pointing it at another way out is a delete and an add.",
            json!([{ "name": "<id>", "help": "edge id", "required": true },
                   { "name": "--to <id> / --exit-to [name] / --done / --halt", "help": "what happens after that way out" },
                   { "name": "--max-times <n> / --no-max", "help": "how often it may be taken for one task" }]),
            json!(["amenbo automation edge-update 51 --no-max"])),
        cmd("automation edge-rm", "Deletes an edge — that way out then says nothing about what happens after it. Confirms unless --yes.",
            json!([{ "name": "<id>", "help": "edge id", "required": true },
                   { "name": "--yes/-y", "help": "skip the confirmation" }]),
            json!(["amenbo automation edge-rm 51 --yes"])),

        cmd("automation wire-add", "Joins what one way out hands on to what a later box takes in. Both ends are named rather than keyed: one action placed twice on an automation gives two placements whose ports carry the same names, so only the box, the way out and the port name together say which is meant. --from is the same <box>:<way out> token the edge takes, and --in-action picks the same picture it picks there. The two ends are refused unless they carry the same kind of thing. Inside an action (--in-action), either end may be `0`, the action itself: --from 0 hands an input the action declares to a step, and --to 0 fills an output on the way out of the action that --from's way out returns to — so that step's --exit-to edge is drawn first.",
            json!([{ "name": "--from <box>:<way out>", "help": "where it comes from (`4:` is the unnamed way out; `0` is the action itself, with --in-action)", "required": true },
                   { "name": "--in-action", "help": "draw it inside a library action, between its steps" },
                   { "name": "--from-port <name>", "help": "the output's name on that way out", "required": true },
                   { "name": "--to <id>", "help": "the box that takes it in (`0` is the action itself, with --in-action)", "required": true },
                   { "name": "--to-port <name>", "help": "the input's name on that box, or the output's name on the action's way out", "required": true }]),
            json!(["amenbo automation wire-add --from \"11:something to fix\" --from-port report --to 12 --to-port report",
                   "amenbo automation wire-add --in-action --from 0 --from-port task --to 21 --to-port task",
                   "amenbo automation wire-add --in-action --from 21:approved --from-port report --to 0 --to-port report"])),
        cmd("automation wire-rm", "Deletes a wire — the later box is then handed nothing under that name. Confirms unless --yes.",
            json!([{ "name": "<id>", "help": "wire id", "required": true },
                   { "name": "--yes/-y", "help": "skip the confirmation" }]),
            json!(["amenbo automation wire-rm 61 --yes"])),

        cmd("automation run-list", "The runs that worked one task (--task), or the runs one automation has behind it (--automation), newest first. THERE IS NO LISTING OF EVERY RUN and no word search over one: a run is reached from a record already in hand, because what is asked of it is how this task was handled or what this automation has done. A run that came back to the same task twice is listed once.",
            json!([{ "name": "--task <id>", "help": "the task a run worked (AMB-T-n)" },
                   { "name": "--automation <id>", "help": "the automation the runs came from" },
                   { "name": "--limit <n>", "help": "max count (newest first)" },
                   { "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo automation run-list --task AMB-T-123", "amenbo automation run-list --automation 3 --limit 5"])),
        cmd("automation run-show", "One run in full: every step it ran in the order it ran them, grouped by the task each stretch of the run was about — which way out each step left through, how long it stood, what it was handed and what it handed on, and the whole of what it reported. The report is not cut short: a run read back months later is read for exactly that. A step that went looking for a task and found none belongs to no stretch and is written out on its own.",
            json!([{ "name": "<id>", "help": "run id", "required": true },
                   { "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo automation run-show 7"])),
    ])
}

/// The entry-point spec: **how to work here in full, the commands as an index**. The only difference
/// from [`build`] is `commands` — the exhaustive list of names, and nothing else. The entry point is
/// always read, so a fat one costs tens of thousands of tokens at the head of every session, however
/// small the task. The index's whole job is that no command goes unknown; what each one is for is
/// already mapped intent-first by `capabilities`, and the rest — summary, flags, args, examples —
/// comes back whole from the detail side (`agent --command <name>`, `<cmd> --help`, `agent --full`).
/// Nothing becomes unreachable; only the route to it changes.
pub fn build_index() -> Value {
    index(crate::session::surface().is_some())
}

/// [`build_index`], with the one question it asks of the environment answered by the caller: is this
/// process running inside a pane of the talk window? Separate so the size the entry is recorded at can
/// be measured both ways without setting a process-wide variable in a suite that is not one.
fn index(in_a_pane: bool) -> Value {
    let mut spec = build();
    let index: Vec<Value> = spec["commands"]
        .as_array()
        .map(|cmds| cmds.iter().map(|c| c["name"].clone()).collect())
        .unwrap_or_default();
    if let Value::Object(map) = &mut spec {
        map.insert("commands".to_string(), Value::Array(index));
        // Unless we say it is an index, the AI reads it as the whole spec and hallucinates flags.
        // Say how to pull the rest, right here.
        // Added after the retarget, so this one words the CLI's name itself — `<cmd>` is a
        // placeholder rather than a command name, which is not something prose can be read for.
        let cli = Paths::command_name();
        map.insert(
            "commandDetail".to_string(),
            json!(format!("`commands` is an index: every command's name, and nothing more. For what one is for, read `capabilities` — it maps intent to command names. To use one, pull its full spec — summary, flags, args, examples — with `{cli} agent --command <name>` (or `{cli} <cmd> --help`); never guess a flag from the name. `{cli} agent --full` prints every command's full spec at once, but you rarely need it — pull the two or three you are about to run. Holding a word instead of a name is not an index lookup: `{cli} search <word> …` is the one command that reads words.")),
        );
        // The second vocabulary, named only where it exists (`AMB-D-749`). Outside a pane the words it
        // points at all fail, so teaching them there would be teaching a road that is closed — and the
        // reader could not tell that from a road they had simply not tried yet.
        //
        // **What the layer is stays here, though it went from the sentence a pane is opened with.**
        // There (`crate::agents::pane_instruction`) it was said twice over, since `talk --json` is the
        // next thing read and describes itself at length. Here it is doing something that canon
        // cannot: saying why this document does not carry the vocabulary itself, which is a question
        // only a reader of this document has.
        if in_a_pane {
            // How many of them are owed is counted off the canon rather than written down twice: a word
            // that moves between `owed` and `offered` moves this sentence with it — and the vocabulary
            // has been down to one word before, so the sentence reads at one as well as at several.
            let owed = crate::session::spec()["owed"].as_array().map_or(0, |o| o.len());
            let owes = if owed == 1 {
                "one of its words is owed".to_string()
            } else {
                format!("{owed} of its words are owed")
            };
            map.insert(
                "talk".to_string(),
                json!(format!("You are running in a pane of Amenbo's talk window, and there is a second vocabulary here: what you say about **this session** — the pane on the person's screen. It writes to no store and exists in this terminal alone, which is why `agent` does not carry it. Read `{cli} talk --json` and follow it; {owes}, and the person sees only what you say. Do not go looking for work until the person speaks: step 1 is for when you have decided to, and an agent opened into an empty pane has not.")),
            );
        }
    }
    spec
}

/// **The entry point inside a step of an automation run** — what `agent --json` answers in a terminal
/// a run opened ([`crate::env::automation_step`]), in place of [`build_index`].
///
/// A step's session is started afresh for every step, and the folder's own instructions send the
/// agent to `agent --json` first. The whole entry is about working a mailbox, none of which applies
/// in a step, and an agent reads only its head — which never reached the `automation step-*` verbs a
/// step does need (`AMB-T-5385`). So this says the two things a step needs and nothing else: the text
/// it was started on is the whole of its work, and these are the commands it reaches, the three that
/// hand the work back in full. Both lists are read off [`Cmd::in_a_step`] (`AMB-D-968`), so a command
/// moved to the other side there is taught on the other side here.
/// `agent --command` and `agent --full` answer as they do anywhere.
pub fn build_step() -> Value {
    let cli = Paths::command_name();
    let on = |side: InAStep| Cmd::ALL.iter().filter(move |c| c.in_a_step() == side).map(|c| c.name());
    let hand_back: Vec<Value> = on(InAStep::HandsBack)
        .map(|name| command_spec(name).unwrap_or_else(|| json!({ "name": name })))
        .collect();
    let reaches: Vec<&str> = on(InAStep::Reaches).collect();
    let moves = on(InAStep::MovesTheTask).map(|name| format!("`{name}`")).collect::<Vec<String>>().join(", ");
    json!({
        "mode": "step",
        "version": VERSION,
        "schemaVersion": SCHEMA_VERSION,
        "step": format!("This terminal is a step of an automation run. The text it started you on is the whole of this session's work: do it, then hand it back with the commands below — `{cli} automation step-take` for the task the step is about (where it declares one), `step-out` for each thing it hands on, and `step-done` for the way out taken and the report owed whichever one it is. Take nothing from the mailbox and file no other work; opening the step after yours is the run's. Pass --actor ai on every command."),
        "commands": hand_back,
        "reaches": {
            "commands": reaches,
            "note": format!("The rest of what this terminal is for: reading where the step stands, writing on a timeline, and what the text you were started on has you do. Use no other command here. The ones that move a task's status or who it is assigned to ({moves}) are the run's: Amenbo moves the task's status, and if a person's judgement is needed, leave by that way out. `{cli} agent --command <name>` prints any command's full spec."),
        },
    })
}

/// One command's full spec (`name` / `summary` / `args` / `flags` / `examples`) — the detail side an
/// index row points at. `name` is the name the command is registered under in the agent spec,
/// compound names with a space (`task add`) included; [`command_names`] is the canonical list.
pub fn command_spec(name: &str) -> Option<Value> {
    build()["commands"]
        .as_array()?
        .iter()
        .find(|c| c["name"].as_str() == Some(name))
        .cloned()
}

/// Every command name registered in the agent JSON — what the "was this command ever registered?"
/// test compares against.
pub fn command_names() -> Vec<String> {
    build()["commands"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|c| c["name"].as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Discipline: one store, so `mode` is always the single `personal` shape.
    #[test]
    fn spec_is_single_personal_store() {
        assert_eq!(build()["mode"], "personal");
    }

    /// What every step owes, wherever it was written: an id nothing else in its run answers to,
    /// prose that is actually there, and a way of being reached — its place in a run (`n`, which must
    /// be its actual place, or a step inserted without renumbering leaves every cross-reference
    /// pointing at the wrong work) or a `trigger` that fires it, or both. A step declaring neither is
    /// unreachable: nothing arrives at it and nothing calls it. Returns the id, so the caller can
    /// hold the run's ids apart.
    ///
    /// What it no longer asks is whether the commands exist: they are [`Cmd`] variants now, so a step
    /// naming a command that is not in the table does not build (`every_command_a_step_names_exists`
    /// is what holds the table's other end against the registry).
    fn check_step<'a>(step: &'a Value, at: usize, run: &str) -> &'a str {
        let id = step["id"].as_str().unwrap_or("");
        assert!(!id.is_empty(), "a step of {run} has no id: {step}");
        assert!(step["step"].as_str().is_some_and(|s| !s.is_empty()), "{run}.{id} has no prose");
        assert!(step["commands"].is_array(), "{run}.{id} does not emit its commands as an array");
        let placed = step.get("n").is_some();
        if placed {
            assert_eq!(step["n"], json!(at), "{run}.{id} is numbered {} but sits at {at}", step["n"]);
        }
        match step.get("trigger") {
            Some(trigger) => {
                assert!(trigger.as_str().is_some_and(|s| !s.is_empty()), "{run}.{id} carries an empty trigger");
            }
            None => assert!(placed, "{run}.{id} declares neither a place nor a trigger — nothing reaches it"),
        }
        id
    }

    /// Discipline: the hot-path backbone is a run. Every step is addressable, every one carries the
    /// number the cross-references elsewhere in the spec name it by, and the entry points are the
    /// only ones that declare a `trigger` — a step that is simply the next one must not, or the run
    /// can be entered in the middle.
    #[test]
    fn agent_cycle_steps_are_addressable() {
        let spec = build();
        assert!(
            spec["agentCycle"]["description"].as_str().is_some_and(|s| !s.is_empty()),
            "the backbone needs the standing description that says how to read the steps"
        );
        let steps = spec["agentCycle"]["steps"].as_array().expect("agentCycle.steps is an array");
        assert!(steps.len() >= 5, "the backbone lost steps: only {} left", steps.len());

        let mut ids = HashSet::new();
        for (at, step) in steps.iter().enumerate() {
            let id = check_step(step, at, "agentCycle");
            assert!(ids.insert(id), "two steps of the backbone answer to the id {id:?}");
            assert!(step.get("n").is_some(), "agentCycle.{id} has no place in the run");
            assert!(step.get("kind").is_none(), "the backbone has one bucket, so a step needs no kind: {step}");
        }
        assert!(
            steps.iter().any(|s| s.get("trigger").is_some()),
            "no step declares a trigger, so nothing says where the run is entered"
        );
    }

    /// Discipline: every cold-path cycle is addressable, and its items are the same steps the hot
    /// path is written in. Each item stays self-describing if it is ever flattened or filtered out of
    /// its bucket (`kind`), a `backbone` item carries its place and no trigger, and an `optional`
    /// item is the other way round — self-gated, so a trigger and no place to arrive at.
    #[test]
    fn cycles_are_addressable() {
        let spec = build();
        let cycles = spec["cycles"].as_object().expect("cycles is an object");
        let mut cycle_ids = HashSet::new();
        let mut item_count = 0;
        for cycle in CYCLES {
            let name = cycle.id.key();
            assert!(cycle_ids.insert(name), "two cycles answer to the id {name:?}");
            let emitted = cycles.get(name).unwrap_or_else(|| panic!("cycle {name} is not emitted"));
            assert!(emitted["when"].as_str().is_some_and(|s| !s.is_empty()), "cycle {name} needs a `when`");

            let mut ids = HashSet::new();
            for (bucket, expect_trigger) in [("backbone", false), ("optional", true)] {
                let items = emitted[bucket].as_array().unwrap_or_else(|| panic!("cycle {name}.{bucket} is an array"));
                for (at, item) in items.iter().enumerate() {
                    item_count += 1;
                    let id = check_step(item, at, name);
                    assert!(ids.insert(id), "two items of {name} answer to the id {id:?}");
                    assert_eq!(item["kind"], json!(bucket), "item in {name}.{bucket} carries the wrong kind: {item}");
                    assert_eq!(
                        item.get("trigger").is_some(),
                        expect_trigger,
                        "an item of {name}.{bucket} has the trigger the other bucket's items carry: {item}"
                    );
                    assert_eq!(
                        item.get("n").is_some(),
                        !expect_trigger,
                        "a self-gated item is numbered, or an ordered one is not: {item}"
                    );
                }
            }
        }
        assert!(item_count >= 9, "expected every cycle to carry its items, got {item_count}");
        assert_eq!(
            cycles.len(),
            CYCLES.len() + 1,
            "the emitted map holds something other than the cycles and their description"
        );
    }

    /// Every step of both runs, hot path and cold, in one iterator — what the checks below hold the
    /// whole spec to have to reach either without caring which it is looking at.
    fn every_step() -> impl Iterator<Item = (&'static str, &'static Step)> {
        AGENT_CYCLE.iter().map(|s| (BACKBONE, s)).chain(
            CYCLES
                .iter()
                .flat_map(|c| c.backbone.iter().chain(c.optional).map(move |s| (c.id.key(), s))),
        )
    }

    /// Discipline: a cycle nothing branches to is unreachable. It is written, emitted, and paid for
    /// by every session that reads the entry — and arrived at by no one, because the only thing that
    /// ever said when to take it was its own `when`, which nothing in the run pointed at
    /// (`AMB-D-574`). The other direction needs no test: a branch names a [`Cyc`], so one pointing at
    /// a cycle nobody wrote does not compile.
    #[test]
    fn every_cycle_is_reachable_from_a_step() {
        let mut branched: HashMap<&str, Vec<&str>> = HashMap::new();
        for (run, step) in every_step() {
            let mut named = HashSet::new();
            for cycle in step.cycles {
                assert!(named.insert(*cycle), "{run}.{} branches to {} twice", step.id, cycle.key());
                branched.entry(cycle.key()).or_default().push(step.id);
            }
        }
        for cycle in CYCLES {
            let key = cycle.id.key();
            assert!(
                branched.contains_key(key),
                "no step branches to the {key} cycle, so nothing reaches it — name it in the `cycles` of \
                 the step it fires at, or drop the cycle"
            );
        }
    }

    /// The most one step may say, in bytes of its prose. A step is one block a reader takes in before
    /// acting on it, and past this it has stopped being a step and become the argument for one — which
    /// belongs in a decision record, where it is read once, rather than here, where it is read every
    /// session (`AMB-D-574`).
    const MOST_A_STEP_SAYS: usize = 1600;

    /// The most one cold-path cycle may weigh, as the JSON it is emitted as: its `when`, its items,
    /// and their prose. A cycle past this is two cycles.
    const MOST_A_CYCLE_SAYS: usize = 4500;

    /// The most the whole entry point may weigh. Every AI session reads it before it does anything,
    /// so this is the standing cost of the tool having a face at all.
    const MOST_THE_ENTRY_SAYS: usize = 48_000;

    /// The most the entry inside a step may weigh. It is read once for every step of every run, so it
    /// carries what a step needs and nothing else ([`build_step`]).
    const MOST_THE_STEP_ENTRY_SAYS: usize = 8_000;

    /// Discipline: the entry inside a step stays short, and teaches what the table says a step may
    /// type — the three that hand the work back in full, the rest by name, and none that moves the
    /// task.
    #[test]
    fn the_entry_inside_a_step_is_short_and_names_registered_commands() {
        let entry = build_step();
        let whole = entry.to_string().len();
        assert!(
            whole <= MOST_THE_STEP_ENTRY_SAYS,
            "the entry inside a step weighs {whole} bytes, past the {MOST_THE_STEP_ENTRY_SAYS} it may",
        );
        let reaches: Vec<&str> =
            entry["reaches"]["commands"].as_array().expect("reaches").iter().filter_map(|c| c.as_str()).collect();
        for name in ["task show", "comment add", "automation run-show"] {
            assert!(reaches.contains(&name), "{name} is left with a step: {reaches:?}");
        }
        for name in ["task status", "task done", "task assign", "automation start"] {
            assert!(!reaches.contains(&name), "{name} is not a step's to type: {reaches:?}");
        }
        let handed_back: Vec<&str> =
            entry["commands"].as_array().expect("commands").iter().filter_map(|c| c["name"].as_str()).collect();
        assert_eq!(
            handed_back,
            vec!["automation step-take", "automation step-out", "automation step-done"],
            "the three that hand the work back come in full",
        );
        assert!(entry["commands"][2]["flags"].is_array(), "with their flags");
    }

    /// Discipline: nothing in the spec says more than its share. The ceilings are deliberately above
    /// where the writing sits — they catch the piece that ran away, not the sentence that grew, which
    /// is what the snapshot beside them is for (`the_entry_size_is_the_one_recorded`).
    #[test]
    fn nothing_says_more_than_its_share() {
        for (run, step) in every_step() {
            let said = step.prose.len();
            assert!(
                said <= MOST_A_STEP_SAYS,
                "{run}.{} says {said} bytes, past the {MOST_A_STEP_SAYS} one step may — split it, or move \
                 the reasoning to a decision record",
                step.id
            );
        }
        for cycle in CYCLES {
            let said = cycle.to_value().to_string().len();
            assert!(
                said <= MOST_A_CYCLE_SAYS,
                "the {} cycle weighs {said} bytes, past the {MOST_A_CYCLE_SAYS} one cycle may",
                cycle.id.key()
            );
        }
        let whole = entry_as_measured().to_string().len();
        assert!(
            whole <= MOST_THE_ENTRY_SAYS,
            "the entry point weighs {whole} bytes, past the {MOST_THE_ENTRY_SAYS} it may — every session \
             pays this before it does anything"
        );
    }

    /// The entry point as the size checks read it: [`build_index`], with the version pinned. The
    /// version is the one part of the document that moves without anyone writing a word, and a
    /// release that grows it by a digit would otherwise turn the snapshot red at the worst moment.
    fn entry_as_measured() -> Value {
        entry_as_measured_in(false)
    }

    /// [`entry_as_measured`], for either side of the one conditional the entry has. `false` is the
    /// document nearly every reader is served; `true` adds the line served inside a pane of the talk
    /// window, which is measured too so that line cannot grow unwatched either.
    fn entry_as_measured_in(in_a_pane: bool) -> Value {
        let mut spec = index(in_a_pane);
        if let Value::Object(map) = &mut spec {
            map.insert("version".to_string(), json!("x.y.z"));
        }
        spec
    }

    /// Where the recorded sizes live — beside the source they measure, so the diff that grows the
    /// entry and the diff that records the growth are the same diff.
    fn size_snapshot() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/agent-entry-size.txt")
    }

    /// The sizes as the snapshot spells them: every top-level section by the bytes of its value, then
    /// the whole document. The sections never sum to the total — the keys and punctuation between
    /// them are the document's too — and that is left as it is rather than explained away.
    fn entry_sizes() -> String {
        let spec = entry_as_measured();
        let mut out = String::from(
            "# `amenbo agent --json` in bytes: each top-level section, then the whole document.\n\
             # Not a ceiling — those are constants in agent.rs. This is the increment, made to land in a diff.\n\
             # Rewrite it deliberately: cargo test -p amenbo-core record_the_entry_size -- --ignored\n",
        );
        for (key, value) in spec.as_object().expect("the entry point is an object") {
            out.push_str(&format!("{key:<16} {}\n", value.to_string().len()));
        }
        out.push_str(&format!("{:<16} {}\n", "TOTAL", spec.to_string().len()));
        // And the one conditional: inside a pane of the talk window the entry carries a line naming the
        // surface layer. Almost nobody is served it, so it is recorded apart from the total above rather
        // than folded into it — and recorded all the same, so it cannot creep either.
        out.push_str(&format!(
            "{:<16} {}\n",
            "TOTAL_IN_A_PANE",
            entry_as_measured_in(true).to_string().len()
        ));
        out
    }

    /// The entry names the second vocabulary only where it can be used. Inside a pane it points at
    /// `talk --json` and says the words are owed; outside one it says nothing about it at all, which
    /// is the whole of `AMB-D-749` as the entry point sees it — a reader outside cannot tell a road they
    /// have not tried from one that is closed, so they are not shown a closed one.
    #[test]
    fn the_entry_names_the_surface_layer_only_inside_a_pane() {
        let outside = index(false);
        assert!(outside.get("talk").is_none(), "outside a pane the entry says nothing of it: {outside:#}");
        assert!(
            !outside.to_string().contains("talk --json"),
            "and points at it nowhere else either",
        );

        let inside = index(true);
        let said = inside["talk"].as_str().expect("inside a pane the entry names it");
        assert!(said.contains("talk --json"), "it names the canon to read: {said}");
        assert!(said.contains("owed"), "and says the layer asks something of the reader: {said}");
        assert_eq!(
            index(false).as_object().map(|m| m.len() + 1),
            index(true).as_object().map(|m| m.len()),
            "the pane adds that one section and moves nothing else",
        );
    }

    /// Discipline: the entry point is the size it was last recorded as. The ceilings above catch a
    /// section that ran away; this catches the creep under them — a paragraph a session, none of them
    /// worth stopping for, and a year later the entry costs twice what it did (`AMB-D-574`).
    ///
    /// It is a snapshot rather than a diff against another revision, so nothing has to be built twice
    /// and no measurement can quietly read a different document at each end. Recording the new size is
    /// one line, and it lands in the commit, where what grew is read next to why.
    #[test]
    fn the_entry_size_is_the_one_recorded() {
        // What is recorded is the shipped spelling. The retarget writes every command with this
        // build's CLI name, and a dev build's is four bytes longer everywhere it appears — measuring
        // one would record a document nobody is served. The gate builds the tests under neither, so
        // this only steps aside for a developer who set the name by hand.
        if Paths::command_name() != Paths::PRODUCTION_APP_NAME {
            return;
        }
        let now = entry_sizes();
        let recorded = std::fs::read_to_string(size_snapshot()).unwrap_or_default();
        if now == recorded {
            return;
        }
        let was: HashMap<&str, i64> = recorded.lines().filter_map(one_size).collect();
        let mut moved: Vec<String> = now
            .lines()
            .filter_map(one_size)
            .filter(|(section, size)| was.get(section) != Some(size))
            .map(|(section, size)| format!("  {section:<16} {:>+8}  → {size}", size - was.get(section).unwrap_or(&0)))
            .collect();
        moved.sort();
        panic!(
            "the entry point is not the size it was recorded as:\n{}\n\nEvery session reads this. Is what \
             you added a spec, or an argument? An argument belongs in a decision record; a command's detail \
             belongs behind `agent --command`. If it belongs here, record the new size:\n  \
             cargo test -p amenbo-core record_the_entry_size -- --ignored",
            moved.join("\n")
        );
    }

    /// Records what the check above compares against. It is the one way the snapshot is written, and
    /// deliberately not something a failing run does for you: `#[ignore]`d so no gate ever reaches it,
    /// typed out by whoever grew the entry, and landing in their commit beside the growth it measures.
    #[test]
    #[ignore = "writes the recorded entry sizes — run it when the entry has deliberately grown"]
    fn record_the_entry_size() {
        assert_eq!(
            Paths::command_name(),
            Paths::PRODUCTION_APP_NAME,
            "this build spells the CLI the dev way, so what it would record is a document nobody is served"
        );
        std::fs::write(size_snapshot(), entry_sizes()).expect("could not write the recorded sizes");
    }

    /// One `<section> <bytes>` line of the snapshot. A comment or a blank line is not one.
    fn one_size(line: &str) -> Option<(&str, i64)> {
        let (section, size) = line.split_once(char::is_whitespace)?;
        Some((section, size.trim().parse().ok()?))
    }

    /// Every piece of prose the spec emits, with its path, walked the way [`retarget_node`] walks it
    /// and split the same three ways: a runnable line is a line to type rather than prose about one,
    /// a reference is a name. Everything else is something a reader reads.
    fn all_prose(node: &Value, key: &str, path: &str, out: &mut Vec<(String, String)>) {
        match node {
            Value::Object(map) => {
                for (key, value) in map {
                    if RUNNABLE_LINE_FIELDS.contains(&key.as_str()) {
                        continue;
                    }
                    all_prose(value, key, &format!("{path}/{key}"), out);
                }
            }
            Value::Array(items) => {
                for (at, item) in items.iter().enumerate() {
                    all_prose(item, key, &format!("{path}[{at}]"), out);
                }
            }
            Value::String(text) if !REFERENCE_FIELDS.contains(&key) => {
                out.push((path.to_string(), text.clone()));
            }
            _ => {}
        }
    }

    /// The length at which a shared run of text stops being a turn of phrase and starts being the
    /// same fact written twice (`AMB-D-574`). Any longer match contains a window this size, so
    /// windows are all this has to look at.
    const SAME_FACT_TWICE: usize = 40;

    /// Reports the same fact written in two places — and reports only, never failing. Some
    /// repetition is doing real work (a refrain the reader is meant to meet again where it applies),
    /// so which of these is a duplicate and which is deliberate is the writer's call, not a
    /// threshold's (`AMB-D-574`). What it replaces is nobody watching at all: the cross-references
    /// this spec holds by hand — "the mailbox query is defined once in agentCycle step 1, referenced
    /// and not repeated here" — keep only as long as a writer's attention does.
    ///
    /// Read it with `cargo test duplicated_facts -- --nocapture`. What is asserted is only that the
    /// walk still reaches the prose: a collector that quietly stopped would otherwise report a clean
    /// spec forever.
    #[test]
    fn duplicated_facts_are_reported() {
        let mut prose = Vec::new();
        all_prose(&build(), "", "", &mut prose);
        assert!(prose.len() > 100, "the walk found almost no prose ({}) — it stopped reaching it", prose.len());

        let normalised: Vec<String> = prose
            .iter()
            .map(|(_, t)| t.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase())
            .collect();
        // Window → the strings holding it. A window in one string alone is not a repetition.
        let mut windows: HashMap<&str, std::collections::BTreeSet<usize>> = HashMap::new();
        for (at, text) in normalised.iter().enumerate() {
            let edges: Vec<usize> = text.char_indices().map(|(i, _)| i).chain([text.len()]).collect();
            for window in edges.windows(SAME_FACT_TWICE + 1) {
                windows.entry(&text[window[0]..window[SAME_FACT_TWICE]]).or_default().insert(at);
            }
        }
        // One pair of places may share several overlapping windows; the longest is the readable one.
        let mut shared: HashMap<std::collections::BTreeSet<usize>, &str> = HashMap::new();
        for (window, places) in windows.iter().filter(|(_, places)| places.len() > 1) {
            let longest = shared.entry(places.clone()).or_insert(window);
            if window.len() > longest.len() {
                *longest = window;
            }
        }

        let mut report: Vec<(&std::collections::BTreeSet<usize>, &&str)> = shared.iter().collect();
        report.sort_by_key(|(places, _)| places.iter().copied().collect::<Vec<_>>());
        println!("── the same text in more than one place ({} to judge) ──", report.len());
        for (places, window) in report {
            let where_ = places.iter().map(|at| prose[*at].0.as_str()).collect::<Vec<_>>().join(" ⇄ ");
            println!("  {where_}\n    …{window}…");
        }
    }

    /// The other end of [`Cmd`]. A variant names the registry by a string, and the registry is
    /// written as text, so this is the one seam the type cannot close — held in both directions. A
    /// command renamed or dropped there leaves a variant pointing at nothing, and every step holding
    /// it goes on compiling; a command added there and not here has no word on whether a run's step
    /// may type it, which is the one thing the table exists to make every command say (`AMB-D-968`).
    #[test]
    fn the_table_and_the_registry_name_the_same_commands() {
        let known: HashSet<String> = command_names().into_iter().collect();
        let mut table = HashSet::new();
        for cmd in Cmd::ALL {
            assert!(known.contains(cmd.name()), "{cmd:?} names {:?}, which is no command of ours", cmd.name());
            assert!(table.insert(cmd.name()), "two variants name {:?}", cmd.name());
        }
        for name in &known {
            assert!(
                table.contains(name.as_str()),
                "{name:?} is registered and has no variant, so nothing says whether a step may type it"
            );
        }
    }

    /// The narrowing the references bought: a name is exact, so the retarget's guess — is the word
    /// after `amenbo` a command? — is not run over one. Held against the authored spec rather than
    /// against a list of paths, so a reference field added anywhere is covered by having been added.
    #[test]
    fn the_retarget_leaves_references_alone() {
        fn compare(authored: &Value, dev: &Value, key: &str, path: &str) {
            match (authored, dev) {
                (Value::Object(a), Value::Object(d)) => {
                    for (key, value) in a {
                        compare(value, &d[key], key, &format!("{path}/{key}"));
                    }
                }
                (Value::Array(a), Value::Array(d)) => {
                    for (at, item) in a.iter().enumerate() {
                        compare(item, &d[at], key, &format!("{path}[{at}]"));
                    }
                }
                (Value::String(a), Value::String(d)) if REFERENCE_FIELDS.contains(&key) => {
                    assert_eq!(a, d, "the retarget rewrote the reference at {path}");
                }
                _ => {}
            }
        }
        let mut dev = spec_as_authored();
        retarget(&mut dev, Paths::DEV_APP_NAME);
        compare(&spec_as_authored(), &dev, "", "");
    }

    /// A command named in the prose must exist. The `commands` beside each step is a [`Cmd`] and so
    /// cannot be wrong; what nothing watched is the sentences, where a command is named far more
    /// often — and a name that has been renamed or dropped reads exactly like one that still works
    /// (`AMB-D-574`).
    ///
    /// A code span is what marks prose as a line to type, so that is the whole of what is read: a
    /// span that opens a command name must go on to name a real one. Prose outside the spans is left
    /// alone, where "the task status" is English and not an instruction.
    ///
    /// One word on its own is only read as a command when it *is* one whole. A cycle is spelled the
    /// same way a command opens — the `decision` cycle, the `commit` cycle — and a lone opener is a
    /// noun as often as an instruction, so demanding it name a command would fail on English.
    #[test]
    fn every_command_named_in_the_prose_exists() {
        let spec = build();
        let known: HashSet<String> = command_names().into_iter().collect();
        let openers: HashSet<&str> = known.iter().filter_map(|n| n.split(' ').next()).collect();
        let cli = Paths::command_name();

        let mut named = 0;
        let mut check = |where_: &str, prose: &str| {
            for span in prose.split('`').skip(1).step_by(2) {
                let words: Vec<&str> = span.split_whitespace().skip_while(|w| *w == cli).collect();
                let Some(opener) = words.first() else { continue };
                if !openers.contains(opener) || (words.len() == 1 && !known.contains(*opener)) {
                    continue;
                }
                // Longest first: `notify` is a command and so is `notify target-list`, and the longer
                // one is what the span calls. Two words is the whole depth of a name (`AMB-D-946`).
                let matched =
                    (1..=2.min(words.len())).rev().map(|n| words[..n].join(" ")).find(|c| known.contains(c));
                assert!(matched.is_some(), "{where_} names `{span}`, which is no command of ours");
                named += 1;
            }
        };

        for step in spec["agentCycle"]["steps"].as_array().expect("agentCycle.steps is an array") {
            check(&format!("agentCycle.{}", step["id"]), step["step"].as_str().unwrap_or(""));
        }
        for cycle in CYCLES {
            for bucket in ["backbone", "optional"] {
                for item in spec["cycles"][cycle.id.key()][bucket].as_array().expect("a cycle bucket is an array") {
                    check(&format!("{}.{}", cycle.id.key(), item["id"]), item["step"].as_str().unwrap_or(""));
                }
            }
        }
        assert!(named > 10, "the scan found almost no commands in the prose ({named}) — it stopped reading the spans");
    }





    /// A cycle the runtime drops takes every branch to it with it. Left behind, a step would go on
    /// telling the reader to take a cycle that is not in the document they were handed — which reads
    /// as a piece gone missing rather than one that never applied here.
    #[test]
    fn dropping_a_cycle_takes_the_branches_to_it() {
        let mut spec = build();
        assert!(
            spec[BACKBONE]["steps"].as_array().is_some_and(|steps| steps
                .iter()
                .any(|s| s["cycles"].as_array().is_some_and(|c| c.contains(&json!("worktree"))))),
            "nothing branches to the worktree cycle, so this proves nothing"
        );
        drop_cycle(&mut spec, Cyc::Worktree);

        assert!(spec["cycles"].get("worktree").is_none(), "the cycle itself is still there");
        for (run, step) in every_step() {
            let emitted = find_step(&mut spec, run, step.id);
            let Some(emitted) = emitted else { continue }; // the step was inside the dropped cycle
            let branches = emitted["cycles"].as_array().cloned().unwrap_or_default();
            assert!(!branches.contains(&json!("worktree")), "{run}.{} still branches to it", step.id);
            assert!(!branches.is_empty() || emitted.get("cycles").is_none(), "an emptied branch was left as []");
        }
    }

    /// Discipline: every pair in [`GIT_ONLY`] names a step that is actually written. A `Cyc` the
    /// compiler checks and an id it does not is half a reference, and the half it does not check is
    /// the one that rots — a step renamed here would leave the table pointing at nothing, and the
    /// run would go on handing a reader without git a line they cannot run.
    #[test]
    fn every_git_only_step_is_written() {
        for (cycle, id) in GIT_ONLY {
            assert!(
                every_step().any(|(run, step)| run == cycle.key() && step.id == *id),
                "GIT_ONLY names {}.{id}, and no step is written there",
                cycle.key()
            );
        }
    }

    /// A run with no git keeps the advice it can act on and loses the rest. `commit` is the cycle
    /// the line runs through: the lint step stands, because linting a file or piped text needs no
    /// checkout, and the two steps that do need one are gone.
    #[test]
    fn a_run_without_git_keeps_the_lint_step_and_drops_the_git_ones() {
        let mut spec = build();
        for (cycle, id) in GIT_ONLY {
            assert!(find_step(&mut spec, cycle.key(), id).is_some(), "{id} is not there to drop");
        }

        drop_git_only_steps(&mut spec);

        for (cycle, id) in GIT_ONLY {
            assert!(find_step(&mut spec, cycle.key(), id).is_none(), "{id} survived the drop");
        }
        assert!(
            find_step(&mut spec, Cyc::Commit.key(), "lint-what-leaves").is_some(),
            "the advice that holds without git went with the advice that does not"
        );
    }

    /// What is left of a bucket is numbered from where it now sits. A hole in the run reads as a
    /// piece gone missing, which is exactly the impression a reader who was never meant to see the
    /// step should not be given.
    #[test]
    fn dropping_a_step_closes_up_the_numbering() {
        let mut spec = build();
        drop_git_only_steps(&mut spec);
        for (key, cycle) in spec["cycles"].as_object().expect("cycles is an object") {
            if key == "description" {
                continue;
            }
            for bucket in ["backbone", "optional"] {
                let items = cycle[bucket].as_array().expect("a bucket is an array");
                for (at, step) in items.iter().enumerate() {
                    if step.get("n").is_some() {
                        assert_eq!(step["n"], json!(at), "{key}.{} is numbered off its place", step["id"]);
                    }
                }
            }
        }
    }

    // ──────────────────── The two layers: index ⇄ full spec ────────────────────

    /// The entry point carries how to work here in full and the commands as an index. An index row
    /// is the name alone: anything else on it — a summary, let alone flags/args/examples — is the
    /// detail side leaking back into the layer that is read every session.
    #[test]
    fn the_entry_point_indexes_commands_and_keeps_everything_else_whole() {
        let full = build();
        let index = build_index();

        for key in ["principles", "operating", "agentCycle", "cycles", "conventions", "filterGrammar", "notes", "capabilities", "inspect"] {
            assert_eq!(index[key], full[key], "{key} is cut from the entry point (only commands are indexed)");
        }

        let rows = index["commands"].as_array().expect("commands is an array");
        let names: Vec<&Value> = full["commands"].as_array().unwrap().iter().map(|c| &c["name"]).collect();
        assert_eq!(rows.iter().collect::<Vec<_>>(), names, "the index is every command's name, in the source's order — a command missing from it cannot be pulled");
        for row in rows {
            assert!(row.as_str().is_some_and(|n| !n.is_empty()), "an index row is the name alone: {row}");
        }
        assert!(index["commandDetail"].as_str().unwrap().contains("--command"), "an index that does not say how to pull invites hallucination");
        assert!(index["commandDetail"].as_str().unwrap().contains("capabilities"), "a name-only index must point at the map from intent to name");
    }

    /// Removing information is a non-goal: every name in the index must lead to its full spec. Only
    /// the route changed.
    #[test]
    fn every_indexed_command_can_be_pulled_in_full() {
        for name in command_names() {
            let spec = command_spec(&name).unwrap_or_else(|| panic!("indexed command {name} cannot be pulled"));
            assert_eq!(spec["name"], json!(name));
            assert!(spec.get("summary").is_some(), "the full spec of {name} has no summary");
            // Whatever the entry point dropped comes back whole on the detail side.
            let full_row = build()["commands"].as_array().unwrap().iter()
                .find(|c| c["name"] == json!(name)).cloned().unwrap();
            assert_eq!(spec, full_row, "the pulled spec differs from the source of truth");
        }
        assert!(command_spec("no such command").is_none());
    }

    /// Walks the runnable-line fields the same way [`retarget_runnable_lines`] does, so a field it
    /// stops reaching is a line this collector still finds — spelled at the wrong CLI.
    fn runnable_lines(node: &Value, out: &mut Vec<String>) {
        fn collect(node: &Value, out: &mut Vec<String>) {
            match node {
                Value::Array(items) => items.iter().for_each(|i| collect(i, out)),
                Value::String(line) => out.push(line.clone()),
                _ => {}
            }
        }
        match node {
            Value::Object(map) => {
                for (key, value) in map {
                    if RUNNABLE_LINE_FIELDS.contains(&key.as_str()) {
                        collect(value, out);
                    } else {
                        runnable_lines(value, out);
                    }
                }
            }
            Value::Array(items) => items.iter().for_each(|i| runnable_lines(i, out)),
            _ => {}
        }
    }

    /// A line the spec tells someone to type must name the CLI this build installs — and name it at
    /// all, since a line that dropped the command word is unrunnable on every channel. The dev
    /// channel is where the first half bites: its examples would otherwise send an AI to a command
    /// that is not installed there. So the rule is checked twice: as this build hands the spec out,
    /// and after a retarget to the dev spelling, which is what says the rewrite reaches every
    /// runnable-line field.
    #[test]
    fn every_runnable_line_names_this_builds_cli() {
        /// Whether the line names `cli` as a word of its own — the reading side of [`standalone`].
        fn names(line: &str, cli: &str) -> bool {
            line.match_indices(cli)
                .any(|(at, _)| standalone(&line[..at], &line[at + cli.len()..]))
        }

        let mut lines = Vec::new();
        runnable_lines(&build(), &mut lines);
        assert!(lines.len() > 50, "the walk found almost no runnable lines ({}) — it stopped reaching them", lines.len());
        let here = Paths::command_name();
        for line in &lines {
            assert!(names(line, here), "a runnable line does not name this build's CLI ({here}): {line}");
        }

        let mut dev = build();
        retarget(&mut dev, Paths::DEV_APP_NAME);
        let mut dev_lines = Vec::new();
        runnable_lines(&dev, &mut dev_lines);
        assert_eq!(dev_lines.len(), lines.len(), "the retarget changed how many runnable lines there are");
        for line in &dev_lines {
            assert!(names(line, Paths::DEV_APP_NAME), "a runnable line kept its authored CLI through the retarget: {line}");
            assert!(!names(line, Paths::PRODUCTION_APP_NAME), "a runnable line still names the production CLI after the retarget: {line}");
        }
    }

    /// The other half of the retarget: prose that tells the reader to type something must move too,
    /// while prose that names the product must not. Both directions are checked on the dev spelling,
    /// where the two spellings finally differ — and the second is the one that needs a test, since a
    /// rule loose enough to catch every command would also rename the product ("a newer Amenbo").
    #[test]
    fn retargeting_prose_moves_commands_and_leaves_the_product_alone() {
        let mut dev = spec_as_authored();
        retarget(&mut dev, Paths::DEV_APP_NAME);

        let cycle = dev["agentCycle"]["steps"].as_array().unwrap().iter()
            .filter_map(|s| s["step"].as_str()).collect::<Vec<_>>().join(" ");
        assert!(cycle.contains("`amenbo-dev task list --filter"), "the mailbox query still names the production CLI: {cycle}");
        assert!(cycle.contains("`amenbo-dev task status <id> in_progress`"), "the reserve step still names the production CLI");
        assert!(dev["conventions"]["reach"].as_str().unwrap().contains("`amenbo-dev bind --project"), "the way out of an unbound folder still names the production CLI");
        let hooks = dev["commands"].as_array().unwrap().iter().find(|c| c["name"] == "hooks install").unwrap();
        assert!(hooks["summary"].as_str().unwrap().contains("`amenbo-dev lint`"), "a command summary still names the production CLI");

        // The product keeps its name. Prose spells it `Amenbo`, which no retarget matches by
        // construction — these pin that spelling so a lowercase regression, which the prose rule
        // could then mistake for a command, has a test to fail.
        assert_eq!(dev["amenbo"], spec_as_authored()["amenbo"], "the product line was retargeted as if it were a command");
        let update = dev["commands"].as_array().unwrap().iter().find(|c| c["name"] == "update").unwrap();
        assert!(update["summary"].as_str().unwrap().contains("Updates Amenbo."), "the product's name was rewritten inside prose");
        assert!(update["summary"].as_str().unwrap().contains("Amenbo never updates in the background"), "the product's name was rewritten inside prose");
        // A noun that doubles as a command word is the third arm of that rule, and no line of the spec
        // is written in that shape any more — the one that was went with the plugin group. It is held
        // where the rule itself is (`retargeting_help_prose_moves_commands_and_flags_only`), which is
        // the honest place for it: over text rather than over whichever sentence happens to carry it.
    }

    /// The prose rule read at the door the CLI's `--help` comes through: text authored elsewhere, one
    /// string at a time. What follows the name is the whole of the rule — a command word or a flag
    /// makes it a line to type, anything else leaves it the product's name — so all three arms are
    /// held here, on the dev spelling, where the two spellings differ.
    #[test]
    fn retargeting_help_prose_moves_commands_and_flags_only() {
        let commands = command_words(&spec_as_authored());
        let dev = |text: &str| rewrite(text, Paths::DEV_APP_NAME, |a| names_a_command(a, &commands));

        assert_eq!(dev("run `amenbo lint` on every commit"), "run `amenbo-dev lint` on every commit");
        // A global flag may sit ahead of the subcommand, putting a dash where the command word goes.
        assert_eq!(dev("`amenbo --project <name> decision add …`"), "`amenbo-dev --project <name> decision add …`");
        // Wrapped rather than leading, and still a line to type.
        assert_eq!(dev(r#"`eval "$(amenbo worktree start 123)"`"#), r#"`eval "$(amenbo-dev worktree start 123)"`"#);
        // The product, not a command: nothing follows that says otherwise. Authored lowercase on
        // purpose — spec prose now spells the product `Amenbo`, but this door takes text authored
        // elsewhere, where a lowercase product mention can still arrive.
        assert_eq!(dev("Update amenbo to the latest release."), "Update amenbo to the latest release.");
        // A command word that doubles as a plain noun is not read as a command (NOT_A_COMMAND_IN_PROSE).
        assert_eq!(dev("a minimum amenbo version"), "a minimum amenbo version");
        assert_eq!(dev("the store amenbo-dev keeps"), "the store amenbo-dev keeps");
    }

    /// The entry point must teach how to explore — narrow, list, then open the few that matter. Lose
    /// that and the AI falls back on dumping everything, melting its context before it has started
    /// the work. The norm and the two-layer shape guard the same thing, so they are kept together.
    /// The filter grammar is what an agent reads *instead of* the source before it queries, so a status
    /// the store accepts and the grammar omits is a task the agent will never think to ask for. Held to
    /// the enum itself rather than to a copy of the list.
    #[test]
    fn the_filter_grammar_names_every_status_the_store_accepts() {
        let grammar = build()["filterGrammar"]["keys"]["status"].as_array().unwrap().clone();
        let listed: Vec<&str> = grammar.iter().filter_map(|v| v.as_str()).collect();
        for status in crate::model::TaskStatus::ALL {
            assert!(
                listed.contains(&status.as_str()),
                "the grammar does not name `{}`: {listed:?}",
                status.as_str()
            );
        }
    }

    /// What an AI reads about `export` has to be what happens to an AI. The spec said the dump
    /// streams to stdout with no `--out`, which is the one shape a closed reach never gets: a reader
    /// who believed it redirected the call, and the export landed in the folder they were standing
    /// in — 87 MB of it, in a repository, with `git status` dirty afterwards.
    #[test]
    fn the_export_spec_says_what_a_closed_reach_is_given_instead_of_the_stream() {
        let spec = command_spec("export").expect("export is a command the index carries");
        let said = spec.to_string();
        assert!(said.contains("stdout"), "the stream is still what a human gets: {said}");
        for needle in ["closed reach", "amenbo-export-"] {
            assert!(
                said.contains(needle),
                "the export spec does not say what a closed reach is handed ({needle}): {said}",
            );
        }
    }

    #[test]
    fn the_entry_point_teaches_narrowing_before_reading() {
        let operating = build()["operating"].as_array().unwrap().clone();
        let prose: String =
            operating.iter().filter_map(|s| s.as_str()).collect::<Vec<_>>().join(" ");
        for needle in ["--filter", "--limit", "task show"] {
            assert!(prose.contains(needle), "operating does not teach the exploration tool {needle}");
        }
    }
}
