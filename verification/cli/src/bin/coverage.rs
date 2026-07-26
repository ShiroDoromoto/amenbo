//! `verify-coverage` — count the scenario set against the capabilities amenbo declares.
//!
//! The denominator is not maintained here: it is the capability list the **shipped binary** prints
//! from `agent --json`, so it grows the moment amenbo grows and this count notices without anyone
//! remembering to update it. The numerator is the scenario set, one file per capability, named after
//! the command that capability leads with (`task assign` → `task-assign.yaml`, see
//! `verification/README.md`).
//!
//! Usage: `verify-coverage [<scenario-dir>] [--bin <amenbo>] [--json]`
//!   positional  the scenario directory to count (default: `scenarios/`)
//!   `--bin`     path to the amenbo binary to ask (default: `$AMENBO_BIN`, else `amenbo` on PATH)
//!   `--json`    emit the inventory as JSON instead of the human summary
//!
//! **A gap is not a failure.** Exit is 0 whether or not every capability is covered — an uncovered
//! line is work to file, not a reason to hold a release, and a gate that blocked on it would only
//! teach everyone to skip the gate. Non-zero is reserved for not being able to count at all: the
//! binary would not run, or the scenario directory would not be read.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use amenbo_verify_cli::json_string;

fn main() -> ExitCode {
    let opts = match Opts::parse(std::env::args().skip(1)) {
        Ok(o) => o,
        Err(msg) => {
            eprintln!("verify-coverage: {msg}");
            eprintln!("usage: verify-coverage [<scenario-dir>] [--bin <amenbo>] [--json]");
            return ExitCode::from(2);
        }
    };

    let capabilities = match declared_capabilities(&opts.bin) {
        Ok(c) => c,
        Err(msg) => {
            eprintln!("verify-coverage: {msg}");
            return ExitCode::from(2);
        }
    };
    let owned = match scenario_files(&opts.dir) {
        Ok(s) => s,
        Err(msg) => {
            eprintln!("verify-coverage: {msg}");
            return ExitCode::from(2);
        }
    };

    let uncovered: Vec<&Capability> =
        capabilities.iter().filter(|c| !owned.stems.contains(&c.slug)).collect();
    // A file whose stem answers for no capability: a leftover from a capability that went, or a name
    // that never matched one. Either way nothing keeps it honest, so it is worth saying out loud.
    let slugs: BTreeSet<&str> = capabilities.iter().map(|c| c.slug.as_str()).collect();
    let unowned: Vec<&String> = owned.stems.iter().filter(|s| !slugs.contains(s.as_str())).collect();

    if opts.json {
        println!("{}", inventory_json(&capabilities, &uncovered, &unowned, &owned.misfiled));
    } else {
        print_human(&capabilities, &uncovered, &unowned, &owned.misfiled);
    }
    ExitCode::SUCCESS
}

/// One capability as amenbo declares it: the prose it is listed under, the commands it names, and
/// the slug the scenario set files it as.
struct Capability {
    text: String,
    commands: Vec<String>,
    slug: String,
}

