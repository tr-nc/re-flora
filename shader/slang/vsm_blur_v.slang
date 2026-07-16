import vsm_filtering;

static const uint VSM_GROUP_SIZE = 8u;

[[vk::push_constant]]
ConstantBuffer<VsmFilterPushConstants> vsm_filter_push;

[[vk::binding(0, 0)]]
[[vk::image_format("r32f")]]
readonly RWTexture2D<float> shadow_map_tex;

[[vk::binding(1, 0)]]
[[vk::image_format("rgba32f")]]
WTexture2D<float4> shadow_map_tex_for_vsm_ping;

[[vk::binding(2, 0)]]
[[vk::image_format("rgba32f")]]
readonly RWTexture2D<float4> shadow_map_tex_for_vsm_pong;

[[vk::binding(3, 0)]]
[[vk::image_format("rgba32f")]]
readonly RWTexture2D<float4> shadow_map_tex_for_vsm_prev;

[shader("compute")]
[numthreads(VSM_GROUP_SIZE, VSM_GROUP_SIZE, 1)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    uint width;
    uint height;
    shadow_map_tex_for_vsm_pong.GetDimensions(width, height);
    uint2 coordinate = dispatchThreadId.xy;
    if (any(coordinate >= uint2(width, height)))
    {
        return;
    }

    float4 filtered = filterVsmVertical(
        shadow_map_tex_for_vsm_pong, int2(coordinate),
        vsm_filter_push.blur_radius);
    float4 blended = blendVsmTemporalHistory(
        shadow_map_tex_for_vsm_prev, int2(coordinate), filtered,
        vsm_filter_push.temporal_alpha,
        vsm_filter_push.reset_history);
    shadow_map_tex_for_vsm_ping.Store(coordinate, blended);
}
