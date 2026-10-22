// Narrow visual ownership check for NOL's empty selection panel. Text labels
// alone do not establish that they belong to the same UI container.
use crate::{Rect, vision::RgbaView};

#[derive(Clone, Copy, Debug)]
struct Bounds { left: usize, top: usize, right: usize, bottom: usize }

impl Bounds {
    fn width(self) -> usize { self.right - self.left }
    fn height(self) -> usize { self.bottom - self.top }
    fn contains(self, other: Self) -> bool {
        self.left <= other.left && self.top <= other.top
            && self.right >= other.right && self.bottom >= other.bottom
    }
    fn padded(self, amount: usize, width: usize, height: usize) -> Self {
        Self { left: self.left.saturating_sub(amount), top: self.top.saturating_sub(amount),
            right: self.right.saturating_add(amount).min(width),
            bottom: self.bottom.saturating_add(amount).min(height) }
    }
    fn has_point(self, x: usize, y: usize) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }
    fn rect(self) -> Rect {
        Rect { x: self.left as f64, y: self.top as f64,
            width: self.width() as f64, height: self.height() as f64 }
    }
}

fn bounds(frame: RgbaView<'_>, rect: Rect) -> Option<Bounds> {
    if !rect.valid() || rect.x < 0.0 || rect.y < 0.0
        || rect.x + rect.width > frame.width as f64 || rect.y + rect.height > frame.height as f64 { return None; }
    Some(Bounds { left: rect.x.floor() as usize, top: rect.y.floor() as usize,
        right: (rect.x + rect.width).ceil() as usize, bottom: (rect.y + rect.height).ceil() as usize })
}

fn pixel(frame: RgbaView<'_>, x: usize, y: usize) -> Option<[u8; 3]> {
    if x >= frame.width as usize || y >= frame.height as usize { return None; }
    let offset = y.checked_mul(frame.stride)?.checked_add(x.checked_mul(4)?)?;
    let value = frame.rgba.get(offset..offset.checked_add(4)?)?;
    (value[3] >= 250).then_some([value[0], value[1], value[2]])
}

fn difference(a: [u8; 3], b: [u8; 3]) -> u8 {
    a.into_iter().zip(b).map(|(x, y)| x.abs_diff(y)).max().unwrap()
}

fn matches(frame: RgbaView<'_>, x: usize, y: usize, color: [u8; 3], tolerance: u8) -> bool {
    pixel(frame, x, y).is_some_and(|value| difference(value, color) <= tolerance)
}

fn median(colors: &[[u8; 3]]) -> [u8; 3] {
    let mut result = [0; 3];
    for channel in 0..3 {
        let mut values: Vec<_> = colors.iter().map(|value| value[channel]).collect();
        values.sort_unstable();
        result[channel] = values[values.len() / 2];
    }
    result
}

/// Require a bounded solid rounded control on a light background. The color
/// itself is unrestricted: a disabled gray button is a legitimate anchor.
fn button_background(frame: RgbaView<'_>, button: Bounds) -> Option<[u8; 3]> {
    if button.left == 0 || button.top == 0 || button.right >= frame.width as usize
        || button.bottom >= frame.height as usize || button.height() < 8 { return None; }
    let cx = (button.left + button.right) / 2;
    let cy = (button.top + button.bottom) / 2;
    let outer_points = [(button.left - 1, cy), (button.right, cy), (cx, button.top - 1), (cx, button.bottom)];
    let outer: Option<Vec<_>> = outer_points.into_iter().map(|(x, y)| pixel(frame, x, y)).collect();
    let outer = outer?;
    let background = median(&outer);
    if background.into_iter().any(|value| value < 235)
        || outer.iter().any(|value| difference(*value, background) > 8) { return None; }

    // The upper/lower interior strips avoid the central text, regardless of
    // whether the text is white, gray or dark.
    let mut fills = vec![];
    let inset = (button.height() / 4).max(2);
    for y in [button.top + inset, button.bottom - inset - 1] {
        for numerator in 1..=7 {
            fills.push(pixel(frame, button.left + button.width() * numerator / 8, y)?);
        }
    }
    let fill = median(&fills);
    if difference(fill, background) < 12 || fills.iter().filter(|value| difference(**value, fill) <= 8).count() < 12 { return None; }
    // Rounded outer corners must expose the same panel background, while the
    // midpoints lie on the filled control. A filled rectangular content block
    // or an unbounded page background is not a button anchor.
    let corner = (button.height() / 20).max(1);
    if ![(button.left + corner, button.top + corner), (button.right - corner - 1, button.top + corner),
        (button.left + corner, button.bottom - corner - 1), (button.right - corner - 1, button.bottom - corner - 1)]
        .into_iter().all(|(x, y)| matches(frame, x, y, background, 12)) { return None; }
    let edge = (button.height() / 10).max(2);
    if ![(cx, button.top + edge), (cx, button.bottom - edge - 1),
        (button.left + edge, cy), (button.right - edge - 1, cy)]
        .into_iter().all(|(x, y)| matches(frame, x, y, fill, 12)) { return None; }
    Some(background)
}

