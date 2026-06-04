#ifndef FLORA_WIND_MOTION_GLSL
#define FLORA_WIND_MOTION_GLSL

const float FLORA_TWO_PI = 6.28318530718;

float flora_wind_planar_strength(vec3 wind_vec) {
    return smoothstep(0.03, 2.0, length(wind_vec.xz));
}

float flora_wind_curve_response(float wind_strength, float min_strength, float max_strength,
                                float curve_power) {
    float lo = min(min_strength, max_strength);
    float hi = max(min_strength, max_strength);
    float t = clamp((wind_strength - lo) / max(hi - lo, 1e-4), 0.0, 1.0);
    t = t * t * (3.0 - 2.0 * t);
    return pow(t, max(curve_power, 0.001));
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
        wind_strength, gui_input.leaf_paddle_amplitude_wind_min_strength,
        gui_input.leaf_paddle_amplitude_wind_max_strength,
        gui_input.leaf_paddle_amplitude_wind_curve_power);
    if (amplitude_response <= 0.0) {
        return vec3(0.0);
    }

    float frequency_response = flora_wind_curve_response(
        wind_strength, gui_input.leaf_paddle_frequency_wind_min_strength,
        gui_input.leaf_paddle_frequency_wind_max_strength,
        gui_input.leaf_paddle_frequency_wind_curve_power);
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
