// PROTOTYPE/TRACER BULLET: throwaway static traversal ceiling experiment.

static const uint GROUP_SIZE = 64u;
static const uint INVALID_INDEX = 0xffffffffu;
static const uint MAX_DDA_STEPS = 256u;

struct BulletRay
{
    float3 origin;
    float t_min;
    float3 direction;
    float t_max;
};

struct BulletResult
{
    float hit_t;
    uint voxel_index;
    uint face_index;
    uint normal_code;
    uint primitive_index;
    uint candidate_count;
    uint rejected_candidate_count;
    uint generated_candidate_count;
    uint committed_candidate_count;
    uint traversal_exhausted;
    uint committed_disagreement;
    uint confirmed_candidate_count;
};

struct BulletPushConstants
{
    uint mode;
    uint world_dimension;
    uint ray_count;
    uint candidate_budget;
};

[[vk::binding(0, 0)]]
RaytracingAccelerationStructure triangle_tlas;

[[vk::binding(1, 0)]]
RaytracingAccelerationStructure aabb_tlas;

[[vk::binding(2, 0)]]
StructuredBuffer<uint4> triangle_faces;

[[vk::binding(3, 0)]]
StructuredBuffer<uint> surface_voxels;

[[vk::binding(4, 0)]]
StructuredBuffer<uint> occupancy;

[[vk::binding(5, 0)]]
StructuredBuffer<BulletRay> rays;

[[vk::binding(6, 0)]]
RWStructuredBuffer<BulletResult> results;

[[vk::push_constant]]
ConstantBuffer<BulletPushConstants> benchmark;

uint voxelIndex(uint3 voxel)
{
    uint dimension = benchmark.world_dimension;
    return voxel.x + dimension * (voxel.y + dimension * voxel.z);
}

uint3 voxelCoordinate(uint index)
{
    uint dimension = benchmark.world_dimension;
    uint plane = dimension * dimension;
    uint z = index / plane;
    uint remainder = index - z * plane;
    uint y = remainder / dimension;
    return uint3(remainder - y * dimension, y, z);
}

uint entryFace(uint axis, float direction)
{
    return axis * 2u + (direction < 0.0 ? 1u : 0u);
}

bool rayBox(BulletRay ray, float3 box_min, float3 box_max,
            out float entry, out float exit, out uint face)
{
    float3 safe_direction = float3(
        abs(ray.direction.x) < 1.0e-20 ? (ray.direction.x < 0.0 ? -1.0e-20 : 1.0e-20) : ray.direction.x,
        abs(ray.direction.y) < 1.0e-20 ? (ray.direction.y < 0.0 ? -1.0e-20 : 1.0e-20) : ray.direction.y,
        abs(ray.direction.z) < 1.0e-20 ? (ray.direction.z < 0.0 ? -1.0e-20 : 1.0e-20) : ray.direction.z);
    float3 t0 = (box_min - ray.origin) / safe_direction;
    float3 t1 = (box_max - ray.origin) / safe_direction;
    float3 near_value = min(t0, t1);
    float3 far_value = max(t0, t1);
    float raw_entry = max(near_value.x, max(near_value.y, near_value.z));
    entry = max(ray.t_min, raw_entry);
    exit = min(ray.t_max, min(far_value.x, min(far_value.y, far_value.z)));
    face = INVALID_INDEX;
    if (raw_entry >= ray.t_min)
    {
        uint axis = near_value.x >= near_value.y && near_value.x >= near_value.z
            ? 0u : (near_value.y >= near_value.z ? 1u : 2u);
        face = entryFace(axis, ray.direction[axis]);
    }
    return entry <= exit;
}

