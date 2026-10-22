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

