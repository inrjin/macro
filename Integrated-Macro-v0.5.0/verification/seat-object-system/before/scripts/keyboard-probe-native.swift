// Standalone diagnostic; never reads characters, posts input, requests access,
// activates a window, or changes secure input. Output is routing metadata only.
// --preflight (default): read this process's permissions; install no listener.
// --synthetic: compare unposted CGEvent/NSEvent factories; install no listener.
// --monitor [--compare-tap] [--duration 20]: explicit live observation, at most
// 16 character-candidate records per API, followed by aggregate counters.
// A standalone process has different TCC identity/run-loop context from the
// released app. Success here does not prove the released app receives keys.
import AppKit
import ApplicationServices
import Carbon
import Foundation

func emit(_ value: [String: Any]) {
    guard let data = try? JSONSerialization.data(withJSONObject: value, options: [.sortedKeys]),
          let line = String(data: data, encoding: .utf8) else { return }
    print(line)
    fflush(stdout)
}

func candidate(_ code: UInt16) -> Bool {
    switch code {
    case 0...9, 11...23, 25, 26, 28, 29, 31, 32, 34, 35, 37, 38, 40, 45, 46,
         82...89, 91, 92: return true
    default: return false
    }
}

func environment() -> [String: Any] {
    ["pid": getpid(), "accessibilityTrusted": AXIsProcessTrusted(),
     "listenEventAccess": CGPreflightListenEventAccess(),
     "postEventAccess": CGPreflightPostEventAccess(),
     "secureEventInput": IsSecureEventInputEnabled(),
     "nowNs": DispatchTime.now().uptimeNanoseconds]
}

func cgMetadata(_ event: CGEvent?) -> [String: Any] {
    guard let event else { return ["cgEventPresent": false] }
    return ["cgEventPresent": true,
        "sourcePID": event.getIntegerValueField(.eventSourceUnixProcessID),
        "recipientPID": event.getIntegerValueField(.eventTargetUnixProcessID),
        "eventTimestampNs": event.timestamp]
}

func nsMetadata(_ event: NSEvent) -> [String: Any] {
    var result = cgMetadata(event.cgEvent)
    result["windowNumber"] = event.windowNumber
    result["nsEventTimestampSeconds"] = event.timestamp
    result["repeated"] = event.isARepeat
    result["modifiers"] = event.modifierFlags.rawValue
    // A second getter makes lazy conversion/caching differences observable.
    result["secondCGRead"] = cgMetadata(event.cgEvent)
    return result
}

func synthetic() -> Int32 {
    // Do not set source/recipient fields: inspect the actual factory defaults.
    guard let quartz = CGEvent(keyboardEventSource: nil, virtualKey: 0, keyDown: true),
          let wrapped = NSEvent(cgEvent: quartz),
          let cocoa = NSEvent.keyEvent(with: .keyDown, location: .zero,
            modifierFlags: [], timestamp: 1, windowNumber: 12345, context: nil,
            characters: "", charactersIgnoringModifiers: "", isARepeat: false, keyCode: 0) else {
        emit(["kind": "synthetic_error", "reason": "event_creation_failed"])
        return 1
    }
    emit(["kind": "synthetic", "pid": getpid(), "cgFactory": cgMetadata(quartz),
        "cgWrappedInNSEvent": nsMetadata(wrapped), "nsKeyEventFactory": nsMetadata(cocoa),
        "limitation": "Unposted factories are not a physical global-monitor callback."])
    return 0
}

func productionRejection(repeated: Bool, modifiers: NSEvent.ModifierFlags,
                         source: Int64, recipient: Int64, front: pid_t?) -> String? {
    if repeated { return "repeated" }
    if source == Int64(getpid()) { return "own_source" }
    if !modifiers.intersection([.control, .option, .command, .function]).isEmpty { return "modifier_pressed" }
    guard let front, front > 0 else { return "front_app_unavailable" }
    if front == getpid() { return "own_front_app" }
    if recipient > 0 && recipient != Int64(front) { return "recipient_mismatch" }
    return nil
}

