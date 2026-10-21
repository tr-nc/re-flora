module grass_moisture_material;

import flora_types;
import voxel_types;

// The atlas is the sole authority. This is root/soil moisture, not leaf water.
public uint floraSupportMoistureLevel(uint atlasData, bool validSupport)
{
    uint type = atlasData & 0x0fu;
    return validSupport && (type == VOXEL_TYPE_DIRT || type == VOXEL_TYPE_SAND)
        ? (atlasData >> 4u) & 3u : 0u;
}

// Apply to linear albedo AFTER stress tint, BEFORE any lighting. A fixed modest
// strength keeps this a material response, independent of growth/competition.
public float3 applyGrassMoistureMaterial(float3 color, uint species,
                                        uint moistureLevel, bool enabled)
{
    if (!enabled || (species != FLORA_SPECIES_TALL_GRASS &&
                     species != FLORA_SPECIES_SHORT_GRASS)) return color;
    return color * (1.0 - 0.25 * float(min(moistureLevel, 3u)) / 3.0);
}
