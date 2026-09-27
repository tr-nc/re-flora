// Production only reads prepared surfaces. No live path or memory fallback.
module model_pixel_cache;
import model_pixel_bake;
import gpu_paged_storage;
import tracer_types;
[[vk::binding(0,0)]] ConstantBuffer<U_GuiInput> gui_input;
struct ModelCacheEntry {
    GpuStorageHandle storage;
    uint resolution; uint views; uint shapes; uint version;
};
[[vk::binding(7,1)]] StructuredBuffer<ModelCacheEntry> model_cache_entries;
[[vk::binding(10,1)]] RWStructuredBuffer<uint> model_cache_validation;
public ModelBakedSurface modelSurface(uint source,uint view,uint n,uint2 pixel) {
    uint4 range=model_bake_ranges[source];
    uint kind=range.z,shape=range.w;
    ModelCacheEntry entry=model_cache_entries[kind];
    uint64_t address=((uint64_t(shape)*entry.views+view)*n+pixel.y)*n+pixel.x;
    ModelBakedSurface stored=gpuStorageLoad<ModelBakedSurface>(entry.storage,address);
    // Test instrumentation only: never used as a rendering source or fallback.
    // The sole generator is invoked here to verify stored bits, not to display it.
    if(gui_input.model_pixel_cache_verify!=0u) {
        ModelBakedSurface expected=bakeModelSurface(source,entry.views,view,n,pixel);
        bool mismatch=any(asuint(stored.positionDepth)!=asuint(expected.positionDepth))
            || any(asuint(stored.normalMaterial)!=asuint(expected.normalMaterial));
        InterlockedAdd(model_cache_validation[kind*2u],1u);
        if(mismatch)InterlockedAdd(model_cache_validation[kind*2u+1u],1u);
    }
    return stored;
}
