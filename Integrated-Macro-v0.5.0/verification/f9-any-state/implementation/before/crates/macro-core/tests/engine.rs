use macro_core::*;

fn target(x: f64, y: f64) -> Target { Target::Point { point: Point { x, y } } }
fn candidate(id: &str, x: f64, y: f64) -> Candidate {
    Candidate { id: id.into(), point: Point { x, y }, color: Profile::default().color, target: target(x, y), selected: false, geometry: None }
}
fn observed(candidates: Vec<Candidate>) -> Event { Event::CandidatesObserved { generation: 0, candidates } }
fn recovery_profile(count: usize) -> Profile {
    let mut p = Profile::default();
    p.target_seats = count;
    p.features.conflict_recovery = true;
    p.features.seat_priority = true;
    p.legacy.complete = Some(target(900.0, 500.0));
    p
}
fn physical(actions: &[Action]) -> Vec<&Action> {
    actions.iter().filter(|a| !matches!(a.kind, ActionKind::Observe { .. })).collect()
}
fn ack(engine: &mut Engine, actions: &[Action], now: u64) -> Vec<Action> {
    let action = physical(actions).last().unwrap().id;
    engine.step(Event::ActionAcknowledged { id: action, success: true, detail: String::new() }, now)
}
fn selected(engine: &mut Engine, id: &str, now: u64) -> Vec<Action> {
    engine.step(Event::SeatResult { candidate_id: id.into(), result: SeatOutcome::Selected }, now)
}
fn start(engine: &mut Engine) { engine.step(Event::Start { mode: RunMode::Normal }, 0); }
fn conflict(engine: &mut Engine, id: &str, now: u64) -> Vec<Action> {
    engine.step(Event::SeatResult { candidate_id: id.into(), result: SeatOutcome::Conflict }, now)
}
fn delivered_result(engine: &mut Engine, attempt: &DeliveredSeatAttempt, result: SeatOutcome, now: u64) -> Vec<Action> {
    engine.step(Event::DeliveredSeatResult {
        action_id: attempt.action_id, candidate_id: attempt.candidate_id.clone(), result,
    }, now)
}

fn independent_seats_profile() -> Profile {
    let mut p = recovery_profile(2);
    p.features = Features {
        server_clock: true, auto_entry: true, auto_captcha_resume: true, manual_entry: false,
        nol_captcha_detection: false,
        nol_selection_count: false, seat_objects: false,
        region_priority: true, seat_priority: true, adjacent_seats: false, conflict_recovery: true,
        automatic_refresh: true, status_overlay: true,
    };
    p.entry_target = Some(target(500.0, 100.0));
    p.legacy.date = Some(target(500.0, 200.0));
    p.legacy.session = Some(target(500.0, 300.0));
    p.legacy.next = Some(target(500.0, 400.0));
    p.legacy.cancellation_navigation = CancellationNavigation::ArrowKeys;
    p.vision.roi = Some(Rect { x: 0.0, y: 0.0, width: 100.0, height: 100.0 });
    p.legacy.regions = vec![LegacyRegionSettings {
        color: Some(Color { r: 255, g: 0, b: 0 }),
        vision: Some(VisionConfig {
            roi: Some(Rect { x: 500.0, y: 500.0, width: 100.0, height: 100.0 }),
            ..VisionConfig::default()
        }),
        complete: Some(target(700.0, 700.0)),
    }];
    p.refresh_interval_ms = 100;
    p
}

fn continuous_cancellation_profile() -> Profile {
    let mut p = recovery_profile(2);
    p.features.automatic_refresh = true;
    p.refresh_interval_ms = 100;
    p
}

#[test]
fn single_zone_cancellation_ignores_normal_entry_flags_and_keeps_scanning_without_refresh() {
    for deadline in [None, Some(100_000)] {
        let mut p = independent_seats_profile();
        p.sale_deadline_ms = deadline;
        p.legacy.cancellation_navigation = CancellationNavigation::None;
        p.features.automatic_refresh = false;
        // Even a saved button must not trigger refresh while its option is off.
        p.refresh_target = Some(target(90.0, 90.0));
        let common_complete = p.legacy.complete.clone();
        let mut engine = Engine::new(p).unwrap();
        let started = engine.step(Event::Start { mode: RunMode::Cancellation }, 0);
        assert_eq!(engine.snapshot().phase, Phase::Seats);
        assert_eq!(engine.snapshot().active_region, None);
        assert_eq!(started.len(), 1);
        assert!(matches!(&started[0].kind, ActionKind::Observe { scope } if scope == "seats"));
        assert!(!engine.accepts_captcha_enter());
        assert!(engine.step(Event::CaptchaSubmitted, 1).is_empty());
        assert!(engine.step(Event::RegionsObserved { candidates: vec![RegionCandidate {
            candidate: candidate("old-region", 10.0, 10.0), center_rank: 0, distance_to_stage: 10.0,
        }] }, 2).is_empty());
        engine.step(observed(vec![]), 3);
        for time in [103, 203, 10_003] {
            let poll = engine.step(Event::Tick, time);
            assert_eq!(poll.len(), 1);
            assert!(matches!(&poll[0].kind, ActionKind::Observe { scope } if scope == "seats"));
            assert_eq!(engine.snapshot().phase, Phase::Seats);
            assert_eq!(engine.snapshot().generation, 0);
        }
        let first = engine.step(observed(vec![
            candidate("a", 20.0, 20.0), candidate("b", 30.0, 20.0),
        ]), 10_004);
        assert!(matches!(&physical(&first)[0].kind, ActionKind::Click { purpose, .. } if purpose == "seat:a"));
        let second = ack(&mut engine, &first, 10_005);
        assert!(matches!(&physical(&second)[0].kind, ActionKind::Click { purpose, .. } if purpose == "seat:b"));
        let completed = ack(&mut engine, &second, 10_006);
        assert!(matches!(&physical(&completed)[0].kind, ActionKind::CompleteSelection { target }
            if Some(target) == common_complete.as_ref()));
    }
}

#[test]
fn single_zone_cancellation_without_common_completion_hands_off_after_seat_attempt() {
    let mut p = independent_seats_profile();
    p.target_seats = 1;
    p.legacy.cancellation_navigation = CancellationNavigation::None;
    p.legacy.complete = None;
    p.features.automatic_refresh = false;
    let mut engine = Engine::new(p).unwrap();
    engine.step(Event::Start { mode: RunMode::Cancellation }, 0);
    assert_eq!(engine.effective_completion_target(), None);
    let seat = engine.step(observed(vec![candidate("a", 20.0, 20.0)]), 1);
    let handoff = ack(&mut engine, &seat, 2);
    assert!(physical(&handoff).is_empty());
    assert_eq!(engine.snapshot().phase, Phase::Completing);
    assert_eq!(engine.snapshot().attempted_ids, vec!["a"]);
    assert!(engine.snapshot().message.contains("완료 버튼을 지정하거나 직접 진행"));
    assert!(physical(&engine.step(Event::Tick, 10_000)).is_empty());
}

#[test]
fn independent_seats_mode_serialization_preserves_existing_mode_values() {
    for (mode, value) in [(RunMode::Normal, "normal"), (RunMode::Cancellation, "cancellation"), (RunMode::Seats, "seats")] {
        assert_eq!(serde_json::to_value(mode).unwrap(), serde_json::json!(value));
        assert_eq!(serde_json::from_value::<RunMode>(serde_json::json!(value)).unwrap(), mode);
    }
}

#[test]
fn independent_seats_starts_directly_with_all_entry_and_region_features_enabled() {
    for deadline in [None, Some(100_000)] {
        let mut p = independent_seats_profile();
        p.sale_deadline_ms = deadline;
        let global_color = p.color;
        let global_roi = p.vision.roi;
        let global_complete = p.legacy.complete.clone();
        let mut engine = Engine::new(p).unwrap();
        let actions = engine.step(Event::Start { mode: RunMode::Seats }, 0);
        assert_eq!(engine.snapshot().phase, Phase::Seats);
        assert_eq!(engine.snapshot().mode, RunMode::Seats);
        assert_eq!(engine.snapshot().active_region, None);
        assert_eq!(engine.effective_color(), global_color);
        assert_eq!(engine.effective_vision().roi, global_roi);
        assert_eq!(engine.effective_completion_target(), global_complete.as_ref());
        assert_eq!(actions.len(), 1);
        assert!(matches!(&actions[0].kind, ActionKind::Observe { scope } if scope == "seats"));
        assert!(engine.step(entry(EntryStage::ProductBooking, true, None), 1).is_empty());
        assert!(engine.step(Event::RegionsObserved { candidates: vec![RegionCandidate {
            candidate: candidate("old-region", 10.0, 10.0), center_rank: 0, distance_to_stage: 10.0,
        }] }, 2).is_empty());
        let seat = engine.step(observed(vec![candidate("seat", 20.0, 30.0)]), 3);
        assert!(matches!(&physical(&seat)[0].kind, ActionKind::Click { purpose, .. } if purpose == "seat:seat"));
    }
}

#[test]
fn independent_seats_empty_maps_never_enter_legacy_region_navigation() {
    for navigation in [CancellationNavigation::ArrowKeys, CancellationNavigation::TwoRegions {
        first: target(500.0, 500.0), second: target(600.0, 500.0),
    }] {
        for region_priority in [false, true] {
            for automatic_refresh in [false, true] {
                let mut p = independent_seats_profile();
                p.features.region_priority = region_priority;
                p.features.automatic_refresh = automatic_refresh;
                p.legacy.cancellation_navigation = navigation.clone();
                let mut engine = Engine::new(p).unwrap();
                engine.step(Event::Start { mode: RunMode::Seats }, 0);
                engine.step(observed(vec![]), 1);
                for time in [101, 201, 10_001] {
                    let actions = engine.step(Event::Tick, time);
                    assert_eq!(actions.len(), 1);
                    assert!(matches!(&actions[0].kind, ActionKind::Observe { scope } if scope == "seats"));
                    assert_eq!(engine.snapshot().phase, Phase::Seats);
                    assert_eq!(engine.snapshot().active_region, None);
                    assert_eq!(engine.snapshot().generation, 0);
                }
            }
        }
    }
}

#[test]
fn independent_seats_returns_to_seats_after_positive_captcha_confirmation() {
    for automatic_resume in [false, true] {
        let mut p = independent_seats_profile();
        p.features.auto_captcha_resume = automatic_resume;
        let mut engine = Engine::new(p).unwrap();
        engine.step(Event::Start { mode: RunMode::Seats }, 0);
        engine.step(entry(EntryStage::Captcha, true, None), 1);
        assert_eq!(engine.snapshot().phase, Phase::Captcha);
        assert!(engine.step(Event::Resume, 2).is_empty());
        assert_eq!(engine.snapshot().phase, Phase::Captcha);
        let confirmed = engine.step(Event::CaptchaCompleted, 3);
        assert!(physical(&confirmed).is_empty());
        if !automatic_resume {
            assert_eq!(engine.snapshot().phase, Phase::Captcha);
            assert!(physical(&engine.step(Event::Resume, 4)).is_empty());
        }
        assert_eq!(engine.snapshot().phase, Phase::Seats);
        assert_eq!(engine.snapshot().active_region, None);
        let seat = engine.step(observed(vec![candidate("seat", 20.0, 30.0)]), 5);
        assert!(matches!(&physical(&seat)[0].kind, ActionKind::Click { purpose, .. } if purpose == "seat:seat"));
    }
}

#[test]
fn independent_seats_repeat_start_and_resume_preserve_delivered_attempts() {
    let mut p = independent_seats_profile();
    p.target_seats = 3;
    let mut engine = Engine::new(p).unwrap();
    engine.step(Event::Start { mode: RunMode::Seats }, 0);
    let kept = engine.step(observed(vec![candidate("kept", 10.0, 10.0)]), 1);
    ack(&mut engine, &kept, 2);
    selected(&mut engine, "kept", 3);
    let failed = engine.step(observed(vec![candidate("failed", 20.0, 10.0)]), 4);
    ack(&mut engine, &failed, 5);
    conflict(&mut engine, "failed", 6);
    engine.step(Event::ConflictDialogGone, 7);
    let click = engine.step(observed(vec![candidate("provisional", 30.0, 10.0)]), 8);
    ack(&mut engine, &click, 9);
    assert!(engine.step(Event::Start { mode: RunMode::Seats }, 10).is_empty());
    engine.step(Event::ContextLost { reason: "temporarily hidden".into() }, 11);
    assert!(engine.step(Event::Start { mode: RunMode::Seats }, 12).is_empty());
    assert!(physical(&engine.step(Event::Resume, 13)).is_empty());
    assert_eq!(engine.snapshot().phase, Phase::Seats);
    assert_eq!(engine.snapshot().mode, RunMode::Seats);
    assert_eq!(engine.snapshot().selected_ids, vec!["kept"]);
    assert_eq!(engine.snapshot().attempted_ids, vec!["provisional"]);
    assert_eq!(engine.snapshot().rejected_ids, vec!["failed"]);
    assert!(engine.snapshot().current_candidate_id.is_none());
    engine.step(Event::SelectionObserved { selected_ids: vec![] }, 14);
    assert_eq!(engine.snapshot().attempted_ids, vec!["provisional"]);
    assert_eq!(engine.snapshot().selected_ids, vec!["kept"]);
    let next = engine.step(observed(vec![candidate("other", 40.0, 10.0)]), 15);
    assert!(matches!(&physical(&next)[0].kind, ActionKind::Click { purpose, .. } if purpose == "seat:other"));
    assert!(engine.step(Event::CompletionConfirmed, 16).is_empty());
    assert_eq!(engine.snapshot().phase, Phase::Seats);
}

#[test]
fn independent_seats_resumes_after_calibration_without_reentering_booking_or_regions() {
    let mut p = independent_seats_profile();
    p.legacy.normal_click_delay_ms = 10;
    p.legacy.cancellation_click_delay_ms = 1000;
    let mut engine = Engine::new(p).unwrap();
    engine.step(Event::Start { mode: RunMode::Seats }, 0);
    let kept = engine.step(observed(vec![candidate("kept", 10.0, 10.0)]), 1);
    ack(&mut engine, &kept, 2);
    selected(&mut engine, "kept", 3);
    engine.step(Event::Pause, 3);
    engine.update_entry_target(Some(target(550.0, 100.0))).unwrap();
    let profile = engine.profile().clone();
    engine.update_calibration(profile.vision, profile.color, profile.region_color, profile.legacy.complete).unwrap();
    let resumed = engine.step(Event::Resume, 4);
    assert_eq!(resumed.len(), 1);
    assert!(matches!(&resumed[0].kind, ActionKind::Observe { scope } if scope == "seats"));
    assert_eq!(engine.snapshot().selected_ids, vec!["kept"]);
    assert!(physical(&engine.step(observed(vec![candidate("next", 30.0, 10.0)]), 5)).is_empty());
    assert!(engine.step(Event::Tick, 11).is_empty());
    let click = engine.step(Event::Tick, 12);
    assert!(matches!(&physical(&click)[0].kind, ActionKind::Click { purpose, .. } if purpose == "seat:next"));
}

#[test]
fn independent_seats_new_run_discards_prior_cancellation_navigation_and_late_ack() {
    let mut p = independent_seats_profile();
    p.features.server_clock = false;
    p.features.auto_entry = false;
    p.features.region_priority = false;
    let mut engine = Engine::new(p).unwrap();
    let old_navigation = engine.step(Event::Start { mode: RunMode::Cancellation }, 0);
    assert!(matches!(&physical(&old_navigation)[0].kind, ActionKind::Key { purpose, .. } if purpose == "legacy_cancellation_region"));
    assert_eq!(engine.snapshot().active_region, Some(0));
    engine.step(Event::Stop, 1);
    let direct = engine.step(Event::Start { mode: RunMode::Seats }, 2);
    assert!(physical(&direct).is_empty());
    assert_eq!(engine.snapshot().phase, Phase::Seats);
    assert_eq!(engine.snapshot().active_region, None);
    assert!(ack(&mut engine, &old_navigation, 3).is_empty());
    assert_eq!(engine.snapshot().phase, Phase::Seats);
    let click = engine.step(observed(vec![candidate("global-seat", 20.0, 30.0)]), 4);
    assert!(matches!(&physical(&click)[0].kind, ActionKind::Click { purpose, .. } if purpose == "seat:global-seat"));
}

#[test]
fn stopping_independent_seats_preserves_normal_scheduled_entry_behavior() {
    let mut p = independent_seats_profile();
    p.sale_deadline_ms = Some(100);
    let mut engine = Engine::new(p).unwrap();
    engine.step(Event::Start { mode: RunMode::Seats }, 0);
    assert_eq!(engine.snapshot().phase, Phase::Seats);
    engine.step(Event::Stop, 1);
    assert!(engine.step(Event::Start { mode: RunMode::Normal }, 2).is_empty());
    assert_eq!(engine.snapshot().phase, Phase::WaitingSale);
    assert!(engine.step(Event::Tick, 99).is_empty());
    let initial = engine.step(Event::Tick, 100);
    assert!(matches!(&physical(&initial)[0].kind, ActionKind::Click { purpose, .. } if purpose == "initial_entry"));
}

