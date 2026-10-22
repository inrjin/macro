//! Independent actual Engine transitions for explicit F9 override.
//! Injected delivery/count observations do not establish browser or OS success.
use macro_core::*;

fn target(x: f64) -> Target { Target::Point { point: Point { x, y: 10.0 } } }
fn profile() -> Profile { Profile { target_seats: 2, ..Profile::default() } }
fn candidate(id: &str, x: f64) -> Candidate {
    let point = Point { x, y: 25.0 };
    Candidate { id: id.into(), point, target: Target::Point { point }, color: Profile::default().color,
        selected: false, geometry: Some(CandidateGeometry {
            bounds: Rect { x: x - 4.0, y: 21.0, width: 9.0, height: 9.0 }, pixels: 81,
            object_shape: None, object_context: None,
        }) }
}
fn no_input(actions: &[Action]) {
    assert!(actions.iter().all(|a| matches!(a.kind, ActionKind::Observe { .. })), "{actions:?}");
}
fn one_input(actions: Vec<Action>) -> Action {
    let mut input = actions.into_iter().filter(|a| !matches!(a.kind, ActionKind::Observe { .. }));
    let action = input.next().expect("one input");
    assert!(input.next().is_none());
    action
}
fn ack(engine: &mut Engine, action: &Action, now: u64) -> Vec<Action> {
    engine.step(Event::ActionAcknowledged { id: action.id, success: true, detail: String::new() }, now)
}
fn map(engine: &mut Engine, candidates: Vec<Candidate>, now: u64) -> Vec<Action> {
    engine.step(Event::CandidatesObserved { generation: engine.snapshot().generation, candidates }, now)
}
fn count(engine: &mut Engine, total: usize, now: u64) -> Vec<Action> {
    let request = engine.selection_count_request().unwrap();
    engine.step(Event::SelectionCountObserved { request_id: request.id, observation_id: now, count: total }, now)
}
fn region(engine: &mut Engine, now: u64) -> Action {
    one_input(engine.step(Event::RegionsObserved { candidates: vec![RegionCandidate {
        candidate: candidate("region", 80.0), center_rank: 0, distance_to_stage: 1.0,
    }] }, now))
}

fn at_phase(phase: Phase) -> Engine {
    let mut p = profile();
    match phase {
        Phase::Idle => return Engine::new(p).unwrap(),
        Phase::WaitingSale => { p.features.server_clock = true; p.sale_deadline_ms = Some(1000); }
        Phase::Entry => p.entry_target = Some(target(100.0)),
        Phase::Captcha => p.features.manual_entry = true,
        Phase::Region | Phase::WaitingRegion => p.features.region_priority = true,
        Phase::Conflict => p.features.conflict_recovery = true,
        Phase::WaitingRefresh => p.legacy.cancellation_navigation = CancellationNavigation::ArrowKeys,
        Phase::Completing | Phase::Completed => { p.target_seats = 1; if phase == Phase::Completed { p.legacy.complete = Some(target(900.0)); } }
        _ => {}
    }
    let mode = match phase {
        Phase::WaitingSale | Phase::Entry | Phase::Captcha | Phase::Region | Phase::WaitingRegion => RunMode::Normal,
        Phase::WaitingRefresh => RunMode::Cancellation,
        _ => RunMode::Seats,
    };
    let mut e = Engine::new(p).unwrap();
    e.step(Event::Start { mode }, 0);
    match phase {
        Phase::WaitingRegion => { let input = region(&mut e, 1); ack(&mut e, &input, 2); }
        Phase::Conflict | Phase::Completing | Phase::Completed | Phase::Stopped | Phase::Faulted => {
            let input = one_input(map(&mut e, vec![candidate("a", 10.0)], 1));
            if phase == Phase::Faulted {
                e.step(Event::ActionAcknowledged { id: input.id, success: false, detail: "synthetic transport failure".into() }, 2);
            } else {
                let completion = ack(&mut e, &input, 2);
                if phase == Phase::Completed { let completion = one_input(completion); ack(&mut e, &completion, 3); }
                if phase == Phase::Conflict {
                    e.step(Event::DeliveredSeatResult { action_id: input.id, candidate_id: "a".into(), result: SeatOutcome::Conflict }, 3);
                }
                if phase == Phase::Stopped { e.step(Event::Stop, 3); }
            }
        }
        Phase::Paused => { e.step(Event::Pause, 1); }
        _ => {}
    }
    assert_eq!(e.snapshot().phase, phase, "the setup must reach a real production transition");
    e
}

