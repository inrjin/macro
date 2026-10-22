use count_scope_experiment::{selection_count::{diagnose_selection_count,CountRead},vision::RgbaView};
use std::{fs,path::PathBuf};
fn main(){
 let manifest=PathBuf::from(std::env::args().nth(1).expect("manifest path"));
 let value:serde_json::Value=serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
 let entries=value.as_array().or_else(||value["fixtures"].as_array()).unwrap();
 for f in entries {
  let path=manifest.parent().unwrap().join(f["file"].as_str().unwrap());
  let bytes=fs::read(path).unwrap();
  let d=diagnose_selection_count(RgbaView{width:f["width"].as_u64().unwrap() as u32,height:f["height"].as_u64().unwrap() as u32,stride:f["stride"].as_u64().unwrap() as usize,rgba:&bytes});
  let count=match d.read{CountRead::Confirmed{count,..}=>Some(count),_=>None};
  println!("{}",serde_json::json!({"case":f["name"],"count":count,"issue":d.issue,"scope":if cfg!(feature="heading_only"){"heading_only"}else{"current"}}));
 }
}
