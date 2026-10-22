//! Pixel proposals are exercised through the real Engine. Site hit ownership
//! and selected cardinality come from the independent analytic scene renderer,
//! never candidate IDs. These tests do not execute the text reader or OS input.
#[path = "support/seat_object_shapes.rs"]
mod fixtures;

use fixtures::*;
use macro_core::{seat_objects::{extract_seat_objects_view_with_stats,
    seat_object_candidate_supported}, *};

fn extract(scene: &Scene) -> Vec<Candidate> {
    extract_seat_objects_view_with_stats(scene.view(), AVAILABLE, 12,
        &VisionConfig::default(), 0).unwrap().candidates
}
fn proposals(scene: &Scene, objects: bool) -> Vec<Candidate> {
    if objects { extract(scene) } else {
        macro_core::vision::extract_seat_candidates_view_with_stats(scene.view(), AVAILABLE,
            12, &VisionConfig::default(), 0).unwrap().candidates
    }
}
fn inputs(actions: Vec<Action>) -> Vec<Action> {
    actions.into_iter().filter(|a| !matches!(a.kind, ActionKind::Observe { .. })).collect()
}
fn observe(engine: &mut Engine, candidates: &[Candidate], now: u64) -> Vec<Action> {
    let generation = engine.snapshot().generation;
    engine.step(Event::CandidatesObserved { generation, candidates: candidates.to_vec() }, now)
}
fn count(engine: &mut Engine, total: usize, now: u64) -> Vec<Action> {
    let request = engine.selection_count_request().expect("fresh counter request");
    engine.step(Event::SelectionCountObserved { request_id: request.id,
        observation_id: now, count: total }, now)
}
fn engine(target: usize, count_enabled: bool, adjacent: bool, objects: bool) -> Engine {
    let mut profile = Profile::default();
    profile.target_seats = target;
    profile.color = AVAILABLE;
    profile.features.seat_objects = objects;
    profile.features.seat_priority = true;
    profile.features.adjacent_seats = adjacent;
    profile.features.nol_selection_count = count_enabled;
    profile.features.conflict_recovery = true;
    profile.legacy.complete = Some(Target::Point { point: Point { x: 310.0, y: 170.0 } });
    let mut engine = Engine::new(profile).unwrap();
    assert!(inputs(engine.step(Event::Start { mode: RunMode::Seats }, 0)).is_empty());
    engine
}

/// Exact accepted click/toggle oracle. The external count is supplied only
/// after the independently owned site object has actually been toggled.
fn run(scene: Scene, target: usize, verified: bool, adjacent: bool) -> Vec<usize> {
    run_mode(scene, target, verified, adjacent, true)
}
fn run_mode(mut scene: Scene, target: usize, verified: bool, adjacent: bool, objects: bool) -> Vec<usize> {
    let mut engine = engine(target, verified, adjacent, objects);
    let mut selected = vec![false; scene.objects.len()];
    let mut hits = vec![];
    let mut now = 1;
    let mut observed = proposals(&scene, objects);
    let mut queued = inputs(observe(&mut engine, &observed, now));
    for _ in 0..32 {
        now += 1;
        if queued.is_empty() {
            observed = proposals(&scene, objects);
            queued = inputs(observe(&mut engine, &observed, now));
            if verified && queued.is_empty() {
                now += 1;
                queued = inputs(count(&mut engine, selected.iter().filter(|v| **v).count(), now));
            }
        }
        assert_eq!(queued.len(), 1, "supported fixture must make progress: {:?}", engine.snapshot());
        let action = queued.remove(0);
        match &action.kind {
            ActionKind::Click { target: Target::Point { point }, purpose } if purpose.starts_with("seat:") => {
                // Keep the proposal that produced this queued input. A new
                // extraction must not silently replace the original proof.
                let candidate = observed.iter().find(|c| purpose == &format!("seat:{}", c.id))
                    .expect("queued input retains its original candidate proof");
                assert_eq!(candidate.point, *point);
                if objects {
                    assert!(seat_object_candidate_supported(scene.view(), candidate,
                        AVAILABLE, 12, &VisionConfig::default()));
                } else {
                    assert!(candidate.geometry.as_ref().unwrap().object_shape.is_none());
                    assert!(macro_core::vision::seat_point_supported(scene.view(), *point,
                        AVAILABLE, 12, &VisionConfig::default()));
                }
                let owners = scene.hits(*point);
                assert_eq!(owners.len(), 1, "candidate must hit one independent site object");
                let id = owners[0];
                assert!(!selected[id], "reclick would toggle a retained seat off");
                assert!(scene.objects[id].available, "inactive objects are not seats to acquire");
                hits.push(id);
                selected[id] = !selected[id];
                scene.selected(id);
                let prior_count = selected.iter().filter(|v| **v).count() - 1;
                now += 1;
                queued = inputs(engine.step(Event::ActionAcknowledged {
                    id: action.id, success: true, detail: String::new() }, now));
                if verified {
                    assert!(queued.is_empty(), "native delivery is not selected cardinality");
                    now += 1;
                    assert!(inputs(count(&mut engine, prior_count, now)).is_empty(),
                        "lagging display cannot authorize another click or completion");
                }
            }
            ActionKind::CompleteSelection { .. } => {
                assert_eq!(selected.iter().filter(|v| **v).count(), target);
                assert_eq!(hits.len(), target);
                return hits;
            }
            other => panic!("unexpected input: {other:?}"),
        }
    }
    panic!("supported fixture failed to complete within a bounded event sequence");
}

