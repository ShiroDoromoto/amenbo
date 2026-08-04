// screen — drive a mac screen from the outside and read what is on it: bring an app to the front,
// click, type, send a key, shoot its window, and read the text off a shot.
//
// One tool rather than one per caller. What a caller needs of a screen is the moves, a shot and the
// words on it — never a window id or its bounds, which is why neither leaves here: the tool shoots
// the window itself. A format nobody is handed is a format nobody parses, and two callers cannot
// drift apart over one they never see.
//
// Accessibility is what reaches a webview. AppleScript (System Events) cannot enumerate a Tauri
// window — not even `window 1` resolves — whereas a CGEvent posted directly arrives. The permission
// is granted on the parent process that runs this (a terminal, an editor), and `trusted` reports
// whether it is there; shooting needs Screen Recording on that same parent.
//
// Usage:
//   swift screen.swift front <pid>               bring the app owning that pid to the front
//   swift screen.swift shot <pid> <out.png>      shoot that app's window into a png
//   swift screen.swift read <image.png>          the words on a shot, as JSON: corrected, and as read
//   swift screen.swift find <pid> [name]         every named element on screen, or those that name reaches
//   swift screen.swift click-named <pid> <name>  left-click what that name names (a part of it will do)
//   swift screen.swift click <x> <y>             left-click at a screen point
//   swift screen.swift dblclick <x> <y>          double-click at a screen point (what opens a dialog's row)
//   swift screen.swift type "text"               type into the focused element (Unicode direct, so no IME)
//   swift screen.swift key <keycode>             one virtual keycode (36=Return / 48=Tab / 53=Esc / 125=Down / 126=Up)
//   swift screen.swift trusted                   whether the accessibility permission is granted (prompts if not)
//
// Reach for `click-named` over a point. A point costs two conversions a name costs neither of: a
// shot's pixels are the window's points times the scale of *that* display, which is 2 on a built-in
// panel and 1 on an external one, and the screen may have reflowed since the shot was taken —
// opening the right pane moves a column header by tens of pixels. An element wider than the error
// swallows both, so the two go unnoticed until something small is aimed at.
//
// The tool holds no notion of what to operate in which order: an app-specific sequence burned in
// here would go false every time the UI moves.

import AppKit
import ApplicationServices
import CoreGraphics
import Foundation
import Vision

let src = CGEventSource(stateID: .hidSystemState)

func fail(_ msg: String) -> Never {
    FileHandle.standardError.write("screen: \(msg)\n".data(using: .utf8)!)
    exit(1)
}

// ---------------------------------------------------------------------------
// The window: found here, shot here, and never handed out
// ---------------------------------------------------------------------------

/// windowID answers with the substantial window of the given pid — the real one, not a title bar or
/// a shadow. A window behind another Space is not counted as on-screen, so an app that has not been
/// fronted comes back empty.
func windowID(pid: Int) -> Int {
    guard let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] else {
        fail("could not read the window list")
    }
    for w in list {
        guard let owner = w[kCGWindowOwnerPID as String] as? Int, owner == pid,
              let id = w[kCGWindowNumber as String] as? Int,
              let b = w[kCGWindowBounds as String] as? [String: Any],
              let height = b["Height"] as? Double,
              height > 200 // drop the incidental windows: shadows, tooltips and the like
        else { continue }
        return id
    }
    fail("no window for pid \(pid) — is the app running, on screen and in front?")
}

/// Bring the app owning that pid to the front, so its window counts as on-screen. By pid and not by
/// name: two builds of one app carry the same name, and the pid is what every other subcommand here
/// is aimed with.
func front(pid: Int) {
    guard let app = NSRunningApplication(processIdentifier: pid_t(pid)) else {
        fail("no running application with pid \(pid)")
    }
    if #available(macOS 14.0, *) {
        app.activate()
    } else {
        app.activate(options: [.activateIgnoringOtherApps])
    }
    usleep(400_000) // the window arrives in front after the call returns, not with it
}

