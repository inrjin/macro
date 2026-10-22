//! CoreText independently renders complete strings. A single-digit count must
//! not be inferred by dropping part of a compound numeric expression. These
//! tests do not claim that unsupported expressions are valid NOL totals.
use count_scope_experiment::{selection_count::{diagnose_selection_count, read_selection_count, CountIssue, CountRead}, vision::RgbaView};
use serde::Deserialize;
use std::{fs, path::PathBuf};

#[derive(Deserialize)]
struct Fixture { name:String, width:u32, height:u32, stride:usize, expected:Option<usize> }
fn root()->PathBuf { PathBuf::from("/Users/inrjin/Documents/機械人間/Integrated-Macro-v0.5.0/crates/macro-core").join("../../verification/selection-count-audit/implementation") }
fn fixtures()->Vec<Fixture> {serde_json::from_slice(&fs::read(root().join("manifest.json")).unwrap()).unwrap()}
fn read(f:&Fixture)->CountRead {
    let rgba=fs::read(root().join("fixtures").join(format!("{}.rgba",f.name))).unwrap();
    read_selection_count(RgbaView{width:f.width,height:f.height,stride:f.stride,rgba:&rgba})
}

#[test]
fn complete_single_digit_controls_remain_readable() {
    let mut failures=vec![];
    for f in fixtures().iter().filter(|f|f.expected.is_some()) {
        let result=read(f);
        println!("{}: {result:?}",f.name);
        // Larger gaps are coverage probes, not a newly promised layout. The
        // default 6/12px controls must recognize, so reject-all cannot pass.
        if f.name.starts_with("gap6_") || f.name.starts_with("gap12_") || !f.name.starts_with("gap") {
            if !matches!(result,CountRead::Confirmed{count,..} if Some(count)==f.expected) {failures.push(f.name.clone());}
        } else if let CountRead::Confirmed{count,..}=result {assert_eq!(Some(count),f.expected);}
    }
    assert!(failures.is_empty(),"{}",failures.join("\n"));
}

#[test]
fn compound_numeric_context_cannot_be_truncated_to_one_digit() {
    let mut failures=vec![];
    for f in fixtures().iter().filter(|f|f.expected.is_none()) {
        if let result @ CountRead::Confirmed{..}=read(f) {
            failures.push(format!("{}: {result:?}",f.name));
        }
    }
    assert!(failures.is_empty(),"{}",failures.join("\n"));
}

#[test]
fn diagnostic_reasons_share_the_legacy_result_without_authorizing_unknowns() {
    let cases=fixtures();
    for (name,issue) in [
        ("missing_count",Some(CountIssue::CountMissing)),
        ("unknown_digit",Some(CountIssue::DigitUnrecognized)),
        ("prefixed_heading",Some(CountIssue::HeadingContext)),
        ("duplicate_heading",Some(CountIssue::HeadingAmbiguous)),
        ("gap32_12",Some(CountIssue::NumericContext)),
        ("gap6_1",None),
    ] {
        let f=cases.iter().find(|f|f.name==name).unwrap();
        let rgba=fs::read(root().join("fixtures").join(format!("{name}.rgba"))).unwrap();
        let frame=RgbaView{width:f.width,height:f.height,stride:f.stride,rgba:&rgba};
        let diagnosis=diagnose_selection_count(frame);
        assert_eq!(diagnosis.issue,issue,"{name}: {diagnosis:?}");
        assert_eq!(diagnosis.read,read_selection_count(frame));
        assert_eq!(matches!(diagnosis.read,CountRead::Unreadable),issue.is_some());
    }
    assert_eq!(diagnose_selection_count(RgbaView{width:0,height:0,stride:0,rgba:&[]}).issue,Some(CountIssue::InvalidFrame));
    let mut blank=vec![255;100*60*4];
    fn frame(bytes:&[u8])->RgbaView<'_> {RgbaView{width:100,height:60,stride:400,rgba:bytes}}
    assert_eq!(diagnose_selection_count(frame(&blank)).issue,Some(CountIssue::HeadingMissing));
    blank[3]=249;
    assert_eq!(diagnose_selection_count(frame(&blank)).issue,Some(CountIssue::IncompleteCapture));

    let old=PathBuf::from("/Users/inrjin/Documents/機械人間/Integrated-Macro-v0.5.0/crates/macro-core").join("../../verification/seat-object-system/reader-review/fixtures/panel_empty_no_button.rgba");
    let rgba=fs::read(old).unwrap();
    let diagnosis=diagnose_selection_count(RgbaView{width:640,height:300,stride:2560,rgba:&rgba});
    assert_eq!(diagnosis.issue,Some(CountIssue::EmptyLayoutUnsupported));
}
