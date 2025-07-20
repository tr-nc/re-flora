#version 450

layout(location = 0) in vec3 vert_color;
layout(location = 1) in vec3 world_normal;

layout(location = 0) out vec4 out_color;

void main() {
    // Simple diffuse lighting based on normal
    vec3 light_dir = normalize(vec3(0.5, 1.0, 0.3)); // Simple directional light
    float ndotl = max(dot(normalize(world_normal), light_dir), 0.3); // Min ambient
    
    vec3 final_color = vert_color * ndotl;
    out_color = vec4(final_color, 1.0);
}