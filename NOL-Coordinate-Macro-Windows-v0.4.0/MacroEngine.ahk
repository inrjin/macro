; The timer advances bounded phases. No infinite polling loops, automatic Enter,
; focus activation, network requests, or code from legacy DLLs are used.
NowMs() {
    static freq := 0
    if (!freq)
        DllCall("QueryPerformanceFrequency", "Int64P", freq)
    DllCall("QueryPerformanceCounter", "Int64P", ticks)
    return ticks * 1000.0 / freq
}

BrowserForeground(hwnd := 0) {
    if (!hwnd)
        hwnd := DllCall("GetForegroundWindow", "Ptr")
    if (!hwnd || DllCall("GetAncestor", "Ptr", hwnd, "UInt", 2, "Ptr") != hwnd)
        return false
    WinGet, exe, ProcessName, % "ahk_id " hwnd
    WinGetClass, cls, % "ahk_id " hwnd
    return (exe = "chrome.exe" || exe = "msedge.exe") && cls = "Chrome_WidgetWin_1"
}

ClientSize(hwnd, ByRef width, ByRef height) {
    VarSetCapacity(rect, 16, 0)
    if (!DllCall("GetClientRect", "Ptr", hwnd, "Ptr", &rect))
        return false
    width := NumGet(rect, 8, "Int"), height := NumGet(rect, 12, "Int")
    return width > 0 && height > 0
}

ClientPoint(hwnd, x, y, ByRef sx, ByRef sy) {
    VarSetCapacity(pt, 8, 0)
    NumPut(x, pt, 0, "Int"), NumPut(y, pt, 4, "Int")
    if (!DllCall("ClientToScreen", "Ptr", hwnd, "Ptr", &pt))
        return false
    sx := NumGet(pt, 0, "Int"), sy := NumGet(pt, 4, "Int")
    return true
}

PointBelongs(hwnd, sx, sy) {
    ; POINT is passed by value (two signed 32-bit coordinates packed into Int64).
    packed := (sx & 0xFFFFFFFF) | (sy << 32)
    under := DllCall("WindowFromPoint", "Int64", packed, "Ptr")
    return under && DllCall("GetAncestor", "Ptr", under, "UInt", 2, "Ptr") = hwnd
}

BindBrowser() {
    global Bound, State
    humanRebind := State.running && State.phase = "human"
    if ((State.running && !humanRebind))
        return
    hwnd := DllCall("GetForegroundWindow", "Ptr")
    if (!BrowserForeground(hwnd) || !ClientSize(hwnd, w, h)) {
        SetStatus("Chrome/Edge 예매 창을 활성화한 뒤 F7을 누르세요.")
        return
    }
    WinGet, pid, PID, % "ahk_id " hwnd
    Bound := {hwnd:hwnd, pid:pid, width:w, height:h}
    Put("Verified", 0)
    if (humanRebind) {
        cfg := ReadRunConfig(false, reason)
        if (!IsObject(cfg)) {
            StopRun("새 예매 창 설정 확인 필요: " reason)
            return
        }
        State.hwnd := hwnd, State.cfg := cfg, State.cfg.ReadyEnabled := false
        SetStatus("예매 창 재연결 완료. 캡챠 입력 후 F6으로 재개하세요.")
        return
    }
    SetStatus("대상 창 연결됨 (" w " × " h "). F8로 좌표를 따고 좌표 확인을 체크하세요.")
}

BindingOK(hwnd, ByRef reason) {
    global Bound
    if (!Bound.hwnd || hwnd != Bound.hwnd) {
        reason := "F7로 연결한 브라우저 창에서만 시작할 수 있습니다."
        return false
    }
    if (DllCall("GetForegroundWindow", "Ptr") != hwnd || !BrowserForeground(hwnd)) {
        reason := "대상 브라우저가 활성 창이 아닙니다."
        return false
    }
    WinGet, pid, PID, % "ahk_id " hwnd
    WinGet, minmax, MinMax, % "ahk_id " hwnd
    if (pid != Bound.pid || minmax = -1 || !ClientSize(hwnd, w, h)
        || w != Bound.width || h != Bound.height) {
        reason := "대상 프로세스 또는 창 크기가 바뀌었습니다. 다시 F7로 연결하세요."
        return false
    }
    return true
}

