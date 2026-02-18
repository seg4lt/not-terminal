use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=src/ghostty_runtime_shim.c");
    println!("cargo:rerun-if-changed=vendor/ghostty/include/ghostty.h");
    println!("cargo:rerun-if-changed=build.rs");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }

    cc::Build::new()
        .file("src/ghostty_runtime_shim.c")
        .include("vendor/ghostty/include")
        .flag_if_supported("-std=c11")
        .compile("ghostty_runtime_shim");

    let static_lib = resolve_static_lib_path();
    if !static_lib.exists() {
        build_ghostty_static();
    }
    if !static_lib.exists() {
        panic!(
            "Ghostty static library not found at {}",
            static_lib.display()
        );
    }

    let static_lib_dir = static_lib
        .parent()
        .expect("static library path has no parent directory");

    println!(
        "cargo:rustc-link-search=native={}",
        static_lib_dir.display()
    );
    println!("cargo:rustc-link-lib=static=ghostty-fat");
    println!("cargo:rustc-link-lib=c++");

    // Frameworks required by libghostty on macOS.
    for framework in [
        "AppKit",
        "Foundation",
        "Carbon",
        "CoreFoundation",
        "CoreGraphics",
        "CoreText",
        "CoreVideo",
        "QuartzCore",
        "IOSurface",
        "Metal",
        "UniformTypeIdentifiers",
        "SystemConfiguration",
        "Security",
    ] {
        println!("cargo:rustc-link-lib=framework={framework}");
    }
}

fn resolve_static_lib_path() -> PathBuf {
    if let Ok(path) = env::var("GHOSTTY_STATIC_LIB") {
        let path = PathBuf::from(path);
        if !path.as_os_str().is_empty() {
            return path;
        }
    }

    PathBuf::from("vendor/ghostty/macos/GhosttyKit.xcframework/macos-arm64/libghostty-fat.a")
}

fn build_ghostty_static() {
    let ghostty_dir = Path::new("vendor/ghostty");
    if !ghostty_dir.exists() {
        panic!("vendor/ghostty is missing; initialize submodules first");
    }

    let status = Command::new("zig")
        .current_dir(ghostty_dir)
        .args([
            "build",
            "-Dapp-runtime=none",
            "-Demit-xcframework=true",
            "-Dxcframework-target=native",
            "-Doptimize=ReleaseFast",
        ])
        .status()
        .expect("failed to execute zig build for Ghostty static library");

    if !status.success() {
        panic!("zig build failed while producing Ghostty static library");
    }
}
