#version 450
#extension GL_ARB_separate_shader_objects : enable

layout(location = 0) in vec2 vPosition;
layout(location = 1) in vec2 vUV;
layout(location = 2) in vec4 vColor;

layout(push_constant) uniform Matrices {
    mat4 ortho;
} matrices;

layout(location = 0) out vec4 oColor;
layout(location = 1) out vec2 oUV;

const float GAMMA = 2.2;

vec3 SRGBtoLINEAR(vec3 color) {
    return pow(color, vec3(GAMMA));
}
vec4 SRGBtoLINEAR(vec4 color) {
    return vec4(SRGBtoLINEAR(color.rgb), color.a);
}

void main() {
    oColor = SRGBtoLINEAR(vColor);
    oUV = vUV;

    gl_Position = matrices.ortho*vec4(vPosition.x, vPosition.y, 0.0, 1.0);
}