#[test]
fn independent_shape_families_select_two_distinct_objects_with_count_on_and_off() {
    for (name, shape, angle) in families() {
        for verified in [false, true] {
            let hits = run(repeated(shape, angle), 2, verified, false);
            assert_eq!(hits, [8, 9], "family={name}, count={verified}");
        }
    }
}

#[test]
fn all_available_maps_keep_learning_and_progress_without_gray_reference_objects() {
    for (name, shape, angle) in families() {
        let layout = repeated(shape, angle);
        let mut scene = Scene::new(320, 180);
        for object in layout.objects {
            scene.add(object.shape, object.center, object.angle, true);
        }
        assert_eq!(scene.available_ids().len(), 10);
        for verified in [false, true] {
            assert_eq!(run(scene.clone(), 3, verified, false), [0, 1, 2],
                "available-color observations still supply repeated whole boundaries: {name}, count={verified}");
        }
    }
}

#[test]
fn optional_object_mode_off_keeps_the_ordinary_color_pipeline_with_either_count_policy() {
    for (name, shape, angle) in families() {
        for verified in [false, true] {
            assert_eq!(run_mode(repeated(shape, angle), 2, verified, false, false), [8, 9],
                "ordinary color pipeline, family={name}, count={verified}");
        }
    }
}

#[test]
fn internal_dark_color_splits_do_not_create_multiple_engine_seats() {
    for (name, shape, angle) in families() {
        if name == "small_square7" { continue; } // A cross can cover most of this tiny fill.
        let mut scene = repeated(shape, angle);
        scene.dark_vertical_mark(8);
        scene.dark_cross(9);
        for verified in [false, true] {
            let mut hits = run(scene.clone(), 2, verified, true);
            hits.sort_unstable();
            assert_eq!(hits, [8, 9], "family={name}");
        }
    }
}

fn slanted_close_pair(isolated: bool) -> Scene {
    slanted_close_pair_shifted(isolated, 0.0)
}
fn slanted_close_pair_shifted(isolated: bool, shift: f64) -> Scene {
    let shape = Shape::Rectangle { width: 37.0, height: 7.0 };
    let mut scene = Scene::new(320, 180);
    for row in 0..2 { for column in 0..4 {
        scene.add(shape, Point { x: 30.5 + column as f64 * 60.0,
            y: 25.5 + row as f64 * 36.0 }, 45.0, false);
    }}
    scene.add(shape, Point { x: 65.5 + shift, y: 125.5 }, 45.0, true);
    scene.add(shape, Point { x: 79.5 + shift, y: 125.5 }, 45.0, true);
    assert!(scene.objects[8].pixels.iter().all(|p| !scene.objects[9].pixels.contains(p)),
        "fixture site objects must be physically disjoint");
    if isolated { scene.add(shape, Point { x: 250.5, y: 95.5 }, 45.0, true); }
    scene
}

#[test]
fn overlapping_boxes_do_not_merge_independent_slanted_neighbors() {
    let scene = slanted_close_pair(false);
    let found = extract(&scene);
    assert_eq!(found.len(), 2);
    assert!(found[0].geometry.as_ref().unwrap().bounds.contains(found[1].point)
        || found[1].geometry.as_ref().unwrap().bounds.contains(found[0].point),
        "this regression must exercise bbox containment, not just close centers: {found:?}");
    for verified in [false, true] {
        assert_eq!(run(scene.clone(), 2, verified, true), [8, 9]);
    }
}

