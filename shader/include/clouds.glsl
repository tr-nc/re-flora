#ifndef CLOUDS_GLSL
#define CLOUDS_GLSL

#include "../include/core/hash.glsl"

const float CLOUD_PI = 3.14159265;

struct CloudRaymarchResult {
    vec3 color;
    float alpha;
};

float cloud_time_seconds() { return float(env_info.frame_serial_idx) * 0.016; }

vec2 cloud_wind_offset(float multiplier) {
    vec2 wind_dir = normalize(vec2(0.82, 0.31));
    return wind_dir * cloud_time_seconds() * gui_input.cloud_wind_speed * multiplier;
}

float cloud_value_noise2(vec2 p) {
    vec2 i = floor(p);
    vec2 f = fract(p);
    f      = f * f * (3.0 - 2.0 * f);

    float a = hash_12(i + vec2(0.0, 0.0));
    float b = hash_12(i + vec2(1.0, 0.0));
    float c = hash_12(i + vec2(0.0, 1.0));
    float d = hash_12(i + vec2(1.0, 1.0));

    return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
}

float cloud_value_noise3(vec3 p) {
    vec3 i = floor(p);
    vec3 f = fract(p);
    f      = f * f * (3.0 - 2.0 * f);

    float n000 = hash13(i + vec3(0.0, 0.0, 0.0));
    float n100 = hash13(i + vec3(1.0, 0.0, 0.0));
    float n010 = hash13(i + vec3(0.0, 1.0, 0.0));
    float n110 = hash13(i + vec3(1.0, 1.0, 0.0));
    float n001 = hash13(i + vec3(0.0, 0.0, 1.0));
    float n101 = hash13(i + vec3(1.0, 0.0, 1.0));
    float n011 = hash13(i + vec3(0.0, 1.0, 1.0));
    float n111 = hash13(i + vec3(1.0, 1.0, 1.0));

    float x00 = mix(n000, n100, f.x);
    float x10 = mix(n010, n110, f.x);
    float x01 = mix(n001, n101, f.x);
    float x11 = mix(n011, n111, f.x);
    float y0  = mix(x00, x10, f.y);
    float y1  = mix(x01, x11, f.y);
    return mix(y0, y1, f.z);
}

float cloud_fbm2(vec2 p) {
    float sum = 0.0;
    float amp = 0.58;
    sum += cloud_value_noise2(p) * amp;
    p *= 2.03;
    amp *= 0.50;
    sum += cloud_value_noise2(p + vec2(17.2, 9.4)) * amp;
    return clamp(sum / 0.87, 0.0, 1.0);
}

float cloud_fbm3_shape(vec3 p) {
    float sum = 0.0;
    float amp = 0.58;
    sum += cloud_value_noise3(p) * amp;
    p *= 2.03;
    amp *= 0.50;
    sum += cloud_value_noise3(p + vec3(19.1, 3.7, 11.3)) * amp;
    p *= 2.01;
    amp *= 0.42;
    sum += cloud_value_noise3(p + vec3(5.6, 23.9, 41.1)) * amp;
    return clamp(sum / 1.29, 0.0, 1.0);
}

float cloud_layer_bottom() { return min(gui_input.cloud_bottom_height, gui_input.cloud_top_height - 1.0); }

float cloud_layer_top() { return max(gui_input.cloud_top_height, gui_input.cloud_bottom_height + 1.0); }

float cloud_height_fraction(vec3 world_pos) {
    float bottom = cloud_layer_bottom();
    float top    = cloud_layer_top();
    return clamp((world_pos.y - bottom) / max(top - bottom, 1.0), 0.0, 1.0);
}

float cloud_height_gradient(vec3 world_pos) {
    float h       = cloud_height_fraction(world_pos);
    float bottom  = smoothstep(0.00, 0.16, h);
    float top     = 1.0 - smoothstep(0.72, 1.00, h);
    float anvil   = smoothstep(0.18, 0.52, h) * (1.0 - smoothstep(0.86, 1.0, h));
    float wisps   = smoothstep(0.02, 0.10, h) * (1.0 - smoothstep(0.18, 0.38, h));
    return clamp(bottom * top * mix(0.70, 1.15, anvil) + wisps * 0.20, 0.0, 1.0);
}

float cloud_coverage_at(vec2 xz) {
    vec2 weather_p = xz * 0.0045 + cloud_wind_offset(0.012);
    float weather  = cloud_fbm2(weather_p);
    float coverage = gui_input.cloud_coverage * mix(0.58, 1.35, weather);
    return clamp(coverage, 0.0, 1.0);
}

