[[vk::binding(6, 0)]]
Sampler2DArray particle_lod_tex_lut;

struct FragmentInput
{
    [[vk::location(0)]] float4 color : COLOR0;
    [[vk::location(1)]] float2 uv : TEXCOORD0;
    [[vk::location(2)]] nointerpolation uint texture_index : TEXCOORD1;
};

[shader("fragment")]
float4 main(FragmentInput input) : SV_Target0
{
    float4 texel = particle_lod_tex_lut.Sample(
        float3(input.uv, float(input.texture_index)));
    float alpha = texel.a * input.color.a;
    return float4(input.color.rgb * texel.rgb * alpha, alpha);
}
