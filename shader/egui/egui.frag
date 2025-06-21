#version 450
#extension GL_ARB_separate_shader_objects : enable

layout(location = 0) in vec4 o_color;
layout(location = 1) in vec2 o_uv;

layout(binding = 0, set = 0) uniform sampler2D font_sampler;

layout(location = 0) out vec4 out_color;

void main() { out_color = o_color * texture(font_sampler, o_uv); }
