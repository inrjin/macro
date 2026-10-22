use macro_core::Rect;
mod vision {pub use macro_core::vision::RgbaView;}
mod selection_count_layout {include!("selection_count_layout.rs");}
mod selection_count {include!("selection_count.rs");
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
 let empty=find_label(&mask,width,height,&boxes,"empty");
 let selected:Vec<_>=boxes.iter().filter(|b|b.y>=140).copied().collect();
 let exact=Box2{x:selected.iter().map(|b|b.x).min().unwrap(),y:selected.iter().map(|b|b.y).min().unwrap(),w:0,h:0,pixels:0};
 let exact=Box2{w:selected.iter().map(|b|b.x+b.w).max().unwrap()-exact.x,h:selected.iter().map(|b|b.y+b.h).max().unwrap()-exact.y,..exact};
 let mut scores=templates().iter().enumerate().filter(|(_,t)|t.label=="empty").map(|(i,t)|(i,score(&mask,width,height,exact,t),t.width,t.height)).collect::<Vec<_>>();scores.sort_by(|a,b|b.1.total_cmp(&a.1));
 let actual_components:Vec<_>=selected.iter().map(|b|[b.x,b.y,b.w,b.h,b.pixels]).collect();
 serde_json::json!({"read":format!("{:?}",diagnose_selection_count(frame)),"emptyMatches":empty.iter().map(|b|[b.x,b.y,b.w,b.h]).collect::<Vec<_>>(),"exactInkBox":[exact.x,exact.y,exact.w,exact.h],"bestScores":scores.iter().take(5).collect::<Vec<_>>(),"components":actual_components})
}}
fn main(){for file in std::env::args().skip(1){let bytes=std::fs::read(&file).unwrap();let height=bytes.len()/2560;let frame=vision::RgbaView{width:640,height:height as u32,stride:2560,rgba:&bytes};println!("{}",serde_json::json!({"file":file,"result":selection_count::diagnostic(frame)}));}}
