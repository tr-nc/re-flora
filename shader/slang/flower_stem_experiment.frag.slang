import flower_model;
import flower_stem_geometry;
import flower_stem_experiment;
import flower_stem_model_sampling;
struct StemFragment {
    float4 color:SV_Target0;
    float depth:SV_Depth;
};
[shader("fragment")]
StemFragment main(StemProxyVertex input) {
    float3 displayRay=normalize(input.ray);
    bool pixelized=flowerStemSamplingSettings().y!=0u;
    StemModelGrid grid=stemProxyGrid(input);
    float3 sampleRay=pixelized?stemModelSampleRay(grid,camera_info.pos.xyz,displayRay):displayRay;
    float3 sampleOrigin=camera_info.pos.xyz;
    float3 ro=stemToLocalVector(input,sampleOrigin-input.root_scale.xyz)/input.root_scale.w;
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
    if(flowerStemSamplingSettings().x!=0u) {
        // motion_edge.w is the base GEOMETRY dimension (also used for radius).
        // Keep the independently tunable shading cell size local to shading.
        float cellSize=flowerStemShapeSettings().y!=0.0
            ?stemWorldCellSize(flowerStemShapeSettings().z,input.root_scale.w)
            :input.motion_edge.w;
        stemSurfaceCell(s,hit,cellSize,shadePoint,shadeNormal);
    }
    float3 color=flowerStemColor(shadePoint.y,s.height);
    output.color=float4(shadeFlower(color,stemWorldPoint(input,shadePoint),stemToWorldVector(input,shadeNormal)),1);
    // Each model texel carries the sampled surface's perspective depth.
    // Intersect its plane with the display ray; never paste RGBA over terrain.
    if(pixelized) {
        world=stemModelDisplayPoint(grid,camera_info.pos.xyz,displayRay,world);
    }
    // Quantized source-ray texels are finite planes: their display footprint
    // can cross the socket even when the sampled point does not. Restrict the
    // actual displayed/depth point too; never paint stem on the outward head
    // side or force its depth behind petals to conceal a geometric mismatch.
    float3 displayedLocal=stemToLocalVector(input,world-input.root_scale.xyz)/input.root_scale.w;
    if(!stemInsideSocket(s,displayedLocal)) { output.color=float4(0);output.depth=1.0;return output; }
    float4 clip=mul(camera_info.view_proj_mat,float4(world,1));
    output.depth=clip.z/clip.w;
    if(clip.w<=0.0||output.depth<0.0||output.depth>1.0) {
        output.color=float4(0);output.depth=1.0;
    }
    return output;
}
