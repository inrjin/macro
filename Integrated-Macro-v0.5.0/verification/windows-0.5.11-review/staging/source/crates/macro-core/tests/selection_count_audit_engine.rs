//! Engine-only count audit. A separate site ledger owns actual selections;
//! ACKs represent delivery, never a successful selection. No OCR/OS is used.
use macro_core::*;
use std::collections::BTreeSet;

fn target(x: f64) -> Target { Target::Point { point: Point { x, y: 20.0 } } }
fn profile(count: bool, wanted: usize) -> Profile {
    let mut p = Profile::default();
    p.features.nol_selection_count = count;
    p.features.conflict_recovery = true;
    p.target_seats = wanted;
    p.legacy.complete = Some(target(900.0));
    p.refresh_interval_ms = 25;
    p.legacy.region_delay_ms = 1;
    p
}
fn candidates(e: &mut Engine, ids: &[&str], time: u64) -> Vec<Action> {
    let color = e.effective_color();
    e.step(Event::CandidatesObserved { generation: e.snapshot().generation,
        candidates: ids.iter().enumerate().map(|(i,id)| {
            let point = Point {x: 10.0 + i as f64 * 20.0, y: 20.0};
            Candidate {id:(*id).into(), point, target:Target::Point {point},color,
                selected:false,geometry:None}
        }).collect() }, time)
}
fn physical(actions: Vec<Action>) -> Vec<Action> {
    actions.into_iter().filter(|a|!matches!(a.kind,ActionKind::Observe {..})).collect()
}
fn one(actions: Vec<Action>) -> Action {
    let mut inputs=physical(actions);assert_eq!(inputs.len(),1,"{inputs:?}");inputs.remove(0)
}
fn none(actions: Vec<Action>) {assert!(physical(actions).is_empty());}
fn seat(actions: Vec<Action>, expected: &str) -> Action {
    let a=one(actions);assert!(matches!(&a.kind,ActionKind::Click {purpose,..} if purpose==&format!("seat:{expected}")),"{a:?}");a
}
fn ack(e:&mut Engine,a:&Action,time:u64)->Vec<Action>{e.step(Event::ActionAcknowledged{id:a.id,success:true,detail:String::new()},time)}
fn conflict(e:&mut Engine,a:&Action,id:&str,time:u64)->Vec<Action>{e.step(Event::DeliveredSeatResult{action_id:a.id,candidate_id:id.into(),result:SeatOutcome::Conflict},time)}
fn read(e:&mut Engine,count:usize,time:u64)->Vec<Action>{
    let q=e.selection_count_request().expect("active request");
    e.step(Event::SelectionCountObserved{request_id:q.id,observation_id:time+1,count},time)
}
fn unknown(e:&mut Engine,time:u64)->Vec<Action>{
    let q=e.selection_count_request().expect("active request");
    e.step(Event::SelectionCountUnavailable{request_id:q.id,observation_id:time+1},time)
}
fn start(p:Profile,mode:RunMode)->Engine{let mut e=Engine::new(p).unwrap();none(e.step(Event::Start{mode},0));e}
#[derive(Default)]
struct Site {selected:BTreeSet<&'static str>, delivered:Vec<&'static str>}
impl Site {
    fn accept(&mut self,id:&'static str){self.delivered.push(id);assert!(self.selected.insert(id),"supported scenario must not toggle a retained seat");}
    fn count(&self)->usize{self.selected.len()}
}

#[test]
fn revoked_next_click_never_spends_a_selection_or_accepts_old_ack_in_any_run_mode(){
    for mode in [RunMode::Normal,RunMode::Seats,RunMode::Cancellation]{
        let mut e=start(profile(true,3),mode);let mut site=Site::default();site.selected.insert("preexisting");
        none(candidates(&mut e,&["a","b","c"],1));
        let a=seat(read(&mut e,site.count(),2),"a");site.accept("a");none(ack(&mut e,&a,3));
        let b=seat(read(&mut e,site.count(),4),"b");
        // Runtime proved B was never posted because the latest count panel was
        // unreadable. That is cancellation, not a failed site selection.
        none(e.step(Event::ActionCancelled{id:b.id},5));let q=e.selection_count_request().unwrap();
        none(unknown(&mut e,6));none(ack(&mut e,&b,7));none(e.step(Event::Tick,1000));
        assert_eq!(site.count(),2);assert_eq!(e.snapshot().attempted_ids,vec!["a"]);
        none(candidates(&mut e,&["a","b","c"],1001));
        none(e.step(Event::SelectionCountObserved{request_id:q.id,observation_id:7,count:3},1002));
        let retry=seat(read(&mut e,site.count(),1003),"b");assert_ne!(retry.id,b.id);
        site.accept("b");none(ack(&mut e,&retry,1004));
        let complete=one(read(&mut e,site.count(),1005));assert!(matches!(complete.kind,ActionKind::CompleteSelection{..}));
        none(ack(&mut e,&complete,1006));assert_eq!(e.snapshot().phase,Phase::Completed);
        assert_eq!(site.delivered,vec!["a","b"]);assert_eq!(site.count(),3);
    }
}

#[test]
fn completion_revocation_unknown_decrease_and_context_change_need_a_new_matching_observation(){
    let mut e=start(profile(true,2),RunMode::Seats);let mut site=Site::default();site.selected.extend(["existing-a","existing-b"]);
    let complete=one(read(&mut e,site.count(),1));assert!(matches!(complete.kind,ActionKind::CompleteSelection{..}));
    none(e.step(Event::ActionCancelled{id:complete.id},2));let old=e.selection_count_request().unwrap();
    none(unknown(&mut e,3));none(e.step(Event::CompletionConfirmed,4));none(ack(&mut e,&complete,5));
    assert_eq!(e.snapshot().phase,Phase::Completing);
    site.selected.remove("existing-b");none(read(&mut e,site.count(),6));
    none(e.step(Event::CompletionConfirmed,7));none(e.step(Event::Tick,10000));
    none(e.step(Event::ContextLost{reason:"fixture focus".into()},10001));none(e.step(Event::Resume,10002));
    none(e.step(Event::SelectionCountObserved{request_id:old.id,observation_id:10004,count:2},10003));
    site.selected.insert("existing-b");let new_complete=one(read(&mut e,site.count(),10004));
    assert_ne!(new_complete.id,complete.id);assert!(matches!(new_complete.kind,ActionKind::CompleteSelection{..}));
    none(ack(&mut e,&new_complete,10005));assert_eq!(e.snapshot().phase,Phase::Completed);assert!(site.delivered.is_empty());
}

#[test]
fn conflict_of_accounted_seat_cancels_queued_completion_and_replaces_only_lost_selection(){
    for mode in [RunMode::Seats,RunMode::Cancellation]{
        let mut e=start(profile(true,2),mode);let mut site=Site::default();
        none(candidates(&mut e,&["a","b","c"],1));
        let a=seat(read(&mut e,0,2),"a");site.accept("a");none(ack(&mut e,&a,3));
        let b=seat(read(&mut e,site.count(),4),"b");site.accept("b");none(ack(&mut e,&b,5));
        let complete=one(read(&mut e,site.count(),6));
        // An explicit failed result for B and an independently decreased site
        // ledger agree. No image-popup causality is invented by this harness.
        assert!(site.selected.remove("b"));let esc=one(conflict(&mut e,&b,"b",7));
        assert!(matches!(esc.kind,ActionKind::Key{key:Key::Escape,..}));
        none(ack(&mut e,&complete,8));none(ack(&mut e,&esc,9));none(e.step(Event::ConflictDialogGone,10));
        none(candidates(&mut e,&["a","b","c"],11));
        let c=seat(read(&mut e,site.count(),12),"c");site.accept("c");none(ack(&mut e,&c,13));
        let done=one(read(&mut e,site.count(),14));assert!(matches!(done.kind,ActionKind::CompleteSelection{..}));
        assert_eq!(site.selected,BTreeSet::from(["a","c"]));assert_eq!(e.snapshot().rejected_ids,vec!["b"]);
        assert_eq!(e.snapshot().attempted_ids,vec!["a","c"]);
    }
}

#[test]
fn two_distinct_late_failures_share_one_escape_and_do_not_erase_a_remaining_pending_effect(){
    let mut e=start(profile(true,3),RunMode::Seats);let mut site=Site::default();
    none(candidates(&mut e,&["a","b","c","d"],1));
    let a=seat(read(&mut e,0,2),"a");site.accept("a");none(ack(&mut e,&a,3));
    let b=seat(read(&mut e,1,4),"b");site.accept("b");none(ack(&mut e,&b,5));
    let c=seat(read(&mut e,2,6),"c");site.accept("c");none(ack(&mut e,&c,7));
    site.selected.remove("a");let esc=one(conflict(&mut e,&a,"a",8));
    site.selected.remove("b");none(conflict(&mut e,&b,"b",9));none(conflict(&mut e,&a,"a",10));
    assert_eq!(e.snapshot().pending_action_id,Some(esc.id));none(ack(&mut e,&esc,11));none(e.step(Event::ConflictDialogGone,12));
    none(candidates(&mut e,&["a","b","c","d"],13));
    none(read(&mut e,0,14));assert!(e.snapshot().selection_count_pending);
    let d=seat(read(&mut e,site.count(),15),"d");
    assert!(e.snapshot().attempted_ids.contains(&"c".into()));
    assert_eq!(e.snapshot().rejected_ids,vec!["a","b"]);
    none(ack(&mut e,&esc,16));assert_eq!(e.snapshot().pending_action_id,Some(d.id));
}

#[test]
fn cancelling_an_escape_then_pausing_preserves_the_one_unresolved_seat_and_retries_only_escape(){
    let mut e=start(profile(true,2),RunMode::Cancellation);let mut site=Site::default();
    none(candidates(&mut e,&["a","b","c"],1));let a=seat(read(&mut e,0,2),"a");site.accept("a");none(ack(&mut e,&a,3));
    let b=seat(read(&mut e,1,4),"b");none(ack(&mut e,&b,5)); // B delivered, rejected by site.
    let esc=one(conflict(&mut e,&b,"b",6));none(e.step(Event::ActionCancelled{id:esc.id},7));
    none(e.step(Event::Pause,8));none(e.step(Event::Resume,9));none(ack(&mut e,&esc,10));
    assert_eq!(e.snapshot().phase,Phase::Conflict);let retry=one(conflict(&mut e,&b,"b",11));assert_ne!(retry.id,esc.id);
    none(ack(&mut e,&retry,12));none(e.step(Event::ConflictDialogGone,13));none(candidates(&mut e,&["a","b","c"],14));
    assert_eq!(e.selection_count_request().unwrap().after_action_id,Some(retry.id));
    let c=seat(read(&mut e,site.count(),15),"c");site.accept("c");none(ack(&mut e,&c,16));
    assert!(matches!(one(read(&mut e,site.count(),17)).kind,ActionKind::CompleteSelection{..}));assert_eq!(site.selected,BTreeSet::from(["a","c"]));
}

#[test]
fn refresh_and_legacy_navigation_cancellation_rotate_requests_and_never_discard_positive_baseline(){
    for navigation in [CancellationNavigation::None,CancellationNavigation::ArrowKeys,
        CancellationNavigation::TwoRegions{first:target(100.0),second:target(200.0)}]{
        let mut p=profile(true,2);p.features.automatic_refresh=matches!(navigation,CancellationNavigation::None);
        p.refresh_target=Some(target(800.0));p.legacy.cancellation_navigation=navigation;p.features.conflict_recovery=false;
        let mut retained=start(p.clone(),RunMode::Cancellation);
        let mut site=Site::default();site.selected.insert("preexisting");
        none(read(&mut retained,site.count(),1));
        for t in [100,1000,10000] {
            none(candidates(&mut retained,&[],t));none(read(&mut retained,site.count(),t+1));
            none(retained.step(Event::Tick,t+2));
        }
        assert!(retained.snapshot().attempted_ids.is_empty());assert_eq!(site.count(),1);
        let mut e=start(p,RunMode::Cancellation);none(candidates(&mut e,&[],1));
        let request=e.selection_count_request().unwrap();let action=one(read(&mut e,0,30));
        assert!(!matches!(action.kind,ActionKind::CompleteSelection{..}));
        none(e.step(Event::ActionCancelled{id:action.id},31));let rotated=e.selection_count_request().unwrap();assert_ne!(rotated.id,request.id);
        none(ack(&mut e,&action,32));none(e.step(Event::SelectionCountObserved{request_id:request.id,observation_id:100,count:0},33));
        // A changed external total must not be silently accepted as zero or
        // authorize the cancelled navigation. Restore the known zero to resume.
        none(read(&mut e,1,34));none(e.step(Event::Tick,3000));assert_eq!(e.snapshot().phase,Phase::Seats);
        none(candidates(&mut e,&[],3001));let retry=one(read(&mut e,0,3002));assert_ne!(retry.id,action.id);
        none(ack(&mut e,&retry,3003));
    }
}

#[test]
fn repeated_unknown_static_counts_and_stale_requests_keep_one_effect_until_real_increment(){
    let mut e=start(profile(true,2),RunMode::Seats);let mut site=Site::default();
    none(candidates(&mut e,&["a","b"],1));let old=e.selection_count_request().unwrap();let a=seat(read(&mut e,0,2),"a");none(ack(&mut e,&a,3));
    for cycle in 0..150 {let t=10+cycle*4;none(unknown(&mut e,t));none(read(&mut e,site.count(),t+1));none(e.step(Event::SelectionCountObserved{request_id:old.id,observation_id:t+1000,count:2},t+2));none(e.step(Event::Tick,t+3));}
    assert_eq!(e.snapshot().attempted_ids,vec!["a"]);assert_eq!(e.snapshot().phase,Phase::Seats);
    site.accept("a");let b=seat(read(&mut e,site.count(),1000),"b");site.accept("b");none(ack(&mut e,&b,1001));
    assert!(matches!(one(read(&mut e,site.count(),1002)).kind,ActionKind::CompleteSelection{..}));
}

#[test]
fn stop_restart_discards_pending_effect_and_late_count_without_changing_off_policy(){
    for count_on in [false,true]{
        let mut e=start(profile(count_on,2),RunMode::Seats);let first=if count_on{none(candidates(&mut e,&["a","b"],1));seat(read(&mut e,0,2),"a")}else{seat(candidates(&mut e,&["a","b"],1),"a")};
        let after=ack(&mut e,&first,3);if count_on{none(after);}else{seat(after,"b");}
        let old=e.selection_count_request();none(e.step(Event::Stop,4));none(e.step(Event::Start{mode:RunMode::Seats},5));none(ack(&mut e,&first,6));
        if let Some(q)=old{none(e.step(Event::SelectionCountObserved{request_id:q.id,observation_id:100,count:2},7));}
        assert!(e.snapshot().attempted_ids.is_empty());assert!(e.snapshot().delivered_attempts.is_empty());assert_eq!(e.snapshot().site_selection_count,None);
        if count_on{none(candidates(&mut e,&["a"],8));seat(read(&mut e,0,9),"a");}else{seat(candidates(&mut e,&["a"],8),"a");}
    }
}

#[test]
fn latest_frame_candidates_must_precede_its_count_to_avoid_discarding_a_new_seat(){
    for navigation in [CancellationNavigation::None,CancellationNavigation::ArrowKeys,
        CancellationNavigation::TwoRegions{first:target(100.0),second:target(200.0)}] {
        let internal=matches!(navigation,CancellationNavigation::None);
        let mut p=profile(true,2);p.features.automatic_refresh=internal;
        p.refresh_target=Some(target(800.0));p.features.conflict_recovery=false;
        p.legacy.cancellation_navigation=navigation;
        let mut e=start(p,RunMode::Cancellation);
        if !internal {
            let nav=one(read(&mut e,0,1));none(ack(&mut e,&nav,2));none(e.step(Event::Tick,3));
            assert_eq!(e.snapshot().phase,Phase::Seats);
        }
        none(read(&mut e,0,4)); // Initial current frame still has no map evidence.
        none(candidates(&mut e,&[],5)); // Last observed map was empty.
        let mut count_first=e.clone();
        let before=one(read(&mut count_first,0,40));
        assert!(matches!(&before.kind,ActionKind::Refresh{..})
            ||matches!(&before.kind,ActionKind::Click{purpose,..}|ActionKind::Key{purpose,..} if purpose=="legacy_cancellation_region"));
        assert_eq!(count_first.snapshot().phase,Phase::WaitingRefresh);
        // Mirrors the caller's phase != Seats early return: the newly acquired
        // frame's available A is never delivered as CandidatesObserved above.
        let mut candidates_first=e;
        none(candidates(&mut candidates_first,&["a"],40));
        let after=seat(read(&mut candidates_first,0,41),"a");
        println!("same_frame_order internal={internal} count_first={:?} candidates_first={:?}",before.kind,after.kind);
    }
}

#[test]
fn count_map_requirement_is_explicit_and_preserves_completion_and_initial_navigation(){
    for mode in [RunMode::Normal,RunMode::Seats,RunMode::Cancellation] {
        let mut e=Engine::new(profile(true,2)).unwrap();assert!(!e.selection_count_needs_seat_observation(0));
        none(e.step(Event::Start{mode},0));assert!(e.selection_count_needs_seat_observation(0));
        assert!(e.selection_count_needs_seat_observation(1));
        assert!(!e.selection_count_needs_seat_observation(2));assert!(!e.selection_count_needs_seat_observation(3));
        let completion=one(read(&mut e,2,1));assert!(matches!(completion.kind,ActionKind::CompleteSelection{..}));
        assert!(!e.selection_count_needs_seat_observation(1));none(ack(&mut e,&completion,2));
        assert!(!e.selection_count_needs_seat_observation(0));
        let off=start(profile(false,2),mode);assert!(!off.selection_count_needs_seat_observation(0));
    }
    for navigation in [CancellationNavigation::ArrowKeys,
        CancellationNavigation::TwoRegions{first:target(100.0),second:target(200.0)}] {
        let mut p=profile(true,2);p.features.conflict_recovery=false;p.legacy.cancellation_navigation=navigation;
        let mut e=start(p.clone(),RunMode::Cancellation);assert!(!e.selection_count_needs_seat_observation(0));
        let first=one(read(&mut e,0,1));assert!(!e.selection_count_needs_seat_observation(0));
        none(e.step(Event::ActionCancelled{id:first.id},2));assert!(!e.selection_count_needs_seat_observation(0));
        let retry=one(read(&mut e,0,3));none(ack(&mut e,&retry,4));none(e.step(Event::Tick,5));
        assert!(e.selection_count_needs_seat_observation(0));
        let mut retained=start(p,RunMode::Cancellation);none(read(&mut retained,1,1));
        assert_eq!(retained.snapshot().phase,Phase::Seats);assert!(retained.selection_count_needs_seat_observation(1));
    }
}
