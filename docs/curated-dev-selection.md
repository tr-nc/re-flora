# Curated `dev` feature selection

> Historical integration record: the frozen GLSL fallback described here was removed after the native-only transition completed.

This branch starts from `main` at `f15f29f2` and imports only changes that preserve rendered appearance. Native Slang is the shader source of truth; legacy GLSL fallback sources are unchanged.

## Selected

| Source work | Decision | Reason |
|---|---|---|
| Performance benchmark suite (`e8c24526`) | Keep | Tooling and documentation only. |
| Native Slang default | Keep | Makes the completed native inventory the development path; `--no-default-features` preserves frozen GLSL rollback. |
| Surface workgroup atomic aggregation (`906a9214`) | Keep, Slang-only | Changes extraction bookkeeping, not voxel selection or packed surface values. Workload signatures matched. |
| Integer surface-normal accumulation (`03064468`) | Keep, Slang-only | The 5×5×5 estimator is unchanged; bounded integer sums are exactly representable and produce the same normalized input. |

## Excluded because they intentionally affect appearance or gameplay

| Source work | Decision |
|---|---|
| Kochia profile and palette simplification (`28362d86`) | Exclude. |
| Hybrid rasterized tiny tree branches and leaf sprays (`33cad8a8`) | Exclude. |
| Irrigation pipe network (`d79968b9`) | Exclude; adds visible content and gameplay behavior. |
| Compact 3×3×3 normal estimator (`4ceea9bf`) | Exclude; intentionally changes terrain shading. |

## Excluded or deferred despite appearance-neutral intent

| Source work | Decision | Reason |
|---|---|---|
| Contree stack clearing removal (`8d9aeac2`) | Exclude | Release timing was neutral; avoid taking traversal risk without a measured benefit. |
| Unroll and workgroup experiments (`1b22f4aa`, `8a7ff00f`, `5915a6fe`, `1c551cae`) | Exclude | Rejected or documentation-only experiments. |
| Static gameplay tracer (`c2affe79`) | Defer | The measured idea is promising, but the existing implementation adds a GLSL entry and shared GLSL implementation. Reimplement later as a native-only entry after the build manifest supports Slang entries without physical GLSL reference files. |
| Payload audit and tracer report commits | Exclude | Documentation for a tracer implementation not selected here. |

## Validation

- Default `cargo check` compiled 76 native Slang entries; `cargo check --no-default-features` compiled the 76-entry frozen GLSL fallback.
- `cargo test`: 166 passed, 1 ignored.
- Hidden muted release smoke completed without Vulkan or shader errors.
- Native Slang surface A/B runs retained matching active voxel, active brick, and solid workgroup signatures.
- Four `player-default` screenshots showed only normal temporal scene variation: same-build repeat RMSE was 0.00754–0.00872 and cross-build RMSE was 0.00474–0.01001, with no visible geometry, material, terrain-shading, silhouette, or depth change. Local captures are `/tmp/curated-{baseline,current}-{1,2}.png`.
