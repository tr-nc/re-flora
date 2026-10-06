// Exercise the production cached-surface projection and its native empty vertex.
// The oracle independently clips the emitted triangles against Vulkan's frustum.
import model_surface_cache;
import model_mesh;

float clipDistance(float4 p,uint plane) {
    if(plane==0u)return p.w+p.x;
    if(plane==1u)return p.w-p.x;
    if(plane==2u)return p.w+p.y;
    if(plane==3u)return p.w-p.y;
    if(plane==4u)return p.z;
    return p.w-p.z;
}
float clippedArea(float4 a,float4 b,float4 c) {
    float4 polygon[16];
    polygon[0]=a;polygon[1]=b;polygon[2]=c;
    uint count=3u;
    for(uint plane=0u;plane<6u&&count!=0u;plane++) {
        float4 next[16];uint nextCount=0u;
        for(uint i=0u;i<count;i++) {
            float4 start=polygon[i],end=polygon[(i+1u)%count];
            float d0=clipDistance(start,plane),d1=clipDistance(end,plane);
            bool inside0=d0>=0.0,inside1=d1>=0.0;
            if(inside0!=inside1)next[nextCount++]=lerp(start,end,d0/(d0-d1));
            if(inside1)next[nextCount++]=end;
        }
        count=nextCount;
        for(uint i=0u;i<count;i++)polygon[i]=next[i];
    }
    float area=0.0;
    for(uint i=2u;i<count;i++) {
        float2 p0=polygon[0].xy/polygon[0].w;
        float2 p1=polygon[i-1u].xy/polygon[i-1u].w;
        float2 p2=polygon[i].xy/polygon[i].w;
        float2 e1=p1-p0,e2=p2-p0;
        area+=abs(e1.x*e2.y-e1.y*e2.x)*0.5;
    }
    return area;
}
float4 emittedVertex(float4 planeClip,float4 surfaceClip) {
    float4 clip;
    bool visible=projectCachedModelSurfaceVertex(planeClip,surfaceClip,clip);
    // All native cached adapters take this exact branch on invisible vertices.
    return visible?clip:emptyModelMeshVertex().position;
}
float emittedQuadArea(float4 planes[4],float4 surface) {
    float4 vertices[4];
    for(uint i=0u;i<4u;i++)vertices[i]=emittedVertex(planes[i],surface);
    return clippedArea(vertices[0],vertices[1],vertices[2])+
        clippedArea(vertices[2],vertices[1],vertices[3]);
}
float4x4 perspective() {
    float n=0.01,f=10.0;
    return float4x4(1,0,0,0, 0,1,0,0,
        0,0,f/(n-f),f*n/(n-f), 0,0,-1,0);
}
export __extern_cpp int main() {
    float4x4 projection=perspective();
    float4 surfaceClip=mul(projection,float4(-0.04,-0.04,-0.0005,1));
    // A cache cell straddles the eye plane. Its surface is entirely
    // before the near plane, outside the viewport; it must draw nothing.
    float3 corners[4]={float3(-0.039,-0.041,-0.0015),
        float3(-0.039,-0.041,0.0005),float3(-0.041,-0.039,-0.0015),
        float3(-0.041,-0.039,0.0005)};
    float4 planes[4];
    for(uint i=0u;i<4u;i++)planes[i]=mul(projection,float4(corners[i],1));
    float phantomArea=emittedQuadArea(planes,surfaceClip);
    uint failures=phantomArea>1e-8?1u:0u;
    printf("offscreen_apple_phantom_triangle_area=%.9f\n",phantomArea);

    // A valid surface depth must not turn an entirely offscreen image plane
    // into a visible triangle either. Both triangles lie outside the left plane.
    float3 outside[4]={float3(-0.03,-0.02,-0.02),float3(-0.03,-0.02,0.02),
        float3(-0.03,0.02,-0.02),float3(-0.03,0.02,0.02)};
    for(uint i=0u;i<4u;i++)planes[i]=mul(projection,float4(outside[i],1));
    float outsideArea=emittedQuadArea(planes,mul(projection,float4(-0.03,0,-0.02,1)));
    if(outsideArea>1e-8)failures|=2u;
    printf("offscreen_valid_depth_phantom_triangle_area=%.9f\n",outsideArea);

    // Eye-plane rejection may not vary between vertices of one cell. Preserve
    // finite homogeneous XY/W, including zero/negative W, for Vulkan to clip.
    // Offset from the eye itself so the independent area oracle does not need
    // to assign a screen coordinate to the projective zero vector (0,0,0,0).
    float4 crossing[4]={float4(-0.005,-0.01,0,0.02),float4(0.015,-0.01,0,-0.02),
        float4(-0.005,0.01,0,0.02),float4(0.015,0.01,0,-0.02)};
    for(uint i=0u;i<5u;i++)for(uint d=0u;d<3u;d++) {
        float4 plane=i<4u?crossing[i]:float4(0.01,0.02,0,0);
        float depth=d==0u?0.0:d==1u?0.25:1.0;
        float4 clip;
        bool visible=projectCachedModelSurfaceVertex(plane,float4(0,0,depth,1),clip);
        if(!visible||any(clip.xy!=plane.xy)||clip.w!=plane.w||clip.z!=depth*plane.w)
            failures|=4u;
    }
    float crossingArea=emittedQuadArea(crossing,float4(0,0,0.5,1));
    if(crossingArea<=0.1||crossingArea>4.00001||isnan(crossingArea))failures|=8u;

    // Invalid reconstructed depths reject the ENTIRE cell, while near/far
    // boundary depths above remain valid. No per-vertex sentinel substitution.
    float4 invalidDepths[5]={float4(0,0,0,0),float4(0,0,0,-1),
        float4(0,0,0,1e-8),float4(0,0,-0.01,1),float4(0,0,1.01,1)};
    for(uint d=0u;d<5u;d++)for(uint i=0u;i<4u;i++) {
        float4 clip;
        if(projectCachedModelSurfaceVertex(crossing[i],invalidDepths[d],clip))failures|=16u;
    }

    // Ordinary on-screen cells retain exactly the existing footprint and depth.
    float4 onScreen[4]={float4(-0.005,-0.005,0,0.02),float4(0.005,-0.005,0,0.02),
        float4(-0.005,0.005,0,0.02),float4(0.005,0.005,0,0.02)};
    if(abs(emittedQuadArea(onScreen,float4(0,0,0.015,0.02))-0.25)>1e-6)failures|=32u;
    printf("visible_eye_crossing_area=%.9f failures=%u\n",crossingArea,failures);
    return int(failures);
}
