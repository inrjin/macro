// Independent count-reader fixtures. This uses system CoreText rendering, not
// production reader templates, live windows, OCR, keyboard input or screenshots.
import Foundation
import CoreGraphics
import CoreText
import ImageIO

struct Ink {
    let r: CGFloat, g: CGFloat, b: CGFloat
    var cg: CGColor { CGColor(red: r / 255, green: g / 255, blue: b / 255, alpha: 1) }
    static let dark = Ink(r: 34, g: 34, b: 34)
    static let blue = Ink(r: 78, g: 82, b: 255)
    static let gray = Ink(r: 125, g: 125, b: 125)
}

final class Canvas {
    let width: Int, height: Int
    let pixels: UnsafeMutablePointer<UInt8>
    let context: CGContext
    var text: [[String: Any]] = []

    init(width: Int = 640, height: Int = 300, background: Ink = Ink(r: 255, g: 255, b: 255)) {
        self.width = width; self.height = height
        pixels = .allocate(capacity: width * height * 4)
        pixels.initialize(repeating: 255, count: width * height * 4)
        context = CGContext(data: pixels, width: width, height: height,
            bitsPerComponent: 8, bytesPerRow: width * 4, space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGBitmapInfo.byteOrder32Big.rawValue | CGImageAlphaInfo.premultipliedLast.rawValue)!
        context.setFillColor(background.cg)
        context.fill(CGRect(x: 0, y: 0, width: width, height: height))
        context.setAllowsAntialiasing(true)
        context.setShouldAntialias(true)
    }

    deinit { pixels.deallocate() }

    @discardableResult
    func draw(_ string: String, x: CGFloat, y: CGFloat, size: CGFloat = 24,
              font name: String = "AppleSDGothicNeo-Bold", ink: Ink = .dark,
              clip: CGRect? = nil) -> CGRect {
        let font = CTFontCreateWithName(name as CFString, size, nil)
        let attributes: [NSAttributedString.Key: Any] = [
            NSAttributedString.Key(kCTFontAttributeName as String): font,
            NSAttributedString.Key(kCTForegroundColorAttributeName as String): ink.cg,
        ]
        let line = CTLineCreateWithAttributedString(NSAttributedString(string: string, attributes: attributes))
        var ascent: CGFloat = 0, descent: CGFloat = 0, leading: CGFloat = 0
        let advance = CTLineGetTypographicBounds(line, &ascent, &descent, &leading)
        context.saveGState()
        if let clip {
            context.clip(to: CGRect(x: clip.minX, y: CGFloat(height) - clip.maxY,
                                    width: clip.width, height: clip.height))
        }
        context.textMatrix = .identity
        context.textPosition = CGPoint(x: x, y: CGFloat(height) - y - ascent)
        CTLineDraw(line, context)
        context.restoreGState()
        text.append(["value": string, "requestedFont": name,
            "resolvedFont": CTFontCopyPostScriptName(font) as String,
            "fontSize": Double(size), "x": Double(x), "y": Double(y),
            "advance": advance, "ascent": Double(ascent), "descent": Double(descent)])
        return CGRect(x: x, y: y, width: advance, height: ascent + descent)
    }

    func heading(_ count: String?, x: CGFloat = 32, y: CGFloat = 28, size: CGFloat = 24,
                 headingFont: String = "AppleSDGothicNeo-Bold", digitFont: String = "Arial-BoldMT",
                 digitInk: Ink = .blue) {
        let rect = draw("선택 좌석", x: x, y: y, size: size, font: headingFont)
        if let count { draw(count, x: rect.maxX + size * 0.38, y: y, size: size, font: digitFont, ink: digitInk) }
    }

    func box(_ rect: CGRect, fill: Ink, border: Ink? = nil, radius: CGFloat = 0) {
        let flipped = CGRect(x: rect.minX, y: CGFloat(height) - rect.maxY, width: rect.width, height: rect.height)
        let path = CGPath(roundedRect: flipped, cornerWidth: radius, cornerHeight: radius, transform: nil)
        context.addPath(path); context.setFillColor(fill.cg); context.fillPath()
        if let border {
            context.addPath(path); context.setStrokeColor(border.cg); context.setLineWidth(1); context.strokePath()
        }
    }