#[test]
fn cancellation_internal_refresh_repeats_without_manual_phase_confirmation() {
    let mut p = continuous_cancellation_profile();
    p.refresh_target = Some(target(90.0, 90.0));
    let mut engine = Engine::new(p).unwrap();
    engine.step(Event::Start { mode: RunMode::Cancellation }, 0);
    engine.step(observed(vec![]), 1);
    for due in [100, 201, 302] {
        let refresh = engine.step(Event::Tick, due);
        assert!(matches!(&physical(&refresh)[0].kind, ActionKind::Refresh { .. }));
        let resumed = ack(&mut engine, &refresh, due + 1);
        assert_eq!(resumed.len(), 1);
        assert!(matches!(&resumed[0].kind, ActionKind::Observe { scope } if scope == "seats"));
        assert_eq!(engine.snapshot().phase, Phase::Seats);
        assert_eq!(engine.snapshot().generation, 0);
        assert!(!engine.snapshot().message.contains("F6"));
        assert!(engine.step(Event::Tick, due + 2).is_empty());
        engine.step(observed(vec![]), due + 3);
        // Refresh cadence starts at delivery, rather than at the initial run.
        assert!(engine.step(Event::Tick, due + 100).is_empty());
    }
    let seat = engine.step(observed(vec![candidate("new-seat", 10.0, 10.0)]), 404);
    assert!(matches!(&physical(&seat)[0].kind, ActionKind::Click { purpose, .. } if purpose == "seat:new-seat"));
}

#[test]
fn cancellation_refresh_delivery_is_not_refresh_evidence_and_retry_has_its_own_interval() {
    let mut p = continuous_cancellation_profile();
    p.refresh_target = Some(target(90.0, 90.0));
    let mut engine = Engine::new(p).unwrap();
    engine.step(Event::Start { mode: RunMode::Cancellation }, 0);
    let failed = engine.step(observed(vec![candidate("failed", 10.0, 10.0)]), 1);
    ack(&mut engine, &failed, 1);
    let escape = conflict(&mut engine, "failed", 2);
    ack(&mut engine, &escape, 3);
    engine.step(Event::ConflictDialogGone, 4);
    assert!(engine.step(observed(vec![candidate("failed", 10.0, 10.0)]), 5).is_empty());
    let refresh = engine.step(Event::Tick, 103);
    assert!(matches!(&physical(&refresh)[0].kind, ActionKind::Refresh { .. }));
    ack(&mut engine, &refresh, 103);
    assert_eq!(engine.snapshot().phase, Phase::Seats);
    assert_eq!(engine.snapshot().generation, 0);
    assert_eq!(engine.snapshot().rejected_ids, vec!["failed"]);
    assert!(engine.step(observed(vec![candidate("failed", 10.0, 10.0)]), 103).is_empty());
    let retry = engine.step(Event::Tick, 104);
    assert!(physical(&retry).is_empty());
    assert_eq!(engine.snapshot().phase, Phase::Seats);
    assert_eq!(engine.snapshot().generation, 0);
    assert!(engine.snapshot().rejected_ids.is_empty());
    let seat = engine.step(Event::CandidatesObserved {
        generation: 0, candidates: vec![candidate("failed", 10.0, 10.0)],
    }, 105);
    assert!(matches!(&physical(&seat)[0].kind, ActionKind::Click { purpose, .. } if purpose == "seat:failed"));
}

#[test]
fn independent_seats_does_not_refresh_retained_or_provisional_selections() {
    for outcome in ["selected", "attempted", "unknown"] {
        let mut p = independent_seats_profile();
        p.refresh_target = Some(target(90.0, 90.0));
        p.features.conflict_recovery = outcome != "attempted";
        let mut engine = Engine::new(p).unwrap();
        engine.step(Event::Start { mode: RunMode::Seats }, 0);
        let click = engine.step(observed(vec![candidate("kept", 10.0, 10.0)]), 1);
        ack(&mut engine, &click, 2);
        if outcome == "selected" { selected(&mut engine, "kept", 3); }
        if outcome == "unknown" {
            engine.step(Event::SeatResult { candidate_id: "kept".into(), result: SeatOutcome::Unknown }, 3);
        }
        engine.step(observed(vec![]), 4);
        let actions = engine.step(Event::Tick, 104);
        assert!(physical(&actions).is_empty());
        assert_eq!(engine.snapshot().phase, Phase::Seats);
        assert_eq!(engine.snapshot().generation, 0);
        match outcome {
            "selected" => assert_eq!(engine.snapshot().selected_ids, vec!["kept"]),
            "attempted" => assert_eq!(engine.snapshot().attempted_ids, vec!["kept"]),
            _ => {
                assert_eq!(engine.snapshot().attempted_ids, vec!["kept"]);
                assert!(engine.snapshot().current_candidate_id.is_none());
            }
        }
    }
}

#[test]
fn independent_seats_counts_delivered_clicks_without_selection_markers() {
    for recovery in [false, true] {
        for legacy_blind in [false, true] {
            let mut p = independent_seats_profile();
            p.features.conflict_recovery = recovery;
            p.legacy.blind_click_sequence = legacy_blind;
            let mut engine = Engine::new(p).unwrap();
            engine.step(Event::Start { mode: RunMode::Seats }, 0);
            let first = engine.step(observed(vec![candidate("a", 10.0, 10.0), candidate("b", 20.0, 10.0)]), 1);
            assert!(engine.snapshot().optimistic_selection);
            assert!(engine.snapshot().delivered_attempts.is_empty());
            assert!(selected(&mut engine, "a", 2).is_empty());
            assert!(conflict(&mut engine, "a", 3).is_empty());
            assert!(engine.snapshot().attempted_ids.is_empty());
            let second = ack(&mut engine, &first, 4);
            assert!(matches!(&physical(&second)[0].kind, ActionKind::Click { purpose, .. } if purpose == "seat:b"));
            assert_eq!(engine.snapshot().attempted_ids, vec!["a"]);
            assert_eq!(engine.snapshot().delivered_attempts[0].action_id, physical(&first)[0].id);
            assert!(ack(&mut engine, &first, 5).is_empty());
            assert_eq!(engine.snapshot().delivered_attempts.len(), 1);
            let complete = ack(&mut engine, &second, 6);
            assert!(matches!(&physical(&complete)[0].kind, ActionKind::CompleteSelection { .. }));
            assert_eq!(engine.snapshot().phase, Phase::Completing);
            assert_eq!(engine.snapshot().attempted_ids, vec!["a", "b"]);
            assert!(engine.snapshot().selected_ids.is_empty());
            let attempts = engine.snapshot().delivered_attempts;
            assert_eq!(attempts.iter().map(|a| a.candidate_id.as_str()).collect::<Vec<_>>(), vec!["a", "b"]);
            assert!(delivered_result(&mut engine, &attempts[0], SeatOutcome::Unknown, 7).is_empty());
            ack(&mut engine, &complete, 8);
            assert_eq!(engine.snapshot().phase, Phase::Completed);
            assert!(engine.snapshot().selected_ids.is_empty());
            assert_eq!(engine.snapshot().attempted_ids, vec!["a", "b"]);
            engine.step(Event::CompletionConfirmed, 9);
            assert_eq!(engine.snapshot().phase, Phase::Completed);
        }
    }
}

#[test]
fn independent_seats_never_counts_failed_or_cancelled_input_delivery() {
    for cancelled in [false, true] {
        let mut engine = Engine::new(independent_seats_profile()).unwrap();
        engine.step(Event::Start { mode: RunMode::Seats }, 0);
        let click = engine.step(observed(vec![candidate("a", 10.0, 10.0)]), 1);
        let id = physical(&click)[0].id;
        if cancelled {
            engine.step(Event::ActionCancelled { id }, 2);
        } else {
            engine.step(Event::ActionAcknowledged { id, success: false, detail: "not delivered".into() }, 2);
        }
        assert!(engine.snapshot().attempted_ids.is_empty());
        assert!(engine.snapshot().delivered_attempts.is_empty());
        assert!(ack(&mut engine, &click, 3).is_empty());
        assert!(engine.snapshot().attempted_ids.is_empty());
    }
}

#[test]
fn independent_seats_late_conflict_cancels_queued_next_seat_without_counting_it() {
    let mut engine = Engine::new(independent_seats_profile()).unwrap();
    engine.step(Event::Start { mode: RunMode::Seats }, 0);
    let first = engine.step(observed(vec![candidate("a", 10.0, 10.0), candidate("b", 20.0, 10.0)]), 1);
    let second = ack(&mut engine, &first, 2);
    let failed = engine.snapshot().delivered_attempts[0].clone();
    assert_eq!(engine.snapshot().current_candidate_id.as_deref(), Some("b"));
    assert!(conflict(&mut engine, "b", 3).is_empty());
    let escape = delivered_result(&mut engine, &failed, SeatOutcome::Conflict, 4);
    assert!(matches!(&physical(&escape)[0].kind, ActionKind::Key { key: Key::Escape, .. }));
    assert_eq!(engine.snapshot().rejected_ids, vec!["a"]);
    assert!(engine.snapshot().attempted_ids.is_empty());
    assert!(engine.snapshot().delivered_attempts.is_empty());
    assert!(ack(&mut engine, &second, 5).is_empty());
    assert!(engine.step(Event::ActionCancelled { id: physical(&second)[0].id }, 6).is_empty());
    assert_eq!(engine.snapshot().pending_action_id, Some(physical(&escape)[0].id));
    ack(&mut engine, &escape, 7);
    let gone = engine.step(Event::ConflictDialogGone, 8);
    assert!(matches!(&gone[0].kind, ActionKind::Observe { scope } if scope == "seats"));
    let replacement = engine.step(observed(vec![candidate("a", 10.0, 10.0), candidate("b", 20.0, 10.0)]), 9);
    assert!(matches!(&physical(&replacement)[0].kind, ActionKind::Click { purpose, .. } if purpose == "seat:b"));
}

#[test]
fn independent_seats_late_conflict_keeps_other_attempt_and_cancels_queued_completion() {
    let mut engine = Engine::new(independent_seats_profile()).unwrap();
    engine.step(Event::Start { mode: RunMode::Seats }, 0);
    let first = engine.step(observed(vec![candidate("a", 10.0, 10.0), candidate("b", 20.0, 10.0)]), 1);
    let second = ack(&mut engine, &first, 2);
    let complete = ack(&mut engine, &second, 3);
    let failed = engine.snapshot().delivered_attempts[0].clone();
    let escape = delivered_result(&mut engine, &failed, SeatOutcome::Conflict, 4);
    assert_eq!(engine.snapshot().attempted_ids, vec!["b"]);
    assert!(delivered_result(&mut engine, &failed, SeatOutcome::Conflict, 5).is_empty());
    assert_eq!(engine.snapshot().pending_action_id, Some(physical(&escape)[0].id));
    assert!(ack(&mut engine, &complete, 6).is_empty());
    // Positive evidence that the dialog closed cancels a queued Esc before it is sent.
    let gone = engine.step(Event::ConflictDialogGone, 7);
    assert!(matches!(&gone[0].kind, ActionKind::Observe { scope } if scope == "seats"));
    assert!(ack(&mut engine, &escape, 8).is_empty());
    assert!(engine.snapshot().pending_action_id.is_none());
    assert_eq!(engine.snapshot().attempted_ids, vec!["b"]);
    engine.step(Event::SelectionObserved { selected_ids: vec!["unrelated".into()] }, 9);
    assert_eq!(engine.snapshot().attempted_ids, vec!["b"]);
    assert!(engine.snapshot().selected_ids.is_empty());
    let replacement = engine.step(observed(vec![
        candidate("a", 10.0, 10.0), candidate("b", 20.0, 10.0), candidate("c", 30.0, 10.0),
    ]), 10);
    assert!(matches!(&physical(&replacement)[0].kind, ActionKind::Click { purpose, .. } if purpose == "seat:c"));
    let completion_retry = ack(&mut engine, &replacement, 11);
    assert!(matches!(&physical(&completion_retry)[0].kind, ActionKind::CompleteSelection { .. }));
    assert_eq!(engine.snapshot().attempted_ids, vec!["b", "c"]);
}

#[test]
fn independent_seats_multiple_late_failures_share_one_escape_and_allow_cancelled_retry() {
    let mut p = independent_seats_profile();
    p.target_seats = 3;
    let mut engine = Engine::new(p).unwrap();
    engine.step(Event::Start { mode: RunMode::Seats }, 0);
    let first = engine.step(observed(vec![candidate("a", 10.0, 10.0), candidate("b", 20.0, 10.0), candidate("c", 30.0, 10.0)]), 1);
    let second = ack(&mut engine, &first, 2);
    let third = ack(&mut engine, &second, 3);
    let attempts = engine.snapshot().delivered_attempts;
    let escape = delivered_result(&mut engine, &attempts[0], SeatOutcome::Conflict, 4);
    assert!(delivered_result(&mut engine, &attempts[1], SeatOutcome::Conflict, 5).is_empty());
    assert_eq!(engine.snapshot().pending_action_id, Some(physical(&escape)[0].id));
    assert_eq!(engine.snapshot().rejected_ids, vec!["a", "b"]);
    assert!(engine.snapshot().attempted_ids.is_empty());
    assert!(ack(&mut engine, &third, 6).is_empty());
    engine.step(Event::ActionCancelled { id: physical(&escape)[0].id }, 7);
    let retry = delivered_result(&mut engine, &attempts[0], SeatOutcome::Conflict, 8);
    assert!(matches!(&physical(&retry)[0].kind, ActionKind::Key { key: Key::Escape, .. }));
    assert_ne!(physical(&retry)[0].id, physical(&escape)[0].id);
    engine.step(Event::ConflictDialogGone, 9);
    assert!(ack(&mut engine, &retry, 10).is_empty());
    let replacement = engine.step(observed(vec![candidate("c", 30.0, 10.0)]), 11);
    assert!(matches!(&physical(&replacement)[0].kind, ActionKind::Click { purpose, .. } if purpose == "seat:c"));
}

#[test]
fn independent_seats_action_correlated_results_reject_stale_retry_and_previous_run() {
    let mut engine = Engine::new(independent_seats_profile()).unwrap();
    engine.step(Event::Start { mode: RunMode::Seats }, 0);
    let click = engine.step(observed(vec![candidate("same", 10.0, 10.0)]), 1);
    ack(&mut engine, &click, 2);
    let old = engine.snapshot().delivered_attempts[0].clone();
    delivered_result(&mut engine, &old, SeatOutcome::Conflict, 3);
    engine.step(Event::ConflictDialogGone, 4);
    engine.step(Event::RefreshCompleted { generation: 1 }, 5);
    let retry = engine.step(Event::CandidatesObserved { generation: 1, candidates: vec![candidate("same", 10.0, 10.0)] }, 6);
    ack(&mut engine, &retry, 7);
    let current = engine.snapshot().delivered_attempts[0].clone();
    assert_ne!(old.action_id, current.action_id);
    assert!(delivered_result(&mut engine, &old, SeatOutcome::Conflict, 8).is_empty());
    assert!(delivered_result(&mut engine, &old, SeatOutcome::Selected, 9).is_empty());
    assert_eq!(engine.snapshot().attempted_ids, vec!["same"]);
    assert!(engine.snapshot().selected_ids.is_empty());
    engine.step(Event::Stop, 10);
    engine.step(Event::Start { mode: RunMode::Seats }, 11);
    let fresh = engine.step(observed(vec![candidate("same", 10.0, 10.0)]), 12);
    ack(&mut engine, &fresh, 13);
    assert!(delivered_result(&mut engine, &current, SeatOutcome::Conflict, 14).is_empty());
    assert_eq!(engine.snapshot().attempted_ids, vec!["same"]);
    assert!(engine.snapshot().rejected_ids.is_empty());
}

#[test]
fn independent_seats_optional_conflict_recovery_off_still_excludes_only_failed_attempt() {
    let mut p = independent_seats_profile();
    p.features.conflict_recovery = false;
    let mut engine = Engine::new(p).unwrap();
    engine.step(Event::Start { mode: RunMode::Seats }, 0);
    let first = engine.step(observed(vec![candidate("a", 10.0, 10.0), candidate("b", 20.0, 10.0)]), 1);
    let second = ack(&mut engine, &first, 2);
    ack(&mut engine, &second, 3);
    let failed = engine.snapshot().delivered_attempts[1].clone();
    assert!(delivered_result(&mut engine, &failed, SeatOutcome::Conflict, 4).is_empty());
    assert_eq!(engine.snapshot().phase, Phase::Conflict);
    assert_eq!(engine.snapshot().attempted_ids, vec!["a"]);
    assert_eq!(engine.snapshot().rejected_ids, vec!["b"]);
    assert!(engine.snapshot().pending_action_id.is_none());
    engine.step(Event::ConflictDialogGone, 5);
    let replacement = engine.step(observed(vec![candidate("c", 30.0, 10.0)]), 6);
    assert!(matches!(&physical(&replacement)[0].kind, ActionKind::Click { purpose, .. } if purpose == "seat:c"));
}

#[test]
fn additions_default_off_and_old_coordinate_flow_needs_no_hidden_coordinates() {
    let p = Profile::from_json("{}").unwrap();
    let flags = serde_json::to_value(&p.features).unwrap();
    assert!(flags.as_object().unwrap().values().all(|v| v.as_bool() == Some(false)));
    assert!(p.legacy.blind_click_sequence);
    let mut engine = Engine::new(p).unwrap();
    let actions = engine.step(Event::Start { mode: RunMode::Normal }, 0);
    assert_eq!(engine.snapshot().phase, Phase::Seats);
    assert!(physical(&actions).is_empty());
}

