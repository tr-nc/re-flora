# DDGI Temporal Transport Acceptance

`scripts/check_ddgi_transport_acceptance.sh` is the top-level hidden release-mode acceptance runner.
It owns the current temporal transport matrix, then invokes portal/walls correctness, runtime
terrain-edit continuity, radiance/density lifecycle, and committed sky-normalization checks. A
missing subordinate checker is a failure.

## Evidence format

Runs write beneath `target/ddgi-transport-acceptance/<run-id>/`:

- `.rfirr` capture v8 contains pre-albedo environment irradiance, world position plus exact sun
  visibility, the raster terrain's independent direct-light RGB, and marcher receiver-center XYZ
  plus terrain VSM transmittance, followed by terrain/leaf/cloud/combined direct-shadow
  transmittance;
- `.analysis.json` records lifecycle identity, ROI measurements, finiteness, and atlas deltas;
- `.console.log` records scheduling, source/destination slots, per-epoch retention, full-atlas
  validation, atomic publication, and terminal sleep reason;
- `convergence-calibration.json` contains every validated convergence curve.

Old capture versions remain readable for committed historical evidence, but all current-runtime
acceptance requires v8 `Converging` / `Converged` metadata and `update_epoch`.

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

The runner's 60-second auto-exit is a local readiness budget, increased after spacing-16 captures
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
- the latest geometry epoch zero publishes before the density retry;
- spacing 16 first becomes visible only as a complete epoch-zero field.

## Terrain-edit continuity

`scripts/check_ddgi_runtime_terrain_edits.sh` requires the last complete Active field to remain
finite, nonnegative, and available while Staging progresses. It also requires one physical Staging
update, latest-geometry promotion, shared terrain/Flora identity, and no partially filtered atlas
publication. Exact direct sun remains a separate capture plane and is not accepted as evidence that
indirect continuity worked.

## Response latency and static sleep

On the NVIDIA GeForce RTX 3060 Ti, three matched release runs of
`terrain-edits-closed` produced six complete edit-to-epoch-zero promotions in `31-36 ms`, with
median `34.5 ms` and p95 `36 ms`. The retained two-stage baseline log contains `87 ms` and `88 ms`
for the same two edits, median `87.5 ms`; the observed first-valid-field latency is therefore about
`60.6%` lower. The old baseline has only two observations, so this is a response-latency result,
not a broad frame-performance claim. Atomic descriptor/resource publication itself remained
`0.0095 ms` median. Current evidence is under `target/ddgi-temporal-lifecycle-final/`; the baseline
is under `target/ddgi-temporal-lifecycle-baseline/`.

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
does not claim capture-v8 reference correctness and cannot weaken the current analyzer's v8-only
reference contract.

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
