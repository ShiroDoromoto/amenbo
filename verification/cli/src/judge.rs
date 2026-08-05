//! The judgments more than one domain makes, and the reads they all stand on.
//!
//! A verdict is a domain's to reach, but the *shape* of one repeats: is this row in that listing, does
//! this timeline carry that wording, does this field hold that value. Kept here, the wording a red line
//! comes back with is written once — a scenario that fails reads the same whichever domain it failed in.

use amenbo_scenario::Args;

use crate::{opt_bool, req_str, Driver, Outcome};

/// Judge a listing assert: is the row in this listing, and — when the step names a `position` — is it
/// where the order says it is. `position` is the only place a reorder shows: order is the store's to
/// keep and the key it keeps it by is opaque, so where a row sits is all a reader can ask about.
/// `listing` names the listing in the note, since a row can be absent from one and present in another
/// (the archived projects being exactly that).
pub(crate) fn judge_listing(
    noun: &str,
    id: i64,
    listing: &str,
    rows: &[serde_json::Value],
    with: &Args,
) -> Result<Outcome, String> {
    let at = rows.iter().position(|r| r["id"].as_i64() == Some(id));
    let found = at.is_some();
    if let Some(want) = with.get("position").and_then(|v| v.as_str()) {
        let last = rows.len().saturating_sub(1);
        let (pass, seen) = match (want, at) {
            ("first", Some(i)) => (i == 0, i),
            ("last", Some(i)) => (i == last, i),
            (other, Some(_)) => return Err(format!("`position: {other}` is not first or last")),
            (_, None) => (false, 0),
        };
        return Ok(Outcome::assert(
            pass,
            format!(
                "{noun} {id} sits at {} of {} in {listing} (expected {want}, {})",
                if found { seen.to_string() } else { "nowhere".to_string() },
                rows.len(),
                if pass { "as expected" } else { "MISMATCH" }
            ),
        ));
    }
    let present = opt_bool(with, "present").unwrap_or(true);
    let pass = found == present;
    Ok(Outcome::assert(
        pass,
        format!(
            "{noun} {id} {} {listing} (expected {}, {})",
            if found { "is present in" } else { "is absent from" },
            if present { "present" } else { "absent" },
            if pass { "as expected" } else { "MISMATCH" }
        ),
    ))
}

impl Driver<'_> {
    /// The one read a `found` assert stands on: the words a step names, and the narrowings it may name
    /// beside them. Shared by both sides because a search crosses them — the same invocation answers for
    /// a task and for a decision, and only the ref it is judged against differs.
    ///
    /// The words go over as separate arguments, which is how a person types them: a shell splits them,
    /// and the binary ANDs them.
    pub(crate) fn search(&self, with: &Args) -> Result<serde_json::Value, String> {
        let words = req_str(with, "words")?;
        let mut args: Vec<String> = vec!["search".into()];
        args.extend(words.split_whitespace().map(str::to_string));
        for key in ["kind", "filter"] {
            if let Some(value) = with.get(key).and_then(|v| v.as_str()) {
                args.push(format!("--{key}"));
                args.push(value.to_string());
            }
        }
        // The face narrowing, under a name of its own: `face` is already the face a step reads the
        // answer against, and the two are opposite ends of the same run — one goes in with the
        // question, the other is checked against what came back.
        if let Some(value) = with.get("only_face").and_then(|v| v.as_str()) {
            args.push("--face".into());
            args.push(value.to_string());
        }
        // The project is named the way every other step names one — by a binding a `project create`
        // left behind — because it is a record, not a word of the grammar. So it rides a flag of its
        // own here, and never goes into the filter expression.
        if with.contains_key("project") {
            args.push("--project".into());
            args.push(self.resolve_key(with, "project")?.to_string());
        }
        args.push("--json".into());
        self.run_json(&args.iter().map(String::as_str).collect::<Vec<_>>())
    }
}

