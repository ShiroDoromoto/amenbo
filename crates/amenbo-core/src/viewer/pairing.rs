//! **Letting a phone read, and stopping it** (`AMB-D-884`, ported from the `viewer` plugin the retreat
//! retired).
//!
//! Pairing is the one piece of setting a phone up, and the whole of it happens off the network: this
//! machine draws a code, the phone's camera reads it, and nothing in between is asked to carry the key.
//! **Route the key through the Worker and the Worker can read what it is storing**, and the sealing stops
//! meaning anything.
//!
//! **There is one read code, and not one per phone.** The Worker compares a hash and never learns which
//! phone offered it, so a code per phone would be a name kept on this side and nothing at all in the
//! server. Two things follow, and both are for a screen to say out loud rather than for a reader to
//! discover:
//!
//! - **Adding a second phone is the same press as the first.** The code that is drawn is drawn again.
//! - **Unpairing takes every phone off at once.** There is nothing to name and so nothing to single out.
//!
//! **Nothing about who may read is written down here.** The server holds the code, so the server is what
//! answers whether a phone can read. A list on this side could only say what this machine believes it
//! has issued — and a server stood up anew underneath it makes every row of that list name a phone that
//! reads nothing, which nothing here can find out.

use chrono::Utc;
use sha2::{Digest as _, Sha256};

use crate::error::Result;
use crate::store::Store;

use super::send::{Server, SPEC_V};

/// The door the read code is kept behind. It takes the write token, which is all this device holds.
const TOKENS: &str = "/tokens";

/// What the code carries, and nothing else is on it.
///
/// **The keys are one letter each.** Not for the code's capacity, which is ample, but for the camera:
/// fewer modules is a read that catches sooner, and a phone is being held up to a screen by hand.
#[derive(Debug, serde::Serialize)]
struct OnTheCode<'a> {
    v: i64,
    url: &'a str,
    t: &'a str,
    k: &'a str,
}

/// What issuing a code left behind: what goes on the code, and when the server says it was issued.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Issued {
    /// What the phone is to read — the whole of the code's content, ready to be drawn
    /// ([`super::code::in_blocks`]) or handed to whatever draws it.
    ///
    /// **It carries the key**, so wherever it is put is somewhere the key has been: a terminal's
    /// scrollback is one of those, and that is worth saying where it is drawn.
    pub carried: String,
    /// When the server wrote the code down, in its own words (RFC 3339).
    pub issued_at: String,
}

/// Whether a phone may read at all, and since when.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize)]
pub struct Reading {
    /// Whether the server is holding a read code.
    #[serde(default)]
    pub paired: bool,
    /// When that code was issued. Absent when none is held.
    #[serde(default)]
    pub issued_at: Option<String>,
}

/// Where the phone's half of this is got.
///
/// **Neither address carries a country**, both being the store's own id form: one that named a country
/// would send everybody else's phone to a page that is not for their store.
pub struct TheApp {
    /// The kind of phone, which is what tells the two apart wherever they are drawn side by side.
    ///
    /// **It is a brand and is never translated.** It is the word a reader matches against the phone in
    /// their hand.
    pub phone: &'static str,
    /// Where that phone installs it from.
    pub link: &'static str,
}

/// The two of them, in the order they are drawn.
pub const THE_APP: &[TheApp] = &[
    TheApp { phone: "iPhone", link: "https://apps.apple.com/app/id6800196224" },
    TheApp { phone: "Android", link: "https://play.google.com/store/apps/details?id=work.amenbo.viewer" },
];

/// Draw a new read code, and tell the server the hash of it.
///
/// **The code itself is sent nowhere.** The server is told its hash, the phone reads the value off the
/// screen, and this side forgets it the moment the run ends — which is what makes the screen and the
/// camera the whole of the path the key travels.
///
/// **It replaces whatever the server was holding**, so pressing it a second time is not refused: a new
/// code is drawn and the phone that had the one before stops reading. That is what makes pairing a
/// second phone one press, and re-pairing after a lost phone one press as well.
///
/// `None` where no server has been stood up on this device, which is a state to explain rather than a
/// failure to raise.
pub fn issue(store: &Store) -> Result<Option<Issued>> {
    let Some(server) = Server::of_device(store)? else {
        return Ok(None);
    };
    // The key is read from where setup left it rather than off the server, which holds neither it nor
    // anything it opens. A device with an address and a token but no key is not one setup produces —
    // `Server::of_device` reads all three together — so there is nothing here to explain away.
    let key = store
        .secret_value(None, crate::model::SecretArea::Viewer, None, super::ENCRYPTION_KEY)?
        .unwrap_or_default();
    let code = super::drawn();
    let told = serde_json::json!({ "hash": hash_of(&code) }).to_string();

    #[derive(serde::Deserialize)]
    struct Said {
        #[serde(default)]
        issued_at: String,
    }
    let answered: Said = read(&server, "PUT", Some(told.into_bytes()))?;

    let carried = serde_json::to_string(&OnTheCode {
        v: SPEC_V,
        url: server.at(),
        t: &code,
        k: &key,
    })?;
    Ok(Some(Issued { carried, issued_at: answered.issued_at }))
}