#[test]
fn slanted_same_row_pair_outranks_isolated_seat_only_when_adjacent_enabled() {
    for verified in [false, true] {
        assert_eq!(run(slanted_close_pair(true), 2, verified, true), [8, 9]);
        assert_eq!(run(slanted_close_pair(true), 2, verified, false)[0], 10);
    }
}

#[test]
fn stale_nonround_shape_is_rejected_even_if_its_old_click_patch_stays_available() {
    for (name, shape, angle) in families() {
        let mut scene = repeated(shape, angle);
        let candidate = extract(&scene).into_iter()
            .find(|candidate| scene.hits(candidate.point) == [8]).expect(name);
        scene.paint_object(8, WHITE);
        let point = candidate.point;
        for y in point.y as usize..point.y as usize + 2 {
            for x in point.x as usize..point.x as usize + 2 { scene.paint(x, y, AVAILABLE); }
        }
        assert!(macro_core::vision::seat_point_supported(scene.view(), point,
            AVAILABLE, 12, &VisionConfig::default()));
        assert!(!seat_object_candidate_supported(scene.view(), &candidate,
            AVAILABLE, 12, &VisionConfig::default()), "family={name}");
    }
}

#[test]
fn serialized_candidates_do_not_recreate_process_local_shape_authorization() {
    let scene = repeated(Shape::Rectangle { width: 19.0, height: 11.0 }, 0.0);
    let candidate = extract(&scene).remove(0);
    assert!(candidate.geometry.as_ref().unwrap().object_shape.is_some());
    assert!(candidate.geometry.as_ref().unwrap().object_context.is_some());
    let json = serde_json::to_value(&candidate).unwrap();
    assert!(json["geometry"].get("object_shape").is_none(), "shape storage is not IPC/profile data");
    assert!(json["geometry"].get("object_context").is_none(), "observation authorization is process-local too");
    let restored: Candidate = serde_json::from_value(json).unwrap();
    assert!(restored.geometry.as_ref().unwrap().object_shape.is_none());
    assert!(restored.geometry.as_ref().unwrap().object_context.is_none());
    assert!(!seat_object_candidate_supported(scene.view(), &restored,
        AVAILABLE, 12, &VisionConfig::default()), "an imported bbox is not learned-object proof");
    assert!(macro_core::vision::seat_point_supported(scene.view(), restored.point,
        AVAILABLE, 12, &VisionConfig::default()), "ordinary color mode keeps its independent point contract");
}

#[test]
fn zone_prefix_and_benign_roi_changes_preserve_original_point_authorization() {
    let mut scene = repeated(Shape::Rectangle { width: 19.0, height: 11.0 }, 0.0);
    let config = VisionConfig { roi: Some(Rect { x: 0.0, y: 0.0, width: 280.0, height: 160.0 }),
        ..VisionConfig::default() };
    let mut candidate = extract_seat_objects_view_with_stats(scene.view(), AVAILABLE, 12,
        &config, 0).unwrap().candidates.remove(0);
    candidate.id = format!("zone1:{}", candidate.id);
    let original_point = candidate.point;
    assert!(seat_object_candidate_supported(scene.view(), &candidate, AVAILABLE, 12, &config),
        "runtime zone scoping is not a change in image evidence");
    scene.paint(310, 175, INK);
    assert!(seat_object_candidate_supported(scene.view(), &candidate, AVAILABLE, 12, &config),
        "pixels outside the observation ROI are not used by this extractor");
    scene.paint(260, 150, INK);
    assert!(seat_object_candidate_supported(scene.view(), &candidate, AVAILABLE, 12, &config),
        "changed ROI is reinterpreted; an unrelated one-pixel mark does not cancel a valid zone-prefixed candidate");
    assert_eq!(candidate.point, original_point, "revalidation never replaces the queued input location");
}

fn only_input(actions: Vec<Action>) -> Action {
    let mut actions = inputs(actions);
    assert_eq!(actions.len(), 1, "{actions:?}");
    actions.remove(0)
}
fn acknowledge(engine: &mut Engine, action: &Action, now: u64) -> Vec<Action> {
    engine.step(Event::ActionAcknowledged { id: action.id, success: true, detail: String::new() }, now)
}
fn hit(scene: &Scene, action: &Action) -> usize {
    let ActionKind::Click { target: Target::Point { point }, purpose } = &action.kind else { panic!("seat input expected"); };
    assert!(purpose.starts_with("seat:"));
    let hits = scene.hits(*point);
    assert_eq!(hits.len(), 1);
    hits[0]
}

