//! A deliberately narrow, position-independent NOL label reader, not general OCR.
//! Missing/ambiguous labels are never evidence of zero selected seats.
use crate::{Rect, vision::RgbaView};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

#[derive(Clone, Debug, PartialEq)]
pub enum CountRead {
    /// `evidence` follows the detected text/panel, not a viewport location. A
    /// caller must invalidate an input decision if these pixels change before
    /// delivery. A readable UI total does not establish server-side ownership.
    Confirmed { count: usize, heading: Rect, evidence: Rect },
    Unreadable,
}

/// A diagnostic describes why this specific image did not authorize a count.
/// It contains no recognized text, image pixels or user information.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all="snake_case")]
pub enum CountIssue {
    InvalidFrame,
    IncompleteCapture,
    HeadingMissing,
    HeadingAmbiguous,
    HeadingContext,
    NumericContext,
    DigitUnrecognized,
    EmptyLayoutUnsupported,
    CountMissing,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CountDiagnosis {
    pub read: CountRead,
    pub issue: Option<CountIssue>,
}

#[derive(Clone, Copy, Debug)]
struct Box2 { x: usize, y: usize, w: usize, h: usize, pixels: usize }
impl Box2 {
    fn rect(self) -> Rect { Rect { x: self.x as f64, y: self.y as f64, width: self.w as f64, height: self.h as f64 } }
}

#[derive(Deserialize)]
struct Glyph { label: String, width: usize, height: usize, mask: String }
struct Template { label: String, width: usize, height: usize, mask: Vec<bool>, ink: usize, anchors: Vec<Box2> }
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
            let ink=mask.iter().filter(|&&value|value).count();
            Template { label: g.label, width: g.width, height: g.height, mask, ink, anchors }
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
    score_at_least(mask,width,height,b,t,0.0)
}

// Reject only when even perfect agreement on all remaining expected ink cannot
// reach the requested score. This avoids fully scoring thousands of unrelated
// seat-map components without weakening recognition thresholds.
fn score_at_least(mask: &[bool], width: usize, height: usize, b: Box2, t: &Template, minimum: f64) -> f64 {
    if b.x+b.w>width || b.y+b.h>height || b.w==0 || b.h==0 { return 0.0; }
    let mut intersection=0; let mut union=0; let mut remaining=t.ink;
    for y in 0..t.height { for x in 0..t.width {
        let px=b.x+((x*2+1)*b.w/(t.width*2)).min(b.w-1);
        let py=b.y+((y*2+1)*b.h/(t.height*2)).min(b.h-1);
        let actual=mask[py*width+px]; let expected=t.mask[y*t.width+x];
        intersection+=usize::from(actual && expected); union+=usize::from(actual || expected);
        remaining-=usize::from(expected);
    }
        // Use the same division as the final score. Multiplying a threshold
        // such as 0.55 by 100 rounds above 55 and would reject an exact match.
        let possible_union=union+remaining;
        let upper=if possible_union==0 {0.0} else {(intersection+remaining) as f64/possible_union as f64};
        if upper<minimum {return 0.0;}
    }
    if union==0 { 0.0 } else { intersection as f64/union as f64 }
}

