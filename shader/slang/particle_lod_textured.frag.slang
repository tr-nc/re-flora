[[vk::binding(6, 0)]]
Sampler2DArray particle_lod_tex_lut;

struct FragmentInput
{
    [[vk::location(0)]] float4 color : COLOR0;
    [[vk::location(1)]] float2 uv : TEXCOORD0;
    [[vk::location(2)]] nointerpolation uint texture_index : TEXCOORD1;
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
    float4 texel = particle_lod_tex_lut.Sample(
        float3(input.uv, float(input.texture_index)));
    float alphaMask = step(0.5, texel.a);
    float maskedAlpha = texel.a * alphaMask * input.color.a;

    FragmentOutput output;
    output.color = float4(
        input.color.rgb * texel.rgb * maskedAlpha,
        maskedAlpha);
    output.depth = lerp(1.0, input.position.z, alphaMask);
    return output;
}
