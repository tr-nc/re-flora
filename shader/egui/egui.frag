#version 450

#extension GL_ARB_separate_shader_objects : enable

layout(location = 0) in vec4 o_color;
layout(location = 1) in vec2 o_uv;

layout(location = 0) out vec4 out_color;

layout(binding = 0, set = 0) uniform sampler2D manual_font_sampler;

void main() { out_color = o_color * texture(manual_font_sampler, o_uv); }
