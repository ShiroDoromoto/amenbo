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
//   swift screen.swift click-named <pid> <name>  left-click what that name names (fronts the app first)
//   swift screen.swift click <x> <y>             left-click at a screen point
//   swift screen.swift dblclick <x> <y>          double-click at a screen point (what opens a dialog's row)
//   swift screen.swift type "text"               type into the focused element (Unicode direct, so no IME)
//   swift screen.swift key <keycode>             one virtual keycode (36=Return / 48=Tab / 53=Esc / 125=Down / 126=Up)
//   swift screen.swift trusted                   whether the accessibility permission is granted (prompts if not)
//
// Anything aimed at a pid also takes `--window <title>`, anywhere in the line. An app draws one
// screen per window, so a pid alone stops naming a screen the moment it has two: `front` raises the
// window named, `shot` shoots it, and `find` / `click-named` read and press inside it and nowhere
// else. Left out, it means the app's one window — and an app with two is told to say which rather
// than answered with whichever the list held first. That silence is the failure worth refusing: a
// road reading the wrong window finds nothing it expected and comes out red for a reason nobody can
// see, or finds a name both windows carry and comes out green without having looked at the screen
// under test.
//
// Reach for `click-named` over a point. A point costs two conversions a name costs neither of: a
// shot's pixels are the window's points times the scale of *that* display, which is 2 on a built-in
// panel and 1 on an external one, and the screen may have reflowed since the shot was taken —
// opening the right pane moves a column header by tens of pixels. An element wider than the error
// swallows both, so the two go unnoticed until something small is aimed at.
//
// A click lands on whatever is frontmost where it is aimed, so `click-named` brings its app to the
// front before pressing — the one action here that does not have to be sequenced by its caller.
// `click` and `type` take no pid, so what is in front is still the caller's to know.
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
// The window: named here, found here, shot here, and never handed out
// ---------------------------------------------------------------------------

/// The one of `all` that `wanted` names — exact first and then by part, the same rule a name reaches
/// an element with (`named`), and for the same reason: one window's title is often the start of
/// another's ("Amenbo", over "Amenbo — Talk"), so a whole title has to be able to name the shorter.
///
/// Nothing is guessed at either end. A name that reaches none is a caller naming a window this app
/// does not have; a name that reaches two, and no name at all when two are up, is a caller who has
/// not said which. Both are answered with the titles that are up, because the caller can then write
/// one down — where picking whichever the list held first would be a screen chosen by an ordering
/// nobody controls, reported as the screen that was asked for.
func theWindow<W>(_ wanted: String?, among all: [W], titled title: (W) -> String, of pid: Int) -> W {
    func listing(_ ws: [W]) -> String { ws.map { "\"\(oneLine(title($0)))\"" }.joined(separator: " / ") }
    if all.isEmpty { fail("no window for pid \(pid) — is the app running, on screen and in front?") }
    guard let wanted else {
        if all.count == 1 { return all[0] }
        fail("pid \(pid) has \(all.count) windows on screen — \(listing(all)); say which with --window <title>")
    }
    let exact = all.filter { title($0) == wanted }
    let found = exact.isEmpty ? all.filter { title($0).contains(wanted) } : exact
    if found.isEmpty { fail("no window of pid \(pid) is called \(wanted) — it has \(listing(all))") }
    if found.count > 1 {
        fail("\(found.count) windows of pid \(pid) hold \(wanted) — \(listing(found)); name one of them in full")
    }
    return found[0]
}

/// The title drawn in a window's bar. It is the only name a window carries out of the app: the label
/// the app knows its own windows by does not reach the accessibility tree at all, and the address
/// behind a webview is not something a person reads off the screen and could write in a road.
func windowTitle(_ w: AXUIElement) -> String {
    axString(w, kAXTitleAttribute as String) ?? ""
}

/// The windows of an app that a person would say are windows.
///
/// A webview app keeps panels beside its real ones — a shadow, a drag surface — and they arrive in
/// the same list, untitled and filed under `AXUnknown`. Counting them would make an app with a single
/// window look ambiguous, and then every call would have to name a window that has no name.
func standardWindows(of app: AXUIElement) -> [AXUIElement] {
    let all = axAttribute(app, kAXWindowsAttribute as String) as? [AXUIElement] ?? []
    return all.filter { axString($0, kAXSubroleAttribute as String) == (kAXStandardWindowSubrole as String) }
}

