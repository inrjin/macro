import AppKit
import ApplicationServices
import Carbon
import CoreMedia
import CoreVideo
import ScreenCaptureKit

private func errorString(_ message: String) -> UnsafeMutablePointer<CChar>? { strdup(message) }
private func nowNS() -> UInt64 { DispatchTime.now().uptimeNanoseconds }
private func windowList() -> [[String: Any]] {
    (CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements],
                              kCGNullWindowID) as? [[String: Any]]) ?? []
}
private func allWindowList() -> [[String: Any]] {
    // Explicit restoration also needs minimized/off-screen window identity.
    (CGWindowListCopyWindowInfo([.optionAll, .excludeDesktopElements],
                              kCGNullWindowID) as? [[String: Any]]) ?? []
}
private func bounds(_ value: [String: Any]) -> CGRect? {
    guard let dictionary = value[kCGWindowBounds as String] as? [String: Any] else { return nil }
    return CGRect(dictionaryRepresentation: dictionary as CFDictionary)
}
private func info(_ value: [String: Any]) -> [String: Any]? {
    guard let id = value[kCGWindowNumber as String] as? UInt32,
          let pid = value[kCGWindowOwnerPID as String] as? UInt32,
          let rect = bounds(value), rect.width > 0, rect.height > 0 else { return nil }
    return ["id": id, "pid": pid,
            "title": value[kCGWindowName as String] as? String
                ?? value[kCGWindowOwnerName as String] as? String ?? "",
            "rect": ["x": rect.minX, "y": rect.minY, "width": rect.width, "height": rect.height]]
}

// AX uses the same top-left global coordinate system as CGWindowList. These
// probes never activate apps, request permissions, change focus, or post input.
private let axTimeout: Float = 0.12
private struct AXWindowIdentity {
    let pid: pid_t
    let rect: CGRect
}
private enum AXProbeFailure: Error {
    case permission
    case unavailable
}
private typealias AXWindowResult = Result<AXWindowIdentity, AXProbeFailure>

private func sameRect(_ a: CGRect, _ b: CGRect) -> Bool {
    abs(a.minX - b.minX) < 0.5 && abs(a.minY - b.minY) < 0.5
        && abs(a.width - b.width) < 0.5 && abs(a.height - b.height) < 0.5
}
private func axAttribute(_ element: AXUIElement, _ attribute: CFString) -> CFTypeRef? {
    var value: CFTypeRef?
    guard AXUIElementCopyAttributeValue(element, attribute, &value) == .success else { return nil }
    return value
}
private func axElement(_ value: CFTypeRef?) -> AXUIElement? {
    guard let value, CFGetTypeID(value) == AXUIElementGetTypeID() else { return nil }
    return unsafeBitCast(value, to: AXUIElement.self)
}
private func axWindowIdentity(_ element: AXUIElement) -> AXWindowIdentity? {
    AXUIElementSetMessagingTimeout(element, axTimeout)
    // kAXWindowAttribute resolves a control or sheet to its containing window.
    // Window elements themselves need not expose that attribute.
    let role = axAttribute(element, kAXRoleAttribute as CFString) as? String
    guard let window = role == (kAXWindowRole as String) ? element
        : axElement(axAttribute(element, kAXWindowAttribute as CFString)) else { return nil }
    AXUIElementSetMessagingTimeout(window, axTimeout)
    var pid: pid_t = 0
    guard AXUIElementGetPid(window, &pid) == .success, pid > 0,
          let position = axAttribute(window, kAXPositionAttribute as CFString),
          let size = axAttribute(window, kAXSizeAttribute as CFString),
          CFGetTypeID(position) == AXValueGetTypeID(), CFGetTypeID(size) == AXValueGetTypeID() else { return nil }
    let positionValue = unsafeBitCast(position, to: AXValue.self)
    let sizeValue = unsafeBitCast(size, to: AXValue.self)
    guard AXValueGetType(positionValue) == .cgPoint, AXValueGetType(sizeValue) == .cgSize else { return nil }
    var origin = CGPoint.zero, dimensions = CGSize.zero
    guard AXValueGetValue(positionValue, .cgPoint, &origin), AXValueGetValue(sizeValue, .cgSize, &dimensions),
          origin.x.isFinite, origin.y.isFinite, dimensions.width.isFinite, dimensions.height.isFinite,
          dimensions.width > 0, dimensions.height > 0 else { return nil }
    return AXWindowIdentity(pid: pid, rect: CGRect(origin: origin, size: dimensions))
}
private func focusedAXWindow(_ pid: UInt32) -> AXWindowResult {
    guard AXIsProcessTrusted() else { return .failure(.permission) }
    let app = AXUIElementCreateApplication(pid_t(pid))
    AXUIElementSetMessagingTimeout(app, axTimeout)
    guard let focused = axElement(axAttribute(app, kAXFocusedWindowAttribute as CFString)),
          let identity = axWindowIdentity(focused), identity.pid == pid_t(pid) else { return .failure(.unavailable) }
    return .success(identity)
}
private func hitAXWindow(_ point: CGPoint) -> AXWindowResult {
    guard AXIsProcessTrusted() else { return .failure(.permission) }
    let system = AXUIElementCreateSystemWide()
    AXUIElementSetMessagingTimeout(system, axTimeout)
    var element: AXUIElement?
    guard AXUIElementCopyElementAtPosition(system, Float(point.x), Float(point.y), &element) == .success,
          let element else { return .failure(.unavailable) }
    // Preserve the hit owner even when it is a Dock item or menu without a
    // window: a known foreign owner is an occluder, never an unknown success.
    var owner: pid_t = 0
    guard AXUIElementGetPid(element, &owner) == .success else { return .failure(.unavailable) }
    if let identity = axWindowIdentity(element), identity.pid == owner { return .success(identity) }
    return .success(AXWindowIdentity(pid: owner, rect: .null))
}
private func probeFailure(_ result: AXWindowResult, focused: Bool) -> String? {
    guard case let .failure(failure) = result else { return nil }
    switch failure {
    case .permission:
        return "accessibility_permission|창 가림 확인에 손쉬운 사용 권한이 필요합니다. ‘시스템 설정 → 개인정보 보호 및 보안 → 손쉬운 사용’에서 Integrated Macro를 허용한 뒤 앱을 다시 실행하세요."
    case .unavailable:
        return focused
            ? "target_focus_unavailable|연결할 브라우저의 활성 창을 확인하지 못했습니다. 예매창을 클릭한 뒤 다시 시도하세요. 계속되면 앱의 손쉬운 사용 권한을 확인하세요."
            : "target_hit_test_unavailable|지정 위치의 실제 창을 확인하지 못했습니다. Dock·메뉴를 닫고 예매창 위에서 다시 F8을 누르세요. 계속되면 앱의 손쉬운 사용 권한을 확인하세요."
    }
}
private func uniqueWindowID(_ identity: AXWindowIdentity, windows: [[String: Any]]) -> UInt64? {
    let matches = windows.filter {
        ($0[kCGWindowOwnerPID as String] as? Int32) == identity.pid
            && ($0[kCGWindowLayer as String] as? Int) == 0
            && bounds($0).map { sameRect($0, identity.rect) } == true
            && ($0[kCGWindowAlpha as String] as? Double ?? 1) > 0.01
    }
    guard matches.count == 1 else { return nil }
    return matches[0][kCGWindowNumber as String] as? UInt64
}
private func activeDisplayBounds() -> [CGRect] {
    var count: UInt32 = 0
    guard CGGetActiveDisplayList(0, nil, &count) == .success, count > 0 else { return [] }
    var displays = [CGDirectDisplayID](repeating: 0, count: Int(count))
    guard CGGetActiveDisplayList(count, &displays, &count) == .success else { return [] }
    return displays.prefix(Int(count)).map(CGDisplayBounds)
}
private struct WindowProbes {
    var focused: (UInt32) -> AXWindowResult = focusedAXWindow
    var hit: (CGPoint) -> AXWindowResult = hitAXWindow
    var bundle: (Int32) -> String? = { NSRunningApplication(processIdentifier: $0)?.bundleIdentifier }
    var displays: () -> [CGRect] = activeDisplayBounds
}
private func isDockDisplaySurface(_ window: [String: Any], probes: WindowProbes) -> Bool {
    guard (window[kCGWindowLayer as String] as? Int) == Int(CGWindowLevelForKey(.dockWindow)),
          let pid = window[kCGWindowOwnerPID as String] as? Int32,
          probes.bundle(pid) == "com.apple.dock", let rect = bounds(window) else { return false }
    // Only the evidenced full-display compositor surface is ambiguous. A
    // regular Dock, menu, launcher or other high-level window stays an occluder.
    return probes.displays().contains { sameRect($0, rect) }
}

// Observation-only focus cache: capture polls run much faster than the window
// manager changes. Input, binding and explicit validation never use this cache.
// Window-order/geometry changes invalidate it immediately; an unchanged key is
// reusable for at most 75 ms. No hit-test/occlusion decision is cached.
private struct FocusWindowKey: Equatable {
    let id: UInt64
    let rect: CGRect?
    let alpha: Double
}
private struct FocusCacheKey: Equatable {
    let pid: UInt32
    let foregroundPID: Int32?
    let targetID: UInt64
    let windows: [FocusWindowKey]

    init(pid: UInt32, foregroundPID: Int32?, targetID: UInt64, windows: [[String: Any]]) {
        self.pid = pid
        self.foregroundPID = foregroundPID
        self.targetID = targetID
        self.windows = windows.filter {
            ($0[kCGWindowOwnerPID as String] as? UInt32) == pid
                && ($0[kCGWindowLayer as String] as? Int) == 0
        }.map {
            FocusWindowKey(id: $0[kCGWindowNumber as String] as? UInt64 ?? 0,
                rect: bounds($0), alpha: $0[kCGWindowAlpha as String] as? Double ?? 1)
        }
    }
}
private final class FrameFocusCache {
    private let lock = NSLock()
    private var entry: (FocusCacheKey, UInt64, AXWindowIdentity)?
    func clear() { lock.lock(); entry = nil; lock.unlock() }
    func resolve(key: FocusCacheKey, at now: UInt64, probe: () -> AXWindowResult) -> AXWindowResult {
        lock.lock()
        let previous = entry
        lock.unlock()
        if let (oldKey, at, identity) = previous, oldKey == key, now >= at, now - at < 75_000_000 {
            return .success(identity)
        }
        let result = probe()
        lock.lock()
        if case let .success(identity) = result { entry = (key, now, identity) }
        else { entry = nil }
        lock.unlock()
        return result
    }
}
private let frameFocusCache = FrameFocusCache()

// Activation is an explicit UI command, never a side effect of validation.
// Public AX has no CGWindowID attribute: require unique PID + geometry matches
// in both registries rather than guessing by title, order or application alone.
private enum ActivationSelection {
    case window(Int)
    case failure(String)
}
private func activationIdentityFailure(id: UInt64, pid: UInt32, expected: CGRect,
                                       windows: [[String: Any]]) -> String? {
    guard let target = windows.first(where: { ($0[kCGWindowNumber as String] as? UInt64) == id }) else {
        return "target_closed|연결한 예매창이 닫혔습니다. 실제 예매창에서 F7로 다시 연결하세요."
    }
    guard (target[kCGWindowOwnerPID as String] as? UInt32) == pid else {
        return "target_identity_changed|연결한 창의 프로세스가 바뀌었습니다. 실제 예매창에서 F7로 다시 연결하세요."
    }
    guard let rect = bounds(target), sameRect(rect, expected) else {
        return "target_geometry_changed|브라우저 창 위치·크기·배율이 바뀌었습니다. 실제 예매창에서 F7로 다시 연결한 뒤 좌표를 지정하세요."
    }
    return nil
}
private func selectActivationWindow(id: UInt64, pid: UInt32, expected: CGRect,
                                    windows: [[String: Any]], candidates: [AXWindowIdentity?]) -> ActivationSelection {
    if let error = activationIdentityFailure(id: id, pid: pid, expected: expected, windows: windows) {
        return .failure(error)
    }
    let identity = AXWindowIdentity(pid: pid_t(pid), rect: expected)
    guard uniqueWindowID(identity, windows: windows) == id else {
        return .failure("target_activation_ambiguous|같은 위치·크기의 브라우저 창이 여러 개라 연결한 예매창을 구분하지 못했습니다. 예매창을 직접 클릭한 뒤 F1으로 시작하세요.")
    }
    let matching = candidates.indices.filter {
        candidates[$0].map { $0.pid == pid_t(pid) && sameRect($0.rect, expected) } == true
    }
    guard matching.count == 1 else {
        return .failure("target_activation_ambiguous|연결한 예매창을 손쉬운 사용 창 목록에서 정확히 구분하지 못했습니다. 예매창을 직접 클릭한 뒤 F1으로 시작하세요.")
    }
    return .window(matching[0])
}
private func waitForActivation(deadline: UInt64, now: () -> UInt64 = nowNS,
                               pause: () -> Void = { Thread.sleep(forTimeInterval: 0.01) },
                               validate: () -> String?) -> String? {
    while true {
        guard let failure = validate() else { return nil }
        let code = String(failure.split(separator: "|", maxSplits: 1)[0])
        // Position changes and lost identities never become acceptable merely
        // because the OS eventually reports the application as active.
        guard ["target_not_foreground", "target_window_changed", "target_focus_unavailable",
               "target_not_visible"].contains(code) else { return failure }
        guard now() < deadline else {
            return "target_activation_failed|연결한 예매창을 전면으로 가져오지 못했습니다. 예매창을 직접 클릭한 뒤 F1으로 시작하세요. 확인 결과: [\(code)]"
        }
        pause()
    }
}

