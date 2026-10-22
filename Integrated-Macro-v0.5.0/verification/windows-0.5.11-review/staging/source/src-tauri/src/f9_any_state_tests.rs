//! Production ingress/context helpers and Engine. No Worker, OS or live page execution.
use super::*;

fn candidate() -> Candidate {
    let point = Point { x:20.0,y:30.0 };
    Candidate { id:"a".into(), point, color:Profile::default().color,
        target:Target::Point {point}, selected:false, geometry:None }
}

fn delivered(count: bool) -> (Engine, u64) {
    let mut profile = Profile::default();
    profile.target_seats = 2;
    profile.features.nol_selection_count = count;
    let mut engine = Engine::new(profile).unwrap();
    engine.step(Event::Start {mode:RunMode::Seats}, 0);
    let mut actions = engine.step(Event::CandidatesObserved {generation:0,candidates:vec![candidate()]}, 1);
    if count {
        let request = engine.selection_count_request().unwrap();
        actions = engine.step(Event::SelectionCountObserved {request_id:request.id,observation_id:1,count:0}, 2);
    }
    let id = actions.iter().find(|action| matches!(action.kind, ActionKind::Click {..})).unwrap().id;
    engine.step(Event::ActionAcknowledged {id,success:true,detail:String::new()}, 3);
    (engine,id)
}

#[test]
fn stopped_count_handoff_keeps_receipt_lineage_but_rotates_authorization() {
    let (mut engine, action) = delivered(true);
    let old_request = engine.selection_count_request().unwrap();
    engine.step(Event::Stop, 4);
    assert!(!f9_starts_new_logical_run(&engine, 7));
    let profile = engine.profile().clone();
    engine.restart_after_entry(profile, 5).unwrap();
    let request = engine.selection_count_request().unwrap();
    assert_ne!(request.id, old_request.id);
    assert_eq!(request.after_action_id, Some(action));
    let frame = OwnedFrame {width:2,height:2,stride:8,rgba:vec![255;16],
        origin:macro_platform::Point {x:0.0,y:0.0},scale_x:1.0,scale_y:1.0,
        sequence:1,captured_at_ns:110,last_observed_ns:120};
    assert!(selection_count_frame_fresh(&frame, 7, request.after_action_id, Some((7,action,100))));
    assert!(!selection_count_frame_fresh(&frame, 8, request.after_action_id, Some((7,action,100))));
    assert!(engine.step(Event::SelectionCountObserved {
        request_id:old_request.id,observation_id:99,count:2}, 6).is_empty());
    assert!(f9_starts_new_logical_run(&Engine::new(Profile::default()).unwrap(), 0));
    assert!(f9_starts_new_logical_run(&Engine::new(Profile::default()).unwrap(), 7));
}

#[test]
fn changed_binding_resets_only_the_previous_windows_selection_context() {
    let target = |id,pid| macro_platform::TargetWindow {id,pid,title:String::new(),
        rect:macro_platform::Rect {x:0.0,y:0.0,width:100.0,height:100.0}};
    let a = target(1,100);
    assert!(!binding_changes_context(Some(&a), &target(1,100)));
    assert!(binding_changes_context(Some(&a), &target(2,100)));
    assert!(binding_changes_context(Some(&a), &target(1,200)));
    for count in [false,true] {
        let (mut engine,_) = delivered(count);
        engine.step(Event::Stop,4);
        let profile = engine.profile().clone();
        reset_engine_after_rebind(&mut engine,&profile,false).unwrap();
        assert!(engine.has_retained_selection());
        reset_engine_after_rebind(&mut engine,&profile,true).unwrap();
        assert_eq!(engine.snapshot().phase,Phase::Idle);
        assert!(!engine.has_retained_selection());
        assert!(engine.snapshot().attempted_ids.is_empty());
        assert!(engine.selection_count_request().is_none());
    }
}