/// The id `screencapture -l` takes, for the window the caller named. Resolved and spent inside `shot`,
/// so a caller shoots by pid and holds no window of its own. A window behind another Space is not
/// counted as on-screen, so an app that has not been fronted comes back empty.
///
/// The titles come off the window list, which serves them only to a process granted Screen Recording
/// — the same permission shooting needs, so a run that can take the picture can also say which window
/// it took.
func windowID(pid: Int, named wanted: String?) -> Int {
    guard let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] else {
        fail("could not read the window list")
    }
    var windows: [(id: Int, title: String)] = []
    for w in list {
        guard let owner = w[kCGWindowOwnerPID as String] as? Int, owner == pid,
              let id = w[kCGWindowNumber as String] as? Int,
              let b = w[kCGWindowBounds as String] as? [String: Any],
              let height = b["Height"] as? Double,
              height > 200 // drop the incidental windows: shadows, tooltips and the like
        else { continue }
        windows.append((id, w[kCGWindowName as String] as? String ?? ""))
    }
    return theWindow(wanted, among: windows, titled: { $0.title }, of: pid).id
}

/// Bring the app owning that pid to the front, so its windows count as on-screen, and — when one is
/// named — raise that one within the app. By pid and not by name: two builds of one app carry the
/// same name, and the pid is what every other subcommand here is aimed with.
///
/// Without a name this is the app's move and nothing more, which is unambiguous however many windows
/// it has: what an app being frontmost decides is that its windows are shootable at all. Which of
/// them is in front is a second question, and it is asked only when the caller asks it.
func front(pid: Int, window wanted: String?) {
    guard let app = NSRunningApplication(processIdentifier: pid_t(pid)) else {
        fail("no running application with pid \(pid)")
    }
    if #available(macOS 14.0, *) {
        app.activate()
    } else {
        app.activate(options: [.activateIgnoringOtherApps])
    }
    usleep(400_000) // the window arrives in front after the call returns, not with it
    guard wanted != nil else { return }
    AXUIElementPerformAction(appWindow(pid: pid, named: wanted), kAXRaiseAction as CFString)
    usleep(200_000) // the raise lands after the call returns, as the activation does
}

