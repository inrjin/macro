use macro_core::*;
struct OwnedFrame { sequence:u64, captured_at_ns:u64 }
fn frame_passes_transition(sequence: u64, after: Option<u64>) -> bool {
    after.is_none_or(|previous| sequence > previous)
}
fn frame_passes_input_transition(frame: &OwnedFrame, after: Option<u64>, started_at_ns: Option<u64>) -> bool {
    frame_passes_transition(frame.sequence, after)
        && started_at_ns.is_none_or(|started| frame.captured_at_ns > started)
}
fn can_rescan_seat_frame(state: &macro_core::Snapshot, scope: &str, requested: bool) -> bool {
    requested && state.phase == Phase::Seats && scope == "seats"
        && state.current_candidate_id.is_none() && state.pending_action_id.is_none()
}
fn target(x:f64)->Target{Target::Point{point:Point{x,y:3.}}}
fn seat()->Candidate{Candidate{id:"prior-zone-seat".into(),point:Point{x:3.,y:3.},target:target(3.),color:Profile::default().color,selected:false}}
fn physical(actions:&[Action])->&Action{actions.iter().find(|a|!matches!(a.kind,ActionKind::Observe{..})).unwrap()}
#[test] fn legacy_timed_navigation_reuses_a_pre_input_frame_as_next_zone() {
 for nav in [CancellationNavigation::ArrowKeys,CancellationNavigation::TwoRegions{first:target(10.),second:target(20.)}] {
  let mut p=Profile::default();p.legacy.region_delay_ms=10;p.legacy.cancellation_navigation=nav;
  let mut e=Engine::new(p).unwrap();let actions=e.step(Event::Start{mode:RunMode::Cancellation},0);let input=physical(&actions);
  assert!(matches!(&input.kind,ActionKind::Click{purpose,..}|ActionKind::Key{purpose,..} if purpose=="legacy_cancellation_region"));
  // Production currently only establishes the region fence for continuous region: clicks.
  let old_sets_fence=e.continuous_search()&&matches!(&input.kind,ActionKind::Click{purpose,..} if purpose.starts_with("region:"));
  assert!(!old_sets_fence);
  e.step(Event::ActionAcknowledged{id:input.id,success:true,detail:String::new()},100);
  let resumed=e.step(Event::Tick,110); assert!(resumed.iter().any(|a|matches!(&a.kind,ActionKind::Observe{scope} if scope=="seats")));
  let state=e.snapshot();assert!(can_rescan_seat_frame(&state,"seats",true));
  let prior=OwnedFrame{sequence:12,captured_at_ns:99_000_000};
  assert!(frame_passes_input_transition(&prior,None,None));
  let clicks=e.step(Event::CandidatesObserved{generation:state.generation,candidates:vec![seat()]},111);
  assert!(matches!(&physical(&clicks).kind,ActionKind::Click{purpose,..} if purpose.starts_with("seat:")));
 }
}
#[test] fn strict_new_pixel_fence_alone_deadlocks_a_valid_static_original_map() {
 let mut p=Profile::default();p.legacy.region_delay_ms=10;p.legacy.cancellation_navigation=CancellationNavigation::ArrowKeys;
 let mut e=Engine::new(p).unwrap();let a=e.step(Event::Start{mode:RunMode::Cancellation},0);
 e.step(Event::ActionAcknowledged{id:physical(&a).id,success:true,detail:String::new()},100);e.step(Event::Tick,110);
 let unchanged=OwnedFrame{sequence:12,captured_at_ns:99_000_000};
 // Example: down-key or selecting the already-open zone changes no pixels.
 // No newer SCK image is promised, despite valid ongoing window observations.
 for time in [111,1_000,60_000] { assert!(!frame_passes_input_transition(&unchanged,Some(12),Some(100_000_000)));let a=e.step(Event::Tick,time);assert!(a.iter().all(|a|matches!(a.kind,ActionKind::Observe{..}))); }
 assert_eq!(e.snapshot().phase,Phase::Seats);
}