float cloud_density_low_quality(vec3 world_pos) {
    float vertical = cloud_height_gradient(world_pos);
    if (vertical <= 0.0) {
        return 0.0;
    }

    float coverage = cloud_coverage_at(world_pos.xz);
    if (coverage <= 0.001 || gui_input.cloud_density <= 0.001) {
        return 0.0;
    }

    vec2 wind = cloud_wind_offset(0.055);
    vec3 p    = vec3(world_pos.xz * gui_input.cloud_shape_scale + wind,
                  world_pos.y * gui_input.cloud_shape_scale * 1.8);
    float shape = cloud_value_noise3(p);
    float threshold = mix(0.78, 0.24, coverage);
    float density = smoothstep(threshold, threshold + 0.15, shape);
    return density * vertical * gui_input.cloud_density;
}

float cloud_density(vec3 world_pos) {
    float vertical = cloud_height_gradient(world_pos);
    if (vertical <= 0.0) {
        return 0.0;
    }

    float coverage = cloud_coverage_at(world_pos.xz);
    if (coverage <= 0.001 || gui_input.cloud_density <= 0.001) {
        return 0.0;
    }

    vec2 wind = cloud_wind_offset(0.055);
    vec3 shape_p = vec3(world_pos.xz * gui_input.cloud_shape_scale + wind,
                       world_pos.y * gui_input.cloud_shape_scale * 1.8);
    float shape = cloud_fbm3_shape(shape_p);

    // Cheap Perlin-Worley-style shaping: a connected low-frequency volume with
    // higher-frequency erosion concentrated at the edges and lower wispy parts.
    float threshold = mix(0.76, 0.22, coverage);
    float base_density = smoothstep(threshold, threshold + 0.14, shape);

    vec2 detail_wind = cloud_wind_offset(0.17);
    vec3 detail_p = vec3(world_pos.xz * gui_input.cloud_detail_scale + detail_wind,
                        world_pos.y * gui_input.cloud_detail_scale * 2.4 + 37.0);
    float detail = cloud_value_noise3(detail_p);
    float edge_weight = 1.0 - smoothstep(0.20, 0.72, base_density);
    float lower_wisp = 1.0 - smoothstep(0.24, 0.58, cloud_height_fraction(world_pos));
    float erosion = detail * gui_input.cloud_detail_strength * (0.46 + 0.44 * lower_wisp) * edge_weight;

    float density = max(base_density - erosion, 0.0);
    density = min(density * 1.10, 1.0);
    density *= vertical * gui_input.cloud_density;
    return max(density, 0.0);
}

bool cloud_layer_interval(vec3 view_dir, out float t0, out float t1) {
    t0 = 0.0;
    t1 = 0.0;

    if (gui_input.clouds_enabled == 0u || gui_input.cloud_coverage <= 0.001 ||
        gui_input.cloud_density <= 0.001) {
        return false;
    }

    if (abs(view_dir.y) < 0.018) {
        return false;
    }

    float bottom = cloud_layer_bottom();
    float top    = cloud_layer_top();
    vec3 camera_pos = camera_info.pos.xyz;

    float tb = (bottom - camera_pos.y) / view_dir.y;
    float tt = (top - camera_pos.y) / view_dir.y;
    t0 = max(min(tb, tt), 0.0);
    t1 = max(tb, tt);

    if (t1 <= t0) {
        return false;
    }

    float max_distance = max(gui_input.cloud_max_distance, 1.0);
    t1 = min(t1, max_distance);
    return t1 > t0;
}

float cloud_sun_transmittance(vec3 world_pos, float local_density) {
    float sun_day = sun_luminance_from_dir(sun_info.sun_dir, 1.0);
    if (sun_day <= 0.0) {
        return 0.0;
    }

    float top = cloud_layer_top();
    float bottom = cloud_layer_bottom();
    float layer_thickness = max(top - bottom, 1.0);
    float sun_y = max(sun_info.sun_dir.y, 0.06);
    float path_len = max(top - world_pos.y, 0.0) / sun_y;

    // Start with a cheap analytic path term so lighting remains stable even when
    // optional light probes are set to one sample.
    float optical_depth = local_density * path_len * 0.040;

    uint light_steps = clamp(gui_input.cloud_light_steps, 1u, 8u);
    if (light_steps > 1u) {
        float probe_dt = min(layer_thickness * 0.18, path_len / float(light_steps));
        vec3 probe_pos = world_pos;
        for (uint i = 1u; i < light_steps; ++i) {
            probe_pos += sun_info.sun_dir * probe_dt;
            optical_depth += cloud_density_low_quality(probe_pos) * probe_dt * 0.028;
        }
    }

    return exp(-optical_depth * gui_input.cloud_absorption);
}