/// Shoot the app's window into `path`. The id `screencapture -l` takes is resolved and spent here,
/// so a caller shoots by pid and holds no window of its own.
func shot(pid: Int, path: String) {
    let id = windowID(pid: pid)
    let p = Process()
    p.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture")
    // -x is silent, -l shoots the one window rather than a display (a window on a second monitor is
    // otherwise somebody else's screen), -o leaves the shadow out — a shadow is asymmetric, so with
    // it the png's pixels stop corresponding to screen points by any fixed offset.
    p.arguments = ["-x", "-o", "-l", String(id), path]
    do {
        try p.run()
    } catch {
        fail("could not run screencapture: \(error)")
    }
    p.waitUntilExit()
    guard p.terminationStatus == 0 else {
        fail("screencapture exited with \(p.terminationStatus)")
    }
    guard let size = try? FileManager.default.attributesOfItem(atPath: path)[.size] as? Int, size > 0 else {
        fail("screencapture wrote nothing to \(path) — is Screen Recording granted?")
    }
}

// ---------------------------------------------------------------------------
// Reading the words off a shot (Vision), and the correction Vision needs
// ---------------------------------------------------------------------------

/// The dashes Unicode files under letters. A long vowel mark is what Vision most often returns for
/// an em dash on a Japanese screen, and it is alphanumeric where every other dash is punctuation —
/// so it would survive the fold on the read side while the dash it stands for is dropped on the
/// expectation's, and the two halves of one title would stop matching. Dropped here, a title that
/// really carries one still matches itself.
let dashesFiledAsLetters: Set<Character> = ["\u{30FC}", "\u{FF70}"]

/// Fold a reading to the part of it this reader can be held to: the words, not the glyphs. Vision
/// reads the words on a card reliably and the punctuation between them however it likes — an em dash
/// comes back as a hyphen, a space, a long vowel mark, or nothing — so a verbatim comparison fails on
/// a title no human would call misread. Case goes the same way, and a line break where the card
/// wrapped folds to the single space the title was written with. Alphanumerics are what survives,
/// Japanese included: the screen under test is in Japanese and is judged by this same rule.
///
/// This is the correction of one reader's habits, so it lives with the reader — the caller is handed
/// the folded reading and the unfolded one both, and folds its own expectation by the same rule
/// before matching (`verification/gui/src/lib.rs`).
func fold(_ s: String) -> String {
    var out = ""
    var pendingSpace = false
    for c in s {
        if (c.isLetter || c.isNumber) && !dashesFiledAsLetters.contains(c) {
            if pendingSpace && !out.isEmpty { out.append(" ") }
            pendingSpace = false
            out += c.lowercased()
        } else {
            pendingSpace = true
        }
    }
    return out
}

/// Read the text off a screenshot and print it as JSON: `text` is the reading folded, `raw` is what
/// Vision handed back — one line per region it recognized. Both, because the fold is what a caller
/// matches on and the raw is what a person reads when a match fails. A region Vision cannot read is
/// simply absent, which is the honest answer for an assert that expected words there.
func readText(path: String) {
    guard let data = FileManager.default.contents(atPath: path) else {
        fail("could not read \(path)")
    }
    guard let source = CGImageSourceCreateWithData(data as CFData, nil),
          let image = CGImageSourceCreateImageAtIndex(source, 0, nil)
    else {
        fail("could not decode an image from \(path)")
    }

    let request = VNRecognizeTextRequest()
    // Accurate over fast: the board's card titles are small, and a missed character turns a present
    // card into an absent verdict. Language correction stays off — the text under test is titles and
    // ids (e.g. "SCENARIO SEED"), not prose a dictionary should second-guess.
    request.recognitionLevel = .accurate
    request.usesLanguageCorrection = false
    // The app renders both scripts; a scenario seeds English titles but the surrounding UI is
    // Japanese. The order is not a list of what to accept, it is which language leads: asked for
    // English first, Vision reads a Japanese line as mangled Latin and drops the words inside it — a
    // file name in the middle of a Japanese sentence comes back as nothing at all, which is the shape
    // of an assert failing over text plainly on screen. Japanese leads, and the Latin around it —
    // ids, paths, file names, English card titles — is read as well as it was, or better.
    request.recognitionLanguages = ["ja-JP", "en-US"]

    let handler = VNImageRequestHandler(cgImage: image, options: [:])
    do {
        try handler.perform([request])
    } catch {
        fail("Vision could not run text recognition: \(error)")
    }

    let lines = (request.results ?? []).compactMap { $0.topCandidates(1).first?.string }
    let raw = lines.joined(separator: "\n")
    let json = ["text": fold(raw), "raw": raw]
    guard let out = try? JSONSerialization.data(withJSONObject: json, options: [.sortedKeys]) else {
        fail("could not encode the reading as JSON")
    }
    FileHandle.standardOutput.write(out)
    FileHandle.standardOutput.write("\n".data(using: .utf8)!)
}

