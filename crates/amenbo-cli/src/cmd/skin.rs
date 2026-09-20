//! `skin`: the skins this device holds, from the side with no window.
//!
//! The one thing a skin can get wrong is make the screen unreadable, and the screen is where it
//! would be fixed. So the whole of it is here as well: what is held, what is on, taking one in,
//! putting one on, taking one off. A reader who cannot read their own window is not out of options.
//!
//! `validate` is the author's face of the same check the import runs, so the reason a file will be
//! turned away is learned before it is handed to anybody. `template` writes out what this build
//! sets, which is a skin already — an author starts by editing the colours they can see.

use std::path::Path;

use serde_json::json;

use amenbo_core::config::Paths;
use amenbo_core::skin::{Refusal, Skin, Taken, Warning};
use amenbo_core::skin_contrast::{self, Report};
use amenbo_core::Store;

use crate::cli::SkinCmd;
use crate::output::{human, print_json, CliError, Flags};

/// The word `use` takes to mean "no skin". Not a skin's name — a name may not be it, being what the
/// absence is called rather than a thing that could be installed.
const NONE: &str = "none";

pub(crate) fn skin(store: &mut Store, flags: &Flags, sub: SkinCmd) -> Result<i32, CliError> {
    match sub {
        SkinCmd::List => list(store, flags),
        SkinCmd::Add { path, yes } => add(store, flags, &path, yes),
        SkinCmd::Use { name } => wear(store, flags, &name),
        SkinCmd::Rm { name } => remove(store, flags, &name),
        SkinCmd::Validate { path } => validate(flags, &path),
        SkinCmd::Template => template(store, flags),
    }
}

/// What is held, and which of them is on.
fn list(store: &Store, flags: &Flags) -> Result<i32, CliError> {
    let on = store.config.skin.clone();
    let held = Skin::installed_all(&store.paths);

    if flags.json {
        print_json(&json!({
            "on": on,
            "skins": held.iter().map(|(name, read)| match read {
                Ok(s) => json!({
                    "name": name, "title": s.title, "titles": s.titles, "author": s.author,
                    "version": s.version,
                    "themes": s.themes, "license": s.license, "homepage": s.homepage,
                    "on": Some(name) == on.as_ref(),
                    "official": amenbo_core::skin_official::is_official(name),
                }),
                Err(e) => json!({
                    "name": name, "error": e.to_string(), "on": Some(name) == on.as_ref(),
                    "official": amenbo_core::skin_official::is_official(name),
                }),
            }).collect::<Vec<_>>(),
        }));
        return Ok(0);
    }

    for (name, read) in &held {
        let mark = if Some(name) == on.as_ref() { "*" } else { " " };
        // Said on the line rather than left to be discovered by `skin rm`: what this build ships
        // is not on the device, so it is not there to take off.
        let from = if amenbo_core::skin_official::is_official(name) { "  — shipped with Amenbo" } else { "" };
        match read {
            Ok(s) => {
                let version = s.version.as_deref().unwrap_or("-");
                human(flags, format!("{mark} {name}  {} ({version}) [{}]{from}", s.title, s.themes.join(", ")));
            }
            // A file that will not read is said out loud: it is in the directory, so a list that left
            // it out would have the reader hunting for a skin that is sitting right there.
            Err(e) => human(flags, format!("{mark} {name}  cannot be read: {e}{from}")),
        }
    }
    match &on {
        Some(name) if !held.iter().any(|(held, _)| held == name) => human(
            flags,
            format!("on: {name} — but no skin is kept under that name; the built-in colours are what is drawn"),
        ),
        Some(_) => {}
        None => human(flags, "on: none (the colours this build ships with)"),
    }
    Ok(0)
}

