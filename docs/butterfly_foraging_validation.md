# Butterfly flower visiting — implementation and validation

## Step 1: authoritative flower contacts

- Flower contact points are ray-tested against the same transformed authored head triangles used by native/cached rendering, not roots or stem socket planes.
- A bounded (512 heads) GPU pass calls the production `flowerPlantPose`, including growth, spawn, height variation and vegetation response. Identity is `(world/mode epoch, authored response_id, head)`, independent of draw order.
- Readback uses existing acquired-frame fences, with no additional query submission/wait. Per-slot resources and metadata remain paired; older generations are discarded. World replacement invalidates the publication.
- This commit supplies infrastructure only; behavior and the saved A/B control follow in the next validated step. No saved config or model asset was changed.

Validation in `new-bird`:

- `cargo fmt --check`, `cargo check`, `cargo build --release`: passed.
- `cargo test every_authored_flower_has_a_real_upward_contact`: 1 passed, all authored flowers at 0.25/1/4 head scales.
- `cargo test abi_and_epochs`: 1 passed.
- Release hidden muted smoke (`--auto-exit 0.5`): passed.
- Native flower fixture: `RUST_LOG=re_flora=info,re_flora_vkn=warn RE_FLORA_FLOWER_CONTACT_REVIEW=1 RE_FLORA_FLOWER_MODEL_REVIEW=a cargo run --release -- --hidden --mute --auto-exit 3` (with GPU lock and Wayland unset): 6 valid real flower contacts, 288-byte publication, `frame_fence=true query_wait=false authored_surface=true`; shutdown `failures=0`, no ERROR/panic/VUID.
- An initial zero-size Vulkan copy caused by argument order was caught by the native log inspection and corrected before commit; the successful validation above is the corrected run.
- Generated tracked files: unchanged. Interim unused APIs will be consumed by the behavior step. These runs establish resource/contact correctness, not visual or performance acceptance.
