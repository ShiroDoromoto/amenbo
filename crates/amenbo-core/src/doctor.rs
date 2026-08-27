//! The doctor surface — the list of integrity issues.
//!
//! [`crate::validate::doctor`] looks only inside the store (self-reference and duplicates in the
//! read-model); this machine's **environment** — the `.amenbo` pointers of bound folders and their managed
//! blocks — is detected on other core paths ([`crate::binding`] / [`crate::agents`]). The doctor surface
//! gathers both into a single [`DoctorResult`].
//!
//! The gathering lives in core because there are two surfaces (the CLI's `doctor` and the GUI's Settings >
//! Integrity), and the list of issues has to be the same on both.
//!
//! **An issue carries no sentence.** The core returns a [`DoctorIssueKind`] (the id of a wording template)
//! and its `params` (the difference); the sentence a person reads is assembled by the surface — fixed
//! English in the CLI, `config.language` in the GUI. i18n is split at the reader. This is the same shape as
//! the error codes: a typed registry in the core, the wording on the surface.
//!
//! Repair (`--fix`, or the GUI's repair button) does not live here. The cleanup entry points are methods on
//! [`crate::store::Store`] (`gc_blobs` / `forget_orphan_dirs`), and both surfaces call those same ones.
//! Reading and fixing stay apart: a check is read-only, a repair is an explicit manual act. Every cleanup
//! entry point is **non-destructive** — it drops only index rows and bytes with zero references — so a
//! surface may run one without asking for confirmation.

use std::collections::BTreeMap;

use serde::{Serialize, Serializer};

use crate::store::Store;
use crate::validate::DoctorResult;

/// The kind of issue doctor raises. It is the **template id for the sentence a surface assembles**, and it
/// is also part of the machine contract that appears in `--json`.
///
/// The granularity is "can one sentence explain it?" — the same detection splits into two kinds when the fix
/// differs (for instance `LegacyPointer`, whose target project is unambiguous, versus
/// `LegacyPointerAmbiguous`, which only a human can settle). Keeping a surface's templates one-to-one with
/// the kinds stops branch-on-`params` logic from growing on the surface side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DoctorIssueKind {
    /// A dependency edge that points at itself.
    SelfDependency,
    /// A duplicated `order_key` within one project.
    DuplicateOrderKey,
    /// An attachment whose target row is gone. The one reference in the store no foreign key holds
    /// (`attachment.target_type` / `target_id` is polymorphic), so it is the one that can be left dangling.
    OrphanAttachment,
    /// A bound folder's managed block is an older version.
    StaleManagedBlock,
    /// A legacy-format `.amenbo` whose project resolves unambiguously.
    LegacyPointer,
    /// A legacy-format `.amenbo` whose project does not resolve.
    LegacyPointerAmbiguous,
    /// A bound folder whose `.amenbo` is gone, claimed by exactly one project.
    MissingPointer,
    /// A bound folder whose `.amenbo` is gone, with no single project claiming it.
    MissingPointerAmbiguous,
    /// A folder row no live project claims — the debris a deleted project left in the index.
    OrphanBinding,
    /// A body whose prose points at refs that resolve to nothing.
    DeadRef,
    /// A task declared to start after the day it is due. Two declarations that contradict each other,
    /// and the pair hides: the task stays out of the mailbox while the day it was due goes by.
    StartAfterDue,
    /// A live project no folder on this machine leads to — nothing can operate it.
    ProjectWithoutFolder,
    /// A bound folder that does not start its AI on Amenbo, and that names the tools it is waiting on.
    UnwiredFolder,
    /// A bound folder that does not start its AI on Amenbo and shows no sign of which tool it uses.
    UnwiredFolderAmbiguous,
}

