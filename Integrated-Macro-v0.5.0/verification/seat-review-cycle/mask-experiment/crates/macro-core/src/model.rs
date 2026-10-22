use serde::{Deserialize, Serialize};
use std::fmt;

pub const PROFILE_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.x && p.y >= self.y && p.x < self.x + self.width && p.y < self.y + self.height
    }
    pub fn valid(&self) -> bool {
        [self.x, self.y, self.width, self.height].iter().all(|v| v.is_finite())
            && self.width > 0.0 && self.height > 0.0
    }
}

/// Coordinates are physical pixels relative to the captured browser content, never global.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Target {
    Point { point: Point },
    Element { id: String },
}

impl Target {
    pub fn valid(&self) -> bool {
        match self {
            Self::Point { point } => point.x.is_finite() && point.y.is_finite() && point.x >= 0.0 && point.y >= 0.0,
            Self::Element { id } => !id.trim().is_empty(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Features {
    pub server_clock: bool,
    pub auto_entry: bool,
    pub auto_captcha_resume: bool,
    /// Wait for the observed NOL CAPTCHA panel to disappear before searching.
    pub nol_captcha_detection: bool,
    pub region_priority: bool,
    pub seat_priority: bool,
    /// Prefer a same-row group; fall back to separate seats without waiting.
    pub adjacent_seats: bool,
    pub conflict_recovery: bool,
    pub automatic_refresh: bool,
    pub status_overlay: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunMode {
    #[default]
    Normal,
    Cancellation,
    /// Start on an already open seat map using the global seat settings.
    Seats,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CancellationNavigation {
    #[default]
    None,
    TwoRegions { first: Target, second: Target },
    ArrowKeys,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct LegacySettings {
    pub date: Option<Target>,
    pub session: Option<Target>,
    pub next: Option<Target>,
    pub complete: Option<Target>,
    pub cancellation_navigation: CancellationNavigation,
    pub normal_click_delay_ms: u64,
    pub cancellation_click_delay_ms: u64,
    pub region_delay_ms: u64,
    /// Original click-only behavior. Input delivery counts attempts, never confirmed seats.
    /// Legacy conflict recovery requires positive selection evidence; continuous
    /// search instead keeps a separate estimate of delivered attempts.
    pub blind_click_sequence: bool,
    /// Original F2 scan settings, indexed by the two navigation regions.
    pub regions: Vec<LegacyRegionSettings>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LegacyRegionSettings {
    pub color: Option<Color>,
    pub vision: Option<VisionConfig>,
    pub complete: Option<Target>,
}

impl Default for LegacySettings {
    fn default() -> Self {
        Self {
            date: None, session: None, next: None, complete: None,
            cancellation_navigation: CancellationNavigation::None,
            normal_click_delay_ms: 0, cancellation_click_delay_ms: 0,
            region_delay_ms: 0, blind_click_sequence: true,
            regions: vec![],
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub schema_version: u32,
    pub name: String,
    pub target_seats: usize,
    pub color: Color,
    /// None uses the seat color; stage maps may use a different preferred color.
    pub region_color: Option<Color>,
    /// Maximum per-channel RGB difference, normalized across both OSes.
    pub color_tolerance: u8,
    pub features: Features,
    pub legacy: LegacySettings,
    pub desired_date: Option<String>,
    pub desired_session: Option<String>,
    pub server_url: Option<String>,
    /// Optional visible booking button clicked once at normal/scheduled run start.
    /// Its delivery never implies that the future seat screen is ready.
    pub entry_target: Option<Target>,
    /// Runtime converts an estimated wall-clock sale time into this monotonic deadline.
    pub sale_deadline_ms: Option<u64>,
    pub refresh_target: Option<Target>,
    /// Legacy configuration hint; repeated observation does not prove a site refresh.
    pub site_auto_refresh: bool,
    /// Empty-map observation cadence; also the delay for explicitly enabled refresh.
    pub refresh_interval_ms: u64,
    pub action_timeout_ms: u64,
    pub observation_timeout_ms: u64,
    pub vision: VisionConfig,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            schema_version: PROFILE_SCHEMA_VERSION,
            name: "기본 설정".into(),
            target_seats: 1,
            color: Color { r: 130, g: 70, b: 210 },
            region_color: None,
            color_tolerance: 12,
            features: Features::default(),
            legacy: LegacySettings::default(),
            desired_date: None,
            desired_session: None,
            server_url: None,
            entry_target: None,
            sale_deadline_ms: None,
            refresh_target: None,
            site_auto_refresh: false,
            refresh_interval_ms: 1000,
            action_timeout_ms: 2000,
            observation_timeout_ms: 5000,
            vision: VisionConfig::default(),
        }
    }
}

impl Profile {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.schema_version != PROFILE_SCHEMA_VERSION {
            return Err(ValidationError("지원하지 않는 설정 파일 버전입니다".into()));
        }
        if !(1..=4).contains(&self.target_seats) {
            return Err(ValidationError("좌석 선택 수는 1~4석이어야 합니다".into()));
        }
        if self.action_timeout_ms == 0 || self.observation_timeout_ms == 0 {
            return Err(ValidationError("응답 대기 시간은 0보다 커야 합니다".into()));
        }
        if self.legacy.regions.len() > 2 {
            return Err(ValidationError("취소표 구역별 설정은 최대 두 개까지 지정할 수 있습니다".into()));
        }
        for vision in std::iter::once(&self.vision).chain(self.legacy.regions.iter().filter_map(|r| r.vision.as_ref())) {
            if vision.min_component_pixels == 0 || vision.max_component_pixels < vision.min_component_pixels
                || vision.roi.is_some_and(|r| !r.valid()) || vision.exclusions.iter().any(|r| !r.valid())
            {
                return Err(ValidationError("좌석 탐색 영역 또는 색상 영역 크기 설정이 올바르지 않습니다".into()));
            }
        }
        let global_targets = [&self.entry_target, &self.legacy.date, &self.legacy.session, &self.legacy.next, &self.legacy.complete, &self.refresh_target];
        for target in global_targets.into_iter().flatten().chain(self.legacy.regions.iter().filter_map(|r| r.complete.as_ref())) {
            if !target.valid() { return Err(ValidationError("지정한 좌표 또는 버튼이 올바르지 않습니다".into())); }
        }
        if let CancellationNavigation::TwoRegions { first, second } = &self.legacy.cancellation_navigation {
            if !first.valid() || !second.valid() {
                return Err(ValidationError("취소표 구역 좌표가 올바르지 않습니다".into()));
            }
        }
        Ok(())
    }

    pub fn from_json(json: &str) -> Result<Self, ValidationError> {
        let profile: Self = serde_json::from_str(json).map_err(|e| ValidationError(e.to_string()))?;
        profile.validate()?;
        Ok(profile)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationError(pub String);
impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(&self.0) }
}
impl std::error::Error for ValidationError {}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct VisionConfig {
    pub roi: Option<Rect>,
    /// Legend, status overlay and other known non-map areas.
    pub exclusions: Vec<Rect>,
    pub min_component_pixels: usize,
    pub max_component_pixels: usize,
}
impl Default for VisionConfig {
    fn default() -> Self {
        Self { roi: None, exclusions: vec![], min_component_pixels: 3, max_component_pixels: 100_000 }
    }
}

/// Original matching pixels, not the complete outline or identity of a site seat.
/// Immutable row runs are shared by observations and delivered-seat marks. Only
/// the extractor constructs them; they never enter profile/IPC JSON.
#[derive(Clone, PartialEq, Eq)]
pub struct ComponentMask {
    runs: Vec<PixelRun>,
    pixels: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct PixelRun { y: u32, start: u32, end: u32 }

impl fmt::Debug for ComponentMask {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ComponentMask").field("pixels", &self.pixels)
            .field("row_runs", &self.runs.len()).finish()
    }
}

impl ComponentMask {
    /// Called only after the extractor has accepted a nonempty component under
    /// its pixel limit. Reuse its pixel buffer rather than allocating a bitmap
    /// for a potentially large, mostly empty bounding rectangle.
    pub(crate) fn from_roi_pixels(pixels: &mut [usize], width: usize, origin_x: usize, origin_y: usize) -> Self {
        pixels.sort_unstable();
        let mut runs: Vec<PixelRun> = Vec::new();
        for &index in pixels.iter() {
            let x = (origin_x + index % width) as u32;
            let y = (origin_y + index / width) as u32;
            if let Some(last) = runs.last_mut().filter(|last| last.y == y && last.end == x) {
                last.end += 1;
            } else { runs.push(PixelRun { y, start: x, end: x + 1 }); }
        }
        Self { runs, pixels: pixels.len() }
    }

    /// A shrink or split fragment retains most of its pixels from the original
    /// component. Bounding-box containment alone is deliberately insufficient.
    pub(crate) fn substantially_overlaps(&self, other: &Self) -> bool {
        let smaller = self.pixels.min(other.pixels);
        if smaller == 0 { return false; }
        let needed = smaller - smaller / 4; // ceil(75%), without multiplication overflow
        let (mut a, mut b, mut shared) = (0, 0, 0);
        while a < self.runs.len() && b < other.runs.len() {
            let left = self.runs[a];
            let right = other.runs[b];
            if left.y < right.y || left.y == right.y && left.end <= right.start { a += 1; continue; }
            if right.y < left.y || left.y == right.y && right.end <= left.start { b += 1; continue; }
            shared += (left.end.min(right.end) - left.start.max(right.start)) as usize;
            if shared >= needed { return true; }
            if left.end <= right.end { a += 1; } else { b += 1; }
        }
        false
    }
}

#[cfg(test)]
mod component_mask_tests {
    use super::ComponentMask;
    use std::collections::HashSet;

    #[test]
    fn intersection_threshold_rounds_up_and_is_symmetric() {
        for size in 1..=8 {
            for shared in 0..=size {
                let mut original: Vec<usize> = (0..size).rev().collect();
                let mut other: Vec<usize> = (size - shared..2 * size - shared).collect();
                let a = ComponentMask::from_roi_pixels(&mut original, 32, 0, 0);
                let b = ComponentMask::from_roi_pixels(&mut other, 32, 0, 0);
                let expected = 4 * shared >= 3 * size;
                assert_eq!(a.substantially_overlaps(&b), expected, "size={size}, shared={shared}");
                assert_eq!(b.substantially_overlaps(&a), expected);
            }
        }
    }

    #[test]
    fn row_run_intersection_matches_independent_pixel_sets_across_roi_origins() {
        let mut outcomes = [0, 0];
        for seed in 0..256 {
            let mut a: Vec<usize> = (0..64).filter(|i| (i * 17 + seed * 13) % 11 < 6).rev().collect();
            let mut b: Vec<usize> = if seed % 2 == 0 {
                // The same absolute pixels expressed using a different ROI
                // stride/origin must also exercise the positive multirow path.
                a.iter().map(|i| (i % 8 + 1) + (i / 8 + 1) * 10).collect()
            } else { (0..80).filter(|i| (i * 7 + seed * 3) % 9 < 5).rev().collect() };
            let a_pixels: HashSet<_> = a.iter().map(|i| (i % 8 + 3, i / 8 + 2)).collect();
            let b_pixels: HashSet<_> = b.iter().map(|i| (i % 10 + 2, i / 10 + 1)).collect();
            let expected = a_pixels.intersection(&b_pixels).count() * 4
                >= a_pixels.len().min(b_pixels.len()) * 3;
            outcomes[usize::from(expected)] += 1;
            let a_mask = ComponentMask::from_roi_pixels(&mut a, 8, 3, 2);
            let b_mask = ComponentMask::from_roi_pixels(&mut b, 10, 2, 1);
            assert_eq!(a_mask.substantially_overlaps(&b_mask), expected, "seed={seed}");
            assert_eq!(b_mask.substantially_overlaps(&a_mask), expected, "seed={seed}");
        }
        assert!(outcomes.iter().all(|count| *count > 0));
        for second_row_shared in [3, 4] {
            // 8 shared pixels on row one are insufficient; row two must bring
            // the total to exactly 12/16 (75%) before the result becomes true.
            let mut a: Vec<usize> = (0..8).chain(16..24).collect();
            let mut b: Vec<usize> = (0..8).chain(16..16 + second_row_shared)
                .chain(24..32 - second_row_shared).collect();
            let a = ComponentMask::from_roi_pixels(&mut a, 16, 4, 2);
            let b = ComponentMask::from_roi_pixels(&mut b, 16, 4, 2);
            assert_eq!(a.substantially_overlaps(&b), second_row_shared == 4);
            assert_eq!(b.substantially_overlaps(&a), second_row_shared == 4);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CandidateGeometry {
    /// Bounds of the observed color component, in captured physical pixels.
    pub bounds: Rect,
    pub pixels: usize,
    /// Present only for native seat extraction. Older observations retain the
    /// bounds-only compatibility path. Ignore any untrusted serialized mask.
    #[serde(skip)]
    pub mask: Option<std::sync::Arc<ComponentMask>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Candidate {
    pub id: String,
    pub point: Point,
    pub color: Color,
    pub target: Target,
    #[serde(default)]
    pub selected: bool,
    /// Optional for older browser observations; never proves site acceptance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geometry: Option<CandidateGeometry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RegionCandidate {
    pub candidate: Candidate,
    /// Supplied from reliable map semantics/reference; 0 means stage-center group.
    pub center_rank: u32,
    pub distance_to_stage: f64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryStage {
    ProductBooking,
    SaleGate,
    Calendar,
    Session,
    ConfirmBooking,
    Captcha,
    Queue,
    SeatMap,
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeatOutcome {
    Selected,
    /// Positive UI selection marker only; completion still needs its own confirmation.
    LocallySelected,
    Conflict,
    Unknown,
}

/// A seat input acknowledged as delivered, not a confirmed reservation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveredSeatAttempt {
    pub action_id: u64,
    pub candidate_id: String,
    pub generation: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    Start { mode: RunMode },
    Pause,
    Resume,
    Stop,
    Tick,
    /// Adapter proof that a queued input was never delivered; not an input failure/timeout.
    ActionCancelled { id: u64 },
    ActionAcknowledged { id: u64, success: bool, #[serde(default)] detail: String },
    EntryObserved {
        stage: EntryStage,
        #[serde(default)] target: Option<Target>,
        #[serde(default)] enabled: bool,
        #[serde(default)] matched_value: Option<String>,
        #[serde(default)] screen_id: String,
    },
    CaptchaCompleted,
    /// Explicit user confirmation releases only the current CAPTCHA gate.
    /// It cannot skip registered entry inputs or restart an ongoing seat search.
    CaptchaManuallyCompleted,
    /// A real browser Enter submission restarts the optional normal booking search.
    /// This is a user trigger, not evidence that the CAPTCHA was accepted.
    CaptchaSubmitted,
    /// The armed NOL panel detector observed disappearance. This is a visual
    /// search trigger, not verification that the server accepted the CAPTCHA.
    NolCaptchaDismissed,
    RegionsObserved { candidates: Vec<RegionCandidate> },
    /// Region navigation completed and the actual seat map is now positively identified.
    /// Explicit evidence that the region's seat map is visible; never a timeout guess.
    RegionMapReady,
    /// Generation advances for explicit refresh evidence or a post-CAPTCHA
    /// search, never for every screenshot or a timed retry of failed candidates.
    CandidatesObserved { generation: u64, candidates: Vec<Candidate> },
    SeatResult { candidate_id: String, result: SeatOutcome },
    /// Correlates an outcome with one delivered input, rejecting old retries/runs.
    DeliveredSeatResult { action_id: u64, candidate_id: String, result: SeatOutcome },
    ConflictDialogGone,
    /// Legacy modes reconcile selected IDs; continuous search only promotes known delivered IDs.
    SelectionObserved { selected_ids: Vec<String> },
    RefreshCompleted { generation: u64 },
    /// Supplied only after the current seat screen exposes the completion control.
    CompletionTargetObserved { target: Target },
    CompletionConfirmed,
    ContextLost { reason: String },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Key { Escape, ArrowUp, ArrowDown }

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ActionKind {
    Click { target: Target, purpose: String },
    Key { key: Key, purpose: String },
    Observe { scope: String },
    Refresh { target: Target },
    CompleteSelection { target: Target },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Action {
    pub id: u64,
    pub kind: ActionKind,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    #[default]
    Idle,
    WaitingSale,
    Entry,
    Captcha,
    Region,
    WaitingRegion,
    Seats,
    Conflict,
    WaitingRefresh,
    Completing,
    Paused,
    Completed,
    Stopped,
    Faulted,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub phase: Phase,
    pub mode: RunMode,
    pub message: String,
    pub selected_ids: Vec<String>,
    pub attempted_ids: Vec<String>,
    pub rejected_ids: Vec<String>,
    pub target_seats: usize,
    pub generation: u64,
    pub pending_action_id: Option<u64>,
    pub current_candidate_id: Option<String>,
    pub active_region: Option<usize>,
    #[serde(default)]
    pub optimistic_selection: bool,
    /// Active delivered attempts in delivery order, including positively selected ones.
    #[serde(default)]
    pub delivered_attempts: Vec<DeliveredSeatAttempt>,
    #[serde(default)]
    pub conflict_attempt: Option<DeliveredSeatAttempt>,
}
