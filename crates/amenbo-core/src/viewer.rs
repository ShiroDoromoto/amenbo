//! **The Viewer's server, stood up in the user's own Cloudflare account** (`AMB-D-884`, `AMB-D-886`).
//!
//! What Amenbo Viewer reads from is a Worker and a D1 database in the reader's own account. Nothing is
//! hosted here, and the whole of what a person is asked for is one press and one paste.
//!
//! **They are never asked to judge anything.** Which permissions a token needs, what the database is
//! called, how long a key is, where the Worker answers — every one of those is a decision this build has
//! already made, and a setup that handed any of them over would be asking someone to get right something
//! they have no way of checking.
//!
//! **What the API token can do is held for one run and by one process.** It is taken as an argument,
//! never written down: what is left in the store afterwards is the endpoint, the write token and the
//! encryption key, none of which can create anything in that account. What is left in the account is a
//! Worker and a database that are theirs — nothing here can reach them again without a token they would
//! have to paste a second time.

use std::time::Duration;

use base64::Engine as _;

use crate::error::{Error, Result};
use crate::model::SecretArea;
use crate::store::Store;

pub mod carried;
mod cloudflare;
pub mod code;
mod migrate;
pub mod pace;
pub mod pairing;
pub mod refusal;
pub mod sealing;
pub mod send;

use cloudflare::Sky;

/// The name the Worker and its database carry in the user's account. It is the same one the Worker's own
/// config names it by, so a user who later reaches for wrangler in that directory is pointed at the thing
/// this put there rather than at a second copy of it.
pub const SERVER_NAME: &str = "amenbo-viewer";

/// The bindings the uploaded Worker is given: the database it reads and writes, and the token it compares
/// a writer against. Both names are the Worker's own — they are how its code reaches them.
pub(crate) const DATABASE_BINDING: &str = "RECORDS";
pub(crate) const WRITE_TOKEN_BINDING: &str = "WRITE_TOKEN";

/// The compatibility date the Worker is deployed under, which is the one its own config names.
pub(crate) const COMPATIBILITY_DATE: &str = "2026-08-01";

/// The built Worker, baked in at compile time. The user's PC has no Node on it and no copy of this
/// repository, so what stands the server up carries what it deploys. The migrations its database is
/// brought up to are baked the same way (see [`migrate`]).
///
/// **It is generated.** The Worker's own source is what it is built from, and `make -C worker baked` is
/// what writes it here.
pub const WORKER_SCRIPT: &str = include_str!("viewer/worker.js");

/// Which build the script above is, spelled here because the number lives in the JavaScript as a literal
/// nothing on this side can import.
///
/// **It is what a Worker already standing is compared against.** The one in the user's account changes
/// only when they press setup, so the number it answers a write with says whether they are talking to the
/// Worker this build carries or to an older one — and that answer is what a screen says out loud, since
/// pressing the button is theirs to do and nobody else's.
///
/// `tests::the_baked_script_is_the_build_this_says_it_is` keeps the two level: a `make -C worker baked`
/// that was never run is a build claiming a Worker it does not carry.
pub const WORKER_BUILD: u32 = 3;

/// Where the number sits in the built script. esbuild emits the constant as a plain declaration, so the
/// name and the assignment are the whole of what has to be found.
const BUILD_IN_THE_SCRIPT: &str = "var BUILD = ";

/// The three fields setup leaves behind, in the table no road out of the store walks
/// ([`crate::ops::secret`]). They hang off no row — the Viewer keeps one set per device — so their
/// address is `(None, `[`SecretArea::Viewer`]`, None, field)`.
pub const WORKER_URL: &str = "worker_url";
pub const AUTH_TOKEN: &str = "auth_token";
pub const ENCRYPTION_KEY: &str = "encryption_key";

/// How long the two secrets are. The cipher fixes one of them — a key is 256 bits — and there is no
/// reason for the token that opens the writing door to be shorter than the key that opens the records.
const SECRET_SIZE: usize = 32;

/// What the check after the deploy is willing to wait through. A workers.dev name turned on a second ago
/// is not answering everywhere yet, and that is not a failure to report — it is a wait to sit through
/// once.
const CHECK_ATTEMPTS: usize = 6;
const CHECK_PAUSE: Duration = Duration::from_secs(3);

