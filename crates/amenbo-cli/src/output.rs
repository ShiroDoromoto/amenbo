//! The two output layers (human-facing text / machine-readable `--json`), plus the shared error and
//! confirmation handling.

use std::io::{IsTerminal, Write};
use std::sync::OnceLock;

use amenbo_core::model::ActorKind;
use serde::Serialize;
use serde_json::json;

#[derive(Clone, Copy)]
pub struct Flags {
    pub json: bool,
    pub yes: bool,
    pub quiet: bool,
    /// Reserved for colored output; nothing is colored yet.
    #[allow(dead_code)]
    pub no_color: bool,
    /// The facet of this invocation (human / ai). From `--actor`, then `AMENBO_ACTOR`, defaulting to human.
    pub actor: amenbo_core::model::ActorKind,
}

/// The **typed registry** of CLI-specific error codes (English, fixed — they are contract). Shaped like
/// core's [`amenbo_core::ErrorCode`]: `as_str()` is the **only** place the strings are defined, and
/// [`CliErrorCode::ALL`] is the single source of truth for the set. Core-originated codes (`e.code()`
/// via `From<Error>`) do not belong here; a parity test keeps the two sets disjoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CliErrorCode {
    ConfirmationRequired,
    NoPointer,
    InitPointerExists,
    InitAmbiguousOwners,
    BindingNestedTree,
    NestedWorktree,
    UnbindNoBinding,
    FacetRequired,
    AiGuardrail,
}

impl CliErrorCode {
    /// The stable strings the contract names (this match is the only place they are defined).
    /// It is exhaustive, so adding a variant breaks the build rather than silently omitting a code.
    pub const fn as_str(self) -> &'static str {
        match self {
            CliErrorCode::ConfirmationRequired => "confirmation_required",
            CliErrorCode::NoPointer => "no_pointer",
            CliErrorCode::InitPointerExists => "init_pointer_exists",
            CliErrorCode::InitAmbiguousOwners => "init_ambiguous_owners",
            CliErrorCode::BindingNestedTree => "binding_nested_tree",
            CliErrorCode::NestedWorktree => "nested_worktree",
            CliErrorCode::UnbindNoBinding => "unbind_no_binding",
            CliErrorCode::FacetRequired => "facet_required",
            CliErrorCode::AiGuardrail => "ai_guardrail",
        }
    }

    /// Every CLI-specific error code: the single source of truth for the parity test and the contract
    /// snapshot. Only the tests enumerate it in the binary today, but it is kept deliberately as the
    /// registry's truth source (the GUI's TS side is checked for parity against it too).
    #[allow(dead_code)]
    pub const ALL: &'static [CliErrorCode] = &[
        CliErrorCode::ConfirmationRequired,
        CliErrorCode::NoPointer,
        CliErrorCode::InitPointerExists,
        CliErrorCode::InitAmbiguousOwners,
        CliErrorCode::BindingNestedTree,
        CliErrorCode::NestedWorktree,
        CliErrorCode::UnbindNoBinding,
        CliErrorCode::FacetRequired,
        CliErrorCode::AiGuardrail,
    ];
}

/// A CLI-level error (core-originated or CLI-specific). `code` comes from [`amenbo_core::ErrorCode`]
/// for the former and [`CliErrorCode`] for the latter — never a raw literal.
pub struct CliError {
    pub code: &'static str,
    pub message: String,
    pub hint: Option<String>,
    pub exit: i32,
}

impl CliError {
    pub fn confirmation_required(what: &str) -> CliError {
        CliError {
            code: CliErrorCode::ConfirmationRequired.as_str(),
            message: format!("{what} is a destructive operation and requires confirmation."),
            hint: Some("To run non-interactively, pass --yes / -y.".to_string()),
            exit: 1,
        }
    }