StartRun(cancelMode := false, scheduled := false) {
    global State, Bound, Clock
    if (State.running)
        return false
    hwnd := DllCall("GetForegroundWindow", "Ptr")
    if (!Get("Verified") || !BindingOK(hwnd, reason)) {
        SetStatus("시작 거절: " (reason != "" ? reason : "좌표·색상 재확인이 필요합니다."))
        return false
    }
    cfg := ReadRunConfig(cancelMode, reason)
    if (!IsObject(cfg)) {
        SetStatus("시작 거절: " reason)
        return false
    }
    if (scheduled && (!Clock.due || NowMs()-Clock.measured > 600000 || Clock.due <= NowMs() || Clock.identity != ClockIdentity())) {
        SetStatus("예약 전 시각 API를 다시 측정하세요. 측정은 10분간 유효합니다.")
        return false
    }
    now := NowMs(), extra := 0
    for _, step in cfg.steps
        extra += step.timeout
    State := {running:true, cfg:cfg, hwnd:hwnd, phase:"search", due:now
        , started:now, deadline:now + cfg.SearchTimeout + extra + 3 * cfg.PreDelay + 10000
        , searchDeadline:now + cfg.SearchTimeout, selected:[], rejected:[], failures:0, zone:1, step:0}
    if (cfg.UseDate && !cancelMode) {
        State.phase := "pre", State.preIndex := 1
        State.searchDeadline += 3 * cfg.PreDelay
    } else if (cancelMode && cfg.UseZones) {
        State.phase := "zoneMove"
    }
    if (!cancelMode && cfg.UseRegion && !cfg.UseDate)
        State.phase := "region"
    if (scheduled) {
        if (!ValidPoint(cfg, "Open", reason)) {
            State.running := false
            SetStatus(reason)
            return false
        }
        State.phase := "opening", State.due := Clock.due
        State.deadline := Clock.due + 900000 + cfg.SearchTimeout + extra
    }
    DllCall("winmm\timeBeginPeriod", "UInt", 1)
    SetEditing(false)
    SetStatus(scheduled ? "예약 대기 · F3 중지" : "좌석 탐색 준비 · F3 중지")
    SetTimer, EngineTick, 5
    return true
}

Guard() {
    global State
    if (!State.running)
        return false
    if (!BindingOK(State.hwnd, reason)) {
        StopRun("중지: " reason)
        return false
    }
    if (NowMs() > State.deadline) {
        StopRun("실패: 전체 실행 제한시간 초과")
        return false
    }
    return true
}

SafeClick(x, y) {
    global State, Bound
    if (!Guard())
        return false
    if (x < 0 || y < 0 || x >= Bound.width || y >= Bound.height
        || !ClientPoint(State.hwnd, x, y, sx, sy) || !PointBelongs(State.hwnd, sx, sy)) {
        StopRun("실패: 클릭 좌표가 대상 창 밖이거나 다른 창에 가려져 있습니다.")
        return false
    }
    ; Check again immediately before input. Windows input still has a small
    ; unavoidable focus race; this is not a DOM-bound browser action.
    if (!Guard())
        return false
    MouseClick, Left, %sx%, %sy%, 1, 0
    return State.running
}

ReadPixel(x, y, ByRef color) {
    global State
    if (!Guard() || !ClientPoint(State.hwnd, x, y, sx, sy)
        || !PointBelongs(State.hwnd, sx, sy)) {
        if (State.running)
            StopRun("실패: 색상 확인 위치가 다른 창에 가려져 있습니다.")
        return false
    }
    PixelGetColor, color, %sx%, %sy%
    if (ErrorLevel) {
        StopRun("실패: 픽셀 색상을 읽을 수 없습니다.")
        return false
    }
    return true
}

ColorMatches(actual, wanted, tolerance) {
    return Abs((actual & 255) - (wanted & 255)) <= tolerance
        && Abs(((actual >> 8) & 255) - ((wanted >> 8) & 255)) <= tolerance
        && Abs(((actual >> 16) & 255) - ((wanted >> 16) & 255)) <= tolerance
}

