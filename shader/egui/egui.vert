#version 450
#extension GL_ARB_separate_shader_objects : enable

layout(location = 0) in vec2 v_position;
layout(location = 1) in vec2 v_uv;
layout(location = 2) in vec4 v_color;

layout(push_constant) uniform Matrices { mat4 ortho; }
matrices;

layout(location = 0) out vec4 o_color;
layout(location = 1) out vec2 o_uv;

#include "../include/core/color.glsl"

void main() {
  o_color = vec4(srgb_to_linear(v_color.rgb), v_color.a);
  o_uv    = v_uv;

  gl_Position = matrices.ortho * vec4(v_position.x, v_position.y, 0.0, 1.0);
}