/// How long the check after the deploy gives one attempt.
const CHECK_TIMEOUT: Duration = Duration::from_secs(60);

/// The permissions the API token has to carry, and nothing besides. They go into the link that opens
/// Cloudflare's token screen with the boxes already ticked — the user presses Create, and is never asked
/// which of Cloudflare's permission groups a drop box takes.
const TOKEN_PERMISSIONS: &[(&str, &str)] = &[
    // to deploy the Worker
    ("workers_scripts", "edit"),
    // to make the database and write to it
    ("d1", "edit"),
    // to learn which account this is
    ("account_settings", "read"),
];

/// What a run of [`setup`] stood up.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Stood {
    /// Where the Worker answers — what a send is addressed to.
    pub url: String,
    /// The account it was built in.
    pub account: String,
    /// The database it reads and writes.
    pub database: String,
    /// Whether the keys already in the store were kept, or drawn afresh. A key drawn now opens nothing
    /// already on the server, which is what the next send has to know.
    pub keys: Keys,
}

/// Whether setup kept the keys it found or drew new ones.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Keys {
    Kept,
    Generated,
}

/// Where the calls go. The real Cloudflare unless a test puts something else in front of it — nobody has
/// an account to try this against, and a command whose only rehearsal was being read is one that runs for
/// the first time in somebody's account.
struct Reach {
    api: String,
    /// Where the deployed Worker answers, when it is not on workers.dev.
    worker: Option<String>,
}

impl Default for Reach {
    fn default() -> Reach {
        Reach { api: cloudflare::CLOUDFLARE_API.to_string(), worker: None }
    }
}

/// Stand the Viewer's server up in the user's own Cloudflare account, and leave the three settings a send
/// needs behind.
///
/// `api_token` is the token the user pasted. It is used here and nowhere else, and nothing writes it
/// down. `account` says which account to build in, and is only needed when the token reaches more than
/// one.
pub fn setup(store: &mut Store, api_token: &str, account: Option<&str>) -> Result<Stood> {
    setup_reaching(store, api_token, account, &Reach::default())
}

fn setup_reaching(
    store: &mut Store,
    api_token: &str,
    account: Option<&str>,
    reach: &Reach,
) -> Result<Stood> {
    let sky = Sky::reaching(api_token, &reach.api);

    let where_ = sky.the_account(account)?;
    tracing::info!(account = %where_, "building the Viewer's server in this account");

    let (database, fresh) = sky.the_database(&where_, SERVER_NAME)?;
    let applied = migrate::lay_the_schema_down(&sky, &where_, &database, fresh)?;
    tracing::info!(database = %database, fresh, applied = ?applied, "the database is up to date");

    // What is already in the store is kept. A key that opens the records on the server is the only copy
    // of it there is, so drawing a new one over it is what makes every row already up there unreadable.
    let kept_token = store.secret_value(None, SecretArea::Viewer, None, AUTH_TOKEN)?;
    let kept_key = store.secret_value(None, SecretArea::Viewer, None, ENCRYPTION_KEY)?;
    let keys = if kept_token.is_some() && kept_key.is_some() { Keys::Kept } else { Keys::Generated };
    let write_token = kept_token.clone().unwrap_or_else(drawn);
    let key = kept_key.clone().unwrap_or_else(drawn);

    sky.deploy(&where_, SERVER_NAME, WORKER_SCRIPT, &database, &write_token)?;
    tracing::info!(script = SERVER_NAME, "the Worker is deployed");

    let Some(subdomain) = sky.the_subdomain(&where_)? else {
        return Err(Error::invalid(format!(
            "this Cloudflare account has no workers.dev name yet, and it is yours to choose \
             — pick one at https://dash.cloudflare.com/{where_}/workers/subdomain, then press setup again"
        )));
    };
    sky.answer_on_the_subdomain(&where_, SERVER_NAME)?;
    let endpoint = match &reach.worker {
        Some(base) => format!("{}/worker", base.trim_end_matches('/')),
        None => format!("https://{SERVER_NAME}.{subdomain}.workers.dev"),
    };

    // The endpoint is written last on purpose. Half a route is not a route — a send reads the URL and the
    // token together — so a run interrupted between these three leaves the store where it was rather than
    // pointing it at somewhere it cannot get into.
    if kept_key.is_none() {
        store.set_secret(None, SecretArea::Viewer, None, ENCRYPTION_KEY, Some(&key))?;
    }
    if kept_token.is_none() {
        store.set_secret(None, SecretArea::Viewer, None, AUTH_TOKEN, Some(&write_token))?;
    }
    store.set_secret(None, SecretArea::Viewer, None, WORKER_URL, Some(&endpoint))?;

    await_the_worker(&endpoint)?;

    // A server that has just been stood up is one nothing has been sent to, whatever this device
    // remembers sending to the last one. Forgetting here is what makes the next turn place the whole
    // backlog rather than the next edit alone — and it is the only place it can be settled, since the
    // write token is refused at the server's reading door.
    //
    // **Forgetting is the whole of what this does to the store's own rows**, and nothing else is written
    // from here. A key drawn just now opens nothing already up there; the forgotten carrier sends the
    // whole backlog next, key by key, so every row the backlog still holds is written again under the new
    // key. What is left over is the rows the backlog no longer holds, which open for nobody — and a phone
    // reads that as its key not fitting, which is answered by the PC writing.
    store.forget_viewer_carried()?;

    tracing::info!(url = %endpoint, ?keys, "the Viewer's server is up");

    Ok(Stood { url: endpoint, account: where_, database, keys })
}

