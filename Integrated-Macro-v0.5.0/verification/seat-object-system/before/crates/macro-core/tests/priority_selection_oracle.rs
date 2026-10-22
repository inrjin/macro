//! P6 audit: an independent local site's selection truth is never supplied to
//! the engine. Synthetic pixels pass through the real extractor and Engine.
//! These characterize the chosen optimistic policy, not browser/OS execution.
use macro_core::{vision::{extract_candidates, RgbaFrame}, *};
use std::collections::{BTreeSet, VecDeque};

#[derive(Clone, Copy, Debug)]
enum SitePolicy { Accept, IgnoreFirst, ClearEarlierOnSecond }

struct Site {
    policy: SitePolicy,
    selected: BTreeSet<usize>,
    clicks: usize,
}

impl Site {
    fn frame(&self, color: Color) -> RgbaFrame {
        let mut rgba = vec![240; 100 * 60 * 4];
        for pixel in rgba.chunks_exact_mut(4) { pixel[3] = 255; }
        for seat in 0..3 {
            let color = if self.selected.contains(&seat) { Color { r: 70, g: 70, b: 70 } } else { color };
            for y in 20..25 {
                for x in (10 + seat * 25)..(15 + seat * 25) {
                    let offset = (y * 100 + x) * 4;
                    rgba[offset..offset + 4].copy_from_slice(&[color.r, color.g, color.b, 255]);
                }
            }
        }
        RgbaFrame { width: 100, height: 60, rgba }
    }

    // Hit-testing uses site rectangles, not the candidate's ID or engine count.
    fn receive_click(&mut self, point: Point) {
        let seat = (0..3).find(|seat| Rect {
            x: (10 + seat * 25) as f64, y: 20.0, width: 5.0, height: 5.0,
        }.contains(point)).expect("engine click should hit a rendered seat");
        self.clicks += 1;
        if matches!(self.policy, SitePolicy::IgnoreFirst) && self.clicks == 1 { return; }
        if matches!(self.policy, SitePolicy::ClearEarlierOnSecond) && self.clicks == 2 {
            self.selected.clear();
        }
        if !self.selected.insert(seat) { self.selected.remove(&seat); }
    }
}

fn run(policy: SitePolicy) -> (Site, Snapshot) {
    let mut profile = Profile::default();
    profile.target_seats = 2;
    profile.features.conflict_recovery = true;
    profile.legacy.cancellation_click_delay_ms = 1;
    profile.legacy.complete = Some(Target::Point { point: Point { x: 90.0, y: 50.0 } });
    profile.vision.roi = Some(Rect { x: 0.0, y: 0.0, width: 100.0, height: 60.0 });
    let mut site = Site { policy, selected: BTreeSet::new(), clicks: 0 };
    let mut engine = Engine::new(profile.clone()).unwrap();
    let mut queue: VecDeque<_> = engine.step(Event::Start { mode: RunMode::Cancellation }, 0).into();
    for now in 1..100 {
        if let Some(action) = queue.pop_front() {
            match action.kind {
                ActionKind::Observe { .. } => {
                    let frame = site.frame(profile.color);
                    let candidates = extract_candidates(&frame, profile.color,
                        profile.color_tolerance, &profile.vision, engine.snapshot().generation).unwrap();
                    queue.extend(engine.step(Event::CandidatesObserved {
                        generation: engine.snapshot().generation, candidates,
                    }, now));
                }
                ActionKind::Click { target: Target::Point { point }, purpose } if purpose.starts_with("seat:") => {
                    site.receive_click(point);
                    // A successful OS delivery says nothing about site acceptance.
                    queue.extend(engine.step(Event::ActionAcknowledged {
                        id: action.id, success: true, detail: String::new(),
                    }, now));
                }
                ActionKind::CompleteSelection { .. } => {
                    assert_eq!(engine.snapshot().phase, Phase::Completing);
                    println!("P6 {policy:?}: requested=2 estimated={} site={} clicks={} completion_emitted=true",
                        engine.snapshot().attempted_ids.len(), site.selected.len(), site.clicks);
                    return (site, engine.snapshot());
                }
                other => panic!("unexpected site action: {other:?}"),
            }
        }
        queue.extend(engine.step(Event::Tick, now));
    }
    panic!("completion was not emitted within the deterministic scenario");
}

#[test]
fn p6_control_accepted_clicks_match_independent_site_selection() {
    let (site, state) = run(SitePolicy::Accept);
    assert_eq!(site.selected.len(), 2);
    assert_eq!(state.attempted_ids.len(), 2);
    assert!(state.selected_ids.is_empty()); // no server confirmation was injected
}

#[test]
fn p6_counterexample_silent_ignored_click_can_complete_with_one_actual_seat() {
    let (site, state) = run(SitePolicy::IgnoreFirst);
    assert_eq!(site.selected.len(), 1);
    assert_eq!(state.attempted_ids.len(), 2);
    assert!(state.selected_ids.is_empty());
}

#[test]
fn p6_counterexample_site_clearing_earlier_selection_is_not_observed_by_optimistic_count() {
    let (site, state) = run(SitePolicy::ClearEarlierOnSecond);
    assert_eq!(site.selected.len(), 1);
    assert_eq!(state.attempted_ids.len(), 2);
    assert!(state.selected_ids.is_empty());
}
