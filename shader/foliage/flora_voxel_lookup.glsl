#ifndef FLORA_VOXEL_LOOKUP_GLSL
#define FLORA_VOXEL_LOOKUP_GLSL

#include "../include/flora_registry.glsl"

const uint FLORA_VOXEL_LOOKUP_TYPE_COUNT = FLORA_SPECIES_APPLE + 1u;
const uint FLORA_VOXEL_LOOKUP_EMPTY_KEY  = 0xffffffffu;
const uint FLORA_VOXEL_LOOKUP_MAX_PROBES = 64u;
const uint FLORA_VOXEL_MATERIAL_GRADIENT = 0u;
const uint FLORA_VOXEL_MATERIAL_ALLIUM_CORE = 1u;
const uint FLORA_LOOKUP_BITS_PER_AXIS    = 10u;
const uint FLORA_LOOKUP_AXIS_MASK        = (1u << FLORA_LOOKUP_BITS_PER_AXIS) - 1u;
const int FLORA_LOOKUP_AXIS_BIAS         = 1 << (FLORA_LOOKUP_BITS_PER_AXIS - 1u);

layout(set = 0, binding = 13, std430) readonly buffer B_FloraVoxelTableDescs {
    // x: entry offset, y: hash-table capacity, z: fallback packed info, w: species max length.
    uvec4 descs[FLORA_VOXEL_LOOKUP_TYPE_COUNT];
}
flora_voxel_table_descs;

layout(set = 0, binding = 14, std430) readonly buffer B_FloraVoxelInfos {
    // x: encoded local-position key, y: packed FloraVoxelInfo.
    uvec2 entries[];
}
flora_voxel_infos;

uint flora_lookup_hash(uint key) {
    uint x = key ^ 0x9E3779B9u;
    x ^= x >> 16u;
    x *= 0x7FEB352Du;
    x ^= x >> 15u;
    x *= 0x846CA68Bu;
    return x ^ (x >> 16u);
}

uint encode_flora_lookup_pos_key(ivec3 pos) {
    ivec3 shifted = pos + ivec3(FLORA_LOOKUP_AXIS_BIAS);
    if (any(lessThan(shifted, ivec3(0))) || any(greaterThan(shifted, ivec3(int(FLORA_LOOKUP_AXIS_MASK))))) {
        return FLORA_VOXEL_LOOKUP_EMPTY_KEY;
    }
    uvec3 p = uvec3(shifted);
    return p.x | (p.y << FLORA_LOOKUP_BITS_PER_AXIS) |
           (p.z << (FLORA_LOOKUP_BITS_PER_AXIS * 2u));
}

uint lookup_flora_voxel_info(uint instance_ty, ivec3 vox_local_pos) {
    if (instance_ty >= FLORA_VOXEL_LOOKUP_TYPE_COUNT) {
        return 0u;
    }
    uvec4 desc = flora_voxel_table_descs.descs[instance_ty];
    uint offset = desc.x;
    uint capacity = desc.y;
    uint fallback_info = desc.z;
    if (capacity == 0u) {
        return fallback_info;
    }

    uint key = encode_flora_lookup_pos_key(vox_local_pos);
    if (key == FLORA_VOXEL_LOOKUP_EMPTY_KEY) {
        return fallback_info;
    }

    uint mask = capacity - 1u;
    uint start_slot = flora_lookup_hash(key) & mask;
    for (uint probe = 0u; probe < FLORA_VOXEL_LOOKUP_MAX_PROBES; probe++) {
        uint slot = offset + ((start_slot + probe) & mask);
        uvec2 entry = flora_voxel_infos.entries[slot];
        if (entry.x == key) {
            return entry.y;
        }
        if (entry.x == FLORA_VOXEL_LOOKUP_EMPTY_KEY) {
            return fallback_info;
        }
    }
    return fallback_info;
}

float flora_voxel_info_unorm8(uint info, uint shift) {
    return float((info >> shift) & 0xffu) * (1.0 / 255.0);
}

float flora_voxel_color_gradient(uint info) {
    return flora_voxel_info_unorm8(info, 0u);
}

float flora_voxel_wind_gradient(uint info) {
    return flora_voxel_info_unorm8(info, 8u);
}

float flora_voxel_growth_gradient(uint info) {
    return flora_voxel_info_unorm8(info, 16u);
}

uint flora_voxel_material_id(uint info) {
    return info >> 24u;
}

float flora_voxel_lookup_max_length(uint instance_ty) {
    if (instance_ty >= FLORA_VOXEL_LOOKUP_TYPE_COUNT) {
        return 1.0;
    }
    return max(float(flora_voxel_table_descs.descs[instance_ty].w), 1.0);
}

#endif // FLORA_VOXEL_LOOKUP_GLSL