/// Judge a search assert: is the word written on this record — and, when the step names a `face`, is it
/// written *there*. The hits carry the ref rather than a bare id, so the side is named by the caller
/// (`AMB-T` / `AMB-D`) and a task and a decision that happen to share a number cannot be mistaken for
/// each other.
///
/// A face is what separates this from a listing: a listing can only say a record matched, and what a
/// search is for is saying where. `present: false` is the same question in reverse — the proof that a
/// narrowing (`kind`, `only_face`, `filter`) really left something out.
///
/// `standing` asks the other half of what a row says: where the record it points at stands. It is put
/// to the rows this record owns, so a step naming it is asserting that the answer carries the state and
/// not merely the ref — which is what saves the reader the `show` the search was meant to replace.
///
/// The two questions a row is asked on the screen's road alone are turned away here instead of quietly
/// passing. What a row *calls* a place and where the words are marked inside its excerpt are both drawn
/// rather than reported: down this pipe the first is a `kind` and a comment ref for the reader to put
/// together, and the second is a pair of offsets. A step asking either would come back green off a build
/// that had stopped drawing it, which is the reading this gate exists to refuse.
pub(crate) fn judge_found(
    noun: &str,
    id_ref: &str,
    words: &str,
    with: &Args,
    hits: &serde_json::Value,
) -> Result<Outcome, String> {
    for key in ["landed_on", "marked"] {
        if with.contains_key(key) {
            return Err(format!(
                "`{key}` is a question about what the row draws, so it belongs on a `steps_gui` road where an eye reads it"
            ));
        }
    }
    let rows = hits.as_array().map(Vec::as_slice).unwrap_or(&[]);
    let face = with.get("face").and_then(|v| v.as_str());
    let mine: Vec<&serde_json::Value> =
        rows.iter().filter(|h| h["ref"].as_str() == Some(id_ref)).collect();
    let found = match face {
        Some(want) => mine.iter().any(|h| h["face"].as_str() == Some(want)),
        None => !mine.is_empty(),
    };
    // Where the record stands, asked of the rows it owns. A step naming it is one that expects the
    // record to be there, so a row that is missing the state fails as loudly as a wrong one.
    if let Some(want) = with.get("standing").and_then(|v| v.as_str()) {
        let said: Vec<&str> = mine.iter().filter_map(|h| h["standing"]["status"].as_str()).collect();
        let says = said.contains(&want);
        return Ok(Outcome::assert(
            says,
            format!(
                "{noun} {id_ref} is a place that carries `{words}`, and its row says it stands at {} (expected {want}, {})",
                match said.first() {
                    Some(s) => (*s).to_string(),
                    None => "nothing at all".to_string(),
                },
                if says { "as expected" } else { "MISMATCH" }
            ),
        ));
    }
    let present = opt_bool(with, "present").unwrap_or(true);
    let pass = found == present;
    let where_ = match face {
        Some(want) => format!(" on its {want}"),
        None => String::new(),
    };
    Ok(Outcome::assert(
        pass,
        format!(
            "{noun} {id_ref} {} `{words}`{where_} ({} hit(s) of its own, expected {}, {})",
            if found { "is a place that carries" } else { "is not a place that carries" },
            mine.len(),
            if present { "found" } else { "not found" },
            if pass { "as expected" } else { "MISMATCH" }
        ),
    ))
}

/// Judge a timeline assert: does this stream of entries carry the wording the step names? With no
/// `text` the question is only whether the stream has anything in it at all, which is what a
/// narrowed activity read (a `kind`) is asked. `present: false` is the same question in reverse —
/// the proof a comment that was deleted is really gone.
pub(crate) fn judge_timeline(noun: &str, id: i64, with: &Args, entries: &serde_json::Value) -> Result<Outcome, String> {
    let want = with.get("text").and_then(|v| v.as_str());
    let present = opt_bool(with, "present").unwrap_or(true);
    let rows = entries.as_array().map(Vec::as_slice).unwrap_or(&[]);
    let found = match want {
        Some(text) => rows.iter().any(|c| c["text"].as_str() == Some(text)),
        None => !rows.is_empty(),
    };
    let pass = found == present;
    Ok(Outcome::assert(
        pass,
        format!(
            "{noun} {id} timeline {} {} ({} entries, expected {}, {})",
            if found { "carries" } else { "does not carry" },
            want.map(|t| format!("`{t}`")).unwrap_or_else(|| "an entry".to_string()),
            rows.len(),
            if present { "carried" } else { "gone" },
            if pass { "as expected" } else { "MISMATCH" }
        ),
    ))
}

