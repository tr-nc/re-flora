import packer;

static const uint DENOISER_GROUP_SIZE = 8u;
static const int2 TEMPORAL_OFFSETS[4] =
{
    int2(0, 0),
    int2(1, 0),
    int2(0, 1),
    int2(1, 1),
};

struct U_TemporalInfo
{
    float temporal_position_phi;
    float temporal_alpha;
};

struct TemporalPushConstants
{
    uint reset_history;
};

[[vk::push_constant]]
ConstantBuffer<TemporalPushConstants> pc;

[[vk::binding(0, 0)]]
ConstantBuffer<U_TemporalInfo> temporal_info;
[[vk::binding(1, 0)]]
[[vk::image_format("unknown")]]
Sampler2D<uint> compute_output_tex;

[[vk::binding(0, 1)]]
[[vk::image_format("unknown")]]
Sampler2D<uint> denoiser_normal_tex;
[[vk::binding(1, 1)]]
[[vk::image_format("unknown")]]
Sampler2D<uint> denoiser_normal_tex_prev;
[[vk::binding(2, 1)]]
[[vk::image_format("rgba32f")]]
// XYZ is the current hit position. Temporal resolve writes the new history
// length to W before this texture is copied to the previous-frame image. The
// tracer writes zero to W for a miss.
RWTexture2D<float4> denoiser_position_tex;
[[vk::binding(3, 1)]] Sampler2D denoiser_position_tex_prev;
[[vk::binding(4, 1)]]
[[vk::image_format("unknown")]]
Sampler2D<uint> denoiser_vox_id_tex;
[[vk::binding(5, 1)]]
[[vk::image_format("unknown")]]
Sampler2D<uint> denoiser_vox_id_tex_prev;
[[vk::binding(7, 1)]]
[[vk::image_format("unknown")]]
Sampler2D<uint> denoiser_accumed_tex_prev;
[[vk::binding(8, 1)]] Sampler2D denoiser_motion_tex;
[[vk::binding(11, 1)]]
[[vk::image_format("r11f_g11f_b10f")]]
RWTexture2D<float4> denoiser_spatial_ping_tex;

int2 imageSize()
{
    uint width;
    uint height;
    uint levels;
    denoiser_normal_tex.GetDimensions(0, width, height, levels);
    return int2(width, height);
}

float3 fetchAccumulatedColor(int2 coordinate, int2 size)
{
    if (any(coordinate < int2(0)) || any(coordinate >= size))
    {
        return float3(0.0);
    }
    return unpackRgbe(
        denoiser_accumed_tex_prev.Load(int3(coordinate, 0)).x);
}

bool isConsistent(float3 normal, float3 previousNormal,
                  float3 position, float3 previousPosition,
                  uint voxelId, uint previousVoxelId)
{
    float normalFactor = dot(normal, previousNormal);
    float positionFactor = exp(-distance(position, previousPosition));
    return voxelId == previousVoxelId &&
           normalFactor > 0.9 &&
           positionFactor > temporal_info.temporal_position_phi;
}

[shader("compute")]
[numthreads(DENOISER_GROUP_SIZE, DENOISER_GROUP_SIZE, 1)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    int2 coordinate = int2(dispatchThreadId.xy);
    int2 size = imageSize();
    if (any(coordinate >= size))
    {
        return;
    }
    float4 currentPositionAndHit = denoiser_position_tex[coordinate];
    if (currentPositionAndHit.w == 0.0)
    {
        return;
    }

    float2 motion =
        denoiser_motion_tex.Load(int3(coordinate, 0)).xy * float2(size);
    float2 previousCoordinate = float2(coordinate) + motion;
    float2 baseCoordinateFloat = floor(previousCoordinate);
    float2 fraction = frac(previousCoordinate - baseCoordinateFloat);

    float weights[4];
    weights[0] = (1.0 - fraction.x) * (1.0 - fraction.y);
    weights[1] = fraction.x * (1.0 - fraction.y);
    weights[2] = (1.0 - fraction.x) * fraction.y;
    weights[3] = fraction.x * fraction.y;

    float totalWeight = 0.0;
    float historyLengthSum = 0.0;
    float3 colorSum = float3(0.0);
    float3 currentNormal = unpackNormalV2(
        denoiser_normal_tex.Load(int3(coordinate, 0)).x);
    float3 currentPosition = currentPositionAndHit.xyz;
    uint currentVoxelId =
        denoiser_vox_id_tex.Load(int3(coordinate, 0)).x;

    for (int index = 0; index < 4 && pc.reset_history == 0u; ++index)
    {
        int2 tapCoordinate = int2(baseCoordinateFloat) +
                             TEMPORAL_OFFSETS[index];
        if (any(tapCoordinate < int2(0)) || any(tapCoordinate >= size))
        {
            continue;
        }
        float3 previousNormal = unpackNormalV2(
            denoiser_normal_tex_prev.Load(int3(tapCoordinate, 0)).x);
        // Keeping history length in the copied position texture avoids an
        // in-place history-length image read/write race across workgroups.
        float4 previousPositionAndHistory =
            denoiser_position_tex_prev.Load(int3(tapCoordinate, 0));
        uint previousVoxelId =
            denoiser_vox_id_tex_prev.Load(int3(tapCoordinate, 0)).x;

        if (isConsistent(currentNormal, previousNormal,
                         currentPosition, previousPositionAndHistory.xyz,
                         currentVoxelId, previousVoxelId) &&
            previousPositionAndHistory.w >= 1.0)
        {
            colorSum += weights[index] *
                        fetchAccumulatedColor(tapCoordinate, size);
            totalWeight += weights[index];
            historyLengthSum += weights[index] *
                previousPositionAndHistory.w;
        }
    }

    float3 rawColor = unpackRgbe(
        compute_output_tex.Load(int3(coordinate, 0)).x);
    float historyLength;
    float3 outputColor;
    if (totalWeight >= 1.0e-6)
    {
        historyLengthSum /= totalWeight;
        colorSum /= totalWeight;
        historyLength = min(255.0, historyLengthSum + 1.0);
        float alpha = max(
            temporal_info.temporal_alpha, 1.0 / historyLength);
        outputColor = lerp(colorSum, rawColor, alpha);
    }
    else
    {
        historyLength = 1.0;
        outputColor = rawColor;
    }

    denoiser_spatial_ping_tex.Store(
        coordinate, float4(outputColor, 0.0));
    denoiser_position_tex.Store(
        coordinate, float4(currentPosition, historyLength));
}
