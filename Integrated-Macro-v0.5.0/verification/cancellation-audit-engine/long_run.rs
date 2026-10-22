use macro_core::*;
fn target(x: f64) -> Target { Target::Point { point: Point { x, y: 1.0 } } }
fn candidate(id: &str, x: f64) -> Candidate { Candidate { id: id.into(), point: Point { x, y: 1.0 },
    target: target(x), color: Profile::default().color, selected: false } }
fn physical(actions: &[Action]) -> Vec<&Action> { actions.iter().filter(|a| !matches!(a.kind, ActionKind::Observe { .. })).collect() }
fn ack(e: &mut Engine, a: &[Action], t: u64) -> Vec<Action> { e.step(Event::ActionAcknowledged {
    id: physical(a)[0].id, success: true, detail: String::new() }, t) }
fn observe(e: &mut Engine, candidates: Vec<Candidate>, t: u64) -> Vec<Action> {
    e.step(Event::CandidatesObserved { generation: e.snapshot().generation, candidates }, t)
}
fn main() {
    for refresh in [false, true] {
        let mut p = Profile::default(); p.target_seats = 2; p.features.conflict_recovery = true;
        p.features.automatic_refresh = refresh; p.refresh_target = Some(target(90.0));
        p.refresh_interval_ms = 100; p.legacy.complete = Some(target(99.0));
        let mut e = Engine::new(p).unwrap(); e.step(Event::Start { mode: RunMode::Cancellation }, 0);
        let first = observe(&mut e, vec![candidate("kept", 1.0)], 1); ack(&mut e, &first, 2);
        let kept = e.snapshot().delivered_attempts[0].clone();
        let mut t = 3;
        for iteration in 0..1000 {
            let click = observe(&mut e, vec![candidate("kept", 1.0), candidate("failed", 2.0)], t);
            let completion = ack(&mut e, &click, t + 1);
            assert!(matches!(physical(&completion)[0].kind, ActionKind::CompleteSelection { .. }));
            let failed = e.snapshot().delivered_attempts[1].clone();
            let failure = || Event::DeliveredSeatResult { action_id: failed.action_id,
                candidate_id: failed.candidate_id.clone(), result: SeatOutcome::Conflict };
            let mut escape = e.step(failure(), t + 2);
            assert!(ack(&mut e, &completion, t + 3).is_empty());
            if iteration % 5 == 0 {
                let old_escape = physical(&escape)[0].id;
                e.step(Event::ActionCancelled { id: old_escape }, t + 4);
                e.step(Event::Pause, t + 5); e.step(Event::Resume, t + 6);
                escape = e.step(failure(), t + 7);
                assert!(matches!(physical(&escape)[0].kind, ActionKind::Key { key: Key::Escape, .. }));
                assert!(e.step(Event::ActionAcknowledged { id: old_escape, success: true, detail: String::new() }, t + 8).is_empty());
            }
            ack(&mut e, &escape, t + 9); e.step(Event::ConflictDialogGone, t + 10);
            let retry_poll = e.step(Event::Tick, t + 110);
            assert!(physical(&retry_poll).is_empty());
            let state = e.snapshot();
            assert_eq!(state.phase, Phase::Seats); assert_eq!(state.generation, 0);
            assert_eq!(state.attempted_ids, vec!["kept"]); assert!(state.selected_ids.is_empty());
            assert_eq!(state.delivered_attempts, vec![kept.clone()]); assert!(state.rejected_ids.is_empty());
            t += 111;
        }
        println!("PASS retained+failed x1000 refresh={refresh}: count/attempt bound, generation0, cancelled Esc resume x200, stale completion/Esc ACK inert");
    }
    for refresh in [false, true] {
        let mut p = Profile::default(); p.features.automatic_refresh = refresh;
        p.refresh_target = Some(target(90.0)); p.refresh_interval_ms = 100;
        let mut e = Engine::new(p).unwrap(); e.step(Event::Start { mode: RunMode::Cancellation }, 0);
        observe(&mut e, vec![], 1);
        let mut t = 101;
        for _ in 0..1000 {
            let a = e.step(Event::Tick, t);
            if refresh {
                assert!(matches!(physical(&a)[0].kind, ActionKind::Refresh { .. }));
                ack(&mut e, &a, t + 1); observe(&mut e, vec![], t + 2);
                t += 101;
            } else {
                assert!(physical(&a).is_empty());
                assert!(a.iter().any(|a| matches!(&a.kind, ActionKind::Observe { scope } if scope == "seats")));
                observe(&mut e, vec![], t); t += 100;
            }
            assert_eq!(e.snapshot().phase, Phase::Seats); assert_eq!(e.snapshot().generation, 0);
        }
        let seat = observe(&mut e, vec![candidate("arrived", 1.0)], t + 1);
        assert!(matches!(&physical(&seat)[0].kind, ActionKind::Click { purpose, .. } if purpose == "seat:arrived"));
        println!("PASS empty-map x1000 refresh={refresh}: continuous poll/button cadence, no generation invention, arriving seat selected");
    }
    let mut p = Profile::default(); p.legacy.complete = Some(target(99.0));
    p.legacy.cancellation_navigation = CancellationNavigation::TwoRegions { first: target(10.0), second: target(20.0) };
    p.legacy.region_delay_ms = 10;
    p.legacy.regions = vec![LegacyRegionSettings { color: Some(Color { r: 1, g: 2, b: 3 }),
        vision: Some(VisionConfig { roi: Some(Rect { x: 0.0, y: 0.0, width: 10.0, height: 10.0 }), ..VisionConfig::default() }),
        complete: Some(target(98.0)) }, LegacyRegionSettings::default()];
    let global = p.clone(); let mut e = Engine::new(p).unwrap();
    let mut navigation = e.step(Event::Start { mode: RunMode::Cancellation }, 0); let mut t = 1;
    for iteration in 0..1000 {
        let region = iteration % 2;
        assert_eq!(e.snapshot().active_region, Some(region));
        ack(&mut e, &navigation, t); e.step(Event::Tick, t + 10);
        assert_eq!(e.snapshot().generation, iteration as u64 + 1);
        assert_eq!(e.effective_color(), if region == 0 { global.legacy.regions[0].color.unwrap() } else { global.color });
        assert_eq!(e.effective_vision().roi, if region == 0 { global.legacy.regions[0].vision.as_ref().unwrap().roi } else { global.vision.roi });
        assert_eq!(e.effective_completion_target(), if region == 0 { global.legacy.regions[0].complete.as_ref() } else { global.legacy.complete.as_ref() });
        observe(&mut e, vec![], t + 11); navigation = e.step(Event::Tick, t + 11);
        assert_eq!(physical(&navigation).len(), 1); t += 12;
    }
    println!("PASS alternating zones x1000: generation increments once, color/ROI/complete override and fallback remain consistent");
}
