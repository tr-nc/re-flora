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

Each `check_ddgi*.sh` runner defines the same normalized `analyze_current_capture` function: its
body directly executes the current-only entry and every analysis branch invokes that function in
command position. Dry-run and production execution share those call sites; the wrapper alone turns
execution into command emission for dry-run. Transport's narrow `execute_analysis` helper changes
only the output sink (`cat` or `/usr/bin/env tee "$json"`) around its single analyzer invocation.
Absolute `/usr/bin/env` owns external-tool resolution, preventing Bash functions from shadowing the
production sink while retaining PATH lookup.
`scripts/rfirr_production_runner_contract.py` is intentionally only a
source-wiring tripwire for that controlled form; it does not claim to interpret arbitrary shell.
The typed current-only CLI behavior above is the schema seal.
The tripwire owns an exact per-runner, per-function invocation inventory for all seven production
runners and rejects missing canonical call sites, unexpected helper scopes, or later function
overrides. Its deliberately narrow structural parser also rejects canonical analysis execution
inside any single-line, backslash-continued, or multiline `if`/`elif`/`else` chain whose assembled
condition names `dry_run`; `else` inherits ownership from the whole chain. A raw identifier
inventory rejects analyzer mentions that the controlled parser cannot classify as the sealed
wrapper or a canonical command-position invocation, including calls adjacent to redirection. This
closes maintained source-form bypasses without claiming general Bash reachability. Each runner's
`--dry-run` is the executable normal-entry contract: it emits every current-analyzer command that
the corresponding production matrix would execute, and the behavioral tests pin those per-runner
command counts and representative branch arguments. Transport additionally seals the dry-run
`cat` sink, production `/usr/bin/env tee "$json"` sink, and exactly one two-stage analyzer-to-sink
pipeline, with an executable function-shadow test for the production sink. A whole-tree manifest
proves no filesystem side effects. Each runner owns exactly one readonly canonical `repo_root` and
makes `dry_run` readonly immediately after its only false/true argument-policy assignments. The
controlled stateful lexical pass tracks comments and quotes across lines. Its control stream masks
ordinary double-quoted text and command-substitution child-shell bodies, so neither can inject an
outer `fi`; separate code/active streams retain child commands and real expansions for authority
auditing. Braced expansions are recursively inventoried by exact base identifier, including nested
fallbacks. It inventories actual simple, compound, arithmetic, parameter, and loop-variable
assignment, unset, readonly, and expansion facts while excluding `[[...]]` comparisons. Its logical command/argv policy explicitly
rejects `eval`, `source`/`.`, shell `-c`, authority or dynamic targets passed to `printf -v`,
`read`, `readarray`/`mapfile`, `getopts`, or `let`, and every declaration nameref. Literal writes to
other variables, comparisons, comments, and quoted prose remain allowed. This is a fail-closed
allowance for the maintained grammar, not a proof of arbitrary Bash. `/usr/bin/env` is the external-tool resolution owner for canonical Cargo
build/run, decision-related `tee`, and normalization Python. Bare/shadowable forms and direct app
launches are rejected; fail-fast PATH, repository absolute-path, and external absolute-path
sentinels dynamically cover those known entrypoints. This does not prove arbitrary
variable-encoded Bash execution.

Three ownership seams were compared. Extending a regex or custom shell lexer was rejected because
it cannot establish the executed argv. Parsing a complete shell AST was rejected because dynamic
evaluation still prevents a general static proof and would add a large dependency surface. The
selected seam is the production-only current-schema entry above: all analysis behavior remains in
the deep analyzer module, while the production interface makes schema selection unrepresentable.
For dry-run parity specifically, line-local regex expansion was rejected because it cannot see an
outer conditional; centralizing every capture/file policy behind one helper was rejected because
it would widen that helper beyond analysis execution. The selected narrow conditional-stack seam
tracks only the repository's controlled function, pending condition headers, and
`if`/`elif`/`else` structure. For launch guarding, PATH-only substitution was rejected because
absolute paths bypass it; general process tracing was rejected as platform-coupled and much wider
than this CPU contract. The selected source-token inventory plus known-entrypoint sentinels keeps
the interface narrow while covering both maintained path forms. Readonly policy/root authority and
the absolute `/usr/bin/env` owner close mutable-name and function-shadow seams without exposing new
runner parameters.

The shader-validation workflow contract likewise uses a fail-closed path-filter subset. It
supports literals, `*`, and `**` (including zero-directory `**/`) with ordered `!` exclusions; any
other glob special form rejects the workflow contract rather than approximating GitHub semantics.

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

The E1 production-debug extension was calibrated from 48 release-hidden captures on the local
validation GPU: sealed, portal, and walls at spacing 32 and 16, with two Final captures plus
Moment, Exact, Unoccluded, Equal Weight, and Raw Cage diagnostics per tuple. The walls Final pair
had environment-irradiance error P99 `0` in the final matrix; an earlier pair had a maximum tail
error `2.89e-6`. The stability gate therefore requires exact world XYZ and terrain hit masks and
bounds both environment P99 and maximum error at `1e-5`, while intentionally excluding the known
temporal direct-light and shadow planes from this DDGI-specific comparison. On walls, measured
pairwise P99 differences were at least `0.0355` for Equal Weight versus Unoccluded and `0.214` for
Raw Cage versus Equal Weight; the committed route-distinction floor is conservatively `0.01`.

The runner's 60-second auto-exit is a local readiness budget, increased after spacing-16 captures
occasionally missed the former 24-second limit. It has not been established as portable across GPU
classes; cross-GPU CI or calibration must treat exhaustion as a readiness-risk signal, not silently
weaken the correctness thresholds.

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

For the real `sequential-reopened` local-recovery captures, the analyzer derives the required Q16
retention independently from the configured Q16 identity and captured update epoch using
`min(configured, epoch / (epoch + 1))`. The runner does not carry an `e1` or `e8` retention
constant: with the default configured retention, e1 derives to `32768`, while e8 derives to
`58254`; a lower configured value caps the witness.

For v10, the grid product must equal the complete epoch probe count. Visibility samples must form
whole 64-ray probes, cover every Blend probe, and cannot exceed the Blend+Replace fresh-probe
partition. These relations are checked independently by the Rust producer and Python analyzer.

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
