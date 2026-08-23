//! The labels of the two windows this app opens, and which of them anything that raises a window
//! means.
//!
//! Amenbo is one app with two faces (`AMB-D-733`'s theme): the **board**, where the ledger is read
//! and written, and the **talk** window, where the agents run. They are separate windows rather than
//! two views of one so they can sit on separate displays, and one process still backs both — there
//! is one store, one watcher, one guard.
//!
//! Everything that raises a window from outside the webview — a second launch turned away, a
//! notification clicked — means the **board**. What sends those is an arrival in the inbox or the
//! user asking for the app they already have open, and both of those are the board's subject. So
//! [`BOARD`](crate::windows::BOARD) is what they name, spelled once here rather than as a string in
//! three files.
//!
//! A launch at login is the ordinary launch (`autostart`), so both windows come up there exactly as
//! they do when the user opens the app: the board in front, the talk window behind it (`focus:
//! false` in `tauri.conf.json`). Nothing decides between them at startup, because a window that came
//! up is one the user can move to another display and leave there.

/// The window the ledger is read in — the app as it was before the second one existed, which is why
/// its label is still `main`: the label is what `tauri.conf.json` and every `get_webview_window`
/// call agree on, and renaming it would buy nothing.
pub const BOARD: &str = "main";

/// The window the agents run in.
pub const TALK: &str = "talk";
