//! Synthetic pixels -> actual extractor -> actual Engine. Reproduces an ID
//! aliasing limit; the changing fixture is not a recovered real-site image.
use macro_core::{vision::{extract_candidates, RgbaFrame}, *};

fn image(inset: usize) -> RgbaFrame {
    let mut rgba = [240, 240, 240, 255].repeat(64 * 48);
    let c = Profile::default().color;
    // One site seat has a fixed hit rectangle [10,19) x [20,29).
    // Selection styling shrinks its matching-color area, not its hit target.
    for y in 20 + inset..29 - inset { for x in 10 + inset..19 - inset {
        let i = (y * 64 + x) * 4;
        rgba[i..i + 3].copy_from_slice(&[c.r, c.g, c.b]);
    }}
    RgbaFrame { width: 64, height: 48, rgba }
}
fn engine(target: usize) -> Engine {
    let mut p = Profile::default();
    p.target_seats = target;
    p.legacy.complete = Some(Target::Point { point: Point { x: 60.0, y: 40.0 } });
    let mut e = Engine::new(p).unwrap();
    e.step(Event::Start { mode: RunMode::Seats }, 0);
    e
}
fn observed(e: &mut Engine, inset: usize, now: u64) -> Vec<Action> {
    let generation = e.snapshot().generation;
    let candidates = extract_candidates(&image(inset), e.effective_color(),
        e.profile().color_tolerance, e.effective_vision(), generation).unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].point, Point { x: 14.0, y: 24.0 });
    println!("observed inset={inset} id={} point={:?}", candidates[0].id, candidates[0].point);
    e.step(Event::CandidatesObserved { generation, candidates }, now)
}
fn seat(actions: &[Action]) -> Option<Action> {
    actions.iter().find(|a| matches!(&a.kind, ActionKind::Click { purpose, .. } if purpose.starts_with("seat:"))).cloned()
}
fn ack(e: &mut Engine, a: &Action, now: u64) -> Vec<Action> {
    e.step(Event::ActionAcknowledged { id: a.id, success: true, detail: String::new() }, now)
}
fn same_id_control(target: usize) {
    let mut e = engine(target);
    let first = seat(&observed(&mut e, 0, 1)).unwrap();
    ack(&mut e, &first, 2);
    for tick in 3..10 {
        assert!(seat(&observed(&mut e, 0, tick)).is_none());
        assert_eq!(e.snapshot().phase, Phase::Seats);
        assert_eq!(e.snapshot().attempted_ids.len(), 1);
    }
    println!("PASS target={target}: unchanged component ID is never clicked or counted twice");
}
fn changed_id_reproduction(target: usize) {
    let mut e = engine(target);
    let mut actual_selected = false;
    let mut points = vec![];
    let mut completion = false;
    for index in 0..target {
        let a = seat(&observed(&mut e, index, index as u64 * 2 + 1)).unwrap();
        let ActionKind::Click { target: Target::Point { point }, .. } = &a.kind else { unreachable!() };
        assert!(Rect { x: 10.0, y: 20.0, width: 9.0, height: 9.0 }.contains(*point));
        points.push(*point);
        // Independent site's hit-test toggles the same seat. No result or
        // selected-count event is sent to the engine, only delivery ACK.
        actual_selected = !actual_selected;
        let next = ack(&mut e, &a, index as u64 * 2 + 2);
        completion = next.iter().any(|a| matches!(a.kind, ActionKind::CompleteSelection { .. }));
        assert_eq!(e.snapshot().attempted_ids.len(), index + 1);
    }
    assert!(points.iter().all(|p| *p == points[0]));
    assert_eq!(e.snapshot().phase, Phase::Completing);
    assert!(completion);
    assert_eq!(e.snapshot().delivered_attempts.len(), target);
    assert_eq!(usize::from(actual_selected), target % 2);
    println!("REPRODUCED target={target}: {target} distinct component IDs count {target} attempts at identical point {:?}; site selected={}, completion=true",
        points[0], usize::from(actual_selected));
}
fn cancellation_ack_control() {
    let mut e = engine(3);
    let unsent = seat(&observed(&mut e, 0, 1)).unwrap();
    e.step(Event::ActionCancelled { id: unsent.id }, 2);
    assert!(ack(&mut e, &unsent, 3).is_empty());
    assert!(e.snapshot().attempted_ids.is_empty());
    let first = seat(&observed(&mut e, 0, 4)).unwrap();
    assert_ne!(first.id, unsent.id);
    ack(&mut e, &first, 5);
    assert_eq!(e.snapshot().attempted_ids.len(), 1);
    assert!(ack(&mut e, &first, 6).is_empty());
    assert_eq!(e.snapshot().attempted_ids.len(), 1);
    println!("PASS duplicate/stale ACK control: IDs prevent duplicate ACK counting; the alias is newly generated candidate identity, not repeated ACK handling");
}
fn main() {
    for target in [2, 3] { same_id_control(target); changed_id_reproduction(target); }
    cancellation_ack_control();
    println!("5/5 component-level scenarios completed; 2 are reproductions, not fixed behavior; no OS/browser/site request executed");
}
