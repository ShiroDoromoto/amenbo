//! **The SMTP conversation** — how one message is handed to the relay a mail target names
//! (`AMB-D-469`).
//!
//! Every provider speaks SMTP the same way, so the account a person already has is the whole setup and
//! nobody sits between Amenbo and the mailbox it reports to. What is written here is the conversation
//! itself — connect, greet, upgrade, authenticate, name the recipients, hand over the body, close —
//! because the thing the Go plugin this was ported from leaned on (`net/smtp`) has no counterpart in
//! Rust's standard library, and the one crate that would stand in for it is ten crates in a tree that
//! already carries the TLS half.
//!
//! **The message arrives finished.** Headers, body and the line endings between them are
//! [`crate::notify_mail`]'s; what is added here is the envelope — who it is from, who it is for — and
//! the transport under it.
//!
//! **Nothing is retried.** A message this fails to deliver is one the caller still holds, so trying
//! again is the caller's decision to make with everything else it is holding rather than this
//! module's to take behind its back.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::Engine as _;

use crate::error::{Error, Result};

/// How long one message gets, from the first packet to the last.
///
/// A send is a conversation rather than a single round trip, so it is given far longer than one. The
/// limit is here for the server that accepts a connection and then says nothing: without it that send
/// never ends, and the run that started it never comes back.
pub const SEND_TIMEOUT: Duration = Duration::from_secs(60);

/// The port that is encrypted from the first byte. The other submission ports (587, 25) open in the
/// clear and are upgraded during the conversation, and that split is old and settled among providers —
/// so the port a person filled in already says which one they meant, and asking a second time in a
/// setting of its own would only let them contradict themselves.
pub const IMPLICIT_TLS_PORT: u16 = 465;

/// Where a message goes when nobody filled the port in. 587 is the submission port, which is what a
/// provider expects an authenticated client to use and what nearly every account is reached on — so the
/// setting exists for the accounts that are not, rather than as a question everyone has to answer.
pub const DEFAULT_PORT: u16 = 587;

/// The name this client greets the server with. It is the client's own name and no part of the
/// message: a submission server authenticates the account and pays the greeting no mind, and a name
/// read off this machine would put the reader's hostname on the wire for nothing.
const GREETING_NAME: &str = "localhost";

/// One relay, and how to be let in at it. Assembled by [`crate::notify_mail`] from the target's row and
/// the secret beside it; by the time it is here every field has been settled, including the ones a
/// person left for the account to answer for.
#[derive(Clone, Debug)]
pub struct Relay {
    pub host: String,
    pub port: u16,
    /// The account to authenticate as. **Empty is the whole of "do not authenticate"** (`AMB-D-476`):
    /// a relay that asks for none — one on the same machine, one inside a network — is used by leaving
    /// it empty, and no `AUTH` is sent at all.
    pub user: String,
    /// The password that goes with the account. Never written into an error (see [`without_secret`]).
    pub password: String,
}

/// One envelope: who the message is from, and who the relay is asked to carry it to.
///
/// These are the addresses the *relay* is told about, which is not the same list as the `From:` and
/// `To:` a reader sees — the headers are text in the message and these decide where it goes.
#[derive(Clone, Debug)]
pub struct Envelope {
    pub from: String,
    /// Every recipient, in the order they were written. Never empty by the time it is here.
    pub to: Vec<String>,
}

/// Hand one finished message to the relay.
///
/// What comes back is nothing at all or the first thing that went wrong, with the password taken out
/// of it — a failure ends up in a log, and a server that quotes the credential it just rejected must
/// not be repeated word for word.
pub fn send(relay: &Relay, envelope: &Envelope, message: &str) -> Result<()> {
    let deadline = Instant::now() + SEND_TIMEOUT;
    without_secret(carry(relay, envelope, Some(message), deadline), &relay.password)
}

/// Hold the same conversation as [`send`] and stop before the message, so a person filling the form in
/// is told whether the relay is reachable and the account accepted without a message being posted to
/// prove it.
///
/// It goes as far as `AUTH` — which is where nearly every real failure is (a wrong host, a port
/// nothing listens on, a certificate that does not check out, an app password that was pasted with its
/// spaces in) — and then says `QUIT`.
pub fn probe(relay: &Relay) -> Result<()> {
    let deadline = Instant::now() + SEND_TIMEOUT;
    let envelope = Envelope { from: String::new(), to: Vec::new() };
    without_secret(carry(relay, &envelope, None, deadline), &relay.password)
}

