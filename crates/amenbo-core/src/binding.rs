//! Directory ↔ project bindings. **Local, never synced.**
//!
//! - `.amenbo` (the dir→project **pointer**): a marker dropped in the current directory (or found by
//!   walking upward). Its `project_id` names the current project; there is only one store, so the
//!   pointer plays no part in selecting one. **Store contents and secrets never live here** — it is a
//!   small static JSON file, safe even inside an iCloud-synced tree. Several directories may point at
//!   the same `project_id` (many-to-one).
//! - The binding registry ([`Registry`]: project→dir and back): the local record behind "work on this
//!   project in this folder". It lives in the consolidated store's `binding_path` /
//!   `binding_project_dir` tables ([`crate::overview`]). If the recorded path has vanished we return
//!   `binding_stale` rather than **silently operating somewhere else**.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::store::Store;

/// The contents of a `.amenbo` pointer.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DirBinding {
    pub v: u32,
    /// Primary key (INTEGER) of the bound project; `None` while it is still undecided (right after
    /// `init`, say). Exposing an internal primary key as an external identifier is safe here because
    /// there is a single store and the key is stable — the one dangerous case, "export, take the data
    /// into another environment, and the id now means something else", is exactly what the
    /// [`DirBinding::slug`] cross-check catches. Unknown keys in the pointer are **skipped silently**
    /// by serde's default: an old pointer still parses, and the stale keys disappear on the next write.
    #[serde(default, deserialize_with = "integer_id_or_none")]
    pub project_id: Option<i64>,
    /// The project's human-readable identifier ([`crate::slug`]). **It is a cross-check, not a
    /// reference** — resolution always goes through `project_id`. The slug exists only so that, when it
    /// disagrees with the project the id names, we can tell that this pointer belongs to a different
    /// store (its id points at something else entirely). See [`DirBinding::mismatched_slug`].
    #[serde(default)]
    pub slug: Option<String>,
}

/// Read `project_id`: take integers as they are, and map **anything that is not an integer to `None`**
/// instead of failing the whole pointer. Such a pointer is treated as "binding undecided" and left to
/// the bindings registry to recover and rewrite on the next visit.
fn integer_id_or_none<'de, D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Option<i64>, D::Error> {
    Ok(match Option::<serde_json::Value>::deserialize(d)? {
        Some(serde_json::Value::Number(n)) => n.as_i64(),
        _ => None,
    })
}

impl DirBinding {
    pub fn new(project_id: Option<i64>, slug: Option<String>) -> DirBinding {
        DirBinding { v: 1, project_id, slug }
    }

    /// If the recorded slug disagrees with the real slug (`actual`) of the project `project_id` names,
    /// return the recorded value — the material for a warning. **The id wins**, so resolution is never
    /// blocked; the job here is only to tell the human that this folder's pointer came from a different
    /// store. With either side missing there is nothing to compare, so we return `None`.
    pub fn mismatched_slug(&self, actual: Option<&str>) -> Option<&str> {
        let recorded = self.slug.as_deref()?;
        let actual = actual?;
        (recorded != actual).then_some(recorded)
    }

    /// Write the pointer to `<dir>/.amenbo`.
    pub fn write(&self, dir: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(dir.join(".amenbo"), json.as_bytes())?;
        Ok(())
    }
}

/// Read the `.amenbo` of **that directory itself** (never walking upward). Every surface that inspects
/// the pointer of a known folder — the folder list's cross-check, [`legacy_pointers`] — goes through
/// here: with [`find_upward`], a folder that has no `.amenbo` would pick up an ancestor's pointer and
/// report it as its own.
pub fn read_pointer(dir: &Path) -> Option<DirBinding> {
    let raw = std::fs::read_to_string(dir.join(".amenbo")).ok()?;
    serde_json::from_str::<DirBinding>(&raw).ok()
}

/// Walk upward from `start` looking for a `.amenbo`, and return `(the directory holding it, its
/// contents)`.
pub fn find_upward(start: &Path) -> Option<(PathBuf, DirBinding)> {
    let mut cur = Some(start);
    while let Some(dir) = cur {
        if let Some(b) = read_pointer(dir) {
            return Some((dir.to_path_buf(), b));
        }
        cur = dir.parent();
    }
    None
}

/// Return the nearest `.amenbo` **strictly above** `start` (excluding `start` itself). This is how
/// nested bindings are detected: running `init`/`bind` in a subdirectory of an already
/// managed tree drops a new pointer that **shadows** the binding above it — amenbo run in that subdir
/// would resolve to the subdir's project rather than the parent's. `find_upward` would also pick up
/// `start`'s own `.amenbo`, so to keep re-binding (repointing the same folder at another project)
/// distinguishable from nesting, we search from the parent up.
pub fn find_upward_ancestor(start: &Path) -> Option<(PathBuf, DirBinding)> {
    find_upward(start.parent()?)
}

// ───────── Resolving pointers against the store (compat reads + lazy rewrites) ─────────

/// The pointer to write into `.amenbo`: the project's primary key plus the slug we cross-check against.
/// The slug is **taken from the store** — if the writer of a pointer could put an arbitrary string
/// there, the cross-check ([`DirBinding::mismatched_slug`]) would mean nothing.
pub fn pointer_for(store: &Store, project_id: i64) -> DirBinding {
    let slug = store.project(project_id).ok().flatten().and_then(|p| p.slug);
    DirBinding::new(Some(project_id), slug)
}

/// The slug recorded in `.amenbo` disagrees with the project its `project_id` names — this pointer came
/// from a different store (the folder was copied wholesale, or brought in from another environment) and
/// its id may quietly name something else. **Resolution is not blocked** (the id wins): this is
/// material for telling the human, not grounds for refusing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlugMismatch {
    /// Primary key of the project the pointer names.
    pub project_id: i64,
    /// The slug that was written in `.amenbo`.
    pub recorded: String,
    /// The real slug of the project `project_id` names (it may have none).
    pub actual: Option<String>,
}

/// Cross-check a pointer against the store — the single predicate behind both the CLI's warning and the
/// GUI's folder list. Nothing is compared (`None`) when the project it names is not alive, or when
/// either side has no slug.
pub fn slug_mismatch(store: &Store, binding: &DirBinding) -> Option<SlugMismatch> {
    let project_id = binding.project_id?;
    let actual = store.project(project_id).ok().flatten()?.slug;
    let recorded = binding.mismatched_slug(actual.as_deref())?.to_string();
    Some(SlugMismatch { project_id, recorded, actual })
}