fn find_label(mask: &[bool], width: usize, height: usize, boxes: &[Box2], label: &str) -> Vec<Box2> {
    let mut matches: Vec<(Box2,f64)> = vec![];
    for t in templates().iter().filter(|t|t.label==label) {
        for anchor in &t.anchors { for b in boxes {
            if !(6..=96).contains(&b.h) || b.w>100 || b.pixels<10 { continue; }
            let sx=b.w as f64/anchor.w as f64; let sy=b.h as f64/anchor.h as f64;
            if (sx/sy-1.0).abs()>0.16 || !(0.3..=2.8).contains(&sy) { continue; }
            let density=b.pixels as f64/(b.w*b.h) as f64
                / (anchor.pixels as f64/(anchor.w*anchor.h) as f64);
            if !(0.60..=1.55).contains(&density) {continue;}
            for scale in [sy, (sx+sy)/2.0] {
                let left=b.x as f64-anchor.x as f64*scale;
                let top=b.y as f64-anchor.y as f64*scale;
                // Fractional rasterization can shift the inferred word origin
                // by two pixels even though the connected anchor itself is
                // within one pixel. Refine translation, not the score threshold.
                for dy in -1..=1 { for dx in -2..=2 {
                    let x=left.round() as isize+dx; let y=top.round() as isize+dy;
                    if x<0 || y<0 {continue;}
                    let region=Box2 {x:x as usize,y:y as usize,w:(t.width as f64*scale).round() as usize,
                        h:(t.height as f64*scale).round() as usize,pixels:0};
                    let preliminary=score_at_least(mask,width,height,region,t,0.55);
                    if preliminary<0.55 {continue;}
                    // A connected anchor can round one pixel differently from
                    // the full word. The full-word score must pass unchanged.
                    for dw in -1..=1 { for dh in -1..=1 {
                        let w=region.w as isize+dw;let h=region.h as isize+dh;
                        if w<=0 || h<=0 {continue;}
                        let refined=Box2 {w:w as usize,h:h as usize,..region};
                        let s=score_at_least(mask,width,height,refined,t,0.77);
                        if s<0.77 {continue;}
                        if let Some(existing)=matches.iter_mut().find(|(old,_)|overlap(*old,refined)) {
                            if s>existing.1 {*existing=(refined,s);}
                        } else {matches.push((refined,s));}
                    }}
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

// The component-size filter proposes a digit; it does not prove that digit is
// the complete count. Signs, punctuation, smaller digits and a second digit at
// the end of the proposal band must not disappear from the interpretation.
fn isolated_count(mask: &[bool], width: usize, height: usize, heading: Box2, digit: Box2) -> bool {
    let slack=heading.h/3+1;
    let left=heading.x+heading.w;
    let right=left+heading.h*3;
    let top=heading.y.saturating_sub(slack);
    let bottom=heading.y+slack+heading.h*13/10;
    if right>=width || bottom>=height {return false;}
    for y in top..bottom {for x in left..right {
        if mask[y*width+x] && !(x>=digit.x && x<digit.x+digit.w && y>=digit.y && y<digit.y+digit.h) {
            return false;
        }
    }}
    true
}

fn standalone_heading(mask: &[bool], width: usize, heading: Box2) -> bool {
    // "미선택 좌석" and "최대 선택 좌석" contain the expected glyphs but
    // describe a different quantity. Require a blank word boundary, without
    // extending the check into another row or assuming a viewport location.
    let left=heading.x.saturating_sub((heading.h/2).max(2));
    (heading.y..heading.y+heading.h).all(|y|
        (left..heading.x).all(|x|!mask[y*width+x]))
}

fn complete_heading_bounds(mask: &[bool], width: usize, height: usize, heading: Box2) -> Option<Box2> {
    // Include actual connected ink at the matched glyph boundary. Bounding-box
    // overlap alone is insufficient: a panel border can enclose the word while
    // none of its pixels belongs to the word. The flood stays within a two-pixel
    // rasterization correction; a connected larger neighbor is unsupported.
    let x0=heading.x.saturating_sub(2);let y0=heading.y.saturating_sub(2);
    let x1=(heading.x+heading.w+2).min(width);let y1=(heading.y+heading.h+2).min(height);
    let local_width=x1-x0;
    let mut seen=vec![false;local_width*(y1-y0)];
    let mut stack=Vec::new();
    let (mut left,mut top,mut right,mut bottom)=(heading.x,heading.y,heading.x+heading.w,heading.y+heading.h);
    for sy in heading.y..heading.y+heading.h {for sx in heading.x..heading.x+heading.w {
        if !mask[sy*width+sx] || seen[(sy-y0)*local_width+sx-x0] {continue;}
        stack.push((sx,sy));seen[(sy-y0)*local_width+sx-x0]=true;
        while let Some((x,y))=stack.pop() {
            left=left.min(x);top=top.min(y);right=right.max(x+1);bottom=bottom.max(y+1);
            for (nx,ny) in [x.checked_sub(1).map(|nx|(nx,y)),(x+1<width).then_some((x+1,y)),
                y.checked_sub(1).map(|ny|(x,ny)),(y+1<height).then_some((x,y+1))].into_iter().flatten() {
                if !mask[ny*width+nx] {continue;}
                if nx<x0 || nx>=x1 || ny<y0 || ny>=y1 {return None;}
                let i=(ny-y0)*local_width+nx-x0;
                if !seen[i] {seen[i]=true;stack.push((nx,ny));}
            }
        }
    }}
    Some(Box2{x:left,y:top,w:right-left,h:bottom-top,pixels:0})
}

/// Recognize the completion label inside a separately detected button. The
/// caller establishes the panel geometry; this routine supplies only text
/// evidence, using contrast against that button's measured fill.
pub(crate) fn completion_label_supported(frame: RgbaView<'_>, button: Rect) -> bool {
    if !button.valid() || button.x < 0.0 || button.y < 0.0 { return false; }
    let (x0,y0)=(button.x.floor() as usize,button.y.floor() as usize);
    let (width,height)=(button.width.ceil() as usize,button.height.ceil() as usize);
    if width<20 || height<10 || x0+width>frame.width as usize || y0+height>frame.height as usize {return false;}
    let mut values=Vec::with_capacity(width*height);
    for y in y0..y0+height {for x in x0..x0+width {
        let p=&frame.rgba[y*frame.stride+x*4..][..4];
        if p[3]<250 {return false;}
        values.push(((u32::from(p[0])*3+u32::from(p[1])*6+u32::from(p[2]))/10) as u8);
    }}
    let mut histogram=[0usize;256];
    for &v in &values {histogram[v as usize]+=1;}
    let background=histogram.iter().enumerate().max_by_key(|(_,n)|*n).map(|(v,_)|v as i32).unwrap();
    let darkest=*values.iter().min().unwrap() as i32;
    let brightest=*values.iter().max().unwrap() as i32;
    for light in [true,false] {
        let span=if light {brightest-background} else {background-darkest};
        if span<16 {continue;}
        let threshold=if light {background+span*75/255} else {background-span*75/255};
        let mask:Vec<bool>=values.iter().map(|&v|if light {v as i32>threshold} else {(v as i32)<threshold}).collect();
        let boxes=components(&mask,width,height);
        let labels=find_label(&mask,width,height,&boxes,"complete");
        if labels.len()==1 && labels[0].x>1 && labels[0].y>1
            && labels[0].x+labels[0].w<width-1 && labels[0].y+labels[0].h<height-1 {return true;}
    }
    false
}

/// Search labels across the entire captured content. No viewport quadrant or
/// fixed position is part of the contract. Unsupported fonts/layouts return
/// Unreadable; a digit alone, timer, absent label, or dimmed backdrop is invalid.
pub fn read_selection_count(frame: RgbaView<'_>) -> CountRead {
    diagnose_selection_count(frame).read
}

/// The legacy result and diagnostic share one analysis. No extra OCR pass,
/// image copy or pixel/text logging is performed to provide the reason.
pub fn diagnose_selection_count(frame: RgbaView<'_>) -> CountDiagnosis {
    match read_with_issue(frame) {
        Ok(read)=>CountDiagnosis{read,issue:None},
        Err(issue)=>CountDiagnosis{read:CountRead::Unreadable,issue:Some(issue)},
    }
}

fn read_with_issue(frame: RgbaView<'_>) -> Result<CountRead,CountIssue> {
    let width=frame.width as usize; let height=frame.height as usize;
    if width==0 || height==0 || width.checked_mul(4).is_none_or(|n|n>frame.stride)
        || frame.stride.checked_mul(height)!=Some(frame.rgba.len()) {return Err(CountIssue::InvalidFrame);}
    let Some(len)=width.checked_mul(height) else {return Err(CountIssue::InvalidFrame);};
    let mut luminance=vec![255u8;len];
    for y in 0..height {for x in 0..width {
        let p=&frame.rgba[y*frame.stride+x*4..][..4];
        // Unobserved pixels cannot establish global absence of a contradictory
        // label or second heading. Do not substitute white for missing data.
        if p[3]<250 {return Err(CountIssue::IncompleteCapture);}
        luminance[y*width+x]=((u32::from(p[0])*3+u32::from(p[1])*6+u32::from(p[2]))/10) as u8;
    }}
    // Normalize antialias coverage to the nearby ink color. A blue numeral or
    // gray empty label has the same glyph as black text, but a fixed absolute
    // threshold otherwise erodes its strokes and breaks connected anchors.
    let mut horizontal=vec![255u8;len];
    for y in 0..height { for x in 0..width {
        horizontal[y*width+x]=*luminance[y*width+x.saturating_sub(2)..=y*width+(x+2).min(width-1)].iter().min().unwrap();
    }}
    let mut mask=vec![false;len];
    for y in 0..height {for x in 0..width {
        let ink=(y.saturating_sub(2)..=(y+2).min(height-1)).map(|row|horizontal[row*width+x]).min().unwrap();
        let threshold=180u16+75u16*ink as u16/255;
        mask[y*width+x]=(luminance[y*width+x] as u16)<threshold;
    }}
    let boxes=components(&mask,width,height);
    let headings=find_label(&mask,width,height,&boxes,"heading");
    if headings.is_empty() {return Err(CountIssue::HeadingMissing);}
    if headings.len()!=1 {return Err(CountIssue::HeadingAmbiguous);}
    if !light_surround(frame,headings[0]) {return Err(CountIssue::HeadingContext);}
    let Some(heading)=complete_heading_bounds(&mask,width,height,headings[0]) else {return Err(CountIssue::HeadingContext);};
    if !standalone_heading(&mask,width,heading) {return Err(CountIssue::HeadingContext);}
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
        if digits.len()!=1 || !light_surround(frame,digit) || has_empty
            || !isolated_count(&mask,width,height,heading,digit) {return Err(CountIssue::NumericContext);}
        let mut scores=[0.0_f64;10];
        for t in templates() {
            if let Ok(n)=t.label.parse::<usize>() {
                let ratio=digit.w as f64/digit.h as f64/(t.width as f64/t.height as f64);
                if (ratio-1.0).abs()<0.25 { scores[n]=scores[n].max(score(&mask,width,height,digit,t)); }
            }
        }
        let mut order:Vec<usize>=(0..10).collect(); order.sort_by(|a,b|scores[*b].total_cmp(&scores[*a]));
        if scores[order[0]]>=0.79 && scores[order[0]]-scores[order[1]]>=0.07 {
            let pad=(heading.h/5).max(2);
            let left=heading.x.saturating_sub(pad);
            // Cover every possible companion component in the digit search
            // band, not just the recognized glyph (e.g. a later 1 -> 11).
            let vertical_slack=heading.h/3+1;
            let top=heading.y.saturating_sub(vertical_slack+pad);
            let right=(heading.x+heading.w+heading.h*3+pad).min(width);
            let bottom=(heading.y+vertical_slack+heading.h*13/10+pad).min(height);
            return Ok(CountRead::Confirmed {count:order[0],heading:heading.rect(),
                evidence:Rect{x:left as f64,y:top as f64,width:(right-left) as f64,height:(bottom-top) as f64}});
        }
        return Err(CountIssue::DigitUnrecognized);
    }
    if empty.len()==1 && light_surround(frame,empty[0]) {
        if let Some(evidence)=crate::selection_count_layout::empty_panel_evidence(frame,heading.rect(),empty[0].rect(),
            &boxes.iter().map(|b|b.rect()).collect::<Vec<_>>()) {
            return Ok(CountRead::Confirmed { count:0, heading:heading.rect(), evidence });
        }
        return Err(CountIssue::EmptyLayoutUnsupported);
    }
    if !empty.is_empty() {return Err(CountIssue::NumericContext);}
    Err(CountIssue::CountMissing)
}

#[cfg(test)]
mod score_tests {
    use super::*;

    #[test]
    fn exact_threshold_is_not_lost_to_float_multiplication_rounding() {
        let actual=vec![true;100];
        let rect=Box2{x:0,y:0,w:100,h:1,pixels:100};
        for ink in [55,77,79] {
            let template=Template{label:String::new(),width:100,height:1,
                mask:(0..100).map(|i|i<ink).collect(),ink,anchors:vec![]};
            let threshold=ink as f64/100.0;
            assert_eq!(score(&actual,100,1,rect,&template),threshold);
            assert_eq!(score_at_least(&actual,100,1,rect,&template,threshold),threshold);
        }
    }

    #[test]
    fn early_rejection_preserves_every_score_that_can_meet_the_threshold() {
        // Include perfect, increasingly damaged, and unrelated masks. The
        // exact-score oracle never takes the early-rejection branch.
        for template in templates() {
            let rect=Box2{x:0,y:0,w:template.width,h:template.height,pixels:0};
            for damage in [0,3,5,11,23] {
                let mut actual=template.mask.clone();
                if damage>0 {for (i,value) in actual.iter_mut().enumerate() {
                    if i%damage==0 {*value=!*value;}
                }}
                let exact=score(&actual,template.width,template.height,rect,template);
                for threshold in [0.55,0.77,0.79] {
                    let bounded=score_at_least(&actual,template.width,template.height,rect,template,threshold);
                    if exact>=threshold {assert_eq!(bounded,exact);}
                    else {assert!(bounded<threshold);}
                }
            }
        }
    }
}
