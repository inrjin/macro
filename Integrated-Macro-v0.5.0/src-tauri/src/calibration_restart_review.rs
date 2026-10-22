//! Independent restart/calibration review using production helpers and Engine.
//! Save/Bind ordering is checked separately in the actual Worker source. These
//! tests do not run a Worker, capture a browser, or deliver OS/site input.
use super::*;

fn profile(count: bool) -> Profile {
    let mut p = Profile::default();
    p.target_seats = 2;
    p.features.nol_selection_count = count;
    p.features.conflict_recovery = true;
    p.vision.roi = Some(Rect { x:0.0, y:0.0, width:150.0, height:80.0 });
    p.legacy.complete = Some(Target::Point { point:Point { x:140.0, y:70.0 } });
    p
}
fn candidate(id: &str, x: f64, color: Color) -> Candidate {
    let point = Point { x, y:20.0 };
    Candidate { id:id.into(), point, target:Target::Point { point }, color, selected:false,
        geometry:Some(macro_core::CandidateGeometry {
            bounds:Rect { x:x-4.0, y:16.0, width:9.0, height:9.0 }, pixels:81,
            object_shape:None, object_context:None,
        }) }
}
fn no_input(actions: Vec<Action>) {
    assert!(actions.iter().all(|a| matches!(a.kind,ActionKind::Observe { .. })),"{actions:?}");
}
fn only_input(actions: Vec<Action>) -> Action {
    let mut inputs:Vec<_> = actions.into_iter().filter(|a| !matches!(a.kind,ActionKind::Observe { .. })).collect();
    assert_eq!(inputs.len(),1,"{inputs:?}"); inputs.remove(0)
}
fn seat(actions: Vec<Action>, id: &str) -> Action {
    let a=only_input(actions);
    assert!(matches!(&a.kind,ActionKind::Click { purpose,.. } if purpose==&format!("seat:{id}")),"{a:?}"); a
}
fn map(e: &mut Engine, candidates: Vec<Candidate>, now: u64) -> Vec<Action> {
    e.step(Event::CandidatesObserved { generation:e.snapshot().generation, candidates },now)
}
fn read(e: &mut Engine, count: usize, now: u64) -> Vec<Action> {
    let q=e.selection_count_request().unwrap();
    e.step(Event::SelectionCountObserved { request_id:q.id, observation_id:now+1, count },now)
}
fn ack(e: &mut Engine, a: &Action, now: u64) -> Vec<Action> {
    e.step(Event::ActionAcknowledged { id:a.id, success:true, detail:String::new() },now)
}
fn delivered(count: bool) -> (Engine,Action) {
    let p=profile(count); let mut e=Engine::new(p.clone()).unwrap();
    no_input(e.step(Event::Start { mode:RunMode::Seats },0));
    let actions=map(&mut e,vec![candidate("a",20.0,p.color)],1);
    let a=if count { no_input(actions); seat(read(&mut e,0,2),"a") } else { seat(actions,"a") };
    no_input(ack(&mut e,&a,3)); assert!(e.has_retained_selection()); (e,a)
}
fn context(e: &Engine, receipt: Option<(u64,u64,u64)>) -> RestartContext {
    RestartContext { engine:e.clone(), target_id:Some(5),
        geometry:Some(FrameReference { width:150, height:80, scale_x:1.0, scale_y:1.0 }),
        run_id:8, started_at:Instant::now(), count_receipt:receipt }
}
fn recalibrated(e: &Engine, exclude_a: bool) -> Profile {
    let mut next=e.profile().clone();
    next.vision.roi=Some(Rect { x:if exclude_a { 60.0 } else { 0.0 }, y:5.0,
        width:if exclude_a { 90.0 } else { 150.0 }, height:60.0 });
    next.vision.exclusions=vec![Rect { x:100.0,y:50.0,width:10.0,height:10.0 }];
    next.vision.min_component_pixels=4;
    next.vision.max_component_pixels=99_999;
    next.color=Color { r:20,g:150,b:80 };
    next.color_tolerance=13;
    next
}
fn frame() -> OwnedFrame {
    OwnedFrame { width:150,height:80,stride:600,rgba:vec![255;150*80*4],
        origin:macro_platform::Point {x:0.0,y:0.0},scale_x:1.0,scale_y:1.0,
        sequence:10,captured_at_ns:2000,last_observed_ns:2000 }
}

