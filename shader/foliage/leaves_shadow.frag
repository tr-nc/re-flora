#version 450

layout(location = 0) in float vert_leaf_shadow_opacity;
layout(location = 0) out vec4 out_leaf_shadow_opacity;

void main() {
    float opacity = vert_leaf_shadow_opacity;
    float light_depth = clamp(gl_FragCoord.z, 0.0, 1.0);

    // Store depth premultiplied by opacity in R. The alpha channel remains the
    // accumulated opacity used by the 2D leaf shadow path; receivers can divide
    // R by A to approximate the light-space caster depth for self-shadow gating.
    out_leaf_shadow_opacity = vec4(light_depth * opacity, 0.0, 0.0, opacity);
}
