//! How a record's ref is written where output names it — the `AMB-` namespace, in one place, so
//! every command spells a task, a decision, a comment, an axis and a project the same way.

/// What a decision is called (`AMB-D-<n>`). The id is the conversational number, so reading the row would
/// tell us nothing the ref does not already carry — like [`task_label`], it is built straight from the id.
pub(crate) fn decision_label(id: i64) -> String {
    amenbo_core::idref::decision(id)
}

/// What a task is called (`AMB-T-<n>`). The id is the conversational number, so the truth source is never
/// queried.
pub(crate) fn task_label(id: i64) -> String {
    amenbo_core::idref::task(id)
}

/// What a task comment is called (`AMB-TC-<n>`), and what a decision comment is called (`AMB-DC-<n>`). A
/// comment carries no conversational number of its own, so this ref is the only handle `comment rm` /
/// `comment attach` can be given — and the two tables number independently, so which spelling a caller
/// reaches for is decided by the table it just wrote, never by the id (`AMB-D-377`).
pub(crate) fn task_comment_label(id: i64) -> String {
    amenbo_core::idref::task_comment(id)
}

pub(crate) fn decision_comment_label(id: i64) -> String {
    amenbo_core::idref::decision_comment(id)
}

/// What a dimension is called (`AMB-DIM-<n>`), and one of its values (`AMB-DIMV-<n>`). Both resolve by name
/// too, so the ref is the tie-breaker rather than the everyday handle.
pub(crate) fn dimension_label(id: i64) -> String {
    amenbo_core::idref::render(amenbo_core::idref::RefKind::Dimension, id)
}

pub(crate) fn dimension_value_label(id: i64) -> String {
    amenbo_core::idref::render(amenbo_core::idref::RefKind::DimensionValue, id)
}

/// What a project is called (`AMB-P-<n>`), taken from the id as the probe reports it (a decimal string). An
/// unreadable id is shown as it came, rather than dressed up as a ref it is not.
pub(crate) fn project_label(id: &str) -> String {
    match id.parse::<i64>() {
        Ok(n) => amenbo_core::idref::project(n),
        Err(_) => id.to_string(),
    }
}
