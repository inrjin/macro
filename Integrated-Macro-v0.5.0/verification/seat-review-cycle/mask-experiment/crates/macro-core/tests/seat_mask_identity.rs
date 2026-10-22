use macro_core::*;
use macro_core::vision::{extract_seat_candidates_view_with_stats, RgbaFrame, RgbaView};

fn frame(rects: &[(usize, usize, usize, usize)]) -> RgbaFrame {
    let mut rgba = [245, 245, 245, 255].repeat(64 * 48);
    let color = Profile::default().color;
    for &(x, y, width, height) in rects {
        for py in y..y + height { for px in x..x + width {
            let i = (py * 64 + px) * 4;
            rgba[i..i + 3].copy_from_slice(&[color.r, color.g, color.b]);
        }}
    }
    RgbaFrame { width: 64, height: 48, rgba }
}

fn extract(frame: &RgbaFrame) -> Vec<Candidate> {
    extract_seat_candidates_view_with_stats(RgbaView {
        width: frame.width, height: frame.height, stride: frame.width as usize * 4, rgba: &frame.rgba,
    }, Profile::default().color, 0, &VisionConfig::default(), 0).unwrap().candidates
}

fn engine(count: usize) -> Engine {
    let mut profile = Profile::default();
    profile.target_seats = count;
    profile.features.seat_priority = true;
    profile.features.conflict_recovery = true;
    profile.refresh_interval_ms = 100;
    profile.legacy.complete = Some(Target::Point { point: Point { x: 60.0, y: 40.0 } });
    let mut engine = Engine::new(profile).unwrap();
    engine.step(Event::Start { mode: RunMode::Cancellation }, 0);
    engine
}

fn observe(engine: &mut Engine, candidates: Vec<Candidate>, now: u64) -> Vec<Action> {
    engine.step(Event::CandidatesObserved { generation: engine.snapshot().generation, candidates }, now)
}

fn seat(actions: &[Action]) -> &Action {
    actions.iter().find(|action| matches!(&action.kind, ActionKind::Click { purpose, .. } if purpose.starts_with("seat:")))
        .unwrap_or_else(|| panic!("expected a seat click, got {actions:?}"))
}

fn ack(engine: &mut Engine, action: &Action, now: u64) -> Vec<Action> {
    engine.step(Event::ActionAcknowledged { id: action.id, success: true, detail: String::new() }, now)
}

fn no_click(actions: &[Action]) {
    assert!(!actions.iter().any(|action| matches!(action.kind, ActionKind::Click { .. } | ActionKind::CompleteSelection { .. })), "{actions:?}");
}

#[test]
fn separate_seat_inside_concave_component_bounds_remains_selectable() {
    // The U has a genuine empty interior. Its bounding box also contains an
    // independently connected 3x3 seat; neither color component shares pixels.
    let candidates = extract(&frame(&[(2, 2, 16, 2), (2, 4, 2, 10), (16, 4, 2, 10), (8, 10, 3, 3)]));
    assert_eq!(candidates.len(), 2);
    assert_eq!(candidates[0].id, "pixel:2:2");
    assert_eq!(candidates[1].id, "pixel:8:10");
    assert!(candidates[0].geometry.as_ref().unwrap().bounds.contains(candidates[1].point));
    let mut engine = engine(2);
    let first = observe(&mut engine, candidates, 1);
    let second = ack(&mut engine, seat(&first), 2);
    assert!(matches!(&seat(&second).kind, ActionKind::Click { purpose, .. } if purpose == "seat:pixel:8:10"));
    let done = ack(&mut engine, seat(&second), 3);
    assert_eq!(engine.snapshot().attempted_ids.len(), 2);
    assert!(done.iter().any(|action| matches!(action.kind, ActionKind::CompleteSelection { .. })));
}

#[test]
fn original_mask_survives_shrink_split_pause_and_refresh_without_expanding_to_neighbors() {
    let mut engine = engine(3);
    let first = observe(&mut engine, extract(&frame(&[(10, 10, 10, 10)])), 1);
    ack(&mut engine, seat(&first), 2);
    no_click(&observe(&mut engine, extract(&frame(&[(12, 12, 6, 6)])), 3));
    engine.step(Event::Pause, 4);
    engine.step(Event::Resume, 5);
    engine.step(Event::RefreshCompleted { generation: 1 }, 6);
    // Both pieces are subsets of the ORIGINAL mask. The right piece must not
    // escape because an intermediate observation showed only the middle.
    no_click(&observe(&mut engine, extract(&frame(&[(10, 10, 2, 10), (18, 10, 2, 10)])), 7));
    let next = observe(&mut engine, extract(&frame(&[(18, 10, 2, 10), (21, 10, 2, 10)])), 8);
    assert!(matches!(&seat(&next).kind, ActionKind::Click { purpose, .. } if purpose == "seat:pixel:21:10"));
    assert_eq!(engine.snapshot().attempted_ids, vec!["pixel:10:10"]);
}

