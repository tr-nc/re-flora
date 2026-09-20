# v2 five-pose rhythm before modeling

The following is authored art direction informed by the reference, not a recovered biomechanical trajectory. The canvas, world reference and camera remain fixed. Body motion is intentional and will not be recentered away.

| t / frame | Readable action | Wing hinge | Body z | Whole-body pitch | Abdomen relative pitch | Abdomen-tip follow |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| 0.0 / 1 | recovery, wings rising; body finishing downward response | +28° | -0.05 | -10° | +12° | +8° |
| 0.2 / 6 | high-wing anticipation, briefly held | +46° | -0.09 | -2° | +15° | +12° |
| 0.4 / 11 | decisive downstroke crossing open wings; chest lifts and pitches up | +4° | +0.11 | +25° | -18° | -10° |
| 0.6 / 16 | low-wing follow-through; body response begins settling | -38° | +0.08 | +8° | -20° | -16° |
| 0.8 / 21 | slower recovery, body pitches down, abdomen catches up | -8° | -0.08 | -17° | +8° | +14° |
| 1.0 / 26 | exact return to first pose | +28° | -0.05 | -10° | +12° | +8° |

This asymmetric pose progression replaces v1's pure symmetric sine. A smooth periodic cubic through these five authored states supplies the editable 25 fps animation. Key samples remain the same five columns as v1 and the current asset; the continuous source is not used to hide coarse five-frame playback.

Initial modeling choice: globally fixed 45-degree elevation and 2.55 orthographic scale; larger, fuller paired wings and a compact 2-pixel-scale body. The first measured key poses reached a 1.3024-world-unit half extent and clipped at 2.55 scale. The single global scale was therefore increased to 2.95 for the entire set, retaining roughly one target-pixel padding. No per-camera/per-frame adaptive fitting.

Reference facts incorporated: current runtime views are rows 1 and 3; their silhouette changes include front-half lifting/opening, a narrow diagonal transition, and later downward extension. Silhouette centroids are explicitly not anatomical anchor tracks. Body lift/pitch and abdomen lag implement the user's requested new stylization.
