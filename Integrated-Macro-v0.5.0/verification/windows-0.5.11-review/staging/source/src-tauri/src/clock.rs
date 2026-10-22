use reqwest::blocking::Client;
use reqwest::header::{AGE, CACHE_CONTROL, DATE, PRAGMA};
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClockEstimate {
    pub url: String,
    pub measured_at_ms: u64,
    pub server_epoch_ms: i64,
    pub offset_ms: i64,
    pub rtt_ms: u64,
    pub uncertainty_ms: u64,
    pub source: String,
}

pub fn unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

impl ClockEstimate {
    pub fn deadline(&self, sale_time: &str, now_ms: u64) -> Result<u64, String> {
        if now_ms.saturating_sub(self.measured_at_ms) > 600_000 {
            return Err("서버 시간 측정이 10분 이상 지났습니다. ‘서버 시간 측정’을 다시 누르세요.".into());
        }
        let sale = parse_sale_time(sale_time)?;
        let delta = sale - self.server_epoch_ms;
        if delta < 0 {
            return Ok(now_ms);
        }
        Ok(self.measured_at_ms.saturating_add(delta as u64).max(now_ms))
    }
}

pub fn parse_sale_time(text: &str) -> Result<i64, String> {
    let date = OffsetDateTime::parse(text, &Rfc3339)
        .map_err(|_| "판매 시각을 시간대가 포함된 날짜·시간으로 입력하세요.".to_string())?;
    i64::try_from(date.unix_timestamp_nanos() / 1_000_000)
        .map_err(|_| "판매 시각의 범위를 확인하세요.".into())
}

/// HTTP Date has one-second resolution and may describe an edge server.
/// This reports that uncertainty instead of promising the booking server's open instant.
pub fn synchronize(url: &str, now: impl Fn() -> u64) -> Result<ClockEstimate, String> {
    let parsed = reqwest::Url::parse(url).map_err(|_| "사이트 주소를 확인하세요.".to_string())?;
    if parsed.scheme() != "https" || parsed.host_str().is_none()
        || !parsed.username().is_empty() || parsed.password().is_some()
    {
        return Err("사이트 주소에는 로그인 정보가 없는 HTTPS 주소를 입력하세요.".into());
    }
    let client = Client::builder()
        .timeout(Duration::from_secs(4))
        .redirect(reqwest::redirect::Policy::limited(3))
        .user_agent(concat!("IntegratedMacro/", env!("CARGO_PKG_VERSION"), " clock-check"))
        .build()
        .map_err(|e| format!("서버 시간 측정 준비 실패: {e}"))?;
    let mut samples = Vec::new();
    let mut last_error = "응답에 Date 헤더가 없습니다.".to_string();
    for _ in 0..3 {
        let start = now();
        let wall_start = unix_ms();
        let result = client.head(parsed.clone())
            .header(CACHE_CONTROL, "no-cache, no-store")
            .header(PRAGMA, "no-cache")
            .send();
        let response = match result {
            Ok(r) => r,
            Err(e) => { last_error = format!("사이트에 연결하지 못했습니다: {e}"); continue; }
        };
        let end = now();
        let rtt = end.saturating_sub(start);
        if !response.status().is_success() {
            last_error = format!("사이트가 시각 조회에 HTTP {}로 응답했습니다.", response.status());
            continue;
        }
        if response.url().scheme() != "https" {
            last_error = "사이트가 HTTPS가 아닌 주소로 이동했습니다.".into();
            continue;
        }
        if response.headers().get(AGE).map(|v| v.to_str().ok().and_then(|s| s.parse::<u64>().ok()))
            .is_some_and(|age| age != Some(0))
        {
            last_error = "캐시된 응답이어서 서버 시간 추정에 사용하지 않았습니다.".into();
            continue;
        }
        let date = response.headers().get(DATE).and_then(|v| v.to_str().ok())
            .and_then(|v| httpdate::parse_http_date(v).ok())
            .and_then(|v| v.duration_since(UNIX_EPOCH).ok());
        let Some(date) = date else { continue; };
        if rtt > 3000 {
            last_error = "서버 시간 응답이 너무 늦습니다. 다시 측정하세요.".into();
            continue;
        }
        // Date is truncated to seconds. Use its interval midpoint, not false ms precision.
        let server_epoch_ms = date.as_millis() as i64 + 500;
        samples.push(ClockEstimate {
            url: url.into(),
            measured_at_ms: start + rtt / 2,
            server_epoch_ms,
            offset_ms: server_epoch_ms - (wall_start + rtt as i64 / 2),
            rtt_ms: rtt,
            uncertainty_ms: 500 + rtt.div_ceil(2),
            source: "HTTP Date · 응답 서버 기준 추정".into(),
        });
    }
    samples.into_iter().min_by_key(|s| s.rtt_ms)
        .ok_or_else(|| format!("서버 시간 측정 실패: {last_error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timezone_is_explicit_and_equivalent() {
        assert_eq!(parse_sale_time("2026-10-22T12:00:00+09:00").unwrap(),
                   parse_sale_time("2026-10-22T03:00:00Z").unwrap());
        assert!(parse_sale_time("2026-10-22T12:00:00").is_err());
    }

    #[test]
    fn deadline_uses_monotonic_sample_and_rejects_stale_clock() {
        let epoch = parse_sale_time("2026-10-22T03:00:00Z").unwrap();
        let clock = ClockEstimate {
            url: "https://example.test".into(), measured_at_ms: 10_000,
            server_epoch_ms: epoch - 5000, offset_ms: 100, rtt_ms: 20,
            uncertainty_ms: 510, source: "test".into(),
        };
        assert_eq!(clock.deadline("2026-10-22T03:00:00Z", 11_000).unwrap(), 15_000);
        assert!(clock.deadline("2026-10-22T03:00:00Z", 611_000).is_err());
    }
}
