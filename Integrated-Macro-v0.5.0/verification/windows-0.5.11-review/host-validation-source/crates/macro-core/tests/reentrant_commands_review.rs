//! Independent engine boundaries for explicit reruns. Native delivery, browser
//! selections, and runtime window compatibility are not inferred by these tests.
use macro_core::*;

fn profile(count: bool, wanted: usize) -> Profile {
    let mut p = Profile::default();
    p.target_seats = wanted;
    p.features.auto_captcha_resume = true;
    p.features.conflict_recovery = true;
    p.features.nol_selection_count = count;
    p.legacy.complete = Some(Target::Point { point: Point { x: 900.0, y: 900.0 } });
    p.refresh_interval_ms = 1000;
    p
}

fn candidate(id: &str, x: f64, inset: f64) -> Candidate {
    let point = Point { x: x + 5.0, y: 25.0 };
    Candidate {
        id: id.into(), point, target: Target::Point { point }, selected: false,
        color: Profile::default().color,
        geometry: Some(CandidateGeometry {
            bounds: Rect { x: x + inset, y: 20.0 + inset, width: 10.0 - 2.0 * inset, height: 10.0 - 2.0 * inset },
            pixels: (10.0 - 2.0 * inset).powi(2) as usize,
            object_shape: None, object_context: None,
        }),
    }
}

fn no_input(actions: &[Action]) {
    assert!(actions.iter().all(|a| matches!(a.kind, ActionKind::Observe { .. })), "{actions:?}");
}

fn input(actions: Vec<Action>) -> Action {
    let mut inputs = actions.into_iter().filter(|a| !matches!(a.kind, ActionKind::Observe { .. }));
    let action = inputs.next().expect("one physical input expected");
    assert!(inputs.next().is_none());
    action
}

fn seat(actions: Vec<Action>, id: &str) -> Action {
    let action = input(actions);
    assert!(matches!(&action.kind, ActionKind::Click { purpose, .. } if purpose == &format!("seat:{id}")), "{action:?}");
    action
}

fn ack(e: &mut Engine, action: &Action, now: u64) -> Vec<Action> {
    e.step(Event::ActionAcknowledged { id: action.id, success: true, detail: String::new() }, now)
}

fn observe(e: &mut Engine, candidates: Vec<Candidate>, now: u64) -> Vec<Action> {
    e.step(Event::CandidatesObserved { generation: e.snapshot().generation, candidates }, now)
}

fn read(e: &mut Engine, count: usize, now: u64) -> Vec<Action> {
    let request = e.selection_count_request().expect("count request");
    e.step(Event::SelectionCountObserved { request_id: request.id, observation_id: now, count }, now)
}

fn normal(p: Profile) -> Engine {
    let mut e = Engine::new(p).unwrap();
    no_input(&e.step(Event::Start { mode: RunMode::Normal }, 0));
    assert_eq!(e.snapshot().phase, Phase::Captcha);
    no_input(&e.step(Event::CaptchaManuallyCompleted, 1));
    assert_eq!(e.snapshot().phase, Phase::Seats);
    e
}

#[test]
fn f9_retry_keeps_delivered_footprint_and_reobserves_without_replaying_entry() {
    let mut e = normal(profile(false, 2));
    let first = seat(observe(&mut e, vec![candidate("a", 10.0, 0.0)], 2), "a");
    no_input(&ack(&mut e, &first, 3));
    let generation = e.snapshot().generation;
    no_input(&e.step(Event::CaptchaManuallyCompleted, 4));
    assert!(e.snapshot().generation > generation, "an explicit F9 retry must discard the old observation generation");
    assert_eq!(e.snapshot().attempted_ids, vec!["a"]);
    assert_eq!(e.snapshot().phase, Phase::Seats);
    let next = seat(observe(&mut e, vec![candidate("a-repainted", 10.0, 1.0), candidate("b", 25.0, 0.0)], 5), "b");
    assert!(next.id > first.id);
}

