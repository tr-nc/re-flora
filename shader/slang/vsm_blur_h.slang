import vsm_filtering;

static const uint VSM_GROUP_SIZE = 8u;

[[vk::push_constant]]
ConstantBuffer<VsmFilterPushConstants> vsm_filter_push;

[[vk::binding(0, 0)]]
[[vk::image_format("r32f")]]
readonly RWTexture2D<float> shadow_map_tex;

[[vk::binding(1, 0)]]
[[vk::image_format("rgba32f")]]
readonly RWTexture2D<float4> shadow_map_tex_for_vsm_ping;

[[vk::binding(2, 0)]]
[[vk::image_format("rgba32f")]]
WTexture2D<float4> shadow_map_tex_for_vsm_pong;

[shader("compute")]
[numthreads(VSM_GROUP_SIZE, VSM_GROUP_SIZE, 1)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    uint width;
    uint height;
    shadow_map_tex_for_vsm_ping.GetDimensions(width, height);
    uint2 coordinate = dispatchThreadId.xy;
    if (any(coordinate >= uint2(width, height)))
    {
        return;
    }

    float4 filtered = filterVsmHorizontal(
        shadow_map_tex_for_vsm_ping, int2(coordinate),
        vsm_filter_push.blur_radius);
    shadow_map_tex_for_vsm_pong.Store(coordinate, filtered);
}
