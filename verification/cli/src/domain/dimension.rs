//! The `dimension` domain: the classification axes a project declares, their values, and the
//! assignment that files a task under one. Both travel as names, which is how the CLI takes them.

use amenbo_scenario::{Args, Domain};

use crate::{opt_bool, req_str, unmapped, Driver, Outcome};

impl Driver<'_> {
    pub(crate) fn dimension_action(&mut self, op: &str, with: &Args) -> Result<Outcome, String> {
        match op {
            "create" => {
                let name = req_str(with, "name")?;
                let pid = self.project_id.to_string();
                self.run_json(&["dimension", "add", "--name", name, "--project", &pid, "--json"])?;
                Ok(Outcome::action(format!("defined the axis `{name}`")))
            }
            "value-add" => {
                let dimension = req_str(with, "dimension")?;
                let value = req_str(with, "value")?;
                self.run_json(&["dimension", "value-add", dimension, "--name", value, "--json"])?;
                Ok(Outcome::action(format!("added the value `{value}` to `{dimension}`")))
            }
            // Filing a task under an axis and taking it back off. The axis and value go by name, which
            // is what the command takes — a bare number there would be read as a name, not an id.
            verb @ ("set" | "unset") => {
                let target = self.resolve(with)?;
                let dimension = req_str(with, "dimension")?;
                let value = req_str(with, "value")?;
                self.run_json(&["dimension", verb, &target.to_string(), dimension, value, "--json"])?;
                let note = match verb {
                    "set" => format!("filed task {target} under `{dimension}` = `{value}`"),
                    _ => format!("took task {target} out of `{dimension}` = `{value}`"),
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
                let found = match (axis, value) {
                    (None, _) => false,
                    (Some(_), None) => true,
                    (Some(a), Some(want)) => a["values"]
                        .as_array()
                        .map(|vs| vs.iter().any(|v| v["name"].as_str() == Some(want)))
                        .unwrap_or(false),
                };
                let pass = found == present;
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "axis `{dimension}`{} {} defined (expected {}, {})",
                        value.map(|v| format!(" value `{v}`")).unwrap_or_default(),
                        if found { "is" } else { "is not" },
                        if present { "defined" } else { "gone" },
                        if pass { "as expected" } else { "MISMATCH" }
                    ),
                ))
            }
            _ => Err(unmapped(Domain::Dimension, op)),
        }
    }
}
