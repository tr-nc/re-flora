module marching_result;

public struct MarchingResult
{
    public uint iteration_count;
    public bool is_hit;
    public float3 position;
    public float3 center_position;
    public float distance;
    public float3 normal;
    public uint voxel_type;
    public uint voxel_address;
};
