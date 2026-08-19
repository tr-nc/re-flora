module flora_surface_planting;

import voxel_types;

public bool isFloraSurfacePlantable(uint voxelType)
{
    return voxelType == VOXEL_TYPE_DIRT || voxelType == VOXEL_TYPE_STUCCO;
}