#[test]
fn recovery_after_explicit_handoff_resumes_search_and_never_old_entry_or_sale() {
    for region in [false,true] {
        let mut profile = Profile::default();
        profile.entry_target = Some(Target::Point {point:Point {x:1.0,y:2.0}});
        profile.legacy.date = Some(Target::Point {point:Point {x:3.0,y:4.0}});
        profile.features.server_clock = true;
        profile.sale_deadline_ms = Some(1_000_000);
        profile.features.region_priority = region;
        let mut engine = Engine::new(profile.clone()).unwrap();
        engine.step(Event::Start {mode:RunMode::Normal},0);
        let actions = engine.restart_after_entry(profile,1).unwrap();
        assert!(actions.iter().all(|action| matches!(action.kind,ActionKind::Observe {..})));
        let phase = if region {Phase::Region} else {Phase::Seats};
        assert_eq!(engine.snapshot().phase,phase);
        let reason = "[target_occluded] fixture occlusion";
        assert!(recoverable_context_error(reason));
        assert!(automatic_context_recovery(engine.continuous_search(),&engine.snapshot()));
        engine.step(Event::ContextLost {reason:reason.into()},2);
        assert_eq!(engine.confirmation_phase(),phase);
        for event in [Event::Resume,Event::Tick] {
            let actions = engine.step(event,2_000_000);
            assert!(actions.iter().all(|action| matches!(action.kind,ActionKind::Observe {..})));
        }
        assert_eq!(engine.snapshot().phase,phase);
    }
}

#[test]
fn legacy_conflict_retry_caller_rearms_unsent_escape_but_keeps_success_latched() {
    let mut profile = Profile::default();
    profile.features.conflict_recovery = true;
    profile.target_seats = 2;
    let mut engine = Engine::new(profile.clone()).unwrap();
    engine.step(Event::Start {mode:RunMode::Normal},0);
    assert!(!engine.continuous_search());
    let actions = engine.step(Event::CandidatesObserved {generation:0,candidates:vec![candidate()]},1);
    let seat_id = actions.iter().find(|a| matches!(a.kind,ActionKind::Click {..})).unwrap().id;
    engine.step(Event::ActionAcknowledged {id:seat_id,success:true,detail:String::new()},2);
    let actions = engine.step(Event::SeatResult {candidate_id:"a".into(),result:SeatOutcome::Conflict},3);
    let old_escape = actions.iter().find(|a| matches!(a.kind,ActionKind::Key {key:Key::Escape,..})).unwrap().id;
    engine.restart_after_entry(profile,4).unwrap(); // cancels the unsent Esc
    assert!(engine.continuous_search());
    assert!(engine.snapshot().attempted_ids.is_empty());
    let event = known_conflict_retry_event(&engine.snapshot(),false).unwrap();
    let actions = engine.step(event,5);
    let escape = actions.iter().find(|a| matches!(a.kind,ActionKind::Key {key:Key::Escape,..})).unwrap().id;
    assert!(escape > old_escape);
    assert!(known_conflict_retry_event(&engine.snapshot(),false).is_none());
    engine.step(Event::ActionAcknowledged {id:escape,success:true,detail:String::new()},6);
    let event = known_conflict_retry_event(&engine.snapshot(),false).unwrap();
    assert!(engine.step(event,7).iter().all(|a| matches!(a.kind,ActionKind::Observe {..})));
    engine.step(Event::ConflictDialogGone,8);
    assert!(known_conflict_retry_event(&engine.snapshot(),false).is_none());
}

#[test]
fn f8_capture_commit_rechecks_ownership_after_logging_or_publishing_delay() {
    for newer in [Command::ContinueAfterCaptcha,Command::Stop] {
        let (tx,rx) = mpsc::channel();
        let handle = RuntimeHandle {tx,interrupt:Arc::new(AtomicBool::new(false)),
            shortcut_epoch:Arc::new(AtomicU64::new(0))};
        handle.send(Command::CaptureArmed);
        let capture = rx.recv().unwrap();
        assert!(capture_commit_current(capture.shortcut_epoch,handle.shortcut_epoch.load(Ordering::SeqCst)));
        // The capture and local profile update finished, but publish/log I/O
        // still held the Worker. New ingress already owns the upcoming commit.
        handle.send(newer);
        rx.recv().unwrap();
        assert!(!capture_commit_current(capture.shortcut_epoch,handle.shortcut_epoch.load(Ordering::SeqCst)));
    }
}

fn conflict_sample(sequence: u64, popup: bool) -> macro_core::visual_dialog::VisualSample {
    let mut rgba = vec![200;64*48*4];
    for (i,pixel) in rgba.chunks_exact_mut(4).enumerate() {
        pixel[3] = 255;
        if popup && (20..44).contains(&(i%64)) && (16..32).contains(&(i/64)) {
            pixel[..3].fill(30);
        }
    }
    macro_core::visual_dialog::sample_frame(macro_core::vision::RgbaView {
        width:64,height:48,stride:256,rgba:&rgba},sequence).unwrap()
}

