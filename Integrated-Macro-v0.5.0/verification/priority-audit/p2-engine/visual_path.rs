//! Standalone synthetic-pixel probe. Real vision, Engine and VisualRecovery;
//! native delivery is a simulated ACK, not an OS or server acknowledgement.
#![allow(dead_code)]
#[path = "../../../src-tauri/src/visual_recovery.rs"]
mod visual_recovery;
use macro_core::*;
use macro_core::vision::{extract_candidates, RgbaFrame, RgbaView};
use macro_core::visual_dialog::{sample_frame, VisualSample};
use visual_recovery::{VisualRecovery, VisualSignal};

fn frame(accepted: &[usize], popup: bool) -> RgbaFrame {
    let mut frame = RgbaFrame { width: 64, height: 48, rgba: vec![255; 64 * 48 * 4] };
    let color = Profile::default().color;
    for (seat, left) in [10, 30, 50].into_iter().enumerate() {
        for y in 20..23 { for x in left..left + 3 {
            let offset = (y * 64 + x) * 4;
            let rgb = if accepted.contains(&seat) { [150, 150, 150] } else { [color.r, color.g, color.b] };
            frame.rgba[offset..offset + 3].copy_from_slice(&rgb);
        }}
    }
    if popup {
        for y in 12..36 { for x in 18..46 {
            let offset = (y * 64 + x) * 4;
            frame.rgba[offset..offset + 3].fill(30);
        }}
    }
    frame
}
fn sample(frame: &RgbaFrame, sequence: u64) -> VisualSample {
    sample_frame(RgbaView { width: frame.width, height: frame.height,
        stride: frame.width as usize * 4, rgba: &frame.rgba }, sequence).unwrap()
}
fn physical(actions: &[Action]) -> Action {
    actions.iter().find(|action| !matches!(action.kind, ActionKind::Observe { .. })).unwrap().clone()
}
fn ack(engine: &mut Engine, action: &Action, now: u64) -> Vec<Action> {
    engine.step(Event::ActionAcknowledged { id: action.id, success: true, detail: String::new() }, now)
}
fn scan(engine: &mut Engine, pixels: &RgbaFrame, now: u64) -> Vec<Action> {
    let generation = engine.snapshot().generation;
    let candidates = extract_candidates(pixels, engine.effective_color(),
        engine.profile().color_tolerance, engine.effective_vision(), generation).unwrap();
    engine.step(Event::CandidatesObserved { generation, candidates }, now)
}
fn setup() -> (Engine, VisualRecovery, Action, Action) {
    let mut profile = Profile::default();
    profile.target_seats = 2;
    profile.features.conflict_recovery = true;
    profile.legacy.complete = Some(Target::Point { point: Point { x: 60.0, y: 44.0 } });
    profile.vision.min_component_pixels = 3;
    profile.vision.max_component_pixels = 9;
    let mut engine = Engine::new(profile).unwrap();
    engine.step(Event::Start { mode: RunMode::Cancellation }, 0);
    let first = physical(&scan(&mut engine, &frame(&[], false), 1));
    let second = physical(&ack(&mut engine, &first, 2));
    assert!(matches!(&first.kind, ActionKind::Click { purpose, .. } if purpose.starts_with("seat:")));
    assert!(matches!(&second.kind, ActionKind::Click { purpose, .. } if purpose.starts_with("seat:")));
    let mut recovery = VisualRecovery::default();
    recovery.arm(sample(&frame(&[], false), 1), engine.snapshot().delivered_attempts[0].clone());
    (engine, recovery, first, second)
}
fn consume_signal(engine: &mut Engine, signal: VisualSignal, now: u64) -> Vec<Action> {
    // The failed ID comes only from the real pixel detector's signal, never
    // from the fixture's independent knowledge of which request failed.
    match signal {
        VisualSignal::Suspected { attempt, .. } => engine.step(Event::DeliveredSeatResult {
            action_id: attempt.action_id, candidate_id: attempt.candidate_id,
            result: SeatOutcome::Conflict,
        }, now),
        VisualSignal::Restored { .. } => engine.step(Event::ConflictDialogGone, now),
    }
}
fn on_time_failure_preserves_other_attempt() {
    let (mut engine, mut recovery, first, second) = setup();
    assert!(recovery.observe(sample(&frame(&[0], false), 2)).unwrap().is_none());
    let completion = physical(&ack(&mut engine, &second, 3));
    let failed = engine.snapshot().delivered_attempts[1].clone();
    recovery.arm(sample(&frame(&[0], false), 2), failed.clone());
    let signal = recovery.observe(sample(&frame(&[0], true), 3)).unwrap().unwrap();
    let escape = physical(&consume_signal(&mut engine, signal, 4));
    assert!(matches!(escape.kind, ActionKind::Key { key: Key::Escape, .. }));
    assert_eq!(engine.snapshot().delivered_attempts[0].action_id, first.id);
    assert_eq!(engine.snapshot().conflict_attempt.as_ref().unwrap().action_id, second.id);
    assert!(ack(&mut engine, &completion, 5).is_empty());
    ack(&mut engine, &escape, 6);
    assert!(recovery.observe(sample(&frame(&[0], true), 4)).unwrap().is_none());
    let signal = recovery.observe(sample(&frame(&[0], false), 5)).unwrap().unwrap();
    consume_signal(&mut engine, signal, 7);
    let replacement = physical(&scan(&mut engine, &frame(&[0], false), 8));
    assert!(matches!(replacement.kind, ActionKind::Click { target: Target::Point { point }, .. } if point.x == 51.0));
    assert_eq!(engine.snapshot().attempted_ids.len(), 1);
    println!("PASS on-time second-seat popup: actual visual signal -> Esc -> actual restoration -> third pixel candidate, first attempt preserved");
}
fn late_first_failure_is_attributed_to_latest_attempt() {
    let (mut engine, mut recovery, first, second) = setup();
    // Independent fixture schedule: first request will fail later, second is
    // accepted. Neither fact is sent as a seat-result event to the application.
    let failed_request = first.id;
    let accepted_request = second.id;
    assert!(recovery.observe(sample(&frame(&[], false), 2)).unwrap().is_none());
    ack(&mut engine, &second, 3);
    let newest = engine.snapshot().delivered_attempts[1].clone();
    recovery.arm(sample(&frame(&[], false), 2), newest);
    let signal = recovery.observe(sample(&frame(&[1], true), 3)).unwrap().unwrap();
    let VisualSignal::Suspected { attempt, .. } = &signal else { panic!("popup must be suspected") };
    assert_eq!(attempt.action_id, accepted_request);
    assert_ne!(attempt.action_id, failed_request);
    consume_signal(&mut engine, signal, 4);
    assert_eq!(engine.snapshot().delivered_attempts[0].action_id, failed_request);
    assert_eq!(engine.snapshot().conflict_attempt.unwrap().action_id, accepted_request);
    println!("REPRODUCED late first-seat popup: detector attributes it to latest second attempt; failed first attempt remains estimated, accepted second is removed");
}
fn popup_after_delivered_completion_is_outside_watch() {
    let (mut engine, mut recovery, _, second) = setup();
    let completion = physical(&ack(&mut engine, &second, 3));
    recovery.arm(sample(&frame(&[0], false), 2), engine.snapshot().delivered_attempts[1].clone());
    assert!(recovery.observe(sample(&frame(&[0], false), 3)).unwrap().is_none());
    // Match runtime's documented successful completion/navigation watch reset.
    recovery.reset();
    ack(&mut engine, &completion, 4);
    assert_eq!(engine.snapshot().phase, Phase::Completed);
    assert!(recovery.observe(sample(&frame(&[0], true), 4)).unwrap().is_none());
    println!("REPRODUCED post-completion popup: successful completion delivery ends visual watch, so later pixels generate no failure signal");
}
fn main() {
    on_time_failure_preserves_other_attempt();
    late_first_failure_is_attributed_to_latest_attempt();
    popup_after_delivered_completion_is_outside_watch();
    println!("3/3 synthetic pixel/production component probes completed; no live browser or native input was executed");
}
