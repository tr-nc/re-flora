// Geometry-free cache display projection; instance distance and roll stay live.
module model_pixel_projection;
import tracer_types;

public struct ModelPixelView { public float3 x,y,z; public float2 roll; };
public ModelPixelView modelPixelView(float4 x,float4 y,float4 z) {
    ModelPixelView view;view.x=x.xyz;view.y=y.xyz;view.z=z.xyz;view.roll=float2(x.w,y.w);return view;
}
public float3 modelRotateBetween(float3 a,float3 b,float3 v) {
    float3 c=cross(a,b);
    return v+cross(c,v)+cross(c,cross(c,v))/max(1.0+dot(a,b),1e-6);
}
// Shared head/stem object-image roll. Camera orientation affects display only,
// not the fixed orthographic source grid or its resolution.
public float2 modelObjectRoll(float3 x,float3 z,float3 pivot,U_CameraInfo camera) {
    float3 delta=camera.pos.xyz-pivot;
    float3 view=length(delta)>1e-8?normalize(delta):z;
    float3 right=modelRotateBetween(z,view,x);
    float3 cameraBack=normalize(mul(camera.view_mat_inv,float4(0,0,1,0)).xyz);
    right=modelRotateBetween(view,cameraBack,right);
    return normalize(mul(camera.view_mat,float4(right,0)).xy);
}
public float3 modelBakeVector(ModelPixelView view,float3 v) {
    return float3(dot(v,view.x),dot(v,view.y),dot(v,view.z));
}
public float2 modelRotatePixel(float2 p,float2 roll) {
    return float2(roll.x*p.x-roll.y*p.y,roll.y*p.x+roll.x*p.y);
}
public float4 modelOrthographicQuad(ModelPixelView view,float3 pivot,float radius,float2 uv,
    U_CameraInfo camera) {
    float4 center=mul(camera.view_proj_mat,float4(pivot,1));
    if(center.w<=1e-7 || radius<=1e-8)return float4(2,2,0,1);
    float2 p=modelRotatePixel(uv*2.0-1.0,view.roll);
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
