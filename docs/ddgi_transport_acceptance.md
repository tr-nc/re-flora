# DDGI Temporal Transport Acceptance

`scripts/check_ddgi_transport_acceptance.sh` is the top-level hidden release-mode acceptance runner.
It owns the current temporal transport matrix, then invokes portal/walls correctness, runtime
terrain-edit continuity, radiance/density lifecycle, and committed sky-normalization checks. A
missing subordinate checker is a failure.

## Evidence format

Runs write beneath `target/ddgi-transport-acceptance/<run-id>/`:

- `.rfirr` capture v10 contains pre-albedo environment irradiance, world position plus exact sun
  visibility, the raster terrain's independent direct-light RGB, and marcher receiver-center XYZ
  plus terrain VSM transmittance, followed by terrain/leaf/cloud/combined direct-shadow
  transmittance. Its filter extension records exact owner-version masks, action partitions, and
  Blend retention `sum+max` witnesses from one complete GPU-produced epoch. Its independent host
  identity records the authoritative probe-grid dimensions and configured history-retention Q16;
- `.analysis.json` records lifecycle identity, ROI measurements, finiteness, and atlas deltas;
- `.console.log` records scheduling, source/destination slots, per-epoch retention, full-atlas
  validation, atomic publication, and terminal sleep reason;
- `convergence-calibration.json` contains every validated convergence curve.

RFIRR v8 reference captures and the published 252-byte/11Q v9 evidence layout remain readable for
committed historical evidence. All current-runtime acceptance requires the fixed v10 layout,
`Converging` / `Converged` metadata, `update_epoch`, authoritative grid/config identity, and
owner-generated filter evidence. Version selects the layout; byte length never selects a second
layout for the same version. Source-shape tests remain wiring guards only; they are not runtime
proof.

Production runners invoke `scripts/analyze_current_environment_irradiance_capture.py`. That entry
does not expose `--expect-version`; it binds the analyzer to `CURRENT_RFIRR_VERSION` internally, so
escaped or dynamically expanded shell arguments cannot select a historical schema. Explicit
numeric compatibility remains available only through
`scripts/analyze_environment_irradiance_capture.py` for historical fixtures and tests.

All seven `check_ddgi*.sh` files are four-line adapters into `scripts/ddgi_evidence/`. The typed
module owns two stable interfaces: `plan(RunRequest) -> ExecutionPlan` expands a closed `Suite` into
typed stages and actions, while `execute(plan, host) -> RunReport` applies that same plan through
either `SubprocessHost` or the zero-side-effect `RecordingHost`. Environment variables are decoded
once into suite-specific closed options. Arbitrary commands, callbacks, schema registries, and
numeric version selection are not representable in the plan.

`SubprocessHost` launches every process with an argv vector and `shell=False`. Capture actions bind
the process console to its canonical run log and preserve both beside the `.rfirr`; analysis actions
invoke only the current-schema entry and tee transport JSON to its committed artifact path.
`RecordingHost` observes the same actions and argv without creating a directory or file. Typed
`FactRef` values carry revisions and publication tokens from ordered scenario validation into later
analysis actions, and typed `FailureKey` values preserve case-level aggregation. `Claim` stages emit
`ACCEPTED` or `PROVEN` only after their required evidence stages succeed.

The transport plan includes correctness, runtime-recovery, and lifecycle as nested typed plans,
rather than recursively launching shell scripts. Behavioral tests pin the seven capture/analysis
inventories (`48/44`, `4/6`, `3/3`, `1/1`, `29/11`, `4/4`, and recursive transport `100/78`), exact
representative argv, exit `0/1/2`, four-line wrappers, and a whole-tree dry-run manifest. The
radiance lifecycle is one ordered typed stream: it rejects duplicate or reordered checkpoints,
field/revision drift, an obsolete r3 publication, consumer-visible in-flight mutation, and direct-
sun capture later than the first rendered frame after mutation. Process-bound console/run-log,
density, local recovery, terrain edit, stale-active, and Flora consumer validation live behind the
same validation module.