bool traceGrid(BulletRay ray, out float hit_t, out uint hit_voxel,
               out uint hit_face, out uint exhausted)
{
    hit_t = ray.t_max;
    hit_voxel = INVALID_INDEX;
    hit_face = INVALID_INDEX;
    exhausted = 0u;

    float entry;
    float exit;
    uint current_face;
    float dimension = float(benchmark.world_dimension);
    if (!rayBox(ray, float3(0.0), float3(dimension), entry, exit, current_face))
    {
        return false;
    }

    float current_t = entry;
    float3 position = ray.origin + ray.direction * (current_t + 1.0e-4);
    int3 voxel = int3(floor(position));
    int dimension_i = int(benchmark.world_dimension);
    if (any(voxel < int3(0)) || any(voxel >= int3(dimension_i)))
    {
        return false;
    }
    int3 step = int3(
        ray.direction.x > 0.0 ? 1 : (ray.direction.x < 0.0 ? -1 : 0),
        ray.direction.y > 0.0 ? 1 : (ray.direction.y < 0.0 ? -1 : 0),
        ray.direction.z > 0.0 ? 1 : (ray.direction.z < 0.0 ? -1 : 0));
    float3 delta_t = float3(
        step.x == 0 ? 1.0e30 : abs(1.0 / ray.direction.x),
        step.y == 0 ? 1.0e30 : abs(1.0 / ray.direction.y),
        step.z == 0 ? 1.0e30 : abs(1.0 / ray.direction.z));
    float3 boundary = float3(
        step.x > 0 ? voxel.x + 1 : voxel.x,
        step.y > 0 ? voxel.y + 1 : voxel.y,
        step.z > 0 ? voxel.z + 1 : voxel.z);
    float3 next_t = float3(
        step.x == 0 ? 1.0e30 : (boundary.x - ray.origin.x) / ray.direction.x,
        step.y == 0 ? 1.0e30 : (boundary.y - ray.origin.y) / ray.direction.y,
        step.z == 0 ? 1.0e30 : (boundary.z - ray.origin.z) / ray.direction.z);

    for (uint dda_step = 0u; dda_step < MAX_DDA_STEPS; ++dda_step)
    {
        if (any(voxel < int3(0)) || any(voxel >= int3(dimension_i)) || current_t > exit)
        {
            return false;
        }
        uint index = voxelIndex(uint3(voxel));
        if (occupancy[index] != 0u)
        {
            hit_t = current_t;
            hit_voxel = index;
            hit_face = current_face;
            return true;
        }

        float advance_t = min(next_t.x, min(next_t.y, next_t.z));
        uint advance_axis = next_t.x <= next_t.y && next_t.x <= next_t.z
            ? 0u : (next_t.y <= next_t.z ? 1u : 2u);
        bool3 tied = next_t == advance_t;
        if (tied.x) { voxel.x += step.x; next_t.x += delta_t.x; }
        if (tied.y) { voxel.y += step.y; next_t.y += delta_t.y; }
        if (tied.z) { voxel.z += step.z; next_t.z += delta_t.z; }
        current_t = advance_t;
        current_face = entryFace(advance_axis, ray.direction[advance_axis]);
    }

    exhausted = 1u;
    return false;
}

BulletResult traceSoftware(BulletRay ray)
{
    BulletResult result = {};
    result.hit_t = ray.t_max;
    result.voxel_index = INVALID_INDEX;
    result.face_index = INVALID_INDEX;
    result.normal_code = INVALID_INDEX;
    result.primitive_index = INVALID_INDEX;
    if (traceGrid(ray, result.hit_t, result.voxel_index,
                  result.face_index, result.traversal_exhausted))
    {
        result.normal_code = result.face_index;
    }
    return result;
}

