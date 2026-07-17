import chunk_writer_types;

[[vk::binding(0, 0)]]
ConstantBuffer<U_RegionInfo> region_info;

[[vk::binding(1, 0)]]
writeonly GLSLShaderStorageBuffer<B_RegionIndirect, Std430DataLayout>
    region_indirect;

uint groupByFour(uint count)
{
    return (count + 3u) / 4u;
}

[shader("compute")]
[numthreads(1, 1, 1)]
void main()
{
    region_indirect.dispatch_x = groupByFour(region_info.dim.x);
    region_indirect.dispatch_y = groupByFour(region_info.dim.y);
    region_indirect.dispatch_z = groupByFour(region_info.dim.z);
}