fn background_column(frame: RgbaView<'_>, x: usize, top: usize, bottom: usize, background: [u8; 3]) -> bool {
    // Check every pixel: a narrow horizontal divider must not disappear merely
    // because an image was sampled at an unfortunate phase.
    (top..bottom).all(|y| matches(frame, x, y, background, 8))
}

fn panel_geometry(frame: RgbaView<'_>, heading: Bounds, empty: Bounds, button: Bounds) -> bool {
    let h = heading.height();
    if h == 0 || button.top <= empty.bottom || empty.top <= heading.bottom
        || button.width() < heading.width().saturating_mul(2)
        || button.width() < empty.width() || button.width() < button.height().saturating_mul(3)
        || button.width() > button.height().saturating_mul(30)
        || button.height() < h || button.height() > h.saturating_mul(5) { return false; }
    let Some(background) = button_background(frame, button) else { return false; };
    let padding = (h / 6).max(2);
    let top = heading.top.saturating_sub(padding);
    let bottom = button.bottom;
    // Search outward from the control, so the scope follows the actual panel
    // after translation/scale changes. At least one real side boundary must be
    // present; an entirely white viewport with unrelated labels is unsupported.
    let mut left = button.left;
    while left > 0 && background_column(frame, left - 1, top, bottom, background) { left -= 1; }
    let mut right = button.right;
    while right < frame.width as usize && background_column(frame, right, top, bottom, background) { right += 1; }
    if left == 0 && right == frame.width as usize { return false; }
    if left == button.left || right == button.right { return false; }
    let panel = Bounds { left, top, right, bottom };
    if !panel.contains(heading.padded(padding, frame.width as usize, frame.height as usize))
        || !panel.contains(empty.padded(padding, frame.width as usize, frame.height as usize)) { return false; }
    // A local panel edge must actually differ over most of this vertical span,
    // not merely consist of one glyph or a short divider from another control.
    for outside in [left.checked_sub(1), (right < frame.width as usize).then_some(right)].into_iter().flatten() {
        let differs = (top..bottom).filter(|&y| pixel(frame, outside, y)
            .is_some_and(|value| difference(value, background) > 8)).count();
        if differs * 100 < (bottom - top) * 90 { return false; }
    }
    // Require clear content except for the two recognized labels. This rejects
    // a wrapped numeral, seat rows, another card, or an intervening separator.
    // It also rejects otherwise legitimate unsupported explanatory layouts.
    let allowed_heading = heading.padded(padding, frame.width as usize, frame.height as usize);
    let allowed_empty = empty.padded(padding, frame.width as usize, frame.height as usize);
    let content_left = left.saturating_add(padding);
    let content_right = right.saturating_sub(padding);
    let content_bottom = button.top.saturating_sub(padding);
    if content_left >= content_right || top >= content_bottom { return false; }
    for y in top..content_bottom { for x in content_left..content_right {
        if allowed_heading.has_point(x, y) || allowed_empty.has_point(x, y) { continue; }
        if !matches(frame, x, y, background, 8) { return false; }
    }}
    true
}

