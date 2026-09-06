// screen — drive a mac screen from the outside and read what is on it: bring an app to the front,
// click, drag, type, send a key, shoot its window, and read the text off a shot.
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
//                                                — with `--role <role>` when the name is on several kinds
//   swift screen.swift click <x> <y>             left-click at a screen point
//   swift screen.swift dblclick <x> <y>          double-click at a screen point (what opens a dialog's row)
//   swift screen.swift drag <pid> <x1> <y1> <x2> <y2> [steps]   press at the first point, move to the second, let go
//   swift screen.swift drop-file <pid> <x> <y> <path>…   drag those files in from outside the app and
//                                                let them go at that point (a real dragging session, which `drag` is not)
//   swift screen.swift type "text"               type into the focused element (Unicode direct, so no IME)
//   swift screen.swift key <keycode>             one virtual keycode (36=Return / 48=Tab / 53=Esc / 51=Backspace / 121=Page Down)
//                                                — held under `--cmd` / `--shift` / `--opt` / `--ctrl` when the press is a
//                                                  shortcut: ⌘C is `key 8 --cmd`, ⌘V is `key 9 --cmd`
//                                                  (the presses take the same four — see below)
//   swift screen.swift scroll <pid> <dx> <dy>    turn a wheel over that app's window, in points (+dy is back
//                                                toward the top) — over `--at <x> <y>` when what is to move is
//                                                not what the middle of the window is on
//   swift screen.swift right-click <x> <y>       right-click at a screen point
//   swift screen.swift right-click-named <pid> <name>  right-click what that name names
//   swift screen.swift dblclick-named <pid> <name>     double-click what that name names
//   swift screen.swift set-date <pid> <name> <yyyy-mm-dd> [--near <name>]
//                                                put a day into the date field of that name, on the
//                                                row that --near names when the name reaches several
//   swift screen.swift trusted                   whether the accessibility permission is granted (prompts if not)
//
// Anything aimed at a pid also takes `--window <title>`, anywhere in the line. An app draws one
// screen per window, so a pid alone stops naming a screen the moment it has two: `front` raises the
// window named, `shot` shoots it, and `find` / `click-named` / `drag` / `scroll` / `set-date` read,
// press, turn and write inside it and nowhere else. Left out, it means the app's one window — and an app with two is
// told to say which rather than answered with whichever the list held first. That silence is the failure worth refusing: a
// road reading the wrong window finds nothing it expected and comes out red for a reason nobody can
// see, or finds a name both windows carry and comes out green without having looked at the screen
// under test. A panel the app puts up — the one it opens a file with — is one of those windows while
// it is up, so it is named the same way and reached the same way, and until one is named the app is
// two windows rather than one.
//
// `find` / `click-named` / `right-click-named` / `dblclick-named` take `--role <role>` the same
// way, for a name that is on more than one kind of element. The role is the first column `find`
// prints, so it is read off the screen rather than remembered: `--role AXPopUpButton` reaches the
// pane's own field where a filter's `AXCheckBox` carries the same word, and it is the way past the
// refusal one name in two places otherwise ends in.
//
// `key` and the presses — `click` / `right-click` / `dblclick` and the three `-named` forms — take
// `--cmd` / `--shift` / `--opt` / `--ctrl` anywhere in the line, for a press held under a modifier.
// A list that adds to a selection with a held key has no other road to two rows at once, so
// `click-named <pid> <name> --cmd` is how the second row is reached. The modifier rides on the
// event's own flags and is never pressed as a key of its own, so nothing is left held afterwards.
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
// Being in the tree is not being on the window. A webview keeps a row it has scrolled out of sight
// named and framed, and the frame stands past the window's edge — so `find` writes `outside the
// window` at the end of that line, and the named presses refuse it instead of aiming at it. A press
// is a screen point like any other: one sent there lands on the desktop, or on whatever app is
// under it, and comes back 0 for having pressed nothing.
//
// The points a caller works out are refused on the same ground, against whatever the subcommand
// knows. `drag`, `drop-file` and `scroll --at` were handed a pid, so their points are held to that
// window; `click`, `dblclick` and `right-click` were not, so theirs are held to the displays — a
// point on none of them is a press nobody could have meant, and it is the shape a scale conversion
// gone the wrong way arrives in.
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
///
/// Those surfaces are therefore what is dropped, rather than everything but a standard window being
/// kept. A file panel arrives here as `AXDialog`, and it is a window in every sense that matters: it
/// is what is in front, it takes the presses, and the app behind it takes none. Kept out of this
/// list it was invisible to `find` and unreachable by `--window`, while `click-named` went on aiming
/// at the window behind it and reported the press as made — the silence this tool refuses everywhere
/// else. Counted, an app with a panel up simply has two windows, and is answered the way any app
/// with two is: with the titles, and a caller who says which (measured 2026-09-05, the Open panel
/// over the app's own window: `AXDialog` "Open" beside `AXStandardWindow` "Amenbo", with the drawing
/// surface `AXUnknown` and untitled alongside them).
func windowsOnScreen(of app: AXUIElement) -> [AXUIElement] {
    let all = axAttribute(app, kAXWindowsAttribute as String) as? [AXUIElement] ?? []
    return all.filter { axString($0, kAXSubroleAttribute as String) != (kAXUnknownSubrole as String) }
}

/// The window the caller named, as the two things anything here does to one: the id
/// `screencapture -l` takes, and the frame a pointer is aimed into. Both are spent by the callers in
/// this file and neither leaves it — a caller names a window by its title and nothing else. A window
/// behind another Space is not counted as on-screen, so an app that has not been fronted comes back
/// empty.
///
/// The titles come off the window list, which serves them only to a process granted Screen Recording
/// — the same permission shooting needs, so a run that can take the picture can also say which window
/// it took.
func windowOf(pid: Int, named wanted: String?) -> (id: Int, frame: CGRect) {
    guard let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] else {
        fail("could not read the window list")
    }
    var windows: [(id: Int, title: String, frame: CGRect)] = []
    for w in list {
        guard let owner = w[kCGWindowOwnerPID as String] as? Int, owner == pid,
              let id = w[kCGWindowNumber as String] as? Int,
              let b = w[kCGWindowBounds as String] as? [String: Any],
              let x = b["X"] as? Double,
              let y = b["Y"] as? Double,
              let width = b["Width"] as? Double,
              let height = b["Height"] as? Double,
              height > 200 // drop the incidental windows: shadows, tooltips and the like
        else { continue }
        // Bounds come back in the same top-left space a mouse event is posted into, so the frame is
        // aimable as it stands.
        windows.append((id, w[kCGWindowName as String] as? String ?? "",
                        CGRect(x: x, y: y, width: width, height: height)))
    }
    let found = theWindow(wanted, among: windows, titled: { $0.title }, of: pid)
    return (found.id, found.frame)
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
    let id = windowOf(pid: pid, named: window).id
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

