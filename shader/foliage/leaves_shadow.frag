#version 450

layout(location = 0) in float vert_leaf_shadow_opacity;
layout(location = 0) out vec4 out_leaf_shadow_opacity;

void main() {
    out_leaf_shadow_opacity = vec4(0.0, 0.0, 0.0, vert_leaf_shadow_opacity);
}
