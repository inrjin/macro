use macro_core::*;

fn point(x: f64) -> Target { Target::Point { point: Point { x, y: 20.0 } } }
fn candidate(id: &str, x: f64) -> Candidate {
    Candidate { id: id.into(), point: Point { x, y: 20.0 }, target: point(x),
        color: Profile::default().color, selected: false, geometry: None }
}
fn profile(target: usize) -> Profile {
    let mut p = Profile::default();
    p.target_seats = target;
    p.features.nol_selection_count = true;
    p.features.conflict_recovery = true;
    p.legacy.complete = Some(point(900.0));
    p.refresh_interval_ms = 100;
    p
}
fn inputs(actions: &[Action]) -> Vec<&Action> {
    actions.iter().filter(|a| !matches!(a.kind, ActionKind::Observe { .. })).collect()
}
fn start(p: Profile, mode: RunMode) -> Engine {
    let mut e = Engine::new(p).unwrap();
    assert!(inputs(&e.step(Event::Start { mode }, 0)).is_empty());
    e
}
fn observe(e: &mut Engine, names: &[&str], now: u64) -> Vec<Action> {
    e.step(Event::CandidatesObserved { generation: e.snapshot().generation,
        candidates: names.iter().enumerate().map(|(i, id)| candidate(id, 10.0 + i as f64 * 20.0)).collect() }, now)
}
fn count(e: &mut Engine, n: usize, now: u64) -> Vec<Action> {
    let request = e.selection_count_request().expect("count request available");
    e.step(Event::SelectionCountObserved { request_id: request.id, observation_id: now + 1, count: n }, now)
}
fn only_input(actions: &[Action]) -> Action {
    let physical = inputs(actions);
    assert_eq!(physical.len(), 1, "{actions:?}");
    physical[0].clone()
}
fn seat(actions: &[Action], id: &str) -> Action {
    let action = only_input(actions);
    assert!(matches!(&action.kind, ActionKind::Click { purpose, .. } if purpose == &format!("seat:{id}")), "{action:?}");
    action
}
fn ack(e: &mut Engine, action: &Action, now: u64) -> Vec<Action> {
    e.step(Event::ActionAcknowledged { id: action.id, success: true, detail: String::new() }, now)
}
fn conflict(e: &mut Engine, action: &Action, id: &str, now: u64) -> Vec<Action> {
    e.step(Event::DeliveredSeatResult { action_id: action.id, candidate_id: id.into(), result: SeatOutcome::Conflict }, now)
}

#[test]
fn options_default_off_and_existing_profile_roundtrips() {
    let old = serde_json::json!({"features": {"adjacent_seats": true}});
    let p: Profile = serde_json::from_value(old).unwrap();
    assert!(!p.features.nol_selection_count);
    assert!(!p.features.seat_objects);
    assert!(p.features.adjacent_seats);
    let mut p = p;
    p.features.nol_selection_count = true;
    p.features.seat_objects = true;
    let restored: Profile = serde_json::from_value(serde_json::to_value(p).unwrap()).unwrap();
    assert!(restored.features.nol_selection_count && restored.features.seat_objects);
}

#[test]
fn unknown_baseline_never_clicks_and_initial_total_is_not_an_additional_target() {
    for mode in [RunMode::Seats, RunMode::Cancellation, RunMode::Normal] {
        let mut e = start(profile(2), mode);
        assert!(inputs(&observe(&mut e, &["a", "b"], 1)).is_empty());
        for time in [10, 100, 10_000] {
            assert!(inputs(&e.step(Event::Tick, time)).is_empty());
        }
        assert_eq!(e.snapshot().site_selection_count, None);
        let first = seat(&count(&mut e, 1, 10_001), "a");
        assert!(inputs(&ack(&mut e, &first, 10_002)).is_empty());
        let completion = only_input(&count(&mut e, 2, 10_003));
        assert!(matches!(completion.kind, ActionKind::CompleteSelection { .. }));
        assert_eq!(e.snapshot().attempted_ids, vec!["a"]);
        assert!(e.snapshot().selected_ids.is_empty(), "total count does not invent semantic seat IDs");
    }
}

#[test]
fn initial_target_completes_without_seat_input_and_excess_count_holds() {
    let mut satisfied = start(profile(2), RunMode::Seats);
    let completion = only_input(&count(&mut satisfied, 2, 1));
    assert!(matches!(completion.kind, ActionKind::CompleteSelection { .. }));
    assert!(satisfied.snapshot().attempted_ids.is_empty());
    ack(&mut satisfied, &completion, 2);
    assert_eq!(satisfied.snapshot().phase, Phase::Completed);
    assert!(!satisfied.snapshot().selection_count_pending, "completed runs no longer poll the site counter");

    let mut excess = start(profile(2), RunMode::Seats);
    observe(&mut excess, &["a"], 1);
    assert!(inputs(&count(&mut excess, 3, 2)).is_empty());
    assert_eq!(excess.snapshot().site_selection_count, Some(3));
    assert!(excess.snapshot().selection_count_pending);
    assert!(inputs(&e_tick(&mut excess, 1_000)).is_empty());
    // Before any app delivery, the user can correct their preexisting selection.
    assert!(matches!(only_input(&count(&mut excess, 2, 1_001)).kind, ActionKind::CompleteSelection { .. }));
}
fn e_tick(e: &mut Engine, now: u64) -> Vec<Action> { e.step(Event::Tick, now) }

