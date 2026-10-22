//! Independent review scenarios. Synthetic RGBA and delivered-input ACKs exercise
//! production extraction and Engine logic; they do not establish site acceptance.
use macro_core::*;
use macro_core::vision::{extract_seat_candidates_view_with_stats, RgbaView};

fn pixels(rectangles: &[(usize, usize, usize, usize)]) -> Vec<Candidate> {
    let color = Profile::default().color;
    let mut rgba = [250, 250, 250, 255].repeat(160 * 100);
    for &(left, top, width, height) in rectangles {
        for y in top..top + height {
            for x in left..left + width {
                let offset = (y * 160 + x) * 4;
                rgba[offset..offset + 3].copy_from_slice(&[color.r, color.g, color.b]);
            }
        }
    }
    extract_seat_candidates_view_with_stats(
        RgbaView { width: 160, height: 100, stride: 160 * 4, rgba: &rgba },
        color, 0, &VisionConfig::default(), 0,
    ).unwrap().candidates
}

fn running(count: usize, adjacent: bool) -> Engine {
    let mut profile = Profile::default();
    profile.target_seats = count;
    profile.features.adjacent_seats = adjacent;
    profile.features.seat_priority = true;
    let mut engine = Engine::new(profile).unwrap();
    engine.step(Event::Start { mode: RunMode::Seats }, 0);
    engine
}

fn observe(engine: &mut Engine, candidates: Vec<Candidate>, now: u64) -> Vec<Action> {
    engine.step(Event::CandidatesObserved { generation: engine.snapshot().generation, candidates }, now)
}

fn input(actions: &[Action]) -> &Action {
    actions.iter().find(|action| !matches!(action.kind, ActionKind::Observe { .. }))
        .unwrap_or_else(|| panic!("expected input, got {actions:?}"))
}

fn seat(actions: &[Action]) -> (&str, Point) {
    match &input(actions).kind {
        ActionKind::Click { purpose, target: Target::Point { point } } =>
            (purpose.strip_prefix("seat:").expect("seat action"), *point),
        other => panic!("expected seat click, got {other:?}"),
    }
}

fn ack(engine: &mut Engine, action: &Action, now: u64) -> Vec<Action> {
    engine.step(Event::ActionAcknowledged { id: action.id, success: true, detail: String::new() }, now)
}

fn no_input(actions: &[Action]) {
    assert!(actions.iter().all(|action| matches!(action.kind, ActionKind::Observe { .. })), "{actions:?}");
}

#[test]
fn distant_small_component_cannot_break_an_existing_same_row_pair() {
    // These two 10x10 components have a 2px horizontal gap and a 3px y
    // displacement, within the production row tolerance. The far 2x2 component
    // lies between their center-y values but must not split their grouping.
    for include_distant_small_component in [false, true] {
        let mut rectangles = vec![(0, 0, 10, 10), (10, 20, 10, 10), (22, 23, 10, 10)];
        if include_distant_small_component { rectangles.push((100, 25, 2, 2)); }
        let candidates = pixels(&rectangles);
        assert_eq!(candidates.len(), rectangles.len());
        let mut engine = running(2, true);
        let first = observe(&mut engine, candidates, 1);
        assert_eq!(seat(&first).0, "pixel:10:20", "distant small component present: {include_distant_small_component}");
        let second = ack(&mut engine, input(&first), 2);
        assert_eq!(seat(&second).0, "pixel:22:23");
    }
}

#[test]
fn retained_middle_seat_extends_from_its_neighbor_instead_of_the_group_far_end() {
    let mut engine = running(3, true);
    let first = observe(&mut engine, pixels(&[(34, 30, 10, 10)]), 1);
    ack(&mut engine, input(&first), 2);
    // The far-left member ranks earlier by x, but x22 is immediately beside
    // the retained x34 seat. A distant top pair must not replace that anchor.
    let next = observe(&mut engine, pixels(&[
        (10, 0, 10, 10), (22, 0, 10, 10),
        (10, 30, 10, 10), (22, 30, 10, 10), (46, 30, 10, 10),
    ]), 3);
    assert_eq!(seat(&next).0, "pixel:22:30");
    assert_eq!(seat(&ack(&mut engine, input(&next), 4)).0, "pixel:10:30");
}

