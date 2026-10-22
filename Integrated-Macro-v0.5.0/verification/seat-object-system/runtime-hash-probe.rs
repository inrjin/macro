// Extracted production digest function; minimal byte-container fixture, not Worker/capture/OCR.
use std::{hash::Hasher,time::Instant};
struct OwnedFrame { rgba: Vec<u8> }
fn selection_count_frame_digest(frame: &OwnedFrame) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    // Padding is included conservatively: a change can cause one re-read but
    // never suppress revalidation of a changed global reader precondition.
    hasher.write(&frame.rgba);
    hasher.finish()
}

fn main() { let frame=OwnedFrame{rgba:vec![255;3024*1608*4]}; let start=Instant::now(); let n=200; for _ in 0..n { std::hint::black_box(selection_count_frame_digest(std::hint::black_box(&frame))); } println!("{{\"width\":3024,\"height\":1608,\"rounds\":{},\"meanUs\":{},\"profile\":\"rustc -O; production digest body, synthetic RGBA; excludes crop, OCR, capture, native validation and UI\"}}",n,start.elapsed().as_micros() as f64/n as f64); }
