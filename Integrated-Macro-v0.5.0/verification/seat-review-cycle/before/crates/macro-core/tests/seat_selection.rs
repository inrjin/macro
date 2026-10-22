use macro_core::*;

fn candidate(id: &str, x: f64, y: f64, width: f64, height: f64) -> Candidate {
    let point = Point { x: x + width / 2.0, y: y + height / 2.0 };
    Candidate { id: id.into(), point, target: Target::Point { point },
        color: Profile::default().color, selected: false,
        geometry: Some(CandidateGeometry { bounds: Rect { x, y, width, height }, pixels: (width * height) as usize }) }
}
fn engine(count: usize, adjacent: bool) -> Engine {
    let mut p = Profile::default();
    p.target_seats = count;
    p.features.adjacent_seats = adjacent;
    p.features.seat_priority = true;
    p.features.conflict_recovery = true;
    p.legacy.complete = Some(Target::Point { point: Point { x: 900.0, y: 500.0 } });
    p.refresh_interval_ms = 100;
    let mut e = Engine::new(p).unwrap();
    e.step(Event::Start { mode: RunMode::Cancellation }, 0);
    e
}
fn observe(e: &mut Engine, candidates: Vec<Candidate>, now: u64) -> Vec<Action> {
    e.step(Event::CandidatesObserved { generation: e.snapshot().generation, candidates }, now)
}
fn input(actions: &[Action]) -> &Action {
    actions.iter().find(|a| !matches!(a.kind, ActionKind::Observe { .. })).unwrap()
}
fn seat_id(actions: &[Action]) -> &str {
    match &input(actions).kind { ActionKind::Click { purpose, .. } => purpose.strip_prefix("seat:").unwrap(), other => panic!("not a seat: {other:?}") }
}
fn ack(e: &mut Engine, action: &Action, now: u64) -> Vec<Action> {
    e.step(Event::ActionAcknowledged { id: action.id, success: true, detail: String::new() }, now)
}
fn no_input(actions: &[Action]) { assert!(actions.iter().all(|a| matches!(a.kind, ActionKind::Observe { .. }))); }

#[test]
fn real_pixel_extraction_cannot_count_three_shrinking_renders_as_three_seats() {
    let mut e = engine(3, false);
    for inset in 0..3 {
        let mut rgba = [240, 240, 240, 255].repeat(64 * 48);
        let color = e.effective_color();
        for y in 20 + inset..29 - inset { for x in 10 + inset..19 - inset {
            let i = (y * 64 + x) * 4;
            rgba[i..i + 3].copy_from_slice(&[color.r, color.g, color.b]);
        }}
        let candidates = macro_core::vision::extract_candidates(
            &macro_core::vision::RgbaFrame { width: 64, height: 48, rgba },
            color, 0, e.effective_vision(), 0).unwrap();
        assert_eq!(candidates.len(), 1);
        assert!(candidates[0].geometry.is_some());
        let actions = observe(&mut e, candidates, inset as u64 * 2 + 1);
        if inset == 0 { ack(&mut e, input(&actions), 2); } else { no_input(&actions); }
        assert_eq!(e.snapshot().attempted_ids.len(), 1);
        assert_eq!(e.snapshot().phase, Phase::Seats);
    }
}

#[test]
fn touching_small_neighbor_boxes_are_not_treated_as_the_same_seat() {
    let mut e = engine(2, true);
    let first = observe(&mut e, vec![candidate("left", 10.0, 10.0, 2.0, 2.0),
        candidate("right", 12.0, 10.0, 2.0, 2.0)], 1);
    assert_eq!(seat_id(&first), "left");
    let second = ack(&mut e, input(&first), 2);
    assert_eq!(seat_id(&second), "right");
}

#[test]
fn original_component_blocks_shrunk_and_split_aliases_but_not_a_close_neighbor() {
    let mut e = engine(2, false);
    let first = observe(&mut e, vec![candidate("original", 10.0, 10.0, 10.0, 10.0)], 1);
    ack(&mut e, input(&first), 2);
    for now in 3..6 {
        no_input(&observe(&mut e, vec![candidate("inside", 12.0, 12.0, 6.0, 6.0),
            candidate("left-piece", 10.0, 10.0, 2.0, 10.0),
            candidate("right-piece", 18.0, 10.0, 2.0, 10.0)], now));
    }
    let next = observe(&mut e, vec![candidate("alias", 18.0, 10.0, 2.0, 10.0),
        candidate("real-neighbor", 21.0, 10.0, 10.0, 10.0)], 6);
    assert_eq!(seat_id(&next), "real-neighbor");
    let done = ack(&mut e, input(&next), 7);
    assert!(matches!(input(&done).kind, ActionKind::CompleteSelection { .. }));
    assert_eq!(e.snapshot().attempted_ids, vec!["original", "real-neighbor"]);
}

