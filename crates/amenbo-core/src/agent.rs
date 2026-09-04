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
use std::collections::{BTreeMap, HashSet};

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
            "A decision record is offered, never authored on the user's behalf: `decision add` starts it proposed, and the human is the one who accepts or rejects it. It also weighs less than filing the task — that duty is standing, this one is a nudge, so don't paper every change with a decision. Where the judgement is made is step 0, and nowhere else."
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
            "reach": "An AI works inside the project its folder is bound to, and nowhere else. The `.amenbo` pointer is not a way of naming a store; it **is** the AI's reach: what it can list, read, and write. Three consequences, all enforced (an AI does not have to remember them — the CLI refuses with `out_of_reach`, never a silent empty result): (1) **you do not pick a project.** `--project` and the `project:` filter are human-only vocabulary; passing either is refused even when it names your own bound project. The flip side is that you rarely need one — `task add` / `decision add` / `dimension add` / `dimension list` take their project from the binding, so just omit it. (2) **you read only your project.** Lists, timelines, `status` and `project list` show your project's rows; an id from another project — a task, a decision, a comment id, an attachment id, a dimension — is refused rather than served. (3) **you write only inside it.** Mutations outside are refused, and so is creating anything outside (a new project included). Being out of reach is not being absent: the answer is `out_of_reach`, never `not_found` — the row exists, you just cannot get to it from here. A folder with no binding at all reaches nothing, so an AI there is refused outright: ask the human to `amenbo bind --project <name or id>`, or work in a bound folder. Why: what an AI reads lands in its context and leaks from there into summaries, commits and handoffs, and a write outside its project is a decision made without that project's context. This closes Amenbo's own surface, and nothing more — the human is not scoped (an overview is a person's job), and a shell can still read the file underneath.",
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
/// (`eval "$(amenbo plugin run …)"`), and one that names it in the middle is as unrunnable on the dev
/// channel as one that opens with it.
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

/// Declares [`Cmd`] — the variant a step names a command by, and the name the registry answers to —
/// in one table, so the two can never be written apart.
macro_rules! commands {
    ($($variant:ident => $name:literal,)*) => {
        /// A command a step names, as a value rather than a spelling. A step used to carry the name
        /// as text, where a command renamed or dropped left a reference that read exactly like a
        /// working one and that nothing caught until an AI typed it. Naming one is now
        /// `Cmd::TaskAdd`, and a command that is not in the table below does not compile
        /// (`AMB-D-574`: what a type can refuse never needs a test).
        ///
        /// What the type cannot reach is the registry ([`all_commands`]), which is still written as
        /// text — so `every_command_a_step_names_exists` holds this table's far end against it, and
        /// `no_command_is_named_that_no_step_names` keeps the table from outliving its callers.
        #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
        enum Cmd { $($variant,)* }

        impl Cmd {
            /// Every variant, for the tests that hold this table against the registry and the steps.
            #[cfg(test)]
            const ALL: &'static [Cmd] = &[$(Cmd::$variant,)*];

            /// The name the registry answers to — what is emitted, and what the prose spells out.
            const fn name(self) -> &'static str {
                match self { $(Cmd::$variant => $name,)* }
            }
        }
    };
}

commands! {
    CommentAdd => "comment add",
    CommentList => "comment list",
    DecisionAccept => "decision accept",
    DecisionAdd => "decision add",
    DecisionAmend => "decision amend",
    DecisionEdit => "decision edit",
    DecisionLink => "decision link",
    DecisionList => "decision list",
    DecisionPromote => "decision promote",
    DecisionShow => "decision show",
    DecisionSupersede => "decision supersede",
    DimensionAdd => "dimension add",
    DimensionSet => "dimension set",
    HooksInstall => "hooks install",
    HooksStatus => "hooks status",
    Lint => "lint",
    Search => "search",
    TaskAdd => "task add",
    TaskAssign => "task assign",
    TaskBlock => "task block",
    TaskCommitAdd => "task commit add",
    TaskDepend => "task depend",
    TaskDone => "task done",
    TaskFinishCreating => "task finish-creating",
    TaskList => "task list",
    TaskReject => "task reject",
    TaskShow => "task show",
    TaskStatus => "task status",
    TaskUndepend => "task undepend",
    TaskUpdate => "task update",
    Validate => "validate",
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
            Cmd::DecisionAccept,
            Cmd::DecisionLink,
            Cmd::TaskUpdate,
            Cmd::TaskFinishCreating,
        ],
        cycles: &[Cyc::Worktree, Cyc::ExecutionExceptions],
        prose: "`amenbo task status <id> in_progress` (todo→in_progress), then re-confirm it is in_progress with `amenbo task show <id>` before starting. `status` is the whole double-work guard, and reserving is a compare-and-swap: `→ in_progress` succeeds only when the task is currently `todo`. If another session reserved it first, your reserve is rejected with `already_reserved` (a non-zero exit) instead of silently succeeding — so the collision is actually detected. The reserve also requires `ready`, so a task with an open blocker, an unsettled premise, a start day still ahead, or a creation still unfinished is rejected with `not_ready` (also a non-zero exit), and there is no `--force`. The two failures pull in opposite directions. On `already_reserved` the task is taken by someone else: go back to step 1 and pick the next one. On `not_ready` it is your own declaration that holds it: resolve the premise — finish the blocker or `task undepend` it; `decision accept` the linked decision, or `decision link --unlink` it; correct a start day you declared wrong with `task update <id> --start today` (or `--clear-start`); end a creation that is still open with `task finish-creating <id>` — and reserve only then. Both guards judge this transition only: an edge added, or a decision rejected, under a running task never strips its status, and `→ todo` stays unconditional, so the hand-back path is never closed. While a creation is unfinished the task stays at `todo`: `→ done` / `→ blocked` / `→ rejected` are refused as `invalid_task_status_draft`; a draft leaves by `task delete`.",
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
        commands: &[Cmd::TaskDone, Cmd::TaskReject, Cmd::CommentAdd, Cmd::TaskStatus],
        cycles: &[Cyc::Worktree],
        prose: "a task ends one of two ways — `amenbo task done <id>` for work you carried out, `amenbo task reject <id> --reason ...` for work you concluded should not be done (neither `done` nor `delete`). Either way, leave the context on the task's timeline with `amenbo comment add <id> --text ...` so the next session can pick up. (If you decide not to take a reserved task — as opposed to deciding it should not be done at all — hand it back with `amenbo task status <id> todo`.)",
    },
];

