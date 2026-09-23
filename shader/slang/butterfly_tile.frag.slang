import particle_model_shading;
import butterfly_mesh_types;
[[vk::binding(74,0)]] StructuredBuffer<float4> butterfly_pixel_tiles;
struct FragmentInput {
    [[vk::location(0)]] float2 uv : TEXCOORD0;
    [[vk::location(1)]] nointerpolation uint instanceIndex : TEXCOORD1;
};
struct FragmentOutput {
    float4 color : SV_Target0;
    float depth : SV_Depth;
};
[shader("fragment")]
FragmentOutput main(FragmentInput input) {
    ButterflyMeshInstance instance=butterfly_mesh_instances[input.instanceIndex];
    uint resolution=instance.metadata.z;
    uint2 pixel=min(uint2(saturate(input.uv)*float(resolution)),uint2(resolution-1));
    // Leaves have a virtual N×N buffer, evaluated at snapped pixel centers.
    // No resolution-dependent allocation per falling particle.
    float4 sample;
    if ((instance.metadata.w&2u)!=0u) sample=sampleParticleModel(instance,pixel);
    else sample=butterfly_pixel_tiles[butterflyTileAddress(input.instanceIndex,pixel)];
    FragmentOutput output;
    // Same transparent-mask contract as the existing particle pipeline. Avoid
    // requiring Vulkan shaderDemoteToHelperInvocation for HLSL discard.
    if (sample.w>=1.0 || instance.color.a<=0.0) {
        output.color=float4(0.0); output.depth=1.0; return output;
    }
    output.color=float4(sample.rgb*instance.color.a,instance.color.a);
    output.depth=sample.w;
    return output;
}
