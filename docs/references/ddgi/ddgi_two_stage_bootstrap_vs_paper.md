# Re: Flora two-stage DDGI bootstrap versus Majercik et al. 2019

Research date: 2026-08-16

## Short answer

Re: Flora performs `S0 SeedSky -> S1 SingleBounce` before replacing the active DDGI field because
its staging volume deliberately starts without a source field from the old geometry revision. S0
builds a complete, revision-matched sky irradiance and visibility source; S1 can then read that
immutable source for every probe, independent of batch order, and publish one complete
single-bounce field atomically.

This is a Re: Flora cold-start and geometry-rebuild policy. **Majercik et al. 2019 does not require
two complete updates before display and does not define S0/S1 stages.** Its normal algorithm updates
the displayed probe field continuously from current direct lighting plus previous-frame probe data,
then temporally blends the result with history. Higher bounces emerge recursively over later frames.

## Paper facts

The original paper describes one recurring frame update: trace rays from active probes, shade hit
surfels with direct and indirect illumination, then blend the new radiance and distance results into
the probe atlases ([Majercik et al. 2019, section 4, PDF pages 8-10][paper-pdf]). The surfel shader
reads lighting and probe data from the **previous frame**. The authors give two reasons: amortizing
multiple indirect bounces over frames and smoothing temporal discontinuities. Their reference
results conservatively update every probe every frame, but this is a scheduling choice, not a
two-update publication barrier.

The atlas update uses hysteresis to blend old and new irradiance; the paper reports values from
0.85 to 0.98 (section 4.4, PDF page 10). Section 5.3 (PDF page 15) explicitly describes multiple
bounces as recursion across frames, seeded by the previous bounce. It says collecting several
bounces before display is a possible adaptation for fixed-view use cases, which confirms that
waiting before display is optional rather than the default algorithm.

Therefore the paper requires a prior field/history for its ordinary recursive update, but it does
not prescribe how an engine must initialize a brand-new field or atomically replace a field after a
topology revision. It does not specify an exact two-pass `SeedSky -> SingleBounce` bootstrap.

The official [RTXGI integration guide][rtxgi-integration] follows the same recurring model: at a
front-face probe-ray hit, evaluate direct lighting, sample nearby probe irradiance recursively, and
store the combined radiance.

## Current Re: Flora implementation facts

- Geometry and density bootstrap work creates two identities together: source-free `SeedSky`
  iteration 0 and `SingleBounce` iteration 1 whose source is that S0 field
  ([`src/ddgi/scheduler.rs` lines 415-443][scheduler-bootstrap]).
- A bootstrap staging volume is required to have no published resident field; feedback work, by
  contrast, reads an already published resident source
  ([`src/ddgi/resources.rs` lines 99-155][resident-work]). This prevents a new geometry revision
  from silently inheriting irradiance history owned by the previous geometry revision.
- During S0, ray misses store authored sky radiance while front-face hits store no radiance because
  hit shading is enabled only from iteration 1 onward
  ([`ddgi_probe_trace.slang` lines 157-180][probe-trace]). S0 is also the only iteration that writes
  the geometry-owned visibility atlas
  ([`src/ddgi/resources.rs` lines 253-255][visibility-owner]).
- During S1 and later iterations, a front-face hit stores material albedo multiplied by current
  direct-sun irradiance plus irradiance sampled from the immutable source atlas
  ([`ddgi_probe_trace.slang` lines 89-124][hit-radiance]).
- Completing S0 makes it the private source for S1 but does not publish it. Only a validated,
  complete S1 becomes the staging volume's published field
  ([`src/ddgi/resources.rs` lines 1368-1404][bootstrap-publication]). The consumer contract likewise
  states that the first new field allowed to publish after a geometry rebuild is complete S1
  ([`docs/ddgi_indirect_transport_spec.md` lines 199-211][transport-spec]).
- After S1 publication, Re: Flora schedules `Feedback` iterations from the previous complete
  published field, matching the paper's recursive idea, with explicit convergence classification
  ([`src/ddgi/scheduler.rs` lines 446-485][feedback-scheduler]).

## Why two full probe passes here?

S1 needs a complete irradiance field in order to shade terrain hits indirectly. Re: Flora refuses to
use the old Active field as that source during a geometry bootstrap, because its irradiance and
visibility belong to a different geometry revision. S0 supplies a clean source for the new revision.

The source must be complete and immutable before S1 starts. Re: Flora updates the atlas in batches
across several render frames; reading from an atlas while earlier S1 batches overwrite it would make
later probes see different bounce depths and would make the result depend on batch order. The
separate S0 atlas avoids that feedback hazard. Consequently both S0 and S1 trace all probes, although
only S0 runs the visibility-filter update.

The tradeoff is intentional: the extra full trace/filter pass buys a deterministic, revision-local,
atomically publishable first field. It is not needed for every ordinary update. Once S1 is active,
each later feedback iteration is one full probe pass that reads the previous complete field and may
publish the next complete field.

## Conclusion

The measured roughly `40 ms` S0 and roughly `89.5 ms` end-to-end S1 promotion are not evidence that
the paper mandates two updates. They reflect Re: Flora's stricter replacement boundary: about one
full pass to manufacture a clean source, a second full pass to produce the first publishable bounce,
plus validation and promotion overhead. Removing S0 would require choosing a different semantic
contract--for example, accepting old-geometry history, publishing a direct-only first field, or
defining another safe initialization source--rather than simply deleting redundant work.

## Primary sources

- Zander Majercik et al., [*Dynamic Diffuse Global Illumination with Ray-Traced Irradiance
  Fields*][paper-page], JCGT 8(2), 2019. See sections 4, 4.2-4.4, and 5.3.
- NVIDIA, [RTXGI DDGI Integration Guide: Tracing Probe Rays][rtxgi-integration].

[paper-page]: https://jcgt.org/published/0008/02/01/
[paper-pdf]: https://jcgt.org/published/0008/02/01/paper-lowres.pdf
[rtxgi-integration]: https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/docs/Integration.md#tracing-probe-rays-for-a-ddgivolume
[scheduler-bootstrap]: ../../../src/ddgi/scheduler.rs#L415-L443
[resident-work]: ../../../src/ddgi/resources.rs#L99-L155
[probe-trace]: ../../../shader/slang/ddgi_probe_trace.slang#L157-L180
[visibility-owner]: ../../../src/ddgi/resources.rs#L253-L255
[hit-radiance]: ../../../shader/slang/ddgi_probe_trace.slang#L89-L124
[bootstrap-publication]: ../../../src/ddgi/resources.rs#L1368-L1404
[transport-spec]: ../../ddgi_indirect_transport_spec.md#L199-L211
[feedback-scheduler]: ../../../src/ddgi/scheduler.rs#L446-L485
