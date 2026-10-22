use crate::model::*;
use crate::seat_selection::{preferred_candidate, same_seat};
use crate::vision::{color_matches, rank_regions, rank_seats};
use std::collections::{BTreeSet, VecDeque};

#[derive(Clone, Debug)]
enum Effect {
    Legacy,
    InitialEntry,
    Entry,
    Region,
    Seat,
    Escape,
    Refresh,
    LegacyRegion,
    Complete,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum InitialEntryState {
    #[default]
    NotStarted,
    AwaitingDelivery,
    AwaitingMap,
    Confirmed,
}

#[derive(Clone, Debug)]
struct Pending {
    id: u64,
    sent_at_ms: u64,
    effect: Effect,
    kind: ActionKind,
    navigation_rollback: Option<(Option<usize>, usize)>,
}

#[derive(Clone, Default)]
struct SelectionCountState {
    observed: Option<usize>,
    baseline: Option<usize>,
    awaiting: Option<DeliveredSeatAttempt>,
    accounted: BTreeSet<u64>,
    request: Option<SelectionCountRequest>,
    last_observation: Option<u64>,
    after_action: Option<u64>,
    verified: bool,
    // A valid observation may authorize input only during its own event step.
    permit_input: bool,
}

/// Actions are requests, acknowledgements are input delivery, observations are state evidence.
/// Neither an acknowledged click nor the absence of an error confirms a reservation.
#[derive(Clone)]
pub struct Engine {
    profile: Profile,
    phase: Phase,
    resume_phase: Phase,
    mode: RunMode,
    message: String,
    next_id: u64,
    pending: Option<Pending>,
    selected: BTreeSet<String>,
    attempted: BTreeSet<String>,
    delivered_attempts: Vec<DeliveredSeatAttempt>,
    conflict_attempt: Option<DeliveredSeatAttempt>,
    rejected: BTreeSet<String>,
    candidates: Vec<Candidate>,
    current: Option<String>,
    current_seat: Option<Candidate>,
    seat_marks: Vec<(Option<usize>, Candidate)>,
    selection_count_state: SelectionCountState,
    generation: u64,
    legacy_queue: VecDeque<Target>,
    legacy_entry_complete: bool,
    last_entry: Option<String>,
    captcha_verified: bool,
    manual_captcha_completed: bool,
    conflict_dismiss_requested: bool,
    reconcile_selection: bool,
    next_eligible_ms: u64,
    last_observation_ms: u64,
    last_seat_search_ms: u64,
    last_rejected_retry_ms: u64,
    last_progress_ms: u64,
    now_ms: u64,
    cancellation_turn: usize,
    completion_submitted: bool,
    map_observed: bool,
    restart_observation_required: bool,
    legacy_region_due_ms: Option<u64>,
    active_region: Option<usize>,
    retry_cancellation: bool,
    initial_entry: InitialEntryState,
    initial_entry_enabled: bool,
    initial_clicked_target: Option<Target>,
}

impl Engine {
    pub fn new(profile: Profile) -> Result<Self, ValidationError> {
        profile.validate()?;
        let legacy_entry_complete = [&profile.legacy.date, &profile.legacy.session, &profile.legacy.next]
            .into_iter().all(Option::is_none);
        Ok(Self {
            profile,
            phase: Phase::Idle,
            resume_phase: Phase::Idle,
            mode: RunMode::Normal,
            message: "대기 중".into(),
            next_id: 1,
            pending: None,
            selected: BTreeSet::new(),
            attempted: BTreeSet::new(),
            delivered_attempts: vec![],
            conflict_attempt: None,
            rejected: BTreeSet::new(),
            candidates: vec![],
            current: None,
            current_seat: None,
            seat_marks: vec![],
            selection_count_state: SelectionCountState::default(),
            generation: 0,
            legacy_queue: VecDeque::new(),
            legacy_entry_complete,
            last_entry: None,
            captcha_verified: false,
            manual_captcha_completed: false,
            conflict_dismiss_requested: false,
            reconcile_selection: false,
            next_eligible_ms: 0,
            last_observation_ms: 0,
            last_seat_search_ms: 0,
            last_rejected_retry_ms: 0,
            last_progress_ms: 0,
            now_ms: 0,
            cancellation_turn: 0,
            completion_submitted: false,
            map_observed: false,
            restart_observation_required: false,
            legacy_region_due_ms: None,
            active_region: None,
            retry_cancellation: false,
            initial_entry: InitialEntryState::NotStarted,
            initial_entry_enabled: false,
            initial_clicked_target: None,
        })
    }

    pub fn profile(&self) -> &Profile { &self.profile }

    pub fn selection_count_request(&self) -> Option<SelectionCountRequest> {
        (self.profile.features.nol_selection_count
            && matches!(self.phase, Phase::Seats | Phase::Completing)
            && self.pending.is_none()).then_some(self.selection_count_state.request).flatten()
    }

    /// A count below the target may authorize a seat/refresh decision only
    /// after the caller has observed candidates from that same acquired frame.
    /// Initial/retried legacy navigation deliberately precedes a seat map; an
    /// already-satisfied total may complete without a seat-map calibration.
    pub fn selection_count_needs_seat_observation(&self, count: usize) -> bool {
        self.profile.features.nol_selection_count && self.phase == Phase::Seats
            && count < self.profile.target_seats && !self.retry_cancellation
    }

    /// These routes count delivered attempts and keep looking without selection markers.
    pub fn continuous_search(&self) -> bool {
        self.mode == RunMode::Seats
            || self.has_captcha_gate()
            || (self.mode == RunMode::Normal && self.manual_captcha_completed)
            || (self.mode == RunMode::Cancellation
                && matches!(self.profile.legacy.cancellation_navigation, CancellationNavigation::None))
    }

    /// A single-zone cancellation run starts inside the user's current seat map.
    /// General-booking entry and region settings must not redirect this route.
    fn starts_at_seat_map(&self) -> bool {
        self.mode == RunMode::Seats
            || (self.mode == RunMode::Cancellation
                && matches!(self.profile.legacy.cancellation_navigation, CancellationNavigation::None))
    }

    fn has_captcha_gate(&self) -> bool {
        self.mode == RunMode::Normal
            && (self.profile.features.auto_captcha_resume || self.profile.features.nol_captcha_detection
                || self.profile.features.manual_entry)
    }

    fn nol_captcha_enabled(&self) -> bool {
        self.mode == RunMode::Normal && self.profile.features.nol_captcha_detection
    }

    /// The visual detector may run only after registered entry inputs finish.
    /// Pausing, stopping or another run mode cannot be bypassed by an old image.
    pub fn awaits_nol_captcha(&self) -> bool {
        self.nol_captcha_enabled() && self.phase == Phase::Captcha
            && self.pending.is_none() && self.legacy_queue.is_empty()
            && (!self.has_initial_entry()
                || matches!(self.initial_entry, InitialEntryState::AwaitingMap | InitialEntryState::Confirmed))
    }

    /// Manual confirmation needs no detector/Enter option or recognized image.
    /// Only fully delivered entry inputs may hand off to region/seat searching.
    pub fn can_manually_complete_captcha(&self) -> bool {
        let phase = self.confirmation_phase();
        if self.mode != RunMode::Normal {
            return matches!(phase, Phase::Captcha | Phase::Region | Phase::WaitingRegion | Phase::Seats
                | Phase::Conflict | Phase::Completing | Phase::Completed | Phase::WaitingRefresh);
        }
        // With automatic CAPTCHA routes disabled, delivery of the initial
        // booking click deliberately waits in Entry for the user's handoff.
        // F9 confirms those intervening manual steps without replaying them.
        if phase == Phase::Entry && !self.has_captcha_gate() && self.has_initial_entry()
            && self.initial_entry == InitialEntryState::AwaitingMap && self.pending.is_none() {
            return true;
        }
        matches!(phase, Phase::Captcha | Phase::Region | Phase::WaitingRegion
                | Phase::Seats | Phase::Conflict | Phase::Completing | Phase::Completed)
            && self.legacy_entry_complete && self.legacy_queue.is_empty()
            && (self.confirmation_phase() != Phase::Captcha || self.pending.is_none())
            && (!self.has_initial_entry()
                || matches!(self.initial_entry, InitialEntryState::AwaitingMap | InitialEntryState::Confirmed))
    }

    /// Enter must come from the bound browser. Scheduled or undelivered entry clicks
    /// cannot be skipped, and stopped/paused runs never restart from a keyboard event.
    pub fn accepts_captcha_enter(&self) -> bool {
        self.mode == RunMode::Normal && self.profile.features.auto_captcha_resume
            && !self.profile.features.nol_captcha_detection
            && matches!(self.phase, Phase::Captcha | Phase::Region
                | Phase::WaitingRegion | Phase::Seats | Phase::Conflict | Phase::Completing)
            && !(self.phase == Phase::Completing && self.effective_completion_target().is_none())
            && (!self.has_initial_entry()
                || matches!(self.initial_entry, InitialEntryState::AwaitingMap | InitialEntryState::Confirmed))
    }

    /// Identify the current confirmation without silently resuming a paused run.
    /// The adapter must validate the target and explicitly resume before applying it.
    pub fn confirmation_phase(&self) -> Phase {
        if self.phase == Phase::Paused { self.resume_phase } else { self.phase }
    }