    /// Execution guard: an operating command was run in a bare directory with no pointer (`.amenbo`)
    /// and no `AMENBO_HOME` / `AMENBO_PROJECT_DIR`. Rather than silently creating a store, point the
    /// user at `amenbo init`. The commands that create or name the pointer (init / join / bind) are the
    /// only exceptions.
    /// What we hand back is not an explanation of the machinery but a **fork in the road** — the two
    /// things they can do (init a new project / bind to an existing one) — plus, when
    /// `candidate_projects` is non-empty, the projects that actually exist on this machine. Enumerating
    /// them is a plain SQLite probe: it never opens a store, so it cannot forward-migrate one as a side
    /// effect.
    pub fn no_pointer(candidate_projects: &[String]) -> CliError {
        let mut hint = String::from(
            "Pick one:\n  • Start a new project here: amenbo init --name <you>\n  • Link an existing project: amenbo bind --project <name or id>",
        );
        if !candidate_projects.is_empty() {
            hint.push_str("\nExisting projects:");
            for p in candidate_projects {
                hint.push_str(&format!("\n  • amenbo bind --project {p}"));
            }
        }
        CliError {
            code: CliErrorCode::NoPointer.as_str(),
            message: "This folder is not linked to amenbo (no .amenbo found).".to_string(),
            hint: Some(hint),
            exit: 1,
        }
    }

    /// init guard: this folder (or an ancestor) is already bound to another project by a `.amenbo`
    /// pointer. Creating a new project would overwrite that pointer unasked and the folder's real
    /// project would vanish from the CWD's view (a clobber), so we refuse by default — the same
    /// "respect what is already there" rule AGENTS.md follows. Re-point it with `bind`; start over for
    /// real with `--force`. We name the project (or `(no project)` when the pointer is unset).
    pub fn init_pointer_exists(dir: &str, project_id: &str, has_data: bool) -> CliError {
        let data_note = if has_data { " (it has data)" } else { "" };
        CliError {
            code: CliErrorCode::InitPointerExists.as_str(),
            message: format!(
                "This folder ({dir}) is already linked to amenbo (project {project_id}){data_note}. init was refused because it would create a new project and overwrite the pointer."
            ),
            hint: Some(format!(
                "To use the existing project in this folder, run `amenbo bind --project {project_id}`. If only the pointer is broken, recover with `amenbo bind --project <name or id>`. To really create a brand-new project and overwrite, run `amenbo init --force`."
            )),
            exit: 1,
        }
    }

    /// init guard, ambiguous case: in a folder with no `.amenbo` pointer but with a managed block, the
    /// reverse lookup in the bindings registry named **several live projects** as owners of this folder.
    /// There is no way to tell whose lost pointer we should restore, so instead of silently picking one
    /// we stop and list the candidates (the plural sibling of `init_pointer_exists`). One live owner is
    /// recovered outright and zero means init just proceeds, so we only land here when it is genuinely
    /// ambiguous.
    pub fn init_ambiguous_owners(dir: &str, project_ids: &[String]) -> CliError {
        let mut hint = String::from("Multiple projects claim this folder. Pick the one to recover:");
        for pid in project_ids {
            hint.push_str(&format!("\n  • amenbo bind --project {pid}"));
        }
        hint.push_str("\nOr run `amenbo init --force` to create a brand-new project here.");
        CliError {
            code: CliErrorCode::InitAmbiguousOwners.as_str(),
            message: format!(
                "This folder ({dir}) has no .amenbo pointer, but {} existing projects claim it, so init could not tell which lost pointer to recover.",
                project_ids.len()
            ),
            hint: Some(hint),
            exit: 1,
        }
    }

    /// Nested-binding guard: a new binding was requested in a **subdirectory** of a tree an ancestor's
    /// `.amenbo` already manages. A pointer there shadows the ancestor's binding (amenbo run in that
    /// subdirectory would resolve to the subdirectory's store, not the parent's) and scatters
    /// `.amenbo`/AGENTS.md/CLAUDE.md through the source tree. Same "respect the existing tree" rule as
    /// `init`'s clobber guard. `--force` is the way through when binding a subdirectory separately is
    /// what you actually meant.
    pub fn binding_nested_tree(ancestor_dir: &str) -> CliError {
        CliError {
            code: CliErrorCode::BindingNestedTree.as_str(),
            message: format!(
                "This folder is already inside an amenbo-managed tree (bound at {ancestor_dir}). Binding a subdirectory would shadow that pointer."
            ),
            hint: Some(
                "Run bind from the managed root instead, or pass --force to intentionally bind this subdirectory.".to_string(),
            ),
            exit: 1,
        }
    }

