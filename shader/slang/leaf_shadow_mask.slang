static const uint LEAF_SHADOW_GROUP_SIZE = 8u;
static const float LEAF_SHADOW_MASK_THRESHOLD = 0.003;

[[vk::binding(0, 0)]] Sampler2D leaf_shadow_opacity_blended_tex;

[[vk::binding(1, 0)]]
[[vk::image_format("rgba8")]]
WTexture2D<float4> leaf_shadow_mask_tex;

[shader("compute")]
[numthreads(LEAF_SHADOW_GROUP_SIZE, LEAF_SHADOW_GROUP_SIZE, 1)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    uint width;
    uint height;
    leaf_shadow_mask_tex.GetDimensions(width, height);
    uint2 coordinate = dispatchThreadId.xy;
    if (any(coordinate >= uint2(width, height)))
    {
        return;
    }

    int2 maskSize = int2(width, height);
    float maximumOpacity = 0.0;
    for (int y = -1; y <= 1; ++y)
    {
        for (int x = -1; x <= 1; ++x)
        {
            int2 cell = clamp(
                int2(coordinate) + int2(x, y),
                int2(0), maskSize - int2(1));
            for (int subY = 0; subY < 2; ++subY)
            {
                for (int subX = 0; subX < 2; ++subX)
                {
                    float2 subcell = float2(
                        0.25 + 0.5 * float(subX),
                        0.25 + 0.5 * float(subY));
                    float2 uv = (float2(cell) + subcell) /
                                float2(maskSize);
                    maximumOpacity = max(
                        maximumOpacity,
                        leaf_shadow_opacity_blended_tex.SampleLevel(uv, 0.0).a);
                }
            }
        }
    }

    float mask = maximumOpacity > LEAF_SHADOW_MASK_THRESHOLD ? 1.0 : 0.0;
    leaf_shadow_mask_tex.Store(
        coordinate, float4(0.0, 0.0, 0.0, mask));
}
