// display — put the guest's screen on the mode its virtual panel is built for, and say which one
// that is.
//
// Sent into the verification VM by `devtool vm up`, and run nowhere else. It is a second Swift tool
// rather than another verb on `scripts/screen.swift` for one reason: that one runs on a developer's
// own Mac too, and something that reconfigures a display does not belong next to click and type
// there. This one only ever reaches a guest.
//
// Why it exists: a macOS guest does not come up on the mode its virtual panel is built for.
// Measured on a clone freshly cut from the golden and set to 1920x1200pt, the desktop is 1024x768pt
// stretched across it — too narrow for the window under verification, and stretched under every
// shot. The mode is in the list the whole time; it has to be asked for.
//
// Usage:
//   display          the mode the screen is on, and the one it is built for
//   display native   put it on the one it is built for (idempotent)

import CoreGraphics
import Foundation

// IOGraphicsTypes.h, which has no Swift overlay: the panel's own mode, and the one macOS would pick
// for it. Both are needed — a panel carries two native modes, the pixel-for-pixel one and the 2x one
// over half the points, and only the second is marked default. It is the second that is wanted: an
// assert that reads words off a shot needs the scale, and a window laid out in points needs the room.
let nativeFlag: UInt32 = 0x0200_0000
let defaultFlag: UInt32 = 0x0000_0004

func fail(_ msg: String) -> Never {
    FileHandle.standardError.write("display: \(msg)\n".data(using: .utf8)!)
    exit(1)
}

/// scaleOf is the backing scale of a mode — 2 on a HiDPI one, 1 on a pixel-for-pixel one.
func scaleOf(_ m: CGDisplayMode) -> Int {
    m.width == 0 ? 1 : m.pixelWidth / m.width
}

func describe(_ m: CGDisplayMode) -> String {
    "\(m.width)x\(m.height)pt @\(scaleOf(m))x"
}

let display = CGMainDisplayID()

guard let current = CGDisplayCopyDisplayMode(display) else {
    fail("the main display reports no mode — is there a GUI session? (`stat -f %Su /dev/console`)")
}

// The HiDPI modes are hidden from the list unless they are asked for, and every mode worth having
// here is one of them.
let withDuplicates = [kCGDisplayShowDuplicateLowResolutionModes as String: kCFBooleanTrue!] as CFDictionary
guard let modes = CGDisplayCopyAllDisplayModes(display, withDuplicates) as? [CGDisplayMode], !modes.isEmpty else {
    fail("the main display lists no modes")
}

let native = modes.first { $0.ioFlags & nativeFlag != 0 && $0.ioFlags & defaultFlag != 0 }
    ?? modes.filter { $0.ioFlags & nativeFlag != 0 }.max { scaleOf($0) < scaleOf($1) }

guard let native else {
    fail("no mode in the list is marked as the panel's own — the display is not one this is for")
}

switch CommandLine.arguments.dropFirst().first {
case nil:
    print("\(describe(current)) (built for \(describe(native)))")
case "native":
    // Already there is not a no-op worth a display reconfiguration: it flickers the session and
    // anything watching the screen sees it move for nothing.
    if current.width == native.width, current.height == native.height, scaleOf(current) == scaleOf(native) {
        print(describe(native))
        exit(0)
    }
    var config: CGDisplayConfigRef?
    guard CGBeginDisplayConfiguration(&config) == .success else { fail("cannot begin a display configuration") }
    guard CGConfigureDisplayWithDisplayMode(config, display, native, nil) == .success else {
        fail("cannot configure the display to \(describe(native))")
    }
    // For the session only: the clone is thrown away, and `devtool vm up` asks for this every time,
    // so writing it into the guest's preferences would leave state behind that nothing reads back.
    let err = CGCompleteDisplayConfiguration(config, .forSession)
    guard err == .success else { fail("setting \(describe(native)) failed with \(err.rawValue)") }
    print(describe(native))
default:
    fail("usage: display [native]")
}
