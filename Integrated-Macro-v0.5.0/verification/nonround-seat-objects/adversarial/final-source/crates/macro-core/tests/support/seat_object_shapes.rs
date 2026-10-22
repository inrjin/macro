//! Independent scene renderer and site-object oracle. No production template,
//! component, boundary, fitting or recognition helper constructs these pixels.
#![allow(dead_code)]
use macro_core::{vision::{RgbaFrame, RgbaView}, Color, Point};

pub const AVAILABLE: Color = Color { r: 184, g: 219, b: 172 };
pub const OUTLINE: Color = Color { r: 64, g: 138, b: 46 };
pub const GRAY: Color = Color { r: 226, g: 229, b: 234 };
pub const WHITE: Color = Color { r: 255, g: 255, b: 255 };
pub const INK: Color = Color { r: 28, g: 33, b: 39 };

#[derive(Clone, Copy, Debug)]
pub enum Shape {
    Rectangle { width: f64, height: f64 },
    RoundedRectangle { width: f64, height: f64, radius: f64 },
    Ellipse { width: f64, height: f64 },
    Diamond { width: f64, height: f64 },
}

impl Shape {
    pub fn size(self) -> (f64, f64) {
        match self {
            Self::Rectangle { width, height } | Self::RoundedRectangle { width, height, .. }
                | Self::Ellipse { width, height } | Self::Diamond { width, height } => (width, height),
        }
    }
    fn contains(self, x: f64, y: f64, inset: f64) -> bool {
        let (width, height) = self.size();
        let a = width / 2.0 - inset;
        let b = height / 2.0 - inset;
        if a <= 0.0 || b <= 0.0 { return false; }
        match self {
            Self::Rectangle { .. } => x.abs() <= a && y.abs() <= b,
            Self::RoundedRectangle { radius, .. } => {
                let r = (radius - inset).max(0.0).min(a).min(b);
                if x.abs() > a || y.abs() > b { return false; }
                let dx = (x.abs() - (a - r)).max(0.0);
                let dy = (y.abs() - (b - r)).max(0.0);
                dx * dx + dy * dy <= r * r
            }
            Self::Ellipse { .. } => x * x / (a * a) + y * y / (b * b) <= 1.0,
            Self::Diamond { .. } => x.abs() / a + y.abs() / b <= 1.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Object {
    pub center: Point,
    pub shape: Shape,
    pub angle: f64,
    pub pixels: Vec<(usize, usize)>,
    pub available: bool,
}

#[derive(Clone)]
pub struct Scene {
    pub frame: RgbaFrame,
    pub objects: Vec<Object>,
}

impl Scene {
    pub fn new(width: usize, height: usize) -> Self {
        Self { frame: RgbaFrame { width: width as u32, height: height as u32,
            rgba: [255, 255, 255, 255].repeat(width * height) }, objects: vec![] }
    }
    pub fn view(&self) -> RgbaView<'_> {
        RgbaView { width: self.frame.width, height: self.frame.height,
            stride: self.frame.width as usize * 4, rgba: &self.frame.rgba }
    }
    pub fn paint(&mut self, x: usize, y: usize, color: Color) {
        if x >= self.frame.width as usize || y >= self.frame.height as usize { return; }
        let i = (y * self.frame.width as usize + x) * 4;
        self.frame.rgba[i..i + 4].copy_from_slice(&[color.r, color.g, color.b, 255]);
    }
    pub fn alpha(&mut self, x: usize, y: usize, alpha: u8) {
        let i = (y * self.frame.width as usize + x) * 4;
        self.frame.rgba[i + 3] = alpha;
    }
    pub fn add(&mut self, shape: Shape, center: Point, angle: f64, available: bool) -> usize {
        let mut pixels = vec![];
        let (sin, cos) = angle.to_radians().sin_cos();
        let inset = if shape.size().0.min(shape.size().1) < 8.0 { 0.7 } else { 1.4 };
        let (width, height) = shape.size();
        let extent_x = (width * cos.abs() + height * sin.abs()) / 2.0 + 1.0;
        let extent_y = (width * sin.abs() + height * cos.abs()) / 2.0 + 1.0;
        let x0 = (center.x - extent_x).floor().max(0.0) as usize;
        let y0 = (center.y - extent_y).floor().max(0.0) as usize;
        let x1 = (center.x + extent_x).ceil().min(self.frame.width as f64) as usize;
        let y1 = (center.y + extent_y).ceil().min(self.frame.height as f64) as usize;
        for y in y0..y1 { for x in x0..x1 {
            let (dx, dy) = (x as f64 + 0.5 - center.x, y as f64 + 0.5 - center.y);
            let (u, v) = (dx * cos + dy * sin, -dx * sin + dy * cos);
            if !shape.contains(u, v, 0.0) { continue; }
            pixels.push((x, y));
            self.paint(x, y, if !available { GRAY }
                else if shape.contains(u, v, inset) { AVAILABLE } else { OUTLINE });
        }}
        let id = self.objects.len();
        self.objects.push(Object { center, shape, angle, pixels, available });
        id
    }
    pub fn hits(&self, point: Point) -> Vec<usize> {
        let pixel = (point.x.floor() as usize, point.y.floor() as usize);
        self.objects.iter().enumerate().filter_map(|(id, object)|
            object.pixels.contains(&pixel).then_some(id)).collect()
    }
    pub fn available_ids(&self) -> Vec<usize> {
        self.objects.iter().enumerate().filter_map(|(id, object)| object.available.then_some(id)).collect()
    }
    pub fn paint_object(&mut self, id: usize, color: Color) {
        for (x, y) in self.objects[id].pixels.clone() { self.paint(x, y, color); }
    }
    pub fn selected(&mut self, id: usize) { self.objects[id].available = false; self.paint_object(id, OUTLINE); }
    pub fn dark_vertical_mark(&mut self, id: usize) {
        let x = self.objects[id].center.x.floor() as usize;
        for (px, y) in self.objects[id].pixels.clone() {
            if px == x { self.paint(px, y, INK); }
        }
    }
    pub fn dark_cross(&mut self, id: usize) {
        let c = self.objects[id].center;
        for (x, y) in self.objects[id].pixels.clone() {
            if x == c.x.floor() as usize || y == c.y.floor() as usize { self.paint(x, y, INK); }
        }
    }
    pub fn bridge(&mut self, first: usize, second: usize, height: usize, color: Color) {
        let a = self.objects[first].center; let b = self.objects[second].center;
        let y = a.y.floor() as usize;
        for row in y.saturating_sub(height / 2)..=y + height / 2 {
            for x in a.x.floor() as usize..=b.x.floor() as usize { self.paint(x, row, color); }
        }
    }
}

pub fn repeated(shape: Shape, angle: f64) -> Scene {
    let mut scene = Scene::new(320, 180);
    for row in 0..2 { for column in 0..4 {
        scene.add(shape, Point { x: 30.5 + column as f64 * 60.0, y: 25.5 + row as f64 * 36.0 }, angle, false);
    }}
    scene.add(shape, Point { x: 65.5, y: 125.5 }, angle, true);
    scene.add(shape, Point { x: 125.5, y: 125.5 }, angle, true);
    scene
}

pub fn families() -> Vec<(&'static str, Shape, f64)> {
    vec![
        ("circle15", Shape::Ellipse { width: 15.0, height: 15.0 }, 0.0),
        ("square15", Shape::Rectangle { width: 15.0, height: 15.0 }, 0.0),
        ("rectangle19x11", Shape::Rectangle { width: 19.0, height: 11.0 }, 0.0),
        ("rounded19x13", Shape::RoundedRectangle { width: 19.0, height: 13.0, radius: 4.0 }, 0.0),
        ("diamond19", Shape::Diamond { width: 19.0, height: 19.0 }, 0.0),
        ("ellipse21x13", Shape::Ellipse { width: 21.0, height: 13.0 }, 0.0),
        ("rotated_rectangle30", Shape::Rectangle { width: 19.0, height: 11.0 }, 30.0),
        ("rotated_ellipse25", Shape::Ellipse { width: 21.0, height: 13.0 }, 25.0),
        ("small_square7", Shape::Rectangle { width: 7.0, height: 7.0 }, 0.0),
    ]
}

impl Scene {
    pub fn with_background(width: usize, height: usize, background: Color) -> Self {
        let mut scene = Self::new(width, height);
        scene.frame.rgba = [background.r, background.g, background.b, 255].repeat(width * height);
        scene
    }
    /// Supersample an analytic shape without borrowing any extraction template.
    /// Semantic hit truth is still the independently defined pixel-center shape.
    pub fn add_aa(&mut self, shape: Shape, center: Point, angle: f64, available: bool, background: Color) -> usize {
        let (sin, cos) = angle.to_radians().sin_cos();
        let inset = if shape.size().0.min(shape.size().1) < 8.0 { 0.7 } else { 1.4 };
        let mut pixels = vec![];
        let (width, height) = shape.size();
        let extent_x = (width * cos.abs() + height * sin.abs()) / 2.0 + 1.0;
        let extent_y = (width * sin.abs() + height * cos.abs()) / 2.0 + 1.0;
        let x0 = (center.x - extent_x).floor().max(0.0) as usize;
        let y0 = (center.y - extent_y).floor().max(0.0) as usize;
        let x1 = (center.x + extent_x).ceil().min(self.frame.width as f64) as usize;
        let y1 = (center.y + extent_y).ceil().min(self.frame.height as f64) as usize;
        for y in y0..y1 { for x in x0..x1 {
            let convert = |px: f64, py: f64| { let (dx, dy) = (px - center.x, py - center.y);
                (dx * cos + dy * sin, -dx * sin + dy * cos) };
            let (u, v) = convert(x as f64 + 0.5, y as f64 + 0.5);
            if shape.contains(u, v, 0.0) { pixels.push((x, y)); }
            let mut sums = [0_u32; 3]; let mut any = false;
            for sy in 0..4 { for sx in 0..4 {
                let (u, v) = convert(x as f64 + (sx as f64 + 0.5) / 4.0, y as f64 + (sy as f64 + 0.5) / 4.0);
                let color = if !shape.contains(u, v, 0.0) { background }
                    else { any = true; if !available { GRAY } else if shape.contains(u, v, inset) { AVAILABLE } else { OUTLINE } };
                sums[0] += color.r as u32; sums[1] += color.g as u32; sums[2] += color.b as u32;
            }}
            if any { self.paint(x, y, Color { r: ((sums[0] + 8) / 16) as u8,
                g: ((sums[1] + 8) / 16) as u8, b: ((sums[2] + 8) / 16) as u8 }); }
        }}
        let id = self.objects.len(); self.objects.push(Object { center, shape, angle, pixels, available }); id
    }
}