/// Of the projects that the registry's reverse lookup ([`Registry::projects_for_dir`]) says claim this
/// folder, only those that **still read back** from the store (ascending). A deleted project's rows are
/// physically gone while its registry entry is only released by a best-effort teardown, so an entry can
/// outlive the project it names; such an entry is not an owner and is dropped — otherwise we would write
/// back a pointer naming a project that is no longer there.
/// Zero results means no project claims the folder; exactly one means the owner is unambiguous (we can
/// recover and upgrade the pointer); several means it is ambiguous and the caller must stop and offer
/// the candidates.
pub fn live_projects_claiming(store: &Store, dir: &Path) -> Vec<i64> {
    store
        .bindings()
        .projects_for_dir(&dir.to_string_lossy())
        .into_iter()
        .filter(|pid| store.project(*pid).ok().flatten().is_some())
        .collect()
}

/// [`find_upward`] plus **compatibility reads of old pointers and a lazy rewrite**. Every surface that
/// reads a pointer (the CLI's `bound_project`, the location header, what `bind` prints) goes through
/// here. `.amenbo` files are scattered across the filesystem — unmounted, on external drives, deleted —
/// and cannot be enumerated reliably from app-data, so a pointer whose `project_id` does not read as an
/// integer is resolved through the registry's reverse lookup ([`live_projects_claiming`]). We take that
/// answer **only when it lands on exactly one live project**; zero or several are returned as still
/// undecided rather than silently picking a project (`doctor`'s [`legacy_pointers`] surfaces those).
/// Once resolved, the pointer is rewritten in place into the current shape (`project_id` + `slug`), so
/// it heals the next time amenbo runs in that folder. The write is best-effort: on a read-only
/// filesystem we resolve, give up quietly, and do not fail a read command. The compatibility read has
/// no expiry date — folders cannot be enumerated, so there is no way to know that the last old pointer
/// is gone, and the cost is bounded at a single reverse lookup taken only when `project_id` is `None`.
/// The same moment is used to bring a stale managed block in the resolved folder's `CLAUDE.md` /
/// `AGENTS.md` up to the current version ([`crate::agents::follow_stale_block`]): leftovers on the
/// filesystem can only be fixed when amenbo actually runs in that folder, and since every surface
/// resolves `.amenbo` through here, one hook suffices.
pub fn resolve_upward(store: &Store, start: &Path) -> Option<(PathBuf, DirBinding)> {
    let (dir, binding) = find_upward(start)?;
    crate::agents::follow_stale_block(&dir, crate::config::Paths::APP_NAME);
    if binding.project_id.is_some() {
        return Some((dir, binding));
    }
    let [project_id] = live_projects_claiming(store, &dir)[..] else {
        return Some((dir, binding));
    };
    let upgraded = pointer_for(store, project_id);
    let _ = upgraded.write(&dir);
    Some((dir, upgraded))
}

/// Is this folder's `.amenbo` in the **old shape** (its `project_id` does not read as an integer)? No
/// pointer, or a current one, gives false. Both `doctor`'s bulk scan ([`legacy_pointers`]) and the
/// GUI's folder list (inspecting one row at a time) decide "legacy" through this one predicate.
pub fn is_legacy_pointer(dir: &Path) -> bool {
    read_pointer(dir).is_some_and(|b| b.project_id.is_none())
}

/// Does this folder exist while its `.amenbo` is **gone**? The registry still records the folder as
/// bound, yet an AI started there will not resolve to that project (it climbs to an ancestor, or falls
/// back to recovery via `init`). A vanished (stale) folder is false — saying "the pointer is missing"
/// gets us nowhere when the folder itself is missing; that is reported on its own. Both `doctor`'s bulk
/// scan ([`missing_pointers`]) and the GUI's folder list decide this through one predicate.
pub fn is_pointer_missing(dir: &Path) -> bool {
    dir.is_dir() && read_pointer(dir).is_none()
}

/// A folder this machine records as bound ([`Registry::all_dirs`]) that still exists while its
/// `.amenbo` has **vanished**. `doctor` detects these and prompts for a re-link. `claimed_by` holds the
/// live projects the registry's reverse lookup ([`live_projects_claiming`]) says claim this folder
/// (ascending). One of them means the owner is unambiguous — running `init` in that folder restores the
/// pointer, and `doctor` can say so. Several means a human has to settle it with `bind --project`.
/// **Zero is not listed at all**: if no project claims the folder, saying "the pointer is missing"
/// leaves nothing to re-link to. Such a row is registry debris, not a stray folder, so [`orphan_dirs`]
/// reports it under its own name and `doctor --fix` drops it from the index. The scan relies on a
/// best-effort index (the registry), so **a folder absent from this list may still have lost its
/// pointer** — the same limit as [`legacy_pointers`].
#[derive(Debug, Clone)]
pub struct MissingPointer {
    pub dir: String,
    pub claimed_by: Vec<i64>,
}

/// The folders this machine records as bound, restricted to **what the current reach can see**. With an
/// unrestricted reach (human / GUI) that is every folder; with a closed reach it is **only the folders
/// that claim that project**. Every check of the environment — [`legacy_pointers`],
/// [`missing_pointers`], [`orphan_dirs`], and the `doctor` issues and `doctor --fix` repairs built on
/// them — starts here: a folder path is itself evidence that a project outside the reach exists, so the
/// narrowing happens once at the entrance to the listing instead of being scattered across each check.
/// A folder no project claims (the subject of [`orphan_dirs`]) belongs to no project, and so is
/// invisible from a closed reach.
pub(crate) fn dirs_in_reach(store: &Store) -> Vec<String> {
    let dirs = store.bindings().all_dirs();
    let Some(pid) = store.reach().project() else {
        return dirs;
    };
    dirs.into_iter()
        .filter(|dir| live_projects_claiming(store, Path::new(dir)).contains(&pid))
        .collect()
}

pub fn missing_pointers(store: &Store) -> Vec<MissingPointer> {
    dirs_in_reach(store)
        .into_iter()
        .filter(|dir| is_pointer_missing(Path::new(dir)))
        .filter_map(|dir| {
            let claimed_by = live_projects_claiming(store, Path::new(&dir));
            (!claimed_by.is_empty()).then_some(MissingPointer { dir, claimed_by })
        })
        .collect()
}

