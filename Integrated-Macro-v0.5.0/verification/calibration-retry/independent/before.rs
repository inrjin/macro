use macro_core::*;
use std::time::Instant;
#[path="/Users/inrjin/Documents/機械人間/Integrated-Macro-v0.5.0/src-tauri/src/visual_recovery.rs"] mod visual_recovery;
use visual_recovery::VisualRecovery;
#[derive(Clone,Debug)] struct FrameReference {width:usize,height:usize,scale_x:f64,scale_y:f64}
include!("baseline-helpers.rs");
#[path="before-tests.rs"] mod calibration_restart_review;
