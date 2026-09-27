// Conservative geometry sampler: bake and standalone diagnostics only.
module model_pixel_bake_geometry;
import model_pixel_types;
import model_bake_triangle;
import tracer_types;
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

public struct ModelPixelHit {
    public uint triangle;
    public float2 bary;
    public float3 position;
    public float3 direction;
    public float depth;
    public float3 localOrigin;
    public float3 localDirection;
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
public bool pixelTriangleCovers(float2 a,float2 b,float2 c,float2 pixel) {
    const float epsilon=0.0001;
    if(any(max(max(a,b),c)<pixel-epsilon)||any(min(min(a,b),c)>pixel+1.0+epsilon))return false;
    float winding=((b.x-a.x)*(c.y-a.y)-(b.y-a.y)*(c.x-a.x))>=0.0?1.0:-1.0;
    float2 center=pixel+0.5;
    [unroll] for(uint edge=0u;edge<3u;edge++) {
        float2 p=edge==0u?a:edge==1u?b:c,q=edge==0u?b:edge==1u?c:a;
        float2 d=q-p;
        if(winding*(d.x*(center.y-p.y)-d.y*(center.x-p.x))<
            -(0.5+epsilon)*(abs(d.x)+abs(d.y)))return false;
    }
    return true;
}
// The native oracle observes these vertices, not this module's coverage decision.
// Keep clipping/projection at the same seam as the real tile producer.
public struct ModelPixelPolygon {
    public float3 vertices[12];
    public uint count;
};
struct ModelPixelClipPolygon {
    float4 vertices[12];
    uint count;
};
ModelPixelClipPolygon clipModelPixelTriangle(ButterflyMeshTriangle tri,
    ModelPixelFrame frame,U_CameraInfo camera) {
    ModelPixelClipPolygon result;
    float3 p0=tri.a.xyz,p1=p0+tri.e1.xyz,p2=p0+tri.e2.xyz;
    precise float4 polygon[12];uint length=3u;
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
    result.count=length;
    for(uint j=0u;j<length;j++)result.vertices[j]=polygon[j];
    return result;
}
float2 modelPixelTileXY(float3 ndc,float4 bounds,uint resolution) {
    precise float2 pixel=(ndc.xy-bounds.xy)/(bounds.zw-bounds.xy)*float(resolution);
    return pixel;
}
public ModelPixelPolygon projectModelPixelTriangle(ButterflyMeshTriangle tri,
    ModelPixelFrame frame,float4 bounds,uint resolution,U_CameraInfo camera) {
    ModelPixelPolygon result;
    result.count=0u;
    if(any(bounds.xy>float2(1.0)) || any(bounds.zw<float2(-1.0)))return result;
    ModelPixelClipPolygon polygon=clipModelPixelTriangle(tri,frame,camera);
    result.count=polygon.count;
    for(uint j=0u;j<polygon.count;j++) {
        float3 p=polygon.vertices[j].xyz/polygon.vertices[j].w;
        result.vertices[j]=float3(modelPixelTileXY(p,bounds,resolution),saturate(p.z));
    }
    return result;
}

// A native diagnostic observes the actual coverage loop, not a second call to
// projection code: legal division rounding can differ across compiler contexts.
// Ordinary model consumers specialize this to a no-op (no buffer/readback).
public interface IModelPixelProjectionObserver {
    bool enabled();
    void center(ModelPixelHit hit);
    void surface(ModelPixelHit hit);
    void triangle(uint source,uint vertices,float4 bounds,uint resolution);
    void fan(uint source,uint index,float3 a,float3 b,float3 c);
};
public struct NoModelPixelProjectionObserver : IModelPixelProjectionObserver {
    public bool enabled() { return false; }
    public void center(ModelPixelHit hit) {}
    public void surface(ModelPixelHit hit) {}
    public void triangle(uint source,uint vertices,float4 bounds,uint resolution) {}
    public void fan(uint source,uint index,float3 a,float3 b,float3 c) {}
};

public ModelPixelHit modelPixelCoverageHit<T : IModelPixelProjectionObserver>(StructuredBuffer<ButterflyMeshTriangle> triangles,
    uint first,uint count,ModelPixelFrame frame,float4 bounds,uint2 pixel,uint resolution,
    U_CameraInfo camera,ModelPixelHit hit,T observer) {
    bool centerOccupied=hit.triangle!=0xffffffffu;
    if(centerOccupied && !observer.enabled())return hit;
    if(any(bounds.xy>float2(1.0)) || any(bounds.zw<float2(-1.0))) {
        if(observer.enabled())for(uint i=0u;i<count;i++)observer.triangle(first+i,0u,bounds,resolution);
        return hit;
    }
    for(uint i=0u;i<count;i++) {
        ButterflyMeshTriangle tri=triangles[first+i];
        float3 p0=tri.a.xyz,p1=p0+tri.e1.xyz,p2=p0+tri.e2.xyz;
        ModelPixelClipPolygon polygon=clipModelPixelTriangle(tri,frame,camera);
        if(observer.enabled())observer.triangle(first+i,polygon.count,bounds,resolution);
        for(uint j=1u;j+1u<polygon.count;j++) {
            // Keep the original per-fan division and precise XY dataflow.
            // Materializing normalized/saturated depths in a projected polygon
            // changes rounding, which can change the nearest supporting material.
            float3 pa=polygon.vertices[0].xyz/polygon.vertices[0].w;
            float3 pb=polygon.vertices[j].xyz/polygon.vertices[j].w;
            float3 pc=polygon.vertices[j+1u].xyz/polygon.vertices[j+1u].w;
            precise float2 a=modelPixelTileXY(pa,bounds,resolution);
            precise float2 b=modelPixelTileXY(pb,bounds,resolution);
            precise float2 c=modelPixelTileXY(pc,bounds,resolution);
            if(observer.enabled())observer.fan(first+i,j,float3(a,saturate(pa.z)),
                float3(b,saturate(pb.z)),float3(c,saturate(pc.z)));
            // The designated observation invocation must visit every triangle,
            // even if its center is occupied. Never replace that center's hit.
            if(centerOccupied)continue;
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

public ModelPixelHit modelPixelCoverageHit(StructuredBuffer<ButterflyMeshTriangle> triangles,
    uint first,uint count,ModelPixelFrame frame,float4 bounds,uint2 pixel,uint resolution,
    U_CameraInfo camera,ModelPixelHit hit) {
    return modelPixelCoverageHit(triangles,first,count,frame,bounds,pixel,resolution,camera,hit,
        NoModelPixelProjectionObserver());
}

// Sole geometry entry point for cache baking and standalone validation.
public ModelPixelHit sampleModelPixelGeometry<T : IModelPixelProjectionObserver>(StructuredBuffer<ButterflyMeshTriangle> triangles,
    uint first,uint count,ModelPixelFrame frame,float4 bounds,uint2 pixel,uint resolution,
    U_CameraInfo camera,T observer) {
    ModelPixelHit hit=modelPixelCenterHit(triangles,first,count,frame,bounds,
        float2(pixel)+0.5,resolution,camera);
    observer.center(hit);
    hit=modelPixelCoverageHit(triangles,first,count,frame,bounds,pixel,resolution,camera,hit,observer);
    observer.surface(hit);
    return hit;
}

public ModelPixelHit sampleModelPixelGeometry(StructuredBuffer<ButterflyMeshTriangle> triangles,
    uint first,uint count,ModelPixelFrame frame,float4 bounds,uint2 pixel,uint resolution,
    U_CameraInfo camera) {
    return sampleModelPixelGeometry(triangles,first,count,frame,bounds,pixel,resolution,camera,
        NoModelPixelProjectionObserver());
}

public ModelPixelHit modelPixelCenterHit(StructuredBuffer<ButterflyMeshTriangle> triangles,
    uint first,uint count,ModelPixelFrame frame,float4 bounds,float2 pixel,uint resolution,
    U_CameraInfo camera) {
    ModelPixelHit hit;
    hit.triangle=0xffffffffu; hit.bary=float2(0.0); hit.position=float3(0.0);
    hit.direction=float3(0.0); hit.depth=1.0;
    hit.localOrigin=float3(0.0);hit.localDirection=float3(0.0);
    if(any(bounds.xy>float2(1.0))||any(bounds.zw<float2(-1.0)))return hit;
    float3 worldOrigin,worldDirection;
    modelPixelRay(bounds,pixel,resolution,camera,worldOrigin,worldDirection);
    hit.direction=worldDirection;
    float3 origin=modelPixelLocalVector(frame,worldOrigin-frame.center)/frame.scale;
    float3 direction=modelPixelLocalVector(frame,worldDirection);
    hit.localOrigin=origin;hit.localDirection=direction;
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
