import butterfly_mesh_types;
import model_pixel_display;
[[vk::binding(72,0)]] StructuredBuffer<ButterflyMeshInstance> butterfly_mesh_instances;
struct FragmentInput {
    [[vk::location(0)]] float2 uv:TEXCOORD0;
    [[vk::location(1)]] nointerpolation uint instanceIndex:TEXCOORD1;
};
[shader("fragment")]
ModelPixelOutput main(FragmentInput input) {
    ButterflyMeshInstance instance=butterfly_mesh_instances[input.instanceIndex];
    if(instance.repair.w==0u || instance.color.a<=0.0) {
        ModelPixelOutput output;output.color=float4(0.0);output.depth=1.0;return output;
    }
    return displayModelPixel(input.uv,instance.metadata.z,instance.repair.z,instance.color.a,
        input.instanceIndex,instance.position_size.xyz,instance.position_size.w*(1.53125*0.5));
}