Tick() {
    global State
    if (State.running && State.phase = "human" && NowMs() <= State.deadline
        && !BindingOK(State.hwnd, reason)) {
        if (!State.waitingWindow)
            SetStatus("캡챠 직접 입력 · 새 예매 창이면 그 창에서 F7 재연결 후 F6")
        State.waitingWindow := true
        return
    }
    State.waitingWindow := false
    if (!Guard())
        return
    now := NowMs(), cfg := State.cfg
    if (VisualConflict(now))
        return
    if (now < State.due)
        return
    if (CoordinateTick(now))
        return
    if (State.phase = "pre") {
        names := ["Date", "Round", "Next"], id := names[State.preIndex]
        if (!SafeClick(cfg[id "X"], cfg[id "Y"]))
            return
        State.preIndex += 1, State.due := now + cfg.PreDelay
        if (State.preIndex > 3)
            State.phase := cfg.UseRegion ? "region" : "search"
        return
    }
    if (State.phase = "zoneMove") {
        name := "Zone" State.zone
        if (!SafeClick(cfg[name "X"], cfg[name "Y"]))
            return
        State.phase := "search", State.due := now + cfg.ZoneDelay
        return
    }
    if (State.phase = "search") {
        if (now >= State.searchDeadline) {
            StopRun("실패: 좌석 탐색 제한시간 초과. 입력을 더 보내지 않습니다.")
            return
        }
        found := FindSeat(px, py)
        if (!State.running)
            return
        if (found) {
            State.candidate := {px:px, py:py, x:px, y:py, zone:State.zone}
            id := cfg.cancelMode ? "Zone" State.zone : "Seat"
            tx := px+cfg.SeatOffset, ty := py+cfg.SeatOffset
            if (cfg.SeatOffset && tx <= cfg[id "BRX"] && ty <= cfg[id "BRY"]
                && ReadPixel(tx,ty,offsetColor) && ColorMatches(offsetColor,SeatColor(),cfg.Tolerance))
                State.candidate.x := tx, State.candidate.y := ty
            State.phase := "seatClick", State.due := now + cfg.ClickDelay
        } else if (cfg.cancelMode && cfg.UseZones) {
            State.zone := State.zone = 1 ? 2 : 1
            State.phase := "zoneMove", State.due := now + cfg.PollDelay
        } else {
            State.due := now + cfg.PollDelay
        }
        return
    }
    if (State.phase = "seatClick") {
        p := State.candidate
        if (!ReadPixel(p.px, p.py, color))
            return
        if (!ColorMatches(color, SeatColor(), cfg.Tolerance)) {
            State.phase := "search", State.due := now + cfg.PollDelay
            return
        }
        if (!ReadPixel(p.x,p.y,clickColor))
            return
        if (!ColorMatches(clickColor,SeatColor(),cfg.Tolerance)) {
            State.phase := "search", State.due := now+cfg.PollDelay
            return
        }
        if (!SafeClick(p.x, p.y))
            return
        State.phase := "confirmSeat", State.due := now + cfg.PollDelay
        State.confirmDeadline := now + cfg.SeatConfirmTimeout
        return
    }
    if (State.phase = "confirmSeat") {
        p := State.candidate
        if (!ReadPixel(p.px, p.py, color))
            return
        if (!ColorMatches(color, SeatColor(), cfg.Tolerance)) {
            State.selected.Push(p)
            State.phase := State.selected.Length() >= cfg.WantedSeats ? "completeClick" : "search"
            State.due := now + cfg.PollDelay
        } else if (now >= State.confirmDeadline) {
            StopRun("실패: 좌석 클릭 후 색상 변화가 확인되지 않았습니다.")
        } else {
            State.due := now + cfg.PollDelay
        }
        return
    }
    if (State.phase = "completeClick") {
        for _, p in State.selected {
            if (!ReadPixel(p.px, p.py, color))
                return
            if (ColorMatches(color, SeatColor(), cfg.Tolerance)) {
                StopRun("실패: 선택한 좌석이 원래 색으로 돌아왔습니다.")
                return
            }
        }
        id := cfg.cancelMode ? "CancelDone" : "Done"
        if (!SafeClick(cfg[id "X"], cfg[id "Y"]))
            return
        State.phase := cfg.NextEnabled ? "resultWait" : "nextStep", State.due := now
        State.resultDeadline := now + cfg.ResultWait
        return
    }
    if (State.phase = "nextStep") {
        State.step += 1
        if (State.step > cfg.steps.Length()) {
            StopRun("완료: 지정한 클릭을 1회 실행했습니다. 사이트 결과는 직접 확인하세요.")
            SoundBeep, 900, 100
            return
        }
        step := cfg.steps[State.step]
        State.phase := "stepWait", State.due := now + step.delay
        State.stepDeadline := now + step.timeout
        SetStatus("후속 단계 " State.step ": " step.name " · F3 중지")
        return
    }
    if (State.phase = "stepWait") {
        step := cfg.steps[State.step]
        if (now >= State.stepDeadline) {
            StopRun("실패: 후속 단계 제한시간 초과 — " step.name)
            return
        }
        if (step.wait) {
            if (!ReadPixel(step.waitX, step.waitY, color))
                return
            if (!ColorMatches(color, step.color, step.tolerance)) {
                State.due := now + cfg.PollDelay
                return
            }
        }
        if (!SafeClick(step.x, step.y))
            return
        State.phase := "nextStep", State.due := now
    }
}

