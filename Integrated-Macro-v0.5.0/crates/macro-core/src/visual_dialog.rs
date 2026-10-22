//! Coarse visual-change evidence, not identification of a dialog or failed seat.
//! Sample the whole bound browser image without copying its full pixel buffer.
//! Center page updates/zoom can trigger the same signal; a small popup can be missed.
//! The caller retains a pre-click baseline and enforces newer observation sequences.

use crate::model::ValidationError;
use crate::vision::RgbaView;

const GRID_COLUMNS: usize = 64;
const GRID_ROWS: usize = 48;
const BASIS_POINTS: usize = 10_000;
const MIN_OPAQUE_BASIS_POINTS: usize = 9800;
const MIN_OPAQUE_ALPHA: u8 = 250;

#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct VisualChangeConfig {
    pub material_delta: u8,
    pub central_change_basis_points: u16,
    pub surrounding_darkening_delta: u8,
    pub surrounding_darkened_basis_points: u16,
    pub central_restored_basis_points: u16,
    pub surrounding_restored_basis_points: u16,
}

impl Default for VisualChangeConfig {
    fn default() -> Self {
        Self {
            material_delta: 32, central_change_basis_points: 1000,
            surrounding_darkening_delta: 32, surrounding_darkened_basis_points: 3500,
            central_restored_basis_points: 300, surrounding_restored_basis_points: 500,
        }
    }
}

