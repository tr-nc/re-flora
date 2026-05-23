#ifndef SOLID_WORKGROUPS_GLSL
#define SOLID_WORKGROUPS_GLSL

#define SOLID_WORKGROUP_SIZE 8u

uvec3 solid_workgroup_grid_dim() {
    uvec3 atlas_dim = uvec3(imageSize(chunk_atlas).xyz);
    return (atlas_dim + uvec3(SOLID_WORKGROUP_SIZE - 1u)) / SOLID_WORKGROUP_SIZE;
}

uint solid_workgroup_index_from_coord(uvec3 workgroup_coord, uvec3 grid_dim) {
    return workgroup_coord.x + workgroup_coord.z * grid_dim.x +
           workgroup_coord.y * grid_dim.x * grid_dim.z;
}

uint solid_workgroup_index_from_voxel(uvec3 voxel_pos) {
    uvec3 grid_dim        = solid_workgroup_grid_dim();
    uvec3 workgroup_coord = voxel_pos / SOLID_WORKGROUP_SIZE;
    return solid_workgroup_index_from_coord(workgroup_coord, grid_dim);
}

#ifndef SOLID_WORKGROUPS_READ_ONLY
void mark_solid_workgroup(uvec3 voxel_pos) {
    uint workgroup_idx = solid_workgroup_index_from_voxel(voxel_pos);
    uint flag_word     = workgroup_idx >> 5u;
    uint flag_mask     = 1u << (workgroup_idx & 31u);
    atomicOr(solid_workgroup_flags.data[flag_word], flag_mask);
}
#endif

#endif
