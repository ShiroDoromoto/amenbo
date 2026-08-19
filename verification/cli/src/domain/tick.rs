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
    /// Whether the machine's scheduler is holding the hourly tick right now. It is read here and at
    /// the start of a run, and the assert is the difference between the two — see `holds` below.
    pub(crate) fn tick_registered(&self) -> Result<bool, String> {
        let v = self.run_json(&["tick", "status", "--json"])?;
        Ok(v["registered"].as_bool().unwrap_or(false))
    }

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
            // What the run did to the scheduler's registration, and never what the machine holds.
            // The registration lives outside the throwaway store, in the launchd, systemd or Task
            // Scheduler this run was started on, so the absolute reading is the machine's own answer
            // and not this road's: on a machine where somebody uses the hourly tick it is `true`
            // before anything walks. What a road can honestly say is the difference across it —
            // `changed: false` being "left as it was found", which is what every road here is owed
            // since nothing in this harness registers a timer.
            "holds" => {
                let want = req_bool(with, "changed")?;
                let now = self.tick_registered()?;
                let changed = now != self.tick_at_start;
                let pass = changed == want;
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "the run left the scheduler {} ({}; expected {}, {})",
                        if changed { "holding something else" } else { "as it was found" },
                        match (self.tick_at_start, now) {
                            (true, true) => "it was holding the hourly tick before and still is",
                            (false, false) => "it was holding nothing of ours before and still is",
                            (false, true) => "it was holding nothing of ours before and holds the hourly tick now",
                            (true, false) => "it was holding the hourly tick before and holds nothing of ours now",
                        },
                        if want { "a change" } else { "no change" },
                        if pass { "as expected" } else { "MISMATCH" }
                    ),
                ))
            }
            _ => Err(unmapped(Domain::Tick, op)),
        }
    }
}
