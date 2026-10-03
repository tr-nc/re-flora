module egui_color;

import color;

public float4 eguiColorToLinear(float4 color)
{
    // egui premultiplies RGB in gamma space. Decode the unassociated color,
    // then premultiply in linear space for ONE / ONE_MINUS_SRC_ALPHA blending.
    // Zero-alpha colors may be additive, so preserve their RGB contribution.
    if (color.a > 0.0)
        return float4(srgbToLinear(color.rgb / color.a) * color.a, color.a);
    return float4(srgbToLinear(color.rgb), 0.0);
}
