/// Filters the raw VSM texture
/// Requires:
/// ifdef VSM_FILTER_H: shadow_map_tex_for_vsm_ping
/// ifdef VSM_FILTER_V: shadow_map_tex_for_vsm_pong

#ifndef VSM_FILTERING_GLSL
#define VSM_FILTERING_GLSL

const int VSM_FILTER_HALF_SIZE = 5;

float get_gaussian_weight(float offset, float sigma) {
    // if sigma is zero (for a 0-sized kernel), only the center pixel gets weight.
    if (sigma <= 0.0) {
        return (offset == 0.0) ? 1.0 : 0.0;
    }
    // the Gaussian function: G(x) = exp( -(x^2) / (2 * sigma^2) )
    return exp(-(offset * offset) / (2.0 * sigma * sigma));
}

// read from ping
#ifdef VSM_FILTER_H
vec4 get_filtered_vsm(ivec2 uvi) {
    vec4 filtered      = vec4(0.0);
    float total_weight = 0.0;

    // sigma controls the "spread" of the blur. A larger sigma gives a softer blur.
    // A common practice is to base it on the kernel size.
    float sigma = float(VSM_FILTER_HALF_SIZE) / 2.0;

    // do a horizontal Gaussian blur
    for (int i = -VSM_FILTER_HALF_SIZE; i <= VSM_FILTER_HALF_SIZE; i++) {
        // check if the sample is within the texture bounds
        if (uvi.x + i < 0 || uvi.x + i >= imageSize(shadow_map_tex_for_vsm_ping).x) {
            continue;
        }

        // calculate the weight dynamically instead of using an array lookup
        float weight = get_gaussian_weight(float(i), sigma);

        vec4 sample_value = imageLoad(shadow_map_tex_for_vsm_ping, uvi + ivec2(i, 0));

        // apply the weight to the sampled value
        filtered += sample_value * weight;
        // accumulate the weight for final normalization (handles edges correctly)
        total_weight += weight;
    }

    // normalize the result by the sum of weights actually used.
    // this prevents darkening at the image edges.
    if (total_weight > 0.0) {
        return filtered / total_weight;
    }
    return filtered;
}
#endif // VSM_FILTER_H

#ifdef VSM_FILTER_V
// read from pong
vec4 get_filtered_vsm(ivec2 uvi) {
    vec4 filtered      = vec4(0.0);
    float total_weight = 0.0;

    // calculate sigma based on the kernel size
    float sigma = float(VSM_FILTER_HALF_SIZE) / 2.0;

    // do a vertical Gaussian blur
    for (int i = -VSM_FILTER_HALF_SIZE; i <= VSM_FILTER_HALF_SIZE; i++) {
        // check if the sample is within the texture bounds
        if (uvi.y + i < 0 || uvi.y + i >= imageSize(shadow_map_tex_for_vsm_pong).y) {
            continue;
        }

        // calculate the weight dynamically
        float weight = get_gaussian_weight(float(i), sigma);

        vec4 sample_value = imageLoad(shadow_map_tex_for_vsm_pong, uvi + ivec2(0, i));

        // apply the weight to the sampled value
        filtered += sample_value * weight;
        // accumulate the weight for final normalization
        total_weight += weight;
    }

    // normalize the result
    if (total_weight > 0.0) {
        return filtered / total_weight;
    }
    return filtered;
}
#endif // VSM_FILTER_V

#endif // VSM_FILTERING_GLSL
