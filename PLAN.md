# Tree Audio & Wind Roadmap

## Current State

- Tree audio sources now flow through `TreeAudioManager`, which stores `{uuid, tree_id, position, cluster_size}` for every looping emitter.
- Spatial playback still handled by `SpatialSoundManager` (PetalSonic), but higher-level logic (wind modulation, clustering heuristics) can act on the cached metadata without engine round-trips.

## Near-Term Tasks

1. **Global clustering pass**  
   - Instead of clustering per tree, collect all leaf centroids (or existing per-tree clusters) and merge them across tree boundaries using a spatial hash.  
   - Keep metadata so we can later split clusters if trees move or get culled.  
   - Experiment with listener-distance weighting: close to the player, use a smaller merge radius; far away, increase it to keep source counts manageable.

2. **Distance-aware reclustering**  
   - Given cached positions, recompute `desired_cluster_distance` as `base_dist * lerp(near_factor, far_factor, distance_to_listener / falloff)`.  
   - Consider supplementary heuristics: angle between listener forward vector and `(source_pos - listener_pos)`, distance buckets, or even terrain occlusion tests.  
   - Evaluate whether dynamically morphing cluster distances produces noticeable popping; if so, introduce crossfades when merging/splitting emitters.

3. **Wind modulation system**  
   - Treat each emitter’s `(x, z)` as its deterministic “seed”. Sample a 3D noise function `noise(time * freq, pos.x * scale, pos.z * scale)` to obtain smooth gust envelopes that sweep coherently across space.  
   - Map noise output to ±N dB gain adjustments and optionally drive subtle positional offsets (`update_source_pos`) to simulate branches swaying.  
   - Allow global wind parameters (direction, speed, strength) to influence noise phase and bias (e.g., sources downwind respond slightly earlier).

4. **Manager → PetalSonic utilities**  
   - Add helper APIs to `SpatialSoundManager` for bulk gain/pose updates so the wind system can submit batched adjustments without excessive locking.  
   - Investigate exposing “query pose” hooks inside PetalSonic so the manager can double-check engine state if needed (otherwise keep relying on cached positions).

5. **Future integration**  
   - If the clustering + wind workflow proves general, upstream a `TreeAmbienceSystem` module into `petalsonic` so other projects can reuse it.  
   - Document tuning knobs (cluster radius, noise speed/amplitude, distance falloff) for designers.

## Open Questions

- Is clustering distance alone the right abstraction, or should we move toward perceptual metrics that include listener orientation and binaural cues?  
- How aggressive can cross-tree clustering be before the spatial image feels too smeared? Need usability tests with various tree densities.  
- Should wind also influence other ambient layers (grass, shrub rustle) via the same noise field for coherence?
