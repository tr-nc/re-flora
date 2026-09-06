module vegetation_response_sample;
import vegetation_response_types;
import flora_types;
import flora_motion;

struct B_ManualVegetationResponse { VegetationResponseState data[]; };
[[vk::binding(4, 1)]] readonly GLSLShaderStorageBuffer<B_ManualVegetationResponse, Std430DataLayout> vegetation_response;
[[vk::binding(5, 1)]] ConstantBuffer<U_VegetationResponseInfo> vegetation_response_info;

public bool vegetationResponseEnabled() { return vegetation_response_info.shape.z != 0u; }
public bool attachedVegetationResponseEnabled() {
    return vegetationResponseEnabled() && vegetation_response_info.grid.w >= 3.0;
}

float2 responseAt(uint index, uint bucket)
{
    return vegetation_response.data[index].held[bucket].xy;
}

public float3 sampleVegetationResponse(uint3 rootVoxels, uint seed, uint species,
                                       uint localIndex, uint responseOffset)
{
    if (!vegetationResponseEnabled()) return float3(0.0);
    uint bucket = windVolumeBucketIndex(seed);
    float2 response;
    if (species < 2u || species >= FLORA_SPECIES_TREE_LEAF)
    {
        uint field = species < 2u ? 0u : (species == FLORA_SPECIES_TREE_LEAF ? 1u : 2u);
        uint offset = field * vegetation_response_info.shape.x * vegetation_response_info.shape.y;
        float2 cell = (float2(rootVoxels.xz) / 256.0 - vegetation_response_info.grid.xy) /
                     vegetation_response_info.grid.z;
        uint2 shape = vegetation_response_info.shape.xy;
        cell = clamp(cell, float2(0.0), float2(shape - 1u));
        uint2 lo = uint2(floor(cell));
        uint2 hi = min(lo + 1u, shape - 1u);
        float2 f = frac(cell);
        response = lerp(
            lerp(responseAt(offset + lo.x + lo.y * shape.x, bucket), responseAt(offset + hi.x + lo.y * shape.x, bucket), f.x),
            lerp(responseAt(offset + lo.x + hi.y * shape.x, bucket), responseAt(offset + hi.x + hi.y * shape.x, bucket), f.x), f.y);
    }
    else
    {
        response = responseAt(responseOffset + localIndex, bucket);
    }
    return float3(response.x, 0.0, response.y);
}