impl VisualChangeConfig {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.material_delta == 0 || self.surrounding_darkening_delta == 0
            || self.central_change_basis_points == 0 || self.surrounding_darkened_basis_points == 0
            || self.central_change_basis_points as usize > BASIS_POINTS
            || self.surrounding_darkened_basis_points as usize > BASIS_POINTS
            || self.central_restored_basis_points >= self.central_change_basis_points
            || self.surrounding_restored_basis_points >= self.surrounding_darkened_basis_points
        {
            return Err(ValidationError("화면 변화 감지 기준이 올바르지 않습니다".into()));
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct VisualSample {
    pub sequence: u64,
    pub width: u32,
    pub height: u32,
    columns: usize,
    rows: usize,
    pixels: Vec<[u8; 4]>,
}

#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct VisualChangeReport {
    pub baseline_sequence: u64,
    pub sequence: u64,
    pub sample_count: usize,
    pub changed_samples: usize,
    pub changed_basis_points: u16,
    pub central_sample_count: usize,
    pub central_changed_samples: usize,
    pub central_changed_basis_points: u16,
    pub surrounding_sample_count: usize,
    pub surrounding_darkened_samples: usize,
    pub surrounding_darkened_basis_points: u16,
    pub trigger_kind: Option<VisualTriggerKind>,
    /// Either central change OR surrounding darkening; whole-image change is diagnostic only.
    pub large_change: bool,
    pub baseline_restored: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VisualTriggerKind { CentralChange, SurroundingDarkening, Both }

/// Take cell-center samples. Transparent RGB bytes are not visual evidence.
pub fn sample_frame(frame: RgbaView<'_>, sequence: u64) -> Result<VisualSample, ValidationError> {
    let width = frame.width as usize;
    let height = frame.height as usize;
    if width == 0 || height == 0
        || width.checked_mul(4).map_or(true, |row| frame.stride < row)
        || frame.stride.checked_mul(height) != Some(frame.rgba.len())
    {
        return Err(ValidationError("화면 변화 감지용 이미지 크기와 픽셀 데이터가 맞지 않습니다".into()));
    }
    let columns = width.min(GRID_COLUMNS);
    let rows = height.min(GRID_ROWS);
    let mut pixels = Vec::with_capacity(columns * rows);
    for row in 0..rows {
        let y = (((row * 2 + 1) as u64 * frame.height as u64) / (rows * 2) as u64) as usize;
        for column in 0..columns {
            let x = (((column * 2 + 1) as u64 * frame.width as u64) / (columns * 2) as u64) as usize;
            let offset = y * frame.stride + x * 4;
            pixels.push([frame.rgba[offset], frame.rgba[offset + 1], frame.rgba[offset + 2], frame.rgba[offset + 3]]);
        }
    }
    let mut central = (0, 0);
    let mut surrounding = (0, 0);
    for (index, pixel) in pixels.iter().enumerate() {
        let counts = if is_central(index, columns, rows) { &mut central } else { &mut surrounding };
        counts.0 += usize::from(pixel[3] >= MIN_OPAQUE_ALPHA);
        counts.1 += 1;
    }
    validate_opacity(central, surrounding)?;
    Ok(VisualSample { sequence, width: frame.width, height: frame.height, columns, rows, pixels })
}

/// Compare to the same pre-click baseline for both appearance and restoration.
/// This function does not replace the baseline after an ordinary seat color change.
/// Callers must additionally reject sequences at/before their last processed frame.
pub fn compare_samples(baseline: &VisualSample, current: &VisualSample, config: &VisualChangeConfig) -> Result<Option<VisualChangeReport>, ValidationError> {
    config.validate()?;
    if current.sequence <= baseline.sequence { return Ok(None); }
    if baseline.width != current.width || baseline.height != current.height
        || baseline.columns != current.columns || baseline.rows != current.rows
        || baseline.pixels.len() != current.pixels.len()
    {
        return Err(ValidationError("예매창 크기가 바뀌어 화면 변화 감지 기준을 다시 준비해야 합니다".into()));
    }
    let mut sample_count = 0;
    let mut changed_samples = 0;
    let mut central_sample_count = 0;
    let mut central_changed_samples = 0;
    let mut surrounding_darkened_samples = 0;
    let mut total_central = 0;
    for (index, (before, after)) in baseline.pixels.iter().zip(&current.pixels).enumerate() {
        let central = is_central(index, baseline.columns, baseline.rows);
        total_central += usize::from(central);
        if before[3] < MIN_OPAQUE_ALPHA || after[3] < MIN_OPAQUE_ALPHA { continue; }
        sample_count += 1;
        let changed = before[..3].iter().zip(&after[..3]).any(|(a, b)| a.abs_diff(*b) >= config.material_delta);
        if changed { changed_samples += 1; }
        if central {
            central_sample_count += 1;
            if changed { central_changed_samples += 1; }
        } else if brightness(before).saturating_sub(brightness(after)) >= config.surrounding_darkening_delta {
            surrounding_darkened_samples += 1;
        }
    }
    let surrounding_sample_count = sample_count - central_sample_count;
    // Each frame can have a few transparent edge samples. Require sufficient
    // overlapping evidence too, so missing center pixels cannot fake closure.
    validate_opacity((central_sample_count, total_central),
        (surrounding_sample_count, baseline.pixels.len() - total_central))?;
    let central_trigger = at_least(central_changed_samples, central_sample_count, config.central_change_basis_points);
    let darkening_trigger = at_least(surrounding_darkened_samples, surrounding_sample_count, config.surrounding_darkened_basis_points);
    let trigger_kind = match (central_trigger, darkening_trigger) {
        (true, true) => Some(VisualTriggerKind::Both),
        (true, false) => Some(VisualTriggerKind::CentralChange),
        (false, true) => Some(VisualTriggerKind::SurroundingDarkening),
        (false, false) => None,
    };
    Ok(Some(VisualChangeReport {
        baseline_sequence: baseline.sequence,
        sequence: current.sequence,
        sample_count,
        changed_samples,
        changed_basis_points: basis_points(changed_samples, sample_count),
        central_sample_count,
        central_changed_samples,
        central_changed_basis_points: basis_points(central_changed_samples, central_sample_count),
        surrounding_sample_count,
        surrounding_darkened_samples,
        surrounding_darkened_basis_points: basis_points(surrounding_darkened_samples, surrounding_sample_count),
        trigger_kind,
        large_change: trigger_kind.is_some(),
        baseline_restored: central_sample_count > 0
            && central_changed_samples * BASIS_POINTS <= central_sample_count * config.central_restored_basis_points as usize
            && surrounding_darkened_samples * BASIS_POINTS <= surrounding_sample_count * config.surrounding_restored_basis_points as usize,
    }))
}

// Weighted RGB brightness is a cheap visual heuristic, not calibrated luminance.
fn brightness(pixel: &[u8; 4]) -> u8 {
    ((pixel[0] as u32 * 54 + pixel[1] as u32 * 183 + pixel[2] as u32 * 19) / 256) as u8
}

fn is_central(index: usize, columns: usize, rows: usize) -> bool {
    let column = index % columns;
    let row = index / columns;
    (column * 2 + 1) * 10 >= columns * 6 && (column * 2 + 1) * 10 < columns * 14
        && (row * 2 + 1) * 10 >= rows * 6 && (row * 2 + 1) * 10 < rows * 14
}

fn validate_opacity(central: (usize, usize), surrounding: (usize, usize)) -> Result<(), ValidationError> {
    if [central, surrounding].iter().any(|(opaque, total)|
        *total > 0 && opaque * BASIS_POINTS < total * MIN_OPAQUE_BASIS_POINTS) {
        return Err(ValidationError("[capture_incomplete] 예매창 화면이 불완전합니다. 유효한 화면을 다시 확인합니다.".into()));
    }
    Ok(())
}

fn basis_points(count: usize, total: usize) -> u16 {
    if total == 0 { 0 } else { (count * BASIS_POINTS / total) as u16 }
}

fn at_least(count: usize, total: usize, threshold: u16) -> bool {
    total > 0 && count * BASIS_POINTS >= total * threshold as usize
}
