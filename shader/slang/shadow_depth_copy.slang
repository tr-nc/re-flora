static const uint SHADOW_GROUP_SIZE = 8u;

[[vk::binding(0, 0)]] Sampler2D shadow_map_depth_tex;

[[vk::binding(1, 0)]]
[[vk::image_format("r32f")]]
WTexture2D<float> shadow_map_tex;

[shader("compute")]
[numthreads(SHADOW_GROUP_SIZE, SHADOW_GROUP_SIZE, 1)]
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

    float depth = shadow_map_depth_tex.Load(int3(coordinate, 0)).r;
    shadow_map_tex.Store(coordinate, depth);
}
