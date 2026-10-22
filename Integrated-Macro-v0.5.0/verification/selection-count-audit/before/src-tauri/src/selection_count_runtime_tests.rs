//! Actual runtime frame/freshness helpers + real Engine integration.
//! Count values and successful native polls are test inputs. This does not
//! instantiate Worker, render the text reader, or send physical input.
use super::*;

fn frame() -> OwnedFrame {
    OwnedFrame { width:2,height:2,stride:8,rgba:vec![255;16],
        origin:macro_platform::Point{x:0.0,y:0.0},scale_x:2.0,scale_y:2.0,
        sequence:10,captured_at_ns:400,last_observed_ns:500 }
}
fn candidates() -> Vec<Candidate> {
    [20.0,40.0].into_iter().enumerate().map(|(i,x)| {
        let point=Point{x,y:20.0};Candidate{id:format!("seat-{i}"),point,target:Target::Point{point},
            color:Profile::default().color,selected:false,geometry:None}
    }).collect()
}
fn setup(target:usize,verify:bool,complete:bool)->Engine {
    let mut p=Profile::default();p.target_seats=target;p.features.nol_selection_count=verify;
    p.legacy.normal_click_delay_ms=100;
    if complete {p.legacy.complete=Some(Target::Point{point:Point{x:90.0,y:90.0}});}
    let mut e=Engine::new(p).unwrap();e.step(Event::Start{mode:RunMode::Seats},0);
    e
}
fn physical(actions:Vec<Action>)->Vec<Action> {
    actions.into_iter().filter(|a|!matches!(a.kind,ActionKind::Observe{..})).collect()
}
fn map(e:&mut Engine,now:u64)->Vec<Action> {
    let generation=e.snapshot().generation;
    e.step(Event::CandidatesObserved{generation,candidates:candidates()},now)
}
fn read(e:&mut Engine,f:&OwnedFrame,run:u64,receipt:Option<(u64,u64,u64)>,
    read_serial:u64,count:Option<usize>,now:u64)->Vec<Action> {
    let request=e.selection_count_request().expect("active count request");
    if !selection_count_frame_fresh(f,run,request.after_action_id,receipt) {return vec![];}
    let event=match count {
        Some(count)=>Event::SelectionCountObserved{request_id:request.id,observation_id:read_serial,count},
        None=>Event::SelectionCountUnavailable{request_id:request.id,observation_id:read_serial},
    };
    e.step(event,now)
}
fn first(e:&mut Engine)->Action {
    assert!(physical(map(e,1)).is_empty());
    let mut actions=physical(read(e,&frame(),7,None,1,Some(0),2));
    assert_eq!(actions.len(),1);let first=actions.remove(0);
    assert!(matches!(first.kind,ActionKind::Click{..}));
    assert!(physical(e.step(Event::ActionAcknowledged{id:first.id,success:true,detail:String::new()},3)).is_empty());
    first
}

#[test]
fn static_sck_pixels_and_timestamp_can_progress_after_delay_with_distinct_read_serials() {
    let mut engine=setup(2,true,true);let first=first(&mut engine);
    let receipt=Some((7,first.id,300));let mut native=Some(frame());
    let mut old_mapping=engine.clone();
    assert!(physical(read(&mut old_mapping,native.as_ref().unwrap(),7,receipt,500,Some(1),4)).is_empty());
    assert!(physical(read(&mut engine,native.as_ref().unwrap(),7,receipt,2,Some(1),4)).is_empty());
    for now in [104,204,10_004] {
        apply_frame_observation(&mut native,FrameObservation::Unchanged{
            sequence:10,captured_at_ns:400,last_observed_ns:500}).unwrap();
        assert!(physical(read(&mut old_mapping,native.as_ref().unwrap(),7,receipt,500,Some(1),now)).is_empty(),
            "negative control: raw SCK timestamp as read ID stalls permanently");
    }
    let f=native.as_ref().unwrap();
    let second=physical(read(&mut engine,f,7,receipt,3,Some(1),10_004));
    assert_eq!(second.len(),1);assert!(matches!(&second[0].kind,ActionKind::Click{purpose,..} if purpose=="seat:seat-1"));
    assert_eq!((f.sequence,f.captured_at_ns,f.last_observed_ns),(10,400,500));
    assert!(!frame_passes_input_transition(f,Some(10),Some(300)),"polling count is not evidence that a new map rendered");
}

