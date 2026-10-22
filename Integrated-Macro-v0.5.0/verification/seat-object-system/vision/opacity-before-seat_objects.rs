//! Conservative repeated-round-object recognition. Color components are seeds,
//! never seat identities. Unsupported/ambiguous geometry produces no click.
use crate::model::*;
use crate::vision::{color_matches, extract_candidates_view_with_stats, rank_seats,
    seat_point_supported, ExtractionStats, RgbaView};
use serde::Serialize;
use std::collections::{BTreeMap, VecDeque};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectReason {
    AvailableFill, NoRepeatedShape, BackgroundUncertain, UnsupportedShape,
    OutlineOnlyColor, InternalBackgroundMark, OverlappingHypotheses,
    ColorUsedForFillAndOutline,
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
    pub source_components: usize,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct SeatObjectStats {
    pub background: Option<Color>,
    pub background_samples: usize,
    pub foreground_components: usize,
    pub reference_objects: usize,
    pub template_diameter: Option<f64>,
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

#[derive(Clone)]
struct Component { bounds: Rect, pixels: usize }

#[derive(Clone, Copy)]
struct Circle { center: Point, diameter: f64, score: usize }
impl Circle {
    fn bounds(self) -> Rect {
        Rect { x: self.center.x - (self.diameter - 1.0) / 2.0,
            y: self.center.y - (self.diameter - 1.0) / 2.0,
            width: self.diameter, height: self.diameter }
    }
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

fn components(frame: RgbaView<'_>, area: (usize, usize, usize, usize), bg: Color,
    config: &VisionConfig) -> Result<Vec<Component>, ValidationError> {
    let (x0,y0,x1,y1) = area;
    let width = x1 - x0; let height = y1 - y0;
    let mut mask = vec![false; width * height];
    let mut opaque = 0; let mut sampled = 0;
    for y in 0..height { for x in 0..width {
        if !allowed(Point { x: (x + x0) as f64, y: (y + y0) as f64 }, config) { continue; }
        sampled += 1;
        if let Some(color) = pixel(frame, (x + x0) as i64, (y + y0) as i64) {
            opaque += 1; mask[y * width + x] = contrast(color, bg) >= 6;
        }
    }}
    if sampled > 0 && opaque * 100 < sampled * 98 {
        return Err(ValidationError("[capture_incomplete] 좌석 객체 화면에 불투명하지 않은 픽셀이 너무 많습니다".into()));
    }
    let mut result = vec![]; let mut queue = VecDeque::new();
    for start in 0..mask.len() {
        if !mask[start] { continue; }
        mask[start] = false; queue.push_back(start);
        let (mut left, mut top, mut right, mut bottom, mut pixels) = (width, height, 0, 0, 0);
        while let Some(index) = queue.pop_front() {
            let x = index % width; let y = index / width;
            left = left.min(x); right = right.max(x); top = top.min(y); bottom = bottom.max(y); pixels += 1;
            for next in [x.checked_sub(1).map(|_| index - 1), (x + 1 < width).then_some(index + 1),
                y.checked_sub(1).map(|_| index - width), (y + 1 < height).then_some(index + width)].into_iter().flatten() {
                if mask[next] { mask[next] = false; queue.push_back(next); }
            }
        }
        if pixels >= 4 && pixels <= config.max_component_pixels {
            result.push(Component { bounds: Rect { x: (left + x0) as f64, y: (top + y0) as f64,
                width: (right - left + 1) as f64, height: (bottom - top + 1) as f64 }, pixels });
        }
    }
    Ok(result)
}

fn circumference_score(frame: RgbaView<'_>, circle: Circle, bg: Color, config: &VisionConfig) -> Option<usize> {
    let radius = circle.diameter / 2.0;
    let mut inside = 0; let mut outside = 0;
    for index in 0..32 {
        let angle = std::f64::consts::TAU * index as f64 / 32.0;
        for (ratio, score) in [(0.82, &mut inside), (1.16, &mut outside)] {
            let x = (circle.center.x + angle.cos() * radius * ratio).round() as i64;
            let y = (circle.center.y + angle.sin() * radius * ratio).round() as i64;
            if !allowed(Point { x: x as f64, y: y as f64 }, config) { return None; }
            let color = pixel(frame, x, y)?;
            if if ratio < 1.0 { contrast(color, bg) >= 6 } else { contrast(color, bg) < 6 } { *score += 1; }
        }
    }
    // A few antialiased edge pixels, letters and a narrow bridge may interrupt
    // the perimeter; a broad merge or no surrounding background is unresolved.
    (inside >= 25 && outside >= 27).then_some(inside + outside)
}

fn circle_from_component(component: &Component) -> Option<Circle> {
    let b = component.bounds;
    let diameter = (b.width + b.height) / 2.0;
    let density = component.pixels as f64 / (b.width * b.height);
    (diameter >= 6.0 && b.width.min(b.height) >= 0.85 * b.width.max(b.height)
        && (0.50..=0.92).contains(&density)).then_some(Circle {
            center: Point { x: b.x + (b.width - 1.0) / 2.0, y: b.y + (b.height - 1.0) / 2.0 }, diameter, score: 0 })
}

fn learn_template(frame: RgbaView<'_>, components: &[Component], bg: Color, config: &VisionConfig) -> Option<(f64, usize)> {
    let mut bins: BTreeMap<u32, Vec<f64>> = BTreeMap::new();
    for component in components {
        if let Some(circle) = circle_from_component(component) {
            if circumference_score(frame, circle, bg, config).is_some() {
                bins.entry((circle.diameter / 2.0).round() as u32).or_default().push(circle.diameter);
            }
        }
    }
    let values = bins.values().max_by_key(|values| values.len())?;
    if values.len() < 6 { return None; }
    Some((values.iter().sum::<f64>() / values.len() as f64, values.len()))
}

fn color_counts(frame: RgbaView<'_>, circle: Circle, color: Color, tolerance: u8, bg: Color,
    config: &VisionConfig) -> (usize, usize, usize, usize, Option<Point>) {
    let radius = circle.diameter / 2.0; let bounds = circle.bounds();
    let (mut target, mut core_target, mut core, mut holes) = (0, 0, 0, 0);
    let mut click = None; let mut distance = f64::INFINITY;
    for y in bounds.y.floor() as i64..(bounds.y + bounds.height).ceil() as i64 {
        for x in bounds.x.floor() as i64..(bounds.x + bounds.width).ceil() as i64 {
            let point = Point { x: x as f64, y: y as f64 };
            let squared = (point.x - circle.center.x).powi(2) + (point.y - circle.center.y).powi(2);
            if squared > radius * radius || !allowed(point, config) { continue; }
            let Some(actual) = pixel(frame, x, y) else { continue; };
            let matches = color_matches(actual, color, tolerance);
            if squared <= (radius * 0.65).powi(2) {
                core += 1;
                if matches { core_target += 1; }
                if contrast(actual, bg) < 6 { holes += 1; }
            }
            if matches {
                target += 1;
                if squared < distance && seat_point_supported(frame, point, color, tolerance, config) {
                    click = Some(point); distance = squared;
                }
            }
        }
    }
    (target, core_target, core, holes, click)
}

fn local_background(frame: RgbaView<'_>, circle: Circle, config: &VisionConfig) -> Option<Color> {
    let mut colors = vec![];
    for index in 0..32 {
        let angle = std::f64::consts::TAU * index as f64 / 32.0;
        let point = Point { x: (circle.center.x + angle.cos() * circle.diameter * 0.65).round(),
            y: (circle.center.y + angle.sin() * circle.diameter * 0.65).round() };
        if !allowed(point, config) { return None; }
        colors.push(pixel(frame, point.x as i64, point.y as i64)?);
    }
    let mut channels = [vec![], vec![], vec![]];
    for color in &colors { channels[0].push(color.r); channels[1].push(color.g); channels[2].push(color.b); }
    for channel in &mut channels { channel.sort_unstable(); }
    let median = Color { r: channels[0][16], g: channels[1][16], b: channels[2][16] };
    (colors.iter().filter(|&&color| contrast(color, median) < 6).count() >= 24).then_some(median)
}

/// Local revalidation of the full round-object evidence on the latest frame.
/// This verifies an image proposal, never site acceptance or semantic identity.
pub fn seat_object_candidate_supported(frame: RgbaView<'_>, candidate: &Candidate,
    color: Color, tolerance: u8, config: &VisionConfig) -> bool {
    let Some(geometry) = candidate.geometry.as_ref() else { return false; };
    let b = geometry.bounds;
    if !b.valid() || !b.contains(candidate.point) || !seat_point_supported(frame, candidate.point, color, tolerance, config) { return false; }
    let circle = Circle { center: Point { x: b.x + (b.width - 1.0) / 2.0, y: b.y + (b.height - 1.0) / 2.0 },
        diameter: (b.width + b.height) / 2.0, score: 0 };
    let Some(bg) = local_background(frame, circle, config) else { return false; };
    let (_, target, core, holes, _) = color_counts(frame, circle, color, tolerance, bg, config);
    circumference_score(frame, circle, bg, config).is_some()
        && core > 0 && target * 100 >= core * 45 && holes * 100 <= core * 8
}

/// Shared Rust image analysis. Requires a repeated round-object family and a
/// sufficiently uniform local background; unsupported scenes stay ambiguous.
pub fn extract_seat_objects_view_with_stats(frame: RgbaView<'_>, color: Color, tolerance: u8,
    config: &VisionConfig, generation: u64) -> Result<SeatObjectExtraction, ValidationError> {
    // Reuse strict frame/config validation and color-component diagnostics.
    let seeds = extract_candidates_view_with_stats(frame, color, tolerance, config, generation)?;
    let mut output = SeatObjectExtraction { candidates: vec![], stats: seeds.stats,
        object_stats: SeatObjectStats::default(), objects: vec![] };
    output.stats.accepted_count = 0;
    let area = roi(frame, config);
    if area.0 == area.2 || area.1 == area.3 { return Ok(output); }
    let Some((bg, samples)) = background(frame, area, config) else {
        unresolved_seeds(&mut output, &seeds.candidates, ObjectReason::BackgroundUncertain);
        return Ok(output);
    };
    output.object_stats.background = Some(bg); output.object_stats.background_samples = samples;
    let components = components(frame, area, bg, config)?;
    output.object_stats.foreground_components = components.len();
    let Some((diameter, references)) = learn_template(frame, &components, bg, config) else {
        unresolved_seeds(&mut output, &seeds.candidates, ObjectReason::NoRepeatedShape);
        return Ok(output);
    };
    output.object_stats.reference_objects = references;
    output.object_stats.template_diameter = Some(diameter);
    let mut circles = vec![];
    // Fast path: independent full objects, irrespective of their internal colors.
    for component in &components {
        if let Some(mut circle) = circle_from_component(component) {
            if (circle.diameter - diameter).abs() <= 2.0_f64.max(diameter * 0.12) {
                if let Some(score) = circumference_score(frame, circle, bg, config) { circle.score = score; circles.push(circle); }
            }
        }
    }
    // Only unresolved color seeds need a bounded local center search. A thin
    // bridge may connect circles; lettering may split one circle into seeds.
    for seed in &seeds.candidates {
        if circles.iter().any(|circle| circle.bounds().contains(seed.point)) { continue; }
        let Some(geometry) = &seed.geometry else { continue; };
        let b = geometry.bounds;
        if b.width > diameter * 4.0 || b.height > diameter * 4.0 { continue; }
        let step = (diameter / 16.0).floor().max(1.0) as usize;
        let left = (b.x - diameter / 2.0).floor().max(0.0) as usize;
        let top = (b.y - diameter / 2.0).floor().max(0.0) as usize;
        let right = (b.x + b.width + diameter / 2.0).ceil().min(frame.width as f64) as usize;
        let bottom = (b.y + b.height + diameter / 2.0).ceil().min(frame.height as f64) as usize;
        let mut proposals = vec![];
        for y in (top..bottom).step_by(step) { for x in (left..right).step_by(step) {
            let mut circle = Circle { center: Point { x: x as f64, y: y as f64 }, diameter, score: 0 };
            if let Some(score) = circumference_score(frame, circle, bg, config) {
                let (_, target, core, _, _) = color_counts(frame, circle, color, tolerance, bg, config);
                if core > 0 && target * 100 >= core * 45 { circle.score = score; proposals.push(circle); }
            }
        }}
        proposals.sort_by(|a,b| b.score.cmp(&a.score).then_with(|| a.center.y.total_cmp(&b.center.y)).then_with(|| a.center.x.total_cmp(&b.center.x)));
        for circle in proposals {
            if !circles.iter().any(|other| (other.center.x-circle.center.x).hypot(other.center.y-circle.center.y) < diameter * 0.45) {
                circles.push(circle);
            }
        }
    }
    let mut outline_only = false;
    for (index, circle) in circles.iter().copied().enumerate() {
        let (target, core_target, core, holes, click) = color_counts(frame, circle, color, tolerance, bg, config);
        if target == 0 { continue; }
        let members = seeds.candidates.iter().filter(|seed| circle.bounds().contains(seed.point)).count();
        let overlaps = circles.iter().enumerate().any(|(other_index, other)| index != other_index
            && (other.center.x-circle.center.x).hypot(other.center.y-circle.center.y) < diameter * 0.8);
        let reason = if overlaps { ObjectReason::OverlappingHypotheses }
            else if core == 0 || core_target * 100 < core * 45 { outline_only = true; ObjectReason::OutlineOnlyColor }
            else if holes * 100 > core * 8 { ObjectReason::InternalBackgroundMark }
            else if click.is_none() { ObjectReason::UnsupportedShape }
            else { ObjectReason::AvailableFill };
        if matches!(reason, ObjectReason::AvailableFill) {
            let point = click.unwrap();
            output.candidates.push(Candidate { id: format!("object:{:.0}:{:.0}", circle.center.x, circle.center.y),
                point, target: Target::Point { point }, color, selected: false,
                geometry: Some(CandidateGeometry { bounds: circle.bounds(), pixels: target }) });
            output.object_stats.resolved_objects += 1;
            output.object_stats.merged_color_components += members.saturating_sub(1);
        } else { output.object_stats.ambiguous_objects += 1; }
        output.objects.push(SeatObjectEvidence { bounds: circle.bounds(), center: circle.center,
            reason, target_pixels: target, core_pixels: core, core_target_pixels: core_target,
            core_background_pixels: holes, source_components: members });
    }
    // An outline color can also be a selected object's solid fill. Without an
    // independent state signal, do not choose the opposite visual state.
    if outline_only && !output.candidates.is_empty() {
        output.object_stats.ambiguous_objects += output.candidates.len();
        output.object_stats.resolved_objects = 0; output.candidates.clear();
        for object in &mut output.objects {
            if matches!(object.reason, ObjectReason::AvailableFill) { object.reason = ObjectReason::ColorUsedForFillAndOutline; }
        }
    }
    for seed in &seeds.candidates {
        if !output.objects.iter().any(|object| object.bounds.contains(seed.point)) {
            unresolved_seeds(&mut output, std::slice::from_ref(seed), ObjectReason::UnsupportedShape);
        }
    }
    output.object_stats.split_connected_components = seeds.candidates.iter().filter(|seed| {
        let Some(geometry) = &seed.geometry else { return false; };
        output.candidates.iter().filter(|candidate| geometry.bounds.contains(candidate.point)).count() > 1
    }).count();
    rank_seats(&mut output.candidates);
    output.stats.accepted_count = output.candidates.len();
    Ok(output)
}

fn unresolved_seeds(output: &mut SeatObjectExtraction, seeds: &[Candidate], reason: ObjectReason) {
    for seed in seeds {
        if let Some(geometry) = &seed.geometry {
            output.object_stats.ambiguous_objects += 1;
            output.objects.push(SeatObjectEvidence { bounds: geometry.bounds, center: seed.point,
                reason: reason.clone(), target_pixels: geometry.pixels, core_pixels: 0,
                core_target_pixels: 0, core_background_pixels: 0, source_components: 1 });
        }
    }
}
