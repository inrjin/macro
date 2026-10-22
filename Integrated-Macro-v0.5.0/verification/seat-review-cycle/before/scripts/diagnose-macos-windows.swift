// Read-only diagnostic. Does not capture pixels, request permissions or send input.
// Window titles, URLs and browser contents are deliberately omitted.
import AppKit
import CoreGraphics
import Foundation

func rectangle(_ window: [String: Any]) -> CGRect? {
    guard let value = window[kCGWindowBounds as String] as? [String: Any] else { return nil }
    return CGRect(dictionaryRepresentation: value as CFDictionary)
}

func metadata(_ window: [String: Any], index: Int, point: CGPoint?) -> [String: Any] {
    var row: [String: Any] = ["index": index]
    for (name, key) in [
        ("id", kCGWindowNumber), ("pid", kCGWindowOwnerPID), ("owner", kCGWindowOwnerName),
        ("layer", kCGWindowLayer), ("alpha", kCGWindowAlpha), ("bounds", kCGWindowBounds),
        ("onScreen", kCGWindowIsOnscreen), ("sharingState", kCGWindowSharingState),
    ] {
        if let value = window[key as String] { row[name] = value }
    }
    if let point { row["containsPointer"] = rectangle(window)?.contains(point) ?? false }
    return row
}

print("5초 뒤 창 정보만 확인합니다. 오류가 나는 예매 버튼 위로 마우스를 옮기고 2초간 그대로 두세요. 클릭하거나 F8을 누를 필요는 없습니다.")
fflush(stdout)
Thread.sleep(forTimeInterval: 5)
var samples: [[String: Any]] = []
for index in 0..<3 {
    let point = CGEvent(source: nil)?.location
    let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements],
                                           kCGNullWindowID) as? [[String: Any]]
    let front = NSWorkspace.shared.frontmostApplication
    var sample: [String: Any] = [
        "sample": index + 1,
        "windowListAvailable": windows?.isEmpty == false,
        "cursorLayer": CGWindowLevelForKey(.cursorWindow),
        "screenPermission": CGPreflightScreenCaptureAccess(),
    ]
    if let front { sample["frontApp"] = front.localizedName ?? ""; sample["frontPID"] = front.processIdentifier }
    if let point { sample["point"] = ["x": point.x, "y": point.y] }
    sample["windowsAtPointer"] = (windows ?? []).enumerated().filter { _, window in
        guard let point else { return false }
        return rectangle(window)?.contains(point) == true
    }.prefix(12).map { metadata($0.element, index: $0.offset, point: point) }
    sample["chromeWindows"] = (windows ?? []).enumerated().filter { _, window in
        (window[kCGWindowOwnerName as String] as? String) == "Google Chrome"
            && (window[kCGWindowLayer as String] as? Int) == 0
    }.map { metadata($0.element, index: $0.offset, point: point) }
    samples.append(sample)
    if index < 2 { Thread.sleep(forTimeInterval: 0.5) }
}
let data = try JSONSerialization.data(withJSONObject: [
    "diagnostic": "macro-window-occlusion",
    "note": "창 이름·주소·화면 이미지·로그인 정보는 수집하지 않았습니다. 빈 창 목록은 이 실행 환경에서 조회 불가를 뜻합니다.",
    "samples": samples,
], options: [.prettyPrinted, .sortedKeys])
print(String(data: data, encoding: .utf8)!)