fn conflict_checkpoint(engine: &Engine) -> RestartContext {
    RestartContext {engine:engine.clone(),target_id:Some(5),geometry:None,
        run_id:8,started_at:Instant::now(),count_receipt:None}
}

fn only_input(actions: &[Action]) -> &Action {
    let inputs:Vec<_> = actions.iter().filter(|action| !matches!(action.kind,ActionKind::Observe {..})).collect();
    assert_eq!(inputs.len(),1,"{actions:?}");
    inputs[0]
}

#[test]
fn stop_and_f9_preserve_known_dialog_and_only_retry_undelivered_escape() {
    for visual in [false,true] { for delivery in ["queued","failed","delivered"] {
        let mut profile = Profile::default();
        profile.target_seats = 2;
        profile.features.conflict_recovery = true;
        let mut engine = Engine::new(profile.clone()).unwrap();
        engine.step(Event::Start {mode:if visual {RunMode::Seats} else {RunMode::Normal}},0);
        let actions = engine.step(Event::CandidatesObserved {generation:0,candidates:vec![candidate()]},1);
        let seat = only_input(&actions).clone();
        engine.step(Event::ActionAcknowledged {id:seat.id,success:true,detail:String::new()},2);
        let mut recovery = VisualRecovery::default();
        let event = if visual {
            let attempt = engine.snapshot().delivered_attempts[0].clone();
            recovery.arm(conflict_sample(1,false),attempt);
            match recovery.observe(conflict_sample(2,true)).unwrap().unwrap() {
                VisualSignal::Suspected {attempt,..} => Event::DeliveredSeatResult {
                    action_id:attempt.action_id,candidate_id:attempt.candidate_id,result:SeatOutcome::Conflict},
                _ => panic!("the synthetic popup must produce a real visual incident"),
            }
        } else { Event::SeatResult {candidate_id:"a".into(),result:SeatOutcome::Conflict} };
        let mut queue:VecDeque<_> = engine.step(event,3).into();
        let escape = only_input(queue.make_contiguous()).clone();
        if delivery != "queued" {
            queue.clear(); // native call has already consumed the action
            engine.step(Event::ActionAcknowledged {id:escape.id,success:delivery=="delivered",detail:String::new()},4);
        }
        // These are the production Stop helpers, in actual caller order. The
        // live Worker/adapter is not instantiated by this deterministic test.
        cancel_unposted_actions(&mut engine,&mut queue,5);
        let current = engine.has_unresolved_conflict().then(||conflict_checkpoint(&engine));
        let mut checkpoint = stopped_conflict_context(current,None,&mut recovery);
        engine.step(Event::Stop,5);
        assert_eq!(engine.snapshot().phase,Phase::Stopped);
        assert!(queue.is_empty());
        assert!(engine.step(Event::Tick,6).is_empty());
        assert!(engine.step(Event::ActionAcknowledged {id:escape.id,success:true,detail:String::new()},6).is_empty());
        // A repeated F4 has no active Conflict engine but keeps its known checkpoint.
        cancel_unposted_actions(&mut engine,&mut queue,7);
        checkpoint = stopped_conflict_context(None,checkpoint,&mut recovery);
        engine.step(Event::Stop,7);
        assert_eq!(recovery.is_incident(),visual);
        engine = checkpoint.unwrap().engine; // existing F9 supersede restoration
        let actions = engine.restart_after_entry(profile,8).unwrap();
        assert!(actions.iter().all(|a| matches!(a.kind,ActionKind::Observe {..})));
        assert_eq!(engine.snapshot().phase,Phase::Conflict,"{visual}/{delivery}");
        if visual { assert!(recovery.observe(conflict_sample(3,true)).unwrap().is_none()); }
        let retry = known_conflict_retry_event(&engine.snapshot(),recovery.is_incident()).unwrap();
        let actions = engine.step(retry,9);
        if delivery == "delivered" {
            assert!(actions.iter().all(|a| matches!(a.kind,ActionKind::Observe {..})));
        } else {
            let input = only_input(&actions);
            assert!(matches!(input.kind,ActionKind::Key {key:Key::Escape,..}));
            assert!(input.id>escape.id);
            engine.step(Event::ActionAcknowledged {id:input.id,success:true,detail:String::new()},10);
        }
        if visual { assert!(matches!(recovery.observe(conflict_sample(4,false)).unwrap(),Some(VisualSignal::Restored {..}))); }
        engine.step(Event::ConflictDialogGone,11);
        assert!(engine.snapshot().attempted_ids.is_empty());
        assert!(engine.snapshot().rejected_ids.contains(&"a".into()));
        let mut next = candidate();next.id="b".into();next.point.x=60.0;next.target=Target::Point {point:next.point};
        let actions = engine.step(Event::CandidatesObserved {generation:engine.snapshot().generation,candidates:vec![candidate(),next]},12);
        assert!(matches!(&only_input(&actions).kind,ActionKind::Click {purpose,..} if purpose=="seat:b"));
    }}
}

