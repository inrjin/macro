import SwiftUI
import AppKit
import Combine
import Darwin

struct ClickStep: Identifiable {
    var id = UUID()
    var enabled = true
    var selector = ""
    var x = ""
    var y = ""
    var delay = "0"
    private var original: [String: Any] = [:]
    init(_ p: [String: Any] = [:]) {
        original = p
        selector = p["selector"] as? String ?? ""
        x = Self.number(p["x"]); y = Self.number(p["y"])
        delay = Self.number(p["delayMs"]) ; if delay.isEmpty { delay = "0" }
    }
    static func number(_ value: Any?) -> String { (value as? NSNumber)?.stringValue ?? "" }
    func object() throws -> [String: Any] {
        var result = original
        if selector != original["selector"] as? String {
            for key in ["relative", "anchorSelector", "imageSource", "text", "kind", "identity"] { result.removeValue(forKey: key) }
        }
        result.removeValue(forKey: "selector"); result.removeValue(forKey: "x"); result.removeValue(forKey: "y")
        if !selector.trimmingCharacters(in: .whitespaces).isEmpty { result["selector"] = selector }
        else {
            guard let a = Double(x), let b = Double(y), a.isFinite, b.isFinite, a >= 0, b >= 0 else { throw AssistantError.message("클릭 좌표 X/Y를 입력하세요.") }
            result["x"] = a; result["y"] = b
        }
        guard let wait = Int(delay), (0...30000).contains(wait) else { throw AssistantError.message("클릭 대기는 0~30000ms입니다.") }
        result["delayMs"] = wait
        return result
    }
}

@MainActor final class AssistantModel: ObservableObject {
    @Published var browser = "chrome"
    @Published var extensionID = ""
    @Published var productURL = ""
    @Published var saleAt = ""
    @Published var buttonSelector = "#booking-aside-container button"
    @Published var buttonText = "예매하기"
    @Published var timeoutMinutes = "30"
    @Published var autoSeat = false
    @Published var readyURL = ""
    @Published var readySelector = ""
    @Published var entryEnabled = false
    @Published var regionName = ""
    @Published var region = ClickStep()
    @Published var regionFrameURL = ""
    @Published var entryBlocked = ""
    @Published var entryReady = ""
    @Published var entryFrameURL = ""
    @Published var mapReady = ""
    @Published var mapFrameURL = ""
    @Published var entryTimeout = "180000"
    @Published var mapTimeout = "10000"
    @Published var mapSelector = ""
    @Published var mapAnchorFrameURL = ""
    @Published var mode = "dom"
    @Published var candidateSelector = ""
    @Published var successSelector = ""
    @Published var successText = ""
    @Published var alertText = ""
    @Published var conflictSelector = ""
    @Published var conflictText = ""
    @Published var outcomeFrameURL = ""
    @Published var attempts = "12"
    @Published var responseTimeout = "4000"
    @Published var roiX = "0"
    @Published var roiY = "0"
    @Published var roiWidth = "100"
    @Published var roiHeight = "100"
    @Published var color = "#8B5CF6"
    @Published var tolerance = "3"
    @Published var minimumArea = "4"
    @Published var maximumArea = "2500"
    @Published var commit = ClickStep()
    @Published var dismiss = ClickStep()
    @Published var after: [ClickStep] = []
    @Published var advancedJSON = ""
    @Published var connected = false
    @Published var statusText = "브라우저 연결 대기"
    @Published var localNotice = "설정은 실전 공연에서 검증되지 않았습니다. 먼저 연습 페이지에서 확인하세요."
    @Published var logText = ""
    @Published var lastUpdate = "—"
    @Published var commandBusy = false
    private var baseProfile = AssistantFiles.defaultProfile()
    private var remoteProfileFingerprint: Data?
    private var ticker: Timer?
    private var localLogs: [String] = []

