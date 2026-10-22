//! Independent rendered-object truth -> real vision -> real Engine actions.
//! ACKs model delivered inputs, not site success. No browser/OS behavior is tested.
use macro_core::{vision::{extract_seat_candidates_view_with_stats, seat_point_supported, RgbaFrame, RgbaView}, *};
use std::collections::{BTreeSet, VecDeque};

const WIDTH: usize = 96;
const HEIGHT: usize = 64;
const COLOR: Color = Color { r: 185, g: 224, b: 180 };
type Pixel = (usize, usize);

fn rectangle(x: usize, y: usize, width: usize, height: usize) -> Vec<Pixel> {
    (y..y + height).flat_map(|y| (x..x + width).map(move |x| (x, y))).collect()
}

#[derive(Clone)]
struct Object {
    available: Vec<Pixel>,
    selected: Vec<Pixel>,
}

impl Object {
    fn unchanged(pixels: Vec<Pixel>) -> Self { Self { selected: pixels.clone(), available: pixels } }
    fn contains(&self, point: Point) -> bool {
        let pixel = (point.x as usize, point.y as usize);
        self.available.contains(&pixel) || self.selected.contains(&pixel)
    }
}

struct Site {
    objects: Vec<Object>,
    selected: BTreeSet<usize>,
    clicks: Vec<(u64, usize)>,
    fail_once: Option<usize>,
    invalid_clicks: usize,
}

impl Site {
    fn frame(&self) -> RgbaFrame {
        let mut frame = RgbaFrame { width: WIDTH as u32, height: HEIGHT as u32,
            rgba: [240, 240, 240, 255].repeat(WIDTH * HEIGHT) };
        for (index, object) in self.objects.iter().enumerate() {
            let pixels = if self.selected.contains(&index) { &object.selected } else { &object.available };
            for &(x, y) in pixels {
                let offset = (y * WIDTH + x) * 4;
                frame.rgba[offset..offset + 4].copy_from_slice(&[COLOR.r, COLOR.g, COLOR.b, 255]);
            }
        }
        frame
    }

    fn click(&mut self, point: Point, now: u64) -> bool {
        let Some(index) = self.objects.iter().position(|object| object.contains(point)) else {
            self.invalid_clicks += 1;
            return false;
        };
        self.clicks.push((now, index));
        if self.fail_once == Some(index) { self.fail_once = None; return false; }
        // Clicking the same physical object twice toggles its real selection off.
        if !self.selected.insert(index) { self.selected.remove(&index); }
        true
    }
}

fn view(frame: &RgbaFrame) -> RgbaView<'_> {
    RgbaView { width: frame.width, height: frame.height, stride: frame.width as usize * 4, rgba: &frame.rgba }
}

struct Outcome { site: Site, state: Snapshot, completed: bool, escapes: usize }

fn run(objects: Vec<Object>, target: usize, adjacent: bool, mode: RunMode, fail_once: Option<usize>) -> Outcome {
    let mut site = Site { objects, selected: BTreeSet::new(), clicks: vec![], fail_once, invalid_clicks: 0 };
    let mut profile = Profile::default();
    profile.color = COLOR;
    profile.target_seats = target;
    profile.features.adjacent_seats = adjacent;
    profile.features.conflict_recovery = true;
    profile.legacy.normal_click_delay_ms = 1;
    profile.legacy.cancellation_click_delay_ms = 1;
    profile.refresh_interval_ms = 30;
    profile.legacy.complete = Some(Target::Point { point: Point { x: 90.0, y: 60.0 } });
    let mut engine = Engine::new(profile.clone()).unwrap();
    let mut queue: VecDeque<_> = engine.step(Event::Start { mode }, 0).into();
    let mut escapes = 0;
    for now in 1..160 {
        let frame = site.frame();
        let generation = engine.snapshot().generation;
        let extraction = extract_seat_candidates_view_with_stats(view(&frame), COLOR, 12, &profile.vision, generation).unwrap();
        queue.extend(engine.step(Event::CandidatesObserved { generation, candidates: extraction.candidates }, now));
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
                    let accepted = site.click(point, now);
                    queue.extend(engine.step(Event::ActionAcknowledged { id: action.id, success: true, detail: String::new() }, now));
                    if !accepted {
                        queue.extend(engine.step(Event::DeliveredSeatResult { action_id: action.id,
                            candidate_id: purpose.strip_prefix("seat:").unwrap().into(), result: SeatOutcome::Conflict }, now));
                    }
                }
                ActionKind::Key { key: Key::Escape, .. } => {
                    escapes += 1;
                    queue.extend(engine.step(Event::ActionAcknowledged { id: action.id, success: true, detail: String::new() }, now));
                    queue.extend(engine.step(Event::ConflictDialogGone, now));
                }
                ActionKind::CompleteSelection { .. } => return Outcome { site, state: engine.snapshot(), completed: true, escapes },
                other => panic!("unexpected action: {other:?}"),
            }
        }
    }
    Outcome { site, state: engine.snapshot(), completed: false, escapes }
}

