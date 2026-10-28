import color;
import ui_matte_material;

struct FragmentInput
{
    [[vk::location(0)]] float opacity : COLOR0;
    [[vk::location(1)]] float2 local_points : TEXCOORD0;
    [[vk::location(2)]] float2 screen_uv : TEXCOORD1;
    [[vk::location(3)]] float2 point_uv : TEXCOORD2;
};

[[vk::binding(0, 0)]] Sampler2D backdrop_sampler;

[shader("fragment")]
float4 main(FragmentInput input) : SV_Target0
{
    // A bounded 7x7 Gaussian reference kernel, sigma 7 logical points. This
    // approximates CSS blur; it is not claimed to be a browser-identical kernel.
    // Scene-only sampling means text stays crisp and no attachment is sampled
    // while being written. Clamp handles panels touching a viewport edge.
    static const float weights[7] = { 0.00443305, 0.05400558, 0.24203623,
        0.39905028, 0.24203623, 0.05400558, 0.00443305 };
    float3 blurred = float3(0.0);
    for (int y = -3; y <= 3; ++y)
    {
        for (int x = -3; x <= 3; ++x)
        {
            float2 uv = saturate(input.screen_uv + float2(x, y) * input.point_uv * UI_MATTE_FROST_POINTS);
            float3 sampleSrgb = linearToSrgb(backdrop_sampler.Sample(uv).rgb);
            blurred += sampleSrgb * (weights[x + 3] * weights[y + 3]);
        }
    }
    float3 surface = uiMatteSurfaceSrgb(blurred, uiMatteGrain(input.local_points));
    // This is the resolved backdrop, not an 18%-alpha flat overlay. Opacity here
    // belongs only to egui visibility fades; body opacity is applied above.
    return float4(srgbToLinear(surface) * input.opacity, input.opacity);
}