SeatColor() {
    global State
    return State.cfg.cancelMode ? State.cfg["Zone" State.zone "Color"] : State.cfg.SeatColor
}

FindSeat(ByRef px, ByRef py) {
    global State
    cfg := State.cfg, id := cfg.cancelMode ? "Zone" State.zone : "Seat"
    rects := [{x1:cfg[id "TLX"], y1:cfg[id "TLY"], x2:cfg[id "BRX"], y2:cfg[id "BRY"]}]
    ; Exclude an adjustable square around every already-clicked seat. This is
    ; spatial deduplication, not an adjacent-seat/seat-identity guarantee.
    excluded := State.selected.Clone()
    for _, rejected in State.rejected
        if (rejected.zone = State.zone)
            excluded.Push(rejected)
    for _, p in excluded {
        next := [], gap := cfg.SeatGap
        for _, r in rects {
            left := Max(r.x1, p.x-gap), right := Min(r.x2, p.x+gap)
            top := Max(r.y1, p.y-gap), bottom := Min(r.y2, p.y+gap)
            if (left > right || top > bottom) {
                next.Push(r)
                continue
            }
            AddRect(next, r.x1, r.y1, r.x2, top-1)
            AddRect(next, r.x1, bottom+1, r.x2, r.y2)
            AddRect(next, r.x1, top, left-1, bottom)
            AddRect(next, right+1, top, r.x2, bottom)
        }
        rects := next
    }
    color := SeatColor(), tolerance := cfg.Tolerance
    for _, r in rects {
        if (!Guard())
            return false
        if (!ClientPoint(State.hwnd, r.x1, r.y1, x1, y1)
            || !ClientPoint(State.hwnd, r.x2, r.y2, x2, y2)) {
            StopRun("실패: 좌석 범위 변환 오류")
            return false
        }
        PixelSearch, sx, sy, %x1%, %y1%, %x2%, %y2%, %color%, %tolerance%, Fast
        result := ErrorLevel
        if (result = 2) {
            StopRun("실패: PixelSearch 오류")
            return false
        }
        if (result = 0) {
            if (!PointBelongs(State.hwnd, sx, sy)) {
                StopRun("실패: 검색 결과가 다른 창에 가려져 있습니다.")
                return false
            }
            px := r.x1 + sx - x1, py := r.y1 + sy - y1
            return true
        }
    }
    return false
}

AddRect(ByRef rects, x1, y1, x2, y2) {
    if (x1 <= x2 && y1 <= y2)
        rects.Push({x1:x1, y1:y1, x2:x2, y2:y2})
}

StopRun(message) {
    global State
    SetTimer, EngineTick, Off
    State.running := false
    DllCall("winmm\timeEndPeriod", "UInt", 1)
    SetEditing(true)
    SetStatus(message)
}