    init(testing: Bool = false) {
        applyProfile(AssistantFiles.defaultProfile())
        if testing { return }
        if let saved = AssistantFiles.read(AssistantFiles.root.appendingPathComponent("preferences.json")) {
            browser = saved["browser"] as? String ?? browser
            extensionID = saved["extensionID"] as? String ?? ""
            if let config = saved["config"] as? [String: Any] { applyConfig(config) }
            if let profile = saved["profile"] as? [String: Any] { applyProfile(profile) }
        }
        ticker = Timer.scheduledTimer(withTimeInterval: 0.5, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.refreshStatus() }
        }
        refreshStatus()
    }
    func log(_ text: String) {
        localNotice = text
        localLogs.append("\(Date().formatted(date: .omitted, time: .standard))  \(text)")
        localLogs = Array(localLogs.suffix(100))
    }
    func applyConfig(_ c: [String: Any]) {
        productURL = c["url"] as? String ?? productURL
        saleAt = c["saleAt"] as? String ?? ""
        buttonSelector = c["buttonSelector"] as? String ?? buttonSelector
        buttonText = c["buttonText"] as? String ?? buttonText
        autoSeat = c["autoMacro"] as? Bool ?? false; readyURL = c["readyUrl"] as? String ?? ""; readySelector = c["readySelector"] as? String ?? ""
        timeoutMinutes = ClickStep.number(c["timeoutMinutes"]); if timeoutMinutes.isEmpty { timeoutMinutes = "30" }
    }
    func applyProfile(_ p: [String: Any]) {
        baseProfile = p
        let entrance = p["entrance"] as? [String: Any] ?? [:]
        entryEnabled = entrance["enabled"] as? Bool ?? false
        regionName = entrance["name"] as? String ?? ""
        let binding = entrance["region"] as? [String: Any] ?? [:]
        region = ClickStep(binding); regionFrameURL = binding["frameUrl"] as? String ?? ""
        entryBlocked = entrance["blockedSelector"] as? String ?? ""
        entryReady = entrance["readySelector"] as? String ?? ""
        entryFrameURL = entrance["frameUrl"] as? String ?? ""
        mapReady = entrance["mapReadySelector"] as? String ?? ""
        mapFrameURL = entrance["mapFrameUrl"] as? String ?? ""
        entryTimeout = ClickStep.number(entrance["timeoutMs"]); if entryTimeout.isEmpty { entryTimeout = "180000" }
        mapTimeout = ClickStep.number(entrance["mapTimeoutMs"]); if mapTimeout.isEmpty { mapTimeout = "10000" }
        let anchor = p["mapAnchor"] as? [String: Any] ?? [:]
        mapSelector = anchor["selector"] as? String ?? ""; mapAnchorFrameURL = anchor["frameUrl"] as? String ?? ""
        mode = p["mode"] as? String ?? "dom"
        candidateSelector = p["candidateSelector"] as? String ?? ""
        successSelector = p["successSelector"] as? String ?? ""
        successText = p["successText"] as? String ?? ""
        alertText = p["alertText"] as? String ?? ""
        conflictSelector = p["conflictSelector"] as? String ?? ""
        conflictText = p["conflictText"] as? String ?? ""
        outcomeFrameURL = p["outcomeFrameUrl"] as? String ?? ""
        attempts = ClickStep.number(p["maxAttempts"]); if attempts.isEmpty { attempts = "12" }
        responseTimeout = ClickStep.number(p["timeoutMs"]); if responseTimeout.isEmpty { responseTimeout = "4000" }
        let roi = p["roi"] as? [String: Any] ?? [:]
        roiX = ClickStep.number(roi["x"]); roiY = ClickStep.number(roi["y"])
        roiWidth = ClickStep.number(roi["width"]); roiHeight = ClickStep.number(roi["height"])
        color = p["color"] as? String ?? "#8B5CF6"
        tolerance = ClickStep.number(p["tolerance"]); if tolerance.isEmpty { tolerance = "3" }
        minimumArea = ClickStep.number(p["minArea"]); if minimumArea.isEmpty { minimumArea = "4" }
        maximumArea = ClickStep.number(p["maxArea"]); if maximumArea.isEmpty { maximumArea = "2500" }
        commit = ClickStep(p["commit"] as? [String: Any] ?? [:])
        dismiss = ClickStep(p["dismiss"] as? [String: Any] ?? [:])
        after = (p["after"] as? [[String: Any]] ?? []).map { ClickStep($0) }
        advancedJSON = String(data: (try? AssistantFiles.json(p, pretty: true)) ?? Data(), encoding: .utf8) ?? "{}"
    }
    func profile(validate: Bool = false) throws -> [String: Any] {
        var p = baseProfile
        p["mode"] = mode; p["candidateSelector"] = candidateSelector
        p["successSelector"] = successSelector; p["successText"] = successText
        p["alertText"] = alertText; p["conflictSelector"] = conflictSelector; p["conflictText"] = conflictText
        p["outcomeFrameUrl"] = outcomeFrameURL
        guard let count = Int(attempts), let timeout = Int(responseTimeout) else { throw AssistantError.message("시도 횟수와 응답 제한은 정수입니다.") }
        p["maxAttempts"] = count; p["timeoutMs"] = timeout
        p["mapAnchor"] = mapSelector.isEmpty ? NSNull() : ["selector": mapSelector, "frameUrl": mapAnchorFrameURL]
        if mode == "pixels" {
            if mapSelector.isEmpty {
                guard let x = Double(roiX), let y = Double(roiY), let w = Double(roiWidth), let h = Double(roiHeight) else { throw AssistantError.message("지도 요소를 지정하거나 픽셀 영역에 숫자를 입력하세요.") }
                p["roi"] = ["x": x, "y": y, "width": w, "height": h]
            }
            guard let t = Int(tolerance), let minimum = Int(minimumArea), let maximum = Int(maximumArea) else { throw AssistantError.message("색 오차와 크기 범위에 숫자를 입력하세요.") }
            p["color"] = color; p["tolerance"] = t; p["minArea"] = minimum; p["maxArea"] = maximum
        }
        var entrance = p["entrance"] as? [String: Any] ?? [:]
        entrance["enabled"] = entryEnabled; entrance["name"] = regionName
        entrance["blockedSelector"] = entryBlocked; entrance["readySelector"] = entryReady; entrance["frameUrl"] = entryFrameURL
        entrance["mapReadySelector"] = mapReady; entrance["mapFrameUrl"] = mapFrameURL
        entrance["timeoutMs"] = Int(entryTimeout) ?? 0; entrance["mapTimeoutMs"] = Int(mapTimeout) ?? 0
        if var binding = try optionalStep(region) as? [String: Any] { binding["frameUrl"] = regionFrameURL; entrance["region"] = binding }
        else { entrance["region"] = NSNull() }
        p["entrance"] = entrance
        p["commit"] = try optionalStep(commit)
        p["dismiss"] = try optionalStep(dismiss)
        p["after"] = try after.filter(\.enabled).map { try $0.object() }
        // Round trip gives all numeric fields consistent Foundation number bridging.
        p = try AssistantFiles.object(AssistantFiles.json(p))
        if validate { try AssistantFiles.validateProfile(p) }
        return p
    }
    private func optionalStep(_ value: ClickStep) throws -> Any {
        if value.selector.isEmpty && value.x.isEmpty && value.y.isEmpty { return NSNull() }
        return try value.object()
    }
    func config(validate: Bool = true) throws -> [String: Any] {
        if validate || !productURL.isEmpty {
        guard let url = URL(string: productURL), url.scheme == "https", url.host == "nol.yanolja.com", url.user == nil, url.password == nil,
              url.path.range(of: "^/ticket/products/[0-9]+/?$", options: .regularExpression) != nil else { throw AssistantError.message("NOL 상품 URL을 입력하세요.") }
        }
        guard let wait = Int(timeoutMinutes), (1...180).contains(wait) else { throw AssistantError.message("최대 대기는 1~180분입니다.") }
        let time = saleAt.trimmingCharacters(in: .whitespacesAndNewlines).replacingOccurrences(of: " ", with: "T")
        if !time.isEmpty {
            let formatter = DateFormatter(); formatter.locale = Locale(identifier: "en_US_POSIX"); formatter.timeZone = TimeZone(secondsFromGMT: 9*3600)
            formatter.dateFormat = time.count == 16 ? "yyyy-MM-dd'T'HH:mm" : "yyyy-MM-dd'T'HH:mm:ss"; formatter.isLenient = false
            guard let date = formatter.date(from: time), formatter.string(from: date) == time else { throw AssistantError.message("한국시간을 YYYY-MM-DD HH:MM:SS로 입력하세요.") }
        }
        return ["url": productURL, "saleAt": time, "buttonSelector": buttonSelector, "buttonText": buttonText,
                "timeoutMinutes": wait, "autoMacro": autoSeat, "seatDriver": "browser", "readyUrl": readyURL, "readySelector": readySelector, "readyText": "", "browser": browser]
    }
    func save() {
        do {
            try AssistantFiles.write(["browser": browser, "extensionID": extensionID, "config": config(validate: false), "profile": profile()], to: AssistantFiles.root.appendingPathComponent("preferences.json"))
            log("이 Mac에 설정을 저장했습니다.")
        } catch { log(error.localizedDescription) }
    }
    func updateJSON() { do { advancedJSON = String(data: try AssistantFiles.json(profile(), pretty: true), encoding: .utf8)!; log("현재 입력을 JSON에 반영했습니다.") } catch { log(error.localizedDescription) } }
    func applyJSON() {
        do {
            let p = try AssistantFiles.object(Data(advancedJSON.utf8))
            guard ["dom", "pixels"].contains(p["mode"] as? String ?? "") else { throw AssistantError.message("mode는 dom 또는 pixels입니다.") }
            applyProfile(p); log("JSON 입력을 설정에 적용했습니다. 시작할 때 전체 항목을 검증합니다.")
        } catch { log(error.localizedDescription) }
    }
    func refreshStatus() {
        guard let state = AssistantFiles.read(AssistantFiles.root.appendingPathComponent("status-\(browser).json")),
              let received = state["_receivedAt"] as? Double else { if connected { connected = false }; if statusText != "브라우저 연결 대기" { statusText = "브라우저 연결 대기" }; return }
        let age = AssistantFiles.now() - received
        let fromBrowser = state["browser"] as? String
        let isConnected = age >= 0 && age < 6000 && fromBrowser == browser
        if connected != isConnected { connected = isConnected }
        let updated = Date(timeIntervalSince1970: received / 1000).formatted(date: .omitted, time: .standard)
        if lastUpdate != updated { lastUpdate = updated }
        if let id = state["extensionId"] as? String, extensionID.isEmpty { extensionID = id }
        let run = state["seatRun"] as? [String: Any] ?? state["run"] as? [String: Any] ?? [:]
        let label = connected ? (run["phase"] as? String ?? run["status"] as? String ?? "연결됨 · 실행 대기") : "연결 끊김 또는 다른 브라우저"
        if statusText != label { statusText = label }
        if connected, let remote = state["profile"] as? [String: Any], let fingerprint = try? AssistantFiles.json(remote), fingerprint != remoteProfileFingerprint {
            remoteProfileFingerprint = fingerprint; applyProfile(remote)
        }
        let remoteLogs: [String] = (state["logs"] as? [Any] ?? []).suffix(100).map { entry in
            if let text = entry as? String { return text }
            if let item = entry as? [String: Any] {
                return [item["time"], item["at"], item["message"], item["text"]].compactMap { $0.map { String(describing: $0) } }.joined(separator: "  ")
            }
            return String(describing: entry)
        }
        let logs = (remoteLogs + localLogs).suffix(150).joined(separator: "\n")
        if logText != logs { logText = logs }
    }
    func send(_ action: String, kind: String? = nil) {
        do {
            if action != "stop" { guard connected else { throw AssistantError.message("선택한 브라우저에서 확장 프로그램을 연결하세요.") } }
            var payload: [String: Any] = ["browser": browser]
            if action == "seatStart" { payload["profile"] = try profile(validate: true) }
            if action == "seatPick" { payload["kind"] = kind ?? "roi"; payload["profile"] = try profile() }
            if action == "arm" { payload["config"] = try config(); if autoSeat { payload["profile"] = try profile(validate: true) } }
            let command: [String: Any] = ["id": UUID().uuidString, "action": action, "payload": payload]
            try AssistantFiles.validateCommand(command)
            if action == "stop" { try AssistantFiles.write(command, to: AssistantFiles.root.appendingPathComponent("command.json")); log("중지 명령을 보냈습니다."); return }
            guard !commandBusy else { return }
            commandBusy = true
            activateBrowser { [weak self] error in
                guard let self = self else { return }
                if let error = error { self.commandBusy = false; self.log(error.localizedDescription); return }
                NSApp.hide(nil)
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.2) {
                    defer { self.commandBusy = false }
                    do { try AssistantFiles.write(command, to: AssistantFiles.root.appendingPathComponent("command.json")); self.log("브라우저에 \(action) 명령을 전달했습니다.") }
                    catch { self.log(error.localizedDescription) }
                }
            }
        } catch { log(error.localizedDescription) }
    }
    func activateBrowser(completion: @escaping (Error?) -> Void) {
        let bundle = browser == "msedge" ? "com.microsoft.edgemac" : "com.google.Chrome"
        guard let url = NSWorkspace.shared.urlForApplication(withBundleIdentifier: bundle) else { completion(AssistantError.message("선택한 브라우저가 설치되어 있지 않습니다.")); return }
        let options = NSWorkspace.OpenConfiguration(); options.activates = true
        NSWorkspace.shared.openApplication(at: url, configuration: options) { app, error in
            Task { @MainActor in
                if let error = error { completion(error); return }
                guard let app = app, app.activate(options: [.activateAllWindows]) else { completion(AssistantError.message("브라우저를 앞으로 가져오지 못했습니다.")); return }
                completion(nil)
            }
        }
    }
    func installBridge() {
        do {
            guard AssistantFiles.validExtensionID(extensionID) else { throw AssistantError.message("확장 프로그램 ID 32자리(a~p 소문자)를 입력하세요.") }
            guard let executable = Bundle.main.executableURL else { throw AssistantError.message("앱 경로를 확인하지 못했습니다.") }
            let host = executable.deletingLastPathComponent().appendingPathComponent("NolTicketNativeHost")
            guard FileManager.default.isExecutableFile(atPath: host.path) else { throw AssistantError.message("NativeHost가 앱 안에 없습니다. 앱을 다시 빌드하세요.") }
            let relative = browser == "msedge" ? "Library/Application Support/Microsoft Edge/NativeMessagingHosts" : "Library/Application Support/Google/Chrome/NativeMessagingHosts"
            let directory = FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(relative)
            let manifest = directory.appendingPathComponent(AssistantFiles.hostName + ".json")
            if FileManager.default.fileExists(atPath: manifest.path) {
                let backup = manifest.appendingPathExtension("backup-\(Int(AssistantFiles.now()))")
                try FileManager.default.copyItem(at: manifest, to: backup)
            }
            try AssistantFiles.write(["name": AssistantFiles.hostName, "description": "NOL Ticket Assistant local bridge", "path": host.path, "type": "stdio", "allowed_origins": ["chrome-extension://\(extensionID)/"]], to: manifest)
            log("\(browser == "msedge" ? "Edge" : "Chrome") 연결을 등록했습니다. 확장 프로그램을 새로고침하세요.")
        } catch { log(error.localizedDescription) }
    }
    func practice(withEntry: Bool = false) {
        let p: [String: Any] = ["mode":"dom", "candidateSelector":".seat:not(.rejected)", "successSelector":"#success", "successText":"연습: 좌석 선택 성공", "alertText":"연습: 이미 선택된 좌석입니다.", "conflictSelector":"#conflictText", "conflictText":"연습: 이미 선택된 좌석입니다.", "dismiss":["selector":"#dismiss"], "maxAttempts":100, "timeoutMs":4000, "after":[["selector":"#after1"],["selector":"#after2"]]]
        var profile = p
        if withEntry { profile["entrance"] = ["enabled":true, "name":"A 구역", "blockedSelector":"#human-check", "readySelector":"#regions", "region":["selector":"#region-a", "relative":["x":0.5,"y":0.5], "text":"A 구역", "kind":"element"], "mapReadySelector":"#mapWrap[data-region=\"A\"][data-loaded=\"true\"]", "timeoutMs":180000, "mapTimeoutMs":10000] }
        applyProfile(profile); log(withEntry ? "구역 연습 프로필입니다. 연습 페이지에서 ‘구역 진입 연습’을 먼저 켜세요." : "연습 페이지 전용 프로필을 불러왔습니다. 실전 페이지에는 사용할 수 없습니다.")
    }
    func openResource(_ name: String) {
        guard let resources = Bundle.main.resourceURL else { return }
        let target = resources.appendingPathComponent(name)
        guard FileManager.default.fileExists(atPath: target.path) else { log("앱에 \(name) 파일이 없습니다. build.sh를 다시 실행하세요."); return }
        if name.hasSuffix(".html") {
            let bundle = browser == "msedge" ? "com.microsoft.edgemac" : "com.google.Chrome"
            guard let app = NSWorkspace.shared.urlForApplication(withBundleIdentifier: bundle) else { log("선택한 브라우저를 설치하세요."); return }
            NSWorkspace.shared.open([target], withApplicationAt: app, configuration: NSWorkspace.OpenConfiguration()) { _, _ in }
        } else { NSWorkspace.shared.open(target) }
    }
}

