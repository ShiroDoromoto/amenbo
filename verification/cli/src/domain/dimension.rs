//! The `dimension` domain: the classification axes a project declares, their values, and the
//! assignment that files a task under one. Both travel as words — a name, or the readable key the row
//! answers to — which is how the CLI takes them either way.

use amenbo_scenario::{Args, Domain};

use crate::{opt_bool, req_str, unmapped, Driver, Outcome};

impl Driver<'_> {
    pub(crate) fn dimension_action(&mut self, op: &str, with: &Args) -> Result<Outcome, String> {
        match op {
            "create" => {
                let name = req_str(with, "name")?;
                let pid = self.project_id.to_string();
                let mut args = vec!["dimension", "add", "--name", name, "--project", &pid];
                // The key is named only where a road names it; left out, the door derives one from the
                // id, and passing an empty `--slug` would be a different request altogether.
                if let Some(slug) = slug(with) {
                    args.extend_from_slice(&["--slug", slug]);
                }
                args.push("--json");
                self.run_json(&args)?;
                Ok(Outcome::action(format!("defined the axis `{name}`{}", named_key(with))))
            }
            "value-add" => {
                let dimension = req_str(with, "dimension")?;
                let value = req_str(with, "value")?;
                let mut args = vec!["dimension", "value-add", dimension, "--name", value];
                if let Some(slug) = slug(with) {
                    args.extend_from_slice(&["--slug", slug]);
                }
                args.push("--json");
                self.run_json(&args)?;
                Ok(Outcome::action(format!(
                    "added the value `{value}` to `{dimension}`{}",
                    named_key(with)
                )))
            }
            // The value gone again, with the tasks that answered with it either going with it or going
            // somewhere `to` names. `--yes` rides along because the command asks before it deletes and a
            // driver has nobody to answer with — and it asks *before* the guards, so a refusal walked
            // without it would come back as the unanswered question rather than as the guard the road is
            // about.
            "value-rm" => {
                let dimension = req_str(with, "dimension")?;
                let value = req_str(with, "value")?;
                let mut args = vec!["dimension", "value-rm", dimension, value];
                if let Some(to) = destination(with) {
                    args.extend_from_slice(&["--reassign-to", to]);
                }
                args.extend_from_slice(&["--yes", "--json"]);
                self.run_json(&args)?;
                Ok(Outcome::action(match destination(with) {
                    Some(to) => format!(
                        "removed `{value}` from `{dimension}`, carrying the tasks on it over to `{to}`"
                    ),
                    None => format!("removed `{value}` from `{dimension}`"),
                }))
            }
            // Renaming the key afterwards, on the axis or on one of its values. Two commands behind one
            // op because the two rows are the same thing to a reader — what a key is for is being typed
            // outside Amenbo, and neither an axis nor a value is more outside than the other.
            "rekey" => {
                let dimension = req_str(with, "dimension")?;
                let slug = req_str(with, "slug")?;
                match with.get("value").and_then(|v| v.as_str()) {
                    Some(value) => {
                        self.run_json(&[
                            "dimension",
                            "value-update",
                            dimension,
                            value,
                            "--slug",
                            slug,
                            "--json",
                        ])?;
                        Ok(Outcome::action(format!(
                            "made `{value}` of `{dimension}` answer to `{slug}`"
                        )))
                    }
                    None => {
                        self.run_json(&["dimension", "update", dimension, "--slug", slug, "--json"])?;
                        Ok(Outcome::action(format!("made `{dimension}` answer to `{slug}`")))
                    }
                }
            }
            // Whether the axis goes on the board's task cards. It is the axis's own answer and not a
            // reader's setting, so a screen road stands it up through here — the world its `given:`
            // declares — rather than reaching for the toggle that also writes it.
            "show-on-card" => {
                let dimension = req_str(with, "dimension")?;
                let show = opt_bool(with, "show").unwrap_or(true);
                let flag = if show { "true" } else { "false" };
                self.run_json(&["dimension", "update", dimension, "--show-on-card", flag, "--json"])?;
                let note = match show {
                    true => format!("put `{dimension}` on the task card"),
                    false => format!("took `{dimension}` off the task card"),
                };
                Ok(Outcome::action(note))
            }
            // Whether the axis refuses to be left empty. The command takes the answer as a word rather
            // than as a bare switch, since it is lowered as often as it is raised.
            "required" => {
                let dimension = req_str(with, "dimension")?;
                let demand = opt_bool(with, "required").unwrap_or(true);
                let flag = if demand { "true" } else { "false" };
                self.run_json(&["dimension", "update", dimension, "--required", flag, "--json"])?;
                let note = match demand {
                    true => format!("made `{dimension}` demand an answer"),
                    false => format!("stopped `{dimension}` demanding an answer"),
                };
                Ok(Outcome::action(note))
            }
            // How many of the axis's values one record may answer it with. A flag like the two above
            // it, and the one whose lowering the store can refuse: `--cardinality single` on an axis
            // some record still answers with several is turned away with the count, rather than
            // quietly dropping every value but one.
            "cardinality" => {
                let dimension = req_str(with, "dimension")?;
                let several = opt_bool(with, "multi").unwrap_or(true);
                let word = if several { "multi" } else { "single" };
                self.run_json(&["dimension", "update", dimension, "--cardinality", word, "--json"])?;
                let note = match several {
                    true => format!("let one record answer `{dimension}` with several values"),
                    false => format!("held `{dimension}` back to one value per record"),
                };
                Ok(Outcome::action(note))
            }
            // Which side of the store the axis classifies at all. The answer is a word and not a
            // switch — there are three of them, and `both` is where every axis starts — so a road
            // takes this door to narrow, and takes it again with `both` to widen back.
            "applies-to" => {
                let dimension = req_str(with, "dimension")?;
                let side = side(with)?;
                self.run_json(&["dimension", "update", dimension, "--applies-to", side, "--json"])?;
                let note = match side {
                    "both" => format!("offered `{dimension}` on both sides again"),
                    one => format!("narrowed `{dimension}` to {one}s alone"),
                };
                Ok(Outcome::action(note))
            }
            // Which axis is the project's time axis. A flag like the two above it, and the same door —
            // the axis exists first, and the role is named on it.
            "time-axis" => {
                let dimension = req_str(with, "dimension")?;
                let named = opt_bool(with, "time_axis").unwrap_or(true);
                let flag = if named { "true" } else { "false" };
                self.run_json(&["dimension", "update", dimension, "--time-axis", flag, "--json"])?;
                let note = match named {
                    true => format!("made `{dimension}` the project's time axis"),
                    false => format!("stopped `{dimension}` being the project's time axis"),
                };
                Ok(Outcome::action(note))
            }
            // The window one of its values covers. Either end alone is a period — an open end is
            // unbounded on that side — so the road writes the ends it means and the command leaves the
            // rest as it found them; writing neither would be a step that asked for nothing, and is
            // refused here rather than sent as a command that changes nothing.
            "period" => {
                let dimension = req_str(with, "dimension")?;
                let value = req_str(with, "value")?;
                let start = with.get("start").and_then(|v| v.as_str());
                let end = with.get("end").and_then(|v| v.as_str());
                if start.is_none() && end.is_none() {
                    return Err("action `period` names neither `start` nor `end`".to_string());
                }
                let mut args = vec!["dimension", "value-update", dimension, value];
                if let Some(start) = start {
                    args.extend_from_slice(&["--start", start]);
                }
                if let Some(end) = end {
                    args.extend_from_slice(&["--end", end]);
                }
                args.push("--json");
                self.run_json(&args)?;
                let note = match (start, end) {
                    (Some(s), Some(e)) => format!("gave `{value}` of `{dimension}` the period {s} — {e}"),
                    (Some(s), None) => format!("started `{value}` of `{dimension}` at {s}, open-ended"),
                    (None, Some(e)) => format!("ended `{value}` of `{dimension}` at {e}, open at the start"),
                    (None, None) => unreachable!("the empty period is refused above"),
                };
                Ok(Outcome::action(note))
            }
            // Filing a task or a decision under an axis, and taking it back off. The axis and value go
            // by name, which is what the command takes — a bare number there would be read as a name,
            // not an id. The target goes as a reference: the command takes either kind on that argument,
            // and a bare number would not say which of the two is meant.
            verb @ ("set" | "unset") => {
                let target = self.resolve_ref(with)?;
                let dimension = req_str(with, "dimension")?;
                let value = req_str(with, "value")?;
                self.run_json(&["dimension", verb, &target, dimension, value, "--json"])?;
                let note = match verb {
                    "set" => format!("filed {target} under `{dimension}` = `{value}`"),
                    _ => format!("took {target} out of `{dimension}` = `{value}`"),
                };
                Ok(Outcome::action(note))
            }
            _ => Err(unmapped(Domain::Dimension, op)),
        }
    }
    pub(crate) fn dimension_assert(&self, op: &str, with: &Args) -> Result<Outcome, String> {
        match op {
            "listed" => {
                let dimension = req_str(with, "dimension")?;
                let value = with.get("value").and_then(|v| v.as_str());
                let side = with.get("side").and_then(|v| v.as_str());
                let present = opt_bool(with, "present").unwrap_or(true);
                let v = self.run_json(&["dimension", "list", "--json"])?;
                let axis = v["dimensions"]
                    .as_array()
                    .map(Vec::as_slice)
                    .unwrap_or(&[])
                    .iter()
                    .find(|d| d["dimension"]["name"].as_str() == Some(dimension));
                // Without a `value` the question is whether the axis is defined at all; with one it is
                // whether the axis carries that value, since a value is only ever read through its axis.
                let defined = match (axis, value) {
                    (None, _) => false,
                    (Some(_), None) => true,
                    (Some(a), Some(want)) => a["values"]
                        .as_array()
                        .map(|vs| vs.iter().any(|v| v["name"].as_str() == Some(want)))
                        .unwrap_or(false),
                };
                // A `side` narrows the question to whether that side is offered the axis at all. An
                // axis narrowed off a side stays on this listing — being defined is not being offered —
                // so the answer folds the axis's own `applies_to` into the row it was read from rather
                // than taking the row's presence for it.
                let found = defined && axis.is_some_and(|a| classifies(a, side));
                let pass = found == present;
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "axis `{dimension}`{} {} {} (expected {}, {})",
                        value.map(|v| format!(" value `{v}`")).unwrap_or_default(),
                        if found { "is" } else { "is not" },
                        side.map(|s| format!("offered to {s}s")).unwrap_or_else(|| "defined".to_string()),
                        if present { "it to be" } else { "it not to be" },
                        if pass { "as expected" } else { "MISMATCH" }
                    ),
                ))
            }
            // The key a row answers to, read back off the same listing `listed` reads. An axis carries
            // one and so does every value, and which of the two is being read is said by whether the
            // step named a value — the way it is said everywhere else in this domain.
            "key" => {
                let dimension = req_str(with, "dimension")?;
                let value = with.get("value").and_then(|v| v.as_str());
                let want = req_str(with, "equals")?;
                let v = self.run_json(&["dimension", "list", "--json"])?;
                let axis = v["dimensions"]
                    .as_array()
                    .map(Vec::as_slice)
                    .unwrap_or(&[])
                    .iter()
                    .find(|d| d["dimension"]["name"].as_str() == Some(dimension))
                    .ok_or_else(|| format!("no axis named `{dimension}` to read a key off"))?;
                let held = match value {
                    None => axis["dimension"]["slug"].as_str(),
                    Some(name) => axis["values"]
                        .as_array()
                        .map(Vec::as_slice)
                        .unwrap_or(&[])
                        .iter()
                        .find(|v| v["name"].as_str() == Some(name))
                        .ok_or_else(|| format!("`{dimension}` has no value named `{name}`"))?
                        ["slug"]
                        .as_str(),
                };
                let pass = held == Some(want);
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "{} answers to `{}` (expected `{want}`, {})",
                        value
                            .map(|v| format!("value `{v}` of `{dimension}`"))
                            .unwrap_or_else(|| format!("axis `{dimension}`")),
                        held.unwrap_or("<no key>"),
                        if pass { "as expected" } else { "MISMATCH" }
                    ),
                ))
            }
            _ => Err(unmapped(Domain::Dimension, op)),
        }
    }
}

