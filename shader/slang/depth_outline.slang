import color;
import depth_outline_filter;
import tracer_types;
struct OutlineParameters { float4 color_strength; float4 thresholds; float4 detail; };
[[vk::push_constant]] ConstantBuffer<OutlineParameters> outline_parameters;
[[vk::binding(0,0)]] ConstantBuffer<U_CameraInfo> camera_info;
[[vk::binding(1,0)]] Sampler2D gfx_depth_tex;
[[vk::binding(2,0)]] Sampler2D gfx_output_tex;
[[vk::binding(3,0)]] [[vk::image_format("r32f")]] readonly RWTexture2D<float> compute_depth_tex;
[[vk::binding(4,0)]] [[vk::image_format("rgba16f")]] RWTexture2D<float4> composited_tex;

float reciprocalDepth(int2 p, int2 size)
{
    float raster = gfx_depth_tex.Load(int3(p,0)).r;
    float alpha = gfx_output_tex.Load(int3(p,0)).a;
    float terrain = compute_depth_tex[p];
    // Partial-alpha droplets/glass must not create outlines over the opaque
    // scene. An opaque raster surface and terrain use their nearest depth.
    float depth = outlineVisibleDepth(raster, alpha, terrain);
    return outlineInverseEyeDepth(depth, (float2(p)+0.5)/float2(size), camera_info.proj_mat_inv);
}
[shader("compute")]
[numthreads(8,8,1)]
void main(uint3 id : SV_DispatchThreadID)
{
    uint w,h; composited_tex.GetDimensions(w,h);
    int2 size=int2(w,h),p=int2(id.xy);
    if(any(p>=size))return;
    float c=reciprocalDepth(p,size);
    if(c<=0.0)return; // sky receives no outer halo
    static const int2 directions[4]={int2(1,0),int2(0,1),int2(1,1),int2(1,-1)};
    float signal=0.0; uint thinPairs=0u;
    for(uint i=0u;i<4u;i++) {
        int2 a=p-directions[i],b=p+directions[i];
        // Missing samples are not duplicated slopes and not treated as sky.
        if(any(a<0)||any(b<0)||any(a>=size)||any(b>=size))continue;
        float da=reciprocalDepth(a,size),db=reciprocalDepth(b,size);
        signal=max(signal,outlinePair(c,da,db,outline_parameters.thresholds.x,
            outline_parameters.thresholds.y,outline_parameters.thresholds.z,outline_parameters.detail.x));
        if(outlineThinPair(c,da,db,outline_parameters.thresholds.x))thinPairs++;
    }
    float coverage=outlineCoverage(signal,thinPairs,outline_parameters.detail.y);
    float4 color=composited_tex[p];
    color.rgb=lerp(color.rgb,srgbToLinear(outline_parameters.color_strength.rgb),
        coverage*outline_parameters.color_strength.w);
    // Only this invocation accesses this color pixel; neighbours read depth
    // only. No cross-invocation color feedback or extra history/atlas needed.
    composited_tex[p]=color;
}
