import Foundation
import Darwin

enum AssistantFiles {
    static let hostName = "com.nol_ticket_assistant.mac"
    static let maxFrameBytes = 65_536
    static let actions: Set<String> = ["seatStart", "stop", "seatPick", "arm"]
    static var root: URL {
        FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent("Library/Application Support/NOLTicketAssistant", isDirectory: true)
    }
    static func now() -> Double { Date().timeIntervalSince1970 * 1000 }
    static func json(_ value: Any, pretty: Bool = false) throws -> Data {
        try JSONSerialization.data(withJSONObject: value, options: pretty ? [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes] : [.sortedKeys])
    }
    static func object(_ data: Data) throws -> [String: Any] {
        guard let result = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw AssistantError.message("JSON 객체가 필요합니다.")
        }
        return result
    }
    static func read(_ url: URL) -> [String: Any]? {
        guard let values = try? url.resourceValues(forKeys: [.fileSizeKey]), let size = values.fileSize,
              size <= maxFrameBytes, let data = try? Data(contentsOf: url) else { return nil }
        return try? object(data)
    }
    static func makeDirectory(_ directory: URL) throws {
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
    }
    static func write(_ object: [String: Any], to target: URL) throws {
        let bytes = try json(object, pretty: true)
        guard bytes.count <= maxFrameBytes else { throw AssistantError.message("저장할 JSON은 64 KiB 이하여야 합니다.") }
        try makeDirectory(target.deletingLastPathComponent())
        let temporary = target.deletingLastPathComponent().appendingPathComponent(".\(target.lastPathComponent).\(UUID().uuidString).tmp")
        let fd = Darwin.open(temporary.path, O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW, 0o600)
        guard fd >= 0 else { throw AssistantError.message("임시 파일을 만들지 못했습니다 (\(errno)).") }
        var descriptorOpen = true
        defer {
            if descriptorOpen { Darwin.close(fd) }
            try? FileManager.default.removeItem(at: temporary)
        }
        try bytes.withUnsafeBytes { buffer in
            guard let base = buffer.baseAddress else { return }
            var offset = 0
            while offset < buffer.count {
                let count = Darwin.write(fd, base.advanced(by: offset), buffer.count - offset)
                if count < 0 && errno == EINTR { continue }
                guard count > 0 else { throw AssistantError.message("파일 저장에 실패했습니다 (\(errno)).") }
                offset += count
            }
        }
        guard fsync(fd) == 0 else { throw AssistantError.message("파일 동기화에 실패했습니다.") }
        Darwin.close(fd); descriptorOpen = false
        guard rename(temporary.path, target.path) == 0 else { throw AssistantError.message("파일 교체에 실패했습니다 (\(errno)).") }
    }
    static func validExtensionID(_ value: String) -> Bool {
        value.range(of: "^[a-p]{32}$", options: .regularExpression) != nil
    }
    static func validateCommand(_ value: [String: Any]) throws {
        guard let id = value["id"] as? String, UUID(uuidString: id) != nil,
              let action = value["action"] as? String, actions.contains(action),
              value["payload"] is [String: Any] else {
            throw AssistantError.message("지원하지 않는 앱 명령입니다.")
        }
    }
    static func defaultProfile() -> [String: Any] {
        ["mode": "dom", "candidateSelector": "", "successSelector": "", "successText": "", "alertText": "",
         "conflictSelector": "", "conflictText": "", "dismiss": NSNull(), "commit": NSNull(), "after": [],
         "maxAttempts": 12, "timeoutMs": 4000, "outcomeFrameUrl": "",
         "roi": ["x": 0, "y": 0, "width": 100, "height": 100], "color": "#8B5CF6", "tolerance": 3, "minArea": 4, "maxArea": 2500]
    }
    static func validateProfile(_ p: [String: Any]) throws {
        guard let mode = p["mode"] as? String, ["dom", "pixels"].contains(mode) else { throw AssistantError.message("mode는 dom 또는 pixels입니다.") }
        func text(_ key: String) -> String { (p[key] as? String ?? "").trimmingCharacters(in: .whitespacesAndNewlines) }
        if mode == "dom" && text("candidateSelector").isEmpty { throw AssistantError.message("선택 가능한 좌석의 CSS 선택자를 입력하세요.") }
        if mode == "pixels" {
            if ((p["mapAnchor"] as? [String: Any])?["selector"] as? String ?? "").isEmpty {
            guard let roi = p["roi"] as? [String: Any], let x = roi["x"] as? Double, let y = roi["y"] as? Double,
                  let w = roi["width"] as? Double, let h = roi["height"] as? Double,
                  x.isFinite, y.isFinite, w.isFinite, h.isFinite, x >= 0, y >= 0, w >= 1, h >= 1, w*h <= 2_000_000 else {
                throw AssistantError.message("좌석 영역은 양수 크기이며 최대 200만 CSS 픽셀이어야 합니다.")
            }
            }
            guard text("color").range(of: "^#[0-9a-fA-F]{6}$", options: .regularExpression) != nil else { throw AssistantError.message("좌석 색은 #RRGGBB 형식입니다.") }
            let tolerance = (p["tolerance"] as? Int) ?? 3, minimum = (p["minArea"] as? Int) ?? 4, maximum = (p["maxArea"] as? Int) ?? 2500
            guard (0...64).contains(tolerance), minimum >= 1, maximum >= minimum else { throw AssistantError.message("색 오차와 좌석 크기 범위를 확인하세요.") }
        }
        guard !text("successSelector").isEmpty else { throw AssistantError.message("실제 성공을 확인할 CSS 선택자가 필요합니다.") }
        guard !text("alertText").isEmpty || (!text("conflictSelector").isEmpty && !text("conflictText").isEmpty && p["dismiss"] is [String: Any]) else {
            throw AssistantError.message("확인된 선점 실패 문구 또는 안내창 조건/닫기 단계를 지정하세요.")
        }
        guard let attempts = p["maxAttempts"] as? Int, (1...100).contains(attempts),
              let timeout = p["timeoutMs"] as? Int, (250...30000).contains(timeout) else { throw AssistantError.message("시도 횟수는 1~100, 응답 제한은 250~30000ms입니다.") }
        guard let after = p["after"] as? [[String: Any]], after.count <= 20 else { throw AssistantError.message("후속 클릭은 최대 20개입니다.") }
        var steps = after
        if let e = p["entrance"] as? [String: Any], e["enabled"] as? Bool == true {
            guard ["blockedSelector", "readySelector", "mapReadySelector"].allSatisfy({ !(e[$0] as? String ?? "").trimmingCharacters(in: .whitespaces).isEmpty }), let region = e["region"] as? [String: Any], !(region["selector"] as? String ?? "").isEmpty else { throw AssistantError.message("보안 확인 패널, 구역 준비 조건, 지정 구역, 해당 지도 준비 조건을 입력하세요.") }
            guard region["kind"] as? String != "canvas" else { throw AssistantError.message("Canvas 구역 내부의 확대·이동 구조 확인이 필요합니다.") }
            let wait = e["timeoutMs"] as? Int ?? 180000, mapWait = e["mapTimeoutMs"] as? Int ?? 10000
            guard (1000...600000).contains(wait), (250...60000).contains(mapWait) else { throw AssistantError.message("구역 진입 대기 시간 범위를 확인하세요.") }
            steps.append(region)
        }
        if let c = p["commit"] as? [String: Any] { steps.append(c) }
        if let d = p["dismiss"] as? [String: Any] { steps.append(d) }
        for step in steps {
            if let relative = step["relative"] as? [String: Any] {
                guard !(step["selector"] as? String ?? "").isEmpty, ["x", "y"].allSatisfy({ key in if let n = relative[key] as? Double { return n.isFinite && (0...1).contains(n) }; return false }) else { throw AssistantError.message("구역 상대 좌표는 0~1이어야 합니다.") }
            }
            let selector = (step["selector"] as? String ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
            let x = step["x"] as? Double, y = step["y"] as? Double
            guard !selector.isEmpty || (x != nil && y != nil && x!.isFinite && y!.isFinite && x! >= 0 && y! >= 0) else { throw AssistantError.message("클릭 단계에는 CSS 선택자 또는 X/Y가 필요합니다.") }
            if let delay = step["delayMs"] as? Double, !delay.isFinite || delay < 0 || delay > 30000 { throw AssistantError.message("클릭 대기는 0~30000ms입니다.") }
        }
    }
}

