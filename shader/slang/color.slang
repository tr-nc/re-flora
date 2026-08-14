module color;

public float3 linearToSrgb(float3 color)
{
    color = max(color, float3(0.0));
    bool3 useLinearSegment = color < float3(0.0031308);
    float3 higher = 1.055 * pow(color, float3(1.0 / 2.4)) - 0.055;
    float3 lower = color * 12.92;
    return select(useLinearSegment, lower, higher);
}

public float3 srgbToLinear(float3 color)
{
    bool3 useLinearSegment = color < float3(0.04045);
    float3 higher = pow((color + 0.055) / 1.055, float3(2.4));
    float3 lower = color / 12.92;
    return select(useLinearSegment, lower, higher);
}

public float3 rgbToHsv(float3 color)
{
    float4 coefficients = float4(0.0, -1.0 / 3.0, 2.0 / 3.0, -1.0);
    float4 p = select(color.b <= color.g,
                      float4(color.gb, coefficients.xy),
                      float4(color.bg, coefficients.wz));
    float4 q = select(p.x <= color.r,
                      float4(color.r, p.yzx),
                      float4(p.xyw, color.r));

    float difference = q.x - min(q.w, q.y);
    const float epsilon = 1.0e-10;
    return float3(abs(q.z + (q.w - q.y) / (6.0 * difference + epsilon)),
                  difference / (q.x + epsilon), q.x);
}

public float3 hsvToRgb(float3 color)
{
    float3 p = abs(frac(color.xxx + float3(0.0, 2.0 / 3.0, 1.0 / 3.0)) * 6.0 - 3.0);
    return color.z * lerp(float3(1.0), clamp(p - 1.0, 0.0, 1.0), color.y);
}
