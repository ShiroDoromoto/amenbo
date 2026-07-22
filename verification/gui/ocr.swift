// Read the text off a screenshot with macOS's own Vision framework — the mac OCR primitive for
// the GUI verification harness (decision `AMB-D-355`). Vision ships with the OS (no brew install)
// and reads Retina webview screenshots more accurately than tesseract, which stays the Linux
// container path (scripts/docker/gui-e2e.sh). Like uiauto.swift, a mac-specific primitive kept in
// Swift and run directly (`swift ocr.swift <image>`), never a compiled dependency.
//
// It prints one recognized line per output line (Vision's top candidate for each text region), so
// the caller judges an expected string present/absent by a plain substring match. A region Vision
// cannot read is simply absent from the output — the caller reads that as "text not on screen",
// which is the honest verdict for an assert that expected it.
//
// Usage:
//   swift ocr.swift <image.png>     print the recognized text, one region per line

import Foundation
import Vision

func fail(_ msg: String) -> Never {
    FileHandle.standardError.write("ocr: \(msg)\n".data(using: .utf8)!)
    exit(1)
}

let args = CommandLine.arguments
guard args.count == 2 else { fail("usage: ocr <image.png>") }
let path = args[1]

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
// The app renders both scripts; a scenario seeds English titles but the surrounding UI is Japanese.
request.recognitionLanguages = ["en-US", "ja-JP"]

let handler = VNImageRequestHandler(cgImage: image, options: [:])
do {
    try handler.perform([request])
} catch {
    fail("Vision could not run text recognition: \(error)")
}

guard let observations = request.results else { exit(0) }
for observation in observations {
    if let candidate = observation.topCandidates(1).first {
        print(candidate.string)
    }
}
