// The only surface lookup seam for model-pixel consumers. No generator imports.
module model_pixel_cache;
import model_cache_types;
import gpu_paged_storage;
import tracer_types;
[[vk::binding(0,0)]] ConstantBuffer<U_GuiInput> gui_input;
struct ModelCacheEntry {
    GpuStorageHandle storage;
    uint resolution; uint views; uint shapes; uint version;
};
[[vk::binding(7,1)]] StructuredBuffer<ModelCacheEntry> model_cache_entries;
[[vk::binding(10,1)]] RWStructuredBuffer<uint> model_cache_validation;
[[vk::binding(87,0)]] StructuredBuffer<uint2> model_cache_sources;
[[vk::binding(88,0)]] public StructuredBuffer<float4> model_cache_palette;
public ModelBakedSurface modelSurface(uint source,uint view,uint n,uint2 pixel) {
    uint2 key=model_cache_sources[source];
    ModelCacheEntry entry=model_cache_entries[key.x];
    uint64_t address=((uint64_t(key.y)*entry.views+view)*n+pixel.y)*n+pixel.x;
    ModelBakedSurface stored=gpuStorageLoad<ModelBakedSurface>(entry.storage,address);
    // Explicit consumption telemetry only. Geometry validation runs once per bake.
    if(gui_input.model_pixel_cache_review!=0u)InterlockedAdd(model_cache_validation[key.x*2u],1u);
    return stored;
}
