// An input device for driving a macOS webview app from the outside (GUI checks during development).
//
// AppleScript (System Events) cannot enumerate a Tauri webview window — not even `window 1` resolves
// — whereas a CGEvent posted directly reaches the webview. The accessibility permission is enough on
// the parent app that launches this (a terminal, an editor); `AXIsProcessTrustedWithOptions` reports
// whether it is there.
//
// This tool offers only clicking, typing, sending a key, locating a window, and finding an element by
// the name it carries on screen. It holds no notion of what to operate in which order: an app-specific
// sequence burned in here would go false every time the UI moves.
//
// Usage:
//   swift uiauto.swift window <pid>      the app's window id and bounds (the id goes to `screencapture -l`)
//   swift uiauto.swift find <pid> [name] every named element on screen, or the ones of that exact name
//   swift uiauto.swift click-named <pid> <name>  left-click the centre of the one element of that name
//   swift uiauto.swift click <x> <y>     left-click at a screen point
//   swift uiauto.swift dblclick <x> <y>  double-click at a screen point (what opens a file dialog's row)
//   swift uiauto.swift type "text"       type into the focused element (Unicode direct, so no IME)
//   swift uiauto.swift key <keycode>     one virtual keycode (36=Return / 48=Tab / 53=Esc / 125=Down / 126=Up)
//   swift uiauto.swift trusted           whether the accessibility permission is granted (prompts if not)
//
// Reach for `click-named` over a point read off a screenshot. A point costs two conversions that a name
// costs neither of: the shot's pixels are the window's points times the scale of *that* display, which
// is 2 on a built-in panel and 1 on an external one, and the screen may have reflowed since the shot
// was taken — opening the right pane moves a column header by tens of pixels. An element wider than the
// error still swallows both, so the two go unnoticed until something small is aimed at.
//
// Coordinates, when a point is what there is: divide the shot's pixels by its scale (the png's width
// over the width `window` prints, not an assumption about the display), then add the window origin.
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

/// One element on screen: what it is, what it is called, and where it sits.
struct Element {
    let role: String
    let name: String
    let frame: CGRect
}

func axAttribute(_ el: AXUIElement, _ name: String) -> AnyObject? {
    var value: AnyObject?
    return AXUIElementCopyAttributeValue(el, name as CFString, &value) == .success ? value : nil
}

func axString(_ el: AXUIElement, _ name: String) -> String? {
    axAttribute(el, name) as? String
}

func axFrame(_ el: AXUIElement) -> CGRect? {
    guard let p = axAttribute(el, kAXPositionAttribute as String),
          let s = axAttribute(el, kAXSizeAttribute as String) else { return nil }
    var origin = CGPoint.zero
    var size = CGSize.zero
    guard AXValueGetValue(p as! AXValue, .cgPoint, &origin),
          AXValueGetValue(s as! AXValue, .cgSize, &size) else { return nil }
    return CGRect(origin: origin, size: size)
}

/// The name an element answers to. A control carries it as its title, an image as its description, and a
/// piece of text as its value, so all three are read and the first one there wins — that is the single
/// string a person reading the screen would call the thing.
func axName(_ el: AXUIElement) -> String? {
    for attribute in [kAXTitleAttribute, kAXDescriptionAttribute, kAXValueAttribute] {
        if let s = axString(el, attribute as String), !s.isEmpty { return s }
    }
    return nil
}

/// A webview keeps its contents out of the accessibility tree until a client asks for them, and answers
/// with the window's frame alone until then. Setting this is the asking; the answer it returns is not
/// the point (a webview declines to hold the attribute and serves the tree regardless), and the tree
/// stays served for the rest of the app's life, so doing it on every run costs one call and no state.
func openTree(_ app: AXUIElement) {
    AXUIElementSetAttributeValue(app, "AXEnhancedUserInterface" as CFString, kCFBooleanTrue)
    usleep(300_000) // the contents arrive from the web process, not from the call
}

/// Every named element under `el`, in the order the tree holds them. `matching` narrows to one exact
/// name — exact rather than partial because a partial match makes "Create" find "Create a project" too,
/// and a click on the wrong one of two is the failure this whole route exists to remove.
func elements(under el: AXUIElement, matching wanted: String?, depth: Int = 0) -> [Element] {
    guard depth < 60 else { return [] } // a tree deeper than this is a cycle, not a screen
    var found: [Element] = []
    if let name = axName(el), wanted == nil || name == wanted!, let frame = axFrame(el) {
        let role = axString(el, kAXRoleAttribute as String) ?? ""
        found.append(Element(role: role.isEmpty ? "?" : role, name: name, frame: frame))
    }
    for child in axAttribute(el, kAXChildrenAttribute as String) as? [AXUIElement] ?? [] {
        found += elements(under: child, matching: wanted, depth: depth + 1)
    }
    return found
}

