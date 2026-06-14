#version 450

layout(location = 0) in vec3 vert_position_ws;
layout(location = 1) in vec2 vert_uv;
layout(location = 2) flat in uint vert_face_id;
layout(location = 3) flat in float vert_near_side;
layout(location = 4) flat in vec2 vert_alpha_pair;
layout(location = 5) in vec3 vert_normal_ws;
layout(location = 6) flat in uint vert_part_kind;
layout(location = 7) in vec3 vert_view_dir_ws;

layout(location = 0) out vec4 out_color;

layout(set = 0, binding = 1) uniform U_SunInfo {
    vec3 sun_dir;
    float sun_size;
    vec3 sun_color;
    float sun_luminance;
    float sun_display_luminance;
    float sun_altitude;
    float sun_azimuth;
}
sun_info;

vec3 sky_reflection_color(vec3 dir) {
    float up = clamp(dir.y * 0.5 + 0.5, 0.0, 1.0);
    vec3 low_sky = vec3(0.46, 0.68, 0.86);
    vec3 high_sky = vec3(0.88, 0.97, 1.00);
    vec3 ground_green = vec3(0.12, 0.23, 0.17);
    vec3 sky = mix(low_sky, high_sky, up * up);
    return dir.y >= 0.0 ? sky : mix(low_sky, ground_green, clamp(-dir.y, 0.0, 1.0));
}

void main() {
    vec2 uv = clamp(vert_uv, vec2(0.0), vec2(1.0));
    float edge_dist = min(min(uv.x, 1.0 - uv.x), min(uv.y, 1.0 - uv.y));
    float uv_edge = 1.0 - smoothstep(0.0, 0.060, edge_dist);
    float core_edge = 1.0 - smoothstep(0.0, 0.018, edge_dist);
    float side_edge = 1.0 - smoothstep(0.0, 0.060, min(uv.x, 1.0 - uv.x));
    float top_edge = 1.0 - smoothstep(0.0, 0.065, 1.0 - uv.y);
    float bottom_edge = 1.0 - smoothstep(0.0, 0.045, uv.y);
    float corner = side_edge * max(top_edge, bottom_edge);

    vec3 view_dir = normalize(vert_view_dir_ws);
    vec3 normal = normalize(vert_normal_ws);
    normal = dot(normal, view_dir) < 0.0 ? -normal : normal;
    float ndotv = clamp(dot(normal, view_dir), 0.0, 1.0);
    float fresnel = pow(1.0 - ndotv, 4.0);
    vec3 reflected_dir = reflect(-view_dir, normal);
    vec3 env_reflection = sky_reflection_color(reflected_dir);

    vec3 sun_dir = normalize(sun_info.sun_dir);
    float sun_glint = pow(max(dot(reflected_dir, sun_dir), 0.0), 96.0);
    sun_glint *= clamp(sun_info.sun_display_luminance, 0.0, 2.0);

    float near_side = clamp(vert_near_side, 0.0, 1.0);
    float body_alpha = mix(vert_alpha_pair.y, vert_alpha_pair.x, near_side);
    float pane_alpha = body_alpha + fresnel * 0.030 + uv_edge * 0.016;
    float edge_alpha = 0.22 + fresnel * 0.24 + core_edge * 0.18 + corner * 0.12;
    float rim_alpha = 0.28 + fresnel * 0.22 + core_edge * 0.16;
    float corner_alpha = 0.45 + fresnel * 0.26 + core_edge * 0.20;

    float alpha = pane_alpha;
    if (vert_part_kind == 1u) {
        alpha = edge_alpha;
    } else if (vert_part_kind == 2u) {
        alpha = rim_alpha;
    } else if (vert_part_kind == 3u) {
        alpha = corner_alpha;
    }
    alpha = clamp(alpha, 0.0, 0.78);

    vec3 pane_color = mix(vec3(0.62, 0.82, 0.94), vec3(0.86, 0.96, 1.00), near_side);
    vec3 edge_color = vec3(0.70, 1.00, 0.96);
    vec3 top_color = vec3(1.00, 0.94, 0.74);
    vec3 bottom_color = vec3(0.54, 0.76, 1.00);
    vec3 corner_color = vec3(0.96, 1.00, 0.98);

    vec3 color = pane_color;
    float edge_material = vert_part_kind == 0u ? 0.0 : 1.0;
    color = mix(color, edge_color, edge_material * 0.55);
    color = mix(color, bottom_color, max(bottom_edge, step(0.6, -normal.y)) * edge_material * 0.35);
    color = mix(color, top_color, max(top_edge, step(0.6, normal.y)) * edge_material * 0.40);
    color = mix(color, corner_color, max(corner, vert_part_kind == 3u ? 1.0 : 0.0) * 0.42);

    float long_top_glint = 0.0;
    if (vert_part_kind == 2u && normal.y > 0.35) {
        long_top_glint = smoothstep(0.08, 0.35, uv.x) * (1.0 - smoothstep(0.68, 0.96, uv.x));
    }

    float optical_edge = max(edge_material, uv_edge * 0.35);
    vec3 reflection = env_reflection * (0.08 + fresnel * 0.45 + optical_edge * 0.26);
    reflection += sun_info.sun_color * (sun_glint * (0.20 + optical_edge * 0.90));

    // Cheap stylized refraction: concentrate pale spectral separation at thick glass edges.
    vec3 spectral = vec3(0.10, 0.02, 0.00) * top_edge;
    spectral += vec3(0.00, 0.12, 0.18) * side_edge;
    spectral += vec3(0.00, 0.05, 0.14) * bottom_edge;
    spectral *= optical_edge * (0.35 + fresnel);

    color += reflection + spectral;
    color = mix(color, vec3(1.18, 1.28, 1.22), core_edge * edge_material * 0.42);
    color += vec3(1.25, 1.14, 0.82) * long_top_glint * 0.38;

    // Premultiplied alpha for the engine blend state.
    out_color = vec4(color * alpha, alpha);
}
