#ifndef VSM_FILTERING_GLSL
#define VSM_FILTERING_GLSL

const int VSM_FILTER_HALF_SIZE = 5;

// Pre-calculated 1D Gaussian weights for a kernel of size 11 (half_size = 5).
// These weights are normalized. They were generated with a sigma of approximately
// (VSM_FILTER_HALF_SIZE / 2.0). The array stores weights for the center (index 0) up to the edge
// (index 5).
const float gaussian_weights[6] = float[](0.227027,  // center (offset 0)
                                          0.1945946, // offset 1
                                          0.1216216, // offset 2
                                          0.054054,  // offset 3
                                          0.016216,  // offset 4
                                          0.003378   // offset 5
);

// read from ping
#ifdef VSM_FILTER_H
vec2 get_filtered_vsm(ivec2 uvi) {
    vec2 filtered      = vec2(0.0, 0.0);
    float total_weight = 0.0;

    // Do a horizontal Gaussian blur
    for (int i = -VSM_FILTER_HALF_SIZE; i <= VSM_FILTER_HALF_SIZE; i++) {
        // Check if the sample is within the texture bounds
        if (uvi.x + i < 0 || uvi.x + i >= imageSize(shadow_map_tex_for_vsm_ping).x) {
            continue;
        }

        // Get the weight for the current sample from our pre-calculated array.
        // abs(i) is used because the Gaussian kernel is symmetric.
        float weight = gaussian_weights[abs(i)];

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

    // Do a vertical Gaussian blur
    for (int i = -VSM_FILTER_HALF_SIZE; i <= VSM_FILTER_HALF_SIZE; i++) {
        // Check if the sample is within the texture bounds
        if (uvi.y + i < 0 || uvi.y + i >= imageSize(shadow_map_tex_for_vsm_pong).y) {
            continue;
        }

        // Get the weight for the current sample from our pre-calculated array.
        float weight = gaussian_weights[abs(i)];

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
