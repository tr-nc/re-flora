#ifndef FLORA_WIND_MOTION_GLSL
#define FLORA_WIND_MOTION_GLSL

const float FLORA_TWO_PI = 6.28318530718;
const float FLORA_NATURAL_BEND_MAX_ANGLE = 1.45;

float flora_wind_planar_strength(vec3 wind_vec) {
    return smoothstep(0.03, 2.0, length(wind_vec.xz));
}

float flora_smootherstep(float t) {
    t = clamp(t, 0.0, 1.0);
    return t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
}

float flora_bend_height_factor(float height_fraction) {
    float power = max(gui_input.flora_bend_height_power, 0.05);
    return pow(clamp(height_fraction, 0.0, 1.0), power);
}

float flora_wind_curve_response(float wind_strength, float start_strength, float full_strength,
                                float knee_bias) {
    float lo = min(start_strength, full_strength);
    float hi = max(start_strength, full_strength);
    float range = hi - lo;
    float t = range <= 1e-4 ? (wind_strength >= hi ? 1.0 : 0.0) :
                               clamp((wind_strength - lo) / range, 0.0, 1.0);
    float exponent = exp2(clamp(knee_bias, -6.0, 6.0));
    return flora_smootherstep(pow(t, exponent));
}

vec2 flora_wind_planar_dir(vec3 wind_vec) {
    vec2 planar = wind_vec.xz;
    float len = length(planar);
    return len > 1e-4 ? planar / len : vec2(1.0, 0.0);
}

float flora_wind_phase(uint instance_seed, ivec3 vox_local_pos, uint salt) {
    uint seed = instance_seed ^ salt;
    seed ^= uint(vox_local_pos.x) * 0x85EBCA6Bu;
    seed ^= uint(vox_local_pos.y) * 0xC2B2AE35u;
    seed ^= uint(vox_local_pos.z) * 0x27D4EB2Fu;
    return construct_float_01(wellons_hash(seed)) * FLORA_TWO_PI;
}

vec3 flora_natural_rest_bend(uint instance_seed, float height_fraction, float flora_height_voxels) {
    // Even in calm air, blades are not perfectly vertical: weight, growth curvature,
    // and clump variation give each blade a stable rest lean in a random direction.
    // Move AABB voxel centers onto a circular-arc centerline instead of applying a
    // pure horizontal shear, so the perceived blade length stays much closer to the
    // unbent blade length while each voxel remains axis-aligned.
    float direction_angle =
        construct_float_01(wellons_hash(instance_seed ^ 0xD1B54A32u)) * FLORA_TWO_PI;
    vec3 bend_dir = vec3(cos(direction_angle), 0.0, sin(direction_angle));

    float amount_jitter = construct_float_01(wellons_hash(instance_seed ^ 0x94D049BBu));
    float bend_min = max(gui_input.grass_natural_bend_min_voxels, 0.0);
    float bend_max = max(gui_input.grass_natural_bend_max_voxels, bend_min);
    float tip_bend_voxels = mix(bend_min, bend_max, amount_jitter);

    float blade_height = max(flora_height_voxels, 1.0);
    float t = clamp(height_fraction, 0.0, 1.0);
    float bend_t = flora_bend_height_factor(t);
    float bend_center_y = bend_t * blade_height;

    float bend_angle = clamp(2.0 * tip_bend_voxels / blade_height, 0.0,
                             FLORA_NATURAL_BEND_MAX_ANGLE);
    if (bend_angle <= 1e-4) {
        return vec3(0.0);
    }

    float radius = blade_height / bend_angle;
    float arc_angle = bend_angle * bend_t;
    float horizontal_offset = radius * (1.0 - cos(arc_angle));
    float bent_y = radius * sin(arc_angle);
    float vertical_offset = bent_y - bend_center_y;

    return bend_dir * horizontal_offset + vec3(0.0, vertical_offset, 0.0);
}