/// The conversation both doors hold. `message` of `None` is the probe: everything up to and including
/// the account, and then the close.
fn carry(relay: &Relay, envelope: &Envelope, message: Option<&str>, deadline: Instant) -> Result<()> {
    let tcp = dial(&relay.host, relay.port, deadline)?;
    let encrypted_from_the_start = relay.port == IMPLICIT_TLS_PORT;
    let mut talk = if encrypted_from_the_start {
        Session::encrypted(tcp, &relay.host)?
    } else {
        Session::clear(tcp)
    };

    talk.expect(220, "the server did not greet us")?;
    let offered = talk.ehlo()?;

    if !talk.encrypted && offered.iter().any(|line| line.eq_ignore_ascii_case("starttls")) {
        talk.command("STARTTLS")?;
        talk.expect(220, "the connection could not be encrypted")?;
        talk = talk.upgrade(&relay.host)?;
        // The greeting is said again on the encrypted connection because the protocol asks for it:
        // everything the server offered before the upgrade is forgotten, so the conversation starts
        // over on the connection nobody can read.
        let _ = talk.ehlo()?;
    }

    if !relay.user.is_empty() {
        // **The clear connection is where this stops.** Go's standard library refused to authenticate
        // over one, and `AMB-D-476` was written on that guarantee; carrying the conversation ourselves
        // means carrying the guarantee with it, so the password is never written to a connection
        // anybody could read it off.
        //
        // A relay that is encrypted and wants no account is left to say so itself: it answers `AUTH`
        // with a refusal that names the command, which is a sentence as plain as one written here and
        // one fewer branch to be wrong about.
        if !talk.encrypted {
            return Err(Error::Io(std::io::Error::other(
                "the connection could not be encrypted, so the account was not sent: this relay offers no STARTTLS on this port",
            )));
        }
        talk.auth_plain(&relay.user, &relay.password)?;
    }

    let Some(message) = message else {
        let _ = talk.command("QUIT");
        return Ok(());
    };

    talk.command(&format!("MAIL FROM:<{}>", envelope.from))?;
    talk.expect(250, &format!("the server would not take the message from {}", envelope.from))?;
    for to in &envelope.to {
        talk.command(&format!("RCPT TO:<{to}>"))?;
        // 251 is "not local, will forward" — an acceptance, and one a relay answers with often enough
        // that reading it as a refusal would drop messages that went out perfectly well.
        talk.expect_one_of(&[250, 251], &format!("the server would not take the message for {to}"))?;
    }
    talk.command("DATA")?;
    talk.expect(354, "the server would not take the body")?;
    talk.write_body(message)?;
    // The dot is what ends the body, so the answer to it is the server's verdict on the message and
    // not a detail of finishing a write.
    talk.expect(250, "the server did not accept the message")?;
    talk.command("QUIT")?;
    Ok(())
}

/// Open the connection, inside what is left of the deadline.
///
/// The address is resolved first because a name that does not resolve is the commonest way a filled-in
/// host is wrong, and `connect_timeout` takes one socket address rather than a name.
fn dial(host: &str, port: u16, deadline: Instant) -> Result<TcpStream> {
    let left = remaining(deadline)?;
    let mut last = None;
    let addrs = (host, port)
        .to_socket_addrs()
        .map_err(|e| Error::Io(std::io::Error::other(format!("{host} could not be looked up: {e}"))))?;
    for addr in addrs {
        match TcpStream::connect_timeout(&addr, left) {
            Ok(tcp) => {
                tcp.set_read_timeout(Some(left))?;
                tcp.set_write_timeout(Some(left))?;
                return Ok(tcp);
            }
            Err(e) => last = Some(e),
        }
    }
    Err(Error::Io(std::io::Error::other(match last {
        Some(e) => format!("no message was sent: {host}:{port} could not be reached: {e}"),
        None => format!("no message was sent: {host}:{port} resolved to no address"),
    })))
}

