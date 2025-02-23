// build.rs
use shaderc;
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

fn main() {
    // Where Cargo places its build artifacts
    let out_dir = env::var("OUT_DIR").expect("Failed to read OUT_DIR.");

    log!("OUT_DIR: {}", out_dir);

    let shader_dir = Path::new("src/egui_renderer/shaders");

    let shader_extensions = [".vert", ".frag", ".comp"];

    let compiler = shaderc::Compiler::new().expect("Failed to create shader compiler.");

    let mut compile_options =
        shaderc::CompileOptions::new().expect("Failed to initialize shader compile options.");
    compile_options.set_optimization_level(shaderc::OptimizationLevel::Performance);

    // Recursively walk the shader directory
    visit_shader_files(shader_dir, &shader_extensions, &mut |path| {
        println!("cargo:rerun-if-changed={}", path.display());

        let stage = match path.extension().and_then(|ext| ext.to_str()) {
            Some("vert") => shaderc::ShaderKind::Vertex,
            Some("frag") => shaderc::ShaderKind::Fragment,
            Some("comp") => shaderc::ShaderKind::Compute,
            _ => return, // Unrecognized extension, skip
        };

        let source = fs::read_to_string(path)
            .unwrap_or_else(|_| panic!("Failed to read shader source: {}", path.display()));

        // Compile to SPIR-V
        let compiled_spirv = compiler
            .compile_into_spirv(
                &source,
                stage,
                path.to_str().unwrap(),
                "main", // entry point name
                Some(&compile_options),
            )
            .unwrap_or_else(|e| panic!("Failed to compile {}: {}", path.display(), e));

        // Construct output path in OUT_DIR: e.g., $OUT_DIR/shader.frag.spv
        let out_path = PathBuf::from(&out_dir)
            .join(path.file_stem().unwrap())
            .with_extension(format!(
                "{}.spv",
                path.extension().and_then(|x| x.to_str()).unwrap()
            ));

        fs::write(&out_path, &compiled_spirv.as_binary_u8())
            .unwrap_or_else(|_| panic!("Failed to write SPIR-V file: {:?}", out_path));

        log!("Compiled: {} -> {}", path.display(), out_path.display());
    });
}

/// Recursively visits all files in `dir` that match one of the given `extensions`.
fn visit_shader_files<F: FnMut(&Path)>(dir: &Path, extensions: &[&str], callback: &mut F) {
    if !dir.exists() {
        panic!("Shader directory not found: {}", dir.display());
    }

    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();

        if path.is_dir() {
            visit_shader_files(&path, extensions, callback);
        } else if let Some(ext) = path.extension().and_then(|x| x.to_str()) {
            if extensions
                .iter()
                .any(|e| ext.eq_ignore_ascii_case(e.trim_start_matches('.')))
            {
                callback(&path);
            }
        }
    }
}