    fn region_settings(&self) -> Option<&LegacyRegionSettings> {
        self.active_region.and_then(|index| self.profile.legacy.regions.get(index))
    }

    pub fn effective_color(&self) -> Color {
        self.region_settings().and_then(|r| r.color).unwrap_or(self.profile.color)
    }

    pub fn effective_vision(&self) -> &VisionConfig {
        self.region_settings().and_then(|r| r.vision.as_ref()).unwrap_or(&self.profile.vision)
    }

    pub fn effective_completion_target(&self) -> Option<&Target> {
        self.region_settings().and_then(|r| r.complete.as_ref()).or(self.profile.legacy.complete.as_ref())
    }

    /// Changing calibration after delivery must not re-arm the initial booking click.
    pub fn update_entry_target(&mut self, target: Option<Target>) -> Result<(), ValidationError> {
        if !matches!(self.phase, Phase::Idle | Phase::Paused) {
            return Err(ValidationError("예매하기 버튼을 바꾸려면 먼저 일시정지하세요".into()));
        }
        if self.initial_entry == InitialEntryState::AwaitingDelivery && target != self.profile.entry_target {
            return Err(ValidationError("예매하기 클릭 전달 여부 확인 중에는 버튼을 바꿀 수 없습니다".into()));
        }
        let mut profile = self.profile.clone();
        profile.entry_target = target;
        profile.validate()?;
        if self.phase == Phase::Idle {
            self.initial_entry_enabled = self.mode == RunMode::Normal && profile.entry_target.is_some();
        } else if self.mode == RunMode::Normal && self.resume_phase == Phase::WaitingSale && profile.entry_target.is_some() {
            self.initial_entry_enabled = true;
        }
        self.profile = profile;
        Ok(())
    }

    /// Discard coordinates tied to an old capture geometry without restarting the run.
    /// The adapter must cancel proven-undelivered actions before pausing and calling this.
    pub fn invalidate_coordinate_geometry(&mut self) -> Result<(), ValidationError> {
        if !matches!(self.phase, Phase::Idle | Phase::Paused) {
            return Err(ValidationError("화면 크기·배율 설정을 바꾸려면 먼저 일시정지하세요".into()));
        }
        // Validate ambiguous delivery before changing any state. Clearing a delivered
        // initial target preserves the route's manual seat-map confirmation gate.
        self.update_entry_target(None)?;
        self.profile.legacy.date = None;
        self.profile.legacy.session = None;
        self.profile.legacy.next = None;
        self.profile.legacy.complete = None;
        if matches!(self.profile.legacy.cancellation_navigation, CancellationNavigation::TwoRegions { .. }) {
            self.profile.legacy.cancellation_navigation = CancellationNavigation::None;
        }
        for region in &mut self.profile.legacy.regions {
            region.vision = None;
            region.complete = None;
        }
        self.profile.vision.roi = None;
        self.profile.vision.exclusions.clear();
        self.profile.refresh_target = None;
        self.legacy_queue.clear();
        self.candidates.clear();
        self.seat_marks.clear();
        self.current_seat = None;
        self.map_observed = false;
        self.invalidate_selection_count_request();
        Ok(())
    }

    pub fn update_legacy_regions(&mut self, regions: Vec<LegacyRegionSettings>) -> Result<(), ValidationError> {
        if !matches!(self.phase, Phase::Idle | Phase::Paused) {
            return Err(ValidationError("구역별 설정을 바꾸려면 먼저 일시정지하세요".into()));
        }
        let mut profile = self.profile.clone();
        profile.legacy.regions = regions;
        profile.validate()?;
        let new_color = self.active_region.and_then(|index| profile.legacy.regions.get(index))
            .and_then(|r| r.color).unwrap_or(profile.color);
        if self.current.is_some() && new_color != self.effective_color() {
            return Err(ValidationError("좌석 선택 결과 확인 중에는 현재 구역의 색상을 바꿀 수 없습니다".into()));
        }
        self.profile = profile;
        self.candidates.clear();
        self.map_observed = false;
        Ok(())
    }

    /// Calibrate a previously hidden screen without losing retained selections.
    pub fn update_calibration(&mut self, vision: VisionConfig, color: Color, region_color: Option<Color>, complete: Option<Target>) -> Result<(), ValidationError> {
        if !matches!(self.phase, Phase::Idle | Phase::Paused) {
            return Err(ValidationError("화면 설정을 바꾸려면 먼저 일시정지하세요".into()));
        }
        if self.current.is_some() && (color != self.profile.color || region_color != self.profile.region_color) {
            return Err(ValidationError("좌석 선택 결과 확인 중에는 색상을 바꿀 수 없습니다".into()));
        }
        let mut profile = self.profile.clone();
        profile.vision = vision;
        profile.color = color;
        profile.region_color = region_color;
        profile.legacy.complete = complete;
        profile.validate()?;
        self.profile = profile;
        self.candidates.clear();
        self.map_observed = false;
        Ok(())
    }

    fn invalidate_selection_count_request(&mut self) {
        self.selection_count_state.request = None;
        self.selection_count_state.last_observation = None;
        self.selection_count_state.verified = false;
        self.selection_count_state.permit_input = false;
    }

    fn ensure_selection_count_request(&mut self) {
        if !self.profile.features.nol_selection_count
            || !matches!(self.phase, Phase::Seats | Phase::Completing) || self.pending.is_some() {
            self.selection_count_state.request = None;
            self.selection_count_state.last_observation = None;
            return;
        }
        if self.selection_count_state.request.is_none() {
            let id = self.next_id;
            self.next_id += 1;
            self.selection_count_state.request = Some(SelectionCountRequest {
                id, generation: self.generation, after_action_id: self.selection_count_state.after_action,
            });
            self.selection_count_state.last_observation = None;
        }
    }

    fn accept_selection_count_observation(&mut self, request_id: u64, observation_id: u64) -> bool {
        let Some(request) = self.selection_count_request() else { return false; };
        if request.id != request_id || request.generation != self.generation
            || self.selection_count_state.last_observation.is_some_and(|last| observation_id <= last) { return false; }
        self.selection_count_state.last_observation = Some(observation_id);
        self.selection_count_state.verified = false;
        self.selection_count_state.permit_input = false;
        self.last_observation_ms = self.now_ms;
        true
    }

    fn observe_selection_count(&mut self, request_id: u64, observation_id: u64, count: usize, actions: &mut Vec<Action>) {
        if !self.accept_selection_count_observation(request_id, observation_id) { return; }
        self.selection_count_state.observed = Some(count);
        let expected = self.selection_count_state.baseline.and_then(|baseline|
            baseline.checked_add(usize::from(self.selection_count_state.awaiting.is_some())));
        if count > self.profile.target_seats || self.selection_count_state.baseline.is_some() && expected != Some(count) {
            self.message = if self.selection_count_state.awaiting.is_some()
                && self.selection_count_state.baseline == Some(count) {
                "NOL 선택 좌석 수 변화 대기 · 성공·실패를 추정하지 않고 화면을 계속 확인합니다".into()
            } else { format!("NOL 선택 좌석 수 불일치 · 화면 {count}석 · 추가 클릭과 완료를 보류합니다") };
            return;
        }
        if let Some(attempt) = self.selection_count_state.awaiting.take() {
            // A count increment is total-cardinality evidence. Keep the image
            // attempt mark; do not manufacture semantic selected-seat IDs.
            self.selection_count_state.accounted.insert(attempt.action_id);
        }
        self.selection_count_state.baseline = Some(count);
        self.selection_count_state.verified = true;
        self.restart_observation_required = false;
        self.selection_count_state.permit_input = true;
        self.reconcile_selection = false;
        self.message = format!("NOL 선택 좌석 {count} / {}석 확인 · 다음 후보 탐색", self.profile.target_seats);
        if self.phase == Phase::Completing && !self.completion_submitted {
            self.submit_completion(actions);
        } else { self.tick(actions); }
        self.selection_count_state.permit_input = false;
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            phase: self.phase,
            mode: self.mode,
            message: self.message.clone(),
            selected_ids: self.selected.iter().cloned().collect(),
            attempted_ids: self.attempted.iter().cloned().collect(),
            rejected_ids: self.rejected.iter().cloned().collect(),
            target_seats: self.profile.target_seats,
            generation: self.generation,
            pending_action_id: self.pending.as_ref().map(|p| p.id),
            current_candidate_id: self.current.clone(),
            active_region: self.active_region,
            optimistic_selection: self.continuous_search(),
            selection_count_verification: self.profile.features.nol_selection_count,
            delivered_attempts: self.delivered_attempts.clone(),
            conflict_attempt: self.conflict_attempt.clone(),
            site_selection_count: self.selection_count_state.observed,
            selection_count_pending: self.profile.features.nol_selection_count
                && matches!(self.phase, Phase::Seats | Phase::Completing | Phase::Conflict | Phase::Paused)
                && (!self.selection_count_state.verified || self.selection_count_state.awaiting.is_some()),
        }
    }

