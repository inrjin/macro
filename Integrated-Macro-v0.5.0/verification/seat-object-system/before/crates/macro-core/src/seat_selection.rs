//! Image-based identity and adjacency. These describe candidate geometry, not
//! server seat numbers or proof that a delivered click selected a ticket.
use crate::model::{Candidate, Point, Rect, Target};

fn click_point(candidate: &Candidate) -> Option<Point> {
    match candidate.target { Target::Point { point } => Some(point), _ => None }
}

fn bounds(candidate: &Candidate) -> Option<Rect> {
    let geometry = candidate.geometry?;
    let point = click_point(candidate)?;
    let rect = geometry.bounds;
    (geometry.pixels > 0 && rect.valid() && rect.contains(point)
        && (rect.width * rect.height).is_finite()).then_some(rect)
}

/// Compare against the ORIGINAL delivered component. Selection marks can split
/// or shrink its remaining color; replacing this footprint with a fragment
/// would lose the rest of the seat. No fixed-radius neighbor exclusion is used.
pub(crate) fn same_seat(original: &Candidate, candidate: &Candidate) -> bool {
    let (Some(a), Some(b)) = (click_point(original), click_point(candidate)) else { return false; };
    if a == b { return true; }
    let (Some(a_box), Some(b_box)) = (bounds(original), bounds(candidate)) else { return false; };
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

fn neighbors(left: Rect, right: Rect) -> bool {
    let gap = right.x - (left.x + left.width);
    let width = left.width.min(right.width);
    same_row(left, right) && width >= 0.6 * left.width.max(right.width)
        && gap >= 0.0 && gap <= 0.75 * width
}

#[derive(Clone, Copy)]
struct Node { index: Option<usize>, bounds: Rect }

/// Return an index in the already ordered, eligible candidates. Geometry-less
/// imports keep their old order. Tight same-row groups win over isolated seats;
/// after delivery, extending a group containing a retained seat wins first.
pub(crate) fn preferred_candidate(candidates: &[&Candidate], retained: &[&Candidate], needed: usize) -> usize {
    if candidates.len() < 2 || needed == 0 { return 0; }
    let mut nodes: Vec<Node> = candidates.iter().enumerate().filter_map(|(index, candidate)|
        bounds(candidate).map(|bounds| Node { index: Some(index), bounds })).collect();
    nodes.extend(retained.iter().filter_map(|candidate| bounds(candidate).map(|bounds| Node { index: None, bounds })));
    nodes.sort_by(|a, b| (a.bounds.y + a.bounds.height / 2.0).total_cmp(&(b.bounds.y + b.bounds.height / 2.0))
        .then_with(|| a.bounds.x.total_cmp(&b.bounds.x)));
    let mut rows: Vec<Vec<Node>> = vec![];
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
            if end < row.len() && neighbors(row[end - 1].bounds, row[end].bounds) { continue; }
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
