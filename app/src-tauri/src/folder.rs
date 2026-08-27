//! What a folder holds — the question [`crate::fileproto`] refuses, answered here.
//!
//! That door hands out the bytes of one file and says so plainly: a directory is not listed there,
//! because listing is not what a door that streams bytes is for. The file face asks the other half
//! of the question — what is in this folder, and what does this file say — and it asks over the
//! command seam, where an answer can be a list.
//!
//! **The three doors are all that is left here.** How far a name may reach is
//! [`crate::folder_fence`]'s, what is in a folder is [`crate::folder_walk`]'s, and what one file
//! says is [`crate::folder_bytes`]'s; each door below asks those three and returns what they say.

use crate::dto::FolderEntryDto;
use crate::error::CmdError;
use crate::folder_fence::{gone, plain, rooted, under};
use crate::folder_walk::{level, shown, walker};

/// The front of the name a save writes its bytes into before it puts them in place
/// (`crate::folder_save`).
///
/// **It is left out of both walks below.** The window it exists in is about a tenth of a
/// millisecond and the watch waits 400 ms for quiet before it looks, so it is rarely seen
/// (`AMB-T-3739`) — but a burst arriving around it draws a row in the tree under a name nobody
/// wrote, which reads as the app being broken rather than as the app working. The row would go on
/// standing there too: what takes it away is the next walk, and nothing is bound to happen next.
///
/// **It carries no part of the file's own name.** A name may be 255 bytes and no more, so one built
/// out of another name plus a front of its own is longer than the filesystem will take for exactly
/// the files whose names are longest — a refusal that would have nothing to do with what the reader
/// was trying to save.
pub const SAVING: &str = ".amenbo-saving-";

/// The names directly inside one folder — folders first, then files, each run in the order a person
/// reads them. Nothing recurses: a folded tree opens one level at a time (`AMB-T-3602`).
#[tauri::command]
pub fn folder_entries(
    project_id: i64,
    root: String,
    path: Vec<String>,
) -> Result<Vec<FolderEntryDto>, CmdError> {
    let (roots, base) = rooted(project_id, &root)?;
    let (_owner, dir) = under(&roots, base, &path).ok_or_else(gone)?;
    // Read off the name itself and not off what it leads to: a folder that is a link is not walked,
    // whatever is on the other side of it (`AMB-D-782`).
    if !std::fs::symlink_metadata(&dir).is_ok_and(|meta| meta.is_dir()) {
        return Err(gone());
    }
    // The level is walked twice, and the difference between the two walks is the mark: what the
    // repository ignores is drawn, and drawn as ignored (`AMB-D-786`). Asking the ignore rules
    // directly instead would be a second reading of them — global file, parents, `.git/info/exclude`
    // and all — and the one that could drift from the walk the watch is actually laid over.
    let kept: std::collections::HashSet<String> =
        level(&mut walker(&dir)).into_iter().map(|(name, _)| name).collect();
    let mut rows: Vec<FolderEntryDto> = level(&mut shown(&dir))
        .into_iter()
        .map(|(name, is_dir)| FolderEntryDto {
            ignored: !kept.contains(&name),
            name,
            is_dir,
        })
        .collect();
    rows.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(rows)
}

/// Open one file the way the machine would open it — the reader's own applications, not ours.
///
/// The face has an editor of its own, and this is still worth having: what it opens a file in is
/// whatever the person already opens that kind of file with. The OS decides what that is, and Amenbo
/// does not keep an opinion about it.
///
/// The path goes out through [`plain`] because this is a door out of the process: past 260
/// characters the fence answers in Windows's internal spelling, and what is on the other side of
/// this call is the shell (`AMB-T-3749`).
#[tauri::command]
pub fn folder_open_file(project_id: i64, root: String, path: Vec<String>) -> Result<(), CmdError> {
    let (roots, base) = rooted(project_id, &root)?;
    let (_owner, file) = under(&roots, base, &path).ok_or_else(gone)?;
    tauri_plugin_opener::open_path(plain(&file).as_ref(), None::<&str>)
        .map_err(|e| CmdError::coded("folder.open", e.to_string(), serde_json::Value::Null))
}

/// Show one file where it lives, in the machine's file manager.
///
/// It is the other half of opening: what a person wants of a file is as often "where is this" as
/// "what is in it", and a panel that could only read would leave them hunting for a path they can
/// already see.
///
/// Spelled with [`plain`] for the same reason as [`folder_open_file`]: the file manager is outside
/// this process, and the plugin's own levelling stops at 260 characters where ours does not
/// (`dunce::simplified`, which it calls, keeps the verbatim front on past that).
#[tauri::command]
pub fn folder_reveal_file(project_id: i64, root: String, path: Vec<String>) -> Result<(), CmdError> {
    let (roots, base) = rooted(project_id, &root)?;
    let (_owner, file) = under(&roots, base, &path).ok_or_else(gone)?;
    tauri_plugin_opener::reveal_item_in_dir(plain(&file).as_ref())
        .map_err(|e| CmdError::coded("folder.reveal", e.to_string(), serde_json::Value::Null))
}
