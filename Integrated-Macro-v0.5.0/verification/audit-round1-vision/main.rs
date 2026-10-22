use macro_core::{Color, VisionConfig};
use macro_core::vision::{extract_candidates_view_with_stats,RgbaView};
use std::time::Instant;
use std::alloc::{GlobalAlloc,Layout,System};
use std::sync::atomic::{AtomicUsize,Ordering};
static USED: AtomicUsize=AtomicUsize::new(0);
static PEAK: AtomicUsize=AtomicUsize::new(0);
struct Meter;
unsafe impl GlobalAlloc for Meter {
    unsafe fn alloc(&self, layout:Layout)->*mut u8 {
        let p=System.alloc(layout);
        if !p.is_null(){let used=USED.fetch_add(layout.size(),Ordering::Relaxed)+layout.size();PEAK.fetch_max(used,Ordering::Relaxed);}
        p
    }
    unsafe fn dealloc(&self,p:*mut u8,layout:Layout){USED.fetch_sub(layout.size(),Ordering::Relaxed);System.dealloc(p,layout);}
    unsafe fn realloc(&self,p:*mut u8,layout:Layout,size:usize)->*mut u8{
        let next=System.realloc(p,layout,size);
        if !next.is_null(){let used=USED.fetch_sub(layout.size(),Ordering::Relaxed)-layout.size()+size;USED.fetch_add(size,Ordering::Relaxed);PEAK.fetch_max(used,Ordering::Relaxed);}
        next
    }
}
#[global_allocator] static ALLOCATOR:Meter=Meter;
fn main() {
    let color=Color{r:130,g:70,b:210};
    let (width,height)=(3024,1780);
    let rgba=[color.r,color.g,color.b,255].repeat(width*height);
    let config=VisionConfig::default();
    let mut times=Vec::new();
    let base=USED.load(Ordering::Relaxed);
    PEAK.store(base,Ordering::Relaxed);
    for _ in 0..5 {
        let start=Instant::now();
        let result=extract_candidates_view_with_stats(RgbaView{width:width as u32,height:height as u32,stride:width*4,rgba:&rgba},color,12,&config,0).unwrap();
        times.push(start.elapsed().as_micros());
        assert_eq!(result.stats.rejected_too_large,1);
        assert!(result.candidates.is_empty());
    }
    println!("full-match rejected component: {width}x{height}, microseconds={times:?}, extra_peak_bytes={}",PEAK.load(Ordering::Relaxed)-base);
}
