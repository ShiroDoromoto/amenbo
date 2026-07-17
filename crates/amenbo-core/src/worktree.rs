//! Is this folder a **git worktree cut inside an amenbo-managed tree**? A `.amenbo` is found by walking
//! upward ([`crate::binding::find_upward`]), so a worktree cut *inside* a managed project folder inherits
//! that project's binding — and while the worktree is throwaway, the store it would write to is not: that
//! one lives in app-data, outside the checkout, and survives its deletion, so a throwaway environment can
//! drive the real backlog. The asymmetry is made by amenbo's own binding mechanism, so amenbo is what closes
//! it, and [`nested`] is the predicate the CLI refuses on. It keys on where the worktree root sits rather
//! than on where the pointer sits, so a `.amenbo` written *inside* the checkout is never read and buys no
//! passage: `--force` means "overwrite the pointer already there", which says nothing about this hazard.
//! The CLI holds `bind` / `init` to this predicate as well, so nothing writes such a pointer today — but an
//! older build did, and reading one would hand exactly those writes their passage back.
//! Untouched are the shapes with no managed tree above their root: a
//! worktree parked beside the project, and a project folder that is itself a worktree
//! (`~/repos/proj-main`, `~/repos/proj-feature`), which is where someone who wants a bound worktree puts
//! one. Recovering the binding with `bind` is the accident this guards against, so nothing here points at
//! it.

use std::path::{Path, PathBuf};

/// What kind of git checkout a `.git` marker names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Checkout {
    /// `.git` is a **directory**: an ordinary repository (a worktree set's main checkout included). Also
    /// where a `gitdir:` file we cannot make sense of lands — an unrecognized pointer is not grounds for
    /// refusing.
    Plain,
    /// `.git` is a `gitdir:` file pointing into `…/.git/worktrees/<name>` — a linked worktree.
    Worktree,
    /// `.git` is a `gitdir:` file pointing into `…/.git/modules/<name>` — a submodule. It shares the "`.git`
    /// is a file" shape with a worktree, so it must be told apart here or every submodule would trip this
    /// guard. A submodule is a normal part of the tree it sits in, not a throwaway copy of it.
    Submodule,
}

/// Read a `gitdir:` marker file and say what it names. Git writes an absolute path for a worktree
/// (`…/.git/worktrees/<name>`) and often a relative one for a submodule (`../.git/modules/<name>`), so we
/// key on the component **after** a `.git` component rather than on the string's shape. Anything we do not
/// recognize (an unreadable file, a form git does not write today) is [`Checkout::Plain`]: the guard refuses
/// only what it positively identifies.
fn classify_gitdir(marker: &Path) -> Checkout {
    let Ok(raw) = std::fs::read_to_string(marker) else {
        return Checkout::Plain;
    };
    let Some(gitdir) = raw.trim().strip_prefix("gitdir:") else {
        return Checkout::Plain;
    };
    let mut after_git = false;
    for c in Path::new(gitdir.trim()).components() {
        if after_git {
            match c.as_os_str().to_str() {
                Some("worktrees") => return Checkout::Worktree,
                Some("modules") => return Checkout::Submodule,
                _ => {}
            }
        }
        after_git = c.as_os_str() == ".git";
    }
    Checkout::Plain
}

/// Walk upward from `start` for the nearest `.git`, and return `(the directory holding it, its kind)`. The
/// walk is what lets the guard fire from a subdirectory of a worktree, not only from its root.
fn checkout(start: &Path) -> Option<(PathBuf, Checkout)> {
    let mut cur = Some(start);
    while let Some(dir) = cur {
        let git = dir.join(".git");
        if git.is_dir() {
            return Some((dir.to_path_buf(), Checkout::Plain));
        }
        if git.is_file() {
            return Some((dir.to_path_buf(), classify_gitdir(&git)));
        }
        cur = dir.parent();
    }
    None
}

/// A worktree cut inside a managed tree, and the tree that manages it — the material for the refusal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Nested {
    /// The root of the worktree the CWD sits in.
    pub worktree_root: PathBuf,
    /// The folder whose `.amenbo` the worktree was cut inside — where amenbo is meant to be run instead.
    pub bound_dir: PathBuf,
}

