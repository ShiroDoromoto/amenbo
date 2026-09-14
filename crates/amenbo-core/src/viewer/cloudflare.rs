//! **The road to Cloudflare's REST API**, walked once per setup with a token the user pasted
//! (`AMB-D-886`).
//!
//! Standing the Viewer's server up is five things in the user's own account, and every one of them goes
//! through here: find which account this is, make the database, put the schema in it, upload the Worker
//! with the database bound to it and the write token sealed into it, and turn on the name it answers
//! under.
//!
//! **The API token is never kept.** It stands the route up and goes when the call ends. What is written
//! back into the store — the endpoint, the write token, the encryption key — can create nothing in their
//! account, which is the point: the credential that could is held for one run and by one process.
//!
//! **No failure here quotes the token.** These sentences reach a log, and one that echoed the credential
//! would put it somewhere it outlives the run that needed it.

use std::time::Duration;

use serde::de::DeserializeOwned;

use crate::error::Error;

/// The road. A stand-in takes its place through [`Sky::reaching`], which is how a test walks the whole of
/// setup: nobody has a Cloudflare account to try this against, and a command whose only rehearsal was
/// being read is one that runs for the first time in somebody's account.
pub(crate) const CLOUDFLARE_API: &str = "https://api.cloudflare.com/client/v4";

/// What one API call gets. Standing the route up is a handful of calls in a row with a person waiting on
/// them, so a call that is not going to answer has to give the run back rather than hold it.
const CALL_TIMEOUT: Duration = Duration::from_secs(60);

/// What Cloudflare puts in the envelope when an account has no workers.dev name yet. The same call is
/// also turned down for a token missing the permission, for a rate limit, and for the API's own bad days
/// — none of which a user can do anything about by choosing a name — so this code, and nothing else,
/// means "choose one".
const CODE_NO_SUBDOMAIN: i64 = 10007;

/// The module the uploaded Worker starts at. It is a name inside the upload and nothing else — the file
/// it came from is the copy baked into this binary.
const SCRIPT_ENTRY: &str = "index.js";

/// A call that did not give back what was asked for.
#[derive(Debug)]
pub(crate) enum Failure {
    /// Cloudflare answered its own envelope with `success: false`. It answers every call in one envelope,
    /// refusals included, so what a caller reads is the code and the sentence Cloudflare put there rather
    /// than an HTTP status with nothing behind it.
    Refused { call: String, code: i64, message: String },
    /// Nothing answered, or what answered was not that envelope.
    Unreadable(Error),
}

impl Failure {
    fn unreadable(said: impl Into<String>) -> Failure {
        Failure::Unreadable(Error::Io(std::io::Error::other(said.into())))
    }
}

impl From<Failure> for Error {
    fn from(failure: Failure) -> Error {
        match failure {
            Failure::Refused { call, code, message } => {
                Error::Io(std::io::Error::other(format!(
                    "Cloudflare refused {call}: {message} ({code})"
                )))
            }
            Failure::Unreadable(error) => error,
        }
    }
}

pub(crate) type Reached<T> = std::result::Result<T, Failure>;

/// The Cloudflare API, holding the token for one run of setup.
pub(crate) struct Sky {
    token: String,
    base: String,
    agent: ureq::Agent,
}

/// One account the token reaches.
#[derive(serde::Deserialize)]
pub(crate) struct Account {
    pub id: String,
    pub name: String,
}

/// One statement's outcome, as D1 answers a query: several of them, one per statement.
#[derive(serde::Deserialize)]
pub(crate) struct Queried {
    pub success: bool,
    #[serde(default)]
    pub results: Vec<serde_json::Value>,
}

