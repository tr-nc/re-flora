# Recorded tree-leaf wind mixing

Research date: 2026-10-24. Question: is one randomly selected gentle/strong loop pair,
with independently randomized offsets per source and a live wind crossfade, conventional?

## Finding

**Yes: parameter-driven layering, randomized variations and randomized loop start offsets
are established game-audio techniques.** This is an appropriate implementation pattern,
not a claim that every game uses the same formula or that these particular recordings have
passed listening review. No middleware dependency is needed to apply it in PetalSonic.

### Evidence and retrieval limits

1. Audiokinetic's **Blend Container Property Editor** describes simultaneous child playback
   and blend tracks/crossfades driven by Game Parameters. Its **Creating Random Containers**
   documentation describes randomized selection and continuous/looping playback.
   These establish the standard container/parameter vocabulary. The official pages were
   available through indexed search excerpts; direct retrieval returned HTTP 403, so no
   full-page review or exact quotation from their bodies is claimed.
   - <https://www.audiokinetic.com/en/public-library/2024.1.7_8863/?id=blend_container_property_editor&source=Help>
   - <https://www.audiokinetic.com/en/public-library/2024.1.4_8780/?id=creating_random_container&source=Help>
2. FMOD support's Joseph explicitly recommends randomizing **Start Offset** to prevent
   phasing when several instances start together. This is directly applicable to multiple
   trees using a small recording bank. Full first-party support discussion retrieved.
   - <https://qa.fmod.com/t/phasing-issues-automatic-weapon-sound/12796/4>
3. Joseph explains that re-triggering a synchronous instrument rerandomizes its offset,
   whereas asynchronous looping preserves continuity. Thus choose the offset at source
   creation, **not on every wind change, frame or loop boundary**. Full discussion retrieved.
   - <https://qa.fmod.com/t/set-random-start-offset-once/16235/2>
4. FMOD's official instrument documentation describes Multi Instruments and randomized
   playlist selection. Indexed official documentation was available; direct HTML retrieval
   returned the JavaScript application shell, not the document body. This is corroboration,
   not a claimed full-text review.
   - <https://www.fmod.com/docs/2.04/studio/working-with-instruments.html>
5. FMOD's equal-power automation discussion confirms the distinction between the intended
   crossfade and the tool's curve authoring. In April 2024 Joseph said that Studio did not
   automatically construct that curve; this is **not** evidence that equal-power layering
   is unavailable or that every mixer uses it. Full discussion retrieved.
   - <https://qa.fmod.com/t/how-to-create-an-equal-power-automation-curve/21467/2>

Additional practitioner evidence, not vendor API specification: FMOD community discussion
of asynchronous loop overlap and equal-power fades:
<https://qa.fmod.com/t/multi-instrument-crossfade-or-equivalent/16610>.

## Project design derived from the evidence

- **One pair per actual source:** the current game has one distributed acoustic source per
  canopy generation, not one independent source for every visual leaf. Select one of four
  gentle takes and one of four strong takes independently for that source. Repeats across
  sources are allowed; forced global shuffling is not required for independent selection.
- **Two independent uniform phase draws:** maintain their relative offset during playback.
  Both layers continue advancing when their gain is zero. No repeated seeks or rerolls.
  Returning from A to B reuses the same selection and a time-advanced phase within a session.
- **Use the existing local wind owner:** weighted canopy leaf samples and the saved response
  curve/attack/release produce response `r`. Use it for overall activity and the layer mix,
  rather than inventing an independent wind oscillator inside the audio system.
- **Equal-power mix:** `gentle = sqrt(1-r)`, `strong = sqrt(r)`; their squared gains sum to one.
  This preserves expected power for similarly leveled, uncorrelated material. It does not
  guarantee constant perceived loudness or bounded sums for correlated recordings. A 20 ms
  sample-domain control ramp avoids instantaneous gain changes; existing wind inertia remains.
- **Prepare, do not destroy, originals:** source mono RMS measurements range approximately
  −50.76 to −40.39 dBFS, so raw variants differ by over 10 dB. Reproducible runtime derivatives
  use mono downmix, DC removal, 100 ms tail/head overlap and −30 dBFS RMS matching. They run
  at original speed; overlap shortens each loop from 8.0 to 7.9 seconds. This is engineering
  level matching, not an assertion of perceptual normalization.
- **One acoustic Voice:** blend PCM before spatialization and environmental acoustics;
  do not duplicate canopy geometry, rays or permanent spatial emitters for the two layers.
- **Recorded-only (2026-10-27 follow-up):** the original saved A/B comparison and
  procedural ambient path have been removed at the player's request. Recorded layers
  retain the existing Tree leaves mixer and Tree Wind Volume controls. Short start/stop
  ramps protect random-phase source starts and generation transitions. No pitch
  randomization is added. The research and original asset preparation remain unchanged.

The exact gain calibration and two-layer formula above are project design decisions, not
requirements imposed by Wwise or FMOD. Additional per-take tuning, longer assets or subtle
rate variation could be considered after listening, not assumed necessary in advance.

## Boundaries

Automated signal/lifecycle/native-scene validation is documented in
[recorded-tree-leaf-audio.md](../reports/recorded-tree-leaf-audio.md). It does not substitute
for audible loop, texture, spatial balance or physical-device approval. All eight originals
retain their recorded ElevenLabs Free-plan restrictions; derivatives do not gain commercial
or standalone redistribution permission. The user authorized this local development use;
no publication or release is authorized by this implementation.
