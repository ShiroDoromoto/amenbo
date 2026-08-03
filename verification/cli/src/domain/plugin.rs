//! The `plugin` domain: what is installed on this machine, whose gate is open, what a call
//! returned, what the execution log kept — and the catalogs all of it comes from, including the
//! ones the run stands up itself to walk the key a registration pins.

use std::path::Path;

use amenbo_scenario::{Args, Domain};
use amenbo_static_host::StaticHost;

use crate::{opt_bool, path_str, req_bool, req_i64, req_str, unmapped, Driver, Outcome};

/// Where a stood catalog publishes its index — the URL a registration is given.
const CATALOG_PATH: &str = "/catalog.json";

/// And where it publishes its signing key. amenbo looks for the key **beside** the `catalog.json` it
/// was given, so this path is not a choice: it follows from the one above.
const CATALOG_KEY_PATH: &str = "/catalog-key.pub";

/// What a stood catalog offers: nothing. Standing one up is about the trust root taken at
/// registration, and an empty shelf keeps every count in the run about the official catalog alone.
const EMPTY_CATALOG: &str = r#"{"catalog_v": 1, "generated_at": "2026-07-27T00:00:00Z", "plugins": []}"#;

/// The two signing keys a stood catalog publishes, before and after a rotation. Both are real
/// minisign public keys from this repository's own tests — which key is which means nothing here,
/// only that they are two, since what a pin has to notice is that the second is not the first.
const FIRST_KEY: &str = "RWSgV8uCt8tyYg74JbwBblWoE+g7bxSGvK8blkKW7gUo3EuBXaqy5oMR\n";
const SECOND_KEY: &str = "RWSw3wZ34b1PMyHu4KajlLhV0SdlMAgQGefo4pFIxv7MgRoWSVpCVXSE\n";

/// A catalog the run is publishing, and which of the two keys it is publishing now. Rotating serves
/// **the other** one, so a scenario never names a key: what it can say is that the key changed, and
/// changing it back is how a step asks which key a registration ended up pinned to.
pub(crate) struct StoodCatalog {
    host: StaticHost,
    key: Option<&'static str>,
}

impl StoodCatalog {
    /// Publish the key this catalog is not publishing now — a catalog that published none starts.
    fn rotate_key(&mut self) {
        let next = if self.key == Some(FIRST_KEY) { SECOND_KEY } else { FIRST_KEY };
        self.host.set(CATALOG_KEY_PATH, next);
        self.key = Some(next);
    }
}

