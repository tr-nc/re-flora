// Stem-only adapter for ordinary grass. Keep the existing sampled voxel-center
// pose, including trimming, rest bend and wind; connect it with the same rounded
// cone primitive used by flowers. No flower head, branch or socket allocation.
module grass_stem_geometry;
import flower_stem_geometry;
public static const uint GRASS_STEM_MAX_POINTS=8u;
public static const float GRASS_STEM_RADIUS=0.5; // old blade is one voxel wide
public void grassStemBounds(float3 points[8],uint count,out float3 lo,out float3 hi) {
    lo=float3(1e20);hi=float3(-1e20);
    for(uint i=0u;i<count;i++) {lo=min(lo,points[i]-GRASS_STEM_RADIUS);hi=max(hi,points[i]+GRASS_STEM_RADIUS);}
}
public StemHit traceGrassStem(float3 points[8],uint count,float3 ro,float3 rd) {
    StemHit hit;hit.distance=1e20;hit.point=float3(0);hit.normal=float3(0,1,0);
    hit.branch=0u;hit.parameter=0.0;
    if(count==0u)return hit;
    if(count==1u) {
        stemRoundCone(hit,ro,rd,points[0],points[0],GRASS_STEM_RADIUS,
            GRASS_STEM_RADIUS,0u,0.0,0.0,float4(0));
    } else for(uint i=0u;i+1u<count;i++) {
        stemRoundCone(hit,ro,rd,points[i],points[i+1u],GRASS_STEM_RADIUS,
            GRASS_STEM_RADIUS,0u,float(i),float(i+1u),float4(0));
    }
    return hit;
}
