//! Which project a command works in, and where this invocation stands — the binding, the
//! explicit override, and resolving the names a caller writes against them.

use amenbo_core::Store;
use amenbo_core::config::Paths;

use crate::PROJECT_OVERRIDE;
use crate::cli::*;
use crate::output::CliError;

/// The quiet first line of `status`/`whoami`: which project, and which folder, this run is operating in.
/// Searches upward for `.amenbo` and returns `Project: <name>  (this folder: <the folder holding
/// .amenbo>)` — anchored on the bound folder, exactly as `bind` displays it. With no binding (an
/// `AMENBO_HOME` sandbox store, say) it is None and no header is printed.
pub(crate) fn location_header(store: &Store) -> Option<String> {
    let cwd = std::env::current_dir().ok()?;
    let (dir, binding) = amenbo_core::binding::resolve_upward(store, &cwd)?;
    let name = binding
        .project_id
        .and_then(|pid| {
            store
                .project(pid)
                .ok()
                .flatten()
                
                .map(|p| p.name)
        })
        .unwrap_or_else(|| "(no project set)".to_string());
    let header = format!("Project: {}  (this folder: {})", name, dir.to_string_lossy());
    // Report a mismatch in the cross-check right here, so it is seen at the very start of a session.
    Some(match slug_mismatch_warning(store, &binding) {
        Some(warning) => format!("{header}\n{warning}"),
        None => header,
    })
}

/// Use the binding's (`.amenbo`) default project as the context for ref resolution — but only while that
/// project is live. A legacy pointer is read compatibly by `resolve_upward`, which rewrites it into the
/// current form on the spot.
pub(crate) fn bound_project(store: &Store) -> Option<i64> {
    // An explicit override (`--project`) wins. It was validated as live against this store when it was set,
    // so return it as is.
    if let Some(pid) = PROJECT_OVERRIDE.get() {
        return Some(*pid);
    }
    binding_project(store)
}

/// Does this invocation name a project with `--project` — either the global flag or a sub-command that
/// carries one? What an AI is forbidden is the naming itself, so which project it names is not looked at:
/// naming the bound project is refused just the same.
pub(crate) fn named_project_flag(cli: &Cli) -> Option<&'static str> {
    let named = cli.project.is_some()
        || match &cli.command {
            Some(Command::Bind { project, .. })
            | Some(Command::Activity { project, .. })
            | Some(Command::Search { project, .. }) => project.is_some(),
            Some(Command::Task { sub }) => matches!(
                sub,
                TaskCmd::Add { project: Some(_), .. }
                    | TaskCmd::List { project: Some(_), .. }
                    | TaskCmd::Move { project: Some(_), .. }
            ),
            Some(Command::Decision { sub }) => matches!(
                sub,
                DecisionCmd::Add { project: Some(_), .. }
                    | DecisionCmd::List { project: Some(_), .. }
                    | DecisionCmd::Promote { project: Some(_), .. }
            ),
            Some(Command::Dimension { sub }) => matches!(
                sub,
                DimensionCmd::Add { project: Some(_), .. } | DimensionCmd::List { project: Some(_), .. }
            ),
            _ => false,
        };
    named.then_some("--project")
}

/// Resolve an explicit `--project`, and fill the slot from the binding when none is given. With neither,
/// fail loud rather than guess — nothing gets created without a project. An AI cannot pass `--project`
/// ([`named_project_flag`]), so for an AI this is the only route: the binding decides where things land.
pub(crate) fn project_or_bound(store: &Store, project: Option<String>) -> Result<i64, CliError> {
    match project {
        Some(p) => store.resolve_project_ref(&p).map_err(CliError::from),
        None => bound_project(store).ok_or_else(|| project_required(store)),
    }
}

/// The error for "there is nowhere to put this". Lists the existing projects, so the answer is to pick one.
pub(crate) fn project_required(store: &Store) -> CliError {
    let projects = store.project_list(false).map(|r| r.projects).unwrap_or_default();
    if projects.is_empty() {
        return CliError::from(amenbo_core::Error::invalid(
            "--project is required, but no projects exist yet — create one first",
        ));
    }
    let en = projects.iter().map(|p| format!("{} ({})", p.name, p.id)).collect::<Vec<_>>().join(", ");
    CliError::from(amenbo_core::Error::invalid(
        format!("--project is required. existing projects: {en}. pass --project <id|name>"),
    ))
}