/// Take one file in. What the check refuses is refused here; what it only warns about is said and
/// taken anyway, and so is a pairing that falls short of AA — an author has to be able to try their
/// own work in progress.
fn add(store: &mut Store, flags: &Flags, path: &Path, yes: bool) -> Result<i32, CliError> {
    let (yaml, taken, report) = judge(path)?;
    let name = taken.skin.name.clone();

    // Asked before the one below, and not answerable with --yes: what this build ships is not a
    // file on the device, so there is nothing here for a replace to replace.
    if amenbo_core::skin_official::is_official(&name) {
        return Err(CliError {
            code: "skin_name_is_ours",
            message: amenbo_core::skin_official::name_is_ours(&name),
            hint: Some("Change the `name:` line; `title:` is the one shown on screen.".to_string()),
            exit: 1,
        });
    }

    if let Some(there) = Skin::installed(&store.paths, &name).map_err(CliError::from)? {
        if !yes && !flags.yes {
            let mine = version_of(&taken.skin);
            let theirs = version_of(&there);
            return Err(CliError {
                code: "skin_name_taken",
                message: format!("a skin is already kept as '{name}': {theirs} is there, {mine} is the one being added"),
                hint: Some("Pass --yes to replace it, or rename the one being added.".to_string()),
                exit: 1,
            });
        }
    }

    Skin::install(&store.paths, &name, &yaml).map_err(CliError::from)?;

    if flags.json {
        print_json(&json!({
            "ok": true, "action": "skin.add", "name": name,
            "path": store.paths.skin_file(&name).display().to_string(),
            "warnings": warnings_json(&taken),
            "contrast": contrast_json(&report),
        }));
        return Ok(0);
    }
    human(flags, format!("✓ {name} is in {}", store.paths.skin_file(&name).display()));
    say_warnings(flags, &taken);
    say_contrast(flags, &report);
    human(flags, format!("Put it on with `{} skin use {name}`.", Paths::command_name()));
    Ok(0)
}

/// Put one on, or take whatever is on off.
fn wear(store: &mut Store, flags: &Flags, name: &str) -> Result<i32, CliError> {
    let off = name == NONE;
    if !off && Skin::installed(&store.paths, name).map_err(CliError::from)?.is_none() {
        return Err(CliError {
            code: "skin_not_found",
            message: format!("no skin is kept as '{name}'"),
            hint: Some(format!("`{} skin list` shows what is held.", Paths::command_name())),
            exit: 1,
        });
    }
    store.config.set("skin", if off { "" } else { name }).map_err(CliError::from)?;
    store.save_config().map_err(CliError::from)?;

    if flags.json {
        print_json(&json!({ "ok": true, "action": "skin.use", "on": store.config.skin }));
        return Ok(0);
    }
    match &store.config.skin {
        Some(on) => human(flags, format!("✓ {on} is on")),
        None => human(flags, "✓ no skin is on (the colours this build ships with)"),
    }
    Ok(0)
}

/// Take one off the device. The one that is on goes off with it — a name in the config pointing at
/// a file that is gone says the device is wearing something it has not got.
fn remove(store: &mut Store, flags: &Flags, name: &str) -> Result<i32, CliError> {
    if amenbo_core::skin_official::is_official(name) {
        return Err(CliError {
            code: "skin_shipped",
            message: amenbo_core::skin_official::not_on_the_device(name),
            hint: Some(format!("`{} skin use none` takes off whatever is on.", Paths::command_name())),
            exit: 1,
        });
    }
    let gone = Skin::uninstall(&store.paths, name).map_err(CliError::from)?;
    let was_on = store.config.skin.as_deref() == Some(name);
    if gone && was_on {
        store.config.set("skin", "").map_err(CliError::from)?;
        store.save_config().map_err(CliError::from)?;
    }
    if flags.json {
        print_json(&json!({ "ok": true, "action": "skin.rm", "name": name, "removed": gone, "taken_off": gone && was_on }));
        return Ok(0);
    }
    if gone {
        human(flags, format!("✓ {name} is off the device"));
        if was_on {
            human(flags, "  it was the one on, so the built-in colours are back");
        }
    } else {
        human(flags, format!("no skin is kept as '{name}'"));
    }
    Ok(0)
}