#[test]
fn f9_retry_rotates_count_request_and_keeps_an_unresolved_delivered_attempt() {
    let mut e = normal(profile(true, 2));
    no_input(&observe(&mut e, vec![candidate("a", 10.0, 0.0)], 2));
    let first = seat(read(&mut e, 0, 3), "a");
    no_input(&ack(&mut e, &first, 4));
    let old = e.selection_count_request().unwrap();
    no_input(&e.step(Event::CaptchaManuallyCompleted, 5));
    let current = e.selection_count_request().unwrap();
    assert_ne!(current.id, old.id, "retry needs a fresh observation request");
    assert_eq!(current.after_action_id, Some(first.id));
    no_input(&e.step(Event::SelectionCountObserved { request_id: old.id, observation_id: 100, count: 2 }, 6));
    no_input(&observe(&mut e, vec![candidate("a-repainted", 10.0, 1.0), candidate("b", 25.0, 0.0)], 7));
    no_input(&read(&mut e, 0, 8)); // Unchanged count does not resolve delivery.
    let next = seat(read(&mut e, 1, 9), "b");
    assert!(next.id > first.id);
    assert_eq!(e.snapshot().site_selection_count, Some(1));
}

#[test]
fn explicit_completed_retry_requires_new_observation_and_delivers_completion_only_once() {
    for count in [false, true] {
        for retry_mode in [None, Some(RunMode::Seats), Some(RunMode::Cancellation)] {
            let p = profile(count, 1);
            let mut e = normal(p.clone());
            let observed = observe(&mut e, vec![candidate("a", 10.0, 0.0)], 2);
            let first = if count { no_input(&observed); seat(read(&mut e, 0, 3), "a") }
                else { seat(observed, "a") };
            let delivered = ack(&mut e, &first, 4);
            let complete = if count { no_input(&delivered); input(read(&mut e, 1, 5)) }
                else { input(delivered) };
            assert!(matches!(complete.kind, ActionKind::CompleteSelection { .. }));
            no_input(&ack(&mut e, &complete, 6));
            assert_eq!(e.snapshot().phase, Phase::Completed);
            let generation = e.snapshot().generation;
            let retry = if let Some(mode) = retry_mode { e.restart_search(p, mode, true, 7).unwrap() }
                else { e.step(Event::CaptchaManuallyCompleted, 7) };
            no_input(&retry);
            assert!(e.snapshot().generation > generation);
            no_input(&ack(&mut e, &complete, 8));
            no_input(&e.step(Event::Tick, 9));
            no_input(&e.step(Event::CandidatesObserved { generation, candidates: vec![] }, 10));
            no_input(&e.step(Event::Tick, 11));
            // Only a fresh current-map observation (OFF), or a new request's
            // externally observed total (ON), can reauthorize this user retry.
            let current = observe(&mut e, vec![candidate("a-repainted", 10.0, 1.0)], 12);
            let repeated = if count { no_input(&current); input(read(&mut e, 1, 13)) }
                else { input(current) };
            assert!(matches!(repeated.kind, ActionKind::CompleteSelection { .. }));
            assert!(repeated.id > complete.id);
            assert_eq!(e.snapshot().attempted_ids, vec!["a"]);
            no_input(&ack(&mut e, &complete, 14)); // Old completion cannot ACK the retry.
            assert_eq!(e.snapshot().pending_action_id, Some(repeated.id));
            no_input(&ack(&mut e, &repeated, 15));
            assert_eq!(e.snapshot().phase, Phase::Completed);
            for now in [16, 100, 10_000] { no_input(&e.step(Event::Tick, now)); }
        }
    }
}

#[test]
fn compatible_mode_restart_keeps_retained_and_failed_marks_but_not_cancelled_input() {
    let p = profile(false, 3);
    let mut e = Engine::new(p.clone()).unwrap();
    no_input(&e.step(Event::Start { mode: RunMode::Cancellation }, 0));
    let a = seat(observe(&mut e, vec![candidate("a", 10.0, 0.0), candidate("b", 25.0, 0.0)], 1), "a");
    let b = seat(ack(&mut e, &a, 2), "b");
    no_input(&ack(&mut e, &b, 3));
    let escape = input(e.step(Event::DeliveredSeatResult { action_id: b.id, candidate_id: "b".into(), result: SeatOutcome::Conflict }, 4));
    assert!(matches!(&escape.kind, ActionKind::Key { key: Key::Escape, .. }));
    no_input(&ack(&mut e, &escape, 5));
    no_input(&e.step(Event::ConflictDialogGone, 6));
    let old_c = seat(observe(&mut e, vec![candidate("c", 40.0, 0.0)], 7), "c");
    no_input(&e.step(Event::ActionCancelled { id: old_c.id }, 8));
    let old_generation = e.snapshot().generation;
    no_input(&e.restart_search(p, RunMode::Seats, true, 9).unwrap());
    assert!(e.snapshot().generation > old_generation);
    assert_eq!(e.snapshot().attempted_ids, vec!["a"]);
    let c = seat(observe(&mut e, vec![candidate("a-repainted", 10.0, 1.0), candidate("b-repainted", 25.0, 1.0), candidate("c", 40.0, 0.0)], 10), "c");
    assert!(c.id > old_c.id);
    no_input(&ack(&mut e, &old_c, 11));
    assert_eq!(e.snapshot().attempted_ids, vec!["a"]);
    no_input(&ack(&mut e, &c, 12));
    no_input(&e.step(Event::RefreshCompleted { generation: e.snapshot().generation + 1 }, 13));
    let retried = seat(observe(&mut e, vec![candidate("a-repainted", 10.0, 1.0), candidate("b-repainted", 25.0, 1.0), candidate("c-repainted", 40.0, 1.0)], 14), "b-repainted");
    assert!(retried.id > c.id);
    assert_eq!(e.snapshot().attempted_ids, vec!["a", "c"]);
}

