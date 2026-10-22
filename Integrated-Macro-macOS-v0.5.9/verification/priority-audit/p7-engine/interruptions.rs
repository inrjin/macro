//! Actual Engine + image extraction + VisualRecovery interruption matrix.
//! ACKs model input delivery only. No OS, browser or server execution occurs.
#![allow(dead_code)]
#[path = "../../../src-tauri/src/visual_recovery.rs"]
mod visual_recovery;
use macro_core::*;
use macro_core::vision::{extract_candidates, RgbaFrame, RgbaView};
use macro_core::visual_dialog::sample_frame;
use visual_recovery::{VisualRecovery, VisualSignal};

fn frame(popup: bool) -> RgbaFrame {
    let mut frame = RgbaFrame { width: 64, height: 48, rgba: vec![255; 64 * 48 * 4] };
    let c = Profile::default().color;
    for left in [10, 30, 50] { for y in 20..23 { for x in left..left + 3 {
        let n = (y * 64 + x) * 4;
        frame.rgba[n..n + 3].copy_from_slice(&[c.r, c.g, c.b]);
    }}}
    if popup { for y in 12..36 { for x in 18..46 {
        let n = (y * 64 + x) * 4;
        frame.rgba[n..n + 3].fill(30);
    }}}
    frame
}
fn sample(sequence: u64, popup: bool) -> macro_core::visual_dialog::VisualSample {
    let f = frame(popup);
    sample_frame(RgbaView { width: f.width, height: f.height, stride: 256, rgba: &f.rgba }, sequence).unwrap()
}
fn setup() -> Engine {
    let mut p = Profile::default();
    p.target_seats = 2;
    p.features.conflict_recovery = true;
    p.legacy.complete = Some(Target::Point { point: Point { x: 60.0, y: 44.0 } });
    p.vision.min_component_pixels = 3;
    p.vision.max_component_pixels = 9;
    let mut e = Engine::new(p).unwrap();
    e.step(Event::Start { mode: RunMode::Cancellation }, 0);
    e
}
fn input(actions: &[Action]) -> Action {
    actions.iter().find(|a| !matches!(a.kind, ActionKind::Observe { .. })).unwrap().clone()
}
fn no_input(actions: &[Action]) {
    assert!(actions.iter().all(|a| matches!(a.kind, ActionKind::Observe { .. })));
}
fn ack(e: &mut Engine, a: &Action, now: u64) -> Vec<Action> {
    e.step(Event::ActionAcknowledged { id: a.id, success: true, detail: String::new() }, now)
}
fn scan(e: &mut Engine, now: u64) -> Vec<Action> {
    let generation = e.snapshot().generation;
    let candidates = extract_candidates(&frame(false), e.effective_color(), e.profile().color_tolerance,
        e.effective_vision(), generation).unwrap();
    e.step(Event::CandidatesObserved { generation, candidates }, now)
}
fn control(e: &mut Engine, context_lost: bool, now: u64) {
    e.step(if context_lost { Event::ContextLost { reason: "synthetic foreground loss".into() } }
        else { Event::Pause }, now);
    assert_eq!(e.snapshot().phase, Phase::Paused);
    assert!(e.snapshot().pending_action_id.is_none());
}
fn pause_queued_seat_matrix() {
    for context_lost in [false, true] { for first_delivered in [false, true] {
        let mut e = setup();
        let first = input(&scan(&mut e, 1));
        let queued = if first_delivered { input(&ack(&mut e, &first, 2)) } else { first.clone() };
        let kept = e.snapshot().delivered_attempts;
        // Mirror runtime cancel_queued_actions before either pause route.
        e.step(Event::ActionCancelled { id: queued.id }, 3);
        control(&mut e, context_lost, 4);
        no_input(&ack(&mut e, &queued, 5));
        assert_eq!(e.snapshot().delivered_attempts, kept);
        no_input(&e.step(Event::Resume, 6));
        no_input(&e.step(Event::Tick, 7));
        let resumed = input(&scan(&mut e, 8));
        assert_ne!(resumed.id, queued.id);
        assert_eq!(format!("{:?}", resumed.kind), format!("{:?}", queued.kind));
        if first_delivered {
            assert_ne!(format!("{:?}", resumed.kind), format!("{:?}", first.kind));
            assert_eq!(e.snapshot().attempted_ids.len(), 1);
        } else { assert!(e.snapshot().attempted_ids.is_empty()); }
        ack(&mut e, &resumed, 9);
        assert_eq!(e.snapshot().delivered_attempts.len(), if first_delivered { 2 } else { 1 });
    }}
    println!("PASS 4 pause/context-loss cases: queued input IDs discarded, stale ACK inert, delivered first seat retained, only fresh candidate resumes");
}
fn conflict(e: &mut Engine, a: &DeliveredSeatAttempt, now: u64) -> Vec<Action> {
    e.step(Event::DeliveredSeatResult { action_id: a.action_id, candidate_id: a.candidate_id.clone(),
        result: SeatOutcome::Conflict }, now)
}
fn pause_escape_matrix() {
    for context_lost in [false, true] { for escape_delivered in [false, true] {
        let mut e = setup();
        let first = input(&scan(&mut e, 1));
        let second = input(&ack(&mut e, &first, 2));
        let completion = input(&ack(&mut e, &second, 3));
        let mut v = VisualRecovery::default();
        v.arm(sample(1, false), e.snapshot().delivered_attempts[1].clone());
        let Some(VisualSignal::Suspected { attempt, .. }) = v.observe(sample(2, true)).unwrap() else { panic!("no real visual incident") };
        let escape = input(&conflict(&mut e, &attempt, 4));
        assert!(matches!(escape.kind, ActionKind::Key { key: Key::Escape, .. }));
        if escape_delivered { ack(&mut e, &escape, 5); }
        else { e.step(Event::ActionCancelled { id: escape.id }, 5); }
        control(&mut e, context_lost, 6);
        no_input(&ack(&mut e, &completion, 7));
        if !escape_delivered { no_input(&ack(&mut e, &escape, 8)); }
        assert_eq!(e.snapshot().delivered_attempts[0].action_id, first.id);
        no_input(&e.step(Event::Resume, 9));
        assert!(v.observe(sample(3, true)).unwrap().is_none());
        let rearmed = conflict(&mut e, &attempt, 10);
        if escape_delivered { no_input(&rearmed); }
        else {
            let retry = input(&rearmed);
            assert!(matches!(retry.kind, ActionKind::Key { key: Key::Escape, .. }));
            assert_ne!(retry.id, escape.id);
            ack(&mut e, &retry, 11);
        }
        no_input(&conflict(&mut e, &attempt, 12));
        assert!(v.observe(sample(4, true)).unwrap().is_none());
        assert!(matches!(v.observe(sample(5, false)).unwrap(), Some(VisualSignal::Restored { .. })));
        no_input(&e.step(Event::ConflictDialogGone, 13));
        let next = input(&scan(&mut e, 14));
        assert!(matches!(next.kind, ActionKind::Click { target: Target::Point { point }, .. } if point.x == 51.0));
        assert_eq!(e.snapshot().attempted_ids.len(), 1);
        assert_eq!(e.snapshot().rejected_ids.len(), 1);
    }}
    println!("PASS 4 popup interruption cases: unsent Esc retries once after resume, delivered Esc does not repeat, actual restoration releases search, first seat preserved");
}
fn stop_matrix() {
    for stage in ["first_queued", "second_queued", "completion_queued", "completion_delivered", "escape_queued"] {
        let mut e = setup();
        let first = input(&scan(&mut e, 1));
        let mut last = first.clone();
        if stage != "first_queued" { last = input(&ack(&mut e, &first, 2)); }
        if matches!(stage, "completion_queued" | "completion_delivered" | "escape_queued") {
            last = input(&ack(&mut e, &last, 3));
        }
        let mut v = VisualRecovery::default();
        if stage == "escape_queued" {
            v.arm(sample(1, false), e.snapshot().delivered_attempts[1].clone());
            let Some(VisualSignal::Suspected { attempt, .. }) = v.observe(sample(2, true)).unwrap() else { panic!("no incident") };
            last = input(&conflict(&mut e, &attempt, 4));
        }
        if stage == "completion_delivered" { ack(&mut e, &last, 4); assert_eq!(e.snapshot().phase, Phase::Completed); }
        // Match runtime Stop: clear physical queue and visual watcher, then Stop.
        v.reset();
        no_input(&e.step(Event::Stop, 5));
        no_input(&ack(&mut e, &last, 6));
        no_input(&e.step(Event::Resume, 7));
        no_input(&e.step(Event::Tick, 8));
        no_input(&scan(&mut e, 9));
        assert_eq!(e.snapshot().phase, Phase::Stopped);
        assert!(e.snapshot().pending_action_id.is_none());
        assert!(v.observe(sample(3, true)).unwrap().is_none());
        e.step(Event::Start { mode: RunMode::Cancellation }, 10);
        assert!(e.snapshot().delivered_attempts.is_empty());
        let new = input(&scan(&mut e, 11));
        assert_ne!(new.id, last.id);
        no_input(&ack(&mut e, &last, 12));
        assert_eq!(e.snapshot().pending_action_id, Some(new.id));
        assert!(e.snapshot().attempted_ids.is_empty());
    }
    println!("PASS 5 Stop cases: queued seat/completion/Esc and delivered completion stay stopped, stale ACK cannot revive them or count toward a new run");
}
fn main() {
    pause_queued_seat_matrix();
    pause_escape_matrix();
    stop_matrix();
    println!("13/13 production-component interruption scenarios passed; OS queue/drivers are outside this harness");
}
