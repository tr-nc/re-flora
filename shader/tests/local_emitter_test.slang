import local_emitter;
import terrain_ray_origin;

float integral(float3 center, float3 halfExtents, float3 receiver, float3 normal,
               float range, uint visibilityMode)
{
    float energy = 0.0;
    for (uint i = 0u; i < LOCAL_EMITTER_SAMPLE_COUNT; ++i)
    {
        LocalEmitterSample sample = sampleLocalEmitter(center, halfExtents, range, receiver, normal, i);
        // Half-plane blocker at z=1. Trace the same finite segment to each actual
        // source sample; never filter the final color or blur across receivers.
        float t = (1.0 - receiver.z) / (sample.position.z - receiver.z);
        float x = lerp(receiver.x, sample.position.x, t);
        float visibility = visibilityMode == 1u ? (x > 0.0 ? 1.0 : 0.0) :
            visibilityMode == 2u ? 0.0 : visibilityMode == 3u ? 0.25 : 1.0;
        energy += sample.irradiance_per_intensity * visibility;
    }
    return energy;
}

export __extern_cpp int main()
{
    float3 center = float3(0,0,2);
    float3 half = float3(0.4,0.4,0.1);
    float full = integral(center, half, float3(0), float3(0,0,1), 100.0, 0u);
    float partial = integral(center, half, float3(0), float3(0,0,1), 100.0, 1u);
    bool penumbra = full > 0.0 && abs(partial / full - 0.5) < 1.0e-5;
    bool opaque = integral(center, half, float3(0), float3(0,0,1), 100.0, 2u) == 0.0;
    bool glass = abs(integral(center, half, float3(0), float3(0,0,1), 100.0, 3u) / full - 0.25) < 1.0e-5;
    bool backface = integral(center, half, float3(0), float3(0,0,-1), 100.0, 0u) == 0.0;
    bool range = integral(center, half, float3(0), float3(0,0,1), 1.0, 0u) == 0.0;

    // Far field of a cube on one axis: I=L*(surface/4)=L*6*h*h,
    // while that receiver sees a single 4*h*h face. It must approach 2I/(3d²),
    // not one unweighted point per tap and not renormalised visible-only power.
    float far = integral(float3(0,0,20), float3(0.01), float3(0), float3(0,0,1), 10000.0, 0u);
    bool energy = abs(far / (2.0 / (3.0 * 20.0 * 20.0)) - 1.0) < 0.002;
    float rotated = integral(float3(20,0,0), float3(0.01), float3(0), float3(1,0,0), 10000.0, 0u);
    bool rotation = abs(rotated / far - 1.0) < 1.0e-5;
    bool finite = true;
    for (uint i = 0u; i < LOCAL_EMITTER_SAMPLE_COUNT; ++i)
    {
        LocalEmitterSample s = sampleLocalEmitter(center, half, 100.0, float3(0), float3(0,0,1), i);
        float3 offset = abs(s.position - center);
        finite = finite && all(offset <= half + float3(1.0e-6)) &&
            abs(dot(s.position - center, s.normal) - dot(half, abs(s.normal))) < 1.0e-6;
    }
    // Canonical terrain receiver depends on stored voxel center/normal only;
    // every camera hit in that voxel gets exactly the same stable integration.
    float3 voxelCenter = float3(0.5,0.5,0.5) / 256.0;
    float3 receiver = terrainVoxelSurfacePositionAlongNormal(voxelCenter, float3(0,0,1));
    float first = integral(center, half, receiver, float3(0,0,1), 100.0, 1u);
    bool stable = true;
    for (uint i = 0u; i < 32u; ++i)
        stable = stable && integral(center, half, receiver, float3(0,0,1), 100.0, 1u) == first;
    printf("finite_emitter penumbra=%d ratio=%f opaque=%d glass=%d backface=%d range=%d energy=%d rotation=%d bounds=%d stable=%d\n",
        penumbra, partial / full, opaque, glass, backface, range, energy, rotation, finite, stable);
    return penumbra && opaque && glass && backface && range && energy && rotation && finite && stable ? 0 : 1;
}
