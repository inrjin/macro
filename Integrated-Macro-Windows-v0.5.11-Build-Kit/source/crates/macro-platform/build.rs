use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=native/MacPlatform.swift");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    let arch = env::var("CARGO_CFG_TARGET_ARCH").expect("target architecture");
    let swift_arch = if arch == "aarch64" { "arm64" } else { &arch };
    let sdk = Command::new("xcrun")
        .arg("--show-sdk-path")
        .output()
        .expect("Xcode Command Line Tools의 xcrun이 필요합니다.");
    assert!(sdk.status.success(), "macOS SDK 경로를 찾지 못했습니다.");
    let sdk = String::from_utf8(sdk.stdout).expect("SDK path UTF-8");
    let status = Command::new("xcrun")
        .args(["swiftc", "-swift-version", "5", "-O", "-parse-as-library"])
        .args(["-emit-library", "-static", "-module-name", "MacroNative"])
        .args(["-target", &format!("{swift_arch}-apple-macosx13.0")])
        .args(["-sdk", sdk.trim()])
        .arg("-module-cache-path")
        .arg(out.join("swift-module-cache"))
        .arg("native/MacPlatform.swift")
        .arg("-o")
        .arg(out.join("libMacroNative.a"))
        .status()
        .expect("Swift 네이티브 모듈을 빌드할 수 없습니다.");
    assert!(status.success(), "MacPlatform.swift 빌드 실패");
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-search=native={}/usr/lib/swift", sdk.trim());
    // Swift compatibility libraries live next to swiftc, not in the system SDK.
    if let Ok(tool) = Command::new("xcrun").args(["--find", "swiftc"]).output() {
        if let Ok(path) = String::from_utf8(tool.stdout) {
            if let Some(root) = PathBuf::from(path.trim()).parent().and_then(|p| p.parent()) {
                println!("cargo:rustc-link-search=native={}/lib/swift/macosx", root.display());
                println!("cargo:rustc-link-search=native={}/lib/swift-5.5/macosx", root.display());
            }
        }
    }
    println!("cargo:rustc-link-lib=static=MacroNative");
    for framework in ["AppKit", "ScreenCaptureKit", "CoreMedia", "CoreVideo", "CoreGraphics", "ApplicationServices", "Carbon", "Foundation"] {
        println!("cargo:rustc-link-lib=framework={framework}");
    }
    println!("cargo:rustc-link-lib=dylib=swiftCore");
    println!("cargo:rustc-link-lib=dylib=swiftFoundation");
    println!("cargo:rustc-link-lib=dylib=swiftDispatch");
}