/// What a bulk repair of broken pointers did. `repaired` lists the folders whose pointer was rewritten
/// or written back in the current shape; `unresolved` lists the folders we left alone because their
/// owner was not unambiguous (zero or several claimants — a human has to settle it with
/// `bind --project`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PointerRepair {
    pub repaired: Vec<String>,
    pub unresolved: Vec<String>,
}

/// Fix broken pointers — old-shape ([`legacy_pointers`]) and vanished ([`missing_pointers`]) alike —
/// **without going to those folders**; both the GUI banner and `doctor --fix` call this. It does in one
/// pass exactly what [`resolve_upward`] would do the next time amenbo ran in each folder. The only
/// thing written is each folder's `.amenbo`: amenbo's own file, in a place the user explicitly bound.
/// Folders whose owner is not unambiguous are left untouched — we never silently pick a project, the
/// same discipline as [`resolve_upward`].
pub fn repair_pointers(store: &Store) -> PointerRepair {
    let legacy = legacy_pointers(store).into_iter().map(|p| (p.dir, p.recoverable));
    let missing = missing_pointers(store)
        .into_iter()
        .map(|p| (p.dir, if let [pid] = p.claimed_by[..] { Some(pid) } else { None }));
    let mut repair = PointerRepair::default();
    for (dir, owner) in legacy.chain(missing) {
        let Some(project_id) = owner else {
            repair.unresolved.push(dir);
            continue;
        };
        match pointer_for(store, project_id).write(Path::new(&dir)) {
            Ok(()) => repair.repaired.push(dir),
            // Could not write (read-only filesystem, permissions) — we did not fix it, so we do not
            // claim we did.
            Err(_) => repair.unresolved.push(dir),
        }
    }
    repair
}

/// The folders this machine records as bound ([`Registry::all_dirs`]) that **no live project claims** —
/// debris a deleted project left in the index. Whether the folder still exists is irrelevant: present
/// or gone, with no claimant it is debris. No misresolution follows from these rows, because every
/// reader filters by live projects
/// ([`live_projects_claiming`]) — but the rows do persist. [`missing_pointers`] discards
/// zero-claimant folders, and deletion teardown ([`crate::project_teardown`]) only unhooks the folders
/// of the project being deleted, so a row left behind by an interrupted deletion is neither reported
/// nor cleaned by anyone. `doctor` lists them here, and `doctor --fix`
/// ([`crate::store::Store::forget_orphan_dirs`]) drops them from the index. Only **the index row** is
/// dropped: neither the folder's contents nor its `.amenbo` is touched, so if that folder happens to
/// hold a pointer of its own (an old-shape one lands in [`legacy_pointers`]) it is still reported
/// separately.
pub fn orphan_dirs(store: &Store) -> Vec<String> {
    dirs_in_reach(store)
        .into_iter()
        .filter(|dir| live_projects_claiming(store, Path::new(dir)).is_empty())
        .collect()
}

/// A folder this machine records as bound ([`Registry::all_dirs`]) whose `.amenbo` is in the **old
/// shape** (its `project_id` does not read as an integer). `doctor` detects these and prompts for a
/// rewrite; vanished folders, folders with no pointer, and current-shape pointers are skipped quietly.
/// `recoverable` says whether [`resolve_upward`] can upgrade the pointer on its own (whether it lands
/// on exactly one live project); `None` means a human has to settle it with `bind --project`, which
/// changes what `doctor` advises. The scan relies on a best-effort index (the registry), so **a folder
/// absent from this list may still hold an old pointer** — running amenbo in that folder lets
/// [`resolve_upward`] fix it.
#[derive(Debug, Clone)]
pub struct LegacyPointer {
    pub dir: String,
    pub recoverable: Option<i64>,
}

pub fn legacy_pointers(store: &Store) -> Vec<LegacyPointer> {
    dirs_in_reach(store)
        .into_iter()
        .filter(|dir| is_legacy_pointer(Path::new(dir)))
        .map(|dir| {
            let recoverable = match live_projects_claiming(store, Path::new(&dir))[..] {
                [pid] => Some(pid),
                _ => None,
            };
            LegacyPointer { dir, recoverable }
        })
        .collect()
}

/// The machine-local binding registry (an in-memory value type). Its durable home is the consolidated
/// store's binding tables ([`crate::overview`]). `paths` is project→dir, the main directory behind "work
/// on this project in this folder"; `project_dirs` is project→set of dirs, the many-to-one reverse
/// lookup over every folder that points at the project. Keys are project primary keys (`INTEGER`).
#[derive(Clone, Debug, Default)]
pub struct Registry {
    /// project_id → absolute path (as a string).
    pub paths: BTreeMap<i64, String>,
    /// project_id → the set of `.amenbo` directories (absolute path strings) that point at that
    /// project. Separately from `paths` (the single main directory behind "work on this project in this
    /// folder"), this collects **every** folder pointing at the project — the many-to-one reverse
    /// lookup the settings screen's folder list is built on. `.amenbo` files are scattered across the
    /// filesystem and cannot be enumerated from app-data, so we gather them here at bind time. It is a
    /// best-effort index: paths that have vanished are judged stale by readers, via an existence check.
    pub project_dirs: BTreeMap<i64, BTreeSet<String>>,
}

impl Registry {
    /// Repoint the main directory (the single `paths` slot). **Before overwriting, stash the previous
    /// main directory into `project_dirs`** (the many-to-one reverse-lookup set) — otherwise binding an
    /// extra folder to an existing project would have `set` silently discard the old main directory,
    /// and if that folder lived only in `paths` (never recorded in `project_dirs`) it would disappear
    /// from the union `dirs_for_project` returns. Stashing keeps the old folder in the listing even on
    /// the first extra bind (`record_project_ref` is idempotent, so a duplicate collapses).
    pub fn set(&mut self, project_id: i64, dir: impl Into<String>) {
        if let Some(prev) = self.paths.insert(project_id, dir.into()) {
            self.project_dirs.entry(project_id).or_default().insert(prev);
        }
    }

    pub fn get(&self, project_id: i64) -> Option<&str> {
        self.paths.get(&project_id).map(|s| s.as_str())
    }

    /// Record that `dir` holds a `.amenbo` pointing at `project_id` (many-to-one: several folders may
    /// point at one project). Idempotent — it is a set. Every bind path calls this alongside writing the
    /// pointer and `set`ting the main directory.
    pub fn record_project_ref(&mut self, project_id: i64, dir: impl Into<String>) {
        self.project_dirs.entry(project_id).or_default().insert(dir.into());
    }

