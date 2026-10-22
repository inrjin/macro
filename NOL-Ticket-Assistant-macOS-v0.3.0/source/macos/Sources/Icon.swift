import AppKit
let root=URL(fileURLWithPath:CommandLine.arguments[1],isDirectory:true)
try FileManager.default.createDirectory(at:root,withIntermediateDirectories:true)
for (file,size) in [("icon_16x16.png",16),("icon_16x16@2x.png",32),("icon_32x32.png",32),("icon_32x32@2x.png",64),("icon_128x128.png",128),("icon_128x128@2x.png",256),("icon_256x256.png",256),("icon_256x256@2x.png",512),("icon_512x512.png",512),("icon_512x512@2x.png",1024)] {
 let bitmap=NSBitmapImageRep(bitmapDataPlanes:nil,pixelsWide:size,pixelsHigh:size,bitsPerSample:8,samplesPerPixel:4,hasAlpha:true,isPlanar:false,colorSpaceName:.deviceRGB,bytesPerRow:0,bitsPerPixel:0)!
 NSGraphicsContext.saveGraphicsState();NSGraphicsContext.current=NSGraphicsContext(bitmapImageRep:bitmap)
 let s=CGFloat(size);let rect=NSRect(x:s*0.05,y:s*0.05,width:s*0.90,height:s*0.90)
 let background=NSBezierPath(roundedRect:rect,xRadius:s*0.20,yRadius:s*0.20)
 NSGradient(starting:NSColor(red:0.94,green:0.20,blue:0.42,alpha:1),ending:NSColor(red:0.47,green:0.20,blue:0.78,alpha:1))!.draw(in:background,angle:45)
 let ticket=NSBezierPath(roundedRect:NSRect(x:s*0.20,y:s*0.29,width:s*0.60,height:s*0.42),xRadius:s*0.04,yRadius:s*0.04)
 NSColor.white.setFill();ticket.fill()
 NSColor(red:0.71,green:0.20,blue:0.62,alpha:1).setStroke()
 let line=NSBezierPath();line.move(to:NSPoint(x:s*0.61,y:s*0.34));line.line(to:NSPoint(x:s*0.61,y:s*0.66));line.lineWidth=s*0.018;line.setLineDash([s*0.035,s*0.03],count:2,phase:0);line.stroke()
 let mark="N" as NSString;mark.draw(at:NSPoint(x:s*0.29,y:s*0.355),withAttributes:[.font:NSFont.systemFont(ofSize:s*0.27,weight:.black),.foregroundColor:NSColor(red:0.78,green:0.19,blue:0.54,alpha:1)])
 NSGraphicsContext.restoreGraphicsState();try bitmap.representation(using:.png,properties:[:])!.write(to:root.appendingPathComponent(file))
}
