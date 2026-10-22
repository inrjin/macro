//! Pure RGB geometry. A matching pixel component is a candidate, not proof it is a seat.
use crate::model::*;
use std::collections::VecDeque;

#[derive(Clone, Debug)]
pub struct RgbaFrame {
    pub width: u32,
    pub height: u32,
    /// Tightly packed, top-to-bottom RGBA8. Native adapters remove row padding.
    pub rgba: Vec<u8>,
}

#[derive(Clone, Copy, Debug)]
pub struct RgbaView<'a> {
    pub width: u32,
    pub height: u32,
    pub stride: usize,
    pub rgba: &'a [u8],
}

/// Counts only eligible pixels inside the clipped ROI, after exclusions and alpha.
/// These describe color geometry; they do not establish that a component is a seat.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct ExtractionStats {
    pub matching_pixels: usize,
    pub components_found: usize,
    pub accepted_count: usize,
    pub rejected_too_small: usize,
    pub rejected_too_large: usize,
    pub rejected_unsupported: usize,
}

#[derive(Clone, Debug)]
pub struct CandidateExtraction {
    pub candidates: Vec<Candidate>,
    pub stats: ExtractionStats,
}

pub fn color_matches(actual: Color, target: Color, tolerance: u8) -> bool {
    actual.r.abs_diff(target.r) <= tolerance
        && actual.g.abs_diff(target.g) <= tolerance
        && actual.b.abs_diff(target.b) <= tolerance
}

/// Coordinates and exclusions are expressed in physical pixels of this captured frame.
pub fn extract_candidates(frame: &RgbaFrame, color: Color, tolerance: u8, config: &VisionConfig, generation: u64) -> Result<Vec<Candidate>, ValidationError> {
    extract_candidates_view(RgbaView {
        width: frame.width, height: frame.height, stride: frame.width as usize * 4, rgba: &frame.rgba,
    }, color, tolerance, config, generation)
}

/// Borrow a native capture without cloning or packing the entire pixel buffer.
pub fn extract_candidates_view(frame: RgbaView<'_>, color: Color, tolerance: u8, config: &VisionConfig, generation: u64) -> Result<Vec<Candidate>, ValidationError> {
    extract_candidates_view_with_stats(frame, color, tolerance, config, generation)
        .map(|extraction| extraction.candidates)
}

/// Report recognition diagnostics from the same pixel and component traversal.
pub fn extract_candidates_view_with_stats(frame: RgbaView<'_>, color: Color, tolerance: u8, config: &VisionConfig, _generation: u64) -> Result<CandidateExtraction, ValidationError> {
    extract_view(frame, color, tolerance, config, false)
}

/// Seat candidates need an opaque matching 2x2 patch. This is a geometric
/// filter, not proof that a site seat is available or that a click succeeded.
pub fn extract_seat_candidates_view_with_stats(frame: RgbaView<'_>, color: Color, tolerance: u8, config: &VisionConfig, _generation: u64) -> Result<CandidateExtraction, ValidationError> {
    extract_view(frame, color, tolerance, config, true)
}

fn validate_inputs(frame: RgbaView<'_>, config: &VisionConfig) -> Result<(), ValidationError> {
    let full_width = frame.width as usize;
    let full_height = frame.height as usize;
    if full_width == 0 || full_height == 0 || full_width.checked_mul(4).map_or(true, |row| frame.stride < row)
        || frame.stride.checked_mul(full_height) != Some(frame.rgba.len())
    {
        return Err(ValidationError("캡처 이미지 크기와 픽셀 데이터 길이가 맞지 않습니다".into()));
    }
    if config.min_component_pixels == 0 || config.max_component_pixels < config.min_component_pixels
        || config.roi.is_some_and(|r| !r.valid()) || config.exclusions.iter().any(|r| !r.valid())
    {
        return Err(ValidationError("좌석 탐색 범위 또는 색상 영역 크기 설정이 올바르지 않습니다".into()));
    }
    Ok(())
}

fn supported_pixel(frame: RgbaView<'_>, x: usize, y: usize, color: Color, tolerance: u8, config: &VisionConfig) -> bool {
    if x >= frame.width as usize || y >= frame.height as usize { return false; }
    let point = Point { x: x as f64, y: y as f64 };
    if config.roi.is_some_and(|roi| !roi.contains(point))
        || config.exclusions.iter().any(|exclusion| exclusion.contains(point)) { return false; }
    let offset = y * frame.stride + x * 4;
    let pixel = &frame.rgba[offset..offset + 4];
    pixel[3] >= 250 && color_matches(Color { r: pixel[0], g: pixel[1], b: pixel[2] }, color, tolerance)
}