/// Ask the shipped binary for its own capability list.
///
/// `agent` reads no store, but it is still run inside a throwaway session: an amenbo asked from a
/// folder it is not bound to answers `no_pointer` before it answers anything else, and an
/// `AMENBO_HOME` of its own is what lets it speak without reaching for the user's data.
fn declared_capabilities(bin: &Path) -> Result<Vec<Capability>, String> {
    let session = amenbo_verify_cli::scratch::session("coverage", false)
        .map_err(|e| format!("could not create a throwaway store: {e}"))?;
    let out = Command::new(bin)
        .args(["agent", "--json"])
        .current_dir(&session.cwd)
        .env("AMENBO_HOME", &session.home)
        .env("AMENBO_UPDATE_CHECK", "0")
        .env("NO_COLOR", "1")
        .output()
        .map_err(|e| format!("could not run `{}`: {e}", bin.display()))?;
    if !out.status.success() {
        return Err(format!("`amenbo agent --json` failed: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    let doc: serde_json::Value = serde_json::from_slice(&out.stdout)
        .map_err(|e| format!("`amenbo agent --json` did not print JSON: {e}"))?;
    let listed = doc
        .get("capabilities")
        .and_then(|c| c.as_array())
        .ok_or("`amenbo agent --json` carries no `capabilities` array")?;

    let mut out = Vec::new();
    for entry in listed {
        let text = entry.get("capability").and_then(|t| t.as_str()).unwrap_or("").to_string();
        let commands: Vec<String> = entry
            .get("commands")
            .and_then(|c| c.as_array())
            .map(|a| a.iter().filter_map(|c| c.as_str()).map(str::to_string).collect())
            .unwrap_or_default();
        // The file is named after the command the capability leads with — the prose gets reworded,
        // the command does not. A capability that names none cannot be filed, and saying so beats
        // counting it as covered.
        let Some(first) = commands.first() else {
            return Err(format!("capability `{text}` names no command, so nothing can own it"));
        };
        let slug = slug_of(first);
        out.push(Capability { text, commands, slug });
    }
    Ok(out)
}

/// The file name a capability's first command files it under: the command with its spaces closed up
/// into dashes (`task commit add` → `task-commit-add`).
fn slug_of(command: &str) -> String {
    command.replace(' ', "-")
}

/// What the scenario directory holds: the stem of every scenario file — the capabilities the set
/// claims to own — and any file whose `id` has drifted from its name.
struct Owned {
    stems: BTreeSet<String>,
    /// `(file stem, the id inside it)`. The two are one handle wearing two hats: the name is what a
    /// count matches on, the id is what a report prints, and a file where they disagree is filed
    /// under one capability and reported as another.
    misfiled: Vec<(String, String)>,
}

fn scenario_files(dir: &Path) -> Result<Owned, String> {
    let entries =
        std::fs::read_dir(dir).map_err(|e| format!("cannot read directory {}: {e}", dir.display()))?;
    let mut owned = Owned { stems: BTreeSet::new(), misfiled: Vec::new() };
    for entry in entries {
        let path = entry.map_err(|e| format!("cannot read {}: {e}", dir.display()))?.path();
        let is_yaml = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e == "yaml" || e == "yml");
        if !path.is_file() || !is_yaml {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else { continue };
        // A file that will not parse is the lint's to shout about — it runs in the same gate, and
        // saying it twice in two voices helps nobody. Here it simply carries its name.
        if let Ok(scenario) = amenbo_scenario::load_file(&path) {
            if scenario.id != stem {
                owned.misfiled.push((stem.to_string(), scenario.id.clone()));
            }
        }
        owned.stems.insert(stem.to_string());
    }
    Ok(owned)
}

fn print_human(
    capabilities: &[Capability],
    uncovered: &[&Capability],
    unowned: &[&String],
    misfiled: &[(String, String)],
) {
    let total = capabilities.len();
    let covered = total - uncovered.len();
    println!("{covered}/{total} capabilities have a scenario file");
    if !uncovered.is_empty() {
        println!("---");
        println!("uncovered:");
        for c in uncovered {
            println!("  {}.yaml — {}", c.slug, c.text);
        }
    }
    if !unowned.is_empty() {
        println!("---");
        println!("scenario files answering for no capability:");
        for stem in unowned {
            println!("  {stem}.yaml");
        }
    }
    if !misfiled.is_empty() {
        println!("---");
        println!("scenario files whose id is not their name:");
        for (stem, id) in misfiled {
            println!("  {stem}.yaml carries id `{id}`");
        }
    }
}

/// The machine face: the same inventory a release's stock-take reads, and what a filing session
/// splits into tasks.
fn inventory_json(
    capabilities: &[Capability],
    uncovered: &[&Capability],
    unowned: &[&String],
    misfiled: &[(String, String)],
) -> String {
    let items: Vec<String> = uncovered
        .iter()
        .map(|c| {
            let commands: Vec<String> = c.commands.iter().map(|s| json_string(s)).collect();
            format!(
                "{{\"slug\":{},\"capability\":{},\"commands\":[{}]}}",
                json_string(&c.slug),
                json_string(&c.text),
                commands.join(",")
            )
        })
        .collect();
    let strays: Vec<String> = unowned.iter().map(|s| json_string(s)).collect();
    let drifted: Vec<String> = misfiled
        .iter()
        .map(|(stem, id)| format!("{{\"file\":{},\"id\":{}}}", json_string(stem), json_string(id)))
        .collect();
    format!(
        "{{\"total\":{},\"covered\":{},\"uncovered\":[{}],\"unowned\":[{}],\"misfiled\":[{}]}}",
        capabilities.len(),
        capabilities.len() - uncovered.len(),
        items.join(","),
        strays.join(","),
        drifted.join(",")
    )
}

// ---------------------------------------------------------------------------
// Command line
// ---------------------------------------------------------------------------

struct Opts {
    dir: PathBuf,
    bin: PathBuf,
    json: bool,
}

impl Opts {
    fn parse(args: impl Iterator<Item = String>) -> Result<Opts, String> {
        let mut dir = None;
        let mut bin = std::env::var_os("AMENBO_BIN").map(PathBuf::from);
        let mut json = false;
        let mut it = args.peekable();
        while let Some(a) = it.next() {
            match a.as_str() {
                "--json" => json = true,
                "--bin" => bin = Some(PathBuf::from(it.next().ok_or("--bin needs a path")?)),
                s if s.starts_with("--") => return Err(format!("unknown flag `{s}`")),
                _ if dir.is_none() => dir = Some(PathBuf::from(a)),
                s => return Err(format!("only one scenario directory is taken (got `{s}` as well)")),
            }
        }
        Ok(Opts {
            dir: dir.unwrap_or_else(|| PathBuf::from("scenarios")),
            bin: bin.unwrap_or_else(|| PathBuf::from("amenbo")),
            json,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The file name is the capability's first command with its spaces closed up — the whole naming
    /// rule, and the one thing a count depends on being able to derive.
    #[test]
    fn a_command_files_under_its_dashed_name() {
        assert_eq!(slug_of("task add"), "task-add");
        assert_eq!(slug_of("task commit add"), "task-commit-add");
        assert_eq!(slug_of("status"), "status");
    }
}