#[test]
fn changed_context_reset_does_not_apply_old_footprints_or_old_ack_to_the_new_map() {
    let mut e = normal(profile(false, 2));
    let a = seat(observe(&mut e, vec![candidate("old-map-a", 10.0, 0.0)], 2), "old-map-a");
    no_input(&ack(&mut e, &a, 3));
    let mut changed = profile(false, 2);
    changed.vision.roi = Some(Rect { x: 0.0, y: 0.0, width: 80.0, height: 60.0 });
    // The runtime owns the real window/geometry comparison. This tests the
    // explicit non-preserving branch, not whether that runtime decision is right.
    no_input(&e.restart_search(changed, RunMode::Seats, false, 4).unwrap());
    assert!(e.snapshot().attempted_ids.is_empty());
    let b = seat(observe(&mut e, vec![candidate("new-map-seat", 10.0, 0.0)], 5), "new-map-seat");
    assert!(b.id > a.id);
    no_input(&ack(&mut e, &a, 6));
    assert!(e.snapshot().attempted_ids.is_empty());
    no_input(&ack(&mut e, &b, 7));
    assert_eq!(e.snapshot().attempted_ids, vec!["new-map-seat"]);
}

#[test]
fn compatible_restart_preserves_count_awaiting_but_never_its_previous_authorization() {
    let p = profile(true, 2);
    let mut e = normal(p.clone());
    no_input(&observe(&mut e, vec![candidate("a", 10.0, 0.0)], 2));
    let a = seat(read(&mut e, 0, 3), "a");
    no_input(&ack(&mut e, &a, 4));
    let old_request = e.selection_count_request().unwrap();
    no_input(&e.restart_search(p, RunMode::Cancellation, true, 5).unwrap());
    let request = e.selection_count_request().unwrap();
    assert_ne!(request.id, old_request.id);
    assert_eq!(request.after_action_id, Some(a.id));
    no_input(&e.step(Event::SelectionCountObserved { request_id: old_request.id, observation_id: 99, count: 2 }, 6));
    no_input(&observe(&mut e, vec![candidate("a-repainted", 10.0, 1.0), candidate("b", 25.0, 0.0)], 7));
    no_input(&read(&mut e, 0, 8));
    seat(read(&mut e, 1, 9), "b");
}

#[test]
fn retry_during_conflict_preserves_dialog_and_rearms_only_an_undelivered_escape() {
    for switch_mode in [false, true] {
        for escape_delivered in [false, true] {
            let p = profile(false, 3);
            let mut e = normal(p.clone());
            let a = seat(observe(&mut e, vec![candidate("a", 10.0, 0.0), candidate("b", 25.0, 0.0)], 2), "a");
            let b = seat(ack(&mut e, &a, 3), "b");
            no_input(&ack(&mut e, &b, 4));
            let failed = || Event::DeliveredSeatResult { action_id: b.id, candidate_id: "b".into(), result: SeatOutcome::Conflict };
            let escape = input(e.step(failed(), 5));
            if escape_delivered { no_input(&ack(&mut e, &escape, 6)); }
            let retry = if switch_mode { e.restart_search(p, RunMode::Cancellation, true, 7).unwrap() }
                else { e.step(Event::CaptchaManuallyCompleted, 7) };
            no_input(&retry);
            assert_eq!(e.snapshot().phase, Phase::Conflict);
            assert_eq!(e.snapshot().attempted_ids, vec!["a"]);
            no_input(&observe(&mut e, vec![candidate("c", 40.0, 0.0)], 8));
            let repeated = e.step(failed(), 9);
            if escape_delivered { no_input(&repeated); }
            else {
                let replacement = input(repeated);
                assert!(matches!(replacement.kind, ActionKind::Key { key: Key::Escape, .. }));
                assert!(replacement.id > escape.id);
                no_input(&ack(&mut e, &escape, 10));
                no_input(&ack(&mut e, &replacement, 11));
            }
            no_input(&e.step(Event::ConflictDialogGone, 12));
            seat(observe(&mut e, vec![candidate("a-alias", 10.0, 1.0), candidate("b-alias", 25.0, 1.0), candidate("c", 40.0, 0.0)], 13), "c");
        }
    }
}