    /// Nested-worktree guard: the CWD sits in a git worktree cut **inside** an amenbo-managed folder, so it
    /// inherited that folder's binding by the upward walk. The worktree is throwaway; the store it would
    /// write to is not — it lives in app-data, outside the worktree, and outlives its deletion. Refusing
    /// beats warning: a warning that can be ignored is prose that merely moved house. The way out is to run
    /// amenbo in the project folder, never to `bind` this checkout — restoring the binding here is the
    /// accident itself, so it is neither offered nor a way through ([`amenbo_core::worktree`]).
    pub fn nested_worktree(worktree_root: &str, bound_dir: &str) -> CliError {
        CliError {
            code: CliErrorCode::NestedWorktree.as_str(),
            message: format!(
                "This is a git worktree ({worktree_root}) cut inside an amenbo-managed folder, so it inherited the binding of {bound_dir} from above. Deleting the worktree would not undo what amenbo wrote: the store lives outside it."
            ),
            hint: Some(format!(
                "Operate amenbo in the project folder itself ({bound_dir}). Cut worktrees outside it, where there is no binding to inherit."
            )),
            exit: 1,
        }
    }

    /// unbind guard: the folder being unbound has no `.amenbo` pointer — it was never bound in the first
    /// place. If an ancestor is bound, we do not quietly unbind it and take the whole tree down with it;
    /// we say to run unbind there instead (unbind only ever releases **that** folder's binding).
    pub fn unbind_no_binding(dir: &str, ancestor: Option<&str>) -> CliError {
        let hint = match ancestor {
            Some(a) => format!(
                "This folder inherits a binding from an ancestor ({a}). To unbind that, run `amenbo unbind` there (or `amenbo unbind --dir {a}`)."
            ),
            None => "Nothing to unbind here. Link a folder with `amenbo init` or `amenbo bind`.".to_string(),
        };
        CliError {
            code: CliErrorCode::UnbindNoBinding.as_str(),
            message: format!("This folder ({dir}) has no .amenbo pointer to unbind."),
            hint: Some(hint),
            exit: 1,
        }
    }

    /// No silent facet fallback. The facet (`--actor` / `AMENBO_ACTOR`) was left unset while the call
    /// bears the **marks of a machine** (`--json`, or a non-TTY). Quietly defaulting to human would file
    /// an AI session's status/comment/done under the human facet and rot the human/ai distinction in the
    /// activity stream, so we refuse to default and demand the facet be stated (fail loud). An
    /// interactive human (TTY, no `--json`) still gets the human default and is untouched.
    pub fn facet_required() -> CliError {
        CliError {
            code: CliErrorCode::FacetRequired.as_str(),
            message: "facet is unspecified in a machine context (--json or a non-TTY pipe). It is not defaulted to human here, to avoid silently recording AI actions as a human."
                .to_string(),
            hint: Some(
                "Declare the facet explicitly: set AMENBO_ACTOR=ai (AI agents) or AMENBO_ACTOR=human, or pass --actor ai|human."
                    .to_string(),
            ),
            exit: 2,
        }
    }

    /// Guardrail violation: an AI (`--actor ai`) attempted an operation the machine-local policy forbids.
    /// This prevents accidents by an honest actor; it is not a security boundary.
    pub fn ai_guardrail(message: impl Into<String>) -> CliError {
        CliError {
            code: CliErrorCode::AiGuardrail.as_str(),
            message: message.into(),
            hint: Some("Have a human run it, or allow it via local policy.".to_string()),
            exit: 1,
        }
    }
}