#[test]
fn calibration_changes_are_not_a_new_window_or_selection_policy() {
    let (e,_) = delivered(false);
    for kind in ["roi","exclusion","minimum","maximum","color","tolerance"] {
        let mut next=e.profile().clone();
        match kind {
            "roi" => next.vision.roi.as_mut().unwrap().x=1.0,
            "exclusion" => next.vision.exclusions.push(Rect { x:100.0,y:50.0,width:10.0,height:10.0 }),
            "minimum" => next.vision.min_component_pixels+=1,
            "maximum" => next.vision.max_component_pixels-=1,
            "color" => next.color.g=next.color.g.wrapping_add(1),
            "tolerance" => next.color_tolerance+=1,
            _ => unreachable!(),
        }
        for mode in [RunMode::Seats,RunMode::Cancellation] {
            assert!(same_search_mapping(&e,&next,mode),"{kind}/{mode:?}: extraction settings must not erase delivery history");
        }
        assert!(same_post_entry_mapping(&e,&next),"F9/{kind}");
    }
}

#[test]
fn saved_idle_then_stop_keeps_old_delivery_context_and_resets_speculative_watch() {
    let (old,a)=delivered(true);
    let receipt=(8,a.id,1000);
    let mut saved=old.profile().clone(); saved.vision.roi.as_mut().unwrap().x=30.0;
    let mut live=Engine::new(saved.clone()).unwrap(); // actual successful Save branch
    let mut retained=Some(context(&old,Some(receipt)));
    let rgba=vec![255;64*48*4];
    let sample=macro_core::visual_dialog::sample_frame(macro_core::vision::RgbaView {
        width:64,height:48,stride:256,rgba:&rgba },1).unwrap();
    let mut visual=VisualRecovery::default();
    assert!(visual.arm(sample,old.snapshot().delivered_attempts[0].clone()));
    for time in [5,6] {
        // There is no current conflict: the previous failed-start checkpoint
        // is the only source of the real delivery/count obligation after Save.
        let current=live.has_unresolved_conflict().then(||context(&live,None));
        retained=stopped_conflict_context(current,retained,&mut visual);
        no_input(live.step(Event::Stop,time));
        assert!(retained.is_some(),"F4 must not discard previously delivered A after Save made the live Engine idle");
        assert!(!visual.is_armed(),"keeping seat history does not keep a speculative popup watch");
    }
    let retained=retained.unwrap();
    assert_eq!(retained.count_receipt,Some(receipt));
    assert_eq!(retained.engine.snapshot().attempted_ids,vec!["a"]);
    for f9 in [false,true] {
        let mut restored=retained.engine.clone();
        let actions=if f9 { restored.restart_after_entry(saved.clone(),10).unwrap() }
            else { restored.restart_search(saved.clone(),RunMode::Seats,true,10).unwrap() };
        no_input(actions);
        assert_eq!(restored.snapshot().attempted_ids,vec!["a"]);
        assert_eq!(restored.selection_count_request().unwrap().after_action_id,Some(a.id));
        assert_eq!(retained.count_receipt,Some(receipt));
    }
}

#[test]
fn recalibration_keeps_original_footprint_and_count_even_when_a_leaves_the_new_roi() {
    for exclude_a in [false,true] { for mode in [RunMode::Seats,RunMode::Cancellation,RunMode::Normal] {
        let (mut e,a)=delivered(false);let old=e.snapshot();let next=recalibrated(&e,exclude_a);
        assert_eq!(restart_block_reason(&context(&e,None),Some(5),Some(&frame()),&next,mode),None);
        let actions=if mode==RunMode::Normal { e.restart_after_entry(next.clone(),10).unwrap() }
            else { e.restart_search(next.clone(),mode,true,10).unwrap() };
        no_input(actions);no_input(e.step(Event::Tick,11));no_input(ack(&mut e,&a,12));
        no_input(e.step(Event::CandidatesObserved {generation:old.generation,
            candidates:vec![candidate("stale-b",80.0,next.color)]},13));
        assert_eq!(e.snapshot().attempted_ids,vec!["a"]);
        let b=seat(map(&mut e,vec![candidate("new-a-alias",21.0,next.color),candidate("b",80.0,next.color)],14),"b");
        assert!(b.id>a.id);
        assert_eq!(e.snapshot().attempted_ids,vec!["a"]);
        let complete=only_input(ack(&mut e,&b,15));
        assert!(matches!(complete.kind,ActionKind::CompleteSelection { .. }));
        assert_eq!(e.snapshot().attempted_ids,vec!["a","b"]);
        no_input(ack(&mut e,&a,16));no_input(ack(&mut e,&complete,17));
        assert_eq!(e.snapshot().phase,Phase::Completed);
        no_input(e.step(Event::Tick,100));
    }}
}