final class Probe {
    var monitor: Any?
    var tap: CFMachPort?
    var source: CFRunLoopSource?
    var totals: [String: Int] = [:]
    var candidates: [String: Int] = [:]
    var emitted: [String: Int] = [:]
    // Both listeners run on the main run loop. At most 16 records per channel
    // can be queued; window enumeration/JSON/I/O never runs in a tap callback.
    var pending: [[String: Any]] = []
    var heartbeatCount = 0
    var heartbeat: Timer?
    var drainTimer: Timer?
    var expiry: Timer?

    func record(_ channel: String, isCandidate: Bool, metadata: [String: Any],
                repeated: Bool, modifiers: NSEvent.ModifierFlags,
                sourcePID: Int64, recipientPID: Int64) {
        totals[channel, default: 0] += 1
        guard isCandidate else { return }
        candidates[channel, default: 0] += 1
        guard emitted[channel, default: 0] < 16 else { return }
        emitted[channel, default: 0] += 1
        let front = NSWorkspace.shared.frontmostApplication?.processIdentifier
        var row = metadata
        row["kind"] = "candidate"
        row["channel"] = channel
        row["observedAtNs"] = DispatchTime.now().uptimeNanoseconds
        row["sourcePID"] = sourcePID
        row["recipientPID"] = recipientPID
        row["frontPID"] = front.map { Int64($0) } ?? 0
        row["modifiers"] = modifiers.rawValue
        row["repeated"] = repeated
        // Preserve the raw recipient and the old decision for comparison. A
        // global NSEvent copy may name this observer as its recipient even
        // when the paired session-tap event targets the foreground browser.
        let routingRecipient = channel == "nsevent_global" && recipientPID == Int64(getpid())
            ? 0 : recipientPID
        row["recipientNormalizedForRouting"] = routingRecipient != recipientPID
        row["legacyProductionFilter"] = productionRejection(repeated: repeated, modifiers: modifiers,
            source: sourcePID, recipient: recipientPID, front: front) ?? "accepted_before_target_check"
        row["productionFilter"] = productionRejection(repeated: repeated, modifiers: modifiers,
            source: sourcePID, recipient: routingRecipient, front: front) ?? "accepted_before_target_check"
        pending.append(row)
    }

    func drain() {
        let rows = pending
        pending.removeAll(keepingCapacity: true)
        for var row in rows {
            // This is later window-list evidence, not the event's focused window.
            // Only numeric IDs are retained. No titles, bounds or images.
            if let front = row["frontPID"] as? Int64, front > 0,
               let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID)
                    as? [[String: Any]] {
                row["windowListObservedAtNs"] = DispatchTime.now().uptimeNanoseconds
                row["frontAppCGWindowIDs"] = windows.filter {
                    ($0[kCGWindowOwnerPID as String] as? Int64) == front
                        && ($0[kCGWindowLayer as String] as? Int) == 0
                }.compactMap { $0[kCGWindowNumber as String] as? UInt32 }.prefix(16).map { $0 }
            }
            emit(row)
        }
    }

    func receive(_ event: NSEvent) {
        let cg = event.cgEvent
        record("nsevent_global", isCandidate: candidate(event.keyCode), metadata: nsMetadata(event),
            repeated: event.isARepeat, modifiers: event.modifierFlags,
            sourcePID: cg?.getIntegerValueField(.eventSourceUnixProcessID) ?? 0,
            recipientPID: cg?.getIntegerValueField(.eventTargetUnixProcessID) ?? 0)
    }

    func receive(_ event: CGEvent) {
        let code = event.getIntegerValueField(.keyboardEventKeycode)
        record("cg_session_tap", isCandidate: code >= 0 && code <= Int64(UInt16.max) && candidate(UInt16(code)),
            metadata: cgMetadata(event), repeated: event.getIntegerValueField(.keyboardEventAutorepeat) != 0,
            modifiers: NSEvent.ModifierFlags(rawValue: UInt(event.flags.rawValue)),
            sourcePID: event.getIntegerValueField(.eventSourceUnixProcessID),
            recipientPID: event.getIntegerValueField(.eventTargetUnixProcessID))
    }

    func finish() {
        if let monitor { NSEvent.removeMonitor(monitor) }
        if let tap { CGEvent.tapEnable(tap: tap, enable: false); CFMachPortInvalidate(tap) }
        if let source { CFRunLoopRemoveSource(CFRunLoopGetMain(), source, .commonModes) }
        heartbeat?.invalidate()
        drainTimer?.invalidate()
        drain()
        emit(["kind": "summary", "allKeyDownCallbacks": totals, "characterCandidateCallbacks": candidates,
              "recordsEmitted": emitted, "mainRunLoopHeartbeats": heartbeatCount, "environment": environment()])
    }
}

