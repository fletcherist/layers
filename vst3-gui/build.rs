use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let vst3_sdk_path = ensure_vst3_sdk().expect("VST3 SDK is required for vst3-gui");

    let sdk_path_abs = if vst3_sdk_path.is_absolute() {
        vst3_sdk_path
    } else {
        env::current_dir().unwrap().join(&vst3_sdk_path)
    };

    let dst = cmake::Config::new("cpp")
        .define("CMAKE_BUILD_TYPE", "Release")
        .define("VST3_SDK_PATH", sdk_path_abs.to_str().unwrap())
        .profile("Release")
        .build();

    println!("cargo:rustc-link-search=native={}/lib", dst.display());
    println!("cargo:rustc-link-lib=static=vst3_gui");

    // Platform-specific linking
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    match target_os.as_str() {
        "macos" => {
            println!("cargo:rustc-link-lib=c++");
            println!("cargo:rustc-link-lib=framework=AppKit");
            println!("cargo:rustc-link-lib=framework=Cocoa");
            println!("cargo:rustc-link-lib=framework=CoreFoundation");
        }
        "windows" => {
            println!("cargo:rustc-link-lib=ole32");
            println!("cargo:rustc-link-lib=user32");
        }
        _ => {}
    }

    // Rerun if any source changes
    println!("cargo:rerun-if-changed=cpp/src/vst3_gui_common.cpp");
    println!("cargo:rerun-if-changed=cpp/src/vst3_gui_mac.mm");
    println!("cargo:rerun-if-changed=cpp/src/vst3_gui_win.cpp");
    println!("cargo:rerun-if-changed=cpp/include/vst3_gui.h");
    println!("cargo:rerun-if-changed=cpp/include/vst3_gui_internal.h");
    println!("cargo:rerun-if-changed=cpp/CMakeLists.txt");
}

fn ensure_vst3_sdk() -> Option<PathBuf> {
    let sdk_path = PathBuf::from("cpp/external/vst3sdk");

    // Check if SDK already exists with actual content
    if sdk_path.exists()
        && sdk_path.join("CMakeLists.txt").exists()
        && sdk_path.join("pluginterfaces/base/funknown.cpp").exists()
        && sdk_path.join("public.sdk/source/common/commoniids.cpp").exists()
    {
        eprintln!("VST3 SDK found at {}", sdk_path.display());
        return Some(sdk_path);
    }

    eprintln!("Cloning VST3 SDK to {}...", sdk_path.display());

    if let Some(parent) = sdk_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let status = Command::new("git")
        .args([
            "clone",
            "--recursive",
            "--depth=1",
            "https://github.com/steinbergmedia/vst3sdk.git",
            sdk_path.to_str().unwrap(),
        ])
        .status();

    match status {
        Ok(s) if s.success() => {
            eprintln!("VST3 SDK cloned successfully");
            Some(sdk_path)
        }
        Ok(s) => {
            eprintln!("Failed to clone VST3 SDK (exit code: {:?})", s.code());
            None
        }
        Err(e) => {
            eprintln!("Failed to execute git clone: {}", e);
            None
        }
    }
}
