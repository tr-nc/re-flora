import chunk_writer_types;
import solid_workgroups;
import sparse_surface_build;
import voxel_data;
import voxel_types;

[[vk::binding(0, 0)]]
ConstantBuffer<U_ModelVoxelizeInfo> model_voxelize_info;

[[vk::binding(1, 0)]]
readonly GLSLShaderStorageBuffer<B_ModelTriangles, Std430DataLayout>
    model_triangles;

[[vk::binding(2, 0)]]
[[vk::image_format("r8ui")]]
RWTexture3D<uint> chunk_atlas;

[[vk::binding(3, 0)]]
GLSLShaderStorageBuffer<B_SolidWorkgroupFlags, Std430DataLayout>
    solid_workgroup_flags;

bool inAtlasBound(int3 position)
{
    uint width;
    uint height;
    uint depth;
    chunk_atlas.GetDimensions(width, height, depth);
    return all(position >= int3(0)) &&
           all(position < int3(width, height, depth));
}

float signedSolidAngle(float3 point, float3 a, float3 b, float3 c)
{
    static const float EPSILON = 1.0e-7;
    float3 va = a - point;
    float3 vb = b - point;
    float3 vc = c - point;
    float la = length(va);
    float lb = length(vb);
    float lc = length(vc);
    if (la < EPSILON || lb < EPSILON || lc < EPSILON)
    {
        return 0.0;
    }

    float numerator = dot(va, cross(vb, vc));
    float denominator = la * lb * lc + dot(va, vb) * lc +
                        dot(vb, vc) * la + dot(vc, va) * lb;
    if (abs(numerator) < EPSILON && abs(denominator) < EPSILON)
    {
        return 0.0;
    }
    return 2.0 * atan2(numerator, denominator);
}

float squaredDistancePointTriangle(float3 point,
                                   float3 a,
                                   float3 b,
                                   float3 c)
{
    float3 ab = b - a;
    float3 ac = c - a;
    float3 ap = point - a;
    float d1 = dot(ab, ap);
    float d2 = dot(ac, ap);
    if (d1 <= 0.0 && d2 <= 0.0)
    {
        return dot(ap, ap);
    }

    float3 bp = point - b;
    float d3 = dot(ab, bp);
    float d4 = dot(ac, bp);
    if (d3 >= 0.0 && d4 <= d3)
    {
        return dot(bp, bp);
    }

    float vc = d1 * d4 - d3 * d2;
    if (vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0)
    {
        float v = d1 / (d1 - d3);
        float3 delta = point - (a + v * ab);
        return dot(delta, delta);
    }

    float3 cp = point - c;
    float d5 = dot(ab, cp);
    float d6 = dot(ac, cp);
    if (d6 >= 0.0 && d5 <= d6)
    {
        return dot(cp, cp);
    }

    float vb = d5 * d2 - d1 * d6;
    if (vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0)
    {
        float w = d2 / (d2 - d6);
        float3 delta = point - (a + w * ac);
        return dot(delta, delta);
    }

    float va = d3 * d6 - d5 * d4;
    if (va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0)
    {
        float w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        float3 delta = point - (b + w * (c - b));
        return dot(delta, delta);
    }

    float3 normal = normalize(cross(ab, ac));
    float distanceFromPlane = dot(point - a, normal);
    return distanceFromPlane * distanceFromPlane;
}

void markSolidWorkgroup(uint3 voxelPosition)
{
    uint width;
    uint height;
    uint depth;
    chunk_atlas.GetDimensions(width, height, depth);
    uint workgroupIndex = solidWorkgroupIndexFromVoxel(
        voxelPosition, uint3(width, height, depth));
    uint flagWord = workgroupIndex >> 5u;
    uint flagMask = 1u << (workgroupIndex & 31u);
    uint ignored;
    InterlockedOr(solid_workgroup_flags.data[flagWord], flagMask, ignored);
}

[shader("compute")]
[numthreads(8, 8, 8)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    uint3 local = dispatchThreadId;
    if (any(local >= model_voxelize_info.dim))
    {
        return;
    }

    int3 worldVoxelPosition = int3(model_voxelize_info.offset + local);
    if (!inAtlasBound(worldVoxelPosition))
    {
        return;
    }

    float3 center = float3(worldVoxelPosition) + float3(0.5);
    static const float INSIDE_SOLID_ANGLE_THRESHOLD = 6.28318530718;
    float totalSolidAngle = 0.0;
    bool nearSurface = false;
    float surfaceThresholdSquared =
        model_voxelize_info.surface_thickness_vox *
        model_voxelize_info.surface_thickness_vox;

    for (uint i = 0u; i < model_voxelize_info.triangle_count; ++i)
    {
        uint base = i * 3u;
        float3 a = model_triangles.data[base].xyz * 256.0 +
                   model_voxelize_info.position_vox;
        float3 b = model_triangles.data[base + 1u].xyz * 256.0 +
                   model_voxelize_info.position_vox;
        float3 c = model_triangles.data[base + 2u].xyz * 256.0 +
                   model_voxelize_info.position_vox;
        totalSolidAngle += signedSolidAngle(center, a, b, c);
        if (!nearSurface && squaredDistancePointTriangle(
            center, a, b, c) <= surfaceThresholdSquared)
        {
            nearSurface = true;
        }
    }

    if (abs(totalSolidAngle) > INSIDE_SOLID_ANGLE_THRESHOLD || nearSurface)
    {
        chunk_atlas[worldVoxelPosition] = packVoxelAtlasTypeClearState(
            model_voxelize_info.fill_voxel_type);
        if (model_voxelize_info.fill_voxel_type != VOXEL_TYPE_EMPTY)
        {
            markSolidWorkgroup(uint3(worldVoxelPosition));
        }
    }
}
