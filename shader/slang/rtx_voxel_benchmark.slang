static const uint RTX_GROUP_SIZE = 64u;
static const uint INVALID_VOXEL = 0xffffffffu;
static const uint MAX_DDA_STEPS = 512u;

struct BenchmarkRay
{
    float3 origin;
    float t_min;
    float3 direction;
    float t_max;
};

struct BenchmarkResult
{
    float hit_t;
    uint voxel_index;
    uint candidate_count;
    uint rejected_candidate_count;
    uint committed_candidate_count;
    uint traversal_exhausted;
    uint primitive_index;
    uint padding;
};

struct BenchmarkPushConstants
{
    uint mode;
    uint world_dimension;
    uint macro_dimension;
    uint ray_count;
    uint candidate_budget;
};

[[vk::binding(0, 0)]]
RaytracingAccelerationStructure tlas;

[[vk::binding(1, 0)]]
StructuredBuffer<uint4> macro_data;

[[vk::binding(2, 0)]]
StructuredBuffer<uint> occupancy;

[[vk::binding(3, 0)]]
StructuredBuffer<BenchmarkRay> rays;

[[vk::binding(4, 0)]]
RWStructuredBuffer<BenchmarkResult> results;

[[vk::push_constant]]
ConstantBuffer<BenchmarkPushConstants> benchmark;

uint voxelIndex(uint3 voxel, uint dimension)
{
    return voxel.x + dimension * (voxel.y + dimension * voxel.z);
}

bool rayBox(float3 origin, float3 direction, float3 boxMin, float3 boxMax,
            float rayMin, float rayMax, out float entry, out float exit)
{
    float3 safeDirection = float3(
        abs(direction.x) < 1.0e-20 ? (direction.x < 0.0 ? -1.0e-20 : 1.0e-20) : direction.x,
        abs(direction.y) < 1.0e-20 ? (direction.y < 0.0 ? -1.0e-20 : 1.0e-20) : direction.y,
        abs(direction.z) < 1.0e-20 ? (direction.z < 0.0 ? -1.0e-20 : 1.0e-20) : direction.z);
    float3 t0 = (boxMin - origin) / safeDirection;
    float3 t1 = (boxMax - origin) / safeDirection;
    float3 nearValue = min(t0, t1);
    float3 farValue = max(t0, t1);
    entry = max(rayMin, max(nearValue.x, max(nearValue.y, nearValue.z)));
    exit = min(rayMax, min(farValue.x, min(farValue.y, farValue.z)));
    return entry <= exit;
}

bool traceVoxelRange(BenchmarkRay ray, int3 lower, int3 upper,
                     out float hitT, out float reportT,
                     out uint hitVoxel, out uint exhausted)
{
    hitT = ray.t_max;
    reportT = ray.t_max;
    hitVoxel = INVALID_VOXEL;
    exhausted = 0u;

    float entry;
    float exit;
    if (!rayBox(ray.origin, ray.direction, float3(lower), float3(upper),
                ray.t_min, ray.t_max, entry, exit))
    {
        return false;
    }

    float currentT = max(ray.t_min, entry);
    float3 position = ray.origin + ray.direction * (currentT + 1.0e-4);
    int3 voxel = int3(floor(position));
    if (any(voxel < lower) || any(voxel >= upper))
    {
        return false;
    }
    int3 step = int3(
        ray.direction.x > 0.0 ? 1 : (ray.direction.x < 0.0 ? -1 : 0),
        ray.direction.y > 0.0 ? 1 : (ray.direction.y < 0.0 ? -1 : 0),
        ray.direction.z > 0.0 ? 1 : (ray.direction.z < 0.0 ? -1 : 0));
    float3 deltaT = float3(
        step.x == 0 ? 1.0e30 : abs(1.0 / ray.direction.x),
        step.y == 0 ? 1.0e30 : abs(1.0 / ray.direction.y),
        step.z == 0 ? 1.0e30 : abs(1.0 / ray.direction.z));
    float3 boundary = float3(
        step.x > 0 ? voxel.x + 1 : voxel.x,
        step.y > 0 ? voxel.y + 1 : voxel.y,
        step.z > 0 ? voxel.z + 1 : voxel.z);
    float3 nextT = float3(
        step.x == 0 ? 1.0e30 : (boundary.x - ray.origin.x) / ray.direction.x,
        step.y == 0 ? 1.0e30 : (boundary.y - ray.origin.y) / ray.direction.y,
        step.z == 0 ? 1.0e30 : (boundary.z - ray.origin.z) / ray.direction.z);

    for (uint ddaStep = 0u; ddaStep < MAX_DDA_STEPS; ++ddaStep)
    {
        if (any(voxel < lower) || any(voxel >= upper) || currentT > exit)
        {
            return false;
        }
        uint index = voxelIndex(uint3(voxel), benchmark.world_dimension);
        if (occupancy[index] != 0u)
        {
            float voxelExitT = min(exit, min(nextT.x, min(nextT.y, nextT.z)));
            if (voxelExitT > currentT + 1.0e-6)
            {
                hitT = currentT;
                reportT = 0.5 * (currentT + voxelExitT);
                hitVoxel = index;
                return true;
            }
        }

        float advanceT = min(nextT.x, min(nextT.y, nextT.z));
        bool3 tied = nextT <= advanceT + 1.0e-5;
        if (tied.x) { voxel.x += step.x; nextT.x += deltaT.x; }
        if (tied.y) { voxel.y += step.y; nextT.y += deltaT.y; }
        if (tied.z) { voxel.z += step.z; nextT.z += deltaT.z; }
        currentT = advanceT;
    }

    exhausted = 1u;
    return false;
}

