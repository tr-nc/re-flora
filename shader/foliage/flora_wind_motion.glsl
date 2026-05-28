#ifndef FLORA_WIND_MOTION_GLSL
#define FLORA_WIND_MOTION_GLSL

const float FLORA_TWO_PI = 6.28318530718;
const float GRASS_VIBRATION_AMPLITUDE_VOXELS = 0.30;
const float GRASS_VIBRATION_PRIMARY_SPEED = 31.0;
const float GRASS_VIBRATION_SECONDARY_SPEED = 47.0;
const float LEAF_PADDLE_AMPLITUDE_VOXELS = 0.80;
const float LEAF_PADDLE_PRIMARY_SPEED = 9.0;
const float LEAF_PADDLE_SECONDARY_SPEED = 15.0;

float flora_wind_planar_strength(vec3 wind_vec) {
    return smoothstep(0.03, 2.0, length(wind_vec.xz));
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
    float vibration = sin(time * GRASS_VIBRATION_PRIMARY_SPEED + phase + height_phase);
    vibration += 0.45 * sin(time * GRASS_VIBRATION_SECONDARY_SPEED + phase * 1.37 - height_phase * 0.6);

    return cross_wind_dir * (vibration * GRASS_VIBRATION_AMPLITUDE_VOXELS * strength * tip_weight);
}

vec3 leaf_wind_paddling(vec3 wind_vec, float wind_gradient, uint instance_seed,
                        ivec3 vox_local_pos, ivec3 gradient_origin, float time) {
    float strength = flora_wind_planar_strength(wind_vec);
    if (strength <= 0.0) {
        return vec3(0.0);
    }

    vec2 wind_dir = flora_wind_planar_dir(wind_vec);
    vec3 cross_wind_dir = vec3(-wind_dir.y, 0.0, wind_dir.x);
    vec3 flap_dir = normalize(vec3(cross_wind_dir.x * 0.35, 1.0, cross_wind_dir.z * 0.35));
    vec3 local_dir = normalize(vec3(vox_local_pos - gradient_origin) + vec3(1e-3));
    float wind_side = dot(local_dir, vec3(wind_dir.x, 0.0, wind_dir.y));
    float phase = flora_wind_phase(instance_seed, vox_local_pos, 0x68E31DA4u);
    float shell_phase = wind_side * 2.4 + float(vox_local_pos.y - gradient_origin.y) * 0.13;
    float paddle = sin(time * LEAF_PADDLE_PRIMARY_SPEED + phase + shell_phase);
    paddle += 0.35 * sin(time * LEAF_PADDLE_SECONDARY_SPEED + phase * 1.61 - shell_phase * 0.7);

    // Keep the cloud attached near the branch-facing center while letting the exposed shell flutter.
    float shell_weight = pow(clamp(wind_gradient, 0.0, 1.0), 1.15);
    float exposed_side_weight = 0.55 + 0.45 * abs(wind_side);
    return flap_dir * (paddle * LEAF_PADDLE_AMPLITUDE_VOXELS * strength * shell_weight * exposed_side_weight);
}

#endif // FLORA_WIND_MOTION_GLSL