@_cdecl("mmp_activate")
public func mmpActivate(_ id: UInt64, _ pid: UInt32, _ x: Double, _ y: Double,
                        _ width: Double, _ height: Double) -> UnsafeMutablePointer<CChar>? {
    frameFocusCache.clear()
    let expected = CGRect(x: x, y: y, width: width, height: height)
    guard id > 0, id <= UInt64(UInt32.max), pid > 0, pid <= UInt32(Int32.max),
          [x, y, width, height].allSatisfy(\.isFinite), width > 0, height > 0 else {
        return errorString("invalid_target|연결한 예매창 정보가 올바르지 않습니다. 실제 예매창에서 F7로 다시 연결하세요.")
    }
    let windows = allWindowList()
    if let error = activationIdentityFailure(id: id, pid: pid, expected: expected, windows: windows) {
        return errorString(error)
    }
    guard AXIsProcessTrusted() else {
        return errorString("accessibility_permission|예매창 전환에 손쉬운 사용 권한이 필요합니다. ‘시스템 설정 → 개인정보 보호 및 보안 → 손쉬운 사용’에서 Integrated Macro를 허용한 뒤 앱을 다시 실행하세요.")
    }
    let deadline = nowNS() + 1_500_000_000
    let application = AXUIElementCreateApplication(pid_t(pid))
    AXUIElementSetMessagingTimeout(application, axTimeout)
    guard let raw = axAttribute(application, kAXWindowsAttribute as CFString),
          CFGetTypeID(raw) == CFArrayGetTypeID(), let elements = raw as? [AXUIElement],
          elements.count <= 128 else {
        return errorString("target_focus_unavailable|연결한 예매창의 창 목록을 읽지 못했습니다. 예매창을 직접 클릭한 뒤 F1으로 시작하세요.")
    }
    var identities: [AXWindowIdentity?] = []
    for element in elements {
        guard nowNS() < deadline else {
            return errorString("target_activation_failed|브라우저의 창 확인 응답이 지연되었습니다. 예매창을 직접 클릭한 뒤 F1으로 시작하세요.")
        }
        identities.append(axWindowIdentity(element))
    }
    let selection = selectActivationWindow(id: id, pid: pid, expected: expected,
        windows: windows, candidates: identities)
    let target: AXUIElement
    switch selection {
    case let .window(index): target = elements[index]
    case let .failure(error): return errorString(error)
    }
    // Recheck the saved identity immediately before requesting any window change.
    if let error = activationIdentityFailure(id: id, pid: pid, expected: expected, windows: allWindowList()) {
        return errorString(error)
    }
    guard let running = NSRunningApplication(processIdentifier: pid_t(pid)), !running.isTerminated else {
        return errorString("target_closed|연결한 브라우저가 종료되었습니다. 실제 예매창에서 F7로 다시 연결하세요.")
    }
    if (axAttribute(target, kAXMinimizedAttribute as CFString) as? Bool) == true {
        guard AXUIElementSetAttributeValue(target, kAXMinimizedAttribute as CFString, kCFBooleanFalse) == .success else {
            return errorString("target_activation_failed|연결한 예매창의 최소화를 해제하지 못했습니다. 예매창을 직접 열고 F1으로 시작하세요.")
        }
    }
    // These are requests, not proof of activation. Some applications expose
    // only a subset as settable; the fresh AX + CG checks below decide success.
    AXUIElementSetAttributeValue(target, kAXMainAttribute as CFString, kCFBooleanTrue)
    AXUIElementSetAttributeValue(target, kAXFocusedAttribute as CFString, kCFBooleanTrue)
    AXUIElementSetAttributeValue(application, kAXFocusedWindowAttribute as CFString, target)
    AXUIElementPerformAction(target, kAXRaiseAction as CFString)
    DispatchQueue.main.async {
        guard nowNS() < deadline else { return }
        if #available(macOS 14.0, *) {
            NSApplication.shared.yieldActivation(to: running)
            _ = running.activate(from: .current, options: [])
        } else {
            _ = running.activate(options: [])
        }
    }
    return waitForActivation(deadline: deadline) {
        if let failure = activationIdentityFailure(id: id, pid: pid, expected: expected, windows: allWindowList()) {
            return failure
        }
        let visible = windowList()
        guard NSWorkspace.shared.frontmostApplication?.processIdentifier == pid_t(pid) else {
            return "target_not_foreground|연결한 브라우저가 아직 전면이 아닙니다."
        }
        // Always probe focused AX here, including the ordinary CG-first case.
        let focused = focusedAXWindow(pid)
        if let failure = probeFailure(focused, focused: true) { return failure }
        guard case let .success(identity) = focused,
              let focusedID = uniqueWindowID(identity, windows: visible) else {
            return "target_focus_unavailable|연결한 예매창의 활성 상태를 확인하지 못했습니다."
        }
        guard focusedID == id else { return "target_window_changed|같은 브라우저의 다른 창이 전면에 있습니다." }
        guard visible.contains(where: { ($0[kCGWindowNumber as String] as? UInt64) == id }) else {
            return "target_not_visible|연결한 예매창이 아직 화면에 표시되지 않았습니다."
        }
        return validateWindow(id: id, pid: pid, expected: expected, point: nil, windows: visible,
            ignored: [], ownPID: getpid(), foregroundPID: pid_t(pid), requireForeground: true)
    }.flatMap(errorString)
}

@_cdecl("mmp_free_string")
public func mmpFreeString(_ pointer: UnsafeMutablePointer<CChar>?) { free(pointer) }

@_cdecl("mmp_permissions")
public func mmpPermissions() -> UInt32 {
    (CGPreflightScreenCaptureAccess() ? 1 : 0) | (AXIsProcessTrusted() ? 2 : 0)
}

// All three calls only read state. They do not request permission, install a
// monitor, change secure input, or establish that key callbacks are delivered.
private func keyboardInputStatus(
    accessibility: () -> Bool = AXIsProcessTrusted,
    listen: () -> Bool = CGPreflightListenEventAccess,
    secure: () -> Bool = IsSecureEventInputEnabled
) -> UInt32 {
    let accessibilityTrusted = accessibility()
    let listenEventAccess = listen()
    let secureEventInput = secure()
    return (accessibilityTrusted ? 1 : 0) | (listenEventAccess ? 2 : 0) | (secureEventInput ? 4 : 0)
}

@_cdecl("mmp_keyboard_input_status")
public func mmpKeyboardInputStatus() -> UInt32 { keyboardInputStatus() }

@_cdecl("mmp_windows_json")
public func mmpWindowsJSON(_ onlyForeground: Int32) -> UnsafeMutablePointer<CChar>? {
    let windows = windowList()
    let list: [[String: Any]]
    if onlyForeground == 0 {
        list = windows.filter { ($0[kCGWindowLayer as String] as? Int) == 0 }.compactMap(info)
    } else {
        frameFocusCache.clear()
        guard let pid = NSWorkspace.shared.frontmostApplication?.processIdentifier else { return strdup("[]") }
        let result = focusedAXWindow(UInt32(pid))
        if let failure = probeFailure(result, focused: true) { return errorString(failure) }
        guard case let .success(identity) = result, let id = uniqueWindowID(identity, windows: windows),
              let window = windows.first(where: { ($0[kCGWindowNumber as String] as? UInt64) == id }),
              let value = info(window) else {
            return errorString("target_focus_unavailable|활성 창과 화면의 창을 정확히 연결하지 못했습니다. 예매창을 전면에 두고 다시 연결하세요.")
        }
        list = [value]
    }
    let value: Any = list
    guard let data = try? JSONSerialization.data(withJSONObject: value),
          let string = String(data: data, encoding: .utf8) else { return nil }
    return strdup(string)
}

@_cdecl("mmp_pointer")
public func mmpPointer(_ x: UnsafeMutablePointer<Double>?, _ y: UnsafeMutablePointer<Double>?) -> Int32 {
    guard let event = CGEvent(source: nil), let x, let y else { return 0 }
    x.pointee = event.location.x
    y.pointee = event.location.y
    return 1
}

@_cdecl("mmp_owned_window_id")
public func mmpOwnedWindowID(_ pointer: UnsafeMutableRawPointer?) -> UInt64 {
    guard Thread.isMainThread, let pointer else { return 0 }
    let window = Unmanaged<NSWindow>.fromOpaque(pointer).takeUnretainedValue()
    return UInt64(max(0, window.windowNumber))
}

@_cdecl("mmp_overlay_owned")
public func mmpOverlayOwned(_ id: UInt64) -> Int32 {
    let entries = CGWindowListCopyWindowInfo(.optionIncludingWindow, CGWindowID(id)) as? [[String: Any]]
    return entries?.contains {
        ($0[kCGWindowNumber as String] as? UInt64) == id
            && ($0[kCGWindowOwnerPID as String] as? Int32) == getpid()
    } == true ? 1 : 0
}

// Reading a visible point does not require keyboard focus. Posting input does.
// Both paths still require the same bound identity, geometry and point visibility.
private func validateWindow(
    id: UInt64, pid: UInt32, expected: CGRect, point: CGPoint?,
    windows: [[String: Any]], ignored: [UInt64], ownPID: Int32,
    foregroundPID: Int32?, requireForeground: Bool, probes: WindowProbes = WindowProbes()
) -> String? {
    if requireForeground && foregroundPID != pid_t(pid) {
        return "target_not_foreground|연결한 브라우저가 전면이 아닙니다. 연결한 예매창을 전면에 둔 뒤 다시 시작하세요."
    }
    guard let target = windows.first(where: {
        ($0[kCGWindowNumber as String] as? UInt64) == id
            && ($0[kCGWindowOwnerPID as String] as? UInt32) == pid
    }), let rect = bounds(target) else {
        return "target_closed|연결한 창이 닫혔거나 화면에서 사라졌습니다. 실제 예매창을 전면에 두고 F7로 ‘브라우저 연결’을 다시 실행하세요."
    }
    guard abs(rect.minX - expected.minX) < 0.5, abs(rect.minY - expected.minY) < 0.5,
          abs(rect.width - expected.width) < 0.5, abs(rect.height - expected.height) < 0.5 else {
        return "target_geometry_changed|브라우저 창 위치·크기·배율이 바뀌었습니다. 실제 예매창을 전면에 두고 F7로 다시 연결한 뒤 좌표를 지정하세요."
    }
    if requireForeground {
        let firstNormal = windows.first {
            ($0[kCGWindowLayer as String] as? Int) == 0
                && ($0[kCGWindowOwnerPID as String] as? UInt32) == pid
        }
        if (firstNormal?[kCGWindowNumber as String] as? UInt64) != id {
            // A browser may publish a tiny normal-layer helper before its
            // focused window. Only positive AX focus evidence resolves that.
            let result = probes.focused(pid)
            if let failure = probeFailure(result, focused: true) { return failure }
            guard case let .success(identity) = result,
                  let focusedID = uniqueWindowID(identity, windows: windows) else {
                return "target_focus_unavailable|활성 창과 화면의 창을 정확히 연결하지 못했습니다. 예매창을 전면에 두고 다시 연결하세요."
            }
            guard focusedID == id else {
                return "target_window_changed|같은 브라우저의 다른 창이 전면에 있습니다. 연결한 예매창으로 돌아오세요. 새 예매창이면 그 창에서 F7로 다시 연결하세요."
            }
        }
    }
    if let point {
        guard rect.contains(point) else {
            return "point_outside_target|마우스 위치가 연결한 창 밖입니다. 연결한 창 안에 놓으세요. 새 예매창이면 그 창에서 F7로 다시 연결하세요."
        }
        for window in windows {
            guard let r = bounds(window), r.contains(point),
                  (window[kCGWindowAlpha as String] as? Double ?? 1) > 0.01,
                  let current = window[kCGWindowNumber as String] as? UInt64 else { continue }
            // Never ignore an unrelated window, including another window of
            // the same browser, even if its ID appears in the supplied list.
            if ignored.contains(current), (window[kCGWindowOwnerPID as String] as? Int32) == ownPID { continue }
            if current == id { return nil }
            if (window[kCGWindowOwnerPID as String] as? UInt32) == pid {
                return "target_window_changed|마우스 아래가 연결한 창이 아닌 같은 브라우저의 다른 창입니다. 실제 예매창을 전면에 두고 F7로 다시 연결하세요."
            }
            if isDockDisplaySurface(window, probes: probes) {
                let hit = probes.hit(point)
                if let failure = probeFailure(hit, focused: false) { return failure }
                guard case let .success(identity) = hit else { return "target_hit_test_unavailable|지정 위치의 실제 창을 확인하지 못했습니다." }
                guard identity.pid == pid_t(pid) else {
                    return "target_occluded|지정 위치에서 Dock 또는 다른 앱이 확인되었습니다. 해당 창·메뉴를 닫고 예매창 위에서 다시 시도하세요."
                }
                guard let hitID = uniqueWindowID(identity, windows: windows) else {
                    return "target_hit_test_unavailable|지정 위치의 브라우저 창을 정확히 식별하지 못했습니다. 예매창을 전면에 두고 다시 시도하세요."
                }
                guard hitID == id else {
                    return "target_window_changed|마우스 아래가 연결한 창이 아닌 같은 브라우저의 다른 창입니다. 실제 예매창을 전면에 두고 F7로 다시 연결하세요."
                }
                continue
            }
            let owner = window[kCGWindowOwnerName as String] as? String ?? "다른 앱"
            let layer = window[kCGWindowLayer as String] as? Int ?? 0
            return "target_occluded|지정 위치가 \(owner) 창에 가려져 있습니다 (창 \(current), 레이어 \(layer)). 해당 창을 옮긴 뒤 다시 시작하세요."
        }
        return "target_not_visible|지정 위치에서 연결한 창을 확인할 수 없습니다."
    }
    return nil
}

@_cdecl("mmp_validate")
public func mmpValidate(
    _ id: UInt64, _ pid: UInt32, _ x: Double, _ y: Double, _ width: Double, _ height: Double,
    _ hasPoint: Int32, _ px: Double, _ py: Double,
    _ overlays: UnsafePointer<UInt64>?, _ overlayCount: Int32
) -> UnsafeMutablePointer<CChar>? {
    // Never let observation-cache evidence authorize a click, key or bind.
    frameFocusCache.clear()
    let ignored = overlays.map { Array(UnsafeBufferPointer(start: $0, count: max(0, Int(overlayCount)))) } ?? []
    return validateWindow(id: id, pid: pid, expected: CGRect(x: x, y: y, width: width, height: height),
        point: hasPoint != 0 ? CGPoint(x: px, y: py) : nil, windows: windowList(),
        ignored: ignored, ownPID: getpid(),
        foregroundPID: NSWorkspace.shared.frontmostApplication?.processIdentifier,
        requireForeground: true).flatMap(errorString)
}