#[test]
fn one_pixel_background_gap_and_diagonal_contact_keep_actual_neighbors_clickable() {
    for pixels in [vec![rectangle(4, 4, 3, 3), rectangle(8, 4, 3, 3)],
        vec![rectangle(4, 4, 2, 2), rectangle(6, 6, 2, 2)]] {
        for adjacent in [false, true] {
            let result = run(pixels.iter().cloned().map(Object::unchanged).collect(), 2, adjacent, RunMode::Seats, None);
            assert_eq!(result.site.selected, BTreeSet::from([0, 1]));
            assert_eq!(result.site.clicks.len(), 2);
            assert!(result.completed);
        }
    }
}

#[test]
fn selected_inset_and_split_pixels_cannot_count_one_physical_object_twice() {
    let mut split = rectangle(4, 4, 3, 9);
    split.extend(rectangle(10, 4, 3, 9));
    for selected in [rectangle(5, 5, 7, 7), split] {
        for adjacent in [false, true] { for target in [2, 3] {
            let result = run(vec![Object { available: rectangle(4, 4, 9, 9), selected: selected.clone() }],
                target, adjacent, RunMode::Seats, None);
            assert_eq!(result.site.clicks, vec![(1, 0)]);
            assert_eq!(result.site.selected, BTreeSet::from([0]));
            assert_eq!(result.state.attempted_ids.len(), 1);
            assert_eq!(result.state.phase, Phase::Seats);
            assert!(!result.completed);
        }}
    }
}

#[test]
fn split_selected_object_does_not_hide_its_neighbor_across_a_one_pixel_gap() {
    let mut split = rectangle(4, 4, 3, 9);
    split.extend(rectangle(10, 4, 3, 9));
    for adjacent in [false, true] {
        let result = run(vec![Object { available: rectangle(4, 4, 9, 9), selected: split.clone() },
            Object::unchanged(rectangle(14, 4, 9, 9))], 2, adjacent, RunMode::Seats, None);
        assert_eq!(result.site.selected, BTreeSet::from([0, 1]));
        assert_eq!(result.site.clicks.len(), 2);
        assert!(result.completed);
    }
}

#[test]
fn conflict_retry_releases_only_the_failed_object_and_keeps_selected_fragments_excluded() {
    let objects = vec![Object { available: rectangle(4, 4, 9, 9), selected: rectangle(5, 5, 7, 7) },
        Object { available: rectangle(16, 4, 9, 9), selected: rectangle(17, 5, 7, 7) }];
    for adjacent in [false, true] {
        let result = run(objects.clone(), 2, adjacent, RunMode::Cancellation, Some(1));
        assert_eq!(result.site.clicks.iter().map(|(_, object)| *object).collect::<Vec<_>>(), vec![0, 1, 1]);
        assert!(result.site.clicks[2].0 - result.site.clicks[1].0 >= 30);
        assert_eq!(result.site.selected, BTreeSet::from([0, 1]));
        assert_eq!(result.escapes, 1);
        assert!(result.completed);
    }
}

#[test]
fn retained_ring_blocks_a_repainted_inner_marker_instead_of_completing_with_one_object() {
    let mut ring = rectangle(4, 4, 12, 2);
    ring.extend(rectangle(4, 14, 12, 2));
    ring.extend(rectangle(4, 6, 2, 8));
    ring.extend(rectangle(14, 6, 2, 8));
    let result = run(vec![Object { available: ring, selected: rectangle(9, 9, 2, 2) }],
        2, false, RunMode::Seats, None);
    // The mask-only experiment allowed this second click because old/new
    // color had zero overlap. Preserve the established physical-object guard.
    assert_eq!(result.site.clicks.len(), 1);
    assert_eq!(result.site.selected, BTreeSet::from([0]));
    assert_eq!(result.state.attempted_ids.len(), 1);
    assert_eq!(result.state.phase, Phase::Seats);
    assert!(!result.completed);
}

