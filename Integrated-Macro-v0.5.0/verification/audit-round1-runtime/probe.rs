use macro_core::*;
fn frame_passes_transition(sequence: u64, after: Option<u64>) -> bool {
    after.is_none_or(|previous| sequence > previous)
}
fn target(x:f64,y:f64)->Target { Target::Point{point:Point{x,y}} }
fn candidate()->Candidate { Candidate{id:"pre-input-frame-seat".into(),point:Point{x:3.,y:3.},color:Profile::default().color,target:target(3.,3.),selected:false} }
fn physical(actions:&[Action])->Vec<&Action> { actions.iter().filter(|a|!matches!(a.kind,ActionKind::Observe{..})).collect() }
#[test] fn pre_region_click_unconsumed_sequence_passes_existing_fence() {
 let mut p=Profile::default(); p.features.nol_captcha_detection=true; p.features.region_priority=true;
 let mut e=Engine::new(p).unwrap(); e.step(Event::Start{mode:RunMode::Normal},0); e.step(Event::CaptchaManuallyCompleted,1);
 let region=e.step(Event::RegionsObserved{candidates:vec![RegionCandidate{candidate:candidate(),center_rank:0,distance_to_stage:1.}]},2);
 let action=physical(&region)[0]; e.step(Event::ActionAcknowledged{id:action.id,success:true,detail:String::new()},100);
 assert_eq!(e.snapshot().phase,Phase::Seats);
 let last_cached_sequence=12; let unread_sequence=13; let unread_capture_ns=99_000_000; let input_post_ns=100_000_000;
 assert!(unread_capture_ns<input_post_ns); assert!(frame_passes_transition(unread_sequence,Some(last_cached_sequence)));
 let generation=e.snapshot().generation; let after=e.step(Event::CandidatesObserved{generation,candidates:vec![candidate()]},101);
 assert!(physical(&after).iter().any(|a|matches!(&a.kind,ActionKind::Click{purpose,..} if purpose.starts_with("seat:"))));
 println!("REPRO region: cachedSeq=12 unreadSeq=13 capturedNs=99000000 postNs=100000000 old frame accepted -> seat action");
}
#[test] fn pre_refresh_click_unconsumed_sequence_passes_existing_fence() {
 let mut p=Profile::default(); p.features.automatic_refresh=true; p.refresh_target=Some(target(9.,9.)); p.refresh_interval_ms=100;
 let mut e=Engine::new(p).unwrap(); e.step(Event::Start{mode:RunMode::Cancellation},0);
 e.step(Event::CandidatesObserved{generation:0,candidates:vec![]},1);
 let refresh=e.step(Event::Tick,100); let action=physical(&refresh)[0]; assert!(matches!(action.kind,ActionKind::Refresh{..}));
 e.step(Event::ActionAcknowledged{id:action.id,success:true,detail:String::new()},101);
 assert!(frame_passes_transition(13,Some(12)));
 let after=e.step(Event::CandidatesObserved{generation:0,candidates:vec![candidate()]},102);
 assert!(physical(&after).iter().any(|a|matches!(&a.kind,ActionKind::Click{purpose,..} if purpose.starts_with("seat:"))));
 println!("REPRO refresh: sequence newer than app cache can have been captured before refresh -> seat action");
}
