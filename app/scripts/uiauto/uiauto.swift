// An input device for driving a macOS webview app from the outside (GUI checks during development).
//
// AppleScript (System Events) cannot enumerate a Tauri webview window — not even `window 1` resolves
// — whereas a CGEvent posted directly reaches the webview. The accessibility permission is enough on
// the parent app that launches this (a terminal, an editor); `AXIsProcessTrustedWithOptions` reports
// whether it is there.
//
// This tool offers only clicking, typing, sending a key, and locating a window. It holds no notion of
// what to operate in which order: an app-specific sequence burned in here would go false every time
// the UI moves.
//
// Usage:
//   swift uiauto.swift window <pid>      the app's window id and bounds (the id goes to `screencapture -l`)
//   swift uiauto.swift click <x> <y>     left-click at a screen point
//   swift uiauto.swift type "text"       type into the focused element (Unicode direct, so no IME)
//   swift uiauto.swift key <keycode>     one virtual keycode (36=Return / 48=Tab / 53=Esc / 125=Down / 126=Up)
//   swift uiauto.swift trusted           whether the accessibility permission is granted (prompts if not)
//
// Coordinates: take the pixels `screencapture` returns, halve them on Retina, and add the window
// origin (the x/y `window` prints) to reach a screen point. Click coordinates are those screen points.
//
// Bring the target app to the front first (`osascript -e 'tell application "…" to activate'`). A window
// behind another Space is not counted as on-screen, so `window` comes back empty and a click lands in
// whatever app is in front instead.

import ApplicationServices
import CoreGraphics
import Foundation

let src = CGEventSource(stateID: .hidSystemState)

func fail(_ msg: String) -> Never {
    FileHandle.standardError.write("uiauto: \(msg)\n".data(using: .utf8)!)
    exit(1)
}

/// windows prints the substantial windows of the given pid — the real ones, not a title bar or a shadow.
func windows(pid: Int) {
    guard let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] else {
        fail("could not read the window list")
    }
    var found = false
    for w in list {
        guard let owner = w[kCGWindowOwnerPID as String] as? Int, owner == pid,
              let id = w[kCGWindowNumber as String] as? Int,
              let b = w[kCGWindowBounds as String] as? [String: Any],
              let x = b["X"] as? Double, let y = b["Y"] as? Double,
              let width = b["Width"] as? Double, let height = b["Height"] as? Double,
              height > 200 // drop the incidental windows: shadows, tooltips and the like
        else { continue }
        print("\(id) \(x) \(y) \(width) \(height)")
        found = true
    }
    if !found { fail("no window for pid \(pid) — is the app running and on screen?") }
}

func click(x: Double, y: Double) {
    let p = CGPoint(x: x, y: y)
    // Move once before pressing, so an element that expects a hover first (a button's hover state)
    // is not missed.
    CGEvent(mouseEventSource: src, mouseType: .mouseMoved, mouseCursorPosition: p, mouseButton: .left)?
        .post(tap: .cghidEventTap)
    usleep(120_000)
    CGEvent(mouseEventSource: src, mouseType: .leftMouseDown, mouseCursorPosition: p, mouseButton: .left)?
        .post(tap: .cghidEventTap)
    usleep(60_000)
    CGEvent(mouseEventSource: src, mouseType: .leftMouseUp, mouseCursorPosition: p, mouseButton: .left)?
        .post(tap: .cghidEventTap)
}

/// type sends the string itself rather than keycodes. It bypasses the IME, so any script goes in as-is.
func type(_ s: String) {
    for ch in s {
        let utf16 = Array(String(ch).utf16)
        for down in [true, false] {
            let e = CGEvent(keyboardEventSource: src, virtualKey: 0, keyDown: down)
            e?.keyboardSetUnicodeString(stringLength: utf16.count, unicodeString: utf16)
            e?.post(tap: .cghidEventTap)
        }
        usleep(12_000) // enough of a gap that the webview's input handler misses nothing
    }
}

func key(_ code: CGKeyCode) {
    CGEvent(keyboardEventSource: src, virtualKey: code, keyDown: true)?.post(tap: .cghidEventTap)
    usleep(40_000)
    CGEvent(keyboardEventSource: src, virtualKey: code, keyDown: false)?.post(tap: .cghidEventTap)
}

let args = CommandLine.arguments
guard args.count >= 2 else { fail("usage: uiauto <window|click|type|key|trusted> …") }

switch args[1] {
case "window":
    guard args.count == 3, let pid = Int(args[2]) else { fail("usage: uiauto window <pid>") }
    windows(pid: pid)
case "click":
    guard args.count == 4, let x = Double(args[2]), let y = Double(args[3]) else { fail("usage: uiauto click <x> <y>") }
    click(x: x, y: y)
case "type":
    guard args.count == 3 else { fail("usage: uiauto type <text>") }
    type(args[2])
case "key":
    guard args.count == 3, let code = UInt16(args[2]) else { fail("usage: uiauto key <keycode>") }
    key(CGKeyCode(code))
case "trusted":
    // Without the permission, raise the dialog that leads to System Settings. Granting it does not
    // require restarting the parent app.
    let opts = [kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String: true] as CFDictionary
    print(AXIsProcessTrustedWithOptions(opts) ? "trusted" : "not trusted")
default:
    fail("unknown action \(args[1])")
}

// Grace for the posted events to reach the app: a screenshot taken right after can otherwise outrun them.
usleep(150_000)
