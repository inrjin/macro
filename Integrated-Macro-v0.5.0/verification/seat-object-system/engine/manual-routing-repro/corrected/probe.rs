use macro_core::{Action,ActionKind,Candidate,Engine,Event,Phase,Point,Profile,RunMode,SeatOutcome,Target};
fn manual_confirmation_kind(state: &macro_core::Snapshot, resume_phase: Phase, scope: &str) -> Option<String> {
    let phase = if state.phase == Phase::Paused && matches!(resume_phase, Phase::Region | Phase::WaitingRegion) {
        resume_phase
    } else { state.phase };
    match phase {
        Phase::Captcha if !state.optimistic_selection => Some("captcha_completed"),
        Phase::Conflict => Some("dialog_gone"),
        Phase::WaitingRefresh if !state.optimistic_selection => Some("refresh_completed"),
        Phase::Region | Phase::WaitingRegion if !state.optimistic_selection => Some("region_map_ready"),
        Phase::Seats if !state.optimistic_selection && !state.selection_count_verification
            && (scope == "selection" || state.current_candidate_id.is_some()) => Some("selection"),
        Phase::Entry if !state.optimistic_selection => Some("seat_map"),
        _ => None,
    }.map(str::to_string)
}


fn context_confirmation_phase(state: &macro_core::Snapshot, resume_phase: Phase, recovering: bool) -> Phase {
    if recovering && (state.optimistic_selection || state.selection_count_verification) && state.phase == Phase::Paused
        && matches!(resume_phase, Phase::Seats | Phase::Completing | Phase::Conflict) {
        resume_phase
    } else {
        state.phase
    }
}


fn conflict_confirmation_event(state: &macro_core::Snapshot, selected_ids: Option<&[String]>) -> Result<Event, String> {
    let requested = selected_ids.and_then(|ids| ids.first());
    if state.optimistic_selection || state.selection_count_verification {
        // A queued current candidate has not received input. Attribute a generic
        // manual report to the latest delivered attempt, never to that candidate.
        // This is a fallback estimate; it cannot identify a delayed server error.
        let attempt = match requested {
            Some(id) => state.delivered_attempts.iter().rev().find(|attempt| &attempt.candidate_id == id),
            None => state.delivered_attempts.last(),
        }.ok_or("실패로 처리할 클릭 기록이 없습니다. 아직 전달하지 않은 좌석은 차감하지 않습니다.")?;
        return Ok(Event::DeliveredSeatResult {
            action_id: attempt.action_id,
            candidate_id: attempt.candidate_id.clone(),
            result: SeatOutcome::Conflict,
        });
    }
    let id = requested.cloned().or_else(|| state.current_candidate_id.clone())
        .ok_or("선점 실패한 좌석을 특정할 수 없습니다. 실패한 좌석을 확인하세요.")?;
    Ok(Event::SeatResult { candidate_id: id, result: SeatOutcome::Conflict })
}

fn physical(actions:&[Action])->Vec<&Action>{actions.iter().filter(|a|!matches!(a.kind,ActionKind::Observe{..})).collect()}
fn count(e:&mut Engine,n:usize,time:u64)->Vec<Action>{let r=e.selection_count_request().unwrap();e.step(Event::SelectionCountObserved{request_id:r.id,observation_id:time,count:n},time)}
fn main(){
 let mut p=Profile::default();p.features.nol_selection_count=true;p.features.conflict_recovery=true;p.target_seats=2;let color=p.color;
 let mut e=Engine::new(p).unwrap();e.step(Event::Start{mode:RunMode::Normal},0);
 assert!(!e.snapshot().optimistic_selection);assert!(e.snapshot().selection_count_verification);
 let candidates=["delivered-a","queued-b"].iter().enumerate().map(|(i,id)|{let point=Point{x:10.+i as f64*20.,y:20.};Candidate{id:(*id).into(),point,color,target:Target::Point{point},selected:false,geometry:None}}).collect();
 e.step(Event::CandidatesObserved{generation:e.snapshot().generation,candidates},1);
 let actions=count(&mut e,0,2);let a=physical(&actions)[0].clone();
 assert!(manual_confirmation_kind(&e.snapshot(),Phase::Seats,"seat_result").is_none());assert!(conflict_confirmation_event(&e.snapshot(),None).is_err());
 println!("FIX1 count mode does not advertise ineffective manual selection; no failure before delivery");
 e.step(Event::ActionAcknowledged{id:a.id,success:true,detail:String::new()},3);
 let event=conflict_confirmation_event(&e.snapshot(),None).unwrap();
 assert!(matches!(&event,Event::DeliveredSeatResult{action_id,candidate_id,..} if *action_id==a.id&&candidate_id=="delivered-a"));
 println!("FIX2 generic failure finds delivered A even when current candidate is empty");
 let actions=count(&mut e,1,4);let b=physical(&actions)[0].clone();let before=e.clone();
 assert!(conflict_confirmation_event(&e.snapshot(),Some(&["queued-b".into()])).is_err());
 let event=conflict_confirmation_event(&e.snapshot(),None).unwrap();
 assert!(matches!(&event,Event::DeliveredSeatResult{action_id,candidate_id,..} if *action_id==a.id&&candidate_id=="delivered-a"));
 let emitted=e.step(event,5);assert_eq!(e.snapshot().phase,Phase::Conflict);assert_eq!(physical(&emitted).len(),1);assert_eq!(e.snapshot().rejected_ids,vec!["delivered-a"]);assert!(e.snapshot().attempted_ids.is_empty());
 let escaped=e.snapshot().pending_action_id;e.step(Event::ActionAcknowledged{id:b.id,success:true,detail:String::new()},6);assert_eq!(e.snapshot().pending_action_id,escaped);
 println!("FIX3 generic failure excludes only delivered A, sends one Esc, never charges queued B or accepts its stale ACK");
 let mut held=before;held.step(Event::ContextLost{reason:"settings foreground".into()},5);
 assert_eq!(context_confirmation_phase(&held.snapshot(),Phase::Seats,true),Phase::Seats);
 assert_eq!(context_confirmation_phase(&held.snapshot(),Phase::Seats,false),Phase::Paused);
 println!("FIX4 automatic context hold permits deliberate failure reporting; manual Pause stays paused");
 let mut legacy=held.snapshot();legacy.selection_count_verification=false;legacy.phase=Phase::Seats;
 assert_eq!(manual_confirmation_kind(&legacy,Phase::Seats,"seat_result").as_deref(),Some("selection"));
 for (phase,expected) in [(Phase::Entry,"seat_map"),(Phase::Captcha,"captcha_completed"),(Phase::Region,"region_map_ready"),(Phase::WaitingRefresh,"refresh_completed")] {
  legacy.selection_count_verification=true;legacy.phase=phase;assert_eq!(manual_confirmation_kind(&legacy,phase,"seats").as_deref(),Some(expected));
 }
 println!("CONTROLS original count-OFF selection and Entry/CAPTCHA/Region/Refresh confirmations are preserved");
}
