use std::env;
use std::path::Path;

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

fn main() {
    dump_env();
}
