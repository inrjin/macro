; Runs without pointer/keyboard input. Hidden GUI exists only for validating settings.
RunSelfTest() {
    global Bound, Steps, TestCount
    TestCount := 0
    try {
        TestAssert(ColorMatches(0x123456, 0x123456, 0), "exact pixel color")
        TestAssert(!ColorMatches(0x123457, 0x123456, 0), "color mismatch")
        TestAssert(ColorMatches(0x123457, 0x123456, 1), "color tolerance")
        TestAssert(!ValidInt("-1", 0, 100), "negative coordinates rejected")
        TestAssert(ValidColor("0x112233"), "BGR color")
        TestAssert(!ValidColor("0x11223344"), "oversized color rejected")
        Bound := {width:1280, height:720}
        step := {name:"test", x:200, y:300, enabled:1, delay:500, timeout:2000, wait:0}
        TestAssert(ValidateStep(step, reason, true), "valid step")
        step.timeout := 500
        TestAssert(!ValidateStep(step, reason, true), "timeout must exceed delay")
        step.timeout := 2000, step.x := 1280
        TestAssert(!ValidateStep(step, reason, true), "out-of-bounds step rejected")
        rects := []
        AddRect(rects, 3, 4, 2, 5)
        TestAssert(rects.Length() = 0, "inverted rectangles rejected")
        AddRect(rects, 0, 0, 10, 10)
        TestAssert(rects.Length() = 1, "valid rectangle")
        cfg := {UseRegion:0, RegionWait:150, ResultWait:5000, RetryLimit:30, steps:[], ConflictEnabled:0, NextEnabled:0, FollowEnabled:0}
        TestAssert(ValidateCoordinates(cfg, reason), "coordinate defaults")
        cfg.ConflictEnabled := 1, cfg.ConflictAX := 10, cfg.ConflictAY := 10, cfg.ConflictBX := 10, cfg.ConflictBY := 10
        cfg.ConflictAColor := "0x112233", cfg.ConflictBColor := "0x445566", cfg.DismissX := 20, cfg.DismissY := 20
        TestAssert(!ValidateCoordinates(cfg, reason), "duplicate conflict points rejected")
        cfg.ConflictBX := 11
        TestAssert(ValidateCoordinates(cfg, reason), "distinct conflict probes accepted")
        cfg.FollowEnabled := 1, cfg.steps.Push({enabled:1})
        TestAssert(!ValidateCoordinates(cfg, reason), "follow-up requires price gate")
        cfg.NextEnabled := 1, cfg.SuccessX := 30, cfg.SuccessY := 30, cfg.SuccessColor := "0x778899"
        TestAssert(ValidateCoordinates(cfg, reason), "price gate accepts follow-up")
        FileAppend, % "PASS: " TestCount " macro checks. No browser input sent.`n", *
        ExitApp, 0
    } catch e {
        FileAppend, % "FAIL: " e.Message "`n", *
        ExitApp, 1
    }
}
TestAssert(condition, label) {
    global TestCount
    TestCount += 1
    if (!condition)
        throw Exception(label)
}
