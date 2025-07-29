// build.rs

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => {{
        let message = format!($($arg)*);
        println!("cargo:warning={}", message);
    }};
}

fn dump_env() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is not set");
    let target_dir = env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| {
        let default = Path::new(&manifest_dir).join("target");
        default.to_str().unwrap().to_owned()
    });
    println!("cargo:rustc-env=PROJECT_ROOT={}/", manifest_dir);
    println!("cargo:rustc-env=TARGET_DIR={}/", target_dir);
}

/// Sets the link-time search path for the native library.
fn link_native_library(lib_path: &Path) {
    // tell rustc where to find the native static library (.lib) for linking.
    println!("cargo:rustc-link-search=native={}", lib_path.display());
}

/// Copies the necessary runtime .dll files to the correct output directories.
fn copy_runtime_dlls(source_path: &Path) {
    // use the PROFILE env var to determine if we are in "debug" or "release" mode.
    let profile = env::var("PROFILE").unwrap();

    // the OUT_DIR is deep inside the target directory. we need to find the root of the target folder.
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let target_root = out_dir
        .ancestors()
        .find(|p| p.ends_with("target"))
        .expect("failed to find target directory");

    // the destination for the main executable (from `cargo run`).
    let exe_dest = target_root.join(&profile);

    // the destination for test executables (from `cargo test`).
    let deps_dest = exe_dest.join("deps");

    // ensure both directories exist.
    fs::create_dir_all(&exe_dest).unwrap();
    fs::create_dir_all(&deps_dest).unwrap();

    // iterate over the source directory and copy all dll files.
    for entry in fs::read_dir(source_path).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().map_or(false, |ext| ext == "dll") {
            let file_name = path.file_name().unwrap();

            // copy to the main exe directory.
            fs::copy(&path, exe_dest.join(file_name)).expect("failed to copy dll to exe dir");

            // copy to the deps directory for tests.
            fs::copy(&path, deps_dest.join(file_name)).expect("failed to copy dll to deps dir");
        }
    }
}

/// Determines library paths and calls the linking and copying functions.
fn setup_steam_audio() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();

    let lib_source_path = match (target_os.as_str(), target_arch.as_str()) {
        ("windows", "x86_64") => manifest_dir.join("deps/steam-audio-4.6.1-windows-x64"),
        _ => panic!("unsupported target platform for steam audio"),
    };

    // solve the link-time problem.
    link_native_library(&lib_source_path);

    // solve the run-time problem for windows.
    if target_os == "windows" {
        copy_runtime_dlls(&lib_source_path);
    }

    println!("cargo:rerun-if-changed=build.rs");
    // it's also good practice to tell cargo to rerun if the dlls change.
    println!("cargo:rerun-if-changed=deps/");
}

fn main() {
    dump_env();
    setup_steam_audio();
}
