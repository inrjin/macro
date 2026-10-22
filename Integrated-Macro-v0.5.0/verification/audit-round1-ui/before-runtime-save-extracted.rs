#![allow(dead_code)]
use macro_core::{Color,Engine,Event,Phase,Point,Profile,Rect,RunMode,Target};
use serde::{Serialize,Deserialize};
use serde_json::{Value,json};
use std::path::PathBuf;
mod storage {
use serde_json::Value;
use std::{fs, io::Write, path::Path};

pub fn load(path: &Path) -> Result<Option<Value>, String> {
    if !path.exists() { return Ok(None); }
    let data = fs::read(path).map_err(|e| format!("설정 불러오기 실패: {e}"))?;
    if data.len() > 1_048_576 { return Err("설정 파일이 너무 큽니다.".into()); }
    serde_json::from_slice(&data).map(Some).map_err(|e| format!("설정 파일 형식 오류: {e}"))
}

pub fn save(path: &Path, value: &Value) -> Result<(), String> {
    let parent = path.parent().ok_or("설정 저장 위치를 확인하세요.")?;
    fs::create_dir_all(parent).map_err(|e| format!("설정 저장 폴더 생성 실패: {e}"))?;
    let suffix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default().as_nanos();
    let temp = parent.join(format!(".settings-{}-{suffix}.tmp", std::process::id()));
    let result = (|| {
        let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&temp)?;
        file.write_all(&serde_json::to_vec_pretty(value).map_err(std::io::Error::other)?)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        drop(file);
        replace(&temp, path)
    })();
    if result.is_err() { let _ = fs::remove_file(&temp); }
    result.map_err(|e| format!("설정 저장 실패: {e}"))
}

#[cfg(not(target_os = "windows"))]
fn replace(source: &Path, dest: &Path) -> std::io::Result<()> {
    fs::rename(source, dest)
}