impl Driver<'_> {
    pub(crate) fn plugin_action(&mut self, op: &str, with: &Args, bind: Option<&str>) -> Result<Outcome, String> {
        match op {
            "install" => {
                let name = req_str(with, "name")?;
                let v = self.run_json(&["plugin", "install", name, "--json"])?;
                let bytes = v["program_bytes"].as_i64().unwrap_or(0);
                Ok(Outcome::action(format!("installed plugin `{name}` ({bytes} bytes of program)")))
            }
            // A gate is one project's, so the line says whose it opened. The write answers with that
            // project's id, which is not what a reader recognises — the name is read back for the
            // evidence, and the id stands in if that read cannot be made.
            "enable" => {
                let name = req_str(with, "name")?;
                let v = self.run_json(&["plugin", "enable", name, "--json"])?;
                let where_ = v["project"].as_i64().map(|id| {
                    self.run_json(&["project", "show", &id.to_string(), "--json"])
                        .ok()
                        .and_then(|p| p["name"].as_str().map(str::to_string))
                        .unwrap_or_else(|| format!("project {id}"))
                });
                Ok(Outcome::action(match where_ {
                    Some(where_) => format!("opened `{name}`'s gate in {where_}"),
                    None => format!("opened `{name}`'s gate"),
                }))
            }
            "disable" => {
                let name = req_str(with, "name")?;
                self.run_json(&["plugin", "disable", name, "--json"])?;
                Ok(Outcome::action(format!("closed `{name}`'s gate")))
            }
            "uninstall" => {
                let name = req_str(with, "name")?;
                // Removing a plugin takes its settings, its consent and its log rows with it, so it
                // asks first; the driver is unattended and answers up front.
                self.run_json(&["plugin", "uninstall", name, "--yes", "--json"])?;
                Ok(Outcome::action(format!("removed plugin `{name}`")))
            }
            "run" => {
                let name = req_str(with, "name")?.to_string();
                let command = req_str(with, "command")?.to_string();
                // Everything after `plugin run <name>` belongs to the plugin, so amenbo's own flags
                // have to be said before the subcommand — appended, they would reach the plugin as
                // arguments and amenbo would see no facet at all.
                let mut args: Vec<String> =
                    vec!["--actor".into(), "human".into(), "--json".into(), "plugin".into(), "run".into()];
                args.push(name.clone());
                args.push(command.clone());
                if with.contains_key("task") {
                    args.push(self.resolve_key(with, "task")?.to_string());
                }
                for extra in with.get("args").and_then(|v| v.as_sequence()).unwrap_or(&Vec::new()) {
                    let extra = extra.as_str().ok_or("every entry under `args` must be a string")?;
                    args.push(extra.to_string());
                }
                let v = self.run_json(&args.iter().map(String::as_str).collect::<Vec<_>>())?;
                let value = v["value"].as_str().unwrap_or_default().len();
                self.last_run = Some(v);
                Ok(Outcome::action(format!(
                    "called `{name} {command}` — it returned {value} byte(s)"
                )))
            }
            // Work the queues in this process rather than leave them to a runner nobody watches, which
            // is what makes what moved reportable at all. What it says is read by the assert that
            // follows it; the state it leaves behind is read by `waiting`, as ever.
            "flush" => {
                let v = self.run_json(&["plugin", "flush", "--json"])?;
                let delivered = v["delivered"].as_i64().unwrap_or(0);
                let held = v["queues"].as_array().map_or(0, Vec::len);
                self.last_flush = Some(v);
                Ok(Outcome::action(format!(
                    "flushed {delivered} event(s), leaving {held} queue(s) to the runners on them"
                )))
            }
            "update" => {
                let name = req_str(with, "name")?;
                let v = self.run_json(&["plugin", "update", name, "--json"])?;
                // `applied: false` is the honest answer for a plugin already on the catalog's build,
                // and it is not a failure — so it is reported rather than judged here. What the line
                // is about is judged by the asserts around it.
                let applied = v["applied"].as_bool().unwrap_or(false);
                Ok(Outcome::action(match applied {
                    true => format!("updated plugin `{name}`, keeping the build it replaced"),
                    false => format!("plugin `{name}` was already the build the catalog publishes"),
                }))
            }
            "rollback" => {
                let name = req_str(with, "name")?;
                self.run_json(&["plugin", "rollback", name, "--json"])?;
                Ok(Outcome::action(format!("put `{name}`'s retained build back")))
            }
            // Leaving an installed plugin recording a build the catalog has moved past — the state an
            // update exists for, which no sequence of amenbo commands can arrive at (see the registry).
            // Every distributable's digest goes, not this platform's alone: which one the machine
            // running the scenario resolves to is not this driver's to work out, and a manifest whose
            // digests all disagree with the catalog is outdated on any of them.
            "stale-manifest" => {
                let name = req_str(with, "name")?;
                let path = self.session.home.join("plugins").join(name).join("manifest.json");
                let raw = std::fs::read_to_string(&path)
                    .map_err(|e| format!("could not read {}: {e}", path.display()))?;
                let mut manifest: serde_json::Value = serde_json::from_str(&raw)
                    .map_err(|e| format!("{} is not the manifest it should be: {e}", path.display()))?;
                // What an update is compared by, and the whole of it: the digest of the detail document
                // this install was made from. Age that one value and the machine is one the catalog has
                // moved past — which is what the scenario after this walks. The asset digests are
                // deliberately left alone: they are checked at the door over the bytes that arrive, and
                // ageing them here would age nothing detection looks at.
                //
                // Written rather than only replaced: a record carrying none — a plugin placed by hand — is
                // still one a moved list has to be able to move past.
                manifest["detail_sum"] = serde_json::Value::String(format!("sha256:{}", "0".repeat(64)));
                std::fs::write(&path, serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?)
                    .map_err(|e| format!("could not write {}: {e}", path.display()))?;
                Ok(Outcome::action(format!(
                    "left `{name}` recording a detail document the catalog no longer names"
                )))
            }
            // Adding a plain line to what an installed plugin says it takes. amenbo reads the schema
            // off the installed manifest and never invents a field, so this is the author's declaration
            // arriving the only way it can while no published plugin carries one (see the registry).
            // What the scenario then walks — where the value is kept, what a read gives back, what
            // clearing it does — is amenbo's own, untouched.
            "declare-setting" => {
                let name = req_str(with, "name")?;
                let key = req_str(with, "key")?;
                self.declare_field(name, key, with, serde_json::json!({}))?;
                // Whether the plugin can work without an answer is the difference the scenario after this
                // turns on, so the evidence line says which of the two was written.
                Ok(Outcome::action(match opt_bool(with, "required") {
                    Some(true) => {
                        format!("`{name}` now declares `{key}`, and cannot be enabled while it is empty")
                    }
                    _ => format!("`{name}` now declares `{key}`"),
                }))
            }
            // The same door for a setting the author marked secret. That flag is the whole of what
            // sends a value down the other road — off every export, injected as an environment
            // variable — and no published plugin sets it, so the road exists only once this is written.
            "declare-secret" => {
                let name = req_str(with, "name")?;
                let key = req_str(with, "key")?;
                self.declare_field(name, key, with, serde_json::json!({ "secret": true }))?;
                Ok(Outcome::action(format!("`{name}` now declares `{key}` as a secret setting")))
            }
            // And for a setting whose answers the author listed, with the one that stands until someone
            // answers. Same reason again: candidates are the author's to declare and no published plugin
            // declares any, so the half of `plugin config` that keeps three answers apart — a choice
            // made, none of them chosen, nobody asked yet — would go unwalked. The label a candidate
            // carries is display text the scenario never reads back, so each one is labelled with its
            // own value rather than given wording to keep in step.
            "declare-choice" => {
                let name = req_str(with, "name")?;
                let key = req_str(with, "key")?;
                let options = req_str(with, "options")?;
                let candidates: Vec<serde_json::Value> = options
                    .split(',')
                    .map(|value| serde_json::json!({ "value": value, "label": value }))
                    .collect();
                let mut over = serde_json::json!({ "type": "multi", "options": candidates });
                // A field that declares no default is the other shape a choice comes in — one where
                // nothing stands in for an answer nobody gave — so the key is left off rather than
                // written empty, which the manifest would read as a default of the empty string.
                if let Some(default) = with.get("default").and_then(|v| v.as_str()) {
                    over["default"] = serde_json::json!(default);
                }
                self.declare_field(name, key, with, over)?;
                Ok(Outcome::action(format!(
                    "`{name}` now offers `{options}` as the answers to `{key}`"
                )))
            }
            // Writing what a plugin says for itself onto the manifest beside its binary — the author's
            // `agent` block, arriving the only way it can while the scenario is not to depend on the
            // catalog's own wording (see the registry). The block is one thing rather than a list, so a
            // plugin that already carries one has it replaced: what the asserts after this read back is
            // the sentence written here, whatever the catalog says today.
            "declare-agent" => {
                let name = req_str(with, "name")?;
                let when = req_str(with, "when")?;
                let path = self.session.home.join("plugins").join(name).join("manifest.json");
                let raw = std::fs::read_to_string(&path)
                    .map_err(|e| format!("could not read {}: {e}", path.display()))?;
                let mut manifest: serde_json::Value = serde_json::from_str(&raw)
                    .map_err(|e| format!("{} is not the manifest it should be: {e}", path.display()))?;
                let mut agent = serde_json::json!({ "when": when });
                // A plugin whose whole surface is observation hooks names its occasion and stops there,
                // so a step that names no call writes no `commands` — the same shape its author would.
                if let Some(cmd) = with.get("cmd").and_then(|v| v.as_str()) {
                    let does = with.get("does").and_then(|v| v.as_str()).unwrap_or(cmd);
                    agent["commands"] = serde_json::json!([{ "cmd": cmd, "does": does }]);
                }
                manifest["agent"] = agent;
                std::fs::write(&path, serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?)
                    .map_err(|e| format!("could not write {}: {e}", path.display()))?;
                Ok(Outcome::action(format!("`{name}` now says for itself: {when}")))
            }
            // Standing in a program that says what it was handed, so the injection has a witness. A
            // config value reaches a run and nowhere else: the store holds the answer rather than what
            // it is worth, the log is kept clear of it, and the read that says a secret is set says
            // nothing more. A plugin is the only thing on the receiving end, and the published ones use
            // their settings rather than report them (see the registry) — so this one prints its
            // injected config and stops there. Both roads, because a setting travels by the one its
            // author's `secret` flag chose: an environment variable for a secret, the stdin document
            // for the rest. The `AMENBO_CONFIG_` prefix is amenbo's own; the callback variables beside
            // it are left out, because what is under test is the value a plugin was told, not the door
            // it can read back through.
            "echo-program" => {
                let name = req_str(with, "name")?;
                let path = self.session.home.join("plugins").join(name).join(name);
                if !path.exists() {
                    return Err(format!("`{name}` has no program at {} to stand in for", path.display()));
                }
                // `grep` finding nothing is a non-zero exit, and a command run that exits non-zero is a
                // failure amenbo reports rather than a return value — so the script ends by saying it
                // is fine. Handed nothing, it returns nothing, which is exactly the reading a scenario
                // asking whether a secret is gone needs. The stdin document is echoed verbatim: what a
                // non-secret setting is worth to a run is a key in it, and the whole point of a value
                // resolved on the way out is that it cannot be read anywhere else.
                std::fs::write(&path, "#!/bin/sh\nenv | grep '^AMENBO_CONFIG_'\ncat\nexit 0\n")
                    .map_err(|e| format!("could not write {}: {e}", path.display()))?;
                make_runnable(&path)?;
                Ok(Outcome::action(format!(
                    "left `{name}` answering with the config it is handed, and nothing else"
                )))
            }
            // Standing in a program that calls amenbo back, so the read-back path has a witness. What a
            // plugin is handed when it is launched is the store to open and the window to read through,
            // and both travel in its environment — the script below names neither, which is the whole of
            // what is under test: a read goes through with no facet written anywhere, a read past the
            // window does not, and a write still says who is writing. The published plugins take none of
            // this route (see the registry), so there is nothing to ask but a stand-in.
            //
            // Two faces, `read` and `write`, and everything after the id goes to amenbo as it was
            // written: the call each step makes is then readable in the scenario. The binary is named in
            // full because the one under test is not the `amenbo` on `PATH` — an author writes `amenbo`,
            // and this is that line pointed at the build being verified.
            "read-back-program" => {
                let name = req_str(with, "name")?;
                let path = self.session.home.join("plugins").join(name).join(name);
                if !path.exists() {
                    return Err(format!("`{name}` has no program at {} to stand in for", path.display()));
                }
                let bin = self.bin.display().to_string();
                // The path goes into the script single-quoted, which is the one thing a path could break.
                if bin.contains('\'') {
                    return Err(format!("`{bin}` cannot be called from a script: its path carries a quote"));
                }
                // A refusal is what half of this goes to find, and a plugin exiting non-zero is a failed
                // call rather than a value — so both streams are gathered into the return value and the
                // program ends cleanly whatever amenbo said.
                std::fs::write(
                    &path,
                    format!(
                        "#!/bin/sh\n\
                         if [ $# -lt 2 ]; then echo 'this face takes the id of a task'; exit 0; fi\n\
                         face=\"$1\"; id=\"$2\"; shift 2\n\
                         case \"$face\" in\n\
                         read) set -- task show \"$id\" --json \"$@\" ;;\n\
                         write) set -- comment add \"$id\" --json \"$@\" ;;\n\
                         *) echo \"no face called $face\"; exit 0 ;;\n\
                         esac\n\
                         '{bin}' \"$@\" 2>&1\n\
                         exit 0\n"
                    ),
                )
                .map_err(|e| format!("could not write {}: {e}", path.display()))?;
                make_runnable(&path)?;
                Ok(Outcome::action(format!(
                    "left `{name}` calling amenbo back with the store and window it is handed"
                )))
            }
            // Leaving an installed plugin answering slowly, so its queue has something in it to read.
            // A row comes off a queue the moment the plugin replies, so a backlog is a window and not
            // a state amenbo can be asked for (see the registry): what is queued while a plugin is
            // still on it is the only backlog there is. The program is replaced rather than the
            // manifest edited — how long a plugin takes is the program's own doing, and nothing about
            // the install is being lied about. Everything after it is amenbo's: the queue, the lease
            // and the runner are its own, and the events are ones the plugin really subscribes to.
            "slow-program" => {
                let name = req_str(with, "name")?;
                let seconds = req_i64(with, "seconds")?;
                if seconds <= 0 {
                    return Err("`seconds` has to be a window an assert can read in".to_string());
                }
                // `<home>/plugins/<name>/<name>` — the executable amenbo runs, under the plugin's own
                // name. A shell script stands in for it: every plugin the catalog publishes ships a
                // binary, and no binary can be written here that sleeps.
                let path = self.session.home.join("plugins").join(name).join(name);
                if !path.exists() {
                    return Err(format!("`{name}` has no program at {} to slow down", path.display()));
                }
                // The payload arrives on stdin and is small enough to sit in the pipe, so a program
                // that never reads it still gets to sleep and answer cleanly — which is what this is
                // standing in for: a plugin that is slow, not one that is broken.
                std::fs::write(&path, format!("#!/bin/sh\nsleep {seconds}\nexit 0\n"))
                    .map_err(|e| format!("could not write {}: {e}", path.display()))?;
                make_runnable(&path)?;
                Ok(Outcome::action(format!(
                    "left `{name}` taking {seconds}s to answer, so what is queued for it stays queued"
                )))
            }
            // Closing amenbo's view of what is installed, and opening it again — the only way to leave a
            // write's delivery standing (see the registry). Delivery rides along with the write that
            // caused it, so every event a scenario writes is already carried out by the time the next
            // step runs; with the directory shut, the drive behind the write resolves nobody, hands
            // nothing on, and leaves the event where it was appended. The permissions are the whole of
            // it — nothing inside is moved or rewritten, so what the flush afterwards resolves is the
            // same installed plugin that was there before.
            "installed-dir" => {
                let readable = req_bool(with, "readable")?;
                let path = self.session.home.join("plugins");
                if !path.exists() {
                    return Err(format!(
                        "there is no {} to shut: nothing is installed on this machine",
                        path.display()
                    ));
                }
                set_readable(&path, readable)?;
                Ok(Outcome::action(match readable {
                    true => "gave back what is installed, so delivery resolves it again".to_string(),
                    false => "shut what is installed away, so the next write is delivered to nobody".to_string(),
                }))
            }
            // Filling in a setting the plugin's author declared. An empty value is the way one is
            // taken back, so it is passed through as written rather than being turned into an op of
            // its own — the command reads it the same way a person typing `""` does.
            "config-set" => {
                let name = req_str(with, "name")?;
                let key = req_str(with, "key")?;
                let value = req_str(with, "value")?;
                let v = self.run_json(&[
                    "plugin", "config", "set", name, key, value, "--json",
                ])?;
                Ok(Outcome::action(match v["cleared"].as_bool() {
                    Some(true) => format!("took `{key}` back off `{name}` for this project"),
                    _ => format!("told `{name}` its `{key}` for this project"),
                }))
            }
            // A catalog of the run's own on the loopback, so a scenario can walk what only a catalog
            // that answers can show: the key it publishes beside its `catalog.json`, and the pin
            // taken on it. What it offers is deliberately nothing — this is about the trust root
            // amenbo takes at registration, not about what is on the shelf.
            "catalog-stand" => {
                let publishes_key = req_bool(with, "publishes_key")?;
                let name = bind.ok_or("`catalog-stand` produces a catalog, so it needs an `as:` name")?;
                let host = StaticHost::serve([(CATALOG_PATH, EMPTY_CATALOG)]);
                let url = host.url(CATALOG_PATH);
                let mut stood = StoodCatalog { host, key: None };
                if publishes_key {
                    stood.rotate_key();
                }
                self.catalogs.insert(name.to_string(), stood);
                Ok(Outcome::action(format!(
                    "stood a catalog at {url} ({})",
                    if publishes_key { "publishing a signing key" } else { "publishing no key" }
                )))
            }
            // The publisher rotates their key, at the same URL. Nothing about the catalog moves —
            // that is the point: what amenbo has to notice is the key alone.
            "catalog-rotate-key" => {
                let name = req_str(with, "target")?;
                let stood = self
                    .catalogs
                    .get_mut(name)
                    .ok_or_else(|| format!("internal: no catalog was stood up as `{name}`"))?;
                stood.rotate_key();
                Ok(Outcome::action(format!("`{name}` now publishes a different signing key")))
            }
            verb @ ("catalog-add" | "catalog-remove") => {
                let url = self.catalog_url(with)?;
                let sub = verb.trim_start_matches("catalog-");
                // `--yes` is the consent a registration takes when the catalog publishes a key: this
                // is a non-interactive run, and amenbo refuses to pin a trust root without being told
                // so. A catalog with no key to pin never asks, so passing it costs that case nothing.
                let v = self.run_json(&["plugin", "catalog", sub, &url, "--yes", "--json"])?;
                // Adding fetches the catalog once, so the count it comes back with is what a first
                // browse would have found — and the reachability, which is the half a bad URL shows.
                Ok(Outcome::action(match sub {
                    "add" => format!(
                        "registered {url} ({} plugin(s), {}, {})",
                        v["offered"].as_i64().unwrap_or(0),
                        if v["reachable"].as_bool().unwrap_or(false) { "reached" } else { "unreachable" },
                        match v["fingerprint"].as_str() {
                            Some(fp) => format!("key {fp} pinned"),
                            None => "no key to pin".to_string(),
                        }
                    ),
                    _ => format!("dropped {url} from the browsing view"),
                }))
            }
            _ => Err(unmapped(Domain::Plugin, op)),
        }
    }
    pub(crate) fn plugin_assert(&self, op: &str, with: &Args) -> Result<Outcome, String> {
        match op {
            "listed" => {
                let name = req_str(with, "name")?;
                let v = self.run_json(&["plugin", "list", "--json"])?;
                let row = v["plugins"]
                    .as_array()
                    .and_then(|rows| rows.iter().find(|p| p["name"].as_str() == Some(name)));
                // With `enabled` the question is the gate, and `install ≠ enable` is exactly what a
                // reader gets wrong — so the two are asked apart, never rolled into one answer. The row
                // names the projects holding the gate open, so "open" is that list having anything in
                // it, and the projects themselves are read back into the line.
                match opt_bool(with, "enabled") {
                    Some(want) => {
                        let on: Option<Vec<&str>> = row.and_then(|r| {
                            r["enabled_projects"]
                                .as_array()
                                .map(|ps| ps.iter().filter_map(|p| p["name"].as_str()).collect())
                        });
                        let pass = on.as_ref().map(|ps| !ps.is_empty()) == Some(want);
                        Ok(Outcome::assert(
                            pass,
                            format!(
                                "plugin `{name}` fires in {} (expected {}, {})",
                                match on.as_deref() {
                                    Some([]) => "no project".to_string(),
                                    Some(projects) => projects.join(", "),
                                    None => "— it is not installed at all".to_string(),
                                },
                                if want { "at least one project" } else { "none" },
                                if pass { "as expected" } else { "MISMATCH" }
                            ),
                        ))
                    }
                    None => {
                        let present = opt_bool(with, "present").unwrap_or(true);
                        let pass = row.is_some() == present;
                        Ok(Outcome::assert(
                            pass,
                            format!(
                                "plugin `{name}` {} on this machine (expected {}, {})",
                                if row.is_some() { "is installed" } else { "is not installed" },
                                if present { "installed" } else { "gone" },
                                if pass { "as expected" } else { "MISMATCH" }
                            ),
                        ))
                    }
                }
            }
            // The entry point as an AI meets it: `plugins` names what this folder can actually call, in
            // the words their authors wrote. Four readings live here because they fail apart — a plugin
            // offered when its gate is shut, a line paraphrased instead of relayed, a command handed
            // over in a form nobody can type, and an empty key that does not say which empty it is are
            // four different breakages of the same key.
            "at-entry" => {
                let name = req_str(with, "name")?;
                let present = req_bool(with, "present")?;
                // Asked as the AI, which is who the document is for: the `plugins` key is what *this
                // folder* can call. Which folder that is comes from the binding, so either facet reads
                // the same gates — and when the key comes back empty it says why, rather than leaving
                // a reader to guess which of several empty-handed states they are in.
                let v = self.run_json(&["agent", "--json", "--actor", "ai"])?;
                let rows = v["plugins"].as_array().map(Vec::as_slice).unwrap_or_default();
                let why = v["pluginsEmptyBecause"].as_str();
                // The floor every reading here stands on: the reason is the answer for an empty list,
                // so it is there exactly when there is nothing to name. A sentence beside a list is one
                // a reader has no use for, and an empty list with nothing said leaves them guessing
                // which of several empty-handed states they are in — both are the key failing at its
                // job, whichever question the step went on to ask.
                if why.is_some() != rows.is_empty() {
                    return Ok(Outcome::assert(
                        false,
                        format!(
                            "the entry point offers {} plugin(s) and {} why it is empty (MISMATCH — a reason stands exactly where there is nothing to list)",
                            rows.len(),
                            if why.is_some() { "still says" } else { "does not say" }
                        ),
                    ));
                }
                // Which empty-handed state this is. Asked before the row, since with nothing offered
                // there is no row to ask about — and the reason is the whole of what a reader gets.
                if let Some(want) = with.get("because").and_then(|v| v.as_str()) {
                    let said = why.unwrap_or_default();
                    let pass = said.contains(want);
                    return Ok(Outcome::assert(
                        pass,
                        format!(
                            "the entry point says it is empty because {said:?} (expected it to carry `{want}`, {})",
                            if pass { "as expected" } else { "MISMATCH" }
                        ),
                    ));
                }
                let entry = rows.iter().find(|p| p["name"].as_str() == Some(name));
                let Some(entry) = entry else {
                    return Ok(Outcome::assert(
                        !present,
                        format!(
                            "the entry point does not offer `{name}` (expected {}, {})",
                            if present { "offered" } else { "nothing" },
                            if present { "MISMATCH" } else { "as expected" }
                        ),
                    ));
                };
                if !present {
                    return Ok(Outcome::assert(
                        false,
                        format!("the entry point offers `{name}` (expected nothing, MISMATCH)"),
                    ));
                }
                // The author's own line, verbatim. A `when` the step does not name is not asked about:
                // a plugin whose author wrote no block is still offered, and that reading is the step
                // that names nothing here.
                if let Some(want) = with.get("when").and_then(|v| v.as_str()) {
                    let said = entry["when"].as_str().unwrap_or_default();
                    if said != want {
                        return Ok(Outcome::assert(
                            false,
                            format!("`{name}` says `{said}` at the entry point (expected `{want}`, MISMATCH)"),
                        ));
                    }
                }
                // What amenbo adds: the author wrote `<cmd>`, and what an AI receives has to be a line
                // it can type — `<the command word> plugin run <name> <cmd>`. The word in front is this
                // build's own name, so it is read back rather than dictated.
                if let Some(face) = with.get("cmd").and_then(|v| v.as_str()) {
                    let tail = format!(" plugin run {name} {face}");
                    let lines: Vec<&str> = entry["commands"]
                        .as_array()
                        .map(|cs| cs.iter().filter_map(|c| c["cmd"].as_str()).collect())
                        .unwrap_or_default();
                    let typed = lines
                        .iter()
                        .find(|line| line.ends_with(&tail) && line.len() > tail.len());
                    return Ok(Outcome::assert(
                        typed.is_some(),
                        match typed {
                            Some(line) => format!("`{name}` hands over `{line}` — a line to type, as expected"),
                            None => format!(
                                "`{name}` offers {} — none of them a call to `{face}` under this build's own name (MISMATCH)",
                                match lines.as_slice() {
                                    [] => "no command at all".to_string(),
                                    ls => ls.join(", "),
                                }
                            ),
                        },
                    ));
                }
                Ok(Outcome::assert(
                    true,
                    format!(
                        "the entry point offers `{name}` — {}, as expected",
                        entry["desc"].as_str().unwrap_or("no description")
                    ),
                ))
            }
            "outdated" => {
                let name = req_str(with, "name")?;
                let present = req_bool(with, "present")?;
                let v = self.run_json(&["plugin", "update", "--check", "--json"])?;
                let offered = v["updates"]
                    .as_array()
                    .is_some_and(|rows| rows.iter().any(|u| u["name"].as_str() == Some(name)));
                let pass = offered == present;
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "for `{name}` the catalog {} (expected {}, {})",
                        if offered {
                            "holds a different build"
                        } else {
                            "holds nothing this machine is not already on"
                        },
                        if present { "one offered" } else { "none" },
                        if pass { "as expected" } else { "MISMATCH" }
                    ),
                ))
            }
            "flushed" => {
                let want = req_i64(with, "delivered")?;
                let last = self
                    .last_flush
                    .as_ref()
                    .ok_or("no `plugin flush` has been run yet, so there is no report to read")?;
                let delivered = last["delivered"].as_i64().unwrap_or(0);
                let named = with.get("held").and_then(|v| v.as_str());
                // A queue named here has to be one the flush reported leaving alone. Reading it back
                // off the store instead would pass just as well for a flush that never ran.
                let stepped_around = named.is_none_or(|plugin| {
                    last["queues"]
                        .as_array()
                        .is_some_and(|queues| queues.iter().any(|q| q["plugin"].as_str() == Some(plugin)))
                });
                let pass = delivered == want && stepped_around;
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "the flush got {delivered} event(s) through{} (expected {want}{}, {})",
                        match named {
                            Some(plugin) if stepped_around => format!(" and left `{plugin}`'s queue to its runner"),
                            Some(plugin) => format!(" and said nothing about `{plugin}`'s queue"),
                            None => String::new(),
                        },
                        named.map(|plugin| format!(" and `{plugin}` left alone")).unwrap_or_default(),
                        if pass { "as expected" } else { "MISMATCH" }
                    ),
                ))
            }
            "returned" => {
                let want = req_str(with, "contains")?;
                let last = self
                    .last_run
                    .as_ref()
                    .ok_or("no `plugin run` has been called yet, so there is no return value to read")?;
                let value = last["value"].as_str().unwrap_or_default();
                let pass = value.contains(want);
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "the call returned {:?} (expected it to carry `{want}`, {})",
                        value,
                        if pass { "as expected" } else { "MISMATCH" }
                    ),
                ))
            }
            // A setting read back as this project holds it — one value per setting, so the read asks
            // the same question the write answered.
            "config" => {
                let name = req_str(with, "name")?;
                let key = req_str(with, "key")?;
                let v = self.run_json(&["plugin", "config", "get", name, key, "--json"])?;
                // Which of the three answers the field holds. It is asked apart from the value because
                // the value cannot tell two of them apart: a choice answered with none of the
                // candidates and one nobody has answered yet both read as nothing chosen, and only the
                // second is the author's default speaking.
                if let Some(want) = with.get("state").and_then(|v| v.as_str()) {
                    let got = v["state"].as_str().unwrap_or_default();
                    let pass = got == want;
                    return Ok(Outcome::assert(
                        pass,
                        format!(
                            "plugin `{name}` holds `{key}` as `{got}` (expected `{want}`, {})",
                            if pass { "as expected" } else { "MISMATCH" }
                        ),
                    ));
                }
                // Whether the read treats the setting as a secret — and, when it should, whether it
                // kept the value to itself. Both halves are the same promise: a `get` that printed a
                // token would put it in the terminal, the scrollback and the shell's history, so a
                // read that says `secret` and hands the value over anyway is the failure this asks
                // about.
                if let Some(want) = opt_bool(with, "secret") {
                    let declared = v["secret"].as_bool().unwrap_or(false);
                    let leaked = v.get("value").is_some_and(|v| !v.is_null());
                    let pass = declared == want && !(want && leaked);
                    return Ok(Outcome::assert(
                        pass,
                        format!(
                            "plugin `{name}` reads `{key}` back as {}{} (expected {}, {})",
                            if declared { "a secret" } else { "an ordinary setting" },
                            if leaked { ", value and all" } else { "" },
                            if want { "a secret nobody echoes" } else { "an ordinary setting" },
                            if pass { "as expected" } else { "MISMATCH" }
                        ),
                    ));
                }
                match with.get("equals") {
                    Some(want) => {
                        let want = serde_json::to_value(want)
                            .map_err(|e| format!("arg `equals` is not a valid value: {e}"))?;
                        let pass = v["value"] == want;
                        Ok(Outcome::assert(
                            pass,
                            format!(
                                "plugin `{name}` reads `{key}` back as {} (expected {want}, {})",
                                v["value"],
                                if pass { "as expected" } else { "MISMATCH" }
                            ),
                        ))
                    }
                    // With no value named, the question is whether the project holds one at all — which
                    // is how a setting taken back is told apart from one that was never given.
                    None => {
                        let want = opt_bool(with, "set").unwrap_or(true);
                        let got = v["set"].as_bool().unwrap_or(false);
                        Ok(Outcome::assert(
                            got == want,
                            format!(
                                "plugin `{name}` {} `{key}` (expected {}, {})",
                                if got { "holds a" } else { "holds no" },
                                if want { "set" } else { "unset" },
                                if got == want { "as expected" } else { "MISMATCH" }
                            ),
                        ))
                    }
                }
            }
            // A catalog as the browsing view sees it: whether it is a source at all, whether the
            // browse could reach it, and whether a key of its is pinned — the last being what
            // separates a catalog that can be installed from one that can only be looked at.
            "catalog" => {
                let url = self.catalog_url(with)?;
                let v = self.run_json(&["plugin", "catalog", "list", "--json"])?;
                let row = v["sources"]
                    .as_array()
                    .and_then(|rows| rows.iter().find(|s| s["url"].as_str() == Some(url.as_str())));
                if let Some(want) = opt_bool(with, "pinned_key") {
                    // A row that is not there at all answers neither way, so it is reported as the
                    // third state rather than folded into "no key".
                    let got = row.map(|r| !r["fingerprint"].is_null());
                    let pass = got == Some(want);
                    return Ok(Outcome::assert(
                        pass,
                        format!(
                            "{url} {} (expected {}, {})",
                            match got {
                                Some(true) => "has a key pinned on it".to_string(),
                                Some(false) => "has no key pinned on it".to_string(),
                                None => "is not a source at all".to_string(),
                            },
                            if want { "a pinned key" } else { "no pinned key" },
                            if pass { "as expected" } else { "MISMATCH" }
                        ),
                    ));
                }
                match opt_bool(with, "reachable") {
                    Some(want) => {
                        let got = row.and_then(|r| r["reachable"].as_bool());
                        let pass = got == Some(want);
                        Ok(Outcome::assert(
                            pass,
                            format!(
                                "{url} is {} (expected {}, {})",
                                match got {
                                    Some(true) => "reachable".to_string(),
                                    Some(false) => "unreachable".to_string(),
                                    None => "not a source at all".to_string(),
                                },
                                if want { "reachable" } else { "unreachable" },
                                if pass { "as expected" } else { "MISMATCH" }
                            ),
                        ))
                    }
                    None => {
                        let present = opt_bool(with, "present").unwrap_or(true);
                        let pass = row.is_some() == present;
                        Ok(Outcome::assert(
                            pass,
                            format!(
                                "{url} {} the browsing view (expected {}, {})",
                                if row.is_some() { "is a source in" } else { "is absent from" },
                                if present { "registered" } else { "gone" },
                                if pass { "as expected" } else { "MISMATCH" }
                            ),
                        ))
                    }
                }
            }
            // The author's door. Its exit code is the verdict — an invalid manifest is a value to
            // judge and not a driver failure — so it is read the way the integrity checks are.
            "validated" => {
                let path = req_str(with, "path")?;
                let want = req_bool(with, "ok")?;
                let full = self.in_session(path)?;
                let v = self.run_check(&["plugin", "validate", path_str(&full)?, "--json"])?;
                let ok = v["ok"].as_bool().unwrap_or(false);
                let codes: Vec<&str> = v["problems"]
                    .as_array()
                    .map(|rows| rows.iter().filter_map(|p| p["code"].as_str()).collect())
                    .unwrap_or_default();
                // Naming a code asks what it was turned down *for*: a manifest can be wrong in more
                // ways than one, and a line that passes on the wrong reason proves nothing.
                let named = with.get("problem").and_then(|v| v.as_str());
                let pass = ok == want && named.is_none_or(|c| codes.contains(&c));
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "`{path}` reads as {} ({}) (expected {}{}, {})",
                        if ok { "valid" } else { "invalid" },
                        if codes.is_empty() {
                            v["parse_error"].as_str().unwrap_or("no problems").to_string()
                        } else {
                            codes.join(", ")
                        },
                        if want { "valid" } else { "invalid" },
                        named.map(|c| format!(" for `{c}`")).unwrap_or_default(),
                        if pass { "as expected" } else { "MISMATCH" }
                    ),
                ))
            }
            "ran" => {
                let name = req_str(with, "name")?;
                let present = opt_bool(with, "present").unwrap_or(true);
                let v = self.run_json(&["plugin", "log", name, "--json"])?;
                // Newest first, so the run a step is asking about is the one at the head.
                let newest = v["runs"].as_array().and_then(|rows| rows.first());
                match with.get("outcome").and_then(|v| v.as_str()) {
                    Some(want) => {
                        let got = newest.and_then(|r| r["outcome"].as_str());
                        let pass = got == Some(want);
                        Ok(Outcome::assert(
                            pass,
                            format!(
                                "`{name}`'s last run ended {} (expected {want}, {})",
                                got.unwrap_or("— it has no runs at all"),
                                if pass { "as expected" } else { "MISMATCH" }
                            ),
                        ))
                    }
                    None => {
                        let pass = newest.is_some() == present;
                        Ok(Outcome::assert(
                            pass,
                            format!(
                                "the log holds {} run(s) for `{name}` (expected {}, {})",
                                v["count"].as_i64().unwrap_or(0),
                                if present { "at least one" } else { "none" },
                                if pass { "as expected" } else { "MISMATCH" }
                            ),
                        ))
                    }
                }
            }
            // The other half of the same command's answer: the log says what ran, this says what has
            // not run yet. A plugin with an empty queue is on no backlog at all, so an absent row is
            // read as nothing waiting rather than as an error — "none queued" is a real answer and the
            // ordinary one. The oldest row's instant rides in the message and is judged by nothing:
            // it is a clock reading, and what a line can hold up is the count and the lease.
            "waiting" => {
                let name = req_str(with, "name")?;
                let v = self.run_json(&["plugin", "log", name, "--json"])?;
                let queued = v["queues"]
                    .as_array()
                    .and_then(|rows| rows.iter().find(|q| q["plugin"].as_str() == Some(name)));
                let count = queued.and_then(|q| q["waiting"].as_i64()).unwrap_or(0);
                let running = queued.and_then(|q| q["running"].as_bool()).unwrap_or(false);
                let oldest = queued
                    .and_then(|q| q["oldest"].as_str())
                    .map(|at| format!(", oldest {at}"))
                    .unwrap_or_default();
                let mut pass = true;
                let mut wanted = Vec::new();
                if let Some(want) = with.get("count").and_then(|v| v.as_i64()) {
                    pass &= count == want;
                    wanted.push(format!("{want} queued"));
                }
                if let Some(want) = opt_bool(with, "running") {
                    pass &= running == want;
                    wanted.push(match want {
                        true => "a runner on it".to_string(),
                        false => "nothing running it".to_string(),
                    });
                }
                if let Some(want) = opt_bool(with, "present") {
                    pass &= queued.is_some() == want;
                    wanted.push(match want {
                        true => "a queue at all".to_string(),
                        false => "no queue at all".to_string(),
                    });
                }
                if wanted.is_empty() {
                    return Err("a `waiting` assert has to ask for `count`, `running` or `present`".to_string());
                }
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "`{name}` has {count} event(s) waiting{oldest}, {} (expected {}, {})",
                        if running { "with a runner on them" } else { "with nothing running them" },
                        wanted.join(" and "),
                        if pass { "as expected" } else { "MISMATCH" }
                    ),
                ))
            }
            _ => Err(unmapped(Domain::Plugin, op)),
        }
    }

    /// The catalog a step names: `url` for one out on the network, `target` for one the run stood up
    /// (whose port is handed out while it runs, so there is no URL a scenario could have written).
    /// Naming neither is caught here rather than in the loader — a required key is one key, and
    /// which of the two a step carries is what tells the two kinds of catalog apart.
    fn catalog_url(&self, with: &Args) -> Result<String, String> {
        if let Some(name) = with.get("target").and_then(|v| v.as_str()) {
            return self
                .catalogs
                .get(name)
                .map(|stood| stood.host.url(CATALOG_PATH))
                .ok_or_else(|| format!("internal: no catalog was stood up as `{name}`"));
        }
        with.get("url")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .ok_or_else(|| "a catalog is named by `url:` or by the `target:` a `catalog-stand` bound".to_string())
    }

    /// Write one field onto an installed plugin's declared schema — the one road the three `declare-`
    /// setting ops take, since what separates them is the field they write and nothing else.
    ///
    /// `over` is what that op adds to a plain line: the `secret` flag, the candidates and their default.
    /// The label is display text no scenario reads back, so a step that names none is given the key.
    ///
    /// A step's own `required` rides along whichever op it came through, since the flag that says a plugin
    /// cannot work without an answer is orthogonal to the shape that answer takes.
    fn declare_field(&self, name: &str, key: &str, with: &Args, over: serde_json::Value) -> Result<(), String> {
        let path = self.session.home.join("plugins").join(name).join("manifest.json");
        let raw = std::fs::read_to_string(&path)
            .map_err(|e| format!("could not read {}: {e}", path.display()))?;
        let mut manifest: serde_json::Value = serde_json::from_str(&raw)
            .map_err(|e| format!("{} is not the manifest it should be: {e}", path.display()))?;
        // A plugin that takes no settings at all carries no list, which is a list to add to all the
        // same — the schema is absent, not closed.
        if manifest["config"].is_null() {
            manifest["config"] = serde_json::json!([]);
        }
        let fields = manifest["config"]
            .as_array_mut()
            .ok_or_else(|| format!("{}'s config schema is not a list of fields", path.display()))?;
        if fields.iter().any(|f| f["key"].as_str() == Some(key)) {
            return Err(format!("`{name}` already declares a setting called `{key}`"));
        }
        let label = with.get("label").and_then(|v| v.as_str()).unwrap_or(key);
        let must_be_answered = with.get("required").and_then(|v| v.as_bool()).unwrap_or(false);
        let mut field = serde_json::json!({
            "key": key, "label": label, "secret": false, "required": must_be_answered
        });
        for (k, v) in over.as_object().into_iter().flatten() {
            field[k] = v.clone();
        }
        fields.push(field);
        std::fs::write(&path, serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?)
            .map_err(|e| format!("could not write {}: {e}", path.display()))
    }
}