#[test]
fn input_ack_is_not_selection_evidence_in_recovery_mode() {
    let mut engine = Engine::new(recovery_profile(1)).unwrap();
    start(&mut engine);
    let actions = engine.step(observed(vec![candidate("a", 20.0, 30.0)]), 1);
    assert!(matches!(actions[0].kind, ActionKind::Observe { .. }));
    assert!(matches!(actions[1].kind, ActionKind::Click { .. }));
    ack(&mut engine, &actions, 2);
    assert!(engine.snapshot().selected_ids.is_empty());
    assert!(engine.snapshot().attempted_ids.is_empty());
    assert!(engine.step(Event::Tick, 100).is_empty());
    engine.step(Event::CompletionConfirmed, 101);
    assert_eq!(engine.snapshot().phase, Phase::Seats);
    selected(&mut engine, "a", 102);
    assert_eq!(engine.snapshot().phase, Phase::Completing);
    engine.step(Event::CompletionConfirmed, 103);
    assert_eq!(engine.snapshot().phase, Phase::Completed);
}

#[test]
fn legacy_click_sequence_stops_on_completion_delivery_without_confirming_attempts() {
    let mut p = Profile::default();
    p.target_seats = 2;
    p.legacy.complete = Some(target(500.0, 500.0));
    let mut engine = Engine::new(p).unwrap();
    start(&mut engine);
    let first = engine.step(observed(vec![candidate("a", 20.0, 30.0), candidate("b", 40.0, 30.0)]), 1);
    let second = ack(&mut engine, &first, 2);
    let complete = ack(&mut engine, &second, 3);
    assert!(matches!(physical(&complete)[0].kind, ActionKind::CompleteSelection { .. }));
    ack(&mut engine, &complete, 4);
    assert_eq!(engine.snapshot().phase, Phase::Completed);
    assert!(engine.snapshot().selected_ids.is_empty());
    assert_eq!(engine.snapshot().attempted_ids, vec!["a", "b"]);
    engine.step(Event::CompletionConfirmed, 5);
    assert_eq!(engine.snapshot().phase, Phase::Completed);
    assert!(engine.snapshot().selected_ids.is_empty());
    assert_eq!(engine.snapshot().attempted_ids, vec!["a", "b"]);
}

#[test]
fn partial_failure_retains_one_and_fills_exactly_one_shortfall() {
    let mut engine = Engine::new(recovery_profile(2)).unwrap();
    start(&mut engine);
    engine.step(observed(vec![candidate("a", 10.0, 10.0)]), 1);
    selected(&mut engine, "a", 2);
    engine.step(observed(vec![candidate("b", 20.0, 10.0)]), 3);
    let escape = conflict(&mut engine, "b", 4);
    assert!(matches!(physical(&escape)[0].kind, ActionKind::Key { key: Key::Escape, .. }));
    assert!(conflict(&mut engine, "b", 5).is_empty());
    ack(&mut engine, &escape, 6);
    assert_eq!(engine.snapshot().phase, Phase::Conflict);
    engine.step(Event::ConflictDialogGone, 7);
    assert_eq!(engine.snapshot().selected_ids, vec!["a"]);
    assert_eq!(engine.snapshot().rejected_ids, vec!["b"]);
    let wait = engine.step(observed(vec![candidate("b", 20.0, 10.0), candidate("c", 30.0, 10.0)]), 8);
    assert!(physical(&wait).is_empty());
    let next = engine.step(Event::SelectionObserved { selected_ids: vec!["a".into()] }, 9);
    assert!(matches!(&physical(&next)[0].kind, ActionKind::Click { purpose, .. } if purpose == "seat:c"));
    selected(&mut engine, "c", 10);
    assert_eq!(engine.snapshot().selected_ids, vec!["a", "c"]);
    assert_eq!(engine.snapshot().phase, Phase::Completing);
}

#[test]
fn selection_reconciliation_handles_server_clearing_all_seats() {
    let mut engine = Engine::new(recovery_profile(2)).unwrap();
    start(&mut engine);
    engine.step(observed(vec![candidate("a", 1.0, 1.0)]), 1);
    selected(&mut engine, "a", 2);
    engine.step(observed(vec![candidate("b", 2.0, 1.0)]), 3);
    conflict(&mut engine, "b", 4);
    engine.step(Event::ConflictDialogGone, 5);
    engine.step(Event::SelectionObserved { selected_ids: vec![] }, 6);
    assert!(engine.snapshot().selected_ids.is_empty());
    let next = engine.step(observed(vec![candidate("c", 3.0, 1.0)]), 7);
    assert_eq!(physical(&next).len(), 1);
    selected(&mut engine, "c", 8);
    assert_eq!(engine.snapshot().phase, Phase::Seats);
}

#[test]
fn failed_candidates_return_only_after_confirmed_new_refresh_generation() {
    let mut engine = Engine::new(recovery_profile(1)).unwrap();
    start(&mut engine);
    engine.step(observed(vec![candidate("a", 10.0, 10.0)]), 1);
    conflict(&mut engine, "a", 2);
    engine.step(Event::ConflictDialogGone, 3);
    assert!(physical(&engine.step(observed(vec![candidate("a", 10.0, 10.0)]), 4)).is_empty());
    assert!(engine.step(Event::CandidatesObserved { generation: 1, candidates: vec![candidate("a", 10.0, 10.0)] }, 5).is_empty());
    assert_eq!(engine.snapshot().rejected_ids, vec!["a"]);
    engine.step(Event::RefreshCompleted { generation: 1 }, 6);
    assert!(engine.snapshot().rejected_ids.is_empty());
    assert_eq!(physical(&engine.step(Event::CandidatesObserved { generation: 1, candidates: vec![candidate("a", 10.0, 10.0)] }, 7)).len(), 1);
}

#[test]
fn empty_live_frames_do_not_postpone_refresh_forever() {
    let mut p = recovery_profile(1);
    p.features.automatic_refresh = true;
    p.refresh_interval_ms = 100;
    p.refresh_target = Some(target(40.0, 70.0));
    let mut engine = Engine::new(p).unwrap();
    engine.step(Event::Start { mode: RunMode::Cancellation }, 0);
    for time in [1, 20, 40, 60, 80, 99] { engine.step(observed(vec![]), time); }
    let actions = engine.step(Event::Tick, 100);
    assert!(matches!(physical(&actions)[0].kind, ActionKind::Refresh { .. }));
    ack(&mut engine, &actions, 101);
    assert_eq!(engine.snapshot().phase, Phase::Seats);
    assert!(!engine.snapshot().message.contains("F6"));
    let seat = engine.step(observed(vec![candidate("a", 1.0, 1.0)]), 102);
    assert!(matches!(&physical(&seat)[0].kind, ActionKind::Click { purpose, .. } if purpose == "seat:a"));
}

#[test]
fn empty_seat_map_keeps_polling_without_a_conflict_or_page_refresh() {
    for mode in [RunMode::Normal, RunMode::Cancellation] {
        let mut p = Profile::default();
        p.refresh_interval_ms = 100;
        p.observation_timeout_ms = 250;
        let mut engine = Engine::new(p).unwrap();
        engine.step(Event::Start { mode }, 0);
        engine.step(observed(vec![]), 1);
        assert!(engine.step(Event::Tick, 100).is_empty());
        for time in [101, 201, 301, 10_001] {
            let actions = engine.step(Event::Tick, time);
            assert_eq!(actions.len(), 1);
            assert!(matches!(&actions[0].kind, ActionKind::Observe { scope } if scope == "seats"));
            assert!(engine.step(Event::Tick, time + 1).is_empty());
            assert_eq!(engine.snapshot().phase, Phase::Seats);
            assert_eq!(engine.snapshot().generation, 0);
            assert!(engine.snapshot().message.contains("계속 탐색"));
            assert!(!engine.snapshot().message.contains("좌석 선택 결과"));
        }
        let available = engine.step(observed(vec![candidate("new", 20.0, 30.0)]), 10_003);
        assert!(matches!(&physical(&available)[0].kind, ActionKind::Click { purpose, .. } if purpose == "seat:new"));
    }
}

#[test]
fn fresh_seat_observation_does_not_wait_for_the_periodic_poll() {
    let mut p = recovery_profile(1);
    p.refresh_interval_ms = 1000;
    let mut engine = Engine::new(p).unwrap();
    start(&mut engine);
    engine.step(observed(vec![]), 1);
    let available = engine.step(observed(vec![candidate("new", 20.0, 30.0)]), 2);
    assert!(matches!(&physical(&available)[0].kind, ActionKind::Click { purpose, .. } if purpose == "seat:new"));
}

#[test]
fn periodic_poll_preserves_partial_selection_and_rejected_candidates() {
    let mut p = recovery_profile(2);
    p.refresh_interval_ms = 100;
    let mut engine = Engine::new(p).unwrap();
    start(&mut engine);
    engine.step(observed(vec![candidate("kept", 10.0, 10.0)]), 1);
    selected(&mut engine, "kept", 2);
    engine.step(observed(vec![candidate("failed", 20.0, 10.0)]), 3);
    conflict(&mut engine, "failed", 4);
    engine.step(Event::ConflictDialogGone, 5);
    engine.step(Event::SelectionObserved { selected_ids: vec!["kept".into()] }, 6);
    let exhausted = engine.step(observed(vec![
        candidate("kept", 10.0, 10.0), candidate("failed", 20.0, 10.0),
    ]), 7);
    assert!(physical(&exhausted).is_empty());
    let poll = engine.step(Event::Tick, 107);
    assert!(matches!(&poll[0].kind, ActionKind::Observe { scope } if scope == "seats"));
    assert_eq!(engine.snapshot().selected_ids, vec!["kept"]);
    assert_eq!(engine.snapshot().rejected_ids, vec!["failed"]);
    assert_eq!(engine.snapshot().generation, 0);
    let next = engine.step(observed(vec![
        candidate("kept", 10.0, 10.0), candidate("failed", 20.0, 10.0), candidate("new", 30.0, 10.0),
    ]), 108);
    assert!(matches!(&physical(&next)[0].kind, ActionKind::Click { purpose, .. } if purpose == "seat:new"));
    selected(&mut engine, "new", 109);
    assert_eq!(engine.snapshot().phase, Phase::Completing);
    assert_eq!(engine.snapshot().selected_ids, vec!["kept", "new"]);
}

#[test]
fn periodic_search_preserves_confirmed_and_attempted_seats_without_refreshing_them() {
    for recovery in [false, true] {
        let mut p = recovery_profile(2);
        p.features.conflict_recovery = recovery;
        p.features.automatic_refresh = true;
        p.refresh_target = Some(target(40.0, 70.0));
        p.refresh_interval_ms = 100;
        let mut engine = Engine::new(p).unwrap();
        engine.step(Event::Start { mode: RunMode::Cancellation }, 0);
        let click = engine.step(observed(vec![candidate("kept", 10.0, 10.0)]), 1);
        ack(&mut engine, &click, 2);
        if recovery { selected(&mut engine, "kept", 3); }
        engine.step(observed(vec![candidate("kept", 10.0, 10.0)]), 4);
        let poll = engine.step(Event::Tick, 104);
        assert_eq!(poll.len(), 1);
        assert!(matches!(&poll[0].kind, ActionKind::Observe { scope } if scope == "seats"));
        assert!(engine.snapshot().message.contains("새로고침 보류"));
        assert_eq!(engine.snapshot().phase, Phase::Seats);
        if recovery {
            assert_eq!(engine.snapshot().selected_ids, vec!["kept"]);
        } else {
            assert_eq!(engine.snapshot().attempted_ids, vec!["kept"]);
        }
        let remaining = engine.step(observed(vec![
            candidate("kept", 10.0, 10.0), candidate("new", 20.0, 10.0),
        ]), 105);
        assert!(matches!(&physical(&remaining)[0].kind, ActionKind::Click { purpose, .. } if purpose == "seat:new"));
    }
}

#[test]
fn empty_search_poll_never_retries_a_click_with_an_unknown_result() {
    let mut p = recovery_profile(2);
    p.refresh_interval_ms = 100;
    p.observation_timeout_ms = 250;
    let mut engine = Engine::new(p).unwrap();
    start(&mut engine);
    let clicked = engine.step(observed(vec![candidate("uncertain", 10.0, 10.0)]), 1);
    ack(&mut engine, &clicked, 2);
    for time in [102, 252, 10_002] {
        assert!(engine.step(observed(vec![]), time).is_empty());
        assert!(engine.step(Event::Tick, time).is_empty());
        assert_eq!(engine.snapshot().current_candidate_id.as_deref(), Some("uncertain"));
        assert!(engine.snapshot().selected_ids.is_empty());
        assert!(engine.snapshot().attempted_ids.is_empty());
    }
    assert!(engine.snapshot().message.contains("좌석 선택 결과"));
}

#[test]
fn repeated_empty_polls_do_not_hide_an_input_delivery_timeout() {
    let mut p = recovery_profile(1);
    p.refresh_interval_ms = 10;
    p.action_timeout_ms = 100;
    let mut engine = Engine::new(p).unwrap();
    start(&mut engine);
    engine.step(observed(vec![]), 1);
    let poll = engine.step(Event::Tick, 11);
    assert!(matches!(&poll[0].kind, ActionKind::Observe { scope } if scope == "seats"));
    engine.step(observed(vec![candidate("new", 20.0, 30.0)]), 12);
    assert!(engine.step(Event::Tick, 22).is_empty());
    assert!(engine.step(Event::Tick, 112).is_empty());
    assert_eq!(engine.snapshot().phase, Phase::Faulted);
}

#[test]
fn unexpected_ack_and_late_selection_events_do_not_affect_new_runs() {
    let mut engine = Engine::new(recovery_profile(1)).unwrap();
    start(&mut engine);
    let old = engine.step(observed(vec![candidate("a", 1.0, 1.0)]), 1);
    engine.step(Event::Stop, 2);
    start(&mut engine);
    ack(&mut engine, &old, 3);
    selected(&mut engine, "a", 4);
    assert!(engine.snapshot().selected_ids.is_empty());
    let new = engine.step(observed(vec![candidate("b", 2.0, 1.0)]), 5);
    assert!(physical(&new)[0].id > physical(&old)[0].id);
}

#[test]
fn pause_drops_stale_candidate_frame_and_resume_waits_for_current_click_result() {
    let mut engine = Engine::new(recovery_profile(2)).unwrap();
    start(&mut engine);
    let action = engine.step(observed(vec![candidate("a", 1.0, 1.0)]), 1);
    engine.step(Event::Pause, 2);
    ack(&mut engine, &action, 3);
    assert_eq!(engine.snapshot().phase, Phase::Paused);
    assert!(physical(&engine.step(Event::Resume, 4)).is_empty());
    assert!(physical(&engine.step(observed(vec![candidate("b", 2.0, 1.0)]), 5)).is_empty());
    selected(&mut engine, "a", 6);
    assert_eq!(engine.snapshot().selected_ids, vec!["a"]);
}

#[test]
fn unacknowledged_input_faults_without_retrying_the_click() {
    let mut p = recovery_profile(1);
    p.action_timeout_ms = 20;
    let mut engine = Engine::new(p).unwrap();
    start(&mut engine);
    engine.step(observed(vec![candidate("a", 1.0, 1.0)]), 1);
    assert!(engine.step(Event::Tick, 21).is_empty());
    assert_eq!(engine.snapshot().phase, Phase::Faulted);
}

fn entry(stage: EntryStage, enabled: bool, value: Option<&str>) -> Event {
    Event::EntryObserved { stage, target: Some(Target::Element { id: format!("{stage:?}") }), enabled,
        matched_value: value.map(str::to_string), screen_id: "same-window".into() }
}

#[test]
fn timed_multi_stage_entry_uses_enabled_controls_and_exact_date_session() {
    let mut p = recovery_profile(1);
    p.features.server_clock = true;
    p.features.auto_entry = true;
    p.sale_deadline_ms = Some(100);
    p.desired_date = Some("2027-01-14".into());
    p.desired_session = Some("19:00".into());
    let mut engine = Engine::new(p).unwrap();
    start(&mut engine);
    assert_eq!(engine.snapshot().phase, Phase::WaitingSale);
    assert!(engine.step(Event::Tick, 99).is_empty());
    engine.step(Event::Tick, 100);
    let product = engine.step(entry(EntryStage::ProductBooking, true, None), 101);
    ack(&mut engine, &product, 102);
    assert!(engine.step(entry(EntryStage::SaleGate, false, None), 103).is_empty());
    let gate = engine.step(entry(EntryStage::SaleGate, true, None), 104);
    ack(&mut engine, &gate, 105);
    assert!(engine.step(entry(EntryStage::Calendar, true, Some("2027-01-16")), 106).is_empty());
    let date = engine.step(entry(EntryStage::Calendar, true, Some("2027-01-14")), 107);
    ack(&mut engine, &date, 108);
    assert!(engine.step(entry(EntryStage::Session, true, Some("17:00")), 109).is_empty());
    let session = engine.step(entry(EntryStage::Session, true, Some("19:00")), 110);
    ack(&mut engine, &session, 111);
    let confirm = engine.step(entry(EntryStage::ConfirmBooking, true, None), 112);
    ack(&mut engine, &confirm, 113);
    engine.step(entry(EntryStage::SeatMap, true, None), 114);
    assert_eq!(engine.snapshot().phase, Phase::Seats);
}

#[test]
fn duplicate_entry_observation_does_not_repeat_the_same_click() {
    let mut p = recovery_profile(1);
    p.features.auto_entry = true;
    let mut engine = Engine::new(p).unwrap();
    start(&mut engine);
    let click = engine.step(entry(EntryStage::ProductBooking, true, None), 1);
    ack(&mut engine, &click, 2);
    assert!(engine.step(entry(EntryStage::ProductBooking, true, None), 3).is_empty());
}

