BuildGui() {
    global C, Buttons, CaptureTargets
    Gui, Main:New, +HwndMainHwnd
    Gui, Main:Font, s9, Segoe UI
    Gui, Main:Add, Text, x20 y12 w920 h34, F7 대상 브라우저 연결 → 좌표·색상 설정/확인 → 브라우저에서 F1(일반) / F2(취소표). F3은 재개 없는 긴급 중지.
    AddCheck("Verified", 20, 48, 390, "현재 창 배치에서 모든 좌표·색상을 확인했습니다", 0)
    Gui, Main:Add, Tab2, x20 y84 w920 h530, 일반 티켓팅|취소표|선택 완료 뒤 추가 클릭|예약·구역 진입|경합·전환 감지
    Gui, Main:Tab, 1
    AddCheck("UseDate", 40, 122, 370, "F1 시작 시 날짜 → 회차 → 다음 단계 클릭", 0)
    PointRow("Date", "날짜", 40, 160)
    PointRow("Round", "회차", 40, 194)
    PointRow("Next", "다음 단계", 40, 228)
    PointRow("SeatTL", "좌석 범위 ↖", 40, 274)
    PointRow("SeatBR", "좌석 범위 ↘", 40, 308)
    PointRow("Done", "좌석 선택 완료", 40, 342)
    LabeledEdit("SeatColor", "좌석 색 (BGR)", 40, 384, 145, "")
    LabeledEdit("Tolerance", "색 허용 오차 0~255", 40, 422, 90, "3")
    Gui, Main:Add, Text, x40 y467 w390 h110, 모든 좌표는 브라우저 클라이언트 기준 X/Y입니다.`nF8은 아래 선택한 항목에 현재 마우스 위치를 저장합니다.`n색은 0xBBGGRR 형식이며 F8로 직접 채집할 수 있습니다.`n원본 INI 좌표는 창 기준이므로 반드시 다시 설정하세요.
    LabeledEdit("SeatCount", "좌석 수 1~4 (연석 보장 없음)", 480, 145, 85, "1")
    LabeledEdit("SeatDelay", "좌석 클릭 전 지연 (ms)", 480, 183, 85, "0")
    LabeledEdit("SeatConfirmTimeout", "클릭 뒤 색 변화 제한 (ms)", 480, 221, 85, "1500")
    LabeledEdit("SearchTimeout", "좌석 탐색 제한 (ms)", 480, 259, 85, "30000")
    LabeledEdit("PollDelay", "검색 간격 (ms, 최소 1)", 480, 297, 85, "8")
    LabeledEdit("SeatOffset", "발견 픽셀 클릭 오프셋 X/Y", 480, 335, 85, "0")
    LabeledEdit("SeatGap", "같은 좌석 제외 반경 (px)", 480, 373, 85, "12")
    LabeledEdit("PreDelay", "날짜/회차/다음 클릭 간격 (ms)", 480, 411, 85, "700")
    Gui, Main:Add, Text, x480 y464 w420 h100, 좌석 클릭 뒤 검색 픽셀의 색이 바뀌어야 진행합니다.`n색 변화만으로 실제 확보를 보장하지는 않습니다.`n타임아웃·포커스 변경·창 크기 변경 시 중지합니다.`n후속 클릭까지 끝나면 중지하며 Enter를 보내지 않습니다.
    Gui, Main:Tab, 2
    AddCheck("UseZones", 40, 124, 410, "두 구역을 번갈아 클릭 (끄면 1구역 화면만 검색)", 0)
    PointRow("Zone1", "1구역 이동", 40, 165)
    PointRow("Zone1TL", "1구역 범위 ↖", 40, 202)
    PointRow("Zone1BR", "1구역 범위 ↘", 40, 239)
    LabeledEdit("Zone1Color", "1구역 색 (BGR)", 40, 279, 140, "")
    PointRow("Zone2", "2구역 이동", 480, 165)
    PointRow("Zone2TL", "2구역 범위 ↖", 480, 202)
    PointRow("Zone2BR", "2구역 범위 ↘", 480, 239)
    LabeledEdit("Zone2Color", "2구역 색 (BGR)", 480, 279, 140, "")
    PointRow("CancelDone", "선택 완료", 40, 335)
    LabeledEdit("CancelDelay", "좌석 클릭 전 지연 (ms)", 40, 389, 85, "0")
    LabeledEdit("ZoneDelay", "구역 이동 후 대기 (ms)", 480, 389, 85, "500")
    LabeledEdit("CancelTimeout", "취소표 탐색 제한 ms · 기본 1시간", 40, 430, 110, "3600000")
    Gui, Main:Add, Text, x40 y478 w850 h100, F2는 원본과 같이 1석을 찾습니다. 방향키 자동 입력은 사용하지 않습니다.`n구역 이동을 켜면 두 이동 좌표와 두 검색 범위/색을 모두 설정해야 합니다.`n검색 간격·색 허용 오차·선택 뒤 확인 제한은 일반 탭과 공유합니다.`n후속 클릭 목록은 F1/F2 모두에 적용됩니다.
    Gui, Main:Tab, 3
    AddCheck("FollowEnabled", 40, 120, 830, "좌석 선택 완료 뒤 아래 목록 실행 (등록한 활성 단계만 각 1회)", 0)
    Gui, Main:Add, ListView, x40 y150 w870 h180 hwndh gStepListEvent Grid -Multi, 순서|사용|이름|클릭 X/Y|지연 ms|제한 ms|색 대기
    C.StepList := h
    AddButton("UpStepButton", "위로", 40, 342, 75)
    AddButton("DownStepButton", "아래로", 125, 342, 75)
    AddButton("DeleteStepButton", "삭제", 210, 342, 75)
    Gui, Main:Add, Text, x310 y347 w600 h24, 목록을 더블클릭하여 편집 → '선택 단계 수정'. 순서는 위/아래 버튼으로 변경.
    AddCheck("StepEnabled", 40, 382, 100, "단계 사용", 1)
    Gui, Main:Add, Text, x155 y386 w55, 이름
    AddEdit("StepName", 205, 381, 230, "추가 클릭")
    Gui, Main:Add, Text, x460 y386 w100, 클릭 X / Y
    AddEdit("StepX", 565, 381, 80, "")
    AddEdit("StepY", 655, 381, 80, "")
    LabeledEdit("StepDelay", "클릭 전 최소 대기 (ms)", 40, 420, 85, "500")
    LabeledEdit("StepTimeout", "단계 전체 제한 (ms)", 480, 420, 85, "5000")
    AddCheck("StepWait", 40, 464, 175, "특정 픽셀 색 대기", 0)
    Gui, Main:Add, Text, x215 y469 w60, X / Y
    AddEdit("StepWaitX", 270, 464, 70, "")
    AddEdit("StepWaitY", 350, 464, 70, "")
    Gui, Main:Add, Text, x450 y469 w85, BGR 색
    AddEdit("StepColor", 535, 464, 115, "")
    Gui, Main:Add, Text, x670 y469 w75, 허용 오차
    AddEdit("StepTolerance", 750, 464, 65, "3")
    AddButton("AddStepButton", "새 단계 추가", 40, 515, 150)
    AddButton("UpdateStepButton", "선택 단계 수정", 205, 515, 160)
    Gui, Main:Add, Text, x40 y558 w850 h40, 색 대기는 지정 픽셀이 해당 색일 때 진행합니다. 대기+클릭은 단계 제한 안에 끝나야 합니다.`n전체 추가 클릭은 기본 꺼짐이며 실제 버튼 좌표와 화면 전환 대기는 직접 지정해야 합니다.
    BuildCoordinateTabs()
    Gui, Main:Tab
    Gui, Main:Add, Text, x20 y631 w110, F8 채집할 항목
    CaptureTargets := []
    choices := ""
    labels := [["Date", "날짜"], ["Round", "회차"], ["Next", "다음 단계"], ["SeatTL", "일반 범위 ↖"]
        , ["SeatBR", "일반 범위 ↘"], ["Done", "일반 선택 완료"], ["SeatColor", "일반 좌석 색"]
        , ["Zone1", "1구역 이동"], ["Zone1TL", "1구역 범위 ↖"], ["Zone1BR", "1구역 범위 ↘"]
        , ["Zone1Color", "1구역 색"], ["Zone2", "2구역 이동"], ["Zone2TL", "2구역 범위 ↖"]
        , ["Zone2BR", "2구역 범위 ↘"], ["Zone2Color", "2구역 색"], ["CancelDone", "취소표 선택 완료"]
        , ["Step", "편집 중 후속 클릭 좌표"], ["StepWait", "편집 중 후속 대기 좌표+색"]]
    for _, pair in [["Open","예약 예매 버튼"],["Region","캡챠 뒤 구역"],["Ready","구역 준비 픽셀+색"],["ConflictA","경합 안내 픽셀 A+색"],["ConflictB","경합 안내 픽셀 B+색"],["Dismiss","경합 안내 닫기"],["Success","가격 화면 준비 픽셀+색"]]
        labels.Push(pair)
    for _, pair in labels {
        CaptureTargets.Push(pair[1])
        choices .= (choices = "" ? "" : "|") pair[2]
    }
    Gui, Main:Add, DropDownList, x135 y625 w250 hwndh AltSubmit Choose1, %choices%
    C.CaptureChoice := h
    AddButton("SaveButton", "설정 저장", 405, 624, 105)
    AddButton("LoadButton", "불러오기", 520, 624, 105)
    AddButton("ImportButton", "원본 6INI 가져오기", 635, 624, 170)
    Gui, Main:Add, Button, x820 y624 w120 h29 gStopButton, 중지 (F3)
    Gui, Main:Add, Text, x20 y668 w920 h58 hwndh, 준비: 실제 예매 창에서 F7을 누르고 좌표를 설정하세요.
    C.Status := h
    RefreshSteps()
}