#[test]
fn exact_point_alias_without_geometry_is_blocked_across_refresh_and_pause() {
    let mut e = engine(3, false);
    let mut a = candidate("original", 10.0, 10.0, 10.0, 10.0); a.geometry = None;
    let first = observe(&mut e, vec![a.clone()], 1);
    ack(&mut e, input(&first), 2);
    e.step(Event::Pause, 3); e.step(Event::Resume, 4);
    e.step(Event::RefreshCompleted { generation: 1 }, 5);
    a.id = "alias".into();
    no_input(&observe(&mut e, vec![a], 6));
    assert_eq!(e.snapshot().attempted_ids, vec!["original"]);
}

#[test]
fn proven_undelivered_candidate_does_not_reserve_its_footprint() {
    let mut e = engine(2, false);
    let first = observe(&mut e, vec![candidate("cancelled", 10.0, 10.0, 10.0, 10.0)], 1);
    e.step(Event::ActionCancelled { id: input(&first).id }, 2);
    no_input(&ack(&mut e, input(&first), 3));
    let alias = observe(&mut e, vec![candidate("replacement-id", 12.0, 12.0, 6.0, 6.0)], 4);
    assert_eq!(seat_id(&alias), "replacement-id");
    assert!(e.snapshot().attempted_ids.is_empty());
}

#[test]
fn failed_footprint_is_excluded_until_retry_or_refresh_while_retained_seat_stays_blocked() {
    for refresh in [false, true] {
        let mut e = engine(3, true);
        let first = observe(&mut e, vec![candidate("kept", 10.0, 10.0, 10.0, 10.0),
            candidate("failed", 22.0, 10.0, 10.0, 10.0)], 1);
        let second = ack(&mut e, input(&first), 2);
        ack(&mut e, input(&second), 3);
        let failed = e.snapshot().delivered_attempts[1].clone();
        let escape = e.step(Event::DeliveredSeatResult { action_id: failed.action_id,
            candidate_id: failed.candidate_id, result: SeatOutcome::Conflict }, 4);
        ack(&mut e, input(&escape), 5);
        e.step(Event::ConflictDialogGone, 6);
        let aliases = || vec![candidate("kept-fragment", 12.0, 12.0, 6.0, 6.0),
            candidate("failed-fragment", 24.0, 12.0, 6.0, 6.0)];
        no_input(&observe(&mut e, aliases(), 7));
        if refresh { e.step(Event::RefreshCompleted { generation: 1 }, 8); }
        else { e.step(Event::Tick, 106); }
        let retry = observe(&mut e, aliases(), 107);
        assert_eq!(seat_id(&retry), "failed-fragment");
        assert_eq!(e.snapshot().attempted_ids, vec!["kept"]);
    }
}

#[test]
fn new_run_clears_geometric_marks() {
    let mut e = engine(2, false);
    let first = observe(&mut e, vec![candidate("old", 10.0, 10.0, 10.0, 10.0)], 1);
    ack(&mut e, input(&first), 2);
    e.step(Event::Stop, 3); e.step(Event::Start { mode: RunMode::Cancellation }, 4);
    let fresh = observe(&mut e, vec![candidate("new", 12.0, 12.0, 6.0, 6.0)], 5);
    assert_eq!(seat_id(&fresh), "new");
    assert!(e.snapshot().attempted_ids.is_empty());
}

#[test]
fn failed_geometry_from_another_legacy_zone_does_not_block_the_same_position() {
    let mut p = Profile::default(); p.target_seats = 1; p.refresh_interval_ms = 10_000;
    p.legacy.cancellation_navigation = CancellationNavigation::ArrowKeys;
    let mut e = Engine::new(p).unwrap();
    let nav = e.step(Event::Start { mode: RunMode::Cancellation }, 0);
    ack(&mut e, input(&nav), 1); e.step(Event::Tick, 2);
    let first = observe(&mut e, vec![candidate("zone0", 10.0, 10.0, 10.0, 10.0)], 3);
    ack(&mut e, input(&first), 4);
    e.step(Event::SeatResult { candidate_id: "zone0".into(), result: SeatOutcome::Conflict }, 5);
    e.step(Event::ConflictDialogGone, 6);
    observe(&mut e, vec![], 7);
    let nav = e.step(Event::Tick, 8);
    ack(&mut e, input(&nav), 9); e.step(Event::Tick, 10);
    assert_eq!(e.snapshot().active_region, Some(1));
    let next = observe(&mut e, vec![candidate("zone1", 12.0, 12.0, 6.0, 6.0)], 11);
    assert_eq!(seat_id(&next), "zone1");
}