struct StepEditor: View {
    @Binding var step: ClickStep
    var body: some View {
        HStack {
            TextField("CSS 선택자 (선택)", text: $step.selector).frame(minWidth: 170)
            TextField("X", text: $step.x).frame(width: 62)
            TextField("Y", text: $step.y).frame(width: 62)
            TextField("대기 ms", text: $step.delay).frame(width: 76)
        }
        .textFieldStyle(.roundedBorder)
    }
}

struct AssistantView: View {
    @StateObject var model = AssistantModel()
    @State private var tab = 0
    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack {
                Image(systemName: "ticket.fill").font(.system(size: 32)).foregroundStyle(.pink)
                VStack(alignment: .leading) { Text("NOL Ticket Assistant").font(.title2.bold()); Text("브라우저 상태를 확인하며 실행하는 macOS 도우미").foregroundStyle(.secondary) }
                Spacer()
                Circle().fill(model.connected ? Color.green : Color.orange).frame(width: 9, height: 9)
                Text(model.statusText).font(.callout.monospaced()).lineLimit(1)
            }
            HStack {
                Button("예매 열림 감시") { model.send("arm") }
                Button("좌석 시도 시작") { model.send("seatStart") }.buttonStyle(.borderedProminent).tint(.pink)
                Button("중지") { model.send("stop") }.keyboardShortcut(".", modifiers: [.command])
                Spacer()
                Button("설정 저장") { model.save() }
            }
            .disabled(model.commandBusy)
            Text("실전 좌석 프로필 미검증 · 예매 성공은 페이지의 실제 성공 표식으로 확인합니다.").font(.caption).foregroundStyle(.orange)
            TabView(selection: $tab) {
                opening.tabItem { Label("예매 감시", systemImage: "clock") }.tag(0)
                entry.tabItem { Label("구역 진입", systemImage: "map") }.tag(5)
                seat.tabItem { Label("좌석 조건", systemImage: "chair") }.tag(1)
                clicks.tabItem { Label("후속 클릭", systemImage: "cursorarrow.click") }.tag(2)
                advanced.tabItem { Label("고급 JSON", systemImage: "curlybraces") }.tag(3)
                connection.tabItem { Label("연결 · 기록", systemImage: "link") }.tag(4)
            }
            Text(model.localNotice).font(.callout).textSelection(.enabled).frame(maxWidth: .infinity, alignment: .leading).padding(10).background(.quaternary.opacity(0.5)).cornerRadius(8)
        }.padding(22).frame(width: 980, height: 790)
    }
    var opening: some View {
        Form {
            Section("상품과 판매 시각") {
                TextField("NOL 상품 URL", text: $model.productURL)
                TextField("판매 시작 · 한국시간", text: $model.saleAt, prompt: Text("2026-11-01 20:00:00 · 비우면 버튼 활성화 감지"))
                TextField("최대 대기 (분)", text: $model.timeoutMinutes)
                Text("시간은 감시를 시작할 기준입니다. 실제 예매 버튼 활성화를 확인한 뒤 클릭합니다.").font(.caption).foregroundStyle(.secondary)
            }
            Section("실제 예매 진입 버튼") {
                TextField("CSS 선택자", text: $model.buttonSelector)
                TextField("버튼 문구", text: $model.buttonText)
                Text("현재 브라우저의 상품 탭을 미리 열어 두세요. 상품 상단의 스크롤용 버튼과 예매 진입 버튼은 다릅니다.").font(.caption).foregroundStyle(.secondary)
            }
            Section("좌석 화면 준비 후 자동 시작 · 선택") {
                Toggle("준비 조건 충족 시 좌석 시도 시작", isOn: $model.autoSeat)
                TextField("준비 요소가 있는 프레임 URL", text: $model.readyURL)
                TextField("좌석 지도 준비 요소 CSS", text: $model.readySelector)
                Text("구역 진입 기능을 켜면 보안 확인 패널 등 예매 화면 진입 요소를 지정할 수 있습니다. 꺼져 있으면 좌석 지도 준비 요소를 지정하세요.").font(.caption).foregroundStyle(.secondary)
            }
            Section("연습") {
                Button("좌석 연습 프로필") { model.practice() }
                Button("구역 진입 연습 프로필") { model.practice(withEntry: true) }
                Button("경쟁 좌석 연습 페이지 열기") { model.openResource("rehearsal-competition.html") }
                Text("먼저 확장의 파일 URL 액세스를 허용하세요. 실제 공연에서는 로그인·대기열·보안 확인을 직접 진행하세요.").font(.caption).foregroundStyle(.secondary)
            }
        }.formStyle(.grouped)
    }
    var entry: some View {
        Form {
            Section("미리 정한 구역 1곳") {
                Toggle("직접 보안 확인 후 지정 구역으로 진입", isOn: $model.entryEnabled)
                TextField("구역 이름 · 표시용", text: $model.regionName)
                HStack { TextField("구역 요소 CSS", text: $model.region.selector); Button("브라우저에서 구역 지정") { model.send("seatPick", kind: "region") } }
                TextField("구역 요소 프레임 URL · 최상위면 비움", text: $model.regionFrameURL)
                Text("화면 좌표를 고정하지 않습니다. 저장한 요소의 현재 위치를 계산합니다. 같은 그림은 크기 변화에 맞춰 보정하며 Canvas 구역의 내부 이동은 지원 전 구조 확인이 필요합니다.").font(.caption).foregroundStyle(.secondary)
            }
            Section("보안 확인 완료와 지도 준비") {
                TextField("보안 확인 패널 CSS · 보이면 대기", text: $model.entryBlocked)
                TextField("구역 선택 화면 준비 CSS", text: $model.entryReady)
                TextField("위 조건의 프레임 URL · 최상위면 비움", text: $model.entryFrameURL)
                TextField("지정 구역 지도 로딩 완료 CSS", text: $model.mapReady)
                TextField("지도 준비 조건 프레임 URL · 같으면 비움", text: $model.mapFrameURL)
                HStack { TextField("사용자 확인 대기 ms", text: $model.entryTimeout); TextField("지도 로딩 제한 ms", text: $model.mapTimeout) }
                Text("문자 입력은 직접 합니다. 지도 조건에는 지정 구역 식별과 로딩 완료를 함께 포함해야 합니다. 그 구역에 후보가 없으면 중지합니다.").font(.caption).foregroundStyle(.secondary)
            }
        }.formStyle(.grouped)
    }
    var seat: some View {
        Form {
            Section("좌석 탐색") {
                Picker("방식", selection: $model.mode) { Text("DOM 요소").tag("dom"); Text("화면 픽셀").tag("pixels") }.pickerStyle(.segmented)
                if model.mode == "dom" { TextField("선택 가능한 좌석 CSS", text: $model.candidateSelector) }
                else {
                    HStack { TextField("좌석 지도 요소 CSS", text: $model.mapSelector); Button("지도 요소 지정") { model.send("seatPick", kind: "map") } }
                    TextField("지도 요소 프레임 URL · 필요할 때", text: $model.mapAnchorFrameURL)
                    if model.mapSelector.isEmpty { HStack { TextField("X", text: $model.roiX); TextField("Y", text: $model.roiY); TextField("너비", text: $model.roiWidth); TextField("높이", text: $model.roiHeight) } }
                    HStack { TextField("색 #RRGGBB", text: $model.color); TextField("오차 0~64", text: $model.tolerance); Button("영역 선택") { model.send("seatPick", kind: "roi") }; Button("색 선택") { model.send("seatPick", kind: "color") } }
                    HStack { TextField("최소 면적", text: $model.minimumArea); TextField("최대 면적", text: $model.maximumArea) }
                    Text("지도 요소를 지정하면 현재 위치와 크기로 검색 범위를 계산합니다. 요소를 비운 경우에만 고정 X/Y 범위를 사용합니다.").font(.caption).foregroundStyle(.secondary)
                }
            }
            Section("성공과 선점 실패 판정") {
                TextField("실제 성공 요소 CSS · 필수", text: $model.successSelector)
                TextField("성공 요소 문구 · 선택", text: $model.successText)
                TextField("선점 실패 alert의 정확한 문구", text: $model.alertText)
                TextField("화면 실패 안내 CSS · alert 대신 사용", text: $model.conflictSelector)
                TextField("화면 실패 안내 정확한 문구", text: $model.conflictText)
                TextField("결과 iframe URL · 필요할 때", text: $model.outcomeFrameURL)
                HStack { TextField("최대 시도", text: $model.attempts); TextField("결과 응답 제한 ms", text: $model.responseTimeout) }
            }
        }.formStyle(.grouped)
    }
    var clicks: some View {
        Form {
            Section("선택 완료 버튼 · 필요할 때") { StepEditor(step: $model.commit); Button("브라우저에서 완료 좌표 선택") { model.send("seatPick", kind: "commit") } }
            Section("화면 실패 안내 닫기 · 필요할 때") { StepEditor(step: $model.dismiss) }
            Section("성공 확인 후 추가 클릭 · 위에서부터 실행") {
                ForEach($model.after) { $step in
                    HStack {
                        Toggle("", isOn: $step.enabled).labelsHidden()
                        StepEditor(step: $step)
                        Button { move(step.id, by: -1) } label: { Image(systemName: "arrow.up") }
                        Button { move(step.id, by: 1) } label: { Image(systemName: "arrow.down") }
                        Button { model.after.removeAll { $0.id == step.id } } label: { Image(systemName: "minus.circle") }
                    }
                }
                HStack {
                    Button("단계 추가") { if model.after.count < 20 { model.after.append(ClickStep()) } }
                    Button("브라우저에서 추가 좌표 선택") { model.send("seatPick", kind: "after") }
                }
                Text("CSS 선택자가 있으면 그 요소를 클릭합니다. 비우면 X/Y를 사용합니다. 완료/닫기를 사용하지 않으면 선택자와 좌표를 모두 비워 두세요.").font(.caption).foregroundStyle(.secondary)
            }
        }.formStyle(.grouped)
    }
    func move(_ id: UUID, by offset: Int) { if let index = model.after.firstIndex(where: { $0.id == id }), model.after.indices.contains(index+offset) { model.after.swapAt(index, index+offset) } }
    var advanced: some View {
        VStack(alignment: .leading) {
            HStack { Button("현재 입력으로 JSON 갱신") { model.updateJSON() }; Button("JSON 입력 적용") { model.applyJSON() }; Spacer() }
            Text("JSON을 수정한 뒤 ‘JSON 입력 적용’을 눌러야 실행 설정에 반영됩니다.").font(.caption).foregroundStyle(.secondary)
            TextEditor(text: $model.advancedJSON).font(.system(.body, design: .monospaced)).border(.quaternary)
        }.padding()
    }
    var connection: some View {
        VStack(alignment: .leading, spacing: 12) {
            Picker("브라우저", selection: $model.browser) { Text("Google Chrome").tag("chrome"); Text("Microsoft Edge").tag("msedge") }.pickerStyle(.segmented)
            TextField("확장 프로그램 ID 32자리", text: $model.extensionID).textFieldStyle(.roundedBorder)
            HStack { Button("확장 프로그램 폴더 열기") { model.openResource("extension") }; Button("이 브라우저에 연결 등록") { model.installBridge() }; Button("연습 페이지") { model.openResource("rehearsal-competition.html") } }
            Text("앱 위치를 정한 뒤 확장 프로그램을 로드하고 ID를 등록하세요. 등록 버튼을 누를 때만 현재 사용자의 브라우저 연결 파일을 만들며, 기존 파일은 백업합니다.").font(.caption).foregroundStyle(.secondary)
            Text("최근 연결 확인: \(model.lastUpdate) · 선택한 브라우저에서 실행할 탭을 활성화하세요.").font(.caption)
            Divider()
            ScrollView { Text(model.logText.isEmpty ? "아직 기록이 없습니다." : model.logText).font(.system(.caption, design: .monospaced)).textSelection(.enabled).frame(maxWidth: .infinity, alignment: .leading).padding(8) }.background(.quaternary.opacity(0.3)).cornerRadius(6)
        }.padding()
    }
}