/// The author's face: the same reading and the same check, over a file that is not being taken in.
fn validate(flags: &Flags, path: &Path) -> Result<i32, CliError> {
    let (_, taken, report) = judge(path)?;
    if flags.json {
        print_json(&json!({
            "ok": true, "action": "skin.validate", "name": taken.skin.name,
            "title": taken.skin.title, "titles": taken.skin.titles,
            "themes": taken.skin.themes,
            "warnings": warnings_json(&taken),
            "contrast": contrast_json(&report),
        }));
        return Ok(0);
    }
    human(flags, format!("✓ {} reads as a skin ({})", taken.skin.name, taken.skin.themes.join(", ")));
    say_warnings(flags, &taken);
    say_contrast(flags, &report);
    Ok(0)
}

/// Write out a whole skin to start from: every colour a skin may set, on both sides, each with a
/// line saying what it is for. Taken from the skin that is on where there is one, and filled in
/// from this build for everything that skin left alone — so what comes out is a complete file
/// whichever it was taken from. The name is not the one it was taken from: a file calling itself
/// what is already held would replace it on the way back in.
///
/// The other names a skin may set (the type scale, the spacing, the font stacks) take no colour and
/// are left for the author to add; `skin validate` says whether a name is one this build has.
fn template(store: &Store, flags: &Flags) -> Result<i32, CliError> {
    // Taken from the skin that is on, where one is: an author who is editing what they are looking
    // at starts from those values, and one who is starting out gets this build's.
    let on = match &store.config.skin {
        Some(name) => Skin::installed(&store.paths, name)
            .map_err(CliError::from)?
            .and_then(|s| s.check().ok())
            .map(|t| t.skin),
        None => None,
    };
    let yaml = skin_contrast::template(&skin_contrast::template_name(on.as_ref()), on.as_ref());
    if flags.json {
        print_json(&json!({ "ok": true, "action": "skin.template", "yaml": yaml }));
        return Ok(0);
    }
    print!("{yaml}");
    Ok(0)
}

/// Read one file, judge it, and measure it. The three steps every face here takes, in the one order
/// they can be taken in.
fn judge(path: &Path) -> Result<(String, Taken, Report), CliError> {
    let yaml = std::fs::read_to_string(path).map_err(|e| CliError {
        code: "io_error",
        message: format!("{}: {e}", path.display()),
        hint: None,
        exit: 1,
    })?;
    let read = Skin::read(&yaml).map_err(CliError::from)?;
    let taken = read.check().map_err(|r| refused(path, r))?;
    let report = skin_contrast::measure(&taken.skin);
    Ok((yaml, taken, report))
}

/// A whole-skin refusal, said as the command's failure.
fn refused(path: &Path, refusal: Refusal) -> CliError {
    let (message, hint) = match refusal {
        Refusal::SkinVAhead { declared, understood } => (
            format!("this skin is written for skin_v {declared}; this Amenbo reads {understood}"),
            Some(format!("Update Amenbo (`{} update`), or ask the author for a version for this one.", Paths::command_name())),
        ),
        Refusal::UnknownSide(word) => (
            format!("'{word}' in themes is not a side; light and dark are the two there are"),
            None,
        ),
        Refusal::SideDeclaredEmpty(side) => (
            format!("themes says {side}, and the {side} table sets nothing"),
            Some(format!("Set the {side} colours, or take {side} out of themes.")),
        ),
        Refusal::SideNotDeclared(side) => (
            format!("the {side} table sets colours, and themes does not say {side}"),
            Some(format!("Add {side} to themes, or take the table out.")),
        ),
        Refusal::UnusableName(name) => (
            format!("'{name}' is not a name a skin can be kept under"),
            Some("Use lowercase letters, digits, '-' and '_', opening on a letter or a digit.".to_string()),
        ),
        Refusal::FontWithoutLicenceText => (
            "the embedded font has no license_text".to_string(),
            Some(
                "A font's terms have to travel with it. Put the licence in full under \
                 font_file.license_text, or take the font out."
                    .to_string(),
            ),
        ),
    };
    CliError { code: "skin_refused", message: format!("{}: {message}", path.display()), hint, exit: 1 }
}

/// The version an author wrote, or a stand-in — two files calling themselves the same skin are told
/// apart by it, and an author who wrote none has to be shown as having written none.
fn version_of(s: &Skin) -> String {
    match &s.version {
        Some(v) => format!("version {v}"),
        None => "a copy with no version".to_string(),
    }
}