/// What is left of the deadline, or the failure that says there is none.
fn remaining(deadline: Instant) -> Result<Duration> {
    let left = deadline.saturating_duration_since(Instant::now());
    if left.is_zero() {
        return Err(Error::Io(std::io::Error::other(format!(
            "no message was sent: the send ran past {} seconds",
            SEND_TIMEOUT.as_secs()
        ))));
    }
    Ok(left)
}

/// The connection under the conversation — in the clear, or wrapped.
///
/// It is one type rather than a generic parameter because the two shapes are not chosen by the caller:
/// a conversation starts in the clear and becomes encrypted partway through, and a value that changes
/// which of the two it is halfway is a value, not a type parameter.
enum Wire {
    Clear(TcpStream),
    Tls(Box<rustls::StreamOwned<rustls::ClientConnection, TcpStream>>),
}

impl Read for Wire {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Wire::Clear(tcp) => tcp.read(buf),
            Wire::Tls(tls) => tls.read(buf),
        }
    }
}

impl Write for Wire {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Wire::Clear(tcp) => tcp.write(buf),
            Wire::Tls(tls) => tls.write(buf),
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Wire::Clear(tcp) => tcp.flush(),
            Wire::Tls(tls) => tls.flush(),
        }
    }
}

/// One conversation: the wire, a reader over it, and whether anything said on it can be read by a third
/// party.
struct Session {
    wire: BufReader<Wire>,
    encrypted: bool,
}

impl Session {
    fn clear(tcp: TcpStream) -> Session {
        Session { wire: BufReader::new(Wire::Clear(tcp)), encrypted: false }
    }

    fn encrypted(tcp: TcpStream, host: &str) -> Result<Session> {
        Ok(Session { wire: BufReader::new(wrap(tcp, host)?), encrypted: true })
    }

    /// Take the connection out from under the conversation and put it back encrypted. The reader is
    /// dropped with the clear wire: anything it had buffered was said before the upgrade, and reading
    /// it afterwards is how a client is talked into trusting what an eavesdropper wrote.
    fn upgrade(self, host: &str) -> Result<Session> {
        let tcp = match self.wire.into_inner() {
            Wire::Clear(tcp) => tcp,
            // Unreachable: nothing asks for the upgrade twice, and the caller checks `encrypted` first.
            Wire::Tls(tls) => return Ok(Session { wire: BufReader::new(Wire::Tls(tls)), encrypted: true }),
        };
        Session::encrypted(tcp, host)
    }

    /// Say `EHLO` and answer with the extension lines, the leading keyword of each left as the server
    /// spelled it.
    ///
    /// A server too old for `EHLO` is answered with `HELO`, which offers nothing — no upgrade and no
    /// account — and is left in the clear. That is the relay inside a network that wants neither, and
    /// the one this still reaches.
    fn ehlo(&mut self) -> Result<Vec<String>> {
        self.command(&format!("EHLO {GREETING_NAME}"))?;
        match self.reply()? {
            (250, lines) => Ok(lines.into_iter().skip(1).collect()),
            _ => {
                self.command(&format!("HELO {GREETING_NAME}"))?;
                self.expect(250, "the server would not be greeted")?;
                Ok(Vec::new())
            }
        }
    }

    /// `AUTH PLAIN` and nothing else (`AMB-D-476`). The credential is one base64 run of
    /// `\0account\0password`, which is the whole of the mechanism.
    fn auth_plain(&mut self, user: &str, password: &str) -> Result<()> {
        let mut raw = Vec::new();
        raw.push(0u8);
        raw.extend_from_slice(user.as_bytes());
        raw.push(0u8);
        raw.extend_from_slice(password.as_bytes());
        let encoded = base64::engine::general_purpose::STANDARD.encode(raw);
        self.command(&format!("AUTH PLAIN {encoded}"))?;
        self.expect(235, "the server would not accept the account")?;
        Ok(())
    }

