//! Tracks one visual incident against a delivered seat input.
//! The runtime owns input, phase checks and resets before completion/navigation.
//! These signals describe image changes, not proof of a server-side seat failure.

use macro_core::{
    visual_dialog::{compare_samples, VisualChangeConfig, VisualChangeReport, VisualSample},
    DeliveredSeatAttempt, ValidationError,
};

#[derive(Clone, Debug)]
pub enum VisualSignal {
    Suspected { attempt: DeliveredSeatAttempt, report: VisualChangeReport },
    Restored { report: VisualChangeReport },
}

#[derive(Clone, Debug)]
struct ArmedObservation {
    baseline: VisualSample,
    attempt: DeliveredSeatAttempt,
    last_sequence: u64,
    restarted_stream: bool,
    incident: bool,
}

#[derive(Default)]
pub struct VisualRecovery {
    config: VisualChangeConfig,
    armed: Option<ArmedObservation>,
    report: Option<VisualChangeReport>,
}

impl VisualRecovery {
    /// Called after the seat ACK with its pre-input image, never for queued input.
    /// An active incident keeps its original baseline and attributed attempt.
    pub fn arm(&mut self, sample: VisualSample, attempt: DeliveredSeatAttempt) -> bool {
        if self.is_incident() { return false; }
        self.armed = Some(ArmedObservation {
            last_sequence: sample.sequence,
            restarted_stream: false,
            baseline: sample,
            attempt,
            incident: false,
        });
        self.report = None;
        true
    }

    /// Only a newer, compatible image can begin or end an incident.
    /// Once suspected, further changed images cannot request another rollback/Esc.
    /// There is no timeout-based restoration or automatic input retry.
    pub fn observe(&mut self, sample: VisualSample) -> Result<Option<VisualSignal>, ValidationError> {
        let Some(armed) = self.armed.as_mut() else { return Ok(None); };
        if sample.sequence <= armed.last_sequence { return Ok(None); }
        // Native sequence numbers restart after a successful same-target bind.
        // Compare retained pixels against the new stream without changing the
        // original baseline provenance or any native/count frame metadata.
        let mut comparison_baseline;
        let baseline = if armed.restarted_stream {
            comparison_baseline = armed.baseline.clone();
            comparison_baseline.sequence = 0;
            &comparison_baseline
        } else { &armed.baseline };
        let Some(mut report) = compare_samples(baseline, &sample, &self.config)? else {
            return Ok(None);
        };
        report.baseline_sequence = armed.baseline.sequence;
        armed.last_sequence = sample.sequence;
        self.report = Some(report);
        if armed.incident {
            if report.baseline_restored {
                self.armed = None;
                return Ok(Some(VisualSignal::Restored { report }));
            }
        } else if report.large_change {
            armed.incident = true;
            return Ok(Some(VisualSignal::Suspected { attempt: armed.attempt.clone(), report }));
        }
        Ok(None)
    }

    pub fn is_armed(&self) -> bool { self.armed.is_some() }

    pub fn is_incident(&self) -> bool {
        self.armed.as_ref().is_some_and(|armed| armed.incident)
    }

    /// Call only after a successful bind to the same window and process. A
    /// known incident keeps its baseline pixels, original sequence provenance
    /// and attributed attempt; only new-stream ordering starts again at zero.
    /// Consequently report baseline/current sequences may belong to different
    /// capture streams. A failed bind must not call this method.
    pub fn same_target_stream_restarted(&mut self) {
        if let Some(armed) = self.armed.as_mut().filter(|armed| armed.incident) {
            armed.last_sequence = 0;
            armed.restarted_stream = true;
            self.report = None;
        } else { self.reset(); }
    }

    pub fn reset(&mut self) {
        self.armed = None;
        self.report = None;
    }

    pub fn latest_report(&self) -> Option<&VisualChangeReport> { self.report.as_ref() }

    pub fn status(&self) -> &'static str {
        if self.is_incident() { "incident" } else if self.is_armed() { "armed" } else { "idle" }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use macro_core::{
        vision::RgbaView, visual_dialog::sample_frame,
        Action, ActionKind, Candidate, Engine, Event, Key, Phase, Point, Profile,
        RunMode, SeatOutcome, Target,
    };

