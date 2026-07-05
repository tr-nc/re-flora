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

float flora_growth_factor(uint growth_progress) {
    // TODO: advance non-mature growth progress over time instead of treating it as static state.
    return float(growth_progress) / float(INSTANCE_GROWTH_PROGRESS_MATURE);
}

float get_shadow_weight(ivec3 vox_local_pos) {
    vec3 vox_dir_normalized            = normalize(vec3(vox_local_pos));
    float shadow_negative_side_dropoff = max(0.0, dot(-vox_dir_normalized, sun_info.sun_dir));
    shadow_negative_side_dropoff       = pow(shadow_negative_side_dropoff, 2.0);
    float shadow_weight                = 1.0 - shadow_negative_side_dropoff;

    shadow_weight = max(0.7, shadow_weight);
    return shadow_weight;
}

void prepaverdarium_vertex(ivec3 vox_local_pos, ivec3 gradient_origin, uint max_length,
                           uvec3 instance_pos_voxels, uint instance_ty, uint instance_seed,
                           uint in_instance_growth_progress, out bool is_grass,
                           out float color_gradient, out vec3 voxel_pos, out vec3 anchor_pos,
                           out float shadow_weight, out bool should_trim_voxel) {
    is_grass = instance_ty == FLORA_SPECIES_TALL_GRASS || instance_ty == FLORA_SPECIES_SHORT_GRASS;
    bool is_surface_flora = instance_ty < FLORA_SPECIES_COUNT;
    bool is_apple = instance_ty == FLORA_SPECIES_APPLE;

    bool is_short_grass = instance_ty == FLORA_SPECIES_SHORT_GRASS;
    uint gradient_length = is_short_grass ? tall_grass_height_voxels : max_length;
    float wind_gradient = is_apple ? 1.0 : compute_gradient(vox_local_pos, gradient_origin, gradient_length);
    color_gradient = is_apple
                         ? clamp((float(vox_local_pos.y) + float(max_length)) /
                                     max(1.0, float(max_length) * 2.0),
                                 0.0, 1.0)
                         : wind_gradient;

    uint grass_height_voxels =
        is_grass ? sample_grass_height(instance_ty, instance_seed) : tall_grass_height_voxels;
    float growth_factor    = flora_growth_factor(in_instance_growth_progress);
    should_trim_voxel      = false;

    if (is_grass) {
        float grown_height_voxels_f = floor(float(grass_height_voxels) * growth_factor + 0.001);
        should_trim_voxel           = float(vox_local_pos.y) >= grown_height_voxels_f;
    } else {
        if (growth_factor <= 0.0) {
            should_trim_voxel = true;
        } else {
            float grown_length = float(max_length) * growth_factor;
            float voxel_length = length(vec3(vox_local_pos - gradient_origin));
            should_trim_voxel  = voxel_length > grown_length;
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
        float natural_bend_height = is_grass ? float(grass_height_voxels) : max(float(max_length), 1.0);
        float natural_bend_t = is_grass ? grass_rooted_height_t : wind_gradient;
        wind_offset += flora_natural_rest_bend(instance_seed, natural_bend_t, natural_bend_height) *
                       species_wind_affect;
        wind_offset += flora_wind_vibration(wind_vec, flora_bend_height_t, instance_seed,
                                            vox_local_pos, wind_motion_time) *
                       species_wind_affect;
    } else if (instance_ty == FLORA_SPECIES_TREE_LEAF) {
        wind_offset += leaf_wind_paddling(wind_vec, wind_gradient, instance_seed, vox_local_pos,
                                          gradient_origin, wind_motion_time);
    } else if (is_apple) {
        wind_offset += apple_wind_swing(wind_vec, instance_seed, wind_motion_time);
    }
    anchor_pos = (vec3(vox_local_pos) + wind_offset) * scaling_factor + instance_pos;
    voxel_pos         = anchor_pos + vec3(0.5) * scaling_factor;

    shadow_weight = get_shadow_weight_vsm_temporal(vec4(voxel_pos, 1.0));
    shadow_weight *= get_leaf_shadow_transmittance(vec4(voxel_pos, 1.0), true, true);
    shadow_weight *= get_cloud_shadow_transmittance(vec4(voxel_pos, 1.0));
    shadow_weight *= get_shadow_weight(vox_local_pos);
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

const int TOMATO_FRUIT_CENTER_COUNT = 9;
const ivec3 TOMATO_FRUIT_CENTERS[TOMATO_FRUIT_CENTER_COUNT] = ivec3[](
    ivec3(-5, 7, -1), ivec3(-2, 10, 2), ivec3(4, 8, 1), ivec3(7, 11, 2),
    ivec3(-7, 14, 2), ivec3(3, 14, -3), ivec3(6, 16, -1), ivec3(-3, 18, 1),
    ivec3(1, 19, 3));

bool tomato_fruit_sample(ivec3 vox_local_pos, out vec3 fruit_rel, out float fruit_metric) {
    const float fruit_radius_xz = 2.15;
    const float fruit_radius_y  = 2.05;

    for (int i = 0; i < TOMATO_FRUIT_CENTER_COUNT; ++i) {
        fruit_rel = vec3(vox_local_pos - TOMATO_FRUIT_CENTERS[i]);
        vec3 p = vec3(fruit_rel.x / fruit_radius_xz, fruit_rel.y / fruit_radius_y,
                      fruit_rel.z / fruit_radius_xz);
        fruit_metric = dot(p, p);
        if (fruit_metric <= 1.0) {
            return true;
        }
    }

    fruit_rel    = vec3(0.0);
    fruit_metric = 1.0;
    return false;
}

vec3 sample_tomato_base_color(ivec3 vox_local_pos) {
    vec3 fruit_rel;
    float fruit_metric;
    if (tomato_fruit_sample(vox_local_pos, fruit_rel, fruit_metric)) {
        float top_t = clamp((fruit_rel.y + 2.05) / 4.10, 0.0, 1.0);
        vec3 lower_red_srgb = vec3(174.0, 34.0, 11.0) / 255.0;
        vec3 ripe_red_srgb  = vec3(236.0, 66.0, 22.0) / 255.0;
        vec3 warm_top_srgb  = vec3(255.0, 105.0, 36.0) / 255.0;
        vec3 color_srgb     = mix(lower_red_srgb, ripe_red_srgb, 0.55 + top_t * 0.25);
        color_srgb          = mix(color_srgb, warm_top_srgb, smoothstep(0.55, 1.0, top_t) * 0.25);

        vec3 fruit_normal = normalize(fruit_rel + vec3(0.001));
        float highlight = smoothstep(0.72, 0.98,
                                     dot(fruit_normal, normalize(vec3(-0.55, 0.62, -0.35))));
        highlight *= 1.0 - smoothstep(0.22, 0.92, fruit_metric);
        color_srgb = mix(color_srgb, vec3(1.0, 0.62, 0.26), highlight * 0.30);
        return srgb_to_linear(color_srgb);
    }

    float height_t = clamp(float(vox_local_pos.y) / 23.0, 0.0, 1.0);
    float outer_leaf_t = smoothstep(1.2, 7.5, length(vec2(vox_local_pos.x, vox_local_pos.z)));
    vec3 stem_srgb = mix(vec3(45.0, 111.0, 42.0), vec3(78.0, 148.0, 55.0), height_t) / 255.0;
    vec3 leaf_srgb = mix(vec3(32.0, 104.0, 43.0), vec3(119.0, 191.0, 45.0), height_t) / 255.0;
    vec3 color_srgb = mix(stem_srgb, leaf_srgb, outer_leaf_t);

    // A fixed, authored vein-like modulation gives the leaf clusters detail while keeping every
    // tomato plant identical.
    float vein = 0.035 * cos(float(vox_local_pos.x * 2 + vox_local_pos.z * 3 - vox_local_pos.y));
    color_srgb = clamp(color_srgb + vec3(vein), vec3(0.0), vec3(1.0));
    return srgb_to_linear(color_srgb);
}

vec3 sample_flora_base_color(bool is_grass, uint instance_ty, uint instance_seed,
                             ivec3 vox_local_pos, uvec3 instance_pos_voxels,
                             float color_gradient) {
    if (instance_ty == FLORA_SPECIES_TOMATO) {
        return sample_tomato_base_color(vox_local_pos);
    }

    uint color_row = flora_height_color_row(color_gradient);
    uint dark_height_color_rgb10 = pc.height_dark_color_rgb10[color_row];
    if (instance_ty == FLORA_SPECIES_LAVENDER) {
        dark_height_color_rgb10 = sample_lavender_height_palette_rgb10(instance_seed, color_row);
    }
    vec3 dark_height_color = unpack_linear_rgb10(dark_height_color_rgb10);

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