#[test]
fn recalibration_keeps_the_pending_count_effect_but_requires_a_new_observation() {
    for mode in [RunMode::Seats,RunMode::Cancellation,RunMode::Normal] {
        let (mut e,a)=delivered(true);let old=e.selection_count_request().unwrap();
        let next=recalibrated(&e,true);
        let actions=if mode==RunMode::Normal { e.restart_after_entry(next.clone(),10).unwrap() }
            else { e.restart_search(next.clone(),mode,true,10).unwrap() };
        no_input(actions);let new=e.selection_count_request().unwrap();
        assert_ne!(new.id,old.id);assert_eq!(new.after_action_id,Some(a.id));
        assert_eq!(e.snapshot().delivered_attempts.len(),1);
        no_input(e.step(Event::SelectionCountObserved {request_id:old.id,observation_id:100,count:2},11));
        no_input(ack(&mut e,&a,12));
        no_input(map(&mut e,vec![candidate("b",80.0,next.color)],13));
        no_input(e.step(Event::SelectionCountUnavailable {request_id:new.id,observation_id:14},14));
        no_input(read(&mut e,0,15));no_input(e.step(Event::Tick,16));
        assert_eq!(e.snapshot().attempted_ids,vec!["a"]);
        // The independent site total finally reflects the already delivered A.
        let b=seat(read(&mut e,1,17),"b");no_input(ack(&mut e,&b,18));
        no_input(read(&mut e,1,19));
        assert!(matches!(only_input(read(&mut e,2,20)).kind,ActionKind::CompleteSelection { .. }));
    }
}

#[test]
fn failed_seat_alias_stays_excluded_after_new_color_and_bounds_are_saved() {
    let mut p=profile(false);p.target_seats=3;
    let mut e=Engine::new(p.clone()).unwrap();no_input(e.step(Event::Start {mode:RunMode::Seats},0));
    let a=seat(map(&mut e,vec![candidate("a",20.0,p.color)],1),"a");no_input(ack(&mut e,&a,2));
    let b=seat(map(&mut e,vec![candidate("b",50.0,p.color)],3),"b");no_input(ack(&mut e,&b,4));
    let escape=only_input(e.step(Event::DeliveredSeatResult {action_id:b.id,candidate_id:"b".into(),result:SeatOutcome::Conflict},5));
    no_input(ack(&mut e,&escape,6));no_input(e.step(Event::ConflictDialogGone,7));
    assert_eq!(e.snapshot().rejected_ids,vec!["b"]);
    let next=recalibrated(&e,false);
    no_input(e.restart_search(next.clone(),RunMode::Seats,true,10).unwrap());
    assert_eq!(e.snapshot().rejected_ids,vec!["b"]);
    assert_eq!(e.snapshot().attempted_ids,vec!["a"]);
    let c=seat(map(&mut e,vec![candidate("new-a",21.0,next.color),candidate("new-b",51.0,next.color),
        candidate("c",80.0,next.color)],11),"c");
    no_input(ack(&mut e,&b,12));assert_eq!(e.snapshot().pending_action_id,Some(c.id));
    no_input(ack(&mut e,&c,13));assert_eq!(e.snapshot().attempted_ids,vec!["a","c"]);
}