#[test]
fn failed_mask_can_retry_but_retained_mask_is_not_discarded() {
    for explicit_refresh in [false, true] {
        let mut engine = engine(3);
        let first = observe(&mut engine, extract(&frame(&[(10, 10, 10, 10), (22, 10, 10, 10)])), 1);
        let second = ack(&mut engine, seat(&first), 2);
        ack(&mut engine, seat(&second), 3);
        let failed = engine.snapshot().delivered_attempts[1].clone();
        let dismiss = engine.step(Event::DeliveredSeatResult { action_id: failed.action_id,
            candidate_id: failed.candidate_id, result: SeatOutcome::Conflict }, 4);
        let escape = dismiss.iter().find(|action| matches!(action.kind, ActionKind::Key { key: Key::Escape, .. })).unwrap();
        ack(&mut engine, escape, 5);
        engine.step(Event::ConflictDialogGone, 6);
        let shrunk = || extract(&frame(&[(12, 12, 6, 6), (24, 12, 6, 6)]));
        no_click(&observe(&mut engine, shrunk(), 7));
        if explicit_refresh { engine.step(Event::RefreshCompleted { generation: 1 }, 8); }
        else { engine.step(Event::Tick, 106); }
        let retry = observe(&mut engine, shrunk(), 107);
        assert!(matches!(&seat(&retry).kind, ActionKind::Click { purpose, .. } if purpose == "seat:pixel:24:12"));
        assert_eq!(engine.snapshot().attempted_ids, vec!["pixel:10:10"]);
    }
}

#[test]
fn cancelled_input_and_new_run_do_not_reserve_old_masks() {
    let mut engine = engine(2);
    let first = observe(&mut engine, extract(&frame(&[(10, 10, 10, 10)])), 1);
    engine.step(Event::ActionCancelled { id: seat(&first).id }, 2);
    no_click(&ack(&mut engine, seat(&first), 3));
    let replacement = observe(&mut engine, extract(&frame(&[(12, 12, 6, 6)])), 4);
    ack(&mut engine, seat(&replacement), 5);
    assert_eq!(engine.snapshot().attempted_ids, vec!["pixel:12:12"]);
    engine.step(Event::Stop, 6);
    engine.step(Event::Start { mode: RunMode::Cancellation }, 7);
    let new_run = observe(&mut engine, extract(&frame(&[(13, 13, 4, 4)])), 8);
    assert!(matches!(&seat(&new_run).kind, ActionKind::Click { purpose, .. } if purpose == "seat:pixel:13:13"));
    assert!(engine.snapshot().attempted_ids.is_empty());
}

#[test]
fn masks_are_shared_and_excluded_from_serialization_and_verbose_debug() {
    let candidate = extract(&frame(&[(1, 1, 40, 30)])).remove(0);
    let clone = candidate.clone();
    let a = candidate.geometry.as_ref().unwrap().mask.as_ref().unwrap();
    let b = clone.geometry.as_ref().unwrap().mask.as_ref().unwrap();
    assert!(std::sync::Arc::ptr_eq(a, b));
    assert!(format!("{a:?}").len() < 100);
    let mut encoded = serde_json::to_value(&candidate).unwrap();
    assert_eq!(encoded["geometry"].as_object().unwrap().len(), 2);
    assert!(encoded["geometry"].get("mask").is_none());
    // Deserialization cannot manufacture a trusted native mask.
    encoded["geometry"]["mask"] = serde_json::json!({"runs": [1, 2, 3], "pixels": 9});
    let decoded: Candidate = serde_json::from_value(encoded).unwrap();
    assert!(decoded.geometry.as_ref().unwrap().mask.is_none());
    assert_eq!(decoded.geometry.as_ref().unwrap().bounds, candidate.geometry.as_ref().unwrap().bounds);
    assert_eq!(decoded.geometry.as_ref().unwrap().pixels, candidate.geometry.as_ref().unwrap().pixels);
}