enum AssistantError: LocalizedError {
    case message(String)
    var errorDescription: String? { if case .message(let text) = self { return text }; return nil }
}

enum NativeFrame {
    static func encode(_ value: [String: Any]) throws -> Data {
        let payload = try AssistantFiles.json(value)
        guard payload.count > 0, payload.count <= AssistantFiles.maxFrameBytes else { throw AssistantError.message("Invalid native frame length") }
        let n = UInt32(payload.count)
        var data = Data([UInt8(n & 255), UInt8((n >> 8) & 255), UInt8((n >> 16) & 255), UInt8((n >> 24) & 255)])
        data.append(payload); return data
    }
    static func readExactly(_ input: FileHandle, count: Int, allowEOF: Bool = false) throws -> Data? {
        var result = Data()
        while result.count < count {
            let next = try input.read(upToCount: count - result.count) ?? Data()
            if next.isEmpty {
                if allowEOF && result.isEmpty { return nil }
                throw AssistantError.message("Truncated native frame")
            }
            result.append(next)
        }
        return result
    }
    static func read(_ input: FileHandle) throws -> [String: Any]? {
        guard let header = try readExactly(input, count: 4, allowEOF: true) else { return nil }
        let n = header.enumerated().reduce(UInt32(0)) { $0 | (UInt32($1.element) << (UInt32($1.offset)*8)) }
        guard n > 0, n <= AssistantFiles.maxFrameBytes else { throw AssistantError.message("Invalid native frame length") }
        guard let payload = try readExactly(input, count: Int(n)) else { throw AssistantError.message("Missing native payload") }
        return try AssistantFiles.object(payload)
    }
}

