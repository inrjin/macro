use macro_core::{selection_count::read_selection_count, vision::RgbaView};
fn main() {
 let a:Vec<_>=std::env::args().collect();let width:u32=a[2].parse().unwrap();let height:u32=a[3].parse().unwrap();
 let rgba=std::fs::read(&a[1]).unwrap();let t=std::time::Instant::now();
 let result=read_selection_count(RgbaView{width,height,stride:width as usize*4,rgba:&rgba});
 println!("{:?}, {}ms",result,t.elapsed().as_millis());
}
