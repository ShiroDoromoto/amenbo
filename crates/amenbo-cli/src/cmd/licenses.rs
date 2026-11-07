//! `licenses` — Amenbo's own license and the third-party notices, compiled into this binary. Opens no
//! store, so it answers anywhere.

use serde_json::json;

use crate::output::{print_json, CliError, Flags};

/// Amenbo's own license (Apache-2.0), the repository root's `LICENSE`.
const LICENSE: &str = include_str!("../../../../LICENSE");

/// The third-party notices, the repository root's `THIRD-PARTY-NOTICES`. CI holds that file to the
/// lockfiles, and it is read again on every build, so what is printed here is the set this binary was
/// built from.
const THIRD_PARTY_NOTICES: &str = include_str!("../../../../THIRD-PARTY-NOTICES");

/// `amenbo licenses`: print both texts. They live inside the binary rather than beside it because the
/// release archive holds the binary alone, and this way the standalone CLI and the one the desktop app
/// bundles carry the same notices.
pub(crate) fn licenses_cmd(flags: &Flags) -> Result<i32, CliError> {
    if flags.json {
        print_json(&json!({
            "license": LICENSE,
            "third_party_notices": THIRD_PARTY_NOTICES,
        }));
    } else {
        print!("{LICENSE}");
        if !LICENSE.ends_with('\n') {
            println!();
        }
        println!();
        print!("{THIRD_PARTY_NOTICES}");
    }
    Ok(0)
}
