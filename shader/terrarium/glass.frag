#version 450

layout(location = 0) in vec3 vert_position_ws;
layout(location = 1) in vec2 vert_uv;
layout(location = 2) flat in uint vert_face_id;
layout(location = 3) flat in float vert_near_side;
layout(location = 4) flat in vec2 vert_alpha_pair;

layout(location = 0) out vec4 out_color;

float pixel_hash(vec2 cell) {
    return fract(sin(dot(cell, vec2(127.1, 311.7))) * 43758.5453123);
}

void main() {
    vec2 uv = clamp(vert_uv, vec2(0.0), vec2(1.0));
    vec2 cell_coord = uv * vec2(18.0, 18.0);
    vec2 cell = fract(cell_coord);
    vec2 cell_id = floor(cell_coord);

    float grid_line = step(min(min(cell.x, 1.0 - cell.x), min(cell.y, 1.0 - cell.y)), 0.045);
    float pixel_frost = step(0.72, pixel_hash(cell_id + float(vert_face_id) * 19.0));
    float edge_dist = min(min(uv.x, 1.0 - uv.x), min(uv.y, 1.0 - uv.y));
    float rim = 1.0 - smoothstep(0.0, 0.055, edge_dist);

    float near_side = clamp(vert_near_side, 0.0, 1.0);
    float alpha = mix(vert_alpha_pair.y, vert_alpha_pair.x, near_side);
    alpha += grid_line * mix(0.055, 0.025, near_side);
    alpha += pixel_frost * mix(0.060, 0.018, near_side);
    alpha += rim * mix(0.26, 0.12, near_side);
    alpha = clamp(alpha, 0.0, 0.72);

    vec3 tint_near = vec3(0.74, 0.94, 1.0);
    vec3 tint_far = vec3(0.27, 0.55, 0.64);
    vec3 rim_color = vec3(0.89, 1.0, 0.96);
    vec3 color = mix(tint_far, tint_near, near_side);
    color = mix(color, rim_color, rim * 0.55 + grid_line * 0.18);

    // Premultiplied alpha for the engine blend state.
    out_color = vec4(color * alpha, alpha);
}
