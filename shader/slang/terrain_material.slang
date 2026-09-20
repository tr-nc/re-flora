module terrain_material;

import voxel_types;

// A stable independent value per global voxel. Never interpolate neighboring cells or use
// camera, time, chunk-local coordinates or a ray seed. floor also handles negative cells.
uint terrainMaterialHash(int3 cell, uint seed)
{
    uint n = uint(cell.x) * 374761393u ^ uint(cell.y) * 668265263u ^
             uint(cell.z) * 2147483647u ^ seed;
    n = (n ^ (n >> 13u)) * 1274126177u;
    n = (n ^ (n >> 16u)) * 2246822519u;
    return n ^ (n >> 15u);
}

public float3 applyTerrainMaterial(float3 baseLinear, uint voxelType,
                                  float3 voxelCenterWorld, float4 variation,
                                  uint seed, uint enabled)
{
    bool isSoil = voxelType == VOXEL_TYPE_DIRT || voxelType == VOXEL_TYPE_SAND;
    bool isRock = voxelType == VOXEL_TYPE_ROCK;
    if (enabled == 0u || (!isSoil && !isRock)) return baseLinear;
    float strength = isRock ? variation.y : variation.x;
    if (strength <= 0.0) return baseLinear;

    int3 cell = int3(floor(voxelCenterWorld * 256.0));
    float pattern = float(terrainMaterialHash(cell, seed) >> 8u) * (2.0 / 16777215.0) - 1.0;
    float signedAmount = pattern * strength;
    // Authored linear-RGB base is the midpoint. Optional cool-dark/warm-light bias fades
    // with strength; disabled/zero strength preserve the original palette exactly.
    float3 band = lerp(float3(1.0),
                      signedAmount < 0.0 ? float3(0.90, 0.97, 1.05)
                                         : float3(1.05, 1.00, 0.90),
                      abs(signedAmount) * variation.z);
    return saturate(baseLinear * (1.0 + signedAmount) * band);
}
