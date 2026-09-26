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
ModelBakedSurface cachedSurface(uint kind,uint address) {
    if(kind==0u)return model_cache_leaves[address];
    if(kind==1u)return model_cache_apples[address];
    return model_cache_butterflies[address];
}
public ModelBakedSurface modelSurface(uint kind,uint shape,uint view,uint n,uint2 pixel) {
    uint views=gui_input.model_pixel_view_count;
    float4 header=cachedSurface(kind,0).positionDepth;
    bool ready=header.x==float(n) && header.y==float(views) && shape<uint(header.z);
    if(gui_input.model_pixel_cache==0u || !ready)
        return bakeModelSurface(kind,shape,views,view,n,pixel);
    uint address=1u+((shape*views+view)*n+pixel.y)*n+pixel.x;
    ModelBakedSurface cached=cachedSurface(kind,address);
    // Internal, opt-in GPU oracle. Normal production performs no atomics or
    // geometry traversal. Coverage equality is exact; attribute error is bounded.
    if(gui_input.model_pixel_cache==2u) {
        ModelBakedSurface live=bakeModelSurface(kind,shape,views,view,n,pixel);
        bool covered=cached.positionDepth.w<1.0;
        bool mismatch=covered!=(live.positionDepth.w<1.0);
        if(covered) mismatch=mismatch || any(abs(cached.positionDepth-live.positionDepth)>float4(1e-6))
            || any(abs(cached.normalMaterial-live.normalMaterial)>float4(1e-5));
        InterlockedAdd(model_cache_validation[kind*2u],1u);
        if(mismatch)InterlockedAdd(model_cache_validation[kind*2u+1u],1u);
    }
    return cached;
}