/// Resolve `--at <folder>` to one of `project_id`'s bound folders (`AMB-D-648`), as the binding id a task
/// carries. The candidates are that project's folders and no others: a task's folder is one of its own
/// project's, which is what keeps a place from pointing outside the project the task lives in — and, for an
/// AI, outside its reach.
///
/// Three spellings are accepted, in order, because all three are what a person has in hand: the path as the
/// registry recorded it, the path as this machine canonicalises what was typed (so `.` and a relative path
/// work), and the folder's own name (`--at amenbo-plugin-mail`, which is how the folders of one project
/// usually differ). A name that lands on several is refused with them listed rather than one being picked;
/// a name that lands on none is refused with the project's folders listed, since that is the answer.
pub(crate) fn resolve_bound_folder(store: &Store, project_id: i64, token: &str) -> Result<i64, CliError> {
    let folders = store.bound_folders_of(project_id).map_err(CliError::from)?;
    let listed = || folders.iter().map(|f| f.dir.as_str()).collect::<Vec<_>>().join(", ");
    if folders.is_empty() {
        return Err(CliError::from(amenbo_core::Error::invalid(format!(
            "--at names one of the project's linked folders, and this project has none. Link one first (`{} bind --project <name or ID>`)",
            Paths::command_name()
        ))));
    }
    let canonical = amenbo_core::binding::canonical_dir(token).map(|p| p.to_string_lossy().to_string());
    let mut hits: Vec<&amenbo_core::binding::BoundFolder> =
        folders.iter().filter(|f| f.dir == token).collect();
    if hits.is_empty() {
        if let Ok(ref canon) = canonical {
            hits = folders.iter().filter(|f| &f.dir == canon).collect();
        }
    }
    if hits.is_empty() {
        hits = folders
            .iter()
            .filter(|f| std::path::Path::new(&f.dir).file_name().is_some_and(|n| n == token))
            .collect();
    }
    match hits.as_slice() {
        [one] => Ok(one.id),
        [] => Err(CliError::from(amenbo_core::Error::invalid(format!(
            "--at `{token}` is not one of this project's linked folders: {}",
            listed()
        )))),
        several => Err(CliError::from(amenbo_core::Error::invalid(format!(
            "--at `{token}` names {} of this project's linked folders ({}) — say which by its path",
            several.len(),
            several.iter().map(|f| f.dir.as_str()).collect::<Vec<_>>().join(", ")
        )))),
    }
}

/// Resolve `--dim <axis>=<value>` pairs into the value ids to file a new task under, in the order given.
/// The axis is looked up **inside the task's own project** — axes are per-project, so a name two projects
/// share must not resolve to the neighbour's — and the value inside that axis, the same rules `dimension
/// set` uses.
///
/// Two refusals, both before anything is written:
/// - a pair that is not `<axis>=<value>` (split on the first `=`, so a value may contain one);
/// - the same axis named twice. An axis holds one value, so the second would silently replace the first,
///   and which one the caller meant is not ours to pick.
///
/// `=none` is not accepted here, unlike the `dim:` filter: there it selects the tasks with no value on
/// that axis, and clearing an axis that was never set is what a new task already is.
pub(crate) fn resolve_dim_pairs(store: &Store, project_id: i64, pairs: &[String]) -> Result<Vec<i64>, CliError> {
    let mut value_ids = Vec::with_capacity(pairs.len());
    let mut axes: Vec<i64> = Vec::new();
    for pair in pairs {
        let Some((axis, value)) = pair.split_once('=') else {
            return Err(CliError::from(amenbo_core::Error::invalid(
                format!("--dim takes <axis>=<value> (e.g. --dim \"Category=bug\"), got `{pair}`"),
            )));
        };
        let dimension_id = store.resolve_dimension(Some(project_id), axis).map_err(CliError::from)?;
        if axes.contains(&dimension_id) {
            return Err(CliError::from(amenbo_core::Error::invalid(
                format!("--dim names the axis `{axis}` twice — an axis holds one value, so pass it once"),
            )));
        }
        axes.push(dimension_id);
        value_ids.push(store.resolve_dimension_value(dimension_id, value).map_err(CliError::from)?);
    }
    Ok(value_ids)
}

