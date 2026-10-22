import Foundation
import CoreText
import CoreGraphics

// Development-only glyph assets. Runtime uses the same Rust matcher on both OSes.
let destination = CommandLine.arguments[1]
var entries: [[String:Any]] = []
func add(_ text: String, _ fontName: String, _ label: String, _ size: CGFloat, _ phase: CGFloat = 0) {
    let font = CTFontCreateWithName(fontName as CFString, size, nil)
    let string = NSAttributedString(string: text, attributes: [
        NSAttributedString.Key(kCTFontAttributeName as String): font,
        NSAttributedString.Key(kCTForegroundColorAttributeName as String): CGColor(gray: 0, alpha: 1)
    ])
    let line = CTLineCreateWithAttributedString(string)
    let width = Int(ceil(CTLineGetTypographicBounds(line, nil, nil, nil))) + 30
    let height = 90
    var rgba = [UInt8](repeating: 255, count: width * height * 4)
    rgba.withUnsafeMutableBytes { bytes in
        let context = CGContext(data: bytes.baseAddress, width: width, height: height, bitsPerComponent: 8,
            bytesPerRow: width * 4, space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
        context.setFillColor(CGColor(gray: 1, alpha: 1)); context.fill(CGRect(x: 0, y: 0, width: width, height: height))
        context.textPosition = CGPoint(x: 12 + phase, y: 24 + phase); CTLineDraw(line, context)
    }
    var xs: [Int] = [], ys: [Int] = []
    for y in 0..<height { for x in 0..<width {
        if rgba[(y * width + x) * 4] < 180 { xs.append(x); ys.append(y) }
    }}
    let x0 = xs.min()!, x1 = xs.max()!, y0 = ys.min()!, y1 = ys.max()!
    var mask = ""
    for y in y0...y1 { for x in x0...x1 { mask.append(rgba[(y * width + x) * 4] < 180 ? "1" : "0") }}
    entries.append(["label": label, "source": fontName, "width": x1-x0+1, "height": y1-y0+1, "mask": mask])
}
// Negative evidence only: common font/size/half-pixel raster references.
// These are rendered words, never crops from the independent fixtures.
for name in ["AppleSDGothicNeo-Regular","AppleSDGothicNeo-Medium","AppleGothic"] {
 for size in stride(from:16.0,through:40.0,by:2.0) {for phase in [0.0,0.5] {
  add("선택된 좌석이 없습니다.",name,"empty_negative",size,phase)
 }}
}
try JSONSerialization.data(withJSONObject:entries,options:[.sortedKeys]).write(to:URL(fileURLWithPath:destination))
print("templates=\(entries.count)")