    func button(_ rect: CGRect, label: String = "선택 완료", disabled: Bool = true, size: CGFloat = 18) {
        box(rect, fill: disabled ? Ink(r: 210, g: 210, b: 210) : Ink(r: 78, g: 82, b: 255), radius: 9)
        let font = CTFontCreateWithName("AppleSDGothicNeo-Bold" as CFString, size, nil)
        let line = CTLineCreateWithAttributedString(NSAttributedString(string: label, attributes: [
            NSAttributedString.Key(kCTFontAttributeName as String): font,
        ]))
        var ascent: CGFloat = 0, descent: CGFloat = 0
        let width = CTLineGetTypographicBounds(line, &ascent, &descent, nil)
        draw(label, x: rect.midX - width / 2, y: rect.midY - (ascent + descent) / 2,
            size: size, ink: Ink(r: 255, g: 255, b: 255))
    }

    func save(to directory: URL, name: String, png: Bool) throws {
        try Data(bytes: pixels, count: width * height * 4).write(to: directory.appendingPathComponent(name + ".rgba"))
        if png {
            let destination = CGImageDestinationCreateWithURL(directory.appendingPathComponent(name + ".png") as CFURL,
                "public.png" as CFString, 1, nil)!
            CGImageDestinationAddImage(destination, context.makeImage()!, nil)
            guard CGImageDestinationFinalize(destination) else { throw NSError(domain: "fixture", code: 1) }
        }
    }
}

let base = URL(fileURLWithPath: CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : FileManager.default.currentDirectoryPath)
let directory = base.appendingPathComponent("fixtures", isDirectory: true)
try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
// Remove only files named in this generator's prior manifest, so renamed test
// cases do not leave stale inputs that could accidentally enter a later run.
if let oldData = try? Data(contentsOf: base.appendingPathComponent("manifest.json")),
   let old = try? JSONSerialization.jsonObject(with: oldData) as? [String: Any],
   let entries = old["fixtures"] as? [[String: Any]] {
    for entry in entries {
        guard let name = entry["name"] as? String,
              name.range(of: "^[A-Za-z0-9_]+$", options: .regularExpression) != nil else { continue }
        for suffix in [".rgba", ".png"] { try? FileManager.default.removeItem(at: directory.appendingPathComponent(name + suffix)) }
    }
}
var fixtures: [[String: Any]] = []

// Required cases are the proposed support contract, not a statement that the
// reader currently passes them. Safety-only cases permit Unreadable, never a
// different count. This keeps unsupported-font failures separate from false reads.
func emit(_ name: String, _ canvas: Canvas, expectation: String, count: Int? = nil,
          group: String, note: String = "", png: Bool = false) throws {
    try canvas.save(to: directory, name: name, png: png)
    var entry: [String: Any] = ["name": name, "file": "fixtures/" + name + ".rgba",
        "width": canvas.width, "height": canvas.height, "stride": canvas.width * 4,
        "expectation": expectation, "group": group, "note": note, "text": canvas.text]
    if let count { entry["count"] = count }
    fixtures.append(entry)
}

for count in 0...9 {
    let canvas = Canvas(); canvas.heading(String(count))
    try emit("digit_\(count)", canvas, expectation: count <= 4 ? "confirmed" : "correct_or_unreadable",
        count: count, group: "digits", note: "Independent CoreText heading and numeral; not copied from reader masks.", png: count == 3)
}

for (name, x, y) in [("top_left", 3.0, 2.0), ("bottom_left", 8.0, 240.0),
                       ("bottom_right", 430.0, 240.0), ("middle", 230.0, 120.0)] {
    let canvas = Canvas(); canvas.heading("3", x: x, y: y)
    try emit("position_\(name)", canvas, expectation: "confirmed", count: 3, group: "position", png: name == "bottom_right")
    let control = Canvas(); control.heading("2", x: x, y: y)
    try emit("position_control_two_\(name)", control, expectation: "confirmed", count: 2, group: "position",
        note: "Digit2 is recognized at default position in initial prototype; separates digit failure from position failure.")
}

for size in [16.0, 19.0, 24.0, 29.0, 36.0, 44.0] {
    let canvas = Canvas(); canvas.heading("2", size: size)
    try emit("size_\(Int(size))", canvas, expectation: size >= 19 && size <= 36 ? "confirmed" : "correct_or_unreadable",
        count: 2, group: "scale")
}

