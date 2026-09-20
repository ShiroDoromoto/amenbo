//! `skin template` / `skin write-out`, end to end: the two roads a skin file travels out on, and the
//! one road back in.
//!
//! Driven as a process because what these two produce is a file on disk — a zip, read back by the
//! same commands an author would run over one somebody handed them (`AMB-D-936`).

mod harness;

use harness::*;

/// The path a test writes to, inside its own throwaway home so nothing outlives the run.
fn out(cli: &Cli, name: &str) -> String {
    cli.home.join(name).display().to_string()
}

/// A skin to start from is a zip, it reads back as a skin, and it goes in under the name it gives
/// itself. The whole loop, because each step is only worth anything if the next one accepts it.
#[test]
fn the_template_is_a_zip_that_reads_back_as_a_skin_and_goes_in() {
    let cli = Cli::new();
    let at = out(&cli, "start.zip");

    let wrote = cli.json(&["skin", "template", &at, "--json"]);
    assert_eq!(wrote["path"].as_str(), Some(at.as_str()));
    let bytes = std::fs::read(&at).expect("the template was written");
    assert_eq!(&bytes[..2], b"PK", "a zip, not the document on its own");

    let read = cli.json(&["skin", "validate", &at, "--json"]);
    assert_eq!(read["name"].as_str(), Some("my-skin"));

    let took = cli.json(&["skin", "add", &at, "--json"]);
    assert_eq!(took["name"].as_str(), Some("my-skin"));
    let (listed, _) = cli.run(&["skin", "list"]);
    assert!(listed.contains("my-skin"), "{listed}");
}

/// What comes back out is the file that went in, byte for byte. Rebuilding the document from the
/// values this build read would hand back the colours and leave the author's materials behind.
#[test]
fn a_skin_is_written_back_out_as_the_file_it_arrived_in() {
    let cli = Cli::new();
    let at = out(&cli, "start.zip");
    let again = out(&cli, "again.zip");

    cli.json(&["skin", "template", &at, "--json"]);
    cli.json(&["skin", "add", &at, "--json"]);
    cli.json(&["skin", "write-out", "my-skin", &again, "--json"]);

    assert_eq!(
        std::fs::read(&at).unwrap(),
        std::fs::read(&again).unwrap(),
        "what was handed in is what comes back"
    );
}

/// Neither road writes over a file that is already there. A path is typed on a command line, and the
/// thing it most nearly misses is the skin the author is still editing.
#[test]
fn neither_road_writes_over_a_file_that_is_already_there() {
    let cli = Cli::new();
    let at = out(&cli, "taken.zip");
    std::fs::write(&at, b"not mine to lose").unwrap();

    let (said, code) = cli.run_err(&["skin", "template", &at]);
    assert_eq!(code, 1, "{said}");
    assert!(said.contains("already there"), "{said}");

    let start = out(&cli, "start.zip");
    cli.json(&["skin", "template", &start, "--json"]);
    cli.json(&["skin", "add", &start, "--json"]);
    let (said, code) = cli.run_err(&["skin", "write-out", "my-skin", &at]);
    assert_eq!(code, 1, "{said}");
    assert!(said.contains("already there"), "{said}");

    assert_eq!(std::fs::read(&at).unwrap(), b"not mine to lose", "and the file is untouched");
}

/// The four this build ships are not files on the device, so there is nothing to copy out. What the
/// refusal points at is the road that does work for them.
#[test]
fn a_skin_that_ships_with_the_build_is_not_written_out() {
    let cli = Cli::new();
    let (said, code) = cli.run_err(&["skin", "write-out", "washi", &out(&cli, "washi.zip")]);
    assert_eq!(code, 1, "{said}");
    assert!(said.contains("ships with this build"), "{said}");
    assert!(said.contains("skin template"), "and says where to go instead: {said}");
}

/// A name nothing is held under is a refusal rather than an empty file left behind.
#[test]
fn a_name_nothing_is_held_under_is_refused() {
    let cli = Cli::new();
    let at = out(&cli, "ghost.zip");
    let (said, code) = cli.run_err(&["skin", "write-out", "ghost", &at]);
    assert_eq!(code, 1, "{said}");
    assert!(said.contains("no skin is kept as 'ghost'"), "{said}");
    assert!(!std::path::Path::new(&at).exists(), "and nothing was written");
}