/// Assembles [`AGENT_CYCLE`] into the emitted shape: the standing description, then the steps in
/// order, each carrying the id that names it. Where a step used to open with `0. intake:`, the
/// number and the name are now fields beside the prose — one place, readable by a machine, and no
/// longer said twice.
fn agent_cycle() -> Value {
    json!({
        "description": "The AI's recommended execution backbone (pass --actor ai on every command) — a proven default for proceeding autonomously while avoiding parallel collisions, not a mandate. When you follow it, enter at the step whose `trigger` describes where you are and run from there in order; a step with no trigger is simply the next one. A step's `cycles` are the cold-path cycles that branch off it — take one only when its own trigger fires, then come back. A step may also carry `tools`: lines to type that an installed plugin declared for it — its own, and described under `plugins`.",
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
                prose: "Offer to record the rationale as a decision — the `decision` cycle carries it from there, linking it to the tasks that implement it. Surface it for the human to accept or reject; don't author it as already settled.",
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
        when: "You are putting a 'why' on the record — offering a new one, or fitting it to the decisions already there (step 0 is where you judge that it is worth offering).",
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
                prose: "Freeze the rationale as an append-only decision — it starts proposed (your proposal; the human accepts or rejects it), so offer the record, don't impose it.",
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
                id: "hand-it-over",
                n: Some(3),
                trigger: None,
                commands: &[],
                cycles: &[],
                prose: "Hand it over, rather than filing it and moving on — that, and not the record itself, is where your part ends. It stays proposed until the human rules on it, and meanwhile every task you linked it to reads ready:no, so the work it governs is stalled on an answer nobody was asked for. Say what you offered and what waits on it.",
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
                id: "accept",
                n: None,
                trigger: Some("a proposed decision is agreed and ready to settle"),
                commands: &[Cmd::DecisionAccept],
                cycles: &[],
                prose: "Accept it (stamps decided_at/decided_by).",
            },
            Step {
                id: "edit-in-place",
                n: None,
                trigger: Some("an accepted decision needs a minor fix (typo / stale line)"),
                commands: &[Cmd::DecisionEdit],
                cycles: &[],
                prose: "Edit it in place — accepting no longer freezes the body, so there is no reopen/re-accept round-trip. Supersede stays for a change of mind; reopen for un-settling a too-hasty acceptance.",
            },
            Step {
                id: "check-for-contradiction",
                n: None,
                trigger: Some("you just added or accepted a decision"),
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
                prose: "Sweep the current (accepted) decisions for semantically contradicting pairs, but keep each pass bounded: page through a slice you can actually reason over (decision list --filter status:accepted --with-body --limit N --offset …) rather than loading everything at once, and rotate the window across passes. Detection is best-effort recall by design — as the corpus outgrows one pass, coverage stays bounded to what you can reach now and repeated passes widen it over time.",
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
                commands: &[Cmd::TaskDone, Cmd::TaskUndepend, Cmd::DecisionAccept, Cmd::DecisionLink, Cmd::TaskUpdate, Cmd::TaskFinishCreating],
                cycles: &[],
                prose: "Resolve the premise you declared, rather than working around it: finish the blocker or drop the edge; get the linked decision accepted, unlink it, or relink it to its successor; move or clear a start day that has not come; end a creation that is still open. There is no --force. Two resolve without anyone else: a start day resolves itself, so if the work is meant to wait, leave it and take the next task; an open creation resolves where you stand, once you have read what is there and it is a task to work on.",
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
        when: "You are about to commit, or to send text out of this store some other way (a PR body, an issue, a message).",
        backbone: &[
            Step {
                id: "lint-what-leaves",
                n: Some(0),
                trigger: None,
                commands: &[Cmd::Lint],
                cycles: &[],
                prose: "Lint what is leaving, before it leaves: the staged diff, and the commit message. It reports and never edits, so a ref it names is yours to rewrite out of the text.",
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
                commands: &[],
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
                prose: "If cutting one hands a line back to you, that line is to run, not to read: what a called command face writes on stdout is its return value, relayed verbatim, so the way in arrives as text — wrap it (`eval \"$(…)\"`, or `iex (…)` in PowerShell). Read it and stop, and you are still standing in the project folder with a worktree nobody entered.",
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
                commands: &[],
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

/// **Hangs each call line on the step that named it** (`AMB-D-571`): `tools` keyed by `<run>.<step>`,
/// against a step of `agentCycle` or of a cycle, wherever the id lands.
///
/// The join is one-directional and that is the point. A plugin's manifest names a step
/// ([`crate::plugin_agent::tools`] is where the naming is read); nothing here names a plugin, so
/// Amenbo's source stays free of any plugin's name (`AMB-D-346`) and this spec keeps saying the same
/// true thing with none installed. What lands is the line to type and nothing around it — the words
/// about it are on the `plugins` shelf, where a reader can tell whose they are (`AMB-D-437`).
///
/// **A ref that names nothing here is dropped, not reported.** The steps travel with Amenbo and a
/// manifest stays where it was installed, so a ref can outlive the step it named; the runtime also
/// drops whole cycles that do not apply (the `worktree` cycle off a git checkout), which would make an
/// accurate ref look like a broken one. Either way the reader loses a line they had no use for.
///
/// Runtime, so it runs on the built spec rather than inside it: what is installed and switched on is
/// the store's answer, and [`build`] stays a static builder.
pub fn attach_tools(spec: &mut Value, tools: &BTreeMap<String, Vec<String>>) {
    for (step_ref, lines) in tools {
        let Some((run, id)) = step_ref.split_once('.') else { continue };
        let Some(step) = find_step(spec, run, id) else { continue };
        let Some(map) = step.as_object_mut() else { continue };
        // Call lines, and never prose: `tools` is a shelf a sentence has no way onto.
        map.insert("tools".to_string(), json!(lines));
    }
}

/// The emitted step `id` names within `run` — the backbone's steps under [`BACKBONE`], a cycle's under
/// its own key, both buckets of it, since which bucket an item sits in is the cycle's business and not
/// the namer's. `None` where the run, the step, or the whole cycle is not in this document.
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
            &["task commit add", "task commit list", "task commit rm"],
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
        cap("Discuss on a decision's timeline (accept/reject reasons land here; a comment posted by mistake can be edited or deleted)", &["decision comment add", "decision comment list", "decision comment edit", "decision comment rm"]),
        cap(
            "Attach what text cannot hold — screenshots, raw logs, benchmarks — to tasks, decisions, comments",
            &["task attach", "decision attach", "comment attach", "decision comment attach", "attach ls", "attach show", "attach open", "attach save", "attach rm"],
        ),
        cap(
            "Find where a word is written (tasks, decisions, the comments on both)",
            &["search"],
        ),
        cap(
            "Read the shared activity timeline (system events plus comments)",
            &["activity"],
        ),
        cap("Record a decision — an append-only \"why we chose X\"", &["decision add"]),
        cap("Find and filter decisions (words are search's)", &["decision list"]),
        cap("See a decision, its supersession chain, and the premises it stands on", &["decision show"]),
        cap(
            "Record that a decision stands on an older one (read that first; revisit this if it is overturned)",
            &["decision builds-on"],
        ),
        cap(
            "Move a decision through its lifecycle (accept / reject / reopen / edit / supersede / delete)",
            &["decision accept", "decision reject", "decision reopen", "decision edit", "decision supersede", "decision delete"],
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
            "Hand a carrier plugin what it takes this store's data outward with — the version of what it may see, one whole snapshot of it, what has changed since a cursor, and those records read back by id (a carrier's road, not a way to read the work)",
            &["sync version", "sync snapshot", "sync changes", "sync records"],
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
            "Write a verified full snapshot of the store's truth source to a single file",
            &["backup"],
        ),
        cap(
            "Restore the store's truth source from a verified snapshot (the recovery side of backup)",
            &["restore"],
        ),
        cap(
            "Physically erase content from the store — a comment on a task or a decision in full, or one accepted decision's body (human-gated maintenance)",
            &["hard-erase comment", "hard-erase decision-comment", "hard-erase decision"],
        ),
        cap(
            "Validate a plugin manifest against the catalog rules before submitting it (an author's self-check)",
            &["plugin validate"],
        ),
        cap(
            "Put a plugin from the catalog on this machine, see what is installed and what the catalog has moved past, bring one onto the published build or roll that back, open or close each one's gate (install ≠ enable, one project at a time), and remove one with everything it left behind",
            &[
                "plugin list",
                "plugin install",
                "plugin update",
                "plugin rollback",
                "plugin enable",
                "plugin disable",
                "plugin uninstall",
            ],
        ),
        cap(
            "Call an enabled plugin's command face and use what it returns (its stdout is the return value)",
            &["plugin run"],
        ),
        cap(
            "Read the plugin execution log — the last runs of each plugin, how each one ended, and what it wrote to stderr",
            &["plugin log"],
        ),
        cap(
            "Deliver what is waiting on the plugins' queues now, and see what each one got through",
            &["plugin flush"],
        ),
        cap(
            "Fill in and read back an installed plugin's settings (the keys its author declared)",
            &["plugin config set", "plugin config get"],
        ),
        cap(
            "Register third-party catalogs to browse alongside the official one, pinning the key each one signs with, and list what is registered",
            &["plugin catalog list", "plugin catalog add", "plugin catalog remove"],
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
        cmd("update", "Updates Amenbo. By default opens this OS's one-piece installer (GUI + CLI) — resolved from the published latest.json, falling back to the releases page — in your browser. Because you typed it, the lookup asks upstream rather than answering from the detection cache, so it never reports on an entry up to 24 hours old (offline it still falls back to the last one it had). `--apply` self-updates the standalone CLI in place instead: it downloads the new CLI over TLS and swaps this binary (no installer, no elevation), keeping the replaced binary beside it; a GUI-managed CLI is updated from the desktop app, not here. `--rollback` undoes the last `--apply` offline, restoring that kept binary. Applying is always your explicit call — Amenbo never updates in the background.",
            json!([{ "name": "--print", "help": "print the installer URL instead of opening a browser (headless / scripted use)" },
                   { "name": "--apply", "help": "self-update the standalone CLI in place (download + swap this binary) instead of opening the installer" },
                   { "name": "--rollback", "help": "undo the last --apply, restoring the previous binary kept beside this one (offline, no download)" },
                   { "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo update", "amenbo update --print", "amenbo update --apply", "amenbo update --rollback"])),
        cmd("config", "Shows this store's configuration: where its files are, and every setting `config set` writes — the same settings `--json` carries, each line naming the key alongside its value, so a value that was set can be read back by whoever set it.",
            json!([{ "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo config", "amenbo config --json"])),
        json!({ "name": "config set", "summary": "Changes a configuration value. Known keys: default_view, language, date_locale (how dates are written, as a BCP-47 tag; unset follows language — the GUI reads it, the CLI never does), human_name, ai_name (the display names of the two actors — this is the only way to rename either), human_avatar, ai_avatar (their icons, as a data:image/png;base64 URI), ai_allow_project_ops, startup_integrity_check (read-only integrity doctor at open; warnings only; default on), update_check (asks Amenbo's update endpoint whether a newer release is out; infra-side only — no user data; timeout + silent-fail + cached; default on; AMENBO_UPDATE_CHECK=0 overrides).",
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
        json!({ "name": "search", "summary": "Finds where words are written, across tasks, decisions and the comments on both — plus the labels a task is filed under and the names of what is attached. Answers with one line per PLACE, not per record: the face the words landed on (title / body / comment / label / attachment), the record it belongs to, the comment to open where there is one, and a short excerpt. Words are ANDed and match as substrings, with no word boundaries (so a search for part of a compound word finds it); full-width, case and kana spellings are folded together. Every word has to land somewhere on the record — not all on one face — and each face carrying one is a line, which is what makes the answer 'here is where each of your words is written'. A word written as a ref (AMB-T-<n> / AMB-D-<n>) pins that record to the top, so holding a number takes the same command as holding a phrase. A number alone (12 / #12) is a ref as well: the two sides number themselves apart, so it pins the task AND the decision that carry it, and --kind keeps the side you named. Default order is the face first (a name outranks a paragraph, and that a remark), newest within it. Every line also says where the record it points at stands — a task's status, its priority and what it is filed under, a decision's status — so a hit on work that is already over says so without a second read. This read has a ceiling of its own — a hit carries an excerpt — so with no --limit it returns the first 20 and reports the total.",
            "args": [{ "name": "word...", "required": true, "help": "the words to look for (ANDed)" }],
            "flags": [{ "name": "--project <id>", "help": "look in this project alone (human only — an AI is already scoped to its bound project). An argument of its own, not a --filter key: a project is an axis tasks and decisions carry alike, so scoping to one keeps decisions in the answer" },
                      { "name": "--filter <expr>", "help": "narrow structurally, in the grammar of the side --kind names: task list's for a task (see filterGrammar), decision list's for a decision. It requires --kind, because the two grammars share spellings that mean different things — status:rejected is work decided against on one side and a decision turned down on the other" },
                      { "name": "--kind <task|decision>", "help": "keep one side: the words on a task, or the words on a decision" },
                      { "name": "--face <title|body|comment|label|attachment>", "help": "keep one face of the record: its name, its text, a remark on it, a label it is filed under, the name of what is attached. The axis beside --kind and judged apart from it, so naming both is the product of the two — --kind decision --face comment is the remarks on decisions, which neither axis alone can ask for" },
                      { "name": "--sort <face|time|-time>", "help": "face (default: the face first, newest within it), -time (newest first), time (oldest first). The instant is the hit's own — a comment's posting time, or when the text it sits in was last written" },
                      { "name": "--limit <n>", "help": "max hits (default 20). total_matched says what the ceiling left behind" },
                      { "name": "--offset <n>", "help": "number of hits to skip in sort order (paging)" },
                      { "name": "--json", "help": "machine-readable output — { query, count, total_matched, hits[face,kind,ref,title,comment,at,snippet,matches,standing] }. matches[start,end] are where the words landed in snippet, in its own characters; standing is the record's own state (status, and a task's priority and labels)" }],
            "examples": ["amenbo search plugin distribution --json", "amenbo search AMB-T-<n> --json", "amenbo search rollout --kind decision --json", "amenbo search rollout --kind decision --face comment --json", "amenbo search plugin --kind task --filter \"status:todo\" --limit 5 --json", "amenbo search rollout --kind decision --filter \"status:accepted\" --json"] }),
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
                   "amenbo task add --title \"Fix the sender\" --at amenbo-plugin-mail"])),
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
        json!({ "name": "task show", "summary": "Shows task details — project, classification (dimensions: the axis=value pairs it is filed under, absent when it is filed under none), the folder it is worked in (folder / `at` in --json, absent when it names none), blockers (blocked_by) and dependents (blocks: what finishing this task would unblock). It carries the timeline too: the count of what has been said on the task, the newest three previewed a line each (cut where they run long), and the way to the rest — --json returns every comment whole, under `comments`. It dates the task itself too: `created`, and `updated` once anything has written to it (both RFC3339 UTC, and `created_at` / `updated_at` in --json). **Any** write moves `updated` — a comment, a due date, a title fix — so it is not when the status last moved, and neither stamp is something to judge from (`AMB-D-372`).",
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
                         "amenbo task update AMB-T-<n> --at amenbo-plugin-mail"] }),
        json!({ "name": "task done", "summary": "Marks a task done.",
            "args": [{ "name": "id", "required": true, "help": "task ID" }],
            "flags": [], "examples": ["amenbo task done AMB-T-<n>"] }),
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
        json!({ "name": "task commit add", "summary": "Records a git commit SHA on a task (1 task : many commits) — the anchor from history back to a task, since a public commit carries no store-local reference. Amenbo stores the SHA as an opaque string: it never reads git, verifies the commit, or knows which forge it lives on. The SHA is validated at the door — only full-length lower-case hex is admitted (40 for SHA-1, 64 for SHA-256), case is folded, and short forms, branches, tags and revisions are refused. Idempotent: a SHA already on the task is a no-op (the `(task_id, sha)` index sees bytes only).",
            "args": [{ "name": "task", "required": true, "help": "task ID" },
                     { "name": "sha", "required": true, "help": "the full commit SHA — 40 hex for SHA-1, 64 for SHA-256 (short forms, branches, tags and revisions are refused)" }],
            "flags": [],
            "examples": ["amenbo task commit add AMB-T-<n> 0123456789abcdef0123456789abcdef01234567"] }),
        json!({ "name": "task commit list", "summary": "Lists a task's recorded commit SHAs, oldest first. To go the other way — the task a SHA belongs to — read the commit with `git show <sha>` (Amenbo does not read git).",
            "args": [{ "name": "task", "required": true, "help": "task ID" }],
            "flags": [{ "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo task commit list AMB-T-<n> --json"] }),
        json!({ "name": "task commit rm", "summary": "Forgets a commit SHA on a task — a hard delete (idempotent; the SHA is normalised the way it was stored, so any case removes it). The commit itself and the task are untouched.",
            "args": [{ "name": "task", "required": true, "help": "task ID" },
                     { "name": "sha", "required": true, "help": "the commit SHA to forget (any case — normalised the way it was stored)" }],
            "flags": [{ "name": "--yes", "help": "skip confirmation" }],
            "examples": ["amenbo task commit rm AMB-T-<n> 0123456789abcdef0123456789abcdef01234567 --yes"] }),
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
        json!({ "name": "comment attach", "summary": "Attaches a file (content-addressed `blob`) or external link (--url) to a single TASK comment, kept separate from the parent task's own attachments so a comment's own attachment timeline is preserved. Same two modes as `task attach`, and the same judgement of what is worth attaching — read it there. Identify the comment by id; find ids with `comment list <task> --json`. A decision comment is attached to with `decision comment attach` — the two comment tables number apart, so the command, not the id, says which table an id belongs to. List them with `attach ls --task-comment <id>`.",
            "args": [{ "name": "comment", "required": true, "help": "target task comment ref, AMB-TC-n (from `comment list`)" },
                     { "name": "source", "required": true, "help": "file path to ingest as a blob, or the external URL with --url" }],
            "flags": [{ "name": "--url", "help": "treat <source> as an external URL link instead of ingesting a file" },
                      { "name": "--name <str>", "help": "display label (defaults to the file name / URL; on a file it keeps that file's suffix, and what the file is stays read from the file)" }],
            "examples": ["amenbo comment attach AMB-TC-<n> ./note.png", "amenbo comment attach AMB-TC-<n> https://example.com/spec --url --name spec"] }),
        json!({ "name": "decision comment attach", "summary": "Attaches a file (content-addressed `blob`) or external link (--url) to a single DECISION comment — the mirror of `comment attach` (which takes task comments), down to the judgement of what is worth attaching (`task attach`). Identify the comment by id; find ids with `decision comment list <decision> --json`. List them with `attach ls --decision-comment <id>`.",
            "args": [{ "name": "comment", "required": true, "help": "target decision comment ref, AMB-DC-n (from `decision comment list`)" },
                     { "name": "source", "required": true, "help": "file path to ingest as a blob, or the external URL with --url" }],
            "flags": [{ "name": "--url", "help": "treat <source> as an external URL link instead of ingesting a file" },
                      { "name": "--name <str>", "help": "display label (defaults to the file name / URL; on a file it keeps that file's suffix, and what the file is stays read from the file)" }],
            "examples": ["amenbo decision comment attach AMB-DC-<n> ./benchmark.csv"] }),

        json!({ "name": "decision add", "summary": "Records a new decision (proposed) — an append-only \"why we chose X\". A decision is a Task sibling (project-scoped), NOT a task: it has no mailbox workflow and never appears in task lists. Decisions have their own device-global number space, shown as AMB-D-N (tasks are AMB-T-N); the kind code keeps AMB-D-<n> / AMB-T-<n> unambiguous. The body should be the conclusion + rationale (compress; do not paste raw discussion, and keep PII out). The project defaults to the bound project. Classify it here with --dim: an axis the project marked required refuses the record itself with invalid_decision_required_dimension, naming the axes to fill in — a decision has no second stage to be turned away at, and the one who accepts it is not the one who wrote it, so the demand is put to the writer here rather than to the accepter later. The time axis is the one exception, filled from the era that contains today; a decision left blank on that alone is recorded, and the response names it (`unmet_required_dimensions` in --json).",
            "args": [], "flags": [{ "name": "--title <str>", "required": true, "help": "decision title" },
                                   { "name": "--body <str>", "help": "conclusion + rationale (Markdown). Pass `-` to read it from stdin (a shell eats code spans out of a quoted argument)." },
                                   { "name": "--dim <axis>=<value>", "help": "classify at creation, resolving names as dimension set does; repeatable for different axes, and for one multi axis named several times (a single axis named twice is refused, and so is an axis that does not classify decisions). It saves the create→dimension set round trip, and it is the only road past a required axis: left blank, the record itself is refused" },
                                   { "name": "--project <id>", "help": "project (name or ID; defaults to the bound project — an AI omits it, and naming one is refused)" }],
            "examples": ["amenbo decision add --title \"Adopt UTC for stored timestamps\" --body \"store in UTC and localize on display — removes timezone ambiguity\" --project AMB-P-<n>"] }),
        cmd("decision list", "Lists decisions (status:proposed|accepted|rejected, superseded:yes|no — the edge itself, so superseded:yes lists the decisions another decision draws a supersedes edge at — project:, dim:<axis>=<value> and its time_axis:<value> sugar, which read the same axes the tasks are classified on and fold the same way (different axes AND, the same axis ORs), decided_before:/decided_after: over the day a decision was accepted (YYYY-MM-DD, or today/-30d; both ends inclusive; a decision that was never accepted has no such day and matches neither)). Words are not among the keys — search <word> --kind decision is what finds where they are written. To ask which policies were settled by a date, compose this filter with superseded: — there is no separate as-of switch, and the composition recovers neither status transitions nor deleted decisions. Sort by decided/created/number/title/status (prefix - for descending; default -created). --limit/--offset page (JSON carries total_matched = the count before paging, count = this page). --with-body adds each decision's body to the rows — a projection that composes with --filter/--limit/--offset (narrow by status and page; it does not dump the whole corpus).",
            json!([{ "name": "--project <id>", "help": "limit to the given project (human only)" },
                   { "name": "--filter <expr>", "help": "e.g. status:accepted superseded:no dim:Theme=main" },
                   { "name": "--sort <key>", "help": "decided/created/number/title/status (- for descending; default -created)" },
                   { "name": "--limit <n>", "help": "max count (in sort order; pairs with --offset for paging)" },
                   { "name": "--offset <n>", "help": "number of items to skip in sort order (paging)" },
                   { "name": "--with-body", "help": "include each decision's body (projection; composes with --filter/--limit/--offset)" },
                   { "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo decision list --filter status:accepted --json", "amenbo decision list --sort -decided", "amenbo decision list --filter \"status:accepted superseded:no\" --with-body --limit 20 --json"])),
        json!({ "name": "decision show", "summary": "Shows a decision: body, status, the supersession chain (both directions), the premises it builds on (read those first — a premise another decision has overturned is flagged, because this decision then stands on rotten ground) and the decisions that build on it (the impact radius: overturn this one and they want revisiting), and its linked tasks. It dates itself as well — when it was recorded, when it was settled and by which facet, and when it last changed where that is neither of the two — so how fresh the record is, and whose ruling it is, come off the page rather than being guessed at. Its timeline comes with it, in the same shape task show gives its own: the count of what has been said, the newest three previewed a line each, and the way to the rest — which is where the reason an acceptance or a rejection was given lives, since --reason writes one as a comment (--json returns every comment whole, under `comments`).",
            "args": [{ "name": "id", "required": true, "help": "decision ref (AMB-D-n)" }],
            "flags": [{ "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo decision show AMB-D-<n> --json"] }),
        json!({ "name": "decision edit", "summary": "Edits a decision's title/body in place — proposed or accepted alike. Editing is not re-deciding, so an accepted decision's decided_at/decided_by are left untouched, and there is no revision history. Supersede when a new decision replaces it; a rejected decision is terminal and cannot be edited.",
            "args": [{ "name": "id", "required": true, "help": "decision ref (AMB-D-n)" }],
            "flags": [{ "name": "--title <str>", "help": "new title" }, { "name": "--body <str>", "help": "new body (Markdown). Pass `-` to read it from stdin (a shell eats code spans out of a quoted argument)." }],
            "examples": ["amenbo decision edit AMB-D-<n> --body \"…refined rationale…\""] }),
        json!({ "name": "decision accept", "summary": "Accepts a decision (proposed → accepted); stamps decided_at/decided_by. It is where a classification the project requires of decisions is read (the axes whose applies_to covers decisions): an axis left blank holds the acceptance and is named in the refusal. --reason records the reason for accepting as a decision comment — the reason lives on the timeline, not in a dedicated field.",
            "args": [{ "name": "id", "required": true, "help": "decision ref (AMB-D-n)" }],
            "flags": [{ "name": "--reason <str>", "help": "reason for accepting (recorded as a decision comment). Pass `-` to read it from stdin (a shell eats code spans out of a quoted argument)." }],
            "examples": ["amenbo decision accept AMB-D-<n>", "amenbo decision accept AMB-D-<n> --reason \"agreed after the perf review\""] }),
        json!({ "name": "decision reject", "summary": "Rejects a decision (proposed → rejected). --reason records the reason for rejecting as a decision comment — the reason lives on the timeline, not in a dedicated field.",
            "args": [{ "name": "id", "required": true, "help": "decision ref (AMB-D-n)" }],
            "flags": [{ "name": "--reason <str>", "help": "reason for rejecting (recorded as a decision comment). Pass `-` to read it from stdin (a shell eats code spans out of a quoted argument)." }],
            "examples": ["amenbo decision reject AMB-D-<n>", "amenbo decision reject AMB-D-<n> --reason \"superseded by the simpler approach in AMB-D-<m>\""] }),
        json!({ "name": "decision reopen", "summary": "Un-settles an accepted decision back to proposed (accepted → proposed; clears decided_at/decided_by), and sends the tasks that rest on it back to ready:no. Use it to pull a too-hastily accepted decision back into debate — neither reject (a verdict) nor supersede (a replacement) says that. It is not needed to edit: an accepted decision edits in place. No-op if already proposed; refused for rejected decisions. A decision another one supersedes stays accepted (currency is derived, not a status), so it can be reopened too.",
            "args": [{ "name": "id", "required": true, "help": "decision ref (AMB-D-n)" }],
            "flags": [], "examples": ["amenbo decision reopen AMB-D-<n>"] }),
        json!({ "name": "decision delete", "summary": "Deletes (retires) a decision — accepted ones included. The delete is physical and irreversible, its comments and edges go with it, and linked tasks are unlinked. Use this to retire a decision outright; use supersede when a new decision replaces it (which keeps the old one readable).",
            "args": [{ "name": "id", "required": true, "help": "decision ref (AMB-D-n)" }],
            "flags": [{ "name": "--yes", "help": "skip the confirmation prompt" }],
            "examples": ["amenbo decision delete AMB-D-<n> --yes"] }),
        json!({ "name": "decision supersede", "summary": "Records that a new decision replaces an existing one (supersession chain): the new one is accepted and draws a `supersedes` edge at the old one, which stops being current (the old row itself is not touched — currency is derived from the edge, not stored). A decision may supersede several others — each supersede draws its own edge, none replaces the last. Promoting the new side is settling it, so it reads the same required-classification door `decision accept` does; drawing the edge over an already-accepted side settles nothing and is not asked.",
            "args": [{ "name": "decision", "required": true, "help": "the new decision (it replaces the old one)" }],
            "flags": [{ "name": "--replaces <id>", "required": true, "help": "the decision being replaced" }],
            "examples": ["amenbo decision supersede AMB-D-<n> --replaces AMB-D-<m>"] }),
        json!({ "name": "decision amend", "summary": "Records that a new decision amends (partially revises) an existing one: the new one draws an `amends` edge at the old one, which stays current (not superseded) — read the two together. A decision may amend several others (one edge each). Amend only records the revision link; it does not change either side's status (the amending side stays proposed until you accept it separately). Use supersede when the new decision fully replaces the old one.",
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
        json!({ "name": "decision promote", "summary": "Promotes a comment into a decision: the comment text becomes the body, and the project defaults to the project of what the comment sits on. What is drawn afterwards differs by kind. A task comment (AMB-TC-n) links the new decision to that task — the decision is that task's premise. A decision comment (AMB-DC-n) draws no edge: a record raised out of a decision's thread is a question that turned into its own, and a link would claim a relation this cannot know — where one holds, name it yourself with builds-on / amend / supersede. The two tables number independently, so a bare <n> naming a row in each is refused: spell the kind code. Classify it here with --dim, the same as `decision add`: an axis the project marked required refuses the record itself (`invalid_decision_required_dimension`), so nothing is left behind to classify by hand. The time axis is not asked for — the create fills it with the era containing today.",
            "args": [{ "name": "comment", "required": true, "help": "the comment ref to promote, AMB-TC-n (on a task) or AMB-DC-n (on a decision)" }],
            "flags": [{ "name": "--title <str>", "required": true, "help": "decision title" },
                      { "name": "--project <id>", "help": "project (defaults to the project of the comment's task or decision)" },
                      { "name": "--dim <AXIS=VALUE>", "help": "classify the new decision (repeatable for different axes; refused on an axis named twice or one that does not classify decisions)" }],
            "examples": ["amenbo decision promote AMB-TC-<n> --title \"Standardize on ISO-8601 dates\"", "amenbo decision promote AMB-DC-<n> --title \"Standardize on ISO-8601 dates\" --dim \"Theme=main\""] }),
        json!({ "name": "decision comment rm", "summary": "Deletes a comment posted by mistake — permanently, and its attachments go with it. Identify the comment by id; `decision comment list` prints it.",
            "args": [{ "name": "comment", "required": true, "help": "target decision comment ref, AMB-DC-n (from `decision comment list`)" }],
            "flags": [{ "name": "--yes", "help": "skip confirmation" }],
            "examples": ["amenbo decision comment rm AMB-DC-<n> --yes"] }),
        json!({ "name": "decision comment edit", "summary": "Rewrites a comment's body in place — the id, its place on the timeline, and its attachments all stay. This edits a comment, not the decision's own body (conclusion + rationale); the two are separate. Identify the comment by id; `decision comment list` prints it.",
            "args": [{ "name": "comment", "required": true, "help": "target decision comment ref, AMB-DC-n (from `decision comment list`)" }],
            "flags": [{ "name": "--text <str>", "required": true, "help": "the new body, as Markdown — it replaces the old one outright. Pass `-` to read it from stdin (a shell eats code spans out of a quoted argument)." }],
            "examples": ["amenbo decision comment edit AMB-DC-<n> --text \"Corrected: the benchmark was 10k, not 1k\""] }),
        json!({ "name": "decision comment add", "summary": "Adds a comment to a decision's timeline. The decision's own body (conclusion + rationale) holds what was decided; comments are the way to discuss it or record accept/reject reasoning (`decision accept/reject --reason` is thin sugar over this).",
            "args": [{ "name": "decision", "required": true, "help": "target decision ref (AMB-D-n)" }],
            "flags": [{ "name": "--text <str>", "required": true, "help": "comment body (Markdown). Pass `-` to read it from stdin (a shell eats code spans out of a quoted argument)." }],
            "examples": ["amenbo decision comment add AMB-D-<n> --text \"revisited after the 10k benchmark — still holds\""] }),
        json!({ "name": "decision comment list", "summary": "Shows a decision's comments, oldest first. --limit/--offset page (JSON carries total_matched = the count before paging, count = this page).",
            "args": [{ "name": "decision", "required": true, "help": "target decision ref (AMB-D-n)" }],
            "flags": [{ "name": "--limit <n>", "help": "max count (oldest first; pairs with --offset for paging)" },
                      { "name": "--offset <n>", "help": "number of items to skip, oldest first (paging)" },
                      { "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo decision comment list AMB-D-<n> --json"] }),

        json!({ "name": "task attach", "summary": "Attaches a file or external link to a task. WHAT TO ATTACH: an attachment's bytes are not searchable — `search` reaches an attachment's filename and its link address, never what is inside it — so text is always the body's job, and attaching is giving up on ever finding it again. The test: if the content holds a word you might one day search for, that word stays in the body. An attachment is not where words go to disappear; it is where the backing evidence sits. Attach non-text you generated yourself — the screenshot of a GUI check (so the next session can see what you verified instead of taking your word for it), images, video, PDF, binary samples; they could never be searched anyway, so nothing is lost, but write one line in the body saying what is in it. Attach long raw data behind a conclusion — a failing run's log, a profile, a before/after benchmark: keep the conclusion and the fragments worth searching (the error line, the identifier) in the body, and attach only the raw data. Do not attach text that fits in the body (short output, a minimal repro), source or diffs (anchor those with `task commit add`), or reasoning and history (that is a decision record). A URL belongs in the body; `--url` adds a click path for the human GUI, it does not replace writing the link down. MECHANICS: a file is ingested as a content-addressed `blob` (the bytes are copied into the store keyed by their BLAKE3 digest; the truth source records only metadata — hash/filename/mime/size); --url instead records an external link (`url` mode, not managed). MIME is guessed from the file extension. The blob is checked against the per-file size cap before ingest. Manage attachments with `attach ls/show/open/rm`.",
            "args": [{ "name": "id", "required": true, "help": "target task ref (AMB-T-n)" },
                     { "name": "source", "required": true, "help": "file path to ingest, or the external URL with --url" }],
            "flags": [{ "name": "--url", "help": "treat <source> as an external URL link instead of ingesting a file" },
                      { "name": "--name <str>", "help": "display label (defaults to the file name / URL; on a file it keeps that file's suffix, and what the file is stays read from the file)" }],
            "examples": ["amenbo task attach AMB-T-<n> ./design.png", "amenbo task attach AMB-T-<n> https://example.com/spec --url --name spec"] }),
        json!({ "name": "decision attach", "summary": "Attaches a file (content-addressed `blob`) or external link (--url) to a decision record. Same two modes as `task attach`, and the same judgement of what is worth attaching — read it there. Manage with `attach ls/show/open/rm`.",
            "args": [{ "name": "id", "required": true, "help": "target decision ref (AMB-D-n)" },
                     { "name": "source", "required": true, "help": "file path to ingest, or the external URL with --url" }],
            "flags": [{ "name": "--url", "help": "treat <source> as an external URL link instead of ingesting a file" },
                      { "name": "--name <str>", "help": "display label (defaults to the file name / URL; on a file it keeps that file's suffix, and what the file is stays read from the file)" }],
            "examples": ["amenbo decision attach AMB-D-<n> ./benchmark.csv"] }),
        cmd("attach ls", "Lists the attachments on a task, decision, or a single comment, in attach order. WHETHER TO OPEN ONE: decide from the metadata this prints — name, mime, size — and from what the body already told you. The body carries the conclusion and the searchable words (see `task attach`); an attachment is the backing evidence behind them, so most of the time the listing alone answers your question and reading the bytes only spends context. Open one when you actually need the evidence — the body's claim is what you must check, or the raw data is what you were sent for — and when in doubt, do not: an attachment you read and did not need has cost you the very context it was put there to save. To read one, `attach save --out <path>` writes the bytes to a file you can open (`attach open` hands it to the OS's default opener, which is the human's route, not yours). A comment is named by a flag, not by the positional target: the task and decision comment tables number apart, so a bare id cannot say which table it belongs to.",
            json!([{ "name": "target", "help": "task / decision ref (AMB-T-n / AMB-D-n — the kind code is what disjoins the two spaces)" },
                   { "name": "--task-comment <id>", "help": "list this task comment's attachments (id from `comment list`)" },
                   { "name": "--decision-comment <id>", "help": "list this decision comment's attachments (id from `decision comment list`)" }]),
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

        cmd("export", "Exports all data — everything on this device, as JSON, and nothing narrower: export exists for moving to another tool, which an excerpt or a human-readable table does not serve. The core of data sovereignty, and one way: Amenbo writes your data out for whatever you move to next, and reads nothing back in — the way back is `restore` from a `backup` archive. `--out <dir>` writes an **export directory**: `export.json` plus `attachments/`, holding every attachment's actual file under the task or decision it hangs on (each row names its `export_path`). With no `--out` the same JSON streams to stdout — a stream has nowhere to put the files, so that shape carries records only. A plugin's secrets are the one thing left behind (`AMB-D-434`): this file goes out to another tool and stays in its hands, and a credential in the clear is not something to hand over on the way past — they ride `backup` instead.",
            json!([{ "name": "--out <path>", "help": "the export directory to create (must not exist yet). Default: stream to stdout" }]),
            json!(["amenbo export --out ./amenbo-export",
                   "amenbo export > ./amenbo-export.json"])),
        cmd("sync version", "Answers what version this window is at — one number, and the question a carrier plugin asks often. It moves whenever something inside the window is written and stays put when nothing is, so a carrier that remembers the last number it sent knows in one cheap call whether there is anything to send; no snapshot is built to answer it. Compare it for inequality, not for order: a `restore` winds the store back and the version comes back with it, so lower still means changed. A window nothing has written since this build began stamping answers 0. Through a project plugin's window it is that one project's version, so churn in another project sends nobody re-reading; for a human, or a plugin that reaches the device, it is the whole device's. This is a carrier's question and not a way to watch for work — for that, read `activity --since`.",
            json!([{ "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo sync version", "amenbo sync version --json"])),
        cmd("sync snapshot", "Takes one whole picture of what this window may see and writes it to stdout as JSON — the payload a carrier plugin sends outward. Every table is read from **one instant**, so nothing in it refers to something that is not in it, and a join row travels only when both of its ends are inside the window; a plugin's secrets stay home (`AMB-D-434`). Records only: an attachment's row travels and names the bytes it stands for, but the bytes stay on this device. The shape is the export's — `{\"amenbo_sync\": …, \"tables\": {…}}` — so whatever reads one reads the other, and the road is one-way: nothing reads a snapshot back in. The header also names **where in the ledger the picture stands** (`cursor`), read from that same instant, so a carrier reads the changes on from there instead of having to guess where the whole ended. **Not a way to read the work.** The whole window lands on stdout at once, so this is for sending, not for looking — `task list` and `task show` are what read.",
            json!([{ "name": "--json", "help": "the snapshot is JSON either way; this silences the note on stderr" }]),
            json!(["amenbo sync snapshot > ./window.json"])),
        cmd("sync changes", "Reads what has changed in this window since a cursor — the unread, oldest first, and the cursor to come back with. This is the road a carrier walks when it already holds a copy: instead of re-sending the window, it re-reads only the records that moved. What comes back is which record moved and how (`insert` / `update` / `delete`), never what it now holds — the ledger carries no values (`AMB-D-367`), so read a changed record back by name and drop a deleted one. `delete` is what makes the road work at all: there is nothing left to read back, and noticing by re-reading everything you hold would mean asking after the whole window on every pass. Start from the `cursor` a `sync snapshot` names in its header; every answer names the next one. A page is bounded and says `more` when it cut one short — come straight back with the cursor it handed you. When the cursor has fallen out of the ledger's window the answer is a gap (`sync_gap`, non-zero exit) rather than an empty page that would read as \"nothing changed\": take a fresh `sync snapshot` and read on from the cursor it names (`AMB-D-583`).",
            json!([
                { "name": "--since <cursor>", "help": "read on from this cursor — everything after it, exclusive (required)" },
                { "name": "--json", "help": "machine-readable output" },
            ]),
            json!(["amenbo sync changes --since 0 --json", "amenbo sync changes --since 4821 --json"])),
        cmd("sync records", "Reads the records `sync changes` named back — the rows themselves, by id, in the shape a `sync snapshot` carries them. This is where the ledger leads: it says which record moved and never what it now holds, so without this the only way to see a changed task is to take the whole window again, which is the ledger's whole point undone. `--dataset` is the name the changes gave (`task`, `task_dependency`, …) and `--ids` are that answer's `record_id`s. What comes back is `{\"amenbo_sync\": …, \"tables\": {\"<table>\": [rows]}}` — the snapshot's document with one table in it, so whatever reads a snapshot reads this. The rows are the raw stored ones, not the assembled cards `task show` builds, and the window is the same one the snapshot closes to: an id it does not reach simply is not there. Nor is an id that has been deleted — the `delete` in the changes already said which is which, so a gap in the answer is never a surprise. One read answers at most a page of changes' worth of ids (500); past that it is refused rather than cut short, so ask in pages.",
            json!([
                { "name": "--dataset <name>", "help": "which records — the dataset name `sync changes` gave (required)" },
                { "name": "--ids <ids>", "help": "the record ids to read back, comma-separated (required)" },
            ]),
            json!(["amenbo sync records --dataset task --ids 12,15,31", "amenbo sync records --dataset task_dependency --ids 4"])),
        json!({ "name": "backup", "summary": "Backs up everything on this device — one database, holding every project — into one verified `.amenbo-backup` archive at the given path (VACUUM INTO: checkpointed, transactionally consistent, no torn DB+WAL; bounded-verified; the manifest records its migration generation). The attachment bytes (blobs) are bundled too, so a restore elsewhere brings the files back and not just the rows referencing them. The device's own secrets (at-rest key / identity) are not part of the engine, so none are included; a plugin's secrets are store rows, so those do ride along and come back working (`AMB-D-434`). The destination must not already exist (managed generation rotation is retired).",
            "args": [{ "name": "path", "required": false, "help": "destination .amenbo-backup archive that must not already exist" }],
            "flags": [{ "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo backup ./everything.amenbo-backup"] }),
        json!({ "name": "restore", "summary": "Restores this device from a verified `.amenbo-backup` archive at the given path — a destructive replace of the database the archive carries (all-or-nothing stage-and-swap; the replaced truth source is set aside with a timestamp; an archive newer than this build is refused — update first). It is the one command that runs on a store this build cannot open, because it replaces the truth source instead of reading it — which is what makes the pre-migration backup a real way back from a store a newer Amenbo carried past this build (there is no downgrade). The snapshot is validated before anything is swapped in, so an unusable archive is refused without harm. The archive's attachment bytes (blobs) are placed additively — a blob the machine already holds is left alone, and none are ever deleted. An archive written before the consolidation carries the pre-consolidation shape (a list of stores) and is refused whole by its layout version, before its manifest is even parsed, rather than partially applied: restore it with the build that wrote it. Destructive — confirms unless --yes.",
            "args": [{ "name": "path", "required": false, "help": "the .amenbo-backup archive to restore from (must exist and pass verification)" }],
            "flags": [{ "name": "--json", "help": "machine-readable output" }, { "name": "--yes/-y", "help": "skip confirmation" }],
            "examples": ["amenbo restore ./everything.amenbo-backup --yes"] }),
        json!({ "name": "hard-erase comment", "summary": "Physically erases one or more task comments from this store's truth source — deletes the read-model row outright — then VACUUMs so the bytes leave the file (unrecoverable). An ordinary delete removes the row — and `comment rm` deletes a comment posted by mistake — but the freed pages keep their bytes readable until something reclaims them, so this is the deliberate, gated exception: use it for content that must be GONE from the file. Identify comments by id; find ids with `comment list <task> --json`. Human-gated maintenance: takes a safety backup first (a `pre-erase-*.amenbo-backup` archive next to the store, which `restore` puts the store back from — only the newest is kept), confirms unless --yes, and is refused for the AI actor (a human must run it). The safety backup still holds the erased content — delete it after verifying.",
            "args": [{ "name": "ids", "required": true, "help": "task comment ref(s) to erase, AMB-TC-n" }],
            "flags": [{ "name": "--yes/-y", "help": "skip confirmation" }, { "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo hard-erase comment AMB-TC-<n> --yes"] }),
        json!({ "name": "hard-erase decision-comment", "summary": "Physically erases one or more decision comments — the same surgery `hard-erase comment` performs on the task side, on the other comment table. It is a separate command rather than a flag because the two tables number independently: a bare id belongs to whichever table the command names, and an erase that guessed would destroy the wrong row. The comment's row goes outright (a comment's number is not a conversational one, so nothing is left pointing at it) along with the bytes of any file attached to it, then a VACUUM takes the freed pages out of the file. Find ids with `decision comment list <decision> --json`. Human-gated maintenance, on the same footing as the task side: a safety backup first, confirms unless --yes, and refused for the AI actor. The safety backup still holds the erased content — delete it after verifying.",
            "args": [{ "name": "ids", "required": true, "help": "decision comment ref(s) to erase, AMB-DC-n" }],
            "flags": [{ "name": "--yes/-y", "help": "skip confirmation" }, { "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo hard-erase decision-comment AMB-DC-<n> --yes"] }),
        json!({ "name": "hard-erase decision", "summary": "Redacts an accepted decision's body: overwrites it with the given text in place (the prior body is physically replaced, not merely superseded), then VACUUMs — so one section can be removed while the decision keeps its number, links and other fields. The replacement body comes from --body, --body-file, or stdin. Destructive maintenance: takes a safety backup first (a `pre-erase-*.amenbo-backup` archive next to the store, which `restore` puts the store back from — only the newest is kept), confirms unless --yes, and is refused for the AI actor (a human must run it). The safety backup still holds the old body — delete it after verifying.",
            "args": [{ "name": "id", "required": true, "help": "decision reference (AMB-D-n)" }],
            "flags": [{ "name": "--body <text>", "help": "replacement body (Markdown); omit to use --body-file or stdin" }, { "name": "--body-file <path>", "help": "read the replacement body from this file instead of --body/stdin" }, { "name": "--yes/-y", "help": "skip confirmation" }, { "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo hard-erase decision AMB-D-<n> --body-file ./redacted.md --yes"] }),
        json!({ "name": "plugin validate", "summary": "Validates a plugin manifest file against the catalog rules — a well-formed id, repo, non-empty OS set and config schema, plus a distributable in one of the two forms an entry may take (one https url and checksum for every OS it lists, or one per OS, whose platforms must be exactly the ones declared) — reporting every problem it finds so an author can self-check before opening a catalog PR. It reads the same rules Amenbo enforces at the install/intake door, so the two never disagree. The path may be .yaml (the form authored in the catalog repo) or .json (the aggregated catalog.json form); the format is taken from the extension, defaulting to YAML. A manifest that does not even parse is reported too — a missing required field is the shape half of the fail-closed door. It opens no store and needs no binding, so it runs anywhere (a plugin checkout, CI). On --json a passing manifest also carries what Amenbo read, as the two documents the catalog serves: the 'entry' everyone fetches to draw the list, and the 'detail' fetched only for the plugin being opened or installed, which is where the signature and checksums live. A consumer such as the catalog aggregator therefore publishes what Amenbo hands it, keeping neither its own list of which fields to copy — a list that silently drops a field Amenbo later adds — nor its own idea of which half each field belongs in. The entry carries added_at and detail_sum as empty slots for the catalog to fill, neither being knowable from a manifest. A manifest that does not pass carries neither document. The translations an author wrote beside the manifest are read with it: every sibling <name>.<lang>.<ext> is an overlay of it, refused unless the language is one Amenbo is read in, everything it names exists in the base, and its text stays within the caps its base fields obey. They come back split the same way, each half following its base fields — 'entry_i18n' is the list half, one document per language for the catalog to publish as catalog.<lang>.json, and the detail half rides inside 'detail' with every language at once. The exit code is the verdict: 0 valid, 1 invalid (or the file could not be read).",
            "args": [{ "name": "path", "required": true, "help": "path to the manifest file (.yaml or .json)" }],
            "flags": [{ "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo plugin validate plugins/worktree.yaml", "amenbo plugin validate ./manifest.json --json"] }),
        json!({ "name": "plugin list", "summary": "Lists the plugins installed on this machine — name, description and the official badge — beside whose gate is open. The two facts sit together because installing a plugin never runs it: an installed plugin that fires nothing is the ordinary state, not a fault. Each plugin has exactly one switch, and which layer it sits at is its author's declaration: a plugin declaring the project layer has one switch per project, so every row names the projects holding it open rather than answering yes or no from wherever the terminal happens to stand — a plugin still firing somewhere else cannot be hidden by where you ran this, and an empty list is itself an answer, off everywhere. One declaring the machine layer has a single gate for the device instead, and its row says so rather than naming projects it does not answer for (--json carries enabled_projects, each with its id, ref and name, beside the declared scope and whether the device gate is open). Under an AI's reach the row names its own project alone, the way every listing is narrowed, and the wording says as much instead of claiming 'everywhere' over projects it was not shown. An open gate is not the same as a plugin that fires, so each row carries whether this Amenbo can speak to it at all: a plugin whose declared payload contract or minimum Amenbo version this build does not meet is skipped at dispatch, and since Amenbo updates underneath an install, one enabled while it was compatible can stop firing with nobody having touched it — the listing names the mismatch rather than leaving it to the log (--json carries compatible and the reason). Whether a newer build is out is a third fact each row can carry: when the last-fetched catalog holds a different build of an install it is marked 'update available', read from the catalog cached beside the installs so the listing stays offline — refreshing the catalog and putting the build in place are the explicit plugin update --check / plugin update (--json carries update_available). Reads only the app-data plugins/ directory — the installs and the catalog cached beside them — and the store's gate rows — no network, no catalog fetch — so it answers the same offline. A directory it cannot read as an install is skipped rather than allowed to hide the rest. --json adds each plugin's subscribed events and the path of the executable Amenbo would run.",
            "flags": [{ "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo plugin list", "amenbo plugin list --json"] }),
        json!({ "name": "plugin log", "summary": "Reads the plugin execution log: the last runs of each plugin, newest first, narrowed to one when you name it. A hook is fire-and-forget — nobody waits on it, and nothing fails when it fails — so this is the only place that answers 'my plugin did nothing, why'. One line per run: when it ran, which plugin, on which event, how it ended (ok / failed / timed_out / not_launched), its exit code and how long it took. A run that did not end cleanly is followed by what the plugin wrote to stderr, which is where its author put the diagnosis; --json carries that text for every run, clean ones included. A gap line is not a run at all — it marks events that reached nobody because retention trimmed them away before the dispatcher read them, and it names no plugin because what was lost was never resolved to one. A name with nothing on file reports an empty log rather than an error. Under the cursor it shows one `waiting` line per plugin that still owes something: how many events are on its queue, since when, and whether a runner is on it. That is the half the runs cannot show, because a plugin that never ran wrote no line — a queue piling up with nobody running it is a plugin that stopped, one piling up with a live runner is a plugin taking its time, and the two want opposite responses. Nothing is printed when nothing is waiting. Reads one machine-local file and a few store rows, and no network — nothing here leaves this device (the log itself is outside every backup and export). It is bounded by construction — the last runs of each installed plugin, each with a capped slice of stderr — so there is no window to ask for and no deeper history to page: a longer one is a logging plugin's business, not Amenbo's. No secret can appear in it, structurally: the log is never handed a plugin's environment, so there is no field one could ride in.",
            "args": [{ "name": "name", "required": false, "help": "narrow to one plugin's runs; omit for every plugin's, newest first" }],
            "flags": [{ "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo plugin log", "amenbo plugin log slack --json"] }),
        json!({ "name": "plugin flush", "summary": "Delivers what is waiting on the plugins' queues now, in this process, and reports what each one got through. Delivery otherwise rides along with whatever you were doing — a write fans its events out onto the queues and starts a runner per queue, and no command is ever made to wait for a plugin — so a runner killed mid-queue leaves its rows standing until the next write, which may be days away. This is the door for pushing them through on purpose. Because the queues are worked here rather than by a process nobody watches, it returns only once they are empty, and can say per plugin how many events came off its queue and how many are still on it. A queue a live runner already holds is left to that runner and named as such: one queue is worked by one runner, which is what its lease is for. Nothing waiting is not an error — it says so and exits 0. How each delivery *ended* is not here: a failed one is dropped rather than retried, and `plugin log` is where every run's outcome and the plugin's own diagnosis are written. This says what moved; the log says how it went.",
            "flags": [{ "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo plugin flush", "amenbo plugin flush --json"] }),
        json!({ "name": "plugin install", "summary": "Installs a plugin from the catalogs: resolves the name across the official catalog and every catalog you registered (each fetched fresh when the network answers, its cached copy when it does not; the official one wins a name clash), downloads the asset its manifest points at, verifies it fail-closed, and lays it down under the app-data plugins/ directory. Verification is the whole point of the door: the asset's minisign signature against the key the catalog that served it answers for — Amenbo's own for the official index, the key pinned when that catalog was registered — then the manifest's checksum over the exact bytes served (integrity). Unsigned, signed by any other key, or a digest that does not match, and nothing is written; a registered catalog that publishes no key has no key to check against, so nothing installs from it at all. Installing never enables: the plugin lands inert and `plugin enable` is the separate, explicit act, which is also where compatibility with this build is judged. A name already installed is refused rather than overwritten, and so is a broken install in the way (uninstall it first) — a home left by an install that did not finish is not one, so a retry goes straight through. An OS the manifest does not list is refused too — a platform the entry never claimed has no build behind it — and the asset fetched is the one published for the OS running the install, since an entry may carry a separate distributable per platform. The asset may be a gzip'd tar holding an entry named after the plugin, or the executable itself; a zip is refused by name. The only command in this group that touches the network.",
            "args": [{ "name": "name", "required": true, "help": "the plugin's name, as the catalog lists it" }],
            "flags": [{ "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo plugin install worktree", "amenbo plugin install worktree --json"] }),
        json!({ "name": "plugin update", "summary": "Brings an installed plugin onto the build the catalog publishes — or, with --check, only reports which installs it has moved past. Detection is the catalog Amenbo already fetches whole laid beside the manifest that sits next to each installed binary — no central server, no per-plugin request. A manifest carries no version number, so what is compared is detail_sum: one digest per catalog entry, over the whole document an install acts on — the assets, the config schema, the compatibility floor, and what the plugin says for itself at the AI's entry point. That is why an update which changes no binary is still an update: comparing executables would hide every one of them. The asset checksums are still checked where they mean something, at the install door — the bytes that arrive must be the bytes the entry published. It therefore reports different, not newer: a catalog that rolls an entry back offers that older build, because the catalog is the authority on what is published. A plugin the catalog does not list is passed over rather than reported (installed by hand, or delisted). The three jobs are kept distinct so a safe report and a replacing apply are never a typo apart: --check reports and applies nothing, a name applies one, --all applies every one; a bare `plugin update` with none of them is refused rather than guessed at. Nothing is ever applied on Amenbo's own account — naming a plugin, or --all, is the whole consent. Applying re-walks the install door over the new asset (the catalog signature, then this OS's checksum), retains the build it replaced as a .bak pair so `plugin rollback` has somewhere to go, and keeps the plugin's gate and every setting the new build still declares — an update is not a re-install, and wiping a plugin's settings wholesale is uninstall's job. What it does take is a value stored under a key the new build has stopped declaring: nothing would read it again, and a rollback does not bring it back. Any step that refuses (a build this Amenbo cannot speak to, an asset that will not verify) leaves the working plugin exactly as it was; with --all one plugin's failure is reported and the rest are still applied. --check is cheap on purpose: with nothing installed no catalog is read at all, and otherwise a cached catalog younger than an hour answers with no request — which is what lets a check ride along with something you were doing anyway. It says which of the two it did, so a cached answer is never read as a current one, and --fresh fetches the index first when that distance matters. Applying always asks for the current index, since replacing a binary on an hour-old answer is not the same bargain.",
            "args": [{ "name": "name", "required": false, "help": "the installed plugin to update; omit it with --all or --check" }],
            "flags": [{ "name": "--check", "help": "report what has an update without applying anything" }, { "name": "--all", "help": "apply every update the catalog holds, one plugin at a time" }, { "name": "--fresh", "help": "with --check: fetch the catalog now instead of letting a cache under an hour old answer" }, { "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo plugin update --check", "amenbo plugin update worktree", "amenbo plugin update --all"] }),
        json!({ "name": "plugin rollback", "summary": "Undoes the last `plugin update` for one plugin, restoring the build it retained. An update kept the previous executable and its manifest as a .bak pair beside the new ones; this puts both back — the pair, never one without the other, so the installed manifest never disagrees with the bytes beside it. Offline and instant: nothing is fetched and nothing is re-verified, because the retained build already passed the door on its way in and a rollback is a deliberate return to it (the same shape self-update's `update --rollback` takes). It leaves the gate, the settings and the secrets alone — a value the update purged for want of a declaration included, which stays gone: what is retained is one generation of the build and its manifest. Goes back one build, and only one: the retained copy is consumed, so a second rollback has nothing to restore and says so. Refused, changing nothing, when the plugin is not installed or was never updated (there is no retained build to return to).",
            "args": [{ "name": "name", "required": true, "help": "the installed plugin to roll back" }],
            "flags": [{ "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo plugin rollback worktree"] }),
        json!({ "name": "plugin enable", "summary": "Enables an installed plugin: opens the one gate it fires through, which is the gate of the project you are in — so it needs a bound folder, and turning it on elsewhere is a separate act. That is why there is no --scope: a plugin has one switch, and a user is never shown two. Installing puts a plugin on disk and nothing more; this is the step that lets it run, and doing it is itself the permission to run somebody else's code, so nothing is asked beside it and nothing is kept beside the row — which is what lets a backup carry the answer with it. Fail-closed on the settings the plugin's author marked required — while one is empty the enable is refused and the empty fields are named; fill them with `plugin config set` and enable again. Amenbo checks only that a value is present in that project; whether the value is *meaningful* is the plugin author's to judge at run time. Fail-closed on compatibility too: a plugin whose manifest reads a different event-payload contract than this Amenbo speaks, or needs an Amenbo newer than the one running, is refused with both versions named — update Amenbo (or the plugin) rather than run one against a payload it cannot read.",
            "args": [{ "name": "name", "required": true, "help": "the installed plugin's name" }],
            "flags": [{ "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo plugin enable worktree", "amenbo plugin enable slack"] }),
        json!({ "name": "plugin disable", "summary": "Closes a plugin's gate — the same single switch `enable` opens, in the project you are in, so there is no --scope here either. It stops firing while staying installed, so enabling it again later costs nothing. Deliberately does not require the plugin to still read as installed: this is how a plugin is stopped, and a half-broken install is exactly when stopping it matters most — nothing here is read off the manifest, so a file that will not parse cannot leave a gate open. Disabling one that is already off changes nothing and says so.",
            "args": [{ "name": "name", "required": true, "help": "the plugin's name" }],
            "flags": [{ "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo plugin disable worktree", "amenbo plugin disable slack"] }),
        json!({ "name": "plugin uninstall", "summary": "Removes a plugin and everything it left behind: the binary and its directory, its settings in every project on this device, and its secrets. Disabling stops a plugin while keeping all of that — this is the other end, and the difference is the point: a re-install of the same name starts clean, inheriting no setting. It works from the name alone and never asks whether the plugin still reads as installed, so it is also how a half-broken install is cleaned up; a name that holds nothing is reported as such, not an error. The steps run worst-residue-first — the gates, then the secrets, then the settings, then the binary — so an interrupted removal leaves an inert directory and never a plugin that still fires. Confirms unless --yes.",
            "args": [{ "name": "name", "required": true, "help": "the plugin's name" }],
            "flags": [{ "name": "--yes/-y", "help": "skip confirmation" }, { "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo plugin uninstall worktree --yes"] }),
        json!({ "name": "plugin run", "summary": "Calls an installed, enabled plugin's command face and hands you what it returned. A plugin has two faces: the observation hook fires by itself on an event and nobody waits for it, while this one you call on purpose and get an answer. The answer is the plugin's stdout, relayed to this command's stdout verbatim and with nothing of Amenbo's mixed in — which is what lets a plugin return something a shell consumes directly, as in eval \"$(amenbo plugin run worktree start 123)\" — or iex (amenbo plugin run worktree start 123) in PowerShell, since the line a plugin returns is written to go through either shell. Its stderr is the human-facing diagnostic and is relayed to stderr, before the value, whether the call succeeded or not. Everything after the plugin's name is the plugin's own: Amenbo passes the words through untouched and never parses them, dashes included, because what they mean is the plugin's business — so Amenbo's own flags have to come before the plugin's name (amenbo plugin run --json worktree ...), not after it. A plugin that exits non-zero is a failed call — its return value is discarded rather than handed on, and this exits 1 with the plugin's own exit code named in the message, not impersonated. Refused, with the reason, when the plugin is not installed, is installed but not enabled (installing never runs anything), or is not compatible with this build.",
            "args": [{ "name": "name", "required": true, "help": "the installed plugin's name" }, { "name": "args...", "required": false, "help": "arguments handed to the plugin verbatim, dashes included" }],
            "flags": [{ "name": "--json", "help": "machine-readable output (the return value rides inside the document)" }],
            "examples": ["amenbo plugin run worktree start 123", "eval \"$(amenbo plugin run worktree start 123)\"", "iex (amenbo plugin run worktree start 123)", "amenbo plugin run --json worktree finish 123"] }),
        json!({ "name": "plugin config set", "summary": "Stores one of an installed plugin's settings. The key must be one the plugin's manifest declares — that declaration is also what says whether the value is a secret, and Amenbo never judges that for itself: a secret goes to a store table of its own, which an export must leave (injected later as an environment variable, never echoed anywhere), everything else to the ordinary one. Either way the value is this project's and there is no tier under it; which project is never named here, it is the folder's binding (a human may move that with the global --project). Passing `-` as the value reads it from stdin, which is how a token stays off argv and out of shell history; the trailing newline a pipe adds is dropped, and nothing else. An empty value clears the setting rather than storing a blank, so this is also the unset door. The value is never echoed back. Filling the fields the author marked required is what lets `plugin enable` through. A setting whose author declared candidates takes those candidates, comma-separated, and refuses anything else with the list named; `none` answers with none of them, which is an answer of its own and not the same as leaving the setting empty — an empty value is still nobody having answered, and that is what a `default` in the manifest stands in for.",
            "args": [{ "name": "name", "required": true, "help": "the installed plugin's name" }, { "name": "key", "required": true, "help": "the setting's key, as the manifest declares it" }, { "name": "value", "required": true, "help": "the value; `-` reads it from stdin, an empty string clears it" }],
            "flags": [{ "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo plugin config set slack events task.done,task.rejected", "amenbo plugin config set slack events none", "printf %s \"$TOKEN\" | amenbo plugin config set slack webhook_url -", "amenbo plugin config set slack events \"\""] }),
        json!({ "name": "plugin config get", "summary": "Reads one of an installed plugin's settings back as this project holds it, exactly as stored. A secret's value never comes out this door, --json included: it reports only whether one is set, because a get that prints a token puts it in the terminal, the scrollback and the shell's history. Injection reads secrets whole, into the plugin's environment and nowhere else. A key the manifest does not declare is refused with the keys it does declare, so a typo answers with the vocabulary rather than a silent 'not set'. Where the author declared candidates it prints them too, ticking what is in force, and the line names which of the three states the setting is in: a value someone chose, none of them, or nobody answered — where what the run receives is the author's `default`. It is also where what the author wrote about the setting is read from a terminal: the paragraph explaining the field, and a note on a field whose value the plugin writes back itself rather than the user typing it — a field still writable here, since `plugin config set` is the road that value arrives by. A paragraph the manifest rules no longer admit is withheld whole rather than trimmed, naming `plugin validate` as where to see why. --json carries that as state, with the field's type, its candidates and its default beside the value, and its readonly flag and the author's paragraph beside those.",
            "args": [{ "name": "name", "required": true, "help": "the installed plugin's name" }, { "name": "key", "required": true, "help": "the setting's key, as the manifest declares it" }],
            "flags": [{ "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo plugin config get slack events", "amenbo plugin config get slack events --json"] }),
        json!({ "name": "plugin catalog list", "summary": "Lists the catalogs that make up the browsing view: the official catalog first, then each registered third-party catalog in the order it was added, with its display name, the fingerprint of the key its plugins are trusted on, how many plugins it currently offers, and whether it could be reached (from the network, or its cache). The unit is the catalog, not the plugin — what grows is the number of indexes, never per-plugin requests. Reads caches the incidental way: a catalog fresh on disk answers with no request, so listing many sources is not many fetches, and one dead URL is marked unreachable rather than costing the view. A catalog with no fingerprint published none, which is the line worth noticing: it can be browsed and nothing on it can be installed. --json carries plugins_total (after cross-catalog de-duplication, official winning a name clash) and per-source url/name/fingerprint/official/reachable/offered.",
            "args": [], "flags": [{ "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo plugin catalog list", "amenbo plugin catalog list --json"] }),
        json!({ "name": "plugin catalog add", "summary": "Registers a third-party catalog by the URL of its catalog.json, to browse alongside the official one (the 'free' tier), and pins the signing key it publishes at catalog-key.pub beside it. That key is what plugins from this catalog are trusted on, so registering one is a trust decision, not a bookmark: the fingerprint is shown and confirmed before anything is pinned (--yes confirms non-interactively, which a --json run must pass). A catalog that publishes no key registers without a question — it can be browsed, and nothing on it can be installed. A catalog that now publishes a different key is refused rather than re-pinned: unregister it and register it again, which puts the new fingerprint in front of whoever decides. --name gives it a display name (default: the host of its URL). Idempotent: registering the same URL twice is a no-op. Refuses a non-http(s) URL, and the official catalog's own URL (it is always included and is not a third-party source). The catalog is fetched once here so the first browse is warm, and how many plugins it offers is reported; an unreachable URL still registers and is retried on the next browse.",
            "args": [{ "name": "url", "required": true, "help": "the URL of the third-party catalog's catalog.json" }],
            "flags": [{ "name": "--name <name>", "help": "what to call this catalog on screen (default: the host of its URL)" }, { "name": "--yes/-y", "help": "confirm pinning the key non-interactively" }, { "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo plugin catalog add https://example.com/plugins/catalog.json", "amenbo plugin catalog add https://example.com/plugins/catalog.json --name 'the works catalog' --yes"] }),
        json!({ "name": "plugin catalog remove", "summary": "Unregisters a third-party catalog by its URL and drops its cached copy. Idempotent: removing a URL that is not registered is a no-op. The official catalog cannot be removed — it is not a registered source.",
            "args": [{ "name": "url", "required": true, "help": "the URL that was registered with `plugin catalog add`" }],
            "flags": [{ "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo plugin catalog remove https://example.com/plugins/catalog.json"] }),
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
    }
    spec
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
        let mut spec = build_index();
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
        out
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
    /// written as text, so this is the one seam the type cannot close: a command renamed or dropped
    /// there leaves a variant pointing at nothing, and every step holding it goes on compiling.
    #[test]
    fn every_command_a_step_names_exists() {
        let known: HashSet<String> = command_names().into_iter().collect();
        for cmd in Cmd::ALL {
            assert!(known.contains(cmd.name()), "{cmd:?} names {:?}, which is no command of ours", cmd.name());
        }
    }

    /// The table is for naming commands from steps, so an entry no step names is one nobody asked
    /// for. Left standing it reads as "a step somewhere runs this", which is what makes the list
    /// worth trusting when someone drops a command and comes here to see who is affected.
    #[test]
    fn no_command_is_named_that_no_step_names() {
        let named: HashSet<&str> = AGENT_CYCLE
            .iter()
            .chain(CYCLES.iter().flat_map(|c| c.backbone.iter().chain(c.optional)))
            .flat_map(|s| s.commands.iter().map(|c| c.name()))
            .collect();
        for cmd in Cmd::ALL {
            assert!(named.contains(cmd.name()), "{cmd:?} is in the table and no step names it");
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
                // Longest first: `task commit add` is a command, and so is the `task commit` inside it.
                let matched =
                    (1..=3.min(words.len())).rev().map(|n| words[..n].join(" ")).find(|c| known.contains(c));
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

    /// One plugin's line, hung on a step by the id its author named.
    fn hung(step_ref: &str) -> BTreeMap<String, Vec<String>> {
        BTreeMap::from([(
            step_ref.to_string(),
            vec!["amenbo plugin run worktree start <task-id>".to_string()],
        )])
    }

    /// The step named gets the line, wherever it was written — a step of the backbone, a cycle's
    /// ordered item, or a cycle's self-gated one (`AMB-D-571`). Nothing else on the step moves, and no
    /// other step is touched.
    #[test]
    fn a_line_lands_on_the_step_that_named_it() {
        for (step_ref, at) in [
            ("agentCycle.reserve", "/agentCycle/steps"),
            ("worktree.cut-per-task", "/cycles/worktree/backbone"),
            ("commit.install-the-hooks", "/cycles/commit/optional"),
        ] {
            let (_, id) = step_ref.split_once('.').unwrap();
            let mut spec = build();
            attach_tools(&mut spec, &hung(step_ref));

            let mut pointer = spec.pointer(at).and_then(Value::as_array).unwrap().iter();
            let step = pointer.find(|s| s["id"] == id).unwrap_or_else(|| panic!("{step_ref} is gone"));
            assert_eq!(step["tools"], json!(["amenbo plugin run worktree start <task-id>"]));
            assert!(step["step"].as_str().is_some_and(|s| !s.is_empty()), "the step's own prose stands");
            assert!(
                pointer.all(|other| other.get("tools").is_none()),
                "a line landed on a step nobody named"
            );
        }
    }

    /// A ref naming nothing in this document is dropped where it falls, and never invents what it
    /// names. The steps travel with Amenbo while a manifest stays where it was installed, so an
    /// unknown ref is the ordinary end of a step renamed or a cycle the runtime left out — not an
    /// error to raise at a reader who cannot act on it.
    #[test]
    fn a_ref_naming_nothing_here_is_dropped() {
        let before = build();
        for step_ref in [
            "worktree.retired-long-ago",
            "noSuchCycle.cut-per-task",
            "agentCycle.no-such-step",
            "cycles.worktree",
            "worktree",
            "",
        ] {
            let mut spec = build();
            attach_tools(&mut spec, &hung(step_ref));
            assert_eq!(spec, before, "{step_ref:?} changed the document");
        }
    }

    /// A cycle the runtime dropped takes its steps' refs with it: what is not in the document cannot
    /// be hung on, so the advice and the tool for it disappear together rather than one without the
    /// other.
    #[test]
    fn a_dropped_cycle_leaves_nothing_to_hang_on() {
        let mut spec = build();
        spec["cycles"].as_object_mut().unwrap().remove("worktree");
        let without = spec.clone();
        attach_tools(&mut spec, &hung("worktree.cut-per-task"));
        assert_eq!(spec, without);
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
        let prose = dev.to_string();
        assert!(prose.contains("minimum Amenbo version"), "the product's name lost its capital before a noun that doubles as a command word");
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
        assert_eq!(dev(r#"`eval "$(amenbo plugin run worktree start 123)"`"#), r#"`eval "$(amenbo-dev plugin run worktree start 123)"`"#);
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