    /// The directories recorded as pointing at `project_id` (ascending, deduped, empty if none) — the
    /// project→folders reverse lookup. It returns the **union** of `project_dirs` (the many-to-one set)
    /// and `paths` (the single main directory). The two are a redundant pair that ought to be updated
    /// together, but a write path can drift and fill only `paths` (a forgotten `record_project_ref`),
    /// and looking at `project_dirs` alone would then miss a folder whose `.amenbo` really exists.
    /// Taking the union enumerates every bound folder no matter which index recorded it. `forget_dir`
    /// removes a folder from both indexes, so an unbound folder never reappears here.
    pub fn dirs_for_project(&self, project_id: i64) -> Vec<&str> {
        let mut dirs: BTreeSet<&str> = self
            .project_dirs
            .get(&project_id)
            .map(|s| s.iter().map(String::as_str).collect())
            .unwrap_or_default();
        if let Some(main) = self.paths.get(&project_id) {
            dirs.insert(main.as_str());
        }
        dirs.into_iter().collect()
    }

    /// **Every folder** this machine records as amenbo-managed (ascending, deduped): the union of both
    /// indexes, `paths` (main directories) and `project_dirs` (project→folders). `doctor`'s stale
    /// managed-block detection walks this set; paths that have vanished are skipped quietly by the
    /// walker's existence check.
    pub fn all_dirs(&self) -> Vec<String> {
        let mut dirs: BTreeSet<String> = self.paths.values().cloned().collect();
        for set in self.project_dirs.values() {
            dirs.extend(set.iter().cloned());
        }
        dirs.into_iter().collect()
    }

    /// Forget every binding record for `dir` (this is what `unbind` uses). The folder is removed from
    /// each `project_dirs` set (project keys left empty are cleaned up) and from every `paths` entry
    /// pointing at it. Returns the number of records removed — zero means the folder was not recorded
    /// anywhere, so the call is idempotent. Bindings are many-to-one, so records for **other folders**
    /// of the same project are never touched.
    pub fn forget_dir(&mut self, dir: &str) -> usize {
        let mut removed = 0usize;
        for set in self.project_dirs.values_mut() {
            if set.remove(dir) {
                removed += 1;
            }
        }
        self.project_dirs.retain(|_, set| !set.is_empty());
        let before = self.paths.len();
        self.paths.retain(|_, p| p != dir);
        removed += before - self.paths.len();
        removed
    }

    /// **The path→project reverse lookup.** Returns the project ids recorded as pointing at the given
    /// folder (ascending, deduped, empty if none). It scans **both indexes** — `project_dirs` (the
    /// many-to-one set) and `paths` (the single main directory) — normalizing each through
    /// [`normalize_dir_for_match`], symmetrically with the union `dirs_for_project` takes. Pointer
    /// recovery uses it to work out, best-effort, which project a vanished `.amenbo` used to name; if
    /// the answer is not unique the caller collapses it to `None`.
    pub fn projects_for_dir(&self, dir: &str) -> Vec<i64> {
        let want = normalize_dir_for_match(dir);
        let mut ids: BTreeSet<i64> = self
            .project_dirs
            .iter()
            .filter(|(_, dirs)| dirs.iter().any(|d| normalize_dir_for_match(d) == want))
            .map(|(id, _)| *id)
            .collect();
        for (pid, p) in &self.paths {
            if normalize_dir_for_match(p) == want {
                ids.insert(*pid);
            }
        }
        ids.into_iter().collect()
    }

    /// Resolve a project's working directory. If a record exists but the path has vanished, this is
    /// `binding_stale`.
    pub fn resolve_dir(&self, project_id: i64) -> Result<Option<PathBuf>> {
        match self.get(project_id) {
            None => Ok(None),
            Some(p) => {
                let path = PathBuf::from(p);
                if path.is_dir() {
                    Ok(Some(path))
                } else {
                    Err(Error::BindingStale(p.to_string()))
                }
            }
        }
    }
}

