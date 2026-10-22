//! Actual ingress/gate/profile helpers, plus Engine state. No OS/Worker execution.
use super::*;

fn handle() -> (RuntimeHandle, mpsc::Receiver<Request>) {
    let (tx, rx) = mpsc::channel();
    (RuntimeHandle { tx, interrupt: Arc::new(AtomicBool::new(false)),
        shortcut_epoch: Arc::new(AtomicU64::new(0)) }, rx)
}
fn starts() -> Vec<Command> {
    vec![Command::PrepareShortcut(RunMode::Normal), Command::PrepareShortcut(RunMode::Cancellation),
        Command::PrepareShortcut(RunMode::Seats), Command::ContinueAfterCaptcha,
        Command::StartFromUi { mode: RunMode::Seats, profile: json!({}), expected_profile_revision:0 }]
}

#[test]
fn every_execution_supersedes_every_older_execution_before_native_validation_returns() {
    for first in starts() { for second in starts() {
        let (handle, rx) = handle();
        handle.send(first.clone());
        let older = rx.recv().unwrap();
        let input_epoch = older.shortcut_epoch; // captured before blocking validation
        handle.send(second.clone());
        let newest = rx.recv().unwrap();
        let current = handle.shortcut_epoch.load(Ordering::SeqCst);
        assert_ne!(input_epoch, current);
        assert!(!control_request_current(&older, current));
        assert!(control_request_current(&newest, current));
    }}
}

#[test]
fn inverted_queue_order_does_not_let_old_controls_clear_the_new_start_gate() {
    for old in [Command::Stop, Command::Pause, Command::CaptureArmed,
        Command::ArmCapture { field:"color".into(), profile:None, expected_profile_revision:0 },
        Command::PrepareShortcut(RunMode::Normal)] {
        let (handle, rx) = handle();
        // Sender A is delayed between ingress ownership and queue insertion.
        let old_epoch = handle.receive_command(&old);
        handle.send(Command::PrepareShortcut(RunMode::Seats));
        let newer = rx.recv().unwrap();
        let mut gate = StartShortcutGate::default();
        let id = gate.prepare(RunMode::Seats, 0, newer.shortcut_epoch).unwrap();
        let delayed = Request { command:old, reply:None, shortcut_epoch:old_epoch, received_at:Instant::now() };
        handle.tx.send(delayed).unwrap();
        let old_request = rx.recv().unwrap();
        assert!(!control_request_current(&old_request, handle.shortcut_epoch.load(Ordering::SeqCst)));
        // handle() uses that rejection before gate/engine/watch mutation.
        assert!(gate.take(id, 1, newer.shortcut_epoch).is_some());
    }
}

#[test]
fn later_stop_or_capture_invalidates_prepared_payload_and_new_start_has_its_own_epoch() {
    for control in [Command::Stop, Command::Pause, Command::CaptureArmed] {
        let (handle, rx) = handle();
        handle.send(Command::PrepareShortcut(RunMode::Normal));
        let old = rx.recv().unwrap();
        let mut gate = StartShortcutGate::default();
        let id = gate.prepare(RunMode::Normal, 0, old.shortcut_epoch).unwrap();
        handle.send(control);
        rx.recv().unwrap();
        assert!(handle.interrupt.load(Ordering::SeqCst));
        assert!(gate.take(id, 1, handle.shortcut_epoch.load(Ordering::SeqCst)).is_none());
        handle.send(Command::PrepareShortcut(RunMode::Seats));
        let newer = rx.recv().unwrap();
        assert!(newer.shortcut_epoch > old.shortcut_epoch);
        let next_id = gate.prepare(RunMode::Seats, 2, newer.shortcut_epoch).unwrap();
        assert!(gate.take(id, 3, newer.shortcut_epoch).is_none());
        assert!(gate.take(next_id, 3, newer.shortcut_epoch).is_some());
    }
}

#[test]
fn repeated_identical_start_payload_is_idempotent_but_changed_stale_settings_stay_rejected() {
    let stored = json!({"target_seats":2,"vision":{"roi":{"x":10}}});
    assert!(!start_profile_save_needed(&stored, 0, &stored, 1).unwrap());
    assert!(start_profile_save_needed(&stored, 1, &stored, 1).unwrap());
    let changed = json!({"target_seats":3,"vision":{"roi":null}});
    assert!(start_profile_save_needed(&changed, 0, &stored, 1).is_err());
    assert!(start_profile_save_needed(&stored, 2, &stored, 1).is_err());
}

