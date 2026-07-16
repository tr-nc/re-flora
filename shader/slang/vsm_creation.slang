import vsm;

static const uint VSM_GROUP_SIZE = 8u;

[[vk::binding(0, 0)]]
[[vk::image_format("r32f")]]
readonly RWTexture2D<float> shadow_map_tex;

[[vk::binding(1, 0)]]
[[vk::image_format("rgba32f")]]
WTexture2D<float4> shadow_map_tex_for_vsm_ping;

[[vk::binding(2, 0)]]
[[vk::image_format("rgba32f")]]
WTexture2D<float4> shadow_map_tex_for_vsm_pong;

float4 convertToEvsm(float depth)
{
    float2 evsmDepth = warpDepth(depth, evsmExponents());
    return float4(evsmDepth, evsmDepth * evsmDepth);
}

[shader("compute")]
[numthreads(VSM_GROUP_SIZE, VSM_GROUP_SIZE, 1)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    uint width;
    uint height;
    shadow_map_tex.GetDimensions(width, height);
    uint2 coordinate = dispatchThreadId.xy;
    if (any(coordinate >= uint2(width, height)))
    {
        return;
    }

    float depth = shadow_map_tex[coordinate];
    shadow_map_tex_for_vsm_ping.Store(
        coordinate, convertToEvsm(depth));
}
