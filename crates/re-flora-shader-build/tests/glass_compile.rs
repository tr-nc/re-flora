use re_flora_shader_build::{NativeSlangCompiler, OptimizationLevel, NATIVE_SHADERS};
use std::path::Path;

// Exercise the production shader graph and both build-script compile modes. CPU-only
// optics tests cannot catch SPIR-V optimizer failures in the full lighting call graph.
// Run: cargo test --manifest-path crates/re-flora-shader-build/Cargo.toml --test glass_compile -- --ignored
#[test]
#[ignore = "requires a native Slang shared library; run explicitly for shader validation"]
fn glass_resolve_compiles_with_reflection_and_release_optimization() {
    let project_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let shader = NATIVE_SHADERS
        .iter()
        .find(|shader| shader.logical_path == "shader/tracer/glass_resolve.comp")
        .expect("Glass resolve must be registered for production compilation");
    let compiler = NativeSlangCompiler::new();
    for optimization in [OptimizationLevel::Zero, OptimizationLevel::Performance] {
        let output = compiler.compile_shader(shader, &project_root, optimization);
        assert!(output.spirv.len() >= 20, "SPIR-V must contain a header");
        assert_eq!(&output.spirv[..4], &0x0723_0203_u32.to_le_bytes());
        assert_eq!(output.spirv.len() % 4, 0);
        let source = project_root
            .join(shader.source_path)
            .canonicalize()
            .unwrap();
        assert!(output.dependencies.contains(&source));
    }
}
