// The sole canonical surface generator: immutable source + view + resolution.
// Startup baking and the diagnostic bit oracle call this same coverage sampler.
module model_pixel_bake;
import model_pixel_bake_projection;
import model_pixel_bake_geometry;
import model_pixel_types;
import model_pixel_projection;
import model_bake_triangle;
import model_cache_types;
import model_pixel_views;
[[vk::binding(18,1)]] public StructuredBuffer<ButterflyMeshTriangle> model_bake_triangles;
[[vk::binding(19,1)]] public StructuredBuffer<uint4> model_bake_ranges;
[[vk::binding(20,1)]] StructuredBuffer<float4> model_bake_frames;
public ModelBakedSurface bakeModelSurface<T : IModelPixelProjectionObserver>(uint source,uint views,uint view,uint n,uint2 pixel,T observer) {
    uint4 range=model_bake_ranges[source]; uint kind=range.z;
    float4 frame=model_bake_frames[source];
    ModelPixelHit hit=sampleOrthographicModelPixel(model_bake_triangles,range.x,range.y,
        identityModelPixelFrame(),canonicalModelView(view,views),frame.xyz,frame.w,pixel,n,observer);
    ModelBakedSurface result=(ModelBakedSurface)0;
    result.positionDepth.w=hit.depth;
    if(hit.triangle==0xffffffffu)return result;
    ButterflyMeshTriangle tri=model_bake_triangles[hit.triangle];
    float3 bary=float3(1.0-hit.bary.x-hit.bary.y,hit.bary);
    result.positionDepth.xyz=tri.a.xyz+tri.e1.xyz*hit.bary.x+tri.e2.xyz*hit.bary.y;
    result.normalMaterial.xyz=kind==2u?normalize(cross(tri.e1.xyz,tri.e2.xyz)):
        kind==3u?tri.normal0.xyz:normalize(tri.normal0.xyz*bary.x+tri.normal1.xyz*bary.y+tri.normal2.xyz*bary.z);
    float2 uv=tri.uv01.xy*bary.x+tri.uv01.zw*bary.y+tri.uv2.xy*bary.z;
    // Flowers store interpolated shared-atlas UVs, not fixed anatomical roles
    // or species colors. Other consumers retain their existing material ABI.
    result.normalMaterial.w=kind==3u?encodeFlowerUV(uv):kind==1u?tri.uv2.z:kind==0u&&uv.x>1.5?1.0:0.0;
    return result;
}

public ModelBakedSurface bakeModelSurface(uint source,uint views,uint view,uint n,uint2 pixel) {
    return bakeModelSurface(source,views,view,n,pixel,NoModelPixelProjectionObserver());
}