The deleted alternative was a custom 1,482-line Bash-source interpreter plus source-mutation tests.
It could prove only one maintained source shape, not the argv that production executed. A complete
shell AST was also rejected because dynamic shell evaluation would still make the proof partial and
would add a large dependency surface. The typed plan makes invalid orchestration unrepresentable,
gives production and dry-run two real adapters at one seam, and concentrates command construction,
failure policy, and evidence ordering in one module.

The shader-validation workflow contract likewise uses a fail-closed path-filter subset. It
supports literals, `*`, and `**` (including zero-directory `**/`) with ordered `!` exclusions; any
other glob special form rejects the workflow contract rather than approximating GitHub semantics.

## Resident publication ownership

The physical Volume owns one composite resident publication: its owner-issued generation root,
current field, atlas/sky slots, build token, and latched radiance revision are validated together.
There is no separate raw `published_field` authority. Atlas completion and staging promotion each
mint a linear permit only after that complete tuple and the scheduler/coordinator transition pass
preflight. Descriptor code can borrow resources only through the permit. A descriptor error drops
the permit without committing the Volume or scheduler; after descriptor success, consuming the
permit is an infallible ownership transition.

The collected runtime writes RFIRR v10, and all seven production runners invoke the v10 current-only
entry without a version-selection surface. The compatibility analyzer retains the published v9
layout for historical evidence; it cannot redefine the production schema seal.

The density runner feeds its console to one ordered parser rather than accepting independent marker
matches. The parser advances through baseline, obsolete-density preemption, private terrain e0,
terrain promotion and consumer publication, recovered same-generation observation, density retry,
density promotion and consumer publication, then final capture/summary. Duplicate or shuffled
checkpoints, mixed token lineages, and any obsolete-token promotion/consumer event fail closed.
Promotion, consumer, and capture markers carry the owner-issued generation token, epoch-zero root,
current field serial, and radiance revision. The parser binds those values to the private geometry
root and baseline radiance rather than trusting `same_generation=true`. Geometry e0 retains the
baseline field as its cross-generation history source; the retried density e0 must have no source.
Runtime e0 capture markers may precede their App checkpoint, so the parser buffers them and closes
the set only after every identity is known. It requires exactly one capture for the baseline,
terrain, and retried density generations. The preempted density generation never completes and is
forbidden from publishing a capture; its midflight and preemption markers instead prove that it
never became consumer-visible. The four generation tokens must be distinct and strictly ordered
before capture matching. Each buffered capture also retains its arrival phase: baseline capture
must immediately precede the baseline checkpoint, terrain capture must follow preemption and
immediately precede the private checkpoint, and retried-density capture must follow retry token
declaration and immediately precede promotion. Capture markers parse `target` as a field and accept
only the exact value `e0`. A private epoch-zero current field must equal its epoch-zero root.
Terrain promotion cannot regress behind the observed private-current epoch; at the same epoch it
must publish that exact private-current field.

## Transport matrix

The matrix runs spacing 32 and 16 and includes:

| Scene | Required checkpoints | Purpose |
|---|---|---|
| sealed | e0, e1, converged | no e0 energy and bounded Moment leakage through feedback |
| portal | converged plus exact-reference runner | moment-visibility leak bound |
| donor | e0 forward/reverse, converged | first-publication signal and batch-order invariance |
| dogleg | e0, e1, converged | delayed multi-segment propagation |

Epoch labels are temporal sample identities, not exact bounce-order claims. The first geometry field
contains sky misses and direct-sun terrain reflection; later epochs recursively query the previous
complete field and add new rotated angular samples.

The sealed e0 capture remains bit-exact zero. E1 and terminal captures use the same committed
`1e-5` maximum linear-luminance ceiling as the correctness matrix and its exact reference; this is
the Moment-only production-query leakage contract, not a permission to create energy. On the local
validation GPU, two bit-exact spacing-32 runs measured maxima `2.2463949e-6` at e1 and
`1.2473703e-6` at e127, while spacing 16 remained bit-exact zero at both checkpoints. The matching
exact-irradiance capture was also bit-exact zero. Keeping one threshold value across transport and
correctness avoids the contradictory prior state where the same capture passed the documented
`1e-5` contract but failed transport merely for containing a nonzero float.

