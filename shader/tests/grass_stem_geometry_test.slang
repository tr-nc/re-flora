import grass_stem_geometry;
import flower_stem_geometry;
import stem_proxy_projection;
import tracer_types;
bool close(float a,float b) {return abs(a-b)<2e-5;}
export __extern_cpp int main() {
    float3 points[8];
    for(uint i=0u;i<8u;i++)points[i]=float3(0.5,float(i)+0.5,0.5);
    if(traceGrassStem(points,0u,float3(0.5,1,2),float3(0,0,-1)).distance<1e19)return 1;
    for(uint count=1u;count<=8u;count++) {
        StemHit side=traceGrassStem(points,count,float3(0.5,0.5,2),float3(0,0,-1));
        if(!close(side.distance,1.0)||any(isnan(side.normal)))return 2;
        if(!close(traceGrassStem(points,count,float3(0.5,-1,0.5),float3(0,1,0)).distance,1.0))return 3;
        if(!close(traceGrassStem(points,count,float3(0.5,9,0.5),float3(0,-1,0)).distance,9.0-float(count)))return 4;
        float3 lo,hi;grassStemBounds(points,count,lo,hi);
        if(any(lo!=float3(0))||any(hi!=float3(1,float(count),1)))return 5;
    }
    for(uint i=0u;i<8u;i++)points[i]+=float3(-0.03,0,0.02)*float(i*i);
    float3 lo,hi;grassStemBounds(points,8u,lo,hi);
    for(uint i=0u;i<8u;i++) {
        if(any(points[i]-0.5<lo)||any(points[i]+0.5>hi))return 6;
        StemHit hit=traceGrassStem(points,8u,points[i]+float3(0,0,2),float3(0,0,-1));
        if(hit.distance>=1e19||any(isnan(hit.normal)))return 7;
    }
    // Zero scale and fully-behind-camera proxies cannot produce visible draws.
    U_CameraInfo camera;
    camera.view_proj_mat=float4x4(1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1);
    camera.proj_mat_inv=camera.view_proj_mat;camera.view_mat_inv=camera.view_proj_mat;
    camera.pos=float4(0,0,-3,1);
    StemProjection projection=projectStemProxy(0u,camera,float3(0),0,float2(1,0),
        lo,hi,(lo+hi)*0.5,8,45u,true);
    if(any(projection.position!=float4(2,2,2,1)))return 8;
    return 0;
}