vec3 flora_wind_vibration(vec3 wind_vec, float wind_gradient, uint instance_seed,
                          ivec3 vox_local_pos, float time) {
    float strength = flora_wind_planar_strength(wind_vec);
    if (strength <= 0.0) {
        return vec3(0.0);
    }

    vec2 wind_dir = flora_wind_planar_dir(wind_vec);
    vec3 cross_wind_dir = vec3(-wind_dir.y, 0.0, wind_dir.x);
    float tip_weight = flora_bend_height_factor(wind_gradient);
    float phase = flora_wind_phase(instance_seed, vox_local_pos, 0xB5297A4Du);
    float height_phase = float(vox_local_pos.y) * 1.35;
    float vibration = sin(time * gui_input.grass_vibration_primary_speed + phase + height_phase);
    vibration += 0.45 * sin(time * gui_input.grass_vibration_secondary_speed + phase * 1.37 - height_phase * 0.6);

    return cross_wind_dir * (vibration * gui_input.grass_vibration_amplitude_voxels * strength * tip_weight);
}

vec3 kochia_branch_jelly_motion(vec3 wind_vec, float branch_height_t, uint instance_seed,
                                uint animation_group, float time) {
    float strength = flora_wind_planar_strength(wind_vec);
    float branch_weight = flora_smootherstep(branch_height_t);
    if (strength <= 0.0 || branch_weight <= 0.0) {
        return vec3(0.0);
    }

    vec2 wind_dir = flora_wind_planar_dir(wind_vec);
    vec3 along_wind_dir = vec3(wind_dir.x, 0.0, wind_dir.y);
    vec3 cross_wind_dir = vec3(-wind_dir.y, 0.0, wind_dir.x);
    uint group_seed = wellons_hash(instance_seed ^ (animation_group * 0x9E3779B9u));
    float instance_phase = construct_float_01(wellons_hash(instance_seed ^ 0xA511E9B3u));
    float group_phase = fract(instance_phase + float(animation_group) * 0.61803398875 *
                                                   gui_input.kochia_branch_phase_spread) *
                        FLORA_TWO_PI;
    float direction_jitter = construct_float_01(group_seed) * 2.0 - 1.0;
    vec3 branch_direction = normalize(along_wind_dir + cross_wind_dir * direction_jitter * 0.7);
    float speed_jitter = mix(0.85, 1.15, construct_float_01(wellons_hash(group_seed ^ 0xB5297A4Du)));
    float jelly_speed = max(gui_input.kochia_branch_jelly_speed, 0.0) * speed_jitter;
    float jelly = sin(time * jelly_speed + group_phase);
    jelly += 0.35 * sin(time * jelly_speed * 1.67 + group_phase * 1.31);
    jelly *= 1.0 / 1.35;

    vec3 offset = branch_direction *
                  (jelly * gui_input.kochia_branch_jelly_amplitude_voxels * strength *
                   branch_weight);

    float flutter_phase = group_phase * 1.73 + float(animation_group) * 0.41;
    float flutter = sin(time * max(gui_input.kochia_tip_flutter_speed, 0.0) + flutter_phase);
    float tip_weight = branch_weight * branch_weight * branch_weight;
    offset += cross_wind_dir *
              (flutter * gui_input.kochia_tip_flutter_amplitude_voxels * strength * tip_weight);
    return offset;
}

