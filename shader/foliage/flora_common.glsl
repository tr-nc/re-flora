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
const float PLAYER_PUSH_RADIUS              = 0.3;
const float PLAYER_PUSH_STRENGTH            = 0.02;

float sample_standard_normal(uint seed) {
    float sum  = 0.0;
    uint state = seed ^ 0xA511E9B3u;
    for (uint i = 0u; i < 12u; ++i) {
        state = wellons_hash(state + i * 0x9E3779B9u);
        sum += construct_float_01(state);
    }
    return sum - 6.0;
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

float flora_growth_factor(uint growth_start_tick) {
    if (flora_growth_info.full_growth_ticks <= flora_growth_info.sprout_delay_ticks) {
        return 1.0;
    }
    uint age_ticks = flora_growth_info.flora_tick - growth_start_tick;
    return smoothstep(float(flora_growth_info.sprout_delay_ticks),
                      float(flora_growth_info.full_growth_ticks), float(age_ticks));
}

float get_shadow_weight(ivec3 vox_local_pos) {
    vec3 vox_dir_normalized            = normalize(vec3(vox_local_pos));
    float shadow_negative_side_dropoff = max(0.0, dot(-vox_dir_normalized, sun_info.sun_dir));
    shadow_negative_side_dropoff       = pow(shadow_negative_side_dropoff, 2.0);
    float shadow_weight                = 1.0 - shadow_negative_side_dropoff;

    shadow_weight = max(0.7, shadow_weight);
    return shadow_weight;
}

void prepare_flora_vertex(ivec3 vox_local_pos, ivec3 gradient_origin, uint max_length,
                          uvec3 instance_pos_voxels, uint in_instance_ty_seed,
                          uint in_instance_growth_start_tick, out uint instance_ty,
                          out uint instance_seed, out bool is_grass, out float color_gradient,
                          out vec3 voxel_pos, out vec3 anchor_pos, out float shadow_weight,
                          out bool should_trim_voxel) {
    instance_ty   = decode_instance_ty(in_instance_ty_seed);
    instance_seed = decode_instance_seed(in_instance_ty_seed);
    is_grass = instance_ty == FLORA_SPECIES_TALL_GRASS || instance_ty == FLORA_SPECIES_SHORT_GRASS;

    float base_gradient = compute_gradient(vox_local_pos, gradient_origin, max_length);
    color_gradient = instance_ty == FLORA_SPECIES_SHORT_GRASS
                         ? compute_gradient(vox_local_pos, gradient_origin, tall_grass_height_voxels)
                         : base_gradient;
    float wind_gradient = instance_ty == FLORA_SPECIES_SHORT_GRASS
                              ? compute_gradient(vox_local_pos, gradient_origin,
                                                 tall_grass_height_voxels)
                              : base_gradient;

    uint grass_height_voxels =
        is_grass ? sample_grass_height(instance_ty, instance_seed) : tall_grass_height_voxels;
    float growth_factor    = flora_growth_factor(in_instance_growth_start_tick);
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

    vec3 instance_pos = vec3(instance_pos_voxels) * scaling_factor;
    vec3 wind_vec     = sample_wind_volume(instance_pos, instance_seed);
    vec3 wind_offset  = wind_vec * wind_gradient * wind_gradient;
    vec2 player_delta = instance_pos.xz - camera_info.pos.xz;
    float player_dist = length(player_delta);
    vec2 player_push_dir = player_dist > 1e-4 ? player_delta / player_dist : vec2(0.0);
    float player_push_amount =
        (1.0 - smoothstep(0.0, PLAYER_PUSH_RADIUS, player_dist)) * wind_gradient * wind_gradient;
    vec3 player_push =
        vec3(player_push_dir.x, 0.0, player_push_dir.y) * (PLAYER_PUSH_STRENGTH * player_push_amount);

    anchor_pos = (vec3(vox_local_pos) + wind_offset) * scaling_factor + instance_pos + player_push;
    voxel_pos         = anchor_pos + vec3(0.5) * scaling_factor;

    shadow_weight = get_shadow_weight_vsm(shadow_camera_info.view_proj_mat, vec4(voxel_pos, 1.0));
    shadow_weight *= get_shadow_weight(vox_local_pos);
}

vec3 sample_flora_base_color(bool is_grass, uint instance_ty, uint instance_seed,
                             ivec3 vox_local_pos, uvec3 instance_pos_voxels,
                             float color_gradient) {
    if (is_grass) {
        vec3 grass_bottom_color_linear;
        vec3 grass_tip_color_linear;
        sample_grass_band_gradient(vec2(float(instance_pos_voxels.x), float(instance_pos_voxels.z)),
                                   grass_bottom_color_linear, grass_tip_color_linear);
        return mix(grass_bottom_color_linear, grass_tip_color_linear, color_gradient);
    }

    uint palette_seed        = combine_color_seed(instance_seed);
    vec3 bottom_color_linear = srgb_to_linear(pc.bottom_color);
    vec3 tip_color_linear    = sample_tip_palette(instance_ty, palette_seed, pc.tip_color);
    vec3 interpolated_color  = mix(bottom_color_linear, tip_color_linear, color_gradient);
    vec3 instance_color_variation =
        signed_unit_noise(float(instance_seed)) * gui_input.flora_instance_hsv_offset_max;
    vec3 voxel_color_variation =
        signed_unit_noise(vec4(vec3(vox_local_pos), float(instance_seed))) *
        gui_input.flora_voxel_hsv_offset_max;
    vec3 total_color_variation = instance_color_variation + voxel_color_variation;
    return apply_hsv_offset(interpolated_color, total_color_variation);
}

#endif // FLORA_COMMON_GLSL
