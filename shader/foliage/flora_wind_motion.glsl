#ifndef FLORA_WIND_MOTION_GLSL
#define FLORA_WIND_MOTION_GLSL

const float FLORA_TWO_PI = 6.28318530718;
const float GRASS_NATURAL_BEND_MIN_VOXELS = 0.12;
const float GRASS_NATURAL_BEND_MAX_VOXELS = 0.70;
const float SHORT_GRASS_NATURAL_BEND_SCALE = 0.55;

float flora_wind_planar_strength(vec3 wind_vec) {
    return smoothstep(0.03, 2.0, length(wind_vec.xz));
}

float flora_smootherstep(float t) {
    t = clamp(t, 0.0, 1.0);
    return t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
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

vec3 grass_natural_rest_bend(uint instance_ty, uint instance_seed, float wind_gradient) {
    // Even in calm air, blades are not perfectly vertical: weight, growth curvature,
    // and clump variation give each blade a stable rest lean in a random direction.
    float direction_angle =
        construct_float_01(wellons_hash(instance_seed ^ 0xD1B54A32u)) * FLORA_TWO_PI;
    float amount_jitter = construct_float_01(wellons_hash(instance_seed ^ 0x94D049BBu));
    float species_scale = instance_ty == FLORA_SPECIES_SHORT_GRASS ?
                              SHORT_GRASS_NATURAL_BEND_SCALE :
                              1.0;
    float tip_weight = pow(clamp(wind_gradient, 0.0, 1.0), 1.65);
    float bend_voxels =
        mix(GRASS_NATURAL_BEND_MIN_VOXELS, GRASS_NATURAL_BEND_MAX_VOXELS, amount_jitter) *
        species_scale * tip_weight;
    return vec3(cos(direction_angle), 0.0, sin(direction_angle)) * bend_voxels;
}

vec3 grass_wind_vibration(vec3 wind_vec, float wind_gradient, uint instance_seed,
                          ivec3 vox_local_pos, float time) {
    float strength = flora_wind_planar_strength(wind_vec);
    if (strength <= 0.0) {
        return vec3(0.0);
    }

    vec2 wind_dir = flora_wind_planar_dir(wind_vec);
    vec3 cross_wind_dir = vec3(-wind_dir.y, 0.0, wind_dir.x);
    float tip_weight = pow(clamp(wind_gradient, 0.0, 1.0), 1.7);
    float phase = flora_wind_phase(instance_seed, vox_local_pos, 0xB5297A4Du);
    float height_phase = float(vox_local_pos.y) * 1.35;
    float vibration = sin(time * gui_input.grass_vibration_primary_speed + phase + height_phase);
    vibration += 0.45 * sin(time * gui_input.grass_vibration_secondary_speed + phase * 1.37 - height_phase * 0.6);

    return cross_wind_dir * (vibration * gui_input.grass_vibration_amplitude_voxels * strength * tip_weight);
}

vec3 apple_wind_swing(vec3 wind_vec, uint instance_seed, float time) {
    float strength = max(0.18, flora_wind_planar_strength(wind_vec));
    vec2 wind_dir = flora_wind_planar_dir(wind_vec);
    vec3 along_wind_dir = vec3(wind_dir.x, 0.0, wind_dir.y);
    vec3 cross_wind_dir = vec3(-wind_dir.y, 0.0, wind_dir.x);

    float phase = construct_float_01(wellons_hash(instance_seed ^ 0x9E3779B9u)) * FLORA_TWO_PI;
    float speed_jitter = construct_float_01(wellons_hash(instance_seed ^ 0xB7E15162u));
    float amp_jitter = construct_float_01(wellons_hash(instance_seed ^ 0xC2B2AE35u));
    float speed = mix(1.6, 2.7, speed_jitter);
    float amplitude_voxels = mix(1.3, 2.7, amp_jitter) * strength;

    float swing = sin(time * speed + phase);
    float clatter = 0.36 * sin(time * speed * 2.73 + phase * 1.41);
    float bob = -abs(swing) * 0.28 * amplitude_voxels;

    return along_wind_dir * (swing * amplitude_voxels) +
           cross_wind_dir * (clatter * amplitude_voxels) + vec3(0.0, bob, 0.0);
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