/// Read a field out of a `show --json` object, following a dotted path into what the output nests:
/// `placement.project.name` walks two objects, `blocked_by.0.name` indexes an array on the way. A
/// path that runs off the output is `None`, which the caller reports as a mismatch — a scenario
/// naming a path is asserting about the shape of the shipped output as much as about the value.
pub(crate) fn dig<'a>(shown: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    let mut here = shown;
    for step in path.split('.') {
        here = match step.parse::<usize>() {
            Ok(i) => here.get(i)?,
            Err(_) => here.get(step)?,
        };
    }
    Some(here)
}

/// Judge a `field` assert against a read of the thing it is about — an object's `show --json`, or
/// one of the reads the store answers about itself. `equals` is any scalar (string / bool / number /
/// null), compared structurally against the field's JSON value, so `status: todo` and `completed:
/// false` both work — and a field the output does not carry at all is a mismatch, not an error,
/// since a scenario naming one is asserting about the shipped output's shape too.
pub(crate) fn judge_field(subject: &str, with: &Args, shown: &serde_json::Value) -> Result<Outcome, String> {
    let field = req_str(with, "field")?;
    let expected = with.get("equals").ok_or("arg `equals` is required")?;
    let expected = serde_json::to_value(expected)
        .map_err(|e| format!("arg `equals` is not a valid value: {e}"))?;
    match dig(shown, field) {
        None => Ok(Outcome::assert(
            false,
            format!("{subject} has no field `{field}` in its output (MISMATCH)"),
        )),
        Some(actual) => {
            let pass = *actual == expected;
            Ok(Outcome::assert(
                pass,
                format!(
                    "{subject} field `{field}` = {actual} (expected {expected}, {})",
                    if pass { "as expected" } else { "MISMATCH" }
                ),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_dotted_path_walks_objects_and_indexes_arrays() {
        let v = json!({
            "placement": { "project": { "name": "verify" } },
            "blocked_by": [{ "name": "the blocker" }],
            "status": "todo",
        });
        assert_eq!(dig(&v, "status"), Some(&json!("todo")));
        assert_eq!(dig(&v, "placement.project.name"), Some(&json!("verify")));
        assert_eq!(dig(&v, "blocked_by.0.name"), Some(&json!("the blocker")));
    }

    /// A path that runs off the output comes back empty rather than landing on something else —
    /// the caller reports that as a mismatch, which is what a scenario naming a stale path deserves.
    #[test]
    fn a_path_that_does_not_exist_is_none() {
        let v = json!({ "blocked_by": [] });
        assert_eq!(dig(&v, "blocked_by.0.name"), None);
        assert_eq!(dig(&v, "placement.project.name"), None);
    }

    /// What only a screen draws is turned away here, and the turning away is the point: this pipe
    /// carries a `kind` and a pair of offsets where the screen carries a word and a highlight, so a
    /// step asking about either would come back green off a build that had stopped drawing it.
    #[test]
    fn the_two_questions_only_a_screen_can_answer_are_refused_here() {
        let hits = json!([{ "ref": "AMB-T-1", "face": "comment" }]);
        for key in ["landed_on", "marked"] {
            let with: Args = [(key.to_string(), serde_yaml::Value::from("anything"))].into();
            let Err(err) = judge_found("task", "AMB-T-1", "sweep", &with, &hits) else {
                panic!("`{key}` is a screen's question, and it has no answer down this pipe");
            };
            assert!(err.contains(key) && err.contains("steps_gui"), "got: {err}");
        }
    }
}