// ---------------------------------------------------------------------------
// The elements on screen, and the moves aimed at them
// ---------------------------------------------------------------------------

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

/// The name an element answers to. A control carries it as its title, an image as its description,
/// and a piece of text as its value, so all three are read and the first one there wins — that is the
/// single string a person reading the screen would call the thing.
func axName(_ el: AXUIElement) -> String? {
    for attribute in [kAXTitleAttribute, kAXDescriptionAttribute, kAXValueAttribute] {
        if let s = axString(el, attribute as String), !s.isEmpty { return s }
    }
    return nil
}

/// A webview keeps its contents out of the accessibility tree until a client asks for them, and
/// answers with the window's frame alone until then. Setting this is the asking; the answer it
/// returns is not the point (a webview declines to hold the attribute and serves the tree
/// regardless), and the tree stays served for the rest of the app's life, so doing it on every run
/// costs one call and no state.
func openTree(_ app: AXUIElement) {
    AXUIElementSetAttributeValue(app, "AXEnhancedUserInterface" as CFString, kCFBooleanTrue)
    usleep(300_000) // the contents arrive from the web process, not from the call
}

/// Every named element under `el`, in the order the tree holds them.
func elements(under el: AXUIElement, depth: Int = 0) -> [Element] {
    guard depth < 60 else { return [] } // a tree deeper than this is a cycle, not a screen
    var found: [Element] = []
    if let name = axName(el), let frame = axFrame(el) {
        let role = axString(el, kAXRoleAttribute as String) ?? ""
        found.append(Element(role: role.isEmpty ? "?" : role, name: name, frame: frame))
    }
    for child in axAttribute(el, kAXChildrenAttribute as String) as? [AXUIElement] ?? [] {
        found += elements(under: child, depth: depth + 1)
    }
    return found
}

/// The elements of one app, menu bar left out: it belongs to whichever app is frontmost rather than
/// to the window under test, and its items carry names that collide with the screen's own.
func appElements(pid: Int) -> [Element] {
    let app = AXUIElementCreateApplication(pid_t(pid))
    openTree(app)
    let windows = axAttribute(app, kAXWindowsAttribute as String) as? [AXUIElement] ?? []
    if windows.isEmpty { fail("no window for pid \(pid) — is the app running and on screen?") }
    return windows.flatMap { elements(under: $0) }
}

/// The elements `wanted` names: the ones called exactly that, or — when the screen carries no such
/// name — the ones whose name holds it.
///
/// Partial is needed because the name an element answers to is not the label a person reads off the
/// screen. An emoji in front of the words is part of it (`🪿 はじめに` for the sidebar item that
/// reads はじめに), and a card folds every line it shows into one string. Neither is knowable before
/// looking, so an exact-only match sends every caller through a full listing to copy a name out of,
/// for a thing it can already see.
///
/// Exact first, and not merely as an optimization: a whole name is the caller saying which one, and
/// a screen where one name is also a part of another (a column called 未着手, over cards whose names
/// end in it) would otherwise have no way to name the shorter.
func named(_ wanted: String, among all: [Element]) -> [Element] {
    let exact = all.filter { $0.name == wanted }
    return exact.isEmpty ? all.filter { $0.name.contains(wanted) } : exact
}

/// A card's name carries the line breaks of the lines it folded, so it is shown escaped rather than
/// broken across the message it is listed in.
func oneLine(_ s: String) -> String {
    s.replacingOccurrences(of: "\n", with: "\\n")
}