/// The live project this CWD's `.amenbo` points at — the binding itself, with no override folded in. An AI
/// facet's reach is drawn from here: if `--project` could widen it, the binding would decay into decoration
/// that merely says which store to open.
pub(crate) fn binding_project(store: &Store) -> Option<i64> {
    let cwd = std::env::current_dir().ok()?;
    let (_, binding) = amenbo_core::binding::resolve_upward(store, &cwd)?;
    let pid = binding.project_id?;
    store
        .project(pid)
        .ok()
        .flatten()
        .is_some()
        .then_some(pid)
}

/// The warning for when the slug recorded in `.amenbo` disagrees with the project its `project_id` names.
/// Resolution is not stopped: the id is authoritative and the slug is only a cross-check. What the
/// disagreement means is that the pointer came from another store — the folder was copied, or imported from
/// another environment — and that the id may now quietly name something else entirely. This warning is the
/// only sign of it, so it goes on the surfaces every session passes through first: the location header of
/// `status`/`whoami`, and what `bind` displays.
pub(crate) fn slug_mismatch_warning(store: &Store, binding: &amenbo_core::binding::DirBinding) -> Option<String> {
    Some(slug_mismatch_sentence(&amenbo_core::binding::slug_mismatch(store, binding)?))
}

/// The sentence itself, made from the mismatch alone — so what it says can be read back without a store
/// to stand it up in. Every slot is **named**: the four it holds are two slugs, a ref and a command
/// name, and positional arguments let them be written in an order that reads perfectly well while
/// saying something else entirely.
fn slug_mismatch_sentence(m: &amenbo_core::binding::SlugMismatch) -> String {
    format!(
        "warning: this folder's .amenbo names project '{recorded}', but {project} is '{actual}' — the \
         pointer looks like it came from another store. Re-link it with `{cmd} bind --project <name or ID>`.",
        recorded = m.recorded,
        project = amenbo_core::idref::project(m.project_id),
        actual = m.actual.as_deref().unwrap_or("(no slug)"),
        cmd = Paths::command_name(),
    )
}

/// The project name shown alongside a binding. `None` when the id names no record — a `.amenbo` can go on
/// pointing at a project that is gone.
pub(crate) fn project_name(store: &Store, project_id: Option<i64>) -> Result<Option<String>, CliError> {
    let Some(pid) = project_id else { return Ok(None) };
    Ok(store.project(pid).map_err(CliError::from)?.map(|p| p.name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use amenbo_core::binding::SlugMismatch;

    /// The warning names each side where the reader expects it: the slug the pointer carries as the
    /// project it claims to name, the project its id really names, and that project's own slug. Getting
    /// the three the wrong way round costs nothing at compile time and leaves a sentence that reads
    /// fluently while telling the reader the opposite of what happened.
    #[test]
    fn the_slug_warning_puts_each_side_where_the_reader_expects_it() {
        let w = slug_mismatch_sentence(&SlugMismatch {
            project_id: 1,
            recorded: "greenhouse".into(),
            actual: Some("workshop".into()),
        });
        assert!(w.contains("names project 'greenhouse'"), "the slug the pointer carries: {w}");
        assert!(w.contains("but AMB-P-1 is 'workshop'"), "and what that id really names: {w}");
        assert!(
            w.contains(&format!("`{} bind --project", Paths::command_name())),
            "the way out is a command to type, not a slug: {w}",
        );

        // The project the id names may have no slug of its own, and the sentence still has to say which
        // side is empty rather than leaving a blank pair of quotes.
        let none = slug_mismatch_sentence(&SlugMismatch {
            project_id: 2,
            recorded: "greenhouse".into(),
            actual: None,
        });
        assert!(none.contains("but AMB-P-2 is '(no slug)'"), "{none}");
    }
}
