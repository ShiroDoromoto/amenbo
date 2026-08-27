//! Whether a drop was a copy or a move — the one thing about it the drag event will not say.
//!
//! Files dragged in from the desktop reach the application rather than the page (`AMB-D-775`), and
//! what the host is handed is paths and a point. **The modifier keys are on none of the three**: wry
//! receives them on Windows and throws them away, and does not ask for them at all on macOS or Linux
//! (`AMB-T-3740`). So they are not waited for — the keyboard is read directly, at the moment the
//! drop lands, and that reading is the whole of this module.
//!
//! Each operating system is asked in its own way, and each has its own idea of which key means
//! what:
//!
//! | | how it is read | copy | move |
//! |---|---|---|---|
//! | macOS | `+[NSEvent modifierFlags]` | Option | Command |
//! | Windows | `GetAsyncKeyState` | Ctrl | Shift |
//! | Linux | the display's keymap, through GTK | Ctrl | Shift |
//!
//! The keys are the platform's own convention and are not levelled: a reader holding Option on a Mac
//! is asking for a copy in every application they own, and Amenbo is not the one to teach them
//! otherwise.

use crate::dto::DropEffectDto;

/// What the keys held at the drop asked for.
///
/// Read now rather than remembered: a drop is over by the time anything downstream of it runs, and
/// what is wanted is the keyboard as it was when the file was let go.
#[tauri::command]
pub async fn drop_effect(app: tauri::AppHandle) -> DropEffectDto {
    let (copy, moved) = held(app).await;
    effect_of(copy, moved)
}

/// Turn the two keys into the answer.
///
/// **Both held is a copy.** Nothing about either operating system says which of the two wins, so the
/// tie is broken on what the mistake costs: a copy that was meant to be a move leaves a file to
/// delete, and a move that was meant to be a copy leaves nothing to undo.
fn effect_of(copy: bool, moved: bool) -> DropEffectDto {
    match (copy, moved) {
        (true, _) => DropEffectDto::Copy,
        (false, true) => DropEffectDto::Move,
        (false, false) => DropEffectDto::Default,
    }
}

/// `(copy key, move key)`, as this machine's keyboard has them this instant.
///
/// Unreadable is both false — the plain drop. What is being read is a refinement of something that
/// has already happened, and a face that could not read it still has a drop to answer.
#[cfg(target_os = "macos")]
async fn held(_app: tauri::AppHandle) -> (bool, bool) {
    use objc2_app_kit::{NSEvent, NSEventModifierFlags};

    // `+[NSEvent modifierFlags]`, not an event's own — the trailing `_class` is objc2 telling the two
    // apart. The class's answer is the keys as they are now, which is what is being asked, and it
    // needs no event to have been delivered to us.
    let flags = NSEvent::modifierFlags_class();
    (
        flags.contains(NSEventModifierFlags::Option),
        flags.contains(NSEventModifierFlags::Command),
    )
}

#[cfg(target_os = "windows")]
async fn held(_app: tauri::AppHandle) -> (bool, bool) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_SHIFT};

    // The high bit is "down now"; the low one is "was pressed since the last ask", which would
    // answer for a key the reader had already let go of.
    let down = |vk: u16| (unsafe { GetAsyncKeyState(i32::from(vk)) } as u16 & 0x8000) != 0;
    (down(VK_CONTROL), down(VK_SHIFT))
}

/// Linux reads the keymap, which is GTK, which is the main thread's alone — the same fence the file
/// chooser is behind (`crate::open_with`). So the reading is posted there and waited for, off the
/// thread this command was given, for the moment it takes.
#[cfg(target_os = "linux")]
async fn held(app: tauri::AppHandle) -> (bool, bool) {
    use std::sync::mpsc::sync_channel;
    use std::time::Duration;

    use tauri::Manager;

    /// Long enough for a main loop that is drawing, short enough that a drop is still answered.
    const GRACE: Duration = Duration::from_millis(200);

    let (tx, rx) = sync_channel::<(bool, bool)>(1);
    let posted = app.run_on_main_thread(move || {
        use gtk::gdk::{Display, Keymap, ModifierType};
        use gtk::prelude::*;

        let state = Display::default()
            .and_then(|display| Keymap::for_display(&display))
            .map(|keymap| keymap.modifier_state())
            .unwrap_or_else(ModifierType::empty);
        let _ = tx.send((
            state.contains(ModifierType::CONTROL_MASK),
            state.contains(ModifierType::SHIFT_MASK),
        ));
    });
    if posted.is_err() {
        return (false, false);
    }
    tauri::async_runtime::spawn_blocking(move || rx.recv_timeout(GRACE).unwrap_or((false, false)))
        .await
        .unwrap_or((false, false))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tie is the only part of this that is a decision rather than a reading, so it is the part
    /// written down: holding both keys copies, because a copy is the one of the two that cannot
    /// lose the original.
    #[test]
    fn holding_both_keys_copies_rather_than_moves() {
        assert!(matches!(effect_of(true, true), DropEffectDto::Copy));
        assert!(matches!(effect_of(true, false), DropEffectDto::Copy));
        assert!(matches!(effect_of(false, true), DropEffectDto::Move));
        assert!(matches!(effect_of(false, false), DropEffectDto::Default));
    }
}