AddEdit(key, x, y, width, value) {
    global C
    Gui, Main:Add, Edit, % "x" x " y" y " w" width " h24 hwndh", %value%
    C[key] := h
}

AddCheck(key, x, y, width, text, checked := 0) {
    global C
    Gui, Main:Add, CheckBox, % "x" x " y" y " w" width " h25 hwndh Checked" checked, %text%
    C[key] := h
}

AddButton(label, text, x, y, width) {
    global Buttons
    Gui, Main:Add, Button, % "x" x " y" y " w" width " h29 hwndh g" label, %text%
    Buttons.Push(h)
}

PointRow(id, text, x, y) {
    Gui, Main:Add, Text, % "x" x " y" y+4 " w145", %text%
    AddEdit(id "X", x+155, y, 85, "")
    AddEdit(id "Y", x+250, y, 85, "")
}

LabeledEdit(key, text, x, y, width, value) {
    Gui, Main:Add, Text, % "x" x " y" y+4 " w240", %text%
    AddEdit(key, x+245, y, width, value)
}

Get(key) {
    global C
    GuiControlGet, value, Main:, % C[key]
    return Trim(value)
}

Put(key, value) {
    global C
    GuiControl, Main:, % C[key], %value%
}

SetStatus(message) {
    Put("Status", message)
}