#[test]
fn unknown_count_holds_completion_then_fresh_read_changes_phase_once() {
    let mut engine=setup(1,true,true);let first=first(&mut engine);let receipt=Some((7,first.id,300));
    let f=frame();
    assert!(physical(read(&mut engine,&f,7,receipt,2,Some(1),4)).is_empty());
    assert!(!engine.snapshot().selection_count_pending);
    assert!(physical(read(&mut engine,&f,7,receipt,3,None,104)).is_empty());
    assert!(engine.snapshot().selection_count_pending);
    assert!(physical(engine.step(Event::Tick,105)).is_empty());
    let request=engine.selection_count_request().unwrap();
    let complete=physical(read(&mut engine,&f,7,receipt,4,Some(1),106));
    assert_eq!(engine.snapshot().phase,Phase::Completing);
    assert_eq!(complete.len(),1);assert!(matches!(complete[0].kind,ActionKind::CompleteSelection{..}));
    assert!(engine.selection_count_request().is_none(),"queued completion suppresses another reader request");
    assert!(physical(engine.step(Event::SelectionCountUnavailable{request_id:request.id,observation_id:5},107)).is_empty());
    assert_eq!(engine.snapshot().pending_action_id,Some(complete[0].id));
    engine.step(Event::ActionAcknowledged{id:complete[0].id,success:true,detail:String::new()},108);
    assert_eq!(engine.snapshot().phase,Phase::Completed);
    assert!(engine.selection_count_request().is_none());assert!(!engine.snapshot().selection_count_pending);
}

#[test]
fn manual_completion_remains_observable_and_unknown_never_claims_completion() {
    let mut engine=setup(1,true,false);let first=first(&mut engine);let receipt=Some((7,first.id,300));
    assert!(physical(read(&mut engine,&frame(),7,receipt,2,Some(1),104)).is_empty());
    assert_eq!(engine.snapshot().phase,Phase::Completing);
    assert!(engine.selection_count_request().is_some(),"no completion target: keep observing the counter");
    read(&mut engine,&frame(),7,receipt,3,None,105);
    engine.step(Event::CompletionConfirmed,106);
    assert_eq!(engine.snapshot().phase,Phase::Completing);
    read(&mut engine,&frame(),7,receipt,4,Some(1),107);
    engine.step(Event::CompletionConfirmed,108);
    assert_eq!(engine.snapshot().phase,Phase::Completed);
}

#[test]
fn pause_and_stop_rotate_requests_while_native_receipts_remain_bound_to_the_run() {
    let mut engine=setup(2,true,true);let first=first(&mut engine);let receipt=Some((7,first.id,300));
    read(&mut engine,&frame(),7,receipt,2,Some(1),4);
    let old_request=engine.selection_count_request().unwrap();
    engine.step(Event::Pause,5);assert!(engine.selection_count_request().is_none());
    engine.step(Event::Resume,6);let resumed=engine.selection_count_request().unwrap();
    assert_ne!(old_request.id,resumed.id);
    assert!(physical(engine.step(Event::SelectionCountObserved{request_id:old_request.id,observation_id:50,count:2},104)).is_empty());
    assert!(physical(read(&mut engine,&frame(),8,receipt,3,Some(1),104)).is_empty(),"old run receipt cannot authorize input");
    assert!(physical(map(&mut engine,105)).is_empty());
    let second=physical(read(&mut engine,&frame(),7,receipt,4,Some(1),106));
    assert_eq!(second.len(),1);assert!(matches!(&second[0].kind,ActionKind::Click{purpose,..} if purpose=="seat:seat-1"));
    engine.step(Event::Stop,107);engine.step(Event::Start{mode:RunMode::Seats},108);
    let restarted=engine.selection_count_request().unwrap();assert_ne!(restarted.id,resumed.id);
    assert_eq!(restarted.after_action_id,None);
    assert!(physical(engine.step(Event::SelectionCountObserved{request_id:resumed.id,observation_id:60,count:2},109)).is_empty());
    assert_eq!(engine.snapshot().site_selection_count,None);
    assert!(physical(map(&mut engine,110)).is_empty());
    let new_first=physical(read(&mut engine,&frame(),8,None,5,Some(0),111));
    assert_eq!(new_first.len(),1);
    engine.step(Event::ActionAcknowledged{id:new_first[0].id,success:true,detail:String::new()},112);
    assert!(physical(read(&mut engine,&frame(),8,Some((7,new_first[0].id,300)),6,Some(1),213)).is_empty());
    assert!(engine.snapshot().selection_count_pending);
}

