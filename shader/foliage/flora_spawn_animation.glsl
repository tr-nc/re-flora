#ifndef FLORA_SPAWN_ANIMATION_GLSL
#define FLORA_SPAWN_ANIMATION_GLSL

// The renderer consumes a pose rather than embedding a particular entrance curve in the
// vertex path. New motion programs can add scale or visibility behavior here without changing
// flora placement, instance generation, or species-specific code.
struct FloraSpawnAnimationPose {
    vec3 translation_voxels;
    vec3 scale;
    float visibility;
};

float flora_spawn_ease_out_cubic(float t) {
    float inverse = 1.0 - clamp(t, 0.0, 1.0);
    return 1.0 - inverse * inverse * inverse;
}

float flora_spawn_smoothstep(float t) {
    t = clamp(t, 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

FloraSpawnAnimationPose sample_flora_spawn_animation(uint spawn_start_ms, uint instance_seed,
                                                      float plant_height_voxels) {
    FloraSpawnAnimationPose pose;
    pose.translation_voxels = vec3(0.0);
    pose.scale = vec3(1.0);
    pose.visibility = 1.0;

    if (spawn_start_ms == INSTANCE_SPAWN_INACTIVE) {
        return pose;
    }

    float duration = max(flora_growth_info.spawn_duration_seconds, 0.001);
    float stagger = construct_float_01(wellons_hash(instance_seed ^ 0xD1B54A35u)) *
                    max(flora_growth_info.spawn_stagger_seconds, 0.0);
    float age_seconds = float(instance_spawn_age_ms(
                            spawn_start_ms, flora_growth_info.spawn_time_ms)) * 0.001 - stagger;
    float timeline = clamp(age_seconds / duration, 0.0, 1.0);
    float rise_end = clamp(flora_growth_info.spawn_rise_fraction, 0.05, 0.95);
    float overshoot_min = min(flora_growth_info.spawn_overshoot_min_voxels,
                              flora_growth_info.spawn_overshoot_max_voxels);
    float overshoot_max = max(flora_growth_info.spawn_overshoot_min_voxels,
                              flora_growth_info.spawn_overshoot_max_voxels);
    float overshoot = clamp(plant_height_voxels * 0.08, overshoot_min, overshoot_max);
    float underground = -(plant_height_voxels +
                          max(flora_growth_info.spawn_depth_padding_voxels, 0.0));

    float vertical_offset;
    if (timeline < rise_end) {
        float rise_t = flora_spawn_ease_out_cubic(timeline / rise_end);
        vertical_offset = mix(underground, overshoot, rise_t);
    } else {
        float settle_t = flora_spawn_smoothstep((timeline - rise_end) / (1.0 - rise_end));
        vertical_offset = mix(overshoot, 0.0, settle_t);
    }

    pose.translation_voxels.y = vertical_offset;
    return pose;
}

#endif // FLORA_SPAWN_ANIMATION_GLSL