/// Ask the server whether a phone may read, and since when.
///
/// **It asks rather than answering from here.** The one thing worth knowing is whether the code that was
/// issued is still the code the server holds, and that is a question only the server can answer.
///
/// `None` where no server has been stood up on this device.
pub fn who_may_read(store: &Store) -> Result<Option<Reading>> {
    let Some(server) = Server::of_device(store)? else {
        return Ok(None);
    };
    Ok(Some(read(&server, "GET", None)?))
}

/// Take the read code away, so whatever was holding it stops reading.
///
/// **There is one code, so this takes every phone off at once.** Nothing is named because there is
/// nothing to name — which is also why there is nothing to mistype.
///
/// `Some(false)` where the server had no code to take away. **That is not a refusal**: asking for the
/// state a server is already in is answered rather than turned down. `None` where there is no server.
pub fn cut_off(store: &Store) -> Result<Option<bool>> {
    let Some(server) = Server::of_device(store)? else {
        return Ok(None);
    };

    #[derive(serde::Deserialize)]
    struct Said {
        #[serde(default)]
        cut: bool,
    }
    let answered: Said = read(&server, "DELETE", None)?;
    Ok(Some(answered.cut))
}

/// Ask the read code's door, and read what it answered.
fn read<T: serde::de::DeserializeOwned>(
    server: &Server,
    method: &str,
    body: Option<Vec<u8>>,
) -> Result<T> {
    let answered = server.ask(method, TOKENS, body).map_err(|it| it.in_words(Utc::now()))?;
    serde_json::from_slice(&answered).map_err(|err| {
        crate::error::Error::invalid(format!(
            "{TOKENS} answered with something this build cannot read: {err}"
        ))
    })
}