#[test]
fn count_option_off_preserves_the_original_timed_attempt_path() {
    for objects in [false,true] {
        let mut p=Profile::default();p.target_seats=2;p.features.seat_objects=objects;
        p.legacy.normal_click_delay_ms=100;
        let mut engine=Engine::new(p).unwrap();engine.step(Event::Start{mode:RunMode::Seats},0);
        assert!(engine.selection_count_request().is_none());
        let first=physical(map(&mut engine,1));assert_eq!(first.len(),1);
        assert!(physical(engine.step(Event::ActionAcknowledged{id:first[0].id,success:true,detail:String::new()},2)).is_empty());
        assert!(physical(engine.step(Event::SelectionCountUnavailable{request_id:1,observation_id:2},3)).is_empty());
        assert!(!engine.snapshot().selection_count_pending);
        let second=physical(engine.step(Event::Tick,102));assert_eq!(second.len(),1);
        assert!(matches!(&second[0].kind,ActionKind::Click{purpose,..} if purpose=="seat:seat-1"));
    }
}

// These crops are synthetic evidence pixels, not OCR fixtures. The guard
// invokes the real reader in the separate rendered-text regression below.
fn evidence_frame() -> OwnedFrame {
    OwnedFrame { width:64,height:32,stride:256,rgba:vec![255;64*32*4],
        origin:macro_platform::Point{x:0.0,y:0.0},scale_x:2.0,scale_y:2.0,
        sequence:10,captured_at_ns:400,last_observed_ns:500 }
}
fn pixel(frame:&mut OwnedFrame,x:usize,y:usize,rgb:[u8;3]) {
    let i=y*frame.stride+x*4;frame.rgba[i..i+3].copy_from_slice(&rgb);
    frame.sequence+=1;frame.captured_at_ns+=1;frame.last_observed_ns+=1;
}
fn proof(frame:&OwnedFrame,action:u64,count:usize)->SelectionCountInputProof {
    SelectionCountInputProof {run_id:7,action_id:action,request_id:10,observation_id:20,
        evidence:SelectionCountEvidence::capture(frame,count,
            Rect{x:10.0,y:6.0,width:12.0,height:5.0},Rect{x:8.0,y:4.0,width:30.0,height:12.0}).unwrap()}
}
fn same_read(p:&SelectionCountInputProof)->macro_core::selection_count::CountRead {
    macro_core::selection_count::CountRead::Confirmed {count:p.evidence.count,
        heading:p.evidence.heading,evidence:p.evidence.area}
}

#[test]
fn preinput_count_change_cancels_next_seat_without_delivery_or_completion() {
    let mut engine=setup(2,true,true);let first=first(&mut engine);
    let second=physical(read(&mut engine,&frame(),7,Some((7,first.id,300)),2,Some(1),104)).remove(0);
    let old=evidence_frame();let p=proof(&old,second.id,1);let mut latest=old.clone();
    pixel(&mut latest,30,8,[0,0,0]); // 1 -> 2 within the dynamic count band.
    assert!(!selection_count_input_allowed(Some(&p),Some(&latest),7,second.id));
    assert!(physical(engine.step(Event::ActionCancelled{id:second.id},105)).is_empty());
    assert!(engine.selection_count_request().is_some());
    assert!(physical(read(&mut engine,&frame(),7,Some((7,first.id,300)),3,Some(2),106)).is_empty());
    assert_eq!(engine.snapshot().phase,Phase::Seats);
    assert_eq!(engine.snapshot().pending_action_id,None);
    assert!(engine.snapshot().selection_count_pending);
}

