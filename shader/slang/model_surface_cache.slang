// Read-only baked canonical surfaces; poses, roll, lighting and depth stay live.
module model_surface_cache;
import model_mesh_view;
import model_mesh_lighting;
import model_pixel_types;
import model_view_quantization;
struct CachedModelSample { float4 position; float4 normal; float4 uv; };
struct CachedModelEntry { uint4 layout; float4 bounds; };
[[vk::binding(20,1)]] StructuredBuffer<CachedModelSample> model_cached_apple_surfaces;
[[vk::binding(21,1)]] StructuredBuffer<CachedModelEntry> model_cached_apple_entries;
[[vk::binding(22,1)]] StructuredBuffer<CachedModelSample> model_cached_butterfly_surfaces;
[[vk::binding(23,1)]] StructuredBuffer<CachedModelEntry> model_cached_butterfly_entries;
[[vk::binding(24,1)]] StructuredBuffer<CachedModelSample> model_cached_flower_surfaces;
[[vk::binding(25,1)]] StructuredBuffer<CachedModelEntry> model_cached_flower_entries;
public struct CachedModelVertex {
    public bool visible; public float4 clip; public float3 position; public float3 normal; public float2 uv; public uint material;
};
public bool cachedModelVertex(uint vertex,uint source,uint kind,ModelPixelFrame physical,float3 pivot,out CachedModelVertex output) {
    output.visible=false;output.clip=float4(2,2,2,1);output.position=float3(0);output.normal=float3(0,1,0);output.uv=float2(0);output.material=0u;
    CachedModelEntry entry;
    if(kind==1u) entry=model_cached_apple_entries[source];
    else if(kind==2u) entry=model_cached_butterfly_entries[source];
    else entry=model_cached_flower_entries[source];
    if(entry.layout.w==0u)return false;
    uint n=entry.layout.y;
    if(vertex>=n*n*6u||physical.scale<=1e-7)return true;
    NativeModelViewBank bank;
    float3 cameraVector=camera_info.pos.xyz-modelViewWorldPoint(physical,pivot);
    uint view=nearestModelView(modelPixelLocalVector(physical,cameraVector),entry.layout.z,bank);
    uint cell=vertex/6u;
    uint address=entry.layout.x+(view*n*n)+cell;
    CachedModelSample sample;
    if(kind==1u) sample=model_cached_apple_surfaces[address];
    else if(kind==2u) sample=model_cached_butterfly_surfaces[address];
    else sample=model_cached_flower_surfaces[address];
    if(sample.position.w==0.0)return true;
    float3 direction=bank.direction(view,entry.layout.z);
    float3 reference=abs(direction.y)>0.99?float3(1,0,0):float3(0,1,0);
    float3 axisX=normalize(cross(reference,direction)),axisY=cross(direction,axisX);
    static const uint corners[6]={0u,1u,2u,2u,1u,3u};
    uint corner=corners[vertex%6u];
    float2 grid=float2(cell%n,cell/n)+float2(corner&1u,corner>>1u);
    float2 coordinate=(grid/float(n)*2.0-1.0)*entry.bounds.w;
    float3 plane=entry.bounds.xyz+axisX*coordinate.x+axisY*coordinate.y;
    ModelPixelFrame rendered=modelMeshViewFrame(physical,pivot);
    output.position=modelViewWorldPoint(rendered,sample.position.xyz);
    output.normal=normalize(modelViewWorldVector(rendered,sample.normal.xyz));
    output.uv=sample.uv.xy;output.material=uint(sample.normal.w+0.5);
    // Put each cell on one common image plane; reconstruct its baked surface
    // depth without changing its N×N projected footprint or joining depth seams.
    output.clip=mul(camera_info.view_proj_mat,float4(modelViewWorldPoint(rendered,plane),1));
    float4 depth=mul(camera_info.view_proj_mat,float4(output.position,1));
    if(output.clip.w<=1e-7||depth.w<=1e-7)return true;
    output.clip.z=(depth.z/depth.w)*output.clip.w;
    output.visible=true;return true;
}