impl From<amenbo_core::Error> for CliError {
    fn from(e: amenbo_core::Error) -> CliError {
        use amenbo_core::Error as E;
        let hint = match &e {
            E::AmbiguousId { candidates, .. } => {
                Some(format!("Candidates: {}", candidates.join(", ")))
            }
            E::NotFound(_) => Some("Run `amenbo agent --json` to see how to operate.".to_string()),
            E::BindingStale(_) => Some("Re-link it with `amenbo bind --project <id>`.".to_string()),
            E::AlreadyReserved(_) => Some(
                "Another session reserved it first. Pick the next task (`amenbo agent --json`), or hand it back with `amenbo task status <id> todo` if the reservation is stale."
                    .to_string(),
            ),
            // The counterpart to `already_reserved`. That one means someone else holds it (→ move on to
            // the next task); this one means a premise you declared is unmet (→ resolve the premise).
            // The two point in opposite directions, so keep them apart.
            E::NotReady(_) => Some(
                "A declared premise is unmet, and there is no --force. Resolve it: finish the blocker (`amenbo task done <blocker>`) or drop the edge (`amenbo task undepend <id> --on <blocker>`); settle the premise (`amenbo decision accept AMB-D-N`) or unlink it (`amenbo decision link AMB-D-N <id> --unlink`)."
                    .to_string(),
            ),
            _ => None,
        };
        CliError {
            code: e.code(),
            // The CLI surface is English-only; the GUI (`e.to_string()` in the Tauri commands) uses the localized Display.
            message: e.message_en(),
            hint,
            exit: 1,
        }
    }
}

/// The unfinished-setup report to hang on every `--json` response, set once per run from argv and the
/// filesystem. Unset means there is nothing to report, which is the ordinary case and stays free: a run
/// that never calls [`set_setup_report`] serialises exactly what it always did.
static SETUP_REPORT: OnceLock<serde_json::Value> = OnceLock::new();

/// Declare what setup is unfinished, so [`print_json`] carries it. Idempotent by construction — one run
/// decides this once, before any output.
pub fn set_setup_report(report: serde_json::Value) {
    let _ = SETUP_REPORT.set(report);
}

/// Pretty-print a JSON value to stdout, carrying the unfinished-setup report when there is one. The report
/// rides here rather than at each of the forty call sites because its whole point is to reach an AI on
/// every response, and a field the caller has to remember to add is one they forget; it is grafted on only
/// when the payload is a JSON object and only when something is actually unfinished, so the ordinary
/// response is untouched and a payload that is not an object cannot be corrupted into one.
pub fn print_json<T: Serialize>(value: &T) {
    let s = match (SETUP_REPORT.get(), serde_json::to_value(value)) {
        (Some(report), Ok(mut v)) => {
            if let Some(obj) = v.as_object_mut() {
                obj.insert("setup_incomplete".to_string(), report.clone());
            }
            serde_json::to_string_pretty(&v)
        }
        _ => serde_json::to_string_pretty(value),
    };
    println!("{}", s.unwrap_or_else(|_| "{}".to_string()));
}

/// One human-facing line (suppressed under `--json` / `--quiet`).
pub fn human(flags: &Flags, line: impl AsRef<str>) {
    if !flags.json && !flags.quiet {
        println!("{}", line.as_ref());
    }
}

/// Print an error to stderr and return the exit code.
pub fn render_error(flags: &Flags, err: &CliError) -> i32 {
    if flags.json {
        let mut obj = json!({ "error": { "code": err.code, "message": err.message } });
        if let Some(h) = &err.hint {
            obj["error"]["hint"] = json!(h);
        }
        eprintln!("{}", serde_json::to_string_pretty(&obj).unwrap());
    } else {
        eprintln!("Error: {}", err.message);
        if let Some(h) = &err.hint {
            eprintln!("Hint: {h}");
        }
    }
    err.exit
}