    pub fn step(&mut self, event: Event, now_ms: u64) -> Vec<Action> {
        // A misordered clock sample cannot move timers backwards.
        self.now_ms = self.now_ms.max(now_ms);
        let mut actions = vec![];
        match event {
            Event::Start { mode } => {
                if !matches!(self.phase, Phase::Idle | Phase::Stopped | Phase::Completed | Phase::Faulted) {
                    self.message = "이미 실행 중입니다. 새로 시작하려면 먼저 중지하세요".into();
                    return actions;
                }
                self.reset_run(mode);
                if !self.starts_at_seat_map() && self.profile.features.server_clock {
                    match self.profile.sale_deadline_ms {
                        Some(deadline) if self.now_ms < deadline => {
                            self.phase = Phase::WaitingSale;
                            self.message = "보정한 판매 시작 시각을 기다리는 중".into();
                        }
                        Some(_) => self.begin(&mut actions),
                        None => self.fault("서버 시간 측정이 켜져 있지만 판매 시작 시각이 준비되지 않았습니다"),
                    }
                } else {
                    self.begin(&mut actions);
                }
            }
            Event::Stop => {
                self.invalidate_selection_count_request();
                self.pending = None;
                self.phase = Phase::Stopped;
                self.message = "중지됨".into();
                self.current = None;
                self.candidates.clear();
                self.map_observed = false;
            }
            Event::Pause => {
                if self.running() {
                    self.invalidate_selection_count_request();
                    self.resume_phase = self.phase;
                    self.phase = Phase::Paused;
                    self.message = "일시정지됨".into();
                    // Already delivered inputs are not rolled back; resume observes their result.
                    self.pending = None;
                    self.candidates.clear();
                    self.map_observed = false;
                }
            }
            Event::Resume => {
                if self.phase == Phase::Paused {
                    self.phase = self.resume_phase;
                    self.last_observation_ms = self.now_ms;
                    self.last_progress_ms = self.now_ms;
                    self.message = "재개됨 · 현재 화면 확인 중".into();
                    if self.phase == Phase::Completing && !self.completion_submitted {
                        self.submit_completion(&mut actions);
                    } else {
                        self.observe(self.scope(), &mut actions);
                    }
                } else if self.phase == Phase::Captcha && self.captcha_verified {
                    self.after_captcha(&mut actions);
                }
            }
            Event::ActionCancelled { id } => {
                if self.pending.as_ref().is_some_and(|p| p.id == id) {
                    self.invalidate_selection_count_request();
                    let pending = self.pending.take().unwrap();
                    self.candidates.clear();
                    self.map_observed = false;
                    self.message = "전달 전 입력 취소됨 · 재개 시 현재 화면을 다시 확인합니다".into();
                    match pending.effect {
                        Effect::Seat => { self.current = None; self.current_seat = None; self.phase = Phase::Seats; }
                        Effect::Legacy => {
                            if let ActionKind::Click { target, .. } = pending.kind {
                                self.legacy_queue.push_front(target);
                            }
                            self.phase = Phase::Entry;
                        }
                        Effect::Entry => { self.last_entry = None; self.phase = Phase::Entry; }
                        Effect::InitialEntry => {
                            self.initial_entry = InitialEntryState::NotStarted;
                            self.initial_clicked_target = None;
                            self.phase = Phase::Entry;
                        }
                        Effect::Region => { self.phase = Phase::Region; }
                        Effect::Refresh | Effect::LegacyRegion => { self.phase = Phase::Seats; }
                        Effect::Complete => {
                            self.completion_submitted = false;
                            self.phase = Phase::Completing;
                        }
                        Effect::Escape => {
                            // The known dialog still needs Esc, but do not blindly resend
                            // until the adapter explicitly resumes observation/handling.
                            self.conflict_dismiss_requested = false;
                            self.phase = Phase::Conflict;
                        }
                    }
                    if let Some((previous_region, previous_turn)) = pending.navigation_rollback {
                        self.active_region = previous_region;
                        self.cancellation_turn = previous_turn;
                        self.legacy_region_due_ms = None;
                        self.retry_cancellation = true;
                    }
                }
            }
            Event::CaptchaManuallyCompleted => {
                if self.can_manually_complete_captcha() {
                    self.manual_captcha_completed = true;
                    self.restart_post_entry(&mut actions);
                }
            }
            _ if !self.running() => {}
            Event::ContextLost { reason } => {
                self.invalidate_selection_count_request();
                self.resume_phase = self.phase;
                self.phase = Phase::Paused;
                self.pending = None;
                self.candidates.clear();
                self.map_observed = false;
                self.message = format!("일시정지 · {reason}");
            }
            Event::Tick => self.tick(&mut actions),
            Event::ActionAcknowledged { id, success, detail } => {
                if self.pending.as_ref().is_some_and(|p| p.id == id) {
                    let pending = self.pending.take().unwrap();
                    if !success {
                        self.fault(&format!("입력 실패 · {detail}"));
                        return actions;
                    }
                    self.last_observation_ms = self.now_ms;
                    self.last_progress_ms = self.now_ms;
                    if self.profile.features.nol_selection_count {
                        self.selection_count_state.after_action = Some(id);
                        self.invalidate_selection_count_request();
                    }
                    match pending.effect {
                        Effect::Legacy => {
                            if self.legacy_queue.is_empty() { self.legacy_entry_complete = true; }
                            self.next_eligible_ms = self.now_ms.saturating_add(self.click_delay());
                            self.advance_legacy(&mut actions);
                        }
                        Effect::Region => {
                            self.next_eligible_ms = self.now_ms.saturating_add(self.profile.legacy.region_delay_ms);
                            if self.continuous_search() {
                                self.phase = Phase::Seats;
                                self.candidates.clear();
                                self.map_observed = false;
                                self.message = "구역 클릭 전달됨 · 새 화면에서 좌석 탐색 중".into();
                                self.observe("seats", &mut actions);
                            } else {
                                self.phase = Phase::WaitingRegion;
                                self.message = "구역 좌석 화면이 보이면 F6을 누르세요 · 확인 후 좌석 탐색".into();
                                self.observe("region_transition", &mut actions);
                            }
                        }
                        Effect::Seat => {
                            self.remember_current_seat();
                            if self.continuous_search() || self.profile.features.nol_selection_count {
                                if let Some(candidate_id) = self.current.take() {
                                    self.attempted.insert(candidate_id.clone());
                                    let attempt = DeliveredSeatAttempt {
                                        action_id: id, candidate_id, generation: self.generation,
                                    };
                                    if self.profile.features.nol_selection_count {
                                        self.selection_count_state.awaiting = Some(attempt.clone());
                                    }
                                    self.delivered_attempts.push(attempt);
                                }
                                self.next_eligible_ms = self.now_ms.saturating_add(self.click_delay());
                                self.message = if self.profile.features.nol_selection_count {
                                    "좌석 클릭 전달됨 · NOL 선택 좌석 수 증가 확인 중".into()
                                } else { format!("{} / {}석 선택 추정 · 클릭 전달 후 계속 진행", self.selection_count(), self.profile.target_seats) };
                                self.select_next(&mut actions);
                                if actions.is_empty() && self.phase == Phase::Seats {
                                    self.observe("seats", &mut actions);
                                }
                            } else if self.blind_sequence() {
                                if let Some(id) = self.current.take() { self.attempted.insert(id); }
                                self.next_eligible_ms = self.now_ms.saturating_add(self.click_delay());
                                self.message = "원본 클릭 전달됨 · 클릭 시도이며 좌석 확보 확인은 아닙니다".into();
                                self.select_next(&mut actions);
                            } else {
                                self.message = "좌석 클릭 전달됨 · 선택 결과 확인 중".into();
                            }
                        }
                        Effect::Escape => self.message = "Esc 전달됨 · 선점 실패 안내창 닫힘 확인 중".into(),
                        Effect::Refresh => {
                            if matches!(pending.kind, ActionKind::Refresh { .. }) {
                                // Delivery restarts observation, not the refresh generation.
                                // Cancellation retries use their own search interval; the
                                // acknowledgement only throttles the next button input.
                                self.phase = Phase::Seats;
                                self.candidates.clear();
                                self.map_observed = false;
                                self.observe("seats", &mut actions);
                                self.message = "새로고침 클릭 전달됨 · 현재 좌석 화면 탐색을 계속합니다".into();
                            } else {
                                if self.mode == RunMode::Cancellation {
                                    self.next_eligible_ms = self.now_ms.saturating_add(self.profile.legacy.region_delay_ms);
                                }
                                self.message = "새로고침이 끝나면 F6을 누르세요 · 확인 후 좌석 탐색".into();
                            }
                        }
                        Effect::LegacyRegion => {
                            self.legacy_region_due_ms = Some(self.now_ms.saturating_add(self.profile.legacy.region_delay_ms));
                            self.message = "원본 방식으로 취소표 구역 전환 · 지정한 구역 대기 시간 적용 중".into();
                        }
                        Effect::Complete => {
                            self.phase = Phase::Completed;
                            self.current = None;
                            self.candidates.clear();
                            self.map_observed = false;
                            self.message = "완료 버튼 클릭 전달 · 실행 종료".into();
                        }
                        Effect::Entry => self.message = "예매 진입 클릭 전달됨 · 다음 화면 확인 중".into(),
                        Effect::InitialEntry => {
                            self.initial_entry = InitialEntryState::AwaitingMap;
                            if self.has_captcha_gate() {
                                self.begin_legacy_entry(&mut actions);
                            } else {
                                self.message = "예매하기 클릭 전달됨 · 좌석 화면에 진입한 뒤 F6으로 확인하세요".into();
                            }
                        }
                    }
                }
            }
            Event::EntryObserved { stage, target, enabled, matched_value, screen_id } => {
                self.entry_observed(stage, target, enabled, matched_value, screen_id, &mut actions);
            }
            Event::CaptchaCompleted => {
                if self.phase == Phase::Captcha && !self.nol_captcha_enabled() {
                    self.captcha_verified = true;
                    if self.profile.features.auto_captcha_resume {
                        self.after_captcha(&mut actions);
                    } else {
                        self.message = "캡챠 완료 확인됨 · 재개를 누르세요".into();
                    }
                }
            }
            Event::CaptchaSubmitted => {
                if self.accepts_captcha_enter() {
                    self.begin_post_captcha_search(&mut actions);
                    self.message = if self.phase == Phase::Region {
                        "Enter 입력 감지 · 구역 탐색 시작"
                    } else { "Enter 입력 감지 · 좌석 탐색 시작" }.into();
                }
            }
            Event::NolCaptchaDismissed => {
                if self.awaits_nol_captcha() {
                    self.begin_post_captcha_search(&mut actions);
                    self.message = if self.phase == Phase::Region {
                        "NOL 캡챠 배경 색 변화 감지 · 구역 탐색 시작"
                    } else { "NOL 캡챠 배경 색 변화 감지 · 좌석 탐색 시작" }.into();
                }
            }
            Event::RegionsObserved { candidates } => {
                if self.phase == Phase::Region && self.pending.is_none() {
                    let ranked = rank_regions(candidates, self.profile.region_color.unwrap_or(self.profile.color), self.profile.color_tolerance);
                    if let Some(region) = ranked.first() {
                        self.phase = Phase::WaitingRegion;
                        self.observe("region_transition", &mut actions);
                        self.issue(ActionKind::Click {
                            target: region.candidate.target.clone(),
                            purpose: format!("region:{}", region.candidate.id),
                        }, Effect::Region, &mut actions);
                        self.message = "무대 정면 중앙 구역 선택 중".into();
                    } else {
                        self.message = "지정한 구역 색상과 확인된 무대 위치를 기다리는 중".into();
                    }
                }
            }
            Event::RegionMapReady => {
                if matches!(self.phase, Phase::Region | Phase::WaitingRegion) {
                    // Explicit map confirmation also supports starting while already
                    // inside a region. Any queued region click is now obsolete;
                    // its old acknowledgement must not override the seat phase.
                    self.pending = None;
                    self.phase = Phase::Seats;
                    self.last_observation_ms = self.now_ms;
                    self.last_progress_ms = self.now_ms;
                    self.candidates.clear();
                    self.map_observed = false;
                    self.observe("seats", &mut actions);
                    self.message = "구역 좌석 화면 확인됨 · 좌석 탐색 중".into();
                }
            }
            Event::CandidatesObserved { generation, candidates } => {
                if self.phase == Phase::Seats && generation == self.generation && !self.retry_cancellation {
                    self.map_observed = true;
                    if !self.profile.features.nol_selection_count {
                        self.restart_observation_required = false;
                    }
                    if self.current.is_none() {
                        self.last_observation_ms = self.now_ms;
                        self.last_seat_search_ms = self.now_ms;
                    }
                    self.candidates = candidates.into_iter().filter(|c| self.valid_candidate(c)).collect();
                    if self.profile.features.seat_priority {
                        rank_seats(&mut self.candidates);
                    }
                    self.select_next(&mut actions);
                }
            }
            Event::SeatResult { candidate_id, result } => {
                self.seat_result(candidate_id, result, &mut actions);
            }
            Event::DeliveredSeatResult { action_id, candidate_id, result } => {
                if self.continuous_search() || self.profile.features.nol_selection_count {
                    self.optimistic_seat_result(candidate_id, Some(action_id), result, &mut actions);
                }
            }
            Event::ConflictDialogGone => {
                if self.phase == Phase::Conflict {
                    self.pending = None;
                    if let Some(id) = self.current.take() {
                        self.selected.remove(&id);
                        self.attempted.remove(&id);
                        self.rejected.insert(id);
                        self.last_rejected_retry_ms = self.now_ms;
                    }
                    self.phase = Phase::Seats;
                    self.conflict_dismiss_requested = false;
                    self.conflict_attempt = None;
                    self.candidates.clear();
                    self.map_observed = false;
                    self.reconcile_selection = !self.continuous_search() && !self.profile.features.nol_selection_count && !self.selected.is_empty();
                    self.message = if self.reconcile_selection {
                        "선점 실패 안내창 닫힘 · 유지된 좌석 확인 중"
                    } else {
                        "선점 실패 안내창 닫힘 · 다음 후보 탐색 중"
                    }.into();
                    self.observe(if self.reconcile_selection { "selection" } else { "seats" }, &mut actions);
                }
            }
            Event::SelectionObserved { selected_ids } => {
                if self.profile.features.nol_selection_count { return actions; }
                if matches!(self.phase, Phase::Seats | Phase::Completing) {
                    if self.continuous_search() {
                        // Positive markers may improve the display, but absence from a
                        // site count never erases provisional inputs in optimistic mode.
                        for id in selected_ids {
                            if self.delivered_attempts.iter().any(|attempt| attempt.candidate_id == id) {
                                self.attempted.remove(&id);
                                self.selected.insert(id);
                            }
                        }
                        return actions;
                    }
                    let selected: BTreeSet<String> = selected_ids.into_iter().filter(|s| !s.is_empty()).collect();
                    if selected.len() > self.profile.target_seats {
                        self.fault("선택된 좌석이 좌석 선택 수보다 많습니다. 직접 확인하세요");
                        return actions;
                    }
                    if let Some(id) = self.current.as_ref() {
                        if selected.contains(id) {
                            self.current = None;
                            self.pending = None;
                        }
                    }
                    self.selected = selected;
                    self.attempted.clear();
                    self.prune_seat_marks();
                    self.reconcile_selection = false;
                    self.last_observation_ms = self.now_ms;
                    self.last_progress_ms = self.now_ms;
                    if self.phase == Phase::Seats {
                        self.select_next(&mut actions);
                        if actions.is_empty() && self.current.is_none() && self.pending.is_none() {
                            self.observe("seats", &mut actions);
                        }
                    }
                }
            }
            Event::RefreshCompleted { generation } => {
                if matches!(self.phase, Phase::WaitingRefresh | Phase::Seats) && generation > self.generation {
                    self.invalidate_selection_count_request();
                    let had_unresolved_click = self.current.is_some() || !self.attempted.is_empty();
                    self.generation = generation;
                    self.rejected.clear();
                    self.prune_seat_marks();
                    self.last_rejected_retry_ms = self.now_ms;
                    self.candidates.clear();
                    self.map_observed = false;
                    self.current = None;
                    self.pending = None;
                    self.phase = Phase::Seats;
                    self.last_observation_ms = self.now_ms;
                    self.last_progress_ms = self.now_ms;
                    self.reconcile_selection = !self.continuous_search() && !self.profile.features.nol_selection_count
                        && (!self.selected.is_empty() || had_unresolved_click);
                    self.next_eligible_ms = self.now_ms.max(self.next_eligible_ms);
                    self.message = "새로고침 확인됨 · 이전 실패 후보도 다시 탐색합니다".into();
                    self.observe(if self.reconcile_selection { "selection" } else { "seats" }, &mut actions);
                }
            }
            Event::CompletionTargetObserved { target } => {
                if matches!(self.phase, Phase::Seats | Phase::Completing) && target.valid() {
                    if let Some(index) = self.active_region {
                        while self.profile.legacy.regions.len() <= index {
                            self.profile.legacy.regions.push(LegacyRegionSettings::default());
                        }
                        self.profile.legacy.regions[index].complete = Some(target);
                    } else {
                        self.profile.legacy.complete = Some(target);
                    }
                    if self.phase == Phase::Completing && self.pending.is_none() && !self.completion_submitted {
                        self.submit_completion(&mut actions);
                    }
                }
            }
            Event::CompletionConfirmed => {
                if self.phase == Phase::Completing && self.selection_count() == self.profile.target_seats
                    && (!self.profile.features.nol_selection_count || self.selection_count_state.verified && self.selection_count_state.awaiting.is_none()) {
                    self.pending = None;
                    self.phase = Phase::Completed;
                    self.message = "직접 진행 확인 · 실행 종료".into();
                    self.candidates.clear();
                }
            }
            Event::SelectionCountObserved { request_id, observation_id, count } => {
                self.observe_selection_count(request_id, observation_id, count, &mut actions);
            }
            Event::SelectionCountUnavailable { request_id, observation_id } => {
                if self.accept_selection_count_observation(request_id, observation_id) {
                    self.message = "NOL 선택 좌석 수 표시를 찾는 중 · 추가 클릭·완료 보류".into();
                }
            }
        }
        self.ensure_selection_count_request();
        actions
    }

