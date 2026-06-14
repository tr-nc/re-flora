#version 450

layout(location = 0) in vec3 vert_position_ws;
layout(location = 1) in vec2 vert_uv;
layout(location = 2) flat in uint vert_face_id;
layout(location = 3) flat in float vert_near_side;
layout(location = 4) flat in vec2 vert_alpha_pair;

layout(location = 0) out vec4 out_color;

void main() {
    vec2 uv = clamp(vert_uv, vec2(0.0), vec2(1.0));

    float edge_dist = min(min(uv.x, 1.0 - uv.x), min(uv.y, 1.0 - uv.y));
    float rim = 1.0 - smoothstep(0.0, 0.035, edge_dist);
    float side_edge = 1.0 - smoothstep(0.0, 0.045, min(uv.x, 1.0 - uv.x));
    float top_edge = 1.0 - smoothstep(0.0, 0.050, 1.0 - uv.y);
    float bottom_edge = 1.0 - smoothstep(0.0, 0.035, uv.y);
    float corner = side_edge * max(top_edge, bottom_edge);

    float near_side = clamp(vert_near_side, 0.0, 1.0);
    float height_glow = smoothstep(0.15, 1.05, vert_position_ws.y) * 0.018;
    float alpha = mix(vert_alpha_pair.y, vert_alpha_pair.x, near_side);
    alpha += height_glow;
    alpha += rim * mix(0.085, 0.045, near_side);
    alpha += corner * 0.035;
    alpha = clamp(alpha, 0.0, 0.26);

    vec3 pane_blue = vec3(0.78, 0.92, 1.0);
    vec3 far_blue = vec3(0.58, 0.78, 0.92);
    vec3 side_edge_color = vec3(0.58, 0.96, 0.92);
    vec3 top_edge_color = vec3(1.00, 0.92, 0.72);
    vec3 bottom_edge_color = vec3(0.66, 0.82, 1.00);

    vec3 color = mix(far_blue, pane_blue, near_side);
    color = mix(color, bottom_edge_color, bottom_edge * 0.22);
    color = mix(color, side_edge_color, side_edge * 0.30);
    color = mix(color, top_edge_color, top_edge * 0.38);
    color = mix(color, vec3(0.95, 1.0, 0.96), corner * 0.35);

    // Premultiplied alpha for the engine blend state.
    out_color = vec4(color * alpha, alpha);
}