impl Sky {
    /// Reach whatever is answering at `base` instead — a stand-in, in a test.
    pub(crate) fn reaching(token: &str, base: &str) -> Sky {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(CALL_TIMEOUT))
            .http_status_as_error(false)
            .build()
            .into();
        Sky { token: token.to_string(), base: base.trim_end_matches('/').to_string(), agent }
    }

    /// Make one call and hand back what the envelope carried as its result.
    fn ask<T: DeserializeOwned>(
        &self,
        method: &str,
        path: &str,
        content_type: Option<&str>,
        body: Option<Vec<u8>>,
    ) -> Reached<T> {
        let at = format!("{}{path}", self.base);
        let mut building = ureq::http::Request::builder()
            .method(method)
            .uri(&at)
            .header("authorization", format!("Bearer {}", self.token))
            .header("accept", "application/json");
        if let Some(kind) = content_type {
            building = building.header("content-type", kind);
        }
        let request = building.body(body.unwrap_or_default()).map_err(|err| {
            Failure::unreadable(format!("this build could not put the {path} call together: {err}"))
        })?;
        let mut answer = self.agent.run(request).map_err(|err| {
            Failure::unreadable(format!("Cloudflare did not answer {path}: {err}"))
        })?;
        let status = answer.status();
        let said = answer
            .body_mut()
            .read_to_string()
            .map_err(|err| Failure::unreadable(format!("Cloudflare answered {path} with {status} and a body this build could not read: {err}")))?;

        let envelope: Envelope = serde_json::from_str(&said).map_err(|err| {
            Failure::unreadable(format!(
                "Cloudflare answered {path} with {status} and something this build cannot read: {err}"
            ))
        })?;
        if !envelope.success {
            return Err(match envelope.errors.first() {
                Some(first) => Failure::Refused {
                    call: path.to_string(),
                    code: first.code,
                    message: first.message.clone(),
                },
                None => Failure::unreadable(format!(
                    "Cloudflare refused {path} with {status} and gave no reason"
                )),
            });
        }
        serde_json::from_value(envelope.result).map_err(|err| {
            Failure::unreadable(format!(
                "Cloudflare answered {path} in a shape this build cannot read: {err}"
            ))
        })
    }

    /// [`Sky::ask`] for the calls that send JSON, which is all of them but the upload.
    fn ask_with<T: DeserializeOwned>(
        &self,
        method: &str,
        path: &str,
        body: &serde_json::Value,
    ) -> Reached<T> {
        self.ask(method, path, Some("application/json"), Some(body.to_string().into_bytes()))
    }

    /// Work out which account to build in.
    ///
    /// One account is the ordinary answer and needs nothing from the user. Several is the case worth
    /// refusing over rather than guessing at: building in the wrong one leaves a Worker and a database
    /// somewhere they were not meant to be, and the user would have no reason to look there.
    pub(crate) fn the_account(&self, chosen: Option<&str>) -> Reached<String> {
        let accounts: Vec<Account> = self.ask("GET", "/accounts", None, None)?;
        if accounts.is_empty() {
            return Err(Failure::Unreadable(Error::invalid(
                "this token reaches no Cloudflare account — make one that does, and paste it again",
            )));
        }
        if let Some(chosen) = chosen {
            return match accounts.iter().find(|account| account.id == chosen) {
                Some(account) => Ok(account.id.clone()),
                None => Err(Failure::Unreadable(Error::invalid(format!(
                    "this token does not reach the account {chosen}"
                )))),
            };
        }
        if accounts.len() == 1 {
            return Ok(accounts[0].id.clone());
        }
        let named: Vec<String> =
            accounts.iter().map(|a| format!("  {}  {}", a.id, a.name)).collect();
        Err(Failure::Unreadable(Error::invalid(format!(
            "this token reaches {} accounts, so say which one to build in:\n{}",
            accounts.len(),
            named.join("\n")
        ))))
    }

    /// Find the database by name, or make it, and say which of the two happened — the schema is only laid
    /// down in one that was not there a moment ago.
    pub(crate) fn the_database(&self, account: &str, name: &str) -> Reached<(String, bool)> {
        #[derive(serde::Deserialize)]
        struct Found {
            uuid: String,
            name: String,
        }
        let path = format!("/accounts/{account}/d1/database?name={}", escaped(name));
        let found: Vec<Found> = self.ask("GET", &path, None, None)?;
        if let Some(database) = found.into_iter().find(|database| database.name == name) {
            return Ok((database.uuid, false));
        }

        #[derive(serde::Deserialize)]
        struct Made {
            #[serde(default)]
            uuid: String,
        }
        let made: Made = self.ask_with(
            "POST",
            &format!("/accounts/{account}/d1/database"),
            &serde_json::json!({ "name": name }),
        )?;
        if made.uuid.is_empty() {
            return Err(Failure::unreadable("Cloudflare made a database and did not say which one"));
        }
        Ok((made.uuid, true))
    }

    /// Run SQL against the database. A whole run goes through in one call — D1 takes the statements
    /// together and stops at the first that fails, which is what keeps a half-applied schema from being a
    /// state anyone has to reason about.
    pub(crate) fn query(&self, account: &str, database: &str, sql: &str) -> Reached<Vec<Queried>> {
        let path = format!("/accounts/{account}/d1/database/{database}/query");
        let outcomes: Vec<Queried> = self.ask_with("POST", &path, &serde_json::json!({ "sql": sql }))?;
        if outcomes.iter().any(|outcome| !outcome.success) {
            return Err(Failure::unreadable("the database refused a statement of the schema"));
        }
        Ok(outcomes)
    }

    /// Upload the Worker, with the database bound to it and the write token set on it in the same breath.
    ///
    /// **The secret travels as a binding rather than as a second call.** An upload replaces the bindings
    /// it does not carry, so a token set separately afterwards is one an ordinary redeploy would quietly
    /// drop — and a Worker with no write token takes no writes.
    pub(crate) fn deploy(
        &self,
        account: &str,
        script: &str,
        source: &str,
        database: &str,
        write_token: &str,
    ) -> Reached<()> {
        let metadata = serde_json::json!({
            "main_module": SCRIPT_ENTRY,
            "compatibility_date": super::COMPATIBILITY_DATE,
            "bindings": [
                { "type": "d1", "name": super::DATABASE_BINDING, "id": database },
                { "type": "secret_text", "name": super::WRITE_TOKEN_BINDING, "text": write_token },
            ],
            "observability": { "enabled": true },
        })
        .to_string();

        let boundary = boundary();
        let mut body = Vec::new();
        part(&mut body, &boundary, "metadata", "metadata.json", "application/json", metadata.as_bytes());
        part(
            &mut body,
            &boundary,
            SCRIPT_ENTRY,
            SCRIPT_ENTRY,
            "application/javascript+module",
            source.as_bytes(),
        );
        body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());

        let path = format!("/accounts/{account}/workers/scripts/{script}");
        let _: serde_json::Value = self.ask(
            "PUT",
            &path,
            Some(&format!("multipart/form-data; boundary={boundary}")),
            Some(body),
        )?;
        Ok(())
    }

    /// Read the account's workers.dev name, which is the middle of every URL a Worker in it answers on.
    ///
    /// `None` is the account not having one yet — the one refusal in the whole of setup the user has to
    /// answer themselves, since the name is theirs to choose and is taken account-wide.
    pub(crate) fn the_subdomain(&self, account: &str) -> Reached<Option<String>> {
        #[derive(serde::Deserialize)]
        struct Said {
            #[serde(default)]
            subdomain: String,
        }
        let path = format!("/accounts/{account}/workers/subdomain");
        match self.ask::<Said>("GET", &path, None, None) {
            Ok(said) if said.subdomain.is_empty() => Ok(None),
            Ok(said) => Ok(Some(said.subdomain)),
            Err(Failure::Refused { code, .. }) if code == CODE_NO_SUBDOMAIN => Ok(None),
            Err(other) => Err(other),
        }
    }

    /// Give the Worker its workers.dev URL. An uploaded Worker sits there answering nothing until this is
    /// done, so it is part of standing the route up rather than something the user is left to find in the
    /// dashboard.
    pub(crate) fn answer_on_the_subdomain(&self, account: &str, script: &str) -> Reached<()> {
        let path = format!("/accounts/{account}/workers/scripts/{script}/subdomain");
        let _: serde_json::Value = self.ask_with(
            "POST",
            &path,
            &serde_json::json!({ "enabled": true, "previews_enabled": false }),
        )?;
        Ok(())
    }
}

