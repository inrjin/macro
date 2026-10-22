use macro_core::Rect;
mod vision {pub use macro_core::vision::RgbaView;}
mod selection_count {include!("selection_count_snapshot.rs");}
mod selection_count_layout {include!("selection_count_layout_snapshot.rs");}
fn main(){
 let base=std::path::PathBuf::from(std::env::args().nth(1).unwrap());
 let m:serde_json::Value=serde_json::from_slice(&std::fs::read(base.join("zero-support-manifest.json")).unwrap()).unwrap();
 for f in m["fixtures"].as_array().unwrap(){
  let p=std::fs::read(base.join(f["file"].as_str().unwrap())).unwrap();
  let v=vision::RgbaView{width:f["width"].as_u64().unwrap() as u32,height:f["height"].as_u64().unwrap() as u32,stride:f["stride"].as_u64().unwrap() as usize,rgba:&p};
  let r=selection_count::read_selection_count(v);
  let n=match r{selection_count::CountRead::Confirmed{count,..}=>Some(count),_=>None};
  println!("{}",serde_json::json!({"case":f["name"],"count":n}));
 }
}