vec3 apple_wind_swing(vec3 wind_vec, uint instance_seed, float time) {
    float strength = max(clamp(gui_input.fruit_swing_min_response, 0.0, 1.0),
                         flora_wind_planar_strength(wind_vec));
    vec2 wind_dir = flora_wind_planar_dir(wind_vec);
    vec3 along_wind_dir = vec3(wind_dir.x, 0.0, wind_dir.y);
    vec3 cross_wind_dir = vec3(-wind_dir.y, 0.0, wind_dir.x);

    float phase = construct_float_01(wellons_hash(instance_seed ^ 0x9E3779B9u)) * FLORA_TWO_PI;
    float speed_jitter =
        construct_float_01(wellons_hash(instance_seed ^ 0xB7E15162u)) * 2.0 - 1.0;
    float direction_jitter =
        construct_float_01(wellons_hash(instance_seed ^ 0xC2B2AE35u)) * 2.0 - 1.0;
    float speed = max(gui_input.fruit_swing_speed, 0.0) *
                  max(0.0, 1.0 + speed_jitter * gui_input.fruit_swing_speed_variation);
    vec3 swing_dir = normalize(along_wind_dir + cross_wind_dir * direction_jitter * 0.35);
    float angle = sin(time * speed + phase) *
                  max(gui_input.fruit_swing_max_angle_radians, 0.0) * strength;
    float pivot_length = max(gui_input.fruit_swing_length_voxels, 0.0);

    // Translate every voxel by the center displacement of a pendulum rotating around the fruit's
    // fixed top-center attachment. The mesh itself is never rotated, so voxel faces stay aligned
    // with the world axes while the fruit follows the expected circular arc.
    float horizontal_offset = sin(angle) * pivot_length;
    float vertical_offset = (1.0 - cos(angle)) * pivot_length;
    return swing_dir * horizontal_offset + vec3(0.0, vertical_offset, 0.0);
}

vec3 leaf_wind_paddling(vec3 wind_vec, float wind_gradient, uint instance_seed,
                        ivec3 vox_local_pos, ivec3 gradient_origin, float time) {
    float wind_strength = length(wind_vec.xz);
    float amplitude_response = flora_wind_curve_response(
        wind_strength, gui_input.leaf_paddle_amplitude_wind_start_strength,
        gui_input.leaf_paddle_amplitude_wind_full_strength,
        gui_input.leaf_paddle_amplitude_wind_knee_bias);
    if (amplitude_response <= 0.0) {
        return vec3(0.0);
    }

    float frequency_response = flora_wind_curve_response(
        wind_strength, gui_input.leaf_paddle_frequency_wind_start_strength,
        gui_input.leaf_paddle_frequency_wind_full_strength,
        gui_input.leaf_paddle_frequency_wind_knee_bias);
    float frequency_multiplier = max(
        0.0, mix(gui_input.leaf_paddle_frequency_min_multiplier,
                 gui_input.leaf_paddle_frequency_max_multiplier, frequency_response));

    vec2 wind_dir = flora_wind_planar_dir(wind_vec);
    vec3 cross_wind_dir = vec3(-wind_dir.y, 0.0, wind_dir.x);
    vec3 flap_dir = normalize(vec3(cross_wind_dir.x * 0.35, 1.0, cross_wind_dir.z * 0.35));
    vec3 local_dir = normalize(vec3(vox_local_pos - gradient_origin) + vec3(1e-3));
    float wind_side = dot(local_dir, vec3(wind_dir.x, 0.0, wind_dir.y));
    float phase = flora_wind_phase(instance_seed, vox_local_pos, 0x68E31DA4u);
    float shell_phase = wind_side * 2.4 + float(vox_local_pos.y - gradient_origin.y) * 0.13;
    float paddle = sin(time * gui_input.leaf_paddle_primary_speed * frequency_multiplier + phase + shell_phase);
    paddle += 0.35 * sin(time * gui_input.leaf_paddle_secondary_speed * frequency_multiplier +
                         phase * 1.61 - shell_phase * 0.7);

    // Keep the cloud attached near the branch-facing center while letting the exposed shell flutter.
    float shell_weight = pow(clamp(wind_gradient, 0.0, 1.0), 1.15);
    float exposed_side_weight = 0.55 + 0.45 * abs(wind_side);
    return flap_dir * (paddle * gui_input.leaf_paddle_amplitude_voxels * amplitude_response *
                       shell_weight * exposed_side_weight);
}

#endif // FLORA_WIND_MOTION_GLSL
