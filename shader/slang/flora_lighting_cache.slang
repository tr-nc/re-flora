module flora_lighting_cache;

import flora_types;

public uint floraLightingCacheIndex(PushConstantFlora pc,
                                    uint localInstanceIndex,
                                    uint voxelIndex)
{
    return floraLightingCacheOffset(pc) +
           localInstanceIndex * floraLightingCacheVoxelCount(pc) + voxelIndex;
}