for (label, headingFont, digitFont) in [
    ("neo_regular", "AppleSDGothicNeo-Regular", "ArialMT"),
    ("neo_medium", "AppleSDGothicNeo-Medium", "HelveticaNeue-Medium"),
    ("neo_semibold", "AppleSDGothicNeo-SemiBold", "Helvetica-Bold"),
    ("neo_heavy", "AppleSDGothicNeo-Heavy", "AvenirNext-Heavy"),
    ("apple_gothic", "AppleGothic", "Arial-BoldMT"),
] {
    for count in [1, 2, 3, 4, 8] {
        let canvas = Canvas(); canvas.heading(String(count), headingFont: headingFont, digitFont: digitFont)
        try emit("font_\(label)_\(count)", canvas, expectation: "correct_or_unreadable", count: count, group: "font",
            note: "Alternative-font safety coverage; recognition rate is reported separately.", png: count == 3)
    }
}

for (name, ink) in [("darker_blue", Ink(r: 40, g: 50, b: 220)),
                    ("purple_blue", Ink(r: 100, g: 70, b: 245)),
                    ("dark_digit", Ink.dark)] {
    let canvas = Canvas(); canvas.heading("3", digitInk: ink)
    try emit("ink_\(name)", canvas, expectation: "correct_or_unreadable", count: 3, group: "ink")
}

let empty = Canvas(); empty.heading(nil)
empty.draw("선택된 좌석이 없습니다.", x: 110, y: 155, size: 22, font: "AppleSDGothicNeo-Regular", ink: .gray)
try emit("explicit_empty", empty, expectation: "unreadable", group: "empty_unassociated",
    note: "Former positive reclassified by explicit stronger panel-evidence contract; bare heading+empty is insufficient.", png: true)

let headingOnly = Canvas(); headingOnly.heading(nil)
try emit("heading_without_number", headingOnly, expectation: "unreadable", group: "absence", note: "No numeral is not zero.")
try emit("blank", Canvas(), expectation: "unreadable", group: "absence")
let numberOnly = Canvas(); numberOnly.draw("3", x: 165, y: 28, font: "Arial-BoldMT", ink: .blue)
try emit("number_without_heading", numberOnly, expectation: "unreadable", group: "absence")
let emptyWithoutHeading = Canvas(); emptyWithoutHeading.draw("선택된 좌석이 없습니다.", x: 70, y: 130, ink: .gray)
try emit("empty_without_heading", emptyWithoutHeading, expectation: "unreadable", group: "absence")

for word in ["선택 완료", "좌석 선택", "선택 좌", "전체삭제", "선택 좌석수"] {
    let canvas = Canvas(); let rect = canvas.draw(word, x: 32, y: 28)
    canvas.draw("3", x: rect.maxX + 9, y: 28, font: "Arial-BoldMT", ink: .blue)
    try emit("lookalike_\(fixtures.count)", canvas,
        expectation: word == "선택 좌석수" ? "correct_or_unreadable" : "unreadable",
        count: word == "선택 좌석수" ? 3 : nil, group: "lookalike", note: word)
}

let timer = Canvas(); timer.draw("좌석 선택 시간", x: 32, y: 28)
timer.draw("9:42", x: 210, y: 28, font: "Arial-BoldMT", ink: .blue)
timer.draw("154,000원", x: 32, y: 140, font: "Arial-BoldMT")
try emit("timer_and_price", timer, expectation: "unreadable", group: "distractor", png: true)

let complete = Canvas(); complete.draw("선택 완료", x: 32, y: 28)
try emit("completion_label", complete, expectation: "unreadable", group: "distractor")

for number in ["12", "23", "3:00", "3석", "3.5"] {
    let canvas = Canvas(); canvas.heading(number)
    try emit("compound_number_\(fixtures.count)", canvas,
        expectation: number == "3석" ? "correct_or_unreadable" : "unreadable",
        count: number == "3석" ? 3 : nil, group: "compound", note: number)
}

for counts in [("3", "3"), ("1", "3")] {
    let canvas = Canvas(); canvas.heading(counts.0, x: 28, y: 28); canvas.heading(counts.1, x: 330, y: 170)
    try emit("duplicate_\(counts.0)_\(counts.1)", canvas, expectation: "unreadable", group: "duplicate")
}

