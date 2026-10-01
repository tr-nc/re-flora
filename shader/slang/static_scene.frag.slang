import color;
import tracer_types;

[[vk::binding(1, 0)]] ConstantBuffer<U_SunInfo> sun_info;
struct FragmentInput
{
    [[vk::location(0)]] float3 normal : NORMAL0;
    [[vk::location(1)]] float4 color : COLOR0;
};
[shader("fragment")]
float4 main(FragmentInput input) : SV_Target0
{
    // A deliberately simple authored-sun/sky consumer for the rooftop proof.
    // No claim that these fixed models contribute to DDGI visibility or cast shadows.
    float diffuse = saturate(dot(normalize(input.normal), sun_info.sun_dir));
    float band = floor(diffuse * 3.0 + 0.5) / 3.0;
    float daylight = saturate(sun_info.sun_dir.y * 4.0);
    float light = lerp(0.14, 0.52 + band * 0.48, daylight);
    // Both passes use premultiplied alpha. Opaque geometry writes depth;
    // sorted glass blends over the interior without obscuring its depth.
    float alpha = saturate(input.color.a);
    return float4(srgbToLinear(input.color.rgb) * light * alpha, alpha);
}