#[test]
fn timer_only_change_rereads_once_but_global_unknown_blocks_same_numeric_crop() {
    let old=evidence_frame();let p=proof(&old,42,1);
    assert!(selection_count_input_allowed_with_reader(Some(&p),Some(&old),7,42,|_|panic!("identical frame needs no repeat OCR")));
    let mut latest=old.clone();pixel(&mut latest,60,25,[0,0,0]);
    assert!(p.evidence.matches(&latest),"change lies outside the local numeric evidence");
    let calls=std::cell::Cell::new(0);
    assert!(selection_count_input_allowed_with_reader(Some(&p),Some(&latest),7,42,|_|{
        calls.set(calls.get()+1);same_read(&p)
    }));
    assert_eq!(calls.get(),1,"timer animation triggers one re-read, not cancellation/re-capture looping");
    assert!(!selection_count_input_allowed_with_reader(Some(&p),Some(&latest),7,42,|_|macro_core::selection_count::CountRead::Unreadable),
        "new contradictory text or second heading cannot reuse an unchanged numeric crop");
}

#[test]
fn heading_move_zero_panel_change_and_complete_count_decrease_invalidate_proof() {
    let old=evidence_frame();let p=proof(&old,42,0);
    let mut moved=old.clone();pixel(&mut moved,12,8,[0,0,0]);
    assert!(!selection_count_input_allowed(Some(&p),Some(&moved),7,42));
    // An empty-state proof covers the supplied entire panel, not a zero glyph.
    let panel=SelectionCountInputProof {evidence:SelectionCountEvidence::capture(&old,0,p.evidence.heading,
        Rect{x:4.0,y:3.0,width:50.0,height:24.0}).unwrap(),..p.clone()};
    let mut changed_panel=old.clone();pixel(&mut changed_panel,50,22,[100,100,100]);
    assert!(!selection_count_input_allowed(Some(&panel),Some(&changed_panel),7,42));
    let mut engine=setup(1,true,true);let first=first(&mut engine);
    let completion=physical(read(&mut engine,&frame(),7,Some((7,first.id,300)),2,Some(1),104)).remove(0);
    assert!(matches!(completion.kind,ActionKind::CompleteSelection{..}));
    let complete_proof=proof(&old,completion.id,1);
    assert!(!selection_count_input_allowed(Some(&complete_proof),Some(&moved),7,completion.id));
    engine.step(Event::ActionCancelled{id:completion.id},105);
    assert!(physical(read(&mut engine,&frame(),7,Some((7,first.id,300)),3,Some(0),106)).is_empty());
    assert_eq!(engine.snapshot().phase,Phase::Completing);
    assert_eq!(engine.snapshot().pending_action_id,None);
    assert!(engine.snapshot().selection_count_pending);
}

#[test]
fn proof_is_action_and_run_scoped_and_rejects_incomplete_or_regressed_frames() {
    let old=evidence_frame();let p=proof(&old,42,1);
    assert!(!selection_count_input_allowed(None,Some(&old),7,42));
    assert!(!selection_count_input_allowed(Some(&p),Some(&old),8,42));
    assert!(!selection_count_input_allowed(Some(&p),Some(&old),7,43));
    let mut latest=old.clone();latest.origin.x=-100.0;
    assert!(!selection_count_input_allowed(Some(&p),Some(&latest),7,42));
    latest=old.clone();latest.sequence-=1;
    assert!(!selection_count_input_allowed(Some(&p),Some(&latest),7,42));
    latest=old.clone();latest.rgba[8*latest.stride+12*4+3]=0;
    assert!(!selection_count_input_allowed(Some(&p),Some(&latest),7,42));
    assert!(needs_selection_count_proof(&ActionKind::CompleteSelection{target:Target::Point{point:Point{x:1.0,y:1.0}}}));
    assert!(!needs_selection_count_proof(&ActionKind::Click{target:Target::Point{point:Point{x:1.0,y:1.0}},purpose:"region:a".into()}));
}