@_cdecl("mmp_validate_frame")
public func mmpValidateFrame(
    _ id: UInt64, _ pid: UInt32, _ x: Double, _ y: Double, _ width: Double, _ height: Double,
    _ hasPoint: Int32, _ px: Double, _ py: Double,
    _ overlays: UnsafePointer<UInt64>?, _ overlayCount: Int32
) -> UnsafeMutablePointer<CChar>? {
    // Point-bearing calls must always use the fresh input path, even if a
    // caller accidentally supplies one to this observation-only entry point.
    if hasPoint != 0 { return mmpValidate(id, pid, x, y, width, height, hasPoint, px, py, overlays, overlayCount) }
    let ignored = overlays.map { Array(UnsafeBufferPointer(start: $0, count: max(0, Int(overlayCount)))) } ?? []
    let windows = windowList()
    let foreground = NSWorkspace.shared.frontmostApplication?.processIdentifier
    let key = FocusCacheKey(pid: pid, foregroundPID: foreground, targetID: id, windows: windows)
    var probes = WindowProbes()
    probes.focused = { pid in frameFocusCache.resolve(key: key, at: nowNS()) { focusedAXWindow(pid) } }
    return validateWindow(id: id, pid: pid, expected: CGRect(x: x, y: y, width: width, height: height),
        point: nil, windows: windows, ignored: ignored, ownPID: getpid(), foregroundPID: foreground,
        requireForeground: true, probes: probes).flatMap(errorString)
}

@_cdecl("mmp_validate_capture")
public func mmpValidateCapture(
    _ id: UInt64, _ pid: UInt32, _ x: Double, _ y: Double, _ width: Double, _ height: Double,
    _ px: Double, _ py: Double, _ overlays: UnsafePointer<UInt64>?, _ overlayCount: Int32
) -> UnsafeMutablePointer<CChar>? {
    let ignored = overlays.map { Array(UnsafeBufferPointer(start: $0, count: max(0, Int(overlayCount)))) } ?? []
    return validateWindow(id: id, pid: pid, expected: CGRect(x: x, y: y, width: width, height: height),
        point: CGPoint(x: px, y: py), windows: windowList(), ignored: ignored, ownPID: getpid(),
        foregroundPID: nil, requireForeground: false).flatMap(errorString)
}

public typealias NativeInputCancellation = @convention(c) (UnsafeRawPointer?) -> Int32
private let cancelledInputError = "input_cancelled|중지 또는 실행 변경으로 아직 전달하지 않은 입력을 취소했습니다."

// Guards are synchronous and are never retained or dispatched. Once down has
// been posted its matching up must always follow, even if Stop arrived meanwhile.
private func postGuardedClick(cancelled: () -> Bool, move: () -> Void,
                              down: () -> Void, up: () -> Void, clock: () -> UInt64 = nowNS,
                              recordStart: (UInt64) -> Void = { _ in }) -> String? {
    if cancelled() { return cancelledInputError }
    move()
    if cancelled() { return cancelledInputError }
    let startedAt = clock()
    guard startedAt > 0 else { return "input_clock_unavailable|입력 시각을 확인하지 못해 전송을 보류했습니다." }
    recordStart(startedAt)
    down()
    up()
    return nil
}

private func postGuardedKey(cancelled: () -> Bool, down: () -> Void, up: () -> Void,
                            clock: () -> UInt64 = nowNS, recordStart: (UInt64) -> Void = { _ in }) -> String? {
    if cancelled() { return cancelledInputError }
    let startedAt = clock()
    guard startedAt > 0 else { return "input_clock_unavailable|입력 시각을 확인하지 못해 전송을 보류했습니다." }
    recordStart(startedAt)
    down()
    up()
    return nil
}

@_cdecl("mmp_click")
public func mmpClick(_ x: Double, _ y: Double) -> UnsafeMutablePointer<CChar>? {
    var startedAt: UInt64 = 0
    return mmpClickGuarded(x, y, nil, nil, &startedAt)
}

@_cdecl("mmp_click_guarded")
public func mmpClickGuarded(_ x: Double, _ y: Double,
                           _ cancelled: NativeInputCancellation?, _ context: UnsafeRawPointer?,
                           _ startedAt: UnsafeMutablePointer<UInt64>?) -> UnsafeMutablePointer<CChar>? {
    guard let startedAt else { return errorString("input_receipt_invalid|입력 시각 응답 버퍼가 없습니다.") }
    startedAt.pointee = 0
    if (cancelled?(context) ?? 0) != 0 { return errorString(cancelledInputError) }
    guard AXIsProcessTrusted() else {
        return errorString("input_permission|‘시스템 설정 → 개인정보 보호 및 보안 → 손쉬운 사용’에서 이 앱을 허용한 뒤 다시 실행하세요.")
    }
    let modifiers = CGEventSource.flagsState(.combinedSessionState)
    if !modifiers.intersection([.maskShift, .maskControl, .maskAlternate, .maskCommand]).isEmpty {
        return errorString("modifier_pressed|Shift·Control·Option·Command 키를 놓은 뒤 다시 시작하세요.")
    }
    let point = CGPoint(x: x, y: y)
    guard let source = CGEventSource(stateID: .hidSystemState),
          let move = CGEvent(mouseEventSource: source, mouseType: .mouseMoved, mouseCursorPosition: point, mouseButton: .left),
          let down = CGEvent(mouseEventSource: source, mouseType: .leftMouseDown, mouseCursorPosition: point, mouseButton: .left),
          let up = CGEvent(mouseEventSource: source, mouseType: .leftMouseUp, mouseCursorPosition: point, mouseButton: .left) else {
        return errorString("input_event_creation|마우스 입력 이벤트를 만들지 못했습니다. 앱을 다시 실행하세요.")
    }
    // CGEventPost has no delivery acknowledgement. Browser result observation
    // is deliberately left to the shared engine.
    return postGuardedClick(cancelled: { (cancelled?(context) ?? 0) != 0 },
        move: { move.post(tap: .cghidEventTap) }, down: { down.post(tap: .cghidEventTap) },
        up: { up.post(tap: .cghidEventTap) }, recordStart: { startedAt.pointee = $0 }).flatMap(errorString)
}

@_cdecl("mmp_key")
public func mmpKey(_ key: UInt16) -> UnsafeMutablePointer<CChar>? {
    var startedAt: UInt64 = 0
    return mmpKeyGuarded(key, nil, nil, &startedAt)
}

@_cdecl("mmp_key_guarded")
public func mmpKeyGuarded(_ key: UInt16, _ cancelled: NativeInputCancellation?,
                         _ context: UnsafeRawPointer?, _ startedAt: UnsafeMutablePointer<UInt64>?) -> UnsafeMutablePointer<CChar>? {
    guard let startedAt else { return errorString("input_receipt_invalid|입력 시각 응답 버퍼가 없습니다.") }
    startedAt.pointee = 0
    if (cancelled?(context) ?? 0) != 0 { return errorString(cancelledInputError) }
    guard AXIsProcessTrusted() else {
        return errorString("input_permission|‘시스템 설정 → 개인정보 보호 및 보안 → 손쉬운 사용’에서 이 앱을 허용해 주세요.")
    }
    let modifiers = CGEventSource.flagsState(.combinedSessionState)
    if !modifiers.intersection([.maskShift, .maskControl, .maskAlternate, .maskCommand]).isEmpty {
        return errorString("modifier_pressed|Shift·Control·Option·Command 키를 놓은 뒤 다시 시작하세요.")
    }
    guard [UInt16(53), 126, 125, 36].contains(key),
          let source = CGEventSource(stateID: .hidSystemState),
          let down = CGEvent(keyboardEventSource: source, virtualKey: key, keyDown: true),
          let up = CGEvent(keyboardEventSource: source, virtualKey: key, keyDown: false) else {
        return errorString("input_event_creation|키보드 입력 이벤트를 만들지 못했습니다.")
    }
    return postGuardedKey(cancelled: { (cancelled?(context) ?? 0) != 0 },
        down: { down.post(tap: .cghidEventTap) }, up: { up.post(tap: .cghidEventTap) },
        recordStart: { startedAt.pointee = $0 }).flatMap(errorString)
}

private struct EnterNotification {
    let token: UInt64
    let windowID: UInt64
    let pid: UInt32
    let observedAt: UInt64
}

private func isUserEnter(keyCode: UInt16, repeated: Bool, modifiers: NSEvent.ModifierFlags,
                         sourcePID: Int64, ownPID: Int64) -> Bool {
    [UInt16(36), 76].contains(keyCode) && !repeated
        && modifiers.intersection([.shift, .control, .option, .command]).isEmpty
        && sourcePID != ownPID
}

private func isCaptchaCharacterKey(_ code: UInt16) -> Bool {
    switch code {
    // Physical ANSI letter/digit positions and keypad digits; no text is read.
    case 0...9, 11...23, 25, 26, 28, 29, 31, 32, 34, 35, 37, 38, 40, 45, 46,
         82...89, 91, 92: return true
    default: return false
    }
}

private func isUserCaptchaInput(keyCode: UInt16, repeated: Bool,
                                modifiers: NSEvent.ModifierFlags, sourcePID: Int64, ownPID: Int64) -> Bool {
    isCaptchaCharacterKey(keyCode) && captchaKeyRejection(repeated: repeated,
        modifiers: modifiers, sourcePID: sourcePID, ownPID: ownPID) == nil
}

private func captchaKeyRejection(repeated: Bool, modifiers: NSEvent.ModifierFlags,
                                 sourcePID: Int64, ownPID: Int64) -> String? {
    if repeated { return "repeated" }
    if sourcePID == ownPID { return "own_source" }
    if !modifiers.intersection([.control, .option, .command, .function]).isEmpty { return "modifier_pressed" }
    return nil
}

private func captchaInputError(_ error: String) -> String {
    error.replacingOccurrences(of: "enter_watch", with: "captcha_input_watch")
        .replacingOccurrences(of: "Enter", with: "캡챠 첫 입력")
}

private func freshEnterNotification(_ press: EnterNotification, token: UInt64?, now: UInt64) -> Bool {
    press.token == token && press.pid > 0 && press.observedAt > 0 && press.observedAt <= now
        && now - press.observedAt <= 500_000_000
}

// Reasons mirror the existing notification filters without weakening them.
// Character candidates reuse routing checks after their own modifier filter.
private func enterCallbackRejection(repeated: Bool, modifiers: NSEvent.ModifierFlags,
                                    sourcePID: Int64, ownPID: Int64,
                                    frontPID: pid_t?, recipientPID: Int64) -> String? {
    if repeated { return "repeated" }
    if !modifiers.intersection([.shift, .control, .option, .command]).isEmpty { return "modifier_pressed" }
    if sourcePID == ownPID { return "own_source" }
    guard let frontPID, frontPID > 0 else { return "front_app_unavailable" }
    if Int64(frontPID) == ownPID { return "own_front_app" }
    if recipientPID > 0 && recipientPID != Int64(frontPID) { return "recipient_mismatch" }
    return nil
}

// Only for NSEvent global-monitor copies. The paired physical-input probe
// showed CGSession targeting Chrome while AppKit's copy targeted this observer.
// That alias is not an actual browser recipient; preserve the raw value in
// diagnostics, but let the foreground and runtime bound-window checks decide.
// Raw CGEvent/tap routing and unrelated recipient PIDs stay strict.
private func globalMonitorRecipient(_ recipientPID: Int64, ownPID: Int64) -> Int64 {
    recipientPID == ownPID ? 0 : recipientPID
}

private func enterQueueRejection(_ press: EnterNotification, token: UInt64?, now: UInt64) -> String? {
    if press.token != token { return "stale_token" }
    if press.pid == 0 || press.observedAt == 0 { return "invalid_event" }
    if press.observedAt > now { return "future_event" }
    if now - press.observedAt > 500_000_000 { return "stale_event" }
    return nil
}

private struct EnterDiagnostics {
    private var entries: [[String: Any]] = []
    mutating func append(_ value: [String: Any]) {
        if entries.count == 16 { entries.removeFirst() }
        entries.append(value)
    }
    mutating func take() -> [[String: Any]] {
        let result = entries
        entries.removeAll(keepingCapacity: true)
        return result
    }
}

// Only OS context is injectable. Tests still pass an NSEvent through the same
// receive path and extract its CGEvent fields; they never install/post input.
#if MACRO_NATIVE_SELF_TEST
private struct KeyboardMonitorEnvironment {
    var ownPID: () -> Int64 = { Int64(getpid()) }
    var frontPID: () -> pid_t? = { NSWorkspace.shared.frontmostApplication?.processIdentifier }
    var clock: () -> UInt64 = nowNS
}
#endif

// A notification-only AppKit monitor: it cannot consume or replace the key.
// No characters, window titles, AX queries or screenshots enter this callback.
// A global NSEvent may omit its window number. Preserve that as 0 rather than
// guessing the first Chrome window; the worker must validate its exact binding.
private final class EnterMonitor {
    enum Kind { case enter, captchaInput }
    private let kind: Kind
    private let lock = NSLock()
    private var epoch: UInt64 = 0
    private var token: UInt64?
    private var notifications: [EnterNotification] = []
    private var diagnostics = EnterDiagnostics()
    // Accessed only on the AppKit main thread.
    private var monitor: Any?
    private var installed = false // protected by lock
    #if MACRO_NATIVE_SELF_TEST
    private var environmentForTest: KeyboardMonitorEnvironment?
    private var drainEnvironmentForTest: (clock: () -> UInt64, trusted: () -> Bool)?
    #endif

    init(kind: Kind = .enter) { self.kind = kind }

    #if MACRO_NATIVE_SELF_TEST
    convenience init(kind: Kind, environment: KeyboardMonitorEnvironment) {
        self.init(kind: kind)
        self.environmentForTest = environment
    }
    #endif

    private func ownPID() -> Int64 {
        #if MACRO_NATIVE_SELF_TEST
        if let environmentForTest { return environmentForTest.ownPID() }
        #endif
        return Int64(getpid())
    }

    private func frontPID() -> pid_t? {
        #if MACRO_NATIVE_SELF_TEST
        if let environmentForTest { return environmentForTest.frontPID() }
        #endif
        return NSWorkspace.shared.frontmostApplication?.processIdentifier
    }

    private func notificationTime() -> UInt64 {
        #if MACRO_NATIVE_SELF_TEST
        if let environmentForTest { return environmentForTest.clock() }
        #endif
        return nowNS()
    }

