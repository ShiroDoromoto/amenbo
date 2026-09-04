//! The one place a person can write that is not a record — a draft page per project (`AMB-T-3608`).
//!
//! Everything else Amenbo keeps is a record: a task, a decision, a comment on either. This is not.
//! It is where a long request is put together before it is sent, and the request is the point — the
//! page is the workbench it was assembled on.
//!
//! **It stays and it does not grow.** There is one per project and it is plain text: no history, no
//! second page, nothing drawn from it. What is worth keeping moves to a task or a decision, and what
//! is left is a draft that has served its purpose. A page that grew — versions, sections, a list of
//! them — would become a second place to look for what a project is about, and the project already
//! has one.
//!
//! **One per project, and one project at a time**: the memo belongs to the project rather than to
//! the machine, so it is in the store rather than the device row beside the frame names
//! ([`crate::frames`]). Two machines on the same store are writing on the same page, which is what a
//! project's own draft should be — and a draft from another project is never mixed into it.

use crate::error::Result;
use crate::store_engine::StoreEngine;

/// The `store_meta` key one project's page lives under. Keyed by id rather than by name, so a
/// project that is renamed keeps what was written on it.
fn key(project_id: i64) -> String {
    format!("memo.project.{project_id}")
}

/// What is written on this project's page — empty where nothing is.
pub fn memo(engine: &StoreEngine, project_id: i64) -> Result<String> {
    Ok(engine.get_meta(&key(project_id))?.unwrap_or_default())
}

/// Write the page. Blank erases it rather than keeping an empty scalar: a page nobody wrote on is a
/// page that is not there, and the two must not read differently.
pub fn set_memo(engine: &StoreEngine, project_id: i64, text: &str) -> Result<()> {
    let key = key(project_id);
    let written = if text.trim().is_empty() { None } else { Some(text) };
    engine.set_meta(&key, written)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The page keeps what was written on it, per project, and a blank one is no page at all.
    #[test]
    fn a_page_is_a_projects_own_and_a_blank_one_is_none() {
        let engine = StoreEngine::open_in_memory().unwrap();
        assert_eq!(memo(&engine, 1).unwrap(), "", "nothing has been written yet");

        set_memo(&engine, 1, "  頼みたいことの下書き  ").unwrap();
        set_memo(&engine, 2, "別のプロジェクトの下書き").unwrap();
        // What was written is kept as it was typed: trimming is what decides whether there is a page,
        // not what the page says. A draft's own leading blank line is the writer's.
        assert_eq!(memo(&engine, 1).unwrap(), "  頼みたいことの下書き  ");
        assert_eq!(memo(&engine, 2).unwrap(), "別のプロジェクトの下書き");

        set_memo(&engine, 1, "   ").unwrap();
        assert_eq!(memo(&engine, 1).unwrap(), "", "a page with nothing on it is not a page");
        assert_eq!(memo(&engine, 2).unwrap(), "別のプロジェクトの下書き", "and its neighbour is untouched");
    }
}
