use macro_core::*;

fn point(x: f64) -> Target { Target::Point { point: Point { x, y: 10.0 } } }
fn physical(actions: &[Action]) -> Vec<&Action> {
    actions.iter().filter(|action| !matches!(action.kind, ActionKind::Observe { .. })).collect()
}
fn main() {
    for navigation in [CancellationNavigation::ArrowKeys,
        CancellationNavigation::TwoRegions { first: point(100.0), second: point(200.0) }] {
        for automatic_refresh in [false, true] {
            let mut profile = Profile::default();
            profile.legacy.cancellation_navigation = navigation.clone();
            profile.features.automatic_refresh = automatic_refresh;
            // Runtime removes an internal-button target for these navigation modes,
            // but preserves the saved feature flag from the previous mode.
            profile.refresh_target = None;
            profile.legacy.region_delay_ms = 10;
            profile.refresh_interval_ms = 100;
            let mut engine = Engine::new(profile).unwrap();
            let first = engine.step(Event::Start { mode: RunMode::Cancellation }, 0);
            let first_input = physical(&first)[0];
            engine.step(Event::ActionAcknowledged { id: first_input.id, success: true, detail: String::new() }, 1);
            let after_delay = engine.step(Event::Tick, 11);
            let state = engine.snapshot();
            println!("navigation={navigation:?} refreshFlag={automatic_refresh} afterDelay={:?} generation={} observations={} message={}",
                state.phase, state.generation, after_delay.len(), state.message);
            assert_eq!(state.phase, if automatic_refresh { Phase::WaitingRefresh } else { Phase::Seats });
            if automatic_refresh {
                // Even manual confirmation of the first switch cannot restore
                // automatic alternation: the next empty scan takes the button branch.
                engine.step(Event::RefreshCompleted { generation: 1 }, 12);
            }
            let generation = engine.snapshot().generation;
            engine.step(Event::CandidatesObserved { generation, candidates: vec![] }, 13);
            let next = engine.step(Event::Tick, 113);
            println!("nextPhysical={} phase={:?} message={}", physical(&next).len(), engine.snapshot().phase, engine.snapshot().message);
            assert_eq!(physical(&next).len(), usize::from(!automatic_refresh));
        }
    }
    for navigation in [CancellationNavigation::ArrowKeys,
        CancellationNavigation::TwoRegions { first: point(100.0), second: point(200.0) }] {
        let mut profile = Profile::default();
        profile.legacy.cancellation_navigation = navigation.clone();
        profile.features.region_priority = true;
        let mut engine = Engine::new(profile).unwrap();
        let started = engine.step(Event::Start { mode: RunMode::Cancellation }, 0);
        println!("navigation={navigation:?} normalRegionPriority=true phase={:?} physical={}",
            engine.snapshot().phase, physical(&started).len());
        assert_eq!(engine.snapshot().phase, Phase::Region);
        assert!(physical(&started).is_empty());
    }
    println!("REPRODUCED: stale internal-refresh flag changes both legacy navigation modes; no OS input or app execution.");
}