    private func receive(_ event: NSEvent, epoch expectedEpoch: UInt64) {
        if kind == .captchaInput {
            // Never retain a key code or read text. Non-character events do not
            // enter either queue, even when the first-input watch is active.
            guard isCaptchaCharacterKey(event.keyCode) else { return }
            let sourcePID = event.cgEvent?.getIntegerValueField(.eventSourceUnixProcessID) ?? 0
            let recipientPID = event.cgEvent?.getIntegerValueField(.eventTargetUnixProcessID) ?? 0
            let ownPID = self.ownPID()
            let frontPID = self.frontPID()
            let rejection = captchaKeyRejection(repeated: event.isARepeat,
                modifiers: event.modifierFlags, sourcePID: sourcePID, ownPID: ownPID)
                ?? enterCallbackRejection(repeated: false, modifiers: [], sourcePID: sourcePID,
                    ownPID: ownPID, frontPID: frontPID,
                    recipientPID: globalMonitorRecipient(recipientPID, ownPID: ownPID))
            let notificationTime = self.notificationTime()
            let windowID = event.windowNumber > 0 ? UInt64(event.windowNumber) : 0
            lock.lock()
            defer { lock.unlock() }
            let active = epoch == expectedEpoch && installed && token != nil
            var diagnostic: [String: Any] = [
                "stage": "callback", "reason": rejection ?? (active ? "enqueued" : "inactive_monitor"),
                "window_id": windowID, "source_pid": sourcePID, "recipient_pid": recipientPID,
                "repeated": event.isARepeat, "modifiers": event.modifierFlags.rawValue,
                "observed_at_ns": notificationTime
            ]
            if epoch == expectedEpoch, let token { diagnostic["token"] = token }
            if let frontPID { diagnostic["front_pid"] = frontPID }
            diagnostics.append(diagnostic)
            guard rejection == nil, active, let token, let frontPID else { return }
            if notifications.count == 16 { notifications.removeFirst() }
            notifications.append(EnterNotification(token: token, windowID: windowID,
                pid: UInt32(frontPID), observedAt: notificationTime))
            return
        }
        // No record is created for any other key, even while watching Enter.
        guard [UInt16(36), 76].contains(event.keyCode) else { return }
        let sourcePID = event.cgEvent?.getIntegerValueField(.eventSourceUnixProcessID) ?? 0
        let recipientPID = event.cgEvent?.getIntegerValueField(.eventTargetUnixProcessID) ?? 0
        let ownPID = self.ownPID()
        let frontPID = self.frontPID()
        let rejection = enterCallbackRejection(repeated: event.isARepeat,
            modifiers: event.modifierFlags, sourcePID: sourcePID, ownPID: ownPID,
            frontPID: frontPID, recipientPID: globalMonitorRecipient(recipientPID, ownPID: ownPID))
        let observedAt = notificationTime()
        let windowID = event.windowNumber > 0 ? UInt64(event.windowNumber) : 0
        // The critical section only appends to a fixed-size queue. Nothing in
        // it waits on the main thread, the capture stream, or another process.
        lock.lock()
        defer { lock.unlock() }
        let active = epoch == expectedEpoch && installed && token != nil
        var diagnostic: [String: Any] = [
            "stage": "callback", "reason": rejection ?? (active ? "enqueued" : "inactive_monitor"),
            "window_id": windowID, "source_pid": sourcePID, "recipient_pid": recipientPID,
            "repeated": event.isARepeat, "modifiers": event.modifierFlags.rawValue,
            "observed_at_ns": observedAt
        ]
        if let token { diagnostic["token"] = token }
        if let frontPID { diagnostic["front_pid"] = frontPID }
        diagnostics.append(diagnostic)
        guard rejection == nil, active, let token, let frontPID else { return }
        if notifications.count == 16 { notifications.removeFirst() }
        notifications.append(EnterNotification(token: token, windowID: windowID,
                                               pid: UInt32(frontPID), observedAt: observedAt))
    }

    private func removeOnMain() {
        if let monitor { NSEvent.removeMonitor(monitor) }
        monitor = nil
    }

    private final class Request {
        let done = DispatchSemaphore(value: 0)
        let lock = NSLock()
        var result: String?
        func finish(_ error: String?) {
            lock.lock(); result = error; lock.unlock()
            done.signal()
        }
        func value() -> String? {
            lock.lock(); defer { lock.unlock() }
            return result
        }
    }

    func set(_ nextToken: UInt64?) -> String? {
        if nextToken != nil && !AXIsProcessTrusted() {
            _ = set(nil)
            return "enter_watch_permission|Enter 감지에 손쉬운 사용 권한이 필요합니다. 시스템 설정에서 Integrated Macro를 허용하세요."
        }
        lock.lock()
        if token == nextToken && (nextToken == nil ? !installed : installed) {
            lock.unlock()
            return nil
        }
        epoch &+= 1
        let requestedEpoch = epoch
        token = nil
        installed = false
        notifications.removeAll(keepingCapacity: true)
        lock.unlock()

        let request = Request()
        let change = { [self] in
            lock.lock()
            let current = epoch == requestedEpoch
            lock.unlock()
            guard current else {
                request.finish("enter_watch_cancelled|Enter 감지 설정이 취소되었습니다.")
                return
            }
            removeOnMain()
            guard let nextToken else { request.finish(nil); return }
            guard AXIsProcessTrusted() else {
                request.finish("enter_watch_permission|Enter 감지에 손쉬운 사용 권한이 필요합니다.")
                return
            }
            guard let newMonitor = NSEvent.addGlobalMonitorForEvents(matching: .keyDown, handler: { [weak self] event in
                self?.receive(event, epoch: requestedEpoch)
            }) else {
                request.finish("enter_watch_install_failed|브라우저 Enter 감지를 설치하지 못했습니다.")
                return
            }
            lock.lock()
            guard epoch == requestedEpoch else {
                lock.unlock()
                NSEvent.removeMonitor(newMonitor)
                request.finish("enter_watch_cancelled|Enter 감지 설정이 취소되었습니다.")
                return
            }
            monitor = newMonitor
            installed = true
            token = nextToken
            lock.unlock()
            request.finish(nil)
        }
        if Thread.isMainThread {
            change()
            return request.value()
        }
        // Never dispatch sync to a potentially waiting UI thread. A bounded
        // wait reports failure; invalidating the epoch prevents a late install.
        DispatchQueue.main.async(execute: change)
        if request.done.wait(timeout: .now() + 1.0) == .timedOut {
            lock.lock()
            if epoch == requestedEpoch {
                epoch &+= 1
                token = nil
                installed = false
                notifications.removeAll(keepingCapacity: true)
            }
            let cleanupEpoch = epoch
            lock.unlock()
            DispatchQueue.main.async { [self] in
                lock.lock()
                let needsCleanup = epoch == cleanupEpoch && token == nil
                lock.unlock()
                if needsCleanup { removeOnMain() }
            }
            return "enter_watch_timeout|Enter 감지 설정을 제시간에 완료하지 못했습니다. 앱이 응답하는지 확인하고 다시 시작하세요."
        }
        return request.value()
    }

    func take(clock: () -> UInt64 = nowNS, trusted: () -> Bool = { AXIsProcessTrusted() })
        -> (entries: [EnterNotification], now: UInt64, error: String?) {
        lock.lock()
        let currentToken = token
        let entries = notifications
        notifications.removeAll(keepingCapacity: true)
        lock.unlock()
        #if MACRO_NATIVE_SELF_TEST
        let now = drainEnvironmentForTest?.clock() ?? clock()
        let inputTrusted = currentToken == nil || (drainEnvironmentForTest?.trusted() ?? trusted())
        #else
        let now = clock()
        let inputTrusted = currentToken == nil || trusted()
        #endif
        if !inputTrusted {
            _ = set(nil)
            return ([], now, "enter_watch_permission|Enter 감지 권한이 해제되었습니다. 손쉬운 사용 권한을 확인하세요.")
        }
        let fresh = entries.filter { freshEnterNotification($0, token: currentToken, now: now) }
        lock.lock()
        for press in entries {
            guard let reason = enterQueueRejection(press, token: currentToken, now: now) else { continue }
            var diagnostic: [String: Any] = [
                "stage": "queue", "reason": reason, "token": press.token,
                "window_id": press.windowID, "front_pid": press.pid, "observed_at_ns": press.observedAt
            ]
            if now >= press.observedAt { diagnostic["age_ns"] = now - press.observedAt }
            diagnostics.append(diagnostic)
        }
        lock.unlock()
        return (fresh, now, nil)
    }

    func takeDiagnostics() -> [[String: Any]] {
        lock.lock()
        defer { lock.unlock() }
        return diagnostics.take()
    }

    #if MACRO_NATIVE_SELF_TEST
    // Synthetic state only: these helpers never install an OS monitor.
    func setDrainEnvironmentForTest(clock: @escaping () -> UInt64, trusted: @escaping () -> Bool) {
        drainEnvironmentForTest = (clock, trusted)
    }
    func seedForTest(token value: UInt64?, entries: [EnterNotification]) {
        lock.lock(); defer { lock.unlock() }
        epoch &+= 1
        token = value
        installed = value != nil
        notifications = entries
    }
    func stateForTest() -> (token: UInt64?, count: Int) {
        lock.lock(); defer { lock.unlock() }
        return (token, notifications.count)
    }
    func receiveForTest(_ event: NSEvent, oldEpoch: Bool = false) {
        lock.lock()
        let expected = oldEpoch ? epoch &- 1 : epoch
        lock.unlock()
        receive(event, epoch: expected)
    }
    #endif

    func close() {
        lock.lock()
        epoch &+= 1
        token = nil
        installed = false
        notifications.removeAll()
        _ = diagnostics.take()
        lock.unlock()
        if Thread.isMainThread { removeOnMain() }
        else { DispatchQueue.main.async { [self] in removeOnMain() } }
    }
}

@_cdecl("mmp_enter_new")
public func mmpEnterNew() -> UnsafeMutableRawPointer? {
    Unmanaged.passRetained(EnterMonitor()).toOpaque()
}

@_cdecl("mmp_enter_free")
public func mmpEnterFree(_ handle: UnsafeMutableRawPointer?) {
    guard let handle else { return }
    Unmanaged<EnterMonitor>.fromOpaque(handle).takeRetainedValue().close()
}

@_cdecl("mmp_enter_watch")
public func mmpEnterWatch(_ handle: UnsafeMutableRawPointer?, _ enabled: Int32,
                          _ token: UInt64) -> UnsafeMutablePointer<CChar>? {
    guard let handle else { return errorString("enter_watch_invalid|Enter 감지 모듈이 없습니다.") }
    return Unmanaged<EnterMonitor>.fromOpaque(handle).takeUnretainedValue()
        .set(enabled == 0 ? nil : token).flatMap(errorString)
}

// Each entry is exactly four UInt64 words: token, window ID, PID, timestamp.
@_cdecl("mmp_enter_take")
public func mmpEnterTake(_ handle: UnsafeMutableRawPointer?, _ entries: UnsafeMutablePointer<UInt64>?,
                         _ capacity: UInt64, _ count: UnsafeMutablePointer<UInt64>?,
                         _ observedNow: UnsafeMutablePointer<UInt64>?) -> UnsafeMutablePointer<CChar>? {
    guard let handle, let entries, let count, let observedNow, capacity >= 16 else {
        return errorString("enter_watch_invalid|Enter 감지 응답 버퍼가 올바르지 않습니다.")
    }
    count.pointee = 0
    let result = Unmanaged<EnterMonitor>.fromOpaque(handle).takeUnretainedValue().take()
    observedNow.pointee = result.now
    if let error = result.error { return errorString(error) }
    for (index, press) in result.entries.enumerated() {
        entries[index * 4] = press.token
        entries[index * 4 + 1] = press.windowID
        entries[index * 4 + 2] = UInt64(press.pid)
        entries[index * 4 + 3] = press.observedAt
    }
    count.pointee = UInt64(result.entries.count)
    return nil
}

// A null pointer means the diagnostic queue was empty. This keeps the normal
// polling path free of JSON allocations; nonempty results use mmp_free_string.
@_cdecl("mmp_enter_diagnostics_json")
public func mmpEnterDiagnosticsJSON(_ handle: UnsafeMutableRawPointer?) -> UnsafeMutablePointer<CChar>? {
    guard let handle else { return errorString("enter_diagnostics_invalid|Enter 감지 모듈이 없습니다.") }
    let entries = Unmanaged<EnterMonitor>.fromOpaque(handle).takeUnretainedValue().takeDiagnostics()
    guard !entries.isEmpty else { return nil }
    guard let data = try? JSONSerialization.data(withJSONObject: entries),
          let value = String(data: data, encoding: .utf8) else {
        return errorString("enter_diagnostics_invalid|Enter 진단 정보를 변환하지 못했습니다.")
    }
    return strdup(value)
}

// Separate instances keep token changes/None/Drop independent of Enter.
@_cdecl("mmp_captcha_input_new")
public func mmpCaptchaInputNew() -> UnsafeMutableRawPointer? {
    Unmanaged.passRetained(EnterMonitor(kind: .captchaInput)).toOpaque()
}

@_cdecl("mmp_captcha_input_free")
public func mmpCaptchaInputFree(_ handle: UnsafeMutableRawPointer?) {
    mmpEnterFree(handle)
}

@_cdecl("mmp_captcha_input_watch")
public func mmpCaptchaInputWatch(_ handle: UnsafeMutableRawPointer?, _ enabled: Int32,
                                 _ token: UInt64) -> UnsafeMutablePointer<CChar>? {
    guard let handle else { return errorString("captcha_input_watch_invalid|캡챠 첫 입력 감지 모듈이 없습니다.") }
    return Unmanaged<EnterMonitor>.fromOpaque(handle).takeUnretainedValue()
        .set(enabled == 0 ? nil : token).map(captchaInputError).flatMap(errorString)
}

@_cdecl("mmp_captcha_input_take")
public func mmpCaptchaInputTake(_ handle: UnsafeMutableRawPointer?, _ entries: UnsafeMutablePointer<UInt64>?,
                                _ capacity: UInt64, _ count: UnsafeMutablePointer<UInt64>?,
                                _ observedNow: UnsafeMutablePointer<UInt64>?) -> UnsafeMutablePointer<CChar>? {
    guard let failure = mmpEnterTake(handle, entries, capacity, count, observedNow) else { return nil }
    let message = captchaInputError(String(cString: failure))
    free(failure)
    return errorString(message)
}

