//! Image-based identity and adjacency. These describe candidate geometry, not
//! server seat numbers or proof that a delivered click selected a ticket.
use crate::model::{Candidate, Point, Rect, Target};

fn click_point(candidate: &Candidate) -> Option<Point> {
    match candidate.target { Target::Point { point } => Some(point), _ => None }
}

fn bounds(candidate: &Candidate) -> Option<Rect> {
    let geometry = candidate.geometry.as_ref()?;
    let point = click_point(candidate)?;
    let rect = geometry.bounds;
    (geometry.pixels > 0 && rect.valid() && rect.contains(point)
        && geometry.object_shape.as_ref().is_none_or(|shape| shape.valid_at(rect))
        && (rect.width * rect.height).is_finite()).then_some(rect)
}

/// Compare against the ORIGINAL delivered footprint. A learned silhouette does
/// not occupy the empty corners of its bounding box. Legacy color observations
/// without silhouette evidence retain their original component policy.
pub(crate) fn same_seat(original: &Candidate, candidate: &Candidate) -> bool {
    let (Some(a), Some(b)) = (click_point(original), click_point(candidate)) else { return false; };
    if a == b { return true; }
    let (Some(a_box), Some(b_box)) = (bounds(original), bounds(candidate)) else { return false; };
    let a_shape = original.geometry.as_ref().and_then(|geometry| geometry.object_shape.as_ref());
    let b_shape = candidate.geometry.as_ref().and_then(|geometry| geometry.object_shape.as_ref());
    if a_shape.is_some() || b_shape.is_some() {
        if a_shape.is_some_and(|shape| shape.contains_at(a_box, b))
            || b_shape.is_some_and(|shape| shape.contains_at(b_box, a)) { return true; }
        return match (a_shape, b_shape) {
            (Some(a), Some(b)) => {
                let smaller = a.pixel_count().min(b.pixel_count());
                smaller > 0 && a.overlap_pixels(a_box, b, b_box) as f64 >= 0.75 * smaller as f64
            }
            _ => false,
        };
    }
    if a_box.contains(b) || b_box.contains(a) { return true; }
    let width = (a_box.x + a_box.width).min(b_box.x + b_box.width) - a_box.x.max(b_box.x);
    let height = (a_box.y + a_box.height).min(b_box.y + b_box.height) - a_box.y.max(b_box.y);
    width > 0.0 && height > 0.0
        && width * height >= 0.75 * (a_box.width * a_box.height).min(b_box.width * b_box.height)
}

fn same_row(a: Rect, b: Rect) -> bool {
    let height = a.height.min(b.height);
    height >= 0.6 * a.height.max(b.height)
        && ((a.y + a.height / 2.0) - (b.y + b.height / 2.0)).abs() <= 0.35 * height
}

fn neighbors(left: &Node<'_>, right: &Node<'_>) -> bool {
    let a = left.bounds;
    let b = right.bounds;
    let width = a.width.min(b.width);
    if !same_row(a, b) || width < 0.6 * a.width.max(b.width) { return false; }
    let mut gap = b.x - (a.x + a.width);
    if gap < 0.0 {
        // Slanted silhouettes can have overlapping boxes without touching.
        // Only full shape evidence can establish the visible horizontal gap.
        let Some(a_shape) = left.candidate.geometry.as_ref().and_then(|g| g.object_shape.as_ref()) else { return false; };
        let Some(b_shape) = right.candidate.geometry.as_ref().and_then(|g| g.object_shape.as_ref()) else { return false; };
        gap = f64::INFINITY;
        for y in a.y.max(b.y).ceil() as i64..(a.y + a.height).min(b.y + b.height).ceil() as i64 {
            let a_right = (a.x.ceil() as i64..(a.x + a.width).ceil() as i64).rev()
                .find(|&x| a_shape.contains_at(a, Point { x: x as f64, y: y as f64 }));
            let b_left = (b.x.ceil() as i64..(b.x + b.width).ceil() as i64)
                .find(|&x| b_shape.contains_at(b, Point { x: x as f64, y: y as f64 }));
            if let (Some(a_right), Some(b_left)) = (a_right, b_left) {
                let row_gap = (b_left - a_right - 1) as f64;
                if row_gap < 0.0 { return false; }
                gap = gap.min(row_gap);
            }
        }
    }
    gap >= 0.0 && gap <= 0.75 * width
}

#[derive(Clone, Copy)]
struct Node<'a> { index: Option<usize>, bounds: Rect, candidate: &'a Candidate }

/// Return an index in the already ordered, eligible candidates. Geometry-less
/// imports keep their old order. Tight same-row groups win over isolated seats;
/// after delivery, extending a group containing a retained seat wins first.
pub(crate) fn preferred_candidate(candidates: &[&Candidate], retained: &[&Candidate], needed: usize) -> usize {
    if candidates.len() < 2 || needed == 0 { return 0; }
    let mut nodes: Vec<Node<'_>> = candidates.iter().enumerate().filter_map(|(index, candidate)|
        bounds(candidate).map(|bounds| Node { index: Some(index), bounds, candidate })).collect();
    nodes.extend(retained.iter().filter_map(|candidate| bounds(candidate).map(|bounds| Node { index: None, bounds, candidate })));
    nodes.sort_by(|a, b| (a.bounds.y + a.bounds.height / 2.0).total_cmp(&(b.bounds.y + b.bounds.height / 2.0))
        .then_with(|| a.bounds.x.total_cmp(&b.bounds.x)));
    let mut rows: Vec<Vec<Node<'_>>> = vec![];
    for node in nodes {
        // Check both vertical extremes: gradual slopes cannot bridge two rows.
        // An unrelated small component can occur between two row members in
        // center-y order. It must not split a still-compatible earlier row.
        let center_y = node.bounds.y + node.bounds.height / 2.0;
        if let Some(row) = rows.iter_mut().rev().take_while(|row|
            center_y - (row[0].bounds.y + row[0].bounds.height / 2.0) <= 0.35 * node.bounds.height).find(|row|
            same_row(row[0].bounds, node.bounds) && same_row(row.last().unwrap().bounds, node.bounds)) {
            row.push(node);
        } else { rows.push(vec![node]); }
    }
    let mut best = (false, 0_usize, usize::MAX);
    let mut chosen = 0;
    for mut row in rows {
        row.sort_by(|a, b| a.bounds.x.total_cmp(&b.bounds.x));
        let mut begin = 0;
        for end in 1..=row.len() {
            if end < row.len() && neighbors(&row[end - 1], &row[end]) { continue; }
            let group = &row[begin..end];
            let available = group.iter().filter(|node| node.index.is_some()).count();
            if available > 0 {
                let anchored = group.iter().any(|node| node.index.is_none());
                let first = if anchored {
                    // Click next to an existing seat, not the far end of its run.
                    group.iter().enumerate().filter_map(|(position, node)| node.index.filter(|_|
                        position > 0 && group[position - 1].index.is_none()
                            || position + 1 < group.len() && group[position + 1].index.is_none())).min()
                } else { group.iter().filter_map(|node| node.index).min() }.unwrap();
                let score = (anchored, available.min(needed), first);
                if score.0 > best.0 || score.0 == best.0 && (score.1 > best.1 || score.1 == best.1 && score.2 < best.2) {
                    best = score;
                    chosen = first;
                }
            }
            begin = end;
        }
    }
    // A lone geometry candidate must not overtake a prior legacy candidate.
    if !best.0 && best.1 <= 1 { 0 } else { chosen }
}
