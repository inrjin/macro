import Foundation
import Darwin

final class NativeBridge {
    let root: URL
    private let outputLock = NSLock()
    private var lastCommandID: String?
    private var timer: DispatchSourceTimer?

    init(root: URL) {
        self.root = root
        // A newly connected browser must not replay a command from an earlier session.
        self.lastCommandID = AssistantFiles.read(root.appendingPathComponent("command.json"))?["id"] as? String
    }
    func send(_ message: [String: Any]) throws {
        let data = try NativeFrame.encode(message)
        outputLock.lock(); defer { outputLock.unlock() }
        try FileHandle.standardOutput.write(contentsOf: data)
    }
    func pollCommand() {
        guard let command = AssistantFiles.read(root.appendingPathComponent("command.json")),
              let id = command["id"] as? String, id != lastCommandID else { return }
        // Remember even an invalid command, avoiding repeated work on malformed input.
        lastCommandID = id
        guard (try? AssistantFiles.validateCommand(command)) != nil else { return }
        var message = command; message["op"] = "appCommand"
        do { try send(message) } catch { Darwin.exit(1) }
    }
    func run() throws {
        try AssistantFiles.makeDirectory(root)
        let timer = DispatchSource.makeTimerSource(queue: DispatchQueue(label: "com.nol-ticket-assistant.command-poll"))
        timer.schedule(deadline: .now() + .milliseconds(100), repeating: .milliseconds(100), leeway: .milliseconds(10))
        timer.setEventHandler { [weak self] in self?.pollCommand() }
        self.timer = timer; timer.resume()
        defer { timer.cancel() }
        while let message = try NativeFrame.read(.standardInput) {
            switch message["op"] as? String {
            case "ping": try send(["ok": true, "version": 1])
            case "status":
                guard var state = message["state"] as? [String: Any] else {
                    try send(["ok": false, "error": "invalid_status"]); continue
                }
                state["_receivedAt"] = AssistantFiles.now()
                let browser = state["browser"] as? String ?? "chrome"
                guard ["chrome", "msedge"].contains(browser) else { continue }
                try AssistantFiles.write(state, to: root.appendingPathComponent("status-\(browser).json"))
            default: try send(["ok": false, "error": "unsupported_operation"])
            }
        }
    }
}

@main struct NativeHostMain {
    static func main() {
        signal(SIGPIPE, SIG_IGN)
        if CommandLine.arguments.contains("--self-test") {
            do {
                let checks = try runSharedSelfTest()
                print(String(data: try AssistantFiles.json(["ok": true, "checks": checks]), encoding: .utf8)!)
            } catch { fputs("Self-test failed: \(error.localizedDescription)\n", stderr); Darwin.exit(1) }
            return
        }
        do {
            var root = AssistantFiles.root
            // Integration tests may isolate IPC in a temporary directory. Browser manifests never use this flag.
            if let index = CommandLine.arguments.firstIndex(of: "--test-root"), index + 1 < CommandLine.arguments.count {
                let candidate = URL(fileURLWithPath: CommandLine.arguments[index + 1], isDirectory: true).standardizedFileURL.resolvingSymlinksInPath()
                let temp = FileManager.default.temporaryDirectory.standardizedFileURL.resolvingSymlinksInPath().path + "/"
                guard candidate.path.hasPrefix(temp) else { throw AssistantError.message("Test root must be in the current user's temporary directory.") }
                root = candidate
            }
            let bridge = NativeBridge(root: root)
            try withExtendedLifetime(bridge) { try bridge.run() }
        } catch {
            // stderr is diagnostic only; stdout is always native messaging framing.
            fputs("NOL native bridge: \(error.localizedDescription)\n", stderr)
            Darwin.exit(1)
        }
    }
}
