// Exercise the production helper, including near/eye crossings and offscreen boxes.
import stem_proxy_projection;
import tracer_types;

U_CameraInfo testCamera() {
    U_CameraInfo c;
    float n=0.01,f=10.0;
    c.view_proj_mat=float4x4(1,0,0,0, 0,1,0,0,
        0,0,f/(n-f),f*n/(n-f), 0,0,-1,0);
    c.view_mat_inv=float4x4(1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1);
    // Inverse of the perspective above, so the returned ray is also exercised.
    c.proj_mat_inv=float4x4(1,0,0,0,0,1,0,0,0,0,0,-1,
        0,0,(n-f)/(f*n),1/n);
    c.pos=float4(0,0,0,1);
    return c;
}
StemProjection project(uint vertex,float3 lo,float3 hi) {
    return projectStemProxy(vertex,testCamera(),float3(0),1,float2(1,0),
        lo,hi,(lo+hi)*0.5,hi.y-lo.y,45u,false);
}
bool rejected(float3 lo,float3 hi) {
    for(uint v=0u;v<6u;v++)if(any(project(v,lo,hi).position!=float4(2,2,2,1)))return false;
    return true;
}
bool boundsEqual(float3 lo,float3 hi,float2 expectedLo,float2 expectedHi) {
    return all(abs(project(0u,lo,hi).position.xy-expectedLo)<2e-5)
        &&all(abs(project(2u,lo,hi).position.xy-expectedHi)<2e-5);
}
export __extern_cpp int main() {
    // Original minimal reproduction: near crossing, wholly to the right.
    if(!rejected(float3(100,0,-0.015),float3(100.01,0.01,-0.005)))return 1;
    if(!rejected(float3(-100.01,0,-0.015),float3(-100,0.01,-0.005)))return 2;
    if(!rejected(float3(0,100,-0.015),float3(0.01,100.01,-0.005)))return 3;
    if(!rejected(float3(0,-100.01,-0.015),float3(0.01,-100,-0.005)))return 4;
    // Entirely before near, behind eye, beyond far, including touching near.
    if(!rejected(float3(-0.001,-0.001,-0.009),float3(0.001,0.001,-0.001)))return 5;
    if(!rejected(float3(-1,-1,0.001),float3(1,1,2)))return 6;
    if(!rejected(float3(-1,-1,-12),float3(1,1,-11)))return 7;
    // Crossings must retain the near-plane edge intersections, not just corners.
    if(!boundsEqual(float3(-0.001,-0.002,-0.02),float3(0.003,0.004,-0.005),
        float2(-0.1,-0.2),float2(0.3,0.4)))return 8;
    if(!boundsEqual(float3(-0.001,-0.002,-0.02),float3(0.003,0.004,0.02),
        float2(-0.1,-0.2),float2(0.3,0.4)))return 9;
    // A real box around the camera can legitimately cover the entire viewport.
    if(!boundsEqual(float3(-1,-1,-1),float3(1,1,1),float2(-1),float2(1)))return 10;
    if(!boundsEqual(float3(-0.002,-0.004,-0.04),float3(0.006,0.008,-0.02),
        float2(-0.1,-0.2),float2(0.3,0.4)))return 11;

    // Deterministic coverage oracle: dense samples of differently rotated/scaled
    // boxes, with and without the sampling-grid inflation used by both adapters.
    // Every visible point of the conservative bounds must lie in the proxy.
    U_CameraInfo camera=testCamera();
    for(uint scene=0u;scene<64u;scene++)for(uint pixel=0u;pixel<2u;pixel++) {
        float angle=float(scene)*0.37;
        float2 yaw=float2(cos(angle),sin(angle));
        float scale=0.002+float(scene%7u)*0.003;
        float3 root=float3(float(int(scene%5u)-2)*0.013,
            float(int(scene%3u)-1)*0.01,float(int(scene%9u)-5)*0.008);
        float3 lo=float3(-1,-0.7,-0.6),hi=float3(1.5,1.3,0.8);
        StemProjection a=projectStemProxy(0u,camera,root,scale,yaw,lo,hi,(lo+hi)*0.5,2,45u,pixel!=0u);
        StemProjection b=projectStemProxy(2u,camera,root,scale,yaw,lo,hi,(lo+hi)*0.5,2,45u,pixel!=0u);
        float margin=pixel!=0u?4.0/45.0:0.0;
        lo-=margin;hi+=margin;
        for(uint x=0u;x<=8u;x++)for(uint y=0u;y<=8u;y++)for(uint z=0u;z<=8u;z++) {
            float3 local=lerp(lo,hi,float3(x,y,z)/8.0);
            float4 clip=mul(camera.view_proj_mat,float4(stemProjectionWorld(local,root,scale,yaw),1));
            if(clip.w<=0||clip.z<0||clip.z>clip.w||any(abs(clip.xy)>clip.w))continue;
            float2 ndc=clip.xy/clip.w;
            if(any(ndc<a.position.xy-2e-5)||any(ndc>b.position.xy+2e-5))return 12;
            if(any(isnan(a.position))||any(isnan(b.position)))return 13;
        }
    }
    return 0;
}