#[test]
fn unrelated_window_geometry_and_selection_policy_fail_for_distinct_reasons() {
    let (e,_)=delivered(false);let mut ctx=context(&e,None);let next=recalibrated(&e,false);let mut image=frame();
    assert_eq!(restart_block_reason(&ctx,Some(5),Some(&image),&next,RunMode::Seats),None);
    for target in [None,Some(6)] {
        assert_eq!(restart_block_reason(&ctx,target,Some(&image),&next,RunMode::Seats),Some(RestartBlockReason::WindowChanged));
    }
    assert_eq!(restart_block_reason(&ctx,Some(5),None,&next,RunMode::Seats),Some(RestartBlockReason::GeometryUnavailable));
    let old_geometry=ctx.geometry.take();
    assert_eq!(restart_block_reason(&ctx,Some(5),Some(&image),&next,RunMode::Seats),Some(RestartBlockReason::GeometryUnavailable));
    ctx.geometry=old_geometry;
    for geometry in ["width","height","scale_x","scale_y"] {
        image=frame();match geometry { "width"=>image.width+=1,"height"=>image.height+=1,
            "scale_x"=>image.scale_x=2.0,"scale_y"=>image.scale_y=2.0,_=>unreachable!() }
        assert_eq!(restart_block_reason(&ctx,Some(5),Some(&image),&next,RunMode::Seats),Some(RestartBlockReason::GeometryChanged));
    }
    image=frame();
    for count in [false,true] {
        let mut changed=next.clone();
        if count {changed.features.nol_selection_count=true;} else {changed.features.seat_objects=true;}
        assert_eq!(restart_block_reason(&ctx,Some(5),Some(&image),&changed,RunMode::Seats),
            Some(if count {RestartBlockReason::SelectionCountPolicyChanged} else {RestartBlockReason::ObjectPolicyChanged}));
        assert!(!same_post_entry_mapping(&e,&changed));
    }
}

#[test]
fn known_region_navigation_changes_stay_guarded_while_recalibration_is_allowed() {
    let mut p=profile(false);p.legacy.cancellation_navigation=macro_core::CancellationNavigation::ArrowKeys;
    p.legacy.region_delay_ms=1;
    let mut e=Engine::new(p.clone()).unwrap();
    let nav=only_input(e.step(Event::Start {mode:RunMode::Cancellation},0));
    no_input(ack(&mut e,&nav,1));no_input(e.step(Event::Tick,2));
    no_input(e.step(Event::RefreshCompleted { generation:e.snapshot().generation+1 },2));
    assert_eq!(e.snapshot().phase,Phase::Seats);
    let a=seat(map(&mut e,vec![candidate("zone0:a",20.0,p.color)],3),"zone0:a");no_input(ack(&mut e,&a,4));
    assert_eq!(e.snapshot().active_region,Some(0));
    let next=recalibrated(&e,false);let ctx=context(&e,None);let image=frame();
    assert_eq!(restart_block_reason(&ctx,Some(5),Some(&image),&next,RunMode::Cancellation),None);
    let mut changed=next.clone();changed.legacy.cancellation_navigation=macro_core::CancellationNavigation::None;
    assert_eq!(restart_block_reason(&ctx,Some(5),Some(&image),&changed,RunMode::Cancellation),Some(RestartBlockReason::NavigationChanged));
    changed.legacy.cancellation_navigation=macro_core::CancellationNavigation::TwoRegions {
        first:Target::Point {point:Point{x:10.0,y:10.0}},second:Target::Point {point:Point{x:100.0,y:10.0}} };
    assert_eq!(restart_block_reason(&ctx,Some(5),Some(&image),&changed,RunMode::Cancellation),Some(RestartBlockReason::NavigationChanged));
}

#[test]
fn cancelled_unposted_seat_is_not_promoted_to_retained_history_after_save_and_stop() {
    let p=profile(false);let mut e=Engine::new(p.clone()).unwrap();
    no_input(e.step(Event::Start {mode:RunMode::Seats},0));
    let a=seat(map(&mut e,vec![candidate("a",20.0,p.color)],1),"a");
    let mut queue=VecDeque::from([a.clone()]);cancel_unposted_actions(&mut e,&mut queue,2);
    assert!(!e.has_retained_selection());
    let mut visual=VisualRecovery::default();
    assert!(stopped_conflict_context(Some(context(&e,None)),None,&mut visual).is_none());
    no_input(e.step(Event::Stop,3));
    let next=recalibrated(&e,false);no_input(e.restart_after_entry(next.clone(),4).unwrap());
    no_input(ack(&mut e,&a,5));
    let retry=seat(map(&mut e,vec![candidate("a",20.0,next.color)],6),"a");assert!(retry.id>a.id);
}