/// Wait for the deployed Worker to turn a stranger away.
///
/// **Being refused is the answer that means everything landed.** A 401 says the script is running, the
/// database it looks tokens up in is bound, and the write token it compares against is set — there is no
/// other single call that says all three, since reading a record needs a token no phone has been given
/// yet.
fn await_the_worker(endpoint: &str) -> Result<()> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(CHECK_TIMEOUT))
        .http_status_as_error(false)
        .build()
        .into();
    let at = format!("{endpoint}/meta");
    let mut last = String::new();
    for attempt in 1..=CHECK_ATTEMPTS {
        match agent.get(&at).call() {
            Ok(answer) => match answer.status().as_u16() {
                401 => return Ok(()),
                503 => {
                    return Err(Error::invalid(
                        "the Worker is up and has no write token set, so it takes no writes — press setup again",
                    ))
                }
                other => {
                    last = format!(
                        "{at} answered {other}, where a caller with no token should be turned away with 401"
                    )
                }
            },
            Err(err) => last = format!("{at} did not answer: {err}"),
        }
        if attempt < CHECK_ATTEMPTS {
            std::thread::sleep(CHECK_PAUSE);
        }
    }
    Err(Error::invalid(format!(
        "the Worker was deployed and is not answering yet — press setup again in a minute ({last})"
    )))
}

/// Draw one of the two secrets this keeps: [`SECRET_SIZE`] bytes of randomness, written the way
/// everything that leaves here is written.
fn drawn() -> String {
    let mut raw = [0u8; SECRET_SIZE];
    getrandom::fill(&mut raw).expect("failed to draw OS randomness");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw)
}

/// Cloudflare's token screen, opened with the permissions setup needs already chosen.
///
/// **The account form of the link, not the user form.** A user-token link (`/profile/api-tokens?…`) loses
/// its query the moment an unsigned-in visitor signs in, and lands them on the account home with nothing
/// ticked — and someone making a Cloudflare account for this is signed out by definition.
pub fn token_link() -> String {
    let permissions: Vec<serde_json::Value> = TOKEN_PERMISSIONS
        .iter()
        .map(|(key, kind)| serde_json::json!({ "key": key, "type": kind }))
        .collect();
    let asked = serde_json::Value::Array(permissions).to_string();
    format!(
        "https://dash.cloudflare.com/?to={}&permissionGroupKeys={}&name=Amenbo",
        escaped("/:account/api-tokens"),
        escaped(&asked)
    )
}