/// An explicit empty label authorizes zero only in a visibly separate, clear
/// selection panel with its own recognized completion control. Bare labels,
/// shared full-width footers and unresolvable container boundaries are unknown.
pub(crate) fn empty_panel_supported(frame: RgbaView<'_>, heading: Rect, empty: Rect, ink_boxes: &[Rect]) -> bool {
    let width = frame.width as usize;
    let height = frame.height as usize;
    if width == 0 || height == 0 || width.checked_mul(4).is_none_or(|row| row > frame.stride)
        || frame.stride.checked_mul(height) != Some(frame.rgba.len()) { return false; }
    let (Some(heading), Some(empty)) = (bounds(frame, heading), bounds(frame, empty)) else { return false; };
    let mut accepted = 0;
    for button in ink_boxes.iter().filter_map(|rect| bounds(frame, *rect)) {
        if panel_geometry(frame, heading, empty, button)
            && crate::selection_count::completion_label_supported(frame, button.rect()) {
            accepted += 1;
            if accepted > 1 { return false; }
        }
    }
    accepted == 1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paint(bytes: &mut [u8], width: usize, rect: Bounds, color: u8) {
        for y in rect.top..rect.bottom { for x in rect.left..rect.right {
            bytes[(y * width + x) * 4..(y * width + x) * 4 + 4].copy_from_slice(&[color, color, color, 255]);
        }}
    }
    fn fixture() -> (Vec<u8>, Bounds, Bounds, Bounds) {
        let mut bytes = [232, 232, 232, 255].repeat(640 * 320);
        paint(&mut bytes, 640, Bounds { left: 260, top: 10, right: 620, bottom: 300 }, 255);
        let heading = Bounds { left: 280, top: 25, right: 380, bottom: 45 };
        let empty = Bounds { left: 310, top: 130, right: 560, bottom: 150 };
        let button = Bounds { left: 280, top: 235, right: 600, bottom: 277 };
        paint(&mut bytes, 640, heading, 30);
        paint(&mut bytes, 640, empty, 125);
        let radius = 12_i64;
        for y in button.top..button.bottom { for x in button.left..button.right {
            let cx = (x as i64).clamp(button.left as i64 + radius, button.right as i64 - radius - 1);
            let cy = (y as i64).clamp(button.top as i64 + radius, button.bottom as i64 - radius - 1);
            if (x as i64 - cx).pow(2) + (y as i64 - cy).pow(2) <= radius.pow(2) {
                bytes[(y * 640 + x) * 4..(y * 640 + x) * 4 + 4].copy_from_slice(&[220, 220, 220, 255]);
            }
        }}
        (bytes, heading, empty, button)
    }
    fn view(bytes: &[u8]) -> RgbaView<'_> { RgbaView { width: 640, height: 320, stride: 2560, rgba: bytes } }

    #[test]
    fn geometry_requires_a_panel_and_rejects_extra_wrapped_count_content() {
        let (mut bytes, heading, empty, button) = fixture();
        assert!(panel_geometry(view(&bytes), heading, empty, button));
        paint(&mut bytes, 640, Bounds { left: 280, top: 50, right: 290, bottom: 65 }, 20);
        assert!(!panel_geometry(view(&bytes), heading, empty, button));
    }

    #[test]
    fn whole_white_viewport_and_shared_footer_do_not_establish_panel_ownership() {
        let (bytes, heading, empty, button) = fixture();
        let mut white = bytes.clone();
        for p in white.chunks_exact_mut(4) { if p[0] == 232 { p[..3].fill(255); } }
        assert!(!panel_geometry(view(&white), heading, empty, button));
        let other_heading = Bounds { left: 20, top: 25, right: 120, bottom: 45 };
        assert!(!panel_geometry(view(&bytes), other_heading, empty, button));
    }

    #[test]
    fn missing_pixels_and_interior_separator_are_not_an_empty_panel() {
        let (mut bytes, heading, empty, button) = fixture();
        bytes[(90 * 640 + 450) * 4 + 3] = 0;
        assert!(!panel_geometry(view(&bytes), heading, empty, button));
        let (mut bytes, heading, empty, button) = fixture();
        paint(&mut bytes, 640, Bounds { left: 265, top: 90, right: 615, bottom: 91 }, 224);
        assert!(!panel_geometry(view(&bytes), heading, empty, button));
    }
}
