#ifndef FLORA_SURFACE_PLANTING_GLSL
#define FLORA_SURFACE_PLANTING_GLSL

#include "./voxel_types.glsl"

bool is_flora_surface_plantable(uint voxel_type) { return voxel_type == VOXEL_TYPE_DIRT; }

#endif // FLORA_SURFACE_PLANTING_GLSL
