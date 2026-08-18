//! `tick run`: the hourly wake-up the scheduler starts amenbo through, and what it reports for it.
//! Its siblings — the answer, and the registration the answer stands beside — are `setup`'s.

use serde_json::json;

use amenbo_core::Store;

use crate::output::{human, print_json, CliError, Flags};

/// Carry out one tick and say what it did (`AMB-D-706`).
///
/// The judgement is the core's ([`amenbo_core::tick`]); this is the reading. Whether a purpose failed is
/// the one thing the tick can report **only** through its exit code and its stderr: nobody is waiting on
/// this process, and a scheduler keeps what a run wrote and what it exited with. Delivery is not held to
/// that — a failed run is dropped rather than retried, and the execution log is where each one's outcome is
/// written (`AMB-D-399`, `AMB-D-361`).
pub(crate) fn tick_run_cmd(store: &Store, flags: &Flags) -> Result<i32, CliError> {
    let day = amenbo_core::time::today();
    let report = amenbo_core::tick::run(store, day).map_err(CliError::from)?;

    for (purpose, why) in &report.failed {
        eprintln!("✗ {purpose}: {why}");
    }

    let now = amenbo_core::time::Timestamp::now().to_rfc3339_z();
    if flags.json {
        print_json(&json!({
            "ok": report.failed.is_empty(),
            "action": "tick.run",
            "day": amenbo_core::time::date_to_string(day),
            "ran": report.ran,
            "already_done": report.already_done,
            "failed": report.failed.iter().map(|(purpose, why)| json!({
                "purpose": purpose,
                "error": why,
            })).collect::<Vec<_>>(),
            "delivered": report.delivered(),
            "flushed": report.worked.iter().map(|w| json!({
                "plugin": w.plugin,
                "delivered": w.delivered,
                "left": w.left,
            })).collect::<Vec<_>>(),
            "queues": report.left.iter().map(|w| json!({
                "plugin": w.depth.plugin,
                "waiting": w.depth.waiting,
                "oldest": w.depth.oldest,
                "running": w.is_running(&now),
            })).collect::<Vec<_>>(),
        }));
        return Ok(exit_code(&report));
    }

    let ran = match report.ran.len() {
        0 => "nothing was owed today".to_string(),
        1 => format!("carried out {}", report.ran[0]),
        n => format!("carried out {n}: {}", report.ran.join(", ")),
    };
    let events = report.delivered();
    let plural = if events == 1 { "event" } else { "events" };
    human(flags, format!("tick  {ran}; {events} {plural} delivered"));
    for w in &report.left {
        human(
            flags,
            format!(
                "  {}  {} waiting{}",
                w.depth.plugin,
                w.depth.waiting,
                if w.is_running(&now) { " (a runner is on it)" } else { "" },
            ),
        );
    }
    Ok(exit_code(&report))
}

/// A purpose that did not run is the one failure this process can report, so it leaves a non-zero exit.
/// Everything else a tick meets — nothing owed, an empty queue, a queue another runner is holding — is the
/// ordinary shape of being woken, and exits 0.
fn exit_code(report: &amenbo_core::tick::Report) -> i32 {
    i32::from(!report.failed.is_empty())
}
