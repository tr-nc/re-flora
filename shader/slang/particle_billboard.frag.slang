// Solid leaf/terrain particles. Butterflies have their own live wing-mesh pipeline.
struct FragmentInput
{
    [[vk::location(0)]] float4 color : COLOR0;
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
    output.color = float4(input.color.rgb * input.color.a, input.color.a);
    output.depth = input.position.z;
    return output;
}