/// The one envelope Cloudflare answers every call in, refusals included.
#[derive(serde::Deserialize)]
struct Envelope {
    success: bool,
    #[serde(default)]
    errors: Vec<Refusal>,
    #[serde(default)]
    result: serde_json::Value,
}

#[derive(Clone, serde::Deserialize)]
struct Refusal {
    #[serde(default)]
    code: i64,
    #[serde(default)]
    message: String,
}

/// Percent-escape what goes in a query. The one value that does is this build's own name for the
/// database, so there is nothing here to escape — which is exactly why doing it anyway costs nothing.
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

/// A boundary no part can contain, drawn rather than spelled. What is uploaded is this build's own bytes,
/// so a fixed one would do — and a drawn one is what keeps that from being something to check again every
/// time the script changes.
fn boundary() -> String {
    let mut raw = [0u8; 16];
    getrandom::fill(&mut raw).expect("failed to draw OS randomness");
    let mut out = String::from("amenbo");
    for byte in raw {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Put one part into the upload with its own content type. The parts are named and typed rather than just
/// written, because the API reads the metadata part as JSON and the module part as a module, and tells
/// them apart by exactly this.
fn part(
    body: &mut Vec<u8>,
    boundary: &str,
    name: &str,
    filename: &str,
    content_type: &str,
    content: &[u8],
) {
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"; filename=\"{filename}\"\r\nContent-Type: {content_type}\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(content);
    body.extend_from_slice(b"\r\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a part carries is what was handed over, framed the way the API reads it: a header naming the
    /// part and its type, then the bytes, then the line that ends it.
    #[test]
    fn a_part_frames_the_bytes_it_was_handed() {
        let mut body = Vec::new();
        part(&mut body, "B", "metadata", "metadata.json", "application/json", b"{}");
        let written = String::from_utf8(body).expect("the frame is text around the bytes");
        assert!(written.starts_with("--B\r\n"), "{written}");
        assert!(written.contains("name=\"metadata\"; filename=\"metadata.json\""), "{written}");
        assert!(written.contains("Content-Type: application/json"), "{written}");
        assert!(written.ends_with("\r\n{}\r\n"), "{written}");
    }

    /// Two uploads do not share a boundary, so nothing rests on one that was chosen once.
    #[test]
    fn a_boundary_is_drawn_each_time() {
        assert_ne!(boundary(), boundary());
        assert!(boundary().starts_with("amenbo"));
    }

    /// The one value that reaches a query is escaped, whatever it is.
    #[test]
    fn a_query_value_is_escaped() {
        assert_eq!(escaped("amenbo-viewer"), "amenbo-viewer");
        assert_eq!(escaped("a b&c=d"), "a%20b%26c%3Dd");
    }

    /// A refusal reads as Cloudflare's own sentence and code. What it must not read as is a status with
    /// nothing behind it — the envelope is where the reason is.
    #[test]
    fn a_refusal_carries_the_reason_cloudflare_gave() {
        let said: Error = Failure::Refused {
            call: "/accounts".into(),
            code: 10000,
            message: "Authentication error".into(),
        }
        .into();
        assert!(
            said.message_en().ends_with("Cloudflare refused /accounts: Authentication error (10000)"),
            "{}",
            said.message_en()
        );
    }
}
