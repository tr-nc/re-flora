import flower_model;
import flower_stem_geometry;
import flower_stem_experiment;
struct StemFragment {
    float4 color:SV_Target0;
    float depth:SV_Depth;
};
[shader("fragment")]
StemFragment main(StemProxyVertex input) {
    float3 displayRay=normalize(input.ray);
    uint mode=flowerStemSamplingSettings().y;
    float3 sampleRay=mode==1u?stemDirectionCell(displayRay,flowerStemSamplingSettings().z):displayRay;
    float3 ro=stemToLocalVector(input,camera_info.pos.xyz-input.root_scale.xyz)/input.root_scale.w;
    float3 rd=stemToLocalVector(input,sampleRay);
    StemGeometry s=stemGeometry(input);
    StemHit hit=traceStem(s,ro,rd);
    // Same transparent/far-depth miss convention as model_pixel_display.
    // The pipeline uses LESS depth: a miss cannot overwrite any scene depth.
    // Avoid discard, which requires an unenabled Vulkan demote capability on
    // the pinned Slang compiler's SPIR-V target.
    StemFragment output;output.color=float4(0);output.depth=1.0;
    if(hit.distance>=1e19)return output;
    float3 world=stemWorldPoint(input,hit.point);
    float3 shadePoint=hit.point,shadeNormal=hit.normal;
    if(mode==2u)stemSurfaceCell(s,hit,input.motion_edge.w*flowerStemShapeSettings().y,shadePoint,shadeNormal);
    float3 color=flowerStemColor(shadePoint.y,s.height);
    output.color=float4(shadeFlower(color,stemWorldPoint(input,shadePoint),stemToWorldVector(input,shadeNormal)),1);
    // A direction texel is a little plane perpendicular to its fixed source
    // ray, through the sampled surface. Intersect that plane with the actual
    // display ray before writing scene depth: never paste RGBA over terrain or
    // petals, and never use a stale offscreen/cubemap depth convention.
    if(mode==1u) {
        float distance=dot(world-camera_info.pos.xyz,sampleRay)/max(dot(displayRay,sampleRay),1e-6);
        world=camera_info.pos.xyz+displayRay*distance;
    }
    float4 clip=mul(camera_info.view_proj_mat,float4(world,1));
    output.depth=clip.z/clip.w;
    if(clip.w<=0.0||output.depth<0.0||output.depth>1.0) {
        output.color=float4(0);output.depth=1.0;
    }
    return output;
}
