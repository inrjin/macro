// A deliberately narrow, position-independent NOL label reader, not general OCR.
// Missing/ambiguous labels are never evidence of zero selected seats.
use crate::{Rect, vision::RgbaView};
use serde::Deserialize;
use std::sync::OnceLock;

#[derive(Clone, Debug, PartialEq)]
pub enum CountRead {
    Confirmed { count: usize, heading: Rect },
    Unreadable,
}

#[derive(Clone, Copy, Debug)]
struct Box2 { x: usize, y: usize, w: usize, h: usize, pixels: usize }
impl Box2 {
    fn rect(self) -> Rect { Rect { x: self.x as f64, y: self.y as f64, width: self.w as f64, height: self.h as f64 } }
}

#[derive(Deserialize)]
struct Glyph { label: String, width: usize, height: usize, mask: String }
struct Template { label: String, width: usize, height: usize, mask: Vec<bool>, anchors: Vec<Box2> }
fn templates() -> &'static [Template] {
    static TEMPLATES: OnceLock<Vec<Template>> = OnceLock::new();
    TEMPLATES.get_or_init(|| {
        let glyphs: Vec<Glyph> = serde_json::from_str(include_str!("selection_count_assets/templates.json")).expect("bundled glyph assets");
        glyphs.into_iter().map(|g| {
            let mask: Vec<bool> = g.mask.bytes().map(|x| x == b'1').collect();
            let mut anchors = components(&mask, g.width, g.height);
            anchors.retain(|b| b.h * 2 >= g.height && b.pixels >= 12);
            anchors.sort_by_key(|b| std::cmp::Reverse(b.pixels));
            anchors.truncate(2);
            Template { label: g.label, width: g.width, height: g.height, mask, anchors }
        }).collect()
    })
}

fn components(mask: &[bool], width: usize, height: usize) -> Vec<Box2> {
    let mut todo = mask.to_vec();
    let mut result = vec![];
    let mut stack = vec![];
    for start in 0..todo.len() {
        if !todo[start] { continue; }
        todo[start] = false; stack.push(start);
        let (mut x0, mut y0, mut x1, mut y1, mut pixels) = (width, height, 0, 0, 0);
        while let Some(i) = stack.pop() {
            let x = i % width; let y = i / width;
            x0=x0.min(x); y0=y0.min(y); x1=x1.max(x); y1=y1.max(y); pixels+=1;
            for n in [x.checked_sub(1).map(|_| i-1), (x+1<width).then_some(i+1),
                y.checked_sub(1).map(|_| i-width), (y+1<height).then_some(i+width)].into_iter().flatten() {
                if todo[n] { todo[n]=false; stack.push(n); }
            }
        }
        if pixels >= 3 { result.push(Box2 { x:x0, y:y0, w:x1-x0+1, h:y1-y0+1, pixels }); }
    }
    result
}

fn overlap(a: Box2, b: Box2) -> bool {
    let w=(a.x+a.w).min(b.x+b.w).saturating_sub(a.x.max(b.x));
    let h=(a.y+a.h).min(b.y+b.h).saturating_sub(a.y.max(b.y));
    w*h*2 >= (a.w*a.h).min(b.w*b.h)
}

// Scale-normalized binary intersection/union. Both missing ink and extra ink
// penalize the score; a background rectangle cannot match a text template.
fn score(mask: &[bool], width: usize, height: usize, b: Box2, t: &Template) -> f64 {
    if b.x+b.w>width || b.y+b.h>height || b.w==0 || b.h==0 { return 0.0; }
    let mut intersection=0; let mut union=0;
    for y in 0..t.height { for x in 0..t.width {
        let px=b.x+((x*2+1)*b.w/(t.width*2)).min(b.w-1);
        let py=b.y+((y*2+1)*b.h/(t.height*2)).min(b.h-1);
        let actual=mask[py*width+px]; let expected=t.mask[y*t.width+x];
        intersection+=usize::from(actual && expected); union+=usize::from(actual || expected);
    }}
    if union==0 { 0.0 } else { intersection as f64/union as f64 }
}