#[test]
fn cancellation_zero_proof_also_guards_refresh_and_both_legacy_navigation_inputs() {
    for variant in 0..3 {
        let mut profile=Profile::default();profile.features.nol_selection_count=true;profile.refresh_interval_ms=1;
        let target=Target::Point{point:Point{x:90.0,y:90.0}};
        match variant {
            0=>{profile.features.automatic_refresh=true;profile.refresh_target=Some(target.clone());},
            1=>profile.legacy.cancellation_navigation=macro_core::CancellationNavigation::TwoRegions{
                first:target.clone(),second:Target::Point{point:Point{x:120.0,y:90.0}}},
            _=>profile.legacy.cancellation_navigation=macro_core::CancellationNavigation::ArrowKeys,
        }
        let mut unverified_profile=profile.clone();unverified_profile.features.nol_selection_count=false;
        let mut unverified=Engine::new(unverified_profile).unwrap();
        let mut old_inputs=physical(unverified.step(Event::Start{mode:RunMode::Cancellation},0));
        assert!(unverified.selection_count_request().is_none());
        if variant==0 {
            let generation=unverified.snapshot().generation;
            old_inputs.extend(physical(unverified.step(Event::CandidatesObserved{generation,candidates:vec![]},1000)));
            old_inputs.extend(physical(unverified.step(Event::Tick,1001)));
        }
        assert_eq!(old_inputs.len(),1,"option OFF retains original navigation or refresh without an OCR grant");
        let mut engine=Engine::new(profile).unwrap();
        assert!(physical(engine.step(Event::Start{mode:RunMode::Cancellation},0)).is_empty());
        let generation=engine.snapshot().generation;
        assert!(physical(engine.step(Event::CandidatesObserved{generation,candidates:vec![]},1)).is_empty());
        let actions=physical(read(&mut engine,&frame(),7,None,1,Some(0),1000));
        assert_eq!(actions.len(),1,"variant {variant}");let action=&actions[0];
        assert!(needs_selection_count_proof(&action.kind),"{:#?}",action.kind);
        let base=evidence_frame();let p=proof(&base,action.id,0);
        assert!(selection_count_input_allowed(Some(&p),Some(&base),7,action.id));
        let mut selected=base.clone();pixel(&mut selected,30,8,[0,0,0]);
        assert!(!selection_count_input_allowed(Some(&p),Some(&selected),7,action.id));
        assert!(physical(engine.step(Event::ActionCancelled{id:action.id},1001)).is_empty());
        let phase=engine.snapshot().phase;
        assert!(physical(engine.step(Event::ActionAcknowledged{id:action.id,success:true,detail:String::new()},1001)).is_empty());
        assert_eq!(engine.snapshot().phase,phase,"a late ACK cannot resurrect a cancelled navigation");
        assert!(physical(engine.step(Event::Tick,1002)).is_empty());
        assert!(physical(read(&mut engine,&frame(),7,None,2,Some(1),1003)).is_empty(),
            "a selection that appeared after the zero read must not be cleared by stale navigation");
        assert_eq!(engine.snapshot().pending_action_id,None);
    }
}

fn rendered_count(name:&str)->OwnedFrame {
    let path=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../verification/seat-object-system/reader-review/fixtures")
        .join(format!("{name}.rgba"));
    let rgba=std::fs::read(path).expect("independent CoreText fixture; regenerate with reader-review/generate-fixtures.swift");
    assert_eq!(rgba.len(),640*300*4);
    OwnedFrame{width:640,height:300,stride:2560,rgba,origin:macro_platform::Point{x:0.0,y:0.0},
        scale_x:2.0,scale_y:2.0,sequence:10,captured_at_ns:400,last_observed_ns:500}
}
fn read_proof(f:&OwnedFrame,count:usize)->SelectionCountInputProof {
    let result=macro_core::selection_count::read_selection_count(macro_core::vision::RgbaView{
        width:f.width as u32,height:f.height as u32,stride:f.stride,rgba:&f.rgba});
    let macro_core::selection_count::CountRead::Confirmed{count:actual,heading,evidence}=result else {
        panic!("fixture must first establish readable count: {result:?}");
    };
    assert_eq!(actual,count);
    SelectionCountInputProof{run_id:7,action_id:42,request_id:10,observation_id:20,
        evidence:SelectionCountEvidence::capture(f,count,heading,evidence).unwrap()}
}