#[test]
fn captcha_requires_positive_completion_and_obeys_optional_auto_resume() {
    for auto in [false, true] {
        let mut p = recovery_profile(1);
        p.features.auto_captcha_resume = auto;
        let mut engine = Engine::new(p).unwrap();
        start(&mut engine);
        engine.step(entry(EntryStage::Captcha, true, None), 1);
        engine.step(Event::Resume, 2);
        engine.step(entry(EntryStage::SeatMap, true, None), 3);
        assert_eq!(engine.snapshot().phase, Phase::Captcha);
        engine.step(Event::CaptchaCompleted, 4);
        assert_eq!(engine.snapshot().phase, if auto { Phase::Seats } else { Phase::Captcha });
        if !auto { engine.step(Event::Resume, 5); }
        assert_eq!(engine.snapshot().phase, Phase::Seats);
    }
}

#[test]
fn region_click_waits_for_identified_seat_map_before_any_seat_click() {
    let mut p = recovery_profile(1);
    p.features.region_priority = true;
    let mut engine = Engine::new(p).unwrap();
    start(&mut engine);
    let region = engine.step(Event::RegionsObserved { candidates: vec![RegionCandidate {
        candidate: candidate("zone", 50.0, 60.0), center_rank: 0, distance_to_stage: 30.0,
    }] }, 1);
    ack(&mut engine, &region, 2);
    assert_eq!(engine.snapshot().phase, Phase::WaitingRegion);
    assert!(engine.snapshot().message.contains("구역 좌석 화면이 보이면 F6"));
    assert!(engine.step(observed(vec![candidate("legend", 1.0, 1.0)]), 3).is_empty());
    assert!(engine.step(Event::Tick, 5_002).is_empty());
    assert_eq!(engine.snapshot().phase, Phase::WaitingRegion);
    assert!(engine.snapshot().message.contains("구역 좌석 화면이 보이면 F6"));
    engine.step(Event::RegionMapReady, 5_003);
    assert_eq!(physical(&engine.step(observed(vec![candidate("seat", 20.0, 30.0)]), 5_004)).len(), 1);
}

#[test]
fn explicit_region_map_confirmation_handles_already_open_and_paused_seat_maps() {
    for mode in [RunMode::Normal, RunMode::Cancellation] {
        for pause in [false, true] {
            let mut p = recovery_profile(1);
            p.features.region_priority = true;
            if mode == RunMode::Cancellation {
                p.legacy.cancellation_navigation = CancellationNavigation::ArrowKeys;
            }
            let mut engine = Engine::new(p).unwrap();
            assert!(physical(&engine.step(Event::Start { mode }, 0)).is_empty());
            assert_eq!(engine.confirmation_phase(), Phase::Region);
            if pause {
                engine.step(Event::ContextLost { reason: "settings took focus".into() }, 1);
                assert_eq!(engine.snapshot().phase, Phase::Paused);
                assert_eq!(engine.confirmation_phase(), Phase::Region);
                // Asking what can be confirmed never resumes or advances implicitly.
                assert!(engine.step(Event::RegionMapReady, 2).is_empty());
                assert_eq!(engine.snapshot().phase, Phase::Paused);
                assert!(physical(&engine.step(Event::Resume, 3)).is_empty());
            }
            let ready = engine.step(Event::RegionMapReady, 4);
            assert!(physical(&ready).is_empty());
            assert!(ready.iter().any(|a| matches!(&a.kind, ActionKind::Observe { scope } if scope == "seats")));
            assert_eq!(engine.snapshot().phase, Phase::Seats);
            assert_eq!(engine.confirmation_phase(), Phase::Seats);
            assert!(engine.snapshot().pending_action_id.is_none());
            let seat = engine.step(observed(vec![candidate("seat", 20.0, 30.0)]), 5);
            assert_eq!(physical(&seat).len(), 1);
            assert!(matches!(&physical(&seat)[0].kind, ActionKind::Click { purpose, .. } if purpose == "seat:seat"));
        }
    }
}

#[test]
fn explicit_region_map_confirmation_invalidates_queued_region_input_and_late_ack() {
    let mut p = recovery_profile(1);
    p.features.region_priority = true;
    let mut engine = Engine::new(p).unwrap();
    start(&mut engine);
    let region = engine.step(Event::RegionsObserved { candidates: vec![RegionCandidate {
        candidate: candidate("zone", 50.0, 60.0), center_rank: 0, distance_to_stage: 30.0,
    }] }, 1);
    let old_id = physical(&region)[0].id;
    assert_eq!(engine.snapshot().pending_action_id, Some(old_id));
    assert!(physical(&engine.step(Event::RegionMapReady, 2)).is_empty());
    assert_eq!(engine.snapshot().phase, Phase::Seats);
    // The runtime only sends inputs whose ID still equals pending_action_id.
    assert!(engine.snapshot().pending_action_id.is_none());
    assert!(ack(&mut engine, &region, 3).is_empty());
    assert_eq!(engine.snapshot().phase, Phase::Seats);
    let seat = engine.step(observed(vec![candidate("seat", 20.0, 30.0)]), 4);
    let seat_id = physical(&seat)[0].id;
    assert_ne!(old_id, seat_id);
    assert!(engine.step(Event::ActionAcknowledged { id: old_id, success: false, detail: "late region failure".into() }, 5).is_empty());
    assert!(engine.step(Event::ActionCancelled { id: old_id }, 6).is_empty());
    assert_eq!(engine.snapshot().phase, Phase::Seats);
    assert_eq!(engine.snapshot().pending_action_id, Some(seat_id));
}

#[test]
fn region_map_confirmation_does_not_advance_unrelated_or_paused_phases() {
    let idle = Engine::new(recovery_profile(1)).unwrap();
    let mut cases = vec![idle.clone()];
    let mut seats = idle.clone();
    start(&mut seats);
    cases.push(seats.clone());
    let mut paused = seats.clone();
    paused.step(Event::Pause, 1);
    assert_eq!(paused.confirmation_phase(), Phase::Seats);
    cases.push(paused);
    let mut stopped = seats.clone();
    stopped.step(Event::Stop, 1);
    cases.push(stopped);
    let mut captcha = seats.clone();
    captcha.step(entry(EntryStage::Captcha, true, None), 1);
    cases.push(captcha);
    let mut clicked = seats.clone();
    clicked.step(observed(vec![candidate("seat", 20.0, 30.0)]), 1);
    cases.push(clicked.clone());
    let mut conflict_state = clicked.clone();
    conflict(&mut conflict_state, "seat", 2);
    cases.push(conflict_state);
    let mut faulted = clicked.clone();
    faulted.step(Event::Tick, 2_001);
    cases.push(faulted);
    selected(&mut clicked, "seat", 2);
    cases.push(clicked.clone());
    clicked.step(Event::CompletionConfirmed, 3);
    cases.push(clicked);
    let mut p = recovery_profile(1);
    p.features.server_clock = true;
    p.sale_deadline_ms = Some(100_000);
    let mut waiting_sale = Engine::new(p).unwrap();
    start(&mut waiting_sale);
    cases.push(waiting_sale);
    let mut p = recovery_profile(1);
    p.entry_target = Some(target(10.0, 20.0));
    let mut entering = Engine::new(p).unwrap();
    start(&mut entering);
    cases.push(entering);
    let mut p = recovery_profile(1);
    p.features.automatic_refresh = true;
    p.refresh_target = Some(target(40.0, 70.0));
    let mut refreshing = Engine::new(p).unwrap();
    refreshing.step(Event::Start { mode: RunMode::Cancellation }, 0);
    refreshing.step(observed(vec![]), 1);
    refreshing.step(Event::Tick, 1000);
    cases.push(refreshing);
    for mut engine in cases {
        let before = serde_json::to_value(engine.snapshot()).unwrap();
        assert!(engine.step(Event::RegionMapReady, 10_000).is_empty());
        assert_eq!(serde_json::to_value(engine.snapshot()).unwrap(), before);
    }
}

#[test]
fn completion_target_can_be_discovered_after_booking_opens() {
    let mut p = recovery_profile(1);
    p.legacy.complete = None;
    let mut engine = Engine::new(p).unwrap();
    start(&mut engine);
    engine.step(observed(vec![candidate("a", 1.0, 1.0)]), 1);
    assert!(physical(&selected(&mut engine, "a", 2)).is_empty());
    assert_eq!(engine.snapshot().phase, Phase::Completing);
    let complete = engine.step(Event::CompletionTargetObserved { target: target(90.0, 80.0) }, 3);
    assert!(matches!(physical(&complete)[0].kind, ActionKind::CompleteSelection { .. }));
}

#[test]
fn late_conflict_at_selection_complete_reopens_only_missing_seat() {
    let mut engine = Engine::new(recovery_profile(2)).unwrap();
    start(&mut engine);
    engine.step(observed(vec![candidate("a", 1.0, 1.0)]), 1);
    selected(&mut engine, "a", 2);
    engine.step(observed(vec![candidate("b", 2.0, 1.0)]), 3);
    selected(&mut engine, "b", 4);
    assert_eq!(engine.snapshot().phase, Phase::Completing);
    conflict(&mut engine, "b", 5);
    engine.step(Event::ConflictDialogGone, 6);
    engine.step(Event::SelectionObserved { selected_ids: vec!["a".into()] }, 7);
    assert_eq!(engine.snapshot().selected_ids, vec!["a"]);
    assert_eq!(engine.snapshot().phase, Phase::Seats);
    assert_eq!(engine.snapshot().rejected_ids, vec!["b"]);
}

#[test]
fn original_optional_date_session_next_sequence_and_delays_remain() {
    let mut p = Profile::default();
    p.legacy.date = Some(target(10.0, 10.0));
    p.legacy.session = Some(target(20.0, 10.0));
    p.legacy.next = Some(target(30.0, 10.0));
    p.legacy.normal_click_delay_ms = 10;
    let mut engine = Engine::new(p).unwrap();
    let first = engine.step(Event::Start { mode: RunMode::Normal }, 0);
    assert_eq!(physical(&first).len(), 1);
    assert!(ack(&mut engine, &first, 1).is_empty());
    assert!(engine.step(Event::Tick, 10).is_empty());
    let second = engine.step(Event::Tick, 11);
    ack(&mut engine, &second, 12);
    let third = engine.step(Event::Tick, 22);
    ack(&mut engine, &third, 23);
    engine.step(Event::Tick, 33);
    assert_eq!(engine.snapshot().phase, Phase::Seats);
}

#[test]
fn enhanced_cancellation_navigation_requires_confirmed_updates() {
    let mut p = Profile::default();
    p.features.conflict_recovery = true;
    p.refresh_interval_ms = 10;
    p.legacy.region_delay_ms = 10;
    p.legacy.cancellation_navigation = CancellationNavigation::ArrowKeys;
    let mut engine = Engine::new(p).unwrap();
    let down = engine.step(Event::Start { mode: RunMode::Cancellation }, 0);
    assert!(matches!(physical(&down)[0].kind, ActionKind::Key { key: Key::ArrowDown, .. }));
    ack(&mut engine, &down, 1);
    assert!(engine.step(Event::Tick, 11).is_empty());
    engine.step(Event::RefreshCompleted { generation: 1 }, 12);
    engine.step(Event::CandidatesObserved { generation: 1, candidates: vec![] }, 13);
    let up = engine.step(Event::Tick, 13);
    assert!(matches!(physical(&up)[0].kind, ActionKind::Key { key: Key::ArrowUp, .. }));
}

#[test]
fn original_cancellation_blank_maps_keep_alternating_without_manual_confirmation() {
    let mut p = Profile::default();
    p.legacy.region_delay_ms = 10;
    p.legacy.cancellation_navigation = CancellationNavigation::ArrowKeys;
    let mut engine = Engine::new(p).unwrap();
    let mut navigation = engine.step(Event::Start { mode: RunMode::Cancellation }, 0);
    let mut now = 1;
    for generation in 0..20 {
        let expected = if generation % 2 == 0 { Key::ArrowDown } else { Key::ArrowUp };
        assert!(matches!(physical(&navigation)[0].kind, ActionKind::Key { key, .. } if key == expected));
        assert_eq!(engine.snapshot().active_region, Some(generation as usize % 2));
        ack(&mut engine, &navigation, now + 1);
        assert!(engine.step(Event::Tick, now + 10).is_empty());
        let resume = engine.step(Event::Tick, now + 11);
        assert!(physical(&resume).is_empty());
        assert_eq!(engine.snapshot().phase, Phase::Seats);
        assert_eq!(engine.snapshot().generation, generation + 1);
        assert!(engine.snapshot().selected_ids.is_empty());
        now += 12;
        engine.step(Event::CandidatesObserved { generation: generation + 1, candidates: vec![] }, now);
        navigation = engine.step(Event::Tick, now);
    }
}

#[test]
fn original_timed_region_wait_does_not_request_manual_confirmation() {
    let mut p = Profile::default();
    p.legacy.region_delay_ms = 6000;
    p.legacy.cancellation_navigation = CancellationNavigation::ArrowKeys;
    let mut engine = Engine::new(p).unwrap();
    let navigation = engine.step(Event::Start { mode: RunMode::Cancellation }, 0);
    ack(&mut engine, &navigation, 1);
    assert!(engine.step(Event::Tick, 5_001).is_empty());
    assert_eq!(engine.snapshot().phase, Phase::WaitingRefresh);
    assert!(!engine.snapshot().message.contains("F6"));
    assert!(engine.snapshot().message.contains("지정한 대기 시간"));
    engine.step(Event::Tick, 6_001);
    assert_eq!(engine.snapshot().phase, Phase::Seats);
}

fn two_region_profile() -> Profile {
    let mut p = Profile::default();
    p.color_tolerance = 0;
    p.legacy.cancellation_navigation = CancellationNavigation::TwoRegions {
        first: target(100.0, 10.0), second: target(200.0, 10.0),
    };
    p.legacy.regions = vec![
        LegacyRegionSettings {
            color: Some(Color { r: 10, g: 20, b: 30 }),
            vision: Some(VisionConfig { roi: Some(Rect { x: 0.0, y: 0.0, width: 10.0, height: 10.0 }), ..VisionConfig::default() }),
            complete: Some(target(100.0, 100.0)),
        },
        LegacyRegionSettings {
            color: Some(Color { r: 30, g: 20, b: 10 }),
            vision: Some(VisionConfig { roi: Some(Rect { x: 20.0, y: 20.0, width: 10.0, height: 10.0 }), ..VisionConfig::default() }),
            complete: Some(target(200.0, 200.0)),
        },
    ];
    p
}

#[test]
fn original_f2_uses_each_regions_color_roi_and_completion_target() {
    for desired_region in 0..2 {
        let p = two_region_profile();
        let expected = p.legacy.regions[desired_region].clone();
        let mut engine = Engine::new(p).unwrap();
        let first = engine.step(Event::Start { mode: RunMode::Cancellation }, 0);
        assert!(matches!(&physical(&first)[0].kind, ActionKind::Click { target: clicked, .. } if clicked == &target(100.0, 10.0)));
        ack(&mut engine, &first, 1);
        engine.step(Event::Tick, 1);
        let mut generation = 1;
        if desired_region == 1 {
            engine.step(Event::CandidatesObserved { generation, candidates: vec![] }, 2);
            let second = engine.step(Event::Tick, 2);
            assert!(matches!(&physical(&second)[0].kind, ActionKind::Click { target: clicked, .. } if clicked == &target(200.0, 10.0)));
            ack(&mut engine, &second, 3);
            engine.step(Event::Tick, 3);
            generation = 2;
        }
        assert_eq!(engine.snapshot().active_region, Some(desired_region));
        assert_eq!(engine.effective_color(), expected.color.unwrap());
        let roi = expected.vision.as_ref().unwrap().roi.unwrap();
        let mut right = candidate("right", roi.x + 1.0, roi.y + 1.0);
        right.color = expected.color.unwrap();
        let mut wrong_position = right.clone();
        wrong_position.id = "outside".into();
        wrong_position.point = Point { x: 300.0, y: 300.0 };
        wrong_position.target = target(300.0, 300.0);
        let wrong_color = candidate("wrong-color", roi.x + 2.0, roi.y + 2.0);
        let click = engine.step(Event::CandidatesObserved { generation, candidates: vec![wrong_color, wrong_position, right] }, 4);
        assert!(matches!(&physical(&click)[0].kind, ActionKind::Click { purpose, .. } if purpose == "seat:right"));
        let complete = ack(&mut engine, &click, 5);
        assert!(matches!(&physical(&complete)[0].kind, ActionKind::CompleteSelection { target } if Some(target) == expected.complete.as_ref()));
    }
}

#[test]
fn normal_and_single_region_cancellation_ignore_legacy_region_overrides() {
    let mut p = two_region_profile();
    let global_color = p.color;
    let mut normal = Engine::new(p.clone()).unwrap();
    start(&mut normal);
    assert_eq!(normal.snapshot().active_region, None);
    assert_eq!(normal.effective_color(), global_color);
    p.legacy.cancellation_navigation = CancellationNavigation::None;
    let global_roi = p.vision.roi;
    let global_complete = p.legacy.complete.clone();
    let mut cancellation = Engine::new(p).unwrap();
    cancellation.step(Event::Start { mode: RunMode::Cancellation }, 0);
    assert_eq!(cancellation.snapshot().active_region, None);
    assert_eq!(cancellation.effective_color(), global_color);
    assert_eq!(cancellation.effective_vision().roi, global_roi);
    assert_eq!(cancellation.effective_completion_target(), global_complete.as_ref());
    assert!(cancellation.continuous_search());
}

#[test]
fn region_settings_validate_limits_and_can_change_during_pause() {
    let mut p = Profile::default();
    p.legacy.regions = vec![LegacyRegionSettings::default(); 3];
    assert!(p.validate().is_err());
    p.legacy.regions = vec![LegacyRegionSettings {
        vision: Some(VisionConfig { roi: Some(Rect::default()), ..VisionConfig::default() }),
        ..LegacyRegionSettings::default()
    }];
    assert!(p.validate().is_err());
    let mut engine = Engine::new(recovery_profile(2)).unwrap();
    start(&mut engine);
    assert!(engine.update_legacy_regions(vec![]).is_err());
    engine.step(Event::Pause, 1);
    engine.update_legacy_regions(two_region_profile().legacy.regions).unwrap();
    assert_eq!(engine.profile().legacy.regions.len(), 2);
}

