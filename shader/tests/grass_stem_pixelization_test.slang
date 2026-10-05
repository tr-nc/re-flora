import stem_proxy_projection;
import grass_stem_geometry;
import flower_stem_geometry;
import tracer_types;
import flower_stem_model_sampling;
bool close3(float3 a,float3 b){return all(abs(a-b)<1e-6);}
export __extern_cpp int main()
{
    U_CameraInfo camera={};
    float n=0.01,f=10.0;
    camera.view_proj_mat=float4x4(1,0,0,0,0,1,0,0,0,0,f/(n-f),f*n/(n-f),0,0,-1,0);
    camera.view_mat_inv=float4x4(1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1);
    camera.proj_mat_inv=float4x4(1,0,0,0,0,1,0,0,0,0,0,-1,0,0,(n-f)/(f*n),1/n);
    camera.pos=float4(0,0,0,1);
    float3 lo=float3(-0.1,-0.5,-1.1),hi=float3(0.1,0.5,-0.9);
    StemProjection off=projectStemProxy(0u,camera,float3(0),1,float2(1,0),lo,hi,(lo+hi)*0.5,1,32u,false);
    StemProjection on=projectStemProxy(0u,camera,float3(0),1,float2(1,0),lo,hi,(lo+hi)*0.5,1,32u,true);
    if(off.grid.cell!=0.0||on.grid.cell<=0.0)return 1;
    float3 ray=normalize(float3(0.103,0.117,-1));
    float3 plain=stemModelSampleRay(off.grid,camera.pos.xyz,ray);
    if(!close3(plain,ray))return 2;
    float3 sampled=stemModelSampleRay(on.grid,camera.pos.xyz,ray);
    if(close3(sampled,ray))return 3;
    float3 points[8]; for(uint i=0u;i<8u;i++)points[i]=float3(0,float(i),0);
    float3 ro=float3(0,0,3);
    StemHit hit=traceGrassStem(points,2u,ro,plain);
    StemHit reference=traceGrassStem(points,2u,ro,ray);
    if(hit.distance>=1e19||abs(hit.distance-reference.distance)>1e-6)return 4;
    if(!close3(stemModelDisplayPoint(off.grid,ro,ray,hit.point),hit.point))return 5;
    return 0;
}
