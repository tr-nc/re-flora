// Model-independent geometry sampling. Adapters supply only a mesh range and
// a rigid frame; materials, storage and scheduling do not own intersection rules.
module model_pixel_surface;
import model_pixel_grid;
import butterfly_mesh_types;
import tracer_types;

public struct ModelPixelFrame {
    public float3 center;
    public float3 axisX;
    public float3 axisY;
    public float3 axisZ;
    public float scale;
};
public ModelPixelFrame identityModelPixelFrame() {
    ModelPixelFrame f;
    f.center=float3(0.0); f.axisX=float3(1,0,0);
    f.axisY=float3(0,1,0); f.axisZ=float3(0,0,1); f.scale=1.0;
    return f;
}
public float3 modelPixelWorld(ModelPixelFrame f,float3 p) {
    return f.center+f.scale*(f.axisX*p.x+f.axisY*p.y+f.axisZ*p.z);
}
public float3 modelPixelLocalVector(ModelPixelFrame f,float3 v) {
    return float3(dot(v,f.axisX),dot(v,f.axisY),dot(v,f.axisZ));
}
public struct ModelPixelHit {
    public uint triangle;
    public float2 bary;
    public float3 position;
    public float3 direction;
    public float depth;
};
// Conservative cells use the nearest projected triangle depth and its real
// material surface. Original center hits always win. This is the final output
// of the CPU eight-neighbour + coverage planner: coverage supersedes its bridge
// expressions, including their color interpolation (see equivalence tests).
struct PixelClipPoint { float4 clip; };
float pixelClipDistance(float4 p,uint plane) {
    return plane==0u?p.w-1e-7:plane==1u?p.z:p.w-p.z;
}
float3 pixelClosestBary(float2 p,float2 a,float2 b,float2 c) {
    float2 ab=b-a,ac=c-a,ap=p-a;
    float area=ab.x*ac.y-ab.y*ac.x;
    if(abs(area)>1e-10) {
        float u=(ap.x*ac.y-ap.y*ac.x)/area;
        float v=(ab.x*ap.y-ab.y*ap.x)/area;
        if(u>=0.0&&v>=0.0&&u+v<=1.0)return float3(1.0-u-v,u,v);
    }
    float best=1e30;float3 result=float3(1,0,0);
    [unroll] for(uint edge=0u;edge<3u;edge++) {
        float2 v0=edge==0u?a:edge==1u?b:c,v1=edge==0u?b:edge==1u?c:a;
        float t=saturate(dot(p-v0,v1-v0)/max(dot(v1-v0,v1-v0),1e-20));
        float2 delta=lerp(v0,v1,t)-p;float d=dot(delta,delta);
        if(d<best){best=d;result=edge==0u?float3(1-t,t,0):edge==1u?float3(0,1-t,t):float3(t,0,1-t);}
    }
    return result;
}
bool pixelTriangleCovers(float2 a,float2 b,float2 c,float2 pixel) {
    const float epsilon=0.0001;
    if(any(max(max(a,b),c)<pixel-epsilon)||any(min(min(a,b),c)>pixel+1.0+epsilon))return false;
    float winding=((b.x-a.x)*(c.y-a.y)-(b.y-a.y)*(c.x-a.x))>=0.0?1.0:-1.0;
    float2 center=pixel+0.5;
    [unroll] for(uint edge=0u;edge<3u;edge++) {
        float2 p=edge==0u?a:edge==1u?b:c,q=edge==0u?b:edge==1u?c:a;
        float2 d=q-p;
        if(winding*(d.x*(center.y-p.y)-d.y*(center.x-p.x))<
            -0.5*(abs(d.x)+abs(d.y))-epsilon)return false;
    }
    return true;
}
public ModelPixelHit modelPixelCoverageHit(StructuredBuffer<ButterflyMeshTriangle> triangles,
    uint first,uint count,ModelPixelFrame frame,float4 bounds,uint2 pixel,uint resolution,
    U_CameraInfo camera,ModelPixelHit hit) {
    if(hit.triangle!=0xffffffffu || any(bounds.xy>float2(1.0)) || any(bounds.zw<float2(-1.0)))return hit;
    for(uint i=0u;i<count;i++) {
        ButterflyMeshTriangle tri=triangles[first+i];
        float3 p0=tri.a.xyz,p1=p0+tri.e1.xyz,p2=p0+tri.e2.xyz;
        float4 polygon[12];uint length=3u;
        polygon[0]=mul(camera.view_proj_mat,float4(modelPixelWorld(frame,p0),1));
        polygon[1]=mul(camera.view_proj_mat,float4(modelPixelWorld(frame,p1),1));
        polygon[2]=mul(camera.view_proj_mat,float4(modelPixelWorld(frame,p2),1));
        for(uint plane=0u;plane<3u;plane++) {
            bool inside=true;
            for(uint j=0u;j<length;j++)inside=inside&&pixelClipDistance(polygon[j],plane)>=0.0;
            if(inside)continue;
            float4 next[12];uint nextLength=0u;
            for(uint edge=0u;edge<length;edge++) {
                float4 a=polygon[edge],b=polygon[(edge+1u)%length];
                float da=pixelClipDistance(a,plane),db=pixelClipDistance(b,plane);
                if(da>=0.0)next[nextLength++]=a;
                if((da>=0.0)!=(db>=0.0))next[nextLength++]=lerp(a,b,da/(da-db));
            }
            length=nextLength;
            for(uint j=0u;j<length;j++)polygon[j]=next[j];
            if(length<3u)break;
        }
        for(uint j=1u;j+1u<length;j++) {
            float3 pa=polygon[0].xyz/polygon[0].w;
            float3 pb=polygon[j].xyz/polygon[j].w;
            float3 pc=polygon[j+1u].xyz/polygon[j+1u].w;
            float2 a=(pa.xy-bounds.xy)/(bounds.zw-bounds.xy)*float(resolution);
            float2 b=(pb.xy-bounds.xy)/(bounds.zw-bounds.xy)*float(resolution);
            float2 c=(pc.xy-bounds.xy)/(bounds.zw-bounds.xy)*float(resolution);
            if(!pixelTriangleCovers(a,b,c,float2(pixel)))continue;
            float3 weights=pixelClosestBary(float2(pixel)+0.5,a,b,c);
            float depth=dot(weights,saturate(float3(pa.z,pb.z,pc.z)));
            if(depth>=hit.depth)continue;
            hit.triangle=first+i;hit.bary=float2(1.0/3.0);
            hit.position=modelPixelWorld(frame,(p0+p1+p2)/3.0);
            hit.depth=depth;
        }
    }
    return hit;
}

