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