    fn sample(sequence: u64, popup: bool) -> VisualSample {
        let mut rgba = vec![200; 64 * 48 * 4];
        for (index, pixel) in rgba.chunks_exact_mut(4).enumerate() {
            pixel[3] = 255;
            let x = index % 64;
            let y = index / 64;
            if popup && (20..44).contains(&x) && (16..32).contains(&y) {
                pixel[..3].fill(30);
            }
        }
        sample_frame(RgbaView { width: 64, height: 48, stride: 64 * 4, rgba: &rgba }, sequence).unwrap()
    }

    fn attempt(action_id: u64, candidate_id: &str) -> DeliveredSeatAttempt {
        DeliveredSeatAttempt { action_id, candidate_id: candidate_id.into(), generation: 0 }
    }

    fn engine(target_seats: usize) -> Engine {
        let mut profile = Profile::default();
        profile.target_seats = target_seats;
        profile.features.conflict_recovery = true;
        profile.legacy.complete = Some(Target::Point { point: Point { x: 90.0, y: 90.0 } });
        let mut engine = Engine::new(profile).unwrap();
        engine.step(Event::Start { mode: RunMode::Seats }, 0);
        engine
    }

    fn candidates(ids: &[&str]) -> Event {
        Event::CandidatesObserved {
            generation: 0,
            candidates: ids.iter().enumerate().map(|(index, id)| {
                let point = Point { x: 10.0 + index as f64 * 10.0, y: 10.0 };
                Candidate {
                    id: (*id).into(), point, color: Profile::default().color,
                    target: Target::Point { point }, selected: false, geometry: None,
                }
            }).collect(),
        }
    }

    fn physical(actions: &[Action]) -> &Action {
        actions.iter().find(|action| !matches!(&action.kind, ActionKind::Observe { .. })).unwrap()
    }

    fn ack(engine: &mut Engine, id: u64, now: u64) -> Vec<Action> {
        engine.step(Event::ActionAcknowledged { id, success: true, detail: String::new() }, now)
    }

    fn conflict(attempt: &DeliveredSeatAttempt) -> Event {
        Event::DeliveredSeatResult {
            action_id: attempt.action_id, candidate_id: attempt.candidate_id.clone(),
            result: SeatOutcome::Conflict,
        }
    }

    #[test]
    fn no_signal_without_a_delivered_attempt_and_normal_images_keep_watch_armed() {
        let mut recovery = VisualRecovery::default();
        assert!(recovery.observe(sample(2, true)).unwrap().is_none());
        assert_eq!(recovery.status(), "idle");
        assert!(recovery.arm(sample(3, false), attempt(10, "seat-a")));
        assert!(recovery.observe(sample(4, false)).unwrap().is_none());
        assert!(recovery.is_armed());
        assert!(!recovery.is_incident());
        assert_eq!(recovery.status(), "armed");
        assert!(recovery.latest_report().unwrap().baseline_restored);
    }

    #[test]
    fn one_incident_emits_one_failure_and_only_a_new_restored_image_ends_it() {
        let mut recovery = VisualRecovery::default();
        let delivered = attempt(10, "seat-a");
        recovery.arm(sample(10, false), delivered.clone());
        let signal = recovery.observe(sample(11, true)).unwrap().unwrap();
        assert!(matches!(signal, VisualSignal::Suspected { attempt, report }
            if attempt == delivered && report.baseline_sequence == 10 && report.sequence == 11));
        assert_eq!(recovery.status(), "incident");
        assert!(recovery.observe(sample(11, false)).unwrap().is_none());
        assert!(recovery.observe(sample(9, false)).unwrap().is_none());
        assert!(recovery.observe(sample(12, true)).unwrap().is_none());
        assert!(recovery.is_incident());
        assert!(matches!(recovery.observe(sample(13, false)).unwrap(),
            Some(VisualSignal::Restored { report }) if report.baseline_sequence == 10 && report.sequence == 13));
        assert!(!recovery.is_armed());
        assert_eq!(recovery.latest_report().unwrap().sequence, 13);
        assert!(recovery.observe(sample(14, true)).unwrap().is_none());
    }