// Like Enter diagnostics, an idle drain returns null without JSON allocation.
@_cdecl("mmp_captcha_input_diagnostics_json")
public func mmpCaptchaInputDiagnosticsJSON(_ handle: UnsafeMutableRawPointer?) -> UnsafeMutablePointer<CChar>? {
    guard let handle else { return errorString("captcha_input_diagnostics_invalid|캡챠 첫 입력 감지 모듈이 없습니다.") }
    let entries = Unmanaged<EnterMonitor>.fromOpaque(handle).takeUnretainedValue().takeDiagnostics()
    guard !entries.isEmpty else { return nil }
    guard let data = try? JSONSerialization.data(withJSONObject: entries),
          let value = String(data: data, encoding: .utf8) else {
        return errorString("captcha_input_diagnostics_invalid|캡챠 첫 입력 진단 정보를 변환하지 못했습니다.")
    }
    return strdup(value)
}

@_cdecl("mmp_beep")
public func mmpBeep() {
    DispatchQueue.main.async { NSSound.beep() }
}

private final class CaptureSession: NSObject, SCStreamOutput, SCStreamDelegate {
    let lock = NSLock()
    let queue = DispatchQueue(label: "integrated-macro.capture", qos: .userInteractive)
    var stream: SCStream?
    var frame: CVPixelBuffer?
    var sequence: UInt64 = 0
    var capturedAt: UInt64 = 0
    var observedAt: UInt64 = 0
    var error: String?
    var active = false
    var generation: UInt64 = 0
    var firstFrame = DispatchSemaphore(value: 0)

    func stop() {
        lock.lock()
        let previous = stream
        stream = nil
        frame = nil
        active = false
        generation &+= 1
        lock.unlock()
        previous?.stopCapture(completionHandler: { _ in })
    }

    func start(id: CGWindowID) -> String? {
        stop()
        guard CGPreflightScreenCaptureAccess() else {
            return "capture_permission|‘시스템 설정 → 개인정보 보호 및 보안 → 화면 및 시스템 오디오 녹음’에서 이 앱을 허용한 뒤 다시 실행하세요."
        }
        let completed = DispatchSemaphore(value: 0)
        firstFrame = DispatchSemaphore(value: 0)
        lock.lock()
        error = nil
        sequence = 0
        active = true
        let generation = self.generation
        lock.unlock()
        SCShareableContent.getExcludingDesktopWindows(true, onScreenWindowsOnly: true) { [self] content, failure in
            lock.lock()
            let current = active && self.generation == generation
            lock.unlock()
            guard current else { completed.signal(); return }
            guard let content, let window = content.windows.first(where: { $0.windowID == id }) else {
                lock.lock()
                if self.generation == generation {
                    error = "capture_target_missing|캡처할 예매창을 찾지 못했습니다. \(failure?.localizedDescription ?? "")"
                }
                lock.unlock()
                completed.signal()
                return
            }
            guard let display = content.displays.first(where: { $0.frame.contains(window.frame) }) else {
                lock.lock()
                if self.generation == generation {
                    error = "capture_cross_display|예매창 전체를 한 모니터 안에 놓은 뒤 ‘브라우저 연결’을 다시 실행하세요."
                }
                lock.unlock()
                completed.signal()
                return
            }
            // A display-space crop has exact CGEvent coordinates even on macOS
            // 13, where independent-window captures cannot suppress shadows.
            // Including only the bound window also excludes our status overlay.
            let filter = SCContentFilter(display: display, including: [window])
            let config = SCStreamConfiguration()
            var scale = Double(CGDisplayPixelsWide(display.displayID)) / Double(CGDisplayBounds(display.displayID).width)
            if #available(macOS 14.0, *) { scale = Double(filter.pointPixelScale) }
            config.width = max(1, Int((window.frame.width * scale).rounded()))
            config.height = max(1, Int((window.frame.height * scale).rounded()))
            config.sourceRect = window.frame.offsetBy(dx: -display.frame.minX, dy: -display.frame.minY)
            config.pixelFormat = kCVPixelFormatType_32BGRA
            config.colorSpaceName = CGColorSpace.sRGB
            config.minimumFrameInterval = CMTime(value: 1, timescale: 120)
            config.queueDepth = 3
            config.showsCursor = false
            config.capturesAudio = false
            let next = SCStream(filter: filter, configuration: config, delegate: self)
            do {
                try next.addStreamOutput(self, type: .screen, sampleHandlerQueue: queue)
                lock.lock()
                guard active, self.generation == generation else { lock.unlock(); completed.signal(); return }
                stream = next
                lock.unlock()
                next.startCapture { failure in
                    if let failure {
                        self.lock.lock()
                        if self.generation == generation {
                            self.error = "capture_start_failed|화면 캡처 시작 실패: \(failure.localizedDescription)"
                            self.active = false
                        }
                        self.lock.unlock()
                    }
                    completed.signal()
                }
            } catch {
                lock.lock()
                if self.generation == generation {
                    self.error = "capture_start_failed|화면 캡처 출력 연결 실패: \(error.localizedDescription)"
                    active = false
                }
                lock.unlock()
                completed.signal()
            }
        }
        guard completed.wait(timeout: .now() + 8) == .success else {
            stop()
            return "capture_timeout|화면 캡처 시작 응답이 없습니다. 화면 기록 권한과 연결한 창을 확인하세요."
        }
        lock.lock()
        let failure = error
        lock.unlock()
        if let failure { stop(); return failure }
        guard firstFrame.wait(timeout: .now() + 3) == .success else {
            stop()
            return "capture_no_frame|화면 캡처가 시작됐지만 첫 화면이 없습니다. 창이 최소화되었는지 확인하세요."
        }
        return nil
    }

    func stream(_ stream: SCStream, didOutputSampleBuffer buffer: CMSampleBuffer, of type: SCStreamOutputType) {
        guard type == .screen, CMSampleBufferIsValid(buffer),
              let attachments = CMSampleBufferGetSampleAttachmentsArray(buffer, createIfNecessary: false) as? [[SCStreamFrameInfo: Any]],
              let raw = attachments.first?[.status] as? Int,
              let status = SCFrameStatus(rawValue: raw) else { return }
        lock.lock()
        defer { lock.unlock() }
        guard active, self.stream === stream else { return }
        acceptFrame(status: status, image: CMSampleBufferGetImageBuffer(buffer))
    }

    // The callback holds the lock. Status notifications do not all carry an
    // image: only a real image may advance the sequence or satisfy firstFrame.
    func acceptFrame(status: SCFrameStatus, image: CVPixelBuffer?) {
        observedAt = nowNS()
        switch status {
        case .idle:
            // Keep the last valid image for a static screen. An idle callback
            // must never revive an image invalidated by blank/suspended.
            return
        case .started where image == nil:
            // A status-only start is not an image and not a capture failure.
            // Wait for the first complete image, preserving any prior failure.
            return
        case .complete, .started:
            guard let image else {
                frame = nil
                error = "capture_frame_missing|화면 갱신 알림에 이미지가 없습니다. ‘브라우저 연결’을 다시 실행하세요."
                return
            }
            error = nil
            let wasEmpty = frame == nil
            frame = image
            sequence &+= 1
            capturedAt = observedAt
            if wasEmpty { firstFrame.signal() }
        case .blank, .suspended, .stopped:
            frame = nil
            let state = status == .blank ? "blank" : status == .suspended ? "suspended" : "stopped"
            error = "capture_interrupted|화면 캡처가 중단되었습니다 (\(state)). 예매창으로 돌아온 뒤 ‘브라우저 연결’을 다시 실행하세요."
        @unknown default:
            frame = nil
            error = "capture_status_unknown|지원하지 않는 화면 캡처 상태입니다 (\(status.rawValue)). ‘브라우저 연결’을 다시 실행하세요."
            return
        }
    }

    func stream(_ stream: SCStream, didStopWithError failure: Error) {
        lock.lock()
        defer { lock.unlock() }
        guard self.stream === stream else { return }
        active = false
        frame = nil
        error = "capture_stopped|화면 캡처가 종료되었습니다: \(failure.localizedDescription)"
    }
}

@_cdecl("mmp_capture_new")
public func mmpCaptureNew() -> UnsafeMutableRawPointer {
    Unmanaged.passRetained(CaptureSession()).toOpaque()
}

@_cdecl("mmp_capture_free")
public func mmpCaptureFree(_ pointer: UnsafeMutableRawPointer?) {
    guard let pointer else { return }
    let session = Unmanaged<CaptureSession>.fromOpaque(pointer).takeRetainedValue()
    session.stop()
}

@_cdecl("mmp_capture_start")
public func mmpCaptureStart(_ pointer: UnsafeMutableRawPointer?, _ id: UInt64) -> UnsafeMutablePointer<CChar>? {
    guard let pointer else { return errorString("capture_invalid|캡처 모듈이 초기화되지 않았습니다.") }
    guard !Thread.isMainThread else {
        return errorString("capture_thread|화면 캡처 연결은 실행 작업 스레드에서 수행해야 합니다.")
    }
    let session = Unmanaged<CaptureSession>.fromOpaque(pointer).takeUnretainedValue()
    return session.start(id: CGWindowID(id)).flatMap(errorString)
}

// metadata: width, height, packed BGRA stride, sequence, image timestamp,
// most recent stream callback timestamp, required byte count.
@_cdecl("mmp_capture_copy")
public func mmpCaptureCopy(_ pointer: UnsafeMutableRawPointer?, _ destination: UnsafeMutablePointer<UInt8>?,
                          _ capacity: UInt64, _ metadata: UnsafeMutablePointer<UInt64>?) -> UnsafeMutablePointer<CChar>? {
    guard let pointer, let metadata else { return errorString("capture_invalid|캡처 모듈이 초기화되지 않았습니다.") }
    let session = Unmanaged<CaptureSession>.fromOpaque(pointer).takeUnretainedValue()
    session.lock.lock()
    if let failure = session.error { session.lock.unlock(); return errorString(failure) }
    guard session.active, let frame = session.frame else {
        session.lock.unlock()
        return errorString("capture_waiting|최신 캡처 화면을 기다리고 있습니다.")
    }
    let width = CVPixelBufferGetWidth(frame), height = CVPixelBufferGetHeight(frame), stride = width * 4
    metadata[0] = UInt64(width); metadata[1] = UInt64(height); metadata[2] = UInt64(stride)
    metadata[3] = session.sequence; metadata[4] = session.capturedAt; metadata[5] = session.observedAt
    metadata[6] = UInt64(stride * height)
    session.lock.unlock()
    guard let destination, capacity >= metadata[6] else { return nil }
    guard CVPixelBufferGetPixelFormatType(frame) == kCVPixelFormatType_32BGRA,
          CVPixelBufferLockBaseAddress(frame, .readOnly) == kCVReturnSuccess else {
        return errorString("capture_pixel_format|캡처 픽셀 형식을 읽을 수 없습니다.")
    }
    defer { CVPixelBufferUnlockBaseAddress(frame, .readOnly) }
    guard let source = CVPixelBufferGetBaseAddress(frame) else {
        return errorString("capture_empty_buffer|캡처 화면 버퍼가 비어 있습니다.")
    }
    let sourceStride = CVPixelBufferGetBytesPerRow(frame)
    for row in 0..<height {
        memcpy(destination.advanced(by: row * stride), source.advanced(by: row * sourceStride), stride)
    }
    return nil
}

#if MACRO_NATIVE_SELF_TEST
@_cdecl("mmp_input_cancellation_test")
public func mmpInputCancellationTest() -> UnsafeMutablePointer<CChar>? {
    for cancelAt in 0...3 {
        var cancelled = cancelAt == 0
        var events: [String] = []
        var startedAt: UInt64 = 0
        let result = postGuardedClick(cancelled: { cancelled },
            move: { events.append("move"); if cancelAt == 1 { cancelled = true } },
            down: { events.append("down"); if cancelAt == 2 { cancelled = true } },
            up: { events.append("up") }, clock: { 77 }, recordStart: { startedAt = $0 })
        let expected = cancelAt == 0 ? [] : cancelAt == 1 ? ["move"] : ["move", "down", "up"]
        guard events == expected, startedAt == (cancelAt < 2 ? 0 : 77),
              (result == cancelledInputError) == (cancelAt < 2) else {
            return errorString("test|click cancellation posted down or omitted the required release")
        }
    }
    for initiallyCancelled in [true, false] {
        var cancelled = initiallyCancelled
        var events: [String] = []
        var startedAt: UInt64 = 0
        let result = postGuardedKey(cancelled: { cancelled },
            down: { events.append("down"); cancelled = true }, up: { events.append("up") },
            clock: { 88 }, recordStart: { startedAt = $0 })
        guard events == (initiallyCancelled ? [] : ["down", "up"]),
              startedAt == (initiallyCancelled ? 0 : 88),
              (result == cancelledInputError) == initiallyCancelled else {
            return errorString("test|key cancellation omitted release or reported a delivered down as cancelled")
        }
    }
    var order: [String] = []
    let timing = postGuardedClick(cancelled: { false }, move: { order.append("move") },
        down: { order.append("down") }, up: { order.append("up") },
        clock: { order.append("clock"); return 99 }, recordStart: { _ in order.append("receipt") })
    guard timing == nil, order == ["move", "clock", "receipt", "down", "up"] else {
        return errorString("test|input receipt was recorded before setup or after posting")
    }
    return nil
}

// Pure state probes, including AX allowed/listen denied. Do not infer that
// permission bits are equivalent or that any combination ensures delivery.
@_cdecl("mmp_keyboard_input_status_test")
public func mmpKeyboardInputStatusTest() -> UnsafeMutablePointer<CChar>? {
    for expected in UInt32(0)..<UInt32(8) {
        var calls = [0, 0, 0]
        let actual = keyboardInputStatus(
            accessibility: { calls[0] += 1; return expected & 1 != 0 },
            listen: { calls[1] += 1; return expected & 2 != 0 },
            secure: { calls[2] += 1; return expected & 4 != 0 })
        guard actual == expected, calls == [1, 1, 1] else {
            return errorString("test|read-only keyboard status bits changed")
        }
    }
    return nil
}