    fn reset_run(&mut self, mode: RunMode) {
        self.selection_count_state = SelectionCountState::default();
        self.mode = mode;
        self.phase = Phase::Entry;
        self.pending = None;
        self.selected.clear();
        self.attempted.clear();
        self.delivered_attempts.clear();
        self.conflict_attempt = None;
        self.rejected.clear();
        self.seat_marks.clear();
        self.current_seat = None;
        self.current = None;
        self.candidates.clear();
        self.legacy_queue.clear();
        self.legacy_entry_complete = [&self.profile.legacy.date, &self.profile.legacy.session, &self.profile.legacy.next]
            .into_iter().all(Option::is_none);
        self.generation = 0;
        self.last_entry = None;
        self.captcha_verified = false;
        self.manual_captcha_completed = false;
        self.conflict_dismiss_requested = false;
        self.reconcile_selection = false;
        self.next_eligible_ms = self.now_ms;
        self.last_observation_ms = self.now_ms;
        self.last_seat_search_ms = self.now_ms;
        self.last_rejected_retry_ms = self.now_ms;
        self.last_progress_ms = self.now_ms;
        self.cancellation_turn = 0;
        self.completion_submitted = false;
        self.map_observed = false;
        self.restart_observation_required = false;
        self.legacy_region_due_ms = None;
        self.active_region = if mode == RunMode::Cancellation && !self.profile.features.region_priority
            && !matches!(self.profile.legacy.cancellation_navigation, CancellationNavigation::None)
        { Some(0) } else { None };
        self.retry_cancellation = false;
        self.initial_entry = InitialEntryState::NotStarted;
        self.initial_entry_enabled = mode == RunMode::Normal && self.profile.entry_target.is_some();
        self.initial_clicked_target = None;
    }