    #[test]
    fn an_incident_cannot_replace_its_baseline_or_attribute_failure_to_another_seat() {
        let mut recovery = VisualRecovery::default();
        recovery.arm(sample(1, false), attempt(10, "seat-a"));
        assert!(matches!(recovery.observe(sample(2, true)).unwrap(),
            Some(VisualSignal::Suspected { .. })));
        assert!(!recovery.arm(sample(3, true), attempt(20, "seat-b")));
        assert!(recovery.observe(sample(4, true)).unwrap().is_none());
        assert_eq!(recovery.latest_report().unwrap().baseline_sequence, 1);
        assert!(matches!(recovery.observe(sample(5, false)).unwrap(),
            Some(VisualSignal::Restored { .. })));
    }

    #[test]
    fn next_delivered_seat_replaces_armed_attempt_but_small_changes_do_not_end_watch() {
        let mut recovery = VisualRecovery::default();
        recovery.arm(sample(1, false), attempt(10, "seat-a"));
        assert!(recovery.observe(sample(2, false)).unwrap().is_none());
        let next = attempt(20, "seat-b");
        assert!(recovery.arm(sample(2, false), next.clone()));
        assert!(matches!(recovery.observe(sample(3, true)).unwrap(),
            Some(VisualSignal::Suspected { attempt, report })
                if attempt == next && report.baseline_sequence == 2));
    }

    #[test]
    fn geometry_errors_preserve_incident_and_do_not_fake_restoration() {
        let mut recovery = VisualRecovery::default();
        recovery.arm(sample(1, false), attempt(10, "seat-a"));
        recovery.observe(sample(2, true)).unwrap();
        let mut rgba = vec![200; 32 * 24 * 4];
        for pixel in rgba.chunks_exact_mut(4) { pixel[3] = 255; }
        let resized = sample_frame(RgbaView { width: 32, height: 24, stride: 32 * 4, rgba: &rgba }, 3).unwrap();
        assert!(recovery.observe(resized).is_err());
        assert!(recovery.is_incident());
        assert_eq!(recovery.latest_report().unwrap().sequence, 2);
        assert!(matches!(recovery.observe(sample(3, false)).unwrap(),
            Some(VisualSignal::Restored { .. })));
    }

    #[test]
    fn transparent_capture_preserves_incident_until_opaque_restoration() {
        let mut recovery = VisualRecovery::default();
        recovery.arm(sample(1, false), attempt(10, "seat-a"));
        recovery.observe(sample(2, true)).unwrap();
        let rgba = vec![0; 64 * 48 * 4];
        let invalid = sample_frame(RgbaView { width: 64, height: 48, stride: 64 * 4, rgba: &rgba }, 3);
        assert!(invalid.unwrap_err().to_string().contains("[capture_incomplete]"));
        assert!(recovery.is_incident());
        assert_eq!(recovery.latest_report().unwrap().sequence, 2);
        assert!(matches!(recovery.observe(sample(4, false)).unwrap(), Some(VisualSignal::Restored { .. })));
    }

    #[test]
    fn manual_reset_clears_incident_and_accepts_a_new_run_sequence() {
        let mut recovery = VisualRecovery::default();
        recovery.arm(sample(100, false), attempt(10, "seat-a"));
        recovery.observe(sample(101, true)).unwrap();
        recovery.reset();
        assert_eq!(recovery.status(), "idle");
        assert!(recovery.latest_report().is_none());
        assert!(recovery.observe(sample(102, true)).unwrap().is_none());
        assert!(recovery.arm(sample(1, false), attempt(1, "seat-new-run")));
        assert!(matches!(recovery.observe(sample(2, true)).unwrap(),
            Some(VisualSignal::Suspected { attempt, .. }) if attempt.candidate_id == "seat-new-run"));
    }