let clippedHeading = Canvas(); clippedHeading.draw("선택 좌석", x: -35, y: 28)
clippedHeading.draw("3", x: 88, y: 28, font: "Arial-BoldMT", ink: .blue)
try emit("clipped_heading", clippedHeading, expectation: "unreadable", group: "clipping")
let clippedDigit = Canvas(width: 144, height: 90)
clippedDigit.heading("3", x: 40, y: 28)
try emit("clipped_digit", clippedDigit, expectation: "unreadable", group: "clipping", png: true)

let transparent = Canvas(); transparent.heading("3")
for index in stride(from: 3, to: transparent.width * transparent.height * 4, by: 4) { transparent.pixels[index] = 0 }
try emit("transparent_pixels", transparent, expectation: "unreadable", group: "opacity")

let contradictory = Canvas(); contradictory.heading("3")
contradictory.draw("선택된 좌석이 없습니다.", x: 110, y: 155, size: 22, font: "AppleSDGothicNeo-Regular", ink: .gray)
try emit("contradictory_empty_and_three", contradictory, expectation: "unreadable", group: "contradiction")
let contradictoryTwo = Canvas(); contradictoryTwo.heading("2")
contradictoryTwo.draw("선택된 좌석이 없습니다.", x: 110, y: 155, size: 22, font: "AppleSDGothicNeo-Regular", ink: .gray)
try emit("contradictory_empty_and_two", contradictoryTwo, expectation: "unreadable", group: "contradiction",
    note: "Same contradiction using a numeral already recognized by the initial reader.")

let structured = Canvas(); structured.heading("3", x: 330, y: 28)
structured.draw("211구역 A열 11번", x: 330, y: 90, size: 18, font: "AppleSDGothicNeo-Regular", ink: .gray)
structured.draw("211구역 D열 10번", x: 330, y: 145, size: 18, font: "AppleSDGothicNeo-Regular", ink: .gray)
structured.draw("211구역 F열 10번", x: 330, y: 200, size: 18, font: "AppleSDGothicNeo-Regular", ink: .gray)
structured.draw("좌석 선택 시간", x: 20, y: 25, size: 18)
structured.draw("9:42", x: 140, y: 25, size: 18, font: "Arial-BoldMT", ink: .blue)
try emit("panel_with_list_and_timer", structured, expectation: "confirmed", count: 3, group: "layout", png: true)

// Held out from the first template correction (18/24/32/40). These cases are
// added only after inspecting that correction; they are not asset inputs.
for size in [21.0, 27.0, 35.0] {
    for count in 0...4 {
        let canvas = Canvas(); canvas.heading(String(count), size: size)
        try emit("holdout_size_\(Int(size))_digit_\(count)", canvas, expectation: "confirmed",
            count: count, group: "holdout_scale", note: "Held-out size after first template expansion.")
    }
}
for count in [2, 3] {
    let canvas = Canvas(); canvas.heading(String(count), x: 32.5, y: 28.5)
    try emit("holdout_fractional_digit_\(count)", canvas, expectation: "confirmed", count: count,
        group: "holdout_raster", note: "Subpixel origin changes antialiasing without changing the string.")
}
let emptyControl = Canvas(); emptyControl.heading(nil)
emptyControl.draw("선택된 좌석이 없습니다.", x: 110, y: 155, size: 24, font: "AppleSDGothicNeo-Medium")
try emit("explicit_empty_dark_control", emptyControl, expectation: "unreadable",
    group: "empty_unassociated", note: "Former dark positive reclassified by stronger panel-evidence contract; retained as negative.")
let unrelatedEmpty = Canvas(); unrelatedEmpty.heading(nil)
unrelatedEmpty.draw("3", x: 32, y: 70, font: "Arial-BoldMT", ink: .blue)
unrelatedEmpty.draw("211구역 A열 11번", x: 32, y: 110, size: 18, font: "AppleSDGothicNeo-Regular")
unrelatedEmpty.draw("211구역 D열 10번", x: 32, y: 145, size: 18, font: "AppleSDGothicNeo-Regular")
unrelatedEmpty.draw("211구역 F열 10번", x: 32, y: 180, size: 18, font: "AppleSDGothicNeo-Regular")
unrelatedEmpty.draw("선택된 좌석이 없습니다.", x: 330, y: 190, size: 24, font: "AppleSDGothicNeo-Medium")
try emit("unrelated_empty_with_wrapped_count_three", unrelatedEmpty, expectation: "unreadable",
    group: "association", note: "Unsupported wrapped count with actual rows in one panel and unrelated empty text in another must not become zero.", png: true)