/// Give the file at `path` the exec bit, so what was just written there can be run as a program.
///
/// Standing in for a plugin's program with a script is a unix shape: elsewhere the executable carries
/// the platform's suffix and no script is one, which is refused rather than left to fail as a plugin
/// that will not start. Nothing is lost by it — the plugins these scenarios install publish no build
/// for such a platform either, so the install ahead of this is the step that stops there.
#[cfg(unix)]
fn make_runnable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| format!("could not make {} runnable: {e}", path.display()))
}

/// Open or shut `path` to its owner — the directory holding the installed plugins, which amenbo either
/// reads or does not.
///
/// Unix-only for the same reason standing a program in is: the permission bit that decides a read is a unix
/// shape, and elsewhere a directory is kept from being read by another mechanism entirely. Refused rather
/// than left to pass silently, which would run the scenario's step and change nothing.
#[cfg(unix)]
fn set_readable(path: &Path, readable: bool) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let mode = if readable { 0o700 } else { 0o000 };
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .map_err(|e| format!("could not set the permissions on {}: {e}", path.display()))
}

#[cfg(not(unix))]
fn set_readable(path: &Path, _readable: bool) -> Result<(), String> {
    Err(format!(
        "{} cannot be shut here: what a directory may be read by is not a permission bit on this platform",
        path.display()
    ))
}

#[cfg(not(unix))]
fn make_runnable(path: &Path) -> Result<(), String> {
    Err(format!(
        "{} cannot be stood in for here: a plugin's program is a script only on unix",
        path.display()
    ))
}