#[test]
fn f9_accepts_every_reachable_engine_phase_without_replaying_the_previous_work() {
    let phases = [Phase::Idle, Phase::WaitingSale, Phase::Entry, Phase::Captcha, Phase::Region,
        Phase::WaitingRegion, Phase::Seats, Phase::Conflict, Phase::WaitingRefresh, Phase::Completing,
        Phase::Paused, Phase::Completed, Phase::Stopped, Phase::Faulted];
    for phase in phases {
        for region_priority in [false, true] {
            let mut e = at_phase(phase);
            let old = e.snapshot();
            let retained = e.has_retained_selection();
            let mut next = e.profile().clone();
            next.features.region_priority = region_priority;
            next.sale_deadline_ms = None;
            let result = e.restart_after_entry(next, 5000);
            assert!(result.is_ok(), "{phase:?} must accept explicit F9: {:?}", result.err());
            no_input(&result.unwrap());
            assert_eq!(e.snapshot().mode, RunMode::Normal);
            let expected = if phase == Phase::Conflict { Phase::Conflict }
                else if retained || !region_priority { Phase::Seats } else { Phase::Region };
            assert_eq!(e.snapshot().phase, expected, "source {phase:?}, region {region_priority}");
            assert!(e.snapshot().generation > old.generation);
            assert_eq!(e.snapshot().attempted_ids, old.attempted_ids);
            if let Some(id) = old.pending_action_id {
                no_input(&e.step(Event::ActionAcknowledged { id, success: true, detail: String::new() }, 5001));
            }
            no_input(&e.step(Event::CandidatesObserved { generation: old.generation, candidates: vec![candidate("old", 30.0)] }, 5002));
            no_input(&e.step(Event::Tick, 5003));
            // The original sale deadline is long past. It must not resurrect
            // the initial booking/date/session/next/navigation queue.
            no_input(&e.step(Event::Tick, 10_000));
        }
    }
}

#[test]
fn f9_abandons_an_undelivered_initial_or_legacy_entry_queue_instead_of_replaying_it() {
    for initial in [false, true] {
        let mut p = profile();
        p.features.auto_captcha_resume = true;
        if initial { p.entry_target = Some(target(100.0)); }
        p.legacy.date = Some(target(200.0)); p.legacy.session = Some(target(300.0)); p.legacy.next = Some(target(400.0));
        let mut e = Engine::new(p.clone()).unwrap();
        let old = one_input(e.step(Event::Start { mode: RunMode::Normal }, 0));
        no_input(&e.restart_after_entry(p, 1).unwrap());
        assert_eq!(e.snapshot().phase, Phase::Seats);
        no_input(&ack(&mut e, &old, 2));
        no_input(&e.step(Event::Tick, 3));
        let seat = one_input(map(&mut e, vec![candidate("fresh", 10.0)], 4));
        assert!(matches!(&seat.kind, ActionKind::Click { purpose, .. } if purpose == "seat:fresh"));
        assert!(seat.id > old.id);
    }
}