#[test]
fn new_single_region_priority_does_not_alternate_original_f2_regions() {
    let mut p = two_region_profile();
    p.features.region_priority = true;
    let mut engine = Engine::new(p).unwrap();
    let actions = engine.step(Event::Start { mode: RunMode::Cancellation }, 0);
    assert!(physical(&actions).is_empty());
    assert_eq!(engine.snapshot().active_region, None);
    assert_eq!(engine.snapshot().phase, Phase::Region);
}

#[test]
fn proven_undelivered_seat_cancellation_allows_fresh_retry_after_pause() {
    let mut engine = Engine::new(recovery_profile(1)).unwrap();
    start(&mut engine);
    let first = engine.step(observed(vec![candidate("a", 1.0, 1.0)]), 1);
    engine.step(Event::ActionCancelled { id: physical(&first)[0].id }, 2);
    engine.step(Event::Pause, 3);
    engine.step(Event::Resume, 4);
    assert!(engine.snapshot().current_candidate_id.is_none());
    let retry = engine.step(observed(vec![candidate("a", 1.0, 1.0)]), 5);
    assert_eq!(physical(&retry).len(), 1);
    assert_ne!(physical(&retry)[0].id, physical(&first)[0].id);
}

#[test]
fn acknowledged_seat_cannot_be_rolled_back_as_undelivered() {
    let mut engine = Engine::new(recovery_profile(1)).unwrap();
    start(&mut engine);
    let first = engine.step(observed(vec![candidate("a", 1.0, 1.0)]), 1);
    ack(&mut engine, &first, 2);
    engine.step(Event::ActionCancelled { id: physical(&first)[0].id }, 3);
    engine.step(Event::Pause, 4);
    engine.step(Event::Resume, 5);
    assert_eq!(engine.snapshot().current_candidate_id.as_deref(), Some("a"));
    assert!(physical(&engine.step(observed(vec![candidate("a", 1.0, 1.0)]), 6)).is_empty());
}

#[test]
fn undelivered_legacy_date_is_requeued_instead_of_skipped() {
    let mut p = Profile::default();
    p.legacy.date = Some(target(10.0, 10.0));
    p.legacy.session = Some(target(20.0, 10.0));
    let mut engine = Engine::new(p).unwrap();
    let first = engine.step(Event::Start { mode: RunMode::Normal }, 0);
    engine.step(Event::ActionCancelled { id: physical(&first)[0].id }, 1);
    engine.step(Event::Pause, 2);
    engine.step(Event::Resume, 3);
    let retry = engine.step(Event::Tick, 4);
    assert!(matches!(&physical(&retry)[0].kind, ActionKind::Click { target: clicked, .. } if clicked == &target(10.0, 10.0)));
}

#[test]
fn undelivered_f2_navigation_restores_zone_and_does_not_scan_before_retry() {
    let mut engine = Engine::new(two_region_profile()).unwrap();
    let first = engine.step(Event::Start { mode: RunMode::Cancellation }, 0);
    engine.step(Event::ActionCancelled { id: physical(&first)[0].id }, 1);
    engine.step(Event::Pause, 2);
    engine.step(Event::Resume, 3);
    assert!(engine.step(observed(vec![candidate("stale", 1.0, 1.0)]), 4).is_empty());
    let retry = engine.step(Event::Tick, 5);
    assert!(matches!(&physical(&retry)[0].kind, ActionKind::Click { target: clicked, .. } if clicked == &target(100.0, 10.0)));
    assert_eq!(engine.snapshot().active_region, Some(0));
    assert_eq!(engine.snapshot().generation, 0);
}

#[test]
fn undelivered_completion_can_resume_without_losing_selected_seat() {
    let mut engine = Engine::new(recovery_profile(1)).unwrap();
    start(&mut engine);
    engine.step(observed(vec![candidate("a", 1.0, 1.0)]), 1);
    let complete = selected(&mut engine, "a", 2);
    engine.step(Event::ActionCancelled { id: physical(&complete)[0].id }, 3);
    engine.step(Event::Pause, 4);
    let retry = engine.step(Event::Resume, 5);
    assert!(matches!(physical(&retry)[0].kind, ActionKind::CompleteSelection { .. }));
    assert_eq!(engine.snapshot().selected_ids, vec!["a"]);
}

#[test]
fn manually_closed_known_conflict_can_continue_after_undelivered_escape() {
    let mut engine = Engine::new(recovery_profile(1)).unwrap();
    start(&mut engine);
    engine.step(observed(vec![candidate("a", 1.0, 1.0)]), 1);
    let escape = conflict(&mut engine, "a", 2);
    engine.step(Event::ActionCancelled { id: physical(&escape)[0].id }, 3);
    engine.step(Event::Pause, 4);
    assert!(physical(&engine.step(Event::Resume, 5)).is_empty());
    let retry = conflict(&mut engine, "a", 6);
    assert!(matches!(physical(&retry)[0].kind, ActionKind::Key { key: Key::Escape, .. }));
    engine.step(Event::ActionCancelled { id: physical(&retry)[0].id }, 7);
    engine.step(Event::ConflictDialogGone, 8);
    assert_eq!(engine.snapshot().phase, Phase::Seats);
    assert_eq!(engine.snapshot().rejected_ids, vec!["a"]);
}

fn initial_entry_profile() -> Profile {
    let mut p = recovery_profile(1);
    p.entry_target = Some(target(300.0, 400.0));
    // These belong to another screen and must never run after the initial click.
    p.legacy.date = Some(target(1.0, 1.0));
    p.legacy.session = Some(target(2.0, 2.0));
    p.legacy.next = Some(target(3.0, 3.0));
    p
}

#[test]
fn scheduled_entry_clicks_at_deadline_once_then_waits_for_confirmed_seat_map() {
    let mut p = initial_entry_profile();
    p.features.server_clock = true;
    p.sale_deadline_ms = Some(100);
    let mut engine = Engine::new(p).unwrap();
    assert!(physical(&engine.step(Event::Start { mode: RunMode::Normal }, 0)).is_empty());
    assert!(engine.step(Event::Tick, 99).is_empty());
    let click = engine.step(Event::Tick, 100);
    assert!(matches!(&physical(&click)[0].kind, ActionKind::Click { target: clicked, purpose }
        if clicked == &target(300.0, 400.0) && purpose == "initial_entry"));
    ack(&mut engine, &click, 101);
    assert_eq!(engine.snapshot().phase, Phase::Entry);
    assert!(engine.step(Event::Tick, 102).is_empty());
    assert!(engine.step(observed(vec![candidate("old-screen", 10.0, 10.0)]), 103).is_empty());
    let ready = engine.step(entry(EntryStage::SeatMap, true, None), 104);
    assert!(physical(&ready).is_empty());
    assert_eq!(engine.snapshot().phase, Phase::Seats);
    assert!(physical(&engine.step(Event::Tick, 105)).is_empty());
}

#[test]
fn entry_target_works_with_or_without_semantic_auto_entry_flag() {
    for automatic in [false, true] {
        let mut p = initial_entry_profile();
        p.features.auto_entry = automatic;
        let mut engine = Engine::new(p).unwrap();
        let first = engine.step(Event::Start { mode: RunMode::Normal }, 0);
        assert!(matches!(&physical(&first)[0].kind, ActionKind::Click { purpose, .. } if purpose == "initial_entry"));
        ack(&mut engine, &first, 1);
        let duplicate = Event::EntryObserved {
            stage: EntryStage::ProductBooking, target: Some(target(300.0, 400.0)),
            enabled: true, matched_value: None, screen_id: "product".into(),
        };
        assert!(physical(&engine.step(duplicate, 2)).is_empty());
        let next_stage = engine.step(entry(EntryStage::SaleGate, true, None), 3);
        assert_eq!(physical(&next_stage).len(), usize::from(automatic));
    }
}

#[test]
fn acknowledged_initial_entry_is_not_repeated_after_pause_or_late_cancel() {
    let mut engine = Engine::new(initial_entry_profile()).unwrap();
    let first = engine.step(Event::Start { mode: RunMode::Normal }, 0);
    ack(&mut engine, &first, 1);
    engine.step(Event::ActionCancelled { id: physical(&first)[0].id }, 2);
    engine.step(Event::Pause, 3);
    assert!(physical(&engine.step(Event::Resume, 4)).is_empty());
    assert!(engine.step(Event::Tick, 5).is_empty());
    assert_eq!(engine.snapshot().phase, Phase::Entry);
    engine.step(entry(EntryStage::SeatMap, true, None), 6);
    assert_eq!(engine.snapshot().phase, Phase::Seats);
}

#[test]
fn proven_undelivered_initial_entry_retries_once_after_pause() {
    let mut engine = Engine::new(initial_entry_profile()).unwrap();
    let first = engine.step(Event::Start { mode: RunMode::Normal }, 0);
    engine.step(Event::ActionCancelled { id: physical(&first)[0].id }, 1);
    engine.step(Event::Pause, 2);
    engine.step(Event::Resume, 3);
    let retry = engine.step(Event::Tick, 4);
    assert_eq!(physical(&retry).len(), 1);
    assert_ne!(physical(&retry)[0].id, physical(&first)[0].id);
    assert!(matches!(&physical(&retry)[0].kind, ActionKind::Click { purpose, .. } if purpose == "initial_entry"));
    ack(&mut engine, &retry, 5);
    assert!(engine.step(Event::Tick, 6).is_empty());
}

#[test]
fn pausing_before_sale_keeps_entry_pending_until_resume() {
    let mut p = initial_entry_profile();
    p.features.server_clock = true;
    p.sale_deadline_ms = Some(100);
    let mut engine = Engine::new(p).unwrap();
    start(&mut engine);
    engine.step(Event::Pause, 50);
    assert!(engine.step(Event::Tick, 150).is_empty());
    engine.step(Event::Resume, 151);
    let first = engine.step(Event::Tick, 152);
    assert!(matches!(&physical(&first)[0].kind, ActionKind::Click { purpose, .. } if purpose == "initial_entry"));
}

#[test]
fn captcha_completion_after_initial_entry_does_not_imply_ready_seat_screen() {
    let mut p = initial_entry_profile();
    p.features.auto_captcha_resume = true;
    let mut engine = Engine::new(p).unwrap();
    let first = engine.step(Event::Start { mode: RunMode::Normal }, 0);
    ack(&mut engine, &first, 1);
    engine.step(entry(EntryStage::Captcha, true, None), 2);
    let completed = engine.step(Event::CaptchaCompleted, 3);
    assert!(physical(&completed).is_empty());
    assert_eq!(engine.snapshot().phase, Phase::Entry);
    assert!(engine.step(observed(vec![candidate("old-map", 1.0, 1.0)]), 4).is_empty());
    engine.step(entry(EntryStage::SeatMap, true, None), 5);
    assert_eq!(engine.snapshot().phase, Phase::Seats);
}

#[test]
fn original_f2_ignores_the_normal_booking_entry_target() {
    let mut p = two_region_profile();
    p.entry_target = Some(target(300.0, 400.0));
    let mut engine = Engine::new(p).unwrap();
    let first = engine.step(Event::Start { mode: RunMode::Cancellation }, 0);
    assert!(matches!(&physical(&first)[0].kind, ActionKind::Click { target: clicked, purpose }
        if clicked == &target(100.0, 10.0) && purpose == "legacy_cancellation_region"));
}

#[test]
fn visible_entry_target_validation_rejects_invalid_coordinates() {
    let mut p = Profile::default();
    p.entry_target = Some(target(-1.0, 2.0));
    assert!(p.validate().is_err());
}

#[test]
fn updating_or_clearing_entry_after_delivery_never_reclicks_or_skips_manual_wait() {
    for replacement in [None, Some(target(700.0, 800.0))] {
        let mut engine = Engine::new(initial_entry_profile()).unwrap();
        let first = engine.step(Event::Start { mode: RunMode::Normal }, 0);
        ack(&mut engine, &first, 1);
        engine.step(Event::Pause, 2);
        engine.update_entry_target(replacement).unwrap();
        assert!(physical(&engine.step(Event::Resume, 3)).is_empty());
        assert!(engine.step(Event::Tick, 1_000_000).is_empty());
        assert_eq!(engine.snapshot().phase, Phase::Entry);
        engine.step(entry(EntryStage::SeatMap, true, None), 1_000_001);
        assert_eq!(engine.snapshot().phase, Phase::Seats);
    }
}

#[test]
fn entry_target_cannot_change_while_delivery_is_ambiguous() {
    let mut engine = Engine::new(initial_entry_profile()).unwrap();
    engine.step(Event::Start { mode: RunMode::Normal }, 0);
    // No acknowledgement or proof of non-delivery has arrived.
    engine.step(Event::Pause, 1);
    assert!(engine.update_entry_target(None).is_err());
    assert!(engine.update_entry_target(Some(target(700.0, 800.0))).is_err());
    assert!(engine.update_entry_target(Some(target(300.0, 400.0))).is_ok());
    engine.step(Event::Resume, 2);
    assert!(engine.step(Event::Tick, 100_000).is_empty());
    assert_eq!(engine.snapshot().phase, Phase::Entry);
}

#[test]
fn undelivered_entry_can_be_recalibrated_before_retry() {
    let mut engine = Engine::new(initial_entry_profile()).unwrap();
    let first = engine.step(Event::Start { mode: RunMode::Normal }, 0);
    engine.step(Event::ActionCancelled { id: physical(&first)[0].id }, 1);
    engine.step(Event::Pause, 2);
    engine.update_entry_target(Some(target(700.0, 800.0))).unwrap();
    engine.step(Event::Resume, 3);
    let retry = engine.step(Event::Tick, 4);
    assert!(matches!(&physical(&retry)[0].kind, ActionKind::Click { target: clicked, .. }
        if clicked == &target(700.0, 800.0)));
}

#[test]
fn clearing_scheduled_entry_before_delivery_does_not_fall_through_to_old_coordinates() {
    let mut p = initial_entry_profile();
    p.features.server_clock = true;
    p.sale_deadline_ms = Some(100);
    let mut engine = Engine::new(p).unwrap();
    start(&mut engine);
    engine.step(Event::Pause, 1);
    engine.update_entry_target(None).unwrap();
    engine.step(Event::Resume, 2);
    assert!(physical(&engine.step(Event::Tick, 100)).is_empty());
    assert!(physical(&engine.step(Event::Tick, 101)).is_empty());
    assert_eq!(engine.snapshot().phase, Phase::Entry);
}

#[test]
fn geometry_invalidation_after_entry_clears_old_coordinates_without_replaying_them() {
    for navigation in [
        CancellationNavigation::TwoRegions { first: target(10.0, 10.0), second: target(20.0, 10.0) },
        CancellationNavigation::ArrowKeys,
    ] {
        let mut p = initial_entry_profile();
        let arrow_keys = matches!(navigation, CancellationNavigation::ArrowKeys);
        p.legacy.cancellation_navigation = navigation;
        p.vision.roi = Some(Rect { x: 0.0, y: 0.0, width: 100.0, height: 100.0 });
        p.vision.exclusions = vec![Rect { x: 80.0, y: 80.0, width: 10.0, height: 10.0 }];
        p.legacy.regions = vec![LegacyRegionSettings {
            color: Some(Color { r: 12, g: 34, b: 56 }),
            vision: Some(p.vision.clone()), complete: Some(target(90.0, 90.0)),
        }];
        p.refresh_target = Some(target(400.0, 400.0));
        let mut engine = Engine::new(p).unwrap();
        let first = engine.step(Event::Start { mode: RunMode::Normal }, 0);
        ack(&mut engine, &first, 1);
        assert!(engine.invalidate_coordinate_geometry().is_err());
        engine.step(Event::Pause, 2);
        engine.invalidate_coordinate_geometry().unwrap();
        let p = engine.profile();
        assert!(p.entry_target.is_none());
        assert!(p.refresh_target.is_none());
        assert!([&p.legacy.date, &p.legacy.session, &p.legacy.next, &p.legacy.complete]
            .into_iter().all(Option::is_none));
        assert_eq!(matches!(p.legacy.cancellation_navigation, CancellationNavigation::ArrowKeys), arrow_keys);
        assert!(arrow_keys || matches!(p.legacy.cancellation_navigation, CancellationNavigation::None));
        assert!(p.vision.roi.is_none() && p.vision.exclusions.is_empty());
        assert_eq!(p.legacy.regions[0].color, Some(Color { r: 12, g: 34, b: 56 }));
        assert!(p.legacy.regions[0].vision.is_none() && p.legacy.regions[0].complete.is_none());
        assert_eq!(engine.snapshot().phase, Phase::Paused);
        assert!(physical(&engine.step(Event::Resume, 3)).is_empty());
        assert!(physical(&engine.step(Event::Tick, 1_000_000)).is_empty());
        assert_eq!(engine.snapshot().phase, Phase::Entry);
        assert!(physical(&engine.step(entry(EntryStage::SeatMap, true, None), 1_000_001)).is_empty());
        assert_eq!(engine.snapshot().phase, Phase::Seats);
    }
}

