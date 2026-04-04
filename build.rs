use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    build_search_frontend();

    println!("cargo:rerun-if-changed=src/ghostty_runtime_shim.m");
    println!("cargo:rerun-if-changed=src/webview_shim.m");
    println!("cargo:rerun-if-changed=vendor/ghostty/include/ghostty.h");
    println!("cargo:rerun-if-changed=build.rs");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }

    cc::Build::new()
        .file("src/ghostty_runtime_shim.m")
        .file("src/webview_shim.m")
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
        "WebKit",
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

fn build_search_frontend() {
    let frontend_dir = Path::new("frontend/search-pane");
    if !frontend_dir.exists() {
        return;
    }

    println!("cargo:rerun-if-changed=frontend/search-pane/package.json");
    println!("cargo:rerun-if-changed=frontend/search-pane/bun.lock");
    println!("cargo:rerun-if-changed=frontend/search-pane/src/main.js");

    ensure_bun_dependencies(frontend_dir);

    let out_dir =
        PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR missing for search frontend build"));
    let output_file = out_dir.join("project_search_bundle.js");

    let status = Command::new("bun")
        .current_dir(frontend_dir)
        .args([
            "build",
            "./src/main.js",
            "--outfile",
            output_file
                .to_str()
                .expect("search frontend output path is not valid UTF-8"),
            "--target=browser",
            "--format=iife",
            "--minify",
        ])
        .status()
        .expect("failed to execute bun build for search frontend");

    if !status.success() {
        panic!("bun build failed while producing project search frontend bundle");
    }
}

fn ensure_bun_dependencies(frontend_dir: &Path) {
    let installed = frontend_dir
        .join("node_modules")
        .join("@pierre")
        .join("diffs")
        .join("package.json");
    if installed.exists() {
        return;
    }

    let lockfile = frontend_dir.join("bun.lock");
    let mut install = Command::new("bun");
    install.current_dir(frontend_dir);
    install.arg("install");
    if lockfile.exists() {
        install.arg("--frozen-lockfile");
    }

    let status = install
        .status()
        .expect("failed to execute bun install for search frontend");

    if !status.success() {
        panic!("bun install failed while preparing search frontend dependencies");
    }
}