    fn begin(&mut self, actions: &mut Vec<Action>) {
        if self.starts_at_seat_map() {
            self.after_entry(actions);
            return;
        }
        self.phase = Phase::Entry;
        if self.has_initial_entry() {
            self.issue_initial_entry(actions);
            return;
        }
        if self.has_captcha_gate() {
            self.begin_legacy_entry(actions);
            return;
        }
        if self.profile.features.auto_entry {
            self.message = "예매 진입 화면을 기다리는 중".into();
            self.observe("entry", actions);
            return;
        }
        self.begin_legacy_entry(actions);
    }

    fn begin_legacy_entry(&mut self, actions: &mut Vec<Action>) {
        self.phase = Phase::Entry;
        if self.mode == RunMode::Normal && self.profile.features.manual_entry {
            self.legacy_queue.clear();
            self.legacy_entry_complete = true;
            self.wait_for_captcha(actions);
            return;
        }
        if self.mode == RunMode::Normal {
            self.legacy_queue = [&self.profile.legacy.date, &self.profile.legacy.session, &self.profile.legacy.next]
                .into_iter().flatten().cloned().collect();
            self.legacy_entry_complete = self.legacy_queue.is_empty();
            if !self.legacy_queue.is_empty() && self.initial_entry == InitialEntryState::AwaitingMap {
                self.next_eligible_ms = self.now_ms.saturating_add(self.click_delay());
            }
        }
        self.advance_legacy(actions);
    }

    fn has_initial_entry(&self) -> bool {
        self.initial_entry_enabled
    }

    fn wait_for_captcha(&mut self, actions: &mut Vec<Action>) {
        self.phase = Phase::Captcha;
        self.captcha_verified = false;
        self.message = if self.profile.features.manual_entry && !self.nol_captcha_enabled()
            && !self.profile.features.auto_captcha_resume {
            "날짜·회차·예매 단계를 직접 진행한 뒤 F9을 누르세요"
        } else if self.nol_captcha_enabled() {
            "등록된 진입 버튼 처리 끝 · NOL 캡챠 첫 문자 입력에 기준 색을 저장합니다"
        } else {
            "등록된 진입 버튼 처리 끝 · 필요한 수동 진행과 캡챠 입력 후 Enter를 누르세요"
        }.into();
        self.observe("captcha", actions);
    }

    fn begin_post_captcha_search(&mut self, actions: &mut Vec<Action>) {
        self.selection_count_state = SelectionCountState::default();
        self.pending = None;
        self.current = None;
        self.selected.clear();
        self.attempted.clear();
        self.delivered_attempts.clear();
        self.conflict_attempt = None;
        self.rejected.clear();
        self.seat_marks.clear();
        self.current_seat = None;
        self.candidates.clear();
        self.legacy_queue.clear();
        self.last_entry = None;
        self.generation = self.generation.saturating_add(1);
        self.initial_entry = InitialEntryState::Confirmed;
        self.captcha_verified = false;
        self.conflict_dismiss_requested = false;
        self.reconcile_selection = false;
        self.completion_submitted = false;
        self.map_observed = false;
        self.legacy_region_due_ms = None;
        self.active_region = None;
        self.retry_cancellation = false;
        self.next_eligible_ms = self.now_ms;
        self.last_progress_ms = self.now_ms;
        self.last_rejected_retry_ms = self.now_ms;
        self.after_entry(actions);
    }

    /// Runtime must validate the same bound window/geometry before preserving a
    /// search. Counts, delivery receipts and this engine remain one logical run.
    pub fn restart_search(&mut self, profile: Profile, mode: RunMode, preserve_selection: bool,
        now_ms: u64) -> Result<Vec<Action>, ValidationError> {
        profile.validate()?;
        if mode == RunMode::Normal {
            return Err(ValidationError("일반 예매는 새 입장 실행으로 시작하세요".into()));
        }
        if let Some(id) = self.pending.as_ref().map(|pending| pending.id) {
            self.step(Event::ActionCancelled { id }, now_ms);
        }
        self.retain_unconfirmed_delivery();
        let previous = self.clone();
        let previous_phase = previous.confirmation_phase();
        self.now_ms = self.now_ms.max(now_ms);
        self.profile = profile;
        self.reset_run(mode);
        self.generation = previous.generation.saturating_add(1);
        let mut actions = vec![];
        if preserve_selection {
            self.selected = previous.selected;
            self.attempted = previous.attempted;
            self.delivered_attempts = previous.delivered_attempts;
            self.conflict_attempt = previous.conflict_attempt;
            self.rejected = previous.rejected;
            self.seat_marks = previous.seat_marks;
            self.selection_count_state = previous.selection_count_state;
            self.invalidate_selection_count_request();
            let next_region = if mode == RunMode::Cancellation { previous.active_region } else { None };
            for (region, _) in &mut self.seat_marks {
                if *region == previous.active_region { *region = next_region; }
            }
            self.active_region = next_region;
            self.cancellation_turn = previous.cancellation_turn;
            // Only this explicit execution command reopens completion. A new
            // map/count observation is required before it can be sent again.
            self.completion_submitted = false;
            self.restart_observation_required = true;
            if previous_phase == Phase::Conflict {
                self.phase = Phase::Conflict;
                self.conflict_dismiss_requested = previous.conflict_dismiss_requested;
                self.observe("conflict_dialog", &mut actions);
            } else if self.has_retained_selection() {
                self.phase = Phase::Seats;
                self.observe("seats", &mut actions);
            } else {
                self.after_entry(&mut actions);
            }
        } else { self.begin(&mut actions); }
        self.message = "새 실행 명령 수신 · 현재 화면에서 탐색을 다시 시작합니다".into();
        self.ensure_selection_count_request();
        Ok(actions)
    }

    pub fn has_retained_selection(&self) -> bool {
        !self.selected.is_empty() || !self.attempted.is_empty()
            || self.selection_count_state.awaiting.is_some()
            || self.selection_count_state.baseline.is_some_and(|count| count > 0)
            || self.seat_marks.iter().any(|(_, seat)| !self.rejected.contains(&seat.id))
    }

    /// Apply the caller's normal-mode execution settings without replaying entry.
    /// Runtime first checks retained color/area/shape/count compatibility.
    pub fn restart_after_entry(&mut self, profile: Profile, now_ms: u64) -> Result<Vec<Action>, ValidationError> {
        profile.validate()?;
        if !self.can_manually_complete_captcha() {
            return Err(ValidationError("F9은 진행 중인 탐색 또는 일반 예매 입장 완료 후 사용하세요".into()));
        }
        self.profile = profile;
        self.now_ms = self.now_ms.max(now_ms);
        self.manual_captcha_completed = true;
        let mut actions = vec![];
        // The delivered-entry preflight belongs to the previous run settings.
        // Newly enabled CAPTCHA options must not reclassify that old state.
        self.restart_post_entry(&mut actions);
        self.ensure_selection_count_request();
        Ok(actions)
    }

    fn retain_unconfirmed_delivery(&mut self) {
        if let Some(id) = &self.current {
            if self.seat_marks.iter().any(|(_, seat)| seat.id == *id) && !self.rejected.contains(id) {
                self.attempted.insert(id.clone());
            }
        }
    }

