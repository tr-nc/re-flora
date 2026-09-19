module tree_cells;
[[vk::binding(47, 0)]] public StructuredBuffer<uint4> raster_tree_cells;

public bool isRasterTreeCell(float3 center)
{
    uint3 p = uint3(floor(center * 256.0));
    uint hash = p.x * 73856093u ^ p.y * 19349663u ^ p.z * 83492791u;
    hash ^= hash >> 16u;
    uint count, stride;
    raster_tree_cells.GetDimensions(count, stride);
    for (uint i = 0u; i < 64u; ++i) {
        uint4 cell = raster_tree_cells[(hash + i) & (count - 1u)];
        if (cell.w == 0u) return false;
        if (all(cell.xyz == p)) return true;
    }
    return false;
}