SetEditing(enabled) {
    global C, Buttons
    command := enabled ? "Main:Enable" : "Main:Disable"
    for key, handle in C {
        if (key != "Status")
            GuiControl, %command%, %handle%
    }
    for _, handle in Buttons
        GuiControl, %command%, %handle%
}

CaptureField() {
    global Bound, State, CaptureTargets
    if (State.running)
        return
    hwnd := DllCall("GetForegroundWindow", "Ptr")
    if (!BindingOK(hwnd, reason)) {
        SetStatus(reason)
        return
    }
    MouseGetPos, sx, sy
    if (!PointBelongs(hwnd, sx, sy)) {
        SetStatus("마우스를 연결한 브라우저 위로 옮기세요.")
        return
    }
    VarSetCapacity(pt, 8, 0)
    NumPut(sx, pt, 0, "Int"), NumPut(sy, pt, 4, "Int")
    if (!DllCall("ScreenToClient", "Ptr", hwnd, "Ptr", &pt))
        return
    x := NumGet(pt, 0, "Int"), y := NumGet(pt, 4, "Int")
    if (x < 0 || y < 0 || x >= Bound.width || y >= Bound.height)
        return
    id := CaptureTargets[Get("CaptureChoice")]
    if (InStr(id, "Color") || id = "StepWait" || id = "Ready" || id = "ConflictA" || id = "ConflictB" || id = "Success") {
        PixelGetColor, color, %sx%, %sy%
        if (ErrorLevel) {
            SetStatus("색상을 읽지 못했습니다.")
            return
        }
        Put(id = "StepWait" ? "StepColor" : InStr(id,"Color") ? id : id "Color", color)
    }
    if (!InStr(id, "Color")) {
        Put(id "X", x), Put(id "Y", y)
    }
    Put("Verified", 0)
    SetStatus("F8 저장: " id " (" x ", " y "). 모든 설정 후 좌표 확인을 체크하세요.")
}

