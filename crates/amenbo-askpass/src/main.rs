//! The program git and ssh run when they need to ask the person something and have no terminal to
//! ask it in.
//!
//! It is named by `GIT_ASKPASS` and `SSH_ASKPASS`, it is handed the question as its one argument,
//! and whatever it writes to standard output is taken as the answer. That contract is not ours —
//! it is the one both of them have had for decades — and the whole of what this binary does is put
//! the question to the window and write back what the window says.
//!
//! **It is a program of its own, and not a face of the CLI**, because ssh runs it with `execlp`
//! and one argument: there is no room in that call for a subcommand word, and no shell to split
//! one out. git would allow it — its side goes through a shell — but a helper that behaved
//! differently depending on which of the two started it is a helper with two behaviours to keep
//! true.
//!
//! **It links nothing of Amenbo but the wire** ([`amenbo_askpass`]). This runs once per question,
//! in front of a person who is waiting, and a binary that opened a store on the way would be
//! reading the disk to do nothing with it.
//!
//! **Nothing said here is written anywhere.** The answer passes from the window's socket to
//! standard output and the process ends; what becomes of it afterwards is git's, and whether it is
//! kept is the credential helper's (`AMB-D-913`).

// The environment is this program's whole input: it is started by git, by name, with no say in its
// own arguments, and the two variables below are how the app that started git reaches it. The
// funnel `clippy.toml` holds every other crate to is the app's — one reviewable list of what
// *Amenbo* depends on inheriting — and this process is not Amenbo. Both of the variables it reads
// are named a line apart, in the module that also writes them.
#![allow(clippy::disallowed_methods)]

use std::io::Write;
use std::net::TcpStream;
use std::time::Duration;

use amenbo_askpass::{question, read_answer, whole, ADDR_VAR, TOKEN_VAR};

/// How long the window is given to come back.
///
/// It is a person being asked, so this is measured in the time somebody takes to find a password
/// rather than in the time a machine takes to answer. What it is really for is the case where
/// nobody is coming: the app was killed, the socket stayed open, and without a limit here git
/// would sit on it for as long as the terminal it was started from lived.
const WAIT: Duration = Duration::from_secs(10 * 60);

/// How long the window is given to accept the connection at all. It is a listener on this same
/// machine, already bound before git was started, so reaching it is instant or never.
const REACH: Duration = Duration::from_secs(5);

fn main() {
    let Some(said) = asked() else { std::process::exit(1) };
    // The newline is what both readers cut the answer at. A password with a newline in it cannot
    // be carried through this door by anyone, git's own prompt included, so there is nothing here
    // to escape.
    let mut out = std::io::stdout();
    if out.write_all(said.as_bytes()).and_then(|()| out.write_all(b"\n")).is_err() {
        std::process::exit(1);
    }
}

/// Put the question to the window, and hand back what it said. `None` is every way of having no
/// answer, and they are one case: git is told the same thing by a non-zero exit whichever it was,
/// and then carries on failing exactly as it fails today.
fn asked() -> Option<String> {
    let addr = std::env::var(ADDR_VAR).ok()?;
    let token = std::env::var(TOKEN_VAR).ok()?;
    // Everything after the program's own name, rejoined. git and ssh each pass exactly one
    // argument, and a question that arrived split is one this would otherwise answer half of.
    let asked = std::env::args().skip(1).collect::<Vec<_>>().join(" ");

    let at = addr.parse().ok()?;
    let mut door = TcpStream::connect_timeout(&at, REACH).ok()?;
    door.set_read_timeout(Some(WAIT)).ok()?;
    door.write_all(&question(&token, &asked)).ok()?;
    // The half-close is the end of the question: the window reads to the end of the stream, so
    // without this both sides wait for the other.
    door.shutdown(std::net::Shutdown::Write).ok()?;
    read_answer(&whole(&mut door)?)
}
