use serde::{Deserialize, Serialize};
use std::{fmt, sync::Arc};

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
    /// Keep the initial booking click; enter date/session/next manually.
    pub manual_entry: bool,
    pub auto_captcha_resume: bool,
    /// Wait for the observed NOL CAPTCHA panel to disappear before searching.
    pub nol_captcha_detection: bool,
    pub region_priority: bool,
    pub seat_priority: bool,
    /// Prefer a same-row group; fall back to separate seats without waiting.
    pub adjacent_seats: bool,
    /// Read the NOL selection count before further seat input or completion.
    pub nol_selection_count: bool,
    /// Use repeated round outlines instead of available-color fragments.
    pub seat_objects: bool,
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

/// Process-local expected silhouette learned from reference foreground. This
/// is visual evidence, not a server seat identity. Reference holes are never
/// filled; state marks missing inside a fitted outline remain validation input.
#[derive(Clone, PartialEq)]
pub struct SeatShape {
    width: u32,
    height: u32,
    occupancy: Vec<u64>,
    pixels: usize,
    fingerprint: u64,
}
impl fmt::Debug for SeatShape {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SeatShape").field("width", &self.width).field("height", &self.height)
            .field("pixels", &self.pixels).field("fingerprint", &self.fingerprint).finish()
    }
}
impl SeatShape {
    /// Bounded private storage prevents malformed imported evidence. JSON
    /// deliberately omits this proof; an external observation must be re-read.
    pub fn from_occupancy(width: u32, height: u32, occupied: &[bool]) -> Option<Self> {
        if width == 0 || height == 0 || width > 256 || height > 256
            || occupied.len() != width as usize * height as usize { return None; }
        let pixels = occupied.iter().filter(|&&v| v).count();
        if pixels == 0 { return None; }
        let mut occupancy = vec![0_u64; occupied.len().div_ceil(64)];
        for (index, &value) in occupied.iter().enumerate() {
            if value { occupancy[index / 64] |= 1_u64 << (index % 64); }
        }
        let mut fingerprint = 14695981039346656037_u64;
        for value in [width as u64, height as u64].into_iter().chain(occupancy.iter().copied()) {
            fingerprint = (fingerprint ^ value).wrapping_mul(1099511628211);
        }
        Some(Self { width, height, occupancy, pixels, fingerprint })
    }
    pub fn width(&self) -> u32 { self.width }
    pub fn height(&self) -> u32 { self.height }
    pub fn pixel_count(&self) -> usize { self.pixels }
    pub fn fingerprint(&self) -> u64 { self.fingerprint }
    pub fn occupied(&self, x: i64, y: i64) -> bool {
        if x < 0 || y < 0 || x >= self.width as i64 || y >= self.height as i64 { return false; }
        let index = y as usize * self.width as usize + x as usize;
        self.occupancy.get(index / 64).is_some_and(|word| word & (1_u64 << (index % 64)) != 0)
    }
    pub fn valid_at(&self, bounds: Rect) -> bool {
        bounds.valid() && bounds.x.fract() == 0.0 && bounds.y.fract() == 0.0
            && bounds.x >= 0.0 && bounds.y >= 0.0 && bounds.x <= u32::MAX as f64
            && bounds.y <= u32::MAX as f64 && bounds.width == self.width as f64
            && bounds.height == self.height as f64 && self.pixels > 0
            && self.width > 0 && self.height > 0 && self.width <= 256 && self.height <= 256
            && self.occupancy.len() == (self.width as usize * self.height as usize).div_ceil(64)
    }
    pub fn contains_at(&self, bounds: Rect, point: Point) -> bool {
        self.valid_at(bounds) && point.x.is_finite() && point.y.is_finite()
            && point.x.fract() == 0.0 && point.y.fract() == 0.0 && bounds.contains(point)
            && self.occupied((point.x - bounds.x) as i64, (point.y - bounds.y) as i64)
    }
    pub fn overlap_pixels(&self, bounds: Rect, other: &Self, other_bounds: Rect) -> usize {
        if !self.valid_at(bounds) || !other.valid_at(other_bounds) { return 0; }
        let left = bounds.x.max(other_bounds.x) as i64;
        let top = bounds.y.max(other_bounds.y) as i64;
        let right = (bounds.x + bounds.width).min(other_bounds.x + other_bounds.width) as i64;
        let bottom = (bounds.y + bounds.height).min(other_bounds.y + other_bounds.height) as i64;
        let mut count = 0;
        for y in top..bottom { for x in left..right {
            if self.occupied(x - bounds.x as i64, y - bounds.y as i64)
                && other.occupied(x - other_bounds.x as i64, y - other_bounds.y as i64) { count += 1; }
        }}
        count
    }
}

/// Shared pure-analysis inputs and ROI fingerprint for object eligibility.
/// Omitted from JSON and never contains image bytes or site identifiers.
#[derive(Clone, Debug, PartialEq)]
pub struct SeatObjectContext {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) stride: usize,
    pub(crate) digest: u64,
    pub(crate) roi: Option<Rect>,
    pub(crate) exclusions: Vec<Rect>,
    pub(crate) min_component_pixels: usize,
    pub(crate) max_component_pixels: usize,
    pub(crate) color: Color,
    pub(crate) tolerance: u8,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CandidateGeometry {
    /// Color-component bounds in ordinary mode, learned whole-object bounds in
    /// object mode; both are captured physical pixels, never site semantics.
    pub bounds: Rect,
    pub pixels: usize,
    #[serde(skip)]
    pub object_shape: Option<Arc<SeatShape>>,
    #[serde(skip)]
    pub object_context: Option<Arc<SeatObjectContext>>,
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
    /// Explicit user confirmation hands every phase directly to post-CAPTCHA
    /// search, retaining delivered seats and discarding unsent entry clicks.
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
    /// A readable site count is independent of image candidate IDs. The adapter
    /// must validate the current run/window and a fresh post-input observation.
    SelectionCountObserved { request_id: u64, observation_id: u64, count: usize },
    /// A fresh read was attempted but its site count was not readable.
    SelectionCountUnavailable { request_id: u64, observation_id: u64 },
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectionCountRequest {
    pub id: u64,
    pub generation: u64,
    pub after_action_id: Option<u64>,
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
    /// Site-total verification is independent of the original selection policy.
    #[serde(default)]
    pub selection_count_verification: bool,
    /// Active delivered attempts in delivery order, including positively selected ones.
    #[serde(default)]
    pub delivered_attempts: Vec<DeliveredSeatAttempt>,
    #[serde(default)]
    pub conflict_attempt: Option<DeliveredSeatAttempt>,
    #[serde(default)]
    pub site_selection_count: Option<usize>,
    #[serde(default)]
    pub selection_count_pending: bool,
}