/// Hand one image to Vision and give back a line per region it read.
///
/// Every setting the reader has lives here, so the whole shot and each of its quarters
/// ([`quarters`]) are read by one reader rather than two that could drift apart.
///
/// `cuts` are the sides this image was cut on rather than the sides the screen ends on. A row the
/// cut ran through is half a row, and half a row is not something anybody typed: it is dropped
/// rather than read, because the whole shot's own reading has that row entire.
func recognize(_ image: CGImage, ignoringWhatRuns cuts: Set<Edge> = []) -> [String] {
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
    return (request.results ?? [])
        .filter { observation in !cuts.contains(where: { $0.runs(through: observation.boundingBox) }) }
        .compactMap { $0.topCandidates(1).first?.string }
}

/// A side a quarter was cut on. Vision hands a region back in the quarter's own coordinates, where
/// the origin is bottom-left and the box is a share of the quarter — so a region touching the side
/// is one the cut ran through.
enum Edge {
    case left, right, bottom, top

    /// Whether this cut ran through `box`. The tolerance is a share of the quarter: a row that ends
    /// where the cut is has nothing between it and the side, and a row that merely ends near it has
    /// the space a character would take.
    func runs(through box: CGRect) -> Bool {
        let slack = 0.002
        switch self {
        case .left: return box.minX <= slack
        case .right: return box.maxX >= 1 - slack
        case .bottom: return box.minY <= slack
        case .top: return box.maxY >= 1 - slack
        }
    }
}

/// How far each quarter reaches past its own edge, as a share of its width and height.
///
/// A quarter cut exactly on the middle would cut some row of the screen in half, and half a row is
/// a row neither quarter reads whole. The margin is wide enough for the rows this app draws — a
/// pane's line, a card's title — to stand complete inside at least one quarter wherever the cut
/// happens to land.
let quarterOverlap = 0.15

/// The shot cut into four overlapping quarters ([`quarterOverlap`]), each with the sides it was cut
/// on — the sides the screen itself ends on are not cuts, and what stands against them is whole.
func quarters(of image: CGImage) -> [(CGImage, Set<Edge>)] {
    let width = Double(image.width), height = Double(image.height)
    let tileWidth = width / 2, tileHeight = height / 2
    var out: [(CGImage, Set<Edge>)] = []
    for column in 0..<2 {
        for row in 0..<2 {
            let x = max(0, Double(column) * tileWidth - tileWidth * quarterOverlap)
            let y = max(0, Double(row) * tileHeight - tileHeight * quarterOverlap)
            let rect = CGRect(
                x: x,
                y: y,
                width: min(tileWidth * (1 + 2 * quarterOverlap), width - x),
                height: min(tileHeight * (1 + 2 * quarterOverlap), height - y)
            )
            guard let tile = image.cropping(to: rect) else { continue }
            var cuts: Set<Edge> = []
            if rect.minX > 0 { cuts.insert(.left) }
            if rect.maxX < width { cuts.insert(.right) }
            // Vision counts from the bottom and `cropping` from the top, so the quarter's low edge
            // in one is its high edge in the other.
            if rect.minY > 0 { cuts.insert(.top) }
            if rect.maxY < height { cuts.insert(.bottom) }
            out.append((tile, cuts))
        }
    }
    return out
}

