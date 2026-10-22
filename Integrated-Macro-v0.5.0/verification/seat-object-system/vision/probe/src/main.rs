use macro_core::*;
use macro_core::seat_objects::extract_seat_objects_view_with_stats;
use macro_core::vision::RgbaView;
use serde_json::json;

fn count(e: &mut Engine, count: usize, observation_id: u64, now: u64) -> Vec<Action> {
    let request=e.selection_count_request().unwrap();
    e.step(Event::SelectionCountObserved {request_id:request.id,observation_id,count},now)
}
fn physical(actions: &[Action]) -> Vec<&Action> {
    actions.iter().filter(|a|!matches!(a.kind,ActionKind::Observe{..})).collect()
}
fn static_observation() {
    let mut p=Profile::default();p.features.nol_selection_count=true;p.target_seats=2;
    p.legacy.normal_click_delay_ms=100;
    let color=p.color;
    let mut e=Engine::new(p).unwrap();e.step(Event::Start{mode:RunMode::Seats},0);
    e.step(Event::CandidatesObserved{generation:0,candidates:[10.,30.].into_iter().enumerate().map(|(n,x)|{
        let point=Point{x,y:20.};Candidate{id:format!("seat{n}"),point,target:Target::Point{point},color,selected:false,geometry:None}
    }).collect()},1);
    let actions=count(&mut e,0,2000,2);let first=physical(&actions)[0].clone();
    e.step(Event::ActionAcknowledged{id:first.id,success:true,detail:String::new()},3);
    assert!(physical(&count(&mut e,1,4000,4)).is_empty());
    let mut blocked=0;
    for now in [104,204,1004,10004] {
        blocked+=usize::from(physical(&count(&mut e,1,4000,now)).is_empty());
    }
    let fresh=count(&mut e,1,4001,10005);
    assert_eq!(physical(&fresh).len(),1);
    println!("{}",json!({"case":"static_native_timestamp_after_early_count","duplicate_native_stamp_polls_blocked":blocked,"new_read_serial_releases_one_seat":physical(&fresh).len(),"scope":"actual Engine; runtime observation-ID mapping modeled; no OS"}));
}

fn same_color_marker() {
    let width=150;let height=100;let mut rgba=[255,255,255,255].repeat(width*height);
    fn disc(rgba:&mut[u8],x:i32,y:i32,r:i32,c:[u8;3]){
        for dy in -r..=r{for dx in -r..=r{if dx*dx+dy*dy<=r*r{
            let i=((y+dy)as usize*150+(x+dx)as usize)*4;
            rgba[i..i+4].copy_from_slice(&[c[0],c[1],c[2],255]);
        }}}
    }
    for x in [15,35,55,75,95,115]{disc(&mut rgba,x,20,7,[238,239,243]);}
    disc(&mut rgba,30,60,7,[80,166,53]);disc(&mut rgba,30,60,5,[194,224,184]);
    let color=Color{r:194,g:224,b:184};let config=VisionConfig::default();
    let read=|rgba:&[u8]|extract_seat_objects_view_with_stats(RgbaView{width:150,height:100,stride:600,rgba},color,12,&config,0).unwrap();
    let before=read(&rgba);
    for (x,y) in [(29,60),(30,61),(31,60)]{
        let i=(y*150+x)*4;rgba[i..i+4].copy_from_slice(&[255;4]);
    }
    let marked=read(&rgba);
    assert_eq!(before.candidates.len(),1);assert_eq!(marked.candidates.len(),1);
    println!("{}",json!({"case":"same_available_fill_small_white_marker","unmarked_proposals":before.candidates.len(),"marked_proposals":marked.candidates.len(),"evidence":marked.objects,"classification":"known semantic ambiguity, not a normal correctness pass; object proposal is not selected-state proof"}));
}
fn main(){static_observation();same_color_marker();}