/// How a read code is written down: SHA-256, lower-case hex, which is what the server compares an
/// offered code against.
///
/// **The code itself is never written anywhere**, here or there. What the server keeps is this, and a
/// hash is no use to whoever reads it out of that database.
fn hash_of(code: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(code.as_bytes());
    hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SecretArea;
    use amenbo_static_host::{Reply, StaticHost};

    /// A store on a scratch base, so what setup leaves behind lands somewhere.
    fn store_at(tag: &str) -> Store {
        let dir = amenbo_scratch::scratch(&format!("viewer-pairing-{tag}"));
        std::fs::create_dir_all(&dir).unwrap();
        Store::open_at(crate::config::Paths::at(dir)).unwrap()
    }

    /// The key a test device seals with, written the way setup writes one.
    const THE_KEY: &str = "BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc";

    /// Leave behind what setup leaves behind, pointed at a stand-in.
    fn set_up(store: &mut Store, host: &StaticHost) {
        for (field, value) in [
            (super::super::WORKER_URL, host.url("")),
            (super::super::AUTH_TOKEN, "write-token".to_string()),
            (super::super::ENCRYPTION_KEY, THE_KEY.to_string()),
        ] {
            store.set_secret(None, SecretArea::Viewer, None, field, Some(&value)).unwrap();
        }
    }

    /// The requests the door was sent, oldest first.
    fn asked(host: &StaticHost) -> Vec<(String, String)> {
        host.heard()
            .iter()
            .filter(|heard| heard.target == TOKENS)
            .map(|heard| (heard.method.clone(), String::from_utf8_lossy(&heard.body).to_string()))
            .collect()
    }

    /// **A device nobody has stood a server up on answers nothing rather than failing.** There is a
    /// screen behind every one of these, and "you have not set this up yet" is a state to draw, not an
    /// error to raise.
    #[test]
    fn a_device_with_no_server_answers_none() {
        let store = store_at("no-server");
        assert_eq!(issue(&store).unwrap(), None);
        assert_eq!(who_may_read(&store).unwrap(), None);
        assert_eq!(cut_off(&store).unwrap(), None);
    }

    /// **The code goes on the screen and its hash goes to the server.** What the phone reads carries the
    /// address, the code and the key; what the server is told is 64 hex characters and nothing else.
    #[test]
    fn the_server_is_told_the_hash_and_the_phone_is_shown_the_code() {
        let mut store = store_at("issue");
        let host = StaticHost::serve(Vec::<(String, String)>::new());
        set_up(&mut store, &host);
        host.set_reply(TOKENS, Reply::ok(r#"{"issued_at":"2026-09-14T10:00:00Z"}"#));

        let issued = issue(&store).unwrap().unwrap();
        assert_eq!(issued.issued_at, "2026-09-14T10:00:00Z");

        let on_the_code: serde_json::Value = serde_json::from_str(&issued.carried).unwrap();
        assert_eq!(on_the_code["v"], SPEC_V);
        assert_eq!(on_the_code["url"], host.url(""));
        assert_eq!(on_the_code["k"], THE_KEY, "the key travels on the code and nowhere else");
        let code = on_the_code["t"].as_str().unwrap();
        assert!(!code.is_empty());

        let sent = asked(&host);
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, "PUT");
        let told: serde_json::Value = serde_json::from_str(&sent[0].1).unwrap();
        let hash = told["hash"].as_str().unwrap();
        assert_eq!(hash, hash_of(code), "what the server is told is the hash of what was drawn");
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        assert!(!sent[0].1.contains(code), "the code itself is sent nowhere");
    }

    /// Two presses draw two codes. There is one code on the server, so the second replaces the first —
    /// which is what makes adding a phone, and replacing a lost one, the same single press.
    #[test]
    fn pressing_it_again_draws_a_new_code() {
        let mut store = store_at("again");
        let host = StaticHost::serve(Vec::<(String, String)>::new());
        set_up(&mut store, &host);
        host.set_reply(TOKENS, Reply::ok(r#"{"issued_at":"2026-09-14T10:00:00Z"}"#));

        let first = issue(&store).unwrap().unwrap();
        let second = issue(&store).unwrap().unwrap();
        assert_ne!(first.carried, second.carried);
        assert_eq!(asked(&host).len(), 2, "and the server is told each time");
    }

    /// Who may read is the server's answer, read back as it stands.
    #[test]
    fn whether_a_phone_may_read_is_asked_of_the_server() {
        let mut store = store_at("who");
        let host = StaticHost::serve(Vec::<(String, String)>::new());
        set_up(&mut store, &host);
        host.set_replies(TOKENS, [
            Reply::ok(r#"{"paired":true,"issued_at":"2026-09-14T10:00:00Z"}"#),
            Reply::ok(r#"{"paired":false,"issued_at":null}"#),
        ]);

        assert_eq!(who_may_read(&store).unwrap().unwrap(), Reading {
            paired: true,
            issued_at: Some("2026-09-14T10:00:00Z".into()),
        });
        assert_eq!(who_may_read(&store).unwrap().unwrap(), Reading::default());
        assert!(asked(&host).iter().all(|(method, _)| method == "GET"));
    }

    /// Taking the code away says whether there was one. **Having none is not a refusal** — pressing it on
    /// a server nobody is paired with asks for the state it is already in.
    #[test]
    fn taking_the_code_away_says_whether_there_was_one() {
        let mut store = store_at("cut");
        let host = StaticHost::serve(Vec::<(String, String)>::new());
        set_up(&mut store, &host);
        host.set_replies(TOKENS, [Reply::ok(r#"{"cut":true}"#), Reply::ok(r#"{"cut":false}"#)]);

        assert_eq!(cut_off(&store).unwrap(), Some(true));
        assert_eq!(cut_off(&store).unwrap(), Some(false));
        assert!(asked(&host).iter().all(|(method, _)| method == "DELETE"));
    }

    /// A door that turns this device away says so in words, and the sentence names the door and what it
    /// answered — the same reading every other door of that server gets.
    #[test]
    fn a_door_that_turns_this_device_away_says_so() {
        let mut store = store_at("refused");
        let host = StaticHost::serve(Vec::<(String, String)>::new());
        set_up(&mut store, &host);
        host.set_reply(TOKENS, Reply::status(403, r#"{"error":"this endpoint takes the write token"}"#));

        let refused = who_may_read(&store).unwrap_err().to_string();
        assert!(refused.contains(TOKENS) && refused.contains("403"), "{refused}");
        assert!(!refused.contains("write-token"), "no sentence carries the token — {refused}");
    }

    /// What goes on a code is small enough for a camera to catch, and what it says is what a phone needs
    /// to read the backlog: where, with what code, under which key.
    #[test]
    fn what_is_on_the_code_is_the_four_things_a_phone_needs() {
        let mut store = store_at("shape");
        let host = StaticHost::serve(Vec::<(String, String)>::new());
        set_up(&mut store, &host);
        host.set_reply(TOKENS, Reply::ok(r#"{"issued_at":"2026-09-14T10:00:00Z"}"#));

        let issued = issue(&store).unwrap().unwrap();
        let on_the_code: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(&issued.carried).unwrap();
        let mut named: Vec<&str> = on_the_code.keys().map(String::as_str).collect();
        named.sort_unstable();
        assert_eq!(named, ["k", "t", "url", "v"], "four fields, and nothing beside them");
        // It has to fit on a code, and it is drawn on one here rather than taken on trust.
        assert!(super::super::code::in_blocks(&issued.carried).is_ok());
    }

    /// Both addresses are the store's own id form, and neither names a country — one that did would send
    /// everybody else's phone to a page that is not for their store.
    #[test]
    fn the_app_is_named_once_per_kind_of_phone() {
        assert_eq!(THE_APP.len(), 2);
        for app in THE_APP {
            assert!(app.link.starts_with("https://"), "{}", app.link);
            assert!(!app.phone.is_empty());
        }
        let countries = ["/us/", "/jp/", "/gb/"];
        assert!(
            THE_APP.iter().all(|app| !countries.iter().any(|c| app.link.contains(c))),
            "no address names a country"
        );
    }
}
