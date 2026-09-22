//! The `notify` domain: the connections this device can send through, and the selection a project
//! makes from them.
//!
//! **Two halves, kept in two places on purpose.** The connection belongs to the device and the
//! choosing to the project, so a road that means to prove a project's notifications go where its
//! screen says walks both — raise a target, write its connection, switch the project on, select it,
//! and read the two answers back.
//!
//! **Nothing here posts.** Sending would need a channel somebody owns and a relay that would take the
//! message, and a gate that reached either would hold a release on whether a third party answered
//! today. What is walked is everything up to the sending, `usable` included: the check reads the
//! settings, and on a webhook it reads the shape of a URL — neither touches the network.

use amenbo_scenario::{Args, Domain};

use crate::{req_bool, req_str, unmapped, Driver, Outcome};

impl Driver<'_> {
    pub(crate) fn notify_action(
        &mut self,
        op: &str,
        with: &Args,
        bind: Option<&str>,
    ) -> Result<Outcome, String> {
        match op {
            // Raising and connecting are two steps because the store takes them in that order: a
            // credential needs a row to hang off, so there is no shape where a target arrives whole.
            "raise" => {
                let kind = req_str(with, "kind")?;
                let name = req_str(with, "name")?;
                let v = self.run_json(&["notify", "target-add", "--kind", kind, name, "--json"])?;
                let id = v["target"]["id"].as_i64().ok_or("notify target-add did not report an id")?;
                if let Some(binding) = bind {
                    self.bindings.insert(binding.to_string(), id);
                }
                Ok(Outcome::action(format!("raised {kind} notification target {id} `{name}`")))
            }
            "connect" => {
                let target = self.resolve(with)?.to_string();
                let mut args: Vec<String> =
                    vec!["notify".into(), "target-set".into(), target.clone(), "--json".into()];
                for (key, flag) in [
                    ("secret", "--secret"),
                    ("smtp_host", "--smtp-host"),
                    ("smtp_user", "--smtp-user"),
                    ("mail_from", "--mail-from"),
                ] {
                    if let Some(value) = with.get(key).and_then(|v| v.as_str()) {
                        args.push(flag.into());
                        args.push(value.into());
                    }
                }
                if let Some(port) = with.get("smtp_port").and_then(|v| v.as_i64()) {
                    args.push("--smtp-port".into());
                    args.push(port.to_string());
                }
                self.run_json(&args.iter().map(String::as_str).collect::<Vec<_>>())?;
                Ok(Outcome::action(format!("wrote the connection of notification target {target}")))
            }
            "mark-default" => {
                let target = self.resolve(with)?;
                self.run_json(&["notify", "target-default", &target.to_string(), "--json"])?;
                Ok(Outcome::action(format!(
                    "new projects now start out pointing at notification target {target}"
                )))
            }
            // `--yes` because there is nobody at the keyboard: the confirmation is what a person is
            // asked, and a driver that met it would stall rather than walk the road.
            "remove" => {
                let target = self.resolve(with)?;
                self.run_json(&["notify", "target-rm", &target.to_string(), "--yes", "--json"])?;
                Ok(Outcome::action(format!("removed notification target {target}")))
            }
            "report" => {
                let on = req_bool(with, "on")?;
                self.run_json(&["notify", if on { "on" } else { "off" }, "--json"])?;
                Ok(Outcome::action(format!(
                    "this project {} reporting",
                    if on { "is" } else { "is not" }
                )))
            }
            "carry" => {
                let target = self.resolve(with)?;
                let on = req_bool(with, "on")?;
                self.run_json(&[
                    "notify",
                    if on { "use" } else { "unuse" },
                    &target.to_string(),
                    "--json",
                ])?;
                Ok(Outcome::action(format!(
                    "this project {} notification target {target}",
                    if on { "sends through" } else { "no longer sends through" }
                )))
            }
            "address" => {
                let to = req_str(with, "to")?;
                self.run_json(&["notify", "to", to, "--json"])?;
                Ok(Outcome::action(format!("mail from this project is addressed to `{to}`")))
            }
            "choose" => {
                let event = req_str(with, "event")?;
                let on = req_bool(with, "on")?;
                let mut args = vec!["notify", "event", event, "--json"];
                if !on {
                    args.push("--off");
                }
                self.run_json(&args)?;
                Ok(Outcome::action(format!(
                    "this project {} `{event}`",
                    if on { "reports" } else { "no longer reports" }
                )))
            }
            _ => Err(unmapped(Domain::Notify, op)),
        }
    }

    pub(crate) fn notify_assert(&self, op: &str, with: &Args) -> Result<Outcome, String> {
        match op {
            // The shelf row, read for the one thing the connection cannot say for itself. The
            // credential's value is never read back by anybody — what the row carries is whether one
            // is held, which is what decides whether the target can send at all.
            "shelved" => {
                let target = self.resolve(with)?;
                let want = req_bool(with, "credential")?;
                let v = self.run_json(&["notify", "--json"])?;
                let row = v["targets"]
                    .as_array()
                    .map(Vec::as_slice)
                    .unwrap_or(&[])
                    .iter()
                    .find(|t| t["id"].as_i64() == Some(target));
                let held = row.and_then(|t| t["secret_set"].as_bool());
                let pass = held == Some(want);
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "notification target {target} {} (expected a credential {}, {})",
                        match held {
                            Some(true) => "is on the shelf holding a credential".to_string(),
                            Some(false) => "is on the shelf holding none".to_string(),
                            None => "is not on the shelf at all".to_string(),
                        },
                        if want { "held" } else { "unset" },
                        if pass { "as expected" } else { "MISMATCH" }
                    ),
                ))
            }
            // What this project does with the shelf: the switch, and — where the step names one — the
            // target carrying it and the event it reports. Three facts, asked as the step asks them,
            // so a road can say one of them without claiming the other two.
            "reports" => {
                let want = req_bool(with, "on")?;
                let v = self.run_json(&["notify", "--json"])?;
                let mine = &v["project"];
                let on = mine["enabled"].as_bool().unwrap_or(false);
                let mut said = vec![format!(
                    "this project is {} (expected {})",
                    if on { "reporting" } else { "not reporting" },
                    if want { "on" } else { "off" }
                )];
                let mut pass = on == want;
                if with.contains_key("target") {
                    let target = self.resolve(with)?;
                    let carried = mine["targets"]
                        .as_array()
                        .map(Vec::as_slice)
                        .unwrap_or(&[])
                        .iter()
                        .any(|t| t.as_i64() == Some(target));
                    pass = pass && carried;
                    said.push(format!(
                        "it {} notification target {target}",
                        if carried { "sends through" } else { "does not send through" }
                    ));
                }
                if let Some(event) = with.get("event").and_then(|v| v.as_str()) {
                    let chosen = mine["events"]
                        .as_array()
                        .map(Vec::as_slice)
                        .unwrap_or(&[])
                        .iter()
                        .any(|e| e.as_str() == Some(event));
                    pass = pass && chosen;
                    said.push(format!(
                        "it {} `{event}`",
                        if chosen { "reports" } else { "does not report" }
                    ));
                }
                Ok(Outcome::assert(
                    pass,
                    format!("{} ({})", said.join("; "), if pass { "as expected" } else { "MISMATCH" }),
                ))
            }
            // The check, and nothing leaves on it. A relay is connected to and the account offered to
            // it; a webhook has the shape of its URL read and no more, a webhook revoked yesterday
            // still having the shape — so a road saying `yes: true` of a Slack target is saying the
            // smaller thing on purpose.
            "usable" => {
                let target = self.resolve(with)?;
                let want = req_bool(with, "yes")?;
                // The exit status is the verdict, not the shape of what was printed: a refusal writes
                // its sentence to stderr and leaves stdout empty, and reading a missing JSON object
                // as "not usable" would let a command that printed nothing at all pass for one.
                let out = self.invoke(&["notify", "target-check", &target.to_string(), "--json"])?;
                let usable = out.status.success();
                let said = if usable {
                    String::from_utf8_lossy(&out.stdout).trim().to_string()
                } else {
                    String::from_utf8_lossy(&out.stderr).trim().to_string()
                };
                let pass = usable == want;
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "notification target {target} {} — {said} (expected {}, {})",
                        if usable { "is usable as it stands" } else { "is not usable" },
                        if want { "usable" } else { "not usable" },
                        if pass { "as expected" } else { "MISMATCH" }
                    ),
                ))
            }
            _ => Err(unmapped(Domain::Notify, op)),
        }
    }
}