#[test]
fn rendered_reader_rechecks_global_contradictions_but_accepts_timer_only_changes() {
    let base=rendered_count("digit_2");let p=read_proof(&base,2);
    for name in ["contradictory_empty_and_two","count_two_second_heading_outside_band"] {
        let changed=rendered_count(name);
        assert!(p.evidence.matches(&changed),"{name}: negative control must retain identical count-band pixels");
        assert_ne!(selection_count_frame_digest(&base),selection_count_frame_digest(&changed));
        assert!(!selection_count_input_allowed(Some(&p),Some(&changed),7,42),"{name}: actual latest reader Unknown must revoke permission");
    }
    let timer=rendered_count("count_two_timer_9_42");let timer_proof=read_proof(&timer,2);
    let later=rendered_count("count_two_timer_9_41");
    assert_ne!(selection_count_frame_digest(&timer),selection_count_frame_digest(&later));
    assert!(selection_count_input_allowed(Some(&timer_proof),Some(&later),7,42),
        "actual reader rechecks an animated frame once, without rejecting unrelated timer changes");
    let one=rendered_count("digit_1");
    assert!(!selection_count_input_allowed(Some(&p),Some(&one),7,42),"readable decrease blocks completion");
    let one_proof=read_proof(&one,1);
    assert!(!selection_count_input_allowed(Some(&one_proof),Some(&base),7,42),"readable increment blocks the queued next seat");
    let mut unobserved=base.clone();unobserved.rgba[290*unobserved.stride+630*4+3]=0;
    assert!(p.evidence.matches(&unobserved),"missing data is outside the count band");
    assert!(!selection_count_input_allowed(Some(&p),Some(&unobserved),7,42),
        "actual reader cannot infer absence of contradictory labels from an unobserved pixel");
}

#[test]
fn pure_count_cache_preserves_fresh_poll_serials_and_rechecks_changed_pixels_or_layout() {
    let f=evidence_frame();let p=proof(&f,42,1);let calls=std::cell::Cell::new(0);let mut cache=None;
    let reader=|_:macro_core::vision::RgbaView<'_>| {calls.set(calls.get()+1);same_read(&p)};
    let mut engine=setup(2,true,true);let first=first(&mut engine);let receipt=Some((7,first.id,300));
    for (now,serial,expected_inputs) in [(4,2,0),(104,3,1)] {
        let (result,digest)=cached_selection_count_read(&f,&mut cache,&reader);
        assert_eq!(digest,selection_count_frame_digest(&f));
        let macro_core::selection_count::CountRead::Confirmed{count,..}=result else {panic!("cached known count");};
        assert_eq!(physical(read(&mut engine,&f,7,receipt,serial,Some(count),now)).len(),expected_inputs);
    }
    assert_eq!(calls.get(),1,"same pixels reuse only decoded text, while a later verified native poll grants a new read serial");
    let mut changed=f.clone();pixel(&mut changed,60,25,[0,0,0]);
    let (unknown,_)=cached_selection_count_read(&changed,&mut cache,|_|{
        calls.set(calls.get()+1);macro_core::selection_count::CountRead::Unreadable
    });
    assert_eq!(unknown,macro_core::selection_count::CountRead::Unreadable);assert_eq!(calls.get(),2);
    let (still_unknown,_)=cached_selection_count_read(&changed,&mut cache,|_|panic!("Unknown is cached as Unknown, not zero"));
    assert_eq!(still_unknown,macro_core::selection_count::CountRead::Unreadable);
    changed.width=32;changed.height=64;changed.stride=128;
    cached_selection_count_read(&changed,&mut cache,|_|{calls.set(calls.get()+1);macro_core::selection_count::CountRead::Unreadable});
    assert_eq!(calls.get(),3,"same byte vector with a different pixel layout must be re-read");
    assert!(!selection_count_input_allowed(None,Some(&f),7,42),"cached result never substitutes for an action proof");
}