/// Recheck the exact click pixel against its four possible enclosing 2x2
/// patches, including ROI/exclusions and opacity. No capture or input occurs.
pub fn seat_point_supported(frame: RgbaView<'_>, point: Point, color: Color, tolerance: u8, config: &VisionConfig) -> bool {
    if validate_inputs(frame, config).is_err() || !point.x.is_finite() || !point.y.is_finite()
        || point.x < 0.0 || point.y < 0.0 || point.x >= frame.width as f64 || point.y >= frame.height as f64
        || point.x.fract() != 0.0 || point.y.fract() != 0.0 { return false; }
    let x = point.x as usize;
    let y = point.y as usize;
    for top in [y.checked_sub(1), Some(y)].into_iter().flatten() {
        for left in [x.checked_sub(1), Some(x)].into_iter().flatten() {
            if left + 1 < frame.width as usize && top + 1 < frame.height as usize
                && [(left, top), (left + 1, top), (left, top + 1), (left + 1, top + 1)].into_iter()
                    .all(|(x, y)| supported_pixel(frame, x, y, color, tolerance, config)) { return true; }
        }
    }
    false
}

fn extract_view(frame: RgbaView<'_>, color: Color, tolerance: u8, config: &VisionConfig, seats: bool) -> Result<CandidateExtraction, ValidationError> {
    validate_inputs(frame, config)?;
    let full_width = frame.width as usize;
    let full_height = frame.height as usize;
    // Allocate and inspect only the ROI, while borrowing the full native buffer.
    let (origin_x, origin_y, end_x, end_y) = if let Some(roi) = config.roi {
        (
            roi.x.ceil().clamp(0.0, full_width as f64) as usize,
            roi.y.ceil().clamp(0.0, full_height as f64) as usize,
            (roi.x + roi.width).ceil().clamp(0.0, full_width as f64) as usize,
            (roi.y + roi.height).ceil().clamp(0.0, full_height as f64) as usize,
        )
    } else { (0, 0, full_width, full_height) };
    let mut stats = ExtractionStats::default();
    if end_x <= origin_x || end_y <= origin_y {
        return Ok(CandidateExtraction { candidates: vec![], stats });
    }
    let width = end_x - origin_x;
    let height = end_y - origin_y;
    let count = width.checked_mul(height).ok_or_else(|| ValidationError("캡처 이미지 크기가 지원 범위를 벗어났습니다".into()))?;
    let mut eligible = vec![false; count];
    for (index, allowed) in eligible.iter_mut().enumerate() {
        let point = Point { x: (origin_x + index % width) as f64, y: (origin_y + index / width) as f64 };
        let offset = (origin_y + index / width) * frame.stride + (origin_x + index % width) * 4;
        let pixel = &frame.rgba[offset..offset + 4];
        *allowed = (if seats { pixel[3] >= 250 } else { pixel[3] != 0 })
            && color_matches(Color { r: pixel[0], g: pixel[1], b: pixel[2] }, color, tolerance)
            && config.roi.map_or(true, |r| r.contains(point))
            && !config.exclusions.iter().any(|r| r.contains(point));
        if *allowed { stats.matching_pixels += 1; }
    }
    // Retain the original eligibility while the flood fill consumes its copy.
    // Four adjacent eligible pixels belong to the same connected component.
    let supported = seats.then(|| {
        let mut supported = vec![false; count];
        for y in 0..height.saturating_sub(1) {
            for x in 0..width.saturating_sub(1) {
                let i = y * width + x;
                if eligible[i] && eligible[i + 1] && eligible[i + width] && eligible[i + width + 1] {
                    for index in [i, i + 1, i + width, i + width + 1] { supported[index] = true; }
                }
            }
        }
        supported
    });
    let mut result = vec![];
    let mut queue = VecDeque::new();
    for start in 0..count {
        if !eligible[start] { continue; }
        stats.components_found += 1;
        eligible[start] = false;
        queue.push_back(start);
        let mut pixels = vec![];
        let mut component_pixels = 0;
        let mut sum_x = 0.0;
        let mut sum_y = 0.0;
        let mut min_x = width;
        let mut min_y = height;
        let mut max_x = 0;
        let mut max_y = 0;
        while let Some(index) = queue.pop_front() {
            let x = index % width;
            let y = index / width;
            component_pixels += 1;
            // Still visit the whole rejected component so it cannot be split
            // into false seat candidates, but retain geometry only while it
            // could fit the configured limit.
            if component_pixels <= config.max_component_pixels {
                pixels.push(index);
                sum_x += x as f64;
                sum_y += y as f64;
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
            let neighbors = [
                if x > 0 { Some(index - 1) } else { None },
                if x + 1 < width { Some(index + 1) } else { None },
                if y > 0 { Some(index - width) } else { None },
                if y + 1 < height { Some(index + width) } else { None },
            ];
            for next in neighbors.into_iter().flatten() {
                if eligible[next] {
                    eligible[next] = false;
                    queue.push_back(next);
                }
            }
        }
        if component_pixels < config.min_component_pixels {
            stats.rejected_too_small += 1;
            continue;
        }
        if component_pixels > config.max_component_pixels {
            stats.rejected_too_large += 1;
            continue;
        }
        let center = Point { x: sum_x / pixels.len() as f64, y: sum_y / pixels.len() as f64 };
        // A ring/concave shape's centroid may be empty. Click an actual matched pixel.
        let selected = pixels.iter().filter(|index| supported.as_ref().is_none_or(|mask| mask[**index])).min_by(|a, b| {
            distance_squared(**a, width, center).total_cmp(&distance_squared(**b, width, center))
        });
        let Some(&selected) = selected else {
            stats.rejected_unsupported += 1;
            continue;
        };
        let point = Point { x: (origin_x + selected % width) as f64, y: (origin_y + selected / width) as f64 };
        let offset = (origin_y + selected / width) * frame.stride + (origin_x + selected % width) * 4;
        let actual = &frame.rgba[offset..offset + 4];
        result.push(Candidate {
            // Refresh generation is event metadata, not part of a seat's identity.
            // Adapters must rebind/stop on a different map geometry/context.
            id: format!("pixel:{}:{}", origin_x + min_x, origin_y + min_y),
            point,
            color: Color { r: actual[0], g: actual[1], b: actual[2] },
            target: Target::Point { point },
            selected: false,
            geometry: Some(CandidateGeometry {
                bounds: Rect { x: (origin_x + min_x) as f64, y: (origin_y + min_y) as f64,
                    width: (max_x - min_x + 1) as f64, height: (max_y - min_y + 1) as f64 },
                pixels: component_pixels,
                mask: seats.then(|| std::sync::Arc::new(ComponentMask::from_roi_pixels(
                    &mut pixels, width, origin_x, origin_y))),
            }),
        });
        stats.accepted_count += 1;
    }
    rank_seats(&mut result);
    Ok(CandidateExtraction { candidates: result, stats })
}

fn distance_squared(index: usize, width: usize, p: Point) -> f64 {
    ((index % width) as f64 - p.x).powi(2) + ((index / width) as f64 - p.y).powi(2)
}

pub fn rank_seats(candidates: &mut [Candidate]) {
    candidates.sort_by(|a, b| a.point.y.total_cmp(&b.point.y)
        .then_with(|| a.point.x.total_cmp(&b.point.x)).then_with(|| a.id.cmp(&b.id)));
}

/// Strict lexicographic priority: matching color -> verified center group -> stage distance.
/// Never substitutes image center for stage-center evidence.
pub fn rank_regions(candidates: Vec<RegionCandidate>, color: Color, tolerance: u8) -> Vec<RegionCandidate> {
    let mut result: Vec<_> = candidates.into_iter().filter(|r| {
        color_matches(r.candidate.color, color, tolerance)
            && !r.candidate.id.is_empty() && r.candidate.target.valid()
            && r.candidate.point.x.is_finite() && r.candidate.point.y.is_finite()
            && r.distance_to_stage.is_finite() && r.distance_to_stage >= 0.0
    }).collect();
    result.sort_by(|a, b| a.center_rank.cmp(&b.center_rank)
        .then_with(|| a.distance_to_stage.total_cmp(&b.distance_to_stage))
        .then_with(|| a.candidate.id.cmp(&b.candidate.id)));
    result
}

/// Native adapters map captured pixel coordinates into the exact bound content rectangle.
/// Rebind after a size change; this transform does not establish an unknown future map layout.
pub fn frame_to_content(point: Point, frame_width: u32, frame_height: u32, content: Rect) -> Result<Point, ValidationError> {
    if frame_width == 0 || frame_height == 0 || !content.valid() || !point.x.is_finite() || !point.y.is_finite()
        || point.x < 0.0 || point.y < 0.0 || point.x >= frame_width as f64 || point.y >= frame_height as f64
    {
        return Err(ValidationError("클릭 위치가 연결한 브라우저 화면 밖입니다".into()));
    }
    Ok(Point {
        x: content.x + point.x * content.width / frame_width as f64,
        y: content.y + point.y * content.height / frame_height as f64,
    })
}
