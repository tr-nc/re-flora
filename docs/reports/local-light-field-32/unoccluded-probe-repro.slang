// Diagnostic: production authored-light evaluator and DDGI surface-side policy,
// with ideal exact-normal probe values (no oct-map approximation or occluders).
import local_lighting;
import ddgi_query;
export __extern_cpp int main()
{
    float spacing = 32.0/256.0;
    U_LightGpu light = {};
    light.abi_version=LOCAL_LIGHT_GPU_ABI_VERSION;
    light.kind=LOCAL_LIGHT_KIND_POINT;
    light.position=float3(.5,.4,.5)*spacing;
    light.range=1.0;light.source_radius=1.0/256.0;
    light.intensity=1.0;light.color=float3(1.0);
    float3 receiver=float3(.5,.1,.5)*spacing;
    float3 normal=float3(0,1,0);
    float sum=0.0,weightSum=0.0;
    for(uint i=0;i<8;++i){
        float3 corner=float3(i&1u,(i>>1u)&1u,(i>>2u)&1u);
        float3 probe=corner*spacing;
        float3 axial=lerp(float3(1)-receiver/spacing,receiver/spacing,corner);
        float weight=axial.x*axial.y*axial.z*ddgiSurfaceSideWeight(dot(normal,normalize(probe-receiver)),0u);
        sum+=weight*evaluateUnshadowedPointLight(light,probe,normal).irradiance.x;
        weightSum+=weight;
    }
    float exact=evaluateUnshadowedPointLight(light,receiver,normal).irradiance.x;
    float interpolated=sum/weightSum;
    printf("spacing=32 no_blockers exact=%f original_interpolation=%f retained=%f\n",exact,interpolated,interpolated/exact);
    return abs(interpolated-exact)<=.05*exact?0:1;
}
