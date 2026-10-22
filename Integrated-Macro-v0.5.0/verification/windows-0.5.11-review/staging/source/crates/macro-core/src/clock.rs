//! Bounded HTTP Date estimates. The Date header may come from a cache/proxy rather than
//! the booking scheduler; callers must display uncertainty and never promise exact opening.
use crate::model::ValidationError;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HttpClockSample {
    pub sent_wall_ms: i64,
    pub sent_monotonic_ms: u64,
    pub received_monotonic_ms: u64,
    pub server_date_ms: i64,
    /// HTTP Date normally has one-second resolution (1000), not millisecond precision.
    pub precision_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClockEstimate {
    pub offset_ms: i64,
    pub uncertainty_ms: u64,
    pub best_round_trip_ms: u64,
    pub sample_count: usize,
    pub consistent: bool,
}

/// Uses the three fastest valid samples, avoiding a slow request dominating the estimate.
/// Inconsistent intervals expand uncertainty instead of presenting false precision.
pub fn estimate_clock(samples: &[HttpClockSample]) -> Result<ClockEstimate, ValidationError> {
    let mut intervals: Vec<(u64, i128, i128)> = samples.iter().filter_map(|s| {
        let rtt = s.received_monotonic_ms.checked_sub(s.sent_monotonic_ms)?;
        if rtt > 60_000 || s.precision_ms == 0 || s.precision_ms > 10_000 { return None; }
        let lower = s.server_date_ms as i128 - s.sent_wall_ms as i128 - rtt as i128;
        let upper = s.server_date_ms as i128 + s.precision_ms as i128 - s.sent_wall_ms as i128;
        Some((rtt, lower, upper))
    }).collect();
    intervals.sort_by_key(|s| s.0);
    intervals.truncate(3);
    if intervals.is_empty() { return Err(ValidationError("사용할 수 있는 서버 시간 측정값이 없습니다".into())); }
    let mut lower = intervals.iter().map(|s| s.1).max().unwrap();
    let mut upper = intervals.iter().map(|s| s.2).min().unwrap();
    let consistent = lower <= upper;
    if !consistent {
        lower = intervals.iter().map(|s| s.1).min().unwrap();
        upper = intervals.iter().map(|s| s.2).max().unwrap();
    }
    let midpoint = lower + (upper - lower) / 2;
    let offset_ms = i64::try_from(midpoint).map_err(|_| ValidationError("서버 시간 보정값이 지원 범위를 벗어났습니다".into()))?;
    let uncertainty_ms = u64::try_from((upper - lower + 1) / 2).map_err(|_| ValidationError("서버 시간 추정 오차가 지원 범위를 벗어났습니다".into()))?;
    Ok(ClockEstimate {
        offset_ms, uncertainty_ms,
        best_round_trip_ms: intervals[0].0,
        sample_count: intervals.len(),
        consistent,
    })
}

/// Call once when arming the run. Subsequent wall-clock changes do not shift this deadline.
/// Monotonic deadlines are process-specific and must be recomputed after application restart.
pub fn monotonic_deadline(sale_epoch_ms: i64, anchor_wall_ms: i64, anchor_monotonic_ms: u64, estimate: &ClockEstimate) -> Result<u64, ValidationError> {
    let remaining = (sale_epoch_ms as i128 - anchor_wall_ms as i128 - estimate.offset_ms as i128).max(0);
    let remaining = u64::try_from(remaining).map_err(|_| ValidationError("판매 시작 시각이 지원 범위를 벗어났습니다".into()))?;
    anchor_monotonic_ms.checked_add(remaining).ok_or_else(|| ValidationError("판매 시작 대기 시간이 지원 범위를 벗어났습니다".into()))
}
