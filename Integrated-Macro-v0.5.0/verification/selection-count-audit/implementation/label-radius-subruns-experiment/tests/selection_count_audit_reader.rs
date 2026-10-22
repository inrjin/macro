//! Independent rendered scene audit. Fixtures use system CoreText, never the
//! production template masks. Synthetic recognition, rejection and support
//! rates are separate; these tests do not execute a browser or OS input.
use count_scope_experiment::{selection_count::{read_selection_count, CountRead}, vision::RgbaView};
use serde::Deserialize;
use std::{fs, path::PathBuf, time::Instant};

#[derive(Deserialize)]
struct Manifest { fixtures: Vec<Fixture> }
#[derive(Deserialize)]
struct Fixture { name: String, file: String, width: u32, height: u32, stride: usize,
    expectation: String, count: Option<usize>, group: String }
fn dir() -> PathBuf { PathBuf::from("/Users/inrjin/Documents/機械人間/Integrated-Macro-v0.5.0/crates/macro-core").join("../../verification/selection-count-audit/adversarial") }
fn fixtures() -> Vec<Fixture> {
    let mut all=vec![];
    for name in ["manifest.json","extra-manifest.json","zero-support-manifest.json"] {
        all.extend(serde_json::from_slice::<Manifest>(&fs::read(dir().join(name)).unwrap()).unwrap().fixtures);
    }
    all
}
fn pixels(f: &Fixture) -> Vec<u8> { let p=fs::read(dir().join(&f.file)).unwrap(); assert_eq!(p.len(),f.stride*f.height as usize);p }
fn read(f:&Fixture,p:&[u8])->CountRead {read_selection_count(RgbaView{width:f.width,height:f.height,stride:f.stride,rgba:p})}
fn numeric(v:&CountRead)->Option<usize>{match v {CountRead::Confirmed{count,..}=>Some(*count),_=>None}}
fn check_group(filter:impl Fn(&Fixture)->bool) {
    check_fixture_set(fixtures(),filter);
}
fn check_fixture_set(inputs:Vec<Fixture>,filter:impl Fn(&Fixture)->bool) {
    let mut failures=Vec::new();let(mut yes,mut unknown)=(0,0);
    for f in inputs.iter().filter(|f|filter(f)) {
        let p=pixels(f);let now=Instant::now();let r=read(f,&p);let n=numeric(&r);
        println!("{}",serde_json::json!({"case":f.name,"group":f.group,"expected":f.count,"policy":f.expectation,"observed":n,"elapsedUs":now.elapsed().as_micros()}));
        if n.is_some(){yes+=1}else{unknown+=1}
        let ok=match f.expectation.as_str(){"confirmed"=>n==f.count,"unreadable"=>n.is_none(),"correct_or_unreadable"=>n.is_none()||n==f.count,_=>panic!("bad expectation")};
        if !ok {failures.push(format!("{}: {:?} expected {:?} ({})",f.name,r,f.count,f.expectation));}
    }
    println!("{}",serde_json::json!({"confirmed":yes,"unreadable":unknown,"failures":failures.len()}));
    assert!(failures.is_empty(),"{}",failures.join("\n"));
}
#[test]fn heldout_typography_never_changes_the_number(){check_group(|f| !matches!(f.group.as_str(),"context"|"duplicate"|"zero"|"zero_extra"|"zero_support"));}
#[test]fn unrelated_or_duplicate_labels_do_not_authorize_a_count(){check_group(|f|matches!(f.group.as_str(),"context"|"duplicate"));}
#[test]fn zero_panel_recognition_and_dimmed_rejection_are_separate(){check_group(|f|f.group=="zero");}
#[test]fn normal_empty_panel_scales_and_ink_keep_prior_readable_controls(){check_group(|f|f.group=="zero_support");}
#[test]fn small_or_faint_nonzero_text_is_not_evidence_of_zero(){check_group(|f|f.group=="zero_extra");}

#[test]
fn contradictory_empty_text_blocks_numeric_confirmation_across_raster_scales(){
    let mut manifest:Manifest=serde_json::from_slice(&fs::read(dir().join("radius-negative/manifest.json")).unwrap()).unwrap();
    for fixture in &mut manifest.fixtures {fixture.file=format!("radius-negative/{}",fixture.file);}
    check_fixture_set(manifest.fixtures,|_|true);
}

#[test]
fn nearby_other_ui_does_not_hide_empty_text_or_invent_it(){
    let manifest:Manifest=serde_json::from_slice(&fs::read(dir().join("empty-neighbor/manifest.json")).unwrap()).unwrap();
    check_fixture_set(manifest.fixtures,|_|true);
}

