BuildCoordinateTabs() {
    Gui, Main:Tab, 4
    Gui, Main:Add, Text, x40 y118 w850, F4 예약 예매 클릭 → 캡챠 직접 입력 → F6 구역/좌석 시작. F1 즉시 일반 / F2 취소표 / F3 중지.
    LabeledEdit("ClockURL", "서버 시각 API (HTTPS)", 40, 158, 570, "")
    LabeledEdit("ClockMode", "형식: date / milliseconds / seconds / iso", 40, 198, 180, "milliseconds")
    LabeledEdit("ClockPath", "JSON 필드 경로 · 예: data.timestamp", 40, 238, 250, "")
    LabeledEdit("SaleTime", "한국시간 YYYY-MM-DD HH:mm:ss.fff", 40, 278, 280, "")
    AddButton("ClockSyncButton", "설정 저장 + 시각 측정 (3회)", 635, 278, 270)
    PointRow("Open", "예약 예매 버튼", 40, 322)
    AddCheck("UseRegion", 480, 322, 410, "F1/F6 시작 때 지정 구역 1회 클릭", 1)
    PointRow("Region", "캡챠 이후 구역", 40, 362)
    LabeledEdit("RegionWait", "구역 클릭 후 최소 대기 ms", 480, 362, 80, "150")
    AddCheck("ReadyEnabled", 40, 408, 850, "F6 대신 준비 픽셀로 재개 · 예약 클릭 후 불일치 → 일치 2회 확인", 0)
    PointRow("Ready", "지도 준비 픽셀", 40, 448)
    LabeledEdit("ReadyColor", "준비 색 (BGR)", 480, 448, 120, "")
    Gui, Main:Add, Text, x40 y500 w850 h80, 기본은 캡챠 입력을 마친 뒤 F6입니다. F6은 사이트에 키를 보내지 않고 매크로만 재개합니다.`n시각 API 이외 통신·확장·CSS가 필요 없습니다. API의 RTT/정밀도 오차는 측정 후 표시됩니다.`n취소표는 F2로 시작하며 기존 1구역/두 구역 왕복·색상·후속 클릭을 모두 유지합니다.
    Gui, Main:Tab, 5
    AddCheck("ConflictEnabled", 40, 122, 850, "경합 안내의 두 픽셀 색이 모두 일치하면 닫고 다른 좌석 재시도", 0)
    PointRow("ConflictA", "안내 표식 A", 40, 166)
    LabeledEdit("ConflictAColor", "표식 A 색 (BGR)", 480, 166, 120, "")
    PointRow("ConflictB", "안내 표식 B", 40, 204)
    LabeledEdit("ConflictBColor", "표식 B 색 (BGR)", 480, 204, 120, "")
    PointRow("Dismiss", "경합 안내 닫기", 40, 242)
    LabeledEdit("RetryLimit", "최대 경합 재시도", 480, 242, 80, "30")
    AddCheck("NextEnabled", 40, 294, 850, "선택 완료 후 가격 화면 준비 색을 기다림 · 후속 클릭 사용 시 필수", 0)
    PointRow("Success", "가격 화면 표식", 40, 335)
    LabeledEdit("SuccessColor", "가격 화면 준비 색 (BGR)", 480, 335, 120, "")
    LabeledEdit("ResultWait", "선택 완료 후 결과 제한 ms", 40, 380, 100, "5000")
    Gui, Main:Add, Text, x40 y438 w850 h135, 오류 문구/서버 응답을 읽지 않습니다. 경합 안내가 구별되는 두 위치의 색을 직접 지정하세요.`n일반 안내창과 같은 표식을 사용하면 다른 안내를 오인할 수 있습니다.`n색 변화는 화면 반응이며 좌석 확보 확정이 아닙니다. 모호한 결과는 제한시간 후 중지합니다.`n실패한 좌석은 해당 실행에서 제외합니다. 취소표를 새로 시작하면 제외 목록도 초기화합니다.`n후속 클릭에는 가격 + 버튼 → 1매 예매하기 등을 화면별 대기 픽셀과 함께 등록하세요.
}

ValidateCoordinates(ByRef cfg, ByRef reason) {
    for key, bounds in {RegionWait:[0,10000], ResultWait:[100,30000], RetryLimit:[1,100]} {
        if (!ValidInt(cfg[key], bounds[1], bounds[2])) {
            reason := key " 설정 범위를 확인하세요."
            return false
        }
        cfg[key] += 0
    }
    if (cfg.UseRegion && !cfg.cancelMode && !ValidPoint(cfg,"Region",reason))
        return false
    groups := []
    if (cfg.ReadyEnabled)
        groups.Push("Ready")
    if (cfg.ConflictEnabled) {
        groups.Push("ConflictA"), groups.Push("ConflictB")
        if (!ValidPoint(cfg,"Dismiss",reason))
            return false
        if (cfg.ConflictAX = cfg.ConflictBX && cfg.ConflictAY = cfg.ConflictBY) {
            reason := "경합 표식 A/B는 서로 다른 위치여야 합니다."
            return false
        }
    }
    if (cfg.NextEnabled)
        groups.Push("Success")
    for _, id in groups {
        if (!ValidPoint(cfg,id,reason) || !ValidColor(cfg[id "Color"])) {
            reason := id " 좌표/색을 F8로 지정하세요."
            return false
        }
    }
    if (cfg.FollowEnabled && cfg.steps.Length() && !cfg.NextEnabled) {
        reason := "후속 클릭 전 가격 화면 준비 색을 설정하세요."
        return false
    }
    return true
}