#[test]
fn full_pair_wins_over_an_earlier_isolated_seat() {
    let mut e = engine(2, true);
    let first = observe(&mut e, vec![candidate("isolated", 0.0, 0.0, 10.0, 10.0),
        candidate("pair-a", 10.0, 20.0, 10.0, 10.0), candidate("pair-b", 22.0, 20.0, 10.0, 10.0)], 1);
    assert_eq!(seat_id(&first), "pair-a");
    assert_eq!(seat_id(&ack(&mut e, input(&first), 2)), "pair-b");
}

#[test]
fn full_triple_wins_over_pair_and_isolated_candidates() {
    let mut e = engine(3, true);
    let first = observe(&mut e, vec![candidate("isolated", 0.0, 0.0, 10.0, 10.0),
        candidate("pair-a", 10.0, 20.0, 10.0, 10.0), candidate("pair-b", 22.0, 20.0, 10.0, 10.0),
        candidate("triple-a", 10.0, 40.0, 10.0, 10.0), candidate("triple-b", 22.0, 40.0, 10.0, 10.0),
        candidate("triple-c", 34.0, 40.0, 10.0, 10.0)], 1);
    assert_eq!(seat_id(&first), "triple-a");
    let second = ack(&mut e, input(&first), 2); assert_eq!(seat_id(&second), "triple-b");
    let third = ack(&mut e, input(&second), 3); assert_eq!(seat_id(&third), "triple-c");
    assert!(matches!(input(&ack(&mut e, input(&third), 4)).kind, ActionKind::CompleteSelection { .. }));
}

#[test]
fn partial_pair_then_separated_fallback_fills_three_without_waiting() {
    let mut e = engine(3, true);
    let first = observe(&mut e, vec![candidate("isolated", 0.0, 0.0, 10.0, 10.0),
        candidate("pair-a", 10.0, 20.0, 10.0, 10.0), candidate("pair-b", 22.0, 20.0, 10.0, 10.0)], 1);
    assert_eq!(seat_id(&first), "pair-a");
    let second = ack(&mut e, input(&first), 2); assert_eq!(seat_id(&second), "pair-b");
    let third = ack(&mut e, input(&second), 3); assert_eq!(seat_id(&third), "isolated");
}

#[test]
fn new_observation_prefers_extending_retained_seat_to_a_distant_new_group() {
    let mut e = engine(3, true);
    let first = observe(&mut e, vec![candidate("kept", 10.0, 30.0, 10.0, 10.0)], 1);
    ack(&mut e, input(&first), 2);
    let next = observe(&mut e, vec![candidate("new-a", 10.0, 0.0, 10.0, 10.0),
        candidate("new-b", 22.0, 0.0, 10.0, 10.0), candidate("beside-kept", 22.0, 30.0, 10.0, 10.0)], 3);
    assert_eq!(seat_id(&next), "beside-kept");
}

#[test]
fn missing_middle_seat_or_different_rows_are_not_an_adjacent_pair() {
    for other in [candidate("far", 34.0, 20.0, 10.0, 10.0), candidate("other-row", 22.0, 32.0, 10.0, 10.0)] {
        let mut e = engine(2, true);
        let first = observe(&mut e, vec![candidate("earlier", 0.0, 0.0, 10.0, 10.0),
            candidate("one", 10.0, 20.0, 10.0, 10.0), other], 1);
        assert_eq!(seat_id(&first), "earlier");
    }
}

#[test]
fn disabled_option_and_geometryless_imports_keep_existing_order() {
    for (adjacent, geometry) in [(false, true), (true, false)] {
        let mut e = engine(2, adjacent);
        let mut seats = vec![candidate("top", 0.0, 0.0, 10.0, 10.0),
            candidate("pair-a", 10.0, 20.0, 10.0, 10.0), candidate("pair-b", 22.0, 20.0, 10.0, 10.0)];
        if !geometry { for seat in &mut seats { seat.geometry = None; } }
        assert_eq!(seat_id(&observe(&mut e, seats, 1)), "top");
    }
}
