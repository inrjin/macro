//! Independent site rectangles and selection state; only rendered pixels and
//! OS-delivery ACKs reach the real extractor/engine. No site truth is injected.
use macro_core::{vision::{extract_seat_candidates_view_with_stats, seat_point_supported, RgbaFrame, RgbaView}, *};
use std::collections::{BTreeSet, VecDeque};

const COLOR: Color = Color { r: 185, g: 224, b: 180 };

struct Site {
    seats: Vec<Rect>,
    selected: BTreeSet<usize>,
    clicks: Vec<usize>,
    invalid_clicks: usize,
    residue: bool,
}

impl Site {
    fn frame(&self) -> RgbaFrame {
        let mut frame = RgbaFrame { width: 100, height: 70, rgba: [240, 240, 240, 255].repeat(100 * 70) };
        for (index, rect) in self.seats.iter().enumerate() {
            // A selected seat keeps a smaller patch of the ORIGINAL color.
            // Its component ID changes; clicking it again toggles it off.
            let inset = usize::from(self.selected.contains(&index));
            for y in rect.y as usize + inset..(rect.y + rect.height) as usize - inset {
                for x in rect.x as usize + inset..(rect.x + rect.width) as usize - inset {
                    frame.rgba[(y * 100 + x) * 4..(y * 100 + x) * 4 + 4]
                        .copy_from_slice(&[COLOR.r, COLOR.g, COLOR.b, 255]);
                }
            }
        }
        if self.residue {
            // An inactive decoration has a 1x3 matching edge. It is NOT a seat.
            for y in 1..4 { frame.rgba[(y * 100 + 80) * 4..(y * 100 + 80) * 4 + 4]
                .copy_from_slice(&[COLOR.r, COLOR.g, COLOR.b, 255]); }
        }
        frame
    }

    fn click(&mut self, point: Point) {
        if let Some(index) = self.seats.iter().position(|rect| rect.contains(point)) {
            self.clicks.push(index);
            if !self.selected.insert(index) { self.selected.remove(&index); }
        } else { self.invalid_clicks += 1; }
    }
}

fn view(frame: &RgbaFrame) -> RgbaView<'_> {
    RgbaView { width: frame.width, height: frame.height, stride: frame.width as usize * 4, rgba: &frame.rgba }
}

fn run(points: &[(f64, f64)], target: usize, adjacent: bool, residue: bool) -> (Site, Snapshot, bool) {
    let mut site = Site { seats: points.iter().map(|&(x,y)| Rect { x, y, width: 9.0, height: 9.0 }).collect(),
        selected: BTreeSet::new(), clicks: vec![], invalid_clicks: 0, residue };
    let mut profile = Profile::default();
    profile.color = COLOR;
    profile.target_seats = target;
    profile.features.adjacent_seats = adjacent;
    profile.legacy.normal_click_delay_ms = 1;
    profile.legacy.complete = Some(Target::Point { point: Point { x: 95.0, y: 65.0 } });
    let mut engine = Engine::new(profile.clone()).unwrap();
    let mut queue: VecDeque<_> = engine.step(Event::Start { mode: RunMode::Seats }, 0).into();
    for now in 1..150 {
        let frame = site.frame();
        let generation = engine.snapshot().generation;
        let candidates = extract_seat_candidates_view_with_stats(view(&frame), COLOR, 12, &profile.vision, generation)
            .unwrap().candidates;
        queue.extend(engine.step(Event::CandidatesObserved { generation, candidates }, now));
        queue.extend(engine.step(Event::Tick, now));
        while let Some(action) = queue.pop_front() {
            if matches!(action.kind, ActionKind::Observe { .. }) { continue; }
            if engine.snapshot().pending_action_id != Some(action.id) { continue; }
            match action.kind {
                ActionKind::Click { target: Target::Point { point }, purpose } if purpose.starts_with("seat:") => {
                    let latest = site.frame();
                    if !seat_point_supported(view(&latest), point, COLOR, 12, &profile.vision) {
                        queue.extend(engine.step(Event::ActionCancelled { id: action.id }, now));
                        continue;
                    }
                    site.click(point);
                    queue.extend(engine.step(Event::ActionAcknowledged { id: action.id, success: true, detail: String::new() }, now));
                }
                ActionKind::CompleteSelection { .. } => return (site, engine.snapshot(), true),
                other => panic!("unexpected action: {other:?}"),
            }
        }
    }
    (site, engine.snapshot(), false)
}

#[test]
fn selected_seat_repaint_cannot_fill_two_or_three_requested_seats_with_one_physical_seat() {
    for target in [2, 3] { for adjacent in [false, true] {
        let (site, state, completed) = run(&[(10.0, 20.0)], target, adjacent, false);
        assert_eq!(site.clicks, vec![0]);
        assert_eq!(site.selected.len(), 1);
        assert_eq!(state.attempted_ids.len(), 1);
        assert!(!completed);
        assert_eq!(state.phase, Phase::Seats, "keep searching for the missing seats");
    }}
}

#[test]
fn two_and_three_actual_seats_are_selected_despite_repaint_and_inactive_color_residue() {
    for target in [2, 3] { for adjacent in [false, true] {
        let (site, state, completed) = run(&[(10.0, 20.0), (30.0, 20.0), (50.0, 20.0)], target, adjacent, true);
        assert!(completed);
        assert_eq!(site.selected.len(), target);
        assert_eq!(site.clicks.len(), target);
        assert_eq!(site.invalid_clicks, 0);
        assert_eq!(state.attempted_ids.len(), target);
    }}
}

#[test]
fn actual_pixels_prefer_lower_adjacent_groups_then_fall_back_to_separate_seats() {
    for target in [2, 3] {
        let mut seats = vec![(5.0, 5.0), (30.0, 25.0), (43.0, 25.0)];
        if target == 3 { seats.push((56.0, 25.0)); }
        let (site, _, completed) = run(&seats, target, true, true);
        assert!(completed);
        assert_eq!(site.selected, (1..=target).collect());
        assert_eq!(site.invalid_clicks, 0);
        let (without, _, _) = run(&seats, target, false, true);
        assert_eq!(without.clicks[0], 0, "OFF keeps the original top-first candidate order");
    }
    let (site, _, completed) = run(&[(5.0, 5.0), (30.0, 25.0), (43.0, 25.0)], 3, true, true);
    assert!(completed);
    assert_eq!(site.clicks, vec![1, 2, 0], "take the pair, then fill with the separate seat");
    assert_eq!(site.selected.len(), 3);
}