impl DoctorIssueKind {
    /// The contractual set. `--json` consumers (an AI) and the GUI's wording table line up with it; the
    /// parity is checked by a TypeScript-side test.
    pub const ALL: &'static [DoctorIssueKind] = &[
        Self::SelfDependency,
        Self::DuplicateOrderKey,
        Self::OrphanAttachment,
        Self::StaleManagedBlock,
        Self::LegacyPointer,
        Self::LegacyPointerAmbiguous,
        Self::MissingPointer,
        Self::MissingPointerAmbiguous,
        Self::OrphanBinding,
        Self::DeadRef,
        Self::StartAfterDue,
        Self::ProjectWithoutFolder,
        Self::UnwiredFolder,
        Self::UnwiredFolderAmbiguous,
    ];

    /// The one and only place a kind string is written — `Serialize` goes through here too.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SelfDependency => "self_dependency",
            Self::DuplicateOrderKey => "duplicate_order_key",
            Self::OrphanAttachment => "orphan_attachment",
            Self::StaleManagedBlock => "stale_managed_block",
            Self::LegacyPointer => "legacy_pointer",
            Self::LegacyPointerAmbiguous => "legacy_pointer_ambiguous",
            Self::MissingPointer => "missing_pointer",
            Self::MissingPointerAmbiguous => "missing_pointer_ambiguous",
            Self::OrphanBinding => "orphan_binding",
            Self::DeadRef => "dead_ref",
            Self::StartAfterDue => "start_after_due",
            Self::ProjectWithoutFolder => "project_without_folder",
            Self::UnwiredFolder => "unwired_folder",
            Self::UnwiredFolderAmbiguous => "unwired_folder_ambiguous",
        }
    }

    /// Only a broken store is an `error` — a self-referencing edge, and an attachment whose target is gone
    /// ([`Self::OrphanAttachment`]: the one reference no foreign key holds, so the one that can dangle).
    /// The environment issues have not broken anything in the store — they
    /// only mean a human's or an AI's path into it is misaligned — so they are `warning`s and do not knock
    /// `ok` down. [`Self::DeadRef`] is a warning for the same reason from the other side: every row is intact
    /// and every constraint holds, and what has rotted is a sentence inside a body — prose Amenbo stores and
    /// does not own. Severity is an attribute of the kind, so whoever builds an issue does not get to choose
    /// it.
    pub const fn severity(self) -> &'static str {
        match self {
            Self::SelfDependency | Self::OrphanAttachment => "error",
            _ => "warning",
        }
    }

    /// The difference this kind's sentence needs. These are the only keys a surface's template may refer to
    /// as `{key}`, and whoever builds an issue fills exactly them — no more, no less ([`DoctorIssue::new`]
    /// checks).
    pub const fn param_keys(self) -> &'static [&'static str] {
        match self {
            Self::SelfDependency => &["dep"],
            Self::DuplicateOrderKey => &["project", "order_key"],
            Self::OrphanAttachment => &["attachment", "target"],
            Self::StaleManagedBlock => &["path", "dir", "version", "current"],
            Self::LegacyPointer => &["path", "dir", "project"],
            Self::LegacyPointerAmbiguous => &["path"],
            Self::MissingPointer => &["dir", "project"],
            Self::MissingPointerAmbiguous => &["dir", "claims"],
            Self::OrphanBinding => &["dir"],
            Self::DeadRef => &["at", "refs"],
            Self::StartAfterDue => &["task", "start_on", "due_on"],
            Self::ProjectWithoutFolder => &["project", "name"],
            Self::UnwiredFolder => &["dir", "tools"],
            Self::UnwiredFolderAmbiguous => &["dir"],
        }
    }
}