func tapCallback(_ proxy: CGEventTapProxy, _ type: CGEventType, _ event: CGEvent,
                 _ userInfo: UnsafeMutableRawPointer?) -> Unmanaged<CGEvent>? {
    if let userInfo {
        let probe = Unmanaged<Probe>.fromOpaque(userInfo).takeUnretainedValue()
        if type == .keyDown { probe.receive(event) }
        else if type == .tapDisabledByTimeout || type == .tapDisabledByUserInput {
            // There is no automatic reenable, so this adds at most one record.
            probe.pending.append(["kind": "tap_disabled", "reason": type == .tapDisabledByTimeout ? "timeout" : "user_input"])
        }
    }
    // listenOnly observes the event; it never changes it or posts another one.
    return Unmanaged.passUnretained(event)
}

var mode = "preflight"
var compareTap = false
var duration: Double = 20
var arguments = Array(CommandLine.arguments.dropFirst())
while !arguments.isEmpty {
    let argument = arguments.removeFirst()
    switch argument {
    case "--preflight", "--synthetic", "--monitor": mode = String(argument.dropFirst(2))
    case "--compare-tap": compareTap = true
    case "--duration":
        guard !arguments.isEmpty, let parsed = Double(arguments.removeFirst()), parsed.isFinite,
              parsed >= 1 && parsed <= 60 else {
            emit(["kind": "argument_error", "reason": "duration_must_be_between_1_and_60"]); exit(2)
        }
        duration = parsed
    default: emit(["kind": "argument_error", "reason": "unknown_argument"]); exit(2)
    }
}
if mode == "synthetic" { exit(synthetic()) }
emit(["kind": "preflight", "environment": environment(), "mode": mode,
      "limitation": "This process's permission is not the released app's permission."])
guard mode == "monitor" else { exit(0) }
guard AXIsProcessTrusted() else {
    emit(["kind": "not_started", "reason": "accessibility_not_trusted_no_permission_requested"]); exit(3)
}
let application = NSApplication.shared
application.setActivationPolicy(.prohibited)
let probe = Probe()
probe.monitor = NSEvent.addGlobalMonitorForEvents(matching: .keyDown) { probe.receive($0) }
if compareTap {
    if CGPreflightListenEventAccess() {
        probe.tap = CGEvent.tapCreate(tap: .cgSessionEventTap, place: .tailAppendEventTap,
            options: .listenOnly, eventsOfInterest: CGEventMask(1) << CGEventType.keyDown.rawValue,
            callback: tapCallback, userInfo: Unmanaged.passUnretained(probe).toOpaque())
        if let tap = probe.tap {
            probe.source = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, tap, 0)
            if let source = probe.source { CFRunLoopAddSource(CFRunLoopGetMain(), source, .commonModes) }
        }
    } else { emit(["kind": "tap_skipped", "reason": "listen_access_not_granted_no_permission_requested"]) }
}
emit(["kind": "installed", "globalMonitor": probe.monitor != nil,
      "sessionTap": probe.tap != nil && probe.source != nil, "durationSeconds": duration,
      "sessionTapEnabled": probe.tap.map { CGEvent.tapIsEnabled(tap: $0) } ?? false,
      "note": "Only type non-sensitive test letters in the intended window; no characters are read or recorded."])
probe.heartbeat = Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { _ in
    probe.heartbeatCount += 1
    emit(["kind": "heartbeat", "count": probe.heartbeatCount, "environment": environment()])
}
probe.drainTimer = Timer.scheduledTimer(withTimeInterval: 0.05, repeats: true) { _ in probe.drain() }
probe.expiry = Timer.scheduledTimer(withTimeInterval: duration, repeats: false) { _ in
    probe.finish(); exit(0)
}
application.run()
