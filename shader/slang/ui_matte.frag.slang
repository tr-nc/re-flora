import color;
import ui_matte_material;

struct FragmentInput
{
    float4 position : SV_Position;
    [[vk::location(0)]] float opacity : COLOR0;
    [[vk::location(1)]] float2 local_points : TEXCOORD0;
    [[vk::location(2)]] float2 screen_uv : TEXCOORD1;
    [[vk::location(3)]] nointerpolation uint pixel_cell_size : TEXCOORD2;
};

[[vk::binding(0, 0)]] Sampler2D backdrop_sampler;

[shader("fragment")]
float4 main(FragmentInput input) : SV_Target0
{
    // Both backdrop filter passes completed before this surface's render pass.
    // The encoded-sRGB backdrop includes the scene and all earlier UI layers.
    float3 surface;
    if (input.pixel_cell_size != 0u)
    {
        uint width, height;
        backdrop_sampler.GetDimensions(width, height);
        uint2 cell = uint2(input.position.xy) / input.pixel_cell_size;
        // Read exactly one packed cell texel. No interpolation across cells and
        // no per-pixel grain: every covered fragment in a cell gets one color.
        float2 uv = (float2(cell) + 0.5) / float2(width, height);
        surface = uiMatteTintSrgb(backdrop_sampler.SampleLevel(uv, 0.0).rgb);
    }
    else
    {
        float3 blurred = backdrop_sampler.Sample(saturate(input.screen_uv)).rgb;
        surface = uiMatteSurfaceSrgb(blurred, uiMatteGrain(input.local_points));
    }
    // This is the resolved backdrop, not an 18%-alpha flat overlay. Opacity here
    // belongs only to egui visibility fades; body opacity is applied above.
    return float4(srgbToLinear(surface) * input.opacity, input.opacity);
}
