module terrain_material;

import voxel_types;

// One deterministic macro field for all opaque terrain consumers. Inputs are global voxel
// centers (world units), never a chunk-local coordinate, frame seed or camera position.
// The two packed vectors are shared by the live and latched DDGI palette uniforms.
uint terrainMaterialHash(int3 cell, uint seed)
{
    uint n = uint(cell.x) * 374761393u ^ uint(cell.y) * 668265263u ^
             uint(cell.z) * 2147483647u ^ seed;
    n = (n ^ (n >> 13u)) * 1274126177u;
    n = (n ^ (n >> 16u)) * 2246822519u;
    return n ^ (n >> 15u);
}

float terrainMaterialLattice(int3 cell, uint seed)
{
    return float(terrainMaterialHash(cell, seed) >> 8u) * (2.0 / 16777215.0) - 1.0;
}

float terrainMaterialNoise(float3 position, uint seed)
{
    int3 cell = int3(floor(position));
    float3 f = position - float3(cell);
    float3 w = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    float z0 = lerp(
        lerp(terrainMaterialLattice(cell, seed),
             terrainMaterialLattice(cell + int3(1, 0, 0), seed), w.x),
        lerp(terrainMaterialLattice(cell + int3(0, 1, 0), seed),
             terrainMaterialLattice(cell + int3(1, 1, 0), seed), w.x), w.y);
    float z1 = lerp(
        lerp(terrainMaterialLattice(cell + int3(0, 0, 1), seed),
             terrainMaterialLattice(cell + int3(1, 0, 1), seed), w.x),
        lerp(terrainMaterialLattice(cell + int3(0, 1, 1), seed),
             terrainMaterialLattice(cell + int3(1, 1, 1), seed), w.x), w.y);
    return lerp(z0, z1, w.z);
}

public float3 applyTerrainMaterial(float3 baseLinear, uint voxelType,
                                  float3 voxelCenterWorld,
                                  float4 soil, float4 rock,
                                  uint seed, uint enabled)
{
    bool isSoil = voxelType == VOXEL_TYPE_DIRT || voxelType == VOXEL_TYPE_SAND;
    bool isRock = voxelType == VOXEL_TYPE_ROCK;
    if (enabled == 0u || (!isSoil && !isRock)) return baseLinear;
    float strength = isRock ? rock.y : soil.y;
    if (strength <= 0.0) return baseLinear;

    float3 p = voxelCenterWorld * 256.0;
    float pattern;
    if (isRock)
    {
        float scale = max(rock.x, 8.0);
        // Reuse a single broad field for both the phase warp and the weak mottling.
        float warp = terrainMaterialNoise(p / (1.7 * scale), seed ^ 0x02e5be93u);
        float phase = dot(p, float3(rock.z, rock.w, 0.0)) / scale + 0.12 * warp;
        pattern = 0.75 * sin(6.28318530718 * phase) + 0.25 * warp;
    }
    else
    {
        pattern = terrainMaterialNoise(p / max(soil.x, 8.0), seed);
    }

    // Continuous linear-RGB band: authored base is the midpoint. Zero strength is exact
    // baseline; the optional cool-dark/warm-light bias vanishes together with the contrast.
    float signedAmount = pattern * strength;
    float3 band = lerp(float3(1.0),
                      signedAmount < 0.0 ? float3(0.90, 0.97, 1.05)
                                         : float3(1.05, 1.00, 0.90),
                      abs(signedAmount) * soil.z);
    return saturate(baseLinear * (1.0 + signedAmount) * band);
}
