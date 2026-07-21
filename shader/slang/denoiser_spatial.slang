import packer;

static const uint DENOISER_GROUP_SIZE = 8u;
static const float WAVELET_FACTOR = 0.67;

struct SpatialPushConstants
{
    uint iteration;
};

struct U_SpatialInfo
{
    float phi_c;
    float phi_n;
    float phi_p;
    float min_phi_z;
    float max_phi_z;
    float phi_z_stable_sample_count;
    uint is_changing_lum_phi;
    uint is_spatial_denoising_enabled;
};

[[vk::push_constant]]
ConstantBuffer<SpatialPushConstants> pc;

[[vk::binding(0, 0)]]
ConstantBuffer<U_SpatialInfo> spatial_info;

[[vk::binding(0, 1)]] Sampler2D compute_depth_tex;

[[vk::binding(0, 2)]]
[[vk::image_format("unknown")]]
Sampler2D<uint> denoiser_normal_tex;
[[vk::binding(2, 2)]] Sampler2D denoiser_position_tex;
[[vk::binding(4, 2)]]
[[vk::image_format("unknown")]]
Sampler2D<uint> denoiser_vox_id_tex;
[[vk::binding(6, 2)]]
[[vk::image_format("r32ui")]]
RWTexture2D<uint> denoiser_accumed_tex;
[[vk::binding(11, 2)]]
[[vk::image_format("r11f_g11f_b10f")]]
RWTexture2D<float4> denoiser_spatial_ping_tex;
[[vk::binding(12, 2)]]
[[vk::image_format("r11f_g11f_b10f")]]
RWTexture2D<float4> denoiser_spatial_pong_tex;

int2 imageSize()
{
    uint width;
    uint height;
    uint levels;
    denoiser_normal_tex.GetDimensions(0, width, height, levels);
    return int2(width, height);
}

void loadFromPingPong(out float3 normal, out float3 color,
                      out float3 position, int2 coordinate, uint iteration)
{
    normal = unpackNormalV2(
        denoiser_normal_tex.Load(int3(coordinate, 0)).x);
    color = iteration % 2u == 0u
        ? denoiser_spatial_ping_tex[coordinate].rgb
        : denoiser_spatial_pong_tex[coordinate].rgb;
    position = denoiser_position_tex.Load(int3(coordinate, 0)).xyz;
}

void saveToPingPong(int2 coordinate, float3 color, uint iteration)
{
    if (iteration % 2u == 0u)
    {
        denoiser_spatial_pong_tex.Store(
            coordinate, float4(color, 0.0));
    }
    else
    {
        denoiser_spatial_ping_tex.Store(
            coordinate, float4(color, 0.0));
    }
}

float luminance(float3 color)
{
    return dot(color, float3(0.299, 0.587, 0.114));
}

float kernelWeight(int2 offset)
{
    if (all(offset == int2(0)))
    {
        return 1.0;
    }
    if (offset.x == 0 || offset.y == 0)
    {
        return WAVELET_FACTOR;
    }
    return WAVELET_FACTOR * WAVELET_FACTOR;
}

void blurKernel(inout float weightSum, inout float3 colorSum,
                int2 baseCoordinate, int2 offset, uint iteration,
                float3 centerNormal, float3 centerColor,
                float3 centerPosition, float centerLuminance,
                float centerDepth)
{
    int step = 1 << iteration;
    int2 sampleCoordinate = baseCoordinate + offset * step;
    float waveletWeight = kernelWeight(offset);

    if (all(offset == int2(0)))
    {
        weightSum += waveletWeight;
        colorSum += waveletWeight * centerColor;
        return;
    }

    int2 size = imageSize();
    if (any(sampleCoordinate < int2(0)) ||
        any(sampleCoordinate >= size) ||
        denoiser_position_tex.Load(int3(sampleCoordinate, 0)).w == 0.0)
    {
        return;
    }

    float3 sampleNormal;
    float3 sampleColor;
    float3 samplePosition;
    loadFromPingPong(sampleNormal, sampleColor, samplePosition,
                     sampleCoordinate, iteration);

    float colorPhi = spatial_info.phi_c;
    if (spatial_info.is_changing_lum_phi != 0u)
    {
        colorPhi *= pow(2.0, -float(iteration));
    }
    float colorWeight = exp(
        -abs(luminance(sampleColor) - centerLuminance) / colorPhi);
    float normalWeight = max(
        0.0, pow(dot(centerNormal, sampleNormal), spatial_info.phi_n));
    float positionWeight = exp(
        -distance(samplePosition, centerPosition) / spatial_info.phi_p);

    float depthFalloff = exp(-centerDepth);
    float depthWeight = 1.0;
    if (depthFalloff > spatial_info.min_phi_z)
    {
        bool sameVoxel =
            denoiser_vox_id_tex.Load(int3(baseCoordinate, 0)).x ==
            denoiser_vox_id_tex.Load(int3(sampleCoordinate, 0)).x;
        if (!sameVoxel)
        {
            float historyLength = float(
                denoiser_position_tex.Load(
                    int3(baseCoordinate, 0)).w);
            float distanceWeight = smoothstep(
                spatial_info.min_phi_z,
                max(spatial_info.min_phi_z, spatial_info.max_phi_z),
                depthFalloff);
            distanceWeight *= smoothstep(
                0.0,
                spatial_info.phi_z_stable_sample_count * 256.0,
                historyLength);
            depthWeight = 1.0 - distanceWeight;
        }
    }

    float weight = waveletWeight * colorWeight * normalWeight *
                   positionWeight * depthWeight;
    weightSum += weight;
    colorSum += weight * sampleColor;
}

[shader("compute")]
[numthreads(DENOISER_GROUP_SIZE, DENOISER_GROUP_SIZE, 1)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    int2 coordinate = int2(dispatchThreadId.xy);
    if (any(coordinate >= imageSize()) ||
        denoiser_position_tex.Load(int3(coordinate, 0)).w == 0.0)
    {
        return;
    }

    uint iteration = pc.iteration;
    if (spatial_info.is_spatial_denoising_enabled == 0u)
    {
        if (iteration != 0u)
        {
            return;
        }
        float3 color = denoiser_spatial_ping_tex[coordinate].rgb;
        denoiser_spatial_pong_tex.Store(
            coordinate, float4(color, 0.0));
        denoiser_accumed_tex.Store(coordinate, packRgbe(color));
        return;
    }

    float3 centerNormal;
    float3 centerColor;
    float3 centerPosition;
    loadFromPingPong(centerNormal, centerColor, centerPosition,
                     coordinate, iteration);
    float centerLuminance = luminance(centerColor);
    float centerDepth = compute_depth_tex.Load(int3(coordinate, 0)).x;

    float weightSum = 0.0;
    float3 colorSum = float3(0.0);
    for (int y = -1; y <= 1; ++y)
    {
        for (int x = -1; x <= 1; ++x)
        {
            blurKernel(weightSum, colorSum, coordinate, int2(x, y),
                       iteration, centerNormal, centerColor,
                       centerPosition, centerLuminance, centerDepth);
        }
    }

    float3 outputColor = colorSum / max(weightSum, 1.0e-6);
    saveToPingPong(coordinate, outputColor, iteration);
    if (iteration == 0u)
    {
        denoiser_accumed_tex.Store(
            coordinate, packRgbe(outputColor));
    }
}
