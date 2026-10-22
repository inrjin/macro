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
}
