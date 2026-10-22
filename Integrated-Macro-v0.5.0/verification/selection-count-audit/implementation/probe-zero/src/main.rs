use macro_core::Rect;
mod vision {pub use macro_core::vision::RgbaView;}
mod selection_count_layout {include!("selection_count_layout_snapshot.rs");}
mod selection_count {include!("selection_count_snapshot.rs");
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

pub fn diagnostic(frame:RgbaView<'_>)->serde_json::Value {
 let (width,height)=(frame.width as usize,frame.height as usize);
 let mask=production_mask(frame);let boxes=components(&mask,width,height);
 let headings=find_label(&mask,width,height,&boxes,"heading");
 let mut hs=vec![];
 for original in headings {let completed=complete_heading_bounds(&mask,width,height,original).unwrap_or(original);
 let mut matches=templates().iter().enumerate().filter(|(_,t)|t.label=="heading").map(|(i,t)|(i,score(&mask,width,height,original,t),score(&mask,width,height,completed,t))).collect::<Vec<_>>();
 matches.sort_by(|a,b|b.1.total_cmp(&a.1));
 let grid=(completed.y..completed.y+completed.h).map(|y|(completed.x+completed.w-12..completed.x+completed.w+3).map(|x|if mask[y*width+x]{'#'}else{'.'}).collect::<String>()).collect::<Vec<_>>();
 let mut added=vec![]; for y in completed.y..completed.y+completed.h {for x in original.x+original.w..completed.x+completed.w {let p=&frame.rgba[y*frame.stride+x*4..][..3]; let prior=&frame.rgba[y*frame.stride+(x-1)*4..][..3]; if mask[y*width+x] {added.push(serde_json::json!([x,y,p,prior]));}}}
 hs.push(serde_json::json!({"added":added,"original":[original.x,original.y,original.w,original.h],"completed":[completed.x,completed.y,completed.w,completed.h],"scores":matches.iter().take(6).collect::<Vec<_>>(),"grid":grid}));
 }
 serde_json::json!({"read":format!("{:?}",diagnose_selection_count(frame)),"headings":hs})
}}
fn main(){for file in std::env::args().skip(1){let bytes=std::fs::read(&file).unwrap();let height=bytes.len()/2560;let frame=vision::RgbaView{width:640,height:height as u32,stride:2560,rgba:&bytes};println!("{}",serde_json::json!({"file":file,"result":selection_count::diagnostic(frame)}));}}
