# Leaf sound after a tree kick

## Finding and evidence

The useful model is **a short excitation followed by a decaying, stochastic leaf-contact texture**.
Calling the whole sound a measured "impulse response" would overstate what we have measured.

1. Komatsu et al., *Physical Characteristics of Wind-induced Foliage Sounds of Planting Trees*
   (2000), measured six tree species. Their publisher's abstract reports stable components from
   **100–1,000 Hz**, high-frequency attenuation with distance, and a correlation between wind
   velocity and foliage level. This supports a warm/body layer plus higher-frequency friction;
   it is **wind-driven foliage evidence, not a measurement of someone kicking a tree**.
   [Publisher abstract / DOI](https://www.jstage.jst.go.jp/article/souonseigyo1977/24/4/24_4_268/_article/-char/en),
   DOI `10.11372/souonseigyo1977.24.268`.
2. Perry Cook's *Physically Informed Sonic Modeling (PhISM): Percussive Synthesis* (ICMC 1996)
   describes PhISEM as pseudo-random overlap/add of small grains governed by physical rules.
   That is a useful synthesis precedent for many small contacts, **not a validated tree-kick model**.
   [Primary conference paper](https://quod.lib.umich.edu/i/icmc/bbp2372.1996.071/1/--physically-informed-sonic-modeling-phism-percussive?page=root%3Bsize%3D100%3Bview%3Dtext),
   [author's Princeton research page](https://soundlab.cs.princeton.edu/research/phymod/).
   The paper's indexed full-text passage was retrieved; a separate direct fetch returned HTTP 403.
3. CMU's *Introduction to Computer Music*, §9.4, defines an impulse response as a filter's output
   to a unit impulse and explains convolution/room reverberation for an LTI system.
   A leaf-contact cascade and nonlinear/stochastic friction are not automatically one universal
   LTI impulse response. Dry source synthesis and the garden's acoustic propagation stay separate.
   [Primary course text](https://www.cs.cmu.edu/~15322/book/ch09/04.html).

No primary source located here supplies an actual kick-to-leaf waveform, an authoritative kick
spectrum, or a duration for this authored tree. Those parts below are explicitly **sound-design
choices requiring listening**, not empirical constants. No external recording/code was copied.

## Integrated candidate

- An accepted, scene-checked K event calls the same tree-owned shake/leaf/fruit path and plays
  exactly one finite leaf rustle when that tree has a canopy sample. Stale/rejected presses and
  OS key repeat cannot play it.
- Reuse `TreeRustleVoice`'s existing warm leaf body, filtered friction/contact sheen and random
  micro-grains. Reduce the air bed; branch creaks are disabled. No extra wood thump/foot sound.
- Apply a 9 ms attack, exponential energy decay, mild squared-cosine energy modulation at the
  saved shake frequency (contacts on both directions), and a 40 ms smooth end fade. This is a
  phenomenological excited-source envelope, not a measured tree or room IR.
- Four deterministic variants are prepared into immutable mono 48 kHz clips at startup or when
  duration/timbre/frequency changes. K only chooses a resident clip; no realtime DSP, file I/O or
  new GPU job/wait. Peak is bounded to 0.35 before the saved volume and normal mixer routing.
- Source placement is the nearest real wood-clear sample in the exact tree's current canopy
  descriptor, not a centroid inside wood. Normal Leaves bus, world distance/panning, transmission
  and environmental acoustics remain enabled. Wind thresholds/ambient volume do not gate it.
- At most eight event Voices overlap. Expired sources retire on wall time (also after shorter
  live tuning), oldest excess sources retire immediately, and shutdown clears them before audio
  stops. Clip replacement does not mutate already-playing immutable clips. The maximum four-clip
  PCM bank is about **2.3 MB** at three seconds, separate from existing ambience.
- Clip preparation runs synchronously on the main thread **only on tuning changes**; it is not
  free or part of the audio callback. Release log timings are recorded below, not inferred from
  debug/unit test speed. No sustained kick/hover performance acceptance is claimed.

## Saved controls

Debug → **Tree Kick**, normal Save; older files add declarations in memory without implicit writes.
Previously saved impulse/frequency/damping/reach/cooldown values are unchanged.

| Control | Default | Range | Use |
|---|---:|---:|---|
| Leaf rustle volume | −6 dB | −40…6 | Event loudness, independent of ambient wind volume |
| Leaf rustle tail | 1.1 s | 0.25…3 | Settle duration |
| Leaf rustle brightness | 0.42 | 0…1 | Warm versus bright friction/contact texture |
| Leaf contact detail | 0.18 | 0…1 | Softer/wet-ish versus more small crackly contacts |

Shake speed also controls the gentle rebound modulation. Voice parameters/volume are captured when
that event plays; changes affect the next kick, not a rewrite of an already-playing clip.

## Validation

Pure tests check fast attack/decay, exact silent boundaries, finite bounded samples, repeatability,
variant changes and invalid settings. Config migration, unified Save and Debug single ownership
cover the added controls. An explicitly ignored audio-runtime integration test checks unknown-tree
rejection, wind-independent admission, the eight-Voice cap and retirement without putting device
checks into ordinary tests. Hidden muted Release smoke checks that one canonical kick creates one
spatial finite source while tree wind is off, alongside the GPU shake and partial fruit/leaf burst.

Release evidence: `/tmp/tree-kick-rustle-smoke-01.out` records preparation of four default
1.1-second variants in **9.562 ms**, one finite source at a tree canopy point with tree wind off,
**24** leaves and a **5/17** fruit release, the 320-byte frame-fenced fruit handoff, existing GPU
picking/lighting/exact rollback and successful shutdown (`failures=0`). Normal pure synthesis tests:
**4 passed, 1 ignored benchmark**; envelope/validation **2 passed**; the explicit audio-runtime test
**1 passed**; migration, generic saved values and single Debug ownership pass. Final repeat
`/tmp/tree-kick-rustle-smoke-final.out` passed the same gates (preparation **9.638 ms**);
`target/re-flora-logs/re-flora-20261007-223751.387-2263977.log` is the inspected successful short
hidden muted Release launch. Formatting/check and the final Release build pass. No full-suite claim.

Listening and actual OS K/prompt review remain manual. Muted smoke cannot prove that the selected
texture sounds natural or that its gain is right for this garden.