/// Percent-escape one value of that link's query.
fn escaped(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Which build a Worker script says it is, read out of the script itself.
///
/// It is the other half of [`WORKER_BUILD`]: that constant is what this build claims to carry, and this is
/// what the script it carries actually says. `tests::the_baked_script_is_the_build_this_says_it_is` holds
/// the two together.
///
/// `None` is a script with no such declaration — a bundler that emitted the constant some other way, or a
/// copy that is not the Worker at all.
pub fn build_in_the_script(script: &str) -> Option<u32> {
    let at = script.find(BUILD_IN_THE_SCRIPT)? + BUILD_IN_THE_SCRIPT.len();
    let digits: String = script[at..].chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use amenbo_static_host::{Reply, StaticHost};

    /// The number this build says it carries is the number in the script it carries. A
    /// `make -C worker baked` that was never run is what this catches: the script would still be the
    /// older one, and everything about the deploy would be right except which Worker landed.
    #[test]
    fn the_baked_script_is_the_build_this_says_it_is() {
        assert_eq!(
            build_in_the_script(WORKER_SCRIPT),
            Some(WORKER_BUILD),
            "the baked worker.js is not the build this says it deploys — run `make worker-baked`"
        );
    }

    /// A script with no such declaration reads as none rather than as some other number.
    #[test]
    fn a_script_that_declares_no_build_says_so() {
        assert_eq!(build_in_the_script("export default {};"), None);
        assert_eq!(build_in_the_script("var BUILD = 12;\n"), Some(12));
    }

    /// The link carries the three permission groups, ticked, and goes to the account form of the token
    /// screen — the one a signed-out visitor keeps its query through.
    #[test]
    fn the_token_link_ticks_the_permissions_it_needs() {
        let link = token_link();
        assert!(link.starts_with("https://dash.cloudflare.com/?to=%2F%3Aaccount%2Fapi-tokens"), "{link}");
        for (key, _) in TOKEN_PERMISSIONS {
            assert!(link.contains(&escaped(&format!("\"{key}\""))), "{link} is missing {key}");
        }
        assert!(link.ends_with("&name=Amenbo"), "{link}");
    }

    /// Two draws do not agree, and what is drawn is the size the cipher fixes.
    #[test]
    fn a_drawn_secret_is_its_own() {
        let (one, two) = (drawn(), drawn());
        assert_ne!(one, two);
        let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(&one).expect("base64");
        assert_eq!(raw.len(), SECRET_SIZE);
    }

    /// A store on a scratch base, so the settings setup writes land somewhere.
    fn store_at(tag: &str) -> Store {
        let dir = amenbo_scratch::scratch(&format!("viewer-{tag}"));
        std::fs::create_dir_all(&dir).unwrap();
        Store::open_at(crate::config::Paths::at(dir)).unwrap()
    }

    /// The envelope Cloudflare answers every call in.
    fn said(result: serde_json::Value) -> Reply {
        Reply::ok(serde_json::json!({ "success": true, "errors": [], "result": result }).to_string())
    }

    /// The same envelope, refused, with the code Cloudflare puts in it.
    fn refused(code: i64, message: &str) -> Reply {
        Reply::ok(
            serde_json::json!({
                "success": false,
                "errors": [{ "code": code, "message": message }],
                "result": null,
            })
            .to_string(),
        )
    }

    /// A stand-in answering the whole of setup: one account, a database that is not there yet, a deploy
    /// that lands, a workers.dev name, and a Worker that turns a caller with no token away.
    fn cloudflare_standing_in() -> StaticHost {
        let host = StaticHost::serve(Vec::<(String, String)>::new());
        host.set_reply(
            "/client/v4/accounts",
            said(serde_json::json!([{ "id": "acc1", "name": "Alice" }])),
        );
        host.set_reply(
            "/client/v4/accounts/acc1/d1/database?name=amenbo-viewer",
            said(serde_json::json!([])),
        );
        host.set_reply(
            "/client/v4/accounts/acc1/d1/database",
            said(serde_json::json!({ "uuid": "db1" })),
        );
        host.set_reply(
            "/client/v4/accounts/acc1/d1/database/db1/query",
            said(serde_json::json!([{ "success": true, "results": [] }])),
        );
        host.set_reply(
            "/client/v4/accounts/acc1/workers/scripts/amenbo-viewer",
            said(serde_json::json!({})),
        );
        host.set_reply(
            "/client/v4/accounts/acc1/workers/subdomain",
            said(serde_json::json!({ "subdomain": "alice" })),
        );
        host.set_reply(
            "/client/v4/accounts/acc1/workers/scripts/amenbo-viewer/subdomain",
            said(serde_json::json!({})),
        );
        // The deployed Worker, answering where the reach says it does: a caller with no token is turned
        // away, which is the one answer that says the script, the database and the secret all landed.
        host.set_reply("/worker/meta", Reply::status(401, ""));
        host
    }

    fn reaching(host: &StaticHost) -> Reach {
        Reach { api: host.url("/client/v4"), worker: Some(host.url("")) }
    }

    /// A whole run: the account is found, the database is made and given the schema, the Worker is
    /// uploaded with the database and the token bound to it, the name is turned on, and the three
    /// settings a send needs are in the store.
    #[test]
    fn a_run_stands_the_server_up_and_leaves_the_three_settings() {
        let host = cloudflare_standing_in();
        let mut store = store_at("stands-up");

        let stood = setup_reaching(&mut store, "a-pasted-token", None, &reaching(&host))
            .expect("the stand-in answers the whole of setup");

        assert_eq!(stood.account, "acc1");
        assert_eq!(stood.database, "db1");
        assert_eq!(stood.keys, Keys::Generated);
        assert_eq!(stood.url, host.url("/worker"));

        let read = |field: &str| {
            store.secret_value(None, SecretArea::Viewer, None, field).unwrap()
        };
        assert_eq!(read(WORKER_URL).as_deref(), Some(stood.url.as_str()));
        assert!(read(AUTH_TOKEN).is_some(), "a send needs the token as much as the URL");
        assert!(read(ENCRYPTION_KEY).is_some());
    }

    /// The upload carries the database and the write token as bindings of the same request. Set
    /// separately afterwards, a secret is one an ordinary redeploy would quietly drop — and a Worker with
    /// no write token takes no writes.
    #[test]
    fn the_upload_binds_the_database_and_seals_the_token_in() {
        let host = cloudflare_standing_in();
        let mut store = store_at("upload");
        setup_reaching(&mut store, "a-pasted-token", None, &reaching(&host)).expect("setup");

        let upload = host
            .heard()
            .into_iter()
            .find(|heard| {
                heard.method == "PUT"
                    && heard.target == "/client/v4/accounts/acc1/workers/scripts/amenbo-viewer"
            })
            .expect("the Worker was uploaded");
        let sent = String::from_utf8_lossy(&upload.body).to_string();
        assert!(
            upload.content_type.as_deref().unwrap_or_default().starts_with("multipart/form-data;"),
            "{:?}",
            upload.content_type
        );
        assert!(sent.contains(&format!("\"name\":\"{DATABASE_BINDING}\"")), "the database is bound");
        assert!(sent.contains("\"id\":\"db1\""), "and it is the one that was just made");
        assert!(sent.contains(&format!("\"name\":\"{WRITE_TOKEN_BINDING}\"")), "the token is sealed in");
        let token = store.secret_value(None, SecretArea::Viewer, None, AUTH_TOKEN).unwrap().unwrap();
        assert!(sent.contains(&token), "what the Worker compares against is what the store kept");
        assert!(sent.contains("var BUILD = 3;"), "and the script uploaded is the one baked in");
    }

    /// Keys already in the store are kept. A key that opens the records on the server is the only copy
    /// there is, so drawing a new one over it is what makes every row already up there unreadable.
    #[test]
    fn keys_already_kept_are_not_drawn_again() {
        let host = cloudflare_standing_in();
        let mut store = store_at("kept-keys");
        store.set_secret(None, SecretArea::Viewer, None, AUTH_TOKEN, Some("the-token")).unwrap();
        store.set_secret(None, SecretArea::Viewer, None, ENCRYPTION_KEY, Some("the-key")).unwrap();

        let stood = setup_reaching(&mut store, "a-pasted-token", None, &reaching(&host)).expect("setup");

        assert_eq!(stood.keys, Keys::Kept);
        assert_eq!(
            store.secret_value(None, SecretArea::Viewer, None, ENCRYPTION_KEY).unwrap().as_deref(),
            Some("the-key")
        );
    }

    /// An account with no workers.dev name is the one refusal the user has to answer themselves, and it
    /// says where to answer it. Nothing here picks a name for them: it is taken account-wide.
    #[test]
    fn an_account_with_no_workers_dev_name_is_told_to_choose_one() {
        let host = cloudflare_standing_in();
        host.set_reply(
            "/client/v4/accounts/acc1/workers/subdomain",
            refused(10007, "workers.dev subdomain not found"),
        );
        let mut store = store_at("no-subdomain");

        let refusal = setup_reaching(&mut store, "a-pasted-token", None, &reaching(&host))
            .expect_err("a name nobody has chosen is not one to guess at");

        let said = refusal.message_en();
        assert!(said.contains("workers.dev"), "{said}");
        assert!(said.contains("/workers/subdomain"), "{said}");
        assert!(
            store.secret_value(None, SecretArea::Viewer, None, WORKER_URL).unwrap().is_none(),
            "half a route is not a route, so nothing is pointed at one"
        );
    }

    /// A token that reaches several accounts is refused rather than guessed at, and the refusal names
    /// them: building in the wrong one leaves a Worker somewhere the user has no reason to look.
    #[test]
    fn a_token_reaching_several_accounts_is_asked_which_one() {
        let host = cloudflare_standing_in();
        host.set_reply(
            "/client/v4/accounts",
            said(serde_json::json!([
                { "id": "acc1", "name": "Alice" },
                { "id": "acc2", "name": "Bob" },
            ])),
        );
        let mut store = store_at("two-accounts");

        let refusal = setup_reaching(&mut store, "a-pasted-token", None, &reaching(&host))
            .expect_err("two accounts is not a coin to toss");
        let said = refusal.message_en();
        assert!(said.contains("acc1") && said.contains("acc2"), "{said}");

        // Named, it builds there and asks nothing.
        let stood = setup_reaching(&mut store, "a-pasted-token", Some("acc1"), &reaching(&host));
        assert_eq!(stood.expect("the named account is built in").account, "acc1");
    }

    /// The SQL of the one call the schema goes over in, as the stand-in heard it.
    fn the_schema_run(host: &StaticHost) -> String {
        let asked = host
            .heard()
            .into_iter()
            .rfind(|heard| heard.target == "/client/v4/accounts/acc1/d1/database/db1/query")
            .expect("the schema went over");
        let body: serde_json::Value =
            serde_json::from_slice(&asked.body).expect("D1 is asked in JSON");
        body["sql"].as_str().expect("the statements").to_string()
    }

    /// A database made a moment ago is asked nothing and given everything, each migration followed by the
    /// line that records it. Both travel in one call: D1 stops at the first statement that fails, so what
    /// the ledger says was applied is what applied.
    #[test]
    fn a_database_made_now_is_given_every_migration_and_a_ledger_that_says_so() {
        let host = cloudflare_standing_in();
        let mut store = store_at("fresh-database");
        setup_reaching(&mut store, "a-pasted-token", None, &reaching(&host)).expect("setup");

        let run = the_schema_run(&host);
        assert!(run.starts_with("CREATE TABLE IF NOT EXISTS d1_migrations"), "{run}");
        let mut reached = 0;
        for (name, _) in migrate::MIGRATIONS {
            let recorded = format!("INSERT INTO d1_migrations (name) VALUES ('{name}');");
            let at = run[reached..].find(&recorded).expect("every migration is recorded, in order");
            reached += at;
        }
    }

    /// A database laid out before the ledger existed is credited with what the first release shipped,
    /// rather than with nothing — crediting it with nothing would re-apply the very tables it stands on.
    #[test]
    fn a_database_from_before_the_ledger_is_credited_with_what_it_has() {
        let host = cloudflare_standing_in();
        // It has records and no ledger: laid out by a release that recorded nothing.
        host.set_reply(
            "/client/v4/accounts/acc1/d1/database?name=amenbo-viewer",
            said(serde_json::json!([{ "uuid": "db1", "name": "amenbo-viewer" }])),
        );
        host.set_reply(
            "/client/v4/accounts/acc1/d1/database/db1/query",
            said(serde_json::json!([{ "success": true, "results": [{ "name": "records" }] }])),
        );
        let mut store = store_at("pre-ledger");
        setup_reaching(&mut store, "a-pasted-token", None, &reaching(&host)).expect("setup");

        let run = the_schema_run(&host);
        assert!(
            run.contains("VALUES ('0001_records_and_tokens.sql')"),
            "what it already has is written down rather than applied again: {run}"
        );
        assert!(
            !run.contains("CREATE TABLE IF NOT EXISTS records"),
            "and the tables it stands on are not laid down a second time: {run}"
        );
        assert!(run.contains("VALUES ('0006_one_code_and_not_one_per_phone.sql')"), "{run}");
    }

    /// A database that keeps a ledger is given only what its ledger does not name.
    #[test]
    fn a_database_that_says_what_it_has_had_is_given_only_the_rest() {
        let host = cloudflare_standing_in();
        host.set_reply(
            "/client/v4/accounts/acc1/d1/database?name=amenbo-viewer",
            said(serde_json::json!([{ "uuid": "db1", "name": "amenbo-viewer" }])),
        );
        // Asked what tables it has, then what it has had — two questions down one road, so two answers.
        let had: Vec<serde_json::Value> = migrate::MIGRATIONS[..4]
            .iter()
            .map(|(name, _)| serde_json::json!({ "name": name }))
            .collect();
        host.set_replies(
            "/client/v4/accounts/acc1/d1/database/db1/query",
            [
                said(serde_json::json!([{
                    "success": true,
                    "results": [{ "name": "records" }, { "name": "d1_migrations" }],
                }])),
                said(serde_json::json!([{ "success": true, "results": had }])),
                said(serde_json::json!([{ "success": true, "results": [] }])),
            ],
        );
        let mut store = store_at("has-a-ledger");
        setup_reaching(&mut store, "a-pasted-token", None, &reaching(&host)).expect("setup");

        let run = the_schema_run(&host);
        for (name, _) in &migrate::MIGRATIONS[..4] {
            assert!(!run.contains(name), "{name} was already had and went over again: {run}");
        }
        for (name, _) in &migrate::MIGRATIONS[4..] {
            assert!(run.contains(name), "{name} had not been applied and did not go over: {run}");
        }
    }

    /// Standing a server up forgets what the carrier was holding, so the next turn places the whole
    /// backlog. A memory that says "level" behind a server that has taken nothing would hand it the next
    /// edit and nothing else — and nothing can ask the server which it is, the write token being refused
    /// at its reading door.
    #[test]
    fn standing_a_server_up_forgets_what_was_sent_to_the_last_one() {
        let host = cloudflare_standing_in();
        let mut store = store_at("forgets-the-carrier");
        store
            .set_viewer_carried(&crate::viewer::carried::Carried {
                version: 12_345,
                cursor: 980,
                ..Default::default()
            })
            .unwrap();
        store
            .enqueue_viewer(&[crate::viewer::carried::Waiting {
                record_key: "task/1".into(),
                op: crate::viewer::carried::Op::Placed,
                body: Some("{}".into()),
            }])
            .unwrap();

        setup_reaching(&mut store, "a-pasted-token", None, &reaching(&host)).expect("setup");

        assert_eq!(store.viewer_carried().unwrap(), crate::viewer::carried::Carried::default());
        assert_eq!(store.viewer_waiting().unwrap(), 0, "what was read out under the old cursor goes too");
    }

    /// The token is used and not written down. What is left in the store can create nothing in that
    /// account, which is the whole point of holding the one that can for a single run.
    #[test]
    fn the_api_token_is_not_left_behind() {
        let host = cloudflare_standing_in();
        let mut store = store_at("token-not-kept");
        setup_reaching(&mut store, "a-pasted-token", None, &reaching(&host)).expect("setup");

        for field in [WORKER_URL, AUTH_TOKEN, ENCRYPTION_KEY] {
            let kept = store.secret_value(None, SecretArea::Viewer, None, field).unwrap();
            assert_ne!(kept.as_deref(), Some("a-pasted-token"), "{field} kept the API token");
        }
    }
}