#[test]
fn ordinary_stop_discards_speculative_watch_and_restored_image_closes_retained_incident() {
    let (mut engine,_) = delivered(false);
    let attempt = engine.snapshot().delivered_attempts[0].clone();
    let mut recovery = VisualRecovery::default();
    recovery.arm(conflict_sample(1,false),attempt);
    assert!(stopped_conflict_context(None,None,&mut recovery).is_none());
    assert!(!recovery.is_armed());
    let attempt = engine.snapshot().delivered_attempts[0].clone();
    recovery.arm(conflict_sample(1,false),attempt);
    let Some(VisualSignal::Suspected {attempt,..}) = recovery.observe(conflict_sample(2,true)).unwrap() else {panic!()};
    let mut queue:VecDeque<_> = engine.step(Event::DeliveredSeatResult {
        action_id:attempt.action_id,candidate_id:attempt.candidate_id,result:SeatOutcome::Conflict},4).into();
    cancel_unposted_actions(&mut engine,&mut queue,5);
    let checkpoint = stopped_conflict_context(Some(conflict_checkpoint(&engine)),None,&mut recovery).unwrap();
    engine.step(Event::Stop,5);
    engine = checkpoint.engine;
    let profile = engine.profile().clone();
    engine.restart_after_entry(profile,6).unwrap();
    // observe_frame runs before the retry helper: a popup already closed while
    // stopped releases the incident without an unnecessary Esc.
    assert!(matches!(recovery.observe(conflict_sample(3,false)).unwrap(),Some(VisualSignal::Restored {..})));
    engine.step(Event::ConflictDialogGone,7);
    assert!(known_conflict_retry_event(&engine.snapshot(),recovery.is_incident()).is_none());
    assert_eq!(engine.snapshot().phase,Phase::Seats);
}

#[test]
fn same_window_rebind_restarts_visual_sequence_without_forgetting_incident_pixels() {
    let (mut engine,_) = delivered(false);
    let attempt = engine.snapshot().delivered_attempts[0].clone();
    let mut recovery = VisualRecovery::default();
    recovery.arm(conflict_sample(100,false),attempt);
    let Some(VisualSignal::Suspected {attempt,..}) = recovery.observe(conflict_sample(101,true)).unwrap() else {panic!()};
    engine.step(Event::DeliveredSeatResult {action_id:attempt.action_id,
        candidate_id:attempt.candidate_id,result:SeatOutcome::Conflict},4);
    let checkpoint = stopped_conflict_context(Some(conflict_checkpoint(&engine)),None,&mut recovery).unwrap();
    engine.step(Event::Stop,5);
    // The actual Bind caller invokes this only after adapter.bind succeeds
    // with the same ID/PID; a driver failure is not simulated by this test.
    recovery.same_target_stream_restarted();
    assert!(recovery.is_incident());
    assert!(recovery.observe(conflict_sample(1,true)).unwrap().is_none());
    assert_eq!(recovery.latest_report().unwrap().baseline_sequence,100);
    assert!(recovery.is_incident());
    engine = checkpoint.engine;
    let profile = engine.profile().clone();engine.restart_after_entry(profile,6).unwrap();
    let Some(VisualSignal::Restored {report}) = recovery.observe(conflict_sample(2,false)).unwrap() else {panic!()};
    assert_eq!(report.baseline_sequence,100,"original source provenance is retained");
    assert_eq!(report.sequence,2);
    engine.step(Event::ConflictDialogGone,7);
    assert_eq!(engine.snapshot().phase,Phase::Seats);
    assert!(!recovery.is_armed());
}