#[test]
fn manual_entry_skips_saved_hidden_steps_but_keeps_the_scheduled_first_booking_click() {
    for (nol, enter, automatic) in [(false, false, false), (false, true, false), (true, true, true)] {
        let mut p = profile(false, 2);
        p.features.manual_entry = true;
        p.features.nol_captcha_detection = nol;
        p.features.auto_captcha_resume = enter;
        p.features.auto_entry = automatic;
        p.features.server_clock = true;
        p.sale_deadline_ms = Some(100);
        let point = |x| Target::Point { point: Point { x, y: 10.0 } };
        p.entry_target = Some(point(100.0));
        p.legacy.date = Some(point(200.0)); p.legacy.session = Some(point(300.0)); p.legacy.next = Some(point(400.0));
        let mut e = Engine::new(p).unwrap();
        no_input(&e.step(Event::Start { mode: RunMode::Normal }, 0));
        // Without an explicit F9 override, the configured schedule is preserved.
        assert_eq!(e.snapshot().phase, Phase::WaitingSale);
        no_input(&e.step(Event::Tick, 99));
        let first = input(e.step(Event::Tick, 100));
        assert!(matches!(&first.kind, ActionKind::Click { target, purpose } if target == &point(100.0) && purpose == "initial_entry"));
        no_input(&ack(&mut e, &first, 102));
        assert_eq!(e.snapshot().phase, Phase::Captcha);
        no_input(&e.step(Event::Tick, 200));
        no_input(&e.step(Event::CaptchaManuallyCompleted, 201));
        assert_eq!(e.snapshot().phase, Phase::Seats);
        no_input(&e.step(Event::Tick, 202));
        assert!(e.snapshot().delivered_attempts.is_empty());
    }
}

#[test]
fn manual_entry_off_keeps_the_existing_date_session_next_order() {
    let mut p = profile(false, 2);
    p.features.manual_entry = false;
    let point = |x| Target::Point { point: Point { x, y: 10.0 } };
    p.entry_target = Some(point(100.0));
    p.legacy.date = Some(point(200.0)); p.legacy.session = Some(point(300.0)); p.legacy.next = Some(point(400.0));
    let mut e = Engine::new(p).unwrap();
    let mut action = input(e.step(Event::Start { mode: RunMode::Normal }, 0));
    for (i, x) in [100.0, 200.0, 300.0, 400.0].into_iter().enumerate() {
        assert!(matches!(&action.kind, ActionKind::Click { target, .. } if target == &point(x)));
        let next = ack(&mut e, &action, (i + 1) as u64);
        if i == 3 { no_input(&next); } else { action = input(next); }
    }
    assert_eq!(e.snapshot().phase, Phase::Captcha);
}

#[test]
fn a_lower_restart_target_cannot_add_more_seats_than_the_retained_total() {
    let p = profile(false, 3);
    let mut e = normal(p.clone());
    let a = seat(observe(&mut e, vec![candidate("a", 10.0, 0.0), candidate("b", 25.0, 0.0)], 2), "a");
    let b = seat(ack(&mut e, &a, 3), "b");
    no_input(&ack(&mut e, &b, 4));
    assert_eq!(e.snapshot().attempted_ids, vec!["a", "b"]);
    let mut changed = p.clone();
    changed.target_seats = 1;
    no_input(&e.restart_search(changed, RunMode::Seats, true, 5).unwrap());
    no_input(&observe(&mut e, vec![candidate("a-repainted", 10.0, 1.0), candidate("b-repainted", 25.0, 1.0), candidate("c", 40.0, 0.0)], 6));
    no_input(&e.step(Event::Tick, 7));
    assert_eq!(e.snapshot().attempted_ids, vec!["a", "b"]);
    // Raising the target again must resume useful work while keeping A/B.
    no_input(&e.restart_search(p, RunMode::Seats, true, 8).unwrap());
    seat(observe(&mut e, vec![candidate("a-repainted", 10.0, 1.0), candidate("b-repainted", 25.0, 1.0), candidate("c", 40.0, 0.0)], 9), "c");
}

