# DDGI staging publication owner mismatch

## Reproduction and diagnosis

On `agent/tree-edit-stall` at `2f6d9874`, running
`cargo run --release -- --hidden --mute --raster-tree-smoke --auto-exit 30`
reproduced `DDGI staging publication lost its owner radiance tuple` twice.

The point-light fixture uses the normal local-light registry. It changes transport lighting
while a terrain replacement is being built. The failing preflight had:

- complete staging field: geometry revision 2, radiance revision 1;
- resident sky revision: 1 (not overwritten);
- builder radiance revision: 2;
- scheduled work: `RadianceUpdate`, consuming the completed revision-1 field.

`Tracer::update_buffers` promotes ready staging at its beginning. Later in that same call,
completion readback publishes a complete staging field, then
`start_next_ddgi_scheduled_work` claims another epoch. Before the next frame can promote
staging, the new epoch latches revision 2 into its builder. The revision-1 publication
therefore no longer has its required owner tuple. This is a runtime lifecycle bug, not
an invalid smoke operation; normal lighting changes overlapping terrain work can reach
this path too.

## Fix and test decision

Keep the full smoke, including light insertion/removal. Keep the publication consistency
check. `DdgiRuntime::claim_transport_work` now stops claiming while a completed staging
publication awaits physical promotion or obsolete-candidate retirement. Both existing
transitions clear that pending publication; later work can then proceed normally.
Active-volume convergence does not use this barrier. Publication error diagnostics now
include the builder, resident, sky and scheduled-work identities.

The deterministic unit regression exercises replacement completion with and without a
pending lighting change. It failed before the guard and passes afterwards. This is the
minimal scheduling interleaving; GPU resources and tree operations are not necessary
to expose the premature claim.

## Validation

- `cargo fmt --check`, `cargo check`, `git diff --check`: passed.
- `cargo test`: 1172 main tests passed, 4 ignored; 4 library tests passed.
  The first command was interrupted by a 200-second tool timeout; the full rerun completed.
- Original release hidden/muted raster-tree smoke: passed, including point-light insertion
  and removal, all tree lifecycle checks, and lighting revisions advancing to 3.
- Release raster-tree smoke with `--resize-lifecycle-test`: passed.
- Release default `--hidden --mute --auto-exit 0.5`: passed.
- Per-worktree `--tail-latest-log 200` inspected; final runs have no ERROR/panic/VUID and
  shutdown reports `failures=0`. This is not a Vulkan validation-layer claim.
- No smoke, shader, generated-file or saved-config changes.

Local diagnostic command output is in `/tmp/ddgi-diagnosis/`. Successful application logs:
`target/re-flora-logs/re-flora-20260926-194914.590-457864.log` (smoke + resize), and the
original smoke log identified by `/tmp/ddgi-diagnosis/fixed-original.log`.

The lifecycle barrier belongs to the runtime, rather than relying on every caller to
remember the gap between logical field completion and physical consumer publication.
