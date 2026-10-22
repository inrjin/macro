use macro_core::*;
fn target(x: f64) -> Target { Target::Point { point: Point { x, y: 10.0 } } }
fn candidate(id: &str, x: f64) -> Candidate {
    Candidate { id: id.into(), point: Point { x, y: 10.0 }, color: Profile::default().color,
        target: target(x), selected: false }
}
fn physical(actions: &[Action]) -> Vec<&Action> {
    actions.iter().filter(|action| !matches!(action.kind, ActionKind::Observe { .. })).collect()
}
fn ack(engine: &mut Engine, actions: &[Action], at: u64) -> Vec<Action> {
    engine.step(Event::ActionAcknowledged { id: physical(actions)[0].id, success: true, detail: String::new() }, at)
}
fn observe(engine: &mut Engine, at: u64) -> Vec<Action> {
    engine.step(Event::CandidatesObserved { generation: engine.snapshot().generation,
        candidates: vec![candidate("kept", 10.0), candidate("failed", 20.0), candidate("replacement", 30.0)] }, at)
}
fn main() {
    for automatic_refresh in [false, true] {
        for confirm_kept in [false, true] {
            let mut profile = Profile::default();
            profile.target_seats = 3;
            profile.features.conflict_recovery = true;
            profile.features.automatic_refresh = automatic_refresh;
            profile.refresh_target = Some(target(90.0));
            profile.refresh_interval_ms = 100;
            let mut engine = Engine::new(profile).unwrap();
            engine.step(Event::Start { mode: RunMode::Cancellation }, 0);
            let first = observe(&mut engine, 1);
            let second = ack(&mut engine, &first, 2);
            let queued_third = ack(&mut engine, &second, 3);
            let failed = engine.snapshot().delivered_attempts[1].clone();
            if confirm_kept {
                engine.step(Event::SelectionObserved { selected_ids: vec!["kept".into()] }, 3);
            }
            let escape = engine.step(Event::DeliveredSeatResult {
                action_id: failed.action_id, candidate_id: failed.candidate_id.clone(), result: SeatOutcome::Conflict,
            }, 4);
            // The old next-seat action was never delivered and its late ACK is inert.
            ack(&mut engine, &queued_third, 5);
            assert_eq!(engine.snapshot().pending_action_id, Some(physical(&escape)[0].id));
            ack(&mut engine, &escape, 6);
            engine.step(Event::ConflictDialogGone, 7);
            let replacement = observe(&mut engine, 8);
            assert!(matches!(&physical(&replacement)[0].kind, ActionKind::Click { purpose, .. } if purpose == "seat:replacement"));
            ack(&mut engine, &replacement, 9);
            let before = engine.snapshot();
            assert_eq!(before.rejected_ids, vec!["failed"]);
            let pass = engine.step(Event::Tick, 107);
            assert!(physical(&pass).is_empty());
            let after = engine.snapshot();
            assert_eq!(after.generation, before.generation);
            assert_eq!(after.selected_ids, before.selected_ids);
            assert_eq!(after.attempted_ids, before.attempted_ids);
            assert_eq!(after.delivered_attempts, before.delivered_attempts);
            assert!(after.rejected_ids.is_empty());
            let retry = observe(&mut engine, 108);
            assert!(matches!(&physical(&retry)[0].kind, ActionKind::Click { purpose, .. } if purpose == "seat:failed"));
            let stale = engine.step(Event::DeliveredSeatResult {
                action_id: failed.action_id, candidate_id: failed.candidate_id, result: SeatOutcome::Conflict,
            }, 109);
            assert!(stale.is_empty());
            assert_eq!(engine.snapshot().pending_action_id, Some(physical(&retry)[0].id));
            ack(&mut engine, &retry, 110);
            let state = engine.snapshot();
            assert_eq!(state.phase, Phase::Completing);
            assert_eq!(state.selected_ids.len() + state.attempted_ids.len(), 3);
            println!("PASS refreshFlag={automatic_refresh} confirmedRetained={confirm_kept}: retained state unchanged, same-generation retry, no refresh, exact shortfall, stale ACK/result ignored");
        }
    }
}