func panelFixture(x: CGFloat, disabled: Bool = true, border: Bool = true, label: String = "선택 완료",
                  includeButton: Bool = true, count: String? = nil, rows: Bool = false) -> Canvas {
    let canvas = Canvas(background: Ink(r: 232, g: 232, b: 232))
    canvas.box(CGRect(x: x, y: 10, width: 360, height: 280), fill: Ink(r: 255, g: 255, b: 255),
        border: border ? Ink(r: 224, g: 224, b: 224) : nil)
    canvas.heading(count, x: x + 20, y: 25)
    canvas.draw("선택된 좌석이 없습니다.", x: x + 50, y: 130, size: 22,
        font: "AppleSDGothicNeo-Regular", ink: .gray)
    if rows { canvas.draw("211구역 A열 11번", x: x + 20, y: 80, size: 18, font: "AppleSDGothicNeo-Regular") }
    if includeButton { canvas.button(CGRect(x: x + 20, y: 235, width: 320, height: 42), label: label, disabled: disabled) }
    return canvas
}
for (name, x, disabled) in [("right_disabled", 260.0, true), ("left_disabled", 20.0, true),
                            ("right_enabled", 260.0, false), ("left_viewport_edge", 0.0, true)] {
    let canvas = panelFixture(x: x, disabled: disabled)
    try emit("panel_empty_\(name)", canvas, expectation: "confirmed", count: 0, group: "panel_empty",
        note: "Declared same-panel heading, explicit-empty and completion-button layout; no absolute viewport quadrant assumption.", png: true)
}
try emit("panel_empty_no_button", panelFixture(x: 260, includeButton: false), expectation: "unreadable", group: "panel_empty_negative")
try emit("panel_empty_wrong_button", panelFixture(x: 260, label: "돌아가기"), expectation: "unreadable", group: "panel_empty_negative")
try emit("panel_empty_with_rows", panelFixture(x: 260, rows: true), expectation: "unreadable", group: "panel_empty_negative")
try emit("panel_empty_with_count_two", panelFixture(x: 260, count: "2"), expectation: "unreadable", group: "panel_empty_negative")

let footer = Canvas(background: Ink(r: 232, g: 232, b: 232))
footer.box(CGRect(x: 10, y: 10, width: 290, height: 210), fill: Ink(r: 255, g: 255, b: 255), border: Ink(r: 224, g: 224, b: 224))
footer.box(CGRect(x: 320, y: 10, width: 310, height: 210), fill: Ink(r: 255, g: 255, b: 255), border: Ink(r: 224, g: 224, b: 224))
footer.heading(nil, x: 30, y: 25)
footer.draw("선택된 좌석이 없습니다.", x: 350, y: 120, size: 22, font: "AppleSDGothicNeo-Regular", ink: .gray)
footer.button(CGRect(x: 20, y: 240, width: 600, height: 42))
try emit("two_panels_shared_footer", footer, expectation: "unreadable", group: "panel_empty_negative", png: true)

for timerText in ["9:42", "9:41"] {
    let canvas = Canvas(); canvas.heading("2")
    canvas.draw(timerText, x: 420, y: 200, size: 18, font: "Arial-BoldMT", ink: .blue)
    try emit("count_two_timer_\(timerText.replacingOccurrences(of: ":", with: "_"))", canvas,
        expectation: "confirmed", count: 2, group: "temporal_control",
        note: "Only the unrelated timer changes between this pair; the selected count remains2.")
}
let outsideHeading = Canvas(); outsideHeading.heading("2"); outsideHeading.heading("2", x: 330, y: 170)
try emit("count_two_second_heading_outside_band", outsideHeading, expectation: "unreadable", group: "temporal_negative",
    note: "A second source appears outside the first numeric evidence band; the original band remains byte-identical.")

let manifest: [String: Any] = ["generator": "CoreText independent text renderer",
    "pixelFormat": "RGBA8, top-to-bottom, opaque unless fixture explicitly tests alpha",
    "evidenceScope": "Synthetic glyph/layout fixtures. No live site or OCR performance guarantee.",
    "fixtures": fixtures]
let json = try JSONSerialization.data(withJSONObject: manifest, options: [.prettyPrinted, .sortedKeys])
try json.write(to: base.appendingPathComponent("manifest.json"))
print("Generated \(fixtures.count) independent fixtures in \(directory.path)")
