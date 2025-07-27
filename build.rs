use std::env;
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

fn main() {
    dump_env();

    // this is disabled for now
    // pre_compile_shaders();
}

#[allow(dead_code)]
fn pre_compile_shaders() {
    use shaderc;
    use std::fs;

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

    let out_dir = env::var("OUT_DIR").expect("Failed to read OUT_DIR.");
    let shader_root_out_dir = PathBuf::from(&out_dir).join("shaders_root");
    if shader_root_out_dir.exists() {
        fs::remove_dir_all(&shader_root_out_dir).expect("Failed to remove old shader_root folder.");
    }

    fs::create_dir_all(&shader_root_out_dir).expect("Failed to recreate empty shader_root folder.");

    let shader_dir = Path::new("src/");

    let shader_extensions = [".vert", ".frag", ".comp"];

    let compiler = shaderc::Compiler::new().expect("Failed to create shader compiler.");

    let mut compile_options =
        shaderc::CompileOptions::new().expect("Failed to initialize shader compile options.");
    compile_options.set_optimization_level(shaderc::OptimizationLevel::Performance);

    // recursively compile all shaders
    visit_shader_files(shader_dir, &shader_extensions, &mut |path| {
        let stage = match path.extension().and_then(|ext| ext.to_str()) {
            Some("vert") => shaderc::ShaderKind::Vertex,
            Some("frag") => shaderc::ShaderKind::Fragment,
            Some("comp") => shaderc::ShaderKind::Compute,
            _ => return,
        };

        let source = fs::read_to_string(path)
            .unwrap_or_else(|_| panic!("Failed to read shader source: {}", path.display()));

        // compile to SPIR-V
        let compiled_spirv = compiler
            .compile_into_spirv(
                &source,
                stage,
                path.to_str().unwrap(),
                "main", // entry point name
                Some(&compile_options),
            )
            .unwrap_or_else(|e| panic!("Failed to compile {}: {}", path.display(), e));

        let project_root = Path::new(env!("CARGO_MANIFEST_DIR"));
        log!("Project root: {}", project_root.display());
        log!("Shader path: {}", path.display());
        let relative_path = path;

        // construct the final output path inside OUT_DIR/shaders_root
        let out_path = shader_root_out_dir
            .join(relative_path)
            .with_extension(format!(
                "{}.spv",
                path.extension().and_then(|x| x.to_str()).unwrap()
            ));

        // create directories if needed
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)
                .unwrap_or_else(|_| panic!("Failed to create directories for {:?}", parent));
        }

        // write the compiled SPIR-V
        fs::write(&out_path, &compiled_spirv.as_binary_u8())
            .unwrap_or_else(|_| panic!("Failed to write SPIR-V file: {:?}", out_path));

        log!("Compiled: {} -> {}", path.display(), out_path.display());
    });
}
