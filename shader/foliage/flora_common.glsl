#ifndef FLORA_COMMON_GLSL
#define FLORA_COMMON_GLSL

const float scaling_factor                  = 1.0 / 256.0;
const uint tall_grass_height_voxels         = 8u;
const uint short_grass_height_voxels        = 4u;
const uint tall_grass_min_height_voxels     = 3u;
const uint short_grass_min_height_voxels    = 2u;
const float tall_grass_height_mean_voxels   = 5.0;
const float tall_grass_height_stddev_voxels = 1.0;
const float short_grass_height_mean_voxels  = 3.0;
const float short_grass_height_stddev_voxels = 0.6;

#include "./flora_wind_motion.glsl"
#include "./flora_species_profile.glsl"
#include "./flora_spawn_animation.glsl"
#define DIRECT_SUN_SHADOW_ENABLE_LEAF
#define DIRECT_SUN_SHADOW_ENABLE_CLOUD
#include "../include/direct_sun_shadow.glsl"
#include "../include/stylized_voxel_lighting.glsl"

float sample_standard_normal(uint seed) {
    uint state_a = wellons_hash(seed ^ 0xA511E9B3u);
    uint state_b = wellons_hash(seed ^ 0x63D83595u);
    return (construct_float_01(state_a) + construct_float_01(state_b) - 1.0) * 2.4494898;
}

uint sample_grass_height(uint instance_ty, uint seed) {
    bool is_short_grass = instance_ty == FLORA_SPECIES_SHORT_GRASS;
    float mean_height =
        is_short_grass ? short_grass_height_mean_voxels : tall_grass_height_mean_voxels;
    float stddev_height =
        is_short_grass ? short_grass_height_stddev_voxels : tall_grass_height_stddev_voxels;
    float min_height =
        float(is_short_grass ? short_grass_min_height_voxels : tall_grass_min_height_voxels);
    float max_height = float(is_short_grass ? short_grass_height_voxels : tall_grass_height_voxels);

    float sampled_height = mean_height + sample_standard_normal(seed) * stddev_height;
    sampled_height       = clamp(sampled_height, min_height, max_height);
    return uint(round(sampled_height));
}

float flora_lifecycle_growth_factor(uint growth_progress) {
    return float(growth_progress) / float(INSTANCE_GROWTH_PROGRESS_MATURE);
}

vec3 apply_grass_growth_stress_tint(vec3 base_color_linear, bool is_grass,
                                    float competition_growth_factor) {
    if (!is_grass) {
        return base_color_linear;
    }
    const vec3 stressed_grass_srgb = vec3(0.62, 0.68, 0.275);
    const float max_tint_strength = 0.55;
    float stress = 1.0 - clamp(competition_growth_factor, 0.0, 1.0);
    return mix(base_color_linear, srgb_to_linear(stressed_grass_srgb),
               stress * max_tint_strength);
}

void prepare_flora_vertex(ivec3 vox_local_pos, uint voxel_info, uvec3 instance_pos_voxels,
                           uint instance_ty, uint instance_seed, uint in_instance_growth_progress,
                           float competition_growth_factor, float environment_growth_factor,
                           uint instance_spawn_start_ms,
                           out bool is_grass,
                           out float color_gradient, out vec3 voxel_pos,
                           out vec3 anchor_pos, out float shadow_weight,
                           out bool should_trim_voxel) {
    is_grass = instance_ty == FLORA_SPECIES_TALL_GRASS || instance_ty == FLORA_SPECIES_SHORT_GRASS;
    bool is_surface_flora = instance_ty < FLORA_SPECIES_COUNT;
    bool is_apple = instance_ty == FLORA_SPECIES_APPLE;

    float wind_gradient = is_apple ? 1.0 : flora_voxel_wind_gradient(voxel_info);
    color_gradient = flora_voxel_color_gradient(voxel_info);

    uint grass_height_voxels =
        is_grass ? sample_grass_height(instance_ty, instance_seed) : tall_grass_height_voxels;
    float growth_factor = min(flora_lifecycle_growth_factor(in_instance_growth_progress),
                              min(competition_growth_factor, environment_growth_factor));
    should_trim_voxel      = false;

    if (is_grass) {
        float grown_height_voxels_f = floor(float(grass_height_voxels) * growth_factor + 0.001);
        should_trim_voxel           = float(vox_local_pos.y) >= grown_height_voxels_f;
    } else {
        if (growth_factor <= 0.0) {
            should_trim_voxel = true;
        } else {
            should_trim_voxel = flora_voxel_growth_gradient(voxel_info) > growth_factor;
        }
    }

    vec3 instance_pos    = vec3(instance_pos_voxels) * scaling_factor;
    vec3 wind_sample_pos = (is_grass || is_apple) ? instance_pos :
                                                  instance_pos + vec3(vox_local_pos) * scaling_factor;
    uint wind_seed = (is_grass || is_apple) ? instance_seed :
                                             get_wind_volume_voxel_seed(instance_seed, vox_local_pos);
    vec3 wind_vec = sample_wind_volume(wind_sample_pos, wind_seed);
    float grass_rooted_height_t =
        float(vox_local_pos.y) / max(float(grass_height_voxels) - 1.0, 1.0);
    float flora_bend_height_t = is_grass ? grass_rooted_height_t : wind_gradient;
    float flora_bend_weight = flora_bend_height_factor(flora_bend_height_t);
    float wind_bend_weight = is_surface_flora ? flora_bend_weight : wind_gradient * wind_gradient;
    float species_wind_affect = is_surface_flora ? flora_species_voxel_wind_affect_multiplier(
                                                       instance_ty, vox_local_pos, wind_gradient) :
                                                   1.0;
    vec3 wind_offset = is_apple ? vec3(0.0) : wind_vec * wind_bend_weight * species_wind_affect;
    float wind_motion_time =
        wind_volume_bucket_update_time(get_wind_volume_bucket_index(wind_seed), pc.time);
    if (is_surface_flora) {
        float natural_bend_height = is_grass ? float(grass_height_voxels) :
                                               flora_voxel_lookup_max_length(instance_ty);
        float natural_bend_t = is_grass ? grass_rooted_height_t : wind_gradient;
        wind_offset += flora_natural_rest_bend(instance_seed, natural_bend_t, natural_bend_height) *
                       species_wind_affect;
        wind_offset += flora_wind_vibration(wind_vec, flora_bend_height_t, instance_seed,
                                            vox_local_pos, wind_motion_time) *
                       species_wind_affect;
    } else if (instance_ty == FLORA_SPECIES_TREE_LEAF) {
        wind_offset += leaf_wind_paddling(wind_vec, wind_gradient, instance_seed, vox_local_pos,
                                          ivec3(0), wind_motion_time);
    } else if (is_apple) {
        wind_offset += apple_wind_swing(wind_vec, instance_seed, wind_motion_time);
    }
    float plant_height_voxels = is_grass ? float(grass_height_voxels) :
                                            flora_voxel_lookup_max_length(instance_ty) + 1.0;
    FloraSpawnAnimationPose spawn_pose = is_surface_flora ? sample_flora_spawn_animation(
                                                               instance_spawn_start_ms,
                                                               instance_seed,
                                                               plant_height_voxels) :
                                                           FloraSpawnAnimationPose(
                                                               vec3(0.0), vec3(1.0), 1.0);
    anchor_pos = (vec3(vox_local_pos) + wind_offset + spawn_pose.translation_voxels) *
                     scaling_factor +
                 instance_pos;
    voxel_pos = anchor_pos + vec3(0.5) * scaling_factor;

    shadow_weight = stylized_voxel_shadow_weight(voxel_pos, vec3(vox_local_pos));
}

