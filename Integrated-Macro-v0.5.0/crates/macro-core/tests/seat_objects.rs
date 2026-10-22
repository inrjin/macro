use macro_core::{seat_objects::{extract_seat_objects_view_with_stats, seat_object_candidate_supported,
    ObjectReason}, vision::{RgbaFrame, RgbaView}, *};

const AVAILABLE: Color = Color { r: 194, g: 224, b: 184 };
const OUTLINE: Color = Color { r: 80, g: 166, b: 53 };
const GRAY: Color = Color { r: 238, g: 239, b: 243 };
const WHITE: Color = Color { r: 255, g: 255, b: 255 };

fn view(frame: &RgbaFrame) -> RgbaView<'_> {
    RgbaView { width: frame.width, height: frame.height, stride: frame.width as usize * 4, rgba: &frame.rgba }
}
fn put(frame: &mut RgbaFrame, x: i32, y: i32, color: Color) {
    if x < 0 || y < 0 || x >= frame.width as i32 || y >= frame.height as i32 { return; }
    let offset = (y as usize * frame.width as usize + x as usize) * 4;
    frame.rgba[offset..offset+4].copy_from_slice(&[color.r,color.g,color.b,255]);
}
fn disc(frame: &mut RgbaFrame, x: i32, y: i32, radius: i32, color: Color) {
    for dy in -radius..=radius { for dx in -radius..=radius {
        if dx*dx+dy*dy<=radius*radius { put(frame,x+dx,y+dy,color); }
    }}
}
fn available(frame: &mut RgbaFrame, x: i32, y: i32) {
    disc(frame,x,y,7,OUTLINE); disc(frame,x,y,5,AVAILABLE);
}
fn fixture() -> RgbaFrame {
    let mut frame = RgbaFrame { width: 150, height: 100, rgba: [255,255,255,255].repeat(150*100) };
    for x in [15,35,55,75,95,115] { disc(&mut frame,x,20,7,GRAY); }
    frame
}
fn extract(frame: &RgbaFrame) -> macro_core::seat_objects::SeatObjectExtraction {
    extract_seat_objects_view_with_stats(view(frame),AVAILABLE,12,&VisionConfig::default(),0).unwrap()
}

#[test]
fn repeated_gray_objects_define_size_and_full_outline_survives_internal_color_splits() {
    let mut frame=fixture(); available(&mut frame,30,60);
    let original=extract(&frame);
    assert_eq!(original.candidates.len(),1);
    assert!(original.object_stats.reference_objects>=6);
    let bounds=original.candidates[0].geometry.as_ref().unwrap().bounds;
    assert_eq!(bounds,Rect {x:23.0,y:53.0,width:15.0,height:15.0});
    for y in 55..=65 { put(&mut frame,30,y,Color {r:20,g:20,b:20}); }
    let split=extract(&frame);
    assert_eq!(split.candidates.len(),1,"dark text splits the available color, not the object outline");
    assert_eq!(split.candidates[0].geometry.as_ref().unwrap().bounds,bounds);
    assert_eq!(split.candidates[0].id,original.candidates[0].id);
    assert_eq!(split.object_stats.merged_color_components,1);
    assert!(seat_object_candidate_supported(view(&frame),&split.candidates[0],AVAILABLE,12,&VisionConfig::default()));
}

#[test]
fn one_pixel_background_gap_keeps_close_round_neighbors_separate() {
    let mut frame=fixture(); available(&mut frame,30,60); available(&mut frame,46,60);
    let result=extract(&frame);
    assert_eq!(result.candidates.len(),2);
    assert_eq!(result.candidates[0].geometry.as_ref().unwrap().bounds.x,23.0);
    assert_eq!(result.candidates[1].geometry.as_ref().unwrap().bounds.x,39.0);
    assert!(result.candidates.iter().all(|candidate|seat_object_candidate_supported(view(&frame),candidate,AVAILABLE,12,&VisionConfig::default())));
}

#[test]
fn one_pixel_gaps_in_four_directions_remain_clickable_without_neighbor_absorption() {
    let mut frame=fixture();
    for (x,y) in [(30,60),(14,60),(46,60),(30,44),(30,76)] {available(&mut frame,x,y);}
    let result=extract(&frame);
    assert_eq!(result.candidates.len(),5,"five site objects with a one-pixel background gap");
    assert!(result.candidates.iter().all(|candidate|seat_object_candidate_supported(view(&frame),candidate,AVAILABLE,12,&VisionConfig::default())),"pre-click must not sample a wider ring through the four neighboring objects");
}

#[test]
fn a_narrow_same_color_bridge_does_not_turn_two_round_objects_into_one_click() {
    let mut frame=fixture(); available(&mut frame,30,60); available(&mut frame,46,60);
    for x in 35..=41 { put(&mut frame,x,60,AVAILABLE); }
    let result=extract(&frame);
    assert_eq!(result.candidates.len(),2,"two independently supported circular boundaries remain visible");
    let x:Vec<_>=result.candidates.iter().map(|candidate|candidate.point.x).collect();
    assert!(x.iter().any(|x|(*x-30.0).abs()<=2.0));
    assert!(x.iter().any(|x|(*x-46.0).abs()<=2.0));
    assert_eq!(result.object_stats.split_connected_components,1);
    assert!(result.candidates.iter().all(|candidate|seat_object_candidate_supported(view(&frame),candidate,AVAILABLE,12,&VisionConfig::default())));
}