#[test]
fn f9_after_stop_keeps_delivered_seats_but_a_later_stop_still_blocks_all_input() {
    for mode in [RunMode::Normal, RunMode::Cancellation, RunMode::Seats] {
        let mut p = profile(); p.features.manual_entry = true;
        let mut e = Engine::new(p.clone()).unwrap();
        e.step(Event::Start { mode }, 0);
        if mode == RunMode::Normal { no_input(&e.restart_after_entry(p.clone(), 1).unwrap()); }
        let a = one_input(map(&mut e, vec![candidate("a", 10.0)], 2));
        no_input(&ack(&mut e, &a, 3));
        e.step(Event::Stop, 4);
        no_input(&e.restart_after_entry(p, 5).unwrap());
        assert_eq!(e.snapshot().attempted_ids, vec!["a"]);
        let b = one_input(map(&mut e, vec![candidate("a-repainted", 10.0), candidate("b", 30.0)], 6));
        assert!(matches!(&b.kind, ActionKind::Click { purpose, .. } if purpose == "seat:b"));
        e.step(Event::Stop, 7);
        no_input(&ack(&mut e, &b, 8));
        no_input(&map(&mut e, vec![candidate("c", 50.0)], 9));
        no_input(&e.step(Event::Tick, 10));
        assert_eq!(e.snapshot().phase, Phase::Stopped);
        assert_eq!(e.snapshot().attempted_ids, vec!["a"]);
    }
}

#[test]
fn f9_restarts_unknown_and_stopped_count_waits_without_reusing_old_authorization() {
    let mut p = profile(); p.features.nol_selection_count = true;
    let mut e = Engine::new(p.clone()).unwrap();
    no_input(&e.restart_after_entry(p.clone(), 0).unwrap()); // Idle is a valid F9 source.
    no_input(&map(&mut e, vec![candidate("a", 10.0)], 1));
    let a = one_input(count(&mut e, 0, 2));
    no_input(&ack(&mut e, &a, 3));
    let old = e.selection_count_request().unwrap();
    no_input(&e.step(Event::SelectionCountUnavailable { request_id: old.id, observation_id: 4 }, 4));
    e.step(Event::Stop, 5);
    no_input(&e.restart_after_entry(p, 6).unwrap());
    let current = e.selection_count_request().unwrap();
    assert_ne!(old.id, current.id);
    assert_eq!(current.after_action_id, Some(a.id));
    no_input(&e.step(Event::SelectionCountObserved { request_id: old.id, observation_id: 100, count: 2 }, 7));
    no_input(&map(&mut e, vec![candidate("a-alias", 10.0), candidate("b", 30.0)], 8));
    no_input(&count(&mut e, 0, 9));
    let b = one_input(count(&mut e, 1, 10));
    assert!(matches!(&b.kind, ActionKind::Click { purpose, .. } if purpose == "seat:b"));
}

#[test]
fn idle_f9_uses_post_captcha_options_without_waiting_for_sale_or_captcha_detection() {
    for (manual, enter, nol, count_enabled, region_priority) in [
        (false, false, false, false, false), (true, false, false, false, true),
        (false, true, false, true, false), (false, true, true, true, true),
    ] {
        let mut p = profile();
        p.features.manual_entry = manual; p.features.auto_captcha_resume = enter;
        p.features.nol_captcha_detection = nol; p.features.nol_selection_count = count_enabled;
        p.features.region_priority = region_priority; p.features.server_clock = true;
        p.sale_deadline_ms = Some(1_000_000); p.entry_target = Some(target(100.0));
        p.legacy.date = Some(target(200.0));
        let mut e = Engine::new(p.clone()).unwrap();
        no_input(&e.restart_after_entry(p, 1).unwrap());
        assert_eq!(e.snapshot().phase, if region_priority { Phase::Region } else { Phase::Seats });
        assert!(!e.awaits_nol_captcha());
        no_input(&e.step(Event::Tick, 2));
        if region_priority {
            let action = region(&mut e, 3);
            assert!(matches!(&action.kind, ActionKind::Click { purpose, .. } if purpose.starts_with("region:")));
        } else {
            let observed = map(&mut e, vec![candidate("fresh", 10.0)], 3);
            if count_enabled { no_input(&observed); one_input(count(&mut e, 0, 4)); }
            else { one_input(observed); }
        }
    }
}