// Pure key metadata and timestamps only; never installs a monitor or types.
@_cdecl("mmp_enter_filter_test")
public func mmpEnterFilterTest() -> UnsafeMutablePointer<CChar>? {
    let cases: [(UInt16, Bool, NSEvent.ModifierFlags, Int64, Bool)] = [
        (36, false, [], 0, true),
        (76, false, [], 0, true),
        (36, true, [], 0, false),
        (36, false, [.command], 0, false),
        (36, false, [.shift], 0, false),
        (36, false, [.control], 0, false),
        (36, false, [.option], 0, false),
        (36, false, [.capsLock], 0, true),
        (36, false, [], 42, false),
        (0, false, [], 0, false)
    ]
    for (code, repeated, modifiers, source, expected) in cases {
        guard isUserEnter(keyCode: code, repeated: repeated, modifiers: modifiers,
                          sourcePID: source, ownPID: 42) == expected else {
            return errorString("test|passive Enter key filtering failed")
        }
        if [UInt16(36), 76].contains(code) {
            guard (enterCallbackRejection(repeated: repeated, modifiers: modifiers,
                sourcePID: source, ownPID: 42, frontPID: 10, recipientPID: 10) == nil) == expected else {
                return errorString("test|Enter diagnostic changed key filtering")
            }
        }
    }
    let routing: [(pid_t?, Int64, String?)] = [
        (nil, 0, "front_app_unavailable"), (0, 0, "front_app_unavailable"),
        (42, 42, "own_front_app"), (10, 20, "recipient_mismatch"),
        (10, 0, nil), (10, -1, nil), (10, 10, nil)
    ]
    for (front, recipient, reason) in routing {
        guard enterCallbackRejection(repeated: false, modifiers: [], sourcePID: 0,
            ownPID: 42, frontPID: front, recipientPID: recipient) == reason else {
            return errorString("test|Enter recipient diagnostic changed routing")
        }
    }
    let press = EnterNotification(token: 7, windowID: 0, pid: 10, observedAt: 1_000_000_000)
    guard freshEnterNotification(press, token: 7, now: 1_500_000_000),
          !freshEnterNotification(press, token: 7, now: 1_500_000_001),
          !freshEnterNotification(press, token: 8, now: 1_000_000_000),
          !freshEnterNotification(press, token: nil, now: 1_000_000_000),
          !freshEnterNotification(press, token: 7, now: 999_999_999) else {
        return errorString("test|passive Enter age/token filtering failed")
    }
    let freshness: [(UInt64?, UInt64, String?)] = [
        (7, 1_500_000_000, nil), (7, 1_500_000_001, "stale_event"),
        (8, 1_000_000_000, "stale_token"), (nil, 1_000_000_000, "stale_token"),
        (7, 999_999_999, "future_event")
    ]
    for (token, now, reason) in freshness {
        guard enterQueueRejection(press, token: token, now: now) == reason,
              freshEnterNotification(press, token: token, now: now) == (reason == nil) else {
            return errorString("test|Enter freshness diagnostic changed filtering")
        }
    }
    var diagnostics = EnterDiagnostics()
    for token in UInt64(0)..<20 {
        diagnostics.append(["stage": "callback", "reason": "enqueued", "token": token,
            "window_id": UInt64(44), "front_pid": Int32(10), "source_pid": Int64(0),
            "recipient_pid": Int64(10), "repeated": false, "modifiers": UInt(0),
            "observed_at_ns": UInt64(1_000_000_000)])
    }
    let metadata = diagnostics.take()
    guard metadata.count == 16, metadata.first?["token"] as? UInt64 == 4,
          diagnostics.take().isEmpty,
          let encoded = try? JSONSerialization.data(withJSONObject: metadata),
          let decoded = (try? JSONSerialization.jsonObject(with: encoded)) as? [[String: Any]],
          decoded.count == 16, decoded.first?["stage"] as? String == "callback" else {
        return errorString("test|Enter diagnostic queue/JSON contract failed")
    }
    return nil
}

@_cdecl("mmp_captcha_input_filter_test")
public func mmpCaptchaInputFilterTest() -> UnsafeMutablePointer<CChar>? {
    let allowed: [UInt16] = [0, 9, 11, 12, 17, 18, 23, 25, 26, 28, 29,
        31, 32, 34, 35, 37, 38, 40, 45, 46, 82, 83, 89, 91, 92]
    let excluded: [UInt16] = [10, 24, 27, 30, 33, 36, 39, 41, 42, 43, 44, 47,
        48, 49, 50, 51, 53, 63, 65, 67, 69, 71, 75, 76, 78, 81, 90, 93, 122, 123, 124, 125, 126]
    for code in allowed {
        guard isUserCaptchaInput(keyCode: code, repeated: false, modifiers: [], sourcePID: 0, ownPID: 42),
              isUserCaptchaInput(keyCode: code, repeated: false, modifiers: [.shift], sourcePID: 0, ownPID: 42),
              !isUserCaptchaInput(keyCode: code, repeated: true, modifiers: [], sourcePID: 0, ownPID: 42) else {
            return errorString("test|CAPTCHA character/repeat filtering failed")
        }
    }
    for code in excluded {
        guard !isUserCaptchaInput(keyCode: code, repeated: false, modifiers: [], sourcePID: 0, ownPID: 42) else {
            return errorString("test|CAPTCHA non-character filtering failed")
        }
    }
    for modifier in [NSEvent.ModifierFlags.control, .option, .command, .function] {
        guard !isUserCaptchaInput(keyCode: 0, repeated: false, modifiers: modifier, sourcePID: 0, ownPID: 42) else {
            return errorString("test|CAPTCHA shortcut filtering failed")
        }
    }
    for source in [Int64(0), 7, -1] {
        guard isUserCaptchaInput(keyCode: 0, repeated: false, modifiers: [], sourcePID: source, ownPID: 42) else {
            return errorString("test|CAPTCHA non-own source metadata was incorrectly rejected")
        }
    }
    guard !isUserCaptchaInput(keyCode: 0, repeated: false, modifiers: [], sourcePID: 42, ownPID: 42),
          isUserCaptchaInput(keyCode: 0, repeated: false, modifiers: [.shift, .capsLock], sourcePID: 0, ownPID: 42),
          isUserCaptchaInput(keyCode: 82, repeated: false, modifiers: [.numericPad], sourcePID: 0, ownPID: 42) else {
        return errorString("test|CAPTCHA allowed modifier filtering failed")
    }
    let enter = EnterMonitor()
    let captcha = EnterMonitor(kind: .captchaInput)
    defer { enter.close(); captcha.close() }
    let press = EnterNotification(token: 8, windowID: 44, pid: 10, observedAt: 1_000_000_000)
    enter.seedForTest(token: 7, entries: [press])
    captcha.seedForTest(token: 8, entries: [press])
    guard enter.set(nil) == nil,
          enter.stateForTest().token == nil, enter.stateForTest().count == 0,
          captcha.stateForTest().token == 8, captcha.stateForTest().count == 1 else {
        return errorString("test|disabling Enter affected independent CAPTCHA input watch")
    }
    enter.seedForTest(token: 9, entries: [press])
    guard captcha.set(nil) == nil,
          captcha.stateForTest().token == nil, captcha.stateForTest().count == 0,
          enter.stateForTest().token == 9, enter.stateForTest().count == 1 else {
        return errorString("test|disabling CAPTCHA input affected independent Enter watch")
    }
    captcha.seedForTest(token: nil, entries: [press])
    let discarded = captcha.take()
    let queueDiagnostics = captcha.takeDiagnostics()
    guard discarded.error == nil, discarded.entries.isEmpty,
          queueDiagnostics.count == 1,
          queueDiagnostics.first?["reason"] as? String == "stale_token",
          captcha.takeDiagnostics().isEmpty else {
        return errorString("test|CAPTCHA input stale queue diagnostic was lost or retained twice")
    }
    return nil
}

// Synthetic NSEvents call the production receive method. No monitor is
// installed, no event is posted, and no live keyboard or foreground app is read.
@_cdecl("mmp_captcha_input_callback_test")
public func mmpCaptchaInputCallbackTest() -> UnsafeMutablePointer<CChar>? {
    var at: UInt64 = 1_000_000_000
    var front: pid_t? = 10
    let captcha = EnterMonitor(kind: .captchaInput, environment: KeyboardMonitorEnvironment(
        ownPID: { 42 }, frontPID: { front }, clock: { at }))
    defer { captcha.close() }
    captcha.seedForTest(token: 8, entries: [])
    func send(_ key: UInt16 = 0, repeated: Bool = false,
              modifiers: NSEvent.ModifierFlags = [], source: Int64 = 0,
              recipient: Int64 = 10, oldEpoch: Bool = false) -> Bool {
        guard let cg = CGEvent(keyboardEventSource: nil, virtualKey: key, keyDown: true) else { return false }
        cg.flags = CGEventFlags(rawValue: UInt64(modifiers.rawValue))
        cg.setIntegerValueField(.keyboardEventAutorepeat, value: repeated ? 1 : 0)
        cg.setIntegerValueField(.eventSourceUnixProcessID, value: source)
        cg.setIntegerValueField(.eventTargetUnixProcessID, value: recipient)
        guard let event = NSEvent(cgEvent: cg) else { return false }
        captcha.receiveForTest(event, oldEpoch: oldEpoch)
        return true
    }
    func take() -> (entries: [EnterNotification], now: UInt64, error: String?) {
        captcha.take(clock: { at }, trusted: { true })
    }

    guard send(modifiers: [.shift]) else { return errorString("test|synthetic CAPTCHA event construction failed") }
    let accepted = take()
    let acceptedDiagnostics = captcha.takeDiagnostics()
    guard accepted.error == nil, accepted.entries.count == 1,
          accepted.entries[0].token == 8, accepted.entries[0].pid == 10,
          accepted.entries[0].observedAt == at,
          acceptedDiagnostics.count == 1,
          acceptedDiagnostics[0]["reason"] as? String == "enqueued",
          acceptedDiagnostics[0]["source_pid"] as? Int64 == 0,
          acceptedDiagnostics[0]["recipient_pid"] as? Int64 == 10,
          acceptedDiagnostics[0]["window_id"] as? UInt64 == accepted.entries[0].windowID else {
        return errorString("test|CAPTCHA NSEvent callback did not deliver routing metadata to queue/take")
    }

    let rejected: [(Bool, NSEvent.ModifierFlags, Int64, pid_t?, Int64, Bool, String)] = [
        (true, [], 0, 10, 10, false, "repeated"),
        (false, [.command], 0, 10, 10, false, "modifier_pressed"),
        (false, [.control], 0, 10, 10, false, "modifier_pressed"),
        (false, [.option], 0, 10, 10, false, "modifier_pressed"),
        (false, [.function], 0, 10, 10, false, "modifier_pressed"),
        (false, [], 42, 10, 10, false, "own_source"),
        (false, [], 0, nil, 10, false, "front_app_unavailable"),
        (false, [], 0, 42, 42, false, "own_front_app"),
        (false, [], 0, 10, 20, false, "recipient_mismatch"),
        (false, [], 0, 10, 10, true, "inactive_monitor")
    ]
    for (repeated, modifiers, source, foreground, recipient, oldEpoch, expected) in rejected {
        front = foreground
        guard send(repeated: repeated, modifiers: modifiers, source: source,
                   recipient: recipient, oldEpoch: oldEpoch) else {
            return errorString("test|synthetic rejected CAPTCHA event construction failed")
        }
        let drained = take()
        let records = captcha.takeDiagnostics()
        guard drained.error == nil, drained.entries.isEmpty, records.count == 1,
              records[0]["reason"] as? String == expected,
              !oldEpoch || records[0]["token"] == nil else {
            return errorString("test|CAPTCHA callback diagnostic changed rejection: \(expected)")
        }
    }
    front = 10
    for key in [UInt16(36), 53, 123, 48] {
        guard send(key), take().entries.isEmpty, captcha.takeDiagnostics().isEmpty else {
            return errorString("test|non-character event entered CAPTCHA notification/diagnostic queue")
        }
    }

    guard send() else { return errorString("test|synthetic stale CAPTCHA event construction failed") }
    at += 500_000_001
    let expired = take()
    let expiryDiagnostics = captcha.takeDiagnostics()
    guard expired.error == nil, expired.entries.isEmpty, expiryDiagnostics.count == 2,
          expiryDiagnostics[1]["stage"] as? String == "queue",
          expiryDiagnostics[1]["reason"] as? String == "stale_event",
          expiryDiagnostics[1]["age_ns"] as? UInt64 == 500_000_001 else {
        return errorString("test|CAPTCHA callback notification expiration was not diagnosed")
    }

    let first = at
    for index in UInt64(0)..<20 {
        at = first + index
        guard send(source: 7, recipient: 0) else { return errorString("test|synthetic bounded CAPTCHA event construction failed") }
    }
    let bounded = take()
    guard bounded.error == nil, bounded.entries.count == 16,
          bounded.entries.first?.observedAt == first + 4 else {
        return errorString("test|CAPTCHA callback notification queue exceeded its limit")
    }
    let handle = Unmanaged.passUnretained(captcha).toOpaque()
    guard let json = mmpCaptchaInputDiagnosticsJSON(handle) else {
        return errorString("test|CAPTCHA diagnostic FFI dropped callback records")
    }
    let bytes = Data(String(cString: json).utf8)
    free(json)
    guard let records = (try? JSONSerialization.jsonObject(with: bytes)) as? [[String: Any]],
          records.count == 16,
          (records.first?["observed_at_ns"] as? NSNumber)?.uint64Value == first + 4 else {
        return errorString("test|CAPTCHA diagnostic FFI queue/JSON contract failed")
    }
    let allowedFields: Set<String> = ["stage", "reason", "token", "window_id", "source_pid",
        "recipient_pid", "front_pid", "repeated", "modifiers", "observed_at_ns", "age_ns"]
    guard records.allSatisfy({ Set($0.keys).isSubset(of: allowedFields) }),
          mmpCaptchaInputDiagnosticsJSON(handle) == nil else {
        return errorString("test|CAPTCHA diagnostics retained input content or did not drain")
    }
    return nil
}

