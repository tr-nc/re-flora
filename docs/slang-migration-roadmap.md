# Native Slang shader architecture

The shader migration is complete. Native Slang is the only shader source language in the repository and the only compiler path used by normal, release, and `--no-default-features` builds.

Historical migration measurements and frontend comparisons remain in [`slang-poc.md`](slang-poc.md). The active validation policy is in [`slang-validation-plan.md`](slang-validation-plan.md).

## Source layout

- `shader/slang/*.slang` contains all 76 production entry points and their imported modules.
- `crates/re-flora-shader-build/src/lib.rs` owns the `NATIVE_SHADERS` manifest and the shared in-process Slang compiler integration.
- `crates/re-flora-vkn/build.rs` compiles reflection and optimized SPIR-V artifacts for Vulkan pipeline creation.
- The root `build.rs` compiles the manifest entries needed to generate Rust CPU/GPU ABI structs.
- Logical pipeline paths retain their historical `.comp`, `.vert`, and `.frag` identities so Rust call sites do not depend on physical shader source paths.

There are no checked-in GLSL entry points or include files. Shared shader behavior belongs in imported `.slang` modules rather than textual includes.

## Build contract

A normal build always:

1. loads the pinned-compatible Slang compiler API;
2. validates the 76-entry native manifest;
3. compiles reflection and performance-optimized SPIR-V for each entry;
4. tracks compiler-reported module dependencies in the artifact cache;
5. generates Rust ABI structs from native Slang reflection.

`--no-default-features` changes no shader frontend behavior. The old per-family `slang-*` feature switches and aggregate `slang-validation` feature were removed because there is no alternate shader implementation to select.

## Validation gate

Run this ladder after shader or shader-build changes:

```bash
python3 scripts/check_shader_manifest.py
cargo fmt --check
cargo check
cargo test
cargo run --release -- --hidden --mute --auto-exit 0.5
cargo run --release -- --latest-log
```

The manifest checker rejects non-Slang files under `shader/`, textual includes, shared sources without module declarations, duplicate logical/source entries, missing entry points, missing module directories, and stage/logical-extension mismatches.

## Completed cleanup

- [x] Port all 76 production entry points to native Slang.
- [x] Consolidate shared shader behavior into 133 native Slang entry/module files.
- [x] Make Slang compilation unconditional.
- [x] Share compiler and manifest code between both build scripts.
- [x] Generate Rust GPU structs from native Slang SPIR-V.
- [x] Remove shaderc and mixed-frontend build logic.
- [x] Remove per-shader migration feature flags.
- [x] Remove all legacy `.comp`, `.vert`, `.frag`, and `.glsl` source files.
- [x] Replace the migration inventory checker with the native manifest checker.

## Future shader work

New shader entry points must be added as `.slang` files and registered once in `NATIVE_SHADERS`. Reusable types and algorithms should be public declarations in focused Slang modules under `shader/slang/`. Do not add a parallel source-language implementation or a preprocessor-based include tree.
