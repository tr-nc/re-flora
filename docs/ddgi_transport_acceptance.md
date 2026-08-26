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
| sealed | e0, e1, converged | no created energy and no recursive leak |
| portal | converged plus exact-reference runner | moment-visibility leak bound |
| donor | e0 forward/reverse, converged | first-publication signal and batch-order invariance |
| dogleg | e0, e1, converged | delayed multi-segment propagation |

Epoch labels are temporal sample identities, not exact bounce-order claims. The first geometry field
contains sky misses and direct-sun terrain reflection; later epochs recursively query the previous
complete field and add new rotated angular samples.

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
`target/ddgi-temporal-final-correctness-converged/20260816T174539Z-229568/`, converged e63
environment payloads repeated bit-exactly and measured exact-reference luminance-error P99
`0.391190` at spacing 32 and `0.365590` at spacing 16. The guarded ceilings are `0.400` and `0.375`.
Sealed and portal retain their stricter existing bounds. Every correctness capture explicitly
targets `converged`; default e0 capture timing cannot masquerade as terminal quality.

## Convergence policy

The runner does not override `DDGI_CONVERGENCE_POLICY`:

- absolute delta threshold `0.0025`;
- relative delta threshold `0.02` with floor `0.05`;
- minimum 8 complete epochs and two consecutive passing epochs;
- maximum 128 complete epochs (`e0` through `e127`).

Both `Threshold` and `SampleBudget` are valid terminal reasons. `SampleBudget` means the finite
quality budget completed with a finite nonnegative field; it does not mean the threshold passed.
The convergence summarizer filters records to the captured geometry/radiance/spacing identity so
startup-volume epochs cannot contaminate the target curve.

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

A separate five-second static portal run reached `Converged e63` and recorded zero scheduler claims
after the terminal publication. Camera/display frames alone therefore do not keep DDGI awake;
geometry, density, or radiance revision changes are required to restart work.

## Sky normalization

`scripts/check_ddgi_sky_normalization_evidence.py` keeps the original `E/pi` presentation-parity
evidence pinned to its historical commits. Those v3-v5 stage labels are retained only because they
describe the old artifacts; they do not reintroduce a current runtime stage path.

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