#[test]
fn stopped_legacy_delivery_without_a_selection_verdict_keeps_its_original_footprint() {
    let mut p = profile();
    p.features.conflict_recovery = true; // Original confirmed-result route.
    let mut e = Engine::new(p.clone()).unwrap();
    no_input(&e.step(Event::Start { mode: RunMode::Normal }, 0));
    assert!(!e.continuous_search());
    let a = one_input(map(&mut e, vec![candidate("a", 10.0)], 1));
    no_input(&ack(&mut e, &a, 2));
    assert_eq!(e.snapshot().current_candidate_id.as_deref(), Some("a"));
    assert!(e.snapshot().attempted_ids.is_empty());
    e.step(Event::Stop, 3);
    assert!(e.snapshot().current_candidate_id.is_none());
    no_input(&e.restart_after_entry(p, 4).unwrap());
    assert_eq!(e.snapshot().attempted_ids, vec!["a"], "remember an actually delivered footprint even when Stop cleared current");
    let b = one_input(map(&mut e, vec![candidate("a-alias", 10.0), candidate("b", 30.0)], 5));
    assert!(matches!(&b.kind, ActionKind::Click { purpose, .. } if purpose == "seat:b"));
}

#[test]
fn an_explicit_f9_event_also_overrides_idle_waiting_sale_and_stopped() {
    for phase in [Phase::Idle, Phase::WaitingSale, Phase::Stopped, Phase::Faulted] {
        let mut e = at_phase(phase);
        let old_generation = e.snapshot().generation;
        no_input(&e.step(Event::CaptchaManuallyCompleted, 5000));
        assert_eq!(e.snapshot().phase, Phase::Seats);
        assert_eq!(e.snapshot().mode, RunMode::Normal);
        assert!(e.snapshot().generation > old_generation);
        no_input(&e.step(Event::Tick, 5001));
    }
}

#[test]
fn f9_after_a_failed_escape_keeps_the_known_conflict_and_rearms_only_the_failed_escape() {
    let mut p = profile();
    p.features.conflict_recovery = true;
    let mut e = Engine::new(p.clone()).unwrap();
    e.step(Event::Start { mode: RunMode::Seats }, 0);
    let seat = one_input(map(&mut e, vec![candidate("a", 10.0)], 1));
    no_input(&ack(&mut e, &seat, 2));
    let escape = one_input(e.step(Event::DeliveredSeatResult {
        action_id: seat.id, candidate_id: "a".into(), result: SeatOutcome::Conflict,
    }, 3));
    assert!(matches!(escape.kind, ActionKind::Key { key: Key::Escape, .. }));
    no_input(&e.step(Event::ActionAcknowledged {
        id: escape.id, success: false, detail: "synthetic failed Escape delivery".into(),
    }, 4));
    assert_eq!(e.snapshot().phase, Phase::Faulted);
    assert!(e.snapshot().conflict_attempt.is_some());
    no_input(&e.restart_after_entry(p, 5).unwrap());
    assert_eq!(e.snapshot().phase, Phase::Conflict, "the previously observed dialog is still unresolved");
    no_input(&ack(&mut e, &escape, 6));
    let retried = one_input(e.step(Event::DeliveredSeatResult {
        action_id: seat.id, candidate_id: "a".into(), result: SeatOutcome::Conflict,
    }, 7));
    assert!(matches!(retried.kind, ActionKind::Key { key: Key::Escape, .. }));
    assert!(retried.id > escape.id);
    no_input(&ack(&mut e, &retried, 8));
    no_input(&e.step(Event::DeliveredSeatResult {
        action_id: seat.id, candidate_id: "a".into(), result: SeatOutcome::Conflict,
    }, 9));
    no_input(&e.step(Event::ConflictDialogGone, 10));
    let next = one_input(map(&mut e, vec![candidate("a-alias", 10.0), candidate("b", 30.0)], 11));
    assert!(matches!(&next.kind, ActionKind::Click { purpose, .. } if purpose == "seat:b"));
}