#[test]
fn count_verified_legacy_manual_failure_tracks_delivery_not_queued_candidate() {
    let mut profile=Profile::default();profile.target_seats=2;profile.features.nol_selection_count=true;
    profile.features.conflict_recovery=true;profile.legacy.normal_click_delay_ms=100;
    let mut engine=Engine::new(profile).unwrap();engine.step(Event::Start{mode:RunMode::Normal},0);
    assert!(!engine.snapshot().optimistic_selection);assert!(engine.snapshot().selection_count_verification);
    assert!(physical(map(&mut engine,1)).is_empty());
    let first=physical(read(&mut engine,&frame(),7,None,1,Some(0),2)).remove(0);
    assert!(manual_confirmation_kind(&engine.snapshot(),Phase::Seats,"selection").is_none());
    assert!(conflict_confirmation_event(&engine.snapshot(),None).is_err(),"queued first click is not delivered");
    engine.step(Event::ActionAcknowledged{id:first.id,success:true,detail:String::new()},3);
    let after_ack=conflict_confirmation_event(&engine.snapshot(),None).unwrap();
    assert!(matches!(&after_ack,Event::DeliveredSeatResult{action_id,candidate_id,..} if *action_id==first.id&&candidate_id=="seat-0"));
    let second=physical(read(&mut engine,&frame(),7,Some((7,first.id,300)),2,Some(1),104)).remove(0);
    assert_eq!(engine.snapshot().current_candidate_id.as_deref(),Some("seat-1"));
    let queued=vec!["seat-1".into()];
    assert!(conflict_confirmation_event(&engine.snapshot(),Some(&queued)).is_err());
    let report=conflict_confirmation_event(&engine.snapshot(),None).unwrap();
    assert!(matches!(&report,Event::DeliveredSeatResult{action_id,candidate_id,..} if *action_id==first.id&&candidate_id=="seat-0"));
    let escape=physical(engine.step(report,105));assert_eq!(escape.len(),1);
    assert!(matches!(escape[0].kind,ActionKind::Key{key:Key::Escape,..}));
    assert_eq!(engine.snapshot().rejected_ids,vec!["seat-0"]);
    assert!(!engine.snapshot().rejected_ids.contains(&"seat-1".into()));
    engine.step(Event::ActionAcknowledged{id:second.id,success:true,detail:String::new()},106);
    assert_eq!(engine.snapshot().pending_action_id,Some(escape[0].id),"late ACK from cancelled next seat cannot replace Esc");
    engine.step(Event::ContextLost{reason:"settings cover the window".into()},107);
    let state=engine.snapshot();
    assert_eq!(context_confirmation_phase(&state,Phase::Conflict,true),Phase::Conflict);
    assert_eq!(context_confirmation_phase(&state,Phase::Conflict,false),Phase::Paused);
    assert_eq!(context_confirmation_phase(&state,Phase::Entry,true),Phase::Paused);
}

#[test]
fn count_flag_changes_only_seat_manual_policy_and_preserves_original_step_confirmations() {
    let mut profile=Profile::default();profile.features.nol_selection_count=true;
    let mut state=Engine::new(profile).unwrap().snapshot();
    for (phase,kind) in [(Phase::Entry,"seat_map"),(Phase::Captcha,"captcha_completed"),
        (Phase::Region,"region_map_ready"),(Phase::WaitingRegion,"region_map_ready"),
        (Phase::WaitingRefresh,"refresh_completed"),(Phase::Conflict,"dialog_gone")] {
        state.phase=phase;
        assert_eq!(manual_confirmation_kind(&state,phase,"selection").as_deref(),Some(kind));
    }
    state.phase=Phase::Seats;state.current_candidate_id=Some("pending".into());
    assert!(manual_confirmation_kind(&state,Phase::Seats,"selection").is_none());
    state.selection_count_verification=false;
    assert_eq!(manual_confirmation_kind(&state,Phase::Seats,"selection").as_deref(),Some("selection"));
    assert!(matches!(conflict_confirmation_event(&state,None).unwrap(),Event::SeatResult{candidate_id,..} if candidate_id=="pending"));
}