impl Serialize for DoctorIssueKind {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

/// One issue raised by doctor. **It has no sentence** — a surface assembles one by filling `params` into the
/// [`DoctorIssueKind`] template.
#[derive(Clone, Debug, Serialize)]
pub struct DoctorIssue {
    pub kind: DoctorIssueKind,
    /// A copy of `kind.severity()`, so a surface (and `--json`) can pick a colour or a glyph from severity
    /// alone.
    pub severity: &'static str,
    /// What is broken (`task:12`, or a folder path). It embeds an id, so it is not translated per surface.
    pub target: String,
    pub params: BTreeMap<String, String>,
}

impl DoctorIssue {
    /// `params` must fill `kind.param_keys()` exactly: a missing key leaves a literal `{key}` in the
    /// surface's sentence, and a spare one carries a difference nobody reads. Debug builds (i.e. tests) trip
    /// on it so it is noticed.
    pub fn new(kind: DoctorIssueKind, target: impl Into<String>, params: &[(&str, &str)]) -> Self {
        debug_assert!(
            {
                let mut got: Vec<_> = params.iter().map(|(k, _)| *k).collect();
                got.sort_unstable();
                let mut want: Vec<_> = kind.param_keys().to_vec();
                want.sort_unstable();
                got == want
            },
            "{}: params {:?} do not match the declared keys {:?}",
            kind.as_str(),
            params.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
            kind.param_keys(),
        );
        Self {
            kind,
            severity: kind.severity(),
            target: target.into(),
            params: params
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect(),
        }
    }
}

/// This machine's doctor surface. It takes the store's internal integrity ([`crate::validate::doctor`]) and
/// appends the environment issues: a managed block left behind by a version bump, a legacy or vanished
/// `.amenbo`, a folder row nobody claims. Every check is side-effect-free — nothing is rewritten until
/// `--fix` is called.
///
/// When the reach is closed (an AI), the store side shows only breakage in the bound project and the
/// environment side only the folders that claim that project ([`crate::binding::dirs_in_reach`]). Neither
/// another project's folder paths nor its project id reach the surface: even a diagnostic must not become a
/// path by which something outside the binding enters the AI's context. And since `--fix` can only repair
/// the issues that were raised, repair stays inside the binding too — it never rewrites the `.amenbo` of
/// another project's folder.
pub fn report(store: &Store) -> crate::error::Result<DoctorResult> {
    let mut result = store.doctor()?;
    // Everything gathered here is a warning: the store is not broken. An environment issue means only that a
    // human's or an AI's path into it is misaligned, and a dead ref means only that a sentence inside a body
    // has rotted while every row around it stands.
    //
    // These are also the checks too expensive to run anywhere but here — a filesystem walk per bound folder,
    // and a Markdown parse of every body still open to a reader — which is why they are chained onto the
    // report rather than living in `validate::doctor`, the half that runs at every write open and on every
    // GUI tick.
    let env = stale_managed_block_issues(store)
        .into_iter()
        .chain(pointer_issues(store))
        .chain(orphan_binding_issues(store))
        .chain(project_without_folder_issues(store)?)
        .chain(unwired_folder_issues(store)?)
        .chain(store.dead_refs()?);
    for issue in env {
        result.summary.warning += 1;
        result.issues.push(issue);
    }
    result.ok = result.summary.error == 0;
    Ok(result)
}

/// When a binary update bumps the managed-block template's version, every already-bound folder keeps its old
/// block. Running Amenbo in such a folder repairs it ([automatic catch-up](crate::agents::follow_stale_block))
/// — but **a folder nobody runs Amenbo in is repaired by nobody**, and that residue is what this looks for.
/// It scans the `CLAUDE.md` / `AGENTS.md` of every folder this machine records as bound and warns about any
/// whose version trails the current one. **It rewrites nothing** — detection is side-effect-free. The fixes
/// are `sync-guide` (all folders) or simply running Amenbo in the folder (that one folder).
fn stale_managed_block_issues(store: &Store) -> Vec<DoctorIssue> {
    use crate::agents::{stale_bound_blocks, MANAGED_BLOCK_VERSION};
    let in_reach = crate::binding::dirs_in_reach(store);
    stale_bound_blocks(&store.bindings())
        .into_iter()
        .filter(|s| in_reach.iter().any(|dir| dir == &s.dir))
        .map(|s| {
            let path = std::path::Path::new(&s.dir).join(s.file).display().to_string();
            DoctorIssue::new(
                DoctorIssueKind::StaleManagedBlock,
                &path,
                &[
                    ("path", &path),
                    // A surface repairs a folder at a time (re-sync takes a dir), but the sentence it shows
                    // names the file's path. Rather than make the surface carve the dir back out of the
                    // path, hand it both as separate parts of the difference.
                    ("dir", &s.dir),
                    ("version", &s.version.to_string()),
                    ("current", &MANAGED_BLOCK_VERSION.to_string()),
                ],
            )
        })
        .collect()
}

/// The issues where a bound folder's `.amenbo` is broken — legacy format ([`legacy_pointer_issues`]) or gone
/// ([`missing_pointer_issues`]). Both are the same quiet failure: an AI started in that folder does not
/// resolve to the project it was meant to, and both are fixed the same way — bind the folder to the project
/// again (`bind`). The kinds whose target resolves unambiguously (the ones without `*_ambiguous`) carry
/// `dir` and `project` in their difference, so a surface can wire them straight to the bind path (the button
/// on the GUI's doctor surface). The ambiguous ones only a human can settle, so they get no button.
///
/// [`report`] (the CLI's `doctor` and the GUI's Settings > Integrity) and the GUI's startup health banner
/// (the `pointer_issues` command, called once at startup) draw from **this same** function. It is kept off
/// the snapshot path, which recomputes every tick: an environment check is a filesystem walk over every
/// bound folder, and that is not a price to pay on each tick that follows a change in the store.
pub fn pointer_issues(store: &Store) -> Vec<DoctorIssue> {
    legacy_pointer_issues(store)
        .into_iter()
        .chain(missing_pointer_issues(store))
        .collect()
}

/// Warns about a **legacy-format `.amenbo`** left in a bound folder — one whose `project_id` is a ULID, a
/// string that will not read back as an integer, so today the folder looks bound to no project at all.
///
/// A folder that resolves to a single live project (`recoverable`) is fixed simply by running Amenbo there:
/// `resolve_upward` quietly rewrites the pointer into the current format, and the message says so. One that
/// does not resolve can only be settled by a human with `bind --project`. The fixes differ, so the kinds do
/// too.
fn legacy_pointer_issues(store: &Store) -> Vec<DoctorIssue> {
    crate::binding::legacy_pointers(store)
        .into_iter()
        .map(|p| {
            let path = std::path::Path::new(&p.dir).join(".amenbo").display().to_string();
            match p.recoverable {
                // A repair works on a folder (re-binding takes a dir), so the dir is handed over alongside
                // the `.amenbo` path rather than in place of it.
                Some(pid) => DoctorIssue::new(
                    DoctorIssueKind::LegacyPointer,
                    &path,
                    &[("path", &path), ("dir", &p.dir), ("project", &pid.to_string())],
                ),
                None => DoctorIssue::new(
                    DoctorIssueKind::LegacyPointerAmbiguous,
                    &path,
                    &[("path", &path)],
                ),
            }
        })
        .collect()
}

/// Warns about a bound folder that still exists but whose **`.amenbo` has vanished**.
///
/// The recovery inside `init` only covers the case where someone runs it **in that folder** — so a folder
/// nobody runs it in is repaired by nobody, and that residue is what this looks for. If the folder resolves
/// to a single project, the message says running `init` there will restore it; if it does not, a human
/// settles it with `bind --project`.
fn missing_pointer_issues(store: &Store) -> Vec<DoctorIssue> {
    crate::binding::missing_pointers(store)
        .into_iter()
        .map(|m| match m.claimed_by[..] {
            [pid] => DoctorIssue::new(
                DoctorIssueKind::MissingPointer,
                &m.dir,
                &[("dir", &m.dir), ("project", &pid.to_string())],
            ),
            _ => {
                let claims = m
                    .claimed_by
                    .iter()
                    .map(|pid| crate::idref::project(*pid))
                    .collect::<Vec<_>>()
                    .join(", ");
                DoctorIssue::new(
                    DoctorIssueKind::MissingPointerAmbiguous,
                    &m.dir,
                    &[("dir", &m.dir), ("claims", &claims)],
                )
            }
        })
        .collect()
}

/// Warns about a folder row no live project claims — the debris a deleted project left in the index. Nothing
/// here touches the folder's contents or its `.amenbo`, so there is exactly one repair to offer: forget the
/// row ([`Store::forget_orphan_dirs`], which both the CLI's `doctor --fix` and the GUI's repair call).
fn orphan_binding_issues(store: &Store) -> Vec<DoctorIssue> {
    crate::binding::orphan_dirs(store)
        .into_iter()
        .map(|dir| DoctorIssue::new(DoctorIssueKind::OrphanBinding, &dir, &[("dir", &dir)]))
        .collect()
}

/// Warns about a live project **no folder on this machine leads to** (`AMB-D-533`). A project is reached
/// through the folder someone stands in, so one with no folder is a list nothing can operate: an AI cannot
/// see it at all (`AMB-D-222`), and every command that takes its project from the current directory lands
/// somewhere else. Unbinding the last folder is allowed and says so on the spot (`AMB-D-530`), which covers
/// the moment it happens and nothing after it — this is the standing check.
///
/// A folder recorded but **gone** counts as none: the registry is a best-effort index, and a path that has
/// vanished leads nowhere. That is the same measure the project's own board takes, so the warning a person
/// meets on one board and the list they read here say the same thing (`AMB-D-535` gives the list to `doctor`).
///
/// Archived projects are left out. Archiving is what keeps a record you no longer work on, so having no
/// folder is exactly what is expected of one, not a fault to report.
fn project_without_folder_issues(store: &Store) -> crate::error::Result<Vec<DoctorIssue>> {
    let registry = store.bindings();
    Ok(store
        .project_list(false)?
        .projects
        .into_iter()
        .filter(|p| {
            registry
                .dirs_for_project(p.id)
                .into_iter()
                .all(|dir| !std::path::Path::new(dir).is_dir())
        })
        .map(|p| {
            DoctorIssue::new(
                DoctorIssueKind::ProjectWithoutFolder,
                format!("project:{}", p.id),
                &[("project", &p.id.to_string()), ("name", &p.name)],
            )
        })
        .collect())
}

/// Warns about a bound folder that **does not start its AI on Amenbo** (`AMB-D-535`). An AI launched
/// there reads the guidance only if it happens to read the managed block; the session-start hook is what
/// makes it read at every session, and Amenbo writes no settings file, so the wiring lands only when a
/// person hands their AI the text (`AMB-D-440`).
///
/// The warning already printed on every command can only name a tool the folder shows a trace of — a
/// standing line about a tool a reader does not use is one they cannot act on. A folder that traces
/// nothing therefore never gets a line at all, and that is exactly the folder nobody comes back to. Here
/// there is room for both, which is why the sweep is the place the whole list is read
/// (the GUI's is its project settings screen).
///
/// The two kinds split where the fix does: one names the tools waiting, the other can only ask the reader
/// which one they use. What silences either is core's own judgement ([`crate::harness::setup_notice`]) —
/// a refusal, or the wiring landing — and a project that said no stays silent here too.
///
/// Walked per project, because consent is recorded per project; a folder claimed by two of them is
/// reported once, by the first that still has something to say. Folders that are gone are skipped: there
/// is nothing to paste into one.
fn unwired_folder_issues(store: &Store) -> crate::error::Result<Vec<DoctorIssue>> {
    use crate::harness;
    let cmd = crate::config::Paths::command_name();
    let registry = store.bindings();
    let mut seen = std::collections::BTreeSet::new();
    let mut issues = Vec::new();
    for project in store.project_list(false)?.projects {
        let consent = store.harness_consent(project.id)?;
        for dir in registry.dirs_for_project(project.id) {
            if seen.contains(dir) || !std::path::Path::new(dir).is_dir() {
                continue;
            }
            let Some(notice) = harness::setup_notice(&harness::probe(std::path::Path::new(dir), cmd), consent)
            else {
                continue;
            };
            seen.insert(dir);
            issues.push(match notice.unwired.as_slice() {
                [] => DoctorIssue::new(DoctorIssueKind::UnwiredFolderAmbiguous, dir, &[("dir", dir)]),
                named => {
                    // The token `agent-hook snippet` takes, not the product's own spelling: what the
                    // sentence names is what the reader types next.
                    let tools = named.iter().map(|one| one.id).collect::<Vec<_>>().join(", ");
                    DoctorIssue::new(DoctorIssueKind::UnwiredFolder, dir, &[("dir", dir), ("tools", &tools)])
                }
            });
        }
    }
    Ok(issues)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmp(tag: &str) -> PathBuf {
        let p = amenbo_scratch::scratch(&format!("doctor-{tag}"));
        p
    }

    /// A store holding one project **with a folder** — a store nothing is wrong with. Two things make it
    /// that: a project no folder leads to is itself an issue ([`project_without_folder_issues`]), and a
    /// folder that does not start its AI on Amenbo is another ([`unwired_folder_issues`]), so a fixture
    /// missing either would put every test that starts from "nothing raised yet" a warning down. The
    /// wiring is answered with a refusal rather than a settings file: it is the shortest way to say this
    /// fixture is not about the hook.
    fn store_with_project(tag: &str) -> (Store, i64) {
        let mut store = Store::open_at(crate::config::Paths::at(tmp(&format!("home-{tag}")))).unwrap();
        let project = store
            .project_add(crate::ops::project::NewProject {
                name: "案件X".to_string(),
                view: crate::model::View::Board,
                notes: String::new(),
                color: None,
            })
            .unwrap();
        let id = project.id;
        store.set_harness_consent(id, crate::harness::Consent::answered(false)).unwrap();
        bind_folder(&store, id, &format!("{tag}-home-dir"));
        (store, id)
    }

    /// Bind a fresh folder to the project the way a real bind does — the pointer written and the registry
    /// row recorded — so no environment check reads it as broken. Returns the folder.
    fn bind_folder(store: &Store, project_id: i64, tag: &str) -> PathBuf {
        let dir = tmp(tag);
        crate::binding::pointer_for(store, project_id).write(&dir).unwrap();
        let mut registry = store.bindings();
        registry.record_project_ref(project_id, dir.to_string_lossy());
        store.save_bindings(&registry).unwrap();
        dir
    }

    /// A kind is part of the `--json` machine contract and, at the same time, the template id a surface looks
    /// up (the CLI's English sentences, the GUI's i18n table). Pinning the set catches a kind that sprouted
    /// or got renamed before it can drift away from those wording tables.
    #[test]
    fn doctor_issue_kind_registry_is_the_full_fixed_set() {
        let all: Vec<_> = DoctorIssueKind::ALL.iter().map(|k| k.as_str()).collect();
        assert_eq!(
            all,
            [
                "self_dependency",
                "duplicate_order_key",
                "orphan_attachment",
                "stale_managed_block",
                "legacy_pointer",
                "legacy_pointer_ambiguous",
                "missing_pointer",
                "missing_pointer_ambiguous",
                "orphan_binding",
                "dead_ref",
                "start_after_due",
                "project_without_folder",
                "unwired_folder",
                "unwired_folder_ambiguous",
            ]
        );
    }

    /// A folder whose AI does not start on Amenbo is raised until the wiring lands or the reader says no —
    /// and which of the two kinds it is turns on whether the folder shows any sign of the tool used there,
    /// because that is what decides whether the fix can name one.
    #[test]
    fn a_folder_whose_ai_does_not_start_on_amenbo_is_raised_until_it_is_answered() {
        let (store, pid) = store_with_project("unwired");
        let of_kind = |store: &Store, kind: DoctorIssueKind| -> Vec<DoctorIssue> {
            report(store).unwrap().issues.into_iter().filter(|i| i.kind == kind).collect()
        };
        // The fixture answered the question with a no, which is one of the two things that end this.
        assert!(of_kind(&store, DoctorIssueKind::UnwiredFolderAmbiguous).is_empty());

        store.clear_harness_consent(pid).unwrap();
        let raised = of_kind(&store, DoctorIssueKind::UnwiredFolderAmbiguous);
        assert_eq!(raised.len(), 1, "unanswered, the bound folder is raised: {raised:?}");
        assert_eq!(raised[0].severity, "warning", "an unwired folder works exactly as it always did");
        assert!(
            of_kind(&store, DoctorIssueKind::UnwiredFolder).is_empty(),
            "with no trace of a tool, nothing can be named",
        );

        // The folder now shows it is used with a tool from the catalog, still unwired: the sentence can
        // name what the reader is waiting on.
        let tool = &crate::harness::HARNESSES[0];
        let dir = store.bindings().dirs_for_project(pid)[0].to_string();
        std::fs::create_dir_all(std::path::Path::new(&dir).join(tool.home)).unwrap();
        let named = of_kind(&store, DoctorIssueKind::UnwiredFolder);
        assert_eq!(named.len(), 1, "the traced tool is what gets named: {named:?}");
        assert_eq!(named[0].params.get("tools").map(String::as_str), Some(tool.id));
        assert_eq!(named[0].params.get("dir").map(String::as_str), Some(dir.as_str()));
        assert!(
            of_kind(&store, DoctorIssueKind::UnwiredFolderAmbiguous).is_empty(),
            "a folder is one or the other, never both",
        );

        // The other thing that ends it: a refusal. Consent is not wiring, so only a no silences it here.
        store.set_harness_consent(pid, crate::harness::Consent::answered(true)).unwrap();
        assert_eq!(of_kind(&store, DoctorIssueKind::UnwiredFolder).len(), 1, "a yes is not the wiring");
        store.set_harness_consent(pid, crate::harness::Consent::answered(false)).unwrap();
        assert!(of_kind(&store, DoctorIssueKind::UnwiredFolder).is_empty(), "a no ends the report");
    }

    /// A project no folder leads to cannot be operated by anyone, and the board that says so is only read by
    /// whoever opens it — so the machine-wide list is `doctor`'s. It is raised for a project whose folders
    /// have all vanished just as for one that never had any: a path that is gone leads nowhere either.
    #[test]
    fn a_project_no_folder_leads_to_is_raised_until_one_does() {
        let (store, pid) = store_with_project("no-folder");
        let of_kind = |store: &Store| -> Vec<DoctorIssue> {
            report(store)
                .unwrap()
                .issues
                .into_iter()
                .filter(|i| i.kind == DoctorIssueKind::ProjectWithoutFolder)
                .collect()
        };
        assert!(of_kind(&store).is_empty(), "the fixture project has a folder, so nothing is raised");

        // The folder is taken off, as `unbind` leaves it: the project is still there, with nowhere to
        // operate it from.
        let mut registry = store.bindings();
        for dir in registry.dirs_for_project(pid).into_iter().map(str::to_string).collect::<Vec<_>>() {
            registry.forget_dir(&dir);
        }
        store.save_bindings(&registry).unwrap();

        let raised = of_kind(&store);
        assert_eq!(raised.len(), 1, "one project, one warning: {raised:?}");
        assert_eq!(raised[0].severity, "warning", "the store is intact — only the way in is gone");
        assert_eq!(raised[0].target, format!("project:{pid}"));
        assert_eq!(
            (raised[0].params.get("project").map(String::as_str), raised[0].params.get("name").map(String::as_str)),
            (Some(pid.to_string().as_str()), Some("案件X")),
            "it carries what a surface needs to name the project a reader has to go and bind",
        );

        // A folder recorded but gone leads nowhere, so it does not answer the warning.
        let vanished = tmp("no-folder-vanished");
        let mut registry = store.bindings();
        registry.record_project_ref(pid, vanished.to_string_lossy());
        store.save_bindings(&registry).unwrap();
        std::fs::remove_dir_all(&vanished).unwrap();
        assert_eq!(of_kind(&store).len(), 1, "a folder that is gone is not a folder to work in");

        bind_folder(&store, pid, "no-folder-again");
        assert!(of_kind(&store).is_empty(), "once a folder leads to it again, the warning is gone");
    }

    /// A project you have stopped working on is expected to have no folder — archiving is what keeps the
    /// record — so it is not something to be told about every time `doctor` runs.
    #[test]
    fn an_archived_project_is_not_asked_for_a_folder() {
        let (mut store, pid) = store_with_project("archived");
        let mut registry = store.bindings();
        for dir in registry.dirs_for_project(pid).into_iter().map(str::to_string).collect::<Vec<_>>() {
            registry.forget_dir(&dir);
        }
        store.save_bindings(&registry).unwrap();
        assert_eq!(
            report(&store).unwrap().issues.iter().filter(|i| i.kind == DoctorIssueKind::ProjectWithoutFolder).count(),
            1,
            "while it is live, the missing folder is raised",
        );

        store.project_set_archived(pid, true).unwrap();
        assert_eq!(
            report(&store).unwrap().issues.iter().filter(|i| i.kind == DoctorIssueKind::ProjectWithoutFolder).count(),
            0,
            "archived, it is a record rather than work in flight",
        );
    }

    /// The doctor surface gathers **both the store's insides and the environment** into one list — and
    /// because the gathering lives in core, the CLI and the GUI raise the same issues. This checks that the
    /// environment side (a folder row nobody claims) really does ride along, and that it goes away once the
    /// repair (`forget_orphan_dirs`) has run.
    #[test]
    fn the_report_carries_the_environment_issues_the_db_check_cannot_see() {
        let (store, pid) = store_with_project("report");
        let clean = report(&store).unwrap();
        assert!(clean.ok);
        assert!(clean.issues.is_empty(), "a bare store raises no issues: {:?}", clean.issues);

        // The same shape as the debris a deleted project leaves in the index: no project row with that id
        // exists in the store.
        let ghost = tmp("report-ghost-dir");
        let mut registry = store.bindings();
        registry.record_project_ref(pid + 1_000, ghost.to_string_lossy());
        store.save_bindings(&registry).unwrap();

        let dirty = report(&store).unwrap();
        let orphan: Vec<_> = dirty
            .issues
            .iter()
            .filter(|i| i.kind == DoctorIssueKind::OrphanBinding)
            .collect();
        assert_eq!(orphan.len(), 1, "the leftover folder row is raised, once: {:?}", dirty.issues);
        assert_eq!(orphan[0].severity, "warning");
        assert_eq!(
            orphan[0].params.get("dir").map(String::as_str),
            Some(ghost.to_string_lossy().as_ref()),
            "it carries what a surface needs to compose a sentence (which folder)",
        );
        assert!(dirty.ok, "an environment issue is a warning, so it does not topple ok");
        assert_eq!(dirty.summary.warning, 1);

        assert_eq!(store.forget_orphan_dirs().unwrap(), 1);
        assert!(report(&store).unwrap().issues.is_empty(), "once repaired, it is gone");
    }

    /// The pointer issues shown by [`report`] (the doctor surface) and by the GUI's startup health banner come
    /// from the **same** [`pointer_issues`] — write the detection once per surface and what they show will
    /// eventually diverge.
    #[test]
    fn the_pointer_issues_the_startup_banner_shows_are_the_ones_the_doctor_face_shows() {
        let (store, pid) = store_with_project("pointers");
        assert!(pointer_issues(&store).is_empty(), "a bare store raises no pointer issues");

        // The folder is there but its `.amenbo` is gone, so an AI started in it resolves to no project.
        let dir = tmp("pointers-missing");
        let mut registry = store.bindings();
        registry.record_project_ref(pid, dir.to_string_lossy());
        store.save_bindings(&registry).unwrap();

        let found = pointer_issues(&store);
        assert_eq!(found.len(), 1, "the pointer that went missing is raised, once: {found:?}");
        assert_eq!(found[0].kind, DoctorIssueKind::MissingPointer);
        assert_eq!(
            found[0].params.get("project").map(String::as_str),
            Some(pid.to_string().as_str()),
            "with a single claimant it carries enough for a surface to say \"init there and it is fixed\"",
        );

        let from_face: Vec<_> = report(&store)
            .unwrap()
            .issues
            .into_iter()
            .filter(|i| i.kind == DoctorIssueKind::MissingPointer)
            .collect();
        assert_eq!(from_face.len(), 1);
        assert_eq!(from_face[0].target, found[0].target, "what gets raised does not drift from surface to surface");
        assert_eq!(from_face[0].params, found[0].params);
    }
}