#[test]
fn geometry_invalidation_rejects_ambiguous_initial_delivery_without_mutation() {
    let mut engine = Engine::new(initial_entry_profile()).unwrap();
    engine.step(Event::Start { mode: RunMode::Normal }, 0);
    engine.step(Event::Pause, 1);
    let before = serde_json::to_value(engine.profile()).unwrap();
    assert!(engine.invalidate_coordinate_geometry().is_err());
    assert_eq!(serde_json::to_value(engine.profile()).unwrap(), before);
    assert_eq!(engine.snapshot().phase, Phase::Paused);
    assert!(physical(&engine.step(Event::Resume, 2)).is_empty());
    assert!(physical(&engine.step(Event::Tick, 100_000)).is_empty());
    assert_eq!(engine.snapshot().phase, Phase::Entry);
}

#[test]
fn geometry_invalidation_discards_cancelled_and_queued_legacy_coordinates() {
    let mut p = initial_entry_profile();
    p.entry_target = None;
    let mut engine = Engine::new(p).unwrap();
    let first = engine.step(Event::Start { mode: RunMode::Normal }, 0);
    engine.step(Event::ActionCancelled { id: physical(&first)[0].id }, 1);
    engine.step(Event::Pause, 2);
    engine.invalidate_coordinate_geometry().unwrap();
    assert!(physical(&engine.step(Event::Resume, 3)).is_empty());
    assert!(physical(&engine.step(Event::Tick, 4)).is_empty());
    assert_eq!(engine.snapshot().phase, Phase::Seats);
}

#[test]
fn geometry_invalidation_preserves_selected_seats_and_requires_new_candidates() {
    let mut engine = Engine::new(recovery_profile(2)).unwrap();
    start(&mut engine);
    let first = engine.step(observed(vec![candidate("kept", 1.0, 1.0)]), 1);
    ack(&mut engine, &first, 2);
    selected(&mut engine, "kept", 3);
    let next = engine.step(observed(vec![candidate("old-map", 2.0, 1.0)]), 4);
    engine.step(Event::ActionCancelled { id: physical(&next)[0].id }, 5);
    engine.step(Event::Pause, 6);
    engine.invalidate_coordinate_geometry().unwrap();
    assert_eq!(engine.snapshot().selected_ids, vec!["kept"]);
    assert!(physical(&engine.step(Event::Resume, 7)).is_empty());
    assert!(physical(&engine.step(Event::Tick, 8)).is_empty());
    let fresh = engine.step(observed(vec![candidate("new-map", 30.0, 20.0)]), 9);
    assert!(matches!(&physical(&fresh)[0].kind, ActionKind::Click { purpose, .. } if purpose == "seat:new-map"));
}

#[test]
fn legacy_auto_refresh_hint_does_not_refresh_normal_mode_or_advance_generation() {
    let mut p = recovery_profile(1);
    p.features.automatic_refresh = true;
    p.site_auto_refresh = true;
    p.refresh_interval_ms = 10;
    let mut engine = Engine::new(p).unwrap();
    start(&mut engine);
    engine.step(observed(vec![]), 1);
    assert!(engine.step(Event::Tick, 10).is_empty());
    assert_eq!(engine.snapshot().phase, Phase::Seats);
    assert_eq!(engine.snapshot().generation, 0);
    let poll = engine.step(Event::Tick, 11);
    assert_eq!(poll.len(), 1);
    assert!(matches!(&poll[0].kind, ActionKind::Observe { scope } if scope == "seats"));
    assert_eq!(engine.snapshot().generation, 0);
    engine.step(Event::RefreshCompleted { generation: 1 }, 12);
    assert_eq!(engine.snapshot().generation, 1);
}

