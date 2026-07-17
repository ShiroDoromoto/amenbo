//! Deriving a project's human-readable identifier (its slug).
//!
//! The identifier of record is the primary key (`project.id`, INTEGER); the slug is what we
//! **cross-check** against it. A `.amenbo` pointer carries both, so when the slug disagrees with the
//! slug of the project the id names, we can warn that the pointer came from another store
//! ([`crate::binding`]). That is why a slug is unique on the machine (the `project_by_slug` unique
//! index) and does not follow a rename.
//!
//! The derivation lives in this one place, so that creation ([`crate::ops::project::add`]) and
//! migration ([`crate::store_engine`]) pick slugs by the same rule and the escape from a collision
//! cannot drift into two different shapes.

use std::collections::HashSet;

/// Build the base slug candidate from a name: runs of ASCII alphanumerics, lowercased and joined with
/// `-`, truncated to 24 characters, with any `-` left at either end trimmed off. A name with no ASCII
/// alphanumerics at all (all full-width, say) falls back to `"project"`.
pub fn base(name: &str) -> String {
    let mut s = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            s.push(c.to_ascii_lowercase());
        } else if !s.is_empty() && !s.ends_with('-') {
            s.push('-');
        }
    }
    s.truncate(24);
    let s = s.trim_matches('-');
    if s.is_empty() {
        "project".to_string()
    } else {
        s.to_string()
    }
}

/// Settle on a slug for `name` that does not collide with the already-taken set. On a collision, append
/// a numeric suffix: `amenbo` → `amenbo-2` → `amenbo-3`, and so on.
pub fn unique(taken: &HashSet<String>, name: &str) -> String {
    let base = base(name);
    if !taken.contains(&base) {
        return base;
    }
    for n in 2u32.. {
        let cand = format!("{base}-{n}");
        if !taken.contains(&cand) {
            return cand;
        }
    }
    unreachable!("the suffix range is unbounded")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_squeezes_a_name_into_one_readable_token() {
        assert_eq!(base("amenbo dev"), "amenbo-dev");
        assert_eq!(base("  Web/Site (2026)!! "), "web-site-2026");
        assert_eq!(base("日本語だけ"), "project");
        // The only ASCII here is "amenbo"; the full-width characters drop out.
        assert_eq!(base("amenbo 開発"), "amenbo");
        // A long name is cut at 24 characters, and a `-` left at the cut is trimmed.
        assert_eq!(base("aaaaaaaaaaaaaaaaaaaaaaaa bbb"), "aaaaaaaaaaaaaaaaaaaaaaaa");
    }

    #[test]
    fn unique_escapes_a_collision_with_a_numeric_suffix() {
        let mut taken: HashSet<String> = HashSet::new();
        assert_eq!(unique(&taken, "amenbo 開発"), "amenbo");
        taken.insert("amenbo".to_string());
        assert_eq!(unique(&taken, "amenbo 本番"), "amenbo-2");
        taken.insert("amenbo-2".to_string());
        assert_eq!(unique(&taken, "amenbo その他"), "amenbo-3");
        // Even a name with no ASCII in it has an escape from a collision.
        taken.insert("project".to_string());
        assert_eq!(unique(&taken, "全角のみ"), "project-2");
    }
}