/// Bring a folder-path string into a comparable normal form. The reverse lookup is keyed by id and
/// holds paths as values, so the values must be levelled before they are compared. An existing path is
/// `canonicalize`d (absorbing symlink differences, and the fact that not every bind path canonicalizes);
/// a path that does not exist — a stale pointer, a vanished folder — is taken lexically as a `PathBuf`,
/// which compares component by component and so absorbs a trailing slash. Recording always writes an
/// absolute path (via `current_dir()` / `canonicalize`), so there is no need to prefix relative paths:
/// this stays best-effort.
fn normalize_dir_for_match(dir: &str) -> PathBuf {
    let p = PathBuf::from(dir);
    std::fs::canonicalize(&p).unwrap_or(p)
}
#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let p = amenbo_scratch::scratch(&format!("bind-{tag}"));
        p
    }

    /// Bring up one store in an isolated app-data tree, create one project, and return
    /// `(store, project_id)`.
    fn store_with_project(tag: &str, name: &str) -> (Store, i64) {
        let home = tmp(&format!("home-{tag}"));
        let mut store = Store::open_at(crate::config::Paths::at(home)).unwrap();
        let project = store
            .project_add(crate::ops::project::NewProject {
                name: name.to_string(),
                view: crate::model::View::Board,
                notes: String::new(),
                color: None,
            })
            .unwrap();
        let id = project.id;
        (store, id)
    }

    #[test]
    fn dir_binding_round_trips_and_resolves_upward() {
        let root = tmp("upward");
        let nested = root.join("a").join("b");
        std::fs::create_dir_all(&nested).unwrap();
        DirBinding::new(Some(7), Some("amenbo".into())).write(&root).unwrap();

        // A new pointer carries nothing but the project's id and slug.
        let raw = std::fs::read_to_string(root.join(".amenbo")).unwrap();
        for gone in ["store_id", "account_id", "persona_id", "workspace_id"] {
            assert!(!raw.contains(gone), "new pointers carry no {gone}: {raw}");
        }
        // The id is written as a decimal integer, not quoted as a string.
        assert!(raw.contains("\"project_id\": 7"), "the id is written as an integer: {raw}");

        let (found_dir, b) = find_upward(&nested).expect("found upward");
        assert_eq!(found_dir, root);
        assert_eq!(b.project_id, Some(7));
        assert_eq!(b.slug.as_deref(), Some("amenbo"));
    }

    #[test]
    fn find_upward_ancestor_skips_the_dir_itself() {
        // A managed tree: bind root, then carve a subdir under it.
        let root = tmp("ancestor");
        let subdir = root.join("crates").join("amenbo-cli");
        std::fs::create_dir_all(&subdir).unwrap();
        DirBinding::new(Some(7), None).write(&root).unwrap();

        // Seen from the subdir, root's binding is an ancestor — nested-binding detection fires.
        let (dir, b) = find_upward_ancestor(&subdir).expect("ancestor binding found above subdir");
        assert_eq!(dir, root);
        assert_eq!(b.project_id, Some(7));

        // Seen from root itself there is no binding above it: its own `.amenbo` is excluded, so
        // re-binding the same folder stays allowed.
        assert!(
            find_upward_ancestor(&root).is_none(),
            "the dir's own pointer is excluded so re-binding the same folder is not treated as nested",
        );

        // Even when the subdir has a pointer of its own, we only look from the parent up.
        DirBinding::new(Some(8), None).write(&subdir).unwrap();
        let (dir2, _) = find_upward_ancestor(&subdir).expect("still finds the ancestor, not itself");
        assert_eq!(dir2, root, "find_upward_ancestor ignores the dir's own pointer");
    }

    #[test]
    fn dir_binding_reads_legacy_pointer_keys_it_no_longer_writes() {
        // The fields an old `.amenbo` carries (`store_id` / `workspace_id` / `account_id` /
        // `persona_id`) are **skipped silently as unknown keys** by serde's default, so the pointer
        // still parses. `project_id` does not read as an integer, so it collapses to undecided (None).
        let root = tmp("legacy-keys");
        std::fs::write(
            root.join(".amenbo"),
            r#"{"v":1,"store_id":"01ST","workspace_id":"01WS","account_id":"P0","persona_id":"P0","project_id":"01PJ"}"#,
        )
        .unwrap();
        let (_dir, b) = find_upward(&root).expect("found");
        assert_eq!(b.project_id, None, "a ULID id no longer resolves — the pointer still parses");

        // A pointer whose id is written as a decimal *string* is treated the same way: undecided.
        let decimal = tmp("legacy-decimal-string");
        std::fs::write(decimal.join(".amenbo"), r#"{"v":1,"project_id":"7"}"#).unwrap();
        let (_dir, b) = find_upward(&decimal).expect("found");
        assert_eq!(b.project_id, None);

        // A pointer without even a project field still reads — it simply binds no project.
        let bare = tmp("legacy-store-only");
        std::fs::write(bare.join(".amenbo"), r#"{"v":1,"store_id":"01ST"}"#).unwrap();
        let (_dir, b) = find_upward(&bare).expect("found");
        assert_eq!(b.project_id, None);
    }

    #[test]
    fn slug_mismatch_is_reported_only_when_both_sides_are_known() {
        let b = DirBinding::new(Some(7), Some("amenbo".into()));
        // They agree: nothing to report.
        assert_eq!(b.mismatched_slug(Some("amenbo")), None);
        // They disagree: return the recorded value — this id names something else, so the pointer came
        // from another store.
        assert_eq!(b.mismatched_slug(Some("wharfy")), Some("amenbo"));
        // The project has no slug, or the pointer recorded none: nothing to compare.
        assert_eq!(b.mismatched_slug(None), None);
        assert_eq!(DirBinding::new(Some(7), None).mismatched_slug(Some("amenbo")), None);
    }

    /// Cross-checking a pointer against the store ([`slug_mismatch`]) — the predicate behind both the
    /// CLI's warning and the GUI's folder list.
    #[test]
    fn a_pointer_from_another_store_is_reported_against_the_store() {
        let (store, pid) = store_with_project("mismatch", "案件X");
        let actual = store.project(pid).unwrap().unwrap().slug;

        // A pointer recording the same slug as the project: healthy, say nothing.
        assert_eq!(slug_mismatch(&store, &DirBinding::new(Some(pid), actual.clone())), None);

        // A pointer carried over from another store: its id is live, but its slug names a different
        // project.
        let m = slug_mismatch(&store, &DirBinding::new(Some(pid), Some("wharfy".into())))
            .expect("the mismatch is reported");
        assert_eq!(m, SlugMismatch { project_id: pid, recorded: "wharfy".into(), actual });

        // Nothing is compared when the id names no project in the store — reporting that absence is
        // not this function's job.
        assert_eq!(slug_mismatch(&store, &DirBinding::new(Some(pid + 999), Some("wharfy".into()))), None);
    }

    /// Deciding whether a pointer is old-shaped — the predicate both `doctor`'s bulk scan and the GUI's
    /// folder list go through.
    #[test]
    fn a_pointer_is_legacy_only_when_its_project_id_cannot_be_read() {
        let old = tmp("legacy-predicate-old");
        std::fs::write(old.join(".amenbo"), r#"{"v":1,"project_id":"01LEGACY"}"#).unwrap();
        assert!(is_legacy_pointer(&old));

        let current = tmp("legacy-predicate-current");
        DirBinding::new(Some(7), Some("amenbo".into())).write(&current).unwrap();
        assert!(!is_legacy_pointer(&current));

        // A folder with no pointer at all is not "old-shaped" — that is a question of recovery.
        assert!(!is_legacy_pointer(&tmp("legacy-predicate-none")));
    }

    /// [`read_pointer`] reads only that folder's own `.amenbo`: mistaking an ancestor's pointer for the
    /// folder's own would have the folder list report mismatches out of nowhere.
    #[test]
    fn read_pointer_does_not_climb_to_an_ancestor() {
        let parent = tmp("read-pointer-parent");
        DirBinding::new(Some(7), Some("amenbo".into())).write(&parent).unwrap();
        let child = parent.join("child");
        std::fs::create_dir_all(&child).unwrap();

        assert_eq!(read_pointer(&parent).unwrap().project_id, Some(7));
        assert!(read_pointer(&child).is_none(), "the child has no `.amenbo` of its own");
        assert_eq!(find_upward(&child).unwrap().0, parent, "climbing upward is find_upward's job");
    }

    /// An old `.amenbo` whose `project_id` does not read as an integer is **read compatibly** through
    /// the registry's reverse lookup, and **lazily rewritten** in place into the current shape (integer
    /// id + slug).
    #[test]
    fn a_legacy_pointer_resolves_through_the_registry_and_is_rewritten_in_place() {
        let (store, pid) = store_with_project("upgrade", "案件X");
        let dir = tmp("upgrade-dir");
        // A pointer whose id does not read as an integer looks, for now, like "no project bound".
        std::fs::write(dir.join(".amenbo"), r#"{"v":1,"project_id":"01LEGACY","store_id":"01ST"}"#).unwrap();
        assert_eq!(find_upward(&dir).unwrap().1.project_id, None, "an old pointer's id does not read");
        // Only the registry knows what this folder is bound to.
        let mut reg = store.bindings();
        reg.record_project_ref(pid, dir.to_string_lossy());
        store.save_bindings(&reg).unwrap();

        // Compat read: the reverse lookup lands on exactly one live project, so it resolves.
        let (found_dir, upgraded) = resolve_upward(&store, &dir).expect("an old pointer resolves too");
        assert_eq!(found_dir, dir);
        assert_eq!(upgraded.project_id, Some(pid));

        // Lazy rewrite: the pointer on disk is now in the current shape (integer id + slug), and the
        // old fields are gone.
        let raw = std::fs::read_to_string(dir.join(".amenbo")).unwrap();
        assert!(raw.contains(&format!("\"project_id\": {pid}")), "it is rewritten to an integer id: {raw}");
        assert!(!raw.contains("store_id"), "the old fields are not written back: {raw}");
        let reread = find_upward(&dir).unwrap().1;
        assert_eq!(reread.project_id, Some(pid));
        assert_eq!(reread.slug, store.project(pid).unwrap().unwrap().slug, "the slug comes from the store");
    }

    /// If the managed block in a folder whose pointer we just resolved is out of date, it follows to the
    /// current version right there — the same occasion, and the same trigger ("amenbo was run in that
    /// folder"), as the lazy rewrite of `.amenbo`.
    #[test]
    fn resolving_a_pointer_follows_that_folder_s_stale_managed_block() {
        let (store, pid) = store_with_project("follow", "案件X");
        let dir = tmp("follow-dir");
        DirBinding::new(Some(pid), None).write(&dir).unwrap();
        // An old block, carrying no version, with a Japanese language label.
        std::fs::write(
            dir.join("CLAUDE.md"),
            format!(
                "# Class P\n\n<!-- amenbo:begin (managed) -->\n{}\n<!-- amenbo:end -->\n",
                crate::agents::managed_block_body("Japanese", "amenbo")
            ),
        )
        .unwrap();

        // Running amenbo in this folder means resolving its pointer.
        resolve_upward(&store, &dir).expect("the pointer resolves");

        let claude = std::fs::read_to_string(dir.join("CLAUDE.md")).unwrap();
        assert_eq!(
            crate::agents::managed_block_version(&claude),
            Some(crate::agents::MANAGED_BLOCK_VERSION),
            "it follows to the current version right where it is resolved",
        );
        assert!(claude.contains("in Japanese."), "the language label is preserved");
        assert!(claude.contains("# Class P"), "what is outside the markers is untouched");
    }

    /// A pointer is upgraded only when its binding lands on **exactly one live project**. Otherwise it
    /// is left in the old shape — we never silently pick a project — and `doctor`'s [`legacy_pointers`]
    /// surfaces it as something the human must decide.
    #[test]
    fn an_unresolvable_legacy_pointer_is_left_alone_and_surfaced_by_doctor() {
        let (store, pid) = store_with_project("ambiguous", "案件X");
        let other = {
            let mut store = Store::open_at(store.paths.clone()).unwrap();
            store
                .project_add(crate::ops::project::NewProject {
                    name: "案件Y".into(),
                    view: crate::model::View::Board,
                    notes: String::new(),
                    color: None,
                })
                .unwrap()
                .id
        };
        let dir = tmp("ambiguous-dir");
        let legacy = r#"{"v":1,"project_id":"01LEGACY"}"#;
        std::fs::write(dir.join(".amenbo"), legacy).unwrap();
        // Two live projects claim the same folder, so there is no single answer to upgrade to.
        let mut reg = store.bindings();
        reg.record_project_ref(pid, dir.to_string_lossy());
        reg.record_project_ref(other, dir.to_string_lossy());
        store.save_bindings(&reg).unwrap();

        let (_, binding) = resolve_upward(&store, &dir).expect("the pointer itself is found");
        assert_eq!(binding.project_id, None, "ambiguous means it stays undecided (we never silently pick one)");
        assert_eq!(std::fs::read_to_string(dir.join(".amenbo")).unwrap(), legacy, "the disk is not touched either");

        // doctor lists it as an old pointer, but it cannot be upgraded automatically — a human decides
        // with `bind`.
        let found = legacy_pointers(&store);
        assert_eq!(found.len(), 1, "the one folder with an old pointer is listed");
        assert_eq!(found[0].recoverable, None);

        // Once the claim is unique again (forget one of them), the same folder moves to the "goes there
        // and heals" side.
        let mut reg = store.bindings();
        reg.forget_dir(&dir.to_string_lossy());
        reg.record_project_ref(pid, dir.to_string_lossy());
        store.save_bindings(&reg).unwrap();
        assert_eq!(legacy_pointers(&store)[0].recoverable, Some(pid));
    }

    /// Bulk repair is the "fix it without going to that folder" move. Old-shaped and vanished pointers
    /// alike are written back in the current shape wherever the owner is unambiguous, and folders where
    /// it is not are left untouched — we never silently pick a project.
    #[test]
    fn repair_rewrites_the_pointers_it_can_and_leaves_the_ambiguous_ones_alone() {
        let (store, pid) = store_with_project("repair", "案件X");
        let other = {
            let mut store = Store::open_at(store.paths.clone()).unwrap();
            store
                .project_add(crate::ops::project::NewProject {
                    name: "案件Y".into(),
                    view: crate::model::View::Board,
                    notes: String::new(),
                    color: None,
                })
                .unwrap()
                .id
        };

        let legacy_dir = tmp("repair-legacy"); // Old shape: an id that does not read as an integer.
        std::fs::write(legacy_dir.join(".amenbo"), r#"{"v":1,"project_id":"01LEGACY"}"#).unwrap();
        let gone_dir = tmp("repair-gone"); // A folder that lost its `.amenbo` entirely.
        let ambiguous_dir = tmp("repair-ambiguous"); // Two live projects claim it: no single owner.
        let ambiguous_raw = r#"{"v":1,"project_id":"01LEGACY"}"#;
        std::fs::write(ambiguous_dir.join(".amenbo"), ambiguous_raw).unwrap();

        let mut reg = store.bindings();
        reg.record_project_ref(pid, legacy_dir.to_string_lossy());
        reg.record_project_ref(pid, gone_dir.to_string_lossy());
        reg.record_project_ref(pid, ambiguous_dir.to_string_lossy());
        reg.record_project_ref(other, ambiguous_dir.to_string_lossy());
        store.save_bindings(&reg).unwrap();

        let repair = repair_pointers(&store);

        assert_eq!(repair.unresolved, vec![ambiguous_dir.to_string_lossy().to_string()]);
        assert_eq!(repair.repaired.len(), 2, "both the old-shaped and the vanished pointer are fixed: {repair:?}");
        for dir in [&legacy_dir, &gone_dir] {
            assert_eq!(
                read_pointer(dir).unwrap().project_id,
                Some(pid),
                "written back in the current shape without going to that folder"
            );
        }
        assert_eq!(
            std::fs::read_to_string(ambiguous_dir.join(".amenbo")).unwrap(),
            ambiguous_raw,
            "the disk of an ambiguous folder is not touched"
        );
        // What was repaired drops out of detection: neither the banner row nor the CLI warning survives
        // the repair.
        assert!(legacy_pointers(&store).iter().all(|p| p.dir != legacy_dir.to_string_lossy()));
        assert!(missing_pointers(&store).is_empty());
    }

    /// A folder that still exists while its `.amenbo` is gone is surfaced by [`missing_pointers`] — the
    /// predicate both `doctor` and the GUI folder list go through. Folders that still have a pointer do
    /// not appear, and neither do folders that vanished (stale): "the pointer is missing" gets us
    /// nowhere there, and the folder's own disappearance is what belongs in the report.
    #[test]
    fn a_bound_folder_that_lost_its_pointer_is_surfaced_but_a_vanished_folder_is_not() {
        let (store, pid) = store_with_project("lost-pointer", "案件X");
        let dir = tmp("lost-pointer-dir");
        pointer_for(&store, pid).write(&dir).unwrap();
        let mut reg = store.bindings();
        reg.record_project_ref(pid, dir.to_string_lossy());
        store.save_bindings(&reg).unwrap();
        assert!(missing_pointers(&store).is_empty(), "a folder that still has its pointer does not appear");

        // Remove only the `.amenbo`: the registry still names this project, yet an AI in that folder
        // will not resolve to it.
        std::fs::remove_file(dir.join(".amenbo")).unwrap();
        let found = missing_pointers(&store);
        assert_eq!(found.len(), 1, "the folder that lost its pointer is listed");
        assert_eq!(found[0].dir, dir.to_string_lossy(), "the subject is that very folder");
        assert_eq!(found[0].claimed_by, vec![pid], "a single claimant — running init there recovers it");

        // Once the folder itself is gone (stale) it drops out — what to report is the missing folder,
        // not the missing pointer.
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(missing_pointers(&store).is_empty(), "a vanished folder is not a missing_pointer");
    }

    /// A folder row no live project claims is surfaced as debris and dropped from the index by
    /// `doctor --fix` (`forget_orphan_dirs`). Folders a live project claims stay, so the cleanup only
    /// ever looks at debris.
    #[test]
    fn a_folder_no_live_project_claims_is_surfaced_and_forgotten() {
        let (store, pid) = store_with_project("orphan-binding", "案件X");
        let live = tmp("orphan-live-dir");
        let orphan = tmp("orphan-stale-dir");
        let mut reg = store.bindings();
        reg.record_project_ref(pid, live.to_string_lossy());
        // A row left behind by a deleted project: no project row with that id exists in the store.
        reg.record_project_ref(pid + 1_000, orphan.to_string_lossy());
        store.save_bindings(&reg).unwrap();

        assert_eq!(
            orphan_dirs(&store),
            vec![orphan.to_string_lossy().to_string()],
            "only a folder nobody claims is debris"
        );

        assert_eq!(store.forget_orphan_dirs().unwrap(), 1, "the one piece of debris is forgotten");
        assert!(orphan_dirs(&store).is_empty(), "after the cleanup no debris is left");
        assert_eq!(
            store.bindings().dirs_for_project(pid),
            vec![live.to_string_lossy().to_string()],
            "a live project's folder stays in the index"
        );
        assert!(orphan.is_dir(), "only the index row was dropped (the folder itself is untouched)");
        assert_eq!(store.forget_orphan_dirs().unwrap(), 0, "idempotent — a second run has nothing to clean up");
    }

    /// A current-shape pointer passes straight through — no reverse lookup, no rewrite — and so never
    /// shows up in `doctor`.
    #[test]
    fn a_current_pointer_is_passed_through_untouched() {
        let (store, pid) = store_with_project("current", "案件X");
        let dir = tmp("current-dir");
        pointer_for(&store, pid).write(&dir).unwrap();
        let before = std::fs::read_to_string(dir.join(".amenbo")).unwrap();

        let (_, binding) = resolve_upward(&store, &dir).expect("it resolves");
        assert_eq!(binding.project_id, Some(pid));
        assert_eq!(std::fs::read_to_string(dir.join(".amenbo")).unwrap(), before, "it is not rewritten");

        let mut reg = store.bindings();
        reg.record_project_ref(pid, dir.to_string_lossy());
        store.save_bindings(&reg).unwrap();
        assert!(legacy_pointers(&store).is_empty(), "a current-shape pointer does not show up in doctor");
    }

    /// A deleted project is not an owner, so we never write back a pointer naming a project that is
    /// no longer there.
    #[test]
    fn a_deleted_project_does_not_claim_the_folder() {
        let (mut store, pid) = store_with_project("dead", "案件X");
        let dir = tmp("dead-dir");
        std::fs::write(dir.join(".amenbo"), r#"{"v":1,"project_id":"01LEGACY"}"#).unwrap();
        let mut reg = store.bindings();
        reg.record_project_ref(pid, dir.to_string_lossy());
        store.save_bindings(&reg).unwrap();
        store.project_delete(pid, crate::model::ActorKind::Human).unwrap();

        assert!(live_projects_claiming(&store, &dir).is_empty(), "a deleted project does not claim it");
        assert_eq!(resolve_upward(&store, &dir).unwrap().1.project_id, None, "it is not upgraded");
    }

    #[test]
    fn registry_records_and_enumerates_project_refs() {
        let mut reg = Registry::default();
        // Many-to-one: two folders point at the same project.
        reg.record_project_ref(1, "/work/a");
        reg.record_project_ref(1, "/work/b");
        reg.record_project_ref(1, "/work/a"); // idempotent
        reg.record_project_ref(2, "/work/c");
        assert_eq!(reg.dirs_for_project(1), vec!["/work/a", "/work/b"]);
        assert_eq!(reg.dirs_for_project(2), vec!["/work/c"]);
        assert!(reg.dirs_for_project(99).is_empty());

        // forget_dir removes the folder from the reverse lookup too, without dragging other folders
        // down with it.
        reg.forget_dir("/work/a");
        assert_eq!(reg.dirs_for_project(1), vec!["/work/b"]);
    }

    #[test]
    fn dirs_for_project_unions_paths_and_project_dirs() {
        // Even when a write path drifts and fills only `paths` (the main directory), forgetting to call
        // `record_project_ref`, the reverse lookup returns the union of both indexes and so does not
        // miss a folder whose `.amenbo` really exists.
        let mut reg = Registry::default();
        reg.set(1, "/work/main"); // paths only — as if record_project_ref had been forgotten
        assert_eq!(reg.dirs_for_project(1), vec!["/work/main"]);

        // Another folder in `project_dirs` joins the union (deduped, ascending); a duplicate of the
        // main directory collapses.
        reg.record_project_ref(1, "/work/extra");
        reg.record_project_ref(1, "/work/main");
        assert_eq!(reg.dirs_for_project(1), vec!["/work/extra", "/work/main"]);

        // Unbinding (forget_dir) drops the folder from `paths` too, so releasing the main directory
        // removes it from the union for good.
        reg.forget_dir("/work/main");
        assert_eq!(reg.dirs_for_project(1), vec!["/work/extra"]);
    }

    #[test]
    fn additional_bind_does_not_drop_a_paths_only_first_folder() {
        // Binding an extra folder while the old one lives only in `paths` (never recorded in
        // `project_dirs`) still enumerates both, because `set` stashes the old main directory into
        // `project_dirs` before overwriting it.
        let mut reg = Registry::default();
        // The first folder is in paths only.
        reg.paths.insert(1, "/work/A".into());
        assert_eq!(reg.dirs_for_project(1), vec!["/work/A"]);

        // An extra bind, along the real project_bind_folder path: set + record_project_ref.
        reg.set(1, "/work/B");
        reg.record_project_ref(1, "/work/B");
        // Both folders are there on the first try — the old /work/A does not vanish.
        assert_eq!(reg.dirs_for_project(1), vec!["/work/A", "/work/B"]);

        // Further binds accumulate, and the main directory is still repointed (that is what resolve_dir
        // reads).
        reg.set(1, "/work/C");
        reg.record_project_ref(1, "/work/C");
        assert_eq!(reg.dirs_for_project(1), vec!["/work/A", "/work/B", "/work/C"]);
        assert_eq!(reg.get(1), Some("/work/C"));
    }

    #[test]
    fn all_dirs_unions_both_indexes_deduped() {
        let mut reg = Registry::default();
        reg.set(1, "/work/main"); // paths
        reg.record_project_ref(1, "/work/extra"); // project_dirs
        reg.record_project_ref(2, "/work/main"); // another project points at the same folder
        // The union of both indexes: deduped, ascending.
        assert_eq!(
            reg.all_dirs(),
            vec!["/work/extra".to_string(), "/work/main".to_string()]
        );
        // Unbinding drops the folder from every index, so it leaves all_dirs too.
        reg.forget_dir("/work/main");
        assert_eq!(reg.all_dirs(), vec!["/work/extra".to_string()]);
    }

    #[test]
    fn registry_forget_dir_removes_only_that_folder() {
        let mut reg = Registry::default();
        // Many-to-one: two projects point at one folder, and another folder holds a main-directory
        // record.
        reg.record_project_ref(1, "/work/a");
        reg.record_project_ref(2, "/work/a");
        reg.set(1, "/work/a");
        reg.set(2, "/work/b");

        // Forget /work/a: two project_dirs entries plus one paths entry — three records removed.
        assert_eq!(reg.forget_dir("/work/a"), 3);
        // Records for the other folder (/work/b) survive: many-to-one does not drag them along.
        assert_eq!(reg.get(2), Some("/work/b"));
        // Nothing pointing at /work/a is left.
        assert!(reg.projects_for_dir("/work/a").is_empty());
        assert_eq!(reg.get(1), None);

        // Idempotent: forgetting it again removes nothing.
        assert_eq!(reg.forget_dir("/work/a"), 0);
    }

    #[test]
    fn projects_for_dir_reverse_lookup_distinguishes_zero_one_many() {
        // The reverse lookup that recovery of a vanished pointer stands on.
        let mut reg = Registry::default();
        // Two projects point at /work/a (many-to-one, so it is ambiguous); only one points at /work/b
        // (unique, so it can be recovered).
        reg.set(1, "/work/a"); // paths only
        reg.record_project_ref(2, "/work/a"); // project_dirs — another project on the same folder
        reg.record_project_ref(3, "/work/b");

        // One: the caller can tell the recovery candidate is unique.
        assert_eq!(reg.projects_for_dir("/work/b"), vec![3]);
        // Several: the union of both indexes, ascending and deduped — ambiguous, so the caller reports
        // an error.
        assert_eq!(reg.projects_for_dir("/work/a"), vec![1, 2]);
        // None: a folder no project points at.
        assert!(reg.projects_for_dir("/work/never").is_empty());
    }

    #[test]
    fn projects_for_dir_matches_despite_trailing_slash() {
        // Values are levelled by component-wise PathBuf comparison, which absorbs a trailing slash; a
        // path that does not exist fails to canonicalize and is compared lexically.
        let mut reg = Registry::default();
        reg.record_project_ref(1, "/work/a/");
        assert_eq!(reg.projects_for_dir("/work/a"), vec![1]);
    }

    #[test]
    fn registry_resolve_dir_flags_stale_paths() {
        let home = tmp("reg");
        let work = home.join("work");
        std::fs::create_dir_all(&work).unwrap();
        let mut reg = Registry::default();
        reg.set(1, work.to_string_lossy().to_string());
        // A live path resolves.
        assert_eq!(reg.resolve_dir(1).unwrap(), Some(work.clone()));
        // A vanished path is binding_stale.
        std::fs::remove_dir_all(&work).unwrap();
        let err = reg.resolve_dir(1).unwrap_err();
        assert_eq!(err.code(), "binding_stale");
        // No record at all is None, not an error.
        assert_eq!(reg.resolve_dir(99).unwrap(), None);
    }
}