    /// A repeated F9 never replays entry and never forgets delivered selections.
    fn restart_post_entry(&mut self, actions: &mut Vec<Action>) {
        if let Some(id) = self.pending.as_ref().map(|pending| pending.id) {
            self.step(Event::ActionCancelled { id }, self.now_ms);
        }
        self.retain_unconfirmed_delivery();
        let conflict = self.confirmation_phase() == Phase::Conflict;
        self.mode = RunMode::Normal;
        self.legacy_queue.clear();
        self.legacy_entry_complete = true;
        self.pending = None;
        self.current = None;
        self.current_seat = None;
        self.candidates.clear();
        self.invalidate_selection_count_request();
        self.generation = self.generation.saturating_add(1);
        self.initial_entry = InitialEntryState::Confirmed;
        self.captcha_verified = false;
        self.completion_submitted = false;
        self.map_observed = false;
        self.restart_observation_required = true;
        self.legacy_region_due_ms = None;
        self.retry_cancellation = false;
        self.next_eligible_ms = self.now_ms;
        if conflict {
            self.phase = Phase::Conflict;
            self.observe("conflict_dialog", actions);
        } else if self.has_retained_selection() {
            self.phase = Phase::Seats;
            self.observe("seats", actions);
        } else {
            self.active_region = None;
            self.after_entry(actions);
        }
        self.message = "F9 재시도 · 전달한 좌석 기록을 유지하고 현재 화면을 다시 확인합니다".into();
    }

    fn issue_initial_entry(&mut self, actions: &mut Vec<Action>) {
        if self.initial_entry != InitialEntryState::NotStarted || self.pending.is_some() { return; }
        let Some(target) = self.profile.entry_target.clone() else {
            self.message = "현재 화면의 예매하기 버튼을 지정한 뒤 재개하세요".into();
            return;
        };
        self.initial_entry = InitialEntryState::AwaitingDelivery;
        self.initial_clicked_target = Some(target.clone());
        self.observe("entry", actions);
        self.issue(ActionKind::Click { target, purpose: "initial_entry".into() }, Effect::InitialEntry, actions);
        self.message = "지정한 예매하기 버튼 클릭 중".into();
    }

    fn after_captcha(&mut self, actions: &mut Vec<Action>) {
        if self.has_initial_entry() && self.initial_entry != InitialEntryState::Confirmed {
            self.phase = Phase::Entry;
            self.message = "캡챠 완료 확인됨 · 좌석 화면 진입을 F6으로 확인하세요".into();
            self.observe("entry", actions);
        } else {
            self.after_entry(actions);
        }
    }

    fn advance_legacy(&mut self, actions: &mut Vec<Action>) {
        if self.pending.is_some() || self.now_ms < self.next_eligible_ms { return; }
        if let Some(target) = self.legacy_queue.pop_front() {
            self.message = "지정된 날짜·회차·다음 좌표 클릭 중".into();
            self.issue(ActionKind::Click { target, purpose: "legacy_entry".into() }, Effect::Legacy, actions);
        } else if self.has_captcha_gate() {
            // The option gates region/seat work, not the original registered
            // date/session/next sequence. Repeated Enter goes straight to
            // after_entry() and never queues these coordinates again.
            self.wait_for_captcha(actions);
        } else {
            self.after_entry(actions);
        }
    }

    fn after_entry(&mut self, actions: &mut Vec<Action>) {
        self.pending = None;
        self.last_observation_ms = self.now_ms;
        if !self.starts_at_seat_map() && self.profile.features.region_priority {
            self.phase = Phase::Region;
            self.message = "구역 색상과 무대 정면 중앙 기준을 기다리는 중".into();
            self.observe("regions", actions);
        } else {
            self.phase = Phase::Seats;
            if self.mode == RunMode::Cancellation && self.cancellation_turn == 0
                && !matches!(self.profile.legacy.cancellation_navigation, CancellationNavigation::None)
            {
                if self.profile.features.nol_selection_count {
                    // First read any already-selected seats before navigation
                    // could discard them. OFF keeps the original navigation.
                    self.retry_cancellation = true;
                } else {
                    self.advance_cancellation(actions);
                    return;
                }
            }
            self.message = if self.starts_at_seat_map() {
                "현재 좌석 화면에서 바로 탐색 중"
            } else {
                "좌석 후보를 기다리는 중"
            }.into();
            self.observe("seats", actions);
        }
    }

    fn entry_observed(&mut self, stage: EntryStage, target: Option<Target>, enabled: bool, matched_value: Option<String>, screen_id: String, actions: &mut Vec<Action>) {
        if self.nol_captcha_enabled() && self.phase == Phase::Entry {
            // This route completes its registered input queue first. A generic
            // stage/F6 confirmation cannot discard that queue or skip its gate.
            return;
        }
        if stage == EntryStage::Captcha && matches!(self.phase, Phase::Entry | Phase::Region | Phase::WaitingRegion | Phase::Seats) {
            self.phase = Phase::Captcha;
            self.pending = None;
            self.current = None;
            self.candidates.clear();
            self.captcha_verified = false;
            self.message = "캡챠를 직접 입력하세요 · 완료 확인 대기 중".into();
            self.observe("captcha", actions);
            return;
        }
        if self.phase != Phase::Entry { return; }
        self.last_observation_ms = self.now_ms;
        if stage == EntryStage::SeatMap {
            self.initial_entry = InitialEntryState::Confirmed;
            self.after_entry(actions);
            return;
        }
        if matches!(stage, EntryStage::Queue | EntryStage::Unknown) {
            self.message = if stage == EntryStage::Queue { "예매 대기열에서 기다리는 중" } else { "확인되지 않은 예매 화면입니다. 사용할 버튼 확인을 기다립니다" }.into();
            return;
        }
        if !self.profile.features.auto_entry || self.pending.is_some() { return; }
        if self.has_initial_entry() && self.initial_entry == InitialEntryState::AwaitingMap
            && stage == EntryStage::ProductBooking && target == self.initial_clicked_target
        {
            self.message = "예매하기 클릭은 이미 전달되었습니다. 다음 예매 화면을 기다립니다".into();
            return;
        }
        if stage == EntryStage::Calendar && self.profile.desired_date.as_ref() != matched_value.as_ref() {
            self.message = "설정한 날짜와 일치하는 버튼을 기다리는 중".into();
            return;
        }
        if stage == EntryStage::Session && self.profile.desired_session.as_ref() != matched_value.as_ref() {
            self.message = "설정한 회차와 일치하는 버튼을 기다리는 중".into();
            return;
        }
        if matches!(stage, EntryStage::Calendar | EntryStage::Session) && matched_value.is_none() {
            self.message = "날짜·회차를 직접 선택하거나 정확한 희망 값을 설정하세요".into();
            return;
        }
        if !enabled {
            self.message = "예매 버튼이 아직 활성화되지 않았습니다".into();
            return;
        }
        let Some(target) = target.filter(Target::valid) else {
            self.message = "확인된 예매 버튼을 기다리는 중".into();
            return;
        };
        let key = format!("{stage:?}:{screen_id}:{}", serde_json::to_string(&target).unwrap_or_default());
        if self.last_entry.as_ref() == Some(&key) { return; }
        self.last_entry = Some(key);
        self.observe("entry", actions);
        self.issue(ActionKind::Click { target, purpose: format!("entry:{stage:?}") }, Effect::Entry, actions);
        let label = match stage {
            EntryStage::ProductBooking => "상품 예매하기",
            EntryStage::SaleGate => "판매 시작 화면의 예매하기",
            EntryStage::Calendar => "날짜",
            EntryStage::Session => "회차",
            EntryStage::ConfirmBooking => "날짜·회차 선택 후 예매하기",
            _ => "확인된 버튼",
        };
        self.message = format!("{label} 선택 중");
    }

    fn seat_result(&mut self, id: String, result: SeatOutcome, actions: &mut Vec<Action>) {
        if self.continuous_search() || self.profile.features.nol_selection_count {
            // Compatibility events can identify only the current active attempt for
            // this candidate. Adapters should use DeliveredSeatResult to distinguish
            // a late outcome from a retry of the same coordinate after refresh.
            self.optimistic_seat_result(id, None, result, actions);
            return;
        }
        let is_current = self.phase == Phase::Seats && self.current.as_ref() == Some(&id);
        let completion_conflict = self.phase == Phase::Completing && (self.selected.contains(&id) || self.attempted.contains(&id));
        let observed_again_after_cancellation = self.phase == Phase::Conflict
            && self.current.as_ref() == Some(&id) && !self.conflict_dismiss_requested;
        if !is_current && !completion_conflict && !observed_again_after_cancellation { return; }
        match result {
            SeatOutcome::Selected | SeatOutcome::LocallySelected if is_current => {
                self.remember_current_seat();
                self.pending = None;
                self.current = None;
                self.selected.insert(id);
                self.next_eligible_ms = self.now_ms.saturating_add(self.click_delay());
                self.last_observation_ms = self.now_ms;
                self.last_progress_ms = self.now_ms;
                // A fresh observation is required after each click: never trust a stale frame.
                self.candidates.clear();
                self.map_observed = false;
                self.select_next(actions);
                if actions.is_empty() {
                    self.message = format!("{} / {}석 선택 확인됨 · 다음 후보 확인 중", self.selected.len(), self.profile.target_seats);
                    self.observe("seats", actions);
                }
            }
            SeatOutcome::Conflict => {
                self.remember_current_seat();
                self.pending = None;
                self.current = Some(id);
                self.phase = Phase::Conflict;
                self.completion_submitted = false;
                self.candidates.clear();
                self.map_observed = false;
                if self.profile.features.conflict_recovery {
                    self.conflict_dismiss_requested = true;
                    self.observe("conflict_dialog", actions);
                    self.issue(ActionKind::Key { key: Key::Escape, purpose: "dismiss_known_seat_conflict".into() }, Effect::Escape, actions);
                    self.message = "선점 실패 확인됨 · Esc로 안내창 닫는 중".into();
                } else {
                    self.message = "선점 실패입니다. 선점 실패 복구가 꺼져 있으므로 직접 처리한 뒤 다시 시작하세요".into();
                }
            }
            SeatOutcome::Unknown => {
                self.message = "좌석 선택 결과를 확인할 수 없습니다. 추가 클릭 없이 기다립니다".into();
            }
            _ => {}
        }
    }