#[test]
fn f9_from_confirmed_legacy_conflict_does_not_turn_the_known_failed_seat_into_a_kept_attempt() {
    for stopped in [false, true] {
        let mut p = profile(); p.features.conflict_recovery = true;
        let mut e = Engine::new(p.clone()).unwrap();
        e.step(Event::Start { mode: RunMode::Normal }, 0);
        assert!(!e.continuous_search());
        let seat = one_input(map(&mut e, vec![candidate("a", 10.0)], 1));
        no_input(&ack(&mut e, &seat, 2));
        let escape = one_input(e.step(Event::SeatResult {
            candidate_id: "a".into(), result: SeatOutcome::Conflict,
        }, 3));
        no_input(&ack(&mut e, &escape, 4));
        assert_eq!(e.snapshot().phase, Phase::Conflict);
        if stopped { no_input(&e.step(Event::Stop, 5)); }
        no_input(&e.restart_after_entry(p, 6).unwrap());
        if !stopped { assert_eq!(e.snapshot().phase, Phase::Conflict); }
        no_input(&e.step(Event::ConflictDialogGone, 7));
        assert!(e.snapshot().attempted_ids.is_empty(), "a known failure must not become a kept attempt; stopped={stopped}");
        let next = one_input(map(&mut e, vec![candidate("a-alias", 10.0), candidate("b", 30.0)], 8));
        assert!(matches!(&next.kind, ActionKind::Click { purpose, .. } if purpose == "seat:b"));
    }
}

#[test]
fn legacy_conflict_handoff_retries_queued_or_failed_escape_but_never_a_delivered_escape() {
    for delivery in ["queued", "failed", "delivered"] {
        let mut p = profile(); p.features.conflict_recovery = true;
        let mut e = Engine::new(p.clone()).unwrap();
        e.step(Event::Start { mode: RunMode::Normal }, 0);
        let seat = one_input(map(&mut e, vec![candidate("a", 10.0)], 1));
        no_input(&ack(&mut e, &seat, 2));
        let escape = one_input(e.step(Event::SeatResult {
            candidate_id: "a".into(), result: SeatOutcome::Conflict,
        }, 3));
        if delivery != "queued" {
            no_input(&e.step(Event::ActionAcknowledged { id: escape.id,
                success: delivery == "delivered", detail: "synthetic delivery verdict".into() }, 4));
        }
        no_input(&e.restart_after_entry(p, 5).unwrap());
        assert_eq!(e.snapshot().phase, Phase::Conflict);
        assert!(e.snapshot().attempted_ids.is_empty());
        no_input(&ack(&mut e, &escape, 6)); // The old native receipt cannot own the new attempt.
        let retried = e.step(Event::SeatResult { candidate_id: "a".into(), result: SeatOutcome::Conflict }, 7);
        if delivery == "delivered" { no_input(&retried); }
        else {
            let retried = one_input(retried);
            assert!(matches!(retried.kind, ActionKind::Key { key: Key::Escape, .. }));
            assert!(retried.id > escape.id);
            no_input(&ack(&mut e, &retried, 8));
        }
        no_input(&e.step(Event::SeatResult { candidate_id: "a".into(), result: SeatOutcome::Conflict }, 9));
        no_input(&e.step(Event::ConflictDialogGone, 10));
        let b = one_input(map(&mut e, vec![candidate("a-alias", 10.0), candidate("b", 30.0)], 11));
        assert!(matches!(&b.kind, ActionKind::Click { purpose, .. } if purpose == "seat:b"));
    }
}