#[test]
fn count_verified_legacy_context_retry_is_limited_to_seat_result_phases() {
    let mut profile=Profile::default();profile.features.nol_selection_count=true;profile.target_seats=2;
    let mut engine=Engine::new(profile).unwrap();engine.step(Event::Start{mode:RunMode::Normal},0);
    assert!(!engine.continuous_search());
    assert!(automatic_context_recovery(engine.continuous_search(),&engine.snapshot()));
    map(&mut engine,1);let queued=physical(read(&mut engine,&frame(),7,None,1,Some(0),2)).remove(0);
    let reason="[target_not_foreground] covered before native delivery";
    let retry=automatic_context_recovery(engine.continuous_search(),&engine.snapshot())&&recoverable_context_error(reason);
    assert!(retry,"both real runtime call sites evaluate this before cancelling/pausing");
    engine.step(Event::ActionCancelled{id:queued.id},3);
    engine.step(Event::ContextLost{reason:reason.into()},3);
    assert_eq!(engine.snapshot().phase,Phase::Paused);
    assert_eq!(context_confirmation_phase(&engine.snapshot(),engine.confirmation_phase(),retry),Phase::Seats);
    assert!(engine.snapshot().attempted_ids.is_empty());
    engine.step(Event::Resume,4);
    assert_eq!(engine.snapshot().phase,Phase::Seats);
    assert!(engine.selection_count_request().is_some());
    assert!(physical(engine.step(Event::Tick,5)).is_empty(),"resume requires a new count/map grant");
    let mut state=engine.snapshot();
    for phase in [Phase::Seats,Phase::Conflict,Phase::Completing] {
        state.phase=phase;assert!(automatic_context_recovery(false,&state));
        state.selection_count_verification=false;assert!(!automatic_context_recovery(false,&state));
        state.selection_count_verification=true;
    }
    for phase in [Phase::Entry,Phase::Captcha,Phase::Region,Phase::WaitingRegion,Phase::WaitingRefresh,Phase::Paused,Phase::Completed] {
        state.phase=phase;assert!(!automatic_context_recovery(false,&state),"legacy {phase:?} keeps its old policy");
        assert!(automatic_context_recovery(true,&state),"original continuous policy remains unchanged");
    }
    assert!(!recoverable_context_error("[input_rejected] partial delivery unknown"),"unknown delivery still faults");
}

#[test]
#[ignore = "manual synthetic proof hashing cost; record build profile, not OS or end-to-end latency"]
fn count_evidence_hash_cost() {
    let mut f=evidence_frame();f.width=3024;f.height=1608;f.stride=f.width*4;f.rgba=vec![255;f.stride*f.height];
    let rounds=50;let start=Instant::now();
    for _ in 0..rounds {std::hint::black_box(proof(std::hint::black_box(&f),42,1));}
    let issue_us=start.elapsed().as_micros() as f64/rounds as f64;
    let p=proof(&f,42,1);let start=Instant::now();
    for _ in 0..rounds {assert!(selection_count_input_allowed_with_reader(Some(&p),Some(std::hint::black_box(&f)),7,42,|_|panic!("static frame")));}
    println!("{}",serde_json::json!({"width":f.width,"height":f.height,"rounds":rounds,
        "issueMeanUs":issue_us,"unchangedGuardMeanUs":start.elapsed().as_micros() as f64/rounds as f64,
        "scope":"synthetic full RGBA hash + small evidence crop only; excludes OCR, capture, native validation, UI and physical input"}));
}
