//! The helper as git actually starts it: a real process, told where to call back by nothing but its
//! environment, and read the way git reads it — one line of standard output, and an exit status.
//!
//! The unit tests next door hold the two ends of the wire against each other. What they cannot
//! reach is the part that is not code here at all: that the question arrives as an argument, that
//! the answer leaves on standard output, and that having nobody to ask ends in a non-zero exit
//! rather than in an empty password git would try to use.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Output;

use amenbo_askpass::{answer, read_question, whole};

/// A window that answers one question and then goes away, standing on a port the operating system
/// picked. Hands back the address the helper is to be told, and the thread's own handle so the test
/// can read what arrived.
fn window(said: Option<String>) -> (String, std::thread::JoinHandle<(String, String)>) {
    let door = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let at = door.local_addr().expect("the port it got").to_string();
    let serving = std::thread::spawn(move || {
        let (mut talk, _) = door.accept().expect("the helper to call back");
        let asked = whole(&mut talk).expect("the question");
        let (token, asked) = read_question(&asked).expect("a question this side can read");
        talk.write_all(&answer(said.as_deref())).expect("to answer");
        (token, asked)
    });
    (at, serving)
}

/// Start the helper the way git does: the question as its one argument, and the two variables in
/// its environment.
fn helper(at: &str, token: &str, asked: &str) -> Output {
    amenbo_scratch::command(env!("CARGO_BIN_EXE_amenbo-askpass"))
        .arg(asked)
        .env(amenbo_askpass::ADDR_VAR, at)
        .env(amenbo_askpass::TOKEN_VAR, token)
        .output()
        .expect("the helper to run")
}

#[test]
fn what_the_window_says_is_what_git_reads() {
    let (at, serving) = window(Some("hunter2".into()));
    let out = helper(&at, "cafef00d", "Password for 'https://git@github.com':");
    let (token, asked) = serving.join().expect("the window's thread");

    assert_eq!(token, "cafef00d", "the helper carries the token it was given");
    assert_eq!(asked, "Password for 'https://git@github.com':", "the question arrives whole");
    assert!(out.status.success(), "an answered question ends well");
    assert_eq!(String::from_utf8_lossy(&out.stdout), "hunter2\n", "git reads one line");
}

/// The dialog was closed, or there was nothing behind the port willing to answer. git must not read
/// an empty password out of that — it would try it, and fail at the remote instead of here.
#[test]
fn a_question_nobody_answers_ends_non_zero_and_says_nothing() {
    let (at, serving) = window(None);
    let out = helper(&at, "cafef00d", "Password for 'https://git@github.com':");
    serving.join().expect("the window's thread");

    assert!(!out.status.success(), "no answer is a failure git can see");
    assert!(out.stdout.is_empty(), "and nothing is offered as the password");
}

/// The everyday case for a binary that sits on `PATH`-adjacent ground: somebody ran it by hand, or
/// git kept the variables from a session that has since ended. There is nowhere to ask, so it fails
/// rather than answering.
#[test]
fn with_nowhere_to_ask_it_fails_rather_than_guessing() {
    let out = amenbo_scratch::command(env!("CARGO_BIN_EXE_amenbo-askpass"))
        .arg("Password for 'https://git@github.com':")
        .output()
        .expect("the helper to run");
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
}

/// A port that is bound but not this app's: the connection is made and then dropped. What comes
/// back is nothing, which is not a password.
#[test]
fn a_door_that_shuts_without_speaking_is_not_an_answer() {
    let door = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let at = door.local_addr().expect("the port it got").to_string();
    let serving = std::thread::spawn(move || {
        let (mut talk, _) = door.accept().expect("the helper to call back");
        let mut sink = Vec::new();
        let _ = talk.read_to_end(&mut sink);
        drop(talk);
    });
    let out = helper(&at, "cafef00d", "Password:");
    serving.join().expect("the door's thread");

    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
}

/// Nothing is listening at all. The helper is given a port it can reach nothing on, and has to end
/// on its own rather than sit there while git waits.
#[test]
fn nothing_listening_is_not_waited_on_for_ever() {
    // Bound and dropped: the number is real, and by the time the helper reaches it nobody is there.
    let free = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let at = free.local_addr().expect("the port it got").to_string();
    drop(free);

    let out = helper(&at, "cafef00d", "Password:");
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
}
