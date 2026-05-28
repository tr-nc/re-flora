#ifndef WIND_VOLUME_GLSL
#define WIND_VOLUME_GLSL

const uint WIND_VOLUME_BUCKET_COUNT = 4u;

uint get_wind_volume_bucket_index(uint instance_seed) {
    return instance_seed % WIND_VOLUME_BUCKET_COUNT;
}

uint get_wind_volume_voxel_seed(uint instance_seed, ivec3 vox_local_pos) {
    uint seed = instance_seed ^ 0x9E3779B9u;
    seed ^= uint(vox_local_pos.x) * 0x85EBCA6Bu;
    seed ^= uint(vox_local_pos.y) * 0xC2B2AE35u;
    seed ^= uint(vox_local_pos.z) * 0x27D4EB2Fu;
    return seed ^ (seed >> 16u);
}

float wind_volume_bucketed_time(uint bucket_index, float time) {
    float step_seconds = gui_input.world_tick_seconds;
    if (step_seconds <= 0.0) {
        return time;
    }

    uint bucket_count = WIND_VOLUME_BUCKET_COUNT;
    uint step_index = uint(floor(max(time, 0.0) / step_seconds));
    uint active_bucket_index = step_index % bucket_count;
    if (bucket_index == active_bucket_index) {
        return time;
    }
    if (step_index < bucket_index) {
        return 0.0;
    }

    uint last_bucket_step =
        bucket_index + ((step_index - bucket_index) / bucket_count) * bucket_count;
    return float(last_bucket_step + 1u) * step_seconds;
}

vec3 sample_wind_volume(vec3 world_pos, uint instance_seed) {
    vec3 local_uv      = clamp(world_pos / wind_volume_info.world_chunk_extent, vec3(0.0), vec3(1.0));
    ivec3 volume_size  = textureSize(wind_volume_tex, 0);
    float bucket_width = float(volume_size.x) / float(WIND_VOLUME_BUCKET_COUNT);
    float bucket_x = float(get_wind_volume_bucket_index(instance_seed)) * bucket_width +
                     local_uv.x * (bucket_width - 1.0) + 0.5;
    float sample_y = local_uv.y * float(volume_size.y - 1) + 0.5;
    float sample_z = local_uv.z * float(volume_size.z - 1) + 0.5;
    vec3 wind_uv = vec3(bucket_x / float(volume_size.x), sample_y / float(volume_size.y),
                        sample_z / float(volume_size.z));
    vec2 wind_planar = texture(wind_volume_tex, wind_uv).xy;
    return vec3(wind_planar.x, 0.0, wind_planar.y);
}

#endif // WIND_VOLUME_GLSL