#[test]
fn each_delivery_requires_exact_increment_and_final_fresh_target_before_completion() {
    let mut e = start(profile(2), RunMode::Seats);
    observe(&mut e, &["a", "b"], 1);
    let first = seat(&count(&mut e, 0, 2), "a");
    assert!(e.selection_count_request().is_none(), "queued native input owns the boundary");
    assert!(inputs(&ack(&mut e, &first, 3)).is_empty());
    assert_eq!(e.selection_count_request().unwrap().after_action_id, Some(first.id));
    for time in [4, 100, 1_000, 100_000] {
        assert!(inputs(&count(&mut e, 0, time)).is_empty());
        assert!(inputs(&e.step(Event::Tick, time)).is_empty());
        assert_eq!(e.snapshot().phase, Phase::Seats);
    }
    assert!(inputs(&e.step(Event::SelectionObserved { selected_ids: vec!["a".into(), "b".into()] }, 100_001)).is_empty());
    let second = seat(&count(&mut e, 1, 100_002), "b");
    assert!(inputs(&ack(&mut e, &second, 100_003)).is_empty());
    assert!(inputs(&count(&mut e, 1, 100_004)).is_empty());
    let completion = only_input(&count(&mut e, 2, 100_005));
    assert!(matches!(completion.kind, ActionKind::CompleteSelection { .. }));
    assert_eq!(e.snapshot().attempted_ids, vec!["a", "b"]);
}

#[test]
fn decrease_jump_and_unreadable_do_not_restore_reclick_or_release_pending_effect() {
    let mut e = start(profile(3), RunMode::Seats);
    observe(&mut e, &["a", "b"], 1);
    let a = seat(&count(&mut e, 1, 2), "a");
    ack(&mut e, &a, 3);
    for (time, n) in [(4, 0), (5, 3), (6, 1)] {
        assert!(inputs(&count(&mut e, n, time)).is_empty());
        assert!(e.snapshot().selection_count_pending);
        assert_eq!(e.snapshot().attempted_ids, vec!["a"]);
    }
    assert!(inputs(&e.step(Event::Tick, 100_000)).is_empty(), "unreadable is absence of an event, never zero");
    seat(&count(&mut e, 2, 100_001), "b");
}

#[test]
fn delayed_next_input_requires_another_fresh_observation_not_tick() {
    let mut p = profile(2);
    p.legacy.normal_click_delay_ms = 100;
    let mut e = start(p, RunMode::Seats);
    observe(&mut e, &["a", "b"], 1);
    let a = seat(&count(&mut e, 0, 2), "a");
    ack(&mut e, &a, 3);
    assert!(inputs(&count(&mut e, 1, 4)).is_empty());
    assert!(!e.snapshot().selection_count_pending);
    assert!(inputs(&e.step(Event::Tick, 103)).is_empty());
    let b = seat(&count(&mut e, 1, 104), "b");
    ack(&mut e, &b, 105);
    assert!(inputs(&count(&mut e, 2, 106)).is_empty());
    assert!(inputs(&e.step(Event::Tick, 205)).is_empty());
    assert!(matches!(only_input(&count(&mut e, 2, 206)).kind, ActionKind::CompleteSelection { .. }));
}

#[test]
fn request_rotation_blocks_old_context_refresh_and_run_results() {
    let mut e = start(profile(2), RunMode::Seats);
    observe(&mut e, &["a", "b"], 1);
    let original = e.selection_count_request().unwrap();
    let a = seat(&count(&mut e, 0, 2), "a");
    ack(&mut e, &a, 3);
    let after_a = e.selection_count_request().unwrap();
    assert_ne!(original.id, after_a.id);
    assert!(inputs(&e.step(Event::SelectionCountObserved { request_id: original.id, observation_id: 10_000, count: 2 }, 4)).is_empty());
    e.step(Event::ContextLost { reason: "fixture".into() }, 5);
    assert!(e.selection_count_request().is_none());
    e.step(Event::Resume, 6);
    let resumed = e.selection_count_request().unwrap();
    assert_ne!(resumed.id, after_a.id);
    assert!(inputs(&e.step(Event::SelectionCountObserved { request_id: after_a.id, observation_id: 20_000, count: 2 }, 7)).is_empty());
    e.step(Event::RefreshCompleted { generation: 1 }, 8);
    let refreshed = e.selection_count_request().unwrap();
    assert_ne!(refreshed.id, resumed.id);
    assert_eq!(refreshed.generation, 1);
    assert_eq!(refreshed.after_action_id, Some(a.id));
    observe(&mut e, &["a", "b"], 9);
    assert!(inputs(&count(&mut e, 0, 10)).is_empty(), "refresh cannot erase unresolved delivery");
    seat(&count(&mut e, 1, 11), "b");
    e.step(Event::Stop, 12);
    e.step(Event::Start { mode: RunMode::Seats }, 13);
    let new_run = e.selection_count_request().unwrap();
    assert_ne!(new_run.id, refreshed.id);
    assert_eq!(new_run.after_action_id, None);
    assert_eq!(e.snapshot().site_selection_count, None);
}