/// Which side of the store a step is talking about, checked against what the flag takes rather than
/// passed through: the loader already holds the word to this set, and a driver that let a fourth one
/// past would hand the command a value it would refuse for a reason the road never meant.
fn side(with: &Args) -> Result<&str, String> {
    match req_str(with, "side")? {
        s @ ("task" | "decision" | "both") => Ok(s),
        other => Err(format!("`side` must be `task`, `decision` or `both`, not `{other}`")),
    }
}

/// Whether the axis on this row classifies the side a step named — `both` covering either. No side
/// named is no question asked, which is every road written before an axis could say which side it
/// classifies at all.
fn classifies(axis: &serde_json::Value, side: Option<&str>) -> bool {
    let Some(want) = side else { return true };
    matches!(axis["dimension"]["applies_to"].as_str(), Some(a) if a == want || a == "both")
}

/// The key a step named, where it named one. Absent is not the same as empty: a row created without
/// one takes the key its id gives it, which is what the door does when the flag is left off.
fn slug(with: &Args) -> Option<&str> {
    with.get("slug").and_then(|v| v.as_str())
}

/// Where a removal sends the tasks that answered with the value going away, where a step named
/// somewhere. Absent is the value's classifications going with it, which is what an axis that demands
/// nothing does — and what a required axis refuses to do.
fn destination(with: &Args) -> Option<&str> {
    with.get("to").and_then(|v| v.as_str())
}

/// The tail an action's note carries when the step named a key, so the run's log says which of the
/// two doors was walked — the one that names a key, or the one that leaves the id to give it.
fn named_key(with: &Args) -> String {
    slug(with).map(|s| format!(", answering to `{s}`")).unwrap_or_default()
}