#[test]
fn invalid_counts_and_schema_versions_are_rejected() {
    assert!(Profile::from_json(r#"{"target_seats":0}"#).is_err());
    assert!(Profile::from_json(r#"{"target_seats":5}"#).is_err());
    assert!(Profile::from_json(r#"{"schema_version":100}"#).is_err());
    for count in 1..=4 { assert!(Profile::from_json(&format!(r#"{{"target_seats":{count}}}"#)).is_ok()); }
}

#[test]
fn paused_calibration_preserves_partial_selection_and_rejected_candidates() {
    let mut engine = Engine::new(recovery_profile(2)).unwrap();
    start(&mut engine);
    engine.step(observed(vec![candidate("a", 1.0, 1.0)]), 1);
    selected(&mut engine, "a", 2);
    engine.step(observed(vec![candidate("b", 2.0, 1.0)]), 3);
    conflict(&mut engine, "b", 4);
    engine.step(Event::ConflictDialogGone, 5);
    engine.step(Event::SelectionObserved { selected_ids: vec!["a".into()] }, 6);
    let mut vision = VisionConfig::default();
    vision.roi = Some(Rect { x: 0.0, y: 0.0, width: 100.0, height: 100.0 });
    assert!(engine.update_calibration(vision.clone(), Profile::default().color, None, None).is_err());
    engine.step(Event::Pause, 7);
    engine.update_calibration(vision, Profile::default().color, None, Some(target(90.0, 90.0))).unwrap();
    assert_eq!(engine.snapshot().selected_ids, vec!["a"]);
    assert_eq!(engine.snapshot().rejected_ids, vec!["b"]);
    engine.step(Event::Resume, 8);
    let next = engine.step(observed(vec![candidate("b", 2.0, 1.0), candidate("c", 3.0, 1.0)]), 9);
    assert!(matches!(&physical(&next)[0].kind, ActionKind::Click { purpose, .. } if purpose == "seat:c"));
}

#[test]
fn calibration_cannot_change_color_while_click_outcome_is_pending() {
    let mut engine = Engine::new(recovery_profile(1)).unwrap();
    start(&mut engine);
    engine.step(observed(vec![candidate("a", 1.0, 1.0)]), 1);
    engine.step(Event::Pause, 2);
    assert!(engine.update_calibration(VisionConfig::default(), Color::default(), None, None).is_err());
}

#[test]
fn completion_bound_while_paused_submits_once_on_resume() {
    let mut p = recovery_profile(1);
    p.legacy.complete = None;
    let mut engine = Engine::new(p).unwrap();
    start(&mut engine);
    engine.step(observed(vec![candidate("a", 1.0, 1.0)]), 1);
    selected(&mut engine, "a", 2);
    engine.step(Event::Pause, 3);
    engine.update_calibration(VisionConfig::default(), Profile::default().color, None, Some(target(90.0, 90.0))).unwrap();
    let complete = engine.step(Event::Resume, 4);
    assert!(matches!(physical(&complete)[0].kind, ActionKind::CompleteSelection { .. }));
    ack(&mut engine, &complete, 5);
    engine.step(Event::Pause, 6);
    assert!(physical(&engine.step(Event::Resume, 7)).is_empty());
}

#[test]
fn captcha_enter_waits_for_sale_initial_entry_and_registered_sequence_without_replaying_them() {
    let mut p = independent_seats_profile();
    p.sale_deadline_ms = Some(100);
    p.legacy.normal_click_delay_ms = 10;
    let mut engine = Engine::new(p).unwrap();
    engine.step(Event::Start { mode: RunMode::Normal }, 0);
    assert!(!engine.accepts_captcha_enter());
    assert!(engine.step(Event::CaptchaSubmitted, 99).is_empty());
    assert_eq!(engine.snapshot().phase, Phase::WaitingSale);
    let initial = engine.step(Event::Tick, 100);
    assert!(matches!(&physical(&initial)[0].kind, ActionKind::Click { purpose, .. } if purpose == "initial_entry"));
    assert!(!engine.accepts_captcha_enter());
    assert!(engine.step(Event::CaptchaSubmitted, 100).is_empty());
    let delivered = ack(&mut engine, &initial, 101);
    assert!(physical(&delivered).is_empty());
    assert_eq!(engine.snapshot().phase, Phase::Entry);
    let mut now = 101;
    for expected in [target(500.0, 200.0), target(500.0, 300.0), target(500.0, 400.0)] {
        assert!(!engine.accepts_captcha_enter());
        assert!(engine.step(Event::CaptchaSubmitted, now).is_empty());
        assert!(physical(&engine.step(Event::Tick, now + 9)).is_empty());
        now += 10;
        let next = engine.step(Event::Tick, now);
        assert!(matches!(&physical(&next)[0].kind, ActionKind::Click { target, purpose }
            if target == &expected && purpose == "legacy_entry"));
        assert!(physical(&ack(&mut engine, &next, now)).is_empty());
    }
    assert!(!engine.accepts_captcha_enter());
    now += 10;
    assert!(physical(&engine.step(Event::Tick, now)).is_empty());
    assert_eq!(engine.snapshot().phase, Phase::Captcha);
    assert!(engine.accepts_captcha_enter());
    assert!(engine.step(Event::Tick, now + 1).is_empty());
    let submitted = engine.step(Event::CaptchaSubmitted, now + 2);
    assert!(physical(&submitted).is_empty());
    assert_eq!(engine.snapshot().phase, Phase::Region);
    assert_eq!(engine.snapshot().generation, 1);
    assert!(engine.snapshot().optimistic_selection);
    let region = engine.step(Event::RegionsObserved { candidates: vec![RegionCandidate {
        candidate: candidate("zone", 20.0, 20.0), center_rank: 0, distance_to_stage: 10.0,
    }] }, now + 3);
    assert_eq!(engine.snapshot().phase, Phase::WaitingRegion);
    let repeated = engine.step(Event::CaptchaSubmitted, now + 4);
    assert!(physical(&repeated).is_empty());
    assert_eq!(engine.snapshot().phase, Phase::Region);
    assert_eq!(engine.snapshot().generation, 2);
    assert!(ack(&mut engine, &region, now + 5).is_empty());
    assert_eq!(engine.snapshot().phase, Phase::Region);
}

#[test]
fn repeated_captcha_enter_discards_old_attempts_conflicts_and_queued_completion() {
    let mut p = recovery_profile(2);
    p.features.auto_captcha_resume = true;
    p.features.region_priority = true;
    p.legacy.date = Some(target(500.0, 200.0));
    p.legacy.normal_click_delay_ms = 0;
    let mut engine = Engine::new(p).unwrap();
    let entry = engine.step(Event::Start { mode: RunMode::Normal }, 0);
    assert!(matches!(&physical(&entry)[0].kind, ActionKind::Click { target: clicked, purpose }
        if clicked == &target(500.0, 200.0) && purpose == "legacy_entry"));
    assert!(!engine.accepts_captcha_enter());
    let waiting = ack(&mut engine, &entry, 0);
    assert!(physical(&waiting).is_empty());
    assert_eq!(engine.snapshot().phase, Phase::Captcha);
    engine.step(Event::CaptchaSubmitted, 1);
    let region = engine.step(Event::RegionsObserved { candidates: vec![RegionCandidate {
        candidate: candidate("zone", 10.0, 10.0), center_rank: 0, distance_to_stage: 10.0,
    }] }, 2);
    let after_region = ack(&mut engine, &region, 3);
    assert_eq!(engine.snapshot().phase, Phase::Seats);
    assert!(physical(&after_region).is_empty());
    assert!(!engine.snapshot().message.contains("F6"));
    let first = engine.step(Event::CandidatesObserved {
        generation: 1, candidates: vec![candidate("a", 20.0, 10.0), candidate("b", 30.0, 10.0)],
    }, 4);
    let second = ack(&mut engine, &first, 5);
    let completion = ack(&mut engine, &second, 6);
    let attempts = engine.snapshot().delivered_attempts;
    assert_eq!(engine.snapshot().phase, Phase::Completing);
    engine.step(Event::CaptchaSubmitted, 7);
    assert_eq!(engine.snapshot().phase, Phase::Region);
    assert_eq!(engine.snapshot().generation, 2);
    assert!(engine.snapshot().attempted_ids.is_empty());
    assert!(engine.snapshot().selected_ids.is_empty());
    assert!(engine.snapshot().delivered_attempts.is_empty());
    assert!(ack(&mut engine, &completion, 8).is_empty());
    assert!(delivered_result(&mut engine, &attempts[1], SeatOutcome::Conflict, 9).is_empty());
    let region = engine.step(Event::RegionsObserved { candidates: vec![RegionCandidate {
        candidate: candidate("zone", 10.0, 10.0), center_rank: 0, distance_to_stage: 10.0,
    }] }, 10);
    ack(&mut engine, &region, 11);
    let retry = engine.step(Event::CandidatesObserved {
        generation: 2, candidates: vec![candidate("failed", 40.0, 10.0)],
    }, 12);
    ack(&mut engine, &retry, 13);
    let failed = engine.snapshot().delivered_attempts[0].clone();
    let escape = delivered_result(&mut engine, &failed, SeatOutcome::Conflict, 14);
    assert_eq!(engine.snapshot().phase, Phase::Conflict);
    assert_eq!(engine.snapshot().rejected_ids, vec!["failed"]);
    engine.step(Event::CaptchaSubmitted, 15);
    assert_eq!(engine.snapshot().phase, Phase::Region);
    assert_eq!(engine.snapshot().generation, 3);
    assert!(engine.snapshot().rejected_ids.is_empty());
    assert!(engine.snapshot().conflict_attempt.is_none());
    assert!(ack(&mut engine, &escape, 16).is_empty());
}

#[test]
fn captcha_option_preserves_original_entry_clicks_then_gates_region_and_empty_seat_search() {
    for enabled in [false, true] {
        let mut p = recovery_profile(1);
        p.features.auto_captcha_resume = enabled;
        p.features.region_priority = true;
        p.legacy.date = Some(target(100.0, 100.0));
        p.legacy.session = Some(target(200.0, 100.0));
        p.legacy.next = Some(target(300.0, 100.0));
        p.legacy.normal_click_delay_ms = 100;
        let mut engine = Engine::new(p).unwrap();
        let mut next = engine.step(Event::Start { mode: RunMode::Normal }, 0);
        for (index, expected) in [target(100.0, 100.0), target(200.0, 100.0), target(300.0, 100.0)].into_iter().enumerate() {
            let now = index as u64 * 100;
            assert_eq!(physical(&next).len(), 1);
            assert!(matches!(&physical(&next)[0].kind, ActionKind::Click { target, purpose }
                if target == &expected && purpose == "legacy_entry"));
            assert!(!engine.accepts_captcha_enter());
            assert!(engine.step(Event::CaptchaSubmitted, now).is_empty());
            assert!(engine.step(observed(vec![candidate("not-a-seat", 10.0, 10.0)]), now).is_empty());
            assert!(physical(&ack(&mut engine, &next, now)).is_empty());
            assert!(physical(&engine.step(Event::Tick, now + 99)).is_empty());
            next = engine.step(Event::Tick, now + 100);
        }
        assert!(physical(&next).is_empty());
        if !enabled {
            assert_eq!(engine.snapshot().phase, Phase::Region);
            continue;
        }
        assert_eq!(engine.snapshot().phase, Phase::Captcha);
        assert!(engine.accepts_captcha_enter());
        let regions = vec![RegionCandidate {
            candidate: candidate("zone", 10.0, 20.0), center_rank: 0, distance_to_stage: 10.0,
        }];
        assert!(engine.step(Event::RegionsObserved { candidates: regions.clone() }, 301).is_empty());
        assert!(engine.step(observed(vec![candidate("not-a-seat", 10.0, 10.0)]), 302).is_empty());
        assert!(engine.step(Event::Tick, 10_000).is_empty());
        assert_eq!(engine.snapshot().phase, Phase::Captcha);
        engine.step(Event::CaptchaSubmitted, 10_001);
        let region = engine.step(Event::RegionsObserved { candidates: regions }, 10_002);
        ack(&mut engine, &region, 10_003);
        assert_eq!(engine.snapshot().phase, Phase::Seats);
        for now in [10_004, 11_005, 12_006, 18_000] {
            assert!(physical(&engine.step(Event::CandidatesObserved { generation: 1, candidates: vec![] }, now)).is_empty());
            assert!(physical(&engine.step(Event::Tick, now)).is_empty());
            assert_eq!(engine.snapshot().phase, Phase::Seats);
            assert!(engine.snapshot().attempted_ids.is_empty());
        }
    }
}

#[test]
fn cancelled_captcha_entry_click_resumes_remaining_coordinates_without_replaying_delivered_ones() {
    let mut p = recovery_profile(1);
    p.features.auto_captcha_resume = true;
    p.legacy.date = Some(target(100.0, 100.0));
    p.legacy.next = Some(target(300.0, 100.0));
    p.legacy.normal_click_delay_ms = 10;
    let mut engine = Engine::new(p).unwrap();
    let date = engine.step(Event::Start { mode: RunMode::Normal }, 0);
    ack(&mut engine, &date, 1);
    let next = engine.step(Event::Tick, 11);
    let cancelled_id = physical(&next)[0].id;
    engine.step(Event::ActionCancelled { id: cancelled_id }, 12);
    engine.step(Event::Pause, 13);
    assert!(engine.step(Event::CaptchaSubmitted, 14).is_empty());
    engine.step(Event::Resume, 15);
    let retry = engine.step(Event::Tick, 16);
    assert_ne!(physical(&retry)[0].id, cancelled_id);
    assert!(matches!(&physical(&retry)[0].kind, ActionKind::Click { target: clicked, purpose }
        if clicked == &target(300.0, 100.0) && purpose == "legacy_entry"));
    assert!(ack(&mut engine, &next, 17).is_empty());
    ack(&mut engine, &retry, 18);
    assert!(!engine.accepts_captcha_enter());
    assert!(physical(&engine.step(Event::Tick, 28)).is_empty());
    assert_eq!(engine.snapshot().phase, Phase::Captcha);
    assert!(engine.accepts_captcha_enter());
}

#[test]
fn captcha_enter_does_not_restart_paused_stopped_or_other_modes() {
    for mode in [RunMode::Normal, RunMode::Seats, RunMode::Cancellation] {
        for enabled in [false, true] {
            let mut p = recovery_profile(2);
            p.features.auto_captcha_resume = enabled;
            let mut engine = Engine::new(p).unwrap();
            assert!(!engine.accepts_captcha_enter());
            engine.step(Event::Start { mode }, 0);
            assert_eq!(engine.accepts_captcha_enter(), mode == RunMode::Normal && enabled);
            if mode != RunMode::Normal || !enabled {
                assert!(engine.step(Event::CaptchaSubmitted, 1).is_empty());
                assert_eq!(engine.snapshot().generation, 0);
            }
            engine.step(Event::Pause, 2);
            assert!(!engine.accepts_captcha_enter());
            assert!(engine.step(Event::CaptchaSubmitted, 3).is_empty());
            assert_eq!(engine.snapshot().phase, Phase::Paused);
            engine.step(Event::Stop, 4);
            assert!(!engine.accepts_captcha_enter());
            assert!(engine.step(Event::CaptchaSubmitted, 5).is_empty());
            assert_eq!(engine.snapshot().phase, Phase::Stopped);
        }
    }
}

#[test]
fn captcha_enter_does_not_restart_after_handing_off_manual_completion() {
    let mut p = recovery_profile(1);
    p.features.auto_captcha_resume = true;
    p.legacy.complete = None;
    let mut engine = Engine::new(p).unwrap();
    engine.step(Event::Start { mode: RunMode::Normal }, 0);
    engine.step(Event::CaptchaSubmitted, 1);
    let seat = engine.step(Event::CandidatesObserved {
        generation: 1, candidates: vec![candidate("a", 10.0, 10.0)],
    }, 2);
    let handoff = ack(&mut engine, &seat, 3);
    assert!(physical(&handoff).is_empty());
    assert_eq!(engine.snapshot().phase, Phase::Completing);
    assert!(!engine.accepts_captcha_enter());
    assert!(engine.step(Event::CaptchaSubmitted, 4).is_empty());
    assert_eq!(engine.snapshot().generation, 1);
    assert_eq!(engine.snapshot().attempted_ids, vec!["a"]);
}

#[test]
fn completion_delivery_ends_every_mode_without_claiming_selected_seats_or_handling_late_errors() {
    for mode in [RunMode::Normal, RunMode::Seats, RunMode::Cancellation] {
        let mut p = Profile::default();
        p.legacy.complete = Some(target(900.0, 500.0));
        let mut engine = Engine::new(p).unwrap();
        engine.step(Event::Start { mode }, 0);
        let seat = engine.step(observed(vec![candidate("a", 10.0, 10.0)]), 1);
        let complete = ack(&mut engine, &seat, 2);
        assert!(matches!(&physical(&complete)[0].kind, ActionKind::CompleteSelection { .. }));
        let mut cancelled = engine.clone();
        cancelled.step(Event::ActionCancelled { id: physical(&complete)[0].id }, 3);
        assert_eq!(cancelled.snapshot().phase, Phase::Completing);
        let mut failed = engine.clone();
        failed.step(Event::ActionAcknowledged {
            id: physical(&complete)[0].id, success: false, detail: "undelivered".into(),
        }, 3);
        assert_eq!(failed.snapshot().phase, Phase::Faulted);
        ack(&mut engine, &complete, 3);
        assert_eq!(engine.snapshot().phase, Phase::Completed);
        assert_eq!(engine.snapshot().message, "완료 버튼 클릭 전달 · 실행 종료");
        assert!(engine.snapshot().selected_ids.is_empty());
        assert_eq!(engine.snapshot().attempted_ids, vec!["a"]);
        assert!(conflict(&mut engine, "a", 4).is_empty());
        assert!(ack(&mut engine, &complete, 5).is_empty());
        assert!(engine.step(Event::CaptchaSubmitted, 6).is_empty());
        assert!(engine.step(Event::Tick, 10_000).is_empty());
        assert_eq!(engine.snapshot().phase, Phase::Completed);
        assert_eq!(engine.snapshot().attempted_ids, vec!["a"]);
    }
}

#[test]
fn refresh_button_requires_cancellation_mode_explicit_option_and_point_target() {
    for mode in [RunMode::Normal, RunMode::Seats, RunMode::Cancellation] {
        for enabled in [false, true] {
            for button in [None, Some(target(90.0, 90.0)), Some(Target::Element { id: "refresh".into() })] {
                let should_refresh = mode == RunMode::Cancellation && enabled
                    && matches!(&button, Some(Target::Point { .. }));
                let mut p = recovery_profile(2);
                p.features.automatic_refresh = enabled;
                p.refresh_target = button;
                p.refresh_interval_ms = 100;
                let mut engine = Engine::new(p).unwrap();
                engine.step(Event::Start { mode }, 0);
                engine.step(observed(vec![]), 1);
                let actions = engine.step(Event::Tick, 101);
                assert_eq!(physical(&actions).iter().any(|action| matches!(action.kind, ActionKind::Refresh { .. })), should_refresh);
                if !should_refresh {
                    assert!(physical(&actions).is_empty());
                    assert_eq!(engine.snapshot().phase, Phase::Seats);
                }
            }
        }
    }
}

#[test]
fn cancellation_retries_failed_seat_after_interval_preserving_other_attempt_without_refresh() {
    let mut p = continuous_cancellation_profile();
    p.features.automatic_refresh = false;
    let mut engine = Engine::new(p).unwrap();
    engine.step(Event::Start { mode: RunMode::Cancellation }, 0);
    assert!(engine.continuous_search());
    let first = engine.step(observed(vec![candidate("kept", 10.0, 10.0), candidate("failed", 20.0, 10.0)]), 1);
    let second = ack(&mut engine, &first, 2);
    ack(&mut engine, &second, 3);
    let failed = engine.snapshot().delivered_attempts[1].clone();
    let escape = delivered_result(&mut engine, &failed, SeatOutcome::Conflict, 4);
    ack(&mut engine, &escape, 5);
    engine.step(Event::ConflictDialogGone, 6);
    assert!(engine.step(observed(vec![candidate("kept", 10.0, 10.0), candidate("failed", 20.0, 10.0)]), 7).is_empty());
    assert!(physical(&engine.step(Event::Tick, 105)).is_empty());
    assert_eq!(engine.snapshot().rejected_ids, vec!["failed"]);
    let pass = engine.step(Event::Tick, 106);
    assert!(physical(&pass).is_empty());
    assert!(engine.snapshot().rejected_ids.is_empty());
    assert_eq!(engine.snapshot().attempted_ids, vec!["kept"]);
    assert_eq!(engine.snapshot().generation, 0);
    let replacement = engine.step(observed(vec![candidate("kept", 10.0, 10.0), candidate("failed", 20.0, 10.0)]), 107);
    assert!(matches!(&physical(&replacement)[0].kind, ActionKind::Click { purpose, .. } if purpose == "seat:failed"));
    assert!(delivered_result(&mut engine, &failed, SeatOutcome::Conflict, 108).is_empty());
    assert_eq!(engine.snapshot().pending_action_id, Some(physical(&replacement)[0].id));
    let completion = ack(&mut engine, &replacement, 109);
    assert!(matches!(&physical(&completion)[0].kind, ActionKind::CompleteSelection { .. }));
    assert_eq!(engine.snapshot().attempted_ids, vec!["failed", "kept"]);
}

#[test]
fn nol_captcha_feature_defaults_off_for_existing_profiles() {
    let mut old_profile = serde_json::to_value(Profile::default()).unwrap();
    old_profile["features"].as_object_mut().unwrap().remove("nol_captcha_detection");
    let parsed: Profile = serde_json::from_value(old_profile).unwrap();
    assert!(!parsed.features.nol_captcha_detection);
    assert!(!Features::default().nol_captcha_detection);
    assert_eq!(serde_json::to_value(parsed).unwrap()["features"]["nol_captcha_detection"], false);
    assert_eq!(serde_json::to_value(Event::NolCaptchaDismissed).unwrap()["type"], "nol_captcha_dismissed");
}

#[test]
fn nol_captcha_gate_preserves_sale_and_registered_entry_then_starts_continuous_region_search() {
    for enter_enabled in [false, true] {
        let mut p = independent_seats_profile();
        p.features.auto_captcha_resume = enter_enabled;
        p.features.nol_captcha_detection = true;
        p.target_seats = 1;
        p.sale_deadline_ms = Some(100);
        p.legacy.normal_click_delay_ms = 10;
        let mut engine = Engine::new(p).unwrap();
        engine.step(Event::Start { mode: RunMode::Normal }, 0);
        assert!(!engine.awaits_nol_captcha());
        assert!(engine.step(Event::NolCaptchaDismissed, 99).is_empty());
        assert_eq!(engine.snapshot().phase, Phase::WaitingSale);
        let initial = engine.step(Event::Tick, 100);
        assert!(matches!(&physical(&initial)[0].kind, ActionKind::Click { purpose, .. } if purpose == "initial_entry"));
        assert!(engine.step(Event::NolCaptchaDismissed, 100).is_empty());
        ack(&mut engine, &initial, 101);
        let mut now = 101;
        for expected in [target(500.0, 200.0), target(500.0, 300.0), target(500.0, 400.0)] {
            assert!(!engine.awaits_nol_captcha());
            assert!(!engine.accepts_captcha_enter());
            assert!(engine.step(Event::NolCaptchaDismissed, now).is_empty());
            assert!(engine.step(Event::CaptchaSubmitted, now).is_empty());
            // Generic F6/stage evidence must not discard the remaining sequence.
            assert!(engine.step(entry(EntryStage::SeatMap, true, None), now).is_empty());
            assert!(engine.step(entry(EntryStage::Captcha, true, None), now).is_empty());
            assert_eq!(engine.snapshot().phase, Phase::Entry);
            assert!(physical(&engine.step(Event::Tick, now + 9)).is_empty());
            now += 10;
            let next = engine.step(Event::Tick, now);
            assert!(matches!(&physical(&next)[0].kind, ActionKind::Click { target, purpose }
                if target == &expected && purpose == "legacy_entry"));
            assert!(!engine.awaits_nol_captcha());
            assert!(engine.step(Event::NolCaptchaDismissed, now).is_empty());
            assert!(physical(&ack(&mut engine, &next, now)).is_empty());
        }
        assert!(!engine.awaits_nol_captcha());
        now += 10;
        assert!(physical(&engine.step(Event::Tick, now)).is_empty());
        assert!(engine.awaits_nol_captcha());
        assert!(!engine.accepts_captcha_enter());
        for event in [Event::CaptchaSubmitted, Event::CaptchaCompleted, Event::RegionMapReady,
            entry(EntryStage::SeatMap, true, None), Event::Resume] {
            assert!(engine.step(event, now + 1).is_empty());
            assert_eq!(engine.snapshot().phase, Phase::Captcha);
            assert_eq!(engine.snapshot().generation, 0);
        }
        let dismissed = engine.step(Event::NolCaptchaDismissed, now + 2);
        assert!(physical(&dismissed).is_empty());
        assert_eq!(engine.snapshot().phase, Phase::Region);
        assert_eq!(engine.snapshot().generation, 1);
        assert!(engine.continuous_search());
        assert!(!engine.awaits_nol_captcha());
        assert!(!engine.accepts_captcha_enter());
        assert!(engine.step(Event::NolCaptchaDismissed, now + 3).is_empty());
        assert!(engine.step(Event::CaptchaSubmitted, now + 3).is_empty());
        let region = engine.step(Event::RegionsObserved { candidates: vec![RegionCandidate {
            candidate: candidate("zone", 20.0, 20.0), center_rank: 0, distance_to_stage: 10.0,
        }] }, now + 4);
        ack(&mut engine, &region, now + 5);
        assert_eq!(engine.snapshot().phase, Phase::Seats);
        assert!(!engine.snapshot().message.contains("F6"));
        engine.step(Event::CandidatesObserved { generation: 1, candidates: vec![] }, now + 6);
        let poll = engine.step(Event::Tick, now + 106);
        assert!(physical(&poll).is_empty());
        assert_eq!(engine.snapshot().phase, Phase::Seats);
        let seat = engine.step(Event::CandidatesObserved {
            generation: 1, candidates: vec![candidate("seat", 20.0, 20.0)],
        }, now + 107);
        assert!(physical(&ack(&mut engine, &seat, now + 108)).is_empty());
        let completed = engine.step(Event::Tick, now + 118);
        assert!(matches!(&physical(&completed)[0].kind, ActionKind::CompleteSelection { .. }));
    }
}

#[test]
fn nol_captcha_without_registered_entry_waits_then_searches_seats_once() {
    let mut p = recovery_profile(1);
    p.features.nol_captcha_detection = true;
    let mut engine = Engine::new(p).unwrap();
    let started = engine.step(Event::Start { mode: RunMode::Normal }, 0);
    assert!(physical(&started).is_empty());
    assert!(engine.awaits_nol_captcha());
    assert!(engine.step(observed(vec![candidate("behind-captcha", 10.0, 10.0)]), 1).is_empty());
    engine.step(Event::NolCaptchaDismissed, 2);
    assert_eq!(engine.snapshot().phase, Phase::Seats);
    assert_eq!(engine.snapshot().generation, 1);
    assert!(engine.step(Event::NolCaptchaDismissed, 3).is_empty());
    let seat = engine.step(Event::CandidatesObserved {
        generation: 1, candidates: vec![candidate("seat", 10.0, 10.0)],
    }, 4);
    let completion = ack(&mut engine, &seat, 5);
    assert!(matches!(&physical(&completion)[0].kind, ActionKind::CompleteSelection { .. }));
    ack(&mut engine, &completion, 6);
    assert_eq!(engine.snapshot().phase, Phase::Completed);
    assert!(engine.step(Event::NolCaptchaDismissed, 7).is_empty());
}

fn nol_captcha_after_registered_entry(region_priority: bool) -> Engine {
    let mut p = recovery_profile(1);
    p.features.nol_captcha_detection = true;
    p.features.region_priority = region_priority;
    p.legacy.date = Some(target(500.0, 200.0));
    p.legacy.session = Some(target(500.0, 300.0));
    p.legacy.next = Some(target(500.0, 400.0));
    p.legacy.normal_click_delay_ms = 0;
    p.legacy.region_delay_ms = 0;
    let mut engine = Engine::new(p).unwrap();
    let mut actions = engine.step(Event::Start { mode: RunMode::Normal }, 0);
    for (index, expected) in [target(500.0, 200.0), target(500.0, 300.0), target(500.0, 400.0)].into_iter().enumerate() {
        assert_eq!(engine.snapshot().phase, Phase::Entry);
        assert_eq!(physical(&actions).len(), 1);
        assert!(matches!(&physical(&actions)[0].kind, ActionKind::Click { target, purpose }
            if target == &expected && purpose == "legacy_entry"));
        actions = ack(&mut engine, &actions, index as u64 + 1);
    }
    assert!(physical(&actions).is_empty());
    assert!(engine.awaits_nol_captcha());
    engine
}

#[test]
fn nol_captcha_handoff_recheck_cancels_region_without_replaying_entry() {
    let mut engine = nol_captcha_after_registered_entry(true);
    assert!(physical(&engine.step(Event::NolCaptchaDismissed, 10)).is_empty());
    assert_eq!(engine.snapshot().phase, Phase::Region);
    let regions = || Event::RegionsObserved { candidates: vec![RegionCandidate {
        candidate: candidate("zone", 20.0, 20.0), center_rank: 0, distance_to_stage: 10.0,
    }] };
    let queued = engine.step(regions(), 11);
    assert_eq!(physical(&queued).len(), 1);
    let cancelled_id = physical(&queued)[0].id;
    assert!(matches!(&physical(&queued)[0].kind, ActionKind::Click { purpose, .. } if purpose == "region:zone"));
    // A queued region click is already in WaitingRegion before native delivery.
    assert_eq!(engine.snapshot().phase, Phase::WaitingRegion);
    assert!(engine.step(Event::ActionCancelled { id: cancelled_id }, 12).is_empty());
    assert!(physical(&engine.step(entry(EntryStage::Captcha, true, None), 13)).is_empty());
    assert!(engine.awaits_nol_captcha());
    assert_eq!(engine.snapshot().pending_action_id, None);
    assert_eq!(engine.snapshot().current_candidate_id, None);
    for (index, event) in [
        Event::ActionAcknowledged { id: cancelled_id, success: true, detail: String::new() },
        Event::Tick, regions(), Event::RegionMapReady,
    ].into_iter().enumerate() {
        assert!(physical(&engine.step(event, 14 + index as u64)).is_empty());
        assert_eq!(engine.snapshot().phase, Phase::Captcha);
        assert_eq!(engine.snapshot().pending_action_id, None);
        assert_eq!(engine.snapshot().generation, 1);
    }

    assert!(physical(&engine.step(Event::NolCaptchaDismissed, 20)).is_empty());
    assert_eq!(engine.snapshot().phase, Phase::Region);
    assert_eq!(engine.snapshot().generation, 2);
    assert!(physical(&engine.step(Event::Tick, 21)).is_empty());
    let retry = engine.step(regions(), 22);
    assert_eq!(physical(&retry).len(), 1);
    assert!(matches!(&physical(&retry)[0].kind, ActionKind::Click { purpose, .. } if purpose == "region:zone"));
    let retry_id = physical(&retry)[0].id;
    assert_ne!(retry_id, cancelled_id);
    assert!(ack(&mut engine, &queued, 23).is_empty());
    assert_eq!(engine.snapshot().phase, Phase::WaitingRegion);
    assert_eq!(engine.snapshot().pending_action_id, Some(retry_id));
    assert!(engine.snapshot().selected_ids.is_empty());
    assert!(engine.snapshot().attempted_ids.is_empty());
    assert!(engine.snapshot().delivered_attempts.is_empty());
    assert!(physical(&ack(&mut engine, &retry, 24)).is_empty());
    assert_eq!(engine.snapshot().phase, Phase::Seats);
    let seat = engine.step(Event::CandidatesObserved {
        generation: 2, candidates: vec![candidate("seat", 20.0, 20.0)],
    }, 25);
    assert_eq!(physical(&seat).len(), 1);
    assert!(matches!(&physical(&seat)[0].kind, ActionKind::Click { purpose, .. } if purpose == "seat:seat"));
}

#[test]
fn nol_captcha_handoff_recheck_cancels_seat_without_counting_stale_ack() {
    let mut engine = nol_captcha_after_registered_entry(false);
    assert!(physical(&engine.step(Event::NolCaptchaDismissed, 10)).is_empty());
    assert_eq!(engine.snapshot().phase, Phase::Seats);
    let queued = engine.step(Event::CandidatesObserved {
        generation: 1, candidates: vec![candidate("seat", 20.0, 20.0)],
    }, 11);
    assert_eq!(physical(&queued).len(), 1);
    assert!(matches!(&physical(&queued)[0].kind, ActionKind::Click { purpose, .. } if purpose == "seat:seat"));
    let cancelled_id = physical(&queued)[0].id;
    assert!(engine.step(Event::ActionCancelled { id: cancelled_id }, 12).is_empty());
    assert!(physical(&engine.step(entry(EntryStage::Captcha, true, None), 13)).is_empty());
    assert!(engine.awaits_nol_captcha());
    assert_eq!(engine.snapshot().pending_action_id, None);
    assert_eq!(engine.snapshot().current_candidate_id, None);
    for (index, event) in [
        Event::ActionAcknowledged { id: cancelled_id, success: true, detail: String::new() },
        Event::Tick,
        Event::CandidatesObserved { generation: 1, candidates: vec![candidate("seat", 20.0, 20.0)] },
    ].into_iter().enumerate() {
        assert!(physical(&engine.step(event, 14 + index as u64)).is_empty());
        assert_eq!(engine.snapshot().phase, Phase::Captcha);
        assert_eq!(engine.snapshot().pending_action_id, None);
        assert!(engine.snapshot().selected_ids.is_empty());
        assert!(engine.snapshot().attempted_ids.is_empty());
        assert!(engine.snapshot().delivered_attempts.is_empty());
    }

    assert!(physical(&engine.step(Event::NolCaptchaDismissed, 20)).is_empty());
    assert_eq!(engine.snapshot().phase, Phase::Seats);
    assert_eq!(engine.snapshot().generation, 2);
    assert!(physical(&engine.step(Event::Tick, 21)).is_empty());
    assert!(engine.step(Event::CandidatesObserved {
        generation: 1, candidates: vec![candidate("seat", 20.0, 20.0)],
    }, 22).is_empty());
    let retry = engine.step(Event::CandidatesObserved {
        generation: 2, candidates: vec![candidate("seat", 20.0, 20.0)],
    }, 23);
    assert_eq!(physical(&retry).len(), 1);
    assert!(matches!(&physical(&retry)[0].kind, ActionKind::Click { purpose, .. } if purpose == "seat:seat"));
    let retry_id = physical(&retry)[0].id;
    assert_ne!(retry_id, cancelled_id);
    assert!(ack(&mut engine, &queued, 24).is_empty());
    assert_eq!(engine.snapshot().phase, Phase::Seats);
    assert_eq!(engine.snapshot().pending_action_id, Some(retry_id));
    assert!(engine.snapshot().selected_ids.is_empty());
    assert!(engine.snapshot().attempted_ids.is_empty());
    assert!(engine.snapshot().delivered_attempts.is_empty());
    let completion = ack(&mut engine, &retry, 25);
    assert_eq!(physical(&completion).len(), 1);
    assert!(matches!(&physical(&completion)[0].kind, ActionKind::CompleteSelection { .. }));
    assert_eq!(engine.snapshot().attempted_ids, vec!["seat"]);
    assert_eq!(engine.snapshot().delivered_attempts.len(), 1);
    assert_eq!(engine.snapshot().delivered_attempts[0].action_id, retry_id);
}

#[test]
fn nol_captcha_signal_cannot_resume_paused_or_stopped_runs() {
    let mut p = recovery_profile(1);
    p.features.nol_captcha_detection = true;
    let mut engine = Engine::new(p).unwrap();
    start(&mut engine);
    assert!(engine.awaits_nol_captcha());
    engine.step(Event::Pause, 1);
    assert!(!engine.awaits_nol_captcha());
    assert!(engine.step(Event::NolCaptchaDismissed, 2).is_empty());
    assert!(engine.step(Event::CaptchaCompleted, 3).is_empty());
    engine.step(Event::Resume, 4);
    assert!(engine.awaits_nol_captcha());
    engine.step(Event::Stop, 5);
    assert!(!engine.awaits_nol_captcha());
    assert!(engine.step(Event::NolCaptchaDismissed, 6).is_empty());
    assert_eq!(engine.snapshot().generation, 0);
}

#[test]
fn nol_captcha_option_does_not_change_seats_or_cancellation_modes() {
    for mode in [RunMode::Seats, RunMode::Cancellation] {
        for navigation in [CancellationNavigation::None, CancellationNavigation::ArrowKeys] {
            let mut p = recovery_profile(1);
            p.legacy.cancellation_navigation = navigation;
            let mut original = Engine::new(p.clone()).unwrap();
            p.features.nol_captcha_detection = true;
            let mut enabled = Engine::new(p).unwrap();
            let original_actions = original.step(Event::Start { mode }, 0);
            let enabled_actions = enabled.step(Event::Start { mode }, 0);
            assert_eq!(serde_json::to_value(original.snapshot()).unwrap(), serde_json::to_value(enabled.snapshot()).unwrap());
            assert_eq!(serde_json::to_value(original_actions).unwrap(), serde_json::to_value(enabled_actions).unwrap());
            assert!(!enabled.awaits_nol_captcha());
            assert!(enabled.step(Event::NolCaptchaDismissed, 1).is_empty());
        }
    }
}

#[test]
fn nol_captcha_signal_is_ignored_when_option_is_off() {
    for enter_enabled in [false, true] {
        let mut p = recovery_profile(1);
        p.features.auto_captcha_resume = enter_enabled;
        let mut engine = Engine::new(p).unwrap();
        start(&mut engine);
        let before = serde_json::to_value(engine.snapshot()).unwrap();
        assert!(!engine.awaits_nol_captcha());
        assert!(engine.step(Event::NolCaptchaDismissed, 1).is_empty());
        assert_eq!(serde_json::to_value(engine.snapshot()).unwrap(), before);
        assert_eq!(engine.accepts_captcha_enter(), enter_enabled);
    }
}

fn assert_manual_captcha_rejected(engine: &mut Engine, now: u64) {
    let before = serde_json::to_value(engine.snapshot()).unwrap();
    assert!(!engine.can_manually_complete_captcha());
    assert!(engine.step(Event::CaptchaManuallyCompleted, now).is_empty());
    assert_eq!(serde_json::to_value(engine.snapshot()).unwrap(), before);
}

#[test]
fn manual_captcha_completion_preserves_entry_and_searches_without_automatic_detection() {
    assert_eq!(serde_json::to_value(Event::CaptchaManuallyCompleted).unwrap()["type"], "captcha_manually_completed");
    for nol in [false, true] {
        for enter in [false, true] {
            for region in [false, true] {
                let mut p = recovery_profile(1);
                p.features.nol_captcha_detection = nol;
                p.features.auto_captcha_resume = enter;
                p.features.region_priority = region;
                p.legacy.date = Some(target(500.0, 200.0));
                p.legacy.session = Some(target(500.0, 300.0));
                p.legacy.next = Some(target(500.0, 400.0));
                p.legacy.normal_click_delay_ms = 0;
                p.legacy.region_delay_ms = 0;
                p.refresh_interval_ms = 100;
                let profile_before = serde_json::to_value(&p).unwrap();
                let mut engine = Engine::new(p).unwrap();
                assert_manual_captcha_rejected(&mut engine, 0);
                let mut actions = engine.step(Event::Start { mode: RunMode::Normal }, 1);
                for (index, expected) in [target(500.0, 200.0), target(500.0, 300.0), target(500.0, 400.0)].into_iter().enumerate() {
                    assert_manual_captcha_rejected(&mut engine, index as u64 + 2);
                    assert_eq!(physical(&actions).len(), 1);
                    assert!(matches!(&physical(&actions)[0].kind, ActionKind::Click { target, purpose }
                        if target == &expected && purpose == "legacy_entry"));
                    actions = ack(&mut engine, &actions, index as u64 + 2);
                }
                assert!(physical(&actions).is_empty());
                if !nol && !enter {
                    // The existing non-automatic route can also observe a CAPTCHA.
                    engine.step(entry(EntryStage::Captcha, true, None), 5);
                }
                assert_eq!(engine.snapshot().phase, Phase::Captcha);
                assert!(engine.can_manually_complete_captcha());
                // No visible-panel, disappearance or Enter evidence has been supplied.
                let manual = engine.step(Event::CaptchaManuallyCompleted, 10);
                assert!(physical(&manual).is_empty());
                assert_eq!(engine.snapshot().phase, if region { Phase::Region } else { Phase::Seats });
                assert_eq!(engine.snapshot().generation, 1);
                assert!(engine.continuous_search());
                assert!(engine.can_manually_complete_captcha());
                if region {
                    let selected_region = engine.step(Event::RegionsObserved { candidates: vec![RegionCandidate {
                        candidate: candidate("zone", 20.0, 20.0), center_rank: 0, distance_to_stage: 10.0,
                    }] }, 12);
                    assert_eq!(physical(&selected_region).len(), 1);
                    assert!(matches!(&physical(&selected_region)[0].kind, ActionKind::Click { purpose, .. }
                        if purpose == "region:zone"));
                    assert!(engine.can_manually_complete_captcha());
                    assert!(physical(&ack(&mut engine, &selected_region, 14)).is_empty());
                }
                assert_eq!(engine.snapshot().phase, Phase::Seats);
                assert!(!engine.snapshot().message.contains("F6"));
                engine.step(Event::CandidatesObserved { generation: 1, candidates: vec![] }, 15);
                let poll = engine.step(Event::Tick, 115);
                assert_eq!(poll.len(), 1);
                assert!(matches!(&poll[0].kind, ActionKind::Observe { scope } if scope == "seats"));
                let seat = engine.step(Event::CandidatesObserved {
                    generation: 1, candidates: vec![candidate("seat", 20.0, 20.0)],
                }, 116);
                assert_eq!(physical(&seat).len(), 1);
                assert!(matches!(&physical(&seat)[0].kind, ActionKind::Click { purpose, .. } if purpose == "seat:seat"));
                assert_eq!(serde_json::to_value(engine.profile()).unwrap(), profile_before);
            }
        }
    }
}

#[test]
fn manual_captcha_completion_cannot_skip_sale_initial_entry_or_registered_inputs() {
    let mut p = independent_seats_profile();
    p.features.nol_captcha_detection = true;
    p.sale_deadline_ms = Some(100);
    p.legacy.normal_click_delay_ms = 0;
    let mut engine = Engine::new(p).unwrap();
    assert!(physical(&engine.step(Event::Start { mode: RunMode::Normal }, 0)).is_empty());
    assert_manual_captcha_rejected(&mut engine, 99);
    let initial = engine.step(Event::Tick, 100);
    assert!(matches!(&physical(&initial)[0].kind, ActionKind::Click { purpose, .. } if purpose == "initial_entry"));
    assert_manual_captcha_rejected(&mut engine, 101);
    let mut actions = ack(&mut engine, &initial, 102);
    for index in 0..3 {
        assert_eq!(physical(&actions).len(), 1);
        assert!(matches!(&physical(&actions)[0].kind, ActionKind::Click { purpose, .. } if purpose == "legacy_entry"));
        assert_manual_captcha_rejected(&mut engine, 103 + index * 2);
        actions = ack(&mut engine, &actions, 104 + index * 2);
    }
    assert!(physical(&actions).is_empty());
    assert!(engine.can_manually_complete_captcha());
    assert!(physical(&engine.step(Event::CaptchaManuallyCompleted, 109)).is_empty());
    assert_eq!(engine.snapshot().phase, Phase::Region);
}

#[test]
fn manual_captcha_completion_requires_last_entry_delivery_again_after_new_start() {
    let mut p = recovery_profile(1);
    p.legacy.date = Some(target(500.0, 200.0));
    p.legacy.session = Some(target(500.0, 300.0));
    p.legacy.next = Some(target(500.0, 400.0));
    p.legacy.normal_click_delay_ms = 0;
    let mut engine = Engine::new(p).unwrap();
    let mut actions = engine.step(Event::Start { mode: RunMode::Normal }, 0);
    for now in 1..=3 { actions = ack(&mut engine, &actions, now); }
    engine.step(entry(EntryStage::Captcha, true, None), 4);
    assert!(engine.can_manually_complete_captcha());
    engine.step(Event::CaptchaManuallyCompleted, 5);
    assert!(engine.continuous_search());
    engine.step(Event::Stop, 6);
    assert_manual_captcha_rejected(&mut engine, 7);
    let first = engine.step(Event::Start { mode: RunMode::Normal }, 8);
    assert!(!engine.continuous_search());
    let second = ack(&mut engine, &first, 9);
    let last = ack(&mut engine, &second, 10);
    assert!(matches!(&physical(&last)[0].kind, ActionKind::Click { target: clicked, purpose }
        if clicked == &target(500.0, 400.0) && purpose == "legacy_entry"));
    // Generic stage evidence can clear this pending ID, but cannot prove delivery.
    engine.step(entry(EntryStage::Captcha, true, None), 11);
    assert_eq!(engine.snapshot().phase, Phase::Captcha);
    assert_eq!(engine.snapshot().pending_action_id, None);
    assert_manual_captcha_rejected(&mut engine, 12);
    assert!(ack(&mut engine, &last, 13).is_empty());
    assert_manual_captcha_rejected(&mut engine, 14);
}

#[test]
fn manual_captcha_completion_hands_search_modes_to_normal_and_explicitly_resumes_normal() {
    for mode in [RunMode::Seats, RunMode::Cancellation] {
        let mut p = recovery_profile(1);
        p.features.nol_captcha_detection = true;
        let mut engine = Engine::new(p).unwrap();
        engine.step(Event::Start { mode }, 0);
        assert!(engine.can_manually_complete_captcha());
        assert!(physical(&engine.step(Event::CaptchaManuallyCompleted, 1)).is_empty());
        assert_eq!(engine.snapshot().mode, RunMode::Normal);
        assert_eq!(engine.snapshot().phase, Phase::Seats);
    }
    let mut p = recovery_profile(1);
    p.features.nol_captcha_detection = true;
    let mut engine = Engine::new(p).unwrap();
    start(&mut engine);
    engine.step(Event::Pause, 1);
    assert!(engine.can_manually_complete_captcha());
    let mut resumed = engine.clone();
    resumed.step(Event::Resume, 3);
    assert!(resumed.can_manually_complete_captcha());
    assert_eq!(engine.snapshot().phase, Phase::Paused);
    engine.step(Event::Resume, 3);
    assert!(engine.can_manually_complete_captcha());
    engine.step(Event::CaptchaManuallyCompleted, 4);
    assert_eq!(engine.snapshot().phase, Phase::Seats);
}

#[test]
fn manual_captcha_completion_cancels_unsent_work_without_resetting_delivered_seats() {
    let mut engine = Engine::new(recovery_profile(2)).unwrap();
    start(&mut engine);
    engine.step(entry(EntryStage::Captcha, true, None), 1);
    engine.step(Event::CaptchaManuallyCompleted, 2);
    let first = engine.step(Event::CandidatesObserved {
        generation: 1, candidates: vec![candidate("kept", 20.0, 20.0), candidate("next", 30.0, 20.0)],
    }, 3);
    let queued_second = ack(&mut engine, &first, 4);
    let restarted = engine.step(Event::CaptchaManuallyCompleted, 5);
    assert!(physical(&restarted).is_empty());
    assert_eq!(engine.snapshot().attempted_ids, vec!["kept"]);
    assert!(ack(&mut engine, &queued_second, 6).is_empty());
    let second = engine.step(Event::CandidatesObserved {
        generation: engine.snapshot().generation,
        candidates: vec![candidate("kept", 20.0, 20.0), candidate("next", 30.0, 20.0)],
    }, 7);
    let old_completion = ack(&mut engine, &second, 8);
    assert_eq!(engine.snapshot().attempted_ids, vec!["kept", "next"]);
    engine.step(Event::CaptchaManuallyCompleted, 9);
    assert!(ack(&mut engine, &old_completion, 10).is_empty());
    assert!(physical(&engine.step(Event::Tick, 11)).is_empty());
    let completion = engine.step(Event::CandidatesObserved {
        generation: engine.snapshot().generation, candidates: vec![],
    }, 11);
    ack(&mut engine, &completion, 12);
    assert_eq!(engine.snapshot().phase, Phase::Completed);
    assert!(physical(&engine.step(Event::CaptchaManuallyCompleted, 13)).is_empty());
    assert_eq!(engine.snapshot().phase, Phase::Seats);
    assert!(physical(&engine.step(Event::Tick, 14)).is_empty());
    let explicit_retry = engine.step(Event::CandidatesObserved {
        generation: engine.snapshot().generation, candidates: vec![],
    }, 15);
    assert_eq!(physical(&explicit_retry).len(), 1);
    assert!(matches!(physical(&explicit_retry)[0].kind, ActionKind::CompleteSelection { .. }));
}
