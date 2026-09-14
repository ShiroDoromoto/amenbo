//! The `viewer` domain: the phone that reads this store, and the server it reads from in the
//! reader's own Cloudflare account.
//!
//! **Nothing here stands a server up.** Setup builds a Worker and a database in an account somebody
//! owns, off an API token for it, and a gate that asked for one would hold a release on whose
//! account it was and on what building in it cost. So what this driver walks is everything before
//! that — the same line `domain::notify` draws in front of sending:
//!
//! - **where the app is got**, which needs no server and no account and so answers in full on a
//!   device nobody has touched;
//! - **the refusal that names the way forward**, which is what every road needing a server answers
//!   with until one exists — and is an answer rather than a failure, a device nobody has set the
//!   Viewer up on not being a device that is failing at anything;
//! - **the read code never reaching anywhere but a screen**, which is the whole of how the key
//!   travels and is refused into a pipe and into `--json` before a code is issued at all.
//!
//! **The switch is not here either, and that is not a gap.** The terminal has no word for it — the
//! `viewer` group opens with setup, the pairing and the carrying — so a road that reads it is a
//! screen road, and a step that named it here is an unmapped op on purpose.

use amenbo_scenario::{Args, Domain};

use crate::{req_bool, unmapped, Driver, Outcome};

impl Driver<'_> {
    pub(crate) fn viewer_action(&mut self, op: &str, with: &Args) -> Result<Outcome, String> {
        match op {
            // The press with nothing pasted in. The token is never an argument — it is read from
            // where the person is, and this driver is nobody, so what arrives on stdin is end of
            // input. That is the refusal a road walks here, and it is the ordinary way to arrive:
            // the form is opened before the token has been made.
            "stand-up" => {
                let mut args: Vec<String> = vec!["viewer".into(), "setup".into(), "--json".into()];
                if let Some(account) = with.get("account").and_then(|v| v.as_str()) {
                    args.push("--account".into());
                    args.push(account.into());
                }
                self.run_json(&args.iter().map(String::as_str).collect::<Vec<_>>())?;
                Ok(Outcome::action(
                    "stood the Viewer's server up — which this gate cannot reach, so a road that \
                     gets here has an account of its own"
                        .to_string(),
                ))
            }
            _ => Err(unmapped(Domain::Viewer, op)),
        }
    }

    pub(crate) fn viewer_assert(&self, op: &str, with: &Args) -> Result<Outcome, String> {
        match op {
            // Whether a server stands. The terminal answers it by refusing: every road that needs
            // one is turned away with `viewer_not_set_up` and told which command makes one, so the
            // refusal is read as the answer rather than as a failure to report.
            "served" => {
                let want = req_bool(with, "yes")?;
                let out = self.invoke(&["viewer", "phones", "--json"])?;
                let stood = out.status.success();
                let said = if stood {
                    String::from_utf8_lossy(&out.stdout).trim().to_string()
                } else {
                    String::from_utf8_lossy(&out.stderr).trim().to_string()
                };
                // A refusal that is not this one is a different fault and must not read as "no
                // server": a store that could not be opened would otherwise pass this line.
                let by_the_right_guard = stood
                    || serde_json::from_str::<serde_json::Value>(&said)
                        .ok()
                        .and_then(|v| v["error"]["code"].as_str().map(str::to_string))
                        .as_deref()
                        == Some("viewer_not_set_up");
                let pass = stood == want && by_the_right_guard;
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "this device {} a Viewer server — {said} (expected {}, {})",
                        if stood { "has" } else { "has no" },
                        if want { "one" } else { "none" },
                        if pass { "as expected" } else { "MISMATCH" }
                    ),
                ))
            }
            // Where the phone's half is got. It asks nothing of the store, so this is the one road
            // here that answers in full before setup has ever run.
            "app-offered" => {
                let v = self.run_json(&["viewer", "app", "--json"])?;
                let rows = v["app"].as_array().map(Vec::as_slice).unwrap_or(&[]);
                let named: Vec<String> = rows
                    .iter()
                    .filter_map(|row| {
                        let phone = row["phone"].as_str()?;
                        let link = row["link"].as_str()?;
                        link.starts_with("https://").then(|| format!("{phone} → {link}"))
                    })
                    .collect();
                // Every row has to carry both halves, or the count alone would pass a row naming a
                // phone nobody can install from.
                let pass = !rows.is_empty() && named.len() == rows.len();
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "the Viewer app is offered for {} kind(s) of phone: {} ({})",
                        rows.len(),
                        named.join("; "),
                        if pass { "as expected" } else { "MISMATCH" }
                    ),
                ))
            }
            // **The key goes to a camera and nowhere else.** A read code carries it, so the two
            // roads out of a terminal that are not a screen are both closed — and both are closed
            // *before* a code is issued, because issuing replaces whatever the server was holding
            // and a run that ended with nothing on the screen would have stopped the phone that was
            // reading for no gain.
            "key-stays-on-screen" => {
                let mut said = Vec::new();
                let mut pass = true;
                for (road, args) in [
                    ("into JSON", vec!["viewer", "qr", "--json"]),
                    ("into a pipe", vec!["viewer", "qr"]),
                ] {
                    let out = self.invoke(&args)?;
                    let refused = !out.status.success();
                    pass = pass && refused;
                    said.push(format!(
                        "a read code {} be written {road}",
                        if refused { "cannot" } else { "CAN" }
                    ));
                }
                Ok(Outcome::assert(
                    pass,
                    format!("{} ({})", said.join("; "), if pass { "as expected" } else { "MISMATCH" }),
                ))
            }
            _ => Err(unmapped(Domain::Viewer, op)),
        }
    }
}