func runSharedSelfTest() throws -> [String] {
    let temporary = FileManager.default.temporaryDirectory.appendingPathComponent("nol-mac-test-\(UUID().uuidString)")
    defer { try? FileManager.default.removeItem(at: temporary) }
    let target = temporary.appendingPathComponent("status.json")
    try AssistantFiles.write(["message": "한글", "version": 1], to: target)
    guard AssistantFiles.read(target)?["message"] as? String == "한글" else { throw AssistantError.message("Atomic JSON round trip failed") }
    let permissions = try FileManager.default.attributesOfItem(atPath: target.path)[.posixPermissions] as? NSNumber
    guard permissions?.intValue == 0o600 else { throw AssistantError.message("Private file permissions failed") }
    try AssistantFiles.write(["version": 2], to: target)
    guard AssistantFiles.read(target)?["version"] as? Int == 2 else { throw AssistantError.message("Atomic replace failed") }
    let frame = try NativeFrame.encode(["op": "ping", "text": "한글"])
    let frameFile = temporary.appendingPathComponent("frame")
    try frame.write(to: frameFile)
    let reader = try FileHandle(forReadingFrom: frameFile)
    defer { try? reader.close() }
    guard try NativeFrame.read(reader)?["op"] as? String == "ping", try NativeFrame.read(reader) == nil else { throw AssistantError.message("Framing failed") }
    guard AssistantFiles.validExtensionID("abcdefghijklmnopabcdefghijklmnop"), !AssistantFiles.validExtensionID("../../") else { throw AssistantError.message("Extension allowlist validation failed") }
    try AssistantFiles.validateCommand(["id": UUID().uuidString, "action": "seatStart", "payload": [:]])
    var rejected = false
    do { try AssistantFiles.validateCommand(["id": UUID().uuidString, "action": "exec", "payload": [:]]) } catch { rejected = true }
    guard rejected else { throw AssistantError.message("Unknown action was accepted") }
    var p = AssistantFiles.defaultProfile(); p["candidateSelector"] = ".seat"; p["successSelector"] = ".selected"; p["alertText"] = "선점된 좌석"
    try AssistantFiles.validateProfile(p)
    p["after"] = [["x": -1, "y": 4]]; rejected = false
    do { try AssistantFiles.validateProfile(p) } catch { rejected = true }
    guard rejected else { throw AssistantError.message("Negative click coordinate was accepted") }
    return ["private atomic JSON write/replace", "native framing UTF-8/EOF", "extension ID allowlist", "command allowlist", "profile valid/invalid boundaries"]
}