/// Is the CWD inside a git worktree that was cut **within** an amenbo-managed tree? `Some` is the refusal;
/// `None` means there is nothing to refuse. Both conditions must hold: the CWD is inside a linked worktree
/// ([`Checkout::Worktree`] — a submodule is not one), and a `.amenbo` sits **strictly above the worktree
/// root**. The search starts above that root, so a pointer within the worktree — including one
/// `bind --force` wrote there — is never consulted, and the ordinary shapes never fire: a bound folder, any
/// subdirectory of it, a worktree parked outside the project, or a project folder that is itself a worktree.
pub fn nested(cwd: &Path) -> Option<Nested> {
    let (worktree_root, Checkout::Worktree) = checkout(cwd)? else {
        return None;
    };
    let (bound_dir, _) = crate::binding::find_upward_ancestor(&worktree_root)?;
    Some(Nested { worktree_root, bound_dir })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binding::DirBinding;

    fn tmp(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("amenbo-wt-{}-{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        // `std::env::temp_dir()` is a symlink on macOS (/tmp → /private/tmp). The predicate compares two
        // paths reached from one CWD, so it needs no canonicalization — but the test builds its paths by
        // hand, so it levels them here.
        std::fs::canonicalize(&p).unwrap()
    }

    /// Create `<dir>/.git` as a `gitdir:` file naming `target`.
    fn git_file(dir: &Path, target: &str) {
        std::fs::write(dir.join(".git"), format!("gitdir: {target}\n")).unwrap();
    }

    fn subdir(parent: &Path, name: &str) -> PathBuf {
        let p = parent.join(name);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    /// The accident this guard closes: a worktree cut inside the bound project folder inherits its `.amenbo`, and
    /// so can drive the real backlog from a throwaway checkout. It fires from the worktree root and from any
    /// subdirectory of it.
    #[test]
    fn a_worktree_nested_in_the_bound_folder_inherits_the_binding() {
        let project = tmp("nested-project");
        DirBinding::new(Some(1), None).write(&project).unwrap();
        std::fs::create_dir_all(project.join(".git")).unwrap();

        let wt = subdir(&project, "wt");
        git_file(&wt, &format!("{}/.git/worktrees/wt", project.display()));

        let found = nested(&wt).expect("the nested worktree inherited the binding");
        assert_eq!(found, Nested { worktree_root: wt.clone(), bound_dir: project.clone() });

        // From a subdirectory of the worktree the walk still lands on the same pair.
        let deep = subdir(&wt, "crates/amenbo-cli");
        assert_eq!(nested(&deep).expect("fires from inside the worktree too").bound_dir, project);
    }

    /// Writing the worktree a `.amenbo` of its own — what `bind --force` leaves behind — buys no passage:
    /// the predicate looks strictly above the worktree root, so a pointer inside the checkout is never read,
    /// and the refusal keeps naming the managed folder above rather than the worktree's own binding.
    #[test]
    fn a_force_bound_nested_worktree_still_fires() {
        let project = tmp("force-bound");
        DirBinding::new(Some(1), None).write(&project).unwrap();
        std::fs::create_dir_all(project.join(".git")).unwrap();

        let wt = subdir(&project, "wt");
        git_file(&wt, &format!("{}/.git/worktrees/wt", project.display()));
        DirBinding::new(Some(2), None).write(&wt).unwrap();

        let found = nested(&wt).expect("a self-written pointer is not consent to the hazard");
        assert_eq!(found, Nested { worktree_root: wt.clone(), bound_dir: project.clone() });
        assert_eq!(nested(&subdir(&wt, "crates")).expect("and from a subdirectory too").bound_dir, project);
    }

    /// The everyday shapes, none of which fire: the bound folder itself, a subdirectory of it, and a
    /// worktree parked outside the project (which has no `.amenbo` above it at all).
    #[test]
    fn an_ordinary_tree_and_an_outside_worktree_do_not_fire() {
        let root = tmp("ordinary");
        let project = subdir(&root, "project");
        DirBinding::new(Some(1), None).write(&project).unwrap();
        std::fs::create_dir_all(project.join(".git")).unwrap();

        assert_eq!(nested(&project), None, "the bound folder itself is not a worktree");
        assert_eq!(
            nested(&subdir(&project, "crates")),
            None,
            "a subdirectory of an ordinary repository is ordinary work",
        );

        // A worktree cut beside the project: it inherits nothing, because `.amenbo` is gitignored and so
        // never lands in the checkout.
        let outside = subdir(&root, "worktrees/1674");
        git_file(&outside, &format!("{}/.git/worktrees/1674", project.display()));
        assert_eq!(nested(&outside), None, "an outside worktree has no binding to inherit");
    }

    /// The legitimate shape the guard must leave alone, and the one place a bound worktree belongs: the
    /// project folder is **itself** a worktree, checked out beside the main one (`~/repos/proj-main`,
    /// `~/repos/proj-feature`) and bound in its own right. No managed tree sits above its root, so it is
    /// nobody's throwaway — neither it nor its subdirectories fire, or every subdirectory of such a folder
    /// would be refused.
    #[test]
    fn a_worktree_that_is_itself_the_project_folder_owns_its_binding() {
        let repos = tmp("own-binding");
        let main = subdir(&repos, "proj-main");
        std::fs::create_dir_all(main.join(".git")).unwrap();

        let wt = subdir(&repos, "proj-feature");
        git_file(&wt, &format!("{}/.git/worktrees/proj-feature", main.display()));
        DirBinding::new(Some(2), None).write(&wt).unwrap();

        assert_eq!(nested(&wt), None, "no managed tree sits above this worktree's root");
        assert_eq!(
            nested(&subdir(&wt, "crates")),
            None,
            "and a subdirectory of it is ordinary work in a bound folder",
        );
    }

    /// A submodule shares the "`.git` is a file" shape with a worktree, so it would trip a shape-only check.
    /// It is a normal part of the tree it sits in, not a throwaway copy of it, and must not fire.
    #[test]
    fn a_submodule_nested_in_the_bound_folder_does_not_fire() {
        let project = tmp("submodule");
        DirBinding::new(Some(1), None).write(&project).unwrap();
        std::fs::create_dir_all(project.join(".git")).unwrap();

        let sub = subdir(&project, "vendor/lib");
        // Git writes a relative gitdir for a submodule — the classifier keys on the component after `.git`,
        // not on the path's shape.
        git_file(&sub, "../../.git/modules/vendor/lib");
        assert_eq!(nested(&sub), None, "a submodule is not a throwaway worktree");
        assert_eq!(nested(&subdir(&sub, "src")), None);
    }

    /// The classifier reads what git writes, and refuses to guess at anything else — an unrecognized or
    /// unreadable marker is `Plain`, so the guard stays silent rather than firing on a form it does not know.
    #[test]
    fn a_gitdir_marker_is_classified_by_the_component_after_dot_git() {
        let dir = tmp("classify");
        let marker = dir.join(".git");

        for (raw, want) in [
            ("gitdir: /repo/.git/worktrees/feature", Checkout::Worktree),
            ("gitdir: ../.git/modules/vendor", Checkout::Submodule),
            // No leading `gitdir:`, an empty file, or a path with neither component: not something to refuse.
            ("/repo/.git/worktrees/feature", Checkout::Plain),
            ("", Checkout::Plain),
            ("gitdir: /repo/.git", Checkout::Plain),
            // `worktrees` that is not under a `.git` is just a folder someone named that.
            ("gitdir: /repo/worktrees/feature", Checkout::Plain),
        ] {
            std::fs::write(&marker, raw).unwrap();
            assert_eq!(classify_gitdir(&marker), want, "gitdir marker {raw:?}");
        }

        // A marker that is not there at all reads as Plain rather than panicking.
        std::fs::remove_file(&marker).unwrap();
        assert_eq!(classify_gitdir(&marker), Checkout::Plain);
    }

    /// Outside a git tree entirely there is no checkout to walk up to, so nothing fires even when a `.amenbo`
    /// sits above.
    #[test]
    fn a_folder_outside_any_git_tree_does_not_fire() {
        let root = tmp("no-git");
        DirBinding::new(Some(1), None).write(&root).unwrap();
        assert_eq!(nested(&subdir(&root, "notes")), None);
    }
}
