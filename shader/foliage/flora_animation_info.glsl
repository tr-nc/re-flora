#ifndef FLORA_ANIMATION_INFO_GLSL
#define FLORA_ANIMATION_INFO_GLSL

// Shared by surface flora and tree foliage pipelines so every consumer sees one authoritative
// buffer layout. Spawn motion itself is implemented in flora_spawn_animation.glsl.
layout(set = 0, binding = 6) uniform U_FloraGrowthInfo {
    uint flora_tick;
    uint sprout_delay_ticks;
    uint full_growth_ticks;
    uint spawn_time_ms;
    float spawn_duration_seconds;
    float spawn_rise_fraction;
    float spawn_overshoot_min_voxels;
    float spawn_overshoot_max_voxels;
    float spawn_stagger_seconds;
    float spawn_depth_padding_voxels;
}
flora_growth_info;

#endif // FLORA_ANIMATION_INFO_GLSL
