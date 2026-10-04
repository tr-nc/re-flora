module cpu_stem_band_input;
import stem_band_geometry;
public struct CpuStemBandInput {
    [[vk::location(0)]] public uint vertex:TEXCOORD0;
    [[vk::location(1)]] public float4 a_radius:TEXCOORD1;
    [[vk::location(2)]] public float4 b_radius:TEXCOORD2;
    [[vk::location(3)]] public float3 tangent_a:TEXCOORD3;
    [[vk::location(4)]] public float3 tangent_b:TEXCOORD4;
    [[vk::location(5)]] public float3 side_a:TEXCOORD5;
    [[vk::location(6)]] public float3 side_b:TEXCOORD6;
    [[vk::location(7)]] public float3 color_srgb:COLOR0;
    [[vk::location(8)]] public float3 shading_position:TEXCOORD7;
    [[vk::location(9)]] public float3 shading_normal:TEXCOORD8;
};
public float3 cpuStemBandPosition(CpuStemBandInput input) {
    return stemBandPoint(input.a_radius.xyz,input.b_radius.xyz,
        input.tangent_a,input.tangent_b,input.side_a,input.side_b,
        float2(input.a_radius.w),float2(input.b_radius.w),stemBandCorner(input.vertex));
}
