#version 450

layout(location = 0) out vec4 out_leaf_shadow_opacity;

const float LEAF_SHADOW_FRAGMENT_OPACITY = 0.11;

void main() {
    out_leaf_shadow_opacity = vec4(0.0, 0.0, 0.0, LEAF_SHADOW_FRAGMENT_OPACITY);
}