/// Shoot the window into `path`.
func shot(pid: Int, window: String?, path: String) {
    let id = windowID(pid: pid, named: window)
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
///
/// The contents arrive from the web process afterwards, so the asking is only half of it — what
/// follows is the wait, in appElements below.
func openTree(_ app: AXUIElement) {
    AXUIElementSetAttributeValue(app, "AXEnhancedUserInterface" as CFString, kCFBooleanTrue)
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

/// Whether the webview's contents have arrived under `el`.
///
/// Before they do, the tree of a freshly launched app is the window and the title drawn in it and
/// nothing else — there is no web area at all — so a web area *holding something* is the one signal
/// that separates "not yet" from "this screen really is that empty".
func contentsArrived(_ el: AXUIElement, depth: Int = 0) -> Bool {
    guard depth < 60 else { return false } // a tree deeper than this is a cycle, not a screen
    let children = axAttribute(el, kAXChildrenAttribute as String) as? [AXUIElement] ?? []
    if axString(el, kAXRoleAttribute as String) == "AXWebArea" { return !children.isEmpty }
    return children.contains { contentsArrived($0, depth: depth + 1) }
}

/// The app's window the caller named, waited for until what is drawn in it has arrived.
///
/// The contents are *waited for*, not slept on. A fixed pause was enough on the machine this was
/// written on and not in the verification VM, where the first command after a launch found the
/// window, nothing inside it, and said the screen did not hold the name — while the very next
/// command, over the tree the failed one had opened, found it (measured 2026-08-23, both runs in the
/// guest). One silent miss per launch is the worst shape that failure has: a road is walked once,
/// and it does not press the button a second time to see whether it was true.
///
/// The wait is on *that* window, not on any of them. A second window fills at its own pace, so a
/// wait satisfied by whichever filled first would hand back the named one still empty — the same
/// silent miss, arriving now on the runs where two windows are up.
///
/// A window that never grows a web area — a native panel — costs the whole budget and is then
/// answered with what it has. That is the right way round: this drives a webview app, so waiting on
/// contents that are coming is what happens on every run, and the wait for ones that never come is
/// paid by the exception.
func appWindow(pid: Int, named wanted: String?) -> AXUIElement {
    let app = AXUIElementCreateApplication(pid_t(pid))
    openTree(app)
    let deadline = Date().addingTimeInterval(5)
    var windows: [AXUIElement] = []
    repeat {
        windows = standardWindows(of: app)
        // Resolved on each pass rather than once at the end: until the named window is both there
        // and filled, there is nothing to wait on and nothing to hand back.
        if windows.count == 1 || wanted != nil {
            let named = wanted.map { w in windows.filter { windowTitle($0).contains(w) } } ?? windows
            if named.count == 1, contentsArrived(named[0]) { break }
        }
        usleep(100_000)
    } while Date() < deadline
    return theWindow(wanted, among: windows, titled: windowTitle, of: pid)
}

/// The elements of one window of one app — the menu bar left out, since it belongs to whichever app
/// is frontmost rather than to the window under test and its items carry names that collide with the
/// screen's own, and the app's other windows left out too. Two windows of one app draw two screens,
/// and a name read off the wrong one is a road that passed without looking at the screen it was
/// written for.
func windowElements(pid: Int, window wanted: String?) -> [Element] {
    elements(under: appWindow(pid: pid, named: wanted))
}

/// The elements `wanted` names: the ones called exactly that, or — when the screen carries no such
/// name — the ones whose name holds it.
///
/// Partial is needed because the name an element answers to is not the label a person reads off the
/// screen: a card folds every line it shows into one string, and a glyph still drawn as text in
/// front of the words is part of the name too. Neither is knowable before looking, so an exact-only
/// match sends every caller through a full listing to copy a name out of, for a thing it can
/// already see.
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

func find(pid: Int, name: String?, window: String?) {
    let all = windowElements(pid: pid, window: window)
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
///
/// Which is also why the app is brought to the front first, rather than left to the caller: a real
/// press lands on whatever is frontmost at that point, so anything that took the front — a sleeping
/// display, a permission dialog — swallows the click and the run still exits 0. A shot says so by
/// failing; a click cannot, so it is not asked to.
func clickNamed(pid: Int, name: String, window: String?) {
    front(pid: pid, window: window)
    let found = named(name, among: windowElements(pid: pid, window: window))
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

/// Pull `--window <title>` out of the line, wherever in it the caller wrote it, and hand back what is
/// left as the positional arguments each subcommand already reads. Which window is a qualifier on the
/// aim rather than another thing to name, so it is not given a place in any subcommand's order — a
/// `find <pid> [name]` whose optional name and optional window sat side by side would read a one-word
/// call either way round.
func takeWindow(_ argv: [String]) -> (window: String?, rest: [String]) {
    var window: String?
    var rest: [String] = []
    var i = 0
    while i < argv.count {
        if argv[i] == "--window" {
            guard i + 1 < argv.count else { fail("--window needs the title of a window") }
            window = argv[i + 1]
            i += 2
            continue
        }
        rest.append(argv[i])
        i += 1
    }
    return (window, rest)
}

let (window, args) = takeWindow(CommandLine.arguments)
guard args.count >= 2 else {
    fail("usage: screen <front|shot|read|find|click-named|click|dblclick|type|key|trusted> … [--window <title>]")
}

switch args[1] {
case "front":
    guard args.count == 3, let pid = Int(args[2]) else { fail("usage: screen front <pid> [--window <title>]") }
    front(pid: pid, window: window)
case "shot":
    guard args.count == 4, let pid = Int(args[2]) else { fail("usage: screen shot <pid> <out.png> [--window <title>]") }
    shot(pid: pid, window: window, path: args[3])
case "read":
    guard args.count == 3 else { fail("usage: screen read <image.png>") }
    readText(path: args[2])
case "find":
    guard args.count == 3 || args.count == 4, let pid = Int(args[2]) else { fail("usage: screen find <pid> [name] [--window <title>]") }
    find(pid: pid, name: args.count == 4 ? args[3] : nil, window: window)
case "click-named":
    guard args.count == 4, let pid = Int(args[2]) else { fail("usage: screen click-named <pid> <name> [--window <title>]") }
    clickNamed(pid: pid, name: args[3], window: window)
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
