// Cache lookup and live reference share format, keys and generator. Display
// still receives fresh per-instance shading; cached data is never scene-lit.
module model_pixel_cache;
import model_pixel_bake;
import tracer_types;
[[vk::binding(0,0)]] ConstantBuffer<U_GuiInput> gui_input;
[[vk::binding(7,1)]] StructuredBuffer<ModelBakedSurface> model_cache_leaves;
[[vk::binding(8,1)]] StructuredBuffer<ModelBakedSurface> model_cache_apples;
[[vk::binding(9,1)]] StructuredBuffer<ModelBakedSurface> model_cache_butterflies;
[[vk::binding(10,1)]] RWStructuredBuffer<uint> model_cache_validation;
[[vk::binding(11,1)]] StructuredBuffer<uint4> model_cache_specs;
ModelBakedSurface cachedSurface(uint kind,uint address) {
    if(kind==0u)return model_cache_leaves[address];
    if(kind==1u)return model_cache_apples[address];
    return model_cache_butterflies[address];
}
public ModelBakedSurface modelSurface(uint source,uint view,uint n,uint2 pixel) {
    uint4 range=model_bake_ranges[source];
    uint kind=range.z,shape=range.w;
    uint views=gui_input.model_pixel_view_count;
    uint4 spec=model_cache_specs[kind];
    bool ready=spec.x==n && spec.y==views && shape<spec.z && spec.w==1u;
    if(gui_input.model_pixel_cache==0u || !ready)
        return bakeModelSurface(source,views,view,n,pixel);
    uint address=((shape*views+view)*n+pixel.y)*n+pixel.x;
    ModelBakedSurface cached=cachedSurface(kind,address);
    // Internal, opt-in GPU oracle. Normal production performs no atomics or
    // geometry traversal. The surface representation must be bit-identical.
    if(gui_input.model_pixel_cache==2u) {
        ModelBakedSurface live=bakeModelSurface(source,views,view,n,pixel);
        bool mismatch=any(asuint(cached.positionDepth)!=asuint(live.positionDepth))
            || any(asuint(cached.normalMaterial)!=asuint(live.normalMaterial));
        InterlockedAdd(model_cache_validation[kind*2u],1u);
        if(mismatch)InterlockedAdd(model_cache_validation[kind*2u+1u],1u);
    }
    return cached;
}