    #[test]
    fn visual_failure_cancels_queued_completion_preserves_other_seat_and_fills_deficit() {
        let mut engine = engine(2);
        let mut recovery = VisualRecovery::default();
        let first = engine.step(candidates(&["kept", "failed", "replacement"]), 1);
        let second = ack(&mut engine, physical(&first).id, 2);
        let completion = ack(&mut engine, physical(&second).id, 3);
        let old_completion = physical(&completion).id;
        assert!(matches!(&physical(&completion).kind, ActionKind::CompleteSelection { .. }));
        let delivered = engine.snapshot().delivered_attempts.last().unwrap().clone();
        recovery.arm(sample(10, false), delivered);
        let Some(VisualSignal::Suspected { attempt, .. }) = recovery.observe(sample(11, true)).unwrap() else {
            panic!("popup must produce an incident");
        };
        let dismiss = engine.step(conflict(&attempt), 4);
        let escape = physical(&dismiss).id;
        assert!(matches!(&physical(&dismiss).kind, ActionKind::Key { key: Key::Escape, .. }));
        assert_ne!(escape, old_completion);
        assert!(ack(&mut engine, old_completion, 5).is_empty());
        assert_eq!(engine.snapshot().attempted_ids, vec!["kept"]);
        assert_eq!(engine.snapshot().rejected_ids, vec!["failed"]);
        ack(&mut engine, escape, 6);
        assert!(recovery.observe(sample(12, true)).unwrap().is_none());
        assert!(matches!(recovery.observe(sample(13, false)).unwrap(),
            Some(VisualSignal::Restored { .. })));
        engine.step(Event::ConflictDialogGone, 7);
        let replacement = engine.step(candidates(&["kept", "failed", "replacement"]), 8);
        assert!(matches!(&physical(&replacement).kind,
            ActionKind::Click { purpose, .. } if purpose == "seat:replacement"));
        ack(&mut engine, physical(&replacement).id, 9);
        assert_eq!(engine.snapshot().attempted_ids, vec!["kept", "replacement"]);
        assert!(engine.snapshot().selected_ids.is_empty());
        assert_eq!(engine.snapshot().phase, Phase::Completing);
    }

    #[test]
    fn interrupted_unsent_escape_retries_after_resume_without_second_rollback_or_sent_escape() {
        let mut engine = engine(1);
        let mut recovery = VisualRecovery::default();
        let click = engine.step(candidates(&["failed"]), 1);
        ack(&mut engine, physical(&click).id, 2);
        let delivered = engine.snapshot().delivered_attempts[0].clone();
        recovery.arm(sample(10, false), delivered);
        let Some(VisualSignal::Suspected { attempt, .. }) = recovery.observe(sample(11, true)).unwrap() else {
            panic!("popup must produce an incident");
        };
        let first_escape = engine.step(conflict(&attempt), 3);
        let cancelled = physical(&first_escape).id;
        engine.step(Event::ActionCancelled { id: cancelled }, 4);
        engine.step(Event::ContextLost { reason: "foreground temporarily changed".into() }, 5);
        assert_eq!(engine.snapshot().phase, Phase::Paused);
        assert!(recovery.is_incident());
        engine.step(Event::Resume, 6);
        assert!(recovery.observe(sample(12, true)).unwrap().is_none());
        let retry = engine.step(conflict(&attempt), 7);
        let delivered_escape = physical(&retry).id;
        assert_ne!(delivered_escape, cancelled);
        assert!(matches!(&physical(&retry).kind, ActionKind::Key { key: Key::Escape, .. }));
        assert_eq!(engine.snapshot().rejected_ids, vec!["failed"]);
        assert!(engine.snapshot().attempted_ids.is_empty());
        ack(&mut engine, delivered_escape, 8);
        assert!(engine.step(conflict(&attempt), 9).is_empty());
        assert!(matches!(recovery.observe(sample(13, false)).unwrap(),
            Some(VisualSignal::Restored { .. })));
        engine.step(Event::ConflictDialogGone, 10);
        assert_eq!(engine.snapshot().phase, Phase::Seats);
        assert!(engine.step(candidates(&["failed"]), 11).iter()
            .all(|action| matches!(&action.kind, ActionKind::Observe { .. })));
    }
}
