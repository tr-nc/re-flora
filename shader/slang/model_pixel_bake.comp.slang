import model_pixel_bake;
import gpu_paged_storage;
struct BakeSpec { uint kind; uint resolution; uint views; uint shapes; GpuStorageHandle storage; };
[[vk::push_constant]] ConstantBuffer<BakeSpec> spec;
[numthreads(8,8,1)]
[shader("compute")]
void main(uint3 id:SV_DispatchThreadID) {
    if(any(id.xy>=uint2(spec.resolution)) || id.z>=spec.views*spec.shapes)return;
    uint64_t address=(uint64_t(id.z)*spec.resolution+id.y)*spec.resolution+id.x;
    ModelBakedSurface surface=bakeModelSurface(modelSourceIndex(spec.kind,id.z/spec.views),spec.views,
        id.z%spec.views,spec.resolution,id.xy);
    gpuStorageStore<ModelBakedSurface>(spec.storage,address,surface);
}
