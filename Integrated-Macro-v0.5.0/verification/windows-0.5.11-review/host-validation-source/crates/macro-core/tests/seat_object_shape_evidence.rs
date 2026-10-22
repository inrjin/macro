//! Process-local proof lifetime/validation; not a browser acceptance oracle.
use macro_core::{seat_objects::{extract_seat_objects_view_with_stats,seat_object_candidate_supported}, vision::RgbaView,*};
use std::sync::Arc;

#[test]
fn shape_proof_requires_finite_integer_grid_and_checked_dimensions() {
    assert!(SeatShape::from_occupancy(0,2,&[]).is_none());
    assert!(SeatShape::from_occupancy(257,1,&vec![true;257]).is_none());
    assert!(SeatShape::from_occupancy(2,2,&[false;4]).is_none());
    assert!(SeatShape::from_occupancy(2,2,&[true;3]).is_none());
    let shape=SeatShape::from_occupancy(3,3,&[true,false,false,true,true,false,true,true,true]).unwrap();
    let b=Rect{x:10.0,y:20.0,width:3.0,height:3.0};
    assert!(shape.valid_at(b));assert_eq!(shape.pixel_count(),6);
    assert!(shape.contains_at(b,Point{x:10.0,y:20.0}));
    assert!(!shape.contains_at(b,Point{x:12.0,y:20.0}));
    assert!(!shape.contains_at(b,Point{x:10.5,y:20.0}));
    assert!(!shape.contains_at(b,Point{x:f64::NAN,y:20.0}));
    assert!(!shape.occupied(-1,0));assert!(!shape.occupied(3,0));
    for invalid in [Rect{x:10.5,..b},Rect{x:-1.0,..b},Rect{x:f64::INFINITY,..b},Rect{width:4.0,..b}] {
        assert!(!shape.valid_at(invalid));assert_eq!(shape.overlap_pixels(invalid,&shape,b),0);
    }
    assert_eq!(shape.overlap_pixels(b,&shape,b),6);
    assert_eq!(shape.overlap_pixels(b,&shape,Rect{x:13.0,..b}),0);
}

#[test]
fn cloned_candidates_share_proof_but_json_roundtrip_cannot_authorize_object_clicks() {
    let (w,h)=(160,100);let mut rgba=[255,255,255,255].repeat(w*h);
    let color=Color{r:180,g:220,b:170};
    for (index,(x,y)) in (0..8).map(|i|(8+i*18,12)).chain([(30,60),(70,60)]).enumerate(){
        for yy in y..y+11 {for xx in x..x+11 {
            let c=if index<8{[230,232,235,255]}else{[180,220,170,255]};
            rgba[(yy*w+xx)*4..(yy*w+xx)*4+4].copy_from_slice(&c);
        }}
    }
    let frame=RgbaView{width:w as u32,height:h as u32,stride:w*4,rgba:&rgba};
    let result=extract_seat_objects_view_with_stats(frame,color,3,&VisionConfig::default(),0).unwrap();
    assert_eq!(result.candidates.len(),2);
    let candidate=&result.candidates[0];let clone=candidate.clone();
    let proof=candidate.geometry.as_ref().unwrap().object_shape.as_ref().unwrap();
    assert!(Arc::ptr_eq(proof,clone.geometry.as_ref().unwrap().object_shape.as_ref().unwrap()));
    assert!(Arc::ptr_eq(proof,result.candidates[1].geometry.as_ref().unwrap().object_shape.as_ref().unwrap()),"same learned shape is shared across independent objects");
    assert!(seat_object_candidate_supported(frame,&clone,color,3,&VisionConfig::default()));
    let context=candidate.geometry.as_ref().unwrap().object_context.as_ref().unwrap();
    assert!(Arc::ptr_eq(context,clone.geometry.as_ref().unwrap().object_context.as_ref().unwrap()));
    assert!(Arc::ptr_eq(context,result.candidates[1].geometry.as_ref().unwrap().object_context.as_ref().unwrap()));
    let mut prefixed=clone.clone();prefixed.id=format!("zone7:{}",prefixed.id);
    assert!(seat_object_candidate_supported(frame,&prefixed,color,3,&VisionConfig::default()));
    let mut changed=rgba.clone();changed[(90*w+150)*4]=0;
    let changed_view=RgbaView{rgba:&changed,..frame};
    assert!(seat_object_candidate_supported(changed_view,&prefixed,color,3,&VisionConfig::default()),
        "unrelated image change causes one new interpretation, not unconditional cancellation");
    let narrowed=VisionConfig{roi:Some(Rect{x:25.0,y:55.0,width:25.0,height:25.0}),..VisionConfig::default()};
    assert!(!seat_object_candidate_supported(frame,&clone,color,3,&narrowed),"changed context cannot reuse old references outside the current ROI");
    let serialized=serde_json::to_string(candidate).unwrap();
    assert!(!serialized.contains("object_shape"));assert!(!serialized.contains("object_context"));assert!(!serialized.contains("occupancy"));
    let external:Candidate=serde_json::from_str(&serialized).unwrap();
    assert!(!seat_object_candidate_supported(frame,&external,color,3,&VisionConfig::default()),"a serialized bbox cannot substitute for an in-memory learned silhouette");
}