public ModelPixelHit modelPixelCenterHit(StructuredBuffer<ButterflyMeshTriangle> triangles,
    uint first,uint count,ModelPixelFrame frame,float4 bounds,float2 pixel,uint resolution,
    U_CameraInfo camera) {
    ModelPixelHit hit;
    hit.triangle=0xffffffffu; hit.bary=float2(0.0); hit.position=float3(0.0);
    hit.direction=float3(0.0); hit.depth=1.0;
    if(any(bounds.xy>float2(1.0))||any(bounds.zw<float2(-1.0)))return hit;
    float3 worldOrigin,worldDirection;
    modelPixelRay(bounds,pixel,resolution,camera,worldOrigin,worldDirection);
    hit.direction=worldDirection;
    float3 origin=modelPixelLocalVector(frame,worldOrigin-frame.center)/frame.scale;
    float3 direction=modelPixelLocalVector(frame,worldDirection);
    float closest=1e30;
    for(uint i=0u;i<count;i++) {
        ButterflyMeshTriangle tri=triangles[first+i];float distance;float2 bary;
        if(!modelPixelTriangleHit(origin,direction,tri.a.xyz,tri.e1.xyz,tri.e2.xyz,distance,bary)
            ||distance>=closest)continue;
        float3 position=worldOrigin+worldDirection*distance*frame.scale;
        float4 clip=mul(camera.view_proj_mat,float4(position,1.0));
        float depth=clip.z/clip.w;
        if(depth<0.0||depth>=1.0)continue;
        closest=distance;hit.triangle=first+i;hit.bary=bary;
        hit.position=position;hit.depth=depth;
    }
    return hit;
}
