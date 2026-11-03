module local_light_field_query;

import ddgi_query;
import local_lighting;
import local_emitter;
import local_light_field_policy;

// Analytic receiver term is shared by the field consumer and its CPU guardrail.
public float3 localLightReceiverSample(
    U_LightGpu light, float3 position, float3 normal, uint tap, out uint fixedTap)
{
    fixedTap = 0u;
    if (light.abi_version != LOCAL_LIGHT_GPU_ABI_VERSION || light.kind != LOCAL_LIGHT_KIND_POINT)
        return float3(0.0);
    if (light.shape_params.w > 0.5)
    {
        fixedTap = localEmitterFixedSampleIndex(light.position, position, tap);
        LocalEmitterSample s = sampleLocalEmitter(light.position,
            light.shape_params.xyz, light.range, position, normal, tap);
        return light.color * light.intensity * s.irradiance_per_intensity;
    }
    return tap == 0u ? evaluateUnshadowedPointLight(light, position, normal).irradiance : float3(0.0);
}

public bool localLightFieldReceiverAuditEnabled(StructuredBuffer<uint2> field)
{
    uint count, stride;
    field.GetDimensions(count, stride);
    if (count < 2u) return false;
    return field[0].x == LOCAL_LIGHT_FIELD_ABI && (field[1].y >> 16u) != 0u;
}

public bool trySampleLocalLightConnectionResponse(
    DdgiReceiverProbeSupport support, StructuredBuffer<uint2> field,
    U_LocalLightInfo info, U_LightGpu light, uint lightIndex,
    float3 position, float3 normal, out float3 response)
{
    response = float3(0.0);
    bool finiteEmitter = light.shape_params.w > 0.5;
    uint taps = finiteEmitter ? LOCAL_EMITTER_SAMPLE_COUNT : 1u;
    uint count = localLightCount(info);
    uint records, stride;
    field.GetDimensions(records, stride);
    if (records < 2u) return false;
    if (field[0].x != LOCAL_LIGHT_FIELD_ABI ||
        field[0].y != support.probe_count || field[1].x != count ||
        (field[1].y & 65535u) != LOCAL_LIGHT_FIELD_CONNECTIONS) return false;
    if (light.abi_version != LOCAL_LIGHT_GPU_ABI_VERSION ||
        light.kind != LOCAL_LIGHT_KIND_POINT) return false;
    // Range/intensity/cosine remain exact at the receiver, including a source
    // radius clamp, range crossings and normals between directional texels.
    float3 unshadowed = float3(0.0);
    for (uint tap = 0u; tap < taps; ++tap)
    {
        uint ignored;
        unshadowed += localLightReceiverSample(light, position, normal, tap, ignored);
    }
    if (!any(unshadowed > float3(0.0))) return true;
    float extent = finiteEmitter ? length(light.shape_params.xyz) : light.source_radius;
    if (!localLightOutsideReceiverSupport(position, light.position, extent, support.radius_squared))
        return false;

    for (uint tap = 0u; tap < taps; ++tap)
    {
        uint fixedTap;
        float3 energy = localLightReceiverSample(light, position, normal, tap, fixedTap);
        if (!any(energy > float3(0.0))) continue;
        LocalLightTransmissionSupport transmissionSupport = emptyLocalLightTransmissionSupport();
        for (uint i = 0u; i < 8u; ++i)
        {
            if (support.weights[i] <= 1.0e-6) continue;
            uint index = localLightConnectionIndex(support.probes[i], lightIndex, count, fixedTap);
            if (index >= records) return false;
            accumulateLocalLightTransmission(transmissionSupport, support.weights[i],
                unpackLocalLightTransmission(field[index]));
        }
        float3 transmission;
        if (!finishLocalLightTransmission(transmissionSupport, support.weight, transmission))
            return false;
        response += energy * transmission;
    }
    return true;
}
