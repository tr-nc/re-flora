import terrain_smooth;

[[vk::binding(0, 0)]]
ConstantBuffer<U_TerrainSmoothInfo> terrain_smooth_info;

[[vk::binding(1, 0)]]
GLSLShaderStorageBuffer<B_TerrainSmoothColumns, Std430DataLayout>
    terrain_smooth_columns;

float columnHeight(uint2 coordinate)
{
    return terrain_smooth_columns.data[
        terrainSmoothColumnIndex(terrain_smooth_info, coordinate)].x;
}

float columnValid(uint2 coordinate)
{
    return terrain_smooth_columns.data[
        terrainSmoothColumnIndex(terrain_smooth_info, coordinate)].z;
}

[shader("compute")]
[numthreads(8, 8, 1)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    uint2 column = dispatchThreadId.xy;
    if (column.x >= terrain_smooth_info.dim.x ||
        column.y >= terrain_smooth_info.dim.z)
    {
        return;
    }

    uint index = terrainSmoothColumnIndex(terrain_smooth_info, column);
    float4 entry = terrain_smooth_columns.data[index];
    if (entry.z <= 0.0)
    {
        terrain_smooth_columns.data[index].y = entry.x;
        return;
    }

    float2 atlasXZ = float2(
        terrain_smooth_info.offset.x + column.x,
        terrain_smooth_info.offset.z + column.y) + float2(0.5);
    float centerDistance = length(
        atlasXZ - terrain_smooth_info.center_xz_vox);
    if (centerDistance > terrain_smooth_info.brush_radius_vox)
    {
        terrain_smooth_columns.data[index].y = entry.x;
        return;
    }

    int kernelRadius = max(
        1, int(ceil(terrain_smooth_info.kernel_radius_vox)));
    float weightedSum = 0.0;
    float totalWeight = 0.0;
    float kernelSigma = max(
        terrain_smooth_info.kernel_radius_vox * 0.65, 1.0);

    for (int dx = -kernelRadius; dx <= kernelRadius; ++dx)
    {
        for (int dz = -kernelRadius; dz <= kernelRadius; ++dz)
        {
            int2 neighbour = int2(column) + int2(dx, dz);
            if (neighbour.x < 0 || neighbour.y < 0 ||
                neighbour.x >= int(terrain_smooth_info.dim.x) ||
                neighbour.y >= int(terrain_smooth_info.dim.z))
            {
                continue;
            }

            uint2 neighbourUnsigned = uint2(neighbour);
            if (columnValid(neighbourUnsigned) <= 0.0)
            {
                continue;
            }

            float2 delta = float2(dx, dz);
            float distanceFromColumn = length(delta);
            if (distanceFromColumn > terrain_smooth_info.kernel_radius_vox)
            {
                continue;
            }

            float weight = exp(
                -(distanceFromColumn * distanceFromColumn) /
                max(2.0 * kernelSigma * kernelSigma, 1.0));
            weightedSum += columnHeight(neighbourUnsigned) * weight;
            totalWeight += weight;
        }
    }

    if (totalWeight <= 0.0)
    {
        terrain_smooth_columns.data[index].y = entry.x;
        return;
    }

    float averageHeight = weightedSum / totalWeight;
    float brushFalloff = 1.0 - smoothstep(
        0.0, terrain_smooth_info.brush_radius_vox, centerDistance);
    float blend = clamp(
        terrain_smooth_info.strength * brushFalloff, 0.0, 1.0);
    float desired = lerp(entry.x, averageHeight, blend);
    float delta = clamp(
        desired - entry.x,
        -terrain_smooth_info.max_delta_vox,
        terrain_smooth_info.max_delta_vox);
    float target = entry.x + delta;
    if (abs(target - entry.x) <= terrain_smooth_info.deadband_vox)
    {
        target = entry.x;
    }

    terrain_smooth_columns.data[index].y = target;
}
