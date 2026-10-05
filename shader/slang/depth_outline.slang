import color;
import depth_outline_filter;
import tracer_types;
struct OutlineParameters { float4 color_strength; float4 thresholds; float4 detail; };
[[vk::push_constant]] ConstantBuffer<OutlineParameters> outline_parameters;
[[vk::binding(0,0)]] ConstantBuffer<U_CameraInfo> camera_info;
[[vk::binding(1,0)]] [[vk::image_format("r32f")]] readonly RWTexture2D<float> scene_depth_tex;
[[vk::binding(2,0)]] [[vk::image_format("rgba16f")]] RWTexture2D<float4> composited_tex;

OutlineSample sampleDepth(int2 p, int2 size)
{
    return outlineSample(scene_depth_tex[p], (float2(p)+0.5)/float2(size), camera_info.proj_mat_inv);
}
float outerDepth(int2 p, int2 size, float fallback)
{
    // A missing outer sample is extrapolated, not treated as sky or clamped.
    if(any(p<0)||any(p>=size))return fallback;
    return sampleDepth(p,size).inverseDepth;
}
[shader("compute")]
[numthreads(8,8,1)]
void main(uint3 id : SV_DispatchThreadID)
{
    uint w,h; composited_tex.GetDimensions(w,h);
    int2 size=int2(w,h),p=int2(id.xy);
    if(any(p>=size))return;
    OutlineSample c=sampleDepth(p,size);
    if(c.inverseDepth<=0.0)return;
    static const int2 directions[4]={int2(1,0),int2(0,1),int2(1,1),int2(1,-1)};
    float signal=0.0; uint thinPairs=0u;
    for(uint i=0u;i<4u;i++) {
        int2 a=p-directions[i],b=p+directions[i];
        if(any(a<0)||any(b<0)||any(a>=size)||any(b>=size))continue;
        OutlineSample da=sampleDepth(a,size),db=sampleDepth(b,size);
        float outerA=outerDepth(p-2*directions[i],size,2*da.inverseDepth-c.inverseDepth);
        float outerB=outerDepth(p+2*directions[i],size,2*db.inverseDepth-c.inverseDepth);
        signal=max(signal,outlinePair(c,da,db,outerA,outerB,
            outline_parameters.thresholds,outline_parameters.detail.x));
        if(outlineThinPair(c.inverseDepth,da.inverseDepth,db.inverseDepth,outline_parameters.thresholds.x))thinPairs++;
    }
    float coverage=outlineCoverage(signal,thinPairs,outline_parameters.detail.y);
    float4 color=composited_tex[p];
    color.rgb=lerp(color.rgb,srgbToLinear(outline_parameters.color_strength.rgb),
        coverage*outline_parameters.color_strength.w);
    // Neighbours read only the authoritative scene depth, never this color.
    composited_tex[p]=color;
}
