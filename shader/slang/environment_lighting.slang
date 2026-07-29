module environment_lighting;

import tracer_types;

// Sloan real-SH ordering adapted to the project's Y-up world:
// [Y00, Y1-1(z), Y10(y), Y11(x), Y2-2(xz), Y2-1(zy),
//  Y20(3y^2-1), Y21(xy), Y22(x^2-z^2)].
float environmentShBasis(uint index, float3 direction)
{
    float x = direction.x;
    float y = direction.y;
    float z = direction.z;
    switch (index)
    {
        case 0u: return 0.2820948;
        case 1u: return 0.48860252 * z;
        case 2u: return 0.48860252 * y;
        case 3u: return 0.48860252 * x;
        case 4u: return 1.0925485 * x * z;
        case 5u: return 1.0925485 * z * y;
        case 6u: return 0.31539157 * (3.0 * y * y - 1.0);
        case 7u: return 1.0925485 * x * y;
        default: return 0.54627424 * (x * x - z * z);
    }
}

public float3 sampleEnvironmentIrradiance(U_ShadingInfo lighting,
                                           float3 worldPosition,
                                           float3 surfaceNormal)
{
    // Reserved for spatial probe lookup without changing material call sites.
    (void)worldPosition;

    float normalLengthSquared = dot(surfaceNormal, surfaceNormal);
    float3 normal = normalLengthSquared > 1.0e-8
        ? surfaceNormal * rsqrt(normalLengthSquared)
        : float3(0.0, 1.0, 0.0);
    float3 irradiance = float3(0.0);
    irradiance += lighting.environment_irradiance_sh_0.xyz *
                  environmentShBasis(0u, normal);
    irradiance += lighting.environment_irradiance_sh_1.xyz *
                  environmentShBasis(1u, normal);
    irradiance += lighting.environment_irradiance_sh_2.xyz *
                  environmentShBasis(2u, normal);
    irradiance += lighting.environment_irradiance_sh_3.xyz *
                  environmentShBasis(3u, normal);
    irradiance += lighting.environment_irradiance_sh_4.xyz *
                  environmentShBasis(4u, normal);
    irradiance += lighting.environment_irradiance_sh_5.xyz *
                  environmentShBasis(5u, normal);
    irradiance += lighting.environment_irradiance_sh_6.xyz *
                  environmentShBasis(6u, normal);
    irradiance += lighting.environment_irradiance_sh_7.xyz *
                  environmentShBasis(7u, normal);
    irradiance += lighting.environment_irradiance_sh_8.xyz *
                  environmentShBasis(8u, normal);
    return max(irradiance, float3(0.0));
}
