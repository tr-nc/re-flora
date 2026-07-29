# Environment Lighting Test Scene

## Purpose

The environment-lighting test scene is a deterministic terrain-edit scenario for developing global
SH, local irradiance probes, probe invalidation, and light-leak rejection. It constructs the scene
at runtime through the normal voxel edit and deferred terrain-rebuild paths; it does not load a
special world or persist terrain changes.

The scenario contains two same-material comparison bays:

```text
                     camera at +Z

                 open front / portal
              +-----------------------+
 lower world X | roofed rock chamber  |
              | carved from a solid  |
              | shell, with plinth   |
              +-----------------------+

              +-----------------------+
 higher world X| open-sky rock bay    |
              | matching back wall   |
              | and matching plinth  |
              +-----------------------+

       dynamic raster tree is retained behind the gallery
```

The roofed chamber provides a ceiling underside, side walls, a deep back wall, a portal transition,
and an upward-facing comparison plinth. The open bay provides matching rock surfaces and plinth
dimensions under unobstructed sky. A startup tree is moved out of the construction bounds but kept
in the camera view so raster vegetation remains part of the lighting comparison. The scenario also
clears the normal synthetic startup obstacle before building the gallery.

The scenario owns a deterministic camera pose and fixes `time_of_day=0.455705` with automatic
day/night cycling disabled in memory. Those runtime overrides are not persisted to `config/gui.toml`.

## Hidden Validation Command

All automated validation must run hidden:

```bash
cargo run --release -- --hidden --mute --windowed \
  --environment-lighting-test-scene \
  --screenshot player-default target/environment-lighting-test-scene.png \
  --screenshot-delay 4 \
  --auto-exit 8

cargo run --release -- --hidden --tail-latest-log 200
```

`player-default` satisfies the existing screenshot interface; the scenario then replaces it with
its deterministic gallery camera. Screenshot capture waits for terrain rebuild completion and two
settling frames, even when the requested screenshot delay has already elapsed. HUD and debug panels
are suppressed while this scenario is producing a screenshot or denoiser-benchmark capture; visible
interactive runs retain the normal UI.

A successful run contains these log milestones:

```text
[ENV_LIGHT_TEST] constructing roofed and open terrain bays with voxel edits
[ENV_LIGHT_TEST] edits applied
[ENV_LIGHT_TEST] terrain rebuild complete
[ENV_LIGHT_TEST] ready
[SCREENSHOT] Capturing
```

## Review Points

The two logged sample locations identify the matching plinths:

- roofed sample: `(0.648, 0.438, 1.180)`;
- open sample: `(1.344, 0.438, 1.180)`.

With the current global SH implementation, the roofed chamber intentionally exposes the limitation
that environment irradiance has no local visibility. It may therefore remain brighter and flatter
than physically expected. This is useful baseline evidence, not a failure of the scenario.

When local probes are added, review the same scene for:

- a stable darkening gradient from the portal toward the roofed back wall;
- a darker roofed plinth than the matching open-sky plinth;
- no irradiance leaking through the roof or side walls;
- no discontinuity or bright halo at the portal;
- immediate global sky-hue changes without waiting for all probes to retrace;
- correct dirty-probe invalidation when the scenario's terrain edits are applied;
- consistent environment hue between terrain and the retained raster tree;
- stable results across repeated hidden runs at the same configuration.

Direct sun and its VSM/leaf/cloud visibility remain separate from probe irradiance. Compare probe
implementations at the same time of day and shadow configuration so a direct-shadow change is not
misidentified as an irradiance-probe improvement.
