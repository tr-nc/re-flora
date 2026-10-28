import color;
import ui_matte_material;

struct FragmentInput
{
    [[vk::location(0)]] float opacity : COLOR0;
    [[vk::location(1)]] float2 local_points : TEXCOORD0;
    [[vk::location(2)]] float2 screen_uv : TEXCOORD1;
};

[[vk::binding(0, 0)]] Sampler2D backdrop_sampler;

[shader("fragment")]
float4 main(FragmentInput input) : SV_Target0
{
    // Both dense Gaussian passes completed before the UI render pass. All
    // panels share this current-frame, scene-only, encoded-sRGB backdrop.
    float3 blurred = backdrop_sampler.Sample(saturate(input.screen_uv)).rgb;
    float3 surface = uiMatteSurfaceSrgb(blurred, uiMatteGrain(input.local_points));
    // This is the resolved backdrop, not an 18%-alpha flat overlay. Opacity here
    // belongs only to egui visibility fades; body opacity is applied above.
    return float4(srgbToLinear(surface) * input.opacity, input.opacity);
}
