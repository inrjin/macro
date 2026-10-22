use std::sync::{atomic::{AtomicBool,AtomicUsize,Ordering},Barrier};
static STOP: AtomicBool=AtomicBool::new(false);
static POSTS: AtomicUsize=AtomicUsize::new(0);
static IN_VALIDATION: Barrier=Barrier::new(2);
static RELEASE_VALIDATION: Barrier=Barrier::new(2);
type Result<T>=std::result::Result<T,()>;
#[derive(Clone,Copy)] struct Point;
#[derive(Clone,Copy)] struct Rect;
impl Rect { fn global(&self,p:Point)->Result<Point>{Ok(p)} }
#[derive(Clone)] struct TargetWindow {rect:Rect}
#[derive(Clone,Copy)] enum Key {Escape}
struct NativeAdapter {target:TargetWindow,overlays:Vec<u64>}
impl NativeAdapter {
fn bound(&self)->Result<&TargetWindow>{Ok(&self.target)}
pub fn click(&mut self, point: Point) -> Result<()> {
        let target = self.bound()?.clone();
        let global = target.rect.global(point)?;
        os::validate(&target, Some(global), &self.overlays)?;
        os::click(global)
    }
pub fn key(&mut self, key: Key) -> Result<()> {
        os::validate(self.bound()?, None, &self.overlays)?;
        os::key(key)
    }
}
mod os {
use super::*;
pub fn validate(_: &TargetWindow,_:Option<Point>,_:&[u64])->Result<()> {
    IN_VALIDATION.wait(); RELEASE_VALIDATION.wait(); Ok(())
}
pub fn click(_:Point)->Result<()> {POSTS.fetch_add(1,Ordering::SeqCst);Ok(())}
pub fn key(_:Key)->Result<()> {POSTS.fetch_add(1,Ordering::SeqCst);Ok(())}
}
fn main(){
for click in [true,false] {
STOP.store(false,Ordering::SeqCst); POSTS.store(0,Ordering::SeqCst);
let worker=std::thread::spawn(move||{
let mut adapter=NativeAdapter{target:TargetWindow{rect:Rect},overlays:vec![]};
// Same final runtime precondition. Only the OS validate/post boundaries are mocked.
assert!(!STOP.load(Ordering::SeqCst));
if click {adapter.click(Point).unwrap()}else{adapter.key(Key::Escape).unwrap()}
});
IN_VALIDATION.wait(); STOP.store(true,Ordering::SeqCst); RELEASE_VALIDATION.wait();
worker.join().unwrap();
assert_eq!(POSTS.load(Ordering::SeqCst),1);
println!("input_kind={} stop_before_validation_return=true posts_after_stop={}",if click{"click"}else{"key"},POSTS.load(Ordering::SeqCst));
}
}