fn find_label(mask: &[bool], width: usize, height: usize, boxes: &[Box2], label: &str) -> Vec<Box2> {
    let mut matches: Vec<(Box2,f64)> = vec![];
    for t in templates().iter().filter(|t|t.label==label) {
        for anchor in &t.anchors { for b in boxes {
            if !(6..=96).contains(&b.h) || b.w>100 || b.pixels<10 { continue; }
            let sx=b.w as f64/anchor.w as f64; let sy=b.h as f64/anchor.h as f64;
            if (sx/sy-1.0).abs()>0.16 || !(0.3..=2.8).contains(&sy) { continue; }
            for scale in [sy, (sx+sy)/2.0] {
                let left=b.x as f64-anchor.x as f64*scale;
                let top=b.y as f64-anchor.y as f64*scale;
                for dy in -1..=1 { for dx in -1..=1 {
                    let x=left.round() as isize+dx; let y=top.round() as isize+dy;
                    if x<0 || y<0 {continue;}
                    let region=Box2 {x:x as usize,y:y as usize,w:(t.width as f64*scale).round() as usize,
                        h:(t.height as f64*scale).round() as usize,pixels:0};
                    let s=score(mask,width,height,region,t);
                    if s<0.77 {continue;}
                    if let Some(existing)=matches.iter_mut().find(|(old,_)|overlap(*old,region)) {
                        if s>existing.1 {*existing=(region,s);}
                    } else {matches.push((region,s));}
                }}
            }
        }}
    }
    matches.into_iter().map(|(b,_)|b).collect()
}

fn light_surround(frame: RgbaView<'_>, b: Box2) -> bool {
    let pad=(b.h/5).max(2);
    if b.x==0 || b.y==0 || b.x+b.w>=frame.width as usize || b.y+b.h>=frame.height as usize {return false;}
    let mut light=0; let mut total=0;
    for y in [b.y.saturating_sub(pad),(b.y+b.h+pad).min(frame.height as usize-1)] {
        for x in b.x.saturating_sub(pad)..=(b.x+b.w+pad).min(frame.width as usize-1) {
            let p=&frame.rgba[y*frame.stride+x*4..][..4];
            light+=usize::from(p[3]>=250 && p[0]>=220 && p[1]>=220 && p[2]>=220); total+=1;
        }
    }
    light*100>=total*90
}

/// Search labels across the entire captured content. No viewport quadrant or
/// fixed position is part of the contract. Unsupported fonts/layouts return
/// Unreadable; a digit alone, timer, absent label, or dimmed backdrop is invalid.
pub fn read_selection_count(frame: RgbaView<'_>) -> CountRead {
    let width=frame.width as usize; let height=frame.height as usize;
    if width==0 || height==0 || width.checked_mul(4).is_none_or(|n|n>frame.stride)
        || frame.stride.checked_mul(height)!=Some(frame.rgba.len()) {return CountRead::Unreadable;}
    let Some(len)=width.checked_mul(height) else {return CountRead::Unreadable;};
    let mut mask=vec![false;len];
    for y in 0..height {for x in 0..width {
        let p=&frame.rgba[y*frame.stride+x*4..][..4];
        mask[y*width+x]=p[3]>=250 && ((u32::from(p[0])*3+u32::from(p[1])*6+u32::from(p[2]))/10<180);
    }}
    let boxes=components(&mask,width,height);
    let headings=find_label(&mask,width,height,&boxes,"heading");
    if headings.len()!=1 || !light_surround(frame,headings[0]) {return CountRead::Unreadable;}
    let heading=headings[0];
    // Inconsistent render transitions must not authorize another input. The
    // explicit empty-state label is checked even when a digit is readable.
    let empty=find_label(&mask,width,height,&boxes,"empty");
    let has_empty=empty.iter().any(|b| light_surround(frame,*b));
    let mut digits: Vec<Box2>=boxes.iter().copied().filter(|b| {
        b.x>=heading.x+heading.w && b.x<=heading.x+heading.w+heading.h*2
        && b.h*10>=heading.h*6 && b.h*10<=heading.h*13 && b.w<=heading.h
        && b.y.abs_diff(heading.y)<heading.h/3+1
    }).collect();
    digits.sort_by_key(|b|b.x);
    if let Some(digit)=digits.first().copied() {
        // Multi-digit counts and fragmented/unrecognized text must not silently
        // become a single-digit count. Targets are currently limited to 1..4.
        if digits.len()!=1 || !light_surround(frame,digit) || has_empty {return CountRead::Unreadable;}
        let mut scores=[0.0_f64;10];
        for t in templates() {
            if let Ok(n)=t.label.parse::<usize>() {
                let ratio=digit.w as f64/digit.h as f64/(t.width as f64/t.height as f64);
                if (ratio-1.0).abs()<0.25 { scores[n]=scores[n].max(score(&mask,width,height,digit,t)); }
            }
        }
        let mut order:Vec<usize>=(0..10).collect(); order.sort_by(|a,b|scores[*b].total_cmp(&scores[*a]));
        if scores[order[0]]>=0.79 && scores[order[0]]-scores[order[1]]>=0.07 {
            return CountRead::Confirmed {count:order[0],heading:heading.rect()};
        }
        return CountRead::Unreadable;
    }
    if empty.len()==1 && light_surround(frame,empty[0]) {
        CountRead::Confirmed { count:0, heading:heading.rect() }
    } else {CountRead::Unreadable}
}
