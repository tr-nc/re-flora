import model_pixel_surface;
import butterfly_mesh_types;
import tracer_types;

export __extern_cpp int main() {
    // Slang 2025.23.2's executable backend leaves these shader qualifiers
    // undefined in C++. Compatibility definitions only for this CPU executable;
    // production SPIR-V keeps all precise arithmetic and unroll annotations.
    __requirePrelude("#define precise\n#ifndef SLANG_UNROLL\n#define SLANG_UNROLL\n#endif\n");
    // Actual producer-projected coordinates at the minimized 64px leaf boundary.
    // Reading a GPU coverage mask would be circular; the CPU oracle instead
    // independently evaluates these vertices with the unchanged cell epsilon.
    float2 a=float2(11.248875617980957,25.61142349243164);
    float2 b=float2(13.594080924987793,26.65847396850586);
    float2 c=float2(13.542619705200195,26.797483444213867);
    if(pixelTriangleCovers(a,b,c,float2(11,26)))return 1;
    if(!pixelTriangleCovers(a,b,c,float2(12,26)))return 2;

    // Exercise the exact clipping/projection helper called by the real coverage
    // producer and the native evidence adapter, not a diagnostic copy.
    U_CameraInfo camera;
    camera.view_proj_mat=float4x4(1,0,0,0, 0,1,0,0, 0,0,1,0, 0,0,0,1);
    ButterflyMeshTriangle tri;
    tri.a=float4(-0.5,-0.5,0.5,0);
    tri.e1=float4(1,0,0,0);
    tri.e2=float4(0.5,1,0,0);
    ModelPixelFrame frame=identityModelPixelFrame();
    float4 bounds=float4(-1,-1,1,1);
    ModelPixelPolygon p=projectModelPixelTriangle(tri,frame,bounds,8u,camera);
    if(p.count!=3u || any(p.vertices[0]!=float3(2,2,0.5))
        || any(p.vertices[1]!=float3(6,2,0.5)) || any(p.vertices[2]!=float3(4,6,0.5)))return 3;
    tri.a.z=-0.5;tri.e1.z=1;tri.e2.z=1;
    p=projectModelPixelTriangle(tri,frame,bounds,8u,camera);
    if(p.count!=4u)return 4;
    uint clipped=0;
    for(uint i=0u;i<p.count;i++) {
        if(p.vertices[i].z<0.0 || p.vertices[i].z>1.0)return 5;
        if(p.vertices[i].z==0.0)clipped++;
    }
    if(clipped!=2u)return 6;
    tri.e1.z=0;tri.e2.z=0;
    if(projectModelPixelTriangle(tri,frame,bounds,8u,camera).count>=3u)return 7;
    if(projectModelPixelTriangle(tri,frame,float4(2,2,3,3),64u,camera).count!=0u)return 8;
    return 0;
}
