//! Repeated foreground-silhouette recognition. Color components are seeds,
//! never seat identities. Unsupported/ambiguous geometry produces no click.
use crate::model::*;
use crate::vision::{color_matches, extract_candidates_view_with_stats, rank_seats,
    seat_point_supported, ExtractionStats, RgbaView};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectReason {
    AvailableFill, NoRepeatedShape, BackgroundUncertain, UnsupportedShape,
    OutlineOnlyColor, InternalBackgroundMark, OverlappingHypotheses,
    ColorUsedForFillAndOutline, IncompleteObject,
}

#[derive(Clone, Debug, Serialize)]
pub struct SeatObjectEvidence {
    pub bounds: Rect,
    pub center: Point,
    pub reason: ObjectReason,
    pub target_pixels: usize,
    pub core_pixels: usize,
    pub core_target_pixels: usize,
    pub core_background_pixels: usize,
    pub core_unobserved_pixels: usize,
    pub source_components: usize,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct SeatObjectStats {
    pub background: Option<Color>,
    pub background_samples: usize,
    pub foreground_components: usize,
    pub reference_objects: usize,
    pub template_diameter: Option<f64>,
    pub template_families: usize,
    pub search_budget_exhausted: usize,
    pub resolved_objects: usize,
    pub ambiguous_objects: usize,
    pub merged_color_components: usize,
    pub split_connected_components: usize,
}

#[derive(Clone, Debug)]
pub struct SeatObjectExtraction {
    pub candidates: Vec<Candidate>,
    pub stats: ExtractionStats,
    pub object_stats: SeatObjectStats,
    pub objects: Vec<SeatObjectEvidence>,
}

fn pixel(frame: RgbaView<'_>, x: i64, y: i64) -> Option<Color> {
    if x < 0 || y < 0 || x >= frame.width as i64 || y >= frame.height as i64 { return None; }
    let offset = y as usize * frame.stride + x as usize * 4;
    let p = frame.rgba.get(offset..offset + 4)?;
    (p[3] >= 250).then_some(Color { r: p[0], g: p[1], b: p[2] })
}

fn allowed(point: Point, config: &VisionConfig) -> bool {
    config.roi.is_none_or(|roi| roi.contains(point))
        && !config.exclusions.iter().any(|rect| rect.contains(point))
}

fn contrast(a: Color, b: Color) -> u8 {
    a.r.abs_diff(b.r).max(a.g.abs_diff(b.g)).max(a.b.abs_diff(b.b))
}

fn roi(frame: RgbaView<'_>, config: &VisionConfig) -> (usize, usize, usize, usize) {
    let rect = config.roi.unwrap_or(Rect { x: 0.0, y: 0.0, width: frame.width as f64, height: frame.height as f64 });
    (rect.x.ceil().clamp(0.0, frame.width as f64) as usize,
     rect.y.ceil().clamp(0.0, frame.height as f64) as usize,
     (rect.x + rect.width).ceil().clamp(0.0, frame.width as f64) as usize,
     (rect.y + rect.height).ceil().clamp(0.0, frame.height as f64) as usize)
}

fn background(frame: RgbaView<'_>, area: (usize, usize, usize, usize), config: &VisionConfig) -> Option<(Color, usize)> {
    let (x0, y0, x1, y1) = area;
    let step = (((x1 - x0) * (y1 - y0) / 4096) as f64).sqrt().ceil().max(1.0) as usize;
    let mut bins: BTreeMap<(u8, u8, u8), (usize, u64, u64, u64)> = BTreeMap::new();
    let mut total = 0;
    for y in (y0..y1).step_by(step) { for x in (x0..x1).step_by(step) {
        if !allowed(Point { x: x as f64, y: y as f64 }, config) { continue; }
        if let Some(c) = pixel(frame, x as i64, y as i64) {
            total += 1;
            let bin = bins.entry((c.r / 8, c.g / 8, c.b / 8)).or_default();
            bin.0 += 1; bin.1 += c.r as u64; bin.2 += c.g as u64; bin.3 += c.b as u64;
        }
    }}
    let &(count, r, g, b) = bins.values().max_by_key(|v| v.0)?;
    if total < 16 || count * 100 < total * 35 { return None; }
    Some((Color { r: (r / count as u64) as u8, g: (g / count as u64) as u8, b: (b / count as u64) as u8 }, total))
}

// Recognition and pre-click validation share the same learned-mask contract.
#[path = "seat_object_shapes.rs"]
mod learned;
pub use learned::{extract_seat_objects_view_with_stats, seat_object_candidate_supported};
