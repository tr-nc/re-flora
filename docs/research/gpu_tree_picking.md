# GPU tree picking without a per-frame CPU forest copy

## Delivered behavior

Normal frames retain GPU branch solving, skinning/normal transformation, BVH refit, wind, leaf
motion, shadows, DDGI, attachments and the accepted immutable no-wind physical collider. They no
longer skin the dense forest on the CPU. Small branch-pose readback and palettes remain, because
fruit/attachments/audio still consume them. CPU deformation is an explicit diagnostic oracle.

Pointer picking uses the same nearest terrain/tree traversal as rendering queries. The GPU returns
48 bytes: world hit/distance, barycentric rest position, integer primitive identity and material.
This preserves original wood coordinates even for blended skin bindings and moved upper branches.
Canonical edits remain CPU-owned; physical/ground queries remain static. Fixed roof model support
and occlusion remain CPU-owned and are merged using the captured ray.

### Asynchronous ownership

- At most one pointer edit is pending in the app. Held tools do not accumulate a per-frame backlog.
  A render batch contains at most two rays: captured edit plus latest-wins hover.
- Input/header/readback storage is per acquired frame slot. Header is a uniform buffer; ray input
  is storage. Only 48 bytes per answer cross back. Buffer allocation is fallible.
- Named descriptor overrides fork the complete reflected active generation. The flight retains
  that immutable descriptor generation until its frame fence completes; no hardcoded scene-binding
  list or descriptor rewrite underneath in-flight work is used.
- Fence status polling does not wait. Acquiring a reused frame slot collects its old readback after
  the existing render fence wait, before overwriting that slot. Edits are processed at the next
  pre-acquire input seam, never after resetting an unsubmitted frame fence.
- Scene stamps pair requests/results with terrain revision, successful tree publication generation,
  raster enablement and GPU wind mode. Stale requests are rejected before dispatch and stale
  answers before edits. Failed preparation keeps the resident generation. Pose motion is not a
  reason to discard every completed answer; each answer belongs to its queried rendered pose.
- The click ray is frozen. Mouse motion cannot cause a second query for its rest-space coordinate.
  Initial press intent survives release; subsequent held dabs require the original active action.
  Selection/radius changes, pointer loss, focus/cursor loss and camera-mode changes cancel intent.
  The app discards pending edits older than 250 ms. Hover accepts the newest completed ticket,
  rather than requiring the newest requested ticket (which could otherwise starve).
- Explicit synchronous GPU queries remain for hidden validation and the startup roof fixture only.
  Normal hover, initial clicks, held dabs and discrete placement never use that blocking API.

**Interaction limitation:** Answers arrive after GPU/frame completion, usually one or two frames
with the current frame configuration. Manual click/drag/hover feel and rapid repeated discrete
clicks have not been reviewed. One pending edit is a bounded admission policy, not an unlimited
click queue. This is not a zero-latency or identical-input-timing claim.

## Controlled Release evidence

Same worktree, i5-12600KF / RTX 3060 Ti, saved size **20**, fixed 12-tree garden fixture, seeds
122–133, hidden native window, muted output, automatic FIFO selection, 12-second final hold and
0.5-second settling exclusion. All successful reports have GPU validation at every count,
zero errors/cliffs and `contended=False`. Settings restored to GUI SHA-256
`450a82ed59ed7ab249dc905610d3d738be55e020bcec99ce1352ee83379a6231`.

Order was new / preserved old / final new, two runs each:

| Report under `target/` | Run 1 frame median | Run 2 frame median |
|---|---:|---:|
| `tree-scaling-gpu-picking-after-01` | 17.086 ms | 17.052 ms |
| `tree-scaling-gpu-picking-reverse-01` (old) | 20.254 ms | 19.780 ms |
| `tree-scaling-gpu-picking-after-02` (final) | 16.983 ms | 17.067 ms |

The reverse old is approximately **49–51 FPS**; final new approximately **59 FPS**. Final new frame
time is roughly **14–16% lower**, not the entire 4.2 ms CPU duration as additive frame savings.
The earlier static-collision report measured 18.351/18.577 ms on the same old executable; this
variation remains disclosed, rather than promising a universal gain.

Matched settled-frame scopes, milliseconds:

