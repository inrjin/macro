use macro_core::Rect;
mod vision { pub use macro_core::vision::RgbaView; }
use vision::RgbaView;
use std::{fs, path::PathBuf};

mod selection_count {
    include!("selection_count_snapshot.rs");
    pub fn completion_diagnostic(frame:RgbaView<'_>,button:Rect)->serde_json::Value {
        let(x0,y0)=(button.x as usize,button.y as usize);let(width,height)=(button.width as usize,button.height as usize);
        let mut values=vec![];
        for y in y0..y0+height {for x in x0..x0+width {let p=&frame.rgba[y*frame.stride+x*4..][..4];values.push(((u32::from(p[0])*3+u32::from(p[1])*6+u32::from(p[2]))/10) as u8);}}
        let mut histogram=[0usize;256];for &v in &values{histogram[v as usize]+=1;}
        let bg=histogram.iter().enumerate().max_by_key(|(_,n)|*n).unwrap().0 as i32;
        let mut modes=vec![];
        for light in [true,false]{
            let span=if light {*values.iter().max().unwrap() as i32-bg}else{bg-*values.iter().min().unwrap() as i32};
            if span<16{continue;}
            let threshold=if light{bg+span*75/255}else{bg-span*75/255};
            let mask:Vec<bool>=values.iter().map(|&v|if light{v as i32>threshold}else{(v as i32)<threshold}).collect();
            let boxes=components(&mask,width,height);let labels=find_label(&mask,width,height,&boxes,"complete");
            let mut oracle=None;
            let text_boxes:Vec<_>=boxes.iter().filter(|b|b.x>width/4&&b.x+b.w<width*3/4).collect();
            if !text_boxes.is_empty(){
                let x=text_boxes.iter().map(|b|b.x).min().unwrap();let y=text_boxes.iter().map(|b|b.y).min().unwrap();
                let right=text_boxes.iter().map(|b|b.x+b.w).max().unwrap();let bottom=text_boxes.iter().map(|b|b.y+b.h).max().unwrap();
                let b=Box2{x,y,w:right-x,h:bottom-y,pixels:0};
                oracle=Some(serde_json::json!({"textUnion":encoded(b),"scores":templates().iter().enumerate().filter(|(_,t)|t.label=="complete").map(|(index,t)|serde_json::json!({"index":index,"score":score(&mask,width,height,b,t)})).collect::<Vec<_>>()}));
            }
            modes.push(serde_json::json!({"light":light,"background":bg,"threshold":threshold,"labels":labels.iter().map(|b|encoded(*b)).collect::<Vec<_>>(),"best":best_label(&mask,width,height,&boxes,"complete"),"oracle":oracle}));
        }
        serde_json::json!(modes)
    }
fn production_mask(frame:RgbaView<'_>)->Vec<bool>{let width=frame.width as usize;let height=frame.height as usize;let len=width*height;
    let mut luminance=vec![255u8;len];
    for y in 0..height {for x in 0..width {
        let p=&frame.rgba[y*frame.stride+x*4..][..4];
        if p[3]>=250 {luminance[y*width+x]=((u32::from(p[0])*3+u32::from(p[1])*6+u32::from(p[2]))/10) as u8;}
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

mask
}
    fn encoded(b: Box2) -> serde_json::Value { serde_json::json!({"x":b.x,"y":b.y,"w":b.w,"h":b.h,"pixels":b.pixels}) }
    // Diagnostic search retains the exact production proposal geometry and
    // scoring, but records the best candidate even below its 0.77 threshold.
    fn best_label(mask: &[bool], width: usize, height: usize, boxes: &[Box2], label: &str) -> serde_json::Value {
        let mut results=vec![];
        for (index,t) in templates().iter().enumerate().filter(|(_,t)|t.label==label) {
            let mut best=0.0_f64; let mut region_best=None;
            for anchor in &t.anchors { for b in boxes {
                if !(6..=96).contains(&b.h) || b.w>100 || b.pixels<10 {continue;}
                let sx=b.w as f64/anchor.w as f64; let sy=b.h as f64/anchor.h as f64;
                if (sx/sy-1.0).abs()>0.16 || !(0.3..=2.8).contains(&sy) {continue;}
                let density=b.pixels as f64/(b.w*b.h) as f64 / (anchor.pixels as f64/(anchor.w*anchor.h) as f64);
 if !(0.60..=1.55).contains(&density) {continue;}
 for scale in [sy,(sx+sy)/2.0] {
                    let left=b.x as f64-anchor.x as f64*scale;
                    let top=b.y as f64-anchor.y as f64*scale;
                    for dy in -1..=1 {for dx in -1..=1 {
                        let x=left.round() as isize+dx; let y=top.round() as isize+dy;
                        if x<0 || y<0 {continue;}
                        let region=Box2{x:x as usize,y:y as usize,w:(t.width as f64*scale).round() as usize,h:(t.height as f64*scale).round() as usize,pixels:0};
                        let value=score(mask,width,height,region,t);
                        if value>best {best=value;region_best=Some(encoded(region));}
                    }}
                }
            }}
            results.push(serde_json::json!({"templateIndex":index,"score":best,"box":region_best,"templateSize":[t.width,t.height]}));
        }
        serde_json::json!(results)
    }
    pub fn diagnose(frame:RgbaView<'_>) -> serde_json::Value {
        let width=frame.width as usize;let height=frame.height as usize;
        let mask=production_mask(frame);
        let boxes=components(&mask,width,height);
        let headings=find_label(&mask,width,height,&boxes,"heading");
        let empty=find_label(&mask,width,height,&boxes,"empty");
        let mut digit_details=vec![];
        for heading in &headings {
            let mut digits:Vec<Box2>=boxes.iter().copied().filter(|b|b.x>=heading.x+heading.w && b.x<=heading.x+heading.w+heading.h*2 && b.h*10>=heading.h*6 && b.h*10<=heading.h*13 && b.w<=heading.h && b.y.abs_diff(heading.y)<heading.h/3+1).collect();
            digits.sort_by_key(|b|b.x);
            for digit in digits {
                let mut scores=[0.0_f64;10];let mut template_details=vec![];
                for (index,t) in templates().iter().enumerate() {
                    if let Ok(n)=t.label.parse::<usize>() {
                        let ratio=digit.w as f64/digit.h as f64/(t.width as f64/t.height as f64);
                        let eligible=(ratio-1.0).abs()<0.25;
                        let value=score(&mask,width,height,digit,t);
                        if eligible {scores[n]=scores[n].max(value);}
                        template_details.push(serde_json::json!({"index":index,"digit":n,"score":value,"eligible":eligible,"ratio":ratio}));
                    }
                }
                digit_details.push(serde_json::json!({"box":encoded(digit),"surround":light_surround(frame,digit),"scores":scores,"templates":template_details}));
            }
        }
        serde_json::json!({"result":format!("{:?}",read_selection_count(frame)),"headings":headings.iter().map(|b|serde_json::json!({"box":encoded(*b),"surround":light_surround(frame,*b)})).collect::<Vec<_>>(),"empty":empty.iter().map(|b|serde_json::json!({"box":encoded(*b),"surround":light_surround(frame,*b)})).collect::<Vec<_>>(),"digits":digit_details,"layoutChecks":headings.iter().flat_map(|h|empty.iter().flat_map(|e|boxes.iter().filter(|b|b.w>=100&&b.h>=20&&b.y>=h.y).map(|b|crate::selection_count_layout::diagnose(frame,h.rect(),e.rect(),b.rect())))).collect::<Vec<_>>(),"bestHeadingProposals":best_label(&mask,width,height,&boxes,"heading"),"bestEmptyProposals":best_label(&mask,width,height,&boxes,"empty")})
    }
    pub fn oracle_regions(frame:RgbaView<'_>,texts:&[serde_json::Value])->Vec<serde_json::Value>{
        let width=frame.width as usize; let height=frame.height as usize;
        let mask=production_mask(frame);
        texts.iter().filter_map(|text|{
            let label=match text["value"].as_str()? {"선택 좌석"=>"heading","선택된 좌석이 없습니다."=>"empty",_=>return None};
            let x=text["x"].as_f64()?.max(0.0) as usize;let y=text["y"].as_f64()?.max(0.0) as usize;
            let right=((text["x"].as_f64()?+text["advance"].as_f64()?).ceil() as usize).min(width);
            let bottom=((text["y"].as_f64()?+text["ascent"].as_f64()?+text["descent"].as_f64()?).ceil() as usize).min(height);
            let(mut x0,mut y0,mut x1,mut y1)=(width,height,0,0);
            for py in y..bottom {for px in x..right {if mask[py*width+px]{x0=x0.min(px);y0=y0.min(py);x1=x1.max(px);y1=y1.max(py);}}}
            if x0==width{return None;}
            let b=Box2{x:x0,y:y0,w:x1-x0+1,h:y1-y0+1,pixels:0};
            Some(serde_json::json!({"label":label,"oracleInkBounds":encoded(b),"scoresAtOracleBounds":templates().iter().enumerate().filter(|(_,t)|t.label==label).map(|(index,t)|serde_json::json!({"templateIndex":index,"score":score(&mask,width,height,b,t)})).collect::<Vec<_>>()}))
        }).collect()
    }
}
fn main() {
    let directory=PathBuf::from(std::env::args().nth(1).expect("fixture directory"));
    let manifest:serde_json::Value=serde_json::from_slice(&fs::read(directory.join("manifest.json")).unwrap()).unwrap();
    let wanted=std::env::args().skip(2).collect::<Vec<_>>();
    for item in manifest["fixtures"].as_array().unwrap() {
        let name=item["name"].as_str().unwrap();
        if !wanted.is_empty() && !wanted.iter().any(|x|x==name) {continue;}
        let rgba=fs::read(directory.join(item["file"].as_str().unwrap())).unwrap();
        let frame=RgbaView{width:item["width"].as_u64().unwrap() as u32,height:item["height"].as_u64().unwrap() as u32,stride:item["stride"].as_u64().unwrap() as usize,rgba:&rgba};
        let result=selection_count::diagnose(frame);
        println!("{}",serde_json::json!({"name":name,"diagnostic":result,"diagnosticOracleRegions":selection_count::oracle_regions(frame,item["text"].as_array().unwrap())}));
    }
}

mod selection_count_layout {
include!("selection_count_layout_snapshot.rs");
pub fn diagnose(frame:RgbaView<'_>,heading:Rect,empty:Rect,button:Rect)->serde_json::Value{
let h=bounds(frame,heading).unwrap();let e=bounds(frame,empty).unwrap();let b=bounds(frame,button).unwrap();
serde_json::json!({"heading":heading,"empty":empty,"button":button,"buttonBackground":button_background(frame,b),"geometry":panel_geometry(frame,h,e,b),"semanticLabel":crate::selection_count::completion_label_supported(frame,button),"completionDetails":crate::selection_count::completion_diagnostic(frame,button)})
}
}
