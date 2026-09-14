//! `viewer`: what the command group answers on a device where the server has never been stood up, and
//! what it refuses to put anywhere a camera cannot reach.
//!
//! **Nothing here reaches Cloudflare.** Everything past setup needs a server in somebody's own account,
//! so what a process can be driven through is the half before that: the refusal that names the button to
//! press, the two addresses that are the same on every device, and the one input that is never an
//! argument.

mod harness;

use harness::*;

/// A device nobody has run setup on. Every road that needs a server says the same thing and names the
/// one command that makes it exist — the ordinary way to arrive here is pressing the second button
/// before the first.
#[test]
fn a_device_with_no_server_is_told_which_command_makes_one() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);

    for road in [
        vec!["viewer", "phones"],
        vec!["viewer", "send"],
        vec!["viewer", "repair"],
        vec!["viewer", "revoke", "--yes"],
    ] {
        let (stderr, code) = cli.run_err(&road);
        assert_eq!(code, 2, "{road:?} should refuse: {stderr}");
        assert!(stderr.contains("viewer setup"), "{road:?} names the command that stands one up: {stderr}");
    }
}

/// Where the app is got is the same on every device, set up or not — so this answers before setup has
/// ever run, and answers in words as well as on a code.
#[test]
fn where_the_app_is_got_is_answered_before_a_server_exists() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);

    let out = cli.json(&["viewer", "app", "--json"]);
    let rows = out["app"].as_array().expect("one row per kind of phone");
    assert_eq!(rows.len(), 2, "{out}");
    let phones: Vec<&str> = rows.iter().map(|row| row["phone"].as_str().unwrap()).collect();
    assert_eq!(phones, vec!["iPhone", "Android"], "the brands are what tell the two apart: {out}");
    for row in rows {
        assert!(row["link"].as_str().unwrap().starts_with("https://"), "{row}");
    }

    // Without `--json` the addresses are still written out, for a terminal that cannot draw a code.
    let (stdout, code) = cli.run(&["viewer", "app"]);
    assert_eq!(code, 0, "{stdout}");
    assert!(stdout.contains("apps.apple.com"), "{stdout}");
    assert!(stdout.contains("play.google.com"), "{stdout}");
}

/// **A read code carries the encryption key**, so it is drawn on a screen and nowhere a pipe can take it.
/// The refusal comes before anything is issued: issuing replaces whatever code the server was holding,
/// and a run that ends with nothing on the screen would have stopped the phone that was reading.
#[test]
fn a_read_code_is_not_written_into_a_pipe() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);

    let (stderr, code) = cli.run_err(&["viewer", "qr", "--json"]);
    assert_eq!(code, 2, "{stderr}");
    assert!(stderr.contains("camera"), "it says what a code is for: {stderr}");

    // The test runner's stdout is not a terminal either, so the plain run is refused for the other
    // reason — and names the way to force it.
    let (stderr, code) = cli.run_err(&["viewer", "qr"]);
    assert_eq!(code, 2, "{stderr}");
    assert!(stderr.contains("--terminal"), "{stderr}");
}

/// **The API token is never an argument.** It comes in on stdin, and a run that is handed nothing says
/// so rather than building in an account it was not told the name of.
#[test]
fn setup_takes_the_api_token_on_stdin_and_refuses_an_empty_one() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);

    let (stderr, code) = cli.run_err(&["viewer", "setup"]);
    assert_eq!(code, 2, "{stderr}");
    assert!(stderr.contains("API token"), "{stderr}");
    // The link that opens Cloudflare's token screen with the permissions already ticked.
    assert!(stderr.contains("dash.cloudflare.com"), "it says where to make one: {stderr}");
}
