#[derive(Clone)]
struct RestartContext {
    engine: Engine,
    target_id: Option<u64>,
    geometry: Option<FrameReference>,
    run_id: u64,
    started_at: Instant,
    count_receipt: Option<(u64, u64, u64)>,
}

fn same_search_mapping(previous: &Engine, next: &Profile, mode: RunMode) -> bool {
    let region = if mode == RunMode::Cancellation {
        previous.snapshot().active_region.and_then(|index| next.legacy.regions.get(index))
    } else { None };
    same_candidate_interpretation(previous, next, region)
        && (mode != RunMode::Cancellation || previous.snapshot().active_region.is_none()
            || serde_json::to_value(&previous.profile().legacy.cancellation_navigation).ok()
                == serde_json::to_value(&next.legacy.cancellation_navigation).ok())
}

fn same_candidate_interpretation(previous: &Engine, next: &Profile,
    region: Option<&macro_core::LegacyRegionSettings>) -> bool {
    let vision = region.and_then(|region| region.vision.as_ref()).unwrap_or(&next.vision);
    let color = region.and_then(|region| region.color).unwrap_or(next.color);
    serde_json::to_value(previous.effective_vision()).ok() == serde_json::to_value(vision).ok()
        && previous.effective_color() == color && previous.profile().color_tolerance == next.color_tolerance
        && previous.profile().features.nol_selection_count == next.features.nol_selection_count
        && previous.profile().features.seat_objects == next.features.seat_objects
}

fn same_post_entry_mapping(previous: &Engine, next: &Profile) -> bool {
    // A delivered selection stays in its current cancellation region even when
    // F9 changes the run mode to Normal. Do not silently switch to global ROI.
    let region = previous.snapshot().active_region.and_then(|index| next.legacy.regions.get(index));
    same_candidate_interpretation(previous, next, region)
}

fn stopped_conflict_context(current: Option<RestartContext>, previous: Option<RestartContext>,
    visual: &mut VisualRecovery) -> Option<RestartContext> {
    let context = current.or(previous).filter(|context| context.engine.has_unresolved_conflict());
    // Stopped performs no observation/input. Retain only an already detected
    // incident, never an ordinary speculative seat watch.
    if context.is_none() || !visual.is_incident() { visual.reset(); }
    context
}
