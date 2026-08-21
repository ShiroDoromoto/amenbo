//! The `plugin` domain: what is installed on this machine, whose gate is open, what a call
//! returned, what the execution log kept — and the catalogs all of it comes from, including the
//! ones the run stands up itself to walk the key a registration pins.

use std::path::Path;

use amenbo_scenario::{Args, Domain};
use amenbo_static_host::StaticHost;

use crate::{opt_bool, path_str, req_bool, req_i64, req_str, unmapped, Driver, Outcome};

/// Where a stood catalog publishes its index — the URL a registration is given.
const CATALOG_PATH: &str = "/catalog.json";

/// And where it publishes its signing key. Amenbo looks for the key **beside** the `catalog.json` it
/// was given, so this path is not a choice: it follows from the one above.
const CATALOG_KEY_PATH: &str = "/catalog-key.pub";

/// Where one plugin's second document is published. Amenbo resolves it beside the `catalog.json` it
/// was given, so this follows from [`CATALOG_PATH`] the same way the key's path does.
fn detail_path(name: &str) -> String {
    format!("/plugins/{name}.json")
}

/// And where one language's translated lines are published — the third document a catalog serves,
/// beside the other two and resolved the same way. A language no row was written in is a path nobody
/// published, and the host answers those with a 404, which is exactly the answer a real catalog gives
/// for a language nobody has translated.
fn list_overlay_path(lang: &str) -> String {
    format!("/catalog.{lang}.json")
}

/// When this catalog says it was generated. A published catalog carries the moment its CI ran; one a
/// run stands up has no such moment, so it carries a fixed one and the bytes stay the same from run
/// to run.
const CATALOG_STAMP: &str = "2026-07-27T00:00:00Z";

/// Who the entries name as their author and where they say they come from. Neither is what a road
/// standing a catalog up is about — the subject is the shelf — but both are fields the catalog rules
/// hold every entry to, so they are answered once here rather than asked of every scenario. The
/// repository is this project's own plugin repo, which is a name that answers: an opened panel reads
/// stars and a README off it, and one nobody publishes would draw a 404 into a screen the road is
/// about to be photographed on.
const OFFER_AUTHOR: &str = "in-house";
const OFFER_REPO: &str = "ShiroDoromoto/amenbo-plugin-worktree";

/// The category every offered entry is filed under. A browse filters by it, and no road yet turns on
/// which one an entry carries, so it is one value rather than a word each scenario has to choose.
const OFFER_CATEGORY: &str = "workflow";

/// One entry a scenario asked a stood catalog to offer, as the two documents a catalog serves carry
/// it: a row in the list, and a document of its own under [`detail_path`].
struct Offer {
    /// The plugin's name — the row's identity, and the key its detail document is fetched by.
    name: String,
    /// The one line a row draws under the name.
    desc: String,
    /// What its author wrote about it at length, which is the body an opened panel is read by. It
    /// rides in the detail document rather than in the list, so a row that has one costs a browse
    /// nothing — and a row that has none is the plugin whose panel goes to the repository for a
    /// README instead.
    about: Option<String>,
    /// The badge this catalog's *document* claims. It is not an entry's to grant: a shelf anyone may
    /// publish into holds no review, so the merge clears it on everything a registered catalog
    /// serves. Claiming it is therefore the only way a road can see that clearing happen at all.
    claims_official: bool,
    /// The one setting the author declares — the key it is stored under, and the label a form shows
    /// beside it. The panel under an opened row is the only place a detail document reaches the
    /// screen, so a shelf whose entries declare nothing puts nothing on that panel that came from it.
    setting: Option<(String, String)>,
    /// The same words as the author wrote them elsewhere, keyed by language code. They leave by
    /// different doors — the line beside the list, the description text and the label inside the
    /// detail — which is the split the delivery is built on, so they are held together here and
    /// parted at the moment of publishing.
    translated: std::collections::BTreeMap<String, Words>,
}

/// One language's half of a row: whichever of the words its author wrote in it. All are
/// optional and they travel on two documents, so a language holding one of them is an ordinary
/// state and not a half-written one.
#[derive(Default)]
struct Words {
    desc: Option<String>,
    about: Option<String>,
    label: Option<String>,
}

impl Offer {
    /// The row, as the list document carries it.
    ///
    /// `detail_sum` is left off. It is the catalog CI's slot — the digest that keeps update detection
    /// riding on the one list fetch — and Amenbo reads it as optional, checking the pairing only where
    /// an entry declares one. A run that stands its own shelf up publishes both halves in the same
    /// breath, so there is nothing here for a digest to catch that the publishing could get wrong.
    fn entry(&self) -> serde_json::Value {
        serde_json::json!({
            "name": self.name,
            "desc": self.desc,
            "author": OFFER_AUTHOR,
            "repo": OFFER_REPO,
            "os": ["macos", "linux"],
            "category": OFFER_CATEGORY,
            "official": self.claims_official,
            "featured": false,
            "added_at": null,
        })
    }

    /// The second document, fetched only when someone opens or installs this row.
    ///
    /// It carries no asset, so an install off this shelf stops at the door — which is the point: what
    /// a stood catalog is for is the seeing, and an install is walked against the real catalog, whose
    /// signature is the very layer a fixture cannot stand in for.
    fn detail(&self) -> String {
        let mut doc = serde_json::json!({ "name": self.name, "payload_v": 1 });
        if let Some((key, label)) = &self.setting {
            doc["config"] = serde_json::json!([{ "key": key, "label": label }]);
        }
        if let Some(about) = &self.about {
            doc["about"] = serde_json::json!(about);
        }
        // Every language at once, since this document is fetched one plugin at a time and then read
        // offline — which is what lets a panel and a form follow a language change with no request
        // behind it. Only what travels this way is in it — the description text and the label — the
        // line beside the list having left by the other door.
        let i18n: serde_json::Map<String, serde_json::Value> = self
            .translated
            .iter()
            .filter_map(|(lang, words)| {
                let mut overlay = serde_json::Map::new();
                if let Some(about) = &words.about {
                    overlay.insert("about".into(), serde_json::json!(about));
                }
                if let (Some((key, _)), Some(label)) = (self.setting.as_ref(), words.label.as_ref()) {
                    overlay.insert("config".into(), serde_json::json!({ key: { "label": label } }));
                }
                (!overlay.is_empty())
                    .then(|| (lang.clone(), serde_json::Value::Object(overlay)))
            })
            .collect();
        if !i18n.is_empty() {
            doc["i18n"] = serde_json::Value::Object(i18n);
        }
        doc.to_string()
    }
}

/// Read what a step asked this catalog to offer. Naming nothing is an empty shelf, which is what a
/// road about the trust root taken at registration wants: every count in the run then belongs to the
/// official catalog alone.
fn offers(with: &Args) -> Result<Vec<Offer>, String> {
    let Some(rows) = with.get("offers") else { return Ok(Vec::new()) };
    let rows = rows
        .as_sequence()
        .ok_or("`offers` is the list of entries this catalog serves")?;
    rows.iter()
        .map(|row| {
            let word = |key: &str| row.get(key).and_then(|v| v.as_str()).map(str::to_string);
            let name = word("name").ok_or("an offered entry needs a `name`")?;
            let desc = word("desc").ok_or_else(|| format!("`{name}` needs a `desc` for its row"))?;
            let setting = word("setting")
                .map(|key| (key.clone(), word("label").unwrap_or(key)));
            Ok(Offer {
                name,
                desc,
                about: word("about"),
                claims_official: row
                    .get("claims_official")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                setting,
                translated: translated(row),
            })
        })
        .collect()
}