ValidInt(value, low, high) {
    return RegExMatch(value, "^\d+$") && value >= low && value <= high
}

ValidColor(value) {
    return RegExMatch(value, "i)^0x[0-9a-f]{6}$") || ValidInt(value, 0, 16777215)
}

ValidPoint(cfg, id, ByRef reason) {
    global Bound
    if (!ValidInt(cfg[id "X"], 0, Bound.width-1) || !ValidInt(cfg[id "Y"], 0, Bound.height-1)) {
        reason := id ": 클라이언트 범위 안의 정수 X/Y를 입력하세요."
        return false
    }
    return true
}

ValidRect(cfg, id, ByRef reason) {
    if (!ValidPoint(cfg, id "TL", reason) || !ValidPoint(cfg, id "BR", reason))
        return false
    x1 := cfg[id "TLX"], y1 := cfg[id "TLY"], x2 := cfg[id "BRX"], y2 := cfg[id "BRY"]
    if (x1 >= x2 || y1 >= y2 || (x2-x1+1)*(y2-y1+1) > 4000000) {
        reason := id ": ↖/↘ 순서와 검색 영역 크기(최대 400만 픽셀)를 확인하세요."
        return false
    }
    return true
}

ReadRunConfig(cancelMode, ByRef reason) {
    global LegacyKeys, OptionKeys, Steps, Bound
    cfg := {cancelMode:cancelMode, steps:[]}
    for _, key in LegacyKeys
        cfg[key] := Get(key)
    for _, key in OptionKeys
        cfg[key] := Get(key)
    ranges := {SeatDelay:[0,5000], CancelDelay:[0,5000], Tolerance:[0,255]
        , ZoneDelay:[10,10000], SearchTimeout:[1000,43200000], CancelTimeout:[1000,43200000], PollDelay:[1,2000]
        , SeatConfirmTimeout:[100,10000], SeatOffset:[0,20], SeatGap:[1,100]
        , PreDelay:[100,10000], SeatCount:[1,4]}
    for key, bounds in ranges {
        if (!ValidInt(cfg[key], bounds[1], bounds[2])) {
            reason := key ": " bounds[1] "~" bounds[2] " 정수가 필요합니다."
            return false
        }
        cfg[key] += 0
    }
    if (cancelMode)
        cfg.SearchTimeout := cfg.CancelTimeout
    cfg.ClickDelay := cancelMode ? cfg.CancelDelay : cfg.SeatDelay
    cfg.WantedSeats := cancelMode ? 1 : cfg.SeatCount
    if (cancelMode) {
        if (!ValidRect(cfg, "Zone1", reason) || !ValidPoint(cfg, "CancelDone", reason))
            return false
        if (!ValidColor(cfg.Zone1Color)) {
            reason := "1구역 색을 0xBBGGRR 형식으로 설정하세요."
            return false
        }
        if (cfg.UseZones) {
            if (!ValidRect(cfg, "Zone2", reason) || !ValidPoint(cfg, "Zone1", reason)
                || !ValidPoint(cfg, "Zone2", reason))
                return false
            if (!ValidColor(cfg.Zone2Color)) {
                reason := "2구역 색을 설정하세요."
                return false
            }
        }
    } else {
        if (!ValidRect(cfg, "Seat", reason) || !ValidPoint(cfg, "Done", reason))
            return false
        if (!ValidColor(cfg.SeatColor)) {
            reason := "일반 좌석 색을 0xBBGGRR 형식으로 설정하세요."
            return false
        }
        if (cfg.UseDate) {
            for _, id in ["Date", "Round", "Next"] {
                if (!ValidPoint(cfg, id, reason))
                    return false
            }
        }
    }
    if (cfg.FollowEnabled) {
        for index, step in Steps {
            if (!step.enabled)
                continue
            if (!ValidateStep(step, reason, true)) {
                reason := "후속 단계 " index ": " reason
                return false
            }
            cfg.steps.Push(step.Clone())
        }
    }
    if (!ValidateCoordinates(cfg, reason))
        return false
    return cfg
}