The donor epoch-zero ROI luminance gate is at least `0.045`; the measured spacing-32 forward and
reverse value was `0.1289403043`, with bit-exact environment and direct-light payloads. Captures may
have different field serials because startup timing is process-local; payload equality is the
determinism criterion after compatible scene/revision/epoch metadata is established.

The dogleg e0 ROI must remain at most `0.00002`. Epoch one must gain at least `0.000035`; this is the
temporal equivalent of the old unblended `0.00007` gate because the sample-age cap deliberately
retains 50% history at e1. The measured gains were `0.00004127` at spacing 32 and `0.00005353` at
spacing 16.

Runtime terrain, Flora, and Leaves intentionally use Moment visibility only; exact segment
visibility remains a transport and diagnostic oracle. The terminal `walls` gate therefore records
the accepted Moment-only leakage ceiling rather than the older Full/Moment-times-Exact ceiling. In
the historical 64-epoch evidence under
`target/ddgi-temporal-final-correctness-converged/20260816T174539Z-229568/`, converged e63
environment payloads repeated bit-exactly and measured exact-reference luminance-error P99
`0.391190` at spacing 32 and `0.365590` at spacing 16. The guarded ceilings are `0.400` and `0.375`.
Sealed and portal retain their stricter existing bounds. Every correctness capture explicitly
targets `converged`; default e0 capture timing cannot masquerade as terminal quality.

The E1 production-debug extension was calibrated from 48 release-hidden captures on the local
validation GPU: sealed, portal, and walls at spacing 32 and 16, with two Final captures plus
Moment, Exact, Unoccluded, Equal Weight, and Raw Cage diagnostics per tuple. The walls Final pair
had environment-irradiance error P99 `0` in the final matrix; an earlier pair had a maximum tail
error `2.89e-6`. The stability gate therefore requires exact world XYZ and terrain hit masks and
bounds both environment P99 and maximum error at `1e-5`, while intentionally excluding the known
temporal direct-light and shadow planes from this DDGI-specific comparison. On walls, measured
pairwise P99 differences were at least `0.0355` for Equal Weight versus Unoccluded and `0.214` for
Raw Cage versus Equal Weight. Unoccluded versus Final measured symmetric P99 differences of
`0.216342` at spacing 32 and `0.174522` at spacing 16. The committed route-distinction floor is
conservatively `0.01`. These gates prove observably distinct production outputs, not route identity;
the retained source-owner contract covers wiring until runtime owner-tag evidence exists. In
particular, no signed global-mean ordering is required: changing visibility can change normalized
probe weights, so Unoccluded is not mathematically required to be globally brighter than Final.

The runner's 120-second auto-exit is a local readiness budget, increased after spacing-16 captures
occasionally missed the former 24-second limit. It has not been established as portable across GPU
classes; cross-GPU CI or calibration must treat exhaustion as a readiness-risk signal, not silently
weaken the correctness thresholds.

Process validation fails closed on every application, Vulkan, panic, device-loss, descriptor, and
stale-readback diagnostic. The sole platform exception is the exact `sctk_adwaita::config`
color-scheme Portal timeout: it is a cosmetic desktop-setting lookup and does not affect the native
window, Vulkan surface, or capture completion. The classifier requires the complete production log
identity and payload; a different module, Portal key, trailing diagnostic, or any other `ERROR`
remains fatal. `scripts/runtime_log_diagnostics.py` owns this classification for process-bound DDGI
evidence, sky-normalization evidence, the canonical latest-run smoke gate, and release performance
logs, so those consumers cannot disagree about the same production event.

## Convergence policy

The runner does not override or duplicate `DDGI_CONVERGENCE_POLICY`. Each production capture logs
the typed runtime policy used by its first build; the convergence summarizer derives the terminal
epoch from that process-bound record and rejects missing or cross-capture policy drift:

- absolute delta threshold `0.0025`;
- relative delta threshold `0.02` with floor `0.05`;
- minimum 8 complete epochs and two consecutive passing epochs;
- maximum 128 complete epochs (`e0` through `e127`).

