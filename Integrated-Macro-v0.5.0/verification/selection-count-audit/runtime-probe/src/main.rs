// Audit reproducer: exact pre-change reader snapshot + the real Engine.
// A successful run confirms the old defect; this is not a product regression.
pub use macro_core::{Rect, vision};
#[path = "../../before/crates/macro-core/src/selection_count.rs"]
mod selection_count;
#[path = "../../before/crates/macro-core/src/selection_count_layout.rs"]
mod selection_count_layout;
use macro_core::{ActionKind, Engine, Event, Phase, Point, Profile, RunMode, Target};
fn main() {
    for name in ["gap32_12", "gap32_1dot5", "gap32_minus1", "gap32_1pct"] {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../implementation/fixtures").join(format!("{name}.rgba"));
        let rgba = std::fs::read(p).unwrap();
        let read = selection_count::read_selection_count(vision::RgbaView {
            width: 640, height: 160, stride: 2560, rgba: &rgba });
        let selection_count::CountRead::Confirmed { count, .. } = read else { panic!("old reader no longer reproduces"); };
        assert_eq!(count, 1);
        let mut profile = Profile::default(); profile.target_seats = 1;
        profile.features.nol_selection_count = true;
        profile.legacy.complete = Some(Target::Point { point: Point { x: 90.0, y: 90.0 } });
        let mut engine = Engine::new(profile).unwrap(); engine.step(Event::Start { mode: RunMode::Seats }, 0);
        let request = engine.selection_count_request().unwrap();
        let actions = engine.step(Event::SelectionCountObserved { request_id: request.id, observation_id: 1, count }, 2);
        assert_eq!(engine.snapshot().phase, Phase::Completing);
        assert!(actions.iter().any(|a| matches!(a.kind, ActionKind::CompleteSelection { .. })));
        println!("{name}: old reader count={count}; actual Engine queued completion for target 1");
    }
}
