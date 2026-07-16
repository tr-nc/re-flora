import surface_result;

static const uint FLORA_WORKGROUP_SIZE = 128u;

struct B_ActiveSurfaceFloraDispatchIndirect
{
    uint dispatch_x;
    uint dispatch_y;
    uint dispatch_z;
};

[[vk::binding(0, 0)]]
readonly GLSLShaderStorageBuffer<B_MakeSurfaceResult, Std430DataLayout> make_surface_result;

[[vk::binding(1, 0)]]
writeonly GLSLShaderStorageBuffer<B_ActiveSurfaceFloraDispatchIndirect, Std430DataLayout>
    active_surface_flora_dispatch_indirect;

[shader("compute")]
[numthreads(1, 1, 1)]
void main()
{
    uint invocationCount = make_surface_result.active_brick_len * 64u;
    uint groupCount =
        (invocationCount + FLORA_WORKGROUP_SIZE - 1u) / FLORA_WORKGROUP_SIZE;

    // Keep the indirect dispatch valid even for empty chunks; the flora shader
    // reads active_brick_len and immediately exits when there is no work.
    active_surface_flora_dispatch_indirect.dispatch_x = max(groupCount, 1u);
    active_surface_flora_dispatch_indirect.dispatch_y = 1u;
    active_surface_flora_dispatch_indirect.dispatch_z = 1u;
}
