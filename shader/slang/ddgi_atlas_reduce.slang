import ddgi_octahedral;
import ddgi_types;

static const uint DDGI_REDUCTION_WORKGROUP_SIZE = 64u;
static const uint DDGI_IRRADIANCE_INTERIOR_SIDE = 8u;
static const uint DDGI_IRRADIANCE_STORED_SIDE = 10u;
static const uint DDGI_STORED_TEXELS_PER_PROBE =
    DDGI_IRRADIANCE_STORED_SIDE * DDGI_IRRADIANCE_STORED_SIDE;

[[vk::binding(24, 0)]]
readonly GLSLShaderStorageBuffer<B_DdgiProbeMetadata, Std430DataLayout>
    ddgi_probe_metadata;

[[vk::binding(27, 0)]]
readonly RWTexture2D<float4> ddgi_irradiance_atlas;

[[vk::binding(29, 0)]]
readonly RWTexture2D<float4> ddgi_transport_source_irradiance_atlas;

[[vk::binding(33, 0)]]
GLSLShaderStorageBuffer<B_DdgiAtlasReduction, Std430DataLayout>
    ddgi_atlas_reduction;

struct DdgiAtlasReductionPushConstants
{
    uint terrain_revision;
    uint probe_count;
    uint tile_columns;
    uint destination_slot;
    uint source_slot;
    uint has_source;
    float relative_floor;
    uint padding;
};

[[vk::push_constant]]
ConstantBuffer<DdgiAtlasReductionPushConstants> pc;

float4 loadAtlas(uint slot, uint2 coordinate)
{
    return slot == 0u
        ? ddgi_irradiance_atlas[coordinate]
        : ddgi_transport_source_irradiance_atlas[coordinate];
}

bool finite4(float4 value)
{
    return !any(isnan(value)) && !any(isinf(value));
}

[shader("compute")]
[numthreads(DDGI_REDUCTION_WORKGROUP_SIZE, 1, 1)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    uint linearIndex = dispatchThreadId.x;
    uint storedTexelCount = pc.probe_count * DDGI_STORED_TEXELS_PER_PROBE;
    if (linearIndex >= storedTexelCount) return;

    uint probeIndex = linearIndex / DDGI_STORED_TEXELS_PER_PROBE;
    uint texelIndex = linearIndex % DDGI_STORED_TEXELS_PER_PROBE;
    DdgiProbeMetadata metadata = ddgi_probe_metadata.data[probeIndex];
    if (metadata.state_and_reserved.x != DDGI_PROBE_STATE_VALID ||
        metadata.state_and_reserved.y != pc.terrain_revision)
    {
        return;
    }

    uint2 local = uint2(texelIndex % DDGI_IRRADIANCE_STORED_SIDE,
                        texelIndex / DDGI_IRRADIANCE_STORED_SIDE);
    uint2 coordinate = ddgiTileOrigin(
        probeIndex, pc.tile_columns, DDGI_IRRADIANCE_STORED_SIDE) + local;
    float4 destination = loadAtlas(pc.destination_slot, coordinate);
    float4 source = pc.has_source != 0u
        ? loadAtlas(pc.source_slot, coordinate)
        : float4(0.0);

    uint ignored;
    InterlockedAdd(ddgi_atlas_reduction.data[5], 1u, ignored);
    if (!finite4(destination) || (pc.has_source != 0u && !finite4(source)))
    {
        InterlockedAdd(ddgi_atlas_reduction.data[2], 1u, ignored);
        return;
    }

    bool negativeRgb = any(destination.xyz < float3(0.0)) ||
        (pc.has_source != 0u && any(source.xyz < float3(0.0)));
    if (negativeRgb)
    {
        InterlockedAdd(ddgi_atlas_reduction.data[3], 1u, ignored);
    }

    bool interior = all(local >= uint2(1u)) &&
                    all(local <= uint2(DDGI_IRRADIANCE_INTERIOR_SIDE));
    if (!interior) return;

    InterlockedAdd(ddgi_atlas_reduction.data[4], 1u, ignored);
    if (pc.has_source == 0u) return;

    float3 absoluteDelta = abs(destination.xyz - source.xyz);
    float3 relativeDenominator = max(
        max(abs(destination.xyz), abs(source.xyz)),
        float3(pc.relative_floor));
    float maxAbsolute = max(absoluteDelta.x,
                            max(absoluteDelta.y, absoluteDelta.z));
    float3 relativeDelta = absoluteDelta / relativeDenominator;
    float maxRelative = max(relativeDelta.x,
                            max(relativeDelta.y, relativeDelta.z));
    InterlockedMax(ddgi_atlas_reduction.data[0], asuint(maxAbsolute), ignored);
    InterlockedMax(ddgi_atlas_reduction.data[1], asuint(maxRelative), ignored);
}