#[test]
fn duplicate_observation_cannot_reauthorize_after_delay() {
    let mut p = profile(2);
    p.legacy.normal_click_delay_ms = 100;
    let mut e = start(p, RunMode::Seats);
    observe(&mut e, &["a", "b"], 1);
    let a = seat(&count(&mut e, 0, 2), "a");
    ack(&mut e, &a, 3);
    let request = e.selection_count_request().unwrap();
    assert!(inputs(&count(&mut e, 1, 4)).is_empty());
    assert!(inputs(&e.step(Event::SelectionCountObserved { request_id: request.id, observation_id: 5, count: 1 }, 104)).is_empty());
    assert!(inputs(&e.step(Event::SelectionCountObserved { request_id: request.id, observation_id: 4, count: 2 }, 105)).is_empty());
    seat(&count(&mut e, 1, 106), "b");
}

#[test]
fn unreadable_observation_invalidates_verified_status_without_inventing_zero_or_erasing_pending_effect() {
    let mut p = profile(2);
    p.legacy.normal_click_delay_ms = 100;
    let mut e = start(p, RunMode::Seats);
    observe(&mut e, &["a", "b"], 1);
    let a = seat(&count(&mut e, 0, 2), "a");
    ack(&mut e, &a, 3);
    assert!(inputs(&count(&mut e, 1, 4)).is_empty());
    assert!(!e.snapshot().selection_count_pending);
    let request = e.selection_count_request().unwrap();
    assert!(inputs(&e.step(Event::SelectionCountUnavailable { request_id: request.id, observation_id: 6 }, 5)).is_empty());
    assert!(e.snapshot().selection_count_pending);
    assert_eq!(e.snapshot().site_selection_count, Some(1), "show last known count, not fabricated zero");
    assert!(inputs(&e.step(Event::SelectionCountObserved { request_id: request.id, observation_id: 6, count: 1 }, 104)).is_empty());
    let b = seat(&count(&mut e, 1, 105), "b");
    ack(&mut e, &b, 106);
    let pending = e.selection_count_request().unwrap();
    assert!(inputs(&e.step(Event::SelectionCountUnavailable { request_id: request.id, observation_id: 1000 }, 107)).is_empty());
    assert!(inputs(&e.step(Event::SelectionCountUnavailable { request_id: pending.id, observation_id: 108 }, 107)).is_empty());
    assert!(inputs(&count(&mut e, 1, 206)).is_empty());
    assert!(matches!(only_input(&count(&mut e, 2, 207)).kind, ActionKind::CompleteSelection { .. }));
}

#[test]
fn cancelled_undelivered_seat_is_not_counted_and_old_ack_is_inert() {
    let mut e = start(profile(1), RunMode::Seats);
    observe(&mut e, &["a"], 1);
    let a = seat(&count(&mut e, 0, 2), "a");
    e.step(Event::ActionCancelled { id: a.id }, 3);
    assert!(inputs(&ack(&mut e, &a, 4)).is_empty());
    assert!(e.snapshot().attempted_ids.is_empty());
    assert_eq!(e.selection_count_request().unwrap().after_action_id, None);
    observe(&mut e, &["a"], 5);
    seat(&count(&mut e, 0, 6), "a");
}

#[test]
fn known_failure_reconciles_count_and_keeps_other_delivered_seat() {
    let mut e = start(profile(2), RunMode::Cancellation);
    observe(&mut e, &["a", "b", "c"], 1);
    let a = seat(&count(&mut e, 0, 2), "a");
    ack(&mut e, &a, 3);
    let b = seat(&count(&mut e, 1, 4), "b");
    ack(&mut e, &b, 5);
    let escape = only_input(&conflict(&mut e, &b, "b", 6));
    assert!(matches!(escape.kind, ActionKind::Key { key: Key::Escape, .. }));
    ack(&mut e, &escape, 7);
    e.step(Event::ConflictDialogGone, 8);
    observe(&mut e, &["a", "b", "c"], 9);
    assert_eq!(e.snapshot().attempted_ids, vec!["a"]);
    assert_eq!(e.snapshot().rejected_ids, vec!["b"]);
    assert_eq!(e.selection_count_request().unwrap().after_action_id, Some(escape.id));
    let c = seat(&count(&mut e, 1, 10), "c");
    ack(&mut e, &c, 11);
    assert!(matches!(only_input(&count(&mut e, 2, 12)).kind, ActionKind::CompleteSelection { .. }));
}

