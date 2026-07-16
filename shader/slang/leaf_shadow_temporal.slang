static const uint LEAF_SHADOW_GROUP_SIZE = 8u;

struct LeafShadowTemporalPushConstants
{
    float temporal_alpha;
    uint reset_history;
};

[[vk::push_constant]]
ConstantBuffer<LeafShadowTemporalPushConstants> leaf_shadow_temporal_push;

[[vk::binding(0, 0)]] Sampler2D leaf_shadow_opacity_tex;
[[vk::binding(1, 0)]] Sampler2D leaf_shadow_opacity_prev_tex;

[[vk::binding(2, 0)]]
[[vk::image_format("rgba8")]]
WTexture2D<float4> leaf_shadow_opacity_blended_tex;

[shader("compute")]
[numthreads(LEAF_SHADOW_GROUP_SIZE, LEAF_SHADOW_GROUP_SIZE, 1)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    uint width;
    uint height;
    leaf_shadow_opacity_blended_tex.GetDimensions(width, height);
    uint2 coordinate = dispatchThreadId.xy;
    if (any(coordinate >= uint2(width, height)))
    {
        return;
    }

    float4 currentSample =
        leaf_shadow_opacity_tex.Load(int3(coordinate, 0));
    float4 previousSample =
        leaf_shadow_opacity_prev_tex.Load(int3(coordinate, 0));
    float alpha = leaf_shadow_temporal_push.reset_history != 0u
        ? 1.0
        : clamp(leaf_shadow_temporal_push.temporal_alpha, 0.0, 1.0);

    float blendedDepthOpacity = lerp(previousSample.r, currentSample.r, alpha);
    float blendedOpacity = lerp(previousSample.a, currentSample.a, alpha);
    leaf_shadow_opacity_blended_tex.Store(
        coordinate,
        float4(blendedDepthOpacity, 0.0, 0.0, blendedOpacity));
}
