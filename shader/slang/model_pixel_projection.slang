// Cache-compatible geometry/display seam. The bake is always a fixed orthographic
// cube in the selected model view; instance distance and screen roll never enter it.
module model_pixel_projection;
import model_pixel_surface;
import butterfly_mesh_types;
import tracer_types;

public struct ModelPixelView { public float3 x,y,z; public float2 roll; };
public ModelPixelView modelPixelView(float4 x,float4 y,float4 z) {
    ModelPixelView view;view.x=x.xyz;view.y=y.xyz;view.z=z.xyz;view.roll=float2(x.w,y.w);return view;
}
public float3 modelBakeVector(ModelPixelView view,float3 v) {
    return float3(dot(v,view.x),dot(v,view.y),dot(v,view.z));
}
public float2 modelRotatePixel(float2 p,float2 roll) {
    return float2(roll.x*p.x-roll.y*p.y,roll.y*p.x+roll.x*p.y);
}
public U_CameraInfo modelBakeCamera() {
    U_CameraInfo camera=(U_CameraInfo)0;
    // Rays start at z=+1 and travel toward -Z. Identical for every instance.
    camera.view_proj_mat=float4x4(1,0,0,0, 0,1,0,0, 0,0,-0.5,0.5, 0,0,0,1);
    camera.view_proj_mat_inv=float4x4(1,0,0,0, 0,1,0,0, 0,0,-2,1, 0,0,0,1);
    return camera;
}
public ModelPixelHit sampleOrthographicModelPixel(StructuredBuffer<ButterflyMeshTriangle> triangles,
    uint first,uint count,ModelPixelFrame physical,ModelPixelView view,float3 pivot,float radius,
    uint2 pixel,uint resolution) {
    if(radius<=1e-8) {
        ModelPixelHit empty=(ModelPixelHit)0;empty.triangle=0xffffffffu;empty.depth=1.0;return empty;
    }
    ModelPixelFrame bake;
    bake.center=modelBakeVector(view,physical.center-pivot)/radius;
    bake.axisX=modelBakeVector(view,physical.axisX);
    bake.axisY=modelBakeVector(view,physical.axisY);
    bake.axisZ=modelBakeVector(view,physical.axisZ);
    bake.scale=physical.scale/radius;
    return sampleModelPixelGeometry(triangles,first,count,bake,float4(-1,-1,1,1),pixel,resolution,modelBakeCamera());
}
public float4 modelOrthographicQuad(ModelPixelView view,float3 pivot,float radius,float2 uv,
    bool screenAligned,U_CameraInfo camera) {
    float4 center=mul(camera.view_proj_mat,float4(pivot,1));
    if(center.w<=1e-7 || radius<=1e-8)return float4(2,2,0,1);
    float2 p=uv*2.0-1.0;
    if(!screenAligned)p=modelRotatePixel(p,view.roll);
    // Perspective only places/scales the billboard. The image itself is orthographic.
    float2 extent=float2(camera.proj_mat[0][0],camera.proj_mat[1][1])*radius/center.w;
    return float4(center.xy/center.w+p*extent,0,1);
}
public float modelOrthographicDepth(float bakeDepth,float3 pivot,float radius,U_CameraInfo camera) {
    float3 center=mul(camera.view_mat,float4(pivot,1)).xyz;
    center.z+=(1.0-2.0*bakeDepth)*radius;
    float4 clip=mul(camera.proj_mat,float4(center,1));
    return clip.w>1e-7?clip.z/clip.w:1.0;
}
// Exact square/square SAT: destination screen cell rotated into canonical pixels.
// Used only to fill empty center samples, never to overwrite an occupied center.
public bool modelPixelCellOverlap(float2 sourceCenter,float2 destCenter,float2 inverseRoll) {
    float2 d=sourceCenter-destCenter;
    // Edge-only contact has zero area: accepting it would dilate B even at 0 roll.
    float reach=0.5*(1.0+abs(inverseRoll.x)+abs(inverseRoll.y))-1e-6;
    float2 rotated=modelRotatePixel(d,float2(inverseRoll.x,-inverseRoll.y));
    return all(abs(d)<=float2(reach)) && all(abs(rotated)<=float2(reach));
}