/// Print the envelope every write command shares.
pub fn write_envelope(
    flags: &Flags,
    action: &str,
    resource_key: &str,
    resource: serde_json::Value,
    changed: Option<Vec<String>>,
    noop: bool,
    human_line: impl AsRef<str>,
) {
    if flags.json {
        // State the effective facet, so a misconfiguration (meant ai, acted as human) is visible right in the output.
        let mut obj = json!({ "ok": true, "action": action, "noop": noop, "acted_facet": flags.actor.as_str() });
        if let Some(c) = changed {
            obj["changed"] = json!(c);
        }
        obj[resource_key] = resource;
        print_json(&obj);
    } else {
        // Writes on the ai facet carry the effective facet in the human line too (so a mix-up shows); the interactive human gets no marker.
        if flags.actor == ActorKind::Ai {
            human(flags, format!("{} · recorded as ai", human_line.as_ref()));
        } else {
            human(flags, human_line);
        }
    }
}

/// Confirm a destructive operation: `--yes` allows it outright, `--json` without it is an error, and an
/// interactive run asks y/N.
pub fn confirm(flags: &Flags, what: &str) -> Result<bool, CliError> {
    if flags.yes {
        return Ok(true);
    }
    if flags.json || !std::io::stdin().is_terminal() {
        return Err(CliError::confirmation_required(what));
    }
    print!("About to: {what}. Proceed? [y/N]: ");
    std::io::stdout().flush().ok();
    let mut buf = String::new();
    std::io::stdin().read_line(&mut buf).ok();
    let yes = matches!(buf.trim(), "y" | "Y" | "yes");
    if !yes {
        human(flags, "Aborted.");
    }
    Ok(yes)
}

/// The rough length at which a body counts as "long prose" (tunable). Counted in chars, so one CJK
/// character counts as one.
const LONG_BODY_CHARS: usize = 600;

/// Inspect a body and return the readability hints that apply (fixed strings, printed to stderr). Kept
/// conservative, to avoid false positives.
pub fn body_hints(body: &str) -> Vec<&'static str> {
    let trimmed = body.trim();
    let mut hints = Vec::new();
    if trimmed.chars().count() > LONG_BODY_CHARS && !trimmed.contains('\n') {
        hints.push(
            "本文が長く無構造です。結論を先頭に置き、箇条書き/表(GFM)で構造化すると走査しやすくなります。",
        );
    }
    if has_mermaid(trimmed) && !has_text_outside_fences(trimmed) {
        hints.push(
            "mermaid 図が本文の全てです。CLI では図は生ソース表示なので、要点をテキスト一行添えてください（図を唯一の伝達路にしない）。",
        );
    }
    hints
}

/// If the line opens a fence (``` / ~~~), return that fence character.
fn fence_char(trimmed_line: &str) -> Option<char> {
    if trimmed_line.starts_with("```") {
        Some('`')
    } else if trimmed_line.starts_with("~~~") {
        Some('~')
    } else {
        None
    }
}

/// Whether the body contains a `` ```mermaid `` (or `~~~mermaid`) code fence.
fn has_mermaid(body: &str) -> bool {
    body.lines().any(|l| {
        let t = l.trim_start();
        t.starts_with("```mermaid") || t.starts_with("~~~mermaid")
    })
}

/// Whether any non-empty text line lives outside a code fence (i.e. the body says something besides the diagram).
fn has_text_outside_fences(body: &str) -> bool {
    let mut fence: Option<char> = None;
    for line in body.lines() {
        let t = line.trim();
        match fence {
            Some(f) => {
                if fence_char(t) == Some(f) {
                    fence = None; // closed by a terminator of the same fence character
                }
            }
            None => match fence_char(t) {
                Some(f) => fence = Some(f),
                None if !t.is_empty() => return true,
                None => {}
            },
        }
    }
    false
}