#[test]
fn split_bridge_proposals_reach_two_real_objects_and_completion_waits_for_external_total() {
    // Independent site truth: clicks hit/toggle two circles. The engine never
    // receives their object IDs. Count events model the external counter oracle,
    // not the text reader, OS delivery, or a real site's selection guarantee.
    let centers=[30,46];let mut selected=[false;2];let mut hits=[0;2];
    let render=|selected:[bool;2]| {
        let mut frame=fixture();
        for (index,x) in centers.into_iter().enumerate() {
            if selected[index] {disc(&mut frame,x,60,7,OUTLINE);}
            else {available(&mut frame,x,60);}
        }
        if !selected[0]&&!selected[1] {for x in 35..=41 {put(&mut frame,x,60,AVAILABLE);}}
        frame
    };
    let mut profile=Profile::default();profile.target_seats=2;profile.color=AVAILABLE;
    profile.features.seat_objects=true;profile.features.nol_selection_count=true;
    profile.features.adjacent_seats=true;profile.features.conflict_recovery=true;
    profile.legacy.complete=Some(Target::Point{point:Point{x:140.0,y:90.0}});
    let mut engine=Engine::new(profile).unwrap();engine.step(Event::Start{mode:RunMode::Seats},0);
    let inputs=|actions:Vec<Action>|actions.into_iter().filter(|a|!matches!(a.kind,ActionKind::Observe{..})).collect::<Vec<_>>();
    let observe_count=|engine:&mut Engine,count,at| {
        let request=engine.selection_count_request().unwrap();
        engine.step(Event::SelectionCountObserved{request_id:request.id,observation_id:at,count},at)
    };
    for step in 0..2 {
        let frame=render(selected);let found=extract(&frame);
        let observed=engine.step(Event::CandidatesObserved{generation:0,candidates:found.candidates.clone()},1+step*10);
        assert!(inputs(observed).is_empty(),"image proposals never bypass the independent count gate");
        let actions=inputs(observe_count(&mut engine,selected.iter().filter(|v|**v).count(),2+step*10));
        assert_eq!(actions.len(),1);
        let ActionKind::Click{target:Target::Point{point},ref purpose}=actions[0].kind else{panic!("expected seat click")};
        assert!(purpose.starts_with("seat:"));
        let candidate=found.candidates.iter().find(|c|c.point==point).unwrap();
        assert!(seat_object_candidate_supported(view(&frame),candidate,AVAILABLE,12,&VisionConfig::default()));
        let hit=centers.iter().position(|x|(point.x-*x as f64).powi(2)+(point.y-60.0).powi(2)<=49.0).expect("site object hit");
        hits[hit]+=1;selected[hit]=!selected[hit];
        let ack=engine.step(Event::ActionAcknowledged{id:actions[0].id,success:true,detail:String::new()},3+step*10);
        assert!(inputs(ack).is_empty(),"delivery does not establish selected cardinality");
        assert!(inputs(observe_count(&mut engine,step as usize,4+step*10)).is_empty(),"a lagging counter cannot release another click or completion");
    }
    assert_eq!(hits,[1,1]);assert_eq!(selected,[true,true]);
    let complete=inputs(observe_count(&mut engine,2,30));
    assert_eq!(complete.len(),1);assert!(matches!(complete[0].kind,ActionKind::CompleteSelection{..}));
}

#[test]
fn selected_dark_fill_and_expanded_halo_cannot_become_new_available_fragments() {
    let mut frame=fixture(); available(&mut frame,30,60);
    let before=extract(&frame); assert_eq!(before.candidates.len(),1);
    disc(&mut frame,30,60,9,Color {r:150,g:201,b:134});
    disc(&mut frame,30,60,7,OUTLINE);
    for (x,y) in [(27,60),(28,61),(29,62),(30,61),(31,60),(32,59)] { put(&mut frame,x,y,WHITE); }
    assert!(extract(&frame).candidates.is_empty());
    assert!(!seat_object_candidate_supported(view(&frame),&before.candidates[0],AVAILABLE,12,&VisionConfig::default()));
}

#[test]
fn registering_outline_color_is_reported_ambiguous_instead_of_choosing_selected_fill() {
    let mut frame=fixture(); available(&mut frame,30,60); disc(&mut frame,70,60,7,OUTLINE);
    let result=extract_seat_objects_view_with_stats(view(&frame),OUTLINE,3,&VisionConfig::default(),0).unwrap();
    assert!(result.candidates.is_empty());
    assert!(result.objects.iter().any(|object|matches!(object.reason,ObjectReason::OutlineOnlyColor)));
    assert!(result.objects.iter().any(|object|matches!(object.reason,ObjectReason::ColorUsedForFillAndOutline)));
}