#[test]
fn failed_f9_retries_cannot_relabel_retained_seats_with_a_new_capture_geometry() {
    let (mut live,a)=delivered(true);
    let original=context(&live,Some((8,a.id,1000)));
    let next=recalibrated(&live,true);
    let mut changed=frame(); changed.width=160; changed.stride=640;
    changed.rgba=vec![255;160*80*4];
    assert_eq!(restart_block_reason(&original,Some(5),Some(&changed),&next,RunMode::Seats),
        Some(RestartBlockReason::GeometryChanged));

    let mut checkpoint=original.clone();
    let mut prior_request_id=live.selection_count_request().unwrap().id;
    for attempt in 0..3 {
        let now=10+attempt*10;
        no_input(live.restart_after_entry(next.clone(),now).unwrap());
        assert_ne!(live.selection_count_request().unwrap().id,prior_request_id);
        prior_request_id=live.selection_count_request().unwrap().id;
        assert_eq!(live.selection_count_request().unwrap().after_action_id,Some(a.id));
        // F9 commits its post-entry Engine before validating the captured frame.
        // Validation failure must retain the old physical checkpoint. The next
        // supersede/capture uses this same production provenance helper even if
        // frame=None would otherwise fall back to a newly saved desktop reference.
        assert_eq!(physical_restart_block_reason(&checkpoint,Some(5),Some(&changed)),
            Some(RestartBlockReason::GeometryChanged));
        no_input(live.step(Event::ContextLost { reason:"changed capture geometry".into() },now+1));
        assert_eq!(live.snapshot().phase,Phase::Paused);
        let mut recaptured=context(&live,Some((8,a.id,1001+attempt)));
        recaptured.geometry=Some(FrameReference {width:160,height:80,scale_x:1.0,scale_y:1.0});
        recaptured.target_id=Some(6); // a newly read target cannot re-own old marks either
        checkpoint=preserve_restart_provenance(recaptured,Some(&checkpoint));
        assert_eq!(checkpoint.target_id,Some(5));
        assert_eq!(checkpoint.geometry.as_ref().unwrap().width,150);
        assert_eq!(checkpoint.engine.snapshot().phase,Phase::Paused);
        assert_eq!(checkpoint.count_receipt,Some((8,a.id,1001+attempt)),"keep current receipt, not stale snapshot data");
        assert_eq!(physical_restart_block_reason(&checkpoint,Some(5),Some(&changed)),
            Some(RestartBlockReason::GeometryChanged));
        assert_eq!(physical_restart_block_reason(&checkpoint,Some(6),Some(&changed)),
            Some(RestartBlockReason::WindowChanged));
        no_input(live.step(Event::Tick,now+2));
        no_input(ack(&mut live,&a,now+3));
        assert_eq!(live.snapshot().attempted_ids,vec!["a"]);
    }

    // Unknown original geometry must stay unknown, rather than be upgraded by
    // a later settings capture that did not establish the old seat coordinates.
    let mut unknown=checkpoint.clone(); unknown.geometry=None;
    let still_unknown=preserve_restart_provenance(checkpoint.clone(),Some(&unknown));
    assert!(still_unknown.geometry.is_none());
    assert_eq!(physical_restart_block_reason(&still_unknown,Some(5),Some(&frame())),
        Some(RestartBlockReason::GeometryUnavailable));

    // Restoring the actual original geometry permits recovery, but the pending
    // site-count effect still needs a new observation before another seat.
    assert_eq!(physical_restart_block_reason(&checkpoint,Some(5),Some(&frame())),None);
    no_input(live.step(Event::Resume,50));
    no_input(map(&mut live,vec![candidate("b",80.0,next.color)],51));
    no_input(read(&mut live,0,52));
    let b=seat(read(&mut live,1,53),"b");
    assert!(b.id>a.id);
    assert_eq!(live.snapshot().attempted_ids,vec!["a"]);

    // A fresh Engine with no delivered history, or a deliberately new logical
    // run, does not inherit the previous run's physical capture provenance.
    let mut fresh=context(&Engine::new(next.clone()).unwrap(),None);
    fresh.geometry=Some(FrameReference {width:160,height:80,scale_x:1.0,scale_y:1.0});
    let fresh=preserve_restart_provenance(fresh,Some(&original));
    assert_eq!(physical_restart_block_reason(&fresh,Some(5),Some(&changed)),None);
    let mut new_run=checkpoint.clone(); new_run.run_id+=1;
    new_run.geometry=Some(FrameReference {width:160,height:80,scale_x:1.0,scale_y:1.0});
    let new_run=preserve_restart_provenance(new_run,Some(&original));
    assert_eq!(physical_restart_block_reason(&new_run,Some(5),Some(&changed)),None);
}
