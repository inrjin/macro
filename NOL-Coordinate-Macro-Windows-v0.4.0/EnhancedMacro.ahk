; AutoHotkey v1.1 Unicode only. Save as UTF-8 with BOM.
; Reimplementation based on the inspected legacy macro; no original binary/DLL is used.
#NoEnv
#SingleInstance Ignore
#Persistent
#MaxThreadsPerHotkey 1
SetBatchLines, -1
SetWorkingDir, %A_ScriptDir%
SendMode, Event
SetMouseDelay, -1
SetDefaultMouseSpeed, 0
CoordMode, Mouse, Screen
CoordMode, Pixel, Screen
Thread, Interrupt, 0

C := {}, Buttons := [], Steps := [], Bound := {}, State := {running:false}
EditingStep := 0
DataDir := A_ScriptDir . "\data"
LegacyKeys := ["DateX", "DateY", "RoundX", "RoundY", "NextX", "NextY"
    , "SeatTLX", "SeatTLY", "SeatBRX", "SeatBRY", "DoneX", "DoneY"
    , "SeatColor", "Zone1Color", "Zone2Color", "CancelDoneX", "CancelDoneY"
    , "Zone1TLX", "Zone1TLY", "Zone1BRX", "Zone1BRY", "Zone2TLX", "Zone2TLY"
    , "Zone2BRX", "Zone2BRY", "SeatDelay", "CancelDelay", "Tolerance", "ZoneDelay"]
OptionKeys := ["UseDate", "SeatCount", "SearchTimeout", "PollDelay", "SeatConfirmTimeout"
    , "SeatOffset", "SeatGap", "PreDelay", "UseZones", "FollowEnabled"
    , "Zone1X", "Zone1Y", "Zone2X", "Zone2Y"]
OptionKeys.Push("CancelTimeout", "UseRegion", "RegionX", "RegionY", "RegionWait", "ReadyEnabled", "ReadyX", "ReadyY", "ReadyColor", "ConflictEnabled", "ConflictAX", "ConflictAY", "ConflictAColor", "ConflictBX", "ConflictBY", "ConflictBColor", "DismissX", "DismissY", "NextEnabled", "SuccessX", "SuccessY", "SuccessColor", "ResultWait", "RetryLimit", "ClockURL", "ClockMode", "ClockPath", "SaleTime", "OpenX", "OpenY")
Clock := {}, ClockPID := 0
BuildGui()
if (A_Args.Length() && A_Args[1] = "--self-test")
    RunSelfTest()
LoadSettings()
Gui, Main:Show, w960 h740, NOL 좌표 매크로 0.4.0
return

#If BrowserForeground()
F1::StartRun(false)
F2::StartRun(true)
F4::StartRun(false, true)
F6::ResumeCoordinates()
F7::BindBrowser()
F8::CaptureField()
#If
F3::StopRun("F3: 긴급 중지. 다시 실행하려면 F1/F2를 누르세요.")

ClockSyncButton:
SyncClock()
return
ClockPoll:
PollClock()
return

EngineTick:
Loop, 8 {
    beforePhase := State.phase
    Tick()
    if (!State.running || State.phase = beforePhase || NowMs() < State.due)
        break
}
return

SaveButton:
SaveSettings()
return
LoadButton:
if (!State.running)
    LoadSettings()
return
ImportButton:
ImportLegacy()
return
StopButton:
StopRun("중지 버튼: 실행을 중지했습니다.")
return
AddStepButton:
SaveStep(false)
return
UpdateStepButton:
SaveStep(true)
return
DeleteStepButton:
DeleteStep()
return
UpStepButton:
MoveStep(-1)
return
DownStepButton:
MoveStep(1)
return
StepListEvent:
if (A_GuiEvent = "DoubleClick")
    EditStep()
return
MainGuiClose:
StopRun("종료")
ExitApp
return

#Include %A_ScriptDir%\MacroGui.ahk
#Include %A_ScriptDir%\MacroEngine.ahk
#Include %A_ScriptDir%\CoordinateFlow.ahk
#Include %A_ScriptDir%\MacroSelfTest.ahk