/// The languages one row was also written in. The shape is the loader's to judge — a row that
/// reached here has been through [`amenbo_scenario`]'s check — so anything that is not a language's
/// mapping of words is simply not one of them.
fn translated(row: &serde_yaml::Value) -> std::collections::BTreeMap<String, Words> {
    let Some(langs) = row.get("translated").and_then(|v| v.as_mapping()) else { return Default::default() };
    langs
        .iter()
        .filter_map(|(lang, words)| {
            let lang = lang.as_str()?.to_string();
            let word = |key: &str| words.get(key).and_then(|v| v.as_str()).map(str::to_string);
            Some((lang, Words { desc: word("desc"), about: word("about"), label: word("label") }))
        })
        .collect()
}

/// Every document a stood catalog publishes, keyed by the path it answers at: the list, one detail
/// beside it per entry, and one document per language any row drew a line in.
///
/// The last of those is published only where there is a line to put in it. A language whose rows
/// translated the description text or the form label alone leaves no document — both of those went
/// out inside the details — so a browse in that language draws the base lines and meets a 404 on the
/// way, which is the answer a real catalog gives and not a failure.
fn catalog_docs(offers: &[Offer]) -> Vec<(String, String)> {
    let mut docs: Vec<(String, String)> =
        offers.iter().map(|o| (detail_path(&o.name), o.detail())).collect();
    let list = serde_json::json!({
        "catalog_v": 1,
        "generated_at": CATALOG_STAMP,
        "plugins": offers.iter().map(Offer::entry).collect::<Vec<_>>(),
    });
    docs.push((CATALOG_PATH.to_string(), list.to_string()));

    let mut lines: std::collections::BTreeMap<&str, serde_json::Map<String, serde_json::Value>> =
        Default::default();
    for offer in offers {
        for (lang, words) in &offer.translated {
            let Some(desc) = &words.desc else { continue };
            lines
                .entry(lang.as_str())
                .or_default()
                .insert(offer.name.clone(), serde_json::json!({ "desc": desc }));
        }
    }
    for (lang, entries) in lines {
        docs.push((list_overlay_path(lang), serde_json::Value::Object(entries).to_string()));
    }
    docs
}

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
                // Everything after `plugin run <name>` belongs to the plugin, so Amenbo's own flags
                // have to be said before the subcommand — appended, they would reach the plugin as
                // arguments and Amenbo would see no facet at all.
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
            // update exists for, which no sequence of Amenbo commands can arrive at (see the registry).
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
            // Adding a plain line to what an installed plugin says it takes. Amenbo reads the schema
            // off the installed manifest and never invents a field, so this is the author's declaration
            // arriving the only way it can while no published plugin carries one (see the registry).
            // What the scenario then walks — where the value is kept, what a read gives back, what
            // clearing it does — is Amenbo's own, untouched.
            "declare-setting" => {
                let name = req_str(with, "name")?;
                let key = req_str(with, "key")?;
                // Whose value it is, when the author said it is not the user's. The flag
                // rides on the same declaration `required` does, and for the same reason: the field
                // written is the same field, and what changes is what the faces do about it.
                let over = match opt_bool(with, "readonly") {
                    Some(true) => serde_json::json!({ "readonly": true }),
                    _ => serde_json::json!({}),
                };
                self.declare_field(name, key, with, over)?;
                // Whether the plugin can work without an answer is the difference the scenario after this
                // turns on, so the evidence line says which of the two was written.
                Ok(Outcome::action(match (opt_bool(with, "readonly"), opt_bool(with, "required")) {
                    (Some(true), _) => {
                        format!("`{name}` now declares `{key}` as a value it fills in itself")
                    }
                    (_, Some(true)) => {
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
                // A candidate carries its own condition, and only one of them can: a step
                // names which by its stored value, so a choice offering one answer under a condition and
                // the rest unconditionally is written in one line rather than several.
                let conditioned = with.get("candidate").and_then(|v| v.as_str());
                let candidate_when = when_from(with, "candidate_when")?;
                if candidate_when.is_some() && conditioned.is_none() {
                    return Err(
                        "`candidate_when_field` needs `candidate` — a condition on a candidate has to say which one".to_string()
                    );
                }
                let candidates: Vec<serde_json::Value> = options
                    .split(',')
                    .map(|value| {
                        let mut option = serde_json::json!({ "value": value, "label": value });
                        if conditioned == Some(value) {
                            if let Some(when) = &candidate_when {
                                option["when"] = when.clone();
                            }
                        }
                        option
                    })
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
            // And the door one tier along: an operation the settings form offers to run, with the one
            // value that press asks for. It is written onto the installed manifest for the reason a
            // declared setting is (see the registry) — the block is the author's word and no published
            // plugin carries one. A plugin already declaring this same call has it replaced, so a road
            // that declares twice reads as the second declaration rather than as two buttons.
            "declare-action" => {
                let name = req_str(with, "name")?;
                let cmd = req_str(with, "cmd")?;
                let label = req_str(with, "label")?;
                let path = self.session.home.join("plugins").join(name).join("manifest.json");
                let raw = std::fs::read_to_string(&path)
                    .map_err(|e| format!("could not read {}: {e}", path.display()))?;
                let mut manifest: serde_json::Value = serde_json::from_str(&raw)
                    .map_err(|e| format!("{} is not the manifest it should be: {e}", path.display()))?;
                let mut action = serde_json::json!({ "cmd": cmd, "label": label });
                // When the button is offered at all — the same pair a setting takes, on the
                // control that acts on those settings.
                if let Some(when) = when_from(with, "when")? {
                    action["when"] = when;
                }
                // What the press asks for, where the step named one. An operation asking for nothing is
                // the other shape it comes in — a button that runs the moment it is pressed — so the key
                // is left off rather than written empty, which the form would draw as a box with no name.
                if let Some(key) = with.get("ask").and_then(|v| v.as_str()) {
                    let asked = with.get("ask_label").and_then(|v| v.as_str()).unwrap_or(key);
                    // Whether the author called that value a credential. It rides on this same
                    // declaration for the reason `required` rides on a setting's: the field written is the
                    // same field, and what the flag changes is what the form does in front of it.
                    let secret = with.get("ask_secret").and_then(|v| v.as_bool()).unwrap_or(false);
                    action["ask"] = serde_json::json!([{ "key": key, "label": asked, "secret": secret }]);
                }
                // A plugin whose author wrote no settings block carries none, which is a block to write
                // into all the same — the face is absent, not closed.
                if manifest["settings"].is_null() {
                    manifest["settings"] = serde_json::json!({});
                }
                if manifest["settings"]["actions"].is_null() {
                    manifest["settings"]["actions"] = serde_json::json!([]);
                }
                let declared = manifest["settings"]["actions"].as_array_mut().ok_or_else(|| {
                    format!("{}'s settings block does not offer a list of operations", path.display())
                })?;
                declared.retain(|a| a["cmd"].as_str() != Some(cmd));
                declared.push(action);
                std::fs::write(&path, serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?)
                    .map_err(|e| format!("could not write {}: {e}", path.display()))?;
                // What it asks for is said back, since a button that runs on the press and one that opens
                // boxes first are two different roads from the same declaration.
                let asks_secretly = with.get("ask_secret").and_then(|v| v.as_bool()).unwrap_or(false);
                Ok(Outcome::action(match (with.get("ask").and_then(|v| v.as_str()), asks_secretly) {
                    (Some(key), true) => {
                        format!("`{name}` now offers `{label}`, which asks for `{key}` as a credential at the press")
                    }
                    (Some(key), false) => {
                        format!("`{name}` now offers `{label}`, which asks for `{key}` at the press")
                    }
                    (None, _) => format!("`{name}` now offers `{label}`, which runs on the press"),
                }))
            }
            // And the check that stands in front of the gate, written onto the same block. It is the one
            // declaration that changes what an enable does, so a plugin carrying it is a plugin whose
            // switch runs somebody else's code before it opens anything.
            "declare-check" => {
                let name = req_str(with, "name")?;
                let cmd = req_str(with, "cmd")?;
                let path = self.session.home.join("plugins").join(name).join("manifest.json");
                let raw = std::fs::read_to_string(&path)
                    .map_err(|e| format!("could not read {}: {e}", path.display()))?;
                let mut manifest: serde_json::Value = serde_json::from_str(&raw)
                    .map_err(|e| format!("{} is not the manifest it should be: {e}", path.display()))?;
                if manifest["settings"].is_null() {
                    manifest["settings"] = serde_json::json!({});
                }
                // One check, not a list: a plugin that already declared one has it replaced, the way a
                // second `declare-agent` replaces the block rather than joining it.
                manifest["settings"]["check"] = serde_json::json!(cmd);
                std::fs::write(&path, serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?)
                    .map_err(|e| format!("could not write {}: {e}", path.display()))?;
                Ok(Outcome::action(format!("`{name}` now has `{cmd}` judge its settings before its gate opens")))
            }
            // Standing in a program that answers that check. Whether values are usable is the author's
            // judgement and Amenbo makes none of its own (see the registry), so a no is a thing only a
            // program can say — and which way it answers is the scenario's to choose, since the road that
            // matters is the same values turned away and then let through. The document goes to stdout,
            // where Amenbo reads a verdict from; anything the run wants to say for a log goes nowhere here.
            "check-program" => {
                let name = req_str(with, "name")?;
                let ok = req_bool(with, "ok")?;
                let path = self.session.home.join("plugins").join(name).join(name);
                if !path.exists() {
                    return Err(format!("`{name}` has no program at {} to stand in for", path.display()));
                }
                let mut verdict = serde_json::json!({ "v": 1, "ok": ok });
                if let Some(message) = with.get("message").and_then(|v| v.as_str()) {
                    verdict["message"] = serde_json::json!(message);
                }
                // The line beside one box, which is a different reading from the sentence over the form:
                // a check may speak about the settings as a whole, about one of them, or about both.
                if let Some(field) = with.get("field").and_then(|v| v.as_str()) {
                    let said = with.get("field_message").and_then(|v| v.as_str()).unwrap_or(field);
                    verdict["fields"] = serde_json::json!({ field: said });
                }
                let document = serde_json::to_string(&verdict).map_err(|e| e.to_string())?;
                // Single quotes around the document, so nothing in the author's sentences reaches the
                // shell. A sentence carrying one would end the string and is refused rather than written.
                if document.contains('\'') {
                    return Err("a verdict's sentences cannot carry a single quote — the program says them from a script".to_string());
                }
                std::fs::write(&path, format!("#!/bin/sh\necho '{document}'\nexit 0\n"))
                    .map_err(|e| format!("could not write {}: {e}", path.display()))?;
                make_runnable(&path)?;
                Ok(Outcome::action(match ok {
                    true => format!("left `{name}`'s own check saying its settings are usable"),
                    false => format!("left `{name}`'s own check turning its settings away"),
                }))
            }
            // Standing in a program that says what a press handed it, so the one line a form draws has an
            // author behind it. An operation gives nothing back (see the registry): what is drawn is the
            // first line the run wrote to stderr, and the value the press asked for arrives in the
            // environment of that same run and is kept nowhere else — so the program says both in one
            // line, and a road can read from the form that the press reached the author's code carrying
            // what was typed into it.
            //
            // It answers a check with a yes on the other stream, because the two faces read different
            // ones: a press draws stderr and discards stdout, a check reads stdout and only logs stderr.
            // A plugin has one program, so a settings block with both halves in it would otherwise need
            // two standing in the same place — and this way the road declares both and neither face
            // hears the other's answer.
            "press-program" => {
                let name = req_str(with, "name")?;
                let path = self.session.home.join("plugins").join(name).join(name);
                if !path.exists() {
                    return Err(format!("`{name}` has no program at {} to stand in for", path.display()));
                }
                // `AMENBO_ASK_` is Amenbo's own prefix for what a press asked for, and the whole value is
                // taken rather than the variable's name, since what the form draws is read by an eye. A
                // press that asked for nothing leaves the line saying so, which is the reading a road
                // about a button that runs outright wants. The verdict on stdout is what a check reads,
                // and a press never looks at it. The script ends cleanly whatever it found: a non-zero
                // exit is a failed operation, and the line would be drawn as one.
                //
                // Before that line, where the road asked for it, the press writes one of the plugin's own
                // settings back — the same `plugin config set` an author writes, run from inside the run
                // with the store and the window Amenbo handed it. It is nowhere near the store's files: a
                // value laid down by hand would prove the form draws what is *there*, and what this road
                // is about is the press putting it there. A refusal takes the operation's line, so the
                // screen says what went wrong instead of leaving an empty field to explain.
                let back = match (
                    with.get("writes").and_then(|v| v.as_str()),
                    with.get("writes_value").and_then(|v| v.as_str()),
                ) {
                    (None, _) => String::new(),
                    (Some(key), value) => {
                        let value = value.unwrap_or_default();
                        let bin = self.bin.display().to_string();
                        // All three go into the script single-quoted, which is the one thing they could
                        // break. A quote is refused rather than escaped: the road would still run, and
                        // what it then wrote would not be what it says it wrote.
                        if [bin.as_str(), key, value].iter().any(|s| s.contains('\'')) {
                            return Err(
                                "a value written back from a press cannot carry a single quote — the program writes it from a script"
                                    .to_string(),
                            );
                        }
                        format!(
                            "if ! said=$('{bin}' plugin config set '{name}' '{key}' '{value}' 2>&1); then\n\
                             \x20 echo \"the write-back was refused: $said\" >&2\n\
                             \x20 echo '{{\"v\":1,\"ok\":true}}'\n\
                             \x20 exit 0\n\
                             fi\n"
                        )
                    }
                };
                std::fs::write(
                    &path,
                    format!(
                        "#!/bin/sh\n\
                         {back}\
                         asked=$(env | sed -n 's/^AMENBO_ASK_[A-Za-z0-9_]*=//p' | head -1)\n\
                         if [ -z \"$asked\" ]; then asked='nothing at all'; fi\n\
                         echo \"the operation was handed $asked\" >&2\n\
                         echo '{{\"v\":1,\"ok\":true}}'\n\
                         exit 0\n"
                    ),
                )
                .map_err(|e| format!("could not write {}: {e}", path.display()))?;
                make_runnable(&path)?;
                Ok(Outcome::action(match with.get("writes").and_then(|v| v.as_str()) {
                    Some(key) => format!(
                        "left `{name}` answering a press by writing `{key}` back, then one line naming what it was asked for, and a check with a yes"
                    ),
                    None => format!(
                        "left `{name}` answering a press with one line naming what it was asked for, and a check with a yes"
                    ),
                }))
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
                    let mut call = serde_json::json!({ "cmd": cmd, "does": does });
                    // Where the author says this call is a tool. A block that names none is the shape
                    // every manifest had before the field existed, so an absent `steps` writes none.
                    let named: Vec<&str> = with
                        .get("steps")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .collect();
                    if !named.is_empty() {
                        call["steps"] = serde_json::json!(named);
                    }
                    agent["commands"] = serde_json::json!([call]);
                }
                manifest["agent"] = agent;
                std::fs::write(&path, serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?)
                    .map_err(|e| format!("could not write {}: {e}", path.display()))?;
                // The steps are said back, so two declarations that differ only in where the call
                // claims to be a tool do not read alike in the report.
                let hung = match with.get("steps").and_then(|v| v.as_str()) {
                    Some(steps) if !steps.trim().is_empty() => format!(", a tool at {steps}"),
                    _ => String::new(),
                };
                Ok(Outcome::action(format!("`{name}` now says for itself: {when}{hung}")))
            }
            // The layer an author declared, written onto the installed manifest — a project's rows, or
            // the device's. It arrives here for the same reason a declared setting does:
            // the layer is the author's word, a manifest saying nothing means `project`, and no plugin
            // the official catalog serves says anything — so there is no install that reaches a
            // machine-wide plugin. What follows is Amenbo's own doing: which rows the enable opens, and
            // how wide a window a launched run is handed.
            "declare-scope" => {
                let name = req_str(with, "name")?;
                let scope = req_str(with, "scope")?;
                // The vocabulary is closed, and a word outside it would be written onto the manifest
                // and read back as a manifest Amenbo cannot parse — a failure two steps from the typo.
                if !matches!(scope, "project" | "machine") {
                    return Err(format!(
                        "`{scope}` is not a layer a manifest declares — it is `project` or `machine`"
                    ));
                }
                let path = self.session.home.join("plugins").join(name).join("manifest.json");
                let raw = std::fs::read_to_string(&path)
                    .map_err(|e| format!("could not read {}: {e}", path.display()))?;
                let mut manifest: serde_json::Value = serde_json::from_str(&raw)
                    .map_err(|e| format!("{} is not the manifest it should be: {e}", path.display()))?;
                manifest["scope"] = serde_json::Value::String(scope.to_string());
                std::fs::write(&path, serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?)
                    .map_err(|e| format!("could not write {}: {e}", path.display()))?;
                Ok(Outcome::action(match scope {
                    "machine" => format!(
                        "`{name}` is installed as the machine's now — its gate, settings and secrets sit at the device"
                    ),
                    _ => format!("`{name}` is installed as one project's now"),
                }))
            }
            // The badge off, so what is installed is a stranger's. Nothing a road does with Amenbo can
            // reach this state: the badge is the catalog's to grant, every plugin the official catalog
            // serves arrives with it, and an author who could write it onto themselves would be the
            // reason the split is worth nothing. So it comes off the installed manifest here — and what
            // follows is what a user meets the moment they install from anywhere else.
            "unbadge" => {
                let name = req_str(with, "name")?;
                let path = self.session.home.join("plugins").join(name).join("manifest.json");
                let raw = std::fs::read_to_string(&path)
                    .map_err(|e| format!("could not read {}: {e}", path.display()))?;
                let mut manifest: serde_json::Value = serde_json::from_str(&raw)
                    .map_err(|e| format!("{} is not the manifest it should be: {e}", path.display()))?;
                manifest["official"] = serde_json::Value::Bool(false);
                std::fs::write(&path, serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?)
                    .map_err(|e| format!("could not write {}: {e}", path.display()))?;
                Ok(Outcome::action(format!("`{name}` is installed as a stranger's now, not the catalog's")))
            }
            // Standing in a program that says what it was handed, so the injection has a witness. A
            // config value reaches a run and nowhere else: the store holds the answer rather than what
            // it is worth, the log is kept clear of it, and the read that says a secret is set says
            // nothing more. A plugin is the only thing on the receiving end, and the published ones use
            // their settings rather than report them (see the registry) — so this one prints its
            // injected config and stops there. Both roads, because a setting travels by the one its
            // author's `secret` flag chose: an environment variable for a secret, the stdin document
            // for the rest. The `AMENBO_CONFIG_` prefix is Amenbo's own; the callback variables beside
            // it are left out, because what is under test is the value a plugin was told, not the door
            // it can read back through.
            "echo-program" => {
                let name = req_str(with, "name")?;
                let path = self.session.home.join("plugins").join(name).join(name);
                if !path.exists() {
                    return Err(format!("`{name}` has no program at {} to stand in for", path.display()));
                }
                // `grep` finding nothing is a non-zero exit, and a command run that exits non-zero is a
                // failure Amenbo reports rather than a return value — so the script ends by saying it
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
            // Standing in a program that calls Amenbo back, so the read-back path has a witness. What a
            // plugin is handed when it is launched is the store to open and the window to read through,
            // and both travel in its environment — the script below names neither, which is the whole of
            // what is under test: a read goes through with no facet written anywhere, a read past the
            // window does not, and a write still says who is writing. The published plugins take none of
            // this route (see the registry), so there is nothing to ask but a stand-in.
            //
            // Eight faces. `read`, `write` and `records` name a task and hand everything after the id to
            // Amenbo as it was written, so the call each step makes is readable in the scenario. `take`
            // and `keep` name no task, because the calls they make — `export` and `backup` — ask for the
            // whole device rather than for anything a window could hold, which is why they are refused
            // outright. `version`, `carry`, `changes` and `records` are the road that *is* the window,
            // answered: the four calls something keeping a copy of this store elsewhere really makes —
            // ask whether it moved, take the whole once, read on from where the whole stood, and read the
            // rows it named back — from the only place a window is real. The binary is named in full
            // because the one under test is not the `amenbo` on
            // `PATH` — an author writes `amenbo`, and this is that line pointed at the build being
            // verified.
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
                // program ends cleanly whatever Amenbo said.
                std::fs::write(
                    &path,
                    format!(
                        "#!/bin/sh\n\
                         face=\"$1\"; shift\n\
                         case \"$face\" in\n\
                         take) set -- export --json \"$@\" ;;\n\
                         keep) set -- backup --json \"$@\" ;;\n\
                         version) set -- sync version --json \"$@\" ;;\n\
                         carry) set -- sync snapshot --json \"$@\" ;;\n\
                         changes) set -- sync changes --json \"$@\" ;;\n\
                         records)\n\
                           if [ $# -lt 1 ]; then echo 'this face takes the id of a task'; exit 0; fi\n\
                           id=\"$1\"; shift\n\
                           set -- sync records --dataset task --ids \"$id\" \"$@\" ;;\n\
                         read|write)\n\
                           if [ $# -lt 1 ]; then echo 'this face takes the id of a task'; exit 0; fi\n\
                           id=\"$1\"; shift\n\
                           if [ \"$face\" = read ]; then set -- task show \"$id\" --json \"$@\"\n\
                           else set -- comment add \"$id\" --json \"$@\"; fi ;;\n\
                         *) echo \"no face called $face\"; exit 0 ;;\n\
                         esac\n\
                         '{bin}' \"$@\" 2>&1\n\
                         exit 0\n"
                    ),
                )
                .map_err(|e| format!("could not write {}: {e}", path.display()))?;
                make_runnable(&path)?;
                Ok(Outcome::action(format!(
                    "left `{name}` calling Amenbo back with the store and window it is handed"
                )))
            }
            // Leaving an installed plugin answering slowly, so its queue has something in it to read.
            // A row comes off a queue the moment the plugin replies, so a backlog is a window and not
            // a state Amenbo can be asked for (see the registry): what is queued while a plugin is
            // still on it is the only backlog there is. The program is replaced rather than the
            // manifest edited — how long a plugin takes is the program's own doing, and nothing about
            // the install is being lied about. Everything after it is Amenbo's: the queue, the lease
            // and the runner are its own, and the events are ones the plugin really subscribes to.
            "slow-program" => {
                let name = req_str(with, "name")?;
                let seconds = req_i64(with, "seconds")?;
                if seconds <= 0 {
                    return Err("`seconds` has to be a window an assert can read in".to_string());
                }
                // `<home>/plugins/<name>/<name>` — the executable Amenbo runs, under the plugin's own
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
            // Closing Amenbo's view of what is installed, and opening it again — the only way to leave a
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
            //
            // A setting is held per crossing, and the command names no project: which one it writes
            // for is answered by where it is typed. So a step that names a crossing is typed from
            // that project's folder, and one that names none is typed where the run stands.
            "config-set" => {
                let name = req_str(with, "name")?;
                let key = req_str(with, "key")?;
                let value = req_str(with, "value")?;
                let argv = ["plugin", "config", "set", name, key, value, "--json"];
                let v = match self.project_folder(with)? {
                    Some(dir) => self.run_json_in(&dir, &argv)?,
                    None => self.run_json(&argv)?,
                };
                let crossing = crossing_named(with);
                Ok(Outcome::action(match v["cleared"].as_bool() {
                    Some(true) => format!("took `{key}` back off `{name}` for {crossing}"),
                    _ => format!("told `{name}` its `{key}` for {crossing}"),
                }))
            }
            // A catalog of the run's own on the loopback, so a scenario can walk what only a catalog
            // that answers can show: the key it publishes beside its `catalog.json`, the pin taken on
            // it, and the rows it serves. What it offers is the scenario's to say — a road about the
            // trust root wants an empty shelf, and one about a row coming off a shelf of one's own
            // wants that row on it, in the words its own document carries.
            "catalog-stand" => {
                let publishes_key = req_bool(with, "publishes_key")?;
                let name = bind.ok_or("`catalog-stand` produces a catalog, so it needs an `as:` name")?;
                let offered = offers(with)?;
                let host = StaticHost::serve(catalog_docs(&offered));
                let url = host.url(CATALOG_PATH);
                let mut stood = StoodCatalog { host, key: None };
                if publishes_key {
                    stood.rotate_key();
                }
                self.catalogs.insert(name.to_string(), stood);
                Ok(Outcome::action(format!(
                    "stood a catalog at {url} ({}, offering {})",
                    if publishes_key { "publishing a signing key" } else { "publishing no key" },
                    match offered.as_slice() {
                        [] => "nothing".to_string(),
                        rows => rows.iter().map(|o| format!("`{}`", o.name)).collect::<Vec<_>>().join(", "),
                    }
                )))
            }
            // The publisher rotates their key, at the same URL. Nothing about the catalog moves —
            // that is the point: what Amenbo has to notice is the key alone.
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
                let mut argv = vec!["plugin", "catalog", sub, &url, "--yes", "--json"];
                // What the shelf is called on screen. A registration that names none is called after
                // the host of its URL, which for a loopback catalog is an address with a port picked
                // this run — so a road that reads where a row came from has to give the shelf a name
                // it can be written down by.
                if let Some(name) = with.get("name").and_then(|v| v.as_str()) {
                    argv.extend_from_slice(&["--name", name]);
                }
                // `--yes` is the consent a registration takes when the catalog publishes a key: this
                // is a non-interactive run, and Amenbo refuses to pin a trust root without being told
                // so. A catalog with no key to pin never asks, so passing it costs that case nothing.
                let v = self.run_json(&argv)?;
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
                // The author's one required sentence, on the face a person reads. Whether it is readable
                // here is the whole of what the split between a colleague's plugin and a stranger's
                // decides — the sentence is not dropped, it is kept where its reader is. What it says is
                // deliberately not read back: the wording is the author's to change any day, and a line
                // holding them to today's would go red on a change Amenbo had no part in.
                if let Some(want) = opt_bool(with, "desc") {
                    let said = row.and_then(|r| r["desc"].as_str()).unwrap_or_default();
                    let pass = said.is_empty() != want;
                    return Ok(Outcome::assert(
                        pass,
                        format!(
                            "`plugin list` {} (expected it {}, {})",
                            if said.is_empty() {
                                format!("says nothing about `{name}`")
                            } else {
                                format!("describes `{name}` as {said:?}")
                            },
                            if want { "described" } else { "left undescribed" },
                            if pass { "as expected" } else { "MISMATCH" }
                        ),
                    ));
                }
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
                // What the author wrote and this reader does not get. Naming a field asks what it says,
                // which a field that is not there cannot answer — so the absence is its own reading, and
                // it is the one two different rules both land in: a stranger's prose never reaches this
                // document at all, and a block that stopped passing the rules is turned away whole where
                // it is read out. Both leave an entry that has to be told apart from one whose author
                // wrote nothing, and this is what tells them apart.
                if let Some(fields) = with.get("absent").and_then(|v| v.as_str()) {
                    let carried: Vec<&str> = fields
                        .split(',')
                        .map(str::trim)
                        .filter(|f| !f.is_empty())
                        .filter(|f| match *f {
                            // `does` is written beside each call rather than on the entry, so it is
                            // asked of the calls: one line still carrying it is one sentence arriving.
                            "does" => entry["commands"]
                                .as_array()
                                .is_some_and(|cs| cs.iter().any(|c| c.get("does").is_some())),
                            field => entry.get(field).is_some(),
                        })
                        .collect();
                    return Ok(Outcome::assert(
                        carried.is_empty(),
                        format!(
                            "`{name}` carries {} at the entry point (expected none of `{fields}`, {})",
                            match carried.as_slice() {
                                [] => "none of them".to_string(),
                                cs => cs.join(", "),
                            },
                            if carried.is_empty() { "as expected" } else { "MISMATCH" }
                        ),
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
                // What Amenbo adds: the author wrote `<cmd>`, and what an AI receives has to be a line
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
            // The same document, read where the author said their call belongs. `at-entry` reads the
            // shelf a plugin's own words sit on; this reads the step Amenbo wrote, and asks whether the
            // line to type is hanging there. Nothing else about the step is read: what a reader is owed
            // at a step is Amenbo's own sentence plus, at most, a call they can make.
            "at-step" => {
                let name = req_str(with, "name")?;
                let named = req_str(with, "step")?;
                let face = req_str(with, "cmd")?;
                let present = req_bool(with, "present")?;
                // As the AI, like `at-entry`: this document is the one an AI is pointed at, and what
                // hangs on a step is what is callable in the folder it is bound to.
                let v = self.run_json(&["agent", "--json", "--actor", "ai"])?;
                let Some((run, id)) = named.split_once('.') else {
                    return Err(format!("`{named}` is no step id — a run and the step within it, `<run>.<step>`"));
                };
                let step = hung_on(&v, run, id);
                // The line Amenbo builds, not the face the author wrote: the command word in front is
                // this build's own name, so it is read back off the line rather than dictated here.
                let tail = format!(" plugin run {name} {face}");
                let hung: Vec<&str> = step.clone().unwrap_or_default();
                let typed = hung.iter().find(|line| line.ends_with(&tail) && line.len() > tail.len());
                Ok(Outcome::assert(
                    typed.is_some() == present,
                    match (typed, present) {
                        (Some(line), true) => format!("`{named}` carries `{line}` — as expected"),
                        (Some(line), false) => {
                            format!("`{named}` carries `{line}` (expected nothing there, MISMATCH)")
                        }
                        (None, false) => format!(
                            "`{named}` carries no call to `{face}` from `{name}` (expected none, as expected)"
                        ),
                        (None, true) => format!(
                            "`{named}` carries {} — none of them a call to `{face}` from `{name}` (MISMATCH)",
                            match (step.is_some(), hung.as_slice()) {
                                (false, _) => "no such step in this document".to_string(),
                                (true, []) => "no tool at all".to_string(),
                                (true, ls) => ls.join(", "),
                            }
                        ),
                    },
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
                // `present: false` is the same reading, asked the other way — what a window shut out
                // must not be in what came back through it.
                let present = opt_bool(with, "present").unwrap_or(true);
                let found = value.contains(want);
                let pass = found == present;
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "the call returned {} bytes and {} `{want}` (expected it {}, {})",
                        value.len(),
                        if found { "carries" } else { "does not carry" },
                        if present { "carried" } else { "left out" },
                        if pass { "as expected" } else { "MISMATCH" }
                    ),
                ))
            }
            // A setting read back as one project holds it — one value per crossing, so the read asks
            // the same question the write answered, and from the same place: which project answers is
            // decided by where the command is typed, not by anything on it.
            "config" => {
                let name = req_str(with, "name")?;
                let key = req_str(with, "key")?;
                refuse_screen_reading(with)?;
                let argv = ["plugin", "config", "get", name, key, "--json"];
                let v = match self.project_folder(with)? {
                    Some(dir) => self.run_json_in(&dir, &argv)?,
                    None => self.run_json(&argv)?,
                };
                let held = crossing_read(with);
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
                            "plugin `{name}` holds `{key}`{held} as `{got}` (expected `{want}`, {})",
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
                            "plugin `{name}` reads `{key}`{held} back as {}{} (expected {}, {})",
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
                                "plugin `{name}` reads `{key}`{held} back as {} (expected {want}, {})",
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
                                "plugin `{name}` {} `{key}`{held} (expected {}, {})",
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
        // What the author said about when this setting is drawn at all. Absent is the
        // unconditional field every declaration here wrote before the key existed.
        if let Some(when) = when_from(with, "when")? {
            field["when"] = when;
        }
        for (k, v) in over.as_object().into_iter().flatten() {
            field[k] = v.clone();
        }
        fields.push(field);
        std::fs::write(&path, serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?)
            .map_err(|e| format!("could not write {}: {e}", path.display()))?;
        self.translate_field(name, key, with)
    }

    /// And the words that field carries in the author's other languages, if the step named any.
    ///
    /// They go beside the manifest rather than in it, in the file an install writes what a catalog
    /// published into — which is what the faces that draw a form read, and what lets one follow a
    /// reader changing language with nothing fetched. The file holds every language at once, keyed by
    /// code, so a second declaration translated in another language joins the first rather than
    /// replacing it.
    fn translate_field(&self, name: &str, key: &str, with: &Args) -> Result<(), String> {
        if with.get("translated").and_then(|v| v.as_mapping()).is_none() {
            return Ok(());
        }
        let path = self.session.home.join("plugins").join(name).join("i18n.json");
        // Absent is a plugin nobody translated, which is where every install off the official catalog
        // stands: there is nothing to read back, and the layer starts here.
        let held: serde_json::Value = match std::fs::read_to_string(&path) {
            Ok(raw) => serde_json::from_str(&raw)
                .map_err(|e| format!("{} is not the record it should be: {e}", path.display()))?,
            Err(_) => serde_json::json!({}),
        };
        let all = translated_into(held, key, with);
        std::fs::write(&path, serde_json::to_string_pretty(&all).map_err(|e| e.to_string())?)
            .map_err(|e| format!("could not write {}: {e}", path.display()))
    }
}

/// The crossing a settings step named, as a line about it reads. A value is held per project, so a
/// line that left the project out would report a write and a read as the same act wherever they
/// landed — which is exactly the disagreement worth naming when two of these sit next to each other.
fn crossing_named(with: &Args) -> String {
    match with.get("project").and_then(|v| v.as_str()) {
        Some(project) => format!("`{project}`"),
        None => "the project this run stands in".to_string(),
    }
}

/// The same, for a verdict that reads a value back — a phrase to drop mid-sentence, and nothing at
/// all where the step named no crossing (the reading is then about the one place the run stands, and
/// saying so in every line would be noise).
fn crossing_read(with: &Args) -> String {
    match with.get("project").and_then(|v| v.as_str()) {
        Some(project) => format!(" in `{project}`"),
        None => String::new(),
    }
}

/// The one question about a setting that only a screen can answer, turned away here rather than quietly
/// passing.
///
/// `readonly` asks that a value is shown with no box to type into and no button to take it back. A
/// terminal has neither to withhold, and the write door it does have stays open on purpose — that door
/// is how the plugin's own value arrives. So a step asking it down this pipe would come
/// back green off a build that had stopped withholding anything, which is the reading this refuses.
fn refuse_screen_reading(with: &Args) -> Result<(), String> {
    if with.contains_key("readonly") {
        return Err(
            "`readonly` is a question about what the form withholds, so it belongs on a `steps_gui` \
             road where an eye reads it — a terminal has no box and no button, and writing the value \
             is open there on purpose"
                .to_string(),
        );
    }
    if with.contains_key("holds") {
        return Err(
            "`holds` is a question about the box a form draws the value in, so it belongs on a \
             `steps_gui` road — down this pipe the value comes back as a value, which is what `equals` \
             already reads"
                .to_string(),
        );
    }
    Ok(())
}

/// What one field's translations leave behind in the record beside the manifest: whatever was already
/// held, with this field's words written into each language the step named.
///
/// The shape is the one an install writes — language, then the fields keyed by the key each is declared
/// under, then the words. Keys rather than positions all the way down, both here and among a choice's
/// candidates, so a form reordered by its author does not silently re-label every language.
fn translated_into(mut held: serde_json::Value, key: &str, with: &Args) -> serde_json::Value {
    let Some(langs) = with.get("translated").and_then(|v| v.as_mapping()) else { return held };
    for (lang, words) in langs {
        let (Some(lang), Some(words)) = (lang.as_str(), words.as_mapping()) else { continue };
        let mut field = serde_json::json!({});
        if let Some(label) = words.get("label").and_then(|v| v.as_str()) {
            field["label"] = serde_json::json!(label);
        }
        if let Some(options) = words.get("options").and_then(|v| v.as_mapping()) {
            let shown: serde_json::Map<String, serde_json::Value> = options
                .iter()
                .filter_map(|(value, shown)| {
                    Some((value.as_str()?.to_string(), serde_json::json!(shown.as_str()?)))
                })
                .collect();
            field["options"] = serde_json::Value::Object(shown);
        }
        if held[lang].is_null() {
            held[lang] = serde_json::json!({ "config": {} });
        }
        held[lang]["config"][key] = field;
    }
    held
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

/// Open or shut `path` to its owner — the directory holding the installed plugins, which Amenbo either
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

/// The call lines hanging on one step of the entry point's document, by the id that names it
/// (`<run>.<step>`, split into its halves by the caller).
///
/// The two runs keep their steps in different places, and both are read here: the backbone's are one
/// list, a cycle's are two buckets — which bucket a step sits in is the cycle's business and no part of
/// the name. `None` says this document has no such step, which is a different answer from a step with
/// nothing on it: a run left out because it does not apply here, or an id no build of Amenbo has, both
/// land in the first, and a scenario walking that case reads the same emptiness for a reason worth
/// naming in the report.
fn hung_on<'a>(doc: &'a serde_json::Value, run: &str, id: &str) -> Option<Vec<&'a str>> {
    let buckets: Vec<&serde_json::Value> = if run == "agentCycle" {
        vec![&doc["agentCycle"]["steps"]]
    } else {
        vec![&doc["cycles"][run]["backbone"], &doc["cycles"][run]["optional"]]
    };
    let step = buckets
        .into_iter()
        .filter_map(|bucket| bucket.as_array())
        .flatten()
        .find(|step| step["id"].as_str() == Some(id))?;
    Some(
        step["tools"]
            .as_array()
            .map(|lines| lines.iter().filter_map(|line| line.as_str()).collect())
            .unwrap_or_default(),
    )
}

/// The condition an author put on what a step is declaring, read off the pair of words
/// a step writes it as — `<prefix>_field` naming the setting whose answer decides, `<prefix>_has` the
/// value looked for among its answers.
///
/// `None` where the step named neither, which is the unconditional shape everything here had before
/// there was anything to hide. Half a pair is a mistake worth a refusal rather than a silent
/// unconditional: a step that meant to hide something and did not would read green over a form
/// drawing everything.
///
/// The manifest takes a list, and one clause is what a step can say; a road needing two would be
/// reading a rule engine rather than a form.
fn when_from(with: &Args, prefix: &str) -> Result<Option<serde_json::Value>, String> {
    let field = with.get(format!("{prefix}_field").as_str()).and_then(|v| v.as_str());
    let has = with.get(format!("{prefix}_has").as_str()).and_then(|v| v.as_str());
    match (field, has) {
        (None, None) => Ok(None),
        (Some(field), Some(has)) => {
            Ok(Some(serde_json::json!([{ "field": field, "has": has }])))
        }
        _ => Err(format!(
            "`{prefix}_field` and `{prefix}_has` are written together — one names the setting the condition reads, the other the answer it looks for"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(yaml: &str) -> Args {
        serde_yaml::from_str(yaml).expect("the args a step would carry")
    }

    /// The reading a terminal has no way to answer, refused rather than passed over: a
    /// form withholds a box and a button, and down this pipe there is neither — while the write door
    /// that is open here is the one the plugin's own value arrives through.
    #[test]
    fn what_only_a_form_withholds_is_refused_on_this_road() {
        let Err(err) = refuse_screen_reading(&args("{ name: worktree, key: worker_url, readonly: true }"))
        else {
            panic!("`readonly` is a screen's question, and it has no answer down this pipe");
        };
        assert!(err.contains("readonly") && err.contains("steps_gui"), "got: {err}");
        // And the reading beside it: what a box on a form is holding is a screen's question too, while
        // the same value read down this pipe is what `equals` already answers.
        let Err(err) = refuse_screen_reading(&args("{ name: worktree, key: base, holds: main }")) else {
            panic!("`holds` names the box a form draws, which this road does not have");
        };
        assert!(err.contains("holds") && err.contains("equals"), "got: {err}");
        assert!(refuse_screen_reading(&args("{ name: worktree, key: base, equals: main }")).is_ok());
    }

    /// The condition a step writes as a pair of words, and the shape the manifest takes it in — one
    /// clause naming a setting and the answer looked for among its own.
    #[test]
    fn a_condition_is_read_off_the_pair_a_step_writes_it_as() {
        assert_eq!(when_from(&args("{ name: viewer, key: worker_url }"), "when"), Ok(None));
        assert_eq!(
            when_from(&args("{ when_field: transport, when_has: cloudflare }"), "when"),
            Ok(Some(serde_json::json!([{ "field": "transport", "has": "cloudflare" }])))
        );
        // The prefix is what lets one step carry both its own condition and its candidate's.
        assert_eq!(
            when_from(&args("{ candidate_when_field: mode, candidate_when_has: advanced }"), "candidate_when"),
            Ok(Some(serde_json::json!([{ "field": "mode", "has": "advanced" }])))
        );
    }

    /// Half a pair is refused rather than read as no condition at all: a step that meant to hide
    /// something and quietly did not would read green over a form drawing everything.
    #[test]
    fn half_a_condition_is_refused() {
        let Err(err) = when_from(&args("{ when_field: transport }"), "when") else {
            panic!("a condition naming no answer decides nothing");
        };
        assert!(err.contains("when_field") && err.contains("when_has"), "got: {err}");
        assert!(when_from(&args("{ when_has: cloudflare }"), "when").is_err());
    }

    fn served(docs: &[(String, String)], path: &str) -> serde_json::Value {
        let body = docs
            .iter()
            .find(|(p, _)| p == path)
            .unwrap_or_else(|| panic!("nothing is published at {path}: {docs:?}"));
        serde_json::from_str(&body.1).expect("a document a catalog serves is JSON")
    }

    /// The shape the entry point hands back: the backbone's steps in one list, and a cycle's split
    /// across the two buckets its items sit in. Tools hang on a step wherever it was written.
    fn entry_point() -> serde_json::Value {
        serde_json::json!({
            "agentCycle": { "steps": [
                { "id": "list", "step": "your mailbox is …" },
                { "id": "reserve", "step": "reserve it …", "tools": ["amenbo plugin run worktree start <task-id>"] },
            ]},
            "cycles": {
                "worktree": {
                    "backbone": [
                        { "id": "cut-per-task", "step": "cut one per task", "tools": [
                            "amenbo plugin run worktree start <task-id>",
                            "amenbo plugin run mirror sync",
                        ]},
                        { "id": "fold-it", "step": "fold it once merged" },
                    ],
                    "optional": [
                        { "id": "borrow-one", "step": "borrow a standing one", "tools": ["amenbo plugin run worktree list"] },
                    ],
                },
            },
        })
    }

    /// A step is found by the id that names it, in whichever run and bucket it was written, and what
    /// comes back is the lines hanging there — every one of them, since a step is a place and two
    /// plugins may both have named it.
    #[test]
    fn a_step_is_found_by_its_id_in_either_run() {
        let doc = entry_point();
        assert_eq!(
            hung_on(&doc, "agentCycle", "reserve"),
            Some(vec!["amenbo plugin run worktree start <task-id>"])
        );
        assert_eq!(
            hung_on(&doc, "worktree", "cut-per-task"),
            Some(vec![
                "amenbo plugin run worktree start <task-id>",
                "amenbo plugin run mirror sync",
            ]),
            "both lines, in the order the document carries them"
        );
        assert_eq!(
            hung_on(&doc, "worktree", "borrow-one"),
            Some(vec!["amenbo plugin run worktree list"]),
            "a self-gated item is a step like any other — the bucket is no part of the name"
        );
    }

    /// A step with nothing hanging on it and a step this document does not have are different
    /// answers: the first is a place a reader reached and found no tool, the second is a name that
    /// landed nowhere — an id no build has, or a whole cycle left out as inapplicable.
    #[test]
    fn nothing_hanging_and_no_such_step_are_told_apart() {
        let doc = entry_point();
        assert_eq!(hung_on(&doc, "worktree", "fold-it"), Some(Vec::new()));
        assert_eq!(hung_on(&doc, "worktree", "no-step-of-this-name"), None);
        assert_eq!(hung_on(&doc, "agentCycle", "cut-per-task"), None, "an id is only its own run's");
        assert_eq!(hung_on(&doc, "commit", "lint-what-leaves"), None, "a cycle the run left out");
    }

    /// A road about the trust root alone names no rows, and the shelf it stands on is empty — which
    /// is what keeps every count in that run about the official catalog.
    #[test]
    fn a_shelf_nobody_stocked_is_empty() {
        let docs = catalog_docs(&offers(&args("publishes_key: true")).expect("no rows to read"));
        assert_eq!(docs.len(), 1, "an empty shelf publishes the list and nothing else: {docs:?}");
        let list = served(&docs, CATALOG_PATH);
        assert_eq!(list["catalog_v"], 1);
        assert_eq!(list["plugins"].as_array().expect("a list of entries").len(), 0);
    }

    /// What a scenario writes is what the catalog's own document says — including the badge it is not
    /// entitled to, which is the claim a merge has to be seen clearing.
    #[test]
    fn a_row_is_served_in_the_words_the_scenario_wrote() {
        let docs = catalog_docs(
            &offers(&args(
                "offers:\n  - name: standup\n    desc: Post the day's finished tasks\n    claims_official: true\n    setting: channel\n    label: Channel webhook\n",
            ))
            .expect("one row to read"),
        );
        let entry = &served(&docs, CATALOG_PATH)["plugins"][0];
        assert_eq!(entry["name"], "standup");
        assert_eq!(entry["desc"], "Post the day's finished tasks");
        assert_eq!(entry["official"], true, "the claim is the document's, and it is served as written");
        assert_eq!(entry["detail_sum"], serde_json::Value::Null, "a shelf standing beside its own details declares no digest");

        // Beside it, under the path Amenbo resolves from the catalog's own URL.
        let detail = served(&docs, "/plugins/standup.json");
        assert_eq!(detail["name"], "standup", "the name is the join between the two documents");
        assert_eq!(detail["config"][0]["key"], "channel");
        assert_eq!(detail["config"][0]["label"], "Channel webhook");
    }

    /// A row that declares no setting carries no schema at all — an absent list, not an empty one, so
    /// the panel under it has nothing of this catalog's to draw.
    #[test]
    fn a_row_declaring_nothing_carries_no_schema() {
        let docs = catalog_docs(
            &offers(&args("offers:\n  - name: burndown\n    desc: Chart what is left\n"))
                .expect("one row to read"),
        );
        let detail = served(&docs, "/plugins/burndown.json");
        assert!(detail["config"].is_null(), "got: {detail}");
        assert_eq!(served(&docs, CATALOG_PATH)["plugins"][0]["official"], false, "a badge nobody claimed is not served as one");
    }

    /// The two words a row cannot be served without. A name is what it is fetched and badged by, and
    /// a description is the line drawn under it — a row missing either reaches a screen as a blank.
    #[test]
    fn a_row_needs_a_name_and_a_line() {
        assert!(offers(&args("offers:\n  - desc: no name here\n")).is_err());
        assert!(offers(&args("offers:\n  - name: nameless\n")).is_err());
        assert!(offers(&args("offers: standup")).is_err(), "a shelf is a list of rows, not one word");
    }

    /// The two halves of a translation leave by different doors, which is the whole of the split:
    /// the line beside the list, one document per language, and the label inside the row's own
    /// detail, every language at once.
    #[test]
    fn a_translated_row_leaves_by_both_doors() {
        let docs = catalog_docs(
            &offers(&args(
                "offers:\n  - name: standup\n    desc: Post the day's finished tasks\n    setting: channel\n    label: Channel webhook\n    translated:\n      ja:\n        desc: Erledigte Aufgaben des Tages posten\n        label: Webhook des Kanals\n",
            ))
            .expect("one row to read"),
        );

        let ja = served(&docs, "/catalog.ja.json");
        assert_eq!(ja["standup"]["desc"], "Erledigte Aufgaben des Tages posten");
        assert_eq!(
            served(&docs, CATALOG_PATH)["plugins"][0]["desc"],
            "Post the day's finished tasks",
            "the list itself stays in the language its author wrote the base row in",
        );

        let detail = served(&docs, "/plugins/standup.json");
        assert_eq!(detail["i18n"]["ja"]["config"]["channel"]["label"], "Webhook des Kanals");
        assert_eq!(detail["config"][0]["label"], "Channel webhook", "the base label is where it was");
    }

    /// The description an opened panel is read by travels inside the detail, every language at once —
    /// the same door the form's labels take, and for the same reason: a panel already fetched follows
    /// a reader changing language with nothing fetched again.
    #[test]
    fn a_described_row_carries_every_language_in_its_detail() {
        let docs = catalog_docs(
            &offers(&args(
                "offers:\n  - name: standup\n    desc: Post the day's finished tasks\n    about: What standup posts, and when.\n    translated:\n      de:\n        about: Was standup postet, und wann.\n",
            ))
            .expect("one row to read"),
        );

        let detail = served(&docs, "/plugins/standup.json");
        assert_eq!(detail["about"], "What standup posts, and when.");
        assert_eq!(detail["i18n"]["de"]["about"], "Was standup postet, und wann.");
        assert!(
            served(&docs, CATALOG_PATH)["plugins"][0]["about"].is_null(),
            "and none of it is in the list, which every reader fetches whole",
        );
    }

    /// A language nobody wrote a line in gets no document of its own, and the 404 that answers for it
    /// is what a reader of an untranslated language already meets. Neither a label nor a description
    /// is a line: both went out inside the detail, so neither raises a document here.
    #[test]
    fn a_language_with_no_line_publishes_no_document() {
        let docs = catalog_docs(
            &offers(&args(
                "offers:\n  - name: standup\n    desc: Post the day's finished tasks\n    about: What standup posts, and when.\n    setting: channel\n    label: Channel webhook\n    translated:\n      de:\n        about: Was standup postet, und wann.\n        label: Webhook des Kanals\n",
            ))
            .expect("one row to read"),
        );
        assert!(
            !docs.iter().any(|(path, _)| path.starts_with("/catalog.") && path != CATALOG_PATH),
            "no language document was called for: {:?}",
            docs.iter().map(|(p, _)| p).collect::<Vec<_>>(),
        );
        let detail = served(&docs, "/plugins/standup.json");
        assert_eq!(detail["i18n"]["de"]["config"]["channel"]["label"], "Webhook des Kanals");
        assert_eq!(detail["i18n"]["de"]["about"], "Was standup postet, und wann.");
    }

    /// A declared field's words in another language land in the record an install writes, under the key
    /// the field is declared by — and a second field translated afterwards joins the first rather than
    /// taking its place, since the record holds a plugin's whole form and not one field of it.
    #[test]
    fn a_declared_fields_words_join_what_the_record_already_holds() {
        let held = translated_into(
            serde_json::json!({}),
            "base",
            &args("translated:\n  de:\n    label: Basis-Branch\n"),
        );
        assert_eq!(held["de"]["config"]["base"]["label"], "Basis-Branch");

        let held = translated_into(held, "remote", &args("translated:\n  de:\n    label: Name der Gegenstelle\n"));
        assert_eq!(held["de"]["config"]["base"]["label"], "Basis-Branch", "the field written first is still there");
        assert_eq!(held["de"]["config"]["remote"]["label"], "Name der Gegenstelle");
    }

    /// A choice's candidates are translated under the value each one stores, never under its position:
    /// an author reordering the list would otherwise move every language's words onto another answer.
    #[test]
    fn a_choices_candidates_are_translated_by_the_value_they_store() {
        let held = translated_into(
            serde_json::json!({}),
            "events",
            &args("translated:\n  de:\n    label: Ereignisse\n    options:\n      task.done: Aufgabe erledigt\n"),
        );
        assert_eq!(held["de"]["config"]["events"]["label"], "Ereignisse");
        assert_eq!(held["de"]["config"]["events"]["options"]["task.done"], "Aufgabe erledigt");
        assert!(
            held["de"]["config"]["events"]["options"]["task.rejected"].is_null(),
            "a candidate nobody translated carries the words its author gave it"
        );
    }

    /// A step that named no other language leaves the record exactly as it was — which is what keeps
    /// every road that declares a setting without translating it writing no record at all.
    #[test]
    fn a_declaration_in_one_language_writes_no_layer() {
        let held = translated_into(serde_json::json!({}), "base", &args("label: Base branch\n"));
        assert_eq!(held, serde_json::json!({}));
    }

    /// One document per language, holding every row that drew a line in it — which is the shape the
    /// CI publishes, and the shape a browse reads one fetch of.
    #[test]
    fn one_language_document_holds_every_row_that_drew_a_line_in_it() {
        let docs = catalog_docs(
            &offers(&args(
                "offers:\n  - name: standup\n    desc: Post the day's finished tasks\n    translated:\n      ja:\n        desc: Erledigte Aufgaben des Tages posten\n  - name: burndown\n    desc: Chart what is left\n    translated:\n      ja:\n        desc: Den Rest als Diagramm zeigen\n",
            ))
            .expect("two rows to read"),
        );
        let ja = served(&docs, "/catalog.ja.json");
        assert_eq!(ja["standup"]["desc"], "Erledigte Aufgaben des Tages posten");
        assert_eq!(ja["burndown"]["desc"], "Den Rest als Diagramm zeigen");
    }
}
