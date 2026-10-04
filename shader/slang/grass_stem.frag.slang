import grass_stem_geometry;
import flower_stem_geometry;
import flower_stem_model_sampling;
import grass_stem_proxy;
import tracer_types;
[[vk::binding(3,0)]] ConstantBuffer<U_CameraInfo> camera_info;
struct GrassStemFragment { float4 color:SV_Target0; float depth:SV_Depth; };
[shader("fragment")]
GrassStemFragment main(GrassStemProxyVertex input) {
    GrassStemFragment output;output.color=float4(0);output.depth=1.0;
    uint count=uint(input.root_count.w);
    if(count==0u)return output;
    float3 displayRay=normalize(input.ray);
    StemModelGrid grid=grassStemGrid(input);
    float3 sampleRay=stemModelSampleRay(grid,camera_info.pos.xyz,displayRay);
    float3 ro=(camera_info.pos.xyz-input.root_count.xyz)*256.0;
    StemHit hit=traceGrassStem(input.points,count,ro,sampleRay);
    if(hit.distance>=1e19)return output;
    // Same block-colored grass palette and lighting cache as the baseline.
    uint layer=min(uint(floor(hit.parameter+0.5)),count-1u);
    float3 world=input.root_count.xyz+hit.point/256.0;
    world=stemModelDisplayPoint(grid,camera_info.pos.xyz,displayRay,world);
    float4 clip=mul(camera_info.view_proj_mat,float4(world,1));
    float depth=clip.z/clip.w;
    if(clip.w<=0.0||depth<0.0||depth>1.0)return output;
    output.color=float4(input.colors[layer],1);output.depth=depth;
    return output;
}
