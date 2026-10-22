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


let base = URL(fileURLWithPath: CommandLine.arguments[1])
let directory = base.appendingPathComponent("fixtures", isDirectory: true)
try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
var fixtures: [[String: Any]] = []
func emit(_ name: String, _ c: Canvas, _ expected: String, _ count: Int? = nil, _ group: String, _ note: String = "") throws {
    try c.save(to: directory, name: name, png: group == "context" || group == "duplicate")
    var item: [String: Any] = ["name": name, "file": "fixtures/" + name + ".rgba", "width": c.width, "height": c.height, "stride": c.width * 4, "expectation": expected, "group": group, "text": c.text, "note": note]
    if let count { item["count"] = count }
    fixtures.append(item)
}
for size in [20.0, 23.0, 26.0, 30.0, 33.0] {
    for digit in 0...9 {
        let c = Canvas(width: 280, height: 95); c.heading(String(digit), x: 17.25, y: 18.5, size: size)
        try emit("holdout_\(Int(size))_\(digit)", c, size == 23 ? "confirmed" : "correct_or_unreadable", digit, "font_scale")
    }
}
for (fontname, slug) in [("Georgia-Bold", "georgia"), ("Menlo-Bold", "menlo"), ("TrebuchetMS-Bold", "trebuchet"), ("Verdana-Bold", "verdana")] {
    for digit in 0...9 {
        let c = Canvas(width: 280, height: 95); c.heading(String(digit), x: 17, y: 18, digitFont: fontname)
        try emit("font_\(slug)_\(digit)", c, "correct_or_unreadable", digit, "heldout_font")
    }
}
for shade in [0.0, 60.0, 120.0, 160.0, 190.0] {
    for digit in [1, 2, 3, 7] {
        let c = Canvas(width: 280, height: 95); c.heading(String(digit), x: 17, y: 18, digitInk: Ink(r: shade, g: shade, b: shade))
        try emit("gray_\(Int(shade))_\(digit)", c, "correct_or_unreadable", digit, "contrast")
    }
}
for (slug, text, count) in [("maximum", "최대 선택 좌석", 3), ("unselected", "미선택 좌석", 2), ("available", "선택 좌석 제한", 3), ("instruction", "선택 좌석 예시", 1)] {
    let c = Canvas(width: 460, height: 110)
    let r = c.draw(text, x: 20, y: 25)
    c.draw(String(count), x: r.maxX+9, y: 25, font: "Arial-BoldMT", ink: .blue)
    try emit("context_"+slug, c, "unreadable", nil, "context", "Count denotes a limit/unselected/instruction, not current selected total.")
}
for (secondSize, font) in [(16.0, "AppleSDGothicNeo-Bold"), (19.0,"AppleSDGothicNeo-Bold"), (24.0,"AppleSDGothicNeo-Bold"), (24.0,"AppleGothic"), (36.0,"AppleSDGothicNeo-Bold")] {
    let c = Canvas(width: 640, height: 240); c.heading("2", x: 30, y: 20)
    c.heading("3", x: 280, y: 130, size: secondSize, headingFont: font)
    try emit("duplicate_\(Int(secondSize))_"+(font == "AppleGothic" ? "gothic" : "neo"), c, "unreadable", nil, "duplicate", "Both independently rendered selected-total headings visible; no source context chooses between them.")
}
for digit in [1,2,3,7] {
    let c = Canvas(width: 280, height: 95); c.heading(String(digit), x: 17, y: 18)
    try emit("cut_base_\(digit)", c, "confirmed", digit, "cut_base", "Unoccluded control for independently applied white-cover mutations.")
}
let bg = Ink(r: 245, g: 245, b: 245)
for background in [250.0, 245.0, 235.0, 220.0] {
    let c = Canvas(width: 280, height: 95, background: Ink(r: background, g: background, b: background)); c.heading("3", x: 17, y: 18)
    try emit("background_\(Int(background))", c, "correct_or_unreadable", 3, "background", "Nonwhite support counted separately; rejection is not progress.")
}
// Independent whole-panel scene, matching neither the reader's sampled columns nor its crop geometry.
for (slug, dimmed) in [("zero_panel", false), ("zero_dimmed", true)] {
    let c = Canvas(width: 640, height: 340, background: bg)
    c.box(CGRect(x: 280,y: 10,width: 350,height: 315), fill: Ink(r:255,g:255,b:255))
    c.heading(nil, x: 300,y: 30)
    c.draw("선택된 좌석이 없습니다.", x: 310,y: 145,size: 22,font:"AppleSDGothicNeo-Regular",ink:.gray)
    c.button(CGRect(x:300,y:260,width:310,height:42),disabled:true)
    if dimmed {
        for i in stride(from: 0, to: c.width*c.height*4, by: 4) {
            for component in 0...2 { c.pixels[i+component] = UInt8(Double(c.pixels[i+component])*0.6) }
        }
    }
    try emit(slug,c,dimmed ? "unreadable" : "confirmed",dimmed ? nil : 0,"zero", "Synthetic panel only; actual unobscured zero screenshot unavailable. Outer background rasterizes to247 vs panel255. The unobscured panel is a required recognition control; rejection is not a passing substitute for reading zero.")
    if !dimmed {
        for y in 0..<c.height {for x in 0..<c.width {
            if !(280..<630).contains(x) || !(10..<325).contains(y) {
                let i=(y*c.width+x)*4; c.pixels[i]=232; c.pixels[i+1]=232; c.pixels[i+2]=232
            }
        }}
        try emit("zero_panel_strong_boundary",c,"confirmed",0,"zero", "Same independent panel and text; only outer background changed to232 to establish declared distinct boundary support.")
    }
}
let out: [String: Any] = ["renderer": "CoreText system fonts; never reads reader templates", "fixtures": fixtures]
try JSONSerialization.data(withJSONObject: out, options: [.prettyPrinted,.sortedKeys]).write(to: base.appendingPathComponent("manifest.json"))
print("Generated \(fixtures.count) independent cases")