| Scope | Reverse old, runs 1 / 2 | Final new, runs 1 / 2 |
|---|---:|---:|
| CPU dense skinning | 4.226 / 4.170 | <0.001 / <0.001 |
| CPU total tree preparation | 4.778 / 4.664 | 0.345 / 0.326 |
| CPU palette publication | 0.127 / 0.123 | 0.121 / 0.121 |
| CPU pose readback | 0.059 / 0.056 | 0.061 / 0.060 |
| Physical tree publication | <0.001 / <0.001 | <0.001 / <0.001 |
| GPU whole frame | 12.612 / 12.377 | 12.846 / 12.228 |
| GPU tree surface update | 0.551 / 0.552 | 0.554 / 0.553 |

GPU execution stays materially similar. Earlier new runs measured 11.489/11.605 ms GPU whole
frame, illustrating variance. CPU command recording remains about 2 ms. Removing serial CPU
preparation exposes more acquisition/FIFO waiting; nested/overlapping scopes are not summed.

Geometry remains **514088 vertices / 340444 triangles**, 170378 canopy leaves, and **176 fruit
bodies** with ordinary fixed-step physics. The tree publication remains 122355728 logical bytes,
with hot immutable inputs device-local. Size, geometry, wind, shading quality, shadows and DDGI were
not reduced. The scaling fixture suppresses ambient emitters and normally has no pointer tool
selected: its FPS results measure removal of unconditional CPU work, **not** sustained hover/edit
GPU cost or decorative-leaf live cost. Full CPU reference work at count validation is diagnostic,
not steady-frame skinning.

Executable SHA-256:
- Preserved old `target/tree-scaling-before-gpu-picking`:
  `a416f8bfb47de0011d6215d6d5f2f08b98bc7ef9a11ca96cd354a4c15c1fa09d`.
- Final measured Release:
  `9e7efb18a0bd164b24899c5e769273f6d191872663356d9336c34e96c3210430`.

The report `commit` records checkout HEAD, not necessarily the binary's source revision. The first
new report was measured before the CPU-removal commit; the reverse uses the preserved old binary
while checkout HEAD had advanced. Hashes and explicit binary paths govern this comparison.

### Excluded evidence

`tree-scaling-gpu-picking-before-01` is invalid: the untouched old executable failed at four trees
with its existing CPU/GPU merged query disagreement (world positions differed by approximately
0.0062). It is not used for timing or blamed on the new code. The completed reverse report passes.
The new diagnostic compares actual GPU tree hits against an explicit full triangle CPU oracle,
not the intentionally static ground-query helper, and also checks ordinary/mapped GPU scene-query
world/validity agreement.

Intermediate async smoke runs failed on incomplete transient overrides, missing uniform-buffer
usage, then premature descriptor-set retirement. These were repaired by reflected complete
off-side generations, uniform usage and flight-owned descriptor residency. The run that printed
successful hit metadata but Vulkan invalid-command-buffer errors is **not valid evidence**. Final
smokes have no such validation errors; the raw artifacts remain under `/tmp/tree-pick-*`.

## Validation

- Formatting/check and Release builds passed; generated/config files did not change.
- Targeted tests: picker **3**, initial-click/hold policy **1**, player-tool **14**, panel-input **3**,
  raster-tree **15**, app-physics **33**, vegetation **42**, response **9**, VKN **58**.
  No full repository-suite success is claimed.
- Hidden raster smoke: GPU skin/refit, preserved normal/confidence/lighting, wind off/on, **17**
  independent posed rays, GPU rest-coordinate removal of **2** wood cells, age rebuild,
  replacement/removal/Conifer/restoration and exact atlas/canonical/resident rollback passed.
- Real frame-owned asynchronous dispatch/readback passes against its same-frame CPU tree oracle:
  `[SURFACE_PICK][ASYNC] passed world=true rest=true bytes=48 frame_fence=true query_wait=false`.
- Production leaf-response bit-exact history/remapping validation passes.
- Hidden roof fixture passes fixed model/soil occlusion, resolved GPU-supported RMB placement,
  backpack preservation, floor/smooth/grow clipping, capsule behavior and voxel A/B roundtrip.
  Its explicit startup diagnostic query is not normal asynchronous input evidence.
- Short hidden muted Release launches exited successfully; logs inspected for errors.

No visible game was automatically launched. Manual async interaction feel, continuous-hover GPU
cost, rapid-click admission behavior and the accepted upper-branch physical/visual mismatch remain
for user review. Existing unrelated GUI-suite failures are not fixed or declared green here.