#[test]
fn f9_from_both_search_modes_keeps_selection_and_changes_only_post_entry_work() {
    for mode in [RunMode::Seats, RunMode::Cancellation] {
        for count in [false, true] {
            let mut p = profile(count, 2);
            p.features.region_priority = true;
            let point = |x| Target::Point { point: Point { x, y: 10.0 } };
            p.entry_target = Some(point(100.0));
            p.legacy.date = Some(point(200.0)); p.legacy.session = Some(point(300.0)); p.legacy.next = Some(point(400.0));
            let mut e = Engine::new(p.clone()).unwrap();
            no_input(&e.step(Event::Start { mode }, 0));
            let observed = observe(&mut e, vec![candidate("a", 10.0, 0.0)], 1);
            let a = if count { no_input(&observed); seat(read(&mut e, 0, 2), "a") } else { seat(observed, "a") };
            no_input(&ack(&mut e, &a, 3));
            let old_request = e.selection_count_request();
            no_input(&e.step(Event::Pause, 4));
            no_input(&e.restart_after_entry(p, 5).unwrap());
            assert_eq!(e.snapshot().mode, RunMode::Normal);
            assert_eq!(e.snapshot().phase, Phase::Seats, "retained seats must not reenter a region");
            assert_eq!(e.snapshot().attempted_ids, vec!["a"]);
            if let Some(old) = old_request {
                no_input(&e.step(Event::SelectionCountObserved { request_id: old.id, observation_id: 100, count: 2 }, 6));
            }
            no_input(&e.step(Event::Tick, 7));
            let observed = observe(&mut e, vec![candidate("a-repainted", 10.0, 1.0), candidate("b", 25.0, 0.0)], 8);
            if count { no_input(&observed); seat(read(&mut e, 1, 9), "b"); }
            else { seat(observed, "b"); }
        }
    }
}

#[test]
fn f9_from_an_empty_search_uses_normal_region_priority_without_replaying_entry() {
    for mode in [RunMode::Seats, RunMode::Cancellation] {
        let mut p = profile(false, 2);
        p.features.region_priority = true;
        p.entry_target = Some(Target::Point { point: Point { x: 100.0, y: 100.0 } });
        p.legacy.date = Some(Target::Point { point: Point { x: 200.0, y: 100.0 } });
        let mut e = Engine::new(p.clone()).unwrap();
        no_input(&e.step(Event::Start { mode }, 0));
        no_input(&e.restart_after_entry(p, 1).unwrap());
        assert_eq!(e.snapshot().mode, RunMode::Normal);
        assert_eq!(e.snapshot().phase, Phase::Region);
        no_input(&e.step(Event::Tick, 2));
    }
}