fn warnings_json(taken: &Taken) -> Vec<serde_json::Value> {
    taken
        .warnings
        .iter()
        .map(|w| match w {
            Warning::UnknownHeaderKey(key) => json!({ "kind": "unknown_header_key", "key": key }),
            Warning::UnknownToken { theme, key } => json!({ "kind": "unknown_token", "theme": theme.as_str(), "key": key }),
            Warning::ClosedToken { theme, key } => json!({ "kind": "closed_token", "theme": theme.as_str(), "key": key }),
            Warning::NotText { theme, key } => json!({ "kind": "not_text", "theme": theme.as_str(), "key": key }),
            Warning::UnsafeValue { theme, key, why } => json!({
                "kind": "unsafe_value", "theme": theme.as_str(), "key": key, "why": why.en(),
            }),
            Warning::FontDropped(why) => json!({ "kind": "font_dropped", "why": why.en() }),
            Warning::Frame { theme, key, wrote, used } => json!({
                "kind": "frame", "theme": theme.as_str(), "key": key,
                "wrote": wrote, "used": used,
            }),
            Warning::Choice { theme, key, wrote } => json!({
                "kind": "choice", "theme": theme.as_str(), "key": key, "wrote": wrote,
            }),
            Warning::Scale { theme, key, wrote, used } => json!({
                "kind": "scale", "theme": theme.as_str(), "key": key,
                "wrote": wrote, "used": used,
            }),
        })
        .collect()
}

fn contrast_json(report: &Report) -> serde_json::Value {
    json!({
        "measured": report.measured,
        "clear": report.is_clear(),
        "short": report.short.iter().map(|r| json!({
            "theme": r.side.as_str(), "ink": r.ink, "ground": r.ground,
            "ratio": (r.ratio * 100.0).round() / 100.0, "floor": r.floor,
        })).collect::<Vec<_>>(),
        "unread": report.unread.iter().map(|u| json!({
            "theme": u.side.as_str(), "name": u.name, "value": u.value,
        })).collect::<Vec<_>>(),
    })
}

fn say_warnings(flags: &Flags, taken: &Taken) {
    for w in &taken.warnings {
        let line = match w {
            Warning::UnknownHeaderKey(key) => format!("  · {key}: this Amenbo has no meaning for it — dropped"),
            Warning::UnknownToken { theme, key } => format!("  · {theme}.{key}: not a name this Amenbo has — dropped"),
            Warning::ClosedToken { theme, key } => format!("  · {theme}.{key}: this one is not a skin's to move — dropped"),
            Warning::NotText { theme, key } => format!("  · {theme}.{key}: the value is not text — dropped"),
            Warning::UnsafeValue { theme, key, why } => format!("  · {theme}.{key}: the value {} — dropped", why.en()),
            Warning::FontDropped(why) => format!("  · the embedded font: {} — dropped", why.en()),
            Warning::Frame { theme, key, wrote, used } => match used {
                Some(used) => format!("  · {theme}.{key}: '{wrote}' — drawn at {used}"),
                None => format!("  · {theme}.{key}: '{wrote}' is not one this build draws — dropped"),
            },
            Warning::Choice { theme, key, wrote } => {
                format!("  · {theme}.{key}: '{wrote}' is not one this build draws — dropped")
            }
            Warning::Scale { theme, key, wrote, used } => match used {
                Some(used) => format!("  · {theme}.{key}: '{wrote}' — moved by {used}"),
                None => format!("  · {theme}.{key}: '{wrote}' is not a number — nothing moved"),
            },
        };
        human(flags, line);
    }
}

fn say_contrast(flags: &Flags, report: &Report) {
    for u in &report.unread {
        human(flags, format!("  ? {}.{}: '{}' is not a colour this Amenbo can measure", u.side, u.name, u.value));
    }
    if report.short.is_empty() {
        human(flags, format!("  contrast: all {} pairings clear their floor", report.measured));
        return;
    }
    human(flags, format!("  contrast: {} of {} pairings are under their floor", report.short.len(), report.measured));
    for r in &report.short {
        human(flags, format!("  ! {}: {} on {} is {:.2}, under {:.1}", r.side, r.ink, r.ground, r.ratio, r.floor));
    }
}
