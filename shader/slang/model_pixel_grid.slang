// Fixed model-space tile geometry shared by butterfly, falling leaf and apple.
// The tile is framed from the published model size, never fitted to the pose.
module model_pixel_grid;
import tracer_types;

public float4 modelPixelBounds(float3 position, float radius, U_CameraInfo camera) {
    precise float3 center=mul(camera.view_mat,float4(position,1.0)).xyz;
    float4 nearH=mul(camera.proj_mat_inv,float4(0.0,0.0,0.0,1.0));
    float nearZ=nearH.z/nearH.w;
    float farZ=center.z-radius, frontZ=min(center.z+radius,nearZ);
    if(farZ>=nearZ) return float4(2.0,2.0,3.0,3.0);
    float2 lo=float2(1e20),hi=float2(-1e20);
    [unroll] for(uint i=0;i<8;++i) {
        float2 xy=center.xy+(float2(i&1u,(i>>1u)&1u)*2.0-1.0)*radius;
        precise float4 clip=mul(camera.proj_mat,float4(xy,(i&4u)!=0u?frontZ:farZ,1.0));
        float2 ndc=clip.xy/clip.w;lo=min(lo,ndc);hi=max(hi,ndc);
    }
    return float4(lo,hi);
}
public float2 modelPixelNdc(float4 bounds,float2 pixel,uint resolution) {
    return lerp(bounds.xy,bounds.zw,pixel/float(resolution));
}
public void modelPixelRay(float4 bounds,float2 pixel,uint resolution,U_CameraInfo camera,
    out float3 origin,out float3 direction) {
    float2 ndc=modelPixelNdc(bounds,pixel,resolution);
    float4 nearH=mul(camera.view_proj_mat_inv,float4(ndc,0.0,1.0));
    float4 farH=mul(camera.view_proj_mat_inv,float4(ndc,1.0,1.0));
    origin=nearH.xyz/nearH.w;
    direction=normalize(farH.xyz/farH.w-origin);
}
public bool modelPixelTriangleHit(float3 origin,float3 direction,float3 a,float3 e1,float3 e2,
    out float t,out float2 bary) {
    float3 p=cross(direction,e2),s=origin-a;
    float det=dot(e1,p);t=0.0;bary=0.0;
    if(abs(det)<1e-12)return false;
    float inv=1.0/det,u=dot(s,p)*inv;
    float3 q=cross(s,e1);float v=dot(direction,q)*inv;
    t=dot(e2,q)*inv;bary=float2(u,v);
    return u>=-1e-6 && v>=-1e-6 && u+v<=1.000001 && t>0.0;
}