#[test]
fn f9_retains_a_legacy_regions_roi_color_completion_and_alias_exclusion() {
    let mut p = profile(false, 2);
    p.features.conflict_recovery = false;
    p.vision.roi = Some(Rect { x: 200.0, y: 200.0, width: 100.0, height: 100.0 });
    let zone_vision = VisionConfig { roi: Some(Rect { x: 0.0, y: 0.0, width: 100.0, height: 100.0 }), ..VisionConfig::default() };
    let zone_color = Color { r: 20, g: 80, b: 140 };
    let zone_complete = Target::Point { point: Point { x: 800.0, y: 800.0 } };
    p.legacy.regions = vec![LegacyRegionSettings { vision: Some(zone_vision.clone()), color: Some(zone_color), complete: Some(zone_complete.clone()) }];
    p.legacy.cancellation_navigation = CancellationNavigation::TwoRegions {
        first: Target::Point { point: Point { x: 100.0, y: 10.0 } },
        second: Target::Point { point: Point { x: 200.0, y: 10.0 } },
    };
    let mut e = Engine::new(p.clone()).unwrap();
    let navigation = input(e.step(Event::Start { mode: RunMode::Cancellation }, 0));
    no_input(&ack(&mut e, &navigation, 1));
    no_input(&e.step(Event::Tick, 1));
    assert_eq!(e.snapshot().active_region, Some(0));
    let in_zone = |id, x, inset| { let mut c = candidate(id, x, inset); c.color = zone_color; c };
    let a = seat(observe(&mut e, vec![in_zone("zone0:a", 10.0, 0.0)], 2), "zone0:a");
    no_input(&ack(&mut e, &a, 3));
    p.features.region_priority = true;
    no_input(&e.restart_after_entry(p.clone(), 4).unwrap());
    assert_eq!(e.snapshot().mode, RunMode::Normal);
    assert_eq!(e.snapshot().phase, Phase::Seats);
    assert_eq!(e.snapshot().active_region, Some(0));
    assert_eq!(e.effective_color(), zone_color);
    assert_eq!(e.effective_vision().roi, zone_vision.roi);
    assert_eq!(e.effective_completion_target(), Some(&zone_complete));
    let b = seat(observe(&mut e, vec![in_zone("new-a-alias", 10.0, 1.0), in_zone("new-b", 25.0, 0.0)], 5), "new-b");
    // Switch back while B is proven unsent. A stays retained in the same zone.
    no_input(&e.restart_search(p, RunMode::Cancellation, true, 6).unwrap());
    assert_eq!(e.snapshot().active_region, Some(0));
    no_input(&ack(&mut e, &b, 7));
    assert_eq!(e.snapshot().attempted_ids, vec!["zone0:a"]);
    seat(observe(&mut e, vec![in_zone("new-a-alias", 10.0, 1.0), in_zone("new-b", 25.0, 0.0)], 8), "new-b");
}

#[test]
fn f9_accepts_a_delivered_initial_booking_wait_with_saved_hidden_buttons() {
    for stored_hidden_buttons in [false, true] {
        let mut p = profile(false, 2);
        p.features.auto_captcha_resume = false;
        p.entry_target = Some(Target::Point { point: Point { x: 100.0, y: 10.0 } });
        if stored_hidden_buttons {
            p.legacy.date = Some(Target::Point { point: Point { x: 200.0, y: 10.0 } });
            p.legacy.session = Some(Target::Point { point: Point { x: 300.0, y: 10.0 } });
            p.legacy.next = Some(Target::Point { point: Point { x: 400.0, y: 10.0 } });
        }
        let mut e = Engine::new(p.clone()).unwrap();
        let first = input(e.step(Event::Start { mode: RunMode::Normal }, 0));
        assert!(e.can_manually_complete_captcha());
        // The separate any-state matrix exercises the explicit override before ACK.
        assert_eq!(e.snapshot().pending_action_id, Some(first.id));
        no_input(&ack(&mut e, &first, 2));
        assert_eq!(e.snapshot().phase, Phase::Entry);
        assert!(e.can_manually_complete_captcha());
        no_input(&e.restart_after_entry(p, 3).unwrap());
        assert_eq!(e.snapshot().phase, Phase::Seats);
        no_input(&ack(&mut e, &first, 4));
        seat(observe(&mut e, vec![candidate("a", 10.0, 0.0)], 5), "a");
    }
}

#[test]
fn valid_manual_handoff_is_not_revoked_by_applying_new_captcha_options() {
    for option in ["manual", "enter", "nol"] {
        let mut p = profile(false, 2);
        p.features.auto_captcha_resume = false;
        p.entry_target = Some(Target::Point { point: Point { x: 100.0, y: 10.0 } });
        let mut e = Engine::new(p.clone()).unwrap();
        let first = input(e.step(Event::Start { mode: RunMode::Normal }, 0));
        no_input(&ack(&mut e, &first, 1));
        assert_eq!(e.snapshot().phase, Phase::Entry);
        assert!(e.can_manually_complete_captcha());
        match option {
            "manual" => p.features.manual_entry = true,
            "enter" => p.features.auto_captcha_resume = true,
            "nol" => p.features.nol_captcha_detection = true,
            _ => unreachable!(),
        }
        no_input(&e.restart_after_entry(p, 2).unwrap());
        assert_eq!(e.snapshot().phase, Phase::Seats, "a handoff approved in the old state must not silently disappear after applying {option}");
        seat(observe(&mut e, vec![candidate("a", 10.0, 0.0)], 3), "a");
    }
}