Probe(id) {
    global State
    c := State.cfg
    return ReadPixel(c[id "X"], c[id "Y"], actual) && ColorMatches(actual,c[id "Color"],c.Tolerance)
}

ResumeCoordinates() {
    global State
    if (!State.running || State.phase != "human" || !Guard())
        return
    State.phase := State.cfg.UseRegion ? "region" : "search"
    State.searchDeadline := NowMs()+State.cfg.SearchTimeout
    State.due := NowMs()
    SetStatus("구역/좌석 탐색 시작 · F3 중지")
}

CoordinateTick(now) {
    global State
    c := State.cfg
    if (State.phase = "opening") {
        if (SafeClick(c.OpenX,c.OpenY)) {
            State.phase := "human", State.due := now, State.readyMiss := false, State.readyCount := 0
            SetStatus("예매 버튼 1회 클릭. 캡챠 직접 입력 후 F6 · 준비 픽셀 설정 시 자동 재개")
        }
        return true
    }
    if (State.phase = "human") {
        if (c.ReadyEnabled) {
            matched := Probe("Ready")
            if (!matched)
                State.readyMiss := true, State.readyCount := 0
            else if (State.readyMiss)
                State.readyCount += 1
            if (State.readyCount >= 2)
                ResumeCoordinates()
        }
        State.due := now+c.PollDelay
        return true
    }
    if (State.phase = "region") {
        if (SafeClick(c.RegionX,c.RegionY))
            State.phase := "search", State.due := now+c.RegionWait, State.searchDeadline := now+c.SearchTimeout
        return true
    }
    if (State.phase = "dismissWait") {
        if (!(Probe("ConflictA") && Probe("ConflictB"))) {
            State.phase := "search", State.due := now+c.PollDelay
        } else if (now >= State.resultDeadline)
            StopRun("경합 안내가 닫히지 않아 중지했습니다.")
        return true
    }
    if (State.phase = "resultWait") {
        if (Probe("Success"))
            State.phase := "nextStep", State.due := now
        else if (now >= State.resultDeadline)
            StopRun("다음 화면을 확인하지 못했습니다. 화면을 직접 확인하세요.")
        return true
    }
    return false
}

VisualConflict(now) {
    global State
    if (!State.cfg.ConflictEnabled || !IsObject(State.candidate)
        || !(State.phase = "confirmSeat" || State.phase = "completeClick" || State.phase = "resultWait" || State.phase = "search"))
        return false
    if (!(Probe("ConflictA") && Probe("ConflictB")))
        return false
    for _, chosen in State.selected {
        if (chosen.x != State.candidate.x || chosen.y != State.candidate.y) {
            StopRun("여러 좌석 선택 중 경합입니다. 좌석 상태를 직접 확인하세요.")
            return true
        }
    }
    State.failures += 1
    if (State.failures > State.cfg.RetryLimit) {
        StopRun("경합 재시도 제한에 도달했습니다.")
        return true
    }
    p := State.candidate
    State.rejected.Push(p)
    remaining := []
    for _, selected in State.selected
        if (selected.x != p.x || selected.y != p.y || selected.zone != p.zone)
            remaining.Push(selected)
    State.selected := remaining
    if (SafeClick(State.cfg.DismissX,State.cfg.DismissY)) {
        State.phase := "dismissWait", State.due := now
        State.resultDeadline := now+State.cfg.ResultWait
    }
    return true
}

SyncClock() {
    global State, ClockPID, Clock, DataDir, ClockRequestIdentity
    if (State.running || ClockPID)
        return
    if (!SaveSettings())
        return
    Clock := {}, ClockRequestIdentity := ClockIdentity()
    output := DataDir "\clock.ini"
    FileDelete, %output%
    command := "powershell.exe -NoProfile -ExecutionPolicy RemoteSigned -File " Chr(34) A_ScriptDir "\ClockSync.ps1" Chr(34)
    Run, %command%, %A_ScriptDir%, Hide UseErrorLevel, ClockPID
    if (ErrorLevel) {
        ClockPID := 0
        SetStatus("시각 측정 프로그램을 시작하지 못했습니다.")
        return
    }
    SetStatus("등록한 시각 API를 3회 측정 중입니다.")
    SetTimer, ClockPoll, 100
}

PollClock() {
    global ClockPID, Clock, DataDir, ClockRequestIdentity
    Process, Exist, %ClockPID%
    if (ErrorLevel)
        return
    SetTimer, ClockPoll, Off
    ClockPID := 0, path := DataDir "\clock.ini"
    if (ReadSetting(path,"Clock","OK") != 1) {
        SetStatus("시각 측정 실패: " ReadSetting(path,"Clock","Error","응답 없음"))
        return
    }
    Clock := {due:ReadSetting(path,"Clock","Due")+0, measured:ReadSetting(path,"Clock","Measured")+0, identity:ClockRequestIdentity}
    SetStatus("시각 보정 완료 · RTT " ReadSetting(path,"Clock","RTT") "ms · 추정 ±" ReadSetting(path,"Clock","Uncertainty") "ms. 브라우저에서 F4 예약 대기.")
}

ClockIdentity() {
    return Get("ClockURL") "|" Get("ClockMode") "|" Get("ClockPath") "|" Get("SaleTime")
}