@MainActor func runAppSelfTest() throws -> [String] {
    var checks = try runSharedSelfTest()
    let model = AssistantModel(testing: true)
    var refused = false
    do { _ = try model.profile(validate: true) } catch { refused = true }
    guard refused else { throw AssistantError.message("An unconfigured seat profile was accepted") }
    model.practice(); let p = try model.profile(validate: true)
    guard (p["after"] as? [[String: Any]])?.count == 2 else { throw AssistantError.message("Practice actions missing") }
    model.after[0].enabled = false
    guard (try model.profile()["after"] as? [[String: Any]])?.count == 1 else { throw AssistantError.message("Disabled step was sent") }
    model.updateJSON(); model.applyJSON()
    guard model.after.count == 1 else { throw AssistantError.message("JSON round trip failed") }
    _ = try model.config(validate: false)
    refused = false; do { _ = try model.config() } catch { refused = true }
    guard refused else { throw AssistantError.message("Empty product was armed") }
    model.productURL = "https://nol.yanolja.com/ticket/products/26010352"
    model.saleAt = "2026-11-01 20:00:00"
    guard try model.config()["saleAt"] as? String == "2026-11-01T20:00:00" else { throw AssistantError.message("KST configuration failed") }
    model.practice(withEntry: true)
    model.updateJSON(); model.applyJSON()
    let region = (try model.profile(validate: true)["entrance"] as? [String: Any])?["region"] as? [String: Any]
    guard (region?["relative"] as? [String: Any])?["x"] as? Double == 0.5, region?["text"] as? String == "A 구역" else { throw AssistantError.message("Region identity round trip failed") }
    model.mode = "pixels"; model.mapSelector = "#seats"; model.roiX = ""; model.roiY = ""; model.roiWidth = ""; model.roiHeight = ""
    _ = try model.profile(validate: true)
    checks += ["entry identity and relative binding JSON round trip", "map anchor without fixed ROI"]
    checks += ["unconfigured profile refused", "practice and optional steps", "GUI JSON round trip", "blank product saves but cannot arm", "KST config normalization"]
    return checks
}

@main struct TicketAssistantApp: App {
    init() {
        if CommandLine.arguments.contains("--self-test") {
            do { print(String(data: try AssistantFiles.json(["ok": true, "checks": runAppSelfTest()]), encoding: .utf8)!); Darwin.exit(0) }
            catch { fputs("Self-test failed: \(error.localizedDescription)\n", stderr); Darwin.exit(1) }
        }
    }
    var body: some Scene {
        WindowGroup { AssistantView() }
            .windowStyle(.titleBar)
            .defaultSize(width: 980, height: 790)
            .windowResizability(.contentSize)
            .commands { CommandGroup(replacing: .newItem) {} }
    }
}