    fn optimistic_seat_result(&mut self, id: String, action_id: Option<u64>, result: SeatOutcome, actions: &mut Vec<Action>) {
        if !matches!(self.phase, Phase::Seats | Phase::Completing | Phase::Conflict) { return; }
        let matches_attempt = |attempt: &DeliveredSeatAttempt| {
            attempt.candidate_id == id && action_id.map_or(true, |value| attempt.action_id == value)
        };
        let Some(index) = self.delivered_attempts.iter().position(matches_attempt) else {
            // A proven-undelivered Esc can be retried for this same known dialog.
            // All other duplicates, unacknowledged seats and old action IDs are inert.
            if matches!(result, SeatOutcome::Conflict) && self.phase == Phase::Conflict
                && !self.conflict_dismiss_requested
                && self.conflict_attempt.as_ref().is_some_and(matches_attempt)
            {
                self.dismiss_optimistic_conflict(actions);
            }
            return;
        };
        match result {
            SeatOutcome::Selected | SeatOutcome::LocallySelected => {
                self.attempted.remove(&id);
                self.selected.insert(id);
            }
            SeatOutcome::Unknown => {}
            SeatOutcome::Conflict => {
                let attempt = self.delivered_attempts.remove(index);
                if self.profile.features.nol_selection_count {
                    if self.selection_count_state.accounted.remove(&attempt.action_id) {
                        self.selection_count_state.baseline = self.selection_count_state.baseline.and_then(|n| n.checked_sub(1));
                    }
                    if self.selection_count_state.awaiting.as_ref().is_some_and(|pending| pending.action_id == attempt.action_id) {
                        self.selection_count_state.awaiting = None;
                    }
                    self.invalidate_selection_count_request();
                }
                self.attempted.remove(&id);
                self.selected.remove(&id);
                self.rejected.insert(id.clone());
                self.last_rejected_retry_ms = self.now_ms;
                self.candidates.clear();
                self.map_observed = false;
                self.reconcile_selection = false;
                self.completion_submitted = false;
                self.last_observation_ms = self.now_ms;
                if self.phase == Phase::Conflict {
                    // One dialog dismissal suffices while multiple delivered seats
                    // are reported failed. Keep its queued Esc ID and other attempts.
                    if !self.conflict_dismiss_requested { self.dismiss_optimistic_conflict(actions); }
                    return;
                }
                // Invalidate the queued next-seat or completion ID before issuing Esc.
                // A late ACK/cancellation for that unsent input cannot count a seat.
                self.pending = None;
                self.current = Some(id);
                self.conflict_attempt = Some(attempt);
                self.phase = Phase::Conflict;
                self.conflict_dismiss_requested = false;
                self.dismiss_optimistic_conflict(actions);
            }
        }
    }

    fn dismiss_optimistic_conflict(&mut self, actions: &mut Vec<Action>) {
        if self.profile.features.conflict_recovery {
            self.conflict_dismiss_requested = true;
            self.observe("conflict_dialog", actions);
            self.issue(ActionKind::Key { key: Key::Escape, purpose: "dismiss_known_seat_conflict".into() }, Effect::Escape, actions);
            self.message = "실패한 좌석만 선택 추정에서 제외 · Esc로 안내창 닫는 중".into();
        } else {
            self.message = "실패한 좌석을 선택 추정에서 제외했습니다. 안내창을 직접 닫고 확인하면 남은 좌석을 계속 탐색합니다".into();
        }
    }

    fn remember_current_seat(&mut self) {
        let Some(candidate) = self.current_seat.as_ref()
            .filter(|candidate| self.current.as_ref() == Some(&candidate.id)) else { return; };
        if !self.seat_marks.iter().any(|(region, mark)|
            *region == self.active_region && mark.id == candidate.id) {
            self.seat_marks.push((self.active_region, candidate.clone()));
        }
    }

    fn prune_seat_marks(&mut self) {
        self.seat_marks.retain(|(_, candidate)| self.selected.contains(&candidate.id)
            || self.attempted.contains(&candidate.id) || self.rejected.contains(&candidate.id));
    }

    fn marked_seat(&self, candidate: &Candidate) -> bool {
        self.seat_marks.iter().any(|(region, mark)| *region == self.active_region
            && (self.selected.contains(&mark.id) || self.attempted.contains(&mark.id) || self.rejected.contains(&mark.id))
            && same_seat(mark, candidate))
    }

    fn select_next(&mut self, actions: &mut Vec<Action>) {
        if self.phase != Phase::Seats || self.pending.is_some() || self.current.is_some()
            || self.reconcile_selection || self.restart_observation_required
            || self.now_ms < self.next_eligible_ms { return; }
        if self.profile.features.nol_selection_count && !self.selection_count_state.permit_input { return; }
        if self.hold_excess_selection(actions) { return; }
        if self.selection_count() == self.profile.target_seats {
            self.phase = Phase::Completing;
            self.last_observation_ms = self.now_ms;
            self.submit_completion(actions);
            return;
        }
        let eligible: Vec<&Candidate> = self.candidates.iter().filter(|c| !c.selected
            && !self.selected.contains(&c.id) && !self.attempted.contains(&c.id)
            && !self.rejected.contains(&c.id) && !self.marked_seat(c)).collect();
        let index = if self.profile.features.adjacent_seats {
            let retained: Vec<&Candidate> = self.seat_marks.iter().filter_map(|(region, candidate)|
                (*region == self.active_region && (self.selected.contains(&candidate.id)
                    || self.attempted.contains(&candidate.id))).then_some(candidate)).collect();
            preferred_candidate(&eligible, &retained, self.profile.target_seats.saturating_sub(self.selection_count()))
        } else { 0 };
        if let Some(candidate) = eligible.get(index).map(|candidate| (*candidate).clone()) {
            self.current = Some(candidate.id.clone());
            self.current_seat = Some(candidate.clone());
            self.last_observation_ms = self.now_ms;
            self.observe("seat_result", actions);
            self.issue(ActionKind::Click { target: candidate.target, purpose: format!("seat:{}", candidate.id) }, Effect::Seat, actions);
            self.message = format!("좌석 선택 중 · {} / {}석", self.selection_count() + 1, self.profile.target_seats);
        } else {
            self.message = if self.map_observed {
                if self.profile.features.seat_objects {
                    "확인된 좌석 객체 후보 없음 · 같은 구역에서 계속 탐색 중"
                } else {
                    "현재 선택 가능한 좌석 없음 · 같은 구역에서 계속 탐색 중"
                }
            } else {
                "좌석 화면을 읽고 후보를 찾는 중"
            }.into();
        }
    }

