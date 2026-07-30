[[vk::binding(0, 0)]] [[vk::image_format("r32f")]]
readonly RWTexture2D<float> compute_depth_tex;

struct FragmentInput
{
    float4 position : SV_Position;
};

struct FragmentOutput
{
    float4 color : SV_Target0;
    float depth : SV_Depth;
};

[shader("fragment")]
FragmentOutput main(FragmentInput input)
{
    FragmentOutput output;
    output.color = float4(0.0);
    output.depth = compute_depth_tex[int2(input.position.xy)];
    return output;
}