// Replay only the shared metadata from nine user-observed physical key pairs.
// The original characters were not recorded. A representative character key
// exercises CAPTCHA; representative Enter uses the same observed routing to
// guard its shared implementation, not to claim Enter was in the live sample.
// This test installs no monitor, posts no input, and uses no live TCC state.
@_cdecl("mmp_global_monitor_recipient_test")
public func mmpGlobalMonitorRecipientTest(_ fixturePath: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? {
    guard let fixturePath,
          let data = try? Data(contentsOf: URL(fileURLWithPath: String(cString: fixturePath))),
          let fixture = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
          let own = (fixture["observerPID"] as? NSNumber)?.int64Value,
          let browser = (fixture["browserPID"] as? NSNumber)?.int32Value,
          let shared = fixture["sharedCandidateFields"] as? [String: Any],
          let source = (shared["sourcePID"] as? NSNumber)?.int64Value,
          let front = (shared["frontPID"] as? NSNumber)?.int32Value,
          let repeated = shared["repeated"] as? Bool,
          let modifiers = (shared["modifiers"] as? NSNumber)?.uint64Value,
          let channels = fixture["channelFields"] as? [String: [String: Any]],
          let rawRecipient = (channels["cg_session_tap"]?["recipientPID"] as? NSNumber)?.int64Value,
          let observedRecipient = (channels["nsevent_global"]?["recipientPID"] as? NSNumber)?.int64Value,
          let observedWindow = (channels["nsevent_global"]?["windowNumber"] as? NSNumber)?.uint64Value,
          let samples = fixture["pairedEventTimestampNs"] as? [NSNumber], samples.count == 9,
          source == 0, front == browser, browser > 0, own > 0, own <= Int64(Int32.max),
          own != Int64(browser), !repeated,
          rawRecipient == Int64(browser), observedRecipient == own, observedWindow == 0 else {
        return errorString("test|physical-input routing fixture is missing or invalid")
    }
    let flags = NSEvent.ModifierFlags(rawValue: UInt(modifiers))
    // Keep the raw route strict: only the AppKit-copy call site normalizes it.
    guard enterCallbackRejection(repeated: repeated, modifiers: flags, sourcePID: source,
              ownPID: own, frontPID: front, recipientPID: rawRecipient) == nil,
          enterCallbackRejection(repeated: repeated, modifiers: flags, sourcePID: source,
              ownPID: own, frontPID: front, recipientPID: observedRecipient) == "recipient_mismatch" else {
        return errorString("test|raw recipient routing was broadened")
    }

    for kind in [EnterMonitor.Kind.captchaInput, .enter] {
        var at: UInt64 = 1
        var currentFront: pid_t? = front
        let monitor = EnterMonitor(kind: kind, environment: KeyboardMonitorEnvironment(
            ownPID: { own }, frontPID: { currentFront }, clock: { at }))
        monitor.setDrainEnvironmentForTest(clock: { at }, trusted: { true })
        defer { monitor.close() }
        let token: UInt64 = 77
        monitor.seedForTest(token: token, entries: [])
        let handle = Unmanaged.passUnretained(monitor).toOpaque()
        let key: UInt16 = kind == .captchaInput ? 0 : 36
        func send(eventTime: UInt64, sourcePID: Int64, recipientPID: Int64, oldEpoch: Bool = false) -> Bool {
            guard let cg = CGEvent(keyboardEventSource: nil, virtualKey: key, keyDown: true) else { return false }
            cg.timestamp = eventTime
            cg.flags = CGEventFlags(rawValue: modifiers)
            cg.setIntegerValueField(.keyboardEventAutorepeat, value: repeated ? 1 : 0)
            cg.setIntegerValueField(.eventSourceUnixProcessID, value: sourcePID)
            cg.setIntegerValueField(.eventTargetUnixProcessID, value: recipientPID)
            guard let event = NSEvent(cgEvent: cg), event.windowNumber == 0 else { return false }
            monitor.receiveForTest(event, oldEpoch: oldEpoch)
            return true
        }
        func drain() -> (words: [UInt64], count: UInt64, now: UInt64, error: String?) {
            var words = [UInt64](repeating: 0, count: 16 * 4)
            var count: UInt64 = 99
            var now: UInt64 = 0
            let failure = words.withUnsafeMutableBufferPointer { buffer in
                kind == .captchaInput
                    ? mmpCaptchaInputTake(handle, buffer.baseAddress, 16, &count, &now)
                    : mmpEnterTake(handle, buffer.baseAddress, 16, &count, &now)
            }
            let error = failure.map { String(cString: $0) }
            if let failure { free(failure) }
            return (words, count, now, error)
        }
        func records() -> [[String: Any]]? {
            let pointer = kind == .captchaInput ? mmpCaptchaInputDiagnosticsJSON(handle) : mmpEnterDiagnosticsJSON(handle)
            guard let pointer else { return [] }
            let bytes = Data(String(cString: pointer).utf8)
            free(pointer)
            return (try? JSONSerialization.jsonObject(with: bytes)) as? [[String: Any]]
        }

        for sample in samples {
            let eventTime = sample.uint64Value
            // Notification/FFI times use a deterministic callback clock. The
            // event timestamp is the actual matching key from the saved pair.
            at = eventTime + 1_000_000
            guard send(eventTime: eventTime, sourcePID: source, recipientPID: observedRecipient) else {
                return errorString("test|observed AppKit routing replay event could not be constructed")
            }
            let result = drain()
            guard result.error == nil, result.count == 1, result.now == at,
                  Array(result.words.prefix(4)) == [token, observedWindow, UInt64(browser), at],
                  let diagnostic = records(), diagnostic.count == 1,
                  diagnostic[0]["reason"] as? String == "enqueued",
                  (diagnostic[0]["recipient_pid"] as? NSNumber)?.int64Value == own,
                  (diagnostic[0]["front_pid"] as? NSNumber)?.int32Value == browser,
                  drain().count == 0, records()?.isEmpty == true else {
                return errorString("test|observed AppKit recipient alias did not pass callback/queue/FFI exactly once")
            }
        }

        let negatives: [(Int64, pid_t?, Int64, Bool, String)] = [
            (own, front, own, false, "own_source"),
            (0, pid_t(own), own, false, "own_front_app"),
            (0, nil, own, false, "front_app_unavailable"),
            (0, front, Int64(browser) + 1, false, "recipient_mismatch"),
            (0, front, own, true, "inactive_monitor")
        ]
        for (sourcePID, foreground, recipientPID, oldEpoch, reason) in negatives {
            currentFront = foreground
            at += 1_000_000
            guard send(eventTime: at, sourcePID: sourcePID, recipientPID: recipientPID, oldEpoch: oldEpoch) else {
                return errorString("test|recipient-negative replay event could not be constructed")
            }
            let result = drain()
            guard result.error == nil, result.count == 0,
                  let diagnostic = records(), diagnostic.count == 1,
                  diagnostic[0]["reason"] as? String == reason else {
                return errorString("test|AppKit alias weakened rejection: \(reason)")
            }
        }
        currentFront = front
        at += 1_000_000
        guard send(eventTime: at, sourcePID: 0, recipientPID: own) else {
            return errorString("test|recipient-age replay event could not be constructed")
        }
        at += 500_000_001
        let expired = drain()
        guard expired.error == nil, expired.count == 0,
              let diagnostic = records(), diagnostic.count == 2,
              diagnostic[1]["reason"] as? String == "stale_event" else {
            return errorString("test|AppKit alias weakened the 500 ms notification limit")
        }
    }
    return nil
}

// Pure metadata and injected clock/state only: never changes desktop focus.
@_cdecl("mmp_activation_test")
public func mmpActivationTest() -> UnsafeMutablePointer<CChar>? {
    let rect = CGRect(x: 0, y: 33, width: 1512, height: 891)
    let identity = AXWindowIdentity(pid: 19961, rect: rect)
    let helperIdentity = AXWindowIdentity(pid: 19961, rect: CGRect(x: 10, y: 44, width: 66, height: 20))
    func window(_ id: UInt64, _ pid: UInt32 = 19961, _ geometry: CGRect? = nil) -> [String: Any] {
        [kCGWindowNumber as String: NSNumber(value: id),
         kCGWindowOwnerPID as String: NSNumber(value: pid),
         kCGWindowBounds as String: (geometry ?? rect).dictionaryRepresentation,
         kCGWindowAlpha as String: NSNumber(value: 1),
         kCGWindowLayer as String: NSNumber(value: 0)]
    }
    let target = window(123013)
    var offscreenTarget = target
    offscreenTarget[kCGWindowIsOnscreen as String] = false
    let helper = window(125397, 19961, helperIdentity.rect)
    func select(_ windows: [[String: Any]], _ candidates: [AXWindowIdentity?]) -> String {
        switch selectActivationWindow(id: 123013, pid: 19961, expected: rect, windows: windows, candidates: candidates) {
        case let .window(index): return "window:\(index)"
        case let .failure(message): return String(message.split(separator: "|", maxSplits: 1)[0])
        }
    }
    let selectionChecks: [(String, String, String)] = [
        ("settings focus cannot replace saved target", select([window(99, 100), target], [identity]), "window:0"),
        ("tiny helper before target is not selected", select([helper, target], [helperIdentity, identity]), "window:1"),
        ("closed target cannot select similar replacement", select([window(123014)], [identity]), "target_closed"),
        ("reused window ID rejects wrong owner", select([window(123013, 100)], [identity]), "target_identity_changed"),
        ("moved target cannot be raised with stale coordinates", select([window(123013, 19961, rect.offsetBy(dx: 1, dy: 0))], [identity]), "target_geometry_changed"),
        ("duplicate CG bounds are ambiguous", select([target, window(123014)], [identity]), "target_activation_ambiguous"),
        ("duplicate AX bounds are ambiguous", select([target], [identity, identity]), "target_activation_ambiguous"),
        ("missing AX target fails", select([target], [nil, helperIdentity]), "target_activation_ambiguous"),
        ("wrong AX owner cannot match geometry", select([target], [AXWindowIdentity(pid: 100, rect: rect)]), "target_activation_ambiguous"),
        ("saved offscreen identity retained for restoration", select([offscreenTarget], [identity]), "window:0")
    ]
    for (name, actual, expected) in selectionChecks where actual != expected {
        return errorString("test|activation selection: \(name); expected \(expected), received \(actual)")
    }
    func poll(_ states: [String?]) -> (String?, Int, UInt64) {
        var now: UInt64 = 0
        var reads = 0
        let result = waitForActivation(deadline: 30, now: { now }, pause: { now += 10 }) {
            defer { reads += 1 }
            return states[min(reads, states.count - 1)]
        }
        return (result, reads, now)
    }
    let ready = poll([nil])
    guard ready.0 == nil, ready.1 == 1, ready.2 == 0 else {
        return errorString("test|already active target unnecessarily waited")
    }
    let restored = poll(["target_not_foreground|settings", "target_window_changed|other Chrome",
                         "target_focus_unavailable|restore animation", nil])
    guard restored.0 == nil, restored.1 == 4, restored.2 == 30 else {
        return errorString("test|activation did not wait for actual saved window restoration")
    }
    let denied = poll(["target_not_foreground|settings"])
    guard denied.0?.hasPrefix("target_activation_failed|") == true, denied.1 == 4, denied.2 == 30 else {
        return errorString("test|denied activation was not bounded or was accepted")
    }
    let changed = poll(["target_geometry_changed|changed", nil])
    guard changed.0?.hasPrefix("target_geometry_changed|") == true, changed.1 == 1, changed.2 == 0 else {
        return errorString("test|activation retried changed coordinates")
    }
    let closed = poll(["target_closed|closed", nil])
    guard closed.0?.hasPrefix("target_closed|") == true, closed.1 == 1 else {
        return errorString("test|activation retried lost identity")
    }
    let permission = poll(["accessibility_permission|denied", nil])
    guard permission.0?.hasPrefix("accessibility_permission|") == true, permission.1 == 1 else {
        return errorString("test|activation concealed missing permission")
    }
    return nil
}

// Synthetic window metadata only: no desktop access, focus changes or input.
@_cdecl("mmp_window_validation_test")
public func mmpWindowValidationTest() -> UnsafeMutablePointer<CChar>? {
    let targetID: UInt64 = 10
    let browserPID: UInt32 = 100
    let ownPID: Int32 = 200
    let targetRect = CGRect(x: -500, y: 100, width: 400, height: 300)
    let samplePoint = CGPoint(x: -350, y: 250)
    func window(_ id: UInt64, _ pid: UInt32, _ rect: CGRect, alpha: Double = 1, layer: Int = 0) -> [String: Any] {
        [kCGWindowNumber as String: NSNumber(value: id),
         kCGWindowOwnerPID as String: NSNumber(value: pid),
         kCGWindowBounds as String: rect.dictionaryRepresentation,
         kCGWindowAlpha as String: NSNumber(value: alpha),
         kCGWindowLayer as String: NSNumber(value: layer)]
    }
    let target = window(targetID, browserPID, targetRect)
    let overlay = window(20, UInt32(ownPID), targetRect, layer: 3)
    let otherBrowser = window(30, browserPID, targetRect)
    let distantBrowser = window(30, browserPID, CGRect(x: 0, y: 0, width: 400, height: 300))
    let unrelated = window(40, 300, targetRect)
    let settings = window(50, UInt32(ownPID), targetRect)
    func validate(_ windows: [[String: Any]], point: CGPoint? = samplePoint,
                  ignored: [UInt64] = [], foreground: Int32? = ownPID, input: Bool = false) -> String? {
        validateWindow(id: targetID, pid: browserPID, expected: targetRect, point: point,
            windows: windows, ignored: ignored, ownPID: ownPID,
            foregroundPID: foreground, requireForeground: input,
            probes: WindowProbes(focused: { _ in
                let first = windows.first { ($0[kCGWindowOwnerPID as String] as? UInt32) == browserPID
                    && ($0[kCGWindowLayer as String] as? Int) == 0 }
                guard let first, let rect = bounds(first) else { return .failure(.unavailable) }
                return .success(AXWindowIdentity(pid: Int32(browserPID), rect: rect))
            }, hit: { _ in .failure(.unavailable) }, bundle: { _ in nil }, displays: { [] }))
    }
    let checks: [(String, String?, String?)] = [
        ("read visible target while settings owns keyboard focus", validate([target]), nil),
        ("input still requires keyboard focus", validate([target], input: true), "target_not_foreground"),
        ("read through registered own overlay", validate([overlay, target], ignored: [20]), nil),
        ("own overlay cannot bypass input focus", validate([overlay, target], ignored: [20], input: true), "target_not_foreground"),
        ("focused bound input remains valid", validate([overlay, target], ignored: [20], foreground: Int32(browserPID), input: true), nil),
        ("read visible point with distant browser window in front", validate([distantBrowser, target]), nil),
        ("input rejects other browser window even if distant", validate([distantBrowser, target], foreground: Int32(browserPID), input: true), "target_window_changed"),
        ("keys retain exact browser window guard", validate([distantBrowser, target], point: nil, foreground: Int32(browserPID), input: true), "target_window_changed"),
        ("read rejects another browser window at pointer", validate([otherBrowser, target]), "target_window_changed"),
        ("browser window cannot pose as own overlay", validate([otherBrowser, target], ignored: [30]), "target_window_changed"),
        ("read rejects unrelated occluding app", validate([unrelated, target]), "target_occluded"),
        ("foreign overlay cannot bypass occlusion", validate([unrelated, target], ignored: [40]), "target_occluded"),
        ("unregistered settings window remains occlusion", validate([settings, target]), "target_occluded"),
        ("point outside bound target rejected", validate([target], point: CGPoint(x: 1, y: 1)), "point_outside_target"),
        ("offscreen or closed target rejected", validate([]), "target_closed"),
        ("reused ID with different owner rejected", validate([window(targetID, 300, targetRect)]), "target_closed"),
        ("changed geometry rejected", validate([window(targetID, browserPID, targetRect.offsetBy(dx: 1, dy: 0))]), "target_geometry_changed"),
        ("transparent window does not obscure point", validate([window(40, 300, targetRect, alpha: 0), target]), nil)
    ]
    // Replay both reported Chrome points plus the final point outside Chrome.
    // Window IDs/owner PIDs/rectangles are the user's diagnostic values; AX is
    // explicitly synthetic because the diagnostic did not include AX evidence.
    let reportedPID: UInt32 = 19961
    let reportedRect = CGRect(x: 0, y: 33, width: 1512, height: 891)
    let display = CGRect(x: 0, y: 0, width: 1512, height: 982)
    let chrome = window(123013, reportedPID, reportedRect)
    let helper = window(125397, reportedPID, CGRect(x: 10, y: 44, width: 66, height: 20))
    let dock = window(13, 1397, display, layer: 20)
    let chromeIdentity = AXWindowIdentity(pid: Int32(reportedPID), rect: reportedRect)
    let positive: AXWindowResult = .success(chromeIdentity)
    func replay(_ windows: [[String: Any]] = [],
                point: CGPoint? = CGPoint(x: 1129.640625, y: 724.62890625),
                hit: AXWindowResult = positive, focus: AXWindowResult = positive,
                bundle: String? = "com.apple.dock", input: Bool = false,
                foreground: Int32? = Int32(reportedPID)) -> String? {
        validateWindow(id: 123013, pid: reportedPID, expected: reportedRect, point: point,
            windows: windows.isEmpty ? [dock, helper, chrome] : windows, ignored: [], ownPID: ownPID,
            foregroundPID: foreground, requireForeground: input,
            probes: WindowProbes(focused: { _ in focus }, hit: { _ in hit },
                bundle: { _ in bundle }, displays: { [display] }))
    }
    let helperIdentity = AXWindowIdentity(pid: Int32(reportedPID), rect: bounds(helper)!)
    let dockIdentity = AXWindowIdentity(pid: 1397, rect: .null)
    let otherIdentity = AXWindowIdentity(pid: 1399, rect: reportedRect)
    let duplicate = window(999, reportedPID, reportedRect)
    let foreign = window(999, 1399, reportedRect)
    let otherChrome = window(999, reportedPID, CGRect(x: 20, y: 40, width: 1400, height: 850))
    let otherChromeIdentity = AXWindowIdentity(pid: Int32(reportedPID), rect: bounds(otherChrome)!)
    let replayChecks: [(String, String?, String?)] = [
        ("reported Chrome foreground point with verified Dock surface", replay(), nil),
        ("reported prior point with terminal focus still readable", replay(point: CGPoint(x: 1146.69140625, y: 703.1953125), foreground: 96549), nil),
        ("reported final outside point rejected before AX", replay(point: CGPoint(x: 546.5625, y: 981.98046875)), "point_outside_target"),
        ("reported helper does not replace focused Chrome for input", replay(input: true), nil),
        ("reported helper does not replace focused Chrome for key", replay(point: nil, input: true), nil),
        ("terminal focus still blocks input through Dock surface", replay(input: true, foreground: 96549), "target_not_foreground"),
        ("actual Dock hit is blocked", replay(hit: .success(dockIdentity)), "target_occluded"),
        ("another app AX hit is blocked", replay(hit: .success(otherIdentity)), "target_occluded"),
        ("own status overlay AX hit is not positive browser evidence", replay(hit: .success(AXWindowIdentity(pid: ownPID, rect: .null))), "target_occluded"),
        ("unknown AX hit is blocked", replay(hit: .failure(.unavailable)), "target_hit_test_unavailable"),
        ("missing accessibility permission is explicit", replay(hit: .failure(.permission)), "accessibility_permission"),
        ("same browser hit without window geometry is blocked", replay(hit: .success(AXWindowIdentity(pid: Int32(reportedPID), rect: .null))), "target_hit_test_unavailable"),
        ("same browser other AX window is blocked", replay([dock, helper, otherChrome, chrome], hit: .success(otherChromeIdentity)), "target_window_changed"),
        ("Dock owner name cannot substitute trusted bundle ID", replay(bundle: "org.example.dock"), "target_occluded"),
        ("missing Dock owner identity is blocked", replay(bundle: nil), "target_occluded"),
        ("real Dock sized window remains blocked", replay([window(13, 1397, CGRect(x: 900, y: 700, width: 400, height: 100), layer: 20), helper, chrome]), "target_occluded"),
        ("other high layer remains blocked", replay([window(13, 1397, display, layer: 21), helper, chrome]), "target_occluded"),
        ("menu above verified Dock remains blocked", replay([window(14, 1397, reportedRect, layer: 24), dock, helper, chrome]), "target_occluded"),
        ("unrelated occluder after verified Dock remains blocked", replay([dock, foreign, helper, chrome]), "target_occluded"),
        ("ambiguous equal browser window bounds cannot resolve hit", replay([dock, helper, duplicate, chrome]), "target_hit_test_unavailable"),
        ("helper focus correctly rejects bound window", replay(focus: .success(helperIdentity), input: true), "target_window_changed"),
        ("unavailable focus blocked", replay(focus: .failure(.unavailable), input: true), "target_focus_unavailable"),
        ("missing focus permission explicit", replay(focus: .failure(.permission), input: true), "accessibility_permission"),
        ("ambiguous focused geometry blocked", replay([dock, helper, duplicate, chrome], input: true), "target_focus_unavailable"),
        ("focused geometry uniquely selects real Chrome after helper", uniqueWindowID(chromeIdentity, windows: [helper, chrome]) == 123013 ? nil : "wrong_id", nil)
    ]
    var probesCalled = 0
    let lazyResult = validateWindow(id: targetID, pid: browserPID, expected: targetRect, point: samplePoint,
        windows: [target], ignored: [], ownPID: ownPID, foregroundPID: Int32(browserPID), requireForeground: true,
        probes: WindowProbes(focused: { _ in probesCalled += 1; return .failure(.unavailable) },
            hit: { _ in probesCalled += 1; return .failure(.unavailable) },
            bundle: { _ in probesCalled += 1; return nil }, displays: { probesCalled += 1; return [] }))
    guard lazyResult == nil, probesCalled == 0 else {
        return errorString("test|unambiguous normal validation must not perform AX probes")
    }
    let cache = FrameFocusCache()
    let focusKey = FocusCacheKey(pid: reportedPID, foregroundPID: Int32(reportedPID), targetID: 123013,
        windows: [helper, chrome])
    var focusCalls = 0
    func cached(_ key: FocusCacheKey, _ at: UInt64) -> AXWindowResult {
        cache.resolve(key: key, at: at) { focusCalls += 1; return positive }
    }
    _ = cached(focusKey, 0)
    _ = cached(focusKey, 74_999_999)
    guard focusCalls == 1 else { return errorString("test|frame focus cache did not reuse unchanged young evidence") }
    _ = cached(focusKey, 75_000_000)
    guard focusCalls == 2 else { return errorString("test|frame focus cache used expired evidence") }
    let changedOrder = FocusCacheKey(pid: reportedPID, foregroundPID: Int32(reportedPID), targetID: 123013,
        windows: [chrome, helper])
    _ = cached(changedOrder, 75_000_001)
    guard focusCalls == 3 else { return errorString("test|frame focus cache ignored changed window order") }
    let changedGeometry = FocusCacheKey(pid: reportedPID, foregroundPID: Int32(reportedPID), targetID: 123013,
        windows: [helper, window(123013, reportedPID, reportedRect.offsetBy(dx: 1, dy: 0))])
    _ = cached(changedGeometry, 75_000_002)
    guard focusCalls == 4 else { return errorString("test|frame focus cache ignored changed window geometry") }
    let changedForeground = FocusCacheKey(pid: reportedPID, foregroundPID: 96549, targetID: 123013,
        windows: [helper, chrome])
    _ = cached(changedForeground, 75_000_003)
    guard focusCalls == 5 else { return errorString("test|frame focus cache ignored changed foreground PID") }
    cache.clear()
    _ = cached(changedForeground, 75_000_004)
    guard focusCalls == 6 else { return errorString("test|fresh validation did not invalidate frame cache") }
    _ = cached(changedForeground, 0)
    guard focusCalls == 7 else { return errorString("test|backwards clock reused focus cache") }
    cache.clear()
    _ = cache.resolve(key: focusKey, at: 0) { .failure(.unavailable) }
    _ = cached(focusKey, 1)
    guard focusCalls == 8 else { return errorString("test|failed AX focus was cached") }
    for (name, result, expected) in checks + replayChecks {
        let actual = result.map { String($0.split(separator: "|", maxSplits: 1)[0]) }
        if actual != expected {
            return errorString("test|window validation: \(name); expected \(expected ?? "success"), received \(actual ?? "success")")
        }
    }
    return nil
}

// Synthetic buffers only: no screen capture, permission request or input.
@_cdecl("mmp_capture_lifecycle_test")
public func mmpCaptureLifecycleTest() -> UnsafeMutablePointer<CChar>? {
    var pixelBuffer: CVPixelBuffer?
    let attributes = [kCVPixelBufferBytesPerRowAlignmentKey: 16] as CFDictionary
    guard CVPixelBufferCreate(kCFAllocatorDefault, 2, 2, kCVPixelFormatType_32BGRA,
                              attributes, &pixelBuffer) == kCVReturnSuccess,
          let pixelBuffer,
          CVPixelBufferLockBaseAddress(pixelBuffer, []) == kCVReturnSuccess,
          let base = CVPixelBufferGetBaseAddress(pixelBuffer) else {
        return errorString("test|synthetic buffer allocation failed")
    }
    let sourceStride = CVPixelBufferGetBytesPerRow(pixelBuffer)
    for row in 0..<2 {
        let bytes = base.advanced(by: row * sourceStride).assumingMemoryBound(to: UInt8.self)
        for column in 0..<2 {
            bytes[column * 4] = 30; bytes[column * 4 + 1] = 20
            bytes[column * 4 + 2] = 10; bytes[column * 4 + 3] = 255
        }
    }
    CVPixelBufferUnlockBaseAddress(pixelBuffer, [])

    let session = CaptureSession()
    session.active = true
    session.acceptFrame(status: .started, image: nil)
    guard session.error == nil, session.frame == nil, session.sequence == 0,
          session.firstFrame.wait(timeout: .now()) == .timedOut else {
        return errorString("test|status-only start was mistaken for an image or an interruption")
    }
    session.acceptFrame(status: .complete, image: pixelBuffer)
    guard session.error == nil, session.sequence == 1,
          session.firstFrame.wait(timeout: .now()) == .success else {
        return errorString("test|first complete image was not accepted")
    }
    let capturedAt = session.capturedAt
    session.acceptFrame(status: .started, image: nil)
    session.acceptFrame(status: .idle, image: nil)
    guard session.error == nil, session.frame != nil, session.sequence == 1,
          session.capturedAt == capturedAt else {
        return errorString("test|static image was lost or falsely advanced")
    }
    let pointer = Unmanaged.passUnretained(session).toOpaque()
    var metadata = [UInt64](repeating: 0, count: 7)
    var destination = [UInt8](repeating: 0, count: 16)
    let copied = destination.withUnsafeMutableBufferPointer { bytes in
        metadata.withUnsafeMutableBufferPointer { info in
            mmpCaptureCopy(pointer, bytes.baseAddress, UInt64(bytes.count), info.baseAddress)
        }
    }
    if let copied { return copied }
    guard metadata[0...3].elementsEqual([2, 2, 8, 1]), metadata[6] == 16,
          destination == Array(repeating: [UInt8(30), 20, 10, 255], count: 4).flatMap({ $0 }) else {
        return errorString("test|cached image or packed row copy is incorrect")
    }
    for status in [SCFrameStatus.blank, .suspended, .stopped] {
        session.acceptFrame(status: status, image: nil)
        session.acceptFrame(status: .idle, image: nil)
        session.acceptFrame(status: .started, image: nil)
        guard session.error?.hasPrefix("capture_interrupted|") == true, session.frame == nil else {
            return errorString("test|unavailable screen was revived without a new image")
        }
        let failure = metadata.withUnsafeMutableBufferPointer {
            mmpCaptureCopy(pointer, nil, 0, $0.baseAddress)
        }
        guard let failure else { return errorString("test|copy returned a stale unavailable image") }
        let interrupted = String(cString: failure).hasPrefix("capture_interrupted|")
        free(failure)
        guard interrupted else { return errorString("test|copy lost the capture error") }
        session.acceptFrame(status: .complete, image: pixelBuffer)
        guard session.error == nil, session.frame != nil else {
            return errorString("test|new image did not recover capture")
        }
    }
    session.acceptFrame(status: .complete, image: nil)
    guard session.error?.hasPrefix("capture_frame_missing|") == true, session.frame == nil else {
        return errorString("test|missing complete image was accepted")
    }
    return nil
}
#endif