#[test]
fn unexplained_inner_background_mark_and_unlearned_shapes_are_not_counted_as_available() {
    let mut frame=fixture(); available(&mut frame,30,60);
    for y in 55..=65 { for x in 30..=31 { put(&mut frame,x,y,WHITE); } }
    let result=extract(&frame);
    assert!(result.candidates.is_empty(),"white internal marks can also be selection marks");
    assert!(result.object_stats.ambiguous_objects>0);
    let mut sparse=RgbaFrame {width:80,height:80,rgba:[255,255,255,255].repeat(80*80)};
    available(&mut sparse,40,40);
    let result=extract(&sparse);
    assert!(result.candidates.is_empty());
    assert!(result.objects.iter().all(|object|matches!(object.reason,ObjectReason::NoRepeatedShape)));
}

#[test]
fn pre_click_rechecks_whole_object_when_the_center_color_patch_still_exists() {
    let mut frame=fixture(); available(&mut frame,30,60);
    let candidate=extract(&frame).candidates.remove(0);
    for y in 50..=70 {for x in 20..=40 {put(&mut frame,x,y,WHITE);}}
    let p=candidate.point;
    for y in p.y as i32..p.y as i32+2 {for x in p.x as i32..p.x as i32+2 {put(&mut frame,x,y,AVAILABLE);}}
    assert!(macro_core::vision::seat_point_supported(view(&frame),p,AVAILABLE,12,&VisionConfig::default()));
    assert!(!seat_object_candidate_supported(view(&frame),&candidate,AVAILABLE,12,&VisionConfig::default()));
}

#[test]
fn exclusions_clipping_and_partial_transparency_do_not_create_clickable_objects() {
    let mut frame=fixture(); available(&mut frame,30,60);
    let config=VisionConfig {exclusions:vec![Rect {x:20.0,y:50.0,width:22.0,height:22.0}],..VisionConfig::default()};
    assert!(extract_seat_objects_view_with_stats(view(&frame),AVAILABLE,12,&config,0).unwrap().candidates.is_empty());
    for y in 50..100 {for x in 0..150 {frame.rgba[(y*150+x)*4+3]=0;}}
    assert!(extract_seat_objects_view_with_stats(view(&frame),AVAILABLE,12,&VisionConfig::default(),0).is_err());
}

#[test]
fn a_small_unobserved_core_cannot_be_hidden_by_the_whole_roi_opacity_ratio() {
    let mut frame=fixture();available(&mut frame,30,60);
    let candidate=extract(&frame).candidates.remove(0);
    frame.rgba[(60*150+30)*4+3]=0;
    assert!(extract(&frame).candidates.is_empty(),"even one missing core pixel is an unresolved object, not fewer total pixels");
    assert!(!seat_object_candidate_supported(view(&frame),&candidate,AVAILABLE,12,&VisionConfig::default()));
    let frame=fixture_with_available();
    let config=VisionConfig{exclusions:vec![Rect{x:30.0,y:60.0,width:1.0,height:1.0}],..VisionConfig::default()};
    assert!(extract_seat_objects_view_with_stats(view(&frame),AVAILABLE,12,&config,0).unwrap().candidates.is_empty());
}

fn fixture_with_available()->RgbaFrame {let mut frame=fixture();available(&mut frame,30,60);frame}

#[test]
#[ignore="manual private screenshot check; no original image is kept in the repository"]
fn manual_provided_screenshot_detects_available_full_objects() {
    let rgba=std::fs::read("/private/tmp/seat-object-analysis/screenshot.rgba").unwrap();
    let image=RgbaView {width:3024,height:1608,stride:12096,rgba:&rgba};
    let config=VisionConfig {roi:Some(Rect {x:0.0,y:250.0,width:1370.0,height:1150.0}),..VisionConfig::default()};
    let started=std::time::Instant::now();
    let result=extract_seat_objects_view_with_stats(image,AVAILABLE,12,&config,0).unwrap();
    println!("real_screenshot candidates={} stats={:?} elapsed_us={}",result.candidates.len(),result.object_stats,started.elapsed().as_micros());
    for candidate in &result.candidates {println!("candidate {} point={:?} bounds={:?}",candidate.id,candidate.point,candidate.geometry.as_ref().unwrap().bounds);}
    for object in &result.objects {println!("object_evidence={object:?}");}
    assert_eq!(result.candidates.len(),2,"two fully visible available seats in this tested ROI");
    assert!(result.candidates.iter().any(|candidate|(candidate.point.x-62.0).abs()<5.0&&(candidate.point.y-865.0).abs()<5.0));
    assert!(result.candidates.iter().any(|candidate|(candidate.point.x-242.0).abs()<5.0&&(candidate.point.y-1153.0).abs()<5.0));
    assert!(result.candidates.iter().all(|candidate|seat_object_candidate_supported(image,candidate,AVAILABLE,12,&config)));
    let outline=extract_seat_objects_view_with_stats(image,OUTLINE,3,&config,0).unwrap();
    println!("actual_outline_registered candidates={} objects={:?}",outline.candidates.len(),outline.objects);
    assert!(outline.candidates.is_empty(),"an outline color shared with the selected fill is not a safe available-state signal");
}
