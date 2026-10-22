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
        Phase::Seats if !state.optimistic_selection
            && (scope == "selection" || state.current_candidate_id.is_some()) => Some("selection"),
        Phase::Entry if !state.optimistic_selection => Some("seat_map"),
        _ => None,
    }.map(str::to_string)
}


fn context_confirmation_phase(state: &macro_core::Snapshot, resume_phase: Phase, recovering: bool) -> Phase {
    if recovering && state.optimistic_selection && state.phase == Phase::Paused
        && matches!(resume_phase, Phase::Seats | Phase::Completing | Phase::Conflict) {
        resume_phase
    } else {
        state.phase
    }
}


fn conflict_confirmation_event(state: &macro_core::Snapshot, selected_ids: Option<&[String]>) -> Result<Event, String> {
    let requested = selected_ids.and_then(|ids| ids.first());
    if state.optimistic_selection {
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
 let mut p=Profile::default(); p.features.nol_selection_count=true;p.features.conflict_recovery=true;p.target_seats=2;
 let color=p.color;let mut e=Engine::new(p).unwrap();
 assert!(physical(&e.step(Event::Start{mode:RunMode::Normal},0)).is_empty());
 assert!(!e.snapshot().optimistic_selection);
 let candidates=["delivered-a","queued-b"].iter().enumerate().map(|(i,id)|{let point=Point{x:10.+i as f64*20.,y:20.};Candidate{id:(*id).into(),point,color,target:Target::Point{point},selected:false,geometry:None}}).collect();
 e.step(Event::CandidatesObserved{generation:e.snapshot().generation,candidates},1);
 let actions=count(&mut e,0,2);let a=physical(&actions)[0].clone();
 assert_eq!(manual_confirmation_kind(&e.snapshot(),Phase::Seats,"seat_result").as_deref(),Some("selection"));
 e.step(Event::SelectionObserved{selected_ids:vec!["delivered-a".into()]},3);
 assert_eq!(e.snapshot().pending_action_id,Some(a.id));assert!(e.snapshot().selected_ids.is_empty());
 println!("CASE1 advertised selection confirmation ignored; pending input unchanged");
 e.step(Event::ActionAcknowledged{id:a.id,success:true,detail:String::new()},4);
 assert_eq!(e.snapshot().delivered_attempts.len(),1);
 assert!(conflict_confirmation_event(&e.snapshot(),None).is_err());
 println!("CASE2 delivered seat exists, generic manual failure returns missing-seat error");
 let actions=count(&mut e,1,5);let b=physical(&actions)[0].clone();
 let event=conflict_confirmation_event(&e.snapshot(),None).unwrap();
 assert!(matches!(&event,Event::SeatResult{candidate_id,result:SeatOutcome::Conflict} if candidate_id=="queued-b"));
 let before=e.snapshot();let emitted=e.step(event,6);
 assert!(physical(&emitted).is_empty());assert_eq!(e.snapshot().pending_action_id,Some(b.id));assert_eq!(e.snapshot().delivered_attempts,before.delivered_attempts);assert!(e.snapshot().rejected_ids.is_empty());
 println!("CASE3 generic manual failure targets undelivered B and is silently ignored; delivered A remains");
 let mut correct=e.clone();let fixed=Event::DeliveredSeatResult{action_id:a.id,candidate_id:"delivered-a".into(),result:SeatOutcome::Conflict};
 let good=correct.step(fixed,7);assert_eq!(correct.snapshot().phase,Phase::Conflict);assert_eq!(physical(&good).len(),1);assert_eq!(correct.snapshot().rejected_ids,vec!["delivered-a"]);
 println!("CONTROL explicit delivered action A produces one Esc and excludes only A");
 e.step(Event::ContextLost{reason:"settings foreground".into()},7);
 assert_eq!(context_confirmation_phase(&e.snapshot(),Phase::Seats,true),Phase::Paused);
 println!("CASE4 automatic context hold is not exposed for count-mode manual failure");
}
