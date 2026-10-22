use macro_core::Phase;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub const KEYBOARD_CHECK_DURATION_MS: u64 = 20_000;

pub fn validate_start(phase: Phase, capture_armed: bool, bound: bool) -> Result<(), String> {
    if !matches!(phase, Phase::Idle | Phase::Stopped | Phase::Completed | Phase::Faulted) {
        return Err("키보드 수신 검사: 예매 실행을 중지한 뒤 시작하세요.".into());
    }
    if capture_armed { return Err("키보드 수신 검사: 좌표 지정을 먼저 마치거나 중지하세요.".into()); }
    if !bound { return Err("키보드 수신 검사: 먼저 예매창을 브라우저 연결로 지정하세요.".into()); }
    Ok(())
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    #[default]
    Idle,
    Listening,
    Received,
    NoInput,
    Filtered,
    Error,
    Cancelled,
}

#[derive(Default)]
pub struct KeyboardCheck {
    deadline_ms: Option<u64>,
    status: Status,
    message: String,
    callbacks: u64,
    received: u64,
    accepted: u64,
    rejections: BTreeMap<String, u64>,
    environment: Value,
}

impl KeyboardCheck {
    pub fn active(&self) -> bool { self.deadline_ms.is_some() }
    pub fn message(&self) -> &str { &self.message }
    pub fn expired(&self, now_ms: u64) -> bool { self.deadline_ms.is_some_and(|deadline| now_ms >= deadline) }
    pub fn start(&mut self, now_ms: u64, environment: Value) {
        *self = Self { deadline_ms: Some(now_ms.saturating_add(KEYBOARD_CHECK_DURATION_MS)),
            status: Status::Listening,
            message: "키보드 수신 검사 중 · 연결한 예매창의 입력칸에 문자를 입력하세요 · F4로 중지".into(),
            environment, ..Self::default() };
    }
    pub fn environment(&mut self, value: Value) -> bool {
        if self.environment == value { return false; }
        self.environment = value;
        true
    }
    pub fn diagnostic(&mut self, diagnostic: &macro_platform::EnterDiagnostic, token: u64) -> bool {
        if diagnostic.token != Some(token) { return false; }
        let stage = diagnostic.stage.as_str();
        let reason = diagnostic.reason.as_str();
        if stage == "callback" { self.callbacks += 1; }
        if reason != "enqueued" { self.reject(&format!("{stage}:{reason}")); }
        true
    }
    pub fn received(&mut self) { self.received += 1; }
    pub fn accept(&mut self) {
        self.accepted += 1;
        self.status = Status::Received;
        self.message = format!("키보드 수신 확인 · 예매창 입력 {}회 · 캡챠 화면 판단은 별도입니다", self.accepted);
    }
    pub fn reject(&mut self, reason: &str) {
        *self.rejections.entry(reason.into()).or_default() += 1;
        if self.accepted == 0 {
            self.status = Status::Filtered;
            self.message = "입력 신호가 검사 조건에서 제외됨 · 검사 결과의 제외 사유를 확인하세요".into();
        }
    }
    pub fn finish(&mut self) {
        self.deadline_ms = None;
        if self.accepted > 0 {
            self.status = Status::Received;
            self.message = format!("키보드 검사 완료 · 예매창 입력 {}회 수신 · 캡챠 화면 판단은 검사하지 않았습니다", self.accepted);
        } else if self.callbacks > 0 || self.received > 0 || !self.rejections.is_empty() {
            self.status = Status::Filtered;
            self.message = "키보드 검사 완료 · 신호는 관측됐지만 예매창 입력으로 확인되지 않았습니다".into();
        } else {
            self.status = Status::NoInput;
            self.message = "키보드 검사 완료 · 앱에서 문자 입력을 수신하지 못했습니다 · 권한 상태를 확인하세요".into();
        }
    }
    pub fn cancel(&mut self) {
        self.deadline_ms = None;
        self.status = Status::Cancelled;
        self.message = "키보드 수신 검사를 중지했습니다".into();
    }
    pub fn error(&mut self, message: String) {
        self.deadline_ms = None;
        self.status = Status::Error;
        self.message = message;
    }
    pub fn snapshot(&self, now_ms: u64) -> Value {
        json!({"active":self.active(),"status":self.status,"message":self.message,
            "deadlinePolicy":"20초 경과 뒤 마지막 대기 큐 확인 · 작업 스케줄에 따라 종료 시각이 달라질 수 있습니다",
            // Only the display is rounded. Polling still uses the exact deadline.
            "remainingMs":self.deadline_ms.map_or(0, |deadline| deadline.saturating_sub(now_ms).div_ceil(1000) * 1000),
            "callbacks":self.callbacks,"received":self.received,"accepted":self.accepted,
            "rejections":self.rejections,"environment":self.environment})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diagnostic(stage: &str, reason: &str, token: Option<u64>) -> macro_platform::EnterDiagnostic {
        macro_platform::EnterDiagnostic {
            stage: stage.into(), reason: reason.into(), token, window_id: None,
            source_pid: None, recipient_pid: None, front_pid: None, repeated: None,
            modifiers: None, observed_at_ns: None, age_ns: None,
        }
    }

    #[test]
    fn check_restricts_start_to_inactive_bound_runs_without_capture() {
        for phase in [Phase::Idle, Phase::Stopped, Phase::Completed, Phase::Faulted] {
            assert!(validate_start(phase, false, true).is_ok());
        }
        for phase in [Phase::Paused, Phase::Entry, Phase::Captcha, Phase::Seats] {
            assert!(validate_start(phase, false, true).is_err());
        }
        assert!(validate_start(Phase::Idle, true, true).is_err());
        assert!(validate_start(Phase::Idle, false, false).is_err());
    }

    #[test]
    fn check_counts_callback_queue_and_target_acceptance_separately() {
        let mut check = KeyboardCheck::default();
        check.start(100, json!({"secureEventInput":false}));
        assert_eq!(check.snapshot(100)["remainingMs"], 20_000);
        assert!(!check.diagnostic(&diagnostic("callback", "enqueued", Some(6)), 7));
        assert!(!check.diagnostic(&diagnostic("callback", "inactive_monitor", None), 7));
        assert_eq!(check.snapshot(100)["callbacks"], 0);
        assert!(check.snapshot(100)["rejections"].as_object().unwrap().is_empty());
        check.diagnostic(&diagnostic("callback", "enqueued", Some(7)), 7);
        check.diagnostic(&diagnostic("callback", "recipient_mismatch", Some(7)), 7);
        check.diagnostic(&diagnostic("queue", "stale_event", Some(7)), 7);
        check.received();
        check.reject("runtime:window_mismatch");
        assert_eq!(check.snapshot(200)["callbacks"], 2);
        assert_eq!(check.snapshot(200)["received"], 1);
        assert_eq!(check.snapshot(200)["accepted"], 0);
        assert_eq!(check.snapshot(200)["rejections"]["queue:stale_event"], 1);
        assert!(!check.expired(20_099));
        assert!(check.expired(20_100));
        check.finish();
        assert_eq!(check.snapshot(20_100)["status"], "filtered");
        assert!(!check.active());
        assert!(check.environment(json!({"secureEventInput":true})));
        assert!(!check.environment(json!({"secureEventInput":true})));
    }

    #[test]
    fn check_accepts_windows_signals_without_callback_diagnostics_and_resets_between_runs() {
        let mut check = KeyboardCheck::default();
        check.start(0, json!({"platform":"windows"}));
        check.received();
        check.accept();
        check.finish();
        assert_eq!(check.snapshot(20_000)["status"], "received");
        assert_eq!(check.snapshot(20_000)["callbacks"], 0);
        assert_eq!(check.snapshot(20_000)["accepted"], 1);
        check.start(30_000, Value::Null);
        assert_eq!(check.snapshot(30_000)["accepted"], 0);
        assert!(check.expired(50_000));
        // The runtime drains its last native batch before finish, so an input
        // already waiting at the deadline is included in the final result.
        check.received();
        check.accept();
        check.finish();
        assert_eq!(check.snapshot(50_000)["status"], "received");
        check.start(50_000, Value::Null);
        check.finish();
        assert_eq!(check.snapshot(50_001)["status"], "no_input");
        check.start(60_000, Value::Null);
        check.cancel();
        assert_eq!(check.snapshot(60_001)["status"], "cancelled");
        check.start(70_000, Value::Null);
        check.error("capture is not used".into());
        assert_eq!(check.snapshot(70_001)["status"], "error");
        assert!(!check.active());
    }
}
