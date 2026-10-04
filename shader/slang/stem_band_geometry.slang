// Shared raster geometry for GPU procedural stems and CPU-authored path bands.
// A band owns one color; topology, deformation and material subdivision are independent.
module stem_band_geometry;
public struct StemBandVertex {
    public float4 position:SV_Position;
    [[vk::location(0)]] public nointerpolation float3 color:COLOR0;
};
public float3 stemBandSide(float3 tangent,float3 preferredSide) {
    float3 side=preferredSide-tangent*dot(preferredSide,tangent);
    if(dot(side,side)<1e-8) {
        float3 reference=abs(tangent.z)<0.9?float3(0,0,1):float3(1,0,0);
        side=cross(tangent,reference);
    }
    return normalize(side);
}
// q is a cube corner in [-.5,.5]. The same eight vertices serve square tubes
// or two crossed ribbons (only the index topology changes). No spherical caps.
public float3 stemBandDirection(float3 delta) {
    return dot(delta,delta)>1e-12?normalize(delta):float3(0,1,0);
}
public float3 stemBandPoint(float3 a,float3 b,float3 tangentA,float3 tangentB,
    float3 sideA,float3 sideB,float2 radiiA,float2 radiiB,float3 q) {
    float t=q.y+0.5;
    float3 tangent=stemBandDirection(lerp(tangentA,tangentB,t));
    float3 x=stemBandSide(tangent,lerp(sideA,sideB,t)),z=cross(x,tangent);
    float2 radii=lerp(radiiA,radiiB,t);
    return lerp(a,b,t)+x*(q.x*2.0*radii.x)+z*(q.z*2.0*radii.y);
}
public float3 stemBandCorner(uint vertex) {
    return float3((vertex&1u)!=0u?0.5:-0.5,(vertex&2u)!=0u?0.5:-0.5,
        (vertex&4u)!=0u?0.5:-0.5);
}
public float stemBandTaper(uint mode,float t) {
    return mode==2u?lerp(1.0,0.12,saturate(t)):1.0;
}
public uint stemColorBand(float restArc,float fullArc,uint bands) {
    return min(uint(floor(saturate(restArc/max(fullArc,1e-7))*float(max(bands,1u)))),max(bands,1u)-1u);
}