#[test]
fn dense_seat_map_noise_keeps_the_same_readable_count(){
    let source=fixtures().into_iter().find(|f|f.name=="cut_base_3").unwrap();
    let text=pixels(&source);
    let (width,height)=(3024usize,1608usize);let stride=width*4;
    let mut rgba=vec![255;stride*height];
    // Independent map geometry: 1,800 solid gray seats with only three pixels
    // between columns. The reader receives no seat geometry or text location.
    for row in 0..30 {for col in 0..60 {
        let (left,top)=(20+col*18,100+row*18);
        for y in top..top+13 {for x in left..left+15 {rgba[y*stride+x*4..y*stride+x*4+3].fill(150);}}
    }}
    for y in 0..source.height as usize {
        rgba[(y+20)*stride+2300*4..(y+20)*stride+(2300+source.width as usize)*4]
            .copy_from_slice(&text[y*source.stride..y*source.stride+source.width as usize*4]);
    }
    let result=read_selection_count(RgbaView{width:width as u32,height:height as u32,stride,rgba:&rgba});
    assert_eq!(numeric(&result),Some(3),"dense nontext map must not suppress a readable numeric-only panel: {result:?}");
}

#[test]
fn white_occlusion_of_a_digit_may_reject_but_must_not_change_its_value(){
    let mut wrong=vec![];let(mut correct,mut unknown)=(0,0);
    for f in fixtures().iter().filter(|f|f.group=="cut_base") {
        let original=pixels(f);assert_eq!(numeric(&read(f,&original)),f.count);
        // The generator deliberately colors numeral blue and heading dark.
        // Locate its painted bounds from color only, not any reader matcher.
        let(mut l,mut t,mut r,mut b)=(f.width as usize,f.height as usize,0usize,0usize);
        for y in 0..f.height as usize{for x in 0..f.width as usize{let i=y*f.stride+x*4;
            if original[i+2] as u16>original[i] as u16+40&&original[i+2] as u16>original[i+1] as u16+40 {
                l=l.min(x);r=r.max(x);t=t.min(y);b=b.max(y);
            }
        }}
        assert!(l<r&&t<b);
        for side in ["top","bottom","left","right"]{for amount in 1..=7{
            let mut p=original.clone();
            for y in t..=b{for x in l..=r{
                let cover=match side{"top"=>y<t+amount,"bottom"=>y+amount>b,"left"=>x<l+amount,_=>x+amount>r};
                if cover{let i=y*f.stride+x*4;p[i..i+3].fill(255);}
            }}
            let result=read(f,&p);let n=numeric(&result);
            if n.is_none(){unknown+=1}else if n==f.count{correct+=1}else{
                let name=format!("{}_{}_{}",f.name,side,amount);
                println!("{}",serde_json::json!({"case":name,"actualSemanticCount":f.count,"observedCount":n,"digitBounds":[l,t,r-l+1,b-t+1],"mutation":"opaque white strip covers part of numeral; not an alpha/missing-capture signal"}));
                fs::create_dir_all(dir().join("counterexamples")).unwrap();
                fs::write(dir().join("counterexamples").join(format!("{name}.rgba")),&p).unwrap();
                wrong.push(format!("{name} expected {:?}/Unknown got {result:?}",f.count));
            }
        }}
    }
    println!("{}",serde_json::json!({"digitOcclusionCorrect":correct,"digitOcclusionUnknown":unknown,"digitOcclusionWrong":wrong.len()}));
    assert!(wrong.is_empty(),"{}",wrong.join("\n"));
}

#[test]
#[ignore="manual optimized reader-only timing; no capture, app or OS latency"]
fn representative_large_frames_manual_timing(){
    let fixture=fixtures().into_iter().find(|f|f.name=="zero_panel_strong_boundary").unwrap();
    let small=pixels(&fixture);
    let width=3024u32;let height=1608u32;let stride=width as usize*4;
    for kind in ["blank","many_components","count3_with_map","zero_panel_with_map"]{
        let mut p=vec![255;stride*height as usize];
        if kind!="blank"{
            // Independent seat-map model: 1,500 gray filled seats with gaps.
            for row in 0..30 {for col in 0..50 {let x=20+col*39;let y=100+row*40;
                for yy in y..y+13 {for xx in x..x+15{let i=yy*stride+xx*4;p[i..i+3].fill(150);}}
            }}
        }
        if kind=="count3_with_map"{
            let f=fixtures().into_iter().find(|f|f.name=="cut_base_3").unwrap();let q=pixels(&f);
            for y in 0..f.height as usize {p[(y+20)*stride+2300*4..(y+20)*stride+(2300+f.width as usize)*4].copy_from_slice(&q[y*f.stride..y*f.stride+f.width as usize*4]);}
        }
        if kind=="zero_panel_with_map"{
            for y in 0..fixture.height as usize {p[(y+20)*stride+2300*4..(y+20)*stride+(2300+fixture.width as usize)*4].copy_from_slice(&small[y*fixture.stride..y*fixture.stride+fixture.width as usize*4]);}
        }
        let mut times=vec![];let mut counts=vec![];
        for _ in 0..3{let start=Instant::now();let r=read_selection_count(RgbaView{width,height,stride,rgba:&p});times.push(start.elapsed().as_micros());counts.push(numeric(&r));}
        println!("{}",serde_json::json!({"manualScenario":kind,"width":width,"height":height,"readerUs":times,"counts":counts,"scope":"optimized static synthetic reader only, 3 samples; not end-to-end app latency"}));
        match kind{"count3_with_map"=>assert!(counts.iter().all(|v|*v==Some(3))),"zero_panel_with_map"=>assert!(counts.iter().all(|v|*v==Some(0))),_=>assert!(counts.iter().all(|v|v.is_none()))}
    }
}