ValidateStep(step, ByRef reason, checkBounds := false) {
    global Bound
    if (step.name = "" || StrLen(step.name) > 80 || RegExMatch(step.name, "[\r\n]")) {
        reason := "단계 이름은 1~80자 한 줄이어야 합니다."
        return false
    }
    if (!ValidInt(step.x, 0, 32767) || !ValidInt(step.y, 0, 32767)
        || !ValidInt(step.delay, 0, 60000) || !ValidInt(step.timeout, 100, 120000)
        || step.timeout <= step.delay) {
        reason := "좌표/지연/제한을 확인하세요. 제한시간은 지연보다 커야 합니다."
        return false
    }
    if (step.wait && (!ValidInt(step.waitX, 0, 32767) || !ValidInt(step.waitY, 0, 32767)
        || !ValidColor(step.color) || !ValidInt(step.tolerance, 0, 255))) {
        reason := "대기 픽셀 좌표·색·허용 오차가 올바르지 않습니다."
        return false
    }
    if (checkBounds && (step.x >= Bound.width || step.y >= Bound.height
        || (step.wait && (step.waitX >= Bound.width || step.waitY >= Bound.height)))) {
        reason := "후속 좌표가 현재 브라우저 클라이언트 범위를 벗어납니다."
        return false
    }
    return true
}

UseStepList() {
    global C
    Gui, Main:Default
    Gui, Main:ListView, % C.StepList
}

RefreshSteps(select := 0) {
    global Steps
    UseStepList()
    LV_Delete()
    for index, step in Steps
        LV_Add("", index, step.enabled ? "사용" : "끔", step.name, step.x "/" step.y
            , step.delay, step.timeout, step.wait ? step.color " @ " step.waitX "/" step.waitY : "없음")
    for column, width in [45, 45, 180, 105, 90, 90, 265]
        LV_ModifyCol(column, width)
    if (select)
        LV_Modify(select, "Select Focus Vis")
}

SaveStep(update) {
    global Steps, State, EditingStep
    if (State.running)
        return
    step := {enabled:Get("StepEnabled")+0, name:Get("StepName"), x:Get("StepX"), y:Get("StepY")
        , delay:Get("StepDelay"), timeout:Get("StepTimeout"), wait:Get("StepWait")+0
        , waitX:Get("StepWaitX"), waitY:Get("StepWaitY"), color:Get("StepColor"), tolerance:Get("StepTolerance")}
    if (!ValidateStep(step, reason)) {
        SetStatus(reason)
        return
    }
    if (update) {
        if (!EditingStep || EditingStep > Steps.Length()) {
            SetStatus("수정할 단계를 먼저 더블클릭하세요.")
            return
        }
        Steps[EditingStep] := step, index := EditingStep
    } else {
        if (Steps.Length() >= 20) {
            SetStatus("후속 단계는 최대 20개입니다.")
            return
        }
        Steps.Push(step), index := Steps.Length()
    }
    RefreshSteps(index), Put("Verified", 0)
    SetStatus("단계가 반영되었습니다. 저장 버튼으로 파일에 저장하세요.")
}

EditStep() {
    global Steps, EditingStep
    UseStepList()
    index := LV_GetNext()
    if (!index)
        return
    EditingStep := index, step := Steps[index]
    map := {enabled:"StepEnabled", name:"StepName", x:"StepX", y:"StepY", delay:"StepDelay"
        , timeout:"StepTimeout", wait:"StepWait", waitX:"StepWaitX", waitY:"StepWaitY"
        , color:"StepColor", tolerance:"StepTolerance"}
    for key, control in map
        Put(control, step[key])
}

DeleteStep() {
    global Steps, EditingStep, State
    if (State.running)
        return
    UseStepList()
    index := LV_GetNext()
    if (index) {
        Steps.RemoveAt(index), EditingStep := 0
        RefreshSteps(), Put("Verified", 0)
    }
}

MoveStep(direction) {
    global Steps, EditingStep, State
    if (State.running)
        return
    UseStepList()
    index := LV_GetNext(), target := index+direction
    if (!index || target < 1 || target > Steps.Length())
        return
    item := Steps.RemoveAt(index), Steps.InsertAt(target, item), EditingStep := 0
    RefreshSteps(target), Put("Verified", 0)
}

WriteSetting(path, section, key, value) {
    IniWrite, %value%, %path%, %section%, %key%
    return !ErrorLevel
}

