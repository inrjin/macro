use macro_core::{selection_count::{read_selection_count,CountRead},vision::RgbaView,Rect};
use std::{fs,path::Path};
fn crop(rgba:&[u8],r:Rect)->Vec<u8>{
    let mut data=vec![];
    for y in r.y as usize..(r.y+r.height) as usize {
        data.extend_from_slice(&rgba[y*2560+r.x as usize*4..y*2560+(r.x+r.width) as usize*4]);
    }
    data
}
fn main(){
    let base=Path::new("verification/seat-object-system/reader-review/fixtures");
    let before=fs::read(base.join("digit_2.rgba")).unwrap();
    let after=fs::read(base.join("contradictory_empty_and_two.rgba")).unwrap();
    let view=|bytes|RgbaView{width:640,height:300,stride:2560,rgba:bytes};
    let original=read_selection_count(view(&before));let changed=read_selection_count(view(&after));
    let CountRead::Confirmed{count:2,evidence,..}=original else{panic!("baseline count2 must read: {original:?}")};
    assert_eq!(changed,CountRead::Unreadable,"the later contradictory render must be rejected");
    let identical=crop(&before,evidence)==crop(&after,evidence);
    println!("{}",serde_json::json!({"baselineCount":2,"laterRead":"Unreadable","evidence":evidence,"evidenceBytesIdentical":identical,"wholeFrameIdentical":before==after,"scope":"actual reader + byte equality; not Worker/native input execution"}));
    assert!(identical,"current crop-only authorization counterexample expected; if corrected, preserve this as before evidence");
}
