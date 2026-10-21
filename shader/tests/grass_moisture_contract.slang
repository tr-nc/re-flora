import grass_moisture_material;
import flora_types;
import voxel_types;

export __extern_cpp int main()
{
    uint failures = 0u;
    float3 dry = float3(0.12, 0.4, 0.03);
    // Every atlas byte, including reserved bits and all non-soil types.
    for (uint data = 0u; data < 256u; ++data)
    {
        uint type = data & 15u;
        uint expected = type == VOXEL_TYPE_DIRT || type == VOXEL_TYPE_SAND
            ? (data >> 4u) & 3u : 0u;
        if (floraSupportMoistureLevel(data, true) != expected) failures++;
        if (floraSupportMoistureLevel(data, false) != 0u) failures++;
    }
    for (uint species = 0u; species <= FLORA_SPECIES_APPLE + 1u; ++species)
    {
        float3 previous = dry;
        for (uint level = 0u; level <= 4u; ++level)
        {
            float3 wet = applyGrassMoistureMaterial(dry, species, level, true);
            if (!all(applyGrassMoistureMaterial(dry, species, level, false) == dry)) failures++;
            if (species > FLORA_SPECIES_SHORT_GRASS || level == 0u)
                if (!all(wet == dry)) failures++;
            if (any(wet > previous) || any(wet < dry * 0.75)) failures++;
            previous = wet;
            // Tint is an input, not an inferred growth factor. Uniform scaling
            // must commute with any pre-existing linear stress tint result.
            float3 stressed = float3(0.31, 0.22, 0.07);
            float3 result = applyGrassMoistureMaterial(stressed, species, level, true);
            if (any(abs(result / stressed - wet / dry) > float3(1e-6))) failures++;
        }
    }
    printf("grass_moisture failures=%u\n", failures);
    return failures == 0u ? 0 : 1;
}