#[test]
fn retained_selection_mapping_requires_same_area_color_and_count_policy() {
    let mut profile = Profile::default();
    profile.vision.roi = Some(Rect { x:0.0, y:0.0, width:100.0, height:100.0 });
    let mut engine = Engine::new(profile.clone()).unwrap();
    engine.step(Event::Start { mode:RunMode::Seats }, 0);
    assert!(same_search_mapping(&engine, &profile, RunMode::Cancellation));
    let mut changed = profile.clone();
    changed.vision.roi.as_mut().unwrap().x = 1.0;
    assert!(!same_search_mapping(&engine, &changed, RunMode::Seats));
    changed = profile.clone(); changed.color.r = changed.color.r.wrapping_add(1);
    assert!(!same_search_mapping(&engine, &changed, RunMode::Seats));
    changed = profile; changed.features.nol_selection_count = true;
    assert!(!same_search_mapping(&engine, &changed, RunMode::Seats));
}

#[test]
fn failed_changed_settings_cannot_leak_into_f9_restored_selection_context() {
    let profile = Profile::default();
    let mut engine = Engine::new(profile.clone()).unwrap();
    engine.step(Event::Start { mode:RunMode::Seats }, 0);
    assert!(same_post_entry_mapping(&engine, &profile));
    for kind in ["color", "tolerance", "roi", "count", "objects"] {
        let mut changed = profile.clone();
        match kind {
            "color" => changed.color.r = changed.color.r.wrapping_add(1),
            "tolerance" => changed.color_tolerance += 1,
            "roi" => changed.vision.roi = Some(Rect { x:1.0,y:2.0,width:3.0,height:4.0 }),
            "count" => changed.features.nol_selection_count = true,
            "objects" => changed.features.seat_objects = true,
            _ => unreachable!(),
        }
        assert!(!same_post_entry_mapping(&engine, &changed), "{kind}");
    }
    let mut new_target = profile;
    new_target.target_seats += 1;
    assert!(same_post_entry_mapping(&engine, &new_target), "new target does not erase seat identity");
}

#[test]
fn f9_normal_handoff_keeps_the_region_mapping_for_a_later_f2() {
    let mut profile = Profile::default();
    profile.legacy.cancellation_navigation = macro_core::CancellationNavigation::ArrowKeys;
    profile.legacy.regions.push(macro_core::LegacyRegionSettings {
        color:Some(Color {r:10,g:20,b:30}),
        vision:Some(VisionConfig {roi:Some(Rect {x:10.0,y:20.0,width:50.0,height:40.0}), ..Default::default()}),
        ..Default::default()
    });
    let mut engine = Engine::new(profile.clone()).unwrap();
    let initial = engine.step(Event::Start {mode:RunMode::Cancellation}, 0);
    let navigation = initial.iter().find(|action| matches!(action.kind, ActionKind::Key {..})).unwrap();
    engine.step(Event::ActionAcknowledged {id:navigation.id,success:true,detail:String::new()}, 1);
    engine.step(Event::Tick, 2);
    let point = Point {x:20.0,y:30.0};
    let seat = Candidate {id:"zone0:one".into(),point,color:Color {r:10,g:20,b:30},
        target:Target::Point {point},selected:false,geometry:None};
    let actions = engine.step(Event::CandidatesObserved {generation:engine.snapshot().generation,candidates:vec![seat]}, 3);
    let click = actions.iter().find(|action| matches!(action.kind, ActionKind::Click {..})).unwrap();
    engine.step(Event::ActionAcknowledged {id:click.id,success:true,detail:String::new()}, 4);
    let current = engine.snapshot().active_region.unwrap();
    assert!(same_post_entry_mapping(&engine, &profile));
    engine.restart_after_entry(profile.clone(), 5).unwrap();
    assert_eq!(engine.snapshot().mode, RunMode::Normal);
    assert_eq!(engine.snapshot().active_region, Some(current));
    let mut snapshot_profile = profile.clone();
    snapshot_profile.legacy.regions[current].color = Some(Color {r:11,g:20,b:30});
    assert!(!same_search_mapping(&engine, &snapshot_profile, RunMode::Cancellation));
    assert!(same_search_mapping(&engine, &profile, RunMode::Cancellation));
}
