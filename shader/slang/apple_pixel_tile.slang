module apple_pixel_tile;
import apple_pixel_frame;
import butterfly_mesh_types;
import color;
import flora_shadow;
import model_pixel_grid;
import model_pixel_surface;
import tracer_types;

[[vk::binding(0,0)]] ConstantBuffer<U_GuiInput> gui_input;
[[vk::binding(1,0)]] ConstantBuffer<U_SunInfo> sun_info;
[[vk::binding(2,0)]] ConstantBuffer<U_ShadingInfo> shading_info;
[[vk::binding(3,0)]] ConstantBuffer<U_CameraInfo> camera_info;
[[vk::binding(4,0)]] ConstantBuffer<U_ShadowCameraInfo> shadow_camera_info;
[[vk::binding(5,0)]] Sampler2D shadow_map_tex_for_vsm_ping;
[[vk::binding(9,0)]] Sampler2D leaf_shadow_opacity_blended_tex;
[[vk::binding(10,0)]] Sampler2D leaf_shadow_mask_tex;
[[vk::binding(11,0)]] Sampler2D cloud_shadow_tex;
[[vk::binding(76,0)]] StructuredBuffer<ButterflyMeshTriangle> apple_pixel_triangles;

public struct ApplePixelOutput {
    float4 color:SV_Target0;
    float depth:SV_Depth;
};
// Closest point to the pixel center in projected space, for a conservatively
// covered cell whose center ray misses the geometry. Barycentrics preserve
// material, smooth normal and depth from the actual contributing triangle.
float3 closestBary(float2 p,float2 a,float2 b,float2 c) {
    float2 ab=b-a,ac=c-a,ap=p-a;
    float area=ab.x*ac.y-ab.y*ac.x;
    if(abs(area)>1e-8) {
        float u=(ap.x*ac.y-ap.y*ac.x)/area;
        float v=(ab.x*ap.y-ab.y*ap.x)/area;
        if(u>=0.0&&v>=0.0&&u+v<=1.0)return float3(1.0-u-v,u,v);
    }
    float best=1e30;float3 result=float3(1.0,0.0,0.0);
    [unroll] for(int edge=0;edge<3;edge++) {
        float2 v0=edge==0?a:edge==1?b:c,v1=edge==0?b:edge==1?c:a;
        float t=saturate(dot(p-v0,v1-v0)/max(dot(v1-v0,v1-v0),1e-12));
        float d=dot(lerp(v0,v1,t)-p,lerp(v0,v1,t)-p);
        if(d<best){best=d;result=edge==0?float3(1.0-t,t,0.0):edge==1?float3(0.0,1.0-t,t):float3(t,0.0,1.0-t);}
    }
    return result;
}
bool triangleOverlapsCell(float2 a,float2 b,float2 c,float2 pixel) {
    float2 center=pixel+0.5;
    if(any(max(max(a,b),c)<pixel)||any(min(min(a,b),c)>pixel+1.0))return false;
    float winding=sign((b.x-a.x)*(c.y-a.y)-(b.y-a.y)*(c.x-a.x));
    if(winding==0.0){
        float2 d=dot(b-a,b-a)>dot(c-a,c-a)?b-a:c-a;
        return abs(d.x*(center.y-a.y)-d.y*(center.x-a.x))<=
            .5*(abs(d.x)+abs(d.y))+1e-5;
    }
    [unroll] for(int edge=0;edge<3;edge++){
        float2 p=edge==0?a:edge==1?b:c,q=edge==0?b:edge==1?c:a;
        float2 d=q-p;
        float signedDistance=winding*(d.x*(center.y-p.y)-d.y*(center.x-p.x));
        if(signedDistance<-.5*(abs(d.x)+abs(d.y))-1e-5)return false;
    }
    return true;
}
struct AppleClipPoint {float4 clip;float3 bary;};
float appleClipDistance(AppleClipPoint point,uint plane) {
    return plane==0u?point.clip.w-1e-7:
        plane==1u?point.clip.z:point.clip.w-point.clip.z;
}
AppleClipPoint appleClipLerp(AppleClipPoint a,AppleClipPoint b,float t) {
    AppleClipPoint point;point.clip=lerp(a.clip,b.clip,t);point.bary=lerp(a.bary,b.bary,t);return point;
}
float2 appleGridPoint(AppleClipPoint point,float4 bounds,uint resolution) {
    return (point.clip.xy/point.clip.w-bounds.xy)/(bounds.zw-bounds.xy)*float(resolution);
}
public ApplePixelOutput sampleApplePixel(ApplePixelVertex input) {
    ApplePixelOutput output;output.color=float4(0.0);output.depth=1.0;
    float4 bounds=applePixelBounds(input,camera_info);
    if(any(bounds.xy>float2(1.0))||any(bounds.zw<float2(-1.0)))return output;
    uint2 pixel=min(uint2(saturate(input.uv)*float(input.resolution)),uint2(input.resolution-1u));
    ModelPixelFrame frame;
    frame.center=input.center;frame.scale=input.scale;
    frame.axisX=input.axisX;frame.axisY=input.axisY;frame.axisZ=input.axisZ;
    uint count,stride;apple_pixel_triangles.GetDimensions(count,stride);
    ModelPixelHit centerHit=modelPixelCenterHit(apple_pixel_triangles,0u,count,frame,
        bounds,float2(pixel)+0.5,input.resolution,camera_info);
    float3 worldDirection=centerHit.direction;
    uint hit=centerHit.triangle;
    float3 bary=float3(1.0-centerHit.bary.x-centerHit.bary.y,centerHit.bary);
    float3 local;
    if(hit!=0xffffffffu){local=modelPixelLocalVector(frame,centerHit.position-input.center)/input.scale;}
    else {
        // Conservative N×N projected coverage, shared in intent with the
        // CPU planner used by butterfly/leaf. Never replace a center hit.
        float bestDepth=1.0;float2 cell=float2(pixel);
        for(uint i=0u;i<count;i++){
            ButterflyMeshTriangle tri=apple_pixel_triangles[i];
            float3 p0=tri.a.xyz,p1=p0+tri.e1.xyz,p2=p0+tri.e2.xyz;
            AppleClipPoint polygon[12];uint length=3u;
            polygon[0].clip=mul(camera_info.view_proj_mat,float4(applePixelWorld(input,p0),1.0));
            polygon[1].clip=mul(camera_info.view_proj_mat,float4(applePixelWorld(input,p1),1.0));
            polygon[2].clip=mul(camera_info.view_proj_mat,float4(applePixelWorld(input,p2),1.0));
            polygon[0].bary=float3(1.0,0.0,0.0);polygon[1].bary=float3(0.0,1.0,0.0);
            polygon[2].bary=float3(0.0,0.0,1.0);
            for(uint plane=0u;plane<3u;plane++){
                AppleClipPoint next[12];uint nextLength=0u;
                for(uint edge=0u;edge<length;edge++){
                    AppleClipPoint a=polygon[edge],b=polygon[(edge+1u)%length];
                    float da=appleClipDistance(a,plane),db=appleClipDistance(b,plane);
                    if(da>=0.0)next[nextLength++]=a;
                    if((da<0.0&&db>0.0)||(da>0.0&&db<0.0))
                        next[nextLength++]=appleClipLerp(a,b,da/(da-db));
                }
                length=nextLength;
                for(uint j=0u;j<length;j++)polygon[j]=next[j];
                if(length==0u)break;
            }
            for(uint j=1u;j+1u<length;j++){
                AppleClipPoint pa=polygon[0],pb=polygon[j],pc=polygon[j+1u];
                float2 a=appleGridPoint(pa,bounds,input.resolution),
                    b=appleGridPoint(pb,bounds,input.resolution),
                    c=appleGridPoint(pc,bounds,input.resolution);
                if(!triangleOverlapsCell(a,b,c,cell))continue;
                float3 weights=closestBary(cell+0.5,a,b,c);
                float3 candidateBary=pa.bary*weights.x+pb.bary*weights.y+pc.bary*weights.z;
                float3 candidate=p0*candidateBary.x+p1*candidateBary.y+p2*candidateBary.z;
                float4 clip=mul(camera_info.view_proj_mat,float4(applePixelWorld(input,candidate),1.0));
                float depth=clip.z/clip.w;
                if(depth>=0.0&&depth<bestDepth){bestDepth=depth;hit=i;bary=candidateBary;local=candidate;}
            }
        }
        if(hit==0xffffffffu)return output;
    }
    ButterflyMeshTriangle tri=apple_pixel_triangles[hit];
    float3 position=applePixelWorld(input,local);
    float4 clip=mul(camera_info.view_proj_mat,float4(position,1.0));
    float depth=clip.z/clip.w;
    if(depth<0.0||depth>=1.0)return output;
    float3 n=normalize(tri.normal0.xyz*bary.x+tri.normal1.xyz*bary.y+tri.normal2.xyz*bary.z);
    float3 normal=normalize(input.axisX*n.x+input.axisY*n.y+input.axisZ*n.z);
    if(dot(normal,worldDirection)>0.0)normal=-normal;
    uint material=uint(tri.uv2.z+0.5);
    float3 srgb=material==0u?float3(203.0,48.0,47.0)/255.0:
        material==1u?float3(116.0,80.0,46.0)/255.0:float3(77.0,145.0,62.0)/255.0;
    float3 base=srgbToLinear(srgb);
    StylizedVoxelShadowSample shadow=sampleStylizedVoxelShadow(gui_input,sun_info,
        shadow_camera_info,shadow_map_tex_for_vsm_ping,
        leaf_shadow_opacity_blended_tex,leaf_shadow_mask_tex,
        cloud_shadow_tex,position,normal);
    float3 color=applyStylizedVoxelLighting(gui_input,sun_info,shading_info,
        base,shadow.weight,position,normal);
    output.color=float4(color,1.0);output.depth=depth;return output;
}
