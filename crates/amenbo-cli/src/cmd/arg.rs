//! Reading argument values the parser hands over as text: dates, priorities, views, a position
//! among siblings, and a body given inline or in a file.

use std::io::IsTerminal;

use chrono::NaiveDate;

use amenbo_core::model::{DimensionAppliesTo, Priority, View};
use amenbo_core::ops::Position;
use amenbo_core::time;

use crate::output::CliError;

pub(crate) fn parse_date_opt(s: &Option<String>) -> Result<Option<NaiveDate>, CliError> {
    match s {
        Some(v) => Ok(Some(
            time::parse_date(v, time::today()).map_err(CliError::from)?,
        )),
        None => Ok(None),
    }
}

pub(crate) fn parse_priority(s: &str) -> Result<Priority, CliError> {
    match s {
        "high" => Ok(Priority::High),
        "medium" => Ok(Priority::Medium),
        "low" => Ok(Priority::Low),
        other => Err(CliError {
            code: "invalid_value",
            message: format!("--priority value '{other}' is invalid."),
            hint: Some("Specify one of: high | medium | low.".to_string()),
            exit: 2,
        }),
    }
}

pub(crate) fn parse_view(s: &str) -> Result<View, CliError> {
    match s {
        "list" => Ok(View::List),
        "board" => Ok(View::Board),
        "calendar" => Ok(View::Calendar),
        "timeline" => Ok(View::Timeline),
        other => Err(CliError {
            code: "invalid_value",
            message: format!("--view value '{other}' is invalid."),
            hint: Some("Specify one of: list | board | calendar | timeline.".to_string()),
            exit: 2,
        }),
    }
}

/// Which side of the store a classification axis classifies (`AMB-D-789`). Spelled out here rather
/// than left to `DimensionAppliesTo::parse` so the refusal names the flag and lists what it takes, the
/// way `--priority`'s and `--view`'s do.
pub(crate) fn parse_applies_to(s: &str) -> Result<DimensionAppliesTo, CliError> {
    DimensionAppliesTo::parse(s).ok_or_else(|| CliError {
        code: "invalid_value",
        message: format!("--applies-to value '{s}' is invalid."),
        hint: Some("Specify one of: task | decision | both.".to_string()),
        exit: 2,
    })
}

/// The reorder position (`--top`/`--bottom`/`--before`/`--after`). The anchor is an id (an integer key)
/// within the same ordering.
pub(crate) fn pos_from_keys(top: bool, bottom: bool, before: Option<i64>, after: Option<i64>) -> Result<Position, CliError> {
    Position::from_flags(top, bottom, before, after).map_err(CliError::from)
}

/// A body-carrying argument, where the value `-` means "the body arrives on stdin" instead.
///
/// Bodies here are Markdown, and in practice they are thick with code spans — which a shell eats out of
/// a double-quoted `--text` argument by command substitution, silently, taking the word with it. `-` lets
/// the text reach Amenbo without passing through word expansion at all (a heredoc piped in).
///
/// `-` is the spelling because it is the only one that works on every body option: omitting the flag is
/// already spoken for and means something different per command ("empty" on an add, "leave it alone" on
/// an edit), so `hard-erase decision`'s implicit-stdin shape ([`read_body_input`]) does not generalize.
/// A terminal on stdin is refused rather than waited on, so a `-` typed by hand never looks like a hang.
pub(crate) fn body_arg(v: String) -> Result<String, CliError> {
    if v != "-" {
        return Ok(v);
    }
    if std::io::stdin().is_terminal() {
        return Err(CliError {
            code: "invalid_value",
            message: "`-` says the body comes in on stdin, but stdin is a terminal".to_string(),
            hint: Some("Pipe the body in (`… | amenbo … -`), or pass the text itself.".to_string()),
            exit: 2,
        });
    }
    use std::io::Read;
    let mut s = String::new();
    std::io::stdin().read_to_string(&mut s).map_err(|e| CliError {
        code: "io_error",
        message: format!("Cannot read the body from stdin: {e}"),
        hint: None,
        exit: 1,
    })?;
    Ok(s)
}

/// [`body_arg`] for an optional body — an absent flag stays absent (it is not a `-`).
pub(crate) fn body_arg_opt(v: Option<String>) -> Result<Option<String>, CliError> {
    v.map(body_arg).transpose()
}

/// The replacement body for `hard-erase decision`: `--body`, else `--body-file`, else stdin (for a
/// piped body). Refuses an interactive terminal with none of them given — a redaction must be
/// explicit about the new text, never an empty accident.
pub(crate) fn read_body_input(body: Option<String>, body_file: Option<String>) -> Result<String, CliError> {
    if let Some(b) = body {
        return Ok(b);
    }
    if let Some(f) = body_file {
        return std::fs::read_to_string(&f).map_err(|e| CliError {
            code: "io_error",
            message: format!("Cannot read --body-file {f}: {e}"),
            hint: None,
            exit: 1,
        });
    }
    if std::io::stdin().is_terminal() {
        return Err(CliError {
            code: "invalid_value",
            message: "no replacement body given".to_string(),
            hint: Some("Pass --body \"…\", --body-file <path>, or pipe the body on stdin.".to_string()),
            exit: 2,
        });
    }
    use std::io::Read;
    let mut s = String::new();
    std::io::stdin().read_to_string(&mut s).map_err(|e| CliError {
        code: "io_error",
        message: format!("Cannot read body from stdin: {e}"),
        hint: None,
        exit: 1,
    })?;
    Ok(s)
}
