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


let base=URL(fileURLWithPath:CommandLine.arguments[1]);let directory=base.appendingPathComponent("fixtures",isDirectory:true)
try FileManager.default.createDirectory(at:directory,withIntermediateDirectories:true)
var fixtures:[[String:Any]]=[]
for (name,withEmpty,kind,gap) in [("empty_near_block",true,"block",4.0),("empty_near_text",true,"text",4.0),("empty_far_block",true,"block",80.0),("noempty_near_block",false,"block",4.0),("noempty_near_text",false,"text",4.0),("noempty_far_block",false,"block",80.0)] {
 let c=Canvas();c.heading("2")
 // This independently measured string advance only positions a neighboring UI
 // element; it is never passed to the production reader.
 let measurement=Canvas();let word=measurement.draw("선택된 좌석이 없습니다.",x:260,y:150,size:20,font:"AppleSDGothicNeo-Regular",ink:.gray)
 if withEmpty {c.draw("선택된 좌석이 없습니다.",x:260,y:150,size:20,font:"AppleSDGothicNeo-Regular",ink:.gray)}
 if kind=="block" {c.box(CGRect(x:word.maxX+gap,y:152,width:60,height:17),fill:.dark)}
 else {c.draw("닫기",x:word.maxX+gap,y:150,size:20,font:"AppleSDGothicNeo-Regular",ink:.dark)}
 try c.save(to:directory,name:name,png:true)
 var fixture:[String:Any]=["name":name,"file":"empty-neighbor/fixtures/"+name+".rgba","width":640,"height":300,"stride":2560,"group":"empty_neighbor","expectation":withEmpty ? "unreadable" : "confirmed","text":c.text]
 if !withEmpty {fixture["count"]=2}
 fixtures.append(fixture)
}
try JSONSerialization.data(withJSONObject:["fixtures":fixtures,"note":"Independent adjacent UI positive/negative controls; known Neo20 empty detector miss must not be revived by greedy full-line merging."],options:[.prettyPrinted,.sortedKeys]).write(to:base.appendingPathComponent("manifest.json"))
print("Generated \(fixtures.count) neighbor controls")