/// Print the body hints to stderr (call after a successful write). Never touches stdout, `--json` or not.
pub fn warn_body(body: &str) {
    for h in body_hints(body) {
        eprintln!("hint: {h}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use amenbo_core::ErrorCode;
    use std::collections::BTreeSet;

    fn set<I: IntoIterator<Item = &'static str>>(it: I) -> BTreeSet<&'static str> {
        it.into_iter().collect()
    }

    #[test]
    fn cli_error_code_registry_is_the_full_fixed_set() {
        // Contract snapshot: pins the whole set of CLI-specific codes, so adding, dropping or renaming one is a deliberate checkpoint.
        let expected = set([
            "confirmation_required",
            "no_pointer",
            "init_pointer_exists",
            "init_ambiguous_owners",
            "binding_nested_tree",
            "nested_worktree",
            "unbind_no_binding",
            "facet_required",
            "ai_guardrail",
        ]);
        let actual = set(CliErrorCode::ALL.iter().map(|c| c.as_str()));
        assert_eq!(actual, expected, "the full set of CLI error codes does not match the contract");
        assert_eq!(
            CliErrorCode::ALL.len(),
            actual.len(),
            "duplicate CLI error code strings"
        );
        for c in CliErrorCode::ALL {
            let s = c.as_str();
            assert!(!s.is_empty(), "an empty code");
            assert!(
                s.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'),
                "a code is lowercase snake_case: {s}"
            );
        }
    }

    #[test]
    fn core_and_cli_code_sets_do_not_collide() {
        // The code strings of the two production registries (core / cli) never overlap — the same code is
        // never defined twice with different meanings — which is what makes "one source of truth" hold across crates.
        let core = set(ErrorCode::ALL.iter().map(|c| c.as_str()));
        let cli = set(CliErrorCode::ALL.iter().map(|c| c.as_str()));
        let overlap: Vec<_> = core.intersection(&cli).collect();
        assert!(overlap.is_empty(), "core and cli define colliding codes: {overlap:?}");
    }

    #[test]
    fn retired_account_binding_codes_are_absent() {
        // There is no account dimension (binding/vault), so the codes that came from the "one folder holds
        // at most one account" collision checks are absent from the registry.
        let cli = set(CliErrorCode::ALL.iter().map(|c| c.as_str()));
        assert!(!cli.contains("folder_already_bound"));
        assert!(!cli.contains("binding_persona_conflict"));
    }

    // ── Body readability hints ───────────────────────────────

    #[test]
    fn body_hints_long_unstructured_prose_warns() {
        // Over 600 chars with no newline is a monolithic wall of prose, so hint at structure.
        let body = "あ".repeat(700);
        let hints = body_hints(&body);
        assert_eq!(hints.len(), 1);
        assert!(hints[0].contains("構造化"));
    }

    #[test]
    fn body_hints_long_but_structured_is_silent() {
        // Past 600 chars but with newlines it is not unstructured, so stay quiet (no false positive).
        let body = format!("# 見出し\n\n{}", "本文。".repeat(300));
        assert!(body.chars().count() > LONG_BODY_CHARS);
        assert!(body_hints(&body).is_empty());
    }

    #[test]
    fn body_hints_short_body_is_silent() {
        // A short body stays quiet even when unstructured (boundary: 600 or fewer).
        assert!(body_hints(&"x".repeat(LONG_BODY_CHARS)).is_empty());
        assert!(body_hints("結論。理由。次の一手。").is_empty());
    }

    #[test]
    fn body_hints_boundary_601_chars_warns() {
        // Boundary: exactly 601 chars (no newline) fires.
        assert!(body_hints(&"x".repeat(LONG_BODY_CHARS + 1)).len() == 1);
    }

    #[test]
    fn body_hints_mermaid_only_warns() {
        // Diagram only (no non-empty text outside the fence), so hint at adding the gist.
        let body = "```mermaid\ngraph TD; A-->B;\n```";
        let hints = body_hints(body);
        assert_eq!(hints.len(), 1);
        assert!(hints[0].contains("mermaid"));
    }

    #[test]
    fn body_hints_mermaid_with_summary_is_silent() {
        // A summary line around the diagram keeps it quiet.
        let body = "要点: A から B へ遷移する。\n\n```mermaid\ngraph TD; A-->B;\n```";
        assert!(body_hints(body).is_empty());
    }

    #[test]
    fn body_hints_no_body_is_silent() {
        assert!(body_hints("").is_empty());
        assert!(body_hints("   \n  ").is_empty());
    }
}
