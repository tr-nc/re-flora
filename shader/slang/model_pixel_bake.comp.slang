import model_pixel_bake;
import model_pixel_bake_geometry;
import model_cache_types;
import gpu_paged_storage;
struct BakeSpec { uint kind; uint resolution; uint views; uint shapes; GpuStorageHandle storage; uint verify; uint evidence_stride; };
[[vk::binding(10,1)]] RWStructuredBuffer<uint> model_cache_validation;
[[vk::binding(14,1)]] RWStructuredBuffer<float4> model_bake_evidence;
public struct BakeObserver : IModelPixelProjectionObserver {
    uint base,first,cell; bool capture;
    public bool enabled() { return capture; }
    public void center(ModelPixelHit hit) {
        model_bake_evidence[cell+2u]=float4(hit.depth,asfloat(hit.triangle),0,0);
        model_bake_evidence[cell+3u]=float4(hit.localOrigin,0);
        model_bake_evidence[cell+4u]=float4(hit.localDirection,0);
    }
    public void surface(ModelPixelHit hit) { model_bake_evidence[cell+2u].w=asfloat(hit.triangle); }
    public void triangle(uint source,uint vertices,float4 bounds,uint resolution) {
        if(source==first)model_bake_evidence[base]=bounds;
        model_bake_evidence[base+1u+(source-first)*13u]=float4(vertices,asfloat(source),resolution,0);
    }
    public void fan(uint source,uint index,float3 a,float3 b,float3 c) {
        uint at=base+2u+(source-first)*13u;
        model_bake_evidence[at]=float4(a,0);
        model_bake_evidence[at+index]=float4(b,0);
        model_bake_evidence[at+index+1u]=float4(c,0);
    }
};
[[vk::push_constant]] ConstantBuffer<BakeSpec> spec;
[numthreads(8,8,1)]
[shader("compute")]
void main(uint3 id:SV_DispatchThreadID) {
    if(any(id.xy>=uint2(spec.resolution)) || id.z>=spec.views*spec.shapes)return;
    if(spec.verify==2u) {
        if(id.z>=spec.shapes*2u)return;
        uint shape=id.z/2u,view=(id.z%2u)==0u?0u:spec.views-1u;
        uint source=modelSourceIndex(spec.kind,shape);
        uint n=spec.resolution,base=id.z*spec.evidence_stride,cell=id.y*n+id.x;
        uint64_t address=((uint64_t(shape)*spec.views+view)*n+id.y)*n+id.x;
        ModelBakedSurface stored=gpuStorageLoad<ModelBakedSurface>(spec.storage,address);
        BakeObserver observer; observer.cell=base+cell*5u;observer.base=base+n*n*5u;observer.first=model_bake_ranges[source].x;observer.capture=all(id.xy==uint2(0));
        ModelBakedSurface observed=bakeModelSurface(source,spec.views,view,n,id.xy,observer);
        // Keep observer's sampling result observable as well as its projection writes.
        model_bake_evidence[base+cell*5u]=stored.positionDepth;
        model_bake_evidence[base+cell*5u+1u]=stored.normalMaterial;
        model_bake_evidence[base+cell*5u+2u].z=observed.positionDepth.w;
        return;
    }
    uint64_t address=(uint64_t(id.z)*spec.resolution+id.y)*spec.resolution+id.x;
    ModelBakedSurface surface=bakeModelSurface(modelSourceIndex(spec.kind,id.z/spec.views),spec.views,
        id.z%spec.views,spec.resolution,id.xy);
    if(spec.verify==0u)gpuStorageStore<ModelBakedSurface>(spec.storage,address,surface);
    else {
        ModelBakedSurface stored=gpuStorageLoad<ModelBakedSurface>(spec.storage,address);
        bool mismatch=any(asuint(stored.positionDepth)!=asuint(surface.positionDepth))
            ||any(asuint(stored.normalMaterial)!=asuint(surface.normalMaterial));
        InterlockedAdd(model_cache_validation[8u+spec.kind*2u],1u);
        if(mismatch)InterlockedAdd(model_cache_validation[9u+spec.kind*2u],1u);
    }
}