/// The elements of one app, menu bar left out: it belongs to whichever app is frontmost rather than to
/// the window under test, and its items carry names that collide with the screen's own.
func appElements(pid: Int, matching wanted: String?) -> [Element] {
    let app = AXUIElementCreateApplication(pid_t(pid))
    openTree(app)
    let windows = axAttribute(app, kAXWindowsAttribute as String) as? [AXUIElement] ?? []
    if windows.isEmpty { fail("no window for pid \(pid) — is the app running and on screen?") }
    return windows.flatMap { elements(under: $0, matching: wanted) }
}

func find(pid: Int, name: String?) {
    let found = appElements(pid: pid, matching: name)
    for e in found {
        print("\(e.role)\t\(e.name)\t\(Int(e.frame.minX)) \(Int(e.frame.minY)) \(Int(e.frame.width)) \(Int(e.frame.height))")
    }
    if found.isEmpty { fail("nothing on screen is called \(name ?? "anything")") }
}

/// Click where that name is on screen.
///
/// A name is often on more than one element without being ambiguous: a link and the text inside it both
/// answer to it, and they sit one on top of the other, so a click anywhere they overlap reaches whatever
/// is uppermost there — which is the thing a person clicking the word would have hit. So the point aimed
/// at is where every element of that name overlaps. Two of a name in two *places* has no such point, and
/// that is refused rather than guessed at: `find` is how to see both and aim at one.
///
/// The click is a real one, at where the element stands now: what the name saves is the arithmetic, not
/// the input path, and a press delivered through the accessibility API would go through whether or not
/// anything was covering the element.
func clickNamed(pid: Int, name: String) {
    let found = appElements(pid: pid, matching: name)
    guard let first = found.first else { fail("nothing on screen is called \(name)") }
    let overlap = found.dropFirst().reduce(first.frame) { $0.intersection($1.frame) }
    if overlap.isNull || overlap.isEmpty {
        let places = found.map { "\(Int($0.frame.minX)),\(Int($0.frame.minY))" }.joined(separator: " ")
        fail("\(found.count) elements are called \(name), in different places — at \(places); click a point instead")
    }
    click(x: overlap.midX, y: overlap.midY)
}

/// Move onto the point before pressing, so an element that expects a hover first (a button's hover
/// state) is not missed.
func hover(_ p: CGPoint) {
    CGEvent(mouseEventSource: src, mouseType: .mouseMoved, mouseCursorPosition: p, mouseButton: .left)?
        .post(tap: .cghidEventTap)
    usleep(120_000)
}

/// One down/up at a point, saying which press of a run it is. That number is the whole difference
/// between two clicks and a double click: the events are otherwise identical, and what is listening
/// reads the count off the field rather than timing the pair itself.
func press(at p: CGPoint, clickState: Int64) {
    for phase in [CGEventType.leftMouseDown, CGEventType.leftMouseUp] {
        let e = CGEvent(mouseEventSource: src, mouseType: phase, mouseCursorPosition: p, mouseButton: .left)
        e?.setIntegerValueField(.mouseEventClickState, value: clickState)
        e?.post(tap: .cghidEventTap)
        usleep(60_000)
    }
}

func click(x: Double, y: Double) {
    let p = CGPoint(x: x, y: y)
    hover(p)
    press(at: p, clickState: 1)
}

/// A native open/save dialog opens the row you are pointing at on a double click and on nothing else
/// — a single click only selects it, and Return does not reach that dialog from here at all. So
/// picking a file out of one needs this.
func doubleClick(x: Double, y: Double) {
    let p = CGPoint(x: x, y: y)
    hover(p)
    press(at: p, clickState: 1)
    press(at: p, clickState: 2)
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
guard args.count >= 2 else { fail("usage: uiauto <window|find|click-named|click|dblclick|type|key|trusted> …") }

switch args[1] {
case "window":
    guard args.count == 3, let pid = Int(args[2]) else { fail("usage: uiauto window <pid>") }
    windows(pid: pid)
case "find":
    guard args.count == 3 || args.count == 4, let pid = Int(args[2]) else { fail("usage: uiauto find <pid> [name]") }
    find(pid: pid, name: args.count == 4 ? args[3] : nil)
case "click-named":
    guard args.count == 4, let pid = Int(args[2]) else { fail("usage: uiauto click-named <pid> <name>") }
    clickNamed(pid: pid, name: args[3])
case "click":
    guard args.count == 4, let x = Double(args[2]), let y = Double(args[3]) else { fail("usage: uiauto click <x> <y>") }
    click(x: x, y: y)
case "dblclick":
    guard args.count == 4, let x = Double(args[2]), let y = Double(args[3]) else { fail("usage: uiauto dblclick <x> <y>") }
    doubleClick(x: x, y: y)
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