Both `Threshold` and `SampleBudget` are valid terminal reasons. `SampleBudget` means the finite
quality budget completed with a finite nonnegative field; it does not mean the threshold passed.
The convergence summarizer requires independently complete and equal console/preserved-run-log
records, then filters them to the captured geometry/radiance/spacing identity. Field serials must be
unique and ordered, and the final validation and terminal field serial must match the RFIRR capture,
so startup-volume epochs or records from another field cannot contaminate the target curve.

## Lifecycle checks

`scripts/check_ddgi_lifecycle_acceptance.sh` proves at spacing 32 and 16 that:

- the old DDGI payload remains bit-exact on the first rendered frame after a radiance mutation;
- direct light responds immediately and changes the fixed sunlit ROI by at least `0.02`;
- the in-flight radiance snapshot remains immutable and queued revisions coalesce latest-wins;
- the final radiance epoch zero uses the expected prior complete source;
- a density update leaves spacing 32 active while spacing 16 builds;
- a geometry edit preempts the first density candidate without making it consumer-visible;
- the latest geometry epoch zero completes privately, the same typed generation survives local
  recovery and publishes, and only then does the density retry begin;
- spacing 16 first becomes visible only as a complete epoch-zero field.

## Terrain-edit continuity

`scripts/check_ddgi_runtime_terrain_edits.sh` requires the last complete Active field to remain
finite, nonnegative, and available while Staging progresses. It also requires one physical Staging
update, latest-geometry promotion, shared terrain/Flora identity, and no partially filtered atlas
publication. Exact direct sun remains a separate capture plane and is not accepted as evidence that
indirect continuity worked.

For the real `sequential-reopened` local-recovery captures, the analyzer derives the required Q16
retention independently from the configured Q16 identity and captured update epoch using
`min(configured, epoch / (epoch + 1))`. The runner does not carry an `e1` or `e8` retention
constant: with the default configured retention, e1 derives to `32768`, while e8 derives to
`58254`; a lower configured value caps the witness.

For v10, the grid product must equal the complete epoch probe count. Visibility samples must form
whole 64-ray probes, cover every Blend probe, and cannot exceed the Blend+Replace fresh-probe
partition. These relations are checked independently by the Rust producer and Python analyzer.

## Response latency and static sleep

On the NVIDIA GeForce RTX 3060 Ti, three historical matched release runs of
`terrain-edits-closed` produced six complete edit-to-epoch-zero promotions in `31-36 ms`, with
median `34.5 ms` and p95 `36 ms`. The retained two-stage baseline log contains `87 ms` and `88 ms`
for the same two edits, median `87.5 ms`. These samples predate localized recovery and therefore do
not measure the current edit-to-consumer-publication interval. They remain historical first-valid
field evidence, not a broad frame-performance claim. Atomic descriptor/resource publication itself
remained `0.0095 ms` median. The historical evidence is under
`target/ddgi-temporal-lifecycle-final/`; the baseline is under
`target/ddgi-temporal-lifecycle-baseline/`.

A separate five-second static portal run under that historical 64-epoch policy reached
`Converged e63` and recorded zero scheduler claims after the terminal publication. Camera/display
frames alone therefore do not keep DDGI awake;
geometry, density, or radiance revision changes are required to restart work.

## Sky normalization

`scripts/check_ddgi_sky_normalization_evidence.py` keeps the original `E/pi` presentation-parity
evidence pinned to its historical commits. Those v3-v5 stage labels are retained only because they
describe the old artifacts; they do not reintroduce a current runtime stage path. Its private
legacy-v2 comparator proves only fixed-command Final payload RGB deltas and an exact hit mask for
the two audited commits. Those captures have no world/identity planes, so this historical evidence
does not claim five-plane reference correctness and cannot weaken the compatibility analyzer's
RFIRR v8-v10 five-plane contract or the production current-only v10 seal.

## Reproduction

Run the full suite from the repository root:

```bash
scripts/check_ddgi_transport_acceptance.sh
```

For focused iteration:

```bash
scripts/check_ddgi_lifecycle_acceptance.sh
python scripts/benchmark_ddgi_publication.py --samples 1 --auto-exit 15
```