SaveSettings() {
    global DataDir, LegacyKeys, OptionKeys, Steps, State
    if (State.running)
        return
    FileCreateDir, %DataDir%
    if (ErrorLevel) {
        SetStatus("설정 폴더 생성 실패: " DataDir)
        return
    }
    ok := true
    for index, key in LegacyKeys {
        fileIndex := Ceil(index/5)
        ok := WriteSetting(DataDir "\Save" fileIndex ".ini", "내용", index, Get(key)) && ok
    }
    path := DataDir "\EnhancedSettings.ini"
    ok := WriteSetting(path, "Meta", "FormatVersion", 1) && ok
    ok := WriteSetting(path, "Meta", "CoordinateMode", "Client") && ok
    for _, key in OptionKeys
        ok := WriteSetting(path, "Options", key, Get(key)) && ok
    ok := WriteSetting(path, "Steps", "Count", Steps.Length()) && ok
    for index, step in Steps {
        for key, value in step
            ok := WriteSetting(path, "Step" index, key, value) && ok
    }
    SetStatus(ok ? "저장 완료: data 폴더 (창 연결은 저장하지 않음)" : "저장 오류: 일부 설정을 쓰지 못했습니다.")
    return ok
}

ReadSetting(path, section, key, fallback := "") {
    IniRead, value, %path%, %section%, %key%, %fallback%
    return value = "ERROR" ? fallback : value
}

LoadSettings() {
    global DataDir, LegacyKeys, OptionKeys, Steps, Bound, EditingStep
    Bound := {}, EditingStep := 0
    Put("Verified", 0)
    path := DataDir "\EnhancedSettings.ini"
    if (!FileExist(path))
        return
    if (ReadSetting(path, "Meta", "FormatVersion") != 1
        || ReadSetting(path, "Meta", "CoordinateMode") != "Client") {
        SetStatus("설정 형식/좌표 기준이 맞지 않아 불러오지 않았습니다.")
        return
    }
    for index, key in LegacyKeys
        Put(key, ReadSetting(DataDir "\Save" Ceil(index/5) ".ini", "내용", index, Get(key)))
    for _, key in OptionKeys
        Put(key, ReadSetting(path, "Options", key, Get(key)))
    count := ReadSetting(path, "Steps", "Count", "0")
    Steps := []
    if (!ValidInt(count, 0, 20)) {
        SetStatus("후속 단계 개수가 잘못되어 목록을 불러오지 않았습니다.")
        RefreshSteps()
        return
    }
    Loop, %count% {
        step := {}, index := A_Index
        for _, key in ["enabled", "name", "x", "y", "delay", "timeout", "wait", "waitX", "waitY", "color", "tolerance"]
            step[key] := ReadSetting(path, "Step" index, key)
        if (!ValidateStep(step, reason)) {
            Steps := [], RefreshSteps()
            SetStatus("후속 단계 " index " 불러오기 오류: " reason)
            return
        }
        Steps.Push(step)
    }
    RefreshSteps()
    SetStatus("설정을 불러왔습니다. 브라우저에서 F7 연결 후 좌표를 다시 확인하세요.")
}

ImportLegacy() {
    global LegacyKeys, Bound, State
    if (State.running)
        return
    FileSelectFolder, folder,, 0, 원본 Save1.ini~Save6.ini 폴더 선택 (읽기 전용으로 가져옴)
    if (folder = "")
        return
    values := {}
    Loop, 6 {
        path := folder "\Save" A_Index ".ini"
        if (!FileExist(path)) {
            SetStatus("가져오기 실패: " path)
            return
        }
        FileRead, contents, % "*P949 " path
        if (ErrorLevel) {
            SetStatus("파일 읽기 실패: " path)
            return
        }
        inSection := false
        Loop, Parse, contents, `n, `r
        {
            line := Trim(A_LoopField)
            if (SubStr(line, 1, 1) = "[")
                inSection := line = "[내용]"
            else if (inSection && RegExMatch(line, "^([0-9]+)\s*=(.*)$", match))
                values[match1+0] := Trim(match2)
        }
    }
    imported := 0
    for index, key in LegacyKeys {
        value := values[index]
        valid := InStr(key, "Color") ? ValidColor(value) : ValidInt(value, 0, 1000000)
        if (valid) {
            Put(key, value), imported += 1
        } else if (InStr(key, "Color") || RegExMatch(key, "[XY]$")) {
            Put(key, "")
        }
    }
    Bound := {}
    Put("Verified", 0)
    SetStatus("원본 값 " imported "개 가져옴. 원본 좌표는 창 기준: F7 연결 후 모든 좌표를 F8로 다시 잡으세요.")
}