    fn tick(&mut self, actions: &mut Vec<Action>) {
        if let Some(pending) = &self.pending {
            if self.now_ms.saturating_sub(pending.sent_at_ms) >= self.profile.action_timeout_ms {
                self.fault("입력 응답이 없습니다. 브라우저 연결과 입력 권한을 확인하세요");
            }
            return;
        }
        if self.phase == Phase::WaitingSale {
            if self.profile.sale_deadline_ms.is_some_and(|d| self.now_ms >= d) { self.begin(actions); }
            return;
        }
        if self.phase == Phase::WaitingRefresh && self.legacy_region_due_ms.is_some_and(|due| self.now_ms >= due) {
            self.legacy_region_due_ms = None;
            self.generation = self.generation.saturating_add(1);
            self.phase = Phase::Seats;
            self.candidates.clear();
            self.map_observed = false;
            self.last_progress_ms = self.now_ms;
            self.last_observation_ms = self.now_ms;
            self.next_eligible_ms = self.now_ms;
            self.message = "원본 구역 대기 시간 경과 · 현재 화면에서 좌석 탐색 재개".into();
            self.observe("seats", actions);
            return;
        }
        if self.phase == Phase::Entry && self.has_initial_entry()
            && !(self.has_captcha_gate()
                && self.initial_entry == InitialEntryState::AwaitingMap) {
            self.issue_initial_entry(actions);
            return;
        }
        if self.phase == Phase::Entry && (!self.profile.features.auto_entry
            || self.has_captcha_gate()) {
            self.advance_legacy(actions);
            return;
        }
        if self.phase == Phase::Seats {
            if self.hold_excess_selection(actions) { return; }
            if self.profile.features.nol_selection_count && (!self.selection_count_state.permit_input
                || !self.selection_count_state.verified || self.selection_count_state.awaiting.is_some()) {
                self.observe("seats", actions);
                return;
            }
            if self.retry_cancellation {
                self.retry_cancellation = false;
                self.advance_cancellation(actions);
                return;
            }
            if self.mode == RunMode::Cancellation && self.current.is_none()
                && !self.reconcile_selection && !self.rejected.is_empty()
                && self.now_ms.saturating_sub(self.last_rejected_retry_ms) >= self.profile.refresh_interval_ms.max(1)
            {
                // A new search pass may retry a failed coordinate. This is not
                // proof of a site refresh and does not alter retained selections.
                self.rejected.clear();
                self.prune_seat_marks();
                self.candidates.clear();
                self.map_observed = false;
                self.last_rejected_retry_ms = self.now_ms;
                self.message = "재탐색 간격 경과 · 이전 실패 좌석도 다시 탐색 중".into();
                self.observe("seats", actions);
                return;
            }
            self.select_next(actions);
            if !actions.is_empty() { return; }
            let refresh_delay = if self.mode == RunMode::Cancellation && !self.profile.features.automatic_refresh {
                0
            } else {
                self.profile.refresh_interval_ms
            };
            if self.current.is_none() && !self.reconcile_selection && self.map_observed
                && self.now_ms >= self.next_eligible_ms
                && self.now_ms.saturating_sub(self.last_progress_ms) >= refresh_delay
            {
                if self.mode == RunMode::Cancellation && self.profile.features.automatic_refresh {
                    if !self.selected.is_empty() || !self.attempted.is_empty()
                        || self.selection_count_state.baseline.is_some_and(|n| n > 0) {
                        self.message = "선택한 좌석을 유지하며 빈 좌석 탐색 중 · 내부 새로고침 보류".into();
                    } else if let Some(target @ Target::Point { .. }) = self.profile.refresh_target.clone() {
                        self.phase = Phase::WaitingRefresh;
                        self.observe("refresh", actions);
                        self.issue(ActionKind::Refresh { target }, Effect::Refresh, actions);
                        self.message = "좌석 화면 내부 버튼으로 새로고침 중".into();
                        return;
                    } else {
                        self.message = "좌석 탐색 중 · 내부 새로고침을 사용하려면 좌석 화면 새로고침 버튼을 지정하세요".into();
                    }
                } else if self.mode == RunMode::Cancellation {
                    self.advance_cancellation(actions);
                    if !actions.is_empty() { return; }
                }
            }
            if self.phase == Phase::Seats && self.current.is_none() && !self.reconcile_selection
                && self.now_ms >= self.next_eligible_ms
            {
                // Re-read an unchanged seat map periodically, even before any conflict.
                // This is observation only: a poll does not refresh the page, prove a
                // click outcome, or restore rejected candidates to this generation.
                if self.now_ms.saturating_sub(self.last_seat_search_ms) >= self.profile.refresh_interval_ms.max(1) {
                    self.observe("seats", actions);
                }
                return;
            }
        }
        if matches!(self.phase, Phase::Seats | Phase::WaitingRegion | Phase::Conflict | Phase::Completing | Phase::WaitingRefresh)
            && self.now_ms.saturating_sub(self.last_observation_ms) >= self.profile.observation_timeout_ms
        {
            self.message = match self.phase {
                Phase::WaitingRegion => "구역 좌석 화면이 보이면 F6을 누르세요 · 확인 후 좌석 탐색".into(),
                Phase::WaitingRefresh if self.legacy_region_due_ms.is_some() =>
                    "구역 이동 대기 중 · 지정한 대기 시간이 지나면 좌석 탐색".into(),
                Phase::WaitingRefresh => "새로고침이 끝나면 F6을 누르세요 · 확인 후 좌석 탐색".into(),
                Phase::Completing =>
                    "목표 수 도달 · 완료 버튼을 지정하거나 직접 진행 후 중지하세요".into(),
                phase => {
                    let label = match phase {
                        Phase::Conflict => "선점 실패 안내창 닫힘",
                        _ => "좌석 선택 결과",
                    };
                    format!("{label} 확인 대기 중 · 추가 클릭을 보내지 않습니다")
                }
            };
        }
    }

    fn advance_cancellation(&mut self, actions: &mut Vec<Action>) {
        // Preserve successful selections; changing regions may clear them.
        if self.mode != RunMode::Cancellation || !self.selected.is_empty()
            || !self.attempted.is_empty() || self.profile.features.region_priority
            || self.selection_count_state.baseline.is_some_and(|n| n > 0)
            || self.selection_count_state.awaiting.is_some() { return; }
        let region = self.cancellation_turn % 2;
        let previous_navigation = (self.active_region, self.cancellation_turn);
        let kind = match &self.profile.legacy.cancellation_navigation {
            CancellationNavigation::None => return,
            CancellationNavigation::TwoRegions { first, second } => ActionKind::Click {
                target: if self.cancellation_turn % 2 == 0 { first } else { second }.clone(),
                purpose: "legacy_cancellation_region".into(),
            },
            CancellationNavigation::ArrowKeys => ActionKind::Key {
                key: if self.cancellation_turn % 2 == 0 { Key::ArrowDown } else { Key::ArrowUp },
                purpose: "legacy_cancellation_region".into(),
            },
        };
        self.active_region = Some(region);
        self.cancellation_turn += 1;
        self.phase = Phase::WaitingRefresh;
        self.observe("refresh", actions);
        let effect = if self.blind_sequence() && !self.profile.features.automatic_refresh {
            Effect::LegacyRegion
        } else {
            Effect::Refresh
        };
        self.issue(kind, effect, actions);
        if let Some(pending) = &mut self.pending {
            pending.navigation_rollback = Some(previous_navigation);
        }
        self.message = "취소표 구역 전환 중 · 새 좌석 화면 확인 대기".into();
    }

    fn valid_candidate(&self, c: &Candidate) -> bool {
        !c.id.is_empty() && c.point.x.is_finite() && c.point.y.is_finite() && c.target.valid()
            && color_matches(c.color, self.effective_color(), self.profile.color_tolerance)
            && self.effective_vision().roi.map_or(true, |r| r.contains(c.point))
            && !self.effective_vision().exclusions.iter().any(|r| r.contains(c.point))
    }

    fn click_delay(&self) -> u64 {
        match self.mode {
            RunMode::Normal | RunMode::Seats => self.profile.legacy.normal_click_delay_ms,
            RunMode::Cancellation => self.profile.legacy.cancellation_click_delay_ms,
        }
    }

    fn blind_sequence(&self) -> bool {
        self.profile.legacy.blind_click_sequence && !self.profile.features.conflict_recovery
    }

    fn submit_completion(&mut self, actions: &mut Vec<Action>) {
        if self.hold_excess_selection(actions) { return; }
        if self.restart_observation_required { self.observe("seats", actions); return; }
        if self.profile.features.nol_selection_count && (!self.selection_count_state.permit_input
            || !self.selection_count_state.verified || self.selection_count_state.awaiting.is_some()
            || self.selection_count_state.baseline != Some(self.profile.target_seats)) {
            self.observe("seats", actions);
            return;
        }
        self.observe("completion", actions);
        if let Some(target) = self.effective_completion_target().cloned() {
            self.completion_submitted = true;
            self.issue(ActionKind::CompleteSelection { target }, Effect::Complete, actions);
            self.message = "선택 완료 진행 중".into();
        } else {
            self.message = "목표 수 도달 · 완료 버튼을 지정하거나 직접 진행 후 중지하세요".into();
        }
    }

    fn selection_count(&self) -> usize {
        if self.profile.features.nol_selection_count { self.selection_count_state.baseline.unwrap_or(0) }
        else { self.selected.union(&self.attempted).count() }
    }

    fn hold_excess_selection(&mut self, actions: &mut Vec<Action>) -> bool {
        let count = self.selection_count();
        if count <= self.profile.target_seats { return false; }
        self.message = format!("유지한 좌석 기록 {count}석이 새 목표 {}석보다 많습니다 · 추가 선택과 완료를 보류합니다", self.profile.target_seats);
        self.observe("seats", actions);
        true
    }

    fn running(&self) -> bool {
        !matches!(self.phase, Phase::Idle | Phase::Paused | Phase::Stopped | Phase::Completed | Phase::Faulted)
    }

    fn scope(&self) -> &'static str {
        match self.phase {
            Phase::Entry | Phase::WaitingSale => "entry",
            Phase::Captcha => "captcha",
            Phase::Region => "regions",
            Phase::WaitingRegion => "region_transition",
            Phase::Conflict => "conflict_dialog",
            Phase::Completing => "completion",
            Phase::WaitingRefresh => "refresh",
            _ => "seats",
        }
    }

    fn observe(&mut self, scope: &str, actions: &mut Vec<Action>) {
        if scope == "seats" { self.last_seat_search_ms = self.now_ms; }
        let id = self.next_id;
        self.next_id += 1;
        actions.push(Action { id, kind: ActionKind::Observe { scope: scope.into() } });
    }

    fn issue(&mut self, kind: ActionKind, effect: Effect, actions: &mut Vec<Action>) {
        let id = self.next_id;
        self.next_id += 1;
        self.pending = Some(Pending { id, sent_at_ms: self.now_ms, effect, kind: kind.clone(), navigation_rollback: None });
        actions.push(Action { id, kind });
    }

    fn fault(&mut self, message: &str) {
        self.pending = None;
        self.phase = Phase::Faulted;
        self.message = message.into();
    }
}
