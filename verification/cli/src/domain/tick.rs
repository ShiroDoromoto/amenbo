//! The `tick` domain: the machine's own scheduler waking amenbo, and what amenbo works out once it
//! is awake.
//!
//! **The wake is the assert.** A tick leaves a day mark behind and nothing else a reader can ask
//! for, so what it says on the way past is the whole of what can be judged — and a command written
//! to read that mark would be a face built for this harness rather than for anybody. Carrying the
//! turn out and judging what came back is what a scheduler itself would have got.
//!
//! **Nothing here registers a timer.** The registration is written outside the throwaway store a run
//! makes — into the launchd, systemd or Task Scheduler of whichever machine the gate is running on —
//! so a road that walked it would leave an hourly timer on a release box. Only the reading is here.

use amenbo_scenario::{Args, Domain};

use crate::{req_bool, req_str, unmapped, Driver, Outcome};

impl Driver<'_> {
    pub(crate) fn tick_assert(&self, op: &str, with: &Args) -> Result<Outcome, String> {
        match op {
            // One hour's turn, and whether the named purpose was carried out on it. The two answers
            // a tick gives about a purpose are opposite sides of the same rule: it ran, or the day it
            // is owed for was already marked. Both are read, so a purpose the build knows nothing
            // about is neither and comes out red rather than passing as "not carried out".
            "woken" => {
                let purpose = req_str(with, "purpose")?;
                let want = req_bool(with, "carried_out")?;
                // A tick exits non-zero when a purpose failed, and that is a verdict rather than a
                // failure to run — so the report is read either way and the failure is named below.
                let v = self.run_check(&["tick", "run", "--json"])?;
                let named = |key: &str| {
                    v[key]
                        .as_array()
                        .map(Vec::as_slice)
                        .unwrap_or(&[])
                        .iter()
                        .any(|p| p.as_str() == Some(purpose))
                };
                let ran = named("ran");
                let held = named("already_done");
                let why = v["failed"]
                    .as_array()
                    .and_then(|all| all.iter().find(|f| f["purpose"].as_str() == Some(purpose)))
                    .and_then(|f| f["error"].as_str());
                let pass = ran == want && (ran || held) && why.is_none();
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "the tick {} `{purpose}`{} (expected it to have {}, {})",
                        match (ran, held) {
                            (true, _) => "carried out",
                            (false, true) => "left the day already marked for",
                            (false, false) => "knows nothing of",
                        },
                        why.map(|e| format!(", failing with {e}")).unwrap_or_default(),
                        if want { "run" } else { "stood down" },
                        if pass { "as expected" } else { "MISMATCH" }
                    ),
                ))
            }
            // What the scheduler is holding. Read-only by design, and false is what a run should
            // always find: nothing in this harness registers a timer, so a true here is a machine
            // carrying one from somewhere else — which is worth reading as much as the answer is.
            "holds" => {
                let want = req_bool(with, "registered")?;
                let v = self.run_json(&["tick", "status", "--json"])?;
                let registered = v["registered"].as_bool().unwrap_or(false);
                let pass = registered == want;
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "the scheduler {} the hourly tick (expected {}, {})",
                        if registered { "is holding" } else { "holds nothing of ours" },
                        if want { "held" } else { "nothing" },
                        if pass { "as expected" } else { "MISMATCH" }
                    ),
                ))
            }
            _ => Err(unmapped(Domain::Tick, op)),
        }
    }
}