#[test]
fn gradual_vertical_drift_does_not_outrank_a_real_three_seat_row() {
    let mut engine = running(3, true);
    // Each step of the upper staircase differs by 3px, but its ends differ
    // by 6px. Chaining pairwise tolerances must not fabricate a three-seat row.
    let first = observe(&mut engine, pixels(&[
        (10, 20, 10, 10), (22, 23, 10, 10), (34, 26, 10, 10),
        (10, 60, 10, 10), (22, 60, 10, 10), (34, 60, 10, 10),
    ]), 1);
    assert_eq!(seat(&first).0, "pixel:10:60");
    let second = ack(&mut engine, input(&first), 2);
    assert_eq!(seat(&second).0, "pixel:22:60");
    assert_eq!(seat(&ack(&mut engine, input(&second), 3)).0, "pixel:34:60");
}

#[test]
fn retained_original_footprint_survives_pause_and_refresh_but_stale_generation_is_inert() {
    let mut engine = running(2, true);
    let first = observe(&mut engine, pixels(&[(10, 20, 10, 10)]), 1);
    ack(&mut engine, input(&first), 2);
    engine.step(Event::Pause, 3);
    engine.step(Event::Resume, 4);
    engine.step(Event::RefreshCompleted { generation: 1 }, 5);
    no_input(&engine.step(Event::CandidatesObserved {
        generation: 0, candidates: pixels(&[(90, 0, 10, 10)]),
    }, 6));
    let next = observe(&mut engine, pixels(&[(12, 22, 6, 6), (22, 20, 10, 10)]), 7);
    assert_eq!(seat(&next).0, "pixel:22:20");
    assert_eq!(engine.snapshot().attempted_ids, vec!["pixel:10:20"]);
}

#[test]
fn geometryless_json_keeps_order_and_exact_point_deduplication() {
    let source = pixels(&[(10, 20, 10, 10), (22, 20, 10, 10)]);
    let mut imported = vec![];
    for candidate in source {
        let mut json = serde_json::to_value(candidate).unwrap();
        json.as_object_mut().unwrap().remove("geometry");
        imported.push(serde_json::from_value::<Candidate>(json).unwrap());
    }
    let mut engine = running(3, true);
    let first = observe(&mut engine, vec![imported[0].clone()], 1);
    ack(&mut engine, input(&first), 2);
    let mut alias = imported[0].clone();
    alias.id = "different-id-same-target".into();
    let next = observe(&mut engine, vec![alias, imported[1].clone()], 3);
    assert_eq!(seat(&next).0, imported[1].id);
    assert_eq!(engine.snapshot().attempted_ids, vec!["pixel:10:20"]);
}

#[test]
fn image_candidates_serialize_metadata_without_pixel_masks() {
    let candidates = pixels(&[(10, 20, 10, 10)]);
    let json = serde_json::to_value(&candidates[0]).unwrap();
    let geometry = json["geometry"].as_object().unwrap();
    assert_eq!(geometry.len(), 2, "only bounds and pixel count belong in JSON");
    assert!(geometry.contains_key("bounds"));
    assert_eq!(geometry["pixels"], 100);
    let restored: Candidate = serde_json::from_value(json).unwrap();
    assert_eq!(restored.id, candidates[0].id);
    assert_eq!(restored.point, candidates[0].point);
}

#[test]
fn missing_adjacent_option_defaults_off_and_off_keeps_front_order() {
    let mut profile_json = serde_json::to_value(Profile::default()).unwrap();
    profile_json["features"].as_object_mut().unwrap().remove("adjacent_seats");
    let restored: Profile = serde_json::from_value(profile_json).unwrap();
    assert!(!restored.features.adjacent_seats);
    let mut engine = running(2, false);
    let first = observe(&mut engine, pixels(&[(0, 0, 10, 10), (10, 20, 10, 10), (22, 20, 10, 10)]), 1);
    assert_eq!(seat(&first).0, "pixel:0:0");
}