BenchmarkResult traceSoftware(BenchmarkRay ray)
{
    BenchmarkResult result = {};
    result.hit_t = ray.t_max;
    result.voxel_index = INVALID_VOXEL;
    result.primitive_index = INVALID_VOXEL;
    float ignoredReportT;
    traceVoxelRange(ray, int3(0), int3(benchmark.world_dimension), result.hit_t,
                    ignoredReportT, result.voxel_index, result.traversal_exhausted);
    return result;
}

BenchmarkResult traceHardware(BenchmarkRay ray)
{
    BenchmarkResult result = {};
    result.hit_t = ray.t_max;
    result.voxel_index = INVALID_VOXEL;
    result.primitive_index = INVALID_VOXEL;
    float closestExactHitT = ray.t_max;
    float closestReportedHitT = ray.t_max;
    uint closestExactVoxel = INVALID_VOXEL;
    uint closestExactPrimitive = INVALID_VOXEL;

    RayDesc rayDesc;
    rayDesc.Origin = ray.origin;
    rayDesc.Direction = ray.direction;
    rayDesc.TMin = ray.t_min;
    rayDesc.TMax = ray.t_max;
    RayQuery<RAY_FLAG_NONE> query;
    query.TraceRayInline(tlas, RAY_FLAG_NONE, 0xffu, rayDesc);
    while (query.Proceed())
    {
        if (query.CandidateType() != CANDIDATE_PROCEDURAL_PRIMITIVE)
        {
            continue;
        }
        result.candidate_count += 1u;
        if (result.candidate_count > benchmark.candidate_budget)
        {
            result.traversal_exhausted = 1u;
            query.Abort();
            break;
        }

        uint primitiveIndex = query.CandidatePrimitiveIndex();
        uint4 macro = macro_data[primitiveIndex];
        int3 lower = int3(macro.xyz);
        int3 upper = min(lower + int3(benchmark.macro_dimension),
                         int3(benchmark.world_dimension));
        float hitT;
        float reportT;
        uint hitVoxel;
        uint ddaExhausted;
        if (traceVoxelRange(ray, lower, upper, hitT, reportT,
                            hitVoxel, ddaExhausted))
        {
            if (hitT < closestExactHitT)
            {
                closestExactHitT = hitT;
                closestReportedHitT = reportT;
                closestExactVoxel = hitVoxel;
                closestExactPrimitive = primitiveIndex;
            }
            // A generated intersection must be inside both the macro AABB and the occupied
            // voxel. Reporting their entry boundary can round outside the candidate interval.
            query.CommitProceduralPrimitiveHit(reportT);
            result.committed_candidate_count += 1u;
        }
        else
        {
            result.rejected_candidate_count += 1u;
            result.traversal_exhausted |= ddaExhausted;
        }
    }

    if (closestExactVoxel != INVALID_VOXEL)
    {
        result.hit_t = closestExactHitT;
        result.voxel_index = closestExactVoxel;
        result.primitive_index = closestExactPrimitive;
    }
    if (query.CommittedStatus() == COMMITTED_PROCEDURAL_PRIMITIVE_HIT)
    {
        bool disagrees = query.CommittedPrimitiveIndex() != closestExactPrimitive ||
            abs(query.CommittedRayT() - closestReportedHitT) > 1.0e-3;
        result.padding = disagrees ? 1u : 0u;
    }
    return result;
}

[shader("compute")]
[numthreads(RTX_GROUP_SIZE, 1, 1)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    uint rayIndex = dispatchThreadId.x;
    if (rayIndex >= benchmark.ray_count)
    {
        return;
    }
    BenchmarkRay ray = rays[rayIndex];
    results[rayIndex] = benchmark.mode == 0u
        ? traceSoftware(ray)
        : traceHardware(ray);
}
