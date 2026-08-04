struct FragmentInput
{
    [[vk::location(0)]] float4 color : COLOR0;
    [[vk::location(1)]] float2 uv : TEXCOORD0;
};

[[vk::binding(0, 0)]]
Sampler2D font_sampler;

[shader("fragment")]
float4 main(FragmentInput input) : SV_Target0
{
    return input.color * font_sampler.Sample(input.uv);
}
