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

let base=URL(fileURLWithPath:CommandLine.arguments[1])
let directory=base.appendingPathComponent("fixtures",isDirectory:true)
try FileManager.default.createDirectory(at:directory,withIntermediateDirectories:true)
var manifest:[[String:Any]]=[]
func emit(_ name:String,_ canvas:Canvas,_ expected:Int?) throws {
    try canvas.save(to:directory,name:name,png:false)
    manifest.append(["name":name,"width":canvas.width,"height":canvas.height,"stride":canvas.width*4,
      "expected":expected as Any? ?? NSNull(),"text":canvas.text])
}
for gap in [6,12,24,32,36,40] {
 for value in ["1","2","10","12","+1","-1","1.0","1.5","1%","1/2"] {
  let c=Canvas(height:160)
  let h=c.draw("선택 좌석",x:32,y:28)
  c.draw(value,x:h.maxX+CGFloat(gap),y:28,font:"Arial-BoldMT",ink:.blue)
  let name="gap\(gap)_"+value.replacingOccurrences(of:"+",with:"plus").replacingOccurrences(of:"-",with:"minus").replacingOccurrences(of:".",with:"dot").replacingOccurrences(of:"%",with:"pct").replacingOccurrences(of:"/",with:"slash")
  try emit(name,c,(value=="1" || value=="2") ? Int(value) : nil)
 }
}
for suffix in ["1","2","5"] {
 let c=Canvas(height:160)
 let h=c.draw("선택 좌석",x:32,y:28)
 let first=c.draw("1",x:h.maxX+9,y:28,font:"Arial-BoldMT",ink:.blue)
 c.draw(suffix,x:first.maxX+1,y:35,size:14,font:"Arial-BoldMT",ink:.blue)
 try emit("small_suffix_"+suffix,c,nil)
}
for name in ["nearby_price_outside_band","nearby_button_outside_band","separate_lower_text"] {
 let c=Canvas(height:160); c.heading("2")
 if name=="nearby_price_outside_band" {c.draw("132,000",x:210,y:28,font:"Arial-BoldMT")}
 if name=="nearby_button_outside_band" {c.button(CGRect(x:210,y:24,width:190,height:38))}
 if name=="separate_lower_text" {c.draw("1층 5열 23번",x:32,y:95)}
 try emit(name,c,2)
}
for name in ["missing_count","unknown_digit","prefixed_heading","duplicate_heading"] {
 let c=Canvas(height:160)
 if name=="prefixed_heading" {let r=c.draw("최대 선택 좌석",x:32,y:28);c.draw("2",x:r.maxX+9,y:28,font:"Arial-BoldMT",ink:.blue)}
 else {c.heading(name=="missing_count" ? nil : (name=="unknown_digit" ? "X" : "2"))}
 if name=="duplicate_heading" {c.heading("3",x:300,y:90)}
 try emit(name,c,nil)
}
for size in [18.0,20.0,23.0,26.0,28.0,32.0] {for phase in [0.0,0.5] {
 let c=Canvas(height:340,background:Ink(r:232,g:232,b:232))
 c.box(CGRect(x:260,y:10,width:360,height:320),fill:Ink(r:255,g:255,b:255))
 c.heading(nil,x:280+CGFloat(phase/2),y:30+CGFloat(phase),size:CGFloat(size))
 c.draw("선택된 좌석이 없습니다.",x:300,y:145,size:22,font:"AppleSDGothicNeo-Regular",ink:.gray)
 c.button(CGRect(x:280,y:260,width:320,height:42))
 try emit("empty_size_\(Int(size))_phase_\(Int(phase*10))",c,0)
}}
try JSONSerialization.data(withJSONObject:manifest,options:[.prettyPrinted,.sortedKeys]).write(to:base.appendingPathComponent("manifest.json"))
print("fixtures=\(manifest.count)")
