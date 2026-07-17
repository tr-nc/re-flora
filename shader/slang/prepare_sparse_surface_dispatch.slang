import solid_workgroups;
import sparse_surface_build;
import surface_build;

static const uint PREPARE_WORKGROUP_SIZE = 128u;
static const uint SURFACE_WORKGROUP_SIZE = 8u;

[[vk::binding(0, 0)]]
ConstantBuffer<U_MakeSurfaceInfo> make_surface_info;

[[vk::binding(1, 0)]]
GLSLShaderStorageBuffer<B_MakeSurfaceResult, Std430DataLayout> make_surface_result;

[[vk::binding(2, 0)]]
readonly GLSLShaderStorageBuffer<B_SolidWorkgroupFlags, Std430DataLayout>
    solid_workgroup_flags;

[[vk::binding(3, 0)]]
writeonly GLSLShaderStorageBuffer<B_SurfaceSolidWorkgroupIndices, Std430DataLayout>
    surface_solid_workgroup_indices;

[[vk::binding(4, 0)]]
GLSLShaderStorageBuffer<B_SurfaceSolidWorkgroupDispatchIndirect, Std430DataLayout>
    surface_solid_workgroup_dispatch_indirect;

[[vk::binding(5, 0)]]
[[vk::image_format("r8ui")]]
readonly RWTexture3D<uint> chunk_atlas;

[shader("compute")]
[numthreads(PREPARE_WORKGROUP_SIZE, 1, 1)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    uint3 localGridDimension =
        (make_surface_info.atlas_read_dim + uint3(SURFACE_WORKGROUP_SIZE - 1u)) /
        SURFACE_WORKGROUP_SIZE;
    uint localWorkgroupCount =
        localGridDimension.x * localGridDimension.y * localGridDimension.z;

    uint localIndex = dispatchThreadId.x;
    if (localIndex >= localWorkgroupCount)
    {
        return;
    }

    if (localIndex == 0u)
    {
        surface_solid_workgroup_dispatch_indirect.dispatch_y = 1u;
        surface_solid_workgroup_dispatch_indirect.dispatch_z = 1u;
    }

    uint3 localWorkgroupCoordinate = uint3(
        localIndex % localGridDimension.x,
        localIndex / (localGridDimension.x * localGridDimension.z),
        (localIndex / localGridDimension.x) % localGridDimension.z);
    uint3 globalWorkgroupCoordinate =
        (make_surface_info.atlas_read_offset / SURFACE_WORKGROUP_SIZE) +
        localWorkgroupCoordinate;

    uint atlasWidth;
    uint atlasHeight;
    uint atlasDepth;
    chunk_atlas.GetDimensions(atlasWidth, atlasHeight, atlasDepth);
    uint globalWorkgroupIndex = solidWorkgroupIndexFromCoordinate(
        globalWorkgroupCoordinate,
        solidWorkgroupGridDimension(uint3(atlasWidth, atlasHeight, atlasDepth)));

    uint flagWord = globalWorkgroupIndex >> 5u;
    uint flagMask = 1u << (globalWorkgroupIndex & 31u);
    if ((solid_workgroup_flags.data[flagWord] & flagMask) == 0u)
    {
        return;
    }

    uint writeIndex;
    InterlockedAdd(make_surface_result.solid_workgroup_len, 1u, writeIndex);
    surface_solid_workgroup_indices.data[writeIndex] = localIndex;

    // The command buffer clears dispatch_x before this pass. InterlockedMax lets all
    // invocations prepare the indirect dispatch without a second finalize pass.
    uint previousDispatchCount;
    InterlockedMax(surface_solid_workgroup_dispatch_indirect.dispatch_x,
                   writeIndex + 1u, previousDispatchCount);
}
