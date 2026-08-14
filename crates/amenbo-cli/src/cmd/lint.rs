//! `lint`, and the two git hook faces that call it — reading text for an amenbo ref on its way
//! out of this store. Opens no store, so it runs anywhere, CI included.

use serde_json::json;

use crate::output::{human, print_json, CliError, Flags};

/// `amenbo lint`: report the amenbo refs in text on its way out of this store, and exit non-zero if there
/// are any. The exit code is the whole verdict, because the callers that matter are machines: a git hook
/// and CI both judge by it, and an AI runs this before it commits — so a hit is not rendered as a
/// `CliError`, since the run succeeded and what it found is the finding. `--quiet` silences the report and
/// leaves the code, for a caller that wants only the verdict; the hook amenbo installs does not pass it,
/// because a person whose commit was just refused has to be told what refused it. It opens no store: it
/// reads the text it is handed and judges it on the `AMB-` prefix alone.
pub(crate) fn lint_cmd(flags: &Flags, paths: Vec<String>, stdin: bool) -> Result<i32, CliError> {
    let (hits, scanned) = if stdin {
        use std::io::Read;
        let mut text = String::new();
        std::io::stdin().read_to_string(&mut text).map_err(|e| CliError {
            code: "io_error",
            message: format!("Cannot read from stdin: {e}"),
            hint: None,
            exit: 1,
        })?;
        (amenbo_core::lint::scan_text(STDIN_LABEL, &text), STDIN_LABEL.to_string())
    } else if !paths.is_empty() {
        let mut hits = Vec::new();
        for path in &paths {
            let text = std::fs::read_to_string(path).map_err(|e| CliError {
                code: "io_error",
                message: format!("Cannot read {path}: {e}"),
                hint: None,
                exit: 1,
            })?;
            hits.extend(amenbo_core::lint::scan_text(path, &text));
        }
        (hits, paths.join(", "))
    } else {
        // The default: what `git commit` is about to record, from wherever the caller stands (a hook runs at
        // the repo root, a person may not).
        let cwd = std::env::current_dir().map_err(|e| CliError {
            code: "io_error",
            message: format!("Cannot read the current directory: {e}"),
            hint: None,
            exit: 1,
        })?;
        let diff = amenbo_core::lint::staged_diff(&cwd).map_err(CliError::from)?;
        (amenbo_core::lint::scan_diff(&diff), "the staged diff".to_string())
    };

    if flags.json {
        print_json(&json!({ "ok": hits.is_empty(), "scanned": scanned, "count": hits.len(), "hits": hits }));
    } else if hits.is_empty() {
        human(flags, format!("lint: ok — no amenbo refs in {scanned}."));
    } else {
        for h in &hits {
            human(flags, format!("{}:{}: {}", h.path, h.line, h.reference));
        }
        human(
            flags,
            format!(
                "✗ lint: {} amenbo ref(s) in {scanned}. An id resolves only in this store — remove them, or spell out what they say.",
                hits.len()
            ),
        );
    }
    Ok(if hits.is_empty() { 0 } else { 1 })
}

/// What the report calls piped text — no path to name it by, so name the stream.
const STDIN_LABEL: &str = "<stdin>";
