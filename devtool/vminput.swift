// input — the keyboard input sources a guest has, and enabling the Japanese one.
//
// Sent into the golden by `devtool vm golden --prepare`, and run nowhere else. It is a second Swift
// tool rather than another verb on `scripts/screen.swift` for the reason the display tool is one:
// that tool runs on a developer's own Mac too, and something that rewrites which input methods a
// machine has does not belong next to click and type there. `screen` selects among what is enabled;
// what is enabled is the machine's own preparation, and this is where that is done.
//
// Why it exists: the third-party image the golden is cut from ships U.S. and the character palette
// and nothing else, so a screen in there cannot be typed at through an input method at all — and a
// word held by one, on the screen and in no field, is what `keep-the-word-the-keyboard-was-taken-
// away-from` is about. The Japanese input method is installed in the image the whole time; it has to
// be enabled.
//
// Enabling is written into the account's preferences, so it is in the image clones are cut from and
// every clone comes up with it. Nothing has to be restarted for it: the running session takes the
// change as it is made, which is what keeps this off the road that killed `loginwindow` and left the
// guest taking clicks and no keys at all.
//
// Usage:
//   input            the keyboard input sources this machine has enabled
//   input japanese   enable the Japanese input method and its Japanese mode (idempotent), leaving
//                    U.S. the one that is selected

import Carbon
import Foundation

// The Japanese input method that is typed at in Latin letters, and the mode of it that converts. Both
// are named: the mode is what a person picks from the input menu, and it cannot be reached while the
// method it belongs to is disabled.
let japaneseMethod = "com.apple.inputmethod.Kotoeri.RomajiTyping"
let japaneseMode = "com.apple.inputmethod.Kotoeri.RomajiTyping.Japanese"
// What a guest is left selected on, so that enabling a second input method changes nothing else
// about the ground every other road is walked on.
let plainKeyboard = "com.apple.keylayout.US"

func fail(_ msg: String) -> Never {
    FileHandle.standardError.write("input: \(msg)\n".data(using: .utf8)!)
    exit(1)
}

/// An input source's own identifier, which is how every caller names one.
func identifier(_ source: TISInputSource) -> String {
    guard let value = TISGetInputSourceProperty(source, kTISPropertyInputSourceID) else { return "?" }
    return Unmanaged<CFString>.fromOpaque(value).takeUnretainedValue() as String
}

/// Every keyboard input source, enabled or not — `installed` is what turns the second half on. The
/// enabled ones alone are what a person sees in the input menu; the rest are what can be added to it.
func sources(installed: Bool) -> [TISInputSource] {
    guard let list = TISCreateInputSourceList(nil, installed)?.takeRetainedValue() as? [TISInputSource] else {
        return []
    }
    return list
}

func source(named wanted: String) -> TISInputSource {
    guard let found = sources(installed: true).first(where: { identifier($0) == wanted }) else {
        fail("this machine has no input source \(wanted) installed")
    }
    return found
}

func enable(_ wanted: String) {
    let status = TISEnableInputSource(source(named: wanted))
    if status != noErr { fail("enabling \(wanted) came back \(status)") }
}

// The domain the enabled input sources are kept in, and the one thing here that has to reach the
// disk: this runs to leave something behind in an image, and what a preference daemon holds in memory
// is not in an image. `defaults` synchronizes as it exits and so needs nothing of the sort;
// `TISEnableInputSource` does not, and a golden stopped before the daemon got round to writing comes
// back with the input method gone — measured, on the first golden prepared this way.
let inputSourceDomain = "com.apple.HIToolbox" as CFString

func flushToDisk() {
    if !CFPreferencesAppSynchronize(inputSourceDomain) {
        fail("\(inputSourceDomain) would not write itself out — what was enabled is in memory only")
    }
}

switch CommandLine.arguments.dropFirst().first {
case nil:
    for s in sources(installed: false) { print(identifier(s)) }
case "japanese":
    // The method before the mode: a mode of a disabled method is not there to be enabled yet.
    enable(japaneseMethod)
    enable(japaneseMode)
    let status = TISSelectInputSource(source(named: plainKeyboard))
    if status != noErr { fail("selecting \(plainKeyboard) came back \(status)") }
    flushToDisk()
    print(japaneseMode)
default:
    fail("usage: input [japanese]")
}