#[test]
fn retained_slanted_object_survives_alias_repaint_while_failed_neighbor_retries_after_refresh() {
    for verified in [false, true] {
        let scene = slanted_close_pair(false);
        let original = extract(&scene);
        let mut engine = engine(3, verified, true, true);
        let observed = observe(&mut engine, &original, 1);
        let first = only_input(if verified { count(&mut engine, 0, 2) } else { observed });
        assert_eq!(hit(&scene, &first), 8);
        let first_ack = acknowledge(&mut engine, &first, 3);
        let failed = only_input(if verified { count(&mut engine, 1, 4) } else { first_ack });
        assert_eq!(hit(&scene, &failed), 9);
        acknowledge(&mut engine, &failed, 5);
        // The independent simulated site accepted the first object and rejected
        // the second. No selected color or candidate ID supplies that outcome.
        let ActionKind::Click { purpose, .. } = &failed.kind else { unreachable!() };
        let escape = only_input(engine.step(Event::DeliveredSeatResult {
            action_id: failed.id, candidate_id: purpose.strip_prefix("seat:").unwrap().into(),
            result: SeatOutcome::Conflict }, 6));
        assert!(matches!(escape.kind, ActionKind::Key { key: Key::Escape, .. }));
        acknowledge(&mut engine, &escape, 7);
        engine.step(Event::ConflictDialogGone, 8);

        // A one-pixel map repaint changes both extracted IDs. The site objects
        // remain the same, and the successful object's old footprint is kept.
        let repaint = slanted_close_pair_shifted(false, 1.0);
        let aliases = extract(&repaint);
        assert_eq!(aliases.len(), 2);
        assert!(aliases.iter().all(|a| original.iter().all(|b| a.id != b.id)));
        assert!(inputs(observe(&mut engine, &aliases, 9)).is_empty());
        if verified { assert!(inputs(count(&mut engine, 1, 10)).is_empty()); }
        engine.step(Event::Pause, 11);
        engine.step(Event::Resume, 12);
        assert!(inputs(observe(&mut engine, &aliases, 13)).is_empty());
        if verified { assert!(inputs(count(&mut engine, 1, 14)).is_empty()); }
        assert_eq!(engine.snapshot().attempted_ids.len(), 1);

        engine.step(Event::RefreshCompleted { generation: 1 }, 15);
        let observed = observe(&mut engine, &aliases, 16);
        let retry = only_input(if verified { count(&mut engine, 1, 17) } else { observed });
        assert_eq!(hit(&repaint, &retry), 9, "only the failed neighbor is eligible again");
        assert_eq!(engine.snapshot().attempted_ids.len(), 1, "refresh preserves the successful attempt");
        engine.step(Event::ActionCancelled { id: retry.id }, 18);
        assert!(inputs(acknowledge(&mut engine, &retry, 19)).is_empty());
        assert_eq!(engine.snapshot().attempted_ids.len(), 1, "a cancelled retry cannot add a second count");
    }
}

#[test]
fn three_fragment_selected_styles_leave_no_middle_fragment_for_the_engine_to_click() {
    let shape = Shape::Rectangle { width: 23.0, height: 15.0 };
    let mut scene = Scene::new(480, 240);
    for col in 0..6 {
        scene.add(shape, Point { x: 35.5 + col as f64 * 70.0, y: 30.5 }, 0.0, false);
    }
    for col in 0..6 {
        let center = Point { x: 35.5 + col as f64 * 70.0, y: 95.5 };
        let id = scene.add(shape, center, 0.0, true);
        scene.objects[id].available = false;
        for cut in [-4_i64, 4] {
            for y in 88..=102 {
                for x in center.x as i64 + cut - 1..=center.x as i64 + cut + 1 {
                    scene.paint(x as usize, y, WHITE);
                }
            }
        }
    }
    scene.add(shape, Point { x: 65.5, y: 180.5 }, 0.0, true);
    scene.add(shape, Point { x: 135.5, y: 180.5 }, 0.0, true);
    let found = extract(&scene);
    let owners: Vec<_> = found.iter().map(|candidate| scene.hits(candidate.point)).collect();
    assert_eq!(owners, [vec![12], vec![13]],
        "every member of a competing whole-object cluster must be held, not only the outer pair");
    assert_eq!(run(scene, 2, false, false), [12, 13],
        "count OFF must not issue a click to a leftover selected fragment");
}