BulletResult traceAabb(BulletRay ray)
{
    BulletResult result = {};
    result.hit_t = ray.t_max;
    result.voxel_index = INVALID_INDEX;
    result.face_index = INVALID_INDEX;
    result.normal_code = INVALID_INDEX;
    result.primitive_index = INVALID_INDEX;
    float closest_t = ray.t_max;
    uint closest_primitive = INVALID_INDEX;

    RayDesc ray_desc;
    ray_desc.Origin = ray.origin;
    ray_desc.Direction = ray.direction;
    ray_desc.TMin = ray.t_min;
    ray_desc.TMax = ray.t_max;
    RayQuery<RAY_FLAG_NONE> query;
    query.TraceRayInline(aabb_tlas, RAY_FLAG_NONE, 0xffu, ray_desc);
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
        uint primitive = query.CandidatePrimitiveIndex();
        uint voxel = surface_voxels[primitive];
        uint3 coordinate = voxelCoordinate(voxel);
        float entry;
        float exit;
        uint face;
        if (rayBox(ray, float3(coordinate), float3(coordinate + 1u), entry, exit, face) &&
            exit > entry + 1.0e-6)
        {
            result.confirmed_candidate_count += 1u;
            float report_t = entry + min(1.0e-6, 0.25 * (exit - entry));
            if (report_t < closest_t)
            {
                closest_t = report_t;
                closest_primitive = primitive;
                query.CommitProceduralPrimitiveHit(report_t);
                result.generated_candidate_count += 1u;
            }
        }
        else
        {
            result.rejected_candidate_count += 1u;
        }
    }

    if (query.CommittedStatus() == COMMITTED_PROCEDURAL_PRIMITIVE_HIT)
    {
        uint primitive = query.CommittedPrimitiveIndex();
        uint voxel = surface_voxels[primitive];
        uint3 coordinate = voxelCoordinate(voxel);
        float entry;
        float exit;
        uint face;
        bool exact = rayBox(ray, float3(coordinate), float3(coordinate + 1u),
                            entry, exit, face) && exit > entry + 1.0e-6;
        if (exact)
        {
            result.hit_t = entry;
            result.voxel_index = voxel;
            result.face_index = face;
            result.normal_code = face;
            result.primitive_index = primitive;
            result.committed_candidate_count = 1u;
        }
        result.committed_disagreement = !exact || primitive != closest_primitive ? 1u : 0u;
    }
    else if (closest_primitive != INVALID_INDEX)
    {
        result.committed_disagreement = 1u;
    }
    return result;
}

BulletResult traceTriangles(BulletRay ray)
{
    BulletResult result = {};
    result.hit_t = ray.t_max;
    result.voxel_index = INVALID_INDEX;
    result.face_index = INVALID_INDEX;
    result.normal_code = INVALID_INDEX;
    result.primitive_index = INVALID_INDEX;

    RayDesc ray_desc;
    ray_desc.Origin = ray.origin;
    ray_desc.Direction = ray.direction;
    ray_desc.TMin = ray.t_min;
    ray_desc.TMax = ray.t_max;
    // A closest-hit identity oracle cannot use ACCEPT_FIRST_HIT_AND_END_SEARCH:
    // that flag is an any-hit early exit and does not promise the nearest primitive.
    RayQuery<RAY_FLAG_FORCE_OPAQUE> query;
    query.TraceRayInline(triangle_tlas, RAY_FLAG_NONE, 0xffu, ray_desc);
    while (query.Proceed()) {}
    if (query.CommittedStatus() == COMMITTED_TRIANGLE_HIT)
    {
        uint primitive = query.CommittedPrimitiveIndex();
        uint4 face = triangle_faces[primitive >> 1u];
        result.hit_t = query.CommittedRayT();
        result.voxel_index = face.x;
        result.face_index = face.y;
        result.normal_code = face.z;
        result.primitive_index = primitive;
        result.committed_candidate_count = 1u;
    }
    return result;
}

[shader("compute")]
[numthreads(GROUP_SIZE, 1, 1)]
void main(uint3 dispatch_thread_id : SV_DispatchThreadID)
{
    uint ray_index = dispatch_thread_id.x;
    if (ray_index >= benchmark.ray_count)
    {
        return;
    }
    BulletRay ray = rays[ray_index];
    BulletResult result = benchmark.mode == 0u
        ? traceSoftware(ray)
        : (benchmark.mode == 1u ? traceAabb(ray) : traceTriangles(ray));
    results[ray_index] = result;
}
