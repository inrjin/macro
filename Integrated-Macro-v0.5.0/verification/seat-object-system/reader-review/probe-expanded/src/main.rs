use macro_core::Rect;
mod vision { pub use macro_core::vision::RgbaView; }
use vision::RgbaView;
use std::{fs, path::PathBuf};

mod reader {
    include!("selection_count_snapshot.rs");
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
        let mut mask=vec![false;width*height];
        for y in 0..height {for x in 0..width {let p=&frame.rgba[y*frame.stride+x*4..][..4];mask[y*width+x]=p[3]>=250 && (u32::from(p[0])*3+u32::from(p[1])*6+u32::from(p[2]))/10<180;}}
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
        serde_json::json!({"result":format!("{:?}",read_selection_count(frame)),"headings":headings.iter().map(|b|serde_json::json!({"box":encoded(*b),"surround":light_surround(frame,*b)})).collect::<Vec<_>>(),"empty":empty.iter().map(|b|serde_json::json!({"box":encoded(*b),"surround":light_surround(frame,*b)})).collect::<Vec<_>>(),"digits":digit_details,"bestHeadingProposals":best_label(&mask,width,height,&boxes,"heading"),"bestEmptyProposals":best_label(&mask,width,height,&boxes,"empty")})
    }
    pub fn oracle_regions(frame:RgbaView<'_>,texts:&[serde_json::Value])->Vec<serde_json::Value>{
        let width=frame.width as usize; let height=frame.height as usize;
        let mut mask=vec![false;width*height];
        for y in 0..height {for x in 0..width {let p=&frame.rgba[y*frame.stride+x*4..][..4];mask[y*width+x]=p[3]>=250 && (u32::from(p[0])*3+u32::from(p[1])*6+u32::from(p[2]))/10<180;}}
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
        let result=reader::diagnose(frame);
        println!("{}",serde_json::json!({"name":name,"diagnostic":result,"diagnosticOracleRegions":reader::oracle_regions(frame,item["text"].as_array().unwrap())}));
    }
}