func find(pid: Int, name: String?) {
    let all = appElements(pid: pid)
    let found = name.map { named($0, among: all) } ?? all
    for e in found {
        print("\(e.role)\t\(e.name)\t\(Int(e.frame.minX)) \(Int(e.frame.minY)) \(Int(e.frame.width)) \(Int(e.frame.height))")
    }
    if found.isEmpty { fail("nothing on screen is called \(name ?? "anything")") }
}

/// Click where that name is on screen.
///
/// A name is often on more than one element without being ambiguous: a link and the text inside it
/// both answer to it, and they sit one on top of the other, so a click anywhere they overlap reaches
/// whatever is uppermost there — which is the thing a person clicking the word would have hit. So the
/// point aimed at is where every element of that name overlaps. Two of a name in two *places* has no
/// such point, and that is refused rather than guessed at: `find` is how to see both and aim at one.
///
/// Matching a part of a name (above) is what puts *several* names within reach of one word, and that
/// is a different ambiguity: ステータス is a label, a filter's pop-up and a group's button; 未着手 is
/// a column header and every card standing under it. Nothing here can tell which was meant, so the
/// names it reached are printed and nothing is pressed — pressing the first would be a click on
/// whatever the tree happened to hold first, reported as success.
///
/// The click is a real one, at where the element stands now: what the name saves is the arithmetic,
/// not the input path, and a press delivered through the accessibility API would go through whether
/// or not anything was covering the element.
func clickNamed(pid: Int, name: String) {
    let found = named(name, among: appElements(pid: pid))
    guard let first = found.first else { fail("nothing on screen is called \(name)") }

    var reached: [String] = []
    for e in found where !reached.contains(e.name) { reached.append(e.name) }
    if reached.count > 1 {
        let names = reached.map(oneLine).joined(separator: " / ")
        fail("\(reached.count) names on screen hold \(name) — \(names); name one of them, or more of the one meant")
    }

    let overlap = found.dropFirst().reduce(first.frame) { $0.intersection($1.frame) }
    if overlap.isNull || overlap.isEmpty {
        let places = found.map { "\(Int($0.frame.minX)),\(Int($0.frame.minY))" }.joined(separator: " ")
        fail("\(found.count) elements are called \(oneLine(first.name)), in different places — at \(places); click a point instead")
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
guard args.count >= 2 else {
    fail("usage: screen <front|shot|read|find|click-named|click|dblclick|type|key|trusted> …")
}

switch args[1] {
case "front":
    guard args.count == 3, let pid = Int(args[2]) else { fail("usage: screen front <pid>") }
    front(pid: pid)
case "shot":
    guard args.count == 4, let pid = Int(args[2]) else { fail("usage: screen shot <pid> <out.png>") }
    shot(pid: pid, path: args[3])
case "read":
    guard args.count == 3 else { fail("usage: screen read <image.png>") }
    readText(path: args[2])
case "find":
    guard args.count == 3 || args.count == 4, let pid = Int(args[2]) else { fail("usage: screen find <pid> [name]") }
    find(pid: pid, name: args.count == 4 ? args[3] : nil)
case "click-named":
    guard args.count == 4, let pid = Int(args[2]) else { fail("usage: screen click-named <pid> <name>") }
    clickNamed(pid: pid, name: args[3])
case "click":
    guard args.count == 4, let x = Double(args[2]), let y = Double(args[3]) else { fail("usage: screen click <x> <y>") }
    click(x: x, y: y)
case "dblclick":
    guard args.count == 4, let x = Double(args[2]), let y = Double(args[3]) else { fail("usage: screen dblclick <x> <y>") }
    doubleClick(x: x, y: y)
case "type":
    guard args.count == 3 else { fail("usage: screen type <text>") }
    type(args[2])
case "key":
    guard args.count == 3, let code = UInt16(args[2]) else { fail("usage: screen key <keycode>") }
    key(CGKeyCode(code))
case "trusted":
    // Without the permission, raise the dialog that leads to System Settings. Granting it does not
    // require restarting the parent app.
    let opts = [kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String: true] as CFDictionary
    print(AXIsProcessTrustedWithOptions(opts) ? "trusted" : "not trusted")
default:
    fail("unknown action \(args[1])")
}

// Grace for the posted events to reach the app: a screenshot taken right after can otherwise outrun
// them.
usleep(150_000)
