# VKN Ping-Pong Texture Wrapper Progress

## Goal

Reduce manual management of repeated GPU texture pairs/triples used for ping-pong blur passes and temporal history by introducing a small, practical wrapper at the `verdarium-vkn` boundary or a thin layer immediately above it.

Done means:

- We have a clear design decision for where the abstraction lives (`verdarium-vkn` vs tracer-side helper layer).
- The chosen wrapper shape is small and mechanical rather than algorithm-specific.
- The design covers the main existing patterns in the codebase without forcing unrelated resources into one abstraction.
- A first implementation target is identified, along with a validation plan.

## Current State

Known from inspection and discussion:

- The repo already has several repeated texture-role patterns:
  - ping-pong scratch pairs for blur/filter work
  - current/prev temporal history pairs
  - raw/history/output triples
- Current manual examples include:
  - `src/tracer/resources.rs`
    - `shadow_map_tex_for_vsm_ping`
    - `shadow_map_tex_for_vsm_pong`
    - `shadow_map_tex_for_vsm_prev`
    - `cloud_shadow_raw_tex`
    - `cloud_shadow_history_tex`
    - `cloud_shadow_tex`
    - `leaf_shadow_opacity_tex`
    - `leaf_shadow_opacity_prev_tex`
    - `leaf_shadow_opacity_blended_tex`
  - `src/tracer/denoiser_resources.rs`
    - `denoiser_*_tex` + `denoiser_*_prev`
    - `denoiser_spatial_ping_tex`
    - `denoiser_spatial_pong_tex`
  - `src/tracer/extent_dependent_resources.rs`
    - `cloud_raw_tex`
    - `cloud_history_tex`
    - `cloud_output_tex`
- `verdarium-vkn` already provides the low-level texture/image building blocks:
  - `crates/verdarium-vkn/src/memory/texture/texture_impl.rs`
  - `crates/verdarium-vkn/src/memory/texture/desc.rs`
- `verdarium-vkn` also already owns image state/layout tracking machinery:
  - `crates/verdarium-vkn/src/resource_state_tracker.rs`
- `crates/verdarium-vkn/src/resource.rs` currently provides generic `Resource<T>` ownership wrappers but no pair/triple role wrapper.

Constraints:

- Keep changes small and focused.
- Do not implement the wrapper yet; this document is planning/progress only.
- Avoid pushing feature-specific render semantics into `verdarium-vkn`.
- Preserve explicit texture usage and existing validation workflow.
- Run `cargo check` after Rust changes once implementation starts.
- Final rendering validation should use a hidden release run and latest-log inspection.

Assumptions to confirm:

- A low-level generic wrapper such as `PingPong<T>` or `CurrentPrev<T>` is acceptable in `verdarium-vkn`.
- Higher-level semantics such as temporal reset policy, blur iteration policy, and pass scheduling should stay in tracer/render code.
- Not every repeated texture bundle should be forced into the same abstraction; triples may need a separate feature-level wrapper.

Current branch:

- `opti`

## Plan / Phases

### Phase 1: Inventory existing role patterns

- Objective: classify current texture sets into reusable mechanical patterns.
- Expected output: list of concrete usages grouped into ping-pong pair, temporal pair, and triple/bundle patterns.
- Dependencies/blockers: none.
- Status: done

### Phase 2: Decide abstraction boundary

- Objective: decide what belongs in `verdarium-vkn` versus tracer/render feature code.
- Expected output: short design decision covering allowed responsibilities for the wrapper.
- Dependencies/blockers: Phase 1 inventory.
- Status: done

### Phase 3: Define minimal wrapper surface

- Objective: choose the smallest API that improves ergonomics without hiding resource state or pass logic.
- Expected output: proposed types and operations, likely in the shape of:
  - `PingPong<T>` with role accessors
  - `CurrentPrevious<T>` with `current()` / `previous()`
  - optional feature-specific bundles for triples
- Dependencies/blockers: Phase 2 decision.
- Status: done

### Phase 4: Choose first adoption targets

- Objective: identify the lowest-risk places to apply the abstraction first.
- Expected output: prioritized rollout list, likely starting with denoiser spatial ping-pong and/or VSM ping-pong resources.
- Dependencies/blockers: Phase 3 API choice.
- Status: done

### Phase 5: Implement and validate

- Objective: introduce the chosen wrapper(s), migrate the first target(s), and verify no behavior change.
- Expected output: small focused code change plus validation results.
- Dependencies/blockers: Phases 2-4.
- Status: done

## Verification Method

Design-phase verification:

- Review that the proposed wrapper only manages ownership/role semantics.
- Confirm it does not hide image layout/state transitions already handled by `verdarium-vkn`.
- Confirm call sites become clearer rather than more abstract.

Implementation-phase verification once coding starts:

```bash
cargo fmt --check
cargo check
cargo test
cargo run --release -- --hidden --mute --auto-exit 0.5
cargo run --release -- --tail-latest-log 200
```

Acceptance criteria:

- First migrated call sites are simpler and still explicit about which texture is read or written.
- No Vulkan/layout/resource binding errors appear in the hidden release run log.
- No accidental change in pass ordering or history reset behavior.
- Resize/recreate paths remain correct for wrapped textures.
- The abstraction does not make triple/bundle cases harder to understand.

If verification is not yet possible:

- Full runtime verification is not yet possible because no implementation has started.
- The main missing piece is a concrete wrapper API and first migration target.

## Progress Log

- 2026-06-08: Discussed repeated ping-pong/history texture management in the renderer and whether a wrapper should exist in `verdarium-vkn`.
- 2026-06-08: Decision direction: a wrapper likely makes sense, but it should stay small and mechanical rather than encode blur or temporal algorithms.
- 2026-06-08: Identified three distinct patterns in current code: ping-pong scratch pairs, current/prev temporal pairs, and raw/history/output triples.
- 2026-06-08: Preliminary conclusion: use a small family of wrappers or feature-specific bundles instead of one universal abstraction.
- 2026-06-08: Created this progress document under `docs/` to match existing project planning/progress conventions.
- 2026-06-08: Chose the abstraction boundary: add small generic role wrappers to `verdarium-vkn`, while keeping algorithm-specific grouping in tracer-side resource structs.
- 2026-06-08: Implemented first low-level wrappers in `crates/verdarium-vkn/src/resource.rs`: `PingPong<T>` and `CurrentPrevious<T>`.
- 2026-06-08: Applied the first migration slice to denoiser temporal histories, denoiser spatial ping-pong access, and shadow/VSM/cloud-shadow/leaf-shadow history copy helpers without changing shader descriptor names.
- 2026-06-08: Verified the implementation with `cargo check`, `cargo run --release -- --hidden --mute --auto-exit 0.5`, and `cargo run --release -- --tail-latest-log 80`; run completed successfully with only the pre-existing hidden-monitor and butterfly-atlas warnings.

## Open Questions / Risks

- Should the generic wrapper live directly in `crates/verdarium-vkn` or in a thin tracer-side GPU utility layer?
- Should temporal pairs use distinct naming/types from ping-pong pairs even if the mechanics are similar?
- Are triples better handled by a dedicated feature bundle instead of a generic container type?
- Could a wrapper accidentally obscure descriptor-binding field names or existing resource-container expectations?
- Could over-abstraction make debugging harder when a specific pass binds the wrong texture?
- Should creation/recreation helpers be part of the wrapper, or should the wrapper only hold already-created resources?
- Long-term, do we also want transient scratch pooling, or is that a separate problem that should stay out of the first iteration?
