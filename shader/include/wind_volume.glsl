#ifndef WIND_VOLUME_GLSL
#define WIND_VOLUME_GLSL

const uint FLORA_UPDATE_BUCKET_COUNT_DEFAULT = 4u;
const uint WIND_VOLUME_BUCKET_COUNT_MAX      = 16u;

uint get_flora_update_bucket_count() {
    uint bucket_count = gui_input.flora_update_bucket_count;
    if (bucket_count == 0u) {
        return FLORA_UPDATE_BUCKET_COUNT_DEFAULT;
    }
    return min(bucket_count, WIND_VOLUME_BUCKET_COUNT_MAX);
}

uint get_wind_volume_bucket_index(uint instance_seed) {
    uint bucket_count = get_flora_update_bucket_count();
    return bucket_count <= 1u ? 0u : instance_seed % bucket_count;
}

vec3 sample_wind_volume(vec3 world_pos, uint instance_seed) {
    vec3 local_uv      = clamp(world_pos / wind_volume_info.world_chunk_extent, vec3(0.0), vec3(1.0));
    ivec3 volume_size  = textureSize(wind_volume_tex, 0);
    float bucket_width = float(volume_size.x) / float(WIND_VOLUME_BUCKET_COUNT_MAX);
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