#[cfg(target_os = "windows")]
fn replace(source: &Path, dest: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" { fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32; }
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let dest: Vec<u16> = dest.as_os_str().encode_wide().chain(Some(0)).collect();
    // REPLACE_EXISTING | WRITE_THROUGH; do not delete the previous profile first.
    if unsafe { MoveFileExW(source.as_ptr(), dest.as_ptr(), 0x1 | 0x8) } == 0 {
        Err(std::io::Error::last_os_error())
    } else { Ok(()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profile_replacement_preserves_unknown_desktop_fields() {
        let dir = std::env::temp_dir().join(format!("macro-storage-{}", std::process::id()));
        let path = dir.join("settings.json");
        save(&path, &serde_json::json!({"name":"first"})).unwrap();
        let second = serde_json::json!({"name":"second","desktop":{"future":{"x":2}}});
        save(&path, &second).unwrap();
        assert_eq!(load(&path).unwrap(), Some(second));
        fs::remove_dir_all(dir).unwrap();
    }
}


}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct DesktopConfig {
    sale_time: Option<String>,
    scheduled_start: bool,
    refresh_mode: String,
    selected_color: Option<Color>,
    stage_center: Option<Point>,
    region_roi: Option<Rect>,
    capture_reference: Option<FrameReference>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct FrameReference { width: usize, height: usize, scale_x: f64, scale_y: f64 }

fn seat_area_presence(profile: &Value) -> Value {
    json!({
        "area":profile.pointer("/vision/roi").is_some_and(|value| !value.is_null()),
        "start":profile.pointer("/desktop/capture_points/seat_start").is_some_and(|value| !value.is_null()),
        "end":profile.pointer("/desktop/capture_points/seat_end").is_some_and(|value| !value.is_null()),
    })
}
fn entry_click_enabled(profile: &Profile, desktop: &DesktopConfig, mode: RunMode) -> bool {
    mode == RunMode::Normal && (desktop.scheduled_start || profile.features.auto_entry)
}
fn reference_geometry_changed(previous: Option<&FrameReference>, next: Option<&FrameReference>) -> bool {
    match (previous, next) {
        (Some(previous), Some(next)) => previous.width != next.width || previous.height != next.height
            || (previous.scale_x - next.scale_x).abs() > 0.001
            || (previous.scale_y - next.scale_y).abs() > 0.001,
        _ => false,
    }
}

fn apply_capture(raw: &Value, field: &str, point: Point, color: Color, reference: &FrameReference) -> Result<Value, String> {
    let mut profile = raw.clone();
    if !profile.get("desktop").is_some_and(Value::is_object) { profile["desktop"] = json!({}); }
    let previous_reference = serde_json::from_value::<FrameReference>(profile["desktop"]["capture_reference"].clone()).ok();
    let changed_geometry = reference_geometry_changed(previous_reference.as_ref(), Some(reference));
    if changed_geometry {
        // A color sample cannot bless old coordinates for a new capture scale.
        for field in ["date","session","next","complete"] { profile["legacy"][field] = Value::Null; }
        if profile["legacy"]["cancellation_navigation"]["type"] == "two_regions" {
            profile["legacy"]["cancellation_navigation"] = json!({"type":"none"});
        }
        if let Some(regions) = profile["legacy"]["regions"].as_array_mut() {
            for region in regions { region["vision"] = Value::Null; region["complete"] = Value::Null; }
        }
        profile["vision"]["roi"] = Value::Null;
        profile["vision"]["exclusions"] = json!([]);
        profile["desktop"]["stage_center"] = Value::Null;
        profile["desktop"]["region_roi"] = Value::Null;
        profile["desktop"]["capture_points"] = json!({});
        profile["refresh_target"] = Value::Null;
        profile["entry_target"] = Value::Null;
    }
    if !profile["desktop"].get("capture_points").is_some_and(Value::is_object) {
        profile["desktop"]["capture_points"] = json!({});
    }
    profile["desktop"]["capture_reference"] = serde_json::to_value(reference).unwrap();
    if let Some(zone_field) = field.strip_prefix("legacy.regions.") {
        let (index, suffix) = zone_field.split_once('.').ok_or("구역별 좌표 채집 항목을 확인하세요.")?;
        let index: usize = index.parse().map_err(|_| "구역 번호를 확인하세요.")?;
        if index > 1 { return Err("구역 번호는 1구역 또는 2구역이어야 합니다.".into()); }
        if !profile["legacy"]["regions"].is_array() { profile["legacy"]["regions"] = json!([]); }
        let regions = profile["legacy"]["regions"].as_array_mut().unwrap();
        while regions.len() <= index { regions.push(json!({"color":null,"vision":null,"complete":null})); }
        match suffix {
            "color" => profile["legacy"]["regions"][index]["color"] = serde_json::to_value(color).unwrap(),
            "complete" => profile["legacy"]["regions"][index]["complete"] = serde_json::to_value(Target::Point {point}).unwrap(),
            "vision.roi.start" | "vision.roi.end" => {
                let end = if suffix.ends_with("start") { "start" } else { "end" };
                profile["desktop"]["capture_points"][format!("zone{index}_{end}")] = serde_json::to_value(point).unwrap();
                let points = &profile["desktop"]["capture_points"];
                let a = serde_json::from_value::<Point>(points[format!("zone{index}_start")].clone());
                let b = serde_json::from_value::<Point>(points[format!("zone{index}_end")].clone());
                if let (Ok(a),Ok(b)) = (a,b) {
                    let rect = Rect { x:a.x.min(b.x), y:a.y.min(b.y), width:(a.x-b.x).abs(), height:(a.y-b.y).abs() };
                    if !rect.valid() { return Err("탐색 범위의 두 모서리는 서로 다른 위치여야 합니다.".into()); }
                    let mut vision = profile["legacy"]["regions"][index]["vision"].clone();
                    if !vision.is_object() { vision = profile["vision"].clone(); }
                    vision["roi"] = serde_json::to_value(rect).unwrap();
                    profile["legacy"]["regions"][index]["vision"] = vision;
                }
            }
            _ => return Err("지원하지 않는 구역별 좌표 채집 항목입니다.".into()),
        }
        return Ok(profile);
    }
    match field {
        "color" => profile["color"] = serde_json::to_value(color).unwrap(),
        "desktop.region_color" => {
            profile["region_color"] = serde_json::to_value(color).unwrap();
            profile["desktop"]["region_color"] = serde_json::to_value(color).unwrap();
        }
        "desktop.selected_color" => profile["desktop"]["selected_color"] = serde_json::to_value(color).unwrap(),
        "desktop.stage_center" => profile["desktop"]["stage_center"] = serde_json::to_value(point).unwrap(),
        "legacy.date" | "legacy.session" | "legacy.next" | "legacy.complete" => {
            profile["legacy"][field.strip_prefix("legacy.").unwrap()] =
                serde_json::to_value(Target::Point { point }).unwrap();
        }
        "refresh_target" | "entry_target" => profile[field] = serde_json::to_value(Target::Point { point }).unwrap(),
        _ => {
            let key = match field {
                "vision.roi.start" => "seat_start",
                "vision.roi.end" => "seat_end",
                "desktop.region_roi.start" => "region_start",
                "desktop.region_roi.end" => "region_end",
                "legacy.region_first" => "legacy_region_first",
                "legacy.region_second" => "legacy_region_second",
                _ => return Err("지원하지 않는 좌표 채집 항목입니다.".into()),
            };
            profile["desktop"]["capture_points"][key] = serde_json::to_value(point).unwrap();
            let points = &profile["desktop"]["capture_points"];
            let region = if key.starts_with("seat_") { Some(("seat_start","seat_end",false)) }
                else if key.starts_with("region_") { Some(("region_start","region_end",true)) } else { None };
            if let Some((first, second, is_region)) = region {
                if let (Ok(a),Ok(b)) = (serde_json::from_value::<Point>(points[first].clone()), serde_json::from_value::<Point>(points[second].clone())) {
                    let rect = Rect { x: a.x.min(b.x), y: a.y.min(b.y), width: (a.x-b.x).abs(), height: (a.y-b.y).abs() };
                    if !rect.valid() { return Err("탐색 범위의 두 모서리는 서로 다른 위치여야 합니다.".into()); }
                    if is_region { profile["desktop"]["region_roi"] = serde_json::to_value(rect).unwrap(); }
                    else { profile["vision"]["roi"] = serde_json::to_value(rect).unwrap(); }
                }
            } else {
                let first = points["legacy_region_first"].clone();
                let second = points["legacy_region_second"].clone();
                if first.is_object() && second.is_object()
                    && profile["desktop"]["cancellation_mode"] == "legacyRegions" {
                    profile["legacy"]["cancellation_navigation"] = json!({
                        "type":"two_regions","first":{"type":"point","point":first},"second":{"type":"point","point":second}
                    });
                }
            }
        }
    }
    Ok(profile)
}

struct VisualRecovery;
impl VisualRecovery { fn reset(&mut self) {} }
enum Command { Overlay(bool) }
struct Worker {
 path:PathBuf, engine:Engine, profile:Profile, raw_profile:Value, desktop:DesktopConfig,
 target_recovery:Option<()>,clock:Option<()>,vision_diagnostics:Option<()>,visual_recovery:VisualRecovery,
 frame:Option<()>,last_sequence:Option<u64>,last_scan_generation:Option<u64>,candidates:Vec<()>,runtime_message:Option<String>,
}
impl Worker {
 fn record(&self,_kind:&str,_detail:Value) {}
 fn command(&mut self,_command:Command)->Result<Value,String>{Ok(Value::Null)}
    fn editable(&self) -> bool {
        matches!(self.engine.snapshot().phase, Phase::Idle | Phase::Stopped | Phase::Faulted | Phase::Completed)
    }
    fn save(&mut self, value: Value) -> Result<Value, String> {
        let profile = Profile::from_json(&value.to_string()).map_err(|e| e.to_string())?;
        let desktop: DesktopConfig = serde_json::from_value(value.get("desktop").cloned().unwrap_or(json!({})))
            .map_err(|e| format!("화면 설정 형식 오류: {e}"))?;
        let paused = self.engine.snapshot().phase == Phase::Paused;
        let changed_geometry = reference_geometry_changed(self.desktop.capture_reference.as_ref(), desktop.capture_reference.as_ref());
        if !self.editable() && !paused {
            return Err("설정을 바꾸려면 먼저 일시정지하세요.".into());
        }
        if paused {
            let mut previous = self.profile.clone();
            previous.vision = profile.vision.clone();
            previous.color = profile.color;
            previous.region_color = profile.region_color;
            previous.legacy.complete = profile.legacy.complete.clone();
            previous.legacy.regions = profile.legacy.regions.clone();
            previous.entry_target = profile.entry_target.clone();
            previous.name = profile.name.clone();
            if changed_geometry {
                // A resized/new booking window invalidates old coordinates. Only
                // clearing them is allowed here; arbitrary edits still fail below.
                if profile.legacy.date.is_none() { previous.legacy.date = None; }
                if profile.legacy.session.is_none() { previous.legacy.session = None; }
                if profile.legacy.next.is_none() { previous.legacy.next = None; }
                if profile.refresh_target.is_none() { previous.refresh_target = None; }
                if matches!(previous.legacy.cancellation_navigation, macro_core::CancellationNavigation::TwoRegions { .. })
                    && matches!(profile.legacy.cancellation_navigation, macro_core::CancellationNavigation::None) {
                    previous.legacy.cancellation_navigation = macro_core::CancellationNavigation::None;
                }
            }
            if serde_json::to_value(&previous).unwrap() != serde_json::to_value(&profile).unwrap() {
                return Err("일시정지 중에는 현재 화면의 탐색 범위·색상·무대 기준·선택 완료 위치만 바꿀 수 있습니다. 다른 설정을 바꾸려면 중지하세요.".into());
            }
        }
        let mut updated_engine = if paused { self.engine.clone() }
            else { Engine::new(profile.clone()).map_err(|e| e.to_string())? };
        if paused {
            if changed_geometry { updated_engine.invalidate_coordinate_geometry().map_err(|e| e.to_string())?; }
            updated_engine.update_calibration(profile.vision.clone(), profile.color, profile.region_color, profile.legacy.complete.clone())
                .map_err(|e| e.to_string())?;
            updated_engine.update_legacy_regions(profile.legacy.regions.clone()).map_err(|e| e.to_string())?;
            let entry_target = if entry_click_enabled(&profile, &desktop, self.engine.snapshot().mode) {
                profile.entry_target.clone()
            } else { None };
            updated_engine.update_entry_target(entry_target).map_err(|e| e.to_string())?;
        }
        storage::save(&self.path, &value)?;
        let previous_area = seat_area_presence(&self.raw_profile);
        let next_area = seat_area_presence(&value);
        if previous_area != next_area {
            // Presence only: a missing value does not prove the user pressed
            // Clear. This also exposes stale full-profile saves after F8.
            self.record("seat_area_settings_changed", json!({"before":previous_area,"after":next_area}));
        }
        self.target_recovery = None;
        if self.profile.server_url != profile.server_url { self.clock = None; }
        self.engine = updated_engine;
        self.profile = profile;
        self.desktop = desktop;
        self.raw_profile = value;
        self.vision_diagnostics = None;
        if changed_geometry {
            self.visual_recovery.reset();
            self.frame = None;
            self.last_sequence = None;
            self.last_scan_generation = None;
            self.candidates.clear();
        }
        self.runtime_message = Some("설정 저장 완료".into());
        if let Err(error) = self.command(Command::Overlay(self.profile.features.status_overlay)) {
            self.runtime_message = Some(format!("설정 저장 완료 · {error}"));
        }
        Ok(self.raw_profile.clone())
    }

}
fn main() {
 let ingress:Value=serde_json::from_slice(&std::fs::read("/tmp/macro-ui-stale-save-ingress.json").unwrap()).unwrap();
 for paused in [false,true] {
  let before=ingress["before"].clone();
  let stale=ingress["savePayload"].clone();
  let profile=Profile::from_json(&before.to_string()).unwrap();
  let mut engine=Engine::new(profile.clone()).unwrap();
  if paused {engine.step(Event::Start{mode:RunMode::Seats},0);engine.step(Event::Pause,1); assert_eq!(engine.snapshot().phase,Phase::Paused);}
  let path=PathBuf::from(format!("/tmp/macro-runtime-save-audit/settings-{}.json",paused));
  let mut worker=Worker {path:path.clone(),engine,profile,desktop:serde_json::from_value(before["desktop"].clone()).unwrap(),raw_profile:before,
   target_recovery:None,clock:None,vision_diagnostics:None,visual_recovery:VisualRecovery,frame:None,last_sequence:None,last_scan_generation:None,candidates:vec![],runtime_message:None};
  let c=&ingress["capture"];
  let updated=apply_capture(&worker.raw_profile,c["field"].as_str().unwrap(),serde_json::from_value(c["point"].clone()).unwrap(),serde_json::from_value(c["color"].clone()).unwrap(),&serde_json::from_value(c["frame"].clone()).unwrap()).unwrap();
  worker.save(updated).unwrap();
  assert_eq!(worker.profile.vision.roi,Some(Rect{x:100.,y:120.,width:600.,height:500.}));
  assert!(worker.raw_profile["desktop"]["capture_points"]["seat_end"].is_object());
  worker.save(stale).unwrap();
  let disk=storage::load(&path).unwrap().unwrap();
  assert_eq!(worker.profile.vision.roi,None);
  assert_eq!(worker.engine.profile().vision.roi,None);
  assert_eq!(worker.raw_profile["vision"]["roi"],Value::Null);
  assert_eq!(disk["vision"]["roi"],Value::Null);
  assert_eq!(disk["desktop"]["capture_points"]["seat_end"],Value::Null);
  println!("REPRODUCED native save body: paused={paused}, F8 ROI saved then UI stale payload removes endpoint and ROI from raw/profile/engine/disk");
 }
 println!("Limit: apply_capture, save, editable, geometry and storage copied verbatim from current production source; real macro-core Engine; only overlay/logging stubbed, no actual Worker/Tauri/OS");
}
