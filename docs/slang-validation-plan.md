# Native Slang validation plan

Native Slang is the sole production shader source. This document defines the current acceptance gate;
the completed mixed-frontend experiment remains available in Git history at
[`003e535d`](https://github.com/tr-nc/re-flora/blob/003e535dc26bf877c6dd5c3e643b4c2d5549a9aa/docs/slang-poc.md).

## Inventory and ownership

The authoritative entry-point inventory is `NATIVE_SHADERS` in `crates/re-flora-shader-build/src/lib.rs`. Each record owns:

- a stable logical pipeline path;
- one physical `.slang` entry-point path;
- shader stage;
- Slang module search directory.

The shared `re-flora-shader-build` crate owns Slang compiler discovery, session configuration, dependency reporting, and SPIR-V generation. Both Vulkan artifact compilation and root GPU-struct code generation consume this API.

Run:

```bash
python3 scripts/check_shader_manifest.py
```

This must report 76 manifest entries and 133 Slang files. Any non-`.slang` file under `shader/` is an error.

## Compile and ABI gate

```bash
cargo fmt --check
cargo check
```

`cargo check` is the native-Slang compile gate. It must:

- load the Slang 2025 compiler API;
- compile all 76 entry points at reflection and performance optimization levels;
- emit Vulkan SPIR-V 1.6;
- resolve all imported Slang modules;
- validate stage/logical-path consistency and source existence;
- generate valid Rust CPU/GPU ABI structs from native reflection;
- compile every Rust pipeline and resource definition against those structs.

The logical path is an API identity, not a physical source. Its `.comp`, `.vert`, or `.frag` suffix remains stable for Rust pipeline call sites while its manifest record points to `.slang` source.

## Unit gate

```bash
cargo test
```

Shader policy tests must inspect the authoritative `.slang` module or entry point. Do not add tests that parse removed GLSL sources or duplicate shader constants in test fixtures.

## Runtime gate

```bash
cargo run --release -- --hidden --mute --auto-exit 0.5
cargo run --release -- --latest-log
```

The hidden release smoke must create all required Vulkan pipelines and exit without validation errors, shader compile failures, panics, or resource-layout lookup failures. Release mode is authoritative for performance evidence.

## CI gate

`.github/workflows/shader-validation.yml` installs the checksum-pinned Slang toolchain, runs the manifest checker, compiles the unconditional native inventory, validates emitted SPIR-V, and runs configured hidden release smokes. CI no longer compares frontends or passes a `slang-validation` feature.

## Change policy

For a new entry point:

1. add one `.slang` source under `shader/slang/`;
2. extract reusable declarations into focused imported `.slang` modules;
3. add one `NATIVE_SHADERS` record;
4. add the logical path to root GPU-struct reflection only if the CPU consumes its ABI;
5. run the complete validation ladder.

For an existing entry point, edit the native source/module graph directly. Do not add a second shader language, textual GLSL include, shaderc path, frontend feature switch, or hand-edited generated Rust output.

## Performance gate

Shader performance changes use release-mode application runs and the scenario suite in [`performance-benchmarking.md`](performance-benchmarking.md). Use order-reversed A/B execution (`A,B,B,A`) and require matching active-voxel, active-brick, and solid-workgroup signatures before comparing timings.