float cloud_hg_phase(float cos_theta, float g) {
    float g2 = g * g;
    float denom = max(1.0 + g2 - 2.0 * g * cos_theta, 1.0e-3);
    return (1.0 - g2) / (4.0 * CLOUD_PI * denom * sqrt(denom));
}

vec3 cloud_sample_lighting(vec3 world_pos, vec3 view_dir, float density) {
    float sun_day = sun_luminance_from_dir(sun_info.sun_dir, 1.0);
    float sun_trans = cloud_sun_transmittance(world_pos, density);
    float cos_theta = clamp(dot(view_dir, sun_info.sun_dir), -1.0, 1.0);
    float g = clamp(gui_input.cloud_phase_eccentricity, 0.0, 0.92);
    float phase_forward = cloud_hg_phase(cos_theta, g) / max(cloud_hg_phase(1.0, g), 1.0e-3);
    float phase_back = cloud_hg_phase(cos_theta, -0.22) / max(cloud_hg_phase(-1.0, -0.22), 1.0e-3);
    float phase = 0.32 + 1.55 * phase_forward + 0.25 * phase_back;

    float powder = 1.0 - exp(-density * 2.2);
    float silver = pow(max(cos_theta, 0.0), 18.0) * gui_input.cloud_silver_intensity;

    vec3 ambient_sky = get_sky_color(normalize(vec3(view_dir.x, abs(view_dir.y) + 0.25, view_dir.z)),
                                     sun_info.sun_dir);
    float twilight = smoothstep(-0.08, 0.22, sun_info.sun_dir.y);
    vec3 ambient = ambient_sky * mix(0.38, 0.62, twilight) * (0.55 + 0.45 * sun_day);

    vec3 sun_light = sun_info.sun_color * sun_info.sun_luminance * sun_day * sun_trans;
    sun_light *= phase * (0.52 + 0.55 * powder);
    sun_light += sun_info.sun_color * sun_info.sun_display_luminance * sun_day * sun_trans * silver;

    return ambient + sun_light;
}

CloudRaymarchResult compute_volumetric_clouds_with_jitter(vec3 view_dir, uint primary_step_override,
                                                        float jitter) {
    CloudRaymarchResult result;
    result.color = vec3(0.0);
    result.alpha = 0.0;

    float t0;
    float t1;
    if (!cloud_layer_interval(view_dir, t0, t1)) {
        return result;
    }

    uint configured_steps = clamp(gui_input.cloud_primary_steps, 4u, 64u);
    uint steps = primary_step_override == 0u ? configured_steps : clamp(primary_step_override, 3u, 64u);
    float march_len = t1 - t0;
    float dt = march_len / float(steps);

    float t = t0 + dt * fract(jitter);
    float transmittance = 1.0;

    for (uint i = 0u; i < steps; ++i) {
        vec3 world_pos = camera_info.pos.xyz + view_dir * t;
        float density = cloud_density(world_pos);
        if (density > 0.001) {
            float extinction = density * gui_input.cloud_absorption;
            float alpha = 1.0 - exp(-extinction * dt * 0.105);
            alpha = clamp(alpha, 0.0, 0.96);

            vec3 lighting = cloud_sample_lighting(world_pos, view_dir, density);
            result.color += transmittance * alpha * lighting;
            transmittance *= 1.0 - alpha;

            if (transmittance < 0.035) {
                break;
            }
        }
        t += dt;
    }

    result.alpha = clamp(1.0 - transmittance, 0.0, 1.0);
    return result;
}

CloudRaymarchResult compute_volumetric_clouds(vec3 view_dir, uint primary_step_override) {
    float jitter = hash13(vec3(view_dir.xy * 173.31 + view_dir.zz * 41.7,
                               float(env_info.frame_serial_idx & 63u)));
    return compute_volumetric_clouds_with_jitter(view_dir, primary_step_override, jitter);
}

vec3 apply_cloud_result(vec3 base_sky_color, CloudRaymarchResult clouds) {
    return mix(base_sky_color, clouds.color, clouds.alpha);
}

#endif // CLOUDS_GLSL
