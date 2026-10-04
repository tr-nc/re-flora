// Shared camera-facing proxy and model-space pixel grid for analytic stems.
// No resources: adapters supply their camera, pose and conservative local bounds.
module stem_proxy_projection;
import flower_stem_model_sampling;
import tracer_types;

public struct StemProjection {
    public float4 position;
    public float3 ray;
    public StemModelGrid grid;
};
public float3 stemProjectionWorld(float3 local,float3 root,float scale,float2 yaw) {
    return root+float3(yaw.x*local.x-yaw.y*local.z,local.y,
        yaw.y*local.x+yaw.x*local.z)*scale;
}
public StemProjection projectStemProxy(uint vertex,U_CameraInfo camera,
    float3 root,float scale,float2 yaw,float3 lo,float3 hi,float3 center,float height,uint resolution,bool pixelized) {
    StemProjection p;
    p.position=float4(2,2,2,1);p.ray=float3(0,0,1);
    p.grid.center=float3(0);p.grid.cell=1.0;
    p.grid.x=float3(1,0,0);p.grid.y=float3(0,1,0);p.grid.normal=float3(0,0,-1);
    if(scale<=1e-7)return p;
    if(pixelized) {
        float radius=length(max(abs(lo-center),abs(hi-center)));
        float3 right=mul(camera.view_mat_inv,float4(1,0,0,0)).xyz;
        p.grid=stemModelGrid(stemProjectionWorld(center,root,scale,yaw),radius*scale,
            height*scale,resolution,camera.pos.xyz,right);
        float margin=2.0*height/float(resolution);
        lo-=margin;hi+=margin;
    }
    float2 ndcLo=float2(1e20),ndcHi=float2(-1e20);
    bool crossesNear=false,anyInFront=false;
    for(uint corner=0u;corner<8u;corner++) {
        float3 local=float3((corner&1u)!=0u?hi.x:lo.x,
            (corner&2u)!=0u?hi.y:lo.y,(corner&4u)!=0u?hi.z:lo.z);
        float4 clip=mul(camera.view_proj_mat,float4(stemProjectionWorld(local,root,scale,yaw),1));
        anyInFront=anyInFront||clip.w>0.0;
        crossesNear=crossesNear||clip.z<=0.0||clip.w<=1e-7;
        if(clip.w>1e-7) {ndcLo=min(ndcLo,clip.xy/clip.w);ndcHi=max(ndcHi,clip.xy/clip.w);}
    }
    if(!anyInFront)return p;
    if(crossesNear) {ndcLo=float2(-1);ndcHi=float2(1);}
    ndcLo=clamp(ndcLo,float2(-1),float2(1));ndcHi=clamp(ndcHi,float2(-1),float2(1));
    uint corners[6]={0u,1u,2u,0u,2u,3u};uint i=corners[vertex];
    float2 uv=float2(i==1u||i==2u?1.0:0.0,i>=2u?1.0:0.0);
    float2 ndc=lerp(ndcLo,ndcHi,uv);p.position=float4(ndc,0,1);
    float4 viewRay=mul(camera.proj_mat_inv,float4(ndc,1,1));
    p.ray=mul(camera.view_mat_inv,float4(viewRay.xyz,0)).xyz;
    return p;
}
