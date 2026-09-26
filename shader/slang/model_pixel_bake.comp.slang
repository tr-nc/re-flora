import model_pixel_bake;
[[vk::binding(7,1)]] RWStructuredBuffer<ModelBakedSurface> model_cache_output;
struct BakeSpec { uint kind; uint resolution; uint views; uint shapes; };
[[vk::push_constant]] ConstantBuffer<BakeSpec> spec;
[numthreads(8,8,1)]
[shader("compute")]
void main(uint3 id:SV_DispatchThreadID) {
    if(any(id.xy>=uint2(spec.resolution)) || id.z>=spec.views*spec.shapes)return;
    if(all(id==uint3(0))) {
        ModelBakedSurface header=(ModelBakedSurface)0;
        header.positionDepth=float4(spec.resolution,spec.views,spec.shapes,1);
        model_cache_output[0]=header;
    }
    uint address=1u+(id.z*spec.resolution+id.y)*spec.resolution+id.x;
    model_cache_output[address]=bakeModelSurface(spec.kind,id.z/spec.views,spec.views,
        id.z%spec.views,spec.resolution,id.xy);
}
