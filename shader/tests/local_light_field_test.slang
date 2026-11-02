import local_light_math;
import local_emitter;
import ddgi_octahedral;

bool close(float a, float b) { return abs(a-b) <= 1.0e-5 * max(1.0, abs(b)); }
export __extern_cpp int main()
{
    // Point units, clamp, finite support and cosine: no implicit pi or ray count.
    bool point = close(pointLightIncidentWeight(4.0, 0.1, 100000.0), 0.25) &&
        close(pointLightIncidentWeight(0.01, 0.5, 100000.0), 4.0) &&
        pointLightIncidentWeight(100.0, 0.1, 10.0) == 0.0 &&
        pointLightIncidentWeight(0.0, 0.1, 10.0) == 0.0;
    float3 direction = float3(0,1,0);
    float3 incident = float3(4,2,1);
    bool cosine = all(projectLocalLightResponse(incident, direction, direction) == incident) &&
        all(projectLocalLightResponse(incident, direction, -direction) == float3(0.0)) &&
        all(projectLocalLightResponse(incident, direction, float3(1,0,0)) == float3(0.0));
    // Each colored-glass connection is attenuated before projecting/summing.
    float3 transmittance = float3(0.1,0.5,0.8);
    bool glass = all(abs(projectLocalLightResponse(incident * transmittance, direction, direction) -
        float3(0.4,1.0,0.8)) < float3(1e-6));
    bool opaque = all(projectLocalLightResponse(float3(0), direction, direction) == float3(0));
    // Same finite shape/taps, many receiver normals. Separate incident transport
    // must reconstruct the direct evaluator at the probe, not collapse to a point.
    bool finite = true;
    for (uint n = 0u; n < 64u; ++n)
    {
        float3 normal = octDecode((float2(n % 8u, n / 8u) + 0.5) / 4.0 - 1.0);
        float direct = 0.0;
        float projected = 0.0;
        for (uint tap = 0u; tap < LOCAL_EMITTER_SAMPLE_COUNT; ++tap)
        {
            LocalEmitterSample sample = sampleLocalEmitterIncident(float3(1,2,3), float3(.3,.2,.1), 10.0, float3(0), tap);
            float3 w = normalize(sample.position);
            float t = sample.position.x < 1.0 ? 0.25 : 0.0;
            direct += t * sampleLocalEmitter(float3(1,2,3), float3(.3,.2,.1), 10.0, float3(0), normal, tap).irradiance_per_intensity;
            projected += projectLocalLightResponse(float3(t * sample.irradiance_per_intensity), w, normal).x;
        }
        finite = finite && close(direct, projected);
    }
    // Gutter samples copy the corresponding interior response, not extended oct coordinates.
    bool gutters = true;
    for (uint t = 0u; t < 100u; ++t)
    {
        uint2 texel = uint2(t % 10u, t / 10u);
        bool border = any(texel == uint2(0u)) || any(texel == uint2(9u));
        uint2 source = border ? ddgiGutterSource(texel, 8u) : texel;
        gutters = gutters && all(source >= uint2(1)) && all(source <= uint2(8));
    }
    printf("local_light_field point=%d cosine=%d glass=%d opaque=%d finite=%d gutters=%d\n", point,cosine,glass,opaque,finite,gutters);
    return point && cosine && glass && opaque && finite && gutters ? 0 : 1;
}
