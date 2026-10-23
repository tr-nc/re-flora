# Sprinkler audio prototype

A standalone listening experiment; no game configuration or runtime changes.

From the repository root:

```sh
python3 -m http.server 18765 --bind 127.0.0.1 --directory experiments/sprinkler-audio-prototype
```

Open http://127.0.0.1:18765 and click **开始试听**. Use HTTP rather than `file://`, which can block Web Audio media sources.

- Unchecked: continuous original loop. Checked: continuous loop with a smooth one-second gain envelope, default 3.5 dB peak-to-trough.
- Animation alternates Z/X every second, matching the game's nominal two-second cycle. This is a continuous-time visualization, not a reproduction of the game's tick quantization or particle simulation.
- A cosine oscillator on the audio clock drives gain at audio rate. Canvas reads the same clock; switching modes does not restart the media or oscillator. Pause suspends both.
- Source gain defaults to +18 dB because the supplied file peaks at approximately −27.45 dBFS. Maximum +24 dB and 100% output still leave source peak headroom. This assumes the bundled, unmodified source.
- The modulated mode attenuates below the original peak level; A/B is not average-loudness matched.
- The two-second graph shows steady-state gain; control changes have a 25 ms smoothing time constant.
- This prototype uses the WAV's native loop boundary without crossfading; audible seam quality remains a listening check.
- Settings are temporary, not persisted. No libraries, build step, or network dependencies.

## Source

`sprinkler.wav` is a byte-for-byte copy of the user's ElevenLabs download `Seamless_loop_of_a_s_#2-1792738811425.wav`: 8 seconds, stereo, 48 kHz, 16-bit PCM. It is referenced by the audio element's relative `src`, not synthesized.