#[test]
fn late_known_failure_during_next_pending_effect_requires_adjusted_total() {
    let mut e = start(profile(3), RunMode::Seats);
    observe(&mut e, &["a", "b", "c"], 1);
    let a = seat(&count(&mut e, 0, 2), "a");
    ack(&mut e, &a, 3);
    let b = seat(&count(&mut e, 1, 4), "b");
    ack(&mut e, &b, 5);
    let escape = only_input(&conflict(&mut e, &a, "a", 6));
    ack(&mut e, &escape, 7);
    e.step(Event::ConflictDialogGone, 8);
    observe(&mut e, &["a", "b", "c"], 9);
    assert!(inputs(&count(&mut e, 0, 10)).is_empty(), "B is still an unresolved delivery");
    seat(&count(&mut e, 1, 11), "c");
    assert_eq!(e.snapshot().attempted_ids, vec!["b"]);
}

#[test]
fn automatic_refresh_needs_fresh_zero_and_never_discards_preexisting_or_pending_selection() {
    for initial in [0, 1] {
        let mut p = profile(2);
        p.features.automatic_refresh = true;
        p.refresh_target = Some(point(800.0));
        let mut e = start(p, RunMode::Cancellation);
        observe(&mut e, &[], 1);
        assert!(inputs(&e.step(Event::Tick, 200)).is_empty());
        let actions = count(&mut e, initial, 201);
        if initial == 0 {
            let refresh = only_input(&actions);
            assert!(matches!(refresh.kind, ActionKind::Refresh { .. }));
            ack(&mut e, &refresh, 202);
            assert_eq!(e.selection_count_request().unwrap().after_action_id, Some(refresh.id));
        } else { assert!(inputs(&actions).is_empty()); }
    }
    let mut e = start(profile(2), RunMode::Cancellation);
    observe(&mut e, &[], 1);
    for now in [200, 1_000, 10_000] { assert!(inputs(&count(&mut e, 0, now)).is_empty()); }
}

#[test]
fn legacy_navigation_is_preserved_but_initial_selection_is_read_before_it() {
    for initial in [0, 1] {
        let mut p = profile(2);
        p.features.conflict_recovery = false;
        p.legacy.cancellation_navigation = CancellationNavigation::ArrowKeys;
        p.legacy.region_delay_ms = 10;
        let mut e = start(p, RunMode::Cancellation);
        assert!(inputs(&e.step(Event::Tick, 10)).is_empty());
        let actions = count(&mut e, initial, 11);
        if initial == 0 {
            let navigation = only_input(&actions);
            assert!(matches!(navigation.kind, ActionKind::Key { key: Key::ArrowDown, .. }));
            ack(&mut e, &navigation, 12);
            e.step(Event::Tick, 22);
            assert_eq!(e.snapshot().generation, 1);
            assert_eq!(e.selection_count_request().unwrap().after_action_id, Some(navigation.id));
        } else {
            assert!(inputs(&actions).is_empty());
            observe(&mut e, &["a"], 12);
            seat(&count(&mut e, 1, 13), "a");
        }
    }
}

#[test]
fn normal_entry_and_captcha_order_are_unchanged_by_count_option() {
    let mut p = profile(1);
    p.legacy.date = Some(point(100.0));
    p.legacy.session = Some(point(200.0));
    p.legacy.next = Some(point(300.0));
    p.features.nol_captcha_detection = true;
    let mut e = Engine::new(p).unwrap();
    let first = only_input(&e.step(Event::Start { mode: RunMode::Normal }, 0));
    assert!(e.selection_count_request().is_none());
    let second = only_input(&ack(&mut e, &first, 1));
    let third = only_input(&ack(&mut e, &second, 2));
    assert!(inputs(&ack(&mut e, &third, 3)).is_empty());
    assert_eq!(e.snapshot().phase, Phase::Captcha);
    assert!(e.selection_count_request().is_none());
    e.step(Event::NolCaptchaDismissed, 4);
    assert_eq!(e.snapshot().phase, Phase::Seats);
    assert!(e.selection_count_request().is_some());
    assert!(inputs(&observe(&mut e, &["a"], 5)).is_empty());
    seat(&count(&mut e, 0, 6), "a");
}
