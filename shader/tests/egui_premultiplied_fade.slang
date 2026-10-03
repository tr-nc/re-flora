import color;
import egui_color;

// Exercise the production conversion with gamma-premultiplied colors, as
// supplied by egui's Splash painter and texture atlas. Over an identical
// backdrop, fading must not create a dark intermediate color.
export __extern_cpp int main()
{
    float3 colors[4] = {
        float3(35.0, 63.0, 50.0) / 255.0,
        float3(238.0, 226.0, 184.0) / 255.0,
        float3(1.0),
        float3(0.0),
    };
    for (int i = 0; i < 4; ++i)
    {
        float3 backdrop = srgbToLinear(colors[i]);
        for (int step = 0; step <= 20; ++step)
        {
            float alpha = float(step) / 20.0;
            float4 source = eguiColorToLinear(float4(colors[i] * alpha, alpha));
            float3 blended = source.rgb + backdrop * (1.0 - source.a);
            if (any(abs(blended - backdrop) > float3(0.00001)))
                return 1;
        }
    }
    // egui also supports additive colors (nonzero RGB with zero alpha).
    float4 additive = eguiColorToLinear(float4(0.5, 0.25, 0.1, 0.0));
    if (any(abs(additive.rgb - srgbToLinear(float3(0.5, 0.25, 0.1))) > float3(0.00001)))
        return 2;
    return 0;
}