    /// Write the body, dot-stuffed, and end it.
    ///
    /// A line of the message that begins with a dot would otherwise be read as the end of the body,
    /// and everything after it as commands — so every such line is written with a second dot, which
    /// the server takes back off. It is the one place a message's own text can reach the protocol.
    fn write_body(&mut self, message: &str) -> Result<()> {
        let wire = self.wire.get_mut();
        // One trailing break is the end of the last line and not a line of its own; leaving it in
        // writes a blank line onto the end of every message.
        let message = message.strip_suffix("\r\n").unwrap_or(message);
        for line in message.split("\r\n") {
            if line.starts_with('.') {
                wire.write_all(b".")?;
            }
            wire.write_all(line.as_bytes())?;
            wire.write_all(b"\r\n")?;
        }
        wire.write_all(b".\r\n")?;
        wire.flush()?;
        Ok(())
    }

    fn command(&mut self, line: &str) -> Result<()> {
        let wire = self.wire.get_mut();
        wire.write_all(line.as_bytes())?;
        wire.write_all(b"\r\n")?;
        wire.flush()?;
        Ok(())
    }

    /// Read one reply: its code, and every line of it.
    ///
    /// A reply may run to several lines, and which one is the last is said by the character after the
    /// code — `-` on a line that continues, a space on the one that ends it. Reading only the first
    /// line leaves the rest of them to be read as the answer to the next command, which puts the whole
    /// conversation one step out and is not noticed until something refuses for no reason.
    fn reply(&mut self) -> Result<(u16, Vec<String>)> {
        let mut lines = Vec::new();
        loop {
            let mut line = String::new();
            let read = self.wire.read_line(&mut line)?;
            if read == 0 {
                return Err(Error::Io(std::io::Error::other(
                    "the server closed the connection without answering",
                )));
            }
            let line = line.trim_end_matches(['\r', '\n']);
            if line.len() < 3 {
                return Err(Error::Io(std::io::Error::other(format!(
                    "the server answered with something that is not a reply: {line:?}"
                ))));
            }
            let code: u16 = line[..3].parse().map_err(|_| {
                Error::Io(std::io::Error::other(format!(
                    "the server answered with something that is not a reply: {line:?}"
                )))
            })?;
            lines.push(line[3..].trim_start_matches(['-', ' ']).to_string());
            if !line[3..].starts_with('-') {
                return Ok((code, lines));
            }
        }
    }

    fn expect(&mut self, want: u16, said: &str) -> Result<()> {
        self.expect_one_of(&[want], said)
    }

    fn expect_one_of(&mut self, want: &[u16], said: &str) -> Result<()> {
        let (code, lines) = self.reply()?;
        if want.contains(&code) {
            return Ok(());
        }
        Err(Error::Io(std::io::Error::other(format!("{said}: {code} {}", lines.join(" ")))))
    }
}

/// Put TLS over an open connection, checking the certificate against the roots the updater already
/// trusts.
fn wrap(tcp: TcpStream, host: &str) -> Result<Wire> {
    let roots =
        rustls::RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
    // The provider is named rather than taken from the process default: the default is whichever one
    // the build happens to have compiled, and a second one arriving with some later dependency turns a
    // silent choice into a panic at the first send.
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .map_err(|e| Error::Io(std::io::Error::other(format!("TLS could not be set up: {e}"))))?
        .with_root_certificates(roots)
        .with_no_client_auth();
    let name = rustls::pki_types::ServerName::try_from(host.to_string()).map_err(|_| {
        Error::Io(std::io::Error::other(format!("{host} is not a name a certificate can be checked against")))
    })?;
    let conn = rustls::ClientConnection::new(Arc::new(config), name)
        .map_err(|e| Error::Io(std::io::Error::other(format!("the connection could not be encrypted: {e}"))))?;
    Ok(Wire::Tls(Box::new(rustls::StreamOwned::new(conn, tcp))))
}

