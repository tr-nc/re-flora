#version 450

layout(location = 0) in vec4 vert_color;
layout(location = 1) in vec2 vert_uv;
layout(location = 2) flat in uint vert_tex_index;

layout(location = 0) out vec4 out_color;

layout(set = 0, binding = 6) uniform sampler2DArray particle_lod_tex_lut;

void main() {
    vec4 texel = texture(particle_lod_tex_lut, vec3(vert_uv, float(vert_tex_index)));
    float alpha = texel.a * vert_color.a;

    // Premultiplied alpha matches the shared ONE / ONE_MINUS_SRC_ALPHA blend state.
    out_color = vec4(vert_color.rgb * texel.rgb * alpha, alpha);
}
