import flower_model;
import flower_stem_geometry;
import flower_stem_blocks;
import flower_stem_experiment;
import model_pixel_projection;
struct StemFragment {
    float4 color:SV_Target0;
    float depth:SV_Depth;
};
[shader("fragment")]
StemFragment main(StemProxyVertex input) {
    float3 displayRay=normalize(input.ray);
    uint mode=flowerStemSamplingSettings().y;
    bool objectGrid=flowerStemShapeSettings().w>0.0;
    float3 sampleRay=mode==1u?stemDirectionCell(displayRay,flowerStemSamplingSettings().z):displayRay;
    float3 sampleOrigin=camera_info.pos.xyz;
    if(objectGrid) {
        float2 cell=stemObjectCell(input.object_uv,uint(flowerStemShapeSettings().w));
        sampleOrigin=stemObjectRayOrigin(input.object_center.xyz,input.object_center.w,
            input.object_x.xyz,input.object_y.xyz,input.object_z.xyz,cell);
        sampleRay=-input.object_z.xyz;
    }
    float3 ro=stemToLocalVector(input,sampleOrigin-input.root_scale.xyz)/input.root_scale.w;
    float3 rd=stemToLocalVector(input,sampleRay);
    StemGeometry s=stemGeometry(input);
    bool blockGeometry=mode==2u&&flowerStemShapeSettings().w<0.0;
    StemHit hit=blockGeometry?traceStemBlocks(s,ro,rd,input.motion_edge.w*-flowerStemShapeSettings().w):traceStem(s,ro,rd);
    // Same transparent/far-depth miss convention as model_pixel_display.
    // The pipeline uses LESS depth: a miss cannot overwrite any scene depth.
    // Avoid discard, which requires an unenabled Vulkan demote capability on
    // the pinned Slang compiler's SPIR-V target.
    StemFragment output;output.color=float4(0);output.depth=1.0;
    if(hit.distance>=1e19)return output;
    float3 world=stemWorldPoint(input,hit.point);
    float3 shadePoint=hit.point,shadeNormal=hit.normal;
    if(blockGeometry)shadePoint=stemCenter(s,hit.branch,hit.parameter);
    else if(mode==2u)stemSurfaceCell(s,hit,input.motion_edge.w*flowerStemShapeSettings().y,shadePoint,shadeNormal);
    float3 color=flowerStemColor(shadePoint.y,s.height);
    output.color=float4(shadeFlower(color,stemWorldPoint(input,shadePoint),stemToWorldVector(input,shadeNormal)),1);
    // A direction texel is a little plane perpendicular to its fixed source
    // ray, through the sampled surface. Intersect that plane with the actual
    // display ray before writing scene depth: never paste RGBA over terrain or
    // petals, and never use a stale offscreen/cubemap depth convention.
    float objectDepth=1.0;
    if(objectGrid) {
        float radius=input.object_center.w;
        float sourceZ=dot(world-input.object_center.xyz,input.object_z.xyz);
        float bakeDepth=(radius-sourceZ)/(2.0*radius);
        // Exact same orthographic object-image depth contract as flower heads.
        objectDepth=modelOrthographicDepth(bakeDepth,input.object_center.xyz,radius,camera_info);
        float cameraZ=mul(camera_info.view_mat,float4(input.object_center.xyz,1)).z+sourceZ;
        float rayZ=mul(camera_info.view_mat,float4(displayRay,0)).z;
        world=camera_info.pos.xyz+displayRay*(cameraZ/min(rayZ,-1e-6));
    } else if(mode==1u) {
        world=stemDirectionDisplayPoint(camera_info.pos.xyz,displayRay,sampleRay,world);
    }
    // Quantized source-ray texels are finite planes: their display footprint
    // can cross the socket even when the sampled point does not. Restrict the
    // actual displayed/depth point too; never paint stem on the outward head
    // side or force its depth behind petals to conceal a geometric mismatch.
    float3 displayedLocal=stemToLocalVector(input,world-input.root_scale.xyz)/input.root_scale.w;
    if(!stemInsideSocket(s,displayedLocal)) { output.color=float4(0);output.depth=1.0;return output; }
    float4 clip=mul(camera_info.view_proj_mat,float4(world,1));
    output.depth=objectGrid?objectDepth:clip.z/clip.w;
    if(clip.w<=0.0||output.depth<0.0||output.depth>1.0) {
        output.color=float4(0);output.depth=1.0;
    }
    return output;
}