/// Keep the password out of what is written down.
///
/// A failure ends up in a log, and a log is read by whoever is helping — so a server that quotes the
/// credential it just rejected must not be repeated word for word.
fn without_secret(result: Result<()>, secret: &str) -> Result<()> {
    let Err(e) = result else { return Ok(()) };
    if secret.is_empty() {
        return Err(e);
    }
    let text = e.to_string();
    if !text.contains(secret) {
        return Err(e);
    }
    Err(Error::Io(std::io::Error::other(text.replace(secret, "[the password]"))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufWriter;
    use std::net::TcpListener;
    use std::sync::mpsc;

    /// A relay that answers from a script and writes down everything it was told.
    ///
    /// It is here rather than as a fake in front of the conversation because what these tests are
    /// about is the conversation: which line goes out when, what is read as one reply and what as two,
    /// and what never leaves at all. A stand-in for the socket would be a second implementation of the
    /// thing under test.
    struct Relayed {
        port: u16,
        said: mpsc::Receiver<Vec<String>>,
    }

    /// Serve one connection from `script`: the greeting first, then one entry per command. A `354`
    /// puts the server into the body, which it reads until the lone dot before answering again.
    fn relay_saying(script: &[&str]) -> Relayed {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("a port to answer on");
        let port = listener.local_addr().expect("the port").port();
        let script: Vec<String> = script.iter().map(|line| (*line).to_string()).collect();
        let (tell, said) = mpsc::channel();
        std::thread::spawn(move || {
            let (stream, _) = listener.accept().expect("a caller");
            let mut reader = BufReader::new(stream.try_clone().expect("a second handle"));
            let mut writer = BufWriter::new(stream);
            let mut heard = Vec::new();
            let mut script = script.into_iter();
            let mut in_body = false;
            let greeting = script.next().unwrap_or_default();
            let _ = writeln!(writer, "{greeting}\r");
            let _ = writer.flush();
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 {
                    break;
                }
                let line = line.trim_end_matches(['\r', '\n']).to_string();
                if in_body {
                    if line == "." {
                        heard.push("<body ends>".to_string());
                    } else {
                        heard.push(format!("| {line}"));
                        continue;
                    }
                } else {
                    heard.push(line.clone());
                    if line == "QUIT" {
                        let _ = writeln!(writer, "221 bye\r");
                        let _ = writer.flush();
                        break;
                    }
                }
                let Some(reply) = script.next() else { break };
                in_body = reply.starts_with("354");
                let _ = writeln!(writer, "{reply}\r");
                let _ = writer.flush();
            }
            let _ = tell.send(heard);
        });
        Relayed { port, said }
    }

    fn relay(port: u16, user: &str) -> Relay {
        Relay {
            host: "127.0.0.1".into(),
            port,
            user: user.into(),
            password: "app-password".into(),
        }
    }

    fn to(addresses: &[&str]) -> Envelope {
        Envelope {
            from: "someone@example.com".into(),
            to: addresses.iter().map(|a| (*a).to_string()).collect(),
        }
    }

    /// The whole conversation, in order — and the extension list said over several lines is read as
    /// one reply rather than left for the next command to trip over.
    #[test]
    fn one_message_walks_the_whole_conversation() {
        let server = relay_saying(&[
            "220 relay ready",
            "250-relay at your service\r\n250-SIZE 35882577\r\n250 8BITMIME",
            "250 sender ok",
            "250 first ok",
            "251 second will be forwarded",
            "354 go ahead",
            "250 queued as 1",
        ]);
        send(
            &relay(server.port, ""),
            &to(&["a@example.com", "b@example.com"]),
            "Subject: hello\r\n\r\nbody\r\n",
        )
        .expect("the message goes out");

        let heard = server.said.recv().expect("what the relay heard");
        assert_eq!(
            heard,
            vec![
                "EHLO localhost",
                "MAIL FROM:<someone@example.com>",
                "RCPT TO:<a@example.com>",
                "RCPT TO:<b@example.com>",
                "DATA",
                "| Subject: hello",
                "| ",
                "| body",
                "<body ends>",
                "QUIT",
            ]
        );
    }

    /// A line of the message that begins with a dot would otherwise end the body, and everything
    /// after it would be read as commands.
    #[test]
    fn a_line_that_begins_with_a_dot_is_written_twice() {
        let server = relay_saying(&[
            "220 relay ready",
            "250 relay",
            "250 sender ok",
            "250 ok",
            "354 go ahead",
            "250 queued",
        ]);
        send(&relay(server.port, ""), &to(&["a@example.com"]), "x\r\n.hidden\r\n..also\r\n")
            .expect("the message goes out");

        let heard = server.said.recv().expect("what the relay heard");
        assert!(heard.contains(&"| ..hidden".to_string()), "{heard:?}");
        assert!(heard.contains(&"| ...also".to_string()), "{heard:?}");
        assert!(heard.contains(&"<body ends>".to_string()), "{heard:?}");
    }

    /// **The password never reaches a connection anybody could read it off** (`AMB-D-476`). Go's
    /// standard library refused this for the plugin; carrying the conversation ourselves means
    /// carrying the refusal with it.
    #[test]
    fn no_account_is_sent_over_a_connection_that_could_not_be_encrypted() {
        let server = relay_saying(&["220 relay ready", "250 relay with no STARTTLS"]);
        let failed = send(&relay(server.port, "someone@example.com"), &to(&["a@example.com"]), "x\r\n")
            .expect_err("nothing is sent in the clear");
        assert!(failed.to_string().contains("no STARTTLS"), "{failed}");

        let heard = server.said.recv().expect("what the relay heard");
        assert_eq!(heard, vec!["EHLO localhost"]);
        assert!(!heard.iter().any(|line| line.contains("AUTH")), "{heard:?}");
    }

    /// A recipient the relay will not take is named, so a person knows which address to look at.
    #[test]
    fn a_refused_recipient_is_named() {
        let server = relay_saying(&[
            "220 relay ready",
            "250 relay",
            "250 sender ok",
            "550 no such user here",
        ]);
        let failed = send(&relay(server.port, ""), &to(&["nobody@example.com"]), "x\r\n")
            .expect_err("the relay refused it");
        let said = failed.to_string();
        assert!(said.contains("nobody@example.com"), "{said}");
        assert!(said.contains("550"), "{said}");
        let _ = server.said.recv();
    }

    /// The probe goes as far as the account and stops: nothing is posted to prove the settings work.
    #[test]
    fn the_probe_posts_nothing() {
        let server = relay_saying(&["220 relay ready", "250 relay"]);
        probe(&relay(server.port, "")).expect("the relay is reachable");

        let heard = server.said.recv().expect("what the relay heard");
        assert_eq!(heard, vec!["EHLO localhost", "QUIT"]);
    }

    /// A relay too old for `EHLO` is greeted the old way. It offers neither the upgrade nor an
    /// account, which is the relay inside a network that wants neither.
    #[test]
    fn a_relay_too_old_for_ehlo_is_still_reached() {
        let server = relay_saying(&[
            "220 relay ready",
            "500 what is EHLO",
            "250 hello",
            "250 sender ok",
            "250 ok",
            "354 go ahead",
            "250 queued",
        ]);
        send(&relay(server.port, ""), &to(&["a@example.com"]), "x\r\n").expect("the message goes out");

        let heard = server.said.recv().expect("what the relay heard");
        assert_eq!(heard.first().map(String::as_str), Some("EHLO localhost"));
        assert_eq!(heard.get(1).map(String::as_str), Some("HELO localhost"));
    }

    /// A relay that takes the connection and says nothing is the reason there is a deadline at all.
    #[test]
    fn a_relay_that_says_nothing_does_not_hold_the_run_forever() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("a port");
        let port = listener.local_addr().expect("the port").port();
        std::thread::spawn(move || {
            let held = listener.accept();
            std::thread::sleep(Duration::from_millis(600));
            drop(held);
        });
        let started = Instant::now();
        let failed = carry(&relay(port, ""), &to(&["a@example.com"]), Some("x\r\n"), started + Duration::from_millis(200))
            .expect_err("the greeting never came");
        assert!(started.elapsed() < SEND_TIMEOUT, "it waited the whole timeout out");
        let _ = failed;
    }

    /// A failure that quotes the credential is not written down word for word: a log is read by
    /// whoever is helping.
    #[test]
    fn a_rejection_that_quotes_the_password_is_not_repeated() {
        let quoted = Err(Error::Io(std::io::Error::other(
            "535 the account or password app-password is wrong",
        )));
        let kept = without_secret(quoted, "app-password").expect_err("still a failure");
        assert!(!kept.to_string().contains("app-password"), "{kept}");
        assert!(kept.to_string().contains("[the password]"), "{kept}");
    }
}