uint flora_height_color_row(float color_gradient) {
    float row = clamp(color_gradient, 0.0, 1.0) * float(FLORA_HEIGHT_COLOR_TABLE_LEN - 1);
    return uint(round(row));
}

vec3 unpack_linear_rgb10(uint packed_color) {
    const float inv_max_channel = 1.0 / 1023.0;
    return vec3(float(packed_color & 0x3FFu),
                float((packed_color >> 10) & 0x3FFu),
                float((packed_color >> 20) & 0x3FFu)) * inv_max_channel;
}

vec3 sample_flora_base_color(bool is_grass, uint instance_ty, uint instance_seed,
                             ivec3 vox_local_pos, uvec3 instance_pos_voxels,
                             float color_gradient, uint voxel_info) {
    uint material_id = flora_voxel_material_id(voxel_info);
    uint color_row = flora_height_color_row(color_gradient);
    uint dark_height_color_rgb10 = pc.height_dark_color_rgb10[color_row];
    if (instance_ty == FLORA_SPECIES_LAVENDER) {
        dark_height_color_rgb10 = sample_lavender_height_palette_rgb10(instance_seed, color_row);
    }
    vec3 dark_height_color = unpack_linear_rgb10(dark_height_color_rgb10);

    if (instance_ty == FLORA_SPECIES_EMBER_BLOOM &&
        material_id == FLORA_VOXEL_MATERIAL_ALLIUM_CORE) {
        const uint flower_color_row = 8u;
        vec3 flower_a = unpack_linear_rgb10(pc.height_dark_color_rgb10[flower_color_row]);
        vec3 flower_b = unpack_linear_rgb10(pc.height_light_color_rgb10[flower_color_row]);
        float preset_choice = construct_float_01(wellons_hash(instance_seed ^ 0x63D83595u));
        if (preset_choice >= 0.5) {
            // Golden seed-head preset: warm yellow through a soft floral white.
            flower_a = srgb_to_linear(vec3(0.89, 0.72, 0.29));
            flower_b = srgb_to_linear(vec3(0.98, 0.96, 0.86));
        }

        // A low-discrepancy lattice sequence gives every voxel its own blend factor.
        // Unlike unconstrained hash noise, neighboring coordinates take large,
        // well-distributed steps through [0, 1), which avoids obvious color clumps.
        float instance_offset = construct_float_01(wellons_hash(instance_seed ^ 0xA511E9B3u));
        float blend_factor = fract(dot(vec3(vox_local_pos),
                                       vec3(0.754877666, 0.569840296, 0.438289173)) +
                                   instance_offset);
        return mix(flower_a, flower_b, blend_factor);
    }

    if (is_grass) {
        float grass_band_t = sample_grass_band_interpolation_t(
            vec2(float(instance_pos_voxels.x), float(instance_pos_voxels.z)));
        vec3 light_height_color = unpack_linear_rgb10(pc.height_light_color_rgb10[color_row]);
        return mix(dark_height_color, light_height_color, grass_band_t);
    }

    if (instance_ty == FLORA_SPECIES_APPLE) {
        float speckle = signed_unit_noise(vec4(vec3(vox_local_pos), float(instance_seed))).x;
        return apply_hsv_offset(dark_height_color, vec3(speckle * 0.018, 0.03, speckle * 0.035));
    }

    vec3 instance_color_variation =
        signed_unit_noise(float(instance_seed)) * gui_input.flora_instance_hsv_offset_max;
    vec3 voxel_color_variation =
        signed_unit_noise(vec4(vec3(vox_local_pos), float(instance_seed))) *
        gui_input.flora_voxel_hsv_offset_max;
    vec3 total_color_variation = instance_color_variation + voxel_color_variation;
    return apply_hsv_offset(dark_height_color, total_color_variation);
}

#endif // FLORA_COMMON_GLSL
