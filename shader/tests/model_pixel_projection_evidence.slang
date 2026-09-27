import model_pixel_bake_geometry;
import model_pixel_types;
import model_bake_triangle;
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

    // Small-leaf counterexample: separate projection of identical clip vertices
    // was covered, but the executed coverage-loop vertices are not.
    float2 sa=float2(5.997374057769775,11.06484317779541);
    float2 sb=float2(5.986352443695068,11.0205078125);
    float2 sc=float2(6.267217636108398,10.604094505310059);
    if(pixelTriangleCovers(sa,sb,sc,float2(5,10)))return 9;
    if(!pixelTriangleCovers(float2(5.997326374053955,11.06486988067627),
        float2(5.986303329467773,11.020524978637695),
        float2(6.267250061035156,10.604071617126465),float2(5,10)))return 10;
    for(int i=-2;i<=2;i++) {
        float2 shift=float2(float(i)*0.001,0);
        if(pixelTriangleCovers(sa+shift,sb+shift,sc+shift,float2(5,10))!=(i<0))return 11;
        if(pixelTriangleCovers(sa+shift,sc+shift,sb+shift,float2(5,10))!=(i<0))return 12;
        if(!pixelTriangleCovers(sa+shift,sb+shift,sc+shift,float2(6,10)))return 13;
        if(pixelTriangleCovers(a+shift,b+shift,c+shift,float2(11,26))!=(i<0))return 14;
        if(!pixelTriangleCovers(a+shift,b+shift,c+shift,float2(12,26)))return 15;
    }

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