/// Read the text off a screenshot and print it as JSON: `text` is the reading folded, `raw` is what
/// Vision handed back — one line per region it recognized. Both, because the fold is what a caller
/// matches on and the raw is what a person reads when a match fails. A region Vision cannot read is
/// simply absent, which is the honest answer for an assert that expected words there.
///
/// **The shot is read whole, and then again in quarters, and the reading is what the five readings
/// hold between them.** Vision finds the regions on an image before it reads any of them, and on a
/// whole window's shot it misses small ones: a terminal pane half the window wide wrapped
/// `admin@…workshop % SCENARIO still taking what is typed` over three rows, and the reading came
/// back with the middle row's first half and the third row missing outright — not misread, absent,
/// while the same rows off a crop of that pane were read in full (measured 2026-09-06 on
/// `stack-the-two-panes-so-each-keeps-the-whole-width`). What the quarters change is how large those
/// regions stand in the frame handed to the detector, and that is enough for it to find them.
///
/// A line the whole reading already carries verbatim is not added a second time, so the raw a person
/// reads stays close to the reading it would have been. A line a quarter read differently — better,
/// or worse — is kept beside it: which of the two a caller's expectation meets is the caller's
/// question, and both were read off this shot.
func readText(path: String) {
    guard let data = FileManager.default.contents(atPath: path) else {
        fail("could not read \(path)")
    }
    guard let source = CGImageSourceCreateWithData(data as CFData, nil),
          let image = CGImageSourceCreateImageAtIndex(source, 0, nil)
    else {
        fail("could not decode an image from \(path)")
    }

    var lines = recognize(image)
    var seen = Set(lines)
    for (tile, cuts) in quarters(of: image) {
        for line in recognize(tile, ignoringWhatRuns: cuts) where !seen.contains(line) {
            seen.insert(line)
            lines.append(line)
        }
    }

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
    /// The tree handle the element was read from. Everything above answers a question about the
    /// element; this is what lets one be *written* — `set-date` sets a value through it rather than
    /// spelling the value out in keystrokes.
    let ref: AXUIElement
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
        found.append(Element(role: role.isEmpty ? "?" : role, name: name, frame: frame, ref: el))
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
        windows = windowsOnScreen(of: app)
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
///
/// The window's own frame comes back beside them, because standing under the window in the tree is
/// not the same as standing on it: a webview answers for a row it has scrolled out of sight with the
/// name it answers to and a frame past the window's edge, and every press here is a screen point. A
/// window that will not say where it stands answers `.infinite`, so a frame nobody could read refuses
/// nothing.
func windowAndElements(pid: Int, window wanted: String?) -> (frame: CGRect, elements: [Element]) {
    let w = appWindow(pid: pid, named: wanted)
    return (axFrame(w) ?? .infinite, elements(under: w))
}

/// Whether the window holds the point a press aimed at `e` would land on.
///
/// The middle of the element is that point — what `pointOf` arrives at for a name on one element, and
/// what `set-date` presses — so a listing is asked exactly what the press will ask of it.
func onTheWindow(_ e: Element, _ window: CGRect) -> Bool {
    window.contains(CGPoint(x: e.frame.midX, y: e.frame.midY))
}

/// Refuse a point the window does not hold.
///
/// A press is a screen point and nothing else, so one aimed past the window's edge lands on whatever
/// is there — the desktop, another app — and comes back 0 having pressed nothing anybody meant. That
/// is the worst shape a failure takes here: the road that sent it goes on to look for what the press
/// was supposed to do, finds none of it, and reports the app broken.
func mustBeOnTheWindow(_ p: CGPoint, _ window: CGRect, _ what: String) {
    guard !window.contains(p) else { return }
    let w = "\(Int(window.minX)),\(Int(window.minY)) \(Int(window.width))x\(Int(window.height))"
    fail("\(what) stands outside the window, at \(Int(p.x)),\(Int(p.y)) — the window is \(w); a press there would land on whatever is at that point. Bring it into the window first — scroll to it, or open the window wider — and aim again")
}

/// Every display, as the rectangles a mouse event is posted into.
///
/// `CGDisplayBounds` rather than `NSScreen.frame`: the two say the same thing in opposite spaces, and
/// this one is already the top-left, y-downward space every point in this file is written in. The
/// other would have to be flipped about the primary screen's height, which is the arithmetic
/// `drop-file` does once and the head of this file is about not doing twice.
func screens() -> [CGRect] {
    var count: UInt32 = 0
    guard CGGetActiveDisplayList(0, nil, &count) == .success, count > 0 else { return [] }
    var ids = [CGDirectDisplayID](repeating: 0, count: Int(count))
    guard CGGetActiveDisplayList(count, &ids, &count) == .success else { return [] }
    return ids.prefix(Int(count)).map(CGDisplayBounds)
}

/// Refuse a point no display holds.
///
/// What [`mustBeOnTheWindow`] is for a caller that named a window, this is for one that named
/// nothing: `click` / `dblclick` / `right-click` take no pid, so which window is under the point is
/// not a question they can ask — but whether any screen is there at all is, and a point on none of
/// them is a press nobody could have meant. It goes to the machine, lands nowhere, and comes back 0
/// with both streams empty, which is the same silent miss read off the far side as an app that did
/// not respond.
func mustBeOnAScreen(_ p: CGPoint, _ what: String) {
    let all = screens()
    guard !all.isEmpty else { fail("this machine has no screen to press on") }
    guard !all.contains(where: { $0.contains(p) }) else { return }
    let places = all.map { "\(Int($0.minX)),\(Int($0.minY)) \(Int($0.width))x\(Int($0.height))" }
        .joined(separator: " / ")
    fail("\(what) at \(Int(p.x)),\(Int(p.y)) is on no screen — the screens are \(places); nothing is there to press. A point is worked out from a shot, whose pixels are the window's points times the scale of the display it was on, so a point off the end of every screen is usually that arithmetic and not the aim")
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

/// The candidates standing on the same row as one of the anchors.
///
/// A row is what the screen has left where names run out. A manager listing the values of an axis
/// draws every row the same two date fields, so `Start date` names as many fields as there are
/// values and nothing in the name tells them apart — telling one of them to name itself more fully
/// asks for a name that does not exist. What does separate them is the value's own name, which
/// stands on that row and nowhere else, so the field is reached by what it stands beside.
///
/// Beside is read off the frames the tree answers with, now: a field and the name on its row share a
/// band of the screen vertically. Those are this moment's positions rather than a shot's pixels, so
/// neither the scale of the display nor a screen that has moved since enters the arithmetic — which
/// is what keeps this a name being followed rather than a point being aimed at.
func onTheRowOf(_ anchors: [Element], _ candidates: [Element]) -> [Element] {
    candidates.filter { candidate in
        anchors.contains { anchor in
            candidate.frame.minY < anchor.frame.maxY && anchor.frame.minY < candidate.frame.maxY
        }
    }
}

/// A card's name carries the line breaks of the lines it folded, so it is shown escaped rather than
/// broken across the message it is listed in.
func oneLine(_ s: String) -> String {
    s.replacingOccurrences(of: "\n", with: "\\n")
}

/// The ones drawn as `role`, when the caller has said which kind of element it means.
///
/// A name is not the whole of what a screen carries: the pane's assignee is a pop-up button whose
/// name is the person it holds, and the word standing beside it — the label a person would call it by
/// — is a piece of static text. One name is also often on two controls at once, a filter's checkbox
/// and the pane's own field both being called `Unassigned`. What separates them is the kind each is
/// drawn as, which `find` prints in its first column, so the caller reads one off the screen and hands
/// it back.
func ofRole(_ role: String?, _ all: [Element]) -> [Element] {
    guard let role else { return all }
    return all.filter { $0.role == role }
}

/// What a caller asked for, as a phrase to refuse it in.
func aimedAt(_ name: String?, _ role: String?) -> String {
    switch (name, role) {
    case let (name?, role?): return "called \(name) and drawn as \(role)"
    case let (name?, nil): return "called \(name)"
    case let (nil, role?): return "drawn as \(role)"
    case (nil, nil): return "on it"
    }
}

func find(pid: Int, name: String?, role: String?, window: String?) {
    let (frame, elements) = windowAndElements(pid: pid, window: window)
    let all = ofRole(role, elements)
    let found = name.map { named($0, among: all) } ?? all
    for e in found {
        // Said on the line rather than left off the listing: a row scrolled out of sight is still on
        // this screen, and a caller is often looking for exactly that — it is there, it just has to be
        // brought into the window before anything can be aimed at it. Left unsaid, the coordinates
        // read like any other and the press that follows them goes nowhere, quietly.
        let standing = onTheWindow(e, frame) ? "" : "\toutside the window"
        print("\(e.role)\t\(e.name)\t\(Int(e.frame.minX)) \(Int(e.frame.minY)) \(Int(e.frame.width)) \(Int(e.frame.height))\(standing)")
    }
    if found.isEmpty { fail("nothing on screen is \(aimedAt(name, role))") }
}

/// Click where that name is on screen.
///
/// The click is a real one, at where the element stands now: what the name saves is the arithmetic,
/// not the input path, and a press delivered through the accessibility API would go through whether
/// or not anything was covering the element.
///
/// Which is also why the app is brought to the front first, rather than left to the caller: a real
/// press lands on whatever is frontmost at that point, so anything that took the front — a sleeping
/// display, a permission dialog — swallows the click and the run still exits 0. A shot says so by
/// failing; a click cannot, so it is not asked to.
func clickNamed(pid: Int, name: String, role: String?, window: String?, flags: CGEventFlags = []) {
    let p = pointOf(pid: pid, name: name, role: role, window: window)
    click(x: p.x, y: p.y, flags: flags)
}

/// The same, with the other button: what a row's own menu is opened by, and the only way to reach one
/// from here. A menu drawn where the pointer is has no name to aim at until it is up.
func rightClickNamed(pid: Int, name: String, role: String?, window: String?, flags: CGEventFlags = []) {
    let p = pointOf(pid: pid, name: name, role: role, window: window)
    rightClick(x: p.x, y: p.y, flags: flags)
}

/// Put the pointer where that name is, and stop there.
///
/// **It is the pointer arriving and nothing else.** What it is for is the states a face draws for a
/// pointer resting on something and takes away when it leaves — a panel dropped under a row, a
/// control coming out of hiding — and every other verb here that arrives at a point goes on to press
/// it, which is the one thing that would take such a panel away again.
///
/// The app is brought to the front for the reason a press is: a pointer over a window that is not
/// frontmost is over that window all the same, but what is drawn under it is whatever is in front,
/// and a shot taken after this would be of the wrong screen.
func pointNamed(pid: Int, name: String, role: String?, window: String?) {
    let p = pointOf(pid: pid, name: name, role: role, window: window)
    hover(p)
}

/// The same press counted twice, for a row a single press only selects. The point is arrived at by
/// name for the reason the other two are, and the coordinate `dblclick` is no substitute here: it
/// takes no pid, so it fronts nothing, and a press aimed at this app's row lands on whatever window
/// took the front instead — silently, since a click cannot report what swallowed it.
func doubleClickNamed(pid: Int, name: String, role: String?, window: String?, flags: CGEventFlags = []) {
    let p = pointOf(pid: pid, name: name, role: role, window: window)
    doubleClick(x: p.x, y: p.y, flags: flags)
}

/// Where a name stands, with the app brought to the front — the arithmetic both named presses share,
/// and the refusals they share with it.
///
/// A name is often on more than one element without being ambiguous: a link and the text inside it
/// both answer to it, and they sit one on top of the other, so a press anywhere they overlap reaches
/// whatever is uppermost there — which is the thing a person pressing the word would have hit. So the
/// point aimed at is where every element of that name overlaps. Two of a name in two *places* has no
/// such point, and that is refused rather than guessed at: `find` is how to see both and aim at one.
///
/// Matching a part of a name (above) is what puts *several* names within reach of one word, and that
/// is a different ambiguity: ステータス is a label, a filter's pop-up and a group's button; 未着手 is
/// a column header and every card standing under it. Nothing here can tell which was meant, so the
/// names it reached are printed and nothing is pressed — pressing the first would be a press on
/// whatever the tree happened to hold first, reported as success.
///
/// One name on two elements in two places is the third ambiguity, and the caller settles it with
/// `--role`: the pane's assignee and a filter's checkbox are both called `Unassigned`, and what tells
/// them apart is that one is drawn as a pop-up button and the other as a checkbox.
///
func pointOf(pid: Int, name: String, role: String?, window: String?) -> CGPoint {
    front(pid: pid, window: window)
    let (frame, elements) = windowAndElements(pid: pid, window: window)
    let found = named(name, among: ofRole(role, elements))
    guard let first = found.first else { fail("nothing on screen is \(aimedAt(name, role))") }

    var reached: [String] = []
    for e in found where !reached.contains(e.name) { reached.append(e.name) }
    if reached.count > 1 {
        let names = reached.map(oneLine).joined(separator: " / ")
        fail("\(reached.count) names on screen hold \(name) — \(names); name one of them, or more of the one meant")
    }

    let overlap = found.dropFirst().reduce(first.frame) { $0.intersection($1.frame) }
    if overlap.isNull || overlap.isEmpty {
        let places = found.map { "\(Int($0.frame.minX)),\(Int($0.frame.minY))" }.joined(separator: " ")
        // A point is the last way out rather than the first: it costs the two conversions the head of
        // this file is about, so a caller is offered the kind it means before it is offered pixels.
        let onward = role == nil
            ? "say which kind with --role <the role find prints, e.g. \(found.map(\.role).joined(separator: " / "))>, or click a point instead"
            : "click a point instead"
        fail("\(found.count) elements are called \(oneLine(first.name)), in different places — at \(places); \(onward)")
    }
    let p = CGPoint(x: overlap.midX, y: overlap.midY)
    mustBeOnTheWindow(p, frame, oneLine(first.name))
    return p
}

/// Move onto the point before pressing, so an element that expects a hover first (a button's hover
/// state) is not missed.
func hover(_ p: CGPoint) {
    CGEvent(mouseEventSource: src, mouseType: .mouseMoved, mouseCursorPosition: p, mouseButton: .left)?
        .post(tap: .cghidEventTap)
    usleep(120_000)
}

/// One left-button event at a point, saying which press of a run it is. That number is the whole
/// difference between two clicks and a double click: the events are otherwise identical, and what is
/// listening reads the count off the field rather than timing the pair itself. A drag's events carry
/// it too — a webview handed a `pointerdown` whose count is zero has been handed a press nobody made.
///
/// **The modifiers ride on the event's own flags**, the way `key` sends its own, and are never
/// pressed as keys around the press. Every event written here says what it is held under, empty
/// included: one left with nothing written on it carries whatever the machine is holding at that
/// moment, and a stray ⌘ turns an ordinary press into a shortcut nobody asked for.
func mouse(
    _ phase: CGEventType, at p: CGPoint, clickState: Int64 = 1, button: CGMouseButton = .left,
    flags: CGEventFlags = []
) {
    let e = CGEvent(mouseEventSource: src, mouseType: phase, mouseCursorPosition: p, mouseButton: button)
    e?.setIntegerValueField(.mouseEventClickState, value: clickState)
    e?.flags = flags
    e?.post(tap: .cghidEventTap)
}

/// One down/up at a point, under whatever modifiers were asked for.
func press(at p: CGPoint, clickState: Int64, flags: CGEventFlags = []) {
    for phase in [CGEventType.leftMouseDown, CGEventType.leftMouseUp] {
        mouse(phase, at: p, clickState: clickState, flags: flags)
        usleep(60_000)
    }
}

/// A press at a point, under whatever modifiers were asked for. A row added to a selection rather
/// than taken as the whole of one is that press held under a key — the machine's own key for it, so
/// the caller says which — and a list that only adds this way has no other road to two rows at once.
func click(x: Double, y: Double, flags: CGEventFlags = []) {
    let p = CGPoint(x: x, y: y)
    mustBeOnAScreen(p, "the click")
    hover(p)
    press(at: p, clickState: 1, flags: flags)
}

/// The pointer moved to a point and left there, the coordinate half of `point-named`. It is for the
/// same states, and it is what a caller reaches for when what draws them is a region rather than
/// anything the tree gives a name to — the blank part of a row, say.
func point(x: Double, y: Double) {
    let p = CGPoint(x: x, y: y)
    mustBeOnAScreen(p, "the pointer")
    hover(p)
}

/// The press a row's own menu comes up on. The button is the whole difference — the same move, the
/// same hover before it — and it is sent as a press rather than through the accessibility API for the
/// reason every other press here is: what comes up has to come up the way it does for a person.
///
/// **A control-click is not this.** macOS raises the same menu for one, and a webview is handed a
/// `contextmenu` either way, but `click --ctrl` is a left press with a flag on it: whatever reads
/// which button was pressed reads the left one. The other button is sent as the other button, so
/// nothing downstream is asked to tell the two apart.
func rightClick(x: Double, y: Double, flags: CGEventFlags = []) {
    let p = CGPoint(x: x, y: y)
    mustBeOnAScreen(p, "the right-click")
    hover(p)
    for phase in [CGEventType.rightMouseDown, CGEventType.rightMouseUp] {
        mouse(phase, at: p, button: .right, flags: flags)
        usleep(60_000)
    }
}

/// A native open/save dialog opens the row you are pointing at on a double click and on nothing else
/// — a single click only selects it, and Return does not reach that dialog from here at all. So
/// picking a file out of one needs this.
func doubleClick(x: Double, y: Double, flags: CGEventFlags = []) {
    let p = CGPoint(x: x, y: y)
    mustBeOnAScreen(p, "the double click")
    hover(p)
    press(at: p, clickState: 1, flags: flags)
    press(at: p, clickState: 2, flags: flags)
}

/// Press at one point, cross to another with the button held, and let go there.
///
/// A point at each end rather than a name, unlike everything else that can be aimed by one: where a
/// drag lands is decided by which side of a row's middle it is let go on, and both sides of that line
/// are the same row. A name says which row and cannot say which side of it, so the two ends are the
/// caller's arithmetic — `find`'s rectangle is what the start is built from.
///
/// The crossing is sent as `steps` moves rather than as one, because moves are what the screen under
/// it is listening to: a webview being reordered works out where the pointer is on every move it is
/// given, and a jump straight to the far end gives it exactly one — after which the drop is right but
/// nothing that was meant to follow the pointer ever moved. 24 crossed a sidebar's list.
///
/// It is also the only way to photograph a drag in progress. Ask for a few hundred steps and the
/// crossing takes seconds, which is long enough for another process to run `screen shot` while the
/// button is still down — the drop line and the faded row it left are on screen for that long and
/// nowhere else.
func drag(pid: Int, window: String?, from a: CGPoint, to b: CGPoint, steps: Int) {
    front(pid: pid, window: window)
    // Both ends, and against the window rather than the screen: this one was handed a pid, so the
    // narrower question is the one it can ask. A crossing that starts off the window picks nothing
    // up, and one that ends off it drops what it carried where the app is not looking — and either
    // way the run comes back 0.
    let frame = windowOf(pid: pid, named: window).frame
    mustBeOnTheWindow(a, frame, "the drag's start")
    mustBeOnTheWindow(b, frame, "the drag's end")
    hover(a)
    mouse(.leftMouseDown, at: a)
    usleep(80_000) // long enough that what is under the pointer has taken the press before it moves
    for i in 1 ... steps {
        let t = Double(i) / Double(steps)
        mouse(.leftMouseDragged, at: CGPoint(x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t))
        usleep(20_000)
    }
    mouse(.leftMouseUp, at: b)
}

/// The near side of a dragging session: what it is willing to be, and the one message worth waiting
/// for.
final class DragSource: NSObject, NSDraggingSource {
    /// What the far side did with it, once it is over. Nothing until then, which is not the same as
    /// nothing having taken it.
    var landed: NSDragOperation?

    /// Copy, outside this app as much as inside it. What a road brings in is a file on the machine
    /// the screen is on, and a move would take it off that machine's disk — a drag that emptied the
    /// folder it came from would be a road with a change nobody wrote down.
    func draggingSession(
        _ session: NSDraggingSession, sourceOperationMaskFor context: NSDraggingContext
    ) -> NSDragOperation {
        .copy
    }

    /// The run loop is stopped rather than exited from, so what the caller does after the drop still
    /// happens. A stop takes effect on the next event, so one is posted to be it.
    func draggingSession(
        _ session: NSDraggingSession, endedAt point: NSPoint, operation: NSDragOperation
    ) {
        landed = operation
        DispatchQueue.main.async {
            NSApp.stop(nil)
            let wake = NSEvent.otherEvent(
                with: .applicationDefined, location: .zero, modifierFlags: [], timestamp: 0,
                windowNumber: 0, context: nil, subtype: 0, data1: 0, data2: 0
            )
            if let wake { NSApp.postEvent(wake, atStart: true) }
        }
    }
}

/// The crossing a begun session is steered by: `drag`'s run of moves, and then a few more in place
/// before the hand opens.
///
/// The ones in place are not a nicety. A window works out what is over it on the moves it is given,
/// and one whose first move arrived in the same instant as the release has been asked to decide
/// about a drop it never saw coming — measured, and it comes back as a drop nothing took.
///
/// **Every move goes out twice**, and the two deliveries do different halves of the job. Posted to
/// the machine, it moves the pointer, which is what decides the window under it; handed to this
/// process, it reaches the session, which follows the moves *it* is given and not the pointer. Only
/// the machine was posted to at first, and the session sat at the point it started from for the
/// whole crossing while the pointer walked away without it (measured on a Mac with a screen of its
/// own; in the verification VM the same run tracked).
///
/// It is posted from a thread of its own, and started *before* the session is: beginning one does
/// not return until the drag is over, so a crossing sent from the line after it is a crossing
/// nobody ever makes and a drag nobody ever ends.
func crossHolding(from a: CGPoint, to b: CGPoint) {
    let mine = pid_t(ProcessInfo.processInfo.processIdentifier)
    func move(_ phase: CGEventType, _ p: CGPoint) {
        let e = CGEvent(mouseEventSource: src, mouseType: phase, mouseCursorPosition: p, mouseButton: .left)
        e?.setIntegerValueField(.mouseEventClickState, value: 1)
        e?.postToPid(mine)
        e?.post(tap: .cghidEventTap)
    }
    let steps = 24
    usleep(150_000) // long enough that the session is up and looking for the pointer
    for i in 1 ... steps {
        let t = Double(i) / Double(steps)
        move(.leftMouseDragged, CGPoint(x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t))
        usleep(20_000)
    }
    for _ in 0 ..< 6 {
        move(.leftMouseDragged, b)
        usleep(80_000)
    }
    usleep(200_000)
    move(.leftMouseUp, b)
}

/// A file brought in from outside the app and let go over a point on it.
///
/// The one gesture here that posted events cannot make. What crosses a screen when a file is dragged
/// in is a **dragging session** — a pasteboard travelling with the pointer — and a pointer on its own
/// carries none: `drag` walks one across a window and the window is told nothing at all. Nor can the
/// events be aimed at the app the file is picked up *from*, since that hand is a file manager's and
/// nothing outside it reaches into its rows.
///
/// So the session is begun here. Only an app with a window on screen can begin one, which is what
/// this puts up: a small clear panel at the point the drag starts from, the files picked up in it,
/// and from there `drag`'s own crossing for `drag`'s own reason.
///
/// **The press that picks them up is written rather than waited for.** One posted at the panel does
/// not begin the session: the panel is on screen and the press was never delivered to it (measured).
/// Nothing is lost by making the event instead — what `beginDraggingSession` reads off one is where
/// the drag starts and which window it starts in. A real press *is* posted first all the same, so
/// the button the crossing then moves is a button the machine is holding down.
///
/// **The front is taken and then given back.** A drag out of an application the machine does not
/// have in front is carried nowhere: the session tracks, the pointer arrives, and every drop comes
/// back as one nothing took (measured). So this app comes forward for the length of the drag, and
/// the app under test is put back in front after it — which is what the steps that read the screen
/// afterwards are read against.
///
/// What the far side did with the drop is read off the session and refused when it is nothing. A
/// point that took no file leaves a screen looking exactly like the one before it, which is the
/// silent miss every refusal in this file is written against.
func dropFile(pid: Int, window: String?, to: CGPoint, paths: [String]) {
    let urls = paths.map { path -> URL in
        let url = URL(fileURLWithPath: path).standardizedFileURL
        guard FileManager.default.fileExists(atPath: url.path) else {
            fail("there is nothing at \(url.path) to drag in — a drop carries a file on this machine's own disk")
        }
        return url
    }
    front(pid: pid, window: window)
    // The far end only. The near one is the panel written below, which is deliberately off the app's
    // window — it is what the session is begun from, not somewhere anything is let go.
    mustBeOnTheWindow(to, windowOf(pid: pid, named: window).frame, "the point the files are let go at")

    let app = NSApplication.shared
    app.setActivationPolicy(.regular)

    // A screen point here is the accessibility tree's — the primary panel's top left, y downward —
    // and a window is placed in the other one. The conversion is made once, here.
    guard let primary = NSScreen.screens.first else { fail("this machine has no screen to drag across") }
    let side = 40.0
    // Clear of the menu bar, and far enough off that the crossing is a crossing. What is under it
    // does not matter — the press that begins the session is written rather than aimed — so the
    // panel is only put above the rest so that the drag starts from a thing that is on screen.
    let from = CGPoint(x: 80, y: 120)
    let well = NSView(frame: NSRect(x: 0, y: 0, width: side, height: side))
    let panel = NSPanel(
        contentRect: NSRect(
            x: from.x - side / 2, y: primary.frame.maxY - from.y - side / 2, width: side, height: side
        ),
        // Non-activating: the app is brought forward as a whole below, and a panel that took the
        // keyboard on its way up would leave it somewhere no road asked for.
        styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false
    )
    panel.level = NSWindow.Level(rawValue: Int(CGWindowLevelForKey(.popUpMenuWindow)))
    panel.isOpaque = false
    panel.backgroundColor = .clear
    panel.contentView = well
    panel.orderFrontRegardless()
    app.activate(ignoringOtherApps: true)

    let source = DragSource()
    // The button goes down for real, and the drag is begun off an event written to match it, once
    // the run loop below has had a moment to put the panel on screen.
    DispatchQueue.global().async {
        usleep(300_000)
        hover(from)
        mouse(.leftMouseDown, at: from)
        usleep(80_000)
        DispatchQueue.main.async {
            guard let press = NSEvent.mouseEvent(
                with: .leftMouseDown, location: NSPoint(x: side / 2, y: side / 2), modifierFlags: [],
                timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: panel.windowNumber,
                context: nil, eventNumber: 0, clickCount: 1, pressure: 1
            ) else { fail("this machine would not make a press to begin a drag with") }
            let items = urls.map { url -> NSDraggingItem in
                let item = NSDraggingItem(pasteboardWriter: url as NSURL)
                item.setDraggingFrame(
                    NSRect(x: 0, y: 0, width: side, height: side),
                    contents: NSWorkspace.shared.icon(forFile: url.path)
                )
                return item
            }
            DispatchQueue.global().async { crossHolding(from: from, to: to) }
            let session = well.beginDraggingSession(with: items, event: press, source: source)
            session.animatesToStartingPositionsOnCancelOrFail = false
        }
    }
    // A session nobody ends would hold the loop for good, with the button still down. This tool is
    // run inside a road, and a road that stops answering is worse than one that comes out red.
    DispatchQueue.global().asyncAfter(deadline: .now() + 30) {
        mouse(.leftMouseUp, at: to)
        fail("the drag never ended — the press went in and nothing let it go")
    }
    app.run()

    panel.orderOut(nil)
    front(pid: pid, window: window)
    guard let landed = source.landed, !landed.isEmpty else {
        let what = urls.count == 1 ? "the file" : "the files"
        fail("nothing took \(what) at \(Int(to.x)),\(Int(to.y)) — the point is on something that reads no drop")
    }
}

/// type sends the string itself rather than keycodes. It bypasses the IME, so any script goes in as-is.
///
/// **Each event is sent under no modifiers**, the way `key` sends its own. An event left with none
/// written on it carries whatever the machine is holding down at that moment, and one modifier still
/// held turns every letter into a shortcut: nothing lands, and the run returns 0 with the field
/// unchanged — the loudest way this tool can be silent.
func type(_ s: String) {
    for ch in s {
        let utf16 = Array(String(ch).utf16)
        for down in [true, false] {
            let e = CGEvent(keyboardEventSource: src, virtualKey: 0, keyDown: down)
            e?.flags = []
            e?.keyboardSetUnicodeString(stringLength: utf16.count, unicodeString: utf16)
            e?.post(tap: .cghidEventTap)
        }
        usleep(12_000) // enough of a gap that the webview's input handler misses nothing
    }
}

/// One keycode, pressed and released, under whatever modifiers were asked for. A shortcut is one
/// press to a caller — ⌘C, ⌘V — so it is said as one here rather than as a key and the keys around it.
///
/// **The modifiers ride on the event's own flags**, and are never pressed as keys of their own. A run
/// that failed between pressing ⌘ and letting it go would leave the machine holding a key nobody
/// pressed, and everything sent after it would arrive as a shortcut. A press is sent the same way,
/// so a ⌘-click leaves nothing held either.
///
/// Not every key arrives. Return, Tab, Backspace and Page Down reach the webview; Page Up, Home, End
/// and the arrows were posted the same way and nothing moved. So a key is not the way to walk a page:
/// `scroll` is, and it goes where these do not.
func key(_ code: CGKeyCode, flags: CGEventFlags = []) {
    for down in [true, false] {
        let e = CGEvent(keyboardEventSource: src, virtualKey: code, keyDown: down)
        e?.flags = flags
        e?.post(tap: .cghidEventTap)
        if down { usleep(40_000) }
    }
}

/// Pull `--<flag> <value>` out of the line, wherever in it the caller wrote it, and hand back what is
/// left as the positional arguments each subcommand already reads. Which window, and which kind of
/// element, are qualifiers on the aim rather than more things to name, so neither is given a place in
/// any subcommand's order — a `find <pid> [name]` whose optional name and optional window sat side by
/// side would read a one-word call either way round.
func takeOption(_ flag: String, _ argv: [String], needs: String) -> (value: String?, rest: [String]) {
    var value: String?
    var rest: [String] = []
    var i = 0
    while i < argv.count {
        if argv[i] == flag {
            guard i + 1 < argv.count else { fail("\(flag) needs \(needs)") }
            value = argv[i + 1]
            i += 2
            continue
        }
        rest.append(argv[i])
        i += 1
    }
    return (value, rest)
}

/// The modifiers a key or a press is held under, pulled out of the line wherever the caller wrote
/// them — the freedom `--window` and `--role` have, for the reason they have it. They carry no value
/// of their own, so each is simply there or not, and several may be.
func takeModifiers(_ argv: [String]) -> (flags: CGEventFlags, rest: [String]) {
    let known: [String: CGEventFlags] = [
        "--cmd": .maskCommand, "--shift": .maskShift, "--opt": .maskAlternate, "--ctrl": .maskControl,
    ]
    var flags: CGEventFlags = []
    var rest: [String] = []
    for arg in argv {
        if let held = known[arg] { flags.insert(held) } else { rest.append(arg) }
    }
    return (flags, rest)
}

/// Pull `--at <x> <y>` out of the line, wherever the caller wrote it — the freedom `--window` and
/// `--role` have, for the reason they have it. Where the finger stands is a qualifier on the aim
/// rather than two more things to name, so it takes no place in `scroll <pid> <dx> <dy>`, whose
/// trailing pair would otherwise read as an amount written across.
///
/// Two numbers rather than one word, so [`takeOption`] does not serve: a point is a pair, and the
/// pair is checked here so that a caller who wrote one number is told, rather than having the second
/// half of the point read as the subcommand's own argument.
func takeAt(_ argv: [String]) -> (point: CGPoint?, rest: [String]) {
    var point: CGPoint?
    var rest: [String] = []
    var i = 0
    while i < argv.count {
        if argv[i] == "--at" {
            guard i + 2 < argv.count, let x = Double(argv[i + 1]), let y = Double(argv[i + 2]) else {
                fail("--at needs a screen point, written as two numbers: --at <x> <y>")
            }
            point = CGPoint(x: x, y: y)
            i += 3
            continue
        }
        rest.append(argv[i])
        i += 1
    }
    return (point, rest)
}

/// Turn a wheel over the app's window.
///
/// This is the way back up a page. Page Down is the one scrolling key that reaches the webview, so a
/// road that walked down a pane could not return to what it had passed — and reopening the pane does
/// not reset it either, the position being kept. A wheel is not a key, and arrives where they do not.
///
/// A wheel event lands wherever the pointer is rather than on whatever holds focus, so the app is
/// fronted and the pointer put somewhere over its window first — the same reason `click-named` fronts
/// before it presses. **Where the pointer stands is the whole of which pane moves**, and nothing else
/// redirects it: clicking into a pane first changes nothing, the click moving focus where the wheel
/// does not look. So a window split into panes takes `--at <x> <y>`, the middle being a divider or
/// another pane on such a screen; left out, the middle of the window is where the finger goes, which
/// is the scrollable body of a window drawing one pane. A finger put off the window is refused for
/// the reason a press there is: the turn would move whatever else is under it, and say nothing.
///
/// Positive is the way back: `scroll <pid> 0 800` goes 800 points up the page, and toward its left
/// across. The amount is in points, and it is delivered as a run of short turns rather than one
/// jump — a webview animates each turn, and a single large delta arrives mid-animation and is
/// swallowed. Rounding is carried along the run, so what is asked for is what is delivered.
func scroll(pid: Int, window: String?, dx: Double, dy: Double, at: CGPoint?) {
    // Refused rather than trusted: an amount no screen is that tall would run the turns below for as
    // long as it took to overflow, which is a hang where a sentence belongs.
    guard dx.isFinite, dy.isFinite, abs(dx) <= 100_000, abs(dy) <= 100_000 else {
        fail("scroll takes an amount in points, and \(dx),\(dy) is longer than any screen")
    }
    front(pid: pid, window: window)
    let frame = windowOf(pid: pid, named: window).frame
    if let at { mustBeOnTheWindow(at, frame, "the finger") }
    hover(at ?? CGPoint(x: frame.midX, y: frame.midY))

    let perTurn = 60.0 // about what one notch of a wheel moves
    let turns = Int((max(abs(dx), abs(dy)) / perTurn).rounded(.up))
    guard turns > 0 else { return }
    // How much of the total has been delivered by the end of turn `upto` — so the rounding of each
    // turn is carried into the next rather than dropped, and the run adds up to what was asked for.
    let sent = { (total: Double, upto: Int) in (total * Double(upto) / Double(turns)).rounded() }
    for i in 1...turns {
        let x = Int32(sent(dx, i) - sent(dx, i - 1))
        let y = Int32(sent(dy, i) - sent(dy, i - 1))
        CGEvent(scrollWheelEvent2Source: src, units: .pixel, wheelCount: 2, wheel1: y, wheel2: x, wheel3: 0)?
            .post(tap: .cghidEventTap)
        usleep(30_000)
    }
}

/// The date element in this app whose day can be written — the picker panel's, once it is open.
///
/// Walked off the raw tree rather than through [`windowAndElements`], which keeps only the elements
/// that answer to a name: the panel carries none, being drawn as the field's own pop-up rather than a
/// control of its own, so a listing of the screen never holds it. It is the one element here that is
/// looked up by what it can do instead of by what it is called.
func writableDay(pid: Int) -> AXUIElement? {
    func walk(_ el: AXUIElement, _ depth: Int) -> AXUIElement? {
        guard depth < 60 else { return nil }
        if axString(el, kAXRoleAttribute as String) == "AXDateTimeArea" {
            var settable: DarwinBoolean = false
            AXUIElementIsAttributeSettable(el, kAXValueAttribute as CFString, &settable)
            if settable.boolValue { return el }
        }
        for child in axAttribute(el, kAXChildrenAttribute as String) as? [AXUIElement] ?? [] {
            if let hit = walk(child, depth + 1) { return hit }
        }
        return nil
    }
    let app = AXUIElementCreateApplication(pid_t(pid))
    openTree(app)
    for window in axAttribute(app, kAXWindowsAttribute as String) as? [AXUIElement] ?? [] {
        if let hit = walk(window, 0) { return hit }
    }
    return nil
}

/// Put a day into a date field, by writing it rather than by spelling it out.
///
/// A `<input type="date">` in a webview is one control with three numeric fields inside it, and a
/// keystroke is the only way in from the outside: a digit at a time, each one moving the field on
/// when it fills. That does not survive the screen it is aimed at. Every digit that leaves the value
/// a valid date makes the app commit and redraw the field it was typed into, and the redraw resets
/// the run of digits WebKit was collecting — so a year, which is four digits and valid after each
/// one, comes back as the last digit alone (`2099` lands as `0009`). Slowing the typing does not
/// help; it is the redraw between the digits, not the pace of them.
///
/// So the day is set where the control keeps it. Opening the picker puts a second date element on
/// the tree — the panel's own, the one whose value is writable — and the value written there reaches
/// the field, the change event, and the store, as one move rather than as eight. What the caller
/// gets is the same shape as a click: name the field, name the day, and the picker is opened and shut
/// again around the write.
///
/// The read-back is the point of the op rather than a nicety: a write that reached nothing would
/// otherwise leave a screen showing the day it had before, and a step that asserts on that day would
/// report the field as the thing that is broken.
///
/// `near` names something standing on the field's own row, for the screens where the field's name
/// does not reach one field. It is asked of every call that passes it, not only of the ambiguous
/// ones: a caller saying which row it means has said where the field is, and a single field standing
/// somewhere else is a screen that is not where the caller thinks it is — which is worth failing on
/// rather than writing into.
func setDate(pid: Int, name: String, day: String, window: String?, near: String?) {
    let stamp = DateFormatter()
    stamp.locale = Locale(identifier: "en_US_POSIX")
    stamp.timeZone = TimeZone(identifier: "UTC")
    stamp.dateFormat = "yyyy-MM-dd"
    guard let wanted = stamp.date(from: day) else { fail("\(day) is not a day — write it as yyyy-mm-dd") }

    front(pid: pid, window: window)
    let (frame, all) = windowAndElements(pid: pid, window: window)
    var fields = named(name, among: all.filter { $0.role == "AXDateTimeArea" })
    if fields.isEmpty { fail("no date field on screen is called \(name)") }
    if let near {
        let anchors = named(near, among: all)
        if anchors.isEmpty { fail("nothing on screen is called \(near), for a \(name) to stand beside") }
        fields = onTheRowOf(anchors, fields)
        if fields.isEmpty { fail("no date field called \(name) stands on a row with \(near)") }
    }
    if fields.count > 1 {
        let names = fields.map { oneLine($0.name) }.joined(separator: " / ")
        let onward = near.map { "name something that stands on the row of the one meant, rather than \($0)" }
            ?? "name one of them, or say --near <a name on its row>"
        fail("\(fields.count) date fields hold \(name) — \(names); \(onward)")
    }
    let field = fields[0]

    // Open the picker, and wait for the element it brings: the panel is drawn by the app rather than
    // by the web process, so it arrives a moment after the press. A picker somebody left open is not
    // opened again — the press that opens one closes one, so pressing regardless would shut the very
    // panel being waited for.
    if writableDay(pid: pid) == nil {
        mustBeOnTheWindow(CGPoint(x: field.frame.midX, y: field.frame.midY), frame, oneLine(field.name))
        click(x: field.frame.midX, y: field.frame.midY)
    }
    let deadline = Date().addingTimeInterval(3)
    var written = false
    repeat {
        if let slot = writableDay(pid: pid) {
            written = AXUIElementSetAttributeValue(slot, kAXValueAttribute as CFString, wanted as CFDate) == .success
        }
        if written { break }
        usleep(100_000)
    } while Date() < deadline
    if !written { fail("the picker for \(oneLine(field.name)) never offered a day to write") }

    // Shut it again — the panel stands over the rows under the field, so a shot taken with it up is a
    // shot of the picker rather than of the screen. It closes when the focus leaves the field, and a
    // tab walks the field's own parts before it goes, so the key is pressed until the panel is gone
    // rather than a fixed number of times. Neither Escape nor a second press on the field closes it
    // (both were tried); tabbing out does.
    for _ in 0..<8 where writableDay(pid: pid) != nil {
        key(48) // tab
        usleep(200_000)
    }
    if writableDay(pid: pid) != nil { fail("the picker for \(oneLine(field.name)) would not close") }

    guard let landed = axAttribute(field.ref, kAXValueAttribute as String) as? Date else {
        fail("\(oneLine(field.name)) does not say what day it holds")
    }
    let got = stamp.string(from: landed)
    if got != day { fail("\(oneLine(field.name)) holds \(got), not \(day)") }
}

let (window, afterWindow) = takeOption("--window", CommandLine.arguments, needs: "the title of a window")
let (role, afterRole) = takeOption("--role", afterWindow, needs: "the role find prints in its first column")
let (at, afterAt) = takeAt(afterRole)
let (held, args) = takeModifiers(afterAt)
guard args.count >= 2 else {
    fail("usage: screen <front|shot|read|find|click-named|right-click-named|dblclick-named|point-named|click|right-click|dblclick|point|drag|drop-file|type|key|scroll|set-date|trusted> … [--window <title>]")
}
// Refused rather than ignored: a qualifier the subcommand never reads would narrow nothing and say so
// nowhere, which is the silent miss every refusal in this file is written against.
if role != nil, !["find", "click-named", "right-click-named", "dblclick-named", "point-named"].contains(args[1]) {
    fail("--role says which kind of element to reach, and only find / click-named / right-click-named / dblclick-named / point-named take one")
}
if !held.isEmpty,
    !["key", "click", "right-click", "dblclick", "click-named", "right-click-named", "dblclick-named"]
        .contains(args[1])
{
    fail("--cmd / --shift / --opt / --ctrl say what a key or a press is held under, and only key / click / right-click / dblclick / click-named / right-click-named / dblclick-named take them")
}
if at != nil, args[1] != "scroll" {
    fail("--at says where the finger stands for a wheel, and only scroll takes one")
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
    guard args.count == 3 || args.count == 4, let pid = Int(args[2]) else { fail("usage: screen find <pid> [name] [--role <role>] [--window <title>]") }
    find(pid: pid, name: args.count == 4 ? args[3] : nil, role: role, window: window)
case "click-named":
    guard args.count == 4, let pid = Int(args[2]) else { fail("usage: screen click-named <pid> <name> [--role <role>] [--cmd] [--shift] [--opt] [--ctrl] [--window <title>]") }
    clickNamed(pid: pid, name: args[3], role: role, window: window, flags: held)
case "right-click-named":
    guard args.count == 4, let pid = Int(args[2]) else { fail("usage: screen right-click-named <pid> <name> [--role <role>] [--cmd] [--shift] [--opt] [--ctrl] [--window <title>]") }
    rightClickNamed(pid: pid, name: args[3], role: role, window: window, flags: held)
case "dblclick-named":
    guard args.count == 4, let pid = Int(args[2]) else { fail("usage: screen dblclick-named <pid> <name> [--role <role>] [--cmd] [--shift] [--opt] [--ctrl] [--window <title>]") }
    doubleClickNamed(pid: pid, name: args[3], role: role, window: window, flags: held)
case "point-named":
    guard args.count == 4, let pid = Int(args[2]) else { fail("usage: screen point-named <pid> <name> [--role <role>] [--window <title>]") }
    pointNamed(pid: pid, name: args[3], role: role, window: window)
case "point":
    guard args.count == 4, let x = Double(args[2]), let y = Double(args[3]) else { fail("usage: screen point <x> <y>") }
    point(x: x, y: y)
case "click":
    guard args.count == 4, let x = Double(args[2]), let y = Double(args[3]) else { fail("usage: screen click <x> <y> [--cmd] [--shift] [--opt] [--ctrl]") }
    click(x: x, y: y, flags: held)
case "right-click":
    guard args.count == 4, let x = Double(args[2]), let y = Double(args[3]) else { fail("usage: screen right-click <x> <y> [--cmd] [--shift] [--opt] [--ctrl]") }
    rightClick(x: x, y: y, flags: held)
case "dblclick":
    guard args.count == 4, let x = Double(args[2]), let y = Double(args[3]) else { fail("usage: screen dblclick <x> <y> [--cmd] [--shift] [--opt] [--ctrl]") }
    doubleClick(x: x, y: y, flags: held)
case "drag":
    let dragUsage = "usage: screen drag <pid> <x1> <y1> <x2> <y2> [steps] [--window <title>]"
    guard args.count == 7 || args.count == 8, let pid = Int(args[2]),
        let x1 = Double(args[3]), let y1 = Double(args[4]),
        let x2 = Double(args[5]), let y2 = Double(args[6])
    else { fail(dragUsage) }
    let steps = args.count == 8 ? Int(args[7]) : 24
    guard let steps, steps > 0 else { fail(dragUsage) }
    drag(pid: pid, window: window, from: CGPoint(x: x1, y: y1), to: CGPoint(x: x2, y: y2), steps: steps)
case "drop-file":
    let dropUsage = "usage: screen drop-file <pid> <x> <y> <path>… [--window <title>]"
    guard args.count >= 6, let pid = Int(args[2]), let x = Double(args[3]), let y = Double(args[4]) else {
        fail(dropUsage)
    }
    dropFile(pid: pid, window: window, to: CGPoint(x: x, y: y), paths: Array(args[5...]))
case "type":
    guard args.count == 3 else { fail("usage: screen type <text>") }
    type(args[2])
case "key":
    guard args.count == 3, let code = UInt16(args[2]) else {
        fail("usage: screen key <keycode> [--cmd] [--shift] [--opt] [--ctrl]")
    }
    key(CGKeyCode(code), flags: held)
case "scroll":
    guard args.count == 5, let pid = Int(args[2]), let dx = Double(args[3]), let dy = Double(args[4]) else {
        fail("usage: screen scroll <pid> <dx> <dy> [--at <x> <y>] [--window <title>]")
    }
    scroll(pid: pid, window: window, dx: dx, dy: dy, at: at)
case "set-date":
    guard args.count == 5 || (args.count == 7 && args[5] == "--near"), let pid = Int(args[2]) else {
        fail("usage: screen set-date <pid> <name> <yyyy-mm-dd> [--near <name>] [--window <title>]")
    }
    setDate(pid: pid, name: args[3], day: args[4], window: window, near: args.count == 7 ? args[6] : nil)
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
