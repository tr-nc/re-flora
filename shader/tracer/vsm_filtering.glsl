#ifndef VSM_FILTERING_GLSL
#define VSM_FILTERING_GLSL

const int VSM_FILTER_HALF_SIZE = 10;

float get_gaussian_weight(float offset, float sigma) {
    // If sigma is zero (for a 0-sized kernel), only the center pixel gets weight.
    if (sigma <= 0.0) {
        return (offset == 0.0) ? 1.0 : 0.0;
    }
    // The Gaussian function: G(x) = exp( -(x^2) / (2 * sigma^2) )
    return exp(-(offset * offset) / (2.0 * sigma * sigma));
}

// read from ping
#ifdef VSM_FILTER_H
vec2 get_filtered_vsm(ivec2 uvi) {
    vec2 filtered      = vec2(0.0, 0.0);
    float total_weight = 0.0;

    // Sigma controls the "spread" of the blur. A larger sigma gives a softer blur.
    // A common practice is to base it on the kernel size.
    float sigma = float(VSM_FILTER_HALF_SIZE) / 2.0;

    // Do a horizontal Gaussian blur
    for (int i = -VSM_FILTER_HALF_SIZE; i <= VSM_FILTER_HALF_SIZE; i++) {
        // Check if the sample is within the texture bounds
        if (uvi.x + i < 0 || uvi.x + i >= imageSize(shadow_map_tex_for_vsm_ping).x) {
            continue;
        }

        // Calculate the weight dynamically instead of using an array lookup
        float weight = get_gaussian_weight(float(i), sigma);

        vec2 sample_value = imageLoad(shadow_map_tex_for_vsm_ping, uvi + ivec2(i, 0)).xy;

        // Apply the weight to the sampled value
        filtered += sample_value * weight;
        // Accumulate the weight for final normalization (handles edges correctly)
        total_weight += weight;
    }

    // Normalize the result by the sum of weights actually used.
    // This prevents darkening at the image edges.
    if (total_weight > 0.0) {
        return filtered / total_weight;
    }
    return filtered;
}
#endif // VSM_FILTER_H

#ifdef VSM_FILTER_V
// read from pong
vec2 get_filtered_vsm(ivec2 uvi) {
    vec2 filtered      = vec2(0.0, 0.0);
    float total_weight = 0.0;

    // Calculate sigma based on the kernel size
    float sigma = float(VSM_FILTER_HALF_SIZE) / 2.0;

    // Do a vertical Gaussian blur
    for (int i = -VSM_FILTER_HALF_SIZE; i <= VSM_FILTER_HALF_SIZE; i++) {
        // Check if the sample is within the texture bounds
        if (uvi.y + i < 0 || uvi.y + i >= imageSize(shadow_map_tex_for_vsm_pong).y) {
            continue;
        }

        // Calculate the weight dynamically
        float weight = get_gaussian_weight(float(i), sigma);

        vec2 sample_value = imageLoad(shadow_map_tex_for_vsm_pong, uvi + ivec2(0, i)).xy;

        // Apply the weight to the sampled value
        filtered += sample_value * weight;
        // Accumulate the weight for final normalization
        total_weight += weight;
    }

    // Normalize the result
    if (total_weight > 0.0) {
        return filtered / total_weight;
    }
    return filtered;
}
#endif // VSM_FILTER_V

#endif // VSM_FILTERING_GLSL
